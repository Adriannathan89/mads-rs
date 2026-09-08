//! Schema-versioned records for machine-readable MADS CLI output.

use std::path::Path;

use serde::Serialize;

use crate::{diagnostic::CliError, output::path::normalize_path};

/// The first supported machine-readable CLI schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// One complete schema-v1 command result.
#[derive(Clone, Debug, Serialize)]
pub struct Envelope {
    schema_version: u32,
    command: Option<String>,
    ok: bool,
    data: Option<CommandData>,
    diagnostics: Vec<CliDiagnostic>,
}

impl Envelope {
    /// Creates a successful command result with structured data.
    pub fn success(command: impl Into<String>, data: CommandData) -> Self {
        Self::success_with_diagnostics(command, data, Vec::new())
    }

    pub(crate) fn success_with_diagnostics(
        command: impl Into<String>,
        data: CommandData,
        diagnostics: Vec<CliDiagnostic>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: Some(command.into()),
            ok: true,
            data: Some(data),
            diagnostics,
        }
    }

    /// Creates a failed command result with optional trustworthy partial data.
    pub fn failure(
        command: Option<String>,
        data: Option<CommandData>,
        diagnostics: Vec<CliDiagnostic>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command,
            ok: false,
            data,
            diagnostics,
        }
    }
}

/// Command-specific data embedded in a schema-v1 envelope.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum CommandData {
    /// `mads new` project publication data.
    New(NewData),
    /// `mads routes` inspection data.
    Routes(RoutesData),
    /// `mads graph` inspection data.
    Graph(GraphData),
    /// `mads doctor` inspection data.
    Doctor(DoctorData),
    /// `mads db generate` data.
    DatabaseGenerate(DatabaseGenerateData),
    /// `mads db migrate` data.
    DatabaseMigrate(DatabaseMigrateData),
    /// `mads db rollback` data.
    DatabaseRollback(DatabaseRollbackData),
    /// `mads db status` data.
    DatabaseStatus(DatabaseStatusData),
}

/// Published project data for `mads new`.
#[derive(Clone, Debug, Serialize)]
pub struct NewData {
    project_name: String,
    path: String,
    files: Vec<String>,
}

impl NewData {
    /// Creates project publication data using normalized relative paths.
    pub fn new(
        project_name: impl Into<String>,
        path: impl Into<String>,
        files: Vec<String>,
    ) -> Self {
        Self {
            project_name: project_name.into(),
            path: path.into(),
            files,
        }
    }
}

/// Route inspection data.
#[derive(Clone, Debug, Serialize)]
pub struct RoutesData {
    routes: Vec<RouteData>,
}

impl RoutesData {
    /// Creates route inspection data from the ordered route records.
    pub fn new(routes: Vec<RouteData>) -> Self {
        Self { routes }
    }
}

/// One inspected HTTP route.
#[derive(Clone, Debug, Serialize)]
pub struct RouteData {
    method: String,
    path: String,
    route_trait: String,
    handler: String,
    controller: String,
    location: SourceLocation,
    guard_active: bool,
}

impl RouteData {
    /// Creates one route record for the public CLI schema.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        method: impl Into<String>,
        path: impl Into<String>,
        route_trait: impl Into<String>,
        handler: impl Into<String>,
        controller: impl Into<String>,
        location: SourceLocation,
        guard_active: bool,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            route_trait: route_trait.into(),
            handler: handler.into(),
            controller: controller.into(),
            location,
            guard_active,
        }
    }
}

/// Graph inspection data.
#[derive(Clone, Debug, Serialize)]
pub struct GraphData {
    root_module: Option<String>,
    modules: Vec<ModuleData>,
    imports: Vec<ImportData>,
    providers: Vec<ProviderData>,
    dependencies: Vec<DependencyData>,
    construction_order: Option<Vec<String>>,
}

