// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.
use crate::backward;
use crate::nodevisitor::NodeVisitorImpl;
use jsonpath_lib::{JsonPathError, SelectorMut};
use redismodule::raw;
use serde_json::Value;
use std::os::raw::{c_int, c_void};

#[derive(Debug)]
pub struct Error {
    msg: String,
}

#[derive(Debug, PartialEq)]
pub enum SetOptions {
    NotExists,
    AlreadyExists,
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error { msg: e }
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Self {
        Error { msg: e.to_string() }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error { msg: e.to_string() }
    }
}

impl From<JsonPathError> for Error {
    fn from(e: JsonPathError) -> Self {
        Error {
            msg: format!("{:?}", e),
        }
    }
}

impl From<Error> for redismodule::RedisError {
    fn from(e: Error) -> Self {
        redismodule::RedisError::String(e.msg)
    }
}

#[derive(Debug)]
pub struct RedisJSON {
    data: Value,
}

impl RedisJSON {
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
                        map.insert(key.to_string(), value.clone());
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

    pub fn from_str(data: &str) -> Result<Self, Error> {
        // Parse the string of data into serde_json::Value.
        let v: Value = serde_json::from_str(data)?;

        Ok(Self { data: v })
    }

    pub fn set_value(
        &mut self,
        data: &str,
        path: &str,
        option: &Option<SetOptions>,
    ) -> Result<bool, Error> {
        // Parse the string of data into serde_json::Value.
        let json: Value = serde_json::from_str(data)?;

        if path == "$" {
            if Some(SetOptions::NotExists) == *option {
                Ok(false)
            } else {
                self.data = json;
                Ok(true)
            }
        } else {
            let mut replaced = false;
            if Some(SetOptions::NotExists) != *option {
                let current_data = self.data.take();
                self.data = jsonpath_lib::replace_with(current_data, path, &mut |_v| {
                    replaced = true;
                    Some(json.clone())
                })?;
            }
            if replaced {
                Ok(true)
            } else if Some(SetOptions::AlreadyExists) != *option {
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
                deleted = deleted + 1; // might delete more than a single value
            }
            None
        })?;
        Ok(deleted)
    }

    pub fn to_string(&self, path: &str) -> Result<String, Error> {
        let results = self.get_doc(path)?;
        Ok(serde_json::to_string(&results)?)
    }

    pub fn str_len(&self, path: &str) -> Result<usize, Error> {
        self.get_doc(path)?
            .as_str()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|s| s.len())
    }

    pub fn arr_len(&self, path: &str) -> Result<usize, Error> {
        self.get_doc(path)?
            .as_array()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|arr| arr.len())
    }

    pub fn obj_len(&self, path: &str) -> Result<usize, Error> {
        self.get_doc(path)?
            .as_object()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|obj| obj.len())
    }

    pub fn obj_keys<'a>(&'a self, path: &'a str) -> Result<Vec<&'a String>, Error> {
        self.get_doc(path)?
            .as_object()
            .ok_or_else(|| "ERR wrong type of path value".into())
            .map(|obj| obj.keys().collect())
    }

    pub fn arr_index(
        &self,
        path: &str,
        scalar: &str,
        start: usize,
        end: usize,
    ) -> Result<i64, Error> {
        if let Value::Array(arr) = self.get_doc(path)? {
            match serde_json::from_str(scalar)? {
                Value::Array(_) | Value::Object(_) => Ok(-1),
                v => {
                    let mut start = start.max(0);
                    let end = end.min(arr.len() - 1);
                    start = end.min(start);

                    let slice = &arr[start..=end];
                    match slice.iter().position(|r| r == &v) {
                        Some(i) => Ok((start + i) as i64),
                        None => Ok(-1),
                    }
                }
            }
        } else {
            Ok(-1)
        }
    }

    pub fn get_type(&self, path: &str) -> Result<String, Error> {
        let s = RedisJSON::value_name(self.get_doc(path)?);
        Ok(s.to_string())
    }

    pub fn value_name(value: &Value) -> &str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
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

    pub fn get_doc<'a>(&'a self, path: &'a str) -> Result<&'a Value, Error> {
        let results = jsonpath_lib::select(&self.data, path)?;
        match results.first() {
            Some(s) => Ok(s),
            None => Err("ERR path does not exist".into()),
        }
    }
}

#[allow(non_snake_case, unused)]
pub unsafe extern "C" fn json_rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
    let json = match encver {
        0 => RedisJSON {
            data: backward::json_rdb_load(rdb),
        },
        2 => RedisJSON::from_str(&raw::load_string(rdb)).unwrap(),
        _ => panic!("Can't load old RedisJSON RDB"),
    };
    Box::into_raw(Box::new(json)) as *mut c_void
}

#[allow(non_snake_case, unused)]
#[no_mangle]
pub unsafe extern "C" fn json_free(value: *mut c_void) {
    Box::from_raw(value as *mut RedisJSON);
}

#[allow(non_snake_case, unused)]
#[no_mangle]
pub unsafe extern "C" fn json_rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
    let json = &*(value as *mut RedisJSON);
    raw::save_string(rdb, &json.data.to_string());
}
