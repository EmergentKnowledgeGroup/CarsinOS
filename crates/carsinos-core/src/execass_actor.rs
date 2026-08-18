//! Server-derived actor assurance for ExecAss owner intake and human decisions.
//!
//! Wire payloads never construct these facts. Gateway and adapter authentication code
//! supplies observed ingress evidence; caller assertions are accepted only so tests can
//! prove that they have no influence on the derived actor or its authority.

#![cfg_attr(
    not(any(test, feature = "execass-test-authority")),
    allow(dead_code, unused_imports)
)]

use sha2::{Digest, Sha256};

const REMOTE_OWNER_EVIDENCE_MAX_AGE_MS: i64 = 60_000;
const REMOTE_OWNER_EVIDENCE_MAX_FUTURE_SKEW_MS: i64 = 5_000;

pub type DerivedActorType = carsinos_protocol::execass::ActorType;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CallerActorClaims {
    pub claimed_actor_type: Option<String>,
    pub claimed_actor_id: Option<String>,
    pub operator_header: Option<String>,
    pub peer_id: Option<String>,
    pub confirmation_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalInteractiveEvidence {
    pub authenticated_client_id: String,
    pub authenticated_ingress: String,
    pub channel_assurance: String,
    pub request_correlation_id: String,
    pub source_message_id: Option<String>,
    pub interactive_owner_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteOwnerEvidence {
    pub adapter_id: String,
    pub adapter_authenticated: bool,
    pub allowlisted_provider_account_id: String,
    pub observed_provider_account_id: String,
    pub authenticated_ingress: String,
    pub channel_assurance: String,
    pub source_message_id: String,
    pub request_correlation_id: String,
    pub callback_fresh: bool,
}

/// Local-owner facts handed across the production authentication boundary.
///
/// This type does not authenticate a request. Call
/// [`AuthenticatedLocalOwnerEvidence::from_verified_native_hmac`] only after
/// trusted gateway code has verified the native HMAC and consumed its replay
/// identity. It deliberately has private fields and no wire deserialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedLocalOwnerEvidence {
    authenticated_client_id: String,
    authenticated_ingress: String,
    channel_assurance: String,
    request_correlation_id: String,
}

impl AuthenticatedLocalOwnerEvidence {
    /// Record exact server-observed facts after the gateway's native-HMAC gate.
    pub fn from_verified_native_hmac(
        authenticated_client_id: impl Into<String>,
        authenticated_ingress: impl Into<String>,
        channel_assurance: impl Into<String>,
        request_correlation_id: impl Into<String>,
    ) -> Result<Self, ActorEvidenceError> {
        let evidence = Self {
            authenticated_client_id: authenticated_client_id.into(),
            authenticated_ingress: authenticated_ingress.into(),
            channel_assurance: channel_assurance.into(),
            request_correlation_id: request_correlation_id.into(),
        };
        if !all_exact_present([
            evidence.authenticated_client_id.as_str(),
            evidence.authenticated_ingress.as_str(),
            evidence.channel_assurance.as_str(),
            evidence.request_correlation_id.as_str(),
        ]) {
            return Err(ActorEvidenceError::InvalidField);
        }
        Ok(evidence)
    }
}

/// Remote-owner facts handed across the production authentication boundary.
///
/// This type does not authenticate a provider callback. Call
/// [`AuthenticatedRemoteOwnerEvidence::from_authenticated_provider_event`]
/// only from the trusted listener after it obtained the event from the
/// provider and loaded the configured owner allowlist. Core independently
/// checks exact account equality and a fixed freshness window; callers cannot
/// supply a `verified`/`fresh` boolean or select an actor type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedRemoteOwnerEvidence {
    adapter_id: String,
    allowlisted_provider_account_id: String,
    observed_provider_account_id: String,
    authenticated_ingress: String,
    channel_assurance: String,
    source_message_id: String,
    provider_event_id: String,
    request_correlation_id: String,
    provider_event_observed_at_ms: i64,
    server_verified_at_ms: i64,
}

