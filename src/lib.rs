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
#![feature(const_alloc_layout)]
#![feature(const_mut_refs)]
#![feature(const_ptr_as_ref)]
#![feature(const_refs_to_cell)]
#![feature(const_slice_from_raw_parts_mut)]
#![feature(const_trait_impl)]
#![feature(const_try)]
#![feature(const_ptr_write)]
#![feature(hasher_prefixfree_extras)]
#![feature(layout_for_ptr)]
#![feature(maybe_uninit_write_slice)]
#![feature(offset_of)]
#![feature(never_type)]
#![feature(ptr_alignment_type)]
#![feature(ptr_as_uninit)]
#![feature(ptr_metadata)]
#![feature(slice_ptr_get)]
#![feature(specialization)]
#![feature(strict_provenance)]
#![feature(unsize)]
#![feature(unwrap_infallible)]
//  Lints
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(incomplete_features)] //  For specialization.

#[cfg(feature = "alloc")]
extern crate alloc;

mod alloc;
pub mod collection;
pub mod extension;
pub mod interface;
pub mod store;
