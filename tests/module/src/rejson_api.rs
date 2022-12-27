
use redis_module::*;
use redis_module::key::RedisKey;
use cstr::cstr;
use std::ffi::{CString, c_char};
use crate::module_change_handler;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

const RedisModuleEvent_ModuleChange: RedisModuleEvent = RedisModuleEvent {
	id: REDISMODULE_EVENT_MODULE_CHANGE,
	dataver: 1,
};

pub struct RjApi {
	japi: *const RedisJSONAPI,
	version: i32,
}

impl RjApi {
	unsafe fn api(&self) -> &RedisJSONAPI {
		&*self.japi
	}

	pub const fn new() -> Self {
		Self {
			japi: std::ptr::null::<RedisJSONAPI>(),
			version: 0
		}
	}

	pub fn get_json_apis(&mut self, ctx: *mut RedisModuleCtx, subscribe_to_module_change: bool) {
		if self.try_get_api(ctx, 2) { return; }
		
		if self.try_get_api(ctx, 1) { return; }

		if subscribe_to_module_change {
			subscribe_to_server_event(ctx, RedisModuleEvent_ModuleChange, Some(module_change_handler));
		}
	}

	fn try_get_api(&mut self, ctx: *mut RedisModuleCtx, version: i32) -> bool {
		let japi = GetSharedAPI(ctx, version);
		if !japi.is_null() {
			self.japi = japi as *const RedisJSONAPI;
			self.version = version;
			return true;
		}
		false
	}

	pub fn is_loaded(&self) -> bool {
		!self.japi.is_null()
	}
	pub fn get_version(&self) -> i32 {
		self.version
	}

	pub fn isJSON(&self, key: RedisKey) -> bool {
		unsafe { self.api().isJSON.unwrap()(key.key_inner) != 0 }
	}
	pub fn openKey(&self, ctx: &Context, keyname: &RedisString) -> RedisJSON {
		unsafe { self.api().openKey.unwrap()(ctx.ctx, keyname.inner) }
	}
	pub fn get(&self, json: &RedisJSON, path: &str) -> JSONResultsIterator {
		let cpath = CString::new(path).unwrap();
		unsafe { self.api().get.unwrap()(*json, cpath.as_ptr()) }
	}
  pub fn next(&self, iter: &JSONResultsIterator) -> RedisJSON {
		unsafe { self.api().next.unwrap()(*iter) }
	}
  pub fn len(&self, iter: &JSONResultsIterator) -> usize {
		unsafe { self.api().len.unwrap()(*iter) }
	}
	pub fn freeIter(&self, iter: &JSONResultsIterator) {
		unsafe { self.api().freeIter.unwrap()(*iter) }
	}
  pub fn getAt(&self, json: &RedisJSON, index: usize) -> RedisJSON {
		unsafe { self.api().getAt.unwrap()(*json, index) }
	}
  pub fn getLen(&self, json: &RedisJSON, count: &mut usize) -> i32 {
		unsafe { self.api().getLen.unwrap()(*json, count as *mut _) }
	}
  pub fn getType(&self, json: &RedisJSON) -> JSONType {
		unsafe { self.api().getType.unwrap()(*json) }
	}

  pub fn getInt(&self, json: &RedisJSON, integer: &mut i64) -> i32 {
		unsafe { self.api().getInt.unwrap()(*json, integer as *mut _) }
	}
  pub fn getDouble(&self, json: &RedisJSON, dbl: &mut f64) -> i32 {
		unsafe { self.api().getDouble.unwrap()(*json, dbl as *mut _) }
	}
  pub fn getBoolean(&self, json: &RedisJSON, boolean: &mut i32) -> i32 {
		unsafe { self.api().getBoolean.unwrap()(*json, boolean as *mut _) }
	}
  pub fn getString(&self, json: &RedisJSON, str: &mut *const c_char, len: &mut usize) -> i32 {
		unsafe { self.api().getString.unwrap()(*json, str as *mut _, len as *mut _) }
	}

	// V2
  pub fn getJSONFromIter(&self, iter: &JSONResultsIterator, ctx: &Context, str: &mut RedisString) -> i32 {
		unsafe { self.api().getJSONFromIter.unwrap()(*iter, ctx.ctx, &mut str.inner as *mut _) }
	}
  pub fn resetIter(&self, iter: &JSONResultsIterator) {
		unsafe { self.api().resetIter.unwrap()(*iter) };
	}
}

pub fn GetSharedAPI(
  ctx: *mut RedisModuleCtx,
  version: i32,
) -> *const RedisJSONAPI {
  match version {
    2 => unsafe { RedisModule_GetSharedAPI.unwrap()(ctx, cstr!("RedisJSON_V2").as_ptr()) as _ }
    1 => unsafe { RedisModule_GetSharedAPI.unwrap()(ctx, cstr!("RedisJSON_V1").as_ptr()) as _ }
    _ => panic!()
  }
}
