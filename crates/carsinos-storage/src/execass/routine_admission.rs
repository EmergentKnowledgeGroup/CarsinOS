//! Atomic claimed-trigger admission for one durable saved-routine occurrence.

use super::confirmation::require_pinned_saved_routine_grant_in_tx;
use super::foundation::create_foundation_in_tx;
use super::rows::{get_authority, get_delegation};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{require_text, validate_routine_foundation_command};
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::owner_normalized_intent_digest;
use carsinos_core::execass_danger::{
    saved_routine_stable_leaf_digest, DangerAdmissionState, SignedDangerAdmissionProof,
};
use carsinos_core::execass_manifest::CanonicalLeafManifest;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutineOccurrenceDispatchOutcome {
    Admitted {
        occurrence_id: String,
        delegation_id: String,
        continuation_id: String,
    },
    Replayed {
        occurrence_id: String,
        delegation_id: String,
    },
    Refused {
        reason: String,
    },
}

pub fn deterministic_routine_occurrence_action_id(occurrence_id: &str) -> String {
    deterministic_id("routine_action", occurrence_id, None)
}

impl ExecAssStore {
    /// Verify one claimed reserved trigger and atomically create its distinct
    /// delegation/continuation, bind the occurrence, and settle the trigger.
    /// No executable continuation is visible before this transaction commits.
    pub fn admit_claimed_routine_occurrence(
        &self,
        request: &RoutineAdmissionRequest,
        manifest: &CanonicalLeafManifest,
        danger_admission: &SignedDangerAdmissionProof,
    ) -> Result<RoutineOccurrenceDispatchOutcome> {
        require_text("occurrence_id", &request.occurrence_id)?;
        require_text("trigger_job_id", &request.trigger_job_id)?;
        require_text("trigger_lease_owner", &request.trigger_lease_owner)?;
        if request.trusted_now <= 0 || request.trigger_lease_expires_at <= request.trusted_now {
            bail!("routine occurrence admission requires a live positive trigger lease");
        }
        if manifest.leaves().len() != 1 {
            bail!("a saved routine occurrence must resolve to exactly one canonical leaf");
        }

        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let occurrence = load_occurrence(&tx, &request.occurrence_id)?
            .context("routine occurrence does not exist")?;
        if occurrence.status == RoutineOccurrenceStatus::Settled {
            let delegation_id = occurrence
                .admitted_delegation_id
                .context("settled routine occurrence has no admitted delegation")?;
            tx.commit().context("closing routine admission replay")?;
            return Ok(RoutineOccurrenceDispatchOutcome::Replayed {
                occurrence_id: request.occurrence_id.clone(),
                delegation_id,
            });
        }
        if !matches!(
            occurrence.status,
            RoutineOccurrenceStatus::Planned | RoutineOccurrenceStatus::AdmissionPlanned
        ) {
            tx.commit()?;
            return Ok(RoutineOccurrenceDispatchOutcome::Refused {
                reason: "occurrence_not_admissible".into(),
            });
        }

        let version = load_version(&tx, &occurrence.routine_id, occurrence.routine_version)?
            .context("routine occurrence version does not exist")?;
        let (enabled, current_version): (i64, i64) = tx.query_row(
            "SELECT enabled,current_version FROM execass_routines WHERE routine_id=?1",
            [&occurrence.routine_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if enabled == 0 {
            tx.commit()?;
            return Ok(RoutineOccurrenceDispatchOutcome::Refused {
                reason: "routine_paused".into(),
            });
        }
        if current_version != occurrence.routine_version {
            tx.commit()?;
            return Ok(RoutineOccurrenceDispatchOutcome::Refused {
                reason: "routine_version_superseded".into(),
            });
        }

        let (global_stop_engaged, global_stop_epoch, current_policy_revision): (i64, i64, i64) =
            tx.query_row(
                "SELECT engaged,global_stop_epoch,current_policy_revision FROM execass_global_runtime_control WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        if global_stop_engaged != 0 {
            tx.commit()?;
            return Ok(RoutineOccurrenceDispatchOutcome::Refused {
                reason: "global_stop_engaged".into(),
            });
        }
        if current_policy_revision != version.effective_policy_revision
            || occurrence.effective_policy_revision != current_policy_revision
        {
            tx.commit()?;
            return Ok(RoutineOccurrenceDispatchOutcome::Refused {
                reason: "current_policy_changed".into(),
            });
        }

        let expected_payload = super::routines::routine_trigger_payload(&occurrence)?;
        let trigger_matches = tx
            .query_row(
                "SELECT 1 FROM execass_routine_job_bindings b JOIN jobs j ON j.job_id=b.job_id WHERE b.occurrence_id=?1 AND b.job_id=?2 AND j.payload_json=?3 AND j.schedule_kind='execass_routine_trigger' AND j.enabled=1 AND j.lease_owner=?4 AND j.lease_expires_at=?5 AND j.lease_expires_at>?6 AND j.deleted_at IS NULL",
                params![occurrence.occurrence_id, request.trigger_job_id, expected_payload, request.trigger_lease_owner, request.trigger_lease_expires_at, request.trusted_now],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !trigger_matches {
            tx.commit()?;
            return Ok(RoutineOccurrenceDispatchOutcome::Refused {
                reason: "trigger_lease_or_binding_invalid".into(),
            });
        }

        let leaf = &manifest.leaves()[0];
        if leaf.logical_action_id()
            != deterministic_routine_occurrence_action_id(&occurrence.occurrence_id)
            || leaf.owner_authority().authority_provenance_id()
                != version.saved_owner_authority_provenance_id
            || owner_normalized_intent_digest(&version.normalized_original_intent).as_deref()
                != Some(leaf.owner_authority().normalized_intent_digest().as_hex())
            || saved_routine_stable_leaf_digest(leaf) != version.stable_leaf_digest
        {
            bail!("routine occurrence action or owner authority materially drifted");
        }

        match self.verify_danger_admission_in_tx(&tx, danger_admission, manifest)? {
            DangerAdmissionState::Ordinary => {}
            DangerAdmissionState::RequiresOneConfirmation => {
                let assessment = danger_admission
                    .proof()
                    .routes()
                    .first()
                    .and_then(|route| route.confirmation_for_leaf(leaf))
                    .context("dangerous routine leaf has no exact consequence disclosure")?;
                require_pinned_saved_routine_grant_in_tx(
                    &tx,
                    &version,
                    manifest,
                    leaf,
                    &assessment.declared_consequence,
                )?;
            }
        }

        let command = build_foundation_command(
            &tx,
            &occurrence,
            &version,
            manifest,
            global_stop_epoch,
            request.trusted_now,
        )?;
        validate_routine_foundation_command(&command, &occurrence.occurrence_id)?;
        let continuation_id = command
            .initial_continuation
            .as_ref()
            .context("routine occurrence foundation has no continuation")?
            .continuation_id
            .clone();
        let delegation_id = command.delegation.delegation_id.clone();
        match create_foundation_in_tx(&tx, &command)? {
            FoundationWriteOutcome::Created(_) => {}
            FoundationWriteOutcome::Replayed(_) | FoundationWriteOutcome::Conflict { .. } => {
                bail!("unsettled routine occurrence collided with an existing foundation")
            }
        }

        let admission_json = serde_json::to_string(&json!({
            "delegation_id": delegation_id,
            "kind": "execass.routine_occurrence_admission.v1",
            "manifest_digest": manifest.canonical().digest().as_hex(),
            "occurrence_id": occurrence.occurrence_id,
            "routine_id": occurrence.routine_id,
            "routine_version": occurrence.routine_version,
        }))?;
        let operation_id = deterministic_id(
            "routine_trigger_settlement",
            &occurrence.occurrence_id,
            None,
        );
        tx.execute(
            "INSERT INTO execass_routine_trigger_operations(operation_id,occurrence_id,job_id,operation,lease_owner,lease_expires_at,occurred_at) VALUES(?1,?2,?3,'settle_trigger',?4,?5,?6)",
            params![operation_id, occurrence.occurrence_id, request.trigger_job_id, request.trigger_lease_owner, request.trigger_lease_expires_at, request.trusted_now],
        )?;
        let occurrence_changed = tx.execute(
            "UPDATE execass_routine_occurrences SET status='settled',admission_plan_json=?1,admitted_delegation_id=?2,updated_at=MAX(updated_at,?3) WHERE occurrence_id=?4 AND status IN ('planned','admission_planned') AND admitted_delegation_id IS NULL",
            params![admission_json, delegation_id, request.trusted_now, occurrence.occurrence_id],
        )?;
        let trigger_changed = tx.execute(
            "UPDATE jobs SET enabled=0,next_run_at=NULL,lease_owner=NULL,lease_expires_at=NULL,updated_at=MAX(updated_at,?1) WHERE job_id=?2 AND lease_owner=?3 AND lease_expires_at=?4",
            params![request.trusted_now, request.trigger_job_id, request.trigger_lease_owner, request.trigger_lease_expires_at],
        )?;
        if occurrence_changed != 1 || trigger_changed != 1 {
            bail!("routine occurrence or trigger changed before atomic settlement");
        }
        tx.commit()
            .context("committing atomic routine occurrence admission")?;
        Ok(RoutineOccurrenceDispatchOutcome::Admitted {
            occurrence_id: request.occurrence_id.clone(),
            delegation_id,
            continuation_id,
        })
    }
}

fn build_foundation_command(
    tx: &Transaction<'_>,
    occurrence: &RoutineOccurrenceRecord,
    version: &RoutineVersionRecord,
    manifest: &CanonicalLeafManifest,
    global_stop_epoch: i64,
    trusted_now: i64,
) -> Result<CreateFoundationCommand> {
    let authority = get_authority(tx, &version.saved_owner_authority_provenance_id)?
        .context("saved routine owner authority does not exist")?;
    let source = get_delegation(tx, &version.source_delegation_id)?
        .context("saved routine source delegation does not exist")?;
    if source.authority_provenance_id != authority.authority_provenance_id
        || source.normalized_original_intent != version.normalized_original_intent
        || source.policy_revision != version.effective_policy_revision
        || authority.policy_revision != version.effective_policy_revision
    {
        bail!("saved routine source authority or policy binding changed");
    }
    let (plan_summary, source_plan_revision): (String, i64) = tx.query_row(
        "SELECT plan_summary,plan_revision FROM execass_plans WHERE delegation_id=?1 AND plan_revision=?2",
        params![source.delegation_id, source.current_plan_revision],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let source_criteria_revision = source
        .current_criteria_revision
        .context("saved routine source has no current criteria")?;
    let mut statement = tx.prepare(
        "SELECT criterion_key,description,material,verifier_type,expected_predicate_json,authoritative_source_kind FROM execass_outcome_criteria WHERE delegation_id=?1 AND criteria_revision=?2 ORDER BY criterion_key,criterion_id",
    )?;
    let criteria = statement
        .query_map(
            params![source.delegation_id, source_criteria_revision],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, VerifierType>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if criteria.is_empty() {
        bail!("saved routine source has no outcome criteria");
    }

    let delegation_id = deterministic_id("routine_delegation", &occurrence.occurrence_id, None);
    let continuation_id = deterministic_id("routine_continuation", &occurrence.occurrence_id, None);
    let idempotency_key = deterministic_id("routine_idempotency", &occurrence.occurrence_id, None);
    let manifest_digest = manifest.canonical().digest().as_hex().to_string();
    let intake_evidence_json = serde_json::to_string(&json!({
        "occurrence_id": occurrence.occurrence_id,
        "routine_id": occurrence.routine_id,
        "routine_version": occurrence.routine_version,
        "scheduled_instant_ms": occurrence.scheduled_instant_ms,
        "source_delegation_id": version.source_delegation_id,
    }))?;
    let outbox_payload = serde_json::to_string(&json!({
        "delegation_id": delegation_id,
        "occurrence_id": occurrence.occurrence_id,
        "routine_id": occurrence.routine_id,
        "summary": "saved routine occurrence admitted",
    }))?;
    let outcome_criteria = criteria
        .into_iter()
        .enumerate()
        .map(
            |(index, (key, description, material, verifier_type, expected, source_kind))| {
                OutcomeCriterionRecord {
                    criterion_id: deterministic_id(
                        "routine_criterion",
                        &occurrence.occurrence_id,
                        Some(&format!("{index}:{key}")),
                    ),
                    delegation_id: delegation_id.clone(),
                    criteria_revision: 1,
                    criterion_key: key,
                    description,
                    material,
                    verifier_type,
                    expected_predicate_json: expected,
                    authoritative_source_kind: source_kind,
                    created_at: trusted_now,
                }
            },
        )
        .collect();
    Ok(CreateFoundationCommand {
        write: WriteContext {
            idempotency_key: idempotency_key.clone(),
            correlation_id: authority.source_correlation_id.clone(),
            causation_id: occurrence.occurrence_id.clone(),
            occurred_at: trusted_now,
        },
        authority: authority.clone(),
        delegation: DelegationRecord {
            delegation_id: delegation_id.clone(),
            normalized_original_intent: version.normalized_original_intent.clone(),
            intake_evidence_json,
            ingress_source: authority.authenticated_ingress.clone(),
            ingress_credential_identity: authority.credential_identity.clone(),
            source_message_id: authority.source_message_id.clone(),
            source_correlation_id: authority.source_correlation_id.clone(),
            ingress_idempotency_key: idempotency_key.clone(),
            classifier_version: "execass.saved_routine.v1".into(),
            classifier_reasons_json: r#"["durable_saved_routine_occurrence"]"#.into(),
            phase: DelegationPhase::InMotion,
            run_control: RunControlState::Running,
            state_revision: 1,
            current_plan_revision: Some(1),
            current_criteria_revision: Some(1),
            policy_revision: version.effective_policy_revision,
            effective_authority_json: source.effective_authority_json,
            authority_provenance_id: authority.authority_provenance_id.clone(),
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
            plan_id: deterministic_id("routine_plan", &occurrence.occurrence_id, None),
            delegation_id: delegation_id.clone(),
            plan_revision: 1,
            based_on_delegation_revision: 1,
            policy_revision: version.effective_policy_revision,
            plan_summary: format!(
                "saved routine occurrence from plan {source_plan_revision}: {plan_summary}"
            ),
            resolved_leaf_manifest_json: String::from_utf8(manifest.canonical().bytes().to_vec())
                .expect("canonical manifest is UTF-8 JSON"),
            manifest_digest,
            created_by_authority_provenance_id: authority.authority_provenance_id.clone(),
            created_at: trusted_now,
        },
        outcome_criteria,
        initial_continuation: Some(ContinuationRecord {
            continuation_id,
            delegation_id: delegation_id.clone(),
            target_delegation_revision: 1,
            target_plan_revision: 1,
            action_id: manifest.leaves()[0].logical_action_id().to_string(),
            branch_kind: ActionBranchKind::Ordinary,
            causation_kind: ContinuationCausationKind::RoutineOccurrence,
            causation_id: occurrence.occurrence_id.clone(),
            status: ContinuationStatus::Runnable,
            job_id: None,
            lease_owner: None,
            lease_expires_at: None,
            fencing_token: 0,
            host_generation: 1,
            stop_epoch: 0,
            global_stop_epoch,
            created_at: trusted_now,
            updated_at: trusted_now,
            completed_at: None,
        }),
        outbox_event: NewOutboxEvent {
            event_id: deterministic_id("routine_outbox", &occurrence.occurrence_id, None),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: delegation_id,
            aggregate_revision: 1,
            correlation_id: authority.source_correlation_id,
            causation_id: occurrence.occurrence_id.clone(),
            occurred_at: trusted_now,
            safe_payload_json: outbox_payload,
            duplicate_identity: idempotency_key,
        },
    })
}

fn deterministic_id(domain: &str, occurrence_id: &str, suffix: Option<&str>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.routine.admission.v1\0");
    digest.update(domain.as_bytes());
    digest.update(b"\0");
    digest.update(occurrence_id.as_bytes());
    if let Some(suffix) = suffix {
        digest.update(b"\0");
        digest.update(suffix.as_bytes());
    }
    format!("{domain}-{:x}", digest.finalize())
}

fn load_occurrence(
    tx: &Transaction<'_>,
    occurrence_id: &str,
) -> Result<Option<RoutineOccurrenceRecord>> {
    tx.query_row("SELECT occurrence_id,routine_id,routine_version,scheduled_instant_ms,scheduled_local,utc_offset_seconds,time_resolution,effective_policy_revision,status,admission_plan_json,admitted_delegation_id,created_at,updated_at FROM execass_routine_occurrences WHERE occurrence_id=?1", [occurrence_id], |row| Ok(RoutineOccurrenceRecord { occurrence_id: row.get(0)?, routine_id: row.get(1)?, routine_version: row.get(2)?, scheduled_instant_ms: row.get(3)?, scheduled_local: row.get(4)?, utc_offset_seconds: row.get(5)?, time_resolution: row.get(6)?, effective_policy_revision: row.get(7)?, status: row.get(8)?, admission_plan_json: row.get(9)?, admitted_delegation_id: row.get(10)?, created_at: row.get(11)?, updated_at: row.get(12)? })).optional().map_err(Into::into)
}

fn load_version(
    tx: &Transaction<'_>,
    routine_id: &str,
    routine_version: i64,
) -> Result<Option<RoutineVersionRecord>> {
    tx.query_row("SELECT routine_id,routine_version,source_delegation_id,saved_owner_authority_provenance_id,normalized_original_intent,resolved_leaf_manifest_json,manifest_digest,saved_selector_json,saved_action_envelope_json,accepted_confirmation_grant_id,effective_policy_snapshot_json,effective_policy_revision,stable_leaf_digest,created_at FROM execass_routine_versions WHERE routine_id=?1 AND routine_version=?2", params![routine_id,routine_version], |row| Ok(RoutineVersionRecord { routine_id: row.get(0)?, routine_version: row.get(1)?, source_delegation_id: row.get(2)?, saved_owner_authority_provenance_id: row.get(3)?, normalized_original_intent: row.get(4)?, resolved_leaf_manifest_json: row.get(5)?, manifest_digest: row.get(6)?, saved_selector_json: row.get(7)?, saved_action_envelope_json: row.get(8)?, accepted_confirmation_grant_id: row.get(9)?, effective_policy_snapshot_json: row.get(10)?, effective_policy_revision: row.get(11)?, stable_leaf_digest: row.get(12)?, created_at: row.get(13)? })).optional().map_err(Into::into)
}
