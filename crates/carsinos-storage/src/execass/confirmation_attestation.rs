//! Strict asymmetric verification for exact owner-confirmation attestations.
//!
//! The attestation is deliberately untrusted input. Its key identifier never
//! selects authority: the caller must provide the separately loaded, storage-
//! pinned verification key record. Key enrollment, private-key custody,
//! replay storage, decision resolution, and grant creation are outside this
//! cryptographic kernel.

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

const ATTESTATION_DOMAIN: &[u8] = b"carsinos.execass.confirmation_attestation.v1";
const ATTESTATION_DIGEST_DOMAIN: &[u8] = b"carsinos.execass.confirmation_attestation.digest.v1";
const DIGEST_HEX_LENGTH: usize = 64;
const SIGNATURE_HEX_LENGTH: usize = 128;

/// Public, untrusted fields presented for exact confirmation verification.
///
/// Every field is included in the fixed-order, length-prefixed signed bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationAttestationPayload {
    pub actor_type: String,
    pub credential_identity: String,
    pub authenticated_ingress: String,
    pub channel_assurance: String,
    pub request_correlation_id: String,
    pub source_message_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub normalized_intent_digest: String,
    pub policy_revision: u64,
    pub decision_id: String,
    pub decision_revision: u64,
    pub decision_result: String,
    pub canonical_manifest_digest: String,
    pub selected_logical_action_id: String,
    pub selected_action_digest: String,
    pub declared_consequence_digest: String,
    pub challenge_nonce_digest: String,
    pub challenge_expires_at_ms: u64,
    pub issued_at_ms: u64,
    pub canonical_root_identity: String,
    pub installation_identity: String,
    pub os_user_identity_digest: String,
    pub state_root_generation: u64,
    pub signer_key_generation: u64,
}

/// Public wire/storage shape. It contains no verification key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationAttestation {
    pub payload: ConfirmationAttestationPayload,
    pub key_id: String,
    pub signature_hex: String,
}

/// A verification key record selected by storage, independently of untrusted
/// attestation input.
#[derive(Clone, PartialEq, Eq)]
pub struct PinnedConfirmationAttestationKey {
    key_id: String,
    generation: u64,
    verifying_key: VerifyingKey,
}

impl fmt::Debug for PinnedConfirmationAttestationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PinnedConfirmationAttestationKey")
            .field("key_id", &self.key_id)
            .field("generation", &self.generation)
            .field("verifying_key", &"[REDACTED]")
            .finish()
    }
}

impl PinnedConfirmationAttestationKey {
    /// Validates a key record already selected from trusted storage.
    ///
    /// This remains dormant until the later DB-pinning integration; keeping
    /// construction crate-private prevents dependent crates from nominating
    /// their own trust root in the meantime.
    #[allow(dead_code)]
    pub(crate) fn from_bytes(
        key_id: impl Into<String>,
        generation: u64,
        verifying_key_bytes: [u8; 32],
    ) -> Result<Self, ConfirmationAttestationVerificationError> {
        let key_id = key_id.into();
        require_text("pinned_key_id", &key_id)?;
        require_positive("pinned_key_generation", generation)?;
        let verifying_key = VerifyingKey::from_bytes(&verifying_key_bytes)
            .map_err(|_| ConfirmationAttestationVerificationError::InvalidVerifyingKey)?;
        if verifying_key.is_weak() {
            return Err(ConfirmationAttestationVerificationError::WeakVerifyingKey);
        }
        Ok(Self {
            key_id,
            generation,
            verifying_key,
        })
    }