impl GraphData {
    /// Creates graph inspection data from already ordered graph records.
    pub fn new(
        root_module: Option<String>,
        modules: Vec<ModuleData>,
        imports: Vec<ImportData>,
        providers: Vec<ProviderData>,
        dependencies: Vec<DependencyData>,
        construction_order: Option<Vec<String>>,
    ) -> Self {
        Self {
            root_module,
            modules,
            imports,
            providers,
            dependencies,
            construction_order,
        }
    }
}

/// One reachable application module.
#[derive(Clone, Debug, Serialize)]
pub struct ModuleData {
    type_name: String,
    namespace: String,
    location: SourceLocation,
}

impl ModuleData {
    /// Creates one module record.
    pub fn new(
        type_name: impl Into<String>,
        namespace: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        Self {
            type_name: type_name.into(),
            namespace: namespace.into(),
            location,
        }
    }
}

/// One direct module import.
#[derive(Clone, Debug, Serialize)]
pub struct ImportData {
    importer: String,
    imported: String,
}

impl ImportData {
    /// Creates one module-import record.
    pub fn new(importer: impl Into<String>, imported: impl Into<String>) -> Self {
        Self {
            importer: importer.into(),
            imported: imported.into(),
        }
    }
}

/// One selected provider.
#[derive(Clone, Debug, Serialize)]
pub struct ProviderData {
    type_name: String,
    owner: Option<String>,
    origin: String,
    visibility: String,
    state: String,
    location: Option<SourceLocation>,
}

impl ProviderData {
    /// Creates one provider record.
    pub fn new(
        type_name: impl Into<String>,
        owner: Option<String>,
        origin: impl Into<String>,
        visibility: impl Into<String>,
        state: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> Self {
        Self {
            type_name: type_name.into(),
            owner,
            origin: origin.into(),
            visibility: visibility.into(),
            state: state.into(),
            location,
        }
    }
}

/// One provider dependency edge.
#[derive(Clone, Debug, Serialize)]
pub struct DependencyData {
    provider: String,
    dependency: String,
}

impl DependencyData {
    /// Creates one provider dependency record.
    pub fn new(provider: impl Into<String>, dependency: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            dependency: dependency.into(),
        }
    }
}

/// Doctor inspection data.
#[derive(Clone, Debug, Serialize)]
pub struct DoctorData {
    checks: Vec<DoctorCheckData>,
}

impl DoctorData {
    /// Creates doctor inspection data from ordered check records.
    pub fn new(checks: Vec<DoctorCheckData>) -> Self {
        Self { checks }
    }
}

/// One doctor check result.
#[derive(Clone, Debug, Serialize)]
pub struct DoctorCheckData {
    group: String,
    status: DoctorStatus,
    summary: String,
}

impl DoctorCheckData {
    /// Creates one doctor check record.
    pub fn new(group: impl Into<String>, status: DoctorStatus, summary: impl Into<String>) -> Self {
        Self {
            group: group.into(),
            status,
            summary: summary.into(),
        }
    }
}

/// A doctor check's stable lower-case status.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    /// The check passed.
    Pass,
    /// The check does not apply.
    Skipped,
    /// Application configuration deliberately overrides the check.
    Overridden,
    /// The check failed.
    Failed,
}

/// Data returned by `mads db generate`.
#[derive(Clone, Debug, Serialize)]
pub struct DatabaseGenerateData {
    status: String,
    migration_path: Option<String>,
    review_required: bool,
}

impl DatabaseGenerateData {
    /// Creates database-generation data.
    pub fn new(
        status: impl Into<String>,
        migration_path: Option<String>,
        review_required: bool,
    ) -> Self {
        Self {
            status: status.into(),
            migration_path,
            review_required,
        }
    }
}

/// Data returned by `mads db migrate`.
#[derive(Clone, Debug, Serialize)]
pub struct DatabaseMigrateData {
    applied: Vec<String>,
}

impl DatabaseMigrateData {
    /// Creates database-migration data.
    pub fn new(applied: Vec<String>) -> Self {
        Self { applied }
    }
}

/// Data returned by `mads db rollback`.
#[derive(Clone, Debug, Serialize)]
pub struct DatabaseRollbackData {
    reverted: Vec<String>,
}

