Feature Name:   storage
Start Date:     2023-04-29
RFC PR:         ...
Rust Issue:     ...


#   Summary

Storage offers a more flexible allocation API, suitable for in-line memory storage, shared memory storage, compaction
of allocations, and more.

A companion repository implementing the APIs presented here, and using them, can be explored at
https://github.com/matthieu-m/storage.


#   Motivation

The Allocator API supports many usecases, but unfortunately falls short in a number of scenarios, due to the use of
pointers.

Specifically:

-   Pointers preclude in-line memory storage, ie, an allocator cannot return a pointer pointing within the allocator
    itself, as any move of the allocator instance invalidates the pointer.
-   Pointers (other than null, dangling, or to static variables) cannot be returned from a const context, preventing
    the use of non-empty regular collections in const or static variables.
-   Pointers are often virtual addresses, preventing the use of non-empty regular collections in shared memory.
-   Pointers are often 32 to 64 bits, which is overkill in many situations.

The key idea of the Storage API is to do away with pointers and instead return abstract, opaque, handles which can be
tailored to fit the particular restrictions of a given scenario.


#   Guide-level explanation

##  Overview

The `Storage` trait is designed to allow allocating blocks of memory and referring to them by opaque handles. The
handles are not meant to be exposed directly, instead the `Storage` should be used to parameterize a collection which
will internally use the storage provided, and its handles, to allocate and deallocate memory as needed.

The `Storage` API is very closely related to the `Allocator` API, and largely mirrors it. The important exceptions are:

-   The `Handle` returned are opaque, and must be resolved into pointers by the instance of `Storage` which allocated
    them.
-   Unless a specific storage type implements `MultipleStorage`, any handle it allocated may be invalidated when the
    storage performs a new allocation.
-   Unless a specific storage type implements `StableStorage`, there is no guarantee that resolving the same handle
    after calling another method on the API -- including `resolve` with a different handle -- will return the same
    pointer. In particular, a call to `resolve` may lead to cache-eviction (think LRU), an allocation may result in
    reallocating the entire block of memory used underneath by the `Storage`, and a deallocation may result in
    consolidating existing allocations (GC style).
-   Unless a specific storage type implements `PinningStorage`, there is no guarantee that resolving the same handle
    after moving the storage will return the same pointer.


##  Points of View

There are 3 point of views when it comes to using the `Storage` API:

-   The user, who gets to mix and match collection and storage based on their usecase.
-   The implementer of a collection parameterized over `Storage`.
-   The implementer of a `Storage`.

Check each section according to your usecase.


##  User Guide

As a developer for latency-sensitive code, using an in-line storage allows me to avoid the latency uncertainty of
memory allocations, as well as the extra latency uncertainty of accessing a different cache line.

This is as simple as parameterizing the collection I wish to use with an appropriate in-line storage.

```rust
use core::{collections::Vec, string::String};

//  A storage parameterized by a type `T`, which provides a single block of memory suitable for `T`, that is: at least
//  aligned for `T` and sized for `T`.
use third_party::InlineSingleStorage;

type InlineString<const N: usize> = String<InlineSingleStorage<[u8; N]>>;
type InlineVec<T, const N: usize> = Vec<T, InlineSingleStorage<[T; N]>>;

//  A struct keeping the N greatest values of `T` submitted, and discarding all others.
pub struct MaxPick<T, const N: usize>(InlineVec<T, N>);

impl<T, const N: usize> MaxPick<T, N> {
    pub fn new() -> Self {
        Self(InlineVec::with_capacity(N))
    }

    pub fn as_slice(&self) -> &[T] { &self.0 }

    pub fn clear(&mut self) { self.clear(); }
}

impl<T: Ord, const N: usize> MaxPick<T, N> {
    pub fn add(&mut self, value: T) {
        if N == 0 {
            return;
        }

        if let Some(last) = self.0.get(N - 1) {
            if *last >= value {
                return;
            }

            self.0.pop();
        }

        self.0.push_within_capacity(value);
        self.0.sort();
    }
}
```

As a developer for performance-sensitive code, using a small storage allows me to avoid the cost of memory allocations
in the majority of cases, whilst retaining the flexibility of unbounded allocations when I need them.

