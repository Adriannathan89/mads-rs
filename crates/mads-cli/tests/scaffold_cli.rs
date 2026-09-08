//! Contract coverage for the bundled minimal MADS project starter.

use std::{
    fs,
    path::{Path, PathBuf},
};

use mads_cli::scaffold::{GENERATED_FILES, ProjectName, publish_project, render_project};
use tempfile::tempdir;

#[test]
fn name_and_templates_accept_the_approved_project_names() {
    for name in ["a", "app1", "my-app", "my_app"] {
        let validated = ProjectName::parse(name)
            .unwrap_or_else(|error| panic!("{name:?} should be accepted: {error}"));

        assert_eq!(validated.as_str(), name);
        assert_eq!(validated.to_string(), name);
    }
}

#[test]
fn name_and_templates_reject_invalid_project_names() {
    let cargo_reserved = ["test", "deps", "examples", "build", "incremental"];
    let rust_2024_strict = [
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn",
    ];
    let rust_2024_reserved = [
        "abstract", "become", "box", "do", "final", "gen", "macro", "override", "priv", "try",
        "typeof", "unsized", "virtual", "yield",
    ];
    let invalid_grammar = [
        "", "App", "my-App", "héllo", "应用", "my app", "\tapp", "1app", "-app", "_app", "my.app",
        "my/app", "my\\app", "my@app", "app!",
    ];

    for name in cargo_reserved
        .iter()
        .chain(rust_2024_strict.iter())
        .chain(rust_2024_reserved.iter())
        .chain(invalid_grammar.iter())
    {
        assert!(
            ProjectName::parse(name).is_err(),
            "{name:?} must be rejected"
        );
    }

    let error = ProjectName::parse("untrusted project name")
        .expect_err("whitespace-containing name should be invalid");
    assert_eq!(error.code(), "MADS230");
    assert_eq!(
        error.to_string(),
        "project name must start with a lowercase ASCII letter and use only lowercase ASCII letters, digits, '-' or '_' thereafter"
    );
    assert!(!error.to_string().contains("untrusted project name"));
}

