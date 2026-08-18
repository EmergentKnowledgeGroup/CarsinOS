#![cfg_attr(not(test), allow(dead_code))]

//! Storage-local deterministic lifecycle projection for the ExecAss aggregate.
//!
//! This module does not claim actions or execute effects. It only records the
//! immutable inputs to lifecycle selection, serializes one revision/transition/
//! outbox append, and fences stop/resume state before later execution lanes.

use super::receipt::{AtomicReceiptMutation, AtomicReceiptWriteOutcome};
use super::receipt_integrity::{IntegrityStatus, ReceiptIntegrityStore};
use super::rows::{
    get_delegation, get_outbox, insert_action_branch, insert_authority, insert_continuation,
    insert_criterion, insert_outbox, insert_plan,
};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{
    require_text, validate_continuation, validate_criterion, validate_outbox, validate_plan,
};
use crate::AppPaths;
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{owner_normalized_intent_digest, VerifiedOwnerAuthority};
use carsinos_core::execass_danger::SignedDangerAdmissionProof;
use carsinos_core::execass_manifest::{canonicalize_owner_authority, CanonicalLeafManifest};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

type StoredActionBranchIdentity = (i64, i64, i64, i64, String, String, i64, String);
type StoredAttentionIdentity = (
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    i64,
);
type StoredExternalWaitIdentity = (String, Option<String>, String, String, String, i64, i64);

pub fn select_lifecycle_phase(input: LifecycleSelectorInput) -> Result<DelegationPhase> {
    if let Some(assessment) = input.completion_assessment {
        return Ok(assessment.phase());
    }
    if let Some(pre_actionable) = input.pre_actionable_phase {
        return Ok(pre_actionable.phase());
    }
    if input.ordinary_runnable_or_executing {
        return Ok(DelegationPhase::InMotion);
    }
    if input.recovery_runnable_or_executing {
        return Ok(DelegationPhase::Recovering);
    }
    if input.actionable_attention {
        return Ok(DelegationPhase::WaitingForUser);
    }
    if input.external_wait {
        return Ok(DelegationPhase::WaitingExternal);
    }
    Err(LifecycleSelectionError::NoHonestPath.into())
}

pub(super) fn record_genesis_lifecycle_history(
    conn: &Connection,
    command: &CreateFoundationCommand,
) -> Result<()> {
    let snapshot = projection_snapshot_json(
        conn,
        &command.delegation.delegation_id,
        command
            .initial_continuation
            .as_ref()
            .map(|item| item.continuation_id.as_str()),
    )?;
    conn.execute(
        "INSERT INTO execass_lifecycle_transitions (transition_id,delegation_id,state_revision,previous_phase,selected_phase,previous_run_control,selected_run_control,selector_input_json,command_identity,projection_snapshot_json,reason,outbox_event_id,occurred_at) VALUES (?1,?2,?3,?4,?4,?5,?5,'{}',?6,?7,'genesis',?8,?9)",
        params![format!("genesis:{}",command.delegation.delegation_id),command.delegation.delegation_id,command.delegation.state_revision,command.delegation.phase.as_str(),command.delegation.run_control.as_str(),format!("{command:#?}"),snapshot,command.outbox_event.event_id,command.write.occurred_at],
    )?;
    Ok(())
}

