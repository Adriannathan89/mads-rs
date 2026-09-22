//! JWT signing, strict verification, and explicitly untrusted decoding.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::de::{DeserializeOwned, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use super::keyring::KeyRing;
use super::{
    JwtAlgorithm, JwtClaims, JwtError, JwtErrorKind, JwtHeader, JwtResult, JwtSignOptions,
    JwtTokenKind, JwtValidation, PassportConfig, RegisteredJwtClaims, VerifiedJwt,
};

const RESERVED_CLAIMS: [&str; 8] = ["iss", "sub", "aud", "exp", "nbf", "iat", "jti", "token_use"];

/// An application-scoped service for MADS access and refresh JWTs.
///
/// The service holds its signing and verification keys privately and never
/// exposes them through its public API or diagnostics.
#[derive(Clone)]
pub struct JwtService {
    inner: Arc<JwtServiceInner>,
}

struct JwtServiceInner {
    key_ring: KeyRing,
    policy: JwtValidationPolicy,
}

#[derive(Clone)]
struct JwtValidationPolicy {
    issuer: Option<String>,
    audiences: Vec<String>,
    clock_skew_seconds: u64,
    max_token_bytes: usize,
}

impl fmt::Debug for JwtService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JwtService")
            .field("has_issuer", &self.inner.policy.issuer.is_some())
            .field("audience_count", &self.inner.policy.audiences.len())
            .field("clock_skew_seconds", &self.inner.policy.clock_skew_seconds)
            .field("max_token_bytes", &self.inner.policy.max_token_bytes)
            .finish()
    }
}

impl JwtService {
    /// Constructs a service from the application's resolved MADS configuration.
    pub fn from_config(config: &mads_core::Config) -> JwtResult<Self> {
        Self::from_passport_config(PassportConfig::from_config(config)?)
    }

    /// Constructs a service from a validated Passport configuration.
    pub fn from_passport_config(config: PassportConfig) -> JwtResult<Self> {
        let policy = JwtValidationPolicy {
            issuer: config.issuer().map(str::to_owned),
            audiences: config.audiences().to_vec(),
            clock_skew_seconds: config.clock_skew_seconds(),
            max_token_bytes: config.max_token_bytes(),
        };
        let key_ring = KeyRing::from_config(&config)?;
        Ok(Self {
            inner: Arc::new(JwtServiceInner { key_ring, policy }),
        })
    }

    /// Signs application-owned object claims as a MADS access or refresh JWT.
    pub fn sign<C>(&self, custom_claims: C, options: JwtSignOptions) -> JwtResult<String>
    where
        C: serde::Serialize,
    {
        let lifetime = options.lifetime().as_secs();
        if lifetime == 0 {
            return Err(JwtError::new(JwtErrorKind::Serialization));
        }

        let issued_at = unix_now()?;
        let expires_at = issued_at
            .checked_add(lifetime)
            .ok_or_else(|| JwtError::new(JwtErrorKind::Serialization))?;
        let mut claims = object_custom_claims(custom_claims)?;

        insert_registered_claims(
            &mut claims,
            RegisteredJwtClaims {
                issuer: self.inner.policy.issuer.clone(),
                subject: options.subject_value().map(str::to_owned),
                audiences: self.inner.policy.audiences.clone(),
                expires_at,
                not_before: options.not_before_value(),
                issued_at,
                jwt_id: options.jwt_id_value().map(str::to_owned),
                token_kind: options.kind(),
            },
        );

        let (key, algorithm, key_id) = self.inner.key_ring.active_signer()?;
        let mut header = jsonwebtoken::Header::new(algorithm.as_jsonwebtoken());
        header.typ = Some(options.kind().header_type().to_owned());
        header.kid = key_id.map(str::to_owned);
        jsonwebtoken::encode(&header, &claims, key).map_err(map_signing_error)
    }

