#[macro_use]
extern crate redismodule;

use redismodule::native_types::RedisType;
use redismodule::{Context, NextArg, RedisError, RedisResult, REDIS_OK};

mod redisjson;

use crate::redisjson::{Error, RedisJSON};

static REDIS_JSON_TYPE: RedisType = RedisType::new("RedisJSON");

#[derive(Debug, PartialEq)]
pub enum SetOptions {
    NotExists,
    AlreadyExists,
}

///
/// backward compatibility convector for RedisJSON 1.x clients
///
fn backward_path(mut path: String) -> String {
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

fn json_del(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backward_path(args.next_string()?);

    let key = ctx.open_key_writable(&key);
    let deleted = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.delete_path(&path)?,
        None => 0,
    };
    Ok(deleted.into())
}

fn json_set(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backward_path(args.next_string()?);
    let value = args.next_string()?;

    let set_option = args
        .next()
        .map(|op| match op.to_uppercase().as_str() {
            "NX" => Ok(SetOptions::NotExists),
            "XX" => Ok(SetOptions::AlreadyExists),
            _ => Err(RedisError::Str("ERR syntax error")),
        })
        .transpose()?;

    let key = ctx.open_key_writable(&key);
    let current = key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;

    match (current, set_option) {
        (Some(_), Some(SetOptions::NotExists)) => Ok(().into()),
        (Some(ref mut doc), _) => {
            doc.set_value(&value, &path)?;
            REDIS_OK
        }
        (None, Some(SetOptions::AlreadyExists)) => Ok(().into()),
        (None, _) => {
            let doc = RedisJSON::from_str(&value)?;
            key.set_value(&REDIS_JSON_TYPE, doc)?;
            REDIS_OK
        }
    }
}

fn json_get(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;

    let mut path = loop {
        let arg = match args.next_string() {
            Ok(s) => s,
            Err(_) => "$".to_owned(), // path is optional
        };

        match arg.as_str() {
            "INDENT" => args.next(),  // TODO add support
            "NEWLINE" => args.next(), // TODO add support
            "SPACE" => args.next(),   // TODO add support
            "NOESCAPE" => continue,   // TODO add support
            _ => break arg,
        };
    };
    path = backward_path(path);

    let key = ctx.open_key_writable(&key);

    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.to_string(&path)?.into(),
        None => ().into(),
    };

    Ok(value)
}

fn json_mget(ctx: &Context, args: Vec<String>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }
    if let Some(path) = args.last() {
        let path = backward_path(path.to_string());
        let mut results: Vec<String> = Vec::with_capacity(args.len() - 2);
        for key in &args[1..args.len() - 1] {
            let redis_key = ctx.open_key_writable(&key);
            match redis_key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
                Some(doc) => {
                    let result = doc.to_string(&path)?;
                    results.push(result);
                }
                None => {}
            };
        }
        Ok(results.into())
    } else {
        Err(RedisError::WrongArity)
    }
}

fn json_str_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backward_path(args.next_string()?);

    let key = ctx.open_key_writable(&key);

    let length = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.str_len(&path)?.into(),
        None => ().into(),
    };

    Ok(length)
}

fn json_type(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backward_path(args.next_string()?);

    let key = ctx.open_key_writable(&key);

    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.get_type(&path)?.into(),
        None => ().into(),
    };

    Ok(value)
}

fn json_num_incrby(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_num_op(ctx, args, |num1, num2| num1 + num2)
}

fn json_num_multby(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_num_op(ctx, args, |num1, num2| num1 * num2)
}

fn json_num_powby(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_num_op(ctx, args, |num1, num2| num1.powf(num2))
}

fn json_num_op<F>(ctx: &Context, args: Vec<String>, fun: F) -> RedisResult
where
    F: Fn(f64, f64) -> f64,
{
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = args.next_string()?;
    let number: f64 = args.next_string()?.parse()?;

    let key = ctx.open_key_writable(&key);

    match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => Ok(doc.num_op(&path, number, fun)?.into()),
        None => Err("ERR could not perform this operation on a key that doesn't exist".into()),
    }
}

fn json_str_append(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_arr_append(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_arr_index(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_arr_insert(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_arr_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_len(ctx, args, |doc, path| doc.arr_len(path))
}

fn json_arr_pop(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_arr_trim(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_obj_keys(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_obj_len(ctx: &Context, args: Vec<String>) -> RedisResult {
    json_len(ctx, args, |doc, path| doc.obj_len(path))
}

fn json_debug(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_forget(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_resp(_ctx: &Context, _args: Vec<String>) -> RedisResult {
    Err("Command was not implemented".into())
}

fn json_len<F: Fn(&RedisJSON, &String) -> Result<usize, Error>>(
    ctx: &Context,
    args: Vec<String>,
    fun: F,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = args.next_string()?;

    let key = ctx.open_key_writable(&key);

    let length = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => fun(&doc, &path)?.into(),
        None => ().into(),
    };

    Ok(length)
}

//////////////////////////////////////////////////////

redis_module! {
    name: "redisjson",
    version: 1,
    data_types: [
        REDIS_JSON_TYPE,
    ],
    commands: [
        ["json.del", json_del, "write"],
        ["json.get", json_get, ""],
        ["json.mget", json_mget, ""],
        ["json.set", json_set, "write"],
        ["json.type", json_type, ""],
        ["json.numincrby", json_num_incrby, ""],
        ["json.nummultby", json_num_multby, ""],
        ["json.numpowby", json_num_powby, ""],
        ["json.strappend", json_str_append, ""],
        ["json.strlen", json_str_len, ""],
        ["json.arrappend", json_arr_append, ""],
        ["json.arrindex", json_arr_index, ""],
        ["json.arrinsert", json_arr_insert, ""],
        ["json.arrlen", json_arr_len, ""],
        ["json.arrpop", json_arr_pop, ""],
        ["json.arrtrim", json_arr_trim, ""],
        ["json.objkeys", json_obj_keys, ""],
        ["json.objlen", json_obj_len, ""],
        ["json.debug", json_debug, ""],
        ["json.forget", json_forget, ""],
        ["json.resp", json_resp, ""],
    ],
}
