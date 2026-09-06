# MADS.rs — Final Timeline to v1

## Scope

Timeline ini mendefinisikan jalur implementasi MADS dari foundation sampai **v1.0.0**.

Target utama v1:

> Membuktikan bahwa MADS dapat menyediakan modular Rust backend dengan type-driven dependency wiring, Axum HTTP integration, Diesel persistence, auto-configuration, diagnostics, CLI, dan testing foundation secara stabil.

**Cache, Redis, `#[cacheable]`, `#[cache_evict]`, built-in rate limiting, dan `#[rate_limit]` tidak masuk scope v1.** Fitur tersebut dimulai setelah v1 agar application graph dan public API tidak berubah karena feature pressure terlalu awal.

---

## Release Strategy

```text
0.1.0  Foundation
  ↓
0.2.0  Dependency Graph
  ↓
0.3.0  HTTP/Common Runtime
  ↓
0.4.0  Diesel Persistence
  ↓
0.5.0  Auto Configuration
  ↓
0.5.5  Configuration Arrays + Cookies + Passport/JWT
  ↓
0.6.0  Modules + Scoped HTTP Runtime
  └─ beta.1  Public HTTP application foundation
  ↓
0.7.0  CLI + Dev Loop + Diagnostics
  └─ beta.1  Complete v0.7 feature set before stable promotion
  ↓
0.8.0  Input Validation + REST Errors + Configuration UX + Scaffolding
  ↓
0.9.0  Testing + Hardening
  ↓
1.0.0  Stable Contract
```

Version number menunjukkan capability milestone, bukan janji calendar date. Setiap milestone harus lolos acceptance criteria sebelum release berikutnya.

---

# Phase 0 — Repository and Architecture Foundation

## Goal

Menetapkan physical crate/module boundaries sebelum API berkembang terlalu jauh.

Target layout:

```text
mads/
├── core/
├── common/
└── extra/
```

Untuk v1:

```text
core   → aktif
common → aktif
extra  → reserved / minimal shell
```

`extra` tidak menjadi dependency requirement aplikasi v1.

### Deliverables

- workspace structure;
- feature flags strategy;
- facade/prelude strategy;
- CI for stable + minimum supported Rust policy;
- formatting/lint/test baseline;
- compile-test harness untuk proc macros;
- architecture dependency rules.

### Acceptance

```text
core does not depend on common
core does not depend on extra
common depends only downward into core abstractions
future extra can plug into core/common without changing core
```

---

# v0.1.0 — Core Runtime Foundation

## Objective

Membangun MADS sebagai application runtime terlebih dahulu, sebelum HTTP/ORM menjadi fokus utama.

### `mads/core`

Implement:

```text
Mads instance
Mads builder
application context
basic provider registry
application-scoped lifecycle
configuration bootstrap foundation
Result/Error foundation
#[mads::main]
#[service]
#[repository]
#[provider]
#[routes]
#[controller]
#[get] / #[post] / #[put] / #[patch] / #[delete] contract validation
```

Route/controller attributes at this milestone emit framework-neutral static
metadata and validate route conflicts, but do not execute HTTP requests.
`#[controller]` is an application-scoped managed provider and may depend on
multiple services/use cases. Route registration, extractors, and Axum adapters
remain assigned to v0.3.

### Service / Repository Macro MVP

Input:

```rust
#[repository]
struct UserRepository {
    db: Database,
}

#[service]
struct UserService {
    users: UserRepository,
}
```

Macro minimal harus mampu menghasilkan metadata dependency yang dapat dibaca graph engine berikutnya.

### Constraints

Belum perlu:

```text
request scope
transient scope
trait qualifiers
cache
rate limit
Redis
advanced HTTP
```

### Exit Criteria

- service/repository metadata deterministic;
- instance dapat dibangun oleh generated constructor path;
- startup/shutdown basic lifecycle bekerja;
- proc macro diagnostics cukup jelas untuk malformed declarations;
- no mandatory Axum dependency di core.

---

# v0.2.0 — Dependency and Application Graph

## Objective

Menjadikan type-driven dependency graph sebagai pusat framework.

### Implement

```text
provider nodes
dependency edges
topological construction order
missing dependency detection
duplicate provider detection
dependency cycle detection
ambiguous provider detection
provider visibility foundation
graph inspection API
```

