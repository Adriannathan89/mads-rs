//! Guarded staging and atomic publication for rendered MADS projects.

use std::{
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(not(windows))]
use std::fs::File;

use crate::diagnostic::CliError;

use super::{ProjectName, RenderedProject};

static NEXT_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_STAGING_COLLISIONS: usize = 128;

/// An operational MADS230 failure while staging or publishing a project.
#[derive(Debug)]
pub struct ScaffoldError {
    diagnostic: CliError,
}

impl ScaffoldError {
    /// Returns the stable scaffolding diagnostic code.
    pub const fn code(&self) -> &'static str {
        self.diagnostic.code()
    }

    /// Returns the stable scaffolding diagnostic title.
    pub const fn title(&self) -> &'static str {
        self.diagnostic.title()
    }
}

impl fmt::Display for ScaffoldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic.fmt(formatter)
    }
}

impl Error for ScaffoldError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.diagnostic.source()
    }
}

impl From<ScaffoldError> for CliError {
    fn from(error: ScaffoldError) -> Self {
        error.diagnostic
    }
}

/// Publishes a complete rendered project below the invocation directory.
///
/// The destination must not already exist as a file, directory, or symlink.
/// On success this returns the newly created child directory. Any failure before
/// publication removes only the private staging directory owned by this call.
pub fn publish_project(
    invocation_directory: &Path,
    name: &ProjectName,
    project: &RenderedProject,
) -> Result<PathBuf, ScaffoldError> {
    publish_project_with_operations(
        invocation_directory,
        name,
        project,
        write_new_file,
        |_| Ok(()),
        sync_directory,
        publish_without_replacement,
    )
}

fn publish_project_with_operations<F, H, S, P>(
    invocation_directory: &Path,
    name: &ProjectName,
    project: &RenderedProject,
    mut write_file: F,
    mut before_publish: H,
    mut sync: S,
    mut publish: P,
) -> Result<PathBuf, ScaffoldError>
where
    F: FnMut(&Path, &[u8]) -> io::Result<()>,
    H: FnMut(&Path) -> io::Result<()>,
    S: FnMut(&Path) -> io::Result<()>,
    P: FnMut(&Path, &Path) -> io::Result<()>,
{
    let destination = invocation_directory.join(name.as_str());
    destination_must_be_absent(&destination)?;

    let staging_directory = create_staging_directory(invocation_directory, name)?;
    let mut guard = StagingGuard::new(staging_directory.clone());

    let result = (|| {
        for file in project.files() {
            let destination = staging_directory.join(file.path());
            let parent = destination.parent().ok_or_else(|| {
                scaffolding_error(
                    "the bundled project template contains an invalid file path",
                    io::Error::new(io::ErrorKind::InvalidInput, "missing generated-file parent"),
                )
            })?;
            fs::create_dir_all(parent).map_err(|error| {
                scaffolding_error("the project staging directory could not be prepared", error)
            })?;
            write_file(&destination, file.contents().as_bytes()).map_err(|error| {
                scaffolding_error("a generated project file could not be written", error)
            })?;
        }

        sync(&staging_directory).map_err(|error| {
            scaffolding_error(
                "the project staging directory could not be synchronized",
                error,
            )
        })?;
        before_publish(&destination).map_err(|error| {
            scaffolding_error(
                "the completed project could not be prepared for publication",
                error,
            )
        })?;
        publish(&staging_directory, &destination).map_err(|error| {
            scaffolding_error("the completed project could not be published", error)
        })?;
        Ok(destination)
    })();

    if result.is_ok() {
        guard.disarm();
    }
    result
}

fn destination_must_be_absent(destination: &Path) -> Result<(), ScaffoldError> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(scaffolding_error(
            "the requested project destination already exists",
            io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(scaffolding_error(
            "the requested project destination could not be inspected",
            error,
        )),
    }
}

fn create_staging_directory(
    invocation_directory: &Path,
    name: &ProjectName,
) -> Result<PathBuf, ScaffoldError> {
    for _ in 0..MAX_STAGING_COLLISIONS {
        let counter = NEXT_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging_directory = invocation_directory.join(format!(
            ".{}.mads-{}-{counter}.tmp",
            name.as_str(),
            process::id()
        ));
        match fs::create_dir(&staging_directory) {
            Ok(()) => return Ok(staging_directory),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(scaffolding_error(
                    "a private project staging directory could not be created",
                    error,
                ));
            }
        }
    }

    Err(scaffolding_error(
        "a private project staging directory could not be created",
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "private staging directory names are exhausted",
        ),
    ))
}

fn write_new_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents)?;
    file.flush()?;
    file.sync_all()
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> io::Result<()> {
    match File::open(path).and_then(|file| file.sync_all()) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::Unsupported
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn publish_without_replacement(staging_directory: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    renameat_with(
        CWD,
        staging_directory,
        CWD,
        destination,
        RenameFlags::NOREPLACE,
    )
    .map_err(io::Error::from)
}

#[cfg(windows)]
fn publish_without_replacement(staging_directory: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(staging_directory, destination)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn publish_without_replacement(_staging_directory: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-replace directory publication is unavailable on this platform",
    ))
}

