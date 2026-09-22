//! Static Passport guard policy metadata.
//!
//! Route macros emit immutable descriptors here. Router construction resolves
//! each descriptor to one strategy adapter, and the route middleware below
//! uses that same binding to authenticate and authorize requests.

use std::any::TypeId;
use std::cmp::Ordering;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};

use axum::{
    extract::{Request, connect_info::ConnectInfo},
    http::header::AUTHORIZATION,
    response::{IntoResponse, Response},
};
use mads_core::{Diagnostic, Error, Result, SourceLocation};
use tower::{Layer, Service};

use super::{
    ErasedAuthentication, MADS131, PassportContext, PassportError, PassportResult,
    PassportStrategyAdapter, PassportStrategyBinding, PassportStrategyCatalog,
    PassportStrategyFuture,
};
use crate::{ClaimsPrincipal, JwtService, JwtValidation, PassportPrincipal};

#[cfg(feature = "cookies")]
use crate::CookieJar;

/// The one token source selected by a Passport guard.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum TokenSource {
    /// Read one RFC 6750 Bearer credential from the `Authorization` header.
    Bearer,
    /// Read one strict request cookie by its literal name.
    #[cfg(feature = "cookies")]
    Cookie(&'static str),
}

impl fmt::Debug for TokenSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer => formatter.write_str("Bearer"),
            // Cookie names can identify a deployment's authentication surface;
            // diagnostics intentionally retain only the source category.
            #[cfg(feature = "cookies")]
            Self::Cookie(_) => formatter.write_str("Cookie(..)"),
        }
    }
}

/// The matching rule applied to a role or permission collection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyMode {
    /// At least one configured value must match.
    Any,
    /// Every configured value must match.
    All,
}

/// One role or permission policy clause emitted by `#[guard]`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyClause {
    mode: PolicyMode,
    values: &'static [&'static str],
}

impl PolicyClause {
    /// Creates one static policy clause.
    #[must_use]
    pub const fn new(mode: PolicyMode, values: &'static [&'static str]) -> Self {
        Self { mode, values }
    }

    /// Returns whether this clause uses `any` or `all` matching.
    #[must_use]
    pub const fn mode(self) -> PolicyMode {
        self.mode
    }

    /// Returns the configured role or permission values.
    #[must_use]
    pub const fn values(self) -> &'static [&'static str] {
        self.values
    }
}

/// A generated, type-safe policy predicate over an erased authentication.
pub type GuardPredicateAdapter = fn(&ErasedAuthentication) -> bool;

/// One named predicate emitted by `#[guard]`.
#[derive(Clone, Copy)]
pub struct GuardPredicate {
    name: &'static str,
    adapter: Option<GuardPredicateAdapter>,
}

impl GuardPredicate {
    /// Creates static predicate metadata.
    ///
    /// `None` is accepted solely so manually supplied metadata can be
    /// validated fail-closed by [`GuardCatalog::validate`]. Macro-generated
    /// descriptors always provide an adapter.
    #[must_use]
    pub const fn new(name: &'static str, adapter: Option<GuardPredicateAdapter>) -> Self {
        Self { name, adapter }
    }

    /// Returns the source path recorded for this predicate.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Returns the generated type-safe adapter when metadata is valid.
    #[doc(hidden)]
    #[must_use]
    pub const fn adapter(self) -> Option<GuardPredicateAdapter> {
        self.adapter
    }
}

/// Framework adapter for the built-in `jwt` strategy supplied by a direct
/// `ClaimsPrincipal<C>` guard.
pub type BuiltinGuardAdapter = for<'a> fn(
    &'a mads_core::ApplicationContext,
    &'a PassportContext<'a>,
    &'a str,
) -> PassportStrategyFuture<'a>;

/// Immutable effective policy for one guarded route method.
pub struct GuardDescriptor {
    route_trait: &'static str,
    handler: &'static str,
    requirement_subject: &'static str,
    namespace: Option<&'static str>,
    strategy: &'static str,
    principal_type_id: Option<fn() -> TypeId>,
    principal_type_name: Option<fn() -> &'static str>,
    source: TokenSource,
    roles: Option<PolicyClause>,
    permissions: Option<PolicyClause>,
    predicates: &'static [GuardPredicate],
    location: SourceLocation,
    builtin_adapter: Option<BuiltinGuardAdapter>,
}

