//! Integration tests for connector factory behavior.

use mads_persistence::{DatabaseConnector, DatabaseFactory, PersistenceResult};

#[derive(Debug, Eq, PartialEq)]
struct NativeDatabase(u8);

struct SuccessConnector;

impl DatabaseConnector for SuccessConnector {
    type Database = NativeDatabase;

    async fn connect(self) -> PersistenceResult<Self::Database> {
        Ok(NativeDatabase(7))
    }
}

#[tokio::test]
async fn factory_returns_the_connector_native_type() {
    let value: NativeDatabase = DatabaseFactory.provide(SuccessConnector).await.unwrap();
    assert_eq!(value, NativeDatabase(7));
}
