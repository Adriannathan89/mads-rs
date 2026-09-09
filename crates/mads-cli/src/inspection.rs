use std::{
    fs,
    path::Path,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use mads_common::__private::{
    DoctorStatus as InspectionDoctorStatus, INSPECTION_ACK_ENV, INSPECTION_KIND_ENV,
    INSPECTION_PROTOCOL_VERSION, INSPECTION_RESPONSE_ENV, INSPECTION_TOKEN_ENV,
    INSPECTION_VERSION_ENV, InspectionEnvelope, InspectionKind, InspectionReport, SourceReport,
};
use serde::Deserialize;
use tokio::{process::Child, time::Instant};

use crate::{
    cargo::BuiltApplication,
    diagnostic::CliError,
    output::{
        HumanOutput, Outcome,
        model::{
            CliDiagnostic, CommandData, DependencyData, DoctorCheckData, DoctorData, DoctorStatus,
            Envelope, GraphData, ImportData, ModuleData, ProviderData, RouteData, RoutesData,
            SourceLocation,
        },
        path::normalize_path,
    },
    render,
};

/// Stable diagnostic code for private application-inspection failures.
pub(crate) const MADS203: &str = "MADS203";

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const REPORT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy)]
struct InspectionTimeouts {
    handshake: Duration,
    report: Duration,
    poll: Duration,
}

#[derive(Clone, Copy)]
enum InspectionStreams {
    Inherit,
    Suppress,
}

impl InspectionTimeouts {
    const fn production() -> Self {
        Self {
            handshake: HANDSHAKE_TIMEOUT,
            report: REPORT_TIMEOUT,
            poll: POLL_INTERVAL,
        }
    }

    #[cfg(test)]
    const fn for_test() -> Self {
        Self {
            handshake: Duration::from_millis(100),
            report: Duration::from_millis(100),
            poll: Duration::from_millis(5),
        }
    }
}

#[derive(Deserialize)]
struct InspectionAcknowledgement {
    protocol_version: u32,
    token: String,
}

/// Runs the selected binary in its private, short-lived inspection mode.
pub(crate) async fn inspect_application(
    built: &BuiltApplication,
    kind: InspectionKind,
) -> Result<InspectionReport, CliError> {
    inspect_application_with_timeouts_and_streams(
        built,
        kind,
        InspectionTimeouts::production(),
        InspectionStreams::Inherit,
    )
    .await
}

/// Runs private inspection without allowing application output to enter JSON stdout or stderr.
pub(crate) async fn inspect_application_silently(
    built: &BuiltApplication,
    kind: InspectionKind,
) -> Result<InspectionReport, CliError> {
    inspect_application_with_timeouts_and_streams(
        built,
        kind,
        InspectionTimeouts::production(),
        InspectionStreams::Suppress,
    )
    .await
}

/// Converts one private inspection report into the shared finite-command outcome.
pub(crate) fn inspection_outcome(report: &InspectionReport, package_root: &Path) -> Outcome {
    let command = inspection_command_name(report.kind);
    let data = inspection_data(report, package_root);
    let diagnostics = inspection_diagnostics(report, package_root);
    let body = match report.kind {
        InspectionKind::Routes => render::render_routes(report),
        InspectionKind::Graph => render::render_graph(report),
        InspectionKind::Doctor => render::render_doctor(report),
    };
    let human_diagnostics = render::render_diagnostics(report);
    let human = HumanOutput::streams(
        format!("{body}\n"),
        if human_diagnostics.is_empty() {
            String::new()
        } else {
            format!("{human_diagnostics}\n")
        },
    );
    let envelope = if report.failed {
        Envelope::failure(Some(command.into()), Some(data), diagnostics)
    } else {
        Envelope::success_with_diagnostics(command, data, diagnostics)
    };

    Outcome::new(envelope, human)
}

/// Converts an inspection failure before a report exists into the shared outcome.
pub(crate) fn inspection_failure_outcome(kind: InspectionKind, error: &CliError) -> Outcome {
    Outcome::new(
        Envelope::failure(
            Some(inspection_command_name(kind).into()),
            None,
            vec![CliDiagnostic::from_error(error, None)],
        ),
        HumanOutput::stderr(format!("{error}\n")),
    )
}

