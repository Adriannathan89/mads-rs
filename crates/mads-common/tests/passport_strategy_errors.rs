//! Bearer Passport authentication rejection and redaction behavior.

#![cfg(all(feature = "http", feature = "jwt"))]

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;

use axum::{
    body::{Body, to_bytes},
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, WWW_AUTHENTICATE},
    },
};
use mads_common::{
    JwtClaims, JwtService, JwtSignOptions, JwtTokenKind, PassportContext, PassportError,
    PassportPrincipal, PassportRejection, PassportResult, PassportStrategy, build_router,
    controller,
    core::{Config, ConfigBuilder, Mads, MapSource},
    passport_strategy, routes,
};
use tower::ServiceExt;

static HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static STRATEGY_CALLS: AtomicUsize = AtomicUsize::new(0);
static STRATEGY_MODE: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct ErrorClaims {
    user_id: u64,
}

struct ErrorPrincipal;

impl PassportPrincipal for ErrorPrincipal {
    fn has_role(&self, _role: &str) -> bool {
        true
    }

    fn has_permission(&self, _permission: &str) -> bool {
        true
    }
}

#[mads_core::service]
struct ErrorStrategy;

#[passport_strategy(name = "jwt")]
impl PassportStrategy for ErrorStrategy {
    type Claims = ErrorClaims;
    type Principal = ErrorPrincipal;

    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;

    async fn validate(
        &self,
        _context: &PassportContext<'_>,
        _claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        STRATEGY_CALLS.fetch_add(1, Ordering::SeqCst);
        match STRATEGY_MODE.load(Ordering::SeqCst) {
            0 => Ok(ErrorPrincipal),
            1 => Err(PassportError::reject()),
            2 => Err(PassportError::internal(std::io::Error::other(
                "strategy-sensitive-source-sentinel",
            ))),
            _ => unreachable!("test strategy modes are exhaustive"),
        }
    }
}

#[routes(prefix = "/errors")]
#[mads_common::guard(strategy = "jwt", principal = ErrorPrincipal)]
trait ErrorRoutes {
    #[mads_common::get("/protected")]
    async fn protected(&self) -> &'static str;
}

#[controller(routes = [ErrorRoutes])]
struct ErrorController;

impl ErrorRoutes for ErrorController {
    async fn protected(&self) -> &'static str {
        HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        "handler-ran"
    }
}

fn config() -> Config {
    ConfigBuilder::new()
        .source(MapSource::new(
            "mads.toml",
            [
                ("passport.secret", "01234567890123456789012345678901"),
                ("passport.max_token_bytes", "512"),
            ],
        ))
        .build()
        .unwrap()
}

async fn request(
    router: &axum::Router,
    authorization: impl IntoIterator<Item = axum::http::HeaderValue>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .uri("/errors/protected")
        .body(Body::empty())
        .unwrap();
    for value in authorization {
        request.headers_mut().append(AUTHORIZATION, value);
    }
    router.clone().oneshot(request).await.unwrap()
}

async fn assert_authentication_failure(
    router: &axum::Router,
    authorization: impl IntoIterator<Item = axum::http::HeaderValue>,
) {
    HANDLER_CALLS.store(0, Ordering::SeqCst);
    let response = request(router, authorization).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get_all(WWW_AUTHENTICATE).iter().count(),
        1
    );
    assert_eq!(response.headers()[WWW_AUTHENTICATE], "Bearer");
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"{\"error\":{\"code\":\"unauthorized\",\"message\":\"authentication was rejected\"}}"
    );
    assert_eq!(HANDLER_CALLS.load(Ordering::SeqCst), 0);
}

fn assert_headers_do_not_contain(response: &axum::response::Response, sentinel: &str) {
    assert!(
        response
            .headers()
            .values()
            .all(|value| { !String::from_utf8_lossy(value.as_bytes()).contains(sentinel) })
    );
}

