//! JSON contract coverage for finite MADS CLI output.

use std::path::Path;

use assert_cmd::Command;
use mads_cli::output::{
    json::render,
    model::{CliDiagnostic, CommandData, Envelope, RoutesData, SourceLocation},
    path::normalize_path,
};

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
