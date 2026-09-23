# MADS.rs v0.9.0 Native SeaORM Persistence Implementation Plan

**Superseded for the 0.9 database/CLI boundary by**
[`2026-09-23-mads-0.9-database-surface-removal.md`](2026-09-23-mads-0.9-database-surface-removal.md).
This document remains the historical connector implementation record.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicitly imported `mads-persistence` crate that injects one native SeaORM PostgreSQL connection, verifies it before serving, closes it during graceful shutdown, and preserves existing provider APIs.

**Architecture:** `mads-core` adds an optional lifecycle constructor beside the unchanged ordinary provider constructor, and `#[provider(lifecycle)]` emits that metadata only for two narrow async return shapes. `mads-persistence` owns configuration, safe errors, the SeaORM connector, a global `DatabaseModule`, and the SeaORM lifecycle hook; applications receive the native `DatabaseConnection`. Release work moves framework crates to 0.9.0 while `mads-cli` remains 0.8.0.

**Tech Stack:** Rust 2024, Rust 1.94, Tokio 1, SeaORM 2.0, SQLx PostgreSQL, Rustls, PostgreSQL 16, `inventory`, `syn`, `quote`, `trybuild`, GitHub Actions

**Spec:** `docs/superpowers/specs/2026-09-23-mads-persistence-design.md`

## Global Constraints

- Keep the workspace on Rust edition 2024 and `rust-version = "1.94"`.
- Release `mads-core-macros`, `mads-common-macros`, `mads-core`, `mads-extra`, `mads-common`, `mads`, and `mads-persistence` as 0.9.0.
- Keep `mads-cli` at 0.8.0 and update only its exact internal framework dependency pins to 0.9.0.
- Keep `ProviderFuture`, `ProviderConstructor`, `ProviderDescriptor::new`, and ordinary `#[provider]`, `#[service]`, and `#[repository]` behavior source-compatible.
- Accept `#[provider(lifecycle)]` only on async functions returning `LifecycleResource<T>` or `mads_core::Result<LifecycleResource<T>>`.
- Keep `mads-core` free of SeaORM, SQLx, Axum, `mads-common`, and `mads-persistence` normal dependencies.
- Keep `mads-persistence` free of Axum and `mads-common` normal dependencies.
- Enable SeaORM default features only through the explicit `sea-orm-postgres` feature, which selects `macros`, `sqlx-postgres`, and `runtime-tokio-rustls`.
- Accept exactly `postgres://` and `postgresql://` as PostgreSQL URL schemes before calling SeaORM.
- Never include configuration values, URLs, credentials, query parameters, SQL, bindings, entity values, or arbitrary driver messages in MADS-owned `Debug`, `Display`, diagnostics, inspection, or startup output.
- Preserve existing lifecycle order, rollback, shutdown continuation, and first-shutdown-failure behavior.
- Do not add migrations, MySQL, SQLite, named connections, an ORM wrapper, automatic HTTP conversion, or a `mads` facade dependency on `mads-persistence`.
- Preserve `#![forbid(unsafe_code)]`, public missing-doc denial, stable/MSRV builds, and the 85% line-coverage gate.
- Follow red-green-refactor for behavior changes and use the commit subjects specified by each task when committing is authorized.

## Review Focus

- A URL or driver error containing `mads-secret-user:mads-secret-password` must never appear in any MADS-owned formatting; Tasks 5, 6, and 8 add source-chain and captured-output tests.
- Omitting `DatabaseModule` must leave the SeaORM provider unselected and must perform no constructor or network work; Task 7 adds graph-analysis and empty-configuration build tests.
- A lifecycle resource followed by a failing provider must never start a contributed hook, and dropping the failed builder must release the value; Task 4 adds this rollback-boundary test.
- `postgresql://` must be accepted while MySQL, SQLite, malformed, and missing schemes fail before the SeaORM call; Task 6 adds a table-driven validator test.
- A SeaORM close failure must not prevent later reverse-order hooks from being attempted, and the first failure must remain primary; Tasks 4 and 8 pin this behavior.

---

## File Map

```text
Cargo.toml                                             workspace 0.9 version, member, SeaORM dependency
Cargo.lock                                             resolved framework and SeaORM graph

crates/mads-core/src/lifecycle.rs                      LifecycleResource and registrations
crates/mads-core/src/descriptor.rs                     optional lifecycle constructor metadata
crates/mads-core/src/builder.rs                        shared contribution invocation/application
crates/mads-core/src/lib.rs                            public and hidden macro-facing exports
crates/mads-core/tests/descriptor.rs                   ordinary descriptor compatibility
crates/mads-core/tests/provider_lifecycle.rs           build/construct/order/failure behavior

crates/mads-core-macros/src/provider.rs                lifecycle argument/signature expansion
crates/mads-core-macros/tests/support/provider.rs      parser and token-expansion unit tests
crates/mads/tests/ui/pass/provider_lifecycle.rs        public compile-pass contract
crates/mads/tests/ui/fail/provider_lifecycle_*.rs      focused compile failures
crates/mads/tests/ui/fail/provider_lifecycle_*.stderr  stable/MSRV diagnostics

crates/mads-persistence/Cargo.toml                     crate metadata and connector feature
crates/mads-persistence/README.md                      package documentation
crates/mads-persistence/src/lib.rs                     root exports
crates/mads-persistence/src/error.rs                   MADS140 and redacted typed errors
crates/mads-persistence/src/factory.rs                 connector trait and DatabaseFactory
crates/mads-persistence/src/sea_orm/mod.rs             SeaORM re-export and DatabaseModule
crates/mads-persistence/src/sea_orm/config.rs          typed settings and relationship validation
crates/mads-persistence/src/sea_orm/connector.rs       PostgreSQL scheme check and native connect
crates/mads-persistence/src/sea_orm/lifecycle.rs       ping and close hook
crates/mads-persistence/tests/module.rs                graph, opt-in, duplicate, inspection tests
crates/mads-persistence/tests/postgres.rs              real PostgreSQL CRUD/transaction/lifecycle
crates/mads-persistence/examples/standard_run.rs       conventional startup child-process fixture

crates/mads-cli/Cargo.toml                             retained 0.8.0 package, 0.9 framework pins
crates/mads-cli/tests/release_automation.rs            version, feature, publish-order assertions
crates/mads-cli/tests/command_matrix.rs                PostgreSQL workflow command matrix
script/release.sh                                      persistence package version updates
script/verify-package-contents.sh                      persistence archive policy
.github/workflows/ci.yml                               feature, minimum, PostgreSQL gates
.github/workflows/beta-publish.yml                     0.9 gates and framework-only publish order
.github/workflows/stable-publish.yml                   0.9 gates and framework-only publish order

README.md                                              connector quick start and crate table
docs/ARCHITECTURE.md                                   native connector and lifecycle boundary
docs/mads-persistence.md                               approved decisions synchronized
CHANGELOG.md                                           0.9 release contract
```

