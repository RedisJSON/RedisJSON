use jsonpath_lib;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("ERR {0}")]
    GeneralError(String),

    #[error("ERR {0}")]
    InvalidJson(#[from] serde_json::Error),

    // Escape newlines so it'll be a valid redis error response
    #[error("ERR JSON path error: {}", format!("{:?}",.0).replace("\n", "\\n"))]
    InvalidJsonPath(jsonpath_lib::JsonPathError),

    #[error("ERR path not an object")]
    PathNotAnObject,

    #[error("ERR wrong static path")]
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

impl From<jsonpath_lib::JsonPathError> for Error {
    fn from(e: jsonpath_lib::JsonPathError) -> Self {
        Error::InvalidJsonPath(e)
    }
}

impl From<Error> for redis_module::RedisError {
    fn from(e: Error) -> Self {
        redis_module::RedisError::String(e.to_string())
    }
}
