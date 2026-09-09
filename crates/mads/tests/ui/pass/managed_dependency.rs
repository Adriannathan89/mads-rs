//! Confirms managed providers accept dependencies with the generated contract.

#[derive(Clone)]
struct Dependency;

#[mads::service]
struct Service {
    dependency: Dependency,
}

fn main() {}
