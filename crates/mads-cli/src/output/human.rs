//! Compatibility rendering for the CLI's established human text output.

use std::io::{self, Write};

/// Human output produced before the outer renderer selects a stream.
pub(crate) struct HumanOutput {
    stdout: String,
    stderr: String,
}

impl HumanOutput {
    pub(crate) fn stderr(stderr: impl Into<String>) -> Self {
        Self::streams(String::new(), stderr)
    }

    pub(crate) fn streams(stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }
}

pub(crate) fn write(output: &HumanOutput) -> io::Result<()> {
    if !output.stdout.is_empty() {
        io::stdout().lock().write_all(output.stdout.as_bytes())?;
    }
    if !output.stderr.is_empty() {
        io::stderr().lock().write_all(output.stderr.as_bytes())?;
    }
    Ok(())
}
