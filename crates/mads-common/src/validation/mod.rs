//! Transport-independent validation after deserialization.

mod error;
mod nested;
mod number;
mod string;

#[doc(hidden)]
pub mod support;

pub use error::{ValidationErrors, ValidationIssue, ValidationPathSegment, ValidationResult};

/// Validates an existing input value without deserializing or transforming it.
///
/// Manual implementations and the input derive return the same ordered issues.
/// Validation performs no asynchronous work; application business checks may
/// still run separately in handlers or services.
pub trait Input {
    /// Returns all independent validation failures in deterministic order.
    fn validate(&self) -> ValidationResult;
}
