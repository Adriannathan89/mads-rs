//! Public HTTP response contract tests.

#![cfg(feature = "http")]

use std::io;

use mads_common::{
    BadRequest, Conflict, Created, Forbidden, HttpError, HttpResult, InternalError, Json,
    NoContent, NotFound, Unauthorized, ValidationError, ValidationIssue, ValidationSource,
    axum::{
        body::{Body, to_bytes},
        http::{
            HeaderValue, StatusCode,
            header::{CONTENT_TYPE, HeaderName},
        },
        response::{IntoResponse, Response},
    },
};

async fn assert_error_response(
    error: impl IntoResponse,
    expected_status: StatusCode,
    expected_body: &str,
) {
    let response = error.into_response();

    assert_eq!(response.status(), expected_status);
    assert_eq!(
        response.headers().get(CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/json"))
    );
    assert_eq!(
        std::str::from_utf8(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("error response body must be readable"),
        )
        .expect("error response body must be UTF-8"),
        expected_body
    );
}

#[tokio::test]
async fn named_errors_and_conversions_share_the_standard_envelope() {
    macro_rules! check {
        ($error:expr, $status:expr, $body:expr) => {{
            assert_error_response($error, $status, $body).await;
            let converted: HttpError = $error.into();
            assert_error_response(converted, $status, $body).await;
        }};
    }
    check!(
        BadRequest::new("safe message"),
        StatusCode::BAD_REQUEST,
        r#"{"error":{"code":"bad_request","message":"safe message"}}"#
    );
    check!(
        Unauthorized::new("safe message"),
        StatusCode::UNAUTHORIZED,
        r#"{"error":{"code":"unauthorized","message":"safe message"}}"#
    );
    check!(
        Forbidden::new("safe message"),
        StatusCode::FORBIDDEN,
        r#"{"error":{"code":"forbidden","message":"safe message"}}"#
    );
    check!(
        NotFound::new("safe message"),
        StatusCode::NOT_FOUND,
        r#"{"error":{"code":"not_found","message":"safe message"}}"#
    );
    check!(
        Conflict::new("safe message"),
        StatusCode::CONFLICT,
        r#"{"error":{"code":"conflict","message":"safe message"}}"#
    );
    check!(
        InternalError::new(io::Error::other("response-secret")),
        StatusCode::INTERNAL_SERVER_ERROR,
        r#"{"error":{"code":"internal","message":"internal server error"}}"#
    );
    check!(
        ValidationError::new(
            [ValidationIssue::custom("invalid_email", "email is invalid")
                .at_field("email")
                .with_source(ValidationSource::Body)]
        ),
        StatusCode::UNPROCESSABLE_ENTITY,
        r#"{"error":{"code":"validation_error","message":"input validation failed","issues":[{"source":"body","path":["email"],"code":"invalid_email","message":"email is invalid"}]}}"#
    );
}

#[test]
fn standard_debug_and_display_never_reveal_internal_sources() {
    let error = InternalError::new(io::Error::other("response-secret"));
    assert_eq!(
        std::error::Error::source(&error).unwrap().to_string(),
        "response-secret"
    );
    assert_eq!(error.to_string(), "internal server error");
    assert!(!format!("{error:?}").contains("response-secret"));
    let error: HttpError = error.into();
    assert_eq!(
        std::error::Error::source(&error).unwrap().to_string(),
        "response-secret"
    );
    assert!(!format!("{error:?}").contains("response-secret"));
    assert_eq!(error.to_string(), "internal server error");
    assert!(!format!("{:?}", BadRequest::new("safe message")).contains("safe message"));
}

#[cfg(not(feature = "database"))]
#[test]
fn http_only_standard_errors_remain_send_and_sync() {
    fn assert_send_and_sync<T: Send + Sync>() {}

    assert_send_and_sync::<HttpError>();
    assert_send_and_sync::<InternalError>();
}

#[tokio::test]
async fn sourced_issues_preserve_paths_order_and_explicit_sources() {
    let issue = ValidationIssue::custom("invalid", "invalid input")
        .at_field("items")
        .at_index(2);
    let sourced = issue.clone().with_source(ValidationSource::Query);
    assert_eq!(sourced.source(), ValidationSource::Query);
    assert_eq!(sourced.issue(), &issue);
    assert_eq!(
        serde_json::to_value(&sourced).unwrap(),
        serde_json::json!({"source":"query","path":["items",2],"code":"invalid","message":"invalid input"})
    );
    assert_error_response(ValidationError::new([sourced, ValidationIssue::custom("required", "required input").with_source(ValidationSource::Path)]), StatusCode::UNPROCESSABLE_ENTITY,
        r#"{"error":{"code":"validation_error","message":"input validation failed","issues":[{"source":"query","path":["items",2],"code":"invalid","message":"invalid input"},{"source":"path","path":[],"code":"required","message":"required input"}]}}"#).await;
}

#[tokio::test]
async fn created_and_no_content_set_exact_statuses() {
    let created = Created(Json(serde_json::json!({"id": 7}))).into_response();
    assert_eq!(created.status(), StatusCode::CREATED);

    let empty = NoContent.into_response();
    assert_eq!(empty.status(), StatusCode::NO_CONTENT);
    assert!(
        to_bytes(empty.into_body(), usize::MAX)
            .await
            .expect("no-content response body must be readable")
            .is_empty()
    );
}

#[tokio::test]
async fn created_preserves_the_inner_response_headers_and_body() {
    let mut inner = Response::new(Body::from("created user"));
    inner.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_static("request-123"),
    );

    let response = Created(inner).into_response();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response.headers().get("x-request-id"),
        Some(&HeaderValue::from_static("request-123"))
    );
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("created response body must be readable"),
        "created user"
    );
}

#[tokio::test]
async fn bad_request_serializes_its_safe_message() {
    assert_error_response(
        HttpError::bad_request("email is required"),
        StatusCode::BAD_REQUEST,
        r#"{"error":{"code":"bad_request","message":"email is required"}}"#,
    )
    .await;
}

#[tokio::test]
async fn not_found_serializes_its_safe_message() {
    assert_error_response(
        HttpError::not_found("user was not found"),
        StatusCode::NOT_FOUND,
        r#"{"error":{"code":"not_found","message":"user was not found"}}"#,
    )
    .await;
}

#[tokio::test]
async fn conflict_serializes_its_safe_message() {
    assert_error_response(
        HttpError::conflict("email is already registered"),
        StatusCode::CONFLICT,
        r#"{"error":{"code":"conflict","message":"email is already registered"}}"#,
    )
    .await;
}

#[tokio::test]
async fn internal_hides_its_source_in_the_response() {
    assert_error_response(
        HttpError::internal(io::Error::other("database credentials unavailable")),
        StatusCode::INTERNAL_SERVER_ERROR,
        r#"{"error":{"code":"internal","message":"internal server error"}}"#,
    )
    .await;
}

#[test]
fn http_result_alias_uses_http_error() {
    let result: HttpResult<()> = Err(HttpError::bad_request("email is required"));

    assert!(result.is_err());
}
