//! Enforces the dependency direction of the core crate.

use std::collections::{HashMap, HashSet};

use cargo_metadata::{CargoOpt, DependencyKind, MetadataCommand, PackageId};

const FORBIDDEN_DEPENDENCY_FRAGMENTS: [&str; 9] = [
    "mads-common",
    "mads-extra",
    "axum",
    "diesel",
    "jsonwebtoken",
    "cookie",
    "http",
    "hyper",
    "tower",
];

fn is_forbidden_dependency(name: &str) -> bool {
    FORBIDDEN_DEPENDENCY_FRAGMENTS
        .iter()
        .any(|fragment| name.contains(fragment))
}

#[test]
fn dependency_name_families_are_forbidden() {
    for name in [
        "axum-extra",
        "mads-common-http",
        "mads-extra-cache",
        "diesel-async",
        "jsonwebtoken",
        "cookie",
    ] {
        assert!(
            is_forbidden_dependency(name),
            "{name} should match a forbidden dependency family"
        );
    }

    assert!(!is_forbidden_dependency("inventory"));
}

#[test]
fn core_configuration_has_no_direct_serialization_or_delivery_dependencies() {
    let workspace_manifest = format!("{}/../../Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let metadata = MetadataCommand::new()
        .manifest_path(workspace_manifest)
        .features(CargoOpt::AllFeatures)
        .exec()
        .expect("workspace metadata should load");
    let core = metadata
        .packages
        .iter()
        .find(|package| package.name == "mads-core")
        .expect("workspace should contain mads-core");
    let normal_dependencies: HashSet<_> = core
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind == DependencyKind::Normal)
        .map(|dependency| dependency.name.as_str())
        .collect();

    for forbidden in ["axum", "serde", "diesel", "jsonwebtoken", "cookie"] {
        assert!(
            !normal_dependencies.contains(forbidden),
            "mads-core configuration must not directly depend on {forbidden}"
        );
    }
}

#[test]
fn core_normal_dependencies_stay_inside_the_core_boundary() {
    let workspace_manifest = format!("{}/../../Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let metadata = MetadataCommand::new()
        .manifest_path(workspace_manifest)
        .features(CargoOpt::AllFeatures)
        .exec()
        .expect("workspace metadata should load");
    let resolve = metadata
        .resolve
        .as_ref()
        .expect("workspace metadata should include a dependency graph");
    let core = metadata
        .packages
        .iter()
        .find(|package| package.name == "mads-core")
        .expect("workspace should contain mads-core");
    let package_names: HashMap<&PackageId, &str> = metadata
        .packages
        .iter()
        .map(|package| (&package.id, package.name.as_str()))
        .collect();
    let nodes: HashMap<&PackageId, _> = resolve.nodes.iter().map(|node| (&node.id, node)).collect();

    let mut visited = HashSet::from([&core.id]);
    let mut pending = vec![&core.id];
    while let Some(package_id) = pending.pop() {
        let node = nodes
            .get(package_id)
            .expect("every package in the dependency graph should have a node");
        for dependency in &node.deps {
            let is_normal = dependency
                .dep_kinds
                .iter()
                .any(|kind| kind.kind == DependencyKind::Normal);
            if is_normal && visited.insert(&dependency.pkg) {
                pending.push(&dependency.pkg);
            }
        }
    }

    let mut violations: Vec<_> = visited
        .into_iter()
        .filter_map(|package_id| package_names.get(package_id).copied())
        .filter(|name| is_forbidden_dependency(name))
        .collect();
    violations.sort_unstable();

    assert!(
        violations.is_empty(),
        "mads-core normal dependency graph crosses a forbidden boundary: {}",
        violations.join(", ")
    );
}