#[tokio::test]
async fn invalid_bearer_credentials_and_strategy_rejections_are_generic_401s() {
    let application = Mads::builder_with_config(config()).build().await.unwrap();
    let jwt = application.context().resolve::<JwtService>().unwrap();
    let access = jwt
        .sign(
            ErrorClaims { user_id: 7 },
            JwtSignOptions::access(Duration::from_secs(60)),
        )
        .unwrap();
    let refresh = jwt
        .sign(
            ErrorClaims { user_id: 7 },
            JwtSignOptions::refresh(Duration::from_secs(60)),
        )
        .unwrap();
    let router = build_router(&application).unwrap();

    STRATEGY_MODE.store(0, Ordering::SeqCst);
    for authorization in [
        Vec::new(),
        vec!["Basic abc".parse().unwrap()],
        vec!["Bearer".parse().unwrap()],
        vec!["Bearer token extra".parse().unwrap()],
        vec![axum::http::HeaderValue::from_bytes(b"Bearer \xff").unwrap()],
        vec!["Bearer malformed.jwt.value".parse().unwrap()],
        vec![format!("Bearer {}", "x".repeat(513)).parse().unwrap()],
        vec![format!("Bearer {refresh}").parse().unwrap()],
        vec![format!("Bearer {access}x").parse().unwrap()],
    ] {
        STRATEGY_CALLS.store(0, Ordering::SeqCst);
        assert_authentication_failure(&router, authorization).await;
        assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 0);
    }

    assert_authentication_failure(
        &router,
        ["Bearer one".parse().unwrap(), "Bearer two".parse().unwrap()],
    )
    .await;

    let expired = jwt
        .sign(
            ErrorClaims { user_id: 7 },
            JwtSignOptions::access(Duration::from_secs(1)),
        )
        .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    STRATEGY_CALLS.store(0, Ordering::SeqCst);
    assert_authentication_failure(&router, [format!("Bearer {expired}").parse().unwrap()]).await;
    assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 0);

    let credential_sentinel = "bearer-credential-sentinel";
    let response = request(
        &router,
        [format!("Bearer {credential_sentinel}").parse().unwrap()],
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[WWW_AUTHENTICATE], "Bearer");
    assert_headers_do_not_contain(&response, credential_sentinel);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert_eq!(
        body,
        "{\"error\":{\"code\":\"unauthorized\",\"message\":\"authentication was rejected\"}}"
    );
    assert!(!body.contains(credential_sentinel));
    assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 0);

    STRATEGY_MODE.store(1, Ordering::SeqCst);
    STRATEGY_CALLS.store(0, Ordering::SeqCst);
    assert_authentication_failure(&router, [format!("Bearer {access}").parse().unwrap()]).await;
    assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 1);

    STRATEGY_MODE.store(2, Ordering::SeqCst);
    HANDLER_CALLS.store(0, Ordering::SeqCst);
    let response = request(&router, [format!("Bearer {access}").parse().unwrap()]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(response.headers().get(WWW_AUTHENTICATE).is_none());
    assert_headers_do_not_contain(&response, "strategy-sensitive-source-sentinel");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        body.as_ref(),
        b"{\"error\":{\"code\":\"internal\",\"message\":\"internal server error\"}}"
    );
    assert!(
        !std::str::from_utf8(&body)
            .unwrap()
            .contains("strategy-sensitive-source-sentinel")
    );
    assert_eq!(HANDLER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn passport_rejections_retain_internal_sources_without_leaking_them_through_formatting() {
    const SENTINEL: &str = "passport-internal-source-sentinel";

    let rejection =
        PassportRejection::from(PassportError::internal(std::io::Error::other(SENTINEL)));
    let display = rejection.to_string();
    let debug = format!("{rejection:?}");

    assert_eq!(display, "Passport operation failed");
    assert!(!display.contains(SENTINEL));
    assert!(!debug.contains(SENTINEL));

    let passport_error = std::error::Error::source(&rejection).unwrap();
    assert_eq!(passport_error.to_string(), "Passport operation failed");
    assert!(!format!("{passport_error:?}").contains(SENTINEL));
    assert_eq!(
        std::error::Error::source(passport_error)
            .unwrap()
            .to_string(),
        SENTINEL
    );
}
