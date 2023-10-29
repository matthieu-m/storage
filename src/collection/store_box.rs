//! Proof-of-Concept implementation of a `Box` atop a `StoreSingle`.

use core::{
    alloc::AllocError,
    fmt,
    marker::Unsize,
    mem::{self, ManuallyDrop},
    ops, ptr,
};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{extension::unique_single::UniqueSingleHandle, interface::StoreSingle};

/// A `Box` atop a `StoreSingle`.
pub struct StoreBox<T: ?Sized, S: StoreSingle> {
    store: ManuallyDrop<S>,
    handle: UniqueSingleHandle<T, S::Handle>,
}

impl<T, S: StoreSingle + Default> StoreBox<T, S> {
    /// Creates a new instance.
    pub fn new(value: T) -> Self {
        Self::new_in(value, S::default())
    }
}

impl<T, S: StoreSingle> StoreBox<T, S> {
    /// Creates a new instance.
    pub fn new_in(value: T, mut store: S) -> Self {
        let handle = UniqueSingleHandle::new(value, &mut store);
        let store = ManuallyDrop::new(store);

        Self { store, handle }
    }

    /// Attempts to create a new instance.
    pub fn try_new_in(value: T, mut store: S) -> Result<Self, AllocError> {
        let handle = UniqueSingleHandle::try_new(value, &mut store)?;
        let store = ManuallyDrop::new(store);

        Ok(Self { store, handle })
    }
}

impl<T: Clone, S: StoreSingle + Default> Clone for StoreBox<T, S> {
    fn clone(&self) -> Self {
        let value: &T = self;

        Self::new(value.clone())
    }

    fn clone_from(&mut self, source: &StoreBox<T, S>) {
        let dest: &mut T = self;
        let source: &T = source;

        dest.clone_from(source);
    }
}

impl<T: ?Sized, S: StoreSingle> Drop for StoreBox<T, S> {
    fn drop(&mut self) {
        let value: &mut T = &mut *self;

        //  Safety:
        //  -   The instance is live.
        unsafe { ptr::drop_in_place(value) };

        //  Safety:
        //  -   `self.handle` is valid.
        //  -   `self.handle` will not be used after this point.
        let handle = unsafe { ptr::read(&self.handle) };

        //  Safety:
        //  -   `self.store` will never be used ever again.
        let mut store = unsafe { ManuallyDrop::take(&mut self.store) };

        //  Safety:
        //  -   `handle` was allocated by `store`.
        //  -   `handle` is still valid.
        unsafe { handle.deallocate(&mut store) };
    }
}

impl<T: ?Sized, S: StoreSingle> StoreBox<T, S> {
    /// Coerces to another `StoreBox`.
    ///
    /// A poor's man `CoerceUnsized`, since that trait cannot unfortunately be implemented.
    pub fn coerce<U: ?Sized>(mut self) -> StoreBox<U, S>
    where
        T: Unsize<U>,
    {
        //  Safety:
        //  -   `self.handle` is valid.
        //  -   `self.handle` will not be used after this point.
        let handle = unsafe { ptr::read(&self.handle) };

        //  Safety:
        //  -   `self.store` will never be used ever again.
        let store = unsafe { ManuallyDrop::take(&mut self.store) };

        mem::forget(self);

        let handle = handle.coerce();

        let store = ManuallyDrop::new(store);

        StoreBox { store, handle }
    }
}

impl<T: ?Sized, S: StoreSingle> ops::Deref for StoreBox<T, S> {
    type Target = T;

    fn deref(&self) -> &T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.store`.
        //  -   `self.handle` is still valid.
        //  -   `handle` is associated to a block of memory containing a live instance of T.
        unsafe { self.handle.resolve(&*self.store) }
    }
}

impl<T: ?Sized, S: StoreSingle> ops::DerefMut for StoreBox<T, S> {
    fn deref_mut(&mut self) -> &mut T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.store`.
        //  -   `self.handle` is still valid.
        //  -   `handle` is associated to a block of memory containing a live instance of T.
        unsafe { self.handle.resolve_mut(&mut *self.store) }
    }
}

impl<T: ?Sized, S: StoreSingle> fmt::Debug for StoreBox<T, S>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let value: &T = self;

        write!(f, "StoreBox({value:?})")
    }
}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, S: StoreSingle> CoerceUnsized<StoreBox<U, S>> for StoreBox<T, S> where T: Unsize<U> {}

#[cfg(test)]
mod test_inline {
    use crate::store::InlineSingleStore;

    use super::*;

    #[test]
    fn sized_store() {
        let store = InlineSingleStore::<u8>::default();
        let mut boxed = StoreBox::new_in(1u8, store);

        assert_eq!(1u8, *boxed);

        *boxed = 2;

        assert_eq!(2u8, *boxed);

        let mut clone = boxed.clone();

        *clone = 3;

        assert_eq!(2u8, *boxed);
        assert_eq!(3u8, *clone);
    }

    #[test]
    fn slice_store() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store);
        let mut boxed: StoreBox<[u8], _> = StoreBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn slice_coercion() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store);
        let mut boxed: StoreBox<[u8], _> = boxed;

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn trait_store() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store);
        let boxed: StoreBox<dyn fmt::Debug, _> = StoreBox::coerce(boxed);

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn trait_coercion() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store);
        let boxed: StoreBox<dyn fmt::Debug, _> = boxed;

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }
} // mod test_inline

#[cfg(test)]
mod test_allocator {
    use std::alloc::System;

    use crate::collection::utils::NonAllocator;

    use super::*;

    #[test]
    fn sized_failure() {
        StoreBox::try_new_in(1, NonAllocator).unwrap_err();
    }

    #[test]
    fn sized_allocated() {
        let mut boxed = StoreBox::new_in(1, System);

        assert_eq!(1u32, *boxed);

        *boxed = 2;

        assert_eq!(2u32, *boxed);

        let mut clone = boxed.clone();

        *clone = 3;

        assert_eq!(2u32, *boxed);
        assert_eq!(3u32, *clone);
    }

    #[test]
    fn slice_failure() {
        StoreBox::try_new_in([1u8, 2, 3], NonAllocator).unwrap_err();
    }

    #[test]
    fn slice_allocated() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System);
        let mut boxed: StoreBox<[u8], _> = StoreBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn slice_coercion() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System);
        let mut boxed: StoreBox<[u8], _> = boxed;

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn trait_failure() {
        StoreBox::try_new_in([1u8, 2, 3], NonAllocator).unwrap_err();
    }

    #[test]
    fn trait_allocated() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System);
        let boxed: StoreBox<dyn fmt::Debug, _> = StoreBox::coerce(boxed);

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn trait_coercion() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System);
        let boxed: StoreBox<dyn fmt::Debug, _> = boxed;

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }
} // mod test_allocator
