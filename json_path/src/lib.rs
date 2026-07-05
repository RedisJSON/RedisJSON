/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

pub mod json_node;
pub mod json_path;
pub mod select_value;

use crate::json_path::{
    CalculationResult, DummyTracker, DummyTrackerGenerator, PTracker, PTrackerGenerator,
    PathCalculator, Query, QueryCompilationError, UserPathTracker,
};
use crate::select_value::{SelectValue, ValueRef};

// Mirror of Redis' `hide-user-data-from-log` server config. Defined in the
// `json_path` module so the standalone `jsonpath` binary (which includes that
// module directly) shares the same gate, and re-exported here for the module.
pub use crate::json_path::{hide_user_data_from_log, set_hide_user_data_from_log};

/// Create a `PathCalculator` object. The path calculator can be re-used
/// to calculate json paths on different JSONs.
///
/// ```
/// #[macro_use] extern crate serde_json;
///
/// use json_path;
///
/// let query = json_path::compile("$..friends[0]").unwrap();
/// let calculator = json_path::create(&query);
///
/// let json_obj = json!({
///     "school": {
///         "friends": [
///             {"name": "foo1", "age": 20},
///             {"name": "foo2", "age": 20}
///         ]
///     },
///     "friends": [
///         {"name": "foo3", "age": 30},
///         {"name": "foo4"}
/// ]});
///
/// let json = calculator.calc(&json_obj);
///
/// assert_eq!(json, vec![
///     &json!({"name": "foo3", "age": 30}),
///     &json!({"name": "foo1", "age": 20})
/// ]);
/// ```
#[must_use]
pub const fn create<'i>(query: &'i Query<'i>) -> PathCalculator<'i, DummyTrackerGenerator> {
    PathCalculator::create(query)
}

/// Create a `PathCalculator` object. The path calculator can be re-used
/// to calculate json paths on different JSONs.
/// Unlike create(), this function will return results with full path as `PTracker` object.
/// It is possible to create your own path tracker by implement the `PTrackerGenerator` trait.
#[must_use]
pub const fn create_with_generator<'i>(
    query: &'i Query<'i>,
) -> PathCalculator<'i, PTrackerGenerator> {
    PathCalculator::create_with_generator(query, PTrackerGenerator)
}

/// Compile the given json path, compilation results can after be used
/// to create `PathCalculator` calculator object to calculate json paths
pub fn compile(s: &str) -> Result<Query<'_>, QueryCompilationError> {
    json_path::compile(s)
}

/// Calc once allows to perform a one time calculation on the give query.
/// The query ownership is taken so it can not be used after. This allows
/// the get a better performance if there is a need to calculate the query
/// only once.
pub fn calc_once<'j, 'p, S: SelectValue>(q: Query<'j>, json: &'p S) -> Vec<ValueRef<'p, S>> {
    let root = q.root;
    PathCalculator::<'p, DummyTrackerGenerator> {
        query: None,
        tracker_generator: None,
    }
    .calc_with_paths_on_root(ValueRef::Borrowed(json), root)
    .into_iter()
    .map(|e: CalculationResult<'p, S, DummyTracker>| e.res)
    .collect()
}

/// Calc once for a projection query (e.g. `$.a + 1`, `$arr.length()`, `$.obj.keys()`,
/// `$.a.append(x)`): returns the flat list of computed values as impl-independent
/// `serde_json::Value`s (empty for Nothing). A scalar projection yields a 1-element list;
/// `keys()`/`~`/`append()` yield multiple. Only meaningful when `q.is_projection()` (empty
/// otherwise). The values are synthesized, never document nodes, so this is GET-output-only.
pub fn calc_once_projection<S: SelectValue>(q: Query, json: &S) -> Vec<serde_json::Value> {
    let Some(expr) = q.projection_expr().cloned() else {
        return Vec::new();
    };
    PathCalculator::<DummyTrackerGenerator> {
        query: None,
        tracker_generator: None,
    }
    .eval_projection(ValueRef::Borrowed(json), expr)
}

/// A version of `calc_once` that returns also paths.
pub fn calc_once_with_paths<'p, S: SelectValue>(
    q: Query<'_>,
    json: &'p S,
) -> Vec<CalculationResult<'p, S, PTracker>> {
    let root = q.root;
    PathCalculator {
        query: None,
        tracker_generator: Some(PTrackerGenerator),
    }
    .calc_with_paths_on_root(ValueRef::Borrowed(json), root)
}

/// A version of `calc_once` that returns only paths as Vec<Vec<String>>.
pub fn calc_once_paths<S: SelectValue>(q: Query, json: &S) -> Vec<Vec<String>> {
    let root = q.root;
    PathCalculator {
        query: None,
        tracker_generator: Some(PTrackerGenerator),
    }
    .calc_with_paths_on_root(ValueRef::Borrowed(json), root)
    .into_iter()
    // SAFETY: `PTrackerGenerator` is configured above: every match must have a path tracker so
    // path count stays aligned with the value match count (callers rely on this).
    .map(|e| e.path_tracker.unwrap().to_string_path())
    .collect()
}

#[cfg(test)]
mod json_path_tests {
    use crate::json_path;
    use crate::{create, create_with_generator};
    use serde_json::json;
    use serde_json::Value;

    #[allow(dead_code)]
    pub fn setup() {
        let _ = env_logger::try_init();
    }

    fn perform_search(path: &str, json: &Value) -> Vec<Value> {
        let query = json_path::compile(path).unwrap();
        let path_calculator = create(&query);
        path_calculator
            .calc(json)
            .into_iter()
            .map(|v| v.inner_cloned())
            .collect()
    }

    fn perform_path_search(path: &str, json: &Value) -> Vec<Vec<String>> {
        let query = json_path::compile(path).unwrap();
        let path_calculator = create_with_generator(&query);
        path_calculator.calc_paths(json)
    }

    /// Evaluate a projection query, returning its flat list of computed values. Asserts the
    /// query is classified as a projection.
    fn perform_projection_multi(path: &str, json: &Value) -> Vec<Value> {
        use crate::calc_once_projection;
        let query = json_path::compile(path).unwrap();
        assert!(
            query.is_projection(),
            "expected `{path}` to be a projection"
        );
        calc_once_projection(query, json)
    }

    /// Evaluate a single-value projection, returning the lone computed value (or `None` for
    /// Nothing). Panics if the projection yields multiple values (use the `_multi` form).
    fn perform_projection(path: &str, json: &Value) -> Option<Value> {
        let mut values = perform_projection_multi(path, json);
        assert!(
            values.len() <= 1,
            "`{path}` yielded multiple values; use perform_projection_multi"
        );
        values.pop()
    }

    macro_rules! verify_json {(
         path: $path:expr,
         json: $json:tt,
         results: [$($result:tt),* $(,)*]
     ) => {
         let j = json!($json);
         let res = perform_search($path, &j);
         let v = vec![$(json!($result)),*];
         assert_eq!(res, v.iter().cloned().collect::<Vec<Value>>());
     }}

    macro_rules! verify_json_path {(
         path: $path:expr,
         json: $json:tt,
         results: [$([$($result:tt),*]),* $(,)*]
     ) => {
         let j = json!($json);
         let res = perform_path_search($path, &j);
         let v = vec![$(vec![$(stringify!($result),)*],)*];
         assert_eq!(res, v);
     }}

