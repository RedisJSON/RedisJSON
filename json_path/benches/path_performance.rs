/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

//! Run with `cargo bench -p json_path --bench path_performance`.
//! The original runner used this timing policy:
//! Compilation and evaluation are timed separately. Each result is the median of
//! three samples, each lasting at least 50 ms; fixture construction is excluded.
//! Criterion now controls sampling and reports; compilation, evaluation, and
//! fixture construction remain separate.

use std::fs::OpenOptions;
use std::hint::black_box;
use std::io::Write;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ijson::IValue;
use json_path::{calc_once_projection, compile, create};
use serde_json::{json, Value};

// Record the exact generated path outside the timed closure for CI summaries.
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

fn evaluate(c: &mut Criterion, name: &str, path: &str, document: &Value, expected: &Value) {
    record_path(name, path);
    let document: IValue = serde_json::from_value(document.clone()).unwrap();
    let query = compile(path).unwrap();
    if query.is_projection() {
        let expected: Vec<Value> = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(
            calc_once_projection(query.clone(), &document),
            expected,
            "{name}: {path}"
        );
        c.bench_function(name, |b| {
            // The projection API consumes its query; clone it outside the timed routine.
            // Drop results inside timing, like the path benchmarks, instead of retaining batches.
            b.iter_batched(
                || query.clone(),
                |query| {
                    black_box(calc_once_projection(black_box(query), black_box(&document)));
                },
                BatchSize::SmallInput,
            );
        });
        return;
    }
    let expected: Vec<IValue> = serde_json::from_value(expected.clone()).unwrap();
    let calculator = create(&query);
    let actual: Vec<IValue> = calculator
        .calc(&document)
        .iter()
        .map(|v| v.inner_cloned())
        .collect();
    assert_eq!(actual, expected, "{name}: {path}");
    c.bench_function(name, |b| {
        b.iter(|| black_box(calculator.calc(black_box(&document))));
    });
}

fn compile_and_evaluate(
    c: &mut Criterion,
    name: &str,
    path: &str,
    document: &Value,
    expected: &Value,
) {
    record_path(&format!("compile/{name}"), path);
    c.bench_function(&format!("compile/{name}"), |b| {
        b.iter(|| black_box(compile(black_box(path)).unwrap()));
    });
    evaluate(c, &format!("eval/{name}"), path, document, expected);
}

fn path_forms(c: &mut Criterion) {
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
    for (name, path, expected) in [
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
        compile_and_evaluate(c, name, path, &document, &expected);
    }
}

fn tree(depth: usize) -> Value {
    if depth == 0 {
        return json!({"uid": 1, "name": "leaf"});
    }
    let children: Vec<_> = (0..4).map(|_| tree(depth - 1)).collect();
    json!({"uid": 1, "children": children, "name": "branch"})
}

