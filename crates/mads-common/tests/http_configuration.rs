//! Conventional HTTP configuration loading and server decision contracts.

#![cfg(feature = "http")]
#![allow(missing_docs)]

use mads_common::__private::{
    enable_automatic_cors_for_test, enable_automatic_server_for_test,
    load_standard_config_from_for_test, server_binding_address_for_test,
};
use mads_common::core::{
    AutoConfigurationReport, AutoConfigurationStatus, Config, ConfigBuilder, EnvSource,
    GraphAnalysis, MADS020, Mads, MadsBuilder, MapSource, Module, TomlSource,
};

const CORS_ID: &str = "mads.common.http.cors";
const SERVER_ID: &str = "mads.common.http.server";

#[test]
fn common_manifest_has_no_database_feature() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!manifest.lines().any(|line| line.starts_with("database =")));
    assert!(!manifest.contains("deadpool-diesel"));
    assert!(!manifest.contains("diesel_migrations"));
}

#[derive(Clone, Copy)]
enum CorsValue {
    Scalar(&'static str, &'static str),
    StringArray(&'static str, &'static [&'static str]),
}

mod routed {
    #[mads_common::routes]
    pub trait RoutedRoutes {
        #[mads_common::get("/health")]
        async fn health(&self) -> &'static str;
    }

    #[mads_common::controller(routes = [RoutedRoutes])]
    pub struct RoutedController;

    impl RoutedRoutes for RoutedController {
        async fn health(&self) -> &'static str {
            "healthy"
        }
    }

    #[mads_common::core::module]
    pub struct RoutedApp;
}

mod empty {
    #[mads_common::core::module]
    pub struct EmptyApp;
}

fn config(values: impl IntoIterator<Item = (&'static str, &'static str)>) -> Config {
    ConfigBuilder::new()
        .source(MapSource::new("test", values))
        .build()
        .unwrap()
}

fn cors_config(values: impl IntoIterator<Item = CorsValue>) -> Config {
    let mut builder = ConfigBuilder::new();
    for value in values {
        builder = match value {
            CorsValue::Scalar(key, value) => builder.source(MapSource::new("test", [(key, value)])),
            CorsValue::StringArray(key, values) => builder.source(
                MapSource::new("test", std::iter::empty::<(&str, &str)>())
                    .with_string_array(key, values.iter().copied()),
            ),
        };
    }
    builder.build().unwrap()
}

fn cors_analysis(config: Config) -> GraphAnalysis {
    Mads::builder_with_config(config).analyze()
}

fn cors_report(reports: &[AutoConfigurationReport]) -> &AutoConfigurationReport {
    reports
        .iter()
        .find(|report| report.identifier() == CORS_ID)
        .expect("the CORS auto-configuration must be registered")
}

fn invalid_cors_output(config: Config) -> (String, String) {
    let analysis = cors_analysis(config);
    let cors = cors_report(analysis.auto_configurations());
    let diagnostics = analysis
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect::<String>();

    assert_eq!(cors.status(), AutoConfigurationStatus::Failed);
    assert_eq!(cors.reason_code().as_str(), "invalid_configuration");
    (format!("{cors:?}"), diagnostics)
}

fn assert_invalid_cors(config: Config, redacted_value: Option<&str>) {
    let (debug, diagnostics) = invalid_cors_output(config);
    if let Some(redacted_value) = redacted_value {
        assert!(!debug.contains(redacted_value));
        assert!(!diagnostics.contains(redacted_value));
    }
}

fn automatic_builder<M: Module>(config: Config) -> MadsBuilder {
    let mut builder = Mads::builder_with_config(config);
    builder.root::<M>().unwrap();
    assert!(enable_automatic_server_for_test(&mut builder));
    builder
}

fn automatic_rootless_builder(config: Config) -> MadsBuilder {
    let mut builder = Mads::builder_with_config(config);
    assert!(enable_automatic_server_for_test(&mut builder));
    builder
}

fn automatic_cors_builder<M: Module>(config: Config) -> MadsBuilder {
    let mut builder = Mads::builder_with_config(config);
    builder.root::<M>().unwrap();
    assert!(enable_automatic_cors_for_test(&mut builder));
    builder
}

fn report(reports: &[AutoConfigurationReport]) -> &AutoConfigurationReport {
    reports
        .iter()
        .find(|report| report.identifier() == SERVER_ID)
        .expect("the HTTP server auto-configuration must be registered")
}

fn environment(values: impl IntoIterator<Item = (&'static str, &'static str)>) -> EnvSource {
    EnvSource::from_iter("MADS_", values)
}

#[test]
fn conventional_sources_apply_dotenv_toml_and_environment_precedence() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".env"), "APP_PORT=3100\n").unwrap();
    std::fs::write(
        root.path().join("mads.toml"),
        "[server]\nport = \"${APP_PORT}\"\nhost = \"127.0.0.1\"\n",
    )
    .unwrap();

    let config = load_standard_config_from_for_test(
        root.path(),
        environment([("MADS_SERVER__PORT", "3200")]),
    )
    .unwrap();

    assert_eq!(config.get("server.port"), Some("3200"));
    assert_eq!(config.source_of("server.port"), Some("environment"));
    assert_eq!(config.get("server.host"), Some("127.0.0.1"));
    assert_eq!(config.get("APP_PORT"), None);
}

