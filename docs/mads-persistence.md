# MADS Persistence Connector Design

Status: original connector design record. The 0.9 database/CLI boundary is
superseded by the [approved removal design](superpowers/specs/2026-09-23-mads-0.9-database-surface-removal-design.md).

In 0.9, applications add `mads-persistence = { version = "0.9.0", features =
["sea-orm-postgres"] }` explicitly and import
`mads_persistence::sea_orm::DatabaseModule`. `DatabaseFactory::provide` returns
the native `DatabaseConnection` or typed `PersistenceError`; database support
is not a `mads`/`mads-common` feature, and the CLI has no database commands.
SeaORM owns migrations. The original proposal below is retained for context;
its statements about retaining Diesel do not describe the shipped 0.9 API.

## Summary

`mads-persistence` is a connector crate between MADS and existing Rust ORM
ecosystems. It is not a new ORM, query builder, schema language, entity model,
or repository abstraction.

The crate integrates an ORM's native database instance with MADS application
construction, dependency injection, configuration, diagnostics, and lifecycle.
Application code continues to use the ORM exactly as documented by that ORM.
The first supported combination is SeaORM with PostgreSQL.

The initial public contract is:

- an application imports `mads_persistence::sea_orm::DatabaseModule` from its
  root module;
- `DatabaseModule` is a MADS global module;
- startup constructs one native `sea_orm::DatabaseConnection`;
- MADS registers that native connection as an application-scoped provider;
- repositories and services inject `DatabaseConnection` directly;
- SeaORM owns entities, relations, indexes, queries, transactions, and
  migrations;
- MADS checks the connection before serving and closes the pool during orderly
  shutdown.

The package and Rust crate name is `mads-persistence` / `mads_persistence`.
The misspelling `mads-persistance` is not used.

## Motivation

Building a MADS-owned ORM would duplicate mature work and force MADS to own a
large persistence surface: SQL generation, entity metadata, relations,
transactions, migrations, backend differences, and compatibility with database
drivers. It would also make native ORM examples harder to reuse.

A connector has a smaller and more durable responsibility:

1. construct the ORM's native database value;
2. register that value in the MADS provider graph;
3. expose it across module boundaries through a global module;
4. integrate readiness and shutdown with the MADS lifecycle;
5. normalize framework-facing failures without disclosing credentials.

This boundary lets MADS support multiple ORM ecosystems without inventing a
lowest-common-denominator database API.

## Goals

The first release must:

- ship as a crate that can be versioned and downloaded separately from the
  main `mads` facade;
- integrate with MADS dependency injection and module scoping;
- support SeaORM with PostgreSQL;
- register the native `sea_orm::DatabaseConnection` type;
- support the conventional `Mads::run::<AppModule>()` startup path;
- read connection settings from normal MADS configuration sources;
- perform connection readiness before the HTTP listener binds;
- explicitly close the SeaORM pool during graceful shutdown;
- retain typed internal causes while keeping configuration values and
  credentials out of public diagnostics;
- preserve an extension point for later ORM connectors.

## Non-goals

The first release does not:

- define a common query, entity, relation, index, or transaction API;
- wrap `DatabaseConnection` in an application-facing MADS type;
- generate SeaORM entities;
- replace `sea-orm-cli` or `sea-orm-migration`;
- automatically run migrations;
- expose MySQL or SQLite connectors;
- support multiple named SeaORM connections in one application;
- translate SeaORM errors into HTTP responses automatically;
- move the existing Diesel integration out of `mads-common`;
- make a database connection available unless the application explicitly
  imports the connector module.

Entity generation, schema definitions, indexes, relations, migrations, and
query execution remain native SeaORM concerns. Applications may run SeaORM
migrations explicitly using `MigratorTrait` without a MADS-specific migration
API.

## Approaches considered

### MADS-owned ORM

This would give MADS complete control over the API, but it would duplicate an
ORM ecosystem and make MADS responsible for database behavior far beyond its
application-framework role. This approach is rejected.

### Universal MADS database wrapper

A type such as `MadsDatabase<Backend>` could hide connector differences, but
applications would no longer receive the ORM's native type. It would either
leak ORM-specific methods over time or restrict users to an artificial common
subset. This approach is rejected.

### Native provider with lifecycle contribution

The selected approach constructs the native ORM value and contributes a
lifecycle hook alongside it. MADS stores only the native value in the provider
registry. The lifecycle contribution is framework metadata and is not visible
in repository or service APIs.

