//! Implementation helpers for generated validation.

use super::{ValidationErrors, ValidationIssue, ValidationResult};

pub use super::number::Number;
pub use super::string::{Length, email};

/// One of the fixed, value-free built-in issue kinds.
#[derive(Clone, Copy)]
pub enum IssueKind {
    /// A required option or input field was absent.
    Required,
    /// A value has an incompatible type or is non-finite.
    InvalidType,
    /// The input representation has invalid syntax.
    InvalidSyntax,
    /// An email address violates the practical syntax policy.
    Email,
    /// A lower bound failed.
    TooSmall,
    /// An upper bound failed.
    TooBig,
    /// A numeric multiple constraint failed.
    NotMultipleOf,
}

/// Builds an issue from the frozen client message catalog.
pub fn issue(kind: IssueKind) -> ValidationIssue {
    let (code, message) = match kind {
        IssueKind::Required => ("required", "required value is missing"),
        IssueKind::InvalidType => ("invalid_type", "input has an invalid type"),
        IssueKind::InvalidSyntax => ("invalid_syntax", "input syntax is invalid"),
        IssueKind::Email => ("invalid_format", "invalid email address"),
        IssueKind::TooSmall => ("too_small", "value is too small"),
        IssueKind::TooBig => ("too_big", "value is too big"),
        IssueKind::NotMultipleOf => (
            "not_multiple_of",
            "value is not a multiple of the required number",
        ),
    };
    ValidationIssue::custom(code, message)
}

/// Collects independent failures without creating an empty error collection.
pub fn merge(errors: &mut Option<ValidationErrors>, result: ValidationResult) {
    if let Err(next) = result {
        match errors {
            Some(errors) => errors.extend(next),
            None => *errors = Some(next),
        }
    }
}

/// Converts an optional nonempty error collection into a validation result.
pub fn finish(errors: Option<ValidationErrors>) -> ValidationResult {
    errors.map_or(Ok(()), Err)
}
