//! Proof-of-Concept implementation of a `Box` atop a `Store`.

use core::{
    alloc::AllocError,
    fmt,
    marker::Unsize,
    mem::{self, ManuallyDrop},
    ops, ptr,
};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{extension::unique::UniqueHandle, interface::Store};

/// A `Box` atop a `Store`.
pub struct StoreBox<T: ?Sized, S: Store> {
    store: ManuallyDrop<S>,
    handle: UniqueHandle<T, S::Handle>,
}

impl<T, S: Store + Default> StoreBox<T, S> {
    /// Creates a new instance.
    pub fn new(value: T) -> Result<Self, (T, S)> {
        Self::new_in(value, S::default())
    }
}

impl<T, S: Store> StoreBox<T, S> {
    /// Creates a new instance.
    pub fn new_in(value: T, store: S) -> Result<Self, (T, S)> {
        let Ok(handle) = UniqueHandle::allocate(&store) else {
            return Err((value, store))
        };

        //  Safety:
        //  -   `handle` was allocated by `self`.
        //  -   `handle` is still valid.
        let pointer = unsafe { handle.resolve_raw(&store) };

        //  Safety:
        //  -   `pointer` is valid for writes of `Layout::new::<T>().size()` bytes.
        unsafe { ptr::write(pointer.cast().as_ptr(), value) };

        let store = ManuallyDrop::new(store);

        Ok(Self { store, handle })
    }
}

impl<T: Clone, S: Store + Default> Clone for StoreBox<T, S> {
    fn clone(&self) -> Self {
        let value: &T = self;

        Self::new(value.clone())
            .map_err(|_| AllocError)
            .expect("Clone would have sufficient store space")
    }

    fn clone_from(&mut self, source: &StoreBox<T, S>) {
        let dest: &mut T = self;
        let source: &T = source;

        dest.clone_from(source);
    }
}

impl<T: ?Sized, S: Store> Drop for StoreBox<T, S> {
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
        let store = unsafe { ManuallyDrop::take(&mut self.store) };

        //  Safety:
        //  -   `handle` was allocated by `store`.
        //  -   `handle` is still valid.
        unsafe { handle.deallocate(&store) };
    }
}

impl<T: ?Sized, S: Store> StoreBox<T, S> {
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

impl<T: ?Sized, S: Store> ops::Deref for StoreBox<T, S> {
    type Target = T;

    fn deref(&self) -> &T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.store`.
        //  -   `self.handle` is still valid.
        //  -   `handle` is associated to a block of memory containing a live instance of T.
        unsafe { self.handle.resolve(&*self.store) }
    }
}

impl<T: ?Sized, S: Store> ops::DerefMut for StoreBox<T, S> {
    fn deref_mut(&mut self) -> &mut T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.store`.
        //  -   `self.handle` is still valid.
        //  -   `handle` is associated to a block of memory containing a live instance of T.
        unsafe { self.handle.resolve_mut(&*self.store) }
    }
}

impl<T: ?Sized, S: Store> fmt::Debug for StoreBox<T, S>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let value: &T = self;

        write!(f, "StoreBox({value:?})")
    }
}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, S: Store> CoerceUnsized<StoreBox<U, S>> for StoreBox<T, S> where T: Unsize<U> {}

#[cfg(test)]
mod test_inline {
    use crate::store::InlineSingleStore;

    use super::*;

    #[test]
    fn sized_store() {
        let store = InlineSingleStore::<u8>::default();
        let mut boxed = StoreBox::new_in(1u8, store).unwrap();

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
        let boxed = StoreBox::new_in([1u8, 2, 3], store).unwrap();
        let mut boxed: StoreBox<[u8], _> = StoreBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn slice_coercion() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store).unwrap();
        let mut boxed: StoreBox<[u8], _> = boxed;

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn trait_store() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store).unwrap();
        let boxed: StoreBox<dyn fmt::Debug, _> = StoreBox::coerce(boxed);

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn trait_coercion() {
        let store = InlineSingleStore::<[u8; 4]>::default();
        let boxed = StoreBox::new_in([1u8, 2, 3], store).unwrap();
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
        StoreBox::new_in(1, NonAllocator).unwrap_err();
    }

    #[test]
    fn sized_allocated() {
        let mut boxed = StoreBox::new_in(1, System).unwrap();

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
        StoreBox::new_in([1u8, 2, 3], NonAllocator).unwrap_err();
    }

    #[test]
    fn slice_allocated() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System).unwrap();
        let mut boxed: StoreBox<[u8], _> = StoreBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn slice_coercion() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System).unwrap();
        let mut boxed: StoreBox<[u8], _> = boxed;

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn trait_failure() {
        StoreBox::new_in([1u8, 2, 3], NonAllocator).unwrap_err();
    }

    #[test]
    fn trait_allocated() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System).unwrap();
        let boxed: StoreBox<dyn fmt::Debug, _> = StoreBox::coerce(boxed);

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn trait_coercion() {
        let boxed = StoreBox::new_in([1u8, 2, 3], System).unwrap();
        let boxed: StoreBox<dyn fmt::Debug, _> = boxed;

        assert_eq!("StoreBox([1, 2, 3])", format!("{:?}", boxed));
    }
} // mod test_allocator