    /// Decodes a JWT header without verifying its signature.
    ///
    /// # Warning
    ///
    /// The returned header is attacker-controlled and **must not** be used for
    /// authorization or key selection outside this service. Use [`Self::verify`]
    /// before trusting any token metadata.
    pub fn decode_header(&self, token: &str) -> JwtResult<JwtHeader> {
        self.ensure_token_size(token)?;
        let (header, _, _) = jwt_parts(token)?;
        decode_jwt_header(header)
    }

    /// Decodes typed JWT claims without verifying a signature or registered claims.
    ///
    /// # Warning
    ///
    /// The returned claims are attacker-controlled and **must not** be used for
    /// authorization, identity, key selection, or application decisions. Use
    /// [`Self::verify`] before trusting any claim.
    pub fn decode_unverified<C>(&self, token: &str) -> JwtResult<JwtClaims<C>>
    where
        C: DeserializeOwned,
    {
        self.ensure_token_size(token)?;
        let (header, payload, _) = jwt_parts(token)?;
        // Require a syntactically valid supported JWT header while deliberately
        // not treating its value as trusted.
        let _ = decode_jwt_header(header)?;
        decode_claims(payload)
    }

    /// Verifies a JWT signature and all MADS and caller-supplied requirements.
    pub fn verify<C>(&self, token: &str, validation: JwtValidation) -> JwtResult<VerifiedJwt<C>>
    where
        C: DeserializeOwned,
    {
        self.ensure_token_size(token)?;
        let (encoded_header, encoded_payload, _) = jwt_parts(token)?;
        let header = decode_jwt_header(encoded_header)?;
        let (key, key_algorithm) = self.inner.key_ring.verifier(header.key_id.as_deref())?;
        if header.algorithm != key_algorithm {
            return Err(JwtError::new(JwtErrorKind::AlgorithmMismatch));
        }

        verify_signature(token, key, key_algorithm)?;
        let claims = decode_claims(encoded_payload)?;
        self.validate_claims(&header, &claims.registered, &validation, unix_now()?)?;
        Ok(VerifiedJwt { header, claims })
    }

    fn ensure_token_size(&self, token: &str) -> JwtResult<()> {
        if token.len() > self.inner.policy.max_token_bytes {
            return Err(JwtError::new(JwtErrorKind::TokenTooLarge));
        }
        Ok(())
    }

    fn validate_claims(
        &self,
        header: &JwtHeader,
        claims: &RegisteredJwtClaims,
        validation: &JwtValidation,
        now: u64,
    ) -> JwtResult<()> {
        if header.token_type.as_deref() != Some(validation.kind().header_type())
            || claims.token_kind != validation.kind()
        {
            return Err(JwtError::new(JwtErrorKind::TokenKindMismatch));
        }

        if claims.expires_at <= now.saturating_sub(self.inner.policy.clock_skew_seconds) {
            return Err(JwtError::new(JwtErrorKind::Expired));
        }
        if let Some(not_before) = claims.not_before
            && not_before > now.saturating_add(self.inner.policy.clock_skew_seconds)
        {
            return Err(JwtError::new(JwtErrorKind::InvalidNotBefore));
        }

        if let Some(expected) = self.inner.policy.issuer.as_deref()
            && claims.issuer.as_deref() != Some(expected)
        {
            return Err(JwtError::new(JwtErrorKind::IssuerMismatch));
        }
        if let Some(expected) = validation.issuer_value()
            && claims.issuer.as_deref() != Some(expected)
        {
            return Err(JwtError::new(JwtErrorKind::IssuerMismatch));
        }

        if !self.inner.policy.audiences.is_empty()
            && !claims.audiences.iter().any(|audience| {
                self.inner
                    .policy
                    .audiences
                    .iter()
                    .any(|expected| expected == audience)
            })
        {
            return Err(JwtError::new(JwtErrorKind::AudienceMismatch));
        }
        if let Some(expected) = validation.audience_value()
            && !claims.audiences.iter().any(|audience| audience == expected)
        {
            return Err(JwtError::new(JwtErrorKind::AudienceMismatch));
        }

        if validation.subject_required() && claims.subject.is_none() {
            return Err(JwtError::new(JwtErrorKind::SubjectMismatch));
        }
        if let Some(expected) = validation.subject_value()
            && claims.subject.as_deref() != Some(expected)
        {
            return Err(JwtError::new(JwtErrorKind::SubjectMismatch));
        }
        if validation.jwt_id_required() && claims.jwt_id.is_none() {
            return Err(JwtError::new(JwtErrorKind::JwtIdMismatch));
        }
        if let Some(expected) = validation.jwt_id_value()
            && claims.jwt_id.as_deref() != Some(expected)
        {
            return Err(JwtError::new(JwtErrorKind::JwtIdMismatch));
        }
        Ok(())
    }
}

