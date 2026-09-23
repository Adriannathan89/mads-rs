# MADS.rs v0.9.0 Native SeaORM Persistence Connector Design

**Status:** Approved for implementation

**Target:** MADS framework crates 0.9.0 and Rust 1.94

**Source:** `docs/mads-persistence.md` plus the decisions approved on 2026-09-23

**Previous release scope:** `docs/superpowers/specs/2026-09-06-v0.8.0-release-scope-design.md`

## Intent

MADS 0.9 adds `mads-persistence`, a separately downloadable framework crate
that connects native ORM instances to MADS configuration, dependency
injection, diagnostics, and lifecycle management. It is not a MADS-owned ORM.

The first connector supports SeaORM 2.0 with PostgreSQL. An application opts
in by importing `mads_persistence::sea_orm::DatabaseModule`. Construction
registers exactly one native `sea_orm::DatabaseConnection`, repositories and
services inject that type directly, lifecycle startup pings it before the HTTP
listener binds, and graceful shutdown explicitly closes it.

Success means ordinary SeaORM entities, queries, and transactions work without
a MADS wrapper; applications that do not import `DatabaseModule` perform no
persistence initialization; inspection performs no I/O; and no MADS-owned
formatting discloses connection data.

## Goals

1. Add a separately publishable `mads-persistence` workspace crate at version
   0.9.0.
2. Support SeaORM 2.0, PostgreSQL, Tokio, Rustls, and SeaORM entity macros
   behind one opt-in connector feature.
3. Re-export SeaORM from the connector module so the DI graph and application
   code use one `DatabaseConnection` type identity.
4. Add a general `LifecycleResource<T>` capability to `mads-core` without
   changing the existing ordinary-provider constructor contract.
5. Support lifecycle resources through a narrow `#[provider(lifecycle)]`
   declaration.
6. Make the graph-visible and registry-visible value of a lifecycle provider
   be `T`, never `LifecycleResource<T>`.
7. Load SeaORM settings from conventional MADS configuration and preserve
   absent SeaORM options as native defaults.
8. Validate the backend before attempting a database connection.
9. Retain typed configuration and SeaORM causes while redacting public output.
10. Prove native entity CRUD, transactions, readiness, and shutdown against a
    real PostgreSQL service.
11. Release all framework crates as 0.9.0 while keeping `mads-cli` at 0.8.0.

## Non-goals

Version 0.9 does not:

- define MADS query, entity, relation, index, active-model, or transaction
  abstractions;
- wrap `DatabaseConnection` in an application-facing MADS database type;
- generate entities or replace `sea-orm-cli` or `sea-orm-migration`;
- run migrations automatically;
- support MySQL, SQLite, SQL Server, or runtime backend selection;
- support multiple named SeaORM connections;
- translate SeaORM errors into HTTP responses;
- move or redesign the existing Diesel integration in `mads-common`;
- make persistence available through the `mads` facade;
- initialize persistence without an explicit `DatabaseModule` import;
- publish a new `mads-cli` version; or
- expose live readiness or lifecycle-owner data in static inspection output.

## Global constraints

- The workspace remains Rust edition 2024 with MSRV Rust 1.94.
- All crate source keeps `#![forbid(unsafe_code)]`; public APIs remain fully
  documented under `#![deny(missing_docs)]`.
- `mads-core` must not depend on SeaORM, SQLx, Axum, `mads-common`, or
  `mads-persistence`.
- `mads-persistence` may depend normally on `mads-core` and optional SeaORM,
  but not on Axum or `mads-common`.
- The `mads-persistence` package and crate names are spelled exactly
  `mads-persistence` and `mads_persistence`.
- Existing `ProviderFuture`, `ProviderConstructor`, and
  `ProviderDescriptor::new` signatures remain source-compatible.
- Existing `#[provider]`, `#[service]`, and `#[repository]` behavior remains
  unchanged.
- Existing lifecycle ordering and failure semantics remain authoritative.
- No new implementation may lower the workspace's 85% line-coverage gate.
- The original `docs/mads-persistence.md` must be updated during implementation
  so it does not contradict this approved design.

## Approaches considered

### Chosen: native provider with a descriptor-attached lifecycle constructor

`ProviderDescriptor` gains an optional lifecycle-contribution constructor.
The existing constructor field and public constructor API remain intact.
Ordinary descriptors have no lifecycle constructor and continue through the
current path. `#[provider(lifecycle)]` emits the existing provider metadata for
`T` and attaches the separate constructor through a document-hidden descriptor
builder.

