//! Private storage-bound confirmation resolution. No route, callback, or
//! enrollment surface is exposed from this module.

#![allow(dead_code)]

use crate::execass_actor_gate::{
    RunControlOperation as ActorRunControlOperation, RunControlTarget as ActorRunControlTarget,
    VerifiedConfirmationEvent, VerifiedRunControlEvent,
};
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{
    bind_decision_resolution_owner_authority, bind_policy_snapshot_owner_authority,
    bind_runtime_settings_snapshot_owner_authority, owner_normalized_intent_digest, ActorAssurance,
    AuthenticatedLocalOwnerEvidence, DecisionResolutionAuthoritySource,
    PolicySnapshotAuthoritySource, RuntimeSettingsSnapshotAuthoritySource,
};
use carsinos_core::execass_danger::{
    danger_admission_signing_bytes, DangerAdmissionProof, SignedDangerAdmissionProof,
};
use carsinos_effect_recorder::{
    canonical_database_for_root, canonical_state_root, ReadOnlyBeganVerifier,
    RecorderChannelCustody, RecorderChannelCustodyError, RecorderClient, RecorderEndpoint,
    EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST, EXACT_OVERWRITE_ADAPTER_IDENTITY,
    EXACT_OVERWRITE_PROVIDER_IDENTITY, EXACT_OVERWRITE_PROVIDER_VERSION,
};
use carsinos_protocol::execass::{
    ActorType, LocalDecisionProofBinding, LocalOwnerMutationBinding, RunControlAttestation,
    RunControlAttestationPayload, RunControlOperation as ProtocolRunControlOperation,
    RunControlTarget as ProtocolRunControlTarget,
};
use carsinos_protocol::execass_recorder::{
    ExecuteOnceV1, OpaqueOperandEnvelopeV1, QueryOnlyV1, RecorderBindingV1,
    RecorderObservationKindV1, RecorderReplyV1, RecorderRequestV1, RECORDER_PROTOCOL_VERSION,
};
use carsinos_storage::execass::{
    technical_resource_lifecycle_evidence_reference_digest, ActorType as StorageActorType,
    AmendLifecycleCommand, AppendReceiptCommand, ApplyVerifiedFollowUpAmendmentCommand,
    AtomicDecisionResolutionCommand, AtomicDecisionResolutionOutcome,
    BeginProviderAttemptInvocationCommand, BeginProviderAttemptInvocationOutcome,
    CompleteDelegationStopDrainCommand, ConfirmationAttestationPayload,
    ConfirmationAuthorityIdentity, ContinuationCausationKind, ContinuationClaimCommand,
    ContinuationClaimIdentity, ContinuationClaimOutcome, ContinuationDispatchValidationCommand,
    ContinuationDispatchValidationOutcome, ContinuationRecord, ContinuationSettleCommand,
    ContinuationSettleOutcome, ContinuationStatus, DangerConfirmationResolutionOutcome,
    DangerConfirmationRuntimeProjection, DecisionKind as StorageDecisionKind,
    DecisionResolutionBinding, DecisionResult as StorageDecisionResult,
    DelegationRunControlMutationOutcome, DelegationRunControlStatus, DelegationStopDrainState,
    EngageGlobalStopCommand, ExecAssPolicyUpdateOutcome, ExecAssRuntimeSettingsUpdateOutcome,
    ExecAssStore, GlobalReceiptContext, GlobalStopDrainState, GlobalStopMutationOutcome,
    GlobalStopStatus, NewOutboxEvent, OutboxEventName, PendingDangerConfirmationAlternativeBinding,
    PrepareProviderAttemptCommand, PrepareProviderAttemptOutcome, ProviderRecoveryCommand,
    ProviderRecoveryOutcome, ReceiptActorBinding, ReceiptEvidenceInput, ReceiptIntegrityStore,
    ReceiptKeyRef, ReceiptKind, ReceiptRedactor, ReceiptRuntimeBinding, ReceiptSubject,
    ReceiptSubjectKind, ReconcileRecorderEvidenceCommand, RecorderAuthorityIdentity,
    RecorderEvidenceImportOutcome, RequestDelegationStopCommand, ResumeDelegationCommand,
    ResumeGlobalStopCommand, RunControlState as StorageRunControlState, RuntimeDesiredMode,
    RuntimeHostLeaseRecord, SafeText, SummaryAcknowledgementOutcome, SummaryDeliveredItem,
    SummaryDeliveryMetadata, SummaryDeliveryOutcome, TechnicalResourceActualInput,
    TechnicalResourceLifecycleCommand, TechnicalResourceLifecycleOutcome,
    TechnicalResourceLifecycleResolution, TechnicalResourceRecoveryKind,
    UpdateExecAssPolicyCommand, UpdateExecAssRuntimeSettingsCommand,
    VerifiedFollowUpAmendmentOutcome, WriteContext,
};
use carsinos_storage::JobRecord;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

const CUSTODY_SERVICE: &str = "com.carsinos.execass.confirmation-authority.v1";
const CUSTODY_DOMAIN: &[u8] = b"carsinos.execass.confirmation-authority.v1";

/// Binary-private authority adapter. It can only resolve a sealed verified
/// event against storage's selected binding; it neither exposes nor accepts
/// credentials, locators, signing keys, or caller-created action bytes.
#[allow(dead_code)]
pub(super) struct ExecAssConfirmationRuntime {
    store: ExecAssStore,
    identity: ConfirmationAuthorityIdentity,
    signer: FixedConfirmationAuthoritySigner,
    receipt_integrity: ReceiptIntegrityStore,
    receipt_key: ReceiptKeyRef,
    receipt_redactor: ReceiptRedactor,
    global_control_lock: Mutex<()>,
    /// The sole authenticated gateway-to-recorder capability. It is installed
    /// only after storage has activated the current runtime host and pinned
    /// the recorder's public identity. It is deliberately not a provider
    /// closure or an adapter registry.
    recorder_client: Option<RecorderClient>,
    recorder_binding: Option<RecorderBindingV1>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct SchedulerTechnicalResourceRecoveryReport {
    pub applied: usize,
    pub replayed: usize,
    pub stale: usize,
    /// A possibly invoked effect may only change after authenticated recorder
    /// QueryOnly/Reconcile evidence is verified and imported by storage.
    pub deferred_pending_recorder: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct SchedulerObjectiveRecoveryReport {
    pub evaluated: usize,
    pub stale: usize,
    pub retry_due: usize,
    pub deferred: usize,
    pub waiting_external: usize,
    pub waiting_for_user: usize,
    pub terminal: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypedDecisionResolutionOutcome {
    Confirmed,
    Applied,
    Replayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SchedulerRecorderImportOutcome {
    Deferred,
    Applied,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RunControlSigningSnapshot {
    pub stopped_epoch: i64,
    pub policy_revision: i64,
    pub unresolved_effect_disclosure_digest: String,
    pub delegation_state_revision: Option<i64>,
    pub current_plan_revision: Option<i64>,
}

/// Closed production dispatch selection. The only installed leaf is the
/// fixed exact-overwrite contract; every other tuple remains unavailable and
/// no arbitrary provider operand/result bridge exists.
#[derive(Debug, Clone)]
pub(super) enum ProductionRecorderDispatchMaterial {
    UnavailablePendingEa304,
    NoLongerAuthoritative,
    Installed(Box<InstalledRecorderDispatchMaterial>),
}

impl ExecAssConfirmationRuntime {
    pub(super) fn read_api_summary(
        &self,
        query: &carsinos_storage::execass::ExecAssProjectionQuery,
        metadata: &SummaryDeliveryMetadata,
    ) -> Result<(
        carsinos_storage::execass::ExecAssExecutiveProjection,
        SummaryDeliveryOutcome,
    )> {
        self.store.read_api_summary_with_delivery_metadata(
            &self.receipt_integrity,
            &self.receipt_redactor,
            query,
            metadata,
        )
    }

    pub(super) fn acknowledge_api_summary(
        &self,
        displayed_cursor: &str,
        idempotency_key: &str,
        acknowledged_at: i64,
        items: Vec<SummaryDeliveredItem>,
    ) -> Result<SummaryAcknowledgementOutcome> {
        self.store.acknowledge_api_summary_cursor(
            displayed_cursor,
            idempotency_key,
            acknowledged_at,
            items,
        )
    }

    pub(super) fn policy_snapshot_digest(&self, snapshot_json: &str) -> Result<String> {
        Ok(sha256_hex(
            &self.receipt_redactor.json(snapshot_json)?.canonical_bytes(),
        ))
    }

    pub(super) fn runtime_settings_digest(
        &self,
        desired_mode: RuntimeDesiredMode,
        start_at_login: bool,
        settings_json: &str,
    ) -> Result<String> {
        let safe = self.receipt_redactor.json(settings_json)?;
        let settings: serde_json::Value = serde_json::from_slice(&safe.canonical_bytes())?;
        Ok(sha256_hex(
            serde_json::json!({
                "desired_mode": desired_mode.as_str(),
                "start_at_login": start_at_login,
                "settings": settings,
            })
            .to_string()
            .as_bytes(),
        ))
    }

    pub(super) fn coordinate_verified_policy_update(
        &self,
        actor: &ActorAssurance,
        verified_client_id: &str,
        binding: &LocalOwnerMutationBinding,
        safe_snapshot_json: &str,
    ) -> Result<ExecAssPolicyUpdateOutcome> {
        let _guard = self
            .global_control_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("ExecAss owner mutation coordinator is poisoned"))?;
        let safe_snapshot = self.receipt_redactor.json(safe_snapshot_json)?;
        let snapshot_digest = sha256_hex(&safe_snapshot.canonical_bytes());
        if snapshot_digest != binding.safe_snapshot_digest {
            bail!("owner mutation safe snapshot binding changed");
        }
        let next_revision = binding
            .expected_revision
            .checked_add(1)
            .context("policy revision overflow")?;
        let custody_actor = self.local_owner_custody_actor(actor, verified_client_id, binding)?;
        let authority = bind_policy_snapshot_owner_authority(
            &custody_actor,
            PolicySnapshotAuthoritySource {
                canonical_mutation_bytes: owner_mutation_instruction_bytes(
                    verified_client_id,
                    binding,
                )?,
                canonical_safe_snapshot_json: String::from_utf8(safe_snapshot.canonical_bytes())?,
                policy_revision: next_revision,
                policy_snapshot_digest: snapshot_digest.clone(),
                created_at_ms: binding.created_at_ms,
            },
        )
        .map_err(|_| anyhow::anyhow!("invalid verified policy owner authority"))?;
        let context = self.current_global_receipt_context()?;
        let operation_id = owner_mutation_identity(binding);
        let event_id = format!("policy-event-{operation_id}");
        let causation_id = format!("policy-causation-{operation_id}");
        let receipt = self.build_owner_snapshot_receipt(
            &context,
            &authority,
            &operation_id,
            &event_id,
            &causation_id,
            ReceiptKind::Policy,
            ReceiptSubjectKind::PolicyRevision,
            "execass-policy",
            next_revision,
            "ExecAss owner policy snapshot changed",
            binding.created_at_ms,
        )?;
        let command = UpdateExecAssPolicyCommand {
            expected_policy_revision: binding.expected_revision,
            idempotency_key: binding.idempotency_key.clone(),
            safe_policy_snapshot: safe_snapshot,
            created_at: binding.created_at_ms,
            outbox_event: NewOutboxEvent {
                event_id,
                event_name: OutboxEventName::PolicyChanged,
                aggregate_id: "execass-policy".to_string(),
                aggregate_revision: next_revision,
                correlation_id: binding.request_correlation_id.clone(),
                causation_id,
                occurred_at: binding.created_at_ms,
                safe_payload_json: serde_json::json!({
                    "configured": true,
                    "policy_revision": next_revision,
                    "policy_snapshot_digest": snapshot_digest,
                })
                .to_string(),
                duplicate_identity: binding.idempotency_key.clone(),
            },
            receipt,
        };
        self.store.update_execass_policy_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
            &authority,
        )
    }

    pub(super) fn coordinate_verified_runtime_settings_update(
        &self,
        actor: &ActorAssurance,
        verified_client_id: &str,
        binding: &LocalOwnerMutationBinding,
        desired_mode: RuntimeDesiredMode,
        start_at_login: bool,
        safe_settings_json: &str,
    ) -> Result<ExecAssRuntimeSettingsUpdateOutcome> {
        let _guard = self
            .global_control_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("ExecAss owner mutation coordinator is poisoned"))?;
        let safe_settings = self.receipt_redactor.json(safe_settings_json)?;
        let settings_value: serde_json::Value =
            serde_json::from_slice(&safe_settings.canonical_bytes())?;
        let settings_digest = sha256_hex(
            serde_json::json!({
                "desired_mode": desired_mode.as_str(),
                "start_at_login": start_at_login,
                "settings": settings_value,
            })
            .to_string()
            .as_bytes(),
        );
        if settings_digest != binding.safe_snapshot_digest {
            bail!("owner mutation safe snapshot binding changed");
        }
        let next_revision = binding
            .expected_revision
            .checked_add(1)
            .context("runtime settings revision overflow")?;
        let context = self.current_global_receipt_context()?;
        let custody_actor = self.local_owner_custody_actor(actor, verified_client_id, binding)?;
        let authority = bind_runtime_settings_snapshot_owner_authority(
            &custody_actor,
            RuntimeSettingsSnapshotAuthoritySource {
                canonical_mutation_bytes: owner_mutation_instruction_bytes(
                    verified_client_id,
                    binding,
                )?,
                canonical_safe_snapshot_json: String::from_utf8(safe_settings.canonical_bytes())?,
                settings_revision: next_revision,
                settings_digest: settings_digest.clone(),
                policy_revision: context.global_stop.current_policy_revision,
                created_at_ms: binding.created_at_ms,
            },
        )
        .map_err(|_| anyhow::anyhow!("invalid verified runtime-settings owner authority"))?;
        let operation_id = owner_mutation_identity(binding);
        let event_id = format!("runtime-settings-event-{operation_id}");
        let causation_id = format!("runtime-settings-causation-{operation_id}");
        let receipt = self.build_owner_snapshot_receipt(
            &context,
            &authority,
            &operation_id,
            &event_id,
            &causation_id,
            ReceiptKind::RuntimeSettings,
            ReceiptSubjectKind::RuntimeSettingsRevision,
            "execass-runtime-host",
            next_revision,
            "ExecAss owner runtime settings changed",
            binding.created_at_ms,
        )?;
        let actual_state = if desired_mode == RuntimeDesiredMode::Background {
            "running_background"
        } else {
            "running_app_bound"
        };
        let command = UpdateExecAssRuntimeSettingsCommand {
            expected_settings_revision: binding.expected_revision,
            idempotency_key: binding.idempotency_key.clone(),
            desired_mode,
            start_at_login,
            safe_settings,
            created_at: binding.created_at_ms,
            outbox_event: NewOutboxEvent {
                event_id,
                event_name: OutboxEventName::RuntimeHostChanged,
                aggregate_id: "execass-runtime-host".to_string(),
                aggregate_revision: next_revision,
                correlation_id: binding.request_correlation_id.clone(),
                causation_id,
                occurred_at: binding.created_at_ms,
                safe_payload_json: serde_json::json!({
                    "actual_state_if_running": actual_state,
                    "desired_mode": desired_mode.as_str(),
                    "settings_digest": settings_digest,
                    "settings_revision": next_revision,
                    "start_at_login": start_at_login,
                })
                .to_string(),
                duplicate_identity: binding.idempotency_key.clone(),
            },
            receipt,
        };
        self.store.update_execass_runtime_settings_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
            &authority,
        )
    }

    fn current_global_receipt_context(&self) -> Result<GlobalReceiptContext> {
        let now = i64::try_from(server_time_ms()?).context("system clock exceeds storage range")?;
        self.store
            .read_global_receipt_context(now)?
            .context("owner mutation requires one active runtime receipt context")
    }

    fn local_owner_custody_actor(
        &self,
        verified_actor: &ActorAssurance,
        verified_client_id: &str,
        binding: &LocalOwnerMutationBinding,
    ) -> Result<ActorAssurance> {
        if verified_actor.actor_type() != ActorType::HumanLocal
            || verified_actor.credential_identity() != verified_client_id
        {
            bail!("owner mutation is not exact local-owner evidence");
        }
        let evidence = AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
            self.identity.local_credential_identity(),
            "native-control",
            "interactive-local",
            binding.request_correlation_id.clone(),
        )
        .map_err(|_| anyhow::anyhow!("local owner custody evidence is invalid"))?;
        Ok(carsinos_core::execass_actor::derive_local_owner_actor_assurance(evidence))
    }

    #[allow(clippy::too_many_arguments)]
    fn build_owner_snapshot_receipt(
        &self,
        context: &GlobalReceiptContext,
        authority: &carsinos_core::execass_actor::VerifiedOwnerAuthority,
        operation_id: &str,
        event_id: &str,
        causation_id: &str,
        receipt_kind: ReceiptKind,
        subject_kind: ReceiptSubjectKind,
        subject_id: &str,
        revision: i64,
        summary: &str,
        created_at: i64,
    ) -> Result<AppendReceiptCommand> {
        let (actor_type, actor_identity) = match authority.evidence() {
            carsinos_core::execass_actor::VerifiedHumanEvidenceRef::Local {
                authenticated_client_id,
                ..
            } => (
                StorageActorType::HumanLocal,
                authenticated_client_id.to_string(),
            ),
            carsinos_core::execass_actor::VerifiedHumanEvidenceRef::Remote {
                adapter_id,
                provider_account_id,
                ..
            } => (
                StorageActorType::HumanRemote,
                format!("{adapter_id}:{provider_account_id}"),
            ),
        };
        Ok(AppendReceiptCommand {
            receipt_id: format!("owner-mutation-receipt-{operation_id}"),
            transaction_id: format!("owner-mutation-transaction-{operation_id}"),
            state_root_generation: context.state_root_generation,
            delegation_id: "execass-global-control-carrier".to_string(),
            expected_state_revision: context.carrier_state_revision,
            expected_global_count: context.global_receipt_count,
            expected_global_head_digest: context.global_receipt_head_digest.clone(),
            expected_delegation_count: context.carrier_receipt_count,
            expected_delegation_head_digest: context.carrier_receipt_head_digest.clone(),
            receipt_kind,
            subject: ReceiptSubject {
                kind: subject_kind,
                subject_id: subject_id.to_string(),
                revision,
            },
            causation_id: causation_id.to_string(),
            causation_event_id: event_id.to_string(),
            actor: ReceiptActorBinding {
                actor_type,
                actor_identity: self.receipt_redactor.text(&actor_identity)?,
                authority_provenance_id: authority.authority_provenance_id().to_string(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id.clone(),
                fencing_token: context.runtime_fencing_token,
            },
            key: self.receipt_key.clone(),
            rotation: None,
            evidence: Vec::new(),
            redacted_summary: self.receipt_redactor.summary(summary)?,
            occurred_at: created_at,
            committed_at: created_at,
        })
    }
}

fn owner_mutation_instruction_bytes(
    verified_client_id: &str,
    binding: &LocalOwnerMutationBinding,
) -> Result<Vec<u8>> {
    let proof = carsinos_protocol::execass::LocalOwnerMutationProof {
        authenticated_client_id: verified_client_id.to_string(),
        request_correlation_id: binding.request_correlation_id.clone(),
        proof_hex: "00".repeat(32),
    };
    carsinos_protocol::execass::local_owner_mutation_proof_bytes(&proof, binding)
        .map_err(|_| anyhow::anyhow!("invalid owner mutation binding"))
}

fn owner_mutation_identity(binding: &LocalOwnerMutationBinding) -> String {
    sha256_hex(
        format!(
            "carsinos.execass.owner-mutation-operation.v1\0{}\0{}\0{}\0{}\0{}\0{}",
            binding.operation.name(),
            binding.idempotency_key,
            binding.request_correlation_id,
            binding.expected_revision,
            binding.canonical_body_digest,
            binding.safe_snapshot_digest,
        )
        .as_bytes(),
    )
}

/// Binary-private EA-304 seam. No route, provider, process, channel, plugin,
/// tool runner, callback, or caller-supplied result can construct it. The fixed
/// installed leaf supplies exact immutable recorder protocol material and uses
/// the signed continuation settlement bridge in this module.
#[derive(Debug, Clone)]
pub(super) struct InstalledRecorderDispatchMaterial {
    execute_once: ExecuteOnceV1,
    query_only: QueryOnlyV1,
}

impl InstalledRecorderDispatchMaterial {
    fn from_attempt(
        binding: &RecorderBindingV1,
        attempt: &carsinos_storage::execass::ProviderAttemptRecord,
        operand_envelope: OpaqueOperandEnvelopeV1,
        trusted_now: i64,
    ) -> Result<Self> {
        let dispatch = &attempt.dispatch;
        if dispatch.provider_identity.as_deref() != Some(EXACT_OVERWRITE_PROVIDER_IDENTITY)
            || dispatch.provider_idempotency_key.is_some()
            || dispatch.reconciliation_key.is_none()
        {
            bail!("storage attempt is not the exact installed overwrite leaf");
        }
        let material_identity = sha256_hex(
            format!(
                "carsinos.execass.exact-overwrite-recorder-command.v1\0{}\0{}\0{}",
                attempt.attempt_id, attempt.provider_request_digest, dispatch.payload_digest
            )
            .as_bytes(),
        );
        let execute_once = ExecuteOnceV1 {
            binding: binding.clone(),
            request_id: format!("exact-overwrite-execute-{material_identity}"),
            claim_event_id: dispatch.claim_event_id.clone(),
            claim_receipt_id: dispatch.claim_receipt_id.clone(),
            continuation_fencing_token: dispatch.continuation_fencing_token,
            delegation_id: dispatch.delegation_id.clone(),
            continuation_id: dispatch.continuation_id.clone(),
            action_id: dispatch.action_id.clone(),
            logical_effect_id: dispatch.logical_effect_id.clone(),
            internal_idempotency_key: dispatch.internal_idempotency_key.clone(),
            attempt_id: attempt.attempt_id.clone(),
            attempt_number: attempt.attempt_number,
            provider_identity: EXACT_OVERWRITE_PROVIDER_IDENTITY.to_owned(),
            provider_version: EXACT_OVERWRITE_PROVIDER_VERSION.to_owned(),
            adapter_identity: EXACT_OVERWRITE_ADAPTER_IDENTITY.to_owned(),
            adapter_artifact_digest: EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST.to_owned(),
            provider_request_digest: attempt.provider_request_digest.clone(),
            provider_idempotency_key: None,
            reconciliation_key: dispatch.reconciliation_key.clone(),
            manifest_digest: dispatch.manifest_digest.clone(),
            payload_digest: dispatch.payload_digest.clone(),
            operand_envelope,
            deadline_ms: trusted_now.saturating_add(30_000),
            client_nonce: format!("exact-overwrite-execute-nonce-{material_identity}"),
            command_mac: String::new(),
        };
        if execute_once.derived_provider_request_digest()? != attempt.provider_request_digest {
            bail!("exact overwrite recorder command drifted from the storage request digest");
        }
        let query_only = QueryOnlyV1 {
            binding: binding.clone(),
            request_id: format!("exact-overwrite-query-{material_identity}"),
            attempt_id: attempt.attempt_id.clone(),
            expected_command_digest: None,
            known_journal_head: None,
            client_nonce: format!("exact-overwrite-query-nonce-{material_identity}"),
            command_mac: String::new(),
        };
        Ok(Self {
            execute_once,
            query_only,
        })
    }

    fn ensure_matches_attempt(
        &self,
        attempt: &carsinos_storage::execass::ProviderAttemptRecord,
    ) -> Result<()> {
        let dispatch = &attempt.dispatch;
        if self.query_only.binding != self.execute_once.binding
            || self.execute_once.attempt_id != attempt.attempt_id
            || self.query_only.attempt_id != attempt.attempt_id
            || self.execute_once.attempt_number != attempt.attempt_number
            || self.execute_once.claim_event_id != dispatch.claim_event_id
            || self.execute_once.claim_receipt_id != dispatch.claim_receipt_id
            || self.execute_once.continuation_fencing_token != dispatch.continuation_fencing_token
            || self.execute_once.delegation_id != dispatch.delegation_id
            || self.execute_once.continuation_id != dispatch.continuation_id
            || self.execute_once.action_id != dispatch.action_id
            || self.execute_once.logical_effect_id != dispatch.logical_effect_id
            || self.execute_once.internal_idempotency_key != dispatch.internal_idempotency_key
            || dispatch.provider_identity.as_deref()
                != Some(self.execute_once.provider_identity.as_str())
            || self.execute_once.provider_idempotency_key != dispatch.provider_idempotency_key
            || self.execute_once.reconciliation_key != dispatch.reconciliation_key
            || self.execute_once.manifest_digest != dispatch.manifest_digest
            || self.execute_once.payload_digest != dispatch.payload_digest
            || self.execute_once.provider_request_digest != attempt.provider_request_digest
            || self.execute_once.binding.runtime_host_generation != dispatch.runtime_host_generation
            || self.execute_once.binding.runtime_host_instance_id
                != dispatch.runtime_host_instance_id
            || self.execute_once.binding.runtime_fencing_token != dispatch.runtime_fencing_token
        {
            bail!("installed EA-304 recorder material does not match the exact storage attempt");
        }
        Ok(())
    }

    fn request_for(
        &self,
        outcome: BeginProviderAttemptInvocationOutcome,
    ) -> Result<Option<RecorderRequestV1>> {
        let attempt = match &outcome {
            BeginProviderAttemptInvocationOutcome::Began(attempt)
            | BeginProviderAttemptInvocationOutcome::AlreadyInvoking(attempt) => attempt,
            BeginProviderAttemptInvocationOutcome::Stale { .. }
            | BeginProviderAttemptInvocationOutcome::Conflict
            | BeginProviderAttemptInvocationOutcome::NotFound => return Ok(None),
        };
        self.ensure_matches_attempt(attempt)?;
        Ok(match outcome {
            BeginProviderAttemptInvocationOutcome::Began(_) => Some(
                RecorderRequestV1::ExecuteOnce(Box::new(self.execute_once.clone())),
            ),
            BeginProviderAttemptInvocationOutcome::AlreadyInvoking(_) => Some(
                RecorderRequestV1::QueryOnly(Box::new(self.query_only.clone())),
            ),
            BeginProviderAttemptInvocationOutcome::Stale { .. }
            | BeginProviderAttemptInvocationOutcome::Conflict
            | BeginProviderAttemptInvocationOutcome::NotFound => None,
        })
    }
}

/// Recorder sidecar supervision cannot be claimed until EA-401 owns an exact
/// install-relative signed artifact contract. No PATH/process fallback exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecorderServiceStartup {
    AuthenticatedRecorderReady,
    UnavailablePendingInstallRelativeSidecarPackagingEa401,
}

impl fmt::Debug for ExecAssConfirmationRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecAssConfirmationRuntime")
            .field("authority", &self.identity)
            .field("signer", &"[REDACTED]")
            .field(
                "recorder_client",
                &self.recorder_client.as_ref().map(|_| "READY"),
            )
            .field(
                "recorder_binding",
                &self.recorder_binding.as_ref().map(|binding| {
                    (
                        &binding.canonical_root_identity,
                        &binding.installation_id,
                        binding.state_root_generation,
                    )
                }),
            )
            .finish_non_exhaustive()
    }
}

