//! Development commands for running MADS.rs applications and managing migrations.
//!
//! The `mads` executable exposes the development command surface and
//! preserves application arguments supplied after `--`. CLI syntax failures
//! exit with 2; configuration, build, and operational failures exit with 1.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

#[allow(dead_code)]
mod cargo;
mod command;
mod database;
mod dev;
#[allow(dead_code)]
mod dev_state;
#[allow(dead_code)]
mod diagnostic;
mod inspection;
/// Versioned finite-command output records and serializers.
pub mod output;
mod process;
#[allow(dead_code)]
mod project;
mod render;
/// Project-name validation and bundled minimal-project templates.
pub mod scaffold;
#[allow(dead_code)]
mod watch;

use std::{ffi::OsString, io, path::PathBuf, process::ExitCode};

use command::{
    CanonicalCommand, Command, DatabaseCommand, DatabaseInvocation, InspectionCommand,
    OutputFormat, ParseFailure,
};
use dev::run_dev;
use diagnostic::{CliError, MADS201, MADS202};
use inspection::{inspect_application, inspect_application_silently};
use project::CargoProject;

/// Runs the MADS.rs CLI using the process arguments.
pub fn run() -> ExitCode {
    mads::core::runtime::block_on(run_with(
        std::env::args_os().skip(1).collect(),
        std::env::current_dir(),
    ))
}

async fn run_with(arguments: Vec<OsString>, current_dir: io::Result<PathBuf>) -> ExitCode {
    let invocation = match command::parse(&arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            let format = error.format.unwrap_or_default();
            if let Err(output_error) = render_parse_error(&error) {
                if format == OutputFormat::Human {
                    let _ = output::write_human(String::new(), format!("{output_error}\n"));
                }
                return ExitCode::from(1);
            }
            return ExitCode::from(2);
        }
    };

    let format = invocation.format;
    match run_command(invocation.command, format, current_dir).await {
        Ok(exit_code) => exit_code,
        Err(error) => {
            if format == OutputFormat::Human {
                let _ = output::write_human(String::new(), format!("{error}\n"));
            }
            ExitCode::from(1)
        }
    }
}

async fn run_command(
    command: Command,
    format: OutputFormat,
    current_dir: io::Result<PathBuf>,
) -> Result<ExitCode, CliError> {
    match command {
        Command::Help => {
            output::write_human(format!("{}\n", help()), String::new())?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Version => {
            output::write_human(
                format!("mads {}\n", env!("CARGO_PKG_VERSION")),
                String::new(),
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Run(command) => {
            let root = current_dir.map_err(current_directory_error)?;
            let project = CargoProject::load(root)?;
            let target = project.resolve_application(&command.target)?;
            let built = cargo::build_application(&target).await?;
            let status = process::run_application(&built, &command.arguments).await?;

            match status.code() {
                Some(code @ 0..=255) => Ok(ExitCode::from(code as u8)),
                _ => Err(CliError::new(
                    MADS202,
                    "Application process failed",
                    "the selected application terminated without an ordinary exit code",
                )),
            }
        }
        Command::Dev(command) => {
            let root = current_dir.map_err(current_directory_error)?;
            run_dev(command, &root).await
        }
        Command::Inspect(command) => run_inspection_command(command, format, current_dir).await,
        Command::Database(DatabaseInvocation {
            command: DatabaseCommand::Help,
            ..
        }) => {
            output::write_human(format!("{}\n", database_help()), String::new())?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Database(DatabaseInvocation { command, package }) => {
            run_database_command(command, package.as_deref(), format, current_dir).await
        }
    }
}

async fn run_inspection_command(
    command: InspectionCommand,
    format: OutputFormat,
    current_dir: io::Result<PathBuf>,
) -> Result<ExitCode, CliError> {
    let kind = command.kind;
    let result = async {
        let root = current_dir.map_err(current_directory_error)?;
        let project = CargoProject::load(root)?;
        let target = project.resolve_application(&command.target)?;
        let built = cargo::build_application(&target).await?;
        let package_root = built.target().package().package_root().to_path_buf();
        let report = match format {
            OutputFormat::Human => inspect_application(&built, kind).await?,
            OutputFormat::Json => inspect_application_silently(&built, kind).await?,
        };
        Ok::<_, CliError>((report, package_root))
    }
    .await;

    match result {
        Ok((report, package_root)) => {
            let exit_code = if report.failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
            let outcome = inspection::inspection_outcome(&report, &package_root);
            output::write(format, &outcome)?;
            Ok(exit_code)
        }
        Err(error) => {
            let outcome = inspection::inspection_failure_outcome(kind, &error);
            output::write(format, &outcome)?;
            Ok(ExitCode::from(1))
        }
    }
}

async fn run_database_command(
    command: DatabaseCommand,
    package: Option<&str>,
    format: OutputFormat,
    current_dir: io::Result<PathBuf>,
) -> Result<ExitCode, CliError> {
    let result = async {
        let root = current_dir
            .map_err(current_directory_error)
            .map_err(database::CliError::diagnostic)?;
        let project = CargoProject::load(root).map_err(database::CliError::diagnostic)?;
        let package = project
            .resolve_package(package)
            .map_err(database::CliError::diagnostic)?;
        database::execute(command, package.package_root()).await
    }
    .await;
    let exit_code = if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    let outcome = database::outcome(command, result);
    output::write(format, &outcome)?;
    Ok(exit_code)
}

fn current_directory_error(error: io::Error) -> CliError {
    CliError::new(
        MADS201,
        "Cargo project could not be loaded",
        "could not determine the invocation directory",
    )
    .with_source(error)
}

fn render_parse_error(failure: &ParseFailure) -> Result<(), CliError> {
    let diagnostic = CliError::syntax(&failure.error);
    let command = failure.command.map(output::command_name).map(str::to_owned);
    let human_stderr = format!(
        "error: {}\n{}\n",
        failure.error,
        if parse_error_needs_database_help(failure) {
            database_help()
        } else {
            help()
        }
    );
    let outcome = output::Outcome::syntax_failure(
        command,
        output::model::CliDiagnostic::from_error(&diagnostic, None),
        human_stderr,
    );
    output::write(failure.format.unwrap_or_default(), &outcome)
}

fn parse_error_needs_database_help(failure: &ParseFailure) -> bool {
    failure.error.is_database_command()
        || matches!(
            failure.command,
            Some(
                CanonicalCommand::DatabaseGenerate
                    | CanonicalCommand::DatabaseMigrate
                    | CanonicalCommand::DatabaseRollback
                    | CanonicalCommand::DatabaseStatus
                    | CanonicalCommand::DatabaseHelp
            )
        )
}

const fn help() -> &'static str {
    "Usage: mads <command> [options]\n\nCommands:\n  run       Build and run a MADS application\n  dev       Watch, rebuild, and restart a MADS application\n  routes    Inspect application routes\n  graph     Inspect the application graph\n  doctor    Diagnose application configuration and metadata\n  db        Manage PostgreSQL migrations\n\nApplication selection:\n  -p, --package <package>\n      --bin <binary>"
}

const fn database_help() -> &'static str {
    "Usage: mads db <command> [--package <package>]\n\nCommands:\n  generate  Generate one complete schema diff as <timestamp>_schema_diff\n  migrate   Apply pending migrations\n  rollback  Revert the latest applied migration\n  status    Show applied and pending migrations\n\nApplication selection:\n  -p, --package <package>"
}
