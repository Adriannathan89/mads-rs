//! Facade contracts for explicit database-to-HTTP delivery mapping.
#![cfg(all(feature = "http", feature = "database"))]

use mads::prelude::*;

fn managed<T>(value: mads::DatabaseResult<T>) -> mads::HttpResult<T> {
    value.into_http()
}

fn native<T>(value: mads::diesel::QueryResult<T>) -> mads::HttpResult<T> {
    value.into_http()
}

fn facade_trait<T>(value: mads::DatabaseResult<T>) -> mads::HttpResult<T> {
    mads::IntoHttpResult::into_http(value)
}

#[test]
fn facade_and_prelude_expose_explicit_database_http_mapping() {
    let _ = managed::<()>;
    let _ = native::<()>;
    let _ = facade_trait::<()>;
}
