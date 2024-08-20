/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use itertools::Itertools;
use redis_module::raw;
use serde_json::Number;
use serde_json::Value;

use crate::error::Error;
use crate::redisjson::ResultInto;

#[derive(Debug, PartialEq)]
enum NodeType {
    Null,
    // used in masks and consistent type checking
    String,
    Number,
    Integer,
    Boolean,
    Dict,
    Array,
    KeyVal,
    // N_DATETIME = 0x100
    // N_BINARY = 0x200
}

impl From<u64> for NodeType {
    fn from(n: u64) -> Self {
        match n {
            0x1u64 => Self::Null,
            0x2u64 => Self::String,
            0x4u64 => Self::Number,
            0x8u64 => Self::Integer,
            0x10u64 => Self::Boolean,
            0x20u64 => Self::Dict,
            0x40u64 => Self::Array,
            0x80u64 => Self::KeyVal,
            _ => panic!("Can't load old RedisJSON RDB1"),
        }
    }
}

pub fn json_rdb_load(rdb: *mut raw::RedisModuleIO) -> Result<Value, Error> {
    let node_type = raw::load_unsigned(rdb)?.into();
    match node_type {
        NodeType::Null => Ok(Value::Null),
        NodeType::Boolean => {
            let buffer = raw::load_string_buffer(rdb)?;
            Ok(Value::Bool(buffer.as_ref()[0] == b'1'))
        }
        NodeType::Integer => {
            let n = raw::load_signed(rdb)?;
            Ok(n.into())
        }
        NodeType::Number => {
            let n = raw::load_double(rdb)?;
            Number::from_f64(n)
                .map(Into::into)
                .ok_or_else(|| Error::from("Can't load as float"))
        }
        NodeType::String => {
            let buffer = raw::load_string_buffer(rdb)?;
            buffer.to_string().into_both()
        }
        NodeType::Dict => {
            let len = raw::load_unsigned(rdb)?;
            (0..len)
                .map(|_| match raw::load_unsigned(rdb)?.into() {
                    NodeType::KeyVal => {
                        let buffer = raw::load_string_buffer(rdb)?;
                        Ok((buffer.to_string()?, json_rdb_load(rdb)?))
                    }
                    _ => Err(Error::from("Can't load old RedisJSON RDB")),
                })
                .try_collect()
                .map(Value::Object)
        }
        NodeType::Array => {
            let len = raw::load_unsigned(rdb)?;
            (0..len)
                .map(|_| json_rdb_load(rdb))
                .try_collect()
                .map(Value::Array)
        }
        NodeType::KeyVal => Err(Error::from("Can't load old RedisJSON RDB")),
    }
}
