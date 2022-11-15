/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::jsonpath::select_value::{SelectValue, SelectValueType};
use ijson::{IValue, ValueType};
use serde_json::Value;

impl SelectValue for Value {
    fn get_type(&self) -> SelectValueType {
        match self {
            Value::Bool(_) => SelectValueType::Bool,
            Value::String(_) => SelectValueType::String,
            Value::Null => SelectValueType::Null,
            Value::Array(_) => SelectValueType::Array,
            Value::Object(_) => SelectValueType::Object,
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    SelectValueType::Long
                } else if n.is_f64() {
                    SelectValueType::Double
                } else {
                    panic!("bad type for Number value");
                }
            }
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        match self {
            Value::Object(o) => o.contains_key(key),
            _ => false,
        }
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a Self> + 'a>> {
        match self {
            Value::Array(arr) => Some(Box::new(arr.iter())),
            Value::Object(o) => Some(Box::new(o.values())),
            _ => None,
        }
    }

    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>> {
        match self {
            Value::Object(o) => Some(Box::new(o.keys().map(|k| &k[..]))),
            _ => None,
        }
    }

    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, &'a Self)> + 'a>> {
        match self {
            Value::Object(o) => Some(Box::new(o.iter().map(|(k, v)| (&k[..], v)))),
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match self {
            Value::Array(arr) => Some(arr.len()),
            Value::Object(obj) => Some(obj.len()),
            _ => None,
        }
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
        match self {
            Value::Object(o) => o.get(key),
            _ => None,
        }
    }

    fn get_index(&self, index: usize) -> Option<&Self> {
        match self {
            Value::Array(arr) => arr.get(index),
            _ => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    fn get_str(&self) -> String {
        match self {
            Value::String(s) => s.to_string(),
            _ => {
                panic!("not a string");
            }
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Value::String(s) => s.as_str(),
            _ => {
                panic!("not a string");
            }
        }
    }

    fn get_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => {
                panic!("not a bool");
            }
        }
    }

    fn get_long(&self) -> i64 {
        match self {
            Value::Number(n) => {
                if let Some(n) = n.as_i64() {
                    n
                } else {
                    panic!("not a long");
                }
            }
            _ => {
                panic!("not a long");
            }
        }
    }

    fn get_double(&self) -> f64 {
        match self {
            Value::Number(n) => {
                if n.is_f64() {
                    n.as_f64().unwrap()
                } else {
                    panic!("not a double");
                }
            }
            _ => {
                panic!("not a double");
            }
        }
    }
}

impl SelectValue for IValue {
    fn get_type(&self) -> SelectValueType {
        match self.type_() {
            ValueType::Bool => SelectValueType::Bool,
            ValueType::String => SelectValueType::String,
            ValueType::Null => SelectValueType::Null,
            ValueType::Array => SelectValueType::Array,
            ValueType::Object => SelectValueType::Object,
            ValueType::Number => {
                let num = self.as_number().unwrap();
                if num.has_decimal_point() {
                    SelectValueType::Double
                } else {
                    SelectValueType::Long
                }
            }
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        if let Some(o) = self.as_object() {
            o.contains_key(key)
        } else {
            false
        }
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a Self> + 'a>> {
        if let Some(arr) = self.as_array() {
            Some(Box::new(arr.iter()))
        } else if let Some(o) = self.as_object() {
            Some(Box::new(o.values()))
        } else {
            None
        }
    }

    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>> {
        match self.as_object() {
            Some(o) => Some(Box::new(o.keys().map(|k| &k[..]))),
            _ => None,
        }
    }

    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, &'a Self)> + 'a>> {
        match self.as_object() {
            Some(o) => Some(Box::new(o.iter().map(|(k, v)| (&k[..], v)))),
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        self.as_array().map_or_else(
            || self.as_object().map(ijson::IObject::len),
            |arr| Some(arr.len()),
        )
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
        match self.as_object() {
            Some(o) => o.get(key),
            _ => None,
        }
    }

    fn get_index(&self, index: usize) -> Option<&Self> {
        match self.as_array() {
            Some(arr) => arr.get(index),
            _ => None,
        }
    }

    fn is_array(&self) -> bool {
        self.is_array()
    }

    fn get_str(&self) -> String {
        match self.as_string() {
            Some(s) => s.to_string(),
            _ => {
                panic!("not a string");
            }
        }
    }

    fn as_str(&self) -> &str {
        match self.as_string() {
            Some(s) => s.as_str(),
            _ => {
                panic!("not a string");
            }
        }
    }

    fn get_bool(&self) -> bool {
        match self.to_bool() {
            Some(b) => b,
            _ => {
                panic!("not a bool");
            }
        }
    }

    fn get_long(&self) -> i64 {
        match self.as_number() {
            Some(n) => {
                if n.has_decimal_point() {
                    panic!("not a long");
                } else {
                    n.to_i64().unwrap()
                }
            }
            _ => {
                panic!("not a number");
            }
        }
    }

    fn get_double(&self) -> f64 {
        match self.as_number() {
            Some(n) => {
                if n.has_decimal_point() {
                    n.to_f64().unwrap()
                } else {
                    panic!("not a double");
                }
            }
            _ => {
                panic!("not a number");
            }
        }
    }
}
