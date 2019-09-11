#[macro_use]
extern crate redismodule;

use redismodule::native_types::RedisType;
use redismodule::raw::RedisModuleTypeMethods;
use redismodule::{raw as rawmod, NextArg};
use redismodule::{Context, RedisError, RedisResult, RedisValue, REDIS_OK};
use serde_json::{Number, Value};

use std::{i64, usize};

mod array_index;
mod backward;
mod commands;
mod error;
mod nodevisitor;
mod redisjson;
mod schema; // TODO: Remove

use crate::array_index::ArrayIndex;
use crate::commands::index;
use crate::error::Error;
use crate::redisjson::{Format, RedisJSON, SetOptions};

static REDIS_JSON_TYPE: RedisType = RedisType::new(
    "ReJSON-RL",
    2,
    RedisModuleTypeMethods {
        version: redismodule::TYPE_METHOD_VERSION,

        rdb_load: Some(redisjson::type_methods::rdb_load),
        rdb_save: Some(redisjson::type_methods::rdb_save),
        aof_rewrite: None, // TODO add support
        free: Some(redisjson::type_methods::free),

        // Currently unused by Redis
        mem_usage: None,
        digest: None,
    },
);

///
/// Backwards compatibility convertor for RedisJSON 1.x clients
///
fn backwards_compat_path(mut path: String) -> String {
    if !path.starts_with("$") {
        if path == "." {
            path.replace_range(..1, "$");
        } else if path.starts_with(".") {
            path.insert(0, '$');
        } else {
            path.insert_str(0, "$.");
        }
    }
    return path;
}

///
/// JSON.DEL <key> [path]
///
fn json_del(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = ctx.open_key_writable(&key);
    let deleted = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => {
            if path == "$" {
                key.delete()?;
                1
            } else {
                doc.delete_path(&path)?
            }
        }
        None => 0,
    };
    Ok(deleted.into())
}

///
/// JSON.SET <key> <path> <json> [NX | XX | FORMAT <format> | INDEX <index>]
///
fn json_set(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let value = args.next_string()?;

    let mut format = Format::JSON;
    let mut set_option = SetOptions::None;
    let mut index = None;

    loop {
        if let Some(s) = args.next() {
            match s.to_uppercase().as_str() {
                "NX" => set_option = SetOptions::NotExists,
                "XX" => set_option = SetOptions::AlreadyExists,
                "FORMAT" => {
                    format = Format::from_str(args.next_string()?.as_str())?;
                }
                "INDEX" => {
                    index = Some(args.next_string()?);
                }
                _ => break,
            };
        } else {
            break;
        }
    }

    let redis_key = ctx.open_key_writable(&key);
    let current = redis_key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;

    match (current, set_option) {
        (Some(ref mut doc), ref op) => {
            if doc.set_value(&value, &path, op, format)? {
                if let Some(index) = index {
                    index::add_document(&key, &index, &doc)?;
                }
                REDIS_OK
            } else {
                Ok(RedisValue::None)
            }
        }
        (None, SetOptions::AlreadyExists) => Ok(RedisValue::None),
        (None, _) => {
            let doc = RedisJSON::from_str(&value, format)?;
            if path == "$" {
                redis_key.set_value(&REDIS_JSON_TYPE, doc)?;

                if let Some(index) = index {
                    // FIXME: We need to get the value even though we just set it,
                    // since the original doc is consumed by set_value.
                    // Can we do better than this?
                    let doc = redis_key.get_value(&REDIS_JSON_TYPE)?.unwrap();
                    index::add_document(&key, &index, doc)?;
                }

                REDIS_OK
            } else {
                Err("ERR new objects must be created at the root".into())
            }
        }
    }
}

