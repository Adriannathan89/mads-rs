//! JSON serialization and single-document writing for schema-v1 output.

use std::{
    error::Error,
    fmt,
    io::{self, Write},
};

use super::model::Envelope;

/// Serializes one schema-v1 envelope followed by exactly one newline.
pub fn render(envelope: &Envelope) -> serde_json::Result<String> {
    let mut output = serde_json::to_string(envelope)?;
    output.push('\n');
    Ok(output)
}

pub(crate) fn write(envelope: &Envelope) -> Result<(), OutputError> {
    let output = render(envelope).map_err(OutputError::Serialize)?;
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(OutputError::Write)
}

#[derive(Debug)]
pub(crate) enum OutputError {
    Serialize(serde_json::Error),
    Write(io::Error),
}

impl fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("could not write schema-v1 output")
    }
}

impl Error for OutputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize(error) => Some(error),
            Self::Write(error) => Some(error),
        }
    }
}