impl ExecAssConfirmationRuntime {
    pub(super) fn sign_verified_run_control(
        &self,
        event: &VerifiedRunControlEvent,
        snapshot: &RunControlSigningSnapshot,
    ) -> Result<RunControlAttestation> {
        require_exact_run_control_snapshot(event, snapshot)?;
        let state_root_generation = i64::try_from(self.identity.state_root_generation())
            .context("run-control state-root generation exceeds signed contract range")?;
        let signer_key_generation = i64::try_from(self.identity.key_generation())
            .context("run-control signer generation exceeds signed contract range")?;
        // The actor gate already freshness-checks this trusted observation.
        // Reusing it as issuance time makes the signed envelope and the
        // storage command byte-for-byte stable across an exact retry.
        let issued_at_ms = event.observed_at_ms();
        let actor_type = event.actor_type();
        let (credential_identity, source_message_id, provider_event_id) = match actor_type {
            ActorType::HumanLocal => (
                self.identity.local_credential_identity().to_string(),
                None,
                None,
            ),
            ActorType::HumanRemote => (
                event.credential_identity().to_string(),
                event.source_message_id().map(str::to_string),
                event.provider_event_id().map(str::to_string),
            ),
            _ => bail!("non-human run-control evidence cannot be signed"),
        };
        let payload = RunControlAttestationPayload {
            actor_type,
            credential_identity,
            authenticated_ingress: event.authenticated_ingress().to_string(),
            channel_assurance: event.channel_assurance().to_string(),
            request_correlation_id: event.request_correlation_id().to_string(),
            source_message_id,
            provider_event_id,
            operation: protocol_run_control_operation(event.operation()),
            target: protocol_run_control_target(event.target()),
            idempotency_key: event.idempotency_key().to_string(),
            replay_identity: event.replay_identity().to_string(),
            observed_at_ms: event.observed_at_ms(),
            issued_at_ms,
            stopped_epoch: snapshot.stopped_epoch,
            policy_revision: snapshot.policy_revision,
            unresolved_effect_disclosure_digest: snapshot
                .unresolved_effect_disclosure_digest
                .clone(),
            delegation_state_revision: snapshot.delegation_state_revision,
            current_plan_revision: snapshot.current_plan_revision,
            canonical_root_identity: self.identity.canonical_root_identity().to_string(),
            installation_identity: self.identity.installation_identity().to_string(),
            os_user_identity_digest: self.identity.os_user_identity_digest().to_string(),
            state_root_generation,
            signer_key_generation,
        };
        self.signer
            .sign_run_control_attestation(&self.identity, payload)
    }

    /// Applies one opaque, server-verified global control event. The caller
    /// cannot provide actor bindings, receipt material, signing snapshots, or
    /// storage fences; every such value is read or derived here.
    pub(super) fn coordinate_verified_global_control(
        &self,
        event: &VerifiedRunControlEvent,
    ) -> Result<GlobalStopMutationOutcome> {
        let operation = require_global_control_event(event)?;
        let _coordinator = self
            .global_control_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("global control coordinator is poisoned"))?;
        let resume_attestation = if operation == ActorRunControlOperation::GlobalResume {
            let resume = event
                .resume()
                .context("verified global resume omitted its signed storage snapshot")?;
            let snapshot = RunControlSigningSnapshot {
                stopped_epoch: resume.stopped_epoch(),
                policy_revision: resume.current_policy_revision(),
                unresolved_effect_disclosure_digest: resume
                    .unresolved_effect_disclosure_digest()
                    .to_string(),
                delegation_state_revision: resume.delegation_state_revision(),
                current_plan_revision: resume.current_plan_revision(),
            };
            let attestation = self.sign_verified_run_control(event, &snapshot)?;
            if let Some(replayed) = self.store.replay_global_resume_attestation(&attestation)? {
                return Ok(replayed);
            }
            Some(attestation)
        } else {
            None
        };

        let status = self.store.global_stop_status()?;
        let context = self
            .store
            .read_global_receipt_context(event.observed_at_ms())?
            .context("global control requires one active, exact runtime receipt context")?;
        if context.global_stop != status {
            bail!("global control storage snapshot changed while it was being read");
        }

