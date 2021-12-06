use std::vec::Vec;

use redis_module::raw;
use serde_json::map::Map;
use ijson::{IValue, INumber};


use crate::error::Error;

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
            0x1u64 => NodeType::Null,
            0x2u64 => NodeType::String,
            0x4u64 => NodeType::Number,
            0x8u64 => NodeType::Integer,
            0x10u64 => NodeType::Boolean,
            0x20u64 => NodeType::Dict,
            0x40u64 => NodeType::Array,
            0x80u64 => NodeType::KeyVal,
            _ => panic!("Can't load old RedisJSON RDB1"),
        }
    }
}

pub fn json_rdb_load(rdb: *mut raw::RedisModuleIO) -> Result<IValue, Error> {
    // let node_type = raw::load_unsigned(rdb)?.into();
    // match node_type {
    //     NodeType::Null => Ok(IValue::NULL),
    //     NodeType::Boolean => {
    //         let buffer = raw::load_string_buffer(rdb)?;
    //         Ok(IValue::Bool(buffer.as_ref()[0] == b'1'))
    //     }
    //     NodeType::Integer => {
    //         let n = raw::load_signed(rdb)?;
    //         Ok(IValue::Number(n.into()))
    //     }
    //     NodeType::Number => {
    //         let n = raw::load_double(rdb)?;
    //         Ok(IValue::Number(
    //             Number::from_f64(n).ok_or_else(|| Error::from("Can't load as float"))?,
    //         ))
    //     }
    //     NodeType::String => {
    //         let buffer = raw::load_string_buffer(rdb)?;
    //         Ok(IValue::String(buffer.to_string()?))
    //     }
    //     NodeType::Dict => {
    //         let len = raw::load_unsigned(rdb)?;
    //         let mut m = Map::with_capacity(len as usize);
    //         for _ in 0..len {
    //             let t: NodeType = raw::load_unsigned(rdb)?.into();
    //             if t != NodeType::KeyVal {
    //                 return Err(Error::from("Can't load old RedisJSON RDB"));
    //             }
    //             let buffer = raw::load_string_buffer(rdb)?;
    //             m.insert(buffer.to_string()?, json_rdb_load(rdb)?);
    //         }
    //         Ok(IValue::Object(m))
    //     }
    //     NodeType::Array => {
    //         let len = raw::load_unsigned(rdb)?;
    //         let mut v = Vec::with_capacity(len as usize);
    //         for _ in 0..len {
    //             let nested = json_rdb_load(rdb)?;
    //             v.push(nested);
    //         }
    //         Ok(IValue::Array(v))
    //     }
    //     NodeType::KeyVal => Err(Error::from("Can't load old RedisJSON RDB")),
    // }
    Ok(IValue::NULL)
}
