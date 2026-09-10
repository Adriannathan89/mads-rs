//! Release preparation scripts and stable workflow policy acceptance tests.

use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::process::{Command, Output};

#[cfg(unix)]
use tempfile::{TempDir, tempdir};

const PACKAGES: &[&str] = &[
    "mads-core-macros",
    "mads-common-macros",
    "mads-core",
    "mads-extra",
    "mads-common",
    "mads",
    "mads-cli",
];

#[cfg(unix)]
#[test]
fn beta_release_increments_a_matching_base_and_only_changes_versions() {
    let fixture = ReleaseFixture::new("0.7.0-beta.1");
    let readme = fs::read(fixture.root().join("README.md")).unwrap();
    let changelog = fs::read(fixture.root().join("CHANGELOG.md")).unwrap();

    let output = fixture.run("release-beta.sh", "0.7.0");

    assert_success(&output);
    fixture.assert_version("0.7.0-beta.2");
    assert_eq!(fs::read(fixture.root().join("README.md")).unwrap(), readme);
    assert_eq!(
        fs::read(fixture.root().join("CHANGELOG.md")).unwrap(),
        changelog
    );
}

#[cfg(unix)]
#[test]
fn beta_release_starts_at_one_for_a_new_base() {
    let fixture = ReleaseFixture::new("0.7.0-beta.4");

    let output = fixture.run("release-beta.sh", "0.8.0");

    assert_success(&output);
    fixture.assert_version("0.8.0-beta.1");
}

#[cfg(unix)]
#[test]
fn stable_release_sets_the_exact_stable_version() {
    let fixture = ReleaseFixture::new("0.7.0-beta.5");

    let output = fixture.run("release.sh", "0.7.0");

    assert_success(&output);
    fixture.assert_version("0.7.0");
}

#[cfg(unix)]
#[test]
fn release_scripts_reject_invalid_versions_without_modifying_the_workspace() {
    let fixture = ReleaseFixture::new("0.7.0-beta.1");
    let before = fixture.version_files();

    let output = fixture.run("release.sh", "0.7");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("X.Y.Z"));
    assert_eq!(fixture.version_files(), before);
}

#[test]
fn stable_workflow_enforces_release_gates_and_dependency_order() {
    let root = workspace_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/stable-publish.yml"))
        .expect("stable publication workflow should exist");

    for required in [
        "branches:\n      - main",
        "Require a stable workspace version",
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cargo test --locked --workspace --all-features",
        "cargo test --locked --workspace --all-features --doc",
        "cargo doc --locked --workspace --all-features --no-deps",
        "cargo test --locked -p mads-cli --test database_generate_postgres -- --ignored --test-threads=1",
        "environment: stable",
        "CARGO_REGISTRY_TOKEN: ${{ secrets.CRATES_IO_TOKEN }}",
        "for attempt in {1..6}",
        "--latest",
    ] {
        assert!(
            workflow.contains(required),
            "missing workflow policy: {required}"
        );
    }
    for dependency in ["verify", "msrv", "postgres"] {
        assert!(
            workflow.contains(&format!("      - {dependency}")),
            "publish must depend on {dependency}"
        );
    }
    assert!(!workflow.contains("--prerelease"));

    let mut offset = 0;
    for package in PACKAGES {
        let relative = workflow[offset..]
            .find(&format!("            {package}\n"))
            .unwrap_or_else(|| panic!("missing package {package} in publication order"));
        offset += relative + package.len();
    }
}

