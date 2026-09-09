//! Confirms controller dependency constraints point at the declared field type.

// Keep the Clone suggestion on a two-digit line so stable and Rust 1.85
// render its help gutter identically.
//
//
//
//
//
struct NotClone;

#[mads::routes]
trait Routes {
    #[mads::get("/")]
    async fn index(&self);
}

#[mads::controller(routes = [Routes])]
struct Controller {
    dependency: NotClone,
}

impl Routes for Controller {
    async fn index(&self) {}
}

fn main() {}
