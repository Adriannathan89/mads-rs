# MADS.rs — Clean Architecture CRUD User Example

> **Version scope:** The module declarations, standard
> `Mads::run::<AppModule>()` startup, CLI workflow, typed configuration,
> validation extractors, and REST errors shown here are the MADS
> `0.8.0` API. Trait bindings and `Inject<dyn Trait>` remain conceptual
> v1 APIs and are labelled where they appear.

## v0.8 CLI workflow

Use the standard entry point from the project root:

```bash
mads doctor
mads routes
mads run
# source/configuration changes are supervised during development
mads dev
```

For split Diesel schema declarations, place files such as
`src/schema/user.rs` and `src/schema/comment.rs` under `src/schema/`. The CLI
loads that directory recursively in lexical order. `mads db generate` creates
one automatically named, review-required diff; inspect `up.sql` and `down.sql`
before `mads db migrate`. Generation never applies migrations automatically.

## Goal

Dokumen ini menunjukkan bagaimana MADS dapat digunakan untuk User CRUD dengan **Clean Architecture**, sambil menjaga domain dan application rules tidak bergantung pada Axum, Diesel, atau detail HTTP/database.

Prinsip dependency:

```text
                 outer layers

HTTP / MADS delivery
        │
        ▼
Infrastructure adapters
        │
        ▼
Application use cases
        │
        ▼
Domain

                 inner layers
```

Source-code dependency selalu mengarah ke dalam.

MADS dipakai terutama sebagai **composition/runtime framework pada outer layer**, bukan sebagai dependency wajib domain.

---

## 1. Target API

```text
GET    /users
GET    /users/:id
POST   /users
PUT    /users/:id
DELETE /users/:id
```

Runtime implementation:

```text
HTTP
 ↓
User Routes
 ↓
User Use Cases
 ↓
UserRepositoryPort
 ↑
DieselUserRepository
 ↓
MADS Database
 ↓
Diesel
 ↓
PostgreSQL
```

Perhatikan inversion di repository boundary:

```text
Application owns UserRepositoryPort
Infrastructure implements it
```

---

## 2. Project Structure

```text
user-api/
├── Cargo.toml
├── mads.toml
├── migrations/
│   └── 0001_create_users/
│       ├── up.sql
│       └── down.sql
│
└── src/
    ├── main.rs
    ├── app.rs
    │
    ├── domain/
    │   └── user/
    │       ├── mod.rs
    │       ├── entity.rs
    │       └── error.rs
    │
    ├── application/
    │   └── user/
    │       ├── mod.rs
    │       ├── dto.rs
    │       ├── ports.rs
    │       └── usecase/
    │           ├── mod.rs
    │           ├── list_users_usecase.rs
    │           ├── get_user_usecase.rs
    │           ├── input_user_usecase.rs
    │           ├── update_user_usecase.rs
    │           └── delete_user_usecase.rs
    │
    ├── infrastructure/
    │   └── persistence/
    │       └── user/
    │           ├── mod.rs
    │           ├── schema.rs
    │           ├── model.rs
    │           └── diesel_repository.rs
    │
    └── delivery/
        └── http/
            └── user/
                ├── mod.rs
                ├── input.rs
                ├── response.rs
                └── controller.rs
```

Layer responsibilities:

```text
domain          → enterprise/domain rules
application     → use cases + ports
infrastructure  → Diesel implementation
HTTP delivery   → MADS routes / request-response mapping
app/main        → composition root
```

---

## 3. Cargo.toml

Current outer-layer dependency baseline:

```toml
[package]
name = "user-api"
version = "0.1.0"
edition = "2024"

[dependencies]
mads = { version = "=0.8.0", default-features = false, features = ["database", "http", "runtime-tokio"] }
serde = { version = "1", features = ["derive"] }
```

The MADS facade/common feature set provides normal Axum/Diesel integration for the standard path.

Strict projects may split each architecture layer into separate Rust crates later, but this example uses modules to keep the example readable.

---

## 4. Configuration

`mads.toml`:

```toml
[app]
name = "clean-user-api"
api_key = "${APP_API_KEY}"

[server]
host = "127.0.0.1"
port = 3000

[database]
url = "${DATABASE_URL}"
pool_size = 10
migrate = false
```

Environment:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost/clean_user_api"
export APP_API_KEY="replace-in-deployment"
```

Infrastructure configuration stays outside domain/application.

The composition edge can make application settings a startup requirement
without adding configuration concerns to the inner layers:

```rust
use mads::prelude::*;

#[derive(Configuration)]
#[config(prefix = "app")]
struct AppConfig {
    #[config(default = "clean-user-api")]
    name: String,
    api_key: Secret<String>,
}

#[provider]
fn app_config(config: Config) -> mads::core::Result<AppConfig> {
    Ok(config.parse()?)
}
```

Only a selected provider makes this parse mandatory. A failure reports the
full key and winning source without the value and stops before lifecycle or
listener binding. Infrastructure deliberately calls `api_key.expose()` only
where the credential must cross into a client library; normal formatting is
always `[REDACTED]`.

Standard v0.8 startup does not discover embedded migrations from the
`migrations/` directory. Readers who set `migrate = true` must use the
documented low-level builder registration path,
`builder.database_migrations(MIGRATIONS)?`, before `build().await`.

---

# DOMAIN LAYER

## 5. Domain Entity

`src/domain/user/entity.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: String,
}

impl User {
    pub fn new(id: i64, email: String, name: String) -> Result<Self, UserDomainError> {
        if name.trim().len() < 2 {
            return Err(UserDomainError::InvalidName);
        }

        if !email.contains('@') {
            return Err(UserDomainError::InvalidEmail);
        }

        Ok(Self { id, email, name })
    }
}
```

Tidak ada:

```text
mads
axum
diesel
HTTP status
Database
```

Domain entity dapat diuji sebagai pure Rust.

---

## 6. Domain Errors

`src/domain/user/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum UserDomainError {
    #[error("invalid user email")]
    InvalidEmail,

    #[error("invalid user name")]
    InvalidName,
}
```

Domain error tidak mengetahui `404`, `409`, atau response JSON.

---

## 7. Domain Module

`src/domain/user/mod.rs`:

```rust
mod entity;
mod error;

