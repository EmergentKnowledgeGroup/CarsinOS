//! Production actor-assurance gate for ExecAss owner ingress.
//!
//! This module deliberately does not resolve a decision, create an authority
//! row, mint a confirmation grant, or invoke an effect. It produces an opaque
//! capability only after server-owned evidence is verified. Later persistence
//! code can consume that capability in its single atomic transaction.

#![cfg_attr(not(test), allow(dead_code))]

use carsinos_core::execass_actor::{
    derive_local_owner_actor_assurance, derive_remote_owner_actor_assurance,
    owner_normalized_intent_digest, ActorAssurance, AuthenticatedLocalOwnerEvidence,
    AuthenticatedRemoteOwnerEvidence, CurrentDecisionBinding, DecisionResponseEvidence,
};
use carsinos_protocol::execass::{
    local_decision_proof_bytes, local_owner_intake_proof_bytes, local_owner_mutation_proof_bytes,
    local_run_control_request_proof_bytes, owner_instruction_digest,
    run_control_request_binding_bytes, ActorType, DecisionResult, LocalDecisionProofBinding,
};
pub(super) use carsinos_protocol::execass::{
    LocalDecisionProof as LocalNativeOwnerProof,
    LocalOwnerIntakeProof as LocalNativeOwnerIntentProof, LocalOwnerMutationBinding,
    LocalOwnerMutationProof, LocalRunControlProof as LocalNativeRunControlProof,
    RunControlOperation, RunControlRequestBinding as RunControlBinding,
    RunControlResumeSnapshot as RunControlResumeBinding, RunControlTarget,
};
use carsinos_storage::execass::SafeText;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

type HmacSha256 = Hmac<Sha256>;

const LOCAL_OWNER_INTAKE_INGRESS: &str = "native-owner-intake";
const REMOTE_RUN_CONTROL_EVIDENCE_DOMAIN: &[u8] =
    b"carsinos.execass.remote_run_control_evidence.v1";
const RUN_CONTROL_EVIDENCE_MAX_AGE_MS: i64 = 60_000;
const RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS: i64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActorGateFailure {
    LocalProofUnavailable,
    LocalProofInvalid,
    OwnerIdentityMismatch,
    ProviderNotEnabled,
    ExpiredChallenge,
    DecisionMismatch,
    DecisionRevisionMismatch,
    NormalizedIntentMismatch,
    PolicyRevisionMismatch,
    CanonicalManifestMismatch,
    SelectedLogicalActionMismatch,
    PresentedActionMismatch,
    DeclaredConsequenceMismatch,
    ChallengeMismatch,
    DecisionResultMismatch,
    CorrelationMismatch,
    SourceMessageMismatch,
    StaleCallback,
    NonHumanRunControl,
    RunControlBindingMismatch,
    StaleRunControlEvidence,
    InvalidEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BaseActorAssurance {
    actor_type: ActorType,
    credential_identity: String,
    source_message_id: Option<String>,
    request_correlation_id: String,
    may_submit_or_amend_owner_intent: bool,
    owner_actor_assurance: Option<ActorAssurance>,
    verified_normalized_intent_digest: Option<String>,
    verified_instruction_digest: Option<String>,
    verified_request_id: Option<String>,
    verified_idempotency_key: Option<String>,
    verified_attach_to_delegation_id: Option<String>,
    verified_provider: Option<String>,
    verified_conversation_id: Option<String>,
    verified_reply_to_message_id: Option<String>,
}

impl BaseActorAssurance {
    pub(super) fn actor_type(&self) -> ActorType {
        self.actor_type
    }

    pub(super) fn may_submit_or_amend_owner_intent(&self) -> bool {
        self.may_submit_or_amend_owner_intent
    }

    pub(super) fn source_message_id(&self) -> Option<&str> {
        self.source_message_id.as_deref()
    }

    pub(super) fn request_correlation_id(&self) -> &str {
        &self.request_correlation_id
    }

    pub(super) fn owner_actor_assurance(&self) -> Option<&ActorAssurance> {
        self.owner_actor_assurance.as_ref()
    }

    pub(super) fn verified_normalized_intent_digest(&self) -> Option<&str> {
        self.verified_normalized_intent_digest.as_deref()
    }

    pub(super) fn verified_instruction_digest(&self) -> Option<&str> {
        self.verified_instruction_digest.as_deref()
    }

    pub(super) fn verified_request_id(&self) -> Option<&str> {
        self.verified_request_id.as_deref()
    }

    pub(super) fn verified_idempotency_key(&self) -> Option<&str> {
        self.verified_idempotency_key.as_deref()
    }

    pub(super) fn verified_attach_to_delegation_id(&self) -> Option<&str> {
        self.verified_attach_to_delegation_id.as_deref()
    }

    pub(super) fn verified_provider(&self) -> Option<&str> {
        self.verified_provider.as_deref()
    }

    pub(super) fn verified_conversation_id(&self) -> Option<&str> {
        self.verified_conversation_id.as_deref()
    }

    pub(super) fn verified_reply_to_message_id(&self) -> Option<&str> {
        self.verified_reply_to_message_id.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VerifiedConfirmationEvent {
    actor_type: ActorType,
    credential_identity: String,
    authenticated_ingress: String,
    channel_assurance: String,
    request_correlation_id: String,
    source_message_id: Option<String>,
    provider_event_id: Option<String>,
    evidence_digest: String,
    owner_actor_assurance: Option<ActorAssurance>,
    verified_decision_binding: CurrentDecisionBinding,
    verified_decision_result: DecisionResult,
    verified_request_binding: Option<LocalDecisionProofBinding>,
}

/// Opaque, server-verified authority for one stop/resume request. Storage owns
/// replay consumption: `replay_identity` is stable but this gate never marks it
/// consumed before the atomic state transition transaction succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VerifiedRunControlEvent {
    actor_type: ActorType,
    credential_identity: String,
    authenticated_ingress: String,
    channel_assurance: String,
    operation: RunControlOperation,
    target: RunControlTarget,
    idempotency_key: String,
    request_correlation_id: String,
    observed_at_ms: i64,
    request_binding_digest: String,
    replay_identity: String,
    source_message_id: Option<String>,
    provider_event_id: Option<String>,
    resume: Option<RunControlResumeBinding>,
}

impl VerifiedRunControlEvent {
    pub(super) fn actor_type(&self) -> ActorType {
        self.actor_type
    }

    pub(super) fn credential_identity(&self) -> &str {
        &self.credential_identity
    }

    pub(super) fn authenticated_ingress(&self) -> &str {
        &self.authenticated_ingress
    }

    pub(super) fn channel_assurance(&self) -> &str {
        &self.channel_assurance
    }

    pub(super) fn operation(&self) -> RunControlOperation {
        self.operation
    }

    pub(super) fn target(&self) -> &RunControlTarget {
        &self.target
    }

    pub(super) fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub(super) fn request_correlation_id(&self) -> &str {
        &self.request_correlation_id
    }

    pub(super) fn observed_at_ms(&self) -> i64 {
        self.observed_at_ms
    }

    pub(super) fn request_binding_digest(&self) -> &str {
        &self.request_binding_digest
    }

    pub(super) fn replay_identity(&self) -> &str {
        &self.replay_identity
    }

    pub(super) fn source_message_id(&self) -> Option<&str> {
        self.source_message_id.as_deref()
    }

    pub(super) fn provider_event_id(&self) -> Option<&str> {
        self.provider_event_id.as_deref()
    }

    pub(super) fn resume(&self) -> Option<&RunControlResumeBinding> {
        self.resume.as_ref()
    }
}

impl VerifiedConfirmationEvent {
    pub(super) fn actor_type(&self) -> ActorType {
        self.actor_type
    }

    pub(super) fn credential_identity(&self) -> &str {
        &self.credential_identity
    }

    pub(super) fn authenticated_ingress(&self) -> &str {
        &self.authenticated_ingress
    }

    pub(super) fn channel_assurance(&self) -> &str {
        &self.channel_assurance
    }

    pub(super) fn request_correlation_id(&self) -> &str {
        &self.request_correlation_id
    }

    pub(super) fn source_message_id(&self) -> Option<&str> {
        self.source_message_id.as_deref()
    }

    pub(super) fn provider_event_id(&self) -> Option<&str> {
        self.provider_event_id.as_deref()
    }

    pub(super) fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }

    pub(super) fn owner_actor_assurance(&self) -> Option<&ActorAssurance> {
        self.owner_actor_assurance.as_ref()
    }

    pub(super) fn verified_decision_binding(&self) -> &CurrentDecisionBinding {
        &self.verified_decision_binding
    }

    pub(super) fn verified_decision_result(&self) -> DecisionResult {
        self.verified_decision_result
    }

    pub(super) fn verified_request_binding(&self) -> Option<&LocalDecisionProofBinding> {
        self.verified_request_binding.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UntrustedTransportAuthentication {
    pub auth_method: String,
    pub principal_id: String,
    pub claimed_operator_id: Option<String>,
    pub claimed_peer_id: Option<String>,
    pub claimed_actor_type: Option<String>,
    pub confirmation_text: Option<String>,
    pub request_correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteProviderOwnerEvent {
    provider: String,
    adapter_instance_id: String,
    observed_provider_account_id: String,
    conversation_id: String,
    source_message_id: String,
    provider_event_id: String,
    request_correlation_id: String,
    reply_to_message_id: Option<String>,
}

/// Trusted-provider event normalized by the Telegram or Discord listener. The
/// binding digest is derived server-side from parsed control syntax; it is not
/// accepted from HTTP headers, bearer claims, request bodies, models, workers,
/// or connectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteProviderRunControlEvent {
    provider: String,
    adapter_instance_id: String,
    observed_provider_account_id: String,
    conversation_id: String,
    source_message_id: String,
    provider_event_id: String,
    request_correlation_id: String,
    normalized_control_binding_digest: String,
}

#[cfg(test)]
pub(super) struct TestRemoteConfirmationEventInput<'a> {
    pub provider: &'a str,
    pub owner_id: &'a str,
    pub adapter_instance_id: &'a str,
    pub source_message_id: &'a str,
    pub provider_event_id: &'a str,
    pub current: &'a CurrentDecisionBinding,
    pub decision_result: DecisionResult,
    pub request_correlation_id: &'a str,
}

impl RemoteProviderOwnerEvent {
    /// This constructor is for the Telegram long-poll transport after the
    /// provider response has been authenticated by the configured bot client.
    pub(super) fn from_telegram_long_poll(
        adapter_instance_id: String,
        observed_provider_account_id: String,
        conversation_id: String,
        source_message_id: String,
        update_id: String,
        request_correlation_id: String,
    ) -> Self {
        Self {
            provider: "telegram".to_string(),
            adapter_instance_id,
            observed_provider_account_id,
            conversation_id,
            source_message_id,
            provider_event_id: update_id,
            request_correlation_id,
            reply_to_message_id: None,
        }
    }

    /// This constructor is for the authenticated Discord gateway/REST
    /// transport after CarsinOS itself obtained the provider event.
    pub(super) fn from_discord_gateway(
        adapter_instance_id: String,
        observed_provider_account_id: String,
        conversation_id: String,
        source_message_id: String,
        provider_event_id: String,
        request_correlation_id: String,
    ) -> Self {
        Self {
            provider: "discord".to_string(),
            adapter_instance_id,
            observed_provider_account_id,
            conversation_id,
            source_message_id,
            provider_event_id,
            request_correlation_id,
            reply_to_message_id: None,
        }
    }

    pub(super) fn with_reply_to_message_id(mut self, reply_to_message_id: Option<String>) -> Self {
        self.reply_to_message_id = reply_to_message_id;
        self
    }
}

impl RemoteProviderRunControlEvent {
    /// Construct only after Telegram's authenticated long-poll response was
    /// parsed into the exact canonical control binding.
    pub(super) fn from_telegram_long_poll(
        adapter_instance_id: String,
        observed_provider_account_id: String,
        conversation_id: String,
        source_message_id: String,
        update_id: String,
        request_correlation_id: String,
        normalized_control_binding_digest: String,
    ) -> Result<Self, ActorGateFailure> {
        Self {
            provider: "telegram".to_string(),
            adapter_instance_id,
            observed_provider_account_id,
            conversation_id,
            source_message_id,
            provider_event_id: update_id,
            request_correlation_id,
            normalized_control_binding_digest,
        }
        .validate()
    }

    /// Construct only after CarsinOS obtained and parsed an authenticated
    /// Discord gateway/REST event into the exact canonical control binding.
    pub(super) fn from_discord_gateway(
        adapter_instance_id: String,
        observed_provider_account_id: String,
        conversation_id: String,
        source_message_id: String,
        provider_event_id: String,
        request_correlation_id: String,
        normalized_control_binding_digest: String,
    ) -> Result<Self, ActorGateFailure> {
        Self {
            provider: "discord".to_string(),
            adapter_instance_id,
            observed_provider_account_id,
            conversation_id,
            source_message_id,
            provider_event_id,
            request_correlation_id,
            normalized_control_binding_digest,
        }
        .validate()
    }

    fn validate(self) -> Result<Self, ActorGateFailure> {
        require_present([
            self.provider.as_str(),
            self.adapter_instance_id.as_str(),
            self.observed_provider_account_id.as_str(),
            self.conversation_id.as_str(),
            self.source_message_id.as_str(),
            self.provider_event_id.as_str(),
            self.request_correlation_id.as_str(),
            self.normalized_control_binding_digest.as_str(),
        ])?;
        if !matches!(self.provider.as_str(), "telegram" | "discord")
            || decode_hex(&self.normalized_control_binding_digest).is_none()
        {
            return Err(ActorGateFailure::InvalidEvidence);
        }
        Ok(self)
    }
}

#[derive(Clone)]
pub(super) struct ExecAssActorGate {
    local_native_secret: Option<Arc<[u8]>>,
    remote_owner_accounts: Arc<HashMap<String, String>>,
    replay_directory: Arc<PathBuf>,
}

impl std::fmt::Debug for ExecAssActorGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExecAssActorGate")
            .field(
                "local_native_proof_configured",
                &self.local_native_secret.is_some(),
            )
            .field(
                "configured_remote_providers",
                &self.remote_owner_accounts.keys().collect::<Vec<_>>(),
            )
            .field("durable_replay_directory", &self.replay_directory)
            .finish_non_exhaustive()
    }
}

