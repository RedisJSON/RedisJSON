// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use serde_json::Value;
use jsonpath_lib as jsonpath;

pub struct Error {
    msg: String,
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error { msg: e }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error { msg: e.to_string() }
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
        eprintln!("Parsing JSON from input '{}'", data);

        // Parse the string of data into serde_json::Value.
        let v: Value = serde_json::from_str(data)?;

        Ok(Self { data: v })
    }

    pub fn set_value(&mut self, data: &str) -> Result<(), Error> {
        eprintln!("Parsing JSON from input '{}'", data);

        // Parse the string of data into serde_json::Value.
        let v: Value = serde_json::from_str(data)?;

        self.data = v;

        Ok(())
    }

    pub fn to_string(&self, path: &str) -> Result<String, Error> {
        eprintln!("Serializing back to JSON");

        let results = self.get_doc(path)?;
        Ok(serde_json::to_string(&results)?)
    }

    pub fn str_len(&self, path: &str) -> Result<usize, Error> {
        match self.get_doc(path)?.as_str() {
            Some(s) => Ok(s.len()),
            None => Err(Error{msg: "ERR wrong type of path value".to_string()})
        }
    }

    pub fn get_type(&self, path: &str) -> Result<String, Error> {
        let s = match self.get_doc(path)? {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        Ok(s.to_string())
    }

    pub fn get_doc(&self, path: &str) -> Result<Value, Error> {
        let results = jsonpath::select(&self.data, path)?;
        Ok(results)
    }
}