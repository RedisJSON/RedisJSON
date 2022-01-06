extern crate redis_module;

use redis_module::native_types::RedisType;
use redis_module::raw::RedisModuleTypeMethods;

#[cfg(not(feature = "as-library"))]
use redis_module::Status;
#[cfg(not(feature = "as-library"))]
use redis_module::{Context, RedisResult};

#[cfg(not(feature = "as-library"))]
use crate::c_api::{
    get_llapi_ctx, json_api_free_iter, json_api_get, json_api_get_at, json_api_get_boolean,
    json_api_get_double, json_api_get_int, json_api_get_json, json_api_get_len,
    json_api_get_string, json_api_get_type, json_api_is_json, json_api_len, json_api_next,
    json_api_open_key_internal, LLAPI_CTX,
};
use crate::redisjson::Format;

mod array_index;
mod backward;
pub mod c_api;
pub mod commands;
pub mod error;
mod formatter;
pub mod ivalue_manager;
pub mod manager;
mod nodevisitor;
pub mod redisjson;

pub const GIT_SHA: Option<&'static str> = std::option_env!("GIT_SHA");
pub const GIT_BRANCH: Option<&'static str> = std::option_env!("GIT_BRANCH");
pub const MODULE_NAME: &'static str = "ReJSON";
pub const MODULE_TYPE_NAME: &'static str = "ReJSON-RL";

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
        copy: None,
        defrag: None,
    },
);
/////////////////////////////////////////////////////

#[derive(Copy, Clone)]
pub enum ManagerType {
    SerdeValue,
    IValue,
}

pub static mut MANAGER: ManagerType = ManagerType::IValue;

pub fn get_manager_type() -> ManagerType {
    unsafe { MANAGER }
}

