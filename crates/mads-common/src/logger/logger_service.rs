use std::sync::Arc;

use super::ConsoleLoggerService;

/// Severity assigned to a log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// An error that prevents the requested operation from completing.
    Error,
    /// A potentially harmful condition that does not stop the operation.
    Warn,
    /// Diagnostic information for development and troubleshooting.
    Debug,
    /// Fine-grained diagnostic information.
    Trace,
    /// A fatal error that requires immediate attention.
    Fatal,
    /// Informational messages that highlight the progress of the application.
    Info,
}

/// Contract implemented by concrete logging backends.
///
/// Applications implement this trait to route MADS logs to a custom destination,
/// then wrap the implementation with [`Logger::new`].
pub trait LoggerService: Send + Sync {
    /// Records `message` at `level`.
    fn log(&self, level: LogLevel, message: &str);

    /// Records a warning message.
    fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    /// Records an error message.
    fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    /// Records an info message.
    fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    /// Records a debug message.
    fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    /// Records a trace message.
    fn trace(&self, message: &str, _trace: Option<&str>, _context: Option<&str>) {
        self.log(LogLevel::Trace, message);
    }

    /// Records a fatal message.
    fn fatal(&self, message: &str, _context: Option<&str>) {
        self.log(LogLevel::Fatal, message);
    }
}

/// Application logging façade backed by a polymorphic [`LoggerService`].
/// Public methods delegate to the inner logger, which is configured at application startup.
#[derive(Clone)]
pub struct Logger {
    inner: Arc<dyn LoggerService>,
}

impl Logger {
    /// Creates a logger that delegates to `inner`.
    pub fn new(inner: impl LoggerService + 'static) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Records `message` at `level`.
    pub fn log(&self, level: LogLevel, message: &str) {
        self.inner.log(level, message);
    }

    /// Records a warning message.
    pub fn warn(&self, message: &str) {
        self.inner.warn(message);
    }

    /// Records an info message.
    pub fn info(&self, message: &str) {
        self.inner.info(message);
    }

    /// Records an error message.
    pub fn error(&self, message: &str) {
        self.inner.error(message);
    }

    /// Records a debug message.
    pub fn debug(&self, message: &str) {
        self.inner.debug(message);
    }

    /// Records a trace message with optional trace and context details.
    pub fn trace(&self, message: &str, trace: Option<&str>, context: Option<&str>) {
        let mut full_message = message.to_string();
        if let Some(trace) = trace {
            full_message.push_str(&format!("\nTrace: {trace}"));
        }
        if let Some(context) = context {
            full_message.push_str(&format!("\nContext: {context}"));
        }

        self.inner.trace(&full_message, None, None);
    }

    /// Records a fatal message with optional context details.
    pub fn fatal(&self, message: &str, context: Option<&str>) {
        let mut full_message = message.to_string();
        if let Some(context) = context {
            full_message.push_str(&format!("\nContext: {context}"));
        }
        self.inner.fatal(&full_message, None);
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new(ConsoleLoggerService)
    }
}