fn path_performance(c: &mut Criterion) {
    for (name, path) in [
        ("compile/simple", "$.rows[0].score"),
        ("compile/filter", "$.rows[?@.score > $.threshold]"),
    ] {
        assert!(compile(path).is_ok());
        record_path(name, path);
        c.bench_function(name, |b| {
            b.iter(|| black_box(compile(black_box(path)).unwrap()));
        });
    }
    for depth in [7, 8, 9] {
        let path = format!(
            "$.a{}@.flag{}",
            "[?@.a".repeat(depth - 1) + "[?",
            "]".repeat(depth)
        );
        assert!(compile(&path).is_ok(), "{path}");
        record_path(&format!("compile/nested-{depth}"), &path);
        c.bench_function(&format!("compile/nested-{depth}"), |b| {
            b.iter(|| black_box(compile(black_box(&path)).unwrap()));
        });
    }
    for (name, before, after) in [
        ("compile/nested-grouped-9", "(", ")"),
        ("compile/nested-comparison-9", "(", " > 0)"),
        ("compile/nested-arithmetic-9", "(", ") > 0"),
    ] {
        let mut inner = "@.flag".to_owned();
        for _ in 0..8 {
            inner = format!("@.a[?{before}{inner}{after}]");
        }
        let path = format!("$.a[?{before}{inner}{after}]");
        assert!(compile(&path).is_ok(), "{path}");
        record_path(name, &path);
        c.bench_function(name, |b| {
            b.iter(|| black_box(compile(black_box(&path)).unwrap()));
        });
    }

    let document = tree(4);
    evaluate(
        c,
        "eval/recursive-objects",
        "$..uid",
        &document,
        &json!(vec![1; 341]),
    );
    evaluate(
        c,
        "eval/recursive-no-match",
        "$..absent",
        &document,
        &json!([]),
    );

    let rows: Vec<_> = (0..128)
        .map(|uid| json!({"uid": uid, "flag": false, "children": document}))
        .collect();
    let existence = json!({"rows": rows});
    evaluate(
        c,
        "eval/existence-early-match",
        "$.rows[?@..flag].uid",
        &existence,
        &json!((0..128).collect::<Vec<_>>()),
    );
    evaluate(
        c,
        "eval/existence-no-match",
        "$.rows[?@..absent].uid",
        &existence,
        &json!([]),
    );

    let rows: Vec<_> = (0..256)
        .map(|uid| json!({"uid": uid, "score": uid}))
        .collect();
    let document =
        json!({"rows": rows, "threshold": 128, "thresholds": vec![json!({"limit": 1000}); 128]});
    evaluate(
        c,
        "eval/root-scalar",
        "$.rows[?@.score > $.threshold].uid",
        &document,
        &json!((129..256).collect::<Vec<_>>()),
    );
    evaluate(
        c,
        "eval/root-single-candidate",
        "$.rows[?@.score > $.threshold].uid",
        &json!({"threshold": 128, "rows": [{"score": 129, "uid": 129}]}),
        &json!([129]),
    );
    evaluate(
        c,
        "eval/root-existence",
        "$.rows[?$.thresholds..limit].uid",
        &document,
        &json!((0..256).collect::<Vec<_>>()),
    );
    evaluate(
        c,
        "eval/root-existence-missing",
        "$.rows[?$.thresholds..absent].uid",
        &document,
        &json!([]),
    );
    evaluate(
        c,
        "eval/root-descendant-list",
        "$.rows[?@.score > $.thresholds..limit].uid",
        &document,
        &json!([]),
    );
    evaluate(
        c,
        "eval/root-descendant-sum",
        "$.rows[?@.score > $.thresholds..limit.sum()].uid",
        &document,
        &json!([]),
    );
    evaluate(
        c,
        "eval/root-descendant-list-large",
        "$.rows[?@.score > $.thresholds..limit].uid",
        &json!({"rows": rows, "thresholds": vec![json!({"limit": 1000}); 2048]}),
        &json!([]),
    );
    evaluate(
        c,
        "eval/root-many-operands",
        &format!("$.rows[?{}].uid", ["$.threshold"; 65].join(" && ")),
        &document,
        &json!((0..256).collect::<Vec<_>>()),
    );
    evaluate(c, "eval/simple", "$.rows[0].score", &document, &json!([0]));
    evaluate(
        c,
        "eval/small-filter",
        "$[?@.score > 1].score",
        &json!([{"score": 0}, {"score": 2}]),
        &json!([2]),
    );

    let prefix = "long-prefix-".repeat(16);
    let allowed: Vec<_> = (0..16).map(|i| format!("{prefix}{i}")).collect();
    let rows: Vec<_> = (0..256).map(|i| format!("{prefix}{}", i % 32)).collect();
    let expected: Vec<_> = rows
        .iter()
        .filter(|s| allowed.contains(s))
        .cloned()
        .collect();
    evaluate(
        c,
        "eval/string-membership",
        &format!("$[?@ in {}]", json!(allowed)),
        &json!(rows),
        &json!(expected),
    );
    let value = json!({"strings": [prefix, "tail"]});
    evaluate(
        c,
        "eval/deep-string-equality",
        &format!("$[?@ == {value}]"),
        &json!(vec![value.clone(); 256]),
        &json!(vec![value; 256]),
    );

    let rows: Vec<_> = (0..1024)
        .map(|uid| json!({"uid": uid, "name": format!("customer-{uid}-active")}))
        .collect();
    let document = json!({"rows": rows});
    let expected = json!((0..1024).collect::<Vec<_>>());
    evaluate(
        c,
        "eval/regex-search-cache",
        r#"$.rows[?@.name =~ "(?:customer|operator)-[0-9]+-active"].uid"#,
        &document,
        &expected,
    );
    evaluate(
        c,
        "eval/regex-match-cache",
        r#"$.rows[?match(@.name, "(?:customer|operator)-[0-9]+-active")].uid"#,
        &document,
        &expected,
    );
    evaluate(
        c,
        "eval/regex-search-function",
        r#"$.rows[?search(@.name, "(?:customer|operator)-[0-9]+-active")].uid"#,
        &document,
        &expected,
    );
}

criterion_group!(benches, path_performance, path_forms);
criterion_main!(benches);
