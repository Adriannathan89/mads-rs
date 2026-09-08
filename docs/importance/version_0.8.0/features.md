# MADS v0.8.0-beta.1 Feature Contract

Status: integrated beta surface; the final release-candidate gate and
cross-platform/PostgreSQL evidence are recorded separately by the release
plan. This document does not announce publication.

Version `0.8.0-beta.1` contains the complete v0.8 feature set. Stable `0.8.0`
promotes this same surface after fixes, documentation corrections, and full
verification; it does not receive additional features.

## Minimal project generation

`mads new <name>` creates a previously absent child directory containing
exactly:

```text
Cargo.toml
mads.toml
src/main.rs
src/app/mod.rs
src/app/routes.rs
src/app/controller.rs
src/app/service.rs
```

The application package starts at `0.1.0`; its exact MADS dependency matches
the installed CLI and enables only `http` and `runtime-tokio`. `GET /` returns
plain `Hello World!`. Generation is offline, validates the fixed Cargo/Rust
name rules before writing, stages in a private sibling, and publishes with one
atomic rename. It does not add database, JWT, cookies, migrations, Git,
dependency installation, remote templates, or extra generator flags.

Focused evidence: `scaffold_cli.rs`, `scaffold_consumer.rs`,
`scaffold_http.rs`, and the CLI `v080_acceptance.rs` matrix.

## Input validation

`#[derive(serde::Deserialize, Input)]` is available from `mads-common`, the
facade root, and the prelude with `http`. `ValidatedJson`, `ValidatedQuery`,
and `ValidatedPath` run Serde first, then `Input`, attach `body`, `query`, or
`path`, and prevent handler invocation on failure.

Built-ins cover email; inclusive min/max or exact Unicode code-point and
collection length; nonempty; inclusive numeric range; strict positive and
negative; nonzero multiple-of; required options; and nested validation.
Synchronous field/whole-value callbacks and manual `Input` implementations are
the extension points. Structs, enums, tuples, arrays, vectors, options, and
string-keyed maps preserve the specified deterministic traversal order.

Validated input failures use 422 `validation_error` with ordered sourced
issues. Built-in diagnostics never echo rejected values. Native Axum `Json`,
`Query`, and `Path` remain native and do not run `Input`.

Focused evidence: `input_validation.rs`, `input_facade.rs`, `input_ui.rs`, and
the facade `v080_acceptance.rs` matrix.

## REST errors and database delivery

The `http` feature exports seven named errors: `BadRequest`, `Unauthorized`,
`Forbidden`, `NotFound`, `Conflict`, `ValidationError`, and `InternalError`.
MADS-owned failures use one safe JSON envelope; validation alone adds issues.
Passport rejection retains `WWW-Authenticate: Bearer`, and Passport/cookie
internal details remain server-side.

With `http + database`, `DatabaseResult<T>` and native Diesel
`QueryResult<T>` may explicitly call `.into_http()`. Typed not-found maps to
404, typed unique violation maps to 409, and every other database failure maps
to a redacted 500. There is no blanket `From<DatabaseError>` conversion.

Focused evidence: `response.rs`, `database_http.rs`,
`database_http_postgres.rs`, `passport_strategy_errors.rs`, cookie request and
response tests, and `database_http_facade.rs`.

## Typed configuration and secrets

`#[derive(Configuration)]` and `Config::parse::<T>()` provide a typed view over
the already loaded `Config`. Supported values are scalar Rust types,
source-relative paths, optional values, `Secret<T>`, optional secrets,
`Vec<String>`, nested configuration, and explicit scalar `parse_with`
callbacks. Prefixes, renames, defaults, and compatible validators are checked
by the derive. Independent failures retain declaration order, complete dotted
keys, stable codes, and valid provenance without values.

A selected provider makes parsing an explicit startup requirement; an unused
derive has no startup effect. Conventional loading remains optional `.env` for
exact-placeholder interpolation, optional `mads.toml`, then final scalar
`MADS_*` overrides. `Secret<T>` exposes values only through `expose` or
`into_exposed`; ordinary `Display` and `Debug` are always `[REDACTED]`.

Focused evidence: `configuration.rs`, `configuration_facade.rs`,
`configuration_ui.rs`, server lifecycle tests, and the facade acceptance
matrix.

## Diagnostics and machine output

MADS macros now own focused errors for the approved Input, Configuration,
controller-route, module-import, managed dependency, known route-extractor,
and Passport principal constraints. General rustc, Cargo, Axum, Diesel, and
application diagnostics remain authoritative.

Finite commands support optional `--format human|json`: `new`, `routes`,
`graph`, `doctor`, `db generate`, `db migrate`, `db rollback`, and `db status`.
Schema version 1 uses one newline-terminated document containing
`schema_version`, canonical `command`, `ok`, structured `data`, and ordered
error/warning `diagnostics`.
Inspection failure retains trustworthy partial data; failures before a report
use null data. Human output remains the default, while `run` and `dev` keep raw
Cargo/rustc/application streams and reject JSON formatting. Exit classes remain
0 success, 1 operational failure, and 2 syntax failure.

Focused evidence: stable/MSRV UI fixtures, `json_cli.rs`, `command_matrix.rs`,
and the CLI acceptance matrix.

## Compatibility and release boundaries

- Core remains free of Axum, Diesel, JWT, cookies, and Serde.
- Input validation and standard REST errors require `http`.
- Database mapping exists only with `http + database` and is explicit.
- Human CLI output and ordinary Axum/Diesel escape hatches remain available.
- Platform-sensitive CLI/scaffold/process evidence belongs on Linux, macOS,
  and Windows; complete workspace/MSRV/coverage/package and PostgreSQL evidence
  remains Linux-owned.
- Release publication, tagging, pushing, and stable version changes are outside
  this documentation task.

The binding contracts and complete coverage table are in the
[v0.8 release-scope design](../../superpowers/specs/2026-09-06-v0.8.0-release-scope-design.md).
