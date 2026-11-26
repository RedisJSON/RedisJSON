
use redis_module::*;
use redis_module::key::RedisKey;
use cstr::cstr;
use std::ffi::{CStr, CString, c_char, c_void};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

const RedisModuleEvent_ModuleChange: RedisModuleEvent = RedisModuleEvent {
	id: REDISMODULE_EVENT_MODULE_CHANGE,
	dataver: 1,
};


pub struct RjRedisJSON {
	inner: RedisJSON,
}
impl RjRedisJSON {
	pub fn openKey(ctx: &Context, keyname: &RedisString) -> Self {
		Self {
			inner: unsafe { rj_api().openKey.unwrap()(ctx.ctx, keyname.inner) }
		}
	}
  pub fn getAt(&self, index: usize) -> Self {
		Self {
			inner: unsafe { rj_api().getAt.unwrap()(self.inner, index) }
		}
	}
  pub fn getLen(&self, count: &mut usize) -> i32 {
		unsafe { rj_api().getLen.unwrap()(self.inner, count as *mut _) }
	}
  pub fn getType(&self) -> JSONType {
		unsafe { rj_api().getType.unwrap()(self.inner) }
	}

  pub fn getInt(&self, integer: &mut i64) -> i32 {
		unsafe { rj_api().getInt.unwrap()(self.inner, integer as *mut _) }
	}
  pub fn getDouble(&self, dbl: &mut f64) -> i32 {
		unsafe { rj_api().getDouble.unwrap()(self.inner, dbl as *mut _) }
	}
  pub fn getBoolean(&self, boolean: &mut i32) -> i32 {
		unsafe { rj_api().getBoolean.unwrap()(self.inner, boolean as *mut _) }
	}
  pub fn getString(&self, str: &mut *const c_char, len: &mut usize) -> i32 {
		unsafe { rj_api().getString.unwrap()(self.inner, str as *mut _, len as *mut _) }
	}

	pub fn is_null(&self) -> bool {
		self.inner.is_null()
	}

	pub fn iter(&self, path: &str) -> RjResults {
		let cpath = CString::new(path).unwrap();
		RjResults {
			inner: unsafe { rj_api().get.unwrap()(self.inner, cpath.as_ptr()) }
		}
	}
}

pub struct RjResults {
	inner: JSONResultsIterator,
}
impl Iterator for RjResults {
	type Item = RjRedisJSON;
  fn next(&mut self) -> Option<Self::Item> {
		let next = unsafe { rj_api().next.unwrap()(self.inner) };
		if next.is_null() { None }
		else { Some( Self::Item {
			inner: next
		})}
	}
}
impl RjResults {
  pub fn len(&self) -> usize {
		unsafe { rj_api().len.unwrap()(self.inner) }
	}
	pub fn drop(self) {
		unsafe { rj_api().freeIter.unwrap()(self.inner) }
	}

	pub fn is_null(&self) -> bool {
		self.inner.is_null()
	}

	// V2
  pub fn getJSON(&self, ctx: &Context, str: &mut RedisString) -> i32 {
		unsafe { rj_api().getJSONFromIter.unwrap()(self.inner, ctx.ctx, &mut str.inner as *mut _) }
	}
  pub fn reset(&self) {
		unsafe { rj_api().resetIter.unwrap()(self.inner) };
	}
}

pub struct RjApi {
	japi: *const RedisJSONAPI,
	version: i32,
}

impl RjApi {
	const fn new() -> Self {
		Self {
			japi: std::ptr::null::<RedisJSONAPI>(),
			version: 0
		}
	}

	pub fn get_json_apis(ctx: *mut RedisModuleCtx, subscribe_to_module_change: bool) {
		unsafe { REDIS_JSON_API.get_json_apis_inner(ctx, subscribe_to_module_change) }
	}

	unsafe fn get_json_apis_inner(&mut self, ctx: *mut RedisModuleCtx, subscribe_to_module_change: bool) {
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

	fn is_null() -> bool {
		unsafe { REDIS_JSON_API.japi }.is_null() 
	}
	pub fn get_version() -> i32 {
		unsafe { REDIS_JSON_API.version }
	}

	pub fn isJSON(key: RedisKey) -> bool {
		unsafe { rj_api().isJSON.unwrap()(key.key_inner) != 0 }
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

static mut REDIS_JSON_API: RjApi = RjApi::new();

pub fn rj_api() -> &'static RedisJSONAPI {
	unsafe { &*REDIS_JSON_API.japi }
}

extern "C" fn module_change_handler(
	ctx: *mut RedisModuleCtx,
	_event: RedisModuleEvent,
	sub: u64,
	ei: *mut c_void
) {
	let ei = unsafe { *(ei as *mut RedisModuleModuleChange) };
	if sub == REDISMODULE_SUBEVENT_MODULE_LOADED as u64 &&                    // If the subscribed event is a module load,
		RjApi::is_null() &&                                                     // and JSON is not already loaded,
		unsafe { CStr::from_ptr(ei.module_name) }.to_str().unwrap() == "ReJSON" // and the loading module is JSON:
	{
		RjApi::get_json_apis(ctx, false);                                       // try to load it.
	}
}
