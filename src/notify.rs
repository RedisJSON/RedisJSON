use std::ffi::CString;
use std::os::raw::{c_double, c_int, c_long};
use std::ptr::null_mut;
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use redis_module::key::RedisKeyWritable;
use redis_module::{raw as rawmod, RedisError};
use redis_module::{Context, NotifyEvent, Status};
use serde_json::{from_slice, Value};

use crate::{redisjson::RedisJSON, REDIS_JSON_TYPE};
use std::ops::Index;

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
        if let Some(ref s) = path.str_val {
            // Use cached string value
            unsafe {
                *str = s.as_bytes_with_nul().as_ptr() as *const c_char;
                *len = s.as_bytes().len();
            }
            return RedisReturnCode::REDISMODULE_OK as c_int;
        } else {
            if let Value::String(s) = path.value {
                // Cache string value
                if let Ok(s) = CString::new(s.as_str()) {
                    unsafe {
                        *str = s.as_bytes_with_nul().as_ptr() as *const c_char;
                        *len = s.as_bytes().len();
                    }
                    path.str_val = Some(s);
                    return RedisReturnCode::REDISMODULE_OK as c_int;
                }
            }
        }
    }
    RedisReturnCode::REDISMODULE_ERR as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getInt(path: JSONApiPathRef, val: *mut c_int) -> c_long {
    0 //FIXME:
}

#[no_mangle]
pub extern "C" fn JSONAPI_getDouble(path: JSONApiPathRef, val: *mut c_double) -> c_int {
    0 //FIXME:
}

#[no_mangle]
pub extern "C" fn JSONAPI_getBoolean(path: JSONApiPathRef, val: *mut c_int) -> c_int {
    0 //FIXME:
}

#[no_mangle]
pub extern "C" fn JSONAPI_replyWith(path: JSONApiPathRef) -> c_int {
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
    str_val: Option<CString>,
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
                str_val: None,
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
                        str_val: None,
                    })
                } else {
                    Err(RedisError::Str("JSON index is out of range"))
                }
            }
            Value::Object(ref map) => {
                //FIXME: get the i'th entry in map
                if index < map.len() {
                    Ok(JSONApiPath {
                        json_key: json_key,
                        value: map.iter().nth(index).unwrap().1,
                        str_val: None,
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
    pub getInt: extern "C" fn(json: JSONApiPathRef, val: *mut c_int) -> c_long,
    pub getDouble: extern "C" fn(json: JSONApiPathRef, val: *mut c_double) -> c_int,
    pub getBoolean: extern "C" fn(json: JSONApiPathRef, val: *mut c_int) -> c_int,
    pub getString: extern "C" fn(
        json: JSONApiPathRef,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int,
    //
    pub replyWith: extern "C" fn(path: JSONApiPathRef) -> c_int,
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}
