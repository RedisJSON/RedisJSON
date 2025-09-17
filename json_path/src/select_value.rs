/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use serde::Serialize;
use std::fmt::Debug;

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

pub enum ValueRef<'a, T> {
    Borrowed(&'a T),
    Owned(T),
}

impl<'a, T> AsRef<T> for ValueRef<'a, T> {
    fn as_ref(&self) -> &T {
        match self {
            ValueRef::Borrowed(t) => t,
            ValueRef::Owned(t) => t,
        }
    }
}

impl<'a, T> ValueRef<'a, T> {
    /// Returns a reference with lifetime 'a.
    ///
    /// # Safety
    /// This is safe because:
    /// - For Borrowed variant: the reference already has lifetime 'a
    /// - For Owned variant: the owned value lives as long as self, and self
    ///   is guaranteed to live at least as long as any references we return
    pub fn as_ref_with_lifetime(&self) -> &'a T {
        match self {
            ValueRef::Borrowed(t) => t,
            ValueRef::Owned(ref t) => {
                // SAFETY: We're extending the lifetime of the reference to 'a.
                // This is safe because:
                // 1. The ValueRef<'a, T> itself must live at least as long as 'a
                //    (due to the lifetime parameter)
                // 2. The Owned(T) variant owns the T, so it lives as long as the ValueRef
                // 3. Therefore, a reference to the owned T is valid for lifetime 'a
                unsafe { std::mem::transmute::<&T, &'a T>(t) }
            }
        }
    }
}

impl<'a, T> std::ops::Deref for ValueRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

pub trait SelectValue: Debug + Eq + PartialEq + Default + Clone + Serialize {
    fn get_type(&self) -> SelectValueType;
    fn contains_key(&self, key: &str) -> bool;
    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = ValueRef<'a, Self>> + 'a>>;
    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>>;
    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, &'a Self)> + 'a>>;
    fn len(&self) -> Option<usize>;
    fn is_empty(&self) -> Option<bool>;
    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self>;
    fn get_index<'a>(&'a self, index: usize) -> Option<ValueRef<'a, Self>>;
    fn is_array(&self) -> bool;
    fn is_double(&self) -> Option<bool>;

    fn get_str(&self) -> String;
    fn as_str(&self) -> &str;
    fn get_bool(&self) -> bool;
    fn get_long(&self) -> i64;
    fn get_double(&self) -> f64;
}
