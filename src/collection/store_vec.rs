//! A Dynamic Array.
//!
//! This implementation is solely meant to demonstrate the use of `StoreSharing`, it is incomplete, and may be buggy.

use core::{
    mem::{self, MaybeUninit},
    ops::Range,
    ptr::{self, NonNull},
};

use crate::{
    extension::unique::UniqueHandle,
    interface::{Store, StoreDangling},
};

/// A dynamic array.
pub struct StoreVec<T, S: Store> {
    //  Type invariant:
    //  -   `self.length < self.array.capacity()`.
    //  -   Slots in `0..self.length` are initialized.
    //  -   Slots in `self.length..` may be uninitialized.
    length: usize,
    array: UniqueArray<T, S>,
}

impl<T, S: Store + Default> StoreVec<T, S> {
    /// Creates a new, empty, instance.
    pub fn new() -> Self {
        Self::new_in(S::default())
    }

    /// Creates a new, empty, instance with at least the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_in(capacity, S::default())
    }
}

impl<T, S: Store> StoreVec<T, S> {
    /// Creates a new, empty, instance.
    pub const fn new_in(store: S) -> Self
    where
        S: ~const StoreDangling,
    {
        let length = 0;
        let array = UniqueArray::new_in(store);

        Self { length, array }
    }

    /// Creates a new, empty, instance with at least the specified capacity.
    pub const fn with_capacity_in(capacity: usize, store: S) -> Self
    where
        S: ~const Store + ~const StoreDangling,
    {
        let length = 0;
        let array = UniqueArray::with_capacity_in(capacity, store);

        Self { length, array }
    }
}

impl<T, S: Store> StoreVec<T, S> {
    /// Returns whether the vector is empty.
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Returns the number of elements in the vector.
    pub const fn len(&self) -> usize {
        self.length
    }

    /// Returns the capacity of the vector.
    pub const fn capacity(&self) -> usize {
        self.array.capacity()
    }

    /// Forces the length of the vector to `new_len`.
    ///
    /// #   Safety
    ///
    /// -   `new_len` must less than or equal to `self.capacity()`.
    /// -   The elements in `self.len()..new_len` must be initialized.
    pub const unsafe fn set_len(&mut self, new_len: usize) {
        self.length = new_len;
    }
}

impl<T, S: Store> StoreVec<T, S> {
    /// Returns a raw pointer to the vector’s buffer.
    ///
    /// If the vector didn't allocate yet, that is, if its capacity is 0, this pointer is dangling, and valid for zero
    /// sized reads.
    pub const fn as_ptr(&self) -> *const T
    where
        S: ~const Store,
    {
        self.array.as_slice().as_mut_ptr() as *const T
    }

    /// Returns a raw pointer to the vector’s buffer.
    ///
    /// If the vector didn't allocate yet, that is, if its capacity is 0, this pointer is dangling, and valid for zero
    /// sized reads.
    pub const fn as_mut_ptr(&mut self) -> *mut T
    where
        S: ~const Store,
    {
        self.array.as_slice().as_mut_ptr()
    }

    /// Returns a slice of the elements of the vector.
    pub const fn as_slice(&self) -> &[T]
    where
        S: ~const Store,
    {
        debug_assert!(self.length <= self.capacity());

        //  Safety:
        //  -   `0 <= self.length`, as `self.length` is unsigned.
        //  -   `self.length <= self.capacity()`, as per type invariant.
        let slice = unsafe { self.array.as_sub_slice_unchecked(0..self.length) };

        //  Safety:
        //  -   Slots in `0..self.length` are initialized, as per type invariant.
        //  -   `self` is borrowed immutably for the lifetime of the result.
        unsafe { slice.as_ref() }
    }

