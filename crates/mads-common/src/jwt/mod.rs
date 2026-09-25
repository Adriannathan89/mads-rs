//! Typed JSON Web Token contracts.
//!
//! This module owns the framework's closed algorithm set, access and refresh
//! token profiles, typed claim wrappers, explicit operation options, and
//! redacted errors, and the application-scoped cryptographic service.
//!
//! Access and refresh profiles are deliberately distinct at both signing and
//! verification:
//!
//! ```
//! use std::time::Duration;
//! use mads_common::{
//!     JwtService, JwtSignOptions, JwtValidation,
//!     core::{ConfigBuilder, MapSource},
//! };
//!
//! #[derive(serde::Deserialize, serde::Serialize)]
//! struct UserClaims { user_id: u64 }
//!
//! let config = ConfigBuilder::new()
//!     .source(MapSource::new("example", [
//!         ("passport.secret", "01234567890123456789012345678901"),
//!     ]))
//!     .build()?;
//! let jwt = JwtService::from_config(&config)?;
//! let access = jwt.sign(
//!     UserClaims { user_id: 7 },
//!     JwtSignOptions::access(Duration::from_secs(900)).subject("7"),
//! )?;
//! let refresh = jwt.sign(
//!     UserClaims { user_id: 7 },
//!     JwtSignOptions::refresh(Duration::from_secs(604_800)).subject("7"),
//! )?;
//! jwt.verify::<UserClaims>(&access, JwtValidation::access().subject("7"))?;
//! jwt.verify::<UserClaims>(&refresh, JwtValidation::refresh().subject("7"))?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Algorithms are selected only from the configured allowlist and each named
//! key is bound to one algorithm. Header decoding and
//! [`JwtService::decode_unverified`] return untrusted data; neither authenticates
//! a request.

mod auto_configuration;
mod claims;
mod config;
mod crypto;
mod error;
mod keyring;
mod service;

pub use claims::{
    JwtAlgorithm, JwtClaims, JwtHeader, JwtSignOptions, JwtTokenKind, JwtValidation,
    RegisteredJwtClaims, VerifiedJwt,
};
pub use config::PassportConfig;
pub use error::{JwtError, JwtErrorKind, JwtResult};
pub use service::JwtService;

/// JWT signing, verification, or key-operation failure.
pub const MADS120: mads_core::DiagnosticCode = mads_core::DiagnosticCode::new("MADS120");

/// JWT auto-configuration failure.
pub const MADS121: mads_core::DiagnosticCode = mads_core::DiagnosticCode::new("MADS121");
