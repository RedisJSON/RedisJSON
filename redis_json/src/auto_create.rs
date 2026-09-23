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
//! 1. Resolve: `prepare_paths` evaluates selectors on the original document,
//!    then walks the trailing object keys through borrowed nodes. It records
//!    existing targets and missing chains without changing the document.
//!    Seeded commands capture their targets here, before any writes, so
//!    creating nodes cannot change filter matches or duplicate replies.
//! 2. Validate and build: discard chains below values that SET or a non-object
//!    MERGE will replace, check depth, then combine overlapping chains. Every
//!    branch is built in detached storage before attachment starts. Seeded
//!    commands also check operand types, numeric results, and final array depth.
//! 3. Attach: each `Addition` owns a concrete `parent` path, its first missing
//!    `key`, and the complete subtree to insert as its `value`. `dict_add`
//!    attaches that value to the existing parent; this is where creation
//!    mutates the document. No borrowed node survives into this phase.
//!    The outermost key stays separate from its payload, matching `dict_add`'s
//!    arguments. All additions are ready before the first write; this is not
//!    a copy of the whole document or a general rollback mechanism. A later
//!    attachment failure can leave partial changes, which must be replicated.
//! 4. Apply: SET/MERGE supply their value as the new leaf. Commands that operate
//!    on a value first need a `Seed`: `[]` for arrays, `0` for increment, or
//!    `""` for string append. After attachment, the command applies its operand
//!    to the captured targets. The seed's `items`, `increment`, or `suffix`
//!    field is that operand, carried along for validation, not the seed value.
//!
//! Example: `JSON.SET k $.a.b.c 5` on `{"a":{}}` prepares parent `["a"]`,
//! key `"b"`, and the complete value `{"c":5}` off-key. One attachment creates
//! both `a.b` and `a.b.c`, producing `{"a":{"b":{"c":5}}}`; no later write
//! to the new leaf is needed.
//!
//! Example: incrementing `$.a.b.c` by 5 in `{"a":{}}` prepares parent `["a"]`,
//! key `"b"`, value `{"c":0}` off-key; attaches it as `a.b`; then increments
//! the attached `c` to 5.
//!
//! Two-level example: `JSON.ARRAPPEND k $.users[*].settings.ui.tags "\"dark\""`
//! on `{"users":[{},{}]}` prepares two additions, with parents `["users","0"]`
//! and `["users","1"]`. Each has key `"settings"` and value `{"ui":{"tags":[]}}`.
//! Both subtrees are built and validated off-key, then attached under their
//! parents' `settings` keys. The command then appends `"dark"` at both captured
//! targets, producing:
//! `{"users":[{"settings":{"ui":{"tags":["dark"]}}},{"settings":{"ui":{"tags":["dark"]}}}]}`.
//!
//! For an absent key, SET/MSET/MERGE instead use `root_key_chain` and
//! `nest_in_objects` to build the whole document before storing it.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use json_path::json_path::{CalculationResult, JsonPathToken, PTracker, Query, UserPathTracker};
use json_path::select_value::{SelectValue, SelectValueType, ValueRef, MAX_DEPTH};
use json_path::{calc_once_paths, calc_once_with_paths, compile, visit_once_with_paths};
use redis_module::{Context, RedisError, RedisResult, RedisValue};
use serde_json::Value;

use crate::commands::prepare_paths_for_updating;
use crate::key_value::KeyValue;
use crate::manager::{
    err_invalid_path, err_json, err_projection_readonly, err_recursion_limit_exceeded, Manager,
    SetUpdateInfo, UpdateInfo, WriteHolder,
};
use crate::redisjson::{Format, SetOptions};

/// Backing store for the `json-auto-create-deep-paths` module config,
/// registered in the `redis_module!` block (see `lib.rs`). The SDK writes here
/// through its `ConfigurationValue<bool> for AtomicBool` impl, which also uses
/// `Relaxed`.
pub static AUTO_CREATE_DEEP_PATHS: AtomicBool = AtomicBool::new(false);

/// Whether JSON write commands may create missing object levels along a path.
#[must_use]
pub fn auto_create_enabled() -> bool {
    AUTO_CREATE_DEEP_PATHS.load(Ordering::Relaxed)
}

/// Concrete path segments: object keys or decimal array indices, without selectors.
type ConcretePath = Vec<String>;
/// A slice of path segments; a creation suffix contains only object keys.
type PathSlice = [String];

/// A command target; `None` means its value does not exist yet.
struct WriteTarget {
    path: ConcretePath,
    value_type: Option<SelectValueType>,
}

/// Where suffix traversal stopped, and the remaining object keys to create.
struct SuffixTarget<'a> {
    parent: ConcretePath,
    missing: &'a PathSlice,
    value_type: Option<SelectValueType>,
}

struct PendingCreation {
    prefix_depth: usize,
    parent: ConcretePath,
    missing_len: usize,
}

/// Paths sharing one attachment, relative to its first missing key.
struct CreationGroup<'a> {
    parent: ConcretePath,
    key: &'a str,
    paths: Vec<&'a PathSlice>,
}

#[derive(Default)]
struct ResolvedPaths {
    targets: Vec<WriteTarget>,
    pending: Vec<PendingCreation>,
}

