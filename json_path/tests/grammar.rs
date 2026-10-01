/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use serde_json::json;
use std::num::NonZeroUsize;

#[test]
fn nested_filters_have_bounded_parser_work() {
    for (before, after) in [("", ""), ("(", " > 0)"), ("(", ") > 0")] {
        let mut inner = "@.x".to_owned();
        for _ in 0..12 {
            inner = format!("@.a[?{before}{inner}{after}]");
        }
        let path = format!("$.a[?{inner}]");

        // Bound parser work rather than wall-clock time, so slow CI machines remain reliable.
        // Keep all budget checks in one test: Pest's call limit is process-global.
        // These cases need fewer than 13,000 calls; the unfactored grammar exceeds 100,000.
        pest::set_call_limit(NonZeroUsize::new(50_000));
        let query = json_path::compile(&path);
        pest::set_call_limit(None);

        assert!(
            query.is_ok(),
            "nested filter exceeded parser budget for {path}: {query:?}"
        );
        assert!(!query.unwrap().is_projection());
    }
}

#[test]
fn factored_primaries_preserve_path_and_projection_classification() {
    for path in [
        "$",
        "$.a.b",
        "($..x)",
        "((($.a)))",
        "$.a[?@.b[?@.x]]",
        "$.a[?(@.b[?(@.x)])]",
        "$.a+1",
        "$.a.length",
        "$.a[?@.x + 1 > 2]",
    ] {
        let query = json_path::compile(path).unwrap();
        assert!(!query.is_projection(), "{path}");
    }
    for path in [
        "$.a + 1",
        "-$.a",
        "($.a + 1) * 2",
        "$.a.length()",
        "$.a.first().length()",
        "length($.a)",
        "$.a~",
        "$.a.keys().first()",
        "1",
        "@.a",
    ] {
        let query = json_path::compile(path).unwrap();
        assert!(query.is_projection(), "{path}");
    }
}

#[test]
fn factored_primaries_preserve_evaluation() {
    let doc = json!({"a": [[1, 2], [3]], "n": 4, "obj": {"x": 1}});
    for (path, expected) in [
        ("$.a.first().length()", vec![json!(2)]),
        ("-$.a.length()", vec![json!(-2)]),
        ("+$.a.first().length()", vec![json!(2)]),
        ("$.a.index(-1 + 2).length()", vec![json!(1)]),
        ("($.n + 2) * 3", vec![json!(18)]),
        ("$.a.index(1 + 0)", vec![json!([3])]),
        ("$.obj~", vec![json!("x")]),
        ("$.obj.keys().first()", vec![json!("x")]),
        ("length($.a)", vec![json!(2)]),
    ] {
        let query = json_path::compile(path).unwrap();
        assert_eq!(
            json_path::calc_once_projection(query, &doc),
            expected,
            "{path}"
        );
    }
    for path in ["$.a[?@[?@ == 2]]", "$.a[?(@[?(@ == 2)])]"] {
        let query = json_path::compile(path).unwrap();
        let actual = json_path::calc_once(query, &doc);
        assert_eq!(actual.len(), 1, "{path}");
        assert_eq!(actual[0].as_ref(), &json!([1, 2]), "{path}");
    }
}

#[test]
fn factored_primaries_reject_incomplete_or_nonterminal_suffixes() {
    for path in [
        "$.a +",
        "$.a[?@.x ==]",
        "$.a[?(@.x]",
        "$.a~.x",
        "$.a~.length()",
        "$.a~~",
        "$.a.keys()~",
        "$.a.keys().x",
        "$.a.index(1 +)",
        "$.a.length(",
    ] {
        assert!(json_path::compile(path).is_err(), "{path}");
    }
}

#[test]
fn parenthesized_filter_dispatch_preserves_arithmetic_strings_and_operators() {
    let doc = json!({"rows": [{"x": 2, "s": "(x)", "a)\"b": 2, "a)'b": 2}]});
    for path in [
        "$.rows[?(@.x) > 1]",
        "$.rows[?(@.x)>1]",
        "$.rows[?(@.x) + 1 == 3]",
        "$.rows[?(@.x) * 2 == 4]",
        "$.rows[?(@.x) in [1,2]]",
        "$.rows[?(@.x) nin [3,4]]",
        "$.rows[?(-@.x) < 0]",
        "$.rows[?(@.x)]",
        "$.rows[?(true)]",
        "$.rows[?((@.x) > 1)]",
        "$.rows[?((@.x > 1))]",
        "$.rows[?(@.x > 1) && (@.s == '(x)')]",
        "$.rows[?!(@.x < 1)]",
        r#"$.rows[?(@["a)\"b"]) == 2]"#,
        r#"$.rows[?(@['a)\'b']) == 2]"#,
    ] {
        let query = json_path::compile(path).unwrap();
        assert_eq!(json_path::calc_once(query, &doc).len(), 1, "{path}");
    }
    for path in ["$.rows[?(false)]", "$.rows[?((@.missing))]"] {
        let query = json_path::compile(path).unwrap();
        assert!(json_path::calc_once(query, &doc).is_empty(), "{path}");
    }
    for path in [
        "$.rows[?(@.x > 1) == true]",
        "$.rows[?(@.x > 1) + 1]",
        "$.rows[?(@.x > 1].x",
        "$.rows[?(@.x))]",
    ] {
        assert!(json_path::compile(path).is_err(), "{path}");
    }
}
