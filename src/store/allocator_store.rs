//! Wraps an allocator to provide a `Store` API.

use core::{
    alloc::{AllocError, Allocator, Layout},
    ptr::NonNull,
};

#[cfg(feature = "alloc")]
use alloc::alloc::Global;

use crate::interface::{MultipleStore, PinningStore, StableStore, Store};

#[cfg(feature = "alloc")]
use crate::interface::SharingStore;

unsafe impl<A> Store for A
where
    A: Allocator,
{
    type Handle = NonNull<u8>;

    fn dangling(&self) -> Self::Handle {
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
unsafe impl<A> MultipleStore for A where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> StableStore for A where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> PinningStore for A where A: Allocator {}

//  Safety:
//  -   `Allocator` are always sharing, today.
#[cfg(feature = "alloc")]
unsafe impl SharingStore for Global {
    type SharingError = !;

    fn is_sharing_with(&self, _other: &Self) -> bool {
        true
    }

    fn share(&self) -> Result<Self, Self::SharingError> {
        Ok(*self)
    }
}
