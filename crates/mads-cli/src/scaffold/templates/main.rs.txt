// src/main.rs
mod app;

use app::AppModule;
use mads::prelude::*;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
