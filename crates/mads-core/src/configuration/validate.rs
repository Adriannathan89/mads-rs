//! Value-free configuration validation support for generated code.

use crate::{Config, ConfigurationErrors, ConfigurationIssue};

/// Appends a safe validation issue with configured or default provenance.
pub fn push(errors: &mut Option<ConfigurationErrors>, config: &Config, key: &str, code: &str) {
    let message = match code {
        "invalid_format" => "invalid email address",
        "too_small" => "value is too small",
        "too_big" => "value is too big",
        "not_multiple_of" => "value is not a multiple of the required number",
        _ => "configuration value has an invalid type",
    };
    let source = config
        .source_of(key)
        .or_else(|| config.source_of_string_array(key))
        .or_else(|| config.table_source(key))
        .unwrap_or("default");
    let issue = ConfigurationIssue::new(key, code, message).with_source(source);
    match errors {
        Some(errors) => errors.push(issue),
        None => *errors = Some(ConfigurationErrors::from_issue(issue)),
    }
}

/// Checks the practical Zod default email policy without normalization.
///
/// Policy reference: <https://github.com/colinhacks/zod/blob/main/packages/zod/src/v4/core/regexes.ts>
pub fn email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || local.starts_with('.') || value.contains("..") {
        return false;
    }
    if !local
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_'+-.".contains(&byte))
        || !local
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || b"_+-".contains(&byte))
    {
        return false;
    }
    let Some((labels, suffix)) = domain.rsplit_once('.') else {
        return false;
    };
    suffix.len() >= 2
        && suffix.bytes().all(|byte| byte.is_ascii_alphabetic())
        && labels.split('.').all(|label| {
            label
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Checks decimal multiples with a tolerance bounded by floating-point precision.
pub fn multiple(value: f64, divisor: f64, epsilon: f64) -> bool {
    if !value.is_finite() || !divisor.is_finite() || divisor == 0.0 {
        return false;
    }
    let quotient = value / divisor;
    if !quotient.is_finite() {
        return false;
    }
    let difference = (quotient - quotient.round()).abs();
    // Include the divisor's decimal quantum so decimal inputs such as
    // 0.3 / 0.1 tolerate representation noise. Cap below half a step so
    // growing machine error cannot turn arbitrary values into multiples.
    let decimal = divisor.abs().to_string();
    let places = decimal
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let quantum = 10_f64.powi(-(places as i32));
    let tolerance = epsilon * (value.abs().max(divisor.abs()) + quantum) * 4.0 / divisor.abs();
    difference <= tolerance.min(0.25)
}
