//! Structural intake classification for ExecAss.
//!
//! Adapters must first reduce a request to this server-owned execution shape.
//! User text, purpose, morality, commerce, and action categories deliberately
//! have no representation here.

use crate::execass_actor_gate::BaseActorAssurance;
use carsinos_core::execass_actor::{
    bind_follow_up_amendment_owner_authority, bind_original_request_owner_authority,
    owner_normalized_intent_digest, FollowUpAmendmentAuthoritySource,
    OriginalRequestAuthoritySource, VerifiedHumanEvidenceRef, VerifiedOwnerAuthority,
};
use carsinos_core::execass_manifest::{
    compile_dispatch, CanonicalValue, DispatchAction, DispatchNode, DispatchTree,
    ManifestCompilation, ResolvedLeafInput, ServerResolutionRegistry, TargetSnapshotInput,
    ToolIdentityInput,
};
use carsinos_core::execass_policy::{
    authorize_exact_owner_leaf, evaluate_objective_technical_validity, ActionIdentityResolution,
    CapabilityAvailability, ExactOwnerActionAuthority, ExactOwnerAuthorityInput,
    ExactOwnerAuthorityOutcome, ObjectiveTechnicalValidityFacts, OperandResolution,
    ReconciliationAvailability, ResourceAvailability, RuntimePreconditionState,
    TransactionFencingState,
};
use carsinos_protocol::execass::{DecisionKind, IntakeRequest};
use carsinos_storage::execass::{
    ActorType, AmendLifecycleCommand, AuthorityKind, AuthorityProvenanceRecord,
    CreateFoundationCommand, CriterionPredicate, DelegationPhase, DelegationRecord,
    EligibleFollowUpTarget, ExecAssChannelProvider, NewOutboxEvent, OutboxEventName,
    OutcomeCriterionRecord, PlanRecord, PredicateVersion, RunControlState, SafeText,
    VerifiedFollowUpAmendmentOutcome, VerifierType, WriteContext,
};
use serde_json::json;
use sha2::{Digest as _, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ImmediateResponseShape {
    Absent,
    Empty,
    NonEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableTrigger {
    ToolExecution,
    SideEffect,
    Delay,
    Schedule,
    Worker,
    DurableMutation,
    HumanDecision(DecisionKind),
    DurableReceipt,
}

/// Server-owned facts about how a request would execute.
///
/// This type is intentionally not deserializable. Channel adapters may build
/// it only after server-side capability and execution-plan assessment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionShapeAssessment {
    immediate_response: ImmediateResponseShape,
    synchronous_read_only_proven: bool,
    authenticated_audit: bool,
    durable_triggers: Vec<DurableTrigger>,
    ambiguous: bool,
    inconsistent: bool,
}

impl ExecutionShapeAssessment {
    pub(crate) fn new(immediate_response: ImmediateResponseShape) -> Self {
        Self {
            immediate_response,
            synchronous_read_only_proven: false,
            authenticated_audit: false,
            durable_triggers: Vec::new(),
            ambiguous: false,
            inconsistent: false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_synchronous_read_only_proof(mut self) -> Self {
        self.synchronous_read_only_proven = true;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_authenticated_audit(mut self) -> Self {
        self.authenticated_audit = true;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_durable_trigger(mut self, trigger: DurableTrigger) -> Self {
        self.durable_triggers.push(trigger);
        self
    }

    pub(crate) fn with_ambiguity(mut self) -> Self {
        self.ambiguous = true;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_inconsistency(mut self) -> Self {
        self.inconsistent = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntakeDisposition {
    Conversational,
    SynchronousReadOnly,
    Durable,
}

/// Canonically ordered facts explaining an intake disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntakeReason {
    AmbiguousExecutionShape,
    InconsistentExecutionShape,
    ReadOnlyDurableConflict,
    ToolExecution,
    SideEffect,
    Delay,
    Schedule,
    Worker,
    DurableMutation,
    HumanDecision(DecisionKind),
    DurableReceipt,
    MissingAuthenticatedAudit,
    MissingImmediateResponse,
    EmptyImmediateResponse,
    ImmediateResponseOnly,
    SynchronousReadOnlyProven,
    AuthenticatedAudit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntakeClassification {
    disposition: IntakeDisposition,
    reasons: Vec<IntakeReason>,
}

impl IntakeClassification {
    pub(crate) fn disposition(&self) -> IntakeDisposition {
        self.disposition
    }

    pub(crate) fn reasons(&self) -> &[IntakeReason] {
        &self.reasons
    }
}

fn intake_reason_code(reason: IntakeReason) -> &'static str {
    match reason {
        IntakeReason::AmbiguousExecutionShape => "ambiguous_execution_shape",
        IntakeReason::InconsistentExecutionShape => "inconsistent_execution_shape",
        IntakeReason::ReadOnlyDurableConflict => "read_only_durable_conflict",
        IntakeReason::ToolExecution => "tool_execution",
        IntakeReason::SideEffect => "side_effect",
        IntakeReason::Delay => "delay",
        IntakeReason::Schedule => "schedule",
        IntakeReason::Worker => "worker",
        IntakeReason::DurableMutation => "durable_mutation",
        IntakeReason::HumanDecision(DecisionKind::Clarification) => "human_decision:clarification",
        IntakeReason::HumanDecision(DecisionKind::DangerousActionConfirmation) => {
            "human_decision:dangerous_action_confirmation"
        }
        IntakeReason::HumanDecision(DecisionKind::OwnerConfiguredCheckpoint) => {
            "human_decision:owner_configured_checkpoint"
        }
        IntakeReason::HumanDecision(DecisionKind::RecoveryChoice) => {
            "human_decision:recovery_choice"
        }
        IntakeReason::HumanDecision(DecisionKind::DuplicateRiskRetry) => {
            "human_decision:duplicate_risk_retry"
        }
        IntakeReason::HumanDecision(DecisionKind::Stop) => "human_decision:stop",
        IntakeReason::HumanDecision(DecisionKind::PolicyChange) => "human_decision:policy_change",
        IntakeReason::DurableReceipt => "durable_receipt",
        IntakeReason::MissingAuthenticatedAudit => "missing_authenticated_audit",
        IntakeReason::MissingImmediateResponse => "missing_immediate_response",
        IntakeReason::EmptyImmediateResponse => "empty_immediate_response",
        IntakeReason::ImmediateResponseOnly => "immediate_response_only",
        IntakeReason::SynchronousReadOnlyProven => "synchronous_read_only_proven",
        IntakeReason::AuthenticatedAudit => "authenticated_audit",
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ExecAssIntakeService;

#[derive(Debug, Clone)]
pub(crate) struct PreparedDurableFoundation {
    pub(crate) command: CreateFoundationCommand,
    pub(crate) dispatch: DispatchTree,
    pub(crate) resolutions: ServerResolutionRegistry,
    pub(crate) authorized_actions: Vec<ExactOwnerActionAuthority>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedFollowUpAmendment {
    pub(crate) amendment: AmendLifecycleCommand,
    pub(crate) manifest: carsinos_core::execass_manifest::CanonicalLeafManifest,
    pub(crate) authority: VerifiedOwnerAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FollowUpAmendmentWriteOutcome {
    Applied { delegation_id: String },
    Replayed { delegation_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WrongAttachmentReason {
    MissingExplicitTarget {
        delegation_id: String,
    },
    TerminalExplicitTarget {
        delegation_id: String,
    },
    IneligibleExplicitTarget {
        delegation_id: String,
    },
    MissingReplyTarget,
    TerminalReplyTarget {
        delegation_id: String,
    },
    InvalidReplyProvider,
    SignalsDisagree {
        explicit_delegation_id: String,
        reply_delegation_id: String,
    },
    SignalsChanged,
    StaleTarget {
        current_state_revision: i64,
    },
    ReplayMaterialMismatch {
        delegation_id: String,
    },
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum VerifiedOwnerIntakeOutcome {
    Conversational(#[allow(dead_code)] IntakeClassification),
    SynchronousReadOnly(#[allow(dead_code)] IntakeClassification),
    Durable {
        classification: IntakeClassification,
        admission: crate::GatewayFoundationAdmissionOutcome,
    },
    Amendment {
        classification: IntakeClassification,
        outcome: FollowUpAmendmentWriteOutcome,
    },
    WrongAttachment {
        classification: IntakeClassification,
        reason: WrongAttachmentReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableFoundationBuildFailure {
    NonDurable(IntakeDisposition),
    NonOriginalRequestAuthority,
    AuthorityRequestMismatch,
    UnsafeIntent,
    ManifestCompilationFailed,
    OwnerAuthorityRejected,
    AuthorityBindingFailed,
    SerializationFailed,
}

enum AttachmentResolution {
    Foundation,
    Target(EligibleFollowUpTarget),
    Replayed { delegation_id: String },
    Wrong(WrongAttachmentReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AmendmentReplayMatch {
    Absent,
    Exact,
    MaterialMismatch,
}

impl ExecAssIntakeService {
    pub(crate) fn classify(&self, assessment: &ExecutionShapeAssessment) -> IntakeClassification {
        let mut reasons = Vec::new();

        push_if(
            &mut reasons,
            assessment.ambiguous,
            IntakeReason::AmbiguousExecutionShape,
        );
        push_if(
            &mut reasons,
            assessment.inconsistent,
            IntakeReason::InconsistentExecutionShape,
        );
        push_if(
            &mut reasons,
            assessment.synchronous_read_only_proven && !assessment.durable_triggers.is_empty(),
            IntakeReason::ReadOnlyDurableConflict,
        );

        for (trigger, reason) in [
            (DurableTrigger::ToolExecution, IntakeReason::ToolExecution),
            (DurableTrigger::SideEffect, IntakeReason::SideEffect),
            (DurableTrigger::Delay, IntakeReason::Delay),
            (DurableTrigger::Schedule, IntakeReason::Schedule),
            (DurableTrigger::Worker, IntakeReason::Worker),
            (
                DurableTrigger::DurableMutation,
                IntakeReason::DurableMutation,
            ),
        ] {
            push_if(
                &mut reasons,
                assessment.durable_triggers.contains(&trigger),
                reason,
            );
        }

        for kind in ALL_DECISION_KINDS {
            push_if(
                &mut reasons,
                assessment
                    .durable_triggers
                    .contains(&DurableTrigger::HumanDecision(kind)),
                IntakeReason::HumanDecision(kind),
            );
        }

        push_if(
            &mut reasons,
            assessment
                .durable_triggers
                .contains(&DurableTrigger::DurableReceipt),
            IntakeReason::DurableReceipt,
        );
        push_if(
            &mut reasons,
            assessment.synchronous_read_only_proven && !assessment.authenticated_audit,
            IntakeReason::MissingAuthenticatedAudit,
        );

        if !reasons.is_empty() {
            return IntakeClassification {
                disposition: IntakeDisposition::Durable,
                reasons,
            };
        }

        if assessment.synchronous_read_only_proven {
            return IntakeClassification {
                disposition: IntakeDisposition::SynchronousReadOnly,
                reasons: vec![
                    IntakeReason::SynchronousReadOnlyProven,
                    IntakeReason::AuthenticatedAudit,
                ],
            };
        }

        match assessment.immediate_response {
            ImmediateResponseShape::NonEmpty => IntakeClassification {
                disposition: IntakeDisposition::Conversational,
                reasons: vec![IntakeReason::ImmediateResponseOnly],
            },
            ImmediateResponseShape::Absent => IntakeClassification {
                disposition: IntakeDisposition::Durable,
                reasons: vec![IntakeReason::MissingImmediateResponse],
            },
            ImmediateResponseShape::Empty => IntakeClassification {
                disposition: IntakeDisposition::Durable,
                reasons: vec![IntakeReason::EmptyImmediateResponse],
            },
        }
    }

    /// Bind one exact original request only from the opaque capability emitted
    /// by the gateway actor gate. Raw instruction bytes are hashed by core and
    /// are never returned for persistence; normalized intent is redacted first.
    pub(crate) fn bind_original_request_authority(
        &self,
        actor: &BaseActorAssurance,
        request: &IntakeRequest,
        policy_revision: i64,
        created_at_ms: i64,
    ) -> Result<VerifiedOwnerAuthority, IntakeAuthorityFailure> {
        if !actor.may_submit_or_amend_owner_intent() {
            return Err(IntakeAuthorityFailure::NonHumanActor);
        }
        if actor.request_correlation_id() != request.source_correlation_id {
            return Err(IntakeAuthorityFailure::CorrelationMismatch);
        }
        if actor
            .source_message_id()
            .is_some_and(|source| source != request.request_id)
        {
            return Err(IntakeAuthorityFailure::SourceMessageMismatch);
        }
        if actor.verified_request_id() != Some(request.request_id.as_str()) {
            return Err(IntakeAuthorityFailure::RequestBindingMismatch);
        }
        if actor.verified_idempotency_key() != Some(request.idempotency_key.as_str()) {
            return Err(IntakeAuthorityFailure::IdempotencyBindingMismatch);
        }
        if actor.verified_attach_to_delegation_id() != request.attach_to_delegation_id.as_deref() {
            return Err(IntakeAuthorityFailure::RequestBindingMismatch);
        }
        let core_actor = actor
            .owner_actor_assurance()
            .ok_or(IntakeAuthorityFailure::MissingOwnerCapability)?;
        let safe_intent = SafeText::new(request.text.trim(), &[])
            .map_err(|_| IntakeAuthorityFailure::UnsafeIntent)?;
        let intent_digest =
            carsinos_core::execass_actor::owner_normalized_intent_digest(safe_intent.as_str())
                .ok_or(IntakeAuthorityFailure::InvalidRequest)?;
        if actor.verified_normalized_intent_digest() != Some(intent_digest.as_str()) {
            return Err(IntakeAuthorityFailure::IntentBindingMismatch);
        }
        let instruction_digest =
            carsinos_protocol::execass::owner_instruction_digest(request.text.as_bytes())
                .ok_or(IntakeAuthorityFailure::InvalidRequest)?;
        if actor.verified_instruction_digest() != Some(instruction_digest.as_str()) {
            return Err(IntakeAuthorityFailure::InstructionBindingMismatch);
        }
        bind_original_request_core(core_actor, request, policy_revision, created_at_ms)
    }

    /// Prepare the one-leaf Accepted foundation for later canonical planning.
    /// This method does not persist, create a continuation, dispatch a tool, or
    /// perform any part of the owner's eventual semantic request.
    pub(crate) fn prepare_durable_foundation(
        &self,
        classification: &IntakeClassification,
        request: &IntakeRequest,
        authority: &VerifiedOwnerAuthority,
        admitted_at_ms: i64,
    ) -> Result<PreparedDurableFoundation, DurableFoundationBuildFailure> {
        if classification.disposition != IntakeDisposition::Durable {
            return Err(DurableFoundationBuildFailure::NonDurable(
                classification.disposition,
            ));
        }
        if authority.authority_kind() != "original_request" {
            return Err(DurableFoundationBuildFailure::NonOriginalRequestAuthority);
        }
        if admitted_at_ms < 0 {
            return Err(DurableFoundationBuildFailure::AuthorityRequestMismatch);
        }

        let redacted_intent = SafeText::new(request.text.trim(), &[])
            .map_err(|_| DurableFoundationBuildFailure::UnsafeIntent)?;
        let redacted_intent = redacted_intent.as_str();
        if request.request_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.source_correlation_id.is_empty()
            || owner_normalized_intent_digest(redacted_intent).as_deref()
                != Some(authority.normalized_intent_digest())
        {
            return Err(DurableFoundationBuildFailure::AuthorityRequestMismatch);
        }

        let (
            actor_type,
            credential_identity,
            authenticated_ingress,
            channel_assurance,
            source_correlation_id,
            source_message_id,
        ) = authority_storage_evidence(authority);
        if source_correlation_id != request.source_correlation_id
            || source_message_id
                .as_deref()
                .is_some_and(|message_id| message_id != request.request_id)
        {
            return Err(DurableFoundationBuildFailure::AuthorityRequestMismatch);
        }

        let identity = FoundationIngressIdentity {
            authenticated_ingress: &authenticated_ingress,
            credential_identity: &credential_identity,
            source_correlation_id: &source_correlation_id,
            request_id: &request.request_id,
            idempotency_key: &request.idempotency_key,
        };
        let delegation_id = deterministic_foundation_id("delegation", &identity);
        let plan_id = deterministic_foundation_id("plan", &identity);
        let criterion_id = deterministic_foundation_id("criterion", &identity);
        let event_id = deterministic_foundation_id("outbox", &identity);
        let write_id = deterministic_foundation_id("write", &identity);
        let logical_action_id = deterministic_foundation_id("planning_leaf", &identity);
        let node_id = deterministic_foundation_id("dispatch_node", &identity);

        let dispatch = DispatchTree {
            root_id: node_id.clone(),
            nodes: vec![DispatchNode {
                node_id,
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id: logical_action_id.clone(),
                    action_kind:
                        crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_ACTION_KIND
                            .to_string(),
                    tool: ToolIdentityInput {
                        tool_id: crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_TOOL_ID
                            .to_string(),
                        version:
                            crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_TOOL_VERSION
                                .to_string(),
                    },
                    operands: CanonicalValue::Object(vec![
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "resolved_target".to_string(),
                            value: CanonicalValue::String(delegation_id.clone()),
                        },
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "intent".to_string(),
                            value: CanonicalValue::String(redacted_intent.to_string()),
                        },
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "request_id".to_string(),
                            value: CanonicalValue::String(request.request_id.clone()),
                        },
                    ]),
                    target_snapshot: TargetSnapshotInput {
                        targets: vec![CanonicalValue::String(delegation_id.clone())],
                    },
                    material_digest: None,
                    owner_authority: authority.clone(),
                })),
            }],
        };
        let resolutions = ServerResolutionRegistry::default();
        let ManifestCompilation::Ready(manifest) = compile_dispatch(&dispatch, &resolutions) else {
            return Err(DurableFoundationBuildFailure::ManifestCompilationFailed);
        };
        let Some(leaf) = manifest.leaves().first() else {
            return Err(DurableFoundationBuildFailure::ManifestCompilationFailed);
        };

        // Every fact is mechanically true for this exact non-effect planning
        // leaf: its fixed action and operands just compiled; the capability is
        // this in-process builder; no external runtime precondition, fenced
        // transaction, reconciliation adapter, or external resource is needed
        // to create an Accepted foundation. Later plan/effect phases must make
        // fresh technical assessments for their own exact leaves.
        let technical_validity = evaluate_objective_technical_validity(
            leaf,
            ObjectiveTechnicalValidityFacts {
                action_identity: ActionIdentityResolution::Resolved,
                operands: OperandResolution::Resolved,
                capability: CapabilityAvailability::Available,
                runtime_precondition: RuntimePreconditionState::Met,
                transaction_and_fencing: TransactionFencingState::Valid,
                reconciliation: ReconciliationAvailability::Available,
                resources: ResourceAvailability::Available,
            },
        );
        let ExactOwnerAuthorityOutcome::Authorized(exact_authority) =
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: authority,
                canonical_leaf: leaf,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &technical_validity,
            })
        else {
            return Err(DurableFoundationBuildFailure::OwnerAuthorityRejected);
        };

        let manifest_json = String::from_utf8(manifest.canonical().bytes().to_vec())
            .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let manifest_digest = manifest.canonical().digest().as_hex().to_string();
        let authority_record = AuthorityProvenanceRecord {
            authority_provenance_id: authority.authority_provenance_id().to_string(),
            actor_type,
            credential_identity: credential_identity.clone(),
            authenticated_ingress: authenticated_ingress.clone(),
            channel_assurance,
            source_correlation_id: source_correlation_id.clone(),
            source_message_id: source_message_id.clone(),
            authority_kind: AuthorityKind::OriginalRequest,
            normalized_scope_json: authority.normalized_scope_json().to_string(),
            policy_revision: authority.policy_revision(),
            bound_decision_id: authority.bound_decision_id().map(str::to_string),
            bound_decision_revision: authority.bound_decision_revision(),
            bound_manifest_digest: authority.bound_manifest_digest().map(str::to_string),
            bound_challenge_nonce_digest: authority
                .bound_challenge_nonce_digest()
                .map(str::to_string),
            evidence_digest: authority.evidence_digest().to_string(),
            created_at: authority.created_at(),
            expires_at: authority.expires_at(),
        };
        let intake_evidence_json = serde_json::to_string(&json!({
            "authority_provenance_id": authority.authority_provenance_id(),
            "request_id": request.request_id,
            "source_correlation_id": source_correlation_id,
        }))
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let effective_authority_json = serde_json::to_string(&json!({
            "authority_kind": "original_request",
            "authority_provenance_id": authority.authority_provenance_id(),
            "instruction_digest": authority.instruction_digest(),
        }))
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let classifier_reasons_json = serde_json::to_string(
            &classification
                .reasons()
                .iter()
                .copied()
                .map(intake_reason_code)
                .collect::<Vec<_>>(),
        )
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let outbox_payload = serde_json::to_string(&json!({
            "delegation_id": delegation_id,
            "phase": "accepted",
            "summary": "owner request admitted for canonical planning",
        }))
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;

        let command = CreateFoundationCommand {
            write: WriteContext {
                idempotency_key: write_id.clone(),
                correlation_id: source_correlation_id.clone(),
                causation_id: request.request_id.clone(),
                occurred_at: admitted_at_ms,
            },
            authority: authority_record,
            delegation: DelegationRecord {
                delegation_id: delegation_id.clone(),
                normalized_original_intent: redacted_intent.to_string(),
                intake_evidence_json,
                ingress_source: authenticated_ingress,
                ingress_credential_identity: credential_identity,
                source_message_id,
                source_correlation_id: source_correlation_id.clone(),
                ingress_idempotency_key: write_id.clone(),
                classifier_version: "execass.structural.v1".to_string(),
                classifier_reasons_json,
                phase: DelegationPhase::Accepted,
                run_control: RunControlState::Running,
                state_revision: 1,
                current_plan_revision: Some(1),
                current_criteria_revision: Some(1),
                policy_revision: authority.policy_revision(),
                effective_authority_json,
                authority_provenance_id: authority.authority_provenance_id().to_string(),
                pending_decision_id: None,
                external_wait_json: None,
                stop_epoch: 0,
                completion_assessment_json: None,
                receipt_chain_count: 0,
                receipt_chain_head_digest: None,
                created_at: admitted_at_ms,
                updated_at: admitted_at_ms,
                acknowledged_at: None,
                terminal_at: None,
            },
            plan: PlanRecord {
                plan_id,
                delegation_id: delegation_id.clone(),
                plan_revision: 1,
                based_on_delegation_revision: 1,
                policy_revision: authority.policy_revision(),
                plan_summary: "produce the canonical plan for the redacted owner intent"
                    .to_string(),
                resolved_leaf_manifest_json: manifest_json,
                manifest_digest,
                created_by_authority_provenance_id: authority.authority_provenance_id().to_string(),
                created_at: admitted_at_ms,
            },
            outcome_criteria: vec![OutcomeCriterionRecord {
                criterion_id,
                delegation_id: delegation_id.clone(),
                criteria_revision: 1,
                criterion_key: "canonical_plan_produced".to_string(),
                description: "a canonical plan is produced for the admitted owner request"
                    .to_string(),
                material: true,
                verifier_type: VerifierType::DatabasePredicate,
                expected_predicate_json: serde_json::to_string(
                    &CriterionPredicate::DatabasePredicate {
                        version: PredicateVersion::V1,
                        delegation_id: delegation_id.clone(),
                        canonical_plan_revision_greater_than: 0,
                    },
                )
                .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?,
                authoritative_source_kind: "execass_plan_store".to_string(),
                created_at: admitted_at_ms,
            }],
            initial_continuation: None,
            outbox_event: NewOutboxEvent {
                event_id,
                event_name: OutboxEventName::DelegationTransitioned,
                aggregate_id: delegation_id,
                aggregate_revision: 1,
                correlation_id: source_correlation_id,
                causation_id: request.request_id.clone(),
                occurred_at: admitted_at_ms,
                safe_payload_json: outbox_payload,
                duplicate_identity: write_id,
            },
        };

        Ok(PreparedDurableFoundation {
            command,
            dispatch,
            resolutions,
            authorized_actions: vec![exact_authority],
        })
    }

    fn prepare_follow_up_amendment(
        &self,
        actor: &BaseActorAssurance,
        request: &IntakeRequest,
        original_authority: &VerifiedOwnerAuthority,
        target: &EligibleFollowUpTarget,
        current: &carsinos_storage::execass::FoundationBundle,
        admitted_at_ms: i64,
    ) -> Result<PreparedFollowUpAmendment, DurableFoundationBuildFailure> {
        if admitted_at_ms < 0
            || current.delegation.delegation_id != target.delegation_id
            || current.delegation.state_revision != target.state_revision
            || current.delegation.current_plan_revision != Some(target.plan_revision)
        {
            return Err(DurableFoundationBuildFailure::AuthorityRequestMismatch);
        }
        let criteria_revision = current
            .delegation
            .current_criteria_revision
            .ok_or(DurableFoundationBuildFailure::AuthorityRequestMismatch)?
            .checked_add(1)
            .ok_or(DurableFoundationBuildFailure::AuthorityRequestMismatch)?;
        let next_state_revision = target
            .state_revision
            .checked_add(1)
            .ok_or(DurableFoundationBuildFailure::AuthorityRequestMismatch)?;
        let next_plan_revision = target
            .plan_revision
            .checked_add(1)
            .ok_or(DurableFoundationBuildFailure::AuthorityRequestMismatch)?;
        let redacted_amendment = SafeText::new(request.text.trim(), &[])
            .map_err(|_| DurableFoundationBuildFailure::UnsafeIntent)?;
        let core_actor = actor
            .owner_actor_assurance()
            .ok_or(DurableFoundationBuildFailure::AuthorityRequestMismatch)?;
        let owner_envelope = serde_json::to_string(&json!({
            "attach_to_delegation_id": request.attach_to_delegation_id,
            "idempotency_key": request.idempotency_key,
            "reply_to_message_id": actor.verified_reply_to_message_id(),
            "request_id": request.request_id,
            "source_correlation_id": request.source_correlation_id,
            "target_delegation_id": target.delegation_id,
            "target_delegation_revision": target.state_revision,
            "target_plan_revision": target.plan_revision,
        }))
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let source = FollowUpAmendmentAuthoritySource::builder()
            .target(
                target.delegation_id.clone(),
                target.state_revision,
                target.plan_revision,
            )
            .normalized_intent(redacted_amendment.as_str())
            .owner_instruction(
                format!("follow-up:{}", request.request_id),
                request.text.as_bytes().to_vec(),
            )
            .canonical_owner_envelope("execass.owner_amendment_envelope.v1", owner_envelope)
            .policy_revision(current.delegation.policy_revision)
            .created_at_ms(admitted_at_ms)
            .build()
            .map_err(|_| DurableFoundationBuildFailure::AuthorityBindingFailed)?;
        let authority = bind_follow_up_amendment_owner_authority(core_actor, source)
            .map_err(|_| DurableFoundationBuildFailure::AuthorityBindingFailed)?;

        let identity =
            FollowUpIngressIdentity::new(&target.delegation_id, request, original_authority);
        let amendment_id = deterministic_follow_up_id("amendment", &identity);
        let transition_id = deterministic_follow_up_id("transition", &identity);
        let plan_id = deterministic_follow_up_id("plan", &identity);
        let criterion_id = deterministic_follow_up_id("criterion", &identity);
        let event_id = deterministic_follow_up_id("outbox", &identity);
        let write_id = deterministic_follow_up_id("write", &identity);
        let logical_action_id = deterministic_follow_up_id("planning_leaf", &identity);
        let node_id = deterministic_follow_up_id("dispatch_node", &identity);

        let dispatch = DispatchTree {
            root_id: node_id.clone(),
            nodes: vec![DispatchNode {
                node_id,
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id,
                    action_kind:
                        crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_ACTION_KIND
                            .to_string(),
                    tool: ToolIdentityInput {
                        tool_id: crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_TOOL_ID
                            .to_string(),
                        version:
                            crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_TOOL_VERSION
                                .to_string(),
                    },
                    operands: CanonicalValue::Object(vec![
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "amendment".to_string(),
                            value: CanonicalValue::String(redacted_amendment.as_str().to_string()),
                        },
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "request_id".to_string(),
                            value: CanonicalValue::String(request.request_id.clone()),
                        },
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "resolved_target".to_string(),
                            value: CanonicalValue::String(target.delegation_id.clone()),
                        },
                        carsinos_core::execass_manifest::CanonicalField {
                            key: "superseded_plan_revision".to_string(),
                            value: CanonicalValue::Integer(target.plan_revision),
                        },
                    ]),
                    target_snapshot: TargetSnapshotInput {
                        targets: vec![CanonicalValue::String(target.delegation_id.clone())],
                    },
                    material_digest: None,
                    owner_authority: authority.clone(),
                })),
            }],
        };
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&dispatch, &ServerResolutionRegistry::default())
        else {
            return Err(DurableFoundationBuildFailure::ManifestCompilationFailed);
        };
        if manifest.leaves().len() != 1 {
            return Err(DurableFoundationBuildFailure::ManifestCompilationFailed);
        }

        let manifest_json = String::from_utf8(manifest.canonical().bytes().to_vec())
            .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let amendment_digest = owner_normalized_intent_digest(redacted_amendment.as_str())
            .ok_or(DurableFoundationBuildFailure::UnsafeIntent)?;
        let intake_evidence_json = serde_json::to_string(&json!({
            "authority_provenance_id": authority.authority_provenance_id(),
            "explicit_attachment": request.attach_to_delegation_id,
            "reply_to_message_id": actor.verified_reply_to_message_id(),
            "request_id": request.request_id,
            "source_correlation_id": request.source_correlation_id,
        }))
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let outbox_payload = serde_json::to_string(&json!({
            "amendment_digest": amendment_digest,
            "attachment_evidence_digest": attachment_evidence_digest(request, actor),
            "delegation_id": target.delegation_id,
            "idempotency_key": request.idempotency_key,
            "instruction_digest": authority.instruction_digest(),
            "request_id": request.request_id,
            "source_correlation_id": request.source_correlation_id,
            "summary": "verified owner amendment admitted for replacement planning",
        }))
        .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?;
        let plan = PlanRecord {
            plan_id,
            delegation_id: target.delegation_id.clone(),
            plan_revision: next_plan_revision,
            based_on_delegation_revision: next_state_revision,
            policy_revision: current.delegation.policy_revision,
            plan_summary: "produce the canonical replacement plan for the redacted owner amendment"
                .to_string(),
            resolved_leaf_manifest_json: manifest_json,
            manifest_digest: manifest.canonical().digest().as_hex().to_string(),
            created_by_authority_provenance_id: authority.authority_provenance_id().to_string(),
            created_at: admitted_at_ms,
        };
        let outbox_event = NewOutboxEvent {
            event_id,
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: target.delegation_id.clone(),
            aggregate_revision: next_state_revision,
            correlation_id: request.source_correlation_id.clone(),
            causation_id: request.request_id.clone(),
            occurred_at: admitted_at_ms,
            safe_payload_json: outbox_payload,
            duplicate_identity: write_id.clone(),
        };
        Ok(PreparedFollowUpAmendment {
            amendment: AmendLifecycleCommand {
                write: WriteContext {
                    idempotency_key: write_id,
                    correlation_id: request.source_correlation_id.clone(),
                    causation_id: request.request_id.clone(),
                    occurred_at: admitted_at_ms,
                },
                delegation_id: target.delegation_id.clone(),
                expected_state_revision: target.state_revision,
                transition_id,
                amendment_id,
                amendment_revision: target.state_revision,
                normalized_amendment: redacted_amendment.as_str().to_string(),
                intake_evidence_json,
                authority_provenance_id: authority.authority_provenance_id().to_string(),
                plan,
                outcome_criteria: vec![OutcomeCriterionRecord {
                    criterion_id,
                    delegation_id: target.delegation_id.clone(),
                    criteria_revision,
                    criterion_key: "canonical_replacement_plan_produced".to_string(),
                    description:
                        "a canonical replacement plan is produced for the verified owner amendment"
                            .to_string(),
                    material: true,
                    verifier_type: VerifierType::DatabasePredicate,
                    expected_predicate_json: serde_json::to_string(
                        &CriterionPredicate::DatabasePredicate {
                            version: PredicateVersion::V1,
                            delegation_id: target.delegation_id.clone(),
                            canonical_plan_revision_greater_than: target.plan_revision,
                        },
                    )
                    .map_err(|_| DurableFoundationBuildFailure::SerializationFailed)?,
                    authoritative_source_kind: "execass_plan_store".to_string(),
                    created_at: admitted_at_ms,
                }],
                outbox_event,
            },
            manifest,
            authority,
        })
    }

    pub(crate) async fn admit_durable_foundation(
        &self,
        state: &crate::AppState,
        classification: &IntakeClassification,
        request: &IntakeRequest,
        authority: &VerifiedOwnerAuthority,
        admitted_at_ms: i64,
    ) -> anyhow::Result<crate::GatewayFoundationAdmissionOutcome> {
        let prepared = self
            .prepare_durable_foundation(classification, request, authority, admitted_at_ms)
            .map_err(|failure| {
                anyhow::anyhow!("durable foundation preparation failed: {failure:?}")
            })?;
        if let Some(existing) = state
            .execass_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("canonical ExecAss store is unavailable"))?
            .read_foundation(&prepared.command.delegation.delegation_id)?
        {
            return Ok(crate::GatewayFoundationAdmissionOutcome::Admitted(
                carsinos_storage::execass::FoundationDispatchAdmissionOutcome::Admitted(Box::new(
                    if exact_foundation_replay_matches(&existing, &prepared.command) {
                        carsinos_storage::execass::FoundationWriteOutcome::Replayed(existing)
                    } else {
                        carsinos_storage::execass::FoundationWriteOutcome::Conflict {
                            existing_delegation_id: Some(
                                prepared.command.delegation.delegation_id.clone(),
                            ),
                        }
                    },
                )),
            ));
        }
        let admission = state
            .admit_execass_foundation_dispatch(
                &prepared.command,
                &prepared.dispatch,
                &prepared.resolutions,
                authority,
                &prepared.authorized_actions,
            )
            .await?;
        if matches!(
            &admission,
            crate::GatewayFoundationAdmissionOutcome::Admitted(
                carsinos_storage::execass::FoundationDispatchAdmissionOutcome::Admitted(outcome)
            ) if matches!(outcome.as_ref(), carsinos_storage::execass::FoundationWriteOutcome::Conflict { .. })
        ) {
            if let Some(existing) = state
                .execass_store
                .as_ref()
                .expect("store checked before admission")
                .read_foundation(&prepared.command.delegation.delegation_id)?
            {
                if exact_foundation_replay_matches(&existing, &prepared.command) {
                    return Ok(crate::GatewayFoundationAdmissionOutcome::Admitted(
                        carsinos_storage::execass::FoundationDispatchAdmissionOutcome::Admitted(
                            Box::new(carsinos_storage::execass::FoundationWriteOutcome::Replayed(
                                existing,
                            )),
                        ),
                    ));
                }
            }
        }
        Ok(admission)
    }

    fn resolve_attachment(
        &self,
        state: &crate::AppState,
        actor: &BaseActorAssurance,
        request: &IntakeRequest,
        original_authority: &VerifiedOwnerAuthority,
    ) -> anyhow::Result<AttachmentResolution> {
        let explicit_id = request.attach_to_delegation_id.as_deref();
        let reply_id = actor.verified_reply_to_message_id();
        if explicit_id.is_none() && reply_id.is_none() {
            return Ok(AttachmentResolution::Foundation);
        }
        let store = state
            .execass_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("canonical ExecAss store is unavailable"))?;
        let mut explicit_target = None;
        let mut explicit_replay = None;
        if let Some(delegation_id) = explicit_id {
            let existing = store.read_foundation(delegation_id)?;
            if let Some(existing) = existing.as_ref() {
                match exact_amendment_replay_matches(existing, actor, request, original_authority) {
                    AmendmentReplayMatch::Exact => {
                        explicit_replay = Some(delegation_id.to_string());
                    }
                    AmendmentReplayMatch::MaterialMismatch => {
                        return Ok(AttachmentResolution::Wrong(
                            WrongAttachmentReason::ReplayMaterialMismatch {
                                delegation_id: delegation_id.to_string(),
                            },
                        ));
                    }
                    AmendmentReplayMatch::Absent => {}
                }
            }
            explicit_target = store.resolve_explicit_follow_up_target(delegation_id)?;
            if explicit_target.is_none() && explicit_replay.is_none() {
                return Ok(AttachmentResolution::Wrong(match existing {
                    None => WrongAttachmentReason::MissingExplicitTarget {
                        delegation_id: delegation_id.to_string(),
                    },
                    Some(bundle)
                        if matches!(
                            bundle.delegation.phase,
                            DelegationPhase::Completed
                                | DelegationPhase::PartiallyCompleted
                                | DelegationPhase::Failed
                        ) =>
                    {
                        WrongAttachmentReason::TerminalExplicitTarget {
                            delegation_id: delegation_id.to_string(),
                        }
                    }
                    Some(_) => WrongAttachmentReason::IneligibleExplicitTarget {
                        delegation_id: delegation_id.to_string(),
                    },
                }));
            }
        }

        let (reply_target, reply_replay) = if let Some(reply_to_message_id) = reply_id {
            let Some(provider) = actor.verified_provider().and_then(channel_provider) else {
                return Ok(AttachmentResolution::Wrong(
                    WrongAttachmentReason::InvalidReplyProvider,
                ));
            };
            let Some(conversation_id) = actor.verified_conversation_id() else {
                return Ok(AttachmentResolution::Wrong(
                    WrongAttachmentReason::MissingReplyTarget,
                ));
            };
            let (_, credential_identity, authenticated_ingress, _, _, _) =
                authority_storage_evidence(original_authority);
            let binding = store.read_channel_reply_binding(
                provider,
                &authenticated_ingress,
                &credential_identity,
                conversation_id,
                reply_to_message_id,
            )?;
            let Some(binding) = binding else {
                return Ok(AttachmentResolution::Wrong(
                    WrongAttachmentReason::MissingReplyTarget,
                ));
            };
            let current = store.read_foundation(&binding.delegation_id)?;
            let replayed_id = match current.as_ref().map(|current| {
                exact_amendment_replay_matches(current, actor, request, original_authority)
            }) {
                Some(AmendmentReplayMatch::Exact) => Some(binding.delegation_id.clone()),
                Some(AmendmentReplayMatch::MaterialMismatch) => {
                    return Ok(AttachmentResolution::Wrong(
                        WrongAttachmentReason::ReplayMaterialMismatch {
                            delegation_id: binding.delegation_id,
                        },
                    ));
                }
                Some(AmendmentReplayMatch::Absent) | None => None,
            };
            let target = store.resolve_channel_reply_target(
                provider,
                &authenticated_ingress,
                &credential_identity,
                conversation_id,
                reply_to_message_id,
            )?;
            if target.is_none() && replayed_id.is_none() {
                return Ok(AttachmentResolution::Wrong(match current {
                    Some(bundle)
                        if matches!(
                            bundle.delegation.phase,
                            DelegationPhase::Completed
                                | DelegationPhase::PartiallyCompleted
                                | DelegationPhase::Failed
                        ) =>
                    {
                        WrongAttachmentReason::TerminalReplyTarget {
                            delegation_id: binding.delegation_id,
                        }
                    }
                    _ => WrongAttachmentReason::MissingReplyTarget,
                }));
            }
            (target, replayed_id)
        } else {
            (None, None)
        };

        let dual_replay =
            match reconcile_dual_replay_ids(explicit_replay.as_deref(), reply_replay.as_deref()) {
                Ok(replayed) => replayed,
                Err(reason) => return Ok(AttachmentResolution::Wrong(reason)),
            };
        if let Some(replayed_id) = dual_replay {
            return Ok(AttachmentResolution::Replayed {
                delegation_id: replayed_id,
            });
        }

        if let Some(replayed_id) = explicit_replay {
            if let Some(reply) = reply_target.as_ref() {
                if reply.delegation_id != replayed_id {
                    return Ok(AttachmentResolution::Wrong(
                        WrongAttachmentReason::SignalsDisagree {
                            explicit_delegation_id: replayed_id,
                            reply_delegation_id: reply.delegation_id.clone(),
                        },
                    ));
                }
            }
            return Ok(AttachmentResolution::Replayed {
                delegation_id: replayed_id,
            });
        }

        if let Some(replayed_id) = reply_replay {
            if let Some(explicit) = explicit_target.as_ref() {
                if explicit.delegation_id != replayed_id {
                    return Ok(AttachmentResolution::Wrong(
                        WrongAttachmentReason::SignalsDisagree {
                            explicit_delegation_id: explicit.delegation_id.clone(),
                            reply_delegation_id: replayed_id,
                        },
                    ));
                }
            }
            return Ok(AttachmentResolution::Replayed {
                delegation_id: replayed_id,
            });
        }

        match reconcile_attachment_targets(explicit_target, reply_target) {
            Ok(Some(target)) => {
                let current = store
                    .read_foundation(&target.delegation_id)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("resolved follow-up target disappeared before admission")
                    })?;
                Ok(
                    match exact_amendment_replay_matches(
                        &current,
                        actor,
                        request,
                        original_authority,
                    ) {
                        AmendmentReplayMatch::Exact => AttachmentResolution::Replayed {
                            delegation_id: target.delegation_id,
                        },
                        AmendmentReplayMatch::MaterialMismatch => AttachmentResolution::Wrong(
                            WrongAttachmentReason::ReplayMaterialMismatch {
                                delegation_id: target.delegation_id,
                            },
                        ),
                        AmendmentReplayMatch::Absent => AttachmentResolution::Target(target),
                    },
                )
            }
            Ok(None) => Ok(AttachmentResolution::Foundation),
            Err(reason) => Ok(AttachmentResolution::Wrong(reason)),
        }
    }

    /// The sole channel/native adapter seam for an already verified owner.
    /// Channel code supplies only server-derived execution-shape facts; this
    /// service owns classification, exact authority binding, and durable
    /// foundation admission. Generic bearer ingress cannot construct the
    /// opaque `BaseActorAssurance` required here.
    pub(crate) async fn route_verified_owner_intake(
        &self,
        state: &crate::AppState,
        actor: &BaseActorAssurance,
        request: &IntakeRequest,
        assessment: &ExecutionShapeAssessment,
        policy_revision: i64,
        admitted_at_ms: i64,
    ) -> anyhow::Result<VerifiedOwnerIntakeOutcome> {
        let classification = self.classify(assessment);
        let original_authority = self
            .bind_original_request_authority(actor, request, policy_revision, admitted_at_ms)
            .map_err(|failure| anyhow::anyhow!("owner intake authority failed: {failure:?}"))?;

        match self.resolve_attachment(state, actor, request, &original_authority)? {
            AttachmentResolution::Wrong(reason) => {
                return Ok(VerifiedOwnerIntakeOutcome::WrongAttachment {
                    classification,
                    reason,
                });
            }
            AttachmentResolution::Replayed { delegation_id } => {
                return Ok(VerifiedOwnerIntakeOutcome::Amendment {
                    classification,
                    outcome: FollowUpAmendmentWriteOutcome::Replayed { delegation_id },
                });
            }
            AttachmentResolution::Target(target) => {
                let store = state
                    .execass_store
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("canonical ExecAss store is unavailable"))?;
                let Some(current) = store.read_foundation(&target.delegation_id)? else {
                    return Ok(VerifiedOwnerIntakeOutcome::WrongAttachment {
                        classification,
                        reason: WrongAttachmentReason::MissingExplicitTarget {
                            delegation_id: target.delegation_id,
                        },
                    });
                };
                if current.delegation.state_revision != target.state_revision
                    || current.delegation.current_plan_revision != Some(target.plan_revision)
                {
                    return Ok(VerifiedOwnerIntakeOutcome::WrongAttachment {
                        classification,
                        reason: WrongAttachmentReason::SignalsChanged,
                    });
                }
                let prepared = self
                    .prepare_follow_up_amendment(
                        actor,
                        request,
                        &original_authority,
                        &target,
                        &current,
                        admitted_at_ms,
                    )
                    .map_err(|failure| {
                        anyhow::anyhow!("follow-up amendment preparation failed: {failure:?}")
                    })?;
                let model_danger_conclusions = state
                    .execass_owned_danger_model_adapter
                    .observe_manifest(
                        &state.providers,
                        &state.storage,
                        &state.secret_store,
                        &prepared.manifest,
                    )
                    .await
                    .map_err(|reason| {
                        anyhow::anyhow!(
                            "follow-up amendment danger model was mechanically unresolved: {reason:?}"
                        )
                    })?;
                let danger_admission = match state
                    .execass_danger_bridge
                    .admit_manifest_with_model_conclusions(
                        &prepared.manifest,
                        &model_danger_conclusions,
                    ) {
                    crate::execass_danger_bridge::DangerBridgeAdmissionOutcome::Admitted(proof) => {
                        proof
                    }
                    crate::execass_danger_bridge::DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                        logical_action_id,
                        reason,
                    } => {
                        anyhow::bail!(
                            "follow-up amendment danger routing unresolved for {logical_action_id}: {reason:?}"
                        )
                    }
                };
                let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("fixed ExecAss confirmation authority is unavailable")
                })?;
                let danger_admission = runtime.seal_danger_admission(danger_admission)?;
                let outcome = runtime.apply_verified_follow_up_amendment(
                    prepared.amendment,
                    &prepared.authority,
                    &prepared.manifest,
                    &danger_admission,
                    admitted_at_ms,
                )?;
                if let Some(reason) =
                    storage_attachment_failure_reason(&outcome, &target.delegation_id)
                {
                    return Ok(VerifiedOwnerIntakeOutcome::WrongAttachment {
                        classification,
                        reason,
                    });
                }
                return Ok(match outcome {
                    VerifiedFollowUpAmendmentOutcome::Applied(snapshot) => {
                        debug_assert!(snapshot.continuation.is_none());
                        VerifiedOwnerIntakeOutcome::Amendment {
                            classification,
                            outcome: FollowUpAmendmentWriteOutcome::Applied {
                                delegation_id: snapshot.delegation.delegation_id,
                            },
                        }
                    }
                    VerifiedFollowUpAmendmentOutcome::Replayed(snapshot) => {
                        debug_assert!(snapshot.continuation.is_none());
                        VerifiedOwnerIntakeOutcome::Amendment {
                            classification,
                            outcome: FollowUpAmendmentWriteOutcome::Replayed {
                                delegation_id: snapshot.delegation.delegation_id,
                            },
                        }
                    }
                    VerifiedFollowUpAmendmentOutcome::Stale { .. }
                    | VerifiedFollowUpAmendmentOutcome::NotFound => {
                        unreachable!("storage failures were mapped before success projection")
                    }
                });
            }
            AttachmentResolution::Foundation => {}
        }
        match classification.disposition() {
            IntakeDisposition::Conversational => {
                Ok(VerifiedOwnerIntakeOutcome::Conversational(classification))
            }
            IntakeDisposition::SynchronousReadOnly => Ok(
                VerifiedOwnerIntakeOutcome::SynchronousReadOnly(classification),
            ),
            IntakeDisposition::Durable => {
                let admission = self
                    .admit_durable_foundation(
                        state,
                        &classification,
                        request,
                        &original_authority,
                        admitted_at_ms,
                    )
                    .await?;
                Ok(VerifiedOwnerIntakeOutcome::Durable {
                    classification,
                    admission,
                })
            }
        }
    }
}