    pub(crate) fn from_hex(
        key_id: impl Into<String>,
        generation: u64,
        verifying_key_hex: &str,
    ) -> Result<Self, ConfirmationAttestationVerificationError> {
        Self::from_bytes(
            key_id,
            generation,
            decode_hex_array::<32>(verifying_key_hex)
                .map_err(|_| ConfirmationAttestationVerificationError::InvalidVerifyingKey)?,
        )
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// Opaque proof that all invariants and the strict Ed25519 verification passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedConfirmationAttestation {
    payload: ConfirmationAttestationPayload,
    key_id: String,
    attestation_digest: String,
}

impl VerifiedConfirmationAttestation {
    pub fn payload(&self) -> &ConfirmationAttestationPayload {
        &self.payload
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Stable SHA-256 identity for a later storage uniqueness constraint.
    pub fn attestation_digest(&self) -> &str {
        &self.attestation_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationAttestationVerificationError {
    InvalidField(&'static str),
    InvalidVerifyingKey,
    WeakVerifyingKey,
    KeyIdMismatch,
    KeyGenerationMismatch,
    MalformedSignatureHex,
    InvalidSignature,
    Expired,
    IssuedInFuture,
}

impl fmt::Display for ConfirmationAttestationVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(formatter, "invalid attestation field: {field}"),
            Self::InvalidVerifyingKey => formatter.write_str("invalid pinned verifying key"),
            Self::WeakVerifyingKey => formatter.write_str("weak pinned verifying key"),
            Self::KeyIdMismatch => {
                formatter.write_str("attestation key ID does not match pinned key")
            }
            Self::KeyGenerationMismatch => {
                formatter.write_str("attestation key generation does not match pinned key")
            }
            Self::MalformedSignatureHex => formatter.write_str("malformed attestation signature"),
            Self::InvalidSignature => formatter.write_str("invalid attestation signature"),
            Self::Expired => formatter.write_str("confirmation attestation is expired"),
            Self::IssuedInFuture => {
                formatter.write_str("confirmation attestation was issued in the future")
            }
        }
    }
}

impl std::error::Error for ConfirmationAttestationVerificationError {}

/// Strictly verifies untrusted input against one independently selected key.
///
/// Expiry is exclusive: `now_ms == challenge_expires_at_ms` is rejected.
pub fn verify_confirmation_attestation(
    attestation: &ConfirmationAttestation,
    pinned_key: &PinnedConfirmationAttestationKey,
    now_ms: u64,
) -> Result<VerifiedConfirmationAttestation, ConfirmationAttestationVerificationError> {
    let signed_bytes =
        confirmation_attestation_signing_bytes(&attestation.payload, &attestation.key_id)?;
    if attestation.key_id != pinned_key.key_id {
        return Err(ConfirmationAttestationVerificationError::KeyIdMismatch);
    }
    if attestation.payload.signer_key_generation != pinned_key.generation {
        return Err(ConfirmationAttestationVerificationError::KeyGenerationMismatch);
    }
    if now_ms >= attestation.payload.challenge_expires_at_ms {
        return Err(ConfirmationAttestationVerificationError::Expired);
    }
    if attestation.payload.issued_at_ms > now_ms {
        return Err(ConfirmationAttestationVerificationError::IssuedInFuture);
    }

    let signature_bytes = decode_signature_hex(&attestation.signature_hex)?;
    let signature = Signature::from_bytes(&signature_bytes);
    pinned_key
        .verifying_key
        .verify_strict(&signed_bytes, &signature)
        .map_err(|_| ConfirmationAttestationVerificationError::InvalidSignature)?;

    let attestation_digest = stable_attestation_digest(&signed_bytes, &signature_bytes);
    Ok(VerifiedConfirmationAttestation {
        payload: attestation.payload.clone(),
        key_id: attestation.key_id.clone(),
        attestation_digest,
    })
}

/// Returns the exact validated bytes consumed by Ed25519 signing and strict
/// verification. It accepts no signer or verification key and therefore
/// cannot nominate confirmation authority.
pub fn confirmation_attestation_signing_bytes(
    payload: &ConfirmationAttestationPayload,
    key_id: &str,
) -> Result<Vec<u8>, ConfirmationAttestationVerificationError> {
    validate_payload(payload, key_id)?;
    Ok(canonical_signed_bytes(payload, key_id))
}

pub(crate) fn confirmation_verifying_key_digest_hex(
    verifying_key_hex: &str,
) -> Result<String, ConfirmationAttestationVerificationError> {
    let bytes = decode_hex_array::<32>(verifying_key_hex)
        .map_err(|_| ConfirmationAttestationVerificationError::InvalidVerifyingKey)?;
    Ok(encode_hex(&Sha256::digest(bytes)))
}

fn validate_payload(
    payload: &ConfirmationAttestationPayload,
    key_id: &str,
) -> Result<(), ConfirmationAttestationVerificationError> {
    require_one_of(
        "actor_type",
        &payload.actor_type,
        &["human_local", "human_remote"],
    )?;
    require_text("credential_identity", &payload.credential_identity)?;
    require_text("authenticated_ingress", &payload.authenticated_ingress)?;
    require_text("channel_assurance", &payload.channel_assurance)?;
    require_text("request_correlation_id", &payload.request_correlation_id)?;
    require_optional_text("source_message_id", payload.source_message_id.as_deref())?;
    require_optional_text("provider_event_id", payload.provider_event_id.as_deref())?;
    require_digest(
        "normalized_intent_digest",
        &payload.normalized_intent_digest,
    )?;
    require_positive("policy_revision", payload.policy_revision)?;
    require_text("decision_id", &payload.decision_id)?;
    require_positive("decision_revision", payload.decision_revision)?;
    require_one_of(
        "decision_result",
        &payload.decision_result,
        &["confirm_and_continue"],
    )?;
    require_digest(
        "canonical_manifest_digest",
        &payload.canonical_manifest_digest,
    )?;
    require_text(
        "selected_logical_action_id",
        &payload.selected_logical_action_id,
    )?;
    require_digest("selected_action_digest", &payload.selected_action_digest)?;
    require_digest(
        "declared_consequence_digest",
        &payload.declared_consequence_digest,
    )?;
    require_digest("challenge_nonce_digest", &payload.challenge_nonce_digest)?;
    require_positive("challenge_expires_at_ms", payload.challenge_expires_at_ms)?;
    require_positive("issued_at_ms", payload.issued_at_ms)?;
    if payload.issued_at_ms >= payload.challenge_expires_at_ms {
        return Err(ConfirmationAttestationVerificationError::InvalidField(
            "attestation_time_window",
        ));
    }
    require_root_identity(&payload.canonical_root_identity)?;
    require_text("installation_identity", &payload.installation_identity)?;
    require_digest("os_user_identity_digest", &payload.os_user_identity_digest)?;
    require_positive("state_root_generation", payload.state_root_generation)?;
    require_positive("signer_key_generation", payload.signer_key_generation)?;
    require_text("key_id", key_id)?;
    match payload.actor_type.as_str() {
        "human_local"
            if payload.source_message_id.is_some() || payload.provider_event_id.is_some() =>
        {
            return Err(ConfirmationAttestationVerificationError::InvalidField(
                "local_actor_source",
            ));
        }
        "human_remote"
            if payload.source_message_id.is_none() || payload.provider_event_id.is_none() =>
        {
            return Err(ConfirmationAttestationVerificationError::InvalidField(
                "remote_actor_source",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn require_root_identity(value: &str) -> Result<(), ConfirmationAttestationVerificationError> {
    value
        .strip_prefix("sha256:")
        .ok_or(ConfirmationAttestationVerificationError::InvalidField(
            "canonical_root_identity",
        ))
        .and_then(|digest| require_digest("canonical_root_identity", digest))
}

fn require_text(
    field: &'static str,
    value: &str,
) -> Result<(), ConfirmationAttestationVerificationError> {
    if value.trim().is_empty() {
        Err(ConfirmationAttestationVerificationError::InvalidField(
            field,
        ))
    } else {
        Ok(())
    }
}

fn require_optional_text(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), ConfirmationAttestationVerificationError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        Err(ConfirmationAttestationVerificationError::InvalidField(
            field,
        ))
    } else {
        Ok(())
    }
}

fn require_one_of(
    field: &'static str,
    value: &str,
    allowed: &[&str],
) -> Result<(), ConfirmationAttestationVerificationError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ConfirmationAttestationVerificationError::InvalidField(
            field,
        ))
    }
}

fn require_positive(
    field: &'static str,
    value: u64,
) -> Result<(), ConfirmationAttestationVerificationError> {
    if value == 0 {
        Err(ConfirmationAttestationVerificationError::InvalidField(
            field,
        ))
    } else {
        Ok(())
    }
}

fn require_digest(
    field: &'static str,
    value: &str,
) -> Result<(), ConfirmationAttestationVerificationError> {
    if value.len() == DIGEST_HEX_LENGTH
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(ConfirmationAttestationVerificationError::InvalidField(
            field,
        ))
    }
}

fn canonical_signed_bytes(payload: &ConfirmationAttestationPayload, key_id: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);
    push_bytes(&mut out, ATTESTATION_DOMAIN);
    push_text(&mut out, &payload.actor_type);
    push_text(&mut out, &payload.credential_identity);
    push_text(&mut out, &payload.authenticated_ingress);
    push_text(&mut out, &payload.channel_assurance);
    push_text(&mut out, &payload.request_correlation_id);
    push_optional_text(&mut out, payload.source_message_id.as_deref());
    push_optional_text(&mut out, payload.provider_event_id.as_deref());
    push_text(&mut out, &payload.normalized_intent_digest);
    push_u64(&mut out, payload.policy_revision);
    push_text(&mut out, &payload.decision_id);
    push_u64(&mut out, payload.decision_revision);
    push_text(&mut out, &payload.decision_result);
    push_text(&mut out, &payload.canonical_manifest_digest);
    push_text(&mut out, &payload.selected_logical_action_id);
    push_text(&mut out, &payload.selected_action_digest);
    push_text(&mut out, &payload.declared_consequence_digest);
    push_text(&mut out, &payload.challenge_nonce_digest);
    push_u64(&mut out, payload.challenge_expires_at_ms);
    push_u64(&mut out, payload.issued_at_ms);
    push_text(&mut out, &payload.canonical_root_identity);
    push_text(&mut out, &payload.installation_identity);
    push_text(&mut out, &payload.os_user_identity_digest);
    push_u64(&mut out, payload.state_root_generation);
    push_text(&mut out, key_id);
    push_u64(&mut out, payload.signer_key_generation);
    out
}

