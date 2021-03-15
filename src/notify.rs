use redis_module::{raw as rawmod, RedisString};
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
pub extern "C" fn getPath(
    module_ctx: *mut rawmod::RedisModuleCtx,
    key_str: *mut rawmod::RedisModuleString,
    path: *const c_char,
) -> *const rawmod::RedisModuleString {
    let ctx = Context::new(module_ctx);
    let key = ctx.open_with_redis_string(key_str);
    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE) {
        Ok(v) => match v {
            Some(doc) => {
                print!("{:?}", doc);
                let mut paths: Vec<Path> = Vec::new();
                paths.push(Path::new(unsafe {
                    CStr::from_ptr(path).to_str().unwrap().into()
                }));
                match doc.to_json(
                    &mut paths,
                    "".to_string(),
                    "\n".to_string(),
                    " ".to_string(),
                    Format::JSON,
                ) {
                    Ok(doc) => doc,
                    Err(e) => e.msg,
                }
            }
            None => String::from("<key not found>"),
        },
        Err(_) => String::from("<err>"),
    };
    unsafe {
        rawmod::RedisModule_CreateString.unwrap()(
            ctx.get_ctx(),
            value.as_ptr() as *const c_char,
            value.len(),
        )
    }
}

static REDISJSON_GETAPI: &str = concat!("RedisJSON_V1", "\0");

pub fn export_shared_api(ctx: &Context) {
    ctx.export_shared_api(
        //RedisJSON_GetApiV1 as *const c_void,
        RedisJSON_GetApiV1 as *const c_void,
        REDISJSON_GETAPI.as_ptr() as *mut i8,
    );
}

static JSONAPI: RedisModuleAPI_V1 = RedisModuleAPI_V1 {
    getPath: Some(getPath),
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
    pub getPath: Option<
        unsafe extern "C" fn(
            module_ctx: *mut rawmod::RedisModuleCtx,
            key_str: *mut rawmod::RedisModuleString,
            path: *const c_char,
        ) -> *const rawmod::RedisModuleString,
    >,
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}
