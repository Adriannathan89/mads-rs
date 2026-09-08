//! JSON contract coverage for finite MADS CLI output.

use std::{fs, path::Path, process::Output};

use assert_cmd::Command;
use mads_cli::output::{
    json::render,
    model::{
        CliDiagnostic, CommandData, DatabaseGenerateData, DatabaseMigrateData,
        DatabaseRollbackData, DatabaseStatusData, Envelope, RoutesData, SourceLocation,
    },
    path::normalize_path,
};
use serde_json::{Deserializer, Value, json};
use tempfile::tempdir;

#[test]
fn model_serializes_the_schema_v1_routes_success() {
    let output = render(&Envelope::success(
        "routes",
        CommandData::Routes(RoutesData::new(Vec::new())),
    ))
    .expect("schema-v1 routes output should serialize");

    assert_eq!(
        output,
        "{\"schema_version\":1,\"command\":\"routes\",\"ok\":true,\"data\":{\"routes\":[]},\"diagnostics\":[]}\n"
    );
    assert!(!output.contains("\u{1b}"));
}

#[test]
fn model_serializes_nullable_diagnostics_and_normalized_locations() {
    let error = CliDiagnostic::error(
        "MADS003",
        "missing provider",
        "UserService needs UserRepository",
    )
    .with_subject("UserRepository")
    .with_location(SourceLocation::new("src/app/service.rs", 8, 5))
    .with_suggestion("register UserRepository");
    let warning = CliDiagnostic::warning("MADS212", "migration review", "review generated SQL");

    let output = render(&Envelope::failure(
        Some("routes".into()),
        Some(CommandData::Routes(RoutesData::new(Vec::new()))),
        vec![error, warning],
    ))
    .expect("schema-v1 diagnostics should serialize");

    assert_eq!(
        output,
        "{\"schema_version\":1,\"command\":\"routes\",\"ok\":false,\"data\":{\"routes\":[]},\"diagnostics\":[{\"severity\":\"error\",\"code\":\"MADS003\",\"title\":\"missing provider\",\"message\":\"UserService needs UserRepository\",\"subject\":\"UserRepository\",\"location\":{\"file\":\"src/app/service.rs\",\"line\":8,\"column\":5},\"suggestions\":[\"register UserRepository\"]},{\"severity\":\"warning\",\"code\":\"MADS212\",\"title\":\"migration review\",\"message\":\"review generated SQL\",\"subject\":null,\"location\":null,\"suggestions\":[]}]}\n"
    );
    assert_eq!(
        normalize_path(
            Path::new("/workspace/app"),
            Path::new("/workspace/app/src/app/routes.rs")
        ),
        "src/app/routes.rs"
    );
    assert_eq!(
        normalize_path(
            Path::new(r"C:\workspace\app"),
            Path::new(r"C:\workspace\app\src\app\routes.rs")
        ),
        "src/app/routes.rs"
    );
}

#[test]
fn model_serializes_unavailable_operational_data_and_unknown_syntax_command() {
    let operational = render(&Envelope::failure(
        Some("db migrate".into()),
        None,
        vec![CliDiagnostic::error(
            "MADS210",
            "database command failed",
            "the database command could not be completed",
        )],
    ))
    .expect("operational failure should serialize");
    assert_eq!(
        operational,
        "{\"schema_version\":1,\"command\":\"db migrate\",\"ok\":false,\"data\":null,\"diagnostics\":[{\"severity\":\"error\",\"code\":\"MADS210\",\"title\":\"database command failed\",\"message\":\"the database command could not be completed\",\"subject\":null,\"location\":null,\"suggestions\":[]}]}\n"
    );

    let syntax = render(&Envelope::failure(
        None,
        None,
        vec![CliDiagnostic::error(
            "MADS204",
            "CLI syntax error",
            "unknown command: nope",
        )],
    ))
    .expect("syntax failure should serialize");
    assert_eq!(
        syntax,
        "{\"schema_version\":1,\"command\":null,\"ok\":false,\"data\":null,\"diagnostics\":[{\"severity\":\"error\",\"code\":\"MADS204\",\"title\":\"CLI syntax error\",\"message\":\"unknown command: nope\",\"subject\":null,\"location\":null,\"suggestions\":[]}]}\n"
    );
}

