//! Cargo feature-composition regression tests.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dependency_tree(features: &str) -> String {
    let root = workspace_root();
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "tree",
            "-e",
            "normal",
            "-p",
            "mads",
            "--no-default-features",
            "--features",
            features,
        ])
        .output()
        .expect("cargo tree should start");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("cargo output should be UTF-8")
}

#[test]
fn jwt_only_excludes_http_and_database_dependencies() {
    let tree = dependency_tree("jwt");
    for forbidden in [
        "axum v",
        "axum-extra v",
        "tower-http v",
        "diesel v",
        "deadpool-diesel v",
        "diesel_migrations v",
    ] {
        assert!(
            !tree.contains(forbidden),
            "unexpected dependency: {forbidden}\n{tree}"
        );
    }
}

#[test]
fn http_includes_validation_and_rest_without_database_or_jwt() {
    let tree = dependency_tree("http");
    for required in [
        "axum v",
        "axum-extra v",
        "mads-common-macros v",
        "tower-http v",
    ] {
        assert!(
            tree.contains(required),
            "missing HTTP validation or REST dependency: {required}\n{tree}"
        );
    }
    for forbidden in [
        "diesel v",
        "deadpool-diesel v",
        "diesel_migrations v",
        "jsonwebtoken v",
        "cookie v",
    ] {
        assert!(
            !tree.contains(forbidden),
            "unexpected HTTP dependency: {forbidden}\n{tree}"
        );
    }
}

#[test]
fn http_and_database_expose_the_explicit_mapping_pair_gate() {
    let tree = dependency_tree("http,database");
    for required in ["axum v", "tower-http v", "diesel v", "deadpool-diesel v"] {
        assert!(
            tree.contains(required),
            "missing dependency required by the http + database mapping gate: {required}\n{tree}"
        );
    }
    for forbidden in ["jsonwebtoken v", "cookie v"] {
        assert!(
            !tree.contains(forbidden),
            "unexpected authentication dependency: {forbidden}\n{tree}"
        );
    }

    let facade = std::fs::read_to_string(workspace_root().join("crates/mads/src/lib.rs"))
        .expect("mads facade source should exist");
    assert!(
        facade.contains(
            "#[cfg(all(feature = \"http\", feature = \"database\"))]\npub use mads_common::IntoHttpResult;"
        ),
        "IntoHttpResult must remain available only when http and database are both enabled"
    );
}

#[test]
fn database_excludes_http_dependencies() {
    let tree = dependency_tree("database");
    for forbidden in ["axum v", "axum-extra v", "tower-http v"] {
        assert!(
            !tree.contains(forbidden),
            "unexpected HTTP dependency: {forbidden}\n{tree}"
        );
    }
}

#[test]
fn cookies_include_http_but_not_jwt_or_database() {
    let tree = dependency_tree("cookies");
    assert!(tree.contains("axum v"));
    assert!(tree.contains("cookie v"));
    assert!(!tree.contains("jsonwebtoken v"));
    assert!(!tree.contains("diesel v"));
}

#[test]
fn generated_minimal_project_selects_only_http_and_tokio() {
    let template = std::fs::read_to_string(
        workspace_root().join("crates/mads-cli/src/scaffold/templates/Cargo.toml.txt"),
    )
    .expect("generated project manifest template should exist");

    assert!(
        template.contains(
            "mads = { version = \"={{mads_version}}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }"
        ),
        "the generated project must select exactly HTTP and Tokio support"
    );
    for forbidden in [
        "database",
        "diesel",
        "deadpool",
        "jwt",
        "jsonwebtoken",
        "cookie",
        "migration",
    ] {
        assert!(
            !template.contains(forbidden),
            "the generated minimal project must not select {forbidden}"
        );
    }
}
