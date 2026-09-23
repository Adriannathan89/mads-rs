//! Process-level tests for the MADS CLI command surface.

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

#[test]
fn version_reports_the_workspace_version() {
    let expected = format!("mads {}", env!("CARGO_PKG_VERSION"));

    Command::cargo_bin("mads")
        .expect("binary should build")
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(expected));
}

#[test]
fn help_is_printed_when_no_command_is_given() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .assert()
        .success()
        .stdout(contains("Usage: mads <command>"))
        .stdout(contains("run"))
        .stdout(contains("dev"))
        .stdout(contains("new       Create a minimal MADS application"))
        .stdout(contains("routes"))
        .stdout(contains("graph"))
        .stdout(contains("doctor"));
}

#[test]
fn foundation_is_removed_with_usage_exit_two() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .arg("foundation")
        .assert()
        .code(2)
        .stderr(contains("unknown command: foundation"));
}

#[test]
fn retired_db_is_an_unknown_top_level_command() {
    for args in [vec!["db"], vec!["db", "status"]] {
        Command::cargo_bin("mads")
            .unwrap()
            .args(args)
            .assert()
            .code(2)
            .stderr(contains("unknown command: db"));
    }
}

#[test]
fn run_needs_no_selector_for_one_application_and_forwards_arguments() {
    let fixture = fixture("single");

    fixture_command("single")
        .args(["run", "--", "--port", "4100", "two words"])
        .assert()
        .success()
        .stdout(contains(format!("cwd={}", fixture.display())))
        .stdout(contains("args=--port|4100|two words"));
}

#[test]
fn run_preserves_the_application_exit_code() {
    fixture_command("single")
        .args(["run", "--", "--exit=23"])
        .assert()
        .code(23);
}

#[test]
fn unknown_arguments_are_rejected_with_help() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .args(["unknown", "extra"])
        .assert()
        .code(2)
        .stderr(contains("error: unknown command: unknown"))
        .stderr(contains("Usage: mads <command>"));
}

#[test]
fn output_format_is_rejected_before_a_streaming_run_command() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .args(["run", "--format", "json"])
        .assert()
        .code(2)
        .stderr(contains("output format is not supported for this command"));
}

#[test]
fn output_format_is_rejected_for_streaming_help_and_version_commands() {
    for arguments in [
        ["--format", "json", "run"].as_slice(),
        ["run", "--format", "json"].as_slice(),
        ["--format", "json", "dev"].as_slice(),
        ["dev", "--format", "json"].as_slice(),
        ["--format", "json", "--help"].as_slice(),
        ["--version", "--format", "json"].as_slice(),
    ] {
        Command::cargo_bin("mads")
            .expect("CLI binary should build")
            .args(arguments)
            .assert()
            .code(2)
            .stdout(predicates::str::is_empty())
            .stderr(contains("output format is not supported for this command"));
    }
}

#[test]
fn run_forwards_a_format_looking_application_argument_after_the_separator() {
    fixture_command("single")
        .args(["run", "--", "--format", "json"])
        .assert()
        .success()
        .stdout(contains("args=--format|json"))
        .stdout(predicates::str::contains("schema_version").not());
}

fn fixture_command(name: &str) -> Command {
    let mut command = Command::cargo_bin("mads").expect("binary should build");
    command.current_dir(fixture(name));
    command
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/run")
        .join(name)
}