impl ExecAssStore {
    /// The sole public storage mutation for an already verified, explicitly
    /// attached follow-up.  It advances one delegation exactly once and never
    /// creates runnable work or an effect.
    pub fn apply_verified_follow_up_amendment(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ApplyVerifiedFollowUpAmendmentCommand,
        follow_up_authority: &VerifiedOwnerAuthority,
        manifest: &CanonicalLeafManifest,
        danger_admission: &SignedDangerAdmissionProof,
    ) -> Result<VerifiedFollowUpAmendmentOutcome> {
        let amendment = &command.amendment;
        let authority = canonicalize_owner_authority(follow_up_authority)
            .map_err(|detail| anyhow::anyhow!("invalid follow-up owner authority: {detail}"))?;
        let authority_record = super::foundation::authority_record_from_manifest(&authority)?;
        validate_verified_follow_up_command(command, &authority_record, manifest)?;

        let result = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            amendment.expected_state_revision,
            &command.receipt,
            |tx| {
                // Replay must be recognized from its immutable transition before
                // consulting current mutable state: a successful amendment has
                // intentionally advanced both the delegation and plan revisions.
                if read_snapshot(tx, &amendment.delegation_id, &amendment.transition_id)?.is_some()
                {
                    let LifecycleWriteOutcome::Replayed(snapshot) =
                        self.amend_lifecycle_in_transaction(tx, amendment)?
                    else {
                        bail!("existing amendment transition did not replay exactly");
                    };
                    let receipt_exists = tx
                        .query_row(
                            "SELECT 1 FROM execass_receipts WHERE causation_event_id=?1",
                            [&command.receipt.causation_event_id],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                    if !receipt_exists {
                        bail!("existing amendment transition is missing its required receipt");
                    }
                    return Ok(AtomicReceiptMutation::NoAppend(
                        VerifiedFollowUpAmendmentOutcome::Replayed(snapshot),
                    ));
                }
                let Some(current) = get_delegation(tx, &amendment.delegation_id)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        VerifiedFollowUpAmendmentOutcome::NotFound,
                    ));
                };
                // A competing writer is a typed stale result, never an
                // authority-validation error against a state the caller did
                // not target.
                if current.state_revision != amendment.expected_state_revision {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        VerifiedFollowUpAmendmentOutcome::Stale {
                            current_state_revision: current.state_revision,
                        },
                    ));
                }
                let expected_plan_revision = current
                    .current_plan_revision
                    .context("follow-up delegation has no current plan")?;
                let expected_scope = serde_json::json!({
                    "delegation_id": current.delegation_id,
                    "delegation_revision": amendment.expected_state_revision,
                    "plan_revision": expected_plan_revision,
                });
                let actual_scope: serde_json::Value =
                    serde_json::from_str(authority.normalized_scope_json())
                        .context("follow-up authority scope is not canonical JSON")?;
                if authority.authority_kind() != "action_specific_owner_amendment"
                    || authority.bound_decision_id().is_some()
                    || authority.bound_decision_revision().is_some()
                    || authority.bound_manifest_digest().is_some()
                    || authority.bound_challenge_nonce_digest().is_some()
                    || owner_normalized_intent_digest(&amendment.normalized_amendment)
                        .as_deref()
                        != Some(authority.normalized_intent_digest().as_hex())
                    || actual_scope != expected_scope
                    || authority.created_at() > amendment.write.occurred_at
                    || authority.expires_at().is_some()
                    || authority.policy_revision() != current.policy_revision
                {
                    bail!("follow-up authority is not bound to this exact live amendment target");
                }
                if manifest.canonical().bytes()
                    != amendment.plan.resolved_leaf_manifest_json.as_bytes()
                    || manifest.canonical().digest().as_hex() != amendment.plan.manifest_digest
                {
                    bail!("follow-up manifest is not the exact owner-authorized replacement plan");
                }
                if manifest.leaves().iter().any(|leaf| {
                    leaf.owner_authority().authority_provenance_id()
                        != authority_record.authority_provenance_id
                }) {
                    bail!("every replacement manifest leaf must carry the exact follow-up authority provenance");
                }
                self.verify_danger_admission_in_tx(tx, danger_admission, manifest)?;
                insert_authority(tx, &authority_record)?;

                match self.amend_lifecycle_in_transaction(tx, amendment)? {
                    LifecycleWriteOutcome::Applied(snapshot) => {
                        super::confirmation::reconcile_accepted_confirmation_grants_for_amendment_in_tx(
                            tx,
                            &current.delegation_id,
                            manifest,
                            danger_admission.proof().routes(),
                            amendment.write.occurred_at,
                            &authority_record.authority_provenance_id,
                        )?;
                        Ok(AtomicReceiptMutation::Append(
                            VerifiedFollowUpAmendmentOutcome::Applied(snapshot),
                        ))
                    }
                    LifecycleWriteOutcome::Replayed(snapshot) => Ok(AtomicReceiptMutation::NoAppend(
                        VerifiedFollowUpAmendmentOutcome::Replayed(snapshot),
                    )),
                    LifecycleWriteOutcome::Stale {
                        current_state_revision,
                    } => Ok(AtomicReceiptMutation::NoAppend(
                        VerifiedFollowUpAmendmentOutcome::Stale {
                            current_state_revision,
                        },
                    )),
                    LifecycleWriteOutcome::NotFound => Ok(AtomicReceiptMutation::NoAppend(
                        VerifiedFollowUpAmendmentOutcome::NotFound,
                    )),
                    LifecycleWriteOutcome::Conflict { .. } => {
                        bail!("amendment lifecycle returned an impossible conflict")
                    }
                }
            },
        )?;
        match result {
            AtomicReceiptWriteOutcome::Appended { value, .. }
            | AtomicReceiptWriteOutcome::NoAppend(value) => Ok(value),
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                ..
            } => Ok(VerifiedFollowUpAmendmentOutcome::Stale {
                current_state_revision,
            }),
        }
    }

    pub(super) fn apply_lifecycle_snapshot(
        &self,
        command: &LifecycleSnapshotCommand,
    ) -> Result<LifecycleWriteOutcome> {
        validate_snapshot(command)?;
        if command.assessment.is_some() {
            self.require_trusted_completion_history()?;
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        if let Some(replayed) = replayed_snapshot(&tx, command)? {
            tx.commit().context("failed closing lifecycle replay")?;
            return Ok(LifecycleWriteOutcome::Replayed(replayed));
        }
        let Some(current) = get_delegation(&tx, &command.delegation_id)? else {
            return Ok(LifecycleWriteOutcome::NotFound);
        };
        if current.state_revision != command.expected_state_revision {
            return Ok(LifecycleWriteOutcome::Stale {
                current_state_revision: current.state_revision,
            });
        }
        if current.phase.is_terminal() {
            bail!("terminal delegation cannot transition; record a terminal correction instead");
        }
        let next_stop_epoch = validate_run_control(&tx, &current, command)?;
        for branch in &command.action_branches {
            validate_action_branch(
                &tx,
                &current,
                branch,
                command.selected_run_control,
                next_stop_epoch,
            )?;
            upsert_action_branch(&tx, branch)?;
        }
        for attention in &command.attention_items {
            validate_attention(&current, attention)?;
            upsert_attention(&tx, attention)?;
        }
        for wait in &command.external_waits {
            validate_external_wait(&current, wait)?;
            upsert_external_wait(&tx, wait)?;
        }

        let assessment = command.assessment.as_ref();
        if let Some(assessment) = assessment {
            validate_assessment(&current, assessment)?;
        }
        let selector_stop_epoch = if command.selected_run_control == RunControlState::StopRequested
        {
            current.stop_epoch
        } else {
            next_stop_epoch
        };
        let input = selector_input(
            &tx,
            &current,
            command.pre_actionable_phase,
            assessment,
            selector_stop_epoch,
        )?;
        let phase = select_lifecycle_phase(input)?;
        if phase.is_terminal() != assessment.is_some() {
            bail!("only a valid completion assessment may select a terminal phase");
        }
        if let Some(continuation) = &command.continuation {
            validate_snapshot_continuation(&tx, &current, command, continuation, next_stop_epoch)?;
        }

        let next_revision = current.state_revision + 1;
        validate_outbox(&command.outbox_event)?;
        if command.outbox_event.event_name != OutboxEventName::DelegationTransitioned
            || command.outbox_event.aggregate_id != current.delegation_id
            || command.outbox_event.aggregate_revision != next_revision
            || command.outbox_event.duplicate_identity != command.write.idempotency_key
            || command.outbox_event.correlation_id != command.write.correlation_id
            || command.outbox_event.causation_id != command.write.causation_id
            || command.outbox_event.occurred_at != command.write.occurred_at
        {
            bail!("lifecycle transition must bind one exact outbox event to its next revision");
        }

        insert_outbox(&tx, &command.outbox_event)?;
        let pending_decision_id = first_actionable_decision(&tx, &current.delegation_id)?;
        let external_wait_json = first_wait_details(&tx, &current.delegation_id)?;
        let assessment_json = assessment.map(|item| item.assessment_json.as_str());
        let terminal_at = phase.is_terminal().then_some(command.write.occurred_at);
        tx.execute(
            r#"UPDATE execass_delegations
               SET phase=?1, run_control=?2, state_revision=?3, pending_decision_id=?4,
                   external_wait_json=?5, completion_assessment_json=?6, updated_at=?7, terminal_at=?8, stop_epoch=?9
             WHERE delegation_id=?10 AND state_revision=?11"#,
            params![
                phase.as_str(), command.selected_run_control.as_str(), next_revision,
                pending_decision_id, external_wait_json, assessment_json, command.write.occurred_at,
                terminal_at, next_stop_epoch, current.delegation_id, current.state_revision,
            ],
        )
        .context("failed applying lifecycle projection CAS")?;
        if let Some(assessment) = assessment {
            insert_assessment(&tx, assessment)?;
        }
        if let Some(continuation) = &command.continuation {
            insert_continuation(&tx, continuation)?;
        }
        let input_json = selector_input_json(input);
        let command_identity = lifecycle_command_identity(command);
        let projection_snapshot_json = projection_snapshot_json(
            &tx,
            &current.delegation_id,
            command
                .continuation
                .as_ref()
                .map(|item| item.continuation_id.as_str()),
        )?;
        tx.execute(
            r#"INSERT INTO execass_lifecycle_transitions (
                 transition_id, delegation_id, state_revision, previous_phase, selected_phase,
                 previous_run_control, selected_run_control, selector_input_json, command_identity, projection_snapshot_json, reason,
                 outbox_event_id, occurred_at
               ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
            params![
                command.transition_id,
                current.delegation_id,
                next_revision,
                current.phase.as_str(),
                phase.as_str(),
                current.run_control.as_str(),
                command.selected_run_control.as_str(),
                input_json,
                command_identity,
                projection_snapshot_json,
                command.reason,
                command.outbox_event.event_id,
                command.write.occurred_at,
            ],
        )
        .context("failed recording immutable lifecycle transition")?;
        let snapshot = read_snapshot(&tx, &command.delegation_id, &command.transition_id)?
            .context("applied lifecycle snapshot disappeared")?;
        tx.commit()
            .context("failed committing lifecycle snapshot")?;
        Ok(LifecycleWriteOutcome::Applied(snapshot))
    }

    fn require_trusted_completion_history(&self) -> Result<()> {
        let state_root = self
            .db_path
            .parent()
            .context("ExecAss database has no canonical state root")?;
        let paths = AppPaths::from_root(state_root);
        let canonical_database = paths
            .db_path
            .canonicalize()
            .context("failed resolving the completion receipt database")?;
        if canonical_database != self.db_path {
            bail!("completion receipt authority does not match this ExecAss store");
        }
        let integrity = ReceiptIntegrityStore::open(&paths)
            .context("completion requires the fixed receipt-integrity authority")?;
        match integrity.status()? {
            IntegrityStatus::Trusted { .. } => Ok(()),
            IntegrityStatus::Uninitialized
            | IntegrityStatus::Prepared { .. }
            | IntegrityStatus::KeyLost { .. }
            | IntegrityStatus::Mismatch { .. }
            | IntegrityStatus::Quarantined { .. } => {
                bail!("completion requires trusted receipt history")
            }
        }
    }

    pub(super) fn record_terminal_correction(
        &self,
        command: &TerminalCorrectionCommand,
    ) -> Result<LifecycleWriteOutcome> {
        require_text("correction_id", &command.correction.correction_id)?;
        require_text("delegation_id", &command.correction.delegation_id)?;
        require_text("warning", &command.correction.warning)?;
        validate_outbox(&command.outbox_event)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(delegation) = get_delegation(&tx, &command.correction.delegation_id)? else {
            return Ok(LifecycleWriteOutcome::NotFound);
        };
        if !delegation.phase.is_terminal() {
            bail!("terminal corrections require an existing terminal assessment");
        }
        let terminal_exists = tx.query_row(
            "SELECT 1 FROM execass_completion_assessments WHERE assessment_id=?1 AND delegation_id=?2",
            params![command.correction.terminal_assessment_id, command.correction.delegation_id], |_| Ok(()),
        ).optional()?.is_some();
        if !terminal_exists {
            bail!("terminal correction must link its immutable terminal assessment");
        }
        if let Some(identity) = tx
            .query_row(
                "SELECT command_identity FROM execass_terminal_corrections WHERE correction_id=?1",
                params![command.correction.correction_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            if identity != format!("{command:#?}") {
                bail!("terminal correction identity conflicts with an immutable prior write");
            }
            let transition = latest_transition(&tx, &delegation.delegation_id)?
                .context("terminal delegation has no lifecycle transition")?;
            let outbox_event = get_outbox(&tx, &command.outbox_event.event_id)?
                .context("terminal correction outbox missing")?;
            if outbox_event.event != command.outbox_event {
                bail!("terminal correction outbox conflicts with immutable replay");
            }
            tx.commit()
                .context("failed closing terminal correction replay")?;
            return Ok(LifecycleWriteOutcome::Replayed(LifecycleSnapshot {
                delegation,
                transition,
                continuation: None,
                outbox_event,
            }));
        }
        if command.outbox_event.aggregate_id != delegation.delegation_id
            || command.outbox_event.event_name != OutboxEventName::CompletionAssessed
            || command.outbox_event.aggregate_revision != delegation.state_revision
            || command.outbox_event.duplicate_identity != command.write.idempotency_key
            || command.outbox_event.correlation_id != command.write.correlation_id
            || command.outbox_event.causation_id != command.write.causation_id
            || command.outbox_event.occurred_at != command.write.occurred_at
            || command.correction.recorded_at != command.write.occurred_at
        {
            bail!("terminal correction must bind its exact write context and terminal revision");
        }
        insert_outbox(&tx, &command.outbox_event)?;
        tx.execute(
            r#"INSERT INTO execass_terminal_corrections (
                correction_id,delegation_id,terminal_assessment_id,correction_revision,
                contrary_evidence_json,warning,recorded_at,command_identity,outbox_event_id
              ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
            params![
                command.correction.correction_id,
                command.correction.delegation_id,
                command.correction.terminal_assessment_id,
                command.correction.correction_revision,
                command.correction.contrary_evidence_json,
                command.correction.warning,
                command.correction.recorded_at,
                format!("{command:#?}"),
                command.outbox_event.event_id
            ],
        )?;
        let transition = latest_transition(&tx, &delegation.delegation_id)?
            .context("terminal delegation has no lifecycle transition")?;
        let outbox_event = get_outbox(&tx, &command.outbox_event.event_id)?
            .context("terminal correction outbox missing")?;
        tx.commit()
            .context("failed committing terminal correction")?;
        Ok(LifecycleWriteOutcome::Applied(LifecycleSnapshot {
            delegation,
            transition,
            continuation: None,
            outbox_event,
        }))
    }

    /// Appends a material amendment and atomically supersedes its prior plan,
    /// criteria linkage, pending decisions, and nonterminal continuations.
    pub(super) fn amend_lifecycle(
        &self,
        command: &AmendLifecycleCommand,
    ) -> Result<LifecycleWriteOutcome> {
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let outcome = self.amend_lifecycle_in_transaction(&tx, command)?;
        tx.commit()?;
        Ok(outcome)
    }

    fn amend_lifecycle_in_transaction(
        &self,
        tx: &Transaction<'_>,
        command: &AmendLifecycleCommand,
    ) -> Result<LifecycleWriteOutcome> {
        require_text("amendment_id", &command.amendment_id)?;
        require_text("transition_id", &command.transition_id)?;
        require_text("normalized_amendment", &command.normalized_amendment)?;
        validate_plan(&command.plan)?;
        if command.outcome_criteria.is_empty() {
            bail!("amendment requires replacement outcome criteria");
        }
        for criterion in &command.outcome_criteria {
            validate_criterion(criterion)?;
        }
        if let Some(snapshot) = read_snapshot(tx, &command.delegation_id, &command.transition_id)? {
            let stored_identity: String = tx.query_row(
                "SELECT command_identity FROM execass_lifecycle_transitions WHERE transition_id=?1",
                params![command.transition_id],
                |row| row.get(0),
            )?;
            if stored_identity == format!("{command:#?}")
                && snapshot.outbox_event.event == command.outbox_event
            {
                return Ok(LifecycleWriteOutcome::Replayed(snapshot));
            }
            bail!("amendment transition conflicts with immutable prior write");
        }
        let Some(current) = get_delegation(tx, &command.delegation_id)? else {
            return Ok(LifecycleWriteOutcome::NotFound);
        };
        if current.state_revision != command.expected_state_revision {
            return Ok(LifecycleWriteOutcome::Stale {
                current_state_revision: current.state_revision,
            });
        }
        if current.phase.is_terminal() {
            bail!("terminal delegation cannot be amended");
        }
        let old_plan = current
            .current_plan_revision
            .context("delegation has no current plan")?;
        let old_criteria = current
            .current_criteria_revision
            .context("delegation has no current criteria")?;
        let next = current.state_revision + 1;
        let criteria_revision = command.outcome_criteria[0].criteria_revision;
        if command.plan.delegation_id != current.delegation_id
            || command.plan.plan_revision <= old_plan
            || command.plan.based_on_delegation_revision != next
            || command.plan.policy_revision != current.policy_revision
            || command.plan.created_by_authority_provenance_id != command.authority_provenance_id
            || criteria_revision <= old_criteria
            || command.outcome_criteria.iter().any(|item| {
                item.delegation_id != current.delegation_id
                    || item.criteria_revision != criteria_revision
            })
        {
            bail!("amendment plan/criteria are not bound to the exact next revision");
        }
        if command.outbox_event.event_name != OutboxEventName::DelegationTransitioned
            || command.outbox_event.aggregate_id != current.delegation_id
            || command.outbox_event.aggregate_revision != next
            || command.outbox_event.duplicate_identity != command.write.idempotency_key
            || command.outbox_event.correlation_id != command.write.correlation_id
            || command.outbox_event.causation_id != command.write.causation_id
            || command.outbox_event.occurred_at != command.write.occurred_at
        {
            bail!("amendment requires one exact next-revision transition event");
        }
        insert_plan(tx, &command.plan)?;
        for criterion in &command.outcome_criteria {
            insert_criterion(tx, criterion)?;
        }
        tx.execute("INSERT INTO execass_criteria_sets (criteria_set_id,delegation_id,criteria_revision,parent_criteria_revision,disposition,created_at) VALUES (?1,?2,?3,?4,'current',?5)", params![format!("criteria-set:{}:{criteria_revision}",current.delegation_id),current.delegation_id,criteria_revision,old_criteria,command.write.occurred_at])?;
        tx.execute("UPDATE execass_criteria_sets SET disposition='superseded' WHERE delegation_id=?1 AND criteria_revision=?2", params![current.delegation_id,old_criteria])?;
        tx.execute("INSERT INTO execass_plan_amendments (amendment_id,delegation_id,amendment_revision,superseded_plan_revision,resulting_plan_revision,normalized_amendment,intake_evidence_json,authority_provenance_id,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![command.amendment_id,current.delegation_id,command.amendment_revision,old_plan,command.plan.plan_revision,command.normalized_amendment,command.intake_evidence_json,command.authority_provenance_id,command.write.occurred_at])?;
        tx.execute("INSERT INTO execass_amendment_criteria_links (amendment_id,delegation_id,superseded_criteria_revision,resulting_criteria_revision) VALUES (?1,?2,?3,?4)", params![command.amendment_id,current.delegation_id,old_criteria,criteria_revision])?;
        tx.execute("UPDATE execass_decisions SET status='superseded' WHERE delegation_id=?1 AND status='pending'", params![current.delegation_id])?;
        tx.execute("UPDATE execass_continuations SET status='superseded',completed_at=?1,updated_at=?1 WHERE delegation_id=?2 AND status NOT IN ('terminal','superseded')", params![command.write.occurred_at,current.delegation_id])?;
        tx.execute("UPDATE execass_action_branches SET status='superseded',terminal_at=?1,updated_at=?1 WHERE delegation_id=?2 AND status NOT IN ('terminal','superseded')", params![command.write.occurred_at,current.delegation_id])?;
        tx.execute("UPDATE execass_attention_items SET status='superseded',resolved_at=?1 WHERE delegation_id=?2 AND status='actionable'", params![command.write.occurred_at,current.delegation_id])?;
        tx.execute("UPDATE execass_external_waits SET status='superseded',resolved_at=?1 WHERE delegation_id=?2 AND status='waiting'", params![command.write.occurred_at,current.delegation_id])?;
        insert_outbox(tx, &command.outbox_event)?;
        tx.execute("UPDATE execass_delegations SET phase='planning',state_revision=?1,current_plan_revision=?2,current_criteria_revision=?3,pending_decision_id=NULL,external_wait_json=NULL,updated_at=?4 WHERE delegation_id=?5 AND state_revision=?6", params![next,command.plan.plan_revision,criteria_revision,command.write.occurred_at,current.delegation_id,current.state_revision])?;
        let amendment_snapshot = projection_snapshot_json(tx, &current.delegation_id, None)?;
        tx.execute("INSERT INTO execass_lifecycle_transitions (transition_id,delegation_id,state_revision,previous_phase,selected_phase,previous_run_control,selected_run_control,selector_input_json,command_identity,projection_snapshot_json,reason,outbox_event_id,occurred_at) VALUES (?1,?2,?3,?4,'planning',?5,?5,?6,?7,?8,'amendment',?9,?10)", params![command.transition_id,current.delegation_id,next,current.phase.as_str(),current.run_control.as_str(),selector_input_json(LifecycleSelectorInput { completion_assessment: None, pre_actionable_phase: Some(PreActionablePhase::Planning), ordinary_runnable_or_executing: false, recovery_runnable_or_executing: false, actionable_attention: false, external_wait: false }),format!("{command:#?}"),amendment_snapshot,command.outbox_event.event_id,command.write.occurred_at])?;
        let snapshot = read_snapshot(tx, &current.delegation_id, &command.transition_id)?
            .context("amendment snapshot disappeared")?;
        Ok(LifecycleWriteOutcome::Applied(snapshot))
    }

    /// Rebuilds the mutable lifecycle projection from immutable genesis and
    /// transition snapshots. It intentionally does not execute work or infer
    /// effects; it only restores EA-109 projection fields.
    pub(super) fn rebuild_lifecycle_projection(
        &self,
        delegation_id: &str,
    ) -> Result<Option<DelegationRecord>> {
        require_text("delegation_id", delegation_id)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let snapshot: Option<String> = tx.query_row("SELECT projection_snapshot_json FROM execass_lifecycle_transitions WHERE delegation_id=?1 ORDER BY state_revision DESC LIMIT 1", params![delegation_id], |row| row.get(0)).optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let value: serde_json::Value = serde_json::from_str(&snapshot)?;
        restore_lifecycle_subprojections(&tx, delegation_id, &value)?;
        tx.execute("UPDATE execass_delegations SET phase=?1,run_control=?2,state_revision=?3,current_plan_revision=?4,current_criteria_revision=?5,pending_decision_id=?6,external_wait_json=?7,completion_assessment_json=?8,stop_epoch=?9,updated_at=?10,terminal_at=?11 WHERE delegation_id=?12", params![value["phase"].as_str(),value["run_control"].as_str(),value["state_revision"].as_i64(),value["current_plan_revision"].as_i64(),value["current_criteria_revision"].as_i64(),value["pending_decision_id"].as_str(),value["external_wait_json"].as_str(),value["completion_assessment_json"].as_str(),value["stop_epoch"].as_i64(),value["updated_at"].as_i64(),value["terminal_at"].as_i64(),delegation_id])?;
        let rebuilt = projection_snapshot_json(
            &tx,
            delegation_id,
            value["transition_continuation_id"].as_str(),
        )?;
        if serde_json::from_str::<serde_json::Value>(&rebuilt)? != value {
            bail!("rebuilt lifecycle projection does not match immutable history");
        }
        let record = get_delegation(&tx, delegation_id)?;
        tx.commit()?;
        Ok(record)
    }
}

fn validate_verified_follow_up_command(
    command: &ApplyVerifiedFollowUpAmendmentCommand,
    authority: &AuthorityProvenanceRecord,
    manifest: &CanonicalLeafManifest,
) -> Result<()> {
    let amendment = &command.amendment;
    let receipt = &command.receipt;
    require_text("follow-up delegation_id", &amendment.delegation_id)?;
    if amendment.authority_provenance_id != authority.authority_provenance_id
        || amendment.plan.created_by_authority_provenance_id != authority.authority_provenance_id
        || receipt.delegation_id != amendment.delegation_id
        || receipt.expected_state_revision != amendment.expected_state_revision + 1
        || receipt.receipt_kind != ReceiptKind::Amendment
        || receipt.subject.kind != ReceiptSubjectKind::PlanAmendment
        || receipt.subject.subject_id != amendment.amendment_id
        || receipt.subject.revision != amendment.amendment_revision
        || receipt.causation_id != amendment.write.causation_id
        || receipt.causation_event_id != amendment.outbox_event.event_id
        || receipt.actor.actor_type != authority.actor_type
        || receipt.actor.actor_identity.as_str() != authority.credential_identity
        || receipt.actor.authority_provenance_id != authority.authority_provenance_id
        || receipt.occurred_at != amendment.write.occurred_at
        || receipt.committed_at != amendment.write.occurred_at
        || manifest.canonical().bytes() != amendment.plan.resolved_leaf_manifest_json.as_bytes()
        || manifest.canonical().digest().as_hex() != amendment.plan.manifest_digest
    {
        bail!("verified follow-up command does not bind one exact amendment receipt");
    }
    Ok(())
}

fn restore_lifecycle_subprojections(
    conn: &Connection,
    delegation_id: &str,
    snapshot: &serde_json::Value,
) -> Result<()> {
    let array_json = |name: &str| -> Result<String> {
        let value = snapshot[name]
            .as_array()
            .with_context(|| format!("snapshot {name} array missing"))?;
        serde_json::to_string(value).map_err(Into::into)
    };
    let branches = array_json("branches")?;
    let continuations = array_json("continuations")?;
    let attention = array_json("attention_items")?;
    let waits = array_json("external_waits")?;

    conn.execute(
        "DELETE FROM execass_attention_items WHERE delegation_id=?1 AND attention_id NOT IN (SELECT json_extract(value,'$.attention_id') FROM json_each(?2))",
        params![delegation_id, attention],
    )?;
    conn.execute(
        "DELETE FROM execass_external_waits WHERE delegation_id=?1 AND external_wait_id NOT IN (SELECT json_extract(value,'$.external_wait_id') FROM json_each(?2))",
        params![delegation_id, waits],
    )?;
    conn.execute(
        "DELETE FROM execass_continuations WHERE delegation_id=?1 AND continuation_id NOT IN (SELECT json_extract(value,'$.continuation_id') FROM json_each(?2))",
        params![delegation_id, continuations],
    )?;
    conn.execute(
        "DELETE FROM execass_action_branches WHERE delegation_id=?1 AND action_id NOT IN (SELECT json_extract(value,'$.action_id') FROM json_each(?2))",
        params![delegation_id, branches],
    )?;

    conn.execute(
        r#"INSERT INTO execass_action_branches (
          action_id,delegation_id,action_revision,target_delegation_revision,target_plan_revision,
          stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
        ) SELECT
          json_extract(value,'$.action_id'),?2,json_extract(value,'$.action_revision'),
          json_extract(value,'$.target_delegation_revision'),json_extract(value,'$.target_plan_revision'),
          json_extract(value,'$.stop_epoch'),json_extract(value,'$.branch_kind'),json_extract(value,'$.status'),
          json_extract(value,'$.action_summary'),json_extract(value,'$.created_at'),json_extract(value,'$.updated_at'),
          json_extract(value,'$.terminal_at') FROM json_each(?1) WHERE 1
        ON CONFLICT(action_id) DO UPDATE SET
          delegation_id=excluded.delegation_id,action_revision=excluded.action_revision,
          target_delegation_revision=excluded.target_delegation_revision,
          target_plan_revision=excluded.target_plan_revision,stop_epoch=excluded.stop_epoch,
          branch_kind=excluded.branch_kind,status=excluded.status,action_summary=excluded.action_summary,
          created_at=excluded.created_at,updated_at=excluded.updated_at,terminal_at=excluded.terminal_at"#,
        params![branches, delegation_id],
    )?;
    conn.execute(
        r#"INSERT INTO execass_continuations (
          continuation_id,delegation_id,target_delegation_revision,target_plan_revision,action_id,
          branch_kind,causation_kind,causation_id,status,job_id,lease_owner,lease_expires_at,
          fencing_token,host_generation,stop_epoch,global_stop_epoch,created_at,updated_at,completed_at
        ) SELECT
          json_extract(value,'$.continuation_id'),?2,json_extract(value,'$.target_delegation_revision'),
          json_extract(value,'$.target_plan_revision'),json_extract(value,'$.action_id'),
          json_extract(value,'$.branch_kind'),json_extract(value,'$.causation_kind'),json_extract(value,'$.causation_id'),
          json_extract(value,'$.status'),json_extract(value,'$.job_id'),json_extract(value,'$.lease_owner'),
          json_extract(value,'$.lease_expires_at'),json_extract(value,'$.fencing_token'),json_extract(value,'$.host_generation'),
          json_extract(value,'$.stop_epoch'),json_extract(value,'$.global_stop_epoch'),json_extract(value,'$.created_at'),json_extract(value,'$.updated_at'),
          json_extract(value,'$.completed_at') FROM json_each(?1) WHERE 1
        ON CONFLICT(continuation_id) DO UPDATE SET
          status=excluded.status,job_id=excluded.job_id,lease_owner=excluded.lease_owner,
          lease_expires_at=excluded.lease_expires_at,fencing_token=excluded.fencing_token,
          host_generation=excluded.host_generation,updated_at=excluded.updated_at,completed_at=excluded.completed_at"#,
        params![continuations, delegation_id],
    )?;
    conn.execute(
        r#"INSERT INTO execass_attention_items (
          attention_id,delegation_id,action_id,kind,status,reason,recommendation,alternatives_json,
          required_assurance,decision_id,delegation_revision,created_at,resolved_at
        ) SELECT
          json_extract(value,'$.attention_id'),?2,json_extract(value,'$.action_id'),json_extract(value,'$.kind'),
          json_extract(value,'$.status'),json_extract(value,'$.reason'),json_extract(value,'$.recommendation'),
          json_extract(value,'$.alternatives_json'),json_extract(value,'$.required_assurance'),
          json_extract(value,'$.decision_id'),json_extract(value,'$.delegation_revision'),
          json_extract(value,'$.created_at'),json_extract(value,'$.resolved_at') FROM json_each(?1) WHERE 1
        ON CONFLICT(attention_id) DO UPDATE SET
          delegation_id=excluded.delegation_id,action_id=excluded.action_id,kind=excluded.kind,
          status=excluded.status,reason=excluded.reason,recommendation=excluded.recommendation,
          alternatives_json=excluded.alternatives_json,required_assurance=excluded.required_assurance,
          decision_id=excluded.decision_id,delegation_revision=excluded.delegation_revision,
          created_at=excluded.created_at,resolved_at=excluded.resolved_at"#,
        params![attention, delegation_id],
    )?;
    conn.execute(
        r#"INSERT INTO execass_external_waits (
          external_wait_id,delegation_id,action_id,kind,status,reason,details_json,
          delegation_revision,created_at,resolved_at
        ) SELECT
          json_extract(value,'$.external_wait_id'),?2,json_extract(value,'$.action_id'),json_extract(value,'$.kind'),
          json_extract(value,'$.status'),json_extract(value,'$.reason'),json_extract(value,'$.details_json'),
          json_extract(value,'$.delegation_revision'),json_extract(value,'$.created_at'),
          json_extract(value,'$.resolved_at') FROM json_each(?1) WHERE 1
        ON CONFLICT(external_wait_id) DO UPDATE SET
          delegation_id=excluded.delegation_id,action_id=excluded.action_id,kind=excluded.kind,
          status=excluded.status,reason=excluded.reason,details_json=excluded.details_json,
          delegation_revision=excluded.delegation_revision,created_at=excluded.created_at,
          resolved_at=excluded.resolved_at"#,
        params![waits, delegation_id],
    )?;
    Ok(())
}

