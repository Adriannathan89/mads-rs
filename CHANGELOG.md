# Changelog

All notable changes to MADS.rs are documented in this file.

## [0.8.0-beta.1] - 2026-09-09

Complete beta of the MADS.rs validation, configuration, REST delivery, machine
output, and minimal-project workflow. Stable `0.8.0` will promote this same
feature set after fixes and verification; it will not add features.

### Included

- Offline, atomic `mads new <name>` generation of the exact seven-file minimal
  HTTP application. The generated package starts at `0.1.0`, pins the installed
  MADS version exactly, enables only `http` and `runtime-tokio`, and serves
  `Hello World!` from `GET /`.
- `#[derive(serde::Deserialize, Input)]`, the complete built-in validator
  matrix, custom and manual validation, deterministic sourced issues, and
  `ValidatedJson`, `ValidatedQuery`, and `ValidatedPath` handler boundaries.
- Seven named REST errors with a common safe JSON envelope, normalized
  MADS-owned Passport/cookie/validated-extractor rejections, retained Bearer
  challenges, and redacted internal sources.
- Explicit `.into_http()` conversion for MADS database results and native
  Diesel query results when `http + database` is enabled; there is no automatic
  database-to-HTTP conversion.
- `#[derive(Configuration)]`, explicit `Config::parse`, structured aggregated
  failures, startup-provider validation, preserved source loading/precedence,
  and `Secret<T>` with explicit exposure and redacted formatting.
- Focused stable/MSRV compiler diagnostics for constraints owned by MADS
  macros, while unrelated rustc, Cargo, Axum, Diesel, and application failures
  remain native.
- Optional schema version 1 JSON for `new`, `routes`, `graph`, `doctor`, and all
  four finite database commands, including deterministic records, partial
  inspection data, safe diagnostics, and existing 0/1/2 exit classes.
- Cross-platform policy for portable parser, JSON, path, scaffold, and process
  gates, with complete workspace/package/coverage and PostgreSQL gates on
  Linux.

### Compatibility boundaries

- Human output remains the default. `run` and `dev` keep raw Cargo, rustc, and
  application streams and reject `--format`.
- Native Axum `Json`, `Query`, `Path`, routers, middleware, and responses remain
  available without automatic `Input` validation or MADS normalization.
- Typed configuration reads the existing loaded `Config`; it does not add
  sources, alter precedence, or globally discover derived types.
- The generator adds no database, JWT, cookie, migration, Git, dependency
  installation, remote-template, or additional-generator behavior.
- Publication, tags, GitHub releases, and stable version changes are separate
  release operations.

## [0.7.0] - 2026-09-04

Stable release of the MADS.rs CLI, development loop, and framework
diagnostics.

### Included

- Cargo-native `mads run` and `mads dev` with package, binary, and argument forwarding.
- `mads routes`, `mads graph`, and `mads doctor` compiled application inspection.
- `mads db generate` with automatic naming and recursive split-schema loading.
- `mads db migrate`, `mads db rollback`, and `mads db status` database operations.
- Human-readable diagnostics, stable exit classes, redaction, and Linux CI coverage.
- Bounded PostgreSQL schema diff generation with review-required reversible SQL.

### Release boundaries

- Inspection supports the standard `Mads::run::<AppModule>()` entry point only.
- Unsupported schema details such as defaults, indexes, checks, triggers, and complete foreign-key policy require manual SQL review.
- Output is human-readable only; no machine-readable mode is part of v0.7.
- Input validation, expanded HTTP errors, generic typed configuration, and related validation work are deferred to v0.8.

## [0.7.0-beta.1] - 2026-09-01

Complete beta of the MADS.rs CLI, development loop, and framework diagnostics.

### Included

- Cargo-native `mads run` and `mads dev` with package, binary, and argument forwarding.
- `mads routes`, `mads graph`, and `mads doctor` compiled application inspection.
- `mads db generate` with automatic naming and recursive split-schema loading.
- `mads db migrate`, `mads db rollback`, and `mads db status` database operations.
- Human-readable diagnostics, stable exit classes, redaction, and Linux CI coverage.
- Bounded PostgreSQL schema diff generation with review-required reversible SQL.

### Beta limitations

- Inspection supports the standard `Mads::run::<AppModule>()` entry point only.
- Unsupported schema details such as defaults, indexes, checks, triggers, and complete foreign-key policy require manual SQL review.
- Output is human-readable only; no machine-readable mode is part of v0.7.
- Input validation, expanded HTTP errors, generic typed configuration, and related validation work are deferred to v0.8.

## [0.6.0-beta.1] - 2026-08-29

First public beta of the MADS.rs HTTP application foundation.

### Included

- Root-module application scope and managed dependency construction.
- Typed HTTP route contracts and managed controllers on Axum.
- Conventional HTTP startup, configuration, CORS, and graceful shutdown.
- PostgreSQL/Diesel integration with managed lifecycle and migrations.
- JWT, cookie, Passport strategy, principal, and route guard support.
- Native Axum and Diesel escape hatches for application-owned composition.

### Beta limitations

- Declarative validation, OpenAPI generation, and generic trait bindings are not included.
- TLS, HTTP/2 configuration, multiple listeners, and declarative middleware are application-owned.
- Public APIs may change in later `0.6.0-beta.*` releases based on adopter feedback.

[0.6.0-beta.1]: https://github.com/Adriannathan89/mads/releases/tag/v0.6.0-beta.1
[0.7.0]: https://github.com/Adriannathan89/mads/releases/tag/v0.7.0
[0.7.0-beta.1]: https://github.com/Adriannathan89/mads/releases/tag/v0.7.0-beta.1
