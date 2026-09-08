/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

//! Opt-in auto-creation of missing object levels along a write path
//!
//! Without it, `JSON.SET k $.a.b.c 5` errors when `k` is absent and returns a
//! silent `nil` when `k` exists but `$.a.b` does not.

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

/// Backing store for the `json-auto-create-deep-paths` module config,
/// registered in the `redis_module!` block (see `lib.rs`). The SDK writes here
/// through its `ConfigurationValue<bool> for AtomicBool` impl, which also uses
/// `Relaxed`.
pub static AUTO_CREATE_DEEP_PATHS: AtomicBool = AtomicBool::new(false);

/// Whether JSON write commands may create missing object levels along a path.
#[must_use]
pub fn auto_create_enabled() -> bool {
    AUTO_CREATE_DEEP_PATHS.load(AtomicOrdering::Relaxed)
}
