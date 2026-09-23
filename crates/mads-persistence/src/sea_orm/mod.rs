//! Native SeaORM PostgreSQL connector and opt-in database module.

mod config;
mod connector;

pub use ::sea_orm::{ConnectOptions, DatabaseConnection};
pub use connector::SeaOrmPostgres;
