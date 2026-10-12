/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

#![cfg(feature = "as-library")]

use redis_module::{
    key::KeyFlags, AclCategory, Context, InfoContext, RedisError, RedisResult, Status,
};
use rejson::{c_api::*, commands::*, *};

// Expand outside the library so private paths cannot hide behind crate-local tests.
redis_json_module_create! {
    data_types: [REDIS_JSON_TYPE],
    pre_command_function: pre_command,
    get_manage: {
        _ => None::<rejson::ivalue_manager::RedisIValueJsonKeyManager>
    },
    version: 1,
    init: |_, _| Status::Ok,
    info: info,
}

fn pre_command(_ctx: &Context, _args: &[RedisString]) -> RedisResult<()> {
    Err(RedisError::Str("ERR commands disabled"))
}

fn info(_ctx: &InfoContext, _for_crash_report: bool) {}

#[test]
fn downstream_module_command_runs_pre_command() {
    assert!(matches!(
        json_get(&Context::dummy(), Vec::new()),
        Err(RedisError::Str("ERR commands disabled"))
    ));
}
