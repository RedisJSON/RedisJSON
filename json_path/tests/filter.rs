/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

#[macro_use]
extern crate serde_json;

use common::{read_json, select_and_then_compare, setup};

mod common;

#[test]
fn quote() {
    setup();

    select_and_then_compare(
        r#"$['single\'quote']"#,
        json!({"single'quote":"value"}),
        json!(["value"]),
    );
    select_and_then_compare(
        r#"$["double\"quote"]"#,
        json!({"double\"quote":"value"}),
        json!(["value"]),
    );
}

#[test]
fn filter_next_all() {
    setup();

    for path in &[r#"$.*"#, r#"$[*]"#] {
        select_and_then_compare(
            path,
            json!(["string", 42, { "key": "value" }, [0, 1]]),
            json!(["string", 42, { "key": "value" }, [0, 1]]),
        );
    }
}

#[test]
fn filter_all() {
    setup();

    for path in &[r#"$..*"#, r#"$..[*]"#] {
        select_and_then_compare(
            path,
            json!(["string", 42, { "key": "value" }, [0, 1]]),
            json!([ "string", 42, { "key" : "value" }, [ 0, 1 ], "value", 0, 1 ]),
        );
    }
}

#[test]
fn filter_array_next_all() {
    setup();

    for path in &[r#"$.*.*"#, r#"$[*].*"#, r#"$.*[*]"#, r#"$[*][*]"#] {
        select_and_then_compare(
            path,
            json!(["string", 42, { "key": "value" }, [0, 1]]),
            json!(["value", 0, 1]),
        );
    }
}

#[test]
fn filter_all_complex() {
    setup();

    for path in &[r#"$..friends.*"#, r#"$[*].friends.*"#] {
        select_and_then_compare(
            path,
            read_json("./json_examples/data_array.json"),
            json!([
               { "id" : 0, "name" : "Millicent Norman" },
               { "id" : 1, "name" : "Vincent Cannon" },
               { "id" : 2, "name" : "Gray Berry" },
               { "id" : 0, "name" : "Tillman Mckay" },
               { "id" : 1, "name" : "Rivera Berg" },
               { "id" : 2, "name" : "Rosetta Erickson" }
            ]),
        );
    }
}

#[test]
fn filter_parent_with_matched_child() {
    setup();

    select_and_then_compare(
        "$[?(@.b.c == 1)]",
        json!({
            "a": {
                "b": {
                    "c": 1
                }
            }
        }),
        json!([
           {
              "b" : {
                 "c" : 1
              }
           }
        ]),
    );

    select_and_then_compare(
        "$.a[?(@.b.c == 1)]",
        json!({
            "a": {
                "b": {
                    "c": 1
                }
            }
        }),
        json!([]),
    );
}

#[test]
fn filter_parent_exist_child() {
    setup();

    select_and_then_compare(
        "$[?(@.b.c)]",
        json!({
            "a": {
                "b": {
                    "c": 1
                }
            }
        }),
        json!([
           {
              "b" : {
                 "c" : 1
              }
           }
        ]),
    );
}

#[test]
fn filter_parent_paths() {
    setup();

    select_and_then_compare(
        "$[?(@.key.subKey == 'subKey2')]",
        json!([
           {"key": {"seq": 1, "subKey": "subKey1"}},
           {"key": {"seq": 2, "subKey": "subKey2"}},
           {"key": 42},
           {"some": "value"}
        ]),
        json!([{"key": {"seq": 2, "subKey": "subKey2"}}]),
    );
}

#[test]
fn bugs33_exist_in_all() {
    setup();

    select_and_then_compare(
        "$..[?(@.first.second)]",
        json!({
            "foo": {
                "first": { "second": "value" }
            },
            "foo2": {
                "first": {}
            },
            "foo3": {
            }
        }),
        json!([
            {
                "first": {
                    "second": "value"
                }
            }
        ]),
    );
}

#[test]
fn bugs33_exist_left_in_all_with_and_condition() {
    setup();

    select_and_then_compare(
        "$..[?(@.first && @.first.second)]",
        json!({
            "foo": {
                "first": { "second": "value" }
            },
            "foo2": {
                "first": {}
            },
            "foo3": {
            }
        }),
        json!([
            {
                "first": {
                    "second": "value"
                }
            }
        ]),
    );
}

#[test]
fn bugs33_exist_right_in_all_with_and_condition() {
    setup();

    select_and_then_compare(
        "$..[?(@.b.c.d && @.b)]",
        json!({
            "a": {
                "b": {
                    "c": {
                        "d" : {
                            "e" : 1
                        }
                    }
                }
            }
        }),
        json!([
           {
              "b" : {
                "c" : {
                   "d" : {
                      "e" : 1
                   }
                }
              }
           }
        ]),
    );
}

