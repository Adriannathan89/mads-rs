# mads-extra

`mads-extra` is the reserved extension boundary for MADS.rs capabilities that
should remain optional and should not enlarge the framework-neutral core or
the standard HTTP/database integration crate.

## Current status

The crate currently exposes only:

~~~rust
pub use mads_core as core;
~~~

There are no extension traits, providers, features, or runtime integrations in
`mads-extra` yet. The `mads` facade exposes it only through the optional
`extra` feature. It is a placeholder boundary, not a source of currently
available application features.

## Dependency boundary

`mads-extra` depends only on `mads-core`. It has no HTTP, database, JWT,
cookie, CLI, or platform dependency. The public facade is its only intended
application-facing route today; future extensions may depend on core and be
re-exported here without forcing them into every MADS application.

Before adding a capability here, decide whether it is:

- framework-neutral and required by every application — consider `mads-core`;
- a standard HTTP/database/auth integration — consider `mads-common`;
- optional tooling or project workflow — consider `mads-cli`;
- genuinely optional and independent of the standard path — consider this crate.

## Source layout

The current implementation is only `src/lib.rs`. Future additions should keep
optional policies isolated, feature-gated, and independent from core internals
where possible.

## Tests

Run the focused crate test with:

~~~sh
cargo test -p mads-extra
~~~

Because the crate is currently a small boundary, most meaningful verification
is done through facade feature tests and the full workspace checks.

See the [workspace crate map](../../README.md#workspace-crates),
[architecture reference](../../docs/ARCHITECTURE.md), and the
[contribution guide](../../CONTRIBUTING.md).
