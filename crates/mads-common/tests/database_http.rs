//! Pure HTTP classification contracts for managed and native Diesel results.

#![cfg(all(feature = "http", feature = "database"))]

use std::error::Error as StdError;

use mads_common::{
    DatabaseError, DatabaseResult, HttpError, IntoHttpResult,
    axum::{
        body::to_bytes,
        http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
        response::IntoResponse,
    },
    diesel::result::{
        DatabaseErrorInformation, DatabaseErrorKind as DieselDatabaseErrorKind,
        Error as DieselError, QueryResult,
    },
};

const NOT_FOUND_BODY: &str = r#"{"error":{"code":"not_found","message":"resource not found"}}"#;
const CONFLICT_BODY: &str = r#"{"error":{"code":"conflict","message":"resource already exists"}}"#;
const INTERNAL_BODY: &str = r#"{"error":{"code":"internal","message":"internal server error"}}"#;

const MESSAGE_SENTINEL: &str = "diesel-http-message-sentinel";
const DETAILS_SENTINEL: &str = "diesel-http-details-sentinel";
const HINT_SENTINEL: &str = "diesel-http-hint-sentinel";
const TABLE_SENTINEL: &str = "diesel-http-table-sentinel";
const COLUMN_SENTINEL: &str = "diesel-http-column-sentinel";
const CONSTRAINT_SENTINEL: &str = "diesel-http-constraint-sentinel";
const SENTINELS: [&str; 6] = [
    MESSAGE_SENTINEL,
    DETAILS_SENTINEL,
    HINT_SENTINEL,
    TABLE_SENTINEL,
    COLUMN_SENTINEL,
    CONSTRAINT_SENTINEL,
];

#[derive(Debug)]
struct SentinelDatabaseErrorInformation;

impl DatabaseErrorInformation for SentinelDatabaseErrorInformation {
    fn message(&self) -> &str {
        MESSAGE_SENTINEL
    }

    fn details(&self) -> Option<&str> {
        Some(DETAILS_SENTINEL)
    }

    fn hint(&self) -> Option<&str> {
        Some(HINT_SENTINEL)
    }

    fn table_name(&self) -> Option<&str> {
        Some(TABLE_SENTINEL)
    }

    fn column_name(&self) -> Option<&str> {
        Some(COLUMN_SENTINEL)
    }

    fn constraint_name(&self) -> Option<&str> {
        Some(CONSTRAINT_SENTINEL)
    }

    fn statement_position(&self) -> Option<i32> {
        Some(73)
    }
}

fn database_error(kind: DieselDatabaseErrorKind) -> DieselError {
    DieselError::DatabaseError(kind, Box::new(SentinelDatabaseErrorInformation))
}

async fn assert_response(
    error: impl IntoResponse,
    expected_status: StatusCode,
    expected_body: &str,
) -> String {
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

    String::from_utf8(body.to_vec()).expect("mapped response body must be UTF-8")
}

fn assert_redacted(output: &str) {
    for sentinel in SENTINELS {
        assert!(
            !output.contains(sentinel),
            "mapped output must not reveal {sentinel}"
        );
    }
}

fn native_source(error: &HttpError) -> &DieselError {
    StdError::source(error)
        .expect("mapped HTTP error must retain its native Diesel source")
        .downcast_ref::<DieselError>()
        .expect("mapped HTTP error must retain the original native Diesel error")
}

fn managed_source(error: &HttpError) -> &DatabaseError {
    StdError::source(error)
        .expect("mapped HTTP error must retain its managed database source")
        .downcast_ref::<DatabaseError>()
        .expect("mapped HTTP error must retain the original managed database error")
}

fn managed_query_source(error: &DatabaseError) -> &DieselError {
    StdError::source(error)
        .expect("managed query error must retain the Diesel source")
        .downcast_ref::<DieselError>()
        .expect("managed query error must retain the original Diesel error")
}

fn assert_sentinel_information(information: &(dyn DatabaseErrorInformation + Send + Sync)) {
    assert_eq!(information.message(), MESSAGE_SENTINEL);
    assert_eq!(information.details(), Some(DETAILS_SENTINEL));
    assert_eq!(information.hint(), Some(HINT_SENTINEL));
    assert_eq!(information.table_name(), Some(TABLE_SENTINEL));
    assert_eq!(information.column_name(), Some(COLUMN_SENTINEL));
    assert_eq!(information.constraint_name(), Some(CONSTRAINT_SENTINEL));
    assert_eq!(information.statement_position(), Some(73));
}

