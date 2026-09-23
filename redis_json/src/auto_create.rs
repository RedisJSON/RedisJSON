/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

//! Recursively prepare missing object branches without changing stored documents.
//! JSONPath selectors run on the original document. The recursive walk validates
//! existing parents and builds missing objects on return, collecting owned values
//! and stable parent paths. Only after preparation succeeds are branches attached.
//! Attachment failures can still leave partial changes, which must be replicated.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use json_path::json_path::{JsonPathToken, Query, UserPathTracker};
use json_path::select_value::{SelectValue, SelectValueType, ValueRef, MAX_DEPTH};
use json_path::{calc_once_paths, calc_once_with_paths, compile};
use redis_module::{Context, RedisError, RedisResult, RedisValue};
use serde_json::Value;

use crate::commands::prepare_paths_for_updating;
use crate::key_value::KeyValue;
use crate::manager::{
    err_invalid_path, err_json, err_projection_readonly, err_recursion_limit_exceeded, Manager,
    SetUpdateInfo, UpdateInfo, WriteHolder,
};
use crate::redisjson::{Format, SetOptions};

/// Load-time configuration; this flag does not synchronize any other data.
pub static AUTO_CREATE_DEEP_PATHS: AtomicBool = AtomicBool::new(false);

#[must_use]
pub fn auto_create_enabled() -> bool {
    AUTO_CREATE_DEEP_PATHS.load(Ordering::Relaxed)
}

/// An owned branch ready to attach. Paths remain valid across sibling insertions;
/// retaining raw pointers into the stored document would not provide that guarantee.
pub(crate) struct Addition<O> {
    parent: Vec<String>,
    key: String,
    value: O,
}

// MSET preserves existing-target updates even if preparing new branches fails.
pub(crate) type PreparedAdditions<O> = RedisResult<Vec<Addition<O>>>;

