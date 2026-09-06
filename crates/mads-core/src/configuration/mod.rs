//! Typed views over an already loaded configuration document.

mod error;
#[doc(hidden)]
pub mod parse;
mod secret;

pub use error::{ConfigurationErrors, ConfigurationIssue, ConfigurationResult};
pub use secret::Secret;

use crate::Config;

/// A typed view parsed explicitly from an already loaded configuration.
///
/// Implementations return independent failures in declaration order and must
/// keep configured values out of their errors. Merely implementing this trait
/// does not register any startup requirement.
pub trait Configuration: Sized {
    /// Parses this view without loading additional configuration sources.
    fn from_config(config: &Config) -> ConfigurationResult<Self>;

    /// Parses a nested view, preserving source origins and complete error keys.
    #[doc(hidden)]
    fn __from_config_prefix(config: &Config, prefix: &str) -> ConfigurationResult<Self> {
        Self::from_config(&config.project(prefix)).map_err(|mut errors| {
            errors.prefix(prefix);
            errors
        })
    }
}
