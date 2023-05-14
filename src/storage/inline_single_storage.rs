//! An implementation of `Storage` providing a single, inline, block of memory.
//!
//! This storage is suitable for `Box`, `Vec`, or `VecDeque`, for example.

use core::{
    alloc::{AllocError, Layout},
    cell::UnsafeCell,
    fmt,
    mem::{self, MaybeUninit},
    ptr::{self, NonNull},
};

use crate::interface::{StableStorage, Storage};

/// An implementation of `Storage` providing a single, inline, block of memory.
///
/// The block of memory is aligned and sized as per `T`.
pub struct InlineSingleStorage<T>(UnsafeCell<MaybeUninit<T>>);

impl<T> Default for InlineSingleStorage<T> {
    fn default() -> Self {
        Self(UnsafeCell::new(MaybeUninit::uninit()))
    }
}

unsafe impl<T> Storage for InlineSingleStorage<T> {
    type Handle = ();

    fn dangling(&self) -> Self::Handle {}

    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Self::validate_layout(layout)?;

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn deallocate(&self, _handle: Self::Handle, _layout: Layout) {}

    unsafe fn resolve(&self, _handle: Self::Handle) -> NonNull<u8> {
        let pointer = self.0.get();

        //  Safety:
        //  -   `self` is non null.
        unsafe { NonNull::new_unchecked(pointer) }.cast()
    }

    unsafe fn grow(
        &self,
        _handle: Self::Handle,
        _old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= _old_layout.size(),
            "{new_layout:?} must have a greater size than {_old_layout:?}"
        );

        Self::validate_layout(new_layout)?;

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn shrink(
        &self,
        _handle: Self::Handle,
        _old_layout: Layout,
        _new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            _new_layout.size() >= _old_layout.size(),
            "{_new_layout:?} must have a smaller size than {_old_layout:?}"
        );

        Ok(((), mem::size_of::<T>()))
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Self::validate_layout(layout)?;

        let pointer = self.0.get() as *mut u8;

        //  Safety:
        //  -   `pointer` is valid, since `self` is valid.
        //  -   `pointer` points to at an area of at least `mem::size_of::<T>()`.
        //  -   Access to the next `mem::size_of::<T>()` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer, 0, mem::size_of::<T>()) };

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn grow_zeroed(
        &self,
        _handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "{new_layout:?} must have a greater size than {old_layout:?}"
        );

        Self::validate_layout(new_layout)?;

        let pointer = self.0.get() as *mut u8;

        //  Safety:
        //  -   Both starting and resulting pointers are in bounds of the same allocated objects as `old_layout` fits
        //      `pointer`, as per the pre-conditions of `grow_zeroed`.
        //  -   The offset does not overflow `isize` as `old_layout.size()` does not.
        let pointer = unsafe { pointer.add(old_layout.size()) };

        //  Safety:
        //  -   `pointer` is valid, since `self` is valid.
        //  -   `pointer` points to at an area of at least `mem::size_of::<T>() - old_layout.size()`.
        //  -   Access to the next `mem::size_of::<T>() - old_layout.size()` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer, 0, mem::size_of::<T>() - old_layout.size()) };

        Ok(((), mem::size_of::<T>()))
    }
}

//  Safety:
//  -   `self.resolve(handle)` always returns the same address, as long as `self` doesn't move.
unsafe impl<T> StableStorage for InlineSingleStorage<T> {}

impl<T> fmt::Debug for InlineSingleStorage<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let layout = Layout::new::<T>();

        f.debug_struct("InlineSingleStorage")
            .field("size", &layout.size())
            .field("align", &layout.align())
            .finish()
    }
}

//
//  Implementation
//

impl<T> InlineSingleStorage<T> {
    fn validate_layout(layout: Layout) -> Result<(), AllocError> {
        let own = Layout::new::<T>();

        if layout.align() <= own.align() && layout.size() <= own.size() {
            Ok(())
        } else {
            Err(AllocError)
        }
    }
}
