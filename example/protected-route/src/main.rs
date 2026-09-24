mod auth;

use auth::AuthModule;
use mads::prelude::*;

#[module(imports = [LoggerModule, AuthModule])]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