fn assert_unique_violation(error: &DieselError) {
    match error {
        DieselError::DatabaseError(DieselDatabaseErrorKind::UniqueViolation, information) => {
            assert_sentinel_information(information.as_ref());
        }
        other => panic!("expected a retained unique violation, got {other:?}"),
    }
}

fn assert_foreign_key_violation(error: &DieselError) {
    match error {
        DieselError::DatabaseError(DieselDatabaseErrorKind::ForeignKeyViolation, information) => {
            assert_sentinel_information(information.as_ref());
        }
        other => panic!("expected a retained foreign-key violation, got {other:?}"),
    }
}

#[test]
fn native_query_success_keeps_the_value() {
    let result: QueryResult<u8> = Ok(7);

    assert_eq!(result.into_http().unwrap(), 7);
}

#[test]
fn managed_database_result_success_keeps_the_value() {
    let result: DatabaseResult<u8> = Ok(11);

    assert_eq!(result.into_http().unwrap(), 11);
}

#[tokio::test]
async fn native_not_found_maps_to_the_fixed_404_and_retains_its_source() {
    let missing: QueryResult<()> = Err(DieselError::NotFound);
    let error = missing.into_http().unwrap_err();

    assert_eq!(error.to_string(), "resource not found");
    assert!(matches!(native_source(&error), DieselError::NotFound));
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::NOT_FOUND, NOT_FOUND_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}

#[tokio::test]
async fn native_unique_violation_maps_to_the_fixed_409_and_retains_full_information() {
    let violation: QueryResult<()> = Err(database_error(DieselDatabaseErrorKind::UniqueViolation));
    let error = violation.into_http().unwrap_err();

    assert_eq!(error.to_string(), "resource already exists");
    assert_unique_violation(native_source(&error));
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::CONFLICT, CONFLICT_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}

#[tokio::test]
async fn native_non_unique_database_errors_map_to_the_fixed_redacted_500() {
    let violation: QueryResult<()> =
        Err(database_error(DieselDatabaseErrorKind::ForeignKeyViolation));
    let error = violation.into_http().unwrap_err();

    assert_eq!(error.to_string(), "internal server error");
    assert_foreign_key_violation(native_source(&error));
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}

#[tokio::test]
async fn managed_query_not_found_maps_to_the_fixed_404_and_retains_the_wrapper_chain() {
    let missing: DatabaseResult<()> = Err(DatabaseError::Query(DieselError::NotFound));
    let error = missing.into_http().unwrap_err();

    assert_eq!(error.to_string(), "resource not found");
    let source = managed_source(&error);
    assert_eq!(source.to_string(), "database query failed");
    assert!(matches!(
        managed_query_source(source),
        DieselError::NotFound
    ));
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::NOT_FOUND, NOT_FOUND_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}

#[tokio::test]
async fn managed_unique_violation_maps_to_the_fixed_409_and_retains_the_wrapper_chain() {
    let violation: DatabaseResult<()> = Err(DatabaseError::Query(database_error(
        DieselDatabaseErrorKind::UniqueViolation,
    )));
    let error = violation.into_http().unwrap_err();

    assert_eq!(error.to_string(), "resource already exists");
    let source = managed_source(&error);
    assert_eq!(source.to_string(), "database query failed");
    assert_unique_violation(managed_query_source(source));
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::CONFLICT, CONFLICT_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}

#[tokio::test]
async fn managed_non_unique_query_errors_map_to_the_fixed_redacted_500() {
    let violation: DatabaseResult<()> = Err(DatabaseError::Query(database_error(
        DieselDatabaseErrorKind::ForeignKeyViolation,
    )));
    let error = violation.into_http().unwrap_err();

    assert_eq!(error.to_string(), "internal server error");
    let source = managed_source(&error);
    assert_eq!(source.to_string(), "database query failed");
    assert_foreign_key_violation(managed_query_source(source));
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}

#[tokio::test]
async fn managed_non_query_errors_map_to_the_fixed_redacted_500() {
    let failure: DatabaseResult<()> = Err(DatabaseError::Configuration {
        message: MESSAGE_SENTINEL.to_owned(),
        source: None,
    });
    let error = failure.into_http().unwrap_err();

    assert_eq!(error.to_string(), "internal server error");
    let source = managed_source(&error);
    assert_eq!(
        source.to_string(),
        format!("database configuration is invalid: {MESSAGE_SENTINEL}")
    );
    let formatting = format!("{error}\n{error:?}");

    let body = assert_response(error, StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_BODY).await;
    assert_redacted(&formatting);
    assert_redacted(&body);
}
