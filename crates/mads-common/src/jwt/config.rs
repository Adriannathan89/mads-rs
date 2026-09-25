//! Passport JWT configuration parsing.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use mads_core::Config;

use super::{JwtAlgorithm, JwtError, JwtErrorKind, JwtResult};

const DEFAULT_MAX_TOKEN_BYTES: usize = 8_192;

#[allow(dead_code)]
pub(crate) enum PassportKeyMode {
    SimpleHmac {
        secret: Arc<[u8]>,
    },
    Named {
        active_key: String,
        keys: BTreeMap<String, ConfiguredKey>,
    },
}

#[allow(dead_code)]
pub(crate) struct ConfiguredKey {
    pub(crate) algorithm: JwtAlgorithm,
    pub(crate) secret: Option<Arc<[u8]>>,
    pub(crate) private_key: Option<KeyMaterial>,
    pub(crate) public_key: Option<KeyMaterial>,
}

#[allow(dead_code)]
pub(crate) enum KeyMaterial {
    Inline(Arc<[u8]>),
    File(PathBuf),
}

impl KeyMaterial {
    pub(crate) fn read(&self) -> JwtResult<Vec<u8>> {
        match self {
            Self::Inline(bytes) => Ok(bytes.to_vec()),
            Self::File(path) => std::fs::read(path)
                .map_err(|source| JwtError::with_source(JwtErrorKind::InvalidKeyMaterial, source)),
        }
    }
}

/// Immutable Passport JWT key description and common validation policy.
pub struct PassportConfig {
    mode: PassportKeyMode,
    algorithms: Vec<JwtAlgorithm>,
    issuer: Option<String>,
    audiences: Vec<String>,
    clock_skew_seconds: u64,
    max_token_bytes: usize,
}

impl PassportConfig {
    /// Parses and validates Passport JWT configuration.
    pub fn from_config(config: &Config) -> JwtResult<Self> {
        let (mode, algorithms) = if has_named_key_fields(config) {
            if config.get("passport.secret").is_some()
                || config.get_string_array("passport.secret").is_some()
            {
                return Err(invalid_configuration());
            }
            parse_named_key_mode(config)?
        } else {
            parse_simple_hmac_mode(config)?
        };

        let issuer = optional_scalar(config, "passport.issuer")?.map(str::to_owned);
        let audiences = match optional_string_array(config, "passport.audiences")? {
            Some(values) => deduplicate_strings(values),
            None => Vec::new(),
        };
        let clock_skew_seconds =
            parse_optional_u64(config, "passport.clock_skew_seconds")?.unwrap_or_default();
        let max_token_bytes = parse_optional_usize(config, "passport.max_token_bytes")?
            .unwrap_or(DEFAULT_MAX_TOKEN_BYTES);
        if max_token_bytes == 0 {
            return Err(invalid_configuration());
        }

        Ok(Self {
            mode,
            algorithms,
            issuer,
            audiences,
            clock_skew_seconds,
            max_token_bytes,
        })
    }

    /// Returns the configured algorithm allowlist in deterministic order.
    pub fn algorithms(&self) -> &[JwtAlgorithm] {
        &self.algorithms
    }

    /// Returns the configured issuer, when issuer validation is enabled.
    pub fn issuer(&self) -> Option<&str> {
        self.issuer.as_deref()
    }

    /// Returns the configured audiences in deterministic order.
    pub fn audiences(&self) -> &[String] {
        &self.audiences
    }

    /// Returns the allowed validation clock skew in seconds.
    pub const fn clock_skew_seconds(&self) -> u64 {
        self.clock_skew_seconds
    }

    /// Returns the maximum accepted encoded-token size in bytes.
    pub const fn max_token_bytes(&self) -> usize {
        self.max_token_bytes
    }

    /// Returns the active named key identifier, when named key mode is enabled.
    pub fn active_key_id(&self) -> Option<&str> {
        match &self.mode {
            PassportKeyMode::SimpleHmac { .. } => None,
            PassportKeyMode::Named { active_key, .. } => Some(active_key),
        }
    }

