//! A variety of collections implemented in terms of `Storage`.
//!
//! The collections may have a rather minimal interface, as the emphasis is put on demonstrating the flexibility of the
//! `Storage` trait, rather than providing fully implemented collections -- for now.

mod storage_box;

#[cfg(test)]
mod utils;

pub use storage_box::StorageBox;
