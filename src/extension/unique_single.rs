//! A typed, unique handle.

use core::{alloc::AllocError, marker::Unsize, ptr::NonNull};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{
    extension::{typed_metadata::TypedMetadata, typed_single::TypedSingleHandle},
    interface::{StoreDangling, StoreSingle},
};

/// A typed, unique handle.
pub struct UniqueSingleHandle<T: ?Sized, H>(TypedSingleHandle<T, H>);

impl<T, H: Copy> UniqueSingleHandle<T, H> {
    /// Creates a dangling handle.
    ///
    /// Calls `handle_alloc_error` on allocation failure.
    #[inline(always)]
    pub const fn dangling<S>(store: &S) -> Self
    where
        S: ~const StoreDangling<Handle = H>,
    {
        Self(TypedSingleHandle::dangling(store))
    }

    /// Attempts to create a dangling handle.
    ///
    /// Returns an error on allocation failure.
    #[inline(always)]
    pub const fn try_dangling<S>(store: &S) -> Result<Self, AllocError>
    where
        S: ~const StoreDangling<Handle = H>,
    {
        let Ok(handle) = TypedSingleHandle::try_dangling(store) else {
            return Err(AllocError);
        };

        Ok(Self(handle))
    }

    /// Creates a new handle, pointing to a `T`.
    #[inline(always)]
    pub fn new<S>(value: T, store: &mut S) -> Self
    where
        S: StoreSingle<Handle = H>,
    {
        Self(TypedSingleHandle::new(value, store))
    }

    /// Attempts to create a new handle, pointing to a `T`.
    #[inline(always)]
    pub fn try_new<S>(value: T, store: &mut S) -> Result<Self, AllocError>
    where
        S: StoreSingle<Handle = H>,
    {
        TypedSingleHandle::try_new(value, store).map(Self)
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn allocate<S>(store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H>,
    {
        Self(TypedSingleHandle::allocate(store))
    }

    /// Attempts to allocate a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn try_allocate<S>(store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        let Ok(handle) = TypedSingleHandle::try_allocate(store) else {
            return Err(AllocError);
        };

        Ok(Self(handle))
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn allocate_zeroed<S>(store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H>,
    {
        Self(TypedSingleHandle::allocate_zeroed(store))
    }

    /// Attempts to allocate a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn try_allocate_zeroed<S>(store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        let Ok(handle) = TypedSingleHandle::try_allocate_zeroed(store) else {
            return Err(AllocError);
        };

        Ok(Self(handle))
    }
}

impl<T: ?Sized, H: Copy> UniqueSingleHandle<T, H> {
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
    pub const unsafe fn from_raw_parts(handle: H, metadata: TypedMetadata<T>) -> Self {
        Self(TypedSingleHandle::from_raw_parts(handle, metadata))
    }

    /// Decomposes a (possibly wide) pointer into its handle and metadata components.
    pub const fn to_raw_parts(self) -> (H, TypedMetadata<T>) {
        self.0.to_raw_parts()
    }

    /// Deallocates the memory associated with the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    #[inline(always)]
    pub const unsafe fn deallocate<S>(self, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is valid, as per pre-conditions.
        unsafe { self.0.deallocate(store) }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    pub const unsafe fn resolve<'a, S>(&'a self, store: &'a S) -> &'a T
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `self.handle` is associated with a block of memory containing a live instance of `T`, as per
        //      pre-conditions.
        //  -   The resulting reference borrows `self` immutably, guaranteeing that no mutable reference exist, nor can
        //      be creating during its lifetime.
        //  -   The resulting reference borrows `store` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying store, though it may still be invalidated by allocating.
        unsafe { self.0.resolve(store) }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    pub const unsafe fn resolve_mut<'a, S>(&'a mut self, store: &'a mut S) -> &'a mut T
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `self.handle` is associated with a block of memory containing a live instance of `T`, as per
        //      pre-conditions.
        //  -   The resulting reference borrows `self` mutably, guaranteeing that no reference exist, nor can be
        //      created during its lifetime.
        //  -   The resulting reference borrows `store` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying store, though it may still be invalidated by allocating.
        unsafe { self.0.resolve_mut(store) }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   The pointer is only guaranteed to be dereferenceable into a shared reference.
    /// -   The pointer is only guaranteed to be valid as long as `self` is valid.
    /// -   The pointer is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the pointer.
    #[inline(always)]
    pub const unsafe fn resolve_raw<S>(&self, store: &S) -> NonNull<T>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        unsafe { self.0.resolve_raw(store) }
    }

