/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

//! Run with `cargo bench -p json_path --bench path_performance`.
//! Gungraun collects Callgrind instructions, with setup and validation excluded.
//! Prefer `bash json_path/benches/run_instructions.sh` for a stable executable path.

use std::fs::OpenOptions;
use std::hint::black_box;
use std::io::Write;

use gungraun::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use ijson::IValue;
use json_path::json_path::Query;
use json_path::{calc_once_projection, compile, create};
use serde_json::{json, Value};

struct Case {
    name: &'static str,
    path: String,
    document: Value,
    expected: Value,
}

fn case(name: &'static str, path: &str, document: Value, expected: Value) -> Case {
    Case {
        name,
        path: path.to_owned(),
        document,
        expected,
    }
}

// Record the exact generated path outside collection for CI summaries.
fn record_path(name: &str, path: &str) {
    let Some(destination) = std::env::var_os("JSONPATH_BENCHMARK_PATHS") else {
        return;
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(destination)
        .expect("open benchmark path metadata");
    writeln!(file, "{}", json!({"name": name, "path": path}))
        .expect("write benchmark path metadata");
}

fn prepare_compile(name: &'static str) -> String {
    let path = match name {
        "simple" => "$.rows[0].score".to_owned(),
        "filter" => "$.rows[?@.score > $.threshold]".to_owned(),
        "nested-7" | "nested-8" | "nested-9" => {
            let depth = name
                .strip_prefix("nested-")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            format!(
                "$.a{}@.flag{}",
                "[?@.a".repeat(depth - 1) + "[?",
                "]".repeat(depth)
            )
        }
        "nested-grouped-9" | "nested-comparison-9" | "nested-arithmetic-9" => {
            let (before, after) = match name {
                "nested-grouped-9" => ("(", ")"),
                "nested-comparison-9" => ("(", " > 0)"),
                _ => ("(", ") > 0"),
            };
            let mut inner = "@.flag".to_owned();
            for _ in 0..8 {
                inner = format!("@.a[?{before}{inner}{after}]");
            }
            format!("$.a[?{before}{inner}{after}]")
        }
        _ => path_form(name).path,
    };
    record_path(&format!("compile/{name}"), &path);
    black_box(compile(black_box(&path)).unwrap());
    path
}

type EvalInput = (Query<'static>, IValue);

fn prepare_eval(case: Case) -> EvalInput {
    let Case {
        name,
        path,
        document,
        expected,
    } = case;
    record_path(&format!("eval/{name}"), &path);
    // Each benchmark runs in its own short-lived process. Retain this one input
    // string so the query returned by setup can borrow it through measurement.
    let path = path.leak();
    let document: IValue = serde_json::from_value(document).unwrap();
    let query = compile(path).unwrap();
    if query.is_projection() {
        let expected: Vec<Value> = serde_json::from_value(expected).unwrap();
        assert_eq!(
            calc_once_projection(query.clone(), &document),
            expected,
            "{name}: {path}"
        );
        drop(expected);
        // The projection API consumes its query; clone it outside collection.
        black_box(calc_once_projection(query.clone(), black_box(&document)));
    } else {
        let expected: Vec<IValue> = serde_json::from_value(expected).unwrap();
        let actual: Vec<IValue> = create(&query)
            .calc(&document)
            .iter()
            .map(|value| value.inner_cloned())
            .collect();
        assert_eq!(actual, expected, "{name}: {path}");
        drop((actual, expected));
        // Warm up after validation cleanup so fixture heap consolidation is not measured.
        black_box(create(&query).calc(black_box(&document)));
    }
    (query, document)
}

fn path_form(name: &'static str) -> Case {
    let rows: Vec<_> = (0..256)
        .map(|uid| {
            json!({"uid": uid, "score": uid, "active": uid % 2 == 0, "name": format!("row-{uid}")})
        })
        .collect();
    let document = json!({
        "name": "Ada",
        "display.name": "Ada",
        "a\"b": 7,
        "profile": {"address": {"city": "London"}},
        "metrics": {"a": 1, "b": 2, "c": 3},
        "numbers": (0..64).collect::<Vec<_>>(),
        "matrix": [[1, 2, 3], [4, 5]],
        "rows": rows,
    });
    for (case_name, path, expected) in [
        ("root", "$", json!([document.clone()])),
        ("field", "$.name", json!(["Ada"])),
        ("quoted-field", "$['display.name']", json!(["Ada"])),
        ("escaped-field", r#"$["a\"b"]"#, json!([7])),
        ("deep-field", "$.profile.address.city", json!(["London"])),
        ("missing-field", "$.missing", json!([])),
        ("object-wildcard", "$.metrics.*", json!([1, 2, 3])),
        (
            "array-wildcard",
            "$.numbers[*]",
            json!((0..64).collect::<Vec<_>>()),
        ),
        ("negative-index", "$.numbers[-1]", json!([63])),
        ("out-of-range-index", "$.numbers[128]", json!([])),
        (
            "slice",
            "$.numbers[8:24]",
            json!((8..24).collect::<Vec<_>>()),
        ),
        (
            "slice-open",
            "$.numbers[-8:]",
            json!((56..64).collect::<Vec<_>>()),
        ),
        (
            "slice-step",
            "$.numbers[::4]",
            json!((0..64).step_by(4).collect::<Vec<_>>()),
        ),
        ("index-union", "$.numbers[3,0,3]", json!([3, 0, 3])),
        ("field-union", "$.metrics['c','a','c']", json!([3, 1, 3])),
        (
            "filter-and",
            "$.rows[?@.score >= 128 && @.active == true].uid",
            json!((128..256).step_by(2).collect::<Vec<_>>()),
        ),
        (
            "filter-or",
            "$.rows[?@.score < 2 || @.score >= 254].uid",
            json!([0, 1, 254, 255]),
        ),
        (
            "filter-grouped",
            "$.rows[?(@.score >= 128 && @.active == true)].uid",
            json!((128..256).step_by(2).collect::<Vec<_>>()),
        ),
        (
            "filter-not",
            "$.rows[?!(@.active == true)].uid",
            json!((1..256).step_by(2).collect::<Vec<_>>()),
        ),
        (
            "filter-arithmetic",
            "$.rows[?(@.score + 1) * 2 >= 510].uid",
            json!([254, 255]),
        ),
        (
            "filter-function",
            "$.rows[?length(@.name) == 5].uid",
            json!((0..10).collect::<Vec<_>>()),
        ),
        (
            "projection-arithmetic",
            "($.numbers[1] + 2) * 3",
            json!([9]),
        ),
        ("projection-function", "length($.numbers)", json!([64])),
        (
            "projection-method-chain",
            "$.matrix.first().length()",
            json!([3]),
        ),
        ("projection-aggregate", "$.numbers.sum()", json!([2016.0])),
        ("projection-keys", "$.metrics~", json!(["a", "b", "c"])),
        (
            "projection-append",
            "$.numbers.append(64)",
            json!((0..65).collect::<Vec<_>>()),
        ),
        ("projection-nothing", "$.missing.length()", json!([])),
    ] {
        if case_name == name {
            return case(name, path, document, expected);
        }
    }
    panic!("Unknown path form: {name}");
}

fn tree(depth: usize) -> Value {
    if depth == 0 {
        return json!({"uid": 1, "name": "leaf"});
    }
    let children: Vec<_> = (0..4).map(|_| tree(depth - 1)).collect();
    json!({"uid": 1, "children": children, "name": "branch"})
}

fn eval_case(name: &'static str) -> Case {
    match name {
        "recursive-objects" => case(name, "$..uid", tree(4), json!(vec![1; 341])),
        "recursive-no-match" => case(name, "$..absent", tree(4), json!([])),
        "existence-early-match" | "existence-no-match" => {
            let document = tree(4);
            let rows: Vec<_> = (0..128)
                .map(|uid| json!({"uid": uid, "flag": false, "children": document}))
                .collect();
            let (path, expected) = if name == "existence-early-match" {
                ("$.rows[?@..flag].uid", json!((0..128).collect::<Vec<_>>()))
            } else {
                ("$.rows[?@..absent].uid", json!([]))
            };
            case(name, path, json!({"rows": rows}), expected)
        }
        "root-single-candidate" => case(
            name,
            "$.rows[?@.score > $.threshold].uid",
            json!({"threshold": 128, "rows": [{"score": 129, "uid": 129}]}),
            json!([129]),
        ),
        "root-scalar"
        | "root-existence"
        | "root-existence-missing"
        | "root-descendant-list"
        | "root-descendant-sum"
        | "root-descendant-list-large"
        | "root-many-operands"
        | "simple" => {
            let rows: Vec<_> = (0..256)
                .map(|uid| json!({"uid": uid, "score": uid}))
                .collect();
            let mut document = json!({"rows": rows, "threshold": 128,
                "thresholds": vec![json!({"limit": 1000}); 128]});
            let (path, expected) = match name {
                "root-scalar" => (
                    "$.rows[?@.score > $.threshold].uid".to_owned(),
                    json!((129..256).collect::<Vec<_>>()),
                ),
                "root-existence" => (
                    "$.rows[?$.thresholds..limit].uid".to_owned(),
                    json!((0..256).collect::<Vec<_>>()),
                ),
                "root-existence-missing" => {
                    ("$.rows[?$.thresholds..absent].uid".to_owned(), json!([]))
                }
                "root-descendant-list" => (
                    "$.rows[?@.score > $.thresholds..limit].uid".to_owned(),
                    json!([]),
                ),
                "root-descendant-sum" => (
                    "$.rows[?@.score > $.thresholds..limit.sum()].uid".to_owned(),
                    json!([]),
                ),
                "root-descendant-list-large" => {
                    document =
                        json!({"rows": rows, "thresholds": vec![json!({"limit": 1000}); 2048]});
                    (
                        "$.rows[?@.score > $.thresholds..limit].uid".to_owned(),
                        json!([]),
                    )
                }
                "root-many-operands" => (
                    format!("$.rows[?{}].uid", ["$.threshold"; 65].join(" && ")),
                    json!((0..256).collect::<Vec<_>>()),
                ),
                _ => ("$.rows[0].score".to_owned(), json!([0])),
            };
            case(name, &path, document, expected)
        }
        "small-filter" => case(
            name,
            "$[?@.score > 1].score",
            json!([{"score": 0}, {"score": 2}]),
            json!([2]),
        ),
        "string-membership" => {
            let prefix = "long-prefix-".repeat(16);
            let allowed: Vec<_> = (0..16).map(|i| format!("{prefix}{i}")).collect();
            let rows: Vec<_> = (0..256).map(|i| format!("{prefix}{}", i % 32)).collect();
            let expected: Vec<_> = rows
                .iter()
                .filter(|s| allowed.contains(s))
                .cloned()
                .collect();
            case(
                name,
                &format!("$[?@ in {}]", json!(allowed)),
                json!(rows),
                json!(expected),
            )
        }
        "deep-string-equality" => {
            let value = json!({"strings": ["long-prefix-".repeat(16), "tail"]});
            case(
                name,
                &format!("$[?@ == {value}]"),
                json!(vec![value.clone(); 256]),
                json!(vec![value; 256]),
            )
        }
        "regex-search-cache" | "regex-match-cache" | "regex-search-function" => {
            let rows: Vec<_> = (0..1024)
                .map(|uid| json!({"uid": uid, "name": format!("customer-{uid}-active")}))
                .collect();
            let path = match name {
                "regex-search-cache" => {
                    r#"$.rows[?@.name =~ "(?:customer|operator)-[0-9]+-active"].uid"#
                }
                "regex-match-cache" => {
                    r#"$.rows[?match(@.name, "(?:customer|operator)-[0-9]+-active")].uid"#
                }
                _ => r#"$.rows[?search(@.name, "(?:customer|operator)-[0-9]+-active")].uid"#,
            };
            case(
                name,
                path,
                json!({"rows": rows}),
                json!((0..1024).collect::<Vec<_>>()),
            )
        }
        _ => path_form(name),
    }
}

#[library_benchmark]
#[bench::simple(prepare_compile("simple"))]
#[bench::filter(prepare_compile("filter"))]
#[bench::nested_7(prepare_compile("nested-7"))]
#[bench::nested_8(prepare_compile("nested-8"))]
#[bench::nested_9(prepare_compile("nested-9"))]
#[bench::nested_grouped_9(prepare_compile("nested-grouped-9"))]
#[bench::nested_comparison_9(prepare_compile("nested-comparison-9"))]
#[bench::nested_arithmetic_9(prepare_compile("nested-arithmetic-9"))]
#[bench::root(prepare_compile("root"))]
#[bench::field(prepare_compile("field"))]
#[bench::quoted_field(prepare_compile("quoted-field"))]
#[bench::escaped_field(prepare_compile("escaped-field"))]
#[bench::deep_field(prepare_compile("deep-field"))]
#[bench::missing_field(prepare_compile("missing-field"))]
#[bench::object_wildcard(prepare_compile("object-wildcard"))]
#[bench::array_wildcard(prepare_compile("array-wildcard"))]
#[bench::negative_index(prepare_compile("negative-index"))]
#[bench::out_of_range_index(prepare_compile("out-of-range-index"))]
#[bench::slice(prepare_compile("slice"))]
#[bench::slice_open(prepare_compile("slice-open"))]
#[bench::slice_step(prepare_compile("slice-step"))]
#[bench::index_union(prepare_compile("index-union"))]
#[bench::field_union(prepare_compile("field-union"))]
#[bench::filter_and(prepare_compile("filter-and"))]
#[bench::filter_or(prepare_compile("filter-or"))]
#[bench::filter_grouped(prepare_compile("filter-grouped"))]
#[bench::filter_not(prepare_compile("filter-not"))]
#[bench::filter_arithmetic(prepare_compile("filter-arithmetic"))]
#[bench::filter_function(prepare_compile("filter-function"))]
#[bench::projection_arithmetic(prepare_compile("projection-arithmetic"))]
#[bench::projection_function(prepare_compile("projection-function"))]
#[bench::projection_method_chain(prepare_compile("projection-method-chain"))]
#[bench::projection_aggregate(prepare_compile("projection-aggregate"))]
#[bench::projection_keys(prepare_compile("projection-keys"))]
#[bench::projection_append(prepare_compile("projection-append"))]
#[bench::projection_nothing(prepare_compile("projection-nothing"))]
fn compile_path(path: String) -> String {
    black_box(compile(black_box(&path)).unwrap());
    // Return the input so its destruction stays outside the measured function.
    path
}

#[library_benchmark(setup = prepare_eval)]
#[bench::recursive_objects(eval_case("recursive-objects"))]
#[bench::recursive_no_match(eval_case("recursive-no-match"))]
#[bench::existence_early_match(eval_case("existence-early-match"))]
#[bench::existence_no_match(eval_case("existence-no-match"))]
#[bench::root_scalar(eval_case("root-scalar"))]
#[bench::root_single_candidate(eval_case("root-single-candidate"))]
#[bench::root_existence(eval_case("root-existence"))]
#[bench::root_existence_missing(eval_case("root-existence-missing"))]
#[bench::root_descendant_list(eval_case("root-descendant-list"))]
#[bench::root_descendant_sum(eval_case("root-descendant-sum"))]
#[bench::root_descendant_list_large(eval_case("root-descendant-list-large"))]
#[bench::root_many_operands(eval_case("root-many-operands"))]
#[bench::simple(eval_case("simple"))]
#[bench::small_filter(eval_case("small-filter"))]
#[bench::string_membership(eval_case("string-membership"))]
#[bench::deep_string_equality(eval_case("deep-string-equality"))]
#[bench::regex_search_cache(eval_case("regex-search-cache"))]
#[bench::regex_match_cache(eval_case("regex-match-cache"))]
#[bench::regex_search_function(eval_case("regex-search-function"))]
#[bench::root(eval_case("root"))]
#[bench::field(eval_case("field"))]
#[bench::quoted_field(eval_case("quoted-field"))]
#[bench::escaped_field(eval_case("escaped-field"))]
#[bench::deep_field(eval_case("deep-field"))]
#[bench::missing_field(eval_case("missing-field"))]
#[bench::object_wildcard(eval_case("object-wildcard"))]
#[bench::array_wildcard(eval_case("array-wildcard"))]
#[bench::negative_index(eval_case("negative-index"))]
#[bench::out_of_range_index(eval_case("out-of-range-index"))]
#[bench::slice(eval_case("slice"))]
#[bench::slice_open(eval_case("slice-open"))]
#[bench::slice_step(eval_case("slice-step"))]
#[bench::index_union(eval_case("index-union"))]
#[bench::field_union(eval_case("field-union"))]
#[bench::filter_and(eval_case("filter-and"))]
#[bench::filter_or(eval_case("filter-or"))]
#[bench::filter_grouped(eval_case("filter-grouped"))]
#[bench::filter_not(eval_case("filter-not"))]
#[bench::filter_arithmetic(eval_case("filter-arithmetic"))]
#[bench::filter_function(eval_case("filter-function"))]
#[bench::projection_arithmetic(eval_case("projection-arithmetic"))]
#[bench::projection_function(eval_case("projection-function"))]
#[bench::projection_method_chain(eval_case("projection-method-chain"))]
#[bench::projection_aggregate(eval_case("projection-aggregate"))]
#[bench::projection_keys(eval_case("projection-keys"))]
#[bench::projection_append(eval_case("projection-append"))]
#[bench::projection_nothing(eval_case("projection-nothing"))]
fn eval_path((query, document): EvalInput) -> (Option<Query<'static>>, IValue) {
    // Include result cleanup in the measurement.
    if query.is_projection() {
        black_box(calc_once_projection(black_box(query), black_box(&document)));
        (None, document)
    } else {
        black_box(create(&query).calc(black_box(&document)));
        // Include result cleanup, but leave query and document cleanup outside measurement.
        (Some(query), document)
    }
}

library_benchmark_group!(name = jsonpath; benchmarks = compile_path, eval_path);
main!(
    config = LibraryBenchmarkConfig::default().pass_through_env("JSONPATH_BENCHMARK_PATHS");
    library_benchmark_groups = jsonpath
);