Example:

```text
UserService
└── UserRepository
    └── Database
```

### Construction Plan

Graph engine harus menghasilkan deterministic plan:

```text
Database
  ↓
UserRepository
  ↓
UserService
```

### Exit Criteria

- graph validation terjadi sebelum server/runtime application start;
- cycle dapat ditampilkan sebagai readable dependency path;
- no reliance on registration order;
- `mads graph` internal data model sudah tersedia meskipun CLI belum final.

---

# v0.3.0 — `mads/common` HTTP and Routing

## Objective

Membawa Axum sebagai standard HTTP engine tanpa membuat `core` menjadi HTTP-specific.

### Implement in `common`

```text
route registry and adapter execution
runtime expansion of the v0.1 route/controller contracts
Axum router generation
Path<T>
Query<T>
Json<T>
Header<T>
Request integration
basic response mapping
Created<T>
NoContent
```

### Route Dependency Injection

Target:

```rust
#[routes(prefix = "/users")]
trait UserRoutes {
    #[get("/:id")]
    async fn get_user(&self, id: Path<i64>) -> Result<Json<User>>;
}

#[controller(routes = [UserRoutes])]
struct UserController {
    users: UserService,
}
```

MADS harus membedakan:

```text
Path<i64>             → extractor
UserService           → controller dependency graph node
UserController method → Axum handler adapter
```

### Exit Criteria

- CRUD routes dapat berjalan;
- no manual `State<AppState>` untuk standard path;
- no manual Axum router wiring list;
- native Axum extractor escape hatch tetap bekerja;
- HTTP adapter tidak mengubah dependency semantics core.

---

# v0.4.0 — Diesel-first Persistence

## Objective

Membuat persistence experience yang cukup lengkap untuk real CRUD application.

### Implement in `common::database`

```text
Database abstraction
DatabaseConfig
Diesel integration
connection pool lifecycle
PostgreSQL initial backend
Database::run
migration runner
startup migration option
Diesel error normalization foundation
```

Target configuration:

```toml
[database]
url = "${DATABASE_URL}"
pool_size = 10
migrate = true
```

Target repository:

```rust
#[repository]
struct UserRepository {
    db: Database,
}
```

### CLI foundation

```bash
mads db migrate
mads db rollback
mads db status
```

### Exit Criteria

- complete User CRUD works with Diesel/PostgreSQL;
- database pool initialized once;
- migration can run manually/startup;
- repository receives `Database` through dependency graph;
- direct Diesel DSL remains available.

---

# v0.5.0 — Auto-Configuration Engine

## Objective

Menggabungkan application requirements, capability availability, dan configuration menjadi automatic infrastructure provisioning.

### Core Auto-Config Model

```text
Requirement
  +
Capability
  +
Configuration
  ↓
AutoConfiguration
```

### Diesel Auto Configuration

Activation example:

```text
UserRepository requires Database
        +
Diesel integration available
        +
database.url configured
        ↓
DieselDatabaseAutoConfiguration
```

### Back-Off

Implement rule:

```text
custom provider exists
      ↓
default auto-config backs off
```

### Inspection Model

Auto-config engine harus menyimpan reason:

```text
ACTIVE
SKIPPED
OVERRIDDEN
FAILED
```

Configuration loading tetap explicit: `Mads::builder()` tidak memuat `.env`,
`mads.toml`, atau `MADS_*` secara otomatis. Requirements memakai complete
statically discovered provider catalog pada v0.5; v0.6 menggantinya dengan
reachability dari root `AppModule`.

v0.5 tidak menambahkan chained defaults, priority selection, module scoping,
third-party registration API, migration generation, proactive schema checks,
atau `mads doctor`. Reports hanya menyimpan evidence yang sudah di-redact,
tidak pernah resolved configuration values atau credentials.

### Exit Criteria

- default database wiring membutuhkan zero bootstrap code di `main`;
- custom Database provider dapat override default;
- auto-config activation deterministic;
- reason dapat dipakai oleh `mads doctor` di milestone v0.7.
- `DatabaseBootstrap` tetap merupakan explicit override; embedded migrations
  didaftarkan secara terpisah dan tidak pernah dihasilkan otomatis.

---

