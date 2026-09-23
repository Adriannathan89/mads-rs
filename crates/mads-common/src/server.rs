//! Validated HTTP server startup and lifecycle coordination.
//!
//! [`serve`] validates route metadata and builds a raw generated router, while
//! [`serve_router`] accepts a complete raw router from a caller. Both finalize
//! router configuration before they start application lifecycle hooks or ask
//! Tokio to bind a listener.

use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mads_core::{Diagnostic, DiagnosticCode, Error, MADS020, Mads, Module};
use tokio::net::TcpListener;

use crate::cors::CORS_AUTO_CONFIGURATION_ID;
use crate::http_scope::HttpApplicationScope;
use crate::server_config::{
    HttpRuntimeMode, SERVER_AUTO_CONFIGURATION_ID, ServerBinding, load_standard_config_from,
};
use crate::{build_router, configure_router};

/// A standard application run had no reachable managed HTTP route.
pub const MADS031: DiagnosticCode = DiagnosticCode::new("MADS031");

/// An error produced while preparing, running, or stopping the HTTP runtime.
#[derive(Debug)]
#[non_exhaustive]
pub enum HttpRuntimeError {
    /// Route validation or final router configuration failed before lifecycle startup.
    Bootstrap(mads_core::Error),
    /// Application lifecycle startup or shutdown failed.
    Lifecycle(mads_core::Error),
    /// The HTTP listener could not bind to the requested address.
    Bind(std::io::Error),
    /// Axum stopped because of an HTTP serving failure.
    Serve(std::io::Error),
    /// An operational failure occurred and the subsequent shutdown also failed.
    OperationAndShutdown {
        /// The bind or serving error that initiated shutdown.
        operation: Box<HttpRuntimeError>,
        /// The lifecycle error returned while attempting shutdown.
        shutdown: mads_core::Error,
    },
}

impl fmt::Display for HttpRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bootstrap(error) => write!(formatter, "HTTP bootstrap failed: {error}"),
            Self::Lifecycle(error) => write!(formatter, "application lifecycle failed: {error}"),
            Self::Bind(error) => write!(formatter, "HTTP listener bind failed: {error}"),
            Self::Serve(error) => write!(formatter, "HTTP serving failed: {error}"),
            Self::OperationAndShutdown {
                operation,
                shutdown,
            } => write!(
                formatter,
                "{operation}; application shutdown also failed: {shutdown}"
            ),
        }
    }
}

impl StdError for HttpRuntimeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Bootstrap(error) | Self::Lifecycle(error) => Some(error),
            Self::Bind(error) | Self::Serve(error) => Some(error),
            Self::OperationAndShutdown { operation, .. } => Some(operation.as_ref()),
        }
    }
}

/// Runs a rooted application module with conventional HTTP configuration.
///
/// Import this trait to call [`Mads::run`]. The standard path loads optional
/// `.env` and `mads.toml` files from the current working directory, applies
/// `MADS_*` environment overrides, and owns automatic HTTP binding. Use the
/// low-level [`Mads::builder`] and [`serve`] APIs when the application needs
/// explicit configuration, providers, lifecycle hooks, or a listener address.
pub trait MadsRunExt {
    /// Builds and runs the selected rooted application module.
    fn run<M>() -> impl Future<Output = Result<(), HttpRuntimeError>> + Send
    where
        M: Module;
}

impl MadsRunExt for Mads {
    #[allow(clippy::manual_async_fn)] // The public trait keeps an explicit `Send` future.
    fn run<M>() -> impl Future<Output = Result<(), HttpRuntimeError>> + Send
    where
        M: Module,
    {
        async move {
            let root = std::env::current_dir().map_err(config_directory_error)?;
            if let Some(result) = crate::inspection::try_run_inspection::<M>(&root) {
                return result;
            }
            let prepared = prepare_standard_run::<M>(&root).await?;
            println!("{}", prepared.startup_summary());
            serve_prepared(prepared, TcpListener::bind, shutdown_signal()).await
        }
    }
}

struct PreparedStandardRun {
    application: Mads,
    router: axum::Router,
    binding: std::sync::Arc<ServerBinding>,
    route_count: usize,
}

struct StartupSummary {
    host: String,
    port: u16,
    route_count: usize,
}

impl StartupSummary {
    const fn new(host: String, port: u16, route_count: usize) -> Self {
        Self {
            host,
            port,
            route_count,
        }
    }
}

impl fmt::Display for StartupSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "MADS application ready\nserver: http://{}:{}\nroutes: {}",
            self.host, self.port, self.route_count
        )
    }
}

impl PreparedStandardRun {
    fn startup_summary(&self) -> StartupSummary {
        StartupSummary::new(
            self.binding.host().to_owned(),
            self.binding.port(),
            self.route_count,
        )
    }
}