    /// Returns a mutable slice of the elements of the vector.
    pub const fn as_mut_slice(&mut self) -> &mut [T]
    where
        S: ~const Store,
    {
        debug_assert!(self.length <= self.capacity());

        //  Safety:
        //  -   `0 <= self.length`, as `self.length` is unsigned.
        //  -   `self.length <= self.capacity()`, as per type invariant.
        let mut slice = unsafe { self.array.as_sub_slice_unchecked(0..self.length) };

        //  Safety:
        //  -   Slots in `0..self.length` are initialized, as per type invariant.
        //  -   `self` is borrowed mutably for the lifetime of the result.
        unsafe { slice.as_mut() }
    }

    /// Returns the remaining spare capacity of the vector as a slice of `MaybeUninit<T>`.
    pub const fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>]
    where
        S: ~const Store,
    {
        debug_assert!(self.length <= self.capacity());

        let capacity = self.capacity();

        //  Safety:
        //  -   `self.length <= self.capacity()`, as per type invariant.
        //  -   `self.capacity() <= self.capacity()`, tautologically.
        let slice = unsafe { self.array.as_sub_slice_unchecked(self.length..capacity) };

        //  Safety:
        //  -   `self` is borrowed mutably for the lifetime of the result.
        unsafe { slice.as_uninit_slice_mut() }
    }
}

impl<T, S: Store> StoreVec<T, S> {
    /// Reserves capacity for at least `additional` more elements.
    ///
    /// #   Panics
    ///
    /// Panics if the new capacity exceeds `isize::MAX` bytes.
    pub const fn reserve(&mut self, additional: usize)
    where
        S: ~const Store + ~const StoreDangling,
    {
        if additional < self.capacity() && self.length <= self.capacity() - additional {
            return;
        }

        self.grow_for(additional)
    }
}

impl<T, S: Store> StoreVec<T, S> {
    /// Returns a reference to the element at index `n`, if any.
    pub const fn get(&self, n: usize) -> Option<&T>
    where
        S: ~const Store,
    {
        debug_assert!(self.length <= self.capacity());

        if n >= self.length {
            return None;
        }

        //  Safety:
        //  -   `n <= self.length`, as per condition above.
        //  -   `self.length <= self.capacity()`, as per type invariant.
        let slice = unsafe { self.array.as_sub_slice_unchecked(n..self.length) };

        let slot = slice.as_mut_ptr() as *const T;

        //  Safety:
        //  -   Slots in `0..self.length` are initialized, as per type invariant.
        //  -   `self` is borrowed immutably for the lifetime of the result.
        unsafe { Some(&*slot) }
    }

    /// Returns a mutable reference to the element at index `n`, if any.
    pub const fn get_mut(&mut self, n: usize) -> Option<&mut T>
    where
        S: ~const Store,
    {
        debug_assert!(self.length <= self.capacity());

        if n >= self.length {
            return None;
        }

        //  Safety:
        //  -   `n <= self.length`, as per condition above.
        //  -   `self.length <= self.capacity()`, as per type invariant.
        let slice = unsafe { self.array.as_sub_slice_unchecked(n..self.length) };

        let slot = slice.as_mut_ptr();

        //  Safety:
        //  -   Slots in `0..self.length` are initialized, as per type invariant.
        //  -   `self` is borrowed mutably for the lifetime of the result.
        unsafe { Some(&mut *slot) }
    }
}

impl<T, S: Store> StoreVec<T, S> {
    /// Clears the vector, removing all values.
    pub fn clear(&mut self) {
        debug_assert!(self.length <= self.capacity());

        let length = mem::replace(&mut self.length, 0);

        //  Safety:
        //  -   `0 <= length`, as `length` is unsigned.
        //  -   `length <= self.capacity()`, as per type invariant.
        let slice = unsafe { self.array.as_sub_slice_unchecked(0..length) };

        let pointer: *mut [T] = slice.as_ptr();

        //  Safety:
        //  -   `pointer` is properly aligned.
        //  -   `pointer` is non-null.
        //  -   `pointer` is valid for both reads and writes.
        //  -   `pointer` points to a slice of initialized elements.
        unsafe { ptr::drop_in_place(pointer) };
    }

