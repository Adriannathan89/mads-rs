//! Conventional MADS startup with the opt-in native SeaORM module.
#![cfg(feature = "sea-orm-postgres")]

use mads::prelude::*;
use mads_persistence::sea_orm::DatabaseModule;

mod delivery {
    use mads::prelude::*;

    #[routes]
    pub trait HealthRoutes {
        #[get("/health")]
        async fn health(&self) -> &'static str;
    }

    #[controller(routes = [HealthRoutes])]
    pub struct HealthController;

    impl HealthRoutes for HealthController {
        async fn health(&self) -> &'static str {
            "healthy"
        }
    }

    #[module]
    pub struct HealthModule;
}

#[module(imports = [DatabaseModule, delivery::HealthModule])]
struct AppModule;

#[mads::main]
async fn main() -> Result<(), HttpRuntimeError> {
    Mads::run::<AppModule>().await
}
