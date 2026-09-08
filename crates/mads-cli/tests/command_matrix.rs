//! Black-box coverage for the complete v0.8 command surface.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output},
};

#[cfg(unix)]
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Deserializer, Value};
use tempfile::tempdir;

#[cfg(unix)]
use tempfile::TempDir;

struct CommandCase {
    arguments: &'static [&'static str],
    expected_code: i32,
    stdout_contains: &'static [&'static str],
    stderr_contains: &'static [&'static str],
}

const USAGE_CASES: &[CommandCase] = &[
    CommandCase {
        arguments: &["new"],
        expected_code: 2,
        stdout_contains: &[],
        stderr_contains: &["missing project name"],
    },
    CommandCase {
        arguments: &["foundation"],
        expected_code: 2,
        stdout_contains: &[],
        stderr_contains: &["unknown command"],
    },
    CommandCase {
        arguments: &["db", "generate", "named"],
        expected_code: 2,
        stdout_contains: &[],
        stderr_contains: &["unknown argument"],
    },
    CommandCase {
        arguments: &["db", "generate", "--diff-schema"],
        expected_code: 2,
        stdout_contains: &[],
        stderr_contains: &["unknown argument"],
    },
    CommandCase {
        arguments: &["routes", "--", "extra"],
        expected_code: 2,
        stdout_contains: &[],
        stderr_contains: &["does not accept application arguments"],
    },
];

