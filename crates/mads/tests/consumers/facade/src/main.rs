//! Verifies attribute expansion through a facade-only dependency.

use mads::prelude::*;

#[module]
struct AppModule;

#[repository]
struct Repository {
    value: u32,
}

#[routes]
trait Routes {
    #[get("/")]
    async fn index(&self);
}

#[controller(routes = [Routes])]
struct Controller;

impl Routes for Controller {
    async fn index(&self) {}
}

fn main() {}