impl GuardDescriptor {
    /// Creates static effective guard metadata.
    ///
    /// The optional principal factories permit integrations to build
    /// descriptors manually. [`GuardCatalog::validate`] rejects omitted or
    /// inconsistent factories before startup.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        route_trait: &'static str,
        handler: &'static str,
        strategy: &'static str,
        principal_type_id: Option<fn() -> TypeId>,
        principal_type_name: Option<fn() -> &'static str>,
        source: TokenSource,
        roles: Option<PolicyClause>,
        permissions: Option<PolicyClause>,
        predicates: &'static [GuardPredicate],
        location: SourceLocation,
        builtin_adapter: Option<BuiltinGuardAdapter>,
    ) -> Self {
        Self {
            route_trait,
            handler,
            requirement_subject: "manual Passport guard",
            namespace: None,
            strategy,
            principal_type_id,
            principal_type_name,
            source,
            roles,
            permissions,
            predicates,
            location,
            builtin_adapter,
        }
    }

    /// Attaches the stable requirement subject emitted by `#[guard]`.
    ///
    /// Route expansion supplies `RouteTrait::method` with `concat!`, keeping
    /// auto-configuration evidence static and free of application values.
    #[doc(hidden)]
    #[must_use]
    pub const fn with_requirement_subject(mut self, subject: &'static str) -> Self {
        self.requirement_subject = subject;
        self
    }

    /// Attaches the Rust namespace containing this guard declaration.
    #[doc(hidden)]
    #[must_use]
    pub const fn with_namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Returns the Rust namespace containing this guard declaration, when available.
    #[doc(hidden)]
    pub const fn namespace(&self) -> Option<&'static str> {
        self.namespace
    }

    /// Returns the route-contract trait name.
    #[must_use]
    pub const fn route_trait(&self) -> &'static str {
        self.route_trait
    }

    /// Returns the guarded route method name.
    #[must_use]
    pub const fn handler(&self) -> &'static str {
        self.handler
    }

    /// Returns the stable subject used for JWT auto-configuration evidence.
    #[doc(hidden)]
    #[must_use]
    pub const fn requirement_subject(&self) -> &'static str {
        self.requirement_subject
    }

    /// Returns the requested Passport strategy name.
    #[must_use]
    pub const fn strategy(&self) -> &'static str {
        self.strategy
    }

    /// Returns the exact principal type identifier when present.
    #[must_use]
    pub fn principal_type_id(&self) -> Option<TypeId> {
        self.principal_type_id.map(|factory| factory())
    }

    /// Returns the exact principal type name when present.
    #[must_use]
    pub fn principal_type_name(&self) -> Option<&'static str> {
        self.principal_type_name.map(|factory| factory())
    }

    /// Returns the one token source used for this guard.
    #[must_use]
    pub const fn source(&self) -> TokenSource {
        self.source
    }

    /// Returns the optional roles clause.
    #[must_use]
    pub const fn roles(&self) -> Option<PolicyClause> {
        self.roles
    }

    /// Returns the optional permissions clause.
    #[must_use]
    pub const fn permissions(&self) -> Option<PolicyClause> {
        self.permissions
    }

    /// Returns every ANDed custom predicate in declaration order.
    #[must_use]
    pub const fn predicates(&self) -> &'static [GuardPredicate] {
        self.predicates
    }

    /// Returns the route declaration source location.
    #[must_use]
    pub const fn location(&self) -> SourceLocation {
        self.location
    }

    /// Returns a built-in typed-claims adapter when this guard can use `jwt`
    /// without a custom managed strategy.
    #[doc(hidden)]
    #[must_use]
    pub const fn builtin_adapter(&self) -> Option<BuiltinGuardAdapter> {
        self.builtin_adapter
    }
}

mads_core::__private::inventory::collect!(&'static GuardDescriptor);

/// Read-only inspection and validation of linked Passport route guards.
pub struct GuardCatalog;

impl GuardCatalog {
    /// Returns every linked effective guard in deterministic route order.
    #[must_use]
    pub fn guards() -> Vec<&'static GuardDescriptor> {
        guard_cache().clone()
    }

    /// Validates every linked guard descriptor before strategy resolution.
    ///
    /// # Errors
    ///
    /// Returns `MADS131` when any descriptor has incomplete or malformed
    /// static policy metadata.
    #[allow(clippy::result_large_err)]
    pub fn validate() -> Result<()> {
        Self::validate_descriptors(&Self::guards())
    }

    /// Validates an explicit static descriptor slice without registering it.
    ///
    /// This permits integration crates to validate manually assembled
    /// descriptors using the same fail-closed rules as inventory metadata.
    #[doc(hidden)]
    #[allow(clippy::result_large_err)]
    pub fn validate_descriptors(descriptors: &[&GuardDescriptor]) -> Result<()> {
        for guard in descriptors {
            validate_guard(guard)?;
        }
        Ok(())
    }
}

