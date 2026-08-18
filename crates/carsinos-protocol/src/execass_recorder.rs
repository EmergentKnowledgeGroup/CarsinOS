//! Canonical, closed wire contract for the EA-213 execute-once recorder.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const RECORDER_PROTOCOL_VERSION: &str = "carsinos.execass.effect-recorder.v1";
pub const RECORDER_MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const RECORDER_HANDSHAKE_VERSION: &str = "carsinos.execass.effect-recorder.handshake.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecorderBindingV1 {
    pub protocol_version: String,
    pub canonical_root_identity: String,
    pub installation_id: String,
    pub state_root_generation: i64,
    pub os_user_identity_digest: String,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OpaqueSecretHandleV1 {
    pub version: i64,
    pub backend: String,
    pub opaque_id: String,
    pub purpose: String,
    pub capability_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpaqueOperandEnvelopeV1 {
    pub non_secret: Value,
    #[serde(default)]
    pub secret_handles: Vec<OpaqueSecretHandleV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecuteOnceV1 {
    pub binding: RecorderBindingV1,
    pub request_id: String,
    pub claim_event_id: String,
    pub claim_receipt_id: String,
    pub continuation_fencing_token: i64,
    pub delegation_id: String,
    pub continuation_id: String,
    pub action_id: String,
    pub logical_effect_id: String,
    pub internal_idempotency_key: String,
    pub attempt_id: String,
    pub attempt_number: i64,
    pub provider_identity: String,
    pub provider_version: String,
    pub adapter_identity: String,
    pub adapter_artifact_digest: String,
    pub provider_request_digest: String,
    pub provider_idempotency_key: Option<String>,
    pub reconciliation_key: Option<String>,
    pub manifest_digest: String,
    pub payload_digest: String,
    pub operand_envelope: OpaqueOperandEnvelopeV1,
    pub deadline_ms: i64,
    pub client_nonce: String,
    pub command_mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryOnlyV1 {
    pub binding: RecorderBindingV1,
    pub request_id: String,
    pub attempt_id: String,
    pub expected_command_digest: Option<String>,
    pub known_journal_head: Option<String>,
    pub client_nonce: String,
    pub command_mac: String,
}

/// A read-only request for provider-backed evidence about an already-journaled
/// attempt. The caller supplies identity digests and a bounded observation
/// window only; it cannot supply a result or technical-resource actuals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReconcileV1 {
    pub binding: RecorderBindingV1,
    pub request_id: String,
    pub attempt_id: String,
    pub expected_command_digest: String,
    pub reconciliation_key: String,
    pub reconciliation_key_digest: String,
    pub consistency_window_start_ms: i64,
    pub consistency_window_end_ms: i64,
    pub client_nonce: String,
    pub command_mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TechnicalResourceActualV1 {
    pub reservation_id: String,
    pub amount_actual: i64,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecorderHandshakeChallengeV1 {
    pub handshake_version: String,
    pub binding: RecorderBindingV1,
    pub client_nonce: String,
    pub request_authentication_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecorderHandshakeAttestationV1 {
    pub handshake_version: String,
    pub binding: RecorderBindingV1,
    pub client_nonce: String,
    pub request_authentication_digest: String,
    pub server_nonce: String,
    pub recorder_key_id: String,
    pub recorder_key_generation: u64,
    pub recorder_verifying_key_hex: String,
    pub signature_hex: String,
}

impl RecorderHandshakeAttestationV1 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, RecorderProtocolError> {
        let mut unsigned = self.clone();
        unsigned.signature_hex.clear();
        canonical_json_bytes(&unsigned)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operation", content = "payload", rename_all = "snake_case")]
pub enum RecorderRequestV1 {
    ExecuteOnce(Box<ExecuteOnceV1>),
    QueryOnly(Box<QueryOnlyV1>),
    Reconcile(Box<ReconcileV1>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecorderObservationKindV1 {
    Accepted,
    InvocationStarted,
    Present,
    Absent,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecorderObservationSourceV1 {
    Execution,
    Reconciliation,
}

/// Closed technical facts derived by the artifact-pinned provider adapter.
///
/// These are intentionally absent from caller requests. `Unknown` is a sealed
/// fallback for a definite, proven-absent provider failure that the adapter
/// cannot otherwise classify; it is not an outcome-uncertainty marker.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFailureClassV1 {
    Transient,
    RateLimited,
    Authentication,
    Permanent,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedRecorderObservationV1 {
    pub sequence: u64,
    pub record_id: String,
    pub canonical_root_identity: String,
    pub installation_id: String,
    pub state_root_generation: i64,
    pub os_user_identity_digest: String,
    pub attempt_id: String,
    pub logical_effect_id: String,
    pub command_digest: String,
    pub kind: RecorderObservationKindV1,
    pub source: RecorderObservationSourceV1,
    pub provider_identity: String,
    pub provider_version: String,
    pub provider_request_digest: String,
    pub provider_idempotency_key_digest: Option<String>,
    pub reconciliation_key_digest: Option<String>,
    pub remote_effect_id: Option<String>,
    pub response_digest: Option<String>,
    pub evidence_payload_digest: Option<String>,
    pub provider_error_class: Option<ProviderFailureClassV1>,
    #[serde(default)]
    pub technical_resource_actuals: Vec<TechnicalResourceActualV1>,
    pub reconciliation_window_start_ms: Option<i64>,
    pub reconciliation_window_end_ms: Option<i64>,
    pub observed_at_ms: i64,
    pub previous_record_digest: String,
    pub record_digest: String,
    pub recorder_key_id: String,
    pub recorder_key_generation: u64,
    pub signature_hex: String,
}

impl SignedRecorderObservationV1 {
    /// Validates the non-negotiable relation between effect outcome and an
    /// adapter-derived provider failure class. Callers cannot provide either
    /// this observation or its class; it is signed recorder evidence.
    pub fn validate_shape(&self) -> Result<(), RecorderProtocolError> {
        let class_required = self.source == RecorderObservationSourceV1::Execution
            && self.kind == RecorderObservationKindV1::Absent;
        let class_forbidden = self.source == RecorderObservationSourceV1::Reconciliation
            || matches!(
                self.kind,
                RecorderObservationKindV1::Accepted
                    | RecorderObservationKindV1::InvocationStarted
                    | RecorderObservationKindV1::Present
                    | RecorderObservationKindV1::Unknown
            );
        if (class_required && self.provider_error_class.is_none())
            || (class_forbidden && self.provider_error_class.is_some())
        {
            return Err(RecorderProtocolError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecorderReplyV1 {
    Observation {
        request_id: String,
        replayed: bool,
        observation: Box<SignedRecorderObservationV1>,
    },
    NotFound {
        request_id: String,
    },
    Rejected {
        request_id: String,
        code: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RecorderProtocolError {
    #[error("recorder frame exceeds the fixed limit")]
    FrameTooLarge,
    #[error("recorder frame is truncated")]
    TruncatedFrame,
    #[error("recorder JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("recorder protocol version is unsupported")]
    UnsupportedVersion,
    #[error("recorder request contains an invalid field")]
    InvalidField,
}

impl RecorderRequestV1 {
    pub fn binding(&self) -> &RecorderBindingV1 {
        match self {
            Self::ExecuteOnce(value) => &value.binding,
            Self::QueryOnly(value) => &value.binding,
            Self::Reconcile(value) => &value.binding,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::ExecuteOnce(value) => &value.request_id,
            Self::QueryOnly(value) => &value.request_id,
            Self::Reconcile(value) => &value.request_id,
        }
    }

    pub fn command_mac(&self) -> &str {
        match self {
            Self::ExecuteOnce(value) => &value.command_mac,
            Self::QueryOnly(value) => &value.command_mac,
            Self::Reconcile(value) => &value.command_mac,
        }
    }

    pub fn client_nonce(&self) -> &str {
        match self {
            Self::ExecuteOnce(value) => &value.client_nonce,
            Self::QueryOnly(value) => &value.client_nonce,
            Self::Reconcile(value) => &value.client_nonce,
        }
    }

    pub fn set_command_mac(&mut self, mac: String) {
        match self {
            Self::ExecuteOnce(value) => value.command_mac = mac,
            Self::QueryOnly(value) => value.command_mac = mac,
            Self::Reconcile(value) => value.command_mac = mac,
        }
    }

    pub fn authentication_bytes(&self) -> Result<Vec<u8>, RecorderProtocolError> {
        let mut unsigned = self.clone();
        unsigned.set_command_mac(String::new());
        canonical_json_bytes(&unsigned)
    }

    pub fn validate(&self) -> Result<(), RecorderProtocolError> {
        let binding = self.binding();
        if binding.protocol_version != RECORDER_PROTOCOL_VERSION
            || binding.state_root_generation <= 0
            || binding.runtime_host_generation <= 0
            || binding.runtime_fencing_token <= 0
            || !valid_text(&binding.canonical_root_identity, 256)
            || !valid_text(&binding.installation_id, 256)
            || !valid_text(&binding.os_user_identity_digest, 128)
            || !valid_text(&binding.runtime_host_instance_id, 256)
            || !valid_text(self.request_id(), 256)
        {
            return Err(RecorderProtocolError::InvalidField);
        }
        match self {
            Self::ExecuteOnce(value) => {
                let required = [
                    &value.claim_event_id,
                    &value.claim_receipt_id,
                    &value.delegation_id,
                    &value.continuation_id,
                    &value.action_id,
                    &value.logical_effect_id,
                    &value.internal_idempotency_key,
                    &value.attempt_id,
                    &value.provider_identity,
                    &value.provider_version,
                    &value.adapter_identity,
                    &value.adapter_artifact_digest,
                    &value.provider_request_digest,
                    &value.manifest_digest,
                    &value.payload_digest,
                    &value.client_nonce,
                ];
                if value.continuation_fencing_token <= 0
                    || value.attempt_number <= 0
                    || value.deadline_ms <= 0
                    || required.iter().any(|field| !valid_text(field, 4096))
                    || value
                        .provider_idempotency_key
                        .as_ref()
                        .is_some_and(|field| !valid_text(field, 4096))
                    || value
                        .reconciliation_key
                        .as_ref()
                        .is_some_and(|field| !valid_text(field, 4096))
                    || value.operand_envelope.secret_handles.iter().any(|handle| {
                        handle.version != 1
                            || [
                                &handle.backend,
                                &handle.opaque_id,
                                &handle.purpose,
                                &handle.capability_class,
                            ]
                            .iter()
                            .any(|field| !valid_text(field, 256))
                    })
                    || value
                        .derived_provider_request_digest()
                        .map_or(true, |derived| derived != value.provider_request_digest)
                {
                    return Err(RecorderProtocolError::InvalidField);
                }
            }
            Self::QueryOnly(value) => {
                if !valid_text(&value.attempt_id, 4096)
                    || !valid_text(&value.client_nonce, 4096)
                    || value
                        .expected_command_digest
                        .as_ref()
                        .is_some_and(|field| !valid_text(field, 4096))
                    || value
                        .known_journal_head
                        .as_ref()
                        .is_some_and(|field| !valid_text(field, 4096))
                {
                    return Err(RecorderProtocolError::InvalidField);
                }
            }
            Self::Reconcile(value) => {
                if !valid_text(&value.attempt_id, 4096)
                    || !valid_digest(&value.expected_command_digest)
                    || !valid_text(&value.reconciliation_key, 4096)
                    || !valid_digest(&value.reconciliation_key_digest)
                    || stable_text_digest(&value.reconciliation_key)
                        != value.reconciliation_key_digest
                    || value.consistency_window_start_ms < 0
                    || value.consistency_window_end_ms < value.consistency_window_start_ms
                    || !valid_text(&value.client_nonce, 4096)
                {
                    return Err(RecorderProtocolError::InvalidField);
                }
            }
        }
        Ok(())
    }
}

impl ExecuteOnceV1 {
    pub fn derived_provider_request_digest(&self) -> Result<String, RecorderProtocolError> {
        let mut digest = Sha256::new();
        // This is byte-for-byte the authoritative digest persisted by
        // carsinos-storage for ProviderAttemptRecord.
        digest.update(b"carsinos.execass.provider-request.v1\0");
        for part in [
            self.internal_idempotency_key.as_str(),
            self.provider_identity.as_str(),
            self.provider_idempotency_key.as_deref().unwrap_or(""),
            self.reconciliation_key.as_deref().unwrap_or(""),
            self.manifest_digest.as_str(),
            self.payload_digest.as_str(),
        ] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part.as_bytes());
        }
        if self.provider_identity == "carsinos.local-fs.exact-overwrite" {
            digest.update(b"carsinos.execass.provider-request.exact-overwrite-operand.v1\0");
            digest.update((self.payload_digest.len() as u64).to_be_bytes());
            digest.update(self.payload_digest.as_bytes());
        }
        Ok(format!("sha256:{:x}", digest.finalize()))
    }
}

pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, RecorderProtocolError> {
    let payload = canonical_json_bytes(value)?;
    if payload.len() > RECORDER_MAX_FRAME_BYTES || payload.len() > u32::MAX as usize {
        return Err(RecorderProtocolError::FrameTooLarge);
    }
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

pub fn decode_frame<'a, T: Deserialize<'a>>(frame: &'a [u8]) -> Result<T, RecorderProtocolError> {
    if frame.len() < 4 {
        return Err(RecorderProtocolError::TruncatedFrame);
    }
    let length = u32::from_be_bytes(frame[..4].try_into().expect("four-byte prefix")) as usize;
    if length > RECORDER_MAX_FRAME_BYTES {
        return Err(RecorderProtocolError::FrameTooLarge);
    }
    if frame.len() != 4 + length {
        return Err(RecorderProtocolError::TruncatedFrame);
    }
    serde_json::from_slice(&frame[4..])
        .map_err(|error| RecorderProtocolError::InvalidJson(error.to_string()))
}

pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, RecorderProtocolError> {
    let value = serde_json::to_value(value)
        .map_err(|error| RecorderProtocolError::InvalidJson(error.to_string()))?;
    serde_json::to_vec(&canonicalize(value))
        .map_err(|error| RecorderProtocolError::InvalidJson(error.to_string()))
}

pub fn recorder_observation_signing_bytes(
    observation: &SignedRecorderObservationV1,
) -> Result<Vec<u8>, RecorderProtocolError> {
    let mut unsigned = observation.clone();
    unsigned.signature_hex.clear();
    canonical_json_bytes(&unsigned)
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let ordered = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(ordered.into_iter().collect())
        }
        other => other,
    }
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn stable_text_digest(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

/// Derive the locked ExecAss state-root identity from an already-canonical
/// filesystem path string. Filesystem canonicalization remains the caller's
/// responsibility; this helper owns the cross-crate normalization/hash rule.
pub fn canonical_root_identity_from_canonical_path(path: &str) -> String {
    let mut normalized = path.replace('/', "\\");
    if cfg!(windows) {
        normalized.make_ascii_lowercase();
    }
    stable_text_digest(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RecorderRequestV1 {
        let mut request = ExecuteOnceV1 {
            binding: RecorderBindingV1 {
                protocol_version: RECORDER_PROTOCOL_VERSION.into(),
                canonical_root_identity: format!("sha256:{}", "a".repeat(64)),
                installation_id: "installation-1".into(),
                state_root_generation: 1,
                os_user_identity_digest: "b".repeat(64),
                runtime_host_generation: 2,
                runtime_host_instance_id: "host-1".into(),
                runtime_fencing_token: 3,
            },
            request_id: "request-1".into(),
            claim_event_id: "claim-event-1".into(),
            claim_receipt_id: "claim-receipt-1".into(),
            continuation_fencing_token: 4,
            delegation_id: "delegation-1".into(),
            continuation_id: "continuation-1".into(),
            action_id: "action-1".into(),
            logical_effect_id: "effect-1".into(),
            internal_idempotency_key: "internal-key-1".into(),
            attempt_id: "attempt-1".into(),
            attempt_number: 1,
            provider_identity: "fake-provider".into(),
            provider_version: "v1".into(),
            adapter_identity: "ea213.fake-provider.v1".into(),
            adapter_artifact_digest: format!("sha256:{}", "c".repeat(64)),
            provider_request_digest: String::new(),
            provider_idempotency_key: Some("provider-key-1".into()),
            reconciliation_key: Some("reconcile-key-1".into()),
            manifest_digest: "manifest-1".into(),
            payload_digest: "payload-1".into(),
            operand_envelope: OpaqueOperandEnvelopeV1 {
                non_secret: serde_json::json!({"z": 2, "a": 1}),
                secret_handles: vec![],
            },
            deadline_ms: 9_999_999_999,
            client_nonce: "nonce-1".into(),
            command_mac: "mac".into(),
        };
        request.provider_request_digest = request.derived_provider_request_digest().unwrap();
        RecorderRequestV1::ExecuteOnce(Box::new(request))
    }

    #[test]
    fn hostile_roundtrip_is_canonical_and_exact() {
        let request = request();
        request.validate().unwrap();
        let frame = encode_frame(&request).unwrap();
        let decoded: RecorderRequestV1 = decode_frame(&frame).unwrap();
        assert_eq!(decoded, request);
        assert_eq!(encode_frame(&decoded).unwrap(), frame);
    }

    #[test]
    fn mutation_changes_authentication_bytes() {
        let original = request();
        let mut mutated = original.clone();
        if let RecorderRequestV1::ExecuteOnce(value) = &mut mutated {
            value.payload_digest.push('x');
        }
        assert_ne!(
            original.authentication_bytes().unwrap(),
            mutated.authentication_bytes().unwrap()
        );
    }

    #[test]
    fn unknown_field_is_rejected() {
        let mut value = serde_json::to_value(request()).unwrap();
        value["payload"]["caller_claimed_absent"] = Value::Bool(true);
        let payload = serde_json::to_vec(&value).unwrap();
        let mut frame = Vec::new();
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        assert!(matches!(
            decode_frame::<RecorderRequestV1>(&frame),
            Err(RecorderProtocolError::InvalidJson(_))
        ));
    }

    #[test]
    fn oversized_and_truncated_frames_fail_closed() {
        let mut oversized = Vec::new();
        oversized.extend_from_slice(&((RECORDER_MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
        assert_eq!(
            decode_frame::<RecorderRequestV1>(&oversized),
            Err(RecorderProtocolError::FrameTooLarge)
        );
        assert_eq!(
            decode_frame::<RecorderRequestV1>(&[0, 0, 0, 4, b'{']),
            Err(RecorderProtocolError::TruncatedFrame)
        );
    }

    #[test]
    fn unsupported_version_and_invalid_secret_handle_fail_validation() {
        let mut bad_version = request();
        match &mut bad_version {
            RecorderRequestV1::ExecuteOnce(value) => value.binding.protocol_version = "v2".into(),
            RecorderRequestV1::QueryOnly(_) | RecorderRequestV1::Reconcile(_) => unreachable!(),
        }
        assert_eq!(
            bad_version.validate(),
            Err(RecorderProtocolError::InvalidField)
        );

        let mut bad_handle = request();
        match &mut bad_handle {
            RecorderRequestV1::ExecuteOnce(value) => {
                value
                    .operand_envelope
                    .secret_handles
                    .push(OpaqueSecretHandleV1 {
                        version: 1,
                        backend: "keyring".into(),
                        opaque_id: "secret\nvalue".into(),
                        purpose: "delivery".into(),
                        capability_class: "provider".into(),
                    })
            }
            RecorderRequestV1::QueryOnly(_) | RecorderRequestV1::Reconcile(_) => unreachable!(),
        }
        assert_eq!(
            bad_handle.validate(),
            Err(RecorderProtocolError::InvalidField)
        );
    }

    #[test]
    fn provider_request_digest_matches_authoritative_storage_algorithm() {
        let RecorderRequestV1::ExecuteOnce(original) = request() else {
            unreachable!()
        };
        let original_digest = original.derived_provider_request_digest().unwrap();
        assert_eq!(
            original_digest,
            "sha256:58cfba82b56b6f65e18df2500364c643bfb56536beacbb885ee52373867da57f"
        );
        type Mutation = Box<dyn Fn(&mut ExecuteOnceV1)>;
        let mutations: Vec<Mutation> = vec![
            Box::new(|value| value.internal_idempotency_key.push_str("-mutated")),
            Box::new(|value| value.provider_identity.push_str("-mutated")),
            Box::new(|value| value.provider_idempotency_key = Some("changed".into())),
            Box::new(|value| value.reconciliation_key = Some("changed".into())),
            Box::new(|value| value.manifest_digest.push_str("-mutated")),
            Box::new(|value| value.payload_digest.push_str("-mutated")),
        ];
        for mutate in mutations {
            let mut changed = original.clone();
            mutate(&mut changed);
            assert_ne!(
                changed.derived_provider_request_digest().unwrap(),
                original_digest
            );
        }

        let mut non_persisted_surface = original.clone();
        non_persisted_surface.operand_envelope.non_secret["a"] = Value::from(99);
        assert_eq!(
            non_persisted_surface
                .derived_provider_request_digest()
                .unwrap(),
            original_digest
        );
        assert_ne!(
            RecorderRequestV1::ExecuteOnce(non_persisted_surface)
                .authentication_bytes()
                .unwrap(),
            RecorderRequestV1::ExecuteOnce(original)
                .authentication_bytes()
                .unwrap()
        );
    }

    #[test]
    fn reconcile_is_closed_and_rejects_invalid_windows_or_caller_actuals() {
        let RecorderRequestV1::ExecuteOnce(execute) = request() else {
            unreachable!()
        };
        let key = execute.reconciliation_key.clone().unwrap();
        let reconcile = RecorderRequestV1::Reconcile(Box::new(ReconcileV1 {
            binding: execute.binding.clone(),
            request_id: "reconcile-1".into(),
            attempt_id: execute.attempt_id.clone(),
            expected_command_digest: format!("sha256:{}", "d".repeat(64)),
            reconciliation_key_digest: stable_text_digest(&key),
            reconciliation_key: key,
            consistency_window_start_ms: 10,
            consistency_window_end_ms: 20,
            client_nonce: "reconcile-nonce".into(),
            command_mac: "mac".into(),
        }));
        reconcile.validate().unwrap();

        let mut backwards = reconcile.clone();
        if let RecorderRequestV1::Reconcile(value) = &mut backwards {
            value.consistency_window_start_ms = 21;
        }
        assert_eq!(
            backwards.validate(),
            Err(RecorderProtocolError::InvalidField)
        );

        let mut caller_actuals = serde_json::to_value(&reconcile).unwrap();
        caller_actuals["payload"]["technical_resource_actuals"] = serde_json::json!([]);
        let payload = serde_json::to_vec(&caller_actuals).unwrap();
        let mut frame = Vec::with_capacity(payload.len() + 4);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        assert!(decode_frame::<RecorderRequestV1>(&frame).is_err());
    }

    #[test]
    fn every_reconciliation_evidence_field_is_signature_bound() {
        let observation = SignedRecorderObservationV1 {
            sequence: 7,
            record_id: "record-7".into(),
            canonical_root_identity: format!("sha256:{}", "a".repeat(64)),
            installation_id: "installation-1".into(),
            state_root_generation: 2,
            os_user_identity_digest: "b".repeat(64),
            attempt_id: "attempt-1".into(),
            logical_effect_id: "effect-1".into(),
            command_digest: format!("sha256:{}", "c".repeat(64)),
            kind: RecorderObservationKindV1::Present,
            source: RecorderObservationSourceV1::Reconciliation,
            provider_identity: "fake-provider".into(),
            provider_version: "v1".into(),
            provider_request_digest: format!("sha256:{}", "d".repeat(64)),
            provider_idempotency_key_digest: Some(format!("sha256:{}", "e".repeat(64))),
            reconciliation_key_digest: Some(format!("sha256:{}", "f".repeat(64))),
            remote_effect_id: Some("remote-1".into()),
            response_digest: Some(format!("sha256:{}", "1".repeat(64))),
            evidence_payload_digest: Some(format!("sha256:{}", "2".repeat(64))),
            provider_error_class: None,
            technical_resource_actuals: vec![TechnicalResourceActualV1 {
                reservation_id: "reservation-1".into(),
                amount_actual: 3,
                evidence_digest: format!("sha256:{}", "3".repeat(64)),
            }],
            reconciliation_window_start_ms: Some(10),
            reconciliation_window_end_ms: Some(20),
            observed_at_ms: 21,
            previous_record_digest: format!("sha256:{}", "4".repeat(64)),
            record_digest: format!("sha256:{}", "5".repeat(64)),
            recorder_key_id: "key-1".into(),
            recorder_key_generation: 1,
            signature_hex: "signature".into(),
        };
        let original = recorder_observation_signing_bytes(&observation).unwrap();
        type Mutation = Box<dyn Fn(&mut SignedRecorderObservationV1)>;
        let mutations: Vec<Mutation> = vec![
            Box::new(|value| value.canonical_root_identity.push('x')),
            Box::new(|value| value.os_user_identity_digest.push('x')),
            Box::new(|value| value.source = RecorderObservationSourceV1::Execution),
            Box::new(|value| value.reconciliation_window_end_ms = Some(22)),
            Box::new(|value| value.technical_resource_actuals[0].amount_actual = 4),
            Box::new(|value| value.reconciliation_key_digest = None),
            Box::new(|value| value.evidence_payload_digest = None),
            Box::new(|value| value.provider_error_class = Some(ProviderFailureClassV1::Permanent)),
        ];
        for mutate in mutations {
            let mut changed = observation.clone();
            mutate(&mut changed);
            assert_ne!(
                recorder_observation_signing_bytes(&changed).unwrap(),
                original
            );
        }
    }

    #[test]
    fn execute_request_rejects_provider_error_class_injection() {
        let mut value = serde_json::to_value(request()).unwrap();
        value["payload"]["provider_error_class"] = serde_json::json!("permanent");
        let payload = serde_json::to_vec(&value).unwrap();
        let mut frame = Vec::with_capacity(payload.len() + 4);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        assert!(matches!(
            decode_frame::<RecorderRequestV1>(&frame),
            Err(RecorderProtocolError::InvalidJson(_))
        ));
    }

    #[test]
    fn provider_error_class_has_closed_outcome_shape() {
        let mut observation = SignedRecorderObservationV1 {
            sequence: 1,
            record_id: "record-1".into(),
            canonical_root_identity: "root".into(),
            installation_id: "installation".into(),
            state_root_generation: 1,
            os_user_identity_digest: "user".into(),
            attempt_id: "attempt".into(),
            logical_effect_id: "effect".into(),
            command_digest: "command".into(),
            kind: RecorderObservationKindV1::Absent,
            source: RecorderObservationSourceV1::Execution,
            provider_identity: "provider".into(),
            provider_version: "v1".into(),
            provider_request_digest: "request".into(),
            provider_idempotency_key_digest: None,
            reconciliation_key_digest: None,
            remote_effect_id: None,
            response_digest: Some("response".into()),
            evidence_payload_digest: Some("evidence".into()),
            provider_error_class: Some(ProviderFailureClassV1::Authentication),
            technical_resource_actuals: Vec::new(),
            reconciliation_window_start_ms: None,
            reconciliation_window_end_ms: None,
            observed_at_ms: 1,
            previous_record_digest: "previous".into(),
            record_digest: "digest".into(),
            recorder_key_id: "key".into(),
            recorder_key_generation: 1,
            signature_hex: "signature".into(),
        };
        assert!(observation.validate_shape().is_ok());

        observation.provider_error_class = None;
        assert_eq!(
            observation.validate_shape(),
            Err(RecorderProtocolError::InvalidField)
        );

        observation.kind = RecorderObservationKindV1::Present;
        observation.provider_error_class = Some(ProviderFailureClassV1::Transient);
        assert_eq!(
            observation.validate_shape(),
            Err(RecorderProtocolError::InvalidField)
        );

        observation.kind = RecorderObservationKindV1::Unknown;
        assert_eq!(
            observation.validate_shape(),
            Err(RecorderProtocolError::InvalidField)
        );

        observation.kind = RecorderObservationKindV1::Absent;
        observation.source = RecorderObservationSourceV1::Reconciliation;
        assert_eq!(
            observation.validate_shape(),
            Err(RecorderProtocolError::InvalidField)
        );
    }

    #[test]
    fn canonical_root_identity_normalizes_separator_aliases() {
        let forward = canonical_root_identity_from_canonical_path("Z:/State/Root");
        let backward = canonical_root_identity_from_canonical_path("Z:\\State\\Root");
        assert_eq!(forward, backward);
        assert_ne!(
            forward,
            canonical_root_identity_from_canonical_path("Z:\\State\\Other")
        );
    }
}