```rust
use std::future::Future;

//  A storage parameterized by a type `T`, which provides an in-line block of memory suitable for `T` -- that is at
//  least aligned for `T` and sized for `T` -- and otherwise defaults to a heap allocation.
use third_party::SmallSingleStorage;

//  A typed-erased future:
//  -   If the future fits within `[usize; 3]`, apart from its metadata, no memory allocation is performed.
//  -   Otherwise, the global allocator is used.
pub type RandomFuture = Box<dyn Future<Output = i32>, SmallSingleStorage<[usize; 3]>>;

pub trait Randomizer {
    fn rand(&self) -> RandomFuture;
}

pub struct FairDie;

impl Randomizer for FairDie {
    fn rand(&self) -> RandomFuture {
        Box::new(async { 4 })
    }
}

pub struct CallHome;

impl Randomizer for CallHome {
    fn rand(&self) -> RandomFuture {
        Box::new(async {
            //  Connect to https://example.com
            //  Hash the result.
            //  Truncate the hash to fit.
            todo!()
        })
    }
}
```

In either case, this allows me to reuse battle-tested collections, and all the ecosystem built around them, rather than
having to implement, or depend on, ad-hoc specialized collections which tend to lag behind in terms of soundness and/or
features.

It also allows me to use the APIs I am used to, rather than slightly different APIs for each specific situation,
thereby allowing me to extert maximum control and extract maximum performance from my code without compromising my
productivity.


##  Collection Implementer Guide

As an implementer of collection code, using the `Storage` abstraction gives maximum flexibility to my users as to how
they'll be able to use my collection.

```rust
pub struct Either<L, R, S: Storage> {
    is_left: true,
    handle: S::Handle,
    storage: ManuallyDrop<S>,
}

impl<L, R, S: Storage> Either<L, R, S> {
    pub fn left(value: L) -> Result<Self, AllocError>
    where
        S: Default,
    {
        let storage = ManuallyDrop::new(S::default());
        let (handle, _) = storage.allocate(Layout::new::<L>())?;

        //  Safety:
        //  -   `handle` was allocated by `storage`.
        //  -   `handle` is still valid.
        let pointer = unsafe { storage.resolve(handle) };

        //  Safety:
        //  -   `pointer` points to a block of memory fitting `value`.
        //  -   `pointer` points to a writeable block of memory.
        unsafe { ptr::write(pointer.cast().as_ptr(), value) };

        Ok(Self { is_left: true, handle, storage })
    }

    pub fn as_left(&self) -> Option<&L> {
        self.is_left.then(|| {
            //  Safety:
            //  -   `handle` was allocated by `storage`.
            //  -   `handle` is still valid.
            let pointer = unsafe { self.storage.resolve(self.handle) };

            //  Safety:
            //  -   `pointer` points to a live instance of `L`.
            //  -   The reference will remain valid for its entire lifetime, since it borrows `self`, thus preventing
            //      any allocation via or move or destruction of `self.storage`.
            //  -   No mutable reference to this instance exists, nor will exist during the lifetime of the resulting
            //      reference, since the reference borrows `self`.
            unsafe { pointer.as_ref() }
        })
    }

    pub fn into_left(mut self) -> Option<core::boxed::Box<L, S>> {
        self.is_left.then(|| {
            let handle = self.handle;

            //  Safety:
            //  -   `self.storage` will no longer be used.
            let storage = unsafe { ManuallyDrop::take(&mut self.storage) };

            mem::forget(self);

            //  Safety:
            //  -   `handle` was allocated by `storage`.
            //  -   `handle` is valid.
            //  -   The block of memory associated to `handle` contains a live instance of `L`.
            unsafe { core::boxed::Box::from_raw_parts(handle, storage) }
        })
    }

    //  And implementations of `as_left_mut`, `right`, `as_right`, `as_right_mut`, ...
}

impl<L, R, S: Storage> Drop for Either<L, R, S> {
    fn drop(&mut self) {
        //  Safety:
        //  -   `handle` was allocated by `storage`.
        //  -   `handle` is still valid.
        let pointer = unsafe { self.storage.resolve(self.handle) };

        if self.is_left {
            let pointer: *mut L = pointer.cast().as_ptr();

            //  Safety:
            //  -   `pointer` is valid for both reads and writes.
            //  -   `pointer` is properly aligned.
            unsafe { ptr::drop_in_place(pointer) }
        } else {
            let pointer: *mut R = pointer.cast().as_ptr();

            //  Safety:
            //  -   `pointer` is valid for both reads and writes.
            //  -   `pointer` is properly aligned.
            unsafe { ptr::drop_in_place(pointer) }
        };

        let layout = if self.is_left {
            Layout::new::<L>()
        } else {
            Layout::new::<R>()
        };

        //  Safety:
        //  -   `self.storage` will no longer be used.
        let storage = unsafe { ManuallyDrop::take(&mut self.storage) };

        //  Safety:
        //  -   `self.handle` was allocated by `self.storage`.
        //  -   `self.handle` is still valid.
        //  -   `layout` fits the block of memory associated to `self.handle`.
        unsafe { storage.deallocate(self.handle, layout) }
    }
}
```

