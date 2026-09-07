//! HTTP response types for MADS route handlers.
//!
//! [`HttpResult`] represents delivery failures as stable HTTP responses. It is
//! intentionally separate from [`mads_core::Result`], which remains the result
//! type for framework construction and bootstrap operations. Handlers may also
//! return any native Axum [`IntoResponse`](axum::response::IntoResponse) type.

use std::{error::Error, fmt};

use crate::SourcedValidationIssue;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

const INTERNAL_SERVER_ERROR_MESSAGE: &str = "internal server error";

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'static str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    issues: Option<&'a [SourcedValidationIssue]>,
}

struct ResponseDescription<'a> {
    status: StatusCode,
    body: ErrorBody<'a>,
}

impl IntoResponse for ResponseDescription<'_> {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorEnvelope { error: self.body })).into_response()
    }
}

#[cfg(all(feature = "http", feature = "database"))]
#[doc(hidden)]
pub struct SourceRetainingClientError {
    message: String,
    source: Box<dyn Error + Send>,
}

#[cfg(all(feature = "http", feature = "database"))]
impl SourceRetainingClientError {
    fn new(message: impl Into<String>, source: impl Error + Send + 'static) -> Self {
        Self {
            message: message.into(),
            source: Box::new(source),
        }
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn source(&self) -> &(dyn Error + 'static) {
        self.source.as_ref()
    }
}

/// An HTTP error rendered as a stable JSON response.
///
/// Construct values with [`HttpError::bad_request`],
/// [`HttpError::not_found`], [`HttpError::conflict`], or
/// [`HttpError::internal`] so callers do not depend on individual variants.
#[non_exhaustive]
pub enum HttpError {
    /// A request that cannot be processed because its client-facing input is invalid.
    BadRequest(String),
    /// A request without accepted authentication.
    Unauthorized(String),
    /// A request without sufficient authorization.
    Forbidden(String),
    /// A requested resource that could not be found.
    NotFound(String),
    /// A request that conflicts with the current state of a resource.
    Conflict(String),
    #[cfg(all(feature = "http", feature = "database"))]
    #[doc(hidden)]
    NotFoundWithSource(SourceRetainingClientError),
    #[cfg(all(feature = "http", feature = "database"))]
    #[doc(hidden)]
    ConflictWithSource(SourceRetainingClientError),
    #[cfg(all(feature = "http", feature = "database"))]
    #[doc(hidden)]
    InternalWithSource(SourceRetainingClientError),
    /// Ordered validation issues with explicit request sources.
    Validation(Vec<SourcedValidationIssue>),
    /// An unexpected server-side failure whose source is not exposed to clients.
    Internal(Box<dyn Error + Send + Sync>),
}

impl HttpError {
    /// Creates a 401 Unauthorized error with a safe client-facing message.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    /// Creates a 403 Forbidden error with a safe client-facing message.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// Creates a 422 response from explicitly sourced validation issues.
    pub fn validation(issues: impl IntoIterator<Item = SourcedValidationIssue>) -> Self {
        Self::Validation(issues.into_iter().collect())
    }
    /// Creates a 400 Bad Request error with a safe client-facing message.
    ///
    /// The message is serialized as the `error.message` field. This constructor
    /// does not expose an internal source because the error is already a
    /// client-facing failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use mads_common::HttpError;
    ///
    /// let error = HttpError::bad_request("invalid page number");
    /// assert_eq!(error.to_string(), "invalid page number");
    /// ```
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// Creates a 404 Not Found error with a safe client-facing message.
    ///
    /// # Examples
    ///
    /// ```
    /// use mads_common::HttpError;
    ///
    /// let error = HttpError::not_found("user does not exist");
    /// assert_eq!(error.to_string(), "user does not exist");
    /// ```
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// Creates a 409 Conflict error with a safe client-facing message.
    ///
    /// # Examples
    ///
    /// ```
    /// use mads_common::HttpError;
    ///
    /// let error = HttpError::conflict("username is already taken");
    /// assert_eq!(error.to_string(), "username is already taken");
    /// ```
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// Creates a 500 Internal Server Error without exposing its source to clients.
    ///
    /// The source remains available through [`std::error::Error::source`] for
    /// server-side logging, while the HTTP response always contains the stable
    /// message `internal server error`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    ///
    /// use mads_common::HttpError;
    ///
    /// let error = HttpError::internal(io::Error::other("database unavailable"));
    /// assert_eq!(error.to_string(), "internal server error");
    /// assert!(std::error::Error::source(&error).is_some());
    /// ```
    pub fn internal(source: impl Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(source))
    }

    fn description(&self) -> ResponseDescription<'_> {
        let (status, code, message): (StatusCode, &'static str, &str) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, "bad_request", message),
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, "unauthorized", message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, "forbidden", message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            #[cfg(all(feature = "http", feature = "database"))]
            Self::NotFoundWithSource(error) => {
                (StatusCode::NOT_FOUND, "not_found", error.message())
            }
            #[cfg(all(feature = "http", feature = "database"))]
            Self::ConflictWithSource(error) => (StatusCode::CONFLICT, "conflict", error.message()),
            #[cfg(all(feature = "http", feature = "database"))]
            Self::InternalWithSource(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                INTERNAL_SERVER_ERROR_MESSAGE,
            ),
            Self::Validation(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                "input validation failed",
            ),
            Self::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                INTERNAL_SERVER_ERROR_MESSAGE,
            ),
        };
        ResponseDescription {
            status,
            body: ErrorBody {
                code,
                message,
                issues: match self {
                    Self::Validation(issues) => Some(issues),
                    _ => None,
                },
            },
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description().body.message)
    }
}