fn inspection_data(report: &InspectionReport, package_root: &Path) -> CommandData {
    match report.kind {
        InspectionKind::Routes => CommandData::Routes(RoutesData::new(
            render::ordered_routes(report)
                .into_iter()
                .map(|route| {
                    RouteData::new(
                        &route.method,
                        &route.path,
                        &route.route_trait,
                        &route.handler,
                        &route.controller,
                        source_location(&route.location, package_root),
                        route.guard_active,
                    )
                })
                .collect(),
        )),
        InspectionKind::Graph => CommandData::Graph(GraphData::new(
            report.graph.root_module.clone(),
            render::ordered_modules(report)
                .into_iter()
                .map(|module| {
                    ModuleData::new(
                        &module.type_name,
                        &module.namespace,
                        source_location(&module.location, package_root),
                    )
                })
                .collect(),
            render::ordered_imports(report)
                .into_iter()
                .map(|import| ImportData::new(&import.importer, &import.imported))
                .collect(),
            render::ordered_providers(report)
                .into_iter()
                .map(|provider| {
                    ProviderData::new(
                        &provider.type_name,
                        provider.owner.clone(),
                        &provider.origin,
                        &provider.visibility,
                        &provider.state,
                        provider
                            .location
                            .as_ref()
                            .map(|location| source_location(location, package_root)),
                    )
                })
                .collect(),
            render::ordered_dependencies(report)
                .into_iter()
                .map(|dependency| DependencyData::new(&dependency.provider, &dependency.dependency))
                .collect(),
            report.graph.construction_order.clone(),
        )),
        InspectionKind::Doctor => CommandData::Doctor(DoctorData::new(
            render::ordered_checks(report)
                .into_iter()
                .map(|check| {
                    DoctorCheckData::new(&check.group, doctor_status(check.status), &check.summary)
                })
                .collect(),
        )),
    }
}

fn inspection_diagnostics(report: &InspectionReport, package_root: &Path) -> Vec<CliDiagnostic> {
    render::ordered_diagnostics(report)
        .into_iter()
        .map(|diagnostic| {
            let mut output =
                CliDiagnostic::error(&diagnostic.code, &diagnostic.title, &diagnostic.message);
            if let Some(subject) = &diagnostic.subject {
                output = output.with_subject(subject);
            }
            if let Some(location) = &diagnostic.location {
                output = output.with_location(source_location(location, package_root));
            }
            for suggestion in &diagnostic.suggestions {
                output = output.with_suggestion(suggestion);
            }
            output
        })
        .collect()
}

fn source_location(source: &SourceReport, package_root: &Path) -> SourceLocation {
    SourceLocation::new(
        normalize_path(package_root, Path::new(&source.file)),
        source.line,
        source.column,
    )
}

const fn doctor_status(status: InspectionDoctorStatus) -> DoctorStatus {
    match status {
        InspectionDoctorStatus::Pass => DoctorStatus::Pass,
        InspectionDoctorStatus::Skipped => DoctorStatus::Skipped,
        InspectionDoctorStatus::Overridden => DoctorStatus::Overridden,
        InspectionDoctorStatus::Failed => DoctorStatus::Failed,
    }
}

const fn inspection_command_name(kind: InspectionKind) -> &'static str {
    match kind {
        InspectionKind::Routes => "routes",
        InspectionKind::Graph => "graph",
        InspectionKind::Doctor => "doctor",
    }
}

#[cfg(test)]
async fn inspect_application_with_timeouts(
    built: &BuiltApplication,
    kind: InspectionKind,
    timeouts: InspectionTimeouts,
) -> Result<InspectionReport, CliError> {
    inspect_application_with_timeouts_and_streams(built, kind, timeouts, InspectionStreams::Inherit)
        .await
}

