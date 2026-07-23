//! Strict verification for signed run-control attestations.
//!
//! The envelope is untrusted. Storage independently selects the one active
//! confirmation-authority key and verifies both its fixed custody identity and
//! the exact protocol-owned canonical bytes.

use carsinos_protocol::execass::{
    run_control_attestation_signing_bytes, RunControlAttestation, RunControlAttestationPayload,
};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;

const ATTESTATION_DIGEST_DOMAIN: &[u8] = b"carsinos.execass.run_control_attestation.digest.v1";
pub(crate) const RUN_CONTROL_ATTESTATION_MAX_AGE_MS: i64 = 60_000;
const RUN_CONTROL_ATTESTATION_MAX_FUTURE_SKEW_MS: i64 = 5_000;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PinnedRunControlAttestationKey {
    pub(crate) key_id: String,
    pub(crate) generation: i64,
    verifying_key: VerifyingKey,
    pub(crate) canonical_root_identity: String,
    pub(crate) installation_identity: String,
    pub(crate) os_user_identity_digest: String,
    pub(crate) state_root_generation: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinnedRunControlIdentity {
    pub(crate) key_id: String,
    pub(crate) generation: i64,
    pub(crate) canonical_root_identity: String,
    pub(crate) installation_identity: String,
    pub(crate) os_user_identity_digest: String,
    pub(crate) state_root_generation: i64,
}

impl fmt::Debug for PinnedRunControlAttestationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PinnedRunControlAttestationKey")
            .field("key_id", &self.key_id)
            .field("generation", &self.generation)
            .field("verifying_key", &"[REDACTED]")
            .field("canonical_root_identity", &self.canonical_root_identity)
            .field("installation_identity", &self.installation_identity)
            .field("os_user_identity_digest", &self.os_user_identity_digest)
            .field("state_root_generation", &self.state_root_generation)
            .finish()
    }
}

