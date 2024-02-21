/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use json_parser::Value;
use redis_module::raw;

use std::os::raw::{c_int, c_void};

use crate::backward;
use crate::error::Error;
use crate::ivalue_manager::RedisIValueJsonKeyManager;
use crate::manager::Manager;
use serde::Serialize;
use std::fmt;
use std::fmt::Display;
use std::marker::PhantomData;
use std::str::FromStr;

/// Returns normalized start index
#[must_use]
pub fn normalize_arr_start_index(start: i64, len: i64) -> i64 {
    if start < 0 {
        0.max(len + start)
    } else {
        // start >= 0
        start.min(len - 1)
    }
}

/// Return normalized `(start, end)` indices as a tuple
#[must_use]
pub fn normalize_arr_indices(start: i64, end: i64, len: i64) -> (i64, i64) {
    // Normalize start
    let start = normalize_arr_start_index(start, len);
    // Normalize end
    let end = match end {
        0 => len,
        e if e < 0 => 0.max(len + end),
        _ => end.min(len),
    };
    (start, end)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetOptions {
    NotExists,
    AlreadyExists,
    None,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Format {
    STRING,
    JSON,
    BSON,
}
impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "STRING" => Ok(Self::STRING),
            "JSON" => Ok(Self::JSON),
            "BSON" => Ok(Self::BSON),
            _ => Err("ERR wrong format".into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyFormat {
    STRING,
    STRINGS,
    EXPAND1,
    EXPAND,
}
impl FromStr for ReplyFormat {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "STRING" => Ok(Self::STRING),
            "STRINGS" => Ok(Self::STRINGS),
            "EXPAND1" => Ok(Self::EXPAND1),
            "EXPAND" => Ok(Self::EXPAND),
            _ => Err("ERR wrong reply format".into()),
        }
    }
}

///
/// Backwards compatibility converter for `RedisJSON` 1.x clients
///
pub struct Path<'a> {
    original_path: &'a str,
    fixed_path: Option<String>,
}

impl<'a> Path<'a> {
    #[must_use]
    pub fn new(path: &'a str) -> Path {
        let fixed_path = if path.starts_with('$')
            && (path.len() < 2 || (path.as_bytes()[1] == b'.' || path.as_bytes()[1] == b'['))
        {
            None
        } else {
            let mut cloned = path.to_string();
            if path == "." {
                cloned.replace_range(..1, "$");
            } else if path.starts_with('.') {
                cloned.insert(0, '$');
            } else {
                cloned.insert_str(0, "$.");
            }
            Some(cloned)
        };
        Path {
            original_path: path,
            fixed_path,
        }
    }

    #[must_use]
    pub const fn is_legacy(&self) -> bool {
        self.fixed_path.is_some()
    }

    pub fn get_path(&self) -> &str {
        self.fixed_path
            .as_ref()
            .map_or(self.original_path, String::as_str)
    }

    #[must_use]
    pub const fn get_original(&self) -> &'a str {
        self.original_path
    }
}

impl Display for Path<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get_path())
    }
}

/// A trait for the types that can be interned.
/// Only the sized types can be interned.
///
/// The `Interned` type is a type of the interned value. For example,
/// `String` is interned not as `String`, but as some other type, that
/// references the same string data and to which a string reference can
/// be converted.
pub trait Internable<Interned>: Sized {
    /// Interns the passed object and returns the interned value.
    fn intern<T: Into<Self>>(value: T) -> Interned;
}

impl Internable<ijson::IString> for String {
    fn intern<T: Into<Self>>(value: T) -> ijson::IString {
        ijson::IString::intern(&value.into())
    }
}

impl Internable<ijson::IString> for &str {
    fn intern<T: Into<Self>>(value: T) -> ijson::IString {
        ijson::IString::intern(value.into())
    }
}

impl Internable<json_parser::JsonString> for String {
    fn intern<T: Into<Self>>(value: T) -> json_parser::JsonString {
        value.into()
    }
}

impl Internable<json_parser::JsonString> for &str {
    fn intern<T: Into<Self>>(value: T) -> json_parser::JsonString {
        value.into().into()
    }
}

/// Allows the object to be cleared, meaning the object is considered
/// "empty".
pub trait Clearable {
    fn clear(&mut self) -> bool;
}

/// Allows to provide with all sorts of memory consumption information.
pub trait MemoryConsumption {
    /// Returns the number of bytes an object which implements thits
    /// trait occupies in memory (RAM).
    fn get_memory_occupied(&self) -> usize;
}

/// Allows to take out an element from the object by index, returning it
/// while removing it from the object.
pub trait TakeOutByIndex<T> {
    /// Takes out an element from the object by index.
    fn take_out(&mut self, index: usize) -> Option<T>;
}

/// A trait for the types that can be used as a value in RedisJSON.
///
/// Contains helpful abstractions for easier change of the underlying
/// JSON implementation.
pub trait JsonValueImpl {
    const NULL: Self;
}

impl JsonValueImpl for json_parser::Value {
    const NULL: Self = json_parser::Value::Null;
}

impl JsonValueImpl for ijson::IValue {
    const NULL: Self = ijson::IValue::NULL;
}

/// A trait for the types that can be used as a value in RedisJSON.
pub trait RedisJSONTypeInfo {
    type Value;
    type Number;
    type String;
}

