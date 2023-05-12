//! Wraps an allocator to provide a `Storage` API.

use core::{
    alloc::{AllocError, Allocator, Layout},
    ptr::NonNull,
};

use crate::interface::{MultipleStorage, PinningStorage, StableStorage, Storage};

unsafe impl<A> Storage for A
where
    A: Allocator,
{
    type Handle = NonNull<u8>;

    fn dangling() -> Self::Handle {
        NonNull::dangling()
    }

    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Allocator::allocate(self, layout).map(|slice| (slice.as_non_null_ptr(), slice.len()))
    }

    unsafe fn deallocate(&self, handle: Self::Handle, layout: Layout) {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `deallocate`.
        //  -   `layout` fits, as per the pre-conditions of `deallocate`.
        unsafe { Allocator::deallocate(self, handle, layout) };
    }

    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8> {
        handle
    }

    unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `grow`.
        //  -   `old_layout` fits, as per the pre-conditions of `grow`.
        //  -   `new_layout.size()` is greater than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `grow`.
        unsafe {
            Allocator::grow(self, handle, old_layout, new_layout).map(|slice| (slice.as_non_null_ptr(), slice.len()))
        }
    }

    unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `shrink`.
        //  -   `old_layout` fits, as per the pre-conditions of `shrink`.
        //  -   `new_layout.size()` is smaller than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `shrink`.
        unsafe {
            Allocator::shrink(self, handle, old_layout, new_layout).map(|slice| (slice.as_non_null_ptr(), slice.len()))
        }
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Allocator::allocate_zeroed(self, layout).map(|slice| (slice.as_non_null_ptr(), slice.len()))
    }

    unsafe fn grow_zeroed(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `grow_zeroed`.
        //  -   `old_layout` fits, as per the pre-conditions of `grow_zeroed`.
        //  -   `new_layout.size()` is greater than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `grow_zeroed`.
        unsafe {
            Allocator::grow_zeroed(self, handle, old_layout, new_layout)
                .map(|slice| (slice.as_non_null_ptr(), slice.len()))
        }
    }
}

//  Safety:
//  -   `Allocator` does not invalidate existing allocations when allocating.
unsafe impl<A> MultipleStorage for A where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> StableStorage for A where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> PinningStorage for A where A: Allocator {}
