//! Managed Passport strategy metadata and verified-claims adapters.

use std::any::{Any, TypeId, type_name};
use std::cmp::Ordering;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use axum::http::Extensions;
use mads_core::{
    ApplicationContext, Catalog, Diagnostic, Error, ModuleGraph, ProviderDescriptor,
    ProviderVisibility, Result, SourceLocation,
};

use crate::http_scope::{ScopedGuard, owner_for_namespace};
use crate::{JwtClaims, JwtTokenKind, VerifiedJwt};

use super::{
    Authenticated, GuardCatalog, GuardDescriptor, MADS130, PassportContext, PassportPrincipal,
    PassportResult, VerifiedToken,
};

/// A typed application strategy that turns verified JWT claims into a principal.
///
/// Passport always verifies JWT cryptography, registered claims, and token kind
/// before invoking this application-owned validation method. Implementations are
/// registered only when annotated with [`crate::passport_strategy`].
#[allow(async_fn_in_trait)]
pub trait PassportStrategy: Send + Sync + 'static {
    /// The application-owned custom JWT claims consumed by this strategy.
    type Claims: serde::de::DeserializeOwned + Send + Sync + 'static;
    /// The authenticated application principal returned by this strategy.
    type Principal: PassportPrincipal;

    /// The MADS access or refresh token profile accepted by this strategy.
    const TOKEN_KIND: JwtTokenKind;

    /// Validates verified custom claims into an authenticated principal.
    async fn validate(
        &self,
        context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal>;
}

/// The future returned by an erased Passport strategy adapter.
pub type PassportStrategyFuture<'a> =
    Pin<Box<dyn Future<Output = PassportResult<ErasedAuthentication>> + Send + 'a>>;

/// Invokes one concrete Passport strategy after framework JWT verification.
///
/// This function-pointer type is emitted by [`crate::passport_strategy`].
/// Applications should select strategies through guards rather than call an
/// adapter directly. The raw token argument is retained exclusively for
/// framework verification; application validation receives only the verified
/// claims and the credential-sanitized [`PassportContext`].
pub type PassportStrategyAdapter = for<'a> fn(
    &'a ApplicationContext,
    &'a PassportContext<'a>,
    &'a str,
) -> PassportStrategyFuture<'a>;

/// The type-erased result of a successful Passport strategy validation.
///
/// The record preserves the policy-capable principal and exact typed principal
/// and verified-JWT allocations so the guard runtime can install typed request
/// extensions without re-validating claims.
pub struct ErasedAuthentication {
    principal: Arc<dyn PassportPrincipal>,
    exact_principal: Arc<dyn Any + Send + Sync>,
    exact_verified: Arc<dyn Any + Send + Sync>,
    principal_type_id: TypeId,
    principal_type_name: &'static str,
    claims_type_id: TypeId,
    claims_type_name: &'static str,
    install_extensions: ExtensionInstaller,
}

type ExtensionInstaller = fn(&ErasedAuthentication, &mut Extensions) -> bool;

impl ErasedAuthentication {
    /// Creates a type-erased authentication record for macro-generated adapters.
    #[doc(hidden)]
    pub fn new<P, C>(principal: P, verified: VerifiedJwt<C>) -> Self
    where
        P: PassportPrincipal,
        C: Send + Sync + 'static,
    {
        let exact_principal = Arc::new(principal);
        let policy_principal: Arc<dyn PassportPrincipal> = exact_principal.clone();
        let exact_verified = Arc::new(verified);
        Self {
            principal: policy_principal,
            exact_principal,
            exact_verified,
            principal_type_id: TypeId::of::<P>(),
            principal_type_name: type_name::<P>(),
            claims_type_id: TypeId::of::<C>(),
            claims_type_name: type_name::<C>(),
            install_extensions: install_typed_extensions::<P, C>,
        }
    }