fn push_text(out: &mut Vec<u8>, value: &str) {
    push_bytes(out, value.as_bytes());
}

fn push_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

fn push_optional_text(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            out.push(1);
            push_text(out, value);
        }
        None => out.push(0),
    }
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn decode_signature_hex(value: &str) -> Result<[u8; 64], ConfirmationAttestationVerificationError> {
    if value.len() != SIGNATURE_HEX_LENGTH {
        return Err(ConfirmationAttestationVerificationError::MalformedSignatureHex);
    }
    decode_hex_array::<64>(value)
}

fn decode_hex_array<const N: usize>(
    value: &str,
) -> Result<[u8; N], ConfirmationAttestationVerificationError> {
    if value.len() != N * 2 {
        return Err(ConfirmationAttestationVerificationError::MalformedSignatureHex);
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (decode_nibble(pair[0])? << 4) | decode_nibble(pair[1])?;
    }
    Ok(bytes)
}

fn decode_nibble(value: u8) -> Result<u8, ConfirmationAttestationVerificationError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(ConfirmationAttestationVerificationError::MalformedSignatureHex),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn stable_attestation_digest(signed_bytes: &[u8], signature_bytes: &[u8; 64]) -> String {
    let mut bytes = Vec::with_capacity(signed_bytes.len() + signature_bytes.len() + 24);
    push_bytes(&mut bytes, ATTESTATION_DIGEST_DOMAIN);
    push_bytes(&mut bytes, signed_bytes);
    push_bytes(&mut bytes, signature_bytes);
    encode_hex(&Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const NOW_MS: u64 = 1_000_000;
    const KEY_ID: &str = "confirmation-key-7";
    const KEY_GENERATION: u64 = 7;
    const TEST_SECRET: [u8; 32] = [42; 32];
    const OTHER_SECRET: [u8; 32] = [24; 32];

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, DIGEST_HEX_LENGTH).collect()
    }

    fn local_payload() -> ConfirmationAttestationPayload {
        ConfirmationAttestationPayload {
            actor_type: "human_local".to_string(),
            credential_identity: "native-client-1".to_string(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: "corr-local-1".to_string(),
            source_message_id: None,
            provider_event_id: None,
            normalized_intent_digest: digest('1'),
            policy_revision: 11,
            decision_id: "decision-local-1".to_string(),
            decision_revision: 3,
            decision_result: "confirm_and_continue".to_string(),
            canonical_manifest_digest: digest('2'),
            selected_logical_action_id: "action-1".to_string(),
            selected_action_digest: digest('3'),
            declared_consequence_digest: digest('4'),
            challenge_nonce_digest: digest('5'),
            challenge_expires_at_ms: NOW_MS + 100,
            issued_at_ms: NOW_MS - 100,
            canonical_root_identity: format!("sha256:{}", digest('7')),
            installation_identity: "install-1".to_string(),
            os_user_identity_digest: digest('6'),
            state_root_generation: 9,
            signer_key_generation: KEY_GENERATION,
        }
    }

    fn remote_payload() -> ConfirmationAttestationPayload {
        ConfirmationAttestationPayload {
            actor_type: "human_remote".to_string(),
            credential_identity: "telegram:owner-provider-1".to_string(),
            authenticated_ingress: "telegram-adapter".to_string(),
            channel_assurance: "allowlisted-remote".to_string(),
            request_correlation_id: "corr-remote-1".to_string(),
            source_message_id: Some("message-1".to_string()),
            provider_event_id: Some("update-22".to_string()),
            decision_id: "decision-remote-1".to_string(),
            ..local_payload()
        }
    }

    fn pinned(secret: [u8; 32], key_id: &str, generation: u64) -> PinnedConfirmationAttestationKey {
        let signing_key = SigningKey::from_bytes(&secret);
        PinnedConfirmationAttestationKey::from_bytes(
            key_id,
            generation,
            signing_key.verifying_key().to_bytes(),
        )
        .expect("valid pinned test key")
    }

    fn issue_test_attestation(
        payload: ConfirmationAttestationPayload,
        key_id: &str,
        secret: [u8; 32],
    ) -> ConfirmationAttestation {
        let signed_bytes = confirmation_attestation_signing_bytes(&payload, key_id)
            .expect("valid test signing payload");
        let signature = SigningKey::from_bytes(&secret).sign(&signed_bytes);
        ConfirmationAttestation {
            payload,
            key_id: key_id.to_string(),
            signature_hex: encode_hex(&signature.to_bytes()),
        }
    }

    fn assert_rejected(attestation: ConfirmationAttestation) {
        assert!(
            verify_confirmation_attestation(
                &attestation,
                &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION),
                NOW_MS,
            )
            .is_err(),
            "mutated attestation unexpectedly verified: {attestation:?}"
        );
    }

    #[test]
    fn confirmation_attestation_accepts_exact_local_payload() {
        let attestation = issue_test_attestation(local_payload(), KEY_ID, TEST_SECRET);
        let signing_bytes =
            confirmation_attestation_signing_bytes(&attestation.payload, &attestation.key_id)
                .expect("public canonical bytes");
        let signature = Signature::from_bytes(
            &decode_signature_hex(&attestation.signature_hex).expect("test signature hex"),
        );
        SigningKey::from_bytes(&TEST_SECRET)
            .verifying_key()
            .verify_strict(&signing_bytes, &signature)
            .expect("public bytes are the exact kernel-verified bytes");
        let verified = verify_confirmation_attestation(
            &attestation,
            &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION),
            NOW_MS,
        )
        .expect("exact local attestation verifies");

        assert_eq!(verified.payload(), &attestation.payload);
        assert_eq!(verified.key_id(), KEY_ID);
        assert_eq!(verified.attestation_digest().len(), DIGEST_HEX_LENGTH);
        assert_eq!(
            verified.attestation_digest(),
            verify_confirmation_attestation(
                &attestation,
                &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION),
                NOW_MS,
            )
            .expect("repeat verification")
            .attestation_digest()
        );
    }

    #[test]
    fn confirmation_attestation_accepts_exact_remote_payload() {
        let attestation = issue_test_attestation(remote_payload(), KEY_ID, TEST_SECRET);
        let verified = verify_confirmation_attestation(
            &attestation,
            &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION),
            NOW_MS,
        )
        .expect("exact remote attestation verifies");

        assert_eq!(verified.payload().actor_type, "human_remote");
        assert_eq!(
            verified.payload().source_message_id.as_deref(),
            Some("message-1")
        );
        assert_eq!(
            verified.payload().provider_event_id.as_deref(),
            Some("update-22")
        );
    }

    #[test]
    fn confirmation_attestation_rejects_every_signed_field_mutation() {
        macro_rules! mutate_and_reject {
            ($field:ident, $value:expr) => {{
                let mut attestation = issue_test_attestation(local_payload(), KEY_ID, TEST_SECRET);
                attestation.payload.$field = $value;
                assert_rejected(attestation);
            }};
        }

        mutate_and_reject!(actor_type, "human_remote".to_string());
        mutate_and_reject!(credential_identity, "native-client-2".to_string());
        mutate_and_reject!(authenticated_ingress, "other-ingress".to_string());
        mutate_and_reject!(channel_assurance, "other-assurance".to_string());
        mutate_and_reject!(request_correlation_id, "corr-other".to_string());
        mutate_and_reject!(source_message_id, Some("message-other".to_string()));
        mutate_and_reject!(provider_event_id, Some("event-other".to_string()));
        mutate_and_reject!(normalized_intent_digest, digest('a'));
        mutate_and_reject!(policy_revision, 12);
        mutate_and_reject!(decision_id, "decision-other".to_string());
        mutate_and_reject!(decision_revision, 4);
        mutate_and_reject!(decision_result, "revise".to_string());
        mutate_and_reject!(canonical_manifest_digest, digest('a'));
        mutate_and_reject!(selected_logical_action_id, "action-other".to_string());
        mutate_and_reject!(selected_action_digest, digest('b'));
        mutate_and_reject!(declared_consequence_digest, digest('c'));
        mutate_and_reject!(challenge_nonce_digest, digest('d'));
        mutate_and_reject!(challenge_expires_at_ms, NOW_MS + 101);
        mutate_and_reject!(issued_at_ms, NOW_MS - 101);
        mutate_and_reject!(canonical_root_identity, format!("sha256:{}", digest('a')));
        mutate_and_reject!(installation_identity, "install-other".to_string());
        mutate_and_reject!(os_user_identity_digest, digest('e'));
        mutate_and_reject!(state_root_generation, 10);
        mutate_and_reject!(signer_key_generation, KEY_GENERATION + 1);

        let mut key_id_mutation = issue_test_attestation(local_payload(), KEY_ID, TEST_SECRET);
        key_id_mutation.key_id = "confirmation-key-other".to_string();
        assert_rejected(key_id_mutation);
    }

    #[test]
    fn confirmation_attestation_rejects_invalid_field_shapes_and_times() {
        fn assert_invalid_payload(payload: ConfirmationAttestationPayload, key_id: &str) {
            assert!(confirmation_attestation_signing_bytes(&payload, key_id).is_err());
            assert_rejected(ConfirmationAttestation {
                payload,
                key_id: key_id.to_string(),
                signature_hex: "00".repeat(64),
            });
        }

        macro_rules! invalid_payload {
            ($field:ident, $value:expr) => {{
                let mut payload = local_payload();
                payload.$field = $value;
                assert_invalid_payload(payload, KEY_ID);
            }};
        }

        invalid_payload!(actor_type, "runtime".to_string());
        invalid_payload!(credential_identity, " ".to_string());
        invalid_payload!(authenticated_ingress, String::new());
        invalid_payload!(channel_assurance, String::new());
        invalid_payload!(request_correlation_id, String::new());
        invalid_payload!(source_message_id, Some(String::new()));
        invalid_payload!(provider_event_id, Some(" ".to_string()));
        invalid_payload!(normalized_intent_digest, "A".repeat(DIGEST_HEX_LENGTH));
        invalid_payload!(policy_revision, 0);
        invalid_payload!(decision_id, String::new());
        invalid_payload!(decision_revision, 0);
        invalid_payload!(decision_result, "confirm".to_string());
        invalid_payload!(canonical_manifest_digest, "a".repeat(63));
        invalid_payload!(selected_logical_action_id, String::new());
        invalid_payload!(selected_action_digest, "g".repeat(DIGEST_HEX_LENGTH));
        invalid_payload!(declared_consequence_digest, String::new());
        invalid_payload!(challenge_nonce_digest, "0x12".to_string());
        invalid_payload!(challenge_expires_at_ms, 0);
        invalid_payload!(issued_at_ms, 0);
        invalid_payload!(canonical_root_identity, String::new());
        invalid_payload!(installation_identity, String::new());
        invalid_payload!(os_user_identity_digest, "f".repeat(65));
        invalid_payload!(state_root_generation, 0);
        invalid_payload!(signer_key_generation, 0);

        let mut equal_times = local_payload();
        equal_times.issued_at_ms = equal_times.challenge_expires_at_ms;
        assert_invalid_payload(equal_times, KEY_ID);

        assert_invalid_payload(local_payload(), "");

        let mut local_with_remote_source = local_payload();
        local_with_remote_source.source_message_id = Some("message".to_string());
        local_with_remote_source.provider_event_id = Some("event".to_string());
        assert_invalid_payload(local_with_remote_source, KEY_ID);

        let mut remote_without_provider_event = remote_payload();
        remote_without_provider_event.provider_event_id = None;
        assert_invalid_payload(remote_without_provider_event, KEY_ID);
    }

    #[test]
    fn confirmation_attestation_rejects_wrong_keys_and_signature_shapes() {
        let attestation = issue_test_attestation(local_payload(), KEY_ID, TEST_SECRET);
        assert_eq!(
            verify_confirmation_attestation(
                &attestation,
                &pinned(OTHER_SECRET, KEY_ID, KEY_GENERATION),
                NOW_MS,
            ),
            Err(ConfirmationAttestationVerificationError::InvalidSignature)
        );
        assert_eq!(
            verify_confirmation_attestation(
                &attestation,
                &pinned(TEST_SECRET, "other-key", KEY_GENERATION),
                NOW_MS,
            ),
            Err(ConfirmationAttestationVerificationError::KeyIdMismatch)
        );
        assert_eq!(
            verify_confirmation_attestation(
                &attestation,
                &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION + 1),
                NOW_MS,
            ),
            Err(ConfirmationAttestationVerificationError::KeyGenerationMismatch)
        );

        for malformed in [
            "00",
            &"0".repeat(SIGNATURE_HEX_LENGTH - 1),
            &"z".repeat(SIGNATURE_HEX_LENGTH),
        ] {
            let mut malformed_attestation = attestation.clone();
            malformed_attestation.signature_hex = malformed.to_string();
            assert_eq!(
                verify_confirmation_attestation(
                    &malformed_attestation,
                    &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION),
                    NOW_MS,
                ),
                Err(ConfirmationAttestationVerificationError::MalformedSignatureHex)
            );
        }

        let mut mutated_signature = attestation;
        mutated_signature.signature_hex.replace_range(0..2, "00");
        assert_eq!(
            verify_confirmation_attestation(
                &mutated_signature,
                &pinned(TEST_SECRET, KEY_ID, KEY_GENERATION),
                NOW_MS,
            ),
            Err(ConfirmationAttestationVerificationError::InvalidSignature)
        );
    }

    #[test]
    fn confirmation_attestation_rejects_weak_invalid_keys_and_expiry_boundaries() {
        let invalid_key =
            PinnedConfirmationAttestationKey::from_bytes(KEY_ID, KEY_GENERATION, [2; 32]);
        assert_eq!(
            invalid_key,
            Err(ConfirmationAttestationVerificationError::InvalidVerifyingKey)
        );
        let weak_key =
            PinnedConfirmationAttestationKey::from_bytes(KEY_ID, KEY_GENERATION, [0; 32]);
        assert_eq!(
            weak_key,
            Err(ConfirmationAttestationVerificationError::WeakVerifyingKey)
        );
        assert_eq!(
            PinnedConfirmationAttestationKey::from_bytes(
                "",
                KEY_GENERATION,
                SigningKey::from_bytes(&TEST_SECRET)
                    .verifying_key()
                    .to_bytes()
            ),
            Err(ConfirmationAttestationVerificationError::InvalidField(
                "pinned_key_id"
            ))
        );
        assert_eq!(
            PinnedConfirmationAttestationKey::from_bytes(
                KEY_ID,
                0,
                SigningKey::from_bytes(&TEST_SECRET)
                    .verifying_key()
                    .to_bytes()
            ),
            Err(ConfirmationAttestationVerificationError::InvalidField(
                "pinned_key_generation"
            ))
        );

        let attestation = issue_test_attestation(local_payload(), KEY_ID, TEST_SECRET);
        let key = pinned(TEST_SECRET, KEY_ID, KEY_GENERATION);
        assert_eq!(
            verify_confirmation_attestation(
                &attestation,
                &key,
                attestation.payload.challenge_expires_at_ms,
            ),
            Err(ConfirmationAttestationVerificationError::Expired)
        );
        assert_eq!(
            verify_confirmation_attestation(&attestation, &key, NOW_MS - 101),
            Err(ConfirmationAttestationVerificationError::IssuedInFuture)
        );
    }
}