fn channel_provider(provider: &str) -> Option<ExecAssChannelProvider> {
    match provider {
        "telegram" => Some(ExecAssChannelProvider::Telegram),
        "discord" => Some(ExecAssChannelProvider::Discord),
        _ => None,
    }
}

fn storage_attachment_failure_reason(
    outcome: &VerifiedFollowUpAmendmentOutcome,
    delegation_id: &str,
) -> Option<WrongAttachmentReason> {
    match outcome {
        VerifiedFollowUpAmendmentOutcome::Stale {
            current_state_revision,
        } => Some(WrongAttachmentReason::StaleTarget {
            current_state_revision: *current_state_revision,
        }),
        VerifiedFollowUpAmendmentOutcome::NotFound => {
            Some(WrongAttachmentReason::MissingExplicitTarget {
                delegation_id: delegation_id.to_string(),
            })
        }
        VerifiedFollowUpAmendmentOutcome::Applied(_)
        | VerifiedFollowUpAmendmentOutcome::Replayed(_) => None,
    }
}

fn reconcile_attachment_targets(
    explicit: Option<EligibleFollowUpTarget>,
    reply: Option<EligibleFollowUpTarget>,
) -> Result<Option<EligibleFollowUpTarget>, WrongAttachmentReason> {
    match (explicit, reply) {
        (None, None) => Ok(None),
        (Some(target), None) | (None, Some(target)) => Ok(Some(target)),
        (Some(explicit), Some(reply)) if explicit.delegation_id != reply.delegation_id => {
            Err(WrongAttachmentReason::SignalsDisagree {
                explicit_delegation_id: explicit.delegation_id,
                reply_delegation_id: reply.delegation_id,
            })
        }
        (Some(explicit), Some(reply)) if explicit != reply => {
            Err(WrongAttachmentReason::SignalsChanged)
        }
        (Some(target), Some(_)) => Ok(Some(target)),
    }
}