    /// Creates an erased authentication record from a shared verified JWT.
    ///
    /// The generated built-in `ClaimsPrincipal<C>` adapter uses this to retain
    /// one allocation for both the principal and typed token extraction.
    #[doc(hidden)]
    pub fn with_verified<P, C>(principal: P, verified: Arc<VerifiedJwt<C>>) -> Self
    where
        P: PassportPrincipal,
        C: Send + Sync + 'static,
    {
        let exact_principal = Arc::new(principal);
        let policy_principal: Arc<dyn PassportPrincipal> = exact_principal.clone();
        let exact_verified: Arc<dyn Any + Send + Sync> = verified;
        Self {
            principal: policy_principal,
            exact_principal,
            exact_verified,
            principal_type_id: TypeId::of::<P>(),
            principal_type_name: type_name::<P>(),
            claims_type_id: TypeId::of::<C>(),
            claims_type_name: type_name::<C>(),
            install_extensions: install_typed_extensions::<P, C>,
        }
    }

    /// Returns the policy-capable application principal.
    #[must_use]
    pub fn principal(&self) -> &(dyn PassportPrincipal + 'static) {
        self.principal.as_ref()
    }

    /// Returns the exact principal type identifier.
    #[must_use]
    pub const fn principal_type_id(&self) -> TypeId {
        self.principal_type_id
    }

    /// Returns the exact principal type name.
    #[must_use]
    pub const fn principal_type_name(&self) -> &'static str {
        self.principal_type_name
    }

    /// Returns the exact verified custom-claims type identifier.
    #[must_use]
    pub const fn claims_type_id(&self) -> TypeId {
        self.claims_type_id
    }

    /// Returns the exact verified custom-claims type name.
    #[must_use]
    pub const fn claims_type_name(&self) -> &'static str {
        self.claims_type_name
    }

    /// Returns the exact principal allocation when it has type `P`.
    #[doc(hidden)]
    pub fn principal_as<P>(&self) -> Option<Arc<P>>
    where
        P: PassportPrincipal,
    {
        Arc::downcast::<P>(Arc::clone(&self.exact_principal)).ok()
    }

    /// Returns the exact verified-JWT allocation when its custom claims are `C`.
    #[doc(hidden)]
    pub fn verified_as<C>(&self) -> Option<Arc<VerifiedJwt<C>>>
    where
        C: Send + Sync + 'static,
    {
        Arc::downcast::<VerifiedJwt<C>>(Arc::clone(&self.exact_verified)).ok()
    }

    pub(crate) fn install_extensions(&self, extensions: &mut Extensions) -> bool {
        (self.install_extensions)(self, extensions)
    }
}

fn install_typed_extensions<P, C>(
    authentication: &ErasedAuthentication,
    extensions: &mut Extensions,
) -> bool
where
    P: PassportPrincipal,
    C: Send + Sync + 'static,
{
    let Some(principal) = authentication.principal_as::<P>() else {
        return false;
    };
    let Some(verified) = authentication.verified_as::<C>() else {
        return false;
    };
    extensions.insert(Authenticated::new(principal));
    extensions.insert(VerifiedToken::new(verified));
    true
}

impl fmt::Debug for ErasedAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ErasedAuthentication")
            .field("principal_type", &self.principal_type_name)
            .field("claims_type", &self.claims_type_name)
            .finish_non_exhaustive()
    }
}

/// Static metadata emitted for one managed Passport strategy implementation.
pub struct PassportStrategyDescriptor {
    name: &'static str,
    provider_type_id: fn() -> TypeId,
    provider_type_name: fn() -> &'static str,
    namespace: Option<&'static str>,
    claims_type_id: fn() -> TypeId,
    claims_type_name: fn() -> &'static str,
    principal_type_id: fn() -> TypeId,
    principal_type_name: fn() -> &'static str,
    token_kind: JwtTokenKind,
    location: SourceLocation,
    adapter: PassportStrategyAdapter,
}