fn validate_snapshot(command: &LifecycleSnapshotCommand) -> Result<()> {
    require_text("transition_id", &command.transition_id)?;
    require_text("delegation_id", &command.delegation_id)?;
    require_text("reason", &command.reason)?;
    require_text("write.idempotency_key", &command.write.idempotency_key)?;
    require_text("write.correlation_id", &command.write.correlation_id)?;
    require_text("write.causation_id", &command.write.causation_id)?;
    if command.expected_state_revision < 1 {
        bail!("lifecycle CAS requires a positive expected revision");
    }
    if command
        .continuation
        .as_ref()
        .is_some_and(|item| item.status != ContinuationStatus::Runnable)
    {
        bail!("a lifecycle mutation may create only one runnable continuation");
    }
    Ok(())
}

fn validate_run_control(
    conn: &Connection,
    current: &DelegationRecord,
    command: &LifecycleSnapshotCommand,
) -> Result<i64> {
    let next = command.selected_run_control;
    if current.run_control == RunControlState::Stopped && next != RunControlState::Running {
        bail!("stopped delegation requires explicit resume to running");
    }
    if next == RunControlState::Stopped {
        if current.run_control != RunControlState::StopRequested {
            bail!("stopped requires a prior stop_requested drain");
        }
        let executing_continuations: i64 = conn.query_row(
            "SELECT COUNT(*) FROM execass_continuations WHERE delegation_id=?1 AND status='executing'",
            params![current.delegation_id],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT action_id FROM execass_action_branches WHERE delegation_id=?1 AND status='executing' ORDER BY action_id",
        )?;
        let executing_actions = statement
            .query_map(params![current.delegation_id], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let unresolved_action = executing_actions.iter().any(|action_id| {
            !command.action_branches.iter().any(|branch| {
                &branch.action_id == action_id
                    && matches!(
                        branch.status,
                        ContinuationStatus::Terminal | ContinuationStatus::Superseded
                    )
            })
        });
        if executing_continuations != 0 || unresolved_action {
            bail!("stop drain cannot expose stopped while an action is executing");
        }
    }
    if next == RunControlState::StopRequested && current.run_control != RunControlState::Running {
        bail!("stop_requested may only begin from running");
    }
    if next == RunControlState::Running && current.run_control == RunControlState::StopRequested {
        bail!("stop_requested must drain to stopped before explicit resume");
    }
    if next == RunControlState::Running && current.run_control == RunControlState::Stopped {
        let proof = command
            .resume_proof
            .as_ref()
            .context("resume requires a fresh exact resume proof")?;
        if proof.plan_revision
            != current
                .current_plan_revision
                .context("resume missing current plan")?
            || proof.policy_revision != current.policy_revision
            || proof.authority_provenance_id != current.authority_provenance_id
            || proof.budget_snapshot_digest.trim().is_empty()
            || proof.global_stop_epoch < 0
        {
            bail!("resume proof is stale or incomplete");
        }
        if command.continuation.is_none() {
            bail!("resume requires exactly one current runnable continuation");
        }
    } else if command.resume_proof.is_some() {
        bail!("resume proof is only legal for stopped-to-running resume");
    }
    Ok(
        if next == RunControlState::StopRequested
            || (next == RunControlState::Running && current.run_control == RunControlState::Stopped)
        {
            current.stop_epoch + 1
        } else {
            current.stop_epoch
        },
    )
}

fn validate_action_branch(
    conn: &Connection,
    current: &DelegationRecord,
    branch: &ActionBranchRecord,
    control: RunControlState,
    next_stop_epoch: i64,
) -> Result<()> {
    require_text("action_id", &branch.action_id)?;
    require_text("action_summary", &branch.action_summary)?;
    if branch.delegation_id != current.delegation_id
        || branch.target_plan_revision
            != current
                .current_plan_revision
                .context("delegation has no active plan")?
    {
        bail!("action branch must bind the current delegation and plan revision");
    }
    let existing: Option<StoredActionBranchIdentity> = conn
        .query_row(
            "SELECT action_revision,target_delegation_revision,target_plan_revision,stop_epoch,branch_kind,action_summary,created_at,status FROM execass_action_branches WHERE action_id=?1",
            params![branch.action_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
        )
        .optional()?;
    if let Some((
        action_revision,
        target_revision,
        plan_revision,
        stop_epoch,
        kind,
        summary,
        created_at,
        status,
    )) = existing
    {
        if action_revision != branch.action_revision
            || target_revision != branch.target_delegation_revision
            || plan_revision != branch.target_plan_revision
            || stop_epoch != branch.stop_epoch
            || kind != branch.branch_kind.as_str()
            || summary != branch.action_summary
            || created_at != branch.created_at
        {
            bail!("action branch identity conflicts with an immutable prior record");
        }
        if matches!(status.as_str(), "terminal" | "superseded") && status != branch.status.as_str()
        {
            bail!("terminal action branch cannot be reopened");
        }
    } else if branch.target_delegation_revision != current.state_revision + 1
        || branch.stop_epoch != next_stop_epoch
    {
        bail!("new action branch must bind the exact next revision and stop epoch");
    }
    if control != RunControlState::Running
        && matches!(
            branch.status,
            ContinuationStatus::Runnable | ContinuationStatus::Executing
        )
    {
        bail!("stop_requested/stopped blocks new runnable or executing action branches");
    }
    Ok(())
}

fn validate_attention(current: &DelegationRecord, item: &AttentionItemRecord) -> Result<()> {
    require_text("attention_id", &item.attention_id)?;
    require_text("attention.reason", &item.reason)?;
    require_text("attention.recommendation", &item.recommendation)?;
    if item.delegation_id != current.delegation_id
        || item.delegation_revision != current.state_revision + 1
    {
        bail!("attention item must bind the mutation's exact next delegation revision");
    }
    serde_json::from_str::<serde_json::Value>(&item.alternatives_json)
        .context("attention alternatives_json must be JSON")?;
    Ok(())
}

fn validate_external_wait(current: &DelegationRecord, item: &ExternalWaitRecord) -> Result<()> {
    require_text("external_wait_id", &item.external_wait_id)?;
    require_text("external_wait.reason", &item.reason)?;
    if item.delegation_id != current.delegation_id
        || item.delegation_revision != current.state_revision + 1
    {
        bail!("external wait must bind the mutation's exact next delegation revision");
    }
    serde_json::from_str::<serde_json::Value>(&item.details_json)
        .context("external wait details_json must be JSON")?;
    Ok(())
}

fn validate_assessment(
    current: &DelegationRecord,
    assessment: &CompletionAssessmentRecord,
) -> Result<()> {
    require_text("assessment_id", &assessment.assessment_id)?;
    if assessment.delegation_id != current.delegation_id
        || assessment.criteria_revision
            != current
                .current_criteria_revision
                .context("delegation has no active criteria")?
    {
        bail!("completion assessment must bind current delegation criteria");
    }
    serde_json::from_str::<serde_json::Value>(&assessment.assessment_json)
        .context("assessment_json must be JSON")?;
    match assessment.kind {
        CompletionAssessmentKind::Completed
            if assessment.material_fail_count != 0 || assessment.material_unknown_count != 0 =>
        {
            bail!("completed requires no material failure or unknown")
        }
        CompletionAssessmentKind::PartiallyCompleted
            if !assessment.useful_outcome
                || !assessment.no_remaining_path
                || assessment
                    .exact_unmet_portion
                    .as_deref()
                    .is_none_or(str::is_empty) =>
        {
            bail!("partial completion requires useful result, exact gaps, and no remaining path")
        }
        CompletionAssessmentKind::Failed if assessment.useful_outcome => {
            bail!("failed requires no useful requested outcome")
        }
        _ => Ok(()),
    }
}

fn validate_snapshot_continuation(
    conn: &Connection,
    current: &DelegationRecord,
    command: &LifecycleSnapshotCommand,
    continuation: &ContinuationRecord,
    next_stop_epoch: i64,
) -> Result<()> {
    validate_continuation(continuation)?;
    if continuation.delegation_id != current.delegation_id
        || continuation.target_delegation_revision != current.state_revision + 1
        || continuation.target_plan_revision
            != current
                .current_plan_revision
                .context("delegation has no active plan")?
        || continuation.stop_epoch != next_stop_epoch
        || command.selected_run_control != RunControlState::Running
    {
        bail!("continuation must target the exact running next revision and current plan");
    }
    let branch: Option<(String, i64, i64, i64, String)> = conn.query_row("SELECT branch_kind,target_delegation_revision,target_plan_revision,stop_epoch,status FROM execass_action_branches WHERE action_id=?1", params![continuation.action_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let Some((kind, revision, plan, stop_epoch, status)) = branch else {
        bail!("continuation action branch is missing");
    };
    if kind != continuation.branch_kind.as_str()
        || revision != current.state_revision + 1
        || plan != continuation.target_plan_revision
        || stop_epoch != next_stop_epoch
        || status != "runnable"
    {
        bail!("continuation must bind a current runnable branch at exact plan and stop epoch");
    }
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM execass_continuations WHERE delegation_id=?1 AND target_delegation_revision=?2 AND status='runnable'", params![current.delegation_id,current.state_revision+1], |row| row.get(0))?;
    if count != 0 {
        bail!("transition already has a current runnable continuation");
    }
    Ok(())
}

fn selector_input(
    conn: &Connection,
    current: &DelegationRecord,
    pre: Option<PreActionablePhase>,
    assessment: Option<&CompletionAssessmentRecord>,
    active_stop_epoch: i64,
) -> Result<LifecycleSelectorInput> {
    let active_plan = current
        .current_plan_revision
        .context("delegation has no active plan")?;
    let action_counts = |kind: ActionBranchKind| -> Result<i64> {
        conn.query_row(
        "SELECT COUNT(*) FROM execass_action_branches WHERE delegation_id=?1 AND branch_kind=?2 AND target_plan_revision=?3 AND stop_epoch=?4 AND status IN ('runnable','executing')",
        params![current.delegation_id, kind.as_str(), active_plan, active_stop_epoch], |row| row.get(0)).map_err(Into::into)
    };
    let attention: i64 = conn.query_row("SELECT COUNT(*) FROM execass_attention_items WHERE delegation_id=?1 AND status='actionable'", params![current.delegation_id], |row| row.get(0))?;
    let waits: i64 = conn.query_row(
        "SELECT COUNT(*) FROM execass_external_waits WHERE delegation_id=?1 AND status='waiting'",
        params![current.delegation_id],
        |row| row.get(0),
    )?;
    Ok(LifecycleSelectorInput {
        completion_assessment: assessment.map(|item| item.kind),
        pre_actionable_phase: pre,
        ordinary_runnable_or_executing: action_counts(ActionBranchKind::Ordinary)? > 0,
        recovery_runnable_or_executing: action_counts(ActionBranchKind::Recovery)? > 0,
        actionable_attention: attention > 0,
        external_wait: waits > 0,
    })
}

fn selector_input_json(input: LifecycleSelectorInput) -> String {
    serde_json::json!({
        "completion_assessment": input.completion_assessment.map(|item| item.phase().as_str()),
        "pre_actionable_phase": input.pre_actionable_phase.map(|item| item.phase().as_str()),
        "ordinary_runnable_or_executing": input.ordinary_runnable_or_executing,
        "recovery_runnable_or_executing": input.recovery_runnable_or_executing,
        "actionable_attention": input.actionable_attention,
        "external_wait": input.external_wait,
    })
    .to_string()
}

fn lifecycle_command_identity(command: &LifecycleSnapshotCommand) -> String {
    // All command members are typed and Debug is structurally complete for
    // these closed records; this is deliberately persisted before mutation so
    // replays compare the original complete command, not mutable projection.
    format!("{command:#?}")
}

pub(super) fn projection_snapshot_json(
    conn: &Connection,
    delegation_id: &str,
    transition_continuation_id: Option<&str>,
) -> Result<String> {
    conn.query_row(
        r#"SELECT json_object(
          'phase', d.phase, 'run_control', d.run_control, 'state_revision', d.state_revision,
          'current_plan_revision', d.current_plan_revision, 'current_criteria_revision', d.current_criteria_revision,
          'pending_decision_id', d.pending_decision_id, 'external_wait_json', d.external_wait_json,
          'completion_assessment_json', d.completion_assessment_json, 'stop_epoch', d.stop_epoch,
          'updated_at', d.updated_at, 'terminal_at', d.terminal_at,
          'transition_continuation_id', ?2,
          'branches', json(COALESCE((SELECT json_group_array(json_object('action_id',action_id,'action_revision',action_revision,'target_delegation_revision',target_delegation_revision,'target_plan_revision',target_plan_revision,'stop_epoch',stop_epoch,'branch_kind',branch_kind,'status',status,'action_summary',action_summary,'created_at',created_at,'updated_at',updated_at,'terminal_at',terminal_at)) FROM (SELECT * FROM execass_action_branches WHERE delegation_id=d.delegation_id ORDER BY action_id)),'[]')),
          'continuations', json(COALESCE((SELECT json_group_array(json_object('continuation_id',continuation_id,'target_delegation_revision',target_delegation_revision,'target_plan_revision',target_plan_revision,'action_id',action_id,'branch_kind',branch_kind,'causation_kind',causation_kind,'causation_id',causation_id,'status',status,'job_id',job_id,'lease_owner',lease_owner,'lease_expires_at',lease_expires_at,'fencing_token',fencing_token,'host_generation',host_generation,'stop_epoch',stop_epoch,'global_stop_epoch',global_stop_epoch,'created_at',created_at,'updated_at',updated_at,'completed_at',completed_at)) FROM (SELECT * FROM execass_continuations WHERE delegation_id=d.delegation_id ORDER BY continuation_id)),'[]')),
          'attention_items', json(COALESCE((SELECT json_group_array(json_object('attention_id',attention_id,'action_id',action_id,'kind',kind,'status',status,'reason',reason,'recommendation',recommendation,'alternatives_json',alternatives_json,'required_assurance',required_assurance,'decision_id',decision_id,'delegation_revision',delegation_revision,'created_at',created_at,'resolved_at',resolved_at)) FROM (SELECT * FROM execass_attention_items WHERE delegation_id=d.delegation_id ORDER BY attention_id)),'[]')),
          'external_waits', json(COALESCE((SELECT json_group_array(json_object('external_wait_id',external_wait_id,'action_id',action_id,'kind',kind,'status',status,'reason',reason,'details_json',details_json,'delegation_revision',delegation_revision,'created_at',created_at,'resolved_at',resolved_at)) FROM (SELECT * FROM execass_external_waits WHERE delegation_id=d.delegation_id ORDER BY external_wait_id)),'[]')),
          'completion_assessments', json(COALESCE((SELECT json_group_array(json_object('assessment_id',assessment_id,'assessment_revision',assessment_revision,'criteria_revision',criteria_revision,'terminal_phase',terminal_phase,'material_pass_count',material_pass_count,'material_fail_count',material_fail_count,'material_unknown_count',material_unknown_count,'useful_outcome',useful_outcome,'exact_unmet_portion',exact_unmet_portion,'no_remaining_path',no_remaining_path,'assessment_json',assessment_json,'assessed_at',assessed_at)) FROM (SELECT * FROM execass_completion_assessments WHERE delegation_id=d.delegation_id ORDER BY assessment_id)),'[]'))
        ) FROM execass_delegations d WHERE d.delegation_id=?1"#,
        params![delegation_id, transition_continuation_id], |row| row.get(0),
    ).map_err(Into::into)
}

fn upsert_action_branch(conn: &Connection, record: &ActionBranchRecord) -> Result<()> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM execass_action_branches WHERE action_id=?1",
            params![record.action_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return insert_action_branch(conn, record);
    }
    conn.execute(
        "UPDATE execass_action_branches SET status=?1, updated_at=?2, terminal_at=?3, stop_epoch=?4 WHERE action_id=?5 AND delegation_id=?6 AND branch_kind=?7",
        params![record.status.as_str(), record.updated_at, record.terminal_at, record.stop_epoch, record.action_id, record.delegation_id, record.branch_kind.as_str()],
    )?;
    Ok(())
}

fn upsert_attention(conn: &Connection, item: &AttentionItemRecord) -> Result<()> {
    let existing: Option<StoredAttentionIdentity> = conn
        .query_row(
            "SELECT delegation_id,action_id,kind,reason,recommendation,alternatives_json,required_assurance,decision_id,delegation_revision,created_at FROM execass_attention_items WHERE attention_id=?1",
            params![item.attention_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)),
        )
        .optional()?;
    if existing.as_ref().is_some_and(|stored| {
        stored.0 != item.delegation_id
            || stored.1 != item.action_id
            || stored.2 != item.kind.as_str()
            || stored.3 != item.reason
            || stored.4 != item.recommendation
            || stored.5 != item.alternatives_json
            || stored.6 != item.required_assurance
            || stored.7 != item.decision_id
            || stored.8 != item.delegation_revision
            || stored.9 != item.created_at
    }) {
        bail!("attention item identity conflicts with an immutable prior record");
    }
    conn.execute(
        r#"INSERT INTO execass_attention_items (attention_id,delegation_id,action_id,kind,status,reason,recommendation,alternatives_json,required_assurance,decision_id,delegation_revision,created_at,resolved_at)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
            ON CONFLICT(attention_id) DO UPDATE SET status=excluded.status,resolved_at=excluded.resolved_at"#,
        params![item.attention_id,item.delegation_id,item.action_id,item.kind.as_str(),item.status.as_str(),item.reason,item.recommendation,item.alternatives_json,item.required_assurance,item.decision_id,item.delegation_revision,item.created_at,item.resolved_at],
    )?;
    Ok(())
}

pub(super) fn insert_runtime_paused_attention_in_tx(
    conn: &Connection,
    item: &RuntimePausedAttentionRecord,
) -> Result<()> {
    require_text("runtime attention_id", &item.attention_id)?;
    require_text(
        "runtime attention host_instance_id",
        &item.runtime_host_instance_id,
    )?;
    require_text("runtime attention reason", &item.reason)?;
    require_text("runtime attention recommendation", &item.recommendation)?;
    require_text(
        "runtime attention required_assurance",
        &item.required_assurance,
    )?;
    require_text("runtime attention end_reason", &item.runtime_end_reason)?;
    require_text("runtime attention outbox_event_id", &item.outbox_event_id)?;
    require_text("runtime attention receipt_id", &item.receipt_id)?;
    if item.status != AttentionStatus::Actionable
        || item.resolved_at.is_some()
        || item.runtime_host_generation <= 0
        || item.runtime_fencing_token <= 0
        || item.created_at <= 0
        || !matches!(
            item.runtime_actual_state,
            RuntimeActualState::Starting
                | RuntimeActualState::RunningAppBound
                | RuntimeActualState::Handoff
                | RuntimeActualState::RunningBackground
                | RuntimeActualState::Draining
                | RuntimeActualState::Faulted
        )
        || serde_json::from_str::<serde_json::Value>(&item.alternatives_json)?
            .as_array()
            .is_none()
        || item.active_work_binding_digest.len() != 71
        || !item.active_work_binding_digest.starts_with("sha256:")
        || !item.active_work_binding_digest[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("runtime-paused attention is not a closed canonical item");
    }
    conn.execute(
        r#"INSERT INTO execass_attention_items(
          attention_id,scope_kind,delegation_id,action_id,kind,status,reason,recommendation,
          alternatives_json,required_assurance,decision_id,delegation_revision,
          runtime_host_generation,runtime_host_instance_id,runtime_fencing_token,
          runtime_actual_state,runtime_end_reason,active_work_binding_digest,
          outbox_event_id,receipt_id,created_at,resolved_at
        ) VALUES(?1,'runtime_host',NULL,NULL,'runtime_paused','actionable',?2,?3,?4,?5,
          NULL,NULL,?6,?7,?8,?9,?10,?11,?12,?13,?14,NULL)"#,
        params![
            item.attention_id,
            item.reason,
            item.recommendation,
            item.alternatives_json,
            item.required_assurance,
            item.runtime_host_generation,
            item.runtime_host_instance_id,
            item.runtime_fencing_token,
            item.runtime_actual_state.as_str(),
            item.runtime_end_reason,
            item.active_work_binding_digest,
            item.outbox_event_id,
            item.receipt_id,
            item.created_at,
        ],
    )
    .context("failed inserting canonical runtime-paused attention")?;
    Ok(())
}

