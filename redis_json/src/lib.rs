/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

extern crate redis_module;

#[cfg(not(feature = "as-library"))]
use commands::*;
use redis_module::native_types::RedisType;
use redis_module::raw::RedisModuleTypeMethods;
#[cfg(not(feature = "as-library"))]
use redis_module::InfoContext;

#[cfg(not(feature = "as-library"))]
use redis_module::Status;
#[cfg(not(feature = "as-library"))]
use redis_module::{Context, RedisResult};

#[cfg(not(feature = "as-library"))]
use redis_module::key::KeyFlags;

#[cfg(not(feature = "as-library"))]
use crate::c_api::{
    get_llapi_ctx, json_api_free_iter, json_api_free_key_values_iter, json_api_get,
    json_api_get_at, json_api_get_boolean, json_api_get_double, json_api_get_int,
    json_api_get_json, json_api_get_json_from_iter, json_api_get_key_value, json_api_get_len,
    json_api_get_string, json_api_get_type, json_api_is_json, json_api_len, json_api_next,
    json_api_next_key_value, json_api_open_key_internal, json_api_open_key_with_flags_internal,
    json_api_reset_iter, LLAPI_CTX,
};
use crate::redisjson::Format;

mod array_index;
mod backward;
pub mod c_api;
pub mod commands;
pub mod error;
mod formatter;
pub mod ivalue_manager;
mod key_value;
pub mod manager;
pub mod redisjson;

pub const GIT_SHA: Option<&str> = std::option_env!("GIT_SHA");
pub const GIT_BRANCH: Option<&str> = std::option_env!("GIT_BRANCH");
pub const MODULE_NAME: &str = "ReJSON";
pub const MODULE_TYPE_NAME: &str = "ReJSON-RL";

pub const REDIS_JSON_TYPE_VERSION: i32 = 3;

pub static REDIS_JSON_TYPE: RedisType = RedisType::new(
    MODULE_TYPE_NAME,
    REDIS_JSON_TYPE_VERSION,
    RedisModuleTypeMethods {
        version: redis_module::TYPE_METHOD_VERSION,

        rdb_load: Some(redisjson::type_methods::rdb_load),
        rdb_save: Some(redisjson::type_methods::rdb_save),
        aof_rewrite: None, // TODO add support
        free: Some(redisjson::type_methods::free),

        // Currently unused by Redis
        mem_usage: Some(redisjson::type_methods::mem_usage),
        digest: None,

        // Auxiliary data (v2)
        aux_load: None,
        aux_save: None,
        aux_save_triggers: 0,

        free_effort: None,
        unlink: None,
        copy: Some(redisjson::type_methods::copy),
        defrag: None,

        free_effort2: None,
        unlink2: None,
        copy2: None,
        mem_usage2: None,
        aux_save2: None,
    },
);
/////////////////////////////////////////////////////

#[macro_export]
macro_rules! run_on_manager {
    (
    pre_command: $pre_command_expr:expr,
    get_mngr: $get_mngr_expr:expr,
    run: $run_expr:expr,
    ) => {{
        $pre_command_expr();
        let m = $get_mngr_expr;
        match m {
            Some(mngr) => $run_expr(mngr),
            None => $run_expr($crate::ivalue_manager::RedisIValueJsonKeyManager {
                phantom: PhantomData,
            }),
        }
    }};
}

