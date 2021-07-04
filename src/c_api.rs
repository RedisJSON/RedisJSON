use std::ffi::CString;
use std::os::raw::{c_double, c_int, c_long};
use std::ptr::{null, null_mut};
use std::slice;
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use crate::commands::KeyValue;
use jsonpath_lib::select::select_value::{SelectValue, SelectValueType};
use jsonpath_lib::select::Selector;
use redis_module::key::verify_type;
use redis_module::redisraw::bindings::RedisModule_StringPtrLen;
use redis_module::{raw as rawmod, RedisError};
use redis_module::{Context, Status};
use serde_json::Value;

use crate::manager::{Manager, ReadHolder, RedisJsonKeyManager};
use crate::{redisjson::RedisJSON, REDIS_JSON_TYPE};

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

pub fn create_rmstring(
    ctx: *mut rawmod::RedisModuleCtx,
    from_str: &str,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    if let Ok(s) = CString::new(from_str) {
        let p = s.as_bytes_with_nul().as_ptr() as *const c_char;
        let len = s.as_bytes().len();
        unsafe { *str = rawmod::RedisModule_CreateString.unwrap()(ctx, p, len) };
        return Status::Ok as c_int;
    }
    Status::Err as c_int
}

fn json_api_open_key_internal<M: Manager>(
    manager: M,
    ctx: *mut rawmod::RedisModuleCtx,
    key: &str,
) -> *mut M::ReadHolder {
    let ctx = Context::new(ctx);
    if let Ok(h) = manager.open_key_read(&ctx, key) {
        if let Ok(v) = h.get_value() {
            if let Some(_) = v {
                return Box::into_raw(Box::new(h));
            }
        }
    }
    null_mut()
}

fn json_api_close_key_internal<M: Manager>(_: M, json: *mut c_void) {
    unsafe {
        Box::from_raw(json as *mut M::ReadHolder);
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_openKey<'a>(
    ctx: *mut rawmod::RedisModuleCtx,
    key_str: *mut rawmod::RedisModuleString,
) -> *mut c_void {
    let mut len = 0;
    let key = unsafe {
        let bytes = RedisModule_StringPtrLen.unwrap()(key_str, &mut len);
        let bytes = slice::from_raw_parts(bytes as *const u8, len);
        String::from_utf8_lossy(bytes).into_owned()
    };
    json_api_open_key_internal(RedisJsonKeyManager, ctx, &key) as *mut c_void
}

#[no_mangle]
pub extern "C" fn JSONAPI_openKeyFromStr<'a>(
    ctx: *mut rawmod::RedisModuleCtx,
    path: *const c_char,
) -> *mut c_void {
    let key = unsafe { CStr::from_ptr(path).to_str().unwrap() };
    json_api_open_key_internal(RedisJsonKeyManager, ctx, &key) as *mut c_void
}
#[no_mangle]
pub extern "C" fn JSONAPI_closeKey(json: *mut c_void) {
    json_api_close_key_internal(RedisJsonKeyManager, json);
}

