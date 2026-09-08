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

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use json_path::calc_once_paths;
use json_path::json_path::{JsonPathToken, Query};
use json_path::select_value::{SelectValue, SelectValueType, ValueRef};
use redis_module::{RedisError, RedisResult, RedisValue};

use crate::manager::{err_projection_readonly, AddUpdateInfo, Manager, UpdateInfo, WriteHolder};

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

impl CreateSite {
    /// The equivalent [`UpdateInfo`], for callers still expressing writes that
    /// way. Only meaningful for a shallow site, where no levels are missing.
    pub(crate) fn into_add_update_info(self) -> UpdateInfo {
        debug_assert!(
            self.levels.is_empty(),
            "a site with missing intermediate levels cannot become an AUI"
        );
        UpdateInfo::AUI(AddUpdateInfo {
            path: self.parent,
            key: self.leaf,
        })
    }
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
    let sites = plan_sites(query.clone(), doc);
    if create_intermediates {
        return Ok(sites);
    }
    if !query.clone().is_static() {
        return Ok(Vec::new());
    }
    Ok(sites
        .into_iter()
        .filter(|site| site.levels.is_empty())
        .collect())
}

/// The reply RedisJSON has always given for a write that matched nothing and
/// could create nothing: an error for a path that could never have worked,
/// otherwise `nil`.
pub(crate) fn nothing_to_write<V: SelectValue>(
    mut query: Query,
    doc: &V,
) -> RedisResult<RedisValue> {
    if !query.is_static() {
        return Err(RedisError::Str("Err wrong static path"));
    }
    if query.size() < 1 {
        return Err(RedisError::Str("Err path must end with object key to set"));
    }
    // A trailing array index is never created, so either it is out of range or
    // an NX is no-oping over a value that is already there.
    if matches!(query.clone().pop_last(), Some((_, JsonPathToken::Number)))
        && calc_once_paths(query, doc).is_empty()
    {
        return Err(RedisError::Str("ERR array index out of range"));
    }
    Ok(RedisValue::Null)
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
    let mut node = node_at(root, &prefix)?;
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
                return Some(CreateSite {
                    parent,
                    levels: levels.to_vec(),
                    leaf: leaf[0].clone(),
                });
            }
        }
    }
    None
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

/// Create every site's missing structure and return each created leaf's
/// concrete path, in the same order as `sites`.
///
/// `leaf` is what ends up at the deepest key: the command's own value for
/// `JSON.SET`/`JSON.MERGE`, or a seed (`[]`, `0`, `""`) for the commands whose
/// operation needs something to act on.
///
/// One `dict_add` per site, always onto an already-existing parent, with the
/// whole missing chain as its value. That keeps the write atomic -- the depth
/// limit is checked before anything is mutated -- and keeps managers that
/// cannot traverse a missing path element working unchanged.
pub(crate) fn materialize<M: Manager>(
    manager: &M,
    key: &mut M::WriteHolder,
    sites: &[CreateSite],
    leaf: &M::O,
) -> RedisResult<Vec<Vec<String>>> {
    sites
        .iter()
        .map(|site| {
            let mut keys = site.levels.clone();
            keys.push(site.leaf.clone());
            // `keys[0]` is added to `parent`; the rest nest inside it.
            let value = manager.nest_in_objects(&keys[1..], leaf.clone())?;
            key.dict_add(site.parent.clone(), &keys[0], value)?;

            let mut leaf_path = site.parent.clone();
            leaf_path.extend(keys);
            Ok(leaf_path)
        })
        .collect()
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
        assert_eq!(no_write("$..[0]", r#"{"a":[]}"#), "Err wrong static path");
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
        assert_eq!(no_write("$['a','b'].c", "{}"), "Err wrong static path");
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
            "Err wrong static path"
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
