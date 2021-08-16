// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use std::io::Cursor;
use std::mem;
use std::os::raw::{c_int, c_void};

use bson::decode_document;
use jsonpath_lib::select::json_node::JsonValueUpdater;
use jsonpath_lib::select::{Selector, SelectorMut};
use redis_module::raw::{self, Status};
use serde_json::Value;

use crate::backward;
use crate::c_api::JSONType;
use crate::error::Error;
use crate::nodevisitor::{StaticPathElement, StaticPathParser, VisitStatus};
use crate::REDIS_JSON_TYPE_VERSION;

use std::fmt;
use std::fmt::Display;

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

    fn add_value(&mut self, path: &str, value: Value) -> Result<bool, Error> {
        let mut parsed_static_path = StaticPathParser::check(path)?;

        if parsed_static_path.valid != VisitStatus::Valid {
            return Err("Err: wrong static path".into());
        }
        if parsed_static_path.static_path_elements.len() < 2 {
            return Err("Err: path must end with object key to set".into());
        }

        if let StaticPathElement::ObjectKey(key) =
            parsed_static_path.static_path_elements.pop().unwrap()
        {
            if let StaticPathElement::Root = parsed_static_path.static_path_elements.last().unwrap()
            {
                // Adding to the root, can't use jsonpath_lib::replace_with
                let mut current_data = self.data.take();
                let res = if let Value::Object(ref mut map) = current_data {
                    if map.contains_key(&key) {
                        false
                    } else {
                        map.insert(key, value);
                        true
                    }
                } else {
                    false
                };
                self.data = current_data;
                Ok(res)
            } else {
                // Adding somewhere in existing object, use jsonpath_lib::replace_with
                let p = parsed_static_path
                    .static_path_elements
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("");
                let mut set = false;
                let mut selector = SelectorMut::default();
                if let Err(e) = selector.str_path(&p) {
                    return Err(e.into());
                }
                selector.value(&mut self.data);
                let mut updater = JsonValueUpdater::new(|mut ret| {
                    if let Value::Object(ref mut map) = ret {
                        if map.contains_key(&key) {
                            set = false;
                        } else {
                            map.insert(key.to_string(), value.clone());
                            set = true;
                        }
                    }
                    Some(ret)
                });
                selector.replace_with(&mut updater)?;
                Ok(set)
            }
        } else {
            Err("Err: path not an object".into())
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
                self.data = jsonpath_lib::replace_with(self.data.take(), path, |_v| {
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
        let mut deleted = 0;
        self.data = jsonpath_lib::replace_with(self.data.take(), path, |v| {
            if !v.is_null() {
                deleted += 1; // might delete more than a single value
            }
            None
        })?;
        Ok(deleted)
    }

    pub fn clear(&mut self, path: &str) -> Result<usize, Error> {
        let current_data = self.data.take();
        let mut cleared = 0;

        let clear_func = &mut |v| match v {
            Value::Object(mut obj) => {
                obj.clear();
                cleared += 1;
                Some(Value::from(obj))
            }
            Value::Array(mut arr) => {
                arr.clear();
                cleared += 1;
                Some(Value::from(arr))
            }
            _ => Some(v),
        };

        self.data = if path == "$" {
            clear_func(current_data).unwrap()
        } else {
            jsonpath_lib::replace_with(current_data, path, clear_func)?
        };
        Ok(cleared)
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

            let len = arr.len() as i64;

            // Normalize start
            let start = if start < 0 {
                0.max(len + start)
            } else {
                // start >= 0
                start.min(len - 1)
            };

            // Normalize end
            let end = match end {
                0 => len,
                e if e < 0 => len + end,
                _ => end.min(len),
            };

            if end < start {
                // don't search at all
                return Ok(-1);
            }

            let slice = &arr[start as usize..end as usize];

            match slice.iter().position(|r| r == &v) {
                Some(i) => Ok((start as usize + i) as i64),
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

    pub fn value_op<F, R, T>(&mut self, path: &str, mut op_fun: F, res_func: R) -> Result<T, Error>
    where
        F: FnMut(&mut Value) -> Result<Value, Error>,
        R: Fn(&Value) -> Result<T, Error>,
    {
        let mut errors = vec![];
        let mut result = None;

        // A wrapper function that is called by replace_with
        // calls op_fun and then res_func
        let mut collect_fun = |mut value: Value| {
            op_fun(&mut value)
                .and_then(|new_value| {
                    // after calling op_fun calling res_func
                    // to prepae the command result
                    res_func(&new_value).map(|res| {
                        result = Some(res);
                        new_value
                    })
                })
                .map_err(|e| {
                    errors.push(e);
                })
                .unwrap_or(value)
        };

        if path == "$" {
            // root needs special handling
            self.data = collect_fun(self.data.take());
        } else {
            match SelectorMut::new().str_path(path) {
                Ok(selector) => {
                    let mut updater = JsonValueUpdater::new(|v| Some(collect_fun(v)));
                    let replace_result = selector.value(&mut self.data).replace_with(&mut updater);

                    if let Err(e) = replace_result {
                        errors.push(e.into());
                    }
                }
                Err(e) => {
                    errors.push(e.into());
                }
            }
        };

        match errors.len() {
            0 => match result {
                Some(r) => Ok(r),
                None => Err(format!("Path '{}' does not exist", path).into()),
            },
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
        let mut selector = Selector::new();
        selector.str_path(path)?;
        selector.value(&self.data);
        let results = selector.select()?;
        Ok(results)
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

    pub unsafe extern "C" fn aux_load(rdb: *mut raw::RedisModuleIO, encver: i32, when: i32) -> i32 {
        match json_aux_load(rdb, encver, when) {
            Ok(v) => v,
            Err(_) => Status::Err.into(),
        }
    }

    #[allow(non_snake_case, unused)]
    fn json_aux_load(rdb: *mut raw::RedisModuleIO, encver: i32, when: i32) -> Result<i32, Error> {
        if (encver > REDIS_JSON_TYPE_VERSION) {
            return Err(Error::from(
                "could not load rdb created with higher RedisJSON version!",
            ));
        }

        // Backward support for modules that had AUX field for RediSarch
        // TODO remove in future versions
        if (encver == 2 && when == raw::Aux::Before as i32) {
            let map_size = raw::load_unsigned(rdb)?;
            for _ in 0..map_size {
                let index_name = raw::load_string(rdb)?;
                let fields_size = raw::load_unsigned(rdb)?;
                for _ in 0..fields_size {
                    let field_name = raw::load_string(rdb)?;
                    let path = raw::load_string(rdb)?;
                }
            }
        }

        Ok(Status::Ok.into())
    }
}