#[test]
fn model_serializes_all_database_command_data_variants() {
    let cases = [
        (
            "db generate",
            CommandData::DatabaseGenerate(DatabaseGenerateData::new(
                "generated",
                Some("migrations/20260906120000_schema_diff".into()),
                true,
            )),
            "{\"status\":\"generated\",\"migration_path\":\"migrations/20260906120000_schema_diff\",\"review_required\":true}",
        ),
        (
            "db migrate",
            CommandData::DatabaseMigrate(DatabaseMigrateData::new(vec!["20260906120000".into()])),
            "{\"applied\":[\"20260906120000\"]}",
        ),
        (
            "db rollback",
            CommandData::DatabaseRollback(DatabaseRollbackData::new(vec!["20260906120000".into()])),
            "{\"reverted\":[\"20260906120000\"]}",
        ),
        (
            "db status",
            CommandData::DatabaseStatus(DatabaseStatusData::new(
                vec!["20260906120000".into()],
                vec!["20260906120001".into()],
            )),
            "{\"applied\":[\"20260906120000\"],\"pending\":[\"20260906120001\"]}",
        ),
    ];

    for (command, data, expected_data) in cases {
        let output = render(&Envelope::success(command, data))
            .expect("database command data should serialize");
        assert_eq!(
            output,
            format!(
                "{{\"schema_version\":1,\"command\":\"{command}\",\"ok\":true,\"data\":{expected_data},\"diagnostics\":[]}}\n"
            )
        );
    }
}

#[test]
fn syntax_failure_after_json_selection_writes_one_json_document_to_stdout() {
    let output = Command::cargo_bin("mads")
        .expect("CLI binary should build")
        .args(["--format", "json", "routes", "--unknown"])
        .output()
        .expect("CLI should run");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be UTF-8"),
        "{\"schema_version\":1,\"command\":\"routes\",\"ok\":false,\"data\":null,\"diagnostics\":[{\"severity\":\"error\",\"code\":\"MADS204\",\"title\":\"CLI syntax error\",\"message\":\"unknown argument: --unknown\",\"subject\":null,\"location\":null,\"suggestions\":[]}]}\n"
    );
}

#[test]
fn new_json_success_uses_the_full_schema_v1_snapshot_for_both_format_placements() {
    for arguments in [
        ["--format", "json", "new", "my-app"],
        ["new", "my-app", "--format", "json"],
    ] {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let output = Command::cargo_bin("mads")
            .expect("CLI binary should build")
            .current_dir(invocation.path())
            .args(arguments)
            .output()
            .expect("CLI should run");

        assert!(output.status.success(), "{arguments:?}: {output:?}");
        assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
        assert_eq!(
            one_json_document(&output),
            json!({
                "schema_version": 1,
                "command": "new",
                "ok": true,
                "data": {
                    "project_name": "my-app",
                    "path": "my-app",
                    "files": [
                        "Cargo.toml",
                        "mads.toml",
                        "src/main.rs",
                        "src/app/mod.rs",
                        "src/app/routes.rs",
                        "src/app/controller.rs",
                        "src/app/service.rs"
                    ]
                },
                "diagnostics": []
            }),
            "{arguments:?}"
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("mads dev"),
            "JSON stdout must not include human next steps: {output:?}"
        );
    }
}

#[test]
fn database_operational_failure_writes_safe_json_with_null_data() {
    let project = tempdir().expect("temporary project should be created");
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"database-json-failure\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest should be written");
    fs::create_dir(project.path().join("src")).expect("source directory should be created");
    fs::write(project.path().join("src/lib.rs"), "").expect("library target should be written");
    fs::write(
        project.path().join("mads.toml"),
        "[database]\nurl = \"postgres://user:database-json-secret@127.0.0.1:1/mads\"\n",
    )
    .expect("database configuration should be written");

    let output = Command::cargo_bin("mads")
        .expect("CLI binary should build")
        .current_dir(project.path())
        .env_remove("DATABASE_URL")
        .env_remove("MADS_DATABASE__URL")
        .args(["db", "status", "--format", "json"])
        .output()
        .expect("database CLI should run");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
    let document = one_json_document(&output);
    assert_eq!(document["command"], "db status");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"], Value::Null);
    assert_eq!(document["diagnostics"][0]["severity"], "error");
    assert_eq!(document["diagnostics"][0]["code"], "MADS210");
    assert_eq!(
        document["diagnostics"][0]["message"],
        "the database command could not be completed"
    );
    assert!(!document.to_string().contains("database-json-secret"));
}