#[test]
fn beta_and_stable_workflows_require_the_complete_v080_gate_set() {
    let root = workspace_root();
    let beta = fs::read_to_string(root.join(".github/workflows/beta-publish.yml"))
        .expect("beta publication workflow should exist");
    let stable = fs::read_to_string(root.join(".github/workflows/stable-publish.yml"))
        .expect("stable publication workflow should exist");

    for (name, workflow, environment) in [
        ("beta", &beta, "environment: beta"),
        ("stable", &stable, "environment: stable"),
    ] {
        let verify = workflow_job(workflow, "verify");
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
                verify.contains(required),
                "{name} verify job missing {required}"
            );
        }

        let platform = workflow_job(workflow, "cli-platform");
        for required in [
            "ubuntu-latest",
            "macos-latest",
            "windows-latest",
            "cargo test -p mads-cli --lib command::tests -- --test-threads=1",
            "cargo test -p mads-cli --test json_cli -- --test-threads=1",
            "model_serializes_nullable_diagnostics_and_normalized_locations",
            "cargo test -p mads-cli --test scaffold_cli -- --test-threads=1",
            "a_binary_without_standard_run_is_killed_and_diagnosed",
            "cargo test -p mads-cli --test dev_cli real_dev_loop -- --test-threads=1",
        ] {
            assert!(
                platform.contains(required),
                "{name} platform job missing {required}"
            );
        }
        assert!(
            !platform.contains("services:"),
            "{name} platform job must stay portable"
        );
        let postgres = workflow_job(workflow, "postgres");
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
                postgres.contains(required),
                "{name} PostgreSQL job missing {required}"
            );
        }

        for dependency in ["verify", "cli-platform", "msrv", "postgres", "coverage"] {
            assert!(
                workflow.contains(&format!("      - {dependency}")),
                "{name} publish must depend on {dependency}",
            );
        }
        assert!(
            workflow.contains(environment),
            "{name} publish must keep its protection"
        );
    }

    let beta_feature_gates = feature_test_commands(&beta);
    let stable_feature_gates = feature_test_commands(&stable);
    assert_eq!(
        stable_feature_gates, beta_feature_gates,
        "stable promotion must verify the same feature set as beta"
    );
}

