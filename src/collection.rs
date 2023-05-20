//! A variety of collections implemented in terms of `Store`.
//!
//! The collections may have a rather minimal interface, as the emphasis is put on demonstrating the flexibility of the
//! `Store` trait, rather than providing fully implemented collections -- for now.

mod concurrent_vec;
mod skip_list;
mod store_box;

#[cfg(test)]
mod utils;

pub use concurrent_vec::ConcurrentVec;
pub use skip_list::SkipList;
pub use store_box::StoreBox;