pub use entity::*;
pub use error::*;
```

---

# APPLICATION LAYER

## 8. Application DTOs

`src/application/user/dto.rs`:

```rust
#[derive(Debug, Clone)]
pub struct CreateUserCommand {
    pub email: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct UpdateUserCommand {
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListUsersQuery {
    pub page: u32,
    pub limit: u32,
}
```

DTO application tidak perlu menjadi HTTP `Json<T>`.

---

## 9. Repository Port

`src/application/user/ports.rs`:

```rust
use crate::domain::user::User;

#[async_trait::async_trait]
pub trait UserRepositoryPort: Send + Sync {
    async fn list(&self, page: u32, limit: u32) -> anyhow::Result<Vec<User>>;

    async fn find(&self, id: i64) -> anyhow::Result<Option<User>>;

    async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>>;

    async fn create(&self, email: String, name: String) -> anyhow::Result<User>;

    async fn update(
        &self,
        id: i64,
        email: Option<String>,
        name: Option<String>,
    ) -> anyhow::Result<Option<User>>;

    async fn delete(&self, id: i64) -> anyhow::Result<bool>;
}
```

> Catatan: exact async-trait strategy dapat mengikuti kemampuan Rust/MADS saat v1 diimplementasikan. Yang penting adalah arah dependency: interface dimiliki application layer.

---

## 10. Application Errors

Application layer dapat memiliki use-case errors sendiri:

```rust
#[derive(Debug, thiserror::Error)]
pub enum UserApplicationError {
    #[error("user not found")]
    NotFound,

    #[error("email already exists")]
    EmailAlreadyExists,

    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}
```

HTTP mapping dilakukan di delivery layer.

---

## 11. User Use Cases

The following types live in separate files under
`src/application/user/usecase/`:

```rust
use mads::prelude::*;

use crate::domain::user::User;

use super::{
    CreateUserCommand,
    ListUsersQuery,
    UpdateUserCommand,
    UserApplicationError,
    UserRepositoryPort,
};

#[service]
pub struct ListUsersUsecase {
    repository: Inject<dyn UserRepositoryPort>,
}

impl ListUsersUsecase {
    pub async fn execute(
        &self,
        query: ListUsersQuery,
    ) -> Result<Vec<User>, UserApplicationError> {
        self.repository
            .list(query.page, query.limit)
            .await
            .map_err(Into::into)
    }
}

#[service]
pub struct GetUserUsecase {
    repository: Inject<dyn UserRepositoryPort>,
}

impl GetUserUsecase {
    pub async fn execute(&self, id: i64) -> Result<User, UserApplicationError> {
        self.repository
            .find(id)
            .await?
            .ok_or(UserApplicationError::NotFound)
    }
}

#[service]
pub struct InputUserUsecase {
    repository: Inject<dyn UserRepositoryPort>,
}

impl InputUserUsecase {
    pub async fn execute(
        &self,
        command: CreateUserCommand,
    ) -> Result<User, UserApplicationError> {
        if self
            .repository
            .find_by_email(&command.email)
            .await?
            .is_some()
        {
            return Err(UserApplicationError::EmailAlreadyExists);
        }

        self.repository
            .create(command.email, command.name)
            .await
            .map_err(Into::into)
    }
}

#[service]
pub struct UpdateUserUsecase {
    repository: Inject<dyn UserRepositoryPort>,
}

impl UpdateUserUsecase {
    pub async fn execute(
        &self,
        id: i64,
        command: UpdateUserCommand,
    ) -> Result<User, UserApplicationError> {
        if let Some(email) = &command.email {
            if let Some(existing) = self.repository.find_by_email(email).await? {
                if existing.id != id {
                    return Err(UserApplicationError::EmailAlreadyExists);
                }
            }
        }

        self.repository
            .update(id, command.email, command.name)
            .await?
            .ok_or(UserApplicationError::NotFound)
    }
}

#[service]
pub struct DeleteUserUsecase {
    repository: Inject<dyn UserRepositoryPort>,
}

impl DeleteUserUsecase {
    pub async fn execute(&self, id: i64) -> Result<(), UserApplicationError> {
        if !self.repository.delete(id).await? {
            return Err(UserApplicationError::NotFound);
        }

        Ok(())
    }
}
```

> **Conceptual v1:** `Inject<dyn UserRepositoryPort>` illustrates the planned
> trait-binding API. MADS v0.8 does not ship trait bindings or
> `Inject<dyn Trait>`.

Each use case has one public application operation named `execute`. This keeps
the HTTP controller independent from repository methods and prevents it from
becoming a second application-service layer.

The application layer uses MADS only for composition metadata (`#[service]`
and `Inject`). It still contains no Axum, Diesel, HTTP-status, or database-pool
API. Projects requiring a completely framework-independent application crate
can replace these annotations with outer-layer provider functions.

---

## 12. Application Module

`src/application/user/mod.rs`:

```rust
mod dto;
mod ports;
mod usecase;

pub use dto::*;
pub use ports::*;
pub use usecase::*;
```

---

# INFRASTRUCTURE LAYER

## 13. Diesel Schema

`src/infrastructure/persistence/user/schema.rs`:

```rust
diesel::table! {
    users (id) {
        id -> Int8,
        email -> Varchar,
        name -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}
```

---

## 14. Persistence Model

Persistence representation may be different from domain representation.

`src/infrastructure/persistence/user/model.rs`:

```rust
use super::schema::users;
use crate::domain::user::User;

#[derive(Queryable, Selectable)]
#[diesel(table_name = users)]
pub struct UserRow {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            email: row.email,
            name: row.name,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUserRow {
    pub email: String,
    pub name: String,
}

#[derive(AsChangeset)]
#[diesel(table_name = users)]
pub struct UserChangeset {
    pub email: Option<String>,
    pub name: Option<String>,
}
```

Clean Architecture tidak memaksa domain entity menggunakan Diesel derives.

---

## 15. Diesel Repository Adapter

`src/infrastructure/persistence/user/diesel_repository.rs`:

```rust
use diesel::prelude::*;
use diesel::OptionalExtension;
use mads::prelude::*;

use crate::{
    application::user::UserRepositoryPort,
    domain::user::User,
};

use super::{
    model::{NewUserRow, UserChangeset, UserRow},
    schema::users,
};

#[repository(as = UserRepositoryPort)]
pub struct DieselUserRepository {
    db: Database,
}

#[async_trait::async_trait]
impl UserRepositoryPort for DieselUserRepository {
    async fn list(&self, page: u32, limit: u32) -> anyhow::Result<Vec<User>> {
        let offset = ((page - 1) * limit) as i64;
        let limit = limit as i64;

        let rows = self.db
            .run(move |conn| {
                users::table
                    .order(users::id.asc())
                    .offset(offset)
                    .limit(limit)
                    .select(UserRow::as_select())
                    .load::<UserRow>(conn)
            })
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find(&self, id: i64) -> anyhow::Result<Option<User>> {
        let row = self.db
            .run(move |conn| {
                users::table
                    .find(id)
                    .select(UserRow::as_select())
                    .first::<UserRow>(conn)
                    .optional()
            })
            .await?;

        Ok(row.map(Into::into))
    }

    async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>> {
        let email = email.to_owned();

        let row = self.db
            .run(move |conn| {
                users::table
                    .filter(users::email.eq(email))
                    .select(UserRow::as_select())
                    .first::<UserRow>(conn)
                    .optional()
            })
            .await?;

        Ok(row.map(Into::into))
    }

    async fn create(&self, email: String, name: String) -> anyhow::Result<User> {
        let row = self.db
            .run(move |conn| {
                diesel::insert_into(users::table)
                    .values(NewUserRow { email, name })
                    .returning(UserRow::as_returning())
                    .get_result::<UserRow>(conn)
            })
            .await?;

        Ok(row.into())
    }

    async fn update(
        &self,
        id: i64,
        email: Option<String>,
        name: Option<String>,
    ) -> anyhow::Result<Option<User>> {
        let row = self.db
            .run(move |conn| {
                diesel::update(users::table.find(id))
                    .set(UserChangeset { email, name })
                    .returning(UserRow::as_returning())
                    .get_result::<UserRow>(conn)
                    .optional()
            })
            .await?;

        Ok(row.map(Into::into))
    }

    async fn delete(&self, id: i64) -> anyhow::Result<bool> {
        let affected = self.db
            .run(move |conn| {
                diesel::delete(users::table.find(id)).execute(conn)
            })
            .await?;

        Ok(affected > 0)
    }
}
```

> **Conceptual v1:** The exact `#[repository(as = ...)]` syntax is a planned
> trait-binding API: it means that this infrastructure adapter satisfies the
> application-owned repository port. It is not part of v0.8.

MADS can therefore resolve:

```text
UserRepositoryPort
       ↑
DieselUserRepository
       ↓
Database
```

without the application layer importing Diesel.

---

## 16. Infrastructure Module

`src/infrastructure/persistence/user/mod.rs`:

```rust
mod diesel_repository;
mod model;
mod schema;

pub use diesel_repository::*;
```

---

# DELIVERY LAYER

## 17. HTTP Inputs

HTTP request models belong in delivery because validation/deserialization can be transport-specific.

The `Input` derive and validated extractor family in this section are the
current v0.8 request boundary. Native Axum extractors remain available when an
application intentionally owns validation itself.

`src/delivery/http/user/input.rs`:

```rust
use mads::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, Input)]
pub struct CreateUserRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 2, max = 120))]
    pub name: String,
}

#[derive(Debug, Deserialize, Input)]
pub struct UpdateUserRequest {
    #[validate(email)]
    pub email: Option<String>,

    #[validate(length(min = 2, max = 120))]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Input)]
pub struct ListUsersRequest {
    #[validate(range(min = 1))]
    pub page: Option<u32>,

    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,
}
```

Mapping to application command happens at the boundary.

---

## 18. HTTP Response DTO

Optional transport DTO:

```rust
use serde::Serialize;

use crate::domain::user::User;

#[derive(Serialize)]
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
```

Domain entity does not need `Serialize` solely because HTTP needs JSON.

---

## 19. Automatic Use Case Injection

No provider functions are required for the use cases. Their fields are enough
for MADS to derive the dependencies:

```text
ListUsersUsecase   ─┐
GetUserUsecase    ─┤
InputUserUsecase  ─┼─→ UserRepositoryPort
UpdateUserUsecase ─┤       ↑
DeleteUserUsecase ─┘       │ implemented by
                       DieselUserRepository
```

`#[repository(as = UserRepositoryPort)]` supplies the port binding and each
`#[service]` field requests it through `Inject<dyn UserRepositoryPort>`. MADS
constructs one application-scoped repository adapter and shares its managed
handle across all five use cases.

Trait bindings are a target capability beyond the concrete-type-only v0.2
graph. The same field-injection model already works in v0.2 when the field is a
concrete managed type such as `DieselUserRepository`.

---

## 20. HTTP Error Mapping

Delivery maps application semantics to HTTP:

```rust
fn map_user_error(error: UserApplicationError) -> HttpError {
    match error {
        UserApplicationError::NotFound => NotFound::new("user not found").into(),
        UserApplicationError::EmailAlreadyExists => {
            Conflict::new("email already exists").into()
        }
        UserApplicationError::Unexpected(error) => InternalError::new(error).into(),
    }
}
```

This keeps:

```text
404 / 409 → delivery concern
```

rather than domain concern.

---

## 21. Controller and Route Contract

`src/delivery/http/user/controller.rs`:

```rust
use mads::prelude::*;

use crate::application::user::{
    CreateUserCommand,
    DeleteUserUsecase,
    GetUserUsecase,
    InputUserUsecase,
    ListUsersQuery,
    ListUsersUsecase,
    UpdateUserCommand,
    UpdateUserUsecase,
};

use super::{
    CreateUserRequest,
    ListUsersRequest,
    UpdateUserRequest,
    UserResponse,
};

#[routes(prefix = "/users")]
pub trait UserRoutes {
    #[get("/")]
    async fn list_users(
        &self,
        query: ValidatedQuery<ListUsersRequest>,
    ) -> HttpResult<Json<Vec<UserResponse>>>;

    #[get("/:id")]
    async fn get_user(
        &self,
        id: ValidatedPath<i64>,
    ) -> HttpResult<Json<UserResponse>>;

    #[post("/")]
    async fn create_user(
        &self,
        request: ValidatedJson<CreateUserRequest>,
    ) -> HttpResult<Created<Json<UserResponse>>>;

    #[put("/:id")]
    async fn update_user(
        &self,
        id: ValidatedPath<i64>,
        request: ValidatedJson<UpdateUserRequest>,
    ) -> HttpResult<Json<UserResponse>>;

    #[delete("/:id")]
    async fn delete_user(
        &self,
        id: ValidatedPath<i64>,
    ) -> HttpResult<NoContent>;
}

#[controller(routes = [UserRoutes])]
pub struct UserController {
    list_users_usecase: ListUsersUsecase,
    get_user_usecase: GetUserUsecase,
    input_user_usecase: InputUserUsecase,
    update_user_usecase: UpdateUserUsecase,
    delete_user_usecase: DeleteUserUsecase,
}

impl UserRoutes for UserController {
    async fn list_users(
        &self,
        query: ValidatedQuery<ListUsersRequest>,
    ) -> HttpResult<Json<Vec<UserResponse>>> {
        let query = query.into_inner();
        let users = self.list_users_usecase
            .execute(ListUsersQuery {
                page: query.page.unwrap_or(1),
                limit: query.limit.unwrap_or(20),
            })
            .await
            .map_err(map_user_error)?;

        Ok(Json(users.into_iter().map(Into::into).collect()))
    }

    async fn get_user(
        &self,
        id: ValidatedPath<i64>,
    ) -> HttpResult<Json<UserResponse>> {
        let user = self.get_user_usecase
            .execute(id.into_inner())
            .await
            .map_err(map_user_error)?;
        Ok(Json(user.into()))
    }

    async fn create_user(
        &self,
        request: ValidatedJson<CreateUserRequest>,
    ) -> HttpResult<Created<Json<UserResponse>>> {
        let request = request.into_inner();
        let user = self.input_user_usecase
            .execute(CreateUserCommand {
                email: request.email,
                name: request.name,
            })
            .await
            .map_err(map_user_error)?;

        Ok(Created(Json(user.into())))
    }

    async fn update_user(
        &self,
        id: ValidatedPath<i64>,
        request: ValidatedJson<UpdateUserRequest>,
    ) -> HttpResult<Json<UserResponse>> {
        let id = id.into_inner();
        let request = request.into_inner();
        let user = self.update_user_usecase
            .execute(
                id,
                UpdateUserCommand {
                    email: request.email,
                    name: request.name,
                },
            )
            .await
            .map_err(map_user_error)?;

        Ok(Json(user.into()))
    }

    async fn delete_user(
        &self,
        id: ValidatedPath<i64>,
    ) -> HttpResult<NoContent> {
        self.delete_user_usecase
            .execute(id.into_inner())
            .await
            .map_err(map_user_error)?;
        Ok(NoContent)
    }
}
```

`#[routes]` owns the HTTP contract. `#[controller]` connects the concrete
controller to that contract, and normal Rust trait checking ensures that all
five endpoints are implemented with matching signatures. One controller may
implement more than one route trait.

The use cases are injected once into `UserController`. Route methods therefore
receive only request extractors and delegate through
`self.<operation>_usecase.execute(...)`.

These attributes are available as compile-time contracts with deterministic,
framework-neutral route metadata in the foundation. They validate the
trait/controller relationship and expose controller dependencies to the
existing provider catalog. Runtime route registration, extractors, and Axum
adapters remain part of the v0.3 HTTP milestone.

HTTP handlers are intentionally thin:

```text
extract request
  ↓
map to command/query
  ↓
execute use case
  ↓
map result/error to HTTP
```

Business rules do not live in the controller.

---

## 22. HTTP Module

`src/delivery/http/user/mod.rs`:

```rust
use mads::prelude::*;

mod input;
mod response;
mod controller;

pub use input::*;
pub use response::*;
pub use controller::*;

#[module]
pub struct UserHttpModule;
```

The `/users` prefix belongs to the `UserRoutes` contract. Route registration is
derived from the controller and route metadata rather than a manual list.

---

# COMPOSITION ROOT

## 23. App Module

`src/app.rs`:

```rust
use mads::prelude::*;

use crate::delivery::http::user::UserHttpModule;

#[module(imports = [UserHttpModule])]
pub struct AppModule;
```

The composition root is allowed to know outer-layer details.

MADS resolves approximately:

```text
UserHttpModule
  ↓
UserController
  ↓
ListUsersUsecase / GetUserUsecase / InputUserUsecase
UpdateUserUsecase / DeleteUserUsecase
  ↓
UserRepositoryPort
  ↓
DieselUserRepository
  ↓
Database
  ↓
DieselDatabaseAutoConfiguration
```

The application use cases know the MADS injection marker, but their business
logic depends only on `UserRepositoryPort`, never on `DieselUserRepository`.

---

## 24. Main

`src/main.rs`:

```rust
use mads::prelude::*;

mod app;
mod application;
mod delivery;
mod domain;
mod infrastructure;

use app::AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
```

No standard-path bootstrap for:

```text
Tokio
Axum Router
AppState
Arc
Diesel pool
repository construction
service construction
route list
```

---

## 25. Final Dependency Diagram

At compile/source level:

```text
┌───────────────────────────────────┐
│              Domain               │
│ User / domain rules / errors      │
└─────────────────▲─────────────────┘
                  │
┌─────────────────┴─────────────────┐
│            Application            │
│ Commands / Use Cases / Ports      │
└────────────▲──────────────▲───────┘
             │              │ implements
             │              │
┌────────────┴───────┐  ┌───┴─────────────────────┐
│   HTTP Delivery    │  │     Infrastructure      │
│ MADS route macros  │  │ DieselUserRepository    │
└────────────┬───────┘  └───────────┬─────────────┘
             │                      │
             └──────────┬───────────┘
                        ▼
                 MADS Composition
```

At runtime:

```text
HTTP Request
    ↓
MADS / Axum Route
    ↓
UserController
    ↓
Focused Usecase.execute(...)
    ↓
UserRepositoryPort
    ↓
DieselUserRepository
    ↓
Database
    ↓
Diesel
    ↓
PostgreSQL
```

---

## 26. Why This Is Cleaner Than the Simple CRUD Example

Simple MADS architecture may intentionally be:

```text
Route
 ↓
UserController
 ↓
UserService
 ↓
UserRepository
 ↓
Database
```

That is appropriate for many applications.

Clean Architecture adds stronger boundaries:

```text
Route
 ↓
UserController
 ↓
Application Use Case
 ↓
Repository Port
 ↑
Infrastructure Adapter
```

Trade-off:

```text
Simple architecture
  + less code
  + faster CRUD development
  - application layer can know concrete repository types

Clean Architecture
  + business rules independent from Axum/Diesel and persistence implementations
  + MADS annotations limited to composition metadata
  + persistence replaceable behind ports
  + easier isolated use-case tests
  - more types and mapping code
```

MADS should support both instead of forcing one style.

---

## 27. Focused Application Test

The MADS test graph can replace the repository-port implementation while
testing only the selected application use case.

```rust
#[repository(as = UserRepositoryPort)]
struct FakeUserRepository {
    // in-memory data
}

#[async_trait::async_trait]
impl UserRepositoryPort for FakeUserRepository {
    // fake methods
}

#[mads::test]
async fn creating_duplicate_email_fails(usecase: InputUserUsecase) {
    let result = usecase
        .execute(CreateUserCommand {
            email: "john@example.com".into(),
            name: "John".into(),
        })
        .await;

    assert!(matches!(
        result,
        Err(UserApplicationError::EmailAlreadyExists)
    ));
}
```

No Diesel database or HTTP server is required. Projects that require completely
framework-free use-case unit tests can keep the use cases unannotated and bind
them with outer-layer `#[provider]` functions instead.

---

## 28. MADS Integration Test

Outer integration can still use MADS test graph:

```rust
#[mads::test]
async fn get_user_endpoint(client: TestClient) {
    client
        .get("/users/1")
        .send()
        .await
        .assert_status(200);
}
```

Use both test levels:

```text
Domain tests             → fast, pure
Application tests        → focused MADS graph with fake ports
Infrastructure tests     → Diesel integration
HTTP/MADS tests          → full application graph
```

---

## 29. Architecture Rule for MADS Projects

A useful rule is:

> **MADS may compose the application, but the domain should not require MADS to exist.**

For the annotation-based Clean Architecture style used in this document:

```text
domain          must not import mads/axum/diesel
application     may import mads injection metadata, but not axum/diesel
infrastructure  may import diesel + mads Database integration
delivery        may import mads/common + HTTP concepts
composition     may know all outer implementations
```

This preserves dependency inversion while benefiting from MADS auto-wiring.
The stricter variant keeps MADS out of `application` too and uses composition
providers, at the cost of additional wiring code.

---

## 30. Final Clean Architecture Experience

Developer explicitly writes the architecture that matters:

```text
Each User use case depends on UserRepositoryPort
DieselUserRepository implements UserRepositoryPort
UserController implements the UserRoutes contract
AppModule imports User HTTP module
```

MADS handles the infrastructure ceremony:

```text
provider discovery
construction order
shared ownership
Database auto-configuration
Diesel pool
Axum router
route registry
startup
validated request extraction
framework diagnostics
```

The final goal is not to hide architecture.

It is to remove the plumbing around architecture.

> **Clean Architecture defines the boundaries. MADS wires the runtime.**
