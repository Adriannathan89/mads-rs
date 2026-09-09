use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use mads_common::__private::DEV_SHUTDOWN_ENV;
use tokio::process::Command;

use crate::{
    cargo::BuiltApplication,
    diagnostic::{CliError, MADS202},
};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StopOutcome {
    Graceful,
    Forced,
}

#[allow(dead_code)]
pub(crate) struct ApplicationProcess {
    child: tokio::process::Child,
    control_directory: tempfile::TempDir,
    shutdown_path: PathBuf,
}

#[allow(dead_code)]
pub(crate) async fn spawn_dev_application(
    built: &BuiltApplication,
    arguments: &[OsString],
) -> Result<ApplicationProcess, CliError> {
    spawn_dev_application_from_parts(
        built.executable(),
        built.target().package().package_root(),
        arguments,
    )
    .await
}

async fn spawn_dev_application_from_parts(
    executable: &Path,
    package_root: &Path,
    arguments: &[OsString],
) -> Result<ApplicationProcess, CliError> {
    let control_directory = tempfile::tempdir().map_err(|error| {
        process_error(
            "could not create application process control directory",
            error,
        )
    })?;
    let shutdown_path = control_directory.path().join("shutdown");
    // Windows locks a running executable, so Cargo cannot replace the build
    // artifact during the next dev-loop rebuild. Run an owned shadow copy.
    let mut shadow_executable = control_directory.path().join("application");
    if let Some(extension) = executable.extension() {
        shadow_executable.set_extension(extension);
    }
    tokio::fs::copy(executable, &shadow_executable)
        .await
        .map_err(|error| {
            process_error(
                "could not prepare the selected application for development",
                error,
            )
        })?;
    let mut command = Command::new(shadow_executable);
    command
        .args(arguments)
        .current_dir(package_root)
        .env(DEV_SHUTDOWN_ENV, &shutdown_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| process_error("could not start the selected application", error))?;

    Ok(ApplicationProcess {
        child,
        control_directory,
        shutdown_path,
    })
}

#[allow(dead_code)]
impl ApplicationProcess {
    pub(crate) fn try_wait(&mut self) -> Result<Option<ExitStatus>, CliError> {
        self.child
            .try_wait()
            .map_err(|error| process_error("could not inspect the selected application", error))
    }

    pub(crate) async fn stop(&mut self, timeout: Duration) -> Result<StopOutcome, CliError> {
        debug_assert_eq!(
            self.shutdown_path.parent(),
            Some(self.control_directory.path())
        );
        tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.shutdown_path)
            .await
            .map_err(|error| {
                process_error("could not request graceful application shutdown", error)
            })?;

        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(result) => {
                result.map_err(|error| {
                    process_error("could not wait for the selected application", error)
                })?;
                Ok(StopOutcome::Graceful)
            }
            Err(_) => {
                self.child.start_kill().map_err(|error| {
                    process_error("could not stop the selected application", error)
                })?;
                self.child.wait().await.map_err(|error| {
                    process_error("could not reap the selected application", error)
                })?;
                Ok(StopOutcome::Forced)
            }
        }
    }
}

fn process_error(message: &'static str, source: std::io::Error) -> CliError {
    CliError::new(MADS202, "Application process failed", message).with_source(source)
}

pub(crate) fn application_command(built: &BuiltApplication, arguments: &[OsString]) -> Command {
    let mut command = Command::new(built.executable());
    command
        .args(arguments)
        .current_dir(built.target().package().package_root())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    command
}

pub(crate) async fn run_application(
    built: &BuiltApplication,
    arguments: &[OsString],
) -> Result<std::process::ExitStatus, CliError> {
    let mut child = application_command(built, arguments)
        .spawn()
        .map_err(|error| {
            CliError::new(
                MADS202,
                "Application process failed",
                "could not start the selected application",
            )
            .with_source(error)
        })?;

    child.wait().await.map_err(|error| {
        CliError::new(
            MADS202,
            "Application process failed",
            "could not wait for the selected application",
        )
        .with_source(error)
    })
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        time::Duration,
    };

    use mads_common::__private::DEV_SHUTDOWN_ENV;

    use super::{StopOutcome, spawn_dev_application_from_parts};

    const GRACEFUL_FIXTURE: &str = "process::tests::graceful_fixture_child";
    const FORCED_FIXTURE: &str = "process::tests::forced_fixture_child";

    #[tokio::test]
    async fn graceful_stop_notifies_and_reaps_the_child() {
        let executable = std::env::current_exe().unwrap();
        let root = tempfile::tempdir().unwrap();
        let arguments = fixture_arguments(GRACEFUL_FIXTURE);
        let mut process = spawn_dev_application_from_parts(&executable, root.path(), &arguments)
            .await
            .unwrap();
        let ready = process.control_directory.path().join("ready");
        let completion = process.control_directory.path().join("completion");

        wait_for_file(&ready, Duration::from_secs(2)).await.unwrap();
        assert_eq!(
            process.stop(Duration::from_secs(2)).await.unwrap(),
            StopOutcome::Graceful
        );
        assert_eq!(fs::read_to_string(completion).unwrap(), "graceful");
        assert!(process.try_wait().unwrap().is_some());
    }

    #[tokio::test]
    async fn timed_out_stop_forcibly_kills_and_reaps_the_child() {
        let executable = std::env::current_exe().unwrap();
        let root = tempfile::tempdir().unwrap();
        let arguments = fixture_arguments(FORCED_FIXTURE);
        let mut process = spawn_dev_application_from_parts(&executable, root.path(), &arguments)
            .await
            .unwrap();
        let ready = process.control_directory.path().join("ready");

        wait_for_file(&ready, Duration::from_secs(2)).await.unwrap();
        assert_eq!(
            process.stop(Duration::from_millis(100)).await.unwrap(),
            StopOutcome::Forced
        );
        assert!(process.try_wait().unwrap().is_some());
    }

    #[test]
    fn graceful_fixture_child() {
        let Some(shutdown) = std::env::var_os(DEV_SHUTDOWN_ENV).map(PathBuf::from) else {
            return;
        };
        let control = shutdown.parent().unwrap();
        fs::write(control.join("ready"), std::process::id().to_string()).unwrap();
        while !shutdown.is_file() {
            std::thread::sleep(Duration::from_millis(10));
        }
        fs::write(control.join("completion"), "graceful").unwrap();
    }

    #[test]
    fn forced_fixture_child() {
        let Some(shutdown) = std::env::var_os(DEV_SHUTDOWN_ENV).map(PathBuf::from) else {
            return;
        };
        fs::write(
            shutdown.parent().unwrap().join("ready"),
            std::process::id().to_string(),
        )
        .unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn fixture_arguments(name: &str) -> Vec<OsString> {
        vec!["--exact".into(), name.into(), "--nocapture".into()]
    }

    async fn wait_for_file(path: &Path, timeout: Duration) -> Result<(), &'static str> {
        tokio::time::timeout(timeout, async {
            while !path.is_file() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| "fixture did not become ready")
    }
}