/// Per-route middleware state produced from one preflight-selected Passport guard.
///
/// Generated route registration constructs this state while the router is built,
/// so the request path uses the exact static descriptor and selected adapter
/// already validated for that route.
#[doc(hidden)]
#[derive(Clone)]
pub struct PassportGuardState {
    application: mads_core::ApplicationContext,
    guard: &'static GuardDescriptor,
    adapter: PassportStrategyAdapter,
}

impl PassportGuardState {
    /// Constructs middleware state from an already selected static binding.
    #[doc(hidden)]
    pub fn from_binding(
        application: &mads_core::ApplicationContext,
        binding: &PassportStrategyBinding<'static>,
    ) -> Self {
        Self {
            application: application.clone(),
            guard: binding.guard(),
            adapter: binding.adapter(),
        }
    }

    /// Selects the one strategy adapter for a static guard descriptor.
    #[doc(hidden)]
    #[allow(clippy::result_large_err)]
    pub fn new(
        application: &mads_core::ApplicationContext,
        guard: &'static GuardDescriptor,
    ) -> Result<Self> {
        let guards = [guard];
        let preflight = PassportStrategyCatalog::preflight(&guards)?;
        let binding = preflight
            .binding_for(guard, None)
            .expect("the preflight result must retain its requested guard");
        Ok(Self {
            application: application.clone(),
            guard,
            adapter: binding.adapter(),
        })
    }
}

/// A route-specific Tower layer that executes a selected Passport guard.
#[doc(hidden)]
#[derive(Clone)]
pub struct PassportGuardLayer {
    state: PassportGuardState,
}

impl PassportGuardLayer {
    /// Creates one route-specific Passport middleware layer.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(state: PassportGuardState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for PassportGuardLayer {
    type Service = PassportGuardService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        PassportGuardService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// A Tower service produced by [`PassportGuardLayer`].
#[doc(hidden)]
#[derive(Clone)]
pub struct PassportGuardService<S> {
    inner: S,
    state: PassportGuardState,
}

impl<S> Service<Request> for PassportGuardService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<std::result::Result<(), S::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, mut request: Request) -> Self::Future {
        let state = self.state.clone();
        let not_ready_inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, not_ready_inner);
        Box::pin(async move {
            if let Err(error) = state.authorize(&mut request).await {
                return Ok(super::PassportRejection::from(error).into_response());
            }
            inner.call(request).await
        })
    }
}

impl PassportGuardState {
    async fn authorize(&self, request: &mut Request) -> PassportResult<()> {
        let (headers, method, uri, remote_addr) = request_metadata(request);
        let authentication = authenticate_request(
            &self.application,
            self.guard.source(),
            self.adapter,
            headers,
            method,
            uri,
            remote_addr,
        )
        .await?;
        if !authorizes(self.guard, &authentication) {
            return Err(PassportError::forbidden());
        }
        if !authentication.install_extensions(request.extensions_mut()) {
            return Err(PassportError::internal(std::io::Error::other(
                "guard authentication type binding failed",
            )));
        }
        Ok(())
    }
}

/// A typed Tower layer for protecting a native Axum route with a Passport policy.
///
/// Construct it with [`PassportGuard::builder`] for a managed strategy, or
/// [`PassportGuard::<ClaimsPrincipal<C>>::jwt`] for the built-in typed-claims
/// strategy. Native guards use the same extraction, JWT verification, strategy,
/// policy, extension, and rejection pipeline as generated MADS routes.
///
/// ```no_run
/// use mads_common::{
///     ClaimsPrincipal, PassportGuard, PassportPrincipal,
///     axum::{Router, routing::get},
///     core::Mads,
/// };
///
/// #[derive(serde::Deserialize)]
/// struct UserClaims { role: String }
/// impl PassportPrincipal for UserClaims {
///     fn has_role(&self, role: &str) -> bool { self.role == role }
///     fn has_permission(&self, _: &str) -> bool { false }
/// }
///
/// # async fn example(application: Mads) -> mads_core::Result<()> {
/// let guard = PassportGuard::<ClaimsPrincipal<UserClaims>>::jwt(
///     application.context().clone(),
/// )
/// .roles_any(["user"])
/// .build()?;
/// let router: Router = Router::new()
///     .route("/profile", get(|| async { "ok" }))
///     .route_layer(guard);
/// # let _ = router;
/// # Ok(())
/// # }
/// ```
///
/// Native guards are runtime escape hatches: they are absent from the static
/// route catalog and cannot activate JWT auto-configuration. Their completed
/// application context must already contain [`JwtService`], or `build` returns
/// [`MADS131`].
pub struct PassportGuard<P> {
    state: NativePassportGuardState<P>,
}