fn scaffolding_error(
    message: &'static str,
    source: impl Error + Send + Sync + 'static,
) -> ScaffoldError {
    ScaffoldError {
        diagnostic: CliError::scaffolding(message, source),
    }
}

struct StagingGuard {
    staging_directory: PathBuf,
    armed: bool,
}

impl StagingGuard {
    fn new(staging_directory: PathBuf) -> Self {
        Self {
            staging_directory,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.staging_directory);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io, path::Path};

    use tempfile::tempdir;

    use crate::scaffold::render_project;

    use super::{
        ProjectName, publish_project_with_operations, publish_without_replacement, sync_directory,
        write_new_file,
    };

    #[test]
    fn write_failure_removes_only_the_owned_staging_directory() {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let sibling = invocation.path().join("keep-me.txt");
        fs::write(&sibling, "preserved").expect("unrelated sibling should be written");
        let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
        let rendered = render_project(&name).expect("bundled templates should render");

        let error = publish_project_with_operations(
            invocation.path(),
            &name,
            &rendered,
            |path, contents| {
                if path
                    .file_name()
                    .is_some_and(|file_name| file_name == "mads.toml")
                {
                    return Err(io::Error::other("simulated generated-file write failure"));
                }
                write_new_file(path, contents)
            },
            |_| Ok(()),
            sync_directory,
            publish_without_replacement,
        )
        .expect_err("an injected file-write failure must abort publication");

        assert_eq!(error.code(), "MADS230");
        assert!(std::error::Error::source(&error).is_some());
        assert!(
            !error
                .to_string()
                .contains(&invocation.path().display().to_string())
        );
        assert!(!invocation.path().join("minimal-app").exists());
        assert_eq!(fs::read_to_string(sibling).unwrap(), "preserved");
        assert_no_staging_sibling(invocation.path());
    }

    #[test]
    fn staging_sync_failure_removes_the_private_staging_directory() {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
        let rendered = render_project(&name).expect("bundled templates should render");

        let error = publish_project_with_operations(
            invocation.path(),
            &name,
            &rendered,
            write_new_file,
            |_| Ok(()),
            |_| {
                Err(io::Error::other(
                    "simulated staging synchronization failure",
                ))
            },
            publish_without_replacement,
        )
        .expect_err("an injected staging-sync failure must abort publication");

        assert_eq!(error.code(), "MADS230");
        assert!(!invocation.path().join("minimal-app").exists());
        assert_no_staging_sibling(invocation.path());
    }

    #[test]
    fn preparation_failure_removes_the_private_staging_directory() {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
        let rendered = render_project(&name).expect("bundled templates should render");

        let error = publish_project_with_operations(
            invocation.path(),
            &name,
            &rendered,
            write_new_file,
            |_| {
                Err(io::Error::other(
                    "simulated publication-preparation failure",
                ))
            },
            sync_directory,
            publish_without_replacement,
        )
        .expect_err("an injected preparation failure must abort publication");

        assert_eq!(error.code(), "MADS230");
        assert!(!invocation.path().join("minimal-app").exists());
        assert_no_staging_sibling(invocation.path());
    }

    #[test]
    fn destination_race_preserves_the_competing_directory_and_cleans_staging() {
        let invocation = tempdir().expect("temporary invocation directory should be created");
        let name = ProjectName::parse("minimal-app").expect("fixture name should be valid");
        let rendered = render_project(&name).expect("bundled templates should render");
        let destination = invocation.path().join(name.as_str());

        let error = publish_project_with_operations(
            invocation.path(),
            &name,
            &rendered,
            write_new_file,
            |path| {
                fs::create_dir(path)?;
                fs::write(path.join("sentinel"), "preserved")
            },
            sync_directory,
            publish_without_replacement,
        )
        .expect_err("a destination created before no-replace publication must win the race");

        assert_eq!(error.code(), "MADS230");
        assert_eq!(
            fs::read_to_string(destination.join("sentinel")).unwrap(),
            "preserved"
        );
        assert_no_staging_sibling(invocation.path());
    }

    #[cfg(windows)]
    #[test]
    fn directory_sync_is_a_safe_noop_on_windows() {
        let directory = tempdir().expect("temporary directory should be created");

        sync_directory(directory.path())
            .expect("Windows directory synchronization must not reject a staging directory");
    }

    #[cfg(not(windows))]
    #[test]
    fn directory_sync_propagates_supported_platform_errors() {
        let directory = tempdir().expect("temporary directory should be created");
        let missing_directory = directory.path().join("missing");

        let error = sync_directory(&missing_directory)
            .expect_err("supported platforms must report directory-sync failures");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    fn assert_no_staging_sibling(invocation: &Path) {
        assert!(
            fs::read_dir(invocation)
                .expect("invocation directory should be readable")
                .filter_map(Result::ok)
                .all(|entry| {
                    !entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".minimal-app.mads-")
                }),
            "private staging directories must be removed after a failed publication"
        );
    }
}
