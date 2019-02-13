#[macro_use]
extern crate redismodule;

use redismodule::{Context, RedisResult, NextArg};
use redismodule::native_types::RedisType;

mod redisdoc;

use crate::redisdoc::RedisDoc;

static DOC_REDIS_TYPE: RedisType = RedisType::new("RedisDoc1");

fn doc_set(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let value = args.next_string()?;

    let key = ctx.open_key_writable(&key);

    match key.get_value::<RedisDoc>(&DOC_REDIS_TYPE)? {
        Some(doc) => {
            doc.set_value(&value)?;
        }
        None => {
            let doc = RedisDoc::from_str(&value)?;
            key.set_value(&DOC_REDIS_TYPE, doc)?;
        }
    }

    Ok(().into())
}

fn doc_get(ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;

    let key = ctx.open_key_writable(&key);

    let value = match key.get_value::<RedisDoc>(&DOC_REDIS_TYPE)? {
        Some(doc) => { doc.to_string()?.into() }
        None => ().into()
    };

    Ok(value)
}

//////////////////////////////////////////////////////

redis_module! {
    name: "redisdoc",
    version: 1,
    data_types: [
        DOC_REDIS_TYPE,
    ],
    commands: [
        ["doc.set", doc_set, "write"],
        ["doc.get", doc_get, ""],
    ],
}
