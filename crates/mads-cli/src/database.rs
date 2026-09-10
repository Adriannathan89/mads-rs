//! Database command execution for project-local Diesel migrations.

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use mads::{
    Database, DatabaseConfig, DatabaseError,
    core::{ConfigBuilder, DotenvSource, EnvSource, TomlSource},
    diesel_migrations::{FileBasedMigrations, MigrationError},
};

use crate::{
    command::DatabaseCommand,
    diagnostic::MADS210,
    output::{
        HumanOutput, Outcome,
        model::{
            CliDiagnostic, CommandData, DatabaseGenerateData, DatabaseMigrateData,
            DatabaseRollbackData, DatabaseStatusData, Envelope,
        },
    },
};

#[allow(dead_code)]
mod catalog;
#[allow(dead_code)]
mod diff;
mod publish;
#[allow(dead_code)]
mod schema;
#[allow(dead_code)]
mod sql;

use catalog::LiveSchema;
use diff::plan_diff;
use publish::{SystemMigrationClock, publish_migration};
use schema::DesiredSchema;
use sql::render_migration;

/// A database-enabled project whose migration source is loaded on demand.
pub(crate) struct LoadedDatabaseProject {
    root: PathBuf,
    database: DatabaseConfig,
}

impl LoadedDatabaseProject {
    /// Returns the selected package root.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Opens the configured database connection boundary.
    pub(crate) fn connect(&self) -> Result<Database, CliError> {
        Database::from_config(&self.database).map_err(CliError::database)
    }

    /// Loads the selected package's file-based migration source.
    pub(crate) fn migrations(&self) -> Result<FileBasedMigrations, CliError> {
        let path = self.root().join("migrations");
        FileBasedMigrations::from_path(&path).map_err(|error| {
            CliError::from_error(
                format!(
                    "migration directory `{}` could not be loaded",
                    path.display()
                ),
                error,
            )
        })
    }
}

/// A CLI-safe failure which retains its original cause for programmatic inspection.
pub(crate) struct CliError {
    message: String,
    source: Box<dyn Error>,
}

impl CliError {
    fn from_error(message: impl Into<String>, source: impl Error + 'static) -> Self {
        Self {
            message: message.into(),
            source: Box::new(source),
        }
    }

    fn database(error: DatabaseError) -> Self {
        if contains_no_migration(&error) {
            return Self::from_error("no migration is available to revert", error);
        }
        Self::from_error(error.to_string(), error)
    }

    pub(crate) fn diagnostic(error: crate::diagnostic::CliError) -> Self {
        Self::from_error(error.to_string(), error)
    }

