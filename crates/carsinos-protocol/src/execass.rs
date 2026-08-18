//! Strict, versioned data-transfer objects for the ExecAss v1 API and outbox.
//!
//! These types deliberately model the wire contract only. State transitions,
//! authority enforcement, and receipt verification remain owned by their
//! respective services.

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::OnceLock;

const RUN_CONTROL_ATTESTATION_DOMAIN: &[u8] = b"carsinos.execass.run_control_attestation.v1";
const RUN_CONTROL_REQUEST_BINDING_DOMAIN: &[u8] = b"carsinos.execass.run_control_binding.v1";
const LOCAL_RUN_CONTROL_REQUEST_PROOF_DOMAIN: &[u8] =
    b"carsinos.execass.local_run_control_proof.v1";
const LOCAL_OWNER_INTAKE_PROOF_DOMAIN: &[u8] = b"carsinos.execass.local_owner_intake_proof.v1";
const LOCAL_OWNER_MUTATION_PROOF_DOMAIN: &[u8] = b"carsinos.execass.local_owner_mutation_proof.v1";
const LOCAL_OWNER_DECISION_PROOF_DOMAIN: &[u8] = b"carsinos.execass.local_owner_proof.v1";
const NORMALIZED_OWNER_INTENT_DIGEST_DOMAIN: &[u8] = b"carsinos.execass.normalized_intent.v1";
const OWNER_INSTRUCTION_DIGEST_DOMAIN: &[u8] = b"carsinos.execass.owner_instruction.v1";

/// Redact built-in secret-shaped text before it can be bound into an ExecAss
/// owner-intent digest. Callers with a registered secret inventory must apply
/// their inventory after this common pass.
pub fn redact_execass_builtin_secret_patterns(raw: &str) -> String {
    let mut output = raw.to_owned();
    for expression in execass_builtin_secret_patterns() {
        output = expression.replace_all(&output, "[REDACTED]").into_owned();
    }
    output
}

#[cfg(test)]
mod owner_mutation_proof_tests {
    use super::*;

    fn binding() -> LocalOwnerMutationBinding {
        LocalOwnerMutationBinding {
            operation: OwnerMutationOperation::PolicyUpdate,
            method: "PUT".into(),
            path: "/api/v1/execass/policy".into(),
            request_correlation_id: "correlation-1".into(),
            idempotency_key: "idempotency-1".into(),
            expected_revision: 1,
            canonical_body_digest: "11".repeat(32),
            safe_snapshot_digest: "22".repeat(32),
            created_at_ms: 1_700_000_000_000,
        }
    }

    fn proof(client: &str) -> LocalOwnerMutationProof {
        LocalOwnerMutationProof {
            authenticated_client_id: client.into(),
            request_correlation_id: "correlation-1".into(),
            proof_hex: "33".repeat(32),
        }
    }

    #[test]
    fn owner_mutation_proof_binds_real_client_and_every_exact_request_fact() {
        let original = binding();
        let expected = local_owner_mutation_proof_bytes(&proof("desktop-1"), &original).unwrap();
        assert_ne!(
            expected,
            local_owner_mutation_proof_bytes(&proof("desktop-2"), &original).unwrap()
        );
        for mutation in 0..9 {
            let mut changed = original.clone();
            match mutation {
                0 => changed.operation = OwnerMutationOperation::RuntimeHostConfigUpdate,
                1 => changed.method = "POST".into(),
                2 => changed.path = "/api/v1/execass/runtime-host".into(),
                3 => changed.request_correlation_id = "correlation-2".into(),
                4 => changed.idempotency_key = "idempotency-2".into(),
                5 => changed.expected_revision += 1,
                6 => changed.canonical_body_digest = "44".repeat(32),
                7 => changed.safe_snapshot_digest = "55".repeat(32),
                _ => changed.created_at_ms += 1,
            }
            match local_owner_mutation_proof_bytes(&proof("desktop-1"), &changed) {
                Ok(bytes) => assert_ne!(expected, bytes, "mutation {mutation}"),
                Err(_) => assert!(mutation <= 2),
            }
        }
    }

    #[test]
    fn owner_mutation_binding_is_operation_closed_and_wire_strict() {
        let mut wrong = binding();
        wrong.path = "/api/v1/execass/runtime-host".into();
        assert!(wrong.validate().is_err());
        let mut value = serde_json::to_value(proof("desktop-1")).unwrap();
        value["role"] = serde_json::Value::String("owner".into());
        assert!(serde_json::from_value::<LocalOwnerMutationProof>(value).is_err());
    }
}

/// Stable lowercase digest for a safe, normalized owner intent. The caller is
/// responsible for applying the shared redaction transform before this digest
/// is used as an authority binding.
pub fn normalized_owner_intent_digest(normalized_intent: &str) -> Option<String> {
    if normalized_intent.trim().is_empty() {
        return None;
    }
    let mut digest = Sha256::new();
    for bytes in [
        NORMALIZED_OWNER_INTENT_DIGEST_DOMAIN,
        normalized_intent.as_bytes(),
    ] {
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Some(format!("{:x}", digest.finalize()))
}

/// Stable lowercase digest for one exact owner instruction byte sequence.
///
/// Unlike the safe normalized-intent digest, this binding is computed before
/// redaction or normalization. The digest is safe to retain; the instruction
/// bytes themselves must remain transient unless another contract explicitly
/// permits their storage.
pub fn owner_instruction_digest(instruction_bytes: &[u8]) -> Option<String> {
    if instruction_bytes.is_empty() {
        return None;
    }
    let mut digest = Sha256::new();
    for bytes in [OWNER_INSTRUCTION_DIGEST_DOMAIN, instruction_bytes] {
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Some(format!("{:x}", digest.finalize()))
}

fn execass_builtin_secret_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            Regex::new(r"(?i)\b(?:bearer|basic)\s+[A-Za-z0-9+/=_\-.:%]{6,}").unwrap(),
            Regex::new(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").unwrap(),
            Regex::new(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----").unwrap(),
            Regex::new(r"\b(?:sk|rk|pk|ghp|github_pat|xox[baprs]|AIza)-?[A-Za-z0-9_-]{6,}\b").unwrap(),
            Regex::new(r"(?i)(?:[?&#]|\b)(?:token|access_token|refresh_token|id_token|api_key|apikey|client_secret|password|oauth_code|auth_code|code_verifier)=[^&#\s]+").unwrap(),
            Regex::new(r"(?i)\b(?:authorization|x-api-key|token|api_key|client_secret|password)\s*[:=]\s*[^\s,;]+").unwrap(),
            Regex::new(r"(?i)secret://[^\s,;]+").unwrap(),
        ]
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunControlOperation {
    GlobalStop,
    GlobalResume,
    DelegationStop,
    DelegationResume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunControlTarget {
    Global,
    Delegation { delegation_id: String },
}

/// Exact server-provided state disclosed to a human before a resume request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunControlResumeSnapshot {
    pub stopped_epoch: i64,
    pub current_policy_revision: i64,
    pub unresolved_effect_disclosure_digest: String,
    pub delegation_state_revision: Option<i64>,
    pub current_plan_revision: Option<i64>,
}

impl RunControlResumeSnapshot {
    pub fn new(
        stopped_epoch: i64,
        current_policy_revision: i64,
        unresolved_effect_disclosure_digest: String,
        delegation_state_revision: Option<i64>,
        current_plan_revision: Option<i64>,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        let snapshot = Self {
            stopped_epoch,
            current_policy_revision,
            unresolved_effect_disclosure_digest,
            delegation_state_revision,
            current_plan_revision,
        };
        validate_run_control_resume_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub fn stopped_epoch(&self) -> i64 {
        self.stopped_epoch
    }

    pub fn current_policy_revision(&self) -> i64 {
        self.current_policy_revision
    }

    pub fn unresolved_effect_disclosure_digest(&self) -> &str {
        &self.unresolved_effect_disclosure_digest
    }

    pub fn delegation_state_revision(&self) -> Option<i64> {
        self.delegation_state_revision
    }

    pub fn current_plan_revision(&self) -> Option<i64> {
        self.current_plan_revision
    }
}

/// Canonical local/remote request facts that may be selected by the desktop
/// caller. This intentionally excludes every field minted later by the fixed
/// Ed25519 confirmation authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunControlRequestBinding {
    pub operation: RunControlOperation,
    pub target: RunControlTarget,
    pub idempotency_key: String,
    pub request_correlation_id: String,
    pub observed_at_ms: i64,
    pub resume: Option<RunControlResumeSnapshot>,
}

impl RunControlRequestBinding {
    pub fn global_stop(
        idempotency_key: String,
        request_correlation_id: String,
        observed_at_ms: i64,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        Self::new(
            RunControlOperation::GlobalStop,
            RunControlTarget::Global,
            idempotency_key,
            request_correlation_id,
            observed_at_ms,
            None,
        )
    }

    pub fn global_resume(
        idempotency_key: String,
        request_correlation_id: String,
        observed_at_ms: i64,
        resume: RunControlResumeSnapshot,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        Self::new(
            RunControlOperation::GlobalResume,
            RunControlTarget::Global,
            idempotency_key,
            request_correlation_id,
            observed_at_ms,
            Some(resume),
        )
    }

    pub fn delegation_stop(
        delegation_id: String,
        idempotency_key: String,
        request_correlation_id: String,
        observed_at_ms: i64,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        Self::new(
            RunControlOperation::DelegationStop,
            RunControlTarget::Delegation { delegation_id },
            idempotency_key,
            request_correlation_id,
            observed_at_ms,
            None,
        )
    }

    pub fn delegation_resume(
        delegation_id: String,
        idempotency_key: String,
        request_correlation_id: String,
        observed_at_ms: i64,
        resume: RunControlResumeSnapshot,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        Self::new(
            RunControlOperation::DelegationResume,
            RunControlTarget::Delegation { delegation_id },
            idempotency_key,
            request_correlation_id,
            observed_at_ms,
            Some(resume),
        )
    }

    pub fn new(
        operation: RunControlOperation,
        target: RunControlTarget,
        idempotency_key: String,
        request_correlation_id: String,
        observed_at_ms: i64,
        resume: Option<RunControlResumeSnapshot>,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        let binding = Self {
            operation,
            target,
            idempotency_key,
            request_correlation_id,
            observed_at_ms,
            resume,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), RunControlAttestationEncodingError> {
        validate_run_control_request_binding(self)
    }

    pub fn operation(&self) -> RunControlOperation {
        self.operation
    }

    pub fn target(&self) -> &RunControlTarget {
        &self.target
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn request_correlation_id(&self) -> &str {
        &self.request_correlation_id
    }

    pub fn observed_at_ms(&self) -> i64 {
        self.observed_at_ms
    }

    pub fn resume(&self) -> Option<&RunControlResumeSnapshot> {
        self.resume.as_ref()
    }

    /// Stable lowercase SHA-256 of the validated canonical request bytes.
    pub fn try_request_binding_digest(&self) -> Result<String, RunControlAttestationEncodingError> {
        Ok(format!(
            "{:x}",
            Sha256::digest(run_control_request_binding_bytes(self)?)
        ))
    }

    /// Compatibility for constructor-only gateway test fixtures. Production
    /// builds do not expose an infallible digest API, and invalid test values
    /// return an unusable empty digest instead of panicking.
    #[cfg(feature = "execass-test-binding-digest")]
    #[doc(hidden)]
    pub fn request_binding_digest(&self) -> String {
        self.try_request_binding_digest().unwrap_or_default()
    }
}

/// HMAC returned by the native shell. It contains no owner secret and cannot
/// claim a runtime, connector, model, worker, or remote actor identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalRunControlProof {
    pub authenticated_client_id: String,
    pub request_correlation_id: String,
    pub proof_hex: String,
}

impl LocalRunControlProof {
    pub fn from_authenticated_native_request(
        authenticated_client_id: String,
        request_correlation_id: String,
        proof_hex: String,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        let proof = Self {
            authenticated_client_id,
            request_correlation_id,
            proof_hex,
        };
        proof.validate()?;
        Ok(proof)
    }

    pub fn validate(&self) -> Result<(), RunControlAttestationEncodingError> {
        if !is_bounded_safe_text(&self.authenticated_client_id, 128)
            || !is_bounded_safe_text(&self.request_correlation_id, 128)
            || !is_lower_hex(&self.proof_hex, 64)
        {
            Err(RunControlAttestationEncodingError::InvalidField(
                "local_proof",
            ))
        } else {
            Ok(())
        }
    }
}

/// HMAC returned by the native shell for one exact owner-intake request.
///
/// The normalized intent digest is the safe, server-comparable representation
/// of the request text. The instruction digest separately binds the exact raw
/// bytes without carrying them. This proof deliberately carries no actor
/// selection, category, purpose, or financial authority fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalOwnerIntakeProof {
    pub authenticated_client_id: String,
    pub request_correlation_id: String,
    pub request_id: String,
    pub idempotency_key: String,
    pub attach_to_delegation_id: Option<String>,
    pub normalized_intent_digest: String,
    pub instruction_digest: String,
    pub proof_hex: String,
}

/// HMAC returned by the native shell for one exact decision response.
///
/// This is authentication evidence only. It does not carry actor selection,
/// purpose, category, commerce, tenant, role, or delegated-authority facts.
/// The proof authenticates the separate server-derived
/// [`LocalDecisionProofBinding`] encoded by [`local_decision_proof_bytes`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDecisionProof {
    pub authenticated_client_id: String,
    pub request_correlation_id: String,
    pub proof_hex: String,
}

/// Exact current decision and requested response facts authenticated by a
/// [`LocalDecisionProof`]. Callers must derive the current fields from
/// authoritative server state, never from bearer/JWT claims or request input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDecisionProofBinding {
    pub decision_id: String,
    pub decision_revision: u64,
    pub normalized_intent_digest: String,
    pub policy_revision: i64,
    pub canonical_manifest_digest: String,
    pub selected_logical_action_id: String,
    pub presented_action_digest: String,
    pub declared_consequence_digest: String,
    pub challenge_digest: String,
    pub expires_at_ms: i64,
    pub response_selected_logical_action_id: String,
    pub decision_result: DecisionResult,
    pub idempotency_key: String,
    pub revision_text_digest: Option<String>,
    pub challenge_response_digest: Option<String>,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OwnerMutationOperation {
    PolicyUpdate,
    RuntimeHostConfigUpdate,
}

impl OwnerMutationOperation {
    pub const fn method(self) -> &'static str {
        "PUT"
    }

    pub const fn path(self) -> &'static str {
        match self {
            Self::PolicyUpdate => "/api/v1/execass/policy",
            Self::RuntimeHostConfigUpdate => "/api/v1/execass/runtime-host",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::PolicyUpdate => "execass.policy.update",
            Self::RuntimeHostConfigUpdate => "execass.runtime_host.update",
        }
    }
}

