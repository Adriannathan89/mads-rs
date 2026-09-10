//! Verifies attribute expansion through a renamed direct-core dependency.

use runtime::AutoConfigurationStatus;

#[derive(runtime::Configuration)]
struct Settings {
    #[config(default = 3000, validate(positive))]
    port: u16,
}

#[runtime::module]
struct AppModule;

#[runtime::repository]
struct Repository;

fn framework_result() -> runtime::Result<()> {
    Ok(())
}

fn status_name(status: AutoConfigurationStatus) -> &'static str {
    match status {
        AutoConfigurationStatus::Active => "active",
        AutoConfigurationStatus::Skipped => "skipped",
        AutoConfigurationStatus::Overridden => "overridden",
        AutoConfigurationStatus::Failed => "failed",
    }
}

fn main() {
    assert_eq!(runtime::Config::empty().parse::<Settings>().unwrap().port, 3000);
    let _ = framework_result;
    let _ = status_name;
}
