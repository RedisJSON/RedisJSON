/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use serde::Serialize;
use std::fmt::Debug;

/// The types a JSON value can have.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SelectValueType {
    /// The JSON value is `null`.
    Null,
    /// The JSON value is a boolean.
    Bool,
    /// The JSON value is a long.
    Long,
    /// The JSON value is a double.
    Double,
    /// The JSON value is a string.
    String,
    /// The JSON value is an array.
    Array,
    /// The JSON value is an object (dictionary), consisting of the
    /// key-value pairs.
    Object,
}

/// The trait that should be implemented by all the types that can be
/// traversed as JSON objects.
pub trait SelectValue: Debug + Eq + PartialEq + Default + Clone + Serialize {
    /// The type of the values this trait should return. This
    /// restriction is due to the hierarchical nature of the JSON
    /// objects, which can include arrays and sub-objects, all of the
    /// same type. In all such cases we still want to be able to work
    /// with such objects as [`SelectValue`] objects, to navigate
    /// through the hierarchy.
    ///
    /// Another reason for this restriction is that we want to be able
    /// to work with the values in the JSON object as if they were
    /// values of the same type, even if they are not. For example, we
    /// want to be able to compare a string with a long, or a double
    /// with a boolean, or even a string with an array, so more complex
    /// types. This is all not to mention that the data we store isn't
    /// necessarily convertible to the actual data structure used:
    /// we can compress the data using some compression algorithms and
    /// this will require decompression before we can use the data and
    /// converting it to the actual data structure used within the
    /// project, or we can store the data in a completely different
    /// format from the one being used within the code, for various
    /// reasons. Due to this, it may not be always possible to convert
    /// the data to the actual data structure used within the project,
    /// so we cannot have a reference to [`Self`] in all cases. Hence,
    /// the put a restriction on the implementation of this trait to
    /// return values not of the same type, but to any type implementing
    /// this trait recursively, to be able to walk through the
    /// hierarchy.
    type Item: SelectValue;

    /// Returns the type of the JSON value.
    fn get_type(&self) -> SelectValueType;

    /// Returns `true` if the JSON object contains a key, meaning it is
    /// a JSON object (dictionary), containing key and value pairs.
    fn contains_key(&self, key: &str) -> bool;

    /// Returns an iterator over the values of the JSON object, in case
    /// it is an array or an object (dictionary).
    fn values(&self) -> Option<Box<dyn Iterator<Item = Self::Item>>>;

    /// Returns an iterator over the keys of the JSON object, in case
    /// it is an object (dictionary).
    fn keys(&self) -> Option<impl Iterator<Item = &str>>;

    /// Returns an iterator over the key-value pairs of the JSON
    /// object, in case it is an object (dictionary).
    fn items(&self) -> Option<impl Iterator<Item = (&str, Self::Item)>>;

    /// Returns the length of the JSON array or an object, if it is an
    /// array or an object (dictionary).
    fn len(&self) -> Option<usize>;

    /// Returns `true` if the JSON object is empty, meaning it is an
    /// empty array or an empty object (dictionary).
    fn is_empty(&self) -> Option<bool>;

    /// Returns the value of the JSON object at the given key, in case
    /// it is an object (dictionary).
    fn get_key(&self, key: &str) -> Option<Self::Item>;

    /// Returns the value of the JSON array at the given index, in
    /// case it is an array.
    fn get_index(&self, index: usize) -> Option<Self::Item>;

    /// Returns `true` if it is a JSON array.
    fn is_array(&self) -> bool;

    /// Returns the [`String`] value of the JSON object.
    ///
    /// # Safety
    ///
    /// This method is unsafe because it can return a value that is
    /// not a string, so it is up to the caller to ensure that the
    /// value is a string prior to calling this method.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a string.
    unsafe fn get_str(&self) -> String;

    /// Returns the [`str`] to value of the JSON object.
    ///
    /// # Safety
    ///
    /// This method is unsafe because it can return a value that is
    /// not a string, so it is up to the caller to ensure that the
    /// value is a string prior to calling this method.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a string.
    unsafe fn as_str(&self) -> &str;

    /// Returns the [`bool`] value of the JSON object.
    ///
    /// # Safety
    ///
    /// This method is unsafe because it can return a value that is
    /// not a boolean, so it is up to the caller to ensure that the
    /// value is a boolean prior to calling this method.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a boolean.
    unsafe fn get_bool(&self) -> bool;

    /// Returns the [`i64`] value of the JSON object.
    ///
    /// # Safety
    ///
    /// This method is unsafe because it can return a value that is
    /// not a long, so it is up to the caller to ensure that the
    /// value is a long prior to calling this method.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a long.
    unsafe fn get_long(&self) -> i64;

    /// Returns the [`f64`] value of the JSON object.
    ///
    /// # Safety
    ///
    /// This method is unsafe because it can return a value that is
    /// not a double, so it is up to the caller to ensure that the
    /// value is a double prior to calling this method.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a double.
    unsafe fn get_double(&self) -> f64;

    /// Returns the [`String`] value of the JSON object, if it is a
    /// string. Otherwise, it returns `None`.
    ///
    /// A safe alternative to [`SelectValue::get_str`].
    fn try_get_string(&self) -> Option<String> {
        if self.get_type() == SelectValueType::String {
            Some(unsafe { self.get_str() })
        } else {
            None
        }
    }

    /// Returns the [`str`] value of the JSON object, if it is a
    /// string. Otherwise, it returns `None`.
    ///
    /// A safe alternative to [`SelectValue::as_str`].
    fn try_get_str(&self) -> Option<&str> {
        if self.get_type() == SelectValueType::String {
            Some(unsafe { self.as_str() })
        } else {
            None
        }
    }

    /// Returns the [`bool`] value of the JSON object, if it is a
    /// boolean. Otherwise, it returns `None`.
    ///
    /// A safe alternative to [`SelectValue::get_bool`].
    fn try_get_bool(&self) -> Option<bool> {
        if self.get_type() == SelectValueType::Bool {
            Some(unsafe { self.get_bool() })
        } else {
            None
        }
    }

    /// Returns the [`i64`] value of the JSON object, if it is a
    /// long. Otherwise, it returns `None`.
    ///
    /// A safe alternative to [`SelectValue::get_long`].
    fn try_get_long(&self) -> Option<i64> {
        if self.get_type() == SelectValueType::Long {
            Some(unsafe { self.get_long() })
        } else {
            None
        }
    }

    /// Returns the [`f64`] value of the JSON object, if it is a
    /// double. Otherwise, it returns `None`.
    ///
    /// A safe alternative to [`SelectValue::get_double`].
    fn try_get_double(&self) -> Option<f64> {
        if self.get_type() == SelectValueType::Double {
            Some(unsafe { self.get_double() })
        } else {
            None
        }
    }
}