///
/// JSON.GET <key>
///         [INDENT indentation-string]
///         [NEWLINE line-break-string]
///         [SPACE space-string]
///         [NOESCAPE]
///         [path ...]
///
/// TODO add support for multi path
fn json_get(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;

    let mut paths: Vec<String> = vec![];
    let mut first_loop = true;
    let mut format = Format::JSON;
    loop {
        let arg = match args.next_string() {
            Ok(s) => s,
            Err(_) => {
                // path is optional -> no path found on the first loop we use root "$"
                if first_loop {
                    paths.push("$".to_owned());
                }
                break;
            }
        };
        first_loop = false;

        match arg.as_str() {
            "INDENT" => {
                args.next();
            } // TODO add support
            "NEWLINE" => {
                args.next();
            } // TODO add support
            "SPACE" => {
                args.next();
            } // TODO add support
            "NOESCAPE" => {
                continue;
            } // TODO add support
            "FORMAT" => {
                format = Format::from_str(args.next_string()?.as_str())?;
            }
            _ => {
                paths.push(backwards_compat_path(arg));
            }
        };
    }

    let key = ctx.open_key_writable(&key);
    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => if paths.len() == 1 {
            doc.to_string(&paths[0], format)?
        } else {
            // can't be smaller than 1
            doc.to_json(&mut paths)?
        }
        .into(),
        None => RedisValue::None,
    };

    Ok(value)
}

///
/// JSON.MGET <key> [key ...] <path>
///
fn json_mget(ctx: &Context, args: Vec<String>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    args.last().ok_or(RedisError::WrongArity).and_then(|path| {
        let path = backwards_compat_path(path.to_string());
        let keys = &args[1..args.len() - 1];

        let results: Result<Vec<RedisValue>, RedisError> = keys
            .iter()
            .map(|key| {
                let result = ctx
                    .open_key(key)
                    .get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
                    .map(|doc| doc.to_string(&path, Format::JSON))
                    .transpose()?;

                Ok(result.into())
            })
            .collect();

        Ok(results?.into())
    })
}

///
/// JSON.STRLEN <key> [path]
///
fn json_str_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_len(ctx, args, |doc, path| doc.str_len(path))
}

///
/// JSON.TYPE <key> [path]
///
fn json_type(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = ctx.open_key(&key);

    let value = key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?.map_or_else(
        || RedisValue::None,
        |doc| match doc.get_type(&path) {
            Ok(s) => s.into(),
            Err(_) => RedisValue::None,
        },
    );

    Ok(value)
}

///
/// JSON.NUMINCRBY <key> <path> <number>
///
fn json_num_incrby(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_num_op(ctx, args, |i1, i2| i1 + i2, |f1, f2| f1 + f2)
}

///
/// JSON.NUMMULTBY <key> <path> <number>
///
fn json_num_multby(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_num_op(ctx, args, |i1, i2| i1 * i2, |f1, f2| f1 * f2)
}

///
/// JSON.NUMPOWBY <key> <path> <number>
///
fn json_num_powby(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_num_op(ctx, args, |i1, i2| i1.pow(i2 as u32), |f1, f2| f1.powf(f2))
}

fn json_num_op<I, F>(ctx: &Context, args: Vec<String>, op_i64: I, op_f64: F) -> RedisResult
where
    I: Fn(i64, i64) -> i64,
    F: Fn(f64, f64) -> f64,
{
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let number = args.next_string()?;

    let key = ctx.open_key_writable(&key);

    key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .ok_or_else(RedisError::nonexistent_key)
        .and_then(|doc| {
            doc.value_op(&path, |value| {
                do_json_num_op(&number, value, &op_i64, &op_f64)
            })
            .map(|v| v.to_string().into())
            .map_err(|e| e.into())
        })
}

fn do_json_num_op<I, F>(
    in_value: &str,
    curr_value: &Value,
    op_i64: I,
    op_f64: F,
) -> Result<Value, Error>
where
    I: FnOnce(i64, i64) -> i64,
    F: FnOnce(f64, f64) -> f64,
{
    if let Value::Number(curr_value) = curr_value {
        let in_value = &serde_json::from_str(in_value)?;
        if let Value::Number(in_value) = in_value {
            let num_res = match (curr_value.as_i64(), in_value.as_i64()) {
                (Some(num1), Some(num2)) => op_i64(num1, num2).into(),
                _ => {
                    let num1 = curr_value.as_f64().unwrap();
                    let num2 = in_value.as_f64().unwrap();
                    Number::from_f64(op_f64(num1, num2)).unwrap()
                }
            };

            Ok(Value::Number(num_res))
        } else {
            Err(err_json(in_value, "number"))
        }
    } else {
        Err(err_json(curr_value, "number"))
    }
}

