//! A Linked List.
//!
//! This implementation is solely meant to demonstrate the use of `SharingStore`, it is incomplete, and may be buggy.

use core::{alloc::AllocError, cmp, fmt, hash, mem, ptr};

use crate::{
    extension::typed::TypedHandle,
    interface::{StoreMultiple, SharingStore, StoreStable, Store},
};

/// A singly-linked list.
pub struct LinkedList<T, S: Store> {
    //  Invariant: number of nodes in the list. A length of 0 means that the `head` and `tail` handles are dangling.
    length: usize,
    head: NodeHandle<T, S::Handle>,
    tail: NodeHandle<T, S::Handle>,
    store: S,
}

impl<T, S: Store> LinkedList<T, S> {
    /// Creates a new, empty, list.
    pub fn new() -> Self
    where
        S: Default,
    {
        Self::new_in(S::default())
    }

    /// Creates a new, empty, list with the specified `store`.
    pub fn new_in(store: S) -> Self {
        let length = 0;
        let head = NodeHandle::dangling(&store);
        let tail = NodeHandle::dangling(&store);

        Self {
            length,
            head,
            tail,
            store,
        }
    }

    /// Returns whether the list is empty, or not.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Returns the number of elements in the list.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Returns whether the list contains `element`, or not.
    pub fn contains(&self, element: &T) -> bool
    where
        T: PartialEq,
    {
        let mut handle = self.head;

        for _ in 0..self.length {
            //  Safety:
            //  -   `handle` has been allocated by `self.store`.
            //  -   `handle` is valid, since there are `length` valid handles.
            //  -   `handle` is associated with a memory block containing a valid instance of `Node`.
            //  -   Access to the resulting `node` is shared, as guaranteed by `self` being borrowed immutably.
            let node = unsafe { handle.resolve(&self.store) };

            if node.element == *element {
                return true;
            }

            handle = node.next;
        }

        false
    }

    /// Clears the list, removing every element.
    ///
    /// The resulting list is empty.
    pub fn clear(&mut self) {
        let length = mem::replace(&mut self.length, 0);

        let mut handle = self.head;

        for _ in 0..length {
            //  Safety:
            //  -   `handle` has been allocated by `self.store`.
            //  -   `handle` is valid, since there are `length` valid handles.
            //  -   `handle` is associated with a memory block containing a valid instance of `Node`.
            //  -   Access to the resulting `node` is exclusive, as guaranteed by `self` being borrowed mutably.
            let node = unsafe { handle.resolve_mut(&self.store) };

            //  Safety:
            //  -   `node.element` is live instance of `T`.
            //  -   `node.element` will not be used afterwards.
            //  -   Access to `node.element` is exclusive, as guaranteed by `node` being borrowed mutably.
            unsafe { ptr::drop_in_place(&mut node.element) };

            let next = node.next;

            //  Safety:
            //  -   `handle` has been allocated by `self.store`.
            //  -   `handle` is valid, since there are `length` valid handles.
            //  -   Access to the resulting `node` is exclusive, as guaranteed by `self` being borrowed mutably.
            unsafe { handle.deallocate(&self.store) };

            handle = next;
        }
    }