#[test]
fn complete_command_matrix_has_stable_usage_and_exit_classes() {
    for case in USAGE_CASES {
        let output = cli_command(&single_fixture(), case.arguments)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(case.expected_code),
            "{:?}",
            case.arguments
        );
        assert_contains_all(&output, case.stdout_contains, case.stderr_contains);
    }

    let success_cases = [
        &["--help"][..],
        &["--version"][..],
        &["routes"][..],
        &["graph"][..],
        &["doctor"][..],
    ];
    for arguments in success_cases {
        let output = cli_command(&single_fixture(), arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{arguments:?}");
    }

    let new_invocation = tempdir().unwrap();
    let output = cli_command(new_invocation.path(), &["new", "matrix-app"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        new_invocation
            .path()
            .join("matrix-app/Cargo.toml")
            .is_file()
    );

    let output = cli_command(
        &workspace_fixture(),
        &["run", "-p", "api", "--bin", "server", "--", "--matrix-arg"],
    )
    .output()
    .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_contains_all(&output, &["matrix server args=--matrix-arg"], &[]);

    let output = cli_command(
        &workspace_fixture(),
        &["run", "-p", "api", "--", "--default-bin"],
    )
    .output()
    .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_contains_all(&output, &["matrix server args=--default-bin"], &[]);
}

#[test]
fn operational_database_failures_are_redacted_and_exit_one() {
    for (arguments, command) in [
        (["db", "generate"].as_slice(), "db generate"),
        (["db", "migrate"].as_slice(), "db migrate"),
        (["db", "rollback"].as_slice(), "db rollback"),
        (["db", "status"].as_slice(), "db status"),
    ] {
        let output = cli_command(&single_fixture(), arguments)
            .env_remove("DATABASE_URL")
            .env_remove("MADS_DATABASE__URL")
            .env(
                "MADS_DATABASE__URL",
                "postgres://matrix-env-secret@127.0.0.1:1/matrix",
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert_contains_all(&output, &[], &[]);
        assert_redacted(&output);

        let mut json_arguments = arguments.to_vec();
        json_arguments.extend(["--format", "json"]);
        let output = cli_command(&single_fixture(), &json_arguments)
            .env_remove("DATABASE_URL")
            .env_remove("MADS_DATABASE__URL")
            .env(
                "MADS_DATABASE__URL",
                "postgres://matrix-env-secret@127.0.0.1:1/matrix",
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{json_arguments:?}");
        assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
        let document = one_json_document(&output);
        assert_eq!(document["command"], command);
        assert_eq!(document["ok"], false);
        assert_eq!(document["data"], Value::Null);
        assert_eq!(document["diagnostics"][0]["severity"], "error");
        assert_redacted(&output);
    }
}

#[test]
fn finite_json_syntax_matrix_has_one_document_and_canonical_commands() {
    let cases: &[(&[&str], &str)] = &[
        (
            &["new", "matrix-app", "--format", "json", "--matrix-unknown"],
            "new",
        ),
        (
            &["routes", "--format", "json", "--matrix-unknown"],
            "routes",
        ),
        (&["graph", "--format", "json", "--matrix-unknown"], "graph"),
        (
            &["doctor", "--format", "json", "--matrix-unknown"],
            "doctor",
        ),
        (
            &["db", "generate", "--format", "json", "--matrix-unknown"],
            "db generate",
        ),
        (
            &["db", "migrate", "--format", "json", "--matrix-unknown"],
            "db migrate",
        ),
        (
            &["db", "rollback", "--format", "json", "--matrix-unknown"],
            "db rollback",
        ),
        (
            &["db", "status", "--format", "json", "--matrix-unknown"],
            "db status",
        ),
    ];

    for (arguments, command) in cases {
        let output = cli_command(&single_fixture(), arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stderr.is_empty(), "stderr was not empty: {output:?}");
        let document = one_json_document(&output);
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["command"], *command);
        assert_eq!(document["ok"], false);
        assert_eq!(document["data"], Value::Null);
        assert_eq!(document["diagnostics"][0]["code"], "MADS204");
    }
}

#[test]
fn release_workflows_enforce_linux_full_and_cli_platform_split() {
    let ci = fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
    let verify_job = workflow_job(&ci, "verify");
    for required in [
        "runs-on: ubuntu-latest",
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cargo test --locked --workspace --all-features",
        "cargo test --locked --workspace --all-features --doc",
        "cargo doc --locked --workspace --all-features --no-deps",
        "cargo package --locked --workspace --no-verify",
    ] {
        assert!(
            verify_job.contains(required),
            "missing Linux verification gate: {required}"
        );
    }

    let platform_job = workflow_job(&ci, "cli-platform");
    for required in [
        "ubuntu-latest",
        "macos-latest",
        "windows-latest",
        "if: runner.os == 'Linux'",
        "sudo apt-get update && sudo apt-get install --yes libpq-dev",
        "if: runner.os == 'macOS'",
        "brew install libpq",
        "LIBRARY_PATH=$(brew --prefix libpq)/lib",
        "PKG_CONFIG_PATH=$(brew --prefix libpq)/lib/pkgconfig",
        "if: runner.os == 'Windows'",
        "$pg = Get-ChildItem 'C:\\Program Files\\PostgreSQL' -Directory",
        "PQ_LIB_DIR=$($pg.FullName)\\lib",
        "$($pg.FullName)\\bin",
        "cargo test -p mads-cli --lib command::tests -- --test-threads=1",
        "cargo test -p mads-cli --test json_cli -- --test-threads=1",
        "scaffold::publish::tests::destination_race_preserves_the_competing_directory_and_cleans_staging",
        "model_serializes_nullable_diagnostics_and_normalized_locations",
        "cargo test -p mads-cli --test scaffold_cli -- --test-threads=1",
        "a_binary_without_standard_run_is_killed_and_diagnosed",
        "cargo test -p mads-cli --test dev_cli real_dev_loop -- --test-threads=1",
    ] {
        assert!(
            platform_job.contains(required),
            "missing platform gate: {required}"
        );
    }
    assert!(!platform_job.contains("services:"));
    assert!(!platform_job.contains("MADS_TEST_DATABASE_URL"));
    assert!(!platform_job.contains("--ignored"));
    for postgres_integration_test in [
        "database_postgres",
        "database_http_postgres",
        "database_migration_failure_prevents_listener_binding",
        "--test database_cli",
        "database_generate_postgres",
        "--test postgres_crud",
    ] {
        assert!(
            !platform_job.contains(postgres_integration_test),
            "platform job must not run PostgreSQL integration test {postgres_integration_test}",
        );
    }

    let postgres_job = workflow_job(&ci, "postgres");
    for required in [
        "runs-on: ubuntu-latest",
        "image: postgres:16",
        "MADS_TEST_DATABASE_URL",
        "--test database_postgres -- --ignored --test-threads=1",
        "--test database_http_postgres -- --ignored --test-threads=1",
        "database_migration_failure_prevents_listener_binding",
        "--test database_cli -- --ignored --test-threads=1",
        "--test database_generate_postgres -- --ignored --test-threads=1",
        "--test postgres_crud -- --ignored --test-threads=1",
    ] {
        assert!(
            postgres_job.contains(required),
            "missing PostgreSQL gate: {required}"
        );
    }

    for workflow_path in [
        ".github/workflows/beta-publish.yml",
        ".github/workflows/stable-publish.yml",
    ] {
        let workflow = fs::read_to_string(workspace_root().join(workflow_path)).unwrap();
        let release_platform_job = workflow_job(&workflow, "cli-platform");
        for required in ["ubuntu-latest", "macos-latest", "windows-latest"] {
            assert!(
                release_platform_job.contains(required),
                "{workflow_path}: {required}"
            );
        }
        assert!(
            !release_platform_job.contains("services:"),
            "{workflow_path}"
        );
        assert!(workflow_job(&workflow, "postgres").contains("image: postgres:16"));
    }
}

#[test]
fn cli_documentation_lists_the_exact_surface() {
    let documentation = fs::read_to_string(workspace_root().join("docs/CLI.md")).unwrap();
    for command in [
        "mads new <name>",
        "mads run",
        "mads dev",
        "mads routes",
        "mads graph",
        "mads doctor",
        "mads db generate",
        "mads db migrate",
        "mads db rollback",
        "mads db status",
    ] {
        assert!(documentation.contains(command), "missing {command}");
    }
    for documented_contract in [
        "mads --format json routes",
        "mads routes --format json",
        "mads --format json db status",
        "mads db status --format json",
        "schema_version\": 1",
        "`new`",
        "`routes`",
        "`graph`",
        "`doctor`",
        "`db generate`",
        "`db migrate`",
        "`db rollback`",
        "`db status`",
        "Cargo.toml",
        "mads.toml",
        "src/main.rs",
        "src/app/mod.rs",
        "src/app/routes.rs",
        "src/app/controller.rs",
        "src/app/service.rs",
        "MADS_SERVER__HOST",
        "MADS_SERVER__PORT",
        "| 0 |",
        "| 1 |",
        "| 2 |",
    ] {
        assert!(
            documentation.contains(documented_contract),
            "missing CLI documentation contract: {documented_contract}",
        );
    }
    assert!(!documentation.contains("mads db generate <name>"));
    assert!(!documentation.contains("mads foundation"));
    for unsupported_form in [
        "mads new <name> [--template",
        "mads new <name> [--database",
        "mads new <name> [--jwt",
        "mads new <name> [--vcs",
    ] {
        assert!(
            !documentation.contains(unsupported_form),
            "unapproved scaffold flag is presented as CLI syntax: {unsupported_form}",
        );
    }
}

#[cfg(unix)]
#[test]
fn dev_starts_an_application_and_can_be_terminated() {
    let fixture = copied_single_fixture();
    let address = available_localhost_address().unwrap();
    fs::write(
        fixture.path().join("mads.toml"),
        format!(
            "[server]\nhost = \"{}\"\nport = {}\n",
            address.ip(),
            address.port()
        ),
    )
    .unwrap();

    let output_path = fixture.path().join("dev-output.log");
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&output_path)
        .unwrap();
    let mut child = ChildGuard::new(
        cli_command(fixture.path(), &["dev"])
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output))
            .spawn()
            .unwrap(),
    );
    wait_for_output(&output_path, "mads dev: starting");
    wait_for_health(address);
    child.kill();
}

