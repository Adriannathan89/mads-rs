//! Generated-route and native-Axum validated extractor integration.

use std::sync::atomic::{AtomicUsize, Ordering};

use mads::{
    axum::{
        Json as AxumJson, Router,
        body::{Body, to_bytes},
        extract::{Path as AxumPath, Query as AxumQuery},
        http::{Method, Request, StatusCode, header},
        response::Response,
        routing::{get as axum_get, post as axum_post},
    },
    prelude::*,
};
use serde_json::{Value, json};
use tower::ServiceExt;

static JSON_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static QUERY_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static PATH_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(serde::Deserialize, Input)]
struct BodyInput {
    #[validate(positive)]
    value: i64,
}

#[derive(serde::Deserialize, Input)]
struct QueryInput {
    #[validate(positive)]
    value: i64,
}

#[derive(serde::Deserialize, Input)]
struct PathInput {
    #[validate(positive)]
    id: i64,
}

#[routes(prefix = "/validated")]
trait ValidatedRoutes {
    #[post("/json")]
    async fn json(&self, input: ValidatedJson<BodyInput>) -> &'static str;

    #[get("/query")]
    async fn query(&self, input: ValidatedQuery<QueryInput>) -> &'static str;

    #[get("/path/:id")]
    async fn path(&self, input: ValidatedPath<PathInput>) -> &'static str;
}

#[controller(routes = [ValidatedRoutes])]
struct ValidatedController;

impl ValidatedRoutes for ValidatedController {
    async fn json(&self, _: ValidatedJson<BodyInput>) -> &'static str {
        JSON_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        "json"
    }

    async fn query(&self, _: ValidatedQuery<QueryInput>) -> &'static str {
        QUERY_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        "query"
    }

    async fn path(&self, _: ValidatedPath<PathInput>) -> &'static str {
        PATH_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        "path"
    }
}

fn reset_handler_calls() {
    JSON_HANDLER_CALLS.store(0, Ordering::SeqCst);
    QUERY_HANDLER_CALLS.store(0, Ordering::SeqCst);
    PATH_HANDLER_CALLS.store(0, Ordering::SeqCst);
}

fn handler_calls() -> [usize; 3] {
    [
        JSON_HANDLER_CALLS.load(Ordering::SeqCst),
        QUERY_HANDLER_CALLS.load(Ordering::SeqCst),
        PATH_HANDLER_CALLS.load(Ordering::SeqCst),
    ]
}

async fn response_json(response: Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body must be readable"),
    )
    .expect("response body must be JSON")
}

#[tokio::test]
async fn generated_validated_extractors_reject_before_every_handler() {
    reset_handler_calls();
    let application = Mads::builder().build().await.unwrap();
    let router = build_router(&application).unwrap();

    for (request, source) in [
        (
            Request::builder()
                .method(Method::POST)
                .uri("/validated/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"value":0}"#))
                .unwrap(),
            "body",
        ),
        (
            Request::get("/validated/query?value=0")
                .body(Body::empty())
                .unwrap(),
            "query",
        ),
        (
            Request::get("/validated/path/0")
                .body(Body::empty())
                .unwrap(),
            "path",
        ),
    ] {
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({
                "error": {
                    "code": "validation_error",
                    "message": "input validation failed",
                    "issues": [{
                        "source": source,
                        "path": [if source == "path" { "id" } else { "value" }],
                        "code": "too_small",
                        "message": "value is too small",
                    }],
                },
            })
        );
        assert_eq!(handler_calls(), [0, 0, 0]);
    }

    for (request, expected) in [
        (
            Request::builder()
                .method(Method::POST)
                .uri("/validated/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"value":2}"#))
                .unwrap(),
            "json",
        ),
        (
            Request::get("/validated/query?value=2")
                .body(Body::empty())
                .unwrap(),
            "query",
        ),
        (
            Request::get("/validated/path/2")
                .body(Body::empty())
                .unwrap(),
            "path",
        ),
    ] {
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            expected
        );
    }
    assert_eq!(handler_calls(), [1, 1, 1]);
}

async fn native_json(_: AxumJson<BodyInput>) {}

async fn native_query(_: AxumQuery<QueryInput>) {}

async fn native_path(_: AxumPath<PathInput>) {}

#[tokio::test]
async fn ordinary_axum_extractors_keep_native_rejection_bodies() {
    let application = Mads::builder().build().await.unwrap();
    let router = build_router(&application).unwrap().merge(
        Router::new()
            .route("/native/json", axum_post(native_json))
            .route("/native/query", axum_get(native_query))
            .route("/native/path/{id}", axum_get(native_path)),
    );

    let requests = [
        (
            Request::builder()
                .method(Method::POST)
                .uri("/native/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"value":"private-value"}"#))
                .unwrap(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "Failed to deserialize the JSON body into the target type",
        ),
        (
            Request::get("/native/query?value=private-value")
                .body(Body::empty())
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "Failed to deserialize query string",
        ),
        (
            Request::get("/native/path/private-value")
                .body(Body::empty())
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "Invalid URL",
        ),
    ];

    for (request, status, native_prefix) in requests {
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), status);
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            body.starts_with(native_prefix),
            "unexpected native body: {body}"
        );
        assert!(!body.contains("\"code\":\"validation_error\""));
    }
}
