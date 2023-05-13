//! The `Storage` trait itself, the new API for allocation.

use core::{
    alloc::{AllocError, Layout},
    ptr::{self, NonNull},
};

/// A trait abstracting memory storage.
///
/// This trait returns handles to allocated memory, which can be freely copied and stored, then resolved into actual
/// pointers at a later time.
///
/// #   Safety
///
/// Only valid handles may be safely resolved. When a handle is invalidated, all its copies are also invalidated at the
/// same time, and all pointers resolved from the handle or any of its copies are invalidated as well.
///
/// Handle Invalidation:
///
/// -   All handles allocated by an instance of `Storage` may be invalidated when calling `Storage::allocate` or
///     `Storage::allocate_zeroed` on this instance of `Storage`. Handles are only guaranteed to remain valid across
///     calls to these methods for instances also implementing `MultipleStorage`.
/// -   A handle is immediately invalidated when used as an argument to the `Storage::deallocate` method.
/// -   A handle is invalidated when used as an argument to the `Storage::grow`, `Storage::grow_zeroed`, or
///     `Storage::shrink` and these methods succeed.
///
/// Pointer Invalidation:
///
/// -   All pointers resolved by an instance of `Storage` may be invalidated when moving this instance of `Storage`.
///     Pointers are only guaranteed to remain valid across moves for instances also implementing `PinningStorage`.
/// -   All pointers resolved by an instance of `Storage` may be invalidated when calling `Storage::allocate`,
///     `Storage::deallocate`, `Storage::grow`, `Storage::shrink`, or their zeroed variants. Pointers are only
///     guaranteed to remain valid across those calls for instances also implementing `StableStorage`.
/// -   All pointers resolved by an instance of `Storage` from a _different_ handle may be invalidated when calling
///     `Storage::resolve`. Pointers from different handles are only guaranteed to remain valid across those calls for
///     instances also implementing `StableStorage`.
///
/// A specific implementation of Storage may provide extended validity guarantees, and should implement the extended
/// guarantees traits when it does so.
pub unsafe trait Storage {
    /// A Handle to memory allocated by the instance of Storage which creates it.
    type Handle: Copy;

    /// Creates a dangling handle.
    ///
    /// A dangling handle is never valid, and thus cannot be deallocated, resolved, grown, shrunk, etc... Furthermore
    /// there is no explicit way to distinguish whether a handle is dangling, or not. It is up to the user to remember
    /// whether a given handle is dangling, valid, or used to be valid but was invalidated.
    fn dangling() -> Self::Handle;

    /// Resolves the `handle` into a pointer to the first byte of the associated block of memory.
    ///
    /// Unless `self` implements `StableStorage`, all previously resolved pointers from different handles may be
    /// invalidated.
    ///
    /// #   Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   The block of memory associated to the handle is only valid for as long as the `handle` is valid itself.
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8>;

    /// Attempts to allocate a block of memory.
    ///
    /// On success, returns a `Handle` to a block of memory meeting the size and alignment guarantees of `Layout` and
    /// actual size of the block of memory.
    ///
    /// Unless `self` implements `MultipleStorage`, all previously allocated handles may be invalidated.
    ///
    /// Unless `self` implements `StableStorage`, all previously resolved pointers may be invalidated.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the storage cannot satisfy `layout`
    /// constraints.
    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError>;

    /// Deallocates the memory referenced by `handle`.
    ///
    /// This invalidates `handle` and all its copies, as well as all pointers resolved from `handle` or any of its
    /// copies.
    ///
    /// Unless `self` implements `MultipleStorage`, all previously allocated handles may be invalidated.
    ///
    /// Unless `self` implements `StableStorage`, all previously resolved pointers may be invalidated.
    ///
    /// #   Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   `layout` must fit the associated block of memory.
    unsafe fn deallocate(&self, handle: Self::Handle, layout: Layout);

    /// Attempts to extend the block of memory associated with `handle`.
    ///
    /// On success, returns a new `Self::Handle` associated with the extended block of memory, and may invalidate
    /// `handle` and all its copies, as well as all pointers resolved from `handle` or any of its copies.
    ///
    /// On failure, `handle` and all its copies are still valid, though any pointer resolved from `handle` or any of
    /// its copies may have been invalidated.
    ///
    /// Unless `self` implements `MultipleStorage`, all previously allocated handles may be invalidated.
    ///
    /// Unless `self` implements `StableStorage`, all previously resolved pointers may be invalidated.
    ///
    /// #    Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   `old_layout` must fit the associated block of memory.
    /// -   `new_layout.size()` must be greater than or equal to `old_layout.size()`.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the storage cannot satisfy `new_layout`
    /// constraints.
    unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError>;

