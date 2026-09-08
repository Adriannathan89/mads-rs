# MADS.rs — Final Idea v1

## Modular Application Development System for Rust

> **Declare what your application needs. MADS wires the rest.**

MADS.rs adalah opinionated, auto-configuring backend application framework untuk Rust. MADS berdiri di atas ekosistem yang sudah matang—terutama Axum/Tower/Tokio untuk HTTP/runtime dan Diesel untuk persistence—dan berfokus pada application structure, type-driven dependency wiring, conventions, diagnostics, serta developer experience.

Dokumen ini adalah baseline desain **MADS v1**, diperbarui untuk public surface
yang sudah tersedia pada `0.8.0-beta.1`. Bagian yang tetap ditandai target atau
future menjelaskan arah setelah v0.8.0, bukan API saat ini. Cache dan rate
limiting sengaja **tidak termasuk scope v1**. Keduanya
direncanakan sebagai capability opsional di `mads/extra` setelah fondasi v1
stabil.

---

## 1. Masalah yang Diselesaikan

Rust backend sudah memiliki primitive yang sangat baik, tetapi aplikasi biasa sering harus mengerti dan merakit terlalu banyak infrastructure plumbing sebelum business logic dapat ditulis:

```text
Tokio runtime
Axum Router / State
Tower layers
Arc/shared ownership
connection pool
repository construction
service construction
route registration
error mapping
configuration bootstrap
```

MADS mengubah common path menjadi:

```text
Module
  ↓
Repository + Service + Route
  ↓
type-driven dependency graph
  ↓
auto configuration
  ↓
run
```

Prinsip utama:

> **Reduce Rust ceremony, not Rust guarantees.**

---

## 2. Positioning

MADS bukan “NestJS dalam Rust” dan bukan sekadar kumpulan macro Axum.

Positioning final v1:

> **MADS is a type-driven, auto-configuring modular application framework for Rust, powered by Axum and Diesel.**

MADS menggabungkan:

- modular application architecture;
- Rust type-driven dependency graph;
- automatic service/repository wiring;
- Spring Boot-inspired auto-configuration;
- Diesel-first persistence;
- Axum HTTP runtime;
- framework-level diagnostics;
- progressive escape hatches ke Axum/Tower/Diesel.

Developer promise:

> **Write the application. MADS wires the infrastructure.**

---

## 3. Core Design Rule

MADS membedakan keputusan architecture dari infrastructure wiring.

```text
Architecture = explicit
Wiring       = automatic
```

Rule yang digunakan saat mendesain API:

```text
Can MADS infer this safely from Rust types/metadata?

YES → infer it
NO  → require an explicit declaration
```

Contoh:

```rust
#[repository]
pub struct UserRepository {
    db: Database,
}

#[service]
pub struct UserService {
    users: UserRepository,
}
```

MADS dapat menyimpulkan:

```text
UserService
  └── UserRepository
        └── Database
```

Developer tidak perlu menulis daftar provider yang mengulang informasi yang sudah ada di type.

---

## 4. Package Architecture

MADS v1 dibagi menjadi tiga boundary utama:

```text
mads/
├── core/
├── common/
└── extra/
```

Dependency direction:

```text
             ┌───────────┐
             │   extra   │   post-v1 optional capabilities
             └─────┬─────┘
                   │
                   ▼
┌───────────┐   ┌───────────┐
│  common   │ ─▶│   core    │
└───────────┘   └───────────┘
```

`core` tidak boleh bergantung pada `common` atau `extra`.

`extra` boleh menggunakan abstraction dari `core` dan, bila fiturnya HTTP-facing, metadata/integration dari `common`.

---

## 5. `mads/core`

`core` adalah application runtime dan graph engine MADS.

Tanggung jawab utama:

```text
Mads instance / builder
Application graph
Dependency graph
Provider registry
Module metadata
Lifecycle
Configuration contracts
Diagnostics foundation
Service construction
Repository construction
Provider construction
```

Macro/primitive utama:

```text
#[module]
#[service]
#[repository]
#[provider]
#[mads::main]
Mads
Result<T>
Config
Provider
```

Target penting: `core` tidak diasumsikan selalu HTTP. Di masa depan application graph yang sama dapat digunakan oleh worker, CLI command, scheduler, atau message consumer.

---

## 6. `mads/common`

`common` menyediakan **standard backend application experience** MADS.

V1 berisi dua capability standar.

### HTTP / Axum

