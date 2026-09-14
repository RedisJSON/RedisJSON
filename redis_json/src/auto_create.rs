/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

//! Opt-in auto-creation of missing object levels along a write path
//!
//! Without it, `JSON.SET k $.a.b.c 5` errors when `k` is absent and returns a
//! silent `nil` when `k` exists but `$.a.b` does not.
//!
//! Creation flow and terminology (for an existing document):
//!
//! A "seeded command" modifies a value rather than assigning one: NUMINCRBY,
//! STRAPPEND, ARRAPPEND, or ARRINSERT at index 0. When its target is missing,
//! auto-creation supplies an initial value (the "seed") before the operation.
//! Existing targets keep their values; only newly created leaves get seeds.
//!
//! 1. Plan: `plan_creation` reads the document and returns `CreateSite`s, not
//!    JSON nodes. Each site describes an existing `parent`, missing object
//!    `levels`, and the final `leaf` key. Planning does not change the document.
//!    For seeded commands, `plan_seed_paths` also captures concrete targets
//!    before any writes, so creating nodes cannot change filter matches.
//! 2. Validate: before attaching anything, `materialize` checks the deepest
//!    planned leaf against the depth limit. Seeded commands also check operand
//!    types and the final depth of array items before writing their seeds.
//! 3. Build and attach: for each site, `nest_in_objects` builds the missing
//!    subtree as a standalone value, not yet attached to the key. `dict_add`
//!    then attaches it to the existing parent: this is where creation mutates
//!    the document. Sites are built and attached one at a time; this is not a
//!    copy of the whole document or a general rollback mechanism.
//!    This per-site walkthrough separates the payload from its attachment key.
//!    Execution prepares all complete objects, including that key, before any
//!    attachment: `$.a.b.c` builds `{"b":{"c":0}}` for existing parent `a`.
//! 4. Apply: SET/MERGE supply their value as the new leaf. Commands that operate
//!    on a value first need a `Seed`: `[]` for arrays, `0` for increment, or
//!    `""` for string append. After attachment, the command applies its operand
//!    to the captured targets. The seed's `items`, `increment`, or `suffix`
//!    field is that operand, carried along for validation, not the seed value.
//!
//! Example: incrementing `$.a.b.c` by 5 in `{"a":{}}` plans parent `["a"]`,
//! levels `["b"]`, leaf `"c"`; builds `{"c":0}` off-key; attaches it as `a.b`;
//! then increments the attached `c` to 5.
//!
//! Two-level example: `JSON.ARRAPPEND k $.users[*].settings.ui.tags "\"dark\""`
//! on `{"users":[{},{}]}` plans two sites, with parents `["users","0"]` and
//! `["users","1"]`. Each has levels `["settings","ui"]` and leaf `"tags"`.
//! After validation, each site builds `{"ui":{"tags":[]}}` off-key and
//! attaches it under its parent's `settings` key. The command then appends
//! `"dark"` at both captured targets, producing:
//! `{"users":[{"settings":{"ui":{"tags":["dark"]}}},{"settings":{"ui":{"tags":["dark"]}}}]}`.
//!
//! For an absent key, SET/MSET/MERGE instead use `root_key_chain` and
//! `nest_in_objects` to build the whole document before storing it.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use json_path::json_path::{JsonPathToken, Query, UserPathTracker};
use json_path::select_value::{SelectValue, SelectValueType, ValueRef, MAX_DEPTH};
use json_path::{calc_once_paths, calc_once_with_paths, compile};
use redis_module::{Context, RedisError, RedisResult, RedisValue};

use serde_json::Value;

use crate::manager::{err_invalid_path, err_json, err_projection_readonly, Manager, WriteHolder};
use crate::redisjson::Format;

/// Backing store for the `json-auto-create-deep-paths` module config,
/// registered in the `redis_module!` block (see `lib.rs`). The SDK writes here
/// through its `ConfigurationValue<bool> for AtomicBool` impl, which also uses
/// `Relaxed`.
pub static AUTO_CREATE_DEEP_PATHS: AtomicBool = AtomicBool::new(false);

/// Whether JSON write commands may create missing object levels along a path.
#[must_use]
pub fn auto_create_enabled() -> bool {
    AUTO_CREATE_DEEP_PATHS.load(AtomicOrdering::Relaxed)
}

/// One place where a write needs structure created before it can be applied.
///
/// `parent` is the concrete path of the deepest node that already exists, in
/// the `Vec<String>` form every [`WriteHolder`] method takes. `levels` are the
/// object keys to create as empty objects under it, outermost first, and `leaf`
/// is the final key the command's value (or seed) goes into.
///
/// For `JSON.SET k $.a.b.c 5` on `{"a":{}}`: `parent = ["a"]`,
/// `levels = ["b"]`, `leaf = "c"`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CreateSite {
    pub parent: Vec<String>,
    pub levels: Vec<String>,
    pub leaf: String,
}

