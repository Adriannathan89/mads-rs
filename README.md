# MADS.rs

MADS.rs 0.8.0 is a Rust application framework with a framework-neutral
core, a scoped Axum HTTP runtime, source-aware typed configuration, safe REST
errors, request validation, and explicit PostgreSQL/Diesel integration. A root
module selects one application; startup validates its scoped graph and routes
before it starts lifecycle hooks, checks a database, or binds a socket.

## CLI quick start

Create a minimal HTTP application, then start its development server:

```bash
mads new my-app
cd my-app
mads dev
```

`mads new` creates exactly `Cargo.toml`, `mads.toml`, `src/main.rs`, and
`src/app/{mod,routes,controller,service}.rs`. The generated application has
only the `http` and `runtime-tokio` MADS features—no database, JWT, cookie,
migration, or authentication setup—and answers `GET /` with `Hello World!`.
The application package starts at `0.1.0`; its MADS dependency is pinned to the
installed CLI version. See [the CLI reference](docs/CLI.md) for its atomic,
offline generator contract, naming rules, exact JSON output, and non-goals.

From an existing project, inspect or run a selected application:

```bash
mads doctor
mads routes
mads run
```

See the [authoritative CLI reference](docs/CLI.md) for target selectors,
forwarded application arguments, diagnostics, watcher behavior, inspection
limits, and database commands.

## Standard application

```rust,no_run
use mads::prelude::*;

mod user {
    use mads::prelude::*;

    #[module]
    pub struct UserHttpModule;
}
use user::UserHttpModule;

#[module(imports = [UserHttpModule])]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
```

`Mads::run` is the recommended application entry point. The root module
selects its direct imports and the providers, controllers, routes, guards,
strategies, and official auto-configurations reachable through that graph.

## Crates and boundaries

- `mads-core` owns construction, providers, lifecycle, diagnostics, and
  generic scalar TOML/dotenv configuration, plus official conditional-default
  evaluation and redacted inspection reports. It has no database or HTTP
  dependency.
- `mads-common` owns route validation, Axum delivery, cookies, JWT/Passport,
  the official Diesel default, PostgreSQL pools, database infrastructure
  lifecycle, and migration execution.
- `mads` is the stable facade. Its default `common` feature exposes the HTTP
  runtime and persistence; disable default features for the core-only boundary.

```toml
[dependencies]
mads = "0.8.0"
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
```

MADS.rs supports Rust 1.85 and uses Rust edition 2024.

The default `common` feature remains the compatibility aggregate for HTTP and
PostgreSQL; it does not silently enable authentication. For an HTTP API without
Diesel, use `default-features = false` with `features = ["http",
"runtime-tokio"]`. Add `jwt` and `cookies` only when the application needs
Passport/cookie support. `jwt` alone does not pull in Axum, while `cookies`
includes HTTP. Passport strategies and Bearer guards require `http + jwt`;
cookie guards additionally require `cookies`. Input validation and the REST
error family are `http` APIs; `.into_http()` requires both `http` and
`database`.

## Conventional configuration and HTTP

Only `Mads::run` loads conventional configuration. It reads the process
current working directory in this order:

1. optional `.env`, used only for interpolation;
2. optional `mads.toml` as ordinary configuration;
3. final scalar `MADS_*` environment overrides.

Process variables win during `${NAME}` interpolation, dotenv loading never
mutates the process environment, `MADS_SERVER__HOST` maps to `server.host`, and
`MADS_SERVER__PORT` maps to `server.port`. Both files may be absent; a present
unreadable or malformed file is a bootstrap failure. MADS does not search
parent directories or `CARGO_MANIFEST_DIR`.

```toml
# mads.toml
[server]
host = "127.0.0.1" # default
port = 3000        # default

[server.cors]
origins = ["https://app.example.com"]
methods = ["GET", "POST"]
allowed_headers = ["authorization", "content-type"]
exposed_headers = ["x-request-id"]
credentials = false
max_age_seconds = 600
```

Wildcard-capable CORS fields use a scalar, not a one-element list:

```toml
[server.cors]
origins = "*"
methods = ["GET", "POST"]
allowed_headers = "*"
exposed_headers = "*"
```

CORS is opt-in, validated before lifecycle startup, and applied as the
outermost layer to both generated and native routes. Wildcard origins or
headers cannot be combined with credentials. It is a browser response-access
policy, not authorization or CSRF protection.

Use the tracked [`.env.example`](.env.example) as a local template, copy it to
the ignored `.env`, and put real secrets in process variables in CI and
production.

## Validated requests and REST errors

Use `#[derive(serde::Deserialize, Input)]` with `ValidatedJson<T>`,
`ValidatedQuery<T>`, or `ValidatedPath<T>` to deserialize, validate, attach a
`body`, `query`, or `path` source, and invoke a handler only on valid input.

```rust,no_run
use mads::prelude::*;

#[derive(serde::Deserialize, Input)]
struct CreateUser {
    #[validate(email, length(max = 254))]
    email: String,
    #[validate(length(min = 8))]
    password: String,
}

#[routes(prefix = "/users")]
trait UserRoutes {
    #[post("/")]
    async fn create(&self, body: ValidatedJson<CreateUser>) -> HttpResult<Json<User>>;
}
# struct User;
```

The built-ins are `email`, `length(min = N)`, `length(max = N)`,
`length(exact = N)`, `nonempty`, `range(min = N)`, `range(max = N)`,
`positive`, `negative`, `multiple_of = N`, `required`, `nested`, and
`custom = path`. They cover supported strings, numbers, `Option`, structs,
enums, tuples, arrays, `Vec`, and string-keyed `HashMap`/`BTreeMap` shapes;
string length is Unicode code-point length. Derived callbacks can report one or
many relative `ValidationIssue`s, and applications may implement `Input`
manually for complete control. Validators run in source order; nested values
follow declaration, index, and lexical map-key order.

Email follows the practical default Zod syntax policy on the unmodified
string; MADS does not trim, normalize, perform DNS checks, or claim full-RFC
mailbox validation. Numeric bounds are inclusive, while `positive` and
`negative` are strict.

```rust,ignore
fn validate_username(value: &str) -> ValidationResult {
    if value == "root" {
        Err(ValidationErrors::from_issue(
            ValidationIssue::custom("reserved_username", "username is reserved"),
        ))
    } else {
        Ok(())
    }
}

// Use #[validate(custom = validate_username)] on a field, a whole-value
// callback on the derived type, or implement Input manually for full control.
```

Custom issue paths are relative; derive-generated nesting prefixes external
Serde field/variant names or collection indices. Body/query/path source is
attached only by the validated extractor, so transport-independent manual and
derived implementations share the same HTTP boundary.

Validation returns status 422 with the fixed safe envelope:

```json
{
  "error": {
    "code": "validation_error",
    "message": "input validation failed",
    "issues": [{
      "source": "body",
      "path": ["email"],
      "code": "invalid_format",
      "message": "invalid email address"
    }]
  }
}
```

Serde conversion remains authoritative and can report its first conversion
issue; after successful deserialization MADS aggregates independent validation
issues. Rejected values never appear in built-in issues. Validated JSON keeps
415 unsupported-media-type and 413 body-limit semantics in the standard error
envelope.

Native `Json<T>`, `Query<T>`, and `Path<T>` remain the unmodified Axum
extractors and deliberately do not run `Input`. Use them when an application
needs its own extraction or validation policy; do not rename a native `Json`
alias and present it as validation.

The `http` feature exports the seven standard errors: `BadRequest` (400),
`Unauthorized` (401), `Forbidden` (403), `NotFound` (404), `Conflict` (409),
`ValidationError` (422), and `InternalError` (500). They use one
`{ "error": { "code", "message" } }` envelope; validation alone adds ordered
issues. Internal errors always render `internal server error` and retain their
source only for server-side error chaining. MADS-owned Passport and cookie
failures use this envelope; Passport authentication rejection retains
`WWW-Authenticate: Bearer`. Native Axum responses remain native.

