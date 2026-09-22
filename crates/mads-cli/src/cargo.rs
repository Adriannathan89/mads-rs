use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use cargo_metadata::{Artifact, Message};
use tokio::{
    io::{AsyncBufReadExt, BufReader, Lines},
    process::{Child, ChildStdout},
};

use crate::{
    diagnostic::{CliError, MADS201},
    project::ResolvedApplication,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuiltApplication {
    target: ResolvedApplication,
    executable: PathBuf,
}

pub(crate) fn cargo_build_command(target: &ResolvedApplication) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("cargo");
    command
        .arg("build")
        .arg("--message-format=json-render-diagnostics")
        .arg("--package")
        .arg(target.package().package_name())
        .arg("--bin")
        .arg(target.binary_name())
        .current_dir(target.package().workspace_root())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    command
}

pub(crate) async fn build_application(
    target: &ResolvedApplication,
) -> Result<BuiltApplication, CliError> {
    build_application_owned(target.clone()).await
}

pub(crate) async fn build_application_owned(
    target: ResolvedApplication,
) -> Result<BuiltApplication, CliError> {
    let mut build = CargoBuild::start(target)?;
    build.wait().await
}

pub(crate) struct CargoBuild {
    target: ResolvedApplication,
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
    collector: ArtifactCollector,
}

impl CargoBuild {
    pub(crate) fn start(target: ResolvedApplication) -> Result<Self, CliError> {
        let mut child = cargo_build_command(&target).spawn().map_err(|error| {
            CliError::new(
                MADS201,
                "Cargo build failed",
                "could not start Cargo for the selected application",
            )
            .with_source(error)
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CliError::new(
                MADS201,
                "Cargo build failed",
                "could not read Cargo build output",
            )
        })?;

        Ok(Self {
            target,
            child,
            lines: BufReader::new(stdout).lines(),
            collector: ArtifactCollector::default(),
        })
    }

    pub(crate) async fn wait(&mut self) -> Result<BuiltApplication, CliError> {
        while let Some(line) = self.lines.next_line().await.map_err(|error| {
            CliError::new(
                MADS201,
                "Cargo build failed",
                "could not read Cargo build output",
            )
            .with_source(error)
        })? {
            let message = serde_json::from_str::<Message>(&line).map_err(|error| {
                CliError::new(
                    MADS201,
                    "Cargo build failed",
                    "Cargo produced malformed build output",
                )
                .with_source(error)
            })?;

            if let Message::CompilerMessage(message) = &message
                && let Some(rendered) = &message.message.rendered
            {
                eprint!("{rendered}");
            }

            self.collector.process(&self.target, message);
        }

        let status = self.child.wait().await.map_err(|error| {
            CliError::new(
                MADS201,
                "Cargo build failed",
                "could not wait for Cargo to finish",
            )
            .with_source(error)
        })?;
        if !status.success() {
            return Err(CliError::new(
                MADS201,
                "Cargo build failed",
                "Cargo could not build the selected application",
            ));
        }

        std::mem::take(&mut self.collector).finish(self.target.clone())
    }

    pub(crate) async fn cancel_and_wait(&mut self) -> Result<(), CliError> {
        let kill_result = self.child.start_kill();
        let wait_result = self.child.wait().await;

        match (kill_result, wait_result) {
            (Ok(()), Ok(_)) => Ok(()),
            (_, Err(error)) => Err(CliError::new(
                MADS201,
                "Cargo build failed",
                "could not wait for the cancelled Cargo build",
            )
            .with_source(error)),
            (Err(error), Ok(_)) => Err(CliError::new(
                MADS201,
                "Cargo build failed",
                "could not cancel Cargo build",
            )
            .with_source(error)),
        }
    }
}

impl BuiltApplication {
    pub(crate) fn target(&self) -> &ResolvedApplication {
        &self.target
    }

    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    #[cfg(test)]
    pub(crate) fn test_identity(binary_name: &str) -> Self {
        Self {
            target: ResolvedApplication::test_identity(binary_name),
            executable: PathBuf::from(format!("/tmp/mads-dev-state/{binary_name}")),
        }
    }
}

fn collect_artifact(
    target: &ResolvedApplication,
    messages: impl IntoIterator<Item = Result<Message, serde_json::Error>>,
) -> Result<BuiltApplication, CliError> {
    let mut collector = ArtifactCollector::default();

    for message in messages {
        let message = message.map_err(|error| {
            CliError::new(
                MADS201,
                "Cargo build failed",
                "Cargo produced malformed build output",
            )
            .with_source(error)
        })?;
        collector.process(target, message);
    }

    collector.finish(target.clone())
}

#[derive(Default)]
struct ArtifactCollector {
    executable: Option<PathBuf>,
    successful: bool,
}

impl ArtifactCollector {
    fn process(&mut self, target: &ResolvedApplication, message: Message) {
        match message {
            Message::CompilerArtifact(artifact) => {
                if let Some(path) = matching_executable(target, &artifact) {
                    self.executable = Some(path);
                }
            }
            Message::BuildFinished(finished) => self.successful = finished.success,
            _ => {}
        }
    }

    fn finish(self, target: ResolvedApplication) -> Result<BuiltApplication, CliError> {
        if !self.successful {
            return Err(CliError::new(
                MADS201,
                "Cargo build failed",
                "Cargo did not report a successful build",
            ));
        }

        let executable = self.executable.ok_or_else(|| {
            CliError::new(
                MADS201,
                "Cargo build failed",
                "Cargo did not produce the selected application binary",
            )
        })?;

        Ok(BuiltApplication { target, executable })
    }
}

