//! End-to-end evidence that a freshly generated project is a usable MADS consumer.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::{Value, json};
use tempfile::tempdir;

#[test]
fn generated_project_compiles_and_exposes_the_minimal_application_offline() {
    let invocation = tempdir().expect("temporary invocation directory should be created");
    let output = Command::new(env!("CARGO_BIN_EXE_mads"))
        .current_dir(invocation.path())
        .args(["new", "generated-app", "--format", "json"])
        .output()
        .expect("mads new should run");
    assert!(output.status.success(), "mads new failed: {output:?}");
    assert!(
        output.stderr.is_empty(),
        "mads new wrote stderr: {output:?}"
    );

    let project = invocation.path().join("generated-app");
    let manifest_path = project.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("generated manifest should exist");
    assert_eq!(manifest, registry_manifest());
    for forbidden in [
        "path =",
        "database",
        "diesel",
        "deadpool",
        "jwt",
        "jsonwebtoken",
        "cookie",
        "migration",
    ] {
        assert!(
            !manifest.to_ascii_lowercase().contains(forbidden),
            "registry-ready manifest unexpectedly contained {forbidden:?}:\n{manifest}"
        );
    }
    for absent in [".git", "Cargo.lock", "target"] {
        assert!(
            !project.join(absent).exists(),
            "generation must not create {absent} before Cargo is invoked"
        );
    }

    fs::write(&manifest_path, local_manifest(&manifest))
        .expect("test-only local MADS dependency substitution should succeed");

    let metadata_output = cargo_command(&project)
        .args(["metadata", "--format-version", "1", "--offline"])
        .output()
        .expect("offline cargo metadata should run");
    assert_success("cargo metadata", &metadata_output);
    let metadata: Value =
        serde_json::from_slice(&metadata_output.stdout).expect("metadata should be JSON");
    let resolved = resolved_package_names(&metadata);
    for required in ["axum", "tokio"] {
        assert!(
            resolved.contains(required),
            "resolved graph should include {required}; packages={resolved:?}"
        );
    }
    for excluded in [
        "diesel",
        "deadpool-diesel",
        "jsonwebtoken",
        "cookie",
        "diesel_migrations",
        "migrations_internals",
        "migrations_macros",
    ] {
        assert!(
            !resolved.contains(excluded),
            "minimal resolved graph unexpectedly included {excluded}; packages={resolved:?}"
        );
    }

    let check = cargo_command(&project)
        .args(["check", "--offline"])
        .output()
        .expect("offline cargo check should run");
    assert_success("cargo check", &check);

    let routes = inspection_json(&project, ["routes", "--format", "json"]);
    assert_eq!(routes["command"], "routes");
    assert_eq!(routes["ok"], true);
    assert_eq!(routes["diagnostics"], json!([]));
    let route = routes["data"]["routes"]
        .as_array()
        .and_then(|routes| routes.first())
        .expect("generated application should expose one route");
    assert_eq!(routes["data"]["routes"].as_array().unwrap().len(), 1);
    assert_eq!(route["method"], "GET");
    assert_eq!(route["path"], "/");
    assert_eq!(route["route_trait"], "AppRoutes");
    assert_eq!(route["handler"], "hello");
    assert!(
        route["controller"]
            .as_str()
            .is_some_and(|name| name.ends_with("::app::controller::AppController")),
        "unexpected route controller: {route}"
    );

    let graph = inspection_json(&project, ["graph", "--format", "json"]);
    assert_eq!(graph["command"], "graph");
    assert_eq!(graph["ok"], true);
    assert_eq!(graph["diagnostics"], json!([]));
    assert!(
        graph["data"]["root_module"]
            .as_str()
            .is_some_and(|name| name.ends_with("::app::AppModule")),
        "unexpected root module: {graph}"
    );
    let providers = string_field_set(&graph["data"]["providers"], "type_name");
    for required in ["AppController", "AppService"] {
        assert!(
            providers.iter().any(|name| name.ends_with(required)),
            "provider graph should include {required}: {providers:?}"
        );
    }
    let construction_order = graph["data"]["construction_order"]
        .as_array()
        .expect("generated provider graph should have a construction order")
        .iter()
        .map(|name| {
            name.as_str()
                .expect("construction entries should be strings")
        })
        .collect::<Vec<_>>();
    let service = construction_order
        .iter()
        .position(|name| name.ends_with("AppService"))
        .expect("construction order should include AppService");
    let controller = construction_order
        .iter()
        .position(|name| name.ends_with("AppController"))
        .expect("construction order should include AppController");
    assert!(
        service < controller,
        "AppService must be constructed before AppController: {construction_order:?}"
    );

    let doctor = inspection_json(&project, ["doctor", "--format", "json"]);
    assert_eq!(doctor["command"], "doctor");
    assert_eq!(doctor["ok"], true);
    assert_eq!(doctor["diagnostics"], json!([]));
    let checks = doctor["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    assert!(!checks.is_empty(), "doctor should report its checks");
    assert!(
        checks.iter().all(|check| check["status"] != "fail"),
        "all generated-project doctor checks should pass or be skipped: {checks:?}"
    );
}

fn registry_manifest() -> String {
    format!(
        "[package]\nname = \"generated-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\n\n[dependencies]\nmads = {{ version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn local_manifest(registry_manifest: &str) -> String {
    let local_mads = workspace_root()
        .join("crates/mads")
        .canonicalize()
        .expect("local mads crate should exist");
    let toml_path = local_mads.to_string_lossy().replace('\\', "\\\\");
    let registry_dependency = format!(
        "mads = {{ version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}",
        env!("CARGO_PKG_VERSION")
    );
    let local_dependency = format!(
        "mads = {{ path = \"{toml_path}\", version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}",
        env!("CARGO_PKG_VERSION")
    );
    let substituted = registry_manifest.replacen(&registry_dependency, &local_dependency, 1);
    assert_ne!(
        substituted, registry_manifest,
        "the registry dependency line should be substituted exactly once"
    );
    assert!(
        !substituted.contains(&registry_dependency),
        "the registry dependency source should not remain after substitution"
    );
    substituted
}

fn cargo_command(project: &Path) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(project)
        .env("CARGO_NET_OFFLINE", "true");
    command
}

fn inspection_json<const N: usize>(project: &Path, arguments: [&str; N]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mads"))
        .current_dir(project)
        .env("CARGO_NET_OFFLINE", "true")
        .args(arguments)
        .output()
        .expect("inspection command should run");
    assert_success("mads inspection", &output);
    serde_json::from_slice(&output.stdout).expect("inspection stdout should be JSON")
}

fn resolved_package_names(metadata: &Value) -> HashSet<String> {
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata packages should be an array")
        .iter()
        .map(|package| {
            (
                package["id"]
                    .as_str()
                    .expect("package id should be a string"),
                package["name"]
                    .as_str()
                    .expect("package name should be a string"),
            )
        })
        .collect::<HashMap<_, _>>();

    metadata["resolve"]["nodes"]
        .as_array()
        .expect("metadata resolve nodes should be an array")
        .iter()
        .map(|node| {
            let id = node["id"]
                .as_str()
                .expect("resolved package id should be a string");
            packages
                .get(id)
                .unwrap_or_else(|| panic!("resolved package {id} should have metadata"))
                .to_string()
        })
        .collect()
}

fn string_field_set(value: &Value, field: &str) -> Vec<String> {
    value
        .as_array()
        .expect("schema field should be an array")
        .iter()
        .map(|entry| {
            entry[field]
                .as_str()
                .unwrap_or_else(|| panic!("{field} should be a string"))
                .to_owned()
        })
        .collect()
}

fn assert_success(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("mads-cli should live under the workspace crates directory")
        .to_path_buf()
}
