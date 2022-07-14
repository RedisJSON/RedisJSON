use libc::size_t;
use std::ffi::CString;
use std::os::raw::{c_double, c_int, c_longlong};
use std::ptr::{null, null_mut};
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use crate::commands::KeyValue;
use jsonpath_lib::select::select_value::{SelectValue, SelectValueType};
use jsonpath_lib::select::Selector;
use redis_module::raw as rawmod;
use redis_module::{Context, RedisString, Status};

use crate::manager::{Manager, ReadHolder};

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

struct ResultsIterator<'a, V: SelectValue> {
    results: Vec<&'a V>,
    pos: usize,
}

//---------------------------------------------------------------------------------------------

pub static mut LLAPI_CTX: Option<*mut rawmod::RedisModuleCtx> = None;

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn create_rmstring(
    ctx: *mut rawmod::RedisModuleCtx,
    from_str: &str,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    if let Ok(s) = CString::new(from_str) {
        let p = s.as_bytes_with_nul().as_ptr().cast::<c_char>();
        let len = s.as_bytes().len();
        unsafe { *str = rawmod::RedisModule_CreateString.unwrap()(ctx, p, len) };
        return Status::Ok as c_int;
    }
    Status::Err as c_int
}

pub fn json_api_open_key_internal<M: Manager>(
    manager: M,
    ctx: *mut rawmod::RedisModuleCtx,
    key: RedisString,
) -> *const M::V {
    let ctx = Context::new(ctx);
    if let Ok(h) = manager.open_key_read(&ctx, &key) {
        if let Ok(Some(v)) = h.get_value() {
            return v;
        }
    }
    null()
}