        match operation {
            ActorRunControlOperation::GlobalStop => {
                let mut command =
                    self.build_global_stop_command(event, &context, GlobalStopDrainState::Drained)?;
                let outcome = match self.store.engage_global_stop_atomically(
                    &self.receipt_integrity,
                    &self.receipt_redactor,
                    &command,
                ) {
                    Ok(outcome) => outcome,
                    Err(error) if is_stop_drain_disclosure_mismatch(&error) => {
                        command = self.build_global_stop_command(
                            event,
                            &context,
                            GlobalStopDrainState::Draining,
                        )?;
                        self.store.engage_global_stop_atomically(
                            &self.receipt_integrity,
                            &self.receipt_redactor,
                            &command,
                        )?
                    }
                    Err(error) => return Err(error),
                };
                Ok(outcome)
            }
            ActorRunControlOperation::GlobalResume => {
                if !status.engaged {
                    bail!("global resume does not match an engaged stop snapshot");
                }
                require_exact_run_control_snapshot(
                    event,
                    &RunControlSigningSnapshot {
                        stopped_epoch: status.global_stop_epoch,
                        policy_revision: status.current_policy_revision,
                        unresolved_effect_disclosure_digest: status
                            .unresolved_external_effects_digest
                            .clone(),
                        delegation_state_revision: None,
                        current_plan_revision: None,
                    },
                )?;
                let attestation = resume_attestation
                    .context("verified global resume lost its signed storage attestation")?;
                let command = self.build_global_resume_command(event, &context, attestation)?;
                self.store.resume_global_stop_atomically(
                    &self.receipt_integrity,
                    &self.receipt_redactor,
                    &command,
                )
            }
            ActorRunControlOperation::DelegationStop
            | ActorRunControlOperation::DelegationResume => {
                unreachable!("delegation operations are rejected before storage access")
            }
        }
    }

    /// Applies one opaque, actor-gate-verified control event to one exact
    /// delegation. Caller-selected authority, receipt, snapshot, and runtime
    /// fields never cross this seam.
    pub(super) fn coordinate_verified_delegation_control(
        &self,
        event: &VerifiedRunControlEvent,
        delegation_id: &str,
    ) -> Result<DelegationRunControlMutationOutcome> {
        let operation = require_delegation_control_event(event, delegation_id)?;
        let _coordinator = self
            .global_control_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("run-control coordinator is poisoned"))?;

        let resume_attestation = if operation == ActorRunControlOperation::DelegationResume {
            let resume = event
                .resume()
                .context("verified delegation resume omitted its exact stopped snapshot")?;
            let snapshot = RunControlSigningSnapshot {
                stopped_epoch: resume.stopped_epoch(),
                policy_revision: resume.current_policy_revision(),
                unresolved_effect_disclosure_digest: resume
                    .unresolved_effect_disclosure_digest()
                    .to_string(),
                delegation_state_revision: resume.delegation_state_revision(),
                current_plan_revision: resume.current_plan_revision(),
            };
            let attestation = self.sign_verified_run_control(event, &snapshot)?;
            if let Some(replayed) = self.store.replay_delegation_resume_attestation(
                delegation_id,
                &attestation,
                event.observed_at_ms(),
            )? {
                return Ok(replayed);
            }
            Some(attestation)
        } else {
            None
        };

        let Some(status) = self
            .store
            .read_delegation_run_control_status(delegation_id, event.observed_at_ms())?
        else {
            return Ok(DelegationRunControlMutationOutcome::NotFound);
        };
        match operation {
            ActorRunControlOperation::DelegationStop => {
                if status.run_control == StorageRunControlState::Stopped {
                    return Ok(DelegationRunControlMutationOutcome::AlreadyStopped(status));
                }
                if status.run_control == StorageRunControlState::StopRequested {
                    return self.complete_ready_delegation_stop(status, event.observed_at_ms());
                }
                let snapshot = RunControlSigningSnapshot {
                    stopped_epoch: status.stop_epoch,
                    policy_revision: status.policy_revision,
                    unresolved_effect_disclosure_digest: status
                        .unresolved_external_effects_digest
                        .clone(),
                    delegation_state_revision: Some(status.state_revision),
                    current_plan_revision: status.current_plan_revision,
                };
                let attestation = self.sign_verified_run_control(event, &snapshot)?;
                let command = self.build_delegation_stop_command(event, &status, attestation)?;
                let outcome = self.store.request_delegation_stop_atomically(
                    &self.receipt_integrity,
                    &self.receipt_redactor,
                    &command,
                )?;
                match outcome {
                    DelegationRunControlMutationOutcome::StopRequested(ref stopped)
                        if stopped.drain_state == DelegationStopDrainState::ReadyToStop =>
                    {
                        self.complete_ready_delegation_stop(stopped.clone(), event.observed_at_ms())
                    }
                    other => Ok(other),
                }
            }
            ActorRunControlOperation::DelegationResume => {
                if status.run_control != StorageRunControlState::Stopped {
                    bail!("delegation resume requires an exact stopped state");
                }
                require_exact_run_control_snapshot(
                    event,
                    &RunControlSigningSnapshot {
                        stopped_epoch: status.stop_epoch,
                        policy_revision: status.policy_revision,
                        unresolved_effect_disclosure_digest: status
                            .unresolved_external_effects_digest
                            .clone(),
                        delegation_state_revision: Some(status.state_revision),
                        current_plan_revision: status.current_plan_revision,
                    },
                )?;
                let attestation = resume_attestation
                    .context("verified delegation resume lost its signed attestation")?;
                let command = self.build_delegation_resume_command(event, &status, attestation)?;
                self.store.resume_delegation_atomically(
                    &self.receipt_integrity,
                    &self.receipt_redactor,
                    &command,
                )
            }
            ActorRunControlOperation::GlobalStop | ActorRunControlOperation::GlobalResume => {
                unreachable!("global operations are rejected before delegation storage access")
            }
        }
    }

    pub(super) fn read_delegation_control_status(
        &self,
        delegation_id: &str,
        trusted_now: i64,
    ) -> Result<Option<DelegationRunControlStatus>> {
        self.store
            .read_delegation_run_control_status(delegation_id, trusted_now)
    }

    /// Completes every bounded stop drain that has reached a safe execution
    /// boundary since the owner's original stop request. No additional human
    /// confirmation is required and no continuation is created.
    pub(super) fn complete_ready_delegation_stops(
        &self,
        trusted_now: i64,
        limit: usize,
    ) -> Result<usize> {
        let _coordinator = self
            .global_control_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("run-control coordinator is poisoned"))?;
        let ready = self.store.list_ready_delegation_stops(trusted_now, limit)?;
        let mut completed = 0usize;
        for status in ready {
            match self.complete_ready_delegation_stop(status, trusted_now)? {
                DelegationRunControlMutationOutcome::Drained(_)
                | DelegationRunControlMutationOutcome::Replayed(_)
                | DelegationRunControlMutationOutcome::AlreadyStopped(_) => {
                    completed = completed.saturating_add(1);
                }
                DelegationRunControlMutationOutcome::Stale(_)
                | DelegationRunControlMutationOutcome::StopRequested(_)
                | DelegationRunControlMutationOutcome::Resumed(_)
                | DelegationRunControlMutationOutcome::NotFound => {}
            }
        }
        Ok(completed)
    }

    fn complete_ready_delegation_stop(
        &self,
        status: DelegationRunControlStatus,
        trusted_now: i64,
    ) -> Result<DelegationRunControlMutationOutcome> {
        if status.run_control == StorageRunControlState::Stopped {
            return Ok(DelegationRunControlMutationOutcome::AlreadyStopped(status));
        }
        if status.drain_state != DelegationStopDrainState::ReadyToStop {
            return Ok(DelegationRunControlMutationOutcome::AlreadyStopped(status));
        }
        let command = self.build_delegation_drain_command(&status, trusted_now)?;
        self.store.complete_delegation_stop_drain_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
        )
    }

    fn build_delegation_stop_command(
        &self,
        event: &VerifiedRunControlEvent,
        status: &DelegationRunControlStatus,
        attestation: RunControlAttestation,
    ) -> Result<RequestDelegationStopCommand> {
        let next_revision = status
            .state_revision
            .checked_add(1)
            .context("delegation state revision overflow")?;
        let next_epoch = status
            .stop_epoch
            .checked_add(1)
            .context("delegation stop epoch overflow")?;
        let operation_id = delegation_control_operation_identity(event);
        let receipt = self.build_delegation_control_receipt(
            status,
            event.observed_at_ms(),
            &operation_id,
            next_revision,
            false,
            "stop requested by verified owner",
        )?;
        let outbox_event = build_delegation_control_outbox(
            &status.delegation_id,
            next_revision,
            event.request_correlation_id(),
            event.idempotency_key(),
            event.observed_at_ms(),
            &operation_id,
            &receipt.causation_id,
            delegation_control_payload(
                status,
                "stop_requested",
                StorageRunControlState::StopRequested,
                next_revision,
                next_epoch,
                if status.executing_branch_count == 0 {
                    DelegationStopDrainState::ReadyToStop
                } else {
                    DelegationStopDrainState::Draining
                },
            ),
        );
        Ok(RequestDelegationStopCommand {
            delegation_id: status.delegation_id.clone(),
            expected_state_revision: status.state_revision,
            expected_stop_epoch: status.stop_epoch,
            expected_plan_revision: status.current_plan_revision,
            expected_policy_revision: status.policy_revision,
            disclosed_unresolved_external_effects_digest: status
                .unresolved_external_effects_digest
                .clone(),
            attestation,
            trusted_now: event.observed_at_ms(),
            outbox_event,
            receipt,
        })
    }

    fn build_delegation_resume_command(
        &self,
        event: &VerifiedRunControlEvent,
        status: &DelegationRunControlStatus,
        attestation: RunControlAttestation,
    ) -> Result<ResumeDelegationCommand> {
        let next_revision = status
            .state_revision
            .checked_add(1)
            .context("delegation state revision overflow")?;
        let next_epoch = status
            .stop_epoch
            .checked_add(1)
            .context("delegation stop epoch overflow")?;
        let operation_id = delegation_control_operation_identity(event);
        let receipt = self.build_delegation_control_receipt(
            status,
            event.observed_at_ms(),
            &operation_id,
            next_revision,
            false,
            "resumed by verified owner",
        )?;
        let outbox_event = build_delegation_control_outbox(
            &status.delegation_id,
            next_revision,
            event.request_correlation_id(),
            event.idempotency_key(),
            event.observed_at_ms(),
            &operation_id,
            &receipt.causation_id,
            delegation_control_payload(
                status,
                "resumed",
                StorageRunControlState::Running,
                next_revision,
                next_epoch,
                DelegationStopDrainState::Running,
            ),
        );
        Ok(ResumeDelegationCommand {
            delegation_id: status.delegation_id.clone(),
            expected_state_revision: status.state_revision,
            expected_plan_revision: status.current_plan_revision,
            expected_stop_epoch: status.stop_epoch,
            expected_policy_revision: status.policy_revision,
            disclosed_unresolved_external_effects_digest: status
                .unresolved_external_effects_digest
                .clone(),
            attestation,
            trusted_now: event.observed_at_ms(),
            outbox_event,
            receipt,
        })
    }

    fn build_delegation_drain_command(
        &self,
        status: &DelegationRunControlStatus,
        trusted_now: i64,
    ) -> Result<CompleteDelegationStopDrainCommand> {
        let next_revision = status
            .state_revision
            .checked_add(1)
            .context("delegation state revision overflow")?;
        let operation_id = sha256_hex(
            format!(
                "carsinos.execass.delegation-drain.v1\0{}\0{}\0{}",
                status.delegation_id, status.state_revision, status.stop_epoch
            )
            .as_bytes(),
        );
        let receipt = self.build_delegation_control_receipt(
            status,
            trusted_now,
            &operation_id,
            next_revision,
            true,
            "drain completed at safe boundary",
        )?;
        let outbox_event = build_delegation_control_outbox(
            &status.delegation_id,
            next_revision,
            &format!("delegation-drain-correlation-{operation_id}"),
            &format!("delegation-drain-idempotency-{operation_id}"),
            trusted_now,
            &operation_id,
            &receipt.causation_id,
            delegation_control_payload(
                status,
                "stopped",
                StorageRunControlState::Stopped,
                next_revision,
                status.stop_epoch,
                DelegationStopDrainState::Stopped,
            ),
        );
        Ok(CompleteDelegationStopDrainCommand {
            delegation_id: status.delegation_id.clone(),
            expected_state_revision: status.state_revision,
            expected_stop_epoch: status.stop_epoch,
            trusted_now,
            outbox_event,
            receipt,
        })
    }

    fn build_delegation_control_receipt(
        &self,
        status: &DelegationRunControlStatus,
        trusted_now: i64,
        operation_id: &str,
        next_revision: i64,
        runtime_actor: bool,
        summary: &str,
    ) -> Result<AppendReceiptCommand> {
        let runtime = status
            .runtime
            .as_ref()
            .context("delegation run control requires one active runtime host")?;
        Ok(AppendReceiptCommand {
            receipt_id: format!("delegation-control-receipt-{operation_id}"),
            transaction_id: format!("delegation-control-transaction-{operation_id}"),
            state_root_generation: runtime.state_root_generation,
            delegation_id: status.delegation_id.clone(),
            expected_state_revision: next_revision,
            expected_global_count: status.global_receipt_count,
            expected_global_head_digest: status.global_receipt_head_digest.clone(),
            expected_delegation_count: status.delegation_receipt_count,
            expected_delegation_head_digest: status.delegation_receipt_head_digest.clone(),
            receipt_kind: ReceiptKind::RunControl,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Delegation,
                subject_id: status.delegation_id.clone(),
                revision: next_revision,
            },
            causation_id: format!("delegation-control-causation-{operation_id}"),
            causation_event_id: format!("delegation-control-event-{operation_id}"),
            actor: ReceiptActorBinding {
                actor_type: StorageActorType::Runtime,
                actor_identity: SafeText::new("execass-global-control", &[])?,
                authority_provenance_id: "execass-global-control-carrier-authority".to_string(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: runtime.runtime_host_generation,
                host_instance_id: runtime.runtime_host_instance_id.clone(),
                fencing_token: runtime.runtime_fencing_token,
            },
            key: self.receipt_key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: self.receipt_redactor.summary(&format!(
                "ExecAss delegation control {summary}{}",
                if runtime_actor {
                    " by trusted runtime"
                } else {
                    ""
                }
            ))?,
            occurred_at: trusted_now,
            committed_at: trusted_now,
        })
    }

    /// Read-only status projection for the gateway route layer. No receipt
    /// key, actor authority, signer, or mutation command crosses this seam.
    pub(super) fn read_global_control_status(&self) -> Result<GlobalStopStatus> {
        self.store.global_stop_status()
    }

    /// Engage the circuit breaker from CarsinOS' sealed runtime safety
    /// authority. This deliberately has no resume branch and accepts no actor,
    /// authority, receipt, target, or policy fields from its caller.
    pub(super) fn engage_trusted_global_fail_safe(&self) -> Result<GlobalStopMutationOutcome> {
        let _coordinator = self
            .global_control_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("global control coordinator is poisoned"))?;
        let trusted_now = i64::try_from(server_time_ms()?)
            .context("trusted global fail-safe clock exceeds storage range")?;
        let status = self.store.global_stop_status()?;
        if status.engaged {
            return Ok(GlobalStopMutationOutcome::AlreadyEngaged(status));
        }
        let context = self
            .store
            .read_global_receipt_context(trusted_now)?
            .context("trusted global fail-safe requires one active runtime receipt context")?;
        if context.global_stop != status {
            bail!("trusted global fail-safe snapshot changed while it was being read");
        }
        let mut command = self.build_trusted_global_stop_command(
            &context,
            trusted_now,
            GlobalStopDrainState::Drained,
        )?;
        match self.store.engage_global_stop_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
        ) {
            Ok(outcome) => Ok(outcome),
            Err(error) if is_stop_drain_disclosure_mismatch(&error) => {
                command = self.build_trusted_global_stop_command(
                    &context,
                    trusted_now,
                    GlobalStopDrainState::Draining,
                )?;
                self.store.engage_global_stop_atomically(
                    &self.receipt_integrity,
                    &self.receipt_redactor,
                    &command,
                )
            }
            Err(error) => Err(error),
        }
    }

    fn build_global_stop_command(
        &self,
        event: &VerifiedRunControlEvent,
        context: &GlobalReceiptContext,
        drain_state: GlobalStopDrainState,
    ) -> Result<EngageGlobalStopCommand> {
        let resulting_epoch = context
            .global_stop
            .global_stop_epoch
            .checked_add(1)
            .context("global stop epoch overflow")?;
        let operation_id = global_control_operation_identity(event);
        let resulting_status = carsinos_storage::execass::GlobalStopStatus {
            engaged: true,
            global_stop_epoch: resulting_epoch,
            drain_state,
            current_policy_revision: context.global_stop.current_policy_revision,
            unresolved_external_effects: context.global_stop.unresolved_external_effects.clone(),
            unresolved_external_effects_digest: context
                .global_stop
                .unresolved_external_effects_digest
                .clone(),
        };
        let receipt = self.build_global_control_receipt(
            event,
            context,
            &operation_id,
            resulting_epoch,
            "stopped",
        )?;
        Ok(EngageGlobalStopCommand {
            expected_global_stop_epoch: context.global_stop.global_stop_epoch,
            trusted_now: event.observed_at_ms(),
            outbox_event: build_global_control_outbox(
                event,
                &operation_id,
                &receipt.causation_id,
                &resulting_status,
                "engaged",
            ),
            receipt,
        })
    }

    fn build_global_resume_command(
        &self,
        event: &VerifiedRunControlEvent,
        context: &GlobalReceiptContext,
        attestation: RunControlAttestation,
    ) -> Result<ResumeGlobalStopCommand> {
        let operation_id = global_control_operation_identity(event);
        let mut resulting_status = context.global_stop.clone();
        resulting_status.engaged = false;
        resulting_status.drain_state = GlobalStopDrainState::Running;
        let receipt = self.build_global_control_receipt(
            event,
            context,
            &operation_id,
            resulting_status.global_stop_epoch,
            "resumed",
        )?;
        Ok(ResumeGlobalStopCommand {
            expected_global_stop_epoch: context.global_stop.global_stop_epoch,
            expected_policy_revision: context.global_stop.current_policy_revision,
            disclosed_unresolved_external_effects_digest: context
                .global_stop
                .unresolved_external_effects_digest
                .clone(),
            attestation,
            trusted_now: event.observed_at_ms(),
            outbox_event: build_global_control_outbox(
                event,
                &operation_id,
                &receipt.causation_id,
                &resulting_status,
                "resumed",
            ),
            receipt,
        })
    }

    fn build_trusted_global_stop_command(
        &self,
        context: &GlobalReceiptContext,
        trusted_now: i64,
        drain_state: GlobalStopDrainState,
    ) -> Result<EngageGlobalStopCommand> {
        let resulting_epoch = context
            .global_stop
            .global_stop_epoch
            .checked_add(1)
            .context("global stop epoch overflow")?;
        let operation_id = sha256_hex(
            format!(
                "carsinos.execass.trusted-global-fail-safe.v1\0{}\0{}\0{}\0{}",
                self.identity.canonical_root_identity(),
                context.state_root_generation,
                context.global_stop.global_stop_epoch,
                trusted_now,
            )
            .as_bytes(),
        );
        let causation_id = format!("global-control-causation-{operation_id}");
        let event_id = format!("global-control-event-{operation_id}");
        let resulting_status = GlobalStopStatus {
            engaged: true,
            global_stop_epoch: resulting_epoch,
            drain_state,
            current_policy_revision: context.global_stop.current_policy_revision,
            unresolved_external_effects: context.global_stop.unresolved_external_effects.clone(),
            unresolved_external_effects_digest: context
                .global_stop
                .unresolved_external_effects_digest
                .clone(),
        };
        let receipt = AppendReceiptCommand {
            receipt_id: format!("global-control-receipt-{operation_id}"),
            transaction_id: format!("global-control-transaction-{operation_id}"),
            state_root_generation: context.state_root_generation,
            delegation_id: "execass-global-control-carrier".to_string(),
            expected_state_revision: context.carrier_state_revision,
            expected_global_count: context.global_receipt_count,
            expected_global_head_digest: context.global_receipt_head_digest.clone(),
            expected_delegation_count: context.carrier_receipt_count,
            expected_delegation_head_digest: context.carrier_receipt_head_digest.clone(),
            receipt_kind: ReceiptKind::GlobalStop,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::GlobalRuntimeControl,
                subject_id: "global-stop-all".to_string(),
                revision: resulting_epoch,
            },
            causation_id: causation_id.clone(),
            causation_event_id: event_id.clone(),
            actor: ReceiptActorBinding {
                actor_type: StorageActorType::Runtime,
                actor_identity: SafeText::new("execass-global-control", &[])?,
                authority_provenance_id: "execass-global-control-carrier-authority".to_string(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id.clone(),
                fencing_token: context.runtime_fencing_token,
            },
            key: self.receipt_key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: self
                .receipt_redactor
                .summary("ExecAss global control stopped by trusted runtime fail-safe")?,
            occurred_at: trusted_now,
            committed_at: trusted_now,
        };
        Ok(EngageGlobalStopCommand {
            expected_global_stop_epoch: context.global_stop.global_stop_epoch,
            trusted_now,
            outbox_event: NewOutboxEvent {
                event_id,
                event_name: OutboxEventName::GlobalStopChanged,
                aggregate_id: "global-stop-all".to_string(),
                aggregate_revision: resulting_epoch,
                correlation_id: format!("global-control-correlation-{operation_id}"),
                causation_id,
                occurred_at: trusted_now,
                safe_payload_json: global_control_payload(&resulting_status, "engaged"),
                duplicate_identity: format!("global-control-idempotency-{operation_id}"),
            },
            receipt,
        })
    }

    fn build_global_control_receipt(
        &self,
        event: &VerifiedRunControlEvent,
        context: &GlobalReceiptContext,
        operation_id: &str,
        resulting_epoch: i64,
        summary_operation: &str,
    ) -> Result<AppendReceiptCommand> {
        Ok(AppendReceiptCommand {
            receipt_id: format!("global-control-receipt-{operation_id}"),
            transaction_id: format!("global-control-transaction-{operation_id}"),
            state_root_generation: context.state_root_generation,
            delegation_id: "execass-global-control-carrier".to_string(),
            expected_state_revision: context.carrier_state_revision,
            expected_global_count: context.global_receipt_count,
            expected_global_head_digest: context.global_receipt_head_digest.clone(),
            expected_delegation_count: context.carrier_receipt_count,
            expected_delegation_head_digest: context.carrier_receipt_head_digest.clone(),
            receipt_kind: ReceiptKind::GlobalStop,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::GlobalRuntimeControl,
                subject_id: "global-stop-all".to_string(),
                revision: resulting_epoch,
            },
            causation_id: format!("global-control-causation-{operation_id}"),
            causation_event_id: format!("global-control-event-{operation_id}"),
            actor: ReceiptActorBinding {
                actor_type: StorageActorType::Runtime,
                actor_identity: SafeText::new("execass-global-control", &[])?,
                authority_provenance_id: "execass-global-control-carrier-authority".to_string(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id.clone(),
                fencing_token: context.runtime_fencing_token,
            },
            key: self.receipt_key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: self.receipt_redactor.summary(&format!(
                "ExecAss global control {summary_operation} by verified owner request"
            ))?,
            occurred_at: event.observed_at_ms(),
            committed_at: event.observed_at_ms(),
        })
    }

    pub(super) fn evaluate_due_scheduler_recoveries(
        &self,
        trusted_now: i64,
        limit: u32,
    ) -> Result<SchedulerObjectiveRecoveryReport> {
        use carsinos_core::execass_recovery::RecoveryDirective;

        let candidates = self
            .store
            .list_due_provider_recovery_effects(trusted_now, limit)?;
        let mut report = SchedulerObjectiveRecoveryReport::default();
        for logical_effect_id in candidates {
            let context = self
                .store
                .read_provider_recovery_receipt_context(&logical_effect_id, trusted_now)?
                .context("due recovery effect has no current receipt/runtime context")?;
            let operation_id = objective_recovery_operation_identity(
                &logical_effect_id,
                context.delegation_revision,
            );
            let event_id = format!("recovery-event-{operation_id}");
            let causation_id = format!("recovery-cause-{operation_id}");
            let post_revision = context.delegation_revision + 1;
            let command = ProviderRecoveryCommand {
                write: WriteContext {
                    idempotency_key: format!("recovery-write-{operation_id}"),
                    correlation_id: format!("recovery-correlation-{operation_id}"),
                    causation_id: causation_id.clone(),
                    occurred_at: trusted_now,
                },
                logical_effect_id: logical_effect_id.clone(),
                trusted_now,
                expected_pre_state_revision: context.delegation_revision,
                receipt: AppendReceiptCommand {
                    receipt_id: format!("recovery-receipt-{operation_id}"),
                    transaction_id: format!("recovery-transaction-{operation_id}"),
                    state_root_generation: context.state_root_generation,
                    delegation_id: context.delegation_id,
                    expected_state_revision: post_revision,
                    expected_global_count: context.global_receipt_count,
                    expected_global_head_digest: context.global_receipt_head_digest,
                    expected_delegation_count: context.delegation_receipt_count,
                    expected_delegation_head_digest: context.delegation_receipt_head_digest,
                    receipt_kind: ReceiptKind::Recovery,
                    subject: ReceiptSubject {
                        kind: ReceiptSubjectKind::OutboxEvent,
                        subject_id: event_id.clone(),
                        revision: post_revision,
                    },
                    causation_id,
                    causation_event_id: event_id,
                    actor: context.runtime_actor,
                    runtime: ReceiptRuntimeBinding {
                        host_generation: context.runtime_host_generation,
                        host_instance_id: context.runtime_host_instance_id,
                        fencing_token: context.runtime_fencing_token,
                    },
                    key: self.receipt_key.clone(),
                    rotation: None,
                    evidence: vec![],
                    redacted_summary: self
                        .receipt_redactor
                        .summary("ExecAss objective recovery updated")?,
                    occurred_at: trusted_now,
                    committed_at: trusted_now,
                },
            };
            let bundle = match self.store.apply_provider_recovery_atomically(
                &self.receipt_integrity,
                &self.receipt_redactor,
                &command,
            )? {
                ProviderRecoveryOutcome::Applied(bundle)
                | ProviderRecoveryOutcome::Replayed(bundle) => bundle,
                ProviderRecoveryOutcome::Stale { .. } => {
                    report.stale += 1;
                    continue;
                }
            };
            report.evaluated += 1;
            match bundle.evaluation.directive {
                RecoveryDirective::RetrySameEffect { .. } => report.retry_due += 1,
                RecoveryDirective::WaitUntil { .. }
                | RecoveryDirective::ReplanWithinOriginalAuthority => report.deferred += 1,
                RecoveryDirective::WaitingExternal => report.waiting_external += 1,
                RecoveryDirective::WaitingForUser => report.waiting_for_user += 1,
                RecoveryDirective::PartiallyCompleted | RecoveryDirective::Failed => {
                    report.terminal += 1
                }
            }
        }
        Ok(report)
    }

    /// Activates and opens only the fixed current-user authority bound to the
    /// already-open canonical store. Any custody/pinning mismatch fails closed.
    pub(super) fn open(store: ExecAssStore) -> Result<Self> {
        let identity = store.activate_confirmation_authority()?;
        let signer = FixedConfirmationAuthoritySigner::open(&identity)?;
        if signer.identity() != &identity {
            bail!("opened confirmation signer does not match activated authority");
        }
        let (receipt_integrity, receipt_key, receipt_redactor) = open_receipt_runtime(&store)?;
        Ok(Self {
            store,
            identity,
            signer,
            receipt_integrity,
            receipt_key,
            receipt_redactor,
            global_control_lock: Mutex::new(()),
            recorder_client: None,
            recorder_binding: None,
        })
    }

    pub(super) fn authority_identity(&self) -> &ConfirmationAuthorityIdentity {
        &self.identity
    }

    /// Activates the single scheduler-owned host through the canonical
    /// receipt/attention recovery transaction. Production startup must use
    /// this seam so an unclean predecessor can never be silently replaced.
    pub(super) fn activate_runtime_host_with_recovery(
        &self,
        host_instance_id: &str,
        trusted_now: i64,
    ) -> Result<RuntimeHostLeaseRecord> {
        self.store.activate_runtime_host_with_recovery(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &self.identity,
            host_instance_id,
            trusted_now,
        )
    }

    /// Pins the recorder identity and creates the authenticated client only
    /// after the scheduler-owned runtime host exists. The gateway has no
    /// caller credential and retains no direct provider execution surface.
    pub(super) async fn activate_recorder_client(
        &mut self,
        configured_state_root: &Path,
        runtime_host: &RuntimeHostLeaseRecord,
        trusted_now: i64,
    ) -> Result<()> {
        if self.recorder_client.is_some() || self.recorder_binding.is_some() {
            bail!("recorder client is already activated for this runtime");
        }
        if trusted_now <= 0 {
            bail!("recorder client activation requires a positive trusted clock");
        }
        let state_root = canonical_state_root(configured_state_root)?;
        let database =
            canonical_database_for_root(&state_root.path.join("carsinos.db"), &state_root)?;
        let authoritative =
            ReadOnlyBeganVerifier::new(database).load_authoritative_binding(trusted_now)?;
        if authoritative.canonical_root_identity != state_root.identity
            || authoritative.state_root_generation != runtime_host.state_root_generation
            || authoritative.runtime_host_generation != runtime_host.generation
            || authoritative.runtime_host_instance_id != runtime_host.host_instance_id
            || authoritative.runtime_fencing_token != runtime_host.fencing_token
        {
            bail!("active runtime host does not match recorder authoritative binding");
        }
        // This opens only the channel/MAC record with public recorder identity.
        // The signing seed remains exclusively in the recorder service custody.
        let credential =
            RecorderChannelCustody::load_existing(&authoritative.canonical_root_identity)
                .map_err(anyhow::Error::new)?;
        let identity = credential.identity();
        identity.validate_for_root(&authoritative.canonical_root_identity)?;
        let binding = RecorderBindingV1 {
            protocol_version: RECORDER_PROTOCOL_VERSION.to_owned(),
            canonical_root_identity: authoritative.canonical_root_identity,
            installation_id: authoritative.installation_id,
            state_root_generation: authoritative.state_root_generation,
            os_user_identity_digest: authoritative.os_user_identity_digest,
            runtime_host_generation: authoritative.runtime_host_generation,
            runtime_host_instance_id: authoritative.runtime_host_instance_id,
            runtime_fencing_token: authoritative.runtime_fencing_token,
        };
        let client = RecorderClient::new(
            RecorderEndpoint::for_binding(
                &state_root.path,
                &binding.installation_id,
                binding.state_root_generation,
            ),
            *credential.channel_key(),
            identity.clone(),
        );
        // Never first-use-pin unauthenticated OS-custody channel metadata. A
        // signed QueryOnly handshake proves the endpoint possesses this exact
        // recorder key; the probe cannot execute a provider effect.
        client.prove_recorder_possession(&binding).await?;
        self.store.pin_or_validate_recorder_identity(
            &RecorderAuthorityIdentity {
                recorder_key_id: identity.key_id.clone(),
                key_generation: identity.key_generation,
                verifying_key_hex: identity.verifying_key_hex.clone(),
                verifying_key_digest: identity.verifying_key_digest.clone(),
                canonical_root_identity: binding.canonical_root_identity.clone(),
                installation_identity: binding.installation_id.clone(),
                os_user_identity_digest: binding.os_user_identity_digest.clone(),
                state_root_generation: binding.state_root_generation,
            },
            trusted_now,
        )?;
        self.recorder_client = Some(client);
        self.recorder_binding = Some(binding);
        Ok(())
    }

    pub(super) fn recorder_client_is_ready(&self) -> bool {
        self.recorder_client.is_some() && self.recorder_binding.is_some()
    }

    pub(super) fn is_expected_recorder_provisioning_gap(error: &anyhow::Error) -> bool {
        error.chain().any(|cause| {
            cause
                .downcast_ref::<RecorderChannelCustodyError>()
                .is_some_and(|cause| matches!(cause, RecorderChannelCustodyError::Missing))
        })
    }

    pub(super) fn production_recorder_dispatch_material(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
    ) -> Result<ProductionRecorderDispatchMaterial> {
        let Some(client_binding) = self.recorder_binding.as_ref() else {
            return Ok(ProductionRecorderDispatchMaterial::UnavailablePendingEa304);
        };
        if self.recorder_client.is_none() {
            return Ok(ProductionRecorderDispatchMaterial::UnavailablePendingEa304);
        }
        let Some(exact) = self.store.read_exact_dangerous_effect_execution_material(
            &identity.delegation_id,
            &identity.continuation_id,
        )?
        else {
            return Ok(ProductionRecorderDispatchMaterial::UnavailablePendingEa304);
        };
        if exact.provider_identity != EXACT_OVERWRITE_PROVIDER_IDENTITY
            || exact.provider_version != EXACT_OVERWRITE_PROVIDER_VERSION
            || exact.adapter_identity != EXACT_OVERWRITE_ADAPTER_IDENTITY
        {
            bail!("storage returned an unsupported exact recorder descriptor");
        }
        let attempt = match self.prepare_scheduler_effect_attempt(identity, trusted_now, None)? {
            PrepareProviderAttemptOutcome::Prepared(attempt)
            | PrepareProviderAttemptOutcome::Replayed(attempt) => attempt,
            PrepareProviderAttemptOutcome::Stale { .. }
            | PrepareProviderAttemptOutcome::Conflict => {
                return Ok(ProductionRecorderDispatchMaterial::NoLongerAuthoritative)
            }
        };
        if exact.logical_effect_id != attempt.dispatch.logical_effect_id
            || exact.payload_digest != attempt.dispatch.payload_digest
            || Some(exact.reconciliation_key.as_str())
                != attempt.dispatch.reconciliation_key.as_deref()
        {
            bail!("exact recorder descriptor drifted from the prepared storage attempt");
        }
        Ok(ProductionRecorderDispatchMaterial::Installed(Box::new(
            InstalledRecorderDispatchMaterial::from_attempt(
                client_binding,
                &attempt,
                exact.operand_envelope,
                trusted_now,
            )?,
        )))
    }

    /// The sole production branch that maps storage authorization to recorder
    /// IPC. Keeping the fixed leaf here prevents any caller or later adapter
    /// from reimplementing or weakening the Begin/AlreadyInvoking cutover.
    pub(super) async fn dispatch_installed_recorder_material(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
        material: &InstalledRecorderDispatchMaterial,
    ) -> Result<Option<carsinos_protocol::execass_recorder::RecorderReplyV1>> {
        let client = self
            .recorder_client
            .as_ref()
            .context("installed EA-304 leaf requires an authenticated recorder client")?;
        let binding = self
            .recorder_binding
            .as_ref()
            .context("installed EA-304 leaf requires an active recorder binding")?;
        if material.execute_once.binding != *binding || material.query_only.binding != *binding {
            bail!("installed EA-304 recorder material is not bound to the active recorder");
        }
        let attempt = match self.prepare_scheduler_effect_attempt(identity, trusted_now, None)? {
            PrepareProviderAttemptOutcome::Prepared(attempt)
            | PrepareProviderAttemptOutcome::Replayed(attempt) => attempt,
            PrepareProviderAttemptOutcome::Stale { .. }
            | PrepareProviderAttemptOutcome::Conflict => return Ok(None),
        };
        // Repeat the exact storage comparison immediately before the
        // irreversible prepared -> invoking boundary. The earlier material
        // constructor check is not authority for this later transition.
        material.ensure_matches_attempt(&attempt)?;
        let outcome = self.begin_scheduler_effect_invocation(
            &material.execute_once.attempt_id,
            identity,
            trusted_now,
        )?;
        let Some(request) = material.request_for(outcome)? else {
            return Ok(None);
        };
        Ok(Some(client.send(request).await?))
    }

    /// Verify and atomically converge one terminal reply from the separately
    /// authenticated recorder. Nonterminal journal observations and a
    /// transport-visible `NotFound` remain deferred: neither is evidence that
    /// the effect did or did not occur, and the next fenced pass may only use
    /// QueryOnly/Reconcile.
    pub(super) fn import_scheduler_recorder_reply(
        &self,
        identity: &ContinuationClaimIdentity,
        reply: RecorderReplyV1,
        trusted_now: i64,
    ) -> Result<SchedulerRecorderImportOutcome> {
        let observation = match reply {
            RecorderReplyV1::NotFound { .. } => {
                return Ok(SchedulerRecorderImportOutcome::Deferred)
            }
            RecorderReplyV1::Rejected { code, .. } => {
                bail!("effect recorder rejected the exact installed leaf: {code}")
            }
            RecorderReplyV1::Observation { observation, .. } => *observation,
        };
        if matches!(
            observation.kind,
            RecorderObservationKindV1::Accepted | RecorderObservationKindV1::InvocationStarted
        ) {
            return Ok(SchedulerRecorderImportOutcome::Deferred);
        }
        if !matches!(
            observation.kind,
            RecorderObservationKindV1::Present
                | RecorderObservationKindV1::Absent
                | RecorderObservationKindV1::Unknown
        ) {
            bail!("effect recorder returned an unsupported observation kind");
        }
        let verified = self
            .store
            .verify_recorder_evidence(&observation, trusted_now)
            .map_err(|error| {
                anyhow::anyhow!("effect recorder evidence verification failed: {error}")
            })?;
        let context = self
            .store
            .read_continuation_receipt_context(&identity.continuation_id, trusted_now)?
            .context("recorder reply lost its continuation receipt context")?;
        let result = match observation.kind {
            RecorderObservationKindV1::Present => "present",
            RecorderObservationKindV1::Absent => "absent",
            RecorderObservationKindV1::Unknown => "unknown",
            RecorderObservationKindV1::Accepted | RecorderObservationKindV1::InvocationStarted => {
                unreachable!("returned above")
            }
        };
        let operation_id = sha256_hex(
            format!(
                "carsinos.execass.recorder-import.v1\0{}\0{}\0{}",
                identity.claim_event_id, observation.attempt_id, observation.record_digest
            )
            .as_bytes(),
        );
        let write_id = format!("recorder-import-write-{operation_id}");
        let event_id = format!("recorder-import-event-{operation_id}");
        let correlation_id = format!("recorder-import-correlation-{operation_id}");
        let safe_payload_json = serde_json::to_string(&serde_json::json!({
            "attempt_id": observation.attempt_id,
            "logical_effect_id": observation.logical_effect_id,
            "recorder_record_digest": observation.record_digest,
            "result": result,
        }))?;
        let command = ReconcileRecorderEvidenceCommand {
            write: WriteContext {
                idempotency_key: write_id.clone(),
                correlation_id: correlation_id.clone(),
                causation_id: observation.record_digest.clone(),
                occurred_at: trusted_now,
            },
            claim_identity: identity.clone(),
            trusted_now,
            verified_evidence: verified,
            outbox_event: NewOutboxEvent {
                event_id: event_id.clone(),
                event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
                aggregate_id: identity.delegation_id.clone(),
                aggregate_revision: context.delegation_revision,
                correlation_id,
                causation_id: observation.record_digest.clone(),
                occurred_at: trusted_now,
                safe_payload_json,
                duplicate_identity: write_id,
            },
            receipt: AppendReceiptCommand {
                receipt_id: format!("recorder-import-receipt-{operation_id}"),
                transaction_id: format!("recorder-import-transaction-{operation_id}"),
                state_root_generation: identity.state_root_generation,
                delegation_id: identity.delegation_id.clone(),
                expected_state_revision: context.delegation_revision,
                expected_global_count: context.global_receipt_count,
                expected_global_head_digest: context.global_receipt_head_digest,
                expected_delegation_count: context.delegation_receipt_count,
                expected_delegation_head_digest: context.delegation_receipt_head_digest,
                receipt_kind: ReceiptKind::Continuation,
                subject: ReceiptSubject {
                    kind: ReceiptSubjectKind::Continuation,
                    subject_id: identity.continuation_id.clone(),
                    revision: context.delegation_revision,
                },
                causation_id: observation.record_digest,
                causation_event_id: event_id,
                actor: context.runtime_actor,
                runtime: ReceiptRuntimeBinding {
                    host_generation: context.runtime_host_generation,
                    host_instance_id: context.runtime_host_instance_id,
                    fencing_token: context.runtime_fencing_token,
                },
                key: self.receipt_key.clone(),
                rotation: None,
                evidence: Vec::new(),
                redacted_summary: self
                    .receipt_redactor
                    .summary("signed effect-recorder result imported")?,
                occurred_at: trusted_now,
                committed_at: trusted_now,
            },
        };
        match self.store.reconcile_recorder_evidence_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
        )? {
            RecorderEvidenceImportOutcome::Applied(_) => {
                Ok(SchedulerRecorderImportOutcome::Applied)
            }
            RecorderEvidenceImportOutcome::Replayed(_) => {
                Ok(SchedulerRecorderImportOutcome::Replayed)
            }
            RecorderEvidenceImportOutcome::Conflict => {
                bail!("signed recorder evidence conflicted with the winning import")
            }
            RecorderEvidenceImportOutcome::Lost { reason } => {
                bail!("signed recorder evidence lost continuation authority: {reason:?}")
            }
            RecorderEvidenceImportOutcome::Stale { .. } => {
                bail!("signed recorder evidence lost receipt-chain authority")
            }
        }
    }

    pub(super) fn recorder_service_startup(&self) -> RecorderServiceStartup {
        if self.recorder_client_is_ready() {
            RecorderServiceStartup::AuthenticatedRecorderReady
        } else {
            RecorderServiceStartup::UnavailablePendingInstallRelativeSidecarPackagingEa401
        }
    }

    pub(super) fn claim_scheduler_continuation(
        &self,
        job: &JobRecord,
        worker_id: &str,
        trusted_now: i64,
    ) -> Result<ContinuationClaimOutcome> {
        let payload: serde_json::Value = serde_json::from_str(&job.payload_json)
            .context("continuation job payload is invalid")?;
        let continuation_id = payload
            .get("continuation_id")
            .and_then(serde_json::Value::as_str)
            .context("continuation job payload has no continuation identity")?;
        let job_lease_expires_at = job
            .lease_expires_at
            .context("continuation job has no acquired lease")?;
        let context = self
            .store
            .read_continuation_receipt_context(continuation_id, trusted_now)?
            .context("continuation has no current receipt/runtime context")?;
        let operation_id = scheduler_operation_identity(
            "claim",
            &job.job_id,
            continuation_id,
            worker_id,
            job_lease_expires_at,
        );
        let event_id = format!("continuation-claim-{operation_id}");
        let command = ContinuationClaimCommand {
            write: WriteContext {
                idempotency_key: format!("continuation-claim-write-{operation_id}"),
                correlation_id: format!("continuation-claim-correlation-{operation_id}"),
                causation_id: format!("continuation-claim-cause-{operation_id}"),
                occurred_at: trusted_now,
            },
            continuation_id: continuation_id.to_owned(),
            job_id: job.job_id.clone(),
            worker_id: worker_id.to_owned(),
            job_lease_expires_at,
            trusted_now,
            outbox_event: NewOutboxEvent {
                event_id: event_id.clone(),
                event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
                aggregate_id: context.delegation_id.clone(),
                aggregate_revision: context.delegation_revision,
                correlation_id: format!("continuation-claim-correlation-{operation_id}"),
                causation_id: format!("continuation-claim-cause-{operation_id}"),
                occurred_at: trusted_now,
                safe_payload_json: serde_json::json!({
                    "operation": "claim",
                    "continuation_id": continuation_id,
                    "job_id": job.job_id,
                })
                .to_string(),
                duplicate_identity: format!("continuation-claim-write-{operation_id}"),
            },
            receipt: AppendReceiptCommand {
                receipt_id: format!("continuation-claim-receipt-{operation_id}"),
                transaction_id: format!("continuation-claim-transaction-{operation_id}"),
                state_root_generation: context.state_root_generation,
                delegation_id: context.delegation_id,
                expected_state_revision: context.delegation_revision,
                expected_global_count: context.global_receipt_count,
                expected_global_head_digest: context.global_receipt_head_digest,
                expected_delegation_count: context.delegation_receipt_count,
                expected_delegation_head_digest: context.delegation_receipt_head_digest,
                receipt_kind: ReceiptKind::Continuation,
                subject: ReceiptSubject {
                    kind: ReceiptSubjectKind::Continuation,
                    subject_id: continuation_id.to_owned(),
                    revision: context.delegation_revision,
                },
                causation_id: format!("continuation-claim-cause-{operation_id}"),
                causation_event_id: event_id,
                actor: context.runtime_actor,
                runtime: ReceiptRuntimeBinding {
                    host_generation: context.runtime_host_generation,
                    host_instance_id: context.runtime_host_instance_id,
                    fencing_token: context.runtime_fencing_token,
                },
                key: self.receipt_key.clone(),
                rotation: None,
                evidence: vec![],
                redacted_summary: self
                    .receipt_redactor
                    .summary("ExecAss continuation claim recorded")?,
                occurred_at: trusted_now,
                committed_at: trusted_now,
            },
        };
        self.store.claim_continuation_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
        )
    }

    pub(super) fn validate_scheduler_continuation_pre_dispatch(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
    ) -> Result<ContinuationDispatchValidationOutcome> {
        self.store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: identity.clone(),
                trusted_now,
            })
    }

    /// Persists stable logical-effect dispatch material without authorizing an
    /// external call. Storage derives every key and attempt identity from the
    /// immutable effect and already-fenced claim.
    pub(super) fn prepare_scheduler_effect_attempt(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
        retry_authorization: Option<carsinos_storage::execass::ProviderRetryAuthorization>,
    ) -> Result<PrepareProviderAttemptOutcome> {
        self.store
            .prepare_provider_attempt(&PrepareProviderAttemptCommand {
                claim: identity.clone(),
                trusted_now,
                retry_authorization,
            })
    }

    /// Crosses the sole dispatch-authorizing boundary. An adapter may be
    /// called only for `Began`; `AlreadyInvoking` is non-authorizing and must
    /// never cause a second provider call.
    pub(super) fn begin_scheduler_effect_invocation(
        &self,
        attempt_id: &str,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
    ) -> Result<BeginProviderAttemptInvocationOutcome> {
        self.store
            .begin_provider_attempt_invocation(&BeginProviderAttemptInvocationCommand {
                attempt_id: attempt_id.to_owned(),
                claim: identity.clone(),
                trusted_now,
            })
    }

    pub(super) fn recover_scheduler_technical_resources(
        &self,
        trusted_now: i64,
        limit: u32,
    ) -> Result<SchedulerTechnicalResourceRecoveryReport> {
        let candidates = self
            .store
            .list_technical_resource_recovery_candidates(trusted_now, limit)?;
        let mut report = SchedulerTechnicalResourceRecoveryReport::default();
        for candidate in candidates {
            let resolution = match candidate.kind {
                TechnicalResourceRecoveryKind::ExpireUndispatched => {
                    TechnicalResourceLifecycleResolution::ExpireUndispatched
                }
                TechnicalResourceRecoveryKind::RecoverPossiblyInvoked => {
                    // A stale/restarted gateway has no authority to construct
                    // an outcome or actuals. EA-304 must provide exact
                    // Reconcile material; until then preserve uncertainty.
                    report.deferred_pending_recorder += 1;
                    continue;
                }
            };
            let authoritative_evidence = self
                .store
                .resolve_technical_resource_recovery_job_evidence(&candidate.identity)?;
            match self.resolve_scheduler_continuation_technical_resources(
                &candidate.identity,
                trusted_now,
                resolution,
                authoritative_evidence,
                vec![],
            )? {
                TechnicalResourceLifecycleOutcome::Applied(_) => report.applied += 1,
                TechnicalResourceLifecycleOutcome::Replayed(_) => report.replayed += 1,
                TechnicalResourceLifecycleOutcome::Lost { .. }
                | TechnicalResourceLifecycleOutcome::Stale { .. } => report.stale += 1,
            }
        }
        Ok(report)
    }

    pub(super) fn resolve_scheduler_continuation_technical_resources(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
        resolution: TechnicalResourceLifecycleResolution,
        authoritative_evidence: ReceiptEvidenceInput,
        technical_resource_actuals: Vec<TechnicalResourceActualInput>,
    ) -> Result<TechnicalResourceLifecycleOutcome> {
        let command = self.build_technical_resource_lifecycle_command(
            identity,
            trusted_now,
            resolution,
            authoritative_evidence,
            technical_resource_actuals,
        )?;
        self.store.resolve_technical_resources_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
        )
    }

    fn build_technical_resource_lifecycle_command(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
        resolution: TechnicalResourceLifecycleResolution,
        authoritative_evidence: ReceiptEvidenceInput,
        technical_resource_actuals: Vec<TechnicalResourceActualInput>,
    ) -> Result<TechnicalResourceLifecycleCommand> {
        let context = self
            .store
            .read_continuation_receipt_context(&identity.continuation_id, trusted_now)?
            .context("claimed continuation has no current receipt/runtime context")?;
        let operation = technical_resource_lifecycle_resolution_name(resolution);
        let evidence_digest = technical_resource_lifecycle_evidence_reference_digest(
            std::slice::from_ref(&authoritative_evidence),
        )?;
        let operation_id = technical_resource_lifecycle_operation_identity(
            identity,
            resolution,
            &evidence_digest,
            &technical_resource_actuals,
        );
        let event_id = format!("continuation-resource-{operation}-{operation_id}");
        let write_id = format!("continuation-resource-write-{operation_id}");
        let correlation_id = format!("continuation-resource-correlation-{operation_id}");
        let causation_id = format!("continuation-resource-cause-{operation_id}");
        let actual_count = technical_resource_actuals.len();
        Ok(TechnicalResourceLifecycleCommand {
            write: WriteContext {
                idempotency_key: write_id.clone(),
                correlation_id: correlation_id.clone(),
                causation_id: causation_id.clone(),
                occurred_at: trusted_now,
            },
            identity: identity.clone(),
            trusted_now,
            resolution,
            evidence_digest,
            technical_resource_actuals,
            outbox_event: NewOutboxEvent {
                event_id: event_id.clone(),
                event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
                aggregate_id: identity.delegation_id.clone(),
                aggregate_revision: context.delegation_revision,
                correlation_id,
                causation_id: causation_id.clone(),
                occurred_at: trusted_now,
                safe_payload_json: serde_json::json!({
                    "operation": "technical_resource_lifecycle",
                    "resolution": operation,
                    "continuation_id": identity.continuation_id,
                    "claim_event_id": identity.claim_event_id,
                    "actual_count": actual_count,
                })
                .to_string(),
                duplicate_identity: write_id,
            },
            receipt: AppendReceiptCommand {
                receipt_id: format!("continuation-resource-receipt-{operation_id}"),
                transaction_id: format!("continuation-resource-transaction-{operation_id}"),
                state_root_generation: context.state_root_generation,
                delegation_id: identity.delegation_id.clone(),
                expected_state_revision: context.delegation_revision,
                expected_global_count: context.global_receipt_count,
                expected_global_head_digest: context.global_receipt_head_digest,
                expected_delegation_count: context.delegation_receipt_count,
                expected_delegation_head_digest: context.delegation_receipt_head_digest,
                receipt_kind: ReceiptKind::Continuation,
                subject: ReceiptSubject {
                    kind: ReceiptSubjectKind::Continuation,
                    subject_id: identity.continuation_id.clone(),
                    revision: context.delegation_revision,
                },
                causation_id,
                causation_event_id: event_id,
                actor: context.runtime_actor,
                runtime: ReceiptRuntimeBinding {
                    host_generation: context.runtime_host_generation,
                    host_instance_id: context.runtime_host_instance_id,
                    fencing_token: context.runtime_fencing_token,
                },
                key: self.receipt_key.clone(),
                rotation: None,
                evidence: vec![authoritative_evidence],
                redacted_summary: self.receipt_redactor.summary(match resolution {
                    TechnicalResourceLifecycleResolution::ExpireUndispatched => {
                        "ExecAss undispatched technical resources expired"
                    }
                    TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => {
                        "ExecAss possible invocation moved to independent reconciliation"
                    }
                    TechnicalResourceLifecycleResolution::ReconcileAbsent => {
                        "ExecAss absent technical resources reconciled"
                    }
                    TechnicalResourceLifecycleResolution::ReconcilePresent => {
                        "ExecAss present technical resources reconciled"
                    }
                })?,
                occurred_at: trusted_now,
                committed_at: trusted_now,
            },
        })
    }

    pub(super) fn settle_scheduler_continuation_adapter_unavailable(
        &self,
        identity: &ContinuationClaimIdentity,
        trusted_now: i64,
    ) -> Result<ContinuationSettleOutcome> {
        let context = self
            .store
            .read_continuation_receipt_context(&identity.continuation_id, trusted_now)?
            .context("claimed continuation lost its receipt context before settle")?;
        let operation_id = scheduler_operation_identity(
            "adapter-unavailable",
            &identity.job_id,
            &identity.continuation_id,
            &identity.worker_id,
            identity.job_lease_expires_at,
        );
        let event_id = format!("continuation-waiting-{operation_id}");
        let command = ContinuationSettleCommand {
            write: WriteContext {
                idempotency_key: format!("continuation-waiting-write-{operation_id}"),
                correlation_id: format!("continuation-waiting-correlation-{operation_id}"),
                causation_id: format!("continuation-waiting-cause-{operation_id}"),
                occurred_at: trusted_now,
            },
            identity: identity.clone(),
            trusted_now,
            result_status: ContinuationStatus::Waiting,
            technical_resource_actuals: Vec::new(),
            outbox_event: NewOutboxEvent {
                event_id: event_id.clone(),
                event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
                aggregate_id: identity.delegation_id.clone(),
                aggregate_revision: context.delegation_revision,
                correlation_id: format!("continuation-waiting-correlation-{operation_id}"),
                causation_id: format!("continuation-waiting-cause-{operation_id}"),
                occurred_at: trusted_now,
                safe_payload_json: serde_json::json!({
                    "operation": "adapter_unavailable",
                    "continuation_id": identity.continuation_id,
                    "job_id": identity.job_id,
                })
                .to_string(),
                duplicate_identity: format!("continuation-waiting-write-{operation_id}"),
            },
            receipt: AppendReceiptCommand {
                receipt_id: format!("continuation-waiting-receipt-{operation_id}"),
                transaction_id: format!("continuation-waiting-transaction-{operation_id}"),
                state_root_generation: identity.state_root_generation,
                delegation_id: identity.delegation_id.clone(),
                expected_state_revision: context.delegation_revision,
                expected_global_count: context.global_receipt_count,
                expected_global_head_digest: context.global_receipt_head_digest,
                expected_delegation_count: context.delegation_receipt_count,
                expected_delegation_head_digest: context.delegation_receipt_head_digest,
                receipt_kind: ReceiptKind::Continuation,
                subject: ReceiptSubject {
                    kind: ReceiptSubjectKind::Continuation,
                    subject_id: identity.continuation_id.clone(),
                    revision: context.delegation_revision,
                },
                causation_id: format!("continuation-waiting-cause-{operation_id}"),
                causation_event_id: event_id,
                actor: ReceiptActorBinding {
                    actor_type: StorageActorType::Runtime,
                    actor_identity: SafeText::new(&identity.runtime_actor_identity, &[])?,
                    authority_provenance_id: identity.runtime_authority_provenance_id.clone(),
                },
                runtime: ReceiptRuntimeBinding {
                    host_generation: identity.runtime_host_generation,
                    host_instance_id: identity.runtime_host_instance_id.clone(),
                    fencing_token: identity.runtime_fencing_token,
                },
                key: self.receipt_key.clone(),
                rotation: None,
                evidence: vec![],
                redacted_summary: self
                    .receipt_redactor
                    .summary("ExecAss continuation waiting for orchestration adapter")?,
                occurred_at: trusted_now,
                committed_at: trusted_now,
            },
        };
        self.store.settle_continuation_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
        )
    }

    #[cfg(test)]
    pub(super) fn open_for_test(store: ExecAssStore, seed: [u8; 32]) -> Result<Self> {
        let identity =
            carsinos_storage::execass::activate_test_confirmation_authority(&store, seed)?;
        let signer = FixedConfirmationAuthoritySigner::from_seed(&identity, Zeroizing::new(seed))?;
        let (receipt_integrity, receipt_key, receipt_redactor) =
            open_receipt_runtime_for_test(&store)?;
        Ok(Self {
            store,
            identity,
            signer,
            receipt_integrity,
            receipt_key,
            receipt_redactor,
            global_control_lock: Mutex::new(()),
            recorder_client: None,
            recorder_binding: None,
        })
    }

    #[cfg(test)]
    pub(super) fn attach_test_recorder_client(
        &mut self,
        client: RecorderClient,
        binding: RecorderBindingV1,
        identity: &carsinos_effect_recorder::RecorderIdentity,
        trusted_now: i64,
    ) -> Result<()> {
        if self.recorder_client.is_some() || self.recorder_binding.is_some() {
            bail!("test recorder client is already attached");
        }
        identity.validate_for_root(&binding.canonical_root_identity)?;
        self.store.pin_or_validate_recorder_identity(
            &RecorderAuthorityIdentity {
                recorder_key_id: identity.key_id.clone(),
                key_generation: identity.key_generation,
                verifying_key_hex: identity.verifying_key_hex.clone(),
                verifying_key_digest: identity.verifying_key_digest.clone(),
                canonical_root_identity: binding.canonical_root_identity.clone(),
                installation_identity: binding.installation_id.clone(),
                os_user_identity_digest: binding.os_user_identity_digest.clone(),
                state_root_generation: binding.state_root_generation,
            },
            trusted_now,
        )?;
        self.recorder_client = Some(client);
        self.recorder_binding = Some(binding);
        Ok(())
    }

    /// Resolve one already-authenticated choice. The caller may name only the
    /// decision and disclosed logical action; grant identity and all signed
    /// action material are storage/runtime-owned.
    pub(super) fn resolve(
        &self,
        event: &VerifiedConfirmationEvent,
        decision_id: &str,
        selected_logical_action_id: &str,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        let projection = self
            .store
            .read_danger_confirmation_runtime_projection(decision_id, selected_logical_action_id)?
            .context("confirmation decision/action is unavailable")?;
        match projection {
            DangerConfirmationRuntimeProjection::Resolved(resolved) => {
                let resolved = *resolved;
                require_exact_event_binding(event, &resolved.binding)?;
                Ok(DangerConfirmationResolutionOutcome::Replayed(
                    resolved.grant,
                ))
            }
            DangerConfirmationRuntimeProjection::Pending(binding) => {
                let binding = *binding;
                require_exact_event_binding(event, &binding)?;
                let payload = payload_from_storage(&self.identity, event, &binding)?;
                let attestation = self
                    .signer
                    .sign_confirmation_attestation(&self.identity, &payload)?;
                let trusted_now = i64::try_from(payload.issued_at_ms)
                    .context("confirmation issuance exceeds storage clock")?;
                let context = self
                    .store
                    .read_decision_receipt_context(&binding.decision_id, trusted_now)?
                    .context("confirmation decision has no active receipt/runtime context")?;
                let resolution_identity = atomic_resolution_identity(&binding);
                let exact_effect = self.store.prepare_exact_dangerous_effect(
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                    trusted_now,
                    context.runtime_host_generation,
                    context.global_stop_epoch,
                )?;
                let continuation = if let Some(prepared) = &exact_effect {
                    Some(prepared.continuation.clone())
                } else {
                    self.store
                        .read_waiting_decision_action(
                            &binding.decision_id,
                            &binding.selected_logical_action_id,
                        )?
                        .map(|action| ContinuationRecord {
                            continuation_id: format!("decision-continuation-{resolution_identity}"),
                            delegation_id: action.delegation_id,
                            target_delegation_revision: action.target_delegation_revision,
                            target_plan_revision: action.target_plan_revision,
                            action_id: action.action_id,
                            branch_kind: action.branch_kind,
                            causation_kind: ContinuationCausationKind::Decision,
                            causation_id: binding.decision_id.clone(),
                            status: ContinuationStatus::Runnable,
                            job_id: None,
                            lease_owner: None,
                            lease_expires_at: None,
                            fencing_token: 0,
                            host_generation: context.runtime_host_generation,
                            stop_epoch: action.stop_epoch,
                            global_stop_epoch: context.global_stop_epoch,
                            created_at: trusted_now,
                            updated_at: trusted_now,
                            completed_at: None,
                        })
                };
                let event_id = format!("decision-event-{resolution_identity}");
                let resolution = AtomicDecisionResolutionCommand {
                    write: WriteContext {
                        idempotency_key: format!("decision-write-{resolution_identity}"),
                        correlation_id: event.request_correlation_id().to_string(),
                        causation_id: binding.decision_id.clone(),
                        occurred_at: trusted_now,
                    },
                    decision_id: binding.decision_id.clone(),
                    decision_revision: binding.decision_revision,
                    result: StorageDecisionResult::ConfirmAndContinue,
                    selected_logical_action_id: Some(binding.selected_logical_action_id.clone()),
                    continuation,
                    logical_effect: exact_effect
                        .as_ref()
                        .map(|prepared| prepared.logical_effect.clone()),
                    technical_quota_snapshot: exact_effect
                        .as_ref()
                        .map(|prepared| prepared.technical_quota_snapshot.clone()),
                    technical_resource_requirements: exact_effect
                        .as_ref()
                        .map(|prepared| prepared.technical_resource_requirements.clone()),
                    outbox_event: NewOutboxEvent {
                        event_id: event_id.clone(),
                        event_name: OutboxEventName::DecisionRecorded,
                        aggregate_id: context.delegation_id.clone(),
                        aggregate_revision: context.delegation_revision,
                        correlation_id: event.request_correlation_id().to_string(),
                        causation_id: binding.decision_id.clone(),
                        occurred_at: trusted_now,
                        safe_payload_json: serde_json::json!({
                            "decision_id": binding.decision_id,
                            "decision_revision": binding.decision_revision,
                            "result": "confirm_and_continue",
                            "selected_logical_action_id": binding.selected_logical_action_id,
                        })
                        .to_string(),
                        duplicate_identity: format!("decision-write-{resolution_identity}"),
                    },
                    receipt: AppendReceiptCommand {
                        receipt_id: format!("decision-receipt-{resolution_identity}"),
                        transaction_id: format!("decision-anchor-{resolution_identity}"),
                        state_root_generation: context.state_root_generation,
                        delegation_id: context.delegation_id,
                        expected_state_revision: context.delegation_revision,
                        expected_global_count: context.global_receipt_count,
                        expected_global_head_digest: context.global_receipt_head_digest,
                        expected_delegation_count: context.delegation_receipt_count,
                        expected_delegation_head_digest: context.delegation_receipt_head_digest,
                        receipt_kind: ReceiptKind::Decision,
                        subject: ReceiptSubject {
                            kind: ReceiptSubjectKind::Decision,
                            subject_id: binding.decision_id.clone(),
                            revision: binding.decision_revision,
                        },
                        causation_id: binding.decision_id.clone(),
                        causation_event_id: event_id,
                        actor: ReceiptActorBinding {
                            actor_type: carsinos_storage::execass::ActorType::HumanLocal,
                            actor_identity: SafeText::new("storage-derived-owner", &[])?,
                            authority_provenance_id: "storage-derived-owner".to_string(),
                        },
                        runtime: ReceiptRuntimeBinding {
                            host_generation: context.runtime_host_generation,
                            host_instance_id: context.runtime_host_instance_id,
                            fencing_token: context.runtime_fencing_token,
                        },
                        key: self.receipt_key.clone(),
                        rotation: None,
                        evidence: Vec::new(),
                        redacted_summary: SafeText::new(
                            "owner confirmed the exact dangerous action",
                            &[],
                        )?,
                        occurred_at: trusted_now,
                        committed_at: trusted_now,
                    },
                };
                let grant_id = format!("confirmation-grant-{resolution_identity}");
                match self.store.confirm_dangerous_action_attested_atomically(
                    &self.receipt_integrity,
                    &self.receipt_redactor,
                    &resolution,
                    &grant_id,
                    &attestation,
                ) {
                    Ok(AtomicDecisionResolutionOutcome::Applied(bundle)) => {
                        Ok(DangerConfirmationResolutionOutcome::Confirmed(
                            bundle
                                .confirmation_grant
                                .context("atomic confirmation omitted its grant")?,
                        ))
                    }
                    Ok(AtomicDecisionResolutionOutcome::Replayed(bundle)) => {
                        Ok(DangerConfirmationResolutionOutcome::Replayed(
                            bundle
                                .confirmation_grant
                                .context("atomic confirmation replay omitted its grant")?,
                        ))
                    }
                    Ok(other) => bail!("atomic confirmation did not resolve: {other:?}"),
                    Err(first_error) => {
                        self.resolve_exact_race_replay(event, &binding, first_error)
                    }
                }
            }
        }
    }

    /// Route one verified owner result through its canonical storage authority.
    /// Affirmative dangerous confirmation retains the fixed-key attestation and
    /// grant path. Ordinary affirmative decisions create exactly one
    /// server-derived continuation. Affirmative duplicate-risk retry delegates
    /// its fresh effect construction to storage from the frozen predecessor.
    /// Non-affirmative results can never create a continuation, effect, or grant.
    pub(super) fn resolve_typed(
        &self,
        event: &VerifiedConfirmationEvent,
        decision_id: &str,
        selected_logical_action_id: &str,
    ) -> Result<TypedDecisionResolutionOutcome> {
        let binding = self
            .store
            .read_decision_resolution_binding(decision_id, selected_logical_action_id)?
            .context("decision/action resolution binding is unavailable")?;
        match (binding.decision_kind, event.verified_decision_result()) {
            (
                StorageDecisionKind::DangerousActionConfirmation,
                carsinos_protocol::execass::DecisionResult::ConfirmAndContinue,
            ) => {
                self.resolve(event, decision_id, selected_logical_action_id)?;
                return Ok(TypedDecisionResolutionOutcome::Confirmed);
            }
            (
                StorageDecisionKind::DangerousActionConfirmation,
                carsinos_protocol::execass::DecisionResult::Stop,
            ) => bail!("a dangerous confirmation cannot be resolved as a typed stop"),
            _ => {}
        }
        require_exact_typed_event_binding(event, &binding)?;
        let request = event
            .verified_request_binding()
            .context("verified decision event omitted its exact request binding")?;
        if request.idempotency_key.trim().is_empty() {
            bail!("verified decision request omitted idempotency");
        }
        let actor = event
            .owner_actor_assurance()
            .context("verified decision event omitted owner actor assurance")?;
        let decision_revision = i64::try_from(event.verified_decision_binding().decision_revision)
            .context("verified decision revision exceeds storage range")?;
        let authority = bind_decision_resolution_owner_authority(
            actor,
            DecisionResolutionAuthoritySource {
                normalized_intent: binding.normalized_intent.clone(),
                canonical_manifest_json: binding.canonical_manifest_json.clone(),
                canonical_manifest_digest: binding.manifest_digest.clone(),
                decision_id: binding.decision_id.clone(),
                decision_revision,
                policy_revision: binding.policy_revision,
                selected_logical_action_id: binding.selected_logical_action_id.clone(),
                decision_result: event.verified_decision_result(),
                request_correlation_id: event.request_correlation_id().to_string(),
                idempotency_key: request.idempotency_key.clone(),
                revision_text_digest: request.revision_text_digest.clone(),
                challenge_response_digest: request.challenge_response_digest.clone(),
                challenge_nonce_digest: binding.challenge_nonce_digest.clone(),
                created_at_ms: request.observed_at_ms,
                expires_at_ms: binding.expires_at,
            },
        )
        .map_err(|_| anyhow::anyhow!("verified decision authority binding failed"))?;
        let context = self
            .store
            .read_decision_receipt_context(&binding.decision_id, request.observed_at_ms)?
            .context("decision has no active receipt/runtime context")?;
        let storage_result = storage_decision_result(event.verified_decision_result())?;
        let resolution_identity = nonaffirmative_resolution_identity(event, request);
        let expects_duplicate_successor = binding.decision_kind
            == StorageDecisionKind::DuplicateRiskRetry
            && storage_result == StorageDecisionResult::ConfirmAndContinue;
        let duplicate_successor = if expects_duplicate_successor {
            Some(
                self.store
                    .prepare_duplicate_risk_successor(
                        &binding.decision_id,
                        &binding.selected_logical_action_id,
                        &resolution_identity,
                        request.observed_at_ms,
                        context.runtime_host_generation,
                        context.global_stop_epoch,
                    )?
                    .context("duplicate-risk decision has no exact unresolved predecessor")?,
            )
        } else {
            None
        };
        let continuation = if let Some(prepared) = &duplicate_successor {
            Some(prepared.continuation.clone())
        } else if storage_result == StorageDecisionResult::ConfirmAndContinue {
            match self.store.read_waiting_decision_action(
                &binding.decision_id,
                &binding.selected_logical_action_id,
            )? {
                Some(action) => {
                    let continuation_identity = sha256_hex(
                        serde_json::json!({
                            "domain": "carsinos.execass.ordinary-decision-continuation.v1",
                            "resolution_identity": resolution_identity.as_str(),
                            "decision_id": binding.decision_id.as_str(),
                            "decision_revision": binding.decision_revision,
                            "delegation_id": context.delegation_id.as_str(),
                            "delegation_revision": context.delegation_revision,
                            "plan_revision": context.plan_revision,
                            "action_id": action.action_id.as_str(),
                            "action_revision": action.action_revision,
                            "branch_kind": match action.branch_kind {
                                carsinos_storage::execass::ActionBranchKind::Ordinary => "ordinary",
                                carsinos_storage::execass::ActionBranchKind::Recovery => "recovery",
                            },
                            "stop_epoch": action.stop_epoch,
                            "global_stop_epoch": context.global_stop_epoch,
                            "runtime_host_generation": context.runtime_host_generation,
                        })
                        .to_string()
                        .as_bytes(),
                    );
                    Some(ContinuationRecord {
                        continuation_id: format!("decision-continuation-{continuation_identity}"),
                        delegation_id: action.delegation_id,
                        target_delegation_revision: action.target_delegation_revision,
                        target_plan_revision: action.target_plan_revision,
                        action_id: action.action_id,
                        branch_kind: action.branch_kind,
                        causation_kind: ContinuationCausationKind::Decision,
                        causation_id: binding.decision_id.clone(),
                        status: ContinuationStatus::Runnable,
                        job_id: None,
                        lease_owner: None,
                        lease_expires_at: None,
                        fencing_token: 0,
                        host_generation: context.runtime_host_generation,
                        stop_epoch: action.stop_epoch,
                        global_stop_epoch: context.global_stop_epoch,
                        created_at: request.observed_at_ms,
                        updated_at: request.observed_at_ms,
                        completed_at: None,
                    })
                }
                None => {
                    let detail = self
                        .store
                        .read_api_delegation_detail(&binding.delegation_id)?
                        .context("ordinary decision continuation target is unavailable")?;
                    let mut matches = detail.continuations.into_iter().filter(|candidate| {
                        candidate.causation_kind == ContinuationCausationKind::Decision
                            && candidate.causation_id == binding.decision_id
                            && candidate.action_id == binding.selected_logical_action_id
                    });
                    let existing = matches
                        .next()
                        .context("ordinary decision has no exact waiting action or replay")?;
                    if matches.next().is_some() {
                        bail!("ordinary decision has multiple persisted continuations");
                    }
                    Some(existing)
                }
            }
        } else {
            None
        };
        let write_id = format!("decision-write-{resolution_identity}");
        let event_id = format!("decision-event-{resolution_identity}");
        let (actor_type, credential_identity, _, _, _, _) =
            crate::execass_intake::authority_storage_evidence(&authority);
        let command = AtomicDecisionResolutionCommand {
            write: WriteContext {
                idempotency_key: write_id.clone(),
                correlation_id: event.request_correlation_id().to_string(),
                causation_id: binding.decision_id.clone(),
                occurred_at: request.observed_at_ms,
            },
            decision_id: binding.decision_id.clone(),
            decision_revision: binding.decision_revision,
            result: storage_result,
            selected_logical_action_id: (binding.decision_kind
                == StorageDecisionKind::DangerousActionConfirmation
                || storage_result == StorageDecisionResult::ConfirmAndContinue)
                .then(|| {
                    duplicate_successor
                        .as_ref()
                        .map(|prepared| prepared.continuation.action_id.clone())
                        .unwrap_or_else(|| binding.selected_logical_action_id.clone())
                }),
            continuation,
            logical_effect: duplicate_successor
                .as_ref()
                .map(|prepared| prepared.logical_effect.clone()),
            technical_quota_snapshot: duplicate_successor
                .as_ref()
                .map(|prepared| prepared.technical_quota_snapshot.clone()),
            technical_resource_requirements: duplicate_successor
                .as_ref()
                .map(|prepared| prepared.technical_resource_requirements.clone()),
            outbox_event: NewOutboxEvent {
                event_id: event_id.clone(),
                event_name: OutboxEventName::DecisionRecorded,
                aggregate_id: context.delegation_id.clone(),
                aggregate_revision: context.delegation_revision,
                correlation_id: event.request_correlation_id().to_string(),
                causation_id: binding.decision_id.clone(),
                occurred_at: request.observed_at_ms,
                safe_payload_json: serde_json::json!({
                    "decision_id": binding.decision_id,
                    "decision_revision": binding.decision_revision,
                    "result": decision_result_name(event.verified_decision_result()),
                    "selected_logical_action_id": binding.selected_logical_action_id,
                })
                .to_string(),
                duplicate_identity: write_id,
            },
            receipt: AppendReceiptCommand {
                receipt_id: format!("decision-receipt-{resolution_identity}"),
                transaction_id: format!("decision-anchor-{resolution_identity}"),
                state_root_generation: context.state_root_generation,
                delegation_id: context.delegation_id,
                expected_state_revision: context.delegation_revision,
                expected_global_count: context.global_receipt_count,
                expected_global_head_digest: context.global_receipt_head_digest,
                expected_delegation_count: context.delegation_receipt_count,
                expected_delegation_head_digest: context.delegation_receipt_head_digest,
                receipt_kind: ReceiptKind::Decision,
                subject: ReceiptSubject {
                    kind: ReceiptSubjectKind::Decision,
                    subject_id: binding.decision_id.clone(),
                    revision: binding.decision_revision,
                },
                causation_id: binding.decision_id,
                causation_event_id: event_id,
                actor: ReceiptActorBinding {
                    actor_type,
                    actor_identity: SafeText::new(&credential_identity, &[])?,
                    authority_provenance_id: authority.authority_provenance_id().to_string(),
                },
                runtime: ReceiptRuntimeBinding {
                    host_generation: context.runtime_host_generation,
                    host_instance_id: context.runtime_host_instance_id,
                    fencing_token: context.runtime_fencing_token,
                },
                key: self.receipt_key.clone(),
                rotation: None,
                evidence: Vec::new(),
                redacted_summary: self.receipt_redactor.summary(
                    match (binding.decision_kind, storage_result) {
                        (
                            StorageDecisionKind::DuplicateRiskRetry,
                            StorageDecisionResult::ConfirmAndContinue,
                        ) => {
                            "owner confirmed a duplicate-risk retry with one fresh effect identity"
                        }
                        (_, StorageDecisionResult::ConfirmAndContinue) => {
                            "owner confirmed an ordinary decision and continued its exact action"
                        }
                        _ => "owner recorded a non-affirmative decision result",
                    },
                )?,
                occurred_at: request.observed_at_ms,
                committed_at: request.observed_at_ms,
            },
        };
        match self.store.resolve_decision_atomically(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
            &authority,
        )? {
            AtomicDecisionResolutionOutcome::Applied(bundle) => {
                if bundle.confirmation_grant.is_some()
                    || expects_duplicate_successor != bundle.logical_effect.is_some()
                {
                    bail!("typed decision returned invalid grant or effect material");
                }
                if (storage_result == StorageDecisionResult::ConfirmAndContinue)
                    != bundle.continuation.is_some()
                {
                    bail!("typed decision returned an invalid continuation shape");
                }
                Ok(TypedDecisionResolutionOutcome::Applied)
            }
            AtomicDecisionResolutionOutcome::Replayed(bundle) => {
                if bundle.confirmation_grant.is_some()
                    || expects_duplicate_successor != bundle.logical_effect.is_some()
                {
                    bail!("typed decision replay returned invalid grant or effect material");
                }
                if (storage_result == StorageDecisionResult::ConfirmAndContinue)
                    != bundle.continuation.is_some()
                {
                    bail!("typed decision replay returned an invalid continuation shape");
                }
                Ok(TypedDecisionResolutionOutcome::Replayed)
            }
            other => bail!("atomic decision did not resolve: {other:?}"),
        }
    }

    /// Supplies fixed receipt custody and live runtime fencing for one already
    /// verified follow-up. Callers provide no key or redactor material.
    pub(super) fn apply_verified_follow_up_amendment(
        &self,
        amendment: AmendLifecycleCommand,
        authority: &carsinos_core::execass_actor::VerifiedOwnerAuthority,
        manifest: &carsinos_core::execass_manifest::CanonicalLeafManifest,
        danger_admission: &SignedDangerAdmissionProof,
        trusted_now: i64,
    ) -> Result<VerifiedFollowUpAmendmentOutcome> {
        let Some(status) = self
            .store
            .read_delegation_run_control_status(&amendment.delegation_id, trusted_now)?
        else {
            return Ok(VerifiedFollowUpAmendmentOutcome::NotFound);
        };
        let runtime = status
            .runtime
            .context("follow-up amendment has no active receipt/runtime context")?;
        let (actor_type, credential_identity, _, _, _, _) =
            crate::execass_intake::authority_storage_evidence(authority);
        let command = ApplyVerifiedFollowUpAmendmentCommand {
            receipt: AppendReceiptCommand {
                receipt_id: format!("{}-receipt", amendment.amendment_id),
                transaction_id: format!("{}-transaction", amendment.amendment_id),
                state_root_generation: runtime.state_root_generation,
                delegation_id: amendment.delegation_id.clone(),
                expected_state_revision: amendment.expected_state_revision + 1,
                expected_global_count: status.global_receipt_count,
                expected_global_head_digest: status.global_receipt_head_digest,
                expected_delegation_count: status.delegation_receipt_count,
                expected_delegation_head_digest: status.delegation_receipt_head_digest,
                receipt_kind: ReceiptKind::Amendment,
                subject: ReceiptSubject {
                    kind: ReceiptSubjectKind::PlanAmendment,
                    subject_id: amendment.amendment_id.clone(),
                    revision: amendment.amendment_revision,
                },
                causation_id: amendment.write.causation_id.clone(),
                causation_event_id: amendment.outbox_event.event_id.clone(),
                actor: ReceiptActorBinding {
                    actor_type,
                    actor_identity: SafeText::new(&credential_identity, &[])?,
                    authority_provenance_id: amendment.authority_provenance_id.clone(),
                },
                runtime: ReceiptRuntimeBinding {
                    host_generation: runtime.runtime_host_generation,
                    host_instance_id: runtime.runtime_host_instance_id,
                    fencing_token: runtime.runtime_fencing_token,
                },
                key: self.receipt_key.clone(),
                rotation: None,
                evidence: vec![],
                redacted_summary: self
                    .receipt_redactor
                    .summary("verified owner amendment admitted for replacement planning")?,
                occurred_at: amendment.write.occurred_at,
                committed_at: amendment.write.occurred_at,
            },
            amendment,
        };
        self.store.apply_verified_follow_up_amendment(
            &self.receipt_integrity,
            &self.receipt_redactor,
            &command,
            authority,
            manifest,
            danger_admission,
        )
    }

    /// Seal the complete server-routed manifest result with the same fixed
    /// current-user authority that storage independently pins. This adds no
    /// user prompt: it only prevents an internal caller from bypassing the
    /// ordinary-vs-one-confirmation routing boundary.
    pub(super) fn seal_danger_admission(
        &self,
        proof: DangerAdmissionProof,
    ) -> Result<SignedDangerAdmissionProof> {
        self.signer.sign_danger_admission(&self.identity, proof)
    }

    fn resolve_exact_race_replay(
        &self,
        event: &VerifiedConfirmationEvent,
        binding: &PendingDangerConfirmationAlternativeBinding,
        first_error: anyhow::Error,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        match self.store.read_danger_confirmation_runtime_projection(
            &binding.decision_id,
            &binding.selected_logical_action_id,
        )? {
            Some(DangerConfirmationRuntimeProjection::Resolved(resolved)) => {
                let resolved = *resolved;
                require_exact_event_binding(event, &resolved.binding)?;
                Ok(DangerConfirmationResolutionOutcome::Replayed(
                    resolved.grant,
                ))
            }
            _ => Err(first_error),
        }
    }
}

