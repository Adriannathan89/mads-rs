use chrono::Local;

use super::{LogLevel, LoggerService};

/// Default logger that writes timestamped messages to standard error.
pub struct ConsoleLoggerService;

impl LoggerService for ConsoleLoggerService {
    fn log(&self, level: LogLevel, message: &str) {
        eprintln!(
            "{} [{level:?}] {message}",
            Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"),
        );
    }
}
