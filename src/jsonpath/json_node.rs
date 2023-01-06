/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::error::Error;
use crate::jsonpath::select_value::{SelectValue, SelectValueType};
use ijson::{IValue, ValueType};
use serde_json::Value;

impl SelectValue for Value {
    fn get_type(&self) -> SelectValueType {
        match self {
            Self::Bool(_) => SelectValueType::Bool,
            Self::String(_) => SelectValueType::String,
            Self::Null => SelectValueType::Null,
            Self::Array(_) => SelectValueType::Array,
            Self::Object(_) => SelectValueType::Object,
            Self::Number(n) => {
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
            Self::Object(o) => o.contains_key(key),
            _ => false,
        }
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a Self> + 'a>> {
        match self {
            Self::Array(arr) => Some(Box::new(arr.iter())),
            Self::Object(o) => Some(Box::new(o.values())),
            _ => None,
        }
    }

    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>> {
        match self {
            Self::Object(o) => Some(Box::new(o.keys().map(|k| &k[..]))),
            _ => None,
        }
    }

    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, &'a Self)> + 'a>> {
        match self {
            Self::Object(o) => Some(Box::new(o.iter().map(|(k, v)| (&k[..], v)))),
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match self {
            Self::Array(arr) => Some(arr.len()),
            Self::Object(obj) => Some(obj.len()),
            _ => None,
        }
    }

    fn is_empty(&self) -> Option<bool> {
        match self {
            Self::Array(arr) => Some(arr.is_empty()),
            Self::Object(obj) => Some(obj.is_empty()),
            _ => None,
        }
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
        match self {
            Self::Object(o) => o.get(key),
            _ => None,
        }
    }

    fn get_index(&self, index: usize) -> Option<&Self> {
        match self {
            Self::Array(arr) => arr.get(index),
            _ => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    fn get_str(&self) -> String {
        match self {
            Self::String(s) => s.to_string(),
            _ => {
                panic!("not a string");
            }
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::String(s) => s.as_str(),
            _ => {
                panic!("not a string");
            }
        }
    }

    fn get_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            _ => {
                panic!("not a bool");
            }
        }
    }

    fn get_long(&self) -> Result<i64, Error> {
        match self {
            Self::Number(n) => {
                if let Some(n) = n.as_i64() {
                    Ok(n)
                } else {
                    Err(Error::from("not a long"))
                }
            }
            _ => {
                panic!("long is not a number");
            }
        }
    }

    fn get_ulong(&self) -> Result<u64, Error> {
        match self {
            Self::Number(n) => {
                if let Some(n) = n.as_u64() {
                    Ok(n)
                } else {
                    Err(Error::from("not a ulong"))
                }
            }
            _ => {
                panic!("ulong is not a number");
            }
        }
    }

    fn get_double(&self) -> f64 {
        match self {
            Self::Number(n) => {
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
        self.as_object().map_or(false, |o| o.contains_key(key))
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
        self.as_object()
            .map_or(None, |o| Some(Box::new(o.keys().map(|k| &k[..]))))
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

    fn is_empty(&self) -> Option<bool> {
        self.is_empty()
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
        self.as_object().and_then(|o| o.get(key))
    }

    fn get_index(&self, index: usize) -> Option<&Self> {
        self.as_array().and_then(|arr| arr.get(index))
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

    fn get_long(&self) -> Result<i64, Error> {
        match self.as_number() {
            Some(n) => {
                if n.has_decimal_point() {
                    panic!("not a long");
                } else {
                    if let Some(n) = n.to_i64() {
                        Ok(n)
                    } else {
                        Err(Error::from("not a long"))
                    }
                }
            }
            _ => {
                panic!("long is not a number");
            }
        }
    }

    fn get_ulong(&self) -> Result<u64, Error> {
        match self.as_number() {
            Some(n) => {
                if n.has_decimal_point() {
                    panic!("not a long");
                } else {
                    if let Some(n) = n.to_u64() {
                        Ok(n)
                    } else {
                        Err(Error::from("not a ulong"))
                    }
                }
            }
            _ => {
                panic!("ulong is not a number");
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