#[test]
fn conventional_sources_allow_both_optional_files_to_be_absent() {
    let root = tempfile::tempdir().unwrap();

    let config = load_standard_config_from_for_test(root.path(), environment([])).unwrap();

    assert!(config.is_empty());
}

#[test]
fn conventional_sources_reject_a_malformed_present_dotenv_file() {
    let root = tempfile::tempdir().unwrap();
    let dotenv = root.path().join(".env");
    let sentinel = "postgres://dotenv-secret";
    std::fs::write(&dotenv, format!("BROKEN='unterminated {sentinel}\n")).unwrap();

    let error = load_standard_config_from_for_test(root.path(), environment([])).unwrap_err();

    assert_eq!(error.code(), MADS020);
    assert!(error.to_string().contains(dotenv.to_str().unwrap()));
    assert!(!error.to_string().contains(sentinel));
}

#[test]
fn conventional_sources_reject_a_malformed_present_toml_file() {
    let root = tempfile::tempdir().unwrap();
    let toml = root.path().join("mads.toml");
    let sentinel = "postgres://toml-secret";
    std::fs::write(&toml, format!("[server\nport = \"{sentinel}\"\n")).unwrap();

    let error = load_standard_config_from_for_test(root.path(), environment([])).unwrap_err();

    assert_eq!(error.code(), MADS020);
    assert!(error.to_string().contains(toml.to_str().unwrap()));
    assert!(!error.to_string().contains(sentinel));
}

#[test]
fn conventional_sources_do_not_search_parent_directories() {
    let parent = tempfile::tempdir().unwrap();
    std::fs::write(
        parent.path().join("mads.toml"),
        "[server]\nport = \"3100\"\n",
    )
    .unwrap();
    let child = parent.path().join("child");
    std::fs::create_dir(&child).unwrap();

    let config = load_standard_config_from_for_test(&child, environment([])).unwrap();

    assert!(config.is_empty());
}

#[test]
fn conventional_toml_source_is_labeled_in_http_diagnostics_and_evidence() {
    let root = tempfile::tempdir().unwrap();
    let toml = root.path().join("mads.toml");
    std::fs::write(
        &toml,
        "[server]\nhost = \"*.private.example\"\n[server.cors]\norigins = [\"https://*.private.example\"]\nmethods = [\"GET\"]\n",
    )
    .unwrap();
    let config = load_standard_config_from_for_test(root.path(), environment([])).unwrap();

    let analysis = automatic_builder::<routed::RoutedApp>(config).analyze();
    let server = report(analysis.auto_configurations());
    let cors = cors_report(analysis.auto_configurations());
    let diagnostics = analysis
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect::<String>();

    assert_eq!(
        server
            .configuration()
            .iter()
            .find(|evidence| evidence.key() == "server.host")
            .and_then(|evidence| evidence.source()),
        Some("mads.toml")
    );
    assert_eq!(
        cors.configuration()
            .iter()
            .find(|evidence| evidence.key() == "server.cors.origins")
            .and_then(|evidence| evidence.source()),
        Some("mads.toml")
    );
    assert_eq!(diagnostics.matches("source: mads.toml").count(), 2);
    assert!(!diagnostics.contains(toml.to_str().unwrap()));
}

