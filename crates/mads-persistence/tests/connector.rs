//! Public PostgreSQL connector contract.
#![cfg(feature = "sea-orm-postgres")]

use mads_persistence::{
    DatabaseFactory, PersistenceErrorKind,
    sea_orm::{ConnectOptions, SeaOrmPostgres},
};

#[tokio::test]
async fn unsupported_schemes_fail_before_connecting() {
    for url in [
        "mysql://localhost/db",
        "sqlite://db.sqlite",
        "localhost/db",
        "",
    ] {
        let result = DatabaseFactory.provide(SeaOrmPostgres::new(url)).await;
        assert_eq!(
            result.unwrap_err().kind(),
            PersistenceErrorKind::UnsupportedScheme
        );
    }
}

#[test]
fn connector_debug_never_discloses_url() {
    let url =
        "postgres://mads-secret-user:mads-secret-password@localhost/mads?token=mads-secret-query";
    let mut connector = SeaOrmPostgres::new(url);
    assert_eq!(connector.options_mut().get_url(), url);
    assert!(!format!("{connector:?}").contains("mads-secret"));
    let from_options = SeaOrmPostgres::from_options(ConnectOptions::new(url));
    assert!(!format!("{from_options:?}").contains("mads-secret"));
}
