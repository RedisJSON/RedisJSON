use lazy_static::lazy_static;
use std::{os::raw::{c_int, c_void, c_char}, sync::{Arc, Mutex}};
//use std::{ops::{Index, IndexMut};
use std::vec::Vec;
use std::option::Option;
use redis_module::{raw as rawmod};
use redis_module::{Context, LogLevel};

//
// callbacks
//
type CallbackKey    = extern fn(*mut rawmod::RedisModuleKey) -> c_int; 
type CallbackEvent  = extern fn(c_int) -> c_int;
type CallbackString = extern fn(*const c_char) -> c_int;
type CallbackRaw = *const c_void;

pub type RedisModuleAPICallbackKeyChange = Option<
    CallbackKey
>;

pub type RedisModuleAPICallbackEvent = Option<
    CallbackEvent
>;

pub type RedisModuleAPICallbackString = Option<
    CallbackString
>;

//
// structs
//

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RedisModuleCtx {
    _unused: [u8; 0],
}

// #[repr(C)]
// #[derive(Debug, Copy, Clone)]
// pub struct RedisModuleKey {
//     _unused: [u8; 0],
// }

#[repr(C)]
#[derive(Debug, Clone)]
pub struct RedisModuleAPICtx {
    subs: [Vec<*const c_void>; 3],
}

unsafe impl Send for RedisModuleAPICtx {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RedisModuleAPI_V1 {
    pub free_string: Option<
        extern "C" fn(
            *mut c_char,
        ),
    >,
    pub register_callback_key_change: Option<
        extern "C" fn(
            RedisModuleAPICallbackKeyChange,
        ) -> c_int
    >,
    pub register_callback_event: Option<
        extern "C" fn(
            RedisModuleAPICallbackEvent,
        ) -> c_int
    >,
    /* pub register_callback_string: Option<
        extern "C" fn(
            RedisModuleAPICallbackString,
        ) -> c_int
    >, */
    pub get_json_path: Option<
        unsafe extern "C" fn(
            key: *mut rawmod::RedisModuleKey,
            path: *const c_char,
        ) -> c_int,
    >,
}

#[repr(C)]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum CallbackKind {
    KeyChange,
    Event,
    StringData,
}


lazy_static! {
    static ref API_CTX : Arc<Mutex<RedisModuleAPICtx>> = {
        let c = RedisModuleAPICtx {
            subs: [Vec::new(), Vec::new(), Vec::new()]
        };
        let m = Mutex::new(c);
        let a = Arc::new(m);
        a
    };
}

//
// APIs
//

#[no_mangle]
pub extern "C" fn free_string(
        string: *mut c_char
    ) {
    unsafe { Box::from_raw(string); }
}
    
#[no_mangle]
pub extern "C" fn register_callback_key_change (
        callback: RedisModuleAPICallbackKeyChange,
    ) -> c_int {
    register_callback(callback.unwrap() as CallbackRaw, CallbackKind::KeyChange)
}

#[no_mangle]
pub extern "C" fn register_callback_event (
        callback: RedisModuleAPICallbackEvent,
    ) -> c_int {
    register_callback(callback.unwrap() as CallbackRaw, CallbackKind::Event)
}

#[no_mangle]
pub extern "C" fn register_callback_string (
        callback: RedisModuleAPICallbackString,
    ) -> c_int {
    register_callback(callback.unwrap() as CallbackRaw, CallbackKind::StringData)
}

