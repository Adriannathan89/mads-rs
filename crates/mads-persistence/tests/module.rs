//! Rooted module selection for the native PostgreSQL database provider.
#![cfg(feature = "sea-orm-postgres")]

use mads_core::{Config, LifecycleResource, MADS002, Mads};
use mads_persistence::sea_orm::{DatabaseConnection, DatabaseModule};

mod imported {
    use super::*;

    #[mads_core::module(imports = [DatabaseModule])]
    pub struct Root;

    pub struct Repository;

    #[mads_core::provider]
    pub fn repository(_database: DatabaseConnection) -> Repository {
        Repository
    }
}

mod unimported {
    #[mads_core::module]
    pub struct Root;
}

mod duplicate {
    pub mod other {
        use super::super::DatabaseConnection;

        #[mads_core::module]
        pub struct OtherModule;

        #[mads_core::provider]
        pub fn other_database() -> DatabaseConnection {
            panic!("duplicate provider should fail before construction")
        }
    }

    pub mod app {
        use super::{super::DatabaseModule, other::OtherModule};

        #[mads_core::module(imports = [DatabaseModule, OtherModule])]
        pub struct Root;
    }
}

#[test]
fn imported_module_selects_only_native_database_without_parsing_configuration() {
    let mut builder = Mads::builder_with_config(Config::empty());
    builder.root::<imported::Root>().unwrap();
    for _ in 0..2 {
        let analysis = builder.analyze();
        assert!(analysis.is_valid(), "{:?}", analysis.diagnostics());
        assert!(analysis.graph().provider::<DatabaseConnection>().is_some());
        let ownership = analysis.module_graph().unwrap().provider_ownership();
        assert!(
            ownership.iter().any(|item| {
                item.provider_type_name() == "DatabaseConnection"
                    && item.module_type_name() == Some(std::any::type_name::<DatabaseModule>())
            }),
            "{:?}",
            ownership
                .iter()
                .map(|item| (item.provider_type_name(), item.module_type_name()))
                .collect::<Vec<_>>()
        );
        assert!(
            analysis
                .graph()
                .provider::<imported::Repository>()
                .is_some()
        );
        assert!(
            analysis
                .graph()
                .provider::<LifecycleResource<DatabaseConnection>>()
                .is_none()
        );
    }
}

#[tokio::test]
async fn unimported_root_builds_without_database_configuration() {
    let mut builder = Mads::builder_with_config(Config::empty());
    builder.root::<unimported::Root>().unwrap();
    assert!(
        builder
            .analyze()
            .graph()
            .provider::<DatabaseConnection>()
            .is_none()
    );
    builder.build().await.unwrap();
}

#[test]
fn duplicate_native_database_keeps_existing_diagnostic() {
    let mut builder = Mads::builder_with_config(Config::empty());
    builder.root::<duplicate::app::Root>().unwrap();
    let analysis = builder.analyze();
    assert_eq!(
        analysis.diagnostics()[0].code(),
        MADS002,
        "{:?}",
        analysis.diagnostics()
    );
}
