//! Verifies main expansion through a facade-only dependency.

use mads::prelude::*;

#[repository]
struct MainRepository {
    value: u32,
}

fn consume_repository(repository: &MainRepository) {
    let _ = repository.value;
}

#[mads::main]
async fn main() {
    let _ = consume_repository;
}
