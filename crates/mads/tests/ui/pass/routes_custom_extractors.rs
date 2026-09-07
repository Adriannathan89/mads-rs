//! Confirms bare custom extractor names remain outside route syntax checks.

#![deny(missing_docs)]

use mads::prelude::*;

/// Application-defined extractor which intentionally shares Axum's short name.
struct Json;

impl<S> mads::common::axum::extract::FromRequestParts<S> for Json
where
    S: Send + Sync,
{
    type Rejection = mads::common::axum::http::StatusCode;

    async fn from_request_parts(
        _parts: &mut mads::common::axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self)
    }
}

/// Contract using a custom extractor whose body behavior belongs to Axum/rustc.
#[routes]
trait CustomExtractorRoutes {
    /// Deliberately leaves custom extractor ordering to native handler checking.
    #[post("/")]
    async fn create(&self, body: Json, id: mads::common::Path<u64>);
}

fn main() {}