impl PassportStrategyDescriptor {
    /// Creates metadata emitted by [`crate::passport_strategy`].
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        name: &'static str,
        provider_type_id: fn() -> TypeId,
        provider_type_name: fn() -> &'static str,
        claims_type_id: fn() -> TypeId,
        claims_type_name: fn() -> &'static str,
        principal_type_id: fn() -> TypeId,
        principal_type_name: fn() -> &'static str,
        token_kind: JwtTokenKind,
        location: SourceLocation,
        adapter: PassportStrategyAdapter,
    ) -> Self {
        Self {
            name,
            provider_type_id,
            provider_type_name,
            namespace: None,
            claims_type_id,
            claims_type_name,
            principal_type_id,
            principal_type_name,
            token_kind,
            location,
            adapter,
        }
    }

    /// Returns the configured strategy name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Attaches the Rust namespace containing this strategy declaration.
    #[doc(hidden)]
    #[must_use]
    pub const fn with_namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Returns the Rust namespace containing this strategy declaration, when available.
    #[doc(hidden)]
    pub const fn namespace(&self) -> Option<&'static str> {
        self.namespace
    }

    /// Returns the concrete managed provider type identifier.
    #[must_use]
    pub fn provider_type_id(&self) -> TypeId {
        (self.provider_type_id)()
    }

    /// Returns the concrete managed provider type name.
    #[must_use]
    pub fn provider_type_name(&self) -> &'static str {
        (self.provider_type_name)()
    }

    /// Returns the custom JWT claims type identifier.
    #[must_use]
    pub fn claims_type_id(&self) -> TypeId {
        (self.claims_type_id)()
    }

    /// Returns the custom JWT claims type name.
    #[must_use]
    pub fn claims_type_name(&self) -> &'static str {
        (self.claims_type_name)()
    }

    /// Returns the authenticated principal type identifier.
    #[must_use]
    pub fn principal_type_id(&self) -> TypeId {
        (self.principal_type_id)()
    }

    /// Returns the authenticated principal type name.
    #[must_use]
    pub fn principal_type_name(&self) -> &'static str {
        (self.principal_type_name)()
    }

    /// Returns the token profile required before this strategy runs.
    #[must_use]
    pub const fn token_kind(&self) -> JwtTokenKind {
        self.token_kind
    }

    /// Returns the strategy declaration source location.
    #[must_use]
    pub const fn location(&self) -> SourceLocation {
        self.location
    }

    /// Returns the generated framework adapter.
    #[doc(hidden)]
    #[must_use]
    pub const fn adapter(&self) -> PassportStrategyAdapter {
        self.adapter
    }
}

impl fmt::Debug for PassportStrategyDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportStrategyDescriptor")
            .field("name", &self.name)
            .field("provider_type", &self.provider_type_name())
            .field("namespace", &self.namespace)
            .field("claims_type", &self.claims_type_name())
            .field("principal_type", &self.principal_type_name())
            .field("token_kind", &self.token_kind)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}

mads_core::__private::inventory::collect!(PassportStrategyDescriptor);

/// Read-only, deterministic inspection of managed Passport strategies.
pub struct PassportStrategyCatalog;

