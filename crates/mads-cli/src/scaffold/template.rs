//! Embedded templates and deterministic rendering for new MADS applications.

use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use crate::diagnostic::MADS230;

use super::ProjectName;

/// The seven generated files in their documented output order.
pub const GENERATED_FILES: [&str; 7] = [
    "Cargo.toml",
    "mads.toml",
    "src/main.rs",
    "src/app/mod.rs",
    "src/app/routes.rs",
    "src/app/controller.rs",
    "src/app/service.rs",
];

const TEMPLATES: [&str; 7] = [
    include_str!("templates/Cargo.toml.txt"),
    include_str!("templates/mads.toml.txt"),
    include_str!("templates/main.rs.txt"),
    include_str!("templates/app_mod.rs.txt"),
    include_str!("templates/routes.rs.txt"),
    include_str!("templates/controller.rs.txt"),
    include_str!("templates/service.rs.txt"),
];

const PROJECT_NAME_TOKEN: &str = "{{project_name}}";
const MADS_VERSION_TOKEN: &str = "{{mads_version}}";

/// One rendered starter file that has not yet been published to disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedFile {
    path: PathBuf,
    contents: String,
}

impl RenderedFile {
    /// Returns the checked relative destination path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the complete UTF-8 contents that should be written to the file.
    pub fn contents(&self) -> &str {
        &self.contents
    }
}

/// The complete deterministic in-memory project manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedProject {
    files: Vec<RenderedFile>,
}

impl RenderedProject {
    /// Returns the seven rendered files in documented output order.
    pub fn files(&self) -> &[RenderedFile] {
        &self.files
    }
}

/// A controlled MADS230 error in the fixed bundled starter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateError;

impl TemplateError {
    /// Returns the stable scaffolding diagnostic code.
    pub const fn code(self) -> &'static str {
        MADS230
    }

    /// Returns the stable scaffolding diagnostic title.
    pub const fn title(self) -> &'static str {
        "Project scaffolding failed"
    }
}

impl fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the bundled project template contains an invalid file path")
    }
}

impl std::error::Error for TemplateError {}

/// Renders the exact seven-file starter without writing to the filesystem.
pub fn render_project(name: &ProjectName) -> Result<RenderedProject, TemplateError> {
    GENERATED_FILES
        .iter()
        .zip(TEMPLATES)
        .map(|(path, template)| {
            let path = PathBuf::from(path);
            if !is_safe_relative_path(&path) {
                return Err(TemplateError);
            }

            Ok(RenderedFile {
                path,
                contents: template
                    .replace("\r\n", "\n")
                    .replace(PROJECT_NAME_TOKEN, name.as_str())
                    .replace(MADS_VERSION_TOKEN, env!("CARGO_PKG_VERSION")),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|files| RenderedProject { files })
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.is_relative()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::is_safe_relative_path;

    #[test]
    fn path_safety_rejects_root_parent_and_current_components() {
        for path in [
            "",
            "/absolute",
            "../parent",
            "src/../main.rs",
            "./src/main.rs",
        ] {
            assert!(
                !is_safe_relative_path(Path::new(path)),
                "{path:?} must not be a generated relative path"
            );
        }

        #[cfg(windows)]
        assert!(!is_safe_relative_path(Path::new(r"C:\absolute")));

        for path in ["Cargo.toml", "src/app/main.rs"] {
            assert!(
                is_safe_relative_path(Path::new(path)),
                "{path:?} should be a generated relative path"
            );
        }
    }
}
