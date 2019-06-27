#[macro_use]
extern crate redismodule;

use redismodule::{Context, RedisResult, NextArg, REDIS_OK, RedisError};
use redismodule::native_types::RedisType;

mod redisjson;

use crate::redisjson::RedisJSON;

static REDIS_JSON_TYPE: RedisType = RedisType::new("RedisJSON");

#[derive(Debug, PartialEq)]
pub enum SetOptions {
    NotExists,
    AlreadyExists,
}

fn json_del(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = args.next_string()?;

    let key = ctx.open_key_writable(&key);
    let deleted : usize = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.delete_path(&path)?,
        None => 0
    };
    Ok(deleted.into())
}

fn json_set(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = args.next_string()?;
    let value = args.next_string()?;

    let set_option = args.next()
        .map(|op| {
            match op.to_uppercase().as_str() {
                "NX" => Ok(SetOptions::NotExists),
                "XX" => Ok(SetOptions::AlreadyExists),
                _ => Err(RedisError::Str("ERR syntax error")),
            }
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
            Err(_) => "$".to_owned() // path is optional
        };

        match arg.as_str() {
            "INDENT" => args.next(), // TODO add support
            "NEWLINE" => args.next(), // TODO add support
            "SPACE" => args.next(), // TODO add support
            "NOESCAPE" => continue, // TODO add support
            "." => break String::from("$"), // backward compatibility suuport
            _ => break arg
        };
    };

    if path.starts_with(".") { // backward compatibility
        path.insert(0, '$');
    }

    let key = ctx.open_key_writable(&key);

    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.to_string(&path)?.into(),
        None => ().into()
    };

    Ok(value)
}

fn json_mget(ctx: &Context, args: Vec<String>) -> RedisResult {

    if args.len() < 2 {
        return Err(RedisError::WrongArity);
    }
    if let Some(path) = args.last() {
        let mut path = path.clone();
        if path.starts_with(".") { // backward compatibility
            path.insert(0, '$');
        }
        let keys_len = args.len()-2;
        let mut args_iter = args.into_iter().skip(1);
        let mut results: Vec<String> = Vec::with_capacity(keys_len);
        for _i in 0..keys_len {
            let key = args_iter.next_string()?;

            let redis_key = ctx.open_key_writable(&key);

            match redis_key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
                Some(doc) => {
                    let result = doc.to_string(&path)?;
                    results.push(result);
                },
                None => {}
            };

        }
        Ok(results.into())
    } else {
        Err(RedisError::WrongArity)
    }
}


fn json_strlen(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = args.next_string()?;

    let key = ctx.open_key_writable(&key);

    let length = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.str_len(&path)?.into(),
        None => ().into()
    };

    Ok(length)
}

fn json_type(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = args.next_string()?;

    let key = ctx.open_key_writable(&key);

    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(doc) => doc.get_type(&path)?.into(),
        None => ().into()
    };

    Ok(value)
}

//////////////////////////////////////////////////////

redis_module! {
    name: "redisjson",
    version: 1,
    data_types: [
        REDIS_JSON_TYPE,
    ],
    commands: [
        ["json.set", json_set, "write"],
        ["json.del", json_del, "write"],
        ["json.get", json_get, ""],
        ["json.mget", json_mget, ""],
        ["json.strlen", json_strlen, ""],
        ["json.type", json_type, ""],
    ],
}