```text
#[get]
#[post]
#[put]
#[patch]
#[delete]
#[routes]
#[controller]
route metadata
route registry
Axum adapter
request extraction
response mapping
standard HTTP errors
validation integration
```

Application-facing types:

```text
Json<T>
Path<T>
Query<T>
ValidatedJson<T>
ValidatedPath<T>
ValidatedQuery<T>
Header<T>
Request
Created<T>
NoContent
BadRequest
Unauthorized
Forbidden
NotFound
Conflict
ValidationError
InternalError
```

`Json<T>`, `Path<T>`, dan `Query<T>` tetap native Axum escape hatch tanpa
automatic `Input` validation. Wrapper `Validated*` adalah boundary opt-in yang
melakukan Serde deserialization lalu validasi sebelum handler dipanggil.

### Database / Diesel

Diesel adalah persistence foundation resmi MADS v1.

`common` menyediakan integration seperti:

```text
Database
DatabaseConfig
Diesel connection pool bootstrap
Migration runner
Database auto-configuration
Diesel error mapping
transaction foundation
```

MADS tidak membuat query language baru. Repository tetap dapat menggunakan Diesel DSL secara native.

---

## 7. `mads/extra`

`extra` adalah rumah untuk optional capabilities yang **bukan requirement framework v1**.

Target setelah v1:

```text
extra/
├── redis/
├── cache/
└── rate_limit/
```

Rencana prinsipnya:

```text
Redis            → reusable infrastructure capability
CacheService     → cache abstraction
#[cacheable]     → declarative route policy
#[cache_evict]   → cache invalidation policy
RateLimitService → rate-limit abstraction
#[rate_limit]    → declarative route policy
```

Namun semua ini **out of scope untuk MADS v1**. V1 tidak perlu membawa Redis, cache, maupun rate limiter agar core architecture dapat diselesaikan lebih dahulu.

---

## 8. Public Mental Model

MADS v1 menjaga public mental model tetap kecil:

```text
Application
Module
Service
Repository
Route
Config
Provider
Result
```

Contoh common path:

```rust
use mads::prelude::*;

#[repository]
pub struct UserRepository {
    db: Database,
}

#[service]
pub struct UserService {
    users: UserRepository,
}

#[routes(prefix = "/users")]
pub trait UserRoutes {
    #[get("/:id")]
    async fn get_user(&self, id: Path<i64>) -> Result<Json<User>>;
}

#[controller(routes = [UserRoutes])]
pub struct UserController {
    users: UserService,
}

impl UserRoutes for UserController {
    async fn get_user(&self, id: Path<i64>) -> Result<Json<User>> {
        Ok(Json(self.users.find(*id).await?))
    }
}
```

Internal graph:

```text
GET /users/:id
  └── UserController
        └── UserService
              └── UserRepository
                    └── Database
                          └── DieselPool
```

---

## 9. Module Model

Module adalah architecture boundary, bukan manual DI manifest. Sejak v0.6.0,
ownership berasal dari Rust namespace dan root menentukan reachable module
graph; module tidak memiliki HTTP path.

```rust
#[module]
pub struct UserModule;
```

Prefix HTTP dimiliki secara eksklusif oleh contract route:

```rust
#[routes(prefix = "/users")]
pub trait UserRoutes {}
```

Dependency lintas module memerlukan direct import dan plain `pub` pada item
yang dipakai. Tidak ada daftar `exports` terpisah, dan import tidak transitif.

Root module mendeskripsikan high-level application architecture:

```rust
#[module(imports = [UserModule, AuthModule])]
pub struct AppModule;
```

Rule:

```text
module relationship   = explicit
provider relationship = inferred
```

MADS tidak meminta:

```rust
services = [UserService]
repositories = [UserRepository]
routes = [get_user, create_user]
```

jika metadata tersebut sudah dapat ditentukan oleh framework.

---

## 10. Dependency Injection Model

MADS tidak ditujukan menjadi dynamic reflection container.

Preferred model:

```text
compile-time metadata
        ↓
dependency graph
        ↓
generated/deterministic construction plan
        ↓
runtime instances
```

Default lifecycle v1:

```text
application scoped / singleton
```

Request/transient scopes dapat dipertimbangkan setelah v1 tanpa mengubah mental model dasar.

Framework harus mendeteksi setidaknya:

```text
missing dependency
duplicate provider
ambiguous provider
dependency cycle
module cycle
private/non-exported dependency
invalid construction
```

