//! Release-level CLI acceptance coverage for the complete v0.8 command surface.

use std::{
    ffi::OsString,
    fs,
    path::Path,
    process::{Command, Output},
};

use serde_json::{Deserializer, Value, json};
use tempfile::tempdir;

const SECRET_SENTINEL: &str = "v080-cli-secret-sentinel";
const GENERATED_FILES: [&str; 7] = [
    "Cargo.toml",
    "mads.toml",
    "src/main.rs",
    "src/app/mod.rs",
    "src/app/routes.rs",
    "src/app/controller.rs",
    "src/app/service.rs",
];

#[test]
fn v080_cli_generates_a_registry_ready_project_and_inspects_it_in_both_formats() {
    let invocation = tempdir().expect("temporary invocation directory should exist");
    assert!(
        !invocation.path().join("Cargo.toml").exists(),
        "generation must run outside a Cargo project"
    );

    let generation = mads_command(invocation.path(), ["new", "release-app"])
        .output()
        .expect("mads new should run");
    assert_exit(&generation, 0, "mads new");
    assert!(
        generation.stderr.is_empty(),
        "mads new stderr: {generation:?}"
    );
    assert_eq!(
        String::from_utf8(generation.stdout).expect("human generation output should be UTF-8"),
        "Created project: release-app\n\ncd release-app\nmads dev\n"
    );

    let json_generation = mads_command(
        invocation.path(),
        ["new", "machine-app", "--format", "json"],
    )
    .output()
    .expect("JSON mads new should run");
    assert_exit(&json_generation, 0, "JSON mads new");
    assert!(
        json_generation.stderr.is_empty(),
        "JSON mads new stderr: {json_generation:?}"
    );
    assert_eq!(
        one_json_document(&json_generation),
        json!({
            "schema_version": 1,
            "command": "new",
            "ok": true,
            "data": {
                "project_name": "machine-app",
                "path": "machine-app",
                "files": GENERATED_FILES,
            },
            "diagnostics": [],
        })
    );

    let project = invocation.path().join("release-app");
    let mut expected_files = GENERATED_FILES.map(str::to_owned).to_vec();
    expected_files.sort();
    assert_eq!(listed_files(&project), expected_files);
    let manifest_path = project.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("generated manifest should exist");
    assert_eq!(manifest, registry_manifest("release-app"));
    for absent in [".git", "Cargo.lock", "target"] {
        assert!(
            !project.join(absent).exists(),
            "generation must not create {absent} before Cargo runs"
        );
    }

    substitute_local_mads(&manifest_path, &manifest);
    let check = cargo_command(&project)
        .args(["check", "--offline"])
        .output()
        .expect("generated application should compile offline after local substitution");
    assert_success("cargo check", &check);

    for (command, human_fragment, json_arguments) in [
        ("routes", "GET", vec!["--format", "json", "routes"]),
        ("graph", "AppModule", vec!["graph", "--format", "json"]),
        (
            "doctor",
            "configuration",
            vec!["doctor", "--format", "json"],
        ),
    ] {
        let human = mads_command(&project, [command])
            .env("CARGO_NET_OFFLINE", "true")
            .output()
            .expect("human inspection should run");
        assert_exit(&human, 0, command);
        let human_stdout = String::from_utf8_lossy(&human.stdout);
        assert!(
            human_stdout.contains(human_fragment),
            "{command} human output missing {human_fragment:?}: {human_stdout}"
        );
        assert_no_secret(&human);

        let json_output = mads_command(&project, json_arguments)
            .env("CARGO_NET_OFFLINE", "true")
            .output()
            .expect("JSON inspection should run");
        assert_exit(&json_output, 0, command);
        let document = one_json_document(&json_output);
        assert_eq!(document["schema_version"], 1, "{command}");
        assert_eq!(document["command"], command, "{command}");
        assert_eq!(document["ok"], true, "{command}");
        assert_eq!(document["diagnostics"], json!([]), "{command}");
        assert!(document["data"].is_object(), "{command}: {document}");
        assert_no_secret(&json_output);
    }

    let duplicate = mads_command(
        invocation.path(),
        ["new", "release-app", "--format", "json"],
    )
    .output()
    .expect("duplicate destination should produce a JSON operational failure");
    assert_exit(&duplicate, 1, "existing scaffold destination");
    assert!(duplicate.stderr.is_empty(), "JSON stderr: {duplicate:?}");
    let document = one_json_document(&duplicate);
    assert_eq!(document["command"], "new");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"], Value::Null);
    assert_eq!(document["diagnostics"][0]["code"], "MADS230");
}

