//! Standard integration contracts for MADS.rs.
//!
//! Enable the `http`, `database`, `jwt`, or `cookies` feature to select only
//! the integration contracts an application needs. The framework-neutral core
//! boundary is always available through [`core`].
#![cfg_attr(
    feature = "http",
    doc = "\nThe `http` feature provides compile-time controller and route contracts and the Axum runtime. [`build_router`] validates every controller selected for the application before it resolves a controller or invokes a typed registrar; [`serve`] performs that same validation before lifecycle startup or socket binding. Use [`serve_router`] to run a complete raw router after merging generated and native routes; it applies final application-wide router configuration before lifecycle startup. Use [`routes`] to declare a route contract and [`controller`] to bind it to a managed controller. The resulting descriptors can be inspected through [`RouteCatalog`] before the HTTP runtime installs handlers. [`axum`] is deliberately re-exported for native extractors, response types, routers, middleware, and Tower composition."
)]
#![cfg_attr(
    feature = "database",
    doc = "\nThe `database` feature provides the official PostgreSQL/Diesel conditional default. When the complete provider catalog requires [`Database`] and the application has not provided one, this crate supplies the official default and its infrastructure lifecycle. Use [`DatabaseBootstrap`] as the explicit override when the application needs native Diesel control; a custom provider owns its complete lifecycle. [`MadsBuilderDatabaseExt::database_migrations`] separately registers at most one embedded migration source for an enabled default. A managed [`Database`] runs synchronous Diesel queries through its blocking pool boundary. The [`diesel`] and [`diesel_migrations`] re-exports are deliberate native escape hatches."
)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

#[cfg(feature = "http")]
mod cors;
#[cfg(feature = "http")]
mod extract;
#[cfg(feature = "http")]
mod http_scope;
#[cfg(feature = "http")]
mod inspection;
#[cfg(feature = "http")]
mod response;
#[cfg(feature = "http")]
mod route;
#[cfg(feature = "http")]
mod router;
#[cfg(feature = "http")]
mod server;
#[cfg(feature = "http")]
mod server_config;
#[cfg(feature = "http")]
mod validation;

extern crate self as mads_common;

/// Derives input validation with built-in, nested, and custom checks.
#[cfg(feature = "http")]
pub use mads_common_macros::Input;

/// Re-exports transport-independent input validation contracts.
#[cfg(feature = "http")]
pub use validation::{
    Input, SourcedValidationIssue, ValidationErrors, ValidationIssue, ValidationPathSegment,
    ValidationResult, ValidationSource,
};

/// Strict cookie extraction, response composition, and established cookie types.
#[cfg(feature = "cookies")]
pub mod cookie;

/// Typed JSON Web Token contracts and services.
#[cfg(feature = "jwt")]
pub mod jwt;

/// Typed Passport principals, request context, and normalized failures.
#[cfg(all(feature = "http", feature = "jwt"))]
pub mod passport;

/// Database configuration and normalized persistence errors.
#[cfg(feature = "database")]
pub mod database;

/// Re-exports Axum for native runtime integration.
#[cfg(feature = "http")]
pub use axum;

/// Re-exports Diesel for native persistence integration.
#[cfg(feature = "database")]
pub use diesel;

/// Re-exports Diesel migrations for native persistence integration.
#[cfg(feature = "database")]
pub use diesel_migrations;

/// Database configuration, managed persistence, and normalized errors.
#[cfg(feature = "database")]
pub use database::{
    Database, DatabaseBootstrap, DatabaseConfig, DatabaseError, DatabaseErrorKind,
    DatabasePoolStatus, DatabaseResult, MADS100, MADS101, MadsBuilderDatabaseExt, MigrationReport,
    MigrationStatus,
};

/// Typed JWT claims, service, options, errors, and diagnostics.
#[cfg(feature = "jwt")]
pub use jwt::{
    JwtAlgorithm, JwtClaims, JwtError, JwtErrorKind, JwtHeader, JwtResult, JwtService,
    JwtSignOptions, JwtTokenKind, JwtValidation, MADS120, MADS121, PassportConfig,
    RegisteredJwtClaims, VerifiedJwt,
};

/// Typed Passport principals, guarded extractors, context, errors, and diagnostics.
#[cfg(all(feature = "http", feature = "jwt"))]
pub use passport::{
    Authenticated, BuiltinGuardAdapter, ClaimsPrincipal, ErasedAuthentication, GuardCatalog,
    GuardDescriptor, GuardPredicate, GuardPredicateAdapter, MADS130, MADS131, PassportContext,
    PassportError, PassportErrorKind, PassportGuard, PassportGuardBuilder, PassportPrincipal,
    PassportRejection, PassportResult, PassportStrategy, PassportStrategyAdapter,
    PassportStrategyBinding, PassportStrategyCatalog, PassportStrategyDescriptor,
    PassportStrategyFuture, PassportStrategyPreflight, PolicyClause, PolicyMode, TokenSource,
    VerifiedToken,
};

/// Safe parsed-cookie metadata available to cookie-authenticated Passport strategies.
#[cfg(all(feature = "http", feature = "jwt", feature = "cookies"))]
pub use passport::PassportCookies;

/// Strict cookie extraction, normalized errors, and established cookie types.
#[cfg(feature = "cookies")]
pub use cookie::{
    Cookie, CookieError, CookieErrorKind, CookieJar, CookieRejection, CookieResult, Expiration,
    MADS110, SameSite,
};

/// Standard Axum-compatible HTTP request extractors.
#[cfg(feature = "http")]
pub use extract::{Header, Json, Path, Query, Request, headers};

