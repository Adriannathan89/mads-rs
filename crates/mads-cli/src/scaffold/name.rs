//! Frozen Cargo 1.85 and Rust 2024 project-name validation.

use std::fmt;

use crate::diagnostic::MADS230;

const CARGO_RESERVED_PACKAGE_NAMES: &[&str] = &["test", "deps", "examples", "build", "incremental"];

const RUST_2024_STRICT_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn",
];

const RUST_2024_RESERVED_KEYWORDS: &[&str] = &[
    "abstract", "become", "box", "do", "final", "gen", "macro", "override", "priv", "try",
    "typeof", "unsized", "virtual", "yield",
];

/// A project name accepted by the frozen MADS scaffolding policy.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProjectName(String);

impl ProjectName {
    /// Validates a project name without normalizing or otherwise changing it.
    pub fn parse(value: &str) -> Result<Self, ProjectNameError> {
        if !has_approved_grammar(value) {
            return Err(ProjectNameError::InvalidGrammar);
        }

        if is_reserved(value) {
            return Err(ProjectNameError::Reserved);
        }

        Ok(Self(value.to_owned()))
    }

    /// Returns the exact user-supplied spelling that passed validation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A controlled MADS230 reason for an invalid project name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectNameError {
    /// The name does not use the approved lowercase ASCII grammar.
    InvalidGrammar,
    /// The name is reserved by the frozen Cargo or Rust policy.
    Reserved,
}

impl ProjectNameError {
    /// Returns the stable scaffolding diagnostic code.
    pub const fn code(self) -> &'static str {
        MADS230
    }

    /// Returns the stable scaffolding diagnostic title.
    pub const fn title(self) -> &'static str {
        "Project scaffolding failed"
    }
}

impl fmt::Display for ProjectNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGrammar => formatter.write_str(
                "project name must start with a lowercase ASCII letter and use only lowercase ASCII letters, digits, '-' or '_' thereafter",
            ),
            Self::Reserved => {
                formatter.write_str("project name is reserved by Cargo 1.85 or Rust 2024")
            }
        }
    }
}

impl std::error::Error for ProjectNameError {}

fn has_approved_grammar(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };

    first.is_ascii_lowercase()
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn is_reserved(value: &str) -> bool {
    CARGO_RESERVED_PACKAGE_NAMES.contains(&value)
        || RUST_2024_STRICT_KEYWORDS.contains(&value)
        || RUST_2024_RESERVED_KEYWORDS.contains(&value)
}
