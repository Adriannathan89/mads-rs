//! Bearer Passport guard request-time enforcement.

#![cfg(all(feature = "http", feature = "jwt"))]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::{
    body::{Body, to_bytes},
    extract::connect_info::ConnectInfo,
    http::{
        Method, Request, StatusCode,
        header::{AUTHORIZATION, WWW_AUTHENTICATE},
    },
};
use mads_common::{
    Authenticated, JwtClaims, JwtService, JwtSignOptions, JwtTokenKind, PassportContext,
    PassportPrincipal, PassportResult, PassportStrategy, VerifiedToken, build_router, controller,
    core::{Config, ConfigBuilder, Mads, MapSource},
    passport_strategy, routes,
};
use tower::ServiceExt;

static HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static STRATEGY_CONTEXT: OnceLock<Mutex<Option<ContextRecord>>> = OnceLock::new();

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct UserClaims {
    user_id: u64,
}

struct UserPrincipal {
    user_id: u64,
}

impl PassportPrincipal for UserPrincipal {
    fn has_role(&self, _role: &str) -> bool {
        false
    }

    fn has_permission(&self, _permission: &str) -> bool {
        false
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ContextRecord {
    method: Method,
    uri: String,
    request_id: String,
    remote_addr: Option<String>,
}

#[mads_core::service]
struct UserStrategy;

#[passport_strategy(name = "jwt")]
impl PassportStrategy for UserStrategy {
    type Claims = UserClaims;
    type Principal = UserPrincipal;

    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;

    async fn validate(
        &self,
        context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        assert!(context.headers().get(AUTHORIZATION).is_none());
        STRATEGY_CONTEXT
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap()
            .replace(ContextRecord {
                method: context.method().clone(),
                uri: context.uri().to_string(),
                request_id: context
                    .headers()
                    .get("x-request-id")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                remote_addr: context.remote_addr().map(|address| address.to_string()),
            });
        Ok(UserPrincipal {
            user_id: claims.custom.user_id,
        })
    }
}

#[routes(prefix = "/users")]
#[mads_common::guard(strategy = "jwt", principal = UserPrincipal)]
trait UserRoutes {
    #[mads_common::get("/profile")]
    async fn profile(
        &self,
        principal: Authenticated<UserPrincipal>,
        token: VerifiedToken<UserClaims>,
    ) -> String;
}

#[controller(routes = [UserRoutes])]
struct UserController;

impl UserRoutes for UserController {
    async fn profile(
        &self,
        principal: Authenticated<UserPrincipal>,
        token: VerifiedToken<UserClaims>,
    ) -> String {
        HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        assert_eq!(principal.user_id, token.claims.custom.user_id);
        format!("user:{}", principal.user_id)
    }
}

fn config() -> Config {
    ConfigBuilder::new()
        .source(MapSource::new(
            "mads.toml",
            [("passport.secret", "01234567890123456789012345678901")],
        ))
        .build()
        .unwrap()
}

#[tokio::test]
async fn guarded_bearer_route_verifies_before_invoking_the_handler() {
    HANDLER_CALLS.store(0, Ordering::SeqCst);
    *STRATEGY_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = None;
    let application = Mads::builder_with_config(config()).build().await.unwrap();
    let token = application
        .context()
        .resolve::<JwtService>()
        .unwrap()
        .sign(
            UserClaims { user_id: 7 },
            JwtSignOptions::access(Duration::from_secs(60)),
        )
        .unwrap();
    let router = build_router(&application).unwrap();
    let mut request = Request::builder()
        .uri("/users/profile")
        .header(AUTHORIZATION, format!("bEaReR {token}"))
        .header("x-request-id", "request-123")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("127.0.0.1:8443".parse::<SocketAddr>().unwrap()));

    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(HANDLER_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"user:7"
    );
    assert_eq!(
        STRATEGY_CONTEXT.get().unwrap().lock().unwrap().take(),
        Some(ContextRecord {
            method: Method::GET,
            uri: "/users/profile".to_owned(),
            request_id: "request-123".to_owned(),
            remote_addr: Some("127.0.0.1:8443".to_owned()),
        })
    );
}

#[tokio::test]
async fn guarded_bearer_route_rejects_missing_credentials_with_json_and_bearer_challenge() {
    HANDLER_CALLS.store(0, Ordering::SeqCst);
    let application = Mads::builder_with_config(config()).build().await.unwrap();
    let router = build_router(&application).unwrap();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/users/profile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

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