#[test]
fn name_and_templates_render_the_exact_minimal_project_manifest() {
    let name = ProjectName::parse("my-app").expect("fixture name should be valid");
    let rendered = render_project(&name).expect("bundled templates should render");
    let actual = rendered
        .files()
        .iter()
        .map(|file| {
            (
                file.path().to_string_lossy().into_owned(),
                file.contents().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let expected = vec![
        (
            "Cargo.toml".to_owned(),
            format!(
                "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\n\n[dependencies]\nmads = {{ version = \"={}\", default-features = false, features = [\"http\", \"runtime-tokio\"] }}\n",
                env!("CARGO_PKG_VERSION")
            ),
        ),
        (
            "mads.toml".to_owned(),
            "[server]\nhost = \"127.0.0.1\"\nport = 3000\n".to_owned(),
        ),
        (
            "src/main.rs".to_owned(),
            "// src/main.rs\nmod app;\n\nuse app::AppModule;\nuse mads::prelude::*;\n\n#[mads::main]\nasync fn main() -> Result<(), HttpRuntimeError> {\n    Mads::run::<AppModule>().await\n}\n"
                .to_owned(),
        ),
        (
            "src/app/mod.rs".to_owned(),
            "// src/app/mod.rs\nmod controller;\nmod routes;\nmod service;\n\nuse mads::prelude::*;\n\n#[module]\npub struct AppModule;\n"
                .to_owned(),
        ),
        (
            "src/app/routes.rs".to_owned(),
            "// src/app/routes.rs\nuse mads::prelude::*;\n\n#[routes]\npub trait AppRoutes {\n    #[get(\"/\")]\n    async fn hello(&self) -> &'static str;\n}\n"
                .to_owned(),
        ),
        (
            "src/app/controller.rs".to_owned(),
            "// src/app/controller.rs\nuse mads::prelude::*;\n\nuse super::{routes::AppRoutes, service::AppService};\n\n#[controller(routes = [AppRoutes])]\npub struct AppController {\n    service: AppService,\n}\n\nimpl AppRoutes for AppController {\n    async fn hello(&self) -> &'static str {\n        self.service.hello()\n    }\n}\n"
                .to_owned(),
        ),
        (
            "src/app/service.rs".to_owned(),
            "// src/app/service.rs\nuse mads::prelude::*;\n\n#[service]\npub struct AppService;\n\nimpl AppService {\n    pub fn hello(&self) -> &'static str {\n        \"Hello World!\"\n    }\n}\n"
                .to_owned(),
        ),
    ];

    assert_eq!(actual, expected);
    assert_eq!(
        GENERATED_FILES.map(Path::new),
        [
            Path::new("Cargo.toml"),
            Path::new("mads.toml"),
            Path::new("src/main.rs"),
            Path::new("src/app/mod.rs"),
            Path::new("src/app/routes.rs"),
            Path::new("src/app/controller.rs"),
            Path::new("src/app/service.rs"),
        ]
    );
}

#[test]
fn filesystem_publishes_the_complete_project_without_a_staging_sibling() {
    let invocation = tempdir().expect("temporary invocation directory should be created");
    let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
    let rendered = render_project(&name).expect("bundled templates should render");

    let destination = publish_project(invocation.path(), &name, &rendered)
        .expect("a fresh destination should be published");

    assert_eq!(destination, invocation.path().join("minimal-app"));
    assert_eq!(published_files(&destination), expected_generated_files());
    assert!(
        !has_staging_sibling(invocation.path(), name.as_str()),
        "successful publication must not leave a staging directory"
    );
}

#[cfg(windows)]
#[test]
fn filesystem_windows_publishes_the_complete_project_layout() {
    let invocation = tempdir().expect("temporary invocation directory should be created");
    let name = ProjectName::parse("windows-app").expect("fixture name should be valid");
    let rendered = render_project(&name).expect("bundled templates should render");

    let destination = publish_project(invocation.path(), &name, &rendered)
        .expect("Windows directory synchronization must not prevent publication");

    assert_eq!(destination, invocation.path().join("windows-app"));
    assert_eq!(published_files(&destination), expected_generated_files());
    assert!(
        !has_staging_sibling(invocation.path(), name.as_str()),
        "successful publication must not leave a staging directory"
    );
}

#[test]
fn filesystem_refuses_existing_files_and_directories_without_touching_them() {
    for existing_kind in ["file", "directory"] {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
        let rendered = render_project(&name).expect("bundled templates should render");
        let destination = invocation.path().join(name.as_str());

        match existing_kind {
            "file" => fs::write(&destination, "preserved").expect("fixture file should be written"),
            "directory" => {
                fs::create_dir(&destination).expect("fixture directory should be created");
                fs::write(destination.join("sentinel"), "preserved")
                    .expect("fixture sentinel should be written");
            }
            _ => unreachable!("the fixture cases are fixed"),
        }

        let error = publish_project(invocation.path(), &name, &rendered)
            .expect_err("an existing destination must never be replaced");

        assert_eq!(error.code(), "MADS230");
        match existing_kind {
            "file" => assert_eq!(fs::read_to_string(&destination).unwrap(), "preserved"),
            "directory" => assert_eq!(
                fs::read_to_string(destination.join("sentinel")).unwrap(),
                "preserved"
            ),
            _ => unreachable!("the fixture cases are fixed"),
        }
        assert!(
            !has_staging_sibling(invocation.path(), name.as_str()),
            "rejected destination must not create a staging directory"
        );
    }
}

#[cfg(unix)]
#[test]
fn filesystem_refuses_ordinary_and_dangling_destination_symlinks() {
    use std::os::unix::fs::symlink;

    for target_kind in ["ordinary", "dangling"] {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
        let rendered = render_project(&name).expect("bundled templates should render");
        let destination = invocation.path().join(name.as_str());
        let target = invocation.path().join(format!("{target_kind}-target"));
        if target_kind == "ordinary" {
            fs::write(&target, "preserved").expect("ordinary symlink target should be written");
        }
        symlink(&target, &destination).expect("destination symlink should be created");

        let error = publish_project(invocation.path(), &name, &rendered)
            .expect_err("a symlink destination must never be followed or replaced");

        assert_eq!(error.code(), "MADS230");
        assert!(
            fs::symlink_metadata(&destination)
                .expect("symlink destination should be preserved")
                .file_type()
                .is_symlink()
        );
        if target_kind == "ordinary" {
            assert_eq!(fs::read_to_string(&target).unwrap(), "preserved");
        }
        assert!(
            !has_staging_sibling(invocation.path(), name.as_str()),
            "rejected symlink must not create a staging directory"
        );
    }
}

#[test]
fn filesystem_preserves_unrelated_siblings_during_publication() {
    let invocation = tempdir().expect("temporary invocation directory should be created");
    let unrelated = invocation.path().join("keep-me.txt");
    fs::write(&unrelated, "preserved").expect("unrelated sibling should be written");
    let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
    let rendered = render_project(&name).expect("bundled templates should render");

    publish_project(invocation.path(), &name, &rendered)
        .expect("publication should not affect unrelated siblings");

    assert_eq!(fs::read_to_string(unrelated).unwrap(), "preserved");
}

fn published_files(destination: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(destination, destination, &mut files);
    files.sort();
    files
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("published directory should be readable") {
        let entry = entry.expect("published directory entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("published entry should be below destination")
                    .to_path_buf(),
            );
        }
    }
}

fn expected_generated_files() -> Vec<PathBuf> {
    let mut files = GENERATED_FILES.map(PathBuf::from).to_vec();
    files.sort();
    files
}

fn has_staging_sibling(invocation: &Path, name: &str) -> bool {
    let prefix = format!(".{name}.mads-");
    fs::read_dir(invocation)
        .expect("invocation directory should be readable")
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_name().to_string_lossy().starts_with(&prefix)
                && entry.file_name().to_string_lossy().ends_with(".tmp")
        })
}
