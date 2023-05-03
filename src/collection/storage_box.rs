//! Proof-of-Concept implementation of a `Box` atop a `Storage`.

use core::{
    fmt,
    marker::Unsize,
    mem::{self, ManuallyDrop},
    ops, ptr,
};

#[cfg(feature = "coercible-metadata")]
use core::ops::CoerceUnsized;

use crate::{extension::unique::UniqueHandle, interface::Storage};

/// A `Box` atop a `Storage`.
pub struct StorageBox<T: ?Sized, S: Storage> {
    storage: ManuallyDrop<S>,
    handle: UniqueHandle<T, S::Handle>,
}

impl<T, S: Storage> StorageBox<T, S> {
    /// Creates a new instance.
    pub fn new(value: T, storage: S) -> Result<Self, (T, S)> {
        let Ok(handle) = UniqueHandle::allocate(&storage) else {
            return Err((value, storage))
        };

        //  Safety:
        //  -   `handle` was allocated by `self`.
        //  -   `handle` is still valid.
        let pointer = unsafe { handle.resolve_raw(&storage) };

        //  Safety:
        //  -   `pointer` is valid for writes of `Layout::new::<T>().size()` bytes.
        unsafe { ptr::write(pointer.cast().as_ptr(), value) };

        let storage = ManuallyDrop::new(storage);

        Ok(Self { storage, handle })
    }
}

impl<T: ?Sized, S: Storage> Drop for StorageBox<T, S> {
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
        //  -   `self.storage` will never be used ever again.
        let storage = unsafe { ManuallyDrop::take(&mut self.storage) };

        //  Safety:
        //  -   `handle` was allocated by `storage`.
        //  -   `handle` is still valid.
        unsafe { handle.deallocate(&storage) };
    }
}

impl<T: ?Sized, S: Storage> StorageBox<T, S> {
    /// Coerces to another `StorageBox`.
    ///
    /// A poor's man `CoerceUnsized`, since that trait cannot unfortunately be implemented.
    pub fn coerce<U: ?Sized>(mut self) -> StorageBox<U, S>
    where
        T: Unsize<U>,
    {
        //  Safety:
        //  -   `self.handle` is valid.
        //  -   `self.handle` will not be used after this point.
        let handle = unsafe { ptr::read(&self.handle) };

        //  Safety:
        //  -   `self.storage` will never be used ever again.
        let storage = unsafe { ManuallyDrop::take(&mut self.storage) };

        mem::forget(self);

        let handle = handle.coerce();

        let storage = ManuallyDrop::new(storage);

        StorageBox { storage, handle }
    }
}

impl<T: ?Sized, S: Storage> ops::Deref for StorageBox<T, S> {
    type Target = T;

    fn deref(&self) -> &T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.storage`.
        //  -   `self.handle` is still valid.
        //  -   `handle` is associated to a block of memory containing a live instance of T.
        unsafe { self.handle.resolve(&*self.storage) }
    }
}

impl<T: ?Sized, S: Storage> ops::DerefMut for StorageBox<T, S> {
    fn deref_mut(&mut self) -> &mut T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.storage`.
        //  -   `self.handle` is still valid.
        //  -   `handle` is associated to a block of memory containing a live instance of T.
        unsafe { self.handle.resolve_mut(&*self.storage) }
    }
}

impl<T: ?Sized, S: Storage> fmt::Debug for StorageBox<T, S>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let value: &T = self;

        write!(f, "StorageBox({value:?})")
    }
}

#[cfg(feature = "coercible-metadata")]
impl<T, U: ?Sized, S: Storage> CoerceUnsized<StorageBox<U, S>> for StorageBox<T, S> where T: Unsize<U> {}

#[cfg(test)]
mod test_inline {
    use crate::storage::InlineSingleStorage;

    use super::*;

    #[test]
    fn sized_storage() {
        let storage = InlineSingleStorage::<u8>::default();
        let mut boxed = StorageBox::new(1u8, storage).unwrap();

        assert_eq!(1u8, *boxed);

        *boxed = 2;

        assert_eq!(2u8, *boxed);
    }

    #[test]
    fn slice_storage() {
        let storage = InlineSingleStorage::<[u8; 4]>::default();
        let boxed = StorageBox::new([1u8, 2, 3], storage).unwrap();
        let mut boxed: StorageBox<[u8], _> = StorageBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn slice_coercion() {
        let storage = InlineSingleStorage::<[u8; 4]>::default();
        let boxed = StorageBox::new([1u8, 2, 3], storage).unwrap();
        let mut boxed: StorageBox<[u8], _> = boxed;

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn trait_storage() {
        let storage = InlineSingleStorage::<[u8; 4]>::default();
        let boxed = StorageBox::new([1u8, 2, 3], storage).unwrap();
        let boxed: StorageBox<dyn fmt::Debug, _> = StorageBox::coerce(boxed);

        assert_eq!("StorageBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn trait_coercion() {
        let storage = InlineSingleStorage::<[u8; 4]>::default();
        let boxed = StorageBox::new([1u8, 2, 3], storage).unwrap();
        let boxed: StorageBox<dyn fmt::Debug, _> = boxed;

        assert_eq!("StorageBox([1, 2, 3])", format!("{:?}", boxed));
    }
} // mod test_inline

#[cfg(test)]
mod test_allocator {
    use std::alloc::System;

    use crate::{collection::utils::NonAllocator, storage::AllocatorStorage};

    use super::*;

    type Storage = AllocatorStorage<System>;
    type NonStorage = AllocatorStorage<NonAllocator>;

    #[test]
    fn sized_failure() {
        StorageBox::new(1, NonStorage::default()).unwrap_err();
    }

    #[test]
    fn sized_allocated() {
        let mut boxed = StorageBox::new(1, Storage::default()).unwrap();

        assert_eq!(1u32, *boxed);

        *boxed = 2;

        assert_eq!(2u32, *boxed);
    }

    #[test]
    fn slice_failure() {
        StorageBox::new([1u8, 2, 3], NonStorage::default()).unwrap_err();
    }

    #[test]
    fn slice_allocated() {
        let boxed = StorageBox::new([1u8, 2, 3], Storage::default()).unwrap();
        let mut boxed: StorageBox<[u8], _> = StorageBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn slice_coercion() {
        let boxed = StorageBox::new([1u8, 2, 3], Storage::default()).unwrap();
        let mut boxed: StorageBox<[u8], _> = boxed;

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn trait_failure() {
        StorageBox::new([1u8, 2, 3], NonStorage::default()).unwrap_err();
    }

    #[test]
    fn trait_allocated() {
        let boxed = StorageBox::new([1u8, 2, 3], Storage::default()).unwrap();
        let boxed: StorageBox<dyn fmt::Debug, _> = StorageBox::coerce(boxed);

        assert_eq!("StorageBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[cfg(feature = "coercible-metadata")]
    #[test]
    fn trait_coercion() {
        let boxed = StorageBox::new([1u8, 2, 3], Storage::default()).unwrap();
        let boxed: StorageBox<dyn fmt::Debug, _> = boxed;

        assert_eq!("StorageBox([1, 2, 3])", format!("{:?}", boxed));
    }
} // mod test_allocator