    /// Resolves the handle to a reference, borrowing the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   The pointer is only guaranteed to be valid as long as `self` is valid.
    /// -   The pointer is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the pointer.
    #[inline(always)]
    pub const unsafe fn resolve_raw_mut<S>(&self, store: &mut S) -> NonNull<T>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        unsafe { self.0.resolve_raw_mut(store) }
    }

    /// Coerces the handle into another.
    #[inline(always)]
    pub const fn coerce<U: ?Sized>(self) -> UniqueSingleHandle<U, H>
    where
        T: Unsize<U>,
    {
        UniqueSingleHandle(self.0.coerce())
    }
}

impl<T, H: Copy> UniqueSingleHandle<[T], H> {
    /// Creates a dangling handle.
    ///
    /// Calls `handle_alloc_error` on allocation failure.
    #[inline(always)]
    pub const fn dangling_slice<S>(store: &S) -> Self
    where
        S: ~const StoreDangling<Handle = H>,
    {
        Self(TypedSingleHandle::dangling_slice(store))
    }

    /// Attempts to create a dangling handle.
    ///
    /// Returns an error on allocation failure.
    #[inline(always)]
    pub const fn try_dangling_slice<S>(store: &S) -> Result<Self, AllocError>
    where
        S: ~const StoreDangling<Handle = H>,
    {
        let Ok(handle) = TypedSingleHandle::try_dangling_slice(store) else {
            return Err(AllocError);
        };

        Ok(Self(handle))
    }

    /// Allocates a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn allocate_slice<S>(size: usize, store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        Self(TypedSingleHandle::allocate_slice(size, store))
    }

    /// Attempts to allocate a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn try_allocate_slice<S>(size: usize, store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        let Ok(handle) = TypedSingleHandle::try_allocate_slice(size, store) else {
            return Err(AllocError);
        };

        Ok(Self(handle))
    }

    /// Allocates a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn allocate_zeroed_slice<S>(size: usize, store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        Self(TypedSingleHandle::allocate_zeroed_slice(size, store))
    }

    /// Attempts to allocate a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn try_allocate_zeroed_slice<S>(size: usize, store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        let Ok(handle) = TypedSingleHandle::try_allocate_zeroed_slice(size, store) else {
            return Err(AllocError);
        };

        Ok(Self(handle))
    }

    /// Returns whether the memory area associated to `self` may not contain any element.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of elements the memory area associated to `self` may contain.
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, the extra memory is left uninitialized. On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub const unsafe fn grow<S>(&mut self, new_size: usize, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is greater than or equal to `self.0.len()`.
        unsafe { self.0.grow(new_size, store) }
    }

    /// Attempts to grow the block of memory associated with the handle.
    ///
    /// On success, the extra memory is left uninitialized. On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub const unsafe fn try_grow<S>(&mut self, new_size: usize, store: &mut S) -> Result<(), AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is greater than or equal to `self.0.len()`.
        unsafe { self.0.try_grow(new_size, store) }
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, the extra memory is zeroed. On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub const unsafe fn grow_zeroed<S>(&mut self, new_size: usize, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is greater than or equal to `self.0.len()`.
        unsafe { self.0.grow_zeroed(new_size, store) }
    }

    /// Attempts to grow the block of memory associated with the handle.
    ///
    /// On success, the extra memory is zeroed. On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub const unsafe fn try_grow_zeroed<S>(&mut self, new_size: usize, store: &mut S) -> Result<(), AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is greater than or equal to `self.0.len()`.
        unsafe { self.0.try_grow_zeroed(new_size, store) }
    }

    /// Shrinks the block of memory associated with the handle.
    ///
    /// On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be less than or equal to `self.len()`.
    pub const unsafe fn shrink<S>(&mut self, new_size: usize, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is less than or equal to `self.0.len()`.
        unsafe { self.0.shrink(new_size, store) }
    }

    /// Shrinks the block of memory associated with the handle.
    ///
    /// On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be less than or equal to `self.len()`.
    pub const unsafe fn try_shrink<S>(&mut self, new_size: usize, store: &mut S) -> Result<(), AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.0` has been allocated by `store`, as per pre-conditions.
        //  -   `self.0` is still valid, as per pre-conditions.
        //  -   `new_size` is less than or equal to `self.0.len()`.
        unsafe { self.0.try_shrink(new_size, store) }
    }
}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, H: Copy> CoerceUnsized<UniqueSingleHandle<U, H>> for UniqueSingleHandle<T, H> where T: Unsize<U> {}
