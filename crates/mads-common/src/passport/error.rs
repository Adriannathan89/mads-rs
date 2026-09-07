//! Redacted Passport errors and Axum rejections.

use std::fmt;

use axum::{
    http::{HeaderValue, header::WWW_AUTHENTICATE},
    response::{IntoResponse, Response},
};

use crate::{Forbidden, InternalError, Unauthorized};

/// The result type used by Passport strategy APIs.
pub type PassportResult<T> = std::result::Result<T, PassportError>;

/// A stable category for a Passport failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PassportErrorKind {
    /// Authentication was rejected by an application strategy.
    Rejected,
    /// A verified principal did not satisfy a route policy.
    Forbidden,
    /// A strategy or framework operation failed unexpectedly.
    Internal,
}

/// A normalized Passport error whose formatting never exposes sensitive data.
#[non_exhaustive]
pub struct PassportError {
    kind: PassportErrorKind,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl PassportError {
    /// Creates an expected authentication rejection.
    #[must_use]
    pub const fn reject() -> Self {
        Self {
            kind: PassportErrorKind::Rejected,
            source: None,
        }
    }

    pub(crate) const fn forbidden() -> Self {
        Self {
            kind: PassportErrorKind::Forbidden,
            source: None,
        }
    }

    /// Creates an operational failure while retaining its source for inspection.
    pub fn internal<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: PassportErrorKind::Internal,
            source: Some(Box::new(source)),
        }
    }

    /// Returns the stable category of this error.
    #[must_use]
    pub const fn kind(&self) -> PassportErrorKind {
        self.kind
    }
}

impl fmt::Debug for PassportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportError")
            .field("kind", &self.kind)
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl fmt::Display for PassportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            PassportErrorKind::Rejected => "authentication was rejected",
            PassportErrorKind::Forbidden => "access was denied",
            PassportErrorKind::Internal => "Passport operation failed",
        })
    }
}

impl std::error::Error for PassportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

impl From<crate::JwtError> for PassportError {
    fn from(error: crate::JwtError) -> Self {
        match error.kind() {
            crate::JwtErrorKind::InvalidConfiguration
            | crate::JwtErrorKind::InvalidKeyMaterial
            | crate::JwtErrorKind::UnavailableSigningKey
            | crate::JwtErrorKind::Serialization => Self::internal(error),
            _ => Self::reject(),
        }
    }
}

/// An Axum rejection produced by Passport extractors and guards.
#[non_exhaustive]
pub struct PassportRejection(PassportError);

impl PassportRejection {
    pub(crate) const fn new(error: PassportError) -> Self {
        Self(error)
    }

    /// Returns the stable category of the rejected Passport operation.
    #[must_use]
    pub const fn kind(&self) -> PassportErrorKind {
        self.0.kind()
    }
}

impl fmt::Debug for PassportRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportRejection")
            .field("kind", &self.kind())
            .finish()
    }
}

impl fmt::Display for PassportRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for PassportRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<PassportError> for PassportRejection {
    fn from(error: PassportError) -> Self {
        Self::new(error)
    }
}

impl IntoResponse for PassportRejection {
    fn into_response(self) -> Response {
        let Self(error) = self;
        match error.kind() {
            PassportErrorKind::Rejected => {
                let mut response = Unauthorized::new("authentication was rejected").into_response();
                response
                    .headers_mut()
                    .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
                response
            }
            PassportErrorKind::Forbidden => Forbidden::new("access was denied").into_response(),
            PassportErrorKind::Internal => InternalError::new(error).into_response(),
        }
    }
}
