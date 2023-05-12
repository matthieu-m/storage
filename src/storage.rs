//! Provides implementations of multiple storages or storage adapters.

mod allocator_storage;
mod inline_single_storage;

pub use inline_single_storage::InlineSingleStorage;
