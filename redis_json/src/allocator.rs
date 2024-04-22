//! The allocator facilities for all JSON values.

use redis_custom_allocator::CustomAllocator;
use redis_module::alloc::RedisAlloc;

/// An allocator for all JSON values.
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonAllocator(RedisAlloc);

impl CustomAllocator for JsonAllocator {
    type Error = std::convert::Infallible;

    fn allocate(&self, layout: std::alloc::Layout) -> Result<std::ptr::NonNull<[u8]>, Self::Error> {
        self.0.alloc(layout)
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        self.0.dealloc(ptr.as_ptr(), layout)
    }
}
