//! Typed Metadata, for coercion purposes.

use core::fmt;

pub use implementation::TypedMetadata;

#[cfg(not(feature = "coercible-metadata"))]
mod implementation {
    use core::{
        marker::Unsize,
        ptr::{self, Pointee},
    };

    /// Typed Metadata, for type-safe APIs.
    pub struct TypedMetadata<T: ?Sized>(<T as Pointee>::Metadata);

    impl<T: ?Sized> TypedMetadata<T> {
        /// Creates a new Typed metadata.
        pub fn new(metadata: <T as Pointee>::Metadata) -> Self {
            Self(metadata)
        }

        /// Retrieves the metadata.
        pub fn get(&self) -> <T as Pointee>::Metadata {
            self.0
        }

        /// Coerces the metadata into another.
        pub fn coerce<U: ?Sized>(&self) -> TypedMetadata<U>
        where
            T: Unsize<U>,
        {
            let pointer: *const T = ptr::from_raw_parts(ptr::null(), self.0);
            let pointer: *const U = pointer as *const _;
            let (_, metadata) = pointer.to_raw_parts();

            TypedMetadata(metadata)
        }
    }
} // mod implementation

#[cfg(feature = "coercible-metadata")]
mod implementation {
    use core::{
        marker::Unsize,
        ops::CoerceUnsized,
        ptr::{NonNull, Pointee},
    };

    /// Typed Metadata, for type-safe APIs.
    pub struct TypedMetadata<T: ?Sized>(NonNull<T>);

    impl<T: ?Sized> TypedMetadata<T> {
        /// Creates a new Typed metadata.
        pub fn new(metadata: <T as Pointee>::Metadata) -> Self {
            Self(NonNull::from_raw_parts(NonNull::dangling(), metadata))
        }

        /// Retrieves the metadata.
        pub fn get(&self) -> <T as Pointee>::Metadata {
            self.0.to_raw_parts().1
        }

        /// Coerces the metadata into another.
        pub fn coerce<U: ?Sized>(&self) -> TypedMetadata<U>
        where
            T: Unsize<U>,
        {
            *self
        }
    }

    #[cfg(feature = "coercible-metadata")]
    impl<T: ?Sized, U: ?Sized> CoerceUnsized<TypedMetadata<U>> for TypedMetadata<T> where T: Unsize<U> {}
} // mod implementation

impl<T: ?Sized> Clone for TypedMetadata<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for TypedMetadata<T> {}

impl<T: ?Sized> fmt::Debug for TypedMetadata<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "TypedMetadata")
    }
}

impl<T> Default for TypedMetadata<T> {
    fn default() -> Self {
        Self::new(())
    }
}

impl<T> From<usize> for TypedMetadata<[T]> {
    fn from(value: usize) -> Self {
        Self::new(value)
    }
}