/// Trait for the types that can be used as a value in RedisJSON.
pub trait RedisJSONValueTraits: Clone + std::fmt::Debug + Serialize + JsonValueImpl {}
// Auto-implementation for all types which are suitable.
impl<T> RedisJSONValueTraits for T where T: Clone + std::fmt::Debug + Serialize + JsonValueImpl {}

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct RedisJSON<T>
where
    T: RedisJSONValueTraits,
{
    //FIXME: expose array/object Values without requiring a path
    data: T,
}

impl RedisJSONTypeInfo for RedisJSON<json_parser::Value> {
    /// The type this RedisJSON holds.
    type Value = json_parser::Value;
    type Number = json_parser::JsonNumber;
    type String = json_parser::JsonString;
}

impl RedisJSONTypeInfo for RedisJSON<ijson::IValue> {
    type Value = ijson::IValue;
    type Number = ijson::INumber;
    type String = ijson::IString;
}

impl<T: RedisJSONValueTraits> From<T> for RedisJSON<T> {
    fn from(data: T) -> Self {
        Self { data }
    }
}

impl<T: RedisJSONValueTraits> AsRef<T> for RedisJSON<T> {
    fn as_ref(&self) -> &T {
        &self.data
    }
}

impl<T: RedisJSONValueTraits> AsMut<T> for RedisJSON<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T: RedisJSONValueTraits> std::ops::Deref for RedisJSON<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: RedisJSONValueTraits> std::ops::DerefMut for RedisJSON<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

/// A trait for the JSON types that can be mutated.
pub trait MutableJsonValue: Clone {
    /// Replaces a value at a given `path`, starting from `root`
    ///
    /// The new value is the value returned from `func`, which is called on the current value.
    ///
    /// If the returned value from `func` is [`None`], the current value is removed.
    /// If the returned value from `func` is [`Err`], the current value remains (although it could be modified by `func`)
    fn replace<F: FnMut(&mut Self) -> Result<Option<Self>, Error>>(
        &mut self,
        path: &[String],
        func: F,
    ) -> Result<(), Error>;

    /// Updates a value at a given `path`, starting from `root`
    ///
    /// The value is modified by `func`, which is called on the current value.
    /// If the returned value from `func` is [`None`], the current value is removed.
    /// If the returned value from `func` is [`Err`], the current value remains (although it could be modified by `func`)
    fn update<F: FnMut(&mut Self) -> Result<Option<()>, Error>>(
        &mut self,
        path: &[String],
        func: F,
    ) -> Result<(), Error>;

    /// Merges two values.
    fn merge(&mut self, patch: &Self);
}

/// An alias for the RedisJSON implementation using the `ijson::IValue`
/// type.
#[cfg(feature = "ijson_parser")]
pub type RedisJSONData = RedisJSON<ijson::IValue>;
#[cfg(feature = "custom_parser")]
pub type RedisJSONData = RedisJSON<Value>;

pub mod type_methods {
    use super::*;
    use std::{ffi::CString, ptr::null_mut};

    pub extern "C" fn rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
        let json_string = value_rdb_load_json(rdb, encver);
        match json_string {
            Ok(json_string) => {
                let m = RedisIValueJsonKeyManager {
                    phantom: PhantomData,
                };
                let v = m.from_str(&json_string, Format::JSON, false);
                v.map_or(null_mut(), |res| {
                    Box::into_raw(Box::new(res)).cast::<libc::c_void>()
                })
            }
            Err(_) => null_mut(),
        }
    }

    #[allow(non_snake_case, unused)]
    pub fn value_rdb_load_json(
        rdb: *mut raw::RedisModuleIO,
        encver: c_int,
    ) -> Result<String, Error> {
        Ok(match encver {
            0 => {
                let v = backward::json_rdb_load(rdb)?;

                let mut out = serde_json::Serializer::new(Vec::new());
                v.serialize(&mut out).unwrap();
                String::from_utf8(out.into_inner()).unwrap()
            }
            2 => {
                let data = raw::load_string(rdb)?;
                // Backward support for modules that had AUX field for RediSarch
                // TODO remove in future versions
                let u = raw::load_unsigned(rdb)?;
                if u > 0 {
                    raw::load_string(rdb)?;
                    raw::load_string(rdb)?;
                }
                data.try_as_str()?.to_string()
            }
            3 => {
                let data = raw::load_string(rdb)?;
                data.try_as_str()?.to_string()
            }
            _ => panic!("Can't load old RedisJSON RDB"),
        })
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn free(value: *mut c_void) {
        if value.is_null() {
            // on Redis 6.0 we might get a NULL value here, so we need to handle it.
            return;
        }
        let v = value.cast::<RedisJSONData>();
        // Take ownership of the data from Redis (causing it to be dropped when we return)
        Box::from_raw(v);
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
        let mut out = serde_json::Serializer::new(Vec::new());

        let v = unsafe { &*value.cast::<RedisJSONData>() };
        v.data.serialize(&mut out).unwrap();
        let json = String::from_utf8(out.into_inner()).unwrap();

        let cjson = CString::new(json).unwrap();
        raw::save_string(rdb, cjson.to_str().unwrap());
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn copy(
        fromkey: *mut raw::RedisModuleString,
        tokey: *mut raw::RedisModuleString,
        value: *const c_void,
    ) -> *mut c_void {
        let v = unsafe { &*value.cast::<RedisJSONData>() };
        let value = v.data.clone();
        Box::into_raw(Box::new(value)).cast::<c_void>()
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn mem_usage(value: *const c_void) -> usize {
        let json = unsafe { &*(value as *mut RedisJSONData) };
        let manager = RedisIValueJsonKeyManager {
            phantom: PhantomData,
        };
        manager.get_memory(&json.data).unwrap_or(0)
    }
}