This approach requires one general addition to `mads-core`: an async provider
must be able to return a provider value together with one or more lifecycle
registrations. That capability is useful for database pools, message brokers,
background clients, and other resources created during provider construction.

## Crate and dependency boundaries

The intended dependency direction is:

```text
application
    |
    +-- mads
    |
    +-- mads-persistence
            |
            +-- mads-core
            +-- sea-orm
                    |
                    +-- sqlx-postgres

mads-core has no dependency on SeaORM or mads-persistence.
```

`mads-persistence` depends only on the MADS core contracts required for
configuration, providers, modules, diagnostics, and lifecycle. It must not
depend on Axum or the HTTP layer.

The original proposal retained the Diesel integration in `mads-common`, but
that integration was removed for 0.9. SeaORM remains an explicit, native
provider, not a universal `Database` wrapper.

The connector module re-exports the supported ORM surface so applications can
use one coherent type identity while the same module also owns MADS-specific
connector types:

```rust
pub mod sea_orm {
    pub use ::sea_orm::*;

    pub struct DatabaseModule;
    pub struct SeaOrmPostgres;
}
```

Application code should prefer:

```rust
use mads_persistence::sea_orm::{DatabaseConnection, EntityTrait};
```

This prevents two independently selected SeaORM versions from producing
different `DatabaseConnection` types in the DI graph.

The first release has no default connector feature. The application enables:

```toml
mads-persistence = {
    version = "0.1",
    default-features = false,
    features = ["sea-orm-postgres"]
}
```

`sea-orm-postgres` enables SeaORM's PostgreSQL SQLx driver and Tokio/Rustls
runtime integration. Additional SeaORM value-type features remain explicitly
selected by `mads-persistence` features or by a compatible direct SeaORM
dependency. MySQL and SQLite require separate future connector features rather
than runtime backend selection.

## Public connector model

The connector abstraction is statically typed. It does not erase the native
database output:

```rust
pub trait DatabaseConnector: Send + Sync + 'static {
    type Database: Send + Sync + 'static;

    fn connect(
        self,
    ) -> impl Future<Output = Result<Self::Database, PersistenceError>> + Send;
}
```

`DatabaseFactory` provides the common construction entry point:

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct DatabaseFactory;

impl DatabaseFactory {
    pub async fn provide<C>(
        &self,
        connector: C,
    ) -> Result<C::Database, PersistenceError>
    where
        C: DatabaseConnector,
    {
        connector.connect().await
    }
}
```

`provide` constructs and returns the connector's native instance. It does not
mutate the MADS registry by side effect. Registration occurs because a MADS
provider returns that instance. This keeps graph construction deterministic and
makes duplicate-provider validation remain a core responsibility.

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

impl DatabaseConnector for SeaOrmPostgres {
    type Database = sea_orm::DatabaseConnection;
}
```

`SeaOrmPostgres` validates that its URL selects PostgreSQL. It does not accept
MySQL or SQLite URLs when compiled as the PostgreSQL connector. Its `Debug`
implementation must redact the URL.

The connector accepts `ConnectOptions` rather than reproducing every SeaORM
pool option in the factory API. This preserves SeaORM as the authority for pool
configuration and provides a native escape hatch for advanced settings.

`DatabaseFactory::provide` can also be called outside MADS construction when
an application only wants the connector utility:

```rust
let factory = DatabaseFactory;
let database: sea_orm::DatabaseConnection = factory
    .provide(SeaOrmPostgres::new(database_url))
    .await?;
```

That standalone call does not register the value or attach a lifecycle hook.
The imported `DatabaseModule` is the supported path for full MADS integration.

## Configuration

The standard module reads this namespace:

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

Only `url` is required. Every optional setting that is absent leaves the
corresponding SeaORM default unchanged. Numeric durations are whole seconds.
Zero connection counts and invalid relationships such as
`min_connections > max_connections` fail configuration validation before
lifecycle startup.

The internal typed configuration uses `Secret<String>` for the URL. It may
expose the URL only while creating `ConnectOptions`. `Debug`, `Display`, MADS
diagnostics, inspection output, and startup summaries must never contain the
URL, username, password, query parameters, or interpolated environment value.

Conventional startup retains the existing source order:

```text
optional .env interpolation
  -> optional mads.toml
  -> MADS_* environment overrides
  -> typed persistence configuration
```

For example, `MADS_PERSISTENCE__SEAORM__MAX_CONNECTIONS` overrides
`persistence.seaorm.max_connections` under the existing configuration rules.

## Module registration and application startup

The public connector module is namespaced by ORM:

```rust
use mads_persistence::sea_orm::DatabaseModule;
```