# v0.5.5 — Configuration Arrays, Cookies, Passport, and JWT

## Objective

Menambahkan konfigurasi string-array, cookie request/response, JWT access dan
refresh profiles, managed Passport strategies, typed principals, dan guard
policies tanpa membuat `mads-core` bergantung pada HTTP atau cryptography.

### Explicit Configuration and String Arrays

`ConfigBuilder` tetap explicit. Dotenv menyediakan interpolation values;
process variables override dotenv values saat `${NAME}` di-resolve. Ordinary
sources merge dari awal ke akhir, sehingga source terakhir menang. Scalar atau
string array yang lebih akhir mengganti value shape sebelumnya secara penuh.
TOML dan programmatic sources mendukung string arrays; `EnvSource` tetap
scalar-only.

### Cookies

Feature `cookies` menyediakan strict `CookieJar`, parsing request cookie,
checked `Set-Cookie` composition, `Path`, `Domain`, `Max-Age`, `Expires`,
`HttpOnly`, `Secure`, dan `SameSite`. Ordinary malformed cookie extraction
menghasilkan `400`; cookie authentication yang missing, malformed, atau
duplicated menghasilkan generic `401`. Cookie-based JWT tidak menyediakan CSRF
protection.

### JWT Service and Key Profiles

`[passport] secret = "${JWT_SECRET}"` memilih HS256 dengan minimum 32 bytes.
Allowlist mendukung HS256/384/512, RS256/384/512, dan ES256/384. Named key rings
memiliki satu active signing key dan retained verification keys selected by
`kid`; setiap key terikat pada tepat satu algorithm. Token header tidak pernah
memperluas configured allowlist.

`JwtService` menyediakan typed signing/verification dan explicit untrusted
decode APIs. Access dan refresh profiles mempunyai mutually exclusive `typ`
header dan `token_use` claim. `JwtSignOptions::access`/`refresh` dan
`JwtValidation::access`/`refresh` selalu meminta caller memilih kind.

### Managed Passport Strategies and Principals

Custom strategy harus implement `PassportStrategy`, memakai
`#[passport_strategy]`, dan concrete type-nya harus managed provider. Framework
memverifikasi signature, registered claims, serta token kind sebelum strategy
validation. `jwt` adalah built-in access strategy; satu custom `jwt` override
built-in. Duplicate custom names ambiguous. `jwt-refresh` adalah
application-defined refresh strategy, bukan built-in.

Strategy menghasilkan typed application principal seperti `UserPrincipal`.
`PassportPrincipal` mengekspos role/permission membership dan dapat di-derive
dengan `#[roles]` serta `#[permissions]`. Guarded handler mengekstrak
`Authenticated<P>` dan `VerifiedToken<C>`.

### Guards

```text
#[guard] on #[routes] trait       inherited policy
#[guard] on route method         supplied fields replace inherited fields
#[guard(skip)]                    sole inherited-policy opt-out
source = bearer                  default, one source only
source = cookie("literal-name")  requires cookies
```

Roles, permissions, dan synchronous `fn(&Principal) -> bool` predicates adalah
separate AND clauses. `any` dan `all` mengontrol matching di dalam satu role
atau permission clause. Authentication/strategy rejection menghasilkan generic
`401` plus `WWW-Authenticate: Bearer`, authorization failure `403`, dan
operational failure `500`.

Native Axum `PassportGuard<P>` adalah runtime escape hatch memakai pipeline yang
sama. Karena tidak masuk static MADS guard catalog, native guards tidak dapat
mengaktifkan JWT auto-configuration; application context harus sudah memiliki
`JwtService`, atau guard build gagal dengan `MADS131`.

### Explicitly Out of Scope

v0.5.5 tidak mengimplementasikan login/credential validation, built-in refresh
endpoint, refresh persistence/rotation/revocation/reuse detection, password
hashing, CSRF, CORS, HTTP auto-binding, remote JWKS, JWE, atau module scoping.

### Exit Criteria

- string-array merge/interpolation/source attribution is deterministic;
- all eight algorithms and active/previous key rotation are verified offline;
- access and refresh profiles reject cross-kind substitution;
- custom/built-in strategy resolution and principal types validate preflight;
- inherited/overridden/skipped Bearer and cookie guards are covered;
- cookies compose with native Axum requests and responses;
- secrets, keys, tokens, claims, principals, and cookie values remain redacted;
- core-only/JWT-only/cookie/Passport feature boundaries remain intact.

