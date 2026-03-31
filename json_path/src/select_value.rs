/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use serde::{Serialize, Serializer};
use std::{ffi::c_void, fmt::Debug};

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
                            debug_assert!(
                                v.is_some(),
                                "key {:?} in keys() but get_key() returned None",
                                k
                            );
                            v.map_or(0, |v| v.calculate_value_depth())
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
            SelectValueType::Bool => {
                debug_assert!(
                    a.get_bool().is_some() && b.get_bool().is_some(),
                    "get_type/getter mismatch in is_equal"
                );
                a.get_bool().zip(b.get_bool()).is_some_and(|(x, y)| x == y)
            }
            SelectValueType::Long => {
                debug_assert!(
                    a.get_long().is_some() && b.get_long().is_some(),
                    "get_type/getter mismatch in is_equal"
                );
                a.get_long().zip(b.get_long()).is_some_and(|(x, y)| x == y)
            }
            SelectValueType::Double => {
                debug_assert!(
                    a.get_double().is_some() && b.get_double().is_some(),
                    "get_type/getter mismatch in is_equal"
                );
                a.get_double()
                    .zip(b.get_double())
                    .is_some_and(|(x, y)| x == y)
            }
            SelectValueType::String => {
                debug_assert!(
                    a.get_str().is_some() && b.get_str().is_some(),
                    "get_type/getter mismatch in is_equal"
                );
                a.get_str().zip(b.get_str()).is_some_and(|(x, y)| x == y)
            }
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