fn err_json(value: &Value, expected_value: &'static str) -> Error {
    Error::from(format!(
        "ERR wrong type of path value - expected {} but found {}",
        expected_value,
        RedisJSON::value_name(value)
    ))
}

///
/// JSON.STRAPPEND <key> [path] <json-string>
///
fn json_str_append(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path_or_json = args.next_string()?;

    let path;
    let json;

    // path is optional
    if let Ok(val) = args.next_string() {
        path = backwards_compat_path(path_or_json);
        json = val;
    } else {
        path = "$".to_string();
        json = path_or_json;
    }

    let key = ctx.open_key_writable(&key);

    key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .ok_or_else(RedisError::nonexistent_key)
        .and_then(|doc| {
            doc.value_op(&path, |value| do_json_str_append(&json, value))
                .map(|v| v.as_str().map_or(usize::MAX, |v| v.len()).into())
                .map_err(|e| e.into())
        })
}

fn do_json_str_append(json: &String, value: &Value) -> Result<Value, Error> {
    value
        .as_str()
        .ok_or_else(|| err_json(value, "string"))
        .and_then(|curr| {
            let v = serde_json::from_str(json)?;
            if let Value::String(s) = v {
                let new_value = [curr, s.as_str()].concat();
                Ok(Value::String(new_value))
            } else {
                Err(format!("ERR wrong type of value - expected string but found {}", v).into())
            }
        })
}

///
/// JSON.ARRAPPEND <key> <path> <json> [json ...]
///
fn json_arr_append(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    // We require at least one JSON item to append
    args.peek().ok_or(RedisError::WrongArity)?;

    let key = ctx.open_key_writable(&key);

    key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .ok_or_else(RedisError::nonexistent_key)
        .and_then(|doc| {
            doc.value_op(&path, |value| do_json_arr_append(args.clone(), value))
                .map(|v| v.as_array().map_or(usize::MAX, |v| v.len()).into())
                .map_err(|e| e.into())
        })
}

fn do_json_arr_append<I>(args: I, value: &Value) -> Result<Value, Error>
where
    I: Iterator<Item = String>,
{
    value
        .as_array()
        .ok_or_else(|| err_json(value, "array"))
        .and_then(|curr| {
            let items: Vec<Value> = args
                .map(|json| serde_json::from_str(&json))
                .collect::<Result<_, _>>()?;

            let new_value = [curr.as_slice(), &items].concat();
            Ok(Value::Array(new_value))
        })
}

///
/// JSON.ARRINDEX <key> <path> <json-scalar> [start [stop]]
///
/// scalar - number, string, Boolean (true or false), or null
///
fn json_arr_index(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let json_scalar = args.next_string()?;
    let start: i64 = args.next().map(|v| v.parse()).unwrap_or(Ok(0))?;
    let end: i64 = args.next().map(|v| v.parse()).unwrap_or(Ok(i64::MAX))?;

    args.done()?; // TODO: Add to other functions as well to terminate args list

    let key = ctx.open_key(&key);

    let index = key
        .get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .map_or(Ok(-1), |doc| doc.arr_index(&path, &json_scalar, start, end))?;

    Ok(index.into())
}

///
/// JSON.ARRINSERT <key> <path> <index> <json> [json ...]
///
fn json_arr_insert(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let index = args.next_i64()?;

    // We require at least one JSON item to append
    args.peek().ok_or(RedisError::WrongArity)?;

    let key = ctx.open_key_writable(&key);

    key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .ok_or_else(RedisError::nonexistent_key)
        .and_then(|doc| {
            doc.value_op(&path, |value| {
                do_json_arr_insert(args.clone(), index, value)
            })
            .map(|v| v.as_array().map_or(usize::MAX, |v| v.len()).into())
            .map_err(|e| e.into())
        })
}

