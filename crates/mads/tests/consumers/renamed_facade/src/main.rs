//! Verifies attribute expansion through a renamed facade-only dependency.

use framework::prelude::*;

#[derive(Configuration)]
struct Settings {
    #[config(default = "localhost", validate(nonempty))]
    host: String,
}

#[module]
struct AppModule;

#[repository]
struct RenamedRepository {
    database: Database,
}

fn consume_repository(repository: &RenamedRepository) {
    let _ = &repository.database;
}

fn diesel_backend(_: std::marker::PhantomData<framework::diesel::pg::Pg>) {}

#[routes]
trait Routes {
    #[get("/")]
    async fn index(&self);
}

#[controller(routes = [Routes])]
struct Controller;

impl Routes for Controller {
    async fn index(&self) {}
}

fn inspect_auto_configuration() {
    let config = framework::core::ConfigBuilder::new()
        .source(framework::core::MapSource::new(
            "consumer",
            [("database.url", "postgres://localhost/renamed")],
        ))
        .build()
        .unwrap();
    let analysis = Mads::builder_with_config(config).analyze();

    assert_eq!(
        analysis.auto_configurations()[0].status(),
        AutoConfigurationStatus::Active,
    );
    assert_eq!(
        analysis.graph().provider::<Database>().unwrap().origin(),
        ProviderOrigin::AutoConfiguration,
    );
}

fn main() {
    assert_eq!(Config::empty().parse::<Settings>().unwrap().host, "localhost");
    let _ = inspect_auto_configuration;
    let _ = diesel_backend;
    let _ = consume_repository;
}
