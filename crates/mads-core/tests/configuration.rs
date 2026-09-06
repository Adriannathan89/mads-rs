//! Typed configuration aggregation, parsing, and secret boundaries.

use mads_core::{ConfigurationErrors, ConfigurationIssue, MADS020, Secret};

use mads_core::{Config, ConfigBuilder, Configuration, ConfigurationResult, MapSource};

#[test]
fn typed_configuration_uses_existing_dotenv_toml_and_environment_precedence() {
    #[derive(Configuration)]
    #[config(prefix = "app")]
    struct Settings {
        host: String,
        port: u16,
    }
    let directory = tempfile::tempdir().unwrap();
    let dotenv = directory.path().join(".env");
    let toml = directory.path().join("mads.toml");
    std::fs::write(&dotenv, "MADS_TYPED_FIXTURE_HOST=from-dotenv\n").unwrap();
    std::fs::write(
        &toml,
        "[app]\nhost = \"${MADS_TYPED_FIXTURE_HOST}\"\nport = 3000\n",
    )
    .unwrap();
    let config = ConfigBuilder::new()
        .dotenv(mads_core::DotenvSource::required(dotenv))
        .source(mads_core::TomlSource::file(&toml))
        .source(mads_core::EnvSource::from_iter(
            "MADS_",
            [
                ("MADS_APP__HOST", "from-environment"),
                ("MADS_APP__PORT", "8080"),
            ],
        ))
        .build()
        .unwrap();
    let settings = config.parse::<Settings>().unwrap();
    assert_eq!(settings.host, "from-environment");
    assert_eq!(settings.port, 8080);
    assert_eq!(config.source_of("app.host"), Some("environment"));
    assert_eq!(config.source_of("app.port"), Some("environment"));
    assert_eq!(config.get("MADS_TYPED_FIXTURE_HOST"), None);
    assert_eq!(config.len(), 2);
}

#[test]
fn configuration_validator_matrix_email_policy_and_decimal_boundaries() {
    use mads_core::__private::configuration_validation::{email, multiple};
    for value in [
        "a@b.co",
        "O'Neil@example.com",
        "a+b@example.com",
        "a_b@example.com",
        "a@sub.example.com",
        "a@host-.com",
    ] {
        assert!(email(value), "{value}");
    }
    for value in [
        "",
        "a",
        "a@b",
        ".a@example.com",
        "a.@example.com",
        "a..b@example.com",
        "a'@example.com",
        "a@-host.com",
        "a@host.c",
        "a@host.12",
        "a@host..com",
        "a@host.com\n",
        "é@example.com",
        "a@é.com",
        "a b@example.com",
        "a@example.com ",
        "\"a\"@example.com",
        "a@127.0.0.1",
    ] {
        assert!(!email(value), "{value}");
    }
    assert!(multiple(0.3, 0.1, f64::EPSILON));
    assert!(multiple(-0.3, 0.1, f64::EPSILON));
    assert!(multiple(0.3_f32 as f64, 0.1, f32::EPSILON as f64));
    assert!(multiple(3e-12, 1e-12, f64::EPSILON));
    assert!(!multiple(0.31, 0.1, f64::EPSILON));
    assert!(!multiple(0.300000001, 0.1, f64::EPSILON));
    assert!(!multiple(3.1e-12, 1e-12, f64::EPSILON));
    assert!(!multiple(f64::NAN, 0.1, f64::EPSILON));
    assert!(!multiple(f64::INFINITY, 0.1, f64::EPSILON));
    assert!(!multiple(1.0, 0.0, f64::EPSILON));
}

#[test]
fn configuration_validator_matrix_float_defaults_ranges_and_custom_finiteness() {
    fn nonfinite(_: &str) -> Result<f32, ()> {
        Ok(f32::NAN)
    }
    #[derive(Configuration, Debug)]
    struct Floats {
        #[config(default = 1, validate(range(min = 0, max = 2)))]
        value: f32,
        #[config(parse_with = nonfinite)]
        custom: Option<f32>,
    }
    assert_eq!(Config::empty().parse::<Floats>().unwrap().value, 1.0);
    assert!(Config::empty().parse::<Floats>().unwrap().custom.is_none());
    let config = ConfigBuilder::new()
        .source(MapSource::new("fixture", [("custom", "sensitive")]))
        .build()
        .unwrap();
    let errors = config.parse::<Floats>().unwrap_err();
    assert_eq!(errors.issues()[0].code(), "invalid_type");
    assert_eq!(errors.issues()[0].source(), Some("fixture"));
}

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

