//! Typed handle, for bonus type safety.

use core::{
    alloc::{AllocError, Layout},
    marker::Unsize,
    mem,
    ptr::{self, Alignment, NonNull},
};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{
    alloc,
    extension::typed_metadata::TypedMetadata,
    interface::{StoreDangling, StoreSingle},
};

/// Arbitrary typed handle, for type safety, and coercion.
///
/// A typed handle may be dangling, or may be invalid. It is the responsibility of the user to ensure that the typed
/// handle is valid when necessary.
pub struct TypedSingleHandle<T: ?Sized, H> {
    handle: H,
    metadata: TypedMetadata<T>,
}

impl<T, H: Copy> TypedSingleHandle<T, H> {
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
            return Err(AllocError);
        };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }

    /// Creates a new handle, pointing to a `T`.
    #[inline(always)]
    pub fn new<S>(value: T, store: &mut S) -> Self
    where
        S: StoreSingle<Handle = H>,
    {
        let Ok(this) = Self::try_new(value, store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to create a new handle, pointing to a `T`.
    #[inline(always)]
    pub fn try_new<S>(value: T, store: &mut S) -> Result<Self, AllocError>
    where
        S: StoreSingle<Handle = H>,
    {
        let (handle, _) = store.allocate(Layout::new::<T>())?;

        //  Safety:
        //  -   `handle` was just allocated by `store`.
        //  -   `handle` is still valid, as no other operation occurred on `store`.
        let pointer = unsafe { store.resolve_mut(handle) };

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
    #[inline(always)]
    pub const fn allocate<S>(store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H>,
    {
        let Ok(this) = Self::try_allocate(store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to allocate a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn try_allocate<S>(store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        let Ok((handle, _)) = store.allocate(Layout::new::<T>()) else {
            return Err(AllocError);
        };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn allocate_zeroed<S>(store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H>,
    {
        let Ok(this) = Self::try_allocate_zeroed(store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to allocate a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn try_allocate_zeroed<S>(store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H>,
    {
        let Ok((handle, _)) = store.allocate_zeroed(Layout::new::<T>()) else {
            return Err(AllocError);
        };

        let metadata = TypedMetadata::new();

        Ok(Self { handle, metadata })
    }
}

impl<T: ?Sized, H: Copy> TypedSingleHandle<T, H> {
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
    pub const unsafe fn deallocate<S>(&self, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { self.resolve_raw_mut(store) };

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
    /// -   The reference is only guaranteed to be valid as long as `self` is valid.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    pub const unsafe fn resolve<'a, S>(&self, store: &'a S) -> &'a T
    where
        S: ~const StoreSingle<Handle = H>,
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
    /// -   The reference is only guaranteed to be valid as long as `self` is valid.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StoreStable`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub const unsafe fn resolve_mut<'a, S>(&mut self, store: &'a mut S) -> &'a mut T
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let mut pointer = unsafe { self.resolve_raw_mut(store) };

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
    /// -   The pointer is only guaranteed to be dereferenceable to a shared reference.
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
        let pointer = unsafe { store.resolve(self.handle) };

        NonNull::from_raw_parts(pointer.cast(), self.metadata.get())
    }

    /// Resolves the handle to a non-null pointer.
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
        let pointer = unsafe { store.resolve_mut(self.handle) };

        NonNull::from_raw_parts(pointer.cast(), self.metadata.get())
    }

    /// Coerces the handle into another.
    ///
    /// If `self` is valid, the resulting typed handle is valid; otherwise it is invalid.
    #[inline(always)]
    pub const fn coerce<U: ?Sized>(&self) -> TypedSingleHandle<U, H>
    where
        T: Unsize<U>,
    {
        let metadata = self.metadata.coerce();

        TypedSingleHandle {
            handle: self.handle,
            metadata,
        }
    }
}

impl<T, H: Copy> TypedSingleHandle<[T], H> {
    /// Creates a dangling handle.
    ///
    /// Calls `handle_alloc_error` if the creation of the handle fails.
    #[inline(always)]
    pub const fn dangling_slice<S>(store: &S) -> Self
    where
        S: ~const StoreDangling<Handle = H>,
    {
        let Ok(this) = Self::try_dangling_slice(store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to create a dangling handle.
    ///
    /// Returns `AllocError` on failure.
    #[inline(always)]
    pub const fn try_dangling_slice<S>(store: &S) -> Result<Self, AllocError>
    where
        S: ~const StoreDangling<Handle = H>,
    {
        let Ok(handle) = store.dangling(Alignment::of::<T>()) else {
            return Err(AllocError);
        };

        let metadata = TypedMetadata::from_metadata(0);

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn allocate_slice<S>(size: usize, store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        let Ok(this) = Self::try_allocate_slice(size, store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to allocate a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is left uninitialized.
    #[inline(always)]
    pub const fn try_allocate_slice<S>(size: usize, store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        if mem::size_of::<T>() == 0 {
            let Ok(mut this) = Self::try_dangling_slice(store) else {
                alloc::handle_alloc_error(Layout::new::<T>())
            };

            this.metadata = TypedMetadata::from_metadata(usize::MAX);

            return Ok(this);
        }

        let Ok(layout) = Self::layout(size) else {
            return Err(AllocError);
        };

        let Ok((handle, bytes)) = store.allocate(layout) else {
            return Err(AllocError);
        };

        debug_assert!(bytes >= layout.size());

        let metadata = TypedMetadata::from_metadata(bytes / mem::size_of::<T>());

        Ok(Self { handle, metadata })
    }

    /// Allocates a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn allocate_zeroed_slice<S>(size: usize, store: &mut S) -> Self
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        let Ok(this) = Self::try_allocate_zeroed_slice(size, store) else {
            alloc::handle_alloc_error(Layout::new::<T>())
        };

        this
    }

    /// Attempts to allocate a new handle, with enough space for `size` elements `T`.
    ///
    /// The allocated memory is zeroed out.
    #[inline(always)]
    pub const fn try_allocate_zeroed_slice<S>(size: usize, store: &mut S) -> Result<Self, AllocError>
    where
        S: ~const StoreSingle<Handle = H> + ~const StoreDangling<Handle = H>,
    {
        if mem::size_of::<T>() == 0 {
            let Ok(mut this) = Self::try_dangling_slice(store) else {
                alloc::handle_alloc_error(Layout::new::<T>())
            };

            this.metadata = TypedMetadata::from_metadata(usize::MAX);

            return Ok(this);
        }

        let Ok(layout) = Self::layout(size) else {
            return Err(AllocError);
        };

        let Ok((handle, bytes)) = store.allocate_zeroed(layout) else {
            return Err(AllocError);
        };

        debug_assert!(bytes >= layout.size());

        let metadata = TypedMetadata::from_metadata(bytes / mem::size_of::<T>());

        Ok(Self { handle, metadata })
    }

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
    pub const unsafe fn grow<S>(&mut self, new_size: usize, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self` has been allocated by `store`, as per pre-conditions.
        //  -   `self` is still valid, as per pre-conditions.
        //  -   `new_size` must be greater than or equal to `self.len()`, as per pre-conditions.
        let result = unsafe { self.try_grow(new_size, store) };

        if result.is_err() {
            alloc::handle_alloc_error(Layout::new::<T>())
        }
    }

    /// Attempts to grow the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated, and the extra memory is left uninitialized. On
    /// failure, an error is returned.
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
        debug_assert!(new_size >= self.len());

        let Ok(old_layout) = Self::layout(self.len()) else {
            return Err(AllocError);
        };

        let Ok(new_layout) = Self::layout(new_size) else {
            return Err(AllocError);
        };

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        let result = unsafe { store.grow(self.handle, old_layout, new_layout) };

        let Ok((handle, bytes)) = result else {
            return Err(AllocError);
        };

        debug_assert!(bytes >= new_layout.size());

        self.handle = handle;
        self.metadata = TypedMetadata::from_metadata(bytes / mem::size_of::<T>());

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
    pub const unsafe fn grow_zeroed<S>(&mut self, new_size: usize, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self` has been allocated by `store`, as per pre-conditions.
        //  -   `self` is still valid, as per pre-conditions.
        //  -   `new_size` must be greater than or equal to `self.len()`, as per pre-conditions.
        let result = unsafe { self.try_grow_zeroed(new_size, store) };

        if result.is_err() {
            alloc::handle_alloc_error(Layout::new::<T>())
        }
    }

    /// Attempts to grow the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated, and the extra memory is zeroed. On failure, an error
    /// is returned.
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
        debug_assert!(new_size >= self.len());

        let Ok(old_layout) = Self::layout(self.len()) else {
            return Err(AllocError);
        };

        let Ok(new_layout) = Self::layout(new_size) else {
            return Err(AllocError);
        };

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is greater than or equal to the size of `old_layout`, as per pre-conditions.
        let result = unsafe { store.grow_zeroed(self.handle, old_layout, new_layout) };

        let Ok((handle, bytes)) = result else {
            return Err(AllocError);
        };

        debug_assert!(bytes >= new_layout.size());

        self.handle = handle;
        self.metadata = TypedMetadata::from_metadata(bytes / mem::size_of::<T>());

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
    pub const unsafe fn shrink<S>(&mut self, new_size: usize, store: &mut S)
    where
        S: ~const StoreSingle<Handle = H>,
    {
        //  Safety:
        //  -   `self` has been allocated by `store`, as per pre-conditions.
        //  -   `self` is still valid, as per pre-conditions.
        //  -   `new_size` must be less than or equal to `self.len()`, as per pre-conditions.
        let result = unsafe { self.try_shrink(new_size, store) };

        if result.is_err() {
            alloc::handle_alloc_error(Layout::new::<T>())
        }
    }

    /// Attempts to shrink the block of memory associated with the handle.
    ///
    /// On success, all the copies of the handle are invalidated. On failure, an error is returned.
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
        debug_assert!(new_size <= self.len());

        if mem::size_of::<T>() == 0 {
            return Ok(());
        }

        let Ok(old_layout) = Self::layout(self.len()) else {
            return Err(AllocError);
        };

        let Ok(new_layout) = Self::layout(new_size) else {
            return Err(AllocError);
        };

        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        //  -   `old_layout` fits the block of memory associated to `self.handle`, by construction.
        //  -   `new_layout`'s size is less than or equal to the size of `old_layout`, as per pre-conditions.
        let result = unsafe { store.shrink(self.handle, old_layout, new_layout) };

        let Ok((handle, bytes)) = result else {
            return Err(AllocError);
        };

        debug_assert!(bytes >= new_layout.size());

        self.handle = handle;
        self.metadata = TypedMetadata::from_metadata(bytes / mem::size_of::<T>());

        Ok(())
    }
}

impl<T: ?Sized, H: Copy> Clone for TypedSingleHandle<T, H> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized, H: Copy> Copy for TypedSingleHandle<T, H> {}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, H: Copy> CoerceUnsized<TypedSingleHandle<U, H>> for TypedSingleHandle<T, H> where T: Unsize<U> {}

//
//  Implementation
//

impl<T, H> TypedSingleHandle<[T], H> {
    const fn layout(size: usize) -> Result<Layout, AllocError> {
        let Some(size) = mem::size_of::<T>().checked_mul(size) else {
            return Err(AllocError);
        };

        let align = mem::align_of::<T>();

        let Ok(layout) = Layout::from_size_align(size, align) else {
            return Err(AllocError);
        };

        Ok(layout)
    }
}
