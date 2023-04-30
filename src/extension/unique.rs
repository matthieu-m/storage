//! A typed, unique handle.

use core::{
    alloc::{AllocError, Layout},
    marker::Unsize,
    ptr::{self, NonNull, Pointee},
};

use crate::interface::Storage;

/// A typed, unique handle.
pub struct Unique<T: ?Sized, H> {
    handle: H,
    metadata: <T as Pointee>::Metadata,
}

impl<T, H: Copy> Unique<T, H> {
    /// Creates a dangling handle.
    #[inline(always)]
    pub fn dangling<S>() -> Self
    where
        S: Storage<Handle = H>,
    {
        let handle = S::dangling();
        let metadata = ();

        Self { handle, metadata }
    }

    /// Creates a new handle, pointing to a `T`.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles.
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

        let metadata = ();

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles.
    #[inline(always)]
    pub fn allocate<S>(storage: &S) -> Result<Self, AllocError>
    where
        S: Storage<Handle = H>,
    {
        let handle = storage.allocate(Layout::new::<T>())?;
        let metadata = ();

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    ///
    /// Unless `storage` implements `MultipleStorage`, this invalidates all existing handles.
    #[inline(always)]
    pub fn allocate_zeroed<S>(storage: &S) -> Result<Self, AllocError>
    where
        S: Storage<Handle = H>,
    {
        let handle = storage.allocate_zeroed(Layout::new::<T>())?;
        let metadata = ();

        Ok(Self { handle, metadata })
    }
}

impl<T: ?Sized, H: Copy> Unique<T, H> {
    /// Creates a handle from raw parts.
    ///
    /// #   Safety
    ///
    /// -   `handle` must be associated to a block of memory which fits an instance of `T`.
    /// -   `handle` must be the only handle to this block of memory.
    /// -   `metadata` must be the metadata of this instance of `T`.
    pub unsafe fn from_raw_parts(handle: H, metadata: <T as Pointee>::Metadata) -> Self {
        Self { handle, metadata }
    }

    /// Decomposes a (possibly wide) pointer into its handle and metadata components.
    pub fn to_raw_parts(self) -> (H, <T as Pointee>::Metadata) {
        (self.handle, self.metadata)
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
        let pointer = unsafe { storage.resolve(self.handle) };

        NonNull::from_raw_parts(pointer.cast(), self.metadata)
    }

    /// Coerces the handle into another.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `storage`.
    /// -   `self` must still be valid.
    /// -   `self` must be associated to a block of memory containing a valid instance of `T`.
    #[inline(always)]
    pub unsafe fn coerce<U, S>(self, storage: &S) -> Unique<U, H>
    where
        T: Unsize<U>,
        U: ?Sized,
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `self.handle` is associated to a block of memory containing a live instance of `T`, as per
        //      pre-conditions.
        let t = unsafe { self.resolve(storage) };

        let u: &U = t;

        let (_, metadata) = NonNull::to_raw_parts(NonNull::from(u));

        Unique {
            handle: self.handle,
            metadata,
        }
    }
}

impl<T, H: Copy> Unique<[T], H> {
    /// Returns whether the memory area associated to `self` may not contain any element.
    pub fn is_empty(&self) -> bool {
        self.metadata == 0
    }

    /// Returns the number of elements the memory area associated to `self` may contain.
    pub fn len(&self) -> usize {
        self.metadata
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
        debug_assert!(new_size >= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        self.handle = unsafe { storage.grow(self.handle, old_layout, new_layout)? };

        self.metadata = new_size;

        Ok(())
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
        debug_assert!(new_size >= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        self.handle = unsafe { storage.grow_zeroed(self.handle, old_layout, new_layout)? };

        self.metadata = new_size;

        Ok(())
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
        debug_assert!(new_size <= self.len());

        let (old_layout, _) = Layout::new::<T>().repeat(self.len()).map_err(|_| AllocError)?;
        let (new_layout, _) = Layout::new::<T>().repeat(new_size).map_err(|_| AllocError)?;

        //  Safety:
        //  -   `self.handle` was allocated by `storage`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is less than or equal to the size of `old_layout`, as per pre-conditions.
        self.handle = unsafe { storage.shrink(self.handle, old_layout, new_layout)? };

        self.metadata = new_size;

        Ok(())
    }
}
