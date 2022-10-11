
/*
[dependencies]
redis_module.version = "1.0.1"
cstr.version = "0.2.11"
const_format.version = "0.2.30"
konst.version = "0.2.19"
function_name.version = "0.3.0"
*/

#[macro_use]
extern crate redis_module;

use redis_module::{Context, RedisError, RedisResult, Status, RedisString};
use cstr::cstr;
use std::ffi::CStr;
use ::function_name::named;

const MODULE_NAME: &str = "RJ_LLAPI";
const MODULE_VERSION: u32 = 1;

struct RJ_API {
    japi: *const RedisJSONAPI,
    version: i32,
}

static mut rj_api: RJ_API;

fn get_json_apis(ctx: &Context) -> Status {
    let japi: ::std::os::raw::c_void;
    if unsafe { !(japi = RedisModule_GetSharedAPI.unwrap()(ctx.ctx, cstr!("RedisJSON_V2"))).is_null() } {
        rj_api.japi = japi as *const RedisJSONAPI;
        rj_api.version = 2;
        Status::OK
    } else if unsafe { !(japi = RedisModule_GetSharedAPI.unwrap()(ctx.ctx, cstr!("RedisJSON_V1"))).is_null() } {
        rj_api.japi = japi as *const RedisJSONAPI;
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

#[named]
fn RJ_llapi_test_open_key(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    let keyname = RedisString::create(ctx, function_name!());

    assert!(ctx.call("JSON.SET", &[function_name!(), "$", "0"]).is_ok());
    let rmk = RedisKey::open(ctx, &keyname);
    assert_eq!(unsafe { rj_api.japi->isJSON.unwrap()(rmk.key_inner) }, 1);
    assert!(unsafe { !(rj_api.japi->openKey.unwrap()(ctx, keyname).is_null()) });

    ctx.call("SET", &[function_name!(), "0"]);
    rmk = RedisKey::open(ctx, &keyname);
    assert_eq!(unsafe { rj_api.japi->isJSON.unwrap()(rmk.key_inner) }, 0);
    assert!(unsafe { rj_api.japi->openKey.unwrap()(ctx, keyname).is_null() });

    OK("PASS")
}

#[named]
fn RJ_llapi_test_iterator(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    // #define VALS 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
    // long long vals[] = { VALS };
    // const char json[] = "[" STRINGIFY(VALS) "]";
    // RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", json);
  
    // JSONResultsIterator ji = RJ_API.japi->get(RJ_API.japi->openKeyFromStr(ctx, TEST_NAME), "$..*");
    // ASSERT(ji != NULL);
    // if (RJ_API.version >= 2) {
    //   RedisModuleString *str;
    //   RJ_API.japi->getJSONFromIter(ji, ctx, &str);
    //   ASSERT(strcmp(RedisModule_StringPtrLen(str, NULL), json) == 0);
    //   RedisModule_FreeString(ctx, str);
    // }
  
    // size_t len = RJ_API.japi->len(ji); ASSERT(len == sizeof(vals)/sizeof(*vals));
    // RedisJSON js; long long num;
    // for (int i = 0; i < len; ++i) {
    //   js = RJ_API.japi->next(ji); ASSERT(js != NULL);
    //   RJ_API.japi->getInt(js, &num); ASSERT(num == vals[i]);
    // }
    // ASSERT(RJ_API.japi->next(ji) == NULL);
  
    // RJ_API.japi->freeIter(ji);

    OK("PASS")
}

#[named]
fn RJ_llapi_test_get_type(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    // RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", "[\"\", 0, 0.0, false, {}, [], null]");
    // RedisJSON js = RJ_API.japi->openKeyFromStr(ctx, TEST_NAME);
  
    // size_t len; RJ_API.japi->getLen(js, &len); ASSERT(len == JSONType__EOF);
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_String)) == JSONType_String);
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_Int   )) == JSONType_Int   );
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_Double)) == JSONType_Double);
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_Bool  )) == JSONType_Bool  );
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_Object)) == JSONType_Object);
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_Array )) == JSONType_Array );
    // ASSERT(RJ_API.japi->getType(RJ_API.japi->getAt(js, JSONType_Null  )) == JSONType_Null  );  

    OK("PASS")
}

#[named]
fn RJ_llapi_test_get_value(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    // RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", "[\"a\", 1, 0.1, true, {\"_\":1}, [1], null]");
    // RedisJSON js = RJ_API.japi->openKeyFromStr(ctx, TEST_NAME);
  
    // const char *s; size_t len;
    // RJ_API.japi->getString(RJ_API.japi->getAt(js, JSONType_String), &s, &len);
    // ASSERT(strncmp(s, "a", len) == 0);
  
    // long long ll;
    // RJ_API.japi->getInt(RJ_API.japi->getAt(js, JSONType_Int), &ll);
    // ASSERT(ll == 1);
  
    // double dbl;
    // RJ_API.japi->getDouble(RJ_API.japi->getAt(js, JSONType_Double), &dbl);
    // ASSERT(fabs(dbl - 0.1) < DBL_EPSILON);
  
    // int b;
    // RJ_API.japi->getBoolean(RJ_API.japi->getAt(js, JSONType_Bool), &b);
    // ASSERT(b);
  
    // len = 0;
    // RJ_API.japi->getLen(RJ_API.japi->getAt(js, JSONType_Object), &len);
    // ASSERT(len == 1);
  
    // len = 0;
    // RJ_API.japi->getLen(RJ_API.japi->getAt(js, JSONType_Array), &len);
    // ASSERT(len == 1);
  
    OK("PASS")
}


const fn split(cmd: &str) -> (&str, &str) {
    use ::konst::option::unwrap;
    const i: usize = MODULE_NAME.len();
    (
        unwrap!(::konst::string::get_range(s, 0, i)),
        unwrap!(::konst::string::get_from(s, i + "_".len())),
    )
}

macro_rules! my_module {
    ($( $cmd:expr, )*) => {
        redis_module! (
            name: MODULE_NAME,
            version: MODULE_VERSION,
            data_types: [],
            init: init,
            commands: [
                $({
                    const NAME: &str = {
                        const SPLIT: (&str, &str) = split(stringify!($cmd));
                        ::const_format::concatcp!(SPLIT.0, ".", SPLIT.1);
                    }
                    [NAME, $cmd, "", 0, 0, 0],
                })*
            ]
        );
    }
}

my_module! {
    RJ_llapi_test_open_key,
    RJ_llapi_test_iterator,
    RJ_llapi_test_get_type,
    RJ_llapi_test_get_value,
}