#[derive(Configuration, Debug)]
#[config(prefix = "app")]
struct DerivedApp<T: Configuration> {
    #[config(rename = "bind_host")]
    host: String,
    #[config(default = 3000)]
    port: u16,
    enabled: bool,
    token: Secret<String>,
    optional_token: Option<Secret<u64>>,
    labels: Vec<String>,
    database: DerivedDatabase,
    marker: Option<T>,
}

#[derive(Configuration, Debug)]
#[config(prefix = "connection")]
struct DerivedDatabase {
    port: u16,
    path: std::path::PathBuf,
}

#[derive(Configuration, Debug)]
#[config(prefix = "")]
struct DerivedMarker {
    name: String,
}

#[test]
fn configuration_validator_matrix_custom_parsers_keep_errors_private() {
    #[derive(Debug, PartialEq)]
    struct Port(u16);
    fn port(raw: &str) -> Result<Port, String> {
        raw.strip_prefix("port:")
            .and_then(|raw| raw.parse().ok())
            .map(Port)
            .ok_or_else(|| format!("sensitive input: {raw}"))
    }
    #[derive(Configuration, Debug)]
    struct Parsed {
        #[config(parse_with = port)]
        port: Port,
        #[config(parse_with = port)]
        secret: Secret<Port>,
        #[config(parse_with = port)]
        optional: Option<Port>,
    }
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "environment",
            [("port", "port:8080"), ("secret", "port:9000")],
        ))
        .build()
        .unwrap();
    let value = config.parse::<Parsed>().unwrap();
    assert_eq!(value.port, Port(8080));
    assert_eq!(value.secret.expose(), &Port(9000));
    assert!(value.optional.is_none());
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "environment",
            [
                ("port", "secret-sentinel"),
                ("secret", "secret-sentinel"),
                ("optional", "secret-sentinel"),
            ],
        ))
        .build()
        .unwrap();
    let errors = config.parse::<Parsed>().unwrap_err();
    assert_eq!(errors.issues().len(), 3);
    assert!(
        errors
            .issues()
            .iter()
            .all(|issue| issue.code() == "invalid_type" && issue.source() == Some("environment"))
    );
    assert!(!format!("{errors:?} {errors}").contains("secret-sentinel"));
}

#[test]
fn configuration_validator_matrix_checks_strings_numbers_arrays_and_defaults() {
    #[derive(Configuration, Debug)]
    struct Validated {
        #[config(validate(email, length(max = 40)))]
        email: String,
        #[config(validate(length(min = 2, max = 3)))]
        name: String,
        #[config(validate(range(min = 1, max = 10), positive, multiple_of = 2))]
        count: u64,
        #[config(validate(negative, multiple_of = -1))]
        negative: i64,
        #[config(validate(multiple_of = 0.1))]
        decimal: f64,
        #[config(validate(nonempty, length(max = 2)))]
        labels: Vec<String>,
        #[config(validate(length(exact = 2)))]
        optional: Option<String>,
        #[config(default = 0, validate(positive))]
        defaulted: i32,
    }
    let config = ConfigBuilder::new()
        .source(
            MapSource::new(
                "fixture",
                [
                    ("email", "person@example.com"),
                    ("name", "é🦀"),
                    ("count", "2"),
                    ("negative", "-9223372036854775808"),
                    ("decimal", "0.3"),
                    ("defaulted", "1"),
                ],
            )
            .with_string_array("labels", ["one"]),
        )
        .build()
        .unwrap();
    let value = config.parse::<Validated>().unwrap();
    assert_eq!(value.email, "person@example.com");
    assert_eq!(value.name, "é🦀");
    assert_eq!(value.count, 2);
    assert_eq!(value.negative, i64::MIN);
    assert_eq!(value.decimal, 0.3);
    assert_eq!(value.labels, ["one"]);
    assert!(value.optional.is_none());
    assert_eq!(value.defaulted, 1);

    let config = ConfigBuilder::new()
        .source(
            MapSource::new(
                "environment",
                [
                    ("email", "invalid"),
                    ("name", "é"),
                    ("count", "11"),
                    ("negative", "0"),
                    ("decimal", "0.31"),
                    ("optional", "long"),
                ],
            )
            .with_string_array("labels", std::iter::empty::<&str>()),
        )
        .build()
        .unwrap();
    let errors = config.parse::<Validated>().unwrap_err();
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| (issue.key(), issue.code()))
            .collect::<Vec<_>>(),
        [
            ("email", "invalid_format"),
            ("name", "too_small"),
            ("count", "too_big"),
            ("count", "not_multiple_of"),
            ("negative", "too_big"),
            ("decimal", "not_multiple_of"),
            ("labels", "too_small"),
            ("optional", "too_big"),
            ("defaulted", "too_small"),
        ]
    );
    assert_eq!(errors.issues().last().unwrap().source(), Some("default"));
}