/// Sites the write must create at `query`. Pure planning: the only error is a
/// bad path.
///
/// Peels the trailing run of plain object keys off `query`
/// ([`json_path::json_path::Query::pop_last_object_key`]) and evaluates the
/// remaining prefix. Each prefix match that can accept the peeled suffix yields
/// one [`CreateSite`]; matches that cannot are skipped silently, so they do not
/// affect the command's reply.
///
/// `create_intermediates` is what the `json-auto-create-deep-paths` config
/// buys. Without it this is held to what RedisJSON has always done -- a final
/// missing key under an already-existing parent, single-target paths only.
///
/// An empty result is not necessarily an error: see [`nothing_to_write`].
pub(crate) fn plan_creation<V: SelectValue>(
    query: Query,
    doc: &V,
    create_intermediates: bool,
) -> RedisResult<Vec<CreateSite>> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    if !create_intermediates && !query.clone().is_static() {
        return Ok(Vec::new());
    }
    let sites = plan_sites(query, doc);
    if create_intermediates {
        return Ok(sites);
    }
    Ok(sites
        .into_iter()
        .filter(|site| site.levels.is_empty())
        .collect())
}

/// The reply RedisJSON has always given for a write that matched nothing and
/// could create nothing: an error for a path that could never have worked,
/// otherwise `nil`.
pub(crate) fn nothing_to_write<V: SelectValue>(query: Query, doc: &V) -> RedisResult<RedisValue> {
    validate_legacy_creation_path(query, doc)?;
    Ok(RedisValue::Null)
}

/// Check legacy path restrictions before planning a final-key addition or
/// returning a no-write reply. This does not check whether a parent exists or
/// accepts object keys; `dict_add` validates the parent when applying the plan.
pub(crate) fn validate_legacy_creation_path<V: SelectValue>(
    mut query: Query,
    doc: &V,
) -> RedisResult<()> {
    if !query.is_static() {
        return Err(RedisError::Str("ERR wrong static path"));
    }
    if query.size() < 1 {
        return Err(RedisError::Str("ERR path must end with object key to set"));
    }
    // A trailing array index is never created, so either it is out of range or
    // an NX is no-oping over a value that is already there.
    if matches!(query.clone().pop_last(), Some((_, JsonPathToken::Number)))
        && calc_once_paths(query, doc).is_empty()
    {
        return Err(RedisError::Str("ERR array index out of range"));
    }
    Ok(())
}

/// Peel the trailing object keys and plan one site per prefix match. Pure:
/// no restrictions, no errors, empty when nothing is creatable.
fn plan_sites<V: SelectValue>(mut query: Query, doc: &V) -> Vec<CreateSite> {
    let mut suffix = Vec::new();
    while let Some(key) = query.pop_last_object_key() {
        suffix.push(key);
    }
    if suffix.is_empty() {
        return Vec::new();
    }
    suffix.reverse();

    calc_once_paths(query, doc)
        .into_iter()
        .filter_map(|prefix| plan_site(doc, prefix, &suffix))
        .collect()
}

/// Visit existing and creatable targets in query order using one prefix evaluation.
/// A missing value type marks a target that will be created. Blocked paths are
/// omitted; existing values retain their type, even if a command cannot use it.
pub(crate) fn plan_write_paths<V: SelectValue>(
    mut query: Query,
    root: &V,
    mut visit: impl FnMut(Vec<String>, Option<SelectValueType>),
) -> RedisResult<Vec<CreateSite>> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    let mut suffix = Vec::new();
    while let Some(key) = query.pop_last_object_key() {
        suffix.push(key);
    }
    if suffix.is_empty() {
        for matched in calc_once_with_paths(query, root) {
            visit(
                matched.path_tracker.unwrap().to_string_path(),
                Some(matched.res.get_type()),
            );
        }
        return Ok(Vec::new());
    }
    suffix.reverse();
    // Evaluate the prefix on the original document. Re-evaluating a filter
    // after seeding can lose matches or select unrelated new targets.
    let mut sites = Vec::new();
    for matched in calc_once_with_paths(query, root) {
        let prefix = matched.path_tracker.unwrap().to_string_path();
        match resolve_object_suffix(matched.res.as_ref(), prefix, &suffix) {
            Some(ObjectTarget::Existing(path, kind)) => visit(path, Some(kind)),
            Some(ObjectTarget::Missing(site)) => {
                let mut path = site.parent.clone();
                path.extend(site.levels.iter().cloned());
                path.push(site.leaf.clone());
                visit(path, None);
                sites.push(site);
            }
            None => {}
        }
    }
    Ok(sites)
}

/// The object-key chain a path addresses from the document root, or `None`
/// when the path is not a plain chain of object keys.
///
/// Used to build a whole new document in one write: there is no document to
/// walk yet, so anything with a wildcard, a filter or an array index cannot be
/// materialized from nothing.
pub(crate) fn root_key_chain(mut query: Query) -> RedisResult<Option<Vec<String>>> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    let mut keys = Vec::new();
    while let Some(key) = query.pop_last_object_key() {
        keys.push(key);
    }
    // Anything left over is a segment we cannot invent.
    if keys.is_empty() || query.size() > 0 {
        return Ok(None);
    }
    keys.reverse();
    Ok(Some(keys))
}

/// Plan the creation for a single prefix match, or `None` if this match cannot
/// take the suffix (it is not an object, it is blocked by a non-object part way
/// down, or the suffix is already fully present).
fn plan_site<V: SelectValue>(
    root: &V,
    prefix: Vec<String>,
    suffix: &[String],
) -> Option<CreateSite> {
    match resolve_object_suffix(node_at(root, &prefix)?, prefix, suffix)? {
        ObjectTarget::Missing(site) => Some(site),
        ObjectTarget::Existing(..) => None,
    }
}

