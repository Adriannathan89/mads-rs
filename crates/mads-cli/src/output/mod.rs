//! Renderer selection for MADS-owned finite-command outcomes.

mod human;
pub mod json;
pub mod model;
pub mod path;

use crate::{
    command::{CanonicalCommand, OutputFormat},
    diagnostic::{CliError, MADS204},
};

pub(crate) use self::human::HumanOutput;
use self::model::Envelope;

/// A finite-command outcome rendered once at the outer CLI boundary.
pub(crate) struct Outcome {
    envelope: Envelope,
    human: HumanOutput,
}

impl Outcome {
    /// Creates an outcome with schema data and the established human streams.
    pub(crate) fn new(envelope: Envelope, human: HumanOutput) -> Self {
        Self { envelope, human }
    }

    /// Creates the syntax-failure outcome shared by both renderer choices.
    pub(crate) fn syntax_failure(
        command: Option<String>,
        diagnostic: model::CliDiagnostic,
        human_stderr: String,
    ) -> Self {
        Self::new(
            Envelope::failure(command, None, vec![diagnostic]),
            HumanOutput::stderr(human_stderr),
        )
    }
}

/// Writes exactly one MADS-owned output outcome using the requested format.
pub(crate) fn write(format: OutputFormat, outcome: &Outcome) -> Result<(), CliError> {
    match format {
        OutputFormat::Human => human::write(&outcome.human).map_err(output_error),
        OutputFormat::Json => match json::write(&outcome.envelope) {
            Ok(()) => Ok(()),
            Err(json::OutputError::Serialize(_)) => {
                let fallback = renderer_failure_outcome(outcome.envelope.command());
                json::write(&fallback.envelope).map_err(output_error)?;
                Err(renderer_failure_error())
            }
            Err(error) => Err(output_error(error)),
        },
    }
}

/// Writes a human-only stream through the same CLI output boundary.
pub(crate) fn write_human(
    stdout: impl Into<String>,
    stderr: impl Into<String>,
) -> Result<(), CliError> {
    human::write(&HumanOutput::streams(stdout, stderr)).map_err(output_error)
}

/// Returns the canonical schema spelling for a finite command.
pub(crate) const fn command_name(command: CanonicalCommand) -> &'static str {
    match command {
        CanonicalCommand::Routes => "routes",
        CanonicalCommand::Graph => "graph",
        CanonicalCommand::Doctor => "doctor",
        CanonicalCommand::DatabaseGenerate => "db generate",
        CanonicalCommand::DatabaseMigrate => "db migrate",
        CanonicalCommand::DatabaseRollback => "db rollback",
        CanonicalCommand::DatabaseStatus => "db status",
        CanonicalCommand::DatabaseHelp => "db help",
    }
}

fn output_error(error: impl std::error::Error + Send + Sync + 'static) -> CliError {
    CliError::new(
        MADS204,
        "CLI output failed",
        "could not write machine-readable command output",
    )
    .with_source(error)
}

fn renderer_failure_outcome(command: Option<&str>) -> Outcome {
    let error = renderer_failure_error();
    Outcome::new(
        Envelope::failure(
            command.map(str::to_owned),
            None,
            vec![model::CliDiagnostic::from_error(&error, None)],
        ),
        HumanOutput::stderr(format!("{error}\n")),
    )
}

fn renderer_failure_error() -> CliError {
    CliError::new(
        MADS204,
        "CLI output failed",
        "could not render machine-readable command output",
    )
}

#[cfg(test)]
mod tests {
    use super::{MADS204, json, renderer_failure_error, renderer_failure_outcome};

    #[test]
    fn renderer_failure_is_an_operational_mads204_error() {
        let error = renderer_failure_error();

        assert_eq!(error.code(), MADS204);
        assert_eq!(error.title(), "CLI output failed");
        assert_eq!(
            error.message(),
            "could not render machine-readable command output"
        );
    }

    #[test]
    fn renderer_failure_is_a_safe_json_operational_diagnostic() {
        let output = json::render(&renderer_failure_outcome(Some("routes")).envelope)
            .expect("renderer failure envelope should serialize");

        assert_eq!(
            output,
            "{\"schema_version\":1,\"command\":\"routes\",\"ok\":false,\"data\":null,\"diagnostics\":[{\"severity\":\"error\",\"code\":\"MADS204\",\"title\":\"CLI output failed\",\"message\":\"could not render machine-readable command output\",\"subject\":null,\"location\":null,\"suggestions\":[]}]}\n"
        );
    }
}