    /// Appends an element at the back the vector.
    pub const fn push(&mut self, value: T)
    where
        S: ~const Store + ~const StoreDangling,
    {
        if self.length == self.capacity() {
            self.grow_for(1);
        }

        let spare = self.spare_capacity_mut();
        debug_assert!(!spare.is_empty());

        let slot = spare.as_mut_ptr() as *mut T;

        //  Safety:
        //  -   `slot` is well aligned.
        //  -   `slot` is valid for writes of size `T`, since `spare` is not empty after growth.
        unsafe { ptr::write(slot, value) };

        self.length += 1;
    }

    /// Removes the last element from this vector and returns it, if any.
    pub const fn pop(&mut self) -> Option<T>
    where
        S: ~const Store,
    {
        debug_assert!(self.length <= self.capacity());

        if self.is_empty() {
            return None;
        }

        self.length -= 1;

        //  Safety:
        //  -   `0 <= self.length`, as `self.length` is unsigned.
        //  -   `self.length <= self.capacity()`, as per type invariant.
        let slice = unsafe { self.array.as_sub_slice_unchecked(self.length..self.capacity()) };

        let slot = slice.as_mut_ptr() as *const T;

        //  Safety:
        //  -   `slot` is well-aligned.
        //  -   `slot` is valid for read of size T.
        //  -   `slot` is initialized, as per type invariant.
        let element = unsafe { ptr::read(slot) };

        Some(element)
    }
}

impl<T, S: Store + Default> Default for StoreVec<T, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, S: Store> Drop for StoreVec<T, S> {
    fn drop(&mut self) {
        self.clear();
    }
}

//
//  Implementation
//

impl<T, S: Store> StoreVec<T, S> {
    #[inline(never)]
    const fn grow_for(&mut self, additional: usize)
    where
        S: ~const Store + ~const StoreDangling,
    {
        let Some(target_capacity) = self.length.checked_add(additional) else {
            UniqueArray::<T, S>::capacity_exceeded()
        };

        //  The caller shouldn't have called...
        if target_capacity <= self.capacity() {
            return;
        }

        let target_capacity = UniqueArray::<T, S>::round_up_capacity(target_capacity);

        //  Safety:
        //  -   `target_capacity` is greater than or equal to `self.array.capacity()`.
        unsafe { self.array.grow_to(target_capacity) };
    }
}

struct UniqueArray<T, S: Store> {
    handle: UniqueHandle<[T], S::Handle>,
    store: S,
}

impl<T, S: Store> UniqueArray<T, S> {
    const fn new_in(store: S) -> Self
    where
        S: ~const StoreDangling,
    {
        let handle = UniqueHandle::dangling_slice(&store);

        Self { handle, store }
    }

    const fn with_capacity_in(capacity: usize, store: S) -> Self
    where
        S: ~const Store + ~const StoreDangling,
    {
        let handle = UniqueHandle::allocate_slice(capacity, &store);

        Self { handle, store }
    }

    const fn capacity(&self) -> usize {
        self.handle.len()
    }

    const fn as_slice(&self) -> NonNull<[T]>
    where
        S: ~const Store,
    {
        //  Safety:
        //  -   `self.handle` is a valid or dangling handle.
        //  -   `self.handle` was obtained from `self.store` in either case.
        unsafe { self.handle.resolve_raw(&self.store) }
    }

    //  #   Safety
    //
    //  -   `range.start <= range.end`.
    //  -   `range.end <= self.capacity()`.
    const unsafe fn as_sub_slice_unchecked(&self, range: Range<usize>) -> NonNull<[T]>
    where
        S: ~const Store,
    {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.handle.len());

        let slice = self.as_slice();

        let pointer = slice.as_mut_ptr();

        //  Safety:
        //  -   `pointer` is correctly aligned.
        //  -   `range.start <= slice.len()`.
        let pointer = unsafe { pointer.add(range.start) };

        //  Safety:
        //  -   `pointer` is non-null, since it comes from a `NonNull`, and was not decremented.
        let pointer = unsafe { NonNull::new_unchecked(pointer) };

        NonNull::slice_from_raw_parts(pointer, range.end - range.start)
    }
}

