use std::future::Future;

use crate::PersistenceResult;

/// Connects one concrete native database type.
pub trait DatabaseConnector: Send + Sync + 'static {
    /// The native database value returned after a successful connection.
    type Database: Send + Sync + 'static;

    /// Establishes and returns the native database value.
    fn connect(self) -> impl Future<Output = PersistenceResult<Self::Database>> + Send;
}

/// Provides native database values without registry or lifecycle side effects.
#[derive(Clone, Copy, Debug, Default)]
pub struct DatabaseFactory;

impl DatabaseFactory {
    /// Runs a connector and returns either its native database or typed error.
    pub async fn provide<C>(&self, connector: C) -> PersistenceResult<C::Database>
    where
        C: DatabaseConnector,
    {
        connector.connect().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PersistenceError, PersistenceErrorKind};

    struct FailureConnector;

    impl DatabaseConnector for FailureConnector {
        type Database = ();

        async fn connect(self) -> crate::PersistenceResult<Self::Database> {
            Err(PersistenceError::new(
                PersistenceErrorKind::Connection,
                "connect",
            ))
        }
    }

    #[tokio::test]
    async fn factory_returns_the_connector_error() {
        let error = DatabaseFactory.provide(FailureConnector).await.unwrap_err();
        assert_eq!(error.kind(), PersistenceErrorKind::Connection);
    }
}
