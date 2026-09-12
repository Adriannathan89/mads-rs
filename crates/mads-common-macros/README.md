# mads-common-macros

`mads-common-macros` contains the procedural macros for the integration layer
implemented by [mads-common](../mads-common/README.md). It generates route
metadata, typed Axum registration adapters, validation traversal, and
Passport metadata.

Application authors should import these macros through `mads::prelude` or
`mads-common` re-exports. This crate is not intended to be a direct
application dependency.

## Macros provided

| Macro | Generated contract |
| --- | --- |
| `#[routes]` | Declares a route-contract trait and immutable route metadata. |
| `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]` | Adds a typed route method declaration and path metadata. |
| `#[controller]` | Binds a managed controller to one or more route contracts and emits a typed registrar. |
| `#[guard]` | Declares route/method policy, inheritance, source, roles, permissions, and predicates. |
| `#[derive(Input)]` | Generates deterministic, transport-independent validation traversal. |
| `#[derive(PassportPrincipal)]` | Generates role/permission accessors for a typed principal. |
| `#[passport_strategy]` | Registers a managed Passport strategy adapter and static strategy metadata. |

`#[routes]` and `#[controller]` keep handler dispatch typed. Handler names in
metadata are used for diagnostics and inspection, not string-based runtime
dispatch. The generated route adapter resolves a controller once from the core
application context.

`#[derive(Input)]` runs after Serde representation conversion at the validated
extractor boundary. It generates ordered issues for supported fields, nested
values, collections, and maps; it does not perform I/O or async validation.

Guard and Passport macros emit static policy information and typed adapters.
Runtime verification, strategy validation, guard inheritance, and safe HTTP
failure mapping belong to `mads-common`.

## Features and dependencies

This is a `proc-macro` crate with no first-party runtime dependency.

| Feature | Meaning |
| --- | --- |
| `passport` | Enables the Passport strategy macro implementation. |
| `cookies` | Enables cookie-related Passport macro support and implies `passport`. |

Direct dependencies:

- `proc-macro-crate` for resolving the downstream integration crate.
- `proc-macro2` for token streams.
- `quote` for generated code.
- `syn` with visitor support for syntax and type traversal.

Only `mads-common` depends on this crate. Its macros are re-exported through
the facade when the relevant public features are enabled.

## Source layout

- `src/lib.rs` — public macro exports and shared expansion helpers.
- `src/routes.rs` and `src/verb.rs` — route contracts, methods, paths, and
  metadata.
- `src/controller.rs` — managed controller binding and registrar generation.
- `src/input/` — validation attributes, Serde paths, and traversal checks.
- `src/guard.rs`, `src/passport_principal.rs`, and
  `src/passport_strategy.rs` — guard and authentication declarations.
- `src/path.rs` — path normalization and generated-path handling.

The macro crate owns declaration syntax and generated code. Keep runtime
selection, graph validation, and error policy in `mads-common`.

## Tests

Run focused tests with:

~~~sh
cargo test -p mads-common-macros
~~~

Consumer-facing compile-fail and compile-pass fixtures live under
`crates/mads/tests/ui`. When a macro expansion changes, test both the syntax
diagnostic and the generated consumer behavior. Follow the feature matrix in
the [common guide](../mads-common/README.md) before changing feature gates.