impl DatabaseRollbackData {
    /// Creates database-rollback data.
    pub fn new(reverted: Vec<String>) -> Self {
        Self { reverted }
    }
}

/// Data returned by `mads db status`.
#[derive(Clone, Debug, Serialize)]
pub struct DatabaseStatusData {
    applied: Vec<String>,
    pending: Vec<String>,
}

impl DatabaseStatusData {
    /// Creates database-status data.
    pub fn new(applied: Vec<String>, pending: Vec<String>) -> Self {
        Self { applied, pending }
    }
}

/// One source location in the public CLI schema.
#[derive(Clone, Debug, Serialize)]
pub struct SourceLocation {
    file: String,
    line: u32,
    column: u32,
}

impl SourceLocation {
    /// Creates a source location whose file path already uses schema form.
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

/// The severity of a MADS-owned diagnostic.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    /// The command could not complete successfully.
    Error,
    /// The command completed but needs review.
    Warning,
}

/// One MADS-owned diagnostic in the public CLI schema.
#[derive(Clone, Debug, Serialize)]
pub struct CliDiagnostic {
    severity: Severity,
    code: String,
    title: String,
    message: String,
    subject: Option<String>,
    location: Option<SourceLocation>,
    suggestions: Vec<String>,
}

impl CliDiagnostic {
    /// Creates an error diagnostic.
    pub fn error(
        code: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Error, code, title, message)
    }

    /// Creates a warning diagnostic.
    pub fn warning(
        code: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Warning, code, title, message)
    }

    /// Adds a related subject.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Adds a source location.
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Adds one ordered remediation suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    pub(crate) fn from_error(error: &CliError, package_root: Option<&Path>) -> Self {
        let mut diagnostic = Self::error(error.code(), error.title(), error.message());
        if let Some(subject) = error.subject() {
            diagnostic = diagnostic.with_subject(subject);
        }
        if let Some(location) = error.location() {
            let file = package_root.map_or_else(
                || location.path().to_string_lossy().replace('\\', "/"),
                |root| normalize_path(root, location.path()),
            );
            diagnostic = diagnostic.with_location(SourceLocation::new(
                file,
                location.line(),
                location.column(),
            ));
        }
        for suggestion in error.suggestions() {
            diagnostic = diagnostic.with_suggestion(suggestion);
        }
        diagnostic
    }

    fn new(
        severity: Severity,
        code: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            title: title.into(),
            message: message.into(),
            subject: None,
            location: None,
            suggestions: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::diagnostic::{CliError, MADS201};

    use super::{CliDiagnostic, SourceLocation};

    #[test]
    fn error_conversion_is_redacted_and_normalizes_its_location() {
        let error = CliError::new(MADS201, "Cargo metadata failed", "safe message")
            .with_subject("manifest")
            .with_location("/workspace/app/src/main.rs", 4, 2)
            .with_suggestion("repair Cargo.toml")
            .with_source(std::io::Error::other("/registry/secret"));

        let diagnostic = CliDiagnostic::from_error(&error, Some(Path::new("/workspace/app")));
        let json = serde_json::to_string(&diagnostic).expect("diagnostic should serialize");

        assert_eq!(
            json,
            "{\"severity\":\"error\",\"code\":\"MADS201\",\"title\":\"Cargo metadata failed\",\"message\":\"safe message\",\"subject\":\"manifest\",\"location\":{\"file\":\"src/main.rs\",\"line\":4,\"column\":2},\"suggestions\":[\"repair Cargo.toml\"]}"
        );
        assert!(!json.contains("/registry/secret"));
    }

    #[test]
    fn explicit_public_locations_preserve_the_given_schema_path() {
        let diagnostic = CliDiagnostic::warning("MADS212", "review", "review SQL")
            .with_location(SourceLocation::new("src/schema.rs", 2, 1));

        assert!(
            serde_json::to_string(&diagnostic)
                .expect("diagnostic should serialize")
                .contains("src/schema.rs")
        );
    }
}
