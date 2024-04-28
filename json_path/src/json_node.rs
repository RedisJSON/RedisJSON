// Copyright Redis Ltd. 2016 - present
// Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
// the Server Side Public License v1 (SSPLv1).

use crate::select_value::{SelectValue, SelectValueType};
use serde_json::Value;
use std::borrow::Cow;

// impl<'t, T> SelectValue for &'t T
// where
//     T: SelectValue,
//     <T as SelectValue>::Item: SelectValue,
//     T: From<<T as SelectValue>::Item>,
//     // &'a T: std::default::Default,
// {
//     type Item = T::Item;

//     fn get_type(&self) -> SelectValueType {
//         (*self).get_type()
//     }

//     fn contains_key(&self, key: &str) -> bool {
//         (*self).contains_key(key)
//     }

//     // fn values(&self) -> Option<Box<dyn Iterator<Item = Cow<Self::Item>>>> {
//     fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = Cow<'a, Self::Item>> + 'a>> {
//         (*self).values()
//     }

//     fn keys(&self) -> Option<impl Iterator<Item = &str>> {
//         (*self).keys()
//     }

//     // fn items(&self) -> Option<impl Iterator<Item = (&str, Cow<Self::Item>)>> {
//     fn items<'a>(
//         &'a self,
//     ) -> Option<Box<dyn Iterator<Item = (&'a str, Cow<'a, Self::Item>)> + 'a>> {
//         None
//         // (*self).items()
//     }

//     fn len(&self) -> Option<usize> {
//         (*self).len()
//     }

//     fn get_key(&self, key: &str) -> Option<Cow<Self::Item>> {
//         (*self).get_key(key)
//     }

//     fn get_index(&self, index: usize) -> Option<Cow<Self::Item>> {
//         (*self).get_index(index)
//     }

//     fn is_empty(&self) -> Option<bool> {
//         (*self).is_empty()
//     }

//     fn is_array(&self) -> bool {
//         (*self).is_array()
//     }

//     unsafe fn get_bool(&self) -> bool {
//         (*self).get_bool()
//     }

//     unsafe fn get_long(&self) -> i64 {
//         (*self).get_long()
//     }

//     unsafe fn get_double(&self) -> f64 {
//         (*self).get_double()
//     }

//     unsafe fn get_str(&self) -> String {
//         (*self).get_str()
//     }

//     unsafe fn as_str(&self) -> &str {
//         (*self).as_str()
//     }
// }

// impl<'a, T> SelectValue for Cow<'a, T>
// where
//     // T: SelectValue + Borrow<T>,
//     // <T as SelectValue>::Item: Borrow<T>,
//     T: SelectValue,
//     // <T as SelectValue>::Item,
// {
//     type Item = T::Item;
//     // type Item = T;

//     fn get_type(&self) -> SelectValueType {
//         self.as_ref().get_type()
//     }

//     fn contains_key(&self, key: &str) -> bool {
//         self.contains_key(key)
//     }

//     fn values(&self) -> Option<Box<dyn Iterator<Item = Cow<Self::Item>>>> {
//         None
//         // self.as_ref().values().map(|i| i.map(|e| Cow::Borrowed(e.as_ref())))
//     }

//     unsafe fn as_str(&self) -> &str {
//         self.as_ref().as_str()
//     }

//     unsafe fn get_bool(&self) -> bool {
//         self.as_ref().get_bool()
//     }

//     unsafe fn get_long(&self) -> i64 {
//         self.as_ref().get_long()
//     }

//     unsafe fn get_double(&self) -> f64 {
//         self.as_ref().get_double()
//     }

//     fn keys(&self) -> Option<impl Iterator<Item = &str>> {
//         self.as_ref().keys()
//     }

//     fn items(&self) -> Option<impl Iterator<Item = (&str, Cow<Self::Item>)>> {
//         None
//         // self.as_ref().items()
//     }

//     fn len(&self) -> Option<usize> {
//         self.as_ref().len()
//     }

//     fn get_index(&self, index: usize) -> Option<Cow<Self::Item>> {
//         self.as_ref().get_index(index)
//     }

//     fn get_key(&self, key: &str) -> Option<Cow<Self::Item>> {
//         self.as_ref().get_key(key)
//     }

//     fn is_empty(&self) -> Option<bool> {
//         self.as_ref().is_empty()
//     }

//     fn is_array(&self) -> bool {
//         self.as_ref().is_array()
//     }

//     unsafe fn get_str(&self) -> String {
//         self.as_ref().get_str()
//     }
// }

impl SelectValue for Value {
    type Item = Self;

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

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = Cow<'a, Self::Item>> + 'a>> {
        match self {
            Self::Array(arr) => Some(Box::new(arr.iter().map(Cow::Borrowed))),
            Self::Object(o) => Some(Box::new(o.values().map(Cow::Borrowed))),
            _ => None,
        }
    }