async fn inspect_application_with_timeouts_and_streams(
    built: &BuiltApplication,
    kind: InspectionKind,
    timeouts: InspectionTimeouts,
    streams: InspectionStreams,
) -> Result<InspectionReport, CliError> {
    ensure_supported_mads_version(built)?;

    let directory = tempfile::tempdir().map_err(|error| {
        inspection_error("could not prepare private inspection transport").with_source(error)
    })?;
    let token = inspection_token()?;
    let acknowledgement = directory.path().join("acknowledgement.json");
    let response = directory.path().join("response.json");
    let mut child = inspection_command(built, kind, &token, &acknowledgement, &response, streams)
        .spawn()
        .map_err(|error| {
            inspection_error("could not start the selected application for inspection")
                .with_source(error)
        })?;

    let result = supervise(
        &mut child,
        kind,
        &token,
        &acknowledgement,
        &response,
        timeouts,
    )
    .await;
    if result.is_err() {
        terminate_child(&mut child).await;
    }
    result
}

fn ensure_supported_mads_version(built: &BuiltApplication) -> Result<(), CliError> {
    let version = built.target().mads_version();
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CLI package version should be valid semver");
    if matches!(version, Some(version) if version.major == current.major && version.minor == current.minor)
    {
        return Ok(());
    }

    let found = version
        .map(ToString::to_string)
        .unwrap_or_else(|| "no direct mads dependency".into());
    let supported = format!("{}.{}", current.major, current.minor);
    Err(inspection_error(format!(
        "the selected application uses {found}; private inspection requires a direct MADS {supported} dependency"
    ))
    .with_suggestion(format!("use MADS {supported} in the selected application")))
}

fn inspection_token() -> Result<String, CliError> {
    let nanoseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            inspection_error("could not create an inspection correlation token").with_source(error)
        })?
        .as_nanos();
    Ok(format!("{}-{nanoseconds}", std::process::id()))
}

fn inspection_command(
    built: &BuiltApplication,
    kind: InspectionKind,
    token: &str,
    acknowledgement: &Path,
    response: &Path,
    streams: InspectionStreams,
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(built.executable());
    command
        .current_dir(built.target().package().package_root())
        .kill_on_drop(true)
        .env(
            INSPECTION_VERSION_ENV,
            INSPECTION_PROTOCOL_VERSION.to_string(),
        )
        .env(INSPECTION_KIND_ENV, inspection_kind_name(kind))
        .env(INSPECTION_TOKEN_ENV, token)
        .env(INSPECTION_ACK_ENV, acknowledgement)
        .env(INSPECTION_RESPONSE_ENV, response);
    match streams {
        InspectionStreams::Inherit => {
            command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }
        InspectionStreams::Suppress => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
    }
    command
}

const fn inspection_kind_name(kind: InspectionKind) -> &'static str {
    match kind {
        InspectionKind::Routes => "routes",
        InspectionKind::Graph => "graph",
        InspectionKind::Doctor => "doctor",
    }
}

async fn supervise(
    child: &mut Child,
    kind: InspectionKind,
    token: &str,
    acknowledgement: &Path,
    response: &Path,
    timeouts: InspectionTimeouts,
) -> Result<InspectionReport, CliError> {
    let handshake_deadline = Instant::now() + timeouts.handshake;
    loop {
        if acknowledgement.exists() {
            let acknowledgement = read_json::<InspectionAcknowledgement>(acknowledgement)?;
            if acknowledgement.protocol_version != INSPECTION_PROTOCOL_VERSION
                || acknowledgement.token != token
            {
                return Err(inspection_error(
                    "the application returned an invalid inspection acknowledgement",
                ));
            }
            break;
        }
        ensure_child_is_running(child)?;
        if Instant::now() >= handshake_deadline {
            return Err(inspection_error(
                "the application did not acknowledge the inspection request in time; private inspection requires the standard Mads::run::<AppModule>() entry point",
            ));
        }
        tokio::time::sleep(timeouts.poll).await;
    }

    let report_deadline = Instant::now() + timeouts.report;
    let envelope = loop {
        if response.exists() {
            let envelope = read_json::<InspectionEnvelope>(response)?;
            if envelope.protocol_version() != INSPECTION_PROTOCOL_VERSION
                || envelope.token() != token
                || envelope.report().kind != kind
            {
                return Err(inspection_error(
                    "the application returned an invalid inspection report",
                ));
            }
            break envelope;
        }
        ensure_child_is_running(child)?;
        if Instant::now() >= report_deadline {
            return Err(inspection_error(
                "the application did not return an inspection report in time",
            ));
        }
        tokio::time::sleep(timeouts.poll).await;
    };

    wait_for_successful_exit(child, report_deadline, timeouts.poll).await?;
    Ok(envelope.into_report())
}