fn cli_command(root: &Path, arguments: &[&str]) -> ProcessCommand {
    let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_mads"));
    command.current_dir(root).args(arguments);
    command
}

fn assert_contains_all(output: &Output, stdout_contains: &[&str], stderr_contains: &[&str]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in stdout_contains {
        assert!(
            stdout.contains(expected),
            "stdout missing {expected}: {stdout}"
        );
    }
    for expected in stderr_contains {
        assert!(
            stderr.contains(expected),
            "stderr missing {expected}: {stderr}"
        );
    }
}

fn assert_redacted(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for secret in ["matrix-config-secret", "matrix-env-secret"] {
        assert!(!stdout.contains(secret), "stdout leaked {secret}: {stdout}");
        assert!(!stderr.contains(secret), "stderr leaked {secret}: {stderr}");
    }
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
                "JSON stdout should be valid: {error}; stdout={:?}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
    match documents.next() {
        None => document,
        Some(Ok(extra)) => panic!(
            "JSON stdout must contain exactly one document; extra={extra}; stdout={:?}",
            String::from_utf8_lossy(&output.stdout)
        ),
        Some(Err(error)) => panic!(
            "JSON stdout must end after one document: {error}; stdout={:?}",
            String::from_utf8_lossy(&output.stdout)
        ),
    }
}

fn single_fixture() -> PathBuf {
    workspace_root().join("crates/mads-cli/tests/fixtures/matrix/single")
}

fn workspace_fixture() -> PathBuf {
    workspace_root().join("crates/mads-cli/tests/fixtures/matrix/workspace")
}

#[cfg(unix)]
fn copied_single_fixture() -> TempDir {
    let destination = tempdir().unwrap();
    copy_directory(&single_fixture(), destination.path()).unwrap();
    let manifest_path = destination.path().join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let mads_path = workspace_root().join("crates/mads").canonicalize().unwrap();
    fs::write(
        manifest_path,
        manifest.replace("../../../../../mads", &mads_path.display().to_string()),
    )
    .unwrap();
    destination
}

#[cfg(unix)]
fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &destination_path)?;
        } else {
            fs::copy(entry.path(), destination_path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn available_localhost_address() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.local_addr()
}

#[cfg(unix)]
fn wait_for_output(path: &Path, expected: &str) {
    wait_until(|| {
        fs::read_to_string(path)
            .unwrap_or_default()
            .contains(expected)
    });
}

#[cfg(unix)]
fn wait_for_health(address: SocketAddr) {
    wait_until(|| {
        let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100))
        else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
        let _ = stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let mut response = String::new();
        stream.read_to_string(&mut response).is_ok() && response.contains("healthy")
    });
}

#[cfg(unix)]
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for CLI fixture"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
struct ChildGuard(Option<Child>);

#[cfg(unix)]
impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn workflow_job<'workflow>(workflow: &'workflow str, job: &str) -> &'workflow str {
    let header = format!("  {job}:\n");
    let (_, remainder) = workflow
        .split_once(&header)
        .unwrap_or_else(|| panic!("workflow should define {job} job"));

    let end = remainder
        .match_indices("\n  ")
        .find_map(|(offset, _)| {
            let line = &remainder[offset + 1..].lines().next()?;
            (!line.as_bytes().get(2).is_some_and(u8::is_ascii_whitespace)).then_some(offset)
        })
        .unwrap_or(remainder.len());
    &remainder[..end]
}
