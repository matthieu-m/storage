//! Wraps an allocator to provide a `Store` API.

use core::{
    alloc::{AllocError, Allocator, Layout},
    ptr::{self, Alignment, NonNull},
};

#[cfg(feature = "alloc")]
use alloc::alloc::Global;

use crate::interface::{Store, StoreDangling, StorePinning, StoreSingle, StoreStable};

#[cfg(feature = "alloc")]
use crate::interface::StoreSharing;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AllocatorHandle(NonNull<u8>);

unsafe impl Send for AllocatorHandle {}
unsafe impl Sync for AllocatorHandle {}

impl From<NonNull<u8>> for AllocatorHandle {
    fn from(value: NonNull<u8>) -> Self {
        Self(value)
    }
}

impl From<AllocatorHandle> for NonNull<u8> {
    fn from(value: AllocatorHandle) -> Self {
        value.0
    }
}

unsafe impl<A> const StoreDangling for A
where
    A: Allocator,
{
    type Handle = AllocatorHandle;

    fn dangling(&self, alignment: Alignment) -> Result<Self::Handle, AllocError> {
        let pointer = ptr::invalid_mut(alignment.as_usize());

        //  Safety:
        //  -   Non-null, since `alignment` is non-zero.
        let pointer = unsafe { NonNull::new_unchecked(pointer) };

        Ok(AllocatorHandle(pointer))
    }
}

unsafe impl<A> Store for A
where
    A: Allocator,
{
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8> {
        handle.into()
    }

    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Allocator::allocate(self, layout).map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }

    unsafe fn deallocate(&self, handle: Self::Handle, layout: Layout) {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `deallocate`.
        //  -   `layout` fits, as per the pre-conditions of `deallocate`.
        unsafe { Allocator::deallocate(self, handle.into(), layout) };
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
        let result = unsafe { Allocator::grow(self, handle.into(), old_layout, new_layout) };

        result.map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
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
        let result = unsafe { Allocator::shrink(self, handle.into(), old_layout, new_layout) };

        result.map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Allocator::allocate_zeroed(self, layout).map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
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
        let result = unsafe { Allocator::grow_zeroed(self, handle.into(), old_layout, new_layout) };

        result.map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }
}

unsafe impl<A> StoreSingle for A
where
    A: Allocator,
{
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8> {
        handle.into()
    }

    unsafe fn resolve_mut(&mut self, handle: Self::Handle) -> NonNull<u8> {
        handle.into()
    }

    fn allocate(&mut self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Allocator::allocate(self, layout).map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }

    unsafe fn deallocate(&mut self, handle: Self::Handle, layout: Layout) {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `deallocate`.
        //  -   `layout` fits, as per the pre-conditions of `deallocate`.
        unsafe { Allocator::deallocate(self, handle.into(), layout) };
    }

    unsafe fn grow(
        &mut self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `grow`.
        //  -   `old_layout` fits, as per the pre-conditions of `grow`.
        //  -   `new_layout.size()` is greater than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `grow`.
        let result = unsafe { Allocator::grow(self, handle.into(), old_layout, new_layout) };

        result.map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }

    unsafe fn shrink(
        &mut self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `shrink`.
        //  -   `old_layout` fits, as per the pre-conditions of `shrink`.
        //  -   `new_layout.size()` is smaller than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `shrink`.
        let result = unsafe { Allocator::shrink(self, handle.into(), old_layout, new_layout) };

        result.map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }

    fn allocate_zeroed(&mut self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Allocator::allocate_zeroed(self, layout).map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }

    unsafe fn grow_zeroed(
        &mut self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   `handle` is valid, as per the pre-conditions of `grow_zeroed`.
        //  -   `old_layout` fits, as per the pre-conditions of `grow_zeroed`.
        //  -   `new_layout.size()` is greater than or equal to `old_layout.size()`, as per the pre-conditions of
        //      `grow_zeroed`.
        let result = unsafe { Allocator::grow_zeroed(self, handle.into(), old_layout, new_layout) };

        result.map(|slice| (slice.as_non_null_ptr().into(), slice.len()))
    }
}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> StoreStable for A where A: Allocator {}

//  Safety:
//  -   `Allocator` allocations are pinned.
unsafe impl<A> StorePinning for A where A: Allocator {}

//  Safety:
//  -   `Allocator` are always sharing, today.
#[cfg(feature = "alloc")]
unsafe impl StoreSharing for Global {
    type SharingError = !;

    fn is_sharing_with(&self, _other: &Self) -> bool {
        true
    }

    fn share(&self) -> Result<Self, Self::SharingError> {
        Ok(*self)
    }
}
