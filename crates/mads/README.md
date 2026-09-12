# mads

`mads` is the stable public facade for MADS.rs applications. It is the crate
most application authors should depend on. The facade composes the
framework-neutral [`mads-core`](../mads-core/README.md) with optional
[mads-common integrations](../mads-common/README.md) and re-exports the
macros, types, native Axum/Diesel APIs, and prelude used by application code.

The facade is intentionally thin: it owns public naming, feature composition,
and re-exports, while graph, HTTP, database, and authentication behavior stays
in the crate that implements it.

## Dependency structure

- `mads-core` is always enabled.
- `mads-common` is optional and is selected with default features disabled.
- `mads-extra` is optional and is selected by the `extra` feature.
- Application crates and `mads-cli` depend on this facade.

The internal dependency shape is:

~~~text
application
└── mads
    ├── mads-core
    │   └── mads-core-macros
    ├── mads-common
    │   ├── mads-core
    │   └── mads-common-macros
    └── mads-extra
        └── mads-core

mads-cli
├── mads
└── mads-common (http-only inspection contract)
~~~

The arrow points from a consumer to its dependency. `mads-common` is
feature-selected by the facade; it is not enabled by default features inside
its own manifest.

## Feature matrix

| Feature | Expands to | Use |
| --- | --- | --- |
| `default` | `common` + `runtime-tokio` | Conventional HTTP + PostgreSQL/Diesel application with the Tokio entry point. |
| `common` | `http` + `database` | Compatibility aggregate; authentication remains opt-in. |
| `http` | `mads-common/http` | Axum routing/server, validation, CORS, and REST errors without Diesel. |
| `database` | `mads-common/database` | PostgreSQL/Diesel infrastructure without HTTP mapping. |
| `jwt` | `mads-common/jwt` | JWT service, claims, profiles, algorithms, and key handling without Axum. |
| `cookies` | `http` + `mads-common/cookies` | Cookie extraction/response support; cookies imply HTTP. |
| `runtime-tokio` | `mads-core/runtime-tokio` | Tokio support for `#[mads::main]`. |
| `extra` | `mads-extra` | Reserved extension boundary. |

Passport guards and strategies require `http + jwt`. Cookie guards add
`cookies`. `.into_http()` requires `http + database` and is an explicit
delivery policy rather than a blanket database error conversion.

For an HTTP-only application, use:

~~~toml
[dependencies]
mads = { version = "0.8.0", default-features = false, features = ["http", "runtime-tokio"] }
~~~

The workspace crates use exact internal version pins. External dependency
versions are maintained in the workspace root `Cargo.toml`.

## Public surface

The facade re-exports:

- Core declarations: `module`, `provider`, `service`, `repository`,
  `Configuration`, `Secret`, `Module`, and the builder/application types under
  `mads::core`.
- Integration declarations: `routes`, HTTP verbs, `controller`, `guard`,
  `Input`, Passport derives, and strategy metadata when the required features
  are enabled.
- HTTP types: native Axum extractors/responses, validated extractors,
  `HttpResult`, standard REST errors, router builders, and serving functions.
- Database types: `Database`, Diesel/Diesel migration re-exports,
  configuration, bootstrap, migration, and explicit HTTP mapping APIs.
- Authentication/cookies: `JwtService`, claims/options, Passport types,
  `CookieJar`, and cookie response composition.
- Native escape hatches: `mads::axum`, `mads::diesel`,
  `mads::diesel_migrations`, and Tower-compatible router composition.

Use `mads::prelude` for the normal application surface. Reach into `mads::core`
when the application needs framework-neutral configuration, graph, or lifecycle
types.

## Startup contract exposed by the facade

The recommended path is:

~~~text
Mads::run::<AppModule>()
        │
        ├── loads .env/mads.toml/MADS_* from the current directory
        ├── selects the rooted module scope
        ├── analyzes auto-configuration and the provider graph
        ├── constructs providers
        ├── validates routes and finalizes the Axum router
        ├── starts lifecycle and database readiness
        ├── binds and serves
        └── shuts down in reverse order
~~~

The low-level builder and `serve_router` APIs remain available for explicit
configuration, migrations, lifecycle hooks, native router merging, or listener
addresses. A rootless builder retains complete-catalog compatibility. The
inspection commands use a separate private path and stop before construction or
runtime infrastructure.

## Source layout

`crates/mads/src/lib.rs` is the facade boundary. It contains:

- public re-exports and feature gates;
- the application prelude;
- Axum, Diesel, migration, JWT, cookie, and Passport export wiring;
- documentation examples that must remain valid for external consumers.

Behavior changes belong in `mads-core` or `mads-common`, not in the facade.
When adding an API, update the owning crate first, then expose it here with the
smallest compatible feature gate.

## Tests and contributor workflow

Run facade consumer tests with:

~~~sh
cargo test -p mads --all-features
~~~

The facade's integration tests verify public feature combinations and external
consumer ergonomics. Macro fixtures live under `crates/mads/tests/ui`. When
changing a re-export or feature, run at least one no-default-features consumer
and the full workspace feature gate.

See the [root workspace guide](../../README.md#workspace-crates),
[architecture reference](../../docs/ARCHITECTURE.md), and
[contribution guide](../../CONTRIBUTING.md).
