//! Validated request extraction after native Axum deserialization.

use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path as AxumPath, Query as AxumQuery, Request},
    http::request::Parts,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

use super::{
    Input, ValidationErrors, ValidationSource,
    rejection::{json_rejection_response, path_rejection_response, query_rejection_response},
};
use crate::ValidationError;

/// JSON extracted by Axum and validated before the handler is invoked.
#[must_use]
pub struct ValidatedJson<T>(
    /// The deserialized and validated input.
    pub T,
);

impl<T> ValidatedJson<T> {
    /// Consumes the wrapper and returns the validated input.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Input,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(input) = Json::<T>::from_request(request, state)
            .await
            .map_err(json_rejection_response)?;

        validate(input)
            .map(Self)
            .map_err(|errors| validation_response(errors, ValidationSource::Body))
    }
}

/// URL query parameters extracted by Axum and validated before handler invocation.
#[must_use]
pub struct ValidatedQuery<T>(
    /// The deserialized and validated input.
    pub T,
);

impl<T> ValidatedQuery<T> {
    /// Consumes the wrapper and returns the validated input.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned + Input,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AxumQuery(input) = AxumQuery::<T>::from_request_parts(parts, state)
            .await
            .map_err(query_rejection_response)?;

        validate(input)
            .map(Self)
            .map_err(|errors| validation_response(errors, ValidationSource::Query))
    }
}

/// URL path parameters extracted by Axum and validated before handler invocation.
#[must_use]
pub struct ValidatedPath<T>(
    /// The deserialized and validated input.
    pub T,
);

impl<T> ValidatedPath<T> {
    /// Consumes the wrapper and returns the validated input.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T, S> FromRequestParts<S> for ValidatedPath<T>
where
    T: DeserializeOwned + Input + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AxumPath(input) = AxumPath::<T>::from_request_parts(parts, state)
            .await
            .map_err(path_rejection_response)?;

        validate(input)
            .map(Self)
            .map_err(|errors| validation_response(errors, ValidationSource::Path))
    }
}

fn validate<T>(input: T) -> Result<T, ValidationErrors>
where
    T: Input,
{
    match input.validate() {
        Ok(()) => Ok(input),
        Err(errors) => Err(errors),
    }
}

fn validation_response(errors: ValidationErrors, source: ValidationSource) -> Response {
    let issues = errors
        .into_issues()
        .into_iter()
        .map(|issue| issue.with_source(source));
    ValidationError::new(issues).into_response()
}
