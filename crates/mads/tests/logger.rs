//! Facade coverage for the standard logger integration.

#![cfg(feature = "logger")]

use mads::{Logger, LoggerModule, LoggerService};

struct TestLogger;

impl LoggerService for TestLogger {
    fn log(&self, _level: mads::LogLevel, _message: &str) {}
}

#[test]
fn facade_reexports_the_logger_contract_and_global_module() {
    let _module = LoggerModule;
    let logger = Logger::new(TestLogger);

    logger.debug("facade logger");
}