#[test]
fn derive_prefix_nested_and_default_matrix_builds_optional_and_generic_fields() {
    let config = ConfigBuilder::new()
        .source(
            MapSource::new(
                "fixture",
                [
                    ("app.bind_host", "localhost"),
                    ("app.enabled", "true"),
                    ("app.token", "secret-sentinel"),
                    ("app.database.connection.port", "5432"),
                    ("app.database.connection.path", "/database"),
                    ("unrelated", "allowed"),
                ],
            )
            .with_string_array("app.labels", ["b", "a"]),
        )
        .build()
        .unwrap();
    let app = config.parse::<DerivedApp<DerivedMarker>>().unwrap();
    assert_eq!(app.host, "localhost");
    assert_eq!(app.port, 3000);
    assert!(app.enabled);
    assert_eq!(app.token.expose(), "secret-sentinel");
    assert!(app.optional_token.is_none());
    assert_eq!(app.labels, ["b", "a"]);
    assert_eq!(app.database.port, 5432);
    assert_eq!(app.database.path, std::path::PathBuf::from("/database"));
    assert!(app.marker.is_none());

    let config = ConfigBuilder::new()
        .source(MapSource::new("fixture", [("name", "marker")]))
        .build()
        .unwrap();
    assert_eq!(config.parse::<DerivedMarker>().unwrap().name, "marker");
}

#[test]
fn derive_prefix_nested_and_default_matrix_aggregates_without_default_fallback() {
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "environment",
            [
                ("app.port", "secret-sentinel"),
                ("app.enabled", "maybe"),
                ("app.optional_token", "invalid"),
                ("app.marker.unknown", "present"),
            ],
        ))
        .build()
        .unwrap();
    let errors = config.parse::<DerivedApp<DerivedMarker>>().unwrap_err();
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| issue.key())
            .collect::<Vec<_>>(),
        [
            "app.bind_host",
            "app.port",
            "app.enabled",
            "app.token",
            "app.optional_token",
            "app.labels",
            "app.database.connection.port",
            "app.database.connection.path",
            "app.marker.name",
        ]
    );
    assert_eq!(errors.issues()[1].code(), "invalid_type");
    assert_eq!(errors.issues()[1].source(), Some("environment"));
    assert!(!format!("{errors:?} {errors}").contains("secret-sentinel"));
}

#[test]
fn derive_prefix_nested_and_default_matrix_detects_empty_optional_tables_and_wrong_shapes() {
    #[derive(Configuration, Debug)]
    struct Parent {
        child: Option<DerivedMarker>,
    }
    let directory = tempfile::tempdir().unwrap();
    let toml = directory.path().join("mads.toml");
    std::fs::write(&toml, "[child]\n").unwrap();
    let config = ConfigBuilder::new()
        .source(mads_core::TomlSource::file(toml))
        .build()
        .unwrap();
    assert_eq!(
        config.parse::<Parent>().unwrap_err().issues()[0].key(),
        "child.name"
    );
    assert!(Config::empty().parse::<Parent>().unwrap().child.is_none());
    let config = ConfigBuilder::new()
        .source(MapSource::new("fixture", [("child", "secret")]))
        .build()
        .unwrap();
    let errors = config.parse::<Parent>().unwrap_err();
    assert_eq!(errors.issues()[0].key(), "child");
    assert_eq!(errors.issues()[0].code(), "invalid_type");
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
