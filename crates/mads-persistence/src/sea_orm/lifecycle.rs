use mads_core::{ApplicationContext, LifecycleFuture, LifecycleHook};

use crate::{PersistenceError, PersistenceErrorKind};

use super::DatabaseConnection;

pub(super) struct SeaOrmLifecycle;

impl LifecycleHook for SeaOrmLifecycle {
    fn name(&self) -> &str {
        "mads.persistence.seaorm.postgres"
    }

    fn start<'a>(&'a self, context: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async move {
            context
                .resolve::<DatabaseConnection>()?
                .ping()
                .await
                .map_err(|source| {
                    mads_core::Error::from(PersistenceError::with_source(
                        PersistenceErrorKind::Readiness,
                        "ping",
                        source,
                    ))
                })
        })
    }

    fn stop<'a>(&'a self, context: &'a ApplicationContext) -> LifecycleFuture<'a> {
        Box::pin(async move {
            context
                .resolve::<DatabaseConnection>()?
                .close_by_ref()
                .await
                .map_err(|source| {
                    mads_core::Error::from(PersistenceError::with_source(
                        PersistenceErrorKind::GracefulClose,
                        "close",
                        source,
                    ))
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use ::sea_orm::{DbBackend, MockDatabase};
    use mads_core::{Config, ProviderRegistry};

    use super::*;

    #[tokio::test]
    async fn mock_database_is_resolved_for_start_and_stop() {
        let database = MockDatabase::new(DbBackend::Postgres).into_connection();
        let mut registry = ProviderRegistry::new();
        registry.insert(database).unwrap();
        let context = ApplicationContext::new(registry, Config::empty());
        let before = context.resolve::<DatabaseConnection>().unwrap();
        let hook = SeaOrmLifecycle;
        assert_eq!(hook.name(), "mads.persistence.seaorm.postgres");
        hook.start(&context).await.unwrap();
        hook.stop(&context).await.unwrap();
        let after = context.resolve::<DatabaseConnection>().unwrap();
        assert!(std::sync::Arc::ptr_eq(&before, &after));
    }
}
