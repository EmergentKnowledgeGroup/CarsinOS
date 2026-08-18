//! EA-210 fenced continuation claim, pre-dispatch, and settle kernel.

use super::receipt::{
    receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{
    get_authority, get_continuation, get_outbox, get_technical_quota_snapshot,
    get_technical_resource_requirements_for_effect, insert_authority, insert_outbox,
};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{require_text, validate_outbox};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Debug)]
enum ClaimMutationOutcome {
    Claimed(Box<ClaimDraft>),
    Replayed(Box<ReplayDraft>),
    Superseded(Box<SupersedeDraft>),
    Lost(ContinuationStaleReason),
}

#[derive(Debug)]
enum SettleMutationOutcome {
    Settled(Box<SettleDraft>),
    Replayed(Box<ReplayDraft>),
    Superseded(Box<SupersedeDraft>),
    Lost(ContinuationStaleReason),
}

#[derive(Debug)]
enum ResourceLifecycleMutationOutcome {
    Applied(Box<ResourceLifecycleDraft>),
    Replayed(Box<ResourceLifecycleDraft>),
    Lost(ContinuationStaleReason),
}

#[derive(Debug)]
struct ClaimDraft {
    continuation: ContinuationRecord,
    action: ActionBranchRecord,
    identity: ContinuationClaimIdentity,
    outbox_event: OutboxEventRecord,
    technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

#[derive(Debug)]
struct SettleDraft {
    continuation: ContinuationRecord,
    action: ActionBranchRecord,
    identity: ContinuationClaimIdentity,
    outbox_event: OutboxEventRecord,
    technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

#[derive(Debug)]
struct ReplayDraft {
    identity: ContinuationClaimIdentity,
    result_status: ContinuationStatus,
    outbox_event: OutboxEventRecord,
    technical_resource_reservations: Vec<TechnicalResourceReservationIdentity>,
}

#[derive(Debug)]
struct ResourceBundleDraft {
    logical_effect_id: Option<String>,
    quota_snapshot_id: Option<String>,
    identities: Vec<TechnicalResourceReservationIdentity>,
    canonical_json: String,
    digest: String,
}

#[derive(Debug)]
struct ResourceLifecycleDraft {
    identity: ContinuationClaimIdentity,
    resolution: TechnicalResourceLifecycleResolution,
    outbox_event: OutboxEventRecord,
    technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

pub(super) struct OperationHistoryWrite<'a> {
    pub(super) event_id: &'a str,
    pub(super) operation: &'a str,
    pub(super) result_status: ContinuationStatus,
    pub(super) identity: &'a ContinuationClaimIdentity,
    pub(super) resource_set_json: &'a str,
    pub(super) resource_evidence_digest: Option<&'a str>,
    pub(super) recorded_at: i64,
}

#[derive(Debug)]
struct SupersedeDraft {
    reason: ContinuationStaleReason,
    continuation: ContinuationRecord,
    action: ActionBranchRecord,
    outbox_event: OutboxEventRecord,
}

#[derive(Debug)]
struct JobLease {
    payload_json: String,
    lease_owner: Option<String>,
    lease_expires_at: Option<i64>,
}

#[derive(Debug)]
pub(super) struct RuntimeLease {
    pub(super) state_root_generation: i64,
    pub(super) generation: i64,
    pub(super) host_instance_id: String,
    pub(super) fencing_token: i64,
    pub(super) acquired_at: i64,
    pub(super) expires_at: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Operation {
    Dispatch,
    Settle,
}

impl ExecAssStore {
    pub fn read_continuation_receipt_context(
        &self,
        continuation_id: &str,
        trusted_now: i64,
    ) -> Result<Option<ContinuationReceiptContext>> {
        require_text("continuation_id", continuation_id)?;
        if trusted_now <= 0 {
            bail!("continuation receipt context requires a positive trusted clock");
        }
        let conn = self.connection()?;
        let Some((
            delegation_id,
            delegation_revision,
            policy_revision,
            effective_authority_json,
            global_stop_epoch,
            global_receipt_count,
            global_receipt_head_digest,
            delegation_receipt_count,
            delegation_receipt_head_digest,
        )) = conn
            .query_row(
                r#"SELECT c.delegation_id,d.state_revision,d.policy_revision,
                          d.effective_authority_json,g.global_stop_epoch,
                          journal.receipt_count,journal.receipt_head_digest,
                          d.receipt_chain_count,d.receipt_chain_head_digest
                   FROM execass_continuations c
                   JOIN execass_delegations d ON d.delegation_id=c.delegation_id
                   CROSS JOIN execass_global_runtime_control g
                   CROSS JOIN execass_receipt_journal_state journal
                   WHERE c.continuation_id=?1 AND g.singleton=1 AND journal.singleton=1"#,
                params![continuation_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()
            .context("failed reading continuation receipt context")?
        else {
            return Ok(None);
        };
        let Some(runtime) = current_runtime_lease(&conn, trusted_now)? else {
            return Ok(None);
        };
        let (_, actor) = derived_runtime_authority(&runtime, policy_revision)?;
        Ok(Some(ContinuationReceiptContext {
            delegation_id,
            delegation_revision,
            policy_revision,
            global_stop_epoch,
            technical_quota_policy_digest: technical_quota_policy_digest(
                policy_revision,
                &effective_authority_json,
            ),
            global_receipt_count,
            global_receipt_head_digest,
            delegation_receipt_count,
            delegation_receipt_head_digest,
            state_root_generation: runtime.state_root_generation,
            runtime_host_generation: runtime.generation,
            runtime_host_instance_id: runtime.host_instance_id,
            runtime_fencing_token: runtime.fencing_token,
            runtime_actor: actor,
        }))
    }

    pub fn claim_continuation_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ContinuationClaimCommand,
    ) -> Result<ContinuationClaimOutcome> {
        validate_claim_command(command)?;
        let outcome =
            self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
                if receipt_by_causation_event(tx, &command.outbox_event.event_id)?.is_some() {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        match load_claim_replay(tx, command)? {
                            Some(draft) => ClaimMutationOutcome::Replayed(Box::new(draft)),
                            None => ClaimMutationOutcome::Lost(
                                ContinuationStaleReason::ClaimIdentityMismatch,
                            ),
                        },
                    ));
                }