    /// Returns named key identifiers in deterministic lexical order.
    pub fn key_ids(&self) -> Vec<&str> {
        match &self.mode {
            PassportKeyMode::SimpleHmac { .. } => Vec::new(),
            PassportKeyMode::Named { keys, .. } => keys.keys().map(String::as_str).collect(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn key_mode(&self) -> &PassportKeyMode {
        &self.mode
    }
}

impl fmt::Debug for PassportConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (key_mode, named_key_count) = match &self.mode {
            PassportKeyMode::SimpleHmac { secret } => {
                debug_assert!(!secret.is_empty());
                ("simple-hmac", 0)
            }
            PassportKeyMode::Named { keys, .. } => ("named", keys.len()),
        };
        formatter
            .debug_struct("PassportConfig")
            .field("key_mode", &key_mode)
            .field("named_key_count", &named_key_count)
            .field("algorithms", &self.algorithms)
            .field("has_issuer", &self.issuer.is_some())
            .field("audience_count", &self.audiences.len())
            .field("clock_skew_seconds", &self.clock_skew_seconds)
            .field("max_token_bytes", &self.max_token_bytes)
            .finish()
    }
}

fn parse_simple_hmac_mode(config: &Config) -> JwtResult<(PassportKeyMode, Vec<JwtAlgorithm>)> {
    let secret = required_scalar(config, "passport.secret")?;
    let algorithms = match optional_string_array(config, "passport.algorithms")? {
        Some(values) => deduplicate_algorithms(values)?,
        None => vec![JwtAlgorithm::Hs256],
    };
    let algorithm = match algorithms.as_slice() {
        [algorithm @ (JwtAlgorithm::Hs256 | JwtAlgorithm::Hs384 | JwtAlgorithm::Hs512)] => {
            *algorithm
        }
        _ => return Err(invalid_configuration()),
    };
    if secret.len() < minimum_hmac_secret_bytes(algorithm) {
        return Err(invalid_configuration());
    }

    Ok((
        PassportKeyMode::SimpleHmac {
            secret: Arc::from(secret.as_bytes()),
        },
        algorithms,
    ))
}

fn parse_named_key_mode(config: &Config) -> JwtResult<(PassportKeyMode, Vec<JwtAlgorithm>)> {
    reject_named_string_arrays(config)?;
    let algorithms = named_algorithms(config)?;
    let active_key = required_scalar(config, "passport.active_key")?;
    if !valid_key_id(active_key) {
        return Err(invalid_configuration());
    }

    let mut fields_by_key = BTreeMap::<String, NamedKeyFields>::new();
    for (key, value) in config.iter() {
        let Some((key_id, field)) = named_key_field(key) else {
            continue;
        };
        if !valid_key_id(key_id) {
            return Err(invalid_configuration());
        }
        fields_by_key
            .entry(key_id.to_owned())
            .or_default()
            .insert(field, value.value())?;
    }
    if fields_by_key.is_empty() || !fields_by_key.contains_key(active_key) {
        return Err(invalid_configuration());
    }

    let mut keys = BTreeMap::new();
    for (key_id, fields) in fields_by_key {
        let configured_key =
            configured_key(config, &key_id, fields, &algorithms, key_id == active_key)?;
        keys.insert(key_id, configured_key);
    }

    Ok((
        PassportKeyMode::Named {
            active_key: active_key.to_owned(),
            keys,
        },
        algorithms,
    ))
}

fn has_named_key_fields(config: &Config) -> bool {
    config.iter().any(|(key, _)| named_key_prefix(key))
        || config
            .iter_string_arrays()
            .any(|(key, _)| named_key_prefix(key))
}

fn named_key_prefix(key: &str) -> bool {
    key == "passport.active_key" || key.starts_with("passport.keys.")
}

fn reject_named_string_arrays(config: &Config) -> JwtResult<()> {
    if config
        .iter_string_arrays()
        .any(|(key, _)| key == "passport.active_key" || key.starts_with("passport.keys."))
    {
        return Err(invalid_configuration());
    }
    Ok(())
}

fn named_algorithms(config: &Config) -> JwtResult<Vec<JwtAlgorithm>> {
    let values =
        optional_string_array(config, "passport.algorithms")?.ok_or_else(invalid_configuration)?;
    let mut seen = HashSet::new();
    let mut algorithms = Vec::with_capacity(values.len());
    for value in values {
        let algorithm = JwtAlgorithm::from_str(value).map_err(|_| invalid_configuration())?;
        if !seen.insert(algorithm) {
            return Err(invalid_configuration());
        }
        algorithms.push(algorithm);
    }
    if algorithms.is_empty() {
        return Err(invalid_configuration());
    }
    Ok(algorithms)
}

fn named_key_field(key: &str) -> Option<(&str, &str)> {
    let remainder = key.strip_prefix("passport.keys.")?;
    remainder.rsplit_once('.')
}

fn valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id.len() <= 128
        && key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[derive(Default)]
struct NamedKeyFields {
    algorithm: Option<String>,
    secret: Option<String>,
    private_key: Option<String>,
    private_key_file: Option<String>,
    public_key: Option<String>,
    public_key_file: Option<String>,
}

impl NamedKeyFields {
    fn insert(&mut self, field: &str, value: &str) -> JwtResult<()> {
        let target = match field {
            "algorithm" => &mut self.algorithm,
            "secret" => &mut self.secret,
            "private_key" => &mut self.private_key,
            "private_key_file" => &mut self.private_key_file,
            "public_key" => &mut self.public_key,
            "public_key_file" => &mut self.public_key_file,
            _ => return Err(invalid_configuration()),
        };
        if target.replace(value.to_owned()).is_some() {
            return Err(invalid_configuration());
        }
        Ok(())
    }
}

fn configured_key(
    config: &Config,
    key_id: &str,
    fields: NamedKeyFields,
    algorithms: &[JwtAlgorithm],
    active: bool,
) -> JwtResult<ConfiguredKey> {
    let algorithm = fields
        .algorithm
        .as_deref()
        .ok_or_else(invalid_configuration)
        .and_then(|algorithm| {
            JwtAlgorithm::from_str(algorithm).map_err(|_| invalid_configuration())
        })?;
    if !algorithms.contains(&algorithm) {
        return Err(invalid_configuration());
    }

    let private_key = material_source(
        config,
        key_id,
        fields.private_key,
        "private_key_file",
        fields.private_key_file,
    )?;
    let public_key = material_source(
        config,
        key_id,
        fields.public_key,
        "public_key_file",
        fields.public_key_file,
    )?;

    match algorithm {
        JwtAlgorithm::Hs256 | JwtAlgorithm::Hs384 | JwtAlgorithm::Hs512 => {
            if private_key.is_some() || public_key.is_some() {
                return Err(invalid_configuration());
            }
            let secret = fields.secret.ok_or_else(invalid_configuration)?;
            if secret.is_empty() || secret.len() < minimum_hmac_secret_bytes(algorithm) {
                return Err(invalid_configuration());
            }
            Ok(ConfiguredKey {
                algorithm,
                secret: Some(Arc::from(secret.as_bytes())),
                private_key: None,
                public_key: None,
            })
        }
        JwtAlgorithm::Rs256
        | JwtAlgorithm::Rs384
        | JwtAlgorithm::Rs512
        | JwtAlgorithm::Es256
        | JwtAlgorithm::Es384 => {
            if fields.secret.is_some() {
                return Err(invalid_configuration());
            }
            let public_key = public_key.ok_or_else(invalid_configuration)?;
            if active && private_key.is_none() {
                return Err(invalid_configuration());
            }
            validate_asymmetric_material(algorithm, &public_key, private_key.as_ref())?;
            Ok(ConfiguredKey {
                algorithm,
                secret: None,
                private_key,
                public_key: Some(public_key),
            })
        }
    }
}

fn material_source(
    config: &Config,
    key_id: &str,
    inline: Option<String>,
    file_field: &str,
    file: Option<String>,
) -> JwtResult<Option<KeyMaterial>> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(invalid_configuration()),
        (Some(value), None) if value.is_empty() => Err(invalid_configuration()),
        (Some(value), None) => Ok(Some(KeyMaterial::Inline(Arc::from(value.as_bytes())))),
        (None, Some(value)) if value.is_empty() => Err(invalid_configuration()),
        (None, Some(_)) => {
            let key = format!("passport.keys.{key_id}.{file_field}");
            let path = config
                .resolve_path(&key)
                .ok_or_else(invalid_configuration)?;
            Ok(Some(KeyMaterial::File(path)))
        }
        (None, None) => Ok(None),
    }
}

