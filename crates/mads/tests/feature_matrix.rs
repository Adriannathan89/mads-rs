//! Cargo feature-composition regression tests.

use std::path::PathBuf;
use std::process::Command;

fn dependency_tree(features: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
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
fn http_includes_tower_http() {
    let tree = dependency_tree("http");
    assert!(tree.contains("tower-http v"));
}

#[test]
fn http_excludes_database_dependencies() {
    let tree = dependency_tree("http");
    for forbidden in ["diesel v", "deadpool-diesel v", "diesel_migrations v"] {
        assert!(
            !tree.contains(forbidden),
            "unexpected database dependency: {forbidden}\n{tree}"
        );
    }
}

#[test]
fn common_remains_http_and_database_without_authentication() {
    let tree = dependency_tree("common");
    assert!(tree.contains("axum v"));
    assert!(tree.contains("tower-http v"));
    assert!(tree.contains("diesel v"));
    assert!(!tree.contains("jsonwebtoken v"));
    assert!(!tree.contains("cookie v"));
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
