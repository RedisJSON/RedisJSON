use std::ffi::CString;
use std::os::raw::{c_double, c_int, c_long};
use std::ptr::null_mut;
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use redis_module::key::RedisKeyWritable;
use redis_module::logging::log_notice;
use redis_module::{raw as rawmod, RedisError};
use redis_module::{Context, NotifyEvent, Status};
use serde_json::Value;

use crate::{redisjson::RedisJSON, REDIS_JSON_TYPE};

// extern crate readies_wd40;
// use crate::readies_wd40::{BB, _BB, getenv};

//
// structs
//

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RedisModuleCtx {
    _unused: [u8; 0],
}

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

#[repr(C)]
#[allow(non_camel_case_types)]
pub enum RedisReturnCode {
    REDISMODULE_OK = 0,
    REDISMODULE_ERR = 1,
}

//---------------------------------------------------------------------------------------------

pub struct JSONApiKey<'a> {
    _key: RedisKeyWritable,
    redis_json: &'a mut RedisJSON,
}

pub type JSONApiKeyRef<'a> = *mut JSONApiKey<'a>;

impl<'a> JSONApiKey<'a> {
    pub fn new(
        ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
    ) -> Result<JSONApiKey<'a>, RedisError> {
        let ctx = Context::new(ctx);
        let key = ctx.open_with_redis_string(key_str);
        let res = key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;

