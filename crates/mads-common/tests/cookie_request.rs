//! Strict request-cookie extraction.

#![cfg(feature = "cookies")]

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header::COOKIE},
    routing::get,
};
use mads_common::{CookieErrorKind, CookieJar, CookieRejection};
use tower::ServiceExt;

#[test]
fn parses_multiple_headers_and_tracks_duplicate_names() {
    let mut headers = HeaderMap::new();
    headers.append(COOKIE, HeaderValue::from_static("theme=dark; session=one"));
    headers.append(COOKIE, HeaderValue::from_static("session=two; language=en"));

    let jar = CookieJar::from_headers(&headers).unwrap();

    assert_eq!(jar.get("theme").unwrap().value(), "dark");
    assert_eq!(jar.get("session").unwrap().value(), "two");
    assert_eq!(jar.occurrences("session"), 2);
    assert_eq!(jar.occurrences("missing"), 0);
    assert_eq!(jar.iter().count(), 3);
}

#[test]
fn parses_multiple_pairs_and_percent_encoded_values() {
    let mut headers = HeaderMap::new();
    headers.append(
        COOKIE,
        HeaderValue::from_static("first=one; encoded=hello%20world; empty="),
    );

    let jar = CookieJar::from_headers(&headers).unwrap();

    assert_eq!(jar.get("first").unwrap().value(), "one");
    assert_eq!(jar.get("encoded").unwrap().value(), "hello world");
    assert_eq!(jar.get("empty").unwrap().value(), "");
    assert_eq!(jar.iter().count(), 3);
}

#[test]
fn no_cookie_headers_produces_an_empty_jar() {
    let jar = CookieJar::from_headers(&HeaderMap::new()).unwrap();

    assert_eq!(jar.iter().count(), 0);
    assert_eq!(jar.occurrences("anything"), 0);
}

#[test]
fn empty_headers_and_segments_are_rejected() {
    for raw in ["", "first=one;", ";first=one", "first=one;;second=two"] {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_str(raw).unwrap());

        let error = CookieJar::from_headers(&headers).unwrap_err();

        assert_eq!(error.kind(), CookieErrorKind::MalformedRequest);
    }
}

#[test]
fn non_utf8_header_bytes_are_rejected() {
    let mut headers = HeaderMap::new();
    headers.append(COOKIE, HeaderValue::from_bytes(b"session=\xff").unwrap());

    let error = CookieJar::from_headers(&headers).unwrap_err();

    assert_eq!(error.kind(), CookieErrorKind::MalformedRequest);
}

#[test]
fn invalid_cookie_names_are_rejected() {
    for raw in ["bad name=value", "bad,name=value", "%20=value"] {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_str(raw).unwrap());

        let error = CookieJar::from_headers(&headers).unwrap_err();

        assert_eq!(error.kind(), CookieErrorKind::MalformedRequest);
    }
}

#[test]
fn invalid_percent_escapes_are_rejected() {
    for raw in ["session=%", "session=%0", "session=%GG", "bad%=value"] {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_str(raw).unwrap());

        let error = CookieJar::from_headers(&headers).unwrap_err();

        assert_eq!(error.kind(), CookieErrorKind::MalformedRequest);
    }
}

#[tokio::test]
async fn malformed_cookie_is_bad_request_instead_of_being_skipped() {
    async fn handler(_: CookieJar) {}

    const SENTINEL: &str = "request-cookie-value-sentinel";
    let app = Router::new().route("/", get(handler));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(COOKIE, format!("valid={SENTINEL}; malformed"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response
            .headers()
            .values()
            .all(|value| { !String::from_utf8_lossy(value.as_bytes()).contains(SENTINEL) })
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert_eq!(
        body,
        "{\"error\":{\"code\":\"bad_request\",\"message\":\"cookie request is malformed\"}}"
    );
    assert!(!body.contains(SENTINEL));
}

#[test]
fn debug_and_errors_disclose_only_safe_structure() {
    let mut valid_headers = HeaderMap::new();
    valid_headers.append(
        COOKIE,
        HeaderValue::from_static("sentinel_name=sentinel_value; second=other"),
    );
    let jar = CookieJar::from_headers(&valid_headers).unwrap();

    let debug = format!("{jar:?}");
    assert!(debug.contains("parsed_count: 2"));
    assert!(debug.contains("distinct_names: 2"));
    assert!(debug.contains("pending_operations: 0"));
    assert!(!debug.contains("sentinel_name"));
    assert!(!debug.contains("sentinel_value"));
    assert!(!debug.contains("second"));
    assert!(!debug.contains("other"));

    let mut invalid_headers = HeaderMap::new();
    invalid_headers.append(
        COOKIE,
        HeaderValue::from_bytes(b"private_name=private_value; source-sentinel=\xff").unwrap(),
    );
    let error = CookieJar::from_headers(&invalid_headers).unwrap_err();
    let display = error.to_string();
    let debug = format!("{error:?}");
    assert!(std::error::Error::source(&error).is_some());
    for sentinel in ["private_name", "private_value", "source-sentinel"] {
        assert!(!display.contains(sentinel));
        assert!(!debug.contains(sentinel));
    }

    let rejection = CookieRejection::from(error);
    let display = rejection.to_string();
    let debug = format!("{rejection:?}");
    for sentinel in ["private_name", "private_value", "source-sentinel"] {
        assert!(!display.contains(sentinel));
        assert!(!debug.contains(sentinel));
    }
    let cookie_error = std::error::Error::source(&rejection).unwrap();
    assert!(std::error::Error::source(cookie_error).is_some());
}
