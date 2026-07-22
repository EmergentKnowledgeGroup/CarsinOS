//! Production-off typed builders for the EA-312 composed reference fixture.
//!
//! The builders seed only deterministic authorities/foundations. Every state
//! transition is routed through the same typed lifecycle, verifier,
//! completion, continuation, routine, receipt, and projection services used by
//! the runtime. No trigger is disabled and no lifecycle row is updated by SQL.

use super::receipt_integrity::IntegrityStatus;
use super::*;
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{
    issue_test_local_owner_authority, TestLocalOwnerAuthorityInput, VerifiedOwnerAuthority,
};
use carsinos_core::execass_danger::{
    bind_danger_admission, danger_admission_signing_bytes, issue_test_verified_danger_metadata,
    match_known_danger, saved_routine_stable_leaf_digest, KnownDangerMatchInput,
    SignedDangerAdmissionProof,
};
use carsinos_core::execass_manifest::{
    compile_dispatch, rebind_persisted_manifest_for_routine_occurrence, CanonicalLeafManifest,
    CanonicalValue, DispatchAction, DispatchNode, DispatchTree, ManifestCompilation,
    ResolvedLeafInput, RoutineOccurrenceLeafBinding, ServerResolutionRegistry, TargetSnapshotInput,
    ToolIdentityInput,
};
use carsinos_core::execass_policy::{
    authorize_exact_owner_leaf, issue_test_objective_technical_validity_proof,
    ExactOwnerActionAuthority, ExactOwnerAuthorityInput, ExactOwnerAuthorityOutcome,
    TechnicalValidity,
};
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::params;