    fn keys(&self) -> std::option::Option<impl std::iter::Iterator<Item = &str>> {
        match self {
            Self::Object(o) => Some(o.keys().map(|k| &k[..])),
            _ => None,
        }
    }

    fn items(
        &self,
    ) -> std::option::Option<impl std::iter::Iterator<Item = (&str, Cow<Self::Item>)>> {
        // fn items<'a>(
        //     &'a self,
        // ) -> Option<Box<dyn Iterator<Item = (&'a str, Cow<'a, Self::Item>)> + 'a>> {
        match self {
            Self::Object(o) => Some(o.iter().map(|(k, v)| (&k[..], Cow::Borrowed(v)))),
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

    fn get_key(&self, key: &str) -> Option<Cow<Self::Item>> {
        match self {
            Self::Object(o) => o.get(key).map(Cow::Borrowed),
            _ => None,
        }
    }

    fn get_index(&self, index: usize) -> Option<Cow<Self::Item>> {
        match self {
            Self::Array(arr) => arr.get(index).map(Cow::Borrowed),
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
            Self::Number(n) => {
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

    unsafe fn get_double(&self) -> f64 {
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

// impl<Allocator> SelectValue for json_value::Value<Allocator>
// where
//     Allocator: redis_custom_allocator::CustomAllocator,
//     <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
// {
//     type Item = Self;

//     fn get_type(&self) -> SelectValueType {
//         match self {
//             Self::Bool(_) => SelectValueType::Bool,
//             Self::String(_) => SelectValueType::String,
//             Self::Null => SelectValueType::Null,
//             Self::Array(_) => SelectValueType::Array,
//             Self::Object(_) => SelectValueType::Object,
//             Self::Number(n) => match n {
//                 json_value::JsonNumber::Unsigned(_) | json_value::JsonNumber::Signed(_) => {
//                     SelectValueType::Long
//                 }
//                 json_value::JsonNumber::Double(_) => SelectValueType::Double,
//             },
//         }
//     }

//     fn contains_key(&self, key: &str) -> bool {
//         match self {
//             Self::Object(o) => o.contains_key(key),
//             _ => false,
//         }
//     }

//     fn values(&self) -> Option<Box<dyn Iterator<Item = Self>>> {
//         match self {
//             Self::Array(arr) => Some(Box::new(arr.iter().map(|v| json_value::Value::from(v)))),
//             Self::Object(o) => Some(Box::new(o.values())),
//             _ => None,
//         }
//     }

//     fn keys(&self) -> std::option::Option<impl std::iter::Iterator<Item = &str>> {
//         match self {
//             Self::Object(o) => Some(o.keys().map(|k| &k[..])),
//             _ => None,
//         }
//     }

//     fn items(&self) -> Option<impl Iterator<Item = (&str, &Self)>> {
//         match self {
//             Self::Object(o) => Some(o.iter().map(|(k, v)| (&k[..], v))),
//             _ => None,
//         }
//     }

//     fn len(&self) -> Option<usize> {
//         match self {
//             Self::Array(arr) => Some(arr.len()),
//             Self::Object(obj) => Some(obj.len()),
//             _ => None,
//         }
//     }

//     fn is_empty(&self) -> Option<bool> {
//         match self {
//             Self::Array(arr) => Some(arr.is_empty()),
//             Self::Object(obj) => Some(obj.is_empty()),
//             _ => None,
//         }
//     }

//     fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self> {
//         match self {
//             Self::Object(o) => o.get(key),
//             _ => None,
//         }
//     }

//     fn get_index(&self, index: usize) -> Option<&Self> {
//         match self {
//             Self::Array(arr) => arr.get(index),
//             _ => None,
//         }
//     }

//     fn is_array(&self) -> bool {
//         matches!(self, Self::Array(_))
//     }

//     fn get_str(&self) -> String {
//         match self {
//             Self::String(s) => s.to_string(),
//             _ => {
//                 panic!("not a string");
//             }
//         }
//     }

//     fn as_str(&self) -> &str {
//         match self {
//             Self::String(s) => s.as_str(),
//             _ => {
//                 panic!("not a string");
//             }
//         }
//     }

//     fn get_bool(&self) -> bool {
//         match self {
//             Self::Bool(b) => *b,
//             _ => {
//                 panic!("not a bool");
//             }
//         }
//     }

//     fn get_long(&self) -> i64 {
//         match self {
//             Self::Number(n) => n.get_signed().expect("A signed number"),
//             _ => panic!("not a long"),
//         }
//     }

//     fn get_double(&self) -> f64 {
//         match self {
//             Self::Number(n) => n.get_double().expect("A signed number"),
//             _ => panic!("not a double"),
//         }
//     }
// }