The namespaced path leaves room for other connectors in future releases
without creating one runtime enum for incompatible native database types.

`DatabaseModule` is global:

```rust
#[mads_core::module(global)]
pub struct DatabaseModule;
```

Importing the module is the explicit opt-in that selects and constructs the
connection. Once the root imports it, its public native provider is visible to
every selected module according to current global-module rules.

A complete application entry point is:

```rust
use mads::prelude::*;
use mads_persistence::sea_orm::DatabaseModule;

mod users {
    use mads::prelude::*;
    use mads_persistence::sea_orm::{
        self,
        DatabaseConnection,
        EntityTrait,
    };

    use crate::entities::user;

    #[repository]
    pub struct UserRepository {
        db: DatabaseConnection,
    }

    impl UserRepository {
        pub async fn find_all(
            &self,
        ) -> Result<Vec<user::Model>, sea_orm::DbErr> {
            user::Entity::find().all(&self.db).await
        }
    }

    #[module]
    pub struct UserModule;
}

use users::UserModule;

#[module(imports = [DatabaseModule, UserModule])]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
```

The repository receives the native SeaORM pool handle. It can use all ordinary
SeaORM entity, query, transaction, and connection traits. No `.run(...)`, MADS
query closure, or adapter method is required.

Conceptually, `DatabaseModule` contains these providers:

```rust
#[provider]
fn database_factory() -> DatabaseFactory {
    DatabaseFactory
}

#[provider]
fn sea_orm_postgres_connector(
    config: Config,
) -> Result<SeaOrmPostgres, PersistenceError> {
    SeaOrmPostgres::from_config(&config)
}

#[provider(lifecycle)]
pub async fn sea_orm_database(
    factory: DatabaseFactory,
    connector: SeaOrmPostgres,
) -> Result<LifecycleResource<DatabaseConnection>, PersistenceError> {
    let database = factory.provide(connector).await?;

    Ok(
        LifecycleResource::new(database)
            .with_infrastructure_hook(
                "mads.persistence.seaorm.postgres",
                SeaOrmLifecycle,
            ),
    )
}
```

The graph-visible output of `sea_orm_database` is
`sea_orm::DatabaseConnection`, not `LifecycleResource<DatabaseConnection>`.
The wrapper exists only as a construction contribution understood by the
provider macro and builder.

## One default connection in v1

MADS identifies providers by concrete Rust `TypeId`. Two unqualified
`DatabaseConnection` providers are therefore duplicates, which is desirable
for the default contract: an application gets exactly one unambiguous native
SeaORM connection.

Multiple named databases are deferred. A later design may introduce explicit
newtypes such as `PrimaryDatabase` and `AnalyticsDatabase`, each containing a
native `DatabaseConnection`. It must not introduce string-based resolution or
silently pick one duplicate connection.

## New lifecycle-provider support in `mads-core`

### Current limitation

Today, ordinary providers return only an `ErasedProvider`. Lifecycle hooks are
registered directly on `MadsBuilder` or supplied by the synchronous official
auto-configuration path. An async provider can create a SeaORM connection, but
it cannot contribute the hook that should ping and close that connection.

Using only a normal async provider would still close the pool eventually when
the application context is dropped, but it would not make readiness and
orderly shutdown explicit MADS lifecycle operations. A builder-only extension
could register the hook, but it would not work with the standard
`Mads::run::<AppModule>()` path requested by this design.

### Provider contribution

`mads-core` will generalize the erased constructor result:

```rust
pub(crate) struct ProviderContribution {
    provider: ErasedProvider,
    lifecycle: Vec<LifecycleRegistration>,
}
```

`ProviderFuture` will return `Result<ProviderContribution>` internally.
Existing `#[provider]`, `#[service]`, and `#[repository]` declarations continue
to return their current Rust values; generated code wraps them in a
contribution with no lifecycle registrations. This is source-compatible for
application code.

The builder construction loop becomes:

1. invoke the provider constructor;
2. split its value and lifecycle registrations;
3. insert the native value under the descriptor's existing output `TypeId`;
4. add contributed hooks to the lifecycle manager;
5. continue construction in dependency order.

If later provider construction fails, lifecycle startup never begins and all
already constructed values are dropped with the failed builder. No listener is
bound.

### Lifecycle resource declaration

Providers that create lifecycle-owned infrastructure use an explicit marker:

```rust
#[provider(lifecycle)]
async fn resource(...) -> Result<LifecycleResource<T>, Error>;
```