/// Standard Axum-compatible HTTP response types.
#[cfg(feature = "http")]
pub use response::{
    BadRequest, Conflict, Created, Forbidden, HttpError, HttpResult, InternalError, NoContent,
    NotFound, Unauthorized, ValidationError,
};

/// Builds a raw Axum router from the application's validated controllers.
#[cfg(feature = "http")]
pub use router::{build_router, configure_router};

/// Invalid or failed private application-inspection request.
#[cfg(feature = "http")]
pub use inspection::MADS032;
/// Runs validated applications and raw composed routers on the Axum HTTP runtime.
#[cfg(feature = "http")]
pub use server::{HttpRuntimeError, MADS031, MadsRunExt, serve, serve_router};

/// Exposes the framework-neutral core boundary to future integrations.
pub use mads_core as core;

/// Declares a managed controller and its route-trait contracts.
#[cfg(feature = "http")]
pub use mads_common_macros::controller;

/// Declares and validates a route-contract trait.
#[cfg(feature = "http")]
pub use mads_common_macros::routes;

/// Declares an inheritable Passport policy inside a `#[routes]` contract.
#[cfg(all(feature = "http", feature = "jwt"))]
pub use mads_common_macros::guard;

/// Derives role and permission membership for a named Passport principal.
#[cfg(all(feature = "http", feature = "jwt"))]
pub use mads_common_macros::PassportPrincipal;

/// Registers a managed, typed Passport JWT strategy.
#[cfg(all(feature = "http", feature = "jwt"))]
pub use mads_common_macros::passport_strategy;

/// Marks a DELETE route inside a route-contract trait.
#[cfg(feature = "http")]
pub use mads_common_macros::delete;

/// Marks a GET route inside a route-contract trait.
#[cfg(feature = "http")]
pub use mads_common_macros::get;

/// Marks a PATCH route inside a route-contract trait.
#[cfg(feature = "http")]
pub use mads_common_macros::patch;

/// Marks a POST route inside a route-contract trait.
#[cfg(feature = "http")]
pub use mads_common_macros::post;

/// Marks a PUT route inside a route-contract trait.
#[cfg(feature = "http")]
pub use mads_common_macros::put;

/// Static route and controller metadata types used by the contract catalog.
#[cfg(feature = "http")]
pub use route::{
    ControllerRegistrar, ControllerRouteDescriptor, HttpMethod, RouteCatalog,
    RouteContractDescriptor, RouteDescriptor,
};

/// Implementation details used by generated HTTP route adapters.
#[doc(hidden)]
#[cfg(feature = "http")]
pub mod __private {
    pub use crate::validation::support as input_validation;
    /// Environment variable used by the development supervisor for graceful shutdown.
    #[doc(hidden)]
    pub const DEV_SHUTDOWN_ENV: &str = "MADS_INTERNAL_DEV_SHUTDOWN_PATH";

    pub use axum::Router;
    pub use axum::routing::{delete, get, patch, post, put};

    /// Loads conventional sources with an injected environment source for tests.
    #[cfg(feature = "http")]
    #[allow(clippy::result_large_err)]
    pub fn load_standard_config_from_for_test(
        root: &std::path::Path,
        environment: mads_core::EnvSource,
    ) -> mads_core::Result<mads_core::Config> {
        crate::server_config::load_standard_config_from_with_environment(root, environment)
    }

    /// Enables the private automatic HTTP server mode for integration tests.
    pub fn enable_automatic_server_for_test(builder: &mut mads_core::MadsBuilder) -> bool {
        crate::server_config::enable_automatic_server(builder)
    }

    /// Enables the private automatic CORS mode for integration tests.
    pub fn enable_automatic_cors_for_test(builder: &mut mads_core::MadsBuilder) -> bool {
        crate::cors::enable_automatic_cors(builder)
    }

    /// Returns the automatic server binding as an owned address tuple for integration tests.
    #[allow(clippy::result_large_err)]
    pub fn server_binding_address_for_test(
        application: &mads_core::Mads,
    ) -> mads_core::Result<(String, u16)> {
        let binding = application
            .context()
            .resolve::<crate::server_config::ServerBinding>()?;
        let (host, port) = binding.address();
        Ok((host.to_owned(), port))
    }

    #[cfg(feature = "jwt")]
    pub use crate::passport::{PassportGuardLayer, PassportGuardState};
    #[cfg(feature = "jwt")]
    #[allow(clippy::result_large_err)]
    pub fn preflight_scoped(
        module_graph: Option<&mads_core::ModuleGraph>,
    ) -> mads_core::Result<crate::passport::PassportStrategyPreflight<'static>> {
        let scope = crate::http_scope::HttpApplicationScope::for_module_graph(module_graph)?;
        crate::passport::PassportStrategyCatalog::preflight_scoped(module_graph, scope.guards())
    }
    pub use crate::route::{RouterBuildContext, ValidatedRouteIter, validate_descriptors};

    #[doc(hidden)]
    pub use crate::inspection::{
        AutoConfigurationReport, ConfigurationEvidenceReport, DependencyReport, DiagnosticReport,
        DoctorCheck, DoctorStatus, GraphReport, INSPECTION_ACK_ENV, INSPECTION_KIND_ENV,
        INSPECTION_PROTOCOL_VERSION, INSPECTION_RESPONSE_ENV, INSPECTION_TOKEN_ENV,
        INSPECTION_VERSION_ENV, InspectionEnvelope, InspectionKind, InspectionReport, MADS032,
        ModuleImportReport, ModuleReport, ProviderReport, RouteReport, SourceReport,
    };
}
