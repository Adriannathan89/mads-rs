//! Verifies main expansion through a renamed facade-only dependency.

use framework::prelude::*;

#[repository]
struct MainRepository {
    value: u32,
}

fn consume_repository(repository: &MainRepository) {
    let _ = repository.value;
}

#[framework::main]
async fn main() {
    let _ = consume_repository;
}