/// Native-shell authentication for one exact owner settings mutation.
/// Bearer identity, roles, authority kinds, and actor claims are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalOwnerMutationProof {
    pub authenticated_client_id: String,
    pub request_correlation_id: String,
    pub proof_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalOwnerMutationBinding {
    pub operation: OwnerMutationOperation,
    pub method: String,
    pub path: String,
    pub request_correlation_id: String,
    pub idempotency_key: String,
    pub expected_revision: i64,
    pub canonical_body_digest: String,
    pub safe_snapshot_digest: String,
    pub created_at_ms: i64,
}

impl LocalOwnerMutationBinding {
    pub fn validate(&self) -> Result<(), RunControlAttestationEncodingError> {
        if self.method != self.operation.method()
            || self.path != self.operation.path()
            || !is_bounded_safe_text(&self.request_correlation_id, 128)
            || !is_bounded_safe_text(&self.idempotency_key, 128)
            || self.expected_revision < 0
            || !is_lower_hex(&self.canonical_body_digest, 64)
            || !is_lower_hex(&self.safe_snapshot_digest, 64)
            || self.created_at_ms <= 0
        {
            Err(RunControlAttestationEncodingError::InvalidField(
                "local_owner_mutation_binding",
            ))
        } else {
            Ok(())
        }
    }
}

impl LocalOwnerMutationProof {
    pub fn validate(&self) -> Result<(), RunControlAttestationEncodingError> {
        if !is_bounded_safe_text(&self.authenticated_client_id, 128)
            || !is_bounded_safe_text(&self.request_correlation_id, 128)
            || !is_lower_hex(&self.proof_hex, 64)
        {
            Err(RunControlAttestationEncodingError::InvalidField(
                "local_owner_mutation_proof",
            ))
        } else {
            Ok(())
        }
    }
}

/// Server-derived current decision facts a native owner signer needs before
/// it can add one exact response and authenticate the resulting binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionProofChallenge {
    pub decision_id: String,
    pub decision_revision: u64,
    pub normalized_intent_digest: String,
    pub policy_revision: i64,
    pub canonical_manifest_digest: String,
    pub selected_logical_action_id: String,
    pub presented_action_digest: String,
    pub declared_consequence_digest: String,
    pub challenge_digest: String,
    pub expires_at_ms: i64,
}

impl LocalOwnerIntakeProof {
    #[allow(clippy::too_many_arguments)]
    pub fn from_authenticated_native_request(
        authenticated_client_id: String,
        request_correlation_id: String,
        request_id: String,
        idempotency_key: String,
        attach_to_delegation_id: Option<String>,
        normalized_intent_digest: String,
        instruction_digest: String,
        proof_hex: String,
    ) -> Result<Self, RunControlAttestationEncodingError> {
        let proof = Self {
            authenticated_client_id,
            request_correlation_id,
            request_id,
            idempotency_key,
            attach_to_delegation_id,
            normalized_intent_digest,
            instruction_digest,
            proof_hex,
        };
        proof.validate()?;
        Ok(proof)
    }

    /// Validate all fields accepted from the native shell, including the
    /// lower-hex HMAC representation.
    pub fn validate(&self) -> Result<(), RunControlAttestationEncodingError> {
        self.validate_binding()?;
        if !is_lower_hex(&self.proof_hex, 64) {
            return Err(RunControlAttestationEncodingError::InvalidField(
                "local_owner_intake_proof",
            ));
        }
        Ok(())
    }

    fn validate_binding(&self) -> Result<(), RunControlAttestationEncodingError> {
        if !is_bounded_safe_text(&self.authenticated_client_id, 128)
            || !is_bounded_safe_text(&self.request_correlation_id, 128)
            || !is_bounded_safe_text(&self.request_id, 128)
            || !is_bounded_safe_text(&self.idempotency_key, 128)
            || self
                .attach_to_delegation_id
                .as_deref()
                .is_some_and(|value| !is_bounded_safe_text(value, 256))
            || !is_lower_hex(&self.normalized_intent_digest, 64)
            || !is_lower_hex(&self.instruction_digest, 64)
        {
            return Err(RunControlAttestationEncodingError::InvalidField(
                "local_owner_intake_binding",
            ));
        }
        Ok(())
    }
}

/// Signed, transport-neutral proof for one exact run-control transition.
/// The fixed confirmation authority key signs `run_control_attestation_signing_bytes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunControlAttestationPayload {
    pub actor_type: ActorType,
    pub credential_identity: String,
    pub authenticated_ingress: String,
    pub channel_assurance: String,
    pub request_correlation_id: String,
    pub source_message_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub operation: RunControlOperation,
    pub target: RunControlTarget,
    pub idempotency_key: String,
    pub replay_identity: String,
    pub observed_at_ms: i64,
    pub issued_at_ms: i64,
    pub stopped_epoch: i64,
    pub policy_revision: i64,
    pub unresolved_effect_disclosure_digest: String,
    pub delegation_state_revision: Option<i64>,
    pub current_plan_revision: Option<i64>,
    pub canonical_root_identity: String,
    pub installation_identity: String,
    pub os_user_identity_digest: String,
    pub state_root_generation: i64,
    pub signer_key_generation: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunControlAttestation {
    pub payload: RunControlAttestationPayload,
    pub key_id: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunControlAttestationEncodingError {
    InvalidField(&'static str),
}
impl fmt::Display for RunControlAttestationEncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => {
                write!(f, "invalid run-control attestation field: {field}")
            }
        }
    }
}
impl std::error::Error for RunControlAttestationEncodingError {}

/// The only canonical bytes signed by the fixed confirmation authority.
pub fn run_control_attestation_signing_bytes(
    payload: &RunControlAttestationPayload,
    key_id: &str,
) -> Result<Vec<u8>, RunControlAttestationEncodingError> {
    validate_run_control_payload(payload, key_id)?;
    let mut out = Vec::with_capacity(768);
    push_run_control_bytes(&mut out, RUN_CONTROL_ATTESTATION_DOMAIN);
    push_run_control_text(&mut out, actor_type_wire(payload.actor_type));
    for value in [
        &payload.credential_identity,
        &payload.authenticated_ingress,
        &payload.channel_assurance,
        &payload.request_correlation_id,
    ] {
        push_run_control_text(&mut out, value);
    }
    push_run_control_optional_text(&mut out, payload.source_message_id.as_deref());
    push_run_control_optional_text(&mut out, payload.provider_event_id.as_deref());
    push_run_control_text(&mut out, operation_wire(payload.operation));
    match &payload.target {
        RunControlTarget::Global => {
            out.push(0);
        }
        RunControlTarget::Delegation { delegation_id } => {
            out.push(1);
            push_run_control_text(&mut out, delegation_id);
        }
    }
    for value in [&payload.idempotency_key, &payload.replay_identity] {
        push_run_control_text(&mut out, value);
    }
    for value in [
        payload.observed_at_ms,
        payload.issued_at_ms,
        payload.stopped_epoch,
        payload.policy_revision,
    ] {
        push_run_control_i64(&mut out, value);
    }
    push_run_control_text(&mut out, &payload.unresolved_effect_disclosure_digest);
    push_run_control_optional_i64(&mut out, payload.delegation_state_revision);
    push_run_control_optional_i64(&mut out, payload.current_plan_revision);
    for value in [
        &payload.canonical_root_identity,
        &payload.installation_identity,
        &payload.os_user_identity_digest,
    ] {
        push_run_control_text(&mut out, value);
    }
    push_run_control_i64(&mut out, payload.state_root_generation);
    push_run_control_i64(&mut out, payload.signer_key_generation);
    push_run_control_text(&mut out, key_id);
    Ok(out)
}

