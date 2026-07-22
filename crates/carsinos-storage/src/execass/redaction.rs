//! Fail-closed redaction before receipt persistence.

use super::canonical::{parse_strict_json, CanonicalValue};
use anyhow::{bail, Result};
use base64::Engine;
use std::collections::BTreeSet;
use std::fmt;
use zeroize::Zeroizing;

const REDACTED: &str = "[REDACTED]";
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_BYTES: usize = 2 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct SafeText(String);

impl fmt::Debug for SafeText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SafeText([REDACTED DEBUG])")
    }
}

/// Required receipt-boundary secret inventory.
///
/// Receipt append accepts this separately from [`SafeText`] so it can rescan
/// every canonical byte derived from storage, not only caller-authored prose.
/// Ordinary builds cannot create an empty inventory and accidentally claim a
/// redaction proof without registering at least the runtime's local secret.
#[derive(Clone)]
pub struct ReceiptRedactor {
    secrets: Vec<Zeroizing<String>>,
}

impl fmt::Debug for ReceiptRedactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReceiptRedactor")
            .field("registered_secret_count", &self.secrets.len())
            .finish()
    }
}

impl ReceiptRedactor {
    pub fn new(known_secrets: &[&str]) -> Result<Self> {
        if known_secrets.is_empty() {
            bail!("receipt redaction requires the authoritative secret inventory");
        }
        known_secret_byte_variants(known_secrets)?;
        Ok(Self {
            secrets: known_secrets
                .iter()
                .map(|secret| Zeroizing::new((*secret).to_owned()))
                .collect(),
        })
    }

    pub fn text(&self, raw: &str) -> Result<SafeText> {
        let secrets = self.secret_refs();
        SafeText::new(raw, &secrets)
    }

    pub fn summary(&self, raw: &str) -> Result<SafeText> {
        let secrets = self.secret_refs();
        SafeText::summary(raw, &secrets)
    }

    pub fn json(&self, raw: &str) -> Result<SafeJson> {
        let secrets = self.secret_refs();
        SafeJson::from_str(raw, &secrets)
    }

    pub(crate) fn reject_sensitive_bytes(&self, bytes: &[u8]) -> Result<()> {
        let secrets = self.secret_refs();
        for variant in known_secret_byte_variants(&secrets)? {
            if contains_bytes(bytes, &variant) {
                bail!("sensitive content could not be stored safely");
            }
        }
        Ok(())
    }

    fn secret_refs(&self) -> Vec<&str> {
        self.secrets.iter().map(|secret| secret.as_str()).collect()
    }
}

impl SafeText {
    pub fn new(raw: &str, known_secrets: &[&str]) -> Result<Self> {
        redact_text(raw, known_secrets, MAX_TEXT_BYTES).map(Self)
    }

    pub fn summary(raw: &str, known_secrets: &[&str]) -> Result<Self> {
        redact_text(raw, known_secrets, MAX_SUMMARY_BYTES).map(Self)
    }

