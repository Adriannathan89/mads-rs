use chrono::Local;

use super::{LogLevel, LoggerService};

/// Default logger that writes timestamped messages to standard error.
pub struct ConsoleLoggerService;

impl LoggerService for ConsoleLoggerService {
    fn log(&self, level: LogLevel, message: &str) {
        let level_str = match level {
            LogLevel::Trace => "\x1b[34mTRACE\x1b[0m", // Blue
            LogLevel::Debug => "\x1b[36mDEBUG\x1b[0m", // Cyan
            LogLevel::Info => "\x1b[32mINFO\x1b[0m",   // Green
            LogLevel::Warn => "\x1b[33mWARN\x1b[0m",   // Yellow
            LogLevel::Error => "\x1b[31mERROR\x1b[0m", // Red
            LogLevel::Fatal => "\x1b[31mFATAL\x1b[0m", // Red
        };
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        eprintln!("{} - [{}] {}", timestamp, level_str, message);
    }
}