impl ExecAssActorGate {
    pub(super) fn new(
        local_native_secret: Option<Vec<u8>>,
        remote_owner_accounts: impl IntoIterator<Item = (String, String)>,
        replay_directory: PathBuf,
    ) -> Self {
        Self {
            local_native_secret: local_native_secret.map(Arc::from),
            remote_owner_accounts: Arc::new(
                remote_owner_accounts
                    .into_iter()
                    .map(|(provider, owner)| {
                        (
                            provider.trim().to_ascii_lowercase(),
                            owner.trim().to_string(),
                        )
                    })
                    .filter(|(provider, owner)| !provider.is_empty() && !owner.is_empty())
                    .collect(),
            ),
            replay_directory: Arc::new(replay_directory),
        }
    }

    pub(super) fn configured_remote_owner(&self, provider: &str) -> Option<&str> {
        self.remote_owner_accounts
            .get(&provider.trim().to_ascii_lowercase())
            .map(String::as_str)
    }

    /// A bearer, JWT, service credential, header, body claim, or convincing
    /// text remains non-human. This function intentionally ignores all claimed
    /// owner facts.
    pub(super) fn classify_untrusted_transport(
        &self,
        authentication: &UntrustedTransportAuthentication,
    ) -> BaseActorAssurance {
        let actor_type = if authentication.auth_method == "internal_ingest" {
            ActorType::Connector
        } else {
            ActorType::Runtime
        };
        BaseActorAssurance {
            actor_type,
            credential_identity: authentication.principal_id.clone(),
            source_message_id: None,
            request_correlation_id: authentication.request_correlation_id.clone(),
            may_submit_or_amend_owner_intent: false,
            owner_actor_assurance: None,
            verified_normalized_intent_digest: None,
            verified_instruction_digest: None,
            verified_request_id: None,
            verified_idempotency_key: None,
            verified_attach_to_delegation_id: None,
            verified_provider: None,
            verified_conversation_id: None,
            verified_reply_to_message_id: None,
        }
    }