fn reconcile_dual_replay_ids(
    explicit: Option<&str>,
    reply: Option<&str>,
) -> Result<Option<String>, WrongAttachmentReason> {
    match (explicit, reply) {
        (Some(explicit), Some(reply)) if explicit != reply => {
            Err(WrongAttachmentReason::SignalsDisagree {
                explicit_delegation_id: explicit.to_string(),
                reply_delegation_id: reply.to_string(),
            })
        }
        (Some(explicit), Some(_)) => Ok(Some(explicit.to_string())),
        _ => Ok(None),
    }
}

struct FollowUpIngressIdentity {
    target_delegation_id: String,
    authenticated_ingress: String,
    credential_identity: String,
    source_correlation_id: String,
    request_id: String,
    idempotency_key: String,
}

impl FollowUpIngressIdentity {
    fn new(
        target_delegation_id: &str,
        request: &IntakeRequest,
        authority: &VerifiedOwnerAuthority,
    ) -> Self {
        let (_, credential_identity, authenticated_ingress, _, source_correlation_id, _) =
            authority_storage_evidence(authority);
        Self {
            target_delegation_id: target_delegation_id.to_string(),
            authenticated_ingress,
            credential_identity,
            source_correlation_id,
            request_id: request.request_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
        }
    }
}

fn deterministic_follow_up_id(kind: &str, identity: &FollowUpIngressIdentity) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"carsinos.execass.owner_follow_up_amendment.v1\0");
    hasher.update(kind.as_bytes());
    for value in [
        identity.target_delegation_id.as_str(),
        identity.authenticated_ingress.as_str(),
        identity.credential_identity.as_str(),
        identity.source_correlation_id.as_str(),
        identity.request_id.as_str(),
        identity.idempotency_key.as_str(),
    ] {
        hasher.update(b"\0");
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("ea303-{kind}-{:x}", hasher.finalize())
}

