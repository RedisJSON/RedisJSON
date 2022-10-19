

use redis_module::{Context, RedisError, RedisResult, Status, RedisString, RedisValue, key::RedisKey, RedisModuleCtx, RedisModule_GetSharedAPI,
    subscribe_to_server_event, RedisModuleEvent, RedisModuleEvent_ModuleChange, REDISMODULE_SUBEVENT_MODULE_LOADED, RedisModuleModuleChange};
use std::ffi::{c_int, c_longlong, c_ulonglong, c_double, CStr, CString, c_void};
use std::f64::EPSILON;
use cstr::cstr;
use function_name::named;

const MODULE_NAME: &str = "RJ_LLAPI";
const MODULE_VERSION: u32 = 1;

struct RjApi {
    japi: *const RedisJSONAPI,
    version: i32,
}

impl RjApi {
    pub fn api(&self) -> &RedisJSONAPI {
        &*self.japi
    }
}

static mut rj_api: RjApi = RjApi {
    japi: std::ptr::null::<RedisJSONAPI>,
    version: 0
};

fn get_json_apis(
    ctx: *mut RedisModuleCtx,
    subscribe_to_module_change: bool
) -> Status {
    let japi: *mut ::std::os::raw::c_void;

    japi = unsafe { RedisModule_GetSharedAPI.unwrap()(ctx, cstr!("RedisJSON_V2").as_ptr()) };
    if !japi.is_null() {
        rj_api.japi = japi as *const RedisJSONAPI;
        rj_api.version = 2;
        return Status::Ok;
    }
    
    japi = unsafe { RedisModule_GetSharedAPI.unwrap()(ctx, cstr!("RedisJSON_V1").as_ptr()) };
    if !japi.is_null() {
        rj_api.japi = japi as *const RedisJSONAPI;
        rj_api.version = 1;
        return Status::Ok;
    }

    if subscribe_to_module_change {
        return subscribe_to_server_event(ctx, RedisModuleEvent_ModuleChange, Some(module_change_handler));
    }

    Status::Ok
}

unsafe extern "C" fn module_change_handler(
    ctx: *mut RedisModuleCtx,
    event: RedisModuleEvent,
    sub: u64,
    ei: *mut c_void
) {
    let ei = &*(ei as *mut RedisModuleModuleChange);
    if sub == REDISMODULE_SUBEVENT_MODULE_LOADED && // If the subscribed event is a module load,
       rj_api.japi.is_null() &&                     // and JSON is not already loaded,
       CStr::from_ptr(ei.module_name)
            .to_str().unwrap() == "ReJSON" &&       // and the loading module is JSON:
       get_json_apis(ctx, false) == Status::Err     // try to load it.
    {
        // Log Error
    }
}

fn init(ctx: &Context, _args: &[RedisString]) -> Status {
    get_json_apis(ctx.ctx, true);
    Status::Ok
}

#[named]
fn RJ_llapi_test_open_key(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    let keyname = RedisString::create(ctx.ctx, function_name!());

    assert!(ctx.call("JSON.SET", &[function_name!(), "$", "0"]).is_ok());
    let rmk = RedisKey::open(ctx.ctx, &keyname);
    assert_eq!(unsafe { rj_api.api().isJSON.unwrap()(rmk.key_inner) }, 1);
    assert!(unsafe { !(rj_api.api().openKey.unwrap()(ctx, &keyname).is_null()) });

    ctx.call("SET", &[function_name!(), "0"]);
    rmk = RedisKey::open(ctx.ctx, &keyname);
    assert_ne!(unsafe { rj_api.api().isJSON.unwrap()(rmk.key_inner) }, 1);
    assert!(unsafe { rj_api.api().openKey.unwrap()(ctx, &keyname).is_null() });

    Ok(RedisValue::SimpleStringStatic("PASS"))
}

