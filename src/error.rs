use jsonpath_lib::select::JsonPathError;
use std::num::ParseIntError;

#[derive(Debug)]
pub struct Error {
    pub msg: String,
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error { msg: e }
    }
}

impl From<redis_module::error::GenericError> for Error {
    fn from(err: redis_module::error::GenericError) -> Error {
        err.to_string().into()
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(err: std::string::FromUtf8Error) -> Error {
        err.to_string().into()
    }
}

impl From<ParseIntError> for Error {
    fn from(err: ParseIntError) -> Error {
        err.to_string().into()
    }
}

impl From<redis_module::error::Error> for Error {
    fn from(e: redis_module::error::Error) -> Self {
        match e {
            redis_module::error::Error::Generic(err) => err.into(),
            redis_module::error::Error::FromUtf8(err) => err.into(),
            redis_module::error::Error::ParseInt(err) => err.into(),
        }
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
