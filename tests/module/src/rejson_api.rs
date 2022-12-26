
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use redis_module::*;
use redis_module::key::RedisKey;
use cstr::cstr;
use crate::module_change_handler;

const RedisModuleEvent_ModuleChange: RedisModuleEvent = RedisModuleEvent {
	id: REDISMODULE_EVENT_MODULE_CHANGE,
	dataver: 1,
};

struct RjApi {
	japi: *const RedisJSONAPI,
	version: i32,
}

impl RjApi {
	unsafe fn api(&self) -> &RedisJSONAPI {
		&*self.japi
	}

	pub fn new() -> Self {
		Self {
			japi: std::ptr::null::<RedisJSONAPI>(),
			version: 0
		}
	}

	pub fn get_json_apis(&mut self, ctx: &Context, subscribe_to_module_change: bool) {
		if self.try_get_api(ctx, 2) { return; }
		
		if self.try_get_api(ctx, 1) { return; }

		if subscribe_to_module_change {
			subscribe_to_server_event(ctx.ctx, RedisModuleEvent_ModuleChange, Some(module_change_handler));
		}
	}

	fn try_get_api(&mut self, ctx: &Context, version: i32) -> bool {
		let mut japi = GetSharedAPI(ctx, version);
		if !japi.is_null() {
			unsafe {
				self.japi = japi as *const RedisJSONAPI;
				self.version = version;
			}
			return true;
		}
		false
	}

	pub fn is_loaded(&self) -> bool {
		!self.japi.is_null()
	}

	pub fn isJSON(&self, key: RedisKey) -> bool {
		unsafe { self.api().isJSON.unwrap()(key.key_inner) != 0 }
	}
	pub fn openKey(&self, ctx: &Context, keyname: &RedisString) -> RedisJSON {
		unsafe { self.api().openKey.unwrap()(ctx.ctx, keyname.inner) }
	}
}

pub fn GetSharedAPI(
  ctx: &Context,
  version: i32,
) -> *const RedisJSONAPI {
  match version {
    2 => unsafe { RedisModule_GetSharedAPI.unwrap()(ctx.ctx, cstr!("RedisJSON_V2").as_ptr()) as _ }
    1 => unsafe { RedisModule_GetSharedAPI.unwrap()(ctx.ctx, cstr!("RedisJSON_V1").as_ptr()) as _ }
    _ => panic!()
  }
}