#[named]
fn RJ_llapi_test_iterator(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    let _keyname = RedisString::create(ctx.ctx, function_name!());

    // #define VALS 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
    // long long vals[] = { VALS };
    // const char json[] = "[" STRINGIFY(VALS) "]";
    // RedisModule_Call(ctx, "JSON.SET", "ccc", TEST_NAME, "$", json);
  
    // JSONResultsIterator ji = RjApi.japi->get(RjApi.japi->openKeyFromStr(ctx, TEST_NAME), "$..*");
    // ASSERT(ji != NULL);
    // if (RjApi.version >= 2) {
    //   RedisModuleString *str;
    //   RjApi.japi->getJSONFromIter(ji, ctx, &str);
    //   ASSERT(strcmp(RedisModule_StringPtrLen(str, NULL), json) == 0);
    //   RedisModule_FreeString(ctx, str);
    // }
  
    // size_t len = RjApi.japi->len(ji); ASSERT(len == sizeof(vals)/sizeof(*vals));
    // RedisJSON js; long long num;
    // for (int i = 0; i < len; ++i) {
    //   js = RjApi.japi->next(ji); ASSERT(js != NULL);
    //   RjApi.japi->getInt(js, &num); ASSERT(num == vals[i]);
    // }
    // ASSERT(RjApi.japi->next(ji) == NULL);
  
    // RjApi.japi->freeIter(ji);

    Ok(RedisValue::SimpleStringStatic("PASS"))
}

#[named]
fn RJ_llapi_test_get_type(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    let keyname = RedisString::create(ctx.ctx, function_name!());

    ctx.call("JSON.SET", &[function_name!(), "$", "[\"\", 0, 0.0, false, {}, [], null]"]);
    let js = unsafe { rj_api.api().openKey.unwrap()(ctx, &keyname) };
    
    let mut len = 0u64;
    unsafe { rj_api.api().getLen(js, &mut len as *mut c_ulonglong) };
    assert_eq!(len, JSONType__EOF as u64);

    for i in 0..len { unsafe { 
        let elem = rj_api.api().getAt.unwrap()(js, i as c_ulonglong);
        let jtype = rj_api.api().getType.unwrap()(elem);
        assert_eq!(jtype, i as c_int);
    }}

    Ok(RedisValue::SimpleStringStatic("PASS"))
}

#[named]
fn RJ_llapi_test_get_value(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 1 {
        return Err(RedisError::WrongArity);
    }

    let keyname = RedisString::create(ctx.ctx, function_name!());

    ctx.call("JSON.SET", &[function_name!(), "$", "[\"a\", 1, 0.1, true, {\"_\":1}, [1], null]"]);
    let js = unsafe { rj_api.api().openKey.unwrap()(ctx, &keyname) };
  
    let mut s: CString;
    let mut len: u64;
    unsafe { rj_api.api().getString.unwrap()(rj_api.api().getAt.unwrap()(js, 0 as c_int), &mut s as *mut CString, &mut len as *mut c_ulonglong) };
    assert_eq!(s.to_str().unwrap(), "a");
  
    let mut ll: i64;
    unsafe { rj_api.api().getInt.unwrap()(rj_api.api().getAt.unwrap()(js, 1 as c_int), &mut ll as *mut c_longlong) };
    assert_eq!(ll, 1);
  
    let mut dbl: f64;
    unsafe { rj_api.api().getDouble.unwrap()(rj_api.api().getAt.unwrap()(js, 2 as c_int), &mut dbl as *mut c_double) };
    assert!((dbl - 0.1).abs() < EPSILON);

    let mut b: i32;
    unsafe { rj_api.api().getBoolean.unwrap()(rj_api.api().getAt.unwrap()(js, 3 as c_int), &mut b as *mut c_int) };
    assert_eq!(b, 1);
  
    len = 0;
    unsafe { rj_api.api().getLen.unwrap()(rj_api.api().getAt.unwrap()(js, 4 as c_int), &mut len as *mut c_ulonglong) };
    assert_eq!(len, 1);
  
    len = 0;
    unsafe { rj_api.api().getLen.unwrap()(rj_api.api().getAt.unwrap()(js, 5 as c_int), &mut len as *mut c_ulonglong) };
    assert_eq!(len, 1);
  
    Ok(RedisValue::SimpleStringStatic("PASS"))
}


const fn split(cmd: &str) -> (&str, &str) {
    use ::konst::option::unwrap;
    use ::konst::slice::get_range;
    use ::konst::slice::get_from;

    const i: usize = MODULE_NAME.len();
    let cmd = cmd.as_bytes();
    let (hd, tl) = (
        unwrap!(get_range(cmd, 0, i)),
        unwrap!(get_from(cmd, i + "_".len())),
    );
    unsafe { (
        ::core::str::from_utf8_unchecked(hd),
        ::core::str::from_utf8_unchecked(tl),
    )}
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