The macro requires exactly one `LifecycleResource<T>` success value and emits
provider metadata for `T`. Consumers therefore depend on `T`; the wrapper never
appears in the application graph or `ApplicationContext`.

The proposed core type is:

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

Infrastructure ownership is explicit so framework resources still start
before application hooks and stop after application hooks. Owner identifiers
must be stable static strings. The SeaORM/PostgreSQL owner is
`mads.persistence.seaorm.postgres`.

This API is general core infrastructure. It contains no database concepts and
can later support clients, consumers, schedulers, or background resources.

### Lifecycle ordering

The established MADS lifecycle rules remain authoritative:

```text
construct complete provider graph
  -> validate routes and finalize router configuration
  -> start infrastructure hooks in deterministic owner/registration order
  -> start application hooks in registration order
  -> bind and serve HTTP
  -> stop application hooks in reverse order
  -> stop infrastructure hooks in reverse order
```

For the SeaORM connector:

1. provider construction calls `sea_orm::Database::connect`;
2. lifecycle startup calls `DatabaseConnection::ping`;
3. only successful lifecycle startup permits listener binding;
4. lifecycle shutdown calls `DatabaseConnection::close_by_ref`;
5. dropping `DatabaseConnection` remains the fallback cleanup path.

The hook resolves `DatabaseConnection` from `ApplicationContext`, so it uses
the same registered native instance as application repositories. It does not
store a second pool or reconnect.

### Failure behavior

Connection construction failures are provider-construction failures. They stop
the build before any lifecycle hook or listener starts. The ordinary MADS
provider diagnostic retains `PersistenceError` as its source.

A failed startup ping is a lifecycle startup failure. The lifecycle manager:

- stops already-started hooks in reverse order;
- never binds the HTTP listener;
- returns the existing `MADS011` lifecycle diagnostic with the safe persistence
  error retained as its source.

A shutdown close failure does not prevent later hooks from being attempted.
The first shutdown failure remains the primary error under current lifecycle
rules. SeaORM connection drop remains a best-effort fallback after the
application is released.

The provider that fails during its own startup is responsible for cleaning up
any partially started external activity before returning an error. SeaORM's
`ping` creates no separately managed activity, so no special rollback is
needed for this connector.

## Error and redaction contract

`mads-persistence` exposes:

```rust
pub struct PersistenceError { /* redacted context and retained source */ }
pub type PersistenceResult<T> = Result<T, PersistenceError>;
pub const MADS140: DiagnosticCode = DiagnosticCode::new("MADS140");
```

`PersistenceError` converts into `mads_core::Error`. This allows fallible
connector providers and lifecycle hooks to retain the persistence cause while
participating in the existing `MADS006` provider-construction and `MADS011`
lifecycle diagnostic boundaries.

Errors have stable safe categories, including:

- invalid connector configuration;
- unsupported database URL scheme;
- connection establishment failure;
- readiness failure;
- graceful close failure.

Public `Display` and `Debug` output may name the connector, backend, operation,
and safe category. They must not include the URL, credentials, SQL text,
bindings, entity values, or arbitrary driver error messages. The original
SeaORM `DbErr` remains available through `std::error::Error::source` for
server-side diagnostics, subject to the same rendering discipline used by
other MADS internal sources.

No blanket conversion from `DbErr` to an HTTP response is added. Applications
own domain and delivery mappings.

## Native SeaORM behavior

The injected connection is the exact native type:

```rust
sea_orm::DatabaseConnection
```

This guarantees that normal SeaORM examples remain applicable:

```rust
let users = user::Entity::find().all(&database).await?;

let user = user::Entity::find_by_id(id).one(&database).await?;

let inserted = user::ActiveModel {
    name: Set("Ada".to_owned()),
    ..Default::default()
}
.insert(&database)
.await?;

database
    .transaction::<_, (), sea_orm::DbErr>(|transaction| {
        Box::pin(async move {
            // ordinary SeaORM transaction work
            Ok(())
        })
    })
    .await?;
```

`mads-persistence` must not define parallel entity, relation, index, active
model, query, or transaction traits.

## Inspection and observability

MADS graph output exposes:

- the native provider type name;
- provider ownership by the global `DatabaseModule`;
- the ordinary provider origin already supported by the core graph.

Inspection must never establish a real database connection. The existing MADS
inspection path performs graph analysis without provider construction, and the
connector must preserve that boundary.

Inspection consequently reports no live readiness result. Readiness belongs to
runtime lifecycle startup, not static graph analysis. Adding public lifecycle
owner metadata to inspection output is outside the first connector release.

