// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use crate::backward;
use crate::commands::index;
use crate::error::Error;
use crate::nodevisitor::NodeVisitorImpl;

use bson::decode_document;
use jsonpath_lib::SelectorMut;
use redis_module::raw;
use serde_json::Value;
use std::io::Cursor;
use std::mem;
use std::os::raw::{c_int, c_void};

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
pub struct Path {
    pub path: String,
    pub fixed: String,
}

impl Path {
    pub fn new(path: String) -> Path {
        let mut fixed = path.clone();
        if !fixed.starts_with('$') {
            if fixed == "." {
                fixed.replace_range(..1, "$");
            } else if fixed.starts_with('.') {
                fixed.insert(0, '$');
            } else {
                fixed.insert_str(0, "$.");
            }
        }
        Path { path, fixed }
    }
}

#[derive(Debug, Clone)]
pub struct ValueIndex {
    pub key: String,
    pub index_name: String,
}

#[derive(Debug)]
pub struct RedisJSON {
    data: Value,
    pub value_index: Option<ValueIndex>,
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

    pub fn from_str(
        data: &str,
        value_index: &Option<ValueIndex>,
        format: Format,
    ) -> Result<Self, Error> {
        let value = RedisJSON::parse_str(data, format)?;
        Ok(Self {
            data: value,
            value_index: value_index.clone(),
        })
    }

    fn add_value(&mut self, path: &str, value: Value) -> Result<bool, Error> {
        if NodeVisitorImpl::check(path)? {
            let mut splits = path.rsplitn(2, '.');
            let key = splits.next().unwrap();
            let prefix = splits.next().unwrap();

            let mut current_data = self.data.take();
            if prefix == "$" {
                let res = if let Value::Object(ref mut map) = current_data {
                    if map.contains_key(key) {
                        false
                    } else {
                        map.insert(key.to_string(), value);
                        true
                    }
                } else {
                    false
                };
                self.data = current_data;
                Ok(res)
            } else {
                let mut set = false;
                self.data = jsonpath_lib::replace_with(current_data, prefix, &mut |mut ret| {
                    if let Value::Object(ref mut map) = ret {
                        if map.contains_key(key) {
                            set = false;
                        } else {
                            map.insert(key.to_string(), value.clone());
                            set = true;
                        }
                    }
                    Some(ret)
                })?;
                Ok(set)
            }
        } else {
            Err("Err: wrong static path".into())
        }
    }

    pub fn set_value(
        &mut self,
        data: &str,
        path: &str,
        option: &SetOptions,
        format: Format,
    ) -> Result<bool, Error> {
        let json: Value = RedisJSON::parse_str(data, format)?;
        if path == "$" {
            if SetOptions::NotExists == *option {
                Ok(false)
            } else {
                self.data = json;
                Ok(true)
            }
        } else {
            let mut replaced = false;
            if SetOptions::NotExists != *option {
                let current_data = self.data.take();
                self.data = jsonpath_lib::replace_with(current_data, path, &mut |_v| {
                    replaced = true;
                    Some(json.clone())
                })?;
            }
            if replaced {
                Ok(true)
            } else if SetOptions::AlreadyExists != *option {
                self.add_value(path, json)
            } else {
                Ok(false)
            }
        }
    }

    pub fn delete_path(&mut self, path: &str) -> Result<usize, Error> {
        let current_data = self.data.take();

        let mut deleted = 0;
        self.data = jsonpath_lib::replace_with(current_data, path, &mut |v| {
            if !v.is_null() {
                deleted += 1; // might delete more than a single value
            }
            None
        })?;
        Ok(deleted)
    }

    pub fn to_string(&self, path: &str, format: Format) -> Result<String, Error> {
        let results = self.get_first(path)?;
        Self::serialize(results, format)
    }

    pub fn serialize(results: &Value, format: Format) -> Result<String, Error> {
        let res = match format {
            Format::JSON => serde_json::to_string(results)?,
            Format::BSON => return Err("Soon to come...".into()), //results.into() as Bson,
        };
        Ok(res)
    }

    // FIXME: Implement this by manipulating serde_json::Value values,
    // and then using serde to serialize to JSON instead of doing it ourselves with strings.
    pub fn to_json(&self, paths: &mut Vec<Path>) -> Result<String, Error> {
        let mut selector = jsonpath_lib::selector(&self.data);
        let mut result = paths.drain(..).fold(String::from("{"), |mut acc, path| {
            let value = match selector(&path.fixed) {
                Ok(s) => match s.first() {
                    Some(v) => v,
                    None => &Value::Null,
                },
                Err(_) => &Value::Null,
            };
            acc.push('\"');
            acc.push_str(&path.path);
            acc.push_str("\":");
            acc.push_str(value.to_string().as_str());
            acc.push(',');
            acc
        });
        if result.ends_with(',') {
            result.pop();
        }
        result.push('}');
        Ok(result)
    }

