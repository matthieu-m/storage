//! An example implementation of a Skip List.
//!
//! The implementation is incomplete, only intended to demonstrate why thin pointers matter.

use core::{
    alloc::Layout,
    cmp,
    marker::PhantomData,
    mem,
    ptr::{self, NonNull},
    slice,
};

use oorandom::Rand32;

use crate::{
    extension::{typed::TypedHandle, typed_metadata::TypedMetadata},
    interface::{MultipleStorage, StableStorage, Storage},
};

/// A Skip List, with minimal memory usage.
pub struct SkipList<K, V, S: Storage> {
    //  Invariant: `length == 0` => `head` is a dangling handle.
    length: usize,
    head: NodeHandle<K, V, S::Handle>,
    storage: S,
    prng: Rand32,
}

impl<K, V, S: Storage> SkipList<K, V, S> {
    /// Creates a new, empty, instance.
    pub fn new() -> Self
    where
        S: Default,
    {
        Self::with_storage(S::default())
    }

    /// Creates a new, empty, instance with the given storage.
    pub fn with_storage(storage: S) -> Self {
        let length = 0;
        let head = TypedHandle::dangling::<S>();
        //  0 is not particularly good; on the allocation of the first node it'll be switched with its address instead.
        let prng = Rand32::new(0);

        Self {
            length,
            head,
            storage,
            prng,
        }
    }

    /// Returns whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Returns the number of nodes in the list.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Clears the list, destroying any node.
    ///
    /// Afterwards, the list is empty.
    pub fn clear(&mut self) {
        if self.length == 0 {
            return;
        }

        //  When `length == 0`, `head` is a dangling handle.
        //
        //  Hence, if a panic occurs during this method, no further attempt at using the handles will occur. This is
        //  safe, at the cost of leaking the existing handles.
        let length = mem::replace(&mut self.length, 0);
        let mut handle = self.head;

        for _ in 0..(length - 1) {
            let next_handle = {
                //  Safety:
                //  -   `handle` has been allocated by `self.storage`.
                //  -   `handle` is valid, since `length` nodes exist.
                //  -   No other reference to the block of memory of `handle` exist, since `self` is borrowed mutably.
                let node = unsafe { handle.resolve_mut(&self.storage) };

                let links = node.links();

                //  Safety:
                //  -   All nodes have at least one link.
                unsafe { *links.get_unchecked(0) }
            };

            //  Safety:
            //  -   `handle` has been allocated by `self.storage`.
            //  -   `handle` is valid, since `length` nodes exist.
            //  -   No other reference to the block of memory of `handle` exist, since `self` is borrowed mutably.
            unsafe { NodeHeader::<K, V, _>::deallocate(handle, &self.storage) };

            handle = next_handle;
        }

        //  Safety:
        //  -   `handle` has been allocated by `self.storage`.
        //  -   `handle` is valid, since `length` nodes exist.
        //  -   No other reference to the block of memory of `handle` exist, since `self` is borrowed mutably.
        unsafe { NodeHeader::<K, V, _>::deallocate(handle, &self.storage) };
    }
}

