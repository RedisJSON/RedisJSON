pub mod json_node;
pub mod json_path;
pub mod select_value;

use crate::jsonpath::select_value::SelectValue;
use json_path::{
    CalculationResult, DummyTracker, DummyTrackerGenerator, PTracker, PTrackerGenerator,
    PathCalculator, Query, QueryCompilationError, UserPathTracker,
};

/// Create a PathCalculator object. The path calculator can be re-used
/// to calculate json paths on different jsons.
/// /// ```rust
/// extern crate jsonpath_rs
/// #[macro_use] extern crate serde_json;
///
/// let query = jsonpath_rs::compile("$..friends[0]");
/// let calculator = jsonpath_rs::create(&query)
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
pub fn create<'i>(query: &'i Query<'i>) -> PathCalculator<'i, DummyTrackerGenerator> {
    PathCalculator::create(query)
}

/// Create a PathCalculator object. The path calculator can be re-used
/// to calculate json paths on different jsons.
/// Unlike create(), this function will return results with full path as PTracker object.
/// It is possible to create your own path tracker by implement the PTrackerGenerator trait.
pub fn create_with_generator<'i>(query: &'i Query<'i>) -> PathCalculator<'i, PTrackerGenerator> {
    PathCalculator::create_with_generator(query, PTrackerGenerator)
}

/// Compile the given json path, compilation results can after be used
/// to create `PathCalculator` calculator object to calculate json paths
pub fn compile(s: &str) -> Result<Query, QueryCompilationError> {
    json_path::compile(s)
}

/// Calc once allows to performe a one time calculation on the give query.
/// The query ownership is taken so it can not be used after. This allows
/// the get a better performance if there is a need to calculate the query
/// only once.
pub fn calc_once<'j, 'p, S: SelectValue>(q: Query<'j>, json: &'p S) -> Vec<&'p S> {
    let root = q.root;
    PathCalculator::<'p, DummyTrackerGenerator> {
        query: None,
        tracker_generator: None,
    }
    .calc_with_paths_on_root(json, root)
    .into_iter()
    .map(|e: CalculationResult<'p, S, DummyTracker>| e.res)
    .collect()
}

/// A version of `calc_once` that returns also paths.
pub fn calc_once_with_paths<'j, 'p, S: SelectValue>(
    q: Query<'j>,
    json: &'p S,
) -> Vec<CalculationResult<'p, S, PTracker>> {
    let root = q.root;
    PathCalculator {
        query: None,
        tracker_generator: Some(PTrackerGenerator),
    }
    .calc_with_paths_on_root(json, root)
}

/// A version of `calc_once` that returns only paths as Vec<Vec<String>>.
pub fn calc_once_paths<S: SelectValue>(q: Query, json: &S) -> Vec<Vec<String>> {
    let root = q.root;
    PathCalculator {
        query: None,
        tracker_generator: Some(PTrackerGenerator),
    }
    .calc_with_paths_on_root(json, root)
    .into_iter()
    .map(|e| e.path_tracker.unwrap().to_string_path())
    .collect()
}

#[cfg(test)]
mod json_path_tests {
    use serde_json::json;
    use serde_json::Value;
    // fn setup() {

    // }