---

## 11. Auto-Configuration

Auto-configuration MADS mengambil inspirasi dari Spring Boot tetapi harus tetap deterministic dan explainable.

Model:

```text
application requirement
        +
available capability
        +
configuration
        ↓
auto configuration
        ↓
provider/runtime infrastructure
```

Contoh:

```rust
#[repository]
struct UserRepository {
    db: Database,
}
```

plus:

```toml
[database]
url = "${DATABASE_URL}"
migrate = true
```

menghasilkan:

```text
Database required
      +
Diesel capability available
      +
database.url configured
      ↓
DieselDatabaseAutoConfiguration
      ↓
Database / DieselPool
```

---

## 12. Auto-Configuration Back-Off

Rule wajib:

> **A user-defined provider wins over a framework default.**

Contoh:

```rust
#[provider]
async fn database(config: CustomDatabaseConfig) -> Result<Database> {
    // custom implementation
}
```

Maka Diesel default tidak diaktifkan untuk capability tersebut.

```text
Custom Database provider detected
        ↓
Diesel auto-configuration backs off
```

Auto-configuration harus membantu, bukan mengunci developer.

---

## 13. Diesel-first Persistence

MADS v1 membawa Diesel sebagai ORM/persistence integration resmi.

Boundary:

```text
Application Repository
        ↓
MADS Database
        ↓
Diesel integration
        ↓
Diesel
        ↓
PostgreSQL / supported backend
```

Repository:

```rust
#[repository]
pub struct UserRepository {
    db: Database,
}
```

Native Diesel tetap tersedia:

```rust
self.db
    .run(move |conn| {
        users::table
            .find(id)
            .select(User::as_select())
            .first(conn)
            .optional()
    })
    .await
```

MADS menangani lifecycle/integration, bukan mengganti Diesel.

---

## 14. HTTP Route Model

Routes mendeskripsikan HTTP intent:

```rust
#[routes(prefix = "/users")]
trait UserRoutes {
    #[post("/")]
    async fn create_user(
        &self,
        body: ValidatedJson<CreateUser>,
    ) -> HttpResult<Created<Json<User>>>;
}

#[controller(routes = [UserRoutes])]
struct UserController {
    users: UserService,
}

impl UserRoutes for UserController {
    async fn create_user(
        &self,
        body: ValidatedJson<CreateUser>,
    ) -> HttpResult<Created<Json<User>>> {
        Ok(Created(Json(self.users.create(body.into_inner()).await?)))
    }
}
```

MADS mengklasifikasikan parameter melalui type metadata:

```text
ValidatedJson<CreateUser> → HTTP body + Input validation
ValidatedPath<i64>        → HTTP path + Input validation
ValidatedQuery<T>         → HTTP query + Input validation
Json/Path/Query            → native Axum extraction without Input
UserService                → controller/application dependency
```

Common path tidak memerlukan `State<AppState>` atau `Arc<UserService>`.

---

## 15. Validation and Errors (shipped in v0.8.0-beta.1)

Current input API:

```rust
#[derive(serde::Deserialize, Input)]
pub struct CreateUser {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 2, max = 120))]
    pub name: String,
}
```

Flow:

```text
request
  ↓
deserialize
  ↓
validate
  ↓
handler
```

`ValidatedJson`, `ValidatedQuery`, dan `ValidatedPath` menambahkan source
`body`, `query`, atau `path` ke ordered issues. Built-in validator adalah
`email`, Unicode code-point `length`, `nonempty`, numeric `range`, `positive`,
`negative`, `multiple_of`, `required`, dan recursive `nested`. Synchronous
`custom = path`, whole-value callback, serta manual `Input` implementation
menjadi extension points. Invalid input menghasilkan 422
`validation_error` tanpa memanggil handler. Native `Json`, `Query`, dan `Path`
tetap tidak menjalankan `Input`.

Framework memiliki standard application/HTTP errors untuk common path:

```text
BadRequest
Unauthorized
Forbidden
NotFound
Conflict
ValidationError
InternalError
```

Tetapi custom Axum response harus tetap mungkin digunakan.

Seluruh MADS-owned HTTP error memakai safe
`{ "error": { "code", "message" } }` envelope; validation menambahkan
source-aware `issues`. Passport rejected tetap mengirim
`WWW-Authenticate: Bearer`, sedangkan internal sources tidak diserialisasi.
Dengan feature `http + database`, `DatabaseResult<T>` dan native Diesel
`QueryResult<T>` dapat memilih `.into_http()`: typed not-found menjadi 404,
typed unique violation menjadi 409, dan error lain menjadi redacted 500. Tidak
ada blanket automatic database conversion.