/// Canonical request bytes authenticated by the native-shell HMAC. The domain
/// is separate from the later fixed-key Ed25519 attestation domain.
pub fn local_run_control_request_proof_bytes(
    authenticated_client_id: &str,
    binding: &RunControlRequestBinding,
) -> Result<Vec<u8>, RunControlAttestationEncodingError> {
    if !is_bounded_safe_text(authenticated_client_id, 128) {
        return Err(RunControlAttestationEncodingError::InvalidField(
            "local_proof",
        ));
    }
    let request_correlation_id = binding.request_correlation_id.clone();
    let binding_bytes = run_control_request_binding_bytes(binding)?;
    let mut out = Vec::with_capacity(binding_bytes.len() + 128);
    push_run_control_bytes(&mut out, LOCAL_RUN_CONTROL_REQUEST_PROOF_DOMAIN);
    push_run_control_text(&mut out, authenticated_client_id);
    push_run_control_text(&mut out, &request_correlation_id);
    push_run_control_bytes(&mut out, &binding_bytes);
    Ok(out)
}

/// Canonical request bytes authenticated by the native-shell HMAC for owner
/// intake. The proof hex itself is intentionally excluded: it is the MAC over
/// these bytes. This has a distinct domain from every control and attestation
/// proof format.
pub fn local_owner_intake_proof_bytes(
    proof: &LocalOwnerIntakeProof,
) -> Result<Vec<u8>, RunControlAttestationEncodingError> {
    proof.validate_binding()?;
    let mut out = Vec::with_capacity(512);
    push_run_control_bytes(&mut out, LOCAL_OWNER_INTAKE_PROOF_DOMAIN);
    for value in [
        &proof.authenticated_client_id,
        &proof.request_correlation_id,
        &proof.request_id,
        &proof.idempotency_key,
        &proof.normalized_intent_digest,
        &proof.instruction_digest,
    ] {
        push_run_control_text(&mut out, value);
    }
    push_run_control_optional_text(&mut out, proof.attach_to_delegation_id.as_deref());
    Ok(out)
}

pub fn local_owner_mutation_proof_bytes(
    proof: &LocalOwnerMutationProof,
    binding: &LocalOwnerMutationBinding,
) -> Result<Vec<u8>, RunControlAttestationEncodingError> {
    proof.validate()?;
    let binding_bytes = local_owner_mutation_binding_bytes(binding)?;
    let mut out = Vec::with_capacity(512);
    push_run_control_bytes(&mut out, LOCAL_OWNER_MUTATION_PROOF_DOMAIN);
    push_run_control_text(&mut out, &proof.authenticated_client_id);
    push_run_control_text(&mut out, &proof.request_correlation_id);
    push_run_control_bytes(&mut out, &binding_bytes);
    Ok(out)
}

pub fn local_owner_mutation_binding_bytes(
    binding: &LocalOwnerMutationBinding,
) -> Result<Vec<u8>, RunControlAttestationEncodingError> {
    binding.validate()?;
    let mut out = Vec::with_capacity(384);
    push_run_control_text(&mut out, binding.operation.name());
    push_run_control_text(&mut out, &binding.method);
    push_run_control_text(&mut out, &binding.path);
    push_run_control_text(&mut out, &binding.request_correlation_id);
    push_run_control_text(&mut out, &binding.idempotency_key);
    out.extend_from_slice(&binding.expected_revision.to_be_bytes());
    push_run_control_text(&mut out, &binding.canonical_body_digest);
    push_run_control_text(&mut out, &binding.safe_snapshot_digest);
    out.extend_from_slice(&binding.created_at_ms.to_be_bytes());
    Ok(out)
}

/// Canonical bytes authenticated by the native-shell HMAC for one exact
/// decision response. The proof hex itself is intentionally excluded: it is
/// the MAC over these bytes. This preserves the original v1 domain, field
/// order, integer widths, and decision-result codes.
pub fn local_decision_proof_bytes(
    proof: &LocalDecisionProof,
    binding: &LocalDecisionProofBinding,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    push_run_control_bytes(&mut out, LOCAL_OWNER_DECISION_PROOF_DOMAIN);
    push_run_control_text(&mut out, &proof.authenticated_client_id);
    push_run_control_text(&mut out, &proof.request_correlation_id);
    push_run_control_text(&mut out, &binding.decision_id);
    out.extend_from_slice(&binding.decision_revision.to_be_bytes());
    push_run_control_text(&mut out, &binding.normalized_intent_digest);
    out.extend_from_slice(&binding.policy_revision.to_be_bytes());
    push_run_control_text(&mut out, &binding.canonical_manifest_digest);
    push_run_control_text(&mut out, &binding.selected_logical_action_id);
    push_run_control_text(&mut out, &binding.presented_action_digest);
    push_run_control_text(&mut out, &binding.declared_consequence_digest);
    push_run_control_text(&mut out, &binding.challenge_digest);
    out.extend_from_slice(&binding.expires_at_ms.to_be_bytes());
    push_run_control_text(&mut out, &binding.response_selected_logical_action_id);
    out.push(decision_result_code(binding.decision_result));
    push_run_control_text(&mut out, &binding.idempotency_key);
    push_run_control_optional_text(&mut out, binding.revision_text_digest.as_deref());
    push_run_control_optional_text(&mut out, binding.challenge_response_digest.as_deref());
    out.extend_from_slice(&binding.observed_at_ms.to_be_bytes());
    out
}

/// The single canonical serialization for an exact stop/resume request.
pub fn run_control_request_binding_bytes(
    binding: &RunControlRequestBinding,
) -> Result<Vec<u8>, RunControlAttestationEncodingError> {
    binding.validate()?;
    let mut out = Vec::with_capacity(384);
    push_run_control_bytes(&mut out, RUN_CONTROL_REQUEST_BINDING_DOMAIN);
    out.push(run_control_operation_code(binding.operation));
    match &binding.target {
        RunControlTarget::Global => out.push(1),
        RunControlTarget::Delegation { delegation_id } => {
            out.push(2);
            push_run_control_text(&mut out, delegation_id);
        }
    }
    push_run_control_text(&mut out, &binding.idempotency_key);
    push_run_control_text(&mut out, &binding.request_correlation_id);
    push_run_control_i64(&mut out, binding.observed_at_ms);
    match &binding.resume {
        None => out.push(0),
        Some(resume) => {
            out.push(1);
            push_run_control_i64(&mut out, resume.stopped_epoch);
            push_run_control_i64(&mut out, resume.current_policy_revision);
            push_run_control_text(&mut out, &resume.unresolved_effect_disclosure_digest);
            push_run_control_optional_i64(&mut out, resume.delegation_state_revision);
            push_run_control_optional_i64(&mut out, resume.current_plan_revision);
        }
    }
    Ok(out)
}

fn validate_run_control_request_binding(
    binding: &RunControlRequestBinding,
) -> Result<(), RunControlAttestationEncodingError> {
    if !is_bounded_safe_text(&binding.idempotency_key, 128)
        || !is_bounded_safe_text(&binding.request_correlation_id, 128)
        || binding.observed_at_ms <= 0
    {
        return Err(RunControlAttestationEncodingError::InvalidField(
            "request_binding",
        ));
    }
    match (&binding.operation, &binding.target, binding.resume.as_ref()) {
        (RunControlOperation::GlobalStop, RunControlTarget::Global, None) => Ok(()),
        (RunControlOperation::GlobalResume, RunControlTarget::Global, Some(snapshot))
            if snapshot.delegation_state_revision.is_none()
                && snapshot.current_plan_revision.is_none() =>
        {
            validate_run_control_resume_snapshot(snapshot)
        }
        (
            RunControlOperation::DelegationStop,
            RunControlTarget::Delegation { delegation_id },
            None,
        ) if is_bounded_safe_text(delegation_id, 256) => Ok(()),
        (
            RunControlOperation::DelegationResume,
            RunControlTarget::Delegation { delegation_id },
            Some(snapshot),
        ) if is_bounded_safe_text(delegation_id, 256)
            && snapshot
                .delegation_state_revision
                .is_some_and(|revision| revision > 0)
            && snapshot
                .current_plan_revision
                .is_some_and(|revision| revision > 0) =>
        {
            validate_run_control_resume_snapshot(snapshot)
        }
        _ => Err(RunControlAttestationEncodingError::InvalidField(
            "request_binding_shape",
        )),
    }
}

fn validate_run_control_resume_snapshot(
    snapshot: &RunControlResumeSnapshot,
) -> Result<(), RunControlAttestationEncodingError> {
    if snapshot.stopped_epoch <= 0
        || snapshot.current_policy_revision <= 0
        || !is_prefixed_lower_sha256(&snapshot.unresolved_effect_disclosure_digest)
        || snapshot
            .delegation_state_revision
            .is_some_and(|revision| revision <= 0)
        || snapshot
            .current_plan_revision
            .is_some_and(|revision| revision <= 0)
    {
        Err(RunControlAttestationEncodingError::InvalidField(
            "resume_snapshot",
        ))
    } else {
        Ok(())
    }
}

fn is_bounded_safe_text(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'\\' && byte != b'\"')
}

