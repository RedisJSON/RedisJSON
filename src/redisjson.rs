// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use std::io::Cursor;
use std::os::raw::{c_int, c_void};

use bson::decode_document;
use redis_module::raw::{self};
use serde_json::Value;

use crate::backward;
use crate::c_api::JSONType;
use crate::error::Error;

use std::fmt;
use std::fmt::Display;

/// Returns normalized start index
pub fn normalize_arr_start_index(start: i64, len: i64) -> i64 {
    if start < 0 {
        0.max(len + start)
    } else {
        // start >= 0
        start.min(len - 1)
    }
}

/// Return normalized `(start, end)` indices as a tuple
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
impl Format {
    pub fn from_str(s: &str) -> Result<Format, Error> {
        match s {
            "JSON" => Ok(Format::JSON),
            "BSON" => Ok(Format::BSON),
            _ => Err("ERR wrong format".into()),
        }
    }
}

///
/// Backwards compatibility convertor for RedisJSON 1.x clients
///
pub struct Path<'a> {
    original_path: &'a str,
    fixed_path: Option<String>,
}

impl<'a> Path<'a> {
    pub fn new(path: &'a str) -> Path {
        let fixed_path = if path.starts_with('$') {
            None
        } else {
            let mut cloned = path.to_string();
            if path == "." {
                cloned.replace_range(..1, "$");
            } else if path.starts_with('.') {
                cloned.insert(0, '$')
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

    pub fn is_legacy(&self) -> bool {
        self.fixed_path.is_some()
    }

    pub fn get_path(&'a self) -> &'a str {
        if let Some(s) = &self.fixed_path {
            s.as_str()
        } else {
            self.original_path
        }
    }

    pub fn get_original(&self) -> &'a str {
        self.original_path
    }
}

impl Display for Path<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get_path())
    }
}

#[derive(Debug)]
pub struct RedisJSON {
    //FIXME: make private and expose array/object Values without requiring a path
    pub data: Value,
}

impl RedisJSON {
    pub fn parse_str(data: &str, format: Format) -> Result<Value, Error> {
        match format {
            Format::JSON => Ok(serde_json::from_str(data)?),
            Format::BSON => decode_document(&mut Cursor::new(data.as_bytes()))
                .map(|docs| {
                    let v = if !docs.is_empty() {
                        docs.iter()
                            .next()
                            .map_or_else(|| Value::Null, |(_, b)| b.clone().into())
                    } else {
                        Value::Null
                    };
                    Ok(v)
                })
                .unwrap_or_else(|e| Err(e.to_string().into())),
        }
    }

    pub fn from_str(data: &str, format: Format) -> Result<Self, Error> {
        let value = RedisJSON::parse_str(data, format)?;
        Ok(Self { data: value })
    }

    pub fn serialize(results: &Value, format: Format) -> Result<String, Error> {
        let res = match format {
            Format::JSON => serde_json::to_string(results)?,
            Format::BSON => return Err("ERR Soon to come...".into()), //results.into() as Bson,
        };
        Ok(res)
    }

    pub fn value_name(value: &Value) -> &str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) => {
                if n.is_f64() {
                    "number"
                } else {
                    "integer"
                }
            }
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    pub fn get_type_and_size(data: &Value) -> (JSONType, libc::size_t) {
        match data {
            Value::Null => (JSONType::Null, 0),
            Value::Bool(_) => (JSONType::Bool, 0),
            Value::Number(n) => {
                if n.is_f64() {
                    (JSONType::Double, 0)
                } else {
                    (JSONType::Int, 0)
                }
            }
            Value::String(_) => (JSONType::String, 0),
            Value::Array(arr) => (JSONType::Array, arr.len()),
            Value::Object(map) => (JSONType::Object, map.len()),
        }
    }
}

pub mod type_methods {
    use super::*;
    use std::ptr::null_mut;

    pub extern "C" fn rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
        match rdb_load_json(rdb, encver) {
            Ok(res) => res,
            Err(_) => null_mut(),
        }
    }

    #[allow(non_snake_case, unused)]
    pub fn rdb_load_json(
        rdb: *mut raw::RedisModuleIO,
        encver: c_int,
    ) -> Result<*mut c_void, Error> {
        let json = match encver {
            0 => {
                let d = backward::json_rdb_load(rdb)?;
                RedisJSON { data: d }
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
                RedisJSON::from_str(data.try_as_str()?, Format::JSON).unwrap()
            }
            3 => {
                let data = raw::load_string(rdb)?;
                RedisJSON::from_str(data.try_as_str()?, Format::JSON).unwrap()
            }
            _ => panic!("Can't load old RedisJSON RDB"),
        };
        Ok(Box::into_raw(Box::new(json)) as *mut c_void)
    }

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn free(value: *mut c_void) {
        let json = value as *mut RedisJSON;

        // Take ownership of the data from Redis (causing it to be dropped when we return)
        Box::from_raw(json);
    }

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
        let json = &*(value as *mut RedisJSON);
        raw::save_string(rdb, &json.data.to_string());
    }
}