SQL logging remains a SeaORM/SQLx option. MADS does not log queries itself.
When enabled, SQL logging follows SeaORM behavior and is outside MADS error
normalization; documentation must warn applications not to enable verbose SQL
or binding logs when values are sensitive.

## Compatibility and versioning

`mads-persistence` is released independently but declares an explicit
compatible MADS core range. Its first release targets the lifecycle-provider
contract introduced for MADS 0.9 and Rust 1.94.

The initial SeaORM line is 2.0. SeaORM 2.0 also declares Rust 1.94, aligning it
with the MADS 0.9 baseline. The connector should use a compatible 2.0 version
range and test the minimum resolved dependency accepted by its manifest.

Compatibility promises are:

- patch releases may expand compatible SeaORM patch versions;
- changing to a new SeaORM major version requires a documented
  `mads-persistence` compatibility release;
- the re-exported SeaORM version is part of the connector's public type
  contract;
- a MADS core lifecycle API incompatibility requires a corresponding
  `mads-persistence` release;
- `mads` and `mads-persistence` do not need identical package versions, but
  their declared Cargo dependency ranges must resolve to one compatible
  `mads-core` type identity.

## Testing strategy

### `mads-core`

- ordinary providers still produce the same graph and registry values;
- `#[provider(lifecycle)]` exposes `T`, not `LifecycleResource<T>`;
- malformed lifecycle provider signatures fail through `trybuild`;
- contributed infrastructure hooks start before application hooks;
- shutdown remains reverse order;
- a later construction failure starts no contributed hook;
- startup failure rolls back earlier contributed hooks;
- shutdown attempts every hook and retains the first failure;
- inspection does not run constructors or hooks.

### `mads-persistence`

- configuration parsing validates required and related fields;
- every debug and error representation redacts a sentinel URL and password;
- non-PostgreSQL URLs are rejected before connection;
- `DatabaseFactory::provide` returns a native `DatabaseConnection`;
- importing `DatabaseModule` contributes exactly one global native provider;
- repositories can inject and use that provider without a wrapper;
- duplicate native connection providers retain the core duplicate diagnostic;
- graph inspection requires no live PostgreSQL server.

### PostgreSQL integration

- startup connects and pings before listener binding;
- invalid credentials prevent listener binding;
- a native SeaORM entity can insert, find, update, and delete through the
  injected connection;
- native transactions commit and roll back correctly;
- orderly application shutdown closes the pool;
- a forced operation failure still attempts shutdown;
- no credential appears in captured diagnostics or logs owned by MADS.

CI runs unit, macro, documentation, and graph tests without PostgreSQL. A
separate PostgreSQL service job runs ignored integration tests, following the
existing repository pattern.

## Acceptance criteria

The design is complete when all of these behaviors are implemented and tested:

1. A new application can depend on `mads-persistence` separately.
2. `DatabaseModule` can be imported directly by a root `AppModule`.
3. `Mads::run::<AppModule>()` constructs the configured PostgreSQL connection.
4. A repository field of type `sea_orm::DatabaseConnection` resolves through
   ordinary MADS DI.
5. Native SeaORM query and transaction APIs work without a MADS wrapper.
6. Graph inspection does not connect to PostgreSQL.
7. Connection or readiness failure occurs before listener binding.
8. Graceful shutdown explicitly closes the SeaORM pool.
9. Credential-bearing configuration is redacted from MADS-owned output.
10. Applications that do not import `DatabaseModule` perform no persistence
    initialization.
11. Existing providers and lifecycle hooks retain their behavior.
12. Stable and Rust 1.94 CI gates pass.

## Future connectors

Future ORM support follows the same boundary:

```text
mads_persistence::<orm>::DatabaseModule
  -> ORM-specific connector configuration
  -> native ORM pool/client type in DI
  -> readiness and shutdown lifecycle contribution
```

Each connector owns its features, native output type, configuration namespace,
and lifecycle adapter. The shared crate may reuse `DatabaseFactory`, connector
error normalization, and generic lifecycle-provider support, but it must not
force unrelated ORMs behind one runtime trait object.

Potential later work includes MySQL/SQLite backends, additional ORMs, named
connection newtypes, opt-in migration hooks, and connector-specific health
details. Each requires a separate design and must preserve the native-instance
principle established here.

## External references

- [SeaORM database connections](https://www.sea-ql.org/SeaORM/docs/install-and-config/connection/)
- [SeaORM crate features](https://docs.rs/crate/sea-orm/latest/features)
- [SeaORM migration setup](https://www.sea-ql.org/SeaORM/docs/migration/setting-up-migration/)