impl<P> PassportGuard<P>
where
    P: PassportPrincipal,
{
    /// Starts a native Passport guard builder for the supplied application context.
    #[must_use]
    pub fn builder(application: mads_core::ApplicationContext) -> PassportGuardBuilder<P> {
        PassportGuardBuilder::new(application)
    }
}

impl<C> PassportGuard<ClaimsPrincipal<C>>
where
    C: PassportPrincipal + serde::de::DeserializeOwned,
{
    /// Starts a native guard builder using the built-in `jwt` claims strategy.
    #[must_use]
    pub fn jwt(
        application: mads_core::ApplicationContext,
    ) -> PassportGuardBuilder<ClaimsPrincipal<C>> {
        PassportGuardBuilder::new(application)
            .strategy("jwt")
            .with_builtin_adapter(builtin_claims_adapter::<C>)
    }
}

impl<P> Clone for PassportGuard<P> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<P> fmt::Debug for PassportGuard<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportGuard")
            .field("source", &self.state.source)
            .field("has_roles", &self.state.roles.is_some())
            .field("has_permissions", &self.state.permissions.is_some())
            .field("predicate_count", &self.state.predicates.len())
            .finish_non_exhaustive()
    }
}

/// Configures one typed native [`PassportGuard`] before it is applied to a route.
pub struct PassportGuardBuilder<P> {
    application: mads_core::ApplicationContext,
    strategy: Option<&'static str>,
    source: TokenSource,
    roles: Option<NativePolicyClause>,
    permissions: Option<NativePolicyClause>,
    predicates: Vec<fn(&P) -> bool>,
    builtin_adapter: Option<BuiltinGuardAdapter>,
}

impl<P> PassportGuardBuilder<P>
where
    P: PassportPrincipal,
{
    fn new(application: mads_core::ApplicationContext) -> Self {
        Self {
            application,
            strategy: None,
            source: TokenSource::Bearer,
            roles: None,
            permissions: None,
            predicates: Vec::new(),
            builtin_adapter: None,
        }
    }

    /// Selects the managed Passport strategy registered under `strategy`.
    #[must_use]
    pub const fn strategy(mut self, strategy: &'static str) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Selects the sole credential source accepted by this guard.
    #[must_use]
    pub const fn source(mut self, source: TokenSource) -> Self {
        self.source = source;
        self
    }

    /// Requires at least one listed principal role.
    #[must_use]
    pub fn roles_any<I, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        self.roles = Some(NativePolicyClause::new(PolicyMode::Any, values));
        self
    }

    /// Requires every listed principal role.
    #[must_use]
    pub fn roles_all<I, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        self.roles = Some(NativePolicyClause::new(PolicyMode::All, values));
        self
    }

    /// Requires at least one listed principal permission.
    #[must_use]
    pub fn permissions_any<I, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        self.permissions = Some(NativePolicyClause::new(PolicyMode::Any, values));
        self
    }

    /// Requires every listed principal permission.
    #[must_use]
    pub fn permissions_all<I, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        self.permissions = Some(NativePolicyClause::new(PolicyMode::All, values));
        self
    }

    /// Adds one synchronous principal predicate that must return `true`.
    #[must_use]
    pub fn predicate(mut self, predicate: fn(&P) -> bool) -> Self {
        self.predicates.push(predicate);
        self
    }

    fn with_builtin_adapter(mut self, adapter: BuiltinGuardAdapter) -> Self {
        self.builtin_adapter = Some(adapter);
        self
    }

    /// Validates the selected strategy and produces a reusable Tower layer.
    ///
    /// The application must already contain a concrete [`JwtService`]. Native
    /// guards do not appear in the static MADS route catalog, so they cannot
    /// activate JWT auto-configuration by themselves. Require `JwtService`
    /// from a managed provider before building the application, or provide a
    /// concrete service through [`mads_core::MadsBuilder`] before calling `build`.
    #[allow(clippy::result_large_err)]
    pub fn build(self) -> Result<PassportGuard<P>> {
        let strategy = self.strategy.ok_or_else(|| {
            native_guard_error("native Passport guards require a strategy selected with `strategy`")
        })?;
        validate_native_policy(self.roles.as_ref(), "roles")?;
        validate_native_policy(self.permissions.as_ref(), "permissions")?;

        let descriptor = GuardDescriptor::new(
            "native Axum",
            "PassportGuard",
            strategy,
            Some(native_principal_type_id::<P>),
            Some(native_principal_type_name::<P>),
            self.source,
            None,
            None,
            &[],
            SourceLocation::new("native Axum PassportGuard", 1, 1),
            self.builtin_adapter,
        );
        let guards = [&descriptor];
        let preflight = PassportStrategyCatalog::preflight(&guards)?;
        let binding = preflight
            .binding_for(&descriptor, None)
            .expect("native guard preflight must retain the requested descriptor");
        self.application.resolve::<JwtService>().map_err(|_| {
            native_guard_error("native Passport guards require an available JwtService")
        })?;

        Ok(PassportGuard {
            state: NativePassportGuardState {
                application: self.application,
                source: self.source,
                adapter: binding.adapter(),
                roles: self.roles,
                permissions: self.permissions,
                predicates: self.predicates,
                marker: PhantomData,
            },
        })
    }
}

