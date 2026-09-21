mod console_logger_service;
mod logger_service;

pub use console_logger_service::ConsoleLoggerService;
pub use logger_service::{LogLevel, Logger, LoggerService};

/// Global module that provides the default console-backed [`Logger`].
///
/// Import this module from the application's root module when the default
/// [`ConsoleLoggerService`] is wanted:
///
/// ```
/// use mads_common::LoggerModule;
///
/// #[mads_common::core::module(imports = [LoggerModule])]
/// struct AppModule;
/// ```
///
/// To override the logger, do not import `LoggerModule`. Instead, declare an
/// application-owned `#[mads_core::module(global)]` that provides exactly one
/// public [`Logger`] constructed with [`Logger::new`]. Registering both
/// providers creates an ambiguous duplicate and is rejected by the dependency
/// graph.
#[crate::core::module(global)]
pub struct LoggerModule;

/// Provides the default console-backed [`Logger`].
#[crate::core::provider]
pub fn logger() -> Logger {
    Logger::default()
}