fn do_json_arr_insert<I>(args: I, index: i64, value: &Value) -> Result<Value, Error>
where
    I: Iterator<Item = String>,
{
    value
        .as_array()
        .ok_or_else(|| err_json(value, "array"))
        .and_then(|curr| {
            let len = curr.len() as i64;

            if !(-len..len).contains(&index) {
                return Err("ERR index out of bounds".into());
            }

            let index = index.normalize(len);

            let items: Vec<Value> = args
                .map(|json| serde_json::from_str(&json))
                .collect::<Result<_, _>>()?;

            let mut new_value = curr.to_owned();
            new_value.splice(index..index, items.into_iter());

            Ok(Value::Array(new_value))
        })
}

///
/// JSON.ARRLEN <key> [path]
///
fn json_arr_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_len(ctx, args, |doc, path| doc.arr_len(path))
}

///
/// JSON.ARRPOP <key> [path [index]]
///
fn json_arr_pop(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;

    let (path, index) = args
        .next()
        .map(|p| {
            let path = backwards_compat_path(p);
            let index = args.next_i64().unwrap_or(i64::MAX);
            (path, index)
        })
        .unwrap_or(("$".to_string(), i64::MAX));

    let key = ctx.open_key_writable(&key);
    let mut res = Value::Null;

    key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .ok_or_else(RedisError::nonexistent_key)
        .and_then(|doc| {
            doc.value_op(&path, |value| do_json_arr_pop(index, &mut res, value))
                .map_err(|e| e.into())
        })?;
    Ok(RedisJSON::serialize(&res, Format::JSON)?.into())
}

fn do_json_arr_pop(mut index: i64, res: &mut Value, value: &Value) -> Result<Value, Error> {
    value
        .as_array()
        .ok_or_else(|| err_json(value, "array"))
        .and_then(|curr| {
            let len = curr.len() as i64;

            index = index.min(len - 1);

            if index < 0 {
                index = len + index;
            }

            if index >= len || index < 0 {
                return Err("ERR index out of bounds".into());
            }

            let mut new_value = curr.to_owned();
            *res = new_value.remove(index as usize);
            Ok(Value::Array(new_value))
        })
}

///
/// JSON.ARRTRIM <key> <path> <start> <stop>
///
fn json_arr_trim(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let start = args.next_i64()?;
    let stop = args.next_i64()?;

    let key = ctx.open_key_writable(&key);

    key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?
        .ok_or_else(RedisError::nonexistent_key)
        .and_then(|doc| {
            doc.value_op(&path, |value| do_json_arr_trim(start, stop, &value))
                .map(|v| v.as_array().map_or(usize::MAX, |v| v.len()).into())
                .map_err(|e| e.into())
        })
}

fn do_json_arr_trim(start: i64, stop: i64, value: &Value) -> Result<Value, Error> {
    value
        .as_array()
        .ok_or_else(|| err_json(value, "array"))
        .and_then(|curr| {
            let len = curr.len() as i64;
            let stop = stop.normalize(len);

            let range = if start > len || start > stop as i64 {
                0..0 // Return an empty array
            } else {
                start.normalize(len)..(stop + 1)
            };

            let res = &curr[range];
            Ok(Value::Array(res.to_vec()))
        })
}

///
/// JSON.OBJKEYS <key> [path]
///
fn json_obj_keys(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = ctx.open_key(&key);

    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.obj_keys(&path)?.into(),
        None => RedisValue::None,
    };

    Ok(value)
}

///
/// JSON.OBJLEN <key> [path]
///
fn json_obj_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_len(ctx, args, |doc, path| doc.obj_len(path))
}