enum ObjectTarget {
    Existing(Vec<String>, SelectValueType),
    Missing(CreateSite),
}

/// Walk the suffix once, stopping at its existing value or first missing key.
fn resolve_object_suffix<V: SelectValue>(
    mut node: &V,
    prefix: Vec<String>,
    suffix: &[String],
) -> Option<ObjectTarget> {
    let mut parent = prefix;

    for (i, key) in suffix.iter().enumerate() {
        if node.get_type() != SelectValueType::Object {
            return None;
        }
        match node.get_key(key) {
            // Already there: descend, nothing to create at this level.
            Some(ValueRef::Borrowed(child)) => {
                node = child;
                parent.push(key.clone());
            }
            // A synthesized value has no address in the document.
            Some(ValueRef::Owned(_)) => return None,
            None => {
                let (levels, leaf) = suffix[i..].split_at(suffix.len() - i - 1);
                return Some(ObjectTarget::Missing(CreateSite {
                    parent,
                    levels: levels.to_vec(),
                    leaf: leaf[0].clone(),
                }));
            }
        }
    }
    Some(ObjectTarget::Existing(parent, node.get_type()))
}

/// Follow a concrete path from `node` and return the document node it
/// addresses, or `None` if any step is missing or hits the wrong container
/// type.
fn node_at<'a, V: SelectValue>(node: &'a V, path: &[String]) -> Option<&'a V> {
    let Some((step, rest)) = path.split_first() else {
        return Some(node);
    };
    let child = match node.get_type() {
        SelectValueType::Object => node.get_key(step)?,
        SelectValueType::Array => node.get_index(step.parse::<usize>().ok()?)?,
        _ => return None,
    };
    match child {
        ValueRef::Borrowed(child) => node_at(child, rest),
        ValueRef::Owned(_) => None,
    }
}

/// Build a chain of single-key objects around `value`, outermost key first:
/// `["b", "c"]` with `5` gives `{"b":{"c":5}}`. Empty `keys` returns `value`.
///
/// Lets a deep path be created in a single write, so the depth limit is
/// checked before anything is mutated. Errors if the result would exceed
/// the nesting limit on its own -- the caller cannot check that, since
/// `Self::O` is opaque to it.
/// The borrowed value view supplies depth; both backends use the shared limit.
pub(crate) fn nest_in_objects<M: Manager>(
    manager: &M,
    keys: &[String],
    value: M::O,
) -> RedisResult<M::O> {
    check_depth::<M::V>(value.borrow(), keys.len())?;
    keys.iter().rev().try_fold(value, |inner, key| {
        manager.create_object(vec![(key.clone(), inner)])
    })
}

fn check_depth<V: SelectValue>(value: &V, parent_depth: usize) -> RedisResult<()> {
    if parent_depth.saturating_add(value.calculate_value_depth()) >= MAX_DEPTH {
        return Err(crate::manager::err_recursion_limit_exceeded());
    }
    Ok(())
}

/// Build overlapping object paths into one detached subtree, preserving every leaf.
fn build_creation_subtree<M: Manager>(
    manager: &M,
    paths: &[Vec<String>],
    leaf: &M::O,
) -> RedisResult<M::O> {
    fn build<M: Manager>(
        manager: &M,
        paths: &[&[String]],
        existing: Option<&M::V>,
        leaf: &M::O,
    ) -> RedisResult<M::O> {
        let replaces_value = paths.first().is_some_and(|path| path.is_empty());
        let existing = if replaces_value {
            Some(leaf.borrow())
        } else {
            existing
        };
        let mut branches: Vec<(&str, Vec<&[String]>)> = Vec::new();
        let mut indices = HashMap::new();
        for path in paths {
            if let Some((name, rest)) = path.split_first() {
                let index = *indices.entry(name.as_str()).or_insert_with(|| {
                    branches.push((name, Vec::new()));
                    branches.len() - 1
                });
                branches[index].1.push(rest);
            }
        }
        if branches.is_empty() {
            return if replaces_value {
                Ok(leaf.clone())
            } else {
                existing.map_or_else(
                    || manager.create_object(Vec::new()),
                    |value| Ok(manager.clone_value(value)),
                )
            };
        }
        let mut fields = Vec::new();
        if let Some(value) = existing {
            for (name, child) in value.items().ok_or_else(crate::manager::err_bad_object)? {
                let child = match indices.get(name) {
                    Some(&index) => build(manager, &branches[index].1, Some(child.as_ref()), leaf)?,
                    None => manager.clone_value(child.as_ref()),
                };
                fields.push((name.to_owned(), child));
            }
        }
        for (name, paths) in branches {
            if !existing.is_some_and(|value| value.contains_key(name)) {
                fields.push((name.to_owned(), build(manager, &paths, None, leaf)?));
            }
        }
        manager.create_object(fields)
    }

    let mut paths: Vec<_> = paths.iter().map(Vec::as_slice).collect();
    // Set ancestor leaves first, then add any descendants to those values.
    paths.sort_by_key(|path| path.len());
    build(manager, &paths, None, leaf)
}

