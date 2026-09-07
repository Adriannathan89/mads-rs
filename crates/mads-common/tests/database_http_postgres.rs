//! Real PostgreSQL evidence for explicit database-to-HTTP error mapping.

#![cfg(all(feature = "http", feature = "database"))]
#![allow(missing_docs)]

use std::error::Error as StdError;

use mads_common::{
    Database, DatabaseConfig, DatabaseError, HttpError, IntoHttpResult,
    axum::{
        body::to_bytes,
        http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
        response::IntoResponse,
    },
    diesel::{
        Connection, ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl,
        result::{DatabaseErrorKind, Error as DieselError},
        sql_query,
    },
};

const TABLE_NAME: &str = "mads_common_v080_database_http_mapping";
const UNIQUE_CONSTRAINT: &str = "mads_common_v080_database_http_mapping_unique_value_key";
const UNIQUE_VALUE: &str = "mads-http-mapping-duplicate-value";
const UNIQUE_COLUMN: &str = "unique_value";
const REQUIRED_COLUMN: &str = "required_value";

const CREATE_TABLE: &str = "
    CREATE TABLE mads_common_v080_database_http_mapping (
        id INTEGER PRIMARY KEY,
        unique_value TEXT NOT NULL,
        required_value TEXT NOT NULL,
        CONSTRAINT mads_common_v080_database_http_mapping_unique_value_key UNIQUE (unique_value)
    )
";
const DROP_TABLE: &str = "DROP TABLE IF EXISTS mads_common_v080_database_http_mapping";
const INSERT_SEED_ROW: &str = "
    INSERT INTO mads_common_v080_database_http_mapping (id, unique_value, required_value)
    VALUES (1, 'mads-http-mapping-duplicate-value', 'mads-http-mapping-required-value')
";
const INSERT_DUPLICATE_ROW: &str = "
    INSERT INTO mads_common_v080_database_http_mapping (id, unique_value, required_value)
    VALUES (2, 'mads-http-mapping-duplicate-value', 'mads-http-mapping-second-value')
";
const INSERT_NULL_REQUIRED_VALUE: &str = "
    INSERT INTO mads_common_v080_database_http_mapping (id, unique_value)
    VALUES (3, 'mads-http-mapping-null-value')
";

const NOT_FOUND_BODY: &str = r#"{"error":{"code":"not_found","message":"resource not found"}}"#;
const CONFLICT_BODY: &str = r#"{"error":{"code":"conflict","message":"resource already exists"}}"#;
const INTERNAL_BODY: &str = r#"{"error":{"code":"internal","message":"internal server error"}}"#;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

mads_common::diesel::table! {
    mads_common_v080_database_http_mapping (id) {
        id -> Integer,
        unique_value -> Text,
        required_value -> Text,
    }
}

struct TestTable {
    connection: PgConnection,
}

impl TestTable {
    fn create() -> Self {
        let database_url = std::env::var("MADS_TEST_DATABASE_URL")
            .expect("MADS_TEST_DATABASE_URL is required for ignored PostgreSQL tests");
        let mut connection = PgConnection::establish(&database_url)
            .expect("PostgreSQL test connection should be established");

        sql_query(DROP_TABLE)
            .execute(&mut connection)
            .expect("previous HTTP mapping test table should be removable");
        sql_query(CREATE_TABLE)
            .execute(&mut connection)
            .expect("HTTP mapping test table should be created");
        sql_query(INSERT_SEED_ROW)
            .execute(&mut connection)
            .expect("HTTP mapping test row should be inserted");

        Self { connection }
    }

    fn connection(&mut self) -> &mut PgConnection {
        &mut self.connection
    }
}

impl Drop for TestTable {
    fn drop(&mut self) {
        let _ = sql_query(DROP_TABLE).execute(&mut self.connection);
    }
}

fn test_database() -> Database {
    let database_url = std::env::var("MADS_TEST_DATABASE_URL")
        .expect("MADS_TEST_DATABASE_URL is required for ignored PostgreSQL tests");
    Database::from_config(&DatabaseConfig::new(database_url).expect("test database URL is valid"))
        .expect("MADS database pool should be configured")
}

fn missing_row(connection: &mut PgConnection, id: i32) -> diesel::QueryResult<i32> {
    use mads_common_v080_database_http_mapping::dsl;

    dsl::mads_common_v080_database_http_mapping
        .filter(dsl::id.eq(id))
        .select(dsl::id)
        .first(connection)
}