    pub fn from_bytes(raw: &[u8], known_secrets: &[&str]) -> Result<Self> {
        let value = std::str::from_utf8(raw)
            .map_err(|_| anyhow::anyhow!("sensitive content could not be stored safely"))?;
        Self::new(value, known_secrets)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeJson(CanonicalValue);

impl SafeJson {
    pub fn from_str(raw: &str, known_secrets: &[&str]) -> Result<Self> {
        if raw.len() > MAX_TEXT_BYTES {
            bail!("sensitive content could not be stored safely");
        }
        let value = parse_strict_json(raw)
            .map_err(|_| anyhow::anyhow!("sensitive content could not be stored safely"))?;
        Ok(Self(redact_value(value, known_secrets, 0)?))
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.0.to_bytes()
    }

    #[cfg(test)]
    pub(crate) fn canonical(&self) -> &CanonicalValue {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueSecretHandle {
    version: i64,
    backend: String,
    opaque_id: String,
    purpose: String,
    capability_class: String,
}

impl OpaqueSecretHandle {
    pub fn new(
        version: i64,
        backend: &str,
        opaque_id: &str,
        purpose: &str,
        capability_class: &str,
    ) -> Result<Self> {
        if version != 1 {
            bail!("opaque secret reference version is unsupported");
        }
        let fields = [backend, opaque_id, purpose, capability_class];
        if fields.iter().any(|value| {
            value.is_empty()
                || value.len() > 256
                || value.chars().any(char::is_control)
                || match redact_text(value, &[], 256) {
                    Ok(redacted) => redacted != *value,
                    Err(_) => true,
                }
        }) {
            bail!("opaque secret reference is invalid");
        }
        Ok(Self {
            version,
            backend: backend.to_owned(),
            opaque_id: opaque_id.to_owned(),
            purpose: purpose.to_owned(),
            capability_class: capability_class.to_owned(),
        })
    }

    pub fn version(&self) -> i64 {
        self.version
    }

    pub fn backend(&self) -> &str {
        &self.backend
    }

    pub fn opaque_id(&self) -> &str {
        &self.opaque_id
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn capability_class(&self) -> &str {
        &self.capability_class
    }
}

fn redact_value(value: CanonicalValue, secrets: &[&str], depth: usize) -> Result<CanonicalValue> {
    if depth > 32 {
        bail!("sensitive content could not be stored safely");
    }
    Ok(match value {
        CanonicalValue::String(value) => {
            CanonicalValue::String(redact_text(&value, secrets, MAX_TEXT_BYTES)?)
        }
        CanonicalValue::Array(values) => {
            if values.len() > 1024 {
                bail!("sensitive content could not be stored safely");
            }
            CanonicalValue::Array(
                values
                    .into_iter()
                    .map(|value| redact_value(value, secrets, depth + 1))
                    .collect::<Result<_>>()?,
            )
        }
        CanonicalValue::Object(values) => {
            if values.len() > 256 {
                bail!("sensitive content could not be stored safely");
            }
            CanonicalValue::Object(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        if sensitive_key(&key) {
                            Ok((key, CanonicalValue::String(REDACTED.to_owned())))
                        } else {
                            Ok((key, redact_value(value, secrets, depth + 1)?))
                        }
                    })
                    .collect::<Result<_>>()?,
            )
        }
        other => other,
    })
}

fn redact_text(raw: &str, known_secrets: &[&str], limit: usize) -> Result<String> {
    if raw.len() > limit {
        bail!("sensitive content could not be stored safely");
    }
    let mut output = carsinos_protocol::execass::redact_execass_builtin_secret_patterns(raw);
    let mut variants = known_secret_byte_variants(known_secrets)?
        .into_iter()
        .filter_map(|variant| String::from_utf8(variant).ok())
        .collect::<Vec<_>>();
    variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
    for variant in &variants {
        if !variant.is_empty() {
            output = output.replace(variant, REDACTED);
        }
    }
    if output.len() > limit {
        bail!("sensitive content could not be stored safely");
    }
    Ok(output)
}

fn sensitive_key(key: &str) -> bool {
    let normalized: String = key
        .chars()
        .filter(|character| !matches!(character, '_' | '-' | '.' | ' '))
        .flat_map(char::to_lowercase)
        .collect();
    [
        "authorization",
        "proxyauthorization",
        "cookie",
        "setcookie",
        "token",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "bearertoken",
        "apikey",
        "clientsecret",
        "secret",
        "password",
        "passwd",
        "pwd",
        "credential",
        "privatekey",
        "signingkey",
        "sessionkey",
        "oauthcode",
        "authcode",
        "codeverifier",
        "otp",
        "recoverycode",
    ]
    .iter()
    .any(|denied| normalized == *denied || normalized.ends_with(denied))
}

fn known_secret_byte_variants(secrets: &[&str]) -> Result<BTreeSet<Vec<u8>>> {
    let mut output = BTreeSet::new();
    for secret in secrets {
        if secret.len() < 4 {
            bail!("sensitive content could not be stored safely");
        }
        let bytes = secret.as_bytes();
        insert_encoded_forms(&mut output, bytes);
        let percent_upper = percent(bytes, true).into_bytes();
        let percent_lower = percent(bytes, false).into_bytes();
        output.insert(percent_upper.clone());
        output.insert(percent_lower);
        output.insert(percent_all(bytes, true).into_bytes());
        output.insert(percent_all(bytes, false).into_bytes());
        output.insert(quote_plus(bytes).into_bytes());
        output.insert(percent(&percent_upper, true).into_bytes());
        let utf16le = secret
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let utf16be = secret
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        for encoded in [
            utf16le.clone(),
            [vec![0xff, 0xfe], utf16le].concat(),
            utf16be.clone(),
            [vec![0xfe, 0xff], utf16be].concat(),
        ] {
            insert_encoded_forms(&mut output, &encoded);
        }
    }
    Ok(output)
}

fn insert_encoded_forms(output: &mut BTreeSet<Vec<u8>>, bytes: &[u8]) {
    output.insert(bytes.to_vec());
    output.insert(hex(bytes, false).into_bytes());
    output.insert(hex(bytes, true).into_bytes());
    output.insert(
        base64::engine::general_purpose::STANDARD
            .encode(bytes)
            .into_bytes(),
    );
    output.insert(
        base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(bytes)
            .into_bytes(),
    );
    output.insert(
        base64::engine::general_purpose::URL_SAFE
            .encode(bytes)
            .into_bytes(),
    );
    output.insert(
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(bytes)
            .into_bytes(),
    );
}

fn hex(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|byte| {
            if upper {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            }
        })
        .collect()
}

fn percent(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
                (*byte as char).to_string()
            } else if upper {
                format!("%{byte:02X}")
            } else {
                format!("%{byte:02x}")
            }
        })
        .collect()
}