This design preserves hand-authored descriptors and macro-authored ordinary
providers while allowing the builder to receive a native value and lifecycle
registrations from one construction operation.

### Rejected: change `ProviderFuture` to return every contribution

Changing the public alias from `Result<ErasedProvider>` to
`Result<ProviderContribution>` would break manually authored constructors and
contradict the minimal-refactoring requirement. Wrapping every ordinary
provider would also expand the change beyond lifecycle-owning resources.

### Rejected: a second lifecycle-provider catalog

A second inventory catalog could preserve the original provider descriptor,
but provider metadata and lifecycle metadata would have to be correlated and
validated by `TypeId`. That duplicates selection, ownership, and duplicate
handling across catalogs.

### Rejected: graph-visible lifecycle marker providers

Registering lifecycle metadata as another provider would expose framework
plumbing in graph analysis, make the resource output ambiguous, and allow the
metadata and native resource to be selected independently.

## Core lifecycle-provider contract

### Existing contracts remain unchanged

These public contracts retain their 0.8 signatures:

```rust
pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = mads_core::Result<ErasedProvider>> + Send + 'a>>;

pub type ProviderConstructor =
    for<'a> fn(&'a ConstructionContext<'a>) -> ProviderFuture<'a>;

impl ProviderDescriptor {
    pub const fn new(
        kind: ProviderKind,
        type_name: &'static str,
        type_id: fn() -> TypeId,
        dependencies: &'static [DependencyDescriptor],
        visibility: ProviderVisibility,
        location: SourceLocation,
        constructor: ProviderConstructor,
    ) -> Self;
}
```

No ordinary provider returns or allocates lifecycle metadata.

### Lifecycle resource

`mads-core` exposes:

```rust
pub struct LifecycleResource<T> {
    value: T,
    registrations: Vec<LifecycleRegistration>,
}

impl<T> LifecycleResource<T> {
    pub fn new(value: T) -> Self;

    pub fn with_infrastructure_hook<H>(
        self,
        owner: &'static str,
        hook: H,
    ) -> Self
    where
        H: LifecycleHook + 'static;

    pub fn with_application_hook<H>(self, hook: H) -> Self
    where
        H: LifecycleHook + 'static;
}
```

The registration representation and lifecycle contribution constructor are
document-hidden core plumbing. Macro expansion can name them through
`mads_core::__private`, but applications do not construct them directly.
Infrastructure owners are stable static strings. The SeaORM/PostgreSQL owner
is `mads.persistence.seaorm.postgres`.

`ProviderDescriptor` retains its ordinary constructor and gains an optional
document-hidden lifecycle constructor. Existing `ProviderDescriptor::new`
initializes that field to `None`; only lifecycle-provider expansion sets it.
The descriptor's output name and `TypeId` describe `T`.

For compatibility with code that directly calls `ProviderDescriptor::constructor`,
the lifecycle macro also emits an ordinary adapter constructor. That adapter
invokes the declared function and returns only the erased native `T`, dropping
unregistered hook metadata. MADS builders always prefer the attached lifecycle
constructor, so normal automatic and explicit construction never loses hooks.

### Macro declaration

The only new attribute form is:

```rust
#[provider(lifecycle)]
```

It accepts only an `async fn` with one of these success shapes:

```rust
async fn resource(...) -> LifecycleResource<T>;

async fn resource(...) -> mads_core::Result<LifecycleResource<T>>;
```

The macro rejects synchronous functions, generic functions, missing concrete
return types, non-resource return types, `std::result::Result<T, E>`, nested or
aliased wrappers it cannot prove, and any argument other than the exact
`lifecycle` marker. Diagnostics point at the invalid signature and state both
accepted forms.

Connector providers use `mads_core::Result<T>` internally. Public connector
operations continue to return `PersistenceResult<T>`, and
`From<PersistenceError> for mads_core::Error` performs the boundary conversion.
The general provider macro does not gain arbitrary two-parameter `Result`
support.

### Construction behavior

Both `MadsBuilder::construct<T>()` and the automatic `MadsBuilder::build()`
loop use one internal construction helper:

1. resolve dependencies from `ConstructionContext`;
2. call the lifecycle constructor when the descriptor has one, otherwise call
   the unchanged ordinary constructor;
3. insert the erased native value under the descriptor's output `TypeId`;
4. register contributed hooks with the existing `LifecycleManager`; and
5. continue in graph dependency order.

