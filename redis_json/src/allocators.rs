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
    ptr::NonNull,
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

#[derive(Debug)]
pub struct CustomAllocError;

impl std::error::Error for CustomAllocError {}

impl std::fmt::Display for CustomAllocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("memory allocation failed")
    }
}

/// This trait is almost a drop-in copy of the [`std::alloc::Allocator`]
/// trait.
pub trait CustomAllocator {
    /// Attempts to allocate a block of memory.
    ///
    /// On success, returns a [`NonNull<[u8]>`][NonNull] meeting the size and alignment guarantees of `layout`.
    ///
    /// The returned block may have a larger size than specified by `layout.size()`, and may or may
    /// not have its contents initialized.
    ///
    /// # Errors
    ///
    /// Returning `Err` indicates that either memory is exhausted or `layout` does not meet
    /// allocator's size or alignment constraints.
    ///
    /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
    /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
    /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
    ///
    /// Clients wishing to abort computation in response to an allocation error are encouraged to
    /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
    ///
    /// [`handle_alloc_error`]: std::alloc::handle_alloc_error
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, CustomAllocError>;

    /// Deallocates the memory referenced by `ptr`.
    ///
    /// # Safety
    ///
    /// * `ptr` must denote a block of memory *currently allocated* via this allocator, and
    /// * `layout` must *fit* that block of memory.
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);

    /// Behaves like `allocate`, but also ensures that the returned memory is zero-initialized.
    ///
    /// # Errors
    ///
    /// Returning `Err` indicates that either memory is exhausted or `layout` does not meet
    /// allocator's size or alignment constraints.
    ///
    /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
    /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
    /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
    ///
    /// Clients wishing to abort computation in response to an allocation error are encouraged to
    /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
    ///
    /// [`handle_alloc_error`]: std::alloc::handle_alloc_error
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, CustomAllocError> {
        self.allocate(layout).map(|mut ptr| {
            unsafe {
                ptr.as_mut().fill(0);
            }
            ptr
        })
    }

    /// Attempts to extend the memory block.
    ///
    /// Returns a new [`NonNull<[u8]>`][NonNull] containing a pointer and the actual size of the allocated
    /// memory. The pointer is suitable for holding data described by `new_layout`. To accomplish
    /// this, the allocator may extend the allocation referenced by `ptr` to fit the new layout.
    ///
    /// If this returns `Ok`, then ownership of the memory block referenced by `ptr` has been
    /// transferred to this allocator. Any access to the old `ptr` is Undefined Behavior, even if the
    /// allocation was grown in-place. The newly returned pointer is the only valid pointer
    /// for accessing this memory now.
    ///
    /// If this method returns `Err`, then ownership of the memory block has not been transferred to
    /// this allocator, and the contents of the memory block are unaltered.
    ///
    /// # Safety
    ///
    /// * `ptr` must denote a block of memory [*currently allocated*] via this allocator.
    /// * `old_layout` must [*fit*] that block of memory (The `new_layout` argument need not fit it.).
    /// * `new_layout.size()` must be greater than or equal to `old_layout.size()`.
    ///
    /// Note that `new_layout.align()` need not be the same as `old_layout.align()`.
    ///
    /// [*currently allocated*]: #currently-allocated-memory
    /// [*fit*]: #memory-fitting
    ///
    /// # Errors
    ///
    /// Returns `Err` if the new layout does not meet the allocator's size and alignment
    /// constraints of the allocator, or if growing otherwise fails.
    ///
    /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
    /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
    /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
    ///
    /// Clients wishing to abort computation in response to an allocation error are encouraged to
    /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
    ///
    /// [`handle_alloc_error`]: std::alloc::handle_alloc_error
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, CustomAllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        let mut new_ptr = self.allocate(new_layout)?;

        // SAFETY: because `new_layout.size()` must be greater than or equal to
        // `old_layout.size()`, both the old and new memory allocation are valid for reads and
        // writes for `old_layout.size()` bytes. Also, because the old allocation wasn't yet
        // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
        // safe. The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            std::ptr::copy_nonoverlapping(
                ptr.as_ptr(),
                new_ptr.as_mut().as_mut_ptr(),
                old_layout.size(),
            );
            self.deallocate(ptr, old_layout);
        }

        Ok(new_ptr)
    }

    /// Behaves like [`Self::grow`], but also ensures that the new contents are set to zero before being
    /// returned.
    ///
    /// The memory block will contain the following contents after a successful call to
    /// `grow_zeroed`:
    ///   * Bytes `0..old_layout.size()` are preserved from the original allocation.
    ///   * Bytes `old_layout.size()..old_size` will either be preserved or zeroed, depending on
    ///     the allocator implementation. `old_size` refers to the size of the memory block prior
    ///     to the `grow_zeroed` call, which may be larger than the size that was originally
    ///     requested when it was allocated.
    ///   * Bytes `old_size..new_size` are zeroed. `new_size` refers to the size of the memory
    ///     block returned by the `grow_zeroed` call.
    ///
    /// # Safety
    ///
    /// * `ptr` must denote a block of memory [*currently allocated*] via this allocator.
    /// * `old_layout` must [*fit*] that block of memory (The `new_layout` argument need not fit it.).
    /// * `new_layout.size()` must be greater than or equal to `old_layout.size()`.
    ///
    /// Note that `new_layout.align()` need not be the same as `old_layout.align()`.
    ///
    /// [*currently allocated*]: #currently-allocated-memory
    /// [*fit*]: #memory-fitting
    ///
    /// # Errors
    ///
    /// Returns `Err` if the new layout does not meet the allocator's size and alignment
    /// constraints of the allocator, or if growing otherwise fails.
    ///
    /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
    /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
    /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
    ///
    /// Clients wishing to abort computation in response to an allocation error are encouraged to
    /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
    ///
    /// [`handle_alloc_error`]: std::alloc::handle_alloc_error
    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, CustomAllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        let mut new_ptr = self.allocate_zeroed(new_layout)?;

        // SAFETY: because `new_layout.size()` must be greater than or equal to
        // `old_layout.size()`, both the old and new memory allocation are valid for reads and
        // writes for `old_layout.size()` bytes. Also, because the old allocation wasn't yet
        // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
        // safe. The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            std::ptr::copy_nonoverlapping(
                ptr.as_ptr(),
                new_ptr.as_mut().as_mut_ptr(),
                old_layout.size(),
            );
            self.deallocate(ptr, old_layout);
        }

        Ok(new_ptr)
    }

    /// Attempts to shrink the memory block.
    ///
    /// Returns a new [`NonNull<[u8]>`][NonNull] containing a pointer and the actual size of the allocated
    /// memory. The pointer is suitable for holding data described by `new_layout`. To accomplish
    /// this, the allocator may shrink the allocation referenced by `ptr` to fit the new layout.
    ///
    /// If this returns `Ok`, then ownership of the memory block referenced by `ptr` has been
    /// transferred to this allocator. Any access to the old `ptr` is Undefined Behavior, even if the
    /// allocation was shrunk in-place. The newly returned pointer is the only valid pointer
    /// for accessing this memory now.
    ///
    /// If this method returns `Err`, then ownership of the memory block has not been transferred to
    /// this allocator, and the contents of the memory block are unaltered.
    ///
    /// # Safety
    ///
    /// * `ptr` must denote a block of memory [*currently allocated*] via this allocator.
    /// * `old_layout` must [*fit*] that block of memory (The `new_layout` argument need not fit it.).
    /// * `new_layout.size()` must be smaller than or equal to `old_layout.size()`.
    ///
    /// Note that `new_layout.align()` need not be the same as `old_layout.align()`.
    ///
    /// [*currently allocated*]: #currently-allocated-memory
    /// [*fit*]: #memory-fitting
    ///
    /// # Errors
    ///
    /// Returns `Err` if the new layout does not meet the allocator's size and alignment
    /// constraints of the allocator, or if shrinking otherwise fails.
    ///
    /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
    /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
    /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
    ///
    /// Clients wishing to abort computation in response to an allocation error are encouraged to
    /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
    ///
    /// [`handle_alloc_error`]: std::alloc::handle_alloc_error
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, CustomAllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );

        let mut new_ptr = self.allocate(new_layout)?;

        // SAFETY: because `new_layout.size()` must be lower than or equal to
        // `old_layout.size()`, both the old and new memory allocation are valid for reads and
        // writes for `new_layout.size()` bytes. Also, because the old allocation wasn't yet
        // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
        // safe. The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            std::ptr::copy_nonoverlapping(
                ptr.as_ptr(),
                new_ptr.as_mut().as_mut_ptr(),
                new_layout.size(),
            );
            self.deallocate(ptr, old_layout);
        }

        Ok(new_ptr)
    }

    /// Creates a "by reference" adapter for this instance of
    /// [`CustomAllocator`].
    ///
    /// The returned adapter also implements [`CustomAllocator`] and
    /// will simply borrow this.
    fn by_ref(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(feature = "allocator_api")]
impl<T> CustomAllocator for T
where
    T: std::alloc::Allocator,
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        (self as &std::alloc::Allocator).allocate(layout)
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        (self as &std::alloc::Allocator).allocate_zeroed(layout)
    }

    fn by_ref(&self) -> &Self
    where
        Self: Sized,
    {
        (self as &std::alloc::Allocator).by_ref()
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        (self as &std::alloc::Allocator).deallocate(ptr, layout)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        (self as &std::alloc::Allocator).grow(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        (self as &std::alloc::Allocator).grow_zeroed(ptr, old_layout, new_layout)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        (self as &std::alloc::Allocator).shrink(ptr, old_layout, new_layout)
    }
}

impl CustomAllocator for JsonValueAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, CustomAllocError> {
        match layout.size() {
            0 => Ok(NonNull::slice_from_raw_parts(
                std::ptr::NonNull::dangling(),
                0,
            )),
            _ => {
                let ptr = unsafe { self.alloc(layout) };
                if let Some(ptr) = NonNull::new(ptr) {
                    Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
                } else {
                    Err(CustomAllocError)
                }
            }
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.dealloc(ptr.as_ptr(), layout)
    }
}
