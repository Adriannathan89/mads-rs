//! Validated request extraction after native Axum deserialization.

use axum::{
    Json,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

use super::{Input, ValidationSource, rejection::json_rejection_response};
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

        if let Err(errors) = input.validate() {
            let issues = errors
                .into_issues()
                .into_iter()
                .map(|issue| issue.with_source(ValidationSource::Body));
            return Err(ValidationError::new(issues).into_response());
        }

        Ok(Self(input))
    }
}
