#[macro_use]
extern crate redismodule;

use redismodule::{Context, RedisResult, NextArg, REDIS_OK};
use redismodule::native_types::RedisType;

mod redisjson;

use crate::redisjson::RedisJSON;

static REDIS_JSON_TYPE: RedisType = RedisType::new("RedisJSON");

#[derive(Debug, PartialEq)]
pub enum SetOptions {
    NotExists,
    AlreadyExists,
    None
}

fn json_set(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let _path = args.next_string()?; // TODO handle this path
    let value = args.next_string()?;
    let option = match args.next() {
        Some(op) => {
            match op.as_str() {
                "NX" => SetOptions::NotExists,
                "XX" => SetOptions::AlreadyExists,
                _ => return Err("ERR syntax error".into())
            }
        }
        None => {
            SetOptions::None
        }
    };

    let key = ctx.open_key_writable(&key);

    match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
        Some(ref mut doc) if option != SetOptions::NotExists  => {
            doc.set_value(&value)?;
            REDIS_OK
        }
        None if option != SetOptions::AlreadyExists => {
            let doc = RedisJSON::from_str(&value)?;
            key.set_value(&REDIS_JSON_TYPE, doc)?;
            REDIS_OK
        }
        _ => Ok(().into())
    }
}

fn json_get(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;

    let mut path = loop {
        let arg = match args.next_string() {
            Ok(s) => s.to_uppercase(),
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
        ["json.get", json_get, ""],
        ["json.strlen", json_strlen, ""],
        ["json.type", json_type, ""],
    ],
}