    /// Returns a mutable reference to the front element, if any.
    pub fn front_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            return None;
        }

        //  Safety:
        //  -   `self.head` has been allocated by `self.store`.
        //  -   `self.head` is valid, since `length` is not 0.
        //  -   `self.head` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `node` is exclusive, as guaranteed by `self` being borrowed mutably.
        let node = unsafe { self.head.resolve_mut(&self.store) };

        //  It is safe to return the reference, as it extends the borrow of `self`, guaranteeing that no operation on
        //  `self.store` will occur which could potentially invalidate either handle or pointer.
        Some(&mut node.element)
    }

    /// Returns a mutable reference to the back element, if any.
    pub fn back_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            return None;
        }

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `node` is exclusive, as guaranteed by `self` being borrowed mutably.
        let node = unsafe { self.tail.resolve_mut(&self.store) };

        //  It is safe to return the reference, as it extends the borrow of `self`, guaranteeing that no operation on
        //  `self.store` will occur which could potentially invalidate either handle or pointer.
        Some(&mut node.element)
    }

    /// Pops the element at the front of the list, if any.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        //  Safety:
        //  -   `self.head` has been allocated by `self.store`.
        //  -   `self.head` is valid, since `length` is not 0.
        //  -   `self.head` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `head` is exclusive, as guaranteed by `self` being borrowed mutably.
        let head = unsafe { self.head.resolve_mut(&self.store) };

        //  Safety:
        //  -   `head.element` is reference.
        //  -   `head.element` will not be used again.
        let element = unsafe { ptr::read(&head.element) };
        let next = head.next;

        //  Safety:
        //  -   `self.head` has been allocated by `self.store`.
        //  -   `self.head` is valid, since `length` is not 0.
        //  -   Access to the resulting `head` is exclusive, as guaranteed by `self` being borrowed mutably.
        unsafe { self.head.deallocate(&self.store) };

        self.head = next;
        self.length -= 1;

        Some(element)
    }

    /// Pops the element at the back of the list, if any.
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `tail` is exclusive, as guaranteed by `self` being borrowed mutably.
        let tail = unsafe { self.tail.resolve_mut(&self.store) };

        //  Safety:
        //  -   `tail.element` is reference.
        //  -   `tail.element` will not be used again.
        let element = unsafe { ptr::read(&tail.element) };
        let prev = tail.prev;

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   Access to the resulting `tail` is exclusive, as guaranteed by `self` being borrowed mutably.
        unsafe { self.tail.deallocate(&self.store) };

        self.tail = prev;
        self.length -= 1;

        Some(element)
    }
}

impl<T, S: StoreMultiple> LinkedList<T, S> {
    /// Pushes an element to the front of the list, unless memory allocation fails.
    pub fn try_push_front(&mut self, element: T) -> Result<(), AllocError> {
        let node = Node {
            element,
            next: self.head,
            prev: NodeHandle::dangling(&self.store),
        };
        let handle = TypedHandle::new(node, &self.store)?;

        self.head = handle;

        if self.is_empty() {
            self.tail = handle;
        }

        self.length += 1;

        Ok(())
    }

    /// Pushes an element to the back of the list, unless memory allocation fails.
    pub fn try_push_back(&mut self, element: T) -> Result<(), AllocError> {
        let node = Node {
            element,
            next: NodeHandle::dangling(&self.store),
            prev: self.tail,
        };
        let handle = TypedHandle::new(node, &self.store)?;

        if !self.is_empty() {
            //  Safety:
            //  -   `self.tail` has been allocated by `self.store`.
            //  -   `self.tail` is valid, since `length` is not 0.
            //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
            //  -   Access to the resulting `tail` is exclusive, as guaranteed by `self` being borrowed mutably.
            let tail = unsafe { self.tail.resolve_mut(&self.store) };

            tail.next = handle;
        } else {
            self.head = handle;
        }

        self.tail = handle;
        self.length += 1;

        Ok(())
    }
}

impl<T, S: StoreStable> LinkedList<T, S> {
    /// Returns an iterator of references to the elements.
    pub fn iter(&self) -> Iter<'_, T, S> {
        Iter {
            length: self.length,
            head: self.head,
            tail: self.tail,
            store: &self.store,
        }
    }

    /// Returns an iterator of mutable references to the elements.
    pub fn iter_mut(&mut self) -> IterMut<'_, T, S> {
        IterMut {
            length: self.length,
            head: self.head,
            tail: self.tail,
            store: &self.store,
        }
    }

    /// Returns a reference to the front element, if any.
    pub fn front(&self) -> Option<&T> {
        if self.is_empty() {
            return None;
        }

        //  Safety:
        //  -   `self.head` has been allocated by `self.store`.
        //  -   `self.head` is valid, since `length` is not 0.
        //  -   `self.head` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `node` is shared, as guaranteed by `self` being borrowed immutably.
        let node = unsafe { self.head.resolve(&self.store) };

        //  It is safe to return the reference, as it extends the borrow of `self`, guaranteeing that `self.store` will
        //  not be moved, in addition to `StoreStable` guaranteeing that no operation on `self.store` will invalidate
        //  either handle or pointer.
        Some(&node.element)
    }

    /// Returns a reference to the back element, if any.
    pub fn back(&self) -> Option<&T> {
        if self.is_empty() {
            return None;
        }

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `node` is shared, as guaranteed by `self` being borrowed immutably.
        let node = unsafe { self.tail.resolve(&self.store) };

        //  It is safe to return the reference, as it extends the borrow of `self`, guaranteeing that `self.store` will
        //  not be moved, in addition to `StoreStable` guaranteeing that no operation on `self.store` will invalidate
        //  either handle or pointer.
        Some(&node.element)
    }
}

