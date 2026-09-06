//! Typed views over an already loaded configuration document.

mod error;
#[doc(hidden)]
pub mod parse;
mod secret;
#[doc(hidden)]
pub mod validate;

pub use error::{ConfigurationErrors, ConfigurationIssue, ConfigurationResult};
pub use secret::Secret;

use crate::Config;

/// A typed view parsed explicitly from an already loaded configuration.
///
/// Implementations return independent failures in declaration order and must
/// keep configured values out of their errors. Merely implementing this trait
/// does not register any startup requirement.
///
/// ```
/// use mads_core::{Config, Configuration};
///
/// #[derive(Configuration)]
/// #[config(prefix = "app")]
/// struct Settings {
///     #[config(default = 3000, validate(range(min = 1, max = 65535)))]
///     port: u16,
/// }
///
/// let settings = Config::empty().parse::<Settings>().unwrap();
/// assert_eq!(settings.port, 3000);
/// ```
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
