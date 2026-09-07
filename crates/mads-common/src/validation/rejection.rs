//! Safe classification of native extractor rejections.

use std::error::Error;

use axum::{
    Json,
    extract::{
        path::ErrorKind as PathErrorKind,
        rejection::{
            FailedToDeserializePathParams, FailedToDeserializeQueryString, JsonDataError,
            JsonRejection, JsonSyntaxError, PathRejection, QueryRejection,
        },
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_path_to_error::{Error as PathError, Segment};

use super::{ValidationIssue, ValidationSource, support};
use crate::{BadRequest, InternalError, ValidationError};

const BODY_READ_MESSAGE: &str = "request body could not be read";
const CONTENT_TYPE_MESSAGE: &str = "content type must be application/json";
const PAYLOAD_TOO_LARGE_MESSAGE: &str = "request body is too large";

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

pub(super) fn json_rejection_response(rejection: JsonRejection) -> Response {
    match rejection {
        JsonRejection::JsonDataError(error) => {
            schema_error_response(data_issue(&error), ValidationSource::Body)
        }
        JsonRejection::JsonSyntaxError(error) => {
            schema_error_response(syntax_issue(&error), ValidationSource::Body)
        }
        rejection => {
            let status = rejection.status();
            transport_error_response(status, rejection)
        }
    }
}

pub(super) fn query_rejection_response(rejection: QueryRejection) -> Response {
    let issue = match rejection {
        QueryRejection::FailedToDeserializeQueryString(error) => query_issue(&error),
        _ => support::issue(support::IssueKind::InvalidType),
    };
    schema_error_response(issue, ValidationSource::Query)
}

pub(super) fn path_rejection_response(rejection: PathRejection) -> Response {
    match rejection {
        PathRejection::FailedToDeserializePathParams(error) => {
            schema_error_response(path_issue(&error), ValidationSource::Path)
        }
        rejection => {
            let status = rejection.status();
            transport_error_response(status, rejection)
        }
    }
}

fn schema_error_response(issue: ValidationIssue, source: ValidationSource) -> Response {
    ValidationError::new([issue.with_source(source)]).into_response()
}

fn data_issue(error: &JsonDataError) -> ValidationIssue {
    let path_error = find_source::<PathError<serde_json::Error>>(error);
    let missing_field = path_error.and_then(|error| parse_missing_field(error.inner()));
    let kind = if missing_field.is_some() {
        support::IssueKind::Required
    } else {
        support::IssueKind::InvalidType
    };
    let mut issue = support::issue(kind);

    if let Some(error) = path_error {
        issue = apply_path(issue, error.path());
    }
    if let Some(field) = missing_field {
        issue = issue.at_field(field);
    }

    issue
}

fn syntax_issue(error: &JsonSyntaxError) -> ValidationIssue {
    let issue = support::issue(support::IssueKind::InvalidSyntax);
    if let Some(error) = find_source::<PathError<serde_json::Error>>(error) {
        apply_path(issue, error.path())
    } else {
        issue
    }
}

fn query_issue(error: &FailedToDeserializeQueryString) -> ValidationIssue {
    let Some(error) = find_source::<PathError<serde_urlencoded::de::Error>>(error) else {
        return support::issue(support::IssueKind::InvalidType);
    };
    let missing_field = parse_plain_missing_field(&error.inner().to_string());
    let kind = if missing_field.is_some() {
        support::IssueKind::Required
    } else {
        support::IssueKind::InvalidType
    };
    let issue = apply_path(support::issue(kind), error.path());
    apply_fallback_field(
        issue,
        missing_field.or_else(|| parse_unknown_field(&error.inner().to_string())),
    )
}

fn path_issue(error: &FailedToDeserializePathParams) -> ValidationIssue {
    match error.kind() {
        PathErrorKind::InvalidUtf8InPathParam { key } => {
            support::issue(support::IssueKind::InvalidSyntax).at_field(key)
        }
        PathErrorKind::ParseErrorAtKey { key, .. }
        | PathErrorKind::DeserializeError { key, .. } => {
            support::issue(support::IssueKind::InvalidType).at_field(key)
        }
        PathErrorKind::ParseErrorAtIndex { index, .. } => {
            support::issue(support::IssueKind::InvalidType).at_index(*index)
        }
        PathErrorKind::Message(message) => {
            let missing_field = parse_plain_missing_field(message);
            let kind = if missing_field.is_some() {
                support::IssueKind::Required
            } else {
                support::IssueKind::InvalidType
            };
            apply_fallback_field(
                support::issue(kind),
                missing_field.or_else(|| parse_unknown_field(message)),
            )
        }
        _ => support::issue(support::IssueKind::InvalidType),
    }
}

fn apply_path(mut issue: ValidationIssue, path: &serde_path_to_error::Path) -> ValidationIssue {
    for segment in path {
        issue = match segment {
            Segment::Seq { index } => issue.at_index(*index),
            Segment::Map { key } | Segment::Enum { variant: key } => issue.at_field(key.clone()),
            Segment::Unknown => issue,
        };
    }
    issue
}

fn parse_missing_field(error: &serde_json::Error) -> Option<String> {
    let message = error.to_string();
    let remainder = message.strip_prefix("missing field `")?;
    let (field, location) = remainder.split_once("` at line ")?;
    let (line, column) = location.split_once(" column ")?;
    let is_exact_form = !field.is_empty()
        && !line.is_empty()
        && !column.is_empty()
        && line.bytes().all(|byte| byte.is_ascii_digit())
        && column.bytes().all(|byte| byte.is_ascii_digit());
    is_exact_form.then(|| field.to_owned())
}

fn parse_plain_missing_field(message: &str) -> Option<String> {
    let field = message.strip_prefix("missing field `")?.strip_suffix('`')?;
    (!field.is_empty()).then(|| field.to_owned())
}

fn parse_unknown_field(message: &str) -> Option<String> {
    let remainder = message.strip_prefix("unknown field `")?;
    let (field, expected) = remainder.split_once("`, expected ")?;
    (!field.is_empty() && !expected.is_empty()).then(|| field.to_owned())
}

fn apply_fallback_field(issue: ValidationIssue, field: Option<String>) -> ValidationIssue {
    if issue.path().is_empty() {
        if let Some(field) = field {
            return issue.at_field(field);
        }
    }
    issue
}

fn find_source<'a, T>(mut error: &'a (dyn Error + 'static)) -> Option<&'a T>
where
    T: Error + 'static,
{
    loop {
        if let Some(error) = error.downcast_ref::<T>() {
            return Some(error);
        }
        error = error.source()?;
    }
}

fn transport_error_response(
    status: StatusCode,
    source: impl Error + Send + Sync + 'static,
) -> Response {
    match status {
        StatusCode::UNSUPPORTED_MEDIA_TYPE => {
            fixed_error_response(status, "unsupported_media_type", CONTENT_TYPE_MESSAGE)
        }
        StatusCode::PAYLOAD_TOO_LARGE => {
            fixed_error_response(status, "payload_too_large", PAYLOAD_TOO_LARGE_MESSAGE)
        }
        status if status.is_client_error() => BadRequest::new(BODY_READ_MESSAGE).into_response(),
        _ => InternalError::new(source).into_response(),
    }
}

fn fixed_error_response(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            error: ErrorBody { code, message },
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::io;

    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};

    use super::transport_error_response;

    #[tokio::test]
    async fn json_server_class_body_read_failure_is_a_redacted_internal_error() {
        let response = transport_error_response(
            StatusCode::BAD_GATEWAY,
            io::Error::other("private upstream body-read detail"),
        )
        .into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body must be readable");
        assert_eq!(
            body,
            r#"{"error":{"code":"internal","message":"internal server error"}}"#
        );
        assert!(
            !body
                .windows("private upstream body-read detail".len())
                .any(|window| window == b"private upstream body-read detail")
        );
    }
}
