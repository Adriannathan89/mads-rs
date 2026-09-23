# Contributing to MADS.rs

Thank you for contributing to MADS.rs. Contributions are made through a local
clone and submitted as pull requests to `develop`. Do not work directly on
`main`, `beta`, or `develop`. The `beta` and `main` branches are managed only by
the maintainers.

## Before you start

Read the project documentation before changing code. This is important because
MADS.rs has deliberate crate boundaries, feature relationships, startup rules,
and compatibility requirements that may not be obvious from one source file.

Start with:

- [`README.md`](README.md) for the public API, supported features, and usage;
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for crate boundaries and
  framework design;
- [`docs/mads-persistence.md`](docs/mads-persistence.md) and
  [`docs/CLI.md`](docs/CLI.md) for current persistence and CLI guidance;
- the relevant files under [`docs/`](docs/) for examples, release decisions,
  historical context, and acceptance requirements;

When documentation disagrees with current code or configuration, mention it in
the pull request instead of silently relying on an assumption. Documents under
`docs/superpowers/` are design history or proposals; use
the current source, `README.md`, `docs/ARCHITECTURE.md`, and CI configuration to
confirm present behavior.

## Clone the repository

For a small change, you may clone the repository directly:

```sh
git clone https://github.com/Adriannathan89/mads.git
cd mads
```

If you do not have permission to push branches to the repository, fork it on
GitHub first and clone your fork instead:

```sh
git clone https://github.com/<your-username>/mads.git
cd mads
git remote add upstream https://github.com/Adriannathan89/mads.git
```

For a fork, keep your local base branch current before starting work:

```sh
git fetch upstream
git switch develop
git pull --ff-only upstream develop
```

For a direct clone, use `origin` in place of `upstream`. Contributors must use
`develop` as their base branch; only maintainers promote changes to `beta` and
`main`.

## Create a branch

Create a focused branch from the pull request's target branch:

```sh
git switch -c feature/short-description
```

Use a descriptive prefix such as `feature/`, `fix/`, `docs/`, `test/`, or
`chore/`. Keep each branch and pull request limited to one coherent change.

## Make the change

MADS.rs is a Rust 2024 workspace with a minimum supported Rust version of
1.94. Follow standard `rustfmt` output and these repository conventions:

- use four-space indentation;
- use `snake_case` for modules and functions, `UpperCamelCase` for types and
  traits, and `SCREAMING_SNAKE_CASE` for constants;
- do not introduce unsafe code;
- document public APIs;
- preserve the dependency layering described in `docs/ARCHITECTURE.md`;
- add focused tests in the crate that owns the behavior;
- use `trybuild` fixtures, including matching `.stderr` files, for procedural
  macro acceptance and diagnostic tests;
- update public documentation when an API, configuration rule, feature, or
  architecture boundary changes.

The main workspace responsibilities are:

- `crates/mads-core`: framework-neutral construction, configuration, provider
  graph, lifecycle, diagnostics, and auto-configuration decisions;
- `crates/mads-core-macros`: core procedural macros;
- `crates/mads-common`: HTTP, routes, Passport/JWT, cookies, CORS, and logging;
- `crates/mads-persistence`: opt-in native SeaORM PostgreSQL integration;
- `crates/mads-common-macros`: shared route-related procedural macros;
- `crates/mads`: stable public facade;
- `crates/mads-cli`: command-line interface;
- `crates/mads-extra`: reserved extension boundary.

## Validate the change

Run the most focused relevant test while developing. Before opening a pull
request, run the same primary gates used by CI from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --all-features --doc
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

Also check the minimum supported toolchain when your change may affect
compatibility:

```sh
rustup run 1.94.0 cargo test --locked --workspace --all-features
```

Coverage contributors can run:

```sh
cargo llvm-cov --workspace --all-features \
  --ignore-filename-regex '(^|/)tests/ui/' \
  --fail-under-lines 85
```

PostgreSQL integration tests require a running PostgreSQL instance and the
`MADS_TEST_DATABASE_URL` environment variable. These tests are ignored during
ordinary local runs and executed separately by CI. Never commit `.env` files,
credentials, private keys, tokens, or database URLs.

## Commit and push

Use a concise, imperative Conventional Commit-style subject. Examples:

```text
feat(routes): add typed header extraction
fix(passport): reject duplicate guard cookies
docs: clarify module visibility
chore: update CI configuration
```

Then push your branch:

```sh
git push -u origin feat/short-description
```

## Open a pull request

Open every contribution pull request from your branch into upstream `develop`.
Do not open contribution pull requests into `beta` or `main`; those branches
are reserved for maintainer-managed promotion and releases. The pull request
should:

- explain the problem and the behavior changed;
- describe important design decisions or tradeoffs;
- list the validation commands you ran and their results;
- link related issues;
- call out breaking changes, configuration changes, migrations, or follow-up
  work;
- include documentation updates for public API or architecture changes;
- include screenshots only for user-facing CLI or visual changes.

Address review feedback with additional commits, push them to the same branch,
and keep the discussion in the pull request. Maintainers will merge the pull
request after review and required checks pass.

## Publishing a beta

The workspace uses one prerelease version for all crates. Internal path
dependencies also carry an exact crates.io version so packaged crates resolve
to the matching release outside this repository.

To publish a beta:

1. Set `[workspace.package].version` and every internal dependency version to
   the same prerelease, for example `0.6.0-beta.2`.
2. Update `Cargo.lock`, `README.md`, examples, CLI version assertions, and
   `CHANGELOG.md`.
3. Run the complete validation commands above.
4. Promote the release commit to the protected `beta` branch.

Every push to `beta` runs `.github/workflows/beta-publish.yml`. After its
release gates pass, it publishes crates in dependency order. A crate/version
already present on crates.io is skipped, making documentation-only pushes
idempotent. Availability checks use Cargo's registry client, and each successful
`cargo publish` already waits for the crate to enter the registry index before
the next dependent crate is published. The publish job requires a GitHub
repository or `beta` environment secret named `CRATES_IO_TOKEN`. Store a
crates.io API token in that secret; never commit or print the token.

After every crate is available on crates.io, the same job creates the matching
`v<version>` tag at the tested `beta` commit and publishes a GitHub prerelease
using that version's `CHANGELOG.md` section. Existing releases are skipped so
a rerun can safely recover after a partial registry publication.

Publishing to crates.io is permanent. Increment the prerelease suffix before
publishing changed package contents; an existing crate version cannot be
overwritten.