#[test]
fn bugs38_array_notation_in_filter() {
    setup();

    select_and_then_compare(
        "$[?(@['key']==42)]",
        json!([
           {"key": 0},
           {"key": 42},
           {"key": -1},
           {"key": 41},
           {"key": 43},
           {"key": 42.0001},
           {"key": 41.9999},
           {"key": 100},
           {"some": "value"}
        ]),
        json!([{"key": 42}]),
    );

    select_and_then_compare(
        "$[?(@['key'].subKey == 'subKey2')]",
        json!([
           {"key": {"seq": 1, "subKey": "subKey1"}},
           {"key": {"seq": 2, "subKey": "subKey2"}},
           {"key": 42},
           {"some": "value"}
        ]),
        json!([{"key": {"seq": 2, "subKey": "subKey2"}}]),
    );

    select_and_then_compare(
        "$[?(@['key']['subKey'] == 'subKey2')]",
        json!([
           {"key": {"seq": 1, "subKey": "subKey1"}},
           {"key": {"seq": 2, "subKey": "subKey2"}},
           {"key": 42},
           {"some": "value"}
        ]),
        json!([{"key": {"seq": 2, "subKey": "subKey2"}}]),
    );

    select_and_then_compare(
        "$..key[?(@['subKey'] == 'subKey2')]",
        json!([
           {"key": [{"seq": 1, "subKey": "subKey1"}]},
           {"key": [{"seq": 2, "subKey": "subKey2"}]},
           {"key": [42]},
           {"some": "value"}
        ]),
        json!([{"seq": 2, "subKey": "subKey2"}]),
    );
}

#[test]
fn unimplemented_in_filter() {
    setup();

    let json = json!([{
       "store": {
           "book": [
             {"authors": [
                 {"firstName": "Nigel",
                   "lastName": "Rees"},
                 {"firstName": "Evelyn",
                   "lastName": "Waugh"}
               ],
               "title": "SayingsoftheCentury"},
             {"authors": [
                 {"firstName": "Herman",
                   "lastName": "Melville"},
                 {"firstName": "Somebody",
                   "lastName": "Else"}
               ],
               "title": "MobyDick"}
           ]}
    }]);

    // Should not panic
    //  unimplemented!("range syntax in filter")
    select_and_then_compare("$.store.book[?(@.authors[0:1])]", json.clone(), json!([]));

    // Should not panic
    //  unimplemented!("union syntax in filter")
    select_and_then_compare("$.store.book[?(@.authors[0,1])]", json.clone(), json!([]));

    // Should not panic
    //  unimplemented!("keys in filter");
    select_and_then_compare("$.store[?(@.book['authors', 'title'])]", json, json!([]));
}

#[test]
fn filter_nested() {
    setup();

    select_and_then_compare(
        "$.store.book[?(@.authors[?(@.lastName == 'Rees')])].title",
        json!({
            "store": {
                "book": [
                    {
                        "authors": [
                            {
                                "firstName": "Nigel",
                                "lastName": "Rees"
                            },
                            {
                                "firstName": "Evelyn",
                                "lastName": "Waugh"
                            }
                        ],
                        "title": "Sayings of the Century"
                    },
                    {
                        "authors": [
                            {
                                "firstName": "Herman",
                                "lastName": "Melville"
                            },
                            {
                                "firstName": "Somebody",
                                "lastName": "Else"
                            }
                        ],
                        "title": "Moby Dick"
                    }
                ]
            }
        }),
        json!(["Sayings of the Century"]),
    );
}

#[test]
fn filter_inner() {
    setup();

    select_and_then_compare(
        "$[?(@.inner.for.inner=='u8')].id",
        json!(
        {
            "a": {
              "id": "0:4",
              "inner": {
                "for": {"inner": "u8", "kind": "primitive"}
              }
            }
        }),
        json!(["0:4"]),
    );
}

#[test]
fn op_object_or_nonexisting_default() {
    setup();

    select_and_then_compare(
        "$.friends[?(@.id >= 2 || @.id == 4 || @.id == 6)]",
        read_json("./json_examples/data_obj.json"),
        json!([
            { "id" : 2, "name" : "Gray Berry" }
        ]),
    );
}

