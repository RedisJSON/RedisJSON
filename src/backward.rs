use redismodule::raw;
use serde_json::map::Map;
use serde_json::Number;
use serde_json::Value;
use std::vec::Vec;

#[derive(Debug, PartialEq)]
enum NodeType {
    Null, // used in masks and consistent type checking
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

pub unsafe fn json_rdb_load(rdb: *mut raw::RedisModuleIO) -> Value {
    let node_type = raw::load_unsigned(rdb).into();
    match node_type {
        NodeType::Null => Value::Null,
        NodeType::Boolean => {
            let mut str_len = 0;
            let str_buffer = raw::load_string_buffer(rdb, &mut str_len);
            Value::Bool(str_buffer.starts_with("1"))
        }
        NodeType::Integer => Value::Number(raw::load_signed(rdb).into()),
        NodeType::Number => Value::Number(Number::from_f64(raw::load_double(rdb)).unwrap()),
        NodeType::String => {
            let mut str_len = 0;
            let str_buffer = raw::load_string_buffer(rdb, &mut str_len);
            Value::String(str_buffer.to_string())
        }
        NodeType::Dict => {
            let len = raw::load_unsigned(rdb);
            let mut m = Map::with_capacity(len as usize);
            for _ in 0..len {
                let t: NodeType = raw::load_unsigned(rdb).into();
                if t != NodeType::KeyVal {
                    panic!("Can't load old RedisJSON RDB");
                }
                let mut str_len = 0;
                let name = &raw::load_string_buffer(rdb, &mut str_len);
                m.insert(name.to_string(), json_rdb_load(rdb));
            }
            Value::Object(m)
        }
        NodeType::Array => {
            let len = raw::load_unsigned(rdb);
            let mut v = Vec::with_capacity(len as usize);
            for _ in 0..len {
                v.push(json_rdb_load(rdb))
            }
            Value::Array(v)
        }
        NodeType::KeyVal => panic!("Can't load old RedisJSON RDB"),
    }
}
