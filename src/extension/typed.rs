//! Typed handle, for bonus type safety.

use core::{
    alloc::{AllocError, Layout},
    marker::Unsize,
    ptr::{self, Alignment, NonNull},
};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{
    alloc,
    extension::typed_metadata::TypedMetadata,
    interface::{Store, StoreDangling},
};

/// Arbitrary typed handle, for type safety, and coercion.
///
/// A typed handle may be dangling, or may be invalid. It is the responsibility of the user to ensure that the typed
/// handle is valid when necessary.
pub struct TypedHandle<T: ?Sized, H> {
    handle: H,
    metadata: TypedMetadata<T>,
}

impl<T, H: Copy> TypedHandle<T, H> {
    /// Creates a dangling handle.
    ///
    /// Calls `handle_alloc_error` if the creation of the handle fails.
    #[inline(always)]
    pub const fn dangling<S>(store: &S) -> Self
    where
        S: ~const StoreDangling<Handle = H>,
    {
        let Ok(this) = Self::try_dangling(store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to create a dangling handle.
    ///
    /// Returns `AllocError` on failure.
    #[inline(always)]
    pub const fn try_dangling<S>(store: &S) -> Result<Self, AllocError>
    where
        S: ~const StoreDangling<Handle = H>,
    {
        let Ok(handle) = store.dangling(Alignment::of::<T>()) else {
            return Err(AllocError)
        };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }

    /// Creates a new handle, pointing to a `T`.
    ///
    /// Unless `store` implements `StoreMultiple`, this invalidates all existing handles of `store`.
    #[inline(always)]
    pub fn new<S>(value: T, store: &S) -> Result<Self, AllocError>
    where
        S: Store<Handle = H>,
    {
        let (handle, _) = store.allocate(Layout::new::<T>())?;

        //  Safety:
        //  -   `handle` was just allocated by `store`.
        //  -   `handle` is still valid, as no other operation occurred on `store`.
        let pointer = unsafe { store.resolve(handle) };

        //  Safety:
        //  -   `pointer` points to writeable memory area.
        //  -   `pointer` points to a sufficiently aligned and sized memory area.
        //  -   `pointer` has exclusive access to the memory area it points to.
        unsafe { ptr::write(pointer.cast().as_ptr(), value) };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    ///
    /// Unless `store` implements `StoreMultiple`, this invalidates all existing handles of `store`.
    #[inline(always)]
    pub const fn allocate<S>(store: &S) -> Result<Self, AllocError>
    where
        S: ~const Store<Handle = H>,
    {
        let Ok((handle, _)) = store.allocate(Layout::new::<T>()) else {
            return Err(AllocError)
        };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    ///
    /// Unless `store` implements `StoreMultiple`, this invalidates all existing handles of `store`.
    #[inline(always)]
    pub const fn allocate_zeroed<S>(store: &S) -> Result<Self, AllocError>
    where
        S: ~const Store<Handle = H>,
    {
        let Ok((handle, _)) = store.allocate_zeroed(Layout::new::<T>()) else {
            return Err(AllocError)
        };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }
}

impl<T: ?Sized, H: Copy> TypedHandle<T, H> {
    /// Creates a handle from raw parts.
    ///
    /// -   If `handle` is valid, and associated to a block of memory which fits an instance of `T`, then the resulting
    ///     typed handle is valid.
    /// -   If `handle` is invalid, then the resulting typed handle is invalid.
    /// -   If `handle` is valid and `metadata` does not fit the block of memory associated with it, then the resulting
    ///     typed handle is invalid.
    pub const fn from_raw_parts(handle: H, metadata: TypedMetadata<T>) -> Self {
        Self { handle, metadata }
    }

    /// Decomposes a (possibly wide) pointer into its (raw) handle and metadata components.
    pub const fn to_raw_parts(self) -> (H, TypedMetadata<T>) {
        (self.handle, self.metadata)
    }

    /// Deallocates the memory associated with the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `self` is invalidated alongside any copy of it.
    #[inline(always)]
    pub const unsafe fn deallocate<S>(&self, store: &S)
    where
        S: ~const Store<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { self.resolve_raw(store) };

        //  Safety:
        //  -   `pointer` has valid metadata for `T`.
        let layout = unsafe { Layout::for_value_raw(pointer.as_ptr() as *const T) };

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `layout` fits the block of memory associated with `self.handle`.
        unsafe { store.deallocate(self.handle, layout) };
    }

    /// Resolves the handle to a reference.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   No access through a mutable reference to this instance of `T` must overlap with accesses through the result.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid. Most notably, unless `store`
    ///     implements `StoreMultiple` allocating from `store` will invalidate it.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    pub const unsafe fn resolve<'a, S>(&self, store: &'a S) -> &'a T
    where
        S: ~const Store<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { self.resolve_raw(store) };

        //  Safety:
        //  -   `pointer` points to a live instance of `T`, as per type-invariant.
        //  -   The resulting reference borrows `store` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying store, though it may still be invalidated by allocating.
        unsafe { pointer.as_ref() }
    }

    /// Resolves the handle to a reference.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   No access through any reference to this instance of `T` must overlap with accesses through the result.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid. Most notably, unless `store`
    ///     implements `StoreMultiple` allocating from `store` will invalidate it.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub const unsafe fn resolve_mut<'a, S>(&mut self, store: &'a S) -> &'a mut T
    where
        S: ~const Store<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let mut pointer = unsafe { self.resolve_raw(store) };

        //  Safety:
        //  -   `pointer` points to a live instance of `T`, as per type-invariant.
        //  -   The resulting reference borrows `store` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying store, though it may still be invalidated by allocating.
        unsafe { pointer.as_mut() }
    }

    /// Resolves the handle to a non-null pointer.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   The pointer is only guaranteed to be valid as long as `self` is valid. Most notably, unless `store`
    ///     implements `StoreMultiple` allocating from `store` will invalidate it.
    /// -   The pointer is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the pointer.
    #[inline(always)]
    pub const unsafe fn resolve_raw<S>(&self, store: &S) -> NonNull<T>
    where
        S: ~const Store<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { store.resolve(self.handle) };

        NonNull::from_raw_parts(pointer.cast(), self.metadata.get())
    }

