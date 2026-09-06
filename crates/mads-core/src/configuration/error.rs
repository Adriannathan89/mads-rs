//! Ordered, value-free typed configuration failures.

use std::fmt;

use crate::{Diagnostic, Error, MADS020};

/// The result of parsing or validating a typed configuration.
pub type ConfigurationResult<T> = Result<T, ConfigurationErrors>;

/// A safe failure for one configuration key.
///
/// Application-supplied codes, messages, and source labels must not contain
/// configured values. Built-in parsing never retains rejected values.
#[derive(Debug)]
pub struct ConfigurationIssue {
    key: String,
    code: String,
    message: String,
    source: Option<String>,
}

impl ConfigurationIssue {
    /// Creates a failure without inventing source attribution.
    pub fn new(
        key: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            code: code.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Attaches the winning source label, or `default` for a rejected default.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Returns the complete dotted key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the stable reason code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the safe explanation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns source attribution when a value or default supplied it.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
}

/// One or more typed configuration failures in declaration order.
///
/// Conversion to the framework error preserves every issue as an ordered
/// `MADS020` diagnostic without attaching arbitrary parser errors.
#[derive(Debug)]
pub struct ConfigurationErrors {
    // The public constructors and mutation methods preserve non-emptiness.
    issues: Vec<ConfigurationIssue>,
}

impl ConfigurationErrors {
    /// Creates an ordered collection containing one issue.
    pub fn from_issue(issue: ConfigurationIssue) -> Self {
        Self {
            issues: vec![issue],
        }
    }

    /// Appends one issue.
    pub fn push(&mut self, issue: ConfigurationIssue) {
        self.issues.push(issue);
    }

    /// Appends another collection, preserving both collections' order.
    pub fn extend(&mut self, other: Self) {
        self.issues.extend(other.issues);
    }

    /// Returns the failures in declaration order.
    pub fn issues(&self) -> &[ConfigurationIssue] {
        &self.issues
    }

    pub(super) fn prefix(&mut self, prefix: &str) {
        if prefix.is_empty() {
            return;
        }
        for issue in &mut self.issues {
            issue.key = if issue.key.is_empty() {
                prefix.to_owned()
            } else {
                format!("{prefix}.{}", issue.key)
            };
        }
    }
}

impl fmt::Display for ConfigurationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index > 0 {
                formatter.write_str("\n")?;
            }
            write!(
                formatter,
                "{} [{}]: {}",
                issue.key, issue.code, issue.message
            )?;
            if let Some(source) = &issue.source {
                write!(formatter, " (source: {source})")?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for ConfigurationErrors {}

impl From<ConfigurationErrors> for Error {
    fn from(errors: ConfigurationErrors) -> Self {
        let mut diagnostics = errors.issues.into_iter().map(|issue| {
            let mut message = format!("[{}]: {}", issue.code, issue.message);
            if let Some(source) = issue.source {
                message.push_str(&format!(" (source: {source})"));
            }
            Diagnostic::new(MADS020, "typed configuration is invalid", message)
                .with_subject(issue.key)
        });
        let primary = diagnostics
            .next()
            .expect("configuration errors are non-empty");
        Self::from_diagnostics(primary, diagnostics)
    }
}