fn upsert_external_wait(conn: &Connection, item: &ExternalWaitRecord) -> Result<()> {
    let existing: Option<StoredExternalWaitIdentity> = conn
        .query_row(
            "SELECT delegation_id,action_id,kind,reason,details_json,delegation_revision,created_at FROM execass_external_waits WHERE external_wait_id=?1",
            params![item.external_wait_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
        )
        .optional()?;
    if existing.as_ref().is_some_and(|stored| {
        stored.0 != item.delegation_id
            || stored.1 != item.action_id
            || stored.2 != item.kind.as_str()
            || stored.3 != item.reason
            || stored.4 != item.details_json
            || stored.5 != item.delegation_revision
            || stored.6 != item.created_at
    }) {
        bail!("external wait identity conflicts with an immutable prior record");
    }
    conn.execute(
        r#"INSERT INTO execass_external_waits (external_wait_id,delegation_id,action_id,kind,status,reason,details_json,delegation_revision,created_at,resolved_at)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
            ON CONFLICT(external_wait_id) DO UPDATE SET status=excluded.status,resolved_at=excluded.resolved_at"#,
        params![item.external_wait_id,item.delegation_id,item.action_id,item.kind.as_str(),item.status.as_str(),item.reason,item.details_json,item.delegation_revision,item.created_at,item.resolved_at],
    )?;
    Ok(())
}

fn insert_assessment(conn: &Connection, item: &CompletionAssessmentRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO execass_completion_assessments (assessment_id,delegation_id,assessment_revision,criteria_revision,terminal_phase,material_pass_count,material_fail_count,material_unknown_count,useful_outcome,exact_unmet_portion,no_remaining_path,assessment_json,assessed_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![item.assessment_id,item.delegation_id,item.assessment_revision,item.criteria_revision,item.kind.phase().as_str(),item.material_pass_count,item.material_fail_count,item.material_unknown_count,item.useful_outcome as i64,item.exact_unmet_portion,item.no_remaining_path as i64,item.assessment_json,item.assessed_at],
    )?;
    Ok(())
}

fn first_actionable_decision(conn: &Connection, delegation_id: &str) -> Result<Option<String>> {
    conn.query_row("SELECT decision_id FROM execass_attention_items WHERE delegation_id=?1 AND status='actionable' AND decision_id IS NOT NULL ORDER BY created_at,attention_id LIMIT 1", params![delegation_id], |row| row.get(0)).optional().map_err(Into::into)
}
fn first_wait_details(conn: &Connection, delegation_id: &str) -> Result<Option<String>> {
    conn.query_row("SELECT details_json FROM execass_external_waits WHERE delegation_id=?1 AND status='waiting' ORDER BY created_at,external_wait_id LIMIT 1", params![delegation_id], |row| row.get(0)).optional().map_err(Into::into)
}

fn map_transition(row: &rusqlite::Row<'_>) -> rusqlite::Result<LifecycleTransitionRecord> {
    Ok(LifecycleTransitionRecord {
        transition_id: row.get(0)?,
        delegation_id: row.get(1)?,
        state_revision: row.get(2)?,
        previous_phase: row.get(3)?,
        selected_phase: row.get(4)?,
        previous_run_control: row.get(5)?,
        selected_run_control: row.get(6)?,
        selector_input_json: row.get(7)?,
        reason: row.get(8)?,
        outbox_event_id: row.get(9)?,
        occurred_at: row.get(10)?,
    })
}
const TRANSITION_SELECT: &str = "SELECT transition_id,delegation_id,state_revision,previous_phase,selected_phase,previous_run_control,selected_run_control,selector_input_json,reason,outbox_event_id,occurred_at FROM execass_lifecycle_transitions";
fn read_snapshot(
    conn: &Connection,
    delegation_id: &str,
    transition_id: &str,
) -> Result<Option<LifecycleSnapshot>> {
    let Some(mut delegation) = get_delegation(conn, delegation_id)? else {
        return Ok(None);
    };
    let transition = conn
        .query_row(
            &format!("{TRANSITION_SELECT} WHERE transition_id=?1 AND delegation_id=?2"),
            params![transition_id, delegation_id],
            map_transition,
        )
        .optional()?;
    let Some(transition) = transition else {
        return Ok(None);
    };
    let historical: String = conn.query_row(
        "SELECT projection_snapshot_json FROM execass_lifecycle_transitions WHERE transition_id=?1",
        params![transition_id],
        |row| row.get(0),
    )?;
    let snapshot: serde_json::Value =
        serde_json::from_str(&historical).context("stored lifecycle snapshot is invalid")?;
    delegation.phase = match snapshot["phase"]
        .as_str()
        .context("snapshot phase missing")?
    {
        "accepted" => DelegationPhase::Accepted,
        "planning" => DelegationPhase::Planning,
        "in_motion" => DelegationPhase::InMotion,
        "waiting_for_user" => DelegationPhase::WaitingForUser,
        "waiting_external" => DelegationPhase::WaitingExternal,
        "recovering" => DelegationPhase::Recovering,
        "completed" => DelegationPhase::Completed,
        "partially_completed" => DelegationPhase::PartiallyCompleted,
        "failed" => DelegationPhase::Failed,
        _ => bail!("invalid snapshot phase"),
    };
    delegation.run_control = match snapshot["run_control"]
        .as_str()
        .context("snapshot run control missing")?
    {
        "running" => RunControlState::Running,
        "stop_requested" => RunControlState::StopRequested,
        "stopped" => RunControlState::Stopped,
        _ => bail!("invalid snapshot run control"),
    };
    delegation.state_revision = snapshot["state_revision"]
        .as_i64()
        .context("snapshot revision missing")?;
    delegation.current_plan_revision = snapshot["current_plan_revision"].as_i64();
    delegation.current_criteria_revision = snapshot["current_criteria_revision"].as_i64();
    delegation.pending_decision_id = snapshot["pending_decision_id"].as_str().map(str::to_owned);
    delegation.external_wait_json = snapshot["external_wait_json"].as_str().map(str::to_owned);
    delegation.completion_assessment_json = snapshot["completion_assessment_json"]
        .as_str()
        .map(str::to_owned);
    delegation.stop_epoch = snapshot["stop_epoch"]
        .as_i64()
        .context("snapshot stop epoch missing")?;
    delegation.updated_at = snapshot["updated_at"]
        .as_i64()
        .context("snapshot updated_at missing")?;
    delegation.terminal_at = snapshot["terminal_at"].as_i64();
    let outbox_event = get_outbox(conn, &transition.outbox_event_id)?
        .context("lifecycle transition outbox missing")?;
    let continuation_id = snapshot["transition_continuation_id"].as_str();
    let continuation = continuation_id
        .map(|continuation_id| {
            snapshot["continuations"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item["continuation_id"].as_str() == Some(continuation_id))
                })
                .context("snapshot transition continuation is missing")
                .and_then(|item| continuation_from_snapshot(item, delegation_id))
        })
        .transpose()?;
    Ok(Some(LifecycleSnapshot {
        delegation,
        transition,
        continuation,
        outbox_event,
    }))
}

