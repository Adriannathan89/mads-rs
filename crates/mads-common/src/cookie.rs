//! Strict, redacted cookie extraction and checked response composition.
//!
//! # Security boundary
//!
//! JWT signatures can protect token integrity, but transporting a token in a
//! cookie does **not** provide CSRF protection. Applications that accept
//! state-changing cross-site requests must install appropriate CSRF middleware
//! and deliberately configure [`Cookie::set_http_only`], [`Cookie::set_secure`],
//! and [`Cookie::set_same_site`]. Signed and private cookie jars are not provided
//! in MADS.rs v0.5.5.
//!
//! Request cookies are available through the strict [`CookieJar`] extractor or
//! by parsing an existing header map:
//!
//! ```
//! use mads_common::{CookieJar, axum::http::{HeaderMap, HeaderValue, header::COOKIE}};
//!
//! let mut headers = HeaderMap::new();
//! headers.insert(COOKIE, HeaderValue::from_static("session=opaque"));
//! let jar = CookieJar::from_headers(&headers)?;
//! assert_eq!(jar.get("session").map(|cookie| cookie.value()), Some("opaque"));
//! # Ok::<(), mads_common::CookieError>(())
//! ```
//!
//! Return a jar in an Axum response tuple to emit pending cookies:
//!
//! ```
//! use mads_common::{Cookie, CookieJar};
//!
//! async fn login(jar: CookieJar) -> (CookieJar, &'static str) {
//!     let cookie = Cookie::build(("session", "opaque"))
//!         .path("/")
//!         .http_only(true)
//!         .secure(true)
//!         .same_site(mads_common::SameSite::Strict)
//!         .max_age(mads_common::cookie::time::Duration::days(7))
//!         .build();
//!     (jar.add(cookie), "signed in")
//! }
//! ```
//!
//! Removing a cookie emits an expired deletion cookie:
//!
//! ```
//! use mads_common::{Cookie, CookieJar};
//!
//! let jar = CookieJar::new().remove(Cookie::new("session", ""));
//! # let _ = jar;
//! ```
//!
//! The extractor and response parts compose directly with native Axum routes:
//!
//! ```
//! use mads_common::{Cookie, CookieJar, axum::{Router, routing::get}};
//!
//! async fn rotate(jar: CookieJar) -> (CookieJar, &'static str) {
//!     (jar.add(Cookie::new("session", "rotated")), "ok")
//! }
//!
//! let app: Router = Router::new().route("/session", get(rotate));
//! # let _ = app;
//! ```

use std::{collections::BTreeMap, fmt};

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, header::COOKIE, request::Parts},
    response::{IntoResponse, IntoResponseParts, Response, ResponseParts},
};
use axum_extra::extract::cookie::CookieJar as AxumCookieJar;

use crate::{BadRequest, InternalError};

pub use ::cookie::{Cookie, Expiration, SameSite, time};

/// Cookie parsing or response-cookie validation failed.
pub const MADS110: mads_core::DiagnosticCode = mads_core::DiagnosticCode::new("MADS110");

/// The result type used by checked cookie operations.
pub type CookieResult<T> = std::result::Result<T, CookieError>;

/// A stable category for a cookie failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CookieErrorKind {
    /// A request contained a malformed cookie header or pair.
    MalformedRequest,
    /// A response cookie could not be composed safely.
    InvalidResponse,
}

/// A normalized cookie error whose formatting never exposes cookie data.
#[non_exhaustive]
pub struct CookieError {
    kind: CookieErrorKind,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl CookieError {
    /// Creates a cookie error with the supplied stable category.
    pub const fn new(kind: CookieErrorKind) -> Self {
        Self { kind, source: None }
    }

    /// Returns the stable category of this error.
    pub const fn kind(&self) -> CookieErrorKind {
        self.kind
    }

    fn with_source<E>(kind: CookieErrorKind, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind,
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Debug for CookieError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CookieError")
            .field("kind", &self.kind)
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl fmt::Display for CookieError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            CookieErrorKind::MalformedRequest => "cookie request is malformed",
            CookieErrorKind::InvalidResponse => "response cookie is invalid",
        })
    }
}

impl std::error::Error for CookieError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

/// An Axum rejection for checked cookie extraction or composition.
pub struct CookieRejection(CookieError);

impl CookieRejection {
    /// Returns the stable category of the rejected cookie operation.
    pub const fn kind(&self) -> CookieErrorKind {
        self.0.kind()
    }
}

impl fmt::Debug for CookieRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CookieRejection")
            .field("kind", &self.kind())
            .finish()
    }
}

impl fmt::Display for CookieRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for CookieRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<CookieError> for CookieRejection {
    fn from(error: CookieError) -> Self {
        Self(error)
    }
}

