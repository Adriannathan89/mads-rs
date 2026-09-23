use std::fmt;

use mads_core::{Diagnostic, DiagnosticCode};

/// Stable diagnostic code for persistence connector failures.
pub const MADS140: DiagnosticCode = DiagnosticCode::new("MADS140");

/// Classifies persistence failures without requiring message parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistenceErrorKind {
    /// Typed persistence configuration was absent or invalid.
    InvalidConfiguration,
    /// The configured database URL uses an unsupported scheme.
    UnsupportedScheme,
    /// The connector could not establish its native database connection.
    Connection,
    /// The connected database failed its lifecycle readiness check.
    Readiness,
    /// The native database connection failed to close gracefully.
    GracefulClose,
}

/// A redacted persistence failure with an optional retained typed cause.
pub struct PersistenceError {
    kind: PersistenceErrorKind,
    operation: &'static str,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

#[allow(dead_code)]
impl PersistenceError {
    pub(crate) const fn new(kind: PersistenceErrorKind, operation: &'static str) -> Self {
        Self {
            kind,
            operation,
            source: None,
        }
    }

    pub(crate) fn with_source<E>(
        kind: PersistenceErrorKind,
        operation: &'static str,
        source: E,
    ) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind,
            operation,
            source: Some(Box::new(source)),
        }
    }

    /// Returns the stable category of this persistence failure.
    pub const fn kind(&self) -> PersistenceErrorKind {
        self.kind
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "database connector operation `{}` failed ({:?})",
            self.operation, self.kind
        )
    }
}

impl fmt::Debug for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistenceError")
            .field("kind", &self.kind)
            .field("operation", &self.operation)
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

impl From<PersistenceError> for mads_core::Error {
    fn from(error: PersistenceError) -> Self {
        let operation = error.operation;
        mads_core::Error::with_source(
            Diagnostic::new(
                MADS140,
                "persistence operation failed",
                "a database connector operation failed",
            )
            .with_subject(operation),
            error,
        )
    }
}

/// A result returned by persistence connector operations.
pub type PersistenceResult<T> = Result<T, PersistenceError>;

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn framework_conversion_retains_kind_without_rendering_the_source() {
        let error = PersistenceError::with_source(
            PersistenceErrorKind::Connection,
            "connect",
            std::io::Error::other("mads-secret-user:mads-secret-password"),
        );
        let framework: mads_core::Error = error.into();

        assert_eq!(framework.code(), MADS140);
        assert!(!format!("{framework:?}\n{framework}").contains("mads-secret"));
        let persistence = framework
            .source()
            .unwrap()
            .downcast_ref::<PersistenceError>()
            .unwrap();
        assert_eq!(persistence.kind(), PersistenceErrorKind::Connection);
        assert!(persistence.source().is_some());
    }
}