    /// Attempts to shrink the block of memory associated with `handle`.
    ///
    /// On success, returns a new `Self::Handle` associated with the extended block of memory, and may invalidate
    /// `handle` and all its copies, as well as all pointers resolved from `handle` or any of its copies.
    ///
    /// On failure, `handle` and all its copies are still valid, though any pointer resolved from `handle` or any of
    /// its copies may have been invalidated.
    ///
    /// Unless `self` implements `MultipleStorage`, all previously allocated handles may be invalidated.
    ///
    /// Unless `self` implements `StableStorage`, all previously resolved pointers may be invalidated.
    ///
    /// #    Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   `old_layout` must fit the associated block of memory.
    /// -   `new_layout.size()` must be smaller than or equal to `old_layout.size()`.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the storage cannot satisfy `new_layout`
    /// constraints.
    unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError>;

    /// Behaves like `allocate`, but also ensures that the associated block of memory is zero-initialized.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the storage cannot satisfy `new_layout`
    /// constraints.
    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        let (handle, size) = self.allocate(layout)?;

        //  Safety:
        //  -   `handle` has been allocated by `self`.
        //  -   `handle` is still valid, since no operation was performed on self.
        let pointer = unsafe { self.resolve(handle) };

        //  Safety:
        //  -   `pointer` is valid, since `handle` is valid.
        //  -   `pointer` points to at an area of at least `size` bytes.
        //  -   Access to the next `size` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer.as_ptr(), 0, size) };

        Ok((handle, size))
    }

    /// Behaves like `grow`, but also ensures that the associated block of memory is zero-initialized.
    ///
    /// #   Safety
    ///
    /// As per `grow`.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the storage cannot satisfy `new_layout`
    /// constraints.
    unsafe fn grow_zeroed(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   All pre-conditions of `grow` are pre-conditions of `grow_zeroed`.
        let (handle, new_size) = unsafe { self.grow(handle, old_layout, new_layout)? };

        //  Safety:
        //  -   `handle` has been allocated by `self`.
        //  -   `handle` is still valid, since no operation was performed on self.
        let pointer = unsafe { self.resolve(handle) };

        //  Safety:
        //  -   Both starting and resulting pointers are in bounds of the same allocated objects as `old_layout` fits
        //      `pointer`, as per the pre-conditions of `grow_zeroed`.
        //  -   The offset does not overflow `isize` as `old_layout.size()` does not.
        let pointer = unsafe { pointer.as_ptr().add(old_layout.size()) };

        //  Safety:
        //  -   `pointer` is valid, since `handle` is valid.
        //  -   `pointer` points to an area of at least `new_size - old_layout.size()`.
        //  -   Access to the next `new_size - old_layout.size()` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer, 0, new_size - old_layout.size()) };

        Ok((handle, new_size))
    }
}

/// A refinement of `Storage` which does not invalidate handles on allocation.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that:
///
/// -   Existing handles are not invalidated by calls to `allocate`, and `allocate_zeroed`.
/// -   Unrelated existing handles are not invalidated by calls to `grow`, `grow_zeroed`, `shrink`, and `deallocate`.
///
/// This trait provides no guarantee with regard to the stability of resolved pointers, for such guarantees see
/// `StableStorage` and `PinningStorage`.
pub unsafe trait MultipleStorage: Storage {}

/// A refinement of `Storage` which guarantees that the blocks of memory are stable in memory across method calls, but
/// not necessarily across moves.
///
/// If the blocks of memory should be stable in memory across moves as well, then `PinningStorage` is required.
///
/// It is common but not required for implementers of this trait to also implement `MultipleStorage`.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that a handle always resolve to the same block of memory for as long as
/// it is valid and the instance of `Storage` has not moved.
pub unsafe trait StableStorage: Storage {}

/// A refinement of `Storage` which guarantees that the blocks of memory are pinned in memory.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that a handle always resolve to the same block of memory for as long as
/// it is valid.
pub unsafe trait PinningStorage: StableStorage {}

//
//  Provided for convenience.
//

