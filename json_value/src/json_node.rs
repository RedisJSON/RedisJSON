use std::{alloc::Layout, borrow::Cow};

// /*
//  * Copyright Redis Ltd. 2016 - present
//  * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
//  * the Server Side Public License v1 (SSPLv1).
//  */
use crate::value::Value;
use crate::JsonNumber;
use json_path::select_value::{SelectValue, SelectValueType};
use listpack_redis::{allocator::ListpackAllocator, ListpackEntry};
use serde::de::value;
// use ijson::{IValue, ValueType};

// impl SelectValue for IValue {
//     fn get_type(&self) -> SelectValueType {
//         match self.type_() {
//             ValueType::Bool => SelectValueType::Bool,
//             ValueType::String => SelectValueType::String,
//             ValueType::Null => SelectValueType::Null,
//             ValueType::Array => SelectValueType::Array,
//             ValueType::Object => SelectValueType::Object,
//             ValueType::Number => {
//                 let num = self.as_number().unwrap();
//                 if num.has_decimal_point() {
//                     SelectValueType::Double
//                 } else {
//                     SelectValueType::Long
//                 }
//             }
//         }
//     }

//     fn contains_key(&self, key: &str) -> bool {
//         self.as_object().map_or(false, |o| o.contains_key(key))
//     }

//     fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a Self> + 'a>> {
//         if let Some(arr) = self.as_array() {
//             Some(Box::new(arr.iter()))
//         } else if let Some(o) = self.as_object() {
//             Some(Box::new(o.values()))
//         } else {
//             None
//         }
//     }

//     fn keys(&self) -> Option<impl Iterator<Item = &str>> {
//         self.as_object().map(|o| o.keys().map(|k| &k[..]))
//     }

//     fn items(&self) -> Option<impl Iterator<Item = (&str, &Self)>> {
//         self.as_object().map(|o| o.iter().map(|(k, v)| (&k[..], v)))
//     }

//     fn len(&self) -> Option<usize> {
//         self.as_array().map_or_else(
//             || self.as_object().map(ijson::IObject::len),
//             |arr| Some(arr.len()),
//         )
//     }

//     fn is_empty(&self) -> Option<bool> {
//         self.is_empty()
//     }

//     fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
//         self.as_object().and_then(|o| o.get(key))
//     }

//     fn get_index(&self, index: usize) -> Option<&Self> {
//         self.as_array().and_then(|arr| arr.get(index))
//     }

//     fn is_array(&self) -> bool {
//         self.is_array()
//     }

//     fn get_str(&self) -> String {
//         self.as_string().expect("not a string").to_string()
//     }

//     fn as_str(&self) -> &str {
//         self.as_string().expect("not a string").as_str()
//     }

//     fn get_bool(&self) -> bool {
//         self.to_bool().expect("not a bool")
//     }

//     fn get_long(&self) -> i64 {
//         let n = self.as_number().expect("not a number");
//         if n.has_decimal_point() {
//             panic!("not a long");
//         } else {
//             n.to_i64().unwrap()
//         }
//     }

//     fn get_double(&self) -> f64 {
//         let n = self.as_number().expect("not a number");
//         if n.has_decimal_point() {
//             n.to_f64().unwrap()
//         } else {
//             panic!("not a double");
//         }
//     }
// }

#[derive(Debug)]
pub enum LazyValueProducer<Allocator: ListpackAllocator>
where
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    /// Creates a new `Value` from a `ListpackEntry`.
    ArrayEntry(ListpackEntry),
    /// Does not create a new `Value`, but returns the object as-is.
    Value(Value<Allocator>),
}

impl<Allocator> LazyValueProducer<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    pub fn produce(&self) -> Value<Allocator> {
        match self {
            Self::ArrayEntry(entry) => Value::from(*entry),
            Self::Value(value) => value.clone(),
        }
    }
}

impl<Allocator> From<LazyValueProducer<Allocator>> for Value<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    fn from(producer: LazyValueProducer<Allocator>) -> Self {
        producer.produce()
    }
}

impl<Allocator> From<ListpackEntry> for LazyValueProducer<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    fn from(entry: ListpackEntry) -> Self {
        Self::ArrayEntry(entry)
    }
}

impl<'a, Allocator> From<Cow<'a, Value<Allocator>>> for LazyValueProducer<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    fn from(value: Cow<'a, Value<Allocator>>) -> Self {
        Self::Value(value.into_owned())
    }
}