fn is_lower_hex(value: &str, exact_len: usize) -> bool {
    value.len() == exact_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_prefixed_lower_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn run_control_operation_code(value: RunControlOperation) -> u8 {
    match value {
        RunControlOperation::GlobalStop => 1,
        RunControlOperation::GlobalResume => 2,
        RunControlOperation::DelegationStop => 3,
        RunControlOperation::DelegationResume => 4,
    }
}

fn validate_run_control_payload(
    p: &RunControlAttestationPayload,
    key_id: &str,
) -> Result<(), RunControlAttestationEncodingError> {
    if key_id.trim().is_empty()
        || p.credential_identity.trim().is_empty()
        || p.authenticated_ingress.trim().is_empty()
        || p.channel_assurance.trim().is_empty()
        || p.request_correlation_id.trim().is_empty()
        || p.idempotency_key.trim().is_empty()
        || p.replay_identity.trim().is_empty()
        || p.observed_at_ms <= 0
        || p.issued_at_ms <= 0
        || p.issued_at_ms < p.observed_at_ms
        || p.stopped_epoch < 0
        || p.policy_revision <= 0
        || p.state_root_generation <= 0
        || p.signer_key_generation <= 0
        || !p
            .unresolved_effect_disclosure_digest
            .strip_prefix("sha256:")
            .is_some_and(|digest| {
                digest.len() == 64 && digest.as_bytes().iter().all(u8::is_ascii_hexdigit)
            })
    {
        return Err(RunControlAttestationEncodingError::InvalidField("binding"));
    }
    let resume = matches!(
        p.operation,
        RunControlOperation::GlobalResume | RunControlOperation::DelegationResume
    );
    if resume && !matches!(p.actor_type, ActorType::HumanLocal | ActorType::HumanRemote) {
        return Err(RunControlAttestationEncodingError::InvalidField(
            "actor_type",
        ));
    }
    match (&p.operation, &p.target) {
        (RunControlOperation::GlobalStop, RunControlTarget::Global)
            if p.delegation_state_revision.is_none() && p.current_plan_revision.is_none() => {}
        (RunControlOperation::GlobalResume, RunControlTarget::Global)
            if p.stopped_epoch > 0
                && p.delegation_state_revision.is_none()
                && p.current_plan_revision.is_none() => {}
        (RunControlOperation::DelegationStop, RunControlTarget::Delegation { delegation_id })
            if !delegation_id.trim().is_empty()
                && p.delegation_state_revision
                    .is_some_and(|revision| revision > 0)
                && p.current_plan_revision.is_none_or(|revision| revision > 0) => {}
        (RunControlOperation::DelegationResume, RunControlTarget::Delegation { delegation_id })
            if !delegation_id.trim().is_empty()
                && p.stopped_epoch > 0
                && p.delegation_state_revision
                    .is_some_and(|revision| revision > 0)
                && p.current_plan_revision.is_some_and(|revision| revision > 0) => {}
        _ => return Err(RunControlAttestationEncodingError::InvalidField("target")),
    }
    match p.actor_type {
        ActorType::HumanLocal if p.source_message_id.is_none() && p.provider_event_id.is_none() => {
        }
        ActorType::HumanRemote
            if p.source_message_id
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty())
                && p.provider_event_id
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty()) => {}
        _ => {
            return Err(RunControlAttestationEncodingError::InvalidField(
                "custody_shape",
            ))
        }
    }
    Ok(())
}
fn actor_type_wire(value: ActorType) -> &'static str {
    match value {
        ActorType::HumanLocal => "human_local",
        ActorType::HumanRemote => "human_remote",
        ActorType::Runtime => "runtime",
        ActorType::Worker => "worker",
        ActorType::Connector => "connector",
        ActorType::Model => "model",
    }
}
fn operation_wire(value: RunControlOperation) -> &'static str {
    match value {
        RunControlOperation::GlobalStop => "global_stop",
        RunControlOperation::GlobalResume => "global_resume",
        RunControlOperation::DelegationStop => "delegation_stop",
        RunControlOperation::DelegationResume => "delegation_resume",
    }
}
fn decision_result_code(value: DecisionResult) -> u8 {
    match value {
        DecisionResult::ConfirmAndContinue => 1,
        DecisionResult::Revise => 2,
        DecisionResult::Decline => 3,
        DecisionResult::Stop => 4,
    }
}
fn push_run_control_text(out: &mut Vec<u8>, value: &str) {
    push_run_control_bytes(out, value.as_bytes())
}
fn push_run_control_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}
fn push_run_control_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_run_control_optional_text(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            out.push(1);
            push_run_control_text(out, value)
        }
        None => out.push(0),
    }
}
fn push_run_control_optional_i64(out: &mut Vec<u8>, value: Option<i64>) {
    match value {
        Some(value) => {
            out.push(1);
            push_run_control_i64(out, value)
        }
        None => out.push(0),
    }
}

pub const EXECASS_API_VERSION: &str = "v1";
pub const EXECASS_SCHEMA_VERSION: &str = "1.1.0";

macro_rules! execass_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }
    };
}

execass_enum!(ActorType {
    HumanLocal,
    HumanRemote,
    Runtime,
    Connector,
    Worker,
    Model,
});

execass_enum!(DelegationPhase {
    Accepted,
    Planning,
    InMotion,
    WaitingForUser,
    WaitingExternal,
    Recovering,
    Completed,
    PartiallyCompleted,
    Failed,
});

execass_enum!(RunControlState {
    Running,
    StopRequested,
    Stopped,
});

execass_enum!(BranchState {
    Runnable,
    Executing,
    Waiting,
    Uncertain,
    Terminal,
});

execass_enum!(AttentionKind {
    Confirmation,
    Clarification,
    Reply,
    RecoveryChoice,
    RuntimePaused,
});

execass_enum!(DecisionKind {
    Clarification,
    DangerousActionConfirmation,
    OwnerConfiguredCheckpoint,
    RecoveryChoice,
    DuplicateRiskRetry,
    Stop,
    PolicyChange,
});

execass_enum!(DecisionResult {
    ConfirmAndContinue,
    Revise,
    Decline,
    Stop,
});

execass_enum!(DecisionStatus {
    Pending,
    Resolved,
    Superseded,
    Expired,
});

execass_enum!(EffectStatus {
    Planned,
    Claimed,
    Succeeded,
    Failed,
    OutcomeUnknown,
    Unresolved,
});

execass_enum!(ContinuationStatus {
    Pending,
    Runnable,
    Claimed,
    Waiting,
    Completed,
    Failed,
    Superseded,
});

execass_enum!(VerifierResult {
    Pass,
    Fail,
    Unknown,
});

execass_enum!(AssuranceRequirement {
    VerifiedOwnerResolution,
    MechanicalResolution,
});

execass_enum!(KnownDangerCategory {
    WholeDriveVolumeBootRecoveryOrCoreOsTreeErasureOrUnusable,
    WholeUserProfileOrHomeErasureOrUnusable,
    CompleteCarsinosStateIntegrityRuntimeEnforcementStopFencingOrRecoveryConfigurationErasureOrUnusable,
    WholeConnectedExternalAccountClosureOrErasure,
    LastVerifiedAdministrativeRecoveryOrDecryptionPathDestruction,
});

execass_enum!(DangerSource {
    KnownCategory,
    ModelCredibleDanger,
});

execass_enum!(TechnicalResourceKind {
    Tokens,
    TimeMs,
    ConnectorCalls,
    ResourceUnits,
});

execass_enum!(ObjectiveRetrySafetyFact {
    AttemptCount,
    ElapsedTime,
    Backoff,
    TechnicalResourceQuota,
    CircuitBreakers,
    ProviderErrorClass,
    Idempotency,
    IndependentAbsenceOrReconciliationProof,
    Reversibility,
    DeclaredSafeBoundary,
});

execass_enum!(OwnerResolutionIngress {
    LocalOwnerSession,
    AuthenticatedRemoteOwnerChannel,
});

execass_enum!(RuntimeHostDesiredMode {
    AppBound,
    Background,
});

execass_enum!(RuntimeHostActualState {
    Stopped,
    Starting,
    RunningAppBound,
    Handoff,
    RunningBackground,
    Draining,
    Faulted,
});

execass_enum!(AutonomyProfile {
    LockedDown,
    Balanced,
    FullSend,
    Custom,
});

