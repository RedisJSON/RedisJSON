use jsonpath_lib::JsonPathError;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    GeneralError(String),
    PathNotAnObject,
    WrongStaticPath,
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::GeneralError(e)
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Self {
        Error::GeneralError(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::GeneralError(e.to_string())
    }
}

impl From<JsonPathError> for Error {
    fn from(e: JsonPathError) -> Self {
        Error::GeneralError(format!("JSON path error: {:?}", e).replace("\n", "\\n"))
    }
}

impl From<Error> for redis_module::RedisError {
    fn from(e: Error) -> Self {
        redis_module::RedisError::String(e.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            Error::GeneralError(msg) => format!("ERR {}", msg),
            Error::PathNotAnObject => "ERR path not an object".to_string(),
            Error::WrongStaticPath => "ERR wrong static path".to_string(),
        };
        write!(f, "{}", msg)
    }
}
