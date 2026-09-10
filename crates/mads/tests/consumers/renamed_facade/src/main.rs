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

mod mads {
    pub struct Json;

    impl<S> framework::axum::extract::FromRequestParts<S> for Json
    where
        S: Send + Sync,
    {
        type Rejection = framework::axum::http::StatusCode;

        async fn from_request_parts(
            _parts: &mut framework::axum::http::request::Parts,
            _state: &S,
        ) -> Result<Self, Self::Rejection> {
            Ok(Self)
        }
    }
}

#[derive(serde::Deserialize, framework::Input)]
struct CreateInput {
    name: String,
}

#[derive(framework::PassportPrincipal)]
struct Principal {
    #[roles]
    roles: Vec<String>,
}

fn passport_principal() {
    let principal = Principal {
        roles: vec!["member".into()],
    };
    assert!(framework::PassportPrincipal::has_role(&principal, "member"));
}

#[routes]
trait Routes {
    #[get("/")]
    async fn index(&self);
}

#[routes]
trait ExtractorRoutes {
    #[post("/:id")]
    async fn custom_json_before_path(&self, body: mads::Json, id: framework::Path<u64>);

    #[post("/json")]
    async fn json(&self, body: framework::Json<String>);

    #[post("/validated")]
    async fn validated(&self, body: framework::ValidatedJson<CreateInput>);

    #[post("/request")]
    async fn request(&self, request: framework::Request);
}

#[controller(routes = [Routes])]
struct Controller;

impl Routes for Controller {
    async fn index(&self) {}
}

#[controller(routes = [ExtractorRoutes])]
struct ExtractorController;

impl ExtractorRoutes for ExtractorController {
    async fn custom_json_before_path(&self, _body: mads::Json, _id: framework::Path<u64>) {}

    async fn json(&self, _body: framework::Json<String>) {}

    async fn validated(&self, body: framework::ValidatedJson<CreateInput>) {
        let _ = body.0.name;
    }

    async fn request(&self, _request: framework::Request) {}
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
    #[derive(Input)]
    struct RequestInput { #[validate(email)] email: String }
    assert!(RequestInput { email: "user@example.com".into() }.validate().is_ok());
    assert_eq!(Config::empty().parse::<Settings>().unwrap().host, "localhost");
    let _ = passport_principal;
    let _ = inspect_auto_configuration;
    let _ = diesel_backend;
    let _ = consume_repository;
}