                let Some(job) = load_job_lease(tx, &command.job_id)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::JobBindingMismatch,
                    )));
                };
                if !job_lease_matches(
                    &job,
                    &command.worker_id,
                    command.job_lease_expires_at,
                    command.trusted_now,
                ) {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::JobLeaseLostOrExpired,
                    )));
                }
                let Some(continuation) = get_continuation(tx, &command.continuation_id)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::NotFound,
                    )));
                };
                if continuation.job_id.as_deref() != Some(command.job_id.as_str())
                    || !job_payload_matches(&job.payload_json, &continuation)?
                {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::JobPayloadMismatch,
                    )));
                }
                if continuation.status != ContinuationStatus::Runnable {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::ContinuationNotRunnable,
                    )));
                }
                let Some(action) = load_action(tx, &continuation.action_id)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::ActionStateDrift,
                    )));
                };
                let Some(runtime) = current_runtime_lease(tx, command.trusted_now)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::RuntimeHostLeaseLostOrExpired,
                    )));
                };
                if receipt_runtime_mismatch(&command.receipt, &runtime) {
                    bail!("claim receipt runtime is not the current live host lease");
                }
                let delegation = super::rows::get_delegation(tx, &continuation.delegation_id)?
                    .context("claim delegation disappeared")?;
                let (runtime_authority, runtime_actor) =
                    derived_runtime_authority(&runtime, delegation.policy_revision)?;
                ensure_runtime_receipt_actor(
                    tx,
                    &command.receipt,
                    &runtime_authority,
                    &runtime_actor,
                )?;
                if let Some(reason) =
                    objective_drift(tx, &continuation, &action, ContinuationStatus::Runnable)?
                {
                    return supersede_with_receipt(tx, command, &continuation, &action, reason)
                        .map(|draft| {
                            AtomicReceiptMutation::Append(ClaimMutationOutcome::Superseded(
                                Box::new(draft),
                            ))
                        });
                }
                validate_claim_receipt_identity(command, &continuation)?;
                let Some(resource_bundle) =
                    prepare_resource_bundle(tx, &continuation, &action, &delegation)?
                else {
                    return Ok(AtomicReceiptMutation::NoAppend(ClaimMutationOutcome::Lost(
                        ContinuationStaleReason::TechnicalResourceUnavailable,
                    )));
                };
                insert_outbox(tx, &command.outbox_event)?;
                let identity = identity_from_parts(
                    &continuation,
                    &action,
                    command,
                    &runtime,
                    &runtime_actor,
                    &delegation,
                    &resource_bundle,
                )?;
                let new_fence = identity.continuation_fencing_token;
                insert_operation_history(
                    tx,
                    OperationHistoryWrite {
                        event_id: &command.outbox_event.event_id,
                        operation: "claim",
                        result_status: ContinuationStatus::Executing,
                        identity: &identity,
                        resource_set_json: &resource_bundle.canonical_json,
                        resource_evidence_digest: None,
                        recorded_at: command.write.occurred_at,
                    },
                )?;
                insert_resource_reservations(
                    tx,
                    &identity,
                    &resource_bundle.identities,
                    command.write.occurred_at,
                )?;
                if let Some(logical_effect_id) = &resource_bundle.logical_effect_id {
                    let changed = tx.execute(
                        "UPDATE execass_logical_effects SET state='claimed',updated_at=?1 WHERE delegation_id=?2 AND logical_effect_id=?3 AND state='planned'",
                        params![
                            command.write.occurred_at,
                            continuation.delegation_id,
                            logical_effect_id
                        ],
                    )?;
                    if changed != 1 {
                        bail!("logical effect lost its atomic claim transition");
                    }
                }
                let changed = tx.execute(
                    r#"UPDATE execass_continuations
                       SET status='executing', lease_owner=?1, lease_expires_at=?2,
                           fencing_token=?3, host_generation=?4, updated_at=?5
                       WHERE continuation_id=?6 AND delegation_id=?7 AND job_id=?8
                         AND status='runnable' AND fencing_token=?9
                         AND stop_epoch=?10 AND global_stop_epoch=?11"#,
                    params![
                        command.worker_id,
                        command.job_lease_expires_at,
                        new_fence,
                        runtime.generation,
                        command.write.occurred_at,
                        continuation.continuation_id,
                        continuation.delegation_id,
                        command.job_id,
                        continuation.fencing_token,
                        continuation.stop_epoch,
                        continuation.global_stop_epoch,
                    ],
                )?;
                if changed != 1 {
                    bail!("continuation lost its claim race");
                }
                update_action_status(
                    tx,
                    &action,
                    ContinuationStatus::Executing,
                    command.write.occurred_at,
                )?;
                let claimed = get_continuation(tx, &continuation.continuation_id)?
                    .context("claimed continuation disappeared")?;
                let claimed_action =
                    load_action(tx, &action.action_id)?.context("claimed action disappeared")?;
                let outbox_event = get_outbox(tx, &command.outbox_event.event_id)?
                    .context("claim outbox disappeared")?;
                let technical_resource_reservations =
                    load_resource_reservations(tx, &identity.claim_event_id)?;
                Ok(AtomicReceiptMutation::Append(
                    ClaimMutationOutcome::Claimed(Box::new(ClaimDraft {
                        continuation: claimed,
                        action: claimed_action,
                        identity,
                        outbox_event,
                        technical_resource_reservations,
                    })),
                ))
            })?;

        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: ClaimMutationOutcome::Claimed(draft),
                receipt,
            } => Ok(ContinuationClaimOutcome::Claimed(Box::new(claim_record(
                *draft, receipt,
            )))),
            AtomicReceiptWriteOutcome::NoAppend(ClaimMutationOutcome::Replayed(draft)) => {
                let receipt = receipt_by_causation_event(
                    &self.connection()?,
                    &draft.outbox_event.event.event_id,
                )?
                .context("replayed claim receipt disappeared")?;
                Ok(ContinuationClaimOutcome::Replayed(Box::new(replay_record(
                    *draft, receipt,
                ))))
            }
            AtomicReceiptWriteOutcome::Appended {
                value: ClaimMutationOutcome::Superseded(draft),
                receipt,
            } => Ok(ContinuationClaimOutcome::Superseded(Box::new(
                supersede_record(*draft, receipt),
            ))),
            AtomicReceiptWriteOutcome::NoAppend(ClaimMutationOutcome::Lost(reason)) => {
                Ok(ContinuationClaimOutcome::Lost { reason })
            }
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            } => Ok(ContinuationClaimOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            }),
            AtomicReceiptWriteOutcome::Appended { .. }
            | AtomicReceiptWriteOutcome::NoAppend(ClaimMutationOutcome::Claimed(_))
            | AtomicReceiptWriteOutcome::NoAppend(ClaimMutationOutcome::Superseded(_)) => {
                bail!("atomic claim receipt coordinator returned an impossible outcome")
            }
        }
    }

    pub fn validate_continuation_pre_dispatch(
        &self,
        command: &ContinuationDispatchValidationCommand,
    ) -> Result<ContinuationDispatchValidationOutcome> {
        validate_identity(&command.identity)?;
        if command.trusted_now <= 0 {
            bail!("pre-dispatch validation requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let outcome = match validate_claim_identity_live(
            &tx,
            &command.identity,
            command.trusted_now,
            Operation::Dispatch,
        )? {
            Ok((continuation, action)) => {
                if let Some(reason) =
                    objective_drift(&tx, &continuation, &action, ContinuationStatus::Executing)?
                {
                    ContinuationDispatchValidationOutcome::Stale { reason }
                } else if mark_claim_effect_invoking(&tx, &command.identity, command.trusted_now)? {
                    ContinuationDispatchValidationOutcome::Valid
                } else {
                    ContinuationDispatchValidationOutcome::Stale {
                        reason: ContinuationStaleReason::RuntimeHostLeaseLostOrExpired,
                    }
                }
            }
            Err(reason) => ContinuationDispatchValidationOutcome::Stale { reason },
        };
        tx.commit()
            .context("failed committing atomic pre-dispatch fence transition")?;
        Ok(outcome)
    }

    /// Returns only scheduler-recoverable technical-resource claims. Claimed
    /// effects become eligible after the reservation deadline. Invoking
    /// effects become eligible only after their exact runtime or job lease is
    /// no longer live, because reinvocation is unsafe beyond that boundary.
    pub fn list_technical_resource_recovery_candidates(
        &self,
        trusted_now: i64,
        limit: u32,
    ) -> Result<Vec<TechnicalResourceRecoveryCandidate>> {
        if trusted_now <= 0 || limit == 0 {
            bail!("technical resource recovery requires a positive clock and limit");
        }
        let conn = self.connection()?;
        let runtime = current_runtime_lease(&conn, trusted_now)?
            .context("technical resource recovery requires a current runtime host")?;
        let mut statement = conn.prepare(
            r#"SELECT DISTINCT h.event_id,e.state,h.recorded_at
               FROM execass_continuation_operation_history h
               JOIN execass_technical_resource_reservations r
                 ON r.claim_event_id=h.event_id AND r.status='reserved'
               JOIN execass_logical_effects e
                 ON e.delegation_id=h.delegation_id
                AND e.logical_effect_id=r.logical_effect_id
               JOIN execass_continuations c
                 ON c.delegation_id=h.delegation_id
                AND c.continuation_id=h.continuation_id
               JOIN jobs j ON j.job_id=h.job_id
               WHERE h.operation='claim'
                 AND (
                   (e.state='claimed' AND r.expires_at<=?1)
                   OR
                   (e.state='invoking' AND (
                     h.runtime_host_generation!=?2
                     OR h.runtime_host_instance_id!=?3
                     OR h.runtime_fencing_token!=?4
                     OR h.job_lease_expires_at<=?1
                     OR c.lease_owner IS NULL OR c.lease_expires_at IS NULL
                     OR c.lease_expires_at<=?1
                     OR j.lease_owner IS NULL OR j.lease_expires_at IS NULL
                     OR j.lease_expires_at<=?1
                   ))
                 )
               ORDER BY h.recorded_at,h.event_id
               LIMIT ?5"#,
        )?;
        let rows = statement
            .query_map(
                params![
                    trusted_now,
                    runtime.generation,
                    runtime.host_instance_id,
                    runtime.fencing_token,
                    i64::from(limit),
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(claim_event_id, state)| {
                let (identity, status, reservations) =
                    load_operation_history(&conn, &claim_event_id, "claim")?
                        .context("recovery candidate lost its immutable claim history")?;
                if status != ContinuationStatus::Executing || reservations.is_empty() {
                    bail!("technical resource recovery candidate has invalid claim history");
                }
                let kind = match state.as_str() {
                    "claimed" => TechnicalResourceRecoveryKind::ExpireUndispatched,
                    "invoking" => TechnicalResourceRecoveryKind::RecoverPossiblyInvoked,
                    _ => bail!("technical resource recovery selected an invalid effect state"),
                };
                Ok(TechnicalResourceRecoveryCandidate { identity, kind })
            })
            .collect()
    }

    pub fn resolve_technical_resource_recovery_job_evidence(
        &self,
        identity: &ContinuationClaimIdentity,
    ) -> Result<ReceiptEvidenceInput> {
        validate_identity(identity)?;
        if !validate_claim_provenance(&self.connection()?, identity)? {
            bail!("technical resource recovery evidence requires immutable claim provenance");
        }
        let link = self
            .resolve_authority_lineage(&identity.delegation_id)?
            .into_iter()
            .find(|link| {
                link.kind == AuthorityLinkKind::Job
                    && link.source_id == identity.job_id
                    && link.authoritative_revision == 0
                    && link.reachable
            })
            .context("technical resource recovery requires a pre-claim job authority link")?;
        Ok(ReceiptEvidenceInput {
            authority_link_id: link.link_id,
            kind: link.kind,
            source_id: link.source_id,
            authoritative_revision: link.authoritative_revision,
        })
    }

    pub fn settle_continuation_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ContinuationSettleCommand,
    ) -> Result<ContinuationSettleOutcome> {
        validate_settle_command(command)?;
        let actual_set_digest =
            technical_resource_actual_set_digest(&command.technical_resource_actuals)?;
        let outcome =
            self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
                if receipt_by_causation_event(tx, &command.outbox_event.event_id)?.is_some() {
                    return Ok(AtomicReceiptMutation::NoAppend(match load_settle_replay(
                        tx, command,
                    )? {
                        Some(draft) => SettleMutationOutcome::Replayed(Box::new(draft)),
                        None => SettleMutationOutcome::Lost(
                            ContinuationStaleReason::ClaimIdentityMismatch,
                        ),
                    }));
                }
                let (continuation, action) = match validate_claim_identity_live(
                    tx,
                    &command.identity,
                    command.trusted_now,
                    Operation::Settle,
                )? {
                    Ok(pair) => pair,
                    Err(reason) => {
                        return Ok(AtomicReceiptMutation::NoAppend(
                            SettleMutationOutcome::Lost(reason),
                        ))
                    }
                };
                if receipt_runtime_mismatch_identity(&command.receipt, &command.identity) {
                    bail!("settle receipt runtime is not the exact claim identity");
                }
                ensure_claim_runtime_receipt_actor(tx, &command.receipt, &command.identity)?;
                if let Some(reason) =
                    objective_drift(tx, &continuation, &action, ContinuationStatus::Executing)?
                {
                    return supersede_with_receipt_from_settle(
                        tx,
                        command,
                        &continuation,
                        &action,
                        reason,
                    )
                    .map(|draft| {
                        AtomicReceiptMutation::Append(SettleMutationOutcome::Superseded(
                            Box::new(draft),
                        ))
                    });
                }
                validate_settle_receipt_identity(command, &continuation)?;
                let resource_set_json = claim_resource_set_json(tx, &command.identity)?;
                insert_outbox(tx, &command.outbox_event)?;
                insert_operation_history(
                    tx,
                    OperationHistoryWrite {
                        event_id: &command.outbox_event.event_id,
                        operation: "settle",
                        result_status: command.result_status,
                        identity: &command.identity,
                        resource_set_json: &resource_set_json,
                        resource_evidence_digest: Some(&actual_set_digest),
                        recorded_at: command.write.occurred_at,
                    },
                )?;
                transition_resource_reservations_on_settle(
                    tx,
                    &command.identity,
                    command.result_status,
                    &command.technical_resource_actuals,
                    &command.outbox_event.event_id,
                    command.write.occurred_at,
                )?;
                let terminal = matches!(
                    command.result_status,
                    ContinuationStatus::Terminal | ContinuationStatus::Superseded
                );
                let completed_at = terminal.then_some(command.write.occurred_at);
                let changed = tx.execute(
                    r#"UPDATE execass_continuations
                       SET status=?1, lease_owner=NULL, lease_expires_at=NULL,
                           updated_at=?2, completed_at=?3
                       WHERE continuation_id=?4 AND delegation_id=?5 AND job_id=?6
                         AND status='executing' AND lease_owner=?7 AND lease_expires_at=?8
                         AND fencing_token=?9 AND host_generation=?10
                         AND stop_epoch=?11 AND global_stop_epoch=?12"#,
                    params![
                        command.result_status.as_str(),
                        command.write.occurred_at,
                        completed_at,
                        continuation.continuation_id,
                        continuation.delegation_id,
                        command.identity.job_id,
                        command.identity.worker_id,
                        command.identity.job_lease_expires_at,
                        command.identity.continuation_fencing_token,
                        command.identity.runtime_host_generation,
                        continuation.stop_epoch,
                        continuation.global_stop_epoch,
                    ],
                )?;
                if changed != 1 {
                    bail!("continuation lost its settle race");
                }
                update_action_status(tx, &action, command.result_status, command.write.occurred_at)?;
                let job_changed = tx.execute(
                    "UPDATE jobs SET enabled=0, next_run_at=NULL, lease_owner=NULL, lease_expires_at=NULL, updated_at=?1 WHERE job_id=?2 AND lease_owner=?3 AND lease_expires_at=?4 AND json_extract(payload_json,'$.mode')='execass.continuation'",
                    params![
                        command.write.occurred_at,
                        command.identity.job_id,
                        command.identity.worker_id,
                        command.identity.job_lease_expires_at,
                    ],
                )?;
                if job_changed != 1 {
                    bail!("continuation job lost its settle lease race");
                }
                let settled = get_continuation(tx, &continuation.continuation_id)?
                    .context("settled continuation disappeared")?;
                let settled_action =
                    load_action(tx, &action.action_id)?.context("settled action disappeared")?;
                let outbox_event = get_outbox(tx, &command.outbox_event.event_id)?
                    .context("settle outbox disappeared")?;
                let technical_resource_reservations =
                    load_resource_reservations(tx, &command.identity.claim_event_id)?;
                Ok(AtomicReceiptMutation::Append(SettleMutationOutcome::Settled(
                    Box::new(SettleDraft {
                        continuation: settled,
                        action: settled_action,
                        identity: command.identity.clone(),
                        outbox_event,
                        technical_resource_reservations,
                    }),
                )))
            })?;

        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: SettleMutationOutcome::Settled(draft),
                receipt,
            } => Ok(ContinuationSettleOutcome::Settled(Box::new(settle_record(
                *draft, receipt,
            )))),
            AtomicReceiptWriteOutcome::NoAppend(SettleMutationOutcome::Replayed(draft)) => {
                let receipt = receipt_by_causation_event(
                    &self.connection()?,
                    &draft.outbox_event.event.event_id,
                )?
                .context("replayed settle receipt disappeared")?;
                Ok(ContinuationSettleOutcome::Replayed(Box::new(
                    replay_record(*draft, receipt),
                )))
            }
            AtomicReceiptWriteOutcome::Appended {
                value: SettleMutationOutcome::Superseded(draft),
                receipt,
            } => Ok(ContinuationSettleOutcome::Superseded(Box::new(
                supersede_record(*draft, receipt),
            ))),
            AtomicReceiptWriteOutcome::NoAppend(SettleMutationOutcome::Lost(reason)) => {
                Ok(ContinuationSettleOutcome::Lost { reason })
            }
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            } => Ok(ContinuationSettleOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            }),
            AtomicReceiptWriteOutcome::Appended { .. }
            | AtomicReceiptWriteOutcome::NoAppend(SettleMutationOutcome::Settled(_))
            | AtomicReceiptWriteOutcome::NoAppend(SettleMutationOutcome::Superseded(_)) => {
                bail!("atomic settle receipt coordinator returned an impossible outcome")
            }
        }
    }

    pub fn resolve_technical_resources_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &TechnicalResourceLifecycleCommand,
    ) -> Result<TechnicalResourceLifecycleOutcome> {
        if matches!(
            command.resolution,
            TechnicalResourceLifecycleResolution::ReconcileAbsent
                | TechnicalResourceLifecycleResolution::ReconcilePresent
        ) {
            bail!("caller-selected technical resource reconciliation is forbidden; verified recorder evidence is required");
        }
        validate_resource_lifecycle_command(command)?;
        let operation = match command.resolution {
            TechnicalResourceLifecycleResolution::ExpireUndispatched => "expire",
            TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => "recover",
            TechnicalResourceLifecycleResolution::ReconcileAbsent
            | TechnicalResourceLifecycleResolution::ReconcilePresent => "reconcile",
        };
        let result_status = match command.resolution {
            TechnicalResourceLifecycleResolution::ReconcilePresent => ContinuationStatus::Terminal,
            TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => {
                ContinuationStatus::Uncertain
            }
            TechnicalResourceLifecycleResolution::ExpireUndispatched
            | TechnicalResourceLifecycleResolution::ReconcileAbsent => {
                ContinuationStatus::Superseded
            }
        };
        let lifecycle_evidence_digest = technical_resource_lifecycle_evidence_digest(command)?;
        let outcome =
            self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
                if receipt_by_causation_event(tx, &command.outbox_event.event_id)?.is_some() {
                    let Some((stored_identity, stored_status, stored_resources)) =
                        load_operation_history(tx, &command.outbox_event.event_id, operation)?
                    else {
                        return Ok(AtomicReceiptMutation::NoAppend(
                            ResourceLifecycleMutationOutcome::Lost(
                                ContinuationStaleReason::ClaimIdentityMismatch,
                            ),
                        ));
                    };
                    let outbox_event = get_outbox(tx, &command.outbox_event.event_id)?
                        .context("resource lifecycle replay outbox disappeared")?;
                    let stored_evidence: Option<String> = tx.query_row(
                        "SELECT technical_resource_evidence_digest FROM execass_continuation_operation_history WHERE event_id=?1 AND operation=?2",
                        params![command.outbox_event.event_id, operation],
                        |row| row.get(0),
                    )?;
                    let reservations =
                        load_resource_reservations(tx, &command.identity.claim_event_id)?;
                    let identities = reservations
                        .iter()
                        .map(|reservation| reservation.identity.clone())
                        .collect::<Vec<_>>();
                    if stored_identity != command.identity
                        || stored_status != result_status
                        || stored_resources != identities
                        || outbox_event.event != command.outbox_event
                        || stored_evidence.as_deref()
                            != Some(lifecycle_evidence_digest.as_str())
                    {
                        return Ok(AtomicReceiptMutation::NoAppend(
                            ResourceLifecycleMutationOutcome::Lost(
                                ContinuationStaleReason::ClaimIdentityMismatch,
                            ),
                        ));
                    }
                    return Ok(AtomicReceiptMutation::NoAppend(
                        ResourceLifecycleMutationOutcome::Replayed(Box::new(
                            ResourceLifecycleDraft {
                                identity: stored_identity,
                                resolution: command.resolution,
                                outbox_event,
                                technical_resource_reservations: reservations,
                            },
                        )),
                    ));
                }
                if !validate_claim_provenance(tx, &command.identity)? {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        ResourceLifecycleMutationOutcome::Lost(
                            ContinuationStaleReason::ClaimIdentityMismatch,
                        ),
                    ));
                }
                let continuation = get_continuation(tx, &command.identity.continuation_id)?
                    .context("resource lifecycle continuation disappeared")?;
                if continuation.delegation_id != command.identity.delegation_id
                    || continuation.action_id != command.identity.action_id
                    || continuation.fencing_token != command.identity.continuation_fencing_token
                {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        ResourceLifecycleMutationOutcome::Lost(
                            ContinuationStaleReason::ClaimIdentityMismatch,
                        ),
                    ));
                }
                let Some(runtime) = current_runtime_lease(tx, command.trusted_now)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        ResourceLifecycleMutationOutcome::Lost(
                            ContinuationStaleReason::RuntimeHostLeaseLostOrExpired,
                        ),
                    ));
                };
                if receipt_runtime_mismatch(&command.receipt, &runtime) {
                    bail!("resource lifecycle receipt is not from the current runtime host");
                }
                let delegation = super::rows::get_delegation(tx, &command.identity.delegation_id)?
                    .context("resource lifecycle delegation disappeared")?;
                let (runtime_authority, runtime_actor) =
                    derived_runtime_authority(&runtime, delegation.policy_revision)?;
                ensure_runtime_receipt_actor(
                    tx,
                    &command.receipt,
                    &runtime_authority,
                    &runtime_actor,
                )?;
                let action = load_action(tx, &continuation.action_id)?
                    .context("resource lifecycle action disappeared")?;
                let claim_resource_json = claim_resource_set_json(tx, &command.identity)?;
                insert_outbox(tx, &command.outbox_event)?;
                insert_operation_history(
                    tx,
                    OperationHistoryWrite {
                        event_id: &command.outbox_event.event_id,
                        operation,
                        result_status,
                        identity: &command.identity,
                        resource_set_json: &claim_resource_json,
                        resource_evidence_digest: Some(&lifecycle_evidence_digest),
                        recorded_at: command.write.occurred_at,
                    },
                )?;
                apply_resource_lifecycle_transition(tx, command)?;
                close_resource_lifecycle_continuation(
                    tx,
                    command,
                    &continuation,
                    &action,
                    result_status,
                )?;
                let outbox_event = get_outbox(tx, &command.outbox_event.event_id)?
                    .context("resource lifecycle outbox disappeared")?;
                let reservations =
                    load_resource_reservations(tx, &command.identity.claim_event_id)?;
                Ok(AtomicReceiptMutation::Append(
                    ResourceLifecycleMutationOutcome::Applied(Box::new(ResourceLifecycleDraft {
                        identity: command.identity.clone(),
                        resolution: command.resolution,
                        outbox_event,
                        technical_resource_reservations: reservations,
                    })),
                ))
            })?;
        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: ResourceLifecycleMutationOutcome::Applied(draft),
                receipt,
            } => Ok(TechnicalResourceLifecycleOutcome::Applied(Box::new(
                resource_lifecycle_record(*draft, receipt),
            ))),
            AtomicReceiptWriteOutcome::NoAppend(ResourceLifecycleMutationOutcome::Replayed(
                draft,
            )) => {
                let receipt = receipt_by_causation_event(
                    &self.connection()?,
                    &draft.outbox_event.event.event_id,
                )?
                .context("resource lifecycle replay receipt disappeared")?;
                Ok(TechnicalResourceLifecycleOutcome::Replayed(Box::new(
                    resource_lifecycle_record(*draft, receipt),
                )))
            }
            AtomicReceiptWriteOutcome::NoAppend(ResourceLifecycleMutationOutcome::Lost(reason)) => {
                Ok(TechnicalResourceLifecycleOutcome::Lost { reason })
            }
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            } => Ok(TechnicalResourceLifecycleOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            }),
            AtomicReceiptWriteOutcome::Appended { .. }
            | AtomicReceiptWriteOutcome::NoAppend(ResourceLifecycleMutationOutcome::Applied(_)) => {
                bail!("resource lifecycle receipt coordinator returned an impossible outcome")
            }
        }
    }
}

