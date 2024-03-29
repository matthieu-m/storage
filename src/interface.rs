//! The `Store` trait itself, the new API for allocation.

use core::{
    alloc::{AllocError, Layout},
    ptr::{self, Alignment, NonNull},
};

/// A trait abstracting a generic memory store.
///
/// This trait returns handles to allocated memory, which can be freely copied and stored, then resolved into actual
/// pointers at a later time.
///
/// _Note: if only required to have a single outstanding allocation at a time, see `StoreSingle`._
///
/// #   Safety
///
/// Only valid handles may be safely resolved. When a handle is invalidated, all its copies are also invalidated at the
/// same time, and all pointers resolved from the handle or any of its copies are invalidated as well.
///
/// Handle Invalidation:
///
/// -   A handle is immediately invalidated when used as an argument to the `Store::deallocate` method.
/// -   A handle is invalidated when used as an argument to the `Store::grow`, `Store::grow_zeroed`, or
///     `Store::shrink` and these methods succeed.
///
/// Handle Conversion:
///
/// -   If a handle can be converted into a `NonNull<u8>`, for example via `Into`, or `TryInto`, and the handle is
///     valid, possibly dangling, then the resulting pointer must be equal to the result of `Store::resolve` and
///     obtaining this pointer must NOT invalidate any other handle.
/// -   If a handle can be created from a `NonNull<u8>`, for example via `From`, or `TryFrom`, then the resulting handle
///     is a copy of the handle which resolved into the `NonNull<u8>` in the first place.
///
/// Pointer Invalidation:
///
/// -   All pointers resolved by an instance of `Store` may be invalidated when dropping this instance of `Store`.
/// -   All pointers resolved by an instance of `Store` may be invalidated when moving this instance of `Store`.
///     Pointers are only guaranteed to remain valid across moves for instances also implementing `StorePinning`.
/// -   All pointers resolved by an instance of `Store` may be invalidated when calling `Store::allocate`,
///     `Store::deallocate`, `Store::grow`, `Store::shrink`, or their zeroed variants. Pointers are only
///     guaranteed to remain valid across those calls for instances also implementing `StoreStable`.
/// -   All pointers resolved by an instance of `Store` from a _different_ handle may be invalidated when calling
///     `Store::resolve`. Pointers from different handles are only guaranteed to remain valid across those calls for
///     instances also implementing `StoreStable`.
///
/// A specific implementation of `Store` may provide extended validity guarantees, and should implement the extended
/// guarantees traits when it does so.
#[const_trait]
pub unsafe trait Store: StoreDangling {
    /// Resolves the `handle` into a pointer to the first byte of the associated block of memory.
    ///
    /// Unless `self` implements `StoreStable`, all previously resolved pointers from different handles may be
    /// invalidated.
    ///
    /// #   Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   The resulting pointer is only valid for as long as the `handle` is valid itself, and may be invalidated
    ///     sooner, see [Pointer Invalidation].
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8>;

    /// Attempts to allocate a block of memory.
    ///
    /// On success, returns a `Handle` to a block of memory meeting the size and alignment guarantees of `Layout` and
    /// actual size of the block of memory.
    ///
    /// Unless `self` implements `StoreStable`, all previously resolved pointers may be invalidated.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `layout`
    /// constraints.
    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError>;

    /// Deallocates the memory referenced by `handle`.
    ///
    /// This invalidates `handle` and all its copies, as well as all pointers resolved from `handle` or any of its
    /// copies.
    ///
    /// Unless `self` implements `StoreStable`, all previously resolved pointers may be invalidated.
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
    /// Unless `self` implements `StoreStable`, all previously resolved pointers may be invalidated.
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
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  FIXME(const): Add message when available in const contexts.
        debug_assert!(new_layout.size() >= old_layout.size());

