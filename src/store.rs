//! Provides implementations of multiple stores or store adapters.

mod allocator_store;
mod inline_bump_store;
mod inline_single_store;

pub use inline_bump_store::InlineBumpStore;
pub use inline_single_store::InlineSingleStore;
