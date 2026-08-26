#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

extern crate redis_module;

use redis_module::*;
use rejson_api::*;
use std::{str, slice};
use std::f64::EPSILON;
use std::ffi::{CStr, c_char};
use function_name::named;

pub mod rejson_api;

const MODULE_NAME: &str = "RJ_LLAPI";
const MODULE_VERSION: u32 = 1;

const OK: RedisResult = Ok(RedisValue::SimpleStringStatic("Ok"));

fn init(ctx: &Context, _args: &[RedisString]) -> Status {
	RjApi::get_json_apis(ctx.ctx, true);
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
	assert!(RjApi::isJSON(rmk));
	assert!(!(RjRedisJSON::openKey(ctx, &keyname).is_null()));

	ctx.call("SET", &[function_name!(), "0"]).unwrap();
	let rmk = key::RedisKey::open(ctx.ctx, &keyname);
	assert!(!RjApi::isJSON(rmk));
	assert!(RjRedisJSON::openKey(ctx, &keyname).is_null());

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

	let mut ji = RjRedisJSON::openKey(ctx, &keyname).iter("$..*");
	assert!(!ji.is_null());
	if RjApi::get_version() >= 2 {
		let mut s = RedisString::create(ctx.ctx, "");
		ji.getJSON(ctx, &mut s);
		let s = unsafe { CStr::from_ptr(string_ptr_len(s.inner, 0 as *mut _)).to_str().unwrap() };
		assert_eq!(s, json);
	}

	let len = ji.len();
	assert_eq!(len, vals.len());
	let mut num = 0i64;
	for i in 0..len {
		let js = ji.next();
		assert!(js.is_some());
		js.unwrap().getInt(&mut num);
		assert_eq!(num, vals[i]);
	}
	assert!(ji.next().is_none());

	if RjApi::get_version() >= 2 {
		ji.reset();
		for i in 0..len {
			let js = ji.next();
			assert!(js.is_some());
			js.unwrap().getInt(&mut num);
			assert_eq!(num, vals[i]);
		}
		assert!(ji.next().is_none());
	}

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
	let js = RjRedisJSON::openKey(ctx, &keyname);
	
	let mut len = 0;
	js.getLen(&mut len);
	assert_eq!(len, JSONType_JSONType__EOF as usize);

	for i in 0..len {
		assert_eq!(js.getAt(i).getType(), i as u32);
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
	let js = RjRedisJSON::openKey(ctx, &keyname);

	let mut s: *const c_char = std::ptr::null::<c_char>();
	let mut len = 0;
	js.getAt(0).getString(&mut s, &mut len);
	assert_eq!(unsafe { str::from_utf8_unchecked(slice::from_raw_parts(s as *const _, len)) }, "a");

	let mut ll = 0;
	js.getAt(1).getInt(&mut ll);
	assert_eq!(ll, 1);

	let mut dbl = 0.;
	js.getAt(2).getDouble(&mut dbl);
	assert!((dbl - 0.1).abs() < EPSILON);

	let mut b = 0;
	js.getAt(3).getBoolean(&mut b);
	assert_eq!(b, 1);

	len = 0;
	js.getAt(4).getLen(&mut len);
	assert_eq!(len, 1);

	len = 0;
	js.getAt(5).getLen(&mut len);
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