fn open_receipt_runtime(
    store: &ExecAssStore,
) -> Result<(ReceiptIntegrityStore, ReceiptKeyRef, ReceiptRedactor)> {
    let integrity = store
        .open_receipt_integrity_store()
        .context("failed opening fixed ExecAss receipt integrity store")?;
    finish_open_receipt_runtime(integrity)
}

#[cfg(test)]
fn open_receipt_runtime_for_test(
    store: &ExecAssStore,
) -> Result<(ReceiptIntegrityStore, ReceiptKeyRef, ReceiptRedactor)> {
    let integrity = store
        .open_receipt_integrity_store_for_test()
        .context("failed opening test-only ExecAss receipt integrity store")?;
    finish_open_receipt_runtime(integrity)
}

fn finish_open_receipt_runtime(
    integrity: ReceiptIntegrityStore,
) -> Result<(ReceiptIntegrityStore, ReceiptKeyRef, ReceiptRedactor)> {
    integrity
        .recover_integrity()
        .context("failed recovering ExecAss receipt integrity before confirmation startup")?;
    let key = match integrity.current_append_key()? {
        Some(key) => key,
        None => integrity
            .provision_initial_key("execass-receipt-key-v1")
            .context("failed provisioning initial ExecAss receipt key")?,
    };
    let redactor = ReceiptRedactor::new(&["carsinos-execass-receipt-redaction-sentinel"])?;
    Ok((integrity, key, redactor))
}

