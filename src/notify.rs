//#[macro_use]
extern crate lazy_static;

use lazy_static::lazy_static;
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

static REGISTER_CALLBACK_NAME: &str = concat!("json_register_callback", "\0");
static GET_PATH_NAME: &str = concat!("json_get_path", "\0");

// use bitflags::bitflags;
// bitflags! {
//     pub struct NotificationEvent1: c_int {
//         const SET_KEY_VAL = 0 as c_int;
//     }
// }

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum NotificationEvent {
    SetKeyValue = 0,
}

pub enum NotificationPoint {
    Pre(bool), //subscriber can abort by returning false
    PreAndPost(bool), //subscriber can abort by returning false
    Post,
}

struct Subscriber {
    callback: *mut ::std::os::raw::c_void
}

impl PartialEq for Subscriber {
    fn eq(&self, other: &Self) -> bool {
        self.callback == other.callback
    }

    fn ne(&self, other: &Self) -> bool {
        self.callback != other.callback
    }
}


pub struct Publisher {
    subscribers : HashMap<NotificationEvent, HashMap<*mut ::std::os::raw::c_void, Subscriber>>,
} 

impl Publisher {
    fn subscribe(mut self, callback: *mut ::std::os::raw::c_void, event: NotificationEvent) {
        let subs = self.subscribers.entry(event).or_insert(HashMap::new());
        let sub = subs.entry(callback).or_insert(Subscriber{callback});
        //TODO: If more Subscriber data exists, update it (pre, post, allow abort)
    }

    fn get_callback(event: NotificationEvent) -> Vec<*mut ::std::os::raw::c_void> {
        let mut result: Vec<*mut ::std::os::raw::c_void> = Vec::new();
        let sub = publisher.subscribers.get(&event);
        match sub {
            Some(sub) => {
                for (_, value) in sub {
                    result.push(value.callback);
                }
            }
            None => {}
        }
        result
    }
}

lazy_static! {
    //static ref publisher : Publisher = Publisher {subscribers: HashMap::new()};
    static ref publisher: HashMap<NotificationEvent, *mut ::std::os::raw::c_void> = HashMap::new();
}

unsafe impl Send for Publisher {}

pub fn notifyKeySet(key: &RedisJSON) {
    //let callbacks : Vec<*mut ::std::os::raw::c_void> = publisher.get
}

pub fn export_shared_api(ctx: &Context) {
    unsafe {
        ctx.export_shared_api(register_callback as *mut c_void, REGISTER_CALLBACK_NAME.as_ptr() as *mut i8);
        ctx.export_shared_api(get_path as *mut c_void, GET_PATH_NAME.as_ptr() as *mut i8);
    }
    // TODO: check return values
    ctx.log(
        LogLevel::Debug,
        &format!(
            "Exported APIs {:?}, {:?}", REGISTER_CALLBACK_NAME, GET_PATH_NAME
        ),
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
    0
    // TODO:
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