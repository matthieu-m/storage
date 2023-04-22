//! Proof of concept concurrent access vector.
//!
//! For simplification, the capacity is fixed at creation, and elements cannot be removed.

use core::{
    alloc::Layout,
    fmt, hint,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ops,
    ptr::{self, NonNull},
    sync::atomic::{AtomicIsize, Ordering},
};

use crate::interface::Storage;

/// A fixed-capacity vector which can be modified concurrently.
pub struct ConcurrentVec<T, S: Storage> {
    //  Invariants:
    //  -   `length` is negative if a thread is appending a new element.
    //  -   `length.abs() - 1 <= self.store.capacity`.
    //  -   Elements in 0..(length.abs() - 1) are initialized.
    length: AtomicIsize,
    store: Store<T, S>,
}

impl<T, S: Storage> ConcurrentVec<T, S> {
    /// Creates a vector with a given capacity and a default storage.
    ///
    /// Since the vector cannot be resized later, pick well!
    pub fn new(capacity: usize) -> Self
    where
        S: Default,
    {
        Self::with_storage(capacity, S::default())
    }

    /// Creates a vector with a given capacity and storage.
    ///
    /// Since the vector cannot be resized later, pick well!
    pub fn with_storage(capacity: usize, storage: S) -> Self {
        let length = AtomicIsize::new(1);
        let store = Store::with_storage(capacity, storage);

        Self { length, store }
    }

    /// Returns whether the vector is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the length of the vector.
    pub fn len(&self) -> usize {
        (self.length.load(Ordering::Acquire).abs() - 1) as usize
    }

    /// Returns the capacity of the vector.
    pub fn capacity(&self) -> usize {
        self.store.capacity
    }

    /// Returns a reference to the slice of initialized elements.
    pub fn as_slice(&self) -> &[T] {
        let initialized = self.initialized();

        //  Safety:
        //  -   `initialized` covers a valid area of memory.
        //  -   `initialized` covers a readable area of memory.
        //  -   `initialized` is accessible in shared mode, as `self` is borrowed immutably for the duration.
        //  -   The lifetime of the resulting slice will not exceed that of `self.store`.
        unsafe { initialized.as_ref() }
    }

    /// Returns a mutable reference to the slice of initialized elements.
    pub fn as_slice_mut(&mut self) -> &mut [T] {
        let mut initialized = self.initialized();

        //  Safety:
        //  -   `initialized` covers a valid area of memory.
        //  -   `initialized` covers a readable and writeable area of memory.
        //  -   `initialized` is accessible in exclusive mode, as `self` is borrowed mutably for the duration.
        //  -   The lifetime of the resulting slice will not exceed that of `self.store`.
        unsafe { initialized.as_mut() }
    }

    /// Returns a reference to the element at `index`.
    ///
    /// #   Safety
    ///
    /// -   `index` must be strictly less than `self.len()`.
    pub unsafe fn get_unchecked(&self, index: usize) -> &T {
        //  Safety:
        //  -   `index <= self.len()`, as per method invariant.
        let initialized = unsafe { self.initialized_unchecked(index + 1) };

        //  Safety:
        //  -   `index < index + 1`.
        let element = unsafe { initialized.get_unchecked_mut(index) };

        //  Safety:
        //  -   `initialized` covers a valid area of memory.
        //  -   `initialized` covers a readable area of memory.
        //  -   `initialized` is accessible in shared mode, as `self` is borrowed immutably for the duration.
        //  -   The lifetime of the resulting slice will not exceed that of `self.store`.
        unsafe { element.as_ref() }
    }

    /// Returns a mutable reference to the element at `index`.
    ///
    /// #   Safety
    ///
    /// -   `index` must be strictly less than `self.len()`.
    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        //  Safety:
        //  -   `index <= self.len()`, as per method invariant.
        let initialized = unsafe { self.initialized_unchecked(index + 1) };

        //  Safety:
        //  -   `index < index + 1`.
        let mut element = unsafe { initialized.get_unchecked_mut(index) };

