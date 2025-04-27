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

pub trait SelectValue: Debug + Eq + PartialEq + Default + Clone + Serialize {
    fn get_type(&self) -> SelectValueType;
    fn contains_key(&self, key: &str) -> bool;
    fn values<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a Self> + 'a>>;
    fn keys<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a str> + 'a>>;
    fn items<'a>(&'a self) -> Option<Box<dyn Iterator<Item = (&'a str, &'a Self)> + 'a>>;
    fn len(&self) -> Option<usize>;
    fn is_empty(&self) -> Option<bool>;
    fn get_key<'a>(&'a self, key: &str) -> Option<&'a Self>;
    fn get_index(&self, index: usize) -> Option<&Self>;
    fn is_array(&self) -> bool;
    fn is_double(&self) -> Option<bool>;

    fn get_str(&self) -> String;
    fn as_str(&self) -> &str;
    fn get_bool(&self) -> bool;
    fn get_long(&self) -> i64;
    fn get_double(&self) -> f64;
}
