//! Explicit secret exposure and redacted ordinary formatting.

use std::fmt;

/// A value whose ordinary formatting is always redacted.
///
/// Access requires an explicit call to [`Self::expose`] or
/// [`Self::into_exposed`]. This wrapper does not promise memory zeroization.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wraps a value without formatting it.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Explicitly borrows the secret value.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Explicitly consumes the wrapper and returns the secret value.
    pub fn into_exposed(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}
