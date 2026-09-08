//! Renderer selection for MADS-owned finite-command outcomes.

mod human;
pub mod json;
pub mod model;
pub mod path;

use crate::{
    command::{CanonicalCommand, OutputFormat},
    diagnostic::{CliError, MADS204},
};

use self::{human::HumanOutput, model::Envelope};

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
        OutputFormat::Json => json::write(&outcome.envelope).map_err(output_error),
    }
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