/// Separate updates from prepared additions so MSET can preserve its existing
/// partial-success replies when branch construction fails.
pub(crate) fn prepare_set<M: Manager>(
    manager: &M,
    doc: &M::V,
    query: Query,
    value: &M::O,
    option: SetOptions,
) -> RedisResult<(Vec<UpdateInfo>, PreparedAdditions<M::O>)> {
    if !auto_create_enabled() || option == SetOptions::AlreadyExists {
        return Ok((
            KeyValue::new(doc).find_paths(query, option)?,
            Ok(Vec::new()),
        ));
    }
    let replace = option != SetOptions::NotExists
        && (option != SetOptions::MergeExisting
            || value.borrow().get_type() != SelectValueType::Object);
    let mut updates = Vec::new();
    let additions = prepare_paths(manager, query, doc, value, replace, |path, kind| {
        if kind.is_some() && option != SetOptions::NotExists {
            updates.push(path);
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

/// Descend through existing objects. At the first missing field, the visitor
/// builds the remaining branch; blocked paths never reach it.
fn walk_suffix<V: SelectValue>(
    node: &V,
    keys: &[String],
    path: &mut Vec<String>,
    visit: &mut impl FnMut(&[String], &[String], Option<SelectValueType>) -> RedisResult<()>,
) -> RedisResult<()> {
    let Some((key, rest)) = keys.split_first() else {
        return visit(path, &[], Some(node.get_type()));
    };
    if node.get_type() != SelectValueType::Object {
        return Ok(());
    }
    match node.get_key(key) {
        Some(ValueRef::Borrowed(child)) => {
            path.push(key.clone());
            let result = walk_suffix(child, rest, path, visit);
            path.pop();
            result
        }
        Some(ValueRef::Owned(_)) => Ok(()), // Synthesized values have no stored address.
        None => visit(path, keys, None),
    }
}

/// Resolve targets once and recursively build missing branches. Target replies
/// retain query order; preparation visits ancestors first to combine overlaps and
/// discard additions below values that SET (or scalar MERGE) will replace.
fn prepare_paths<M: Manager>(
    manager: &M,
    mut query: Query,
    root: &M::V,
    leaf: &M::O,
    replace: bool,
    mut visit: impl FnMut(Vec<String>, Option<SelectValueType>),
) -> RedisResult<PreparedAdditions<M::O>> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    let suffix = object_suffix(&mut query);
    let mut matches: Vec<_> = calc_once_with_paths(query, root)
        .into_iter()
        .enumerate()
        .map(|(order, matched)| {
            (
                order,
                matched.path_tracker.unwrap().to_string_path(),
                matched.res,
            )
        })
        .collect();
    matches.sort_by_key(|(_, path, _)| path.len());
    let mut targets = Vec::with_capacity(matches.len());
    let mut additions: Vec<Addition<M::O>> = Vec::new();
    let mut indices = HashMap::new();
    let mut replacements = HashSet::new();
    let mut created_targets = HashSet::new();
    let mut failure = None;
    let mut leaf_depth = None;
    for (order, mut prefix, node) in matches {
        walk_suffix(
            node.as_ref(),
            &suffix,
            &mut prefix,
            &mut |parent, missing, kind| {
                if kind.is_none() && suffix.len() >= MAX_DEPTH {
                    return Err(err_recursion_limit_exceeded());
                }
                let mut target = parent.to_vec();
                target.extend_from_slice(missing);
                targets.push((order, target.clone(), kind));
                if kind.is_some() {
                    if replace {
                        replacements.insert(target);
                    }
                    return Ok(());
                }
                if (0..=parent.len()).any(|len| replacements.contains(&parent[..len]))
                    || !created_targets.insert(target.clone())
                {
                    return Ok(());
                }
                let depth =
                    *leaf_depth.get_or_insert_with(|| leaf.borrow().calculate_value_depth());
                // Check every creation even after a build error: depth failures
                // take precedence, as they did when all sites were validated first.
                if target.len().saturating_add(depth) >= MAX_DEPTH {
                    failure = Some(err_recursion_limit_exceeded());
                    return Ok(());
                }
                if failure.is_some() {
                    return Ok(());
                }
                let result = (|| {
                    let key = &missing[0];
                    let group = (parent.to_vec(), key.clone());
                    if let Some(&index) = indices.get(&group) {
                        let addition: &mut Addition<M::O> = &mut additions[index];
                        addition.value = build_branch(
                            manager,
                            Some(addition.value.clone()),
                            &missing[1..],
                            leaf,
                        )?;
                    } else {
                        let value = build_branch(manager, None, &missing[1..], leaf)?;
                        indices.insert(group, additions.len());
                        additions.push(Addition {
                            parent: parent.to_vec(),
                            key: key.clone(),
                            value,
                        });
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    failure = Some(error);
                }
                Ok(())
            },
        )?;
    }
    targets.sort_by_key(|(order, _, _)| *order);
    for (_, path, kind) in targets {
        visit(path, kind);
    }
    Ok(match failure {
        Some(error) => Err(error),
        None => Ok(additions),
    })
}

/// Construct on unwind, touching only detached values. Overlapping branches are
/// applied ancestor-first, preserving fields outside the requested path.
fn build_branch<M: Manager>(
    manager: &M,
    existing: Option<M::O>,
    keys: &[String],
    leaf: &M::O,
) -> RedisResult<M::O> {
    let Some((key, rest)) = keys.split_first() else {
        return Ok(leaf.clone());
    };
    let mut fields: Vec<_> = match existing {
        Some(value) => manager.take_object_fields(value)?.collect(),
        None => Vec::new(),
    };
    // ponytail: overlapping branches scan object fields; use indexed fields if profiling warrants it.
    if let Some(index) = fields.iter().position(|(name, _)| name == key) {
        let child = build_branch(manager, Some(fields[index].1.clone()), rest, leaf)?;
        fields[index].1 = child;
    } else {
        let child = build_branch(manager, None, rest, leaf)?;
        fields.push((key.clone(), child));
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
    let mut found = false;
    for matched in calc_once_with_paths(query, root) {
        walk_suffix(
            matched.res.as_ref(),
            &suffix,
            &mut Vec::new(),
            &mut |_, _, kind| {
                found |= kind.is_none();
                Ok(())
            },
        )?;
        if found {
            break;
        }
    }
    Ok(found)
}

pub(crate) fn root_key_chain(mut query: Query) -> RedisResult<Option<Vec<String>>> {
    if query.is_projection() {
        return Err(err_projection_readonly());
    }
    let keys = object_suffix(&mut query);
    Ok((!keys.is_empty() && query.size() == 0).then_some(keys))
}

pub(crate) fn nest_in_objects<M: Manager>(
    manager: &M,
    keys: &[String],
    value: M::O,
) -> RedisResult<M::O> {
    check_depth::<M::V>(value.borrow(), keys.len())?;
    build_branch(manager, None, keys, &value)
}

fn check_depth<V: SelectValue>(value: &V, parent_depth: usize) -> RedisResult<()> {
    if parent_depth.saturating_add(value.calculate_value_depth()) >= MAX_DEPTH {
        return Err(err_recursion_limit_exceeded());
    }
    Ok(())
}

/// No attachment starts unless every branch was prepared successfully.
pub(crate) fn attach_creations<M: Manager>(
    key: &mut M::WriteHolder,
    additions: RedisResult<Vec<Addition<M::O>>>,
) -> CreationResult {
    attach_with(additions, |parent, name, value| {
        key.dict_add(parent, name, value)
    })
}

fn attach_with<O>(
    additions: RedisResult<Vec<Addition<O>>>,
    mut attach: impl FnMut(Vec<String>, &str, O) -> RedisResult<bool>,
) -> CreationResult {
    let mut any_created = false;
    let result = (|| {
        for addition in additions? {
            if !attach(addition.parent, &addition.key, addition.value)? {
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

/// Capture targets before seeding so filters and duplicate replies stay stable.
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
                    .then(|| matched.path_tracker.unwrap().to_string_path())
            })
            .collect());
    }
    let value = manager.from_str(seed.as_json(), Format::JSON, true, None)?;
    let mut paths = Vec::new();
    let additions = prepare_paths(manager, query, root, &value, false, |path, kind| {
        paths.push(kind.is_none_or(|kind| seed.accepts(kind)).then_some(path));
    })?;
    if additions.as_ref().is_ok_and(Vec::is_empty) {
        return Ok(paths);
    }
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
        prepare_paths(&manager(), compile(path)?, root, leaf, false, |_, _| {})?
    }

    #[test]
    fn recursive_creation_preserves_existing_parents_and_builds_on_unwind() {
        for (path, json, parent, key, branch) in [
            (
                "$.a.b.c.d",
                r#"{"a":{"b":{}}}"#,
                vec!["a", "b"],
                "c",
                r#"{"d":"value"}"#,
            ),
            ("$.a.b", r#"{"a":{}}"#, vec!["a"], "b", r#""value""#),
            ("$.a.b.c", "{}", vec![], "a", r#"{"b":{"c":"value"}}"#),
            (
                "$.a[1].b.c",
                r#"{"a":[{},{}]}"#,
                vec!["a", "1"],
                "b",
                r#"{"c":"value"}"#,
            ),
            (
                r#"$["a b"]["c.d"]"#,
                r#"{"a b":{}}"#,
                vec!["a b"],
                "c.d",
                r#""value""#,
            ),
            (
                r#"$["a"]["\\"]"#,
                r#"{"a":{}}"#,
                vec!["a"],
                "\\",
                r#""value""#,
            ),
        ] {
            let root = doc(json);
            let before = root.clone();
            let additions = prepare(path, &root, &doc(r#""value""#)).unwrap();
            assert_eq!(additions.len(), 1, "{path}");
            assert_eq!(additions[0].parent, parent, "{path}");
            assert_eq!(additions[0].key, key, "{path}");
            assert_eq!(additions[0].value, doc(branch), "{path}");
            assert_eq!(root, before);
        }
    }

    #[test]
    fn blocked_existing_and_array_targets_create_nothing() {
        for (path, json) in [
            ("$.a.b.c", r#"{"a":{"b":{"c":1}}}"#),
            ("$.a.b.c", r#"{"a":3}"#),
            ("$.a.b.c", r#"{"a":{"b":"str"}}"#),
            ("$.a.b", r#"{"a":[1,2]}"#),
            ("$.a[0]", "{}"),
            ("$.a[0]", r#"{"a":[1]}"#),
            ("$.a[0].b", "{}"),
            ("$..[0]", r#"{"a":[]}"#),
        ] {
            assert!(
                prepare(path, &doc(json), &doc("5")).unwrap().is_empty(),
                "{path}"
            );
        }
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
                |path, kind| {
                    if let Some(kind) = kind {
                        existing.push((path, kind));
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
            |path, kind| paths.push((path, kind)),
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
    fn slices_wildcards_and_descendants_prepare_expected_parents() {
        for (path, json, parents) in [
            (
                "$.a[0:2].b",
                r#"{"a":[{},{},{}]}"#,
                vec![vec!["a", "0"], vec!["a", "1"]],
            ),
            (
                "$.*.n",
                r#"{"a":{},"b":{},"s":"str"}"#,
                vec![vec!["a"], vec!["b"]],
            ),
            ("$..n", r#"{"a":{}}"#, vec![vec![], vec!["a"]]),
            ("$['a','b'].c", r#"{"a":{},"b":{"c":1}}"#, vec![vec!["a"]]),
        ] {
            let additions = prepare(path, &doc(json), &doc("5")).unwrap();
            assert_eq!(
                additions
                    .iter()
                    .map(|a| a.parent.clone())
                    .collect::<Vec<_>>(),
                parents,
                "{path}"
            );
        }
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
            |path, kind| {
                if kind.is_some() {
                    updates.push(path);
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
        let result = attach_with(additions, |_, _, _| {
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
        let result = attach_with(prepare(&path, &root, &doc("5")), |_, _, _| {
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
        let result = attach_with(prepare("$..a.a", &root, &doc("5")), |_, _, _| {
            panic!("neither the overlapping nor deep branches may attach")
        });
        assert!(!result.any_created);
        assert_eq!(
            result.result.unwrap_err().to_string(),
            err_recursion_limit_exceeded().to_string()
        );
    }

    #[test]
    fn long_missing_suffix_is_rejected_before_recursive_construction() {
        let suffix = (0..1500)
            .map(|i| format!("x{i}"))
            .collect::<Vec<_>>()
            .join(".");
        assert!(prepare(&format!("$.{suffix}"), &doc("{}"), &doc("5")).is_err());
        assert!(prepare(
            &format!("$.*.{suffix}"),
            &doc(r#"{"a":{"x0":1},"b":3}"#),
            &doc("5")
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn creation_reports_progress_when_second_attachment_fails() {
        for fail_with_error in [false, true] {
            let additions = prepare("$.*.n", &doc(r#"{"a":{},"b":{},"c":{}}"#), &doc("5"));
            let mut attempts = 0;
            let mut attached = Vec::new();
            let result = attach_with(additions, |parent, _, _| {
                attempts += 1;
                if attempts == 2 {
                    return if fail_with_error {
                        Err(err_invalid_path())
                    } else {
                        Ok(false)
                    };
                }
                attached.push(parent);
                Ok(true)
            });
            assert_eq!(attached, vec![vec!["a"]]);
            assert_eq!(attempts, 2);
            assert!(result.any_created);
            assert!(result.result.is_err());
        }
    }

    #[test]
    fn increment_validation_checks_duplicate_existing_and_missing_targets() {
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
            assert_eq!(
                validate_increments(&root, &paths, increment)
                    .unwrap_err()
                    .to_string(),
                crate::manager::err_numeric_overflow().to_string()
            );
        }
        assert_eq!(root, before);
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