impl<T, S: Store> UniqueArray<T, S> {
    #[cold]
    #[inline(never)]
    const fn capacity_exceeded() -> ! {
        panic!("New capacity exceeds isize::MAX bytes")
    }

    const fn round_up_capacity(min_capacity: usize) -> usize {
        if min_capacity <= 1 || min_capacity.count_ones() == 1 {
            return min_capacity;
        }

        if min_capacity >= 1 << (usize::BITS - 1) {
            Self::capacity_exceeded()
        }

        let shift = usize::BITS - (min_capacity - 1).leading_zeros();

        1 << shift
    }

    //  #   Safety
    //
    //  -   `target_capacity` must be greater than or equal to `self.capacity()`.
    //
    //  #   Panics
    //
    //  If the new capacity exceeds `isize::MAX` bytes.
    const unsafe fn grow_to(&mut self, target_capacity: usize)
    where
        S: ~const Store + ~const StoreDangling,
    {
        const MAX_BYTES: usize = isize::MAX as usize;

        let Some(target_bytes) = target_capacity.checked_mul(mem::size_of::<T>()) else {
            Self::capacity_exceeded()
        };

        if target_bytes > MAX_BYTES {
            Self::capacity_exceeded()
        }

        if self.handle.is_empty() {
            self.handle = UniqueHandle::allocate_slice(target_capacity, &self.store);
        } else {
            //  Safety:
            //  -   `self.handle` was allocated by `self.store`.
            //  -   `self.handle` is still valid.
            //  -   `target_capacity` is greater than or equal to `self.handle.len()`.
            unsafe { self.handle.grow(target_capacity, &self.store) };
        }
    }
}

impl<T, S: Store> Drop for UniqueArray<T, S> {
    fn drop(&mut self) {
        if self.handle.is_empty() {
            return;
        }

        //  Safety:
        //  -   `self.handle` is valid.
        //  -   `self.handle` will not be used after this point.
        let handle = unsafe { ptr::read(&self.handle) };

        //  Safety:
        //  -   `handle` is still valid, notably it is not dangling since its length is non-zero.
        //  -   `handle` was allocated by `self.store`.
        unsafe { handle.deallocate(&self.store) };
    }
}

#[cfg(test)]
mod tests_inline {
    use crate::store::InlineSingleStore;

    use super::*;

    type InlineVec<T, const N: usize> = StoreVec<T, InlineSingleStore<[T; N]>>;

    #[test]
    fn const_inline_vec() {
        const fn fib<const N: usize>() -> InlineVec<i64, N> {
            let mut v = InlineVec::new_in(InlineSingleStore::new());

            if N > 0 {
                v.push(0);
            }

            if N > 1 {
                v.push(1);
            }

            let mut n_2 = 0;
            let mut n_1 = 1;

            while v.len() < N {
                let n = n_1 + n_2;
                n_2 = n_1;
                n_1 = n;

                v.push(n);
            }

            v
        }

        static FIB: InlineVec<i64, 10> = fib::<10>();

        assert_eq!(&[0, 1, 1, 2, 3, 5, 8, 13, 21, 34][..], FIB.as_slice());
    }

    #[test]
    fn send_sync() {
        fn require_send<T: Send>() {}
        fn require_sync<T: Sync>() {}

        require_send::<InlineVec<String, 2>>();
        require_sync::<InlineVec<String, 2>>();
    }

    #[test]
    fn brush() {
        let mut v = InlineVec::<String, 12>::new();

        assert_eq!(0, v.len());
        assert_eq!(0, v.capacity());
        assert_eq!(None, v.pop());

        v.push(String::from("0"));

        assert_eq!(1, v.len());
        assert_eq!(12, v.capacity());

        v.push(String::from("2"));

        assert_eq!(Some("2"), v.pop().as_deref());

        v.push(String::from("2"));
        v.push(String::from("2"));

        let s = v.get_mut(1).unwrap();
        s.clear();
        s.push('1');

        assert_eq!(["0", "1", "2"], v.as_slice());
    }
} // mod tests_inline
