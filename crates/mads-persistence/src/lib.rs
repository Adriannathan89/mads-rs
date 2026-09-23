//! Native persistence connector contracts for MADS.rs applications.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod error;
mod factory;

pub use error::{MADS140, PersistenceError, PersistenceErrorKind, PersistenceResult};
pub use factory::{DatabaseConnector, DatabaseFactory};
