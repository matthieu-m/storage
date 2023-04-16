//! Proof-of-Concept implementation of a `Box` atop a `Storage`.

use core::{
    alloc::Layout,
    fmt,
    marker::{PhantomData, Unsize},
    mem::{self, ManuallyDrop},
    ops,
    ptr::{self, NonNull, Pointee},
};

use crate::interface::Storage;

/// A `Box` atop a `Storage`.
pub struct StorageBox<T: ?Sized, S: Storage> {
    metadata: <T as Pointee>::Metadata,
    handle: S::Handle,
    storage: ManuallyDrop<S>,
    _marker: PhantomData<T>,
}

impl<T, S: Storage> StorageBox<T, S> {
    /// Creates a new instance.
    pub fn new(value: T, storage: S) -> Result<Self, (T, S)> {
        let Ok(handle) = storage.allocate(Layout::new::<T>()) else {
            return Err((value, storage))
        };

        //  Safety:
        //  -   `handle` was allocated by `self`.
        //  -   `handle` is still valid.
        let pointer = unsafe { storage.resolve(handle) };

        //  Safety:
        //  -   `pointer` is valid for writes of `Layout::new::<T>().size()` bytes.
        unsafe { ptr::write(pointer.cast().as_ptr(), value) };

        #[allow(clippy::let_unit_value)]
        let metadata = ();
        let storage = ManuallyDrop::new(storage);
        let _marker = PhantomData;

        Ok(Self {
            metadata,
            handle,
            storage,
            _marker,
        })
    }
}

impl<T: ?Sized, S: Storage> Drop for StorageBox<T, S> {
    fn drop(&mut self) {
        let value: &mut T = &mut *self;

        let layout = Layout::for_value(value);

        //  Safety:
        //  -   The instance is live.
        unsafe { ptr::drop_in_place(value) };

        //  Safety:
        //  -   `self.storage` will never be used ever again.
        let storage = unsafe { ManuallyDrop::take(&mut self.storage) };

        //  Safety:
        //  -   `self.handle` was allocated by `self.storage`.
        //  -   `self.handle` is still valid.
        //  -   `layout` fits the value for which the allocation was made.
        unsafe { storage.deallocate(self.handle, layout) };
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
        let metadata = ptr::metadata(&*self as &U);
        //  Safety:
        //  -   `self.storage` will never be used ever again.
        let storage = ManuallyDrop::new(unsafe { ManuallyDrop::take(&mut self.storage) });

        let handle = self.handle;
        let _marker = PhantomData;

        mem::forget(self);

        StorageBox {
            metadata,
            handle,
            storage,
            _marker,
        }
    }
}

impl<T: ?Sized, S: Storage> ops::Deref for StorageBox<T, S> {
    type Target = T;

    fn deref(&self) -> &T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.storage`.
        //  -   `self.handle` is still valid.
        let pointer = unsafe { self.storage.resolve(self.handle) };

        let pointer = NonNull::from_raw_parts(pointer.cast(), self.metadata);

        //  Safety:
        //  -   `pointer` points to a valid instance of `T`.
        //  -   Access to result is shared, as `self` is immutably borrowed for its lifetime.
        unsafe { pointer.as_ref() }
    }
}

impl<T: ?Sized, S: Storage> ops::DerefMut for StorageBox<T, S> {
    fn deref_mut(&mut self) -> &mut T {
        //  Safety:
        //  -   `self.handle` was allocated by `self.storage`.
        //  -   `self.handle` is still valid.
        let pointer = unsafe { self.storage.resolve(self.handle) };

        let mut pointer = NonNull::from_raw_parts(pointer.cast(), self.metadata);

        //  Safety:
        //  -   `pointer` points to a valid instance of `T`.
        //  -   Access to result is exclusive, as `self` is mutably borrowed for its lifetime.
        unsafe { pointer.as_mut() }
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

    #[test]
    fn trait_storage() {
        let storage = InlineSingleStorage::<[u8; 4]>::default();
        let boxed = StorageBox::new([1u8, 2, 3], storage).unwrap();
        let boxed: StorageBox<dyn fmt::Debug, _> = StorageBox::coerce(boxed);

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
    fn sized_allocated() {
        let mut boxed = StorageBox::new(1, Storage::default()).unwrap();

        assert_eq!(1u32, *boxed);

        *boxed = 2;

        assert_eq!(2u32, *boxed);
    }

    #[test]
    fn sized_failure() {
        StorageBox::new(1, NonStorage::default()).unwrap_err();
    }

    #[test]
    fn slice_allocated() {
        let boxed = StorageBox::new([1u8, 2, 3], Storage::default()).unwrap();
        let mut boxed: StorageBox<[u8], _> = StorageBox::coerce(boxed);

        assert_eq!([1u8, 2, 3], &*boxed);

        boxed[2] = 4;

        assert_eq!([1u8, 2, 4], &*boxed);
    }

    #[test]
    fn slice_failure() {
        StorageBox::new([1u8, 2, 3], NonStorage::default()).unwrap_err();
    }

    #[test]
    fn trait_allocated() {
        let boxed = StorageBox::new([1u8, 2, 3], Storage::default()).unwrap();
        let boxed: StorageBox<dyn fmt::Debug, _> = StorageBox::coerce(boxed);

        assert_eq!("StorageBox([1, 2, 3])", format!("{:?}", boxed));
    }

    #[test]
    fn trait_failure() {
        StorageBox::new([1u8, 2, 3], NonStorage::default()).unwrap_err();
    }
} // mod test_allocator