impl PassportStrategyCatalog {
    /// Returns every linked strategy descriptor in deterministic order.
    ///
    /// Descriptors are sorted by strategy name, provider type name, and source
    /// location. Inspection does not resolve providers or invoke strategies.
    #[must_use]
    pub fn strategies() -> Vec<&'static PassportStrategyDescriptor> {
        strategy_cache().clone()
    }

    /// Validates all registered strategy metadata and resolves guarded routes.
    ///
    /// The returned bindings retain only static descriptors, function pointers,
    /// and type metadata. They never construct a provider, resolve an
    /// application value, or inspect JWT configuration.
    ///
    /// # Errors
    ///
    /// Returns `MADS130` for duplicate, unmanaged, missing, ambiguous, or
    /// principal-incompatible strategies. Invalid guard metadata is rejected
    /// with `MADS131` before strategy selection.
    #[allow(clippy::result_large_err)]
    pub fn preflight<'a>(
        guards: &'a [&'a GuardDescriptor],
    ) -> Result<PassportStrategyPreflight<'a>> {
        GuardCatalog::validate_descriptors(guards)?;

        let strategies = Self::strategies();
        validate_strategy_catalog(&strategies)?;

        let mut guards = guards.to_vec();
        guards.sort_by(guard_order);
        let mut bindings = Vec::with_capacity(guards.len());
        for guard in guards {
            bindings.push(resolve_guard(guard, &strategies)?);
        }
        Ok(PassportStrategyPreflight { bindings })
    }

    /// Resolves strategies for guards selected from one optional module graph.
    ///
    /// Rootless guards retain the complete-catalog duplicate and selection
    /// behavior. Rooted guards see custom strategies only from their own
    /// module or a directly imported public provider module.
    #[allow(clippy::result_large_err)]
    pub(crate) fn preflight_scoped(
        module_graph: Option<&ModuleGraph>,
        guards: &[ScopedGuard],
    ) -> Result<PassportStrategyPreflight<'static>> {
        let guard_descriptors = guards.iter().map(ScopedGuard::guard).collect::<Vec<_>>();
        GuardCatalog::validate_descriptors(&guard_descriptors)?;

        let strategies = Self::strategies();
        if module_graph.is_none() {
            validate_strategy_catalog(&strategies)?;
        } else {
            validate_strategy_metadata(&strategies)?;
        }
        let providers = Catalog::providers();

        let mut ordered_guards = guards.iter().collect::<Vec<_>>();
        ordered_guards.sort_by(|left, right| guard_order(&left.guard(), &right.guard()));

        let mut bindings = Vec::with_capacity(ordered_guards.len());
        for scoped_guard in ordered_guards {
            let mut binding =
                resolve_scoped_guard(scoped_guard, module_graph, &strategies, &providers)?;
            binding.context_module = scoped_guard.context_module();
            bindings.push(binding);
        }
        Ok(PassportStrategyPreflight { bindings })
    }
}

/// The deterministic static strategy selection for one guarded route.
///
/// This record contains no application-owned values. Its adapter obtains a
/// managed strategy only after ordinary application construction has succeeded.
pub struct PassportStrategyBinding<'a> {
    guard: &'a GuardDescriptor,
    context_module: Option<TypeId>,
    strategy: &'static str,
    adapter: PassportStrategyAdapter,
    token_kind: JwtTokenKind,
    builtin: bool,
}

impl<'a> PassportStrategyBinding<'a> {
    /// Returns the effective static guard.
    #[doc(hidden)]
    #[must_use]
    pub const fn guard(&self) -> &'a GuardDescriptor {
        self.guard
    }

    /// Returns the selected module context for this guard occurrence.
    #[doc(hidden)]
    #[must_use]
    pub const fn context_module(&self) -> Option<TypeId> {
        self.context_module
    }

    /// Returns the selected strategy name.
    #[doc(hidden)]
    #[must_use]
    pub const fn strategy(&self) -> &'static str {
        self.strategy
    }

    /// Returns the adapter selected during preflight.
    #[doc(hidden)]
    #[must_use]
    pub const fn adapter(&self) -> PassportStrategyAdapter {
        self.adapter
    }

    /// Returns the token profile enforced by the selected adapter.
    #[doc(hidden)]
    #[must_use]
    pub const fn token_kind(&self) -> JwtTokenKind {
        self.token_kind
    }

    /// Returns whether this binding uses the built-in typed-claims adapter.
    #[doc(hidden)]
    #[must_use]
    pub const fn is_builtin(&self) -> bool {
        self.builtin
    }
}

impl fmt::Debug for PassportStrategyBinding<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportStrategyBinding")
            .field("guard", &self.guard.requirement_subject())
            .field("strategy", &self.strategy)
            .field("token_kind", &self.token_kind)
            .field("builtin", &self.builtin)
            .finish()
    }
}