fn require_global_control_event(
    event: &VerifiedRunControlEvent,
) -> Result<ActorRunControlOperation> {
    if event.actor_type() != ActorType::HumanLocal && event.actor_type() != ActorType::HumanRemote {
        bail!("global control requires opaque verified human evidence");
    }
    if !matches!(event.target(), ActorRunControlTarget::Global) {
        bail!("global control coordinator rejects a delegation target");
    }
    match event.operation() {
        ActorRunControlOperation::GlobalStop | ActorRunControlOperation::GlobalResume => {
            Ok(event.operation())
        }
        ActorRunControlOperation::DelegationStop | ActorRunControlOperation::DelegationResume => {
            bail!("global control coordinator rejects a delegation operation")
        }
    }
}

fn require_delegation_control_event(
    event: &VerifiedRunControlEvent,
    delegation_id: &str,
) -> Result<ActorRunControlOperation> {
    if event.actor_type() != ActorType::HumanLocal && event.actor_type() != ActorType::HumanRemote {
        bail!("delegation control requires opaque verified human evidence");
    }
    match event.target() {
        ActorRunControlTarget::Delegation {
            delegation_id: target,
        } if target == delegation_id => {}
        _ => bail!("delegation control target does not match the route delegation"),
    }
    match event.operation() {
        ActorRunControlOperation::DelegationStop | ActorRunControlOperation::DelegationResume => {
            Ok(event.operation())
        }
        ActorRunControlOperation::GlobalStop | ActorRunControlOperation::GlobalResume => {
            bail!("delegation control coordinator rejects a global operation")
        }
    }
}

fn delegation_control_operation_identity(event: &VerifiedRunControlEvent) -> String {
    sha256_hex(
        format!(
            "carsinos.execass.delegation-control-operation.v1\0{}\0{}",
            event.replay_identity(),
            event.request_binding_digest(),
        )
        .as_bytes(),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_delegation_control_outbox(
    delegation_id: &str,
    next_revision: i64,
    correlation_id: &str,
    idempotency_key: &str,
    trusted_now: i64,
    operation_id: &str,
    causation_id: &str,
    safe_payload_json: String,
) -> NewOutboxEvent {
    NewOutboxEvent {
        event_id: format!("delegation-control-event-{operation_id}"),
        event_name: OutboxEventName::DelegationTransitioned,
        aggregate_id: delegation_id.to_string(),
        aggregate_revision: next_revision,
        correlation_id: correlation_id.to_string(),
        causation_id: causation_id.to_string(),
        occurred_at: trusted_now,
        safe_payload_json,
        duplicate_identity: idempotency_key.to_string(),
    }
}

fn delegation_control_payload(
    status: &DelegationRunControlStatus,
    operation: &str,
    run_control: StorageRunControlState,
    state_revision: i64,
    stop_epoch: i64,
    drain_state: DelegationStopDrainState,
) -> String {
    serde_json::json!({
        "operation": operation,
        "delegation_id": status.delegation_id,
        "phase": storage_delegation_phase_name(status.phase),
        "run_control": storage_run_control_name(run_control),
        "state_revision": state_revision,
        "current_plan_revision": status.current_plan_revision,
        "stop_epoch": stop_epoch,
        "policy_revision": status.policy_revision,
        "drain_state": delegation_drain_state_name(drain_state),
        "unresolved_external_effects_digest": status.unresolved_external_effects_digest,
    })
    .to_string()
}

fn storage_delegation_phase_name(
    phase: carsinos_storage::execass::DelegationPhase,
) -> &'static str {
    match phase {
        carsinos_storage::execass::DelegationPhase::Accepted => "accepted",
        carsinos_storage::execass::DelegationPhase::Planning => "planning",
        carsinos_storage::execass::DelegationPhase::InMotion => "in_motion",
        carsinos_storage::execass::DelegationPhase::WaitingForUser => "waiting_for_user",
        carsinos_storage::execass::DelegationPhase::WaitingExternal => "waiting_external",
        carsinos_storage::execass::DelegationPhase::Recovering => "recovering",
        carsinos_storage::execass::DelegationPhase::Completed => "completed",
        carsinos_storage::execass::DelegationPhase::PartiallyCompleted => "partially_completed",
        carsinos_storage::execass::DelegationPhase::Failed => "failed",
    }
}

fn storage_run_control_name(run_control: StorageRunControlState) -> &'static str {
    match run_control {
        StorageRunControlState::Running => "running",
        StorageRunControlState::StopRequested => "stop_requested",
        StorageRunControlState::Stopped => "stopped",
    }
}

fn delegation_drain_state_name(drain_state: DelegationStopDrainState) -> &'static str {
    match drain_state {
        DelegationStopDrainState::Running => "running",
        DelegationStopDrainState::Draining => "draining",
        DelegationStopDrainState::ReadyToStop => "ready_to_stop",
        DelegationStopDrainState::Stopped => "stopped",
    }
}

fn global_control_operation_identity(event: &VerifiedRunControlEvent) -> String {
    sha256_hex(
        format!(
            "carsinos.execass.global-control-operation.v1\0{}\0{}",
            event.replay_identity(),
            event.request_binding_digest(),
        )
        .as_bytes(),
    )
}

fn build_global_control_outbox(
    event: &VerifiedRunControlEvent,
    operation_id: &str,
    causation_id: &str,
    status: &carsinos_storage::execass::GlobalStopStatus,
    operation: &str,
) -> NewOutboxEvent {
    NewOutboxEvent {
        event_id: format!("global-control-event-{operation_id}"),
        event_name: OutboxEventName::GlobalStopChanged,
        aggregate_id: "global-stop-all".to_string(),
        aggregate_revision: status.global_stop_epoch,
        correlation_id: event.request_correlation_id().to_string(),
        causation_id: causation_id.to_string(),
        occurred_at: event.observed_at_ms(),
        safe_payload_json: global_control_payload(status, operation),
        duplicate_identity: event.idempotency_key().to_string(),
    }
}

fn global_control_payload(status: &GlobalStopStatus, operation: &str) -> String {
    serde_json::json!({
        "operation": operation,
        "engaged": status.engaged,
        "global_stop_epoch": status.global_stop_epoch,
        "drain_state": global_stop_drain_state_name(status.drain_state),
        "current_policy_revision": status.current_policy_revision,
        "unresolved_external_effects_digest": status.unresolved_external_effects_digest,
    })
    .to_string()
}

fn is_stop_drain_disclosure_mismatch(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.to_string() == "global stop outbox payload is not the deterministic status disclosure"
    })
}

fn global_stop_drain_state_name(state: GlobalStopDrainState) -> &'static str {
    match state {
        GlobalStopDrainState::Running => "running",
        GlobalStopDrainState::Draining => "draining",
        GlobalStopDrainState::Drained => "drained",
    }
}

fn atomic_resolution_identity(binding: &PendingDangerConfirmationAlternativeBinding) -> String {
    sha256_hex(
        format!(
            "carsinos.execass.decision-resolution.v1\0{}\0{}\0{}",
            binding.decision_id, binding.decision_revision, binding.selected_logical_action_id,
        )
        .as_bytes(),
    )
}

fn nonaffirmative_resolution_identity(
    event: &VerifiedConfirmationEvent,
    request: &LocalDecisionProofBinding,
) -> String {
    sha256_hex(
        serde_json::json!({
            "decision_id": request.decision_id,
            "decision_revision": request.decision_revision,
            "decision_result": decision_result_name(event.verified_decision_result()),
            "selected_logical_action_id": request.response_selected_logical_action_id,
            "request_correlation_id": event.request_correlation_id(),
            "idempotency_key": request.idempotency_key,
            "revision_text_digest": request.revision_text_digest,
            "challenge_response_digest": request.challenge_response_digest,
        })
        .to_string()
        .as_bytes(),
    )
}

fn decision_result_name(value: carsinos_protocol::execass::DecisionResult) -> &'static str {
    match value {
        carsinos_protocol::execass::DecisionResult::ConfirmAndContinue => "confirm_and_continue",
        carsinos_protocol::execass::DecisionResult::Revise => "revise",
        carsinos_protocol::execass::DecisionResult::Decline => "decline",
        carsinos_protocol::execass::DecisionResult::Stop => "stop",
    }
}

fn storage_decision_result(
    value: carsinos_protocol::execass::DecisionResult,
) -> Result<StorageDecisionResult> {
    match value {
        carsinos_protocol::execass::DecisionResult::Revise => Ok(StorageDecisionResult::Revise),
        carsinos_protocol::execass::DecisionResult::Decline => Ok(StorageDecisionResult::Decline),
        carsinos_protocol::execass::DecisionResult::Stop => Ok(StorageDecisionResult::Stop),
        carsinos_protocol::execass::DecisionResult::ConfirmAndContinue => {
            Ok(StorageDecisionResult::ConfirmAndContinue)
        }
    }
}

/// Gateway-private view of the fixed OS-custodied key. Storage exposes only
/// activation/public pinning and attestation verification; no production crate
/// API can obtain a signer or invoke this operation.
struct FixedConfirmationAuthoritySigner {
    identity: ConfirmationAuthorityIdentity,
    seed: Zeroizing<[u8; 32]>,
}

impl fmt::Debug for FixedConfirmationAuthoritySigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FixedConfirmationAuthoritySigner")
            .field("identity", &self.identity)
            .field("secret_material", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl FixedConfirmationAuthoritySigner {
    fn open(identity: &ConfirmationAuthorityIdentity) -> Result<Self> {
        let mut locator = Sha256::new();
        locator.update(CUSTODY_DOMAIN);
        locator.update([0]);
        locator.update(identity.canonical_root_identity().as_bytes());
        let account = format!("root-{}", hex_encode(&locator.finalize()));
        let encoded = Zeroizing::new(
            keyring::Entry::new(CUSTODY_SERVICE, &account)
                .context("failed opening fixed confirmation OS custody entry")?
                .get_password()
                .context("failed reading fixed confirmation OS custody entry")?,
        );
        let seed = parse_custody_seed(identity, &encoded)?;
        Self::from_seed(identity, seed)
    }

    fn from_seed(
        identity: &ConfirmationAuthorityIdentity,
        seed: Zeroizing<[u8; 32]>,
    ) -> Result<Self> {
        let verifying_key_hex =
            hex_encode(SigningKey::from_bytes(&seed).verifying_key().as_bytes());
        if verifying_key_hex != identity.verifying_key_hex() {
            bail!("fixed confirmation OS credential does not match its public pin");
        }
        Ok(Self {
            identity: identity.clone(),
            seed,
        })
    }

    fn identity(&self) -> &ConfirmationAuthorityIdentity {
        &self.identity
    }

    fn sign_confirmation_attestation(
        &self,
        identity: &ConfirmationAuthorityIdentity,
        payload: &ConfirmationAttestationPayload,
    ) -> Result<carsinos_storage::execass::ConfirmationAttestation> {
        if identity != &self.identity
            || payload.canonical_root_identity != identity.canonical_root_identity()
            || payload.installation_identity != identity.installation_identity()
            || payload.os_user_identity_digest != identity.os_user_identity_digest()
            || payload.state_root_generation != identity.state_root_generation()
            || payload.signer_key_generation != identity.key_generation()
        {
            bail!("confirmation payload does not match the fixed custody identity");
        }
        let bytes = carsinos_storage::execass::confirmation_attestation_signing_bytes(
            payload,
            identity.key_id(),
        )
        .map_err(|_| anyhow::anyhow!("confirmation attestation payload is invalid"))?;
        let signature = SigningKey::from_bytes(&self.seed).sign(&bytes);
        Ok(carsinos_storage::execass::ConfirmationAttestation {
            payload: payload.clone(),
            key_id: identity.key_id().to_string(),
            signature_hex: hex_encode(&signature.to_bytes()),
        })
    }

    fn sign_danger_admission(
        &self,
        identity: &ConfirmationAuthorityIdentity,
        proof: DangerAdmissionProof,
    ) -> Result<SignedDangerAdmissionProof> {
        if identity != &self.identity {
            bail!("danger-admission signing identity does not match fixed custody");
        }
        let bytes = danger_admission_signing_bytes(
            &proof,
            identity.key_id(),
            identity.key_generation(),
            identity.canonical_root_identity(),
            identity.installation_identity(),
            identity.os_user_identity_digest(),
            identity.state_root_generation(),
        )
        .map_err(|_| anyhow::anyhow!("danger-admission proof is invalid"))?;
        let signature = SigningKey::from_bytes(&self.seed).sign(&bytes);
        Ok(SignedDangerAdmissionProof::from_untrusted_parts(
            proof,
            identity.key_id().to_string(),
            identity.key_generation(),
            identity.canonical_root_identity().to_string(),
            identity.installation_identity().to_string(),
            identity.os_user_identity_digest().to_string(),
            identity.state_root_generation(),
            hex_encode(&signature.to_bytes()),
        ))
    }

    fn sign_run_control_attestation(
        &self,
        identity: &ConfirmationAuthorityIdentity,
        payload: RunControlAttestationPayload,
    ) -> Result<RunControlAttestation> {
        let state_root_generation = i64::try_from(identity.state_root_generation())
            .context("fixed custody state-root generation exceeds signed contract range")?;
        let signer_key_generation = i64::try_from(identity.key_generation())
            .context("fixed custody signer generation exceeds signed contract range")?;
        if identity != &self.identity
            || payload.canonical_root_identity != identity.canonical_root_identity()
            || payload.installation_identity != identity.installation_identity()
            || payload.os_user_identity_digest != identity.os_user_identity_digest()
            || payload.state_root_generation != state_root_generation
            || payload.signer_key_generation != signer_key_generation
        {
            bail!("run-control payload does not match the fixed custody identity");
        }
        let bytes = carsinos_protocol::execass::run_control_attestation_signing_bytes(
            &payload,
            identity.key_id(),
        )
        .map_err(|_| anyhow::anyhow!("run-control attestation payload is invalid"))?;
        let signature = SigningKey::from_bytes(&self.seed).sign(&bytes);
        Ok(RunControlAttestation {
            payload,
            key_id: identity.key_id().to_string(),
            signature_hex: hex_encode(&signature.to_bytes()),
        })
    }
}

fn require_exact_run_control_snapshot(
    event: &VerifiedRunControlEvent,
    snapshot: &RunControlSigningSnapshot,
) -> Result<()> {
    if snapshot.stopped_epoch < 0
        || snapshot.policy_revision <= 0
        || !snapshot
            .unresolved_effect_disclosure_digest
            .strip_prefix("sha256:")
            .is_some_and(|digest| {
                digest.len() == 64 && digest.as_bytes().iter().all(u8::is_ascii_hexdigit)
            })
    {
        bail!("run-control signing snapshot is invalid");
    }
    match (event.operation(), event.resume()) {
        (ActorRunControlOperation::GlobalStop | ActorRunControlOperation::DelegationStop, None) => {
        }
        (
            ActorRunControlOperation::GlobalResume | ActorRunControlOperation::DelegationResume,
            Some(resume),
        ) if resume.stopped_epoch() == snapshot.stopped_epoch
            && resume.current_policy_revision() == snapshot.policy_revision
            && resume.unresolved_effect_disclosure_digest()
                == snapshot.unresolved_effect_disclosure_digest
            && resume.delegation_state_revision() == snapshot.delegation_state_revision
            && resume.current_plan_revision() == snapshot.current_plan_revision => {}
        _ => bail!("verified run-control event does not match the current storage snapshot"),
    }
    Ok(())
}

fn protocol_run_control_operation(value: ActorRunControlOperation) -> ProtocolRunControlOperation {
    match value {
        ActorRunControlOperation::GlobalStop => ProtocolRunControlOperation::GlobalStop,
        ActorRunControlOperation::GlobalResume => ProtocolRunControlOperation::GlobalResume,
        ActorRunControlOperation::DelegationStop => ProtocolRunControlOperation::DelegationStop,
        ActorRunControlOperation::DelegationResume => ProtocolRunControlOperation::DelegationResume,
    }
}

fn protocol_run_control_target(value: &ActorRunControlTarget) -> ProtocolRunControlTarget {
    match value {
        ActorRunControlTarget::Global => ProtocolRunControlTarget::Global,
        ActorRunControlTarget::Delegation { delegation_id } => {
            ProtocolRunControlTarget::Delegation {
                delegation_id: delegation_id.clone(),
            }
        }
    }
}

fn parse_custody_seed(
    identity: &ConfirmationAuthorityIdentity,
    payload: &str,
) -> Result<Zeroizing<[u8; 32]>> {
    let mut parts = payload.split('|');
    let (Some(version), Some(root), Some(installation), Some(seed_hex), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        bail!("confirmation OS credential is malformed");
    };
    if version != "carsinos-confirmation-custody-v1"
        || root != identity.canonical_root_identity()
        || installation != identity.installation_identity()
        || seed_hex.len() != 64
    {
        bail!("confirmation OS credential does not match its public identity");
    }
    let mut seed = Zeroizing::new([0_u8; 32]);
    for (index, slot) in seed.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&seed_hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow::anyhow!("confirmation OS credential is malformed"))?;
    }
    Ok(seed)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn require_exact_event_binding(
    event: &VerifiedConfirmationEvent,
    binding: &PendingDangerConfirmationAlternativeBinding,
) -> Result<()> {
    require_exact_event_binding_for_result(
        event,
        binding,
        carsinos_protocol::execass::DecisionResult::ConfirmAndContinue,
    )
}

fn require_exact_typed_event_binding(
    event: &VerifiedConfirmationEvent,
    binding: &DecisionResolutionBinding,
) -> Result<()> {
    let verified = event.verified_decision_binding();
    let intent_digest = canonical_intent_digest(&binding.normalized_intent)?;
    let decision_revision =
        u64::try_from(binding.decision_revision).context("stored decision revision is invalid")?;
    if verified.decision_id != binding.decision_id
        || verified.decision_revision != decision_revision
        || verified.normalized_intent_digest != intent_digest
        || verified.policy_revision != binding.policy_revision
        || verified.canonical_manifest_digest != binding.manifest_digest
        || verified.selected_logical_action_id != binding.selected_logical_action_id
        || verified.presented_action_digest != binding.exact_selected_action_digest
        || verified.declared_consequence_digest != binding.declared_consequence_digest
        || verified.challenge_digest != binding.challenge_nonce_digest
        || verified.expires_at_ms != binding.expires_at
    {
        bail!("verified decision event is not bound to this exact storage decision");
    }
    require_event_shape(event)
}

fn require_exact_event_binding_for_result(
    event: &VerifiedConfirmationEvent,
    binding: &PendingDangerConfirmationAlternativeBinding,
    expected_result: carsinos_protocol::execass::DecisionResult,
) -> Result<()> {
    let verified = event.verified_decision_binding();
    let intent_digest = canonical_intent_digest(&binding.normalized_intent)?;
    let decision_revision =
        u64::try_from(binding.decision_revision).context("stored decision revision is invalid")?;
    if verified.decision_id != binding.decision_id
        || verified.decision_revision != decision_revision
        || verified.normalized_intent_digest != intent_digest
        || verified.policy_revision != binding.policy_revision
        || verified.canonical_manifest_digest != binding.manifest_digest
        || verified.selected_logical_action_id != binding.selected_logical_action_id
        || verified.presented_action_digest != binding.exact_selected_action_digest
        || verified.declared_consequence_digest != binding.declared_consequence_digest
        || verified.challenge_digest != binding.challenge_nonce_digest
        || verified.expires_at_ms != binding.expires_at
        || event.verified_decision_result() != expected_result
    {
        bail!("verified confirmation event is not bound to this exact storage action");
    }
    require_event_shape(event)
}

fn require_event_shape(event: &VerifiedConfirmationEvent) -> Result<()> {
    match event.actor_type() {
        ActorType::HumanLocal
            if event.authenticated_ingress() == "native-control"
                && event.channel_assurance() == "interactive-local"
                && event.source_message_id().is_none()
                && event.provider_event_id().is_none() =>
        {
            Ok(())
        }
        ActorType::HumanRemote
            if !event.credential_identity().is_empty()
                && !event.authenticated_ingress().is_empty()
                && !event.channel_assurance().is_empty()
                && event.source_message_id().is_some()
                && event.provider_event_id().is_some() =>
        {
            Ok(())
        }
        _ => bail!("verified confirmation event has an invalid custody shape"),
    }
}

fn payload_from_storage(
    identity: &ConfirmationAuthorityIdentity,
    event: &VerifiedConfirmationEvent,
    binding: &PendingDangerConfirmationAlternativeBinding,
) -> Result<ConfirmationAttestationPayload> {
    require_exact_event_binding(event, binding)?;
    let issued_at_ms = server_time_ms()?;
    let policy_revision =
        u64::try_from(binding.policy_revision).context("stored policy revision is invalid")?;
    let decision_revision =
        u64::try_from(binding.decision_revision).context("stored decision revision is invalid")?;
    let expires_at_ms = u64::try_from(binding.expires_at).context("stored expiry is invalid")?;
    if issued_at_ms >= expires_at_ms {
        bail!("confirmation challenge expired before signing");
    }
    let (
        actor_type,
        credential_identity,
        authenticated_ingress,
        channel_assurance,
        source_message_id,
        provider_event_id,
    ) = match event.actor_type() {
        ActorType::HumanLocal => (
            "human_local".to_string(),
            identity.local_credential_identity().to_string(),
            "native-control".to_string(),
            "interactive-local".to_string(),
            None,
            None,
        ),
        ActorType::HumanRemote => (
            "human_remote".to_string(),
            event.credential_identity().to_string(),
            event.authenticated_ingress().to_string(),
            event.channel_assurance().to_string(),
            event.source_message_id().map(str::to_string),
            event.provider_event_id().map(str::to_string),
        ),
        _ => bail!("non-human event cannot resolve confirmation"),
    };
    Ok(ConfirmationAttestationPayload {
        actor_type,
        credential_identity,
        authenticated_ingress,
        channel_assurance,
        request_correlation_id: event.request_correlation_id().to_string(),
        source_message_id,
        provider_event_id,
        normalized_intent_digest: canonical_intent_digest(&binding.normalized_intent)?,
        policy_revision,
        decision_id: binding.decision_id.clone(),
        decision_revision,
        decision_result: "confirm_and_continue".to_string(),
        canonical_manifest_digest: binding.manifest_digest.clone(),
        selected_logical_action_id: binding.selected_logical_action_id.clone(),
        selected_action_digest: binding.exact_selected_action_digest.clone(),
        declared_consequence_digest: binding.declared_consequence_digest.clone(),
        challenge_nonce_digest: binding.challenge_nonce_digest.clone(),
        challenge_expires_at_ms: expires_at_ms,
        issued_at_ms,
        canonical_root_identity: identity.canonical_root_identity().to_string(),
        installation_identity: identity.installation_identity().to_string(),
        os_user_identity_digest: identity.os_user_identity_digest().to_string(),
        state_root_generation: identity.state_root_generation(),
        signer_key_generation: identity.key_generation(),
    })
}

fn server_time_ms() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before UNIX epoch")?
            .as_millis(),
    )
    .context("system clock exceeds confirmation range")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn scheduler_operation_identity(
    operation: &str,
    job_id: &str,
    continuation_id: &str,
    worker_id: &str,
    job_lease_expires_at: i64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.scheduler_operation.v1\0");
    for value in [operation, job_id, continuation_id, worker_id] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    digest.update(job_lease_expires_at.to_le_bytes());
    format!("{:x}", digest.finalize())
}

fn objective_recovery_operation_identity(
    logical_effect_id: &str,
    expected_pre_state_revision: i64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.objective_recovery_operation.v1\0");
    digest.update(logical_effect_id.as_bytes());
    digest.update([0]);
    digest.update(expected_pre_state_revision.to_le_bytes());
    format!("{:x}", digest.finalize())
}

fn technical_resource_lifecycle_resolution_name(
    resolution: TechnicalResourceLifecycleResolution,
) -> &'static str {
    match resolution {
        TechnicalResourceLifecycleResolution::ExpireUndispatched => "expire-undispatched",
        TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => "recover-possibly-invoked",
        TechnicalResourceLifecycleResolution::ReconcileAbsent => "reconcile-absent",
        TechnicalResourceLifecycleResolution::ReconcilePresent => "reconcile-present",
    }
}