#[test]
fn every_database_operational_failure_is_one_safe_json_document() {
    let project = tempdir().expect("temporary project should be created");
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"database-json-matrix\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest should be written");
    fs::create_dir(project.path().join("src")).expect("source directory should be created");
    fs::write(
        project.path().join("src/lib.rs"),
        "diesel::table! { users (id) { id -> Int8, name -> Text, } }\n",
    )
    .expect("schema source should be written");
    fs::write(
        project.path().join("mads.toml"),
        "[database]\nurl = \"postgres://user:database-matrix-secret@127.0.0.1:1/mads\"\n",
    )
    .expect("database configuration should be written");
    fs::write(
        project.path().join(".env"),
        "MADS_PRIVATE_INSPECTION_TOKEN=private-inspection-token-sentinel\nMADS_PRIVATE_INSPECTION_PATH=private-inspection-path-sentinel\nMADS_SQL_SENTINEL=CREATE_TABLE_SQL_SENTINEL\nMADS_CONSTRAINT_SENTINEL=unique_constraint_sentinel\nMADS_SOURCE_SENTINEL=arbitrary-source-error-sentinel\n",
    )
    .expect("dotenv sentinels should be written");

    let cases: &[(&[&str], &str)] = &[
        (&["db", "generate", "--format", "json"], "db generate"),
        (&["db", "migrate", "--format", "json"], "db migrate"),
        (&["db", "rollback", "--format", "json"], "db rollback"),
        (&["db", "status", "--format", "json"], "db status"),
    ];
    let private_sentinels = [
        "database-matrix-secret",
        "postgres://user:database-matrix-secret@127.0.0.1:1/mads",
        "private-inspection-token-sentinel",
        "private-inspection-path-sentinel",
        "CREATE_TABLE_SQL_SENTINEL",
        "unique_constraint_sentinel",
        "arbitrary-source-error-sentinel",
    ];

    for (arguments, command) in cases {
        let output = Command::cargo_bin("mads")
            .expect("CLI binary should build")
            .current_dir(project.path())
            .env_remove("DATABASE_URL")
            .env_remove("MADS_DATABASE__URL")
            .args(*arguments)
            .output()
            .expect("database CLI should run");

        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
        let document = one_json_document(&output);
        assert_eq!(document["command"], *command, "{arguments:?}");
        assert_eq!(document["ok"], false, "{arguments:?}");
        assert_eq!(document["data"], Value::Null, "{arguments:?}");
        assert_eq!(document["diagnostics"][0]["severity"], "error");
        assert_eq!(document["diagnostics"][0]["code"], "MADS210");
        assert_no_sensitive_values(&output, &document, &private_sentinels);
    }
}

#[test]
fn every_finite_json_syntax_failure_has_one_canonical_document() {
    let cases: &[(&[&str], &str)] = &[
        (&["routes", "--format", "json", "--unknown"], "routes"),
        (&["graph", "--format", "json", "--unknown"], "graph"),
        (&["doctor", "--format", "json", "--unknown"], "doctor"),
        (
            &["db", "generate", "--format", "json", "--unknown"],
            "db generate",
        ),
        (
            &["db", "migrate", "--format", "json", "--unknown"],
            "db migrate",
        ),
        (
            &["db", "rollback", "--format", "json", "--unknown"],
            "db rollback",
        ),
        (
            &["db", "status", "--format", "json", "--unknown"],
            "db status",
        ),
    ];

    for (arguments, command) in cases {
        let output = Command::cargo_bin("mads")
            .expect("CLI binary should build")
            .args(*arguments)
            .output()
            .expect("CLI should run");

        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
        let document = one_json_document(&output);
        assert_eq!(document["schema_version"], 1, "{arguments:?}");
        assert_eq!(document["command"], *command, "{arguments:?}");
        assert_eq!(document["ok"], false, "{arguments:?}");
        assert_eq!(document["data"], Value::Null, "{arguments:?}");
        assert_eq!(
            document["diagnostics"],
            json!([{
                "severity": "error",
                "code": "MADS204",
                "title": "CLI syntax error",
                "message": "unknown argument: --unknown",
                "subject": null,
                "location": null,
                "suggestions": []
            }]),
            "{arguments:?}"
        );
    }
}