/// A Tower service created by a native [`PassportGuard`].
#[doc(hidden)]
pub struct NativePassportGuardService<S, P> {
    inner: S,
    state: NativePassportGuardState<P>,
}

impl<S, P> Clone for NativePassportGuardService<S, P>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            state: self.state.clone(),
        }
    }
}

impl<P, S> Layer<S> for PassportGuard<P>
where
    P: PassportPrincipal,
{
    type Service = NativePassportGuardService<S, P>;

    fn layer(&self, inner: S) -> Self::Service {
        NativePassportGuardService {
            inner,
            state: self.state.clone(),
        }
    }
}

impl<P, S> Service<Request> for NativePassportGuardService<S, P>
where
    P: PassportPrincipal,
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<std::result::Result<(), S::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, mut request: Request) -> Self::Future {
        let state = self.state.clone();
        let not_ready_inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, not_ready_inner);
        Box::pin(async move {
            if let Err(error) = state.authorize(&mut request).await {
                return Ok(super::PassportRejection::from(error).into_response());
            }
            inner.call(request).await
        })
    }
}

struct NativePassportGuardState<P> {
    application: mads_core::ApplicationContext,
    source: TokenSource,
    adapter: PassportStrategyAdapter,
    roles: Option<NativePolicyClause>,
    permissions: Option<NativePolicyClause>,
    predicates: Vec<fn(&P) -> bool>,
    marker: PhantomData<fn() -> P>,
}

impl<P> Clone for NativePassportGuardState<P> {
    fn clone(&self) -> Self {
        Self {
            application: self.application.clone(),
            source: self.source,
            adapter: self.adapter,
            roles: self.roles.clone(),
            permissions: self.permissions.clone(),
            predicates: self.predicates.clone(),
            marker: PhantomData,
        }
    }
}

