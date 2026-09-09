# MADS.rs 0.8 PostgreSQL User Slice

This example keeps a PostgreSQL repository explicit while using the current
v0.8 CLI, error, validation, and configuration boundaries. The generated
`mads new` starter is intentionally database-free; add persistence only where
the application needs it.

## v0.8 CLI workflow and split schema

From the application root, inspect and run the standard entry point directly:

```bash
mads doctor
mads routes
mads run
mads dev
```

The desired Diesel schema can be split into ordinary Rust files and loaded
recursively:

```text
src/
├── main.rs
└── schema/
    ├── comment.rs
    └── user.rs
```

`src/schema/user.rs`:

```rust,ignore
diesel::table! {
    users (id) {
        id -> Int8,
        name -> Varchar,
    }
}
```

`src/schema/comment.rs`:

```rust,ignore
diesel::table! {
    comments (id) {
        id -> Int8,
        body -> Text,
    }
}
```

Generate a complete current diff with an automatic migration name, review the
files, and only then apply them:

```bash
mads db generate
# review migrations/<automatic_name>/up.sql and down.sql
mads db migrate
```

There is no named generation form. Defaults, indexes, checks, triggers, and a
complete foreign-key policy remain explicit migration-SQL review items in the
v0.8 bounded schema planner.

This persistence example keeps persistence explicit: the composition root loads
configuration, registers one `DatabaseBootstrap`, and a repository uses native
Diesel through `Database::run`. Database delivery remains explicit: use a
domain-specific `map_err`, native Diesel behavior, or the opt-in `.into_http()`
extension when the approved generic 404/409/500 mapping is appropriate.

## Configuration

```toml
# mads.toml (tracked)
[database]
url = "${DATABASE_URL}"
pool_size = 10
migrate = false
```

Copy `.env.example` to the ignored `.env` for local use. In production or CI,
set `DATABASE_URL` as a process variable. To override the config key itself,
set `MADS_DATABASE__URL`; that final `MADS_` source wins over TOML and dotenv.

## `src/main.rs`

```rust,no_run
use mads::diesel::prelude::*;
use mads::{
    core::{ConfigBuilder, DotenvSource, EnvSource, TomlSource},
    diesel,
    prelude::*,
};

diesel::table! {
    users (id) {
        id -> Int8,
        name -> Varchar,
    }
}

#[derive(serde::Serialize, diesel::Queryable, diesel::Selectable)]
#[diesel(table_name = users)]
struct User {
    id: i64,
    name: String,
}

#[mads::repository]
struct UserRepository {
    database: Database,
}

impl UserRepository {
    async fn find(&self, id: i64) -> DatabaseResult<Option<User>> {
        self.database
            .run(move |connection| {
                users::table
                    .find(id)
                    .select(User::as_select())
                    .first(connection)
                    .optional()
            })
            .await
    }
}

#[mads::routes(prefix = "/users")]
trait UserRoutes {
    #[mads::get("/:id")]
    async fn get_user(&self, id: Path<i64>) -> HttpResult<Json<User>>;
}

#[mads::controller(routes = [UserRoutes])]
struct UserController {
    users: UserRepository,
}

impl UserRoutes for UserController {
    async fn get_user(&self, Path(id): Path<i64>) -> HttpResult<Json<User>> {
        self.users
            .find(id)
            .await
            .map_err(|_| HttpError::internal(std::io::Error::other("database query failed")))?
            .map(Json)
            .ok_or_else(|| HttpError::not_found("user was not found"))
    }
}

#[mads::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConfigBuilder::new()
        .dotenv(DotenvSource::optional(".env"))
        .source(TomlSource::file("mads.toml"))
        .source(EnvSource::new("MADS_"))
        .build()?;
    let database = DatabaseConfig::from_config(&config)?;
    let mut builder = Mads::builder_with_config(config);
    builder.database(DatabaseBootstrap::new(database))?;
    let application = builder.build().await?;
    serve(application, "127.0.0.1:3000").await?;
    Ok(())
}
```

For startup migrations, declare an `EmbeddedMigrations` constant with
`mads::diesel_migrations::embed_migrations!("migrations")` and pass it to
`DatabaseBootstrap::with_migrations`; set `database.migrate = true` only when
that source is registered. File-based migration management is available through
`mads db migrate`, `mads db rollback`, and `mads db status`.

Use `mads::diesel` (or the direct `diesel` dependency shown above) for native
queries, schema macros, and Diesel traits. `Database::run` is the required
asynchronous boundary for synchronous PostgreSQL work.

## Validated write endpoint

Use a validated extractor for a request DTO. `Json<User>` below remains a
response wrapper; it is not a renamed request-validation alias.

```rust,ignore
use mads::prelude::*;

#[derive(serde::Deserialize, Input)]
struct CreateUser {
    #[validate(email, length(max = 254))]
    email: String,
    #[validate(length(min = 1, max = 120))]
    name: String,
}

#[routes(prefix = "/users")]
trait UserWrites {
    #[post("/")]
    async fn create(&self, request: ValidatedJson<CreateUser>) -> HttpResult<Json<User>>;
}
```

`ValidatedJson` rejects malformed or invalid input before the handler with the
source-aware 422 `validation_error` envelope. Native `Json<T>` remains the
public Axum escape hatch and does not invoke `Input`.