#[tokio::test]
async fn server_automatic_mode_uses_defaults_and_contributes_a_binding() {
    let application = automatic_builder::<routed::RoutedApp>(Config::empty())
        .build()
        .await
        .unwrap();

    assert_eq!(
        server_binding_address_for_test(&application).unwrap(),
        ("127.0.0.1".to_owned(), 3000)
    );
    assert_eq!(
        report(application.auto_configurations()).status(),
        AutoConfigurationStatus::Active,
    );
}

#[tokio::test]
async fn server_automatic_mode_accepts_hostnames_ip_addresses_and_explicit_ports() {
    for (host, port) in [
        ("api.internal", "4101"),
        ("192.0.2.10", "4102"),
        ("2001:db8::10", "4103"),
    ] {
        let application = automatic_builder::<routed::RoutedApp>(config([
            ("server.host", host),
            ("server.port", port),
        ]))
        .build()
        .await
        .unwrap();

        assert_eq!(
            server_binding_address_for_test(&application).unwrap(),
            (host.to_owned(), port.parse().unwrap())
        );
        assert_eq!(
            report(application.auto_configurations()).status(),
            AutoConfigurationStatus::Active,
        );
        assert_eq!(
            report(application.auto_configurations())
                .configuration()
                .iter()
                .map(|evidence| evidence.key())
                .collect::<Vec<_>>(),
            ["server.host", "server.port"]
        );
        assert!(
            report(application.auto_configurations())
                .configuration()
                .iter()
                .all(|evidence| evidence.source() == Some("test"))
        );
    }
}

#[test]
fn server_invalid_automatic_configuration_is_failed_and_redacted() {
    for (key, value, redaction_marker) in [
        ("server.host", "", None),
        ("server.host", " \t ", None),
        ("server.host", "private\nserver", Some("private\nserver")),
        ("server.port", "not-a-port", Some("not-a-port")),
        ("server.port", "0", None),
        ("server.port", "65536", Some("65536")),
    ] {
        let analysis = automatic_builder::<routed::RoutedApp>(config([(key, value)])).analyze();
        let server = report(analysis.auto_configurations());
        let diagnostics = analysis
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<String>();

        assert!(!analysis.is_valid());
        assert_eq!(server.status(), AutoConfigurationStatus::Failed);
        assert_eq!(server.reason_code().as_str(), "invalid_configuration");
        assert!(
            analysis
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == MADS020)
        );
        assert!(diagnostics.contains(key));
        assert!(diagnostics.contains("test"));
        if let Some(redaction_marker) = redaction_marker {
            assert!(!format!("{server:?}").contains(redaction_marker));
            assert!(!diagnostics.contains(redaction_marker));
        }
    }
}

#[test]
fn server_automatic_mode_rejects_invalid_host_syntax_without_exposing_it() {
    for invalid_host in [
        "https://private.example",
        "private.example:4100",
        "private.example/path",
        "-private.example",
        "private-.example",
        "private_underscore.example",
        "*.private.example",
    ] {
        let analysis =
            automatic_builder::<routed::RoutedApp>(config([("server.host", invalid_host)]))
                .analyze();
        let server = report(analysis.auto_configurations());
        let diagnostics = analysis
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<String>();

        assert!(!analysis.is_valid());
        assert_eq!(server.status(), AutoConfigurationStatus::Failed);
        assert_eq!(server.reason_code().as_str(), "invalid_configuration");
        assert!(diagnostics.contains("server.host"));
        assert!(!format!("{server:?}").contains(invalid_host));
        assert!(!diagnostics.contains(invalid_host));
    }
}

