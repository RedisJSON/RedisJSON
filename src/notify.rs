//#[macro_use]

use redis_module::{raw as rawmod};
use redis_module::{Context, RedisError, RedisResult, RedisValue, REDIS_OK, LogLevel};

// use std::{i64, usize};
// use crate::error::Error;

use std::vec::Vec;
use std::collections::HashMap;

//use crate::redisjson::{RedisJSON};
use ::std::os::raw::c_void;
use ::std::os::raw::c_int;

use crate::redisjson::RedisJSON;

//static REGISTER_CALLBACK_NAME: &str = concat!("register_callback", "\0");
static GET_PATH_NAME: &str = concat!("RediSearch_GetJSONPathString", "\0");

pub fn export_shared_api(ctx: &Context) {
    unsafe {
        ctx.export_shared_api(get_path as *mut c_void, GET_PATH_NAME.as_ptr() as *mut i8);
        //ctx.export_shared_api(register_callback as *mut c_void, REGISTER_CALLBACK_NAME.as_ptr() as *mut i8);
    }
    // TODO: check return values
    ctx.log(
        LogLevel::Debug,
        &format!("Exported APIs {:?}", GET_PATH_NAME)
    );
}

//
// Externally public functions
//
pub extern "C" fn register_callback(
    ctx: *mut rawmod::RedisModuleCtx,
    _argv: *mut *mut rawmod::RedisModuleString,
    _argc: c_int,
) -> c_int {
    
    let a = String::from("kawabanga");
    let f = a.find("ban");
        
    match f {
        Some(i) => {
            i as c_int
        },
        _ => 0
    }
}


pub extern "C" fn get_path(
    ctx: *mut rawmod::RedisModuleCtx,
    //...,
) -> RedisResult {
    // TODO:
    // Need to understand how to return a path, which is dynamic (on the heap)
    // And move ownership to the caller
    // Maybe get a redis key
    REDIS_OK
}