---

### Task 1: Establish the 0.9 framework version baseline

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/mads-core/Cargo.toml`
- Modify: `crates/mads-common/Cargo.toml`
- Modify: `crates/mads-extra/Cargo.toml`
- Modify: `crates/mads/Cargo.toml`
- Modify: `crates/mads-cli/Cargo.toml`
- Test: `crates/mads-cli/tests/release_automation.rs`

**Interfaces:**

- Consumes: workspace version 0.8.1, CLI package version 0.8.0, exact internal path pins.
- Produces: framework version 0.9.0, unchanged CLI package version 0.8.0, exact 0.9.0 internal pins used by every later task.

- [ ] **Step 1: Change the release-version assertion first**

Rename `framework_packages_use_v081_pins_while_cli_remains_v080` to
`framework_packages_use_v090_pins_while_cli_remains_v080`, set
`VERSION` to `0.9.0`, and add `mads-persistence` to `FRAMEWORK_PACKAGES` only
after Task 5 creates its manifest. For this task, keep the expected framework
list to the six existing framework packages.

- [ ] **Step 2: Run the focused assertion and verify it fails**

Run:

```bash
cargo test -p mads-cli --test release_automation framework_packages_use_v090_pins_while_cli_remains_v080 -- --exact
```

Expected: FAIL because the workspace and internal pins still contain `0.8.1`.

- [ ] **Step 3: Update the version baseline**

Set `[workspace.package].version = "0.9.0"`. Change every existing internal
path dependency pin from `=0.8.1` to `=0.9.0`. Keep this exact CLI package
declaration:

```toml
[package]
name = "mads-cli"
version = "0.8.0"
```

Do not add a `version.workspace` key to the CLI manifest.

- [ ] **Step 4: Refresh the lockfile and rerun the focused assertion**

Run:

```bash
cargo check --workspace --all-targets
cargo test -p mads-cli --test release_automation framework_packages_use_v090_pins_while_cli_remains_v080 -- --exact
```

Expected: PASS, and `Cargo.lock` records the six framework packages at 0.9.0
and `mads-cli` at 0.8.0.

- [ ] **Step 5: Commit the version baseline**

```bash
git add Cargo.toml Cargo.lock crates/*/Cargo.toml crates/mads-cli/tests/release_automation.rs
git commit -m "chore: prepare framework 0.9 version line"
```

---

### Task 2: Add lifecycle-resource and descriptor contribution contracts

**Files:**

- Modify: `crates/mads-core/src/lifecycle.rs`
- Modify: `crates/mads-core/src/descriptor.rs`
- Modify: `crates/mads-core/src/lib.rs`
- Modify: `crates/mads-core/tests/descriptor.rs`
- Test: `crates/mads-core/tests/lifecycle.rs`

**Interfaces:**

- Consumes: `ErasedProvider`, `LifecycleHook`, existing infrastructure/application grouping, and unchanged `ProviderConstructor`.
- Produces: `LifecycleResource<T>`, document-hidden `ProviderContribution`, `LifecycleProviderFuture`, `LifecycleProviderConstructor`, `ProviderDescriptor::with_lifecycle_constructor`, and a registration handoff used by Task 4.

- [ ] **Step 1: Write descriptor compatibility and lifecycle metadata tests**

Extend `crates/mads-core/tests/descriptor.rs` with a fake lifecycle constructor:

```rust
use mads_core::{
    LifecycleProviderFuture, LifecycleResource, ProviderContribution,
};

fn lifecycle_constructor<'a>(_: &'a ConstructionContext<'a>) -> LifecycleProviderFuture<'a> {
    Box::pin(async {
        Ok(ProviderContribution::from_resource(LifecycleResource::new(Output)))
    })
}

#[test]
fn ordinary_descriptor_contract_remains_unchanged_and_lifecycle_is_additive() {
    let plain = ProviderDescriptor::new(
        ProviderKind::Provider,
        "descriptor::Output",
        output_type_id,
        &[],
        ProviderVisibility::Public,
        SourceLocation::new("provider.rs", 1, 1),
        output_constructor,
    );
    assert!(plain.lifecycle_constructor().is_none());

    let lifecycle = plain.with_lifecycle_constructor(lifecycle_constructor);
    assert!(lifecycle.lifecycle_constructor().is_some());
}
```

Add unit tests in `lifecycle.rs` proving repeated
`with_infrastructure_hook`/`with_application_hook` calls preserve authored
registration order and that conversion erases `T` under its native type.

- [ ] **Step 2: Run the focused tests and observe missing APIs**

Run:

```bash
cargo test -p mads-core --test descriptor
cargo test -p mads-core lifecycle::tests
```

Expected: compile failure because lifecycle provider types and descriptor
metadata do not exist.

- [ ] **Step 3: Implement lifecycle resource and contribution types**

Add these exact public shapes, with full rustdoc, to `lifecycle.rs` and
`descriptor.rs`:

```rust
pub struct LifecycleResource<T> {
    value: T,
    registrations: Vec<LifecycleRegistration>,
}

#[doc(hidden)]
pub struct ProviderContribution {
    provider: ErasedProvider,
    registrations: Vec<LifecycleRegistration>,
}

#[doc(hidden)]
pub type LifecycleProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderContribution>> + Send + 'a>>;

#[doc(hidden)]
pub type LifecycleProviderConstructor =
    for<'a> fn(&'a ConstructionContext<'a>) -> LifecycleProviderFuture<'a>;
```

`LifecycleRegistration` stays crate-private and stores either
`Infrastructure { owner, hook }` or `Application { hook }`. Implement:

```rust
impl<T> LifecycleResource<T> {
    pub fn new(value: T) -> Self;
    pub fn with_infrastructure_hook<H>(self, owner: &'static str, hook: H) -> Self
    where H: LifecycleHook + 'static;
    pub fn with_application_hook<H>(self, hook: H) -> Self
    where H: LifecycleHook + 'static;
}

impl ProviderContribution {
    #[doc(hidden)]
    pub fn from_provider(provider: ErasedProvider) -> Self;

    #[doc(hidden)]
    pub fn from_resource<T>(resource: LifecycleResource<T>) -> Self
    where T: Send + Sync + 'static;

    pub(crate) fn into_parts(self) -> (ErasedProvider, Vec<LifecycleRegistration>);

    #[doc(hidden)]
    pub fn into_provider(self) -> ErasedProvider;
}
```

Add `lifecycle_constructor: Option<LifecycleProviderConstructor>` to
`ProviderDescriptor`; `new` sets `None`; add document-hidden const setter and
getter. Do not alter the existing constructor field, aliases, or getter.

- [ ] **Step 4: Add lifecycle manager registration ingestion**

Add a crate-private `LifecycleManager::add_registration` that converts the
new registration enum into the existing `RegisteredHook` representation while
assigning the next sequence number. Make `add_hook` and infrastructure helpers
delegate to this method so ordering has one implementation.

- [ ] **Step 5: Export the approved surface and rerun core tests**

Export `LifecycleResource` publicly and the macro-facing types with
`#[doc(hidden)]` from `mads-core/src/lib.rs`. Run:

```bash
cargo test -p mads-core --test descriptor
cargo test -p mads-core --test lifecycle
cargo test -p mads-core
```

Expected: PASS with existing descriptor and lifecycle behavior unchanged.

- [ ] **Step 6: Commit the core contracts**

```bash
git add crates/mads-core/src crates/mads-core/tests/descriptor.rs crates/mads-core/tests/lifecycle.rs
git commit -m "feat(core): add lifecycle provider contracts"
```

---

### Task 3: Implement the strict lifecycle-provider macro

**Files:**

- Modify: `crates/mads-core-macros/src/provider.rs`
- Modify: `crates/mads-core-macros/tests/support/provider.rs`
- Create: `crates/mads/tests/ui/pass/provider_lifecycle.rs`
- Create: `crates/mads/tests/ui/fail/provider_lifecycle_sync.rs`
- Create: `crates/mads/tests/ui/fail/provider_lifecycle_output.rs`
- Create: `crates/mads/tests/ui/fail/provider_lifecycle_result.rs`
- Create: `crates/mads/tests/ui/fail/provider_lifecycle_argument.rs`
- Create: matching `.stderr` snapshots in `crates/mads/tests/ui/fail/`

**Interfaces:**

- Consumes: Task 2's lifecycle constructor and contribution types.
- Produces: `#[provider(lifecycle)]` with native `T` descriptor identity, retained ordinary adapter construction, and focused signature errors.

- [ ] **Step 1: Add parser and expansion unit tests**

Add tests covering exact argument parsing, async enforcement, direct/fallible
resource extraction, and native output metadata. The expansion assertion must
include both calls:

```rust
.with_runtime_type_name(__mads_runtime_type_name_resource)
.with_lifecycle_constructor(__mads_construct_lifecycle_resource)
```

Assert `TypeId::of::<NativeResource>()`, not
`TypeId::of::<LifecycleResource<NativeResource>>()`.

- [ ] **Step 2: Add public compile fixtures**

Create `tests/ui/pass/provider_lifecycle.rs`:

```rust
use mads::core::LifecycleResource;

struct DirectResource;
struct FallibleResource;

#[mads::provider(lifecycle)]
async fn direct_resource() -> LifecycleResource<DirectResource> {
    LifecycleResource::new(DirectResource)
}

#[mads::provider(lifecycle)]
async fn fallible_resource() -> mads::core::Result<LifecycleResource<FallibleResource>> {
    Ok(LifecycleResource::new(FallibleResource))
}

fn main() {}
```

The four failure fixtures cover a synchronous function, a non-resource output,
`std::result::Result<LifecycleResource<T>, CustomError>`, and any argument
other than `lifecycle`. Include a generic lifecycle function in the output
fixture so the existing generic diagnostic remains enforced.

- [ ] **Step 3: Run macro unit and public UI tests to verify failure**

Run:

```bash
cargo test -p mads-core-macros provider::tests
cargo test -p mads --test ui core_attributes_accept_supported_shapes -- --exact
cargo test -p mads --test ui core_attributes_reject_unsupported_shapes -- --exact
```

Expected: unit assertions and the pass fixture fail; compile-fail cases create
new `wip/*.stderr` output.

- [ ] **Step 4: Implement ordinary/lifecycle mode parsing**

Replace the blanket argument rejection with an enum parsed from the attribute:

```rust
enum ProviderMode {
    Ordinary,
    Lifecycle,
}
```

Empty arguments select `Ordinary`; exactly `lifecycle` selects `Lifecycle`;
trailing tokens, duplicates, and other identifiers return the fixed supported
form. Ordinary expansion must remain token-equivalent to its current output.

For lifecycle mode:

- require `asyncness.is_some()`;
- recognize direct `LifecycleResource<T>` and the existing one-parameter MADS
  result aliases containing `LifecycleResource<T>`;
- emit descriptor metadata for `T`;
- emit a lifecycle constructor returning the full contribution; and
- emit the existing ordinary constructor adapter that invokes the function,
  converts the resource, and calls `ProviderContribution::into_provider()`.

Both constructors resolve dependencies through the existing clone contract.
Do not teach ordinary providers to accept two-parameter `Result`.

- [ ] **Step 5: Record and verify compiler diagnostics**

Run:

```bash
TRYBUILD=overwrite cargo test -p mads --test ui core_attributes_reject_unsupported_shapes -- --exact
cargo test -p mads --test ui
cargo test -p mads-core-macros
```

Inspect each new `.stderr`: it must name the two accepted async forms and must
not contain unstable internal generated identifiers.

- [ ] **Step 6: Commit the macro contract**

```bash
git add crates/mads-core-macros crates/mads/tests/ui
git commit -m "feat(core): declare lifecycle providers"
```

---

### Task 4: Consume lifecycle contributions in explicit and automatic builds

**Files:**

- Modify: `crates/mads-core/src/builder.rs`
- Create: `crates/mads-core/tests/provider_lifecycle.rs`
- Verify: `crates/mads-core/tests/builder.rs`
- Verify: `crates/mads-core/tests/construction_failure.rs`

**Interfaces:**

- Consumes: descriptor ordinary/lifecycle constructors and `ProviderContribution`.
- Produces: one invocation path for `construct::<T>()` and `build()`, native insertion before hook registration, and unchanged MADS006 wrapping.

- [ ] **Step 1: Write lifecycle construction tests**

Create macro-authored fixtures with an event provider and a resource hook.
Cover:

```rust
#[tokio::test]
async fn automatic_build_registers_native_value_and_contributed_hook() {
    let mut builder = Mads::builder();
    builder.root::<AutomaticRoot>().unwrap();
    let mut app = builder.build().await.unwrap();

    assert!(app.context().resolve::<ManagedResource>().is_ok());
    assert!(app.context().resolve::<LifecycleResource<ManagedResource>>().is_err());
    app.start().await.unwrap();
    app.shutdown().await.unwrap();
    assert_eq!(*EVENTS.lock().unwrap(), ["start:resource", "stop:resource"]);
}
```

Add separate tests proving:

- `builder.construct::<ManagedResource>()` registers the hook exactly once;
- contributed infrastructure starts before an application hook and stops after
  it;
- a later constructor failure produces `MADS006`, starts no hook, and drops a
  tracked resource value; and
- an ordinary hand-authored `ProviderFuture` descriptor still constructs and
  resolves unchanged.

- [ ] **Step 2: Run the new test and observe missing contribution consumption**

Run:

```bash
cargo test -p mads-core --test provider_lifecycle -- --nocapture
```

Expected: FAIL because builder paths still invoke only `descriptor.constructor()`.

- [ ] **Step 3: Centralize invocation and contribution application**

Add two private builder helpers with these responsibilities:

```rust
async fn invoke_provider(
    &self,
    descriptor: &'static ProviderDescriptor,
) -> Result<ProviderContribution>;

fn apply_provider_contribution(
    &mut self,
    descriptor: &'static ProviderDescriptor,
    contribution: ProviderContribution,
) -> Result<()>;
```

`invoke_provider` prefers `descriptor.lifecycle_constructor()` and otherwise
wraps the unchanged ordinary constructor result with
`ProviderContribution::from_provider`. `apply_provider_contribution` inserts
the provider first, then adds registrations in authored order.

Use both helpers in explicit construction. In automatic construction, retain
the existing `provider_construction_error(step, graph, source)` mapping before
applying the contribution. Push `SatisfiedProvider::preconstructed::<T>()`
only after explicit contribution application succeeds.

- [ ] **Step 4: Run focused and regression tests**

Run:

```bash
cargo test -p mads-core --test provider_lifecycle
cargo test -p mads-core --test builder
cargo test -p mads-core --test construction_failure
cargo test -p mads-core --test lifecycle
cargo test -p mads-core
```

Expected: PASS; ordinary graph order and diagnostics remain unchanged.

- [ ] **Step 5: Commit builder integration**

```bash
git add crates/mads-core/src/builder.rs crates/mads-core/tests
git commit -m "feat(core): construct lifecycle resources"
```

---

### Task 5: Create the persistence crate, safe error surface, and factory

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/mads-persistence/Cargo.toml`
- Create: `crates/mads-persistence/README.md`
- Create: `crates/mads-persistence/src/lib.rs`
- Create: `crates/mads-persistence/src/error.rs`
- Create: `crates/mads-persistence/src/factory.rs`
- Create: `crates/mads-persistence/tests/factory.rs`
- Modify: `crates/mads-cli/tests/release_automation.rs`

**Interfaces:**

- Consumes: `mads-core = =0.9.0` diagnostics and errors.
- Produces: `MADS140`, `PersistenceErrorKind`, `PersistenceError`, `PersistenceResult<T>`, `DatabaseConnector`, and `DatabaseFactory`.

- [ ] **Step 1: Add failing factory and error tests**

Create a fake connector returning a native test value and a second connector
returning a connection-category error. Put the source-formatting test inside
`src/error.rs`, where crate-private test constructors are available. Assert:

```rust
#[tokio::test]
async fn factory_returns_the_connector_native_type() {
    let value: NativeDatabase = DatabaseFactory.provide(SuccessConnector).await.unwrap();
    assert_eq!(value, NativeDatabase(7));
}

#[test]
fn framework_conversion_retains_kind_without_rendering_the_source() {
    let error = PersistenceError::with_source(
        PersistenceErrorKind::Connection,
        "connect",
        std::io::Error::other("mads-secret-user:mads-secret-password"),
    );
    let framework: mads_core::Error = error.into();
    assert_eq!(framework.code(), MADS140);
    assert!(!format!("{framework:?}\n{framework}").contains("mads-secret"));
    assert!(std::error::Error::source(&framework).is_some());
}
```

Production error constructors remain crate-private.

- [ ] **Step 2: Scaffold the crate and verify tests fail**

Add `crates/mads-persistence` to workspace members and create a manifest with
`version.workspace = true`, `mads-core = { path = "../mads-core", version =
"=0.9.0" }`, `tokio` as a dev dependency, `default = []`, and an initially
empty `sea-orm-postgres` feature. Run:

```bash
cargo test -p mads-persistence --test factory
```

Expected: compile failure because the public error and factory APIs do not
exist.

- [ ] **Step 3: Implement safe typed errors**

Expose:

```rust
pub const MADS140: DiagnosticCode = DiagnosticCode::new("MADS140");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistenceErrorKind {
    InvalidConfiguration,
    UnsupportedScheme,
    Connection,
    Readiness,
    GracefulClose,
}

pub type PersistenceResult<T> = Result<T, PersistenceError>;
```

`PersistenceError` stores its kind, a static operation label, and an optional
boxed `Send + Sync` source. `Display` prints only connector/backend/operation
and kind. `Debug` prints kind, operation, and `has_source`; it never delegates
to the source. `From<PersistenceError> for mads_core::Error` creates `MADS140`,
uses the safe operation as subject, and retains the persistence error as source.

- [ ] **Step 4: Implement the connector trait and factory**

```rust
pub trait DatabaseConnector: Send + Sync + 'static {
    type Database: Send + Sync + 'static;

    fn connect(self) -> impl Future<Output = PersistenceResult<Self::Database>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DatabaseFactory;

impl DatabaseFactory {
    pub async fn provide<C>(&self, connector: C) -> PersistenceResult<C::Database>
    where
        C: DatabaseConnector,
    {
        connector.connect().await
    }
}
```

Document that the factory has no registry or lifecycle side effects.

- [ ] **Step 5: Add the package to version-policy tests and run them**

Add `mads-persistence` to `FRAMEWORK_PACKAGES` and `PACKAGES` in release
automation tests. Run:

```bash
cargo test -p mads-persistence
cargo test -p mads-cli --test release_automation framework_packages_use_v090_pins_while_cli_remains_v080 -- --exact
cargo check -p mads-persistence --no-default-features
```

Expected: PASS.

- [ ] **Step 6: Commit the crate foundation**

```bash
git add Cargo.toml Cargo.lock crates/mads-persistence crates/mads-cli/tests/release_automation.rs
git commit -m "feat(persistence): add connector foundation"
```

---

### Task 6: Implement SeaORM configuration and PostgreSQL connection

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/mads-persistence/Cargo.toml`
- Modify: `crates/mads-persistence/src/lib.rs`
- Create: `crates/mads-persistence/src/sea_orm/mod.rs`
- Create: `crates/mads-persistence/src/sea_orm/config.rs`
- Create: `crates/mads-persistence/src/sea_orm/connector.rs`
- Create: `crates/mads-persistence/tests/config.rs`
- Create: `crates/mads-persistence/tests/connector.rs`

**Interfaces:**

- Consumes: Task 5 factory/errors, MADS `Configuration`/`Secret`, SeaORM 2.0 `ConnectOptions` and `DatabaseConnection`.
- Produces: internal `SeaOrmConfig`, public `SeaOrmPostgres`, and `mads_persistence::sea_orm::*` re-export.

`SeaOrmPostgres` exposes `new(Secret<String>)`, `from_options(ConnectOptions)`,
and `options_mut(&mut self) -> &mut ConnectOptions`. Its crate-private
`from_config(&Config) -> PersistenceResult<Self>` is the only typed-config
construction path used by `DatabaseModule`.

- [ ] **Step 1: Write configuration matrix and redaction tests**

Test required URL, absent optional values, every optional setter, zero counts,
and the approved relationship issue:

```rust
assert_eq!(issue.key(), "persistence.seaorm.min_connections");
assert_eq!(issue.code(), "invalid_relationship");
assert_eq!(issue.message(), "minimum connections must not exceed maximum connections");
```

Zero `min_connections` and `max_connections` must use `too_small` on their own
full keys. Assert omitted options leave every `ConnectOptions::get_*` optional
getter at `None`; explicit `sqlx_logging = false` changes its getter to false.

Use `postgres://mads-secret-user:mads-secret-password@localhost/mads?token=mads-secret-query`
as the sentinel and assert it is absent from config, connector, persistence
error, and converted framework error formatting.

- [ ] **Step 2: Write the scheme table before implementation**

```rust
for accepted in ["postgres://localhost/db", "postgresql://localhost/db"] {
    assert!(validate_postgres_scheme(accepted).is_ok(), "{accepted}");
}
for rejected in [
    "mysql://localhost/db",
    "sqlite://db.sqlite",
    "localhost/db",
    "",
] {
    let error = validate_postgres_scheme(rejected).unwrap_err();
    assert_eq!(error.kind(), PersistenceErrorKind::UnsupportedScheme);
}
```

- [ ] **Step 3: Add SeaORM dependency and feature wiring**

Add to workspace dependencies:

```toml
sea-orm = { version = "2.0.0", default-features = false }
```

In `mads-persistence`:

```toml
[dependencies]
sea-orm = { workspace = true, optional = true }

[features]
default = []
sea-orm-postgres = [
    "dep:sea-orm",
    "sea-orm/macros",
    "sea-orm/sqlx-postgres",
    "sea-orm/runtime-tokio-rustls",
]
```

- [ ] **Step 4: Implement typed configuration and exact validation**

Derive an internal configuration with prefix `persistence.seaorm`, required
`Secret<String>` URL, optional `u32` counts, optional `u64` seconds, and
optional `bool` logging. Use existing `positive` validation for each count.
After typed parsing, create one `ConfigurationIssue` for `min > max` with the
approved key/code/message and the winning source label of the minimum key.
Wrap all `ConfigurationErrors` in
`PersistenceErrorKind::InvalidConfiguration` without formatting their values.

Only call a SeaORM setter for `Some(value)`. Convert durations with
`Duration::from_secs`.

- [ ] **Step 5: Implement `SeaOrmPostgres` and native connect**

Implement `new`, `from_options`, `options_mut`, custom redacted `Debug`, and:

```rust
impl DatabaseConnector for SeaOrmPostgres {
    type Database = sea_orm::DatabaseConnection;

    async fn connect(self) -> PersistenceResult<Self::Database> {
        validate_postgres_scheme(self.options.get_url())?;
        sea_orm::SqlxPostgresConnector::connect(self.options)
            .await
            .map_err(PersistenceError::connection)
    }
}
```

Use the PostgreSQL-specific entry point deliberately: SQLx accepts both
approved schemes, while SeaORM's backend dispatcher documents only the
`postgres://` prefix. Do not alter or log the URL.

- [ ] **Step 6: Run connector and feature tests**

```bash
cargo test -p mads-persistence --features sea-orm-postgres --test config
cargo test -p mads-persistence --features sea-orm-postgres --test connector
cargo check -p mads-persistence --no-default-features
cargo check -p mads-persistence --no-default-features --features sea-orm-postgres
cargo tree -e normal -p mads-persistence --no-default-features
```

Expected: tests PASS; the no-feature tree contains no SeaORM/SQLx; the enabled
tree contains SeaORM PostgreSQL but no Axum, Diesel, MySQL, or SQLite driver.

- [ ] **Step 7: Commit SeaORM construction**

```bash
git add Cargo.toml Cargo.lock crates/mads-persistence
git commit -m "feat(persistence): connect native SeaORM PostgreSQL"
```

---

### Task 7: Register the global database module and lifecycle hook

**Files:**

- Modify: `crates/mads-persistence/src/sea_orm/mod.rs`
- Create: `crates/mads-persistence/src/sea_orm/lifecycle.rs`
- Create: `crates/mads-persistence/tests/module.rs`
- Modify: `crates/mads-persistence/Cargo.toml`

**Interfaces:**

- Consumes: native connector, core lifecycle provider macro, current global-module scoping.
- Produces: public global `DatabaseModule`, one public native `DatabaseConnection` provider, readiness ping, and explicit close.

- [ ] **Step 1: Write graph-only module tests**

Define an imported root with a repository field of native
`DatabaseConnection`, and an unimported root. Assert analysis selects the
native provider only for the imported root, reports ownership by
`DatabaseModule`, and never exposes `LifecycleResource<DatabaseConnection>`.

Build the imported root with an empty config only through `analyze()` twice;
both analyses must succeed without the missing required URL being parsed.
Build the unimported root with the same empty config and prove construction
succeeds, demonstrating that the connector constructor was not selected. Add
a second public native connection provider in another imported module and
assert the existing ambiguous-binding diagnostic remains `MADS002` for two
distinct provider declarations producing `DatabaseConnection`.

- [ ] **Step 2: Run the module test and observe missing provider metadata**

Run:

```bash
cargo test -p mads-persistence --features sea-orm-postgres --test module
```

Expected: compile failure because `DatabaseModule` and its providers do not
exist.

- [ ] **Step 3: Implement the global module and providers**

Declare these functions inside the same module namespace as `DatabaseModule`:

```rust
#[mads_core::provider]
fn database_factory() -> DatabaseFactory {
    DatabaseFactory
}

#[mads_core::provider]
fn sea_orm_postgres_connector(config: mads_core::Config) -> mads_core::Result<SeaOrmPostgres> {
    SeaOrmPostgres::from_config(&config).map_err(Into::into)
}

#[mads_core::provider(lifecycle)]
pub async fn sea_orm_database(
    factory: DatabaseFactory,
    connector: SeaOrmPostgres,
) -> mads_core::Result<LifecycleResource<DatabaseConnection>> {
    let database = factory.provide(connector).await.map_err(mads_core::Error::from)?;
    Ok(LifecycleResource::new(database).with_infrastructure_hook(
        "mads.persistence.seaorm.postgres",
        SeaOrmLifecycle,
    ))
}

#[mads_core::module(global)]
pub struct DatabaseModule;
```

Keep factory and connector functions private; keep only the native database
provider public across the module boundary.

- [ ] **Step 4: Implement the lifecycle hook**

`SeaOrmLifecycle::name()` returns `mads.persistence.seaorm.postgres`.
`start` resolves `DatabaseConnection` from `ApplicationContext`, calls
`ping()`, maps failure to `PersistenceErrorKind::Readiness`, then converts to
`mads_core::Error`. `stop` resolves the same value, calls `close_by_ref()`, and
maps failure to `GracefulClose` through the same boundary.

- [ ] **Step 5: Add mock lifecycle and opt-in construction coverage**

Add a target-local dev-dependency entry for SeaORM 2.0 with only `mock` and
`runtime-tokio-rustls`, leaving the normal optional dependency and public
feature unchanged. In unit tests, provide a mock native connection and attach `SeaOrmLifecycle` through
`LifecycleResource`; assert startup and shutdown resolve the same registered
instance. Use a failing test hook after/before it to confirm existing rollback
and shutdown continuation order without a live server.

- [ ] **Step 6: Run graph, lifecycle, and architecture checks**

```bash
cargo test -p mads-persistence --features sea-orm-postgres --test module
cargo test -p mads-persistence --features sea-orm-postgres
cargo test -p mads-core --test module_scope
cargo test -p mads-core --test architecture
```

Expected: PASS with no database service.

- [ ] **Step 7: Commit module integration**

```bash
git add crates/mads-persistence
git commit -m "feat(persistence): register SeaORM database module"
```

---

### Task 8: Prove real PostgreSQL CRUD, transactions, readiness, and shutdown

**Files:**

- Create: `crates/mads-persistence/tests/postgres.rs`
- Create: `crates/mads-persistence/examples/standard_run.rs`
- Modify: `crates/mads-persistence/Cargo.toml`

**Interfaces:**

- Consumes: `MADS_TEST_DATABASE_URL`, public connector/module APIs, SeaORM entity and transaction traits.
- Produces: ignored PostgreSQL acceptance tests and a conventional-startup subprocess fixture.

Add `mads` as a dev dependency pinned to `=0.9.0`, with only the facade
features needed by `standard_run`, plus the existing workspace test utilities
needed for HTTP polling and child-process cleanup. These remain test/example
dependencies and must not enter `mads-persistence`'s normal graph.

- [ ] **Step 1: Add a serial real-database fixture**

Use one static Tokio mutex. Build config through `ConfigBuilder` and
`MapSource` without mutating process environment. Derive a SeaORM entity for a
dedicated `mads_persistence_v090_items` table and create/drop or truncate it
with native SeaORM statements at test boundaries.

- [ ] **Step 2: Write the ignored CRUD and transaction test**

The test must:

1. import `DatabaseModule` through a rooted application module;
2. build and start the application;
3. resolve a repository containing native `DatabaseConnection`;
4. insert, find, update, and delete through native entity APIs;
5. commit one transaction and deliberately roll back another; and
6. call application shutdown and assert a retained connection clone's later
   `ping()` fails.

Run before the implementation is complete:

```bash
cargo test -p mads-persistence --features sea-orm-postgres --test postgres -- --ignored --test-threads=1
```

Expected: the focused acceptance test identifies any connection, entity,
transaction, or close behavior not yet wired correctly.

- [ ] **Step 3: Add readiness and operational-failure cases**

Add ignored tests proving invalid credentials fail `build()` with the chain
`MADS006 -> MADS140 -> PersistenceErrorKind::Connection`, and that no sentinel
credential appears in `Display` or `Debug`. Add a successful application where
a deliberate native query error occurs, then explicitly call `shutdown()` and
assert the pool is closed.

Add a shutdown observer application hook and verify it runs before the SeaORM
infrastructure close. Add a failing application stop hook after the observer
and verify SeaORM close is still attempted while the first shutdown error
remains primary.

- [ ] **Step 4: Add the conventional startup subprocess fixture**

The example defines one health route and imports `DatabaseModule`. The test
spawns it with child-only environment values for the database URL and server
address. For invalid credentials, occupy the requested listener address first,
run the child, and assert the returned diagnostic is the provider/connection
chain rather than a bind failure. This proves database construction precedes
listener binding without process-global environment mutation.

For a valid URL, start the child, wait for its health endpoint, send SIGTERM on
Linux, and require a clean exit. Keep this case Linux-only and inside the
ignored PostgreSQL suite.

- [ ] **Step 5: Run all real PostgreSQL evidence**

```bash
cargo test -p mads-persistence --features sea-orm-postgres --test postgres -- --ignored --test-threads=1
```

Expected: PASS with PostgreSQL 16 and `MADS_TEST_DATABASE_URL` set.

- [ ] **Step 6: Commit PostgreSQL acceptance coverage**

```bash
git add crates/mads-persistence
git commit -m "test(persistence): prove native PostgreSQL lifecycle"
```

---

### Task 9: Integrate feature, package, CI, and publication automation

**Files:**

- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/beta-publish.yml`
- Modify: `.github/workflows/stable-publish.yml`
- Modify: `script/release.sh`
- Modify: `script/verify-package-contents.sh`
- Modify: `crates/mads-cli/tests/release_automation.rs`
- Modify: `crates/mads-cli/tests/command_matrix.rs`

**Interfaces:**

- Consumes: framework package 0.9 set, SeaORM feature matrix, PostgreSQL tests.
- Produces: reproducible package/version checks, SeaORM 2.0.0 lower-bound job, framework-only publish order, and CI enforcement.

- [ ] **Step 1: Update automation assertions before workflow files**

Require both beta and stable workflows to contain:

```text
cargo check -p mads-persistence --no-default-features
cargo check -p mads-persistence --no-default-features --features sea-orm-postgres
cargo test --locked -p mads-persistence --features sea-orm-postgres --test postgres -- --ignored --test-threads=1
```

Require publish order to contain `mads-persistence` immediately after
`mads-core`. Assert neither publication package array contains `mads-cli`.
Update the version test to include persistence at 0.9.0 and CLI at 0.8.0.

- [ ] **Step 2: Run automation tests and verify they fail**

```bash
cargo test -p mads-cli --test release_automation
cargo test -p mads-cli --test command_matrix
```

Expected: FAIL because workflows and scripts omit the new crate and commands.

- [ ] **Step 3: Update release and package scripts**

Add `mads-persistence` to the release script's framework package tuple so
root/nested lockfiles update it. Keep the existing literal CLI version
preservation behavior. Add persistence to package-content verification and
require its `README.md`, `src/lib.rs`, and SeaORM module source files.

- [ ] **Step 4: Update all workflow gates**

In CI, beta, and stable workflows:

- add no-feature and connector-feature checks to the Linux verification job;
- add the ignored persistence PostgreSQL test to the PostgreSQL job;
- leave cross-platform CLI jobs free of PostgreSQL services and ignored DB
  tests;
- keep workspace lint, test, doc, package, MSRV, and coverage commands; and
- rename visible `v0.8` feature-boundary labels to `v0.9`.

Add a separate Linux `seaorm-minimum` job to each workflow, using a fresh
checkout. In beta and stable, make the publish job depend on this gate. Resolve
only that ephemeral checkout's lockfile to the lower bound and run:

```bash
cargo update -p sea-orm --precise 2.0.0
cargo test -p mads-persistence --features sea-orm-postgres
```

The main committed lockfile remains on the normal compatible 2.0 resolution.

In both publish arrays use this dependency order:

```text
mads-core-macros
mads-common-macros
mads-core
mads-persistence
mads-extra
mads-common
mads
```

Do not publish `mads-cli` in either workflow.

- [ ] **Step 5: Run automation, feature, and package checks**

```bash
cargo test -p mads-cli --test release_automation
cargo test -p mads-cli --test command_matrix
cargo check -p mads-persistence --no-default-features
cargo check -p mads-persistence --no-default-features --features sea-orm-postgres
bash script/verify-package-contents.sh
```

Expected: PASS.

- [ ] **Step 6: Commit automation integration**

```bash
git add .github/workflows script crates/mads-cli/tests
git commit -m "ci: release mads persistence with framework 0.9"
```

---

### Task 10: Synchronize documentation and run final verification

**Files:**

- Modify: `crates/mads-persistence/README.md`
- Modify: `crates/mads-persistence/src/lib.rs`
- Modify: `README.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/mads-persistence.md`
- Modify: `CHANGELOG.md`
- Verify: `docs/superpowers/specs/2026-09-23-mads-persistence-design.md`

**Interfaces:**

- Consumes: completed public API and acceptance evidence.
- Produces: one consistent 0.9 user contract and a fully verified workspace.

- [ ] **Step 1: Write package rustdoc and README examples**

Document the dependency:

```toml
mads-persistence = {
    version = "0.9",
    default-features = false,
    features = ["sea-orm-postgres"],
}
```

Show `DatabaseModule` in the root imports and a repository field of
`mads_persistence::sea_orm::DatabaseConnection`. State that entities,
relations, queries, transactions, and migrations remain native SeaORM. Warn
that enabling SQLx statement/binding logging can expose application values.

- [ ] **Step 2: Synchronize architecture and source design**

Update `docs/mads-persistence.md` to reflect all approved changes:

- version 0.9.0 rather than 0.1;
- unchanged ordinary `ProviderFuture` and a separate optional lifecycle
  constructor;
- internal providers returning `mads_core::Result`;
- restricted async lifecycle signatures;
- both PostgreSQL schemes;
- enabled SeaORM macros;
- public non-exhaustive error kind;
- approved `invalid_relationship` issue; and
- CLI remaining 0.8.0 and excluded from framework publication.

Update architecture diagrams to place `mads-persistence` beside the facade and
keep core free of ORM dependencies. Do not imply that importing `mads` enables
the connector.

- [ ] **Step 3: Add changelog and root discovery documentation**

Add a 0.9.0 section describing lifecycle provider support, the separate native
SeaORM connector, redaction, framework versions, and the retained CLI version.
Update the root crate table and quick start. Do not describe migrations,
additional backends, or automatic HTTP mapping as implemented.

- [ ] **Step 4: Run focused verification**

```bash
cargo fmt --all --check
cargo test -p mads-core
cargo test -p mads-core-macros
cargo test -p mads --test ui
cargo test -p mads-persistence --features sea-orm-postgres
cargo test -p mads-cli --test release_automation
cargo test -p mads-cli --test command_matrix
```

Expected: PASS.

- [ ] **Step 5: Run full non-service verification**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked --workspace --all-features --doc
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps
bash script/verify-package-contents.sh
cargo package --locked --workspace --no-verify
```

Expected: PASS.

- [ ] **Step 6: Run PostgreSQL and coverage gates**

With the repository's PostgreSQL 16 service and `MADS_TEST_DATABASE_URL`:

```bash
cargo test --locked -p mads-persistence --features sea-orm-postgres --test postgres -- --ignored --test-threads=1
cargo llvm-cov --workspace --all-features --ignore-filename-regex '(^|/)tests/ui/' --fail-under-lines 85
```

Expected: PASS and line coverage at or above 85%.

- [ ] **Step 7: Confirm source/spec/plan coverage and clean diff**

```bash
if rg -n "mads-persistence.{0,40}0\.1|mads-cli.{0,40}0\.8\.1|mads-persistance|ProviderFuture.{0,80}ProviderContribution|Result<LifecycleResource<.*PersistenceError" docs/mads-persistence.md docs/superpowers/specs/2026-09-23-mads-persistence-design.md README.md CHANGELOG.md; then exit 1; fi
git diff --check
git status --short
```

Expected: the search finds no stale persistence version, misspelling, literal
ProviderFuture-breaking design, or two-parameter lifecycle provider signature;
diff check succeeds; status lists only the intended Task 10 files.

- [ ] **Step 8: Commit documentation and final release contract**

```bash
git add README.md CHANGELOG.md docs/ARCHITECTURE.md docs/mads-persistence.md crates/mads-persistence/README.md crates/mads-persistence/src/lib.rs
git commit -m "docs: publish mads persistence 0.9 contract"
```
