# mads-cli

`mads-cli` builds the Cargo-native `mads` developer executable. It is the
project/tooling boundary of MADS.rs: it selects Cargo packages and binaries,
runs applications, inspects application graphs, supervises development loops,
manages file migrations, and renders offline project scaffolds.

Application code does not depend on this library crate directly. Install or
run the produced `mads` binary; contributors work in `crates/mads-cli/src`.

## Command responsibilities

| Command area | Responsibility |
| --- | --- |
| `mads new <name>` | Validate a project name, render exactly seven files, and publish the project atomically without network or Cargo side effects. |
| `mads run` | Resolve the selected package/binary through Cargo metadata, build it, and execute the emitted artifact while preserving application streams. |
| `mads dev` | Watch relevant files, debounce changes, rebuild when needed, restart safely, and keep the last successful process alive across compile failures. |
| `mads routes` | Inspect selected route metadata without normal provider construction or server startup. |
| `mads graph` | Inspect rooted modules, providers, dependencies, and construction order. |
| `mads doctor` | Render grouped health/diagnostic evidence for server, database, graph, routes, and auto-configuration. |
| `mads db migrate/rollback/status` | Apply, revert, and inspect file-based PostgreSQL migrations. |
| `mads db generate` | Parse supported Diesel schema declarations, compute a bounded reversible diff, and write a review-required migration without applying it. |

Finite commands default to human output and can request schema-version-1 JSON.
`run` and `dev` intentionally keep streamed Cargo/compiler/application output
instead of wrapping it in that finite-command envelope.

## Tool architecture

The CLI follows Cargo's project model instead of inventing a second package
model:

~~~text
Cargo metadata
      │
      ▼
package/binary selection
      │
      ▼
Cargo build + artifact discovery
      │
      ├── run/dev → process supervisor
      └── routes/graph/doctor → private child inspection protocol
~~~

Package and binary selection honors the current package, `default-run`, single
binary behavior, and explicit `-p`/`--package`/`--bin` selectors. The CLI
forwards arguments only for `run` and `dev`.

Inspection builds the selected application and sends a token-bound private
request through the standard MADS entry point. The child loads read-only
configuration, analyzes its rooted graph and HTTP scope, writes a versioned
report, and returns before provider construction, lifecycle hooks, database
access, socket binding, or request handling.

The development loop is a state machine around a `notify` event adapter,
Cargo's incremental build, and a Tokio process supervisor. Configuration-only
changes can restart the process without a rebuild; compile failures preserve
the last good process. Shutdown and replacement use the standard MADS private
protocol so the behavior remains cross-platform.

Database commands reuse the selected package root. File migrations and
`mads db generate` use the existing MADS database boundary and
`diesel_table_macro_syntax`; unsupported schema objects are surfaced for manual
review rather than guessed. The scaffold renderer validates and renders in
memory, stages beside the destination, and publishes a complete project
atomically.

## Dependencies and consumers

Direct first-party dependencies:

- `mads` for the public runtime, facade exports, and normal application build.
- `mads-common` with default features disabled and `http` enabled for private
  inspection contracts.

Important external dependencies:

- `cargo_metadata` and `semver` for Cargo package/binary resolution.
- `notify` for cross-platform file watching.
- `diesel_table_macro_syntax` for supported schema parsing.
- `serde` and `serde_json` for reports and schema-version-1 JSON output.
- `syn` for scaffold/schema syntax handling.
- `tokio` for process supervision and async orchestration.
- `rustix` for platform-sensitive filesystem/process support.
- `tempfile` for isolated staging and test projects.

This crate produces the `mads` executable and has no first-party dependents.
The implementation depends on the facade and common inspection contract, not
on application internals.

## Source layout

- `src/main.rs` — binary entry point delegating to the library.
- `src/command.rs` — command grammar, selectors, formats, and exit classes.
- `src/project.rs` and `src/cargo.rs` — Cargo metadata, package selection,
  builds, and artifact discovery.
- `src/process.rs`, `src/dev.rs`, `src/dev_state.rs`, and `src/watch.rs` —
  process lifecycle and development supervision.
- `src/inspection.rs` — private child protocol and report acquisition.
- `src/output/` and `src/render.rs` — human output, JSON v1 records, paths,
  routes, graph, and doctor rendering.
- `src/database/` — migration commands, schema loading, diffing, SQL rendering,
  and atomic publication.
- `src/scaffold/` — names, templates, validation, staging, and publication.
- `src/diagnostic.rs` — CLI-owned diagnostics and redaction.

## Tests and contributor workflow

Run focused CLI tests with:

~~~sh
cargo test -p mads-cli
~~~

The CLI has platform-sensitive parser, path, scaffold, process, and JSON tests.
CI runs the command matrix on Linux, macOS, and Windows. Keep Cargo/rustc
output streaming for `run`/`dev`, keep finite JSON stdout machine-clean, and
preserve exit classes 0 (success), 1 (operational failure), and 2 (usage or
syntax failure).

See the [CLI reference](../../docs/CLI.md), the
[workspace contribution guide](../../CONTRIBUTING.md), and the
[inspection architecture](../../docs/ARCHITECTURE.md).
