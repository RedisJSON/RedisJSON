//! The allocator facilities for all JSON values.

use redis_custom_allocator::CustomAllocator;
use redis_module::alloc::RedisAlloc;
use std::alloc::GlobalAlloc;

/// An allocator for all JSON values.
#[repr(transparent)]
#[derive(Default, Debug, Copy, Clone)]
pub struct JsonAllocator(RedisAlloc);

impl CustomAllocator for JsonAllocator {
    type Error = std::convert::Infallible;

    fn allocate(&self, layout: std::alloc::Layout) -> Result<std::ptr::NonNull<[u8]>, Self::Error> {
        let slice = unsafe { std::slice::from_raw_parts_mut(self.0.alloc(layout), layout.size()) };
        Ok(std::ptr::NonNull::from(slice))
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        self.0.dealloc(ptr.as_ptr(), layout)
    }
}
