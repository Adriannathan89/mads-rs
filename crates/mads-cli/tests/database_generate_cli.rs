//! Process-level coverage for the no-name database generation command.

use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::{
    prelude::PredicateBooleanExt,
    str::{contains, is_match},
};
use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

#[test]
fn generate_rejects_every_argument_outside_the_package_selector_with_exit_two() {
    for arguments in [
        ["db", "generate", "users"].as_slice(),
        ["db", "generate", "--diff-schema"].as_slice(),
        ["db", "generate", "--bin", "server"].as_slice(),
        ["db", "generate", "--", "extra"].as_slice(),
        ["db", "generate", "-p", "api", "--package", "web"].as_slice(),
    ] {
        Command::cargo_bin("mads")
            .expect("binary should build")
            .args(arguments)
            .assert()
            .code(2)
            .stderr(contains("Usage: mads db <command>"));
    }
}

#[test]
fn generate_help_names_its_timestamped_directory_shape() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .args(["db", "--help"])
        .assert()
        .success()
        .stdout(contains(
            "generate  Generate one complete schema diff as <timestamp>_schema_diff",
        ));
}

#[test]
fn generate_reports_missing_required_toml_after_loading_schema() {
    let project = project_with_schema();
    let expected = project.path().join("mads.toml");

    generate_command(project.path())
        .assert()
        .code(1)
        .stderr(contains(expected.display().to_string()))
        .stderr(contains("migrations").not());
}

#[test]
fn generate_reports_malformed_required_toml_without_creating_migrations() {
    let project = project_with_schema();
    fs::write(
        project.path().join("mads.toml"),
        "[database\nurl = \"postgres://localhost/mads\"\n",
    )
    .expect("malformed project TOML should be written");

    generate_command(project.path())
        .assert()
        .code(1)
        .stderr(contains("is not valid TOML"));
    assert!(!project.path().join("migrations").exists());
}

#[test]
fn generate_reports_missing_schema_before_touching_migrations() {
    let project = project_without_schema();
    write_toml(project.path(), "postgres://localhost/mads");

    generate_command(project.path())
        .assert()
        .code(1)
        .stderr(contains("MADS210"))
        .stderr(contains("schema.rs"));
    assert!(!project.path().join("migrations").exists());
}

#[test]
fn generate_loads_schema_before_required_configuration() {
    let project = project_without_schema();
    let missing_toml = project.path().join("mads.toml");

    generate_command(project.path())
        .assert()
        .code(1)
        .stderr(contains("MADS210"))
        .stderr(contains(missing_toml.display().to_string()).not());
    assert!(!project.path().join("migrations").exists());
}

#[test]
fn generate_redacts_an_unreachable_database_url_and_password() {
    let project = project_with_schema();
    write_toml(
        project.path(),
        "postgres://user:generate-secret@127.0.0.1:1/mads",
    );

    generate_command(project.path())
        .assert()
        .code(1)
        .stderr(contains("generate-secret").not())
        .stderr(contains("postgres://user").not());
    assert!(!project.path().join("migrations").exists());
}

