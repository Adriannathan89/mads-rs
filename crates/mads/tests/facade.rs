//! Runtime integration tests for facade-exported managed-provider attributes.

use std::sync::Arc;

use mads::common::{HttpMethod, Json, Path, RouteCatalog};
use mads::core::{
    Catalog as CoreCatalog, Mads as CoreMads, ProviderKind, ProviderOrigin, ProviderVisibility,
};

fn framework_result() -> mads::core::Result<()> {
    Ok(())
}

mod declarations {
    use mads::prelude::*;

    #[module]
    pub(super) struct PreludeModule;
}

#[test]
fn prelude_exposes_the_http_runtime_surface() {
    use mads::prelude::{
        BadRequest, Conflict, Created, Forbidden, Header, HttpError, HttpResult, HttpRuntimeError,
        Input, InternalError, Json, Mads, MadsRunExt, Module, ModuleGraph, ModuleImportDescriptor,
        ModuleImportEdge, ModuleNode, NoContent, NotFound, Path, ProviderOwnership, Query, Request,
        SourcedValidationIssue, Unauthorized, ValidatedJson, ValidatedPath, ValidatedQuery,
        ValidationError, ValidationErrors, ValidationIssue, ValidationPathSegment,
        ValidationResult, ValidationSource, build_router, configure_router, serve, serve_router,
    };

    fn assert_module<T: Module>() {}
    fn assert_input<T: Input>() {}

    let _ = std::any::TypeId::of::<BadRequest>();
    let _ = std::any::TypeId::of::<Unauthorized>();
    let _ = std::any::TypeId::of::<Forbidden>();
    let _ = std::any::TypeId::of::<NotFound>();
    let _ = std::any::TypeId::of::<Conflict>();
    let _ = std::any::TypeId::of::<ValidationError>();
    let _ = std::any::TypeId::of::<InternalError>();
    let _ = std::any::TypeId::of::<Created<NoContent>>();
    let _ = std::any::TypeId::of::<Header<mads::common::headers::ContentType>>();
    let _ = std::any::TypeId::of::<HttpError>();
    let _ = std::any::TypeId::of::<HttpResult<NoContent>>();
    let _ = std::any::TypeId::of::<Json<NoContent>>();
    let _ = std::any::TypeId::of::<Path<String>>();
    let _ = std::any::TypeId::of::<Query<String>>();
    let _ = std::any::TypeId::of::<Request>();
    let _ = std::any::TypeId::of::<ValidatedJson<String>>();
    let _ = std::any::TypeId::of::<ValidatedQuery<String>>();
    let _ = std::any::TypeId::of::<ValidatedPath<String>>();
    let _ = std::any::TypeId::of::<SourcedValidationIssue>();
    let _ = std::any::TypeId::of::<ValidationErrors>();
    let _ = std::any::TypeId::of::<ValidationIssue>();
    let _ = std::any::TypeId::of::<ValidationPathSegment>();
    let _ = std::any::TypeId::of::<ValidationResult>();
    let _ = std::any::TypeId::of::<ValidationSource>();
    assert_input::<String>();
    assert_module::<declarations::PreludeModule>();
    let _ = std::any::TypeId::of::<ModuleGraph>();
    let _ = std::any::TypeId::of::<ModuleImportDescriptor>();
    let _ = std::any::TypeId::of::<ModuleImportEdge>();
    let _ = std::any::TypeId::of::<ModuleNode>();
    let _ = std::any::TypeId::of::<ProviderOwnership>();
    let _ = std::any::TypeId::of::<HttpRuntimeError>();
    let _ = build_router;
    let _ = configure_router;
    let _ = |application: mads::core::Mads| serve(application, "127.0.0.1:0");
    let _ = |application: mads::core::Mads, router: mads::axum::Router| {
        serve_router(application, router, "127.0.0.1:0")
    };
    let _ = <Mads as MadsRunExt>::run::<declarations::PreludeModule>;

    fn assert_root_module<T: mads::Module>() {}
    assert_root_module::<declarations::PreludeModule>();
    let _ = std::any::TypeId::of::<mads::ModuleGraph>();
    let _ = std::any::TypeId::of::<mads::ModuleImportDescriptor>();
    let _ = std::any::TypeId::of::<mads::ModuleImportEdge>();
    let _ = std::any::TypeId::of::<mads::ModuleNode>();
    let _ = std::any::TypeId::of::<mads::ProviderOwnership>();
    let _ = std::any::TypeId::of::<mads::BadRequest>();
    let _ = std::any::TypeId::of::<mads::Unauthorized>();
    let _ = std::any::TypeId::of::<mads::Forbidden>();
    let _ = std::any::TypeId::of::<mads::NotFound>();
    let _ = std::any::TypeId::of::<mads::Conflict>();
    let _ = std::any::TypeId::of::<mads::ValidationError>();
    let _ = std::any::TypeId::of::<mads::InternalError>();
    let _ = std::any::TypeId::of::<mads::SourcedValidationIssue>();
    let _ = std::any::TypeId::of::<mads::ValidationSource>();
    let _ = std::any::TypeId::of::<mads::ValidatedJson<String>>();
    let _ = std::any::TypeId::of::<mads::ValidatedQuery<String>>();
    let _ = std::any::TypeId::of::<mads::ValidatedPath<String>>();
    let _ = mads::build_router;
    let _ = mads::configure_router;
    let _ = |application: mads::core::Mads| mads::serve(application, "127.0.0.1:0");
    let _ = |application: mads::core::Mads, router: mads::axum::Router| {
        mads::serve_router(application, router, "127.0.0.1:0")
    };
    let _ = <mads::core::Mads as mads::MadsRunExt>::run::<declarations::PreludeModule>;
    let _ = framework_result;
    let _: mads::common::axum::Router = mads::common::axum::Router::new();
}

