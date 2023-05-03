//! Typed hande, for bonus type safety.

use core::{
    alloc::{AllocError, Layout},
    marker::Unsize,
    ptr::{self, NonNull},
};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{extension::typed_metadata::TypedMetadata, interface::Storage};

/// Arbitrary typed handle, for type safety, and coercion.
///
/// A typed handle may be invalid, either because it was created dangling, or because it became invalid following an
/// operation on the storage that allocated it. It is the responsibility of the user to ensure that the typed handle
/// is valid when necessary.
pub struct TypedHandle<T: ?Sized, H> {
    handle: H,
    metadata: TypedMetadata<T>,
}

impl<T, H: Copy> TypedHandle<T, H> {
    /// Creates a dangling handle.
    #[inline(always)]
    pub fn dangling<S>() -> Self
    where
        S: Storage<Handle = H>,
    {
        let handle = S::dangling();
        let metadata = TypedMetadata::default();

        Self { handle, metadata }
    }

    /// Creates a new handle, pointing to a `T`.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles of `storage`.
    #[inline(always)]
    pub fn new<S>(value: T, storage: &S) -> Result<Self, AllocError>
    where
        S: Storage<Handle = H>,
    {
        let handle = storage.allocate(Layout::new::<T>())?;

        //  Safety:
        //  -   `handle` was just allocated by `storage`.
        //  -   `handle` is still valid, as no other operation occurred on `storage`.
        let pointer = unsafe { storage.resolve(handle) };

        //  Safety:
        //  -   `pointer` points to writeable memory area.
        //  -   `pointer` points to a sufficiently aligned and sized memory area.
        //  -   `pointer` has exclusive access to the memory area it points to.
        unsafe { ptr::write(pointer.cast().as_ptr(), value) };

        let metadata = TypedMetadata::default();

        Ok(Self { handle, metadata })
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
        let handle = storage.allocate(Layout::new::<T>())?;
        let metadata = TypedMetadata::default();

        Ok(Self { handle, metadata })
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
        let handle = storage.allocate_zeroed(Layout::new::<T>())?;
        let metadata = TypedMetadata::default();

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
    pub fn from_raw_parts(handle: H, metadata: TypedMetadata<T>) -> Self {
        Self { handle, metadata }
    }

    /// Decomposes a (possibly wide) pointer into its (raw) handle and metadata components.
    pub fn to_raw_parts(self) -> (H, TypedMetadata<T>) {
        (self.handle, self.metadata)
    }

    /// Deallocates the memory associated with the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `self` is invalidated alongside any copy of it.
    #[inline(always)]
    pub unsafe fn deallocate<S>(&self, storage: &S)
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { self.resolve_raw(storage) };

        //  Safety:
        //  -   `pointer` has valid metadata for `T`.
        let layout = unsafe { Layout::for_value_raw(pointer.as_ptr() as *const T) };

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `layout` fits the block of memory associated with `self.handle`.
        unsafe { storage.deallocate(self.handle, layout) };
    }

    /// Resolves the handle to a reference.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   No access through a mutable reference to this instance of `T` must overlap with accesses through the result.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid. Most notably, unless `storage`
    ///     implements `MultipleStorage` allocating from `storage` will invalidate it.
    #[inline(always)]
    pub unsafe fn resolve<'a, S>(&self, storage: &'a S) -> &'a T
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { self.resolve_raw(storage) };

        //  Safety:
        //  -   `pointer` points to a live instance of `T`, as per type-invariant.
        //  -   The resulting reference borrows `storage` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying storage, though it may still be invalidated by allocating.
        unsafe { pointer.as_ref() }
    }

    /// Resolves the handle to a reference.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    /// -   No access through any reference to this instance of `T` must overlap with accesses through the result.
    /// -   The reference is only guaranteed to be valid as long as `self` is valid. Most notably, unless `storage`
    ///     implements `MultipleStorage` allocating from `storage` will invalidate it.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn resolve_mut<'a, S>(&mut self, storage: &'a S) -> &'a mut T
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let mut pointer = unsafe { self.resolve_raw(storage) };

        //  Safety:
        //  -   `pointer` points to a live instance of `T`, as per type-invariant.
        //  -   The resulting reference borrows `storage` immutably, guaranteeing it won't be invalidated by moving
        //      or destroying storage, though it may still be invalidated by allocating.
        unsafe { pointer.as_mut() }
    }

    /// Resolves the handle to a non-null pointer.
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
        let pointer = unsafe { storage.resolve(self.handle) };

        NonNull::from_raw_parts(pointer.cast(), self.metadata.get())
    }

    /// Coerces the handle into another.
    ///
    /// If `self` is valid, the resulting typed handle is valid; otherwise it is invalid.
    #[inline(always)]
    pub fn coerce<U: ?Sized>(&self) -> TypedHandle<U, H>
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
    pub fn is_empty(&self) -> bool {
        self.metadata.get() == 0
    }

    /// Returns the number of elements the memory area associated to `self` may contain.
    pub fn len(&self) -> usize {
        self.metadata.get()
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated, and the extra memory is left uninitialized. On
    /// failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub unsafe fn grow<S>(&mut self, new_size: usize, storage: &S) -> Result<(), AllocError>
    where
        S: Storage<Handle = H>,
    {
        debug_assert!(new_size >= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        self.handle = unsafe { storage.grow(self.handle, old_layout, new_layout)? };

        self.metadata = TypedMetadata::new(new_size);

        Ok(())
    }

    /// Grows the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated, and the extra memory is zeroed. On failure, an error
    /// is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be greater than or equal to `self.len()`.
    pub unsafe fn grow_zeroed<S>(&mut self, new_size: usize, storage: &S) -> Result<(), AllocError>
    where
        S: Storage<Handle = H>,
    {
        debug_assert!(new_size >= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        self.handle = unsafe { storage.grow_zeroed(self.handle, old_layout, new_layout)? };

        self.metadata = TypedMetadata::new(new_size);

        Ok(())
    }

    /// Shrinks the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated. On failure, an error is returned.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `new_size` must be less than or equal to `self.len()`.
    pub unsafe fn shrink<S>(&mut self, new_size: usize, storage: &S) -> Result<(), AllocError>
    where
        S: Storage<Handle = H>,
    {
        debug_assert!(new_size <= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is less than or equal to the size of `old_layout`, as per pre-conditions.
        self.handle = unsafe { storage.shrink(self.handle, old_layout, new_layout)? };

        self.metadata = TypedMetadata::new(new_size);

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
