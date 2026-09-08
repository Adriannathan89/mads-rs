//! Process-level tests for the MADS CLI command surface.

use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::tempdir;

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
fn output_format_is_rejected_for_streaming_help_version_and_database_help_commands() {
    for arguments in [
        ["--format", "json", "run"].as_slice(),
        ["run", "--format", "json"].as_slice(),
        ["--format", "json", "dev"].as_slice(),
        ["dev", "--format", "json"].as_slice(),
        ["--format", "json", "--help"].as_slice(),
        ["--version", "--format", "json"].as_slice(),
        ["db", "--help", "--format", "json"].as_slice(),
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

#[test]
fn help_lists_database_commands() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("db        Manage PostgreSQL migrations"));

    Command::cargo_bin("mads")
        .expect("binary should build")
        .args(["db", "--help"])
        .assert()
        .success()
        .stdout(contains(
            "generate  Generate one complete schema diff as <timestamp>_schema_diff",
        ))
        .stdout(contains("migrate"))
        .stdout(contains("rollback"))
        .stdout(contains("status"));
}

#[test]
fn unknown_database_subcommands_exit_two() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .args(["db", "destroy"])
        .assert()
        .code(2)
        .stderr(contains("unknown database command: destroy"))
        .stderr(contains("Usage: mads db <command>"));
}

#[test]
fn database_command_without_toml_exits_one_and_names_expected_path() {
    let directory = tempdir().expect("temporary project should be created");
    let expected_path = directory.path().join("mads.toml");

    database_command(directory.path())
        .assert()
        .code(1)
        .stderr(contains(expected_path.to_string_lossy().into_owned()));
}

#[test]
fn database_command_with_malformed_toml_exits_one() {
    let directory = tempdir().expect("temporary project should be created");
    write_toml(
        directory.path(),
        "[database\nurl = \"postgres://localhost/mads\"\n",
    );

    database_command(directory.path())
        .assert()
        .code(1)
        .stderr(contains("is not valid TOML"));
}

#[test]
fn database_command_accepts_missing_dotenv_and_advances_to_migrations_validation() {
    let directory = tempdir().expect("temporary project should be created");
    write_toml(
        directory.path(),
        "[database]\nurl = \"postgres://localhost/mads\"\n",
    );
    let migrations_path = directory.path().join("migrations");

    database_command(directory.path())
        .assert()
        .code(1)
        .stderr(contains(migrations_path.to_string_lossy().into_owned()));
}

#[test]
fn database_command_redacts_malformed_dotenv_contents() {
    let directory = tempdir().expect("temporary project should be created");
    let dotenv_path = directory.path().join(".env");
    write_toml(directory.path(), "[database]\nurl = \"${DATABASE_URL}\"\n");
    fs::write(&dotenv_path, "DATABASE_URL=postgres://user:cli-secret@localhost/mads\nnot dotenv syntax cli-sentinel-secret\n")
        .expect("dotenv should be written");

    database_command(directory.path())
        .assert()
        .code(1)
        .stderr(contains(dotenv_path.to_string_lossy().into_owned()))
        .stderr(
            predicates::str::is_empty()
                .not()
                .and(predicates::str::contains("cli-sentinel-secret").not()),
        );
}

#[test]
fn dotenv_database_url_resolves_toml_placeholder_before_migrations_validation() {
    let directory = tempdir().expect("temporary project should be created");
    write_toml(directory.path(), "[database]\nurl = \"${DATABASE_URL}\"\n");
    fs::write(
        directory.path().join(".env"),
        "DATABASE_URL=postgres://localhost/mads\n",
    )
    .expect("dotenv should be written");
    let migrations_path = directory.path().join("migrations");

    database_command(directory.path())
        .assert()
        .code(1)
        .stderr(contains(migrations_path.to_string_lossy().into_owned()));
}

#[test]
fn process_database_url_overrides_dotenv_before_migrations_validation() {
    let directory = tempdir().expect("temporary project should be created");
    write_toml(directory.path(), "[database]\nurl = \"${DATABASE_URL}\"\n");
    fs::write(directory.path().join(".env"), "DATABASE_URL=\n").expect("dotenv should be written");
    let migrations_path = directory.path().join("migrations");

    database_command(directory.path())
        .env(
            "DATABASE_URL",
            "postgres://process-user:process-secret@localhost/mads",
        )
        .assert()
        .code(1)
        .stderr(contains(migrations_path.to_string_lossy().into_owned()))
        .stderr(contains("process-secret").not());
}

#[test]
fn database_command_never_prints_configured_password() {
    let directory = tempdir().expect("temporary project should be created");
    write_toml(
        directory.path(),
        "[database]\nurl = \"postgres://user:cli-secret@127.0.0.1:1/mads\"\n",
    );
    fs::create_dir(directory.path().join("migrations"))
        .expect("migrations directory should be created");

    database_command(directory.path())
        .assert()
        .code(1)
        .stderr(contains("cli-secret").not());
}

fn database_command(root: &std::path::Path) -> Command {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"database-cli-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("Cargo manifest should be written");
    fs::create_dir_all(root.join("src")).expect("source directory should be created");
    fs::write(root.join("src/lib.rs"), "").expect("library target should be written");

    let mut command = Command::cargo_bin("mads").expect("binary should build");
    command
        .current_dir(root)
        .env_remove("DATABASE_URL")
        .env_remove("MADS_DATABASE__URL")
        .args(["db", "migrate"]);
    command
}

fn write_toml(root: &std::path::Path, contents: &str) {
    fs::write(root.join("mads.toml"), contents).expect("TOML should be written");
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
