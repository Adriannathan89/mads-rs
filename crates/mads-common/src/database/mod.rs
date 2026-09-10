//! Explicit PostgreSQL database configuration, lifecycle, and persistence APIs.

mod auto_configuration;
mod config;
mod error;
#[cfg(all(feature = "http", feature = "database"))]
mod http;
mod lifecycle;
mod migration;
mod pool;

pub use config::DatabaseConfig;
pub use error::{DatabaseError, DatabaseErrorKind, DatabaseResult};
#[cfg(all(feature = "http", feature = "database"))]
pub use http::IntoHttpResult;
pub use lifecycle::{DatabaseBootstrap, MadsBuilderDatabaseExt};
pub use migration::{MigrationReport, MigrationStatus};
pub use pool::{Database, DatabasePoolStatus};

/// Database configuration or persistence integration failure.
pub const MADS100: mads_core::DiagnosticCode = mads_core::DiagnosticCode::new("MADS100");

/// Database auto-configuration failure.
pub const MADS101: mads_core::DiagnosticCode = mads_core::DiagnosticCode::new("MADS101");