fn continuation_from_snapshot(
    value: &serde_json::Value,
    delegation_id: &str,
) -> Result<ContinuationRecord> {
    let required = |name: &str| {
        value[name]
            .as_str()
            .map(str::to_owned)
            .with_context(|| format!("snapshot continuation {name} missing"))
    };
    let status = match required("status")?.as_str() {
        "runnable" => ContinuationStatus::Runnable,
        "executing" => ContinuationStatus::Executing,
        "waiting" => ContinuationStatus::Waiting,
        "uncertain" => ContinuationStatus::Uncertain,
        "terminal" => ContinuationStatus::Terminal,
        "superseded" => ContinuationStatus::Superseded,
        _ => bail!("invalid snapshot continuation status"),
    };
    let branch_kind = match required("branch_kind")?.as_str() {
        "ordinary" => ActionBranchKind::Ordinary,
        "recovery" => ActionBranchKind::Recovery,
        _ => bail!("invalid snapshot branch kind"),
    };
    let causation_kind = match required("causation_kind")?.as_str() {
        "intake" => ContinuationCausationKind::Intake,
        "plan" => ContinuationCausationKind::Plan,
        "amendment" => ContinuationCausationKind::Amendment,
        "decision" => ContinuationCausationKind::Decision,
        "action_result" => ContinuationCausationKind::ActionResult,
        "recovery" => ContinuationCausationKind::Recovery,
        "resume" => ContinuationCausationKind::Resume,
        "routine_occurrence" => ContinuationCausationKind::RoutineOccurrence,
        _ => bail!("invalid snapshot causation kind"),
    };
    Ok(ContinuationRecord {
        continuation_id: required("continuation_id")?,
        delegation_id: delegation_id.into(),
        target_delegation_revision: value["target_delegation_revision"]
            .as_i64()
            .context("snapshot continuation revision missing")?,
        target_plan_revision: value["target_plan_revision"]
            .as_i64()
            .context("snapshot continuation plan missing")?,
        action_id: required("action_id")?,
        branch_kind,
        causation_kind,
        causation_id: required("causation_id")?,
        status,
        job_id: value["job_id"].as_str().map(str::to_owned),
        lease_owner: value["lease_owner"].as_str().map(str::to_owned),
        lease_expires_at: value["lease_expires_at"].as_i64(),
        fencing_token: value["fencing_token"]
            .as_i64()
            .context("snapshot continuation fence missing")?,
        host_generation: value["host_generation"]
            .as_i64()
            .context("snapshot continuation host missing")?,
        stop_epoch: value["stop_epoch"]
            .as_i64()
            .context("snapshot continuation stop missing")?,
        global_stop_epoch: value["global_stop_epoch"]
            .as_i64()
            .context("snapshot continuation global stop missing")?,
        created_at: value["created_at"]
            .as_i64()
            .context("snapshot continuation created missing")?,
        updated_at: value["updated_at"]
            .as_i64()
            .context("snapshot continuation updated missing")?,
        completed_at: value["completed_at"].as_i64(),
    })
}
fn latest_transition(
    conn: &Connection,
    delegation_id: &str,
) -> Result<Option<LifecycleTransitionRecord>> {
    conn.query_row(
        &format!("{TRANSITION_SELECT} WHERE delegation_id=?1 ORDER BY state_revision DESC LIMIT 1"),
        params![delegation_id],
        map_transition,
    )
    .optional()
    .map_err(Into::into)
}
fn replayed_snapshot(
    conn: &Connection,
    command: &LifecycleSnapshotCommand,
) -> Result<Option<LifecycleSnapshot>> {
    let transition = conn
        .query_row(
            &format!("{TRANSITION_SELECT} WHERE transition_id=?1"),
            params![command.transition_id],
            map_transition,
        )
        .optional()?;
    let Some(transition) = transition else {
        return Ok(None);
    };
    let outbox = get_outbox(conn, &transition.outbox_event_id)?
        .context("stored lifecycle replay is missing its outbox event")?;
    let stored_identity: String = conn.query_row(
        "SELECT command_identity FROM execass_lifecycle_transitions WHERE transition_id=?1",
        params![command.transition_id],
        |row| row.get(0),
    )?;
    if transition.delegation_id != command.delegation_id
        || stored_identity != lifecycle_command_identity(command)
        || outbox.event != command.outbox_event
    {
        bail!("lifecycle transition identity conflicts with an immutable prior write");
    }
    read_snapshot(conn, &command.delegation_id, &command.transition_id)
}
