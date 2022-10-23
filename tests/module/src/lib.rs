#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

extern crate redis_module;

use redis_module::*;
use std::str;
use std::f64::EPSILON;
use std::ffi::{CStr, c_void};
use cstr::cstr;
use function_name::named;

const MODULE_NAME: &str = "RJ_LLAPI";
const MODULE_VERSION: u32 = 1;

struct RjApi {
	japi: *const RedisJSONAPI,
	version: i32,
}

impl RjApi {
	pub unsafe fn api(&self) -> &RedisJSONAPI {
		&*self.japi
	}
}

static mut rj_api: RjApi = RjApi {
	japi: std::ptr::null::<RedisJSONAPI>(),
	version: 0
};

fn get_json_apis(
	ctx: *mut RedisModuleCtx,
	subscribe_to_module_change: bool
) -> Status {
	let mut japi: *const std::os::raw::c_void;

	japi = unsafe { RedisModule_GetSharedAPI.unwrap()(ctx, cstr!("RedisJSON_V2").as_ptr()) };
	if !japi.is_null() {
		unsafe {
			rj_api.japi = japi as *const RedisJSONAPI;
			rj_api.version = 2;
		}
		return Status::Ok;
	}
	
	japi = unsafe { RedisModule_GetSharedAPI.unwrap()(ctx, cstr!("RedisJSON_V1").as_ptr()) };
	if !japi.is_null() {
		unsafe {
			rj_api.japi = japi as *const RedisJSONAPI;
			rj_api.version = 1;
		}
		return Status::Ok;
	}

	if subscribe_to_module_change {
		return unsafe { subscribe_to_server_event(ctx, redis_module::RedisModuleEvent_ModuleChange, Some(module_change_handler)) };
	}

	Status::Ok
}

unsafe extern "C" fn module_change_handler(
	ctx: *mut RedisModuleCtx,
	_event: RedisModuleEvent,
	sub: u64,
	ei: *mut c_void
) {
	let ei = &*(ei as *mut RedisModuleModuleChange);
	if sub == REDISMODULE_SUBEVENT_MODULE_LOADED.into() &&            // If the subscribed event is a module load,
		rj_api.japi.is_null() &&                                        // and JSON is not already loaded,
		CStr::from_ptr(ei.module_name).to_str().unwrap() == "ReJSON" && // and the loading module is JSON:
		get_json_apis(ctx, false) == Status::Err                        // try to load it.
	{
			// Log Error
	}
}

fn init(ctx: &Context, _args: &[RedisString]) -> Status {
	get_json_apis(ctx.ctx, true);
	Status::Ok
}

#[named]
fn RJ_llapi_test_open_key(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	assert!(ctx.call("JSON.SET", &[function_name!(), "$", "0"]).is_ok());
	let rmk = key::RedisKey::open(ctx.ctx, &keyname);
	assert_eq!(unsafe { rj_api.api().isJSON.unwrap()(rmk.key_inner) }, 1);
	assert!(unsafe { !(rj_api.api().openKey.unwrap()(ctx.ctx, keyname.inner).is_null()) });

	ctx.call("SET", &[function_name!(), "0"]).unwrap();
	let rmk = key::RedisKey::open(ctx.ctx, &keyname);
	assert_ne!(unsafe { rj_api.api().isJSON.unwrap()(rmk.key_inner) }, 1);
	assert!(unsafe { rj_api.api().openKey.unwrap()(ctx.ctx, keyname.inner).is_null() });

	Ok(RedisValue::SimpleStringStatic("PASS"))
}

#[named]
fn RJ_llapi_test_iterator(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	let vals: [i64; 10] =  [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] ;
	let json            = "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]";
	ctx.call("JSON.SET", &[function_name!(), "$", json]).unwrap();

	let ji = unsafe { rj_api.api().get.unwrap()(rj_api.api().openKey.unwrap()(ctx.ctx, keyname.inner), cstr!("$..*").as_ptr()) };
	assert!(!ji.is_null());
	unsafe { if rj_api.version >= 2 {
		let mut s = RedisString::create(ctx.ctx, "");
		rj_api.api().getJSONFromIter.unwrap()(ji, ctx.ctx, &mut s.inner as *mut _);
		let s = CStr::from_ptr(string_ptr_len(s.inner, 0 as *mut _)).to_str().unwrap();
		assert_eq!(s, json);
	}}

	let len = unsafe { rj_api.api().len.unwrap()(ji) };
	assert_eq!(len, vals.len());
	let mut num = 0i64;
	for i in 0..len {
		let js = unsafe { rj_api.api().next.unwrap()(ji) };
		assert!(!js.is_null());
		unsafe { rj_api.api().getInt.unwrap()(js, &mut num as *mut _) };
		assert_eq!(num, vals[i]);
	}
	assert!(unsafe { rj_api.api().next.unwrap()(ji).is_null() });
	unsafe { rj_api.api().freeIter.unwrap()(ji) };

	Ok(RedisValue::SimpleStringStatic("PASS"))
}