Typed application configuration juga merupakan API core v0.8:

```rust
#[derive(Configuration)]
#[config(prefix = "app")]
struct AppConfig {
    host: String,
    #[config(default = 3000, validate(range(min = 1, max = 65535)))]
    port: u16,
    api_key: Secret<String>,
}

let app = config.parse::<AppConfig>()?;
```

`Config::parse` membaca `Config` yang sudah dimuat; derive tidak melakukan
global registration atau loading kedua. Selected provider membuat parsing
sebagai startup requirement. `Secret<T>` hanya dibuka melalui `expose` atau
`into_exposed`; `Display` dan `Debug` selalu `[REDACTED]`.

---

## 16. Startup Model

Target startup sequence v1:

```text
load configuration
      ↓
resolve environment
      ↓
read root module metadata
      ↓
build module graph
      ↓
build dependency graph
      ↓
resolve auto-configurations
      ↓
validate graph
      ↓
initialize Database / Diesel
      ↓
run migrations when enabled
      ↓
construct repositories
      ↓
construct services
      ↓
register Axum routes
      ↓
start server
```

Initialization order berasal dari graph, bukan urutan registration manual yang rapuh.

---

## 17. Diagnostics as a Product Feature

MADS harus menjelaskan failure pada level abstraction MADS.

Contoh:

```text
error[MADS003]: unresolved dependency

UserService requires UserRepository,
but no visible provider was found.

Dependency graph:

GET /users/:id
└── UserService
    └── UserRepository  ← missing
```

Database configuration:

```text
error[MADS101]: database auto-configuration failed

Database is required by UserRepository.
Missing configuration:
  database.url
```

Tujuannya bukan mengganti semua Rust compiler error, tetapi memastikan error yang diciptakan abstraction MADS berbicara dalam bahasa MADS.

---

## 18. Explainable Magic

Automatic wiring tidak boleh menjadi black box.

CLI v1 ditargetkan menyediakan:

```bash
mads new my-app
mads dev
mads run
mads routes
mads graph
mads doctor
mads db generate
mads db migrate
mads db rollback
mads db status
```

The v0.8 beta implements this command surface and adds `mads new <name>`.
`mads db generate` has no positional name: it creates one automatically named,
review-required schema diff from the loaded Diesel schema. Human output remains
the default. Finite commands (`new`, `routes`, `graph`, `doctor`, and every
`db` operation) accept `--format human|json`; `run` and `dev` remain raw
streams and reject it.

Machine output contains exactly one newline-terminated versioned envelope:

```json
{
  "schema_version": 1,
  "command": "routes",
  "ok": true,
  "data": { "routes": [] },
  "diagnostics": []
}
```

Schema version 1 allows additive fields; removal, rename, type changes, or
semantic changes require a new schema version. CLI exits remain 0 for success,
1 for operational failure, and 2 for syntax failure. `mads new <name>` creates
the exact bundled seven-file HTTP starter atomically and offline, with only
`http` and `runtime-tokio` enabled.

One useful `mads graph` human rendering is:

```text
AppModule
└── UserModule (route prefix: /users)
    ├── UserRepository
    │   └── Database
    │       └── DieselPool
    ├── UserService
    │   └── UserRepository
    └── GET /:id
        └── UserService
```

`mads doctor` dapat menjelaskan auto-configuration yang aktif, skipped, atau overridden.

---

## 19. Development Experience

Target:

```bash
mads dev
```

Output konseptual:

```text
MADS Development Server

✓ configuration loaded
✓ dependency graph validated
✓ Diesel/PostgreSQL configured
✓ database connected
✓ migrations up to date
✓ modules loaded
✓ routes registered

GET     /users
GET     /users/:id
POST    /users
PUT     /users/:id
DELETE  /users/:id

http://127.0.0.1:3000
```

Development loop harus memprioritaskan incremental compile, readable diagnostics, dan route/application graph visibility.

---

## 20. Testing Direction

Application graph yang sama harus dapat dipakai pada test bootstrap.

Target API:

```rust
#[mads::test]
async fn create_user(users: UserService) {
    // graph provides UserService and dependencies
}
```

HTTP test:

```rust
#[mads::test]
async fn get_user(client: TestClient) {
    client
        .get("/users/1")
        .send()
        .await
        .assert_status(200);
}
```

Exact testing DSL dapat berevolusi selama v0.x, tetapi graph reuse merupakan target desain.

---

## 21. Escape Hatches

MADS tidak boleh menjadi closed abstraction.

Advanced users dapat memakai:

```text
Axum extractors
Axum responses
Tower layers
raw HTTP request/response
native Diesel DSL
custom database provider
custom runtime/server configuration
```

Rule:

```text
simple things  → very simple
complex things → possible
low-level work → accessible
```

---

## 22. Clean Architecture Compatibility

MADS tidak memaksa business/domain layer bergantung pada framework.

Aplikasi dapat memakai layering:

```text
Domain
  ↑
Application / Use Cases
  ↑
Infrastructure adapters
  ↑
Delivery / HTTP
  ↑
MADS composition root
```

Domain dapat berupa pure Rust.

Application mendefinisikan ports/contracts.

Infrastructure mengimplementasikan repository port dengan Diesel.

Delivery menggunakan `mads/common` route macros.

MADS `core` menyusun implementation graph pada composition root.

Dengan demikian MADS tetap mendukung aplikasi sederhana maupun architecture yang lebih ketat.

---

## 23. Explicit Non-Goals for v1

MADS v1 sengaja tidak mencoba menyelesaikan semuanya.

Out of scope:

```text
built-in cache
#[cacheable]
#[cache_evict]
built-in rate limiting
#[rate_limit]
Redis integration
request-scoped DI
transient DI
message queue abstraction
background jobs
GraphQL abstraction
full authentication framework
custom HTTP implementation
custom async runtime
runtime reflection container
```

Menunda fitur tersebut adalah bagian dari scope discipline.

---

## 24. Post-v1 Direction

Setelah v1 stabil, `mads/extra` dapat menambahkan declarative application policies tanpa mengubah `core`.

Target architecture:

```text
Route metadata
   │
   ├── CachePolicy
   │      └── CacheService
   │             └── Redis / other store
   │
   └── RateLimitPolicy
          └── RateLimitService
                 └── Redis / other store
```

Contoh future API:

```rust
#[routes(prefix = "/users")]
trait UserRoutes {
    #[get("/:id")]
    #[cacheable(ttl = "5m")]
    #[rate_limit(requests = 100, window = "1m")]
    async fn get_user(&self, ...);
}
```

Macro tersebut harus menghasilkan metadata/policy, bukan hard-code Redis di route handler.

---

## 25. Final v1 Architecture

```text
┌─────────────────────────────────────────────────────┐
│                  Application Code                   │
│                                                     │
│ Module │ Service │ Repository │ Route │ Domain      │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│                    MADS CORE                        │
│                                                     │
│ Mads Runtime                                        │
│ Application Graph                                   │
│ Dependency Graph                                    │
│ DI / Construction                                   │
│ Lifecycle                                           │
│ Configuration Contracts                             │
│ Diagnostics                                         │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│                   MADS COMMON                       │
│                                                     │
│ HTTP / Axum              Database / Diesel          │
│ Routes                   Pool                       │
│ Extractors                Migrations                │
│ Responses                 Auto Configuration        │
│ Validation                Database Errors           │
└───────────────┬───────────────────────┬─────────────┘
                │                       │
                ▼                       ▼
          Axum / Tower                Diesel
                │                       │
                ▼                       ▼
          Hyper / Tokio              Database
```

`mads/extra` berada di luar v1 runtime requirement.

---

## 26. Definition of Done for MADS v1

MADS v1 dianggap berhasil ketika developer dapat membangun production-oriented REST backend dengan:

```text
Module
Service
Repository
CRUD Routes
Typed Input
Validation
Typed Errors
Diesel persistence
Configuration
Migrations
Automatic dependency wiring
Auto-configuration
Graph validation
Useful diagnostics
Development CLI
Testing foundation
Axum/Diesel escape hatches
```

Tanpa common-path boilerplate:

```text
Arc<AppState>
State<T>
manual service construction
manual repository construction
manual connection-pool bootstrap
manual route registry list
manual Tokio bootstrap
manual common error mapping
```

---

## 27. Final Definition

> **MADS.rs v1 is a type-driven, auto-configuring modular application framework for Rust that turns explicit application architecture and Rust dependency types into a running Axum + Diesel backend.**

Short version:

> **Declare what your application needs. MADS wires the rest.**