fn matching_executable(target: &ResolvedApplication, artifact: &Artifact) -> Option<PathBuf> {
    if artifact.package_id == *target.package().package_id()
        && artifact.target.name == target.binary_name()
    {
        artifact
            .executable
            .as_ref()
            .map(|path| path.as_std_path().to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use cargo_metadata::Message;

    use crate::{
        command::TargetSelection,
        diagnostic::MADS201,
        project::{CargoProject, ResolvedApplication},
    };

    use super::{ArtifactCollector, collect_artifact};

    #[test]
    fn artifact_collection_processes_messages_incrementally() {
        let selected = selected_fixture("api", "server");
        let mut collector = ArtifactCollector::default();

        collector.process(
            &selected,
            serde_json::from_str(&worker_artifact(&selected)).unwrap(),
        );
        collector.process(
            &selected,
            serde_json::from_str(&server_artifact(&selected, "/tmp/target/debug/server")).unwrap(),
        );
        collector.process(
            &selected,
            serde_json::from_str(&build_finished(true)).unwrap(),
        );

        let artifact = collector.finish(selected).unwrap();
        assert_eq!(artifact.executable(), Path::new("/tmp/target/debug/server"));
    }

    #[test]
    fn selects_only_the_requested_executable_artifact() {
        let selected = selected_fixture("api", "server");
        let artifact = collect_artifact(
            &selected,
            messages(&[
                worker_artifact(&selected),
                server_artifact(&selected, "/tmp/target/debug/server"),
                foreign_server_artifact(),
                non_executable_server_artifact(&selected),
                build_finished(true),
            ]),
        )
        .unwrap();

        assert_eq!(artifact.executable(), Path::new("/tmp/target/debug/server"));
    }

    #[test]
    fn successful_build_without_the_requested_artifact_is_mads201() {
        let selected = selected_fixture("api", "server");
        let error = collect_artifact(
            &selected,
            messages(&[
                worker_artifact(&selected),
                non_executable_server_artifact(&selected),
                build_finished(true),
            ]),
        )
        .unwrap_err();

        assert_eq!(error.code(), MADS201);
    }

    #[test]
    fn build_command_selects_the_exact_cargo_binary() {
        let selected = selected_fixture("api", "server");
        let command = super::cargo_build_command(&selected);
        let command = command.as_std();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(command.get_program(), "cargo");
        assert_eq!(
            arguments,
            [
                "build",
                "--message-format=json-render-diagnostics",
                "--package",
                "api",
                "--bin",
                "server",
            ]
        );
        assert_eq!(
            command.get_current_dir(),
            Some(selected.package().workspace_root())
        );
    }

    #[tokio::test]
    async fn builds_the_exact_single_application_artifact() {
        let project = CargoProject::load(fixture("single")).unwrap();
        let target = project
            .resolve_application(&TargetSelection::default())
            .unwrap();
        let built = super::build_application(&target).await.unwrap();

        assert!(built.executable().is_file());
        assert_eq!(built.target().binary_name(), "single-app");
    }

    fn selected_fixture(package: &str, binary: &str) -> ResolvedApplication {
        CargoProject::load(fixture("multiple"))
            .unwrap()
            .resolve_application(&TargetSelection {
                package: Some(package.into()),
                binary: Some(binary.into()),
            })
            .unwrap()
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/run")
            .join(name)
    }

    fn messages(lines: &[String]) -> impl Iterator<Item = Result<Message, serde_json::Error>> + '_ {
        lines.iter().map(|line| serde_json::from_str(line))
    }

    fn worker_artifact(selected: &ResolvedApplication) -> String {
        artifact(selected, "worker", Some("/tmp/target/debug/worker"))
    }

    fn non_executable_server_artifact(selected: &ResolvedApplication) -> String {
        artifact(selected, "server", None)
    }

    fn server_artifact(selected: &ResolvedApplication, executable: &str) -> String {
        artifact(selected, "server", Some(executable))
    }

    fn foreign_server_artifact() -> String {
        let foreign = selected_fixture("web", "web");
        artifact(&foreign, "server", Some("/tmp/target/debug/foreign-server"))
    }

    fn artifact(
        selected: &ResolvedApplication,
        target_name: &str,
        executable: Option<&str>,
    ) -> String {
        serde_json::json!({
            "reason": "compiler-artifact",
            "package_id": selected.package().package_id().to_string(),
            "manifest_path": selected.package().manifest_path(),
            "target": {
                "kind": ["bin"],
                "crate_types": ["bin"],
                "name": target_name,
                "src_path": "/tmp/src/main.rs",
                "edition": "2024",
                "doc": false,
                "doctest": false,
                "test": true
            },
            "profile": {
                "opt_level": "0",
                "debuginfo": 0,
                "debug_assertions": true,
                "overflow_checks": true,
                "test": false
            },
            "features": [],
            "filenames": ["/tmp/target/debug/example"],
            "executable": executable,
            "fresh": false
        })
        .to_string()
    }

    fn build_finished(success: bool) -> String {
        serde_json::json!({
            "reason": "build-finished",
            "success": success,
        })
        .to_string()
    }
}
