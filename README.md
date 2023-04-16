Yet another iteration on the Storage API.

#   Goals

This is an advanced Proof-of-Concept aiming at:

-   Demonstrating the technical feasibility.
-   Showcasing the flexibility of storages.
-   Streamlining the API of previous PoCs.

This experiment does not (yet?) intend to provide production-ready collections.


#   Why Storage over Allocator?

`Storage` does NOT intend to displace `Allocator`, `Allocator` is the right level of abstraction for a wide variety of
situations, in which case it should be used.

`Storage`, instead, aims at offering a more flexible, lower-level, API than `Allocator` for those cases where
`Allocator` does not cut it.

When should you favor `Storage` over `Allocator`:

-   To avoid a pointer indirection: an inline storage enables storing the "allocated" item in the same cache line as
    the storage object, hence one less level of indirection.
-   To avoid memory allocation while retaining a `'static` item: an inline storage enables in-situ allocation without
    imposing any restriction on the duration of the storage, the resulting containers can be stored in long-lived
    collections, sent across threads, etc...
-   To allow complex values to be stored in ROM: rustc currently is unable to store items with allocated in ROM --
    although it is theoretically possible -- in which case an inline storage nicely works around this limitation,
    allowing `static` variables of type `Vec`, `BTreeMap`, etc... as long as `const fn` are good enough to calculate
    them.
-   To allow complex values to be stored in shared memory, although restrictions will remain -- traits are out, for
    example.

The `Storage` API achieves this by returning an abstract `Handle`, rather than a pointer, and offering a way to
_temporarily_ derive a pointer from this `Handle`. Since the `Handle` is what is stored, it can be a ZST, it can be an
offset, etc... allowing it to fit where a pointer doesn't always.


#   How to navigate this repository?

The repository contains 3 parts:

-   `interface` sketches out the `Storage` trait, and its two companion traits.
-   `storage` contains a number of storages, including an adapter to turn any `Allocator` into a `Storage`.
-   `collection` contains a variety of collections, demonstrating the viability of `Storage` for those usecases.


#   Can we replace the `std` collections tomorrow?

Most collections are replaceable, `Box`... is a tad more complicated.

The main issue for `Box` there is that `CoerceUnsized` and `Unsize` are pretty locked down. Even though it is possible
to implement a `coerce` method, it is not possible to implement `CoerceUnsized` because `T::Metadata` is not coercible
to `U::Metadata`.

There are various solutions, of course, including a special compiler-blessed solution, etc... which to pick is up in
the air.


#   History

The idea of a Storage API dates back a wee bit -- to 2021 -- and this is yet another iteration:

-   First iteration: https://github.com/matthieu-m/storage-poc
-   Second iteration: https://github.com/CAD97/storages-api

The first iteration started with the concept of a very strongly typed API. Since a Storage could require allocating
different types, this led to requiring GATs for handles, and a proliferation of traits.

@CAD97 had the insight that eliminating typed handles would allow streamlining the API, and thus the second iteration
was born. It was _much_ simpler: no GATs, fewer traits and methods, ... all around better!

Coming back to the second iteration after a few months, I felt that the second iteration was not as simple as it could
be, though, and that a number of decisions were unfortunate -- requiring `Layout` in `resolve`, and taking `&mut` -- as
they would reduce the flexibility. See https://github.com/CAD97/storages-api/issues/6 for my remarks.

Thus the idea for a third iteration was born:

-   Eliminating `Layout` as an argument to `resolve` to support `ThinPointer`, skip lists, etc...
-   Taking `&self` rather than `&mut self` to support concurrent collections.

And taking the opportunity to streamline the API further: less methods, less traits. Or in the words of Saint-Exupery:

> Simplicity is achieved not when there is nothing to add, but when there is nothing to remove.


#   That's all folks!

And thanks for reading so far.