#[test]
fn rootless_automatic_mode_skips_without_parsing_server_values() {
    let invalid_host = "rootless-secret\nexample";
    let analysis = automatic_rootless_builder(config([
        ("server.host", invalid_host),
        ("server.port", "0"),
    ]))
    .analyze();
    let server = report(analysis.auto_configurations());
    let diagnostics = analysis
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect::<String>();

    assert!(analysis.is_valid());
    assert_eq!(server.status(), AutoConfigurationStatus::Skipped);
    assert_eq!(server.reason_code().as_str(), "no_managed_routes");
    assert!(
        analysis
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code() != MADS020)
    );
    assert!(!diagnostics.contains(invalid_host));
}

#[tokio::test]
async fn server_automatic_mode_skips_before_parsing_when_the_root_has_no_routes() {
    let application = automatic_builder::<empty::EmptyApp>(config([
        ("server.host", "private.example"),
        ("server.port", "0"),
    ]))
    .build()
    .await
    .unwrap();

    let server = report(application.auto_configurations());
    assert_eq!(server.status(), AutoConfigurationStatus::Skipped);
    assert_eq!(server.reason_code().as_str(), "no_managed_routes");
    assert!(server_binding_address_for_test(&application).is_err());
}

#[tokio::test]
async fn low_level_builder_overrides_the_server_decision_without_parsing_server_values() {
    let configured_host = "private.example";
    let configured_port = "0";
    let mut builder = Mads::builder_with_config(config([
        ("server.host", configured_host),
        ("server.port", configured_port),
    ]));
    builder.root::<routed::RoutedApp>().unwrap();
    let application = builder.build().await.unwrap();

    let server = report(application.auto_configurations());
    assert_eq!(server.status(), AutoConfigurationStatus::Overridden);
    assert_eq!(server.reason_code().as_str(), "explicit_listener");
    let debug = format!("{server:?}");
    assert!(!debug.contains(configured_host));
    assert!(!debug.contains(configured_port));
}

async fn assert_valid_cors(config: Config) {
    let application = Mads::builder_with_config(config).build().await.unwrap();
    let cors = cors_report(application.auto_configurations());

    assert_eq!(cors.status(), AutoConfigurationStatus::Active);
    assert_eq!(cors.reason_code().as_str(), "conditions_matched");
}

#[test]
fn absent_cors_configuration_is_skipped() {
    let analysis = cors_analysis(cors_config([]));
    let cors = cors_report(analysis.auto_configurations());

    assert!(analysis.is_valid());
    assert_eq!(cors.status(), AutoConfigurationStatus::Skipped);
    assert_eq!(cors.reason_code().as_str(), "configuration_absent");
}

#[test]
fn automatic_cors_skips_present_invalid_configuration_without_managed_routes() {
    const INVALID_ORIGIN: &str = "https://app.example.com/";
    let analysis = automatic_cors_builder::<empty::EmptyApp>(cors_config([
        CorsValue::StringArray("server.cors.origins", &[INVALID_ORIGIN]),
        CorsValue::StringArray("server.cors.methods", &["GET"]),
    ]))
    .analyze();
    let cors = cors_report(analysis.auto_configurations());
    let diagnostics = analysis
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect::<String>();

    assert!(analysis.is_valid());
    assert_eq!(cors.status(), AutoConfigurationStatus::Skipped);
    assert_eq!(cors.reason_code().as_str(), "no_managed_routes");
    assert!(!diagnostics.contains(INVALID_ORIGIN));
}

#[test]
fn cors_empty_table_is_present_and_requires_origins_and_methods() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mads.toml");
    std::fs::write(&path, "[server.cors]\n").unwrap();
    let config = ConfigBuilder::new()
        .source(TomlSource::file(&path))
        .build()
        .unwrap();

    assert_invalid_cors(config, None);
}

