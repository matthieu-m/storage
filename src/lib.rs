//! Store API, for greater flexibility.
//!
//! This project aims at exploring the possibility of a different API for allocation, providing greater flexibility
//! than `Allocator`.
//!
//! This project does NOT aim at displacing `Allocator`, but rather at providing a lower-level abstraction layer for
//! when greater flexibility is required. Zero-Cost compatibility with `Allocator` is desired, so that collections can
//! be implemented in terms of `Store`, but used with an `Allocator` easily.

#![cfg_attr(not(test), no_std)]
//  Features
#![feature(allocator_api)]
#![feature(alloc_layout_extra)]
#![feature(coerce_unsized)]
#![feature(layout_for_ptr)]
#![feature(maybe_uninit_write_slice)]
#![feature(offset_of)]
#![feature(ptr_as_uninit)]
#![feature(ptr_metadata)]
#![feature(slice_ptr_get)]
#![feature(specialization)]
#![feature(unsize)]
//  Lints
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(incomplete_features)] //  For specialization.

pub mod collection;
pub mod extension;
pub mod interface;
pub mod store;