impl Error for HttpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            #[cfg(all(feature = "http", feature = "database"))]
            Self::NotFoundWithSource(error)
            | Self::ConflictWithSource(error)
            | Self::InternalWithSource(error) => Some(error.source()),
            Self::Internal(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        self.description().into_response()
    }
}

impl fmt::Debug for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpError")
            .field("code", &self.description().body.code)
            .field("has_source", &self.source().is_some())
            .finish()
    }
}

macro_rules! named_error_impls {
    ($name:ident) => {
        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("code", &self.0.description().body.code)
                    .field("has_source", &self.source().is_some())
                    .finish()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, formatter)
            }
        }
        impl Error for $name {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                self.0.source()
            }
        }
        impl IntoResponse for $name {
            fn into_response(self) -> Response {
                self.0.into_response()
            }
        }
        impl From<$name> for HttpError {
            fn from(error: $name) -> Self {
                error.0
            }
        }
    };
}

macro_rules! client_error {
    ($name:ident, $constructor:ident, $doc:literal) => {
        #[doc = $doc]
        pub struct $name(HttpError);
        impl $name {
            /// Creates an error with an explicitly safe client-facing message.
            pub fn new(message: impl Into<String>) -> Self {
                Self(HttpError::$constructor(message))
            }
        }
        named_error_impls!($name);
    };
}

client_error!(BadRequest, bad_request, "A 400 Bad Request error.");
client_error!(
    Unauthorized,
    unauthorized,
    "A 401 Unauthorized error without an implicit authentication challenge."
);
client_error!(Forbidden, forbidden, "A 403 Forbidden error.");
/// A 404 Not Found error.
pub struct NotFound(HttpError);

impl NotFound {
    /// Creates an error with an explicitly safe client-facing message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(HttpError::not_found(message))
    }

    #[cfg(all(feature = "http", feature = "database"))]
    pub(crate) fn from_source(source: impl Error + Send + 'static) -> Self {
        Self(HttpError::NotFoundWithSource(
            SourceRetainingClientError::new("resource not found", source),
        ))
    }
}
named_error_impls!(NotFound);

/// A 409 Conflict error.
pub struct Conflict(HttpError);

impl Conflict {
    /// Creates an error with an explicitly safe client-facing message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(HttpError::conflict(message))
    }

    #[cfg(all(feature = "http", feature = "database"))]
    pub(crate) fn from_source(source: impl Error + Send + 'static) -> Self {
        Self(HttpError::ConflictWithSource(
            SourceRetainingClientError::new("resource already exists", source),
        ))
    }
}
named_error_impls!(Conflict);

/// A 422 validation response with ordered, explicitly sourced issues.
pub struct ValidationError(HttpError);

impl ValidationError {
    /// Creates a validation response with the fixed message `input validation failed`.
    pub fn new(issues: impl IntoIterator<Item = SourcedValidationIssue>) -> Self {
        Self(HttpError::validation(issues))
    }
}
named_error_impls!(ValidationError);

/// A 500 response retaining its source without exposing it through formatting or JSON.
pub struct InternalError(HttpError);

impl InternalError {
    /// Retains the source and fixes the public message to `internal server error`.
    pub fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self(HttpError::internal(source))
    }

    #[cfg(all(feature = "http", feature = "database"))]
    pub(crate) fn from_source(source: impl Error + Send + 'static) -> Self {
        Self(HttpError::InternalWithSource(
            SourceRetainingClientError::new(INTERNAL_SERVER_ERROR_MESSAGE, source),
        ))
    }
}
named_error_impls!(InternalError);

/// A result type for HTTP handlers that use [`HttpError`] as their error type.
///
/// This is the delivery-layer result type. Use [`crate::core::Result`] for
/// application construction, dependency resolution, and server bootstrap
/// operations instead.
///
/// # Examples
///
/// ```
/// use mads_common::{HttpResult, Json};
///
/// fn health() -> HttpResult<Json<&'static str>> {
///     Ok(Json("ok"))
/// }
///
/// assert!(health().is_ok());
/// ```
pub type HttpResult<T> = std::result::Result<T, HttpError>;

/// A response wrapper that changes a successful inner response to 201 Created.
///
/// The inner value is converted using Axum's [`IntoResponse`] implementation,
/// then only the status code is replaced. Headers and the response body from
/// the inner value are preserved.
///
/// # Examples
///
/// ```
/// use axum::{http::StatusCode, response::IntoResponse};
///
/// use mads_common::Created;
///
/// let response = Created("created").into_response();
/// assert_eq!(response.status(), StatusCode::CREATED);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Created<T>(
    /// The successful response value to convert to an Axum response.
    pub T,
);

impl<T> IntoResponse for Created<T>
where
    T: IntoResponse,
{
    fn into_response(self) -> Response {
        let mut response = self.0.into_response();
        *response.status_mut() = StatusCode::CREATED;
        response
    }
}

/// An empty response with the 204 No Content status.
///
/// # Examples
///
/// ```
/// use axum::{http::StatusCode, response::IntoResponse};
///
/// use mads_common::NoContent;
///
/// let response = NoContent.into_response();
/// assert_eq!(response.status(), StatusCode::NO_CONTENT);
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContent;

impl IntoResponse for NoContent {
    fn into_response(self) -> Response {
        StatusCode::NO_CONTENT.into_response()
    }
}