fn technical_resource_lifecycle_operation_identity(
    identity: &ContinuationClaimIdentity,
    resolution: TechnicalResourceLifecycleResolution,
    evidence_digest: &str,
    technical_resource_actuals: &[TechnicalResourceActualInput],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical_resource_lifecycle_operation.v1\0");
    for value in [
        &identity.claim_event_id,
        &identity.claim_receipt_id,
        &identity.continuation_id,
        &identity.delegation_id,
        &identity.action_id,
        &identity.job_id,
        &identity.worker_id,
        &identity.runtime_host_instance_id,
        evidence_digest,
        technical_resource_lifecycle_resolution_name(resolution),
    ] {
        digest.update(Sha256::digest(value.as_bytes()));
    }
    for value in [
        identity.job_lease_expires_at,
        identity.continuation_fencing_token,
        identity.runtime_host_generation,
        identity.runtime_fencing_token,
        identity.state_root_generation,
        identity.policy_revision,
        identity.global_stop_epoch,
    ] {
        digest.update(value.to_be_bytes());
    }
    let mut canonical_actuals = technical_resource_actuals.iter().collect::<Vec<_>>();
    canonical_actuals.sort_by(|left, right| {
        (
            &left.reservation_id,
            left.amount_actual,
            &left.evidence_digest,
        )
            .cmp(&(
                &right.reservation_id,
                right.amount_actual,
                &right.evidence_digest,
            ))
    });
    for actual in canonical_actuals {
        digest.update(Sha256::digest(actual.reservation_id.as_bytes()));
        digest.update(actual.amount_actual.to_be_bytes());
        digest.update(Sha256::digest(actual.evidence_digest.as_bytes()));
    }
    format!("{:x}", digest.finalize())
}