| Type | Status | Code | Message policy |
| --- | ---: | --- | --- |
| `BadRequest` | 400 | `bad_request` | application-supplied safe message |
| `Unauthorized` | 401 | `unauthorized` | application-supplied safe message |
| `Forbidden` | 403 | `forbidden` | application-supplied safe message |
| `NotFound` | 404 | `not_found` | application-supplied safe message |
| `Conflict` | 409 | `conflict` | application-supplied safe message |
| `ValidationError` | 422 | `validation_error` | fixed `input validation failed` |
| `InternalError` | 500 | `internal` | fixed `internal server error` |

MADS fixes its own Passport messages to `authentication was rejected` or
`access was denied`, malformed cookies to `cookie request is malformed`,
unsupported validated JSON content types to
`content type must be application/json`, payload overflow to
`request body is too large`, other safe client body-read failures to
`request body could not be read`, and all server-class failures to
`internal server error`.

When both `http` and `database` are selected, conversion is still a deliberate
delivery-policy decision:

```rust,ignore
let user = database.run(move |connection| query.first(connection)).await.into_http()?;
```

`.into_http()` maps only typed Diesel not-found to 404 and typed unique
violations to 409; every other database error is a redacted 500. There is no
automatic `From<DatabaseError>` mapping, so applications can keep native
Diesel results or use a domain-specific `map_err` policy.

## Typed configuration and secrets

Typed configuration reads the existing loaded `Config`; it does not introduce a
new loader or global type discovery. Derive a named configuration struct and
request it explicitly through `Config::parse`:

```rust,no_run
use mads::prelude::*;

#[derive(Configuration)]
#[config(prefix = "app")]
struct AppConfig {
    #[config(rename = "bind_host")]
    host: String,
    #[config(default = 3000, validate(range(min = 1, max = 65535)))]
    port: u16,
    api_key: Secret<String>,
}

#[provider]
fn app_config(config: Config) -> mads::core::Result<AppConfig> {
    Ok(config.parse()?)
}
```

Supported fields are strings, booleans, characters, finite numeric primitives,
source-relative `PathBuf`, `Option<T>`, `Secret<T>`, `Option<Secret<T>>`,
`Vec<String>`, nested `Configuration`, and a scalar `parse_with` callback.
Prefixes and `rename` compose dotted keys; defaults and compatible validators
are checked at compile time. Missing, parse, and validation failures aggregate
in declaration order with full keys, stable codes, and winning-source labels,
never configured values.

A `parse_with` callback receives `&str` and returns `Result<FieldType, E>`;
arbitrary parser error text is discarded so it cannot leak an input. Options
become `None` only when absent, present invalid values never fall back to a
default, and secrets cannot have source-code defaults. Maps, non-string
vectors, arbitrary arrays, inline tables, arrays of tables, and TOML datetimes
remain outside the existing flattened `Config` shape.

The provider makes parsing a startup requirement only when the selected graph
uses it: failure occurs before lifecycle startup and listener binding. The
conventional source order is unchanged: optional `.env` for interpolation,
optional `mads.toml`, then final scalar `MADS_*` overrides. Only an entire
`${NAME}` scalar/array element is interpolated; process variables win over
dotenv, and dotenv is not a configuration source. `Secret<T>` exposes a value
only through `.expose()` or `.into_exposed()`; ordinary `Display` and `Debug`
always print `[REDACTED]`.

## Low-level builder

Use the builder when configuration, migrations, hooks, binding, or router
composition must be explicit. It never loads `.env`, `mads.toml`, or `MADS_*`
on its own. The explicit address overrides `[server]` binding and may use port
zero; merge native Axum routes before passing the raw router to `serve_router`.

```rust,ignore
let mut builder = Mads::builder_with_config(config);
builder.root::<AppModule>()?;
builder.database_migrations(MIGRATIONS)?;
// builder.lifecycle_hook(MyHook);
let application = builder.build().await?;
let router = build_router(&application)?.merge(native_router);
serve_router(application, router, "127.0.0.1:0").await?;
```

