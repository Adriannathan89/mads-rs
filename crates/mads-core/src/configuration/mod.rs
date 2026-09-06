//! Typed views over an already loaded configuration document.

mod error;
mod secret;

pub use error::{ConfigurationErrors, ConfigurationIssue, ConfigurationResult};
pub use secret::Secret;