        if let Some(value) = res {
            Ok(JSONApiKey {
                _key: key,
                redis_json: value,
            })
        } else {
            Err(RedisError::Str("Not a JSON key"))
        }
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_openKey<'a>(
    ctx: *mut rawmod::RedisModuleCtx,
    key_str: *mut rawmod::RedisModuleString,
) -> JSONApiKeyRef<'a> {
    match JSONApiKey::new(ctx, key_str) {
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
    path: JSONApiPathRef,
    index: libc::size_t,
    jtype: *mut c_int,
    count: *mut libc::size_t,
) -> JSONApiPathRef {
    if !path.is_null() {
        let path = unsafe { &*path };
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
    path: JSONApiPathRef,
    str: *mut *const c_char,
    len: *mut libc::size_t,
) -> c_int {
    if !path.is_null() {
        let path = unsafe { &mut *path };
        if let Some(ref s) = path.cstr_val {
            // Use cached string value
            unsafe {
                *str = s.as_bytes_with_nul().as_ptr() as *const c_char;
                *len = s.as_bytes().len();
            }
            return RedisReturnCode::REDISMODULE_OK as c_int;
        } else {
            let res: c_int = match path.value {
                Value::String(s) => path.set_string(s.as_str(), str, len),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        path.set_string(i.to_string().as_str(), str, len)
                    } else {
                        return RedisReturnCode::REDISMODULE_OK as c_int;
                    }
                }
                Value::Bool(b) => path.set_string(b.to_string().as_str(), str, len),
                _ => RedisReturnCode::REDISMODULE_ERR as c_int,
            };
            return res;
        }
    }
    RedisReturnCode::REDISMODULE_ERR as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getRedisModuleString(
    ctx: *mut rawmod::RedisModuleCtx,
    path: JSONApiPathRef,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    if !path.is_null() {
        let path = unsafe { &*path };
        match path.value {
            Value::String(s) => path.create_rmstring(ctx, s.as_str(), str),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    path.create_rmstring(ctx, i.to_string().as_str(), str)
                } else {
                    RedisReturnCode::REDISMODULE_OK as c_int
                }
            }
            Value::Bool(b) => path.create_rmstring(ctx, b.to_string().as_str(), str),
            _ => RedisReturnCode::REDISMODULE_ERR as c_int,
        }
    } else {
        RedisReturnCode::REDISMODULE_ERR as c_int
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getInt(json: JSONApiPathRef, val: *mut c_long) -> c_int {
    if !json.is_null() {
        let path = unsafe { &mut *json };
        if let Value::Number(n) = path.value {
            if let Some(i) = n.as_i64() {
                unsafe { *val = i };
                return RedisReturnCode::REDISMODULE_OK as c_int;
            }
        }
    }
    return RedisReturnCode::REDISMODULE_ERR as c_int;
}

#[no_mangle]
pub extern "C" fn JSONAPI_getDouble(json: JSONApiPathRef, val: *mut c_double) -> c_int {
    if !json.is_null() {
        let path = unsafe { &mut *json };
        if let Value::Number(n) = path.value {
            if let Some(f) = n.as_f64() {
                unsafe { *val = f };
                return RedisReturnCode::REDISMODULE_OK as c_int;
            }
        }
    }
    return RedisReturnCode::REDISMODULE_ERR as c_int;
}

#[no_mangle]
pub extern "C" fn JSONAPI_getBoolean(json: JSONApiPathRef, val: *mut c_int) -> c_int {
    if !json.is_null() {
        let path = unsafe { &*json };
        if let Value::Bool(b) = path.value {
            unsafe { *val = if *b { 1 } else { 0 } };
            return RedisReturnCode::REDISMODULE_OK as c_int;
        }
    }
    return RedisReturnCode::REDISMODULE_ERR as c_int;
}

#[no_mangle]
pub extern "C" fn JSONAPI_replyWith(
    ctx: *mut rawmod::RedisModuleCtx,
    json: JSONApiPathRef,
) -> c_int {
    //FIXME:
    0
}

#[no_mangle]
pub extern "C" fn JSONAPI_isJSON(redis_module_key: *mut c_void) -> c_int {
    //FIXME: Call redis_module::key::verify_type
    0
}
//---------------------------------------------------------------------------------------------

pub struct JSONApiPath<'a> {
    json_key: &'a JSONApiKey<'a>,
    value: &'a Value,
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
                return RedisReturnCode::REDISMODULE_OK as c_int;
            }
        }
        return RedisReturnCode::REDISMODULE_ERR as c_int;
    }

    pub fn create_rmstring(
        &self,
        ctx: *mut rawmod::RedisModuleCtx,
        from_str: &str,
        str: *mut *mut rawmod::RedisModuleString,
    ) -> c_int {
        if let Ok(s) = CString::new(from_str) {
            let p = s.as_bytes_with_nul().as_ptr() as *const c_char;
            let len = s.as_bytes().len();
            unsafe { *str = rawmod::RedisModule_CreateString.unwrap()(ctx, p, len) };
            return RedisReturnCode::REDISMODULE_OK as c_int;
        }
        return RedisReturnCode::REDISMODULE_ERR as c_int;
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
    closeKey: JSONAPI_closeKey,
    get: JSONAPI_get,
    getAt: JSONAPI_getAt,
    close: JSONAPI_close,
    getString: JSONAPI_getString,
    getRedisModuleString: JSONAPI_getRedisModuleString,
    getInt: JSONAPI_getInt,
    getDouble: JSONAPI_getDouble,
    getBoolean: JSONAPI_getBoolean,
    replyWith: JSONAPI_replyWith,
};

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_snake_case)]
pub struct RedisJSONAPI_V1<'a> {
    pub openKey: extern "C" fn(
        ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
    ) -> JSONApiKeyRef<'a>,
    pub closeKey: extern "C" fn(key: JSONApiKeyRef),
    pub get: extern "C" fn(
        key: JSONApiKeyRef,
        path: *const c_char,
        jtype: *mut c_int,
        count: *mut libc::size_t,
    ) -> JSONApiPathRef,
    pub getAt: extern "C" fn(
        json_key: JSONApiPathRef,
        index: libc::size_t,
        jtype: *mut c_int,
        count: *mut libc::size_t,
    ) -> JSONApiPathRef,
    pub close: extern "C" fn(key: JSONApiPathRef),
    // Get
    pub getInt: extern "C" fn(json: JSONApiPathRef, val: *mut c_long) -> c_int,
    pub getDouble: extern "C" fn(json: JSONApiPathRef, val: *mut c_double) -> c_int,
    pub getBoolean: extern "C" fn(json: JSONApiPathRef, val: *mut c_int) -> c_int,
    pub getString: extern "C" fn(
        json: JSONApiPathRef,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int,
    pub getRedisModuleString: extern "C" fn(
        ctx: *mut rawmod::RedisModuleCtx,
        json: JSONApiPathRef,
        str: *mut *mut rawmod::RedisModuleString,
    ) -> c_int,
    //
    pub replyWith: extern "C" fn(ctx: *mut rawmod::RedisModuleCtx, path: JSONApiPathRef) -> c_int,
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}