    /// Verify one exact native-shell local-owner intake proof without
    /// inventing a decision nonce or accepting caller-selected authority.
    pub(super) fn verify_local_owner_intake(
        &self,
        proof: &LocalNativeOwnerIntentProof,
        exact_owner_text: &str,
    ) -> Result<BaseActorAssurance, ActorGateFailure> {
        let secret = self
            .local_native_secret
            .as_deref()
            .ok_or(ActorGateFailure::LocalProofUnavailable)?;
        proof
            .validate()
            .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        require_present([
            proof.authenticated_client_id.as_str(),
            proof.request_correlation_id.as_str(),
            proof.request_id.as_str(),
            proof.idempotency_key.as_str(),
            proof.normalized_intent_digest.as_str(),
            proof.instruction_digest.as_str(),
            proof.proof_hex.as_str(),
        ])?;
        let instruction_digest = owner_instruction_digest(exact_owner_text.as_bytes())
            .ok_or(ActorGateFailure::LocalProofInvalid)?;
        if proof.instruction_digest != instruction_digest {
            return Err(ActorGateFailure::LocalProofInvalid);
        }
        let evidence = local_owner_intake_proof_bytes(proof)
            .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        verify_mac(secret, &evidence, &proof.proof_hex)?;
        let core_evidence = AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
            proof.authenticated_client_id.clone(),
            LOCAL_OWNER_INTAKE_INGRESS,
            "interactive-local",
            proof.request_correlation_id.clone(),
        )
        .map_err(|_| ActorGateFailure::InvalidEvidence)?;
        Ok(BaseActorAssurance {
            actor_type: ActorType::HumanLocal,
            credential_identity: proof.authenticated_client_id.clone(),
            source_message_id: None,
            request_correlation_id: proof.request_correlation_id.clone(),
            may_submit_or_amend_owner_intent: true,
            owner_actor_assurance: Some(derive_local_owner_actor_assurance(core_evidence)),
            verified_normalized_intent_digest: Some(proof.normalized_intent_digest.clone()),
            verified_instruction_digest: Some(instruction_digest),
            verified_request_id: Some(proof.request_id.clone()),
            verified_idempotency_key: Some(proof.idempotency_key.clone()),
            verified_attach_to_delegation_id: proof.attach_to_delegation_id.clone(),
            verified_provider: None,
            verified_conversation_id: None,
            verified_reply_to_message_id: None,
        })
    }

    /// Verify a native owner HMAC over one closed settings operation and its
    /// exact server-reconstructed request binding. Replay is consumed only by
    /// the canonical storage transaction.
    pub(super) fn verify_local_owner_mutation(
        &self,
        proof: &LocalOwnerMutationProof,
        binding: &LocalOwnerMutationBinding,
    ) -> Result<BaseActorAssurance, ActorGateFailure> {
        let secret = self
            .local_native_secret
            .as_deref()
            .ok_or(ActorGateFailure::LocalProofUnavailable)?;
        proof
            .validate()
            .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        binding
            .validate()
            .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        if binding.created_at_ms
            > Utc::now()
                .timestamp_millis()
                .saturating_add(RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS)
        {
            return Err(ActorGateFailure::LocalProofInvalid);
        }
        if proof.request_correlation_id != binding.request_correlation_id {
            return Err(ActorGateFailure::CorrelationMismatch);
        }
        let evidence = local_owner_mutation_proof_bytes(proof, binding)
            .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        verify_mac(secret, &evidence, &proof.proof_hex)?;
        let core_evidence = AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
            proof.authenticated_client_id.clone(),
            "native-control",
            "interactive-local",
            proof.request_correlation_id.clone(),
        )
        .map_err(|_| ActorGateFailure::InvalidEvidence)?;
        Ok(BaseActorAssurance {
            actor_type: ActorType::HumanLocal,
            credential_identity: proof.authenticated_client_id.clone(),
            source_message_id: None,
            request_correlation_id: proof.request_correlation_id.clone(),
            may_submit_or_amend_owner_intent: true,
            owner_actor_assurance: Some(derive_local_owner_actor_assurance(core_evidence)),
            verified_normalized_intent_digest: None,
            verified_instruction_digest: None,
            verified_request_id: None,
            verified_idempotency_key: Some(binding.idempotency_key.clone()),
            verified_attach_to_delegation_id: None,
            verified_provider: None,
            verified_conversation_id: None,
            verified_reply_to_message_id: None,
        })
    }

    pub(super) fn verify_local_decision(
        &self,
        proof: &LocalNativeOwnerProof,
        current: &CurrentDecisionBinding,
        response: &DecisionResponseEvidence,
    ) -> Result<VerifiedConfirmationEvent, ActorGateFailure> {
        let binding = local_decision_binding(current, response);
        self.verify_local_decision_with_binding(proof, current, response, &binding)
    }

    pub(super) fn verify_local_decision_with_binding(
        &self,
        proof: &LocalNativeOwnerProof,
        current: &CurrentDecisionBinding,
        response: &DecisionResponseEvidence,
        binding: &LocalDecisionProofBinding,
    ) -> Result<VerifiedConfirmationEvent, ActorGateFailure> {
        let secret = self
            .local_native_secret
            .as_deref()
            .ok_or(ActorGateFailure::LocalProofUnavailable)?;
        require_present([
            proof.authenticated_client_id.as_str(),
            proof.request_correlation_id.as_str(),
            proof.proof_hex.as_str(),
        ])?;
        verify_common_binding(current, response, Utc::now().timestamp_millis())?;
        if response.source_message_id.is_some()
            || response.request_correlation_id != proof.request_correlation_id
        {
            return Err(ActorGateFailure::CorrelationMismatch);
        }
        let expected = LocalDecisionProofBinding {
            idempotency_key: binding.idempotency_key.clone(),
            revision_text_digest: binding.revision_text_digest.clone(),
            challenge_response_digest: binding.challenge_response_digest.clone(),
            ..local_decision_binding(current, response)
        };
        if binding != &expected {
            return Err(ActorGateFailure::DecisionMismatch);
        }
        let evidence = local_decision_proof_bytes(proof, binding);
        verify_mac(secret, &evidence, &proof.proof_hex)?;
        let evidence_digest = digest_hex(&evidence);
        let core_evidence = AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
            proof.authenticated_client_id.clone(),
            "native-control",
            "interactive-local",
            proof.request_correlation_id.clone(),
        )
        .map_err(|_| ActorGateFailure::InvalidEvidence)?;
        Ok(VerifiedConfirmationEvent {
            actor_type: ActorType::HumanLocal,
            credential_identity: proof.authenticated_client_id.clone(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: proof.request_correlation_id.clone(),
            source_message_id: None,
            provider_event_id: None,
            evidence_digest,
            owner_actor_assurance: Some(derive_local_owner_actor_assurance(core_evidence)),
            verified_decision_binding: current.clone(),
            verified_decision_result: response.decision_result,
            verified_request_binding: Some(binding.clone()),
        })
    }

    /// Verifies a locally interactive owner proof for one exact stop/resume
    /// binding. It intentionally does not create a replay marker: storage must
    /// bind `replay_identity` in the same atomic transaction as the transition.
    pub(super) fn verify_local_run_control(
        &self,
        proof: &LocalNativeRunControlProof,
        binding: &RunControlBinding,
    ) -> Result<VerifiedRunControlEvent, ActorGateFailure> {
        let secret = self
            .local_native_secret
            .as_deref()
            .ok_or(ActorGateFailure::LocalProofUnavailable)?;
        require_present([
            proof.authenticated_client_id.as_str(),
            proof.request_correlation_id.as_str(),
            proof.proof_hex.as_str(),
        ])?;
        proof
            .validate()
            .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        verify_run_control_binding(binding, Utc::now().timestamp_millis())?;
        if proof.request_correlation_id != binding.request_correlation_id {
            return Err(ActorGateFailure::CorrelationMismatch);
        }
        ensure_human_run_control(ActorType::HumanLocal)?;
        let evidence =
            local_run_control_request_proof_bytes(&proof.authenticated_client_id, binding)
                .map_err(|_| ActorGateFailure::RunControlBindingMismatch)?;
        verify_mac(secret, &evidence, &proof.proof_hex)?;
        let request_binding_digest = binding
            .try_request_binding_digest()
            .map_err(|_| ActorGateFailure::RunControlBindingMismatch)?;
        Ok(verified_run_control_event(
            VerifiedRunControlActorEvidence {
                actor_type: ActorType::HumanLocal,
                credential_identity: proof.authenticated_client_id.clone(),
                authenticated_ingress: "native-control".to_string(),
                channel_assurance: "interactive-local".to_string(),
                source_message_id: None,
                provider_event_id: None,
            },
            binding,
            &evidence,
            request_binding_digest,
        ))
    }

    /// Translate an already authenticated, app-bound native close into the
    /// existing human-local global-stop authority. The proof is minted and
    /// immediately verified inside this boundary; no caller-selected actor,
    /// target, operation, or resume authority can enter this path.
    pub(super) fn verify_authenticated_runtime_shutdown(
        &self,
        authorization: &carsinos_runtime_control::ShutdownAuthorizationV1,
        observed_at_ms: i64,
    ) -> Result<VerifiedRunControlEvent, ActorGateFailure> {
        let secret = self
            .local_native_secret
            .as_deref()
            .ok_or(ActorGateFailure::LocalProofUnavailable)?;
        if decode_hex(&authorization.authorization_id).is_none()
            || decode_hex(&authorization.request_id).is_none()
            || authorization.runtime_host_generation <= 0
            || observed_at_ms <= 0
        {
            return Err(ActorGateFailure::InvalidEvidence);
        }
        let binding = RunControlBinding::global_stop(
            format!("runtime-close-{}", authorization.authorization_id),
            authorization.request_id.clone(),
            observed_at_ms,
        )
        .map_err(|_| ActorGateFailure::RunControlBindingMismatch)?;
        let evidence =
            local_run_control_request_proof_bytes(&authorization.client_instance_id, &binding)
                .map_err(|_| ActorGateFailure::RunControlBindingMismatch)?;
        let mut mac =
            HmacSha256::new_from_slice(secret).map_err(|_| ActorGateFailure::InvalidEvidence)?;
        mac.update(&evidence);
        let proof = LocalNativeRunControlProof::from_authenticated_native_request(
            authorization.client_instance_id.clone(),
            authorization.request_id.clone(),
            format!("{:x}", mac.finalize().into_bytes()),
        )
        .map_err(|_| ActorGateFailure::LocalProofInvalid)?;
        self.verify_local_run_control(&proof, &binding)
    }

    /// Base remote owner intake is derived only from a provider event obtained
    /// by the trusted listener and one exact configured owner identity. It does
    /// not consume the event; duplicate intake remains the storage idempotency
    /// layer's responsibility.
    pub(super) fn classify_remote_owner_intake(
        &self,
        event: &RemoteProviderOwnerEvent,
        exact_owner_text: &str,
    ) -> Result<BaseActorAssurance, ActorGateFailure> {
        require_present([
            event.provider.as_str(),
            event.adapter_instance_id.as_str(),
            event.observed_provider_account_id.as_str(),
            event.conversation_id.as_str(),
            event.source_message_id.as_str(),
            event.provider_event_id.as_str(),
            event.request_correlation_id.as_str(),
        ])?;
        if event.reply_to_message_id.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.trim() != value
                || value.len() > 256
                || value.chars().any(char::is_control)
        }) {
            return Err(ActorGateFailure::InvalidEvidence);
        }
        let configured_owner = self
            .configured_remote_owner(&event.provider)
            .ok_or(ActorGateFailure::ProviderNotEnabled)?;
        if configured_owner != event.observed_provider_account_id {
            return Err(ActorGateFailure::OwnerIdentityMismatch);
        }
        let safe_intent = SafeText::new(exact_owner_text.trim(), &[])
            .map_err(|_| ActorGateFailure::InvalidEvidence)?;
        let normalized_intent_digest = owner_normalized_intent_digest(safe_intent.as_str())
            .ok_or(ActorGateFailure::InvalidEvidence)?;
        let instruction_digest = owner_instruction_digest(exact_owner_text.as_bytes())
            .ok_or(ActorGateFailure::InvalidEvidence)?;
        let observed_at = Utc::now().timestamp_millis();
        let core_evidence = AuthenticatedRemoteOwnerEvidence::from_authenticated_provider_event(
            event.adapter_instance_id.clone(),
            configured_owner.to_string(),
            event.observed_provider_account_id.clone(),
            event.adapter_instance_id.clone(),
            format!("authenticated-{}-provider-event", event.provider),
            event.source_message_id.clone(),
            event.provider_event_id.clone(),
            event.request_correlation_id.clone(),
            observed_at,
            observed_at,
        )
        .map_err(|_| ActorGateFailure::InvalidEvidence)?;
        Ok(BaseActorAssurance {
            actor_type: ActorType::HumanRemote,
            credential_identity: format!(
                "{}:{}",
                event.provider, event.observed_provider_account_id
            ),
            source_message_id: Some(event.source_message_id.clone()),
            request_correlation_id: event.request_correlation_id.clone(),
            may_submit_or_amend_owner_intent: true,
            owner_actor_assurance: Some(derive_remote_owner_actor_assurance(core_evidence)),
            verified_normalized_intent_digest: Some(normalized_intent_digest),
            verified_instruction_digest: Some(instruction_digest),
            verified_request_id: Some(event.source_message_id.clone()),
            verified_idempotency_key: Some(event.provider_event_id.clone()),
            verified_attach_to_delegation_id: None,
            verified_provider: Some(event.provider.clone()),
            verified_conversation_id: Some(event.conversation_id.clone()),
            verified_reply_to_message_id: event.reply_to_message_id.clone(),
        })
    }

    pub(super) fn verify_remote_decision(
        &self,
        event: &RemoteProviderOwnerEvent,
        current: &CurrentDecisionBinding,
        response: &DecisionResponseEvidence,
    ) -> Result<VerifiedConfirmationEvent, ActorGateFailure> {
        require_present([
            event.provider.as_str(),
            event.adapter_instance_id.as_str(),
            event.observed_provider_account_id.as_str(),
            event.conversation_id.as_str(),
            event.source_message_id.as_str(),
            event.provider_event_id.as_str(),
            event.request_correlation_id.as_str(),
        ])?;
        let configured_owner = self
            .configured_remote_owner(&event.provider)
            .ok_or(ActorGateFailure::ProviderNotEnabled)?;
        if configured_owner != event.observed_provider_account_id {
            return Err(ActorGateFailure::OwnerIdentityMismatch);
        }
        verify_common_binding(current, response, Utc::now().timestamp_millis())?;
        if response.request_correlation_id != event.request_correlation_id {
            return Err(ActorGateFailure::CorrelationMismatch);
        }
        if response.source_message_id.as_deref() != Some(event.source_message_id.as_str()) {
            return Err(ActorGateFailure::SourceMessageMismatch);
        }
        let evidence = remote_evidence_bytes(event, current, response);
        let evidence_digest = digest_hex(&evidence);
        Ok(VerifiedConfirmationEvent {
            actor_type: ActorType::HumanRemote,
            credential_identity: format!(
                "{}:{}",
                event.provider, event.observed_provider_account_id
            ),
            authenticated_ingress: event.adapter_instance_id.clone(),
            channel_assurance: format!("authenticated-{}-provider-event", event.provider),
            request_correlation_id: event.request_correlation_id.clone(),
            source_message_id: Some(event.source_message_id.clone()),
            provider_event_id: Some(event.provider_event_id.clone()),
            evidence_digest,
            owner_actor_assurance: None,
            verified_decision_binding: current.clone(),
            verified_decision_result: response.decision_result,
            verified_request_binding: None,
        })
    }

    /// Derives one human run-control capability from a normalized, authenticated
    /// Telegram/Discord event. The provider event must carry the server-derived
    /// digest of this exact binding; no generic transport actor can reach here.
    pub(super) fn verify_remote_run_control(
        &self,
        event: &RemoteProviderRunControlEvent,
        binding: &RunControlBinding,
    ) -> Result<VerifiedRunControlEvent, ActorGateFailure> {
        require_present([
            event.provider.as_str(),
            event.adapter_instance_id.as_str(),
            event.observed_provider_account_id.as_str(),
            event.conversation_id.as_str(),
            event.source_message_id.as_str(),
            event.provider_event_id.as_str(),
            event.request_correlation_id.as_str(),
            event.normalized_control_binding_digest.as_str(),
        ])?;
        if !matches!(event.provider.as_str(), "telegram" | "discord") {
            return Err(ActorGateFailure::ProviderNotEnabled);
        }
        let configured_owner = self
            .configured_remote_owner(&event.provider)
            .ok_or(ActorGateFailure::ProviderNotEnabled)?;
        if configured_owner != event.observed_provider_account_id {
            return Err(ActorGateFailure::OwnerIdentityMismatch);
        }
        verify_run_control_binding(binding, Utc::now().timestamp_millis())?;
        if event.request_correlation_id != binding.request_correlation_id {
            return Err(ActorGateFailure::CorrelationMismatch);
        }
        let request_binding_digest = binding
            .try_request_binding_digest()
            .map_err(|_| ActorGateFailure::RunControlBindingMismatch)?;
        if event.normalized_control_binding_digest != request_binding_digest {
            return Err(ActorGateFailure::RunControlBindingMismatch);
        }
        ensure_human_run_control(ActorType::HumanRemote)?;
        let evidence = remote_run_control_evidence_bytes(event, binding)?;
        Ok(verified_run_control_event(
            VerifiedRunControlActorEvidence {
                actor_type: ActorType::HumanRemote,
                credential_identity: format!(
                    "{}:{}",
                    event.provider, event.observed_provider_account_id
                ),
                authenticated_ingress: event.adapter_instance_id.clone(),
                channel_assurance: format!("authenticated-{}-provider-event", event.provider),
                source_message_id: Some(event.source_message_id.clone()),
                provider_event_id: Some(event.provider_event_id.clone()),
            },
            binding,
            &evidence,
            request_binding_digest,
        ))
    }

    #[cfg(test)]
    pub(super) fn issue_test_local_confirmation_event(
        &self,
        current: &CurrentDecisionBinding,
        decision_result: DecisionResult,
        request_correlation_id: &str,
    ) -> Result<VerifiedConfirmationEvent, ActorGateFailure> {
        self.issue_test_local_confirmation_event_with_request(
            current,
            decision_result,
            request_correlation_id,
            "",
            None,
            None,
        )
    }

    #[cfg(test)]
    pub(super) fn issue_test_local_confirmation_event_with_request(
        &self,
        current: &CurrentDecisionBinding,
        decision_result: DecisionResult,
        request_correlation_id: &str,
        idempotency_key: &str,
        revision_text_digest: Option<String>,
        challenge_response_digest: Option<String>,
    ) -> Result<VerifiedConfirmationEvent, ActorGateFailure> {
        let response = DecisionResponseEvidence {
            decision_id: current.decision_id.clone(),
            decision_revision: current.decision_revision,
            normalized_intent_digest: current.normalized_intent_digest.clone(),
            policy_revision: current.policy_revision,
            canonical_manifest_digest: current.canonical_manifest_digest.clone(),
            selected_logical_action_id: current.selected_logical_action_id.clone(),
            presented_action_digest: current.presented_action_digest.clone(),
            declared_consequence_digest: current.declared_consequence_digest.clone(),
            challenge_digest: current.challenge_digest.clone(),
            decision_result,
            observed_at_ms: Utc::now().timestamp_millis(),
            request_correlation_id: request_correlation_id.to_string(),
            source_message_id: None,
            callback_fresh: true,
        };
        let mut proof = LocalNativeOwnerProof {
            authenticated_client_id: "test-native-owner".to_string(),
            request_correlation_id: request_correlation_id.to_string(),
            proof_hex: String::new(),
        };
        let secret = self
            .local_native_secret
            .as_deref()
            .ok_or(ActorGateFailure::LocalProofUnavailable)?;
        let mut mac =
            HmacSha256::new_from_slice(secret).map_err(|_| ActorGateFailure::InvalidEvidence)?;
        let binding = LocalDecisionProofBinding {
            idempotency_key: idempotency_key.to_string(),
            revision_text_digest,
            challenge_response_digest,
            ..local_decision_binding(current, &response)
        };
        mac.update(&local_decision_proof_bytes(&proof, &binding));
        proof.proof_hex = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.verify_local_decision_with_binding(&proof, current, &response, &binding)
    }

    #[cfg(test)]
    pub(super) fn issue_test_remote_confirmation_event(
        &self,
        input: TestRemoteConfirmationEventInput<'_>,
    ) -> Result<VerifiedConfirmationEvent, ActorGateFailure> {
        let event = RemoteProviderOwnerEvent {
            provider: input.provider.to_string(),
            adapter_instance_id: input.adapter_instance_id.to_string(),
            observed_provider_account_id: input.owner_id.to_string(),
            conversation_id: "test-owner-conversation".to_string(),
            source_message_id: input.source_message_id.to_string(),
            provider_event_id: input.provider_event_id.to_string(),
            request_correlation_id: input.request_correlation_id.to_string(),
            reply_to_message_id: None,
        };
        let response = DecisionResponseEvidence {
            decision_id: input.current.decision_id.clone(),
            decision_revision: input.current.decision_revision,
            normalized_intent_digest: input.current.normalized_intent_digest.clone(),
            policy_revision: input.current.policy_revision,
            canonical_manifest_digest: input.current.canonical_manifest_digest.clone(),
            selected_logical_action_id: input.current.selected_logical_action_id.clone(),
            presented_action_digest: input.current.presented_action_digest.clone(),
            declared_consequence_digest: input.current.declared_consequence_digest.clone(),
            challenge_digest: input.current.challenge_digest.clone(),
            decision_result: input.decision_result,
            observed_at_ms: Utc::now().timestamp_millis(),
            request_correlation_id: input.request_correlation_id.to_string(),
            source_message_id: Some(input.source_message_id.to_string()),
            callback_fresh: true,
        };
        self.verify_remote_decision(&event, input.current, &response)
    }
}

