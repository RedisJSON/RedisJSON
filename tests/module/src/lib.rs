#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

extern crate redis_module;

use redis_module::*;
use rejson_api::*;
use std::{str, slice};
use std::f64::EPSILON;
use std::ffi::{CStr, c_char, c_void};
use function_name::named;

pub mod rejson_api;

const MODULE_NAME: &str = "RJ_LLAPI";
const MODULE_VERSION: u32 = 1;

const OK: RedisResult = Ok(RedisValue::SimpleStringStatic("Ok"));

static mut REDIS_JSON_API: RjApi = RjApi::new();

fn rj_api() -> &'static RjApi {
	unsafe { &REDIS_JSON_API }
}

fn init(ctx: &Context, _args: &[RedisString]) -> Status {
	unsafe { REDIS_JSON_API.get_json_apis(ctx.ctx, true) };
	Status::Ok
}

unsafe extern "C" fn module_change_handler(
	ctx: *mut RedisModuleCtx,
	_event: RedisModuleEvent,
	sub: u64,
	ei: *mut c_void
) {
	let ei = &*(ei as *mut RedisModuleModuleChange);
	if sub == REDISMODULE_SUBEVENT_MODULE_LOADED as u64 &&         // If the subscribed event is a module load,
		!rj_api().is_loaded() &&                                     // and JSON is not already loaded,
		CStr::from_ptr(ei.module_name).to_str().unwrap() == "ReJSON" // and the loading module is JSON:
	{
		REDIS_JSON_API.get_json_apis(ctx, false);                    // try to load it.
	}
}

#[named]
fn RJ_llapi_test_open_key(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	assert!(ctx.call("JSON.SET", &[function_name!(), "$", "0"]).is_ok());
	let rmk = key::RedisKey::open(ctx.ctx, &keyname);
	assert!(rj_api().isJSON(rmk));
	assert!(!(rj_api().openKey(ctx, &keyname).is_null()));

	ctx.call("SET", &[function_name!(), "0"]).unwrap();
	let rmk = key::RedisKey::open(ctx.ctx, &keyname);
	assert!(!rj_api().isJSON(rmk));
	assert!(rj_api().openKey(ctx, &keyname).is_null());

	ctx.reply_simple_string(concat!(function_name!(), ": PASSED"));
	OK
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

	let ji = rj_api().get(&rj_api().openKey(ctx, &keyname), "$..*");
	assert!(!ji.is_null());
	if rj_api().get_version() >= 2 {
		let mut s = RedisString::create(ctx.ctx, "");
		rj_api().getJSONFromIter(&ji, ctx, &mut s);
		let s = unsafe { CStr::from_ptr(string_ptr_len(s.inner, 0 as *mut _)).to_str().unwrap() };
		assert_eq!(s, json);
	}

	let len = rj_api().len(&ji);
	assert_eq!(len, vals.len());
	let mut num = 0i64;
	for i in 0..len {
		let js = rj_api().next(&ji);
		assert!(!js.is_null());
		rj_api().getInt(&js, &mut num);
		assert_eq!(num, vals[i]);
	}
	assert!(rj_api().next(&ji).is_null());

	if rj_api().get_version() >= 2 {
		rj_api().resetIter(&ji);
		for i in 0..len {
			let js = rj_api().next(&ji);
			assert!(!js.is_null());
			rj_api().getInt(&js, &mut num);
			assert_eq!(num, vals[i]);
		}
		assert!(rj_api().next(&ji).is_null());
	}

	rj_api().freeIter(&ji);

	ctx.reply_simple_string(concat!(function_name!(), ": PASSED"));
	OK
}

#[named]
fn RJ_llapi_test_get_type(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	ctx.call("JSON.SET", &[function_name!(), "$", "[\"\", 0, 0.0, false, {}, [], null]"]).unwrap();
	let js = rj_api().openKey(ctx, &keyname);
	
	let mut len = 0;
	rj_api().getLen(&js, &mut len);
	assert_eq!(len, JSONType_JSONType__EOF as usize);

	for i in 0..len { 
		let elem = rj_api().getAt(&js, i);
		let jtype = rj_api().getType(&elem);
		assert_eq!(jtype, i as u32);
	}

	ctx.reply_simple_string(concat!(function_name!(), ": PASSED"));
	OK
}

#[named]
fn RJ_llapi_test_get_value(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
	if args.len() != 1 {
		return Err(RedisError::WrongArity);
	}

	let keyname = RedisString::create(ctx.ctx, function_name!());

	ctx.call("JSON.SET", &[function_name!(), "$", "[\"a\", 1, 0.1, true, {\"_\":1}, [1], null]"]).unwrap();
	let js = rj_api().openKey(ctx, &keyname);

	let mut s: *const c_char = std::ptr::null::<c_char>();
	let mut len = 0;
	rj_api().getString(&rj_api().getAt(&js, 0), &mut s, &mut len);
	assert_eq!(unsafe { str::from_utf8_unchecked(slice::from_raw_parts(s as *const _, len)) }, "a");

	let mut ll = 0;
	rj_api().getInt(&rj_api().getAt(&js, 1), &mut ll);
	assert_eq!(ll, 1);

	let mut dbl = 0.;
	rj_api().getDouble(&rj_api().getAt(&js, 2), &mut dbl);
	assert!((dbl - 0.1).abs() < EPSILON);

	let mut b = 0;
	rj_api().getBoolean(&rj_api().getAt(&js, 3), &mut b);
	assert_eq!(b, 1);

	len = 0;
	rj_api().getLen(&rj_api().getAt(&js, 4), &mut len);
	assert_eq!(len, 1);

	len = 0;
	rj_api().getLen(&rj_api().getAt(&js, 5), &mut len);
	assert_eq!(len, 1);

	ctx.reply_simple_string(concat!(function_name!(), ": PASSED"));
	OK
}

fn RJ_llapi_test_all(ctx: &Context, _args: Vec<RedisString>) -> RedisResult {
	ctx.call("FLUSHALL", &[]).unwrap();
	const NUM_TESTS: usize = 4;
	let tests = [
		"RJ_LLAPI.test_open_key", 
		"RJ_LLAPI.test_iterator",
		"RJ_LLAPI.test_get_type",
		"RJ_LLAPI.test_get_value"
	];
	let mut passed = 0usize;
	reply_with_array(ctx.ctx, 2);

	reply_with_array(ctx.ctx, NUM_TESTS as _);
	for i in 0..NUM_TESTS {
		let r = ctx.call(&tests[i], &[]);
		passed += (ctx.reply(r) == Status::Ok) as usize;
	}

	assert_eq!(passed, NUM_TESTS);
	ctx.call("FLUSHALL", &[]).unwrap();
	OK
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
	RJ_llapi_test_all,
}