#[test]
fn generate_discovers_a_project_with_an_empty_path_and_absolute_cargo() {
    let project = project_without_schema();

    generate_command(project.path())
        .env("CARGO", absolute_cargo())
        .env("RUSTC", absolute_rustc())
        .env("PATH", "")
        .assert()
        .code(1)
        .stderr(contains("MADS210"))
        .stderr(contains("MADS201").not());
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL; Task 7 runs database round-trip coverage"]
fn generate_no_diff_reports_up_to_date_without_creating_migrations() {
    let project = project_with_schema_source("// an intentionally empty desired schema\n");
    write_test_database_toml(project.path());

    generate_command(project.path())
        .env("MADS_TEST_DATABASE_URL", test_database_url())
        .assert()
        .success()
        .stdout(contains("schema is up to date"));
    assert!(!project.path().join("migrations").exists());
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL; Task 7 runs database round-trip coverage"]
fn generate_json_reports_up_to_date_with_null_path_and_no_review_warning() {
    let project = project_with_schema_source("// an intentionally empty desired schema\n");
    write_test_database_toml(project.path());

    let document = generate_json(project.path());

    assert_eq!(
        document["data"],
        json!({
            "status": "up_to_date",
            "migration_path": null,
            "review_required": false
        })
    );
    assert_eq!(document["diagnostics"], json!([]));
    assert!(!project.path().join("migrations").exists());
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL; Task 7 runs database round-trip coverage"]
fn generate_with_path_removed_never_needs_an_external_diesel_executable() {
    let project = project_with_schema();
    write_test_database_toml(project.path());

    generate_command(project.path())
        .env("MADS_TEST_DATABASE_URL", test_database_url())
        .env("CARGO", absolute_cargo())
        .env("RUSTC", absolute_rustc())
        .env("PATH", "")
        .assert()
        .success()
        .stdout(is_match(r"(?m)^generated migrations/[0-9]{20}_schema_diff$").unwrap());
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL; Task 7 runs database round-trip coverage"]
fn generate_success_only_names_the_migration_and_review_requirement() {
    let project = project_with_schema();
    write_test_database_toml(project.path());

    generate_command(project.path())
        .env("MADS_TEST_DATABASE_URL", test_database_url())
        .assert()
        .success()
        .stdout(is_match(r"(?m)^generated migrations/[0-9]{20}_schema_diff$").unwrap())
        .stdout(contains("review up.sql and down.sql before applying"))
        .stdout(contains("postgres://").not());
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL; Task 7 runs database round-trip coverage"]
fn generate_json_reports_a_relative_migration_path_and_review_requirement() {
    let project = project_with_schema();
    write_test_database_toml(project.path());

    let document = generate_json(project.path());

    assert_eq!(document["command"], "db generate");
    assert_eq!(document["ok"], true);
    assert_eq!(document["data"]["status"], "generated");
    assert!(
        document["data"]["migration_path"]
            .as_str()
            .is_some_and(|path| path.starts_with("migrations/") && path.ends_with("_schema_diff")),
        "migration path was not package-relative: {document}"
    );
    assert_eq!(document["data"]["review_required"], true);
    assert!(
        document["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array")
            .iter()
            .all(|diagnostic| diagnostic["code"] != "MADS212"),
        "a create-table migration should not repeat review warnings in data"
    );
}

fn project_with_schema() -> TempDir {
    project_with_schema_source("diesel::table! { users (id) { id -> Int8, name -> Text, } }\n")
}

fn project_with_schema_source(schema: &str) -> TempDir {
    let project = project_without_schema();
    fs::write(project.path().join("src/schema.rs"), schema)
        .expect("schema source should be written");
    project
}

fn project_without_schema() -> TempDir {
    let project = tempdir().expect("temporary project should be created");
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"database-generate-cli-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest should be written");
    fs::create_dir(project.path().join("src")).expect("source directory should be created");
    fs::write(project.path().join("src/lib.rs"), "").expect("library target should be written");
    project
}

fn generate_command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("mads").expect("binary should build");
    command
        .current_dir(root)
        .env_remove("DATABASE_URL")
        .env_remove("MADS_DATABASE__URL")
        .args(["db", "generate"]);
    command
}

fn generate_json(root: &Path) -> Value {
    let output = generate_command(root)
        .args(["--format", "json"])
        .env("MADS_TEST_DATABASE_URL", test_database_url())
        .output()
        .expect("database generation CLI should run");
    assert!(
        output.status.success(),
        "database generation failed: {output:?}"
    );
    assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "database generation JSON stdout should be exactly one document: {error}; stdout={:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn write_toml(root: &Path, url: &str) {
    fs::write(
        root.join("mads.toml"),
        format!("[database]\nurl = \"{url}\"\n"),
    )
    .expect("project TOML should be written");
}

fn write_test_database_toml(root: &Path) {
    write_toml(root, "${MADS_TEST_DATABASE_URL}");
}

fn test_database_url() -> String {
    std::env::var("MADS_TEST_DATABASE_URL")
        .expect("MADS_TEST_DATABASE_URL is required for ignored PostgreSQL tests")
}

fn absolute_cargo() -> &'static Path {
    let cargo = Path::new(env!("CARGO"));
    assert!(
        cargo.is_absolute(),
        "Cargo must provide an absolute executable path"
    );
    cargo
}

fn absolute_rustc() -> std::path::PathBuf {
    let rustc = absolute_cargo()
        .parent()
        .expect("Cargo executable should have a parent directory")
        .join("rustc");
    assert!(
        rustc.is_file(),
        "Cargo toolchain should provide rustc next to cargo"
    );
    rustc
}