fn verify_common_binding(
    current: &CurrentDecisionBinding,
    response: &DecisionResponseEvidence,
    server_now_ms: i64,
) -> Result<(), ActorGateFailure> {
    if !response.callback_fresh {
        return Err(ActorGateFailure::StaleCallback);
    }
    if server_now_ms >= current.expires_at_ms || response.observed_at_ms >= current.expires_at_ms {
        return Err(ActorGateFailure::ExpiredChallenge);
    }
    if response.decision_id != current.decision_id {
        return Err(ActorGateFailure::DecisionMismatch);
    }
    if response.decision_revision != current.decision_revision {
        return Err(ActorGateFailure::DecisionRevisionMismatch);
    }
    if response.normalized_intent_digest != current.normalized_intent_digest {
        return Err(ActorGateFailure::NormalizedIntentMismatch);
    }
    if response.policy_revision != current.policy_revision {
        return Err(ActorGateFailure::PolicyRevisionMismatch);
    }
    if response.canonical_manifest_digest != current.canonical_manifest_digest {
        return Err(ActorGateFailure::CanonicalManifestMismatch);
    }
    if response.selected_logical_action_id != current.selected_logical_action_id {
        return Err(ActorGateFailure::SelectedLogicalActionMismatch);
    }
    if response.presented_action_digest != current.presented_action_digest {
        return Err(ActorGateFailure::PresentedActionMismatch);
    }
    if response.declared_consequence_digest != current.declared_consequence_digest {
        return Err(ActorGateFailure::DeclaredConsequenceMismatch);
    }
    if response.challenge_digest != current.challenge_digest {
        return Err(ActorGateFailure::ChallengeMismatch);
    }
    if !matches!(
        response.decision_result,
        DecisionResult::ConfirmAndContinue
            | DecisionResult::Revise
            | DecisionResult::Decline
            | DecisionResult::Stop
    ) {
        return Err(ActorGateFailure::DecisionResultMismatch);
    }
    Ok(())
}

fn ensure_human_run_control(actor_type: ActorType) -> Result<(), ActorGateFailure> {
    if matches!(actor_type, ActorType::HumanLocal | ActorType::HumanRemote) {
        Ok(())
    } else {
        Err(ActorGateFailure::NonHumanRunControl)
    }
}

fn verify_run_control_binding(
    binding: &RunControlBinding,
    server_now_ms: i64,
) -> Result<(), ActorGateFailure> {
    validate_run_control_binding_shape(binding)?;
    let evidence_age_ms = server_now_ms.saturating_sub(binding.observed_at_ms);
    if binding.observed_at_ms
        > server_now_ms.saturating_add(RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS)
        || evidence_age_ms > RUN_CONTROL_EVIDENCE_MAX_AGE_MS
    {
        return Err(ActorGateFailure::StaleRunControlEvidence);
    }
    Ok(())
}

fn validate_run_control_binding_shape(binding: &RunControlBinding) -> Result<(), ActorGateFailure> {
    binding
        .validate()
        .map_err(|_| ActorGateFailure::RunControlBindingMismatch)
}

struct VerifiedRunControlActorEvidence {
    actor_type: ActorType,
    credential_identity: String,
    authenticated_ingress: String,
    channel_assurance: String,
    source_message_id: Option<String>,
    provider_event_id: Option<String>,
}

fn verified_run_control_event(
    actor: VerifiedRunControlActorEvidence,
    binding: &RunControlBinding,
    evidence: &[u8],
    request_binding_digest: String,
) -> VerifiedRunControlEvent {
    VerifiedRunControlEvent {
        actor_type: actor.actor_type,
        credential_identity: actor.credential_identity,
        authenticated_ingress: actor.authenticated_ingress,
        channel_assurance: actor.channel_assurance,
        operation: binding.operation,
        target: binding.target.clone(),
        idempotency_key: binding.idempotency_key.clone(),
        request_correlation_id: binding.request_correlation_id.clone(),
        observed_at_ms: binding.observed_at_ms,
        request_binding_digest,
        // This is deliberately a digest of verified evidence, not a consumed
        // marker. The caller persists/deduplicates it with the state change.
        replay_identity: digest_hex(evidence),
        source_message_id: actor.source_message_id,
        provider_event_id: actor.provider_event_id,
        resume: binding.resume.clone(),
    }
}

fn require_present<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<(), ActorGateFailure> {
    if values.into_iter().all(|value| !value.trim().is_empty()) {
        Ok(())
    } else {
        Err(ActorGateFailure::InvalidEvidence)
    }
}

fn local_decision_binding(
    current: &CurrentDecisionBinding,
    response: &DecisionResponseEvidence,
) -> LocalDecisionProofBinding {
    LocalDecisionProofBinding {
        decision_id: current.decision_id.clone(),
        decision_revision: current.decision_revision,
        normalized_intent_digest: current.normalized_intent_digest.clone(),
        policy_revision: current.policy_revision,
        canonical_manifest_digest: current.canonical_manifest_digest.clone(),
        selected_logical_action_id: current.selected_logical_action_id.clone(),
        presented_action_digest: current.presented_action_digest.clone(),
        declared_consequence_digest: current.declared_consequence_digest.clone(),
        challenge_digest: current.challenge_digest.clone(),
        expires_at_ms: current.expires_at_ms,
        response_selected_logical_action_id: response.selected_logical_action_id.clone(),
        decision_result: response.decision_result,
        idempotency_key: String::new(),
        revision_text_digest: None,
        challenge_response_digest: None,
        observed_at_ms: response.observed_at_ms,
    }
}