#[test]
fn inspection_routes_json_is_ordered_public_schema_data() {
    let (output, document) = inspection_json("standard", ["routes", "--format", "json"]);

    assert!(output.status.success(), "routes failed: {output:?}");
    assert_eq!(
        document,
        json!({
            "schema_version": 1,
            "command": "routes",
            "ok": true,
            "data": {
                "routes": [
                    {
                        "method": "GET",
                        "path": "/users/:id",
                        "route_trait": "UserRoutes",
                        "handler": "get_user",
                        "controller": "inspection_standard_fixture::UserController",
                        "location": {"file": "src/main.rs", "line": 14, "column": 1},
                        "guard_active": false
                    },
                    {
                        "method": "POST",
                        "path": "/users",
                        "route_trait": "UserRoutes",
                        "handler": "create_user",
                        "controller": "inspection_standard_fixture::UserController",
                        "location": {"file": "src/main.rs", "line": 14, "column": 1},
                        "guard_active": false
                    }
                ]
            },
            "diagnostics": []
        })
    );
}

#[test]
fn inspection_graph_json_keeps_only_ordered_public_graph_fields() {
    let (output, document) = inspection_json("standard", ["graph", "--format", "json"]);

    assert!(output.status.success(), "graph failed: {output:?}");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["command"], "graph");
    assert_eq!(document["ok"], true);
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(
        document["data"]["root_module"],
        "inspection_standard_fixture::AppModule"
    );
    assert_eq!(
        document["data"]["modules"],
        json!([{
            "type_name": "inspection_standard_fixture::AppModule",
            "namespace": "inspection_standard_fixture",
            "location": {"file": "src/main.rs", "line": 38, "column": 1}
        }])
    );
    assert_eq!(document["data"]["imports"], json!([]));
    assert_eq!(
        document["data"]["providers"]
            .as_array()
            .expect("providers should be an array")
            .iter()
            .map(|provider| provider["type_name"].as_str().expect("provider type"))
            .collect::<Vec<_>>(),
        [
            "inspection_standard_fixture::Marker",
            "inspection_standard_fixture::UserController",
            "mads_common::server_config::ServerBinding",
            "mads_core::config::Config",
        ]
    );
    assert_eq!(
        document["data"]["dependencies"],
        json!([{
            "provider": "inspection_standard_fixture::UserController",
            "dependency": "inspection_standard_fixture::Marker"
        }])
    );
    assert_eq!(
        document["data"]["construction_order"],
        json!([
            "inspection_standard_fixture::Marker",
            "inspection_standard_fixture::UserController"
        ])
    );
    assert!(
        document["data"].get("auto_configurations").is_none(),
        "private transport fields must not enter the CLI schema"
    );
    let serialized = document.to_string();
    for private_field in ["protocol_version", "token", "acknowledgement", "response"] {
        assert!(
            !serialized.contains(private_field),
            "private inspection field {private_field} entered the CLI schema"
        );
    }
}

#[test]
fn inspection_doctor_json_uses_ordered_lowercase_statuses() {
    let (output, document) = inspection_json("standard", ["doctor", "--format", "json"]);

    assert!(output.status.success(), "doctor failed: {output:?}");
    assert_eq!(document["command"], "doctor");
    assert_eq!(document["ok"], true);
    assert_eq!(
        document["data"]["checks"],
        json!([
            {"group": "configuration", "status": "pass", "summary": "conventional sources loaded"},
            {"group": "module graph", "status": "pass", "summary": "rooted module graph analyzed"},
            {"group": "providers", "status": "pass", "summary": "provider graph has a construction plan"},
            {"group": "routes", "status": "pass", "summary": "selected route metadata validated"},
            {"group": "guards/strategies", "status": "skipped", "summary": "JWT support is disabled"},
            {"group": "server/CORS", "status": "pass", "summary": "2 auto-configuration decision(s) evaluated"},
            {"group": "auto-configuration", "status": "pass", "summary": "2 auto-configuration decision(s) evaluated"}
        ])
    );
}