//  If `S` is a `MultipleStorage`, then `allocate` doesn't invalidate handles, and thus `grow` and `shrink` can be
//  default implemented in terms of `allocate` and `deallocate` -- even if this is not optimal.
//
//  Further, `grow_zeroed` can be optimized compared to its default implementation by using `allocate_zeroed`.
default unsafe impl<S> Storage for S
where
    S: MultipleStorage,
{
    default unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "{new_layout:?} must have a greater size than {old_layout:?}"
        );

        let (new_handle, new_size) = self.allocate(new_layout)?;

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `grow`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `grow`
        //      and has not been invalidated by `self.allocate` since `self` is a `MultipleStorage`.
        let current_ptr = unsafe { self.resolve(handle) };

        //  Safety:
        //  -   `new_handle` has been allocated by `self`.
        //  -   `new_handle` is still valid, since only `self.resolve` was called which doesn't invalidate handles.
        let new_ptr = unsafe { self.resolve(new_handle) };

        //  Safety:
        //  -   `current_ptr` is valid for reads, as `handle` is valid.
        //  -   `new_ptr` is valid for writes, as `handle` is valid _and_ exclusive access is guaranteed.
        //  -   `current_ptr` is valid for `old_layout.size()` bytes, as `old_layout` fits `handle` as per the
        //      pre-conditions of `grow`.
        //  -   `new_ptr` is valid for `old_layout.size()` bytes, as `new_layout.size()` is greater than or equal to
        //      that as per the pre-conditions of `grow`.
        unsafe { ptr::copy_nonoverlapping(current_ptr.as_ptr(), new_ptr.as_ptr(), old_layout.size()) };

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `grow`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `grow`
        //      and has not been invalidated by `self.allocate` since `self` is a `MultipleStorage`.
        //  -   `old_layout` fits `handle`, as per the pre-conditions of `grow`.
        unsafe { self.deallocate(handle, old_layout) };

        Ok((new_handle, new_size))
    }

    default unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "{new_layout:?} must have a smaller size than {old_layout:?}"
        );

        let (new_handle, new_size) = self.allocate(new_layout)?;

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `shrink`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `shrink`
        //      and has not been invalidated by `self.allocate` since `self` is a `MultipleStorage`.
        let current_ptr = unsafe { self.resolve(handle) };

        //  Safety:
        //  -   `new_handle` has been allocated by `self`.
        //  -   `new_handle` is still valid, since only `self.resolve` was called which doesn't invalidate handles.
        let new_ptr = unsafe { self.resolve(new_handle) };

        //  Safety:
        //  -   `current_ptr` is valid for reads, as `handle` is valid.
        //  -   `new_ptr` is valid for writes, as `handle` is valid _and_ exclusive access is guaranteed.
        //  -   `new_ptr` is valid `new_size` bytes, as it was allocated with `new_layout`.
        //  -   `current_ptr` is valid for `new_size` bytes, as it is smaller than or equal to
        //      `old_layout.size()` as per the pre-conditions of `shrink`.
        unsafe { ptr::copy_nonoverlapping(current_ptr.as_ptr(), new_ptr.as_ptr(), new_size) };

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `shrink`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `shrink`
        //      and has not been invalidated by `self.allocate` since `self` is a `MultipleStorage`.
        //  -   `old_layout` fits `handle`, as per the pre-conditions of `shrink`.
        unsafe { self.deallocate(handle, old_layout) };

        Ok((new_handle, new_size))
    }

    default unsafe fn grow_zeroed(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "{new_layout:?} must have a greater size than {old_layout:?}"
        );

        let (new_handle, new_size) = self.allocate_zeroed(new_layout)?;

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `grow`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `grow`
        //      and has not been invalidated by `self.allocate` since `self` is a `MultipleStorage`.
        let current_ptr = unsafe { self.resolve(handle) };

        //  Safety:
        //  -   `new_handle` has been allocated by `self`.
        //  -   `new_handle` is still valid, since only `self.resolve` was called which doesn't invalidate handles.
        let new_ptr = unsafe { self.resolve(new_handle) };

        //  Safety:
        //  -   `current_ptr` is valid for reads, as `handle` is valid.
        //  -   `new_ptr` is valid for writes, as `handle` is valid _and_ exclusive access is guaranteed.
        //  -   `current_ptr` is valid for `old_layout.size()` bytes, as `old_layout` fits `handle` as per the
        //      pre-conditions of `grow`.
        //  -   `new_ptr` is valid for `old_layout.size()` bytes, as `new_layout.size()` is greater than or equal to
        //      that as per the pre-conditions of `grow`.
        unsafe { ptr::copy_nonoverlapping(current_ptr.as_ptr(), new_ptr.as_ptr(), old_layout.size()) };

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `grow`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `grow`
        //      and has not been invalidated by `self.allocate` since `self` is a `MultipleStorage`.
        //  -   `old_layout` fits `handle`, as per the pre-conditions of `grow`.
        unsafe { self.deallocate(handle, old_layout) };

        Ok((new_handle, new_size))
    }
}