For direct in-process router use, call `configure_router(&application, router)`
after the merge. A builder without `root::<AppModule>()` intentionally retains
the complete-catalog compatibility behavior.

## Database provisioning

This is zero **database** bootstrap, not zero application configuration. A
provider in the selected application scope that directly requires `Database`
activates the linked default only after configuration and virtual graph
validation. `database_migrations` separately registers one embedded source; it
does not create a pool, connect, or run migrations. It is required only when
`database.migrate = true`; existing pending embedded migrations then run after
readiness, and no pending migrations are a successful no-op. Normal startup
never generates or auto-applies migrations. The explicit `mads db generate`
command can create one review-required schema-diff migration from `src/schema.rs`
or recursively loaded `src/schema/**/*.rs`; inspect the generated `up.sql` and
`down.sql` before applying it with `mads db migrate`.

Inspect the retained, redacted decision records without exposing configuration
values:

```rust,ignore
for report in application.auto_configurations() {
    println!(
        "{} {:?} {}",
        report.identifier(),
        report.status(),
        report.reason_code().as_str(),
    );
}
```

`DatabaseBootstrap` remains the explicit native Diesel override. It backs off
the conditional default completely and contributes its database lifecycle as
framework infrastructure. An application-provided `Database` instead owns its
complete readiness, migration, and shutdown lifecycle:

```rust,ignore
use mads::{core::{ConfigBuilder, MapSource}, prelude::*};

let config = ConfigBuilder::new()
    .source(MapSource::new(
        "application",
        [("database.url", "postgres://localhost/mads")],
    ))
    .build()?;
let database = DatabaseConfig::from_config(&config)?;
let mut builder = Mads::builder_with_config(config);
builder.database(DatabaseBootstrap::new(database))?;
let application = builder.build().await?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use the direct `mads::diesel` and
`mads::diesel_migrations` re-exports when native Diesel APIs are the right
tool.

`serve(application, "127.0.0.1:3000")` remains the explicit generated-router
escape hatch. Its address overrides automatic server binding; use
`serve_router` when the raw generated router has been merged with native Axum
routes.

## Passport configuration and JWT profiles

`Mads::run` supplies the standard conventional source order; the low-level
builder stays explicit. Dotenv sources provide interpolation values, and
ordinary sources merge from first to last; a later scalar or string array
replaces an earlier value at the same key completely. Process variables override
dotenv values during `${NAME}` interpolation. `EnvSource` is scalar-only, so
arrays such as `algorithms` and `audiences` belong in TOML or a programmatic
`ConfigDocument`/`MapSource`.

```toml
# mads.toml
[passport]
secret = "${JWT_SECRET}"
algorithms = ["HS256"]
issuer = "https://auth.example.com"
audiences = ["mads-api"]
```

Simple `secret` mode permits one HMAC algorithm: HS256 by default, or one of
HS384/HS512 when explicitly selected. Minimum secret sizes are 32/48/64 bytes.
For rotation, configure a named key ring; the active key signs and all retained
keys verify by `kid`:

```toml
[passport]
active_key = "2026-08"
algorithms = ["RS256"]

[passport.keys."2026-08"]
algorithm = "RS256"
private_key_file = "keys/current-private.pem"
public_key_file = "keys/current-public.pem"

[passport.keys."2026-07"]
algorithm = "RS256"
public_key_file = "keys/previous-public.pem"
```

MADS supports HS256/384/512, RS256/384/512, and ES256/384. The configured
allowlist—not an untrusted token header—selects eligible algorithms, and every
named key is bound to one algorithm. Relative paths from TOML resolve beside
that TOML file; paths from environment or programmatic sources resolve from the
process working directory.

```rust,ignore
use std::time::Duration;
use mads::prelude::*;