#[cfg(feature = "cookies")]
#[tokio::test]
async fn prelude_exposes_cookie_types_through_native_axum() {
    use mads::axum::{
        Router,
        body::Body,
        http::{Request, header::SET_COOKIE},
        routing::get,
    };
    use mads::prelude::*;
    use tower::ServiceExt;

    async fn handler(jar: CookieJar) -> (CookieJar, &'static str) {
        let session = Cookie::build(("session", "opaque-token"))
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .max_age(cookie::time::Duration::minutes(5))
            .build();
        (jar.add(session), "ok")
    }

    let response = Router::new()
        .route("/session", get(handler))
        .oneshot(
            Request::builder()
                .uri("/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.headers().contains_key(SET_COOKIE));
    let _ = std::any::TypeId::of::<Expiration>();
    let _ = std::any::TypeId::of::<CookieError>();
    let _ = std::any::TypeId::of::<CookieErrorKind>();
    let _ = std::any::TypeId::of::<CookieRejection>();
    let _: CookieResult<()> = Ok(());
    assert_eq!(MADS110.as_str(), "MADS110");
    let _ = std::any::TypeId::of::<mads::Cookie<'static>>();
    let _ = std::any::TypeId::of::<mads::CookieJar>();
    let _: mads::cookie::time::Duration = cookie::time::Duration::seconds(1);
}

#[cfg(feature = "database")]
#[test]
fn prelude_exposes_the_database_runtime_surface() {
    use mads::diesel_migrations;
    use mads::prelude::{
        AutoConfigurationConfigEvidence, AutoConfigurationReasonCode, AutoConfigurationReport,
        AutoConfigurationRequirement, AutoConfigurationStatus, Database, DatabaseBootstrap,
        DatabaseConfig, DatabaseError, DatabaseErrorKind, DatabasePoolStatus, DatabaseResult,
        MadsBuilderDatabaseExt, MigrationReport, MigrationStatus,
    };

    let _ = std::any::TypeId::of::<Database>();
    let _ = std::any::TypeId::of::<DatabaseBootstrap>();
    let _ = std::any::TypeId::of::<DatabaseConfig>();
    let _ = std::any::TypeId::of::<DatabaseError>();
    let _ = std::any::TypeId::of::<DatabaseErrorKind>();
    let _ = std::any::TypeId::of::<DatabasePoolStatus>();
    let _ = std::any::TypeId::of::<DatabaseResult<()>>();
    let _ = std::any::TypeId::of::<MigrationReport>();
    let _ = std::any::TypeId::of::<MigrationStatus>();
    let _ = std::any::TypeId::of::<AutoConfigurationConfigEvidence>();
    let _ = std::any::TypeId::of::<AutoConfigurationReasonCode>();
    let _ = std::any::TypeId::of::<AutoConfigurationReport>();
    let _ = std::any::TypeId::of::<AutoConfigurationRequirement>();
    let _ = std::any::TypeId::of::<AutoConfigurationStatus>();

    fn needs_extension<T: MadsBuilderDatabaseExt>() {}
    needs_extension::<mads::core::MadsBuilder>();

    let _: std::marker::PhantomData<mads::diesel::pg::Pg> = std::marker::PhantomData;
    const MIGRATIONS: mads::diesel_migrations::EmbeddedMigrations =
        mads::diesel_migrations::embed_migrations!("tests/fixtures/empty_migrations");
    let _: mads::diesel_migrations::EmbeddedMigrations = MIGRATIONS;
    let mut builder = mads::core::Mads::builder();
    builder.database_migrations(MIGRATIONS).unwrap();

    let _ = std::any::TypeId::of::<mads::Database>();
    assert_eq!(mads::MADS100.as_str(), "MADS100");
    assert_eq!(mads::core::MADS007.as_str(), "MADS007");
    assert_eq!(mads::MADS101.as_str(), "MADS101");
}

#[test]
fn prelude_exposes_core_types_and_bare_attributes() {
    use mads::prelude::*;

    mod core_declarations {
        use mads::prelude::*;

        #[module]
        struct PreludeModule;

        #[provider]
        fn prelude_value() -> usize {
            1
        }

        #[repository]
        struct PreludeRepository;

        #[service]
        struct PreludeService;

        #[allow(dead_code)]
        #[routes(prefix = "/prelude")]
        trait PreludeRoutes {
            #[get("/")]
            async fn index(&self);
        }

        #[controller(routes = [PreludeRoutes])]
        struct PreludeController;

        impl PreludeRoutes for PreludeController {
            async fn index(&self) {}
        }

        #[cfg(feature = "runtime-tokio")]
        #[main]
        async fn main() {}
    }

    let _ = std::any::TypeId::of::<Mads>();
    let _ = std::any::TypeId::of::<Config>();
    let _ = std::any::TypeId::of::<Diagnostic>();
    let _ = std::any::TypeId::of::<Catalog>();
    let _ = std::any::TypeId::of::<LifecycleState>();
    let _: mads::core::MadsBuilder = Mads::builder();
    let _: mads::core::MadsBuilder = Mads::builder_with_config(Config::empty());
}

#[mads::module]
struct FacadeModule;

/// Public managed service used to verify facade visibility metadata.
#[mads::service]
pub struct PublicGraphService;

#[mads::provider]
pub(crate) fn restricted_graph_value() -> u16 {
    16
}

#[mads::repository]
struct FacadeRepository;

#[derive(Clone)]
struct Clock;

struct GroupedFallibleProvider;

#[mads::service]
struct QueryUsecase;

#[mads::service]
struct CommandUsecase;

#[mads::routes(prefix = "/users")]
trait QueryRoutes {
    #[mads::get("/:id")]
    async fn get_user(&self, id: Path<i64>) -> String;
}

#[mads::routes]
trait CommandRoutes {
    #[mads::post("/users")]
    async fn create_user(&self, id: Json<i64>) -> String;
}

#[mads::controller(routes = [QueryRoutes, CommandRoutes])]
struct FacadeController {
    query: QueryUsecase,
    command: CommandUsecase,
}

impl QueryRoutes for FacadeController {
    async fn get_user(&self, Path(id): Path<i64>) -> String {
        let _query = &self.query;
        id.to_string()
    }
}

impl CommandRoutes for FacadeController {
    async fn create_user(&self, Json(id): Json<i64>) -> String {
        let _command = &self.command;
        id.to_string()
    }
}

#[allow(clippy::result_large_err, unused_parens)]
#[mads::provider]
fn grouped_fallible_provider() -> (mads::core::Result<GroupedFallibleProvider>) {
    Ok(GroupedFallibleProvider)
}

#[mads::service]
struct FacadeService {
    repository: FacadeRepository,
    clock: Clock,
}

impl FacadeService {
    fn inner_address(&self) -> *const () {
        std::ptr::from_ref(&**self).cast()
    }

    fn has_dependencies(&self) -> bool {
        let _repository = &self.repository;
        let _clock = &self.clock;
        true
    }
}

#[test]
fn facade_attributes_register_stable_descriptors() {
    let module_names: Vec<_> = CoreCatalog::modules()
        .into_iter()
        .map(|descriptor| descriptor.type_name())
        .collect();
    let providers = CoreCatalog::providers();

    assert!(module_names.contains(&"facade::FacadeModule"));
    assert!(providers.iter().any(|descriptor| {
        descriptor.type_name() == "facade::FacadeRepository"
            && descriptor.kind() == ProviderKind::Repository
    }));
    assert!(providers.iter().any(|descriptor| {
        descriptor.type_name() == "facade::FacadeService"
            && descriptor.kind() == ProviderKind::Service
    }));
    assert!(providers.iter().any(|descriptor| {
        descriptor.type_name() == "facade::FacadeController"
            && descriptor.kind() == ProviderKind::Service
    }));
    assert_eq!(
        CoreCatalog::provider_for::<PublicGraphService>()
            .expect("public service descriptor should exist")
            .visibility(),
        ProviderVisibility::Public,
    );
    assert_eq!(
        CoreCatalog::provider_for::<u16>()
            .expect("restricted provider descriptor should exist")
            .visibility(),
        ProviderVisibility::Private,
    );
}

#[test]
fn controller_dependencies_follow_source_field_order() {
    let descriptor = CoreCatalog::provider_for::<FacadeController>()
        .expect("the facade controller descriptor should be registered");
    let dependency_names: Vec<_> = descriptor
        .dependencies()
        .iter()
        .map(|dependency| dependency.type_name())
        .collect();

    assert_eq!(dependency_names, ["QueryUsecase", "CommandUsecase"]);
}

#[test]
fn controller_keeps_deterministic_route_metadata() {
    let routes = RouteCatalog::routes_for::<FacadeController>();

    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].method(), HttpMethod::Get);
    assert_eq!(routes[0].prefix(), "/users");
    assert_eq!(routes[0].path(), "/:id");
    assert_eq!(routes[0].full_path(), "/users/:id");
    assert_eq!(routes[0].handler(), "get_user");
    assert_eq!(routes[1].method(), HttpMethod::Post);
    assert_eq!(routes[1].full_path(), "/users");
    assert!(RouteCatalog::validate_controller::<FacadeController>().is_ok());
}