/// Create every site's missing structure.
///
/// `leaf` is what ends up at the deepest key: the command's own value for
/// `JSON.SET`/`JSON.MERGE`, or a seed (`[]`, `0`, `""`) for the commands whose
/// operation needs something to act on.
///
/// One `dict_add` per site, always onto an already-existing parent, with the
/// whole missing chain as its value. That keeps the write atomic -- the depth
/// limit is checked before anything is mutated -- and keeps managers that
/// cannot traverse a missing path element working unchanged.
/// Overlapping sites share one attachment: their branches are combined off-key,
/// and every subtree is built successfully before the first attachment.
/// `prepare_creations` includes the outermost missing key in each detached
/// object. Its entries become owned `dict_add` arguments before attachment starts.
pub(crate) fn materialize<M: Manager>(
    manager: &M,
    key: &mut M::WriteHolder,
    sites: &[CreateSite],
    leaf: &M::O,
) -> CreationResult {
    materialize_with(manager, sites, leaf, |parent, name, value| {
        key.dict_add(parent, name, value)
    })
}

/// Attachment progress is separate from completion: a later write can fail.
pub(crate) struct CreationResult {
    pub any_created: bool,
    pub result: RedisResult<()>,
}

impl CreationResult {
    /// Finalize partial creation before returning an attachment error. On success,
    /// the command finalizes once its remaining operations have completed.
    pub fn apply_partial_changes_on_error<M: Manager>(
        self,
        manager: &M,
        key: &mut M::WriteHolder,
        ctx: &Context,
        command: &str,
    ) -> RedisResult<bool> {
        if self.any_created && self.result.is_err() {
            let notified = key.notify_keyspace_event(ctx, command);
            manager.apply_changes(ctx);
            notified?;
        }
        self.result.map(|()| self.any_created)
    }
}

fn materialize_with<M: Manager>(
    manager: &M,
    sites: &[CreateSite],
    leaf: &M::O,
    mut attach: impl FnMut(Vec<String>, &str, M::O) -> RedisResult<bool>,
) -> CreationResult {
    let mut any_created = false;
    let result = (|| {
        let prepared = prepare_creations(manager, sites, leaf)?;
        let mut additions = Vec::with_capacity(prepared.len());
        for (parent, object) in prepared {
            let mut fields = manager.take_object_fields(object)?;
            let (name, value) = fields.next().ok_or_else(crate::manager::err_bad_object)?;
            debug_assert!(fields.next().is_none());
            additions.push((parent, name, value));
        }
        for (parent, name, value) in additions {
            if !attach(parent, &name, value)? {
                return Err(err_invalid_path());
            }
            any_created = true;
        }
        Ok(())
    })();
    CreationResult {
        any_created,
        result,
    }
}

/// Build all missing branches as complete detached objects paired with their
/// existing parents. For `$.a.x.y = 5` with `a` present, return parent `["a"]`
/// and object `{"x":{"y":5}}`. No document writes occur in this phase.
/// Each object contains exactly one outermost missing key; overlapping paths
/// under that key are combined into its value.
fn prepare_creations<M: Manager>(
    manager: &M,
    sites: &[CreateSite],
    leaf: &M::O,
) -> RedisResult<Vec<(Vec<String>, M::O)>> {
    if let Some(deepest) = sites
        .iter()
        .map(|site| site.parent.len() + 1 + site.levels.len())
        .max()
    {
        check_depth::<M::V>(leaf.borrow(), deepest)?;
    }
    let mut groups: Vec<(Vec<String>, Vec<Vec<String>>)> = Vec::new();
    let mut group_indices = HashMap::new();
    for site in sites {
        let mut keys = site.levels.clone();
        keys.push(site.leaf.clone());
        // `keys[0]` is added to `parent`; the rest nest inside it.
        let index = *group_indices
            .entry((site.parent.clone(), keys[0].clone()))
            .or_insert_with(|| {
                groups.push((site.parent.clone(), Vec::new()));
                groups.len() - 1
            });
        groups[index].1.push(keys);
    }
    groups
        .into_iter()
        .map(|(parent, paths)| {
            let object = if paths.len() == 1 {
                nest_in_objects(manager, &paths[0], leaf.clone())?
            } else {
                build_creation_subtree(manager, &paths, leaf)?
            };
            Ok((parent, object))
        })
        .collect()
}

