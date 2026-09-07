//! Explicit HTTP delivery mapping for managed and native Diesel query results.

use std::error::Error as StdError;

use crate::{Conflict, HttpError, HttpResult, InternalError, NotFound};
use diesel::result::{DatabaseErrorKind, Error};

use super::DatabaseError;

/// Converts an approved database result into a delivery-layer HTTP result.
///
/// This explicit method is the application's policy boundary: it maps only
/// Diesel not-found and unique-violation errors to client failures. Every
/// other database failure becomes a redacted internal server error.
pub trait IntoHttpResult<T> {
    /// Converts this result into its HTTP delivery representation.
    fn into_http(self) -> HttpResult<T>;
}

#[derive(Clone, Copy)]
enum Classification {
    NotFound,
    Conflict,
    Internal,
}

fn classify(error: &Error) -> Classification {
    match error {
        Error::NotFound => Classification::NotFound,
        Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => Classification::Conflict,
        _ => Classification::Internal,
    }
}

fn map_error(source: impl StdError + Send + 'static, classification: Classification) -> HttpError {
    match classification {
        Classification::NotFound => NotFound::from_source(source).into(),
        Classification::Conflict => Conflict::from_source(source).into(),
        Classification::Internal => InternalError::from_source(source).into(),
    }
}

impl<T> IntoHttpResult<T> for Result<T, DatabaseError> {
    fn into_http(self) -> HttpResult<T> {
        self.map_err(|error| {
            let classification = match &error {
                DatabaseError::Query(error) => classify(error),
                _ => Classification::Internal,
            };
            map_error(error, classification)
        })
    }
}

impl<T> IntoHttpResult<T> for diesel::QueryResult<T> {
    fn into_http(self) -> HttpResult<T> {
        self.map_err(|error| {
            let classification = classify(&error);
            map_error(error, classification)
        })
    }
}