impl<P> NativePassportGuardState<P>
where
    P: PassportPrincipal,
{
    async fn authorize(&self, request: &mut Request) -> PassportResult<()> {
        let (headers, method, uri, remote_addr) = request_metadata(request);
        let authentication = authenticate_request(
            &self.application,
            self.source,
            self.adapter,
            headers,
            method,
            uri,
            remote_addr,
        )
        .await?;
        let Some(principal) = authentication.principal_as::<P>() else {
            return Err(PassportError::internal(std::io::Error::other(
                "native guard authentication type binding failed",
            )));
        };
        if !matches_native_clause(self.roles.as_ref(), |value| principal.has_role(value))
            || !matches_native_clause(self.permissions.as_ref(), |value| {
                principal.has_permission(value)
            })
            || !self
                .predicates
                .iter()
                .all(|predicate| predicate(&principal))
        {
            return Err(PassportError::forbidden());
        }
        if !authentication.install_extensions(request.extensions_mut()) {
            return Err(PassportError::internal(std::io::Error::other(
                "native guard authentication type binding failed",
            )));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct NativePolicyClause {
    mode: PolicyMode,
    values: Vec<String>,
}

impl NativePolicyClause {
    fn new<I, V>(mode: PolicyMode, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        Self {
            mode,
            values: values
                .into_iter()
                .map(|value| value.as_ref().to_owned())
                .collect(),
        }
    }
}

async fn authenticate_request(
    application: &mads_core::ApplicationContext,
    source: TokenSource,
    adapter: PassportStrategyAdapter,
    headers: axum::http::HeaderMap,
    method: axum::http::Method,
    uri: axum::http::Uri,
    remote_addr: Option<std::net::SocketAddr>,
) -> PassportResult<ErasedAuthentication> {
    match source {
        TokenSource::Bearer => {
            let token = bearer_token(&headers)?.to_owned();
            let context = PassportContext::new(&headers, &method, &uri, remote_addr);
            adapter(application, &context, &token).await
        }
        #[cfg(feature = "cookies")]
        TokenSource::Cookie(name) => {
            let jar = CookieJar::from_headers(&headers).map_err(|_| PassportError::reject())?;
            if jar.occurrences(name) != 1 {
                return Err(PassportError::reject());
            }
            let token = jar
                .get(name)
                .map(|cookie| cookie.value().to_owned())
                .ok_or_else(PassportError::reject)?;
            let context = PassportContext::with_cookie_token(
                &headers,
                &method,
                &uri,
                remote_addr,
                &jar,
                name,
            );
            adapter(application, &context, &token).await
        }
    }
}

fn request_metadata(
    request: &Request,
) -> (
    axum::http::HeaderMap,
    axum::http::Method,
    axum::http::Uri,
    Option<std::net::SocketAddr>,
) {
    (
        request.headers().clone(),
        request.method().clone(),
        request.uri().clone(),
        request
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|address| address.0),
    )
}

fn matches_native_clause(
    clause: Option<&NativePolicyClause>,
    matches: impl FnMut(&str) -> bool,
) -> bool {
    let Some(clause) = clause else {
        return true;
    };
    match clause.mode {
        PolicyMode::Any => clause.values.iter().map(String::as_str).any(matches),
        PolicyMode::All => clause.values.iter().map(String::as_str).all(matches),
    }
}

fn validate_native_policy(clause: Option<&NativePolicyClause>, label: &str) -> Result<()> {
    let Some(clause) = clause else {
        return Ok(());
    };
    if clause.values.is_empty()
        || clause
            .values
            .iter()
            .any(|value| value.is_empty() || value.chars().any(char::is_control))
    {
        return Err(native_guard_error(format!(
            "native guard {label} policy must contain non-empty values without control characters"
        )));
    }
    Ok(())
}

fn native_principal_type_id<P>() -> TypeId
where
    P: PassportPrincipal,
{
    TypeId::of::<P>()
}

fn native_principal_type_name<P>() -> &'static str
where
    P: PassportPrincipal,
{
    std::any::type_name::<P>()
}

fn native_guard_error(message: impl Into<String>) -> Error {
    Error::new(
        Diagnostic::new(MADS131, "invalid native Passport guard", message)
            .with_subject("native Axum PassportGuard")
            .with_location(SourceLocation::new("native Axum PassportGuard", 1, 1)),
    )
}

fn builtin_claims_adapter<'a, C>(
    application: &'a mads_core::ApplicationContext,
    _context: &'a PassportContext<'a>,
    token: &'a str,
) -> PassportStrategyFuture<'a>
where
    C: PassportPrincipal + serde::de::DeserializeOwned,
{
    Box::pin(async move {
        let jwt = application
            .resolve::<JwtService>()
            .map_err(PassportError::internal)?;
        let verified = std::sync::Arc::new(
            jwt.verify::<C>(token, JwtValidation::access())
                .map_err(PassportError::from)?,
        );
        let principal = ClaimsPrincipal::<C>::new(std::sync::Arc::clone(&verified));
        Ok(ErasedAuthentication::with_verified(principal, verified))
    })
}

fn bearer_token(headers: &axum::http::HeaderMap) -> PassportResult<&str> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return Err(PassportError::reject());
    };
    if values.next().is_some() {
        return Err(PassportError::reject());
    }
    let value = value.to_str().map_err(|_| PassportError::reject())?;
    let mut parts = value.split_whitespace();
    let Some(scheme) = parts.next() else {
        return Err(PassportError::reject());
    };
    let Some(token) = parts.next() else {
        return Err(PassportError::reject());
    };
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err(PassportError::reject());
    }
    Ok(token)
}

