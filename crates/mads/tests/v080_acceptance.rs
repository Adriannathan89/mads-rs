//! Release-level facade acceptance coverage for the v0.8 public surface.

#![cfg(all(feature = "http", feature = "database"))]

use std::{
    io,
    sync::atomic::{AtomicUsize, Ordering},
};

use mads::{
    axum::{
        Json as AxumJson, Router,
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
        response::{IntoResponse, Response},
        routing::post as axum_post,
    },
    core::{Config, ConfigBuilder, MapSource},
    prelude::*,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const SECRET_SENTINEL: &str = "v080-facade-secret-sentinel";

static VALIDATED_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static NATIVE_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);

mod application {
    use super::*;

    #[derive(Configuration)]
    #[config(prefix = "acceptance")]
    pub(super) struct AcceptanceConfig {
        #[allow(dead_code)]
        pub(super) api_key: Secret<String>,
    }

    #[derive(serde::Deserialize, Input)]
    pub(super) struct CreateAccount {
        #[validate(email)]
        pub(super) email: String,
        #[validate(length(min = 3))]
        pub(super) username: String,
    }

    #[module]
    pub(super) struct AcceptanceModule;

    #[provider]
    fn acceptance_config(config: Config) -> mads::core::Result<AcceptanceConfig> {
        Ok(config.parse()?)
    }

    #[service]
    pub(super) struct AcceptanceService;

    impl AcceptanceService {
        pub(super) fn create_account(&self) {
            VALIDATED_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[routes(prefix = "/acceptance")]
    pub(super) trait AcceptanceRoutes {
        #[post("/accounts")]
        async fn create(&self, input: ValidatedJson<CreateAccount>) -> &'static str;

        #[get("/missing")]
        async fn missing(&self) -> HttpResult<&'static str>;
    }

    #[controller(routes = [AcceptanceRoutes])]
    pub(super) struct AcceptanceController {
        service: AcceptanceService,
    }

    impl AcceptanceRoutes for AcceptanceController {
        async fn create(&self, _: ValidatedJson<CreateAccount>) -> &'static str {
            self.service.create_account();
            "created"
        }

        async fn missing(&self) -> HttpResult<&'static str> {
            Err(NotFound::new("acceptance resource was not found").into())
        }
    }
}

fn configured_application() -> mads::core::MadsBuilder {
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "acceptance fixture",
            [("acceptance.api_key", SECRET_SENTINEL)],
        ))
        .build()
        .expect("fixed acceptance configuration should build");
    let mut builder = Mads::builder_with_config(config);
    builder
        .root::<application::AcceptanceModule>()
        .expect("acceptance module should be selectable");
    builder
}

fn reset_handler_calls() {
    VALIDATED_HANDLER_CALLS.store(0, Ordering::SeqCst);
    NATIVE_HANDLER_CALLS.store(0, Ordering::SeqCst);
}

async fn response_json(response: Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable"),
    )
    .expect("response should contain JSON")
}

async fn native_json(_: AxumJson<application::CreateAccount>) -> &'static str {
    NATIVE_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
    "native"
}

#[tokio::test]
async fn v080_facade_validates_before_service_invocation_and_preserves_native_json() {
    reset_handler_calls();
    let application = configured_application()
        .build()
        .await
        .expect("configured acceptance application should build");
    let router = build_router(&application)
        .expect("generated routes should build")
        .merge(Router::new().route("/native", axum_post(native_json)));

    let invalid = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/acceptance/accounts")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"email":"not-an-email","username":"x"}"#))
                .expect("invalid request should build"),
        )
        .await
        .expect("router should answer invalid request");
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(invalid).await,
        json!({
            "error": {
                "code": "validation_error",
                "message": "input validation failed",
                "issues": [
                    {
                        "source": "body",
                        "path": ["email"],
                        "code": "invalid_format",
                        "message": "invalid email address",
                    },
                    {
                        "source": "body",
                        "path": ["username"],
                        "code": "too_small",
                        "message": "value is too small",
                    }
                ]
            }
        })
    );
    assert_eq!(VALIDATED_HANDLER_CALLS.load(Ordering::SeqCst), 0);

    let valid = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/acceptance/accounts")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"email":"person@example.com","username":"person"}"#,
                ))
                .expect("valid request should build"),
        )
        .await
        .expect("router should answer valid request");
    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(valid.into_body(), usize::MAX)
            .await
            .expect("valid response should be readable"),
        "created"
    );
    assert_eq!(VALIDATED_HANDLER_CALLS.load(Ordering::SeqCst), 1);

    let native = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/native")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"email":"not-an-email","username":"x"}"#))
                .expect("native request should build"),
        )
        .await
        .expect("native router should answer request");
    assert_eq!(native.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(native.into_body(), usize::MAX)
            .await
            .expect("native response should be readable"),
        "native"
    );
    assert_eq!(NATIVE_HANDLER_CALLS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn v080_facade_preserves_named_errors_and_redacts_internal_values() {
    let application = configured_application()
        .build()
        .await
        .expect("configured acceptance application should build");
    let missing = build_router(&application)
        .expect("generated routes should build")
        .oneshot(
            Request::builder()
                .uri("/acceptance/missing")
                .body(Body::empty())
                .expect("missing route request should build"),
        )
        .await
        .expect("router should answer missing route");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing).await,
        json!({
            "error": {
                "code": "not_found",
                "message": "acceptance resource was not found",
            }
        })
    );

    let secret = Secret::new(SECRET_SENTINEL.to_owned());
    assert_eq!(secret.to_string(), "[REDACTED]");
    assert_eq!(format!("{secret:?}"), "[REDACTED]");

    let internal = InternalError::new(io::Error::other(SECRET_SENTINEL));
    let formatting = format!("{internal}\n{internal:?}");
    let response = internal.into_response();
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("internal response should be readable")
            .to_vec(),
    )
    .expect("internal response should be UTF-8");
    assert_eq!(
        body,
        r#"{"error":{"code":"internal","message":"internal server error"}}"#
    );
    for output in [&formatting, &body] {
        assert!(
            !output.contains(SECRET_SENTINEL),
            "internal values must not reach ordinary formatting or responses: {output}"
        );
    }
}

#[tokio::test]
async fn v080_facade_missing_typed_configuration_fails_before_construction() {
    let mut builder = Mads::builder_with_config(Config::empty());
    builder
        .root::<application::AcceptanceModule>()
        .expect("acceptance module should be selectable");
    let Err(error) = builder.build().await else {
        panic!("missing selected typed configuration must fail construction");
    };
    assert_eq!(error.code(), mads::core::MADS006);
    let source = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref::<mads::core::Error>())
        .expect("construction error should retain its typed configuration cause");
    assert_eq!(source.code(), mads::core::MADS020);
    assert_eq!(source.diagnostic().subject(), Some("acceptance.api_key"));
    let rendered = format!("{error}\n{error:?}\n{source}\n{source:?}");
    assert!(
        !rendered.contains(SECRET_SENTINEL),
        "missing configuration diagnostics must not expose values: {rendered}"
    );
}

#[test]
fn v080_facade_exposes_explicit_database_http_mapping_without_postgres() {
    fn managed<T>(value: mads::DatabaseResult<T>) -> mads::HttpResult<T> {
        value.into_http()
    }

    fn native<T>(value: mads::diesel::QueryResult<T>) -> mads::HttpResult<T> {
        value.into_http()
    }

    let _ = managed::<()>;
    let _ = native::<()>;
}