fn validate_claim_command(command: &ContinuationClaimCommand) -> Result<()> {
    require_text("continuation_id", &command.continuation_id)?;
    require_text("job_id", &command.job_id)?;
    require_text("worker_id", &command.worker_id)?;
    validate_write_outbox_receipt(
        &command.write,
        &command.outbox_event,
        &command.receipt,
        &command.continuation_id,
        command.trusted_now,
    )?;
    if command.job_lease_expires_at <= command.trusted_now {
        bail!("claim cannot accept an expired job lease or the expiry boundary");
    }
    Ok(())
}

fn validate_settle_command(command: &ContinuationSettleCommand) -> Result<()> {
    validate_identity(&command.identity)?;
    validate_write_outbox_receipt(
        &command.write,
        &command.outbox_event,
        &command.receipt,
        &command.identity.continuation_id,
        command.trusted_now,
    )?;
    if !matches!(
        command.result_status,
        ContinuationStatus::Waiting
            | ContinuationStatus::Uncertain
            | ContinuationStatus::Terminal
            | ContinuationStatus::Superseded
    ) {
        bail!("settle status must be waiting, uncertain, terminal, or superseded");
    }
    Ok(())
}

fn validate_resource_lifecycle_command(command: &TechnicalResourceLifecycleCommand) -> Result<()> {
    validate_identity(&command.identity)?;
    validate_write_outbox_receipt(
        &command.write,
        &command.outbox_event,
        &command.receipt,
        &command.identity.continuation_id,
        command.trusted_now,
    )?;
    let authoritative_evidence_digest =
        technical_resource_lifecycle_evidence_reference_digest(&command.receipt.evidence)?;
    if command.evidence_digest != authoritative_evidence_digest {
        bail!("technical resource lifecycle evidence digest does not match its exact authoritative reference");
    }
    match command.resolution {
        TechnicalResourceLifecycleResolution::ReconcilePresent => {
            if command.technical_resource_actuals.is_empty() {
                bail!("present reconciliation requires exact technical resource actuals");
            }
        }
        TechnicalResourceLifecycleResolution::ExpireUndispatched
        | TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked
        | TechnicalResourceLifecycleResolution::ReconcileAbsent => {
            if !command.technical_resource_actuals.is_empty() {
                bail!("non-present technical resource lifecycle operation cannot record technical actuals");
            }
        }
    }
    Ok(())
}

pub(super) fn validate_write_outbox_receipt(
    write: &WriteContext,
    outbox: &NewOutboxEvent,
    receipt: &AppendReceiptCommand,
    continuation_id: &str,
    trusted_now: i64,
) -> Result<()> {
    require_text("write.idempotency_key", &write.idempotency_key)?;
    require_text("write.correlation_id", &write.correlation_id)?;
    require_text("write.causation_id", &write.causation_id)?;
    if trusted_now <= 0 || write.occurred_at <= 0 {
        bail!("continuation operation requires positive trusted times");
    }
    validate_outbox(outbox)?;
    if outbox.event_name != OutboxEventName::ContinuationClaimedOrResultRecorded
        || outbox.causation_id != write.causation_id
        || outbox.duplicate_identity != write.idempotency_key
        || outbox.correlation_id != write.correlation_id
        || outbox.occurred_at != write.occurred_at
        || receipt.receipt_kind != ReceiptKind::Continuation
        || receipt.subject.kind != ReceiptSubjectKind::Continuation
        || receipt.subject.subject_id != continuation_id
        || receipt.causation_id != write.causation_id
        || receipt.causation_event_id != outbox.event_id
        || receipt.occurred_at != write.occurred_at
        || receipt.committed_at < write.occurred_at
    {
        bail!("continuation write, outbox, and receipt identities do not match");
    }
    Ok(())
}

fn validate_claim_receipt_identity(
    command: &ContinuationClaimCommand,
    continuation: &ContinuationRecord,
) -> Result<()> {
    if command.outbox_event.aggregate_id != continuation.delegation_id
        || command.outbox_event.aggregate_revision != continuation.target_delegation_revision
        || command.receipt.delegation_id != continuation.delegation_id
        || command.receipt.expected_state_revision != continuation.target_delegation_revision
        || command.receipt.subject.revision != continuation.target_delegation_revision
    {
        bail!("claim receipt does not target the exact continuation delegation revision");
    }
    Ok(())
}

fn validate_settle_receipt_identity(
    command: &ContinuationSettleCommand,
    continuation: &ContinuationRecord,
) -> Result<()> {
    if command.outbox_event.aggregate_id != continuation.delegation_id
        || command.outbox_event.aggregate_revision != continuation.target_delegation_revision
        || command.receipt.delegation_id != continuation.delegation_id
        || command.receipt.expected_state_revision != continuation.target_delegation_revision
        || command.receipt.subject.revision != continuation.target_delegation_revision
    {
        bail!("settle receipt does not target the exact continuation delegation revision");
    }
    Ok(())
}

pub(super) fn validate_identity(identity: &ContinuationClaimIdentity) -> Result<()> {
    require_text("identity.claim_event_id", &identity.claim_event_id)?;
    require_text("identity.claim_receipt_id", &identity.claim_receipt_id)?;
    require_text("identity.continuation_id", &identity.continuation_id)?;
    require_text("identity.delegation_id", &identity.delegation_id)?;
    require_text("identity.action_id", &identity.action_id)?;
    require_text("identity.job_id", &identity.job_id)?;
    require_text("identity.worker_id", &identity.worker_id)?;
    require_text(
        "identity.runtime_host_instance_id",
        &identity.runtime_host_instance_id,
    )?;
    require_text(
        "identity.runtime_authority_provenance_id",
        &identity.runtime_authority_provenance_id,
    )?;
    require_text(
        "identity.runtime_actor_identity",
        &identity.runtime_actor_identity,
    )?;
    require_text(
        "identity.technical_quota_policy_digest",
        &identity.technical_quota_policy_digest,
    )?;
    if identity.job_lease_expires_at <= 0
        || identity.continuation_fencing_token <= 0
        || identity.runtime_host_generation <= 0
        || identity.runtime_fencing_token <= 0
        || identity.state_root_generation <= 0
        || identity.policy_revision <= 0
        || identity.global_stop_epoch < 0
    {
        bail!("claim identity has invalid lease, fence, host, or epoch");
    }
    Ok(())
}

fn load_job_lease(conn: &Connection, job_id: &str) -> Result<Option<JobLease>> {
    conn.query_row(
        "SELECT payload_json,lease_owner,lease_expires_at FROM jobs WHERE job_id=?1 AND deleted_at IS NULL",
        params![job_id],
        |row| {
            Ok(JobLease {
                payload_json: row.get(0)?,
                lease_owner: row.get(1)?,
                lease_expires_at: row.get(2)?,
            })
        },
    )
    .optional()
    .context("failed reading continuation job lease")
}

fn job_lease_matches(job: &JobLease, worker: &str, expires_at: i64, trusted_now: i64) -> bool {
    job.lease_owner.as_deref() == Some(worker)
        && job.lease_expires_at == Some(expires_at)
        && expires_at > trusted_now
}