---

# v0.6.0 — Modules, CORS, and Automatic HTTP Runtime

## Objective

Menggabungkan architecture boundary, scoped automatic wiring, browser delivery,
dan conventional HTTP runtime dalam satu aplikasi berakar. Module bukan DI
manifest: Rust namespace menentukan ownership, sedangkan provider relationship
tetap diinfer dari dependency concrete type.

### Implement

~~~text
#[module(imports = [UserHttpModule])]
Rust-namespace ownership
direct imports + plain pub cross-module access
unowned dependency closure with propagated module context
scoped providers/controllers/routes/guards/strategies/auto-configuration
Mads::run::<AppModule>().await
optional .env + optional mads.toml + MADS_* overrides
server.host/server.port defaults and explicit serve override
strict application-wide CORS
native router composition
~~~

Module graph dilalui dari root dengan direct imports saja. Provider, controller,
route, guard, dan strategy dimiliki oleh nearest annotated Rust namespace.
Provider unowned ikut hanya melalui dependency closure dan mempertahankan module
context pemanggil, sehingga tidak dapat melewati import langsung yang hilang.
Provider lintas module harus public dan module pemiliknya harus di-import
langsung; pub menggantikan exports manifest. Route contract secara eksklusif
memiliki prefix HTTP melalui #[routes(prefix = "...")]; #[module] tidak
menerima path HTTP.

Standard run memuat optional .env untuk interpolation, optional mads.toml,
kemudian override MADS_* dari current working directory. Defaults automatic
server adalah server.host = 127.0.0.1 dan server.port = 3000. Explicit
serve/serve_router address mengambil alih binding, termasuk port zero.

CORS adalah opt-in dan divalidasi ketat sebelum lifecycle; konfigurasi final
diterapkan satu kali sebagai layer terluar setelah generated dan native router
digabung. Native Axum router tetap escape hatch yang didukung.

Builder tanpa root tetap mempertahankan complete-catalog behavior v0.5.5 untuk
compatibility low-level.

### Exit Criteria

- root application graph dapat dimulai dari AppModule dan direct import cycle
  diagnostic tetap readable;
- namespace ownership, direct public access, dan unowned context propagation
  memvalidasi dependency lintas module;
- hanya provider, controller, route, guard, strategy, dan official
  auto-configuration dari application scope yang selected;
- standard Mads::run memuat source konvensional, memvalidasi router/CORS,
  kemudian bind satu listener dengan rollback lifecycle yang deterministik;
- CORS credential dan wildcard validation production-safe untuk generated dan
  native routes;
- explicit low-level builder, server address, dan native router composition
  tetap tersedia.

---

# v0.7.0 — CLI, Dev Loop, and Framework Diagnostics

## Objective

Menjadikan explainable magic dan development loop sebagai bagian produk. Hanya
diagnostic, error, configuration inspection, dan framework metadata yang
dibutuhkan CLI yang masuk milestone ini.

`0.7.0-beta.1` harus membawa seluruh feature set v0.7. Stable `0.7.0` hanya
menambahkan bug fix, koreksi dokumentasi, testing, dan release verification;
stable tidak menambahkan fitur baru setelah beta.

### CLI

```bash
mads dev [--package <package>] [--bin <bin>] [-- <app-args>...]
mads run [--package <package>] [--bin <bin>] [-- <app-args>...]
mads routes [--package <package>] [--bin <bin>]
mads graph [--package <package>] [--bin <bin>]
mads doctor [--package <package>] [--bin <bin>]
mads db generate [--package <package>]
mads db migrate [--package <package>]
mads db rollback [--package <package>]
mads db status [--package <package>]
```

`-p` merupakan alias Cargo-native untuk `--package`. Jika selector tidak
diberikan, Cargo memilih target atau melaporkan ambiguity dengan aturan normal.
Argument setelah `--` diteruskan tanpa perubahan ke application. Command
`mads foundation` dihapus karena capability-nya digantikan oleh `mads doctor`.

Output v0.7 deterministik dan human-readable. JSON atau machine-readable output
ditunda ke v0.8.