The native value is inserted before its hooks can run. Hooks do not run until
the completed application starts. If construction of this or a later provider
fails, startup never begins and dropping the failed builder drops all already
constructed values.

Explicit construction must not discard lifecycle registrations. Inspection
and graph analysis continue to read descriptors without calling either
constructor.

## Lifecycle ordering and failure behavior

The existing sequence remains:

```text
construct complete provider graph
  -> validate routes and finalize router configuration
  -> start infrastructure hooks by owner and registration order
  -> start application hooks in registration order
  -> bind and serve HTTP
  -> stop application hooks in reverse order
  -> stop infrastructure hooks in reverse order
```

For SeaORM:

1. provider construction calls the PostgreSQL-specific
   `sea_orm::SqlxPostgresConnector::connect` entry point;
2. infrastructure startup resolves the registered `DatabaseConnection` from
   `ApplicationContext` and calls `ping()`;
3. a successful ping is required before listener binding;
4. shutdown resolves the same connection and calls `close_by_ref()`; and
5. normal `DatabaseConnection` drop remains fallback cleanup.

The lifecycle hook does not own a second pool and does not reconnect. A failed
startup ping triggers existing rollback of earlier successfully started hooks.
A close failure does not prevent later shutdown hooks from being attempted;
the existing first-shutdown-failure rule remains unchanged.

## Crate and feature boundaries

The dependency direction is:

```text
application
    |
    +-- mads
    |
    +-- mads-persistence
            |
            +-- mads-core
            +-- sea-orm 2.0
                    |
                    +-- sqlx-postgres

mads-core has no persistence dependency.
```

The new manifest has no default connector:

```toml
[features]
default = []
sea-orm-postgres = [
    "dep:sea-orm",
    "sea-orm/macros",
    "sea-orm/sqlx-postgres",
    "sea-orm/runtime-tokio-rustls",
]
```

SeaORM uses a compatible `2.0.0` Cargo range with default features disabled.
The workspace lockfile may resolve a later compatible 2.0 release, while a
dedicated CI check verifies the 2.0.0 lower bound. MySQL and SQLite require
future explicit connector features.

The connector module re-exports the selected SeaORM identity:

```rust
pub mod sea_orm {
    pub use ::sea_orm::*;

    pub struct DatabaseModule;
    pub struct SeaOrmPostgres;
}
```

Applications should import SeaORM types and traits from this module. A direct
compatible SeaORM dependency may add value-type features, but is not required
for normal entity derives, PostgreSQL access, or Tokio/Rustls operation.

## Public connector API

The common connector abstraction remains statically typed:

```rust
pub trait DatabaseConnector: Send + Sync + 'static {
    type Database: Send + Sync + 'static;

    fn connect(
        self,
    ) -> impl Future<Output = PersistenceResult<Self::Database>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DatabaseFactory;

impl DatabaseFactory {
    pub async fn provide<C>(&self, connector: C) -> PersistenceResult<C::Database>
    where
        C: DatabaseConnector;
}
```

The first connector is:

```rust
pub struct SeaOrmPostgres {
    options: sea_orm::ConnectOptions,
}

impl SeaOrmPostgres {
    pub fn new(url: impl Into<String>) -> Self;
    pub fn from_options(options: sea_orm::ConnectOptions) -> Self;
    pub fn options_mut(&mut self) -> &mut sea_orm::ConnectOptions;
}
```

Both constructors are infallible. `connect()` reads the stored URL and accepts
exactly the `postgres://` and `postgresql://` schemes. It returns an unsupported
scheme error before invoking SeaORM for any other or missing scheme.

`SeaOrmPostgres` implements a custom `Debug` that reports only the connector
and backend. It never delegates to `ConnectOptions::Debug`.

`DatabaseFactory::provide` returns the native database instance and has no
registry or lifecycle side effects. The imported module is the full MADS
integration path.

## Configuration contract

The standard module reads:

```toml
[persistence.seaorm]
url = "${DATABASE_URL}"
min_connections = 2
max_connections = 20
connect_timeout_seconds = 8
acquire_timeout_seconds = 8
idle_timeout_seconds = 600
max_lifetime_seconds = 1800
sqlx_logging = false
```

The internal typed configuration contains:

```text
url: Secret<String>                    required
min_connections: Option<u32>           optional
max_connections: Option<u32>           optional
connect_timeout_seconds: Option<u64>    optional
acquire_timeout_seconds: Option<u64>    optional
idle_timeout_seconds: Option<u64>       optional
max_lifetime_seconds: Option<u64>       optional
sqlx_logging: Option<bool>              optional
```

Absent optional fields do not call the corresponding `ConnectOptions` setter,
so SeaORM retains authority over defaults. Present duration values are whole
seconds. Present connection counts must be nonzero, and a present minimum may
not exceed a present maximum.

Conventional startup retains the existing order:

```text
optional .env interpolation
  -> optional mads.toml
  -> MADS_* environment overrides
  -> typed persistence configuration
```

`MADS_PERSISTENCE__SEAORM__MAX_CONNECTIONS`, for example, overrides
`persistence.seaorm.max_connections` under the existing mapping rules.

The URL remains a `Secret<String>` until it is explicitly exposed to construct
`ConnectOptions`. Configuration formatting, validation issues, inspection,
and startup summaries retain keys, safe codes, and source labels only.

## Module registration

The connector declares:

```rust
#[mads_core::module(global)]
pub struct DatabaseModule;
```

Its conceptual providers are:

```rust
#[provider]
fn database_factory() -> DatabaseFactory;

#[provider]
fn sea_orm_postgres_connector(config: Config) -> mads_core::Result<SeaOrmPostgres>;

#[provider(lifecycle)]
pub async fn sea_orm_database(
    factory: DatabaseFactory,
    connector: SeaOrmPostgres,
) -> mads_core::Result<LifecycleResource<DatabaseConnection>>;
```

The factory and connector are private to `DatabaseModule`; the native database
provider is public. Once the root imports the global module, current module
scope rules expose the database to every selected module. Without that import,
the module is unreachable and none of its providers is selected or run.

Provider identity remains concrete Rust `TypeId`. A second unqualified
`DatabaseConnection` is a normal `MADS002` ambiguous-provider error. Named databases,
string resolution, and implicit winner selection remain deferred.

## Errors and redaction

`mads-persistence` exposes:

```rust
pub const MADS140: DiagnosticCode = DiagnosticCode::new("MADS140");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistenceErrorKind {
    InvalidConfiguration,
    UnsupportedScheme,
    Connection,
    Readiness,
    GracefulClose,
}

pub struct PersistenceError { /* safe context and retained source */ }
pub type PersistenceResult<T> = Result<T, PersistenceError>;

impl PersistenceError {
    pub const fn kind(&self) -> PersistenceErrorKind;
}
```

The enum supports programmatic classification without parsing messages.
`PersistenceError` has custom `Display` and `Debug` implementations that may
name only the connector, backend, operation, and safe category. It never
formats its retained source.

`From<PersistenceError> for mads_core::Error` creates the safe `MADS140`
diagnostic and retains `PersistenceError` as its source. Resulting chains are:

```text
provider configuration or connection
MADS006 -> MADS140 -> PersistenceError -> optional ConfigurationErrors or DbErr

lifecycle readiness or close
MADS011 -> MADS140 -> PersistenceError -> DbErr

standalone factory
PersistenceError -> optional DbErr
```

No MADS-owned `Display`, `Debug`, diagnostic, inspection record, startup
summary, or test snapshot may contain a URL, username, password, query
parameter, interpolated value, rejected raw value, SQL statement, binding,
entity value, or arbitrary driver message. Typed causes remain available only
through `std::error::Error::source` for controlled server-side diagnostics.

No conversion from `sea_orm::DbErr` to an HTTP response is added. SQL logging
remains a native SeaORM/SQLx option, and connector documentation warns that
verbose statement or binding logs may reveal application data.

## Inspection and observability

Graph inspection reports the existing information for the native provider:

- its native `DatabaseConnection` type name;
- ownership by the global `DatabaseModule`;
- ordinary provider origin and visibility; and
- planned construction state.

Inspection does not construct `ConnectOptions`, connect, ping, close, or run a
hook. It reports no live readiness result and adds no lifecycle-owner field in
0.9.

## Versioning and release behavior

These packages move to 0.9.0:

- `mads-core-macros`;
- `mads-common-macros`;
- `mads-core`;
- `mads-extra`;
- `mads-common`;
- `mads`;
- `mads-persistence`.

`mads-cli` remains package version 0.8.0 but updates its exact internal MADS
dependency pins to 0.9.0. It remains in workspace builds and platform tests.
Neither beta nor stable 0.9 publication includes `mads-cli`.

