# mads-common

`mads-common` is the integration boundary between the framework-neutral
[mads-core](../mads-core/README.md) and the web/database/authentication
ecosystem. It owns optional Axum, PostgreSQL/Diesel, JWT, cookie, Passport,
validation, REST-error, and CORS behavior.

Application authors normally use the
[`mads` facade](../mads/README.md). Depend on `mads-common` directly when
building or testing an integration boundary, or when a tool needs the private
HTTP inspection contract used by `mads-cli`.

## Feature model

The crate itself has default features `http + database`. The public `mads`
facade disables those defaults and maps its own features explicitly.

| Feature | Adds | Requires |
| --- | --- | --- |
| `http` | Axum 0.8 routing/server delivery, route/controller contracts, extractors, validation, REST errors, CORS, and native Axum/Tower re-exports. | `mads-core` and the HTTP dependency set. |
| `database` | PostgreSQL/Diesel pool, blocking query boundary, migration lifecycle, database configuration, and the official Database auto-configuration. | `mads-core` and Tokio. |
| `jwt` | `JwtService`, claims, validation profiles, algorithms, keyrings, and JWT auto-configuration. | No Axum or database dependency. |
| `cookies` | Strict cookie extraction and checked response-cookie composition. | Implies `http` and enables the cookie/Passport macro support. |

Passport route guards and managed strategies are available when `http + jwt`
are selected. Cookie-backed guards additionally need `cookies`. The
`database` feature does not itself add HTTP error conversion; `IntoHttpResult`
is compiled only for `http + database` and is opt-in at the call site.

## How this crate fits the runtime

`mads-common` receives a constructed core application and a rooted module
scope:

~~~text
mads-core application
        │
        ▼
HttpApplicationScope
        │
        ├── route/controller catalog validation
        ├── guard and Passport preflight
        ├── generated + native Axum router composition
        ├── application-wide CORS configuration
        └── Database/JWT/cookie integration lifecycle
~~~

Route metadata is validated before generated registrars install routes.
Controllers are resolved once while the router is built; request handling uses
the captured application-scoped handles. The standard server path configures
the final router, starts lifecycle hooks, waits for infrastructure readiness,
binds the listener, serves, and then shuts down in reverse order.

The database default is conditional: a selected provider must directly require
`Database`, the database integration must be linked, and valid configuration
must be available. An explicit `DatabaseBootstrap` or application-provided
`Database` backs the default off. Database migrations are a separately
registered embedded source; normal startup never generates migrations.

The JWT service can be used without HTTP. Passport adds a typed strategy and
principal layer on top of verified JWT claims, with guard policy resolved
before requests. Native Axum routes remain available as an escape hatch and do
not silently become MADS-managed routes.

## Public areas

| Area | Main APIs |
| --- | --- |
| Routing | `routes`, `controller`, `get`/`post`/`put`/`patch`/`delete`, `build_router`, `configure_router`, `serve_router` |
| Requests | Native Axum extractors plus `ValidatedJson`, `ValidatedQuery`, and `ValidatedPath` |
| Errors | `BadRequest`, `Unauthorized`, `Forbidden`, `NotFound`, `Conflict`, `ValidationError`, `InternalError` |
| Database | `Database`, `DatabaseConfig`, `DatabaseBootstrap`, `Database::run`, embedded migrations, `IntoHttpResult` |
| Authentication | `JwtService`, `PassportStrategy`, `PassportPrincipal`, `Authenticated`, `PassportGuard` |
| Cookies | `CookieJar` and checked response-cookie composition |
| Configuration | Core `Config`/`Configuration` values consumed by HTTP, database, CORS, and Passport integration |

## Dependencies

First-party dependencies:

- `mads-core` is always present.
- `mads-common-macros` is optional and is enabled by the HTTP/Passport macro
  features that need it.

Important external dependencies are grouped by responsibility:

- HTTP: `axum`, `axum-extra`, `tower`, `tower-http`, `tokio`, and
  platform-specific `rustix`.
- Serialization and extraction: `serde`, `serde_json`,
  `serde_path_to_error`, and `serde_urlencoded`.
- Database: `diesel`, `deadpool-diesel`, and `diesel_migrations`.
- JWT and keys: `jsonwebtoken` and `base64`.
- Cookies: `cookie`.

`mads` depends on this crate for application integrations. `mads-cli` also
depends on it directly with `http` and default features disabled for private
application inspection; its normal database access arrives through the
facade's selected features.

## Source layout

- `src/route.rs` and `src/router.rs` — route metadata, validation, registrar
  dispatch, and generated router construction.
- `src/server.rs`, `src/server_config.rs`, and `src/cors.rs` — standard
  startup, explicit serving, binding, and outer router configuration.
- `src/http_scope.rs` and `src/inspection.rs` — rooted HTTP selection and
  side-effect-free inspection reports.
- `src/extract/` and `src/validation/` — native/validated request extraction and
  ordered input issues.
- `src/response.rs` — safe REST response envelopes and error mapping.
- `src/database/` — PostgreSQL pool, Diesel execution, migrations, auto-config,
  lifecycle, and explicit HTTP mapping.
- `src/jwt/`, `src/passport/`, and `src/cookie.rs` — authentication, guard
  policy, principals, strategies, and cookie handling.
- `src/lib.rs` — feature gates, public exports, and hidden contracts consumed by
  generated code and the CLI.

## Tests and contributor workflow

Run the focused integration tests with:

~~~sh
cargo test -p mads-common --all-features
~~~

Most HTTP, validation, auth, and lifecycle tests can run without external
services. PostgreSQL suites are ignored by default and require PostgreSQL 16
through `MADS_TEST_DATABASE_URL`. Route and macro consumer behavior may also
require the fixtures under `crates/mads/tests/ui`.

See the [architecture reference](../../docs/ARCHITECTURE.md), the
[CLI contract](../../docs/CLI.md), and the
[Passport example](../../docs/examples/passport_jwt.md) before changing a
public integration contract.
