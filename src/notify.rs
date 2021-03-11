use std::{ffi::CStr, os::raw::{c_void, c_char}};
use redis_module::{RedisString, raw as rawmod};
use redis_module::{Context, NotifyEvent, Status};

use crate::{REDIS_JSON_TYPE, redisjson::{Format, Path, RedisJSON}};


//
// structs
//

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum JsonValueType {
    Null,
    Bool, //(bool),
    Number, //(Number),
    String, //(String),
    Array, //(Vec<Value>),
    Object, //(Map<String, Value>),
}

#[repr(C)]
union JsonValueUnion {
    // err: int,
    string: std::mem::ManuallyDrop<String>,
    num_int: i64,
    num_float: f64,
    boolean: bool,
    //nil: std::mem::ManuallyDrop<null>,
    //arr: Vec<JsonValueUnion>,
    //obj: HashMap<String,JsonValueUnion>,
}

#[repr(C)]
pub struct JsonValue {
    value_type: JsonValueType,
    value: JsonValueUnion,
}

#[no_mangle]
pub extern "C" fn get_json_path(
        module_ctx: *mut rawmod::RedisModuleCtx,
        key_str: *mut rawmod::RedisModuleString,
        path: *const c_char,
    ) -> *const rawmod::RedisModuleString {
    let ctx = Context::new(module_ctx);
    let key = ctx.open_with_redis_string(key_str);
    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE) {
        Ok(v) => {
            match v {
                Some(doc) => {
                    print!("{:?}", doc);
                    let mut paths: Vec<Path> = Vec::new();
                    paths.push(Path::new(unsafe { CStr::from_ptr(path).to_str().unwrap().into() } ));
                    match doc
                        .to_json(&mut paths, "".to_string(), "\n".to_string(), " ".to_string(), Format::JSON) {
                            Ok(doc) => {doc}
                            Err(e) => {e.msg}
                        }
                }
                None => String::from("<key not found>")
            }
        }
        Err(_) => String::from("<err>")
    };
    unsafe {
        rawmod::RedisModule_CreateString.unwrap()(ctx.get_ctx(), value.as_ptr() as *const c_char, value.len())
    }
}

static GET_JSON_PATH_API_NAME: &str = concat!("get_json_path", "\0");

pub fn export_shared_api(ctx: &Context) {
    ctx.export_shared_api(
        get_json_path as *const c_void,
        GET_JSON_PATH_API_NAME.as_ptr() as *mut i8
    );
}

pub fn notify_keyspace_event(
    ctx: &Context,
    event_type: NotifyEvent,
    event: &str,
    keyname: &str,
) -> Status {
    ctx.notify_keyspace_event(event_type, event, keyname)
}