By using `Storage`, I empower my users to use my type in a wide variety of scenarios, some of which I cannot even
fathom.

The overhead of using `Storage` over `Allocator` is also fairly low, so that the added flexibility comes at little to
no cost to me.


##  Storage Implementer Guide

The API of `Storage` requires internal mutability, and that's it. I can otherwise provide as few or as many guarantees
as I wish.

```rust
/// An implementation of `Storage` providing a single, in-line, block of memory.
///
/// The block of memory is aligned and sized as per `T`.
pub struct InlineSingleStorage<T>(UnsafeCell<MaybeUninit<T>>);

impl<T> Default for InlineSingleStorage<T> {
    fn default() -> Self {
        Self(UnsafeCell::new(MaybeUninit::uninit()))
    }
}

unsafe impl<T> Storage for InlineSingleStorage<T> {
    type Handle = ();

    fn dangling(&self) -> Self::Handle {}

    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Self::validate_layout(layout)?;

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn deallocate(&self, _handle: Self::Handle, _layout: Layout) {}

    unsafe fn resolve(&self, _handle: Self::Handle) -> NonNull<u8> {
        let pointer = self.0.get();

        //  Safety:
        //  -   `self` is non null.
        unsafe { NonNull::new_unchecked(pointer) }.cast()
    }

    unsafe fn grow(
        &self,
        _handle: Self::Handle,
        _old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= _old_layout.size(),
            "{new_layout:?} must have a greater size than {_old_layout:?}"
        );

        Self::validate_layout(new_layout)?;

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn shrink(
        &self,
        _handle: Self::Handle,
        _old_layout: Layout,
        _new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            _new_layout.size() >= _old_layout.size(),
            "{_new_layout:?} must have a smaller size than {_old_layout:?}"
        );

        Ok(((), mem::size_of::<T>()))
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        Self::validate_layout(layout)?;

        let pointer = self.0.get() as *mut u8;

        //  Safety:
        //  -   `pointer` is valid, since `self` is valid.
        //  -   `pointer` points to at an area of at least `mem::size_of::<T>()`.
        //  -   Access to the next `mem::size_of::<T>()` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer, 0, mem::size_of::<T>()) };

        Ok(((), mem::size_of::<T>()))
    }

    unsafe fn grow_zeroed(
        &self,
        _handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "{new_layout:?} must have a greater size than {old_layout:?}"
        );

        Self::validate_layout(new_layout)?;

        let pointer = self.0.get() as *mut u8;

        //  Safety:
        //  -   Both starting and resulting pointers are in bounds of the same allocated objects as `old_layout` fits
        //      `pointer`, as per the pre-conditions of `grow_zeroed`.
        //  -   The offset does not overflow `isize` as `old_layout.size()` does not.
        let pointer = unsafe { pointer.add(old_layout.size()) };

        //  Safety:
        //  -   `pointer` is valid, since `self` is valid.
        //  -   `pointer` points to at an area of at least `mem::size_of::<T>() - old_layout.size()`.
        //  -   Access to the next `mem::size_of::<T>() - old_layout.size()` bytes is exclusive.
        unsafe { ptr::write_bytes(pointer, 0, mem::size_of::<T>() - old_layout.size()) };

        Ok(((), mem::size_of::<T>()))
    }
}

//  Safety:
//  -   `self.resolve(handle)` always returns the same address, as long as `self` doesn't move.
unsafe impl<T> StableStorage for InlineSingleStorage<T> {}

impl<T> fmt::Debug for InlineSingleStorage<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let layout = Layout::new::<T>();

        f.debug_struct("InlineSingleStorage")
            .field("size", &layout.size())
            .field("align", &layout.align())
            .finish()
    }
}

impl<T> InlineSingleStorage<T> {
    fn validate_layout(layout: Layout) -> Result<(), AllocError> {
        let own = Layout::new::<T>();

        if layout.align() <= own.align() && layout.size() <= own.size() {
            Ok(())
        } else {
            Err(AllocError)
        }
    }
}
```

