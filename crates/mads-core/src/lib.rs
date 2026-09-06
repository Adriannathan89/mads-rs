//! Framework-neutral runtime contracts for MADS.rs.
//!
//! MADS core provides framework-neutral application construction, lifecycle
//! management, deterministic scalar TOML configuration, optional dotenv
//! interpolation, programmatic and environment sources, provider metadata, and
//! dependency graph analysis. Dotenv values only participate in interpolation:
//! they do not mutate process state, and real process variables take
//! precedence. Applications construct configuration explicitly; [`Mads::builder`]
//! does not load files, dotenv variables, or process environment variables.
//!
//! Core also owns deterministic evaluation of official conditional defaults and
//! the public, retained [`AutoConfigurationReport`] records that explain their
//! decisions. Reports retain only stable identifiers, reason codes, and redacted
//! configuration evidence; they never retain resolved configuration values. The
//! v0.5 catalog is complete across statically discovered providers. Module-scoped
//! reachability is deferred to v0.6. Core re-exports the procedural macros
//! without introducing integration dependencies.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod auto_configuration;
mod builder;
mod catalog;
mod config;
mod configuration;
mod context;
mod descriptor;
mod diagnostic;
mod graph;
mod lifecycle;
mod registry;
#[cfg(feature = "runtime-tokio")]
pub mod runtime;

pub use auto_configuration::{
    AutoConfigurationConfigEvidence, AutoConfigurationReasonCode, AutoConfigurationReport,
    AutoConfigurationRequirement, AutoConfigurationStatus,
};
pub use builder::{Mads, MadsBuilder};
pub use catalog::Catalog;
pub use config::{
    Config, ConfigBuilder, ConfigDocument, ConfigSource, ConfigValue, DotenvSource, EnvSource,
    MapSource, TomlSource,
};
pub use configuration::{ConfigurationErrors, ConfigurationIssue, ConfigurationResult, Secret};
pub use context::{ApplicationContext, ConstructionContext};
pub use descriptor::{
    DependencyDescriptor, Module, ModuleDescriptor, ModuleImportDescriptor, ProviderConstructor,
    ProviderDescriptor, ProviderFuture, ProviderKind, ProviderVisibility,
};
pub use diagnostic::{
    Diagnostic, DiagnosticCode, Error, MADS001, MADS002, MADS003, MADS004, MADS005, MADS006,
    MADS007, MADS008, MADS009, MADS010, MADS011, MADS020, MADS030, Result, SourceLocation,
};
pub use graph::{
    ApplicationGraph, ConstructionPlan, ConstructionStep, DependencyEdge, GraphAnalysis,
    ModuleGraph, ModuleImportEdge, ModuleNode, ProviderNode, ProviderOrigin, ProviderOwnership,
    ProviderState,
};
#[doc(hidden)]
pub use graph::{
    AutoConfigurationInspectionSnapshot, DependencyInspectionSnapshot, GraphInspectionSnapshot,
    ModuleImportInspectionSnapshot, ModuleInspectionSnapshot, OwnedSourceLocation,
    ProviderInspectionSnapshot,
};
pub use lifecycle::{LifecycleFuture, LifecycleHook, LifecycleManager, LifecycleState};
pub use registry::{ErasedProvider, ProviderRegistry};

pub use mads_core_macros::{main, module, provider, repository, service};

/// Implementation details used by MADS.rs procedural macro expansions.
#[doc(hidden)]
pub mod __private {
    use std::any::TypeId;

    pub use crate::auto_configuration::{
        AutoConfigurationApplyContext, AutoConfigurationContext, AutoConfigurationContribution,
        AutoConfigurationDescriptor, AutoConfigurationEvaluation,
    };
    pub use inventory;

    /// Builds a rooted module graph for integration coverage and downstream framework crates.
    pub fn build_module_graph<M: crate::Module>() -> crate::Result<crate::ModuleGraph> {
        let modules = crate::Catalog::modules();
        crate::graph::build_module_graph(TypeId::of::<M>(), &modules)
    }
}