impl<T, S: SharingStore> LinkedList<T, S> {
    /// Tries to append the nodes from `other` to `self`.
    ///
    /// On success, the nodes are transferred and `other` is left empty. On failure, `self` and `other` are unmodified.
    ///
    /// Fails if the store of `other` is not sharing with the store of `self`.
    #[allow(clippy::result_unit_err)]
    pub fn try_append(&mut self, other: &mut Self) -> Result<(), ()> {
        if !self.store.is_sharing_with(&other.store) {
            return Err(());
        }

        //  Safety:
        //  -   `self.store` is sharing with `other.store`.
        unsafe { self.append_unchecked(other) };

        Ok(())
    }

    /// Appends the nodes from `other` to `self`, leaving `other` empty.
    ///
    /// #   Safety
    ///
    /// The store from `other` must be sharing with the store from `self`.
    pub unsafe fn append_unchecked(&mut self, other: &mut Self) {
        if other.is_empty() {
            return;
        }

        if self.is_empty() {
            mem::swap(&mut self.length, &mut other.length);
            mem::swap(&mut self.head, &mut other.head);
            mem::swap(&mut self.tail, &mut other.tail);

            return;
        }

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `tail` is exclusive, as guaranteed by `self` being borrowed mutably.
        let tail = unsafe { self.tail.resolve_mut(&self.store) };

        tail.next = other.head;

        //  Safety:
        //  -   `other.head` has been allocated by `other.store`.
        //  -   `other.head` is valid, since `length` is not 0.
        //  -   `other.head` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `head` is exclusive, as guaranteed by `other` being borrowed mutably.
        let head = unsafe { other.head.resolve_mut(&other.store) };

        head.prev = self.tail;

        self.length += other.length;
        other.length = 0;
    }

    /// Splits the list in two at the given index, keeping the first `at` elements in `self` and returning a new list
    /// with the other elements.
    ///
    /// #   Panics
    ///
    /// Panics if `at > self.len()`.
    pub fn split_off(&mut self, at: usize) -> Self
    where
        S: SharingStore<SharingError = !>,
    {
        self.try_split_off(at).into_ok()
    }

    /// Attempts to split the list in two at the given index, keeping the first `at` elements in `self` and returning a
    /// new list with the other elements.
    ///
    /// Returns an error if the store cannot be shared.
    ///
    /// #   Panics
    ///
    /// Panics if `at > self.len()`.
    pub fn try_split_off(&mut self, at: usize) -> Result<Self, S::SharingError> {
        let store = self.store.share()?;

        if at == 0 {
            return Ok(mem::replace(self, Self::new_in(store)));
        }

        if at == self.len() {
            return Ok(Self::new_in(store));
        }

        let new_head = self.nth(at);

        let mut result = Self::new_in(store);
        result.length = self.length - at;
        result.head = new_head;
        result.tail = self.tail;

        //  Safety:
        //  -   `before` has been allocated by `self.store`.
        //  -   `before` is valid, since there are `length` valid handles.
        //  -   `before` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `node` is shared, as guaranteed by `self` being borrowed immutably.
        let new_head = unsafe { new_head.resolve(&self.store) };

        self.length = at;
        self.tail = new_head.prev;

        Ok(result)
    }
}

impl<T: Clone, S: StoreMultiple + StoreStable + Default> Clone for LinkedList<T, S> {
    fn clone(&self) -> Self {
        let mut result = Self::default();

        for item in self {
            result.try_push_back(item.clone()).expect("Sufficient space in store");
        }

        result
    }
}

