//! Public MADS.rs facade and feature composition boundary.
//!
//! The default facade composes the HTTP and database integrations with the
//! Tokio runtime. A root [`Module`] selects the Rust-namespace-owned providers,
//! controllers, routes, guards, strategies, and official auto-configurations
//! that belong to one application. Direct imports and unrestricted `pub`
//! visibility govern access across module boundaries.
//!
//! The standard startup path loads optional `.env`, optional `mads.toml`, and
//! final `MADS_*` overrides from the current working directory in that order.
//! It builds the root module, configures the complete router, starts lifecycle
//! hooks, and serves the configured listener. The automatic server defaults are
//! `127.0.0.1:3000`; `[server.cors]` is an opt-in strict outer router layer:
//!
//! ```no_run
//! use mads::prelude::*;
//!
//! mod user {
//!     use mads::prelude::*;
//!
//!     #[module]
//!     pub struct UserHttpModule;
//! }
//! use user::UserHttpModule;
//!
//! #[module(imports = [UserHttpModule])]
//! struct AppModule;
//!
//! #[mads::main]
//! async fn main() -> Result<(), HttpRuntimeError> {
//!     Mads::run::<AppModule>().await
//! }
//! ```
//!
//! Use the low-level builder for explicit configuration, embedded migrations,
//! lifecycle hooks, native routers, or listener addresses. It never loads
//! conventional sources. [`build_router`] returns an unconfigured generated
//! router so native routes can be merged before [`configure_router`] or
//! [`serve_router`] applies application-wide configuration. A builder without
//! [`core::MadsBuilder::root`] retains complete-catalog compatibility.
//!
//! ```no_run
//! use mads::prelude::*;
//!
//! # #[module]
//! # struct AppModule;
//! # async fn low_level(
//! #     config: Config,
//! #     native_router: mads::axum::Router,
//! #     migrations: mads::diesel_migrations::EmbeddedMigrations,
//! # )
//! # -> Result<(), Box<dyn std::error::Error>> {
//! let mut builder = Mads::builder_with_config(config);
//! builder.root::<AppModule>()?;
//! builder.database_migrations(migrations)?;
//! // builder.lifecycle_hook(MyHook);
//! let application = builder.build().await?;
//! let router = build_router(&application)?.merge(native_router);
//! serve_router(application, router, "127.0.0.1:0").await?;
//! # Ok(())
//! # }
//! ```
//!
//! The explicit address overrides `[server]` binding and may use port zero.
//! `configure_router` is the alternative for direct in-process router use after
//! the merge; do not pass an already configured router to `serve_router`.
//! Database provisioning remains conditional on the selected application, and
//! embedded migrations require explicit
//! [`MadsBuilderDatabaseExt::database_migrations`] registration. The retained
//! [`AutoConfigurationReport`] records expose redacted decision evidence only.
//! `DatabaseBootstrap` remains the native Diesel override.
//!
//! Register custom access and refresh strategies as managed providers. MADS
//! verifies JWT cryptography, registered claims, and token kind before either
//! strategy receives typed claims:
//!
//! ```
//! use mads::prelude::*;
//!
//! #[derive(serde::Deserialize)]
//! struct UserClaims { user_id: u64 }
//! struct UserPrincipal(u64);
//! impl PassportPrincipal for UserPrincipal {
//!     fn has_role(&self, role: &str) -> bool { role == "user" }
//!     fn has_permission(&self, permission: &str) -> bool {
//!         permission == "profile:read"
//!     }
//! }
//!
//! #[service]
//! struct AccessStrategy;
//! #[passport_strategy(name = "jwt")]
//! impl PassportStrategy for AccessStrategy {
//!     type Claims = UserClaims;
//!     type Principal = UserPrincipal;
//!     const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;
//!     async fn validate(
//!         &self,
//!         _context: &PassportContext<'_>,
//!         claims: &JwtClaims<Self::Claims>,
//!     ) -> PassportResult<Self::Principal> {
//!         Ok(UserPrincipal(claims.custom.user_id))
//!     }
//! }
//!
//! #[service]
//! struct RefreshStrategy;
//! #[passport_strategy(name = "jwt-refresh")]
//! impl PassportStrategy for RefreshStrategy {
//!     type Claims = UserClaims;
//!     type Principal = UserPrincipal;
//!     const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Refresh;
//!     async fn validate(
//!         &self,
//!         _context: &PassportContext<'_>,
//!         claims: &JwtClaims<Self::Claims>,
//!     ) -> PassportResult<Self::Principal> {
//!         Ok(UserPrincipal(claims.custom.user_id))
//!     }
//! }
//! # fn main() {}
//! ```
//!
//! Route guards inherit field by field. Method clauses replace only supplied
//! fields, cookie sources select exactly one named cookie, and `skip` removes
//! an inherited guard:
//!
//! ```
//! use mads::prelude::*;
//!
//! struct UserPrincipal;
//! impl PassportPrincipal for UserPrincipal {
//!     fn has_role(&self, role: &str) -> bool { role == "user" }
//!     fn has_permission(&self, permission: &str) -> bool {
//!         permission == "profile:read"
//!     }
//! }
//! fn owns_profile(_: &UserPrincipal) -> bool { true }
//!
//! #[routes(prefix = "/users")]
//! #[guard(
//!     strategy = "jwt",
//!     principal = UserPrincipal,
//!     source = bearer,
//!     roles(any = ["user", "admin"]),
//! )]
//! trait UserRoutes {
//!     #[get("/profile")]
//!     #[guard(
//!         permissions(all = ["profile:read"]),
//!         predicate = owns_profile,
//!     )]
//!     async fn profile(&self, principal: Authenticated<UserPrincipal>);
//!
//!     #[post("/refresh")]
//!     #[guard(strategy = "jwt-refresh", source = cookie("refresh_token"))]
//!     async fn refresh(&self);
//!
//!     #[post("/login")]
//!     #[guard(skip)]
//!     async fn login(&self);
//! }
//! # fn main() {}
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

