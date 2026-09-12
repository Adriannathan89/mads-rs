# mads-core-macros

`mads-core-macros` is the procedural-macro implementation crate for
[mads-core](../mads-core/README.md). It translates user declarations into
typed constructors, static metadata, and small runtime adapters.

Application authors should use the re-exports from `mads-core` or `mads`. This
crate is an implementation dependency, not a normal application dependency.

## Macros provided

| Macro | Generated contract |
| --- | --- |
| `#[module]` | Declares a module node, records its Rust namespace, and records direct module imports. |
| `#[provider]` | Turns a provider function into a typed constructor and provider descriptor. |
| `#[service]` | Declares an application-scoped managed service and its dependency metadata. |
| `#[repository]` | Declares an application-scoped repository with the same concrete-type wiring model. |
| `#[derive(Configuration)]` | Generates a typed, prefix-aware view over the core `Config` document. |
| `#[mads::main]` | Converts an async application entry point into a synchronous Tokio-backed entry point when the runtime feature is enabled. |

Provider, service, and repository arguments become concrete dependency edges.
Managed handles are required to be cloneable and shareable so the constructed
application can retain cheap, safe references. The generated code submits
descriptors to the core catalog and keeps source locations available for
diagnostics.

The macros parse declaration shape and generate metadata. They do not own graph
selection, auto-configuration policy, provider construction order, or
lifecycle semantics; those rules belong to `mads-core`.

## Dependency boundary

This crate is a `proc-macro` crate with no first-party runtime dependency and
no feature-specific API. Its direct dependencies are:

- `proc-macro-crate` for resolving the downstream core/facade crate name.
- `proc-macro2` for token streams.
- `quote` for generated Rust code.
- `syn` for parsing Rust syntax and visiting generated/declaration types.

Only `mads-core` depends on this crate. The public `mads` facade reaches the
macros through `mads-core` re-exports.

## Source layout

- `src/lib.rs` — macro exports and shared expansion helpers.
- `src/module.rs` — module declarations and import metadata.
- `src/managed.rs` and `src/provider.rs` — managed type/function expansion and
  dependency extraction.
- `src/configuration/` — `Configuration` derive parsing and validation.
- `src/main.rs` — async entry-point expansion.
- `src/path.rs` — crate-path and generated-path resolution.

Keep syntax diagnostics close to the offending declaration token. Keep semantic
rules that require catalog state, configuration, or graph context in
`mads-core`.

## Tests

Run the focused macro crate tests with:

~~~sh
cargo test -p mads-core-macros
~~~

Consumer-facing compile-fail and compile-pass behavior is tested through the
trybuild fixtures under `crates/mads/tests/ui`, because the generated code must
be checked from the perspective of an external application. Update the
corresponding fixture output when an intentional diagnostic changes.

See the [workspace crate map](../../README.md#workspace-crates) and
[core guide](../mads-core/README.md) for the runtime side of each macro.