fn unix_now() -> JwtResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| JwtError::new(JwtErrorKind::Serialization))
}

fn object_custom_claims<C>(custom_claims: C) -> JwtResult<Map<String, Value>>
where
    C: serde::Serialize,
{
    let value = serde_json::to_value(custom_claims)
        .map_err(|_| JwtError::new(JwtErrorKind::Serialization))?;
    let object = match value {
        Value::Object(object) => object,
        _ => return Err(JwtError::new(JwtErrorKind::Serialization)),
    };
    if object
        .keys()
        .any(|key| RESERVED_CLAIMS.contains(&key.as_str()))
    {
        return Err(JwtError::new(JwtErrorKind::Serialization));
    }
    Ok(object)
}

fn insert_registered_claims(claims: &mut Map<String, Value>, registered: RegisteredJwtClaims) {
    if let Some(issuer) = registered.issuer {
        claims.insert("iss".to_owned(), Value::String(issuer));
    }
    if let Some(subject) = registered.subject {
        claims.insert("sub".to_owned(), Value::String(subject));
    }
    if !registered.audiences.is_empty() {
        claims.insert(
            "aud".to_owned(),
            Value::Array(
                registered
                    .audiences
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    claims.insert(
        "exp".to_owned(),
        Value::Number(Number::from(registered.expires_at)),
    );
    if let Some(not_before) = registered.not_before {
        claims.insert("nbf".to_owned(), Value::Number(Number::from(not_before)));
    }
    claims.insert(
        "iat".to_owned(),
        Value::Number(Number::from(registered.issued_at)),
    );
    if let Some(jwt_id) = registered.jwt_id {
        claims.insert("jti".to_owned(), Value::String(jwt_id));
    }
    claims.insert(
        "token_use".to_owned(),
        Value::String(registered.token_kind.claim_value().to_owned()),
    );
}

fn jwt_parts(token: &str) -> JwtResult<(&str, &str, &str)> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(JwtError::new(JwtErrorKind::MalformedToken));
    };
    if header.is_empty() || payload.is_empty() || signature.is_empty() {
        return Err(JwtError::new(JwtErrorKind::MalformedToken));
    }
    Ok((header, payload, signature))
}

fn decode_jwt_header(encoded_header: &str) -> JwtResult<JwtHeader> {
    let mut object = decode_json_object(encoded_header)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "alg" | "kid" | "typ"))
    {
        return Err(JwtError::new(JwtErrorKind::MalformedToken));
    }
    let algorithm = match take_string(&mut object, "alg")? {
        Some(value) => value
            .parse()
            .map_err(|_| JwtError::new(JwtErrorKind::DisallowedAlgorithm))?,
        None => return Err(JwtError::new(JwtErrorKind::MalformedToken)),
    };
    let key_id = take_string(&mut object, "kid")?;
    let token_type = take_string(&mut object, "typ")?;
    Ok(JwtHeader {
        algorithm,
        key_id,
        token_type,
    })
}