impl<K, V, S: MultipleStorage + StableStorage> SkipList<K, V, S>
where
    K: Ord,
{
    /// Gets the value associated to a `key`, if it exists.
    pub fn get(&self, key: &K) -> Option<&V> {
        Self::get_impl(key, self.length, self.head, &self.storage).map(|pointer| {
            //  Safety:
            //  -   `pointer` points to a valid instance of `V`.
            //  -   No mutable reference to `V` is active, since `self` is borrowed immutably.
            //  -   The lifetime of the result is tied to that of `self.
            unsafe { pointer.as_ref() }
        })
    }

    /// Gets the value associated to a `key`, if it exists.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        Self::get_impl(key, self.length, self.head, &self.storage).map(|mut pointer| {
            //  Safety:
            //  -   `pointer` points to a valid instance of `V`.
            //  -   No other reference to `V` is active, since `self` is borrowed mutably.
            //  -   The lifetime of the result is tied to that of `self.
            unsafe { pointer.as_mut() }
        })
    }

    /// Inserts a new key and value in the list.
    ///
    /// If a `key` comparing equal is already in the list, it is returned alongside the value it's in with.
    pub fn insert(&mut self, key: K, value: V) -> Option<(K, V)> {
        if self.length == 0 {
            self.head = NodeHeader::new(key, value, 0, &self.storage).0;
            self.length = 1;

            //  Safety:
            //  -   `self.head` was allocated by `self.storage`.
            //  -   `self.head` is still valid.
            let pointer = unsafe { self.head.resolve_raw(&self.storage) };

            let seed = pointer.as_ptr() as usize as u64;

            self.prng = Rand32::new(seed);

            return None;
        }

        let target_links = self.determine_number_links();

        //  There are already elements, so we need to figure out:
        //  -   Whether a node with an equal key exist, and replace it key and value.
        //  -   Otherwise find the pair of nodes between which to allocate this particular node, and link it in.
        //  -   And as a further complication, if the PRNG decides to use more links for this node than the head node
        //      currently has, we need to reallocate the first node with more handles.

        //  Safety:
        //  -   `self.head` was allocated by `self.storage.`
        //  -   `self.head` is still valid, notably it is not dangling per invariant, since `self.length > 0`.
        //  -   No other reference to the block of memory exist, since `self` is borrowed mutably.
        let mut node = unsafe { self.head.resolve_mut(&self.storage) };
        let head_links = node.number_links as usize;

        //  Well, that'll avoid having to reallocate `head`!
        if key < node.key {
            let target_links = cmp::max(target_links, head_links);

            let (node, links) = NodeHeader::new(key, value, target_links, &self.storage);

            links.iter_mut().for_each(|link| *link = self.head);

            self.head = node;
            self.length += 1;

            return None;
        }

        //  And what if the right node is just in front of our eyes?
        if key == node.key {
            let key = mem::replace(&mut node.key, key);
            let value = mem::replace(&mut node.value, value);

            return Some((key, value));
        }

        debug_assert!(key > node.key);

        //  Buffer of handles:
        //  -   For each level in `0..head_links`, a pointer to the handle in the node preceeding the new node, and
        //      pointing to the node following the new node (or dangling).
        //  -   This handle will need to be replaced _if_ the new node is tall enough.
        //
        //  IMPORTANT: if the last node should preceed the new node, they are swapped instead.
        #[allow(clippy::type_complexity)]
        let mut handles: [Option<NonNull<NodeHandle<K, V, S::Handle>>>; MAX_NUMBER_LINKS] = [None; MAX_NUMBER_LINKS];

        let mut last = (head_links == 0).then_some(self.head);

        for level in (0..head_links).rev() {
            //  Advance as far as possible in this level.
            loop {
                let Some(next) = node.links_mut().get_mut(level) else { break };

                //  Safety:
                //  -   `next` was allocated by `self.storage.`
                //  -   `next` is still valid, since apart from `self.head`, only valid handles are kept.
                //  -   No other reference to the block of memory exist, since `self` is borrowed mutably.
                let next_node = unsafe { next.resolve_mut(&self.storage) };

                if key > next_node.key {
                    if next_node.number_links == 0 {
                        last = Some(*next);
                        break;
                    }

                    node = next_node;
                    continue;
                }

                if key == next_node.key {
                    let key = mem::replace(&mut next_node.key, key);
                    let value = mem::replace(&mut next_node.value, value);

                    return Some((key, value));
                }

                debug_assert!(key < next_node.key);

                break;
            }

            debug_assert!(key > node.key);

            handles[level] = Some(NonNull::from(&mut node.links_mut()[level]));
        }

        //  `handles` is now filled, and a new node need be introduced.
        let (mut handle, links) = NodeHeader::new(key, value, target_links, &self.storage);

        //  Splice in the new node, at each level it participates in.
        for (prev_handle, dangling_handle) in handles.iter_mut().take(head_links).zip(links.iter_mut()) {
            let Some(prev_handle) = prev_handle else { continue };

            //  Safety:
            //  -   `prev_handle` points to a readable and writeable block of memory.
            //  -   `prev_handle` points to an initialized handle.
            //  -   No other reference to `prev_handle` is active, since `self` is borrow mutably.
            let prev_handle = unsafe { prev_handle.as_mut() };

            let prev_handle = mem::replace(prev_handle, handle);
            *dangling_handle = prev_handle;
        }

        //  Exchange with last, if it goes beyond last.
        if let Some(mut last) = last {
            //  Safety:
            //  -   `next` was allocated by `self.storage.`
            //  -   `next` is still valid, since apart from `self.head`, only valid handles are kept.
            //  -   No other reference to the block of memory exist, since `self` is borrowed mutably.
            let last_node = unsafe { last.resolve_mut(&self.storage) };

            //  Safety:
            //  -   `handle` was allocated by `self.storage`.
            //  -   `handle` is still valid.
            //  -   No other active reference to the block of memory pointed to by `handle` exists.
            let new_node = unsafe { handle.resolve_mut(&self.storage) };

            mem::swap(&mut last_node.key, &mut new_node.key);
            mem::swap(&mut last_node.value, &mut new_node.value);

            links.iter_mut().for_each(|link| *link = last);
        }

        //  Last is head.
        if head_links == 0 {
            debug_assert!(last.is_some());

            self.head = handle;
            self.length += 1;

            return None;
        }

        //  Reallocate head, if necessary.
        if target_links > head_links {
            //  Safety:
            //  -   `self.head` was allocated by `self.storage`.
            //  -   `self.head` is still valid.
            //  -   No other reference to the block of memory associated with `self.head` is active, since `self` is
            //      borrowed mutably.
            //  -   `head_links` is the number of links of `self.head`.
            //  -   `target_links > head_links`.
            self.head =
                unsafe { NodeHeader::<K, V, _>::grow(self.head, handle, head_links, target_links, &self.storage) };
        }

        self.length += 1;

        None
    }
}

