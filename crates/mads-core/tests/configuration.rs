//! Typed configuration aggregation, parsing, and secret boundaries.

use mads_core::{ConfigurationErrors, ConfigurationIssue, MADS020, Secret};

use mads_core::{Config, ConfigBuilder, Configuration, ConfigurationResult, MapSource};

#[test]
fn manual_configuration_and_scalar_matrix_resolves_paths_with_winning_origin() {
    use mads_core::__private::configuration::path;
    struct FileConfig(std::path::PathBuf);
    impl Configuration for FileConfig {
        fn from_config(config: &Config) -> ConfigurationResult<Self> {
            Ok(Self(path(config, "file")?))
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let toml = directory.path().join("mads.toml");
    std::fs::write(&toml, "[app]\nfile = \"keys/private.pem\"\n").unwrap();
    let config = ConfigBuilder::new()
        .source(mads_core::TomlSource::file(&toml))
        .build()
        .unwrap();
    assert_eq!(
        FileConfig::__from_config_prefix(&config, "app").unwrap().0,
        directory.path().join("keys/private.pem")
    );
    let config = ConfigBuilder::new()
        .source(mads_core::TomlSource::file(toml))
        .source(mads_core::EnvSource::from_iter(
            "MADS_",
            [("MADS_APP__FILE", "override.pem")],
        ))
        .build()
        .unwrap();
    assert_eq!(
        FileConfig::__from_config_prefix(&config, "app").unwrap().0,
        std::env::current_dir().unwrap().join("override.pem")
    );
    assert_eq!(
        path(&Config::empty(), "file").unwrap_err().issues()[0].code(),
        "required"
    );
    let config = ConfigBuilder::new()
        .source(
            MapSource::new("array", std::iter::empty::<(&str, &str)>())
                .with_string_array("file", ["secret"]),
        )
        .build()
        .unwrap();
    assert_eq!(
        path(&config, "file").unwrap_err().issues()[0].code(),
        "invalid_type"
    );
}

struct ManualConfig {
    port: u16,
}

impl Configuration for ManualConfig {
    fn from_config(config: &Config) -> ConfigurationResult<Self> {
        Ok(Self {
            port: mads_core::__private::configuration::scalar(config, "port")?,
        })
    }
}

#[test]
fn manual_configuration_and_scalar_matrix_preserves_manual_prefixes() {
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "environment",
            [("port", "3000"), ("app.port", "8080")],
        ))
        .build()
        .unwrap();
    assert_eq!(config.parse::<ManualConfig>().unwrap().port, 3000);
    assert_eq!(
        ManualConfig::__from_config_prefix(&config, "app")
            .unwrap()
            .port,
        8080
    );
    let invalid = ConfigBuilder::new()
        .source(MapSource::new(
            "environment",
            [("app.port", "secret-sentinel")],
        ))
        .build()
        .unwrap();
    let errors = ManualConfig::__from_config_prefix(&invalid, "app")
        .err()
        .unwrap();
    assert_eq!(errors.issues()[0].key(), "app.port");
    assert_eq!(errors.issues()[0].source(), Some("environment"));
    assert!(!format!("{errors:?} {errors}").contains("secret-sentinel"));
    let framework: mads_core::Error = errors.into();
    assert!(!format!("{framework:?} {framework}").contains("secret-sentinel"));
}

#[test]
fn manual_configuration_and_scalar_matrix_checks_widths_and_finiteness() {
    use mads_core::__private::configuration::scalar;
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "fixture",
            [
                ("one", "1"),
                ("negative", "-1"),
                ("overflow", "340282366920938463463374607431768211456"),
                ("bool", "true"),
                ("char", "é"),
                ("chars", "ab"),
                ("string", "  unchanged  "),
                ("nan", "NaN"),
                ("infinity", "inf"),
                ("huge", "1e1000"),
            ],
        ))
        .build()
        .unwrap();
    macro_rules! integers {
        ($($ty:ty),+ $(,)?) => {$(
            assert_eq!(scalar::<$ty>(&config, "one").unwrap(), 1);
            assert_eq!(scalar::<$ty>(&config, "overflow").unwrap_err().issues()[0].code(), "invalid_type");
        )+};
    }
    integers!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );
    assert!(scalar::<u8>(&config, "negative").is_err());
    assert_eq!(scalar::<i8>(&config, "negative").unwrap(), -1);
    assert!(scalar::<bool>(&config, "bool").unwrap());
    assert!(scalar::<bool>(&config, "one").is_err());
    assert_eq!(scalar::<char>(&config, "char").unwrap(), 'é');
    assert!(scalar::<char>(&config, "chars").is_err());
    assert_eq!(
        scalar::<String>(&config, "string").unwrap(),
        "  unchanged  "
    );
    for key in ["nan", "infinity", "huge"] {
        assert!(scalar::<f32>(&config, key).is_err());
        assert!(scalar::<f64>(&config, key).is_err());
    }
    assert_eq!(scalar::<f32>(&config, "one").unwrap(), 1.0);
    assert_eq!(scalar::<f64>(&config, "one").unwrap(), 1.0);
    let missing = scalar::<String>(&config, "absent").unwrap_err();
    assert_eq!(missing.issues()[0].code(), "required");
    assert_eq!(missing.issues()[0].source(), None);
}

#[test]
fn manual_configuration_and_scalar_matrix_preserves_shapes_and_table_presence() {
    use mads_core::__private::configuration::{present, scalar, string_array};
    let config = ConfigBuilder::new()
        .source(
            MapSource::new(
                "fixture",
                [("app.scalar", "one"), ("application.ignored", "two")],
            )
            .with_string_array("app.items", ["second", "first"]),
        )
        .build()
        .unwrap();
    assert_eq!(
        string_array(&config, "app.items").unwrap(),
        ["second", "first"]
    );
    assert_eq!(
        scalar::<String>(&config, "app.items").unwrap_err().issues()[0].source(),
        Some("fixture")
    );
    assert_eq!(
        string_array(&config, "app.scalar").unwrap_err().issues()[0].code(),
        "invalid_type"
    );
    assert_eq!(
        string_array(&config, "absent").unwrap_err().issues()[0].code(),
        "required"
    );
    assert!(present(&config, "app"));
    assert!(!present(&config, "ap"));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mads.toml");
    std::fs::write(&path, "[app.empty]\n").unwrap();
    let tables = ConfigBuilder::new()
        .source(mads_core::TomlSource::file(path))
        .build()
        .unwrap();
    assert!(present(&tables, "app.empty"));
    assert!(present(&tables, "app"));
    assert!(!present(&tables, "other"));
}

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
