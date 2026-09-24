use mads::prelude::*;

#[routes]
trait HelloRoutes {
    #[get("/")]
    async fn hello(&self) -> &'static str;
}

#[controller(routes = [HelloRoutes])]
struct HelloController;

impl HelloRoutes for HelloController {
    async fn hello(&self) -> &'static str {
        "Hello, world!"
    }
}

#[module]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