Release preparation includes `mads-persistence` in framework version updates
and lockfile checks. Publish workflows release it after `mads-core` and before
any future crate that depends on it. The package remains independently
downloadable even though its version follows the framework release line.

The re-exported SeaORM major version and the compatible `mads-core` type
identity are part of the connector's public contract. A SeaORM major upgrade or
incompatible lifecycle-provider change requires a documented compatibility
release.

## Testing strategy

### Core and macro tests

- ordinary provider graph, registry, and construction behavior is unchanged;
- existing hand-authored `ProviderFuture` and `ProviderDescriptor::new` code
  still compiles and runs;
- lifecycle providers expose `T`, not `LifecycleResource<T>`;
- automatic and explicit construction both register contributed hooks;
- infrastructure ordering, reverse shutdown, rollback, and first-failure
  retention remain unchanged;
- a later construction failure runs no contributed hook;
- inspection runs neither constructors nor hooks;
- compile-pass fixtures cover both accepted lifecycle signatures; and
- compile-fail fixtures cover synchronous, generic, non-resource,
  two-parameter-result, nested-wrapper, and malformed-argument declarations.

### Connector tests without PostgreSQL

- the crate compiles with no default features;
- `sea-orm-postgres` exposes macros, native types, and PostgreSQL runtime
  support;
- required, optional, range, zero, and min/max configuration cases behave as
  specified;
- both PostgreSQL schemes pass validation and every other scheme fails before
  SeaORM is invoked;
- `DatabaseFactory::provide` has the native output type;
- importing `DatabaseModule` selects exactly one global native provider;
- repositories inject the native type without a wrapper;
- duplicate native providers retain the current core diagnostic;
- omitting the module selects and constructs no persistence provider;
- graph inspection succeeds without a database; and
- sentinel credentials and URLs are absent from all MADS-owned formatting and
  diagnostics.

### PostgreSQL integration tests

- startup connects and pings before listener binding;
- invalid credentials prevent listener binding;
- a derived native SeaORM entity can insert, find, update, and delete;
- native transactions commit and roll back;
- graceful shutdown makes the pool reject a later ping;
- an operational failure still attempts shutdown; and
- captured MADS-owned output contains no credential sentinel.

PostgreSQL tests are ignored during ordinary unit runs and execute serially in
the existing PostgreSQL service job. Normal CI also runs formatting, Clippy,
workspace tests, doctests, rustdoc, package checks, Rust 1.94, and 85% coverage.
A dedicated resolution check verifies the declared SeaORM 2.0.0 minimum.

## Documentation changes during implementation

Implementation updates:

- `docs/mads-persistence.md` to match every decision in this specification;
- the new crate README and public rustdoc;
- root workspace and feature documentation;
- application examples for explicit module import and native injection;
- package-content verification;
- release scripts and beta/stable publication workflows; and
- `CHANGELOG.md` for 0.9.0.

Documentation must not describe the connector as implemented until acceptance
tests pass.

## Acceptance criteria

MADS persistence 0.9 is complete only when:

1. Existing ordinary and hand-authored provider constructor contracts remain
   source-compatible.
2. Both approved lifecycle-provider signatures compile and malformed forms
   fail with focused diagnostics.
3. Automatic and explicit construction register the native value and its
   lifecycle hooks exactly once.
4. A separately dependent application can import `DatabaseModule` directly.
5. Conventional `Mads::run::<AppModule>()` constructs the configured native
   PostgreSQL connection.
6. A repository field of native `DatabaseConnection` resolves through normal
   DI and uses ordinary SeaORM APIs.
7. Inspection and applications without the module import perform no database
   I/O.
8. Connection and readiness failures occur before listener binding.
9. Graceful shutdown explicitly closes the pool and retains current hook
   failure semantics.
10. All configuration, connector, lifecycle, and diagnostic formatting passes
    credential-redaction tests.
11. SeaORM 2.0.0 minimum resolution, current 2.0 resolution, stable Rust, Rust
    1.94, feature-boundary, PostgreSQL, package, documentation, lint, and
    coverage gates pass.
12. Framework crates publish as 0.9.0, CLI remains 0.8.0 and is excluded from
    both 0.9 publication lists, and release documentation is internally
    consistent.

## Deferred extensions

Future designs may add MySQL or SQLite features, other ORMs, named connection
newtypes, opt-in migrations, or connector-specific health details. Each must
preserve the native-instance principle and must not force unrelated ORMs behind
one runtime trait object.