/// Re-exports the framework-neutral MADS.rs core.
pub use mads_core as core;

/// Re-exports the asynchronous MADS.rs entry-point attribute.
pub use mads_core::main;

/// Re-exports the application-module declaration attribute.
pub use mads_core::module;

/// Re-exports the general-purpose provider declaration attribute.
pub use mads_core::provider;

/// Re-exports explicit typed configuration, structured failures, and secret values.
pub use mads_core::{
    Configuration, ConfigurationErrors, ConfigurationIssue, ConfigurationResult, Secret,
};

/// Re-exports auto-configuration inspection records.
pub use mads_core::{
    AutoConfigurationConfigEvidence, AutoConfigurationReasonCode, AutoConfigurationReport,
    AutoConfigurationRequirement, AutoConfigurationStatus,
};

/// Re-exports root-module contracts and retained module-graph inspection records.
pub use mads_core::{
    Module, ModuleGraph, ModuleImportDescriptor, ModuleImportEdge, ModuleNode, ProviderOwnership,
};

/// Re-exports the repository declaration attribute.
pub use mads_core::repository;

/// Re-exports the service declaration attribute.
pub use mads_core::service;

/// Re-exports enabled standard integrations.
#[cfg(any(
    feature = "http",
    feature = "database",
    feature = "jwt",
    feature = "cookies"
))]
pub use mads_common as common;

/// Re-exports Axum for native HTTP runtime integration.
#[cfg(feature = "http")]
pub use mads_common::axum;

/// Re-exports Diesel for native persistence integration.
#[cfg(feature = "database")]
pub use mads_common::diesel;

/// Re-exports Diesel migrations for native persistence integration.
#[cfg(feature = "database")]
pub use mads_common::diesel_migrations;

/// Re-exports strict cookie integration and the established cookie time types.
#[cfg(feature = "cookies")]
pub use mads_common::cookie;