#[test]
fn recursive_descent_filter_no_duplicate_scalars() {
    setup();

    // GH#968 $..[?@>=1] must not list each scalar twice
    select_and_then_compare(r#"$..[?@>=1]"#, json!([1, 2, 3]), json!([1, 2, 3]));

    select_and_then_compare(r#"$..[?@>=1]"#, json!({ "a": [1, 2, 3] }), json!([1, 2, 3]));

    // Nested containers: each scalar appears once
    select_and_then_compare(r#"$..[?@>=1]"#, json!([[1, 2], 3]), json!([3, 1, 2]));

    // Mixed object + array nesting
    select_and_then_compare(
        r#"$..[?@>=1]"#,
        json!({"a": 1, "b": [2, 3]}),
        json!([1, 2, 3]),
    );

    select_and_then_compare(
        r#"$..[?@==@]"#,
        json!({"a":[1,2,3,["b",4],{"c":5},{"d":null}], "e":6}),
        json!([
            [1,2,3,["b",4],{"c":5},{"d":null}], 6,
            1, 2, 3, ["b",4], {"c":5}, {"d":null},
            "b", 4, 5, null
        ]),
    );
}

#[test]
fn filter_with_wildcard_subpath() {
    setup();

    // GH#963: @.* in filter should match when ANY child satisfies the condition.
    // Previously, @.*.x producing multiple results was treated as Invalid (always false).
    select_and_then_compare(
        "$[?(@.*.x > 10)]",
        json!([
            {"a": {"x": 5}, "b": {"x": 15}},
            {"c": {"x": 3}},
            {"d": {"x": 20}, "e": {"x": 1}}
        ]),
        json!([
            {"a": {"x": 5}, "b": {"x": 15}},
            {"d": {"x": 20}, "e": {"x": 1}}
        ]),
    );

    // Single result still works as before
    select_and_then_compare(
        "$[?(@.x > 10)]",
        json!([{"x": 5}, {"x": 15}, {"x": 3}]),
        json!([{"x": 15}]),
    );

    // Wildcard + equality
    select_and_then_compare(
        "$[?(@.*.v == 42)]",
        json!([
            {"a": {"v": 1}, "b": {"v": 42}},
            {"c": {"v": 7}}
        ]),
        json!([{"a": {"v": 1}, "b": {"v": 42}}]),
    );

    // Recursive descent in filter sub-path: @..key
    select_and_then_compare(
        "$[?(@..code > 2)]",
        json!([
            {"mode": {"code": 4}},
            {"code": 1},
            {"nested": {"deep": {"code": 10}}}
        ]),
        json!([
            {"mode": {"code": 4}},
            {"nested": {"deep": {"code": 10}}}
        ]),
    );

    // Wildcard sub-path with regex
    select_and_then_compare(
        r#"$[?(@.* =~ "^foo")]"#,
        json!([
            {"a": "foobar", "b": "baz"},
            {"c": "qux"}
        ]),
        json!([{"a": "foobar", "b": "baz"}]),
    );

    // Wildcard sub-path where no child matches
    select_and_then_compare(
        "$[?(@.*.x > 100)]",
        json!([{"a": {"x": 1}, "b": {"x": 2}}]),
        json!([]),
    );

    // NodeList ne: per RFC 9535, != is !(==), so NodeList([1,2]) != 1 is false
    // because NodeList([1,2]) == 1 is true (element 1 matches).
    select_and_then_compare(
        "$[?(@.*.v != 1)]",
        json!([
            {"a": {"v": 1}, "b": {"v": 2}},
            {"c": {"v": 1}},
            {"d": {"v": 3}, "e": {"v": 4}}
        ]),
        json!([{"d": {"v": 3}, "e": {"v": 4}}]),
    );

    // --- Coverage for right-side NodeList in ordering comparisons ---
    // Value < NodeList: scalar on left, wildcard multi-result on right
    select_and_then_compare(
        "$[?(@.x < @.*.y)]",
        json!([
            {"x": 1, "a": {"y": 5}, "b": {"y": 10}},
            {"x": 100, "a": {"y": 5}, "b": {"y": 10}},
        ]),
        json!([{"x": 1, "a": {"y": 5}, "b": {"y": 10}}]),
    );

    // Value >= NodeList
    select_and_then_compare(
        "$[?(@.x >= @.*.y)]",
        json!([
            {"x": 5, "a": {"y": 5}, "b": {"y": 10}},
            {"x": 1, "a": {"y": 5}, "b": {"y": 10}},
        ]),
        json!([{"x": 5, "a": {"y": 5}, "b": {"y": 10}}]),
    );

    // Value <= NodeList
    select_and_then_compare(
        "$[?(@.x <= @.*.y)]",
        json!([
            {"x": 10, "a": {"y": 5}, "b": {"y": 10}},
            {"x": 100, "a": {"y": 5}, "b": {"y": 10}},
        ]),
        json!([{"x": 10, "a": {"y": 5}, "b": {"y": 10}}]),
    );

    // --- Coverage for right-side NodeList in equality ---
    // Value == NodeList: matches if any element in NodeList equals the scalar
    select_and_then_compare(
        "$[?(@.x == @.*.y)]",
        json!([
            {"x": 5, "a": {"y": 5}, "b": {"y": 10}},
            {"x": 99, "a": {"y": 5}, "b": {"y": 10}},
        ]),
        json!([{"x": 5, "a": {"y": 5}, "b": {"y": 10}}]),
    );

    // --- Coverage for root-reference ($) producing NodeList ---
    // $.bounds.* yields [3, 15]; filter keeps data elements > any bound
    select_and_then_compare(
        "$.data[?(@ > $.bounds.*)]",
        json!({"data": [1, 5, 20], "bounds": [3, 15]}),
        json!([5, 20]),
    );

    // --- Coverage for NodeList vs NodeList ordering ---
    // Both sides are wildcards, comparison succeeds if ANY pair matches
    select_and_then_compare(
        "$[?(@.*.x > @.*.y)]",
        json!([
            {"a": {"x": 1}, "b": {"x": 10}, "c": {"y": 5}, "d": {"y": 20}},
            {"a": {"x": 1}, "b": {"x": 2}, "c": {"y": 100}, "d": {"y": 200}},
        ]),
        // First element: 10 > 5 is true → match
        // Second element: max x=2, min y=100, no x > any y → no match
        json!([{"a": {"x": 1}, "b": {"x": 10}, "c": {"y": 5}, "d": {"y": 20}}]),
    );

    // --- Coverage for NodeList vs NodeList equality ---
    select_and_then_compare(
        "$[?(@.*.x == @.*.y)]",
        json!([
            {"a": {"x": 1}, "b": {"x": 2}, "c": {"y": 2}, "d": {"y": 3}},
            {"a": {"x": 10}, "c": {"y": 20}},
        ]),
        // First element: x=2 matches y=2 → match
        // Second element: x=10 ≠ y=20 → no match
        json!([{"a": {"x": 1}, "b": {"x": 2}, "c": {"y": 2}, "d": {"y": 3}}]),
    );

    // --- Coverage for NodeList left with lt ---
    select_and_then_compare(
        "$[?(@.*.x < 10)]",
        json!([
            {"a": {"x": 5}, "b": {"x": 15}},
            {"a": {"x": 20}, "b": {"x": 30}},
        ]),
        // First element: 5 < 10 → match
        json!([{"a": {"x": 5}, "b": {"x": 15}}]),
    );

    // --- Coverage for NodeList left with ge ---
    select_and_then_compare(
        "$[?(@.*.x >= 10)]",
        json!([
            {"a": {"x": 3}, "b": {"x": 10}},
            {"a": {"x": 1}, "b": {"x": 2}},
        ]),
        // First element: 10 >= 10 → match
        json!([{"a": {"x": 3}, "b": {"x": 10}}]),
    );

    // --- Coverage for NodeList left with le ---
    select_and_then_compare(
        "$[?(@.*.x <= 10)]",
        json!([
            {"a": {"x": 5}, "b": {"x": 15}},
            {"a": {"x": 20}, "b": {"x": 30}},
        ]),
        // First element: 5 <= 10 → match
        json!([{"a": {"x": 5}, "b": {"x": 15}}]),
    );
}

#[test]
fn recursive_descent_filter_on_objects() {
    setup();

    // Recursive descent with filter on nested objects (no scalars involved)
    select_and_then_compare(
        "$..[?(@.active==true)]",
        json!({
            "users": [
                {"name": "Alice", "active": true},
                {"name": "Bob", "active": false}
            ]
        }),
        json!([{"name": "Alice", "active": true}]),
    );

    // Recursive descent with comparison filter on mixed nesting
    select_and_then_compare(
        "$..[?(@.score > 80)]",
        json!({
            "teams": {
                "a": [{"score": 90}, {"score": 70}],
                "b": [{"score": 85}]
            }
        }),
        json!([{"score": 90}, {"score": 85}]),
    );
}

#[test]
fn regex_with_nodelist_pattern() {
    setup();

    // Right side of =~ is a NodeList: match if ANY pattern matches the value.
    select_and_then_compare(
        r#"$[?(@.val =~ @.*.pat)]"#,
        json!([
            {"val": "abc", "x": {"pat": "^a"}, "y": {"pat": "^z"}},
            {"val": "xyz", "x": {"pat": "^a"}, "y": {"pat": "^x"}},
            {"val": "nope", "x": {"pat": "^a"}, "y": {"pat": "^z"}}
        ]),
        json!([
            {"val": "abc", "x": {"pat": "^a"}, "y": {"pat": "^z"}},
            {"val": "xyz", "x": {"pat": "^a"}, "y": {"pat": "^x"}}
        ]),
    );

    // Both sides are NodeLists: left @.* produces multiple strings,
    // right @.*.pat produces multiple regex patterns.
    select_and_then_compare(
        r#"$[?(@.* =~ @.*.pat)]"#,
        json!([
            {"a": "foobar", "x": {"pat": "^foo"}, "y": {"pat": "^nope"}}
        ]),
        json!([
            {"a": "foobar", "x": {"pat": "^foo"}, "y": {"pat": "^nope"}}
        ]),
    );
}

#[test]
fn gh963_wildcard_and_recursive_descent_in_filter() {
    setup();

    let doc = json!([
        [{"code":1},{"code":3}],
        [{"mode":{"code":4}},{"code":2}],
        [{"code":0},{"code":2}]
    ]);

    // @.*.code>2: wildcard over array elements, then .code
    select_and_then_compare(
        "$[?(@.*.code>2)]",
        doc.clone(),
        json!([
            [{"code":1},{"code":3}]
        ]),
    );

    // @..code>2: recursive descent finds all nested "code" values
    select_and_then_compare(
        "$[?(@..code>2)]",
        doc.clone(),
        json!([
            [{"code":1},{"code":3}],
            [{"mode":{"code":4}},{"code":2}]
        ]),
    );

    // Equality variant: @.*.code==2
    select_and_then_compare(
        "$[?(@.*.code==2)]",
        doc.clone(),
        json!([
            [{"mode":{"code":4}},{"code":2}],
            [{"code":0},{"code":2}]
        ]),
    );

    // Less-than: @.*.code<2
    select_and_then_compare(
        "$[?(@.*.code<2)]",
        doc.clone(),
        json!([
            [{"code":1},{"code":3}],
            [{"code":0},{"code":2}]
        ]),
    );
}

#[test]
fn nodelist_with_not_comparable_types() {
    setup();

    // NodeList containing mixed types — non-comparable pairs should not match
    select_and_then_compare(
        "$[?(@.*.v > 5)]",
        json!([
            {"a": {"v": "hello"}, "b": {"v": 10}},
            {"a": {"v": null}, "b": {"v": 3}},
        ]),
        // First: "hello" > 5 is not comparable, but 10 > 5 is true → match
        // Second: null > 5 is not comparable, 3 > 5 is false → no match
        json!([{"a": {"v": "hello"}, "b": {"v": 10}}]),
    );

    // NodeList where ALL elements are non-comparable
    select_and_then_compare(
        "$[?(@.*.v > 5)]",
        json!([
            {"a": {"v": "hello"}, "b": {"v": true}},
        ]),
        json!([]),
    );

    // NodeList eq with mixed types
    select_and_then_compare(
        "$[?(@.*.v == true)]",
        json!([
            {"a": {"v": true}, "b": {"v": 42}},
            {"a": {"v": 1}, "b": {"v": "yes"}},
        ]),
        json!([{"a": {"v": true}, "b": {"v": 42}}]),
    );
}

#[test]
fn nodelist_empty_and_single() {
    setup();

    // Sub-path that produces 0 results → Invalid → no match
    select_and_then_compare(
        "$[?(@.*.nonexistent > 0)]",
        json!([{"a": {"x": 1}}, {"b": {"x": 2}}]),
        json!([]),
    );

    // Sub-path that produces exactly 1 result → Value (not NodeList)
    select_and_then_compare(
        "$[?(@.a.x > 5)]",
        json!([{"a": {"x": 10}}, {"a": {"x": 3}}]),
        json!([{"a": {"x": 10}}]),
    );

    // Root-reference producing 0 results → Invalid
    select_and_then_compare(
        "$.data[?(@ > $.nonexistent.*)]",
        json!({"data": [1, 2, 3]}),
        json!([]),
    );

    // Root-reference producing exactly 1 result → Value
    select_and_then_compare(
        "$.data[?(@ > $.threshold)]",
        json!({"data": [1, 5, 20], "threshold": 10}),
        json!([20]),
    );
}