#[test]
fn cors_requires_both_origins_and_methods() {
    assert_invalid_cors(
        cors_config([CorsValue::StringArray("server.cors.methods", &["GET"])]),
        None,
    );
    assert_invalid_cors(
        cors_config([CorsValue::StringArray(
            "server.cors.origins",
            &["https://app.example.com"],
        )]),
        None,
    );
}

#[tokio::test]
async fn cors_accepts_wildcard_origins_and_normalizes_methods() {
    assert_valid_cors(cors_config([
        CorsValue::Scalar("server.cors.origins", "*"),
        CorsValue::StringArray("server.cors.methods", &["GET", "post", "GET"]),
    ]))
    .await;
}

#[test]
fn cors_rejects_ambiguous_and_credentialed_wildcards() {
    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &["*"]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
        ]),
        None,
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::Scalar("server.cors.origins", "*"),
            CorsValue::StringArray("server.cors.methods", &["*"]),
        ]),
        None,
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::Scalar("server.cors.origins", "*"),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
            CorsValue::Scalar("server.cors.credentials", "true"),
        ]),
        None,
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
            CorsValue::Scalar("server.cors.allowed_headers", "*"),
            CorsValue::Scalar("server.cors.credentials", "true"),
        ]),
        None,
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
            CorsValue::Scalar("server.cors.exposed_headers", "*"),
            CorsValue::Scalar("server.cors.credentials", "true"),
        ]),
        None,
    );
}

#[tokio::test]
async fn cors_accepts_explicit_origins_with_credentials_and_defaults() {
    assert_valid_cors(cors_config([
        CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
        CorsValue::StringArray("server.cors.methods", &["GET"]),
        CorsValue::Scalar("server.cors.credentials", "true"),
    ]))
    .await;
}

#[tokio::test]
async fn cors_accepts_wildcard_headers_and_empty_explicit_header_lists() {
    assert_valid_cors(cors_config([
        CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
        CorsValue::StringArray("server.cors.methods", &["GET"]),
        CorsValue::Scalar("server.cors.allowed_headers", "*"),
        CorsValue::Scalar("server.cors.exposed_headers", "*"),
    ]))
    .await;
    assert_valid_cors(cors_config([
        CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
        CorsValue::StringArray("server.cors.methods", &["GET"]),
        CorsValue::StringArray("server.cors.allowed_headers", &[]),
        CorsValue::StringArray("server.cors.exposed_headers", &[]),
        CorsValue::Scalar("server.cors.max_age_seconds", "0"),
    ]))
    .await;
}

#[test]
fn cors_rejects_invalid_shapes_booleans_and_max_age() {
    for invalid in [
        cors_config([
            CorsValue::Scalar("server.cors.origins", "https://app.example.com"),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
        ]),
        cors_config([
            CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
            CorsValue::Scalar("server.cors.methods", "GET"),
        ]),
        cors_config([
            CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
            CorsValue::Scalar("server.cors.credentials", "TRUE"),
        ]),
        cors_config([
            CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
            CorsValue::Scalar("server.cors.max_age_seconds", "sixty"),
        ]),
    ] {
        assert_invalid_cors(invalid, None);
    }
}

#[test]
fn cors_rejects_glob_and_regex_like_origin_authorities_without_exposing_them() {
    const GLOB_ORIGIN: &str = "https://*.private.example";
    const REGEX_LIKE_ORIGIN: &str = "https://.+.private.example";
    const EMPTY_PORT_ORIGIN: &str = "https://private.example:";
    const OUT_OF_RANGE_PORT_ORIGIN: &str = "https://private.example:65536";

    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &[GLOB_ORIGIN]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
        ]),
        Some(GLOB_ORIGIN),
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &[REGEX_LIKE_ORIGIN]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
        ]),
        Some(REGEX_LIKE_ORIGIN),
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &[EMPTY_PORT_ORIGIN]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
        ]),
        Some(EMPTY_PORT_ORIGIN),
    );
    assert_invalid_cors(
        cors_config([
            CorsValue::StringArray("server.cors.origins", &[OUT_OF_RANGE_PORT_ORIGIN]),
            CorsValue::StringArray("server.cors.methods", &["GET"]),
        ]),
        Some(OUT_OF_RANGE_PORT_ORIGIN),
    );
}

