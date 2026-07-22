//! Canonical gateway-owned ExecAss runtime-host activation.

use super::confirmation_custody::ConfirmationAuthorityIdentity;
use super::receipt::{RuntimeAtomicReceiptMutation, RuntimeReceiptCommand};
use super::receipt_integrity::ReceiptIntegrityStore;
use super::redaction::ReceiptRedactor;
use super::rows::insert_outbox;
use super::store::{immediate_transaction, ExecAssStore};
use super::types::{
    ActorType, AttentionStatus, NewOutboxEvent, OutboxEventName, ReceiptActorBinding,
    ReceiptRuntimeBinding, RuntimeActualState, RuntimeDesiredMode, RuntimeHostLeaseRecord,
    RuntimeHostTransition, RuntimeHostTransitionOutcome, RuntimeHostTransitionReason,
    RuntimePausedAttentionRecord,
};
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};

impl ExecAssStore {
    /// Activate the one runtime host after the gateway has acquired its
    /// process-wide scheduler lock. A new activation irreversibly ends the
    /// prior host generation and advances both generation and fencing token.
    pub fn activate_runtime_host(
        &self,
        authority: &ConfirmationAuthorityIdentity,
        host_instance_id: &str,
        trusted_now: i64,
    ) -> Result<RuntimeHostLeaseRecord> {
        require_text("host_instance_id", host_instance_id)?;
        if trusted_now <= 0 {
            bail!("runtime host activation requires a positive trusted clock");
        }
        if authority.canonical_root_identity() != self.root_identity {
            bail!("runtime host authority belongs to a different state root");
        }
        let state_root_generation = i64::try_from(authority.state_root_generation())
            .context("runtime host state-root generation exceeds storage range")?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;

        let current = tx
            .query_row(
                r#"SELECT l.lease_id,g.state_root_generation,l.generation,l.host_instance_id,
                          l.fencing_token,l.acquired_at,l.expires_at,
                          g.installation_identity,g.os_user_identity_digest
                   FROM execass_runtime_host_leases l
                   JOIN execass_runtime_host_generations g
                     ON g.generation=l.generation AND g.host_instance_id=l.host_instance_id
                   WHERE l.ownership_scope='execass' AND g.ownership_scope='execass'
                     AND l.released_at IS NULL AND g.ended_at IS NULL"#,
                [],
                |row| {
                    Ok((
                        RuntimeHostLeaseRecord {
                            lease_id: row.get(0)?,
                            state_root_generation: row.get(1)?,
                            generation: row.get(2)?,
                            host_instance_id: row.get(3)?,
                            fencing_token: row.get(4)?,
                            acquired_at: row.get(5)?,
                            expires_at: row.get(6)?,
                        },
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .context("failed reading current runtime host activation")?;

        if let Some((record, installation_identity, os_user_identity_digest)) = &current {
            if record.host_instance_id == host_instance_id {
                if record.state_root_generation != state_root_generation
                    || installation_identity != authority.installation_identity()
                    || os_user_identity_digest != authority.os_user_identity_digest()
                    || record.expires_at <= trusted_now
                {
                    bail!("current runtime host activation conflicts with its authority identity");
                }
                ensure_runtime_host_state(&tx, record, RuntimeActualState::Starting, trusted_now)?;
                tx.commit()
                    .context("failed replaying runtime host activation")?;
                return Ok(record.clone());
            }
            tx.execute(
                "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE lease_id=?2 AND released_at IS NULL",
                params![trusted_now, record.lease_id],
            )?;
            tx.execute(
                "UPDATE execass_runtime_host_generations SET ended_at=?1,end_reason='gateway_lock_takeover' WHERE generation=?2 AND ended_at IS NULL",
                params![trusted_now, record.generation],
            )?;
        }

        let (generation, fencing_token) = tx.query_row(
            r#"SELECT COALESCE(MAX(generation),0)+1,
                      COALESCE((SELECT MAX(fencing_token) FROM execass_runtime_host_leases),0)+1
               FROM execass_runtime_host_generations"#,
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let expires_at = i64::MAX;
        let lease_id = runtime_lease_id(host_instance_id, generation, fencing_token);
        tx.execute(
            r#"INSERT INTO execass_runtime_host_generations(
                 generation,ownership_scope,state_root_generation,installation_identity,
                 os_user_identity_digest,host_instance_id,started_at
               ) VALUES(?1,'execass',?2,?3,?4,?5,?6)"#,
            params![
                generation,
                state_root_generation,
                authority.installation_identity(),
                authority.os_user_identity_digest(),
                host_instance_id,
                trusted_now,
            ],
        )?;
        tx.execute(
            r#"INSERT INTO execass_runtime_host_leases(
                 lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
               ) VALUES(?1,'execass',?2,?3,?4,?5,?6)"#,
            params![
                lease_id,
                generation,
                host_instance_id,
                fencing_token,
                trusted_now,
                expires_at,
            ],
        )?;
        let record = RuntimeHostLeaseRecord {
            lease_id,
            state_root_generation,
            generation,
            host_instance_id: host_instance_id.to_owned(),
            fencing_token,
            acquired_at: trusted_now,
            expires_at,
        };
        ensure_runtime_host_state(&tx, &record, RuntimeActualState::Starting, trusted_now)?;
        tx.commit().context("failed activating runtime host")?;
        Ok(record)
    }

    /// Activates a successor and, when the exact live predecessor did not
    /// complete an orderly stop, atomically records the predecessor fault,
    /// canonical runtime attention, outbox event, receipt, and global anchor.
    pub fn activate_runtime_host_with_recovery(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        authority: &ConfirmationAuthorityIdentity,
        host_instance_id: &str,
        trusted_now: i64,
    ) -> Result<RuntimeHostLeaseRecord> {
        require_text("host_instance_id", host_instance_id)?;
        if trusted_now <= 0 {
            bail!("runtime host activation requires a positive trusted clock");
        }
        if authority.canonical_root_identity() != self.root_identity {
            bail!("runtime host authority belongs to a different state root");
        }
        let state_root_generation = i64::try_from(authority.state_root_generation())
            .context("runtime host state-root generation exceeds storage range")?;
        let receipt_key = integrity.current_append_key()?;
        let runtime_actor_identity = redactor.text("execass-global-control")?;
        let receipt_summary = redactor
            .summary("ExecAss runtime stopped unexpectedly; recovery attention was recorded.")?;

        self.mutate_with_runtime_atomic_receipt(
            integrity,
            redactor,
            |tx, global_count, global_head| {
                let current = tx
                    .query_row(
                        r#"SELECT l.lease_id,g.state_root_generation,l.generation,l.host_instance_id,
                                  l.fencing_token,l.acquired_at,l.expires_at,
                                  g.installation_identity,g.os_user_identity_digest
                           FROM execass_runtime_host_leases l
                           JOIN execass_runtime_host_generations g
                             ON g.generation=l.generation AND g.host_instance_id=l.host_instance_id
                           WHERE l.ownership_scope='execass' AND g.ownership_scope='execass'
                             AND l.released_at IS NULL AND g.ended_at IS NULL"#,
                        [],
                        |row| {
                            Ok((
                                RuntimeHostLeaseRecord {
                                    lease_id: row.get(0)?,
                                    state_root_generation: row.get(1)?,
                                    generation: row.get(2)?,
                                    host_instance_id: row.get(3)?,
                                    fencing_token: row.get(4)?,
                                    acquired_at: row.get(5)?,
                                    expires_at: row.get(6)?,
                                },
                                row.get::<_, String>(7)?,
                                row.get::<_, String>(8)?,
                            ))
                        },
                    )
                    .optional()?;
                if let Some((record, installation_identity, os_user_identity_digest)) = &current {
                    if record.host_instance_id == host_instance_id {
                        if record.state_root_generation != state_root_generation
                            || installation_identity != authority.installation_identity()
                            || os_user_identity_digest != authority.os_user_identity_digest()
                            || record.expires_at <= trusted_now
                        {
                            bail!("current runtime host activation conflicts with its authority identity");
                        }
                        ensure_runtime_host_state(
                            tx,
                            record,
                            RuntimeActualState::Starting,
                            trusted_now,
                        )?;
                        return Ok(RuntimeAtomicReceiptMutation::NoAppend(record.clone()));
                    }
                }

                let predecessor = current.as_ref().map(|value| value.0.clone());
                let predecessor_state = predecessor
                    .as_ref()
                    .map(|record| {
                        load_runtime_host_state(tx, record)?
                            .context("live predecessor runtime state is missing")
                    })
                    .transpose()?;
                let interruption_end_reason = if let (Some(predecessor), Some(state)) =
                    (predecessor.as_ref(), predecessor_state.as_ref())
                {
                    let end_reason = match state.actual_state {
                        RuntimeActualState::Faulted => "gateway_fault_takeover",
                        RuntimeActualState::Draining => "gateway_drain_interrupted_takeover",
                        RuntimeActualState::Stopped => {
                            bail!("a stopped predecessor cannot retain a live runtime lease")
                        }
                        _ => "gateway_forced_exit_takeover",
                    };
                    if state.actual_state != RuntimeActualState::Faulted {
                        let changed = tx.execute(
                            "UPDATE execass_runtime_host_states SET actual_state='faulted',updated_at=?1 WHERE generation=?2 AND host_instance_id=?3 AND fencing_token=?4 AND actual_state=?5 AND updated_at<=?1",
                            params![trusted_now,predecessor.generation,predecessor.host_instance_id,predecessor.fencing_token,state.actual_state.as_str()],
                        )?;
                        if changed != 1 {
                            bail!("predecessor runtime state changed during fenced takeover");
                        }
                    }
                    let released = tx.execute(
                        "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE lease_id=?2 AND generation=?3 AND host_instance_id=?4 AND fencing_token=?5 AND released_at IS NULL",
                        params![trusted_now,predecessor.lease_id,predecessor.generation,predecessor.host_instance_id,predecessor.fencing_token],
                    )?;
                    let ended = tx.execute(
                        "UPDATE execass_runtime_host_generations SET ended_at=?1,end_reason=?2 WHERE generation=?3 AND host_instance_id=?4 AND ended_at IS NULL",
                        params![trusted_now,end_reason,predecessor.generation,predecessor.host_instance_id],
                    )?;
                    if released != 1 || ended != 1 {
                        bail!("predecessor runtime generation lost its takeover fence");
                    }
                    Some(end_reason)
                } else {
                    None
                };
                let (generation, fencing_token) = tx.query_row(
                    r#"SELECT COALESCE(MAX(generation),0)+1,
                              COALESCE((SELECT MAX(fencing_token) FROM execass_runtime_host_leases),0)+1
                       FROM execass_runtime_host_generations"#,
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )?;
                let expires_at = i64::MAX;
                let lease_id = runtime_lease_id(host_instance_id, generation, fencing_token);
                tx.execute(
                    r#"INSERT INTO execass_runtime_host_generations(
                         generation,ownership_scope,state_root_generation,installation_identity,
                         os_user_identity_digest,host_instance_id,started_at
                       ) VALUES(?1,'execass',?2,?3,?4,?5,?6)"#,
                    params![generation,state_root_generation,authority.installation_identity(),authority.os_user_identity_digest(),host_instance_id,trusted_now],
                )?;
                tx.execute(
                    r#"INSERT INTO execass_runtime_host_leases(
                         lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
                       ) VALUES(?1,'execass',?2,?3,?4,?5,?6)"#,
                    params![lease_id,generation,host_instance_id,fencing_token,trusted_now,expires_at],
                )?;
                let successor = RuntimeHostLeaseRecord {
                    lease_id,
                    state_root_generation,
                    generation,
                    host_instance_id: host_instance_id.to_owned(),
                    fencing_token,
                    acquired_at: trusted_now,
                    expires_at,
                };
                ensure_runtime_host_state(
                    tx,
                    &successor,
                    RuntimeActualState::Starting,
                    trusted_now,
                )?;
                let Some(predecessor) = predecessor else {
                    return Ok(RuntimeAtomicReceiptMutation::NoAppend(successor));
                };
                let predecessor_state = predecessor_state.context("predecessor state disappeared")?;
                let end_reason = interruption_end_reason.context("predecessor end reason missing")?;

                let (active_work, active_work_binding_digest) =
                    super::active_work::load_active_work_snapshot(tx)?;
                let identity = runtime_incident_identity(&predecessor);
                let event = NewOutboxEvent {
                    event_id: format!("runtime-recovery-event-{identity}"),
                    event_name: OutboxEventName::RuntimeHostChanged,
                    aggregate_id: "execass-runtime-host".to_owned(),
                    aggregate_revision: predecessor.generation,
                    correlation_id: format!("runtime-recovery-correlation-{identity}"),
                    causation_id: format!("runtime-recovery-cause-{identity}"),
                    occurred_at: trusted_now,
                    safe_payload_json: json!({
                        "active_work_binding_digest": active_work_binding_digest,
                        "active_work_count": active_work.active_work_count,
                        "end_reason": end_reason,
                        "predecessor_actual_state": predecessor_state.actual_state.as_str(),
                        "predecessor_generation": predecessor.generation,
                        "successor_generation": successor.generation,
                    }).to_string(),
                    duplicate_identity: format!("runtime-recovery-{identity}"),
                };
                insert_outbox(tx, &event)?;
                let receipt_id = format!("runtime-recovery-receipt-{identity}");
                super::lifecycle::insert_runtime_paused_attention_in_tx(
                    tx,
                    &RuntimePausedAttentionRecord {
                        attention_id: format!("runtime-paused-{identity}"),
                        status: AttentionStatus::Actionable,
                        reason: "The ExecAss runtime stopped unexpectedly.".to_owned(),
                        recommendation:
                            "Review the recovery evidence and any interrupted work.".to_owned(),
                        alternatives_json: "[]".to_owned(),
                        required_assurance: "local_owner".to_owned(),
                        runtime_host_generation: predecessor.generation,
                        runtime_host_instance_id: predecessor.host_instance_id.clone(),
                        runtime_fencing_token: predecessor.fencing_token,
                        runtime_actual_state: predecessor_state.actual_state,
                        runtime_end_reason: end_reason.to_owned(),
                        active_work_binding_digest,
                        outbox_event_id: event.event_id.clone(),
                        receipt_id: receipt_id.clone(),
                        created_at: trusted_now,
                        resolved_at: None,
                    },
                )?;
                let key = receipt_key
                    .clone()
                    .context("forced runtime takeover requires an active receipt key")?;
                Ok(RuntimeAtomicReceiptMutation::Append {
                    value: successor.clone(),
                    command: Box::new(RuntimeReceiptCommand {
                        receipt_id,
                        transaction_id: format!("runtime-recovery-transaction-{identity}"),
                        state_root_generation,
                        expected_global_count: global_count,
                        expected_global_head_digest: global_head.map(str::to_owned),
                        subject_generation: predecessor.generation,
                        causation_id: event.causation_id,
                        causation_event_id: event.event_id,
                        actor: ReceiptActorBinding {
                            actor_type: ActorType::Runtime,
                            actor_identity: runtime_actor_identity.clone(),
                            authority_provenance_id:
                                "execass-global-control-carrier-authority".to_owned(),
                        },
                        runtime: ReceiptRuntimeBinding {
                            host_generation: successor.generation,
                            host_instance_id: successor.host_instance_id,
                            fencing_token: successor.fencing_token,
                        },
                        key,
                        redacted_summary: receipt_summary.clone(),
                        occurred_at: trusted_now,
                    }),
                })
            },
        )
    }

    /// Advance the actual host lifecycle for the exact current runtime
    /// generation. Desired owner configuration is read, never written here.
    /// A stale host cannot write a successor generation's state.
    pub fn transition_runtime_host(
        &self,
        host: &RuntimeHostLeaseRecord,
        transition: RuntimeHostTransition,
        trusted_now: i64,
    ) -> Result<RuntimeHostTransitionOutcome> {
        if trusted_now <= 0 {
            bail!("runtime host transition requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        require_current_runtime_host(&tx, host, trusted_now)?;
        let current =
            load_runtime_host_state(&tx, host)?.context("current runtime host state is missing")?;
        let desired = load_desired_mode(&tx)?.unwrap_or(RuntimeDesiredMode::AppBound);
        let (next, reason) = resolve_transition(current.actual_state, desired, transition)?;
        if trusted_now < current.updated_at {
            bail!("runtime host transition clock moved backwards");
        }
        let updated = tx
            .execute(
                r#"UPDATE execass_runtime_host_states
               SET actual_state=?1,updated_at=?2
               WHERE generation=?3 AND host_instance_id=?4 AND fencing_token=?5
                 AND state_root_generation=?6 AND updated_at<=?2"#,
                params![
                    next.as_str(),
                    trusted_now,
                    host.generation,
                    host.host_instance_id,
                    host.fencing_token,
                    host.state_root_generation,
                ],
            )
            .context("failed updating fenced runtime host state")?;
        if updated != 1 {
            bail!("runtime host state write did not match the exact current generation");
        }
        if next == RuntimeActualState::Stopped {
            let released = tx.execute(
                r#"UPDATE execass_runtime_host_leases SET released_at=?1
                   WHERE lease_id=?2 AND ownership_scope='execass' AND generation=?3
                     AND host_instance_id=?4 AND fencing_token=?5 AND released_at IS NULL"#,
                params![
                    trusted_now,
                    host.lease_id,
                    host.generation,
                    host.host_instance_id,
                    host.fencing_token,
                ],
            )?;
            let ended = tx.execute(
                r#"UPDATE execass_runtime_host_generations
                   SET ended_at=?1,end_reason='host_state_stopped'
                   WHERE generation=?2 AND host_instance_id=?3 AND ended_at IS NULL"#,
                params![trusted_now, host.generation, host.host_instance_id],
            )?;
            if released != 1 || ended != 1 {
                bail!("runtime host lost its generation fence while stopping");
            }
        }
        tx.commit()
            .context("failed committing runtime host transition")?;
        Ok(RuntimeHostTransitionOutcome {
            from_state: current.actual_state,
            actual_state: next,
            reason,
        })
    }
}

pub(super) fn actual_state_for_live_lease(
    conn: &rusqlite::Connection,
    live_lease: Option<&RuntimeHostLeaseRecord>,
) -> Result<RuntimeActualState> {
    let Some(host) = live_lease else {
        return Ok(RuntimeActualState::Stopped);
    };
    load_runtime_host_state(conn, host)?
        .map(|state| state.actual_state)
        .context("live runtime host state is missing")
}

pub(super) fn resolve_transition(
    current: RuntimeActualState,
    desired: RuntimeDesiredMode,
    transition: RuntimeHostTransition,
) -> Result<(RuntimeActualState, RuntimeHostTransitionReason)> {
    let running_desired = match desired {
        RuntimeDesiredMode::AppBound => RuntimeActualState::RunningAppBound,
        RuntimeDesiredMode::Background => RuntimeActualState::RunningBackground,
    };
    let result = match transition {
        RuntimeHostTransition::ReachDesiredMode
            if matches!(
                current,
                RuntimeActualState::Starting | RuntimeActualState::Handoff
            ) =>
        {
            (
                running_desired,
                RuntimeHostTransitionReason::DesiredModeReached,
            )
        }
        RuntimeHostTransition::BeginHandoff
            if matches!(
                (current, desired),
                (
                    RuntimeActualState::RunningAppBound,
                    RuntimeDesiredMode::Background
                ) | (
                    RuntimeActualState::RunningBackground,
                    RuntimeDesiredMode::AppBound
                )
            ) =>
        {
            (
                RuntimeActualState::Handoff,
                RuntimeHostTransitionReason::DesiredModeRequiresHandoff,
            )
        }
        RuntimeHostTransition::BeginDrain
            if matches!(
                current,
                RuntimeActualState::Starting
                    | RuntimeActualState::RunningAppBound
                    | RuntimeActualState::Handoff
                    | RuntimeActualState::RunningBackground
            ) =>
        {
            (
                RuntimeActualState::Draining,
                RuntimeHostTransitionReason::OrderlyShutdownRequested,
            )
        }
        RuntimeHostTransition::RecordFault
            if !matches!(
                current,
                RuntimeActualState::Stopped | RuntimeActualState::Faulted
            ) =>
        {
            (
                RuntimeActualState::Faulted,
                RuntimeHostTransitionReason::HostFaultRecorded,
            )
        }
        RuntimeHostTransition::CompleteStop if current == RuntimeActualState::Draining => (
            RuntimeActualState::Stopped,
            RuntimeHostTransitionReason::OrderlyShutdownCompleted,
        ),
        _ => bail!(
            "runtime host transition {:?} is forbidden from {} for desired mode {}",
            transition,
            current.as_str(),
            desired.as_str()
        ),
    };
    Ok(result)
}

struct RuntimeHostStateRecord {
    actual_state: RuntimeActualState,
    updated_at: i64,
}

fn ensure_runtime_host_state(
    conn: &rusqlite::Connection,
    host: &RuntimeHostLeaseRecord,
    actual_state: RuntimeActualState,
    trusted_now: i64,
) -> Result<()> {
    conn.execute(
        r#"INSERT OR IGNORE INTO execass_runtime_host_states(
             generation,host_instance_id,fencing_token,state_root_generation,actual_state,updated_at
           ) VALUES(?1,?2,?3,?4,?5,?6)"#,
        params![
            host.generation,
            host.host_instance_id,
            host.fencing_token,
            host.state_root_generation,
            actual_state.as_str(),
            trusted_now,
        ],
    )
    .context("failed recording initial runtime host state")?;
    Ok(())
}

fn load_runtime_host_state(
    conn: &rusqlite::Connection,
    host: &RuntimeHostLeaseRecord,
) -> Result<Option<RuntimeHostStateRecord>> {
    conn.query_row(
        r#"SELECT actual_state,updated_at FROM execass_runtime_host_states
           WHERE generation=?1 AND host_instance_id=?2 AND fencing_token=?3
             AND state_root_generation=?4"#,
        params![
            host.generation,
            host.host_instance_id,
            host.fencing_token,
            host.state_root_generation,
        ],
        |row| {
            Ok(RuntimeHostStateRecord {
                actual_state: row.get(0)?,
                updated_at: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn require_current_runtime_host(
    conn: &rusqlite::Connection,
    host: &RuntimeHostLeaseRecord,
    trusted_now: i64,
) -> Result<()> {
    let exact_current = conn
        .query_row(
            r#"SELECT 1 FROM execass_runtime_host_leases lease
               JOIN execass_runtime_host_generations generation
                 ON generation.generation=lease.generation
                 AND generation.host_instance_id=lease.host_instance_id
               WHERE lease.lease_id=?1 AND lease.ownership_scope='execass'
                 AND lease.generation=?2 AND lease.host_instance_id=?3
                 AND lease.fencing_token=?4 AND lease.acquired_at=?5 AND lease.expires_at=?6
                 AND generation.state_root_generation=?7 AND lease.released_at IS NULL
                 AND generation.ended_at IS NULL AND lease.expires_at>?8"#,
            params![
                host.lease_id,
                host.generation,
                host.host_instance_id,
                host.fencing_token,
                host.acquired_at,
                host.expires_at,
                host.state_root_generation,
                trusted_now,
            ],
            |_| Ok(()),
        )
        .optional()?;
    if exact_current.is_none() {
        bail!("runtime host transition is fenced to a different current generation");
    }
    Ok(())
}

fn load_desired_mode(conn: &rusqlite::Connection) -> Result<Option<RuntimeDesiredMode>> {
    conn.query_row(
        "SELECT desired_mode FROM execass_runtime_settings_revisions ORDER BY settings_revision DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn runtime_lease_id(host_instance_id: &str, generation: i64, fencing_token: i64) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.runtime_host_lease.v1\0");
    digest.update(host_instance_id.as_bytes());
    digest.update([0]);
    digest.update(generation.to_le_bytes());
    digest.update(fencing_token.to_le_bytes());
    format!("runtime-lease-{:x}", digest.finalize())
}

fn runtime_incident_identity(predecessor: &RuntimeHostLeaseRecord) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.runtime_recovery_incident.v1\0");
    digest.update(predecessor.generation.to_le_bytes());
    digest.update(predecessor.host_instance_id.as_bytes());
    digest.update([0]);
    digest.update(predecessor.fencing_token.to_le_bytes());
    format!("{:x}", digest.finalize())
}