let access = jwt.sign(
    UserClaims { user_id: 7 },
    JwtSignOptions::access(Duration::from_secs(900)).subject("7"),
)?;
let refresh = jwt.sign(
    UserClaims { user_id: 7 },
    JwtSignOptions::refresh(Duration::from_secs(604_800)).subject("7"),
)?;
let verified_access = jwt.verify::<UserClaims>(&access, JwtValidation::access())?;
let verified_refresh = jwt.verify::<UserClaims>(&refresh, JwtValidation::refresh())?;
```

Access and refresh tokens have different protected `typ` values and
`token_use` claims. They are not interchangeable. Unverified decode APIs are
inspection-only and must never authenticate a request.

## Managed strategies, principals, and guards

A custom strategy is both a managed provider and an annotated
`PassportStrategy` implementation. Framework signature, registered-claim, and
token-kind verification always happens before `validate`; the strategy sees
verified claims and a credential-sanitized, read-only `PassportContext`.

```rust,ignore
#[derive(PassportPrincipal)]
struct UserPrincipal {
    user_id: u64,
    #[roles]
    roles: Vec<String>,
    #[permissions]
    permissions: std::collections::BTreeSet<String>,
}

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
        self.users.authenticate_current(context, claims.custom.user_id).await
    }
}
```

`jwt` is the built-in access strategy and can authorize directly as
`ClaimsPrincipal<C>`. A custom `jwt` strategy overrides it. `jwt-refresh` is not
built in: applications define it with `JwtTokenKind::Refresh` and own any
persistence, rotation, reuse detection, and revocation.

```rust,ignore
fn owns_profile(principal: &UserPrincipal) -> bool { principal.user_id == 7 }

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

    #[post("/login")]
    #[guard(skip)]
    async fn login(&self) -> HttpResult<Json<LoginResponse>>;
}
```

Trait policies inherit. A method replaces only fields it supplies;
`#[guard(skip)]` is the sole opt-out. Roles, permissions, and predicates are
ANDed; `any`/`all` controls matching inside one role or permission clause, and
every predicate must be a synchronous `fn(&UserPrincipal) -> bool`. A guard
uses exactly one source. With `cookies`, select
`source = cookie("refresh_token")`; there is no Bearer fallback.

Authentication and strategy rejection map to generic `401 Unauthorized` with
`WWW-Authenticate: Bearer`, authorization policy failures to `403 Forbidden`,
and operational failures to `500 Internal Server Error`. Ordinary malformed
cookie extraction remains `400 Bad Request`; a missing, malformed, or duplicate
guard cookie is a generic `401`.

Cookie jars compose with response tuples and emit checked `Set-Cookie` headers:

```rust,ignore
let cookie = Cookie::build(("refresh_token", refresh))
    .path("/")
    .http_only(true)
    .secure(true)
    .same_site(SameSite::Strict)
    .max_age(cookie::time::Duration::days(7))
    .build();
Ok((jar.add(cookie), Json(response)))
```

For native Axum routes, apply a typed `PassportGuard<P>` Tower layer. This is a
runtime escape hatch, not static MADS guard metadata, so it cannot activate JWT
auto-configuration. Before `PassportGuard::build()`, a managed provider must
directly require `JwtService`, or the builder must explicitly provide a
concrete `JwtService`; otherwise construction fails with `MADS131`.

See the complete [Passport/JWT example](docs/examples/passport_jwt.md) and the
[v0.5.5 security and release notes](docs/importance/version_0.5.5/passport-jwt-and-cookies.md).

## CLI migrations

From a project root containing `mads.toml` and `migrations/`:

```text
mads db migrate   # apply pending file migrations
mads db rollback  # revert the latest migration from this source
mads db status    # report applied and pending versions
mads db generate  # create one automatically named, review-required diff
```

`mads db generate` never applies its output and has no positional name. It
loads split Diesel schema files recursively and warns when a change needs
manual SQL review. `mads db migrate` prints `applied <version>` for work
performed or `database is up to date`; `rollback` prints `reverted <version>`;
`status` prints individual versions plus an applied/pending summary. Invalid
command syntax exits with 2; configuration, pool, or migration failures exit
with 1.

## A typed HTTP route

`#[mads::routes]` records immutable metadata and emits a typed registration
adapter. `#[mads::controller]` resolves the managed controller once while the
router is built; handlers do not receive manual `State<AppState>` or perform
per-request provider resolution.

