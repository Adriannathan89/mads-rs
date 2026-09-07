//! Input validation after deserialization and source-aware HTTP extraction.

mod error;
mod extract;
mod nested;
mod number;
mod primitive;
mod rejection;
mod string;

#[doc(hidden)]
pub mod support;

pub use error::{
    SourcedValidationIssue, ValidationErrors, ValidationIssue, ValidationPathSegment,
    ValidationResult, ValidationSource,
};
pub use extract::ValidatedJson;

/// Validates an existing input value without deserializing or transforming it.
///
/// Manual implementations and the input derive return the same ordered issues.
/// Validation performs no asynchronous work; application business checks may
/// still run separately in handlers or services.
///
/// ```
/// use mads_common::Input;
///
/// #[derive(Input)]
/// struct CreateUser {
///     #[validate(email)]
///     email: String,
///     #[validate(length(min = 8, max = 128))]
///     password: String,
/// }
///
/// let input = CreateUser { email: "invalid".into(), password: "short".into() };
/// let errors = input.validate().unwrap_err();
/// assert_eq!(errors.issues().len(), 2);
/// ```
pub trait Input {
    /// Returns all independent validation failures in deterministic order.
    fn validate(&self) -> ValidationResult;
}
