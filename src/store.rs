//! Provides implementations of multiple stores or store adapters.

mod allocator_store;
mod inline_single_store;

pub use inline_single_store::InlineSingleStore;