```rust,no_run
use mads::prelude::*;

#[derive(Clone, serde::Serialize)]
struct User {
    id: u64,
}

#[mads::routes(prefix = "/readme-users")]
trait UserRoutes {
    #[mads::get("/:id")]
    async fn get_user(&self, id: Path<u64>) -> HttpResult<Json<User>>;
}

#[mads::controller(routes = [UserRoutes])]
struct UserController;

impl UserRoutes for UserController {
    async fn get_user(&self, Path(id): Path<u64>) -> HttpResult<Json<User>> {
        Ok(Json(User { id }))
    }
}

#[module]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
```

## Extractors, responses, and routing

The prelude exports `Path<T>`, `Query<T>`, `Json<T>`, `Header<T>`, `Request`,
`HttpResult<T>`, `Created<T>`, `NoContent`, `build_router`, `configure_router`,
`serve`, and `serve_router`.
`mads::common::axum` remains the native Axum escape hatch for extractors,
responses, routers, middleware, and Tower composition.

MADS route metadata uses `/:parameter`; the validated adapter translates it to
Axum 0.8 syntax only while registering the route. Invalid metadata and
conflicts fail with `MADS030` before router construction. GET also handles
HEAD, OPTIONS is not synthesized, static routes win over parameter routes, and
trailing slashes remain strict. `build_router(&application)` returns the raw
generated router; merge native routes before `configure_router` or
`serve_router` applies final application-wide CORS. Use the configured router
with Tower's `ServiceExt::oneshot` for in-process route tests without binding a
listener.

## Benchmarks

The current benchmark suite covers native Axum/MADS throughput and
process-start-to-ready comparisons with Axum, Go/Gin, and NestJS/Fastify.

| Application | Startup P50 | Startup P95 |
| --- | ---: | ---: |
| Native Axum | 21 ms | 30 ms |
| Go/Gin | 22 ms | 29 ms |
| MADS | 22 ms | 30 ms |
| NestJS/Fastify | 428 ms | 443 ms |

The startup comparison uses 1,000 release-build starts per application and an
equivalent PostgreSQL readiness check. In the exploratory throughput suite,
every native Axum/MADS saturation range overlaps, while both sustain the fixed
1,000 requests/second target with closely grouped latency.

See [BENCHMARK.md](BENCHMARK.md) for the complete results, methodology,
limitations, resource measurements, and interpretation guidance.

## Current scope

Version 0.8.0 includes rooted module scope, conventional startup, CORS,
native router composition, typed input validation, the seven REST errors,
explicit typed configuration and redacted secrets, focused MADS macro
diagnostics, Cargo-native run/dev, compiled route/graph/doctor inspection,
version-1 finite-command JSON, bounded PostgreSQL migration work, and the
offline atomic minimal-project generator. It preserves the low-level builder,
the complete-catalog rootless compatibility path, native Axum extractors and
responses, ordinary human CLI output, and application-owned database policy.

It does **not** implement trait or interface bindings, `Inject<dyn Trait>`,
asynchronous or database-backed derive validators, automatic validation for
native extractors, full-RFC/DNS email validation, login or credential
validation, refresh endpoints or persistence/rotation/revocation, password
hashing, CSRF, remote JWKS, JWE, third-party auto-configuration, arbitrary
configuration sources/shapes, multiple-listener/TLS/HTTP2 server configuration,
JSON-wrapped run/dev streams, or scaffold database/JWT/cookie/migration/Git
setup. Database errors never map automatically: applications opt in with
`.into_http()` or retain a custom/native delivery policy.

## Development

Run the available release checks locally:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --all-features --doc
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo +1.85.0 test --locked --workspace --all-features
```

CI also provisions PostgreSQL 16 and runs the ignored database suites plus the
85% line-coverage gate. To run those locally, set `MADS_TEST_DATABASE_URL` to a
PostgreSQL 16 database and use the commands in the [v0.5 requirements](docs/importance/version_0.5/auto-configuration.md).

## License

MADS.rs is licensed under either the [Apache License 2.0](LICENSE-APACHE) or
the [MIT License](LICENSE-MIT), at your option.