#[macro_export]
macro_rules! run_on_manager {
    (
    $run:expr, $ctx:ident, $args: ident
    ) => {
        match $crate::get_manager_type() {
            $crate::ManagerType::IValue => $run(
                $crate::ivalue_manager::RedisIValueJsonKeyManager {
                    phantom: PhantomData,
                },
                $ctx,
                $args,
            ),
            $crate::ManagerType::SerdeValue => $run(
                $crate::manager::RedisJsonKeyManager {
                    phantom: PhantomData,
                },
                $ctx,
                $args,
            ),
        }
    };
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
    ) => {

        use redis_module::{redis_command, redis_module, RedisString};
        use std::marker::PhantomData;
        use std::os::raw::{c_double, c_int, c_longlong};
        use redis_module::{raw as rawmod, LogLevel};
        use rawmod::ModuleOptions;
        use std::{
            ffi::CStr,
            os::raw::{c_char, c_void},
        };
        use libc::size_t;
        use std::collections::HashMap;

        ///
        /// JSON.DEL <key> [path]
        ///
        fn json_del(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_del(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_del, ctx, args),

            }
        }

        ///
        /// JSON.GET <key>
        ///         [INDENT indentation-string]
        ///         [NEWLINE line-break-string]
        ///         [SPACE space-string]
        ///         [path ...]
        ///
        /// TODO add support for multi path
        fn json_get(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_get(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_get, ctx, args)

            }
        }

        ///
        /// JSON.SET <key> <path> <json> [NX | XX | FORMAT <format>]
        ///
        fn json_set(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_set(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_set, ctx, args)
            }
        }

        ///
        /// JSON.MGET <key> [key ...] <path>
        ///
        fn json_mget(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_mget(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_mget, ctx, args)

            }
        }

        ///
        /// JSON.STRLEN <key> [path]
        ///
        fn json_str_len(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_str_len(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_str_len, ctx, args)

            }
        }

        ///
        /// JSON.TYPE <key> [path]
        ///
        fn json_type(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_type(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_type, ctx, args)

            }
        }

        ///
        /// JSON.NUMINCRBY <key> <path> <number>
        ///
        fn json_num_incrby(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_num_incrby(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_num_incrby, ctx, args)

            }
        }

        ///
        /// JSON.NUMMULTBY <key> <path> <number>
        ///
        fn json_num_multby(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_num_multby(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_num_multby, ctx, args)

            }
        }

        ///
        /// JSON.NUMPOWBY <key> <path> <number>
        ///
        fn json_num_powby(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_num_powby(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_num_powby, ctx, args)

            }
        }

        //
        /// JSON.TOGGLE <key> <path>
        fn json_bool_toggle(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_bool_toggle(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_bool_toggle, ctx, args)

            }
        }

        ///
        /// JSON.STRAPPEND <key> [path] <json-string>
        ///
        fn json_str_append(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_str_append(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_str_append, ctx, args)

            }
        }

        ///
        /// JSON.ARRAPPEND <key> <path> <json> [json ...]
        ///
        fn json_arr_append(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_arr_append(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_arr_append, ctx, args)

            }
        }

        ///
        /// JSON.ARRINDEX <key> <path> <json-scalar> [start [stop]]
        ///
        /// scalar - number, string, Boolean (true or false), or null
        ///
        fn json_arr_index(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_arr_index(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_arr_index, ctx, args)

            }
        }

        ///
        /// JSON.ARRINSERT <key> <path> <index> <json> [json ...]
        ///
        fn json_arr_insert(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_arr_insert(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_arr_insert, ctx, args)
            }
        }

        ///
        /// JSON.ARRLEN <key> [path]
        ///
        fn json_arr_len(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_arr_len(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_arr_len, ctx, args)

            }
        }

        ///
        /// JSON.ARRPOP <key> [path [index]]
        ///
        fn json_arr_pop(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_arr_pop(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_arr_pop, ctx, args)

            }
        }

        ///
        /// JSON.ARRTRIM <key> <path> <start> <stop>
        ///
        fn json_arr_trim(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_arr_trim(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_arr_trim, ctx, args)

            }
        }

        ///
        /// JSON.OBJKEYS <key> [path]
        ///
        fn json_obj_keys(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_obj_keys(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_obj_keys, ctx, args)

            }
        }

        ///
        /// JSON.OBJLEN <key> [path]
        ///
        fn json_obj_len(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_obj_len(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_obj_len, ctx, args)

            }
        }

        ///
        /// JSON.CLEAR <key> [path ...]
        ///
        fn json_clear(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_clear(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_clear, ctx, args)

            }
        }

        ///
        /// JSON.DEBUG <subcommand & arguments>
        ///
        /// subcommands:
        /// MEMORY <key> [path]
        /// HELP
        ///
        fn json_debug(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_debug(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_debug, ctx, args)

            }
        }

        ///
        /// JSON.RESP <key> [path]
        ///
        fn json_resp(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
            $pre_command_function_expr(ctx, &args);
            let m = $get_manager_expr;
            match m {
                Some(mngr) => commands::command_json_resp(mngr, ctx, args),
                None => run_on_manager!(commands::command_json_resp, ctx, args)

            }
        }

        redis_json_module_export_shared_api! {
            get_manage:$get_manager_expr,
            pre_command_function: $pre_command_function_expr,
        }

        fn intialize(ctx: &Context, args: &Vec<RedisString>) -> Status {
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

        fn json_init_config(ctx: &Context, args: &Vec<RedisString>) -> Status{
            if args.len() % 2 != 0 {
                ctx.log(LogLevel::Warning, "RedisJson arguments must be key:value pairs");
                return Status::Err;
            }
            let mut args_map = HashMap::<String, String>::new();
            for i in (0..args.len()).step_by(2) {
                args_map.insert(args[i].to_string_lossy(), args[i + 1].to_string_lossy());
            }

            if let Some(backend) = args_map.get("JSON_BACKEND") {
                if  backend == "SERDE_JSON" {
                    unsafe {$crate::MANAGER = $crate::ManagerType::SerdeValue};
                } else if backend == "IJSON" {
                    unsafe {$crate::MANAGER = $crate::ManagerType::IValue};
                } else {
                    ctx.log(LogLevel::Warning, "Unsupported json backend was given");
                    return Status::Err;
                }
            }

            Status::Ok
        }

        redis_module! {
            name: crate::MODULE_NAME,
            version: $version,
            data_types: [$($data_type,)*],
            init: json_init_config,
            init: intialize,
            commands: [
                ["json.del", json_del, "write", 1,1,1],
                ["json.get", json_get, "readonly", 1,1,1],
                ["json.mget", json_mget, "readonly", 1,1,1],
                ["json.set", json_set, "write deny-oom", 1,1,1],
                ["json.type", json_type, "readonly", 1,1,1],
                ["json.numincrby", json_num_incrby, "write", 1,1,1],
                ["json.toggle", json_bool_toggle, "write deny-oom", 1,1,1],
                ["json.nummultby", json_num_multby, "write", 1,1,1],
                ["json.numpowby", json_num_powby, "write", 1,1,1],
                ["json.strappend", json_str_append, "write deny-oom", 1,1,1],
                ["json.strlen", json_str_len, "readonly", 1,1,1],
                ["json.arrappend", json_arr_append, "write deny-oom", 1,1,1],
                ["json.arrindex", json_arr_index, "readonly", 1,1,1],
                ["json.arrinsert", json_arr_insert, "write deny-oom", 1,1,1],
                ["json.arrlen", json_arr_len, "readonly", 1,1,1],
                ["json.arrpop", json_arr_pop, "write", 1,1,1],
                ["json.arrtrim", json_arr_trim, "write", 1,1,1],
                ["json.objkeys", json_obj_keys, "readonly", 1,1,1],
                ["json.objlen", json_obj_len, "readonly", 1,1,1],
                ["json.clear", json_clear, "write", 1,1,1],
                ["json.debug", json_debug, "readonly", 2,2,1],
                ["json.forget", json_del, "write", 1,1,1],
                ["json.resp", json_resp, "readonly", 1,1,1],
            ],
        }
    }
}

#[cfg(not(feature = "as-library"))]
fn pre_command(_ctx: &Context, _args: &Vec<RedisString>) {}

#[cfg(not(feature = "as-library"))]
fn dummy_init(_ctx: &Context, _args: &Vec<RedisString>) -> Status {
    Status::Ok
}

#[cfg(not(feature = "as-library"))]
redis_json_module_create! {
    data_types: [REDIS_JSON_TYPE],
    pre_command_function: pre_command,
    get_manage: {
        match get_manager_type() {
            ManagerType::IValue => Some(ivalue_manager::RedisIValueJsonKeyManager{phantom:PhantomData}),
            _ => None,
        }
    },
    version: 99_99_99,
    init: dummy_init,
}