fn job_payload_matches(payload_json: &str, continuation: &ContinuationRecord) -> Result<bool> {
    let value: serde_json::Value =
        serde_json::from_str(payload_json).context("continuation job payload is invalid JSON")?;
    Ok(value.get("mode").and_then(serde_json::Value::as_str)
        == Some(super::jobs::EXECASS_CONTINUATION_JOB_MODE)
        && value
            .get("continuation_id")
            .and_then(serde_json::Value::as_str)
            == Some(continuation.continuation_id.as_str())
        && value
            .get("delegation_id")
            .and_then(serde_json::Value::as_str)
            == Some(continuation.delegation_id.as_str())
        && value.get("action_id").and_then(serde_json::Value::as_str)
            == Some(continuation.action_id.as_str())
        && value
            .get("target_delegation_revision")
            .and_then(serde_json::Value::as_i64)
            == Some(continuation.target_delegation_revision)
        && value
            .get("target_plan_revision")
            .and_then(serde_json::Value::as_i64)
            == Some(continuation.target_plan_revision)
        && value.get("branch_kind").and_then(serde_json::Value::as_str)
            == Some(continuation.branch_kind.as_str())
        && value
            .get("causation_kind")
            .and_then(serde_json::Value::as_str)
            == Some(continuation.causation_kind.as_str())
        && value
            .get("causation_id")
            .and_then(serde_json::Value::as_str)
            == Some(continuation.causation_id.as_str()))
}

pub(super) fn current_runtime_lease(
    conn: &Connection,
    trusted_now: i64,
) -> Result<Option<RuntimeLease>> {
    conn.query_row(
        r#"SELECT g.state_root_generation,l.generation,l.host_instance_id,l.fencing_token,
                  l.acquired_at,l.expires_at
           FROM execass_runtime_host_leases l
           JOIN execass_runtime_host_generations g
             ON g.generation=l.generation AND g.host_instance_id=l.host_instance_id
           WHERE l.ownership_scope='execass' AND g.ownership_scope='execass'
             AND l.released_at IS NULL AND l.expires_at>?1 AND g.ended_at IS NULL
           ORDER BY l.generation DESC,l.fencing_token DESC LIMIT 1"#,
        params![trusted_now],
        |row| {
            Ok(RuntimeLease {
                state_root_generation: row.get(0)?,
                generation: row.get(1)?,
                host_instance_id: row.get(2)?,
                fencing_token: row.get(3)?,
                acquired_at: row.get(4)?,
                expires_at: row.get(5)?,
            })
        },
    )
    .optional()
    .context("failed reading current ExecAss runtime host lease")
}