### Application Inspection Protocol

`mads routes`, `mads graph`, dan `mads doctor` menjalankan selected application
binary sebagai short-lived child dalam private versioned inspection mode.
`Mads::run::<AppModule>()` mendeteksi mode ini sebelum provider construction,
lifecycle startup, database connection, migration, listener bind, atau request
serving. Child mengembalikan structured report lalu keluar; hanya parent CLI
yang merender output.

v0.7 mendukung standard `Mads::run::<AppModule>()` entry point. Low-level custom
builder tidak diinspeksi dan menerima unsupported-entry-point diagnostic.
Protocol handshake, version check, timeout, dan child termination mencegah
process yang tidak kompatibel dibiarkan berjalan. Jaminan tanpa runtime side
effect berlaku untuk MADS startup; Cargo build scripts atau arbitrary user code
sebelum `Mads::run` berada di luar standard entry-point contract.

### `mads run` and `mads dev`

`mads run` membangun dan menjalankan selected Cargo binary, meneruskan output,
argument, signal, dan application exit result.

`mads dev` menyediakan watcher sendiri tanpa executable `cargo-watch`. Watcher
mencakup Rust sources, local path dependencies, Cargo manifests/lockfile,
`mads.toml`, `.env`, migrations, dan schema files. Rust, Cargo, schema, atau
embedded migration changes memakai Cargo incremental build. Config-only change
me-restart last successful binary tanpa rebuild.

Events di-debounce. Last successful application tetap berjalan selama rebuild;
successful build menggantinya secara graceful, sedangkan compile failure
menampilkan Cargo/rustc output apa adanya dan tetap menunggu perubahan baru.
Tidak ada HMR atau in-process code swapping pada v0.7.

### Routes, Graph, and Doctor

`mads routes` menampilkan method, full path, route trait/handler, controller,
source location, dan guard state. Conflict atau invalid route tetap menampilkan
partial useful output, diagnostic, dan failure exit.

`mads graph` menampilkan root/direct-import tree, selected providers, dependency
edges, owner, origin, visibility, state, dan construction order ketika valid.
Invalid graph tetap ditampilkan sebatas data yang dapat dianalisis lalu diikuti
diagnostic.

`mads doctor` melakukan offline framework checks untuk configuration, module
graph, providers, routes, guards/strategies, server/CORS, dan auto-configuration.
Status report menggunakan `PASS`, `SKIPPED`, `OVERRIDDEN`, dan `FAILED`.
Command tidak membuka database connection, menjalankan migration, lifecycle,
atau listener.

### Database Migration Generation

`mads db generate` tidak menerima nama. Command memuat `src/schema.rs` dan semua
Rust files di bawah `src/schema/` secara recursive, lalu menggabungkan
`diesel::table!` declarations secara deterministik. Tidak dibutuhkan external
Diesel CLI.

Schema dibandingkan dengan live PostgreSQL menggunakan CLI configuration chain
yang sudah ada: selected package root, optional `.env`, required `mads.toml`,
`MADS_*` overrides, dan `database.url`.

Supported diff v0.7:

```text
create/drop table
add/drop column
column type change
column nullability change
primary-key preservation for created tables
reversible up/down SQL from captured live state
```

Jika ada perbedaan, generation membuat:

```text
migrations/<timestamp>_schema_diff/up.sql
migrations/<timestamp>_schema_diff/down.sql
```

Defaults, indexes, checks, triggers, exact foreign-key policies, risky casts,
dan required columns pada populated tables tidak ditebak diam-diam. Generator
memberi warning dan review comments. SQL selalu review-required, tidak pernah
diterapkan otomatis, tidak menimpa path, dan dibuat secara atomic. Failure atau
no-diff tidak meninggalkan file parsial; no-diff merupakan success.

### Diagnostics and Configuration Inspection

Existing diagnostic model mendapat read-only structured fields dan CLI codes
yang diperlukan untuk Cargo selection, inspection, graph/routes, watcher,
schema parsing, database introspection, dan generation safety. Cargo/rustc
diagnostics tetap diteruskan apa adanya.

Configuration inspection hanya melaporkan presence, winning source, dan hasil
validation yang diperlukan `doctor`. Generic report tidak mencetak raw values
atau credentials.