fn validate_asymmetric_material(
    algorithm: JwtAlgorithm,
    public_material: &KeyMaterial,
    private_material: Option<&KeyMaterial>,
) -> JwtResult<()> {
    let public_bytes = public_material.read()?;
    let decoding_key = decoding_key(algorithm, &public_bytes)?;
    validate_decoding_key(algorithm, &decoding_key)?;

    if let Some(private_material) = private_material {
        let private_bytes = private_material.read()?;
        let encoding_key = encoding_key(algorithm, &private_bytes)?;
        let message = b"mads-passport-key-validation";
        let signature = super::crypto::sign(message, &encoding_key, algorithm.as_jsonwebtoken())
            .map_err(invalid_key_material_with_source)?;
        let matches = super::crypto::verify(
            &signature,
            message,
            &decoding_key,
            algorithm.as_jsonwebtoken(),
        )
        .map_err(invalid_key_material_with_source)?;
        if !matches {
            return Err(JwtError::new(JwtErrorKind::InvalidKeyMaterial));
        }
    }
    Ok(())
}

pub(super) fn encoding_key(
    algorithm: JwtAlgorithm,
    bytes: &[u8],
) -> JwtResult<jsonwebtoken::EncodingKey> {
    match algorithm {
        JwtAlgorithm::Rs256 | JwtAlgorithm::Rs384 | JwtAlgorithm::Rs512 => {
            jsonwebtoken::EncodingKey::from_rsa_pem(bytes).map_err(invalid_key_material_with_source)
        }
        JwtAlgorithm::Es256 | JwtAlgorithm::Es384 => {
            jsonwebtoken::EncodingKey::from_ec_pem(bytes).map_err(invalid_key_material_with_source)
        }
        JwtAlgorithm::Hs256 | JwtAlgorithm::Hs384 | JwtAlgorithm::Hs512 => {
            Err(JwtError::new(JwtErrorKind::InvalidKeyMaterial))
        }
    }
}