fn ensure_child_is_running(child: &mut Child) -> Result<(), CliError> {
    match child.try_wait().map_err(|error| {
        inspection_error("could not observe the inspection application").with_source(error)
    })? {
        Some(status) => Err(inspection_error(format!(
            "the application exited before completing private inspection ({status}); private inspection requires the standard Mads::run::<AppModule>() entry point"
        ))),
        None => Ok(()),
    }
}

async fn wait_for_successful_exit(
    child: &mut Child,
    deadline: Instant,
    poll: Duration,
) -> Result<(), CliError> {
    loop {
        match child.try_wait().map_err(|error| {
            inspection_error("could not observe the inspection application").with_source(error)
        })? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                return Err(inspection_error(format!(
                    "the application exited unsuccessfully after private inspection ({status})"
                )));
            }
            None if Instant::now() >= deadline => {
                return Err(inspection_error(
                    "the application did not exit after private inspection in time",
                ));
            }
            None => tokio::time::sleep(poll).await,
        }
    }
}

fn read_json<T>(path: &Path) -> Result<T, CliError>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path).map_err(|error| {
        inspection_error("could not read private inspection output").with_source(error)
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        inspection_error("the application returned malformed private inspection output")
            .with_source(error)
    })
}

fn inspection_error(message: impl Into<String>) -> CliError {
    CliError::new(MADS203, "Application inspection failed", message)
}

async fn terminate_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill().await;
    }
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use mads_common::__private::InspectionKind;

    use crate::{cargo::build_application, command::TargetSelection, project::CargoProject};

    use super::{InspectionTimeouts, MADS203, inspect_application_with_timeouts};

    #[tokio::test]
    async fn accepts_matching_acknowledgement_and_report_from_current_mads_version() {
        let application = fixture_application("success").await;
        let report = inspect_application_with_timeouts(
            &application,
            InspectionKind::Routes,
            InspectionTimeouts::for_test(),
        )
        .await
        .expect("matching protocol fixture should succeed");

        assert_eq!(report.kind, InspectionKind::Routes);
    }

    #[tokio::test]
    async fn rejects_a_wrong_acknowledgement_token() {
        assert_protocol_failure("wrong_token").await;
    }

    #[tokio::test]
    async fn rejects_a_wrong_protocol_version() {
        assert_protocol_failure("wrong_version").await;
    }

    #[tokio::test]
    async fn rejects_malformed_report_data() {
        assert_protocol_failure("malformed").await;
    }

    #[tokio::test]
    async fn rejects_a_child_that_exits_before_acknowledging() {
        assert_protocol_failure("early_exit").await;
    }

    #[tokio::test]
    async fn terminates_a_child_that_acknowledges_without_reporting() {
        assert_protocol_failure("timeout").await;
    }

    async fn assert_protocol_failure(binary: &str) {
        let application = fixture_application(binary).await;
        let error = inspect_application_with_timeouts(
            &application,
            InspectionKind::Doctor,
            InspectionTimeouts::for_test(),
        )
        .await
        .expect_err("invalid protocol fixture should fail");

        assert_eq!(error.code(), MADS203);
    }

    async fn fixture_application(binary: &str) -> crate::cargo::BuiltApplication {
        let project = CargoProject::load(fixture_root()).expect("fixture project should load");
        let target = project
            .resolve_application(&TargetSelection {
                package: None,
                binary: Some(binary.into()),
            })
            .expect("fixture binary should resolve");
        build_application(&target)
            .await
            .expect("fixture binary should build")
    }

    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inspection/protocol")
    }
}