### Exit Criteria

- seluruh command tersedia di `0.7.0-beta.1` dan bekerja di Linux, macOS, dan
  Windows;
- `run` dan `dev` memakai Cargo selectors dan argument forwarding yang benar;
- `routes`, `graph`, dan `doctor` dapat menganalisis standard application tanpa
  startup side effects;
- watcher hanya rebuild ketika category perubahan memerlukannya;
- split schema files menghasilkan satu deterministic complete database diff;
- generated up/down SQL lolos real PostgreSQL round-trip tests untuk supported
  operations;
- no-diff dan failure tidak membuat migration files;
- stable `0.7.0` tidak menambah fitur setelah beta.

---

# v0.8.0 — Input Validation, REST Errors, Configuration UX, and Scaffolding

## Objective

Membuat common REST application tidak perlu merakit input validation, standard
error mapping, typed configuration plumbing, atau minimal project structure
sendiri. Scope ini tidak memblokir CLI v0.7.

`0.8.0-beta.1` membawa seluruh feature set v0.8. Stable `0.8.0` hanya
menambahkan bug fix, koreksi dokumentasi, dan release verification; stable
tidak menambahkan feature yang belum ada di beta.1.

### Minimal Project Scaffolding

```bash
mads new <name>
```

Command membuat child directory baru dengan tepat tujuh file:

```text
Cargo.toml
mads.toml
src/main.rs
src/app/mod.rs
src/app/routes.rs
src/app/controller.rs
src/app/service.rs
```

Starter memakai `AppModule`, `AppController`, dan `AppService`; `GET /`
mengembalikan plain-text `Hello World!`. Dependency MADS memakai exact version
yang sama dengan CLI, menonaktifkan default features, dan hanya mengaktifkan
`http` serta `runtime-tokio`. Database, JWT, cookies, migration, Git init,
download, dan build tidak dijalankan atau dibuat.

Nama mengikuti lowercase Cargo-style. Existing destination selalu ditolak.
Generation memakai bundled template dan atomic publication sehingga failure
tidak meninggalkan partial project atau mengubah path yang sudah ada.

### Validation

Implement target API:

```rust
#[derive(serde::Deserialize, Input)]
struct CreateUser {
    #[validate(email)]
    email: String,
}
```

Flow:

```text
ValidatedJson / ValidatedQuery / ValidatedPath
  ↓
Serde deserialize
  ↓
Input validate
  ↓
handler
```

Native `Json`, `Query`, dan `Path` tetap tersedia tanpa automatic validation.
Built-in validator mencakup email, string/collection min-max-exact length,
nonempty, numeric inclusive range, positive, negative, multiple-of, required
option, nested input, dan synchronous custom validator. Primitive Rust types,
struct, enum, tuple, array, vector, option, serta deterministic string-keyed map
didukung. User dapat menggabungkan derive dengan custom validator atau
mengimplementasikan `Input` secara manual.

Setelah Serde berhasil, seluruh independent validation issue dikumpulkan dalam
declaration/index order. Serde conversion sendiri tetap first-error karena
Serde adalah deserialization authority. Path mengikuti external Serde field dan
enum representation.

Validation failure memakai status 422 dan schema source-aware:

```json
{
  "error": {
    "code": "validation_error",
    "message": "input validation failed",
    "issues": [
      {
        "source": "body",
        "path": ["email"],
        "code": "invalid_format",
        "message": "invalid email address"
      }
    ]
  }
}
```

JSON transport failure tetap mempertahankan 413/415 ketika relevan dan memakai
standard error envelope.

### Error Model

Standard errors:

```text
BadRequest
Unauthorized
Forbidden
NotFound
Conflict
ValidationError
InternalError
```

Error response schema menggunakan `{ "error": { "code", "message" } }` secara
konsisten untuk MADS-owned standard, validation, Passport, cookie, dan validated
extractor failure. Existing `HttpError` constructors dan code `bad_request`,
`not_found`, `conflict`, serta `internal` tetap compatible. Passport 401 tetap
mengirim `WWW-Authenticate: Bearer`. Internal source tidak pernah masuk body,
display, atau debug output.

Common MADS `DatabaseResult<T>` dan native Diesel `QueryResult<T>` mendapat
explicit `.into_http()` opt-in mapping:

```text
Diesel NotFound          -> 404 not_found
unique constraint       -> 409 conflict
all other database error -> 500 internal, redacted
```

Tidak ada automatic blanket conversion. Developer tetap dapat memakai
`map_err`, native Axum response, extractor, router, dan middleware.

### Typed Config, Secrets, Diagnostics, and CLI Output

Implement generic typed configuration:

```rust
#[derive(Configuration)]
#[config(prefix = "app")]
struct AppConfig {
    host: String,
    #[config(default = 3000)]
    port: u16,
    api_key: Secret<String>,
}

let app = config.parse::<AppConfig>()?;
```

Supported shape mencakup scalar Rust, source-relative `PathBuf`, option,
`Secret<T>`, string array, nested `Configuration`, dan custom scalar
`parse_with`. `ConfigurationIssue`/`ConfigurationErrors` mengumpulkan
independent missing/parse/validation error dalam declaration order dengan
stable reason code, full dotted key, dan optional winning source tanpa value;
collection tersebut dapat dikonversi ke aggregated `MADS020` framework error.
Application-defined typed config divalidasi secara explicit, biasanya melalui
selected startup provider; derive yang tidak digunakan tidak menjadi global
startup requirement.

Existing conventional configuration loading tidak berubah:

```text
optional .env untuk exact ${NAME} interpolation
  -> optional mads.toml
  -> final MADS_* environment overrides
  -> explicit Config::parse<T>()
```

Process environment tetap menang atas dotenv untuk interpolation, dotenv tidak
memutasi process environment, dan `MADS_APP__HOST`/`MADS_SERVER__HOST` tetap
memetakan `app.host`/`server.host`. Typed parsing tidak memuat environment atau
file lagi.

`Secret<T>` hanya dapat dibuka melalui explicit `expose`/`into_exposed`.
Display dan Debug selalu redacted; tidak ada implicit deref atau Serialize, dan
generic memory zeroization tidak dijanjikan.

Opaque compiler diagnostic diperbaiki hanya untuk constraint yang dimiliki
MADS macro, termasuk Input/Configuration, controller-route, module import,
managed dependency, known route signature, dan Passport principal. Cargo,
rustc, Axum, Diesel, serta application diagnostic lain tetap diteruskan apa
adanya.

Finite CLI commands mendukung optional `--format human|json`:

```text
new
routes
graph
doctor
db generate
db migrate
db rollback
db status
```

JSON memakai satu versioned envelope dengan `schema_version`, canonical
`command`, `ok`, command-specific `data`, dan structured `diagnostics` yang
memiliki `error`/`warning` severity. `run` dan `dev` tetap raw streaming dan
menolak format JSON. Existing exit code 0/1/2 tidak berubah.

### Exit Criteria

- `mads new <name>` membuat exact minimal HTTP starter secara offline dan
  atomic; generated project dapat compile, inspect, run, dan melayani
  `Hello World!` tanpa database/JWT feature;
- invalid input tidak masuk handler;
- seluruh approved validator/type matrix bekerja dan validation response
  konsisten, deterministic, serta source-aware;
- seluruh standard error dan MADS-owned rejection memakai safe JSON envelope;
- missing/invalid configuration error memiliki full key dan source yang jelas
  tanpa value, serta environment loading tetap compatible;
- common MADS dan native Diesel errors memiliki standard opt-in mapping;
- secret values aman dalam display dan debug output;
- developer tetap dapat return native Axum response;
- MADS-owned macro failure yang dipilih mempunyai focused stable/MSRV
  diagnostic tanpa menjanjikan rewrite untuk error eksternal;
- seluruh finite command menghasilkan schema JSON v1 yang deterministic dan
  secret-safe, sedangkan `run`/`dev` tetap raw streaming;
- full release gates berjalan di Linux, platform-sensitive CLI/scaffold/process
  tests berjalan di Linux, macOS, dan Windows, serta PostgreSQL integration
  tetap Linux-only;
- seluruh feature tersedia di `0.8.0-beta.1` sebelum stable promotion.

---

# v0.9.0 — Testing, Compatibility, and Hardening

## Objective

Menghentikan feature expansion dan fokus pada reliability menuju 1.0.

### Testing Runtime

Target APIs:

```rust
#[mads::test]
async fn service_test(users: UserService) {}
```

```rust
#[mads::test]
async fn http_test(client: TestClient) {}
```

### Provider Overrides for Tests

Foundation untuk replacing provider pada test graph.

### Hardening

```text
compile-fail macro tests
integration CRUD suite
module visibility suite
auto-config back-off suite
migration suite
shutdown/lifecycle suite
Axum escape-hatch suite
native Diesel suite
configuration error suite
```

### Performance Baseline

Bandingkan:

```text
startup overhead
request overhead
memory overhead
compile impact
```

terhadap equivalent Axum application.

Tujuan bukan artificial zero-overhead claim, tetapi cost yang kecil dan dapat dijelaskan.

### API Freeze Candidate

Mulai larang perubahan besar pada:

```text
#[service]
#[repository]
#[module]
route macros
Mads::run
Database
Result
core graph concepts
```

### Exit Criteria

- sample applications stabil;
- no known graph correctness bugs;
- public API candidate documented;
- migration path dari 0.9 ke 1.0 minimal.

---

# v1.0.0 — Stable MADS Foundation

## Objective

Menetapkan contract yang dapat dibangun ecosystem tanpa terus berubah.

### v1 Includes

```text
mads/core
  Mads runtime
  modules
  service/repository/provider macros
  application graph
  dependency graph
  lifecycle
  auto-configuration engine
  diagnostics core

mads/common
  Axum HTTP integration
  route macros
  request/response types
  validation
  standard errors
  Diesel integration
  Database
  pool lifecycle
  migrations

CLI
  dev
  run
  routes
  graph
  doctor
  db commands

Testing foundation
Escape hatches
Documentation
```

### Explicitly NOT in v1

```text
Redis
CacheService
#[cacheable]
#[cache_evict]
RateLimitService
#[rate_limit]
request-scoped DI
transient DI
advanced auth framework
message queue
background jobs
```

### Stability Commitments

v1 should commit to the concepts, not every internal implementation detail:

```text
explicit modules
type-inferred dependencies
automatic wiring
explainable auto-configuration
Diesel-first persistence
Axum compatibility
framework diagnostics
escape hatches
```

---

# v1 Reference Application Gate

Sebelum 1.0 release, satu reference CRUD application harus membuktikan seluruh common path.

Required routes:

```text
GET    /users
GET    /users/:id
POST   /users
PUT    /users/:id
DELETE /users/:id
```

Required graph:

```text
Route
  ↓
UserService
  ↓
UserRepository
  ↓
Database
  ↓
Diesel
  ↓
PostgreSQL
```

Required developer-visible application code harus bebas dari:

```text
Arc<AppState>
State<T>
manual repository construction
manual service construction
manual Diesel pool bootstrap
manual Axum Router composition
manual Tokio bootstrap
```

---

# Documentation Gate for 1.0

Sebelum v1:

```text
Getting Started
Core Concepts
Modules
Services
Repositories
Routing
Validation
Error Handling
Configuration
Diesel / Database
Migrations
Dependency Graph
Auto Configuration
Diagnostics
Testing
Axum Escape Hatch
Diesel Escape Hatch
Clean Architecture Example
Migration Guide from 0.9
```

---

# Post-v1 — `mads/extra`

Setelah 1.0 stabil, development dapat bergerak ke optional policies.

Suggested first sequence:

```text
1.1 / 1.x foundation
  Redis capability
      ↓
  CacheService
      ↓
  #[cacheable] / #[cache_evict]
      ↓
  RateLimitService
      ↓
  #[rate_limit]
```

Semua fitur ini harus plug into existing graph/policy metadata tanpa memerlukan redesign `mads/core`.

---

# Final v1 Success Criterion

MADS v1 berhasil apabila backend developer yang memahami basic Rust dapat membuat CRUD production-oriented dengan mental model:

```text
Model
  ↓
Repository
  ↓
Service
  ↓
Route
  ↓
Run
```

sementara framework menangani:

```text
application graph
dependency construction
shared ownership
Axum runtime/router
Diesel pool
configuration
migrations
validation
error mapping
diagnostics
```

> **v1 harus membuktikan fondasi MADS terlebih dahulu. Optional infrastructure policies datang setelah fondasi tersebut stabil.**