///
/// JSON.DEBUG <subcommand & arguments>
///
/// subcommands:
/// MEMORY <key> [path]
/// HELP
///
fn json_debug(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    match args.next_string()?.to_uppercase().as_str() {
        "MEMORY" => {
            let key = args.next_string()?;
            let path = backwards_compat_path(args.next_string()?);

            let key = ctx.open_key(&key);
            let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
                Some(doc) => doc.get_memory(&path)?,
                None => 0,
            };
            Ok(value.into())
        }
        "HELP" => {
            let results = vec![
                "MEMORY <key> [path] - reports memory usage",
                "HELP                - this message",
            ];
            Ok(results.into())
        }
        _ => Err("ERR unknown subcommand - try `JSON.DEBUG HELP`".into()),
    }
}

///
/// JSON.RESP <key> [path]
///
fn json_resp(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = ctx.open_key(&key);
    match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => Ok(resp_serialize(doc.get_doc(&path)?)),
        None => Ok(RedisValue::None),
    }
}

fn resp_serialize(doc: &Value) -> RedisValue {
    match doc {
        Value::Null => RedisValue::None,

        Value::Bool(b) => RedisValue::SimpleString(b.to_string()),

        Value::Number(n) => n
            .as_i64()
            .map(|i| RedisValue::Integer(i))
            .unwrap_or_else(|| RedisValue::Float(n.as_f64().unwrap())),

        Value::String(s) => RedisValue::BulkString(s.clone()),

        Value::Array(arr) => {
            let mut res: Vec<RedisValue> = Vec::with_capacity(arr.len() + 1);
            res.push(RedisValue::SimpleStringStatic("["));
            arr.iter().for_each(|v| res.push(resp_serialize(v)));
            RedisValue::Array(res)
        }

        Value::Object(obj) => {
            let mut res: Vec<RedisValue> = Vec::with_capacity(obj.len() + 1);
            res.push(RedisValue::SimpleStringStatic("{"));
            for (key, value) in obj.iter() {
                res.push(RedisValue::BulkString(key.to_string()));
                res.push(resp_serialize(value));
            }
            RedisValue::Array(res)
        }
    }
}

fn json_len<F: Fn(&RedisJSON, &String) -> Result<usize, Error>>(
    ctx: &Context,
    args: Vec<String>,
    fun: F,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = ctx.open_key(&key);
    let length = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => fun(&doc, &path)?.into(),
        None => RedisValue::None,
    };

    Ok(length)
}

fn json_cache_info(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_cache_init(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}
//////////////////////////////////////////////////////

pub extern "C" fn init(raw_ctx: *mut rawmod::RedisModuleCtx) -> c_int {
    crate::commands::index::schema_map::init();
    redisearch_api::init(raw_ctx)
}

redis_module! {
    name: "redisjson",
    version: 1,
    data_types: [
        REDIS_JSON_TYPE,
    ],
    init: init,
    commands: [
        ["json.del", json_del, "write"],
        ["json.get", json_get, ""],
        ["json.mget", json_mget, ""],
        ["json.set", json_set, "write"],
        ["json.type", json_type, ""],
        ["json.numincrby", json_num_incrby, "write"],
        ["json.nummultby", json_num_multby, "write"],
        ["json.numpowby", json_num_powby, "write"],
        ["json.strappend", json_str_append, "write"],
        ["json.strlen", json_str_len, ""],
        ["json.arrappend", json_arr_append, "write"],
        ["json.arrindex", json_arr_index, ""],
        ["json.arrinsert", json_arr_insert, "write"],
        ["json.arrlen", json_arr_len, ""],
        ["json.arrpop", json_arr_pop, "write"],
        ["json.arrtrim", json_arr_trim, "write"],
        ["json.objkeys", json_obj_keys, ""],
        ["json.objlen", json_obj_len, ""],
        ["json.debug", json_debug, ""],
        ["json.forget", json_del, "write"],
        ["json.resp", json_resp, ""],
        ["json.index", commands::index::index, "write"],
        ["json.qget", commands::index::qget, ""],
        ["json._cacheinfo", json_cache_info, ""],
        ["json._cacheinit", json_cache_init, "write"],
    ],
}
