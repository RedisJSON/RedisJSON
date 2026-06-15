/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use serde::{Serialize, Serializer};
use std::{ffi::c_void, fmt::Debug, ptr::null};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SelectValueType {
    Null,
    Bool,
    Long,
    Double,
    String,
    Array,
    Object,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValueRef<'a, T: SelectValue> {
    Borrowed(&'a T),
    Owned(T),
}

impl<'a, T: SelectValue> Serialize for ValueRef<'a, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().serialize(serializer)
    }
}

impl<'a, T: SelectValue> ValueRef<'a, T> {
    pub fn inner_cloned(&self) -> T {
        match self {
            ValueRef::Borrowed(t) => (*t).clone(),
            ValueRef::Owned(t) => t.clone(),
        }
    }
}

impl<'a, T: SelectValue> AsRef<T> for ValueRef<'a, T> {
    fn as_ref(&self) -> &T {
        match self {
            ValueRef::Borrowed(t) => t,
            ValueRef::Owned(t) => t,
        }
    }
}

impl<'a, T: SelectValue> std::ops::Deref for ValueRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<'a, T: SelectValue> PartialEq<&T> for ValueRef<'a, T> {
    fn eq(&self, other: &&T) -> bool {
        is_equal(self.as_ref(), *other)
    }
}

#[repr(C)]
#[allow(unused)]
#[derive(Debug, PartialEq, Eq)]
pub enum JSONArrayType {
    Heterogeneous = 0,
    I8 = 1,
    U8 = 2,
    I16 = 3,
    U16 = 4,
    F16 = 5,
    BF16 = 6,
    I32 = 7,
    U32 = 8,
    F32 = 9,
    I64 = 10,
    U64 = 11,
    F64 = 12,
}

#[allow(unused)]
pub trait SelectValue: Debug + Eq + PartialEq + Default + Clone + Serialize {
    fn get_type(&self) -> SelectValueType;
    fn contains_key(&self, key: &str) -> bool;
    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = ValueRef<'a, Self>> + 'a>>;
    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>>;
    #[allow(clippy::type_complexity)]
    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, ValueRef<'a, Self>)> + 'a>>;
    fn len(&self) -> Option<usize>;
    fn is_empty(&self) -> Option<bool>;
    fn get_key<'a>(&'a self, key: &str) -> Option<ValueRef<'a, Self>>;
    fn get_index<'a>(&'a self, index: usize) -> Option<ValueRef<'a, Self>>;
    fn is_array(&self) -> bool;
    fn is_double(&self) -> Option<bool>;

    fn get_str(&self) -> Option<String>;
    fn as_str(&self) -> Option<&str>;
    fn get_bool(&self) -> Option<bool>;
    fn get_long(&self) -> Option<i64>;
    fn get_double(&self) -> Option<f64>;
    fn get_array(&self) -> *const c_void;
    fn get_array_type(&self) -> Option<JSONArrayType>;

    fn calculate_value_depth(&self) -> usize {
        match self.get_type() {
            SelectValueType::String
            | SelectValueType::Bool
            | SelectValueType::Long
            | SelectValueType::Null
            | SelectValueType::Double => 0,
            SelectValueType::Array => {
                1 + self
                    .values()
                    .map(|vals| vals.map(|v| v.calculate_value_depth()).max().unwrap_or(0))
                    .unwrap_or(0)
            }
            SelectValueType::Object => {
                1 + self
                    .keys()
                    .map(|keys| {
                        keys.map(|k| {
                            let v = self.get_key(k);
                            v.map(|v| v.calculate_value_depth()).unwrap_or(0)
                        })
                        .max()
                        .unwrap_or(0)
                    })
                    .unwrap_or(0)
            }
        }
    }
}

pub fn is_equal<T1: SelectValue, T2: SelectValue>(a: &T1, b: &T2) -> bool {
    a.get_type() == b.get_type()
        && match a.get_type() {
            SelectValueType::Null => true,
            SelectValueType::Bool => a.get_bool().zip(b.get_bool()).is_some_and(|(x, y)| x == y),
            SelectValueType::Long => a.get_long().zip(b.get_long()).is_some_and(|(x, y)| x == y),
            SelectValueType::Double => a
                .get_double()
                .zip(b.get_double())
                .is_some_and(|(x, y)| x == y),
            SelectValueType::String => a.get_str().zip(b.get_str()).is_some_and(|(x, y)| x == y),
            SelectValueType::Array => match (a.len(), b.len()) {
                (Some(alen), Some(blen)) if alen == blen => match (a.values(), b.values()) {
                    (Some(ait), Some(bit)) => {
                        ait.zip(bit).all(|(a, b)| is_equal(a.as_ref(), b.as_ref()))
                    }
                    _ => false,
                },
                _ => false,
            },
            SelectValueType::Object => match (a.len(), b.len()) {
                (Some(alen), Some(blen)) if alen == blen => a.keys().is_some_and(|mut keys| {
                    keys.all(|k| match (a.get_key(k), b.get_key(k)) {
                        (Some(a), Some(b)) => is_equal(a.as_ref(), b.as_ref()),
                        _ => false,
                    })
                }),
                _ => false,
            },
        }
}

