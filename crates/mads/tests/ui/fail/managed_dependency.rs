//! Confirms managed dependency constraints point at the declared field type.

// Keep both Clone suggestions on two-digit lines so stable and Rust 1.85
// render their help gutters identically.
//
//
//
//
//
//
struct NotCloneProvider;

struct ProviderOutput;

#[allow(dead_code)]
#[mads::provider]
fn provide(_dependency: NotCloneProvider) -> ProviderOutput {
    ProviderOutput
}

struct NotCloneManaged;

#[mads::service]
struct Service {
    dependency: NotCloneManaged,
}

fn main() {}