#[no_mangle]
pub extern "C" fn get_json_path(
        _key: *mut rawmod::RedisModuleKey,
        _path: *const c_char,
    ) -> c_int {
        // TODO:
        // FIXME:
    0
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern fn RedisJSON_GetApiV1(
        _module_ctx: *mut RedisModuleCtx)
    -> *mut RedisModuleAPI_V1 {
    Box::into_raw(Box::new(RedisModuleAPI_V1 {
        free_string: Some(free_string),
        register_callback_key_change: Some(register_callback_key_change),
        register_callback_event: Some(register_callback_event),
        //register_callback_string: Some(register_callback_string),
        get_json_path: Some(get_json_path),
    }))
}

#[no_mangle]
pub extern "C" fn trigger_callback_key (
        key: *mut rawmod::RedisModuleKey,
    ) -> c_int {
    let fire_callback = |cb: *const c_void| -> c_int {
        unsafe {
            let f : CallbackKey = std::mem::transmute(cb);
            f(key)
        }
    };
    trigger_callback(CallbackKind::KeyChange, fire_callback)
}

#[no_mangle]
pub extern "C" fn trigger_callback_event (
    event: c_int,
) -> c_int {
    let fire_callback = |cb: *const c_void| -> c_int {
        unsafe {
            let f : CallbackEvent = std::mem::transmute(cb);
            f(event)
        }
    };
    trigger_callback(CallbackKind::Event, fire_callback)
}

#[no_mangle]
pub extern "C" fn trigger_callback_string (
    string: *const c_char,
) -> c_int {
    let fire_callback = |cb: *const c_void| -> c_int {
        unsafe {
            let f : CallbackString = std::mem::transmute(cb);
            f(string)
        }
    };
    trigger_callback(CallbackKind::StringData, fire_callback)
}

//
// Implementation
//

extern "C" fn register_callback (
        callback: CallbackRaw,
        event: CallbackKind,
    ) -> c_int {
    let a  = API_CTX.clone();
    let mut l = a.lock().unwrap();
    let v: &mut Vec<*const c_void>;
    //TODO: use index_mut trait: API_CTX[event].push(callback)
    match event {
        CallbackKind::KeyChange => {v = &mut (*l).subs[0]}
        CallbackKind::Event => {v = &mut (*l).subs[1]}
        CallbackKind::StringData => {v = &mut (*l).subs[2]}
    }
    v.push(callback);
    0
}

// impl Index<CallbackKind> for API_CTX {
//     type Output = Vec<*const c_void>;

//     fn index(&self, event: CallbackKind) -> &Self::Output {
//         let a  = self.clone();
//         let l = a.lock().unwrap();
        
//         match event {
//             CallbackKind::KeyChange => &(*l).subs[0],
//             CallbackKind::Event => &(*l).subs[1],
//             CallbackKind::StringData => &(*l).subs[2],
//         }
//     }
// }

// impl IndexMut<CallbackKind> for API_CTX {
//     fn index_mut(&mut self, event: CallbackKind) -> &mut Self::Output {
//         let a  = self.clone();
//         let mut l = a.lock().unwrap();
//         match event {
//             CallbackKind::KeyChange => &mut (*l).subs[0],
//             CallbackKind::Event => &mut (*l).subs[1],
//             CallbackKind::StringData => &mut (*l).subs[2],
//         }
//     }
// }

impl std::fmt::Display for CallbackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s;
        match self {
            CallbackKind::KeyChange => { s = "KeyChange"; },
            CallbackKind::Event => { s = "Event"; },
            CallbackKind::StringData => { s = "StringData"; },
        }
        write!(f, "{}", s)
    }
}

fn trigger_callback<F>(
        callback_kind: CallbackKind,
        f: F,
    ) -> c_int
    where 
        F: Fn (*const c_void) -> c_int {
    let mut res: c_int = 1;
    println!("in triggerCallback {}", callback_kind);
    //TODO: use index trait: let callbacks = API_CTX[event]
    let a  = API_CTX.clone();
    let l = a.lock().unwrap();
    let callbacks: &Vec<*const c_void>;
    match callback_kind {
        CallbackKind::KeyChange => {callbacks = &(*l).subs[0]}
        CallbackKind::Event => {callbacks = &(*l).subs[1]}
        CallbackKind::StringData => {callbacks = &(*l).subs[2]}
    }
    println!("calling {} callbacks", callbacks.len());
    for cb in callbacks {
        // TODO: use map to return a vec with all callback results?
        let cur_res = f(*cb);
        println!("\tcallback returned {}", cur_res);
        if cur_res == 0 {
            res = 0;
        }
    }
    res
}

impl Drop for RedisModuleAPICtx {
    fn drop(&mut self) {
        println!("Dropping {:p}", self);
    }
}

static REDISJSON_GETAPI_V1: &str = concat!("RedisJSON_GetApiV1", "\0");

pub fn export_shared_api(ctx: &Context) {
    ctx.export_shared_api(
        RedisJSON_GetApiV1 as *const c_void,
        REDISJSON_GETAPI_V1.as_ptr() as *mut i8
    );
    ctx.log(
        LogLevel::Verbose,
        &format!(
            "Exported API {:?}", REDISJSON_GETAPI_V1
        ),
    );
}