/// The complete deterministic selection generated during Passport preflight.
///
/// Preflight is intentionally metadata-only. Request-time guard execution uses
/// these bindings later to resolve providers from a completed application.
pub struct PassportStrategyPreflight<'a> {
    bindings: Vec<PassportStrategyBinding<'a>>,
}

impl<'a> PassportStrategyPreflight<'a> {
    /// Returns every selected guard binding in deterministic route order.
    #[must_use]
    pub fn bindings(&self) -> &[PassportStrategyBinding<'a>] {
        &self.bindings
    }

    /// Finds the selected binding for one guard descriptor in one module context.
    #[doc(hidden)]
    #[must_use]
    pub fn binding_for(
        &self,
        guard: &GuardDescriptor,
        context_module: Option<TypeId>,
    ) -> Option<&PassportStrategyBinding<'a>> {
        self.bindings.iter().find(|binding| {
            std::ptr::eq(binding.guard, guard) && binding.context_module == context_module
        })
    }
}

impl fmt::Debug for PassportStrategyPreflight<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportStrategyPreflight")
            .field("bindings", &self.bindings)
            .finish()
    }
}

fn strategy_cache() -> &'static Vec<&'static PassportStrategyDescriptor> {
    static CACHE: OnceLock<Vec<&'static PassportStrategyDescriptor>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut strategies: Vec<_> =
            mads_core::__private::inventory::iter::<PassportStrategyDescriptor>
                .into_iter()
                .collect();
        strategies.sort_by(strategy_order);
        strategies
    })
}

fn strategy_order(
    left: &&'static PassportStrategyDescriptor,
    right: &&'static PassportStrategyDescriptor,
) -> Ordering {
    left.name()
        .cmp(right.name())
        .then_with(|| left.provider_type_name().cmp(right.provider_type_name()))
        .then_with(|| left.namespace().cmp(&right.namespace()))
        .then_with(|| location_order(left.location(), right.location()))
}

fn guard_order(left: &&GuardDescriptor, right: &&GuardDescriptor) -> Ordering {
    left.route_trait()
        .cmp(right.route_trait())
        .then_with(|| left.handler().cmp(right.handler()))
        .then_with(|| left.namespace().cmp(&right.namespace()))
        .then_with(|| location_order(left.location(), right.location()))
}