#[test]
fn v080_cli_json_failures_preserve_exit_classes_and_redact_every_channel() {
    let syntax_cases: &[(&[&str], Option<&str>)] = &[
        (
            &["new", "release-app", "--format", "json", "--unknown"],
            Some("new"),
        ),
        (&["routes", "--format", "json", "--unknown"], Some("routes")),
        (&["graph", "--format", "json", "--unknown"], Some("graph")),
        (&["doctor", "--format", "json", "--unknown"], Some("doctor")),
        (
            &["db", "generate", "--format", "json", "--unknown"],
            Some("db generate"),
        ),
        (
            &["db", "migrate", "--format", "json", "--unknown"],
            Some("db migrate"),
        ),
        (
            &["db", "rollback", "--format", "json", "--unknown"],
            Some("db rollback"),
        ),
        (
            &["db", "status", "--format", "json", "--unknown"],
            Some("db status"),
        ),
        (&["--format", "json", "unknown-command"], None),
    ];
    for (arguments, command) in syntax_cases {
        let output = mads_command(workspace_root(), *arguments)
            .output()
            .expect("JSON syntax command should run");
        assert_exit(&output, 2, "JSON syntax command");
        assert!(output.stderr.is_empty(), "JSON syntax stderr: {output:?}");
        let document = one_json_document(&output);
        assert_eq!(document["schema_version"], 1, "{arguments:?}");
        match command {
            Some(command) => assert_eq!(document["command"], *command, "{arguments:?}"),
            None => assert!(document["command"].is_null(), "{arguments:?}: {document}"),
        }
        assert_eq!(document["ok"], false, "{arguments:?}");
        assert_eq!(document["data"], Value::Null, "{arguments:?}");
        assert_eq!(
            document["diagnostics"][0]["code"], "MADS204",
            "{arguments:?}"
        );
        assert_no_secret(&output);
    }

    let invalid_name = mads_command(workspace_root(), ["new", "Release-App", "--format", "json"])
        .output()
        .expect("invalid project name should run");
    assert_exit(&invalid_name, 2, "invalid project name");
    assert!(
        invalid_name.stderr.is_empty(),
        "JSON stderr: {invalid_name:?}"
    );
    let document = one_json_document(&invalid_name);
    assert_eq!(document["command"], "new");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"], Value::Null);
    assert_eq!(document["diagnostics"][0]["code"], "MADS230");

    for command in ["run", "dev"] {
        let output = mads_command(workspace_root(), [command, "--format", "json"])
            .output()
            .expect("streaming format rejection should run");
        assert_exit(&output, 2, "streaming format rejection");
        assert!(
            output.stdout.is_empty(),
            "streaming rejection stdout: {output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("output format is not supported for this command"),
            "streaming rejection should remain human: {output:?}"
        );
        assert_no_secret(&output);
    }

    let project = temporary_database_project();
    for (arguments, command) in [
        (["db", "generate", "--format", "json"], "db generate"),
        (["db", "migrate", "--format", "json"], "db migrate"),
        (["db", "rollback", "--format", "json"], "db rollback"),
        (["db", "status", "--format", "json"], "db status"),
    ] {
        let output = mads_command(project.path(), arguments)
            .env_remove("DATABASE_URL")
            .env_remove("MADS_DATABASE__URL")
            .output()
            .expect("database JSON operational failure should run");
        assert_exit(&output, 1, command);
        assert!(output.stderr.is_empty(), "database JSON stderr: {output:?}");
        let document = one_json_document(&output);
        assert_eq!(document["command"], command);
        assert_eq!(document["ok"], false);
        assert_eq!(document["data"], Value::Null);
        assert_eq!(document["diagnostics"][0]["severity"], "error");
        assert_eq!(document["diagnostics"][0]["code"], "MADS210");
        assert_no_secret(&output);
    }
}

fn mads_command<I, S>(directory: &Path, arguments: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_mads"));
    command.current_dir(directory).args(arguments);
    command
}

fn cargo_command(project: &Path) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(project)
        .env("CARGO_NET_OFFLINE", "true");
    command
}

fn registry_manifest(name: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\n\n[dependencies]\nmads = {{ version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn substitute_local_mads(manifest_path: &Path, registry_manifest: &str) {
    let local_mads = workspace_root()
        .join("crates/mads")
        .canonicalize()
        .expect("local mads crate should exist");
    let local_path = local_mads.to_string_lossy().replace('\\', "\\\\");
    let registry_dependency = format!(
        "mads = {{ version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}",
        env!("CARGO_PKG_VERSION")
    );
    let local_dependency = format!(
        "mads = {{ path = \"{local_path}\", version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}",
        env!("CARGO_PKG_VERSION")
    );
    let substituted = registry_manifest.replacen(&registry_dependency, &local_dependency, 1);
    assert_ne!(
        substituted, registry_manifest,
        "the registry dependency should be substituted exactly once"
    );
    assert!(
        !substituted.contains(&registry_dependency),
        "the registry dependency must not remain after local substitution"
    );
    fs::write(manifest_path, substituted).expect("local dependency substitution should write");
}

fn temporary_database_project() -> tempfile::TempDir {
    let project = tempdir().expect("temporary database project should exist");
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"v080-database-failure\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("database fixture manifest should write");
    fs::create_dir(project.path().join("src")).expect("database fixture source directory");
    fs::write(project.path().join("src/lib.rs"), "").expect("database fixture source should write");
    fs::write(
        project.path().join("mads.toml"),
        format!("[database]\nurl = \"postgres://user:{SECRET_SENTINEL}@127.0.0.1:1/v080\"\n"),
    )
    .expect("database fixture configuration should write");
    project
}

fn listed_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, current: &Path, files: &mut Vec<String>) {
        for entry in fs::read_dir(current)
            .expect("generated directory should be readable")
            .map(|entry| entry.expect("generated directory entry should be readable"))
        {
            let path = entry.path();
            if entry
                .file_type()
                .expect("generated file metadata should be readable")
                .is_dir()
            {
                walk(root, &path, files);
            } else {
                files.push(
                    path.strip_prefix(root)
                        .expect("generated path should be below project root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
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
        .expect("JSON stdout should contain a document")
        .unwrap_or_else(|error| panic!("JSON stdout should parse: {error}"));
    assert!(
        documents.next().is_none(),
        "JSON stdout must contain exactly one document: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    document
}

fn assert_exit(output: &Output, expected: i32, context: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{context} had unexpected output\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_success(context: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{context} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_no_secret(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for channel in [&stdout, &stderr] {
        assert!(
            !channel.contains(SECRET_SENTINEL),
            "CLI channel leaked the configured secret: {channel}"
        );
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("mads-cli should live under the workspace crates directory")
}
