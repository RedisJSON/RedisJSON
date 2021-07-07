use libc::size_t;
use std::ffi::CString;
use std::os::raw::{c_double, c_int, c_long};
use std::ptr::{null, null_mut};
use std::{
    ffi::CStr,
    os::raw::{c_char, c_void},
};

use crate::commands::KeyValue;
use jsonpath_lib::select::select_value::{SelectValue, SelectValueType};
use jsonpath_lib::select::Selector;
use redis_module::{raw as rawmod, RedisError};
use redis_module::{Context, Status};
use serde_json::Value;

use crate::manager::{Manager, ReadHolder};
use crate::redisjson::RedisJSON;

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

pub fn json_api_open_key_internal<M: Manager>(
    manager: M,
    ctx: *mut rawmod::RedisModuleCtx,
    key: &str,
) -> *const M::V {
    let ctx = Context::new(ctx);
    if let Ok(h) = manager.open_key_read(&ctx, key) {
        if let Ok(v) = h.get_value() {
            if let Some(v) = v {
                return v;
            }
        }
    }
    null()
}

pub fn json_api_get_at<M: Manager>(_: M, json: *const c_void, index: size_t) -> *const c_void {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Array => match json.get_index(index) {
            Some(v) => v as *const M::V as *const c_void,
            _ => null(),
        },
        _ => null(),
    }
}