fn exact_amendment_replay_matches(
    current: &carsinos_storage::execass::FoundationBundle,
    actor: &BaseActorAssurance,
    request: &IntakeRequest,
    authority: &VerifiedOwnerAuthority,
) -> AmendmentReplayMatch {
    let identity =
        FollowUpIngressIdentity::new(&current.delegation.delegation_id, request, authority);
    let event_id = deterministic_follow_up_id("outbox", &identity);
    let Some(event) = current
        .outbox_events
        .iter()
        .find(|record| record.event.event_id == event_id)
        .map(|record| &record.event)
    else {
        return AmendmentReplayMatch::Absent;
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&event.safe_payload_json) else {
        return AmendmentReplayMatch::MaterialMismatch;
    };
    let expected_digest = SafeText::new(request.text.trim(), &[])
        .ok()
        .and_then(|safe| owner_normalized_intent_digest(safe.as_str()));
    if expected_digest.as_deref() == payload["amendment_digest"].as_str()
        && payload["attachment_evidence_digest"].as_str()
            == Some(attachment_evidence_digest(request, actor).as_str())
        && payload["delegation_id"].as_str() == Some(current.delegation.delegation_id.as_str())
        && payload["idempotency_key"].as_str() == Some(request.idempotency_key.as_str())
        && payload["instruction_digest"].as_str() == Some(authority.instruction_digest())
        && payload["request_id"].as_str() == Some(request.request_id.as_str())
        && payload["source_correlation_id"].as_str() == Some(request.source_correlation_id.as_str())
    {
        AmendmentReplayMatch::Exact
    } else {
        AmendmentReplayMatch::MaterialMismatch
    }
}

