// RedisJSON Redis module.
//
// Translate between JSON and tree of Redis objects:
// User-provided JSON is converted to a tree. This tree is stored transparently in Redis.
// It can be operated on (e.g. INCR) and serialized back to JSON.

use serde_json::Value;

pub struct Error {
    msg: String,
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error { msg: format!("{}", e.to_string()) }
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

    pub fn to_string(&self) -> Result<String, Error> {
        eprintln!("Serializing back to JSON");

        let s = serde_json::to_string(&self.data)?;
        return Ok(s)
    }
}