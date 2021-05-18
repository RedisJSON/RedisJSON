#[cfg(not(feature = "as-library"))]
#[macro_use]
extern crate redis_module;

use redis_module::native_types::RedisType;
use redis_module::raw::RedisModuleTypeMethods;
#[cfg(not(feature = "as-library"))]
use redis_module::{Context, RedisResult};

mod array_index;
mod backward;
pub mod commands;
pub mod error;
mod formatter;
pub mod manager;
mod nodevisitor;
pub mod redisjson;

use crate::redisjson::Format;
pub const REDIS_JSON_TYPE_VERSION: i32 = 3;

pub static REDIS_JSON_TYPE: RedisType = RedisType::new(
    "ReJSON-RL",
    REDIS_JSON_TYPE_VERSION,
    RedisModuleTypeMethods {
        version: redis_module::TYPE_METHOD_VERSION,

        rdb_load: Some(redisjson::type_methods::rdb_load),
        rdb_save: Some(redisjson::type_methods::rdb_save),
        aof_rewrite: None, // TODO add support
        free: Some(redisjson::type_methods::free),

        // Currently unused by Redis
        mem_usage: None,
        digest: None,

        // Auxiliary data (v2)
        aux_load: Some(redisjson::type_methods::aux_load),
        aux_save: None,
        aux_save_triggers: 0,

        free_effort: None,
        unlink: None,
        copy: None,
        defrag: None,
    },
);

///
/// JSON.DEL <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_del(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_del(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.GET <key>
///         [INDENT indentation-string]
///         [NEWLINE line-break-string]
///         [SPACE space-string]
///         [path ...]
///
/// TODO add support for multi path
#[cfg(not(feature = "as-library"))]
fn json_get(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_get(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.SET <key> <path> <json> [NX | XX | FORMAT <format>]
///
#[cfg(not(feature = "as-library"))]
fn json_set(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_set(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.MGET <key> [key ...] <path>
///
#[cfg(not(feature = "as-library"))]
fn json_mget(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_mget(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.STRLEN <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_str_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_str_len(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.TYPE <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_type(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_type(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.NUMINCRBY <key> <path> <number>
///
#[cfg(not(feature = "as-library"))]
fn json_num_incrby(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_num_incrby(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.NUMMULTBY <key> <path> <number>
///
#[cfg(not(feature = "as-library"))]
fn json_num_multby(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_num_multby(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.NUMPOWBY <key> <path> <number>
///
#[cfg(not(feature = "as-library"))]
fn json_num_powby(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_num_powby(manager::RedisJsonKeyManager, ctx, args)
}
//
/// JSON.TOGGLE <key> <path>
#[cfg(not(feature = "as-library"))]
fn json_bool_toggle(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_bool_toggle(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.STRAPPEND <key> [path] <json-string>
///
#[cfg(not(feature = "as-library"))]
fn json_str_append(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_str_append(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.ARRAPPEND <key> <path> <json> [json ...]
///
#[cfg(not(feature = "as-library"))]
fn json_arr_append(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_arr_append(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.ARRINDEX <key> <path> <json-scalar> [start [stop]]
///
/// scalar - number, string, Boolean (true or false), or null
///
#[cfg(not(feature = "as-library"))]
fn json_arr_index(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_arr_index(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.ARRINSERT <key> <path> <index> <json> [json ...]
///
#[cfg(not(feature = "as-library"))]
fn json_arr_insert(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_arr_insert(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.ARRLEN <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_arr_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_arr_len(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.ARRPOP <key> [path [index]]
///
#[cfg(not(feature = "as-library"))]
fn json_arr_pop(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_arr_pop(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.ARRTRIM <key> <path> <start> <stop>
///
#[cfg(not(feature = "as-library"))]
fn json_arr_trim(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_arr_trim(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.OBJKEYS <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_obj_keys(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_obj_keys(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.OBJLEN <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_obj_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_obj_len(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.CLEAR <key> [path ...]
///
#[cfg(not(feature = "as-library"))]
fn json_clear(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_clear(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.DEBUG <subcommand & arguments>
///
/// subcommands:
/// MEMORY <key> [path]
/// HELP
///
#[cfg(not(feature = "as-library"))]
fn json_debug(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_debug(manager::RedisJsonKeyManager, ctx, args)
}

///
/// JSON.RESP <key> [path]
///
#[cfg(not(feature = "as-library"))]
fn json_resp(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_resp(manager::RedisJsonKeyManager, ctx, args)
}

#[cfg(not(feature = "as-library"))]
fn json_cache_info(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_cache_info(manager::RedisJsonKeyManager, ctx, args)
}

#[cfg(not(feature = "as-library"))]
fn json_cache_init(ctx: &Context, args: Vec<String>) -> RedisResult {
    commands::command_json_cache_init(manager::RedisJsonKeyManager, ctx, args)
}
//////////////////////////////////////////////////////

#[cfg(not(feature = "as-library"))]
redis_module! {
    name: "ReJSON",
    version: 99_99_99,
    data_types: [
        REDIS_JSON_TYPE,
    ],
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
        ["json.debug", json_debug, "readonly", 1,1,1],
        ["json.forget", json_del, "write", 1,1,1],
        ["json.resp", json_resp, "readonly", 1,1,1],
        ["json._cacheinfo", json_cache_info, "readonly", 1,1,1],
        ["json._cacheinit", json_cache_init, "write", 1,1,1],
    ],
}
