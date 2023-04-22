use core::{
    alloc::{AllocError, Allocator, Layout},
    ptr::NonNull,
};

use std::alloc::Global as GlobalAllocator;

use crate::storage::AllocatorStorage;

pub(crate) type Global = AllocatorStorage<GlobalAllocator>;

#[derive(Debug, Default)]
pub(crate) struct NonAllocator;

unsafe impl Allocator for NonAllocator {
    fn allocate(&self, _layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Err(AllocError)
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        panic!("NonAllocator::deallocate called!")
    }
}