#[allow(dead_code)]
#[mads::routes(prefix = "/")]
trait RootRoutes {
    #[mads::get("/health")]
    async fn health(&self);
}

#[mads::controller(routes = [RootRoutes])]
struct RootController;

impl RootRoutes for RootController {
    async fn health(&self) {}
}

#[test]
fn root_prefix_does_not_create_an_ambiguous_double_slash_path() {
    let routes = RouteCatalog::routes_for::<RootController>();
    assert_eq!(routes[0].full_path(), "/health");
}

#[test]
fn service_dependencies_follow_source_field_order() {
    let descriptor = CoreCatalog::provider_for::<FacadeService>()
        .expect("the facade service descriptor should be registered");
    let dependency_names: Vec<_> = descriptor
        .dependencies()
        .iter()
        .map(|dependency| dependency.type_name())
        .collect();

    assert_eq!(dependency_names, ["FacadeRepository", "Clock"]);
}

#[tokio::test]
async fn cloned_service_handles_share_the_inner_allocation() {
    let mut builder = CoreMads::builder();
    builder
        .provide(Clock)
        .expect("clock insertion should succeed");

    let application = builder
        .build()
        .await
        .expect("the application graph should build");
    let service = application
        .context()
        .resolve::<FacadeService>()
        .expect("the constructed service should resolve");
    let graph_service = application
        .graph()
        .provider::<FacadeService>()
        .expect("the facade service should be in the graph");
    assert_eq!(graph_service.origin(), ProviderOrigin::Service);
    assert_eq!(graph_service.visibility(), ProviderVisibility::Private);
    assert!(application.graph().dependencies().iter().any(|edge| {
        edge.provider_type_name().ends_with("FacadeService")
            && edge.dependency_type_name().ends_with("FacadeRepository")
    }));
    let cloned = service.as_ref().clone();

    assert!(service.has_dependencies());
    assert_eq!(service.inner_address(), cloned.inner_address());
    assert!(Arc::ptr_eq(
        &service,
        &application.context().resolve().unwrap()
    ));
}