And that's it!

I need not implement `MultipleStorage`, and thus do not have to track allocations and deallocations. And I need not
implement `PinningStorage`, and thus do not have to ensure memory address stability across moves.


#   Reference-level explanation

##  Overview

This RFC introduces 4 new traits.

The core of this RFC is the `Storage` trait:

```rust
pub unsafe trait Storage {
    /// Handle to a block of memory.
    type Handle: Copy;

    /// Returns a dangling handle, always invalid.
    fn dangling() -> Self::Handle;

    /// Return a pointer to the block of memory associated to `handle`.
    unsafe fn resolve(&self, handle: Self::Handle) -> NonNull<u8>;

    //  The following methods are similar to `Allocate`, reformulated in terms of `Self::Handle`.

    /// Allocates a new handle to a block of memory. On success, invalidates any existing handle.
    fn allocate(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError>;

    /// Deallocates a handle.
    unsafe fn deallocate(&self, handle: Self::Handle, layout: Layout);

    /// Grows the block of memory associated to a handle. On success, the handle is invalidated.
    unsafe fn grow(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError>;

    /// Shrinks the block of memory associated to a handle. On success, the handle is invalidated.
    unsafe fn shrink(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError>;

    /// Allocates a new handle to a block of zeroed memory. On success, invalidates any existing handle.
    fn allocate_zeroed(&self, layout: Layout) -> Result<(Self::Handle, usize), AllocError> {
        ...
    }

    /// Grows the block of memory associated to a handle with zeroed memory. On success, the handle is invalidated.
    unsafe fn grow_zeroed(
        &self,
        handle: Self::Handle,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(Self::Handle, usize), AllocError> {
        ...
    }
}
```

_Note:  full-featured documentation can be found in the companion repository at_
        https://github.com/matthieu-m/storage/blob/main/src/interface.rs.

The `Storage` trait is supplemented by 3 additional marker traits, providing extra guarantees:

```rust
/// A refinement of `Storage` which does not invalidate existing handles on allocation, and does not invalidate
/// unrelated existing handles on deallocation.
pub unsafe trait MultipleStorage: Storage {}

/// A refinement of `Storage` which does not invalidate existing pointers on allocation, resolution, or deallocation,
/// but may invalidate them on moves.
pun unsafe trait StableStorage: Storage {}

/// A refinement of `Storage` which does not invalidate existing pointers, not even on moves. That is, this refinement
/// guarantees that the blocks of memory are pinned in memory.
pub unsafe trait PinningStorage: StableStorage {}
```


##  Library Organization

This RFC proposes to follow the lead of the `Allocator` trait, and add the `Storage` traits to the `core` crate, either
in the `alloc` module or in a new `storage` module.

It leaves to a follow-up RFC the introduction of a `storage` or `storage-collections` crate which would contain the
code of the various standard collections: `Box`, `Vec`, `String`, `BinaryHeap`, `BTreeMap`, `BTreeSet`, `LinkedList`,
and `VecDeque`, all adapted for the `Storage` API.

Those types would then be re-exported as-is in the `alloc` crate, drastically reducing its size.


#   Drawbacks

This RFC increases the surface of the standard library, with yet another `Allocator`.

Furthermore, the natural consequence of adopting this RFC would be rewriting the existing collections in terms of
`Storage`, rather than `Allocator`. A mostly mechanical task, certainly, but an opportunity to introduce subtle bugs in
the process, even if MIRI would hopefully catch most such bugs.