/// One place where a write needs structure created before it can be applied.
///
/// `parent` is the concrete path of the deepest node that already exists, in
/// the `Vec<String>` form every [`WriteHolder`] method takes. `key` is its first
/// missing object field; `value` owns the complete chain below that field.
/// For `JSON.SET k $.a.b.c 5` on `{"a":{}}`: `parent = ["a"]`,
/// `key = "b"`, `value = {"c":5}`.
///
/// Paths remain valid across sibling insertions; retaining raw pointers into
/// the stored document would not provide that guarantee.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct Addition<O> {
    parent: ConcretePath,
    key: String,
    value: O,
}

// MSET preserves existing-target updates even if preparing new branches fails.
pub(crate) type PreparedAdditions<O> = RedisResult<Vec<Addition<O>>>;

/// Prepare SET writes without mutating the document: existing-target updates
/// and detached additions remain separate, preserving MSET's partial-success
/// replies when branch construction fails.
///
/// With auto-creation enabled, allow missing object chains alongside updates
/// to existing targets; both are captured by one prefix evaluation. Otherwise
/// preserve SET's original behavior: add only a final key to an existing parent.
/// An empty result is not necessarily an error: see [`nothing_to_write`].
pub(crate) fn prepare_set<M: Manager>(
    manager: &M,
    doc: &M::V,
    query: Query,
    value: &M::O,
    option: SetOptions,
) -> RedisResult<(Vec<UpdateInfo>, PreparedAdditions<M::O>)> {
    if !auto_create_enabled() || option == SetOptions::AlreadyExists {
        // Disabled mode skips missing intermediate objects, including over-deep chains.
        return Ok((
            KeyValue::new(doc).find_paths(query, option)?,
            Ok(Vec::new()),
        ));
    }
    // Non-object patches replace their targets, discarding anything below them.
    let replace = option != SetOptions::NotExists
        && (option != SetOptions::MergeExisting
            || value.borrow().get_type() != SelectValueType::Object);
    let mut updates = Vec::new();
    let additions = prepare_paths(manager, query, doc, value, replace, |target| {
        if target.value_type.is_some() && option != SetOptions::NotExists {
            updates.push(target.path);
        }
    })?;
    if option != SetOptions::MergeExisting {
        prepare_paths_for_updating(&mut updates);
    }
    Ok((
        updates
            .into_iter()
            .map(|path| UpdateInfo::SUI(SetUpdateInfo { path }))
            .collect(),
        additions,
    ))
}

/// Keep dynamic selectors in the prefix; only plain object fields can be invented.
fn object_suffix(query: &mut Query) -> Vec<String> {
    let mut keys = Vec::new();
    while let Some(key) = query.pop_last_object_key() {
        keys.push(key);
    }
    keys.reverse();
    keys
}

/// Walk the suffix once, stopping at its existing value or first missing key.
/// Blocked matches (a non-object at the prefix or part way down) are skipped.
fn walk_suffix<'a, V: SelectValue>(
    node: &V,
    keys: &'a PathSlice,
    mut path: ConcretePath,
) -> Option<SuffixTarget<'a>> {
    let Some((key, rest)) = keys.split_first() else {
        return Some(SuffixTarget {
            parent: path,
            missing: &[],
            value_type: Some(node.get_type()),
        });
    };
    if node.get_type() != SelectValueType::Object {
        return None;
    }
    match node.get_key(key) {
        // Already there: descend, nothing to create at this level.
        Some(ValueRef::Borrowed(child)) => {
            path.push(key.clone());
            walk_suffix(child, rest, path)
        }
        // A synthesized value has no address in the document.
        Some(ValueRef::Owned(_)) => None,
        None => Some(SuffixTarget {
            parent: path,
            missing: keys,
            value_type: None,
        }),
    }
}

/// Visit existing and creatable targets in query order using one prefix evaluation.
/// A missing value type marks a target that will be created. Blocked paths are
/// omitted; existing values retain their type, even if a command cannot use it.
/// An impossibly deep suffix errors before visiting the first creatable target.
///
/// Peel the trailing run of plain object keys off `query`
/// ([`json_path::json_path::Query::pop_last_object_key`]) and evaluate the
/// remaining prefix. Then build all missing branches as detached additions
/// paired with their existing parents. For `$.a.x.y = 5` with `a` present,
/// return parent `["a"]`, key `"x"`, and value `{"y":5}`. Overlapping paths
/// under that key are combined into its value. No document writes occur here.
///
/// `leaf` is the command's own value for SET/MERGE, or a seed (`[]`, `0`, `""`)
/// for commands whose operation needs something to act on. Preparation visits
/// ancestors first to combine overlaps and discard additions below replacements.
fn prepare_paths<M: Manager>(
    manager: &M,
    mut query: Query,
    root: &M::V,
    leaf: &M::O,
    replace: bool,
    mut visit: impl FnMut(WriteTarget),
) -> RedisResult<PreparedAdditions<M::O>> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    let suffix = object_suffix(&mut query);
    let ResolvedPaths { targets, pending } = resolve_paths(query, root, &suffix)?;
    let additions = group_creations(pending, &targets, &suffix, leaf.borrow(), replace)
        .and_then(|groups| build_additions(manager, groups, leaf));
    // MSET still needs existing targets when preparing additions fails.
    for target in targets {
        visit(target);
    }
    Ok(additions)
}

