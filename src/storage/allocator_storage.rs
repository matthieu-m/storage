//! Wraps an allocator to provide a `Storage` API.

use core::{
    alloc::{AllocError, Allocator, Layout},
    ptr::NonNull,
};

use crate::interface::{MultipleStorage, PinningStorage, StableStorage, Storage};

/// Adapter of the `Allocator` API to the `Storage` API.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocatorStorage<A>(A);

impl<A> AllocatorStorage<A> {
    /// Creates a new instance, with the specific allocator.
    pub fn new(allocator: A) -> Self {
        Self(allocator)
    }
}

unsafe impl<A> Storage for AllocatorStorage<A>
where
    A: Allocator,
{
    type Handle = NonNull<u8>;

    fn dangling() -> Self::Handle {
        NonNull::dangling()
    }

    fn allocate(&self, layout: Layout) -> Result<Self::Handle, AllocError> {
        self.0.allocate(layout).map(|slice| slice.as_non_null_ptr())
    }

    unsafe fn deallocate(&self, handle: Self::Handle, layout: Layout) {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `deallocate`.
        //  -   `layout` fits, as per the pre-conditions of `deallocate`.
        unsafe { self.0.deallocate(handle, layout) };
    }

    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8> {
        handle
    }

    unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<Self::Handle, AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `grow`.
        //  -   `old_layout` fits, as per the pre-conditions of `grow`.
        //  -   `new_layout.size()` is greater than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `grow`.
        unsafe {
            self.0
                .grow(handle, old_layout, new_layout)
                .map(|slice| slice.as_non_null_ptr())
        }
    }

    unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<Self::Handle, AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `shrink`.
        //  -   `old_layout` fits, as per the pre-conditions of `shrink`.
        //  -   `new_layout.size()` is smaller than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `shrink`.
        unsafe {
            self.0
                .shrink(handle, old_layout, new_layout)
                .map(|slice| slice.as_non_null_ptr())
        }
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<Self::Handle, AllocError> {
        self.0.allocate_zeroed(layout).map(|slice| slice.as_non_null_ptr())
    }

    unsafe fn grow_zeroed(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<Self::Handle, AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `grow_zeroed`.
        //  -   `old_layout` fits, as per the pre-conditions of `grow_zeroed`.
        //  -   `new_layout.size()` is greater than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `grow_zeroed`.
        unsafe {
            self.0
                .grow_zeroed(handle, old_layout, new_layout)
                .map(|slice| slice.as_non_null_ptr())
        }
    }
}

//  Safety:
//  -   `Allocator` does not invalidate existing allocations when allocating.
unsafe impl<A> MultipleStorage for AllocatorStorage<A> where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> StableStorage for AllocatorStorage<A> where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> PinningStorage for AllocatorStorage<A> where A: Allocator {}