Finally, having two allocator-like APIs, `Storage` and `Allocator`, means that users will forever wonder which trait
they should implement, and which trait they should use when implementing a collection.


#   Rationale and Alternatives

##  Don't Repeat Yourself.

The fact that `Allocator` is unsuitable for many usecases is amply demonstrated by the profileration of ad-hoc rewrites
of existing `std` types for particular scenarios. A non-exhaustive list of crates seeking to work around those short-
comings today is presented here:

-   https://crates.io/crates/arraystring
-   https://crates.io/crates/arrayvec
-   https://crates.io/crates/const-arrayvec
-   https://crates.io/crates/const_lookup_map
-   https://crates.io/crates/generic-vec
-   https://crates.io/crates/phf
-   https://crates.io/crates/smallbox2
-   https://crates.io/crates/stackbox
-   https://crates.io/crates/stackfuture
-   https://crates.io/crates/string-wrapper
-   https://crates.io/crates/toad-string

Those are the alternatives to `Storage`: rather than adapting a data-structure to be flexible enough to be used in
various situations, the entire data-structure is copy/pasted and then tweaked as necessary or re-implemented. The
downsides are inherent to any violation of DRY:

-   Bugs or soundness issues may be introduced, or may not be fixed when fixed in the "original".
-   The new types are not compatible with APIs taking the standard types.
-   The new types do not implement the latest features implemented by the standard types.
-   The new types do not implement all the traits implemented by the standard types, most especially 3rd-party traits.

There are a few advantages to brand new types. For example, a nul-terminated in-line string does not have the 16 bytes
overhead that a "naive" `String<InlineSingleStorage<[u8; N]>>` would have. The Storage API will not eliminate the
potential need for such specialized collections, but it may eliminate the need for many alternatives.


##  Allocator-like

The API of `Storage` is intentionally kept very close to that of `Allocator`.

This similarility, and the similarity of the safety requirements, means that any developer accustomed to the current
`Allocator` API can quickly dive in, and also means that bridging between `Storage` and `Allocator` is as easy as
possible.

There are 3 extra pieces:

-   The `Handle` associated type is used, rather than `NonNull<u8>`. This is the key to the flexibility of `Storage`.
-   The `dangling` method, reminiscent of `NonNull::dangling`, and to be used for the same purposes. It is part of
    `Storage` to simplify the overall API, as otherwise another trait would be required for `Handle`.
-   The `resolve` method, which bridges from `Handle` to `NonNull<u8>`, since access to the allocated blocks of
    memory require pointers to them.

Otherwise, the bulk of the API is a straightforward translation of the `Allocator` API, substituting `Handle` anytime
`NonNull<_>` appears.


##  Guarantees, or absence thereof

The `Storage` API is minimalist, providing a minimum of guarantees.

Beyond being untyped and unchecked, there are also a few oddities, compared to the `Allocator` API:

-   By default, using `Storage::allocate` or `Storage::allocate_zeroed` invalidates all existing handles. This oddity
    stems from the requirement of minimizing the state to be stored in collections using a single allocation at a time
    such as `Box`, `Vec`, `VecDeque`, ...
-   By default, calling any method -- including `resolve` -- invalidates all resolved pointers. This oddity stems from
    the desire to leave the API flexible enough to allow caching storages, single-allocation storages, or copying
    storages.
-   By default, moving `Storage` invalidates all resolved pointers. This oddity stems from the fact that when using an
    in-line storage the pointers point within the block of memory covered by the storage, and thus after it moved, are
    left pointing into the void.

When the above should be guaranteed, extra marker traits can be implemented to provide compile-time checking that these
properties hold, which in turn allows the final user to safely mix and match collection and storage, relying on compiler
errors to prevent erroneous couplings.


##  Mutable Storage

A previous incarnation of the API borrowed the storage mutably to allocate, deallocate, grow, or shrink.

This is tempting, as after all it is likely something within the storage will need to be mutated.

There is, however, a very good reason for `Allocator` to use a shared reference: concurrent uses. In a concurrent
context, requiring a mutable reference to the storage requires a locking mechanism around the storage. Even if the
storage is `Sync`.

