//! Real PostgreSQL process evidence for the MADS database CLI.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

static TEST_LOCK: Mutex<()> = Mutex::new(());

const OVERRIDE_URL: &str = "postgres://user:cli-secret@127.0.0.1:1/mads";

#[test]
fn database_command_rejects_extra_arguments_with_exit_two() {
    Command::cargo_bin("mads")
        .expect("binary should build")
        .args(["db", "migrate", "extra"])
        .assert()
        .code(2)
        .stderr(contains("unknown argument: extra"))
        .stderr(contains("Usage: mads db <command>"));
}

#[test]
fn database_option_errors_print_database_help() {
    for arguments in [
        &["db", "status", "--bin", "server"][..],
        &["db", "status", "--package", "api", "-p", "web"][..],
        &["db", "status", "--package"][..],
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
fn database_command_uses_the_selected_package_root() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/run/multiple");
    let expected = workspace.join("api/mads.toml");

    project_command(&workspace, ["db", "status", "--package", "api"])
        .assert()
        .code(1)
        .stderr(contains(expected.display().to_string()));
}

#[test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
fn database_migration_commands_are_real_and_redact_overrides() {
    let _lock = TEST_LOCK
        .lock()
        .expect("CLI PostgreSQL test lock should not poison");
    let _database_url = std::env::var("MADS_TEST_DATABASE_URL")
        .expect("MADS_TEST_DATABASE_URL is required for ignored PostgreSQL tests");
    let project = temporary_project();
    let mut cleanup = MigrationCleanup::new(project.path());
    cleanup.arm();

    project_command(project.path(), ["db", "migrate"])
        .assert()
        .success()
        .stdout(contains("applied 202608220201"));

    project_command(project.path(), ["db", "migrate"])
        .assert()
        .success()
        .stdout(contains("database is up to date"));

    project_command(project.path(), ["db", "status"])
        .assert()
        .success()
        .stdout(contains("applied 202608220201"))
        .stdout(contains("summary: 1 applied, 0 pending"));

    project_command(project.path(), ["db", "rollback"])
        .assert()
        .success()
        .stdout(contains("reverted 202608220201"));
    cleanup.disarm();

    project_command(project.path(), ["db", "status"])
        .assert()
        .success()
        .stdout(contains("pending 202608220201"))
        .stdout(contains("summary: 0 applied, 1 pending"));

    let migrate = database_json(project.path(), ["db", "migrate", "--format", "json"]);
    assert_eq!(migrate["data"], json!({"applied": ["202608220201"]}));
    assert_eq!(migrate["diagnostics"], json!([]));

    let status = database_json(project.path(), ["db", "status", "--format", "json"]);
    assert_eq!(
        status["data"],
        json!({"applied": ["202608220201"], "pending": []})
    );

    let rollback = database_json(project.path(), ["db", "rollback", "--format", "json"]);
    assert_eq!(rollback["data"], json!({"reverted": ["202608220201"]}));
    cleanup.disarm();

    let status = database_json(project.path(), ["db", "status", "--format", "json"]);
    assert_eq!(
        status["data"],
        json!({"applied": [], "pending": ["202608220201"]})
    );

    project_command(project.path(), ["db", "rollback"])
        .assert()
        .code(1)
        .stderr(contains("no migration is available to revert"));

    project_command(project.path(), ["db", "migrate"])
        .env("MADS_DATABASE__URL", OVERRIDE_URL)
        .assert()
        .code(1)
        .stderr(contains("cli-secret").not())
        .stderr(contains(OVERRIDE_URL).not());
}

fn database_json<I, S>(project: &Path, arguments: I) -> Value
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = project_command(project, arguments)
        .output()
        .expect("database JSON command should run");
    assert!(
        output.status.success(),
        "database JSON command failed: {output:?}"
    );
    assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "database JSON stdout should be exactly one document: {error}; stdout={:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn temporary_project() -> TempDir {
    let project = tempdir().expect("temporary project should be created");
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"database-cli-postgres-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("Cargo manifest should be written");
    fs::create_dir(project.path().join("src")).expect("source directory should be created");
    fs::write(project.path().join("src/lib.rs"), "").expect("library target should be written");
    fs::write(
        project.path().join("mads.toml"),
        "[database]\nurl = \"${MADS_TEST_DATABASE_URL}\"\npool_size = 2\nmigrate = false\n",
    )
    .expect("project TOML should be written");
    copy_directory(&fixture_directory(), &project.path().join("migrations"))
        .expect("migration fixture should be copied");
    project
}

fn fixture_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/migrations")
}

fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn project_command<I, S>(project: &Path, arguments: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::cargo_bin("mads").expect("binary should build");
    command
        .current_dir(project)
        .env_remove("DATABASE_URL")
        .env_remove("MADS_DATABASE__URL")
        .args(arguments);
    command
}

struct MigrationCleanup<'a> {
    project: &'a Path,
    armed: bool,
}

impl<'a> MigrationCleanup<'a> {
    fn new(project: &'a Path) -> Self {
        Self {
            project,
            armed: false,
        }
    }

    fn arm(&mut self) {
        self.armed = true;
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for MigrationCleanup<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = project_command(self.project, ["db", "rollback"]).output();
        }
    }
}