    /// Coerces the handle into another.
    ///
    /// If `self` is valid, the resulting typed handle is valid; otherwise it is invalid.
    #[inline(always)]
    pub const fn coerce<U: ?Sized>(&self) -> TypedHandle<U, H>
    where
        T: Unsize<U>,
    {
        let metadata = self.metadata.coerce();

        TypedHandle {
            handle: self.handle,
            metadata,
        }
    }
}

impl<T, H: Copy> TypedHandle<[T], H> {
    /// Returns whether the memory area associated to `self` may not contain any element.
    pub const fn is_empty(&self) -> bool {
        self.metadata.get() == 0
    }

    /// Returns the number of elements the memory area associated to `self` may contain.
    pub const fn len(&self) -> usize {
        self.metadata.get()
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated, and the extra memory is left uninitialized. On
    /// failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub unsafe fn grow<S>(&mut self, new_size: usize, store: &S) -> Result<(), AllocError>
    where
        S: Store<Handle = H>,
    {
        debug_assert!(new_size >= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        let (handle, _) = unsafe { store.grow(self.handle, old_layout, new_layout)? };

        self.handle = handle;

        self.metadata = TypedMetadata::from_metadata(new_size);

        Ok(())
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated, and the extra memory is zeroed. On failure, an error
    /// is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub unsafe fn grow_zeroed<S>(&mut self, new_size: usize, store: &S) -> Result<(), AllocError>
    where
        S: Store<Handle = H>,
    {
        debug_assert!(new_size >= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        let (handle, _) = unsafe { store.grow_zeroed(self.handle, old_layout, new_layout)? };

        self.handle = handle;

        self.metadata = TypedMetadata::from_metadata(new_size);

        Ok(())
    }

    /// Shrinks the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated. On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be less than or equal to `self.len()`.
    pub unsafe fn shrink<S>(&mut self, new_size: usize, store: &S) -> Result<(), AllocError>
    where
        S: Store<Handle = H>,
    {
        debug_assert!(new_size <= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is less than or equal to the size of `old_layout`, as per pre-conditions.
        let (handle, _) = unsafe { store.shrink(self.handle, old_layout, new_layout)? };

        self.handle = handle;

        self.metadata = TypedMetadata::from_metadata(new_size);

        Ok(())
    }
}

impl<T: ?Sized, H: Copy> Clone for TypedHandle<T, H> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized, H: Copy> Copy for TypedHandle<T, H> {}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, H: Copy> CoerceUnsized<TypedHandle<U, H>> for TypedHandle<T, H> where T: Unsize<U> {}