        //  FIXME(const): Use `?` when available in const contexts.
        let Ok((new_handle, new_size)) = self.allocate(new_layout) else {
            return Err(AllocError);
        };

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `grow`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `grow`
        //      and has not been invalidated by `self.allocate` since `self` is a `StoreMultiple`.
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
        //      and has not been invalidated by `self.allocate` since `self` is a `StoreMultiple`.
        //  -   `old_layout` fits `handle`, as per the pre-conditions of `grow`.
        unsafe { self.deallocate(handle, old_layout) };

        Ok((new_handle, new_size))
    }

    /// Attempts to shrink the block of memory associated with `handle`.
    ///
    /// On success, returns a new `Self::Handle` associated with the extended block of memory, and may invalidate
    /// `handle` and all its copies, as well as all pointers resolved from `handle` or any of its copies.
    ///
    /// On failure, `handle` and all its copies are still valid, though any pointer resolved from `handle` or any of
    /// its copies may have been invalidated.
    ///
    /// Unless `self` implements `StoreStable`, all previously resolved pointers may be invalidated.
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
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  FIXME(const): Add message when available in const contexts.
        debug_assert!(new_layout.size() <= old_layout.size());

        //  FIXME(const): Use `?` when available in const contexts.
        let Ok((new_handle, new_size)) = self.allocate(new_layout) else {
            return Err(AllocError);
        };

        //  Safety:
        //  -   `handle` has been allocated by `self`, as per the pre-conditions of `shrink`.
        //  -   `handle` is valid, as it was valid at beginning of this function as per the pre-conditions of `shrink`
        //      and has not been invalidated by `self.allocate` since `self` is a `StoreMultiple`.
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
        //      and has not been invalidated by `self.allocate` since `self` is a `StoreMultiple`.
        //  -   `old_layout` fits `handle`, as per the pre-conditions of `shrink`.
        unsafe { self.deallocate(handle, old_layout) };

        Ok((new_handle, new_size))
    }

    /// Behaves like `allocate`, but also ensures that the associated block of memory is zero-initialized.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        let Ok((handle, size)) = self.allocate(layout) else {
            return Err(AllocError);
        };

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
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    unsafe fn grow_zeroed(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   All pre-conditions of `grow` are pre-conditions of `grow_zeroed`.
        let Ok((handle, new_size)) = (unsafe { self.grow(handle, old_layout, new_layout) }) else {
            return Err(AllocError);
        };

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

/// A trait abstracting a memory store specialized for a single outstanding allocation.
///
/// This trait returns handles to allocated memory, which can be freely copied and stored, then resolved into actual
/// pointers at a later time.
///
/// _Note: if required to have multiple outstanding allocations at a time, see `Store`._
///
/// #   Safety
///
/// Only valid handles may be safely resolved. When a handle is invalidated, all its copies are also invalidated at the
/// same time, and all pointers resolved from the handle or any of its copies are invalidated as well.
///
/// If a given instance implements both `Store` and `StoreSingle`, then its methods should behave identically, and the
/// following invalidation guarantees are loosened to match those of `Store` instead.
///
/// Handle Invalidation:
///
/// -   All handles allocated by an instance of `StoreSingle` are invalidated when calling `StoreSingle::allocate` or
///     `StoreSingle::allocate_zeroed` on this instance of `StoreSingle`.
/// -   A handle is immediately invalidated when used as an argument to the `StoreSingle::deallocate` method.
/// -   A handle is invalidated when used as an argument to the `StoreSingle::grow`, `StoreSingle::grow_zeroed`, or
///     `StoreSingle::shrink` and these methods succeed.
///
/// Handle Conversion:
///
/// -   If a handle can be converted into a `NonNull<u8>`, for example via `Into`, or `TryInto`, and the handle is
///     valid, possibly dangling, then the resulting pointer must be equal to the result of `StoreSingle::resolve` and
///     `StoreSingle::resolve_mut`.
/// -   If a handle can be created from a `NonNull<u8>`, for example via `From`, or `TryFrom`, then the resulting handle
///     is a copy of the handle which resolved into the `NonNull<u8>` in the first place.
///
/// Pointer Invalidation:
///
/// -   All pointers resolved by an instance of `StoreSingle` may be invalidated when dropping this instance of
///     `StoreSingle`.
/// -   All pointers resolved by an instance of `StoreSingle` may be invalidated when moving this instance of
///     `StoreSingle`.
///     Pointers are only guaranteed to remain valid across moves for instances also implementing `StorePinning`.
/// -   All pointers resolved by an instance of `StoreSingle` are invalidated when calling `StoreSingle::allocate`,
///     `StoreSingle::deallocate`, `StoreSingle::grow`, `StoreSingle::shrink`, or their zeroed variants.
///
/// A specific implementation of `StoreSingle` may provide extended validity guarantees, and should implement the
/// extended guarantees traits when it does so.
#[const_trait]
pub unsafe trait StoreSingle: StoreDangling {
    /// Resolves the `handle` into a pointer to the first byte of the associated block of memory.
    ///
    /// The resolved pointer may not be dereferenced mutably, unless `self` implements `Store` in which case both
    /// `StoreSingle::resolve` and `Store::resolve` must behave identically.
    ///
    /// _Note: see `resolve_mut` for mutably dereferenceable pointers._
    ///
    /// #   Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   The resulting pointer is only valid for as long as the `handle` is valid itself, and may be invalidated
    ///     sooner, see [Pointer Invalidation].
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8>;

    /// Resolves the `handle` into a pointer to the first byte of the associated block of memory.
    ///
    /// #   Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   The resulting pointer is only valid for as long as the `handle` is valid itself, and may be invalidated
    ///     sooner, see [Pointer Invalidation].
    unsafe fn resolve_mut(&mut self, handle: Self::Handle) -> NonNull<u8>;

    /// Attempts to allocate a block of memory.
    ///
    /// On success, returns a `Handle` to a block of memory meeting the size and alignment guarantees of `Layout` and
    /// actual size of the block of memory.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `layout`
    /// constraints.
    fn allocate(&mut self, layout: Layout) -> Result<(Self::Handle, usize), AllocError>;

    /// Deallocates the memory referenced by `handle`.
    ///
    /// This invalidates `handle` and all its copies, as well as all pointers resolved from `handle` or any of its
    /// copies.
    ///
    /// #   Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   `layout` must fit the associated block of memory.
    unsafe fn deallocate(&mut self, handle: Self::Handle, layout: Layout);

    /// Attempts to extend the block of memory associated with `handle`.
    ///
    /// On success, returns a new `Self::Handle` associated with the extended block of memory, and may invalidate
    /// `handle` and all its copies, as well as all pointers resolved from `handle` or any of its copies.
    ///
    /// On failure, `handle` and all its copies are still valid, though any pointer resolved from `handle` or any of
    /// its copies may have been invalidated.
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
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    unsafe fn grow(
        &mut self,
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
    /// #    Safety
    ///
    /// -   `handle` must have been allocated by `self`.
    /// -   `handle` must still be valid.
    /// -   `old_layout` must fit the associated block of memory.
    /// -   `new_layout.size()` must be smaller than or equal to `old_layout.size()`.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    unsafe fn shrink(
        &mut self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError>;

    /// Behaves like `allocate`, but also ensures that the associated block of memory is zero-initialized.
    ///
    /// #   Errors
    ///
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    fn allocate_zeroed(&mut self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        let Ok((handle, size)) = self.allocate(layout) else {
            return Err(AllocError);
        };

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
    /// Returning `Err` indicates that either the memory is exhausted, or the store cannot satisfy `new_layout`
    /// constraints.
    unsafe fn grow_zeroed(
        &mut self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        //  Safety:
        //  -   All pre-conditions of `grow` are pre-conditions of `grow_zeroed`.
        let Ok((handle, new_size)) = (unsafe { self.grow(handle, old_layout, new_layout) }) else {
            return Err(AllocError);
        };

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

/// A base for `Store` and `StoreSingle`, introducing the handle type, and the ability to allocate dangling handles.
///
/// This trait is separate from the main Store traits to allow `const StoreDangling` implementation even when the
/// Store implementations themselves cannot be `const`.
///
/// Note: ideally, `dangling` should be a `const` method of the Store traits.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that:
///
/// -   A dangling handle produced by this trait can be safely resolved by the matching `Store::resolve` implementation.
/// -   The resolved pointer of such an operation will always satisfy the specified alignment.
///
/// No guarantee is provided that the resolved pointer may be safely dereferenced, it may be invalid.
#[const_trait]
pub unsafe trait StoreDangling {
    /// A Handle to memory allocated by the instance of Store which creates it.
    type Handle: Copy;

    /// Creates a dangling handle.
    ///
    /// The only methods of a store which may be called with a dangling handle are the `resolve` and `resolve_mut`
    /// methods. The pointer so obtained is guaranteed to be at least aligned according to `alignment`, though it
    /// remains invalid and cannot be dereferenced.
    ///
    /// For all other purposes, a dangling handle is never valid, and thus cannot be deallocated, grown, nor shrunk...
    /// Furthermore there is no explicit way to distinguish whether a handle is dangling, or not. It is up to the user
    /// to remember whether a given handle is dangling, valid, or used to be valid but was invalidated.
    fn dangling(&self, alignment: Alignment) -> Result<Self::Handle, AllocError>;
}

/// A refinement of a store which guarantees that the blocks of memory are stable in memory across method calls, but
/// not necessarily across moves.
///
/// If the blocks of memory should be stable in memory across moves as well, then `StorePinning` is required.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that a handle always resolve to the same block of memory for as long as
/// it is valid and the instance of the store has not moved.
pub unsafe trait StoreStable {}

/// A refinement of a store which guarantees that the blocks of memory are pinned in memory.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that a handle always resolve to the same block of memory for as long as
/// it is valid, in particular even after the instance of the store was moved.
///
/// As a corrolary, forgetting the instance of a store -- which is moving without dropping -- means that the resolved
/// pointers will remain pinned until either the instance of the store is recovered (from scratch) and dropped, or until
/// the lifetime bound of the `Store` concrete type (if not `'static`) expires, whichever comes first.
pub unsafe trait StorePinning: StoreStable {}

/// A refinement of `StorePinning` which allows multiple instances to share the handles and their associated blocks of
/// memory.
///
/// Normally, a handle created by one instance of `Store` cannot be used in any way with another, different, instance of
/// `Store`. This trait lifts this restriction _partly_ by created sets of sharing stores. In essence, all stores
/// belonging to the same set of sharing stores can be considered "parts" of a single store: all handles created by one
/// "part" can be used with any other "part", and the store is not dropped until all its "parts" are dropped.
///
/// A set of sharing stores is effectively the morale equivalent of a `Rc<Store>` or `Arc<Store>`.
///
/// #   Safety
///
/// Implementers of this trait must guarantee that a handle created by one part of a sharing set may be used with any
/// other part: resolved, deallocated, grown, or shrunk.
pub unsafe trait StoreSharing: StorePinning {
    /// Error returned if sharing is not currently possible.
    type SharingError;

    /// Returns whether two instances belong to the same sharing set.
    ///
    /// The implementation is permitted to return `false` even if the two instances do, indeed, belong to the same
    /// sharing set. This method is only meant to allow users who lost track of whether the implementations are sharing
    /// to possibly recover this piece of information.
    fn is_sharing_with(&self, other: &Self) -> bool;

    /// Creates a new instance of `Store` belonging to the same sharing set as `self`.
    fn share(&self) -> Result<Self, Self::SharingError>
    where
        Self: Sized;
}
