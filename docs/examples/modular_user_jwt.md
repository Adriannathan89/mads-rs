# Modular PostgreSQL user API with JWT-protected updates

This v0.8.0 example implements a small user feature with PostgreSQL persistence
and separate Rust modules. It provides:

| Method | Path | Authentication | Purpose |
| --- | --- | --- | --- |
| `POST` | `/users/` | Public | Create a user |
| `POST` | `/users/login` | Public | Return a JWT access token |
| `PUT` | `/users/:id` | Bearer JWT | Update the authenticated user |

Passwords are hashed with Argon2id before they reach PostgreSQL. The API never
returns the stored password hash.

## Project layout

```text
.
|-- Cargo.toml
|-- mads.toml
|-- migrations/
|   `-- 202608290001_create_users/
|       |-- up.sql
|       `-- down.sql
`-- src/
    |-- main.rs
    |-- app.rs
    `-- user/
        |-- mod.rs
        |-- schema.rs
        |-- model.rs
        |-- repository.rs
        |-- service.rs
        |-- auth.rs
        `-- http/
            |-- mod.rs
            |-- input.rs
            |-- response.rs
            |-- routes.rs
            `-- controller.rs
```

Only `AppModule` is passed to `Mads::run`. Importing `UserModule` makes its
repository, services, Passport strategy, and controller reachable. No manual
route or provider list is required.

## Dependencies and configuration

`Cargo.toml`:

```toml
[package]
name = "modular-user-api"
version = "0.1.0"
edition = "2024"

[dependencies]
mads = { version = "0.8.0", default-features = false, features = ["database", "http", "jwt", "runtime-tokio"] }
argon2 = "0.5"
rand_core = { version = "0.6", features = ["getrandom"] }
serde = { version = "1", features = ["derive"] }
```

`mads.toml`:

```toml
[database]
url = "${DATABASE_URL}"
pool_size = 10
migrate = false

[passport]
secret = "${JWT_SECRET}"
algorithms = ["HS256"]
```

`.env`:

```dotenv
DATABASE_URL=postgres://postgres:postgres@127.0.0.1/modular_user_api
JWT_SECRET=replace-this-demo-secret-with-at-least-32-bytes
```

The standard run path loads optional `.env`, `mads.toml`, and `MADS_`
environment overrides. Keep real database credentials and signing secrets out
of version control.

`database.migrate` stays `false` because the standard v0.8 run path does not
auto-discover a `migrations/` directory. Apply the file-based migration before
starting the server:

```bash
mads db migrate
```

Embedded startup migrations remain available through the low-level builder,
but are not needed by this example.

## Migration

`migrations/202608290001_create_users/up.sql`:

```sql
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    email VARCHAR(320) NOT NULL UNIQUE,
    name VARCHAR(120) NOT NULL,
    password_hash TEXT NOT NULL
);
```

`migrations/202608290001_create_users/down.sql`:

```sql
DROP TABLE users;
```

## Diesel schema

`src/user/schema.rs`:

```rust
use mads::diesel;

diesel::table! {
    users (id) {
        id -> Int8,
        email -> Varchar,
        name -> Varchar,
        password_hash -> Text,
    }
}
```

## Persistence models

`src/user/model.rs`:

```rust
use mads::diesel::{self, prelude::*};

use super::schema::users;

#[derive(Clone, Debug, diesel::Queryable, diesel::Selectable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub password_hash: String,
}

#[derive(diesel::Insertable)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub email: String,
    pub name: String,
    pub password_hash: String,
}

#[derive(diesel::AsChangeset)]
#[diesel(table_name = users)]
pub struct UserNameChangeset {
    pub name: String,
}
```

`User` is a persistence model, not an HTTP response. Keeping the
`password_hash` field here allows login verification while the response mapper
selects only safe fields.

## PostgreSQL repository

`src/user/repository.rs`:

```rust
use mads::{
    diesel::{self, OptionalExtension, prelude::*},
    prelude::*,
};

use super::{NewUser, User, UserNameChangeset, schema::users};

#[repository]
pub struct UserRepository {
    database: Database,
}

impl UserRepository {
    pub async fn create(
        &self,
        email: String,
        name: String,
        password_hash: String,
    ) -> DatabaseResult<User> {
        self.database
            .run(move |connection| {
                diesel::insert_into(users::table)
                    .values(NewUser {
                        email,
                        name,
                        password_hash,
                    })
                    .returning(User::as_returning())
                    .get_result(connection)
            })
            .await
    }

    pub async fn find(&self, id: i64) -> DatabaseResult<Option<User>> {
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

    pub async fn find_by_email(&self, email: String) -> DatabaseResult<Option<User>> {
        self.database
            .run(move |connection| {
                users::table
                    .filter(users::email.eq(email))
                    .select(User::as_select())
                    .first(connection)
                    .optional()
            })
            .await
    }

    pub async fn update_name(&self, id: i64, name: String) -> DatabaseResult<Option<User>> {
        self.database
            .run(move |connection| {
                diesel::update(users::table.find(id))
                    .set(UserNameChangeset { name })
                    .returning(User::as_returning())
                    .get_result(connection)
                    .optional()
            })
            .await
    }
}
```

`#[repository]` makes `UserRepository` a managed provider. Its `Database`
dependency activates MADS's official PostgreSQL auto-configuration. Every
synchronous Diesel query runs through `Database::run`, outside the async
executor's worker threads.

## User service and token issuance

`src/user/service.rs`:

```rust
use std::time::Duration;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::SaltString,
};
use mads::prelude::*;
use rand_core::OsRng;

use super::{User, UserClaims, UserRepository};

#[derive(Debug)]
pub enum CreateUserError {
    InvalidInput,
    EmailTaken,
    Password(argon2::password_hash::Error),
    Database(DatabaseError),
}

#[derive(Debug)]
pub enum LoginError {
    InvalidCredentials,
    Token(JwtError),
    Database(DatabaseError),
}

#[derive(Debug)]
pub enum UpdateUserError {
    InvalidInput,
    NotFound,
    Database(DatabaseError),
}

#[service]
pub struct UserService {
    users: UserRepository,
    jwt: JwtService,
}

impl UserService {
    pub async fn create(
        &self,
        email: String,
        name: String,
        password: String,
    ) -> Result<User, CreateUserError> {
        if email.trim().is_empty() || name.trim().is_empty() || password.is_empty() {
            return Err(CreateUserError::InvalidInput);
        }

        if self
            .users
            .find_by_email(email.clone())
            .await
            .map_err(CreateUserError::Database)?
            .is_some()
        {
            return Err(CreateUserError::EmailTaken);
        }

        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(CreateUserError::Password)?
            .to_string();

        self.users
            .create(email, name, password_hash)
            .await
            .map_err(CreateUserError::Database)
    }

    pub async fn login(&self, email: String, password: String) -> Result<String, LoginError> {
        let user = self
            .users
            .find_by_email(email)
            .await
            .map_err(LoginError::Database)?
            .ok_or(LoginError::InvalidCredentials)?;

        let password_hash =
            PasswordHash::new(&user.password_hash).map_err(|_| LoginError::InvalidCredentials)?;

        Argon2::default()
            .verify_password(password.as_bytes(), &password_hash)
            .map_err(|_| LoginError::InvalidCredentials)?;

        self.jwt
            .sign(
                UserClaims { user_id: user.id },
                JwtSignOptions::access(Duration::from_secs(15 * 60))
                    .subject(user.id.to_string()),
            )
            .map_err(LoginError::Token)
    }

    pub async fn find(&self, id: i64) -> DatabaseResult<Option<User>> {
        self.users.find(id).await
    }

    pub async fn update_name(
        &self,
        id: i64,
        name: String,
    ) -> Result<User, UpdateUserError> {
        if name.trim().is_empty() {
            return Err(UpdateUserError::InvalidInput);
        }

        self.users
            .update_name(id, name)
            .await
            .map_err(UpdateUserError::Database)?
            .ok_or(UpdateUserError::NotFound)
    }
}
```

The email pre-check produces a friendly conflict response. The database
`UNIQUE` constraint remains the final concurrency-safe guarantee; a production
error mapper can additionally translate PostgreSQL unique violations caused by
racing requests into the same conflict response.