fn attachment_evidence_digest(request: &IntakeRequest, actor: &BaseActorAssurance) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"carsinos.execass.follow_up_attachment_evidence.v1");
    for value in [
        request.attach_to_delegation_id.as_deref(),
        actor.verified_provider(),
        actor.verified_conversation_id(),
        actor.verified_reply_to_message_id(),
    ] {
        match value {
            Some(value) => {
                hasher.update([1]);
                hasher.update((value.len() as u64).to_be_bytes());
                hasher.update(value.as_bytes());
            }
            None => hasher.update([0]),
        }
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn exact_foundation_replay_matches(
    existing: &carsinos_storage::execass::FoundationBundle,
    requested: &CreateFoundationCommand,
) -> bool {
    fn json_string(raw: &str, field: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(raw)
            .ok()?
            .get(field)?
            .as_str()
            .map(ToOwned::to_owned)
    }

    let current = &existing.delegation;
    let candidate = &requested.delegation;
    current.delegation_id == candidate.delegation_id
        && current.normalized_original_intent == candidate.normalized_original_intent
        && current.ingress_source == candidate.ingress_source
        && current.ingress_credential_identity == candidate.ingress_credential_identity
        && current.source_message_id == candidate.source_message_id
        && current.source_correlation_id == candidate.source_correlation_id
        && current.ingress_idempotency_key == candidate.ingress_idempotency_key
        && current.classifier_version == candidate.classifier_version
        && current.classifier_reasons_json == candidate.classifier_reasons_json
        && json_string(&current.intake_evidence_json, "request_id")
            == json_string(&candidate.intake_evidence_json, "request_id")
        && json_string(&current.effective_authority_json, "instruction_digest")
            == json_string(&candidate.effective_authority_json, "instruction_digest")
}

struct FoundationIngressIdentity<'a> {
    authenticated_ingress: &'a str,
    credential_identity: &'a str,
    source_correlation_id: &'a str,
    request_id: &'a str,
    idempotency_key: &'a str,
}