fn remote_evidence_bytes(
    event: &RemoteProviderOwnerEvent,
    current: &CurrentDecisionBinding,
    response: &DecisionResponseEvidence,
) -> Vec<u8> {
    let mut out = Vec::new();
    push(&mut out, b"carsinos.execass.remote_owner_evidence.v1");
    for value in [
        event.provider.as_str(),
        event.adapter_instance_id.as_str(),
        event.observed_provider_account_id.as_str(),
        event.conversation_id.as_str(),
        event.source_message_id.as_str(),
        event.provider_event_id.as_str(),
        event.request_correlation_id.as_str(),
    ] {
        push(&mut out, value.as_bytes());
    }
    push_common(&mut out, current, response);
    out
}

fn remote_run_control_evidence_bytes(
    event: &RemoteProviderRunControlEvent,
    binding: &RunControlBinding,
) -> Result<Vec<u8>, ActorGateFailure> {
    let mut out = Vec::new();
    push(&mut out, REMOTE_RUN_CONTROL_EVIDENCE_DOMAIN);
    for value in [
        event.provider.as_str(),
        event.adapter_instance_id.as_str(),
        event.observed_provider_account_id.as_str(),
        event.conversation_id.as_str(),
        event.source_message_id.as_str(),
        event.provider_event_id.as_str(),
        event.request_correlation_id.as_str(),
        event.normalized_control_binding_digest.as_str(),
    ] {
        push(&mut out, value.as_bytes());
    }
    out.extend_from_slice(
        &run_control_request_binding_bytes(binding)
            .map_err(|_| ActorGateFailure::RunControlBindingMismatch)?,
    );
    Ok(out)
}

fn push_common(
    out: &mut Vec<u8>,
    current: &CurrentDecisionBinding,
    response: &DecisionResponseEvidence,
) {
    push(out, current.decision_id.as_bytes());
    out.extend_from_slice(&current.decision_revision.to_be_bytes());
    push(out, current.normalized_intent_digest.as_bytes());
    out.extend_from_slice(&current.policy_revision.to_be_bytes());
    push(out, current.canonical_manifest_digest.as_bytes());
    push(out, current.selected_logical_action_id.as_bytes());
    push(out, current.presented_action_digest.as_bytes());
    push(out, current.declared_consequence_digest.as_bytes());
    push(out, current.challenge_digest.as_bytes());
    out.extend_from_slice(&current.expires_at_ms.to_be_bytes());
    push(out, response.selected_logical_action_id.as_bytes());
    out.push(decision_result_code(response.decision_result));
    out.extend_from_slice(&response.observed_at_ms.to_be_bytes());
}