/// Re-exports database configuration, runtime, migration, and error contracts.
#[cfg(feature = "database")]
pub use mads_common::{
    Database, DatabaseBootstrap, DatabaseConfig, DatabaseError, DatabaseErrorKind,
    DatabasePoolStatus, DatabaseResult, MADS100, MADS101, MadsBuilderDatabaseExt, MigrationReport,
    MigrationStatus,
};

/// Re-exports typed JWT claims, service, options, errors, and diagnostics.
#[cfg(feature = "jwt")]
pub use mads_common::{
    JwtAlgorithm, JwtClaims, JwtError, JwtErrorKind, JwtHeader, JwtResult, JwtService,
    JwtSignOptions, JwtTokenKind, JwtValidation, MADS120, MADS121, PassportConfig,
    RegisteredJwtClaims, VerifiedJwt,
};

/// Re-exports strict cookie extraction, response composition, and diagnostics.
#[cfg(feature = "cookies")]
pub use mads_common::{
    Cookie, CookieError, CookieErrorKind, CookieJar, CookieRejection, CookieResult, Expiration,
    MADS110, SameSite,
};

/// Re-exports HTTP request extractors and their typed-header support.
#[cfg(feature = "http")]
pub use mads_common::{Header, Json, Path, Query, Request, headers};

/// Re-exports standard HTTP response types.
#[cfg(feature = "http")]
pub use mads_common::{
    BadRequest, Conflict, Created, Forbidden, HttpError, HttpResult, InternalError, NoContent,
    NotFound, Unauthorized, ValidationError,
};

/// Re-exports validated extractors, the input derive, and ordered issue contracts.
#[cfg(feature = "http")]
pub use mads_common::{
    Input, SourcedValidationIssue, ValidatedJson, ValidatedPath, ValidatedQuery, ValidationErrors,
    ValidationIssue, ValidationPathSegment, ValidationResult, ValidationSource,
};

/// Re-exports HTTP router construction, configuration, and runtime startup functions.
#[cfg(feature = "http")]
pub use mads_common::{
    HttpRuntimeError, MADS031, MadsRunExt, build_router, configure_router, serve, serve_router,
};

/// Re-exports guarded Passport authentication and policy contracts.
#[cfg(all(feature = "http", feature = "jwt"))]
pub use mads_common::{
    Authenticated, ClaimsPrincipal, MADS130, MADS131, PassportContext, PassportError,
    PassportErrorKind, PassportGuard, PassportGuardBuilder, PassportPrincipal, PassportRejection,
    PassportResult, PassportStrategy, TokenSource, VerifiedToken, guard, passport_strategy,
};

/// Re-exports the managed-controller declaration attribute.
#[cfg(feature = "http")]
pub use mads_common::controller;

/// Re-exports the DELETE route-contract attribute.
#[cfg(feature = "http")]
pub use mads_common::delete;

/// Re-exports the GET route-contract attribute.
#[cfg(feature = "http")]
pub use mads_common::get;

/// Re-exports the PATCH route-contract attribute.
#[cfg(feature = "http")]
pub use mads_common::patch;

/// Re-exports the POST route-contract attribute.
#[cfg(feature = "http")]
pub use mads_common::post;

/// Re-exports the route-trait declaration attribute.
#[cfg(feature = "http")]
pub use mads_common::routes;

/// Re-exports the PUT route-contract attribute.
#[cfg(feature = "http")]
pub use mads_common::put;

/// Re-exports extensions when the `extra` feature is enabled.
#[cfg(feature = "extra")]
pub use mads_extra as extra;

/// Collects application-facing MADS.rs imports.
pub mod prelude {
    /// Re-exports the asynchronous MADS.rs entry-point attribute.
    pub use mads_core::main;

    /// Re-exports the application-module declaration attribute.
    pub use mads_core::module;

    /// Re-exports the general-purpose provider declaration attribute.
    pub use mads_core::provider;