#[allow(unused)]
pub const MAX_DEPTH: usize = 128;

/// A small owned JSON value used to represent array/object literals that appear
/// as filter operands (e.g. `?@==[1]`, `?@=={"a":1}`). It implements `SelectValue`
/// so it can be deep-compared against the document's value
#[derive(Debug, Clone, Default)]
pub(crate) enum Literal {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Literal>),
    Object(Vec<(String, Literal)>),
}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        is_equal(self, other)
    }
}

impl Eq for Literal {}

impl Serialize for Literal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(b) => serializer.serialize_bool(*b),
            Self::Int(i) => serializer.serialize_i64(*i),
            Self::Float(f) => serializer.serialize_f64(*f),
            Self::Str(s) => serializer.serialize_str(s),
            Self::Array(a) => a.serialize(serializer),
            Self::Object(o) => {
                let mut map = serializer.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl SelectValue for Literal {
    fn get_type(&self) -> SelectValueType {
        match self {
            Self::Null => SelectValueType::Null,
            Self::Bool(_) => SelectValueType::Bool,
            Self::Int(_) => SelectValueType::Long,
            Self::Float(_) => SelectValueType::Double,
            Self::Str(_) => SelectValueType::String,
            Self::Array(_) => SelectValueType::Array,
            Self::Object(_) => SelectValueType::Object,
        }
    }

    fn contains_key(&self, key: &str) -> bool {
        match self {
            Self::Object(o) => o.iter().any(|(k, _)| k == key),
            _ => false,
        }
    }

    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = ValueRef<'a, Self>> + 'a>> {
        match self {
            Self::Array(a) => Some(Box::new(a.iter().map(ValueRef::Borrowed))),
            Self::Object(o) => Some(Box::new(o.iter().map(|(_, v)| ValueRef::Borrowed(v)))),
            _ => None,
        }
    }

    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>> {
        match self {
            Self::Object(o) => Some(Box::new(o.iter().map(|(k, _)| k.as_str()))),
            _ => None,
        }
    }

    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, ValueRef<'a, Self>)> + 'a>> {
        match self {
            Self::Object(o) => Some(Box::new(
                o.iter().map(|(k, v)| (k.as_str(), ValueRef::Borrowed(v))),
            )),
            _ => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match self {
            Self::Array(a) => Some(a.len()),
            Self::Object(o) => Some(o.len()),
            _ => None,
        }
    }

    fn is_empty(&self) -> Option<bool> {
        match self {
            Self::Array(a) => Some(a.is_empty()),
            Self::Object(o) => Some(o.is_empty()),
            _ => None,
        }
    }

    fn get_key<'a>(&'a self, key: &str) -> Option<ValueRef<'a, Self>> {
        match self {
            Self::Object(o) => o
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| ValueRef::Borrowed(v)),
            _ => None,
        }
    }

    fn get_index<'a>(&'a self, index: usize) -> Option<ValueRef<'a, Self>> {
        match self {
            Self::Array(a) => a.get(index).map(ValueRef::Borrowed),
            _ => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    fn is_double(&self) -> Option<bool> {
        match self {
            Self::Float(_) => Some(true),
            Self::Int(_) => Some(false),
            _ => None,
        }
    }

    fn get_str(&self) -> Option<String> {
        match self {
            Self::Str(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn get_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn get_long(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    fn get_double(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    fn get_array(&self) -> *const c_void {
        null()
    }

    fn get_array_type(&self) -> Option<JSONArrayType> {
        None
    }
}

#[cfg(test)]
mod literal_tests {
    use super::*;

    fn sample_object() -> Literal {
        Literal::Object(vec![
            ("n".to_string(), Literal::Int(1)),
            ("f".to_string(), Literal::Float(1.5)),
            ("s".to_string(), Literal::Str("x".to_string())),
            ("b".to_string(), Literal::Bool(true)),
            ("z".to_string(), Literal::Null),
            (
                "arr".to_string(),
                Literal::Array(vec![Literal::Int(1), Literal::Int(2)]),
            ),
        ])
    }

    #[test]
    fn scalar_accessors() {
        assert_eq!(Literal::default(), Literal::Null);
        assert_eq!(Literal::Null.get_type(), SelectValueType::Null);
        assert_eq!(Literal::Bool(true).get_type(), SelectValueType::Bool);
        assert_eq!(Literal::Int(3).get_type(), SelectValueType::Long);
        assert_eq!(Literal::Float(1.5).get_type(), SelectValueType::Double);
        assert_eq!(
            Literal::Str("a".to_string()).get_type(),
            SelectValueType::String
        );

        assert_eq!(Literal::Bool(true).get_bool(), Some(true));
        assert_eq!(Literal::Int(7).get_long(), Some(7));
        assert_eq!(Literal::Float(2.5).get_double(), Some(2.5));
        assert_eq!(
            Literal::Str("a".to_string()).get_str(),
            Some("a".to_string())
        );
        assert_eq!(Literal::Str("a".to_string()).as_str(), Some("a"));
        assert_eq!(Literal::Float(1.0).is_double(), Some(true));
        assert_eq!(Literal::Int(1).is_double(), Some(false));
        assert_eq!(Literal::Null.is_double(), None);

        // wrong-variant accessors return None
        assert_eq!(Literal::Null.get_bool(), None);
        assert_eq!(Literal::Null.get_long(), None);
        assert_eq!(Literal::Null.get_double(), None);
        assert_eq!(Literal::Null.get_str(), None);
        assert_eq!(Literal::Null.as_str(), None);
    }

    #[test]
    fn array_accessors() {
        let a = Literal::Array(vec![Literal::Int(10), Literal::Int(20)]);
        assert!(a.is_array());
        assert!(!Literal::Int(1).is_array());
        assert_eq!(a.len(), Some(2));
        assert_eq!(a.is_empty(), Some(false));
        assert_eq!(Literal::Array(vec![]).is_empty(), Some(true));
        assert_eq!(a.get_index(1).unwrap().as_ref().get_long(), Some(20));
        assert!(a.get_index(5).is_none());
        let vals: Vec<i64> = a
            .values()
            .unwrap()
            .map(|v| v.as_ref().get_long().unwrap())
            .collect();
        assert_eq!(vals, vec![10, 20]);
        assert!(a.get_array().is_null());
        assert!(a.get_array_type().is_none());

        // scalars are not containers
        assert_eq!(Literal::Int(1).len(), None);
        assert_eq!(Literal::Int(1).is_empty(), None);
        assert!(Literal::Int(1).get_index(0).is_none());
        assert!(Literal::Int(1).values().is_none());
        assert!(!Literal::Int(1).contains_key("x"));
        assert!(Literal::Int(1).keys().is_none());
        assert!(Literal::Int(1).items().is_none());
    }

    #[test]
    fn object_accessors() {
        let o = sample_object();
        assert_eq!(o.len(), Some(6));
        assert_eq!(o.is_empty(), Some(false));
        assert!(o.contains_key("n"));
        assert!(!o.contains_key("missing"));
        assert_eq!(o.get_key("n").unwrap().as_ref().get_long(), Some(1));
        assert!(o.get_key("missing").is_none());
        assert!(!o.is_array());
        assert!(o.get_index(0).is_none());
        let keys: Vec<&str> = o.keys().unwrap().collect();
        assert!(keys.contains(&"n") && keys.contains(&"arr"));
        let items: Vec<&str> = o.items().unwrap().map(|(k, _)| k).collect();
        assert_eq!(items.len(), 6);
        let vals = o.values().unwrap().count();
        assert_eq!(vals, 6);
    }

    #[test]
    fn serialize_and_eq() {
        let a = Literal::Array(vec![
            Literal::Int(1),
            Literal::Str("x".to_string()),
            Literal::Bool(true),
            Literal::Null,
            Literal::Float(2.5),
        ]);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"[1,"x",true,null,2.5]"#
        );
        let o = Literal::Object(vec![("k".to_string(), Literal::Int(1))]);
        assert_eq!(serde_json::to_string(&o).unwrap(), r#"{"k":1}"#);

        // PartialEq (via is_equal) and is_equal cross-type
        assert_eq!(Literal::Int(1), Literal::Int(1));
        assert_ne!(Literal::Int(1), Literal::Int(2));
        assert!(is_equal(&Literal::Bool(true), &Literal::Bool(true)));
        assert!(!is_equal(&Literal::Int(1), &Literal::Null));
    }
}
