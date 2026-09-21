//! Public API coverage for the optional logger integration.

#![cfg(feature = "logger")]

use std::sync::{Arc, Mutex};

use mads_common::{LogLevel, Logger, LoggerService};
use mads_core::Mads;

#[derive(Clone, Default)]
struct RecordingLogger {
    entries: Arc<Mutex<Vec<(LogLevel, String)>>>,
}

impl LoggerService for RecordingLogger {
    fn log(&self, level: LogLevel, message: &str) {
        self.entries
            .lock()
            .unwrap()
            .push((level, message.to_owned()));
    }
}

#[test]
fn logger_delegates_messages_to_the_configured_inner_logger() {
    let inner = RecordingLogger::default();
    let entries = Arc::clone(&inner.entries);
    let logger = Logger::new(inner);

    logger.error("database is unavailable");

    let entries = entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].0, LogLevel::Error));
    assert_eq!(entries[0].1, "database is unavailable");
}

mod consumer {
    use mads_common::Logger;

    #[mads_core::service]
    pub struct LoggingService {
        _logger: Logger,
    }

    #[mads_core::module]
    pub struct ConsumerModule;
}

mod default_logger_application {
    use mads_common::LoggerModule;

    #[mads_core::module(imports = [LoggerModule, super::consumer::ConsumerModule])]
    pub struct ApplicationModule;
}

#[tokio::test]
async fn global_logger_module_provides_the_default_logger_to_other_modules() {
    let mut builder = Mads::builder();
    builder
        .root::<default_logger_application::ApplicationModule>()
        .unwrap();
    let application = builder.build().await.unwrap();

    assert!(
        application
            .context()
            .resolve::<consumer::LoggingService>()
            .is_ok()
    );
}

mod custom_logger {
    use mads_common::{LogLevel, Logger, LoggerService};

    struct TestLogger;

    impl LoggerService for TestLogger {
        fn log(&self, _level: LogLevel, _message: &str) {}
    }

    #[mads_core::module(global)]
    pub struct CustomLoggerModule;

    #[mads_core::provider]
    pub fn custom_logger() -> Logger {
        Logger::new(TestLogger)
    }
}

mod custom_logger_application {
    #[mads_core::module(imports = [super::custom_logger::CustomLoggerModule, super::consumer::ConsumerModule])]
    pub struct CustomLoggerApplicationModule;
}

#[tokio::test]
async fn application_can_manually_provide_a_custom_global_logger() {
    let mut builder = Mads::builder();
    builder
        .root::<custom_logger_application::CustomLoggerApplicationModule>()
        .unwrap();
    let application = builder.build().await.unwrap();

    assert!(
        application
            .context()
            .resolve::<consumer::LoggingService>()
            .is_ok()
    );
}
