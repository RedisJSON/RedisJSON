use redismodule::raw;
use std::os::raw::{c_int, c_void};
use crate::error::Error;

#[derive(Debug)]
pub struct RedisJSONSchema {
    data: String
}

impl RedisJSONSchema {
    pub fn from_str(data: &str) -> Result<Self, Error> {
        // let value = RedisJSON::parse_str(data, format)?;
        Ok(Self {data: data.to_string()})
    }
}

#[allow(non_snake_case, unused)]
pub unsafe extern "C" fn json_schema_rdb_load(rdb: *mut raw::RedisModuleIO, encver: c_int) -> *mut c_void {
    if encver < 2 {
        panic!("Can't load old RedisJSONSchema RDB"); // TODO add support for backward
    }
    let json = RedisJSONSchema::from_str(&raw::load_string(rdb)).unwrap();
    Box::into_raw(Box::new(json)) as *mut c_void
}

#[allow(non_snake_case, unused)]
#[no_mangle]
pub unsafe extern "C" fn json_schema_free(value: *mut c_void) {
    Box::from_raw(value as *mut RedisJSONSchema);
}

#[allow(non_snake_case, unused)]
#[no_mangle]
pub unsafe extern "C" fn json_schema_rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
    let schema = &*(value as *mut RedisJSONSchema);
    raw::save_string(rdb, &schema.data.to_string());
}