#[tokio::test]
async fn controller_constructs_after_multiple_usecases() {
    let mut builder = CoreMads::builder();
    builder
        .provide(Clock)
        .expect("clock insertion should succeed");
    builder
        .construct::<QueryUsecase>()
        .await
        .expect("query use case construction should succeed");
    builder
        .construct::<CommandUsecase>()
        .await
        .expect("command use case construction should succeed");
    builder
        .construct::<FacadeController>()
        .await
        .expect("controller construction should succeed");

    let application = builder
        .build()
        .await
        .expect("the application graph should build");
    let controller = application
        .context()
        .resolve::<FacadeController>()
        .expect("the constructed controller should resolve");
    let cloned = controller.as_ref().clone();

    assert_eq!(controller.get_user(Path(7)).await, "7");
    assert_eq!(controller.create_user(Json(8)).await, "8");
    assert!(std::ptr::eq(&**controller, &*cloned));
}

#[tokio::test]
async fn grouped_mads_results_register_their_success_type() {
    let mut builder = CoreMads::builder();
    builder
        .provide(Clock)
        .expect("clock insertion should succeed");

    builder
        .construct::<GroupedFallibleProvider>()
        .await
        .expect("a grouped MADS Result provider should construct its success type");

    let application = builder
        .build()
        .await
        .expect("the application graph should build");
    assert!(
        application
            .context()
            .resolve::<GroupedFallibleProvider>()
            .is_ok()
    );
}