/// What a created leaf starts out as, for the commands that need something of
/// their own type to act on.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Seed<'a, O> {
    /// `JSON.ARRAPPEND`, `JSON.ARRINSERT`
    EmptyArray { items: &'a [O] },
    /// `JSON.NUMINCRBY` -- `0 + n == n`. `MULTBY`/`POWBY` get no seed at all:
    /// `0 * n` and `0 ^ n` would fabricate a wrong answer.
    Zero { increment: &'a str },
    /// `JSON.STRAPPEND` -- `"" + s == s`
    EmptyString { suffix: &'a str },
}

impl<O> Seed<'_, O> {
    fn accepts(&self, value_type: SelectValueType) -> bool {
        match self {
            Self::EmptyArray { .. } => value_type == SelectValueType::Array,
            Self::Zero { .. } => {
                matches!(value_type, SelectValueType::Long | SelectValueType::Double)
            }
            Self::EmptyString { .. } => value_type == SelectValueType::String,
        }
    }

    const fn as_json(&self) -> &'static str {
        match self {
            Self::EmptyArray { .. } => "[]",
            Self::Zero { .. } => "0",
            Self::EmptyString { .. } => "\"\"",
        }
    }

    /// `operand` is the JSON the command is about to apply, for the seeds whose
    /// type constrains it (see [`Seed::check_operand`]). `None` where the command
    /// has already parsed it.
    fn operand(&self) -> Option<&str> {
        match self {
            Self::EmptyArray { .. } => None,
            Self::Zero { increment } => Some(increment),
            Self::EmptyString { suffix } => Some(suffix),
        }
    }

    fn check_operand(&self) -> RedisResult<()> {
        let Some(json) = self.operand() else {
            return Ok(());
        };
        let value: Value = serde_json::from_str(json)?;
        match self {
            // A created leaf is `0`, so only a number can be added to it.
            Self::Zero { .. } if !matches!(value, Value::Number(_)) => {
                Err(RedisError::Str("bad input number"))
            }
            Self::EmptyString { .. } if !matches!(value, Value::String(_)) => {
                Err(err_json("string"))
            }
            _ => Ok(()),
        }
    }

    fn check_array_depth<V: SelectValue>(&self, paths: &[Option<Vec<String>>]) -> RedisResult<()>
    where
        O: Borrow<V>,
    {
        if let Self::EmptyArray { items } = self {
            if let Some(depth) = paths.iter().flatten().map(Vec::len).max() {
                for value in *items {
                    check_depth::<V>(value.borrow(), depth + 1)?;
                }
            }
        }
        Ok(())
    }
}

/// Create the missing object levels of `path` and leave `seed` at each new
/// leaf, so the command that follows needs no change of its own.
///
/// Three things are deliberately left alone:
///
/// - the config being off, in which case this does nothing at all;
/// - an absent key, since only `JSON.SET`/`JSON.MSET`/`JSON.MERGE` create a
///   document;
/// - a path that already resolves, so a match of the wrong type still gets the
///   command's own reply for it rather than being overwritten with a seed.
///
/// Returns concrete targets captured before mutation, including wrong-type
/// matches as `None`. Array operands are checked at their final depth before
/// seeding; `create` disables creation for MULTBY/POWBY and nonzero ARRINSERT.
/// The `create` guard lives in the command handlers.
pub(crate) fn seed_missing_paths<M: Manager>(
    manager: &M,
    key: &mut M::WriteHolder,
    ctx: &Context,
    command: &str,
    path: &str,
    seed: Seed<'_, M::O>,
) -> RedisResult<Vec<Option<Vec<String>>>> {
    let root = key.get_value()?.ok_or_else(RedisError::nonexistent_key)?;
    let (paths, sites) = plan_seed_paths(compile(path)?, root, &seed)?;
    if sites.is_empty() {
        return Ok(paths);
    }
    // Checked only here, where a seed is about to be written: a command that
    // creates nothing keeps whatever reply it has always given for an operand
    // it cannot use.
    seed.check_operand()?;
    seed.check_array_depth::<M::V>(&paths)?;
    // Parsed only once there is something to create; a seed cannot fail to
    // parse.
    let value = manager.from_str(seed.as_json(), Format::JSON, true, None)?;
    if let Seed::Zero { increment } = &seed {
        validate_increments(root, &paths, increment)?;
    }
    materialize::<M>(manager, key, &sites, &value)
        .apply_partial_changes_on_error(manager, key, ctx, command)?;
    Ok(paths)
}

fn validate_increments<V: SelectValue>(
    root: &V,
    paths: &[Option<Vec<String>>],
    increment: &str,
) -> RedisResult<()> {
    use crate::number::number_op_result;
    use ijson::IValue;

    let operand: Value = serde_json::from_str(increment)?;
    let zero = IValue::from(0);
    let mut values: HashMap<_, IValue> = HashMap::new();
    for path in paths.iter().flatten() {
        let result = if let Some(value) = values.get(path) {
            number_op_result(value, &operand, i128::checked_add, |a, b| a + b)
        } else if let Some(value) = node_at(root, path) {
            number_op_result(value, &operand, i128::checked_add, |a, b| a + b)
        } else {
            number_op_result(&zero, &operand, i128::checked_add, |a, b| a + b)
        }?;
        values.insert(path, result.into());
    }
    Ok(())
}

