//! A polyfill over some alloc crate pieces of functionality.

#[cfg(feature = "alloc")]
pub use alloc::alloc::handle_alloc_error;

#[cfg(not(feature = "alloc"))]
pub use polyfill::handle_alloc_error;

#[cfg(not(feature = "alloc"))]
mod polyfill {
    use core::alloc::Layout;

    pub const fn handle_alloc_error(_layout: Layout) -> ! {
        panic!("allocation failed")
    }
} // mod polyfill