#[test]
fn failed_inspection_reports_keep_ordered_safe_partial_json_data() {
    let (graph_output, graph) = inspection_json(
        "standard",
        ["graph", "--bin", "invalid-graph", "--format", "json"],
    );
    assert_eq!(graph_output.status.code(), Some(1));
    assert_eq!(graph["command"], "graph");
    assert_eq!(graph["ok"], false);
    assert_eq!(graph["data"]["construction_order"], Value::Null);
    assert_eq!(
        graph["data"]["providers"]
            .as_array()
            .expect("partial providers should be an array")[0]["type_name"],
        "invalid_graph::NeedsMissing"
    );
    assert_eq!(graph["diagnostics"][0]["severity"], "error");
    assert_eq!(graph["diagnostics"][0]["code"], "MADS003");
    assert_eq!(
        graph["diagnostics"][0]["location"]["file"],
        "src/bin/invalid-graph.rs"
    );

    let (routes_output, routes) = inspection_json(
        "standard",
        ["routes", "--bin", "invalid-routes", "--format", "json"],
    );
    assert_eq!(routes_output.status.code(), Some(1));
    assert_eq!(routes["command"], "routes");
    assert_eq!(routes["ok"], false);
    assert_eq!(
        routes["data"]["routes"]
            .as_array()
            .expect("partial routes should be an array")
            .iter()
            .map(|route| route["route_trait"].as_str().expect("route trait"))
            .collect::<Vec<_>>(),
        ["FirstRoutes", "SecondRoutes"]
    );
    assert_eq!(
        routes["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array")
            .iter()
            .map(|diagnostic| diagnostic["code"].as_str().expect("diagnostic code"))
            .collect::<Vec<_>>(),
        ["MADS030", "MADS030"]
    );
}

#[test]
fn inspection_failure_before_a_report_uses_null_data() {
    let (output, document) = inspection_json("unsupported", ["doctor", "--format", "json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(document["command"], "doctor");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"], Value::Null);
    assert_eq!(document["diagnostics"][0]["severity"], "error");
    assert_eq!(document["diagnostics"][0]["code"], "MADS203");
    assert!(
        !document.to_string().contains("MADS_INSPECTION"),
        "private inspection transport details must not enter JSON"
    );
}

#[test]
fn every_inspection_pre_report_failure_is_one_redacted_json_document() {
    let private_sentinels = [
        "MADS_INSPECTION",
        "protocol_version",
        "private-inspection-token",
        "private-inspection-path",
        "arbitrary-source-error-sentinel",
    ];

    for arguments in [
        ["routes", "--format", "json"],
        ["graph", "--format", "json"],
        ["doctor", "--format", "json"],
    ] {
        let (output, document) = inspection_json("unsupported", arguments);

        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("error[MADS"),
            "MADS-owned human output leaked to stderr: {output:?}"
        );
        assert_eq!(document["command"], arguments[0], "{arguments:?}");
        assert_eq!(document["ok"], false, "{arguments:?}");
        assert_eq!(document["data"], Value::Null, "{arguments:?}");
        assert_eq!(document["diagnostics"][0]["severity"], "error");
        assert_eq!(document["diagnostics"][0]["code"], "MADS203");
        assert_no_sensitive_values(&output, &document, &private_sentinels);
    }
}

fn inspection_json<const N: usize>(fixture_name: &str, arguments: [&str; N]) -> (Output, Value) {
    let output = fixture_command(fixture_name)
        .args(arguments)
        .output()
        .expect("inspection CLI should run");
    let document = one_json_document(&output);
    (output, document)
}

fn one_json_document(output: &Output) -> Value {
    assert!(
        output.stdout.ends_with(b"\n"),
        "JSON stdout must end with one newline: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );

    let mut documents = Deserializer::from_slice(&output.stdout).into_iter::<Value>();
    let document = documents
        .next()
        .expect("JSON stdout should contain one document")
        .unwrap_or_else(|error| {
            panic!(
                "JSON stdout should contain a valid document: {error}; stdout={:?}; stderr={:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )
        });
    match documents.next() {
        None => document,
        Some(Ok(extra)) => panic!(
            "JSON stdout must contain exactly one document; extra document={extra}; stdout={:?}",
            String::from_utf8_lossy(&output.stdout)
        ),
        Some(Err(error)) => panic!(
            "JSON stdout must end after one document: {error}; stdout={:?}",
            String::from_utf8_lossy(&output.stdout)
        ),
    }
}

fn assert_no_sensitive_values(output: &Output, document: &Value, values: &[&str]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let serialized = document.to_string();
    for value in values {
        assert!(!stdout.contains(value), "stdout leaked {value}: {stdout}");
        assert!(!stderr.contains(value), "stderr leaked {value}: {stderr}");
        assert!(
            !serialized.contains(value),
            "JSON model leaked {value}: {serialized}"
        );
    }
}

fn fixture_command(name: &str) -> Command {
    let mut command = Command::cargo_bin("mads").expect("CLI binary should build");
    command.current_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/inspection")
            .join(name),
    );
    command
}