    fn perform_search<'a>(path: &str, json: &'a Value) -> Vec<&'a Value> {
        let query = crate::jsonpath::compile(path).unwrap();
        let path_calculator = crate::jsonpath::create(&query);
        path_calculator.calc(json)
    }

    fn perform_path_search<'a>(path: &str, json: &'a Value) -> Vec<Vec<String>> {
        let query = crate::jsonpath::compile(path).unwrap();
        let path_calculator = crate::jsonpath::create_with_generator(&query);
        path_calculator.calc_paths(json)
    }

    macro_rules! verify_json {(
        path: $path:expr,
        json: $json:tt,
        results: [$($result:tt),* $(,)*]
    ) => {
        let j = json!($json);
        let res = perform_search($path, &j);
        let mut v = Vec::new();
        $(
            v.push(json!($result));
        )*
        assert_eq!(res, v.iter().collect::<Vec<&Value>>());
    }}

    macro_rules! verify_json_path {(
        path: $path:expr,
        json: $json:tt,
        results: [$([$($result:tt),*]),* $(,)*]
    ) => {
        let j = json!($json);
        let res = perform_path_search($path, &j);
        let mut v = Vec::new();
        $(
            let mut s = Vec::new();
            $(
                s.push(stringify!($result));
            )*
            v.push(s);
        )*
        assert_eq!(res, v);
    }}

    #[test]
    fn basic1() {
        verify_json!(path:"$.foo", json:{"foo":[1,2,3]}, results:[[1,2,3]]);
    }

    #[test]
    fn basic_bracket_notation() {
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
        verify_json!(path:"$.foo.[\"boo\"][*]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_multi_indexes() {
        verify_json!(path:"$.foo.[\"boo\"][0,2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,3]);
    }

    #[test]
    fn basic_bracket_notation_with_multi_neg_indexes() {
        verify_json!(path:"$.foo.[\"boo\"][-3,-1]", json:{"foo":{"boo":[1,2,3]}}, results:[1,3]);
    }

    #[test]
    fn basic_bracket_notation_with_range() {
        verify_json!(path:"$.foo.[\"boo\"][0:2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
    }

    #[test]
    fn basic_bracket_notation_with_all_range() {
        verify_json!(path:"$.foo.[\"boo\"][:]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_right_range() {
        verify_json!(path:"$.foo.[\"boo\"][:2]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
    }

    #[test]
    fn basic_bracket_notation_with_left_range() {
        verify_json!(path:"$.foo.[\"boo\"][1:]", json:{"foo":{"boo":[1,2,3]}}, results:[2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_left_range_neg() {
        verify_json!(path:"$.foo.[\"boo\"][-2:]", json:{"foo":{"boo":[1,2,3]}}, results:[2,3]);
    }

    #[test]
    fn basic_bracket_notation_with_right_range_neg() {
        verify_json!(path:"$.foo.[\"boo\"][:-1]", json:{"foo":{"boo":[1,2,3]}}, results:[1,2]);
    }

    #[test]
    fn basic_bracket_notation_with_multi_strings() {
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
        verify_json!(path:"$", json:{"foo":[1,2,3]}, results:[{"foo":[1,2,3]}]);
    }

    #[test]
    fn test_filter_number_eq() {
        verify_json!(path:"$.foo[?@ == 1]", json:{"foo":[1,2,3]}, results:[1]);
    }

    #[test]
    fn test_filter_number_eq_on_literal() {
        verify_json!(path:"$[?@.foo>=1].foo", json:[{"foo":1}], results:[1]);
    }

    #[test]
    fn test_filter_number_eq_floats() {
        verify_json!(path:"$.foo[?@ == 1.1]", json:{"foo":[1.1,2,3]}, results:[1.1]);
    }

    #[test]
    fn test_filter_string_eq() {
        verify_json!(path:"$.*[?@ == \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a"]);
    }

    #[test]
    fn test_filter_number_ne() {
        verify_json!(path:"$.*[?@ != 1]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[2,3,4,5,6]);
    }

    #[test]
    fn test_filter_number_ne_floats() {
        verify_json!(path:"$.*[?@ != 1.1]", json:{"foo":[1.1,2,3], "bar":[4.1,5,6]}, results:[2,3,4.1,5,6]);
    }

    #[test]
    fn test_filter_string_ne() {
        verify_json!(path:"$.*[?@ != \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["b","c","d","e","f"]);
    }

    #[test]
    fn test_filter_number_gt() {
        verify_json!(path:"$.*[?@ > 3]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[4,5,6]);
    }

    #[test]
    fn test_filter_number_gt_floats() {
        verify_json!(path:"$.*[?@ > 1.2]", json:{"foo":[1.1,2,3], "bar":[4,5,6]}, results:[2,3,4,5,6]);
    }

    #[test]
    fn test_filter_string_gt() {
        verify_json!(path:"$.*[?@ > \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["b","c","d","e","f"]);
    }

    #[test]
    fn test_filter_number_ge() {
        verify_json!(path:"$.*[?@ >= 3]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[3,4,5,6]);
    }

    #[test]
    fn test_filter_number_ge_floats() {
        verify_json!(path:"$.*[?@ >= 3.1]", json:{"foo":[1,2,3.1], "bar":[4,5,6]}, results:[3.1,4,5,6]);
    }

    #[test]
    fn test_filter_string_ge() {
        verify_json!(path:"$.*[?@ >= \"a\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn test_filter_number_lt() {
        verify_json!(path:"$.*[?@ < 4]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[1,2,3]);
    }

    #[test]
    fn test_filter_number_lt_floats() {
        verify_json!(path:"$.*[?@ < 3.9]", json:{"foo":[1,2,3], "bar":[3,5,6.9]}, results:[1,2,3,3]);
    }

    #[test]
    fn test_filter_string_lt() {
        verify_json!(path:"$.*[?@ < \"d\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a", "b", "c"]);
    }

    #[test]
    fn test_filter_number_le() {
        verify_json!(path:"$.*[?@ <= 6]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_number_le_floats() {
        verify_json!(path:"$.*[?@ <= 6.1]", json:{"foo":[1,2,3], "bar":[4,5,6]}, results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_string_le() {
        verify_json!(path:"$.*[?@ <= \"d\"]", json:{"foo":["a","b","c"], "bar":["d","e","f"]}, results:["a", "b", "c", "d"]);
    }

    #[test]
    fn test_filter_and() {
        verify_json!(path:"$[?@.foo[0] == 1 && @foo[1] == 2].foo[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3]);
    }

    #[test]
    fn test_filter_or() {
        verify_json!(path:"$[?@.foo[0] == 2 || @.bar[0] == 4].*[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_complex_filter() {
        verify_json!(path:"$[?(@.foo[0] == 1 && @.foo[2] == 3)||(@.bar[0]==4&&@.bar[2]==6)].*[0,1,2]", json:[{"foo":[1,2,3], "bar":[4,5,6]}], results:[1,2,3,4,5,6]);
    }

    #[test]
    fn test_filter_with_full_scan() {
        verify_json!(path:"$..[?(@.code==\"2\")].code", json:[{"code":"1"},{"code":"2"}], results:["2"]);
    }

    #[test]
    fn test_full_scan_with_all() {
        verify_json!(path:"$..*.*", json:[{"code":"1"},{"code":"2"}], results:["1", "2"]);
    }

    #[test]
    fn test_filter_with_all() {
        verify_json!(path:"$.*.[?(@.code==\"2\")].code", json:[[{"code":"1"},{"code":"2"}]], results:["2"]);
        verify_json!(path:"$.*[?(@.code==\"2\")].code", json:[[{"code":"1"},{"code":"2"}]], results:["2"]);
        verify_json!(path:"$*[?(@.code==\"2\")].code", json:[[{"code":"1"},{"code":"2"}]], results:["2"]);
    }

    #[test]
    fn test_filter_bool() {
        verify_json!(path:"$.*[?(@==true)]", json:{"a":true, "b":false}, results:[true]);
        verify_json!(path:"$.*[?(@==false)]", json:{"a":true, "b":false}, results:[false]);
    }

    #[test]
    fn test_complex_filter_from_root() {
        verify_json!(path:"$.bar.*[?@ == $.foo]",
                     json:{"foo":1, "bar":{"a":[1,2,3], "b":[4,5,6]}},
                     results:[1]);
    }

    #[test]
    fn test_complex_filter_with_literal() {
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
        verify_json!(path:"$.foo.*.val", 
                          json:{"foo":{"bar1":{"val":[1,2,3]}, "bar2":{"val":[1,2,3]}}},
                          results:[[1,2,3], [1,2,3]]);
    }

    #[test]
    fn test_full_scan() {
        verify_json!(path:"$..val", 
                          json:{"foo":{"bar1":{"val":[1,2,3]}, "bar2":{"val":[1,2,3]}}, "val":[1,2,3]},
                          results:[[1,2,3], [1,2,3], [1,2,3]]);
    }

    #[test]
    fn test_with_path() {
        verify_json_path!(path:"$.foo", json:{"foo":[1,2,3]}, results:[[foo]]);
    }

    #[test]
    fn test_expend_all_with_path() {
        verify_json_path!(path:"$.foo.*.val",
                          json:{"foo":{"bar1":{"val":[1,2,3]}, "bar2":{"val":[1,2,3]}}},
                          results:[[foo, bar1, val], [foo, bar2, val]]);
    }

    #[test]
    fn test_expend_all_with_array_path() {
        verify_json_path!(path:"$.foo.*.val",
                          json:{"foo":[
                                {"val":[1,2,3]},
                                {"val":[1,2,3]}
                            ]
                          },
                          results:[[foo, 0, val], [foo, 1, val]]);
    }
}