To fully support concurrent code with zero overhead, the `Storage` API methods cannot accept `&mut self`.


##  Owned Storage

A third possibility -- beyond accepting `&self` and `&mut self` -- is of course to accept `self` and implement the
`Storage` trait for reference types, as appropriate.

This would require a `Sync` type to implement `Storage` twice (once for immutable references, and once for mutable
references) and would make it more difficult to declare bounds when using it.


##  Typed Handles

A previous incarnation of the API used GATs to provide typed handles.

This is tempting, but I now deem it a mistake, most notably thanks to discussions with @CAD97 on the topic.

Specifically:

1.  A user may wish to allocate raw memory, for example to further parcel it themselves. Thus any API must, in any
    case, offer the ability to allocate raw memory. Providing a typed API on top means doubling the API surface.
2.  Typing can easily be added externally. See the `TypedHandle` possible future extension, which is non-intrusive
    and can be implemented in 3rd-party code.

And the final nail in the coffin, for me, is that even typed handles would not make the API safe. There are many other
invariants to respect -- handle invalidation, pointer invalidation, liveness of the value, borrow-checking -- which
would require the `unsafe` methods to remain `unsafe`.

In comparison to tracking all that, types are a minor concern: in most collections, there's a single type _anyway_.


##  Pointer vs Reference

A previous incarnation of the API provided borrow-checking. That is, resolving a handle would yield a reference and
appropriately borrow the storage.

This is tempting, but I deem it a mistake.

Specifically:

1.  A mutably borrowed storage cannot allocate, deallocate, nor further resolve any other reference. This makes
    implementing any recursive data-structure -- such as a tree -- quite a bit more challenging than it ought to be.
2.  A reference requires a type, for a fat reference this means metadata. Requiring the metadata to be provided when
    calling `resolve` precludes the use of thin handles, which are quite welcome in graphs of objects with a great
    number of copies of each handle.

And the final nail in the coffin, for me, is that borrow-checking is best provided at the _handle_ level, rather than
at the storage level. The `UniqueHandle` possible future extension, which is non-intrusive and can be implemented in
3rd-party code:

-   Borrows the (unique) handle mutably or immutably, ensuring no misgotten access is provided.
-   Borrows the storage immutably, ensuring it is not moved nor dropped, which would invalidate the pointers.

This solution is more flexible, and more minimalist, generally a good sign with regard to API design.


##  Argument-less dangling method

A previous version of the companion repository used an argument-less `Storage::dangling` method.

The main advantage is that no instance of `Storage` is then necessary to create an associated dangling handle. The
somewhat hidden cost, however, is that `Storage` is then no longer dyn-safe.

An intermediate solution to restore dyn-safety would be a where clause `Self: Sized`, but while this would indeed make
`Storage` dyn-safe, it would still result in only providing partial functionality. This seems clearly undesirable.

In the absence of strong usecase for creating dangling handles in the absence of an instance of `Storage`, it seems
preferable to have `Storage::dangling` take `&self` so that `dyn Storage` be fully functional.


##  Adapter vs Blanket Implementation

A previous version of the companion repository used an `AllocatorStorage` adapter struct, instead of a blanket
implementation.

There does not seem to be any benefit to doing so, and it prevents using collections defined in terms of a `Storage`
with an `Allocator`, which would require wrapping all storage-based collections in allocator-based adapters in the
standard library... and duplicate their documentation. Pure overhead.


##  Marker granularity

As a reminder, there are 3 marker traits:

-   `MultipleStorage`: allows concurrent allocations from a single storage.
-   `StableStorage`: ensures existing pointers remain valid across calls to the API methods.
-   `PinningStorage`: ensures existing pointers remain valid even across moves.


Those traits could be merged, or further split.

I would suggest not splitting any further now. Taken to the extreme a marker trait could be introduced for each
guarantee and each operation, for a total of 10 marker traits: 2 guarantees x 4 groups of methods + 2 guarantees on
moves. Such a fine-grained approach is used in C++, and I remember writing generic methods which would static-assert
that the elements they manipulate need be noexcept-movable, noexcept-move-assignable, and noexcept-destructible, then
further divide the method based on whether the elements were noexcept-copyable and trivially copyable. There always is
the nagging doubt of having missed one key guarantee, and therefore while conductive to writing finely tuned code, it
is unfortunately not conductive to writing robust code: the risk of error is too high.

