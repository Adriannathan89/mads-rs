//! Static metadata contracts emitted by provider and module declarations.

use std::any::TypeId;
use std::future::Future;
use std::pin::Pin;

use crate::{ConstructionContext, ErasedProvider, Result, SourceLocation};

/// Marker implemented by `#[module]` declarations.
pub trait Module: Send + Sync + 'static {}

/// One direct module import emitted by `#[module(imports = [ImportedModule])]`.
pub struct ModuleImportDescriptor {
    type_name: &'static str,
    type_id: fn() -> TypeId,
}

impl ModuleImportDescriptor {
    /// Creates an import descriptor from a stable type name and identifier factory.
    pub const fn new(type_name: &'static str, type_id: fn() -> TypeId) -> Self {
        Self { type_name, type_id }
    }

    /// Returns the imported module's authored type name.
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the imported module's runtime type identifier.
    pub fn type_id(&self) -> TypeId {
        (self.type_id)()
    }
}

/// Categorizes the role a provider plays in an application.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProviderKind {
    /// A provider implementing application service behavior.
    Service,
    /// A provider implementing persistence access behavior.
    Repository,
    /// A general-purpose provider.
    Provider,
}

/// Describes whether a provider declaration is public outside its Rust module.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProviderVisibility {
    /// The provider is public outside its Rust module.
    Public,
    /// The provider is private to its Rust module.
    Private,
}

/// The asynchronous result of constructing an erased provider.
pub type ProviderFuture<'a> = Pin<Box<dyn Future<Output = Result<ErasedProvider>> + Send + 'a>>;

/// Constructs a provider using the dependencies and configuration available at startup.
pub type ProviderConstructor = for<'a> fn(&'a ConstructionContext<'a>) -> ProviderFuture<'a>;

/// Describes a provider dependency by its stable type metadata.
pub struct DependencyDescriptor {
    type_name: &'static str,
    type_id: fn() -> TypeId,
}

impl DependencyDescriptor {
    /// Creates a dependency descriptor from a stable type name and identifier factory.
    pub const fn new(type_name: &'static str, type_id: fn() -> TypeId) -> Self {
        Self { type_name, type_id }
    }

    /// Returns the dependency's stable type name.
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the dependency's runtime type identifier.
    pub fn type_id(&self) -> TypeId {
        (self.type_id)()
    }
}

/// Describes a statically declared provider and its constructor.
pub struct ProviderDescriptor {
    kind: ProviderKind,
    type_name: &'static str,
    type_id: fn() -> TypeId,
    runtime_type_name: Option<fn() -> &'static str>,
    namespace: Option<&'static str>,
    dependencies: &'static [DependencyDescriptor],
    visibility: ProviderVisibility,
    location: SourceLocation,
    constructor: ProviderConstructor,
}

impl ProviderDescriptor {
    /// Creates a provider descriptor from static declaration metadata.
    pub const fn new(
        kind: ProviderKind,
        type_name: &'static str,
        type_id: fn() -> TypeId,
        dependencies: &'static [DependencyDescriptor],
        visibility: ProviderVisibility,
        location: SourceLocation,
        constructor: ProviderConstructor,
    ) -> Self {
        Self {
            kind,
            type_name,
            type_id,
            runtime_type_name: None,
            namespace: None,
            dependencies,
            visibility,
            location,
            constructor,
        }
    }

    /// Attaches the resolved Rust type name emitted by a provider macro.
    ///
    /// This document-hidden metadata lets catalog consumers compare types
    /// across separately compiled crate instances while retaining `TypeId` as
    /// the primary identity mechanism.
    #[doc(hidden)]
    pub const fn with_runtime_type_name(mut self, runtime_type_name: fn() -> &'static str) -> Self {
        self.runtime_type_name = Some(runtime_type_name);
        self
    }

    /// Attaches the Rust namespace containing this provider declaration.
    #[must_use]
    pub const fn with_namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Returns the provider's role.
    pub const fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Returns the provider's stable output type name.
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the provider's runtime output type identifier.
    pub fn type_id(&self) -> TypeId {
        (self.type_id)()
    }

    /// Returns the resolved Rust output type name emitted by a provider macro.
    #[doc(hidden)]
    pub fn runtime_type_name(&self) -> Option<&'static str> {
        self.runtime_type_name
            .map(|runtime_type_name| runtime_type_name())
    }

    /// Returns the Rust namespace containing this provider declaration, when available.
    pub const fn namespace(&self) -> Option<&'static str> {
        self.namespace
    }

    /// Returns the static dependency descriptors required by this provider.
    pub const fn dependencies(&self) -> &'static [DependencyDescriptor] {
        self.dependencies
    }

    /// Returns the provider's declaration visibility metadata.
    pub const fn visibility(&self) -> ProviderVisibility {
        self.visibility
    }

    /// Returns the provider declaration's source location.
    pub const fn location(&self) -> SourceLocation {
        self.location
    }

    /// Returns the provider constructor.
    pub const fn constructor(&self) -> ProviderConstructor {
        self.constructor
    }
}

/// Describes a statically declared application module.
pub struct ModuleDescriptor {
    type_name: &'static str,
    type_id: fn() -> TypeId,
    namespace: Option<&'static str>,
    imports: &'static [ModuleImportDescriptor],
    global: bool,
    location: SourceLocation,
}

impl ModuleDescriptor {
    /// Creates a module descriptor from static declaration metadata.
    pub const fn new(
        type_name: &'static str,
        type_id: fn() -> TypeId,
        location: SourceLocation,
    ) -> Self {
        Self {
            type_name,
            type_id,
            namespace: None,
            imports: &[],
            global: false,
            location,
        }
    }

    /// Attaches the Rust namespace containing this module declaration.
    #[must_use]
    pub const fn with_namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Attaches the module's direct imports in authored declaration order.
    #[must_use]
    pub const fn with_imports(mut self, imports: &'static [ModuleImportDescriptor]) -> Self {
        self.imports = imports;
        self
    }

    /// Marks the module as global, allowing its providers to be accessed from any module.
    #[must_use]
    pub const fn with_global(mut self) -> Self {
        self.global = true;
        self
    }

    /// Returns the module's global declaration status.
    pub const fn is_global(&self) -> bool {
        self.global
    }

    /// Returns the module's stable type name.
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the module's runtime type identifier.
    pub fn type_id(&self) -> TypeId {
        (self.type_id)()
    }

    /// Returns the Rust namespace containing this module declaration, when available.
    pub const fn namespace(&self) -> Option<&'static str> {
        self.namespace
    }

    /// Returns the module's direct imports in authored declaration order.
    pub const fn imports(&self) -> &'static [ModuleImportDescriptor] {
        self.imports
    }

    /// Returns the module declaration's source location.
    pub const fn location(&self) -> SourceLocation {
        self.location
    }
}
