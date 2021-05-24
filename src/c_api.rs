use std::ffi::CString;
use std::os::raw::{c_double, c_int, c_long};
use std::ptr::null_mut;
use std::str::FromStr;
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use crate::redisjson::Format;
use crate::{redisjson::RedisJSON, REDIS_JSON_TYPE};
use redis_module::key::verify_type;
use redis_module::key::RedisKeyWritable;
use redis_module::logging::log_notice;
use redis_module::{raw as rawmod, RedisError};
use redis_module::{Context, Status};
use serde_json::Value;

// extern crate readies_wd40;
// use crate::readies_wd40::{BB, _BB, getenv};

//
// structs
//

#[repr(C)]
pub enum JSONType {
    String = 0,
    Int = 1,
    Double = 2,
    Bool = 3,
    Object = 4,
    Array = 5,
    Null = 6,
}

//---------------------------------------------------------------------------------------------

pub struct JSONApiKey<'a> {
    _key: RedisKeyWritable,
    redis_json: &'a mut RedisJSON,
    cstr_val: Option<CString>,
    ctx: Context,
}

impl<'a> JSONApiKey<'a> {
    pub fn create_rmstring(
        &self,
        from_str: &str,
        str: *mut *mut rawmod::RedisModuleString,
    ) -> c_int {
        if let Ok(s) = CString::new(from_str) {
            let p = s.as_bytes_with_nul().as_ptr() as *const c_char;
            let len = s.as_bytes().len();
            unsafe { *str = rawmod::RedisModule_CreateString.unwrap()(self.ctx.get_raw(), p, len) };
            return Status::Ok as c_int;
        }
        Status::Err as c_int
    }

    fn get_value(&self, path: *const c_char) -> Option<&Value> {
        let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
        self.redis_json.get_first(path).ok()
    }
}
pub type JSONApiKeyRef<'a> = *mut JSONApiKey<'a>;