impl<T: fmt::Debug, S: StoreStable> fmt::Debug for LinkedList<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_list().entries(self).finish()
    }
}

impl<T, S: Store + Default> Default for LinkedList<T, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, S: Store> Drop for LinkedList<T, S> {
    fn drop(&mut self) {
        self.clear();
    }
}

unsafe impl<T, S> Send for LinkedList<T, S>
where
    T: Send,
    S: Store + Send,
{
}

unsafe impl<T, S> Sync for LinkedList<T, S>
where
    T: Sync,
    S: Store + Sync,
{
}

//
//  Comparison
//

impl<T, S, OS> cmp::PartialEq<LinkedList<T, OS>> for LinkedList<T, S>
where
    T: cmp::PartialEq,
    S: StoreStable,
    OS: StoreStable,
{
    fn eq(&self, other: &LinkedList<T, OS>) -> bool {
        self.len() == other.len() && self.iter().eq(other)
    }
}

impl<T, S> cmp::Eq for LinkedList<T, S>
where
    T: cmp::Eq,
    S: StoreStable,
{
}

impl<T: hash::Hash, S: StoreStable> hash::Hash for LinkedList<T, S> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write_length_prefix(self.len());

        for element in self {
            element.hash(state);
        }
    }
}

impl<T, S, OS> cmp::PartialOrd<LinkedList<T, OS>> for LinkedList<T, S>
where
    T: cmp::PartialOrd,
    S: StoreStable,
    OS: StoreStable,
{
    fn partial_cmp(&self, other: &LinkedList<T, OS>) -> Option<cmp::Ordering> {
        self.iter().partial_cmp(other)
    }
}

impl<T, S> cmp::Ord for LinkedList<T, S>
where
    T: cmp::Ord,
    S: StoreStable,
{
    fn cmp(&self, other: &LinkedList<T, S>) -> cmp::Ordering {
        self.iter().cmp(other)
    }
}

//
//  Conversion
//

impl<T, S: StoreMultiple + Default, const N: usize> TryFrom<[T; N]> for LinkedList<T, S> {
    type Error = AllocError;

    fn try_from(value: [T; N]) -> Result<Self, Self::Error> {
        let mut result = LinkedList::new();

        for element in value {
            result.try_push_back(element)?;
        }

        Ok(result)
    }
}

//
//  Iteration
//

impl<'a, T: 'a + Clone, S: StoreMultiple> Extend<&'a T> for LinkedList<T, S> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = &'a T>,
    {
        self.extend(iter.into_iter().cloned());
    }
}

impl<T, S: StoreMultiple> Extend<T> for LinkedList<T, S> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for element in iter {
            self.try_push_back(element).expect("Sufficient space in store");
        }
    }
}

impl<T, S: StoreMultiple + Default> FromIterator<T> for LinkedList<T, S> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let mut result = LinkedList::new();

        for element in iter {
            result.try_push_back(element).expect("Sufficient space in store");
        }

        result
    }
}

impl<T, S: StoreStable> IntoIterator for LinkedList<T, S> {
    type Item = T;
    type IntoIter = IntoIter<T, S>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self)
    }
}

impl<'a, T, S: StoreStable> IntoIterator for &'a LinkedList<T, S> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T, S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, S: StoreStable> IntoIterator for &'a mut LinkedList<T, S> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T, S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Iterator over a linked list.
pub struct IntoIter<T, S: Store>(LinkedList<T, S>);

impl<T, S: StoreStable> Iterator for IntoIter<T, S> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop_front()
    }
}

impl<T, S: StoreStable> DoubleEndedIterator for IntoIter<T, S> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.pop_back()
    }
}

/// Iterator over a reference to a linked list.
pub struct Iter<'a, T, S: Store> {
    //  Only `length` iterators are valid.
    length: usize,
    head: NodeHandle<T, S::Handle>,
    tail: NodeHandle<T, S::Handle>,
    store: &'a S,
}