impl<K, V, S: Storage> Drop for SkipList<K, V, S> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<K, V, S> Default for SkipList<K, V, S>
where
    S: MultipleStorage + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

//
//  Implementation
//

const MAX_NUMBER_LINKS: usize = 32;

impl<K, V, S: Storage> SkipList<K, V, S> {
    //  Returns the number of links a (new) node should have.
    fn determine_number_links(&mut self) -> usize {
        (self.prng.rand_u32() | 1).trailing_ones() as usize
    }

    //  #   Safety
    //
    //  -   `handle` must have been allocated by `storage`.
    //  -   `handle` must still be valid.
    unsafe fn resolve_value(handle: NodeHandle<K, V, S::Handle>, storage: &S) -> NonNull<V> {
        //  Safety:
        //  -   `handle` has been allocated by `storage`, as per pre-conditions.
        //  -   `handle` is still valid, as per pre-conditions.
        let pointer = unsafe { handle.resolve_raw(storage) };

        let offset = mem::offset_of!(NodeHeader<K, V, S::Handle>, value);

        //  Safety:
        //  -   `pointer` points to a valid `NodeHeader`.
        //  -   `offset` is an offset within the allocation of `NodeHeader`.
        let pointer = unsafe { pointer.as_ptr().add(offset) };

        //  Safety:
        //  -   `pointer` is not null.
        unsafe { NonNull::new_unchecked(pointer).cast() }
    }
}

