/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

//! The RedisJSON allocators.
//!
//! All the allocators use the Redis memory allocator to allocate
//! memory. The only difference here is that the allocators are
//! specialised for different types, mainly for the memory allocation
//! statistics.

use std::{
    alloc::{GlobalAlloc, Layout},
    fmt::Debug,
    sync::atomic::AtomicUsize,
};

pub struct JsonValueAllocator<A: GlobalAlloc = redis_module::alloc::RedisAlloc> {
    allocated: AtomicUsize,
    allocator: A,
}

impl JsonValueAllocator {
    /// Returns the amount of memory allocated by the allocator.
    #[inline]
    #[allow(unused)]
    pub fn get_allocated(&self) -> usize {
        self.allocated.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for JsonValueAllocator {
    fn default() -> Self {
        Self {
            allocated: 0.into(),
            allocator: redis_module::alloc::RedisAlloc,
        }
    }
}

impl Debug for JsonValueAllocator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonValueAllocator")
            .field("allocated", &self.allocated)
            .field("allocator", &"RedisAlloc")
            .finish()
    }
}

unsafe impl GlobalAlloc for JsonValueAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.allocator.alloc(layout);
        self.allocated
            .fetch_add(layout.size(), std::sync::atomic::Ordering::Relaxed);
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.allocator.dealloc(ptr, layout);
        self.allocated
            .fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
    }
}
