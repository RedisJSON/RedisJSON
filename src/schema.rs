use std::os::raw::{c_int, c_void};

use redisearch_api::Index;
use redismodule::raw;

use crate::error::Error;
use std::collections::HashMap;

////////////////////////////////////////////
pub struct Schema {
    pub index: Index,
    pub fields: HashMap<String, String>,
    pub name: String,
}

impl Schema {
    pub fn new(name: &str) -> Schema {
        Schema {
            index: Index::create(name),
            fields: HashMap::new(),
            name: name.to_owned(),
        }
    }

    pub fn from_str(data: &str) -> Result<Self, Error> {
        Ok(Self {
            // TODO better handle RDB read
            index: Index::create(data.to_string().as_str()),
            fields: HashMap::new(),
            // TODO load count from RDB
            name: String::new(),
        })
    }
}

////////////////////////////////////////////

pub mod type_methods {
    use super::*;

    // TODO: Instead of using a custom type, use Redis Module auxiliary data to save and load.
    //
    // Note that aux data is currently supported only in Redis Enterprise and in unstable Redis
    // (likely to be in 6.0).
    //
    // When implementing, make sure not to unwrap() the relevant methods but rather to check
    // if they are None, and in that case skip the persistence and keep the data only in memory.

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
        if encver < 2 {
            panic!("Can't load old RedisJSON schema RDB"); // TODO add support for backward
        }
        let json = Schema::from_str(&raw::load_string(rdb)).unwrap();
        Box::into_raw(Box::new(json)) as *mut c_void
    }

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn free(value: *mut c_void) {
        Box::from_raw(value as *mut Schema);
    }

    #[allow(non_snake_case, unused)]
    pub unsafe extern "C" fn rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
        let schema = &*(value as *mut Schema);
        // TODO implement RDB write
        // raw::save_string(rdb, &schema.schema.to_string());
    }
}
