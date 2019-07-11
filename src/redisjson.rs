// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.
use jsonpath_lib::JsonPathError;
use serde_json::{Number, Value};
use std::mem;

pub struct Error {
    msg: String,
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
    pub fn from_str(data: &str) -> Result<Self, Error> {
        // Parse the string of data into serde_json::Value.
        let v: Value = serde_json::from_str(data)?;

        Ok(Self { data: v })
    }

    pub fn set_value(&mut self, data: &str, path: &str) -> Result<(), Error> {
        // Parse the string of data into serde_json::Value.
        let json: Value = serde_json::from_str(data)?;

        let current_data = mem::replace(&mut self.data, Value::Null);
        let new_data = jsonpath_lib::replace_with(current_data, path, &mut |_v| json.clone())?;
        self.data = new_data;

        Ok(())
    }

    pub fn delete_path(&mut self, path: &str) -> Result<usize, Error> {
        let current_data = mem::replace(&mut self.data, Value::Null);

        let mut deleted = 0;
        self.data = jsonpath_lib::replace_with(current_data, path, &mut |v| {
            if !v.is_null() {
                deleted = deleted + 1; // might delete more than a single value
            }
            Value::Null
        })?;
        Ok(deleted)
    }

    pub fn to_string(&self, path: &str) -> Result<String, Error> {
        let results = self.get_doc(path)?;
        Ok(serde_json::to_string(&results)?)
    }

    pub fn str_len(&self, path: &str) -> Result<usize, Error> {
        match self.get_doc(path)?.as_str() {
            Some(s) => Ok(s.len()),
            None => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn arr_len(&self, path: &str) -> Result<usize, Error> {
        match self.get_doc(path)?.as_array() {
            Some(s) => Ok(s.len()),
            None => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn obj_len(&self, path: &str) -> Result<usize, Error> {
        match self.get_doc(path)?.as_object() {
            Some(s) => Ok(s.len()),
            None => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn get_type(&self, path: &str) -> Result<String, Error> {
        let s = RedisJSON::value_name(self.get_doc(path)?);
        Ok(s.to_string())
    }

    fn value_name(value: &Value) -> &str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    pub fn num_op<F: Fn(f64, f64) -> f64>(
        &mut self,
        path: &str,
        number: f64,
        fun: F,
    ) -> Result<String, Error> {
        let current_data = mem::replace(&mut self.data, Value::Null);

        let mut errors = vec![];
        let mut result: f64 = 0.0;

        self.data = jsonpath_lib::replace_with(current_data, path, &mut |v| {
            match apply_op(v, number, &fun) {
                Ok((res, new_value)) => {
                    result = res;
                    new_value
                }
                Err(e) => {
                    errors.push(e);
                    v.clone()
                }
            }
        })?;
        if errors.is_empty() {
            Ok(result.to_string())
        } else {
            Err(errors.join("\n").into())
        }
    }

    fn get_doc<'a>(&'a self, path: &'a str) -> Result<&'a Value, Error> {
        let results = jsonpath_lib::select(&self.data, path)?;
        match results.first() {
            Some(s) => Ok(s),
            None => Ok(&Value::Null),
        }
    }
}

fn apply_op<F>(v: &Value, number: f64, fun: F) -> Result<(f64, Value), String>
    where F: Fn(f64, f64) -> f64 {
    if let Value::Number(curr) = v {
        if let Some(curr_value) = curr.as_f64() {
            let res = fun(curr_value, number);

            if let Some(new_value) = Number::from_f64(res) {
                Ok((res, Value::Number(new_value)))
            } else {
                Err("ERR can not represent result as Number".to_string())
            }
        } else {
            Err("ERR can not convert current value as f64".to_string())
        }
    } else {
        Err(format!("ERR wrong type of path value - expected a number but found {}",
                    RedisJSON::value_name(&v)))
    }
}