async fn prepare_standard_run<M: Module>(
    root: &Path,
) -> Result<PreparedStandardRun, HttpRuntimeError> {
    let config = load_standard_config_from(root).map_err(HttpRuntimeError::Bootstrap)?;
    let mut builder = Mads::builder_with_config(config);
    builder.root::<M>().map_err(HttpRuntimeError::Bootstrap)?;
    let server_input_registered = builder
        .__auto_configuration_input(SERVER_AUTO_CONFIGURATION_ID, HttpRuntimeMode::Automatic);
    debug_assert!(server_input_registered);
    let cors_input_registered =
        builder.__auto_configuration_input(CORS_AUTO_CONFIGURATION_ID, HttpRuntimeMode::Automatic);
    debug_assert!(cors_input_registered);
    let application = builder.build().await.map_err(HttpRuntimeError::Bootstrap)?;

    prepare_standard_application(application)
}

fn prepare_standard_application(
    application: Mads,
) -> Result<PreparedStandardRun, HttpRuntimeError> {
    let scope =
        HttpApplicationScope::for_application(&application).map_err(HttpRuntimeError::Bootstrap)?;
    if !scope.has_routes() {
        return Err(HttpRuntimeError::Bootstrap(no_runnable_route_error()));
    }
    let route_count = scope.route_records().count();

    let router = build_router(&application).map_err(HttpRuntimeError::Bootstrap)?;
    let router = configure_router(&application, router).map_err(HttpRuntimeError::Bootstrap)?;
    let binding = application
        .context()
        .resolve::<ServerBinding>()
        .map_err(HttpRuntimeError::Bootstrap)?;

    Ok(PreparedStandardRun {
        application,
        router,
        binding,
        route_count,
    })
}

async fn serve_prepared<B, BindFuture, Shutdown>(
    prepared: PreparedStandardRun,
    binder: B,
    shutdown: Shutdown,
) -> Result<(), HttpRuntimeError>
where
    B: FnOnce((String, u16)) -> BindFuture,
    BindFuture: Future<Output = std::io::Result<TcpListener>>,
    Shutdown: Future<Output = ()> + Send + 'static,
{
    let PreparedStandardRun {
        application,
        router,
        binding,
        route_count: _,
    } = prepared;
    let address = (binding.host().to_owned(), binding.port());
    serve_configured_router_with(application, router, address, binder, shutdown).await
}

fn no_runnable_route_error() -> Error {
    Error::new(
        Diagnostic::new(
            MADS031,
            "no runnable HTTP route",
            "the selected application has no reachable managed HTTP route",
        )
        .with_subject("HTTP runtime")
        .with_suggestion("declare a managed controller route reachable from the root module"),
    )
}

fn config_directory_error(error: std::io::Error) -> HttpRuntimeError {
    HttpRuntimeError::Bootstrap(Error::with_source(
        Diagnostic::new(
            MADS020,
            "configuration directory could not be determined",
            "could not determine the process current working directory",
        )
        .with_subject("current working directory")
        .with_suggestion("run the application from an accessible directory"),
        error,
    ))
}

/// Builds, configures, starts, serves, and shuts down an application on `address`.
///
/// Generated-route validation and final router configuration complete before
/// lifecycle hooks start or the listener is bound. Once lifecycle startup
/// succeeds, every exit path attempts shutdown.
/// A bind or serving failure is retained if shutdown succeeds; if shutdown
/// also fails, both failures are returned in [`HttpRuntimeError::OperationAndShutdown`].
///
/// # Errors
///
/// Returns [`HttpRuntimeError::Bootstrap`] for route validation, controller
/// resolution, or registrar failures; [`HttpRuntimeError::Lifecycle`] for
/// lifecycle start or clean-shutdown failures; [`HttpRuntimeError::Bind`] when
/// the address cannot be bound; [`HttpRuntimeError::Serve`] for an Axum serving
/// failure; or [`HttpRuntimeError::OperationAndShutdown`] when an operational
/// failure and its cleanup failure occur together.
///
/// # Examples
///
/// ```no_run
/// use mads_common::{core::Mads, serve};
///
/// #[tokio::main]
/// async fn main() -> Result<(), mads_common::HttpRuntimeError> {
///     let application = Mads::builder().build().await.map_err(
///         mads_common::HttpRuntimeError::Bootstrap,
///     )?;
///     serve(application, "127.0.0.1:3000").await
/// }
/// ```
#[allow(clippy::result_large_err)]
pub async fn serve(
    application: Mads,
    address: impl tokio::net::ToSocketAddrs,
) -> Result<(), HttpRuntimeError> {
    let router = build_router(&application).map_err(HttpRuntimeError::Bootstrap)?;
    serve_router(application, router, address).await
}

/// Configures, starts, serves, and shuts down an application with a complete raw router.
///
/// Pass the raw router after merging any generated and native routes. This
/// function applies final application-wide configuration, including CORS, once
/// before lifecycle startup. Call [`crate::configure_router`] only when using a
/// router directly; passing an already configured router here would apply that
/// configuration twice.
///
/// The explicit `address` is the complete listener override. It is resolved
/// and bound after lifecycle startup, and it may use port zero regardless of
/// any automatic `server.host` or `server.port` configuration.
///
/// # Errors
///
/// Returns [`HttpRuntimeError::Bootstrap`] when final router configuration
/// fails before lifecycle startup. Lifecycle, bind, serving, and combined
/// operational/shutdown errors follow the same contract as [`serve`].
#[allow(clippy::result_large_err)]
pub async fn serve_router(
    application: Mads,
    router: axum::Router,
    address: impl tokio::net::ToSocketAddrs,
) -> Result<(), HttpRuntimeError> {
    serve_router_with(
        application,
        router,
        address,
        TcpListener::bind,
        shutdown_signal(),
    )
    .await
}

