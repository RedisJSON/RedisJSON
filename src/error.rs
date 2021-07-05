use jsonpath_lib::select::JsonPathError;

#[derive(Debug)]
pub struct Error {
    pub msg: String,
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
            msg: format!("JSON Path error: {:?}", e).replace("\n", "\\n"),
        }
    }
}

impl From<Error> for redis_module::RedisError {
    fn from(e: Error) -> Self {
        redis_module::RedisError::String(e.msg)
    }
}
