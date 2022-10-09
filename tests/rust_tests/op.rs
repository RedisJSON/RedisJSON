#[macro_use]
extern crate serde_json;

use common::{read_json, select_and_then_compare, setup};

mod common;

#[test]
fn op_object_eq() {
    setup();

    select_and_then_compare(
        "$.school[?(@ == @)]",
        read_json("./json_examples/data_obj.json"),
        json!([[
            {"id": 0, "name": "Millicent Norman"},
            {"id": 1, "name": "Vincent Cannon" },
            {"id": 2, "name": "Gray Berry"}
        ]]),
    );
}

#[test]
fn op_object_ge() {
    setup();

    select_and_then_compare(
        "$.friends[?(@.id >= 2)]",
        read_json("./json_examples/data_obj.json"),
        json!([
            { "id" : 2, "name" : "Gray Berry" }
        ]),
    );
}

#[test]
fn op_object_or_default() {
    setup();

    select_and_then_compare(
        "$.friends[?(@.id >= 2 || @.id == 1)]",
        read_json("./json_examples/data_obj.json"),
        json!([
            { "id" : 1, "name" : "Vincent Cannon" },
            { "id" : 2, "name" : "Gray Berry" }
        ]),
    );
}

#[test]
fn op_object_and_or() {
    setup();

    select_and_then_compare(
        "$.friends[?( (@.id >= 2 || @.id == 1) && @.id == 0)]",
        read_json("./json_examples/data_obj.json"),
        json!([]),
    );
}

#[test]
fn op_result_type() {
    setup();

    select_and_then_compare(
        "$..friends[?(@.id == $.index)].id",
        read_json("./json_examples/data_obj.json"),
        json!([0, 0]),
    );
}

#[test]
fn op_absolute_path_result_type() {
    setup();

    select_and_then_compare(
        "$..book[?($.store.bicycle.price < @.price)].price",
        read_json("./json_examples/example.json"),
        json!([22.99]),
    );
}

#[test]
fn op_complicated() {
    setup();

    select_and_then_compare(
        "$..book[?( (@.price == 12.99 || @.category == 'reference') && @.price > 10)].price",
        read_json("./json_examples/example.json"),
        json!([12.99]),
    );
}

#[test]
fn op_gt() {
    setup();

    select_and_then_compare(
        "$..[?(@.age > 40)]",
        json!([
            { "name": "이름1", "age": 40, "phone": "+33 12341234" },
            { "name": "이름2", "age": 42, "phone": "++44 12341234" }
        ]),
        json!([
            { "name" : "이름2", "age" : 42, "phone" : "++44 12341234" }
        ]),
    );
}

#[test]
fn op_ge() {
    setup();

    select_and_then_compare(
        "$..[?(@.age >= 30)]",
        json!({
            "school": {
                "friends": [
                    {"name": "친구1", "age": 20},
                    {"name": "친구2", "age": 20}
                ]
            },
            "friends": [
                {"name": "친구3", "age": 30},
                {"name": "친구4"}
        ]}),
        json!([
            { "name" : "친구3", "age" : 30 }
        ]),
    );
}

#[test]
fn op_eq_for_number() {
    setup();

    select_and_then_compare("$.[?(@ == 1)]", json!({ "a": 1 }), json!([1]));
}

#[test]
fn op_ne_for_number() {
    setup();

    select_and_then_compare("$.[?(@ != 2)]", json!({ "a": 1 }), json!([1]));
}

#[test]
fn op_lt_for_number() {
    setup();

    select_and_then_compare("$.[?(@ < 2)]", json!({ "a": 1 }), json!([1]));
}

#[test]
fn op_le_for_number() {
    setup();

    select_and_then_compare("$.[?(@ <= 1)]", json!({ "a": 1 }), json!([1]));
}

#[test]
fn op_gt_for_number() {
    setup();

    select_and_then_compare("$.[?(@ > 0)]", json!({ "a": 1 }), json!([1]));
}

#[test]
fn op_ge_for_number() {
    setup();

    select_and_then_compare("$.[?(@ >= 0)]", json!({ "a": 1 }), json!([1]));
}

#[test]
fn op_eq_for_string_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a == "b")]"#,
        json!([{ "a": "b" }]),
        json!([{ "a": "b" }]),
    );
}

#[test]
fn op_ne_for_string_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a != "c")]"#,
        json!([{ "a": "b" }]),
        json!([{ "a": "b" }]),
    );
}

#[test]
fn op_lt_for_string_value() {
    setup();

    select_and_then_compare(r#"$.[?(@.a < "b")]"#, json!({ "a": "b" }), json!([]));
}

#[test]
fn op_le_for_string_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a <= "b")]"#,
        json!([{ "a": "b" }]),
        json!([{ "a": "b" }]),
    );
}

#[test]
fn op_gt_for_string_value() {
    setup();

    select_and_then_compare(r#"$.[?(@.a > "b")]"#, json!({ "a": "b" }), json!([]));
}

#[test]
fn op_ge_for_string_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a >= "b")]"#,
        json!([{ "a": "b" }]),
        json!([{ "a": "b" }]),
    );
}

#[test]
fn op_eq_for_object_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a == @.c)]"#,
        json!([{"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}]),
        json!([{"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}]),
    );
}

#[test]
fn op_ne_for_object_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a != @.c)]"#,
        json!([{"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}]),
        json!([]),
    );
}

#[test]
fn op_lt_for_object_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a < @.c)]"#,
        json!({"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}),
        json!([]),
    );
}

#[test]
fn op_le_for_object_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a <= @.c)]"#,
        json!({"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}),
        json!([]),
    );
}

#[test]
fn op_gt_for_object_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a > @.c)]"#,
        json!({"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}),
        json!([]),
    );
}

#[test]
fn op_ge_for_object_value() {
    setup();

    select_and_then_compare(
        r#"$.[?(@.a >= @.c)]"#,
        json!({"a": { "1": 1 }, "b": { "2": 2 }, "c": { "1": 1 }}),
        json!([]),
    );
}

#[test]
fn op_eq_for_complex_value() {
    setup();

    select_and_then_compare(r#"$.[?(1 == @.a)]"#, json!({ "a": { "b": 1 } }), json!([]));
}

#[test]
fn op_ne_for_complex_value() {
    setup();

    select_and_then_compare(
        r#"$.[?("1" != @.a)]"#,
        json!([{ "a": { "b": 1 } }]),
        json!([{ "a": { "b": 1 } }]),
    );
}

#[test]
fn op_le_for_complex_value() {
    setup();

    select_and_then_compare(r#"$.[?(@.a <= 1)]"#, json!({ "a": { "b": 1 } }), json!([]));
}

#[test]
fn op_gt_for_complex_value() {
    setup();

    select_and_then_compare(r#"$.[?(@.a > "1")]"#, json!({ "a": { "b": 1 } }), json!([]));
}

#[test]
fn op_compare_different_types() {
    setup();

    for path in [
        r#"$[?("1" == 1)]"#,
        r#"$[?(1 == "1")]"#,
        r#"$[?(true == 1)]"#,
        r#"$[?(@ == 1)]"#,
    ]
    .iter()
    {
        select_and_then_compare(path, json!({}), json!([]));
    }
}

#[test]
fn op_for_same_type() {
    setup();

    select_and_then_compare(
        r#"$..[?(@.a == 1)]"#,
        json!([{
            "a": 1,
            "b" : {"a": 1},
            "c" : {"a": 1}
        }]),
        json!([
            {
                "a": 1,
                "b" : {"a": 1},
                "c" : {"a": 1}
            },
            {"a": 1},
            {"a": 1}
        ]),
    );
}
