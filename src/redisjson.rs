// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use redis_module::raw;

use std::os::raw::{c_int, c_void};

use crate::backward;
use crate::error::Error;
use crate::ivalue_manager::RedisIValueJsonKeyManager;
use crate::manager::{Manager, RedisJsonKeyManager};
use crate::{get_manager_type, ManagerType};
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

#[derive(Debug, PartialEq)]
pub enum SetOptions {
    NotExists,
    AlreadyExists,
    None,
}

#[derive(Debug, PartialEq)]
pub enum Format {
    JSON,
    BSON,
}
impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "JSON" => Ok(Self::JSON),
            "BSON" => Ok(Self::BSON),
            _ => Err("ERR wrong format".into()),
        }
    }
}

///
/// Backwards compatibility convertor for `RedisJSON` 1.x clients
///
pub struct Path<'a> {
    original_path: &'a str,
    fixed_path: Option<String>,
}

impl<'a> Path<'a> {
    #[must_use]
    pub fn new(path: &'a str) -> Path {
        let fixed_path = if path.starts_with('$') {
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

#[derive(Debug)]
pub struct RedisJSON<T> {
    //FIXME: make private and expose array/object Values without requiring a path
    pub data: T,
}

pub mod type_methods {
    use super::*;
    use std::{ffi::CString, ptr::null_mut};

    pub extern "C" fn rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
        let json_string = value_rdb_load_json(rdb, encver);
        match json_string {
            Ok(json_string) => match get_manager_type() {
                ManagerType::SerdeValue => {
                    let m = RedisJsonKeyManager {
                        phantom: PhantomData,
                    };
                    let v = m.from_str(&json_string, Format::JSON);
                    match v {
                        Ok(res) => Box::into_raw(Box::new(res)).cast::<libc::c_void>(),
                        Err(_) => null_mut(),
                    }
                }
                ManagerType::IValue => {
                    let m = RedisIValueJsonKeyManager {
                        phantom: PhantomData,
                    };
                    let v = m.from_str(&json_string, Format::JSON);
                    match v {
                        Ok(res) => Box::into_raw(Box::new(res)).cast::<libc::c_void>(),
                        Err(_) => null_mut(),
                    }
                }
            },
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
                v.serialize(&mut out)?;
                String::from_utf8(out.into_inner())?
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
            _ => return Err("Can't load old RedisJSON RDB".into()),
        })
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn free(value: *mut c_void) {
        if value.is_null() {
            // on Redis 6.0 we might get a NULL value here, so we need to handle it.
            return;
        }
        match get_manager_type() {
            ManagerType::SerdeValue => {
                let v = value.cast::<RedisJSON<serde_json::Value>>();
                // Take ownership of the data from Redis (causing it to be dropped when we return)
                Box::from_raw(v);
            }
            ManagerType::IValue => {
                let v = value.cast::<RedisJSON<ijson::IValue>>();
                // Take ownership of the data from Redis (causing it to be dropped when we return)
                Box::from_raw(v);
            }
        };
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
        let mut out = serde_json::Serializer::new(Vec::new());
        let json = match get_manager_type() {
            ManagerType::SerdeValue => {
                let v = unsafe { &*value.cast::<RedisJSON<serde_json::Value>>() };
                v.data.serialize(&mut out).unwrap();
                String::from_utf8(out.into_inner()).unwrap()
            }
            ManagerType::IValue => {
                let v = unsafe { &*value.cast::<RedisJSON<ijson::IValue>>() };
                v.data.serialize(&mut out).unwrap();
                String::from_utf8(out.into_inner()).unwrap()
            }
        };
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
        match get_manager_type() {
            ManagerType::SerdeValue => {
                let v = unsafe { &*value.cast::<RedisJSON<serde_json::Value>>() };
                let value = v.data.clone();
                Box::into_raw(Box::new(value)).cast::<c_void>()
            }
            ManagerType::IValue => {
                let v = unsafe { &*value.cast::<RedisJSON<ijson::IValue>>() };
                let value = v.data.clone();
                Box::into_raw(Box::new(value)).cast::<c_void>()
            }
        }
    }

    /// # Safety
    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn mem_usage(value: *const c_void) -> usize {
        match get_manager_type() {
            ManagerType::SerdeValue => {
                let json = unsafe { &*(value as *mut RedisJSON<serde_json::Value>) };
                let manager = RedisJsonKeyManager {
                    phantom: PhantomData,
                };
                manager.get_memory(&json.data).unwrap_or(0)
            }
            ManagerType::IValue => {
                let json = unsafe { &*(value as *mut RedisJSON<ijson::IValue>) };
                let manager = RedisIValueJsonKeyManager {
                    phantom: PhantomData,
                };
                manager.get_memory(&json.data).unwrap_or(0)
            }
        }
    }
}