pub fn json_api_get_len<M: Manager>(_: M, json: *const c_void, count: *mut libc::size_t) -> c_int {
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

pub fn json_api_get_type<M: Manager>(_: M, json: *const c_void) -> c_int {
    json_api_get_type_internal(unsafe { &*(json as *const M::V) }) as c_int
}

pub fn json_api_get_string<M: Manager>(
    _: M,
    json: *const c_void,
    str: *mut *const c_char,
    len: *mut size_t,
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

pub fn json_api_get_json<M: Manager>(
    _: M,
    json: *const c_void,
    ctx: *mut rawmod::RedisModuleCtx,
    str: *mut *mut rawmod::RedisModuleString,
) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    let res = KeyValue::new(json).to_value(json).to_string();
    create_rmstring(ctx, &res, str)
}

pub fn json_api_get_int<M: Manager>(_: M, json: *const c_void, val: *mut c_long) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Long => {
            unsafe { *val = json.get_long() };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

pub fn json_api_get_double<M: Manager>(_: M, json: *const c_void, val: *mut c_double) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Double => {
            unsafe { *val = json.get_double() };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

pub fn json_api_get_boolean<M: Manager>(_: M, json: *const c_void, val: *mut c_int) -> c_int {
    let json = unsafe { &*(json as *const M::V) };
    match json.get_type() {
        SelectValueType::Bool => {
            unsafe { *val = json.get_bool() as c_int };
            Status::Ok as c_int
        }
        _ => Status::Err as c_int,
    }
}

//---------------------------------------------------------------------------------------------

pub fn value_from_index(value: &Value, index: size_t) -> Result<&Value, RedisError> {
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

pub fn get_type_and_size(value: &Value) -> (JSONType, size_t) {
    RedisJSON::get_type_and_size(value)
}

pub fn set_string(from_str: &str, str: *mut *const c_char, len: *mut size_t) -> c_int {
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

pub fn json_api_next<M: Manager>(_: M, iter: *mut c_void) -> *const c_void {
    let iter = unsafe { &mut *(iter as *mut ResultsIterator<M::V>) };
    if iter.pos >= iter.results.len() {
        null_mut()
    } else {
        let res = iter.results[iter.pos] as *const M::V as *const c_void;
        iter.pos = iter.pos + 1;
        res
    }
}

pub fn json_api_len<M: Manager>(_: M, iter: *const c_void) -> size_t {
    let iter = unsafe { &*(iter as *mut ResultsIterator<M::V>) };
    iter.results.len() as size_t
}

pub fn json_api_free_iter<M: Manager>(_: M, iter: *mut c_void) {
    unsafe {
        Box::from_raw(iter as *mut ResultsIterator<M::V>);
    }
}

pub fn json_api_get<M: Manager>(_: M, val: *const c_void, path: *const c_char) -> *const c_void {
    let v = unsafe { &*(val as *const M::V) };
    let mut selector = Selector::new();
    selector.value(v);
    let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
    if selector.str_path(path).is_err() {
        return null();
    }
    match selector.select() {
        Ok(s) => Box::into_raw(Box::new(ResultsIterator { results: s, pos: 0 })) as *mut c_void,
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
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let mut len = 0;
            let key = unsafe {
                let bytes = RedisModule_StringPtrLen.unwrap()(key_str, &mut len);
                let bytes = slice::from_raw_parts(bytes as *const u8, len);
                String::from_utf8_lossy(bytes).into_owned()
            };
            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_open_key_internal(mngr, ctx, &key) as *mut c_void,
                None => json_api_open_key_internal(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    ctx,
                    &key,
                ) as *mut c_void,
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_openKeyFromStr(
            ctx: *mut rawmod::RedisModuleCtx,
            path: *const c_char,
        ) -> *mut c_void {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let key = unsafe { CStr::from_ptr(path).to_str().unwrap() };
            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_open_key_internal(mngr, ctx, &key) as *mut c_void,
                None => json_api_open_key_internal(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    ctx,
                    &key,
                ) as *mut c_void,
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_get(key: *const c_void, path: *const c_char) -> *const c_void {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get(mngr, key, path),
                None => json_api_get(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    key,
                    path,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_next(iter: *mut c_void) -> *const c_void {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_next(mngr, iter),
                None => json_api_next(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    iter,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_len(iter: *const c_void) -> size_t {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_len(mngr, iter),
                None => json_api_len(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    iter,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_freeIter(iter: *mut c_void) {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_free_iter(mngr, iter),
                None => json_api_free_iter(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    iter,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getAt(json: *const c_void, index: size_t) -> *const c_void {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_at(mngr, json, index),
                None => json_api_get_at(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    index,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getLen(json: *const c_void, count: *mut size_t) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_len(mngr, json, count),
                None => json_api_get_len(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    count,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getType(json: *const c_void) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_type(mngr, json),
                None => json_api_get_type(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getInt(json: *const c_void, val: *mut c_long) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_int(mngr, json, val),
                None => json_api_get_int(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    val,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getDouble(json: *const c_void, val: *mut c_double) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_double(mngr, json, val),
                None => json_api_get_double(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    val,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getBoolean(json: *const c_void, val: *mut c_int) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_boolean(mngr, json, val),
                None => json_api_get_boolean(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    val,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getString(
            json: *const c_void,
            str: *mut *const c_char,
            len: *mut size_t,
        ) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_string(mngr, json, str, len),
                None => json_api_get_string(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    str,
                    len,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_getJSON(
            json: *const c_void,
            ctx: *mut rawmod::RedisModuleCtx,
            str: *mut *mut rawmod::RedisModuleString,
        ) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_get_json(mngr, json, ctx, str),
                None => json_api_get_json(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    json,
                    ctx,
                    str,
                ),
            }
        }

        #[no_mangle]
        pub extern "C" fn JSONAPI_isJSON(key: *mut rawmod::RedisModuleKey) -> c_int {
            $pre_command_function_expr(&get_llapi_ctx(), &Vec::new());

            let m = $get_manager_expr;
            match m {
                Some(mngr) => json_api_is_json(mngr, key),
                None => json_api_is_json(
                    manager::RedisJsonKeyManager {
                        phantom: PhantomData,
                    },
                    key,
                ),
            }
        }

        static REDISJSON_GETAPI: &str = concat!("RedisJSON_V1", "\0");

        pub fn export_shared_api(ctx: &Context) {
            ctx.log_notice("Exported RedisJSON_V1 API");
            unsafe {
                LLAPI_CTX = Some(rawmod::RedisModule_GetThreadSafeContext.unwrap()(
                    std::ptr::null_mut(),
                ))
            };
            ctx.export_shared_api(
                &JSONAPI as *const RedisJSONAPI_V1 as *const c_void,
                REDISJSON_GETAPI.as_ptr() as *const c_char,
            );
        }

        static JSONAPI: RedisJSONAPI_V1 = RedisJSONAPI_V1 {
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
            pub get: extern "C" fn(val: *const c_void, path: *const c_char) -> *const c_void,
            pub next: extern "C" fn(iter: *mut c_void) -> *const c_void,
            pub len: extern "C" fn(iter: *const c_void) -> size_t,
            pub freeIter: extern "C" fn(iter: *mut c_void),
            pub getAt: extern "C" fn(json: *const c_void, index: size_t) -> *const c_void,
            pub getLen: extern "C" fn(json: *const c_void, len: *mut size_t) -> c_int,
            pub getType: extern "C" fn(json: *const c_void) -> c_int,
            pub getInt: extern "C" fn(json: *const c_void, val: *mut c_long) -> c_int,
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
        }
    };
}
