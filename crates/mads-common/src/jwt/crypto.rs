//! JWT operations pinned to MADS's AWS-LC provider, independent of the
//! process-wide default selected by other crates.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER;
use jsonwebtoken::errors::Result;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header};
use serde::Serialize;

pub(super) fn sign(message: &[u8], key: &EncodingKey, algorithm: Algorithm) -> Result<String> {
    let signer = (DEFAULT_PROVIDER.signer_factory)(&algorithm, key)?;
    Ok(URL_SAFE_NO_PAD.encode(signer.try_sign(message)?))
}

pub(super) fn verify(
    signature: &str,
    message: &[u8],
    key: &DecodingKey,
    algorithm: Algorithm,
) -> Result<bool> {
    let verifier = (DEFAULT_PROVIDER.verifier_factory)(&algorithm, key)?;
    let decoded = URL_SAFE_NO_PAD.decode(signature)?;
    Ok(verifier.verify(message, &decoded).is_ok())
}

pub(super) fn encode<T: Serialize>(
    header: &Header,
    claims: &T,
    key: &EncodingKey,
) -> Result<String> {
    let header_part = URL_SAFE_NO_PAD.encode(serde_json::to_vec(header)?);
    let claims_part = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
    let message = format!("{header_part}.{claims_part}");
    let signature = sign(message.as_bytes(), key, header.alg)?;
    Ok(format!("{message}.{signature}"))
}