fn canonical_intent_digest(normalized_intent: &str) -> Result<String> {
    owner_normalized_intent_digest(normalized_intent)
        .context("storage normalized intent cannot be canonically digest-bound")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execass_actor_gate::{
        ExecAssActorGate, RemoteProviderRunControlEvent, RunControlBinding,
        RunControlResumeBinding, TestRemoteConfirmationEventInput,
    };
    use carsinos_core::execass_actor::CurrentDecisionBinding;
    use carsinos_core::execass_policy::{
        compile_technical_quota_snapshot, compile_technical_resource_requirements,
        technical_effective_authority_digest, TechnicalQuotaEntryInput,
        TechnicalResourceKind as CoreResourceKind, TechnicalResourceRequirementInput,
    };
    use carsinos_effect_recorder::{RecorderEndpoint, TestRecorderFixture, TestRecorderTransport};
    use carsinos_protocol::execass::DecisionResult;
    use carsinos_protocol::execass_recorder::{
        ExecuteOnceV1, OpaqueOperandEnvelopeV1, QueryOnlyV1, RecorderBindingV1, RecorderRequestV1,
        RECORDER_PROTOCOL_VERSION,
    };
    use carsinos_storage::execass::{AuthorityLinkKind, RemoteOwnerConfirmationIngress};
    use carsinos_storage::{init_execass_fresh_root, AppPaths, Storage};
    use rusqlite::{params, Connection, OptionalExtension};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const TEST_SEED: [u8; 32] = [37; 32];

    impl InstalledRecorderDispatchMaterial {
        fn new() -> Self {
            let binding = RecorderBindingV1 {
                protocol_version: RECORDER_PROTOCOL_VERSION.to_owned(),
                canonical_root_identity: format!("sha256:{}", "a".repeat(64)),
                installation_id: "test-installation".to_owned(),
                state_root_generation: 1,
                os_user_identity_digest: "b".repeat(64),
                runtime_host_generation: 1,
                runtime_host_instance_id: "test-runtime-host".to_owned(),
                runtime_fencing_token: 1,
            };
            let mut execute_once = ExecuteOnceV1 {
                binding: binding.clone(),
                request_id: "test-execute-request".to_owned(),
                claim_event_id: "test-claim-event".to_owned(),
                claim_receipt_id: "test-claim-receipt".to_owned(),
                continuation_fencing_token: 1,
                delegation_id: "test-delegation".to_owned(),
                continuation_id: "test-continuation".to_owned(),
                action_id: "test-action".to_owned(),
                logical_effect_id: "test-effect".to_owned(),
                internal_idempotency_key: "test-internal-idempotency".to_owned(),
                attempt_id: "test-attempt".to_owned(),
                attempt_number: 1,
                provider_identity: "test-provider".to_owned(),
                provider_version: "test-provider-v1".to_owned(),
                adapter_identity: "test-fixed-adapter".to_owned(),
                adapter_artifact_digest: format!("sha256:{}", "c".repeat(64)),
                provider_request_digest: String::new(),
                provider_idempotency_key: Some("test-provider-idempotency".to_owned()),
                reconciliation_key: Some("test-reconciliation-key".to_owned()),
                manifest_digest: format!("sha256:{}", "d".repeat(64)),
                payload_digest: format!("sha256:{}", "e".repeat(64)),
                operand_envelope: OpaqueOperandEnvelopeV1 {
                    non_secret: serde_json::json!({"fixed": "route-proof-only"}),
                    secret_handles: Vec::new(),
                },
                deadline_ms: 9_999_999_999_999,
                client_nonce: "test-execute-nonce".to_owned(),
                command_mac: String::new(),
            };
            execute_once.provider_request_digest = execute_once
                .derived_provider_request_digest()
                .expect("fixed execution material has a digest");
            Self {
                execute_once,
                query_only: QueryOnlyV1 {
                    binding,
                    request_id: "test-query-request".to_owned(),
                    attempt_id: "test-attempt".to_owned(),
                    expected_command_digest: None,
                    known_journal_head: None,
                    client_nonce: "test-query-nonce".to_owned(),
                    command_mac: String::new(),
                },
            }
        }

        fn bind_to_attempt(&mut self, attempt: &carsinos_storage::execass::ProviderAttemptRecord) {
            let dispatch = &attempt.dispatch;
            self.execute_once.attempt_id = attempt.attempt_id.clone();
            self.execute_once.attempt_number = attempt.attempt_number;
            self.execute_once.claim_event_id = dispatch.claim_event_id.clone();
            self.execute_once.claim_receipt_id = dispatch.claim_receipt_id.clone();
            self.execute_once.continuation_fencing_token = dispatch.continuation_fencing_token;
            self.execute_once.delegation_id = dispatch.delegation_id.clone();
            self.execute_once.continuation_id = dispatch.continuation_id.clone();
            self.execute_once.action_id = dispatch.action_id.clone();
            self.execute_once.logical_effect_id = dispatch.logical_effect_id.clone();
            self.execute_once.internal_idempotency_key = dispatch.internal_idempotency_key.clone();
            self.execute_once.provider_identity = dispatch
                .provider_identity
                .clone()
                .expect("test attempt has an external provider");
            self.execute_once.provider_idempotency_key = dispatch.provider_idempotency_key.clone();
            self.execute_once.reconciliation_key = dispatch.reconciliation_key.clone();
            self.execute_once.manifest_digest = dispatch.manifest_digest.clone();
            self.execute_once.payload_digest = dispatch.payload_digest.clone();
            self.execute_once.binding.runtime_host_generation = dispatch.runtime_host_generation;
            self.execute_once.binding.runtime_host_instance_id =
                dispatch.runtime_host_instance_id.clone();
            self.execute_once.binding.runtime_fencing_token = dispatch.runtime_fencing_token;
            self.execute_once.provider_request_digest = self
                .execute_once
                .derived_provider_request_digest()
                .expect("test attempt material has a provider digest");
            assert_eq!(
                self.execute_once.provider_request_digest, attempt.provider_request_digest,
                "test protocol and storage provider-request digests diverged"
            );
            self.query_only.binding = self.execute_once.binding.clone();
            self.query_only.attempt_id = attempt.attempt_id.clone();
        }
    }

    struct Fixture {
        _temp: TempDir,
        paths: AppPaths,
        store: ExecAssStore,
        binding: PendingDangerConfirmationAlternativeBinding,
        runtime: ExecAssConfirmationRuntime,
        gate: ExecAssActorGate,
    }

    fn fixture(suffix: &str) -> Fixture {
        let temp = TempDir::new_in(env!("CARGO_MANIFEST_DIR"))
            .expect("create gateway confirmation fixture on project drive");
        let paths = AppPaths::from_root(temp.path().join("state"));
        init_execass_fresh_root(&paths).expect("initialize exact ExecAss fixture");
        let store = ExecAssStore::open(&paths).expect("open exact ExecAss fixture");
        let requested_at = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_millis(),
        )
        .expect("test clock range");
        let decision_id = format!("decision-{suffix}");
        let action_id = format!("action-{suffix}");
        let binding = store
            .prepare_test_confirmation_runtime_projection(
                &decision_id,
                &action_id,
                requested_at,
                requested_at + 120_000,
            )
            .expect("prepare exact pending confirmation");
        Connection::open(&paths.db_path)
            .expect("open confirmation runtime host fixture")
            .execute_batch(
                r#"
                INSERT INTO execass_runtime_host_generations(
                  generation,ownership_scope,state_root_generation,installation_identity,
                  os_user_identity_digest,host_instance_id,started_at
                ) VALUES(1,'execass',1,'gateway-test-installation','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','gateway-test-host',1);
                INSERT INTO execass_runtime_host_leases(
                  lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
                ) VALUES('gateway-test-lease','execass',1,'gateway-test-host',1,1,9999999999999);
                "#,
            )
            .expect("seed confirmation receipt runtime host");
        let runtime = ExecAssConfirmationRuntime::open_for_test(store.clone(), TEST_SEED)
            .expect("open test confirmation runtime");
        let gate = ExecAssActorGate::new(
            Some(b"gateway-confirmation-runtime-test-secret".to_vec()),
            HashMap::from([
                ("telegram".to_string(), "owner-telegram".to_string()),
                ("discord".to_string(), "owner-discord".to_string()),
            ]),
            temp.path().join("actor-replay"),
        );
        Fixture {
            _temp: temp,
            paths,
            store,
            binding,
            runtime,
            gate,
        }
    }

    fn current(binding: &PendingDangerConfirmationAlternativeBinding) -> CurrentDecisionBinding {
        CurrentDecisionBinding {
            decision_id: binding.decision_id.clone(),
            decision_revision: u64::try_from(binding.decision_revision).expect("revision"),
            normalized_intent_digest: canonical_intent_digest(&binding.normalized_intent)
                .expect("canonical intent digest"),
            policy_revision: binding.policy_revision,
            canonical_manifest_digest: binding.manifest_digest.clone(),
            selected_logical_action_id: binding.selected_logical_action_id.clone(),
            presented_action_digest: binding.exact_selected_action_digest.clone(),
            declared_consequence_digest: binding.declared_consequence_digest.clone(),
            challenge_digest: binding.challenge_nonce_digest.clone(),
            expires_at_ms: binding.expires_at,
        }
    }

    fn current_typed(binding: &DecisionResolutionBinding) -> CurrentDecisionBinding {
        CurrentDecisionBinding {
            decision_id: binding.decision_id.clone(),
            decision_revision: u64::try_from(binding.decision_revision).expect("revision"),
            normalized_intent_digest: canonical_intent_digest(&binding.normalized_intent)
                .expect("canonical intent digest"),
            policy_revision: binding.policy_revision,
            canonical_manifest_digest: binding.manifest_digest.clone(),
            selected_logical_action_id: binding.selected_logical_action_id.clone(),
            presented_action_digest: binding.exact_selected_action_digest.clone(),
            declared_consequence_digest: binding.declared_consequence_digest.clone(),
            challenge_digest: binding.challenge_nonce_digest.clone(),
            expires_at_ms: binding.expires_at,
        }
    }

    fn insert_typed_test_decision(
        fixture: &Fixture,
        decision_id: &str,
        decision_kind: &str,
        persisted_idempotency_key: &str,
    ) {
        insert_typed_test_decision_with_payload(
            fixture,
            decision_id,
            decision_kind,
            persisted_idempotency_key,
            &"b".repeat(64),
        );
    }

    fn insert_typed_test_decision_with_payload(
        fixture: &Fixture,
        decision_id: &str,
        decision_kind: &str,
        persisted_idempotency_key: &str,
        payload_digest: &str,
    ) {
        Connection::open(&fixture.paths.db_path)
            .expect("open typed decision fixture")
            .execute(
                "INSERT INTO execass_decisions (decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,decision_kind,status,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,consequence,alternatives_json,idempotency_key,requested_at) VALUES (?1,'test-delegation',2,2,1,1,?2,'pending',?3,?4,?5,?6,'{}','[]',NULL,NULL,'{}','continue this exact waiting branch',?7,'[\"confirm_and_continue\",\"revise\",\"decline\",\"stop\"]',?8,?9)",
                params![
                    decision_id,
                    decision_kind,
                    fixture.binding.exact_selected_action_json,
                    fixture.binding.selected_logical_action_id,
                    fixture.binding.manifest_digest,
                    payload_digest,
                    fixture.binding.declared_consequence,
                    persisted_idempotency_key,
                    fixture.binding.requested_at,
                ],
            )
            .expect("insert typed decision fixture");
    }

    fn verified_remote_global_control(
        fixture: &Fixture,
        operation: ActorRunControlOperation,
        suffix: &str,
        resume_status: Option<&GlobalStopStatus>,
    ) -> VerifiedRunControlEvent {
        let observed_at =
            i64::try_from(server_time_ms().expect("test clock")).expect("clock range");
        fixture
            .store
            .reconcile_remote_confirmation_ingress(
                &[RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                observed_at,
            )
            .expect("activate exact remote owner ingress");
        let idempotency_key = format!("global-control-idempotency-{suffix}");
        let correlation_id = format!("global-control-correlation-{suffix}");
        let binding = match operation {
            ActorRunControlOperation::GlobalStop => {
                RunControlBinding::global_stop(idempotency_key, correlation_id.clone(), observed_at)
            }
            ActorRunControlOperation::GlobalResume => {
                let status = resume_status.expect("resume status");
                RunControlBinding::global_resume(
                    idempotency_key,
                    correlation_id.clone(),
                    observed_at,
                    RunControlResumeBinding::new(
                        status.global_stop_epoch,
                        status.current_policy_revision,
                        status.unresolved_external_effects_digest.clone(),
                        None,
                        None,
                    )
                    .expect("exact global resume snapshot"),
                )
            }
            ActorRunControlOperation::DelegationStop
            | ActorRunControlOperation::DelegationResume => {
                panic!("global test helper received a delegation operation")
            }
        }
        .expect("valid global control binding");
        let provider_event = RemoteProviderRunControlEvent::from_telegram_long_poll(
            "telegram-listener-1".to_string(),
            "owner-telegram".to_string(),
            "owner-control".to_string(),
            format!("message-{suffix}"),
            format!("provider-event-{suffix}"),
            correlation_id,
            binding.request_binding_digest(),
        )
        .expect("valid remote run-control event");
        fixture
            .gate
            .verify_remote_run_control(&provider_event, &binding)
            .expect("verified remote global control")
    }

    fn verified_remote_delegation_control(
        fixture: &Fixture,
        operation: ActorRunControlOperation,
        suffix: &str,
        resume_status: Option<&DelegationRunControlStatus>,
    ) -> VerifiedRunControlEvent {
        let observed_at =
            i64::try_from(server_time_ms().expect("test clock")).expect("clock range");
        fixture
            .store
            .reconcile_remote_confirmation_ingress(
                &[RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                observed_at,
            )
            .expect("activate exact remote owner ingress");
        let idempotency_key = format!("delegation-control-idempotency-{suffix}");
        let correlation_id = format!("delegation-control-correlation-{suffix}");
        let binding = match operation {
            ActorRunControlOperation::DelegationStop => RunControlBinding::delegation_stop(
                "test-delegation".to_string(),
                idempotency_key,
                correlation_id.clone(),
                observed_at,
            ),
            ActorRunControlOperation::DelegationResume => {
                let status = resume_status.expect("resume status");
                RunControlBinding::delegation_resume(
                    "test-delegation".to_string(),
                    idempotency_key,
                    correlation_id.clone(),
                    observed_at,
                    RunControlResumeBinding::new(
                        status.stop_epoch,
                        status.policy_revision,
                        status.unresolved_external_effects_digest.clone(),
                        Some(status.state_revision),
                        status.current_plan_revision,
                    )
                    .expect("exact delegation resume snapshot"),
                )
            }
            ActorRunControlOperation::GlobalStop | ActorRunControlOperation::GlobalResume => {
                panic!("delegation helper received a global operation")
            }
        }
        .expect("valid delegation control binding");
        let provider_event = RemoteProviderRunControlEvent::from_telegram_long_poll(
            "telegram-listener-1".to_string(),
            "owner-telegram".to_string(),
            "owner-control".to_string(),
            format!("message-{suffix}"),
            format!("provider-event-{suffix}"),
            correlation_id,
            binding.request_binding_digest(),
        )
        .expect("valid remote delegation event");
        fixture
            .gate
            .verify_remote_run_control(&provider_event, &binding)
            .expect("verified remote delegation control")
    }

    #[test]
    fn verified_delegation_stop_drains_and_signed_resume_replays_without_new_work() {
        let fixture = fixture("delegation-control-roundtrip");
        let continuation_count = count(&fixture.paths, "execass_continuations");
        let initial = fixture
            .runtime
            .read_delegation_control_status("test-delegation", server_time_ms().unwrap() as i64)
            .unwrap()
            .unwrap();
        assert_eq!(initial.run_control, StorageRunControlState::Running);

        let stop = verified_remote_delegation_control(
            &fixture,
            ActorRunControlOperation::DelegationStop,
            "stop",
            None,
        );
        let stopped = fixture
            .runtime
            .coordinate_verified_delegation_control(&stop, "test-delegation")
            .expect("stop exact delegation");
        let stopped = match stopped {
            DelegationRunControlMutationOutcome::Drained(status) => status,
            other => panic!("delegation did not drain immediately: {other:?}"),
        };
        assert_eq!(stopped.run_control, StorageRunControlState::Stopped);

        let resume = verified_remote_delegation_control(
            &fixture,
            ActorRunControlOperation::DelegationResume,
            "resume",
            Some(&stopped),
        );
        assert!(matches!(
            fixture
                .runtime
                .coordinate_verified_delegation_control(&resume, "test-delegation")
                .expect("resume exact delegation"),
            DelegationRunControlMutationOutcome::Resumed(ref status)
                if status.run_control == StorageRunControlState::Running
        ));
        let restarted =
            ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED).unwrap();
        assert!(matches!(
            restarted
                .coordinate_verified_delegation_control(&resume, "test-delegation")
                .expect("restart replay exact delegation resume"),
            DelegationRunControlMutationOutcome::Replayed(_)
        ));
        assert_eq!(
            count(&fixture.paths, "execass_continuations"),
            continuation_count
        );
    }

    #[test]
    fn scheduler_completes_requested_delegation_stop_only_after_safe_boundary() {
        let fixture = fixture("delegation-control-safe-boundary");
        let conn = Connection::open(&fixture.paths.db_path).expect("open safe-boundary fixture");
        conn.execute(
            "UPDATE execass_action_branches SET status='executing' WHERE delegation_id='test-delegation'",
            [],
        )
        .expect("seed executing delegation branch");

        let stop = verified_remote_delegation_control(
            &fixture,
            ActorRunControlOperation::DelegationStop,
            "safe-boundary-stop",
            None,
        );
        assert!(matches!(
            fixture
                .runtime
                .coordinate_verified_delegation_control(&stop, "test-delegation")
                .expect("request exact delegation stop"),
            DelegationRunControlMutationOutcome::StopRequested(ref status)
                if status.drain_state == DelegationStopDrainState::Draining
                    && status.executing_branch_count == 1
        ));
        assert_eq!(
            fixture
                .runtime
                .complete_ready_delegation_stops(server_time_ms().unwrap() as i64, 8)
                .unwrap(),
            0
        );

        conn.execute(
            "UPDATE execass_action_branches SET status='waiting' WHERE delegation_id='test-delegation'",
            [],
        )
        .expect("reach safe execution boundary");
        assert_eq!(
            fixture
                .runtime
                .complete_ready_delegation_stops(server_time_ms().unwrap() as i64, 8)
                .unwrap(),
            1
        );
        let stopped = fixture
            .runtime
            .read_delegation_control_status("test-delegation", server_time_ms().unwrap() as i64)
            .unwrap()
            .unwrap();
        assert_eq!(stopped.run_control, StorageRunControlState::Stopped);
        assert_eq!(stopped.drain_state, DelegationStopDrainState::Stopped);
    }

    #[test]
    fn verified_global_stop_and_signed_resume_commit_once_and_exactly_replay() {
        let fixture = fixture("global-control-roundtrip");
        let initial = fixture
            .runtime
            .read_global_control_status()
            .expect("read initial global status");
        assert!(!initial.engaged);

        let stop = verified_remote_global_control(
            &fixture,
            ActorRunControlOperation::GlobalStop,
            "global-control-stop",
            None,
        );
        assert!(matches!(
            fixture
                .runtime
                .coordinate_verified_global_control(&stop)
                .expect("engage verified global stop"),
            GlobalStopMutationOutcome::Engaged(ref status)
                if status.engaged && status.global_stop_epoch == initial.global_stop_epoch + 1
        ));
        let restarted_after_stop =
            ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED)
                .expect("restart runtime after global stop");
        assert!(matches!(
            restarted_after_stop
                .coordinate_verified_global_control(&stop)
                .expect("restart-safe replay exact verified global stop"),
            GlobalStopMutationOutcome::Replayed(_)
        ));

        let stopped = fixture
            .runtime
            .read_global_control_status()
            .expect("read stopped global status");
        let resume = verified_remote_global_control(
            &fixture,
            ActorRunControlOperation::GlobalResume,
            "global-control-resume",
            Some(&stopped),
        );
        assert!(matches!(
            fixture
                .runtime
                .coordinate_verified_global_control(&resume)
                .expect("resume verified global stop"),
            GlobalStopMutationOutcome::Resumed(ref status)
                if !status.engaged && status.global_stop_epoch == stopped.global_stop_epoch
        ));
        let restarted_after_resume =
            ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED)
                .expect("restart runtime after global resume");
        assert!(matches!(
            restarted_after_resume
                .coordinate_verified_global_control(&resume)
                .expect("restart-safe replay exact signed global resume"),
            GlobalStopMutationOutcome::Replayed(_)
        ));
        assert_eq!(count(&fixture.paths, "execass_outbox_events"), 2);
        assert_eq!(count(&fixture.paths, "execass_receipts"), 2);
        assert_eq!(count(&fixture.paths, "execass_run_control_attestations"), 1);
    }

    #[test]
    fn trusted_fail_safe_can_only_engage_and_uses_sealed_runtime_authority() {
        let fixture = fixture("global-control-fail-safe");
        assert!(matches!(
            fixture
                .runtime
                .engage_trusted_global_fail_safe()
                .expect("engage trusted global fail-safe"),
            GlobalStopMutationOutcome::Engaged(ref status) if status.engaged
        ));
        assert!(matches!(
            fixture
                .runtime
                .engage_trusted_global_fail_safe()
                .expect("repeat trusted global fail-safe"),
            GlobalStopMutationOutcome::AlreadyEngaged(_)
        ));
        let connection = Connection::open(&fixture.paths.db_path).expect("open fixture database");
        let actor: (String, String, String) = connection
            .query_row(
                "SELECT actor_type,actor_identity,actor_authority_provenance_id FROM execass_receipts WHERE receipt_kind='global_stop'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read trusted fail-safe receipt actor");
        assert_eq!(
            actor,
            (
                "runtime".to_string(),
                "execass-global-control".to_string(),
                "execass-global-control-carrier-authority".to_string(),
            )
        );
        assert_eq!(count(&fixture.paths, "execass_run_control_attestations"), 0);
    }

    #[test]
    fn global_coordinator_rejects_delegation_target_and_stale_resume_snapshot() {
        let fixture = fixture("global-control-hostile");
        let observed_at =
            i64::try_from(server_time_ms().expect("test clock")).expect("clock range");
        let delegation_binding = RunControlBinding::delegation_stop(
            "delegation-1".to_string(),
            "delegation-stop-idempotency".to_string(),
            "delegation-stop-correlation".to_string(),
            observed_at,
        )
        .expect("valid delegation stop binding");
        let delegation_provider_event = RemoteProviderRunControlEvent::from_telegram_long_poll(
            "telegram-listener-1".to_string(),
            "owner-telegram".to_string(),
            "owner-control".to_string(),
            "delegation-message".to_string(),
            "delegation-provider-event".to_string(),
            "delegation-stop-correlation".to_string(),
            delegation_binding.request_binding_digest(),
        )
        .expect("valid delegation provider event");
        let delegation_event = fixture
            .gate
            .verify_remote_run_control(&delegation_provider_event, &delegation_binding)
            .expect("verified delegation event");
        assert!(fixture
            .runtime
            .coordinate_verified_global_control(&delegation_event)
            .unwrap_err()
            .to_string()
            .contains("delegation target"));

        fixture
            .runtime
            .engage_trusted_global_fail_safe()
            .expect("engage hostile test stop");
        let stopped = fixture
            .runtime
            .read_global_control_status()
            .expect("read hostile stopped status");
        let mut wrong = stopped.clone();
        wrong.global_stop_epoch += 1;
        let stale_resume = verified_remote_global_control(
            &fixture,
            ActorRunControlOperation::GlobalResume,
            "stale-resume",
            Some(&wrong),
        );
        assert!(fixture
            .runtime
            .coordinate_verified_global_control(&stale_resume)
            .unwrap_err()
            .to_string()
            .contains("current storage snapshot"));
        assert!(
            fixture
                .runtime
                .read_global_control_status()
                .expect("global stop remains engaged")
                .engaged
        );
        assert_eq!(count(&fixture.paths, "execass_run_control_attestations"), 0);
    }

    #[test]
    fn production_runtime_has_no_in_process_effect_adapter_or_result_bypass() {
        let source = include_str!("execass_confirmation_runtime.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production module prefix");
        for forbidden in [
            concat!("Scheduler", "EffectAdapter"),
            concat!("dispatch_scheduler_", "effect_once"),
            concat!("RecordProviderAttempt", "ResultCommand"),
            concat!("record_provider_attempt_", "result"),
            concat!("Native", "RecorderCustody"),
            concat!("Recorder", "Credential"),
        ] {
            assert!(
                !production.contains(forbidden),
                "gateway production runtime retains forbidden in-process effect path: {forbidden}"
            );
        }
    }

    fn route_proof_attempt() -> carsinos_storage::execass::ProviderAttemptRecord {
        let provider_request_digest = InstalledRecorderDispatchMaterial::new()
            .execute_once
            .provider_request_digest;
        carsinos_storage::execass::ProviderAttemptRecord {
            attempt_id: "test-attempt".to_owned(),
            attempt_number: 1,
            status: carsinos_storage::execass::ProviderAttemptStatus::Invoking,
            dispatch: carsinos_storage::execass::LogicalEffectDispatchIdentity {
                logical_effect_id: "test-effect".to_owned(),
                delegation_id: "test-delegation".to_owned(),
                continuation_id: "test-continuation".to_owned(),
                action_id: "test-action".to_owned(),
                claim_event_id: "test-claim-event".to_owned(),
                claim_receipt_id: "test-claim-receipt".to_owned(),
                continuation_fencing_token: 1,
                runtime_host_generation: 1,
                runtime_host_instance_id: "test-runtime-host".to_owned(),
                runtime_fencing_token: 1,
                internal_idempotency_key: "test-internal-idempotency".to_owned(),
                provider_identity: Some("test-provider".to_owned()),
                provider_idempotency_key: Some("test-provider-idempotency".to_owned()),
                reconciliation_key: Some("test-reconciliation-key".to_owned()),
                manifest_digest: format!("sha256:{}", "d".repeat(64)),
                payload_digest: format!("sha256:{}", "e".repeat(64)),
            },
            provider_request_digest,
            provider_response_digest: None,
            provider_error_class: None,
            remote_effect_id: None,
            started_at: 1,
            finished_at: None,
        }
    }

    #[test]
    fn recorder_route_is_exhaustive_and_only_began_selects_execute_once() {
        let attempt = route_proof_attempt();
        let mut fixed = InstalledRecorderDispatchMaterial::new();
        fixed.bind_to_attempt(&attempt);
        let outcomes = [
            BeginProviderAttemptInvocationOutcome::Began(Box::new(attempt.clone())),
            BeginProviderAttemptInvocationOutcome::AlreadyInvoking(Box::new(attempt)),
            BeginProviderAttemptInvocationOutcome::Stale {
                reason: carsinos_storage::execass::ContinuationStaleReason::NotFound,
            },
            BeginProviderAttemptInvocationOutcome::Conflict,
            BeginProviderAttemptInvocationOutcome::NotFound,
        ];
        let mut execute_once_count = 0;
        let mut query_only_count = 0;
        let mut no_send_count = 0;
        for outcome in outcomes {
            match fixed.request_for(outcome).unwrap() {
                Some(RecorderRequestV1::ExecuteOnce(request)) => {
                    RecorderRequestV1::ExecuteOnce(request)
                        .validate()
                        .expect("fixed ExecuteOnce request remains protocol-valid");
                    execute_once_count += 1;
                }
                Some(RecorderRequestV1::QueryOnly(request)) => {
                    RecorderRequestV1::QueryOnly(request)
                        .validate()
                        .expect("fixed QueryOnly request remains protocol-valid");
                    query_only_count += 1;
                }
                Some(RecorderRequestV1::Reconcile(_)) => {
                    panic!("begin outcome routing must never select Reconcile")
                }
                None => no_send_count += 1,
            }
        }
        assert_eq!(execute_once_count, 1, "only Began may execute");
        assert_eq!(query_only_count, 1, "AlreadyInvoking is query-only");
        assert_eq!(no_send_count, 3, "stale/conflict/not-found send nothing");
    }

    #[tokio::test]
    async fn production_dispatch_method_sends_execute_once_then_query_only_over_the_sealed_client_transport(
    ) {
        let mut fixture = fixture("production-recorder-client-route");
        let event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "production-recorder-client-route-confirmation",
            )
            .unwrap();
        fixture
            .runtime
            .resolve(
                &event,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            )
            .unwrap();
        let now = i64::try_from(server_time_ms().unwrap()).unwrap();
        attach_test_technical_resources(
            &fixture,
            "production-recorder-client-route",
            now,
            true,
            "planned",
        );
        fixture
            .store
            .materialize_runnable_continuation_jobs(now + 1, 8)
            .unwrap();
        let job = Storage::from_paths(&fixture.paths)
            .acquire_due_jobs("production-recorder-client-worker", now + 2, 30_000, 8)
            .unwrap()
            .remove(0);
        let ContinuationClaimOutcome::Claimed(claimed) = fixture
            .runtime
            .claim_scheduler_continuation(&job, "production-recorder-client-worker", now + 2)
            .unwrap()
        else {
            panic!("continuation was not claimed")
        };
        let PrepareProviderAttemptOutcome::Prepared(prepared) = fixture
            .runtime
            .prepare_scheduler_effect_attempt(&claimed.identity, now + 3, None)
            .unwrap()
        else {
            panic!("attempt was not prepared")
        };

        let mut material = InstalledRecorderDispatchMaterial::new();
        material.bind_to_attempt(&prepared);
        let transport = TestRecorderTransport::default();
        let test_fixture =
            TestRecorderFixture::for_root(&material.execute_once.binding.canonical_root_identity);
        fixture.runtime.recorder_client = Some(
            test_fixture
                .client(RecorderEndpoint::for_binding(
                    fixture.paths.root.as_path(),
                    "test-installation",
                    1,
                ))
                .with_test_transport(transport.clone()),
        );
        fixture.runtime.recorder_binding = Some(material.execute_once.binding.clone());

        let mut drifted = material.clone();
        drifted.execute_once.payload_digest = format!("sha256:{}", "0".repeat(64));
        fixture
            .runtime
            .dispatch_installed_recorder_material(&claimed.identity, now + 4, &drifted)
            .await
            .expect_err("drifted installed material must fail before invoking");
        let status: String = Connection::open(&fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM execass_provider_attempts WHERE attempt_id=?1",
                [&prepared.attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "prepared");
        assert!(transport.requests().lock().await.is_empty());

        fixture
            .runtime
            .dispatch_installed_recorder_material(&claimed.identity, now + 5, &material)
            .await
            .unwrap();
        fixture
            .runtime
            .dispatch_installed_recorder_material(&claimed.identity, now + 6, &material)
            .await
            .unwrap();
        let requests = transport.requests().lock().await.clone();
        assert_eq!(requests.len(), 2);
        assert!(matches!(requests[0], RecorderRequestV1::ExecuteOnce(_)));
        assert!(matches!(requests[1], RecorderRequestV1::QueryOnly(_)));
    }

    #[test]
    fn production_dispatch_remains_closed_before_recorder_activation() {
        let fixture = fixture("closed-production-material");
        assert!(!fixture.runtime.recorder_client_is_ready());
        assert_eq!(count(&fixture.paths, "execass_provider_attempts"), 0);
    }

    fn count(paths: &AppPaths, table: &str) -> i64 {
        assert!(table
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
        Connection::open(&paths.db_path)
            .expect("open fixture database")
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count fixture table")
    }

    fn job_evidence(
        paths: &AppPaths,
        identity: &ContinuationClaimIdentity,
    ) -> ReceiptEvidenceInput {
        let conn = Connection::open(&paths.db_path).expect("open evidence fixture database");
        let materialized_link_id = conn
            .query_row(
                r#"SELECT link_id
                   FROM execass_authority_links
                   WHERE delegation_id=?1
                     AND authority_kind='job'
                     AND job_id=?2
                     AND authoritative_revision=0"#,
                rusqlite::params![identity.delegation_id, identity.job_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .expect("query lifecycle authority link");
        let link_id = if let Some(link_id) = materialized_link_id {
            link_id
        } else {
            let link_id = format!("gateway-lifecycle-job-link-{}", identity.job_id);
            let (event_id, correlation_id, causation_id, occurred_at):
                (String, String, String, i64) = conn
                .query_row(
                    "SELECT event_id,correlation_id,causation_id,occurred_at FROM execass_outbox_events WHERE aggregate_id=?1 AND event_name='execass.v1.delegation.transitioned' ORDER BY aggregate_revision DESC LIMIT 1",
                    [&identity.delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("read lifecycle authority transition");
            let (state_revision, link_revision): (i64, i64) = conn
                .query_row(
                    "SELECT state_revision,(SELECT COALESCE(MAX(link_revision),0)+1 FROM execass_authority_links WHERE delegation_id=?1) FROM execass_delegations WHERE delegation_id=?1",
                    [&identity.delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("read lifecycle authority revision");
            conn.execute(
                r#"INSERT INTO execass_authority_links(
                     link_id,delegation_id,link_revision,delegation_state_revision,
                     correlation_id,causation_id,outbox_event_id,authority_kind,
                     job_id,authoritative_revision,linked_at
                   ) VALUES(?1,?2,?3,?4,?5,?6,?7,'job',?8,0,?9)"#,
                rusqlite::params![
                    link_id,
                    identity.delegation_id,
                    link_revision,
                    state_revision,
                    correlation_id,
                    causation_id,
                    event_id,
                    identity.job_id,
                    occurred_at,
                ],
            )
            .expect("insert durable lifecycle job authority link");
            link_id
        };
        ReceiptEvidenceInput {
            authority_link_id: link_id,
            kind: AuthorityLinkKind::Job,
            source_id: identity.job_id.clone(),
            authoritative_revision: 0,
        }
    }

    fn attach_test_technical_resources(
        fixture: &Fixture,
        suffix: &str,
        now: i64,
        external_provider: bool,
        effect_state: &str,
    ) {
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        let (
            continuation_id,
            delegation_id,
            action_id,
            manifest_digest,
            policy_revision,
            authority_json,
        ) = conn
            .query_row(
                r#"SELECT c.continuation_id,c.delegation_id,c.action_id,p.manifest_digest,
                          d.policy_revision,d.effective_authority_json
                   FROM execass_continuations c
                   JOIN execass_delegations d ON d.delegation_id=c.delegation_id
                   JOIN execass_plans p ON p.delegation_id=c.delegation_id
                    AND p.plan_revision=c.target_plan_revision
                   LIMIT 1"#,
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .unwrap();
        let effect_id = format!("gateway-recovery-effect-{suffix}");
        let authority_digest = technical_effective_authority_digest(&authority_json).unwrap();
        let snapshot = compile_technical_quota_snapshot(
            &delegation_id,
            policy_revision,
            &authority_digest,
            "delegation",
            vec![TechnicalQuotaEntryInput {
                kind: CoreResourceKind::Tokens,
                unit: "token".into(),
                limit: 1,
            }],
        )
        .unwrap();
        let requirements = compile_technical_resource_requirements(
            &snapshot,
            &effect_id,
            &action_id,
            &manifest_digest,
            vec![TechnicalResourceRequirementInput {
                kind: CoreResourceKind::Tokens,
                unit: "token".into(),
                amount: 1,
            }],
        )
        .unwrap();
        let provider_identity = external_provider.then_some("test-provider");
        let provider_idempotency_key = external_provider.then_some("test-provider-idempotency");
        let reconciliation_key = external_provider.then_some("test-reconciliation-key");
        conn.execute(
            r#"INSERT INTO execass_logical_effects(
                 logical_effect_id,delegation_id,continuation_id,action_kind,state,
                 internal_idempotency_key,provider_identity,provider_idempotency_key,
                 reconciliation_key,manifest_digest,payload_digest,created_at,updated_at
               ) VALUES(?1,?2,?3,'read_only_local_inspection_and_bounded_reversible_local_work',
                        ?4,?5,?6,?7,?8,?9,?10,?11,?11)"#,
            params![
                effect_id,
                delegation_id,
                continuation_id,
                effect_state,
                format!("gateway-recovery-idem-{suffix}"),
                provider_identity,
                provider_idempotency_key,
                reconciliation_key,
                manifest_digest,
                format!("sha256:gateway-recovery-payload-{suffix}"),
                now,
            ],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO execass_technical_resource_quota_snapshots(
                 quota_snapshot_id,delegation_id,policy_revision,effective_authority_digest,
                 scope_key,canonical_entries_json,canonical_entries_digest,created_at
               ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"#,
            params![
                snapshot.quota_snapshot_id,
                snapshot.delegation_id,
                snapshot.policy_revision,
                snapshot.effective_authority_digest,
                snapshot.scope_key,
                snapshot.canonical_entries_json,
                snapshot.canonical_entries_digest,
                now,
            ],
        )
        .unwrap();
        let entry = &snapshot.entries[0];
        conn.execute(
            "INSERT INTO execass_technical_resource_quota_entries(quota_snapshot_id,technical_resource_kind,unit,amount_limit) VALUES(?1,?2,?3,?4)",
            params![snapshot.quota_snapshot_id, entry.kind.as_str(), entry.unit, entry.limit],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO execass_technical_resource_requirement_sets(
                 requirement_set_id,quota_snapshot_id,delegation_id,logical_effect_id,action_id,
                 manifest_digest,canonical_requirements_json,canonical_requirements_digest,created_at
               ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
            params![
                requirements.requirement_set_id,
                requirements.quota_snapshot_id,
                requirements.delegation_id,
                requirements.logical_effect_id,
                requirements.action_id,
                requirements.manifest_digest,
                requirements.canonical_requirements_json,
                requirements.canonical_requirements_digest,
                now,
            ],
        )
        .unwrap();
        let requirement = &requirements.requirements[0];
        conn.execute(
            "INSERT INTO execass_technical_resource_requirements(requirement_set_id,quota_snapshot_id,technical_resource_kind,unit,amount_required) VALUES(?1,?2,?3,?4,?5)",
            params![requirements.requirement_set_id, requirements.quota_snapshot_id, requirement.kind.as_str(), requirement.unit, requirement.amount],
        )
        .unwrap();
    }

    fn mark_test_effect_outcome_unknown(fixture: &Fixture, suffix: &str, now: i64) -> String {
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        let effect_id = format!("gateway-recovery-effect-{suffix}");
        let (continuation_id, action_id): (String, String) = conn
            .query_row(
                "SELECT continuation_id,action_id FROM execass_logical_effects JOIN execass_continuations USING(continuation_id) WHERE logical_effect_id=?1",
                [&effect_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let job_id = format!("gateway-duplicate-job-{suffix}");
        let claim_event_id = format!("gateway-duplicate-claim-{suffix}");
        let claim_receipt_id = format!("gateway-duplicate-claim-receipt-{suffix}");
        let runtime_authority: String = conn
            .query_row(
                "SELECT authority_provenance_id FROM execass_delegations WHERE delegation_id='test-delegation'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            r#"INSERT INTO jobs(job_id,agent_id,name,enabled,schedule_kind,interval_seconds,
                 run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,
                 created_at,updated_at)
               SELECT ?1,agent_id,'duplicate-risk test predecessor',0,'at',NULL,NULL,NULL,'{}',0,0,1000,?2,?2
               FROM agents ORDER BY agent_id LIMIT 1"#,
            params![job_id, now],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO execass_outbox_events(
                 event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
                 causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
               ) VALUES(?1,'execass.v1.continuation.claimed_or_result_recorded','test-delegation',
                 2,?2,?3,?4,'v1','{}',?1)"#,
            params![
                claim_event_id,
                format!("{claim_event_id}-correlation"),
                continuation_id,
                now
            ],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO execass_continuation_operation_history(
                 event_id,claim_event_id,claim_receipt_id,operation,result_status,continuation_id,
                 delegation_id,action_id,job_id,worker_id,job_lease_expires_at,
                 continuation_fencing_token,runtime_host_generation,runtime_host_instance_id,
                 runtime_fencing_token,state_root_generation,runtime_authority_provenance_id,
                 runtime_actor_identity,policy_revision,global_stop_epoch,technical_quota_policy_digest,
                 technical_quota_snapshot_id,technical_resource_reservation_set_json,
                 technical_resource_reservation_set_digest,technical_resource_evidence_digest,recorded_at
               ) VALUES(?1,?1,?2,'claim','executing',?3,'test-delegation',?4,?5,
                 'gateway-duplicate-worker',?6,1,1,'gateway-test-host',1,1,?7,
                 'gateway-duplicate-runtime',1,0,'sha256:test-duplicate-quota',NULL,'[]',
                 'sha256:test-duplicate-reservations',NULL,?6)"#,
            params![
                claim_event_id,
                claim_receipt_id,
                continuation_id,
                action_id,
                job_id,
                now,
                runtime_authority,
            ],
        )
        .unwrap();
        conn.execute(
            "UPDATE execass_continuations SET status='executing',job_id=?2,lease_owner='gateway-duplicate-worker',lease_expires_at=?3,fencing_token=1,updated_at=?4 WHERE continuation_id=?1",
            params![continuation_id, job_id, now, now],
        )
        .unwrap();
        let attempt_id = format!("gateway-duplicate-attempt-{suffix}");
        conn.execute(
            r#"INSERT INTO execass_provider_attempts(
                 attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,
                 claim_event_id,claim_receipt_id,attempt_number,fencing_token,host_generation,
                 host_instance_id,runtime_fencing_token,status,provider_request_digest,
                 provider_response_digest,started_at,finished_at
               ) VALUES(?1,'test-delegation',?2,?3,?4,?5,
                 ?6,1,1,1,'gateway-test-host',1,
                 'invoking',?7,NULL,?8,NULL)"#,
            params![
                attempt_id,
                effect_id,
                continuation_id,
                action_id,
                claim_event_id,
                claim_receipt_id,
                format!("sha256:{}", "1".repeat(64)),
                now,
            ],
        )
        .unwrap();
        drop(conn);
        fixture
            .store
            .mark_test_provider_attempt_outcome_unknown(&attempt_id, &effect_id, now + 1)
            .unwrap();
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        let settle_event_id = format!("gateway-duplicate-settle-{suffix}");
        conn.execute(
            r#"INSERT INTO execass_outbox_events(
                 event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
                 causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
               ) VALUES(?1,'execass.v1.continuation.claimed_or_result_recorded','test-delegation',
                 2,?2,?3,?4,'v1','{}',?1)"#,
            params![
                settle_event_id,
                format!("{settle_event_id}-correlation"),
                claim_event_id,
                now + 1,
            ],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO execass_continuation_operation_history(
                 event_id,claim_event_id,claim_receipt_id,operation,result_status,continuation_id,
                 delegation_id,action_id,job_id,worker_id,job_lease_expires_at,
                 continuation_fencing_token,runtime_host_generation,runtime_host_instance_id,
                 runtime_fencing_token,state_root_generation,runtime_authority_provenance_id,
                 runtime_actor_identity,policy_revision,global_stop_epoch,technical_quota_policy_digest,
                 technical_quota_snapshot_id,technical_resource_reservation_set_json,
                 technical_resource_reservation_set_digest,technical_resource_evidence_digest,recorded_at
               ) VALUES(?1,?2,?3,'reconcile','uncertain',?4,'test-delegation',?5,?6,
                 'gateway-duplicate-worker',?7,1,1,'gateway-test-host',1,1,?8,
                 'gateway-duplicate-runtime',1,0,'sha256:test-duplicate-quota',NULL,'[]',
                 'sha256:test-duplicate-reservations',?9,?10)"#,
            params![
                settle_event_id,
                claim_event_id,
                claim_receipt_id,
                continuation_id,
                action_id,
                job_id,
                now,
                runtime_authority,
                format!("sha256:{}", "3".repeat(64)),
                now + 1,
            ],
        )
        .unwrap();
        conn.execute(
            "UPDATE execass_continuations SET status='uncertain',lease_owner=NULL,lease_expires_at=NULL,updated_at=?2 WHERE continuation_id=?1 AND status='executing'",
            params![continuation_id, now + 1],
        )
        .unwrap();
        effect_id
    }

    fn assert_zero_product_rows(paths: &AppPaths) {
        for table in [
            "execass_logical_effects",
            "execass_continuations",
            "execass_receipts",
            "execass_outbox_events",
            "jobs",
        ] {
            assert_eq!(count(paths, table), 0, "unexpected rows in {table}");
        }
    }

    fn assert_atomic_confirmation_rows(paths: &AppPaths) {
        assert_eq!(count(paths, "execass_logical_effects"), 0);
        assert_eq!(count(paths, "execass_continuations"), 1);
        assert_eq!(count(paths, "execass_receipts"), 1);
        assert_eq!(count(paths, "execass_outbox_events"), 1);
        assert_eq!(count(paths, "jobs"), 0);
    }

    #[test]
    fn scheduler_continuation_route_claims_revalidates_and_waits_without_generic_success() {
        let fixture = fixture("scheduler-fenced-route");
        let event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "scheduler-fenced-confirmation",
            )
            .unwrap();
        assert!(matches!(
            fixture
                .runtime
                .resolve(
                    &event,
                    &fixture.binding.decision_id,
                    &fixture.binding.selected_logical_action_id,
                )
                .unwrap(),
            DangerConfirmationResolutionOutcome::Confirmed(_)
        ));
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        fixture
            .store
            .materialize_runnable_continuation_jobs(now, 10)
            .unwrap();
        let scheduler = Storage::from_paths(&fixture.paths);
        let job = scheduler
            .acquire_due_jobs("gateway-fenced-worker", now + 1, 30_000, 10)
            .unwrap()
            .remove(0);
        let claim = fixture
            .runtime
            .claim_scheduler_continuation(&job, "gateway-fenced-worker", now + 1)
            .unwrap();
        let ContinuationClaimOutcome::Claimed(claimed) = claim else {
            panic!("scheduler route did not claim: {claim:?}")
        };
        assert_eq!(
            fixture
                .runtime
                .validate_scheduler_continuation_pre_dispatch(&claimed.identity, now + 2)
                .unwrap(),
            ContinuationDispatchValidationOutcome::Valid
        );
        let evidence = job_evidence(&fixture.paths, &claimed.identity);
        let expiry = fixture
            .runtime
            .build_technical_resource_lifecycle_command(
                &claimed.identity,
                now + 3,
                TechnicalResourceLifecycleResolution::ExpireUndispatched,
                evidence.clone(),
                vec![],
            )
            .unwrap();
        let absent = fixture
            .runtime
            .build_technical_resource_lifecycle_command(
                &claimed.identity,
                now + 3,
                TechnicalResourceLifecycleResolution::ReconcileAbsent,
                evidence.clone(),
                vec![],
            )
            .unwrap();
        let exact_actual = TechnicalResourceActualInput {
            reservation_id: "reservation-exact".to_string(),
            amount_actual: 7,
            evidence_digest: "sha256:present-proof".to_string(),
        };
        let present = fixture
            .runtime
            .build_technical_resource_lifecycle_command(
                &claimed.identity,
                now + 3,
                TechnicalResourceLifecycleResolution::ReconcilePresent,
                evidence.clone(),
                vec![exact_actual.clone()],
            )
            .unwrap();
        assert_eq!(expiry.identity, claimed.identity);
        assert_eq!(expiry.trusted_now, now + 3);
        assert_eq!(present.technical_resource_actuals, vec![exact_actual]);
        assert_eq!(expiry.receipt.actor.actor_type, StorageActorType::Runtime);
        assert_eq!(
            expiry.receipt.runtime.host_generation,
            claimed.identity.runtime_host_generation
        );
        assert_eq!(
            expiry.receipt.runtime.host_instance_id,
            claimed.identity.runtime_host_instance_id
        );
        assert_eq!(
            expiry.receipt.runtime.fencing_token,
            claimed.identity.runtime_fencing_token
        );
        assert_eq!(
            expiry.write.idempotency_key,
            expiry.outbox_event.duplicate_identity
        );
        assert_eq!(
            expiry.write.correlation_id,
            expiry.outbox_event.correlation_id
        );
        assert_eq!(expiry.write.causation_id, expiry.outbox_event.causation_id);
        assert_eq!(
            expiry.receipt.causation_event_id,
            expiry.outbox_event.event_id
        );
        assert_ne!(expiry.outbox_event.event_id, absent.outbox_event.event_id);
        assert_ne!(absent.outbox_event.event_id, present.outbox_event.event_id);
        assert_eq!(expiry.receipt.evidence, vec![evidence.clone()]);
        assert_eq!(absent.receipt.evidence, vec![evidence.clone()]);
        assert_eq!(present.receipt.evidence, vec![evidence.clone()]);

        let receipt_count = count(&fixture.paths, "execass_receipts");
        let outbox_count = count(&fixture.paths, "execass_outbox_events");
        let error = fixture
            .runtime
            .resolve_scheduler_continuation_technical_resources(
                &claimed.identity,
                now + 3,
                TechnicalResourceLifecycleResolution::ExpireUndispatched,
                evidence,
                vec![],
            )
            .expect_err("fixture has no technical reservation set to expire");
        assert!(error
            .to_string()
            .contains("requires a nonempty reservation set"));
        assert_eq!(count(&fixture.paths, "execass_receipts"), receipt_count);
        assert_eq!(count(&fixture.paths, "execass_outbox_events"), outbox_count);
        assert!(matches!(
            fixture
                .runtime
                .settle_scheduler_continuation_adapter_unavailable(&claimed.identity, now + 4)
                .unwrap(),
            ContinuationSettleOutcome::Settled(_)
        ));
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT status FROM execass_continuations LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "waiting"
        );
        assert_eq!(
            conn.query_row(
                "SELECT enabled||':'||COALESCE(lease_owner,'none') FROM jobs LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "0:none"
        );
        assert_eq!(count(&fixture.paths, "job_runs"), 0);
    }

    #[test]
    fn scheduler_restart_defers_possible_invocation_without_second_dispatch() {
        let fixture = fixture("scheduler-restart-recovery");
        let event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "scheduler-restart-confirmation",
            )
            .unwrap();
        assert!(matches!(
            fixture.runtime.resolve(
                &event,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            ),
            Ok(DangerConfirmationResolutionOutcome::Confirmed(_))
        ));
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        attach_test_technical_resources(&fixture, "scheduler-restart", now, false, "planned");
        fixture
            .store
            .materialize_runnable_continuation_jobs(now + 1, 8)
            .unwrap();
        let scheduler = Storage::from_paths(&fixture.paths);
        let job = scheduler
            .acquire_due_jobs("gateway-crashed-worker", now + 2, 30_000, 8)
            .unwrap()
            .remove(0);
        let ContinuationClaimOutcome::Claimed(claimed) = fixture
            .runtime
            .claim_scheduler_continuation(&job, "gateway-crashed-worker", now + 2)
            .unwrap()
        else {
            panic!("scheduler did not claim resource-bound continuation")
        };
        assert_eq!(claimed.technical_resource_reservations.len(), 1);
        assert_eq!(
            fixture
                .runtime
                .validate_scheduler_continuation_pre_dispatch(&claimed.identity, now + 3)
                .unwrap(),
            ContinuationDispatchValidationOutcome::Valid
        );

        let restarted =
            ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED).unwrap();
        let new_host = fixture
            .store
            .activate_runtime_host(
                restarted.authority_identity(),
                "gateway-restarted-runtime-host",
                now + 4,
            )
            .unwrap();
        assert_ne!(
            new_host.generation,
            claimed.identity.runtime_host_generation
        );
        let report = restarted
            .recover_scheduler_technical_resources(now + 5, 8)
            .unwrap();
        assert_eq!(
            report,
            SchedulerTechnicalResourceRecoveryReport {
                applied: 0,
                replayed: 0,
                stale: 0,
                deferred_pending_recorder: 1,
            }
        );
        assert_eq!(
            restarted
                .recover_scheduler_technical_resources(now + 6, 8)
                .unwrap(),
            SchedulerTechnicalResourceRecoveryReport {
                deferred_pending_recorder: 1,
                ..Default::default()
            }
        );

        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT state FROM execass_logical_effects WHERE logical_effect_id='gateway-recovery-effect-scheduler-restart'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "invoking"
        );
        assert_eq!(
            conn.query_row(
                "SELECT status FROM execass_technical_resource_reservations",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "reserved"
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM execass_continuation_operation_history WHERE operation='claim'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM execass_continuation_operation_history WHERE operation='recover'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        drop(conn);
        fixture
            .store
            .materialize_runnable_continuation_jobs(now + 7, 8)
            .unwrap();
        assert!(scheduler
            .acquire_due_jobs("gateway-second-worker", now + 7, 30_000, 8)
            .unwrap()
            .is_empty());
        assert!(matches!(
            fixture
                .runtime
                .validate_scheduler_continuation_pre_dispatch(&claimed.identity, now + 7)
                .unwrap(),
            ContinuationDispatchValidationOutcome::Stale { .. }
        ));
    }

    #[test]
    fn scheduler_effect_begin_replay_is_non_authorizing_after_restart() {
        let fixture = fixture("scheduler-effect-begin-replay");
        let event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "scheduler-effect-begin-confirmation",
            )
            .unwrap();
        fixture
            .runtime
            .resolve(
                &event,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            )
            .unwrap();
        let now = i64::try_from(server_time_ms().unwrap()).unwrap();
        attach_test_technical_resources(&fixture, "scheduler-effect-begin", now, false, "planned");
        fixture
            .store
            .materialize_runnable_continuation_jobs(now + 1, 8)
            .unwrap();
        let scheduler = Storage::from_paths(&fixture.paths);
        let job = scheduler
            .acquire_due_jobs("gateway-effect-begin-worker", now + 2, 30_000, 8)
            .unwrap()
            .remove(0);
        let ContinuationClaimOutcome::Claimed(claimed) = fixture
            .runtime
            .claim_scheduler_continuation(&job, "gateway-effect-begin-worker", now + 2)
            .unwrap()
        else {
            panic!("scheduler did not claim effect continuation")
        };
        let PrepareProviderAttemptOutcome::Prepared(prepared) = fixture
            .runtime
            .prepare_scheduler_effect_attempt(&claimed.identity, now + 3, None)
            .unwrap()
        else {
            panic!("effect attempt was not prepared")
        };
        assert!(matches!(
            fixture.runtime.begin_scheduler_effect_invocation(
                &prepared.attempt_id,
                &claimed.identity,
                now + 4,
            ).unwrap(),
            BeginProviderAttemptInvocationOutcome::Began(_)
        ));
        let restarted =
            ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED).unwrap();
        assert!(matches!(
            restarted
                .begin_scheduler_effect_invocation(
                    &prepared.attempt_id,
                    &claimed.identity,
                    now + 5,
                )
                .unwrap(),
            BeginProviderAttemptInvocationOutcome::AlreadyInvoking(_)
        ));
    }

    #[test]
    fn exact_local_confirmation_creates_one_grant_and_restart_replays_it() {
        let fixture = fixture("local-success");
        let first_event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "local-confirm-1",
            )
            .expect("verify first local owner confirmation");
        let DangerConfirmationResolutionOutcome::Confirmed(first_grant) = fixture
            .runtime
            .resolve(
                &first_event,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            )
            .expect("resolve exact local confirmation")
        else {
            panic!("first confirmation did not create the grant");
        };
        assert_eq!(
            count(&fixture.paths, "execass_confirmation_attestations"),
            1
        );
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            1
        );
        assert_atomic_confirmation_rows(&fixture.paths);

        let reopened = ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED)
            .expect("reopen test authority after restart");
        let replay_event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "local-confirm-2",
            )
            .expect("verify fresh replay owner response");
        let DangerConfirmationResolutionOutcome::Replayed(replayed) = reopened
            .resolve(
                &replay_event,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            )
            .expect("replay resolved confirmation after restart")
        else {
            panic!("restart replay did not return the recorded grant");
        };
        assert_eq!(replayed, first_grant);
        assert_eq!(
            count(&fixture.paths, "execass_confirmation_attestations"),
            1
        );
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            1
        );
        assert_atomic_confirmation_rows(&fixture.paths);
    }

    #[test]
    fn verified_response_is_retryable_after_pretransaction_crash_and_resolves_once() {
        let fixture = fixture("pretransaction-crash");
        let binding = current(&fixture.binding);
        let _lost_event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &binding,
                DecisionResult::ConfirmAndContinue,
                "same-owner-response",
            )
            .expect("verify response before simulated crash");
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            0
        );

        let restarted = ExecAssConfirmationRuntime::open_for_test(fixture.store.clone(), TEST_SEED)
            .expect("reopen authority after simulated crash");
        let retried_event = fixture
            .gate
            .issue_test_local_confirmation_event(
                &binding,
                DecisionResult::ConfirmAndContinue,
                "same-owner-response",
            )
            .expect("same owner response remains valid until storage consumes it");
        assert!(matches!(
            restarted.resolve(
                &retried_event,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            ),
            Ok(DangerConfirmationResolutionOutcome::Confirmed(_))
        ));
        assert_eq!(
            count(&fixture.paths, "execass_confirmation_attestations"),
            1
        );
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            1
        );
        assert_atomic_confirmation_rows(&fixture.paths);
    }

    #[test]
    fn every_storage_binding_mutation_and_plain_intent_digest_creates_zero_grants() {
        for mutation in 0..10 {
            let fixture = fixture(&format!("mutation-{mutation}"));
            let mut hostile = current(&fixture.binding);
            match mutation {
                0 => hostile.decision_id.push_str("-other"),
                1 => hostile.decision_revision += 1,
                2 => {
                    hostile.normalized_intent_digest =
                        sha256_hex(fixture.binding.normalized_intent.as_bytes())
                }
                3 => hostile.policy_revision += 1,
                4 => hostile.canonical_manifest_digest = "d".repeat(64),
                5 => hostile.selected_logical_action_id.push_str("-other"),
                6 => hostile.presented_action_digest = "e".repeat(64),
                7 => hostile.declared_consequence_digest = "f".repeat(64),
                8 => hostile.challenge_digest = "1".repeat(64),
                _ => hostile.expires_at_ms += 1,
            }
            let event = fixture
                .gate
                .issue_test_local_confirmation_event(
                    &hostile,
                    DecisionResult::ConfirmAndContinue,
                    &format!("mutation-correlation-{mutation}"),
                )
                .expect("hostile event is internally self-consistent");
            assert!(fixture
                .runtime
                .resolve(
                    &event,
                    &fixture.binding.decision_id,
                    &fixture.binding.selected_logical_action_id,
                )
                .is_err());
            assert_eq!(
                count(&fixture.paths, "execass_confirmation_attestations"),
                0
            );
            assert_eq!(
                count(&fixture.paths, "execass_accepted_confirmation_grants"),
                0
            );
            assert_zero_product_rows(&fixture.paths);
        }
    }

    #[test]
    fn dangerous_revise_and_decline_resolve_once_and_exactly_replay_without_continuation() {
        for result in [DecisionResult::Revise, DecisionResult::Decline] {
            let label = match result {
                DecisionResult::Revise => "revise",
                DecisionResult::Decline => "decline",
                DecisionResult::Stop => unreachable!(),
                DecisionResult::ConfirmAndContinue => unreachable!(),
            };
            let fixture = fixture(label);
            let binding = fixture.binding.clone();
            let event = fixture
                .gate
                .issue_test_local_confirmation_event_with_request(
                    &current(&binding),
                    result,
                    &format!("result-{label}"),
                    &format!("idempotency-{label}"),
                    None,
                    None,
                )
                .expect("verify exact non-confirming owner result");
            assert_eq!(
                fixture
                    .runtime
                    .resolve_typed(
                        &event,
                        &binding.decision_id,
                        &binding.selected_logical_action_id,
                    )
                    .expect("apply typed non-affirmative result"),
                TypedDecisionResolutionOutcome::Applied
            );
            assert_eq!(
                fixture
                    .runtime
                    .resolve_typed(
                        &event,
                        &binding.decision_id,
                        &binding.selected_logical_action_id,
                    )
                    .expect("replay exact typed result"),
                TypedDecisionResolutionOutcome::Replayed
            );
            assert_eq!(count(&fixture.paths, "execass_continuations"), 0);
            assert_eq!(count(&fixture.paths, "execass_logical_effects"), 0);
            assert_eq!(
                count(&fixture.paths, "execass_accepted_confirmation_grants"),
                0
            );
            assert_eq!(count(&fixture.paths, "execass_receipts"), 1);
            assert_eq!(count(&fixture.paths, "execass_outbox_events"), 1);
        }
    }

    #[test]
    fn typed_stop_decision_resolves_and_replays_without_continuation_or_grant() {
        let fixture = fixture("typed-stop");
        let canonical_binding = fixture.binding.clone();
        let stop_decision_id = "decision-typed-stop-real";
        let conn = Connection::open(&fixture.paths.db_path).expect("open typed stop fixture");
        conn.execute(
            "INSERT INTO execass_decisions (decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,decision_kind,status,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,consequence,alternatives_json,idempotency_key,requested_at) VALUES (?1,'test-delegation',2,2,1,1,'stop','pending',?2,?3,?4,?5,'{}','[]',NULL,NULL,'{}','stop this exact waiting branch',?6,'[\"stop\"]','typed-stop-persisted-idempotency',?7)",
            params![
                stop_decision_id,
                canonical_binding.exact_selected_action_json,
                canonical_binding.selected_logical_action_id,
                canonical_binding.manifest_digest,
                "b".repeat(64),
                canonical_binding.declared_consequence,
                canonical_binding.requested_at,
            ],
        )
        .expect("insert a real typed stop decision");
        drop(conn);
        let binding = fixture
            .store
            .read_decision_resolution_binding(
                stop_decision_id,
                &canonical_binding.selected_logical_action_id,
            )
            .expect("read typed stop binding")
            .expect("typed stop binding exists");
        assert_eq!(binding.decision_kind, StorageDecisionKind::Stop);
        let current = CurrentDecisionBinding {
            decision_id: binding.decision_id.clone(),
            decision_revision: u64::try_from(binding.decision_revision).unwrap(),
            normalized_intent_digest: canonical_intent_digest(&binding.normalized_intent).unwrap(),
            policy_revision: binding.policy_revision,
            canonical_manifest_digest: binding.manifest_digest.clone(),
            selected_logical_action_id: binding.selected_logical_action_id.clone(),
            presented_action_digest: binding.exact_selected_action_digest.clone(),
            declared_consequence_digest: binding.declared_consequence_digest.clone(),
            challenge_digest: binding.challenge_nonce_digest.clone(),
            expires_at_ms: binding.expires_at,
        };
        let event = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current,
                DecisionResult::Stop,
                "typed-stop-correlation",
                "typed-stop-idempotency",
                None,
                None,
            )
            .expect("verify typed stop result");
        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &event,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("apply typed stop"),
            TypedDecisionResolutionOutcome::Applied
        );
        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &event,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("replay typed stop"),
            TypedDecisionResolutionOutcome::Replayed
        );
        assert_eq!(count(&fixture.paths, "execass_continuations"), 0);
        assert_eq!(count(&fixture.paths, "execass_logical_effects"), 0);
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            0
        );
        assert_eq!(count(&fixture.paths, "execass_receipts"), 1);
        assert_eq!(count(&fixture.paths, "execass_outbox_events"), 1);
    }

    #[test]
    fn ordinary_typed_confirmation_continues_once_replays_and_rejects_binding_drift() {
        let fixture = fixture("ordinary-typed-confirm");
        let decision_id = "decision-ordinary-typed-confirm-real";
        insert_typed_test_decision(
            &fixture,
            decision_id,
            "owner_configured_checkpoint",
            "ordinary-typed-persisted-idempotency",
        );
        let binding = fixture
            .store
            .read_decision_resolution_binding(
                decision_id,
                &fixture.binding.selected_logical_action_id,
            )
            .expect("read ordinary typed decision")
            .expect("ordinary typed decision exists");
        assert_eq!(
            binding.decision_kind,
            StorageDecisionKind::OwnerConfiguredCheckpoint
        );
        let action = fixture
            .store
            .read_waiting_decision_action(&binding.decision_id, &binding.selected_logical_action_id)
            .expect("read exact waiting action")
            .expect("ordinary typed action is waiting");
        let receipt_context = fixture
            .store
            .read_decision_receipt_context(&binding.decision_id, binding.requested_at)
            .expect("read exact receipt context")
            .expect("ordinary typed receipt context exists");
        let event = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current_typed(&binding),
                DecisionResult::ConfirmAndContinue,
                "ordinary-typed-correlation",
                "ordinary-typed-idempotency",
                None,
                None,
            )
            .expect("verify ordinary typed confirmation");

        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &event,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("apply ordinary typed confirmation"),
            TypedDecisionResolutionOutcome::Applied
        );
        let first = fixture
            .store
            .read_api_delegation_detail(&binding.delegation_id)
            .expect("read applied continuation")
            .expect("delegation detail exists")
            .continuations
            .into_iter()
            .find(|candidate| candidate.causation_id == binding.decision_id)
            .expect("ordinary confirmation created its continuation");
        assert_eq!(first.action_id, action.action_id);
        assert_eq!(
            first.target_delegation_revision,
            receipt_context.delegation_revision
        );
        assert_eq!(first.target_plan_revision, receipt_context.plan_revision);
        assert_eq!(first.stop_epoch, action.stop_epoch);
        assert_eq!(first.global_stop_epoch, receipt_context.global_stop_epoch);
        assert_eq!(
            first.host_generation,
            receipt_context.runtime_host_generation
        );

        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &event,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("replay ordinary typed confirmation"),
            TypedDecisionResolutionOutcome::Replayed
        );
        let replayed = fixture
            .store
            .read_api_delegation_detail(&binding.delegation_id)
            .expect("read replayed continuation")
            .expect("delegation detail exists")
            .continuations;
        assert_eq!(replayed, vec![first]);

        let changed_material = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current_typed(&binding),
                DecisionResult::ConfirmAndContinue,
                "ordinary-typed-correlation",
                "ordinary-typed-changed-idempotency",
                None,
                None,
            )
            .expect("authenticate changed request material");
        assert!(fixture
            .runtime
            .resolve_typed(
                &changed_material,
                &binding.decision_id,
                &binding.selected_logical_action_id,
            )
            .is_err());

        let mut changed_revision = current_typed(&binding);
        changed_revision.decision_revision += 1;
        let changed_revision = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &changed_revision,
                DecisionResult::ConfirmAndContinue,
                "ordinary-typed-correlation",
                "ordinary-typed-idempotency",
                None,
                None,
            )
            .expect("authenticate changed decision revision");
        assert!(fixture
            .runtime
            .resolve_typed(
                &changed_revision,
                &binding.decision_id,
                &binding.selected_logical_action_id,
            )
            .is_err());
        assert_eq!(count(&fixture.paths, "execass_continuations"), 1);
        assert_eq!(count(&fixture.paths, "execass_logical_effects"), 0);
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            0
        );
        assert_eq!(count(&fixture.paths, "execass_receipts"), 1);
        assert_eq!(count(&fixture.paths, "execass_outbox_events"), 1);
    }

    #[test]
    fn duplicate_risk_typed_confirmation_creates_one_fresh_effect_and_replays_exactly() {
        let fixture = fixture("duplicate-risk-typed-confirm");
        let dangerous = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current(&fixture.binding),
                DecisionResult::ConfirmAndContinue,
                "duplicate-risk-danger-correlation",
                "duplicate-risk-danger-idempotency",
                None,
                None,
            )
            .expect("verify native dangerous owner proof");
        fixture
            .runtime
            .resolve(
                &dangerous,
                &fixture.binding.decision_id,
                &fixture.binding.selected_logical_action_id,
            )
            .expect("accept exact dangerous grant");
        let now = i64::try_from(server_time_ms().expect("test clock")).expect("clock range");
        attach_test_technical_resources(
            &fixture,
            "duplicate-risk-typed-confirm",
            now,
            true,
            "invoking",
        );
        let predecessor_effect =
            mark_test_effect_outcome_unknown(&fixture, "duplicate-risk-typed-confirm", now + 1);
        let decision_id = "decision-duplicate-risk-typed-confirm-real";
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        drop(conn);
        fixture
            .store
            .prepare_test_duplicate_risk_decision(
                decision_id,
                "duplicate-risk-successor-action",
                &fixture.binding.selected_logical_action_id,
                &predecessor_effect,
                "duplicate-risk-persisted-idempotency",
                now,
            )
            .expect("prepare storage-owned duplicate-risk recovery branch");
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        let grant_before: String = conn
            .query_row(
                "SELECT hex(CAST(json_array(grant_id,delegation_id,decision_id,confirmed_logical_action_identity,payload_and_material_operands_digest,accepted_by_authority_provenance_id,confirmation_attestation_digest,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id) AS BLOB)) FROM execass_accepted_confirmation_grants",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        let binding = fixture
            .store
            .read_decision_resolution_binding(decision_id, "duplicate-risk-successor-action")
            .expect("read duplicate-risk decision")
            .expect("duplicate-risk decision exists");
        let event = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current_typed(&binding),
                DecisionResult::ConfirmAndContinue,
                "duplicate-risk-correlation",
                "duplicate-risk-idempotency",
                None,
                None,
            )
            .expect("verify duplicate-risk confirmation request");
        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &event,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("apply duplicate-risk confirmation"),
            TypedDecisionResolutionOutcome::Applied
        );
        let request = event
            .verified_request_binding()
            .expect("verified retry request binding");
        let resolution_identity = nonaffirmative_resolution_identity(&event, request);
        let persisted = fixture
            .store
            .prepare_duplicate_risk_successor(
                &binding.decision_id,
                &binding.selected_logical_action_id,
                &resolution_identity,
                request.observed_at_ms,
                999,
                999,
            )
            .expect("reconstruct persisted successor under changed runtime coordinates")
            .expect("persisted successor exists");
        assert_eq!(persisted.continuation.host_generation, 1);
        assert!(fixture
            .store
            .prepare_duplicate_risk_successor(
                &binding.decision_id,
                &binding.selected_logical_action_id,
                "changed-verified-resolution-identity",
                request.observed_at_ms,
                999,
                999,
            )
            .is_err());
        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &event,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("replay duplicate-risk confirmation"),
            TypedDecisionResolutionOutcome::Replayed
        );
        let changed = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current_typed(&binding),
                DecisionResult::ConfirmAndContinue,
                "duplicate-risk-correlation",
                "duplicate-risk-changed-idempotency",
                None,
                None,
            )
            .expect("verify changed native owner proof");
        assert!(fixture
            .runtime
            .resolve_typed(
                &changed,
                &binding.decision_id,
                &binding.selected_logical_action_id,
            )
            .is_err());
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        let grant_after: String = conn
            .query_row(
                "SELECT hex(CAST(json_array(grant_id,delegation_id,decision_id,confirmed_logical_action_identity,payload_and_material_operands_digest,accepted_by_authority_provenance_id,confirmation_attestation_digest,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id) AS BLOB)) FROM execass_accepted_confirmation_grants",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(grant_after, grant_before);
        assert_eq!(count(&fixture.paths, "execass_continuations"), 2);
        assert_eq!(count(&fixture.paths, "execass_logical_effects"), 2);
        assert_eq!(
            count(&fixture.paths, "execass_duplicate_risk_successors"),
            1
        );
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            1
        );
        assert_eq!(count(&fixture.paths, "execass_receipts"), 2);
        assert_eq!(count(&fixture.paths, "execass_outbox_events"), 4);
    }

    #[test]
    fn nonaffirmative_replay_rejects_changed_material_and_decision_revision() {
        let fixture = fixture("typed-hostile-replay");
        let binding = fixture.binding.clone();
        let exact = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current(&binding),
                DecisionResult::Decline,
                "typed-hostile-correlation",
                "typed-hostile-idempotency",
                None,
                None,
            )
            .expect("verify exact decline");
        assert_eq!(
            fixture
                .runtime
                .resolve_typed(
                    &exact,
                    &binding.decision_id,
                    &binding.selected_logical_action_id,
                )
                .expect("apply exact decline"),
            TypedDecisionResolutionOutcome::Applied
        );

        let changed_material = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &current(&binding),
                DecisionResult::Decline,
                "typed-hostile-correlation",
                "changed-idempotency",
                None,
                None,
            )
            .expect("verify changed but authenticated request material");
        assert!(fixture
            .runtime
            .resolve_typed(
                &changed_material,
                &binding.decision_id,
                &binding.selected_logical_action_id,
            )
            .is_err());

        let mut changed_revision = current(&binding);
        changed_revision.decision_revision += 1;
        let changed_revision = fixture
            .gate
            .issue_test_local_confirmation_event_with_request(
                &changed_revision,
                DecisionResult::Decline,
                "typed-hostile-correlation",
                "typed-hostile-idempotency",
                None,
                None,
            )
            .expect("verify internally consistent changed revision");
        assert!(fixture
            .runtime
            .resolve_typed(
                &changed_revision,
                &binding.decision_id,
                &binding.selected_logical_action_id,
            )
            .is_err());
        assert_eq!(count(&fixture.paths, "execass_receipts"), 1);
        assert_eq!(count(&fixture.paths, "execass_outbox_events"), 1);
        assert_eq!(count(&fixture.paths, "execass_continuations"), 0);
        assert_eq!(
            count(&fixture.paths, "execass_accepted_confirmation_grants"),
            0
        );
    }

    #[test]
    fn exact_remote_owner_binding_confirms_and_provider_swap_fails_closed() {
        let valid = fixture("remote-success");
        valid
            .store
            .reconcile_remote_confirmation_ingress(
                &[carsinos_storage::execass::RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                valid.binding.requested_at,
            )
            .expect("bind exact test remote ingress");
        let event = valid
            .gate
            .issue_test_remote_confirmation_event(TestRemoteConfirmationEventInput {
                provider: "telegram",
                owner_id: "owner-telegram",
                adapter_instance_id: "telegram-listener-1",
                source_message_id: "telegram-message-1",
                provider_event_id: "telegram-event-1",
                current: &current(&valid.binding),
                decision_result: DecisionResult::ConfirmAndContinue,
                request_correlation_id: "telegram-correlation-1",
            })
            .expect("verify exact remote owner event");
        assert!(matches!(
            valid.runtime.resolve(
                &event,
                &valid.binding.decision_id,
                &valid.binding.selected_logical_action_id,
            ),
            Ok(DangerConfirmationResolutionOutcome::Confirmed(_))
        ));
        assert_eq!(
            count(&valid.paths, "execass_accepted_confirmation_grants"),
            1
        );
        assert_atomic_confirmation_rows(&valid.paths);

        let swapped = fixture("remote-swap");
        swapped
            .store
            .reconcile_remote_confirmation_ingress(
                &[carsinos_storage::execass::RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                swapped.binding.requested_at,
            )
            .expect("bind only Telegram ingress");
        let discord = swapped
            .gate
            .issue_test_remote_confirmation_event(TestRemoteConfirmationEventInput {
                provider: "discord",
                owner_id: "owner-discord",
                adapter_instance_id: "discord-listener-1",
                source_message_id: "discord-message-1",
                provider_event_id: "discord-event-1",
                current: &current(&swapped.binding),
                decision_result: DecisionResult::ConfirmAndContinue,
                request_correlation_id: "discord-correlation-1",
            })
            .expect("verify configured Discord owner event");
        assert!(swapped
            .runtime
            .resolve(
                &discord,
                &swapped.binding.decision_id,
                &swapped.binding.selected_logical_action_id,
            )
            .is_err());
        assert_eq!(
            count(&swapped.paths, "execass_confirmation_attestations"),
            0
        );
        assert_eq!(
            count(&swapped.paths, "execass_accepted_confirmation_grants"),
            0
        );
        assert_zero_product_rows(&swapped.paths);
    }

    #[test]
    fn remote_owner_binding_rotation_and_revocation_are_append_only_and_effective() {
        let rotated = fixture("remote-rotation");
        let first_time = rotated.binding.requested_at;
        rotated
            .store
            .reconcile_remote_confirmation_ingress(
                &[carsinos_storage::execass::RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                first_time,
            )
            .expect("enroll first remote owner generation");
        rotated
            .store
            .reconcile_remote_confirmation_ingress(
                &[carsinos_storage::execass::RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram-2".to_string(),
                    authenticated_ingress: "telegram-listener-2".to_string(),
                }],
                first_time + 1,
            )
            .expect("rotate remote owner generation");
        let stale = rotated
            .gate
            .issue_test_remote_confirmation_event(TestRemoteConfirmationEventInput {
                provider: "telegram",
                owner_id: "owner-telegram",
                adapter_instance_id: "telegram-listener-1",
                source_message_id: "stale-message",
                provider_event_id: "stale-event",
                current: &current(&rotated.binding),
                decision_result: DecisionResult::ConfirmAndContinue,
                request_correlation_id: "stale-correlation",
            })
            .expect("old provider event remains actor-gate valid for storage rejection proof");
        assert!(rotated
            .runtime
            .resolve(
                &stale,
                &rotated.binding.decision_id,
                &rotated.binding.selected_logical_action_id,
            )
            .is_err());

        let current_gate = ExecAssActorGate::new(
            Some(b"gateway-confirmation-runtime-test-secret".to_vec()),
            HashMap::from([("telegram".to_string(), "owner-telegram-2".to_string())]),
            rotated._temp.path().join("rotated-actor-replay"),
        );
        let current_event = current_gate
            .issue_test_remote_confirmation_event(TestRemoteConfirmationEventInput {
                provider: "telegram",
                owner_id: "owner-telegram-2",
                adapter_instance_id: "telegram-listener-2",
                source_message_id: "current-message",
                provider_event_id: "current-event",
                current: &current(&rotated.binding),
                decision_result: DecisionResult::ConfirmAndContinue,
                request_correlation_id: "current-correlation",
            })
            .expect("new provider generation verifies");
        assert!(matches!(
            rotated.runtime.resolve(
                &current_event,
                &rotated.binding.decision_id,
                &rotated.binding.selected_logical_action_id,
            ),
            Ok(DangerConfirmationResolutionOutcome::Confirmed(_))
        ));

        let revoked = fixture("remote-revocation");
        revoked
            .store
            .reconcile_remote_confirmation_ingress(
                &[carsinos_storage::execass::RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                revoked.binding.requested_at,
            )
            .expect("enroll revocation fixture");
        revoked
            .store
            .reconcile_remote_confirmation_ingress(&[], revoked.binding.requested_at + 1)
            .expect("append remote retirement tombstone");
        let retired_event = revoked
            .gate
            .issue_test_remote_confirmation_event(TestRemoteConfirmationEventInput {
                provider: "telegram",
                owner_id: "owner-telegram",
                adapter_instance_id: "telegram-listener-1",
                source_message_id: "retired-message",
                provider_event_id: "retired-event",
                current: &current(&revoked.binding),
                decision_result: DecisionResult::ConfirmAndContinue,
                request_correlation_id: "retired-correlation",
            })
            .expect("retired event reaches storage rejection proof");
        assert!(revoked
            .runtime
            .resolve(
                &retired_event,
                &revoked.binding.decision_id,
                &revoked.binding.selected_logical_action_id,
            )
            .is_err());
        assert_eq!(
            count(&revoked.paths, "execass_accepted_confirmation_grants"),
            0
        );
        revoked
            .store
            .reconcile_remote_confirmation_ingress(
                &[carsinos_storage::execass::RemoteOwnerConfirmationIngress {
                    provider: "telegram".to_string(),
                    owner_account_id: "owner-telegram".to_string(),
                    authenticated_ingress: "telegram-listener-1".to_string(),
                }],
                revoked.binding.requested_at + 2,
            )
            .expect("re-enroll the exact retired owner/adapter as a new generation");
        assert!(matches!(
            revoked.runtime.resolve(
                &retired_event,
                &revoked.binding.decision_id,
                &revoked.binding.selected_logical_action_id,
            ),
            Ok(DangerConfirmationResolutionOutcome::Confirmed(_))
        ));
    }

    #[test]
    fn production_build_has_no_test_authority_feature_by_default() {
        let manifest = include_str!("../Cargo.toml");
        let production_dependencies = manifest
            .split("[dev-dependencies]")
            .next()
            .expect("gateway manifest has a production dependency section");
        assert!(!production_dependencies.contains("execass-test-confirmation-runtime"));
        let source = include_str!("execass_confirmation_runtime.rs");
        let prohibited_secret = ["CARSINOS", "EXECASS", "CONFIRMATION", "SIGNING", "KEY"].join("_");
        let prohibited_env_read = ["std::env::", "var"].concat();
        assert!(!source.contains(&prohibited_secret));
        assert!(!source.contains(&prohibited_env_read));
        let storage_store = include_str!("../../carsinos-storage/src/execass/store.rs");
        let storage_exports = include_str!("../../carsinos-storage/src/execass/mod.rs");
        assert!(!storage_store.contains("confirmation_authority_signing_key"));
        assert!(!storage_exports.contains("ConfirmationAuthoritySigningKey"));
        assert!(!storage_exports.contains("sign_confirmation_attestation"));
        let _project_drive_marker = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    }
}
