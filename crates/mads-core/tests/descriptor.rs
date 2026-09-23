//! Integration tests for static descriptor metadata contracts.

use std::any::TypeId;
use std::sync::Arc;

use mads_core::{
    Config, ConstructionContext, DependencyDescriptor, ErasedProvider, LifecycleProviderFuture,
    LifecycleResource, ModuleDescriptor, ModuleImportDescriptor, ProviderContribution,
    ProviderDescriptor, ProviderFuture, ProviderKind, ProviderRegistry, ProviderVisibility,
    SourceLocation,
};

struct Dependency;
struct Output;

fn dependency_type_id() -> TypeId {
    TypeId::of::<Dependency>()
}

fn output_type_id() -> TypeId {
    TypeId::of::<Output>()
}

fn output_constructor<'a>(_: &'a ConstructionContext<'a>) -> ProviderFuture<'a> {
    Box::pin(async { Ok(Arc::new(Output) as ErasedProvider) })
}

fn lifecycle_constructor<'a>(_: &'a ConstructionContext<'a>) -> LifecycleProviderFuture<'a> {
    Box::pin(async {
        Ok(ProviderContribution::from_resource(LifecycleResource::new(
            Output,
        )))
    })
}

static DEPENDENCIES: [DependencyDescriptor; 1] = [DependencyDescriptor::new(
    "descriptor::Dependency",
    dependency_type_id,
)];

static MODULE_IMPORTS: [ModuleImportDescriptor; 1] = [ModuleImportDescriptor::new(
    "descriptor::DependencyModule",
    dependency_type_id,
)];

#[tokio::test]
async fn provider_descriptor_preserves_complete_construction_metadata() {
    let location = SourceLocation::new("provider.rs", 12, 34);
    let descriptor = ProviderDescriptor::new(
        ProviderKind::Service,
        "descriptor::Output",
        output_type_id,
        &DEPENDENCIES,
        ProviderVisibility::Public,
        location,
        output_constructor,
    );

    assert_eq!(descriptor.kind(), ProviderKind::Service);
    assert_eq!(descriptor.type_name(), "descriptor::Output");
    assert_eq!(descriptor.type_id(), TypeId::of::<Output>());
    assert_eq!(descriptor.dependencies().len(), 1);
    assert_eq!(descriptor.visibility(), ProviderVisibility::Public);
    assert_eq!(
        descriptor.dependencies()[0].type_name(),
        "descriptor::Dependency"
    );
    assert_eq!(
        descriptor.dependencies()[0].type_id(),
        TypeId::of::<Dependency>()
    );
    assert_eq!(descriptor.location(), location);

    let registry = ProviderRegistry::new();
    let config = Config::empty();
    let context = ConstructionContext::new(&registry, &config);
    let output = (descriptor.constructor())(&context)
        .await
        .expect("descriptor constructor should succeed");
    assert!(Arc::downcast::<Output>(output).is_ok());
}

#[test]
fn provider_namespace_is_additive_and_optional() {
    let plain = ProviderDescriptor::new(
        ProviderKind::Provider,
        "descriptor::Output",
        output_type_id,
        &[],
        ProviderVisibility::Public,
        SourceLocation::new("provider.rs", 1, 1),
        output_constructor,
    );
    assert_eq!(plain.namespace(), None);

    let owned = plain.with_namespace("descriptor");
    assert_eq!(owned.namespace(), Some("descriptor"));
}

#[test]
fn ordinary_descriptor_contract_remains_unchanged_and_lifecycle_is_additive() {
    let plain = ProviderDescriptor::new(
        ProviderKind::Provider,
        "descriptor::Output",
        output_type_id,
        &[],
        ProviderVisibility::Public,
        SourceLocation::new("provider.rs", 1, 1),
        output_constructor,
    );
    assert!(plain.lifecycle_constructor().is_none());

    let lifecycle = plain.with_lifecycle_constructor(lifecycle_constructor);
    assert!(lifecycle.lifecycle_constructor().is_some());
}

#[test]
fn module_descriptor_preserves_identity_and_location() {
    let location = SourceLocation::new("module.rs", 56, 78);
    let descriptor = ModuleDescriptor::new("descriptor::Module", output_type_id, location);

    assert_eq!(descriptor.type_name(), "descriptor::Module");
    assert_eq!(descriptor.type_id(), TypeId::of::<Output>());
    assert_eq!(descriptor.location(), location);
    assert_eq!(descriptor.namespace(), None);
    assert!(descriptor.imports().is_empty());
}

#[test]
fn module_descriptor_tracks_global_status() {
    let descriptor = ModuleDescriptor::new(
        "descriptor::Global",
        output_type_id,
        SourceLocation::new("module.rs", 1, 1),
    );

    assert!(!descriptor.is_global());
    assert!(descriptor.with_global().is_global());
}

#[test]
fn module_descriptor_preserves_namespace_and_imports() {
    let location = SourceLocation::new("module.rs", 56, 78);
    let descriptor = ModuleDescriptor::new("descriptor::Root", output_type_id, location)
        .with_namespace("descriptor")
        .with_imports(&MODULE_IMPORTS);

    assert_eq!(descriptor.namespace(), Some("descriptor"));
    assert_eq!(descriptor.imports().len(), 1);
    assert_eq!(
        descriptor.imports()[0].type_name(),
        "descriptor::DependencyModule"
    );
    assert_eq!(
        descriptor.imports()[0].type_id(),
        TypeId::of::<Dependency>()
    );
}