#[cfg(test)]
async fn serve_with<Address, B, BindFuture, Shutdown>(
    application: Mads,
    address: Address,
    binder: B,
    shutdown: Shutdown,
) -> Result<(), HttpRuntimeError>
where
    B: FnOnce(Address) -> BindFuture,
    BindFuture: Future<Output = std::io::Result<TcpListener>>,
    Shutdown: Future<Output = ()> + Send + 'static,
{
    let router = build_router(&application).map_err(HttpRuntimeError::Bootstrap)?;
    serve_router_with(application, router, address, binder, shutdown).await
}

async fn serve_router_with<Address, B, BindFuture, Shutdown>(
    application: Mads,
    router: axum::Router,
    address: Address,
    binder: B,
    shutdown: Shutdown,
) -> Result<(), HttpRuntimeError>
where
    B: FnOnce(Address) -> BindFuture,
    BindFuture: Future<Output = std::io::Result<TcpListener>>,
    Shutdown: Future<Output = ()> + Send + 'static,
{
    let router = configure_router(&application, router).map_err(HttpRuntimeError::Bootstrap)?;
    serve_configured_router_with(application, router, address, binder, shutdown).await
}

async fn serve_configured_router_with<Address, B, BindFuture, Shutdown>(
    mut application: Mads,
    router: axum::Router,
    address: Address,
    binder: B,
    shutdown: Shutdown,
) -> Result<(), HttpRuntimeError>
where
    B: FnOnce(Address) -> BindFuture,
    BindFuture: Future<Output = std::io::Result<TcpListener>>,
    Shutdown: Future<Output = ()> + Send + 'static,
{
    application
        .start()
        .await
        .map_err(HttpRuntimeError::Lifecycle)?;
    let listener = match binder(address).await {
        Ok(listener) => listener,
        Err(error) => {
            return finish_after_error(application, HttpRuntimeError::Bind(error)).await;
        }
    };
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(HttpRuntimeError::Serve);
    finish(application, result).await
}

async fn finish(
    mut application: Mads,
    operation: Result<(), HttpRuntimeError>,
) -> Result<(), HttpRuntimeError> {
    match (operation, application.shutdown().await) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(shutdown)) => Err(HttpRuntimeError::Lifecycle(shutdown)),
        (Err(operation), Ok(())) => Err(operation),
        (Err(operation), Err(shutdown)) => Err(HttpRuntimeError::OperationAndShutdown {
            operation: Box::new(operation),
            shutdown,
        }),
    }
}

async fn finish_after_error(
    application: Mads,
    operation: HttpRuntimeError,
) -> Result<(), HttpRuntimeError> {
    finish(application, Err(operation)).await
}

async fn shutdown_signal() {
    let Some(path) = std::env::var_os(crate::__private::DEV_SHUTDOWN_ENV).map(PathBuf::from) else {
        os_shutdown_signal().await;
        return;
    };

    tokio::select! {
        _ = os_shutdown_signal() => {}
        _ = wait_for_dev_shutdown(path) => {}
    }
}

async fn os_shutdown_signal() {
    #[cfg(unix)]
    if let Ok(mut terminate) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
        return;
    }

    let _ = tokio::signal::ctrl_c().await;
}