fn validate_strategy_catalog(strategies: &[&'static PassportStrategyDescriptor]) -> Result<()> {
    validate_unique_strategy_names(strategies)?;
    validate_strategy_metadata(strategies)
}

fn validate_unique_strategy_names(
    strategies: &[&'static PassportStrategyDescriptor],
) -> Result<()> {
    let duplicate_groups = strategies
        .chunk_by(|left, right| left.name() == right.name())
        .filter(|group| group.len() > 1)
        .collect::<Vec<_>>();
    if let Some((first, rest)) = duplicate_groups.split_first() {
        let primary = duplicate_strategy_error(first);
        let related = rest.iter().map(|group| duplicate_strategy_error(group));
        return Err(Error::from_diagnostics(primary, related));
    }
    Ok(())
}

fn validate_strategy_metadata(strategies: &[&'static PassportStrategyDescriptor]) -> Result<()> {
    let providers = Catalog::providers();
    for strategy in strategies {
        validate_reserved_strategy_token_kind(strategy)?;
        let candidates = providers
            .iter()
            .copied()
            .filter(|provider| provider_matches_strategy(provider, strategy))
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [_provider] => {}
            [] => {
                return Err(strategy_error(
                    "unmanaged_strategy",
                    "unmanaged Passport strategy",
                    strategy.provider_type_name(),
                    "the strategy implementation is not registered as exactly one managed provider",
                    strategy.location(),
                    "annotate the concrete strategy type with a MADS managed-provider attribute",
                ));
            }
            _ => {
                return Err(strategy_error(
                    "ambiguous_strategy",
                    "ambiguous Passport strategy provider",
                    strategy.provider_type_name(),
                    "more than one managed provider descriptor matches this strategy implementation",
                    strategy.location(),
                    "retain exactly one managed provider declaration for the strategy type",
                ));
            }
        }
    }
    Ok(())
}

fn validate_reserved_strategy_token_kind(strategy: &PassportStrategyDescriptor) -> Result<()> {
    let (expected, suggestion) = match strategy.name() {
        "jwt" => (
            JwtTokenKind::Access,
            "declare `JwtTokenKind::Access` for the reserved `jwt` strategy",
        ),
        "jwt-refresh" => (
            JwtTokenKind::Refresh,
            "declare `JwtTokenKind::Refresh` for the reserved `jwt-refresh` strategy",
        ),
        _ => return Ok(()),
    };

    if strategy.token_kind() == expected {
        return Ok(());
    }

    Err(strategy_error(
        "reserved_strategy_token_kind",
        "Passport strategy token kind mismatch",
        strategy.name(),
        format!(
            "the reserved `{}` strategy accepts only {} tokens, but its adapter declares {} tokens",
            strategy.name(),
            expected.claim_value(),
            strategy.token_kind().claim_value(),
        ),
        strategy.location(),
        suggestion,
    ))
}

fn duplicate_strategy_error(group: &[&'static PassportStrategyDescriptor]) -> Diagnostic {
    let strategy = group
        .first()
        .expect("duplicate strategy groups always have an entry");
    Diagnostic::new(
        MADS130,
        "duplicate Passport strategy",
        "duplicate_strategy: more than one managed strategy uses this name",
    )
    .with_subject(strategy.name())
    .with_location(strategy.location())
    .with_suggestion("use a unique Passport strategy name")
}

fn resolve_guard<'a>(
    guard: &'a GuardDescriptor,
    strategies: &[&'static PassportStrategyDescriptor],
) -> Result<PassportStrategyBinding<'a>> {
    if let Some(strategy) = strategies
        .iter()
        .copied()
        .find(|strategy| strategy.name() == guard.strategy())
    {
        return resolve_custom_strategy(guard, strategy);
    }

    resolve_builtin_or_missing(guard)
}

fn resolve_scoped_guard(
    scoped_guard: &ScopedGuard,
    module_graph: Option<&ModuleGraph>,
    strategies: &[&'static PassportStrategyDescriptor],
    providers: &[&'static ProviderDescriptor],
) -> Result<PassportStrategyBinding<'static>> {
    let guard = scoped_guard.guard();
    let candidates = strategies
        .iter()
        .copied()
        .filter(|strategy| strategy.name() == guard.strategy())
        .filter(|strategy| {
            scoped_strategy_visible(
                module_graph,
                scoped_guard.context_module(),
                strategy,
                providers,
            )
        })
        .collect::<Vec<_>>();

    match candidates.as_slice() {
        [] => resolve_builtin_or_missing(guard),
        [strategy] => resolve_custom_strategy(guard, strategy),
        _ => Err(ambiguous_scoped_strategy_error(
            guard,
            scoped_guard.context_module(),
            &candidates,
        )),
    }
}

fn scoped_strategy_visible(
    module_graph: Option<&ModuleGraph>,
    guard_context: Option<TypeId>,
    strategy: &PassportStrategyDescriptor,
    providers: &[&'static ProviderDescriptor],
) -> bool {
    let (Some(graph), Some(guard_context)) = (module_graph, guard_context) else {
        return true;
    };
    let Some(namespace) = strategy.namespace() else {
        return false;
    };
    let Some(strategy_owner) = owner_for_namespace(Some(namespace)) else {
        return false;
    };
    let provider = providers
        .iter()
        .copied()
        .find(|provider| provider_matches_strategy(provider, strategy))
        .expect("strategy metadata was validated before scoped resolution");

    strategy_visible(
        graph,
        guard_context,
        strategy_owner.type_id(),
        provider.visibility(),
    )
}

fn provider_matches_strategy(
    provider: &ProviderDescriptor,
    strategy: &PassportStrategyDescriptor,
) -> bool {
    provider.type_id() == strategy.provider_type_id()
        && provider.runtime_type_name() == Some(strategy.provider_type_name())
}

fn strategy_visible(
    graph: &ModuleGraph,
    guard_context: TypeId,
    strategy_owner: TypeId,
    provider_visibility: ProviderVisibility,
) -> bool {
    guard_context == strategy_owner
        || (provider_visibility == ProviderVisibility::Public
            && graph.directly_imports(guard_context, strategy_owner))
}

fn resolve_custom_strategy<'a>(
    guard: &'a GuardDescriptor,
    strategy: &'static PassportStrategyDescriptor,
) -> Result<PassportStrategyBinding<'a>> {
    let principal_type_id = guard
        .principal_type_id()
        .expect("guard metadata was validated before strategy resolution");
    if principal_type_id != strategy.principal_type_id() {
        return Err(strategy_error(
            "principal_mismatch",
            "Passport strategy principal mismatch",
            guard.requirement_subject(),
            format!(
                "the guard requires `{}` but strategy `{}` returns `{}`",
                guard
                    .principal_type_name()
                    .expect("guard metadata was validated before strategy resolution"),
                strategy.name(),
                strategy.principal_type_name(),
            ),
            guard.location(),
            "declare the strategy principal type requested by the guard",
        ));
    }
    Ok(PassportStrategyBinding {
        guard,
        context_module: None,
        strategy: strategy.name(),
        adapter: strategy.adapter(),
        token_kind: strategy.token_kind(),
        builtin: false,
    })
}

fn resolve_builtin_or_missing(guard: &GuardDescriptor) -> Result<PassportStrategyBinding<'_>> {
    if guard.strategy() == "jwt"
        && let Some(adapter) = guard.builtin_adapter()
    {
        return Ok(PassportStrategyBinding {
            guard,
            context_module: None,
            strategy: "jwt",
            adapter,
            token_kind: JwtTokenKind::Access,
            builtin: true,
        });
    }

    Err(strategy_error(
        "missing_strategy",
        "missing Passport strategy",
        guard.requirement_subject(),
        format!(
            "no managed strategy named `{}` can authenticate this guard",
            guard.strategy()
        ),
        guard.location(),
        "register a matching managed Passport strategy or use ClaimsPrincipal<C> with `jwt`",
    ))
}

fn ambiguous_scoped_strategy_error(
    guard: &'static GuardDescriptor,
    guard_context: Option<TypeId>,
    candidates: &[&'static PassportStrategyDescriptor],
) -> Error {
    let context_name = guard_context.map_or_else(
        || "complete Passport catalog".to_owned(),
        |context| format!("{context:?}"),
    );
    let primary = Diagnostic::new(
        MADS130,
        "ambiguous Passport strategy",
        format!(
            "ambiguous_strategy: guard context `{context_name}` can see more than one managed strategy named `{}`",
            guard.strategy(),
        ),
    )
    .with_subject(guard.requirement_subject())
    .with_location(guard.location())
    .with_suggestion("retain one context-visible Passport strategy for this guard");
    let related = candidates.iter().map(|strategy| {
        Diagnostic::new(
            MADS130,
            "visible Passport strategy candidate",
            format!(
                "candidate_strategy: `{}` is visible to guard context `{context_name}`",
                strategy.provider_type_name(),
            ),
        )
        .with_subject(strategy.name())
        .with_location(strategy.location())
    });
    Error::from_diagnostics(primary, related)
}

fn strategy_error(
    reason: &'static str,
    title: &'static str,
    subject: impl Into<String>,
    message: impl Into<String>,
    location: SourceLocation,
    suggestion: &'static str,
) -> Error {
    Error::new(
        Diagnostic::new(MADS130, title, format!("{reason}: {}", message.into()))
            .with_subject(subject)
            .with_location(location)
            .with_suggestion(suggestion),
    )
}

fn location_order(left: SourceLocation, right: SourceLocation) -> Ordering {
    left.file
        .cmp(right.file)
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.column.cmp(&right.column))
}