impl<'a> JSONApiKey<'a> {
    pub fn new_from_key(key: RedisKeyWritable, ctx: Context) -> Result<JSONApiKey<'a>, RedisError> {
        let res = key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;
        if let Some(value) = res {
            Ok(JSONApiKey {
                _key: key,
                redis_json: value,
                cstr_val: None,
                ctx: ctx,
            })
        } else {
            Err(RedisError::Str("Not a JSON key"))
        }
    }

    pub fn new_from_redis_string(
        ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
    ) -> Result<JSONApiKey<'a>, RedisError> {
        let ctx = Context::new(ctx);
        let key = ctx.open_with_redis_string(key_str);
        JSONApiKey::new_from_key(key, ctx)
    }

    pub fn new_from_str(
        ctx: *mut rawmod::RedisModuleCtx,
        path: *const c_char,
    ) -> Result<JSONApiKey<'a>, RedisError> {
        let ctx = Context::new(ctx);
        let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
        let key = ctx.open_key_writable(path);
        JSONApiKey::new_from_key(key, ctx)
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_openKey<'a>(
    ctx: *mut rawmod::RedisModuleCtx,
    key_str: *mut rawmod::RedisModuleString,
) -> JSONApiKeyRef<'a> {
    match JSONApiKey::new_from_redis_string(ctx, key_str) {
        Ok(key) => Box::into_raw(Box::new(key)) as JSONApiKeyRef,
        _ => null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_openKeyFromStr<'a>(
    ctx: *mut rawmod::RedisModuleCtx,
    path: *const c_char,
) -> JSONApiKeyRef<'a> {
    match JSONApiKey::new_from_str(ctx, path) {
        Ok(key) => Box::into_raw(Box::new(key)) as JSONApiKeyRef,
        _ => null_mut(),
    }
}
#[no_mangle]
pub extern "C" fn JSONAPI_closeKey(json: JSONApiKeyRef) {
    if !json.is_null() {
        unsafe {
            Box::from_raw(json);
        }
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getAt(
    json: JSONApiPathRef,
    index: libc::size_t,
    jtype: *mut c_int,
    count: *mut libc::size_t,
) -> JSONApiPathRef {
    if !json.is_null() {
        let path = unsafe { &*json };
        match JSONApiPath::new_from_index(path.json_key, index) {
            Ok(path) => {
                path.get_type_and_size(jtype, count);
                Box::into_raw(Box::new(path)) as JSONApiPathRef
            }
            _ => null_mut(),
        }
    } else {
        null_mut() as JSONApiPathRef
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_close(path: JSONApiPathRef) {
    if !path.is_null() {
        unsafe {
            Box::from_raw(path);
        }
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getString(
    json: JSONApiPathRef,
    str: *mut *const c_char,
    len: *mut libc::size_t,
) -> c_int {
    if !json.is_null() {
        let json = unsafe { &mut *json };
        if let Some(ref s) = json.cstr_val {
            // Use cached string value
            unsafe {
                *str = s.as_bytes_with_nul().as_ptr() as *const c_char;
                *len = s.as_bytes().len();
            }
            return Status::Ok as c_int;
        } else if let Some(ref p) = json.path {
            if let Ok(s) = json.json_key.redis_json.to_string(p.as_str(), Format::JSON) {
                return json.set_string(s.as_str(), str, len);
            }
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getStringFromKey(
    key: JSONApiKeyRef,
    path: *const c_char,
    str: *mut *const c_char,
    len: *mut libc::size_t,
) -> c_int {
    if !key.is_null() {
        let key = unsafe { &mut *key };
        let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
        if let Ok(s) = key.redis_json.to_string(path, Format::JSON) {
            if let Ok(s) = CString::new(s) {
                unsafe {
                    *str = s.as_bytes_with_nul().as_ptr() as *const c_char;
                    *len = s.as_bytes().len();
                }
                key.cstr_val = Some(s);
                return Status::Ok as c_int;
            }
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getRedisModuleString(
    json: JSONApiPathRef,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    if !json.is_null() {
        let json = unsafe { &*json };
        if let Some(ref p) = json.path {
            if let Ok(res) = json.json_key.redis_json.to_string(p, Format::JSON) {
                return json.json_key.create_rmstring(res.as_str(), str);
            }
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getRedisModuleStringFromKey(
    key: JSONApiKeyRef,
    path: *const c_char,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    if !key.is_null() {
        let key = unsafe { &*key };
        let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
        if let Ok(res) = key.redis_json.to_string(path, Format::JSON) {
            key.create_rmstring(res.as_str(), str)
        } else {
            Status::Err as c_int
        }
    } else {
        Status::Err as c_int
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_isJSON(key: *mut rawmod::RedisModuleKey) -> c_int {
    match verify_type(key, &REDIS_JSON_TYPE) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn get_int_value(value: &Value) -> Option<c_long> {
    match value {
        Value::Number(ref n) => {
            if let Some(i) = n.as_i64() {
                return Some(i);
            }
        }
        Value::String(ref s) => {
            if let Ok(v) = c_long::from_str(s.as_str()) {
                return Some(v);
            }
        }
        Value::Bool(ref b) => {
            if *b {
                return Some(1);
            } else {
                return Some(0);
            }
        }
        _ => {}
    }
    None
}

fn get_double_value(value: &Value) -> Option<c_double> {
    match value {
        Value::Number(n) if n.is_f64() => n.as_f64(),
        Value::String(s) => c_double::from_str(s.as_str()).ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn get_bool_value(value: &Value) -> Option<c_int> {
    match value {
        Value::Bool(b) => {
            if *b {
                Some(1)
            } else {
                Some(0)
            }
        }
        Value::Number(n) if n.is_i64() => Some(if n.as_i64().unwrap() != 0 { 1 } else { 0 }),
        Value::Number(n) if n.is_f64() => Some(if n.as_f64().unwrap() != 0.0 { 1 } else { 0 }),
        Value::String(s) => Some(if s.is_empty() { 0 } else { 1 }),
        _ => None,
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getInt(json: JSONApiPathRef, val: *mut c_long) -> c_int {
    if !json.is_null() {
        let json = unsafe { &mut *json };
        if let Some(v) = get_int_value(json.value) {
            unsafe { *val = v };
            return Status::Ok as c_int;
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getIntFromKey(
    key: JSONApiKeyRef,
    path: *const c_char,
    val: *mut c_long,
) -> c_int {
    if !key.is_null() {
        let key = unsafe { &mut *key };
        if let Some(value) = key.get_value(path) {
            if let Some(v) = get_int_value(value) {
                unsafe { *val = v };
                return Status::Ok as c_int;
            }
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getDouble(json: JSONApiPathRef, val: *mut c_double) -> c_int {
    if !json.is_null() {
        let json = unsafe { &mut *json };
        if let Some(v) = get_double_value(json.value) {
            unsafe { *val = v };
            return Status::Ok as c_int;
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getDoubleFromKey(
    key: JSONApiKeyRef,
    path: *const c_char,
    val: *mut c_double,
) -> c_int {
    if !key.is_null() {
        let key = unsafe { &mut *key };
        if let Some(value) = key.get_value(path) {
            if let Some(v) = get_double_value(value) {
                unsafe { *val = v };
                return Status::Ok as c_int;
            }
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getBoolean(json: JSONApiPathRef, val: *mut c_int) -> c_int {
    if !json.is_null() {
        let json = unsafe { &*json };
        if let Some(v) = get_bool_value(json.value) {
            unsafe { *val = v };
            return Status::Ok as c_int;
        }
    }
    Status::Err as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getBooleanFromKey(
    key: JSONApiKeyRef,
    path: *const c_char,
    val: *mut c_int,
) -> c_int {
    if !key.is_null() {
        let key = unsafe { &*key };
        if let Some(value) = key.get_value(path) {
            if let Some(v) = get_bool_value(value) {
                unsafe { *val = v };
                return Status::Ok as c_int;
            }
        }
    }
    Status::Err as c_int
}

//---------------------------------------------------------------------------------------------

pub struct JSONApiPath<'a> {
    json_key: &'a JSONApiKey<'a>,
    value: &'a Value,
    path: Option<String>, // Path is missing when key was obtained by index
    cstr_val: Option<CString>,
}

type JSONApiPathRef<'a> = *mut JSONApiPath<'a>;

impl<'a> JSONApiPath<'a> {
    pub fn new_from_path(
        json_key: &'a JSONApiKey<'a>,
        path: *const c_char,
    ) -> Result<JSONApiPath<'a>, RedisError> {
        let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
        if let Ok(value) = json_key.redis_json.get_first(path) {
            Ok(JSONApiPath {
                json_key,
                value: value,
                path: Some(String::from(path)),
                cstr_val: None,
            })
        } else {
            Err(RedisError::Str("JSON path not found"))
        }
    }

    pub fn new_from_index(
        json_key: &'a JSONApiKey<'a>,
        index: libc::size_t,
    ) -> Result<JSONApiPath<'a>, RedisError> {
        match json_key.redis_json.data {
            Value::Array(ref vec) => {
                //FIXME: move to RedisJSON struct?
                if index < vec.len() {
                    Ok(JSONApiPath {
                        json_key: json_key,
                        value: vec.get(index).unwrap(),
                        path: None,
                        cstr_val: None,
                    })
                } else {
                    Err(RedisError::Str("JSON index is out of range"))
                }
            }
            //FIXME: Return the name of the key in the map (allocate CString)
            Value::Object(ref map) => {
                if index < map.len() {
                    Ok(JSONApiPath {
                        json_key: json_key,
                        value: map.iter().nth(index).unwrap().1,
                        path: None,
                        cstr_val: None,
                    })
                } else {
                    Err(RedisError::Str("JSON index is out of range"))
                }
            }
            _ => Err(RedisError::Str("Not a JSON Array or Object")),
        }
    }

    pub fn get_type_and_size(&self, jtype: *mut c_int, count: *mut libc::size_t) {
        let info = RedisJSON::get_type_and_size(self.value);
        unsafe {
            *jtype = info.0 as c_int;
            if !count.is_null() {
                *count = info.1;
            }
        }
    }

    pub fn set_string(
        &mut self,
        from_str: &str,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int {
        if !str.is_null() {
            if let Ok(s) = CString::new(from_str) {
                unsafe {
                    *str = s.as_bytes_with_nul().as_ptr() as *const c_char;
                    *len = s.as_bytes().len();
                }
                self.cstr_val = Some(s);
                return Status::Ok as c_int;
            }
        }
        Status::Err as c_int
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_get(
    key: JSONApiKeyRef,
    path: *const c_char,
    jtype: *mut c_int,
    count: *mut libc::size_t,
) -> JSONApiPathRef {
    if !key.is_null() {
        let key = unsafe { &*key };
        match JSONApiPath::new_from_path(key, path) {
            Ok(path) => {
                path.get_type_and_size(jtype, count);
                Box::into_raw(Box::new(path)) as JSONApiPathRef
            }
            _ => null_mut(),
        }
    } else {
        null_mut()
    }
}

static REDISJSON_GETAPI: &str = concat!("RedisJSON_V1", "\0");

pub fn export_shared_api(ctx: &Context) {
    log_notice("Exported RedisJSON_V1 API");
    ctx.export_shared_api(
        &JSONAPI as *const RedisJSONAPI_V1 as *const c_void,
        REDISJSON_GETAPI.as_ptr() as *mut i8,
    );
}

static JSONAPI: RedisJSONAPI_V1 = RedisJSONAPI_V1 {
    openKey: JSONAPI_openKey,
    openKeyFromStr: JSONAPI_openKeyFromStr,
    closeKey: JSONAPI_closeKey,
    get: JSONAPI_get,
    getAt: JSONAPI_getAt,
    close: JSONAPI_close,
    getInt: JSONAPI_getInt,
    getIntFromKey: JSONAPI_getIntFromKey,
    getDouble: JSONAPI_getDouble,
    getDoubleFromKey: JSONAPI_getDoubleFromKey,
    getBoolean: JSONAPI_getBoolean,
    getBooleanFromKey: JSONAPI_getBooleanFromKey,
    getString: JSONAPI_getString,
    getStringFromKey: JSONAPI_getStringFromKey,
    getRedisModuleString: JSONAPI_getRedisModuleString,
    getRedisModuleStringFromKey: JSONAPI_getRedisModuleStringFromKey,
    isJSON: JSONAPI_isJSON,
};

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_snake_case)]
pub struct RedisJSONAPI_V1<'a> {
    pub openKey: extern "C" fn(
        ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
    ) -> JSONApiKeyRef<'a>,
    pub openKeyFromStr:
        extern "C" fn(ctx: *mut rawmod::RedisModuleCtx, path: *const c_char) -> JSONApiKeyRef<'a>,
    pub closeKey: extern "C" fn(key: JSONApiKeyRef),
    pub get: extern "C" fn(
        key: JSONApiKeyRef,
        path: *const c_char,
        jtype: *mut c_int,
        count: *mut libc::size_t,
    ) -> JSONApiPathRef,
    pub getAt: extern "C" fn(
        json: JSONApiPathRef,
        index: libc::size_t,
        jtype: *mut c_int,
        count: *mut libc::size_t,
    ) -> JSONApiPathRef,
    pub close: extern "C" fn(key: JSONApiPathRef),
    // Get
    pub getInt: extern "C" fn(json: JSONApiPathRef, val: *mut c_long) -> c_int,
    pub getIntFromKey:
        extern "C" fn(key: JSONApiKeyRef, path: *const c_char, val: *mut c_long) -> c_int,
    pub getDouble: extern "C" fn(json: JSONApiPathRef, val: *mut c_double) -> c_int,
    pub getDoubleFromKey:
        extern "C" fn(key: JSONApiKeyRef, path: *const c_char, val: *mut c_double) -> c_int,
    pub getBoolean: extern "C" fn(json: JSONApiPathRef, val: *mut c_int) -> c_int,
    pub getBooleanFromKey:
        extern "C" fn(key: JSONApiKeyRef, path: *const c_char, val: *mut c_int) -> c_int,
    pub getString: extern "C" fn(
        json: JSONApiPathRef,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int,
    pub getStringFromKey: extern "C" fn(
        key: JSONApiKeyRef,
        path: *const c_char,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int,
    pub getRedisModuleString:
        extern "C" fn(json: JSONApiPathRef, str: *mut *mut rawmod::RedisModuleString) -> c_int,
    pub getRedisModuleStringFromKey: extern "C" fn(
        key: JSONApiKeyRef,
        path: *const c_char,
        str: *mut *mut rawmod::RedisModuleString,
    ) -> c_int,
    pub isJSON: extern "C" fn(key: *mut rawmod::RedisModuleKey) -> c_int,
}