The current set of traits has thus been conceived to provide a reasonable trade-off:

-   A small enough number of markers that developers of collections are not overwhelmed, and thus less likely to miss
    a key requirement leading to unsoundness in their unsafe code.
-   A split on "natural" boundaries: one hierarchy for handle invalidation and one hierarchy for pointer invalidation.

From there, feedback can be gathered as to whether further splitting or merging should be considered before
stabilization.


#   Prior Art

##  C++

In C++, `std::allocator_traits` allows one to create an Allocator which uses handles that are not (strictly) pointers.

The impetus behind this design was to allow greater flexibility, much like this proposal, unfortunately it failed
spectacularly:

1.  While one can specify a non-pointer `pointer` type, this type MUST still be pointer-like: it must be
    dereferenceable, etc... This requirement mostly requires the type to embed a pointer -- possibly augmented -- and
    thus makes it unsuitable for in-line storage, unsuitable for compaction, and only usable for shared memory usage
    with difficulty.
2.  While one can specify a non-reference `reference` type, the lack of `Deref` means that such a type does not,
    actually, behave as a reference, and while proxies can be crafted for specific types (`std::vector<bool>`
    demonstrates it) it's impossible to craft transparent proxies in the generic case.

The mistake made in the C++ allocator API is to require returning pointer-like/reference-like objects directly usable
by the user of the collection based upon the allocator.

This RFC learns from C++ mistake by introducing a level of indirection:

1.  An opaque `Handle` is returned by the `Storage`, which can be stored and copied freely, but cannot be dereferenced.
    It is intended to be kept as an implementation detail within the collection, and invisible to the final user.
2.  A `resolve` method to resolve a `Handle` into a pointer, and from there into a reference.

Throwing in flexible invalidation guarantees ties the knot, allowing this API to be much more flexible than the C++
allocator API.


##  Previous attempts

I have been seeking a better allocator API for years, now. This RFC draws from this experience:

-   I implemented a similar API _specifically_ for vector-like collections in C++. It was much less flexible, and
    tailored for C++ requirements, but did prove that a somewhat minimalist API _was_ sufficient to build a collection
    that could then be declined in Inline, Small, and "regular" versions.
-   Early 2021, I demonstrated the potential for storages in https://github.com/matthieu-m/storage-poc. It was based
    on my C++ experience, from which it inherited strong typing, which itself required GATs...
-   Early 2022, @CAD97 demonstrated that a much leaner API could be made in https://github.com/CAD97/storages-api.
    After reviewing his work, I concluded that the API was not suitable to replace `Allocator` in a number of
    situations as discussed in the Alternatives section, and that further adjustments needed to be made.

And thus in early 2023 I implemented a 3rd revision of the API, a revision I am increasingly confident in for 2 reasons:

1.  It is nearly a direct translation of the `Allocator` API, which has been used within the Rust standard library for
    years now.
2.  A core trait providing functionality and a set of marker traits providing guarantees is much easier to deal with
    than multiple traits each providing related but subtly different functionalities and guarantees.

The ability to develop 3rd-party extensions for increased safety also confirms, to me, that @CAD97 was on the right
track when they removed the strong typing, and on the wrong track when they attempted to bake in borrow-checking: if
it's easy enough to add safety, then it seems better for the "core" API to be minimalist instead.


#   Unresolved Questions

##  (Major) How to make `StorageBox<T, S>` coercible?

Unfortunately, at the moment, `Box` is only coercible because it stores a `NonNull<T>`, which is coercible. Splitting
`NonNull<T>` into `S::Handle` and `<T as Pointee>::Metadata`, as `StorageBox<T, S>` does, leads to losing coercibility.

A separate solution is required to regain coercibility, which is out of scope of this RFC, but would have to be solved
if `StorageBox<T, S>` were to become `Box`, which seems preferable to keeping it separate.

A suggestion would be to have a `TypedMetadata<T>` lang item, which would implement `CoerceUnsized` appropriately, and
the companion repository showcases how building upon this `StorageBox` gains the ability to be `CoerceUnsized`. This is
a topic for another RFC, however.