#[test]
fn release_workflows_verify_v080_feature_boundaries_and_package_contents() {
    let root = workspace_root();
    let beta = fs::read_to_string(root.join(".github/workflows/beta-publish.yml"))
        .expect("beta publication workflow should exist");
    let stable = fs::read_to_string(root.join(".github/workflows/stable-publish.yml"))
        .expect("stable publication workflow should exist");

    for (name, workflow) in [("beta", &beta), ("stable", &stable)] {
        let verify = workflow_job(workflow, "verify");
        for command in [
            "cargo check -p mads-core --no-default-features",
            "cargo check -p mads-common --no-default-features --features http",
            "cargo check -p mads-common --no-default-features --features database",
            "cargo check -p mads-common --no-default-features --features http,database",
            "cargo check -p mads-common --no-default-features --features jwt",
            "cargo check -p mads-common --no-default-features --features cookies",
            "cargo check -p mads --no-default-features",
            "cargo check -p mads --no-default-features --features http,runtime-tokio",
            "cargo check -p mads --no-default-features --features http,database",
            "cargo package --locked --workspace --no-verify",
        ] {
            assert!(
                verify.contains(command),
                "{name} release gate is missing feature or archive verification: {command}"
            );
        }
        for package in PACKAGES {
            assert!(
                verify.contains("bash script/verify-package-contents.sh"),
                "{name} release gate must execute the package-content policy for {package}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn package_content_policy_checks_every_workspace_archive() {
    let root = workspace_root();
    let policy = root.join("script/verify-package-contents.sh");
    assert!(
        policy.is_file(),
        "package-content policy script should exist"
    );

    let output = Command::new("bash")
        .arg(&policy)
        .current_dir(root)
        .output()
        .expect("package-content policy should start");
    assert_success(&output);
}

#[test]
fn workspace_packages_use_exact_v080_stable_internal_pins() {
    const VERSION: &str = "0.8.0";

    let root = workspace_root();
    let workspace_manifest =
        fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest should exist");
    assert!(
        workspace_manifest.contains(&format!("version = \"{VERSION}\"")),
        "the workspace must remain at the approved stable version"
    );

    let lockfile =
        fs::read_to_string(root.join("Cargo.lock")).expect("workspace lockfile should exist");
    for package in PACKAGES {
        let manifest = fs::read_to_string(root.join("crates").join(package).join("Cargo.toml"))
            .unwrap_or_else(|error| panic!("{package} manifest should exist: {error}"));
        assert!(
            manifest.contains("version.workspace = true"),
                "{package} must inherit the workspace stable version"
        );

        for dependency in manifest
            .lines()
            .filter(|line| line.contains("path = \"../"))
        {
            assert!(
                dependency.contains(&format!("version = \"={VERSION}\"")),
                "{package} internal dependency must use an exact stable pin: {dependency}"
            );
        }

        let record = format!("name = \"{package}\"\nversion = \"{VERSION}\"");
        assert!(
            lockfile.contains(&record),
            "lockfile must contain {package} at {VERSION}"
        );
    }
}

#[test]
fn documentation_describes_the_v080_compatibility_boundaries() {
    let root = workspace_root();
    let readme = fs::read_to_string(root.join("README.md")).expect("README should exist");
    let architecture = fs::read_to_string(root.join("docs/ARCHITECTURE.md"))
        .expect("architecture guide should exist");

    for (name, source) in [("README", &readme), ("architecture", &architecture)] {
        for required in [
            "ValidatedJson",
            "native `Json`",
            "Config::parse",
            "Secret",
            ".into_http()",
        ] {
            assert!(
                source.contains(required),
                "{name} must document the v0.8 compatibility contract: {required}",
            );
        }
    }

    for stale_claim in [
        "request-validation derives or schemas",
        "generic typed configuration, third-party",
        "machine-readable CLI output are deferred to\nv0.8",
    ] {
        assert!(
            !readme.contains(stale_claim),
            "README still describes a shipped v0.8 feature as deferred: {stale_claim}",
        );
        assert!(
            !architecture.contains(stale_claim),
            "architecture still describes a shipped v0.8 feature as deferred: {stale_claim}",
        );
    }

    let current_surface = fs::read_to_string(root.join("docs/final_ideav1.md"))
        .expect("current v1 surface should exist");
    for required in [
        "ValidatedJson",
        "Config::parse",
        "Secret",
        ".into_http()",
        "schema_version",
        "mads new <name>",
    ] {
        assert!(
            current_surface.contains(required),
            "current v1 surface must document {required}",
        );
    }
    assert!(!current_surface.contains("validation adalah target v1"));
    assert!(!current_surface.contains("machine-readable CLI output remain v0.8 directions"));

    let complete_example =
        fs::read_to_string(root.join("docs/examples/final_application_clean_architecture.md"))
            .expect("complete application example should exist");
    for extractor in ["ValidatedJson", "ValidatedQuery", "ValidatedPath"] {
        assert!(
            complete_example.contains(extractor),
            "complete application example must use {extractor}",
        );
    }
    assert!(!complete_example.contains("planned validation API"));

    let passport = fs::read_to_string(root.join("docs/examples/passport_jwt.md"))
        .expect("Passport example should exist");
    for required in [
        "0.8.0-beta.1",
        "ValidatedJson",
        "authentication was rejected",
        "access was denied",
        "WWW-Authenticate: Bearer",
    ] {
        assert!(
            passport.contains(required),
            "Passport example missing {required}"
        );
    }

    let features = fs::read_to_string(root.join("docs/importance/version_0.8.0/features.md"))
        .expect("v0.8 feature evidence should exist");
    for required in [
        "0.8.0-beta.1",
        "ValidatedJson",
        "Config::parse",
        ".into_http()",
        "schema_version",
        "mads new",
    ] {
        assert!(
            features.contains(required),
            "v0.8 feature guide missing {required}"
        );
    }

    let stable = fs::read_to_string(root.join("docs/importance/version_0.8.0/stable-promotion.md"))
        .expect("v0.8 stable-promotion guide should exist");
    assert!(stable.contains("no new features"));
    assert!(stable.contains("0.8.0-beta.1"));

    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog should exist");
    assert!(changelog.contains("## [0.8.0-beta.1]"));
    for required in [
        "mads new",
        "ValidatedJson",
        "Config::parse",
        "schema version 1",
    ] {
        assert!(
            changelog.contains(required),
            "v0.8 changelog missing {required}"
        );
    }
}

#[cfg(unix)]
struct ReleaseFixture {
    root: TempDir,
}

#[cfg(unix)]
impl ReleaseFixture {
    fn new(version: &str) -> Self {
        let root = tempdir().expect("release fixture should be created");
        write(
            &root.path().join("Cargo.toml"),
            &format!(
                "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"{version}\"\nedition = \"2024\"\n"
            ),
        );
        for package in PACKAGES {
            let crate_root = root.path().join("crates").join(package);
            fs::create_dir_all(crate_root.join("src")).unwrap();
            write(&crate_root.join("src/lib.rs"), "");
            write(
                &crate_root.join("Cargo.toml"),
                &fixture_manifest(package, version),
            );
        }
        write(&root.path().join("README.md"), "release fixture readme\n");
        write(
            &root.path().join("CHANGELOG.md"),
            "# Changelog\n\nfixture notes\n",
        );
        let git = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(root.path())
            .output()
            .expect("git should initialize the fixture");
        assert_success(&git);
        let lock = Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(root.path())
            .output()
            .expect("Cargo should generate the fixture lockfile");
        assert_success(&lock);
        Self { root }
    }

    fn root(&self) -> &Path {
        self.root.path()
    }

    fn run(&self, script: &str, version: &str) -> Output {
        Command::new("bash")
            .arg(workspace_root().join("script").join(script))
            .arg(version)
            .current_dir(self.root())
            .output()
            .expect("release script should execute")
    }

    fn assert_version(&self, expected: &str) {
        let root_manifest = fs::read_to_string(self.root().join("Cargo.toml")).unwrap();
        assert!(root_manifest.contains(&format!("version = \"{expected}\"")));

        for package in PACKAGES {
            let manifest =
                fs::read_to_string(self.root().join("crates").join(package).join("Cargo.toml"))
                    .unwrap();
            assert!(
                !manifest.contains("0.7.0-beta.1")
                    && !manifest.contains("0.7.0-beta.4")
                    && !manifest.contains("0.7.0-beta.5"),
                "old version remains in {package}"
            );
            for line in manifest.lines().filter(|line| line.contains("path =")) {
                assert!(
                    line.contains(&format!("version = \"={expected}\"")),
                    "internal pin was not updated in {package}: {line}"
                );
            }
        }

        let lock = fs::read_to_string(self.root().join("Cargo.lock")).unwrap();
        for package in PACKAGES {
            let record = format!("name = \"{package}\"\nversion = \"{expected}\"");
            assert!(
                lock.contains(&record),
                "lockfile missing {package} {expected}"
            );
        }
    }

    fn version_files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut paths = vec![
            self.root().join("Cargo.toml"),
            self.root().join("Cargo.lock"),
        ];
        paths.extend(
            PACKAGES
                .iter()
                .map(|package| self.root().join("crates").join(package).join("Cargo.toml")),
        );
        paths
            .into_iter()
            .map(|path| {
                let contents = fs::read(&path).unwrap();
                (path, contents)
            })
            .collect()
    }
}

#[cfg(unix)]
fn fixture_manifest(package: &str, version: &str) -> String {
    let dependencies = match package {
        "mads-core" => vec![("mads-core-macros", "../mads-core-macros")],
        "mads-extra" => vec![("mads-core", "../mads-core")],
        "mads-common" => vec![
            ("mads-common-macros", "../mads-common-macros"),
            ("mads-core", "../mads-core"),
        ],
        "mads" => vec![
            ("mads-common", "../mads-common"),
            ("mads-core", "../mads-core"),
            ("mads-extra", "../mads-extra"),
        ],
        "mads-cli" => vec![("mads", "../mads"), ("mads-common", "../mads-common")],
        _ => Vec::new(),
    };
    let mut manifest = format!(
        "[package]\nname = \"{package}\"\nversion.workspace = true\nedition.workspace = true\n"
    );
    if !dependencies.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for (dependency, path) in dependencies {
            manifest.push_str(&format!(
                "{dependency} = {{ path = \"{path}\", version = \"={version}\" }}\n"
            ));
        }
    }
    manifest
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("mads-cli should be inside the workspace crates directory")
        .to_path_buf()
}

#[cfg(unix)]
fn write(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
}

#[cfg(unix)]
fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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

fn feature_test_commands(workflow: &str) -> Vec<&str> {
    workflow
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("- run: ").or(Some(line)))
        .filter(|line| line.starts_with("cargo test ") || line.starts_with("cargo llvm-cov "))
        .collect()
}