fn deterministic_foundation_id(kind: &str, identity: &FoundationIngressIdentity<'_>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"carsinos.execass.owner_request_foundation.v1\0");
    hasher.update(kind.as_bytes());
    for value in [
        identity.authenticated_ingress,
        identity.credential_identity,
        identity.source_correlation_id,
        identity.request_id,
        identity.idempotency_key,
    ] {
        hasher.update(b"\0");
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("ea301-{kind}-{:x}", hasher.finalize())
}

pub(crate) fn authority_storage_evidence(
    authority: &VerifiedOwnerAuthority,
) -> (ActorType, String, String, String, String, Option<String>) {
    match authority.evidence() {
        VerifiedHumanEvidenceRef::Local {
            authenticated_client_id,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id,
            source_message_id,
        } => (
            ActorType::HumanLocal,
            authenticated_client_id.to_string(),
            authenticated_ingress.to_string(),
            channel_assurance.to_string(),
            request_correlation_id.to_string(),
            source_message_id.map(str::to_string),
        ),
        VerifiedHumanEvidenceRef::Remote {
            adapter_id,
            provider_account_id,
            authenticated_ingress,
            channel_assurance,
            source_message_id,
            request_correlation_id,
        } => (
            ActorType::HumanRemote,
            format!("{adapter_id}:{provider_account_id}"),
            authenticated_ingress.to_string(),
            channel_assurance.to_string(),
            request_correlation_id.to_string(),
            Some(source_message_id.to_string()),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntakeAuthorityFailure {
    NonHumanActor,
    MissingOwnerCapability,
    CorrelationMismatch,
    SourceMessageMismatch,
    RequestBindingMismatch,
    IdempotencyBindingMismatch,
    IntentBindingMismatch,
    InstructionBindingMismatch,
    InvalidRequest,
    UnsafeIntent,
    AuthorityBindingFailed,
}

fn bind_original_request_core(
    actor: &carsinos_core::execass_actor::ActorAssurance,
    request: &IntakeRequest,
    policy_revision: i64,
    created_at_ms: i64,
) -> Result<VerifiedOwnerAuthority, IntakeAuthorityFailure> {
    if request.request_id.is_empty()
        || request.request_id.trim() != request.request_id
        || request.idempotency_key.is_empty()
        || request.idempotency_key.trim() != request.idempotency_key
        || request.source_correlation_id.is_empty()
        || request.source_correlation_id.trim() != request.source_correlation_id
        || request.text.trim().is_empty()
        || policy_revision < 0
        || created_at_ms < 0
    {
        return Err(IntakeAuthorityFailure::InvalidRequest);
    }
    let normalized_intent = SafeText::new(request.text.trim(), &[])
        .map_err(|_| IntakeAuthorityFailure::UnsafeIntent)?;
    let owner_envelope = serde_json::to_string(&json!({
        "attach_to_delegation_id": request.attach_to_delegation_id,
        "idempotency_key": request.idempotency_key,
        "request_id": request.request_id,
        "source_correlation_id": request.source_correlation_id,
    }))
    .map_err(|_| IntakeAuthorityFailure::InvalidRequest)?;
    let scope = serde_json::to_string(&json!({
        "kind": "single_owner_original_request",
        "request_id": request.request_id,
    }))
    .map_err(|_| IntakeAuthorityFailure::InvalidRequest)?;
    let source = OriginalRequestAuthoritySource::builder()
        .normalized_intent(normalized_intent.as_str())
        .owner_instruction(
            format!("intake:{}", request.request_id),
            request.text.as_bytes().to_vec(),
        )
        .canonical_owner_envelope("execass.owner_envelope.v1", owner_envelope)
        .canonical_scope_json(scope)
        .policy_revision(policy_revision)
        .created_at_ms(created_at_ms)
        .build()
        .map_err(|_| IntakeAuthorityFailure::AuthorityBindingFailed)?;
    bind_original_request_owner_authority(actor, source)
        .map_err(|_| IntakeAuthorityFailure::AuthorityBindingFailed)
}

fn push_if(reasons: &mut Vec<IntakeReason>, condition: bool, reason: IntakeReason) {
    if condition && !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

const ALL_DECISION_KINDS: [DecisionKind; 7] = [
    DecisionKind::Clarification,
    DecisionKind::DangerousActionConfirmation,
    DecisionKind::OwnerConfiguredCheckpoint,
    DecisionKind::RecoveryChoice,
    DecisionKind::DuplicateRiskRetry,
    DecisionKind::Stop,
    DecisionKind::PolicyChange,
];

/// Opaque proof that canonical side-effect dispatch passed intake admission.
#[cfg(test)]
pub(crate) struct DurableDispatchAdmission(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct SideEffectDispatchRejection {
    pub(crate) disposition: IntakeDisposition,
}

#[cfg(test)]
pub(crate) struct CanonicalSideEffectDispatchGuard;

#[cfg(test)]
impl CanonicalSideEffectDispatchGuard {
    pub(crate) fn dispatch<T>(
        classification: &IntakeClassification,
        dispatch: impl FnOnce(DurableDispatchAdmission) -> T,
    ) -> Result<T, SideEffectDispatchRejection> {
        if classification.disposition != IntakeDisposition::Durable {
            return Err(SideEffectDispatchRejection {
                disposition: classification.disposition,
            });
        }

        Ok(dispatch(DurableDispatchAdmission(())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn local_owner_actor() -> carsinos_core::execass_actor::ActorAssurance {
        carsinos_core::execass_actor::derive_local_owner_actor_assurance(
            carsinos_core::execass_actor::AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
                "native-window-1",
                "native-control",
                "interactive-local",
                "corr-1",
            )
            .unwrap(),
        )
    }

    fn request(text: &str) -> IntakeRequest {
        IntakeRequest {
            request_id: "request-1".into(),
            idempotency_key: "idem-1".into(),
            text: text.into(),
            source_correlation_id: "corr-1".into(),
            attach_to_delegation_id: None,
        }
    }

    fn classify(assessment: ExecutionShapeAssessment) -> IntakeClassification {
        ExecAssIntakeService.classify(&assessment)
    }

    #[test]
    fn only_nonempty_immediate_zero_execution_shape_is_conversational() {
        let classification = classify(ExecutionShapeAssessment::new(
            ImmediateResponseShape::NonEmpty,
        ));

        assert_eq!(
            classification.disposition(),
            IntakeDisposition::Conversational
        );
        assert_eq!(
            classification.reasons(),
            &[IntakeReason::ImmediateResponseOnly]
        );

        for immediate in [
            ImmediateResponseShape::Absent,
            ImmediateResponseShape::Empty,
        ] {
            assert_eq!(
                classify(ExecutionShapeAssessment::new(immediate)).disposition(),
                IntakeDisposition::Durable
            );
        }
    }

    #[test]
    fn synchronous_read_only_requires_explicit_proof_and_authenticated_audit() {
        let classification = classify(
            ExecutionShapeAssessment::new(ImmediateResponseShape::Absent)
                .with_synchronous_read_only_proof()
                .with_authenticated_audit(),
        );

        assert_eq!(
            classification.disposition(),
            IntakeDisposition::SynchronousReadOnly
        );
        assert_eq!(
            classification.reasons(),
            &[
                IntakeReason::SynchronousReadOnlyProven,
                IntakeReason::AuthenticatedAudit
            ]
        );

        let missing_audit = classify(
            ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                .with_synchronous_read_only_proof(),
        );
        assert_eq!(missing_audit.disposition(), IntakeDisposition::Durable);
        assert_eq!(
            missing_audit.reasons(),
            &[IntakeReason::MissingAuthenticatedAudit]
        );
    }

    #[test]
    fn every_single_structural_trigger_is_durable() {
        let cases = [
            (DurableTrigger::ToolExecution, IntakeReason::ToolExecution),
            (DurableTrigger::SideEffect, IntakeReason::SideEffect),
            (DurableTrigger::Delay, IntakeReason::Delay),
            (DurableTrigger::Schedule, IntakeReason::Schedule),
            (DurableTrigger::Worker, IntakeReason::Worker),
            (
                DurableTrigger::DurableMutation,
                IntakeReason::DurableMutation,
            ),
            (DurableTrigger::DurableReceipt, IntakeReason::DurableReceipt),
        ];

        for (trigger, reason) in cases {
            let classification = classify(
                ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                    .with_durable_trigger(trigger),
            );
            assert_eq!(classification.disposition(), IntakeDisposition::Durable);
            assert_eq!(classification.reasons(), &[reason]);
        }
    }

    #[test]
    fn all_typed_human_decisions_are_durable_without_generic_approval() {
        for kind in ALL_DECISION_KINDS {
            let classification = classify(
                ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                    .with_durable_trigger(DurableTrigger::HumanDecision(kind)),
            );
            assert_eq!(classification.disposition(), IntakeDisposition::Durable);
            assert_eq!(
                classification.reasons(),
                &[IntakeReason::HumanDecision(kind)]
            );
        }
    }

    #[test]
    fn ambiguity_and_inconsistency_are_independently_durable() {
        for (assessment, reason) in [
            (
                ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty).with_ambiguity(),
                IntakeReason::AmbiguousExecutionShape,
            ),
            (
                ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                    .with_inconsistency(),
                IntakeReason::InconsistentExecutionShape,
            ),
        ] {
            let classification = classify(assessment);
            assert_eq!(classification.disposition(), IntakeDisposition::Durable);
            assert_eq!(classification.reasons(), &[reason]);
        }
    }

    #[test]
    fn read_only_proof_conflicting_with_any_durable_trigger_is_durable() {
        let classification = classify(
            ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                .with_synchronous_read_only_proof()
                .with_authenticated_audit()
                .with_durable_trigger(DurableTrigger::ToolExecution),
        );

        assert_eq!(classification.disposition(), IntakeDisposition::Durable);
        assert_eq!(
            classification.reasons(),
            &[
                IntakeReason::ReadOnlyDurableConflict,
                IntakeReason::ToolExecution
            ]
        );
    }

    #[test]
    fn combined_reasons_are_canonically_ordered_and_deduplicated() {
        let classification = classify(
            ExecutionShapeAssessment::new(ImmediateResponseShape::Empty)
                .with_inconsistency()
                .with_ambiguity()
                .with_synchronous_read_only_proof()
                .with_durable_trigger(DurableTrigger::DurableReceipt)
                .with_durable_trigger(DurableTrigger::Worker)
                .with_durable_trigger(DurableTrigger::ToolExecution)
                .with_durable_trigger(DurableTrigger::Worker)
                .with_durable_trigger(DurableTrigger::HumanDecision(DecisionKind::Stop))
                .with_durable_trigger(DurableTrigger::HumanDecision(DecisionKind::Clarification)),
        );

        assert_eq!(classification.disposition(), IntakeDisposition::Durable);
        assert_eq!(
            classification.reasons(),
            &[
                IntakeReason::AmbiguousExecutionShape,
                IntakeReason::InconsistentExecutionShape,
                IntakeReason::ReadOnlyDurableConflict,
                IntakeReason::ToolExecution,
                IntakeReason::Worker,
                IntakeReason::HumanDecision(DecisionKind::Clarification),
                IntakeReason::HumanDecision(DecisionKind::Stop),
                IntakeReason::DurableReceipt,
                IntakeReason::MissingAuthenticatedAudit,
            ]
        );
    }

    #[test]
    fn canonical_side_effect_dispatch_rejects_both_non_durable_dispositions() {
        let called = Cell::new(false);
        for classification in [
            classify(ExecutionShapeAssessment::new(
                ImmediateResponseShape::NonEmpty,
            )),
            classify(
                ExecutionShapeAssessment::new(ImmediateResponseShape::Absent)
                    .with_synchronous_read_only_proof()
                    .with_authenticated_audit(),
            ),
        ] {
            let result = CanonicalSideEffectDispatchGuard::dispatch(&classification, |_| {
                called.set(true);
            });
            assert_eq!(
                result,
                Err(SideEffectDispatchRejection {
                    disposition: classification.disposition()
                })
            );
        }
        assert!(!called.get());

        let durable = classify(
            ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                .with_durable_trigger(DurableTrigger::SideEffect),
        );
        assert_eq!(
            CanonicalSideEffectDispatchGuard::dispatch(&durable, |_| "dispatched"),
            Ok("dispatched")
        );
    }

    #[test]
    fn assessment_surface_contains_execution_shape_only() {
        // This explicit construction fails to compile if adapters gain any
        // required content, purpose, category, morality, or financial field.
        let assessment = ExecutionShapeAssessment {
            immediate_response: ImmediateResponseShape::NonEmpty,
            synchronous_read_only_proven: false,
            authenticated_audit: false,
            durable_triggers: Vec::new(),
            ambiguous: false,
            inconsistent: false,
        };

        assert_eq!(
            classify(assessment).disposition(),
            IntakeDisposition::Conversational
        );
    }

    #[test]
    fn original_request_binding_hashes_raw_instruction_and_redacts_persisted_intent() {
        let raw = "send token sk-proj-abcdefghijklmnopqrstuvwxyz123456 to the exact target";
        let authority = bind_original_request_core(&local_owner_actor(), &request(raw), 3, 100)
            .expect("verified local original request");
        assert_eq!(authority.authority_kind(), "original_request");
        assert_eq!(authority.policy_revision(), 3);
        assert_eq!(authority.bound_decision_id(), None);
        assert_ne!(
            authority.normalized_intent_digest(),
            carsinos_core::execass_actor::owner_normalized_intent_digest(raw)
                .unwrap()
                .as_str()
        );
    }

    #[test]
    fn original_request_binding_rejects_empty_input() {
        let empty = request("   ");
        assert_eq!(
            bind_original_request_core(&local_owner_actor(), &empty, 1, 100),
            Err(IntakeAuthorityFailure::InvalidRequest)
        );
    }

    fn prepared_foundation(
        request: &IntakeRequest,
        admitted_at_ms: i64,
    ) -> PreparedDurableFoundation {
        let authority = bind_original_request_core(&local_owner_actor(), request, 3, 100)
            .expect("verified original request");
        let classification = classify(
            ExecutionShapeAssessment::new(ImmediateResponseShape::NonEmpty)
                .with_durable_trigger(DurableTrigger::Worker),
        );
        ExecAssIntakeService
            .prepare_durable_foundation(&classification, request, &authority, admitted_at_ms)
            .expect("durable foundation")
    }

    #[test]
    fn durable_foundation_is_exactly_one_accepted_planning_leaf_without_continuation() {
        let request = request("prepare the bounded owner result");
        let prepared = prepared_foundation(&request, 200);
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&prepared.dispatch, &prepared.resolutions)
        else {
            panic!("fixed planning leaf must compile")
        };

        assert_eq!(manifest.leaves().len(), 1);
        assert_eq!(
            manifest.leaves()[0].action_kind(),
            crate::execass_danger_bridge::OWNER_REQUEST_ORCHESTRATOR_ACTION_KIND
        );
        assert_eq!(prepared.command.delegation.phase, DelegationPhase::Accepted);
        assert_eq!(
            prepared.command.delegation.classifier_reasons_json,
            r#"["worker"]"#
        );
        assert_eq!(prepared.command.outcome_criteria.len(), 1);
        assert_eq!(
            prepared.command.outcome_criteria[0].criterion_key,
            "canonical_plan_produced"
        );
        assert_eq!(
            serde_json::from_str::<CriterionPredicate>(
                &prepared.command.outcome_criteria[0].expected_predicate_json
            )
            .unwrap(),
            CriterionPredicate::DatabasePredicate {
                version: PredicateVersion::V1,
                delegation_id: prepared.command.delegation.delegation_id.clone(),
                canonical_plan_revision_greater_than: 0,
            }
        );
        assert!(prepared.command.initial_continuation.is_none());
        assert_eq!(prepared.authorized_actions.len(), 1);
        assert!(prepared.authorized_actions[0].matches(
            &bind_original_request_core(&local_owner_actor(), &request, 3, 100).unwrap(),
            &manifest.leaves()[0]
        ));

        let bridge = crate::execass_danger_bridge::DangerActionBridge::from_server_paths(
            &carsinos_storage::AppPaths::from_root("Z:\\carsinos\\runtime\\ea301-test"),
        );
        let crate::execass_danger_bridge::DangerBridgeAdmissionOutcome::Admitted(proof) = bridge
            .admit_manifest_with_model_conclusions(
                &manifest,
                &[crate::execass_danger_bridge::ServerModelDangerConclusion::NoAdditionalMaterialDanger],
            )
        else {
            panic!("prepared planning leaf must resolve through production danger coverage")
        };
        assert_eq!(
            proof.routes()[0].view(),
            carsinos_core::execass_danger::DangerRouteView::Ordinary
        );
    }

    #[test]
    fn replacement_plan_predicate_is_typed_and_strictly_bound_to_the_prior_revision() {
        let foundation_request = request("prepare the bounded owner result");
        let foundation = prepared_foundation(&foundation_request, 200);
        let current = carsinos_storage::execass::FoundationBundle {
            authority: foundation.command.authority.clone(),
            delegation: foundation.command.delegation.clone(),
            plan: foundation.command.plan.clone(),
            outcome_criteria: foundation.command.outcome_criteria.clone(),
            initial_continuation: None,
            outbox_events: Vec::new(),
        };
        let target = EligibleFollowUpTarget {
            delegation_id: current.delegation.delegation_id.clone(),
            state_revision: current.delegation.state_revision,
            plan_revision: current.delegation.current_plan_revision.unwrap(),
        };
        let amendment_request = IntakeRequest {
            request_id: "amendment-message-1".into(),
            idempotency_key: "amendment-update-1".into(),
            text: "replace the canonical plan".into(),
            source_correlation_id: "amendment-correlation-1".into(),
            attach_to_delegation_id: Some(target.delegation_id.clone()),
        };
        let actor_gate = crate::execass_actor_gate::ExecAssActorGate::new(
            None,
            [("telegram".to_string(), "owner-1".to_string())],
            std::path::PathBuf::from("Z:\\carsinos\\runtime\\ea305-actor-replay"),
        );
        let actor = actor_gate
            .classify_remote_owner_intake(
                &crate::execass_actor_gate::RemoteProviderOwnerEvent::from_telegram_long_poll(
                    "telegram-long-poll".into(),
                    "owner-1".into(),
                    "conversation-1".into(),
                    amendment_request.request_id.clone(),
                    amendment_request.idempotency_key.clone(),
                    amendment_request.source_correlation_id.clone(),
                ),
                &amendment_request.text,
            )
            .unwrap();
        let original_authority = bind_original_request_core(
            &local_owner_actor(),
            &foundation_request,
            current.delegation.policy_revision,
            100,
        )
        .unwrap();
        let prepared = ExecAssIntakeService
            .prepare_follow_up_amendment(
                &actor,
                &amendment_request,
                &original_authority,
                &target,
                &current,
                300,
            )
            .unwrap();

        assert_eq!(
            serde_json::from_str::<CriterionPredicate>(
                &prepared.amendment.outcome_criteria[0].expected_predicate_json
            )
            .unwrap(),
            CriterionPredicate::DatabasePredicate {
                version: PredicateVersion::V1,
                delegation_id: target.delegation_id,
                canonical_plan_revision_greater_than: target.plan_revision,
            }
        );
    }

    #[test]
    fn durable_foundation_replay_ids_are_exact_and_idempotency_scoped() {
        let first_request = request("prepare the bounded owner result");
        let first = prepared_foundation(&first_request, 200);
        let replay = prepared_foundation(&first_request, 200);
        assert_eq!(first.command, replay.command);

        let mut changed_request = first_request;
        changed_request.idempotency_key = "idem-2".to_string();
        let changed = prepared_foundation(&changed_request, 200);
        assert_ne!(
            first.command.delegation.delegation_id,
            changed.command.delegation.delegation_id
        );
        assert_ne!(first.command.plan.plan_id, changed.command.plan.plan_id);
        assert_ne!(
            first.command.outbox_event.event_id,
            changed.command.outbox_event.event_id
        );
        assert_ne!(
            first.command.outcome_criteria[0].criterion_id,
            changed.command.outcome_criteria[0].criterion_id
        );
    }

    #[test]
    fn durable_foundation_serialized_surfaces_never_contain_raw_secret() {
        let secret = "sk-proj-abcdefghijklmnopqrstuvwxyz123456";
        let request = request(&format!("plan delivery with token {secret}"));
        let prepared = prepared_foundation(&request, 200);
        let command_debug = format!("{:?}", prepared.command);
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&prepared.dispatch, &prepared.resolutions)
        else {
            panic!("fixed planning leaf must compile")
        };
        let dispatch_json = std::str::from_utf8(manifest.canonical().bytes()).unwrap();

        assert!(!command_debug.contains(secret));
        assert!(!dispatch_json.contains(secret));
        assert!(prepared
            .command
            .delegation
            .normalized_original_intent
            .contains("[REDACTED]"));
        for persisted_json in [
            prepared.command.delegation.intake_evidence_json.as_str(),
            prepared
                .command
                .delegation
                .effective_authority_json
                .as_str(),
            prepared.command.plan.resolved_leaf_manifest_json.as_str(),
            prepared.command.outcome_criteria[0]
                .expected_predicate_json
                .as_str(),
            prepared.command.outbox_event.safe_payload_json.as_str(),
        ] {
            assert!(!persisted_json.contains(secret));
        }
    }

    #[test]
    fn durable_foundation_rejects_conversational_and_read_only_classifications() {
        let request = request("inspect the bounded result");
        let authority = bind_original_request_core(&local_owner_actor(), &request, 3, 100).unwrap();
        for classification in [
            classify(ExecutionShapeAssessment::new(
                ImmediateResponseShape::NonEmpty,
            )),
            classify(
                ExecutionShapeAssessment::new(ImmediateResponseShape::Absent)
                    .with_synchronous_read_only_proof()
                    .with_authenticated_audit(),
            ),
        ] {
            assert!(matches!(
                ExecAssIntakeService.prepare_durable_foundation(
                    &classification,
                    &request,
                    &authority,
                    200
                ),
                Err(DurableFoundationBuildFailure::NonDurable(
                    disposition
                )) if disposition == classification.disposition()
            ));
        }
    }

    fn attachment_target(
        delegation_id: &str,
        state_revision: i64,
        plan_revision: i64,
    ) -> EligibleFollowUpTarget {
        EligibleFollowUpTarget {
            delegation_id: delegation_id.to_string(),
            state_revision,
            plan_revision,
        }
    }

    #[test]
    fn explicit_and_reply_attachment_signals_must_agree_exactly() {
        let target = attachment_target("delegation-1", 4, 3);
        assert_eq!(
            reconcile_attachment_targets(Some(target.clone()), Some(target.clone())),
            Ok(Some(target))
        );

        assert_eq!(
            reconcile_attachment_targets(
                Some(attachment_target("delegation-1", 4, 3)),
                Some(attachment_target("delegation-2", 4, 3)),
            ),
            Err(WrongAttachmentReason::SignalsDisagree {
                explicit_delegation_id: "delegation-1".to_string(),
                reply_delegation_id: "delegation-2".to_string(),
            })
        );
    }

    #[test]
    fn agreeing_ids_with_changed_revision_are_wrong_attachment() {
        assert_eq!(
            reconcile_attachment_targets(
                Some(attachment_target("delegation-1", 4, 3)),
                Some(attachment_target("delegation-1", 5, 4)),
            ),
            Err(WrongAttachmentReason::SignalsChanged)
        );
    }

    #[test]
    fn terminal_dual_signal_replays_must_still_name_the_same_delegation() {
        assert_eq!(
            reconcile_dual_replay_ids(Some("delegation-a"), Some("delegation-a")),
            Ok(Some("delegation-a".to_string()))
        );
        assert_eq!(
            reconcile_dual_replay_ids(Some("delegation-a"), Some("delegation-b")),
            Err(WrongAttachmentReason::SignalsDisagree {
                explicit_delegation_id: "delegation-a".to_string(),
                reply_delegation_id: "delegation-b".to_string(),
            })
        );
    }

    #[test]
    fn storage_stale_and_missing_results_map_to_typed_wrong_attachment() {
        assert_eq!(
            storage_attachment_failure_reason(
                &VerifiedFollowUpAmendmentOutcome::Stale {
                    current_state_revision: 9,
                },
                "delegation-1",
            ),
            Some(WrongAttachmentReason::StaleTarget {
                current_state_revision: 9,
            })
        );
        assert_eq!(
            storage_attachment_failure_reason(
                &VerifiedFollowUpAmendmentOutcome::NotFound,
                "delegation-1",
            ),
            Some(WrongAttachmentReason::MissingExplicitTarget {
                delegation_id: "delegation-1".to_string(),
            })
        );
    }
}
