use std::os::raw::c_int;
use std::ptr::null_mut;
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use redis_module::key::RedisKeyWritable;
use redis_module::{raw as rawmod, RedisError};
use redis_module::{Context, NotifyEvent, Status};
use serde_json::Value;

use crate::{redisjson::RedisJSON, REDIS_JSON_TYPE};

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
    Float = 2,
    Bool = 3,
    Object = 4,
    Array = 5,
    Null = 6,
    Err = 7,
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
pub extern "C" fn JSONAPI_closePath(path: JSONApiPathRef) {
    if !path.is_null() {
        unsafe {
            Box::from_raw(path);
        }
    }
}

//---------------------------------------------------------------------------------------------

pub struct JSONApiPath<'a> {
    json_key: &'a JSONApiKey<'a>,
    path: &'a Value,
}

type JSONApiPathRef<'a> = *mut JSONApiPath<'a>;

impl<'a> JSONApiPath<'a> {
    pub fn new(
        json_key: &'a JSONApiKey<'a>,
        path: *const c_char,
    ) -> Result<JSONApiPath<'a>, RedisError> {
        let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
        if let Ok(value) = json_key.redis_json.get_first(path) {
            Ok(JSONApiPath {
                json_key,
                path: value,
            })
        } else {
            Err(RedisError::Str("JSON path not found"))
        }
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getPath(json_key: JSONApiKeyRef, path: *const c_char) -> JSONApiPathRef {
    if !json_key.is_null() {
        let json = unsafe { &*json_key };
        match JSONApiPath::new(json, path) {
            Ok(path) => Box::into_raw(Box::new(path)) as JSONApiPathRef,
            _ => null_mut(),
        }
    } else {
        null_mut()
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getInfo(
    json: JSONApiPathRef,
    _name: *mut c_void,
    jtype: *mut c_int,
    size: *mut libc::size_t,
) -> c_int {
    let res;
    let info;
    if !json.is_null() {
        let json = unsafe { &*json };
        info = RedisJSON::get_type_and_size(json.path);
        res = 0;
    } else {
        info = (JSONType::Err as c_int, 0 as libc::size_t);
        res = -1;
    }
    unsafe {
        *jtype = info.0;
        *size = info.1;
    }
    res
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
    getPath: JSONAPI_getPath,
    getInfo: JSONAPI_getInfo,
    closeKey: JSONAPI_closeKey,
    closePath: JSONAPI_closePath,
};

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_snake_case)]
pub struct RedisJSONAPI_V1<'a> {
    pub openKey: extern "C" fn(
        ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
    ) -> JSONApiKeyRef<'a>,
    pub getPath: extern "C" fn(json_key: JSONApiKeyRef, path: *const c_char) -> JSONApiPathRef,
    pub getInfo: extern "C" fn(
        json: JSONApiPathRef,
        name: *mut c_void,
        jtype: *mut c_int,
        size: *mut libc::size_t,
    ) -> c_int,
    pub closeKey: extern "C" fn(key: JSONApiKeyRef),
    pub closePath: extern "C" fn(key: JSONApiPathRef),
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}
