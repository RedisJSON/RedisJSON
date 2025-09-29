/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use serde::Serialize;
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ArrayElementsType {
    Heterogeneous,
    I8,
    U8,
    I16,
    U16,
    F16,
    BF16,
    I32,
    U32,
    F32,
    I64,
    U64,
    F64,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValueRef<'a, T: SelectValue> {
    Borrowed(&'a T),
    Owned(T),
}

impl<'a, T: SelectValue> Serialize for ValueRef<'a, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
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
        self.as_ref() == *other
    }
}

pub trait SelectValue: Debug + Eq + PartialEq + Default + Clone + Serialize {
    fn get_type(&self) -> SelectValueType;
    fn contains_key(&self, key: &str) -> bool;
    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = ValueRef<'a, Self>> + 'a>>;
    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>>;
    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, ValueRef<'a, Self>)> + 'a>>;
    fn len(&self) -> Option<usize>;
    fn is_empty(&self) -> Option<bool>;
    fn get_key<'a>(&'a self, key: &str) -> Option<ValueRef<'a, Self>>;
    fn get_index<'a>(&'a self, index: usize) -> Option<ValueRef<'a, Self>>;
    fn get_index_raw_ref<'a>(&'a self, index: usize) -> Option<*const c_void>;
    fn is_array(&self) -> bool;
    fn is_double(&self) -> Option<bool>;

    fn get_str(&self) -> String;
    fn as_str(&self) -> &str;
    fn get_bool(&self) -> bool;
    fn get_long(&self) -> i64;
    fn get_double(&self) -> f64;
    fn get_array_elements_type(&self) -> Option<ArrayElementsType>;
}