        //  Safety:
        //  -   `initialized` covers a valid area of memory.
        //  -   `initialized` covers a readable and writeable area of memory.
        //  -   `initialized` is accessible in exclusive mode, as `self` is borrowed mutably for the duration.
        //  -   The lifetime of the resulting slice will not exceed that of `self.store`.
        unsafe { element.as_mut() }
    }

    /// Attempts to push a new element into the vector.
    ///
    /// The vector is locked for writes for the duration of the operation.
    ///
    /// Returns an error if the vector is full, that is, if `self.len() == self.capacity()`.
    pub fn push(&self, element: T) -> Result<(), T> {
        let mut length = self.length.load(Ordering::Acquire);

        loop {
            if length.unsigned_abs() > self.store.capacity {
                return Err(element);
            }

            if length < 0 {
                hint::spin_loop();

                length = self.length.load(Ordering::Acquire);
                continue;
            }

            debug_assert!(length > 0);

            let result = self
                .length
                .compare_exchange_weak(length, -length, Ordering::Acquire, Ordering::Relaxed);

            if let Err(prev) = result {
                hint::spin_loop();

                length = prev;
                continue;
            }

            break;
        }

        //  The slot at `length - 1` is ours!
        debug_assert!(length > 0, "{length}");
        debug_assert!(
            length.unsigned_abs() <= self.store.capacity,
            "{length} > {}",
            self.store.capacity
        );

        let slots = self.store.slots();

        //  Safety:
        //  -   `length - 1 < self.store.capacity`, since `length > 0` and `length <= self.store.capacity`.
        let slot = unsafe { slots.get_unchecked_mut(length as usize - 1) };

        //  Safety:
        //  -   `slot` points to a valid area of memory.
        //  -   `slot` points to a writeable area of memory.
        //  -   `slot` is accessible in exclusive mode, as per the lock on `self.length`.
        unsafe { ptr::write(slot.as_ptr(), element) };

        self.length.store(length + 1, Ordering::Release);

        Ok(())
    }
}

impl<T, S> Clone for ConcurrentVec<T, S>
where
    T: Clone,
    S: Storage + Clone,
{
    fn clone(&self) -> Self {
        let clone = Self::with_storage(self.store.capacity, self.store.storage.clone());

        let elements = self.as_slice();
        let slots = clone.store.slots();

        debug_assert!(elements.len() <= slots.len());

        //  Safety:
        //  -   `elements.len() <= slots.len()`.
        let slots = unsafe { slots.get_unchecked_mut(..elements.len()) };

        //  Safety:
        //  -   `slots` is valid for reads and writes of `slots.len()`, since the allocation succeeded and we have
        //      exlusive access for now.
        //  -   `slots.len()` is no larger than `isize::MAX`, since the allocation succeeded.
        //  -   The result `slots` will not outlive this function call.
        let slots = unsafe { slots.as_uninit_slice_mut() };

        MaybeUninit::write_slice_cloned(slots, elements);

        clone.length.store(elements.len() as isize + 1, Ordering::Release);

        clone
    }
}

impl<T, S: Storage> Drop for ConcurrentVec<T, S> {
    fn drop(&mut self) {
        if !mem::needs_drop::<T>() {
            return;
        }

        let initialized = self.initialized();

        for index in 0..initialized.len() {
            //  Safety:
            //  -   `index <= initialized.len()`.
            let element = unsafe { initialized.get_unchecked_mut(index) };

            //  Safety:
            //  -   `element` is valid for both reads and writes.
            //  -   `element` is properly aligned.
            //  -   There are no specific invariant to uphold for `element`.
            unsafe { ptr::drop_in_place(element.as_ptr()) };
        }
    }
}

impl<T, S: Storage> fmt::Debug for ConcurrentVec<T, S>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{:?}", self.as_slice())
    }
}

impl<T, S: Storage> ops::Deref for ConcurrentVec<T, S> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, S: Storage> ops::DerefMut for ConcurrentVec<T, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_slice_mut()
    }
}

//  Safety:
//  -   Same as `Vec<T>`.
unsafe impl<T, S> Send for ConcurrentVec<T, S>
where
    T: Send,
    S: Storage + Send,
{
}

//  Safety:
//  -   Same as `Vec<T>`.
unsafe impl<T, S> Sync for ConcurrentVec<T, S>
where
    T: Sync,
    S: Storage + Sync,
{
}