const TEST_CONFIRMATION_SEED: [u8; 32] = [91; 32];
const RUNTIME_AUTHORITY_ID: &str = "ea312-reference-runtime-authority";
const RUNTIME_ACTOR_ID: &str = "ea312-reference-runtime";
const RUNTIME_HOST_ID: &str = "gateway-global-control-host";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFixtureRecoveryReport {
    pub delegation_id: String,
    pub waiting_phase: DelegationPhase,
    pub recovery_phase: DelegationPhase,
    pub external_wait_id: String,
    pub recovery_continuation_id: String,
    pub recovery_job_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFixtureTerminalReport {
    pub delegation_id: String,
    pub kind: CompletionAssessmentKind,
    pub verifier_result_ids: Vec<String>,
    pub assessment_id: String,
    pub receipt_chain_count: i64,
    pub receipt_chain_head_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFixtureRoutineReport {
    pub routine_id: String,
    pub source_delegation_id: String,
    pub occurrence_id: String,
    pub occurrence_delegation_id: String,
    pub occurrence_continuation_id: String,
}

impl ExecAssStore {
    /// Records a real external wait and then a real recovery continuation on
    /// the same nonterminal delegation using the canonical lifecycle kernel.
    /// The continuation is immediately materialized through the shared jobs
    /// scheduler so the fixture cannot leave orphan runnable work.
    #[doc(hidden)]
    pub fn apply_test_reference_wait_and_recovery(
        &self,
        delegation_id: &str,
        action_id: &str,
        trusted_now: i64,
    ) -> Result<ReferenceFixtureRecoveryReport> {
        let current_detail = self
            .read_api_delegation_detail(delegation_id)?
            .context("reference delegation is missing")?;
        let next_action_revision = current_detail
            .actions
            .iter()
            .map(|action| action.action_revision)
            .max()
            .unwrap_or(0)
            + 1;
        let current = current_detail.delegation;
        if current.phase.is_terminal() || current.run_control != RunControlState::Running {
            bail!("reference wait requires a live running delegation");
        }
        let wait_id = format!("ea312-external-wait-{delegation_id}");
        let wait = ExternalWaitRecord {
            external_wait_id: wait_id.clone(),
            delegation_id: delegation_id.to_owned(),
            action_id: Some(action_id.to_owned()),
            kind: ExternalWaitKind::ExternalParty,
            status: ExternalWaitStatus::Waiting,
            reason: "waiting for the bounded external acknowledgement".into(),
            details_json: serde_json::json!({
                "dependency": "reference-provider-ack",
                "safe": true
            })
            .to_string(),
            delegation_revision: current.state_revision + 1,
            created_at: trusted_now,
            resolved_at: None,
        };
        let waiting = lifecycle_command(
            delegation_id,
            current.state_revision,
            current.run_control,
            trusted_now,
            "external-wait",
            vec![],
            vec![wait],
            None,
        );
        let LifecycleWriteOutcome::Applied(waiting) = self.apply_lifecycle_snapshot(&waiting)?
        else {
            bail!("reference external wait did not apply exactly once");
        };
        if waiting.delegation.phase != DelegationPhase::WaitingExternal {
            bail!("reference external wait selected the wrong phase");
        }

        let next_revision = waiting.delegation.state_revision + 1;
        let plan_revision = waiting
            .delegation
            .current_plan_revision
            .context("reference recovery requires a current plan")?;
        let recovery_action_id = format!("ea312-recovery-action-{delegation_id}");
        let recovery_continuation_id = format!("ea312-recovery-continuation-{delegation_id}");
        let recovery_action = ActionBranchRecord {
            action_id: recovery_action_id.clone(),
            delegation_id: delegation_id.to_owned(),
            action_revision: next_action_revision,
            target_delegation_revision: next_revision,
            target_plan_revision: plan_revision,
            stop_epoch: waiting.delegation.stop_epoch,
            branch_kind: ActionBranchKind::Recovery,
            status: ContinuationStatus::Runnable,
            action_summary: "recover the bounded external acknowledgement path".into(),
            created_at: trusted_now + 1,
            updated_at: trusted_now + 1,
            terminal_at: None,
        };
        let recovery_continuation = ContinuationRecord {
            continuation_id: recovery_continuation_id.clone(),
            delegation_id: delegation_id.to_owned(),
            target_delegation_revision: next_revision,
            target_plan_revision: plan_revision,
            action_id: recovery_action_id,
            branch_kind: ActionBranchKind::Recovery,
            causation_kind: ContinuationCausationKind::Recovery,
            causation_id: wait_id.clone(),
            status: ContinuationStatus::Runnable,
            job_id: None,
            lease_owner: None,
            lease_expires_at: None,
            fencing_token: 0,
            host_generation: 1,
            stop_epoch: waiting.delegation.stop_epoch,
            global_stop_epoch: 0,
            created_at: trusted_now + 1,
            updated_at: trusted_now + 1,
            completed_at: None,
        };
        let recovering = lifecycle_command(
            delegation_id,
            waiting.delegation.state_revision,
            RunControlState::Running,
            trusted_now + 1,
            "recovery",
            vec![recovery_action],
            vec![],
            Some(recovery_continuation),
        );
        let LifecycleWriteOutcome::Applied(recovering) =
            self.apply_lifecycle_snapshot(&recovering)?
        else {
            bail!("reference recovery did not apply exactly once");
        };
        if recovering.delegation.phase != DelegationPhase::Recovering {
            bail!("reference recovery selected the wrong phase");
        }
        self.materialize_runnable_continuation_jobs(trusted_now + 2, 100)?;
        let detail = self
            .read_api_delegation_detail(delegation_id)?
            .context("reference recovery delegation disappeared")?;
        let recovery = detail
            .continuations
            .iter()
            .find(|item| item.continuation_id == recovery_continuation_id)
            .context("reference recovery continuation disappeared")?;
        let recovery_job_id = recovery
            .job_id
            .clone()
            .context("reference recovery continuation was orphaned from the scheduler")?;

        Ok(ReferenceFixtureRecoveryReport {
            delegation_id: delegation_id.to_owned(),
            waiting_phase: waiting.delegation.phase,
            recovery_phase: recovering.delegation.phase,
            external_wait_id: wait_id,
            recovery_continuation_id,
            recovery_job_id,
        })
    }

    /// Creates an evidence-backed completed or partially-completed aggregate.
    /// The foundation is deterministic test setup; verifier and completion
    /// changes use the canonical receipt-bearing services.
    #[doc(hidden)]
    pub fn create_test_reference_terminal_case(
        &self,
        label: &str,
        kind: CompletionAssessmentKind,
        trusted_now: i64,
    ) -> Result<ReferenceFixtureTerminalReport> {
        if !matches!(
            kind,
            CompletionAssessmentKind::Completed | CompletionAssessmentKind::PartiallyCompleted
        ) {
            bail!("EA-312 terminal fixture supports completed and partial only");
        }
        self.ensure_reference_runtime_authority(trusted_now)?;
        let delegation_id = format!("ea312-{label}-delegation");
        let criterion_count = if kind == CompletionAssessmentKind::Completed {
            1
        } else {
            2
        };
        let command = reference_foundation(&delegation_id, label, criterion_count, trusted_now);
        let FoundationWriteOutcome::Created(_) = self.create_foundation(&command)? else {
            bail!("reference terminal foundation was not fresh");
        };
        self.verify_reference_criteria(&delegation_id, label, kind, trusted_now + 10)?;

        let detail = self
            .read_api_delegation_detail(&delegation_id)?
            .context("reference terminal delegation disappeared")?;
        let assessment_revision = 1;
        let idempotency_key = format!("ea312-{label}-assessment");
        let assessment_id = deterministic_completion_assessment_id(
            &delegation_id,
            assessment_revision,
            &idempotency_key,
        );
        let event_id = deterministic_completion_event_id(&assessment_id);
        let receipt = self.reference_receipt(
            &delegation_id,
            detail.delegation.state_revision + 1,
            ReceiptKind::Completion,
            ReceiptSubjectKind::CompletionAssessment,
            &assessment_id,
            assessment_revision,
            &event_id,
            &format!("ea312-{label}-assessment-cause"),
            trusted_now + 100,
        )?;
        let outcome = self.assess_completion_atomically(
            &self.open_receipt_integrity_store()?,
            &ReceiptRedactor::new(&["ea312-reference-secret"])?,
            &AssessCompletionCommand {
                write: WriteContext {
                    idempotency_key,
                    correlation_id: format!("ea312-{label}-assessment-correlation"),
                    causation_id: format!("ea312-{label}-assessment-cause"),
                    occurred_at: trusted_now + 100,
                },
                delegation_id: delegation_id.clone(),
                expected_state_revision: detail.delegation.state_revision,
                expected_criteria_revision: 1,
                expected_assessment_revision: assessment_revision,
                receipt,
            },
        )?;
        let CompletionAssessmentOutcome::Terminalized { assessment, .. } = outcome else {
            bail!("reference terminal case did not terminalize: {outcome:?}");
        };
        if assessment.kind != kind {
            bail!("reference terminal case selected the wrong result");
        }
        let stored = self
            .read_api_delegation_detail(&delegation_id)?
            .context("reference terminal result disappeared")?;
        let head = stored
            .receipt_chain_head
            .clone()
            .context("reference terminal result has no receipt head")?;
        Ok(ReferenceFixtureTerminalReport {
            delegation_id,
            kind,
            verifier_result_ids: stored
                .verifiers
                .iter()
                .map(|item| item.verifier_result_id.clone())
                .collect(),
            assessment_id,
            receipt_chain_count: stored.delegation.receipt_chain_count,
            receipt_chain_head_digest: head,
        })
    }

    /// Creates, materializes, admits, and replays one ordinary saved-routine
    /// occurrence through the canonical scheduler/routine admission service.
    #[doc(hidden)]
    pub fn create_test_reference_recurring_occurrence(
        &self,
        trusted_now: i64,
    ) -> Result<ReferenceFixtureRoutineReport> {
        let label = "routine-source";
        let source_delegation_id = "ea312-routine-source-delegation".to_string();
        let authority = reference_owner_authority(label, trusted_now)?;
        let mut dispatch = reference_dispatch(authority.clone(), "ea312-routine-source-action");
        let manifest = ready_manifest(&dispatch)?;
        let command =
            reference_admitted_foundation(&source_delegation_id, label, trusted_now, &authority);
        let authorizations = exact_authorizations(&authority, &manifest)?;
        let danger = ordinary_danger_admission(self, &manifest)?;
        let FoundationDispatchAdmissionOutcome::Admitted(admitted) = self
            .admit_foundation_dispatch(
                &command,
                &dispatch,
                &ServerResolutionRegistry::default(),
                &authority,
                &authorizations,
                &danger,
            )?
        else {
            bail!("reference routine source was not admitted");
        };
        if !matches!(*admitted, FoundationWriteOutcome::Created(_)) {
            bail!("reference routine source was not fresh");
        }
        let source = self
            .read_foundation(&source_delegation_id)?
            .context("reference routine source disappeared")?;
        let routine_id = "ea312-reference-routine".to_string();
        self.create_routine(&CreateRoutineCommand {
            routine: RoutineRecord {
                routine_id: routine_id.clone(),
                current_version: 1,
                enabled: true,
                timezone: "America/Chicago".into(),
                overlap_policy: RoutineOverlapPolicy::Earlier,
                catch_up_policy: RoutineCatchUpPolicy::LatestOnly,
                replay_cap: 1,
                created_at: trusted_now,
                updated_at: trusted_now,
            },
            version: RoutineVersionRecord {
                routine_id: routine_id.clone(),
                routine_version: 1,
                source_delegation_id: source_delegation_id.clone(),
                saved_owner_authority_provenance_id: source.authority.authority_provenance_id,
                normalized_original_intent: source.delegation.normalized_original_intent,
                resolved_leaf_manifest_json: source.plan.resolved_leaf_manifest_json,
                manifest_digest: source.plan.manifest_digest,
                saved_selector_json: r#"{"selector":"exact-reference-target"}"#.into(),
                saved_action_envelope_json: r#"{"tool":"connector.reference","version":"1.0.0"}"#
                    .into(),
                accepted_confirmation_grant_id: None,
                effective_policy_snapshot_json: r#"{"revision":1}"#.into(),
                effective_policy_revision: 1,
                stable_leaf_digest: saved_routine_stable_leaf_digest(&manifest.leaves()[0]),
                created_at: trusted_now,
            },
            schedule: RoutineScheduleSpec {
                local_hour: 2,
                local_minute: 30,
            },
        })?;

        let scheduler = crate::Storage::from_paths(&self.test_app_paths());
        let driver_now = trusted_now + 3 * 86_400_000;
        let driver_job = scheduler
            .acquire_due_jobs("ea312-routine-driver", driver_now, 30_000, 100)?
            .into_iter()
            .find(|job| {
                execass_routine_driver_id(&job.payload_json).as_deref() == Some(&routine_id)
            })
            .context("reference routine driver was not due")?;
        let occurrences = self.materialize_due_routine_occurrences(&RoutineDriverClaim {
            routine_id: routine_id.clone(),
            driver_job_id: driver_job.job_id,
            driver_lease_owner: driver_job
                .lease_owner
                .context("reference routine driver lease owner missing")?,
            driver_lease_expires_at: driver_job
                .lease_expires_at
                .context("reference routine driver lease expiry missing")?,
            trusted_now: driver_now,
        })?;
        let occurrence = occurrences
            .into_iter()
            .last()
            .context("reference routine produced no occurrence")?;
        let trigger_now = driver_now + 1;
        let trigger_job = scheduler
            .acquire_due_jobs("ea312-routine-trigger", trigger_now, 30_000, 100)?
            .into_iter()
            .find(|job| {
                execass_routine_trigger_occurrence_id(&job.payload_json).as_deref()
                    == Some(occurrence.occurrence_id.as_str())
            })
            .context("reference routine trigger was not due")?;
        let request = RoutineAdmissionRequest {
            occurrence_id: occurrence.occurrence_id.clone(),
            trigger_job_id: trigger_job.job_id,
            trigger_lease_owner: trigger_job
                .lease_owner
                .context("reference routine trigger lease owner missing")?,
            trigger_lease_expires_at: trigger_job
                .lease_expires_at
                .context("reference routine trigger lease expiry missing")?,
            trusted_now: trigger_now,
        };
        let plan = match self.plan_routine_occurrence_admission(&request)? {
            RoutineAdmissionOutcome::Planned(plan) => plan,
            other => bail!("reference routine admission was not planned: {other:?}"),
        };
        let target_snapshot = match &dispatch.nodes[0].action {
            DispatchAction::ResolvedLeaf(action) => action.target_snapshot.clone(),
            _ => bail!("reference routine dispatch leaf is unresolved"),
        };
        let persisted_action_id = manifest.leaves()[0].logical_action_id().to_string();
        let occurrence_action_id =
            deterministic_routine_occurrence_action_id(&occurrence.occurrence_id);
        if let DispatchAction::ResolvedLeaf(action) = &mut dispatch.nodes[0].action {
            action.logical_action_id = occurrence_action_id.clone();
        }
        let occurrence_manifest = rebind_persisted_manifest_for_routine_occurrence(
            plan.routine_version.resolved_leaf_manifest_json.as_bytes(),
            &plan.routine_version.manifest_digest,
            &[RoutineOccurrenceLeafBinding {
                persisted_logical_action_id: persisted_action_id,
                occurrence_logical_action_id: occurrence_action_id,
                target_snapshot,
            }],
        )
        .map_err(|error| anyhow::anyhow!(error))?;
        if occurrence_manifest != ready_manifest(&dispatch)? {
            bail!("reference routine occurrence manifest did not rebind exactly");
        }
        let occurrence_danger = ordinary_danger_admission(self, &occurrence_manifest)?;
        let admitted = self.admit_claimed_routine_occurrence(
            &request,
            &occurrence_manifest,
            &occurrence_danger,
        )?;
        let RoutineOccurrenceDispatchOutcome::Admitted {
            delegation_id,
            continuation_id,
            ..
        } = admitted
        else {
            bail!("reference routine occurrence was not admitted");
        };
        if !matches!(
            self.admit_claimed_routine_occurrence(
                &request,
                &occurrence_manifest,
                &occurrence_danger,
            )?,
            RoutineOccurrenceDispatchOutcome::Replayed { .. }
        ) {
            bail!("reference routine occurrence did not replay exactly");
        }
        self.materialize_runnable_continuation_jobs(trigger_now + 1, 100)?;
        let detail = self
            .read_api_delegation_detail(&delegation_id)?
            .context("reference occurrence delegation disappeared")?;
        let continuation = detail
            .continuations
            .iter()
            .find(|item| item.continuation_id == continuation_id)
            .context("reference occurrence continuation disappeared")?;
        if continuation.causation_kind != ContinuationCausationKind::RoutineOccurrence
            || continuation.causation_id != occurrence.occurrence_id
            || continuation.job_id.is_none()
        {
            bail!("reference occurrence lineage or scheduler binding is incomplete");
        }
        Ok(ReferenceFixtureRoutineReport {
            routine_id,
            source_delegation_id,
            occurrence_id: occurrence.occurrence_id,
            occurrence_delegation_id: delegation_id,
            occurrence_continuation_id: continuation_id,
        })
    }

    fn ensure_reference_runtime_authority(&self, trusted_now: i64) -> Result<()> {
        self.connection()?.execute(
            r#"INSERT OR IGNORE INTO execass_authority_provenance(
                 authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
                 channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
                 policy_revision,evidence_digest,created_at)
               VALUES(?1,'runtime',?2,'reference-fixture','local-runtime-fence',
                 'ea312-reference-runtime-bootstrap','runtime_safety_state','{}',1,?3,?4)"#,
            params![
                RUNTIME_AUTHORITY_ID,
                RUNTIME_ACTOR_ID,
                "e".repeat(64),
                trusted_now
            ],
        )?;
        Ok(())
    }

    fn verify_reference_criteria(
        &self,
        delegation_id: &str,
        label: &str,
        kind: CompletionAssessmentKind,
        trusted_now: i64,
    ) -> Result<()> {
        let criteria = self
            .read_api_delegation_detail(delegation_id)?
            .context("reference criteria delegation disappeared")?
            .criteria;
        let integrity = self.open_receipt_integrity_store()?;
        let redactor = ReceiptRedactor::new(&["ea312-reference-secret"])?;
        for (index, criterion) in criteria.iter().enumerate() {
            let current = self
                .read_api_delegation_detail(delegation_id)?
                .context("reference verifier delegation disappeared")?
                .delegation;
            let result_revision = 1;
            let idempotency_key = format!("ea312-{label}-verify-{}", index + 1);
            let verifier_result_id = deterministic_verifier_result_id(
                &criterion.criterion_id,
                result_revision,
                &idempotency_key,
            );
            let event_id = format!("ea312-{label}-verify-event-{}", index + 1);
            let causation_id = format!("ea312-{label}-verify-cause-{}", index + 1);
            let receipt = self.reference_receipt(
                delegation_id,
                current.state_revision + 1,
                ReceiptKind::Verifier,
                ReceiptSubjectKind::VerifierResult,
                &verifier_result_id,
                result_revision,
                &event_id,
                &causation_id,
                trusted_now + index as i64,
            )?;
            let outcome = self.verify_criterion(
                &integrity,
                &redactor,
                &VerifyCriterionCommand {
                    write: WriteContext {
                        idempotency_key,
                        correlation_id: format!("ea312-{label}-verify-correlation-{}", index + 1),
                        causation_id,
                        occurred_at: trusted_now + index as i64,
                    },
                    delegation_id: delegation_id.to_owned(),
                    criterion_id: criterion.criterion_id.clone(),
                    expected_criteria_revision: 1,
                    expected_state_revision: current.state_revision,
                    expected_result_revision: result_revision,
                    verifier_result_id,
                    outbox_event_id: event_id,
                    receipt,
                },
            )?;
            let CriterionVerificationOutcome::Recorded { result, .. } = outcome else {
                bail!("reference verifier result did not record: {outcome:?}");
            };
            let expected = if kind == CompletionAssessmentKind::PartiallyCompleted && index == 1 {
                "fail"
            } else {
                "pass"
            };
            if result.result.as_str() != expected {
                bail!("reference verifier produced {result:?}, expected {expected}");
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn reference_receipt(
        &self,
        delegation_id: &str,
        expected_state_revision: i64,
        receipt_kind: ReceiptKind,
        subject_kind: ReceiptSubjectKind,
        subject_id: &str,
        subject_revision: i64,
        event_id: &str,
        causation_id: &str,
        trusted_now: i64,
    ) -> Result<AppendReceiptCommand> {
        let integrity = self.open_receipt_integrity_store()?;
        let IntegrityStatus::Trusted {
            receipt_count,
            receipt_head_digest,
            key,
            ..
        } = integrity.status()?
        else {
            bail!("reference fixture receipt integrity is not trusted");
        };
        let detail = self
            .read_api_delegation_detail(delegation_id)?
            .context("reference receipt delegation disappeared")?;
        Ok(AppendReceiptCommand {
            receipt_id: format!("{event_id}-receipt"),
            transaction_id: format!("{event_id}-transaction"),
            state_root_generation: 1,
            delegation_id: delegation_id.to_owned(),
            expected_state_revision,
            expected_global_count: receipt_count,
            expected_global_head_digest: receipt_head_digest,
            expected_delegation_count: detail.delegation.receipt_chain_count,
            expected_delegation_head_digest: detail.delegation.receipt_chain_head_digest,
            receipt_kind,
            subject: ReceiptSubject {
                kind: subject_kind,
                subject_id: subject_id.to_owned(),
                revision: subject_revision,
            },
            causation_id: causation_id.to_owned(),
            causation_event_id: event_id.to_owned(),
            actor: ReceiptActorBinding {
                actor_type: ActorType::Runtime,
                actor_identity: SafeText::new(RUNTIME_ACTOR_ID, &[])?,
                authority_provenance_id: RUNTIME_AUTHORITY_ID.into(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: RUNTIME_HOST_ID.into(),
                fencing_token: 1,
            },
            key,
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::new("ExecAss reference evidence recorded", &[])?,
            occurred_at: trusted_now,
            committed_at: trusted_now,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn lifecycle_command(
    delegation_id: &str,
    expected_revision: i64,
    run_control: RunControlState,
    trusted_now: i64,
    suffix: &str,
    action_branches: Vec<ActionBranchRecord>,
    external_waits: Vec<ExternalWaitRecord>,
    continuation: Option<ContinuationRecord>,
) -> LifecycleSnapshotCommand {
    let idempotency_key = format!("ea312-{suffix}-write-{delegation_id}");
    let correlation_id = format!("ea312-{suffix}-correlation-{delegation_id}");
    let causation_id = format!("ea312-{suffix}-cause-{delegation_id}");
    LifecycleSnapshotCommand {
        write: WriteContext {
            idempotency_key: idempotency_key.clone(),
            correlation_id: correlation_id.clone(),
            causation_id: causation_id.clone(),
            occurred_at: trusted_now,
        },
        transition_id: format!("ea312-{suffix}-transition-{delegation_id}"),
        delegation_id: delegation_id.to_owned(),
        expected_state_revision: expected_revision,
        pre_actionable_phase: None,
        selected_run_control: run_control,
        resume_proof: None,
        action_branches,
        attention_items: vec![],
        external_waits,
        assessment: None,
        continuation,
        reason: format!("EA-312 reference {suffix}"),
        outbox_event: NewOutboxEvent {
            event_id: format!("ea312-{suffix}-event-{delegation_id}"),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: delegation_id.to_owned(),
            aggregate_revision: expected_revision + 1,
            correlation_id,
            causation_id,
            occurred_at: trusted_now,
            safe_payload_json: serde_json::json!({"scenario": suffix}).to_string(),
            duplicate_identity: idempotency_key,
        },
    }
}

fn reference_foundation(
    delegation_id: &str,
    label: &str,
    criterion_count: usize,
    trusted_now: i64,
) -> CreateFoundationCommand {
    let authority_id = format!("ea312-{label}-owner-authority");
    let mut criteria = Vec::with_capacity(criterion_count);
    for index in 0..criterion_count {
        let threshold = if index == 0 { 0 } else { 1 };
        criteria.push(OutcomeCriterionRecord {
            criterion_id: format!("ea312-{label}-criterion-{}", index + 1),
            delegation_id: delegation_id.to_owned(),
            criteria_revision: 1,
            criterion_key: format!("criterion-{}", index + 1),
            description: if threshold == 0 {
                "the canonical plan exists".into()
            } else {
                "a later canonical plan exists".into()
            },
            material: true,
            verifier_type: VerifierType::DatabasePredicate,
            expected_predicate_json: serde_json::to_string(
                &CriterionPredicate::DatabasePredicate {
                    version: PredicateVersion::V1,
                    delegation_id: delegation_id.to_owned(),
                    canonical_plan_revision_greater_than: threshold,
                },
            )
            .expect("reference predicate serializes"),
            authoritative_source_kind: "execass_plan_store".into(),
            created_at: trusted_now,
        });
    }
    CreateFoundationCommand {
        write: WriteContext {
            idempotency_key: format!("ea312-{label}-foundation"),
            correlation_id: format!("ea312-{label}-correlation"),
            causation_id: format!("ea312-{label}-cause"),
            occurred_at: trusted_now,
        },
        authority: AuthorityProvenanceRecord {
            authority_provenance_id: authority_id.clone(),
            actor_type: ActorType::HumanLocal,
            credential_identity: "ea312-local-owner".into(),
            authenticated_ingress: "native-control".into(),
            channel_assurance: "interactive-local".into(),
            source_correlation_id: format!("ea312-{label}-correlation"),
            source_message_id: Some(format!("ea312-{label}-message")),
            authority_kind: AuthorityKind::OriginalRequest,
            normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.into(),
            policy_revision: 1,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_digest: None,
            bound_challenge_nonce_digest: None,
            evidence_digest: "a".repeat(64),
            created_at: trusted_now,
            expires_at: None,
        },
        delegation: DelegationRecord {
            delegation_id: delegation_id.to_owned(),
            normalized_original_intent: format!("produce the bounded {label} result"),
            intake_evidence_json: serde_json::json!({"fixture": label}).to_string(),
            ingress_source: "native-control".into(),
            ingress_credential_identity: "ea312-local-owner".into(),
            source_message_id: Some(format!("ea312-{label}-message")),
            source_correlation_id: format!("ea312-{label}-correlation"),
            ingress_idempotency_key: format!("ea312-{label}-foundation"),
            classifier_version: "ea312-reference-v1".into(),
            classifier_reasons_json: r#"["durable_work"]"#.into(),
            phase: DelegationPhase::InMotion,
            run_control: RunControlState::Running,
            state_revision: 1,
            current_plan_revision: Some(1),
            current_criteria_revision: Some(1),
            policy_revision: 1,
            effective_authority_json: r#"{"profile":"balanced"}"#.into(),
            authority_provenance_id: authority_id.clone(),
            pending_decision_id: None,
            external_wait_json: None,
            stop_epoch: 0,
            completion_assessment_json: None,
            receipt_chain_count: 0,
            receipt_chain_head_digest: None,
            created_at: trusted_now,
            updated_at: trusted_now,
            acknowledged_at: None,
            terminal_at: None,
        },
        plan: PlanRecord {
            plan_id: format!("ea312-{label}-plan"),
            delegation_id: delegation_id.to_owned(),
            plan_revision: 1,
            based_on_delegation_revision: 1,
            policy_revision: 1,
            plan_summary: format!("produce the bounded {label} result"),
            resolved_leaf_manifest_json: r#"[{"action":"reference"}]"#.into(),
            manifest_digest: format!("ea312-{label}-manifest"),
            created_by_authority_provenance_id: authority_id,
            created_at: trusted_now,
        },
        outcome_criteria: criteria,
        initial_continuation: None,
        outbox_event: NewOutboxEvent {
            event_id: format!("ea312-{label}-foundation-event"),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: delegation_id.to_owned(),
            aggregate_revision: 1,
            correlation_id: format!("ea312-{label}-correlation"),
            causation_id: format!("ea312-{label}-cause"),
            occurred_at: trusted_now,
            safe_payload_json: serde_json::json!({"scenario": label}).to_string(),
            duplicate_identity: format!("ea312-{label}-foundation"),
        },
    }
}

fn reference_owner_authority(label: &str, trusted_now: i64) -> Result<VerifiedOwnerAuthority> {
    issue_test_local_owner_authority(TestLocalOwnerAuthorityInput {
        authenticated_client_id: "ea312-local-owner".into(),
        authenticated_ingress: "native-control".into(),
        channel_assurance: "interactive-local".into(),
        request_correlation_id: format!("ea312-{label}-correlation"),
        source_message_id: Some(format!("ea312-{label}-message")),
        normalized_intent: format!("produce the bounded {label} result"),
        instruction_revision: "instruction-1".into(),
        instruction_bytes: format!("produce:{label}").into_bytes(),
        owner_envelope_revision: "envelope-1".into(),
        owner_envelope_json: serde_json::json!({"request": label}).to_string(),
        authority_kind: "original_request".into(),
        normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.into(),
        policy_revision: 1,
        bound_decision_id: None,
        bound_decision_revision: None,
        bound_manifest_bytes: None,
        challenge_nonce_bytes: None,
        created_at: trusted_now,
        expires_at: None,
    })
    .map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn reference_dispatch(authority: VerifiedOwnerAuthority, action_id: &str) -> DispatchTree {
    DispatchTree {
        root_id: "root".into(),
        nodes: vec![DispatchNode {
            node_id: "root".into(),
            action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                logical_action_id: action_id.into(),
                action_kind: "tool_call".into(),
                tool: ToolIdentityInput {
                    tool_id: "connector.reference".into(),
                    version: "1.0.0".into(),
                },
                operands: CanonicalValue::Object(vec![]),
                target_snapshot: TargetSnapshotInput {
                    targets: vec![CanonicalValue::String("reference-target".into())],
                },
                material_digest: None,
                owner_authority: authority,
            })),
        }],
    }
}

fn ready_manifest(dispatch: &DispatchTree) -> Result<CanonicalLeafManifest> {
    match compile_dispatch(dispatch, &ServerResolutionRegistry::default()) {
        ManifestCompilation::Ready(manifest) => Ok(manifest),
        ManifestCompilation::MechanicalResolutionRequired(_) => {
            bail!("reference dispatch unexpectedly requires mechanical resolution")
        }
    }
}

fn exact_authorizations(
    authority: &VerifiedOwnerAuthority,
    manifest: &CanonicalLeafManifest,
) -> Result<Vec<ExactOwnerActionAuthority>> {
    manifest
        .leaves()
        .iter()
        .map(|leaf| {
            let technical =
                issue_test_objective_technical_validity_proof(leaf, TechnicalValidity::Valid);
            match authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: authority,
                canonical_leaf: leaf,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &technical,
            }) {
                ExactOwnerAuthorityOutcome::Authorized(value) => Ok(value),
                other => bail!("reference owner action was not authorized: {other:?}"),
            }
        })
        .collect()
}

fn ordinary_danger_admission(
    store: &ExecAssStore,
    manifest: &CanonicalLeafManifest,
) -> Result<SignedDangerAdmissionProof> {
    let routes = manifest
        .leaves()
        .iter()
        .map(|leaf| {
            let metadata = issue_test_verified_danger_metadata(leaf, &[]);
            match_known_danger(KnownDangerMatchInput {
                canonical_leaf: leaf,
                verified_metadata: &metadata,
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let proof =
        bind_danger_admission(manifest, routes).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let identity = activate_test_confirmation_authority(store, TEST_CONFIRMATION_SEED)?;
    let bytes = danger_admission_signing_bytes(
        &proof,
        identity.key_id(),
        identity.key_generation(),
        identity.canonical_root_identity(),
        identity.installation_identity(),
        identity.os_user_identity_digest(),
        identity.state_root_generation(),
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let signature = SigningKey::from_bytes(&TEST_CONFIRMATION_SEED).sign(&bytes);
    Ok(SignedDangerAdmissionProof::from_untrusted_parts(
        proof,
        identity.key_id().to_string(),
        identity.key_generation(),
        identity.canonical_root_identity().to_string(),
        identity.installation_identity().to_string(),
        identity.os_user_identity_digest().to_string(),
        identity.state_root_generation(),
        signature
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}

fn reference_admitted_foundation(
    delegation_id: &str,
    label: &str,
    trusted_now: i64,
    authority: &VerifiedOwnerAuthority,
) -> CreateFoundationCommand {
    let mut command = reference_foundation(delegation_id, label, 1, trusted_now);
    command.authority.authority_provenance_id = authority.authority_provenance_id().to_string();
    command.delegation.authority_provenance_id = authority.authority_provenance_id().to_string();
    command.plan.created_by_authority_provenance_id =
        authority.authority_provenance_id().to_string();
    command
}