fn resolve_paths<V: SelectValue>(
    query: Query,
    root: &V,
    suffix: &PathSlice,
) -> RedisResult<ResolvedPaths> {
    let mut paths = ResolvedPaths::default();
    let mut resolution = Ok(());
    // Evaluate the prefix on the original document. Re-evaluating a filter
    // after seeding can lose matches or select unrelated new targets.
    visit_once_with_paths(query, root, |matched| {
        if resolution.is_ok() {
            resolution = paths.resolve_match(matched, suffix);
        }
    });
    resolution?;
    Ok(paths)
}

impl ResolvedPaths {
    fn resolve_match<V: SelectValue>(
        &mut self,
        matched: CalculationResult<'_, V, PTracker>,
        suffix: &PathSlice,
    ) -> RedisResult<()> {
        let prefix = matched.path_tracker.unwrap().to_string_path();
        let prefix_depth = prefix.len();
        if let Some(target) = walk_suffix(matched.res.as_ref(), suffix, prefix) {
            self.record_target(target, prefix_depth, suffix.len())?;
        }
        Ok(())
    }

    /// The visitor receives the existing path and any remaining keys to create.
    fn record_target(
        &mut self,
        target: SuffixTarget<'_>,
        prefix_depth: usize,
        suffix_len: usize,
    ) -> RedisResult<()> {
        let SuffixTarget {
            parent,
            missing,
            value_type,
        } = target;
        // Resolve the prefix before checking depth so blocked or unmatched paths
        // keep their replies. MSET handles the depth error explicitly as nil.
        // No target with this suffix can fit. Reject before exposing a
        // target or allocating the remaining creation sites.
        if value_type.is_none() && suffix_len >= MAX_DEPTH {
            return Err(err_recursion_limit_exceeded());
        }
        if value_type.is_none() {
            self.pending.push(PendingCreation {
                prefix_depth,
                parent: parent.clone(),
                missing_len: missing.len(),
            });
        }
        let mut path = parent;
        path.extend_from_slice(missing);
        self.targets.push(WriteTarget { path, value_type });
        Ok(())
    }
}

/// Discard replaced descendants, validate depth, and group chains by attachment.
fn group_creations<'a, V: SelectValue>(
    mut pending: Vec<PendingCreation>,
    targets: &[WriteTarget],
    suffix: &'a PathSlice,
    leaf: &V,
    replace: bool,
) -> RedisResult<Vec<CreationGroup<'a>>> {
    // Selectors can yield descendants before ancestors. Prepare missing branches
    // in prefix-depth order after all replacement targets are known; otherwise
    // a discarded descendant could cause a false error.
    pending.sort_by_key(|creation| creation.prefix_depth);
    let replacements: HashSet<_> = if replace && !pending.is_empty() {
        targets
            .iter()
            .filter_map(|target| {
                target
                    .value_type
                    .is_some()
                    .then_some(target.path.as_slice())
            })
            .collect()
    } else {
        HashSet::new()
    };
    let mut groups = Vec::new();
    let mut indices = HashMap::new();
    let mut leaf_depth = None;
    for PendingCreation {
        parent,
        missing_len,
        ..
    } in pending
    {
        // Replacing an existing ancestor discards every creation below it.
        if (0..=parent.len()).any(|len| replacements.contains(&parent[..len])) {
            continue;
        }
        let missing = &suffix[suffix.len() - missing_len..];
        let depth = *leaf_depth.get_or_insert_with(|| leaf.calculate_value_depth());
        // Check every creation before building: depth failures take precedence
        // over an incompatible overlap discovered during construction.
        if parent
            .len()
            .saturating_add(missing_len)
            .saturating_add(depth)
            >= MAX_DEPTH
        {
            return Err(err_recursion_limit_exceeded());
        }
        // `missing[0]` is added to `parent`; the rest nest inside it.
        let key = missing[0].as_str();
        let index = *indices.entry((parent.clone(), key)).or_insert_with(|| {
            groups.push(CreationGroup {
                parent,
                key,
                paths: Vec::new(),
            });
            groups.len() - 1
        });
        groups[index].paths.push(&missing[1..]);
    }
    Ok(groups)
}

/// Build every grouped subtree off-key before any attachment can start.
fn build_additions<M: Manager>(
    manager: &M,
    groups: Vec<CreationGroup<'_>>,
    leaf: &M::O,
) -> PreparedAdditions<M::O> {
    groups
        .into_iter()
        .map(|mut group| {
            // Set ancestor leaves first, then add any descendants to those values.
            group.paths.sort_by_key(|path| path.len());
            group.paths.dedup();
            let value = if group.paths.len() == 1 {
                build_branch(manager, group.paths[0], leaf.clone())?
            } else {
                build_creation_subtree(manager, &group.paths, None, leaf)?
            };
            Ok(Addition {
                parent: group.parent,
                key: group.key.to_owned(),
                value,
            })
        })
        .collect()
}

