
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

impl RJ_API {
    pub fn api(&self) -> &RedisJSONAPI {
        &*japi
    }
}

static mut rj_api: RJ_API = RJ_API {
    japi: 0 as *const RedisJSONAPI,
    version: 0
};

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
    assert_eq!(unsafe { rj_api.api().isJSON.unwrap()(&rmk.key_inner) }, 1);
    assert!(unsafe { !(rj_api.api().openKey.unwrap()(ctx, &keyname).is_null()) });

    ctx.call("SET", &[function_name!(), "0"]);
    rmk = RedisKey::open(&ctx, &keyname);
    assert_ne!(unsafe { rj_api.api().isJSON.unwrap()(&rmk.key_inner) }, 1);
    assert!(unsafe { rj_api.api().openKey.unwrap()(ctx, &keyname).is_null() });

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

    let keyname = RedisString::create(ctx, function_name!());

    ctx.call("JSON.SET", &[function_name!(), "$", "[\"\", 0, 0.0, false, {}, [], null]"]);
    let js = unsafe { rj_api.api().openKey.unwrap()(ctx, &keyname) };
    
    let mut len = 0u64;
    unsafe { rj_api.api().getLen(js, &len as *mut c_ulonglong) };
    assert_eq!(len, JSONType__EOF as u64);

    for i in ..len { unsafe { 
        let elem = rj_api.api().getAt.unwrap()(js, i as c_ulonglong);
        let jtype = rj_api.api().getType.unwrap()(elem);
        assert_eq!(jtype, i as c_int);
    }}

    OK("PASS")
}

#[named]
fn RJ_llapi_test_get_value(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    let keyname = RedisString::create(ctx, function_name!());

    ctx.call("JSON.SET", &[function_name!(), "$", "[\"a\", 1, 0.1, true, {\"_\":1}, [1], null]"]);
    let js = unsafe { rj_api.api().openKey.unwrap()(ctx, &keyname) };
  
    let mut s: CString;
    let mut len: u64;
    unsafe { RJ_API.api().getString.unwrap()(RJ_API.api().getAt.unwrap()(js, 0), &s as *mut CString, &len as *mut c_ulonglong) };
    assert_eq!(s.to_str(), OK("a"));
  
    let mut ll: i64;
    unsafe { RJ_API.api().getInt.unwrap()(RJ_API.api().getAt.unwrap()(js, 1), &ll as *mut c_longlong) };
    assert_eq!(ll, 1);
  
    let mut dbl: f64;
    unsafe { RJ_API.api().getDouble.unwrap()(RJ_API.api().getAt.unwrap()(js, 2), &dbl as *mut c_double) };
    assert!((dbl - 0.1).abs() < EPSILON);

    let mut b: bool;
    unsafe { RJ_API.api().getBoolean.unwrap()(RJ_API.api().getAt.unwrap()(js, 3), &b as *mut c_int) };
    assert_eq!(b, true);
  
    len = 0;
    unsafe { RJ_API.api().getLen.unwrap()(RJ_API.api().getAt.unwrap()(js, 4), &len as *mut c_ulonglong) };
    assert_eq!(len, 1);
  
    len = 0;
    unsafe { RJ_API.api().getLen.unwrap()(RJ_API.api().getAt.unwrap()(js, 5), &len as *mut c_ulonglong) };
    assert_eq!(len, 1);
  
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