#[macro_export]
macro_rules! redis_json_module_create {(
        data_types: [
            $($data_type:ident),* $(,)*
        ],
        pre_command_function: $pre_command_function_expr:expr,
        get_manage: $get_manager_expr:expr,
        version: $version:expr,
        init: $init_func:expr,
        info: $info_func:ident,
    ) => {

        use redis_module::RedisString;
        use std::marker::PhantomData;
        use std::os::raw::{c_double, c_int, c_longlong};
        use redis_module::raw as rawmod;
        use rawmod::ModuleOptions;
        use redis_module::redis_module;
        use redis_module::logging::RedisLogLevel;

        use std::{
            ffi::{CStr, CString},
            os::raw::{c_char, c_void},
        };
        use libc::size_t;
        use std::collections::HashMap;

        macro_rules! json_command {
            ($cmd:ident) => {
                |ctx: &Context, args: Vec<RedisString>| -> RedisResult {
                    run_on_manager!(
                        pre_command: ||$pre_command_function_expr(ctx, &args),
                        get_mngr: $get_manager_expr,
                        run: |mngr|$cmd(mngr, ctx, args),
                    )
                }
            };
        }

        #[cfg(not(test))]
        macro_rules! get_allocator {
            () => {
                redis_module::alloc::RedisAlloc
            };
        }

        #[cfg(test)]
        macro_rules! get_allocator {
            () => {
                std::alloc::System
            };
        }

        redis_json_module_export_shared_api! {
            get_manage:$get_manager_expr,
            pre_command_function: $pre_command_function_expr,
        }

        fn initialize(ctx: &Context, args: &[RedisString]) -> Status {
            ctx.log_notice(&format!("version: {} git sha: {} branch: {}",
                $version,
                match GIT_SHA { Some(val) => val, _ => "unknown"},
                match GIT_BRANCH { Some(val) => val, _ => "unknown"},
                ));
            export_shared_api(ctx);
            ctx.set_module_options(ModuleOptions::HANDLE_IO_ERRORS);
            ctx.log_notice("Enabled diskless replication");
            $init_func(ctx, args)
        }

        fn json_init_config(ctx: &Context, args: &[RedisString]) -> Status{
            if args.len() % 2 != 0 {
                ctx.log(RedisLogLevel::Warning, "RedisJson arguments must be key:value pairs");
                return Status::Err;
            }
            let mut args_map = HashMap::<String, String>::new();
            for i in (0..args.len()).step_by(2) {
                args_map.insert(args[i].to_string_lossy(), args[i + 1].to_string_lossy());
            }

            Status::Ok
        }

        redis_module! {
            name: $crate::MODULE_NAME,
            version: $version,
            allocator: (get_allocator!(), get_allocator!()),
            data_types: [$($data_type,)*],
            init: json_init_config,
            init: initialize,
            info: $info_func,
            commands: [
                ["json.del", json_command!(json_del), "write", 1,1,1],
                ["json.get", json_command!(json_get), "readonly", 1,1,1],
                ["json.mget", json_command!(json_mget), "readonly", 1,1,1],
                ["json.set", json_command!(json_set), "write deny-oom", 1,1,1],
                ["json.mset", json_command!(json_mset), "write deny-oom", 1,-1,3],
                ["json.type", json_command!(json_type), "readonly", 1,1,1],
                ["json.numincrby", json_command!(json_num_incrby), "write", 1,1,1],
                ["json.toggle", json_command!(json_bool_toggle), "write deny-oom", 1,1,1],
                ["json.nummultby", json_command!(json_num_multby), "write", 1,1,1],
                ["json.numpowby", json_command!(json_num_powby), "write", 1,1,1],
                ["json.strappend", json_command!(json_str_append), "write deny-oom", 1,1,1],
                ["json.strlen", json_command!(json_str_len), "readonly", 1,1,1],
                ["json.arrappend", json_command!(json_arr_append), "write deny-oom", 1,1,1],
                ["json.arrindex", json_command!(json_arr_index), "readonly", 1,1,1],
                ["json.arrinsert", json_command!(json_arr_insert), "write deny-oom", 1,1,1],
                ["json.arrlen", json_command!(json_arr_len), "readonly", 1,1,1],
                ["json.arrpop", json_command!(json_arr_pop), "write", 1,1,1],
                ["json.arrtrim", json_command!(json_arr_trim), "write", 1,1,1],
                ["json.objkeys", json_command!(json_obj_keys), "readonly", 1,1,1],
                ["json.objlen", json_command!(json_obj_len), "readonly", 1,1,1],
                ["json.clear", json_command!(json_clear), "write", 1,1,1],
                ["json.debug", json_command!(json_debug), "readonly", 2,2,1],
                ["json.forget", json_command!(json_del), "write", 1,1,1],
                ["json.resp", json_command!(json_resp), "readonly", 1,1,1],
                ["json.merge", json_command!(json_merge), "write deny-oom", 1,1,1],
            ],
        }
    }
}

#[cfg(not(feature = "as-library"))]
const fn pre_command(_ctx: &Context, _args: &[RedisString]) {}

#[cfg(not(feature = "as-library"))]
const fn dummy_init(_ctx: &Context, _args: &[RedisString]) -> Status {
    Status::Ok
}

#[cfg(not(feature = "as-library"))]
const fn dummy_info(_ctx: &InfoContext, _for_crash_report: bool) {}

#[cfg(not(feature = "as-library"))]
redis_json_module_create! {
    data_types: [REDIS_JSON_TYPE],
    pre_command_function: pre_command,
    get_manage: Some(ivalue_manager::RedisIValueJsonKeyManager{phantom:PhantomData}),
    version: 02_06_10,
    init: dummy_init,
    info: dummy_info,
}