/// Construct a single chain on unwind, touching only detached values.
fn build_branch<M: Manager>(manager: &M, keys: &PathSlice, leaf: M::O) -> RedisResult<M::O> {
    let Some((key, rest)) = keys.split_first() else {
        return Ok(leaf);
    };
    let child = build_branch(manager, rest, leaf)?;
    manager.create_object(vec![(key.clone(), child)])
}

/// Build overlapping object paths into one detached subtree, preserving every leaf.
/// Paths are ordered ancestor-first; unrelated fields in object leaves survive.
fn build_creation_subtree<M: Manager>(
    manager: &M,
    paths: &[&PathSlice],
    existing: Option<&M::V>,
    leaf: &M::O,
) -> RedisResult<M::O> {
    let replaces_value = paths.first().is_some_and(|path| path.is_empty());
    let existing = if replaces_value {
        Some(leaf.borrow())
    } else {
        existing
    };
    let mut branches: Vec<(&str, Vec<&PathSlice>)> = Vec::new();
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
                Some(&index) => {
                    build_creation_subtree(manager, &branches[index].1, Some(child.as_ref()), leaf)?
                }
                None => manager.clone_value(child.as_ref()),
            };
            fields.push((name.to_owned(), child));
        }
    }
    for (name, paths) in branches {
        if !existing.is_some_and(|value| value.contains_key(name)) {
            fields.push((
                name.to_owned(),
                build_creation_subtree(manager, &paths, None, leaf)?,
            ));
        }
    }
    manager.create_object(fields)
}

/// MSET validates against the initial document without constructing branches;
/// later triplets resolve again after earlier writes on the same key.
pub(crate) fn can_create<V: SelectValue>(mut query: Query, root: &V) -> RedisResult<bool> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    let suffix = object_suffix(&mut query);
    let mut creatable = false;
    visit_once_with_paths(query, root, |matched| {
        creatable = creatable || has_missing_suffix(matched.res.as_ref(), &suffix);
    });
    Ok(creatable)
}

fn has_missing_suffix<V: SelectValue>(node: &V, suffix: &PathSlice) -> bool {
    walk_suffix(node, suffix, Vec::new()).is_some_and(|target| target.value_type.is_none())
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
    let keys = object_suffix(&mut query);
    // Anything left over is a segment we cannot invent.
    Ok((!keys.is_empty() && query.size() == 0).then_some(keys))
}

/// Build a chain of single-key objects around `value`, outermost key first:
/// `["b", "c"]` with `5` gives `{"b":{"c":5}}`. Empty `keys` returns `value`.
///
/// Lets a deep path be created in a single write, so the depth limit is
/// checked before anything is mutated. Errors if the result would exceed
/// the nesting limit on its own -- the caller cannot check that, since
/// `M::O` is opaque to it.
/// The borrowed value view supplies depth; both backends use the shared limit.
pub(crate) fn nest_in_objects<M: Manager>(
    manager: &M,
    keys: &[String],
    value: M::O,
) -> RedisResult<M::O> {
    check_depth::<M::V>(value.borrow(), keys.len())?;
    build_branch(manager, keys, value)
}

fn check_depth<V: SelectValue>(value: &V, parent_depth: usize) -> RedisResult<()> {
    if parent_depth.saturating_add(value.calculate_value_depth()) >= MAX_DEPTH {
        return Err(err_recursion_limit_exceeded());
    }
    Ok(())
}

/// Attach every prepared branch with one `dict_add`, always onto an existing
/// parent, with the whole missing chain as its value. Depth is checked before
/// mutation, and managers never need to traverse a missing path element.
/// Overlapping sites share one attachment: their branches are combined off-key,
/// and every subtree is built successfully before the first attachment.
/// The outermost missing key and its value are already owned arguments;
/// no detached wrapper needs to be unpacked during attachment.
pub(crate) fn attach_creations<M: Manager>(
    key: &mut M::WriteHolder,
    additions: PreparedAdditions<M::O>,
) -> CreationResult {
    attach_with(additions, |addition| {
        key.dict_add(addition.parent, &addition.key, addition.value)
    })
}