fn authorizes(guard: &GuardDescriptor, authentication: &ErasedAuthentication) -> bool {
    let principal = authentication.principal();
    if !matches_clause(guard.roles(), |value| principal.has_role(value))
        || !matches_clause(guard.permissions(), |value| principal.has_permission(value))
    {
        return false;
    }
    guard.predicates().iter().all(|predicate| {
        predicate
            .adapter()
            .is_some_and(|adapter| adapter(authentication))
    })
}

fn matches_clause(clause: Option<PolicyClause>, matches: impl FnMut(&str) -> bool) -> bool {
    let Some(clause) = clause else {
        return true;
    };
    match clause.mode() {
        PolicyMode::Any => clause.values().iter().copied().any(matches),
        PolicyMode::All => clause.values().iter().copied().all(matches),
    }
}

fn guard_cache() -> &'static Vec<&'static GuardDescriptor> {
    static CACHE: OnceLock<Vec<&'static GuardDescriptor>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut guards: Vec<_> = mads_core::__private::inventory::iter::<&'static GuardDescriptor>
            .into_iter()
            .copied()
            .collect();
        guards.sort_by(guard_order);
        guards
    })
}

fn guard_order(left: &&'static GuardDescriptor, right: &&'static GuardDescriptor) -> Ordering {
    left.route_trait()
        .cmp(right.route_trait())
        .then_with(|| left.handler().cmp(right.handler()))
        .then_with(|| left.namespace().cmp(&right.namespace()))
        .then_with(|| location_order(left.location(), right.location()))
}

fn location_order(left: SourceLocation, right: SourceLocation) -> Ordering {
    left.file
        .cmp(right.file)
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.column.cmp(&right.column))
}

fn validate_guard(guard: &GuardDescriptor) -> Result<()> {
    let subject = guard_subject(guard);
    if guard.route_trait().is_empty() || guard.handler().is_empty() {
        return Err(metadata_error(
            subject,
            "route trait and handler names must be present",
            guard.location(),
        ));
    }
    if !valid_strategy_name(guard.strategy()) {
        return Err(metadata_error(
            subject,
            "guard strategy name must be non-empty lowercase ASCII `[a-z0-9._-]`",
            guard.location(),
        ));
    }
    if guard.principal_type_id().is_none() || guard.principal_type_name().is_none_or(str::is_empty)
    {
        return Err(metadata_error(
            subject,
            "guard principal type metadata must be present",
            guard.location(),
        ));
    }
    #[cfg(feature = "cookies")]
    if let TokenSource::Cookie(name) = guard.source()
        && !valid_cookie_name(name)
    {
        return Err(metadata_error(
            subject,
            "guard cookie token source must use a non-empty RFC cookie name",
            guard.location(),
        ));
    }
    for (label, clause) in [
        ("roles", guard.roles()),
        ("permissions", guard.permissions()),
    ] {
        if let Some(clause) = clause
            && (clause.values().is_empty()
                || clause
                    .values()
                    .iter()
                    .any(|value| value.is_empty() || value.chars().any(char::is_control)))
        {
            return Err(metadata_error(
                subject,
                format!("guard {label} policy must contain non-empty values"),
                guard.location(),
            ));
        }
    }
    if guard
        .predicates()
        .iter()
        .any(|predicate| predicate.name().is_empty() || predicate.adapter().is_none())
    {
        return Err(metadata_error(
            subject,
            "guard predicates must have a name and generated adapter",
            guard.location(),
        ));
    }
    if guard.location().file.is_empty()
        || guard.location().line == 0
        || guard.location().column == 0
    {
        return Err(metadata_error(
            subject,
            "guard source file, line, and column must be present",
            guard.location(),
        ));
    }
    Ok(())
}

fn guard_subject(guard: &GuardDescriptor) -> String {
    format!("{}::{}", guard.route_trait(), guard.handler())
}

fn valid_strategy_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[cfg(feature = "cookies")]
fn valid_cookie_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn metadata_error(subject: String, message: impl Into<String>, location: SourceLocation) -> Error {
    Error::new(
        Diagnostic::new(super::MADS131, "invalid Passport guard metadata", message)
            .with_subject(subject)
            .with_location(location)
            .with_suggestion("correct the guard metadata before starting the application"),
    )
}