fn percent_all(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|byte| {
            if upper {
                format!("%{byte:02X}")
            } else {
                format!("%{byte:02x}")
            }
        })
        .collect()
}

fn quote_plus(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match *byte {
            b' ' => "+".to_owned(),
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') => {
                (byte as char).to_string()
            }
            byte => format!("%{byte:02X}"),
        })
        .collect()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_keys_and_free_text_patterns_are_redacted_before_bytes_exist() {
        let safe = SafeJson::from_str(
            r#"{"outer":[{"Access-Token":"value"}],"text":"Bearer abcdefgh JWT eyJabc.def.ghi"}"#,
            &[],
        )
        .unwrap();
        let bytes = String::from_utf8(safe.canonical().to_bytes()).unwrap();
        assert!(!bytes.contains("value"));
        assert!(!bytes.contains("abcdefgh"));
        assert!(!bytes.contains("eyJabc"));
    }

    #[test]
    fn safe_text_delegates_builtin_secret_pattern_redaction_to_protocol() {
        for raw in [
            "Bearer abcdefghijkl",
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==",
            "JWT eyJabc.def.ghi",
            "token sk-proj-abcdefghijklmnopqrstuvwxyz123456",
            "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----",
        ] {
            assert_eq!(
                SafeText::new(raw, &[]).unwrap().as_str(),
                carsinos_protocol::execass::redact_execass_builtin_secret_patterns(raw),
                "protocol/SafeText mismatch for {raw}"
            );
        }
    }

    #[test]
    fn registered_secret_encodings_are_redacted() {
        let secret = "canary-secret-123";
        let encoded = base64::engine::general_purpose::STANDARD.encode(secret);
        let hexed = hex(secret.as_bytes(), false);
        let url = percent(secret.as_bytes(), true);
        let safe = SafeText::new(&format!("{secret} {encoded} {hexed} {url}"), &[secret]).unwrap();
        assert!(!safe.as_str().contains(secret));
        assert!(!safe.as_str().contains(&encoded));
        assert!(!safe.as_str().contains(&hexed));
        assert!(!safe.as_str().contains(&url));
    }

    #[test]
    fn scanner_encoding_matrix_is_redacted_or_rejected_before_persistence() {
        let secret = "canary Secret/plus?+";
        let redactor = ReceiptRedactor::new(&[secret]).unwrap();
        let variants = scanner_variants(secret);
        assert_eq!(variants.len(), 23);
        for (name, bytes) in variants {
            match std::str::from_utf8(&bytes) {
                Ok(text) => {
                    let safe = redactor.text(text).unwrap();
                    redactor
                        .reject_sensitive_bytes(safe.as_str().as_bytes())
                        .unwrap_or_else(|error| panic!("{name} survived redaction: {error}"));
                }
                Err(_) => assert!(SafeText::from_bytes(&bytes, &[secret]).is_err(), "{name}"),
            }
        }
    }

    #[test]
    fn receipt_boundary_rejects_caller_bypass_and_empty_inventory() {
        let secret = "receipt-canary-secret";
        assert!(ReceiptRedactor::new(&[]).is_err());
        let redactor = ReceiptRedactor::new(&[secret]).unwrap();
        let caller_bypass = SafeText::summary(secret, &[]).unwrap();
        assert!(redactor
            .reject_sensitive_bytes(caller_bypass.as_str().as_bytes())
            .is_err());
        let safe = redactor.summary(secret).unwrap();
        redactor
            .reject_sensitive_bytes(safe.as_str().as_bytes())
            .unwrap();
    }

    #[test]
    fn invalid_utf8_and_bounds_fail_without_echoing_content() {
        let error = SafeText::from_bytes(&[0xff, 0xfe], &[])
            .unwrap_err()
            .to_string();
        assert_eq!(error, "sensitive content could not be stored safely");
        assert!(SafeText::summary(&"x".repeat(MAX_SUMMARY_BYTES + 1), &[]).is_err());
        let too_deep = format!("{}0{}", "[".repeat(33), "]".repeat(33));
        assert!(SafeJson::from_str(&too_deep, &[]).is_err());
    }

    #[test]
    fn secret_references_are_opaque_validated_identifiers() {
        let handle = OpaqueSecretHandle::new(
            1,
            "windows-credential-manager",
            "credential-slot-7",
            "provider-auth",
            "connector-call",
        )
        .unwrap();
        assert_eq!(handle.version(), 1);
        assert_eq!(handle.opaque_id(), "credential-slot-7");
        assert!(OpaqueSecretHandle::new(1, "backend", "Bearer abcdefgh", "auth", "call").is_err());
        assert!(OpaqueSecretHandle::new(2, "backend", "slot", "auth", "call").is_err());
    }

    fn scanner_variants(secret: &str) -> Vec<(&'static str, Vec<u8>)> {
        let raw = secret.as_bytes().to_vec();
        let utf16le = secret
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let utf16be = secret
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        let utf16le_bom = [vec![0xff, 0xfe], utf16le.clone()].concat();
        let utf16be_bom = [vec![0xfe, 0xff], utf16be.clone()].concat();
        let percent_upper = percent(&raw, true).into_bytes();
        vec![
            ("utf8", raw.clone()),
            ("utf16le", utf16le.clone()),
            ("utf16le_bom", utf16le_bom.clone()),
            ("utf16be", utf16be.clone()),
            ("utf16be_bom", utf16be_bom.clone()),
            (
                "base64_padded",
                base64::engine::general_purpose::STANDARD
                    .encode(&raw)
                    .into_bytes(),
            ),
            (
                "base64_unpadded",
                base64::engine::general_purpose::STANDARD_NO_PAD
                    .encode(&raw)
                    .into_bytes(),
            ),
            (
                "base64url_padded",
                base64::engine::general_purpose::URL_SAFE
                    .encode(&raw)
                    .into_bytes(),
            ),
            (
                "base64url_unpadded",
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(&raw)
                    .into_bytes(),
            ),
            ("hex_lower", hex(&raw, false).into_bytes()),
            ("hex_upper", hex(&raw, true).into_bytes()),
            ("percent_upper", percent_upper.clone()),
            ("percent_lower", percent(&raw, false).into_bytes()),
            ("plus_query", quote_plus(&raw).into_bytes()),
            ("double_percent", percent(&percent_upper, true).into_bytes()),
            (
                "base64_utf16le",
                base64::engine::general_purpose::STANDARD
                    .encode(&utf16le)
                    .into_bytes(),
            ),
            ("hex_utf16le", hex(&utf16le, false).into_bytes()),
            (
                "base64_utf16le_bom",
                base64::engine::general_purpose::STANDARD
                    .encode(&utf16le_bom)
                    .into_bytes(),
            ),
            ("hex_utf16le_bom", hex(&utf16le_bom, false).into_bytes()),
            (
                "base64_utf16be",
                base64::engine::general_purpose::STANDARD
                    .encode(&utf16be)
                    .into_bytes(),
            ),
            ("hex_utf16be", hex(&utf16be, false).into_bytes()),
            (
                "base64_utf16be_bom",
                base64::engine::general_purpose::STANDARD
                    .encode(&utf16be_bom)
                    .into_bytes(),
            ),
            ("hex_utf16be_bom", hex(&utf16be_bom, false).into_bytes()),
        ]
    }
}
