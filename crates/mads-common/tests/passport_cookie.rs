//! Cookie Passport guard request-time enforcement.

#![cfg(all(feature = "http", feature = "jwt", feature = "cookies"))]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::{
    body::{Body, to_bytes},
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, COOKIE, WWW_AUTHENTICATE},
    },
};
use mads_common::{
    Authenticated, JwtClaims, JwtService, JwtSignOptions, JwtTokenKind, PassportContext,
    PassportError, PassportPrincipal, PassportResult, PassportStrategy, build_router, controller,
    core::{Config, ConfigBuilder, Mads, MapSource},
    passport_strategy, routes,
};
use tower::ServiceExt;

static HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static STRATEGY_CALLS: AtomicUsize = AtomicUsize::new(0);
static REJECT_REFRESH: AtomicBool = AtomicBool::new(false);
static CONTEXT_WAS_SANITIZED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct RefreshClaims {
    user_id: u64,
}

struct RefreshPrincipal {
    user_id: u64,
}

impl PassportPrincipal for RefreshPrincipal {
    fn has_role(&self, _role: &str) -> bool {
        true
    }

    fn has_permission(&self, _permission: &str) -> bool {
        true
    }
}

#[mads_core::service]
struct RefreshStrategy;

#[passport_strategy(name = "jwt-refresh")]
impl PassportStrategy for RefreshStrategy {
    type Claims = RefreshClaims;
    type Principal = RefreshPrincipal;

    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Refresh;

    async fn validate(
        &self,
        context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        STRATEGY_CALLS.fetch_add(1, Ordering::SeqCst);
        let cookies = context
            .cookies()
            .expect("cookie authentication supplies a parsed cookie view");
        CONTEXT_WAS_SANITIZED.store(
            context.headers().get(AUTHORIZATION).is_none()
                && context.headers().get(COOKIE).is_none()
                && cookies.get("refresh_token").is_none()
                && cookies.get("csrf").map(|cookie| cookie.value()) == Some("checked"),
            Ordering::SeqCst,
        );
        if REJECT_REFRESH.load(Ordering::SeqCst) {
            return Err(PassportError::reject());
        }
        Ok(RefreshPrincipal {
            user_id: claims.custom.user_id,
        })
    }
}

#[routes(prefix = "/session")]
#[mads_common::guard(
    strategy = "jwt-refresh",
    principal = RefreshPrincipal,
    source = cookie("refresh_token"),
)]
trait SessionRoutes {
    #[mads_common::post("/refresh")]
    async fn refresh(&self, principal: Authenticated<RefreshPrincipal>) -> String;
}

#[controller(routes = [SessionRoutes])]
struct SessionController;

impl SessionRoutes for SessionController {
    async fn refresh(&self, principal: Authenticated<RefreshPrincipal>) -> String {
        HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        format!("refreshed:{}", principal.user_id)
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

async fn request(
    router: &axum::Router,
    cookies: impl IntoIterator<Item = axum::http::HeaderValue>,
    authorization: Option<String>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method("POST")
        .uri("/session/refresh")
        .body(Body::empty())
        .unwrap();
    for cookie in cookies {
        request.headers_mut().append(COOKIE, cookie);
    }
    if let Some(authorization) = authorization {
        request
            .headers_mut()
            .insert(AUTHORIZATION, authorization.parse().unwrap());
    }
    router.clone().oneshot(request).await.unwrap()
}

async fn assert_generic_authentication_failure(response: axum::response::Response) {
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

#[tokio::test]
async fn cookie_guards_require_exactly_one_valid_refresh_cookie_without_bearer_fallback() {
    let application = Mads::builder_with_config(config()).build().await.unwrap();
    let jwt = application.context().resolve::<JwtService>().unwrap();
    let refresh = jwt
        .sign(
            RefreshClaims { user_id: 7 },
            JwtSignOptions::refresh(Duration::from_secs(60)),
        )
        .unwrap();
    let access = jwt
        .sign(
            RefreshClaims { user_id: 7 },
            JwtSignOptions::access(Duration::from_secs(60)),
        )
        .unwrap();
    let router = build_router(&application).unwrap();

    HANDLER_CALLS.store(0, Ordering::SeqCst);
    STRATEGY_CALLS.store(0, Ordering::SeqCst);
    REJECT_REFRESH.store(false, Ordering::SeqCst);
    CONTEXT_WAS_SANITIZED.store(false, Ordering::SeqCst);
    let response = request(
        &router,
        [format!("csrf=checked; refresh_token={refresh}")
            .parse()
            .unwrap()],
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"refreshed:7"
    );
    assert_eq!(HANDLER_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 1);
    assert!(CONTEXT_WAS_SANITIZED.load(Ordering::SeqCst));

    for cookies in [
        Vec::new(),
        vec!["malformed".parse().unwrap()],
        vec![
            format!("refresh_token={refresh}; refresh_token={refresh}")
                .parse()
                .unwrap(),
        ],
        vec![
            format!("refresh_token={refresh}").parse().unwrap(),
            format!("refresh_token={refresh}").parse().unwrap(),
        ],
        vec![format!("refresh_token={access}").parse().unwrap()],
    ] {
        HANDLER_CALLS.store(0, Ordering::SeqCst);
        STRATEGY_CALLS.store(0, Ordering::SeqCst);
        assert_generic_authentication_failure(
            request(&router, cookies, Some(format!("Bearer {refresh}"))).await,
        )
        .await;
        assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 0);
    }

    REJECT_REFRESH.store(true, Ordering::SeqCst);
    HANDLER_CALLS.store(0, Ordering::SeqCst);
    STRATEGY_CALLS.store(0, Ordering::SeqCst);
    assert_generic_authentication_failure(
        request(
            &router,
            [format!("refresh_token={refresh}").parse().unwrap()],
            None,
        )
        .await,
    )
    .await;
    assert_eq!(STRATEGY_CALLS.load(Ordering::SeqCst), 1);
}