async fn assert_response(
    error: HttpError,
    expected_status: StatusCode,
    expected_body: &str,
    secrets: &[&str],
) {
    let formatting = format!("{error}\n{error:?}");
    let response = error.into_response();

    assert_eq!(response.status(), expected_status);
    assert_eq!(
        response.headers().get(CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/json"))
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("mapped response body must be readable");
    assert_eq!(body.as_ref(), expected_body.as_bytes());
    let body = String::from_utf8(body.to_vec()).expect("mapped response body must be UTF-8");

    for secret in secrets {
        assert!(
            !formatting.contains(secret),
            "mapped error formatting must not reveal {secret}"
        );
        assert!(
            !body.contains(secret),
            "mapped response body must not reveal {secret}"
        );
    }
}

fn managed_diesel_source(error: &HttpError) -> &DieselError {
    let database_error = StdError::source(error)
        .expect("mapped managed error must retain the database wrapper")
        .downcast_ref::<DatabaseError>()
        .expect("mapped managed error must retain the original database wrapper");
    StdError::source(database_error)
        .expect("managed query error must retain the Diesel source")
        .downcast_ref::<DieselError>()
        .expect("managed query error must retain the original Diesel source")
}

fn native_diesel_source(error: &HttpError) -> &DieselError {
    StdError::source(error)
        .expect("mapped native error must retain the Diesel source")
        .downcast_ref::<DieselError>()
        .expect("mapped native error must retain the original Diesel source")
}

fn assert_not_found(error: &DieselError) {
    assert!(matches!(error, DieselError::NotFound));
}

fn assert_unique_violation(error: &DieselError) {
    assert!(matches!(
        error,
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)
    ));
}

fn assert_not_null_violation(error: &DieselError) {
    assert!(matches!(
        error,
        DieselError::DatabaseError(DatabaseErrorKind::NotNullViolation, _)
    ));
}

#[tokio::test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
async fn managed_database_run_maps_real_postgres_errors_without_leaking_details() {
    let _guard = TEST_LOCK.lock().await;
    let _table = TestTable::create();
    let database = test_database();

    let missing = database
        .run(|connection| missing_row(connection, -101))
        .await
        .into_http()
        .unwrap_err();
    assert_not_found(managed_diesel_source(&missing));
    assert_response(missing, StatusCode::NOT_FOUND, NOT_FOUND_BODY, &[]).await;

    let duplicate = database
        .run(|connection| sql_query(INSERT_DUPLICATE_ROW).execute(connection))
        .await
        .into_http()
        .unwrap_err();
    assert_unique_violation(managed_diesel_source(&duplicate));
    assert_response(
        duplicate,
        StatusCode::CONFLICT,
        CONFLICT_BODY,
        &[TABLE_NAME, UNIQUE_CONSTRAINT, UNIQUE_COLUMN, UNIQUE_VALUE],
    )
    .await;

    let missing_required_value = database
        .run(|connection| sql_query(INSERT_NULL_REQUIRED_VALUE).execute(connection))
        .await
        .into_http()
        .unwrap_err();
    assert_not_null_violation(managed_diesel_source(&missing_required_value));
    assert_response(
        missing_required_value,
        StatusCode::INTERNAL_SERVER_ERROR,
        INTERNAL_BODY,
        &[TABLE_NAME, REQUIRED_COLUMN],
    )
    .await;

    database.close();
}

#[tokio::test]
#[ignore = "requires PostgreSQL through MADS_TEST_DATABASE_URL"]
async fn native_query_results_map_real_postgres_errors_without_leaking_details() {
    let _guard = TEST_LOCK.lock().await;
    let mut table = TestTable::create();

    let missing = missing_row(table.connection(), -202)
        .into_http()
        .unwrap_err();
    assert_not_found(native_diesel_source(&missing));
    assert_response(missing, StatusCode::NOT_FOUND, NOT_FOUND_BODY, &[]).await;

    let duplicate = sql_query(INSERT_DUPLICATE_ROW)
        .execute(table.connection())
        .into_http()
        .unwrap_err();
    assert_unique_violation(native_diesel_source(&duplicate));
    assert_response(
        duplicate,
        StatusCode::CONFLICT,
        CONFLICT_BODY,
        &[TABLE_NAME, UNIQUE_CONSTRAINT, UNIQUE_COLUMN, UNIQUE_VALUE],
    )
    .await;

    let missing_required_value = sql_query(INSERT_NULL_REQUIRED_VALUE)
        .execute(table.connection())
        .into_http()
        .unwrap_err();
    assert_not_null_violation(native_diesel_source(&missing_required_value));
    assert_response(
        missing_required_value,
        StatusCode::INTERNAL_SERVER_ERROR,
        INTERNAL_BODY,
        &[TABLE_NAME, REQUIRED_COLUMN],
    )
    .await;
}