impl IntoResponse for CookieRejection {
    fn into_response(self) -> Response {
        let Self(error) = self;
        match error.kind() {
            CookieErrorKind::MalformedRequest => {
                BadRequest::new("cookie request is malformed").into_response()
            }
            CookieErrorKind::InvalidResponse => InternalError::new(error).into_response(),
        }
    }
}

enum PendingCookie {
    Add(Cookie<'static>),
    Remove(Cookie<'static>),
}

impl PendingCookie {
    fn cookie(&self) -> &Cookie<'static> {
        match self {
            Self::Add(cookie) | Self::Remove(cookie) => cookie,
        }
    }
}

/// A strict Axum-compatible cookie jar with redacted diagnostics.
pub struct CookieJar {
    inner: AxumCookieJar,
    occurrences: BTreeMap<String, usize>,
    parsed_count: usize,
    pending: Vec<PendingCookie>,
}

impl CookieJar {
    /// Creates an empty cookie jar.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: AxumCookieJar::new(),
            occurrences: BTreeMap::new(),
            parsed_count: 0,
            pending: Vec::new(),
        }
    }

    /// Strictly parses every cookie pair from the supplied request headers.
    pub fn from_headers(headers: &HeaderMap) -> CookieResult<Self> {
        let mut occurrences = BTreeMap::new();
        let mut parsed_count = 0usize;

        for header in headers.get_all(COOKIE) {
            let header = header.to_str().map_err(|source| {
                CookieError::with_source(CookieErrorKind::MalformedRequest, source)
            })?;

            for segment in header.split(';') {
                let segment = segment.trim();
                if segment.is_empty() || !has_valid_percent_escapes(segment) {
                    return Err(CookieError::new(CookieErrorKind::MalformedRequest));
                }

                let parsed = Cookie::parse_encoded(segment.to_owned()).map_err(|source| {
                    CookieError::with_source(CookieErrorKind::MalformedRequest, source)
                })?;
                if !is_valid_cookie_name(parsed.name()) {
                    return Err(CookieError::new(CookieErrorKind::MalformedRequest));
                }

                *occurrences.entry(parsed.name().to_owned()).or_insert(0) += 1;
                parsed_count += 1;
            }
        }

        Ok(Self {
            inner: AxumCookieJar::from_headers(headers),
            occurrences,
            parsed_count,
            pending: Vec::new(),
        })
    }

    /// Returns the cookie currently stored under `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Cookie<'static>> {
        self.inner.get(name)
    }

    /// Iterates over the distinct request cookies in the jar.
    pub fn iter(&self) -> impl Iterator<Item = &'_ Cookie<'static>> {
        self.inner.iter()
    }

    /// Returns how many times `name` occurred across all request headers.
    #[must_use]
    pub fn occurrences(&self, name: &str) -> usize {
        self.occurrences.get(name).copied().unwrap_or_default()
    }

    /// Adds a response cookie while retaining it for checked batch emission.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn add<C: Into<Cookie<'static>>>(mut self, cookie: C) -> Self {
        let cookie = cookie.into();
        self.pending.push(PendingCookie::Add(cookie.clone()));
        self.inner = self.inner.add(cookie);
        self
    }

    /// Removes a cookie by emitting the deletion cookie produced by axum-extra.
    #[must_use]
    pub fn remove<C: Into<Cookie<'static>>>(mut self, cookie: C) -> Self {
        let cookie = cookie.into();
        self.pending.push(PendingCookie::Remove(cookie.clone()));
        self.inner = self.inner.remove(cookie);
        self
    }

    fn validate_pending(&self) -> CookieResult<()> {
        for pending in &self.pending {
            let cookie = pending.cookie();
            if cookie.same_site() == Some(SameSite::None) && cookie.secure() == Some(false) {
                return Err(CookieError::new(CookieErrorKind::InvalidResponse));
            }

            HeaderValue::from_str(&cookie.encoded().to_string()).map_err(|source| {
                CookieError::with_source(CookieErrorKind::InvalidResponse, source)
            })?;
        }
        Ok(())
    }
}

impl Default for CookieJar {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CookieJar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CookieJar")
            .field("parsed_count", &self.parsed_count)
            .field("distinct_names", &self.occurrences.len())
            .field("pending_operations", &self.pending.len())
            .finish()
    }
}

impl<S> FromRequestParts<S> for CookieJar
where
    S: Send + Sync,
{
    type Rejection = CookieRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Self::from_headers(&parts.headers).map_err(Into::into)
    }
}

impl IntoResponseParts for CookieJar {
    type Error = CookieRejection;

    fn into_response_parts(self, response: ResponseParts) -> Result<ResponseParts, Self::Error> {
        self.validate_pending()?;

        match self.inner.into_response_parts(response) {
            Ok(response) => Ok(response),
            Err(error) => match error {},
        }
    }
}

impl IntoResponse for CookieJar {
    fn into_response(self) -> Response {
        (self, ()).into_response()
    }
}

fn has_valid_percent_escapes(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn is_valid_cookie_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}