fn plan_seed_paths<V: SelectValue, O>(
    query: Query,
    root: &V,
    seed: &Seed<'_, O>,
) -> RedisResult<(Vec<Option<Vec<String>>>, Vec<CreateSite>)> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    if !auto_create_enabled() {
        let paths = calc_once_with_paths(query, root)
            .into_iter()
            .map(|matched| {
                seed.accepts(matched.res.get_type())
                    .then(|| matched.path_tracker.unwrap().to_string_path())
            })
            .collect();
        return Ok((paths, Vec::new()));
    }
    let mut paths = Vec::new();
    let sites = plan_write_paths(query, root, |path, value_type| {
        paths.push(
            value_type
                .is_none_or(|value_type| seed.accepts(value_type))
                .then_some(path),
        );
    })?;
    Ok((paths, sites))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ijson::IValue;
    use json_path::compile;

    fn doc(json: &str) -> IValue {
        serde_json::from_str(json).unwrap()
    }

    fn plan(path: &str, json: &str) -> Vec<CreateSite> {
        plan_creation(compile(path).unwrap(), &doc(json), true).unwrap()
    }

    /// The reply a command gives once nothing was matched and nothing planned.
    fn no_write(path: &str, json: &str) -> String {
        match nothing_to_write(compile(path).unwrap(), &doc(json)) {
            Ok(reply) => format!("{reply:?}"),
            Err(e) => e.to_string(),
        }
    }

    fn site(parent: &[&str], levels: &[&str], leaf: &str) -> CreateSite {
        CreateSite {
            parent: parent.iter().map(|s| (*s).to_string()).collect(),
            levels: levels.iter().map(|s| (*s).to_string()).collect(),
            leaf: leaf.to_string(),
        }
    }

    #[test]
    fn combined_write_planning_preserves_matches_and_creation_sites() {
        let root = doc(
            r#"{"a":{"n":1},"b":{},"c":{"n":"wrong"},"d":3,"arr":[1,2],"objects":[{},{"n":0}]}"#,
        );
        for path in [
            "$.*.n",
            "$['b','a','a','c','d'].n",
            "$..n",
            "$.arr[*]",
            "$.arr[0]",
            "$.objects[?(@.n==0)].x",
            "$.*.missing.x",
            "$.a.n",
            "$.unknown.a",
        ] {
            let query = compile(path).unwrap();
            let expected: Vec<_> = calc_once_with_paths(query.clone(), &root)
                .into_iter()
                .map(|matched| {
                    (
                        matched.path_tracker.unwrap().to_string_path(),
                        matched.res.get_type(),
                    )
                })
                .collect();
            let mut existing = Vec::new();
            let sites = plan_write_paths(query.clone(), &root, |path, value_type| {
                if let Some(value_type) = value_type {
                    existing.push((path, value_type));
                }
            })
            .unwrap();
            assert_eq!(existing, expected, "{path}");
            assert_eq!(sites, plan_creation(query, &root, true).unwrap(), "{path}");
        }
    }

    #[test]
    fn combined_write_planning_keeps_missing_and_duplicate_targets_in_order() {
        let root = doc(r#"{"a":{"n":1},"b":{},"c":{"n":"wrong"},"d":3}"#);
        let mut targets = Vec::new();
        plan_write_paths(
            compile("$['b','a','a','c','d'].n").unwrap(),
            &root,
            |path, kind| {
                targets.push((path, kind));
            },
        )
        .unwrap();
        assert_eq!(
            targets,
            vec![
                (vec!["b".to_owned(), "n".to_owned()], None),
                (
                    vec!["a".to_owned(), "n".to_owned()],
                    Some(SelectValueType::Long)
                ),
                (
                    vec!["a".to_owned(), "n".to_owned()],
                    Some(SelectValueType::Long)
                ),
                (
                    vec!["c".to_owned(), "n".to_owned()],
                    Some(SelectValueType::String)
                ),
            ]
        );
    }

    #[test]
    fn creation_reports_progress_when_second_attachment_fails() {
        let manager = crate::ivalue_manager::RedisIValueJsonKeyManager {
            phantom: std::marker::PhantomData,
        };
        let sites = [
            site(&[], &[], "a"),
            site(&[], &[], "b"),
            site(&[], &[], "c"),
        ];
        for fail_with_error in [false, true] {
            let mut attached = Vec::new();
            let mut attempts = 0;
            let creation = materialize_with(&manager, &sites, &doc("5"), |_, name, value| {
                attempts += 1;
                if attempts == 2 {
                    return if fail_with_error {
                        Err(err_invalid_path())
                    } else {
                        Ok(false)
                    };
                }
                attached.push((name.to_owned(), value));
                Ok(true)
            });
            assert_eq!(attached, vec![("a".to_owned(), doc("5"))]);
            assert_eq!(attempts, 2);
            assert!(creation.result.is_err());
            assert!(
                creation.any_created,
                "the first attachment must still count"
            );
        }
    }

    #[test]
    fn creation_build_failure_prevents_every_attachment() {
        let manager = crate::ivalue_manager::RedisIValueJsonKeyManager {
            phantom: std::marker::PhantomData,
        };
        let sites = [
            site(&["a"], &["x"], "y"),
            site(&["b"], &[], "x"),
            site(&["b"], &["x"], "y"),
        ];
        let creation = materialize_with(&manager, &sites, &doc("5"), |_, _, _| {
            panic!("nothing may attach before every subtree is built")
        });
        assert!(!creation.any_created);
        assert_eq!(
            creation.result.unwrap_err().to_string(),
            crate::manager::err_bad_object().to_string()
        );
    }

    #[test]
    fn prepared_creations_include_outermost_missing_keys() {
        let manager = crate::ivalue_manager::RedisIValueJsonKeyManager {
            phantom: std::marker::PhantomData,
        };
        let root = doc(r#"{"a":{"keep":1},"b":{}}"#);
        let before = root.clone();
        let sites = plan_creation(compile("$.*.x.y").unwrap(), &root, true).unwrap();
        let prepared = prepare_creations(&manager, &sites, &doc("5")).unwrap();
        assert_eq!(
            prepared,
            vec![
                (vec!["a".into()], doc(r#"{"x":{"y":5}}"#)),
                (vec!["b".into()], doc(r#"{"x":{"y":5}}"#)),
            ]
        );
        assert_eq!(root, before);
    }

    #[test]
    fn increment_validation_checks_duplicate_existing_and_missing_targets() {
        // This fixture allows one increment on the existing number, and two on a
        // new zero. One more increment overflows in either case.
        let root = doc(r#"{"n":5000000000000000000}"#);
        let before = root.clone();
        let increment = "4000000000000000000";
        let existing = Some(vec!["n".into()]);
        let missing = Some(vec!["new".into()]);
        let paths = [existing.clone(), None, missing.clone(), missing.clone()];
        validate_increments(&root, &paths, increment).unwrap();
        for extra in [existing, missing] {
            let mut paths = paths.to_vec();
            paths.push(extra);
            let error = validate_increments(&root, &paths, increment).unwrap_err();
            assert_eq!(
                error.to_string(),
                crate::manager::err_numeric_overflow().to_string()
            );
        }
        assert_eq!(root, before);
    }

    #[test]
    fn overlapping_subtrees_preserve_object_leaves() {
        let manager = crate::ivalue_manager::RedisIValueJsonKeyManager {
            phantom: std::marker::PhantomData,
        };
        let leaf = doc(r#"{"keep":null,"a":{"old":1}}"#);
        let paths = vec![
            vec!["a".into(), "b".into()],
            vec![],
            vec!["a".into(), "b".into()],
            vec!["c\"d".into()],
        ];
        let built = build_creation_subtree(&manager, &paths, &leaf).unwrap();
        assert_eq!(
            built,
            doc(
                r#"{"keep":null,"a":{"old":1,"b":{"keep":null,"a":{"old":1}}},"c\"d":{"keep":null,"a":{"old":1}}}"#
            )
        );
        assert_eq!(leaf, doc(r#"{"keep":null,"a":{"old":1}}"#));

        let scalar = doc("5");
        let error = build_creation_subtree(&manager, &paths, &scalar).unwrap_err();
        assert_eq!(
            error.to_string(),
            crate::manager::err_bad_object().to_string()
        );
    }

    #[test]
    fn disabled_dynamic_creation_does_not_read_the_document() {
        #[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
        struct Unreadable;

        impl SelectValue for Unreadable {
            fn get_type(&self) -> SelectValueType {
                panic!("document was traversed")
            }
            fn contains_key(&self, _: &str) -> bool {
                unreachable!()
            }
            fn values(&self) -> Option<Box<dyn Iterator<Item = ValueRef<'_, Self>> + '_>> {
                unreachable!()
            }
            fn keys(&self) -> Option<Box<dyn Iterator<Item = &str> + '_>> {
                unreachable!()
            }
            fn items(&self) -> Option<Box<dyn Iterator<Item = (&str, ValueRef<'_, Self>)> + '_>> {
                unreachable!()
            }
            fn len(&self) -> Option<usize> {
                unreachable!()
            }
            fn is_empty(&self) -> Option<bool> {
                unreachable!()
            }
            fn get_key(&self, _: &str) -> Option<ValueRef<'_, Self>> {
                unreachable!()
            }
            fn get_index(&self, _: usize) -> Option<ValueRef<'_, Self>> {
                unreachable!()
            }
            fn is_array(&self) -> bool {
                unreachable!()
            }
            fn is_double(&self) -> Option<bool> {
                unreachable!()
            }
            fn get_str(&self) -> Option<String> {
                unreachable!()
            }
            fn as_str(&self) -> Option<&str> {
                unreachable!()
            }
            fn get_bool(&self) -> Option<bool> {
                unreachable!()
            }
            fn get_long(&self) -> Option<i64> {
                unreachable!()
            }
            fn get_double(&self) -> Option<f64> {
                unreachable!()
            }
            fn get_array(&self) -> *const std::ffi::c_void {
                unreachable!()
            }
            fn get_array_type(&self) -> Option<json_path::select_value::JSONArrayType> {
                unreachable!()
            }
        }

        assert!(
            plan_creation(compile("$..missing.a.b").unwrap(), &Unreadable, false)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn plans_nothing_when_the_whole_path_exists() {
        assert!(plan("$.a.b.c", r#"{"a":{"b":{"c":1}}}"#).is_empty());
    }

    #[test]
    fn plans_a_single_missing_leaf() {
        assert_eq!(plan("$.a.b", r#"{"a":{}}"#), vec![site(&["a"], &[], "b")]);
    }

    #[test]
    fn plans_several_missing_levels() {
        assert_eq!(
            plan("$.a.b.c.d", r#"{"a":{}}"#),
            vec![site(&["a"], &["b", "c"], "d")]
        );
    }

    #[test]
    fn plans_from_the_root_of_an_empty_document() {
        assert_eq!(plan("$.a.b.c", "{}"), vec![site(&[], &["a", "b"], "c")]);
    }

    #[test]
    fn skips_a_match_blocked_by_a_scalar() {
        assert!(plan("$.a.b.c", r#"{"a":3}"#).is_empty());
        assert!(plan("$.a.b.c", r#"{"a":{"b":"str"}}"#).is_empty());
    }

    #[test]
    fn skips_a_match_blocked_by_an_array() {
        assert!(plan("$.a.b", r#"{"a":[1,2]}"#).is_empty());
    }

    #[test]
    fn never_creates_a_trailing_array_index() {
        // The MUST: array elements are never invented, even with intermediates
        // allowed -- so nothing is planned and the historical replies stand.
        for (path, json) in [
            ("$.a.b[0]", r#"{"a":{"b":[]}}"#),
            ("$.a[0]", "{}"),
            ("$..[0]", r#"{"a":[]}"#),
        ] {
            assert!(plan(path, json).is_empty(), "path {path}");
        }
        assert_eq!(
            no_write("$.a.b[0]", r#"{"a":{"b":[]}}"#),
            "ERR array index out of range"
        );
        assert_eq!(no_write("$.a[0]", "{}"), "ERR array index out of range");
        // `$..[0]` is not static, so it is refused before the index matters.
        assert_eq!(no_write("$..[0]", r#"{"a":[]}"#), "ERR wrong static path");
    }

    #[test]
    fn no_ops_when_a_trailing_array_index_already_matches() {
        assert!(plan("$.a[0]", r#"{"a":[1]}"#).is_empty());
    }

    #[test]
    fn plans_nothing_when_an_array_index_blocks_the_prefix() {
        // `$.a[0].b` would need `a[0]` invented, which the prefix refuses.
        assert!(plan("$.a[0].b", "{}").is_empty());
    }

    #[test]
    fn plans_through_an_existing_array_index() {
        assert_eq!(
            plan("$.a[1].b.c", r#"{"a":[{},{}]}"#),
            vec![site(&["a", "1"], &["b"], "c")]
        );
    }

    #[test]
    fn plans_only_the_missing_targets_of_a_union() {
        // `b` already has `c`; only `a` needs it. The union itself is never
        // created -- a missing `b` would simply not match.
        assert_eq!(
            plan("$['a','b'].c", r#"{"a":{},"b":{"c":1}}"#),
            vec![site(&["a"], &[], "c")]
        );
        assert_eq!(
            plan("$['a','b'].c", r#"{"a":{},"b":{}}"#),
            vec![site(&["a"], &[], "c"), site(&["b"], &[], "c")]
        );
        // Neither `a` nor `b` exists, and a union is not static, so nothing is
        // creatable and the historical error stands.
        assert_eq!(no_write("$['a','b'].c", "{}"), "ERR wrong static path");
    }

    #[test]
    fn plans_for_each_element_of_a_slice() {
        assert_eq!(
            plan("$.a[0:2].b", r#"{"a":[{},{},{}]}"#),
            vec![site(&["a", "0"], &[], "b"), site(&["a", "1"], &[], "b")]
        );
    }

    #[test]
    fn plans_for_every_object_under_a_wildcard() {
        assert_eq!(
            plan("$.*.n", r#"{"a":{},"b":{},"s":"str"}"#),
            vec![site(&["a"], &[], "n"), site(&["b"], &[], "n")]
        );
    }

    #[test]
    fn plans_for_every_object_under_a_descendant_wildcard() {
        // Non-object matches are skipped rather than erroring.
        let sites = plan("$..*.n", r#"{"a":{"b":{}},"x":[{}],"s":"str"}"#);
        assert_eq!(
            sites,
            vec![
                site(&["a"], &[], "n"),
                site(&["a", "b"], &[], "n"),
                site(&["x", "0"], &[], "n"),
            ]
        );
    }

    #[test]
    fn plans_for_the_root_too_under_a_descendant() {
        // `$..k` peels to a bare descendant prefix, which matches the root as
        // well as every node below it.
        let sites = plan("$..k", r#"{"a":{}}"#);
        assert_eq!(sites, vec![site(&[], &[], "k"), site(&["a"], &[], "k")]);
    }

    #[test]
    fn without_create_intermediates_only_a_final_key_is_created() {
        let leaf_only = |path, json| plan_creation(compile(path).unwrap(), &doc(json), false);
        // One missing key under an existing parent: allowed, as it always was.
        assert_eq!(
            leaf_only("$.a.b", r#"{"a":{}}"#).unwrap(),
            vec![site(&["a"], &[], "b")]
        );
        // A whole missing chain: not created, and nothing matched -> nil.
        assert!(leaf_only("$.a.b.c", r#"{"a":{}}"#).unwrap().is_empty());
        // Multi-target paths stay refused.
        // Multi-target paths plan nothing, and the reply helper reports why.
        assert!(leaf_only("$.*.n", r#"{"p":{},"q":{}}"#).unwrap().is_empty());
        assert_eq!(
            no_write("$.*.n", r#"{"p":{},"q":{}}"#),
            "ERR wrong static path"
        );
    }

    #[test]
    fn rejects_a_projection_path() {
        let err = plan_creation(compile("$.a + 1").unwrap(), &doc(r#"{"a":1}"#), true).unwrap_err();
        assert!(format!("{err}").contains("projection"), "{err}");
    }

    #[test]
    fn rejects_an_uncompilable_path_before_planning() {
        assert!(compile("$.[").is_err());
    }

    #[test]
    fn plans_with_escaped_and_bracketed_keys() {
        assert_eq!(
            plan(r#"$["a b"]["c.d"]"#, r#"{"a b":{}}"#),
            vec![site(&["a b"], &[], "c.d")]
        );
        assert_eq!(
            plan(r#"$["a"]["\\"]"#, r#"{"a":{}}"#),
            vec![site(&["a"], &[], "\\")]
        );
    }
}