pub(super) fn decoding_key(
    algorithm: JwtAlgorithm,
    bytes: &[u8],
) -> JwtResult<jsonwebtoken::DecodingKey> {
    match algorithm {
        JwtAlgorithm::Rs256 | JwtAlgorithm::Rs384 | JwtAlgorithm::Rs512 => {
            jsonwebtoken::DecodingKey::from_rsa_pem(bytes).map_err(invalid_key_material_with_source)
        }
        JwtAlgorithm::Es256 | JwtAlgorithm::Es384 => {
            jsonwebtoken::DecodingKey::from_ec_pem(bytes).map_err(invalid_key_material_with_source)
        }
        JwtAlgorithm::Hs256 | JwtAlgorithm::Hs384 | JwtAlgorithm::Hs512 => {
            Err(JwtError::new(JwtErrorKind::InvalidKeyMaterial))
        }
    }
}

fn validate_decoding_key(
    algorithm: JwtAlgorithm,
    key: &jsonwebtoken::DecodingKey,
) -> JwtResult<()> {
    super::crypto::verify(
        "",
        b"mads-passport-key-validation",
        key,
        algorithm.as_jsonwebtoken(),
    )
    .map(|_| ())
    .map_err(invalid_key_material_with_source)
}

fn invalid_key_material_with_source<E>(source: E) -> JwtError
where
    E: std::error::Error + Send + Sync + 'static,
{
    JwtError::with_source(JwtErrorKind::InvalidKeyMaterial, source)
}

fn required_scalar<'a>(config: &'a Config, key: &str) -> JwtResult<&'a str> {
    optional_scalar(config, key)?.ok_or_else(invalid_configuration)
}

fn optional_scalar<'a>(config: &'a Config, key: &str) -> JwtResult<Option<&'a str>> {
    if config.get_string_array(key).is_some() {
        return Err(invalid_configuration());
    }
    match config.get(key) {
        Some("") => Err(invalid_configuration()),
        value => Ok(value),
    }
}

fn optional_string_array<'a>(config: &'a Config, key: &str) -> JwtResult<Option<&'a [String]>> {
    if config.get(key).is_some() {
        return Err(invalid_configuration());
    }
    match config.get_string_array(key) {
        Some(values) if values.is_empty() || values.iter().any(String::is_empty) => {
            Err(invalid_configuration())
        }
        values => Ok(values),
    }
}

fn parse_optional_u64(config: &Config, key: &str) -> JwtResult<Option<u64>> {
    optional_scalar(config, key)?
        .map(|value| value.parse().map_err(|_| invalid_configuration()))
        .transpose()
}

fn parse_optional_usize(config: &Config, key: &str) -> JwtResult<Option<usize>> {
    optional_scalar(config, key)?
        .map(|value| value.parse().map_err(|_| invalid_configuration()))
        .transpose()
}

fn deduplicate_algorithms(values: &[String]) -> JwtResult<Vec<JwtAlgorithm>> {
    let mut seen = HashSet::new();
    let mut algorithms = Vec::new();
    for value in values {
        let algorithm = JwtAlgorithm::from_str(value).map_err(|_| invalid_configuration())?;
        if seen.insert(algorithm) {
            algorithms.push(algorithm);
        }
    }
    Ok(algorithms)
}

fn deduplicate_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .filter(|value| seen.insert(value.as_str()))
        .cloned()
        .collect()
}

const fn minimum_hmac_secret_bytes(algorithm: JwtAlgorithm) -> usize {
    match algorithm {
        JwtAlgorithm::Hs256 => 32,
        JwtAlgorithm::Hs384 => 48,
        JwtAlgorithm::Hs512 => 64,
        JwtAlgorithm::Rs256
        | JwtAlgorithm::Rs384
        | JwtAlgorithm::Rs512
        | JwtAlgorithm::Es256
        | JwtAlgorithm::Es384 => 0,
    }
}

const fn invalid_configuration() -> JwtError {
    JwtError::new(JwtErrorKind::InvalidConfiguration)
}
