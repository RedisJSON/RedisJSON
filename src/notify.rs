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
#[derive(Debug, Copy, Clone)]
pub enum JSONType {
    Null,
    Bool,   //(bool),
    Number, //(Number),
    String, //(String),
    Array,  //(Vec<Value>),
    Object, //(Map<String, Value>),
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum JSONCType {
    String = 0,
    Int = 1,
    Double = 2,
    Bool = 3,
    Object = 4,
    Array = 5,
    Null = 6,
    Err = 7,
}

// #[repr(C)]
// union JsonValueUnion {
//     // err: int,
//     CString: std::mem::ManuallyDrop<CString>,
//     num_int: i64,
//     num_float: f64,
//     boolean: bool,
//     //nil: std::mem::ManuallyDrop<null>,
//     //arr: Vec<JsonValueUnion>,
//     //obj: HashMap<String,JsonValueUnion>,
// }

// #[repr(C)]
// pub struct JsonValue {
//     value_type: JsonValueType,
//     value: JsonValueUnion,
// }

#[no_mangle]
pub extern "C" fn getInfo(
    redisjson: *mut c_void,
    name: *mut c_void,
    jtype: *mut JSONCType,
    size: *mut libc::size_t,
) -> c_int {
    if !redisjson.is_null() {
        let json = unsafe { &*(redisjson as *mut RedisJSON) };
        let t = json.get_type("");
        match t {
            Ok(o) => {
                print!("{:?}\n", o)
            }
            Err(e) => {
                print!("{:?}\n", e);
            }
        }
    } else {
        //
    }
    0
}

//FIXME: //TODO: Add free API for redisjson: *mut c_void

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
            Box::into_raw(Box::new(value)) as *mut c_void
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
    getPath: getPath,
    getInfo: getInfo,
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
    pub getPath: unsafe extern "C" fn(
        module_ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
        path: *const c_char,
    ) -> *mut c_void,
    pub getInfo:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut JSONCType, *mut libc::size_t) -> c_int,
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}
