//! Typed handle specialized for Sized types.
//!
//! There are some type inference issues with TypedHandle related to the use of `Pointee::Metadata` that this type
//! doesn't run into.

use core::{
    alloc::{AllocError, Layout},
    marker::PhantomData,
    ptr::{self, NonNull},
};

use crate::interface::Store;

/// Arbitrary typed handle, for type safety, and coercion.
///
/// A typed handle may be invalid, either because it was created dangling, or because it became invalid following an
/// operation on the store that allocated it. It is the responsibility of the user to ensure that the typed handle
/// is valid when necessary.
pub struct SizedHandle<T, H> {
    handle: H,
    _marker: PhantomData<fn(T) -> T>,
}

impl<T, H: Copy> SizedHandle<T, H> {
    /// Creates a dangling handle.
    #[inline(always)]
    pub fn dangling<S>(store: &S) -> Self
    where
        S: Store<Handle = H>,
    {
        let handle = store.dangling();
        let _marker = PhantomData;

        Self { handle, _marker }
    }

    /// Creates a new handle, pointing to a `T`.
    ///
    /// Unless `store` implements `MultipleStore`, this invalidates all existing handles of `store`.
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

        let _marker = PhantomData;

        Ok(Self { handle, _marker })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is left uninitialized.
    ///
    /// Unless `store` implements `MultipleStore`, this invalidates all existing handles of `store`.
    #[inline(always)]
    pub fn allocate<S>(store: &S) -> Result<Self, AllocError>
    where
        S: Store<Handle = H>,
    {
        let (handle, _) = store.allocate(Layout::new::<T>())?;
        let _marker = PhantomData;

        Ok(Self { handle, _marker })
    }

    /// Allocates a new handle, with enough space for `T`.
    ///
    /// The allocated memory is zeroed out.
    ///
    /// Unless `store` implements `MultipleStore`, this invalidates all existing handles of `store`.
    #[inline(always)]
    pub fn allocate_zeroed<S>(store: &S) -> Result<Self, AllocError>
    where
        S: Store<Handle = H>,
    {
        let (handle, _) = store.allocate_zeroed(Layout::new::<T>())?;
        let _marker = PhantomData;

        Ok(Self { handle, _marker })
    }

    /// Deallocates the memory associated with the handle.
    ///
    /// #   Safety
    ///
    /// -   `self` must have been allocated by `store`.
    /// -   `self` must still be valid.
    /// -   `self` is invalidated alongside any copy of it.
    #[inline(always)]
    pub unsafe fn deallocate<S>(&self, store: &S)
    where
        S: Store<Handle = H>,
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
    ///     implements `MultipleStore` allocating from `store` will invalidate it.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StableStore`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    pub unsafe fn resolve<'a, S>(&self, store: &'a S) -> &'a T
    where
        S: Store<Handle = H>,
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
    ///     implements `MultipleStore` allocating from `store` will invalidate it.
    /// -   The reference is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StableStore`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the reference.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn resolve_mut<'a, S>(&mut self, store: &'a S) -> &'a mut T
    where
        S: Store<Handle = H>,
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
    ///     implements `MultipleStore` allocating from `store` will invalidate it.
    /// -   The pointer is only guaranteed to be valid as long as pointers resolved from `self` are not invalidated.
    ///     Most notably, unless `store` implements `StableStore`, any method call on `store`, including other
    ///     `resolve` calls, may invalidate the pointer.
    #[inline(always)]
    pub unsafe fn resolve_raw<S>(&self, store: &S) -> NonNull<T>
    where
        S: Store<Handle = H>,
    {
        //  Safety:
        //  -   `self.handle` was allocated by `store`, as per pre-conditions.
        //  -   `self.handle` is still valid, as per pre-conditions.
        let pointer = unsafe { store.resolve(self.handle) };

        pointer.cast()
    }
}

impl<T, H: Copy> Clone for SizedHandle<T, H> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, H: Copy> Copy for SizedHandle<T, H> {}