pub fn json_api_get_at<M: Manager>(_: M, json: *const c_void, index: size_t) -> *const c_void {
    let json = unsafe { &*(json.cast::<M::V>()) };
    match json.get_type() {
        SelectValueType::Array => match json.get_index(index) {
            Some(v) => (v as *const M::V).cast::<c_void>(),
            _ => null(),
        },
        _ => null(),
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn json_api_get_len<M: Manager>(_: M, json: *const c_void, count: *mut libc::size_t) -> c_int {
    let json = unsafe { &*(json.cast::<M::V>()) };
    let len = match json.get_type() {
        SelectValueType::String => Some(json.get_str().len()),
        SelectValueType::Array | SelectValueType::Object => Some(json.len().unwrap()),
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

pub fn json_api_get_type<M: Manager>(_: M, json: *const c_void) -> c_int {
    json_api_get_type_internal(unsafe { &*(json.cast::<M::V>()) }) as c_int
}

pub fn json_api_get_string<M: Manager>(
    _: M,
    json: *const c_void,
    str: *mut *const c_char,
    len: *mut size_t,
) -> c_int {
    let json = unsafe { &*(json.cast::<M::V>()) };
    match json.get_type() {
        SelectValueType::String => {
            let s = json.as_str();
            set_string(s, str, len);
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

pub fn json_api_get_json<M: Manager>(
    _: M,
    json: *const c_void,
    ctx: *mut rawmod::RedisModuleCtx,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    let json = unsafe { &*(json.cast::<M::V>()) };
    let res = KeyValue::<M::V>::serialize_object(json, None, None, None);
    create_rmstring(ctx, &res, str)
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn json_api_get_int<M: Manager>(_: M, json: *const c_void, val: *mut c_longlong) -> c_int {
    let json = unsafe { &*(json.cast::<M::V>()) };
    match json.get_type() {
        SelectValueType::Long => {
            unsafe { *val = json.get_long() };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn json_api_get_double<M: Manager>(_: M, json: *const c_void, val: *mut c_double) -> c_int {
    let json = unsafe { &*(json.cast::<M::V>()) };
    match json.get_type() {
        SelectValueType::Double => {
            unsafe { *val = json.get_double() };
            Status::Ok as c_int
        }
        SelectValueType::Long => {
            unsafe { *val = json.get_long() as f64 };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn json_api_get_boolean<M: Manager>(_: M, json: *const c_void, val: *mut c_int) -> c_int {
    let json = unsafe { &*(json.cast::<M::V>()) };
    match json.get_type() {
        SelectValueType::Bool => {
            unsafe { *val = json.get_bool() as c_int };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

//---------------------------------------------------------------------------------------------

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn set_string(from_str: &str, str: *mut *const c_char, len: *mut size_t) -> c_int {
    if !str.is_null() {
        unsafe {
            *str = from_str.as_ptr().cast::<c_char>();
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

pub fn json_api_next<M: Manager>(_: M, iter: *mut c_void) -> *const c_void {
    let iter = unsafe { &mut *(iter.cast::<ResultsIterator<M::V>>()) };
    if iter.pos >= iter.results.len() {
        null_mut()
    } else {
        let res = (iter.results[iter.pos] as *const M::V).cast::<c_void>();
        iter.pos += 1;
        res
    }
}

pub fn json_api_len<M: Manager>(_: M, iter: *const c_void) -> size_t {
    let iter = unsafe { &*(iter.cast::<ResultsIterator<M::V>>()) };
    iter.results.len() as size_t
}

pub fn json_api_free_iter<M: Manager>(_: M, iter: *mut c_void) {
    unsafe {
        Box::from_raw(iter.cast::<ResultsIterator<M::V>>());
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn json_api_get<M: Manager>(_: M, val: *const c_void, path: *const c_char) -> *const c_void {
    let v = unsafe { &*(val.cast::<M::V>()) };
    let mut selector = Selector::new();
    selector.value(v);
    let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
    if selector.str_path(path).is_err() {
        return null();
    }
    match selector.select() {
        Ok(s) => Box::into_raw(Box::new(ResultsIterator { results: s, pos: 0 })).cast::<c_void>(),
        Err(_) => null(),
    }
}

pub fn json_api_is_json<M: Manager>(m: M, key: *mut rawmod::RedisModuleKey) -> c_int {
    match m.is_json(key) {
        Ok(res) => res as c_int,
        Err(_) => 0,
    }
}

pub fn get_llapi_ctx() -> Context {
    Context::new(unsafe { LLAPI_CTX.unwrap() })
}

#[macro_export]
macro_rules! redis_json_module_export_shared_api {
    (
        get_manage: $get_manager_expr:expr,
        pre_command_function: $pre_command_function_expr:expr,
    ) => {
        #[no_mangle]
        pub extern "C" fn JSONAPI_openKey(
            ctx: *mut rawmod::RedisModuleCtx,
            key_str: *mut rawmod::RedisModuleString,
        ) -> *mut c_void {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_open_key_internal(mngr, ctx, RedisString::new(ctx, key_str))as *mut c_void},
            )
        }

        #[no_mangle]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn JSONAPI_openKeyFromStr(
            ctx: *mut rawmod::RedisModuleCtx,
            path: *const c_char,
        ) -> *mut c_void {
            let key = unsafe { CStr::from_ptr(path).to_str().unwrap() };
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_open_key_internal(mngr, ctx, RedisString::create(ctx, key)) as *mut c_void},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_get(key: *const c_void, path: *const c_char) -> *const c_void {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get(mngr, key, path)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_next(iter: *mut c_void) -> *const c_void {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_next(mngr, iter)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_len(iter: *const c_void) -> size_t {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_len(mngr, iter)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_freeIter(iter: *mut c_void) {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_free_iter(mngr, iter)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getAt(json: *const c_void, index: size_t) -> *const c_void {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_at(mngr, json, index)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getLen(json: *const c_void, count: *mut size_t) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_len(mngr, json, count)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getType(json: *const c_void) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_type(mngr, json)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getInt(json: *const c_void, val: *mut c_longlong) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_int(mngr, json, val)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getDouble(json: *const c_void, val: *mut c_double) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_double(mngr, json, val)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getBoolean(json: *const c_void, val: *mut c_int) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_boolean(mngr, json, val)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getString(
            json: *const c_void,
            str: *mut *const c_char,
            len: *mut size_t,
        ) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_string(mngr, json, str, len)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getJSON(
            json: *const c_void,
            ctx: *mut rawmod::RedisModuleCtx,
            str: *mut *mut rawmod::RedisModuleString,
        ) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_get_json(mngr, json, ctx, str)},
            )
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_isJSON(key: *mut rawmod::RedisModuleKey) -> c_int {
            run_on_manager!(
                pre_command: ||$pre_command_function_expr(&get_llapi_ctx(), &Vec::new()),
                get_mngr: $get_manager_expr,
                run: |mngr|{json_api_is_json(mngr, key)},
            )
        }

        #[no_mangle]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn JSONAPI_pathParse(path: *const c_char, ctx: *mut rawmod::RedisModuleCtx, err_msg: *mut *mut rawmod::RedisModuleString) -> *const c_void {
            let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
            match StaticPathParser::get_path_info(path) {
                Ok(flags) => Box::into_raw(Box::new(flags)).cast::<c_void>(),
                Err(err_str) => {
                    crate::c_api::create_rmstring(ctx, &err_str, err_msg);
                    std::ptr::null()
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_pathFree(json_path: *mut c_void) {
            unsafe { Box::from_raw(json_path.cast::<PathInfoFlags>()) };
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_pathIsSingle(json_path: *const c_void) -> c_int {
            let flags = unsafe { &*(json_path.cast::<PathInfoFlags>()) };
            flags.intersects(PathInfoFlags::SINGLE) as c_int
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_pathHasDefinedOrder(json_path: *const c_void) -> c_int {
            let flags = unsafe { &*(json_path.cast::<PathInfoFlags>()) };
            flags.intersects(PathInfoFlags::DEFINED_ORDER) as c_int
        }

        static REDISJSON_GETAPI_V1: &str = concat!("RedisJSON_V1", "\0");
        static REDISJSON_GETAPI_V2: &str = concat!("RedisJSON_V2", "\0");

        pub fn export_shared_api(ctx: &Context) {
            unsafe {
                LLAPI_CTX = Some(rawmod::RedisModule_GetThreadSafeContext.unwrap()(
                    std::ptr::null_mut(),
                ));
                ctx.export_shared_api(
                    (&JSONAPI_CURRENT as *const RedisJSONAPI_CURRENT).cast::<c_void>(),
                    REDISJSON_GETAPI_V1.as_ptr().cast::<c_char>(),
                );
                ctx.log_notice("Exported RedisJSON_V1 API");
                ctx.export_shared_api(
                    (&JSONAPI_CURRENT as *const RedisJSONAPI_CURRENT).cast::<c_void>(),
                    REDISJSON_GETAPI_V2.as_ptr().cast::<c_char>(),
                );
                ctx.log_notice("Exported RedisJSON_V2 API");
            };
        }

        static JSONAPI_CURRENT : RedisJSONAPI_CURRENT = RedisJSONAPI_CURRENT {
            // V1 entries
            openKey: JSONAPI_openKey,
            openKeyFromStr: JSONAPI_openKeyFromStr,
            get: JSONAPI_get,
            next: JSONAPI_next,
            len: JSONAPI_len,
            freeIter: JSONAPI_freeIter,
            getAt: JSONAPI_getAt,
            getLen: JSONAPI_getLen,
            getType: JSONAPI_getType,
            getInt: JSONAPI_getInt,
            getDouble: JSONAPI_getDouble,
            getBoolean: JSONAPI_getBoolean,
            getString: JSONAPI_getString,
            getJSON: JSONAPI_getJSON,
            isJSON: JSONAPI_isJSON,
            // V2 entries
            pathParse: JSONAPI_pathParse,
            pathFree: JSONAPI_pathFree,
            pathIsSingle: JSONAPI_pathIsSingle,
            pathHasDefinedOrder: JSONAPI_pathHasDefinedOrder,
        };

        #[repr(C)]
        #[derive(Copy, Clone)]
        #[allow(non_snake_case)]
        pub struct RedisJSONAPI_CURRENT {
            // V1 entries
            pub openKey: extern "C" fn(
                ctx: *mut rawmod::RedisModuleCtx,
                key_str: *mut rawmod::RedisModuleString,
            ) -> *mut c_void,
            pub openKeyFromStr:
                extern "C" fn(ctx: *mut rawmod::RedisModuleCtx, path: *const c_char) -> *mut c_void,
            pub get: extern "C" fn(val: *const c_void, path: *const c_char) -> *const c_void,
            pub next: extern "C" fn(iter: *mut c_void) -> *const c_void,
            pub len: extern "C" fn(iter: *const c_void) -> size_t,
            pub freeIter: extern "C" fn(iter: *mut c_void),
            pub getAt: extern "C" fn(json: *const c_void, index: size_t) -> *const c_void,
            pub getLen: extern "C" fn(json: *const c_void, len: *mut size_t) -> c_int,
            pub getType: extern "C" fn(json: *const c_void) -> c_int,
            pub getInt: extern "C" fn(json: *const c_void, val: *mut c_longlong) -> c_int,
            pub getDouble: extern "C" fn(json: *const c_void, val: *mut c_double) -> c_int,
            pub getBoolean: extern "C" fn(json: *const c_void, val: *mut c_int) -> c_int,
            pub getString: extern "C" fn(
                json: *const c_void,
                str: *mut *const c_char,
                len: *mut size_t,
            ) -> c_int,
            pub getJSON: extern "C" fn(
                json: *const c_void,
                ctx: *mut rawmod::RedisModuleCtx,
                str: *mut *mut rawmod::RedisModuleString,
            ) -> c_int,
            pub isJSON: extern "C" fn(key: *mut rawmod::RedisModuleKey) -> c_int,
            // V2 entries
            pub pathParse: extern "C" fn(path: *const c_char, ctx: *mut rawmod::RedisModuleCtx, err_msg: *mut *mut rawmod::RedisModuleString) -> *const c_void,
            pub pathFree: extern "C" fn(json_path: *mut c_void),
            pub pathIsSingle: extern "C" fn(json_path: *const c_void) -> c_int,
            pub pathHasDefinedOrder: extern "C" fn(json_path: *const c_void) -> c_int,
        }
    };
}