execass_enum!(NextItemKind {
    Routine,
    Commitment,
    Deadline,
    FollowUp,
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EventName {
    #[serde(rename = "execass.v1.delegation.transitioned")]
    DelegationTransitioned,
    #[serde(rename = "execass.v1.decision.recorded")]
    DecisionRecorded,
    #[serde(rename = "execass.v1.continuation.claimed_or_result_recorded")]
    ContinuationClaimedOrResultRecorded,
    #[serde(rename = "execass.v1.recovery.updated")]
    RecoveryUpdated,
    #[serde(rename = "execass.v1.completion.assessed")]
    CompletionAssessed,
    #[serde(rename = "execass.v1.summary.changed")]
    SummaryChanged,
    #[serde(rename = "execass.v1.policy.changed")]
    PolicyChanged,
    #[serde(rename = "execass.v1.runtime_host.changed")]
    RuntimeHostChanged,
    #[serde(rename = "execass.v1.receipt.integrity_failed")]
    ReceiptIntegrityFailed,
    #[serde(rename = "execass.v1.notification.scheduled")]
    NotificationScheduled,
    #[serde(rename = "execass.v1.global_stop.changed")]
    GlobalStopChanged,
}

impl EventName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DelegationTransitioned => "execass.v1.delegation.transitioned",
            Self::DecisionRecorded => "execass.v1.decision.recorded",
            Self::ContinuationClaimedOrResultRecorded => {
                "execass.v1.continuation.claimed_or_result_recorded"
            }
            Self::RecoveryUpdated => "execass.v1.recovery.updated",
            Self::CompletionAssessed => "execass.v1.completion.assessed",
            Self::SummaryChanged => "execass.v1.summary.changed",
            Self::PolicyChanged => "execass.v1.policy.changed",
            Self::RuntimeHostChanged => "execass.v1.runtime_host.changed",
            Self::ReceiptIntegrityFailed => "execass.v1.receipt.integrity_failed",
            Self::NotificationScheduled => "execass.v1.notification.scheduled",
            Self::GlobalStopChanged => "execass.v1.global_stop.changed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ApiErrorCode {
    #[serde(rename = "execass.v1.invalid_request")]
    InvalidRequest,
    #[serde(rename = "execass.v1.authentication_required")]
    AuthenticationRequired,
    #[serde(rename = "execass.v1.idempotency_conflict")]
    IdempotencyConflict,
    #[serde(rename = "execass.v1.authority_denied")]
    AuthorityDenied,
    #[serde(rename = "execass.v1.decision_assurance_required")]
    DecisionAssuranceRequired,
    #[serde(rename = "execass.v1.decision_challenge_expired")]
    DecisionChallengeExpired,
    #[serde(rename = "execass.v1.not_found")]
    NotFound,
    #[serde(rename = "execass.v1.revision_conflict")]
    RevisionConflict,
    #[serde(rename = "execass.v1.invalid_transition")]
    InvalidTransition,
    #[serde(rename = "execass.v1.stop_all_engaged")]
    StopAllEngaged,
    #[serde(rename = "execass.v1.outcome_unknown_retry_prohibited")]
    OutcomeUnknownRetryProhibited,
    #[serde(rename = "execass.v1.technical_resource_exhausted")]
    TechnicalResourceExhausted,
    #[serde(rename = "execass.v1.receipt_integrity_quarantined")]
    ReceiptIntegrityQuarantined,
    #[serde(rename = "execass.v1.decision_superseded")]
    DecisionSuperseded,
    #[serde(rename = "execass.v1.runtime_host_conflict")]
    RuntimeHostConflict,
    #[serde(rename = "execass.v1.schema_replace_requires_quiescence")]
    SchemaReplaceRequiresQuiescence,
    #[serde(rename = "execass.v1.rate_limited")]
    RateLimited,
    #[serde(rename = "execass.v1.external_dependency")]
    ExternalDependency,
    #[serde(rename = "execass.v1.schema_version_unsupported")]
    SchemaVersionUnsupported,
    #[serde(rename = "execass.v1.internal_safe_failure")]
    InternalSafeFailure,
}

impl ApiErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "execass.v1.invalid_request",
            Self::AuthenticationRequired => "execass.v1.authentication_required",
            Self::IdempotencyConflict => "execass.v1.idempotency_conflict",
            Self::AuthorityDenied => "execass.v1.authority_denied",
            Self::DecisionAssuranceRequired => "execass.v1.decision_assurance_required",
            Self::DecisionChallengeExpired => "execass.v1.decision_challenge_expired",
            Self::NotFound => "execass.v1.not_found",
            Self::RevisionConflict => "execass.v1.revision_conflict",
            Self::InvalidTransition => "execass.v1.invalid_transition",
            Self::StopAllEngaged => "execass.v1.stop_all_engaged",
            Self::OutcomeUnknownRetryProhibited => "execass.v1.outcome_unknown_retry_prohibited",
            Self::TechnicalResourceExhausted => "execass.v1.technical_resource_exhausted",
            Self::ReceiptIntegrityQuarantined => "execass.v1.receipt_integrity_quarantined",
            Self::DecisionSuperseded => "execass.v1.decision_superseded",
            Self::RuntimeHostConflict => "execass.v1.runtime_host_conflict",
            Self::SchemaReplaceRequiresQuiescence => {
                "execass.v1.schema_replace_requires_quiescence"
            }
            Self::RateLimited => "execass.v1.rate_limited",
            Self::ExternalDependency => "execass.v1.external_dependency",
            Self::SchemaVersionUnsupported => "execass.v1.schema_version_unsupported",
            Self::InternalSafeFailure => "execass.v1.internal_safe_failure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActorSummary {
    pub actor_type: ActorType,
    pub actor_id: String,
    pub verified_evidence: Vec<String>,
    pub may_submit_intent: bool,
    pub may_resolve_decision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DangerAssessment {
    pub source: DangerSource,
    pub known_category: Option<KnownDangerCategory>,
    pub declared_consequence: String,
    pub requires_one_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TechnicalResourceQuota {
    pub kind: TechnicalResourceKind,
    pub limit: i64,
    pub reserved: i64,
    pub consumed: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActionSummary {
    pub action_id: String,
    pub branch_state: BranchState,
    pub manifest_revision: i64,
    pub manifest_digest: String,
    pub required_decision_kind: Option<DecisionKind>,
    pub requires_assurance: AssuranceRequirement,
    pub danger_assessments: Vec<DangerAssessment>,
    pub technical_resources: Vec<TechnicalResourceQuota>,
    pub safe_boundary_description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectSummary {
    pub effect_id: String,
    pub action_id: String,
    pub status: EffectStatus,
    pub provider_idempotency_key: Option<String>,
    pub external_reference: Option<String>,
    pub occurred_at_ms: Option<i64>,
    pub safe_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEvidenceSummary {
    pub authority_kind: String,
    pub source_id: String,
    pub authoritative_revision: i64,
    pub authority_link_id: String,
    pub observation_digest: String,
    pub deep_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(tag = "scope_kind", rename_all = "snake_case")]
pub enum ReceiptScope {
    Delegation {
        delegation_id: String,
        delegation_sequence: i64,
    },
    RuntimeHost {
        runtime_host_aggregate_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReceiptSummary {
    pub receipt_id: String,
    pub scope: ReceiptScope,
    pub global_sequence: i64,
    pub receipt_kind: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub subject_revision: i64,
    pub occurred_at_ms: i64,
    pub committed_at_ms: i64,
    pub evidence_refs: Vec<ReceiptEvidenceSummary>,
    pub receipt_digest: String,
    pub delegation_previous_receipt_digest: Option<String>,
    pub global_previous_receipt_digest: Option<String>,
    pub key_id: String,
    pub key_generation: i64,
    pub integrity_tag: String,
    pub previous_key_integrity_tag: Option<String>,
    pub safe_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifierSummary {
    pub verifier_id: String,
    pub verifier_type: String,
    pub criterion_id: String,
    pub result: VerifierResult,
    pub authoritative_evidence_ref: String,
    pub assessed_at_ms: i64,
    pub safe_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContinuationSummary {
    pub continuation_id: String,
    pub delegation_id: String,
    pub status: ContinuationStatus,
    pub plan_revision: i64,
    pub policy_revision: i64,
    pub scheduled_for_ms: Option<i64>,
    pub claimed_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub safe_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutcomeCriterionSummary {
    pub criterion_id: String,
    pub material: bool,
    pub expected_predicate: String,
    pub verifier_type: String,
    pub verifier_result: VerifierResult,
    pub authoritative_evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionChallenge {
    pub decision_revision: i64,
    pub exact_presented_action_or_alternative: String,
    pub declared_consequence: String,
    pub nonce_or_token: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptedConfirmationGrant {
    pub delegation_id: String,
    pub normalized_intent: String,
    pub confirmed_logical_action_identity: String,
    pub canonical_action_envelope_or_selector: String,
    pub payload_and_material_operands_digest: String,
    pub connector_or_tool_identity_and_version: String,
    pub declared_consequence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnerResolutionSummary {
    pub ingress: OwnerResolutionIngress,
    pub verified_evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecoverySummary {
    pub recovery_id: String,
    pub action_id: String,
    pub objective_retry_safety_facts: Vec<ObjectiveRetrySafetyFact>,
    pub outcome_unknown: bool,
    pub automatic_retry_permitted: bool,
    pub safe_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionSummary {
    pub decision_id: String,
    pub delegation_id: String,
    pub revision: i64,
    pub status: DecisionStatus,
    pub kind: DecisionKind,
    pub result: Option<DecisionResult>,
    pub assurance_required: AssuranceRequirement,
    pub recommendation: String,
    pub why_now: String,
    pub consequence: String,
    pub alternatives: Vec<String>,
    pub exact_manifest_digest: String,
    pub technical_resources: Vec<TechnicalResourceQuota>,
    pub challenge: Option<DecisionChallenge>,
    pub accepted_confirmation_grant: Option<AcceptedConfirmationGrant>,
    pub resolved_owner: Option<OwnerResolutionSummary>,
    pub requested_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
    pub authoritative_deep_link: String,
    pub local_owner_proof_challenge: Option<DecisionProofChallenge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(tag = "scope_kind", rename_all = "snake_case")]
pub enum AttentionSubject {
    Delegation {
        delegation_id: String,
        delegation_revision: i64,
    },
    RuntimeHost {
        runtime_host_generation: i64,
        runtime_host_instance_id: String,
        runtime_fencing_token: i64,
        runtime_actual_state: RuntimeHostActualState,
        runtime_end_reason: String,
        active_work_binding_digest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttentionItem {
    pub attention_id: String,
    pub kind: AttentionKind,
    /// Reply and runtime-paused attention are not decisions. Every
    /// decision-backed attention item carries its exact kind; non-decision
    /// variants serialize this field as `null` so the versioned wire shape
    /// stays explicit rather than inventing a faux decision.
    pub decision_kind: Option<DecisionKind>,
    pub subject: AttentionSubject,
    pub decision_id: Option<String>,
    pub reason: String,
    pub recommendation: String,
    pub alternatives_or_actions: Vec<String>,
    pub assurance_required: AssuranceRequirement,
    pub deadline_reminder_state: String,
    pub deadline_at_ms: Option<i64>,
    pub decision_revision: Option<i64>,
    pub authoritative_deep_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationSummary {
    pub delegation_id: String,
    pub phase: DelegationPhase,
    pub run_control: RunControlState,
    pub state_revision: i64,
    pub intent_summary: String,
    pub outcome_summary: String,
    pub policy_revision: i64,
    pub pending_decision: Option<DecisionSummary>,
    pub pending_external_wait: Option<String>,
    pub stop_epoch: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub acknowledged_at_ms: Option<i64>,
    pub terminal_at_ms: Option<i64>,
    pub authoritative_deep_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationDetail {
    pub delegation: DelegationSummary,
    pub original_intent: String,
    pub immutable_intake_evidence_ref: String,
    pub ingress_source: String,
    pub source_correlation_id: String,
    pub plan_summary: String,
    pub outcome_criteria: Vec<OutcomeCriterionSummary>,
    pub authority_snapshot_ref: String,
    pub technical_resource_summary: String,
    pub internal_record_refs: Vec<String>,
    pub actions: Vec<ActionSummary>,
    pub continuations: Vec<ContinuationSummary>,
    pub effects: Vec<EffectSummary>,
    pub recovery: Option<RecoverySummary>,
    pub completion_verifiers: Vec<VerifierSummary>,
    pub receipt_chain_head: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntakeRequest {
    pub request_id: String,
    pub idempotency_key: String,
    pub text: String,
    pub source_correlation_id: String,
    pub attach_to_delegation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntakeResponse {
    Conversational {
        response_text: String,
        request_audit_ref: String,
    },
    Delegation {
        delegation: Box<DelegationSummary>,
        created: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeliveredItem {
    pub item_id: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SummaryCursor {
    pub cursor: String,
    pub displayed_at_ms: i64,
    pub delivered: Vec<DeliveredItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NextItem {
    pub next_item_id: String,
    pub kind: NextItemKind,
    pub delegation_id: Option<String>,
    pub due_at_ms: Option<i64>,
    pub scheduled_for_ms: Option<i64>,
    pub summary: String,
    pub authoritative_deep_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SummaryResponse {
    pub needs_you: Vec<AttentionItem>,
    pub in_motion: Vec<DelegationSummary>,
    pub done: Vec<DelegationSummary>,
    pub next: Vec<NextItem>,
    pub receipts: Vec<ReceiptSummary>,
    pub displayed: SummaryCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SummaryAckRequest {
    pub idempotency_key: String,
    pub displayed: SummaryCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SummaryAckResponse {
    pub acknowledged: bool,
    pub displayed: SummaryCursor,
    pub acknowledged_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationListQuery {
    pub phase: Option<DelegationPhase>,
    pub run_control: Option<RunControlState>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationListResponse {
    pub items: Vec<DelegationSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationDetailResponse {
    pub detail: DelegationDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationReceiptsResponse {
    pub delegation_id: String,
    pub receipts: Vec<ReceiptSummary>,
    pub receipt_chain_head: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveDecisionRequest {
    pub idempotency_key: String,
    pub decision_revision: i64,
    pub result: DecisionResult,
    pub revision_text: Option<String>,
    pub challenge_response: Option<String>,
    pub local_proof: LocalDecisionProof,
    pub local_proof_binding: LocalDecisionProofBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveDecisionResponse {
    pub decision: DecisionSummary,
    pub delegation: DelegationSummary,
    pub continuation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationRunControlRequest {
    pub binding: RunControlRequestBinding,
    pub local_proof: LocalRunControlProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DelegationRunControlResponse {
    pub delegation_id: String,
    pub phase: DelegationPhase,
    pub run_control: RunControlState,
    pub state_revision: i64,
    pub current_plan_revision: Option<i64>,
    pub stop_epoch: i64,
    pub policy_revision: i64,
    pub drain_state: String,
    pub unresolved_effect_disclosure_digest: String,
    pub unresolved_external_effect_refs: Vec<UnresolvedExternalEffectRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopAllStatusResponse {
    pub engaged: bool,
    pub stop_epoch: i64,
    pub current_policy_revision: i64,
    pub drain_state: String,
    pub unresolved_effect_disclosure_digest: String,
    pub unresolved_external_effect_refs: Vec<UnresolvedExternalEffectRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedExternalEffectRef {
    pub logical_effect_id: String,
    pub delegation_id: String,
    pub continuation_id: String,
    pub state: String,
    pub latest_attempt_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopAllRequest {
    pub binding: RunControlRequestBinding,
    pub local_proof: LocalRunControlProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResumeAllRequest {
    pub binding: RunControlRequestBinding,
    pub local_proof: LocalRunControlProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResumeAllResponse {
    pub stop_all: StopAllStatusResponse,
    pub resumed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyRule {
    pub rule_id: String,
    pub task_or_delegation_scope: Option<String>,
    pub workspace_scope: Option<String>,
    pub routine_scope: Option<String>,
    pub connector_or_tool_identity_and_version_scope: Option<String>,
    pub target_scope: Option<String>,
    pub audience_scope: Option<String>,
    pub technical_resource_quotas: Vec<TechnicalResourceQuota>,
    pub expires_at_ms: Option<i64>,
    pub recovery_limit: Option<i64>,
    pub parallelism_limit: Option<u32>,
    pub clarification_sensitivity: Option<String>,
    pub recurring_work_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyResponse {
    pub policy_id: String,
    pub revision: i64,
    /// `None` only for the immutable fresh-root bootstrap before the owner has
    /// configured an operational profile. The server must not invent one.
    pub profile: Option<AutonomyProfile>,
    pub rules: Vec<PolicyRule>,
    pub effective_operational_summary: String,
    pub configured: bool,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyUpdateRequest {
    pub idempotency_key: String,
    pub expected_policy_revision: i64,
    pub proposed_profile: AutonomyProfile,
    pub proposed_rules: Vec<PolicyRule>,
    pub change_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyUpdateResponse {
    pub policy: PolicyResponse,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeHostStatusResponse {
    pub desired_mode: RuntimeHostDesiredMode,
    pub actual_state: RuntimeHostActualState,
    pub ownership_mode: String,
    pub process_id: Option<i64>,
    pub started_at_ms: Option<i64>,
    pub fencing_generation: i64,
    pub state_root_version: String,
    pub restart_reason: Option<String>,
    pub health: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeHostConfigRequest {
    pub idempotency_key: String,
    pub expected_settings_revision: i64,
    pub desired_mode: RuntimeHostDesiredMode,
    pub start_at_login: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeHostConfigResponse {
    pub status: RuntimeHostStatusResponse,
    pub start_at_login: bool,
    pub bounded_settings_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SafeEventPayload {
    pub summary: String,
    pub delegation_id: Option<String>,
    pub decision_id: Option<String>,
    pub receipt_ref: Option<String>,
    pub authoritative_deep_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DurableEventEnvelope {
    pub event_name: EventName,
    pub aggregate_id: String,
    pub revision: i64,
    pub correlation_id: String,
    pub causation_id: String,
    pub occurred_at_ms: i64,
    pub schema_version: String,
    pub safe_payload: SafeEventPayload,
    pub global_sequence: i64,
    pub duplicate_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub safe_human_message: String,
    pub retryable: bool,
    pub correlation_id: String,
    pub safe_for_display: bool,
    pub exposes_sensitive_metadata: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::schema_for;

    fn run_control_payload() -> RunControlAttestationPayload {
        RunControlAttestationPayload {
            actor_type: ActorType::HumanLocal,
            credential_identity: "mission-control-window-1".into(),
            authenticated_ingress: "native-control".into(),
            channel_assurance: "interactive-local".into(),
            request_correlation_id: "control-correlation-1".into(),
            source_message_id: None,
            provider_event_id: None,
            operation: RunControlOperation::GlobalResume,
            target: RunControlTarget::Global,
            idempotency_key: "control-idempotency-1".into(),
            replay_identity: "control-replay-1".into(),
            observed_at_ms: 1_800_000_000_000,
            issued_at_ms: 1_800_000_000_001,
            stopped_epoch: 3,
            policy_revision: 7,
            unresolved_effect_disclosure_digest: format!("sha256:{}", "a".repeat(64)),
            delegation_state_revision: None,
            current_plan_revision: None,
            canonical_root_identity: "root-1".into(),
            installation_identity: "install-1".into(),
            os_user_identity_digest: "user-1".into(),
            state_root_generation: 1,
            signer_key_generation: 1,
        }
    }

    fn run_control_request_binding() -> RunControlRequestBinding {
        RunControlRequestBinding::global_resume(
            "control-idempotency-1".into(),
            "control-correlation-1".into(),
            1_800_000_000_000,
            RunControlResumeSnapshot::new(3, 7, format!("sha256:{}", "a".repeat(64)), None, None)
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn run_control_attestation_bytes_are_deterministic_and_domain_separated() {
        let payload = run_control_payload();
        let first = run_control_attestation_signing_bytes(&payload, "control-key-1").unwrap();
        let second = run_control_attestation_signing_bytes(&payload, "control-key-1").unwrap();
        let local = local_run_control_request_proof_bytes(
            "mission-control-window-1",
            &run_control_request_binding(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_ne!(first, local);
    }

    #[test]
    fn local_run_control_request_proof_binds_every_caller_selected_field() {
        let original = run_control_request_binding();
        let expected =
            local_run_control_request_proof_bytes("carsinos-mission-control-desktop-v1", &original)
                .unwrap();
        let duplicate =
            local_run_control_request_proof_bytes("carsinos-mission-control-desktop-v1", &original)
                .unwrap();
        assert_eq!(expected, duplicate);

        for mutation in 0..9 {
            let mut binding = original.clone();
            match mutation {
                0 => binding.operation = RunControlOperation::GlobalStop,
                1 => {
                    binding.target = RunControlTarget::Delegation {
                        delegation_id: "delegation-2".into(),
                    }
                }
                2 => binding.idempotency_key = "other-idempotency".into(),
                3 => binding.request_correlation_id = "other-correlation".into(),
                4 => binding.observed_at_ms += 1,
                5 => binding.resume.as_mut().unwrap().stopped_epoch += 1,
                6 => binding.resume.as_mut().unwrap().current_policy_revision += 1,
                7 => {
                    binding
                        .resume
                        .as_mut()
                        .unwrap()
                        .unresolved_effect_disclosure_digest = format!("sha256:{}", "b".repeat(64))
                }
                _ => binding.resume = None,
            }
            if let Ok(bytes) = local_run_control_request_proof_bytes(
                "carsinos-mission-control-desktop-v1",
                &binding,
            ) {
                assert_ne!(expected, bytes, "mutation {mutation} was not bound");
            }
        }

        let other_client =
            local_run_control_request_proof_bytes("another-authenticated-client", &original)
                .unwrap();
        assert_ne!(expected, other_client);
    }

    fn local_owner_intake_proof() -> LocalOwnerIntakeProof {
        LocalOwnerIntakeProof::from_authenticated_native_request(
            "carsinos-mission-control-desktop-v1".into(),
            "intake-correlation-1".into(),
            "intake-request-1".into(),
            "intake-idempotency-1".into(),
            None,
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
        )
        .unwrap()
    }

    fn local_decision_proof() -> (LocalDecisionProof, LocalDecisionProofBinding) {
        (
            LocalDecisionProof {
                authenticated_client_id: "carsinos-mission-control-desktop-v1".into(),
                request_correlation_id: "decision-correlation-1".into(),
                proof_hex: "a".repeat(64),
            },
            LocalDecisionProofBinding {
                decision_id: "decision-1".into(),
                decision_revision: 7,
                normalized_intent_digest: "intent-digest".into(),
                policy_revision: 11,
                canonical_manifest_digest: "manifest-digest".into(),
                selected_logical_action_id: "action-1".into(),
                presented_action_digest: "presented-action-digest".into(),
                declared_consequence_digest: "consequence-digest".into(),
                challenge_digest: "challenge-digest".into(),
                expires_at_ms: 1_800_000_060_000,
                response_selected_logical_action_id: "action-1".into(),
                decision_result: DecisionResult::ConfirmAndContinue,
                idempotency_key: "decision-idempotency-1".into(),
                revision_text_digest: None,
                challenge_response_digest: None,
                observed_at_ms: 1_800_000_000_000,
            },
        )
    }

    #[test]
    fn local_decision_proof_bytes_bind_every_current_result_and_correlation_field() {
        let (proof, binding) = local_decision_proof();
        let expected = local_decision_proof_bytes(&proof, &binding);
        assert_eq!(expected, local_decision_proof_bytes(&proof, &binding));

        for mutation in 0..18 {
            let mut changed_proof = proof.clone();
            let mut changed_binding = binding.clone();
            match mutation {
                0 => changed_proof.authenticated_client_id = "other-client".into(),
                1 => changed_proof.request_correlation_id = "other-correlation".into(),
                2 => changed_binding.decision_id = "other-decision".into(),
                3 => changed_binding.decision_revision += 1,
                4 => changed_binding.normalized_intent_digest = "other-intent".into(),
                5 => changed_binding.policy_revision += 1,
                6 => changed_binding.canonical_manifest_digest = "other-manifest".into(),
                7 => changed_binding.selected_logical_action_id = "other-current-action".into(),
                8 => changed_binding.presented_action_digest = "other-presented-action".into(),
                9 => changed_binding.declared_consequence_digest = "other-consequence".into(),
                10 => changed_binding.challenge_digest = "other-challenge".into(),
                11 => changed_binding.expires_at_ms += 1,
                12 => {
                    changed_binding.response_selected_logical_action_id =
                        "other-response-action".into()
                }
                13 => changed_binding.decision_result = DecisionResult::Decline,
                14 => changed_binding.idempotency_key = "other-idempotency".into(),
                15 => changed_binding.revision_text_digest = Some("revision-digest".into()),
                16 => changed_binding.challenge_response_digest = Some("challenge-response".into()),
                _ => changed_binding.observed_at_ms += 1,
            }
            assert_ne!(
                expected,
                local_decision_proof_bytes(&changed_proof, &changed_binding),
                "canonical MAC input omitted mutation {mutation}"
            );
        }

        let mut changed_tag = proof;
        changed_tag.proof_hex = "b".repeat(64);
        assert_eq!(expected, local_decision_proof_bytes(&changed_tag, &binding));
    }

    #[test]
    fn local_owner_intake_proof_binds_every_field_and_has_its_own_domain() {
        let original = local_owner_intake_proof();
        let expected = local_owner_intake_proof_bytes(&original).unwrap();
        assert_eq!(expected, local_owner_intake_proof_bytes(&original).unwrap());
        assert_ne!(
            expected,
            local_run_control_request_proof_bytes(
                &original.authenticated_client_id,
                &run_control_request_binding(),
            )
            .unwrap()
        );

        for mutation in 0..7 {
            let mut changed = original.clone();
            match mutation {
                0 => changed.authenticated_client_id = "another-native-client".into(),
                1 => changed.request_correlation_id = "another-correlation".into(),
                2 => changed.request_id = "another-request".into(),
                3 => changed.idempotency_key = "another-idempotency".into(),
                4 => changed.attach_to_delegation_id = Some("delegation-2".into()),
                5 => changed.normalized_intent_digest = "d".repeat(64),
                _ => changed.instruction_digest = "e".repeat(64),
            }
            assert_ne!(
                expected,
                local_owner_intake_proof_bytes(&changed).unwrap(),
                "bound mutation {mutation}"
            );
        }
    }

    #[test]
    fn local_owner_intake_proof_rejects_noncanonical_fields_and_proof_hex() {
        let valid = local_owner_intake_proof();
        for mutation in 0..8 {
            let mut changed = valid.clone();
            match mutation {
                0 => changed.authenticated_client_id = " leading-space".into(),
                1 => changed.request_correlation_id.clear(),
                2 => changed.request_id = "request\nline".into(),
                3 => changed.idempotency_key = "x".repeat(129),
                4 => changed.attach_to_delegation_id = Some(" delegation".into()),
                5 => changed.normalized_intent_digest = "A".repeat(64),
                6 => changed.instruction_digest = "A".repeat(64),
                _ => changed.proof_hex = "B".repeat(64),
            }
            assert!(changed.validate().is_err(), "invalid mutation {mutation}");
        }
    }

    #[test]
    fn builtin_secret_pattern_redaction_covers_protocol_intent_canaries() {
        for raw in [
            "Bearer abcdefghijkl",
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==",
            "JWT eyJabc.def.ghi",
            "token sk-proj-abcdefghijklmnopqrstuvwxyz123456",
            "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----",
        ] {
            let redacted = redact_execass_builtin_secret_patterns(raw);
            assert!(redacted.contains("[REDACTED]"), "not redacted: {raw}");
            assert_ne!(redacted, raw, "unchanged secret-shaped input: {raw}");
        }
    }

    #[test]
    fn normalized_owner_intent_digest_is_domain_separated_and_rejects_blank_text() {
        let redacted = redact_execass_builtin_secret_patterns("token sk-proj-abcdefghijk");
        let digest = normalized_owner_intent_digest(&redacted).unwrap();
        assert_eq!(digest.len(), 64);
        assert_ne!(digest, format!("{:x}", Sha256::digest(redacted.as_bytes())));
        assert!(normalized_owner_intent_digest("   ").is_none());
    }

    #[test]
    fn owner_instruction_digest_binds_exact_bytes_with_core_authority_domain() {
        let exact = b" owner instruction with exact whitespace ";
        let digest = owner_instruction_digest(exact).unwrap();
        assert_eq!(digest.len(), 64);
        assert_ne!(digest, format!("{:x}", Sha256::digest(exact)));
        assert_ne!(digest, owner_instruction_digest(&exact[1..]).unwrap());
        assert!(owner_instruction_digest(b"").is_none());
    }

    #[test]
    fn local_owner_intake_schema_requires_both_safe_and_exact_digests() {
        let schema = schemars::schema_for!(LocalOwnerIntakeProof).to_value();
        let required = schema["required"].as_array().unwrap();
        assert!(required
            .iter()
            .any(|field| field == "normalized_intent_digest"));
        assert!(required.iter().any(|field| field == "instruction_digest"));
        assert!(schema["properties"]["instruction_digest"].is_object());
    }

    #[test]
    fn run_control_request_rejects_operation_target_and_resume_shape_mismatch() {
        let valid = run_control_request_binding();
        for mutation in 0..8 {
            let mut binding = valid.clone();
            match mutation {
                0 => binding.operation = RunControlOperation::GlobalStop,
                1 => {
                    binding.target = RunControlTarget::Delegation {
                        delegation_id: "delegation-1".into(),
                    }
                }
                2 => binding.idempotency_key.clear(),
                3 => binding.request_correlation_id = " correlation".into(),
                4 => binding.observed_at_ms = 0,
                5 => binding.resume.as_mut().unwrap().stopped_epoch = 0,
                6 => binding.resume.as_mut().unwrap().current_policy_revision = 0,
                _ => binding.resume.as_mut().unwrap().current_plan_revision = Some(1),
            }
            assert!(
                run_control_request_binding_bytes(&binding).is_err(),
                "invalid request mutation {mutation} encoded"
            );
        }
    }

    #[test]
    fn serde_created_invalid_request_and_noncanonical_proof_fail_validation_without_panic() {
        let mut value = serde_json::to_value(run_control_request_binding()).unwrap();
        value["operation"] = serde_json::json!("global_stop");
        let invalid: RunControlRequestBinding = serde_json::from_value(value).unwrap();
        assert!(invalid.validate().is_err());
        assert!(invalid.try_request_binding_digest().is_err());

        let uppercase: LocalRunControlProof = serde_json::from_value(serde_json::json!({
            "authenticated_client_id": "carsinos-mission-control-desktop-v1",
            "request_correlation_id": "control-correlation-1",
            "proof_hex": "A".repeat(64),
        }))
        .unwrap();
        assert!(uppercase.validate().is_err());
    }

    #[test]
    fn run_control_http_dtos_require_exact_binding_and_native_proof() {
        let binding = run_control_request_binding();
        let local_proof = LocalRunControlProof::from_authenticated_native_request(
            "carsinos-mission-control-desktop-v1".into(),
            binding.request_correlation_id.clone(),
            "ab".repeat(32),
        )
        .unwrap();
        let stop = StopAllRequest {
            binding: binding.clone(),
            local_proof: local_proof.clone(),
        };
        let resume = ResumeAllRequest {
            binding: binding.clone(),
            local_proof: local_proof.clone(),
        };
        let delegation = DelegationRunControlRequest {
            binding,
            local_proof,
        };
        for value in [
            serde_json::to_value(stop).unwrap(),
            serde_json::to_value(resume).unwrap(),
            serde_json::to_value(delegation).unwrap(),
        ] {
            assert!(value.get("binding").is_some());
            assert!(value.get("local_proof").is_some());
            assert!(value.get("reason").is_none());
            assert!(value.get("actor_type").is_none());
        }

        let schema = schema_for!(StopAllStatusResponse).to_value();
        let required = schema["required"].as_array().unwrap();
        for field in [
            "engaged",
            "stop_epoch",
            "current_policy_revision",
            "drain_state",
            "unresolved_effect_disclosure_digest",
            "unresolved_external_effect_refs",
        ] {
            assert!(required.iter().any(|item| item == field), "missing {field}");
        }
    }

    #[test]
    fn run_control_attestation_rejects_invalid_global_and_delegation_shapes() {
        let valid = run_control_payload();
        for mutation in 0..9 {
            let mut payload = valid.clone();
            match mutation {
                0 => payload.policy_revision = 0,
                1 => payload.stopped_epoch = 0,
                2 => {
                    payload.unresolved_effect_disclosure_digest =
                        format!("sha256:{}", "z".repeat(64))
                }
                3 => payload.delegation_state_revision = Some(1),
                4 => payload.current_plan_revision = Some(1),
                5 => payload.operation = RunControlOperation::DelegationResume,
                6 => payload.actor_type = ActorType::Runtime,
                7 => payload.issued_at_ms = payload.observed_at_ms - 1,
                _ => payload.replay_identity.clear(),
            }
            assert!(
                run_control_attestation_signing_bytes(&payload, "control-key-1").is_err(),
                "invalid global mutation {mutation} encoded"
            );
        }

        let mut delegation = valid;
        delegation.operation = RunControlOperation::DelegationResume;
        delegation.target = RunControlTarget::Delegation {
            delegation_id: "delegation-1".into(),
        };
        delegation.delegation_state_revision = Some(11);
        delegation.current_plan_revision = Some(4);
        assert!(run_control_attestation_signing_bytes(&delegation, "control-key-1").is_ok());
        delegation.current_plan_revision = None;
        assert!(run_control_attestation_signing_bytes(&delegation, "control-key-1").is_err());
    }

    fn actor() -> ActorSummary {
        ActorSummary {
            actor_type: ActorType::HumanLocal,
            actor_id: "human_01".into(),
            verified_evidence: vec!["interactive_local_operator_session".into()],
            may_submit_intent: true,
            may_resolve_decision: true,
        }
    }

    fn delegation() -> DelegationSummary {
        DelegationSummary {
            delegation_id: "dlg_01".into(),
            phase: DelegationPhase::InMotion,
            run_control: RunControlState::Running,
            state_revision: 7,
            intent_summary: "Prepare a draft".into(),
            outcome_summary: "Draft is ready".into(),
            policy_revision: 3,
            pending_decision: None,
            pending_external_wait: None,
            stop_epoch: 0,
            created_at_ms: 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_001,
            acknowledged_at_ms: None,
            terminal_at_ms: None,
            authoritative_deep_link: "carsinos://execass/delegations/dlg_01".into(),
        }
    }

    fn decision() -> DecisionSummary {
        DecisionSummary {
            decision_id: "dec_01".into(),
            delegation_id: "dlg_01".into(),
            revision: 4,
            status: DecisionStatus::Pending,
            kind: DecisionKind::DangerousActionConfirmation,
            result: None,
            assurance_required: AssuranceRequirement::VerifiedOwnerResolution,
            recommendation: "Approve the bounded draft.".into(),
            why_now: "The next action needs an explicit decision.".into(),
            consequence: "The draft will remain private.".into(),
            alternatives: vec!["Decline".into(), "Revise".into()],
            exact_manifest_digest: "sha256:manifest_01".into(),
            technical_resources: vec![TechnicalResourceQuota {
                kind: TechnicalResourceKind::Tokens,
                limit: 100,
                reserved: 0,
                consumed: 0,
            }],
            challenge: Some(DecisionChallenge {
                decision_revision: 4,
                exact_presented_action_or_alternative: "action_01".into(),
                declared_consequence: "The action will run once.".into(),
                nonce_or_token: "challenge_01".into(),
                expires_at_ms: 1_700_000_600_000,
            }),
            accepted_confirmation_grant: None,
            resolved_owner: None,
            requested_at_ms: 1_700_000_000_000,
            resolved_at_ms: None,
            authoritative_deep_link: "carsinos://execass/decisions/dec_01".into(),
            local_owner_proof_challenge: None,
        }
    }

    #[test]
    fn intake_response_round_trips_with_a_required_discriminator() {
        let value = IntakeResponse::Delegation {
            delegation: Box::new(delegation()),
            created: true,
        };
        let json = serde_json::to_string(&value).expect("serialize intake response");
        assert!(json.contains("\"kind\":\"delegation\""));
        assert_eq!(
            serde_json::from_str::<IntakeResponse>(&json).expect("deserialize intake response"),
            value
        );
        assert!(serde_json::from_str::<IntakeResponse>(
            r#"{"response_text":"hello","request_audit_ref":"audit_01"}"#
        )
        .is_err());
    }

    #[test]
    fn dto_denies_unknown_fields() {
        assert!(serde_json::from_str::<IntakeRequest>(
            r#"{"request_id":"req_01","idempotency_key":"idem_01","text":"hi","source_correlation_id":"corr_01","attach_to_delegation_id":null,"unexpected":true}"#
        )
        .is_err());
    }

    #[test]
    fn inbound_decision_requests_reject_caller_supplied_actor_facts() {
        assert!(serde_json::from_str::<ResolveDecisionRequest>(
            r#"{"idempotency_key":"idem_01","decision_revision":4,"result":"confirm_and_continue","revision_text":null,"challenge_response":null,"local_proof":{"authenticated_client_id":"mission-control","request_correlation_id":"corr_01","proof_hex":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"actor":{"actor_type":"human_local"}}"#
        )
        .is_err());
    }

    #[test]
    fn phase_and_control_remain_independent() {
        let mut value = delegation();
        value.phase = DelegationPhase::InMotion;
        value.run_control = RunControlState::Stopped;
        let encoded = serde_json::to_value(value).expect("serialize delegation");
        assert_eq!(encoded["phase"], "in_motion");
        assert_eq!(encoded["run_control"], "stopped");
    }

    #[test]
    fn actor_resolution_and_danger_facts_are_explicit() {
        let human = actor();
        assert!(human.may_resolve_decision);
        let remote = ActorSummary {
            actor_type: ActorType::HumanRemote,
            ..human
        };
        assert!(remote.may_resolve_decision);
        assert_eq!(
            DangerSource::ModelCredibleDanger,
            DangerSource::ModelCredibleDanger
        );
    }

    #[test]
    fn versioned_event_and_error_codes_keep_the_locked_wire_names() {
        assert_eq!(
            serde_json::to_value(EventName::ReceiptIntegrityFailed).expect("serialize event name"),
            "execass.v1.receipt.integrity_failed"
        );
        assert_eq!(
            serde_json::to_value(ApiErrorCode::DecisionChallengeExpired)
                .expect("serialize error code"),
            "execass.v1.decision_challenge_expired"
        );
    }

    #[test]
    fn summary_retains_exact_delivered_set() {
        let displayed = SummaryCursor {
            cursor: "summary_03".into(),
            displayed_at_ms: 1_700_000_000_002,
            delivered: vec![
                DeliveredItem {
                    item_id: "attention_01".into(),
                    revision: 11,
                },
                DeliveredItem {
                    item_id: "delegation_01".into(),
                    revision: 7,
                },
            ],
        };
        let ack = SummaryAckRequest {
            idempotency_key: "idem_02".into(),
            displayed: displayed.clone(),
        };
        let decoded: SummaryAckRequest =
            serde_json::from_value(serde_json::to_value(ack).expect("serialize ack"))
                .expect("deserialize ack");
        assert_eq!(decoded.displayed, displayed);
    }

    #[test]
    fn decision_resolution_exposes_zero_or_one_continuation() {
        let none = ResolveDecisionResponse {
            decision: decision(),
            delegation: delegation(),
            continuation_id: None,
        };
        let one = ResolveDecisionResponse {
            continuation_id: Some("continuation_01".into()),
            ..none.clone()
        };
        assert_eq!(
            serde_json::from_value::<ResolveDecisionResponse>(
                serde_json::to_value(none).expect("serialize no continuation")
            )
            .expect("deserialize no continuation")
            .continuation_id,
            None
        );
        assert_eq!(
            serde_json::from_value::<ResolveDecisionResponse>(
                serde_json::to_value(one).expect("serialize continuation")
            )
            .expect("deserialize continuation")
            .continuation_id
            .as_deref(),
            Some("continuation_01")
        );
    }

    #[test]
    fn v11_contract_has_no_financial_approval_or_fresh_local_vocabulary() {
        let policy = schema_for!(PolicyUpdateRequest).to_value().to_string();
        let decision = schema_for!(ResolveDecisionRequest).to_value().to_string();
        let error = schema_for!(ApiError).to_value().to_string();
        for prohibited in [
            "budget",
            "currency",
            "payee",
            "purchase",
            "financial",
            "approval",
            "fresh_local",
            "hard_lock",
            "local_presence",
        ] {
            assert!(!policy.contains(prohibited), "policy exposes {prohibited}");
            assert!(
                !decision.contains(prohibited),
                "decision exposes {prohibited}"
            );
            assert!(!error.contains(prohibited), "error exposes {prohibited}");
        }
    }

    #[test]
    fn accepted_confirmation_grant_has_no_expiry_or_use_counter() {
        let grant = schema_for!(AcceptedConfirmationGrant)
            .to_value()
            .to_string();
        assert!(!grant.contains("expires"));
        assert!(!grant.contains("uses"));
    }

    #[test]
    fn attention_has_a_total_typed_decision_mapping_field() {
        let attention = schema_for!(AttentionItem).to_value();
        assert!(attention["properties"]["decision_kind"]["anyOf"]
            .as_array()
            .expect("nullable decision-kind schema")
            .iter()
            .any(|branch| branch["type"] == "null"));
        for kind in [
            AttentionKind::Confirmation,
            AttentionKind::Clarification,
            AttentionKind::Reply,
            AttentionKind::RecoveryChoice,
            AttentionKind::RuntimePaused,
        ] {
            assert!(!serde_json::to_value(kind)
                .expect("attention kind")
                .is_null());
        }
    }

    #[test]
    fn reply_attention_has_no_invented_decision_kind() {
        let reply = AttentionItem {
            attention_id: "attention_reply_01".into(),
            kind: AttentionKind::Reply,
            decision_kind: None,
            subject: AttentionSubject::Delegation {
                delegation_id: "dlg_01".into(),
                delegation_revision: 7,
            },
            decision_id: None,
            reason: "A verified owner reply is needed to continue the conversation.".into(),
            recommendation: "Reply with the requested information.".into(),
            alternatives_or_actions: vec!["Reply".into()],
            assurance_required: AssuranceRequirement::VerifiedOwnerResolution,
            deadline_reminder_state: "not_scheduled".into(),
            deadline_at_ms: None,
            decision_revision: None,
            authoritative_deep_link: "carsinos://execass/delegations/dlg_01".into(),
        };
        let value = serde_json::to_value(&reply).expect("reply attention serializes");
        assert_eq!(value["decision_kind"], serde_json::Value::Null);

        let decision_backed = AttentionItem {
            kind: AttentionKind::Confirmation,
            decision_kind: Some(DecisionKind::DangerousActionConfirmation),
            decision_id: Some("dec_01".into()),
            decision_revision: Some(4),
            ..reply
        };
        let value = serde_json::to_value(&decision_backed).expect("decision attention serializes");
        assert_eq!(
            value["decision_kind"],
            serde_json::Value::String("dangerous_action_confirmation".into())
        );

        let runtime_paused = AttentionItem {
            attention_id: "attention_runtime_01".into(),
            kind: AttentionKind::RuntimePaused,
            decision_kind: None,
            subject: AttentionSubject::RuntimeHost {
                runtime_host_generation: 8,
                runtime_host_instance_id: "gateway:owner:instance".into(),
                runtime_fencing_token: 8,
                runtime_actual_state: RuntimeHostActualState::Faulted,
                runtime_end_reason: "forced_exit_detected".into(),
                active_work_binding_digest: format!("sha256:{}", "a".repeat(64)),
            },
            decision_id: None,
            reason: "The prior runtime stopped without completing its drain.".into(),
            recommendation: "Review the paused work and runtime recovery evidence.".into(),
            alternatives_or_actions: vec!["Review runtime recovery".into()],
            assurance_required: AssuranceRequirement::MechanicalResolution,
            deadline_reminder_state: "not_scheduled".into(),
            deadline_at_ms: None,
            decision_revision: None,
            authoritative_deep_link: "carsinos://execass/runtime-host/generations/8".into(),
        };
        let value = serde_json::to_value(runtime_paused).expect("runtime attention serializes");
        assert_eq!(value["subject"]["scope_kind"], "runtime_host");
        assert!(value["subject"].get("delegation_id").is_none());
    }

    #[test]
    fn ordinary_actions_and_recovery_remain_objectively_typed() {
        let action = schema_for!(ActionSummary).to_value();
        assert!(action["properties"]["required_decision_kind"]["anyOf"]
            .as_array()
            .expect("nullable decision-kind schema")
            .iter()
            .any(|branch| branch["type"] == "null"));
        let recovery = schema_for!(RecoverySummary).to_value().to_string();
        for prohibited in ["purpose", "morality", "commerce", "category", "risk_score"] {
            assert!(
                !recovery.contains(prohibited),
                "recovery exposes non-objective factor {prohibited}"
            );
        }
    }

    #[test]
    fn representative_roots_generate_json_schema() {
        let summary = schema_for!(SummaryResponse);
        let intake = schema_for!(IntakeResponse);
        let event = schema_for!(DurableEventEnvelope);
        let error = schema_for!(ApiError);
        let summary = summary.to_value();
        assert_eq!(
            summary["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert!(!intake.to_value().is_null());
        assert!(!event.to_value().is_null());
        assert!(!error.to_value().is_null());
    }
}