fn push(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn decision_result_code(value: DecisionResult) -> u8 {
    match value {
        DecisionResult::ConfirmAndContinue => 1,
        DecisionResult::Revise => 2,
        DecisionResult::Decline => 3,
        DecisionResult::Stop => 4,
    }
}

fn verify_mac(secret: &[u8], bytes: &[u8], proof_hex: &str) -> Result<(), ActorGateFailure> {
    let proof = decode_hex(proof_hex).ok_or(ActorGateFailure::LocalProofInvalid)?;
    let mut mac =
        HmacSha256::new_from_slice(secret).map_err(|_| ActorGateFailure::InvalidEvidence)?;
    mac.update(bytes);
    mac.verify_slice(&proof)
        .map_err(|_| ActorGateFailure::LocalProofInvalid)
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() != 64 || !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execass_intake::{ExecAssIntakeService, IntakeAuthorityFailure};
    use carsinos_protocol::execass::IntakeRequest;
    use std::ops::Deref;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    const LOCAL_SECRET: &[u8] = b"test-native-owner-secret-with-32-bytes";

    struct GateFixture {
        _temp_dir: TempDir,
        gate: ExecAssActorGate,
    }

    impl Deref for GateFixture {
        type Target = ExecAssActorGate;

        fn deref(&self) -> &Self::Target {
            &self.gate
        }
    }

    fn gate_at(replay_directory: PathBuf) -> ExecAssActorGate {
        ExecAssActorGate::new(
            Some(LOCAL_SECRET.to_vec()),
            [
                ("telegram".to_string(), "owner-1".to_string()),
                ("discord".to_string(), "discord-owner".to_string()),
            ],
            replay_directory,
        )
    }

    fn gate() -> GateFixture {
        let temp_dir =
            TempDir::new_in(env!("CARGO_MANIFEST_DIR")).expect("actor gate temp directory");
        let gate = gate_at(temp_dir.path().join("replay"));
        GateFixture {
            _temp_dir: temp_dir,
            gate,
        }
    }

    fn current() -> CurrentDecisionBinding {
        CurrentDecisionBinding {
            decision_id: "decision-1".to_string(),
            decision_revision: 7,
            normalized_intent_digest: "intent-digest".to_string(),
            policy_revision: 11,
            canonical_manifest_digest: "manifest-digest".to_string(),
            selected_logical_action_id: "action-1".to_string(),
            presented_action_digest: "action-digest".to_string(),
            declared_consequence_digest: "consequence-digest".to_string(),
            challenge_digest: "challenge-digest".to_string(),
            expires_at_ms: 4_000_000_001_000,
        }
    }

    fn response(remote: bool) -> DecisionResponseEvidence {
        DecisionResponseEvidence {
            decision_id: "decision-1".to_string(),
            decision_revision: 7,
            normalized_intent_digest: "intent-digest".to_string(),
            policy_revision: 11,
            canonical_manifest_digest: "manifest-digest".to_string(),
            selected_logical_action_id: "action-1".to_string(),
            presented_action_digest: "action-digest".to_string(),
            declared_consequence_digest: "consequence-digest".to_string(),
            challenge_digest: "challenge-digest".to_string(),
            decision_result: DecisionResult::ConfirmAndContinue,
            observed_at_ms: 1_800_000_000_000,
            request_correlation_id: "correlation-1".to_string(),
            source_message_id: remote.then(|| "message-1".to_string()),
            callback_fresh: true,
        }
    }

    fn signed_local() -> LocalNativeOwnerProof {
        let mut proof = LocalNativeOwnerProof {
            authenticated_client_id: "mission-control-window-1".to_string(),
            request_correlation_id: "correlation-1".to_string(),
            proof_hex: String::new(),
        };
        let bytes = local_decision_proof_bytes(
            &proof,
            &local_decision_binding(&current(), &response(false)),
        );
        let mut mac = HmacSha256::new_from_slice(LOCAL_SECRET).unwrap();
        mac.update(&bytes);
        proof.proof_hex = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        proof
    }

    fn signed_local_intent() -> LocalNativeOwnerIntentProof {
        signed_local_intent_for("normalized owner intent")
    }

    fn signed_local_intent_for(text: &str) -> LocalNativeOwnerIntentProof {
        let safe = SafeText::new(text.trim(), &[]).unwrap();
        let mut proof = LocalNativeOwnerIntentProof {
            authenticated_client_id: "mission-control-window-1".to_string(),
            request_correlation_id: "intent-correlation-1".to_string(),
            request_id: "native-request-1".to_string(),
            idempotency_key: "native-idem-1".to_string(),
            attach_to_delegation_id: None,
            normalized_intent_digest: owner_normalized_intent_digest(safe.as_str()).unwrap(),
            instruction_digest: owner_instruction_digest(text.as_bytes()).unwrap(),
            proof_hex: String::new(),
        };
        let bytes = local_owner_intake_proof_bytes(&proof).unwrap();
        let mut mac = HmacSha256::new_from_slice(LOCAL_SECRET).unwrap();
        mac.update(&bytes);
        proof.proof_hex = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        proof
    }

    fn global_resume_binding() -> RunControlBinding {
        RunControlBinding::global_resume(
            "global-resume-idempotency-1".to_string(),
            "global-resume-correlation-1".to_string(),
            Utc::now().timestamp_millis(),
            RunControlResumeBinding::new(
                9,
                17,
                prefixed_digest(b"unresolved-effect-set-v1"),
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn delegation_resume_binding() -> RunControlBinding {
        RunControlBinding::delegation_resume(
            "delegation-1".to_string(),
            "delegation-resume-idempotency-1".to_string(),
            "delegation-resume-correlation-1".to_string(),
            Utc::now().timestamp_millis(),
            RunControlResumeBinding::new(
                9,
                17,
                prefixed_digest(b"unresolved-effect-set-v1"),
                Some(31),
                Some(42),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn signed_local_run_control(binding: &RunControlBinding) -> LocalNativeRunControlProof {
        let mut proof = LocalNativeRunControlProof::from_authenticated_native_request(
            "mission-control-window-1".to_string(),
            binding.request_correlation_id.clone(),
            "00".repeat(32),
        )
        .unwrap();
        let mut mac = HmacSha256::new_from_slice(LOCAL_SECRET).unwrap();
        mac.update(
            &local_run_control_request_proof_bytes(&proof.authenticated_client_id, binding)
                .unwrap(),
        );
        proof.proof_hex = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        proof
    }

    fn prefixed_digest(bytes: &[u8]) -> String {
        format!("sha256:{}", digest_hex(bytes))
    }

    fn remote_run_control_event(
        provider: &str,
        owner: &str,
        binding: &RunControlBinding,
    ) -> RemoteProviderRunControlEvent {
        let digest = binding.request_binding_digest();
        match provider {
            "telegram" => RemoteProviderRunControlEvent::from_telegram_long_poll(
                "telegram-listener-1".to_string(),
                owner.to_string(),
                "chat-1".to_string(),
                "message-1".to_string(),
                "update-1".to_string(),
                binding.request_correlation_id.clone(),
                digest,
            )
            .unwrap(),
            "discord" => RemoteProviderRunControlEvent::from_discord_gateway(
                "discord-listener-1".to_string(),
                owner.to_string(),
                "channel-1".to_string(),
                "message-1".to_string(),
                "event-1".to_string(),
                binding.request_correlation_id.clone(),
                digest,
            )
            .unwrap(),
            _ => unreachable!("test helper only supports authenticated providers"),
        }
    }

    fn telegram(_owner: &str, observed: &str) -> RemoteProviderOwnerEvent {
        RemoteProviderOwnerEvent::from_telegram_long_poll(
            "telegram-listener-1".to_string(),
            observed.to_string(),
            "chat-1".to_string(),
            "message-1".to_string(),
            "update-1".to_string(),
            "correlation-1".to_string(),
        )
    }

    fn apply_only_if_verified(
        result: Result<VerifiedConfirmationEvent, ActorGateFailure>,
        transitions: &AtomicUsize,
        effects: &AtomicUsize,
    ) {
        if result.is_ok() {
            transitions.fetch_add(1, Ordering::SeqCst);
            effects.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn bearer_headers_peer_claims_and_confirmation_text_are_never_human() {
        for auth_method in ["static_bearer", "jwt", "service_bearer", "internal_ingest"] {
            let actor = gate().classify_untrusted_transport(&UntrustedTransportAuthentication {
                auth_method: auth_method.to_string(),
                principal_id: "service-principal".to_string(),
                claimed_operator_id: Some("owner".to_string()),
                claimed_peer_id: Some("owner".to_string()),
                claimed_actor_type: Some("human_local".to_string()),
                confirmation_text: Some("the owner confirmed".to_string()),
                request_correlation_id: "correlation-1".to_string(),
            });
            assert!(matches!(
                actor.actor_type(),
                ActorType::Runtime | ActorType::Connector
            ));
            assert!(!actor.may_submit_or_amend_owner_intent());
        }
    }

    #[test]
    fn exact_local_run_control_evidence_is_opaque_replayable_and_binds_resume_snapshot() {
        let gate = gate();
        let binding = delegation_resume_binding();
        let proof = signed_local_run_control(&binding);
        let first = gate.verify_local_run_control(&proof, &binding).unwrap();
        let second = gate.verify_local_run_control(&proof, &binding).unwrap();
        assert_eq!(
            first, second,
            "verification must not consume storage replay"
        );
        assert_eq!(first.actor_type(), ActorType::HumanLocal);
        assert_eq!(first.credential_identity(), "mission-control-window-1");
        assert_eq!(first.authenticated_ingress(), "native-control");
        assert_eq!(first.channel_assurance(), "interactive-local");
        assert_eq!(first.operation(), RunControlOperation::DelegationResume);
        assert_eq!(binding.operation(), RunControlOperation::DelegationResume);
        assert_eq!(first.target(), binding.target());
        assert_eq!(first.idempotency_key(), binding.idempotency_key());
        assert_eq!(
            first.request_correlation_id(),
            binding.request_correlation_id()
        );
        assert_eq!(first.observed_at_ms(), binding.observed_at_ms());
        assert_eq!(
            first.request_binding_digest(),
            binding.request_binding_digest()
        );
        assert_eq!(first.replay_identity().len(), 64);
        assert_eq!(first.resume(), binding.resume());
        let resume = first.resume().unwrap();
        assert_eq!(resume.stopped_epoch(), 9);
        assert_eq!(resume.current_policy_revision(), 17);
        assert_eq!(
            resume.unresolved_effect_disclosure_digest(),
            prefixed_digest(b"unresolved-effect-set-v1")
        );
        assert_eq!(resume.delegation_state_revision(), Some(31));
        assert_eq!(resume.current_plan_revision(), Some(42));
        assert_eq!(first.source_message_id(), None);
        assert_eq!(first.provider_event_id(), None);
    }

    #[test]
    fn run_control_input_constructors_validate_shape_without_exposing_capability_construction() {
        assert!(
            RunControlResumeBinding::new(-1, 17, prefixed_digest(b"effects"), None, None).is_err()
        );
        assert!(
            RunControlResumeBinding::new(1, 17, "not-a-digest".to_string(), None, None).is_err()
        );
        assert!(
            RunControlResumeBinding::new(1, 17, digest_hex(b"unprefixed"), None, None).is_err()
        );
        assert!(RunControlBinding::delegation_stop(
            "".to_string(),
            "idempotency".to_string(),
            "correlation".to_string(),
            Utc::now().timestamp_millis(),
        )
        .is_err());
        assert!(RunControlBinding::global_resume(
            "idempotency".to_string(),
            "correlation".to_string(),
            Utc::now().timestamp_millis(),
            RunControlResumeBinding::new(1, 1, prefixed_digest(b"effects"), Some(1), None).unwrap(),
        )
        .is_err());
        assert!(RunControlBinding::global_stop(
            "stop-idempotency".to_string(),
            "stop-correlation".to_string(),
            Utc::now().timestamp_millis(),
        )
        .is_ok());
        assert!(RunControlBinding::delegation_stop(
            "delegation-1".to_string(),
            "delegation-stop-idempotency".to_string(),
            "delegation-stop-correlation".to_string(),
            Utc::now().timestamp_millis(),
        )
        .is_ok());
        assert!(
            LocalNativeRunControlProof::from_authenticated_native_request(
                "client".to_string(),
                "correlation".to_string(),
                "not-a-mac".to_string(),
            )
            .is_err()
        );
        assert_eq!(
            RemoteProviderRunControlEvent::from_telegram_long_poll(
                "listener".to_string(),
                "owner-1".to_string(),
                "chat".to_string(),
                "message".to_string(),
                "update".to_string(),
                "correlation".to_string(),
                "not-a-digest".to_string(),
            ),
            Err(ActorGateFailure::InvalidEvidence)
        );
    }

    #[test]
    fn local_run_control_rejects_wrong_secret_missing_secret_and_every_bound_field_tamper() {
        let original = delegation_resume_binding();
        let proof = signed_local_run_control(&original);
        let wrong_secret_gate = ExecAssActorGate::new(
            Some(b"different-native-owner-secret-with-32".to_vec()),
            std::iter::empty(),
            gate().replay_directory.as_ref().clone(),
        );
        assert_eq!(
            wrong_secret_gate.verify_local_run_control(&proof, &original),
            Err(ActorGateFailure::LocalProofInvalid)
        );
        let missing_secret_gate = ExecAssActorGate::new(
            None,
            std::iter::empty(),
            gate().replay_directory.as_ref().clone(),
        );
        assert_eq!(
            missing_secret_gate.verify_local_run_control(&proof, &original),
            Err(ActorGateFailure::LocalProofUnavailable)
        );

        for mutation in 0..11 {
            let gate = gate();
            let mut binding = original.clone();
            match mutation {
                0 => binding.operation = RunControlOperation::DelegationStop,
                1 => {
                    binding.target = RunControlTarget::Delegation {
                        delegation_id: "other".to_string(),
                    }
                }
                2 => binding.idempotency_key = "other-idempotency".to_string(),
                3 => binding.request_correlation_id = "other-correlation".to_string(),
                4 => {
                    binding.observed_at_ms =
                        Utc::now().timestamp_millis() - RUN_CONTROL_EVIDENCE_MAX_AGE_MS - 1
                }
                5 => binding.resume.as_mut().unwrap().stopped_epoch += 1,
                6 => binding.resume.as_mut().unwrap().current_policy_revision += 1,
                7 => {
                    binding
                        .resume
                        .as_mut()
                        .unwrap()
                        .unresolved_effect_disclosure_digest = prefixed_digest(b"different-effects")
                }
                8 => binding.resume.as_mut().unwrap().delegation_state_revision = Some(32),
                9 => binding.resume.as_mut().unwrap().current_plan_revision = Some(43),
                _ => binding.resume = None,
            }
            assert!(
                gate.verify_local_run_control(&proof, &binding).is_err(),
                "bound field mutation {mutation} accepted"
            );
        }
    }

    #[test]
    fn authenticated_telegram_and_discord_can_derive_exact_same_human_run_control_capability() {
        for (provider, owner) in [("telegram", "owner-1"), ("discord", "discord-owner")] {
            let gate = gate();
            let binding = global_resume_binding();
            let event = remote_run_control_event(provider, owner, &binding);
            let verified = gate.verify_remote_run_control(&event, &binding).unwrap();
            assert_eq!(verified.actor_type(), ActorType::HumanRemote);
            assert_eq!(
                verified.credential_identity(),
                format!("{provider}:{owner}")
            );
            assert_eq!(verified.operation(), RunControlOperation::GlobalResume);
            assert_eq!(
                verified.request_binding_digest(),
                binding.request_binding_digest()
            );
            assert_eq!(verified.replay_identity().len(), 64);
            assert_eq!(verified.resume(), binding.resume());
        }
    }

    #[test]
    fn remote_run_control_rejects_wrong_provider_account_correlation_and_binding_drift() {
        let binding = global_resume_binding();
        let gate = gate();
        assert_eq!(
            gate.verify_remote_run_control(
                &remote_run_control_event("telegram", "attacker", &binding),
                &binding,
            ),
            Err(ActorGateFailure::OwnerIdentityMismatch)
        );
        let mut wrong_correlation = remote_run_control_event("telegram", "owner-1", &binding);
        wrong_correlation.request_correlation_id = "attacker-correlation".to_string();
        assert_eq!(
            gate.verify_remote_run_control(&wrong_correlation, &binding),
            Err(ActorGateFailure::CorrelationMismatch)
        );
        let mut changed_binding = binding.clone();
        changed_binding.resume.as_mut().unwrap().stopped_epoch += 1;
        assert_eq!(
            gate.verify_remote_run_control(
                &remote_run_control_event("telegram", "owner-1", &binding),
                &changed_binding,
            ),
            Err(ActorGateFailure::RunControlBindingMismatch)
        );
    }

    #[test]
    fn non_human_transport_claims_have_no_run_control_resume_path() {
        for actor_type in [ActorType::Runtime, ActorType::Connector] {
            assert_eq!(
                ensure_human_run_control(actor_type),
                Err(ActorGateFailure::NonHumanRunControl)
            );
        }
        let transport = gate().classify_untrusted_transport(&UntrustedTransportAuthentication {
            auth_method: "service_bearer".to_string(),
            principal_id: "model-worker-claims-owner".to_string(),
            claimed_operator_id: Some("owner".to_string()),
            claimed_peer_id: Some("owner-1".to_string()),
            claimed_actor_type: Some("human_remote".to_string()),
            confirmation_text: Some("resume every stopped run".to_string()),
            request_correlation_id: "global-resume-correlation-1".to_string(),
        });
        assert_eq!(
            ensure_human_run_control(transport.actor_type()),
            Err(ActorGateFailure::NonHumanRunControl)
        );
    }

    #[test]
    fn exact_local_native_proof_can_retry_until_atomic_resolution() {
        let gate = gate();
        let actor = gate
            .verify_local_decision(&signed_local(), &current(), &response(false))
            .unwrap();
        assert_eq!(actor.actor_type(), ActorType::HumanLocal);
        assert_eq!(actor.credential_identity(), "mission-control-window-1");
        assert_eq!(actor.authenticated_ingress(), "native-control");
        assert_eq!(actor.channel_assurance(), "interactive-local");
        assert_eq!(actor.request_correlation_id(), "correlation-1");
        assert_eq!(actor.source_message_id(), None);
        assert_eq!(actor.provider_event_id(), None);
        assert_eq!(actor.verified_decision_binding(), &current());
        assert_eq!(
            actor.verified_decision_result(),
            DecisionResult::ConfirmAndContinue
        );
        assert_eq!(actor.evidence_digest().len(), 64);
        assert!(gate
            .verify_local_decision(&signed_local(), &current(), &response(false))
            .is_ok());
    }

    #[test]
    fn local_decision_mac_binds_every_current_result_and_correlation_field() {
        for mutation in 0..14 {
            let gate = gate();
            let proof = signed_local();
            let mut changed_proof = proof.clone();
            let mut binding = current();
            let mut candidate = response(false);
            match mutation {
                0 => changed_proof.authenticated_client_id = "other-client".into(),
                1 => {
                    changed_proof.request_correlation_id = "other-correlation".into();
                    candidate.request_correlation_id = "other-correlation".into();
                }
                2 => {
                    binding.decision_id = "other-decision".into();
                    candidate.decision_id = "other-decision".into();
                }
                3 => {
                    binding.decision_revision += 1;
                    candidate.decision_revision += 1;
                }
                4 => {
                    binding.normalized_intent_digest = "other-intent".into();
                    candidate.normalized_intent_digest = "other-intent".into();
                }
                5 => {
                    binding.policy_revision += 1;
                    candidate.policy_revision += 1;
                }
                6 => {
                    binding.canonical_manifest_digest = "other-manifest".into();
                    candidate.canonical_manifest_digest = "other-manifest".into();
                }
                7 => {
                    binding.selected_logical_action_id = "other-action".into();
                    candidate.selected_logical_action_id = "other-action".into();
                }
                8 => {
                    binding.presented_action_digest = "other-presented-action".into();
                    candidate.presented_action_digest = "other-presented-action".into();
                }
                9 => {
                    binding.declared_consequence_digest = "other-consequence".into();
                    candidate.declared_consequence_digest = "other-consequence".into();
                }
                10 => {
                    binding.challenge_digest = "other-challenge".into();
                    candidate.challenge_digest = "other-challenge".into();
                }
                11 => binding.expires_at_ms += 1,
                12 => candidate.decision_result = DecisionResult::Decline,
                _ => candidate.observed_at_ms += 1,
            }
            assert_eq!(
                gate.verify_local_decision(&changed_proof, &binding, &candidate),
                Err(ActorGateFailure::LocalProofInvalid),
                "MAC accepted bound mutation {mutation}"
            );
        }
    }

    #[test]
    fn exact_local_owner_intake_proof_retries_and_binds_request_idempotency_and_intent() {
        let first_gate = gate();
        let actor = first_gate
            .verify_local_owner_intake(&signed_local_intent(), "normalized owner intent")
            .expect("exact local owner intake");
        assert_eq!(actor.actor_type(), ActorType::HumanLocal);
        assert_eq!(LOCAL_OWNER_INTAKE_INGRESS, "native-owner-intake");
        assert!(actor.may_submit_or_amend_owner_intent());
        assert_eq!(
            actor
                .owner_actor_assurance()
                .map(ActorAssurance::actor_type),
            Some(ActorType::HumanLocal)
        );
        assert!(first_gate
            .verify_local_owner_intake(&signed_local_intent(), "normalized owner intent")
            .is_ok());

        for mutation in 0..7 {
            let other_gate = gate();
            let mut changed = signed_local_intent();
            match mutation {
                0 => changed.authenticated_client_id = "other-client".into(),
                1 => changed.request_correlation_id = "other-correlation".into(),
                2 => changed.request_id = "other-request".into(),
                3 => changed.idempotency_key = "other-idempotency".into(),
                4 => changed.attach_to_delegation_id = Some("delegation-2".into()),
                5 => changed.normalized_intent_digest = digest_hex(b"different intent"),
                _ => changed.instruction_digest = digest_hex(b"different instruction"),
            }
            assert_eq!(
                other_gate.verify_local_owner_intake(&changed, "normalized owner intent"),
                Err(ActorGateFailure::LocalProofInvalid),
                "mutation {mutation}"
            );
        }
    }

    #[test]
    fn local_owner_intake_rejects_same_redaction_different_raw_secret_and_allows_exact_retry() {
        let first_text = format!("deliver token sk-proj-{}", "a".repeat(24));
        let second_text = format!("deliver token sk-proj-{}", "b".repeat(24));
        let proof = signed_local_intent_for(&first_text);
        assert_eq!(
            signed_local_intent_for(&first_text).normalized_intent_digest,
            signed_local_intent_for(&second_text).normalized_intent_digest
        );
        let gate = gate();
        let first = gate
            .verify_local_owner_intake(&proof, &first_text)
            .expect("exact raw instruction proof");
        let retry = gate
            .verify_local_owner_intake(&proof, &first_text)
            .expect("exact raw instruction retry");
        assert_eq!(first, retry);
        assert_eq!(
            first.verified_instruction_digest(),
            owner_instruction_digest(first_text.as_bytes()).as_deref()
        );
        assert_eq!(
            gate.verify_local_owner_intake(&proof, &second_text),
            Err(ActorGateFailure::LocalProofInvalid)
        );
    }

    #[test]
    fn forged_local_proof_and_every_binding_mutation_have_zero_transition_or_effect() {
        for mutation in 0..16 {
            let gate = gate();
            let mut proof = signed_local();
            let mut binding = current();
            let mut candidate = response(false);
            match mutation {
                0 => proof.proof_hex.replace_range(0..2, "00"),
                1 => proof.authenticated_client_id = "other-client".to_string(),
                2 => candidate.decision_id = "other".to_string(),
                3 => candidate.decision_revision += 1,
                4 => candidate.normalized_intent_digest = "other".to_string(),
                5 => candidate.policy_revision += 1,
                6 => candidate.canonical_manifest_digest = "other".to_string(),
                7 => candidate.selected_logical_action_id = "other".to_string(),
                8 => candidate.presented_action_digest = "other".to_string(),
                9 => candidate.declared_consequence_digest = "other".to_string(),
                10 => candidate.challenge_digest = "other".to_string(),
                11 => candidate.decision_result = DecisionResult::Decline,
                12 => candidate.observed_at_ms = binding.expires_at_ms,
                13 => candidate.request_correlation_id = "other".to_string(),
                14 => candidate.callback_fresh = false,
                _ => binding.challenge_digest = "current-changed".to_string(),
            }
            let transitions = AtomicUsize::new(0);
            let effects = AtomicUsize::new(0);
            apply_only_if_verified(
                gate.verify_local_decision(&proof, &binding, &candidate),
                &transitions,
                &effects,
            );
            assert_eq!(transitions.load(Ordering::SeqCst), 0, "mutation {mutation}");
            assert_eq!(effects.load(Ordering::SeqCst), 0, "mutation {mutation}");
        }
    }

    #[test]
    fn exact_remote_provider_owner_event_can_retry_until_atomic_resolution() {
        let gate = gate();
        let actor = gate
            .verify_remote_decision(&telegram("owner-1", "owner-1"), &current(), &response(true))
            .unwrap();
        assert_eq!(actor.actor_type(), ActorType::HumanRemote);
        assert_eq!(actor.credential_identity(), "telegram:owner-1");
        assert_eq!(actor.authenticated_ingress(), "telegram-listener-1");
        assert_eq!(
            actor.channel_assurance(),
            "authenticated-telegram-provider-event"
        );
        assert_eq!(actor.request_correlation_id(), "correlation-1");
        assert_eq!(actor.source_message_id(), Some("message-1"));
        assert_eq!(actor.provider_event_id(), Some("update-1"));
        assert_eq!(actor.verified_decision_binding(), &current());
        assert_eq!(actor.evidence_digest().len(), 64);
        assert!(gate
            .verify_remote_decision(&telegram("owner-1", "owner-1"), &current(), &response(true))
            .is_ok());
    }

    #[test]
    fn exact_remote_provider_owner_event_derives_base_owner_intake() {
        let actor = gate()
            .classify_remote_owner_intake(
                &telegram("owner-1", "owner-1"),
                "perform the exact owner request",
            )
            .unwrap();
        assert_eq!(actor.actor_type(), ActorType::HumanRemote);
        assert!(actor.may_submit_or_amend_owner_intent());
        assert_eq!(
            actor
                .owner_actor_assurance()
                .map(ActorAssurance::actor_type),
            Some(ActorType::HumanRemote)
        );
        assert_eq!(
            actor.verified_normalized_intent_digest(),
            owner_normalized_intent_digest("perform the exact owner request").as_deref()
        );
        assert_eq!(
            actor.verified_instruction_digest(),
            owner_instruction_digest(b"perform the exact owner request").as_deref()
        );
    }

    #[test]
    fn remote_owner_capability_preserves_only_verified_reply_correlation() {
        let event = telegram("owner-1", "owner-1")
            .with_reply_to_message_id(Some("bot-message-7".to_string()));
        let actor = gate()
            .classify_remote_owner_intake(&event, "clarify the existing delegation")
            .unwrap();
        assert_eq!(actor.verified_provider(), Some("telegram"));
        assert_eq!(actor.verified_conversation_id(), Some("chat-1"));
        assert_eq!(actor.verified_reply_to_message_id(), Some("bot-message-7"));

        let invalid = telegram("owner-1", "owner-1")
            .with_reply_to_message_id(Some("bad\nmessage".to_string()));
        assert_eq!(
            gate().classify_remote_owner_intake(&invalid, "clarify"),
            Err(ActorGateFailure::InvalidEvidence)
        );
    }

    #[test]
    fn shared_intake_authority_requires_exact_verified_intent_correlation_and_message() {
        let text = "perform the exact owner request";
        let local_gate = gate();
        let local = local_gate
            .verify_local_owner_intake(&signed_local_intent_for(text), text)
            .unwrap();
        let local_request = IntakeRequest {
            request_id: "native-request-1".into(),
            idempotency_key: "native-idem-1".into(),
            text: text.into(),
            source_correlation_id: "intent-correlation-1".into(),
            attach_to_delegation_id: None,
        };
        let first_local_authority = ExecAssIntakeService
            .bind_original_request_authority(&local, &local_request, 1, 100)
            .unwrap();
        assert_eq!(
            ExecAssIntakeService
                .bind_original_request_authority(&local, &local_request, 1, 100)
                .unwrap(),
            first_local_authority
        );
        let mut drifted = local_request.clone();
        drifted.text = "perform a different request".into();
        assert_eq!(
            ExecAssIntakeService.bind_original_request_authority(&local, &drifted, 1, 100),
            Err(IntakeAuthorityFailure::IntentBindingMismatch)
        );
        let first_secret_text = format!("deliver token sk-proj-{}", "a".repeat(24));
        let second_secret_text = format!("deliver token sk-proj-{}", "b".repeat(24));
        let secret_actor = local_gate
            .verify_local_owner_intake(
                &signed_local_intent_for(&first_secret_text),
                &first_secret_text,
            )
            .unwrap();
        let mut secret_request = local_request.clone();
        secret_request.text = first_secret_text;
        assert!(ExecAssIntakeService
            .bind_original_request_authority(&secret_actor, &secret_request, 1, 100)
            .is_ok());
        secret_request.text = second_secret_text;
        assert_eq!(
            ExecAssIntakeService.bind_original_request_authority(
                &secret_actor,
                &secret_request,
                1,
                100
            ),
            Err(IntakeAuthorityFailure::InstructionBindingMismatch),
            "different raw secrets that redact identically must not share authority"
        );
        let mut drifted_attachment = local_request.clone();
        drifted_attachment.attach_to_delegation_id = Some("delegation-2".into());
        assert_eq!(
            ExecAssIntakeService.bind_original_request_authority(
                &local,
                &drifted_attachment,
                1,
                100
            ),
            Err(IntakeAuthorityFailure::RequestBindingMismatch)
        );

        let remote = gate()
            .classify_remote_owner_intake(&telegram("owner-1", "owner-1"), text)
            .unwrap();
        let remote_request = IntakeRequest {
            request_id: "message-1".into(),
            idempotency_key: "update-1".into(),
            text: text.into(),
            source_correlation_id: "correlation-1".into(),
            attach_to_delegation_id: None,
        };
        assert!(ExecAssIntakeService
            .bind_original_request_authority(&remote, &remote_request, 1, 100)
            .is_ok());
        let mut wrong_message = remote_request.clone();
        wrong_message.request_id = "other-message".into();
        assert_eq!(
            ExecAssIntakeService.bind_original_request_authority(&remote, &wrong_message, 1, 100),
            Err(IntakeAuthorityFailure::SourceMessageMismatch)
        );

        let runtime = gate().classify_untrusted_transport(&UntrustedTransportAuthentication {
            auth_method: "service_bearer".into(),
            principal_id: "runtime-1".into(),
            claimed_operator_id: Some("owner".into()),
            claimed_peer_id: Some("owner-1".into()),
            claimed_actor_type: Some("human_remote".into()),
            confirmation_text: Some(text.into()),
            request_correlation_id: "correlation-1".into(),
        });
        assert_eq!(
            ExecAssIntakeService.bind_original_request_authority(&runtime, &remote_request, 1, 100),
            Err(IntakeAuthorityFailure::NonHumanActor)
        );
    }

    #[test]
    fn remote_provider_peer_substitution_and_binding_mutations_have_zero_transition_or_effect() {
        for mutation in 0..14 {
            let gate = gate();
            let mut event = telegram("owner-1", "owner-1");
            let mut binding = current();
            let mut candidate = response(true);
            match mutation {
                0 => event.observed_provider_account_id = "attacker".to_string(),
                1 => event.provider = "slack".to_string(),
                2 => candidate.decision_id = "other".to_string(),
                3 => candidate.decision_revision += 1,
                4 => candidate.normalized_intent_digest = "other".to_string(),
                5 => candidate.policy_revision += 1,
                6 => candidate.canonical_manifest_digest = "other".to_string(),
                7 => candidate.presented_action_digest = "other".to_string(),
                8 => candidate.declared_consequence_digest = "other".to_string(),
                9 => candidate.challenge_digest = "other".to_string(),
                10 => candidate.request_correlation_id = "other".to_string(),
                11 => candidate.source_message_id = Some("other".to_string()),
                12 => candidate.callback_fresh = false,
                _ => binding.challenge_digest = "current-changed".to_string(),
            }
            let transitions = AtomicUsize::new(0);
            let effects = AtomicUsize::new(0);
            apply_only_if_verified(
                gate.verify_remote_decision(&event, &binding, &candidate),
                &transitions,
                &effects,
            );
            assert_eq!(transitions.load(Ordering::SeqCst), 0, "mutation {mutation}");
            assert_eq!(effects.load(Ordering::SeqCst), 0, "mutation {mutation}");
        }
    }

    #[test]
    fn model_worker_connector_text_has_no_path_to_verified_decision_actor() {
        let transitions = AtomicUsize::new(0);
        let effects = AtomicUsize::new(0);
        for principal in ["model", "worker", "connector", "tool", "child-agent"] {
            let actor = gate().classify_untrusted_transport(&UntrustedTransportAuthentication {
                auth_method: "service_bearer".to_string(),
                principal_id: principal.to_string(),
                claimed_operator_id: Some("owner".to_string()),
                claimed_peer_id: Some("owner-1".to_string()),
                claimed_actor_type: Some("human_remote".to_string()),
                confirmation_text: Some("confirmed by the owner".to_string()),
                request_correlation_id: "correlation-1".to_string(),
            });
            assert!(!actor.may_submit_or_amend_owner_intent());
            assert!(actor.owner_actor_assurance().is_none());
        }
        assert_eq!(transitions.load(Ordering::SeqCst), 0);
        assert_eq!(effects.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn local_secret_absence_never_falls_back_to_bearer() {
        let temp_dir =
            TempDir::new_in(env!("CARGO_MANIFEST_DIR")).expect("actor gate temp directory");
        let gate = ExecAssActorGate::new(None, std::iter::empty(), temp_dir.path().join("replay"));
        assert_eq!(
            gate.verify_local_decision(&signed_local(), &current(), &response(false)),
            Err(ActorGateFailure::LocalProofUnavailable)
        );
    }

    #[test]
    fn discord_and_telegram_owner_identities_are_provider_scoped() {
        let gate = gate();
        let discord = RemoteProviderOwnerEvent::from_discord_gateway(
            "discord-listener-1".to_string(),
            "telegram-owner".to_string(),
            "channel-1".to_string(),
            "message-1".to_string(),
            "event-1".to_string(),
            "correlation-1".to_string(),
        );
        assert_eq!(
            gate.verify_remote_decision(&discord, &current(), &response(true)),
            Err(ActorGateFailure::OwnerIdentityMismatch)
        );
    }

    #[test]
    fn trusted_server_clock_rejects_stale_evidence_even_with_pre_expiry_observed_time() {
        let binding = current();
        assert_eq!(
            verify_common_binding(&binding, &response(false), binding.expires_at_ms),
            Err(ActorGateFailure::ExpiredChallenge)
        );
    }

    #[test]
    fn verified_decision_evidence_survives_gate_recreation_until_storage_consumes_it() {
        let temp_dir =
            TempDir::new_in(env!("CARGO_MANIFEST_DIR")).expect("actor gate temp directory");
        let replay_directory = temp_dir.path().join("replay");
        let first_process = gate_at(replay_directory.clone());
        first_process
            .verify_local_decision(&signed_local(), &current(), &response(false))
            .expect("first process accepts exact local proof");
        first_process
            .verify_remote_decision(&telegram("owner-1", "owner-1"), &current(), &response(true))
            .expect("first process accepts exact remote event");
        let restarted_process = gate_at(replay_directory);
        assert!(restarted_process
            .verify_local_decision(&signed_local(), &current(), &response(false))
            .is_ok());
        assert!(restarted_process
            .verify_remote_decision(&telegram("owner-1", "owner-1"), &current(), &response(true))
            .is_ok());
    }

    #[test]
    fn concurrent_remote_verification_defers_single_winner_to_storage_transaction() {
        let temp_dir =
            TempDir::new_in(env!("CARGO_MANIFEST_DIR")).expect("actor gate temp directory");
        let replay_directory = temp_dir.path().join("replay");
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let gate = gate_at(replay_directory.clone());
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                gate.verify_remote_decision(
                    &telegram("owner-1", "owner-1"),
                    &current(),
                    &response(true),
                )
            }));
        }
        let results = workers
            .into_iter()
            .map(|worker| worker.join().expect("replay worker"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 8);
    }

    fn signed_owner_mutation(
        binding: &LocalOwnerMutationBinding,
        client: &str,
    ) -> LocalOwnerMutationProof {
        let mut proof = LocalOwnerMutationProof {
            authenticated_client_id: client.to_string(),
            request_correlation_id: binding.request_correlation_id.clone(),
            proof_hex: "00".repeat(32),
        };
        let bytes = local_owner_mutation_proof_bytes(&proof, binding).unwrap();
        let mut mac = HmacSha256::new_from_slice(LOCAL_SECRET).unwrap();
        mac.update(&bytes);
        proof.proof_hex = format!("{:x}", mac.finalize().into_bytes());
        proof
    }

    #[test]
    fn local_owner_mutation_proof_binds_real_client_and_rejects_future_or_changed_material() {
        let binding = LocalOwnerMutationBinding {
            operation: carsinos_protocol::execass::OwnerMutationOperation::PolicyUpdate,
            method: "PUT".into(),
            path: "/api/v1/execass/policy".into(),
            request_correlation_id: "mutation-correlation".into(),
            idempotency_key: "mutation-idempotency".into(),
            expected_revision: 1,
            canonical_body_digest: "11".repeat(32),
            safe_snapshot_digest: "22".repeat(32),
            created_at_ms: Utc::now().timestamp_millis(),
        };
        let proof = signed_owner_mutation(&binding, "native-owner-one");
        let verified = gate()
            .verify_local_owner_mutation(&proof, &binding)
            .expect("exact owner mutation proof");
        assert_eq!(verified.credential_identity, "native-owner-one");
        let second_proof = signed_owner_mutation(&binding, "native-owner-two");
        let second_verified = gate()
            .verify_local_owner_mutation(&second_proof, &binding)
            .expect("second exact owner mutation proof");
        let source = carsinos_core::execass_actor::PolicySnapshotAuthoritySource {
            canonical_mutation_bytes:
                carsinos_protocol::execass::local_owner_mutation_binding_bytes(&binding).unwrap(),
            canonical_safe_snapshot_json: "{}".into(),
            policy_revision: 2,
            policy_snapshot_digest: binding.safe_snapshot_digest.clone(),
            created_at_ms: binding.created_at_ms,
        };
        let first_authority = carsinos_core::execass_actor::bind_policy_snapshot_owner_authority(
            verified.owner_actor_assurance().unwrap(),
            source.clone(),
        )
        .unwrap();
        let second_authority = carsinos_core::execass_actor::bind_policy_snapshot_owner_authority(
            second_verified.owner_actor_assurance().unwrap(),
            source,
        )
        .unwrap();
        assert_ne!(
            first_authority.authority_provenance_id(),
            second_authority.authority_provenance_id()
        );

        let mut changed = binding.clone();
        changed.safe_snapshot_digest = "33".repeat(32);
        assert!(gate()
            .verify_local_owner_mutation(&proof, &changed)
            .is_err());

        let other_client = LocalOwnerMutationProof {
            authenticated_client_id: "native-owner-two".into(),
            ..proof.clone()
        };
        assert!(gate()
            .verify_local_owner_mutation(&other_client, &binding)
            .is_err());

        let mut future = binding.clone();
        future.created_at_ms = Utc::now()
            .timestamp_millis()
            .saturating_add(RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS + 1);
        let future_proof = signed_owner_mutation(&future, "native-owner-one");
        assert!(gate()
            .verify_local_owner_mutation(&future_proof, &future)
            .is_err());
    }
}
