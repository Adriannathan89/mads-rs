//! Typed configuration aggregation, parsing, and secret boundaries.

use mads_core::{ConfigurationErrors, ConfigurationIssue, MADS020, Secret};

#[test]
fn structured_errors_and_secrets_preserve_order_and_safe_diagnostics() {
    let mut errors = ConfigurationErrors::from_issue(ConfigurationIssue::new(
        "app.host",
        "required",
        "required configuration key is missing",
    ));
    errors.push(
        ConfigurationIssue::new(
            "app.port",
            "invalid_type",
            "configuration value has an invalid type",
        )
        .with_source("environment"),
    );
    errors.extend(ConfigurationErrors::from_issue(
        ConfigurationIssue::new("app.name", "too_small", "value is too small")
            .with_source("default"),
    ));
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| issue.key())
            .collect::<Vec<_>>(),
        ["app.host", "app.port", "app.name"]
    );
    assert_eq!(errors.issues()[0].source(), None);
    assert_eq!(errors.issues()[1].source(), Some("environment"));
    assert_eq!(errors.issues()[1].code(), "invalid_type");
    assert_eq!(
        errors.issues()[1].message(),
        "configuration value has an invalid type"
    );
    assert!(std::error::Error::source(&errors).is_none());
    assert!(errors.to_string().contains("app.port"));
    let framework: mads_core::Error = errors.into();
    assert_eq!(framework.diagnostics().len(), 3);
    assert!(
        framework
            .diagnostics()
            .iter()
            .all(|item| item.code() == MADS020)
    );
    assert_eq!(framework.diagnostics()[1].subject(), Some("app.port"));
    assert!(framework.diagnostics()[1].message().contains("environment"));
    assert!(
        framework.diagnostics()[1]
            .message()
            .contains("invalid_type")
    );
    assert!(std::error::Error::source(&framework).is_none());
}

#[test]
fn structured_errors_and_secrets_redact_without_inner_formatting_bounds() {
    struct Opaque;
    let opaque = Secret::new(Opaque);
    assert_eq!(format!("{opaque}"), "[REDACTED]");
    assert_eq!(format!("{opaque:?}"), "[REDACTED]");
    let secret = Secret::new(String::from("configuration-secret-sentinel"));
    assert_eq!(secret, secret.clone());
    assert_eq!(secret.expose(), "configuration-secret-sentinel");
    assert_eq!(format!("{secret:#?}"), "[REDACTED]");
    assert_eq!(format!("{secret}"), "[REDACTED]");
    assert_eq!(secret.into_exposed(), "configuration-secret-sentinel");
}
