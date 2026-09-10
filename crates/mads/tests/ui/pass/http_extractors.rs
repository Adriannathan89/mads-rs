//! Confirms route handlers accept standard and native Axum extractors.

#![deny(missing_docs)]

use mads::common::{Header, Json, Path, Query, Request, ValidatedJson, headers};
use mads::prelude::*;
use serde::{Deserialize, Serialize};

/// A request path and body fixture.
#[derive(Deserialize, Serialize)]
struct User {
    /// Stable user identifier.
    id: u64,
}

/// A query-string fixture.
#[derive(Deserialize)]
struct SearchQuery {
    /// Requested page number.
    page: u64,
}

/// A JSON request body validated before dispatch.
#[derive(Deserialize, mads::Input)]
struct CreateUser {
    /// User name received in the request body.
    name: String,
}

/// A controller demonstrating extractor forwarding.
#[controller(routes = [ExtractorRoutes])]
struct ExtractorController;

/// Route contract using the standard and native extractor surfaces.
#[routes(prefix = "/users")]
trait ExtractorRoutes {
    /// Returns a user after forwarding every supported extractor.
    #[get("/:id")]
    async fn get_user(
        &self,
        id: Path<u64>,
        query: Query<SearchQuery>,
        agent: Header<headers::UserAgent>,
        extension: mads::common::axum::extract::Extension<String>,
        request: Request,
    ) -> Json<User>;

    /// Creates a user after extracting request parts before the JSON body.
    #[mads::post("/:id")]
    async fn create_user(
        &self,
        id: Path<u64>,
        query: Query<SearchQuery>,
        agent: Header<headers::UserAgent>,
        request: Json<CreateUser>,
    ) -> Json<User>;

    /// Creates a validated user after extracting request parts before the JSON body.
    #[mads::post("/:id/validated")]
    async fn create_validated_user(
        &self,
        id: Path<u64>,
        query: Query<SearchQuery>,
        agent: Header<headers::UserAgent>,
        request: ValidatedJson<CreateUser>,
    ) -> Json<User>;
}

impl ExtractorRoutes for ExtractorController {
    async fn get_user(
        &self,
        Path(id): Path<u64>,
        Query(query): Query<SearchQuery>,
        Header(agent): Header<headers::UserAgent>,
        mads::common::axum::extract::Extension(extension): mads::common::axum::extract::Extension<
            String,
        >,
        request: Request,
    ) -> Json<User> {
        let _ = (query.page, agent, extension, request);
        Json(User { id })
    }

    async fn create_user(
        &self,
        Path(id): Path<u64>,
        Query(query): Query<SearchQuery>,
        Header(agent): Header<headers::UserAgent>,
        Json(request): Json<CreateUser>,
    ) -> Json<User> {
        let _ = (query.page, agent, request.name);
        Json(User { id })
    }

    async fn create_validated_user(
        &self,
        Path(id): Path<u64>,
        Query(query): Query<SearchQuery>,
        Header(agent): Header<headers::UserAgent>,
        ValidatedJson(request): ValidatedJson<CreateUser>,
    ) -> Json<User> {
        let _ = (query.page, agent, request.name);
        Json(User { id })
    }
}

fn main() {}
