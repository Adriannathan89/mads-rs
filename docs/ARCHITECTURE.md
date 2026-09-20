# MADS.rs 0.8.0 Architecture

MADS separates framework-neutral construction and configuration from Axum HTTP
delivery, optional PostgreSQL/Diesel persistence, and the Cargo-native CLI.
Version 0.8.0 completes the approved validation, REST-error, typed
configuration, compiler-diagnostic, machine-output, and minimal-scaffolding
surface. Stable 0.8.0 promotes this same surface after fixes, documentation
corrections, and verification only; it does not add features.

~~~text
application modules, providers, route traits, controllers
                 |
                 v
     mads-core macros       mads-common macros
                 |                 |
                 v                 v
 mads-core: graph, Config, Configuration, Secret, lifecycle, diagnostics
                 ^                 |
                 |                 v
 mads-common: Input, validated extractors, HTTP errors, Axum, Diesel, Passport
                 \                 /
                  \--- mads facade ---/ ---- mads-cli
~~~

## Crate and feature boundaries

`mads-core` owns module/provider graphs, lifecycle, diagnostics, the existing
source-attributed `Config`, `#[derive(Configuration)]`, `Config::parse`,
configuration issues, and `Secret<T>`. It has no Axum, Diesel, JWT, cookie, or
Serde dependency. Typed configuration is a view over an already loaded Config;
it does not replace loading or discover types globally.

`mads-common` owns the `http` boundary: route registration, `Input` and
`#[derive(Input)]`, `ValidatedJson`, `ValidatedQuery`, `ValidatedPath`, standard
REST errors, and the Axum adapter. It also owns the feature-gated database,
Passport/JWT, and cookie integrations. The validation/error family requires
`http`; the database result extension `.into_http()` exists only with `http +
database`. Database-only and JWT-only builds do not acquire an HTTP dependency.

`mads` is the stable facade and prelude. It re-exports matching traits and
derives, so `Input`, `Configuration`, `Secret`, validated extractors, REST
errors, and `IntoHttpResult` use their documented feature gates. The default
`common` aggregate remains the compatibility combination for HTTP and database;
it does not enable Passport automatically.

~~~text
core                         no HTTP/database/JWT/cookie/Serde
http                         Axum + validation + standard REST errors
database                     Diesel infrastructure, no HTTP mapping
http + database              explicit IntoHttpResult::into_http()
jwt                          JWT service/configuration, no Axum
cookies                      HTTP cookie support
http + jwt (+ cookies)       Passport Bearer (and cookie) guards
~~~

## Startup, configuration, and secret boundary

`Mads::run::<AppModule>()` retains the conventional process-current-directory
loading sequence:

~~~text
optional .env interpolation map
  -> optional mads.toml document
  -> final scalar MADS_* environment overrides
  -> application code explicitly requests typed views
~~~

Only an entire scalar or string-array element equal to `${NAME}` interpolates.
Process values win during interpolation; dotenv never changes the process
environment and is not a configuration source. No parent-directory or
`CARGO_MANIFEST_DIR` search is added. `MADS_SERVER__HOST` and
`MADS_SERVER__PORT` continue to map to `server.host` and `server.port`.

The low-level builder remains explicit and performs no source loading. A
derived type is parsed only when code calls `Config::parse::<T>()`; a selected
provider commonly makes that call so failures stop construction before
lifecycle startup or listener binding. Supported fields are scalars,
source-relative `PathBuf`, `Option`, `Secret`, `Vec<String>`, nested
`Configuration`, and explicit scalar `parse_with` callbacks. Prefixes, rename,
defaults, and compatible validation are derive-checked. Issues preserve
declaration order, full dotted keys, stable codes, and source labels, without
values.

`Secret<T>` has no implicit reference or serialization access. `.expose()` and
`.into_exposed()` name the deliberate boundary; `Display` and `Debug` always
render `[REDACTED]`. This redaction policy also applies to configuration
reports, diagnostics, JSON output, and database source details.

## Request validation and native escape hatch

`ValidatedJson<T>`, `ValidatedQuery<T>`, and `ValidatedPath<T>` have this
strict runtime order:

~~~text
representation read -> Serde deserialize T -> T::validate()
  -> attach body/query/path source -> invoke handler only on success
~~~

`#[derive(Input)]` supports named, tuple, and unit structs, enum variants,
generics with required bounds, nested input, supported arrays/vectors/tuples,
and string-keyed maps. Its built-ins are email; Unicode code-point length;
nonempty; inclusive range; positive; negative; nonzero multiple-of; required;
nested; and synchronous field or whole-value custom callbacks. Manual `Input`
implementations share the same validated-extractor boundary.