impl<K, V, S: MultipleStorage + StableStorage> SkipList<K, V, S>
where
    K: Ord,
{
    fn get_impl(key: &K, length: usize, head: NodeHandle<K, V, S::Handle>, storage: &S) -> Option<NonNull<V>> {
        if length == 0 {
            return None;
        }

        //  Safety:
        //  -   `head` was allocated by `storage.`
        //  -   `head` is still valid, notably it is not dangling per invariant, since `length > 0`.
        //  -   `head` is associated to block of memory containing a live instance of `NodeHeader`.
        let mut node = unsafe { head.resolve(storage) };
        let number_links = node.number_links as usize;

        if *key < node.key {
            return None;
        }

        if *key == node.key {
            //  Safety:
            //  -   `head` was allocated by `storage`.
            //  -   `head` is still valid.
            let value = unsafe { Self::resolve_value(head, storage) };

            return Some(value);
        }

        for level in (0..number_links).rev() {
            //  Advance as far as possible in this level.
            loop {
                let Some(next) = node.links().get(level) else { break };

                //  Safety:
                //  -   `next` was allocated by `storage.`
                //  -   `next` is still valid, since apart from `head`, only valid handles are kept.
                //  -   `next` is associated to block of memory containing a live instance of `NodeHeader`.
                let next_node = unsafe { next.resolve(storage) };

                if *key > next_node.key {
                    node = next_node;
                    continue;
                }

                if *key == next_node.key {
                    //  Safety:
                    //  -   `next` was allocated by `storage`.
                    //  -   `next` is still valid.
                    let value = unsafe { Self::resolve_value(*next, storage) };

                    return Some(value);
                }

                debug_assert!(*key < next_node.key);

                break;
            }
        }

        None
    }
}

type NodeHandle<K, V, H> = TypedHandle<NodeHeader<K, V, H>, H>;

struct NodeHeader<K, V, H> {
    key: K,
    value: V,
    //  A node always has at least 1 link, with the exception of the last node, which always has 0 links.
    number_links: u8,
    _marker: PhantomData<H>,
}