impl AuthenticatedRemoteOwnerEvidence {
    /// Record one authenticated provider event using server-observed time.
    #[allow(clippy::too_many_arguments)]
    pub fn from_authenticated_provider_event(
        adapter_id: impl Into<String>,
        allowlisted_provider_account_id: impl Into<String>,
        observed_provider_account_id: impl Into<String>,
        authenticated_ingress: impl Into<String>,
        channel_assurance: impl Into<String>,
        source_message_id: impl Into<String>,
        provider_event_id: impl Into<String>,
        request_correlation_id: impl Into<String>,
        provider_event_observed_at_ms: i64,
        server_verified_at_ms: i64,
    ) -> Result<Self, ActorEvidenceError> {
        let evidence = Self {
            adapter_id: adapter_id.into(),
            allowlisted_provider_account_id: allowlisted_provider_account_id.into(),
            observed_provider_account_id: observed_provider_account_id.into(),
            authenticated_ingress: authenticated_ingress.into(),
            channel_assurance: channel_assurance.into(),
            source_message_id: source_message_id.into(),
            provider_event_id: provider_event_id.into(),
            request_correlation_id: request_correlation_id.into(),
            provider_event_observed_at_ms,
            server_verified_at_ms,
        };
        if !all_exact_present([
            evidence.adapter_id.as_str(),
            evidence.allowlisted_provider_account_id.as_str(),
            evidence.observed_provider_account_id.as_str(),
            evidence.authenticated_ingress.as_str(),
            evidence.channel_assurance.as_str(),
            evidence.source_message_id.as_str(),
            evidence.provider_event_id.as_str(),
            evidence.request_correlation_id.as_str(),
        ]) || evidence.provider_event_observed_at_ms < 0
            || evidence.server_verified_at_ms < 0
        {
            return Err(ActorEvidenceError::InvalidField);
        }
        if evidence.allowlisted_provider_account_id != evidence.observed_provider_account_id {
            return Err(ActorEvidenceError::RemoteAccountMismatch);
        }
        if evidence.provider_event_observed_at_ms
            > evidence
                .server_verified_at_ms
                .saturating_add(REMOTE_OWNER_EVIDENCE_MAX_FUTURE_SKEW_MS)
            || evidence
                .server_verified_at_ms
                .saturating_sub(evidence.provider_event_observed_at_ms)
                > REMOTE_OWNER_EVIDENCE_MAX_AGE_MS
        {
            return Err(ActorEvidenceError::RemoteEvidenceOutsideFreshnessWindow);
        }
        Ok(evidence)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorEvidenceError {
    InvalidField,
    RemoteAccountMismatch,
    RemoteEvidenceOutsideFreshnessWindow,
}

/// Observations emitted by trusted server authentication code, never by a wire DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ServerIngressObservation {
    LocalInteractive(Box<LocalInteractiveEvidence>),
    RemoteAuthenticated(Box<RemoteOwnerEvidence>),
    ServiceBearer { credential_id: String },
    Runtime { runtime_id: String },
    Connector { connector_id: String },
    Worker { worker_id: String },
    Model { model_id: String },
    RetrievedContent { source_id: String },
    ToolOutput { tool_id: String },
    ChildAgent { child_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorAssurance {
    actor_type: DerivedActorType,
    credential_identity: String,
    verified_evidence: Vec<String>,
    may_submit_or_amend_owner_intent: bool,
    may_resolve_human_decision: bool,
    may_mint_confirmation_grant: bool,
    human_evidence: Option<VerifiedHumanEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VerifiedHumanEvidence {
    Local {
        authenticated_client_id: String,
        authenticated_ingress: String,
        channel_assurance: String,
        request_correlation_id: String,
        source_message_id: Option<String>,
    },
    Remote {
        adapter_id: String,
        provider_account_id: String,
        authenticated_ingress: String,
        channel_assurance: String,
        source_message_id: String,
        request_correlation_id: String,
        provider_event_id: Option<String>,
        provider_event_observed_at_ms: Option<i64>,
        server_verified_at_ms: Option<i64>,
    },
}

impl ActorAssurance {
    pub fn actor_type(&self) -> DerivedActorType {
        self.actor_type
    }

    pub fn credential_identity(&self) -> &str {
        &self.credential_identity
    }

    pub fn verified_evidence(&self) -> &[String] {
        &self.verified_evidence
    }

    pub fn may_submit_or_amend_owner_intent(&self) -> bool {
        self.may_submit_or_amend_owner_intent
    }

    pub fn may_resolve_human_decision(&self) -> bool {
        self.may_resolve_human_decision
    }

    pub fn may_mint_confirmation_grant(&self) -> bool {
        self.may_mint_confirmation_grant
    }

    pub fn may_work_within_existing_owner_authority(&self) -> bool {
        true
    }

    fn with_verified_decision(mut self) -> Self {
        self.may_resolve_human_decision = true;
        self.may_mint_confirmation_grant = true;
        self
    }
}

/// Derive base actor assurance exclusively from server observations.
pub(crate) fn derive_base_actor_assurance(
    observation: &ServerIngressObservation,
    _caller_claims: &CallerActorClaims,
) -> ActorAssurance {
    match observation {
        ServerIngressObservation::LocalInteractive(evidence)
            if evidence.interactive_owner_verified
                && all_present([
                    evidence.authenticated_client_id.as_str(),
                    evidence.authenticated_ingress.as_str(),
                    evidence.channel_assurance.as_str(),
                    evidence.request_correlation_id.as_str(),
                ]) =>
        {
            ActorAssurance {
                actor_type: DerivedActorType::HumanLocal,
                credential_identity: evidence.authenticated_client_id.clone(),
                verified_evidence: vec![
                    evidence.authenticated_client_id.clone(),
                    evidence.request_correlation_id.clone(),
                ],
                may_submit_or_amend_owner_intent: true,
                may_resolve_human_decision: false,
                may_mint_confirmation_grant: false,
                human_evidence: Some(VerifiedHumanEvidence::Local {
                    authenticated_client_id: evidence.authenticated_client_id.clone(),
                    authenticated_ingress: evidence.authenticated_ingress.clone(),
                    channel_assurance: evidence.channel_assurance.clone(),
                    request_correlation_id: evidence.request_correlation_id.clone(),
                    source_message_id: evidence.source_message_id.clone(),
                }),
            }
        }
        ServerIngressObservation::RemoteAuthenticated(evidence)
            if evidence.adapter_authenticated
                && evidence.callback_fresh
                && evidence.allowlisted_provider_account_id
                    == evidence.observed_provider_account_id
                && all_present([
                    evidence.adapter_id.as_str(),
                    evidence.allowlisted_provider_account_id.as_str(),
                    evidence.authenticated_ingress.as_str(),
                    evidence.channel_assurance.as_str(),
                    evidence.source_message_id.as_str(),
                    evidence.request_correlation_id.as_str(),
                ]) =>
        {
            ActorAssurance {
                actor_type: DerivedActorType::HumanRemote,
                credential_identity: format!(
                    "{}:{}",
                    evidence.adapter_id, evidence.observed_provider_account_id
                ),
                verified_evidence: vec![
                    evidence.adapter_id.clone(),
                    evidence.observed_provider_account_id.clone(),
                    evidence.source_message_id.clone(),
                    evidence.request_correlation_id.clone(),
                ],
                may_submit_or_amend_owner_intent: true,
                may_resolve_human_decision: false,
                may_mint_confirmation_grant: false,
                human_evidence: Some(VerifiedHumanEvidence::Remote {
                    adapter_id: evidence.adapter_id.clone(),
                    provider_account_id: evidence.observed_provider_account_id.clone(),
                    authenticated_ingress: evidence.authenticated_ingress.clone(),
                    channel_assurance: evidence.channel_assurance.clone(),
                    source_message_id: evidence.source_message_id.clone(),
                    request_correlation_id: evidence.request_correlation_id.clone(),
                    provider_event_id: None,
                    provider_event_observed_at_ms: None,
                    server_verified_at_ms: None,
                }),
            }
        }
        ServerIngressObservation::LocalInteractive(evidence) => nonhuman(
            DerivedActorType::Runtime,
            &evidence.authenticated_client_id,
            "unverified_local_interaction",
        ),
        ServerIngressObservation::RemoteAuthenticated(evidence) => nonhuman(
            DerivedActorType::Connector,
            &evidence.adapter_id,
            "unverified_remote_callback",
        ),
        ServerIngressObservation::ServiceBearer { credential_id } => {
            nonhuman(DerivedActorType::Runtime, credential_id, "service_bearer")
        }
        ServerIngressObservation::Runtime { runtime_id } => {
            nonhuman(DerivedActorType::Runtime, runtime_id, "runtime")
        }
        ServerIngressObservation::Connector { connector_id } => {
            nonhuman(DerivedActorType::Connector, connector_id, "connector")
        }
        ServerIngressObservation::Worker { worker_id } => {
            nonhuman(DerivedActorType::Worker, worker_id, "worker")
        }
        ServerIngressObservation::Model { model_id } => {
            nonhuman(DerivedActorType::Model, model_id, "model")
        }
        ServerIngressObservation::RetrievedContent { source_id } => {
            nonhuman(DerivedActorType::Model, source_id, "retrieved_content")
        }
        ServerIngressObservation::ToolOutput { tool_id } => {
            nonhuman(DerivedActorType::Connector, tool_id, "tool_output")
        }
        ServerIngressObservation::ChildAgent { child_id } => {
            nonhuman(DerivedActorType::Worker, child_id, "child_agent")
        }
    }
}

/// Derive opaque local-human assurance from the gateway-authenticated evidence
/// capability. No actor type or authority flags are accepted from the caller.
pub fn derive_local_owner_actor_assurance(
    evidence: AuthenticatedLocalOwnerEvidence,
) -> ActorAssurance {
    ActorAssurance {
        actor_type: DerivedActorType::HumanLocal,
        credential_identity: evidence.authenticated_client_id.clone(),
        verified_evidence: vec![
            evidence.authenticated_client_id.clone(),
            evidence.request_correlation_id.clone(),
        ],
        may_submit_or_amend_owner_intent: true,
        may_resolve_human_decision: false,
        may_mint_confirmation_grant: false,
        human_evidence: Some(VerifiedHumanEvidence::Local {
            authenticated_client_id: evidence.authenticated_client_id,
            authenticated_ingress: evidence.authenticated_ingress,
            channel_assurance: evidence.channel_assurance,
            request_correlation_id: evidence.request_correlation_id,
            source_message_id: None,
        }),
    }
}

/// Derive opaque remote-human assurance from exact account-matched, fresh
/// provider evidence. Authentication itself remains the gateway's job.
pub fn derive_remote_owner_actor_assurance(
    evidence: AuthenticatedRemoteOwnerEvidence,
) -> ActorAssurance {
    ActorAssurance {
        actor_type: DerivedActorType::HumanRemote,
        credential_identity: format!(
            "{}:{}",
            evidence.adapter_id, evidence.observed_provider_account_id
        ),
        verified_evidence: vec![
            evidence.adapter_id.clone(),
            evidence.observed_provider_account_id.clone(),
            evidence.source_message_id.clone(),
            evidence.provider_event_id.clone(),
            evidence.request_correlation_id.clone(),
        ],
        may_submit_or_amend_owner_intent: true,
        may_resolve_human_decision: false,
        may_mint_confirmation_grant: false,
        human_evidence: Some(VerifiedHumanEvidence::Remote {
            adapter_id: evidence.adapter_id,
            provider_account_id: evidence.observed_provider_account_id,
            authenticated_ingress: evidence.authenticated_ingress,
            channel_assurance: evidence.channel_assurance,
            source_message_id: evidence.source_message_id,
            request_correlation_id: evidence.request_correlation_id,
            provider_event_id: Some(evidence.provider_event_id),
            provider_event_observed_at_ms: Some(evidence.provider_event_observed_at_ms),
            server_verified_at_ms: Some(evidence.server_verified_at_ms),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentDecisionBinding {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionResponseEvidence {
    pub decision_id: String,
    pub decision_revision: u64,
    pub normalized_intent_digest: String,
    pub policy_revision: i64,
    pub canonical_manifest_digest: String,
    pub selected_logical_action_id: String,
    pub presented_action_digest: String,
    pub declared_consequence_digest: String,
    pub challenge_digest: String,
    pub decision_result: carsinos_protocol::execass::DecisionResult,
    pub observed_at_ms: i64,
    pub request_correlation_id: String,
    pub source_message_id: Option<String>,
    pub callback_fresh: bool,
}

/// Validated, exact source material for one original owner request.
///
/// Fields remain private, `authority_kind` is fixed to `original_request`, and
/// decision/purpose/category/finance knobs do not exist on this API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalRequestAuthoritySource {
    normalized_intent: String,
    instruction_revision: String,
    instruction_bytes: Vec<u8>,
    owner_envelope_revision: String,
    canonical_owner_envelope_json: String,
    canonical_scope_json: String,
    policy_revision: i64,
    created_at_ms: i64,
    expires_at_ms: Option<i64>,
}

impl OriginalRequestAuthoritySource {
    pub fn builder() -> OriginalRequestAuthoritySourceBuilder {
        OriginalRequestAuthoritySourceBuilder::default()
    }
}

/// Builder for the exact source bytes bound into an original-request authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[must_use = "call build() and bind the validated source to authenticated owner evidence"]
pub struct OriginalRequestAuthoritySourceBuilder {
    normalized_intent: Option<String>,
    instruction_revision: Option<String>,
    instruction_bytes: Option<Vec<u8>>,
    owner_envelope_revision: Option<String>,
    canonical_owner_envelope_json: Option<String>,
    canonical_scope_json: Option<String>,
    policy_revision: Option<i64>,
    created_at_ms: Option<i64>,
    expires_at_ms: Option<i64>,
}

impl OriginalRequestAuthoritySourceBuilder {
    pub fn normalized_intent(mut self, value: impl Into<String>) -> Self {
        self.normalized_intent = Some(value.into());
        self
    }

    pub fn owner_instruction(
        mut self,
        revision: impl Into<String>,
        exact_bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.instruction_revision = Some(revision.into());
        self.instruction_bytes = Some(exact_bytes.into());
        self
    }

    pub fn canonical_owner_envelope(
        mut self,
        revision: impl Into<String>,
        canonical_json: impl Into<String>,
    ) -> Self {
        self.owner_envelope_revision = Some(revision.into());
        self.canonical_owner_envelope_json = Some(canonical_json.into());
        self
    }

    pub fn canonical_scope_json(mut self, canonical_json: impl Into<String>) -> Self {
        self.canonical_scope_json = Some(canonical_json.into());
        self
    }

    pub fn policy_revision(mut self, revision: i64) -> Self {
        self.policy_revision = Some(revision);
        self
    }

    pub fn created_at_ms(mut self, created_at_ms: i64) -> Self {
        self.created_at_ms = Some(created_at_ms);
        self
    }

    pub fn expires_at_ms(mut self, expires_at_ms: i64) -> Self {
        self.expires_at_ms = Some(expires_at_ms);
        self
    }

    pub fn build(self) -> Result<OriginalRequestAuthoritySource, OwnerAuthorityBindingError> {
        let source = OriginalRequestAuthoritySource {
            normalized_intent: self
                .normalized_intent
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            instruction_revision: self
                .instruction_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            instruction_bytes: self
                .instruction_bytes
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            owner_envelope_revision: self
                .owner_envelope_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            canonical_owner_envelope_json: self
                .canonical_owner_envelope_json
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            canonical_scope_json: self
                .canonical_scope_json
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            policy_revision: self
                .policy_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            created_at_ms: self
                .created_at_ms
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            expires_at_ms: self.expires_at_ms,
        };
        if !all_exact_present([
            source.normalized_intent.as_str(),
            source.instruction_revision.as_str(),
            source.owner_envelope_revision.as_str(),
        ]) || source.instruction_bytes.is_empty()
            || source.policy_revision < 0
            || source.created_at_ms < 0
            || source
                .expires_at_ms
                .is_some_and(|expires_at| expires_at <= source.created_at_ms)
            || !is_canonical_json_object(&source.canonical_owner_envelope_json)
            || !is_canonical_json_object(&source.canonical_scope_json)
        {
            return Err(OwnerAuthorityBindingError::InvalidField);
        }
        Ok(source)
    }
}

/// Validated source material for one authenticated owner follow-up that
/// amends an existing delegation. The target and optimistic revisions are
/// typed here so a caller cannot hide them in an arbitrary scope document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpAmendmentAuthoritySource {
    target_delegation_id: String,
    expected_delegation_revision: i64,
    expected_plan_revision: i64,
    normalized_intent: String,
    instruction_revision: String,
    instruction_bytes: Vec<u8>,
    owner_envelope_revision: String,
    canonical_owner_envelope_json: String,
    policy_revision: i64,
    created_at_ms: i64,
}

impl FollowUpAmendmentAuthoritySource {
    pub fn builder() -> FollowUpAmendmentAuthoritySourceBuilder {
        FollowUpAmendmentAuthoritySourceBuilder::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[must_use = "call build() and bind the amendment to authenticated owner evidence"]
pub struct FollowUpAmendmentAuthoritySourceBuilder {
    target_delegation_id: Option<String>,
    expected_delegation_revision: Option<i64>,
    expected_plan_revision: Option<i64>,
    normalized_intent: Option<String>,
    instruction_revision: Option<String>,
    instruction_bytes: Option<Vec<u8>>,
    owner_envelope_revision: Option<String>,
    canonical_owner_envelope_json: Option<String>,
    policy_revision: Option<i64>,
    created_at_ms: Option<i64>,
}

impl FollowUpAmendmentAuthoritySourceBuilder {
    pub fn target(
        mut self,
        delegation_id: impl Into<String>,
        expected_delegation_revision: i64,
        expected_plan_revision: i64,
    ) -> Self {
        self.target_delegation_id = Some(delegation_id.into());
        self.expected_delegation_revision = Some(expected_delegation_revision);
        self.expected_plan_revision = Some(expected_plan_revision);
        self
    }

    pub fn normalized_intent(mut self, value: impl Into<String>) -> Self {
        self.normalized_intent = Some(value.into());
        self
    }

    pub fn owner_instruction(
        mut self,
        revision: impl Into<String>,
        exact_bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.instruction_revision = Some(revision.into());
        self.instruction_bytes = Some(exact_bytes.into());
        self
    }

    pub fn canonical_owner_envelope(
        mut self,
        revision: impl Into<String>,
        canonical_json: impl Into<String>,
    ) -> Self {
        self.owner_envelope_revision = Some(revision.into());
        self.canonical_owner_envelope_json = Some(canonical_json.into());
        self
    }

    pub fn policy_revision(mut self, revision: i64) -> Self {
        self.policy_revision = Some(revision);
        self
    }

    pub fn created_at_ms(mut self, created_at_ms: i64) -> Self {
        self.created_at_ms = Some(created_at_ms);
        self
    }

    pub fn build(self) -> Result<FollowUpAmendmentAuthoritySource, OwnerAuthorityBindingError> {
        let source = FollowUpAmendmentAuthoritySource {
            target_delegation_id: self
                .target_delegation_id
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            expected_delegation_revision: self
                .expected_delegation_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            expected_plan_revision: self
                .expected_plan_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            normalized_intent: self
                .normalized_intent
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            instruction_revision: self
                .instruction_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            instruction_bytes: self
                .instruction_bytes
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            owner_envelope_revision: self
                .owner_envelope_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            canonical_owner_envelope_json: self
                .canonical_owner_envelope_json
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            policy_revision: self
                .policy_revision
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
            created_at_ms: self
                .created_at_ms
                .ok_or(OwnerAuthorityBindingError::InvalidField)?,
        };
        if !all_exact_present([
            source.target_delegation_id.as_str(),
            source.normalized_intent.as_str(),
            source.instruction_revision.as_str(),
            source.owner_envelope_revision.as_str(),
        ]) || source.expected_delegation_revision <= 0
            || source.expected_plan_revision <= 0
            || source.instruction_bytes.is_empty()
            || source.policy_revision <= 0
            || source.created_at_ms < 0
            || !is_canonical_json_object(&source.canonical_owner_envelope_json)
        {
            return Err(OwnerAuthorityBindingError::InvalidField);
        }
        Ok(source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerAuthoritySourceInput {
    pub normalized_intent: String,
    pub instruction_revision: String,
    pub instruction_bytes: Vec<u8>,
    pub owner_envelope_revision: String,
    pub owner_envelope_json: String,
    pub authority_kind: String,
    pub normalized_scope_json: String,
    pub policy_revision: i64,
    pub bound_decision_id: Option<String>,
    pub bound_decision_revision: Option<i64>,
    pub bound_manifest_bytes: Option<Vec<u8>>,
    pub challenge_nonce_bytes: Option<Vec<u8>>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerAuthorityBindingError {
    NonHumanActor,
    InvalidField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOwnerAuthority {
    evidence: VerifiedHumanEvidence,
    authority_provenance_id: String,
    normalized_intent_digest: String,
    instruction_revision: String,
    instruction_digest: String,
    owner_envelope_revision: String,
    owner_envelope_digest: String,
    authority_kind: String,
    normalized_scope_json: String,
    policy_revision: i64,
    bound_decision_id: Option<String>,
    bound_decision_revision: Option<i64>,
    bound_manifest_digest: Option<String>,
    bound_challenge_nonce_digest: Option<String>,
    evidence_digest: String,
    created_at: i64,
    expires_at: Option<i64>,
}

/// Server-derived source for one authenticated, exact decision response.
/// The caller must obtain `actor` from a trusted ingress verifier; this source
/// carries no actor, role, tenant, or generic authority selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionResolutionAuthoritySource {
    pub normalized_intent: String,
    pub canonical_manifest_json: String,
    pub canonical_manifest_digest: String,
    pub decision_id: String,
    pub decision_revision: i64,
    pub policy_revision: i64,
    pub selected_logical_action_id: String,
    pub decision_result: carsinos_protocol::execass::DecisionResult,
    pub request_correlation_id: String,
    pub idempotency_key: String,
    pub revision_text_digest: Option<String>,
    pub challenge_response_digest: Option<String>,
    pub challenge_nonce_digest: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

/// Canonical digest used when binding a verified owner resolution to one exact
/// frozen manifest. It is public so trusted challenge presentation and owner-
/// authority issuance can independently derive the same binding without
/// exposing any mint capability.
pub fn owner_resolution_manifest_digest(manifest_bytes: &[u8]) -> Option<String> {
    (!manifest_bytes.is_empty()).then(|| plain_digest_hex(manifest_bytes))
}

/// Canonical digest for the one-time confirmation challenge nonce/token. The
/// raw nonce is never persisted by the canonical ExecAss store.
pub fn owner_resolution_challenge_nonce_digest(nonce_bytes: &[u8]) -> Option<String> {
    (!nonce_bytes.is_empty())
        .then(|| digest_hex(b"carsinos.execass.challenge_nonce.v1", nonce_bytes))
}

pub fn owner_normalized_intent_digest(normalized_intent: &str) -> Option<String> {
    carsinos_protocol::execass::normalized_owner_intent_digest(normalized_intent)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedHumanEvidenceRef<'a> {
    Local {
        authenticated_client_id: &'a str,
        authenticated_ingress: &'a str,
        channel_assurance: &'a str,
        request_correlation_id: &'a str,
        source_message_id: Option<&'a str>,
    },
    Remote {
        adapter_id: &'a str,
        provider_account_id: &'a str,
        authenticated_ingress: &'a str,
        channel_assurance: &'a str,
        source_message_id: &'a str,
        request_correlation_id: &'a str,
    },
}

impl VerifiedOwnerAuthority {
    pub fn evidence(&self) -> VerifiedHumanEvidenceRef<'_> {
        match &self.evidence {
            VerifiedHumanEvidence::Local {
                authenticated_client_id,
                authenticated_ingress,
                channel_assurance,
                request_correlation_id,
                source_message_id,
            } => VerifiedHumanEvidenceRef::Local {
                authenticated_client_id,
                authenticated_ingress,
                channel_assurance,
                request_correlation_id,
                source_message_id: source_message_id.as_deref(),
            },
            VerifiedHumanEvidence::Remote {
                adapter_id,
                provider_account_id,
                authenticated_ingress,
                channel_assurance,
                source_message_id,
                request_correlation_id,
                ..
            } => VerifiedHumanEvidenceRef::Remote {
                adapter_id,
                provider_account_id,
                authenticated_ingress,
                channel_assurance,
                source_message_id,
                request_correlation_id,
            },
        }
    }

    pub fn authority_provenance_id(&self) -> &str {
        &self.authority_provenance_id
    }

    pub fn normalized_intent_digest(&self) -> &str {
        &self.normalized_intent_digest
    }

    pub fn instruction_revision(&self) -> &str {
        &self.instruction_revision
    }

    pub fn instruction_digest(&self) -> &str {
        &self.instruction_digest
    }

    pub fn owner_envelope_revision(&self) -> &str {
        &self.owner_envelope_revision
    }

    pub fn owner_envelope_digest(&self) -> &str {
        &self.owner_envelope_digest
    }

    pub fn authority_kind(&self) -> &str {
        &self.authority_kind
    }
    pub fn normalized_scope_json(&self) -> &str {
        &self.normalized_scope_json
    }
    pub fn policy_revision(&self) -> i64 {
        self.policy_revision
    }
    pub fn bound_decision_id(&self) -> Option<&str> {
        self.bound_decision_id.as_deref()
    }
    pub fn bound_decision_revision(&self) -> Option<i64> {
        self.bound_decision_revision
    }
    pub fn bound_manifest_digest(&self) -> Option<&str> {
        self.bound_manifest_digest.as_deref()
    }
    pub fn bound_challenge_nonce_digest(&self) -> Option<&str> {
        self.bound_challenge_nonce_digest.as_deref()
    }
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn expires_at(&self) -> Option<i64> {
        self.expires_at
    }
}

/// Issue opaque owner provenance only from server-derived human assurance and
/// canonical source material. This mint is crate-private: production callers
/// cannot construct either the verified ingress observation or the authority.
pub(crate) fn bind_verified_owner_authority(
    actor: &ActorAssurance,
    source: OwnerAuthoritySourceInput,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    bind_verified_owner_authority_with_bound_digests(actor, source, None, None)
}

fn bind_verified_owner_authority_with_bound_digests(
    actor: &ActorAssurance,
    source: OwnerAuthoritySourceInput,
    verified_manifest_digest: Option<String>,
    verified_challenge_nonce_digest: Option<String>,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    let evidence = actor
        .human_evidence
        .clone()
        .ok_or(OwnerAuthorityBindingError::NonHumanActor)?;
    if !actor.may_submit_or_amend_owner_intent
        || !all_present([
            source.normalized_intent.as_str(),
            source.instruction_revision.as_str(),
            source.owner_envelope_revision.as_str(),
            source.authority_kind.as_str(),
            source.normalized_scope_json.as_str(),
            source.owner_envelope_json.as_str(),
        ])
        || source.instruction_bytes.is_empty()
        || !matches!(
            source.authority_kind.as_str(),
            "original_request"
                | "decision_resolution"
                | "action_specific_owner_amendment"
                | "policy_snapshot"
                | "runtime_settings_snapshot"
                | "runtime_safety_state"
        )
        || source.policy_revision < 0
        || verified_manifest_digest
            .as_deref()
            .is_some_and(|digest| !is_lower_hex_digest(digest))
        || verified_challenge_nonce_digest
            .as_deref()
            .is_some_and(|digest| !is_lower_hex_digest(digest))
        || (verified_manifest_digest.is_some() && source.bound_manifest_bytes.is_some())
        || (verified_challenge_nonce_digest.is_some() && source.challenge_nonce_bytes.is_some())
        || source
            .expires_at
            .is_some_and(|expires_at| expires_at <= source.created_at)
        || source.bound_decision_id.is_some() != source.bound_decision_revision.is_some()
        || source
            .bound_decision_revision
            .is_some_and(|revision| revision < 0)
        || source
            .bound_manifest_bytes
            .as_ref()
            .is_some_and(Vec::is_empty)
        || source
            .challenge_nonce_bytes
            .as_ref()
            .is_some_and(Vec::is_empty)
        || serde_json::from_str::<serde_json::Value>(&source.normalized_scope_json).is_err()
        || serde_json::from_str::<serde_json::Value>(&source.owner_envelope_json).is_err()
    {
        return Err(OwnerAuthorityBindingError::InvalidField);
    }
    let normalized_scope_json = canonical_json(&source.normalized_scope_json);
    let owner_envelope_json = canonical_json(&source.owner_envelope_json);
    let evidence_digest = digest_hex(
        b"carsinos.execass.owner_evidence.v1",
        &canonical_human_evidence_bytes(&evidence),
    );
    let normalized_intent_digest = owner_normalized_intent_digest(&source.normalized_intent)
        .ok_or(OwnerAuthorityBindingError::InvalidField)?;
    let instruction_digest =
        carsinos_protocol::execass::owner_instruction_digest(&source.instruction_bytes)
            .ok_or(OwnerAuthorityBindingError::InvalidField)?;
    let owner_envelope_digest = digest_hex(
        b"carsinos.execass.owner_envelope.v1",
        owner_envelope_json.as_bytes(),
    );
    let bound_manifest_digest = verified_manifest_digest.or_else(|| {
        source
            .bound_manifest_bytes
            .as_ref()
            .map(|bytes| plain_digest_hex(bytes))
    });
    let bound_challenge_nonce_digest = verified_challenge_nonce_digest.or_else(|| {
        source
            .challenge_nonce_bytes
            .as_ref()
            .map(|bytes| digest_hex(b"carsinos.execass.challenge_nonce.v1", bytes))
    });
    let authority_provenance_id = authority_provenance_id(
        &evidence_digest,
        &normalized_intent_digest,
        &source,
        &instruction_digest,
        &owner_envelope_digest,
        &normalized_scope_json,
        bound_manifest_digest.as_deref(),
        bound_challenge_nonce_digest.as_deref(),
    );
    Ok(VerifiedOwnerAuthority {
        evidence,
        authority_provenance_id,
        normalized_intent_digest,
        instruction_revision: source.instruction_revision,
        instruction_digest,
        owner_envelope_revision: source.owner_envelope_revision,
        owner_envelope_digest,
        authority_kind: source.authority_kind,
        normalized_scope_json,
        policy_revision: source.policy_revision,
        bound_decision_id: source.bound_decision_id,
        bound_decision_revision: source.bound_decision_revision,
        bound_manifest_digest,
        bound_challenge_nonce_digest,
        evidence_digest,
        created_at: source.created_at,
        expires_at: source.expires_at,
    })
}

/// Bind authenticated human evidence to one exact decision result. The
/// challenge digest is accepted only because the trusted ingress proof already
/// MAC-bound the server-persisted digest; raw challenge material is never
/// reconstructed or exposed.
pub fn bind_decision_resolution_owner_authority(
    actor: &ActorAssurance,
    source: DecisionResolutionAuthoritySource,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    if !all_exact_present([
        source.normalized_intent.as_str(),
        source.canonical_manifest_json.as_str(),
        source.decision_id.as_str(),
        source.selected_logical_action_id.as_str(),
        source.request_correlation_id.as_str(),
        source.idempotency_key.as_str(),
    ]) || source.decision_revision <= 0
        || source.policy_revision <= 0
        || source.created_at_ms <= 0
        || source.expires_at_ms <= source.created_at_ms
        || !is_lower_hex_digest(&source.canonical_manifest_digest)
        || !is_lower_hex_digest(&source.challenge_nonce_digest)
        || serde_json::from_str::<serde_json::Value>(&source.canonical_manifest_json).is_err()
        || source
            .revision_text_digest
            .as_deref()
            .is_some_and(|digest| !is_lower_hex_digest(digest))
        || source
            .challenge_response_digest
            .as_deref()
            .is_some_and(|digest| !is_lower_hex_digest(digest))
    {
        return Err(OwnerAuthorityBindingError::InvalidField);
    }
    let result = serde_json::to_value(source.decision_result)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(OwnerAuthorityBindingError::InvalidField)?;
    let exact_resolution = serde_json::json!({
        "decision_id": source.decision_id,
        "decision_revision": source.decision_revision,
        "selected_logical_action_id": source.selected_logical_action_id,
        "decision_result": result,
        "request_correlation_id": source.request_correlation_id,
        "idempotency_key": source.idempotency_key,
        "revision_text_digest": source.revision_text_digest,
        "challenge_response_digest": source.challenge_response_digest,
        "challenge_nonce_digest": source.challenge_nonce_digest,
    });
    let canonical_resolution = canonical_json(&exact_resolution.to_string());
    bind_verified_owner_authority_with_bound_digests(
        actor,
        OwnerAuthoritySourceInput {
            normalized_intent: source.normalized_intent,
            instruction_revision: "execass-decision-resolution-v1".to_string(),
            instruction_bytes: canonical_resolution.as_bytes().to_vec(),
            owner_envelope_revision: "execass-decision-resolution-v1".to_string(),
            owner_envelope_json: canonical_resolution.clone(),
            authority_kind: "decision_resolution".to_string(),
            normalized_scope_json: canonical_resolution,
            policy_revision: source.policy_revision,
            bound_decision_id: Some(source.decision_id),
            bound_decision_revision: Some(source.decision_revision),
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: source.created_at_ms,
            expires_at: Some(source.expires_at_ms),
        },
        Some(source.canonical_manifest_digest),
        Some(source.challenge_nonce_digest),
    )
}

/// Bind authenticated human evidence to one exact original owner request.
///
/// The actor must originate from one of the authenticated evidence functions
/// above. A runtime, model, worker, connector, tool result, or caller/wire text
/// has no `VerifiedHumanEvidence` and therefore cannot pass this boundary.
pub fn bind_original_request_owner_authority(
    actor: &ActorAssurance,
    source: OriginalRequestAuthoritySource,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    bind_verified_owner_authority(
        actor,
        OwnerAuthoritySourceInput {
            normalized_intent: source.normalized_intent,
            instruction_revision: source.instruction_revision,
            instruction_bytes: source.instruction_bytes,
            owner_envelope_revision: source.owner_envelope_revision,
            owner_envelope_json: source.canonical_owner_envelope_json,
            authority_kind: "original_request".to_string(),
            normalized_scope_json: source.canonical_scope_json,
            policy_revision: source.policy_revision,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: source.created_at_ms,
            expires_at: source.expires_at_ms,
        },
    )
}

/// Bind authenticated human evidence to one exact append-only amendment of an
/// existing delegation. This is owner authority, not a decision resolution or
/// a generic approval, and it carries no challenge or expiry semantics.
pub fn bind_follow_up_amendment_owner_authority(
    actor: &ActorAssurance,
    source: FollowUpAmendmentAuthoritySource,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    let normalized_scope_json = serde_json::to_string(&serde_json::json!({
        "delegation_id": source.target_delegation_id,
        "delegation_revision": source.expected_delegation_revision,
        "plan_revision": source.expected_plan_revision,
    }))
    .map_err(|_| OwnerAuthorityBindingError::InvalidField)?;
    bind_verified_owner_authority(
        actor,
        OwnerAuthoritySourceInput {
            normalized_intent: source.normalized_intent,
            instruction_revision: source.instruction_revision,
            instruction_bytes: source.instruction_bytes,
            owner_envelope_revision: source.owner_envelope_revision,
            owner_envelope_json: source.canonical_owner_envelope_json,
            authority_kind: "action_specific_owner_amendment".to_string(),
            normalized_scope_json,
            policy_revision: source.policy_revision,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: source.created_at_ms,
            expires_at: None,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySnapshotAuthoritySource {
    pub canonical_mutation_bytes: Vec<u8>,
    pub canonical_safe_snapshot_json: String,
    pub policy_revision: i64,
    pub policy_snapshot_digest: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSettingsSnapshotAuthoritySource {
    pub canonical_mutation_bytes: Vec<u8>,
    pub canonical_safe_snapshot_json: String,
    pub settings_revision: i64,
    pub settings_digest: String,
    pub policy_revision: i64,
    pub created_at_ms: i64,
}

/// Bind an authenticated human to one exact immutable policy snapshot.
pub fn bind_policy_snapshot_owner_authority(
    actor: &ActorAssurance,
    source: PolicySnapshotAuthoritySource,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    if source.policy_revision <= 0 || source.policy_snapshot_digest.len() != 64 {
        return Err(OwnerAuthorityBindingError::InvalidField);
    }
    let scope = serde_json::json!({
        "policy_revision": source.policy_revision,
        "policy_snapshot_digest": source.policy_snapshot_digest,
    })
    .to_string();
    bind_verified_owner_authority(
        actor,
        OwnerAuthoritySourceInput {
            normalized_intent: "update ExecAss policy snapshot".to_string(),
            instruction_revision: "execass-policy-update-v1".to_string(),
            instruction_bytes: source.canonical_mutation_bytes,
            owner_envelope_revision: "execass-policy-snapshot-v1".to_string(),
            owner_envelope_json: source.canonical_safe_snapshot_json,
            authority_kind: "policy_snapshot".to_string(),
            normalized_scope_json: scope,
            policy_revision: source.policy_revision,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: source.created_at_ms,
            expires_at: None,
        },
    )
}

/// Bind an authenticated human to one exact immutable runtime-settings snapshot.
pub fn bind_runtime_settings_snapshot_owner_authority(
    actor: &ActorAssurance,
    source: RuntimeSettingsSnapshotAuthoritySource,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    if source.settings_revision <= 0
        || source.policy_revision <= 0
        || source.settings_digest.len() != 64
    {
        return Err(OwnerAuthorityBindingError::InvalidField);
    }
    let scope = serde_json::json!({
        "settings_revision": source.settings_revision,
        "settings_digest": source.settings_digest,
    })
    .to_string();
    bind_verified_owner_authority(
        actor,
        OwnerAuthoritySourceInput {
            normalized_intent: "update ExecAss runtime settings snapshot".to_string(),
            instruction_revision: "execass-runtime-settings-update-v1".to_string(),
            instruction_bytes: source.canonical_mutation_bytes,
            owner_envelope_revision: "execass-runtime-settings-snapshot-v1".to_string(),
            owner_envelope_json: source.canonical_safe_snapshot_json,
            authority_kind: "runtime_settings_snapshot".to_string(),
            normalized_scope_json: scope,
            policy_revision: source.policy_revision,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: source.created_at_ms,
            expires_at: None,
        },
    )
}

#[cfg(any(test, feature = "execass-test-authority"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestLocalOwnerAuthorityInput {
    pub authenticated_client_id: String,
    pub authenticated_ingress: String,
    pub channel_assurance: String,
    pub request_correlation_id: String,
    pub source_message_id: Option<String>,
    pub normalized_intent: String,
    pub instruction_revision: String,
    pub instruction_bytes: Vec<u8>,
    pub owner_envelope_revision: String,
    pub owner_envelope_json: String,
    pub authority_kind: String,
    pub normalized_scope_json: String,
    pub policy_revision: i64,
    pub bound_decision_id: Option<String>,
    pub bound_decision_revision: Option<i64>,
    pub bound_manifest_bytes: Option<Vec<u8>>,
    pub challenge_nonce_bytes: Option<Vec<u8>>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// Test-only authority fixture. The feature is disabled in production builds,
/// so no public production API can manufacture authenticated owner evidence.
#[cfg(any(test, feature = "execass-test-authority"))]
pub fn issue_test_local_owner_authority(
    input: TestLocalOwnerAuthorityInput,
) -> Result<VerifiedOwnerAuthority, OwnerAuthorityBindingError> {
    let actor = derive_base_actor_assurance(
        &ServerIngressObservation::LocalInteractive(Box::new(LocalInteractiveEvidence {
            authenticated_client_id: input.authenticated_client_id,
            authenticated_ingress: input.authenticated_ingress,
            channel_assurance: input.channel_assurance,
            request_correlation_id: input.request_correlation_id,
            source_message_id: input.source_message_id,
            interactive_owner_verified: true,
        })),
        &CallerActorClaims::default(),
    );
    bind_verified_owner_authority(
        &actor,
        OwnerAuthoritySourceInput {
            normalized_intent: input.normalized_intent,
            instruction_revision: input.instruction_revision,
            instruction_bytes: input.instruction_bytes,
            owner_envelope_revision: input.owner_envelope_revision,
            owner_envelope_json: input.owner_envelope_json,
            authority_kind: input.authority_kind,
            normalized_scope_json: input.normalized_scope_json,
            policy_revision: input.policy_revision,
            bound_decision_id: input.bound_decision_id,
            bound_decision_revision: input.bound_decision_revision,
            bound_manifest_bytes: input.bound_manifest_bytes,
            challenge_nonce_bytes: input.challenge_nonce_bytes,
            created_at: input.created_at,
            expires_at: input.expires_at,
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionAssuranceFailure {
    NonHumanActor,
    ExpiredChallenge,
    ReplayedCallback,
    DecisionMismatch,
    DecisionRevisionMismatch,
    NormalizedIntentMismatch,
    PolicyRevisionMismatch,
    CanonicalManifestMismatch,
    SelectedLogicalActionMismatch,
    PresentedActionMismatch,
    DeclaredConsequenceMismatch,
    ChallengeMismatch,
    CorrelationMismatch,
    SourceMessageMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionActorAssurance {
    actor: ActorAssurance,
    failure: Option<DecisionAssuranceFailure>,
}

impl DecisionActorAssurance {
    pub fn actor(&self) -> &ActorAssurance {
        &self.actor
    }

    pub fn failure(&self) -> Option<DecisionAssuranceFailure> {
        self.failure
    }

    pub fn is_verified_owner_resolution(&self) -> bool {
        self.failure.is_none()
            && self.actor.may_resolve_human_decision()
            && self.actor.may_mint_confirmation_grant()
    }
}

/// Bind a server-derived human actor to one exact, current, unexpired decision response.
#[allow(dead_code)]
pub(crate) fn derive_decision_actor_assurance(
    observation: &ServerIngressObservation,
    caller_claims: &CallerActorClaims,
    current: &CurrentDecisionBinding,
    response: &DecisionResponseEvidence,
) -> DecisionActorAssurance {
    let actor = derive_base_actor_assurance(observation, caller_claims);
    let failure = decision_failure(observation, &actor, current, response);
    DecisionActorAssurance {
        actor: if failure.is_none() {
            actor.with_verified_decision()
        } else {
            actor
        },
        failure,
    }
}

fn decision_failure(
    observation: &ServerIngressObservation,
    actor: &ActorAssurance,
    current: &CurrentDecisionBinding,
    response: &DecisionResponseEvidence,
) -> Option<DecisionAssuranceFailure> {
    if !matches!(
        actor.actor_type(),
        DerivedActorType::HumanLocal | DerivedActorType::HumanRemote
    ) {
        return Some(DecisionAssuranceFailure::NonHumanActor);
    }
    if response.observed_at_ms >= current.expires_at_ms {
        return Some(DecisionAssuranceFailure::ExpiredChallenge);
    }
    if !response.callback_fresh {
        return Some(DecisionAssuranceFailure::ReplayedCallback);
    }
    if response.decision_id != current.decision_id {
        return Some(DecisionAssuranceFailure::DecisionMismatch);
    }
    if response.decision_revision != current.decision_revision {
        return Some(DecisionAssuranceFailure::DecisionRevisionMismatch);
    }
    if response.normalized_intent_digest != current.normalized_intent_digest {
        return Some(DecisionAssuranceFailure::NormalizedIntentMismatch);
    }
    if response.policy_revision != current.policy_revision {
        return Some(DecisionAssuranceFailure::PolicyRevisionMismatch);
    }
    if response.canonical_manifest_digest != current.canonical_manifest_digest {
        return Some(DecisionAssuranceFailure::CanonicalManifestMismatch);
    }
    if response.selected_logical_action_id != current.selected_logical_action_id {
        return Some(DecisionAssuranceFailure::SelectedLogicalActionMismatch);
    }
    if response.presented_action_digest != current.presented_action_digest {
        return Some(DecisionAssuranceFailure::PresentedActionMismatch);
    }
    if response.declared_consequence_digest != current.declared_consequence_digest {
        return Some(DecisionAssuranceFailure::DeclaredConsequenceMismatch);
    }
    if response.challenge_digest != current.challenge_digest {
        return Some(DecisionAssuranceFailure::ChallengeMismatch);
    }
    match observation {
        ServerIngressObservation::LocalInteractive(evidence) => {
            if response.request_correlation_id != evidence.request_correlation_id {
                Some(DecisionAssuranceFailure::CorrelationMismatch)
            } else if response.source_message_id.is_some() {
                Some(DecisionAssuranceFailure::SourceMessageMismatch)
            } else {
                None
            }
        }
        ServerIngressObservation::RemoteAuthenticated(evidence) => {
            if response.request_correlation_id != evidence.request_correlation_id {
                Some(DecisionAssuranceFailure::CorrelationMismatch)
            } else if response.source_message_id.as_deref()
                != Some(evidence.source_message_id.as_str())
            {
                Some(DecisionAssuranceFailure::SourceMessageMismatch)
            } else {
                None
            }
        }
        _ => Some(DecisionAssuranceFailure::NonHumanActor),
    }
}

fn nonhuman(actor_type: DerivedActorType, identity: &str, evidence: &str) -> ActorAssurance {
    ActorAssurance {
        actor_type,
        credential_identity: identity.to_string(),
        verified_evidence: vec![evidence.to_string()],
        may_submit_or_amend_owner_intent: false,
        may_resolve_human_decision: false,
        may_mint_confirmation_grant: false,
        human_evidence: None,
    }
}

fn all_present<'a>(values: impl IntoIterator<Item = &'a str>) -> bool {
    values.into_iter().all(|value| !value.trim().is_empty())
}

fn all_exact_present<'a>(values: impl IntoIterator<Item = &'a str>) -> bool {
    values.into_iter().all(|value| {
        !value.is_empty() && value.trim() == value && !value.chars().any(char::is_control)
    })
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_canonical_json_object(value: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(parsed @ serde_json::Value::Object(_)) => {
            serde_json::to_string(&parsed).is_ok_and(|canonical| canonical == value)
        }
        _ => false,
    }
}

fn digest_hex(domain: &[u8], bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    push_bytes(&mut digest, domain);
    push_bytes(&mut digest, bytes);
    format!("{:x}", digest.finalize())
}

fn plain_digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn canonical_human_evidence_bytes(evidence: &VerifiedHumanEvidence) -> Vec<u8> {
    let mut out = Vec::new();
    match evidence {
        VerifiedHumanEvidence::Local {
            authenticated_client_id,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id,
            source_message_id,
        } => {
            push_vec_bytes(&mut out, b"local");
            push_vec_bytes(&mut out, authenticated_client_id.as_bytes());
            push_vec_bytes(&mut out, authenticated_ingress.as_bytes());
            push_vec_bytes(&mut out, channel_assurance.as_bytes());
            push_vec_bytes(&mut out, request_correlation_id.as_bytes());
            push_optional_vec_bytes(&mut out, source_message_id.as_deref().map(str::as_bytes));
        }
        VerifiedHumanEvidence::Remote {
            adapter_id,
            provider_account_id,
            authenticated_ingress,
            channel_assurance,
            source_message_id,
            request_correlation_id,
            provider_event_id,
            provider_event_observed_at_ms,
            server_verified_at_ms,
        } => {
            push_vec_bytes(&mut out, b"remote");
            push_vec_bytes(&mut out, adapter_id.as_bytes());
            push_vec_bytes(&mut out, provider_account_id.as_bytes());
            push_vec_bytes(&mut out, authenticated_ingress.as_bytes());
            push_vec_bytes(&mut out, channel_assurance.as_bytes());
            push_vec_bytes(&mut out, source_message_id.as_bytes());
            push_vec_bytes(&mut out, request_correlation_id.as_bytes());
            push_optional_vec_bytes(&mut out, provider_event_id.as_deref().map(str::as_bytes));
            match provider_event_observed_at_ms {
                Some(value) => {
                    out.push(1);
                    out.extend_from_slice(&value.to_be_bytes());
                }
                None => out.push(0),
            }
            match server_verified_at_ms {
                Some(value) => {
                    out.push(1);
                    out.extend_from_slice(&value.to_be_bytes());
                }
                None => out.push(0),
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn authority_provenance_id(
    evidence_digest: &str,
    normalized_intent_digest: &str,
    source: &OwnerAuthoritySourceInput,
    instruction_digest: &str,
    owner_envelope_digest: &str,
    normalized_scope_json: &str,
    bound_manifest_digest: Option<&str>,
    bound_challenge_nonce_digest: Option<&str>,
) -> String {
    let mut out = Vec::new();
    push_vec_bytes(&mut out, evidence_digest.as_bytes());
    push_vec_bytes(&mut out, normalized_intent_digest.as_bytes());
    push_vec_bytes(&mut out, source.instruction_revision.as_bytes());
    push_vec_bytes(&mut out, instruction_digest.as_bytes());
    push_vec_bytes(&mut out, source.owner_envelope_revision.as_bytes());
    push_vec_bytes(&mut out, owner_envelope_digest.as_bytes());
    push_vec_bytes(&mut out, source.authority_kind.as_bytes());
    push_vec_bytes(&mut out, normalized_scope_json.as_bytes());
    out.extend_from_slice(&source.policy_revision.to_be_bytes());
    push_optional_vec_bytes(
        &mut out,
        source.bound_decision_id.as_deref().map(str::as_bytes),
    );
    match source.bound_decision_revision {
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
        None => out.push(0),
    }
    push_optional_vec_bytes(&mut out, bound_manifest_digest.map(str::as_bytes));
    push_optional_vec_bytes(&mut out, bound_challenge_nonce_digest.map(str::as_bytes));
    out.extend_from_slice(&source.created_at.to_be_bytes());
    match source.expires_at {
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
        None => out.push(0),
    }
    digest_hex(b"carsinos.execass.authority_provenance.v1", &out)
}

fn push_bytes(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

fn push_vec_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn push_optional_vec_bytes(out: &mut Vec<u8>, bytes: Option<&[u8]>) {
    match bytes {
        Some(bytes) => {
            out.push(1);
            push_vec_bytes(out, bytes);
        }
        None => out.push(0),
    }
}

fn canonical_json(value: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(value).expect("validated JSON");
    serde_json::to_string(&value).expect("JSON serialization")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> ServerIngressObservation {
        ServerIngressObservation::LocalInteractive(Box::new(LocalInteractiveEvidence {
            authenticated_client_id: "native-client-1".to_string(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: "corr-1".to_string(),
            source_message_id: None,
            interactive_owner_verified: true,
        }))
    }

    fn remote() -> ServerIngressObservation {
        ServerIngressObservation::RemoteAuthenticated(Box::new(RemoteOwnerEvidence {
            adapter_id: "telegram".to_string(),
            adapter_authenticated: true,
            allowlisted_provider_account_id: "owner-provider-1".to_string(),
            observed_provider_account_id: "owner-provider-1".to_string(),
            authenticated_ingress: "telegram-adapter".to_string(),
            channel_assurance: "allowlisted-remote".to_string(),
            source_message_id: "message-1".to_string(),
            request_correlation_id: "corr-1".to_string(),
            callback_fresh: true,
        }))
    }

    fn current() -> CurrentDecisionBinding {
        CurrentDecisionBinding {
            decision_id: "decision-1".to_string(),
            decision_revision: 3,
            normalized_intent_digest: "intent-digest".to_string(),
            policy_revision: 11,
            canonical_manifest_digest: "manifest-digest".to_string(),
            selected_logical_action_id: "action-1".to_string(),
            presented_action_digest: "action-digest".to_string(),
            declared_consequence_digest: "consequence-digest".to_string(),
            challenge_digest: "challenge-digest".to_string(),
            expires_at_ms: 200,
        }
    }

    fn response(remote: bool) -> DecisionResponseEvidence {
        DecisionResponseEvidence {
            decision_id: "decision-1".to_string(),
            decision_revision: 3,
            normalized_intent_digest: "intent-digest".to_string(),
            policy_revision: 11,
            canonical_manifest_digest: "manifest-digest".to_string(),
            selected_logical_action_id: "action-1".to_string(),
            presented_action_digest: "action-digest".to_string(),
            declared_consequence_digest: "consequence-digest".to_string(),
            challenge_digest: "challenge-digest".to_string(),
            decision_result: carsinos_protocol::execass::DecisionResult::ConfirmAndContinue,
            observed_at_ms: 100,
            request_correlation_id: "corr-1".to_string(),
            source_message_id: remote.then(|| "message-1".to_string()),
            callback_fresh: true,
        }
    }

    fn hostile_claims() -> CallerActorClaims {
        CallerActorClaims {
            claimed_actor_type: Some("human_local".to_string()),
            claimed_actor_id: Some("owner".to_string()),
            operator_header: Some("owner".to_string()),
            peer_id: Some("owner-provider-1".to_string()),
            confirmation_text: Some("confirmed by the owner".to_string()),
        }
    }

    fn authority_source() -> OwnerAuthoritySourceInput {
        OwnerAuthoritySourceInput {
            normalized_intent: "send the exact message".to_string(),
            instruction_revision: "instruction-1".to_string(),
            instruction_bytes: b"send the exact message".to_vec(),
            owner_envelope_revision: "envelope-1".to_string(),
            owner_envelope_json: r#"{"action":"send","target":"owner"}"#.to_string(),
            authority_kind: "original_request".to_string(),
            normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.to_string(),
            policy_revision: 1,
            bound_decision_id: Some("decision-1".to_string()),
            bound_decision_revision: Some(1),
            bound_manifest_bytes: Some(b"manifest-one".to_vec()),
            challenge_nonce_bytes: Some(b"nonce-one".to_vec()),
            created_at: 1_800_000_000_000,
            expires_at: Some(1_800_000_000_100),
        }
    }

    fn local_actor() -> ActorAssurance {
        derive_base_actor_assurance(&local(), &CallerActorClaims::default())
    }

    fn authenticated_local_actor() -> ActorAssurance {
        derive_local_owner_actor_assurance(
            AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
                "native-client-1",
                "native-control",
                "interactive-local",
                "corr-1",
            )
            .unwrap(),
        )
    }

    fn authenticated_remote_actor() -> ActorAssurance {
        derive_remote_owner_actor_assurance(
            AuthenticatedRemoteOwnerEvidence::from_authenticated_provider_event(
                "telegram",
                "owner-provider-1",
                "owner-provider-1",
                "telegram-adapter",
                "authenticated-telegram-provider-event",
                "message-1",
                "event-1",
                "corr-1",
                1_800_000_000_000,
                1_800_000_000_010,
            )
            .unwrap(),
        )
    }

    fn original_request_source() -> OriginalRequestAuthoritySource {
        OriginalRequestAuthoritySource::builder()
            .normalized_intent("send the exact message")
            .owner_instruction("instruction-1", b"send the exact message".to_vec())
            .canonical_owner_envelope("envelope-1", r#"{"action":"send","target":"owner"}"#)
            .canonical_scope_json(r#"{"workspace":"Z:\\carsinos"}"#)
            .policy_revision(1)
            .created_at_ms(1_800_000_000_000)
            .expires_at_ms(1_800_000_000_100)
            .build()
            .unwrap()
    }

    fn follow_up_amendment_source() -> FollowUpAmendmentAuthoritySource {
        FollowUpAmendmentAuthoritySource::builder()
            .target("delegation-1", 2, 1)
            .normalized_intent("clarify the exact requested outcome")
            .owner_instruction(
                "amendment-instruction-1",
                b"clarify the exact requested outcome".to_vec(),
            )
            .canonical_owner_envelope(
                "amendment-envelope-1",
                r#"{"idempotency_key":"amend-idem-1","request_id":"amend-request-1"}"#,
            )
            .policy_revision(1)
            .created_at_ms(1_800_000_000_100)
            .build()
            .unwrap()
    }

    fn assert_zero_decision_authority(result: &DecisionActorAssurance) {
        assert!(!result.is_verified_owner_resolution());
        assert!(!result.actor().may_resolve_human_decision());
        assert!(!result.actor().may_mint_confirmation_grant());
    }

    #[test]
    fn production_bridge_binds_exact_local_and_remote_original_requests() {
        for (actor, expected) in [
            (authenticated_local_actor(), DerivedActorType::HumanLocal),
            (authenticated_remote_actor(), DerivedActorType::HumanRemote),
        ] {
            assert_eq!(actor.actor_type(), expected);
            assert!(actor.may_submit_or_amend_owner_intent());
            assert!(!actor.may_resolve_human_decision());
            assert!(!actor.may_mint_confirmation_grant());

            let authority =
                bind_original_request_owner_authority(&actor, original_request_source()).unwrap();
            assert_eq!(authority.authority_kind(), "original_request");
            assert_eq!(authority.bound_decision_id(), None);
            assert_eq!(authority.bound_decision_revision(), None);
            assert_eq!(authority.bound_manifest_digest(), None);
            assert_eq!(authority.bound_challenge_nonce_digest(), None);
        }
    }

    #[test]
    fn production_bridge_binds_exact_local_and_remote_follow_up_amendments() {
        for actor in [authenticated_local_actor(), authenticated_remote_actor()] {
            let authority =
                bind_follow_up_amendment_owner_authority(&actor, follow_up_amendment_source())
                    .unwrap();
            assert_eq!(
                authority.authority_kind(),
                "action_specific_owner_amendment"
            );
            assert_eq!(
                authority.normalized_scope_json(),
                r#"{"delegation_id":"delegation-1","delegation_revision":2,"plan_revision":1}"#
            );
            assert_eq!(authority.bound_decision_id(), None);
            assert_eq!(authority.bound_manifest_digest(), None);
            assert_eq!(authority.expires_at(), None);
        }
    }

    #[test]
    fn follow_up_amendment_rejects_nonhuman_invalid_target_and_revision() {
        let nonhuman = derive_base_actor_assurance(
            &ServerIngressObservation::Model {
                model_id: "model-1".to_string(),
            },
            &CallerActorClaims::default(),
        );
        assert_eq!(
            bind_follow_up_amendment_owner_authority(&nonhuman, follow_up_amendment_source()),
            Err(OwnerAuthorityBindingError::NonHumanActor)
        );
        for (delegation_id, delegation_revision, plan_revision) in
            [("", 2, 1), ("delegation-1", 0, 1), ("delegation-1", 2, 0)]
        {
            assert_eq!(
                FollowUpAmendmentAuthoritySource::builder()
                    .target(delegation_id, delegation_revision, plan_revision)
                    .normalized_intent("clarify the exact requested outcome")
                    .owner_instruction("amendment-instruction-1", b"clarify".to_vec())
                    .canonical_owner_envelope("amendment-envelope-1", r#"{"request_id":"r"}"#)
                    .policy_revision(1)
                    .created_at_ms(1_800_000_000_100)
                    .build(),
                Err(OwnerAuthorityBindingError::InvalidField)
            );
        }
        assert_eq!(
            FollowUpAmendmentAuthoritySource::builder()
                .target("delegation-1", 2, 1)
                .normalized_intent("clarify the exact requested outcome")
                .owner_instruction("amendment-instruction-1", b"clarify".to_vec())
                .canonical_owner_envelope("amendment-envelope-1", r#"{"request_id":"r"}"#)
                .policy_revision(0)
                .created_at_ms(1_800_000_000_100)
                .build(),
            Err(OwnerAuthorityBindingError::InvalidField)
        );
    }

    #[test]
    fn every_follow_up_target_or_instruction_mutation_changes_authority() {
        let baseline = bind_follow_up_amendment_owner_authority(
            &authenticated_local_actor(),
            follow_up_amendment_source(),
        )
        .unwrap();
        for mutation in 0..6 {
            let source = FollowUpAmendmentAuthoritySource::builder()
                .target(
                    if mutation == 0 {
                        "delegation-2"
                    } else {
                        "delegation-1"
                    },
                    if mutation == 1 { 3 } else { 2 },
                    if mutation == 2 { 2 } else { 1 },
                )
                .normalized_intent(if mutation == 3 {
                    "a materially different clarification"
                } else {
                    "clarify the exact requested outcome"
                })
                .owner_instruction(
                    "amendment-instruction-1",
                    if mutation == 4 {
                        b"different exact bytes".to_vec()
                    } else {
                        b"clarify the exact requested outcome".to_vec()
                    },
                )
                .canonical_owner_envelope(
                    "amendment-envelope-1",
                    if mutation == 5 {
                        r#"{"idempotency_key":"amend-idem-2","request_id":"amend-request-1"}"#
                    } else {
                        r#"{"idempotency_key":"amend-idem-1","request_id":"amend-request-1"}"#
                    },
                )
                .policy_revision(1)
                .created_at_ms(1_800_000_000_100)
                .build()
                .unwrap();
            let changed =
                bind_follow_up_amendment_owner_authority(&authenticated_local_actor(), source)
                    .unwrap();
            assert_ne!(
                changed.authority_provenance_id(),
                baseline.authority_provenance_id(),
                "amendment mutation {mutation} did not change authority"
            );
        }
    }

    #[test]
    fn public_evidence_bridge_rejects_empty_local_and_hostile_remote_facts() {
        assert_eq!(
            AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
                "",
                "native-control",
                "interactive-local",
                "corr-1",
            ),
            Err(ActorEvidenceError::InvalidField)
        );

        let remote = |allowlisted: &str, observed: &str, source_message: &str, observed_at| {
            AuthenticatedRemoteOwnerEvidence::from_authenticated_provider_event(
                "telegram",
                allowlisted,
                observed,
                "telegram-adapter",
                "authenticated-telegram-provider-event",
                source_message,
                "event-1",
                "corr-1",
                observed_at,
                1_800_000_000_000,
            )
        };
        assert_eq!(
            remote(
                "owner-provider-1",
                "attacker-provider-account",
                "message-1",
                1_800_000_000_000,
            ),
            Err(ActorEvidenceError::RemoteAccountMismatch)
        );
        assert_eq!(
            remote(
                "owner-provider-1",
                "owner-provider-1",
                "",
                1_800_000_000_000,
            ),
            Err(ActorEvidenceError::InvalidField)
        );
        assert_eq!(
            remote(
                "owner-provider-1",
                "owner-provider-1",
                "message-1",
                1_799_999_939_999,
            ),
            Err(ActorEvidenceError::RemoteEvidenceOutsideFreshnessWindow)
        );
        assert_eq!(
            remote(
                "owner-provider-1",
                "owner-provider-1",
                "message-1",
                1_800_000_005_001,
            ),
            Err(ActorEvidenceError::RemoteEvidenceOutsideFreshnessWindow)
        );
    }

    #[test]
    fn public_binding_rejects_nonhuman_assurance_and_malformed_sources() {
        let nonhuman = derive_base_actor_assurance(
            &ServerIngressObservation::Model {
                model_id: "model-1".to_string(),
            },
            &CallerActorClaims::default(),
        );
        assert_eq!(
            bind_original_request_owner_authority(&nonhuman, original_request_source()),
            Err(OwnerAuthorityBindingError::NonHumanActor)
        );

        assert_eq!(
            OriginalRequestAuthoritySource::builder()
                .normalized_intent("send the exact message")
                .owner_instruction("instruction-1", b"send the exact message".to_vec())
                .canonical_owner_envelope(
                    "envelope-1",
                    r#"{ "action": "send", "target": "owner" }"#,
                )
                .canonical_scope_json(r#"{"workspace":"Z:\\carsinos"}"#)
                .policy_revision(1)
                .created_at_ms(1_800_000_000_000)
                .build(),
            Err(OwnerAuthorityBindingError::InvalidField)
        );
        assert_eq!(
            OriginalRequestAuthoritySource::builder()
                .normalized_intent("send the exact message")
                .owner_instruction("instruction-1", b"send the exact message".to_vec())
                .canonical_owner_envelope("envelope-1", "not-json")
                .canonical_scope_json(r#"{"workspace":"Z:\\carsinos"}"#)
                .policy_revision(1)
                .created_at_ms(1_800_000_000_000)
                .build(),
            Err(OwnerAuthorityBindingError::InvalidField)
        );
    }

    #[test]
    fn every_public_original_request_source_mutation_changes_authority() {
        let baseline = bind_original_request_owner_authority(
            &authenticated_local_actor(),
            original_request_source(),
        )
        .unwrap();
        for mutation in 0..9 {
            let source = OriginalRequestAuthoritySource::builder()
                .normalized_intent(if mutation == 0 {
                    "send the other exact message"
                } else {
                    "send the exact message"
                })
                .owner_instruction(
                    if mutation == 1 {
                        "instruction-2"
                    } else {
                        "instruction-1"
                    },
                    if mutation == 2 {
                        b"send the other exact message".to_vec()
                    } else {
                        b"send the exact message".to_vec()
                    },
                )
                .canonical_owner_envelope(
                    if mutation == 3 {
                        "envelope-2"
                    } else {
                        "envelope-1"
                    },
                    if mutation == 4 {
                        r#"{"action":"send","target":"other"}"#
                    } else {
                        r#"{"action":"send","target":"owner"}"#
                    },
                )
                .canonical_scope_json(if mutation == 5 {
                    r#"{"workspace":"Z:\\other"}"#
                } else {
                    r#"{"workspace":"Z:\\carsinos"}"#
                })
                .policy_revision(if mutation == 6 { 2 } else { 1 })
                .created_at_ms(if mutation == 7 {
                    1_800_000_000_001
                } else {
                    1_800_000_000_000
                })
                .expires_at_ms(if mutation == 8 {
                    1_800_000_000_200
                } else {
                    1_800_000_000_100
                })
                .build()
                .unwrap();
            let changed =
                bind_original_request_owner_authority(&authenticated_local_actor(), source)
                    .unwrap();
            assert_ne!(changed, baseline, "source mutation {mutation}");
            assert_ne!(
                changed.authority_provenance_id(),
                baseline.authority_provenance_id(),
                "source mutation {mutation}"
            );
        }
    }

    #[test]
    fn every_material_remote_evidence_mutation_changes_authority() {
        let authority_for = |mutation| {
            let account = if mutation == 1 {
                "owner-provider-2"
            } else {
                "owner-provider-1"
            };
            let actor = derive_remote_owner_actor_assurance(
                AuthenticatedRemoteOwnerEvidence::from_authenticated_provider_event(
                    if mutation == 0 { "discord" } else { "telegram" },
                    account,
                    account,
                    if mutation == 2 {
                        "telegram-adapter-2"
                    } else {
                        "telegram-adapter"
                    },
                    if mutation == 3 {
                        "authenticated-provider-event-v2"
                    } else {
                        "authenticated-telegram-provider-event"
                    },
                    if mutation == 4 {
                        "message-2"
                    } else {
                        "message-1"
                    },
                    if mutation == 5 { "event-2" } else { "event-1" },
                    if mutation == 6 { "corr-2" } else { "corr-1" },
                    if mutation == 7 {
                        1_800_000_000_001
                    } else {
                        1_800_000_000_000
                    },
                    if mutation == 8 {
                        1_800_000_000_011
                    } else {
                        1_800_000_000_010
                    },
                )
                .unwrap(),
            );
            bind_original_request_owner_authority(&actor, original_request_source()).unwrap()
        };
        let baseline = authority_for(usize::MAX);
        for mutation in 0..9 {
            let changed = authority_for(mutation);
            assert_ne!(changed, baseline, "remote evidence mutation {mutation}");
            assert_ne!(
                changed.authority_provenance_id(),
                baseline.authority_provenance_id(),
                "remote evidence mutation {mutation}"
            );
        }
    }

    #[test]
    fn authority_ids_and_digests_are_derived_from_canonical_source_material() {
        let source = authority_source();
        let authority = bind_verified_owner_authority(&local_actor(), source.clone()).unwrap();
        assert_eq!(
            authority.normalized_intent_digest(),
            carsinos_protocol::execass::normalized_owner_intent_digest(&source.normalized_intent)
                .unwrap()
        );
        assert_eq!(
            authority.instruction_digest(),
            carsinos_protocol::execass::owner_instruction_digest(&source.instruction_bytes)
                .unwrap()
        );
        let canonical_envelope = canonical_json(&source.owner_envelope_json);
        assert_eq!(
            authority.owner_envelope_digest(),
            digest_hex(
                b"carsinos.execass.owner_envelope.v1",
                canonical_envelope.as_bytes()
            )
        );
        assert_eq!(authority.authority_provenance_id().len(), 64);
        assert_eq!(authority.evidence_digest().len(), 64);
        assert_eq!(authority.bound_manifest_digest().unwrap().len(), 64);
        assert_eq!(authority.bound_challenge_nonce_digest().unwrap().len(), 64);
    }

    #[test]
    fn normalized_intent_digest_is_domain_separated_not_plain_sha256() {
        let intent = "prepare the requested bounded result";
        let canonical = owner_normalized_intent_digest(intent).unwrap();
        assert_eq!(
            canonical,
            carsinos_protocol::execass::normalized_owner_intent_digest(intent).unwrap()
        );
        assert_ne!(canonical, plain_digest_hex(intent.as_bytes()));
        assert!(owner_normalized_intent_digest("   ").is_none());
    }

    #[test]
    fn every_canonical_authority_source_mutation_changes_the_opaque_authority() {
        let baseline = bind_verified_owner_authority(&local_actor(), authority_source()).unwrap();
        for mutation in 0..14 {
            let mut source = authority_source();
            match mutation {
                0 => source.normalized_intent = "other intent".to_string(),
                1 => source.instruction_revision = "instruction-2".to_string(),
                2 => source.instruction_bytes = b"other instruction".to_vec(),
                3 => source.owner_envelope_revision = "envelope-2".to_string(),
                4 => source.owner_envelope_json = r#"{"action":"other"}"#.to_string(),
                5 => source.authority_kind = "policy_snapshot".to_string(),
                6 => source.normalized_scope_json = r#"{"workspace":"Z:\\other"}"#.to_string(),
                7 => source.policy_revision = 2,
                8 => source.bound_decision_id = Some("decision-2".to_string()),
                9 => source.bound_decision_revision = Some(2),
                10 => source.bound_manifest_bytes = Some(b"manifest-two".to_vec()),
                11 => source.challenge_nonce_bytes = Some(b"nonce-two".to_vec()),
                12 => source.created_at += 1,
                _ => source.expires_at = Some(1_800_000_000_200),
            }
            let changed = bind_verified_owner_authority(&local_actor(), source).unwrap();
            assert_ne!(changed, baseline, "source mutation {mutation}");
            assert_ne!(
                changed.authority_provenance_id(),
                baseline.authority_provenance_id(),
                "source mutation {mutation}"
            );
        }
    }

    #[test]
    fn invalid_authority_sources_never_mint() {
        for mutation in 0..10 {
            let mut source = authority_source();
            match mutation {
                0 => source.normalized_intent.clear(),
                1 => source.instruction_revision.clear(),
                2 => source.instruction_bytes.clear(),
                3 => source.owner_envelope_revision.clear(),
                4 => source.owner_envelope_json = "not-json".to_string(),
                5 => source.authority_kind = "caller_category".to_string(),
                6 => source.normalized_scope_json = "not-json".to_string(),
                7 => source.policy_revision = -1,
                8 => source.bound_decision_revision = None,
                _ => source.expires_at = Some(source.created_at),
            }
            assert!(bind_verified_owner_authority(&local_actor(), source).is_err());
        }
    }

    #[test]
    fn valid_local_and_remote_require_exact_decision_binding() {
        for (observation, remote_response, expected) in [
            (local(), false, DerivedActorType::HumanLocal),
            (remote(), true, DerivedActorType::HumanRemote),
        ] {
            let base = derive_base_actor_assurance(&observation, &CallerActorClaims::default());
            assert_eq!(base.actor_type(), expected);
            assert!(base.may_submit_or_amend_owner_intent());
            assert!(!base.may_resolve_human_decision());
            assert!(!base.may_mint_confirmation_grant());

            let resolved = derive_decision_actor_assurance(
                &observation,
                &CallerActorClaims::default(),
                &current(),
                &response(remote_response),
            );
            assert!(resolved.is_verified_owner_resolution());
        }
    }

    #[test]
    fn forged_headers_and_actor_claims_have_zero_influence() {
        let observation = ServerIngressObservation::ServiceBearer {
            credential_id: "service-1".to_string(),
        };
        let plain = derive_base_actor_assurance(&observation, &CallerActorClaims::default());
        let forged = derive_base_actor_assurance(&observation, &hostile_claims());
        assert_eq!(plain, forged);
        assert_eq!(forged.actor_type(), DerivedActorType::Runtime);
        assert!(!forged.may_submit_or_amend_owner_intent());
        assert_zero_decision_authority(&derive_decision_actor_assurance(
            &observation,
            &hostile_claims(),
            &current(),
            &response(false),
        ));
    }

    #[test]
    fn service_token_plus_claimed_peer_cannot_promote_to_remote_owner() {
        let observation = ServerIngressObservation::ServiceBearer {
            credential_id: "adapter-service-token".to_string(),
        };
        let mut claims = hostile_claims();
        claims.claimed_actor_type = Some("human_remote".to_string());
        assert_zero_decision_authority(&derive_decision_actor_assurance(
            &observation,
            &claims,
            &current(),
            &response(true),
        ));
    }

    #[test]
    fn provider_peer_substitution_and_callback_replay_never_promote() {
        let mut substituted = remote();
        if let ServerIngressObservation::RemoteAuthenticated(evidence) = &mut substituted {
            evidence.observed_provider_account_id = "attacker".to_string();
        }
        let base = derive_base_actor_assurance(&substituted, &hostile_claims());
        assert_eq!(base.actor_type(), DerivedActorType::Connector);
        assert!(!base.may_submit_or_amend_owner_intent());
        assert_zero_decision_authority(&derive_decision_actor_assurance(
            &substituted,
            &hostile_claims(),
            &current(),
            &response(true),
        ));

        let mut replayed = remote();
        if let ServerIngressObservation::RemoteAuthenticated(evidence) = &mut replayed {
            evidence.callback_fresh = false;
        }
        assert_zero_decision_authority(&derive_decision_actor_assurance(
            &replayed,
            &hostile_claims(),
            &current(),
            &response(true),
        ));
    }

    #[test]
    fn model_worker_connector_and_related_text_never_resolve() {
        for observation in [
            ServerIngressObservation::Runtime {
                runtime_id: "runtime-1".to_string(),
            },
            ServerIngressObservation::Connector {
                connector_id: "connector-1".to_string(),
            },
            ServerIngressObservation::Worker {
                worker_id: "worker-1".to_string(),
            },
            ServerIngressObservation::Model {
                model_id: "model-1".to_string(),
            },
            ServerIngressObservation::RetrievedContent {
                source_id: "document-1".to_string(),
            },
            ServerIngressObservation::ToolOutput {
                tool_id: "tool-1".to_string(),
            },
            ServerIngressObservation::ChildAgent {
                child_id: "child-1".to_string(),
            },
        ] {
            assert_zero_decision_authority(&derive_decision_actor_assurance(
                &observation,
                &hostile_claims(),
                &current(),
                &response(false),
            ));
        }
    }

    #[test]
    fn local_authentication_is_interactive_and_correlation_bound() {
        let mut unverified = local();
        if let ServerIngressObservation::LocalInteractive(evidence) = &mut unverified {
            evidence.interactive_owner_verified = false;
        }
        let base = derive_base_actor_assurance(&unverified, &hostile_claims());
        assert_eq!(base.actor_type(), DerivedActorType::Runtime);
        assert!(!base.may_submit_or_amend_owner_intent());

        let mut mismatch = response(false);
        mismatch.request_correlation_id = "other-correlation".to_string();
        let result = derive_decision_actor_assurance(
            &local(),
            &CallerActorClaims::default(),
            &current(),
            &mismatch,
        );
        assert_eq!(
            result.failure(),
            Some(DecisionAssuranceFailure::CorrelationMismatch)
        );
        assert_zero_decision_authority(&result);
    }

    #[test]
    fn every_decision_binding_mutation_has_zero_authority() {
        for mutation in 0..14 {
            let mut candidate = response(true);
            let mut binding = current();
            match mutation {
                0 => candidate.decision_id = "other".to_string(),
                1 => candidate.decision_revision += 1,
                2 => candidate.normalized_intent_digest = "other".to_string(),
                3 => candidate.policy_revision += 1,
                4 => candidate.canonical_manifest_digest = "other".to_string(),
                5 => candidate.selected_logical_action_id = "other".to_string(),
                6 => candidate.presented_action_digest = "other".to_string(),
                7 => candidate.declared_consequence_digest = "other".to_string(),
                8 => candidate.challenge_digest = "other".to_string(),
                9 => candidate.observed_at_ms = binding.expires_at_ms,
                10 => candidate.request_correlation_id = "other".to_string(),
                11 => candidate.source_message_id = Some("other".to_string()),
                12 => candidate.callback_fresh = false,
                _ => binding.challenge_digest = "new-current-challenge".to_string(),
            }
            assert_zero_decision_authority(&derive_decision_actor_assurance(
                &remote(),
                &hostile_claims(),
                &binding,
                &candidate,
            ));
        }
    }

    #[test]
    fn valid_owner_actor_is_not_enough_without_a_decision_response() {
        for observation in [local(), remote()] {
            let base = derive_base_actor_assurance(&observation, &hostile_claims());
            assert!(base.may_submit_or_amend_owner_intent());
            assert!(!base.may_resolve_human_decision());
            assert!(!base.may_mint_confirmation_grant());
            assert!(base.may_work_within_existing_owner_authority());
        }
    }
}
