//! Integration tests for lifecycle-aware provider construction.

use std::any::TypeId;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use mads_core::{
    ApplicationContext, ConstructionContext, Diagnostic, ErasedProvider, Error, LifecycleFuture,
    LifecycleHook, LifecycleResource, MADS006, MADS020, Mads, ProviderDescriptor, ProviderFuture,
    ProviderKind, ProviderVisibility, SourceLocation,
};

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
struct EventLog(Arc<Mutex<Vec<&'static str>>>);

impl EventLog {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn snapshot(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().clone()
    }
}

struct RecordingHook {
    log: EventLog,
    name: &'static str,
}

impl LifecycleHook for RecordingHook {
    fn name(&self) -> &str {
        self.name
    }

    fn start<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async move {
            self.log.0.lock().unwrap().push(match self.name {
                "resource" => "start:resource",
                _ => "start:application",
            });
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async move {
            self.log.0.lock().unwrap().push(match self.name {
                "resource" => "stop:resource",
                _ => "stop:application",
            });
            Ok(())
        })
    }
}

mod automatic {
    use super::*;

    #[mads_core::module]
    pub struct AutomaticRoot;

    #[derive(Clone)]
    pub struct ManagedResource;

    #[mads_core::provider]
    fn event_log() -> EventLog {
        EventLog::new()
    }

    #[mads_core::provider(lifecycle)]
    pub async fn managed_resource(log: EventLog) -> LifecycleResource<ManagedResource> {
        LifecycleResource::new(ManagedResource).with_infrastructure_hook(
            "test.resource",
            RecordingHook {
                log,
                name: "resource",
            },
        )
    }
}

mod failure {
    use super::*;

    #[mads_core::module]
    pub struct FailureRoot;

    #[derive(Clone)]
    pub struct DroppedResource(Arc<DropToken>);

    pub struct DropToken;

    impl Drop for DropToken {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[mads_core::provider]
    fn event_log() -> EventLog {
        EventLog::new()
    }

    #[mads_core::provider(lifecycle)]
    pub async fn dropped_resource(log: EventLog) -> LifecycleResource<DroppedResource> {
        LifecycleResource::new(DroppedResource(Arc::new(DropToken))).with_infrastructure_hook(
            "test.failure",
            RecordingHook {
                log,
                name: "resource",
            },
        )
    }

    #[mads_core::provider]
    pub fn later_failure(resource: DroppedResource) -> mads_core::Result<usize> {
        let _ = Arc::strong_count(&resource.0);
        Err(Error::new(Diagnostic::new(
            MADS020,
            "test constructor failed",
            "the later provider deliberately fails",
        )))
    }
}

mod empty {
    #[mads_core::module]
    pub struct EmptyRoot;
}

struct OrdinaryValue;

fn ordinary_type_id() -> TypeId {
    TypeId::of::<OrdinaryValue>()
}

fn ordinary_constructor<'a>(_: &'a ConstructionContext<'a>) -> ProviderFuture<'a> {
    Box::pin(async { Ok(Arc::new(OrdinaryValue) as ErasedProvider) })
}

inventory::submit! {
    ProviderDescriptor::new(
        ProviderKind::Provider,
        "provider_lifecycle::OrdinaryValue",
        ordinary_type_id,
        &[],
        ProviderVisibility::Private,
        SourceLocation::new(file!(), line!(), column!()),
        ordinary_constructor,
    )
}

#[tokio::test]
async fn automatic_build_registers_native_value_and_contributed_hook() {
    let _guard = TEST_LOCK.lock().await;
    let mut builder = Mads::builder();
    builder.root::<automatic::AutomaticRoot>().unwrap();
    let mut app = builder.build().await.unwrap();

    assert!(
        app.context()
            .resolve::<automatic::ManagedResource>()
            .is_ok()
    );
    assert!(
        app.context()
            .resolve::<LifecycleResource<automatic::ManagedResource>>()
            .is_err()
    );
    let log = app.context().resolve::<EventLog>().unwrap();
    app.start().await.unwrap();
    app.shutdown().await.unwrap();
    assert_eq!(log.snapshot(), ["start:resource", "stop:resource"]);
}

#[tokio::test]
async fn explicit_construction_registers_the_contributed_hook_once() {
    let _guard = TEST_LOCK.lock().await;
    let log = EventLog::new();
    let mut builder = Mads::builder();
    builder.root::<automatic::AutomaticRoot>().unwrap();
    builder.provide(log.clone()).unwrap();
    builder
        .construct::<automatic::ManagedResource>()
        .await
        .unwrap();
    let mut app = builder.build().await.unwrap();

    app.start().await.unwrap();
    app.shutdown().await.unwrap();
    assert_eq!(log.snapshot(), ["start:resource", "stop:resource"]);
}

#[tokio::test]
async fn contributed_infrastructure_wraps_application_lifecycle() {
    let _guard = TEST_LOCK.lock().await;
    let log = EventLog::new();
    let mut builder = Mads::builder();
    builder.root::<automatic::AutomaticRoot>().unwrap();
    builder.provide(log.clone()).unwrap();
    builder.lifecycle_hook(RecordingHook {
        log: log.clone(),
        name: "application",
    });
    let mut app = builder.build().await.unwrap();

    app.start().await.unwrap();
    app.shutdown().await.unwrap();
    assert_eq!(
        log.snapshot(),
        [
            "start:resource",
            "start:application",
            "stop:application",
            "stop:resource",
        ]
    );
}

#[tokio::test]
async fn later_construction_failure_starts_no_hook_and_drops_the_resource() {
    let _guard = TEST_LOCK.lock().await;
    DROPS.store(0, Ordering::SeqCst);
    let log = EventLog::new();
    let mut builder = Mads::builder();
    builder.root::<failure::FailureRoot>().unwrap();
    builder.provide(log.clone()).unwrap();

    let error = match builder.build().await {
        Ok(_) => panic!("the later constructor must fail"),
        Err(error) => error,
    };

    assert_eq!(error.code(), MADS006);
    assert!(log.snapshot().is_empty());
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn hand_authored_ordinary_descriptor_still_constructs() {
    let _guard = TEST_LOCK.lock().await;
    let mut builder = Mads::builder();
    builder.root::<empty::EmptyRoot>().unwrap();
    builder.construct::<OrdinaryValue>().await.unwrap();
    let app = builder.build().await.unwrap();

    assert!(app.context().resolve::<OrdinaryValue>().is_ok());
}