##  (Minor) What should the capabilities of `Handle` be?

Since any capability specified in the associated type definition is "mandatory", I am of the opinion that it should be
kept to a minimum, and users can always specify additional requirements if they need to:

-   At the moment, the only required is `Copy`. It could be downgraded to `Clone`, or removed entirely.
    -   Although, do mind that just using `Storage::grow`, or `Storage::shrink` requires a copy/clone.
-   `Eq`, `Hash`, and `Ord` are obvious candidates, yet they are unused in the current proposal:
    -   Implementing `Eq`, `Hash`, or `Ord` for a collection does not require the handles to implement any of them.
-   `Send` and `Sync` should definitely be kept out. `Allocator`-based storages could not use `NonNull<u8>` otherwise.


##  (Minor) Should `Storage::dangling` be `const`?

While const trait associated functions are still a maybe, it seems reasonable to ask ourselves whether some of the
associated functions of `Storage` should be `const` if it were possible.

There doesn't seem to be a practical advantage in doing so for most of the associated functions of `Storage`: if
allocation and deallocation need be executed in a const context, then a `const Storage` is necessary, and there's no
need to single out any of those.

There is, however, a very practical advantage in making `Storage::dangling` const: it allows initialized an empty
collection in a const context even with a non-const `Storage` implementation.

The one downside is that this would preclude some implementations of `dangling` which would rely on global state, or
I/O. @CAD97 notably mentioned the possibility of using randomization for debugging or hardening purposes. Still, it
would still be possible to initialize the instance of `Storage` with a random seed, then use a PRNG within `dangling`.


#   Future Possibilities

##  TypedHandle

A straightforward extension is to define a `TypedHandle<T, H>` type, which wraps a handle (`H`) to a block of memory
suitable for an instance of `T`, as well as the metadata of `T`.

The `Storage` methods can then be mimicked, with the additional type information:

-   `resolve` returns an appropriate pointer type, complete with metadata.
-   Layouts are computed automatically based on the type and metadata.
-   Growing and Shrinking on slices take the target number of elements, rather than a more complex layout.

And because `resolve` and `resolve_mut` can return references -- being typed -- they can borrow the `storage` (shared)
to ensure it's not moved nor dropped.


##  UniqueHandle

A further extension is to define a `UniqueHandle<T, H>` type, which adds ownership & borrow-checking over a
`TypedHandle<T, H>`.

That is:

```rust
impl<T: ?Sized, H: Copy> UniqueHandle<T, H> {
    pub unsafe fn resolve<'a, S>(&'a self, storage: &'a S) -> &'a T
    where
        S: Storage;

    pub unsafe fn resolve_mut<'a, S>(&'a mut self, storage: &'a S) -> &'a mut T
    where
        S: Storage;
}
```

Those two methods are `unsafe` as a dangling handle cannot be soundly resolved, and a valid handle may not necessarily
be associated with a block of memory containing a valid instance of `T` -- it may never have been constructed, it may
have been moved or dropped, etc...

On the other hand, those two methods guarantee:

-   Proper typing: if the handle is valid, and a value exists, then that value is of type `T`.
-   Proper borrow-checking: the handle is the unique entry point to the instance of `T`, hence the name.
-   Proper pinning: even if the storage does not implement `PinningStorage`, borrowing it ensures that it cannot be
    moved nor dropped. If the storage implements `StableStorage`, this means that the result of `resolve` and
    `resolve_mut` can be used safely.

And that's pretty good, given how straightforward the code is.


##  Compact Vectors

How far should DRY go?

One limitation of `Vec<u8, InlineStorage<[u8; 16]>>` is that it contains 2 `usize` for the length and capacity
respectively, which is rather unfortunate.

There are potential solutions to the issue, using separate traits for those so they can be stored in more compact ways
or even elided in the case of fixed capacity.

The `Storage` API could be augmented to provide associated constants / types describing the limits of what it can offer
such as minimum/maximum capacity, to support automatically selecting (or constraint-checking) the appropriate types.

Since those extra capabilities can be brought in by user traits for now, I would favor adopting a wait-and-see approach
here.