fn verify_signature(
    token: &str,
    key: &jsonwebtoken::DecodingKey,
    algorithm: JwtAlgorithm,
) -> JwtResult<()> {
    let (message, signature) = token
        .rsplit_once('.')
        .ok_or_else(|| JwtError::new(JwtErrorKind::MalformedToken))?;
    let valid = jsonwebtoken::crypto::verify(
        signature,
        message.as_bytes(),
        key,
        algorithm.as_jsonwebtoken(),
    )
    .map_err(map_verification_error)?;
    if valid {
        Ok(())
    } else {
        Err(JwtError::new(JwtErrorKind::InvalidSignature))
    }
}

fn map_signing_error(error: jsonwebtoken::errors::Error) -> JwtError {
    use jsonwebtoken::errors::ErrorKind;

    let kind = match error.kind() {
        ErrorKind::InvalidAlgorithm | ErrorKind::InvalidKeyFormat => {
            JwtErrorKind::UnavailableSigningKey
        }
        ErrorKind::Json(_) => JwtErrorKind::Serialization,
        _ => JwtErrorKind::Serialization,
    };
    JwtError::new(kind)
}

fn map_verification_error(error: jsonwebtoken::errors::Error) -> JwtError {
    use jsonwebtoken::errors::ErrorKind;

    let kind = match error.kind() {
        ErrorKind::InvalidSignature => JwtErrorKind::InvalidSignature,
        ErrorKind::InvalidAlgorithm | ErrorKind::MissingAlgorithm => {
            JwtErrorKind::AlgorithmMismatch
        }
        ErrorKind::Base64(_)
        | ErrorKind::Json(_)
        | ErrorKind::Utf8(_)
        | ErrorKind::InvalidToken => JwtErrorKind::MalformedToken,
        _ => JwtErrorKind::MalformedToken,
    };
    JwtError::new(kind)
}

fn decode_claims<C>(encoded_payload: &str) -> JwtResult<JwtClaims<C>>
where
    C: DeserializeOwned,
{
    let mut claims = decode_json_object(encoded_payload)?;
    let expires_at = take_required_u64(&mut claims, "exp", JwtErrorKind::MissingExpiration)?;
    let issued_at = take_required_u64(&mut claims, "iat", JwtErrorKind::MissingIssuedAt)?;
    let not_before = take_optional_u64(&mut claims, "nbf")?;
    let issuer = take_string(&mut claims, "iss")?;
    let subject = take_string(&mut claims, "sub")?;
    let audiences = take_audiences(&mut claims)?;
    let jwt_id = take_string(&mut claims, "jti")?;
    let token_kind = match take_string(&mut claims, "token_use")? {
        Some(value) if value == JwtTokenKind::Access.claim_value() => JwtTokenKind::Access,
        Some(value) if value == JwtTokenKind::Refresh.claim_value() => JwtTokenKind::Refresh,
        _ => return Err(JwtError::new(JwtErrorKind::TokenKindMismatch)),
    };
    let custom = serde_json::from_value(Value::Object(claims))
        .map_err(|_| JwtError::new(JwtErrorKind::Deserialization))?;
    Ok(JwtClaims {
        registered: RegisteredJwtClaims {
            issuer,
            subject,
            audiences,
            expires_at,
            not_before,
            issued_at,
            jwt_id,
            token_kind,
        },
        custom,
    })
}

fn take_required_u64(
    object: &mut Map<String, Value>,
    name: &str,
    missing: JwtErrorKind,
) -> JwtResult<u64> {
    let value = object.remove(name).ok_or_else(|| JwtError::new(missing))?;
    value
        .as_u64()
        .ok_or_else(|| JwtError::new(JwtErrorKind::MalformedToken))
}

fn take_optional_u64(object: &mut Map<String, Value>, name: &str) -> JwtResult<Option<u64>> {
    object
        .remove(name)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| JwtError::new(JwtErrorKind::MalformedToken))
        })
        .transpose()
}

