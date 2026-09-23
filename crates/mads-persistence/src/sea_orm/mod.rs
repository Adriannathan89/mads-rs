//! Native SeaORM PostgreSQL connector and opt-in database module.

mod config;
mod connector;
mod lifecycle;

pub use ::sea_orm::{ConnectOptions, DatabaseConnection};
pub use connector::SeaOrmPostgres;

use mads_core::LifecycleResource;

use crate::DatabaseFactory;
use lifecycle::SeaOrmLifecycle;

/// Global module that provides one native SeaORM PostgreSQL connection.
#[mads_core::module(global)]
pub struct DatabaseModule;

#[mads_core::provider]
fn database_factory() -> DatabaseFactory {
    DatabaseFactory
}

#[mads_core::provider]
fn sea_orm_postgres_connector(config: mads_core::Config) -> mads_core::Result<SeaOrmPostgres> {
    SeaOrmPostgres::from_config(&config).map_err(Into::into)
}

/// Constructs the native database connection and contributes its lifecycle hook.
#[mads_core::provider(lifecycle)]
pub async fn sea_orm_database(
    factory: DatabaseFactory,
    connector: SeaOrmPostgres,
) -> mads_core::Result<LifecycleResource<DatabaseConnection>> {
    let database = factory
        .provide(connector)
        .await
        .map_err(mads_core::Error::from)?;
    Ok(LifecycleResource::new(database)
        .with_infrastructure_hook("mads.persistence.seaorm.postgres", SeaOrmLifecycle))
}