fn exact_runtime_lease(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
    trusted_now: i64,
) -> Result<bool> {
    conn.query_row(
        r#"SELECT 1 FROM execass_runtime_host_leases l
           JOIN execass_runtime_host_generations g
             ON g.generation=l.generation AND g.host_instance_id=l.host_instance_id
           WHERE l.ownership_scope='execass' AND g.ownership_scope='execass'
             AND l.generation=?1 AND l.host_instance_id=?2 AND l.fencing_token=?3
             AND g.state_root_generation=?4
             AND l.released_at IS NULL AND l.expires_at>?5 AND g.ended_at IS NULL"#,
        params![
            identity.runtime_host_generation,
            identity.runtime_host_instance_id,
            identity.runtime_fencing_token,
            identity.state_root_generation,
            trusted_now,
        ],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .context("failed validating exact ExecAss runtime host lease")
}

pub(super) fn receipt_runtime_mismatch(
    receipt: &AppendReceiptCommand,
    runtime: &RuntimeLease,
) -> bool {
    receipt.state_root_generation != runtime.state_root_generation
        || receipt.runtime.host_generation != runtime.generation
        || receipt.runtime.host_instance_id != runtime.host_instance_id
        || receipt.runtime.fencing_token != runtime.fencing_token
}

fn receipt_runtime_mismatch_identity(
    receipt: &AppendReceiptCommand,
    identity: &ContinuationClaimIdentity,
) -> bool {
    receipt.state_root_generation != identity.state_root_generation
        || receipt.runtime.host_generation != identity.runtime_host_generation
        || receipt.runtime.host_instance_id != identity.runtime_host_instance_id
        || receipt.runtime.fencing_token != identity.runtime_fencing_token
}

fn technical_quota_policy_digest(policy_revision: i64, effective_authority_json: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical_quota_policy.v1\0");
    digest.update(policy_revision.to_be_bytes());
    digest.update(b"\0");
    digest.update(effective_authority_json.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

fn prepare_resource_bundle(
    conn: &Connection,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    delegation: &DelegationRecord,
) -> Result<Option<ResourceBundleDraft>> {
    let mut statement = conn.prepare(
        r#"SELECT logical_effect_id FROM execass_logical_effects
           WHERE delegation_id=?1 AND continuation_id=?2 AND state='planned'
           ORDER BY logical_effect_id LIMIT 2"#,
    )?;
    let effect_ids = statement
        .query_map(
            params![continuation.delegation_id, continuation.continuation_id],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if effect_ids.len() > 1 {
        bail!("continuation has multiple planned logical effects");
    }
    let Some(logical_effect_id) = effect_ids.first() else {
        let identities = Vec::new();
        let canonical_json = serde_json::to_string(&identities)?;
        return Ok(Some(ResourceBundleDraft {
            logical_effect_id: None,
            quota_snapshot_id: None,
            digest: resource_identity_set_digest(&identities)?,
            identities,
            canonical_json,
        }));
    };
    let effect = super::rows::get_planned_logical_effect(conn, logical_effect_id)?
        .context("planned logical effect disappeared")?;
    let requirements = get_technical_resource_requirements_for_effect(conn, logical_effect_id)?
        .context("planned logical effect has no immutable technical resource requirement set")?;
    let snapshot = get_technical_quota_snapshot(conn, &requirements.quota_snapshot_id)?
        .context("technical resource requirement set has no immutable quota snapshot")?;
    let plan = super::rows::get_plan_by_revision(
        conn,
        &continuation.delegation_id,
        continuation.target_plan_revision,
    )?
    .context("technical quota snapshot plan disappeared")?;
    let authority_digest = carsinos_core::execass_policy::technical_effective_authority_digest(
        &delegation.effective_authority_json,
    )
    .map_err(|detail| anyhow::anyhow!("invalid effective authority for quota: {detail}"))?;
    if snapshot.delegation_id != continuation.delegation_id
        || snapshot.policy_revision != delegation.policy_revision
        || snapshot.effective_authority_digest != authority_digest
        || snapshot.scope_key != "delegation"
        || requirements.quota_snapshot_id != snapshot.quota_snapshot_id
        || requirements.delegation_id != continuation.delegation_id
        || requirements.logical_effect_id != effect.logical_effect_id
        || requirements.action_id != action.action_id
        || requirements.manifest_digest != effect.manifest_digest
        || requirements.manifest_digest != plan.manifest_digest
    {
        bail!("technical quota or requirement set drifted from its exact action/effect authority");
    }
    let mut identities = Vec::new();
    for requirement in &requirements.requirements {
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| {
                entry.technical_resource_kind == requirement.technical_resource_kind
                    && entry.unit == requirement.unit
            })
            .context("technical resource requirement has no quota entry")?;
        let used: i64 = conn.query_row(
            r#"SELECT COALESCE(SUM(CASE
                 WHEN r.status IN ('reserved','reconciliation_required') THEN r.amount_reserved
                 WHEN r.status='settled' THEN a.amount_actual ELSE 0 END),0)
               FROM execass_technical_resource_reservations r
               LEFT JOIN execass_technical_resource_actuals a ON a.reservation_id=r.reservation_id
               WHERE r.quota_snapshot_id=?1 AND r.technical_resource_kind=?2 AND r.unit=?3"#,
            params![
                snapshot.quota_snapshot_id,
                entry.technical_resource_kind.as_str(),
                entry.unit,
            ],
            |row| row.get(0),
        )?;
        if used
            .checked_add(requirement.amount_required)
            .context("technical resource use overflow")?
            > entry.amount_limit
        {
            return Ok(None);
        }
        let material = format!(
            "{}\0{}\0{}\0{}",
            effect.logical_effect_id,
            snapshot.quota_snapshot_id,
            entry.technical_resource_kind.as_str(),
            entry.unit
        );
        let mut digest = Sha256::new();
        digest.update(b"carsinos.execass.technical-resource.reservation.v1\0");
        digest.update(material.as_bytes());
        identities.push(TechnicalResourceReservationIdentity {
            reservation_id: format!("technical-reservation-{:x}", digest.finalize()),
            quota_snapshot_id: snapshot.quota_snapshot_id.clone(),
            logical_effect_id: effect.logical_effect_id.clone(),
            technical_resource_kind: entry.technical_resource_kind.as_str().to_string(),
            unit: entry.unit.clone(),
            amount_reserved: requirement.amount_required,
        });
    }
    identities.sort_by(|left, right| {
        (&left.technical_resource_kind, &left.unit)
            .cmp(&(&right.technical_resource_kind, &right.unit))
    });
    let canonical_json = serde_json::to_string(&identities)
        .context("failed serializing technical resource reservation set")?;
    let digest = resource_identity_set_digest(&identities)?;
    Ok(Some(ResourceBundleDraft {
        logical_effect_id: Some(effect.logical_effect_id),
        quota_snapshot_id: Some(snapshot.quota_snapshot_id),
        identities,
        canonical_json,
        digest,
    }))
}

pub(super) fn resource_identity_set_digest(
    identities: &[TechnicalResourceReservationIdentity],
) -> Result<String> {
    let canonical_json = serde_json::to_string(identities)
        .context("failed serializing technical resource identity set")?;
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.reservation-set.v1\0");
    digest.update(canonical_json.as_bytes());
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn technical_resource_actual_set_digest(
    actuals: &[TechnicalResourceActualInput],
) -> Result<String> {
    let mut canonical = actuals
        .iter()
        .map(|actual| {
            require_text("technical actual reservation_id", &actual.reservation_id)?;
            require_text("technical actual evidence_digest", &actual.evidence_digest)?;
            if actual.amount_actual < 0 {
                bail!("technical resource actual cannot be negative");
            }
            Ok((
                actual.reservation_id.clone(),
                actual.amount_actual,
                actual.evidence_digest.clone(),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    canonical.sort();
    if canonical.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        bail!("technical resource actual set contains a duplicate reservation");
    }
    let canonical_json = serde_json::to_string(&canonical)?;
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.actual-set.v1\0");
    digest.update(canonical_json.as_bytes());
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn technical_resource_lifecycle_evidence_digest(
    command: &TechnicalResourceLifecycleCommand,
) -> Result<String> {
    let actual_set_digest =
        technical_resource_actual_set_digest(&command.technical_resource_actuals)?;
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.lifecycle-evidence.v1\0");
    digest.update(command.evidence_digest.as_bytes());
    digest.update(b"\0");
    digest.update(actual_set_digest.as_bytes());
    Ok(format!("sha256:{:x}", digest.finalize()))
}

/// Digest the one normalized authority reference that permits a technical
/// resource lifecycle transition. The referenced source itself remains in its
/// canonical durable table; this digest binds replay/history to the exact
/// authority link without copying source content into accounting rows.
pub fn technical_resource_lifecycle_evidence_reference_digest(
    evidence: &[ReceiptEvidenceInput],
) -> Result<String> {
    if evidence.len() != 1 {
        bail!("technical resource lifecycle requires exactly one authoritative evidence reference");
    }
    let item = &evidence[0];
    require_text(
        "technical lifecycle authority_link_id",
        &item.authority_link_id,
    )?;
    require_text("technical lifecycle evidence source_id", &item.source_id)?;
    if item.authoritative_revision != 0 {
        bail!("technical lifecycle evidence must use the canonical source revision");
    }
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.evidence-reference.v1\0");
    for value in [
        item.kind.as_str(),
        item.source_id.as_str(),
        item.authority_link_id.as_str(),
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    digest.update(item.authoritative_revision.to_be_bytes());
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn insert_resource_reservations(
    conn: &Connection,
    claim: &ContinuationClaimIdentity,
    identities: &[TechnicalResourceReservationIdentity],
    created_at: i64,
) -> Result<()> {
    for identity in identities {
        conn.execute(
            r#"INSERT INTO execass_technical_resource_reservations(
                 reservation_id,delegation_id,logical_effect_id,quota_snapshot_id,
                 continuation_id,claim_event_id,claim_receipt_id,technical_resource_kind,
                 unit,amount_reserved,status,idempotency_key,continuation_fencing_token,
                 runtime_host_generation,runtime_fencing_token,created_at,expires_at,settled_at
               ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'reserved',?11,?12,?13,?14,?15,?16,NULL)"#,
            params![
                identity.reservation_id,
                claim.delegation_id,
                identity.logical_effect_id,
                identity.quota_snapshot_id,
                claim.continuation_id,
                claim.claim_event_id,
                claim.claim_receipt_id,
                identity.technical_resource_kind,
                identity.unit,
                identity.amount_reserved,
                format!("{}:{}", claim.claim_event_id, identity.reservation_id),
                claim.continuation_fencing_token,
                claim.runtime_host_generation,
                claim.runtime_fencing_token,
                created_at,
                claim.job_lease_expires_at,
            ],
        )
        .context("failed reserving exact technical resource capacity")?;
    }
    Ok(())
}

pub(super) fn load_resource_reservations(
    conn: &Connection,
    claim_event_id: &str,
) -> Result<Vec<TechnicalResourceReservationRecord>> {
    let mut statement = conn.prepare(
        r#"SELECT reservation_id,quota_snapshot_id,logical_effect_id,technical_resource_kind,
                  unit,amount_reserved,delegation_id,continuation_id,claim_event_id,
                  claim_receipt_id,status,idempotency_key,continuation_fencing_token,
                  runtime_host_generation,runtime_fencing_token,created_at,expires_at,settled_at
           FROM execass_technical_resource_reservations
           WHERE claim_event_id=?1 ORDER BY technical_resource_kind,unit"#,
    )?;
    let reservations = statement
        .query_map(params![claim_event_id], |row| {
            Ok(TechnicalResourceReservationRecord {
                identity: TechnicalResourceReservationIdentity {
                    reservation_id: row.get(0)?,
                    quota_snapshot_id: row.get(1)?,
                    logical_effect_id: row.get(2)?,
                    technical_resource_kind: row.get(3)?,
                    unit: row.get(4)?,
                    amount_reserved: row.get(5)?,
                },
                delegation_id: row.get(6)?,
                continuation_id: row.get(7)?,
                claim_event_id: row.get(8)?,
                claim_receipt_id: row.get(9)?,
                status: row.get(10)?,
                idempotency_key: row.get(11)?,
                continuation_fencing_token: row.get(12)?,
                runtime_host_generation: row.get(13)?,
                runtime_fencing_token: row.get(14)?,
                created_at: row.get(15)?,
                expires_at: row.get(16)?,
                settled_at: row.get(17)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed reading technical resource reservations")?;
    Ok(reservations)
}

pub(super) fn claim_resource_set_json(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
) -> Result<String> {
    let Some((stored, status, resources)) =
        load_operation_history(conn, &identity.claim_event_id, "claim")?
    else {
        bail!("immutable claim resource set disappeared");
    };
    if stored != *identity || status != ContinuationStatus::Executing {
        bail!("immutable claim resource set identity changed");
    }
    serde_json::to_string(&resources).context("failed serializing immutable claim resource set")
}

fn transition_resource_reservations_on_settle(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
    result_status: ContinuationStatus,
    actuals: &[TechnicalResourceActualInput],
    settlement_event_id: &str,
    settled_at: i64,
) -> Result<()> {
    let reservations = load_resource_reservations(conn, &identity.claim_event_id)?;
    if result_status == ContinuationStatus::Terminal && reservations.len() != actuals.len() {
        bail!("terminal technical resource settlement requires one actual per reservation");
    }
    if result_status != ContinuationStatus::Terminal && !actuals.is_empty() {
        bail!("nonterminal technical resource settlement cannot record actual consumption");
    }
    let effect_state =
        transition_claim_effect_on_settle(conn, &reservations, result_status, settled_at)?;
    for reservation in reservations {
        if result_status == ContinuationStatus::Terminal {
            let actual = actuals
                .iter()
                .find(|actual| actual.reservation_id == reservation.identity.reservation_id)
                .context("terminal technical resource settlement omitted a reservation actual")?;
            if actual.amount_actual < 0
                || actual.amount_actual > reservation.identity.amount_reserved
                || actual.evidence_digest.trim().is_empty()
                || actual.evidence_digest.trim() != actual.evidence_digest
                || actuals
                    .iter()
                    .filter(|candidate| candidate.reservation_id == actual.reservation_id)
                    .count()
                    != 1
            {
                bail!("terminal technical resource actual is invalid or duplicated");
            }
            let mut digest = Sha256::new();
            digest.update(b"carsinos.execass.technical-resource.actual.v1\0");
            digest.update(settlement_event_id.as_bytes());
            digest.update(b"\0");
            digest.update(reservation.identity.reservation_id.as_bytes());
            digest.update(b"\0");
            digest.update(actual.amount_actual.to_be_bytes());
            digest.update(b"\0");
            digest.update(actual.evidence_digest.as_bytes());
            let material_digest = format!("{:x}", digest.finalize());
            let actual_id = format!("technical-actual-{material_digest}");
            conn.execute(
                r#"INSERT INTO execass_technical_resource_actuals(
                     technical_resource_actual_id,delegation_id,reservation_id,amount_actual,
                     continuation_fencing_token,runtime_host_generation,runtime_fencing_token,
                     evidence_digest,recorded_at
                   ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
                params![
                    actual_id,
                    reservation.delegation_id,
                    reservation.identity.reservation_id,
                    actual.amount_actual,
                    identity.continuation_fencing_token,
                    identity.runtime_host_generation,
                    identity.runtime_fencing_token,
                    actual.evidence_digest,
                    settled_at,
                ],
            )
            .context("failed recording fenced technical resource actual")?;
            let changed = conn.execute(
                "UPDATE execass_technical_resource_reservations SET status='settled',settled_at=?1 WHERE reservation_id=?2 AND status='reserved'",
                params![settled_at, reservation.identity.reservation_id],
            )
            .context("failed settling technical resource reservation")?;
            if changed != 1 {
                bail!("technical resource reservation lost its settle race");
            }
        } else {
            let proven_not_dispatched = effect_state.as_deref() == Some("claimed")
                && matches!(
                    result_status,
                    ContinuationStatus::Waiting | ContinuationStatus::Superseded
                );
            let (next_status, terminal_at) = if proven_not_dispatched {
                ("released", Some(settled_at))
            } else {
                ("reconciliation_required", None)
            };
            let changed = conn.execute(
                "UPDATE execass_technical_resource_reservations SET status=?1,settled_at=?2 WHERE reservation_id=?3 AND status='reserved'",
                params![next_status, terminal_at, reservation.identity.reservation_id],
            )
            .context("failed resolving nonterminal technical resource reservation")?;
            if changed != 1 {
                bail!("technical resource reservation lost its nonterminal transition race");
            }
        }
    }
    Ok(())
}

fn apply_resource_lifecycle_transition(
    conn: &Connection,
    command: &TechnicalResourceLifecycleCommand,
) -> Result<()> {
    let reservations = load_resource_reservations(conn, &command.identity.claim_event_id)?;
    if reservations.is_empty() {
        bail!("technical resource lifecycle operation requires a nonempty reservation set");
    }
    if command.resolution == TechnicalResourceLifecycleResolution::ReconcilePresent
        && reservations.len() != command.technical_resource_actuals.len()
    {
        bail!("present reconciliation requires one actual per reservation");
    }
    let effect = single_claim_effect(&reservations)?
        .context("technical resource lifecycle operation lost its logical effect")?;
    let effect_state: String = conn.query_row(
        "SELECT state FROM execass_logical_effects WHERE delegation_id=?1 AND logical_effect_id=?2",
        params![effect.0, effect.1],
        |row| row.get(0),
    )?;
    let (required_effect_state, next_effect_state) = match command.resolution {
        TechnicalResourceLifecycleResolution::ExpireUndispatched => ("claimed", "failed"),
        TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => {
            ("invoking", "outcome_unknown")
        }
        TechnicalResourceLifecycleResolution::ReconcileAbsent => {
            ("outcome_unknown", "reconciled_absent")
        }
        TechnicalResourceLifecycleResolution::ReconcilePresent => {
            ("outcome_unknown", "reconciled_present")
        }
    };
    if effect_state != required_effect_state {
        bail!("technical resource lifecycle operation lacks the required durable effect proof");
    }
    if command.resolution == TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked {
        let runtime = current_runtime_lease(conn, command.trusted_now)?
            .context("possible-invocation recovery requires a current runtime host")?;
        let continuation = get_continuation(conn, &command.identity.continuation_id)?
            .context("possible-invocation recovery lost its continuation")?;
        let job = load_job_lease(conn, &command.identity.job_id)?
            .context("possible-invocation recovery lost its continuation job")?;
        let exact_runtime_is_current = runtime.generation
            == command.identity.runtime_host_generation
            && runtime.host_instance_id == command.identity.runtime_host_instance_id
            && runtime.fencing_token == command.identity.runtime_fencing_token;
        let exact_job_is_live = command.identity.job_lease_expires_at > command.trusted_now
            && continuation.lease_owner.as_deref() == Some(command.identity.worker_id.as_str())
            && continuation.lease_expires_at == Some(command.identity.job_lease_expires_at)
            && job_lease_matches(
                &job,
                &command.identity.worker_id,
                command.identity.job_lease_expires_at,
                command.trusted_now,
            );
        if exact_runtime_is_current && exact_job_is_live {
            bail!("live possible invocation cannot be recovered as abandoned");
        }
    }
    for reservation in reservations {
        match command.resolution {
            TechnicalResourceLifecycleResolution::ExpireUndispatched => {
                if reservation.status != "reserved" || reservation.expires_at > command.trusted_now
                {
                    bail!("technical reservation cannot expire without elapsed deadline and durable no-dispatch proof");
                }
                let changed = conn.execute(
                    "UPDATE execass_technical_resource_reservations SET status='expired',settled_at=?1 WHERE reservation_id=?2 AND status='reserved' AND expires_at<=?3",
                    params![
                        command.write.occurred_at,
                        reservation.identity.reservation_id,
                        command.trusted_now
                    ],
                )?;
                if changed != 1 {
                    bail!("technical resource reservation lost its expiry race");
                }
            }
            TechnicalResourceLifecycleResolution::ReconcileAbsent => {
                if reservation.status != "reconciliation_required" {
                    bail!("technical reservation release requires durable absent reconciliation");
                }
                let changed = conn.execute(
                    "UPDATE execass_technical_resource_reservations SET status='released',settled_at=?1 WHERE reservation_id=?2 AND status='reconciliation_required'",
                    params![command.write.occurred_at, reservation.identity.reservation_id],
                )?;
                if changed != 1 {
                    bail!("technical resource reservation lost its absent-reconciliation race");
                }
            }
            TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => {
                if reservation.status != "reserved" {
                    bail!("possible-invocation recovery requires a live reservation");
                }
                let changed = conn.execute(
                    "UPDATE execass_technical_resource_reservations SET status='reconciliation_required',settled_at=NULL WHERE reservation_id=?1 AND status='reserved'",
                    params![reservation.identity.reservation_id],
                )?;
                if changed != 1 {
                    bail!("technical resource reservation lost its recovery race");
                }
            }
            TechnicalResourceLifecycleResolution::ReconcilePresent => {
                if reservation.status != "reconciliation_required" {
                    bail!("technical settlement requires durable present reconciliation");
                }
                let actual = command
                    .technical_resource_actuals
                    .iter()
                    .find(|actual| actual.reservation_id == reservation.identity.reservation_id)
                    .context("present reconciliation omitted a reservation actual")?;
                if actual.amount_actual < 0
                    || actual.amount_actual > reservation.identity.amount_reserved
                    || actual.evidence_digest != command.evidence_digest
                    || command
                        .technical_resource_actuals
                        .iter()
                        .filter(|candidate| candidate.reservation_id == actual.reservation_id)
                        .count()
                        != 1
                {
                    bail!("present reconciliation actual is invalid or duplicated");
                }
                let mut digest = Sha256::new();
                digest.update(b"carsinos.execass.technical-resource.reconciled-actual.v1\0");
                digest.update(command.outbox_event.event_id.as_bytes());
                digest.update(b"\0");
                digest.update(reservation.identity.reservation_id.as_bytes());
                let actual_id = format!("technical-actual-{:x}", digest.finalize());
                conn.execute(
                    r#"INSERT INTO execass_technical_resource_actuals(
                         technical_resource_actual_id,delegation_id,reservation_id,amount_actual,
                         continuation_fencing_token,runtime_host_generation,runtime_fencing_token,
                         evidence_digest,recorded_at
                       ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
                    params![
                        actual_id,
                        reservation.delegation_id,
                        reservation.identity.reservation_id,
                        actual.amount_actual,
                        reservation.continuation_fencing_token,
                        reservation.runtime_host_generation,
                        reservation.runtime_fencing_token,
                        command.evidence_digest,
                        command.write.occurred_at,
                    ],
                )?;
                let changed = conn.execute(
                    "UPDATE execass_technical_resource_reservations SET status='settled',settled_at=?1 WHERE reservation_id=?2 AND status='reconciliation_required'",
                    params![command.write.occurred_at, reservation.identity.reservation_id],
                )?;
                if changed != 1 {
                    bail!("technical resource reservation lost its present-reconciliation race");
                }
            }
        }
    }
    transition_provider_attempt_for_resource_lifecycle(conn, command, &effect.1)?;
    let changed = conn.execute(
        "UPDATE execass_logical_effects SET state=?1,updated_at=?2 WHERE delegation_id=?3 AND logical_effect_id=?4 AND state=?5",
        params![
            next_effect_state,
            command.write.occurred_at,
            effect.0,
            effect.1,
            required_effect_state
        ],
    )?;
    if changed != 1 {
        bail!("logical effect lost its technical resource lifecycle transition");
    }
    Ok(())
}

fn transition_provider_attempt_for_resource_lifecycle(
    conn: &Connection,
    command: &TechnicalResourceLifecycleCommand,
    logical_effect_id: &str,
) -> Result<()> {
    let latest = conn
        .query_row(
            r#"SELECT attempt_id,status
               FROM execass_provider_attempts
               WHERE logical_effect_id=?1
               ORDER BY attempt_number DESC
               LIMIT 1"#,
            params![logical_effect_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((attempt_id, status)) = latest else {
        // Internal effects have no provider attempt. Provider-backed effects
        // acquire one through the EA-212 begin boundary before invocation.
        return Ok(());
    };
    let changed = match command.resolution {
        TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => {
            if status != ProviderAttemptStatus::Invoking.as_str() {
                bail!("possible-invocation recovery requires the latest provider attempt to be invoking");
            }
            conn.execute(
                r#"UPDATE execass_provider_attempts
                   SET status='outcome_unknown',provider_response_digest=?1,finished_at=?2
                   WHERE attempt_id=?3 AND status='invoking'"#,
                params![
                    command.evidence_digest,
                    command.write.occurred_at,
                    attempt_id
                ],
            )?
        }
        TechnicalResourceLifecycleResolution::ReconcileAbsent => {
            if status != ProviderAttemptStatus::OutcomeUnknown.as_str() {
                bail!("absent reconciliation requires the latest provider attempt to be outcome_unknown");
            }
            conn.execute(
                "UPDATE execass_provider_attempts SET status='reconciled_absent' WHERE attempt_id=?1 AND status='outcome_unknown'",
                params![attempt_id],
            )?
        }
        TechnicalResourceLifecycleResolution::ReconcilePresent => {
            if status != ProviderAttemptStatus::OutcomeUnknown.as_str() {
                bail!("present reconciliation requires the latest provider attempt to be outcome_unknown");
            }
            conn.execute(
                "UPDATE execass_provider_attempts SET status='reconciled_present' WHERE attempt_id=?1 AND status='outcome_unknown'",
                params![attempt_id],
            )?
        }
        TechnicalResourceLifecycleResolution::ExpireUndispatched => return Ok(()),
    };
    if changed != 1 {
        bail!("provider attempt lost its technical resource lifecycle transition");
    }
    Ok(())
}

fn single_claim_effect(
    reservations: &[TechnicalResourceReservationRecord],
) -> Result<Option<(String, String)>> {
    let Some(first) = reservations.first() else {
        return Ok(None);
    };
    if reservations.iter().any(|reservation| {
        reservation.delegation_id != first.delegation_id
            || reservation.identity.logical_effect_id != first.identity.logical_effect_id
    }) {
        bail!("one continuation claim cannot span multiple logical effects");
    }
    Ok(Some((
        first.delegation_id.clone(),
        first.identity.logical_effect_id.clone(),
    )))
}

pub(super) fn mark_claim_effect_invoking(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
    occurred_at: i64,
) -> Result<bool> {
    let reservations = load_resource_reservations(conn, &identity.claim_event_id)?;
    let Some((delegation_id, logical_effect_id)) = single_claim_effect(&reservations)? else {
        return Ok(true);
    };
    let state: String = conn.query_row(
        "SELECT state FROM execass_logical_effects WHERE delegation_id=?1 AND logical_effect_id=?2",
        params![delegation_id, logical_effect_id],
        |row| row.get(0),
    )?;
    if state == "invoking" {
        return Ok(true);
    }
    if state != "claimed" {
        bail!("pre-dispatch validation lacks durable claimed-effect proof");
    }
    let changed = conn.execute(
        r#"UPDATE execass_logical_effects
           SET state='invoking',updated_at=?1
           WHERE delegation_id=?2 AND logical_effect_id=?3 AND state='claimed'
             AND EXISTS (
               SELECT 1 FROM execass_runtime_host_leases l
               JOIN execass_runtime_host_generations g
                 ON g.generation=l.generation AND g.host_instance_id=l.host_instance_id
               WHERE l.ownership_scope='execass' AND g.ownership_scope='execass'
                 AND l.generation=?4 AND l.host_instance_id=?5 AND l.fencing_token=?6
                 AND g.state_root_generation=?7 AND l.released_at IS NULL
                 AND l.expires_at>?1 AND g.ended_at IS NULL
             )
             AND EXISTS (
               SELECT 1 FROM execass_continuations c
               JOIN jobs j ON j.job_id=c.job_id
               WHERE c.delegation_id=?2 AND c.continuation_id=?8 AND c.action_id=?9
                 AND c.status='executing' AND c.lease_owner=?10
                 AND c.lease_expires_at=?11 AND c.fencing_token=?12
                 AND c.host_generation=?4 AND j.job_id=?13
                 AND j.lease_owner=?10 AND j.lease_expires_at=?11
             )"#,
        params![
            occurred_at,
            delegation_id,
            logical_effect_id,
            identity.runtime_host_generation,
            identity.runtime_host_instance_id,
            identity.runtime_fencing_token,
            identity.state_root_generation,
            identity.continuation_id,
            identity.action_id,
            identity.worker_id,
            identity.job_lease_expires_at,
            identity.continuation_fencing_token,
            identity.job_id,
        ],
    )?;
    Ok(changed == 1)
}

fn transition_claim_effect_on_settle(
    conn: &Connection,
    reservations: &[TechnicalResourceReservationRecord],
    result_status: ContinuationStatus,
    occurred_at: i64,
) -> Result<Option<String>> {
    let Some((delegation_id, logical_effect_id)) = single_claim_effect(reservations)? else {
        return Ok(None);
    };
    let state: String = conn.query_row(
        "SELECT state FROM execass_logical_effects WHERE delegation_id=?1 AND logical_effect_id=?2",
        params![delegation_id, logical_effect_id],
        |row| row.get(0),
    )?;
    let next_state = match result_status {
        ContinuationStatus::Terminal => "succeeded",
        ContinuationStatus::Uncertain => "outcome_unknown",
        ContinuationStatus::Waiting | ContinuationStatus::Superseded if state == "claimed" => {
            "failed"
        }
        ContinuationStatus::Waiting | ContinuationStatus::Superseded => "outcome_unknown",
        _ => bail!("unsupported technical resource settle status"),
    };
    if state != "claimed" && state != "invoking" {
        bail!("technical resource settlement lacks durable claimed-or-invoking effect proof");
    }
    let changed = conn.execute(
        "UPDATE execass_logical_effects SET state=?1,updated_at=?2 WHERE delegation_id=?3 AND logical_effect_id=?4 AND state=?5",
        params![next_state, occurred_at, delegation_id, logical_effect_id, state],
    )?;
    if changed != 1 {
        bail!("logical effect lost its atomic settlement transition");
    }
    Ok(Some(state))
}

fn close_resource_lifecycle_continuation(
    conn: &Connection,
    command: &TechnicalResourceLifecycleCommand,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    result_status: ContinuationStatus,
) -> Result<()> {
    match command.resolution {
        TechnicalResourceLifecycleResolution::ExpireUndispatched => {
            supersede_continuation_and_action(
                conn,
                continuation,
                action,
                command.write.occurred_at,
                &command.identity.worker_id,
                command.identity.job_lease_expires_at,
            )?;
        }
        TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked => {
            if continuation.status != ContinuationStatus::Executing
                || action.status != ContinuationStatus::Executing
            {
                bail!("possible-invocation recovery requires an executing continuation and action");
            }
            let changed = conn.execute(
                r#"UPDATE execass_continuations
                   SET status='uncertain',lease_owner=NULL,lease_expires_at=NULL,
                       updated_at=?1,completed_at=NULL
                   WHERE continuation_id=?2 AND delegation_id=?3 AND job_id=?4
                     AND status='executing' AND lease_owner=?5 AND lease_expires_at=?6
                     AND fencing_token=?7 AND host_generation=?8"#,
                params![
                    command.write.occurred_at,
                    continuation.continuation_id,
                    continuation.delegation_id,
                    command.identity.job_id,
                    command.identity.worker_id,
                    command.identity.job_lease_expires_at,
                    command.identity.continuation_fencing_token,
                    command.identity.runtime_host_generation,
                ],
            )?;
            if changed != 1 {
                bail!("continuation lost its possible-invocation recovery race");
            }
            update_action_status(
                conn,
                action,
                ContinuationStatus::Uncertain,
                command.write.occurred_at,
            )?;
            let job_changed = conn.execute(
                "UPDATE jobs SET enabled=0,next_run_at=NULL,lease_owner=NULL,lease_expires_at=NULL,updated_at=?1 WHERE job_id=?2 AND lease_owner=?3 AND lease_expires_at=?4 AND json_extract(payload_json,'$.mode')='execass.continuation'",
                params![
                    command.write.occurred_at,
                    command.identity.job_id,
                    command.identity.worker_id,
                    command.identity.job_lease_expires_at,
                ],
            )?;
            if job_changed != 1 {
                bail!("continuation job lost its possible-invocation recovery race");
            }
        }
        TechnicalResourceLifecycleResolution::ReconcileAbsent
        | TechnicalResourceLifecycleResolution::ReconcilePresent => {
            if continuation.status != ContinuationStatus::Uncertain
                || action.status != ContinuationStatus::Uncertain
            {
                bail!("technical resource reconciliation requires an uncertain continuation and action");
            }
            let changed = conn.execute(
                "UPDATE execass_continuations SET status=?1,updated_at=?2,completed_at=?2 WHERE continuation_id=?3 AND delegation_id=?4 AND status='uncertain' AND lease_owner IS NULL AND lease_expires_at IS NULL AND fencing_token=?5",
                params![
                    result_status.as_str(),
                    command.write.occurred_at,
                    continuation.continuation_id,
                    continuation.delegation_id,
                    command.identity.continuation_fencing_token,
                ],
            )?;
            if changed != 1 {
                bail!("continuation lost its technical resource reconciliation race");
            }
            update_action_status(conn, action, result_status, command.write.occurred_at)?;
            let job_closed: i64 = conn.query_row(
                "SELECT COUNT(*) FROM jobs WHERE job_id=?1 AND enabled=0 AND next_run_at IS NULL AND lease_owner IS NULL AND lease_expires_at IS NULL AND json_extract(payload_json,'$.mode')='execass.continuation'",
                params![command.identity.job_id],
                |row| row.get(0),
            )?;
            if job_closed != 1 {
                bail!("technical resource reconciliation found a live continuation job");
            }
        }
    }
    Ok(())
}

pub(super) fn derived_runtime_authority(
    runtime: &RuntimeLease,
    policy_revision: i64,
) -> Result<(AuthorityProvenanceRecord, ReceiptActorBinding)> {
    let mut identity_digest = Sha256::new();
    identity_digest.update(b"carsinos.execass.runtime_actor.v1\0");
    identity_digest.update(runtime.generation.to_be_bytes());
    identity_digest.update(b"\0");
    identity_digest.update(runtime.host_instance_id.as_bytes());
    identity_digest.update(b"\0");
    identity_digest.update(runtime.fencing_token.to_be_bytes());
    identity_digest.update(b"\0");
    identity_digest.update(policy_revision.to_be_bytes());
    let identity_hex = format!("{:x}", identity_digest.finalize());
    let authority_provenance_id = format!("runtime-authority-{identity_hex}");
    let credential_identity = format!("execass-runtime-{identity_hex}");
    let normalized_scope_json = serde_json::to_string(&serde_json::json!({
        "ownership_scope": "execass",
        "state_root_generation": runtime.state_root_generation,
        "host_generation": runtime.generation,
        "host_instance_digest": format!("sha256:{:x}", Sha256::digest(runtime.host_instance_id.as_bytes())),
        "runtime_fencing_token": runtime.fencing_token,
    }))
    .context("failed serializing runtime authority scope")?;
    let mut evidence = Sha256::new();
    evidence.update(b"carsinos.execass.runtime_authority_evidence.v1\0");
    evidence.update(normalized_scope_json.as_bytes());
    evidence.update(b"\0");
    evidence.update(runtime.acquired_at.to_be_bytes());
    evidence.update(b"\0");
    evidence.update(runtime.expires_at.to_be_bytes());
    let authority = AuthorityProvenanceRecord {
        authority_provenance_id: authority_provenance_id.clone(),
        actor_type: ActorType::Runtime,
        credential_identity: credential_identity.clone(),
        authenticated_ingress: "runtime-host-lease".into(),
        channel_assurance: "local-runtime-fence".into(),
        source_correlation_id: format!(
            "runtime-generation-{}-fence-{}-policy-{}",
            runtime.generation, runtime.fencing_token, policy_revision
        ),
        source_message_id: None,
        authority_kind: AuthorityKind::RuntimeSafetyState,
        normalized_scope_json,
        policy_revision,
        bound_decision_id: None,
        bound_decision_revision: None,
        bound_manifest_digest: None,
        bound_challenge_nonce_digest: None,
        evidence_digest: format!("{:x}", evidence.finalize()),
        created_at: runtime.acquired_at,
        expires_at: Some(runtime.expires_at),
    };
    let actor = ReceiptActorBinding {
        actor_type: ActorType::Runtime,
        actor_identity: super::redaction::SafeText::new(&credential_identity, &[])?,
        authority_provenance_id,
    };
    Ok((authority, actor))
}

pub(super) fn ensure_runtime_receipt_actor(
    conn: &Connection,
    receipt: &AppendReceiptCommand,
    authority: &AuthorityProvenanceRecord,
    actor: &ReceiptActorBinding,
) -> Result<()> {
    if receipt.actor != *actor {
        bail!("continuation receipt actor is not the server-derived runtime authority");
    }
    match get_authority(conn, &authority.authority_provenance_id)? {
        Some(existing) if existing == *authority => Ok(()),
        Some(_) => bail!("runtime authority identity collision"),
        None => insert_authority(conn, authority),
    }
}

fn ensure_claim_runtime_receipt_actor(
    conn: &Connection,
    receipt: &AppendReceiptCommand,
    identity: &ContinuationClaimIdentity,
) -> Result<()> {
    if receipt.actor.actor_type != ActorType::Runtime
        || receipt.actor.authority_provenance_id != identity.runtime_authority_provenance_id
        || receipt.actor.actor_identity.as_str() != identity.runtime_actor_identity
    {
        bail!("settle receipt actor is not the exact claimed runtime authority");
    }
    let authority = get_authority(conn, &identity.runtime_authority_provenance_id)?
        .context("claimed runtime authority no longer exists")?;
    if authority.actor_type != ActorType::Runtime
        || authority.authority_kind != AuthorityKind::RuntimeSafetyState
        || authority.credential_identity != identity.runtime_actor_identity
    {
        bail!("claimed runtime authority binding changed");
    }
    Ok(())
}

pub(super) fn objective_drift(
    conn: &Connection,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    expected_action_status: ContinuationStatus,
) -> Result<Option<ContinuationStaleReason>> {
    let Some(delegation) = super::rows::get_delegation(conn, &continuation.delegation_id)? else {
        return Ok(Some(ContinuationStaleReason::NotFound));
    };
    if delegation.current_plan_revision != Some(continuation.target_plan_revision) {
        return Ok(Some(ContinuationStaleReason::PlanRevisionDrift));
    }
    if delegation.current_criteria_revision.is_none() {
        return Ok(Some(ContinuationStaleReason::MissingCurrentCriteria));
    }
    let Some(plan) = super::rows::get_plan_by_revision(
        conn,
        &continuation.delegation_id,
        continuation.target_plan_revision,
    )?
    else {
        return Ok(Some(ContinuationStaleReason::PlanRevisionDrift));
    };
    if plan.policy_revision != delegation.policy_revision {
        return Ok(Some(ContinuationStaleReason::PolicyRevisionDrift));
    }
    if delegation.run_control != RunControlState::Running {
        return Ok(Some(ContinuationStaleReason::DelegationRunControlDrift));
    }
    if delegation.stop_epoch != continuation.stop_epoch {
        return Ok(Some(ContinuationStaleReason::DelegationStopEpochDrift));
    }
    if delegation.state_revision != continuation.target_delegation_revision {
        return Ok(Some(ContinuationStaleReason::DelegationRevisionDrift));
    }
    let (engaged, global_stop_epoch): (i64, i64) = conn.query_row(
        "SELECT engaged,global_stop_epoch FROM execass_global_runtime_control WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if engaged != 0 {
        return Ok(Some(ContinuationStaleReason::GlobalStopEngaged));
    }
    if global_stop_epoch != continuation.global_stop_epoch {
        return Ok(Some(ContinuationStaleReason::GlobalStopEpochDrift));
    }
    if action.delegation_id != continuation.delegation_id
        || action.action_id != continuation.action_id
        || action.target_delegation_revision != continuation.target_delegation_revision
        || action.target_plan_revision != continuation.target_plan_revision
        || action.stop_epoch != continuation.stop_epoch
        || action.branch_kind != continuation.branch_kind
        || action.status != expected_action_status
    {
        return Ok(Some(ContinuationStaleReason::ActionStateDrift));
    }
    Ok(None)
}

pub(super) fn validate_claim_identity_live(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
    trusted_now: i64,
    operation: Operation,
) -> Result<std::result::Result<(ContinuationRecord, ActionBranchRecord), ContinuationStaleReason>>
{
    if !validate_claim_provenance(conn, identity)? {
        return Ok(Err(ContinuationStaleReason::ClaimIdentityMismatch));
    }
    if let Some(reason) = validate_claim_reservations_live(conn, identity, trusted_now)? {
        return Ok(Err(reason));
    }
    if identity.job_lease_expires_at <= trusted_now {
        return Ok(Err(ContinuationStaleReason::JobLeaseLostOrExpired));
    }
    let Some(job) = load_job_lease(conn, &identity.job_id)? else {
        return Ok(Err(ContinuationStaleReason::JobBindingMismatch));
    };
    if !job_lease_matches(
        &job,
        &identity.worker_id,
        identity.job_lease_expires_at,
        trusted_now,
    ) {
        return Ok(Err(ContinuationStaleReason::JobLeaseLostOrExpired));
    }
    let Some(continuation) = get_continuation(conn, &identity.continuation_id)? else {
        return Ok(Err(ContinuationStaleReason::NotFound));
    };
    if continuation.delegation_id != identity.delegation_id
        || continuation.action_id != identity.action_id
        || continuation.job_id.as_deref() != Some(identity.job_id.as_str())
        || !job_payload_matches(&job.payload_json, &continuation)?
    {
        return Ok(Err(ContinuationStaleReason::JobPayloadMismatch));
    }
    if continuation.status != ContinuationStatus::Executing {
        return Ok(Err(match operation {
            Operation::Dispatch | Operation::Settle => {
                ContinuationStaleReason::ContinuationNotExecuting
            }
        }));
    }
    if continuation.lease_owner.as_deref() != Some(identity.worker_id.as_str())
        || continuation.lease_expires_at != Some(identity.job_lease_expires_at)
        || continuation.fencing_token != identity.continuation_fencing_token
        || continuation.host_generation != identity.runtime_host_generation
        || continuation.global_stop_epoch != identity.global_stop_epoch
    {
        return Ok(Err(ContinuationStaleReason::ClaimIdentityMismatch));
    }
    if !exact_runtime_lease(conn, identity, trusted_now)? {
        return Ok(Err(ContinuationStaleReason::RuntimeHostLeaseLostOrExpired));
    }
    let Some(delegation) = super::rows::get_delegation(conn, &identity.delegation_id)? else {
        return Ok(Err(ContinuationStaleReason::NotFound));
    };
    if delegation.policy_revision != identity.policy_revision {
        return Ok(Err(ContinuationStaleReason::PolicyRevisionDrift));
    }
    if technical_quota_policy_digest(
        delegation.policy_revision,
        &delegation.effective_authority_json,
    ) != identity.technical_quota_policy_digest
    {
        return Ok(Err(ContinuationStaleReason::TechnicalQuotaPolicyDrift));
    }
    let Some(action) = load_action(conn, &identity.action_id)? else {
        return Ok(Err(ContinuationStaleReason::ActionStateDrift));
    };
    Ok(Ok((continuation, action)))
}

fn validate_claim_reservations_live(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
    trusted_now: i64,
) -> Result<Option<ContinuationStaleReason>> {
    let reservations = load_resource_reservations(conn, &identity.claim_event_id)?;
    let live_identities = reservations
        .iter()
        .map(|reservation| reservation.identity.clone())
        .collect::<Vec<_>>();
    let Some((stored_identity, status, stored_identities)) =
        load_operation_history(conn, &identity.claim_event_id, "claim")?
    else {
        return Ok(Some(
            ContinuationStaleReason::TechnicalReservationMissingOrChanged,
        ));
    };
    if stored_identity != *identity
        || status != ContinuationStatus::Executing
        || stored_identities != live_identities
        || resource_identity_set_digest(&live_identities)?
            != identity.technical_resource_reservation_set_digest
    {
        return Ok(Some(
            ContinuationStaleReason::TechnicalReservationMissingOrChanged,
        ));
    }
    if identity.technical_quota_snapshot_id.is_none() != reservations.is_empty() {
        return Ok(Some(ContinuationStaleReason::TechnicalQuotaSnapshotDrift));
    }
    for reservation in reservations {
        if Some(reservation.identity.quota_snapshot_id.as_str())
            != identity.technical_quota_snapshot_id.as_deref()
            || reservation.delegation_id != identity.delegation_id
            || reservation.continuation_id != identity.continuation_id
            || reservation.claim_receipt_id != identity.claim_receipt_id
            || reservation.continuation_fencing_token != identity.continuation_fencing_token
            || reservation.runtime_host_generation != identity.runtime_host_generation
            || reservation.runtime_fencing_token != identity.runtime_fencing_token
            || reservation.status != "reserved"
        {
            return Ok(Some(
                ContinuationStaleReason::TechnicalReservationMissingOrChanged,
            ));
        }
        if reservation.expires_at <= trusted_now {
            return Ok(Some(ContinuationStaleReason::TechnicalReservationExpired));
        }
    }
    Ok(None)
}

pub(super) fn validate_claim_provenance(
    conn: &Connection,
    identity: &ContinuationClaimIdentity,
) -> Result<bool> {
    let Some((stored_identity, result_status, stored_reservations)) =
        load_operation_history(conn, &identity.claim_event_id, "claim")?
    else {
        return Ok(false);
    };
    if stored_identity != *identity
        || result_status != ContinuationStatus::Executing
        || resource_identity_set_digest(&stored_reservations)?
            != identity.technical_resource_reservation_set_digest
    {
        return Ok(false);
    }
    conn.query_row(
        r#"SELECT 1 FROM execass_receipts r
           JOIN execass_runtime_host_generations g
             ON g.generation=r.runtime_host_generation
            AND g.host_instance_id=r.runtime_host_instance_id
           WHERE r.receipt_id=?1 AND r.causation_event_id=?2
             AND r.receipt_kind='continuation'
             AND r.subject_kind='continuation' AND r.subject_id=?3
             AND r.actor_type='runtime' AND r.actor_identity=?4
             AND r.actor_authority_provenance_id=?5
             AND g.state_root_generation=?6
             AND r.runtime_host_generation=?7
             AND r.runtime_host_instance_id=?8
             AND r.runtime_fencing_token=?9"#,
        params![
            identity.claim_receipt_id,
            identity.claim_event_id,
            identity.continuation_id,
            identity.runtime_actor_identity,
            identity.runtime_authority_provenance_id,
            identity.state_root_generation,
            identity.runtime_host_generation,
            identity.runtime_host_instance_id,
            identity.runtime_fencing_token,
        ],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .context("failed validating immutable continuation claim receipt provenance")
}

pub(super) fn load_action(
    conn: &Connection,
    action_id: &str,
) -> Result<Option<ActionBranchRecord>> {
    conn.query_row(
        r#"SELECT action_id,delegation_id,action_revision,target_delegation_revision,
          target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
          FROM execass_action_branches WHERE action_id=?1"#,
        params![action_id],
        |row| {
            Ok(ActionBranchRecord {
                action_id: row.get(0)?,
                delegation_id: row.get(1)?,
                action_revision: row.get(2)?,
                target_delegation_revision: row.get(3)?,
                target_plan_revision: row.get(4)?,
                stop_epoch: row.get(5)?,
                branch_kind: row.get(6)?,
                status: row.get(7)?,
                action_summary: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                terminal_at: row.get(11)?,
            })
        },
    )
    .optional()
    .context("failed reading ExecAss action branch")
}

pub(super) fn update_action_status(
    conn: &Connection,
    action: &ActionBranchRecord,
    status: ContinuationStatus,
    occurred_at: i64,
) -> Result<()> {
    let terminal_at = matches!(
        status,
        ContinuationStatus::Terminal | ContinuationStatus::Superseded
    )
    .then_some(occurred_at);
    let changed = conn.execute(
        "UPDATE execass_action_branches SET status=?1, updated_at=?2, terminal_at=?3 WHERE action_id=?4 AND delegation_id=?5 AND status=?6",
        params![
            status.as_str(),
            occurred_at,
            terminal_at,
            action.action_id,
            action.delegation_id,
            action.status.as_str(),
        ],
    )?;
    if changed != 1 {
        bail!("action branch lost its continuation status race");
    }
    Ok(())
}

fn supersede_with_receipt(
    conn: &Connection,
    command: &ContinuationClaimCommand,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    reason: ContinuationStaleReason,
) -> Result<SupersedeDraft> {
    validate_claim_receipt_identity(command, continuation)?;
    insert_outbox(conn, &command.outbox_event)?;
    supersede_continuation_and_action(
        conn,
        continuation,
        action,
        command.write.occurred_at,
        command.worker_id.as_str(),
        command.job_lease_expires_at,
    )?;
    let continuation = get_continuation(conn, &continuation.continuation_id)?
        .context("superseded continuation disappeared")?;
    let action = load_action(conn, &action.action_id)?.context("superseded action disappeared")?;
    let outbox_event = get_outbox(conn, &command.outbox_event.event_id)?
        .context("supersede outbox disappeared")?;
    Ok(SupersedeDraft {
        reason,
        continuation,
        action,
        outbox_event,
    })
}

fn supersede_with_receipt_from_settle(
    conn: &Connection,
    command: &ContinuationSettleCommand,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    reason: ContinuationStaleReason,
) -> Result<SupersedeDraft> {
    validate_settle_receipt_identity(command, continuation)?;
    let actual_set_digest =
        technical_resource_actual_set_digest(&command.technical_resource_actuals)?;
    let resource_set_json = claim_resource_set_json(conn, &command.identity)?;
    insert_outbox(conn, &command.outbox_event)?;
    insert_operation_history(
        conn,
        OperationHistoryWrite {
            event_id: &command.outbox_event.event_id,
            operation: "settle",
            result_status: ContinuationStatus::Superseded,
            identity: &command.identity,
            resource_set_json: &resource_set_json,
            resource_evidence_digest: Some(&actual_set_digest),
            recorded_at: command.write.occurred_at,
        },
    )?;
    transition_resource_reservations_on_settle(
        conn,
        &command.identity,
        ContinuationStatus::Superseded,
        &[],
        &command.outbox_event.event_id,
        command.write.occurred_at,
    )?;
    supersede_continuation_and_action(
        conn,
        continuation,
        action,
        command.write.occurred_at,
        &command.identity.worker_id,
        command.identity.job_lease_expires_at,
    )?;
    let continuation = get_continuation(conn, &continuation.continuation_id)?
        .context("superseded continuation disappeared")?;
    let action = load_action(conn, &action.action_id)?.context("superseded action disappeared")?;
    let outbox_event = get_outbox(conn, &command.outbox_event.event_id)?
        .context("supersede outbox disappeared")?;
    Ok(SupersedeDraft {
        reason,
        continuation,
        action,
        outbox_event,
    })
}

fn supersede_continuation_and_action(
    conn: &Connection,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    occurred_at: i64,
    worker_id: &str,
    job_lease_expires_at: i64,
) -> Result<()> {
    conn.execute(
        r#"UPDATE execass_continuations
           SET status='superseded', lease_owner=NULL, lease_expires_at=NULL,
               updated_at=?1, completed_at=?1
           WHERE continuation_id=?2 AND delegation_id=?3
             AND status IN ('runnable','executing')"#,
        params![
            occurred_at,
            continuation.continuation_id,
            continuation.delegation_id
        ],
    )?;
    update_action_status(conn, action, ContinuationStatus::Superseded, occurred_at)?;
    let job_changed = conn.execute(
        "UPDATE jobs SET enabled=0, next_run_at=NULL, lease_owner=NULL, lease_expires_at=NULL, updated_at=?1 WHERE job_id=?2 AND lease_owner=?3 AND lease_expires_at=?4 AND json_extract(payload_json,'$.mode')='execass.continuation'",
        params![occurred_at, continuation.job_id, worker_id, job_lease_expires_at],
    )?;
    if job_changed != 1 {
        bail!("continuation job lost its supersede lease race");
    }
    Ok(())
}

fn identity_from_parts(
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    command: &ContinuationClaimCommand,
    runtime: &RuntimeLease,
    runtime_actor: &ReceiptActorBinding,
    delegation: &DelegationRecord,
    resource_bundle: &ResourceBundleDraft,
) -> Result<ContinuationClaimIdentity> {
    let continuation_fencing_token = continuation
        .fencing_token
        .checked_add(1)
        .context("continuation fencing token overflow")?;
    Ok(ContinuationClaimIdentity {
        claim_event_id: command.outbox_event.event_id.clone(),
        claim_receipt_id: command.receipt.receipt_id.clone(),
        continuation_id: continuation.continuation_id.clone(),
        delegation_id: continuation.delegation_id.clone(),
        action_id: action.action_id.clone(),
        job_id: command.job_id.clone(),
        worker_id: command.worker_id.clone(),
        job_lease_expires_at: command.job_lease_expires_at,
        continuation_fencing_token,
        runtime_host_generation: runtime.generation,
        runtime_host_instance_id: runtime.host_instance_id.clone(),
        runtime_fencing_token: runtime.fencing_token,
        state_root_generation: runtime.state_root_generation,
        runtime_authority_provenance_id: runtime_actor.authority_provenance_id.clone(),
        runtime_actor_identity: runtime_actor.actor_identity.as_str().to_string(),
        policy_revision: delegation.policy_revision,
        global_stop_epoch: continuation.global_stop_epoch,
        technical_quota_policy_digest: technical_quota_policy_digest(
            delegation.policy_revision,
            &delegation.effective_authority_json,
        ),
        technical_quota_snapshot_id: resource_bundle.quota_snapshot_id.clone(),
        technical_resource_reservation_set_digest: resource_bundle.digest.clone(),
    })
}

pub(super) fn insert_operation_history(
    conn: &Connection,
    write: OperationHistoryWrite<'_>,
) -> Result<()> {
    conn.execute(
        r#"INSERT INTO execass_continuation_operation_history (
             event_id,claim_event_id,claim_receipt_id,operation,result_status,continuation_id,delegation_id,action_id,
             job_id,worker_id,job_lease_expires_at,continuation_fencing_token,
             runtime_host_generation,runtime_host_instance_id,runtime_fencing_token,state_root_generation,
             runtime_authority_provenance_id,runtime_actor_identity,policy_revision,
             global_stop_epoch,technical_quota_policy_digest,technical_quota_snapshot_id,
             technical_resource_reservation_set_json,technical_resource_reservation_set_digest,
             technical_resource_evidence_digest,recorded_at
           ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)"#,
        params![
            write.event_id,
            write.identity.claim_event_id,
            write.identity.claim_receipt_id,
            write.operation,
            write.result_status.as_str(),
            write.identity.continuation_id,
            write.identity.delegation_id,
            write.identity.action_id,
            write.identity.job_id,
            write.identity.worker_id,
            write.identity.job_lease_expires_at,
            write.identity.continuation_fencing_token,
            write.identity.runtime_host_generation,
            write.identity.runtime_host_instance_id,
            write.identity.runtime_fencing_token,
            write.identity.state_root_generation,
            write.identity.runtime_authority_provenance_id,
            write.identity.runtime_actor_identity,
            write.identity.policy_revision,
            write.identity.global_stop_epoch,
            write.identity.technical_quota_policy_digest,
            write.identity.technical_quota_snapshot_id,
            write.resource_set_json,
            write.identity.technical_resource_reservation_set_digest,
            write.resource_evidence_digest,
            write.recorded_at,
        ],
    )
    .context("failed inserting immutable continuation operation history")?;
    Ok(())
}

pub(super) fn load_operation_history(
    conn: &Connection,
    event_id: &str,
    operation: &str,
) -> Result<
    Option<(
        ContinuationClaimIdentity,
        ContinuationStatus,
        Vec<TechnicalResourceReservationIdentity>,
    )>,
> {
    let row = conn.query_row(
        r#"SELECT claim_event_id,claim_receipt_id,continuation_id,delegation_id,action_id,job_id,worker_id,
                  job_lease_expires_at,continuation_fencing_token,
                  runtime_host_generation,runtime_host_instance_id,runtime_fencing_token,state_root_generation,
                  runtime_authority_provenance_id,runtime_actor_identity,policy_revision,
                  global_stop_epoch,technical_quota_policy_digest,technical_quota_snapshot_id,
                  technical_resource_reservation_set_json,technical_resource_reservation_set_digest,
                  result_status
           FROM execass_continuation_operation_history
           WHERE event_id=?1 AND operation=?2"#,
        params![event_id, operation],
        |row| {
            Ok((
                ContinuationClaimIdentity {
                    claim_event_id: row.get(0)?,
                    claim_receipt_id: row.get(1)?,
                    continuation_id: row.get(2)?,
                    delegation_id: row.get(3)?,
                    action_id: row.get(4)?,
                    job_id: row.get(5)?,
                    worker_id: row.get(6)?,
                    job_lease_expires_at: row.get(7)?,
                    continuation_fencing_token: row.get(8)?,
                    runtime_host_generation: row.get(9)?,
                    runtime_host_instance_id: row.get(10)?,
                    runtime_fencing_token: row.get(11)?,
                    state_root_generation: row.get(12)?,
                    runtime_authority_provenance_id: row.get(13)?,
                    runtime_actor_identity: row.get(14)?,
                    policy_revision: row.get(15)?,
                    global_stop_epoch: row.get(16)?,
                    technical_quota_policy_digest: row.get(17)?,
                    technical_quota_snapshot_id: row.get(18)?,
                    technical_resource_reservation_set_digest: row.get(20)?,
                },
                row.get(21)?,
                row.get::<_, String>(19)?,
            ))
        },
    )
    .optional()
    .context("failed reading immutable continuation operation history")?;
    row.map(|(identity, status, canonical_json)| {
        let reservations = serde_json::from_str(&canonical_json)
            .context("immutable continuation resource set is invalid")?;
        Ok((identity, status, reservations))
    })
    .transpose()
}

fn load_claim_replay(
    conn: &Connection,
    command: &ContinuationClaimCommand,
) -> Result<Option<ReplayDraft>> {
    let Some((identity, result_status, technical_resource_reservations)) =
        load_operation_history(conn, &command.outbox_event.event_id, "claim")?
    else {
        return Ok(None);
    };
    if result_status != ContinuationStatus::Executing
        || identity.continuation_id != command.continuation_id
        || identity.job_id != command.job_id
        || identity.worker_id != command.worker_id
        || identity.job_lease_expires_at != command.job_lease_expires_at
        || identity.claim_event_id != command.outbox_event.event_id
        || identity.claim_receipt_id != command.receipt.receipt_id
        || identity.state_root_generation != command.receipt.state_root_generation
        || identity.runtime_host_generation != command.receipt.runtime.host_generation
        || identity.runtime_host_instance_id != command.receipt.runtime.host_instance_id
        || identity.runtime_fencing_token != command.receipt.runtime.fencing_token
        || identity.runtime_authority_provenance_id != command.receipt.actor.authority_provenance_id
        || identity.runtime_actor_identity != command.receipt.actor.actor_identity.as_str()
    {
        return Ok(None);
    }
    let outbox_event = get_outbox(conn, &command.outbox_event.event_id)?;
    let Some(outbox_event) = outbox_event else {
        return Ok(None);
    };
    if outbox_event.event != command.outbox_event {
        return Ok(None);
    }
    let receipt_id = conn
        .query_row(
            "SELECT receipt_id FROM execass_receipts WHERE causation_event_id=?1",
            params![command.outbox_event.event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if receipt_id.as_deref() != Some(command.receipt.receipt_id.as_str()) {
        return Ok(None);
    }
    Ok(Some(ReplayDraft {
        identity,
        result_status,
        outbox_event,
        technical_resource_reservations,
    }))
}

fn load_settle_replay(
    conn: &Connection,
    command: &ContinuationSettleCommand,
) -> Result<Option<ReplayDraft>> {
    let Some((identity, result_status, technical_resource_reservations)) =
        load_operation_history(conn, &command.outbox_event.event_id, "settle")?
    else {
        return Ok(None);
    };
    if identity != command.identity || result_status != command.result_status {
        return Ok(None);
    }
    let stored_actual_set_digest: Option<String> = conn.query_row(
        "SELECT technical_resource_evidence_digest FROM execass_continuation_operation_history WHERE event_id=?1 AND operation='settle'",
        params![command.outbox_event.event_id],
        |row| row.get(0),
    )?;
    if stored_actual_set_digest.as_deref()
        != Some(technical_resource_actual_set_digest(&command.technical_resource_actuals)?.as_str())
    {
        return Ok(None);
    }
    let outbox_event = get_outbox(conn, &command.outbox_event.event_id)?;
    let Some(outbox_event) = outbox_event else {
        return Ok(None);
    };
    if outbox_event.event != command.outbox_event {
        return Ok(None);
    }
    let receipt_id = conn
        .query_row(
            "SELECT receipt_id FROM execass_receipts WHERE causation_event_id=?1",
            params![command.outbox_event.event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if receipt_id.as_deref() != Some(command.receipt.receipt_id.as_str()) {
        return Ok(None);
    }
    Ok(Some(ReplayDraft {
        identity,
        result_status,
        outbox_event,
        technical_resource_reservations,
    }))
}

fn claim_record(draft: ClaimDraft, receipt: ReceiptRecord) -> ContinuationClaimRecord {
    ContinuationClaimRecord {
        continuation: draft.continuation,
        action: draft.action,
        identity: draft.identity,
        outbox_event: draft.outbox_event,
        receipt,
        technical_resource_reservations: draft.technical_resource_reservations,
    }
}

fn settle_record(draft: SettleDraft, receipt: ReceiptRecord) -> ContinuationSettleRecord {
    ContinuationSettleRecord {
        continuation: draft.continuation,
        action: draft.action,
        identity: draft.identity,
        outbox_event: draft.outbox_event,
        receipt,
        technical_resource_reservations: draft.technical_resource_reservations,
    }
}

fn replay_record(draft: ReplayDraft, receipt: ReceiptRecord) -> ContinuationOperationReplayRecord {
    ContinuationOperationReplayRecord {
        identity: draft.identity,
        result_status: draft.result_status,
        outbox_event: draft.outbox_event,
        receipt,
        technical_resource_reservations: draft.technical_resource_reservations,
    }
}

fn resource_lifecycle_record(
    draft: ResourceLifecycleDraft,
    receipt: ReceiptRecord,
) -> TechnicalResourceLifecycleRecord {
    TechnicalResourceLifecycleRecord {
        identity: draft.identity,
        resolution: draft.resolution,
        outbox_event: draft.outbox_event,
        receipt,
        technical_resource_reservations: draft.technical_resource_reservations,
    }
}

fn supersede_record(draft: SupersedeDraft, receipt: ReceiptRecord) -> ContinuationSupersededRecord {
    ContinuationSupersededRecord {
        reason: draft.reason,
        continuation: draft.continuation,
        action: draft.action,
        outbox_event: draft.outbox_event,
        receipt,
    }
}
