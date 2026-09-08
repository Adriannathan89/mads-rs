use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use crate::command::ParseError;

pub(crate) const MADS200: &str = "MADS200";
pub(crate) const MADS201: &str = "MADS201";
pub(crate) const MADS202: &str = "MADS202";
pub(crate) const MADS204: &str = "MADS204";
pub(crate) const MADS210: &str = "MADS210";
pub(crate) const MADS211: &str = "MADS211";
pub(crate) const MADS212: &str = "MADS212";
pub(crate) const MADS213: &str = "MADS213";
pub(crate) const MADS220: &str = "MADS220";
pub(crate) const MADS230: &str = "MADS230";

/// A source location attached to a CLI-owned diagnostic before output rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ErrorLocation {
    path: PathBuf,
    line: u32,
    column: u32,
}

impl ErrorLocation {
    /// Returns the compiler-provided path before package-relative normalization.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the one-based source line.
    pub(crate) const fn line(&self) -> u32 {
        self.line
    }

    /// Returns the one-based source column.
    pub(crate) const fn column(&self) -> u32 {
        self.column
    }
}

#[derive(Default)]
struct CliErrorDetails {
    subject: Option<String>,
    location: Option<ErrorLocation>,
    suggestions: Vec<String>,
}

pub(crate) struct CliError {
    code: &'static str,
    title: &'static str,
    message: String,
    details: Box<CliErrorDetails>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl CliError {
    pub(crate) fn new(code: &'static str, title: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            title,
            message: message.into(),
            details: Box::default(),
            source: None,
        }
    }

    pub(crate) fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.details.subject = Some(subject.into());
        self
    }

    pub(crate) fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.details.suggestions.push(suggestion.into());
        self
    }

    /// Attaches a one-based source location for structured rendering.
    pub(crate) fn with_location(
        mut self,
        path: impl Into<PathBuf>,
        line: u32,
        column: u32,
    ) -> Self {
        self.details.location = Some(ErrorLocation {
            path: path.into(),
            line,
            column,
        });
        self
    }

    pub(crate) fn with_source<E>(mut self, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        self.source = Some(Box::new(source));
        self
    }

    /// Builds the stable, source-redacting diagnostic for scaffold operations.
    pub(crate) fn scaffolding<E>(message: &'static str, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::new(MADS230, "Project scaffolding failed", message).with_source(source)
    }

    pub(crate) const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the short stable diagnostic title.
    pub(crate) const fn title(&self) -> &'static str {
        self.title
    }

    /// Returns the safe diagnostic message.
    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    /// Returns the related subject when one is available.
    pub(crate) fn subject(&self) -> Option<&str> {
        self.details.subject.as_deref()
    }

    /// Returns the source location when one is available.
    pub(crate) fn location(&self) -> Option<&ErrorLocation> {
        self.details.location.as_ref()
    }

    /// Returns the ordered remediation suggestions.
    pub(crate) fn suggestions(&self) -> &[String] {
        &self.details.suggestions
    }

    /// Builds the structured diagnostic reserved for CLI grammar failures.
    pub(crate) fn syntax(error: &ParseError) -> Self {
        Self::new(MADS204, "CLI syntax error", error.to_string())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "error[{}]: {}", self.code, self.title)?;
        if let Some(subject) = &self.details.subject {
            write!(formatter, "\n  = subject: {subject}")?;
        }
        write!(formatter, "\n  = {}", self.message)?;
        for suggestion in &self.details.suggestions {
            write!(formatter, "\n  help: {suggestion}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = if self.source.is_some() {
            "[REDACTED]"
        } else {
            self.message.as_str()
        };
        let source = if self.source.is_some() {
            "[REDACTED]"
        } else {
            "None"
        };

        formatter
            .debug_struct("CliError")
            .field("code", &self.code)
            .field("title", &self.title)
            .field("message", &message)
            .field("subject", &self.details.subject)
            .field("location", &self.details.location)
            .field("suggestions", &self.details.suggestions)
            .field("source", &source)
            .finish()
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::command::ParseError;

    use super::{CliError, MADS200, MADS201, MADS204};

    #[test]
    fn renders_a_stable_human_diagnostic() {
        let error = CliError::new(
            MADS200,
            "Cargo application target is ambiguous",
            "more than one package can be selected",
        )
        .with_subject("workspace")
        .with_suggestion("pass --package <package>");

        assert_eq!(
            error.to_string(),
            "error[MADS200]: Cargo application target is ambiguous\n  = subject: workspace\n  = more than one package can be selected\n  help: pass --package <package>"
        );
    }

    #[test]
    fn redacts_a_sourced_diagnostic_from_debug_output() {
        let error = CliError::new(MADS201, "Cargo metadata failed", "/absolute/secret")
            .with_source(io::Error::other("/absolute/registry/path"));

        let debug = format!("{error:?}");
        assert!(debug.contains(MADS201));
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("/absolute"));
    }

    #[test]
    fn reserves_mads204_for_cli_syntax() {
        let error = CliError::syntax(&ParseError::MissingValue("--format"));

        assert_eq!(error.code(), MADS204);
        assert_eq!(
            error.to_string(),
            "error[MADS204]: CLI syntax error\n  = missing value for --format"
        );
    }
}