//
//  Implementation
//

impl<T, S: Storage> ConcurrentVec<T, S> {
    //  Returns a pointer to the slice of initialized elements.
    fn initialized(&self) -> NonNull<[T]> {
        //  Safety:
        //  -   `self.len() <= self.len()`.
        unsafe { self.initialized_unchecked(self.len()) }
    }

    //  Returns a pointer to the slice of initialized elements up to `index`, not included.
    //
    //  #   Safety
    //
    //  -   `index` must be less than or equal to `self.len()`.
    unsafe fn initialized_unchecked(&self, index: usize) -> NonNull<[T]> {
        debug_assert!(index <= self.len(), "{index} > {}", self.len());

        let slots = self.store.slots();

        debug_assert_eq!(slots.len(), self.capacity());

        //  Safety:
        //  -   `index` is within bounds, as per invariant `self.len() <= self.capacity()`.
        unsafe { slots.get_unchecked_mut(..index) }
    }
}

struct Store<T, S: Storage> {
    capacity: usize,
    handle: S::Handle,
    storage: S,
    _marker: PhantomData<T>,
}

impl<T, S: Storage> Store<T, S> {
    //  Creates a store with a given capacity and storage.
    fn with_storage(capacity: usize, storage: S) -> Self {
        let layout = Layout::array::<T>(capacity).expect("Small enough capacity");

        let handle = storage.allocate(layout).expect("Successful allocation");
        let _marker = PhantomData;

        Self {
            capacity,
            handle,
            storage,
            _marker,
        }
    }

    //  Retrieves the slots of storage.
    //
    //  The slice is only valid as long as `self` is live.
    fn slots(&self) -> NonNull<[T]> {
        //  Safety:
        //  -   `self.handle` has been allocated by `self.storage`.
        //  -   `self.handle` is still valid, since no operation other than `resolve` occurred.
        //  -   The block of memory associated to the handle will only be used as long as `self.handle` is valid.
        let pointer = unsafe { self.storage.resolve(self.handle) };

        NonNull::slice_from_raw_parts(pointer.cast(), self.capacity)
    }
}

impl<T, S: Storage> Drop for Store<T, S> {
    fn drop(&mut self) {
        let layout = Layout::array::<T>(self.capacity).expect("Small enough capacity");

        //  Safety:
        //  -   `self.handle` has been allocated by `self.storage`.
        //  -   `self.handle` is still valid, since no operation other than `resolve` occurred.
        //  -   `layout` fits the layout of the block of memory associated with `self.handle`.
        unsafe { self.storage.deallocate(self.handle, layout) }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use crate::collection::utils::Global;

    use super::*;

    type GlobalVec = ConcurrentVec<String, Global>;

    #[test]
    fn empty() {
        let empty = GlobalVec::new(42);

        assert!(empty.is_empty());
        assert_eq!(0, empty.len());
        assert_eq!(42, empty.capacity());
    }

    #[test]
    fn brush() {
        let vec = GlobalVec::new(42);

        for i in 0..3 {
            vec.push(i.to_string()).unwrap();
        }

        assert_eq!(&["0", "1", "2"][..], vec.as_slice());
    }

    #[test]
    fn overflow() {
        const CAP: usize = 5;

        let vec = GlobalVec::new(CAP);

        for i in 0..CAP {
            vec.push(i.to_string()).unwrap();
        }

        let result = vec.push(CAP.to_string());
        assert_eq!(Err(CAP.to_string()), result);
    }

    #[test]
    fn multithreaded() {
        const THREADS: usize = 4;
        const ELEMENTS: usize = 4;

        let vec = Arc::new(GlobalVec::new(THREADS * ELEMENTS));

        let handles: Vec<_> = (0..THREADS)
            .map(|i| {
                let vec = vec.clone();

                thread::spawn(move || {
                    for k in 0..ELEMENTS {
                        vec.push((i * ELEMENTS + k).to_string()).unwrap();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(THREADS * ELEMENTS, vec.len());

        let mut elements: Vec<usize> = vec.as_slice().iter().map(|n| n.parse().unwrap()).collect();
        elements.sort();

        assert_eq!(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15][..], &elements);
    }
} // mod tests