    /// Re-exports the repository declaration attribute.
    pub use mads_core::repository;

    /// Re-exports the service declaration attribute.
    pub use mads_core::service;

    /// Re-exports the managed-controller declaration attribute.
    #[cfg(feature = "http")]
    pub use mads_common::controller;

    /// Re-exports route-contract attributes.
    #[cfg(feature = "http")]
    pub use mads_common::{delete, get, patch, post, put, routes};

    /// Re-exports standard HTTP request extractors and typed-header support.
    #[cfg(feature = "http")]
    pub use mads_common::{Header, Json, Path, Query, Request, headers};

    /// Re-exports standard HTTP response types.
    #[cfg(feature = "http")]
    pub use mads_common::{
        BadRequest, Conflict, Created, Forbidden, HttpError, HttpResult, InternalError, NoContent,
        NotFound, Unauthorized, ValidationError,
    };

    /// Re-exports validated extractors, the input derive, and ordered issue contracts.
    #[cfg(feature = "http")]
    pub use mads_common::{
        Input, SourcedValidationIssue, ValidatedJson, ValidatedPath, ValidatedQuery,
        ValidationErrors, ValidationIssue, ValidationPathSegment, ValidationResult,
        ValidationSource,
    };

    /// Re-exports HTTP router construction, configuration, and runtime startup functions.
    #[cfg(feature = "http")]
    pub use mads_common::{
        HttpRuntimeError, MADS031, MadsRunExt, build_router, configure_router, serve, serve_router,
    };

    /// Re-exports application-facing Passport guards, strategies, and extractors.
    #[cfg(all(feature = "http", feature = "jwt"))]
    pub use mads_common::{
        Authenticated, ClaimsPrincipal, MADS130, MADS131, PassportContext, PassportError,
        PassportErrorKind, PassportGuard, PassportGuardBuilder, PassportPrincipal,
        PassportRejection, PassportResult, PassportStrategy, TokenSource, VerifiedToken, guard,
        passport_strategy,
    };

    /// Re-exports application-facing database configuration and runtime types.
    #[cfg(feature = "database")]
    pub use mads_common::{
        Database, DatabaseBootstrap, DatabaseConfig, DatabaseError, DatabaseErrorKind,
        DatabasePoolStatus, DatabaseResult, MadsBuilderDatabaseExt, MigrationReport,
        MigrationStatus,
    };

    /// Re-exports application-facing Passport JWT contracts and services.
    #[cfg(feature = "jwt")]
    pub use mads_common::{
        JwtAlgorithm, JwtClaims, JwtError, JwtErrorKind, JwtHeader, JwtResult, JwtService,
        JwtSignOptions, JwtTokenKind, JwtValidation, MADS120, MADS121, PassportConfig,
        RegisteredJwtClaims, VerifiedJwt,
    };

    /// Re-exports strict cookie extraction, response composition, and time types.
    #[cfg(feature = "cookies")]
    pub use mads_common::{
        Cookie, CookieError, CookieErrorKind, CookieJar, CookieRejection, CookieResult, Expiration,
        MADS110, SameSite, cookie,
    };

    /// Re-exports types used to build, run, and inspect an application.
    pub use mads_core::{
        ApplicationContext, ApplicationGraph, AutoConfigurationConfigEvidence,
        AutoConfigurationReasonCode, AutoConfigurationReport, AutoConfigurationRequirement,
        AutoConfigurationStatus, Catalog, Config, ConfigBuilder, Configuration,
        ConfigurationErrors, ConfigurationIssue, ConfigurationResult, ConstructionPlan,
        ConstructionStep, DependencyEdge, Diagnostic, Error, GraphAnalysis, LifecycleHook,
        LifecycleState, Mads, Module, ModuleGraph, ModuleImportDescriptor, ModuleImportEdge,
        ModuleNode, ProviderNode, ProviderOrigin, ProviderOwnership, ProviderState,
        ProviderVisibility, Secret, SourceLocation,
    };
}