#[tokio::test]
async fn cors_accepts_ip_origin_authorities_with_valid_ports() {
    assert_valid_cors(cors_config([
        CorsValue::StringArray(
            "server.cors.origins",
            &["http://192.0.2.10:8080", "https://[2001:db8::10]:8443"],
        ),
        CorsValue::StringArray("server.cors.methods", &["GET"]),
    ]))
    .await;
}

#[test]
fn cors_rejects_invalid_origins_and_headers_without_exposing_values() {
    for (config, invalid_value) in [
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["https://app.example.com/"]),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
            ]),
            "https://app.example.com/",
        ),
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["https://user@example.com"]),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
            ]),
            "https://user@example.com",
        ),
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["null"]),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
            ]),
            "null",
        ),
        (
            cors_config([
                CorsValue::StringArray(
                    "server.cors.origins",
                    &["https://app.example.com#fragment"],
                ),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
            ]),
            "https://app.example.com#fragment",
        ),
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
                CorsValue::StringArray("server.cors.allowed_headers", &["bad header"]),
            ]),
            "bad header",
        ),
    ] {
        assert_invalid_cors(config, Some(invalid_value));
    }
}

#[test]
fn cors_reports_positions_for_invalid_array_values_without_exposing_them() {
    for (config, key, invalid_value) in [
        (
            cors_config([
                CorsValue::StringArray(
                    "server.cors.origins",
                    &[
                        "https://app.example.com",
                        "https://app.example.com#private-fragment",
                    ],
                ),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
            ]),
            "server.cors.origins",
            "https://app.example.com#private-fragment",
        ),
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
                CorsValue::StringArray("server.cors.methods", &["GET", "not a method"]),
            ]),
            "server.cors.methods",
            "not a method",
        ),
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
                CorsValue::StringArray(
                    "server.cors.allowed_headers",
                    &["authorization", "bad allowed header"],
                ),
            ]),
            "server.cors.allowed_headers",
            "bad allowed header",
        ),
        (
            cors_config([
                CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
                CorsValue::StringArray("server.cors.methods", &["GET"]),
                CorsValue::StringArray(
                    "server.cors.exposed_headers",
                    &["x-request-id", "bad exposed header"],
                ),
            ]),
            "server.cors.exposed_headers",
            "bad exposed header",
        ),
    ] {
        let (_, diagnostics) = invalid_cors_output(config);

        assert!(diagnostics.contains(&format!("{key} element at position 2")));
        assert!(!diagnostics.contains(invalid_value));
    }
}

#[test]
fn cors_reports_only_approved_evidence_keys_and_sources() {
    let analysis = cors_analysis(cors_config([
        CorsValue::StringArray("server.cors.origins", &["https://app.example.com"]),
        CorsValue::StringArray("server.cors.methods", &["GET"]),
        CorsValue::StringArray("server.cors.allowed_headers", &["authorization"]),
        CorsValue::StringArray("server.cors.exposed_headers", &["x-request-id"]),
        CorsValue::Scalar("server.cors.credentials", "false"),
        CorsValue::Scalar("server.cors.max_age_seconds", "600"),
    ]));
    let cors = cors_report(analysis.auto_configurations());

    assert_eq!(cors.status(), AutoConfigurationStatus::Active);
    assert_eq!(
        cors.configuration()
            .iter()
            .map(|evidence| evidence.key())
            .collect::<Vec<_>>(),
        [
            "server.cors.origins",
            "server.cors.methods",
            "server.cors.allowed_headers",
            "server.cors.exposed_headers",
            "server.cors.credentials",
            "server.cors.max_age_seconds",
        ]
    );
    assert!(
        cors.configuration()
            .iter()
            .all(|evidence| evidence.source() == Some("test"))
    );
}