fn take_string(object: &mut Map<String, Value>, name: &str) -> JwtResult<Option<String>> {
    object
        .remove(name)
        .map(|value| match value {
            Value::String(value) => Ok(value),
            _ => Err(JwtError::new(JwtErrorKind::MalformedToken)),
        })
        .transpose()
}

fn take_audiences(object: &mut Map<String, Value>) -> JwtResult<Vec<String>> {
    let Some(value) = object.remove("aud") else {
        return Ok(Vec::new());
    };
    match value {
        Value::String(audience) => Ok(vec![audience]),
        Value::Array(audiences) => audiences
            .into_iter()
            .map(|value| match value {
                Value::String(audience) => Ok(audience),
                _ => Err(JwtError::new(JwtErrorKind::MalformedToken)),
            })
            .collect(),
        _ => Err(JwtError::new(JwtErrorKind::MalformedToken)),
    }
}

fn decode_json_object(encoded: &str) -> JwtResult<Map<String, Value>> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| JwtError::new(JwtErrorKind::MalformedToken))?;
    let value = serde_json::from_slice::<StrictJsonValue>(&bytes)
        .map_err(|_| JwtError::new(JwtErrorKind::MalformedToken))?;
    match value.into_value() {
        Value::Object(object) => Ok(object),
        _ => Err(JwtError::new(JwtErrorKind::MalformedToken)),
    }
}

enum StrictJsonValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

impl StrictJsonValue {
    fn into_value(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(value),
            Self::Number(value) => Value::Number(value),
            Self::String(value) => Value::String(value),
            Self::Array(values) => Value::Array(values.into_iter().map(Self::into_value).collect()),
            Self::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.into_value()))
                    .collect(),
            ),
        }
    }
}

impl<'de> serde::Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_f64(value)
            .map(StrictJsonValue::Number)
            .ok_or_else(|| E::custom("invalid JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(StrictJsonValue::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, StrictJsonValue>()? {
            if values.insert(key, value).is_some() {
                return Err(serde::de::Error::custom("duplicate JSON object key"));
            }
        }
        Ok(StrictJsonValue::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use mads_core::{ConfigBuilder, MapSource};

    use super::*;

    const SECRET: &str = "01234567890123456789012345678901";

    fn service_with_clock_skew(clock_skew_seconds: u64) -> JwtService {
        let config = ConfigBuilder::new()
            .source(MapSource::new(
                "test",
                [
                    ("passport.secret", SECRET.to_owned()),
                    (
                        "passport.clock_skew_seconds",
                        clock_skew_seconds.to_string(),
                    ),
                ],
            ))
            .build()
            .unwrap();
        JwtService::from_config(&config).unwrap()
    }

    fn access_header() -> JwtHeader {
        JwtHeader {
            algorithm: JwtAlgorithm::Hs256,
            key_id: None,
            token_type: Some(JwtTokenKind::Access.header_type().to_owned()),
        }
    }

    fn access_claims(expires_at: u64) -> RegisteredJwtClaims {
        RegisteredJwtClaims {
            issuer: None,
            subject: None,
            audiences: Vec::new(),
            expires_at,
            not_before: None,
            issued_at: 1,
            jwt_id: None,
            token_kind: JwtTokenKind::Access,
        }
    }

    #[test]
    fn expiration_at_current_time_is_expired_without_clock_skew() {
        let service = service_with_clock_skew(0);

        assert_eq!(
            service
                .validate_claims(
                    &access_header(),
                    &access_claims(100),
                    &JwtValidation::access(),
                    100,
                )
                .unwrap_err()
                .kind(),
            JwtErrorKind::Expired,
        );
    }

    #[test]
    fn expiration_at_skew_adjusted_current_time_is_expired() {
        let service = service_with_clock_skew(30);

        assert_eq!(
            service
                .validate_claims(
                    &access_header(),
                    &access_claims(70),
                    &JwtValidation::access(),
                    100,
                )
                .unwrap_err()
                .kind(),
            JwtErrorKind::Expired,
        );
    }
}
