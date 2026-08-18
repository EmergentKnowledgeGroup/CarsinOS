//! EA-215 typed, per-delegation run-control storage boundary.
//!
//! These operations change only the run-control projection. They preserve the
//! lifecycle phase, never manufacture a continuation, and keep every claim
//! fenced by the delegation stop epoch.

use super::global_stop::{
    authority_from_run_control, derived_human_run_control_receipt,
    insert_verified_run_control_attestation, load_active_run_control_key, load_run_control_replay,
    validate_exact_run_control_replay,
};
use super::lifecycle::projection_snapshot_json;
use super::receipt::{
    receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{get_authority, get_delegation, get_outbox, insert_authority, insert_outbox};
use super::run_control_attestation::{
    verify_run_control_attestation, VerifiedRunControlAttestation,
};
use super::store::ExecAssStore;
use super::types::*;
use super::validation::{require_text, validate_outbox};
use anyhow::{bail, Context, Result};
use carsinos_protocol::execass::{
    ActorType as ProtocolActorType, RunControlAttestation, RunControlOperation, RunControlTarget,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};

impl ExecAssStore {
    /// Reads the complete, current stop/resume disclosure for one delegation.
    /// Missing live runtime ownership is represented explicitly rather than
    /// making the authoritative delegation state disappear.
    pub fn read_delegation_run_control_status(
        &self,
        delegation_id: &str,
        trusted_now: i64,
    ) -> Result<Option<DelegationRunControlStatus>> {
        require_text("delegation_id", delegation_id)?;
        if trusted_now <= 0 {
            bail!("delegation run-control status requires a positive trusted clock");
        }
        read_status(&self.connection()?, delegation_id, trusted_now)
    }

    /// Returns stop-requested delegations whose in-flight execution has
    /// reached a safe boundary. The caller may complete these drains without
    /// inventing work or relying on a second human command.
    pub fn list_ready_delegation_stops(
        &self,
        trusted_now: i64,
        limit: usize,
    ) -> Result<Vec<DelegationRunControlStatus>> {
        if trusted_now <= 0 {
            bail!("ready delegation stop scan requires a positive trusted clock");
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit =
            i64::try_from(limit.min(1_000)).context("delegation stop scan limit overflow")?;
        let conn = self.connection()?;
        let delegation_ids = {
            let mut statement = conn.prepare(
                "SELECT delegation_id FROM execass_delegations WHERE run_control='stop_requested' ORDER BY updated_at,delegation_id LIMIT ?1",
            )?;
            let rows = statement
                .query_map([limit], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        let mut ready = Vec::with_capacity(delegation_ids.len());
        for delegation_id in delegation_ids {
            let Some(status) = read_status(&conn, &delegation_id, trusted_now)? else {
                continue;
            };
            if status.drain_state == DelegationStopDrainState::ReadyToStop {
                ready.push(status);
            }
        }
        Ok(ready)
    }

    /// Atomically moves a running delegation to `stop_requested`, increments
    /// its stop epoch, and therefore blocks all new claims at commit. Drain
    /// completion is a separate checked transition so `stopped` can never be
    /// exposed while an execution branch remains.
    pub fn request_delegation_stop_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &RequestDelegationStopCommand,
    ) -> Result<DelegationRunControlMutationOutcome> {
        validate_stop_command(command)?;
        if self
            .read_delegation_run_control_status(&command.delegation_id, command.trusted_now)?
            .is_none()
        {
            return Ok(DelegationRunControlMutationOutcome::NotFound);
        }
        let receipt = derived_human_run_control_receipt(&command.attestation, &command.receipt)?;
        validate_receipt_event_binding(
            &command.outbox_event,
            &receipt,
            &command.delegation_id,
            command.trusted_now,
        )?;
        let result = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            command.expected_state_revision,
            &receipt,
            |tx| {
                let pinned = load_active_run_control_key(tx, &self.root_identity)?;
                if let Some(replay) = load_run_control_replay(
                    tx,
                    &command.attestation.payload.replay_identity,
                )? {
                    validate_exact_run_control_replay(
                        tx,
                        &command.attestation,
                        &receipt,
                        &command.outbox_event,
                        &pinned,
                        &replay,
                    )?;
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Replayed));
                }
                if replayed_event(tx, &command.outbox_event)?.is_some() {
                    bail!("delegation stop outbox identity lacks its signed attestation replay");
                }
                let before = read_status(tx, &command.delegation_id, command.trusted_now)?
                    .context("delegation disappeared during signed stop")?;
                let current = get_delegation(tx, &command.delegation_id)?
                    .context("delegation disappeared during stop request")?;
                if before.state_revision != command.expected_state_revision
                    || before.stop_epoch != command.expected_stop_epoch
                    || before.policy_revision != command.expected_policy_revision
                    || before.current_plan_revision != command.expected_plan_revision
                    || before.unresolved_external_effects_digest
                        != command.disclosed_unresolved_external_effects_digest
                {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Stale));
                }
                if current.run_control != RunControlState::Running {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::AlreadyStopped));
                }
                if current.phase.is_terminal() {
                    bail!("terminal delegation cannot enter run-control stop");
                }
                let verified = verify_run_control_attestation(
                    &command.attestation,
                    &pinned,
                    command.trusted_now,
                )?;
                require_exact_delegation_stop_binding(self, tx, command, &before, &verified)?;
                let authority = authority_from_run_control(&verified)?;
                if receipt.actor.authority_provenance_id != authority.authority_provenance_id
                    || receipt.actor.actor_type != authority.actor_type
                    || receipt.actor.actor_identity.as_str() != authority.credential_identity
                {
                    bail!("delegation stop receipt actor is not derived from signed authority");
                }
                insert_authority(tx, &authority)?;
                insert_verified_run_control_attestation(
                    tx,
                    &command.attestation,
                    command.trusted_now,
                    &receipt,
                    &command.outbox_event,
                    &verified,
                    &authority,
                )?;
                let next_epoch = current
                    .stop_epoch
                    .checked_add(1)
                    .context("delegation stop epoch overflow")?;
                let next_revision = current
                    .state_revision
                    .checked_add(1)
                    .context("delegation state revision overflow")?;
                let changed = tx.execute(
                    "UPDATE execass_delegations SET run_control='stop_requested',state_revision=?1,stop_epoch=?2,updated_at=?3 WHERE delegation_id=?4 AND state_revision=?5 AND run_control='running' AND stop_epoch=?6",
                    params![next_revision,next_epoch,command.trusted_now,current.delegation_id,current.state_revision,current.stop_epoch],
                )?;
                if changed != 1 {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Stale));
                }
                validate_result_event(
                    tx,
                    &command.outbox_event,
                    &receipt,
                    &command.delegation_id,
                    next_revision,
                    "stop_requested",
                    command.trusted_now,
                )?;
                insert_outbox(tx, &command.outbox_event)?;
                insert_transition(
                    tx,
                    &current,
                    RunControlState::StopRequested,
                    next_revision,
                    "per-delegation stop requested",
                    &command.outbox_event,
                    format!("{command:#?}"),
                )?;
                Ok(AtomicReceiptMutation::Append(MutationKind::StopRequested))
            },
        )?;
        self.map_mutation(&command.delegation_id, command.trusted_now, result)
    }

    /// Completes the safe-boundary drain only when neither an executing
    /// continuation nor an executing action branch exists for the delegation.
    pub fn complete_delegation_stop_drain_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &CompleteDelegationStopDrainCommand,
    ) -> Result<DelegationRunControlMutationOutcome> {
        validate_drain_command(command)?;
        if self
            .read_delegation_run_control_status(&command.delegation_id, command.trusted_now)?
            .is_none()
        {
            return Ok(DelegationRunControlMutationOutcome::NotFound);
        }
        let result = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            command.expected_state_revision,
            &command.receipt,
            |tx| {
                if let Some(replayed) = replayed_event(tx, &command.outbox_event)? {
                    if replayed.event != command.outbox_event {
                        bail!("delegation drain replay collides with different outbox bytes");
                    }
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Replayed));
                }
                let current = get_delegation(tx, &command.delegation_id)?
                    .context("delegation disappeared during stop drain")?;
                if current.state_revision != command.expected_state_revision
                    || current.stop_epoch != command.expected_stop_epoch
                {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Stale));
                }
                if current.run_control == RunControlState::Stopped {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::AlreadyStopped));
                }
                if current.run_control != RunControlState::StopRequested {
                    bail!("delegation drain completion requires stop_requested");
                }
                require_drain_actor(tx, &command.receipt)?;
                if executing_count(tx, &command.delegation_id)? != 0 {
                    bail!("delegation cannot expose stopped while an execution branch remains");
                }
                let next_revision = current
                    .state_revision
                    .checked_add(1)
                    .context("delegation state revision overflow")?;
                let changed = tx.execute(
                    "UPDATE execass_delegations SET run_control='stopped',state_revision=?1,updated_at=?2 WHERE delegation_id=?3 AND state_revision=?4 AND run_control='stop_requested' AND stop_epoch=?5",
                    params![next_revision,command.trusted_now,current.delegation_id,current.state_revision,current.stop_epoch],
                )?;
                if changed != 1 {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Stale));
                }
                validate_result_event(
                    tx,
                    &command.outbox_event,
                    &command.receipt,
                    &command.delegation_id,
                    next_revision,
                    "stopped",
                    command.trusted_now,
                )?;
                insert_outbox(tx, &command.outbox_event)?;
                insert_transition(
                    tx,
                    &current,
                    RunControlState::Stopped,
                    next_revision,
                    "per-delegation stop drain completed",
                    &command.outbox_event,
                    format!("{command:#?}"),
                )?;
                Ok(AtomicReceiptMutation::Append(MutationKind::Drained))
            },
        )?;
        self.map_mutation(&command.delegation_id, command.trusted_now, result)
    }

    /// Atomically consumes one fixed-key signed human attestation for the
    /// exact stopped snapshot. This only re-opens the control fence; it does
    /// not claim that work resumed and does not synthesize continuation work.
    pub fn resume_delegation_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ResumeDelegationCommand,
    ) -> Result<DelegationRunControlMutationOutcome> {
        validate_resume_command(command)?;
        if self
            .read_delegation_run_control_status(&command.delegation_id, command.trusted_now)?
            .is_none()
        {
            return Ok(DelegationRunControlMutationOutcome::NotFound);
        }
        let receipt = derived_human_run_control_receipt(&command.attestation, &command.receipt)?;
        validate_receipt_event_binding(
            &command.outbox_event,
            &receipt,
            &command.delegation_id,
            command.trusted_now,
        )?;
        let result = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            command.expected_state_revision,
            &receipt,
            |tx| {
                let pinned = load_active_run_control_key(tx, &self.root_identity)?;
                if let Some(replay) = load_run_control_replay(
                    tx,
                    &command.attestation.payload.replay_identity,
                )? {
                    validate_exact_run_control_replay(
                        tx,
                        &command.attestation,
                        &receipt,
                        &command.outbox_event,
                        &pinned,
                        &replay,
                    )?;
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Replayed));
                }
                let before = read_status(tx, &command.delegation_id, command.trusted_now)?
                    .context("delegation disappeared during signed resume")?;
                let current = get_delegation(tx, &command.delegation_id)?
                    .context("delegation disappeared during signed resume")?;
                if before.run_control != RunControlState::Stopped
                    || before.state_revision != command.expected_state_revision
                    || before.stop_epoch != command.expected_stop_epoch
                    || before.policy_revision != command.expected_policy_revision
                    || before.current_plan_revision != command.expected_plan_revision
                    || before.unresolved_external_effects_digest
                        != command.disclosed_unresolved_external_effects_digest
                {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Stale));
                }
                let verified = verify_run_control_attestation(
                    &command.attestation,
                    &pinned,
                    command.trusted_now,
                )?;
                require_exact_delegation_resume_binding(self, tx, command, &before, &verified)?;
                let authority = authority_from_run_control(&verified)?;
                if receipt.actor.authority_provenance_id != authority.authority_provenance_id
                    || receipt.actor.actor_type != authority.actor_type
                    || receipt.actor.actor_identity.as_str() != authority.credential_identity
                {
                    bail!("delegation resume receipt actor is not derived from signed authority");
                }
                insert_authority(tx, &authority)?;
                insert_verified_run_control_attestation(
                    tx,
                    &command.attestation,
                    command.trusted_now,
                    &receipt,
                    &command.outbox_event,
                    &verified,
                    &authority,
                )?;
                let next_epoch = before
                    .stop_epoch
                    .checked_add(1)
                    .context("delegation stop epoch overflow")?;
                let next_revision = before
                    .state_revision
                    .checked_add(1)
                    .context("delegation state revision overflow")?;
                let changed = tx.execute(
                    "UPDATE execass_delegations SET run_control='running',state_revision=?1,stop_epoch=?2,updated_at=?3 WHERE delegation_id=?4 AND state_revision=?5 AND run_control='stopped' AND stop_epoch=?6 AND policy_revision=?7",
                    params![next_revision,next_epoch,command.trusted_now,before.delegation_id,before.state_revision,before.stop_epoch,before.policy_revision],
                )?;
                if changed != 1 {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationKind::Stale));
                }
                validate_result_event(
                    tx,
                    &command.outbox_event,
                    &receipt,
                    &command.delegation_id,
                    next_revision,
                    "resumed",
                    command.trusted_now,
                )?;
                insert_outbox(tx, &command.outbox_event)?;
                insert_transition(
                    tx,
                    &current,
                    RunControlState::Running,
                    next_revision,
                    "signed human per-delegation resume",
                    &command.outbox_event,
                    format!("{command:#?}"),
                )?;
                Ok(AtomicReceiptMutation::Append(MutationKind::Resumed))
            },
        )?;
        self.map_mutation(&command.delegation_id, command.trusted_now, result)
    }

    /// Read-only restart replay for an already committed exact delegation
    /// resume. It never treats stale evidence as authority for a new write.
    pub fn replay_delegation_resume_attestation(
        &self,
        delegation_id: &str,
        attestation: &RunControlAttestation,
        trusted_now: i64,
    ) -> Result<Option<DelegationRunControlMutationOutcome>> {
        require_text("delegation_id", delegation_id)?;
        let conn = self.connection()?;
        let Some(replay) = load_run_control_replay(&conn, &attestation.payload.replay_identity)?
        else {
            return Ok(None);
        };
        let pinned = load_active_run_control_key(&conn, &self.root_identity)?;
        let verified = verify_run_control_attestation(attestation, &pinned, replay.verified_at)?;
        if verified.payload().operation != RunControlOperation::DelegationResume
            || verified.payload().target
                != (RunControlTarget::Delegation {
                    delegation_id: delegation_id.to_string(),
                })
        {
            bail!("persisted run-control replay is not this delegation resume");
        }
        let authority = authority_from_run_control(&verified)?;
        let receipt = receipt_by_causation_event(&conn, &replay.outbox_event_id)?;
        let outbox = get_outbox(&conn, &replay.outbox_event_id)?;
        if replay.attestation_digest != verified.attestation_digest()
            || replay.authority_provenance_id != authority.authority_provenance_id
            || replay.signed_payload_json != serde_json::to_string(verified.payload())?
            || replay.signature_hex != attestation.signature_hex
            || get_authority(&conn, &authority.authority_provenance_id)?.as_ref()
                != Some(&authority)
            || receipt.as_ref().map(|stored| stored.receipt_id.as_str())
                != Some(replay.receipt_id.as_str())
            || outbox.as_ref().map(|stored| stored.event.event_id.as_str())
                != Some(replay.outbox_event_id.as_str())
        {
            bail!("persisted delegation resume replay lost its receipt or outbox");
        }
        Ok(read_status(&conn, delegation_id, trusted_now)?
            .map(DelegationRunControlMutationOutcome::Replayed))
    }

    fn map_mutation(
        &self,
        delegation_id: &str,
        trusted_now: i64,
        result: AtomicReceiptWriteOutcome<MutationKind>,
    ) -> Result<DelegationRunControlMutationOutcome> {
        let kind = match result {
            AtomicReceiptWriteOutcome::Appended { value, .. }
            | AtomicReceiptWriteOutcome::NoAppend(value) => value,
            AtomicReceiptWriteOutcome::Stale { .. } => MutationKind::Stale,
        };
        let Some(status) = self.read_delegation_run_control_status(delegation_id, trusted_now)?
        else {
            return Ok(DelegationRunControlMutationOutcome::NotFound);
        };
        Ok(match kind {
            MutationKind::StopRequested => {
                DelegationRunControlMutationOutcome::StopRequested(status)
            }
            MutationKind::Drained => DelegationRunControlMutationOutcome::Drained(status),
            MutationKind::Resumed => DelegationRunControlMutationOutcome::Resumed(status),
            MutationKind::Replayed => DelegationRunControlMutationOutcome::Replayed(status),
            MutationKind::AlreadyStopped => {
                DelegationRunControlMutationOutcome::AlreadyStopped(status)
            }
            MutationKind::Stale => DelegationRunControlMutationOutcome::Stale(status),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum MutationKind {
    StopRequested,
    Drained,
    Resumed,
    Replayed,
    AlreadyStopped,
    Stale,
}

fn read_status(
    conn: &Connection,
    delegation_id: &str,
    trusted_now: i64,
) -> Result<Option<DelegationRunControlStatus>> {
    let Some(delegation) = get_delegation(conn, delegation_id)? else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        r#"SELECT effect.logical_effect_id,effect.delegation_id,effect.continuation_id,effect.state,
          (SELECT attempt_id FROM execass_provider_attempts attempt
           WHERE attempt.logical_effect_id=effect.logical_effect_id
           ORDER BY attempt.attempt_number DESC LIMIT 1)
          FROM execass_logical_effects effect
          WHERE effect.delegation_id=?1
            AND effect.state IN ('claimed','invoking','outcome_unknown')
            AND effect.action_kind IN (
              'public_or_externally_consequential_communication',
              'irreversible_or_destructive_action',
              'credential_permission_privilege_or_trust_policy_change',
              'project_defining_scope_ownership_or_launch_decision',
              'secret_use_through_authorized_connector',
              'unknown_composite_aliased_plugin_shell_or_changed_version_action'
            )
          ORDER BY effect.logical_effect_id"#,
    )?;
    let unresolved_external_effects = statement
        .query_map([delegation_id], |row| {
            Ok(UnresolvedExternalEffectReference {
                logical_effect_id: row.get(0)?,
                delegation_id: row.get(1)?,
                continuation_id: row.get(2)?,
                state: row.get(3)?,
                latest_attempt_id: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (global_receipt_count, global_receipt_head_digest, receipt_anchor_status): (
        i64,
        Option<String>,
        String,
    ) = conn.query_row(
        r#"SELECT journal.receipt_count,journal.receipt_head_digest,
          COALESCE((SELECT status FROM execass_receipt_anchor_state WHERE singleton=1),'uninitialized')
          FROM execass_receipt_journal_state journal WHERE journal.singleton=1"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let runtime = conn
        .query_row(
            r#"SELECT generation.state_root_generation,lease.generation,lease.host_instance_id,lease.fencing_token
              FROM execass_runtime_host_leases lease
              JOIN execass_runtime_host_generations generation
                ON generation.generation=lease.generation
               AND generation.host_instance_id=lease.host_instance_id
              WHERE lease.ownership_scope='execass' AND lease.released_at IS NULL
                AND lease.expires_at>?1
              ORDER BY lease.generation DESC,lease.fencing_token DESC LIMIT 1"#,
            [trusted_now],
            |row| {
                Ok(DelegationRunControlRuntimeContext {
                    state_root_generation: row.get(0)?,
                    runtime_host_generation: row.get(1)?,
                    runtime_host_instance_id: row.get(2)?,
                    runtime_fencing_token: row.get(3)?,
                })
            },
        )
        .optional()?;
    let executing = executing_count(conn, delegation_id)?;
    let drain_state = match delegation.run_control {
        RunControlState::Running => DelegationStopDrainState::Running,
        RunControlState::StopRequested if executing > 0 => DelegationStopDrainState::Draining,
        RunControlState::StopRequested => DelegationStopDrainState::ReadyToStop,
        RunControlState::Stopped => DelegationStopDrainState::Stopped,
    };
    Ok(Some(DelegationRunControlStatus {
        delegation_id: delegation.delegation_id,
        phase: delegation.phase,
        run_control: delegation.run_control,
        state_revision: delegation.state_revision,
        current_plan_revision: delegation.current_plan_revision,
        stop_epoch: delegation.stop_epoch,
        policy_revision: delegation.policy_revision,
        drain_state,
        executing_branch_count: executing,
        unresolved_external_effects_digest: unresolved_digest(&unresolved_external_effects),
        unresolved_external_effects,
        global_receipt_count,
        global_receipt_head_digest,
        delegation_receipt_count: delegation.receipt_chain_count,
        delegation_receipt_head_digest: delegation.receipt_chain_head_digest,
        receipt_anchor_status,
        runtime,
    }))
}

fn executing_count(conn: &Connection, delegation_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM execass_continuations WHERE delegation_id=?1 AND status='executing') + (SELECT COUNT(*) FROM execass_action_branches WHERE delegation_id=?1 AND status='executing')",
        [delegation_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn unresolved_digest(effects: &[UnresolvedExternalEffectReference]) -> String {
    let canonical = effects
        .iter()
        .map(|effect| {
            json!({
                "logical_effect_id": effect.logical_effect_id,
                "delegation_id": effect.delegation_id,
                "continuation_id": effect.continuation_id,
                "state": effect.state.as_str(),
                "latest_attempt_id": effect.latest_attempt_id,
            })
        })
        .collect::<Vec<_>>();
    let bytes =
        serde_json::to_vec(&canonical).expect("unresolved effect references are serializable");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn validate_stop_command(command: &RequestDelegationStopCommand) -> Result<()> {
    validate_common(
        &command.delegation_id,
        command.expected_state_revision,
        command.expected_stop_epoch,
        command.trusted_now,
        &command.outbox_event,
        &command.receipt,
    )?;
    if command.expected_policy_revision <= 0
        || command
            .expected_plan_revision
            .is_none_or(|revision| revision <= 0)
        || !valid_effect_digest(&command.disclosed_unresolved_external_effects_digest)
    {
        bail!("delegation stop proof is incomplete");
    }
    super::run_control_attestation::run_control_attestation_digest(&command.attestation)?;
    Ok(())
}

fn validate_drain_command(command: &CompleteDelegationStopDrainCommand) -> Result<()> {
    validate_common(
        &command.delegation_id,
        command.expected_state_revision,
        command.expected_stop_epoch,
        command.trusted_now,
        &command.outbox_event,
        &command.receipt,
    )
}

fn validate_resume_command(command: &ResumeDelegationCommand) -> Result<()> {
    validate_common(
        &command.delegation_id,
        command.expected_state_revision,
        command.expected_stop_epoch,
        command.trusted_now,
        &command.outbox_event,
        &command.receipt,
    )?;
    if command.expected_stop_epoch <= 0
        || command.expected_policy_revision <= 0
        || command
            .expected_plan_revision
            .is_none_or(|revision| revision <= 0)
        || !valid_effect_digest(&command.disclosed_unresolved_external_effects_digest)
    {
        bail!("delegation resume proof is incomplete");
    }
    super::run_control_attestation::run_control_attestation_digest(&command.attestation)?;
    Ok(())
}

fn validate_common(
    delegation_id: &str,
    expected_state_revision: i64,
    expected_stop_epoch: i64,
    trusted_now: i64,
    event: &NewOutboxEvent,
    receipt: &AppendReceiptCommand,
) -> Result<()> {
    require_text("delegation_id", delegation_id)?;
    if expected_state_revision <= 0 || expected_stop_epoch < 0 || trusted_now <= 0 {
        bail!("delegation run-control command has an invalid revision, epoch, or clock");
    }
    validate_receipt_event_binding(event, receipt, delegation_id, trusted_now)
}

fn validate_receipt_event_binding(
    event: &NewOutboxEvent,
    receipt: &AppendReceiptCommand,
    delegation_id: &str,
    trusted_now: i64,
) -> Result<()> {
    validate_outbox(event)?;
    if event.event_name != OutboxEventName::DelegationTransitioned
        || event.aggregate_id != delegation_id
        || event.event_id != receipt.causation_event_id
        || event.aggregate_revision != receipt.expected_state_revision
        || event.aggregate_revision != receipt.subject.revision
        || event.causation_id != receipt.causation_id
        || event.occurred_at != trusted_now
        || receipt.occurred_at != trusted_now
        || receipt.committed_at != trusted_now
        || receipt.delegation_id != delegation_id
        || receipt.receipt_kind != ReceiptKind::RunControl
        || receipt.subject.kind != ReceiptSubjectKind::Delegation
        || receipt.subject.subject_id != delegation_id
    {
        bail!("delegation run-control event and receipt are not exact bindings");
    }
    Ok(())
}

fn validate_result_event(
    conn: &Connection,
    event: &NewOutboxEvent,
    receipt: &AppendReceiptCommand,
    delegation_id: &str,
    revision: i64,
    operation: &str,
    trusted_now: i64,
) -> Result<()> {
    validate_receipt_event_binding(event, receipt, delegation_id, trusted_now)?;
    if event.aggregate_revision != revision {
        bail!("delegation run-control event is not the resulting state revision");
    }
    let status = read_status(conn, delegation_id, trusted_now)?
        .context("delegation disappeared before run-control event")?;
    let expected = json!({
        "operation": operation,
        "delegation_id": status.delegation_id,
        "phase": status.phase.as_str(),
        "run_control": status.run_control.as_str(),
        "state_revision": status.state_revision,
        "current_plan_revision": status.current_plan_revision,
        "stop_epoch": status.stop_epoch,
        "policy_revision": status.policy_revision,
        "drain_state": status.drain_state.as_str(),
        "unresolved_external_effects_digest": status.unresolved_external_effects_digest,
    })
    .to_string();
    if event.safe_payload_json != expected {
        bail!("delegation run-control outbox payload is not deterministic live status");
    }
    Ok(())
}

fn insert_transition(
    tx: &Transaction<'_>,
    previous: &DelegationRecord,
    selected: RunControlState,
    next_revision: i64,
    reason: &str,
    event: &NewOutboxEvent,
    command_identity: String,
) -> Result<()> {
    let snapshot = projection_snapshot_json(tx, &previous.delegation_id, None)?;
    tx.execute(
        r#"INSERT INTO execass_lifecycle_transitions(
          transition_id,delegation_id,state_revision,previous_phase,selected_phase,
          previous_run_control,selected_run_control,selector_input_json,command_identity,
          projection_snapshot_json,reason,outbox_event_id,occurred_at
        ) VALUES(?1,?2,?3,?4,?4,?5,?6,'{}',?7,?8,?9,?10,?11)"#,
        params![
            format!("run-control:{}", event.event_id),
            previous.delegation_id,
            next_revision,
            previous.phase.as_str(),
            previous.run_control.as_str(),
            selected.as_str(),
            command_identity,
            snapshot,
            reason,
            event.event_id,
            event.occurred_at,
        ],
    )?;
    Ok(())
}

fn require_drain_actor(tx: &Transaction<'_>, receipt: &AppendReceiptCommand) -> Result<()> {
    let authority = get_authority(tx, &receipt.actor.authority_provenance_id)?
        .context("delegation drain actor authority is absent")?;
    if authority.actor_type != ActorType::Runtime
        || authority.authority_kind != AuthorityKind::RuntimeSafetyState
        || receipt.actor.actor_type != ActorType::Runtime
        || authority.credential_identity != receipt.actor.actor_identity.as_str()
    {
        bail!("delegation drain completion requires trusted runtime safety authority");
    }
    Ok(())
}

fn require_exact_delegation_stop_binding(
    store: &ExecAssStore,
    tx: &Transaction<'_>,
    command: &RequestDelegationStopCommand,
    status: &DelegationRunControlStatus,
    verified: &VerifiedRunControlAttestation,
) -> Result<()> {
    let payload = verified.payload();
    let actor_type = match payload.actor_type {
        ProtocolActorType::HumanLocal => ActorType::HumanLocal,
        ProtocolActorType::HumanRemote => ActorType::HumanRemote,
        _ => bail!("delegation stop requires a verified human actor"),
    };
    let active: Option<i64> = tx
        .query_row(
            "SELECT provider_event_required FROM execass_owner_ingress_bindings WHERE actor_type=?1 AND credential_identity=?2 AND authenticated_ingress=?3 AND channel_assurance=?4 AND status='active'",
            params![actor_type.as_str(),payload.credential_identity,payload.authenticated_ingress,payload.channel_assurance],
            |row| row.get(0),
        )
        .optional()?;
    if active != Some(i64::from(actor_type == ActorType::HumanRemote)) {
        bail!("delegation stop authority does not match an active owner ingress");
    }
    if payload.operation != RunControlOperation::DelegationStop
        || payload.target
            != (RunControlTarget::Delegation {
                delegation_id: command.delegation_id.clone(),
            })
        || payload.stopped_epoch != status.stop_epoch
        || payload.stopped_epoch != command.expected_stop_epoch
        || payload.policy_revision != status.policy_revision
        || payload.policy_revision != command.expected_policy_revision
        || payload.unresolved_effect_disclosure_digest != status.unresolved_external_effects_digest
        || payload.unresolved_effect_disclosure_digest
            != command.disclosed_unresolved_external_effects_digest
        || payload.delegation_state_revision != Some(status.state_revision)
        || payload.delegation_state_revision != Some(command.expected_state_revision)
        || payload.current_plan_revision != status.current_plan_revision
        || payload.current_plan_revision != command.expected_plan_revision
        || payload.idempotency_key != command.outbox_event.duplicate_identity
        || payload.request_correlation_id != command.outbox_event.correlation_id
        || payload.issued_at_ms != command.receipt.occurred_at
        || payload.canonical_root_identity != store.root_identity
    {
        bail!("run-control attestation is not bound to the exact running delegation state");
    }
    Ok(())
}

fn require_exact_delegation_resume_binding(
    store: &ExecAssStore,
    tx: &Transaction<'_>,
    command: &ResumeDelegationCommand,
    status: &DelegationRunControlStatus,
    verified: &VerifiedRunControlAttestation,
) -> Result<()> {
    let payload = verified.payload();
    let actor_type = match payload.actor_type {
        ProtocolActorType::HumanLocal => ActorType::HumanLocal,
        ProtocolActorType::HumanRemote => ActorType::HumanRemote,
        _ => bail!("delegation resume requires a verified human actor"),
    };
    let active: Option<i64> = tx
        .query_row(
            "SELECT provider_event_required FROM execass_owner_ingress_bindings WHERE actor_type=?1 AND credential_identity=?2 AND authenticated_ingress=?3 AND channel_assurance=?4 AND status='active'",
            params![actor_type.as_str(),payload.credential_identity,payload.authenticated_ingress,payload.channel_assurance],
            |row| row.get(0),
        )
        .optional()?;
    if active != Some(i64::from(actor_type == ActorType::HumanRemote)) {
        bail!("delegation resume authority does not match an active owner ingress");
    }
    let stopped_at: i64 = tx.query_row(
        "SELECT updated_at FROM execass_delegations WHERE delegation_id=?1",
        [&command.delegation_id],
        |row| row.get(0),
    )?;
    if payload.operation != RunControlOperation::DelegationResume
        || payload.target
            != (RunControlTarget::Delegation {
                delegation_id: command.delegation_id.clone(),
            })
        || payload.stopped_epoch != status.stop_epoch
        || payload.stopped_epoch != command.expected_stop_epoch
        || payload.policy_revision != status.policy_revision
        || payload.policy_revision != command.expected_policy_revision
        || payload.unresolved_effect_disclosure_digest != status.unresolved_external_effects_digest
        || payload.unresolved_effect_disclosure_digest
            != command.disclosed_unresolved_external_effects_digest
        || payload.delegation_state_revision != Some(status.state_revision)
        || payload.delegation_state_revision != Some(command.expected_state_revision)
        || payload.current_plan_revision != status.current_plan_revision
        || payload.current_plan_revision != command.expected_plan_revision
        || payload.idempotency_key != command.outbox_event.duplicate_identity
        || payload.request_correlation_id != command.outbox_event.correlation_id
        || payload.issued_at_ms != command.receipt.occurred_at
        || payload.observed_at_ms < stopped_at
        || payload.canonical_root_identity != store.root_identity
    {
        bail!("run-control attestation is not bound to the exact stopped delegation state");
    }
    Ok(())
}

fn replayed_event(conn: &Connection, event: &NewOutboxEvent) -> Result<Option<OutboxEventRecord>> {
    if receipt_by_causation_event(conn, &event.event_id)?.is_none() {
        return Ok(None);
    }
    get_outbox(conn, &event.event_id)
}

fn valid_effect_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}
