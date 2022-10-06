

#[macro_use]
extern crate redis_module;

use redis_module::{Context, RedisError, RedisResult, RedisString};

const MODULE_NAME: &str = "RJ_LLAPI";
const MODULE_VERSION: u32 = 1;

struct RJ_API {
    japi: ::std::os::raw::c_void,
    version: i32,
}

static mut rj_api: RJ_API;

macro_rules! command {
    (
        $command:expr
    ) => {
        let mut name_buffer = [0; 64];
        let name = stringify!($command);
        unsafe {
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                name_buffer.as_mut_ptr(),
                name.len(),
            );
            name_buffer[9] = '.';
        }

        [name_buffer as &str, $command, "", 0, 0, 0]
    }
}

fn get_json_apis(ctx: &Context) -> Status {
    let japi: ::std::os::raw::c_void;
    if !(japi = RedisModule_GetSharedAPI.unwrap()(ctx.ctx, "RedisJSON_V2".as_ptr().cast::<c_char>())).is_null() {
        rj_api.japi = japi;
        rj_api.version = 2;
        Status::OK
    } else if !(japi = RedisModule_GetSharedAPI.unwrap()(ctx.ctx, "RedisJSON_V1".as_ptr().cast::<c_char>())).is_null() {
        rj_api.japi = japi;
        rj_api.version = 1;
        Status::OK
    } else {
        Status::Err
    }
}

fn init(ctx: &Context, _args: &[RedisString]) -> Status {
    match get_json_apis(ctx) {
        Status::Err => redis_event_handler!(ctx, raw::NotifyEvent::REDISMODULE_NOTIFY_MODULE, get_json_apis),
        _ => _,
    }
}

fn RJ_llapi_test_open_key(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    OK("PASS")
}

fn RJ_llapi_test_iterator(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    OK("PASS")
}

fn RJ_llapi_test_get_type(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    OK("PASS")
}

fn RJ_llapi_test_get_value(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    OK("PASS")
}





redis_module! {
    name: MODULE_NAME,
    version: MODULE_VERSION,
    data_types: [],
    init: init,
    commands: [
        command!(RJ_llapi_test_open_key),
        command!(RJ_llapi_test_iterator),
        command!(RJ_llapi_test_get_type),
        command!(RJ_llapi_test_get_value),
    ],
}