    fn public_diagnostic(&self) -> CliDiagnostic {
        self.source
            .downcast_ref::<crate::diagnostic::CliError>()
            .map_or_else(
                || {
                    CliDiagnostic::error(
                        MADS210,
                        "Database command failed",
                        "the database command could not be completed",
                    )
                },
                |error| CliDiagnostic::from_error(error, None),
            )
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl fmt::Debug for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CliError")
            .field("message", &self.message)
            .field("source", &"[REDACTED]")
            .finish()
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// One completed database command before the outer renderer selects a format.
pub(crate) struct DatabaseOutcome {
    data: CommandData,
    diagnostics: Vec<CliDiagnostic>,
    human_lines: Vec<String>,
}

impl DatabaseOutcome {
    fn new(data: CommandData, diagnostics: Vec<CliDiagnostic>, human_lines: Vec<String>) -> Self {
        Self {
            data,
            diagnostics,
            human_lines,
        }
    }
}

/// Converts a database execution result into the shared finite-command outcome.
pub(crate) fn outcome(
    command: DatabaseCommand,
    result: Result<DatabaseOutcome, CliError>,
) -> Outcome {
    let command = database_command_name(command);
    match result {
        Ok(result) => Outcome::new(
            Envelope::success_with_diagnostics(command, result.data, result.diagnostics),
            HumanOutput::streams(human_lines(&result.human_lines), String::new()),
        ),
        Err(error) => Outcome::new(
            Envelope::failure(Some(command.into()), None, vec![error.public_diagnostic()]),
            HumanOutput::stderr(format!("error: {error}\n")),
        ),
    }
}

fn database_command_name(command: DatabaseCommand) -> &'static str {
    match command {
        DatabaseCommand::Generate => "db generate",
        DatabaseCommand::Migrate => "db migrate",
        DatabaseCommand::Rollback => "db rollback",
        DatabaseCommand::Status => "db status",
        DatabaseCommand::Help => {
            unreachable!("database help is rendered before database execution")
        }
    }
}

fn human_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

/// Loads database configuration for the selected package root.
pub(crate) fn load_project(root: &Path) -> Result<LoadedDatabaseProject, CliError> {
    let config = ConfigBuilder::new()
        .dotenv(DotenvSource::optional(root.join(".env")))
        .source(TomlSource::file(root.join("mads.toml")))
        .source(EnvSource::new("MADS_"))
        .build()
        .map_err(|error| CliError::from_error(error.to_string(), error))?;
    let database = DatabaseConfig::from_config(&config)
        .map_err(|error| CliError::from_error(error.to_string(), error))?;

    Ok(LoadedDatabaseProject {
        root: root.to_owned(),
        database,
    })
}

/// Loads one project and executes its requested database operation.
pub(crate) async fn execute(
    command: DatabaseCommand,
    root: &Path,
) -> Result<DatabaseOutcome, CliError> {
    if matches!(command, DatabaseCommand::Generate) {
        return generate(root).await;
    }

    let project = load_project(root)?;
    let migrations = match command {
        DatabaseCommand::Migrate | DatabaseCommand::Rollback | DatabaseCommand::Status => {
            Some(project.migrations()?)
        }
        DatabaseCommand::Help => None,
        DatabaseCommand::Generate => unreachable!("generate returns before loading configuration"),
    };
    let database = project.connect()?;

    let result = match command {
        DatabaseCommand::Migrate => database
            .run_pending_migrations(migrations.expect("migrate should load migrations"))
            .await
            .map(|report| {
                let applied = report.versions().to_vec();
                let human_lines = if report.is_empty() {
                    vec!["database is up to date".to_owned()]
                } else {
                    applied
                        .iter()
                        .map(|version| format!("applied {version}"))
                        .collect()
                };
                DatabaseOutcome::new(
                    CommandData::DatabaseMigrate(DatabaseMigrateData::new(applied)),
                    Vec::new(),
                    human_lines,
                )
            }),
        DatabaseCommand::Rollback => database
            .revert_last_migration(migrations.expect("rollback should load migrations"))
            .await
            .map(|report| {
                let reverted = report.versions().to_vec();
                let human_lines = reverted
                    .iter()
                    .map(|version| format!("reverted {version}"))
                    .collect();
                DatabaseOutcome::new(
                    CommandData::DatabaseRollback(DatabaseRollbackData::new(reverted)),
                    Vec::new(),
                    human_lines,
                )
            }),
        DatabaseCommand::Status => database
            .migration_status(migrations.expect("status should load migrations"))
            .await
            .map(|status| {
                let applied = status.applied().to_vec();
                let pending = status.pending().to_vec();
                let mut human_lines = applied
                    .iter()
                    .map(|version| format!("applied {version}"))
                    .collect::<Vec<_>>();
                human_lines.extend(pending.iter().map(|version| format!("pending {version}")));
                human_lines.push(format!(
                    "summary: {} applied, {} pending",
                    applied.len(),
                    pending.len()
                ));
                DatabaseOutcome::new(
                    CommandData::DatabaseStatus(DatabaseStatusData::new(applied, pending)),
                    Vec::new(),
                    human_lines,
                )
            }),
        DatabaseCommand::Generate | DatabaseCommand::Help => unreachable!(
            "generate returns before database execution and help is rendered by dispatch"
        ),
    };
    database.close();

    result.map_err(CliError::database)
}

/// Generates one full supported schema-shape migration without applying it.
pub(crate) async fn generate(root: &Path) -> Result<DatabaseOutcome, CliError> {
    let desired = DesiredSchema::load(root).map_err(CliError::diagnostic)?;
    let project = load_project(root)?;
    let database = project.connect()?;
    let namespaces = desired.namespaces();
    let live_result = LiveSchema::load(&database, &namespaces).await;
    database.close();
    let live = live_result.map_err(CliError::diagnostic)?;
    let plan = plan_diff(&desired, &live).map_err(CliError::diagnostic)?;
    if plan.is_empty() {
        return Ok(DatabaseOutcome::new(
            CommandData::DatabaseGenerate(DatabaseGenerateData::new("up_to_date", None, false)),
            Vec::new(),
            vec!["schema is up to date".to_owned()],
        ));
    }

    let rendered = render_migration(&plan);
    let path =
        publish_migration(root, &rendered, &SystemMigrationClock).map_err(CliError::diagnostic)?;
    let diagnostics = rendered
        .warnings
        .iter()
        .map(diff::MigrationWarning::diagnostic)
        .collect::<Vec<_>>();
    let human_migration_path = format_generated_path(root, &path)?;
    let migration_path = crate::output::path::normalize_path(root, &path);
    let mut human_lines = rendered
        .warnings
        .iter()
        .map(|warning| format!("warning: {}: {}", warning.subject, warning.message))
        .collect::<Vec<_>>();
    human_lines.push(format!("generated {human_migration_path}"));
    human_lines.push("review up.sql and down.sql before applying".to_owned());
    Ok(DatabaseOutcome::new(
        CommandData::DatabaseGenerate(DatabaseGenerateData::new(
            "generated",
            Some(migration_path),
            true,
        )),
        diagnostics,
        human_lines,
    ))
}

fn format_generated_path(root: &Path, path: &Path) -> Result<String, CliError> {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .map_err(|error| {
            CliError::diagnostic(
                crate::diagnostic::CliError::new(
                    crate::diagnostic::MADS213,
                    "Migration publication failed",
                    "the generated migration path is outside the selected package root",
                )
                .with_source(error),
            )
        })
}

fn contains_no_migration(error: &DatabaseError) -> bool {
    let mut current = error.source();
    while let Some(source) = current {
        if matches!(
            source.downcast_ref::<MigrationError>(),
            Some(MigrationError::NoMigrationRun)
        ) {
            return true;
        }
        current = source.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::{TempDir, tempdir};

    use super::{format_generated_path, load_project};

    #[test]
    fn loading_a_database_project_does_not_require_a_migrations_directory() {
        let root = database_project_without_migrations();
        let project = load_project(root.path()).unwrap();

        assert_eq!(project.root(), root.path());
        assert!(project.migrations().is_err());
    }

    #[test]
    fn generated_migration_path_is_relative_to_the_selected_package_root() {
        let root = tempdir().expect("temporary project should be created");
        let generated = root
            .path()
            .join("migrations/01788200000123456789_schema_diff");

        assert_eq!(
            format_generated_path(root.path(), &generated).unwrap(),
            "migrations/01788200000123456789_schema_diff"
        );
    }

    fn database_project_without_migrations() -> TempDir {
        let root = tempdir().expect("temporary project should be created");
        fs::write(
            root.path().join("mads.toml"),
            "[database]\nurl = \"postgres://localhost/mads\"\n",
        )
        .expect("project TOML should be written");
        root
    }
}
