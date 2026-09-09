# MADS CLI

MADS v0.8.0-beta.1 provides Cargo-native execution, inspection, PostgreSQL
migration commands, a minimal-project generator, and a versioned JSON result
for finite MADS-owned commands. Human-readable output remains the default.

## Start a minimal HTTP application

Create an application outside an existing Cargo project:

```bash
mads new my-app
cd my-app
mads dev
```

`mads new <name>` creates `./<name>` relative to the invocation directory. A
name starts with lowercase ASCII; its remaining characters may be lowercase
ASCII, digits, `-`, or `_`. Rust 2024 keywords and Cargo-reserved package names
are rejected. MADS preserves the supplied directory and package spelling.

The generated project contains exactly these seven files:

```text
<name>/
├── Cargo.toml
├── mads.toml
└── src/
    ├── main.rs
    └── app/
        ├── mod.rs
        ├── routes.rs
        ├── controller.rs
        └── service.rs
```

The manifest starts the application at version `0.1.0`, uses edition 2024 and
Rust 1.85, and pins the installed MADS CLI version exactly. Its MADS dependency
uses `default-features = false` with only `http` and `runtime-tokio`; it has no
database, JWT, cookie, schema, migration, or authentication dependency. The
starter's `GET /` response is plain `Hello World!`.

`mads.toml` contains:

```toml
[server]
host = "127.0.0.1"
port = 3000
```

The normal runtime overrides remain available: `MADS_SERVER__HOST` maps to
`server.host` and `MADS_SERVER__PORT` maps to `server.port`.

Generation validates arguments before writing, renders all files into a private
sibling staging directory, and publishes them with one atomic rename. An
existing destination, including an empty directory, is never changed. The
command does not download dependencies, run Cargo, initialize Git, select a
remote template, or ask an interactive question. It offers no template,
database, JWT, VCS, or target-directory option in v0.8. A successful human
result identifies the relative path and prints only `cd <name>` and `mads dev`.

## Project and target selection

The CLI starts from the current working directory and resolves Cargo metadata.
With one eligible package and binary, selectors are unnecessary. Use
`--package <package>` (or `-p <package>`) and `--bin <binary>` when selection is
ambiguous:

```text
mads run [--package <package>] [--bin <binary>] [-- <app-args>...]
mads dev [--package <package>] [--bin <binary>] [-- <app-args>...]
mads routes [--package <package>] [--bin <binary>]
mads graph [--package <package>] [--bin <binary>]
mads doctor [--package <package>] [--bin <binary>]
```

Database commands accept `--package <package>` or `-p <package>`. Cargo's
ordinary single-package, `default-run`, and ambiguity behavior remains
authoritative. Arguments after `--` are forwarded only by `run` and `dev`;
inspection commands reject them.

## Output formats

The following finite commands accept `--format human|json`:

```text
mads new <name>
mads routes
mads graph
mads doctor
mads db generate
mads db migrate
mads db rollback
mads db status
```

The option may appear once, before or after the command path. Both examples
are equivalent:

```bash
mads --format json routes
mads routes --format json
mads --format json db status
mads db status --format json
```

`human` is the default. `run`, `dev`, help, version, and database help reject
`--format` because they are human/streaming interfaces. A duplicate, missing,
or unknown format value is CLI syntax failure `MADS204`.

In JSON mode stdout contains exactly one JSON document followed by one newline;
MADS writes no rendered warning or error text there. Cargo and rustc output
required to build inspection targets still passes through stderr. JSON paths use
`/` and are package-relative when possible.

Every document has this version-1 envelope:

```json
{
  "schema_version": 1,
  "command": "routes",
  "ok": true,
  "data": {},
  "diagnostics": []
}
```

`command` is the canonical spelling (`new`, `routes`, `graph`, `doctor`, `db
generate`, `db migrate`, `db rollback`, or `db status`) and is `null` only when
syntax cannot identify a command. `ok` is true only for exit-zero MADS-owned
completion. `data` is the command object, safe partial inspection data, or
`null`. `diagnostics` is an ordered list of MADS-owned records:

```json
{
  "severity": "error",
  "code": "MADS204",
  "title": "invalid command",
  "message": "...",
  "subject": null,
  "location": null,
  "suggestions": []
}
```

Severity is always `error` or `warning`; nullable `subject` and `location` are
intentional. Schema version 1 may add fields, and consumers must ignore unknown
object fields. Removing, renaming, changing the type of, or changing the
meaning of an existing field requires a new `schema_version`.

The finite schema owners are `new`, `routes`, `graph`, `doctor`, `db generate`,
`db migrate`, `db rollback`, and `db status`. A non-null source location has
one-based line and column numbers:

```json
{"file":"src/app/routes.rs","line":6,"column":5}
```

### JSON command data

`new` returns the project name, relative path, and this ordered file list:

```json
{
  "project_name": "my-app",
  "path": "my-app",
  "files": [
    "Cargo.toml",
    "mads.toml",
    "src/main.rs",
    "src/app/mod.rs",
    "src/app/routes.rs",
    "src/app/controller.rs",
    "src/app/service.rs"
  ]
}
```

`routes` returns `{ "routes": [...] }`; every route record has `method`,
`path`, `route_trait`, `handler`, `controller`, `location`, and
`guard_active`. Route order remains method, path, controller, route trait, and
handler order. `graph` returns `root_module`, `modules`, `imports`,
`providers`, `dependencies`, and nullable `construction_order`. Module records
contain `type_name`, `namespace`, and `location`; import records contain
`importer` and `imported`; providers retain `type_name`,
nullable owner and location, origin, visibility, and state; dependencies carry
`provider` and `dependency` names. `construction_order` is `null` when no valid
construction plan exists.

```json
{
  "routes": [{
    "method": "GET",
    "path": "/",
    "route_trait": "AppRoutes",
    "handler": "hello",
    "controller": "AppController",
    "location": {"file":"src/app/routes.rs","line":6,"column":5},
    "guard_active": false
  }]
}
```

```json
{
  "root_module": "AppModule",
  "modules": [{
    "type_name": "AppModule",
    "namespace": "crate::app",
    "location": {"file":"src/app/mod.rs","line":8,"column":1}
  }],
  "imports": [],
  "providers": [],
  "dependencies": [],
  "construction_order": []
}
```

`doctor` returns `{ "checks": [...] }`, where each check contains `group`,
`status`, and `summary`. Status is `pass`, `skipped`, `overridden`, or `failed`;
the existing group and summary ordering remains authoritative.

```json
{
  "checks": [{
    "group": "configuration",
    "status": "pass",
    "summary": "configuration sources are valid"
  }]
}
```

Database data is deliberately concise:

```json
{"status":"generated","migration_path":"migrations/20260906120000_schema_diff","review_required":true}
```

No-diff `db generate` uses `status: "up_to_date"`, a null `migration_path`,
and `review_required: false`. The remaining schemas are:

```json
{"applied":["20260906120000_schema_diff"]}
```

```json
{"reverted":["20260906120000_schema_diff"]}
```

```json
{"applied":["20260906120000_schema_diff"],"pending":["20260907120000_add_index"]}
```

These represent `db migrate`, `db rollback`, and `db status` respectively.
Version arrays preserve report order. Migration review warnings occur only as
top-level warning diagnostics and are not duplicated inside `data`.

Invalid route or graph inspection retains every trustworthy record in `data`,
adds ordered error diagnostics, sets `ok` false, and exits 1. A failure before a
report exists, scaffold publication failure, or database operational failure
uses `data: null`. JSON syntax failure requested through a recognized format
uses `MADS204`, `ok: false`, `data: null`, and exit 2.

## `mads run` and `mads dev`

`mads run` builds the selected binary and forwards arguments after `--`. It
preserves an ordinary application exit status. `mads dev` builds, supervises,
and watches the selected application's reachable workspace inputs. Changes are
debounced; a failed rebuild keeps the last good process when one is running.
Neither command wraps Cargo, rustc, or arbitrary application streams in JSON.

```bash
mads run -- --seed-data
mads run -p api --bin server -- --port 4000
mads dev
mads dev -p api --bin server -- --log=debug
```

## Inspection commands

`mads routes`, `mads graph`, and `mads doctor` compile the selected standard
`Mads::run::<AppModule>()` application and obtain private inspection metadata
without normal provider construction, lifecycle startup, database connection,
migration, listener binding, or traffic serving. Human output remains the
existing table/section/check rendering; JSON exposes only the public schema
described above, never the private inspection protocol or its tokens.

## Database commands

`mads db generate` creates one automatic timestamp-named, review-required
schema diff. It never applies the migration and has no positional migration
name. `mads db migrate`, `mads db rollback`, and `mads db status` operate on
the selected package's file-based `migrations/` directory and configured
PostgreSQL database. Normal application startup does not generate or apply
file migrations.

The bounded schema planner supports the documented Diesel table/column shape;
defaults, indexes, checks, triggers, and complete foreign-key policy remain
manual SQL review items. `--diff-schema` is not an accepted argument.

## Diagnostics and exit codes

| Code | Meaning |
| --- | --- |
| 0 | Command completed successfully. |
| 1 | Build, Cargo resolution, inspection, database, scaffold filesystem, watcher, or other operational failure. |
| 2 | Invalid MADS CLI syntax, output-format selection, project name, or unsupported argument. |

`MADS204` identifies syntax or output-format failures. `MADS230` identifies
project-name, template rendering, staging, or publication failures. Existing
diagnostic families retain their meanings, and migration review diagnostics are
warnings in top-level JSON `diagnostics`, not duplicated in `data`.

Operational diagnostics redact configuration values, credentials, URLs, private
inspection tokens, and arbitrary source error text. Human output is the default
compatibility surface; JSON is the stable machine-readable surface for the
finite commands only.