    pub fn str_len(&self, path: &str) -> Result<usize, Error> {
        self.get_first(path)?
            .as_str()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|s| s.len())
    }

    pub fn arr_len(&self, path: &str) -> Result<usize, Error> {
        self.get_first(path)?
            .as_array()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|arr| arr.len())
    }

    pub fn obj_len(&self, path: &str) -> Result<usize, Error> {
        self.get_first(path)?
            .as_object()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|obj| obj.len())
    }

    pub fn obj_keys<'a>(&'a self, path: &'a str) -> Result<Vec<&'a String>, Error> {
        self.get_first(path)?
            .as_object()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|obj| obj.keys().collect())
    }

    pub fn arr_index(&self, path: &str, scalar: &str, start: i64, end: i64) -> Result<i64, Error> {
        if let Value::Array(arr) = self.get_first(path)? {
            // end=-1/0 means INFINITY to support backward with RedisJSON
            if arr.is_empty() || end < -1 {
                return Ok(-1);
            }
            let v: Value = serde_json::from_str(scalar)?;

            let end: usize = if end == 0 || end == -1 {
                // default end of array
                arr.len() - 1
            } else {
                (end as usize).min(arr.len() - 1)
            };
            let start = start.max(0) as usize;
            if end < start {
                return Ok(-1);
            }
            let slice = &arr[start..=end];

            match slice.iter().position(|r| r == &v) {
                Some(i) => Ok((start + i) as i64),
                None => Ok(-1),
            }
        } else {
            Ok(-1)
        }
    }

    pub fn get_type(&self, path: &str) -> Result<String, Error> {
        let s = RedisJSON::value_name(self.get_first(path)?);
        Ok(s.to_string())
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

    pub fn value_op<F>(&mut self, path: &str, mut fun: F) -> Result<Value, Error>
    where
        F: FnMut(&Value) -> Result<Value, Error>,
    {
        let current_data = self.data.take();

        let mut errors = vec![];
        let mut result = Value::Null; // TODO handle case where path not found

        let mut collect_fun = |value: Value| {
            fun(&value)
                .map(|new_value| {
                    result = new_value.clone();
                    new_value
                })
                .map_err(|e| {
                    errors.push(e);
                })
                .unwrap_or(value)
        };

        self.data = if path == "$" {
            // root needs special handling
            collect_fun(current_data)
        } else {
            SelectorMut::new()
                .str_path(path)
                .and_then(|selector| {
                    Ok(selector
                        .value(current_data.clone())
                        .replace_with(&mut |v| Some(collect_fun(v)))?
                        .take()
                        .unwrap_or(Value::Null))
                })
                .map_err(|e| {
                    errors.push(e.into());
                })
                .unwrap_or(current_data)
        };

        match errors.len() {
            0 => Ok(result),
            1 => Err(errors.remove(0)),
            _ => Err(errors.into_iter().map(|e| e.msg).collect::<String>().into()),
        }
    }

    pub fn get_memory<'a>(&'a self, path: &'a str) -> Result<usize, Error> {
        // TODO add better calculation, handle wrappers, internals and length
        let res = match self.get_first(path)? {
            Value::Null => 0,
            Value::Bool(v) => mem::size_of_val(v),
            Value::Number(v) => mem::size_of_val(v),
            Value::String(v) => mem::size_of_val(v),
            Value::Array(v) => mem::size_of_val(v),
            Value::Object(v) => mem::size_of_val(v),
        };
        Ok(res)
    }

    pub fn get_first<'a>(&'a self, path: &'a str) -> Result<&'a Value, Error> {
        let results = self.get_values(path)?;
        match results.first() {
            Some(s) => Ok(s),
            None => Err("ERR path does not exist".into()),
        }
    }

    pub fn get_values<'a>(&'a self, path: &'a str) -> Result<Vec<&'a Value>, Error> {
        let results = jsonpath_lib::select(&self.data, path)?;
        Ok(results)
    }
}

pub mod type_methods {
    use super::*;

    #[allow(non_snake_case, unused)]
    pub extern "C" fn rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
        let json = match encver {
            0 => RedisJSON {
                data: backward::json_rdb_load(rdb),
                value_index: None, // TODO handle load from rdb
            },
            2 => {
                let data = raw::load_string(rdb);
                let schema = if raw::load_unsigned(rdb) > 0 {
                    Some(ValueIndex {
                        key: raw::load_string(rdb),
                        index_name: raw::load_string(rdb),
                    })
                } else {
                    None
                };
                RedisJSON::from_str(&data, &schema, Format::JSON).unwrap()
            }
            _ => panic!("Can't load old RedisJSON RDB"),
        };
        Box::into_raw(Box::new(json)) as *mut c_void
    }

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn free(value: *mut c_void) {
        let json = value as *mut RedisJSON;

        // Take ownership of the data from Redis (causing it to be dropped when we return)
        let json = Box::from_raw(json);

        if let Some(value_index) = &json.value_index {
            index::remove_document(&value_index.key, &value_index.index_name);
        }
    }

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
        let json = &*(value as *mut RedisJSON);
        raw::save_string(rdb, &json.data.to_string());
        if let Some(value_index) = &json.value_index {
            raw::save_unsigned(rdb, 1);
            raw::save_string(rdb, &value_index.key);
            raw::save_string(rdb, &value_index.index_name);
        } else {
            raw::save_unsigned(rdb, 0);
        }
    }
}
