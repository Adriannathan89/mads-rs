//! JSON contract coverage for finite MADS CLI output.

use std::{path::Path, process::Output};

use assert_cmd::Command;
use mads_cli::output::{
    json::render,
    model::{CliDiagnostic, CommandData, Envelope, RoutesData, SourceLocation},
    path::normalize_path,
};
use serde_json::{Value, json};

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

fn inspection_json<const N: usize>(fixture_name: &str, arguments: [&str; N]) -> (Output, Value) {
    let output = fixture_command(fixture_name)
        .args(arguments)
        .output()
        .expect("inspection CLI should run");
    let document = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "inspection JSON stdout should be exactly one document: {error}; stdout={:?}; stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    });
    (output, document)
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
