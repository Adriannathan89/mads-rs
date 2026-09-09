//! Process-level coverage for inspection against real MADS applications.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn one_standard_app_is_inspected_without_selectors_or_startup_effects() {
    let marker_directory = tempdir().expect("marker directory should exist");
    let marker = marker_directory.path().join("constructed");

    fixture_command("standard")
        .env("MADS_TEST_CONSTRUCTION_MARKER", &marker)
        .arg("routes")
        .assert()
        .success()
        .stdout(contains("GET"))
        .stdout(contains("/users/:id"))
        .stdout(contains("UserController"));

    assert!(!marker.exists(), "inspection constructed a provider");
}

#[test]
fn graph_and_doctor_return_deterministic_framework_evidence() {
    let graph_markers = tempdir().expect("graph marker directory should exist");
    let graph_construction = graph_markers.path().join("constructed");
    fixture_command("standard")
        .env("MADS_TEST_CONSTRUCTION_MARKER", &graph_construction)
        .arg("graph")
        .assert()
        .success()
        .stdout(contains("AppModule"))
        .stdout(contains("Construction order"))
        .stdout(contains("MADS application ready").not());
    assert!(
        !graph_construction.exists(),
        "graph inspection constructed a provider"
    );

    let doctor_markers = tempdir().expect("doctor marker directory should exist");
    let doctor_construction = doctor_markers.path().join("constructed");
    fixture_command("standard")
        .env("MADS_TEST_CONSTRUCTION_MARKER", &doctor_construction)
        .arg("doctor")
        .assert()
        .success()
        .stdout(contains("configuration"))
        .stdout(contains("server/CORS"))
        .stdout(contains("MADS application ready").not());
    assert!(
        !doctor_construction.exists(),
        "doctor inspection constructed a provider"
    );
}

#[test]
fn database_enabled_app_is_inspected_offline_before_startup_or_bind() {
    let markers = tempdir().expect("database marker directory should exist");
    let construction = markers.path().join("constructed");

    fixture_command("database")
        .env("MADS_TEST_CONSTRUCTION_MARKER", &construction)
        .arg("graph")
        .assert()
        .success()
        .stdout(contains("mads_common::database::pool::Database"))
        .stdout(contains("state=auto_configured"))
        .stdout(contains("MADS application ready").not());

    assert!(
        !construction.exists(),
        "database inspection constructed the application before startup"
    );
}

#[test]
fn a_binary_without_standard_run_is_killed_and_diagnosed() {
    let markers = tempdir().expect("unsupported marker directory should exist");
    let heartbeat = markers.path().join("heartbeat");

    fixture_command("unsupported")
        .env("MADS_TEST_HEARTBEAT_MARKER", &heartbeat)
        .arg("doctor")
        .assert()
        .code(1)
        .stderr(contains("MADS203"))
        .stderr(contains("Mads::run::<AppModule>()"));

    let heartbeat_after_exit = std::fs::read(&heartbeat)
        .expect("non-cooperating fixture should publish a heartbeat before termination");
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(
        std::fs::read(&heartbeat).expect("heartbeat marker should remain readable"),
        heartbeat_after_exit,
        "unsupported fixture remained alive after the CLI returned"
    );
}

#[test]
fn invalid_graph_preserves_partial_provider_evidence() {
    fixture_command("standard")
        .args(["graph", "--bin", "invalid-graph"])
        .assert()
        .code(1)
        .stdout(contains("NeedsMissing"))
        .stderr(contains("MADS003"));
}

#[test]
fn invalid_routes_preserve_partial_route_evidence() {
    fixture_command("standard")
        .args(["routes", "--bin", "invalid-routes"])
        .assert()
        .code(1)
        .stdout(contains("GET"))
        .stdout(contains("/duplicate"))
        .stderr(contains("MADS030"));
}

#[test]
fn json_inspection_keeps_the_human_table_out_of_machine_stdout() {
    let output = fixture_command("standard")
        .args(["routes", "--format", "json"])
        .output()
        .expect("inspection CLI should run");

    assert!(output.status.success(), "routes failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("JSON stdout should be UTF-8");
    let document: Value =
        serde_json::from_str(&stdout).expect("stdout should be one JSON document");
    assert_eq!(document["command"], "routes");
    assert_eq!(document["ok"], true);
    assert!(!stdout.contains("METHOD  PATH"));
    assert!(!stdout.contains("error[MADS"));
}

fn fixture_command(name: &str) -> Command {
    let mut command = Command::cargo_bin("mads").expect("CLI binary should build");
    command.current_dir(fixture(name));
    command
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/inspection")
        .join(name)
}
