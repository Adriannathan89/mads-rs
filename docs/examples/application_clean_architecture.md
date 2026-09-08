# MADS.rs — Clean Architecture with v0.8 Persistence

MADS belongs at the composition and delivery edge. Domain and application code
should not depend on Axum, Diesel, or HTTP response types.

```text
HTTP / MADS delivery
        ↓
Infrastructure (Diesel repository)
        ↓
Application use cases and ports
        ↓
Domain
```

## Available in v0.8

`Database` and `Database::run` are available now. A repository can accept the
managed database and keep native Diesel details inside infrastructure:

```rust,ignore
#[mads::repository]
struct DieselUserRepository {
    database: mads::Database,
}

impl DieselUserRepository {
    async fn find(&self, id: i64) -> mads::DatabaseResult<Option<User>> {
        self.database
            .run(move |connection| {
                // Native Diesel select/query code belongs here.
                users::table.find(id).first(connection).optional()
            })
            .await
    }
}
```

The composition root loads `mads.toml` plus optional `.env`, resolves
`DatabaseConfig`, and explicitly calls
`builder.database(DatabaseBootstrap::new(database_config))`. If startup
migrations are enabled, it provides
`DatabaseBootstrap::new(database_config).with_migrations(MIGRATIONS)` instead.
The database URL stays as `${DATABASE_URL}` in tracked configuration; real
values belong in ignored `.env` locally or in production process variables.

Delivery code maps application outcomes to HTTP deliberately. A failed
`Database::run` does **not** automatically become an HTTP error response. When
the generic safe mapping is suitable, `DatabaseResult<T>` and native Diesel
`QueryResult<T>` can opt in with `.into_http()` (not-found is 404, typed unique
violation is 409, every other database failure is redacted 500). Domain code
still need not depend on HTTP types.

## Project shape

```text
src/
├── domain/                 # entities and domain rules
├── application/            # use cases and repository-port traits
├── infrastructure/         # Diesel schemas/models/repositories
├── delivery/http/          # MADS route traits and controllers
└── main.rs                 # Config + typed provider + DatabaseBootstrap composition root
```

The application layer owns an ordinary Rust repository-port trait. The
infrastructure implementation owns its Diesel schema/query types, and the
controller depends on application-facing behavior rather than moving database
types into the domain.

## v0.8 input and configuration edge

Request DTO validation is a delivery concern and uses `ValidatedJson`, not a
renamed native `Json` alias:

```rust,ignore
use mads::prelude::*;

#[derive(serde::Deserialize, Input)]
struct CreateUserRequest {
    #[validate(email)]
    email: String,
    #[validate(length(min = 8))]
    password: String,
}

#[routes(prefix = "/users")]
trait UserRoutes {
    #[post("/")]
    async fn create(&self, body: ValidatedJson<CreateUserRequest>) -> HttpResult<Json<User>>;
}
```

The application layer receives the typed DTO only after validation. Native
`Json<T>` remains available for an application-owned extraction policy and does
not validate automatically.

Configuration has the same explicit edge. A selected provider calls
`Config::parse::<AppConfig>()`; a failing typed view prevents startup before
the listener binds. Use `Secret<String>` for credentials and deliberately call
`.expose()` only at the infrastructure boundary. Its normal `Display` and
`Debug` representations are `[REDACTED]`.

```rust,ignore
#[derive(Configuration)]
#[config(prefix = "app")]
struct AppConfig {
    #[config(default = "clean-user-api")]
    name: String,
    database_password: Secret<String>,
}

#[provider]
fn app_config(config: Config) -> mads::core::Result<AppConfig> {
    Ok(config.parse()?)
}
```

`Config::parse` does not reload sources. Conventional `Mads::run` still uses
optional `.env` for exact `${NAME}` interpolation, optional `mads.toml`, then
final scalar `MADS_*` overrides. Typed errors aggregate full keys and winning
sources without configured values.

Trait-binding syntax, a MADS database test DSL, and automatic database-to-HTTP
conversion remain outside this example. The v0.8 mapping is opt in through
`.into_http()` so application delivery policy stays explicit.
