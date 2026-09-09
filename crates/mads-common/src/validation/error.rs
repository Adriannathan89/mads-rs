//! Ordered validation failures with paths relative to the validated input.

use serde::Serialize;
use std::fmt;

/// The request representation that produced a validation issue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSource {
    /// The request body.
    Body,
    /// The URL query string.
    Query,
    /// The URL path parameters.
    Path,
}

/// A validation issue explicitly attached to its request representation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourcedValidationIssue {
    source: ValidationSource,
    #[serde(flatten)]
    issue: ValidationIssue,
}

impl SourcedValidationIssue {
    /// Returns the request representation that produced this issue.
    pub fn source(&self) -> ValidationSource {
        self.source
    }

    /// Borrows the original, transport-independent issue.
    pub fn issue(&self) -> &ValidationIssue {
        &self.issue
    }
}

/// The result of validating an already deserialized value.
pub type ValidationResult = Result<(), ValidationErrors>;

/// One segment of a request-representation path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ValidationPathSegment {
    /// An external field, variant, or string map key.
    Field(String),
    /// A zero-based sequence or tuple index.
    Index(usize),
}

/// One validation failure without a rejected value or transport source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationIssue {
    path: Vec<ValidationPathSegment>,
    code: String,
    message: String,
}

impl ValidationIssue {
    /// Attaches this issue to an explicit request representation.
    pub fn with_source(self, source: ValidationSource) -> SourcedValidationIssue {
        SourcedValidationIssue {
            source,
            issue: self,
        }
    }

    /// Creates an application-owned failure at the relative root.
    ///
    /// Custom codes and messages must be safe for clients and ordinary logging;
    /// callers are responsible for excluding rejected values and secrets.
    pub fn custom(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: Vec::new(),
            code: code.into(),
            message: message.into(),
        }
    }

    /// Appends an external field or map key to the relative path.
    pub fn at_field(mut self, field: impl Into<String>) -> Self {
        self.path.push(ValidationPathSegment::Field(field.into()));
        self
    }

    /// Appends a zero-based sequence or tuple index to the relative path.
    pub fn at_index(mut self, index: usize) -> Self {
        self.path.push(ValidationPathSegment::Index(index));
        self
    }

    /// Returns the relative path in request order.
    pub fn path(&self) -> &[ValidationPathSegment] {
        &self.path
    }

    /// Returns the stable issue code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the safe client-facing message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// One or more independent validation failures in deterministic order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationErrors {
    issues: Vec<ValidationIssue>,
}

impl ValidationErrors {
    /// Creates a collection containing one issue.
    pub fn from_issue(issue: ValidationIssue) -> Self {
        Self {
            issues: vec![issue],
        }
    }

    /// Appends an issue without reordering earlier failures.
    pub fn push(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    /// Appends another collection in its existing order.
    pub fn extend(&mut self, other: Self) {
        self.issues.extend(other.issues);
    }

    /// Borrows every issue in insertion order.
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    /// Consumes this collection without changing issue order.
    pub fn into_issues(self) -> Vec<ValidationIssue> {
        self.issues
    }

    /// Prepends a containing representation path to every relative issue.
    #[doc(hidden)]
    pub fn __prefix(mut self, prefix: &[ValidationPathSegment]) -> Self {
        for issue in &mut self.issues {
            let mut path = prefix.to_vec();
            path.append(&mut issue.path);
            issue.path = path;
        }
        self
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index != 0 {
                formatter.write_str("\n")?;
            }
            write!(
                formatter,
                "{:?} [{}]: {}",
                issue.path, issue.code, issue.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}
