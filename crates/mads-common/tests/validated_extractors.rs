//! Validated HTTP extractor behavior.

#![cfg(feature = "http")]

use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use http_body::Frame;
use mads_common::{
    Input, ValidatedJson, ValidatedPath, ValidatedQuery,
    axum::{
        Json, Router,
        body::{Body, Bytes, HttpBody, to_bytes},
        extract::{DefaultBodyLimit, State},
        http::{
            Request, StatusCode,
            header::{CONTENT_TYPE, HeaderValue},
        },
        response::Response,
        routing::{get, post},
    },
};
use serde_json::{Value, json};
use tower::ServiceExt;

type HandlerCounter = Arc<AtomicUsize>;

#[tokio::test]
async fn query_path_server_configuration_errors_are_redacted_internal_errors() {
    async fn wrong_number(_: ValidatedPath<String>) {
        panic!("invalid extraction must not invoke the handler");
    }
    async fn unsupported(_: ValidatedPath<Option<String>>) {
        panic!("invalid extraction must not invoke the handler");
    }
    let router = Router::new()
        .route("/wrong/{first}/{second}", get(wrong_number))
        .route("/unsupported/{value}", get(unsupported));

    for uri in [
        "/wrong/private-one/private-two",
        "/unsupported/private-value",
    ] {
        let response = router
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response_json(response).await,
            json!({
                "error": {"code": "internal", "message": "internal server error"}
            })
        );
    }
}

#[derive(serde::Deserialize, Input)]
#[serde(rename_all = "camelCase")]
struct JsonInput {
    #[validate(email, length(max = 10))]
    email_address: String,
    #[validate(range(min = 1, max = 10), multiple_of = 2)]
    count: i64,
}

async fn accept_json(
    State(counter): State<HandlerCounter>,
    input: ValidatedJson<JsonInput>,
) -> Json<Value> {
    let ValidatedJson(input) = input;
    let input = ValidatedJson(input).into_inner();
    counter.fetch_add(1, Ordering::SeqCst);
    Json(json!({"email": input.email_address, "count": input.count}))
}

fn json_router(counter: HandlerCounter) -> Router {
    Router::new()
        .route("/input", post(accept_json))
        .with_state(counter)
}

fn json_request(body: impl Into<Body>, content_type: Option<&'static str>) -> Request<Body> {
    let mut request = Request::post("/input").body(body.into()).unwrap();
    if let Some(content_type) = content_type {
        request
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    }
    request
}

