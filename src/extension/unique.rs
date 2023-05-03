//! A typed, unique handle.

use core::{alloc::AllocError, marker::Unsize, ptr::NonNull};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{
    extension::{typed::TypedHandle, typed_metadata::TypedMetadata},
    interface::Storage,
};

/// A typed, unique handle.
pub struct UniqueHandle<T: ?Sized, H>(TypedHandle<T, H>);

impl<T, H: Copy> UniqueHandle<T, H> {
    /// Creates a dangling handle.
    #[inline(always)]
    pub fn dangling<S>() -> Self
    where
        S: Storage<Handle = H>,
    {
        Self(TypedHandle::dangling::<S>())
    }

    /// Creates a new handle, pointing to a `T`.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles of `storage`.
    #[inline(always)]
    pub fn new<S>(value: T, storage: &S) -> Result<Self, AllocError>
    where
        S: Storage<Handle = H>,
    {
        TypedHandle::new(value, storage).map(Self)
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles of `storage`.
    #[inline(always)]
    pub fn allocate<S>(storage: &S) -> Result<Self, AllocError>
    where
        S: Storage<Handle = H>,
    {
        TypedHandle::allocate(storage).map(Self)
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles of `storage`.
    #[inline(always)]
    pub fn allocate_zeroed<S>(storage: &S) -> Result<Self, AllocError>
    where
        S: Storage<Handle = H>,
    {
        TypedHandle::allocate_zeroed(storage).map(Self)
    }
}

impl<T: ?Sized, H: Copy> UniqueHandle<T, H> {
    /// Creates a handle from raw parts.
    ///
    /// -   If `handle` is valid, and associated to a block of memory which fits an instance of `T`, then the resulting
    ///     typed handle is valid.
    /// -   If `handle` is invalid, then the resulting typed handle is invalid.
    /// -   If `handle` is valid and `metadata` does not fit the block of memory associated with it, then the resulting
    ///     typed handle is invalid.
    ///
    /// #   Safety
    ///
    /// -   No copy of `handle` must be used henceforth.
    pub unsafe fn from_raw_parts(handle: H, metadata: TypedMetadata<T>) -> Self {
        Self(TypedHandle::from_raw_parts(handle, metadata))
    }

    /// Decomposes a (possibly wide) pointer into its handle and metadata components.
    pub fn to_raw_parts(self) -> (H, TypedMetadata<T>) {
        self.0.to_raw_parts()
    }

    /// Deallocates the memory associated with the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    #[inline(always)]
    pub unsafe fn deallocate<S>(self, storage: &S)
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `storage`, as per pre-conditions.
        //  -   `self.0` is valid, as per pre-conditions.
        unsafe { self.0.deallocate(storage) }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid. Most notably, unless `storage`
    ///     implements `MultipleStorage` allocating from `storage` will invalidate it.
    #[inline(always)]
    pub unsafe fn resolve<'a, S>(&'a self, storage: &'a S) -> &'a T
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { self.resolve_raw(storage) };

        //  Safety:
        //  -   `pointer` points to a live instance of `T`, as per type-invariant.
        //  -   The resulting reference borrows `self` immutably, guaranteeing that no mutable reference exist, nor can
        //      be creating during its lifetime.
        //  -   The resulting reference borrows `storage` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying storage, though it may still be invalidated by allocating.
        unsafe { pointer.as_ref() }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid. Most notably, unless `storage`
    ///     implements `MultipleStorage` allocating from `storage` will invalidate it.
    #[inline(always)]
    pub unsafe fn resolve_mut<'a, S>(&'a mut self, storage: &'a S) -> &'a mut T
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let mut pointer = unsafe { self.resolve_raw(storage) };

        //  Safety:
        //  -   `pointer` points to a live instance of `T`, as per type-invariant.
        //  -   The resulting reference borrows `self` mutably, guaranteeing that no reference exist, nor can be
        //      created during its lifetime.
        //  -   The resulting reference borrows `storage` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying storage, though it may still be invalidated by allocating.
        unsafe { pointer.as_mut() }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   The pointer is only guaranteed to be valid as long as `self` is valid and `storage` is not moved. Most
    ///     notably, unless `storage` implements `MultipleStorage` allocating from `storage` will invalidate it.
    #[inline(always)]
    pub unsafe fn resolve_raw<S>(&self, storage: &S) -> NonNull<T>
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        unsafe { self.0.resolve_raw(storage) }
    }

    /// Coerces the handle into another.
    #[inline(always)]
    pub fn coerce<U: ?Sized>(self) -> UniqueHandle<U, H>
    where
        T: Unsize<U>,
    {
        UniqueHandle(self.0.coerce())
    }
}

impl<T, H: Copy> UniqueHandle<[T], H> {
    /// Returns whether the memory area associated to `self` may not contain any element.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of elements the memory area associated to `self` may contain.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, the extra memory is left uninitialized. On failure, an error is returned.
    ///
    /// #   Safety:
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub unsafe fn grow<S>(&mut self, new_size: usize, storage: &S) -> Result<(), AllocError>
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `storage`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is greater than or equal to `self.0.len()`.
        unsafe { self.0.grow(new_size, storage) }
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, the extra memory is zeroed. On failure, an error is returned.
    ///
    /// #   Safety:
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub unsafe fn grow_zeroed<S>(&mut self, new_size: usize, storage: &S) -> Result<(), AllocError>
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `storage`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is greater than or equal to `self.0.len()`.
        unsafe { self.0.grow_zeroed(new_size, storage) }
    }

    /// Shrinks the block of memory associated with the handle.
    ///
    /// On failure, an error is returned.
    ///
    /// #   Safety:
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be less than or equal to `self.len()`.
    pub unsafe fn shrink<S>(&mut self, new_size: usize, storage: &S) -> Result<(), AllocError>
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `storage`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is less than or equal to `self.0.len()`.
        unsafe { self.0.shrink(new_size, storage) }
    }
}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, H: Copy> CoerceUnsized<UniqueHandle<U, H>> for UniqueHandle<T, H> where T: Unsize<U> {}