impl PinnedRunControlAttestationKey {
    pub(crate) fn from_hex(
        identity: PinnedRunControlIdentity,
        verifying_key_hex: &str,
    ) -> Result<Self, RunControlAttestationVerificationError> {
        if identity.key_id.trim().is_empty()
            || identity.generation <= 0
            || identity.canonical_root_identity.trim().is_empty()
            || identity.installation_identity.trim().is_empty()
            || identity.os_user_identity_digest.trim().is_empty()
            || identity.state_root_generation <= 0
        {
            return Err(RunControlAttestationVerificationError::InvalidPinnedKey);
        }
        let bytes = decode_hex_array::<32>(verifying_key_hex)
            .map_err(|_| RunControlAttestationVerificationError::InvalidPinnedKey)?;
        let verifying_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|_| RunControlAttestationVerificationError::InvalidPinnedKey)?;
        if verifying_key.is_weak() {
            return Err(RunControlAttestationVerificationError::InvalidPinnedKey);
        }
        Ok(Self {
            key_id: identity.key_id,
            generation: identity.generation,
            verifying_key,
            canonical_root_identity: identity.canonical_root_identity,
            installation_identity: identity.installation_identity,
            os_user_identity_digest: identity.os_user_identity_digest,
            state_root_generation: identity.state_root_generation,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedRunControlAttestation {
    payload: RunControlAttestationPayload,
    key_id: String,
    attestation_digest: String,
}

impl VerifiedRunControlAttestation {
    pub(crate) fn payload(&self) -> &RunControlAttestationPayload {
        &self.payload
    }
    pub(crate) fn key_id(&self) -> &str {
        &self.key_id
    }
    pub(crate) fn attestation_digest(&self) -> &str {
        &self.attestation_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunControlAttestationVerificationError {
    InvalidShape,
    InvalidPinnedKey,
    PinnedIdentityMismatch,
    MalformedSignature,
    InvalidSignature,
    Stale,
    IssuedInFuture,
}

impl fmt::Display for RunControlAttestationVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidShape => "invalid run-control attestation shape",
            Self::InvalidPinnedKey => "invalid pinned run-control verification key",
            Self::PinnedIdentityMismatch => {
                "run-control attestation does not match the pinned custody identity"
            }
            Self::MalformedSignature => "malformed run-control attestation signature",
            Self::InvalidSignature => "invalid run-control attestation signature",
            Self::Stale => "run-control attestation is stale",
            Self::IssuedInFuture => "run-control attestation was issued in the future",
        })
    }
}

impl std::error::Error for RunControlAttestationVerificationError {}

pub(crate) fn verify_run_control_attestation(
    attestation: &RunControlAttestation,
    pinned: &PinnedRunControlAttestationKey,
    trusted_now: i64,
) -> Result<VerifiedRunControlAttestation, RunControlAttestationVerificationError> {
    let signed_bytes =
        run_control_attestation_signing_bytes(&attestation.payload, &attestation.key_id)
            .map_err(|_| RunControlAttestationVerificationError::InvalidShape)?;
    let payload = &attestation.payload;
    if attestation.key_id != pinned.key_id
        || payload.signer_key_generation != pinned.generation
        || payload.canonical_root_identity != pinned.canonical_root_identity
        || payload.installation_identity != pinned.installation_identity
        || payload.os_user_identity_digest != pinned.os_user_identity_digest
        || payload.state_root_generation != pinned.state_root_generation
    {
        return Err(RunControlAttestationVerificationError::PinnedIdentityMismatch);
    }
    validate_attestation_timestamps(payload.observed_at_ms, payload.issued_at_ms, trusted_now)?;
    let signature_bytes = decode_hex_array::<64>(&attestation.signature_hex)
        .map_err(|_| RunControlAttestationVerificationError::MalformedSignature)?;
    let signature = Signature::from_bytes(&signature_bytes);
    pinned
        .verifying_key
        .verify_strict(&signed_bytes, &signature)
        .map_err(|_| RunControlAttestationVerificationError::InvalidSignature)?;
    Ok(VerifiedRunControlAttestation {
        payload: payload.clone(),
        key_id: attestation.key_id.clone(),
        attestation_digest: stable_attestation_digest(&signed_bytes, &signature_bytes),
    })
}

fn validate_attestation_timestamps(
    observed_at_ms: i64,
    issued_at_ms: i64,
    trusted_now: i64,
) -> Result<(), RunControlAttestationVerificationError> {
    let issuance_delay_ms = issued_at_ms
        .checked_sub(observed_at_ms)
        .ok_or(RunControlAttestationVerificationError::Stale)?;
    if trusted_now <= 0
        || issuance_delay_ms > RUN_CONTROL_ATTESTATION_MAX_FUTURE_SKEW_MS
        || trusted_now.saturating_sub(observed_at_ms) > RUN_CONTROL_ATTESTATION_MAX_AGE_MS
    {
        return Err(RunControlAttestationVerificationError::Stale);
    }
    let latest_accepted = trusted_now.saturating_add(RUN_CONTROL_ATTESTATION_MAX_FUTURE_SKEW_MS);
    if observed_at_ms > latest_accepted || issued_at_ms > latest_accepted {
        return Err(RunControlAttestationVerificationError::IssuedInFuture);
    }
    Ok(())
}

pub(crate) fn run_control_attestation_digest(
    attestation: &RunControlAttestation,
) -> Result<String, RunControlAttestationVerificationError> {
    let signed_bytes =
        run_control_attestation_signing_bytes(&attestation.payload, &attestation.key_id)
            .map_err(|_| RunControlAttestationVerificationError::InvalidShape)?;
    let signature = decode_hex_array::<64>(&attestation.signature_hex)
        .map_err(|_| RunControlAttestationVerificationError::MalformedSignature)?;
    Ok(stable_attestation_digest(&signed_bytes, &signature))
}

fn stable_attestation_digest(signed_bytes: &[u8], signature: &[u8; 64]) -> String {
    let mut digest = Sha256::new();
    digest.update((ATTESTATION_DIGEST_DOMAIN.len() as u64).to_be_bytes());
    digest.update(ATTESTATION_DIGEST_DOMAIN);
    digest.update((signed_bytes.len() as u64).to_be_bytes());
    digest.update(signed_bytes);
    digest.update((signature.len() as u64).to_be_bytes());
    digest.update(signature);
    format!("{:x}", digest.finalize())
}

fn decode_hex_array<const N: usize>(
    value: &str,
) -> Result<[u8; N], RunControlAttestationVerificationError> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(RunControlAttestationVerificationError::MalformedSignature);
    }
    let mut output = [0_u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| RunControlAttestationVerificationError::MalformedSignature)?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_bounds_accept_exact_edges_and_reject_one_millisecond_beyond() {
        let now = 1_000_000;
        assert_eq!(
            validate_attestation_timestamps(
                now - RUN_CONTROL_ATTESTATION_MAX_AGE_MS,
                now - RUN_CONTROL_ATTESTATION_MAX_AGE_MS
                    + RUN_CONTROL_ATTESTATION_MAX_FUTURE_SKEW_MS,
                now,
            ),
            Ok(())
        );
        assert_eq!(
            validate_attestation_timestamps(
                now - RUN_CONTROL_ATTESTATION_MAX_AGE_MS - 1,
                now - RUN_CONTROL_ATTESTATION_MAX_AGE_MS - 1,
                now,
            ),
            Err(RunControlAttestationVerificationError::Stale)
        );
        assert_eq!(
            validate_attestation_timestamps(
                now,
                now + RUN_CONTROL_ATTESTATION_MAX_FUTURE_SKEW_MS,
                now,
            ),
            Ok(())
        );
        assert_eq!(
            validate_attestation_timestamps(
                now,
                now + RUN_CONTROL_ATTESTATION_MAX_FUTURE_SKEW_MS + 1,
                now,
            ),
            Err(RunControlAttestationVerificationError::Stale)
        );
    }

    #[test]
    fn timestamp_bounds_fail_closed_without_overflow_for_hostile_extremes() {
        assert_eq!(
            validate_attestation_timestamps(i64::MAX, i64::MIN, i64::MAX),
            Err(RunControlAttestationVerificationError::Stale)
        );
        assert_eq!(
            validate_attestation_timestamps(1, i64::MAX, i64::MAX),
            Err(RunControlAttestationVerificationError::Stale)
        );
        assert_eq!(
            validate_attestation_timestamps(i64::MAX, i64::MAX, i64::MAX),
            Ok(())
        );
        assert_eq!(
            validate_attestation_timestamps(i64::MAX, i64::MAX, 1),
            Err(RunControlAttestationVerificationError::IssuedInFuture)
        );
    }
}