    #[test]
    fn basic1() {
        verify_json!(path:"$.foo", json:{"foo":[1,2,3]}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation() {
        setup();
        verify_json!(path:"$[\"foo\"]", json:{"foo":[1,2,3]}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation_with_regular_notation1() {
        verify_json!(path:"$[\"foo\"].boo", json:{"foo":{"boo":[1,2,3]}}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation_with_regular_notation2() {
        verify_json!(path:"$.[\"foo\"].boo", json:{"foo":{"boo":[1,2,3]}}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation_with_regular_notation3() {
        verify_json!(path:"$.foo[\"boo\"]", json:{"foo":{"boo":[1,2,3]}}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation_with_regular_notation4() {
        verify_json!(path:"$.foo.[\"boo\"]", json:{"foo":{"boo":[1,2,3]}}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation_with_all() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][*]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_multi_indexes() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][0,2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,3]);
    }

    #[test]
    fn basic_bracket_notation_with_multi_neg_indexes() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][-3,-1]", json:{"foo":{"boo":[1,2,3]}}, results:[1,3]);
    }

    #[test]
    fn basic_full_range() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][0:2:1]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
        verify_json!(path:"$.foo.[\"boo\"][0:3:2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,3]);
        assert!(json_path::compile("$.foo.[\"boo\"][0:3:0]").is_err());
    }

    #[test]
    fn basic_bracket_notation_with_range() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][0:2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
    }

    #[test]
    fn basic_bracket_notation_with_all_range() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][:]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_right_range() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][:2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
    }

    #[test]
    fn basic_bracket_notation_with_left_range() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][1:]", json:{"foo":{"boo":[1,2,3]}}, results:[2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_left_range_neg() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][-2:]", json:{"foo":{"boo":[1,2,3]}}, results:[2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_right_range_neg() {
        setup();
        verify_json!(path:"$.foo.[\"boo\"][:-1]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
    }

    #[test]
    fn basic_bracket_notation_with_multi_strings() {
        setup();
        verify_json!(path:"$.[\"foo1\",\"foo2\"].boo[0,2]", json:{"foo1":{"boo":[1,2,3]}, "foo2":{"boo":[4,5,6]}}, results:[1,3,4,6]);
    }

    #[test]
    fn basic_index1() {
        verify_json!(path:"$[\"foo\"][1]", json:{"foo":[1,2,3]}, results:[2]);
    }

    #[test]
    fn basic_index2() {
        verify_json!(path:"$[\"foo\"].[1]", json:{"foo":[1,2,3]}, results:[2]);
    }

    #[test]
    fn basic_index3() {
        verify_json!(path:"$.foo.[1]", json:{"foo":[1,2,3]}, results:[2]);
    }

    #[test]
    fn basic_index4() {
        verify_json!(path:"$.foo[1]", json:{"foo":[1,2,3]}, results:[2]);
    }

    #[test]
    fn basic_index5() {
        verify_json!(path:"$[1].foo", json:[{"foo":[1,2,3]}, {"foo":[1]}], results:[[1]]);
    }

    #[test]
    fn basic_index6() {
        verify_json!(path:"$.[1].foo", json:[{"foo":[1,2,3]}, {"foo":[1]}], results:[[1]]);
    }

    #[test]
    fn basic_index7() {
        verify_json!(path:"$[1][\"foo\"]", json:[{"foo":[1,2,3]}, {"foo":[1]}], results:[[1]]);
    }

    #[test]
    fn root_only() {
        setup();
        verify_json!(path:"$", json:{"foo":[1,2,3]}, results:[{"foo":[1,2,3]}]);
    }

    #[test]
    fn test_filter_number_eq() {
        setup();
        verify_json!(path:"$.foo[?@ == 1]", json:{"foo":[1,2,3]}, results:[1]);
    }

    #[test]
    fn test_filter_number_eq_on_literal() {
        setup();
        verify_json!(path:"$[?@.foo>=1].foo", json:[{"foo":1}], results:[1]);
    }

    #[test]
    fn test_filter_number_eq_floats() {
        setup();
        verify_json!(path:"$.foo[?@ == 1.1]", json:{"foo":[1.1,2,3]}, results:[1.1]);
    }

    #[test]
    fn test_filter_string_eq() {
        setup();
        verify_json!(path:"$.*[?@ == \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a"]);
    }

    #[test]
    fn test_filter_number_ne() {
        setup();
        verify_json!(path:"$.*[?@ != 1]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[2,3,4,5,6]);
    }

    #[test]
    fn test_filter_number_ne_floats() {
        setup();
        verify_json!(path:"$.*[?@ != 1.1]", json:{"foo":[1.1,2,3], "bar":[4.1,5,6]}, results:[2,3,4.1,5,6]);
    }

    #[test]
    fn test_filter_string_ne() {
        setup();
        verify_json!(path:"$.*[?@ != \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["b","c","d","e","f"]);
    }

    #[test]
    fn test_filter_number_gt() {
        setup();
        verify_json!(path:"$.*[?@ > 3]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[4,5,6]);
    }

    #[test]
    fn test_filter_number_gt_floats() {
        setup();
        verify_json!(path:"$.*[?@ > 1.2]", json:{"foo":[1.1,2,3], "bar":[4,5,6]}, results:[2,3,4,5,6]);
    }

    #[test]
    fn test_filter_string_gt() {
        setup();
        verify_json!(path:"$.*[?@ > \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["b","c","d","e","f"]);
    }

    #[test]
    fn test_filter_number_ge() {
        setup();
        verify_json!(path:"$.*[?@ >= 3]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[3,4,5,6]);
    }

    #[test]
    fn test_filter_number_ge_floats() {
        setup();
        verify_json!(path:"$.*[?@ >= 3.1]", json:{"foo":[1,2,3.1], "bar":[4,5,6]}, results:[3.1,4,5,6]);
    }

    #[test]
    fn test_filter_string_ge() {
        setup();
        verify_json!(path:"$.*[?@ >= \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn test_filter_number_lt() {
        setup();
        verify_json!(path:"$.*[?@ < 4]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[1,2,3]);
    }

    #[test]
    fn test_filter_number_lt_floats() {
        setup();
        verify_json!(path:"$.*[?@ < 3.9]", json:{"foo":[1,2,3], "bar":[3,5,6.9]}, results:[1,2,3,3]);
    }

    #[test]
    fn test_filter_string_lt() {
        setup();
        verify_json!(path:"$.*[?@ < \"d\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a", "b", "c"]);
    }

    #[test]
    fn test_filter_number_le() {
        setup();
        verify_json!(path:"$.*[?@ <= 6]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_number_le_floats() {
        setup();
        verify_json!(path:"$.*[?@ <= 6.1]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_string_le() {
        setup();
        verify_json!(path:"$.*[?@ <= \"d\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a", "b", "c", "d"]);
    }

    #[test]
    fn test_filter_and() {
        setup();
        verify_json!(path:"$[?@.foo[0] == 1 && @foo[1] == 2].foo[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3]);
    }

    #[test]
    fn test_filter_and_three() {
        setup();
        verify_json!(path:"$[?@.foo[0] == 1 && @foo[1] == 2 && @foo[2] == 0]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[]);
    }

    #[test]
    fn test_filter_and_four() {
        setup();
        verify_json!(path:"$[?@.foo[0] == 1 && @foo[1] == 2 && @foo[2] == 2 && @foo[3] == 0]", json:[{"foo":[1,2,3,4], "bar":[4,5,6]}], results:[]);
    }

    #[test]
    fn test_filter_and_four_obj() {
        setup();
        verify_json!(path:"$[?(@.foo>1 && @.quux>8 && @.bar>3 && @.baz>4)]",
             json:[{"foo":1, "bar":2, "baz": 3, "quux": 4}, {"foo":2, "bar":4, "baz": 6, "quux": 9}, {"foo":2, "bar":3, "baz": 6, "quux": 10}],
             results:[{"foo":2, "bar":4, "baz": 6, "quux": 9}]);
    }

    #[test]
    fn test_filter_or() {
        setup();
        verify_json!(path:"$[?@.foo[0] == 2 || @.bar[0] == 4].*[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_or_three() {
        setup();
        verify_json!(path:"$[?@.foo[0] == 0 || @.bar[0] == 0 || @.foo[1] == 0 || @.bar[0] == 4 ].*[0,1,2]",
             json:[{"foo":[1,2,3], "bar":[4,5,6]}],
             results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_or_four() {
        setup();
        verify_json!(path:"$[?@.foo[0] == 2 || @.bar[0] == 4].*[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_complex_filter() {
        setup();
        verify_json!(path:"$[?(@.foo[0] == 1 && @.foo[2] == 3)||(@.bar[0]==4&&@.bar[2]==6)].*[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_complex_filter_precedence() {
        setup();
        let json = json!([{"t":true, "f":false, "one":1}, {"t":true, "f":false, "one":3}]);
        verify_json!(path:"$[?(@.f==true || @.one==1 && @.t==false)]", json:json, results:[]);
        verify_json!(path:"$[?(@.f==true || @.one==1 && @.t==true)].*", json:json, results:[true, false, 1]);
        verify_json!(path:"$[?(@.t==true && @.one==1 || @.t==true)].*", json:json, results:[true, false, 1, true, false, 3]);

        // With A=False, B=False, C=True
        // "(A && B) || C"  ==> True
        // "A && (B  || C)" ==> False
        verify_json!(path:"$[?(@.f==true &&  @.t==false || @.one==1)].*", json:json, results:[true, false, 1]);
        verify_json!(path:"$[?(@.f==true && (@.t==false || @.one==1))].*", json:json, results:[]);
    }

    #[test]
    fn test_complex_filter_nesting() {
        setup();
        let json = json!([{"t":true, "f":false, "one":1}, {"t":true, "f":false, "one":3}]);
        // With A=False, B=False, C=True
        // "(A && B) || C"  ==> True
        // "A && (B  || C)" ==> False
        verify_json!(path:"$[?(@.f==true &&  (@.f==true || (@.t==true && (@.one>1 && @.f==true))) || ((@.one==2 || @.one==1) && @.t==true))].*", json:json, results:[true, false, 1]);
        verify_json!(path:"$[?(@.f==true &&  ((@.f==true || (@.t==true && (@.one>1 && @.f==true))) || ((@.one==2 || @.one==1) && @.t==true)))].*", json:json, results:[]);
    }

    #[test]
    fn test_filter_negation_existence() {
        setup();
        verify_json!(path:"$[?!@.a]", json:[{"a":1},{"b":2}], results:[{"b":2}]);
    }

    #[test]
    fn test_filter_negation_double() {
        setup();
        verify_json!(path:"$[?!!@.a]", json:[{"a":1},{"b":2}], results:[{"a":1}]);
    }

    #[test]
    fn test_filter_negation_comparison_parenthesized() {
        setup();
        verify_json!(path:"$[?!(@.a==1)]", json:[{"a":1},{"a":2}], results:[{"a":2}]);
    }

    #[test]
    fn test_filter_negation_comparison_bare() {
        setup();
        // `!` applied directly to a comparison negates the whole comparison
        verify_json!(path:"$[?!@.a==1]", json:[{"a":1},{"a":2}], results:[{"a":2}]);
    }

    #[test]
    fn test_filter_negation_precedence_with_and() {
        setup();
        // !@.a && @.b  ==>  (!@.a) && @.b
        verify_json!(path:"$[?!@.a && @.b]", json:[{"a":1,"b":1},{"b":1},{"a":1}], results:[{"b":1}]);
    }

    #[test]
    fn test_filter_negation_with_parens_or() {
        setup();
        // !(@.a || @.b)
        verify_json!(path:"$[?!(@.a || @.b)]", json:[{"a":1},{"b":2},{"c":3}], results:[{"c":3}]);
    }

    #[test]
    fn test_function_length() {
        setup();
        // length: array elements / string chars
        verify_json!(path:"$.a[?length(@) > 2]", json:{"a":[[1,2,3],[1],"abcd","x"]}, results:[[1,2,3],"abcd"]);
    }

    #[test]
    fn test_function_length_object() {
        setup();
        // length of an object = number of members
        verify_json!(path:"$[?length(@) == 2]", json:[{"a":1,"b":2},{"a":1}], results:[{"a":1,"b":2}]);
    }

    #[test]
    fn test_function_count() {
        setup();
        verify_json!(path:"$[?count(@.*) == 3]", json:[{"a":1,"b":2,"c":3},{"a":1}], results:[{"a":1,"b":2,"c":3}]);
    }

    #[test]
    fn test_function_value() {
        setup();
        verify_json!(path:"$[?value(@.a) == 1]", json:[{"a":1},{"a":2}], results:[{"a":1}]);
    }

    #[test]
    fn test_function_match() {
        setup();
        // match is a full (anchored) match
        verify_json!(path:"$.a[?match(@, \"a.*\")]", json:{"a":["abc","xabc","a","b"]}, results:["abc","a"]);
    }

    #[test]
    fn test_function_search() {
        setup();
        // search is a substring match
        verify_json!(path:"$.a[?search(@, \"b\")]", json:{"a":["abc","xyz","b"]}, results:["abc","b"]);
    }

    #[test]
    fn test_re_with_computed_string_pattern() {
        setup();
        // `=~` RHS is a computed String (from concat), not a literal/document string.
        // concat(@.a, @.b) -> "ab"; substring-matches "abc" but not "xyz".
        verify_json!(path:r#"$.items[?@.s =~ concat(@.a, @.b)]"#,
            json:{"items":[{"s":"abc","a":"a","b":"b"},{"s":"xyz","a":"a","b":"b"}]},
            results:[{"s":"abc","a":"a","b":"b"}]);
    }

    #[test]
    fn test_regex_cache_cap_correctness() {
        setup();
        // More distinct (document-value) patterns than the regex-cache cap (64): entries
        // past the cap take the uncached path; results must still be correct.
        let items: Vec<Value> = (0..70)
            .map(|i| json!({"s": format!("v{i}"), "pat": format!("^v{i}$")}))
            .collect();
        let j = json!({ "a": items });
        // each element's string matches its own pattern -> all 70 returned
        assert_eq!(perform_search("$.a[?@.s =~ @.pat]", &j).len(), 70);
    }

    #[test]
    fn test_function_ceiling_floor() {
        setup();
        verify_json!(path:"$.a[?ceiling(@) == 3]", json:{"a":[2.1, 3.9, 1.0]}, results:[2.1]);
        verify_json!(path:"$.a[?floor(@) == 2]", json:{"a":[2.1, 2.9, 3.5]}, results:[2.1, 2.9]);
        // integers pass through unchanged
        verify_json!(path:"$.a[?ceiling(@) == 5]", json:{"a":[5, 6]}, results:[5]);
    }

    #[test]
    fn test_function_round_overflow_nothing() {
        setup();
        // 2^63 is one past i64::MAX; ceiling/floor must yield Nothing (no match), not a
        // value saturated to i64::MAX
        verify_json!(path:"$.a[?ceiling(@) == 9223372036854775807]", json:{"a":[9223372036854775808.0]}, results:[]);
        verify_json!(path:"$.a[?floor(@) == 9223372036854775807]", json:{"a":[9223372036854775808.0]}, results:[]);
    }

    #[test]
    fn test_function_abs() {
        setup();
        // integer abs stays integer; float abs stays float (objects, since the macro's
        // result list can't hold a bare negative literal)
        verify_json!(path:"$.a[?abs(@.n) == 5]", json:{"a":[{"n":-5},{"n":5},{"n":-3}]}, results:[{"n":-5},{"n":5}]);
        verify_json!(path:"$.a[?abs(@.n) == 2.5]", json:{"a":[{"n":-2.5},{"n":2.5},{"n":1.0}]}, results:[{"n":-2.5},{"n":2.5}]);
    }

    #[test]
    fn test_function_concat() {
        setup();
        verify_json!(path:"$.a[?concat(@.x, @.y) == \"ab\"]",
            json:{"a":[{"x":"a","y":"b"},{"x":"a","y":"c"}]},
            results:[{"x":"a","y":"b"}]);
        // a non-string argument yields Nothing -> no match
        verify_json!(path:"$.a[?concat(@.x, @.y) == \"a1\"]", json:{"a":[{"x":"a","y":1}]}, results:[]);
    }

    #[test]
    fn test_function_aggregations() {
        setup();
        verify_json!(path:"$.a[?sum(@.n) == 6]", json:{"a":[{"n":[1,2,3]},{"n":[1,1]}]}, results:[{"n":[1,2,3]}]);
        verify_json!(path:"$.a[?min(@.n) == 1]", json:{"a":[{"n":[3,1,2]},{"n":[5,6]}]}, results:[{"n":[3,1,2]}]);
        verify_json!(path:"$.a[?max(@.n) == 3]", json:{"a":[{"n":[3,1,2]},{"n":[5,6]}]}, results:[{"n":[3,1,2]}]);
        verify_json!(path:"$.a[?avg(@.n) == 2]", json:{"a":[{"n":[1,2,3]},{"n":[5,6]}]}, results:[{"n":[1,2,3]}]);
    }

    #[test]
    fn test_function_stddev() {
        setup();
        // population stddev of [2,4,4,4,5,5,7,9] is 2.0
        verify_json!(path:"$.a[?stddev(@.n) == 2.0]", json:{"a":[{"n":[2,4,4,4,5,5,7,9]},{"n":[1,2]}]}, results:[{"n":[2,4,4,4,5,5,7,9]}]);
    }

    #[test]
    fn test_function_aggregation_non_numeric_nothing() {
        setup();
        // a non-numeric element yields Nothing -> no match
        verify_json!(path:"$.a[?sum(@.n) == 3]", json:{"a":[{"n":[1,"x"]}]}, results:[]);
    }

    #[test]
    fn test_function_first_last_index() {
        setup();
        verify_json!(path:"$.a[?first(@.n) == 1]", json:{"a":[{"n":[1,2]},{"n":[9,8]}]}, results:[{"n":[1,2]}]);
        verify_json!(path:"$.a[?last(@.n) == 8]", json:{"a":[{"n":[1,2]},{"n":[9,8]}]}, results:[{"n":[9,8]}]);
        // index with a negative offset counts from the end
        verify_json!(path:"$.a[?index(@.n, -1) == 2]", json:{"a":[{"n":[1,2]},{"n":[9,8]}]}, results:[{"n":[1,2]}]);
        // out-of-range index -> Nothing -> no match
        verify_json!(path:"$.a[?index(@.n, 5) == 1]", json:{"a":[{"n":[1,2]}]}, results:[]);
    }

    #[test]
    fn test_function_aggregation_negatives() {
        setup();
        // non-array argument -> Nothing (number / string / object)
        verify_json!(path:"$.a[?sum(@.n) == 5]", json:{"a":[{"n":5}]}, results:[]);
        verify_json!(path:"$.a[?avg(@.n) == 0]", json:{"a":[{"n":"x"}]}, results:[]);
        verify_json!(path:"$.a[?max(@.n) == 0]", json:{"a":[{"n":{"k":1}}]}, results:[]);
        // heterogeneous array (a non-numeric element) -> Nothing, even though the numeric
        // elements alone would sum to the target (strict, no silent skipping)
        verify_json!(path:"$.a[?sum(@.n) == 3]", json:{"a":[{"n":[1,true,2]}]}, results:[]);
        verify_json!(path:"$.a[?sum(@.n) == 3]", json:{"a":[{"n":[1,null,2]}]}, results:[]);
        verify_json!(path:"$.a[?sum(@.n) == 3]", json:{"a":[{"n":[1,[2],3]}]}, results:[]);
        verify_json!(path:"$.a[?sum(@.n) == 3]", json:{"a":[{"n":[1,"2"]}]}, results:[]);
        // empty array -> Nothing
        verify_json!(path:"$.a[?sum(@.n) == 0]", json:{"a":[{"n":[]}]}, results:[]);
        verify_json!(path:"$.a[?min(@.n) == 0]", json:{"a":[{"n":[]}]}, results:[]);
    }

    #[test]
    fn test_function_index_negatives() {
        setup();
        // non-array argument -> Nothing
        verify_json!(path:"$.a[?first(@.n) == 1]", json:{"a":[{"n":5}]}, results:[]);
        verify_json!(path:"$.a[?last(@.n) == 1]", json:{"a":[{"n":"x"}]}, results:[]);
        // first/last of an empty array -> Nothing
        verify_json!(path:"$.a[?first(@.n) == 1]", json:{"a":[{"n":[]}]}, results:[]);
        // out-of-range index, positive and negative -> Nothing
        verify_json!(path:"$.a[?index(@.n, 9) == 1]", json:{"a":[{"n":[1,2]}]}, results:[]);
        verify_json!(path:"$.a[?index(@.n, -9) == 1]", json:{"a":[{"n":[1,2]}]}, results:[]);
        // non-numeric index -> Nothing
        verify_json!(path:r#"$.a[?index(@.n, "x") == 1]"#, json:{"a":[{"n":[1,2]}]}, results:[]);
        // a fractional index truncates toward zero: 1.9 -> 1 -> element 2
        verify_json!(path:"$.a[?index(@.n, 1.9) == 2]", json:{"a":[{"n":[1,2]}]}, results:[{"n":[1,2]}]);
    }

    #[test]
    fn test_function_wrong_arity_nothing() {
        setup();
        // wrong argument count -> Nothing (no match), instead of silently using a subset.
        // single-arg functions reject a second arg
        verify_json!(path:"$.a[?ceiling(@.n, 99) == 3]", json:{"a":[{"n":2.1}]}, results:[]);
        verify_json!(path:"$.a[?abs(@.n, 99) == 5]", json:{"a":[{"n":-5}]}, results:[]);
        verify_json!(path:"$.a[?sum(@.n, 99) == 6]", json:{"a":[{"n":[1,2,3]}]}, results:[]);
        verify_json!(path:"$.a[?first(@.n, 99) == 1]", json:{"a":[{"n":[1,2]}]}, results:[]);
        // index requires exactly two args
        verify_json!(path:"$.a[?index(@.n) == 1]", json:{"a":[{"n":[1,2]}]}, results:[]);
        verify_json!(path:"$.a[?index(@.n, 0, 9) == 1]", json:{"a":[{"n":[1,2]}]}, results:[]);
        // concat needs at least one arg: `concat()` is Nothing, not the empty string
        verify_json!(path:r#"$.a[?concat() == ""]"#, json:{"a":[{"n":1}]}, results:[]);
        // length/count are arity-checked too (extra args -> Nothing, not silently dropped)
        verify_json!(path:"$.a[?length(@.arr, 9) == 3]", json:{"a":[{"arr":[1,2,3]}]}, results:[]);
        verify_json!(path:"$.a[?count(@.arr, 9) == 1]", json:{"a":[{"arr":[1,2,3]}]}, results:[]);
        // the correct arity still matches
        verify_json!(path:"$.a[?length(@.arr) == 3]", json:{"a":[{"arr":[1,2,3]}]}, results:[{"arr":[1,2,3]}]);
    }

    #[test]
    fn compile_rejects_excessive_nesting() {
        setup();
        // Deeply nested parens must be rejected up front, not overflow the parser stack.
        let deep = format!("{}$.a{}", "(".repeat(5000), ")".repeat(5000));
        assert!(
            json_path::compile(&deep).is_err(),
            "deep nesting must be rejected"
        );
        // A modestly nested, valid projection still compiles.
        assert!(json_path::compile("((($.a + 1)))").is_ok());
        // Brackets inside a quoted string literal are not structural nesting, so a shallow
        // query with a bracket-heavy string key/regex must NOT be rejected.
        let in_string = format!(r#"$[?@.s == "{}"]"#, "(".repeat(300));
        assert!(
            json_path::compile(&in_string).is_ok(),
            "brackets inside a string literal must not count toward nesting depth"
        );
        // ...including a deep regex group on the right of `=~` (the realistic trigger).
        let re = format!(r#"$.a[?@.s =~ "{}"]"#, "(".repeat(300));
        assert!(
            json_path::compile(&re).is_ok(),
            "brackets inside a regex must not count"
        );
        // ...and a bracket-quoted field name literally containing many parens.
        let key = format!(r#"$["{}"]"#, "(".repeat(300));
        assert!(
            json_path::compile(&key).is_ok(),
            "brackets inside a key name must not count"
        );
    }

    #[test]
    fn test_membership_in_literal() {
        setup();
        verify_json!(path:"$.a[?@ in [2,4]]", json:{"a":[1,2,3,4]}, results:[2,4]);
    }

    #[test]
    fn test_membership_nin_literal() {
        setup();
        verify_json!(path:"$.a[?@ nin [2,4]]", json:{"a":[1,2,3,4]}, results:[1,3]);
    }

    #[test]
    fn test_membership_in_path_array() {
        setup();
        verify_json!(path:"$.a[?@ in $.allow]", json:{"a":[1,2,3],"allow":[2,3]}, results:[2,3]);
    }

    #[test]
    fn test_membership_structured_in_literal() {
        setup();
        verify_json!(path:"$.a[?@ in [[1],[2]]]", json:{"a":[[1],[2],[3]]}, results:[[1],[2]]);
    }

    #[test]
    fn test_membership_literal_in_path() {
        setup();
        // [4] in @.vals
        verify_json!(path:"$.items[?[4] in @.vals]",
            json:{"items":[{"vals":[1,2,[4]]},{"vals":[1,2]}]},
            results:[{"vals":[1,2,[4]]}]);
    }

    #[test]
    fn test_membership_value_in_path() {
        setup();
        // @.val in @.vals
        verify_json!(path:"$.items[?@.val in @.vals]",
            json:{"items":[{"val":2,"vals":[1,2,3]},{"val":9,"vals":[1,2,3]}]},
            results:[{"val":2,"vals":[1,2,3]}]);
    }

    #[test]
    fn test_membership_number_coercion() {
        setup();
        // numbers coerce int/float, aligned with `==`: 1.0 matches literal 1
        verify_json!(path:"$.a[?@ in [1,2]]", json:{"a":[1.0, 2.0, 3.0]}, results:[1.0,2.0]);
        // integer element matches a float in the document
        verify_json!(path:"$.a[?2 in @.vals]", json:{"a":[{"vals":[1.0,2.0]}]}, results:[{"vals":[1.0,2.0]}]);
    }

    #[test]
    fn test_set_subsetof_literal() {
        setup();
        // every element must be present; empty array is always a subset
        verify_json!(path:"$.a[?@ subsetof [1,2,3]]", json:{"a":[[1,2],[1,5],[]]}, results:[[1,2],[]]);
    }

    #[test]
    fn test_set_subsetof_path() {
        setup();
        verify_json!(path:"$.items[?@.val subsetof @.vals]",
            json:{"items":[{"val":[1,2],"vals":[1,2,3]},{"val":[1,9],"vals":[1,2,3]}]},
            results:[{"val":[1,2],"vals":[1,2,3]}]);
    }

    #[test]
    fn test_set_subsetof_numeric_coercion() {
        setup();
        // set ops coerce numbers like `in`/`nin`: an int element matches a float member
        // (`1` == `1.0`), so both arrays are subsets of the float literal
        verify_json!(path:"$.a[?@ subsetof [1.0,2.0,3.0]]", json:{"a":[[1.0,2.0],[1,2]]}, results:[[1.0,2.0],[1,2]]);
        // anyof/noneof coerce too: `2` intersects `[1.0,2.0]`
        verify_json!(path:"$.a[?@ anyof [1.0,2.0]]", json:{"a":[[2],[9]]}, results:[[2]]);
        verify_json!(path:"$.a[?@ noneof [1.0,2.0]]", json:{"a":[[2],[9]]}, results:[[9]]);
    }

    #[test]
    fn test_set_anyof() {
        setup();
        // non-empty intersection; empty array has none
        verify_json!(path:"$.a[?@ anyof [1,2,3]]", json:{"a":[[1,9],[8,9],[]]}, results:[[1,9]]);
    }

    #[test]
    fn test_set_noneof() {
        setup();
        // empty intersection; empty array trivially matches (no shared element)
        verify_json!(path:"$.a[?@ noneof [1,2,3]]", json:{"a":[[4,5],[1,9],[]]}, results:[[4,5],[]]);
    }

    #[test]
    fn test_set_relate_multi_node_any_of() {
        setup();
        // a multi-result left operand (`@.*`) is evaluated any-of per node, each node
        // treated as the array-shaped left operand (not the nodelist itself as one array)
        // subsetof: any node is a subset of the RHS
        verify_json!(path:"$.a[?@.* subsetof [1,2,3]]",
            json:{"a":[{"x":[1,2],"y":[9]},{"x":[7],"y":[8]}]},
            results:[{"x":[1,2],"y":[9]}]);
        // anyof: any node intersects the RHS
        verify_json!(path:"$.a[?@.* anyof [1,2,3]]",
            json:{"a":[{"x":[9,2],"y":[8]},{"x":[7],"y":[6]}]},
            results:[{"x":[9,2],"y":[8]}]);
        // noneof = no node intersects the RHS
        verify_json!(path:"$.a[?@.* noneof [1,2,3]]",
            json:{"a":[{"x":[9],"y":[8]},{"x":[7],"y":[2]}]},
            results:[{"x":[9],"y":[8]}]);
    }

    #[test]
    fn test_literal_built_once_independent_of_document_size() {
        setup();
        // Directly verifies the optimization (not just preserved semantics): the constant
        // literal `[1,2,3]` must be materialized once per query.
        fn build_count(path: &str, json: &Value) -> usize {
            json_path::BUILD_LITERAL_CALLS.with(|c| c.set(0));
            let _ = perform_search(path, json);
            json_path::BUILD_LITERAL_CALLS.with(|c| c.get())
        }

        let path = "$.a[?@ in [1,2,3]]";
        let small = json!({"a": [5, 1, 9, 2]});
        let big: Vec<i64> = (0..200).collect();
        let large = json!({ "a": big });

        let c_small = build_count(path, &small);
        let c_large = build_count(path, &large);

        assert!(c_small > 0, "literal should be built at least once");
        assert_eq!(
            c_small, c_large,
            "build_literal count scaled with document size ({c_small} vs {c_large}) — \
             literal is not being cached across elements"
        );
    }

    #[test]
    fn test_scalar_literal_container_fallbacks() {
        setup();
        // `value_in_array`: scalar-literal haystack via `subsetof` element membership.
        verify_json!(path:"$.a[?@ subsetof [9,8].index(0)]", json:{"a":[[1]]}, results:[]);
        // `member_of`: scalar-literal RHS of `in`.
        verify_json!(path:"$.a[?@ in [9,8].index(0)]", json:{"a":[1]}, results:[]);
        // `set_relate`: scalar-literal LHS of `anyof`.
        verify_json!(path:"$.a[?[9,8].index(0) anyof [1,2]]", json:{"a":[1]}, results:[]);
        // `term_number_seq`: scalar literal fed to an aggregate.
        verify_json!(path:"$.a[?[9,8].index(0).sum()]", json:{"a":[1]}, results:[]);
        // `function_index`: `index()` of a scalar literal.
        verify_json!(path:"$.a[?[9,8].index(0).index(0)]", json:{"a":[1]}, results:[]);
        // `function_keys`: `keys()` of a scalar literal.
        verify_json!(path:"$.a[?[9,8].index(0).keys()]", json:{"a":[1]}, results:[]);
        // `as_bool`: scalar non-bool literal as the RHS of `empty`.
        verify_json!(path:"$.a[?@ empty [9,8].index(0)]", json:{"a":[[]]}, results:[]);
        // `function_append`: `append()` onto a scalar literal wraps it as a single element,
        // so the synthesized list is non-empty and the element matches.
        verify_json!(path:"$.a[?[9,8].index(0).append(7)]", json:{"a":[1]}, results:[1]);
    }

    #[test]
    fn test_literal_in_subquery_built_once_per_query() {
        setup();
        fn build_count(path: &str, json: &Value) -> usize {
            json_path::BUILD_LITERAL_CALLS.with(|c| c.set(0));
            let _ = perform_search(path, json);
            json_path::BUILD_LITERAL_CALLS.with(|c| c.get())
        }

        // Baseline: the flat query caches `[1,2,3]` once across all elements.
        let once = build_count("$.a[?@ in [1,2,3]]", &json!({"a": [5, 1, 9, 2]}));
        assert!(once > 0, "literal should be built at least once");

        let path = "$.a[?@.b[?@ in [1,2,3]]]";
        let two = json!({"a": [{"b": [9]}, {"b": [9]}]});
        let four = json!({"a": [{"b": [9]}, {"b": [9]}, {"b": [9]}, {"b": [9]}]});

        // Correctness is unaffected: no inner `b` element is in `[1,2,3]`, so nothing matches.
        assert!(perform_search(path, &two).is_empty());
        // Materialized the same single time as the flat case, independent of how many outer
        // elements evaluate the subquery (before the shared cache it scaled with `$.a`'s size).
        assert_eq!(build_count(path, &two), once);
        assert_eq!(build_count(path, &four), once);
    }

    #[test]
    fn test_regex_in_subquery_compiled_once_per_query() {
        setup();
        fn compile_count(path: &str, json: &Value) -> usize {
            json_path::REGEX_COMPILE_CALLS.with(|c| c.set(0));
            let _ = perform_search(path, json);
            json_path::REGEX_COMPILE_CALLS.with(|c| c.get())
        }

        let path = "$.a[?@.b[?@ =~ \"x.*\"]]";
        let two = json!({"a": [{"b": ["y"]}, {"b": ["y"]}]});
        let four = json!({"a": [{"b": ["y"]}, {"b": ["y"]}, {"b": ["y"]}, {"b": ["y"]}]});

        // Correctness is unaffected: no inner `b` value matches `x.*`, so nothing matches.
        assert!(perform_search(path, &two).is_empty());
        // Compiled exactly once, independent of the number of outer elements.
        assert_eq!(compile_count(path, &two), 1);
        assert_eq!(compile_count(path, &four), 1);
    }

    #[test]
    fn test_size_of_array_and_string() {
        setup();
        // array element count and string char count
        verify_json!(path:"$.a[?@ sizeof 2]", json:{"a":[[4,5],[1],[7,8,9]]}, results:[[4,5]]);
        verify_json!(path:"$.a[?@ sizeof 2]", json:{"a":["ab","abc","xy"]}, results:["ab","xy"]);
        // `size` is accepted as an alias for `sizeof`
        verify_json!(path:"$.a[?@ size 2]", json:{"a":[[4,5],[1]]}, results:[[4,5]]);
        // objects are NOT sized (only arrays/strings): a 2-member object must not match
        verify_json!(path:"$.a[?@ sizeof 2]", json:{"a":[{"x":1,"y":2}, [3,4]]}, results:[[3,4]]);
    }

    #[test]
    fn test_size_of_truncates_and_rejects_non_numeric() {
        setup();
        // fractional size truncates toward zero; non-numeric size never matches
        verify_json!(path:"$.a[?@ sizeof 2.9]", json:{"a":[[4,5],[1]]}, results:[[4,5]]);
        verify_json!(path:r#"$.a[?@ sizeof "2"]"#, json:{"a":[[4,5]]}, results:[]);
    }

    #[test]
    fn test_empty_true_false() {
        setup();
        // empty true -> empty array/string; empty false -> non-empty
        verify_json!(path:"$.a[?@ empty true]", json:{"a":[[],[1],"",[2,3]]}, results:[[],""]);
        verify_json!(path:"$.a[?@ empty false]", json:{"a":[[],[1],"",[2,3]]}, results:[[1],[2,3]]);
        // objects are NOT subject to empty (only arrays/strings): neither {} nor {"k":1}
        // matches empty true or empty false
        verify_json!(path:"$.a[?@ empty true]", json:{"a":[{}, [], {"k":1}]}, results:[[]]);
        verify_json!(path:"$.a[?@ empty false]", json:{"a":[{}, [1], {"k":1}]}, results:[[1]]);
    }

    #[test]
    fn test_size_of_multi_node_any_of() {
        setup();
        // a multi-result left operand (`@.*`) matches any-of, like `==`/`<`/`in`:
        // the object matches because one of its values is a size-2 array
        verify_json!(path:"$.a[?@.* sizeof 2]",
            json:{"a":[{"x":[1],"y":[1,2]},{"x":[1],"y":[3]}]},
            results:[{"x":[1],"y":[1,2]}]);
    }

    #[test]
    fn test_empty_multi_node_any_of() {
        setup();
        // `@.* empty true` matches if any matched node is an empty array/string
        verify_json!(path:"$.a[?@.* empty true]",
            json:{"a":[{"x":[1],"y":[]},{"x":[1],"y":[3]}]},
            results:[{"x":[1],"y":[]}]);
    }

    #[test]
    fn test_size_of_multi_node_rhs_any_of() {
        setup();
        // a multi-result right operand (the size target) is any-of too, like `==`/`<`/`in`:
        // `@.v` matches if its length equals any of the `@.want` values
        verify_json!(path:"$.a[?@.v sizeof @.want[*]]",
            json:{"a":[{"v":[1,2],"want":[2,3]},{"v":[1],"want":[2,3]}]},
            results:[{"v":[1,2],"want":[2,3]}]);
    }

    #[test]
    fn test_empty_multi_node_rhs_any_of() {
        setup();
        // a multi-result right operand (the boolean) is any-of too
        verify_json!(path:"$.a[?@.v empty @.flags[*]]",
            json:{"a":[{"v":[],"flags":[true,true]},{"v":[1],"flags":[true,true]}]},
            results:[{"v":[],"flags":[true,true]}]);
    }

    #[test]
    fn test_arith_add() {
        setup();
        verify_json!(path:"$[?@.a + 1 == 3]", json:[{"a":2},{"a":5}], results:[{"a":2}]);
    }

    #[test]
    fn test_arith_sub() {
        setup();
        verify_json!(path:"$[?@.a - 1 == 4]", json:[{"a":5},{"a":2}], results:[{"a":5}]);
    }

    #[test]
    fn test_arith_mul() {
        setup();
        verify_json!(path:"$[?@.a * 2 == 6]", json:[{"a":3},{"a":2}], results:[{"a":3}]);
    }

    #[test]
    fn test_arith_div() {
        setup();
        // division is float: 8 / 2 == 4
        verify_json!(path:"$[?@.a / 2 == 4]", json:[{"a":8},{"a":3}], results:[{"a":8}]);
    }

    #[test]
    fn test_arith_rem() {
        setup();
        verify_json!(path:"$[?@.a % 2 == 0]", json:[{"a":4},{"a":3}], results:[{"a":4}]);
    }

    #[test]
    fn test_arith_precedence() {
        setup();
        // * binds tighter than +
        verify_json!(path:"$[?@.a + @.b * 2 == 7]", json:[{"a":1,"b":3},{"a":2,"b":2}], results:[{"a":1,"b":3}]);
    }

    #[test]
    fn test_arith_parens() {
        setup();
        verify_json!(path:"$[?(@.a + @.b) * 2 == 8]", json:[{"a":1,"b":3},{"a":2,"b":3}], results:[{"a":1,"b":3}]);
    }

    #[test]
    fn test_arith_unary_neg() {
        setup();
        verify_json!(path:"$[?-@.a == -3]", json:[{"a":3},{"a":1}], results:[{"a":3}]);
    }

    #[test]
    fn test_arith_parens_current() {
        setup();
        // bare `@ * 2` collides with the wildcard `*`; parens disambiguate
        verify_json!(path:"$.a[?(@) * 2 == 6]", json:{"a":[1,3]}, results:[3]);
    }

    #[test]
    fn test_arith_div_by_zero_no_match() {
        setup();
        // division by zero -> Nothing -> comparison is false
        verify_json!(path:"$[?@.a / 0 == 0]", json:[{"a":5}], results:[]);
    }

    #[test]
    fn test_literal_string_element() {
        setup();
        verify_json!(path:"$.a[?@ == [\"x\"]]", json:{"a":[["x"],["y"]]}, results:[["x"]]);
    }

    #[test]
    fn test_literal_bool_and_null() {
        setup();
        verify_json!(path:"$.a[?@ == [true, null]]", json:{"a":[[true,null],[false,null]]}, results:[[true,null]]);
    }

    #[test]
    fn test_literal_float() {
        setup();
        verify_json!(path:"$.a[?@ == [1.5]]", json:{"a":[[1.5],[2.5]]}, results:[[1.5]]);
    }

    #[test]
    fn test_arith_unary_plus() {
        setup();
        verify_json!(path:"$[?+@.a == 3]", json:[{"a":3},{"a":1}], results:[{"a":3}]);
    }

    #[test]
    fn test_arith_mod_by_zero_no_match() {
        setup();
        verify_json!(path:"$[?@.a % 0 == 0]", json:[{"a":5}], results:[]);
    }

    #[test]
    fn test_arith_mod_min_by_neg_one_no_panic() {
        setup();
        // i64::MIN % -1 overflows; must yield Nothing (no match), not panic
        verify_json!(path:"$[?@.a % -1 == 0]", json:[{"a": i64::MIN}], results:[]);
    }

    #[test]
    fn test_arith_non_numeric_operand_no_match() {
        setup();
        // arithmetic on a non-number yields Nothing -> no match
        verify_json!(path:"$[?@.a * 2 == 4]", json:[{"a":"x"}], results:[]);
    }

    #[test]
    fn test_arith_mixed_int_float() {
        setup();
        verify_json!(path:"$[?@.a + 0.5 == 2.5]", json:[{"a":2},{"a":5}], results:[{"a":2}]);
    }

    #[test]
    fn test_arith_float_mul_and_rem() {
        setup();
        verify_json!(path:"$[?@.a * 2 == 5]", json:[{"a":2.5},{"a":1}], results:[{"a":2.5}]);
        verify_json!(path:"$[?@.a % 2 == 1.5]", json:[{"a":3.5},{"a":4}], results:[{"a":3.5}]);
    }

    #[test]
    fn test_arith_unary_neg_float() {
        setup();
        verify_json!(path:"$[?-@.a == -1.5]", json:[{"a":1.5},{"a":2.0}], results:[{"a":1.5}]);
    }

    #[test]
    fn test_function_length_non_container_nothing() {
        setup();
        // length of a number is Nothing -> never > 0
        verify_json!(path:"$.a[?length(@) > 0]", json:{"a":[1,2]}, results:[]);
    }

    #[test]
    fn test_function_count_zero_and_one() {
        setup();
        // absent query -> 0
        verify_json!(path:"$[?count(@.x) == 0]", json:[{"y":1}], results:[{"y":1}]);
        // single node -> 1
        verify_json!(path:"$[?count(@.y) == 1]", json:[{"y":7}], results:[{"y":7}]);
    }

    #[test]
    fn test_function_value_multi_nothing() {
        setup();
        // value() of a multi-node query is Nothing -> no match
        verify_json!(path:"$[?value(@.*) == 1]", json:[{"a":1,"b":2}], results:[]);
    }

    #[test]
    fn test_membership_string_bool_null_lhs() {
        setup();
        // string / bool / null literal on the left-hand side of `in`
        verify_json!(path:"$.items[?\"x\" in @.tags]",
            json:{"items":[{"tags":["x","y"]},{"tags":["z"]}]},
            results:[{"tags":["x","y"]}]);
        verify_json!(path:"$.items[?true in @.flags]",
            json:{"items":[{"flags":[true]},{"flags":[false]}]},
            results:[{"flags":[true]}]);
        verify_json!(path:"$.items[?null in @.vals]",
            json:{"items":[{"vals":[null]},{"vals":[1]}]},
            results:[{"vals":[null]}]);
    }

    #[test]
    fn test_membership_string_value_in_literal() {
        setup();
        verify_json!(path:"$.a[?@ in [\"x\",\"y\"]]", json:{"a":["x","z"]}, results:["x"]);
    }

    #[test]
    fn test_membership_rhs_not_array_no_match() {
        setup();
        // RHS resolves to a scalar (not an array) -> no membership
        verify_json!(path:"$.items[?@.v in @.set]", json:{"items":[{"v":2,"set":5}]}, results:[]);
    }

    #[test]
    fn test_membership_nin_non_array_rhs() {
        setup();
        // `nin` is the strict negation of `in`: a non-array / absent RHS
        // makes `in` false, so `nin` matches.
        verify_json!(path:"$.items[?@.v nin @.set]", json:{"items":[{"v":2,"set":5}]}, results:[{"v":2,"set":5}]);
        verify_json!(path:"$.items[?@.v nin @.missing]", json:{"items":[{"v":2}]}, results:[{"v":2}]);
    }

    #[test]
    fn test_arith_requires_spaces() {
        setup();
        // `@.a + 1` (spaces) is addition
        verify_json!(path:"$[?@.a + 1 == 3]", json:[{"a":2}], results:[{"a":2}]);
        // `@.a+1` (no spaces) is a field named "a+1" (existence test), NOT arithmetic:
        // only the doc with that key matches; `{"a":2}` does not (which it would if this
        // were `@.a + 1`).
        verify_json!(path:"$[?@.a+1]", json:[{"a+1":5},{"a":2}], results:[{"a+1":5}]);
    }

    #[test]
    fn test_bare_term_bool_literal() {
        setup();
        // Bare boolean term: `false` matches nothing, `true` matches every node.
        verify_json!(path:"$[?false]", json:[1,2,3], results:[]);
        verify_json!(path:"$[?true]", json:[1,2,3], results:[1,2,3]);
    }

    #[test]
    fn test_filter_with_full_scan() {
        setup();
        verify_json!(path:"$..[?(@.code==\"2\")].code", json:[{"code":"1"},{"code":"2"}], results:["2"]);
    }

    #[test]
    fn test_full_scan_with_all() {
        setup();
        verify_json!(path:"$..*.*", json:[{"code":"1"},{"code":"2"}], results:["1", "2"]);
    }

    #[test]
    fn test_filter_with_all() {
        setup();
        verify_json!(path:"$.*.[?(@.code==\"2\")].code", json:[[{"code":"1"},{"code":"2"}]], results:["2"]);
        verify_json!(path:"$.*[?(@.code==\"2\")].code", json:[[{"code":"1"},{"code":"2"}]], results:["2"]);
        verify_json!(path:"$*[?(@.code==\"2\")].code", json:[[{"code":"1"},{"code":"2"}]], results:["2"]);
    }

    #[test]
    fn test_filter_bool() {
        setup();
        // Filter on container children (array elements)
        verify_json!(path:"$[?(@==true)]", json:[true, false], results:[true]);
        verify_json!(path:"$[?(@==false)]", json:[true, false], results:[false]);
        // Filter by object field value
        verify_json!(path:"$[?(@.a==true)]", json:[{"a":true}, {"a":false}], results:[{"a":true}]);
    }

    #[test]
    fn test_filter_null() {
        setup();
        // Filter on container children (array elements)
        verify_json!(path:"$[?(@==null)]", json:[null, 1], results:[null]);
        verify_json!(path:"$[?(@.*==null)]", json:[{"a":10}, {"b":null}, {"c":30}], results:[{"b": null}]);
    }

    #[test]
    fn test_complex_filter_from_root() {
        setup();
        verify_json!(path:"$.bar.*[?@ == $.foo]",
                      json:{"foo":1, "bar":{"a":[1,2,3], "b":[4,5,6]}},
                      results:[1]);
    }

    #[test]
    fn test_complex_filter_with_literal() {
        setup();
        verify_json!(path:"$.foo[?@.a == @.b].boo[:]",
                      json:{"foo":[{"boo":[1,2,3],"a":1,"b":1}]},
                      results:[1,2,3]);
    }

    #[test]
    fn basic2() {
        verify_json!(path:"$.foo.bar", json:{"foo":{"bar":[1,2,3]}}, results:[[1,2,3]]);
    }

    #[test]
    fn basic3() {
        verify_json!(path:"$foo", json:{"foo":[1,2,3]}, results:[[1,2,3]]);
    }

    #[test]
    fn test_expend_all() {
        setup();
        verify_json!(path:"$.foo.*.val",
                           json:{"foo":{"bar1":{"val":[1,2,3]}, "bar2":{"val":[1,2,3]}}},
                           results:[[1,2,3], [1,2,3]]);
    }

    #[test]
    fn test_full_scan() {
        setup();
        verify_json!(path:"$..val",
                           json:{"foo":{"bar1":{"val":[1,2,3]}, "bar2":{"val":[1,2,3]}}, "val":[1,2,3]},
                           results:[[1,2,3], [1,2,3], [1,2,3]]);
    }

    #[test]
    fn test_with_path() {
        setup();
        verify_json_path!(path:"$.foo", json:{"foo":[1,2,3]}, results:[[foo]]);
    }

    #[test]
    fn test_expend_all_with_path() {
        setup();
        verify_json_path!(path:"$.foo.*.val",
                           json:{"foo":{"bar1":{"val":[1,2,3]}, "bar2":{"val":[1,2,3]}}},
                           results:[[foo, bar1, val], [foo, bar2, val]]);
    }

    #[test]
    fn test_expend_all_with_array_path() {
        setup();
        verify_json_path!(path:"$.foo.*.val",
                           json:{"foo":[
                                 {"val":[1,2,3]},
                                 {"val":[1,2,3]}
                             ]
                           },
                           results:[[foo, 0, val], [foo, 1, val]]);
    }

    #[test]
    fn test_query_inside_object_values_indicates_array_path() {
        setup();
        verify_json_path!(path:"$.root[?(@.value > 2)]",
                           json:{
                            "root": {
                              "1": {
                                "value": 1
                              },
                              "2": {
                                "value": 2
                              },
                              "3": {
                                "value": 3
                              },
                              "4": {
                                "value": 4
                              },
                              "5": {
                                "value": 5
                              }
                            }
                          },
                           results:[[root, 3], [root, 4], [root, 5]]);
    }

    #[test]
    fn test_backslash_escape_detailed() {
        setup();
        verify_json!(path:r#"$["\\"]"#, json:{"\\": 1, "\\\\": 2}, results:[1]);
        verify_json!(path:r#"$["\\\\"]"#, json:{"\\": 1, "\\\\": 2}, results:[2]);
        verify_json!(path:r#"$["\\\\\\"]"#, json:{"\\": 1, "\\\\": 2, "\\\\\\": 3}, results:[3]);
        verify_json!(path:r#"$["\\\\\\\\"]"#, json:{"\\": 1, "\\\\": 2, "\\\\\\": 3, "\\\\\\\\": 4}, results:[4]);
    }

    #[test]
    fn test_quote_escape() {
        setup();
        verify_json!(path:r#"$["\""]"#, json:{"\"": 1}, results:[1]);
        verify_json!(path:r#"$["'"]"#, json:{"'": 1}, results:[1]);
        verify_json!(path:r#"$['\'']"#, json:{"'": 1}, results:[1]);
    }

    #[test]
    fn test_tab_escape() {
        setup();
        verify_json!(path:"$[\"\t\"]", json:{"\t": 1}, results:[1]);
    }

    #[test]
    fn test_newline_escape() {
        setup();
        verify_json!(path:"$[\"\n\"]", json:{"\n": 1}, results:[1]);
    }

    #[test]
    fn test_mixed_escapes() {
        setup();
        verify_json!(path:r#"$["\\\""]"#, json:{"\\\"": 1}, results:[1]);
        verify_json!(path:r#"$["a\\b"]"#, json:{"a\\b": 1}, results:[1]);
    }

    #[test]
    fn test_path_calculation_with_escapes() {
        setup();
        use crate::calc_once_paths;
        use crate::compile;
        let test_json = json!({"\\": 1, "\\\\": 2});
        let query1 = compile(r#"$["\\"]"#).unwrap();
        let paths1 = calc_once_paths(query1, &test_json);
        assert_eq!(paths1.len(), 1);
        assert_eq!(paths1[0], vec!["\\".to_string()]);
        let query2 = compile(r#"$["\\\\"]"#).unwrap();
        let paths2 = calc_once_paths(query2, &test_json);
        assert_eq!(paths2.len(), 1);
        assert_eq!(paths2[0], vec!["\\\\".to_string()]);
    }

    /// Guards the invariant used by `calc_once_paths` / `calc_paths`: with `PTrackerGenerator`,
    /// every match has a path tracker and the path list is the same length as the value list.
    #[test]
    fn calc_once_paths_aligns_with_matches_and_every_tracker_present() {
        setup();
        use crate::{calc_once, calc_once_paths, calc_once_with_paths};

        let cases = vec![
            ("$", json!({"a": 1})),
            ("$.a", json!({"a": 1})),
            ("$..*", json!({"a": {"b": 2}})),
            ("$.arr[*]", json!({"arr": [1, 2, 3]})),
        ];

        for (path, doc) in cases {
            let n_vals = calc_once(json_path::compile(path).unwrap(), &doc).len();
            let n_paths = calc_once_paths(json_path::compile(path).unwrap(), &doc).len();
            assert_eq!(
                n_vals, n_paths,
                "value vs path count mismatch for path {path:?}"
            );
            let with_paths = calc_once_with_paths(json_path::compile(path).unwrap(), &doc);
            assert_eq!(
                with_paths.len(),
                n_vals,
                "calc_once_with_paths length for {path:?}"
            );
            assert!(
                with_paths.iter().all(|e| e.path_tracker.is_some()),
                "expected every result to have path_tracker for path {path:?}"
            );
        }
    }

    /// `PathCalculator::calc_paths` (used with `create_with_generator`) must satisfy the same
    /// tracker invariant as `calc_once_paths`.
    #[test]
    fn calc_paths_on_generator_aligns_with_matches() {
        setup();
        use crate::calc_once;

        let path = "$..*";
        let doc = json!({"x": {"y": 1}});
        let q = json_path::compile(path).unwrap();
        let calculator = create_with_generator(&q);
        let string_paths = calculator.calc_paths(&doc);
        let n_vals = calc_once(q, &doc).len();
        assert_eq!(string_paths.len(), n_vals, "calc_paths vs calc_once count");
    }

    // ---- Projection (top-level computed expressions) ----

    #[test]
    fn projection_arithmetic() {
        setup();
        let doc = json!({"a": 2, "b": 4});
        assert_eq!(perform_projection("$.a + 1", &doc), Some(json!(3)));
        assert_eq!(perform_projection("$.a * $.b", &doc), Some(json!(8)));
        assert_eq!(perform_projection("$.a - $.b", &doc), Some(json!(-2)));
        // division is always float
        assert_eq!(
            perform_projection("($.a + $.b) / 2", &doc),
            Some(json!(3.0))
        );
        // unary minus
        assert_eq!(perform_projection("-$.a", &doc), Some(json!(-2)));
        // precedence: * binds tighter than +
        assert_eq!(perform_projection("$.a + $.b * 2", &doc), Some(json!(10)));
        // modulo, integer stays integer
        assert_eq!(perform_projection("$.b % $.a", &doc), Some(json!(0)));
    }

    #[test]
    fn projection_postfix_methods() {
        setup();
        let doc = json!({"arr": [1, 2, 3], "s": "héllo", "nums": [3, 1, 2], "matrix": [[1, 2, 3], [4, 5]]});
        assert_eq!(perform_projection("$.arr.length()", &doc), Some(json!(3)));
        // string length is char count
        assert_eq!(perform_projection("$.s.length()", &doc), Some(json!(5)));
        assert_eq!(perform_projection("$.nums.min()", &doc), Some(json!(1.0)));
        assert_eq!(perform_projection("$.nums.max()", &doc), Some(json!(3.0)));
        assert_eq!(perform_projection("$.nums.sum()", &doc), Some(json!(6.0)));
        // index(n) with the receiver as the array
        assert_eq!(perform_projection("$.arr.index(1)", &doc), Some(json!(2)));
        // first() returns the document element (node pass-through)
        assert_eq!(perform_projection("$.arr.first()", &doc), Some(json!(1)));
        // method chaining: first() -> [1,2,3] -> length() -> 3
        assert_eq!(
            perform_projection("$.matrix.first().length()", &doc),
            Some(json!(3))
        );
    }

    #[test]
    fn projection_function_edge_cases() {
        setup();
        let doc = json!({"a": 2, "s": "x", "big": 1e308});
        // unknown function -> Nothing
        assert_eq!(perform_projection("$.a.bogus()", &doc), None);
        // type-mismatched function -> Nothing
        assert_eq!(perform_projection("$.s.min()", &doc), None); // min on a string
        assert_eq!(perform_projection("$.a.length()", &doc), None); // length of a number
                                                                    // overflow to a non-finite float -> Nothing (not null), like division by zero
        assert_eq!(perform_projection("$.big * $.big", &doc), None);
    }

    #[test]
    fn projection_prefix_functions() {
        setup();
        let doc = json!({"arr": [1, 2, 3], "n": -5});
        assert_eq!(perform_projection("length($.arr)", &doc), Some(json!(3)));
        assert_eq!(perform_projection("abs($.n)", &doc), Some(json!(5)));
    }

    #[test]
    fn projection_nothing_is_empty() {
        setup();
        let doc = json!({"a": 5, "s": "x"});
        // division / modulo by zero -> Nothing
        assert_eq!(perform_projection("$.a / 0", &doc), None);
        assert_eq!(perform_projection("$.a % 0", &doc), None);
        // arithmetic on a non-number -> Nothing
        assert_eq!(perform_projection("$.s * 2", &doc), None);
        // missing field -> Nothing
        assert_eq!(perform_projection("$.missing + 1", &doc), None);
        // multi-node operand is not a single number -> Nothing
        let multi = json!({"o": {"x": 1}, "p": {"x": 2}});
        assert_eq!(perform_projection("$..x + 1", &multi), None);
    }

    #[test]
    fn projection_classification_backward_compat() {
        setup();
        // These must stay PLAIN PATHS (not projections) and keep today's behavior.
        for path in [
            "$",
            "$.a.b",
            "$..x",
            "$[*]",
            "$.a[?@>1]",
            "$[\"k\"]",
            "$[0:2]",
            "$.a+1",        // no spaces -> a field literally named "a+1"
            "$.arr.length", // no parens -> a field named "length"
            // A fully-parenthesized lone path is the same query as the unwrapped path, so it
            // classifies as a path (otherwise a multi-node result double-wraps: `($..x)` would
            // serialize as `[[..]]` instead of `[..]`).
            "($.a)",
            "($..x)",
            "(($.a))",
        ] {
            let q = json_path::compile(path).unwrap();
            assert!(!q.is_projection(), "`{path}` should be a plain path");
        }
        // These are projections.
        for path in [
            "$.a + 1",
            "-$.a",
            "$.a * $.b",
            "$.arr.length()",
            "length($.arr)",
            "($.a + 1)",
            // get-keys (`~` / keys()) and append() are projections (synthesized output).
            "$~",
            "$.obj~",
            "$.obj.keys()",
            "$.a.append(1)",
        ] {
            let q = json_path::compile(path).unwrap();
            assert!(q.is_projection(), "`{path}` should be a projection");
        }
    }

    #[test]
    fn projection_no_space_is_field_not_arithmetic() {
        setup();
        // `$.a+1` is the field "a+1" (path mode), NOT arithmetic.
        verify_json!(path:"$.a+1", json:{"a+1": 7, "a": 2}, results:[7]);
    }

    #[test]
    fn test_get_keys_projection() {
        setup();
        let doc = json!({"obj": {"x": 1, "y": 2}, "arr": [1, 2], "n": 5});
        // `~` and its keys() alias emit member names as a flat list of strings
        assert_eq!(
            perform_projection_multi("$.obj~", &doc),
            vec![json!("x"), json!("y")]
        );
        assert_eq!(
            perform_projection_multi("$.obj.keys()", &doc),
            vec![json!("x"), json!("y")]
        );
        // keys of the root
        assert_eq!(
            perform_projection_multi("$~", &doc),
            vec![json!("obj"), json!("arr"), json!("n")]
        );
        // a multi-match receiver yields the keys of each matched object, flattened
        // (non-object matches contribute nothing); `~` behaves the same.
        let multi = json!({"a": [{"x": 1, "y": 2}, 5, {"z": 3}]});
        assert_eq!(
            perform_projection_multi("$.a[*].keys()", &multi),
            vec![json!("x"), json!("y"), json!("z")]
        );
        assert_eq!(
            perform_projection_multi("$.a[*]~", &multi),
            vec![json!("x"), json!("y"), json!("z")]
        );
        // non-object / missing / empty-object -> empty
        assert_eq!(
            perform_projection_multi("$.arr~", &doc),
            Vec::<Value>::new()
        );
        assert_eq!(
            perform_projection_multi("$.n.keys()", &doc),
            Vec::<Value>::new()
        );
        assert_eq!(
            perform_projection_multi("$.missing~", &doc),
            Vec::<Value>::new()
        );
        assert_eq!(
            perform_projection_multi("$.obj~", &json!({"obj": {}})),
            Vec::<Value>::new()
        );
        // `~` is terminal: it closes the chain, so no trailing path, method, or repeated `~`.
        assert!(json_path::compile("$.obj~.x").is_err());
        assert!(json_path::compile("$.obj.keys().x").is_err());
        assert!(json_path::compile("$.obj~.length()").is_err());
        assert!(json_path::compile("$.obj~.append(1)").is_err());
        assert!(json_path::compile("$.obj~~").is_err());
        // `~` attaches only to a bare term, never after a method.
        assert!(json_path::compile("$.obj.keys()~").is_err());
        // the `keys()` function form stays composable (only the `~` operator is terminal).
        assert_eq!(
            perform_projection_multi(r#"$.obj.keys().append("z")"#, &doc),
            vec![json!("x"), json!("y"), json!("z")]
        );
    }

    #[test]
    fn test_append_projection() {
        setup();
        // A single matched array is appended INTO: `$.arr.append(x)` -> [...arr, x] (and the
        // explicit `$.arr[*]` element form gives the same result).
        let arr_doc = json!({"arr": [1, 2, 3]});
        assert_eq!(
            perform_projection_multi("$.arr.append(9)", &arr_doc),
            vec![json!(1), json!(2), json!(3), json!(9)]
        );
        assert_eq!(
            perform_projection_multi("$.arr[*].append(9)", &arr_doc),
            vec![json!(1), json!(2), json!(3), json!(9)]
        );
        let doc = json!({"books": [{"t": "a", "price": 30}, {"t": "b", "price": 5}]});
        // append(X) adds X as one extra element after the matched nodes (reply enrichment)
        assert_eq!(
            perform_projection_multi(r#"$.books[?(@.price >= 10)].append({"t":"X"})"#, &doc),
            vec![json!({"t": "a", "price": 30}), json!({"t": "X"})]
        );
        // a scalar value works too
        assert_eq!(
            perform_projection_multi("$.books[?(@.price >= 10)].append(99)", &doc),
            vec![json!({"t": "a", "price": 30}), json!(99)]
        );
        // no matched nodes -> just [X] (X is always appended)
        assert_eq!(
            perform_projection_multi(r#"$.books[?(@.price > 999)].append({"t":"X"})"#, &doc),
            vec![json!({"t": "X"})]
        );
        // a Nothing append argument (e.g. a non-matching path) -> the whole result is Nothing
        let xdoc = json!({"arr": [1, 2, 3], "other": [4, 5], "obj": {"k1": 1, "k2": 2}});
        assert_eq!(
            perform_projection_multi("$.arr.append($.missing)", &xdoc),
            Vec::<Value>::new()
        );
        // a multi-value argument is appended as ONE wrapped element (a path array, or a
        // synthesized keys() list) — not spread:
        assert_eq!(
            perform_projection_multi("$.arr.append($.other)", &xdoc),
            vec![json!(1), json!(2), json!(3), json!([4, 5])]
        );
        assert_eq!(
            perform_projection_multi("$.arr.append($.obj.keys())", &xdoc),
            vec![json!(1), json!(2), json!(3), json!(["k1", "k2"])]
        );
    }

    #[test]
    fn test_count_and_length_of_synthesized_results() {
        setup();
        // count() and length() of a keys()/append() Results list both report its element
        // count (not 0 / Nothing).
        let doc = json!({"obj": {"x": 1, "y": 2, "z": 3}, "arr": [1, 2, 3]});
        assert_eq!(
            perform_projection_multi("$.obj.keys().count()", &doc),
            vec![json!(3)]
        );
        assert_eq!(
            perform_projection_multi("$.obj.keys().length()", &doc),
            vec![json!(3)]
        );
        assert_eq!(
            perform_projection_multi("$.arr.append(9).count()", &doc),
            vec![json!(4)]
        );
        assert_eq!(
            perform_projection_multi("$.arr.append(9).length()", &doc),
            vec![json!(4)]
        );
    }

    #[test]
    fn test_value_of_single_key_result() {
        setup();
        // value() of a one-element synthesized list (a single-key object's keys) is that value
        assert_eq!(
            perform_projection_multi("$.obj.keys().value()", &json!({"obj": {"only": 1}})),
            vec![json!("only")]
        );
        // more than one key -> Nothing (RFC value() on a multi-node nodelist)
        assert_eq!(
            perform_projection_multi("$.obj.keys().value()", &json!({"obj": {"a": 1, "b": 2}})),
            Vec::<Value>::new()
        );
    }

    #[test]
    fn test_index_and_aggregate_on_results() {
        setup();
        let doc = json!({"obj": {"x": 1, "y": 2, "z": 3}, "arr": [1, 2, 3]});
        // first()/last()/index() index into a synthesized list
        assert_eq!(
            perform_projection_multi("$.obj.keys().first()", &doc),
            vec![json!("x")]
        );
        assert_eq!(
            perform_projection_multi("$.obj.keys().last()", &doc),
            vec![json!("z")]
        );
        assert_eq!(
            perform_projection_multi("$.obj.keys().index(1)", &doc),
            vec![json!("y")]
        );
        // out-of-range / empty -> Nothing
        assert_eq!(
            perform_projection_multi("$.o.keys().first()", &json!({"o": {}})),
            Vec::<Value>::new()
        );
        // aggregations fold a numeric synthesized list; an all-string list is non-numeric
        assert_eq!(
            perform_projection_multi("$.arr.append(9).sum()", &doc),
            vec![json!(15.0)]
        );
        assert_eq!(
            perform_projection_multi("$.arr.append(9).max()", &doc),
            vec![json!(9.0)]
        );
        assert_eq!(
            perform_projection_multi("$.obj.keys().sum()", &doc),
            Vec::<Value>::new()
        );
    }

    #[test]
    fn test_results_as_membership_and_set_operand() {
        setup();
        // A synthesized list (`keys()`) works as the array operand of `in`/`nin` ...
        verify_json!(path:"$.vals[?@ in $.obj.keys()]",
            json:{"obj":{"id":1,"name":"a"}, "vals":["id","zzz","name"]},
            results:["id","name"]);
        // ... and of the set operators (here as the left operand).
        verify_json!(path:r#"$.a[?@.o.keys() anyof ["x"]]"#,
            json:{"a":[{"o":{"x":1,"y":2}}, {"o":{"z":3}}]},
            results:[{"o":{"x":1,"y":2}}]);
        verify_json!(path:r#"$.a[?@.o.keys() subsetof ["x","y","z"]]"#,
            json:{"a":[{"o":{"x":1,"y":2}}, {"o":{"z":3}}]},
            results:[{"o":{"x":1,"y":2}}, {"o":{"z":3}}]);
        // ... and `sizeof`/`empty` (the key count).
        verify_json!(path:"$.a[?@.o.keys() sizeof 2]",
            json:{"a":[{"o":{"x":1,"y":2}}, {"o":{"z":3}}]},
            results:[{"o":{"x":1,"y":2}}]);
        verify_json!(path:"$.a[?@.o.keys() empty true]",
            json:{"a":[{"o":{}}, {"o":{"x":1}}]},
            results:[{"o":{}}]);
    }

    #[test]
    fn test_results_comparison_any_of() {
        setup();
        // ==, !=, and ordering compare a synthesized list any-of, like a `NodeList`.
        verify_json!(path:r#"$.a[?@.o.keys() == "id"]"#,
            json:{"a":[{"o":{"id":1,"name":"x"}}, {"o":{"zzz":3}}]},
            results:[{"o":{"id":1,"name":"x"}}]);
        verify_json!(path:r#"$.a[?@.o.keys() != "id"]"#,
            json:{"a":[{"o":{"id":1}}, {"o":{"zzz":3}}]},
            results:[{"o":{"zzz":3}}]);
        verify_json!(path:r#"$.a[?@.o.keys() > "m"]"#,
            json:{"a":[{"o":{"id":1}}, {"o":{"zzz":3}}]},
            results:[{"o":{"zzz":3}}]);
    }

    #[test]
    fn test_get_keys_existence_in_filter() {
        setup();
        // `?(@.o.keys())` is a non-empty-object test: an empty object's keys() is an empty
        // synthesized list and must NOT match.
        verify_json!(path:"$.a[?@.o.keys()]",
            json:{"a":[{"o":{}}, {"o":{"x":1}}, {"o":{"y":2,"z":3}}]},
            results:[{"o":{"x":1}}, {"o":{"y":2,"z":3}}]);
    }

    /// `hide-user-data-from-log` only gates whether user data is *logged*; it
    /// must never change the result of a query. Run the same filter with the
    /// gate off and on and assert the results are identical.
    #[test]
    fn hide_user_data_from_log_does_not_change_results() {
        use crate::{hide_user_data_from_log, set_hide_user_data_from_log};
        setup();
        let j = json!({"foo": [1, 2, 3, 4]});

        set_hide_user_data_from_log(false);
        let shown = perform_search("$.foo[?@ > 2]", &j);

        set_hide_user_data_from_log(true);
        assert!(hide_user_data_from_log());
        let hidden = perform_search("$.foo[?@ > 2]", &j);

        // Restore the default so other tests observe the verbose behaviour.
        set_hide_user_data_from_log(false);

        assert_eq!(shown, hidden);
        assert_eq!(shown, vec![json!(3), json!(4)]);
    }
}