Post-deserialization validation aggregates independent issues in declaration,
validator, sequence-index, and lexical-key order. Serde itself remains the
deserialization authority and reports its first conversion failure. Sources,
wire field names, issue codes, and fixed built-in messages are deliberate public
contracts; rejected values are never copied into a built-in issue.

Native `Json<T>`, `Query<T>`, and `Path<T>` are still the ordinary Axum
extractors. A native `Json` handler performs no MADS `Input` validation, which
is the compatibility escape hatch for application-owned extraction, routing,
middleware, and validation policies. Native Axum rejections outside MADS
wrappers remain native.

## HTTP error and delivery-policy boundary

The `http` feature exposes `BadRequest`, `Unauthorized`, `Forbidden`,
`NotFound`, `Conflict`, `ValidationError`, and `InternalError`. All
MADS-owned errors serialize as one safe JSON envelope. Validation adds only the
ordered source-aware issue array and returns 422; unsupported validated JSON
content type remains 415 and configured payload overflow remains 413. Internal
failure is fixed to code `internal` and message `internal server error`, while
its source remains server-side only.

Passport rejection maps to a normalized 401 with `WWW-Authenticate: Bearer`;
Passport forbidden maps to 403; malformed cookie requests map to 400; and
MADS-owned internal failures map to redacted 500 responses. User-created
`Unauthorized` does not claim a Bearer scheme, and ordinary native responses
are not normalized.

Persistence conversion remains opt in. With both HTTP and database features,
`DatabaseResult<T>` and native Diesel `QueryResult<T>` acquire `.into_http()`.
Typed `NotFound` becomes safe 404 and a typed unique violation becomes safe
409; configuration, pool, migration, foreign-key, check, serialization, and
all other failures become redacted 500. There is no blanket
`From<DatabaseError> for HttpError`, so applications can retain native Diesel
behavior or choose a domain-specific `map_err` mapping.

## Root scope and normal runtime

`#[module(imports = [...])]` selects the root application and direct-import
graph. A descriptor belongs to its nearest annotated Rust namespace. Across
modules, dependencies require a directly imported module and ordinary `pub`
visibility; imports are not transitive. A builder without `root::<AppModule>()`
retains complete-catalog compatibility behavior.

~~~text
conventional config (standard run only)
  -> root module graph and scoped requirements
  -> official auto-configuration evaluation
  -> virtual graph and route/guard validation
  -> provider construction and router finalization
  -> lifecycle startup -> bind -> serve -> reverse shutdown
~~~

Preflight failures never start lifecycle hooks or bind a listener. The raw
generated router is available through `build_router`; merge native routes first,
then apply `configure_router` or `serve_router` so CORS is finalised once as the
outermost layer.

## CLI, inspection, and scaffolding boundary

`mads run` and `mads dev` stream Cargo, rustc, and application output unchanged
and do not accept JSON wrapping. `mads routes`, `mads graph`, and `mads doctor`
compile the standard entry point and receive private child inspection metadata
before normal application construction. The child protocol remains private;
the CLI converts it to a public human report or schema-version-1 JSON result.
Invalid route/graph reports preserve safe partial public data with diagnostics.

Finite commands (`new`, `routes`, `graph`, `doctor`, and `db generate`, `db
migrate`, `db rollback`, `db status`) accept `--format human|json` before or
after their command path. JSON stdout has exactly one newline-terminated
document with `schema_version: 1`, a canonical command, `ok`, command-specific
data or null, and ordered warning/error diagnostics. Schema version 1 permits
additive fields only; breaking field changes require a new version.

`mads new <name>` bundles the fixed seven-file starter and validates all input
before private sibling staging. One atomic rename publishes the destination;
pre-existing paths and failed staging remain untouched. It is offline and does
not run Cargo, install dependencies, initialise Git, select a template, or
generate database/JWT/cookie/migration code.

## Deliberate non-goals

v0.8 does not add automatic validation to native extractors, asynchronous or
database-backed derive validation, full-RFC or DNS email validation, automatic
persistence-to-HTTP conversion, new configuration sources or arbitrary TOML
shapes, global configuration discovery, generic compiler-diagnostic rewriting,
JSON wrapping for run/dev streams, additional generators, or starter database,
JWT, cookie, migration, and Git setup. Trait/interface bindings, login,
credential validation, password hashing, CSRF, remote JWKS, JWE, MySQL/SQLite,
multiple listeners, TLS, and HTTP/2-specific configuration remain
application-owned or later work.