async fn wait_for_dev_shutdown(path: PathBuf) {
    loop {
        if tokio::fs::metadata(&path)
            .await
            .is_ok_and(|metadata| metadata.is_file())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    #[cfg(feature = "database")]
    use std::error::Error as StdError;
    use std::io::{self, Read, Write};
    use std::net::{Ipv4Addr, SocketAddr};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use mads_core::{
        ApplicationContext, AutoConfigurationStatus, ConfigBuilder, Diagnostic, Error,
        LifecycleFuture, LifecycleHook, MADS011, MADS020, Mads, MapSource, Module, SourceLocation,
    };
    use tokio::net::TcpListener;

    use super::{
        HttpRuntimeError, MADS031, StartupSummary, prepare_standard_application,
        prepare_standard_run, serve_prepared, serve_router_with, serve_with, wait_for_dev_shutdown,
    };
    use crate::cors::CORS_AUTO_CONFIGURATION_ID;
    use crate::server_config::{HttpRuntimeMode, SERVER_AUTO_CONFIGURATION_ID, ServerBinding};
    use crate::{ControllerRouteDescriptor, HttpMethod, RouteContractDescriptor, RouteDescriptor};
    #[cfg(feature = "database")]
    use crate::{Database, DatabaseConfig, DatabaseErrorKind, MADS100, MadsBuilderDatabaseExt};

    #[cfg(feature = "database")]
    const FAILING_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
        diesel_migrations::embed_migrations!("tests/fixtures/failing_migrations");

    static STARTS: AtomicUsize = AtomicUsize::new(0);
    static BINDS: AtomicUsize = AtomicUsize::new(0);
    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn private_shutdown_file_completes_the_standard_shutdown_signal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shutdown");
        let mut waiting = tokio::spawn(wait_for_dev_shutdown(path.clone()));

        assert!(
            tokio::time::timeout(Duration::from_millis(110), &mut waiting)
                .await
                .is_err()
        );

        tokio::fs::write(&path, b"stop").await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("shutdown file should be observed")
            .unwrap();
    }

    #[mads_core::module]
    struct ServerTestApp;

    mod raw_router {
        #[mads_core::module]
        pub(super) struct App;
    }

    mod standard_run {
        pub(super) mod routed {
            #[mads_common_macros::routes]
            pub(super) trait RoutedRoutes {
                #[mads_common_macros::get("/standard-run-health")]
                async fn health(&self) -> &'static str;
            }

            #[mads_common_macros::controller(routes = [RoutedRoutes])]
            pub(super) struct RoutedController;

            impl RoutedRoutes for RoutedController {
                async fn health(&self) -> &'static str {
                    "healthy"
                }
            }

            #[mads_core::module]
            pub struct RoutedApp;
        }

        pub(super) mod empty {
            #[mads_core::module]
            pub struct EmptyApp;
        }
    }

    #[cfg(feature = "database")]
    mod unreachable_database {
        use super::*;

        #[mads_core::repository]
        pub(super) struct UnreachableRepository {
            _database: Database,
        }

        #[mads_core::module]
        pub(super) struct UnreachableDatabaseModule;
    }

    #[cfg(feature = "jwt")]
    mod unreachable_jwt {
        use crate::{ClaimsPrincipal, PassportPrincipal};

        #[derive(serde::Deserialize)]
        pub(super) struct UnreachableClaims;

        impl PassportPrincipal for UnreachableClaims {
            fn has_role(&self, _: &str) -> bool {
                false
            }

            fn has_permission(&self, _: &str) -> bool {
                false
            }
        }

        #[mads_common_macros::routes]
        #[mads_common_macros::guard(
            strategy = "jwt",
            principal = ClaimsPrincipal<UnreachableClaims>
        )]
        pub(super) trait UnreachableRoutes {
            #[mads_common_macros::get("/unreachable")]
            async fn unreachable(&self) -> &'static str;
        }

        #[mads_common_macros::controller(routes = [UnreachableRoutes])]
        pub(super) struct UnreachableController;

        impl UnreachableRoutes for UnreachableController {
            async fn unreachable(&self) -> &'static str {
                "unreachable"
            }
        }

        #[mads_core::module]
        pub(super) struct UnreachableJwtModule;
    }

    struct PreflightController;
    struct PreflightPermit;

    #[derive(Clone)]
    struct RouterPreflightEvents(Arc<Mutex<Vec<&'static str>>>);

    #[cfg(feature = "database")]
    mod auto_database_repository {
        use super::*;

        #[mads_core::repository]
        pub(super) struct AutoDatabaseRepository {
            database: Database,
        }

        impl AutoDatabaseRepository {
            pub(super) fn database(&self) -> &Database {
                &self.database
            }
        }
    }

    #[cfg(feature = "database")]
    use auto_database_repository::AutoDatabaseRepository;

    fn preflight_controller_type_id() -> TypeId {
        TypeId::of::<PreflightController>()
    }

    fn preflight_registrar(
        router: axum::Router,
        context: &crate::__private::RouterBuildContext<'_>,
        routes: &mut crate::__private::ValidatedRouteIter<'_>,
    ) -> mads_core::Result<axum::Router> {
        let _ = context.application().resolve::<PreflightPermit>()?;
        context
            .application()
            .resolve::<RouterPreflightEvents>()?
            .0
            .lock()
            .unwrap()
            .push("router_preflight");
        let Some(path) = routes.next(HttpMethod::Get, "health")? else {
            routes.finish()?;
            return Ok(router);
        };
        routes.finish()?;
        Ok(router.route(path, axum::routing::get(|| async { "ok" })))
    }

    mads_core::__private::inventory::submit! {
        ControllerRouteDescriptor::with_registrar(
            "server_tests::PreflightController",
            preflight_controller_type_id,
            &[RouteContractDescriptor::new(
                "HealthRoutes",
                &[RouteDescriptor::new(
                    HttpMethod::Get,
                    "",
                    "/health",
                    "/health",
                    "health",
                    SourceLocation::new(file!(), line!(), column!()),
                )],
            )],
            preflight_registrar,
        )
        .with_namespace(module_path!())
    }

    struct RecordingHook {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_shutdown: bool,
    }

    impl LifecycleHook for RecordingHook {
        fn name(&self) -> &str {
            "server-test"
        }

        fn start<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
            Box::pin(async move {
                STARTS.fetch_add(1, Ordering::SeqCst);
                self.events.lock().unwrap().push("lifecycle_start");
                Ok(())
            })
        }

        fn stop<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
            Box::pin(async move {
                self.events.lock().unwrap().push("lifecycle_stop");
                if self.fail_shutdown {
                    Err(test_core_error("shutdown failed"))
                } else {
                    Ok(())
                }
            })
        }
    }

    fn test_core_error(message: &str) -> Error {
        Error::new(Diagnostic::new(MADS020, "server test failure", message))
    }

    async fn application(
        events: Arc<Mutex<Vec<&'static str>>>,
        preflight_permitted: bool,
        fail_shutdown: bool,
    ) -> Mads {
        let mut builder = Mads::builder();
        builder.root::<ServerTestApp>().unwrap();
        #[cfg(feature = "database")]
        builder
            .provide(
                Database::from_config(
                    &DatabaseConfig::new("postgres://127.0.0.1:1/server-test").unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let router_preflight_events = Arc::clone(&events);
        builder.lifecycle_hook(RecordingHook {
            events,
            fail_shutdown,
        });
        builder
            .provide(RouterPreflightEvents(router_preflight_events))
            .unwrap();
        if preflight_permitted {
            builder.provide(PreflightPermit).unwrap();
        }
        builder.build().await.unwrap()
    }

    fn address() -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
    }

    fn automatic_standard_builder<M: Module>(host: &str) -> mads_core::MadsBuilder {
        let config = ConfigBuilder::new()
            .source(MapSource::new(
                "test",
                [("server.host", host), ("server.port", "3000")],
            ))
            .build()
            .unwrap();
        let mut builder = Mads::builder_with_config(config);
        builder.root::<M>().unwrap();
        assert!(
            builder.__auto_configuration_input(
                SERVER_AUTO_CONFIGURATION_ID,
                HttpRuntimeMode::Automatic,
            )
        );
        assert!(builder.__auto_configuration_input(
            CORS_AUTO_CONFIGURATION_ID,
            HttpRuntimeMode::Automatic,
        ));
        builder
    }

    fn automatic_report<'a>(
        application: &'a Mads,
        identifier: &str,
    ) -> &'a mads_core::AutoConfigurationReport {
        application
            .auto_configurations()
            .iter()
            .find(|report| report.identifier() == identifier)
            .expect("the automatic HTTP configuration must be registered")
    }

    #[tokio::test]
    async fn standard_run_preparation_roots_and_configures_a_routed_application() {
        let directory = tempfile::tempdir().unwrap();

        let prepared = prepare_standard_run::<standard_run::routed::RoutedApp>(directory.path())
            .await
            .unwrap();

        assert_eq!(
            prepared
                .application
                .module_graph()
                .unwrap()
                .root()
                .type_name(),
            std::any::type_name::<standard_run::routed::RoutedApp>(),
        );
        assert_eq!(prepared.binding.host(), "127.0.0.1");
        assert_eq!(prepared.binding.port(), 3000);
        assert_eq!(
            automatic_report(&prepared.application, SERVER_AUTO_CONFIGURATION_ID).status(),
            AutoConfigurationStatus::Active,
        );
        assert_eq!(prepared.route_count, 1);
    }

    #[test]
    fn startup_summary_formats_owned_binding_and_validated_route_count() {
        let summary = StartupSummary::new("api.internal".into(), 4321, 7);

        assert_eq!(
            summary.to_string(),
            "MADS application ready\nserver: http://api.internal:4321\nroutes: 7"
        );
    }

    #[tokio::test]
    async fn standard_run_preparation_rejects_roots_without_reachable_routes() {
        let directory = tempfile::tempdir().unwrap();

        let error =
            match prepare_standard_run::<standard_run::empty::EmptyApp>(directory.path()).await {
                Ok(_) => panic!("a root without routes must not be runnable"),
                Err(error) => error,
            };

        match error {
            HttpRuntimeError::Bootstrap(error) => {
                assert_eq!(error.code(), MADS031);
                assert!(error.to_string().contains("no runnable HTTP route"));
            }
            other => panic!("expected bootstrap error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn standard_run_preparation_redacts_malformed_conventional_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let sentinel = "postgres://secret.example/standard-run";
        std::fs::write(
            directory.path().join("mads.toml"),
            format!("[server\nport = \"{sentinel}\"\n"),
        )
        .unwrap();

        let error =
            match prepare_standard_run::<standard_run::routed::RoutedApp>(directory.path()).await {
                Ok(_) => panic!("malformed conventional configuration must fail preparation"),
                Err(error) => error,
            };

        match error {
            HttpRuntimeError::Bootstrap(error) => {
                assert_eq!(error.code(), MADS020);
                assert!(!error.to_string().contains(sentinel));
            }
            other => panic!("expected bootstrap error, got {other:?}"),
        }
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn standard_run_preparation_ignores_unreachable_database_requirements() {
        let directory = tempfile::tempdir().unwrap();

        let prepared = prepare_standard_run::<standard_run::routed::RoutedApp>(directory.path())
            .await
            .unwrap();

        assert_eq!(
            automatic_report(&prepared.application, "mads.common.database.diesel").status(),
            AutoConfigurationStatus::Skipped,
        );
    }

    #[cfg(feature = "jwt")]
    #[tokio::test]
    async fn standard_run_preparation_ignores_unreachable_jwt_requirements() {
        let directory = tempfile::tempdir().unwrap();

        let prepared = prepare_standard_run::<standard_run::routed::RoutedApp>(directory.path())
            .await
            .unwrap();

        assert_eq!(
            automatic_report(&prepared.application, "mads.common.passport.jwt").status(),
            AutoConfigurationStatus::Skipped,
        );
    }

    #[tokio::test]
    async fn standard_run_bind_failure_starts_then_stops_lifecycle_once() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut builder =
            automatic_standard_builder::<standard_run::routed::RoutedApp>("api.internal");
        builder.lifecycle_hook(RecordingHook {
            events: Arc::clone(&events),
            fail_shutdown: false,
        });
        let prepared = prepare_standard_application(builder.build().await.unwrap()).unwrap();
        let binder_events = Arc::clone(&events);
        let binder = move |(host, port): (String, u16)| async move {
            assert_eq!((host.as_str(), port), ("api.internal", 3000));
            binder_events.lock().unwrap().push("bind");
            Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "unavailable",
            ))
        };

        let error = serve_prepared(prepared, binder, async {})
            .await
            .unwrap_err();

        assert!(matches!(error, HttpRuntimeError::Bind(_)));
        assert_eq!(STARTS.load(Ordering::SeqCst), 1);
        assert_eq!(
            *events.lock().unwrap(),
            ["lifecycle_start", "bind", "lifecycle_stop"]
        );
    }

    #[tokio::test]
    async fn standard_run_bind_and_shutdown_failures_are_both_retained() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut builder =
            automatic_standard_builder::<standard_run::routed::RoutedApp>("api.internal");
        builder.lifecycle_hook(RecordingHook {
            events: Arc::clone(&events),
            fail_shutdown: true,
        });
        let prepared = prepare_standard_application(builder.build().await.unwrap()).unwrap();
        let binder_events = Arc::clone(&events);
        let binder = move |(host, port): (String, u16)| async move {
            assert_eq!((host.as_str(), port), ("api.internal", 3000));
            binder_events.lock().unwrap().push("bind");
            Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "unavailable",
            ))
        };

        let error = serve_prepared(prepared, binder, async {})
            .await
            .unwrap_err();

        match error {
            HttpRuntimeError::OperationAndShutdown {
                operation,
                shutdown,
            } => {
                assert!(matches!(*operation, HttpRuntimeError::Bind(_)));
                assert_eq!(shutdown.code(), MADS011);
            }
            other => panic!("expected combined failure, got {other:?}"),
        }
        assert_eq!(STARTS.load(Ordering::SeqCst), 1);
        assert_eq!(
            *events.lock().unwrap(),
            ["lifecycle_start", "bind", "lifecycle_stop"]
        );
    }

    #[test]
    fn ipv6_automatic_server_binding_uses_a_host_port_tuple() {
        let config = ConfigBuilder::new()
            .source(MapSource::new(
                "test",
                [("server.host", "::1"), ("server.port", "3000")],
            ))
            .build()
            .unwrap();
        let binding = ServerBinding::from_config(&config).unwrap();
        let address = binding.address();

        accepts_tokio_socket_address(address);
        assert_eq!(address, ("::1", 3000));
        assert_eq!(format!("{binding:?}"), "[REDACTED]");
    }

    fn accepts_tokio_socket_address(_: impl tokio::net::ToSocketAddrs) {}

    #[tokio::test]
    async fn preflight_failure_prevents_lifecycle_start_and_bind() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        BINDS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let application = application(Arc::clone(&events), false, false).await;
        let binder = |address| async move {
            BINDS.fetch_add(1, Ordering::SeqCst);
            tokio::net::TcpListener::bind(address).await
        };

        let error = serve_with(application, address(), binder, async {})
            .await
            .unwrap_err();

        assert!(matches!(error, HttpRuntimeError::Bootstrap(_)));
        assert_eq!(STARTS.load(Ordering::SeqCst), 0);
        assert_eq!(BINDS.load(Ordering::SeqCst), 0);
        assert!(events.lock().unwrap().is_empty());
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn database_start_failure_prevents_listener_binding() {
        let _guard = TEST_LOCK.lock().await;
        let database_url = "postgres://127.0.0.1:1/mads";
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ConfigBuilder::new()
            .source(MapSource::new("test", [("database.url", database_url)]))
            .build()
            .unwrap();
        let mut builder = Mads::builder_with_config(config);
        builder.root::<ServerTestApp>().unwrap();
        builder.provide(PreflightPermit).unwrap();
        builder
            .provide(RouterPreflightEvents(Arc::clone(&events)))
            .unwrap();
        let application = builder.build().await.unwrap();
        assert_eq!(
            application.auto_configurations()[0].status(),
            AutoConfigurationStatus::Active,
        );
        BINDS.store(0, Ordering::SeqCst);

        let error = serve_with(
            application,
            address(),
            |_| async {
                BINDS.fetch_add(1, Ordering::SeqCst);
                tokio::net::TcpListener::bind(address()).await
            },
            async {},
        )
        .await
        .unwrap_err();

        match &error {
            HttpRuntimeError::Lifecycle(error) => {
                assert_eq!(error.code(), MADS011);
                let source = std::error::Error::source(error)
                    .unwrap()
                    .downcast_ref::<Error>()
                    .unwrap();
                assert_eq!(source.code(), MADS100);
            }
            other => panic!("expected lifecycle failure, got {other:?}"),
        }
        assert_eq!(BINDS.load(Ordering::SeqCst), 0);
        assert_eq!(*events.lock().unwrap(), ["router_preflight"]);
        let output = format!("{error}\n{error:?}");
        assert!(!output.contains(database_url));
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    #[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
    async fn database_migration_failure_prevents_listener_binding() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        BINDS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let database_url = std::env::var("MADS_TEST_DATABASE_URL")
            .expect("MADS_TEST_DATABASE_URL is required for ignored PostgreSQL tests");
        let config = ConfigBuilder::new()
            .source(MapSource::new(
                "test",
                [
                    ("database.url", database_url.clone()),
                    ("database.migrate", "true".to_owned()),
                ],
            ))
            .build()
            .unwrap();
        let mut builder = Mads::builder_with_config(config);
        builder.root::<ServerTestApp>().unwrap();
        builder.provide(PreflightPermit).unwrap();
        builder
            .provide(RouterPreflightEvents(Arc::clone(&events)))
            .unwrap();
        builder.lifecycle_hook(RecordingHook {
            events: Arc::clone(&events),
            fail_shutdown: false,
        });
        builder.database_migrations(FAILING_MIGRATIONS).unwrap();
        let application = builder.build().await.unwrap();
        let database = application.context().resolve::<Database>().unwrap();

        let error = serve_with(
            application,
            address(),
            |address| async move {
                BINDS.fetch_add(1, Ordering::SeqCst);
                TcpListener::bind(address).await
            },
            async {},
        )
        .await
        .unwrap_err();

        match &error {
            HttpRuntimeError::Lifecycle(error) => {
                assert_eq!(error.code(), MADS011);
                let bootstrap_error = StdError::source(error)
                    .expect("MADS011 lifecycle errors retain their database bootstrap source")
                    .downcast_ref::<Error>()
                    .expect("MADS011 source is the database bootstrap error");
                assert_eq!(bootstrap_error.code(), MADS100);
                let database_source = StdError::source(bootstrap_error)
                    .expect("MADS100 errors retain their database error source");
                assert_eq!(
                    database_source.to_string(),
                    "database migration failed",
                    "the source chain must retain DatabaseErrorKind::{:?}",
                    DatabaseErrorKind::Migration,
                );
                assert!(
                    format!("{database_source:?}")
                        .contains(&format!("{:?}", DatabaseErrorKind::Migration))
                );
            }
            other => panic!("expected lifecycle failure, got {other:?}"),
        }
        assert_eq!(STARTS.load(Ordering::SeqCst), 0);
        assert_eq!(BINDS.load(Ordering::SeqCst), 0);
        assert_eq!(*events.lock().unwrap(), ["router_preflight"]);
        assert!(database.is_closed());
        let output = format!("{error}\n{error:?}");
        assert!(!output.contains(&database_url));
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn invalid_routes_prevent_automatic_database_checkout_and_binding() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        BINDS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ConfigBuilder::new()
            .source(MapSource::new(
                "test",
                [("database.url", "postgres://127.0.0.1:1/mads")],
            ))
            .build()
            .unwrap();
        let mut builder = Mads::builder_with_config(config);
        builder.root::<ServerTestApp>().unwrap();
        builder.lifecycle_hook(RecordingHook {
            events: Arc::clone(&events),
            fail_shutdown: false,
        });
        let application = builder.build().await.unwrap();
        assert_eq!(
            application.auto_configurations()[0].status(),
            AutoConfigurationStatus::Active,
        );
        let database = application.context().resolve::<Database>().unwrap();
        let repository = application
            .context()
            .resolve::<AutoDatabaseRepository>()
            .unwrap();
        assert_eq!(repository.database().status().size(), 0);

        let error = serve_with(
            application,
            address(),
            |address| async move {
                BINDS.fetch_add(1, Ordering::SeqCst);
                TcpListener::bind(address).await
            },
            async {},
        )
        .await
        .unwrap_err();

        assert!(matches!(error, HttpRuntimeError::Bootstrap(_)));
        assert_eq!(STARTS.load(Ordering::SeqCst), 0);
        assert_eq!(BINDS.load(Ordering::SeqCst), 0);
        assert_eq!(database.status().size(), 0);
        assert!(events.lock().unwrap().is_empty());
        database.close();
    }

    #[tokio::test]
    async fn raw_router_preflight_precedes_lifecycle_binding_and_graceful_shutdown() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let application = application(Arc::clone(&events), true, false).await;
        let router = crate::build_router(&application).unwrap();
        let binder_events = Arc::clone(&events);
        let binder = move |address| async move {
            binder_events.lock().unwrap().push("bind");
            tokio::net::TcpListener::bind(address).await
        };
        let shutdown_events = Arc::clone(&events);
        let shutdown = async move {
            shutdown_events.lock().unwrap().push("serve");
        };

        serve_router_with(application, router, address(), binder, shutdown)
            .await
            .unwrap();

        assert_eq!(
            *events.lock().unwrap(),
            [
                "router_preflight",
                "lifecycle_start",
                "bind",
                "serve",
                "lifecycle_stop",
            ]
        );
    }

    #[tokio::test]
    async fn bind_failure_still_attempts_shutdown() {
        let _guard = TEST_LOCK.lock().await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let application = application(Arc::clone(&events), true, false).await;
        let binder_events = Arc::clone(&events);
        let binder = move |_| async move {
            binder_events.lock().unwrap().push("bind");
            Err(io::Error::new(io::ErrorKind::AddrInUse, "occupied"))
        };

        let error = serve_with(application, address(), binder, async {})
            .await
            .unwrap_err();

        assert!(matches!(error, HttpRuntimeError::Bind(_)));
        assert_eq!(
            *events.lock().unwrap(),
            [
                "router_preflight",
                "lifecycle_start",
                "bind",
                "lifecycle_stop"
            ]
        );
    }

    #[tokio::test]
    async fn operational_and_shutdown_failures_are_both_retained() {
        let _guard = TEST_LOCK.lock().await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let application = application(Arc::clone(&events), true, true).await;
        let binder = |_| async {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "bind denied",
            ))
        };

        let error = serve_with(application, address(), binder, async {})
            .await
            .unwrap_err();

        match error {
            HttpRuntimeError::OperationAndShutdown {
                operation,
                shutdown,
            } => {
                assert!(matches!(*operation, HttpRuntimeError::Bind(_)));
                assert_eq!(shutdown.code(), MADS011);
                let source = std::error::Error::source(&shutdown)
                    .unwrap()
                    .downcast_ref::<Error>()
                    .unwrap();
                assert_eq!(source.code(), MADS020);
            }
            other => panic!("expected combined failure, got {other:?}"),
        }
        assert_eq!(
            *events.lock().unwrap(),
            ["router_preflight", "lifecycle_start", "lifecycle_stop"]
        );
    }

    #[tokio::test]
    async fn invalid_cors_configuration_prevents_lifecycle_start() {
        let _guard = TEST_LOCK.lock().await;
        STARTS.store(0, Ordering::SeqCst);
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ConfigBuilder::new()
            .source(
                MapSource::new("test", std::iter::empty::<(&str, &str)>())
                    .with_string_array("server.cors.origins", ["https://app.example.com"]),
            )
            .build()
            .unwrap();
        let mut builder = Mads::builder_with_config(config);
        builder.root::<ServerTestApp>().unwrap();
        builder.lifecycle_hook(RecordingHook {
            events: Arc::clone(&events),
            fail_shutdown: false,
        });

        assert!(builder.build().await.is_err());
        assert_eq!(STARTS.load(Ordering::SeqCst), 0);
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn explicit_raw_router_ignores_automatic_binding_keys_and_applies_cors() {
        let _guard = TEST_LOCK.lock().await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ConfigBuilder::new()
            .source(
                MapSource::new(
                    "test",
                    [
                        ("server.host", "private\ninvalid-host"),
                        ("server.port", "0"),
                    ],
                )
                .with_string_array("server.cors.origins", ["https://app.example.com"])
                .with_string_array("server.cors.methods", ["GET"]),
            )
            .build()
            .unwrap();
        let mut builder = Mads::builder_with_config(config);
        builder.root::<raw_router::App>().unwrap();
        builder.lifecycle_hook(RecordingHook {
            events: Arc::clone(&events),
            fail_shutdown: false,
        });
        let application = builder.build().await.unwrap();
        let router = axum::Router::new().route("/native", axum::routing::get(|| async { "ok" }));
        let (bound_address_sender, bound_address_receiver) = tokio::sync::oneshot::channel();
        let binder_events = Arc::clone(&events);
        let binder = move |address: SocketAddr| async move {
            assert_eq!(address.port(), 0);
            binder_events.lock().unwrap().push("bind");
            let listener = TcpListener::bind(address).await?;
            bound_address_sender
                .send(listener.local_addr().unwrap())
                .unwrap();
            Ok(listener)
        };
        let shutdown_events = Arc::clone(&events);
        let shutdown = async move {
            let bound_address = bound_address_receiver.await.unwrap();
            let response = tokio::task::spawn_blocking(move || {
                let mut stream = std::net::TcpStream::connect(bound_address)?;
                stream.write_all(
                    b"GET /native HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://app.example.com\r\nConnection: close\r\n\r\n",
                )?;
                let mut response = String::new();
                stream.read_to_string(&mut response)?;
                Ok::<_, io::Error>(response)
            })
            .await
            .unwrap()
            .unwrap();
            assert!(response.starts_with("HTTP/1.1 200"));
            assert!(response.contains("access-control-allow-origin: https://app.example.com"));
            shutdown_events.lock().unwrap().push("serve");
        };

        serve_router_with(application, router, address(), binder, shutdown)
            .await
            .unwrap();

        assert_eq!(
            *events.lock().unwrap(),
            ["lifecycle_start", "bind", "serve", "lifecycle_stop"]
        );
    }
}