impl<Allocator> From<Value<Allocator>> for LazyValueProducer<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    fn from(value: Value<Allocator>) -> Self {
        Self::Value(value)
    }
}

// impl<Allocator> SelectValue for LazyValueProducer<Allocator>
// where
//     Allocator: ListpackAllocator + Eq,
//     <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
// {
//     type Item = Value<Allocator>;

//     fn get_type(&self) -> SelectValueType {
//         match self {
//             Self::ArrayEntry(_) => SelectValueType::Array,
//             Self::Value(value) => value.get_type(),
//         }
//     }

//     fn contains_key(&self, key: &str) -> bool {
//         match self {
//             Self::ArrayEntry(_) => false,
//             Self::Value(value) => value.contains_key(key),
//         }
//     }

//     fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = Cow<'a, Self::Item>> + 'a>> {
//         match self {
//             Self::ArrayEntry(e) => {
//                 let value = Value::from(e);
//                 if let Value::Array(array) = value {
//                     Some(Box::new(
//                         array.iter().map(|v| Cow::Owned(LazyValueProducer::from(v))),
//                     ))
//                 } else {
//                     None
//                 }
//             }
//             Self::Value(value) => value.values(),
//         }
//     }

//     fn keys(&self) -> Option<impl Iterator<Item = &str>> {
//         match self {
//             Self::ArrayEntry(_) => None,
//             Self::Value(value) => value.keys(),
//         }
//     }

//     fn items(&self) -> Option<impl Iterator<Item = (&str, &Self::Item)>> {
//         match self {
//             Self::ArrayEntry(_) => None,
//             Self::Value(value) => value.items(),
//         }
//     }

//     fn len(&self) -> Option<usize> {
//         match self {
//             Self::ArrayEntry(e) => {
//                 let value = Value::from(e);
//                 if let Value::Array(array) = value {
//                     Some(array.len())
//                 } else {
//                     None
//                 }
//             }
//             Self::Value(value) => value.len(),
//         }
//     }
// }

impl<Allocator> SelectValue for Value<Allocator>
where
    Allocator: listpack_redis::allocator::ListpackAllocator + Eq,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    type Item = LazyValueProducer<Allocator>;
    // type Item = Self;

    fn get_type(&self) -> SelectValueType {
        match self {
            Self::Bool(_) => SelectValueType::Bool,
            Self::String(_) => SelectValueType::String,
            Self::Null => SelectValueType::Null,
            Self::Array(_) => SelectValueType::Array,
            Self::Object(_) => SelectValueType::Object,
            Self::Number(n) => match n {
                JsonNumber::Unsigned(_) | JsonNumber::Signed(_) => SelectValueType::Long,
                JsonNumber::Double(_) => SelectValueType::Double,
            },
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        match self {
            Self::Object(o) => o.contains_key(key),
            _ => false,
        }
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = Cow<'a, Self::Item>> + 'a>> {
        match self {
            Self::Array(arr) => Some(Box::new(
                arr.iter().map(|v| Cow::Owned(LazyValueProducer::from(v))),
            )),
            Self::Object(o) => Some(Box::new(o.values())),
            _ => None,
        }
    }

    fn keys(&self) -> std::option::Option<impl std::iter::Iterator<Item = &str>> {
        match self {
            Self::Object(o) => Some(o.keys().map(|k| &k[..])),
            _ => None,
        }
    }

    fn items(&self) -> Option<impl Iterator<Item = (&str, &Self)>> {
        match self {
            Self::Object(o) => Some(o.iter().map(|(k, v)| (&k[..], v))),
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

    unsafe fn get_str(&self) -> String {
        match self {
            Self::String(s) => s.to_string(),
            _ => {
                panic!("not a string");
            }
        }
    }

    unsafe fn as_str(&self) -> &str {
        match self {
            Self::String(s) => s.as_str(),
            _ => {
                panic!("not a string");
            }
        }
    }

    unsafe fn get_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            _ => {
                panic!("not a bool");
            }
        }
    }

    unsafe fn get_long(&self) -> i64 {
        match self {
            Self::Number(n) => n.get_signed().expect("A signed number"),
            _ => panic!("not a long"),
        }
    }

    unsafe fn get_double(&self) -> f64 {
        match self {
            Self::Number(n) => n.get_double().expect("A signed number"),
            _ => panic!("not a double"),
        }
    }
}
