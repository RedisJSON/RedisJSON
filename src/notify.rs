use redis_module::{raw as rawmod, RedisError, RedisString};
use redis_module::{Context, NotifyEvent, Status};
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use crate::{
    redisjson::{Format, Path, RedisJSON},
    REDIS_JSON_TYPE,
};

use crate::Error;
use redis_module::key::RedisKeyWritable;
use serde_json::Value;
use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr::{null, null_mut};

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

#[no_mangle]
pub extern "C" fn getInfo(
    redisjson: *mut c_void,
    _name: *mut c_void,
    jtype: *mut c_int,
    size: *mut libc::size_t,
) -> c_int {
    let t: c_int;
    if !redisjson.is_null() {
        let json = unsafe { &*(redisjson as *mut RedisJSON) };
        t = json.get_type_as_numeric();
    } else {
        t = JSONType::Err as c_int
    }
    unsafe {
        *jtype = t;
    }
    0
}

#[no_mangle]
pub extern "C" fn free(redisjson: *mut c_void) {
    if !redisjson.is_null() {
        unsafe {
            Box::from_raw(redisjson);
        }
    }
}

struct JSONApiKey<'a> {
    key: RedisKeyWritable,
    redis_json: &'a mut RedisJSON,
}

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
                key,
                redis_json: value,
            })
        } else {
            Err(RedisError::Str("Not a JSON key"))
        }
    }
}

// struct JSONApiPath<'a> {
//     api_key: &'a JSONApiKey,
//     path_value: Value,
// }

type JSONApiKeyRef = *mut c_void;

#[no_mangle]
pub extern "C" fn openKey(
    ctx: *mut rawmod::RedisModuleCtx,
    key_str: *mut rawmod::RedisModuleString,
) -> *mut c_void {
    match JSONApiKey::new(ctx, key_str) {
        Ok(key) => Box::into_raw(Box::new(key)) as *mut c_void,
        Err(e) => null_mut() as *mut c_void,
    }
}

#[no_mangle]
pub extern "C" fn getPath(
    module_ctx: *mut rawmod::RedisModuleCtx,
    key_str: *mut rawmod::RedisModuleString,
    path: *const c_char,
) -> *mut c_void {
    let ctx = Context::new(module_ctx);
    let key = ctx.open_with_redis_string(key_str);
    if let Ok(res) = key.get_value::<RedisJSON>(&REDIS_JSON_TYPE) {
        if let Some(value) = res {
            let p = unsafe { CStr::from_ptr(path).to_str().unwrap() };
            if let Ok(value) = value.get_first(p) {
                Box::into_raw(Box::new(value)) as *mut c_void
            } else {
                null_mut()
            }
        } else {
            null_mut()
        }
    } else {
        null_mut()
    }
}

static REDISJSON_GETAPI: &str = concat!("RedisJSON_V1", "\0");

pub fn export_shared_api(ctx: &Context) {
    ctx.export_shared_api(
        &JSONAPI as *const RedisModuleAPI_V1 as *const c_void,
        REDISJSON_GETAPI.as_ptr() as *mut i8,
    );
}

static JSONAPI: RedisModuleAPI_V1 = RedisModuleAPI_V1 {
    get_path: getPath,
    get_info: getInfo,
    free,
};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn RedisJSON_GetApiV1(_module_ctx: *mut RedisModuleCtx) -> *const RedisModuleAPI_V1 {
    &JSONAPI
}

// #[no_mangle]
// #[allow(non_snake_case)]
// pub extern "C" fn RedisJSON_GetApiV1(_module_ctx: *mut RedisModuleCtx) -> *mut RedisModuleAPI_V1 {
//     Box::into_raw(Box::new(RedisModuleAPI_V1 {
//         getPath: Some(getPath),
//     }))
// }

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RedisModuleAPI_V1 {
    pub get_path: extern "C" fn(
        module_ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
        path: *const c_char,
    ) -> *mut c_void,
    pub get_info: extern "C" fn(*mut c_void, *mut c_void, *mut c_int, *mut libc::size_t) -> c_int,
    pub free: extern "C" fn(*mut c_void),
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}