impl<K, V, H> NodeHeader<K, V, H>
where
    H: Copy,
{
    //  Returns the layout of a node with the given number of links, and the offset of the array of links.
    fn layout(number_links: usize) -> (Layout, usize) {
        let layout = Layout::new::<Self>();
        let links = Layout::array::<H>(number_links).expect("Sufficiently small number of links");

        layout.extend(links).expect("Sufficiently small number of links")
    }

    //  Creates a node with `number_links` links, returning a handle to the node and an array of dangling links.
    #[allow(clippy::new_ret_no_self, clippy::type_complexity)]
    fn new<S>(key: K, value: V, number_links: usize, storage: &S) -> (NodeHandle<K, V, H>, &mut [NodeHandle<K, V, H>])
    where
        S: Storage<Handle = H>,
    {
        let (layout, offset) = Self::layout(number_links);

        let (handle, _) = storage.allocate(layout).expect("Allocation to succeed.");

        //  Safety:
        //  -   `handle` was allocated by `storage`, and is still valid.
        let pointer = unsafe { storage.resolve(handle) };

        {
            let number_links: u8 = number_links.try_into().expect("number_links to be sufficiently small");
            let _marker = PhantomData;

            let header = Self {
                key,
                value,
                number_links,
                _marker,
            };

            //  Safety:
            //  -   `pointer` is valid for writes.
            //  -   `pointer` is properly aligned.
            unsafe { ptr::write(pointer.as_ptr() as *mut _, header) };
        }

        //  Safety:
        //  -   `offset + index * size` is within bounds, since the calculation of the layout succeeded.
        let pointer = unsafe { pointer.as_ptr().add(offset) as *mut NodeHandle<K, V, H> };

        for index in 0..number_links {
            //  Safety:
            //  -   `offset + index * size` is within bounds, since the calculation of the layout succeeded.
            let link = unsafe { pointer.add(index) };

            //  Safety:
            //  -   `link` is valid for writes.
            //  -   `link` is properly aligned.
            unsafe { ptr::write(link, NodeHandle::dangling::<S>()) };
        }

        //  Safety:
        //  -   `pointer` is valid for both reads and writes for `number_links` elements.
        //  -   Access to `links` is exclusive, as the memory is freshly allocated.
        let links = unsafe { slice::from_raw_parts_mut(pointer, number_links) };

        let handle = NodeHandle::from_raw_parts(handle, TypedMetadata::default());

        (handle, links)
    }

    //  #   Safety
    //
    //  -   `handle` must have been allocated by `storage`.
    //  -   `handle` must still be valid.
    //  -   No other reference to its block of memory is active.
    //  -   `old_number_links` must match the previous number of links.
    //  -   `new_number_links` must be strictly greater than `old_number_links`.
    unsafe fn grow<S>(
        handle: NodeHandle<K, V, H>,
        with: NodeHandle<K, V, H>,
        old_number_links: usize,
        new_number_links: usize,
        storage: &S,
    ) -> NodeHandle<K, V, H>
    where
        S: Storage<Handle = H>,
    {
        let (old_layout, offset) = Self::layout(old_number_links);
        let (new_layout, _) = Self::layout(new_number_links);

        //  Safety:
        //  -   `handle` has been allocated by `storage`.
        //  -   `handle` is still valid.
        //  -   No other reference to its block of memory is active.
        //  -   `old_layout` fits the block of memory associated with `handle`.
        //  -   `new_layout` is greater than `old_layout`.
        let (handle, _) = unsafe {
            storage
                .grow(handle.to_raw_parts().0, old_layout, new_layout)
                .expect("Allocation to succeed")
        };

        //  Safety:
        //  -   `handle` was allocated by `storage`, and is still valid.
        let pointer = unsafe { storage.resolve(handle) };

        {
            //  Safety:
            //  -   `pointer` points to a readable and writeable area of memory.
            //  -   `pointer` points to an initialized area of memory of `Self` type.
            //  -   No other reference to this area of memory is active.
            let this: &mut Self = unsafe { pointer.cast().as_mut() };

            this.number_links = new_number_links
                .try_into()
                .expect("new_number_links to be sufficiently small");
        }

        //  Safety:
        //  -   `offset + index * size` is within bounds, since the calculation of the layout succeeded.
        let pointer = unsafe { pointer.as_ptr().add(offset) as *mut NodeHandle<K, V, H> };

        for index in old_number_links..new_number_links {
            //  Safety:
            //  -   `offset + index * size` is within bounds, since the calculation of the layout succeeded.
            let link = unsafe { pointer.add(index) };

            //  Safety:
            //  -   `link` is valid for writes.
            //  -   `link` is properly aligned.
            unsafe { ptr::write(link, with) };
        }

        NodeHandle::from_raw_parts(handle, TypedMetadata::default())
    }

    //  #   Safety
    //
    //  -   `handle` must have been allocated by `storage`.
    //  -   `handle` must still be valid.
    //  -   `handle` must be associated to a block of memory containing a live instance of `NodeHeader`.
    //  -   No other reference to its block of memory is active.
    unsafe fn deallocate<S>(mut handle: NodeHandle<K, V, H>, storage: &S) -> (K, V)
    where
        S: Storage<Handle = H>,
    {
        //  Safety:
        //  -   `handle` was allocated by `storage`, and is still valid, as per pre-conditions.
        //  -   `handle` is associated to a block of memory containing a live instance of `NodeHeader`, as per
        //      pre-conditions.
        //  -   No other reference to its block of memory is active, as per pre-conditions.
        let this = unsafe { handle.resolve_mut(storage) };

        //  Safety:
        //  -   `this.key` and `this.value` are valid for reads.
        //  -   `this.key` and `this.value` are properly aligned.
        //  -   The values are initialized, and will no longer be used.
        let key = unsafe { ptr::read(&this.key) };
        let value = unsafe { ptr::read(&this.value) };
        let number_links: usize = this.number_links.into();

        let (layout, _) = Self::layout(number_links);

        //  Safety:
        //  -   `handle` was allocated by `storage`.
        //  -   `handle` is still valid.
        //  -   `layout` fits the block of memory.
        unsafe { storage.deallocate(handle.to_raw_parts().0, layout) };

        (key, value)
    }

    fn links(&self) -> &[NodeHandle<K, V, H>] {
        let number_links: usize = self.number_links.into();

        if number_links == 0 {
            return &[];
        }

        let (_, offset) = Self::layout(number_links);

        //  Safety:
        //  -   `offset` is within bounds, since the node was allocated.
        let first = unsafe { (self as *const Self as *const u8).add(offset) };

        //  Safety:
        //  -   The pointer is properly aligned.
        //  -   The pointer is dereferenceable.
        //  -   The pointer points to an initialized instance of `[NodeHandle<K, V, H>]`.
        //  -   The slice is accessible in shared mode, since `self` is, and its lifetime is bound to `self`.
        unsafe { slice::from_raw_parts(first as *const NodeHandle<K, V, H>, number_links) }
    }

    fn links_mut(&mut self) -> &mut [NodeHandle<K, V, H>] {
        let number_links: usize = self.number_links.into();

        if number_links == 0 {
            return &mut [];
        }

        let (_, offset) = Self::layout(number_links);

        //  Safety:
        //  -   `offset` is within bounds, since the node was allocated.
        let first = unsafe { (self as *mut Self as *mut u8).add(offset) };

        //  Safety:
        //  -   The pointer is properly aligned.
        //  -   The pointer is dereferenceable.
        //  -   The pointer points to an initialized instance of `[NodeHandle<K, V, H>]`.
        //  -   The slice is accessible in exclusive mode, since `self` is, and its lifetime is bound to `self`.
        unsafe { slice::from_raw_parts_mut(first as *mut NodeHandle<K, V, H>, number_links) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::collection::utils::Global;

    type GlobalList = SkipList<i32, String, Global>;

    #[test]
    fn empty() {
        let list = GlobalList::default();

        assert!(list.is_empty());
        assert_eq!(0, list.len());
        assert_eq!(None, list.get(&0));
    }

    #[test]
    fn insert_single() {
        let mut list = GlobalList::default();

        list.insert(0, String::from("0"));

        assert!(!list.is_empty());
        assert_eq!(1, list.len());

        assert_eq!(None, list.get(&-1));
        assert_eq!(Some(&String::from("0")), list.get(&0));
        assert_eq!(None, list.get(&1));

        let Some(v) = list.get_mut(&0) else { unreachable!() };

        v.push('0');

        assert_eq!(Some(&String::from("00")), list.get(&0));
    }

    //  MIRI does not like the idea of borrowing the "tail" links from the header, due to the original borrow of the
    //  header not encompassing the tail.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn insert_front() {
        let mut list = GlobalList::default();

        list.insert(1, String::from("1"));

        assert_eq!(1, list.len());

        list.insert(0, String::from("0"));

        assert_eq!(2, list.len());

        assert_eq!(None, list.get(&-1));
        assert_eq!(Some(&String::from("0")), list.get(&0));
        assert_eq!(Some(&String::from("1")), list.get(&1));
        assert_eq!(None, list.get(&2));
    }

    //  MIRI does not like the idea of borrowing the "tail" links from the header, due to the original borrow of the
    //  header not encompassing the tail.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn insert_back() {
        let mut list = GlobalList::default();

        list.insert(0, String::from("0"));

        assert_eq!(1, list.len());

        list.insert(1, String::from("1"));

        assert_eq!(2, list.len());

        assert_eq!(None, list.get(&-1));
        assert_eq!(Some(&String::from("0")), list.get(&0));
        assert_eq!(Some(&String::from("1")), list.get(&1));
        assert_eq!(None, list.get(&2));
    }
} // mod tests
