# Passport/JWT example

This example shows the current v0.8 shape: explicit configuration, access
and refresh signing, managed strategies, typed principals, inherited/method
guard policies, validated login input, normalized rejection responses, cookie
response composition, and the native Axum escape hatch. Application identity
lookup and refresh persistence are intentionally shown as application services.

## Configure

```toml
# Cargo.toml: HTTP + Passport + cookies, without the default database feature.
[dependencies]
mads = { version = "=0.8.0", default-features = false,
  features = ["http", "jwt", "cookies", "runtime-tokio"] }
serde = { version = "1", features = ["derive"] }
```

```toml
# mads.toml
[passport]
secret = "${JWT_SECRET}"
algorithms = ["HS256"]
issuer = "https://auth.example.com"
audiences = ["mads-api"]
```

```rust,ignore
let config = ConfigBuilder::new()
    .dotenv(DotenvSource::optional(".env"))
    .source(TomlSource::file("mads.toml"))
    .source(EnvSource::new("MADS_"))
    .build()?;
let application = Mads::builder_with_config(config).build().await?;
```

The last ordinary source wins. Dotenv is an interpolation map, process values
override dotenv values during interpolation, and environment overrides are
scalar-only.

## Claims, principal, and managed strategies

```rust,ignore
use std::collections::BTreeSet;
use mads::prelude::*;

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct UserClaims { user_id: u64 }

#[derive(PassportPrincipal)]
struct UserPrincipal {
    user_id: u64,
    #[roles]
    roles: Vec<String>,
    #[permissions]
    permissions: BTreeSet<String>,
}

#[service]
struct UserService;

#[service]
struct AppJwtStrategy { users: UserService }

#[passport_strategy(name = "jwt")]
impl PassportStrategy for AppJwtStrategy {
    type Claims = UserClaims;
    type Principal = UserPrincipal;
    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;

    async fn validate(
        &self,
        context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        self.users.current_user(context, claims.custom.user_id).await
    }
}

#[service]
struct RefreshStrategy { users: UserService }

#[passport_strategy(name = "jwt-refresh")]
impl PassportStrategy for RefreshStrategy {
    type Claims = UserClaims;
    type Principal = UserPrincipal;
    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Refresh;

    async fn validate(
        &self,
        context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        // Application code validates its persisted refresh/session record here.
        self.users.valid_refresh(context, claims.custom.user_id).await
    }
}
```

Both concrete strategy types are managed providers. MADS verifies each JWT
before calling `validate`. `jwt-refresh` and all refresh persistence/rotation/
revocation are application-defined.

## Sign and set a cookie

```rust,ignore
use std::time::Duration;

let access = jwt.sign(
    UserClaims { user_id },
    JwtSignOptions::access(Duration::from_secs(900))
        .subject(user_id.to_string()),
)?;
let refresh = jwt.sign(
    UserClaims { user_id },
    JwtSignOptions::refresh(Duration::from_secs(604_800))
        .subject(user_id.to_string())
        .jwt_id(refresh_id),
)?;

jwt.verify::<UserClaims>(&access, JwtValidation::access())?;
jwt.verify::<UserClaims>(&refresh, JwtValidation::refresh().require_jwt_id())?;

let refresh_cookie = Cookie::build(("refresh_token", refresh))
    .path("/")
    .http_only(true)
    .secure(true)
    .same_site(SameSite::Strict)
    .max_age(cookie::time::Duration::days(7))
    .build();
let response = (jar.add(refresh_cookie), Json(LoginResponse { access }));
```

Cookie transport is not CSRF protection. Choose deployment-appropriate cookie
attributes and install an application CSRF control for vulnerable cross-site
state-changing requests.

## Protect route contracts

```rust,ignore
fn owns_profile(principal: &UserPrincipal) -> bool {
    principal.user_id != 0
}

#[derive(serde::Deserialize, Input)]
struct LoginRequest {
    #[validate(email)]
    email: String,
    #[validate(length(min = 1))]
    password: String,
}

#[routes(prefix = "/users")]
#[guard(
    strategy = "jwt",
    principal = UserPrincipal,
    source = bearer,
    roles(any = ["user", "admin"]),
)]
trait UserRoutes {
    #[get("/profile")]
    #[guard(
        permissions(all = ["profile:read"]),
        predicate = owns_profile,
    )]
    async fn profile(
        &self,
        principal: Authenticated<UserPrincipal>,
        token: VerifiedToken<UserClaims>,
    ) -> HttpResult<Json<Profile>>;

    #[post("/refresh")]
    #[guard(
        strategy = "jwt-refresh",
        source = cookie("refresh_token"),
        permissions(all = ["session:refresh"]),
    )]
    async fn refresh(
        &self,
        principal: Authenticated<UserPrincipal>,
    ) -> HttpResult<Json<LoginResponse>>;

    #[post("/login")]
    #[guard(skip)]
    async fn login(
        &self,
        request: ValidatedJson<LoginRequest>,
    ) -> HttpResult<Json<LoginResponse>>;
}
```

The method refresh policy replaces strategy, source, and permissions while
inheriting principal and roles. The login method removes the inherited guard.
Roles, permissions, and predicates are ANDed. Each guard reads one source only.
`ValidatedJson<LoginRequest>` uses Serde followed by `Input` and rejects an
invalid body with ordered, source-aware 422 issues before login code runs. A
native `Json<LoginRequest>` is still available as the deliberate no-automatic-
validation escape hatch.

MADS-owned Passport responses use the v0.8 standard JSON envelope. Strategy
rejection returns 401 `unauthorized` with message `authentication was rejected`
and keeps `WWW-Authenticate: Bearer`; policy denial returns 403 `forbidden`
with message `access was denied`. Internal strategy failures return the fixed,
redacted 500 response. Cookie parsing failures likewise use the standard safe
envelope and never expose cookie contents.

## Native Axum route

```rust,ignore
let guard = PassportGuard::<UserPrincipal>::builder(application.context().clone())
    .strategy("jwt")
    .source(TokenSource::Bearer)
    .roles_any(["user"])
    .permissions_all(["profile:read"])
    .predicate(owns_profile)
    .build()?;

let native = mads::axum::Router::new()
    .route("/native/profile", mads::axum::routing::get(native_profile))
    .route_layer(guard);
```

Native guards are runtime-only and cannot statically activate the JWT default.
Ensure a managed provider directly depends on `JwtService`, or explicitly
provide a concrete `JwtService` before building the application. Missing that
service causes `MADS131`.

MADS.rs 0.8 does not provide application login behavior, refresh endpoints,
refresh persistence or rotation/revocation, password hashing, CSRF, remote
JWKS, or JWE. Those remain application authentication and deployment policy.
Root module scope, CORS, and automatic HTTP binding are already part of the
current runtime.
