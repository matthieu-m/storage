//! An implementation of `Store` providing a single, inline, block of memory.
//!
//! This store is suitable for `Box`, `Vec`, or `VecDeque`, for example.

use core::{
    alloc::{AllocError, Layout},
    fmt,
    mem::{self, MaybeUninit},
    ptr::{self, Alignment, NonNull},
};

use crate::interface::{StoreDangling, StoreSingle, StoreStable};

/// An implementation of `Store` providing a single, inline, block of memory.
///
/// The block of memory is aligned and sized as per `T`.
pub struct InlineSingleStore<T>(MaybeUninit<T>);

impl<T> InlineSingleStore<T> {
    /// Creates a new instance.
    pub const fn new() -> Self {
        Self(MaybeUninit::uninit())
    }
}

impl<T> Default for InlineSingleStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<T> const StoreDangling for InlineSingleStore<T> {
    type Handle = ();

    fn dangling(&self, alignment: Alignment) -> Result<Self::Handle, AllocError> {
        if alignment.as_usize() <= Alignment::of::<T>().as_usize() {
            Ok(())
        } else {
            Err(AllocError)
        }
    }
}

unsafe impl<T> const StoreSingle for InlineSingleStore<T> {
    unsafe fn resolve(&self, _handle: Self::Handle) -> NonNull<u8> {
        let pointer = self.0.as_ptr() as *mut T;

        //  Safety:
        //  -   `self` is non null.
        unsafe { NonNull::new_unchecked(pointer) }.cast()
    }

    unsafe fn resolve_mut(&mut self, _handle: Self::Handle) -> NonNull<u8> {
        let pointer = self.0.as_mut_ptr();

        //  Safety:
        //  -   `self` is non null.
        unsafe { NonNull::new_unchecked(pointer) }.cast()
    }

    fn allocate(&mut self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        if Self::validate_layout(layout).is_err() {
            return Err(AllocError);
        }

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn deallocate(&mut self, _handle: Self::Handle, _layout: Layout) {}

    unsafe fn grow(
        &mut self,
        _handle: Self::Handle,
        _old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= _old_layout.size(),
            "new_layout must have a greater size than _old_layout"
        );

        if Self::validate_layout(new_layout).is_err() {
            return Err(AllocError);
        }

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn shrink(
        &mut self,
        _handle: Self::Handle,
        _old_layout: Layout,
        _new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            _new_layout.size() >= _old_layout.size(),
            "_new_layout must have a smaller size than _old_layout"
        );

        Ok(((), mem::size_of::<T>()))
    }

    fn allocate_zeroed(&mut self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        if Self::validate_layout(layout).is_err() {
            return Err(AllocError);
        }

        let pointer = self.0.as_mut_ptr() as *mut u8;

        //  Safety:
        //  -   `pointer` is valid, since `self` is valid.
        //  -   `pointer` points to at an area of at least `mem::size_of::<T>()`.
        //  -   Access to the next `mem::size_of::<T>()` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer, 0, mem::size_of::<T>()) };

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn grow_zeroed(
        &mut self,
        _handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "new_layout must have a greater size than old_layout"
        );

        if Self::validate_layout(new_layout).is_err() {
            return Err(AllocError);
        }

        let pointer = self.0.as_mut_ptr() as *mut u8;

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
unsafe impl<T> StoreStable for InlineSingleStore<T> {}

impl<T> fmt::Debug for InlineSingleStore<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let layout = Layout::new::<T>();

        f.debug_struct("InlineSingleStore")
            .field("size", &layout.size())
            .field("align", &layout.align())
            .finish()
    }
}

//  Safety:
//  -   Self-contained, so can be sent across threads safely.
unsafe impl<T> Send for InlineSingleStore<T> {}

//  Safety:
//  -   Immutable (by itself), so can be shared across threads safely.
unsafe impl<T> Sync for InlineSingleStore<T> {}

//
//  Implementation
//

impl<T> InlineSingleStore<T> {
    const fn validate_layout(layout: Layout) -> Result<(), AllocError> {
        let own = Layout::new::<T>();

        if layout.align() <= own.align() && layout.size() <= own.size() {
            Ok(())
        } else {
            Err(AllocError)
        }
    }
}