fn json_api_get_at<M: Manager>(
    _: M,
    json: *const c_void,
    index: libc::size_t,
    jtype: *mut c_int,
) -> *const c_void {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Array => match json.get_index(index) {
            Some(v) => {
                if jtype != null_mut() {
                    unsafe { *jtype = json_api_get_type_internal(v) as c_int };
                }
                v as *const M::V as *const c_void
            }
            _ => null(),
        },
        _ => null(),
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getAt(
    json: *const c_void,
    index: libc::size_t,
    jtype: *mut c_int,
) -> *const c_void {
    json_api_get_at(RedisJsonKeyManager, json, index, jtype)
}

fn json_api_get_len<M: Manager>(_: M, json: *const c_void, count: *mut libc::size_t) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    let len = match json.get_type() {
        SelectValueType::String => Some(json.get_str().len()),
        SelectValueType::Array => Some(json.len().unwrap()),
        SelectValueType::Object => Some(json.len().unwrap()),
        _ => None,
    };
    match len {
        Some(l) => {
            unsafe { *count = l };
            Status::Ok as c_int
        }
        None => Status::Err as c_int,
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getLen(json: *const c_void, count: *mut libc::size_t) -> c_int {
    json_api_get_len(RedisJsonKeyManager, json, count)
}

fn json_api_get_type<M: Manager>(_: M, json: *const c_void) -> c_int {
    json_api_get_type_internal(unsafe { &*(json as *const M::V) }) as c_int
}

#[no_mangle]
pub extern "C" fn JSONAPI_getType(json: *const c_void) -> c_int {
    json_api_get_type(RedisJsonKeyManager, json)
}

fn json_api_get_string<M: Manager>(
    _: M,
    json: *const c_void,
    str: *mut *const c_char,
    len: *mut libc::size_t,
) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::String => {
            let s = json.as_str();
            set_string(s, str, len);
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getString(
    json: *const c_void,
    str: *mut *const c_char,
    len: *mut libc::size_t,
) -> c_int {
    json_api_get_string(RedisJsonKeyManager, json, str, len)
}

#[no_mangle]
pub extern "C" fn JSONAPI_getStringFromKey(
    key: *mut c_void,
    path: *const c_char,
    str: *mut *const c_char,
    len: *mut libc::size_t,
) -> c_int {
    let mut t: c_int = 0;
    let v = JSONAPI_get(key, path, &mut t);
    if v != null() && t == JSONType::String as c_int {
        JSONAPI_getString(v, str, len)
    } else {
        Status::Err as c_int
    }
}

fn json_api_get_json<M: Manager>(
    _: M,
    json: *const c_void,
    ctx: *mut rawmod::RedisModuleCtx,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    let res = KeyValue::new(json).to_value(json).to_string();
    create_rmstring(ctx, &res, str)
}

#[no_mangle]
pub extern "C" fn JSONAPI_getJSON(
    json: *const c_void,
    ctx: *mut rawmod::RedisModuleCtx,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    json_api_get_json(RedisJsonKeyManager, json, ctx, str)
}

#[no_mangle]
pub extern "C" fn JSONAPI_getJSONFromKey(
    key: *mut c_void,
    ctx: *mut rawmod::RedisModuleCtx,
    path: *const c_char,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    let mut t: c_int = 0;
    let v = JSONAPI_get(key, path, &mut t);
    if v != null() {
        JSONAPI_getJSON(v, ctx, str)
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

fn json_api_get_int<M: Manager>(_: M, json: *const c_void, val: *mut c_long) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Long => {
            unsafe { *val = json.get_long() };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getInt(json: *const c_void, val: *mut c_long) -> c_int {
    json_api_get_int(RedisJsonKeyManager, json, val)
}

#[no_mangle]
pub extern "C" fn JSONAPI_getIntFromKey(
    key: *mut c_void,
    path: *const c_char,
    val: *mut c_long,
) -> c_int {
    let mut t: c_int = 0;
    let v = JSONAPI_get(key, path, &mut t);
    if v != null() && t == JSONType::Int as c_int {
        JSONAPI_getInt(v, val)
    } else {
        Status::Err as c_int
    }
}

fn json_api_get_double<M: Manager>(_: M, json: *const c_void, val: *mut c_double) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Double => {
            unsafe { *val = json.get_double() };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getDouble(json: *const c_void, val: *mut c_double) -> c_int {
    json_api_get_double(RedisJsonKeyManager, json, val)
}

#[no_mangle]
pub extern "C" fn JSONAPI_getDoubleFromKey(
    key: *mut c_void,
    path: *const c_char,
    val: *mut c_double,
) -> c_int {
    let mut t: c_int = 0;
    let v = JSONAPI_get(key, path, &mut t);
    if v != null() && t == JSONType::Double as c_int {
        JSONAPI_getDouble(v, val)
    } else {
        Status::Err as c_int
    }
}

fn json_api_get_boolean<M: Manager>(_: M, json: *const c_void, val: *mut c_int) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Bool => {
            unsafe { *val = json.get_bool() as c_int };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_getBoolean(json: *const c_void, val: *mut c_int) -> c_int {
    json_api_get_boolean(RedisJsonKeyManager, json, val)
}

#[no_mangle]
pub extern "C" fn JSONAPI_getBooleanFromKey(
    key: *mut c_void,
    path: *const c_char,
    val: *mut c_int,
) -> c_int {
    let mut t: c_int = 0;
    let v = JSONAPI_get(key, path, &mut t);
    if v != null() && t == JSONType::Bool as c_int {
        JSONAPI_getBoolean(v, val)
    } else {
        Status::Err as c_int
    }
}

//---------------------------------------------------------------------------------------------

pub fn value_from_index(value: &Value, index: libc::size_t) -> Result<&Value, RedisError> {
    match value {
        Value::Array(ref vec) => {
            if index < vec.len() {
                Ok(vec.get(index).unwrap())
            } else {
                Err(RedisError::Str("JSON index is out of range"))
            }
        }
        Value::Object(ref map) => {
            if index < map.len() {
                Ok(map.iter().nth(index).unwrap().1)
            } else {
                Err(RedisError::Str("JSON index is out of range"))
            }
        }
        _ => Err(RedisError::Str("Not a JSON Array or Object")),
    }
}

pub fn get_type_and_size(value: &Value) -> (JSONType, libc::size_t) {
    RedisJSON::get_type_and_size(value)
}

pub fn set_string(from_str: &str, str: *mut *const c_char, len: *mut libc::size_t) -> c_int {
    if !str.is_null() {
        unsafe {
            *str = from_str.as_ptr() as *const c_char;
            *len = from_str.len();
        }
        return Status::Ok as c_int;
    }
    Status::Err as c_int
}

fn json_api_get_type_internal<V: SelectValue>(v: &V) -> JSONType {
    match v.get_type() {
        SelectValueType::Null => JSONType::Null,
        SelectValueType::Bool => JSONType::Bool,
        SelectValueType::Long => JSONType::Int,
        SelectValueType::Double => JSONType::Double,
        SelectValueType::String => JSONType::String,
        SelectValueType::Array => JSONType::Array,
        SelectValueType::Object => JSONType::Object,
    }
}

pub fn json_api_get<M: Manager>(
    _: M,
    key: *mut c_void,
    path: *const c_char,
    jtype: *mut c_int,
) -> *const c_void {
    let key = unsafe { &*(key as *mut M::ReadHolder) };
    let v = key.get_value().unwrap().unwrap();

    let mut selector = Selector::new();
    selector.value(v);
    let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
    if let Err(_) = selector.str_path(path) {
        return null();
    }
    match selector.select() {
        Ok(s) => match s.first() {
            Some(v) => {
                if jtype != null_mut() {
                    unsafe { *jtype = json_api_get_type_internal(*v) as c_int };
                }
                *v as *const M::V as *const c_void
            }
            None => null(),
        },
        Err(_) => null(),
    }
}

#[no_mangle]
pub extern "C" fn JSONAPI_get(
    key: *mut c_void,
    path: *const c_char,
    jtype: *mut c_int,
) -> *const c_void {
    json_api_get(RedisJsonKeyManager, key, path, jtype)
}

static REDISJSON_GETAPI: &str = concat!("RedisJSON_V1", "\0");

pub fn export_shared_api(ctx: &Context) {
    ctx.log_notice("Exported RedisJSON_V1 API");
    ctx.export_shared_api(
        &JSONAPI as *const RedisJSONAPI_V1 as *const c_void,
        REDISJSON_GETAPI.as_ptr() as *const c_char,
    );
}

static JSONAPI: RedisJSONAPI_V1 = RedisJSONAPI_V1 {
    openKey: JSONAPI_openKey,
    openKeyFromStr: JSONAPI_openKeyFromStr,
    closeKey: JSONAPI_closeKey,
    get: JSONAPI_get,
    getAt: JSONAPI_getAt,
    getLen: JSONAPI_getLen,
    getType: JSONAPI_getType,
    getInt: JSONAPI_getInt,
    getIntFromKey: JSONAPI_getIntFromKey,
    getDouble: JSONAPI_getDouble,
    getDoubleFromKey: JSONAPI_getDoubleFromKey,
    getBoolean: JSONAPI_getBoolean,
    getBooleanFromKey: JSONAPI_getBooleanFromKey,
    getString: JSONAPI_getString,
    getStringFromKey: JSONAPI_getStringFromKey,
    getJSON: JSONAPI_getJSON,
    getJSONFromKey: JSONAPI_getJSONFromKey,
    isJSON: JSONAPI_isJSON,
};

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_snake_case)]
pub struct RedisJSONAPI_V1 {
    pub openKey: extern "C" fn(
        ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
    ) -> *mut c_void,
    pub openKeyFromStr:
        extern "C" fn(ctx: *mut rawmod::RedisModuleCtx, path: *const c_char) -> *mut c_void,
    pub closeKey: extern "C" fn(key: *mut c_void),
    pub get:
        extern "C" fn(key: *mut c_void, path: *const c_char, jtype: *mut c_int) -> *const c_void,
    pub getAt:
        extern "C" fn(json: *const c_void, index: libc::size_t, jtype: *mut c_int) -> *const c_void,
    pub getLen: extern "C" fn(json: *const c_void, len: *mut libc::size_t) -> c_int,
    pub getType: extern "C" fn(json: *const c_void) -> c_int,
    pub getInt: extern "C" fn(json: *const c_void, val: *mut c_long) -> c_int,
    pub getIntFromKey:
        extern "C" fn(key: *mut c_void, path: *const c_char, val: *mut c_long) -> c_int,
    pub getDouble: extern "C" fn(json: *const c_void, val: *mut c_double) -> c_int,
    pub getDoubleFromKey:
        extern "C" fn(key: *mut c_void, path: *const c_char, val: *mut c_double) -> c_int,
    pub getBoolean: extern "C" fn(json: *const c_void, val: *mut c_int) -> c_int,
    pub getBooleanFromKey:
        extern "C" fn(key: *mut c_void, path: *const c_char, val: *mut c_int) -> c_int,
    pub getString: extern "C" fn(
        json: *const c_void,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int,
    pub getStringFromKey: extern "C" fn(
        key: *mut c_void,
        path: *const c_char,
        str: *mut *const c_char,
        len: *mut libc::size_t,
    ) -> c_int,
    pub getJSON: extern "C" fn(
        json: *const c_void,
        ctx: *mut rawmod::RedisModuleCtx,
        str: *mut *mut rawmod::RedisModuleString,
    ) -> c_int,
    pub getJSONFromKey: extern "C" fn(
        key: *mut c_void,
        ctx: *mut rawmod::RedisModuleCtx,
        path: *const c_char,
        str: *mut *mut rawmod::RedisModuleString,
    ) -> c_int,
    pub isJSON: extern "C" fn(key: *mut rawmod::RedisModuleKey) -> c_int,
}