async fn response_json(response: Response) -> Value {
    assert_eq!(
        response.headers().get(CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/json"))
    );
    serde_json::from_slice(
        &to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body must be readable"),
    )
    .expect("response body must be JSON")
}

fn validation_envelope(source: &str, path: Value, code: &str, message: &str) -> Value {
    json!({
        "error": {
            "code": "validation_error",
            "message": "input validation failed",
            "issues": [{
                "source": source,
                "path": path,
                "code": code,
                "message": message,
            }],
        },
    })
}

#[tokio::test]
async fn json_valid_input_reaches_the_handler_once() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .oneshot(json_request(
            r#"{"emailAddress":"a@b.co","count":4}"#,
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({"email":"a@b.co","count":4})
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn json_malformed_syntax_is_one_sourced_validation_issue() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .oneshot(json_request("{", Some("application/json")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        validation_envelope(
            "body",
            json!([]),
            "invalid_syntax",
            "input syntax is invalid"
        )
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn json_type_conversion_is_one_sourced_validation_issue() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .oneshot(json_request(
            r#"{"emailAddress":"a@b.co","count":"private-value"}"#,
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = response_json(response).await;
    assert_eq!(
        body,
        validation_envelope(
            "body",
            json!(["count"]),
            "invalid_type",
            "input has an invalid type"
        )
    );
    assert!(!body.to_string().contains("private-value"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn json_missing_renamed_field_is_one_required_issue_at_its_external_path() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .oneshot(json_request(r#"{"count":4}"#, Some("application/json")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        validation_envelope(
            "body",
            json!(["emailAddress"]),
            "required",
            "required value is missing"
        )
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[derive(serde::Deserialize, Input)]
#[serde(rename_all = "camelCase")]
struct QueryInput {
    #[validate(email, length(max = 10))]
    email_address: String,
    #[validate(range(min = 1, max = 10), multiple_of = 2)]
    page_number: i64,
}

#[derive(serde::Deserialize, Input)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictQueryInput {
    page_number: i64,
}

async fn accept_query(
    State(counter): State<HandlerCounter>,
    input: ValidatedQuery<QueryInput>,
) -> Json<Value> {
    let ValidatedQuery(input) = input;
    counter.fetch_add(1, Ordering::SeqCst);
    Json(json!({"email": input.email_address, "page": input.page_number}))
}

async fn accept_strict_query(
    State(counter): State<HandlerCounter>,
    _: ValidatedQuery<StrictQueryInput>,
) {
    counter.fetch_add(1, Ordering::SeqCst);
}

fn query_router(counter: HandlerCounter) -> Router {
    Router::new()
        .route("/query", get(accept_query))
        .route("/query-strict", get(accept_strict_query))
        .with_state(counter)
}

#[tokio::test]
async fn query_path_valid_query_uses_external_names_and_ignores_unknown_fields() {
    let counter = HandlerCounter::default();
    let response = query_router(counter.clone())
        .oneshot(
            Request::get("/query?emailAddress=a%40b.co&pageNumber=4&ordinarySerdeIgnoresThis=yes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({"email":"a@b.co","page":4})
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn query_path_query_type_conversion_is_one_redacted_query_issue() {
    let counter = HandlerCounter::default();
    let response = query_router(counter.clone())
        .oneshot(
            Request::get("/query?emailAddress=a%40b.co&pageNumber=private-value")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = response_json(response).await;
    assert_eq!(
        body,
        validation_envelope(
            "query",
            json!(["pageNumber"]),
            "invalid_type",
            "input has an invalid type"
        )
    );
    assert!(!body.to_string().contains("private-value"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_query_missing_renamed_field_is_one_required_query_issue() {
    let counter = HandlerCounter::default();
    let response = query_router(counter.clone())
        .oneshot(
            Request::get("/query?pageNumber=4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        validation_envelope(
            "query",
            json!(["emailAddress"]),
            "required",
            "required value is missing"
        )
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_query_validation_returns_every_query_issue_in_derive_order() {
    let counter = HandlerCounter::default();
    let response = query_router(counter.clone())
        .oneshot(
            Request::get("/query?emailAddress=not-an-email&pageNumber=-3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({
            "error": {
                "code": "validation_error",
                "message": "input validation failed",
                "issues": [
                    {"source":"query","path":["emailAddress"],"code":"invalid_format","message":"invalid email address"},
                    {"source":"query","path":["emailAddress"],"code":"too_big","message":"value is too big"},
                    {"source":"query","path":["pageNumber"],"code":"too_small","message":"value is too small"},
                    {"source":"query","path":["pageNumber"],"code":"not_multiple_of","message":"value is not a multiple of the required number"},
                ],
            },
        })
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_query_honors_serde_deny_unknown_fields() {
    let counter = HandlerCounter::default();
    let response = query_router(counter.clone())
        .oneshot(
            Request::get("/query-strict?pageNumber=4&unexpected=private-value")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = response_json(response).await;
    assert_eq!(
        body,
        validation_envelope(
            "query",
            json!(["unexpected"]),
            "invalid_type",
            "input has an invalid type"
        )
    );
    assert!(!body.to_string().contains("private-value"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[derive(serde::Deserialize, Input)]
#[serde(rename_all = "camelCase")]
struct PathInput {
    #[validate(range(min = 1, max = 10), multiple_of = 2)]
    user_id: i64,
    #[validate(length(min = 3, max = 5))]
    slug: String,
}

#[derive(serde::Deserialize, Input)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictPathInput {
    user_id: i64,
}

async fn accept_path(
    State(counter): State<HandlerCounter>,
    input: ValidatedPath<PathInput>,
) -> Json<Value> {
    let input = input.into_inner();
    counter.fetch_add(1, Ordering::SeqCst);
    Json(json!({"userId": input.user_id, "slug": input.slug}))
}

async fn accept_required_path(State(counter): State<HandlerCounter>, _: ValidatedPath<PathInput>) {
    counter.fetch_add(1, Ordering::SeqCst);
}

async fn accept_strict_path(
    State(counter): State<HandlerCounter>,
    _: ValidatedPath<StrictPathInput>,
) {
    counter.fetch_add(1, Ordering::SeqCst);
}

fn path_router(counter: HandlerCounter) -> Router {
    Router::new()
        .route(
            "/path/{userId}/{slug}/{ordinarySerdeIgnoresThis}",
            get(accept_path),
        )
        .route("/path-required/{userId}", get(accept_required_path))
        .route(
            "/path-strict/{userId}/{unexpected}",
            get(accept_strict_path),
        )
        .with_state(counter)
}

#[tokio::test]
async fn query_path_valid_path_uses_into_inner_external_names_and_unknown_field_defaults() {
    let counter = HandlerCounter::default();
    let response = path_router(counter.clone())
        .oneshot(
            Request::get("/path/4/rust/ignored")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({"userId":4,"slug":"rust"})
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn query_path_path_invalid_utf8_is_one_redacted_syntax_issue() {
    let counter = HandlerCounter::default();
    let response = path_router(counter.clone())
        .oneshot(
            Request::get("/path/%FF/rust/ignored")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = response_json(response).await;
    assert_eq!(
        body,
        validation_envelope(
            "path",
            json!(["userId"]),
            "invalid_syntax",
            "input syntax is invalid"
        )
    );
    assert!(!body.to_string().contains("%FF"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_path_type_conversion_is_one_redacted_path_issue() {
    let counter = HandlerCounter::default();
    let response = path_router(counter.clone())
        .oneshot(
            Request::get("/path/private-value/rust/ignored")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = response_json(response).await;
    assert_eq!(
        body,
        validation_envelope(
            "path",
            json!(["userId"]),
            "invalid_type",
            "input has an invalid type"
        )
    );
    assert!(!body.to_string().contains("private-value"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_path_missing_field_is_one_required_path_issue() {
    let counter = HandlerCounter::default();
    let response = path_router(counter.clone())
        .oneshot(
            Request::get("/path-required/4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        validation_envelope(
            "path",
            json!(["slug"]),
            "required",
            "required value is missing"
        )
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_path_validation_returns_every_path_issue_in_derive_order() {
    let counter = HandlerCounter::default();
    let response = path_router(counter.clone())
        .oneshot(
            Request::get("/path/-3/toolong/ignored")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({
            "error": {
                "code": "validation_error",
                "message": "input validation failed",
                "issues": [
                    {"source":"path","path":["userId"],"code":"too_small","message":"value is too small"},
                    {"source":"path","path":["userId"],"code":"not_multiple_of","message":"value is not a multiple of the required number"},
                    {"source":"path","path":["slug"],"code":"too_big","message":"value is too big"},
                ],
            },
        })
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn query_path_path_honors_serde_deny_unknown_fields() {
    let counter = HandlerCounter::default();
    let response = path_router(counter.clone())
        .oneshot(
            Request::get("/path-strict/4/private-value")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = response_json(response).await;
    assert_eq!(
        body,
        validation_envelope(
            "path",
            json!(["unexpected"]),
            "invalid_type",
            "input has an invalid type"
        )
    );
    assert!(!body.to_string().contains("private-value"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn json_post_conversion_validation_returns_every_issue_in_derive_order() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .oneshot(json_request(
            r#"{"emailAddress":"not-an-email","count":-3}"#,
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({
            "error": {
                "code": "validation_error",
                "message": "input validation failed",
                "issues": [
                    {"source":"body","path":["emailAddress"],"code":"invalid_format","message":"invalid email address"},
                    {"source":"body","path":["emailAddress"],"code":"too_big","message":"value is too big"},
                    {"source":"body","path":["count"],"code":"too_small","message":"value is too small"},
                    {"source":"body","path":["count"],"code":"not_multiple_of","message":"value is not a multiple of the required number"},
                ],
            },
        })
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn json_missing_or_wrong_content_type_is_a_safe_unsupported_media_type() {
    let counter = HandlerCounter::default();
    let app = json_router(counter.clone());

    for content_type in [None, Some("text/plain")] {
        let response = app
            .clone()
            .oneshot(json_request(
                r#"{"emailAddress":"a@b.co","count":4}"#,
                content_type,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(
            response_json(response).await,
            json!({"error":{"code":"unsupported_media_type","message":"content type must be application/json"}})
        );
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn json_configured_body_limit_overflow_is_a_safe_payload_too_large() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .layer(DefaultBodyLimit::max(16))
        .oneshot(json_request(
            r#"{"emailAddress":"a@b.co","count":4}"#,
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response_json(response).await,
        json!({"error":{"code":"payload_too_large","message":"request body is too large"}})
    );
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[derive(Debug, Default)]
struct FailingBody {
    emitted: bool,
}

impl HttpBody for FailingBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        if this.emitted {
            Poll::Ready(None)
        } else {
            this.emitted = true;
            Poll::Ready(Some(Err(io::Error::other("private body-read detail"))))
        }
    }
}

#[tokio::test]
async fn json_other_client_body_read_failure_is_a_safe_bad_request() {
    let counter = HandlerCounter::default();
    let response = json_router(counter.clone())
        .oneshot(json_request(
            Body::new(FailingBody::default()),
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({"error":{"code":"bad_request","message":"request body could not be read"}})
    );
    assert!(!body.to_string().contains("private body-read detail"));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}