For a high-traffic service, run Argon2 work in a dedicated blocking task or
password service because password hashing is intentionally CPU-intensive.

## JWT claims, principal, and strategy

`src/user/auth.rs`:

```rust
use mads::prelude::*;

use super::UserService;

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UserClaims {
    pub user_id: i64,
}

#[derive(Debug)]
pub struct UserPrincipal {
    pub user_id: i64,
}

impl PassportPrincipal for UserPrincipal {
    fn has_role(&self, _role: &str) -> bool {
        false
    }

    fn has_permission(&self, _permission: &str) -> bool {
        false
    }
}

#[service]
pub struct UserJwtStrategy {
    users: UserService,
}

#[passport_strategy(name = "jwt")]
impl PassportStrategy for UserJwtStrategy {
    type Claims = UserClaims;
    type Principal = UserPrincipal;

    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;

    async fn validate(
        &self,
        _context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        let user = self
            .users
            .find(claims.custom.user_id)
            .await
            .map_err(PassportError::internal)?
            .ok_or_else(PassportError::reject)?;

        Ok(UserPrincipal { user_id: user.id })
    }
}
```

MADS verifies the bearer token before calling `validate`. The strategy checks
PostgreSQL to ensure that the claimed user still exists, then creates the typed
principal injected into the update handler.

## HTTP request and response types

`src/user/http/input.rs`:

```rust
#[derive(serde::Deserialize, Input)]
pub struct CreateUserRequest {
    #[validate(email, length(max = 254))]
    pub email: String,
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(serde::Deserialize, Input)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1))]
    pub password: String,
}

#[derive(serde::Deserialize, Input)]
pub struct UpdateUserRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
}
```

`src/user/http/response.rs`:

```rust
use crate::user::User;

#[derive(serde::Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub email: String,
    pub name: String,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            name: user.name,
        }
    }
}

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: &'static str,
}
```

## Route contract

`src/user/http/routes.rs`:

```rust
use mads::prelude::*;

use crate::user::UserPrincipal;

use super::{CreateUserRequest, LoginRequest, LoginResponse, UpdateUserRequest, UserResponse};

#[routes(prefix = "/users")]
pub trait UserRoutes {
    #[post("/")]
    async fn create_user(
        &self,
        request: ValidatedJson<CreateUserRequest>,
    ) -> HttpResult<Created<Json<UserResponse>>>;

    #[post("/login")]
    async fn login(
        &self,
        request: ValidatedJson<LoginRequest>,
    ) -> HttpResult<Json<LoginResponse>>;

    #[put("/:id")]
    #[guard(strategy = "jwt", principal = UserPrincipal, source = bearer)]
    async fn update_user(
        &self,
        id: Path<i64>,
        principal: Authenticated<UserPrincipal>,
        request: ValidatedJson<UpdateUserRequest>,
    ) -> HttpResult<Json<UserResponse>>;
}
```

`ValidatedJson<T>` is the MADS request boundary: it deserializes with Serde,
runs `Input`, and returns ordered source-aware 422 issues before the handler.
Keep this body-consuming extractor after `Path<T>` and `Authenticated<T>`.
The public native `Json<T>` re-export still deserializes without automatic
validation; it remains available when a handler deliberately owns that policy.

Create and login are public. The method-level guard requires
`Authorization: Bearer <token>` only for update.

## Controller

`src/user/http/controller.rs`:

```rust
use mads::prelude::*;

use crate::user::{CreateUserError, LoginError, UpdateUserError, UserPrincipal, UserService};

use super::{CreateUserRequest, LoginRequest, LoginResponse, UpdateUserRequest, UserResponse, UserRoutes};

#[controller(routes = [UserRoutes])]
pub struct UserController {
    users: UserService,
}

impl UserRoutes for UserController {
    async fn create_user(
        &self,
        request: ValidatedJson<CreateUserRequest>,
    ) -> HttpResult<Created<Json<UserResponse>>> {
        let request = request.into_inner();
        let user = self
            .users
            .create(request.email, request.name, request.password)
            .await
            .map_err(|error| match error {
                CreateUserError::InvalidInput => {
                    HttpError::bad_request("email, name, and password are required")
                }
                CreateUserError::EmailTaken => HttpError::conflict("email is already registered"),
                CreateUserError::Password(error) => HttpError::internal(error),
                CreateUserError::Database(error) => HttpError::internal(error),
            })?;

        Ok(Created(Json(user.into())))
    }

    async fn login(
        &self,
        request: ValidatedJson<LoginRequest>,
    ) -> HttpResult<Json<LoginResponse>> {
        let request = request.into_inner();
        let access_token = self
            .users
            .login(request.email, request.password)
            .await
            .map_err(|error| match error {
                LoginError::InvalidCredentials => HttpError::bad_request("invalid credentials"),
                LoginError::Token(error) => HttpError::internal(error),
                LoginError::Database(error) => HttpError::internal(error),
            })?;

        Ok(Json(LoginResponse {
            access_token,
            token_type: "Bearer",
        }))
    }

    async fn update_user(
        &self,
        id: Path<i64>,
        principal: Authenticated<UserPrincipal>,
        request: ValidatedJson<UpdateUserRequest>,
    ) -> HttpResult<Json<UserResponse>> {
        if *id != principal.user_id {
            return Err(HttpError::not_found("user was not found"));
        }

        let request = request.into_inner();
        let user = self
            .users
            .update_name(*id, request.name)
            .await
            .map_err(|error| match error {
                UpdateUserError::InvalidInput => HttpError::bad_request("name is required"),
                UpdateUserError::NotFound => HttpError::not_found("user was not found"),
                UpdateUserError::Database(error) => HttpError::internal(error),
            })?;

        Ok(Json(user.into()))
    }
}
```

The controller compares the path ID with the authenticated principal. Returning
the same not-found response for another user's ID avoids disclosing whether
that account exists. `HttpError::internal` retains database and hashing errors
for server-side inspection without returning their details to clients.

## Module files

`src/user/http/mod.rs`:

```rust
mod controller;
mod input;
mod response;
mod routes;

pub use controller::*;
pub use input::*;
pub use response::*;
pub use routes::*;
```

`src/user/mod.rs`:

```rust
use mads::prelude::*;

mod auth;
mod http;
mod model;
mod repository;
mod schema;
mod service;

pub use auth::*;
pub use http::*;
pub use model::*;
pub use repository::*;
pub use service::*;

#[module]
pub struct UserModule;
```

The `UserModule` definition is the registration boundary. The implementation
can remain split across normal Rust files without declaring a separate MADS
module for each file.

## Application module and entry point

`src/app.rs`:

```rust
use mads::prelude::*;

use crate::user::UserModule;

#[module(imports = [UserModule])]
pub struct AppModule;
```

`src/main.rs`:

```rust
use mads::prelude::*;

mod app;
mod user;

use app::AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
```

The resulting managed dependency path is:

```text
AppModule
`-- UserModule
    |-- UserController
    |   `-- UserService
    |       |-- UserRepository
    |       |   `-- Database
    |       `-- JwtService
    `-- UserJwtStrategy
        `-- UserService
```

The builder remains available for low-level customization, but normal startup
needs only `Mads::run::<AppModule>().await`.

## Try the API

Create the database, apply the migration, and start the application. Then
create a user:

```bash
curl -X POST http://127.0.0.1:3000/users/ \
  -H "content-type: application/json" \
  -d '{"email":"ada@example.com","name":"Ada","password":"demo-password"}'
```

Log in and copy the returned `access_token`:

```bash
curl -X POST http://127.0.0.1:3000/users/login \
  -H "content-type: application/json" \
  -d '{"email":"ada@example.com","password":"demo-password"}'
```

Update the same user:

```bash
curl -X PUT http://127.0.0.1:3000/users/1 \
  -H "content-type: application/json" \
  -H "authorization: Bearer YOUR_ACCESS_TOKEN" \
  -d '{"name":"Ada Lovelace"}'
```

Missing, malformed, expired, or rejected bearer tokens are handled by the MADS
Passport guard before `update_user` runs.