#[named]
fn RJ_llapi_test_get_type(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	ctx.call("JSON.SET", &[function_name!(), "$", "[\"\", 0, 0.0, false, {}, [], null]"]).unwrap();
	let js = unsafe { rj_api.api().openKey.unwrap()(ctx.ctx, keyname.inner) };
	
	let mut len = 0usize;
	unsafe { rj_api.api().getLen.unwrap()(js, &mut len as *mut _) };
	assert_eq!(len, JSONType_JSONType__EOF as usize);

	for i in 0..len { unsafe { 
		let elem = rj_api.api().getAt.unwrap()(js, i);
		let jtype = rj_api.api().getType.unwrap()(elem);
		assert_eq!(jtype, i as u32);
	}}

	Ok(RedisValue::SimpleStringStatic("PASS"))
}

#[named]
fn RJ_llapi_test_get_value(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	ctx.call("JSON.SET", &[function_name!(), "$", "[\"a\", 1, 0.1, true, {\"_\":1}, [1], null]"]).unwrap();
	let js = unsafe { rj_api.api().openKey.unwrap()(ctx.ctx, keyname.inner) };

	let s = cstr!("");
	let mut len = 0usize;
	unsafe { rj_api.api().getString.unwrap()(rj_api.api().getAt.unwrap()(js, 0), &mut (s.as_ptr()) as *mut _, &mut len as *mut _) };
	assert_eq!(s.to_str().unwrap(), "a");

	let mut ll = 0i64;
	unsafe { rj_api.api().getInt.unwrap()(rj_api.api().getAt.unwrap()(js, 1), &mut ll as *mut _) };
	assert_eq!(ll, 1);

	let mut dbl = 0f64;
	unsafe { rj_api.api().getDouble.unwrap()(rj_api.api().getAt.unwrap()(js, 2), &mut dbl as *mut _) };
	assert!((dbl - 0.1).abs() < EPSILON);

	let mut b = 0i32;
	unsafe { rj_api.api().getBoolean.unwrap()(rj_api.api().getAt.unwrap()(js, 3), &mut b as *mut _) };
	assert_eq!(b, 1);

	len = 0;
	unsafe { rj_api.api().getLen.unwrap()(rj_api.api().getAt.unwrap()(js, 4), &mut len as *mut _) };
	assert_eq!(len, 1);

	len = 0;
	unsafe { rj_api.api().getLen.unwrap()(rj_api.api().getAt.unwrap()(js, 5), &mut len as *mut _) };
	assert_eq!(len, 1);

	Ok(RedisValue::SimpleStringStatic("PASS"))
}


const fn split(cmd: &str) -> (&str, &str) {
	use konst::option::unwrap;
	use konst::slice::get_range;
	use konst::slice::get_from;

	const i: usize = MODULE_NAME.len();
	let cmd = cmd.as_bytes();
	let (hd, tl) = (
		unwrap!(get_range(cmd, 0, i)),
		unwrap!(get_from(cmd, i + "_".len())),
	);
	unsafe {(
		core::str::from_utf8_unchecked(hd),
		core::str::from_utf8_unchecked(tl),
	)}
}

macro_rules! my_module {
	($( $cmd:expr, )*) => {
		redis_module! (
			name: MODULE_NAME,
			version: MODULE_VERSION,
			data_types: [],
			init: init,
			commands: [
				$(
					[{
						const SPLIT: (&str, &str) = split(stringify!($cmd));
						const_format::concatcp!(SPLIT.0, ".", SPLIT.1)
					}, $cmd, "", 0, 0, 0],
				)*
			]
		);
	}
}

my_module! {
	RJ_llapi_test_open_key,
	RJ_llapi_test_iterator,
	RJ_llapi_test_get_type,
	RJ_llapi_test_get_value,
}
