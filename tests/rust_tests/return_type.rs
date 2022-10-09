#[macro_use]
extern crate serde_json;

use common::{read_json, select_and_then_compare, setup};

mod common;

#[test]
fn return_type_for_single_object() {
    setup();

    select_and_then_compare(
        "$.school",
        read_json("./json_examples/data_obj.json"),
        json!([{
            "friends": [
                {"id": 0, "name": "Millicent Norman"},
                {"id": 1, "name": "Vincent Cannon" },
                {"id": 2, "name": "Gray Berry"}
            ]
        }]),
    );
}

#[test]
fn return_type_for_single_object_key_matched() {
    setup();

    select_and_then_compare(
        "$.friends[?(@.name)]",
        read_json("./json_examples/data_obj.json"),
        json!([
            { "id" : 1, "name" : "Vincent Cannon" },
            { "id" : 2, "name" : "Gray Berry" }
        ]),
    );
}

#[test]
fn return_type_for_child_object_matched() {
    setup();

    select_and_then_compare(
        "$.school[?(@[0])]",
        read_json("./json_examples/data_obj.json"),
        json!([[
                {"id": 0, "name": "Millicent Norman"},
                {"id": 1, "name": "Vincent Cannon" },
                {"id": 2, "name": "Gray Berry"}
        ]]),
    );
}

#[test]
fn return_type_for_child_object_not_matched() {
    setup();

    select_and_then_compare(
        "$.school[?(@.friends[10])]",
        read_json("./json_examples/data_obj.json"),
        json!([]),
    );
}

#[test]
fn return_type_for_object_filter_true() {
    setup();

    select_and_then_compare(
        "$.school[?(1==1)]",
        read_json("./json_examples/data_obj.json"),
        json!([[
            {"id": 0, "name": "Millicent Norman"},
            {"id": 1, "name": "Vincent Cannon" },
            {"id": 2, "name": "Gray Berry"}
        ]]),
    );
}

#[test]
fn return_type_for_array_filter_true() {
    setup();

    select_and_then_compare(
        "$.school.friends[?(1==1)]",
        read_json("./json_examples/data_obj.json"),
        json!([
            {"id": 0, "name": "Millicent Norman"},
            {"id": 1, "name": "Vincent Cannon" },
            {"id": 2, "name": "Gray Berry"}
        ]),
    );
}

#[test]
fn return_type_empty() {
    setup();

    select_and_then_compare("$[?(@.key==43)]", json!([{"key": 42}]), json!([]));
}