impl<'a, T: 'a, S: StoreStable> Iterator for Iter<'a, T, S> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.length == 0 {
            return None;
        }

        //  Safety:
        //  -   `self.head` has been allocated by `self.store`.
        //  -   `self.head` is valid, since `length` is not 0.
        //  -   `self.head` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `head` is shared, as guaranteed by the list being borrowed immutably.
        let head = unsafe { self.head.resolve(self.store) };

        let element = &head.element;

        self.head = head.next;
        self.length -= 1;

        Some(element)
    }
}

impl<'a, T: 'a, S: StoreStable> DoubleEndedIterator for Iter<'a, T, S> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.length == 0 {
            return None;
        }

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `tail` is shared, as guaranteed by the list being borrowed immutably.
        let tail = unsafe { self.tail.resolve(self.store) };

        let element = &tail.element;

        self.tail = tail.prev;
        self.length -= 1;

        Some(element)
    }
}

/// Iterator over a mutable reference to a linked list.
pub struct IterMut<'a, T, S: Store> {
    //  Only `length` iterators are valid.
    length: usize,
    head: NodeHandle<T, S::Handle>,
    tail: NodeHandle<T, S::Handle>,
    store: &'a S,
}

impl<'a, T: 'a, S: StoreStable> Iterator for IterMut<'a, T, S> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.length == 0 {
            return None;
        }

        //  Safety:
        //  -   `self.head` has been allocated by `self.store`.
        //  -   `self.head` is valid, since `length` is not 0.
        //  -   `self.head` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `head` is exclusive, as guaranteed by the list being borrowed mutably.
        let head = unsafe { self.head.resolve_mut(self.store) };

        let element = &mut head.element;

        self.head = head.next;
        self.length -= 1;

        Some(element)
    }
}

impl<'a, T: 'a, S: StoreStable> DoubleEndedIterator for IterMut<'a, T, S> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.length == 0 {
            return None;
        }

        //  Safety:
        //  -   `self.tail` has been allocated by `self.store`.
        //  -   `self.tail` is valid, since `length` is not 0.
        //  -   `self.tail` is associated with a memory block containing a valid instance of `Node`.
        //  -   Access to the resulting `tail` is exclusive, as guaranteed by the list being borrowed mutably.
        let tail = unsafe { self.tail.resolve_mut(self.store) };

        let element = &mut tail.element;

        self.tail = tail.prev;
        self.length -= 1;

        Some(element)
    }
}

//
//  Implementation
//

type NodeHandle<T, H> = TypedHandle<Node<T, H>, H>;

struct Node<T, H> {
    element: T,
    //  Possibly dangling or invalid, in the last node of the list.
    next: NodeHandle<T, H>,
    //  Possibly dangling or invalid, in the first node of the list.
    prev: NodeHandle<T, H>,
}

impl<T, S: Store> LinkedList<T, S> {
    //  Returns the n-th handle from the beginning.
    //
    //  #   Panics
    //
    //  Panics if `n >= self.len()`.
    fn nth(&self, n: usize) -> NodeHandle<T, S::Handle> {
        assert!(n < self.len());

        let mut handle = self.head;

        for _ in 0..n {
            //  Safety:
            //  -   `handle` has been allocated by `self.store`.
            //  -   `handle` is valid, since there are at least `n` valid handles.
            //  -   `handle` is associated with a memory block containing a valid instance of `Node`.
            //  -   Access to the resulting `node` is shared, as guaranteed by `self` being borrowed immutably.
            let node = unsafe { handle.resolve(&self.store) };

            handle = node.next;
        }

        handle
    }
}

#[cfg(test)]
mod allocator_tests {
    use std::alloc::Global;

    use super::*;

    type TestList = LinkedList<String, Global>;