fn attach_with<O>(
    additions: PreparedAdditions<O>,
    mut attach: impl FnMut(Addition<O>) -> RedisResult<bool>,
) -> CreationResult {
    let mut any_created = false;
    let result = (|| {
        for addition in additions? {
            if !attach(addition)? {
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
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
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
/// - the config being off, in which case this creates nothing;
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
    let query = compile(path)?;
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    if !auto_create_enabled() {
        return Ok(calc_once_with_paths(query, root)
            .into_iter()
            .map(|matched| {
                seed.accepts(matched.res.get_type())
                    // SAFETY: we know that the path tracker is not None
                    .then(|| matched.path_tracker.unwrap().to_string_path())
            })
            .collect());
    }
    // Parse the seed once for detached preparation; a seed cannot fail to parse.
    let value = manager.from_str(seed.as_json(), Format::JSON, true, None)?;
    let mut paths = Vec::new();
    let additions = prepare_paths(manager, query, root, &value, false, |target| {
        paths.push(
            target
                .value_type
                .is_none_or(|value_type| seed.accepts(value_type))
                .then_some(target.path),
        );
    })?;
    if additions.as_ref().is_ok_and(Vec::is_empty) {
        return Ok(paths);
    }
    // Checked only here, where a seed is about to be written: a command that
    // creates nothing keeps whatever reply it has always given for an operand
    // it cannot use.
    seed.check_operand()?;
    seed.check_array_depth::<M::V>(&paths)?;
    if let Seed::Zero { increment } = &seed {
        validate_increments(root, &paths, increment)?;
    }
    attach_creations::<M>(key, additions)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ivalue_manager::RedisIValueJsonKeyManager;
    use ijson::IValue;

    fn doc(json: &str) -> IValue {
        serde_json::from_str(json).unwrap()
    }

    fn manager() -> RedisIValueJsonKeyManager<'static> {
        RedisIValueJsonKeyManager {
            phantom: std::marker::PhantomData,
        }
    }

    fn prepare(path: &str, root: &IValue, leaf: &IValue) -> RedisResult<Vec<Addition<IValue>>> {
        prepare_paths(&manager(), compile(path)?, root, leaf, false, |_| {})?
    }

    fn prepare_missing(path: &str, json: &str) -> Vec<Addition<IValue>> {
        prepare(path, &doc(json), &doc("5")).unwrap()
    }

    /// The reply a command gives once nothing was matched and nothing planned.
    fn no_write(path: &str, json: &str) -> String {
        match nothing_to_write(compile(path).unwrap(), &doc(json)) {
            Ok(reply) => format!("{reply:?}"),
            Err(e) => e.to_string(),
        }
    }

    fn addition(parent: &[&str], key: &str, json: &str) -> Addition<IValue> {
        Addition {
            parent: parent.iter().map(|s| (*s).to_string()).collect(),
            key: key.to_owned(),
            value: doc(json),
        }
    }

    #[test]
    fn impossible_suffix_does_not_expand_every_creation_site() {
        let mut root = ijson::IObject::new();
        for i in 0..64 {
            root.insert(format!("p{i}"), doc("{}")).unwrap();
        }
        let root: IValue = root.into();
        for depth in [MAX_DEPTH - 2, MAX_DEPTH, 1500] {
            let suffix = (0..depth)
                .map(|i| format!("x{i}"))
                .collect::<Vec<_>>()
                .join(".");
            for prefix in ["$.*", "$..*"] {
                let path = format!("{prefix}.{suffix}");
                let mut targets = Vec::new();
                let additions = prepare_paths(
                    &manager(),
                    compile(&path).unwrap(),
                    &root,
                    &doc("5"),
                    false,
                    |target| {
                        assert!(target.value_type.is_none());
                        targets.push(target.path);
                    },
                );
                if depth >= MAX_DEPTH {
                    assert_eq!(
                        additions.unwrap_err().to_string(),
                        err_recursion_limit_exceeded().to_string()
                    );
                    assert!(
                        targets.is_empty(),
                        "invalid preparation must not expose a target"
                    );
                } else {
                    assert_eq!(additions.unwrap().unwrap().len(), 64);
                    assert_eq!(targets.len(), 64);
                }
                let blocked = doc(r#"{"a":{"x0":1},"b":3}"#);
                assert!(prepare_paths(
                    &manager(),
                    compile(&path).unwrap(),
                    &blocked,
                    &doc("5"),
                    false,
                    |_| panic!("blocked paths must not produce targets"),
                )
                .unwrap()
                .unwrap()
                .is_empty());
            }
            let static_path = format!("$.{suffix}");
            assert!(KeyValue::new(&root)
                .find_paths(compile(&static_path).unwrap(), SetOptions::None)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn set_nx_rejects_projections_in_both_creation_modes() {
        let root = doc(r#"{"a":1}"#);
        let query = compile("$.a + 1").unwrap();
        let legacy = KeyValue::new(&root)
            .find_paths(query, SetOptions::NotExists)
            .err()
            .unwrap();
        let enabled = prepare("$.a + 1", &root, &doc("5")).unwrap_err();
        for error in [legacy, enabled] {
            assert_eq!(error.to_string(), err_projection_readonly().to_string());
        }
    }

    #[test]
    fn combined_write_preparation_preserves_existing_matches() {
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
            prepare_paths(&manager(), query, &root, &doc("5"), false, |target| {
                if let Some(value_type) = target.value_type {
                    existing.push((target.path, value_type));
                }
            })
            .unwrap()
            .unwrap();
            assert_eq!(existing, expected, "{path}");
        }
    }

    #[test]
    fn combined_write_preparation_keeps_missing_and_duplicate_targets_in_order() {
        let root = doc(r#"{"a":{"n":1},"b":{},"c":{"n":"wrong"},"d":3}"#);
        let mut targets = Vec::new();
        prepare_paths(
            &manager(),
            compile("$['b','a','a','c','d'].n").unwrap(),
            &root,
            &doc("5"),
            false,
            |target| {
                targets.push((target.path, target.value_type));
            },
        )
        .unwrap()
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
        for fail_with_error in [false, true] {
            let additions = prepare("$.*.x", &doc(r#"{"a":{},"b":{},"c":{}}"#), &doc("5"));
            let mut attached = Vec::new();
            let mut attempts = 0;
            let creation = attach_with(additions, |addition| {
                attempts += 1;
                if attempts == 2 {
                    return if fail_with_error {
                        Err(err_invalid_path())
                    } else {
                        Ok(false)
                    };
                }
                attached.push((addition.parent, addition.key, addition.value));
                Ok(true)
            });
            assert_eq!(
                attached,
                vec![(vec!["a".to_owned()], "x".to_owned(), doc("5"))]
            );
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
        let additions = prepare("$..x.x", &doc(r#"{"a":{},"b":{"x":{}}}"#), &doc("5"));
        let creation = attach_with(additions, |_| {
            panic!("nothing may attach before every subtree is built")
        });
        assert!(!creation.any_created);
        assert_eq!(
            creation.result.unwrap_err().to_string(),
            crate::manager::err_bad_object().to_string()
        );
    }

    #[test]
    fn prepared_additions_keep_outermost_missing_keys_separate() {
        let root = doc(r#"{"a":{"keep":1},"b":{}}"#);
        let before = root.clone();
        let prepared = prepare("$.*.x.y", &root, &doc("5")).unwrap();
        assert_eq!(
            prepared,
            vec![
                addition(&["a"], "x", r#"{"y":5}"#),
                addition(&["b"], "x", r#"{"y":5}"#),
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
        let paths = [
            vec!["a".into(), "b".into()],
            vec![],
            vec!["a".into(), "b".into()],
            vec!["c\"d".into()],
        ];
        let mut paths: Vec<_> = paths.iter().map(Vec::as_slice).collect();
        paths.sort_by_key(|path| path.len());
        let built = build_creation_subtree(&manager, &paths, None, &leaf).unwrap();
        assert_eq!(
            built,
            doc(
                r#"{"keep":null,"a":{"old":1,"b":{"keep":null,"a":{"old":1}}},"c\"d":{"keep":null,"a":{"old":1}}}"#
            )
        );
        assert_eq!(leaf, doc(r#"{"keep":null,"a":{"old":1}}"#));

        let scalar = doc("5");
        let error = build_creation_subtree(&manager, &paths, None, &scalar).unwrap_err();
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

        let error = KeyValue::new(&Unreadable)
            .find_paths(compile("$..missing.a.b").unwrap(), SetOptions::NotExists)
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "ERR wrong static path");
    }

    #[test]
    fn prepares_nothing_when_the_whole_path_exists() {
        assert!(prepare_missing("$.a.b.c", r#"{"a":{"b":{"c":1}}}"#).is_empty());
    }

    #[test]
    fn prepares_a_single_missing_leaf() {
        assert_eq!(
            prepare_missing("$.a.b", r#"{"a":{}}"#),
            vec![addition(&["a"], "b", "5")]
        );
    }

    #[test]
    fn prepares_several_missing_levels() {
        assert_eq!(
            prepare_missing("$.a.b.c.d", r#"{"a":{}}"#),
            vec![addition(&["a"], "b", r#"{"c":{"d":5}}"#)]
        );
    }

    #[test]
    fn prepares_from_the_root_of_an_empty_document() {
        assert_eq!(
            prepare_missing("$.a.b.c", "{}"),
            vec![addition(&[], "a", r#"{"b":{"c":5}}"#)]
        );
    }

    #[test]
    fn skips_a_match_blocked_by_a_scalar() {
        assert!(prepare_missing("$.a.b.c", r#"{"a":3}"#).is_empty());
        assert!(prepare_missing("$.a.b.c", r#"{"a":{"b":"str"}}"#).is_empty());
    }

    #[test]
    fn skips_a_match_blocked_by_an_array() {
        assert!(prepare_missing("$.a.b", r#"{"a":[1,2]}"#).is_empty());
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
            assert!(prepare_missing(path, json).is_empty(), "path {path}");
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
        assert!(prepare_missing("$.a[0]", r#"{"a":[1]}"#).is_empty());
    }

    #[test]
    fn prepares_nothing_when_an_array_index_blocks_the_prefix() {
        // `$.a[0].b` would need `a[0]` invented, which the prefix refuses.
        assert!(prepare_missing("$.a[0].b", "{}").is_empty());
    }

    #[test]
    fn prepares_through_an_existing_array_index() {
        assert_eq!(
            prepare_missing("$.a[1].b.c", r#"{"a":[{},{}]}"#),
            vec![addition(&["a", "1"], "b", r#"{"c":5}"#)]
        );
    }

    #[test]
    fn prepares_only_the_missing_targets_of_a_union() {
        // `b` already has `c`; only `a` needs it. The union itself is never
        // created -- a missing `b` would simply not match.
        assert_eq!(
            prepare_missing("$['a','b'].c", r#"{"a":{},"b":{"c":1}}"#),
            vec![addition(&["a"], "c", "5")]
        );
        assert_eq!(
            prepare_missing("$['a','b'].c", r#"{"a":{},"b":{}}"#),
            vec![addition(&["a"], "c", "5"), addition(&["b"], "c", "5")]
        );
        // Neither `a` nor `b` exists, and a union is not static, so nothing is
        // creatable and the historical error stands.
        assert_eq!(no_write("$['a','b'].c", "{}"), "ERR wrong static path");
    }

    #[test]
    fn prepares_for_each_element_of_a_slice() {
        assert_eq!(
            prepare_missing("$.a[0:2].b", r#"{"a":[{},{},{}]}"#),
            vec![
                addition(&["a", "0"], "b", "5"),
                addition(&["a", "1"], "b", "5")
            ]
        );
    }

    #[test]
    fn prepares_for_every_object_under_a_wildcard() {
        assert_eq!(
            prepare_missing("$.*.n", r#"{"a":{},"b":{},"s":"str"}"#),
            vec![addition(&["a"], "n", "5"), addition(&["b"], "n", "5")]
        );
    }

    #[test]
    fn prepares_for_every_object_under_a_descendant_wildcard() {
        // Non-object matches are skipped rather than erroring.
        let sites = prepare_missing("$..*.n", r#"{"a":{"b":{}},"x":[{}],"s":"str"}"#);
        assert_eq!(
            sites,
            vec![
                addition(&["a"], "n", "5"),
                addition(&["a", "b"], "n", "5"),
                addition(&["x", "0"], "n", "5"),
            ]
        );
    }

    #[test]
    fn prepares_for_the_root_too_under_a_descendant() {
        // `$..k` peels to a bare descendant prefix, which matches the root as
        // well as every node below it.
        let sites = prepare_missing("$..k", r#"{"a":{}}"#);
        assert_eq!(
            sites,
            vec![addition(&[], "k", "5"), addition(&["a"], "k", "5")]
        );
    }

    #[test]
    fn without_auto_create_only_a_final_key_is_created() {
        let root = doc(r#"{"a":{}}"#);
        // One missing key under an existing parent: allowed, as it always was.
        let updates = KeyValue::new(&root)
            .find_paths(compile("$.a.b").unwrap(), SetOptions::None)
            .unwrap();
        assert!(
            matches!(updates.as_slice(), [UpdateInfo::AUI(update)] if update.path == ["a"] && update.key == "b")
        );
        // A whole missing chain: not created, and nothing matched -> nil.
        assert!(KeyValue::new(&root)
            .find_paths(compile("$.a.b.c").unwrap(), SetOptions::None)
            .unwrap()
            .is_empty());
        // Multi-target paths stay refused.
        // Multi-target paths plan nothing, and the reply helper reports why.
        let root = doc(r#"{"p":{},"q":{}}"#);
        let error = KeyValue::new(&root)
            .find_paths(compile("$.*.n").unwrap(), SetOptions::None)
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "ERR wrong static path");
        assert_eq!(
            no_write("$.*.n", r#"{"p":{},"q":{}}"#),
            "ERR wrong static path"
        );
    }

    #[test]
    fn rejects_a_projection_path() {
        let err = prepare("$.a + 1", &doc(r#"{"a":1}"#), &doc("5")).unwrap_err();
        assert!(format!("{err}").contains("projection"), "{err}");
    }

    #[test]
    fn rejects_an_uncompilable_path_before_planning() {
        assert!(compile("$.[").is_err());
    }

    #[test]
    fn prepares_with_escaped_and_bracketed_keys() {
        assert_eq!(
            prepare_missing(r#"$["a b"]["c.d"]"#, r#"{"a b":{}}"#),
            vec![addition(&["a b"], "c.d", "5")]
        );
        assert_eq!(
            prepare_missing(r#"$["a"]["\\"]"#, r#"{"a":{}}"#),
            vec![addition(&["a"], "\\", "5")]
        );
    }

    #[test]
    fn streaming_preparation_preserves_many_existing_and_missing_targets() {
        let root: IValue = serde_json::from_value(serde_json::Value::Array(
            (0..1000)
                .map(|i| {
                    if i % 2 == 0 {
                        serde_json::json!({"age": i})
                    } else {
                        serde_json::json!({})
                    }
                })
                .collect(),
        ))
        .unwrap();
        let before = root.clone();
        let mut targets = Vec::new();
        let additions = prepare_paths(
            &manager(),
            compile("$[*].age").unwrap(),
            &root,
            &doc("42"),
            true,
            |target| targets.push((target.path, target.value_type.is_some())),
        )
        .unwrap()
        .unwrap();
        assert_eq!(targets.len(), 1000);
        assert_eq!(additions.len(), 500);
        for (i, (path, exists)) in targets.iter().enumerate() {
            assert_eq!(path, &[i.to_string(), "age".into()]);
            assert_eq!(*exists, i % 2 == 0);
        }
        assert_eq!(root, before);
    }

    #[test]
    fn dynamic_paths_preserve_existing_matches_and_reply_order() {
        let root = doc(r#"{"a":{"n":1},"b":{},"c":{"n":"wrong"},"d":3,"objects":[{},{"n":0}]}"#);
        for path in [
            "$.*.n",
            "$['b','a','a','c','d'].n",
            "$..n",
            "$.objects[?(@.n==0)].x",
            "$.*.missing.x",
        ] {
            let mut existing = Vec::new();
            prepare_paths(
                &manager(),
                compile(path).unwrap(),
                &root,
                &doc("5"),
                false,
                |target| {
                    if let Some(value_type) = target.value_type {
                        existing.push((target.path, value_type));
                    }
                },
            )
            .unwrap()
            .unwrap();
            let expected: Vec<_> = calc_once_with_paths(compile(path).unwrap(), &root)
                .into_iter()
                .map(|m| (m.path_tracker.unwrap().to_string_path(), m.res.get_type()))
                .collect();
            assert_eq!(existing, expected, "{path}");
        }
        let mut paths = Vec::new();
        let additions = prepare_paths(
            &manager(),
            compile("$['b','a','a','c','d','b'].n").unwrap(),
            &root,
            &doc("5"),
            false,
            |target| paths.push((target.path, target.value_type)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            paths.iter().map(|(p, _)| p[0].as_str()).collect::<Vec<_>>(),
            vec!["b", "a", "a", "c", "b"]
        );
        assert!(paths[0].1.is_none());
        assert!(paths[4].1.is_none());
        assert_eq!(additions.len(), 1, "duplicate targets share one attachment");
    }

    #[test]
    fn overlapping_branches_are_combined_before_attachment() {
        let root = doc(r#"{"a":{}}"#);
        let additions = prepare("$..a.a.b", &root, &doc("5")).unwrap();
        assert_eq!(additions.len(), 1);
        assert_eq!(additions[0].parent, ["a"]);
        assert_eq!(additions[0].key, "a");
        assert_eq!(additions[0].value, doc(r#"{"b":5,"a":{"b":5}}"#));
        for leaf in [r#"{"a":1,"keep":2}"#, r#"{"a":{"old":1},"keep":2}"#] {
            let additions = prepare("$..a.a", &root, &doc(leaf)).unwrap();
            assert_eq!(
                additions[0].value,
                doc(&format!(r#"{{"a":{leaf},"keep":2}}"#))
            );
        }
        assert_eq!(root, doc(r#"{"a":{}}"#));
    }

    #[test]
    fn replacement_discards_descendant_creation_before_building() {
        let mut updates = Vec::new();
        let additions = prepare_paths(
            &manager(),
            compile("$..a.a").unwrap(),
            &doc(r#"{"a":{"a":{}}}"#),
            &doc("5"),
            true,
            |target| {
                if target.value_type.is_some() {
                    updates.push(target.path);
                }
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(updates, vec![vec!["a", "a"]]);
        assert!(additions.is_empty());
    }

    #[test]
    fn incompatible_overlap_prevents_every_attachment() {
        let additions = prepare("$..a.a", &doc(r#"{"a":{}}"#), &doc("5"));
        let result = attach_with(additions, |_| {
            panic!("no writes before all branches are ready")
        });
        assert!(!result.any_created);
        assert_eq!(
            result.result.unwrap_err().to_string(),
            crate::manager::err_bad_object().to_string()
        );
    }

    #[test]
    fn deep_later_target_prevents_shallow_attachment() {
        let mut deep = serde_json::json!({});
        for _ in 0..70 {
            deep = serde_json::json!({"child": deep});
        }
        let root = doc(&serde_json::json!({"shallow": {}, "deep": deep}).to_string());
        let before = root.clone();
        let path = format!(
            "$..*.{}",
            (0..60)
                .map(|i| format!("x{i}"))
                .collect::<Vec<_>>()
                .join(".")
        );
        let result = attach_with(prepare(&path, &root, &doc("5")), |_| {
            panic!("must not attach shallow branch")
        });
        assert!(!result.any_created);
        assert_eq!(
            result.result.unwrap_err().to_string(),
            err_recursion_limit_exceeded().to_string()
        );
        assert_eq!(root, before);
    }

    #[test]
    fn depth_failure_takes_precedence_over_an_earlier_overlap_error() {
        let mut deep = serde_json::json!({});
        for _ in 0..125 {
            deep = serde_json::json!({"child": deep});
        }
        let root = doc(&serde_json::json!({"a": {}, "deep": deep}).to_string());
        let result = attach_with(prepare("$..a.a", &root, &doc("5")), |_| {
            panic!("neither the overlapping nor deep branches may attach")
        });
        assert!(!result.any_created);
        assert_eq!(
            result.result.unwrap_err().to_string(),
            err_recursion_limit_exceeded().to_string()
        );
    }

    #[test]
    fn validation_preserves_projection_and_array_errors() {
        assert!(prepare("$.a + 1", &doc(r#"{"a":1}"#), &doc("5")).is_err());
        assert!(root_key_chain(compile("$.a[0].b").unwrap())
            .unwrap()
            .is_none());
        assert_eq!(
            root_key_chain(compile("$.a.b").unwrap()).unwrap(),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(
            nothing_to_write(compile("$.a[0]").unwrap(), &doc("{}"))
                .unwrap_err()
                .to_string(),
            "ERR array index out of range"
        );
        assert!(can_create(compile("$.*.n").unwrap(), &doc(r#"{"a":{}}"#)).unwrap());
        assert!(!can_create(compile("$.a[0].n").unwrap(), &doc("{}")).unwrap());
    }
}
