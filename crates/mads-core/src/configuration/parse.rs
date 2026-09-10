//! Safe field reads used by configuration derive expansions.

use crate::{Config, ConfigurationErrors, ConfigurationIssue, ConfigurationResult};

/// Parses a nested view after rejecting incompatible values at its table key.
pub fn nested<T: crate::Configuration>(config: &Config, key: &str) -> ConfigurationResult<T> {
    if config.get(key).is_some() || config.get_string_array(key).is_some() {
        return Err(failure(config, key, "invalid_type"));
    }
    T::__from_config_prefix(config, key)
}

/// Retains a parsed field or appends its failures in declaration order.
pub fn collect<T>(
    result: ConfigurationResult<T>,
    errors: &mut Option<ConfigurationErrors>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(next) => {
            match errors {
                Some(errors) => errors.extend(next),
                None => *errors = Some(next),
            }
            None
        }
    }
}

/// A supported scalar parser which never exposes native parse errors.
pub trait Scalar: Sized {
    /// Parses a scalar, returning only success or failure.
    fn parse_scalar(value: &str) -> Option<Self>;
}

macro_rules! scalars {
    ($($ty:ty),+ $(,)?) => {$(
        impl Scalar for $ty {
            fn parse_scalar(value: &str) -> Option<Self> {
                value.parse().ok()
            }
        }
    )+};
}

scalars!(
    String, bool, char, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

impl Scalar for f32 {
    fn parse_scalar(value: &str) -> Option<Self> {
        value.parse::<Self>().ok().filter(|value| value.is_finite())
    }
}

impl Scalar for f64 {
    fn parse_scalar(value: &str) -> Option<Self> {
        value.parse::<Self>().ok().filter(|value| value.is_finite())
    }
}

/// Calls an application scalar parser without retaining its error or input.
pub fn parse_with<T, E, F>(config: &Config, key: &str, parser: F) -> ConfigurationResult<T>
where
    F: FnOnce(&str) -> Result<T, E>,
{
    match config.get(key) {
        Some(value) => parser(value).map_err(|_| failure(config, key, "invalid_type")),
        None => Err(failure(config, key, absent_code(config, key))),
    }
}

/// Reads and parses one scalar, rejecting arrays and tables.
pub fn scalar<T: Scalar>(config: &Config, key: &str) -> ConfigurationResult<T> {
    match config.get(key) {
        Some(value) => T::parse_scalar(value).ok_or_else(|| failure(config, key, "invalid_type")),
        None => Err(failure(config, key, absent_code(config, key))),
    }
}

/// Reads an ordered string array, rejecting scalar and table values.
pub fn string_array(config: &Config, key: &str) -> ConfigurationResult<Vec<String>> {
    config
        .get_string_array(key)
        .map(<[String]>::to_vec)
        .ok_or_else(|| failure(config, key, absent_code(config, key)))
}

/// Resolves a scalar path using the winning source base or current directory.
pub fn path(config: &Config, key: &str) -> ConfigurationResult<std::path::PathBuf> {
    config
        .resolve_path(key)
        .ok_or_else(|| failure(config, key, absent_code(config, key)))
}

/// Detects values, explicitly declared tables, or descendants at a field key.
pub fn present(config: &Config, key: &str) -> bool {
    config.get(key).is_some()
        || config.get_string_array(key).is_some()
        || config.contains_table(key)
        || config.has_descendants(key)
}

fn absent_code(config: &Config, key: &str) -> &'static str {
    if present(config, key) {
        "invalid_type"
    } else {
        "required"
    }
}

fn failure(config: &Config, key: &str, code: &str) -> ConfigurationErrors {
    let message = if code == "required" {
        "required configuration key is missing"
    } else {
        "configuration value has an invalid type"
    };
    let mut issue = ConfigurationIssue::new(key, code, message);
    if let Some(source) = config
        .source_of(key)
        .or_else(|| config.source_of_string_array(key))
        .or_else(|| config.table_source(key))
    {
        issue = issue.with_source(source);
    }
    ConfigurationErrors::from_issue(issue)
}
