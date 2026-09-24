mod post;

use mads::prelude::*;
use mads_persistence::sea_orm::DatabaseModule;
use post::PostModule;

#[module(imports = [DatabaseModule, PostModule])]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