    #[test]
    fn list_empty() {
        let list = TestList::new();

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_front() {
        let mut list = TestList::new();

        list.try_push_front(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("0"), list.front().map(|s| s.as_str()));

        if let Some(e) = list.front_mut() {
            e.push('1');
        }

        assert_eq!(Some("01"), list.pop_front().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_front_multiple() {
        let mut list = TestList::new();

        list.try_push_front(String::from("2")).unwrap();
        list.try_push_front(String::from("1")).unwrap();
        list.try_push_front(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(3, list.len());
        assert_eq!(Some("0"), list.front().map(|s| s.as_str()));
        assert_eq!(Some("0"), list.pop_front().as_deref());

        assert!(!list.is_empty());
        assert_eq!(2, list.len());
        assert_eq!(Some("1"), list.front().map(|s| s.as_str()));
        assert_eq!(Some("1"), list.pop_front().as_deref());

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("2"), list.front().map(|s| s.as_str()));
        assert_eq!(Some("2"), list.pop_front().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_back() {
        let mut list = TestList::new();

        list.try_push_back(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("0"), list.back().map(|s| s.as_str()));

        if let Some(e) = list.back_mut() {
            e.push('1');
        }

        assert_eq!(Some("01"), list.pop_back().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_back_multiple() {
        let mut list = TestList::new();

        list.try_push_back(String::from("2")).unwrap();
        list.try_push_back(String::from("1")).unwrap();
        list.try_push_back(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(3, list.len());
        assert_eq!(Some("0"), list.back().map(|s| s.as_str()));
        assert_eq!(Some("0"), list.pop_back().as_deref());

        assert!(!list.is_empty());
        assert_eq!(2, list.len());
        assert_eq!(Some("1"), list.back().map(|s| s.as_str()));
        assert_eq!(Some("1"), list.pop_back().as_deref());

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("2"), list.back().map(|s| s.as_str()));
        assert_eq!(Some("2"), list.pop_back().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_clone() {
        let mut list = TestList::new();

        list.try_push_front(String::from("2")).unwrap();
        list.try_push_front(String::from("1")).unwrap();
        list.try_push_front(String::from("0")).unwrap();

        let mut clone = list.clone();

        assert_eq!(Some("0"), clone.pop_front().as_deref());
        assert_eq!(Some("1"), clone.pop_front().as_deref());
        assert_eq!(Some("2"), clone.pop_front().as_deref());
        assert_eq!(None, clone.pop_front().as_deref());

        assert_eq!(Some("0"), list.pop_front().as_deref());
        assert_eq!(Some("1"), list.pop_front().as_deref());
        assert_eq!(Some("2"), list.pop_front().as_deref());
        assert_eq!(None, list.pop_front().as_deref());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn list_append() {
        let mut list = TestList::new();

        list.try_push_front(String::from("2")).unwrap();
        list.try_push_front(String::from("1")).unwrap();
        list.try_push_front(String::from("0")).unwrap();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{list:?}"));

        let mut other = TestList::new();

        list.try_append(&mut other).unwrap();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{list:?}"));
        assert_eq!(r#"[]"#, format!("{other:?}"));

        other.try_push_front(String::from("5")).unwrap();
        other.try_push_front(String::from("4")).unwrap();
        other.try_push_front(String::from("3")).unwrap();

        list.try_append(&mut other).unwrap();

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));
        assert_eq!(r#"[]"#, format!("{other:?}"));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn list_split_off() {
        let mut list = TestList::new();

        list.try_push_front(String::from("5")).unwrap();
        list.try_push_front(String::from("4")).unwrap();
        list.try_push_front(String::from("3")).unwrap();
        list.try_push_front(String::from("2")).unwrap();
        list.try_push_front(String::from("1")).unwrap();
        list.try_push_front(String::from("0")).unwrap();

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));

        let other = list.split_off(6);

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));
        assert_eq!(r#"[]"#, format!("{other:?}"));

        let mut other = list.split_off(0);

        assert_eq!(r#"[]"#, format!("{list:?}"));
        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{other:?}"));

        list = other.split_off(3);

        assert_eq!(r#"["3", "4", "5"]"#, format!("{list:?}"));
        assert_eq!(r#"["0", "1", "2"]"#, format!("{other:?}"));
    }

    #[test]
    fn list_from_array() {
        let list = TestList::try_from([String::from("0"), String::from("1"), String::from("2")]).unwrap();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_partial_comparison() {
        let one: LinkedList<_, Global> = [0.1, 0.2, 0.3].try_into().unwrap();
        let two: LinkedList<_, Global> = [0.1, 0.2, f32::NAN].try_into().unwrap();

        assert_eq!(one, one);
        assert_ne!(one, two);
        assert_ne!(two, two);

        assert_eq!(Some(cmp::Ordering::Equal), one.partial_cmp(&one));
        assert_eq!(None, one.partial_cmp(&two));
        assert_eq!(None, two.partial_cmp(&two));
    }

    #[test]
    fn list_comparison() {
        let one: TestList = [String::from("0"), String::from("1"), String::from("2")]
            .try_into()
            .unwrap();
        let two: TestList = [String::from("0"), String::from("1"), String::from("3")]
            .try_into()
            .unwrap();

        assert_eq!(one, one);
        assert_ne!(one, two);
        assert_eq!(two, two);

        assert_eq!(cmp::Ordering::Equal, one.cmp(&one));
        assert_eq!(cmp::Ordering::Less, one.cmp(&two));
        assert_eq!(cmp::Ordering::Equal, two.cmp(&two));
        assert_eq!(cmp::Ordering::Greater, two.cmp(&one));
    }

    #[test]
    fn list_extend_clone() {
        let mut list = TestList::try_from([String::from("0"), String::from("1"), String::from("2")]).unwrap();

        list.extend(&[String::from("3"), String::from("4"), String::from("5")]);

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_extend() {
        let mut list = TestList::try_from([String::from("0"), String::from("1"), String::from("2")]).unwrap();

        list.extend([String::from("3"), String::from("4"), String::from("5")]);

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_from_iterator() {
        let list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_into_iter() {
        let list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        let v: Vec<_> = list.into_iter().collect();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{v:?}"));
    }

    #[test]
    fn list_iter() {
        let list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        let v: Vec<_> = list.iter().collect();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{v:?}"));
    }

    #[test]
    fn list_iter_mut() {
        let mut list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        let mut v: Vec<_> = list.iter_mut().collect();

        for e in &mut v {
            e.push('a');
        }

        assert_eq!(r#"["0a", "1a", "2a"]"#, format!("{list:?}"));
    }
} // mod allocator_tests

#[cfg(test)]
mod inline_bump_tests {
    use crate::store::InlineBumpStore;

    use super::*;

    type InlineLinkedList<T, H, const N: usize> = LinkedList<T, InlineBumpStore<H, [Node<T, H>; N]>>;

    type TestList = InlineLinkedList<String, u8, 6>;

    #[test]
    fn list_empty() {
        let list = TestList::new();

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_front() {
        let mut list = TestList::new();

        list.try_push_front(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("0"), list.front().map(|s| s.as_str()));

        if let Some(e) = list.front_mut() {
            e.push('1');
        }

        assert_eq!(Some("01"), list.pop_front().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_front_multiple() {
        let mut list = TestList::new();

        list.try_push_front(String::from("2")).unwrap();
        list.try_push_front(String::from("1")).unwrap();
        list.try_push_front(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(3, list.len());
        assert_eq!(Some("0"), list.front().map(|s| s.as_str()));
        assert_eq!(Some("0"), list.pop_front().as_deref());

        assert!(!list.is_empty());
        assert_eq!(2, list.len());
        assert_eq!(Some("1"), list.front().map(|s| s.as_str()));
        assert_eq!(Some("1"), list.pop_front().as_deref());

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("2"), list.front().map(|s| s.as_str()));
        assert_eq!(Some("2"), list.pop_front().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_back() {
        let mut list = TestList::new();

        list.try_push_back(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("0"), list.back().map(|s| s.as_str()));

        if let Some(e) = list.back_mut() {
            e.push('1');
        }

        assert_eq!(Some("01"), list.pop_back().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_back_multiple() {
        let mut list = TestList::new();

        list.try_push_back(String::from("2")).unwrap();
        list.try_push_back(String::from("1")).unwrap();
        list.try_push_back(String::from("0")).unwrap();

        assert!(!list.is_empty());
        assert_eq!(3, list.len());
        assert_eq!(Some("0"), list.back().map(|s| s.as_str()));
        assert_eq!(Some("0"), list.pop_back().as_deref());

        assert!(!list.is_empty());
        assert_eq!(2, list.len());
        assert_eq!(Some("1"), list.back().map(|s| s.as_str()));
        assert_eq!(Some("1"), list.pop_back().as_deref());

        assert!(!list.is_empty());
        assert_eq!(1, list.len());
        assert_eq!(Some("2"), list.back().map(|s| s.as_str()));
        assert_eq!(Some("2"), list.pop_back().as_deref());

        assert!(list.is_empty());
        assert_eq!(0, list.len());
    }

    #[test]
    fn list_clone() {
        let mut list = TestList::new();

        list.try_push_front(String::from("2")).unwrap();
        list.try_push_front(String::from("1")).unwrap();
        list.try_push_front(String::from("0")).unwrap();

        let mut clone = list.clone();

        assert_eq!(Some("0"), clone.pop_front().as_deref());
        assert_eq!(Some("1"), clone.pop_front().as_deref());
        assert_eq!(Some("2"), clone.pop_front().as_deref());
        assert_eq!(None, clone.pop_front().as_deref());

        assert_eq!(Some("0"), list.pop_front().as_deref());
        assert_eq!(Some("1"), list.pop_front().as_deref());
        assert_eq!(Some("2"), list.pop_front().as_deref());
        assert_eq!(None, list.pop_front().as_deref());
    }

    #[test]
    fn list_from_array() {
        let list = TestList::try_from([String::from("0"), String::from("1"), String::from("2")]).unwrap();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_partial_comparison() {
        let one: InlineLinkedList<f32, u8, 3> = [0.1, 0.2, 0.3].try_into().unwrap();
        let two: InlineLinkedList<f32, u8, 3> = [0.1, 0.2, f32::NAN].try_into().unwrap();

        assert_eq!(one, one);
        assert_ne!(one, two);
        assert_ne!(two, two);

        assert_eq!(Some(cmp::Ordering::Equal), one.partial_cmp(&one));
        assert_eq!(None, one.partial_cmp(&two));
        assert_eq!(None, two.partial_cmp(&two));
    }

    #[test]
    fn list_comparison() {
        let one: TestList = [String::from("0"), String::from("1"), String::from("2")]
            .try_into()
            .unwrap();
        let two: TestList = [String::from("0"), String::from("1"), String::from("3")]
            .try_into()
            .unwrap();

        assert_eq!(one, one);
        assert_ne!(one, two);
        assert_eq!(two, two);

        assert_eq!(cmp::Ordering::Equal, one.cmp(&one));
        assert_eq!(cmp::Ordering::Less, one.cmp(&two));
        assert_eq!(cmp::Ordering::Equal, two.cmp(&two));
        assert_eq!(cmp::Ordering::Greater, two.cmp(&one));
    }

    #[test]
    fn list_extend_clone() {
        let mut list = TestList::try_from([String::from("0"), String::from("1"), String::from("2")]).unwrap();

        list.extend(&[String::from("3"), String::from("4"), String::from("5")]);

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_extend() {
        let mut list = TestList::try_from([String::from("0"), String::from("1"), String::from("2")]).unwrap();

        list.extend([String::from("3"), String::from("4"), String::from("5")]);

        assert_eq!(r#"["0", "1", "2", "3", "4", "5"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_from_iterator() {
        let list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{list:?}"));
    }

    #[test]
    fn list_into_iter() {
        let list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        let v: Vec<_> = list.into_iter().collect();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{v:?}"));
    }

    #[test]
    fn list_iter() {
        let list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        let v: Vec<_> = list.iter().collect();

        assert_eq!(r#"["0", "1", "2"]"#, format!("{v:?}"));
    }

    #[test]
    fn list_iter_mut() {
        let mut list: TestList = [0, 1, 2].iter().map(|i| i.to_string()).collect();

        let mut v: Vec<_> = list.iter_mut().collect();

        for e in &mut v {
            e.push('a');
        }

        assert_eq!(r#"["0a", "1a", "2a"]"#, format!("{list:?}"));
    }
} // mod inline_bump_tests
