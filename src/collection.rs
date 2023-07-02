//! A variety of collections implemented in terms of `Store`.
//!
//! The collections may have a rather minimal interface, as the emphasis is put on demonstrating the flexibility of the
//! `Store` trait, rather than providing fully implemented collections -- for now.

mod concurrent_vec;
mod linked_list;
mod skip_list;
mod store_box;
mod store_vec;

#[cfg(test)]
mod utils;

pub use concurrent_vec::ConcurrentVec;
pub use linked_list::LinkedList;
pub use skip_list::SkipList;
pub use store_box::StoreBox;
pub use store_vec::StoreVec;
