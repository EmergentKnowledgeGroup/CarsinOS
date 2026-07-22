//! EA-212 storage-owned logical-effect dispatch and provider-attempt lifecycle.

use super::claim::{objective_drift, validate_claim_identity_live, Operation};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

impl ExecAssStore {
    /// At the exact persisted continuation claim fence, creates one prepared
    /// provider attempt or returns the same persisted attempt. This is not an
    /// external-call authorization; begin invocation owns that boundary.
    pub fn prepare_provider_attempt(
        &self,
        command: &PrepareProviderAttemptCommand,
    ) -> Result<PrepareProviderAttemptOutcome> {
        validate_prepare(command)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let live = validate_claim_identity_live(
            &tx,
            &command.claim,
            command.trusted_now,
            Operation::Dispatch,
        )?;
        let outcome = match live {
            Err(reason) => PrepareProviderAttemptOutcome::Stale { reason },
            Ok((_continuation, action)) => {
                match load_effect_for_claim(&tx, &command.claim, &action.action_id)? {
                    None => PrepareProviderAttemptOutcome::Conflict,
                    Some(effect) => match latest_attempt(&tx, &effect.logical_effect_id)? {
                        Some(existing) => {
                            prepare_against_existing(&tx, command, &effect, existing)?
                        }
                        None => create_attempt(&tx, command, &effect, 1)?,
                    },
                }
            }
        };
        tx.commit()
            .context("failed committing provider-attempt preparation")?;
        Ok(outcome)
    }

    /// Records one immutable terminal provider result. `outcome_unknown` is
    /// intentionally terminal here and remains unretryable until EA-213
    /// reconciliation changes it through a later, separate path.
    pub fn record_provider_attempt_result(
        &self,
        command: &RecordProviderAttemptResultCommand,
    ) -> Result<RecordProviderAttemptResultOutcome> {
        validate_result(command)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(existing) = attempt_by_id(&tx, &command.attempt_id)? else {
            tx.commit()?;
            return Ok(RecordProviderAttemptResultOutcome::NotFound);
        };
        let outcome = if existing.dispatch
            != dispatch_from_claim(&existing.dispatch, &command.claim)
        {
            RecordProviderAttemptResultOutcome::Conflict
        } else if existing.status.is_terminal() {
            if existing.status == command.status
                && existing.provider_response_digest.as_deref()
                    == Some(command.provider_response_digest.as_str())
                && existing.remote_effect_id == command.remote_effect_id
                && existing.finished_at == Some(command.finished_at)
            {
                RecordProviderAttemptResultOutcome::Replayed(Box::new(existing))
            } else {
                RecordProviderAttemptResultOutcome::Conflict
            }
        } else if existing.status != ProviderAttemptStatus::Invoking {
            RecordProviderAttemptResultOutcome::Conflict
        } else if let Err(reason) = validate_claim_identity_live(
            &tx,
            &command.claim,
            command.trusted_now,
            Operation::Dispatch,
        )? {
            RecordProviderAttemptResultOutcome::Stale { reason }
        } else {
            let changed = tx.execute(
                r#"UPDATE execass_provider_attempts
                   SET status=?1,provider_response_digest=?2,
                       provider_error_class=CASE WHEN ?1='failed' THEN (
                         SELECT evidence.provider_error_class
                         FROM execass_effect_recorder_evidence evidence
                         WHERE evidence.attempt_id=?5 AND evidence.journal_source='execution'
                           AND evidence.journal_kind='absent'
                           AND evidence.response_digest IS ?2
                         ORDER BY evidence.journal_sequence DESC LIMIT 1
                       ) ELSE NULL END,
                       remote_effect_id=?3,finished_at=?4
                   WHERE attempt_id=?5 AND status=?6"#,
                params![
                    command.status.as_str(),
                    command.provider_response_digest,
                    command.remote_effect_id,
                    command.finished_at,
                    command.attempt_id,
                    existing.status.as_str()
                ],
            )?;
            if changed != 1 {
                bail!("provider attempt lost its terminal-result race");
            }
            let effect_state = match command.status {
                ProviderAttemptStatus::Succeeded => "succeeded",
                ProviderAttemptStatus::Failed => "failed",
                ProviderAttemptStatus::OutcomeUnknown => "outcome_unknown",
                _ => unreachable!("result validation admits terminal outcomes only"),
            };
            let changed = tx.execute(
                "UPDATE execass_logical_effects SET state=?1,updated_at=?2 WHERE delegation_id=?3 AND logical_effect_id=?4 AND state='invoking'",
                params![effect_state, command.finished_at, existing.dispatch.delegation_id, existing.dispatch.logical_effect_id],
            )?;
            if changed != 1 {
                bail!("logical effect lost its provider-result transition");
            }
            RecordProviderAttemptResultOutcome::Recorded(Box::new(
                attempt_by_id(&tx, &command.attempt_id)?
                    .context("recorded provider attempt disappeared")?,
            ))
        };
        tx.commit()
            .context("failed committing provider-attempt result")?;
        Ok(outcome)
    }

    /// Atomically turns one persisted prepared attempt into the only
    /// dispatch-authorizing invoking attempt. This revalidates the exact live
    /// continuation/job/resource/host fence immediately before authorization.
    pub fn begin_provider_attempt_invocation(
        &self,
        command: &BeginProviderAttemptInvocationCommand,
    ) -> Result<BeginProviderAttemptInvocationOutcome> {
        require_text("attempt_id", &command.attempt_id)?;
        if command.trusted_now <= 0 {
            bail!("provider invocation requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(existing) = attempt_by_id(&tx, &command.attempt_id)? else {
            tx.commit()?;
            return Ok(BeginProviderAttemptInvocationOutcome::NotFound);
        };
        let outcome = if existing.dispatch
            != dispatch_from_claim(&existing.dispatch, &command.claim)
        {
            BeginProviderAttemptInvocationOutcome::Conflict
        } else if existing.status == ProviderAttemptStatus::Invoking {
            BeginProviderAttemptInvocationOutcome::AlreadyInvoking(Box::new(existing))
        } else if existing.status != ProviderAttemptStatus::Prepared {
            BeginProviderAttemptInvocationOutcome::Conflict
        } else {
            let live = validate_claim_identity_live(
                &tx,
                &command.claim,
                command.trusted_now,
                Operation::Dispatch,
            )?;
            let (continuation, action) = match live {
                Ok(live) => live,
                Err(reason) => {
                    tx.commit()?;
                    return Ok(BeginProviderAttemptInvocationOutcome::Stale { reason });
                }
            };
            if let Some(reason) =
                objective_drift(&tx, &continuation, &action, ContinuationStatus::Executing)?
            {
                tx.commit()?;
                return Ok(BeginProviderAttemptInvocationOutcome::Stale { reason });
            }
            let changed = tx.execute(
                "UPDATE execass_logical_effects SET state='invoking',updated_at=?1 WHERE delegation_id=?2 AND logical_effect_id=?3 AND state IN ('claimed','failed')",
                params![command.trusted_now, existing.dispatch.delegation_id, existing.dispatch.logical_effect_id],
            )?;
            if changed != 1 {
                BeginProviderAttemptInvocationOutcome::Conflict
            } else {
                let changed = tx.execute(
                    "UPDATE execass_provider_attempts SET status='invoking' WHERE attempt_id=?1 AND status='prepared'",
                    params![command.attempt_id],
                )?;
                if changed != 1 {
                    bail!("provider attempt lost its begin-invocation race");
                }
                BeginProviderAttemptInvocationOutcome::Began(Box::new(
                    attempt_by_id(&tx, &command.attempt_id)?
                        .context("begun provider attempt disappeared")?,
                ))
            }
        };
        tx.commit()
            .context("failed committing provider invocation boundary")?;
        Ok(outcome)
    }
}

impl ProviderAttemptStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::OutcomeUnknown
                | Self::ReconciledAbsent
                | Self::ReconciledPresent
        )
    }
}

#[derive(Debug, Clone)]
struct EffectRow {
    logical_effect_id: String,
    delegation_id: String,
    continuation_id: String,
    action_id: String,
    state: LogicalEffectState,
    internal_idempotency_key: String,
    provider_identity: Option<String>,
    provider_idempotency_key: Option<String>,
    reconciliation_key: Option<String>,
    manifest_digest: String,
    payload_digest: String,
}

fn validate_prepare(command: &PrepareProviderAttemptCommand) -> Result<()> {
    if command.trusted_now <= 0 {
        bail!("provider attempt preparation requires a positive trusted clock");
    }
    Ok(())
}

fn validate_result(command: &RecordProviderAttemptResultCommand) -> Result<()> {
    require_text("attempt_id", &command.attempt_id)?;
    require_text(
        "provider_response_digest",
        &command.provider_response_digest,
    )?;
    if command.trusted_now <= 0
        || command.finished_at <= 0
        || !matches!(
            command.status,
            ProviderAttemptStatus::Succeeded
                | ProviderAttemptStatus::Failed
                | ProviderAttemptStatus::OutcomeUnknown
        )
    {
        bail!("provider attempt result must be a positive-clock terminal dispatch outcome");
    }
    if let Some(remote_effect_id) = &command.remote_effect_id {
        require_text("remote_effect_id", remote_effect_id)?;
    }
    Ok(())
}

fn load_effect_for_claim(
    conn: &Connection,
    claim: &ContinuationClaimIdentity,
    action_id: &str,
) -> Result<Option<EffectRow>> {
    conn.query_row(
        r#"SELECT logical_effect_id,delegation_id,continuation_id,state,internal_idempotency_key,
                  provider_identity,provider_idempotency_key,reconciliation_key,manifest_digest,payload_digest
           FROM execass_logical_effects
           WHERE delegation_id=?1 AND continuation_id=?2"#,
        params![claim.delegation_id, claim.continuation_id],
        |row| Ok(EffectRow {
            logical_effect_id: row.get(0)?, delegation_id: row.get(1)?, continuation_id: row.get(2)?,
            action_id: action_id.to_owned(), state: row.get(3)?, internal_idempotency_key: row.get(4)?,
            provider_identity: row.get(5)?, provider_idempotency_key: row.get(6)?, reconciliation_key: row.get(7)?,
            manifest_digest: row.get(8)?, payload_digest: row.get(9)?,
        }),
    ).optional().context("failed reading logical effect at claim fence")
}

fn prepare_against_existing(
    conn: &Connection,
    command: &PrepareProviderAttemptCommand,
    effect: &EffectRow,
    existing: ProviderAttemptRecord,
) -> Result<PrepareProviderAttemptOutcome> {
    let dispatch = dispatch_identity(effect, &command.claim);
    if existing.dispatch != dispatch
        || existing.provider_request_digest != provider_request_digest(&dispatch)
    {
        return Ok(PrepareProviderAttemptOutcome::Conflict);
    }
    match existing.status {
        ProviderAttemptStatus::Failed if command.retry_authorization.is_some() => {
            let authorization = command
                .retry_authorization
                .as_ref()
                .expect("checked retry authorization presence");
            if !retry_authorization_matches(
                conn,
                authorization,
                effect,
                &existing,
                command.trusted_now,
            ) {
                return Ok(PrepareProviderAttemptOutcome::Conflict);
            }
            create_attempt(conn, command, effect, existing.attempt_number + 1)
        }
        ProviderAttemptStatus::Failed => {
            Ok(PrepareProviderAttemptOutcome::Replayed(Box::new(existing)))
        }
        ProviderAttemptStatus::OutcomeUnknown
        | ProviderAttemptStatus::ReconciledAbsent
        | ProviderAttemptStatus::ReconciledPresent
            if command.retry_authorization.is_some() =>
        {
            Ok(PrepareProviderAttemptOutcome::Conflict)
        }
        _ => Ok(PrepareProviderAttemptOutcome::Replayed(Box::new(existing))),
    }
}

fn retry_authorization_matches(
    conn: &Connection,
    authorization: &ProviderRetryAuthorization,
    effect: &EffectRow,
    existing: &ProviderAttemptRecord,
    trusted_now: i64,
) -> bool {
    let persisted = conn
        .query_row(
            r#"SELECT EXISTS(
                 SELECT 1 FROM execass_recovery_evaluations evaluation
                 JOIN execass_recovery_episodes episode
                   ON episode.recovery_episode_id=evaluation.recovery_episode_id
                 JOIN execass_delegations delegation
                   ON delegation.delegation_id=evaluation.delegation_id
                 WHERE evaluation.recovery_evaluation_id=?1
                   AND evaluation.logical_effect_id=?2
                   AND evaluation.predecessor_attempt_id=?3
                   AND evaluation.objective_facts_digest=?4
                   AND evaluation.directive='retry_same_effect'
                   AND evaluation.not_before_ms=?5
                   AND evaluation.recovery_state_revision=?6
                   AND delegation.state_revision=?6
                   AND delegation.phase='recovering'
                   AND evaluation.evaluation_revision=(
                     SELECT MAX(latest.evaluation_revision)
                     FROM execass_recovery_evaluations latest
                     WHERE latest.recovery_episode_id=episode.recovery_episode_id
                   )
               )"#,
            params![
                authorization.recovery_evaluation_id,
                authorization.logical_effect_id,
                authorization.predecessor_attempt_id,
                authorization.objective_facts_digest,
                authorization.not_before_ms,
                authorization.recovery_state_revision,
            ],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        == 1;
    persisted
        && authorization.logical_effect_id == effect.logical_effect_id
        && authorization.predecessor_attempt_id == existing.attempt_id
        && authorization.authorized_attempt_number == existing.attempt_number + 1
        && authorization.not_before_ms <= trusted_now
        && authorization.recovery_state_revision > 0
        && authorization.objective_facts_digest.starts_with("sha256:")
        && authorization.objective_facts_digest.len() == 71
}

fn create_attempt(
    conn: &Connection,
    command: &PrepareProviderAttemptCommand,
    effect: &EffectRow,
    attempt_number: i64,
) -> Result<PrepareProviderAttemptOutcome> {
    if effect.state == LogicalEffectState::OutcomeUnknown
        || effect.state == LogicalEffectState::ReconciledPresent
    {
        return Ok(PrepareProviderAttemptOutcome::Conflict);
    }
    if attempt_number > 1 && effect.state != LogicalEffectState::Failed {
        return Ok(PrepareProviderAttemptOutcome::Conflict);
    }
    let dispatch = dispatch_identity(effect, &command.claim);
    let provider_request_digest = provider_request_digest(&dispatch);
    let attempt_id = attempt_id(&dispatch, attempt_number);
    conn.execute(
        r#"INSERT INTO execass_provider_attempts(
             attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,claim_event_id,claim_receipt_id,
             attempt_number,fencing_token,host_generation,host_instance_id,runtime_fencing_token,status,
             provider_request_digest,provider_response_digest,remote_effect_id,started_at,finished_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'prepared',?13,NULL,NULL,?14,NULL)"#,
        params![attempt_id, dispatch.delegation_id, dispatch.logical_effect_id, dispatch.continuation_id, dispatch.action_id,
            dispatch.claim_event_id, dispatch.claim_receipt_id, attempt_number, dispatch.continuation_fencing_token,
            dispatch.runtime_host_generation, dispatch.runtime_host_instance_id, dispatch.runtime_fencing_token,
            provider_request_digest, command.trusted_now],
    ).context("failed inserting immutable provider attempt")?;
    Ok(PrepareProviderAttemptOutcome::Prepared(Box::new(
        attempt_by_id(conn, &attempt_id)?.context("prepared provider attempt disappeared")?,
    )))
}

fn dispatch_identity(
    effect: &EffectRow,
    claim: &ContinuationClaimIdentity,
) -> LogicalEffectDispatchIdentity {
    LogicalEffectDispatchIdentity {
        logical_effect_id: effect.logical_effect_id.clone(),
        delegation_id: effect.delegation_id.clone(),
        continuation_id: effect.continuation_id.clone(),
        action_id: effect.action_id.clone(),
        claim_event_id: claim.claim_event_id.clone(),
        claim_receipt_id: claim.claim_receipt_id.clone(),
        continuation_fencing_token: claim.continuation_fencing_token,
        runtime_host_generation: claim.runtime_host_generation,
        runtime_host_instance_id: claim.runtime_host_instance_id.clone(),
        runtime_fencing_token: claim.runtime_fencing_token,
        internal_idempotency_key: effect.internal_idempotency_key.clone(),
        provider_identity: effect.provider_identity.clone(),
        provider_idempotency_key: effect.provider_idempotency_key.clone(),
        reconciliation_key: effect.reconciliation_key.clone(),
        manifest_digest: effect.manifest_digest.clone(),
        payload_digest: effect.payload_digest.clone(),
    }
}

fn dispatch_from_claim(
    persisted: &LogicalEffectDispatchIdentity,
    claim: &ContinuationClaimIdentity,
) -> LogicalEffectDispatchIdentity {
    LogicalEffectDispatchIdentity {
        logical_effect_id: persisted.logical_effect_id.clone(),
        delegation_id: persisted.delegation_id.clone(),
        continuation_id: claim.continuation_id.clone(),
        action_id: claim.action_id.clone(),
        claim_event_id: claim.claim_event_id.clone(),
        claim_receipt_id: claim.claim_receipt_id.clone(),
        continuation_fencing_token: claim.continuation_fencing_token,
        runtime_host_generation: claim.runtime_host_generation,
        runtime_host_instance_id: claim.runtime_host_instance_id.clone(),
        runtime_fencing_token: claim.runtime_fencing_token,
        internal_idempotency_key: persisted.internal_idempotency_key.clone(),
        provider_identity: persisted.provider_identity.clone(),
        provider_idempotency_key: persisted.provider_idempotency_key.clone(),
        reconciliation_key: persisted.reconciliation_key.clone(),
        manifest_digest: persisted.manifest_digest.clone(),
        payload_digest: persisted.payload_digest.clone(),
    }
}

fn attempt_id(dispatch: &LogicalEffectDispatchIdentity, attempt_number: i64) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.provider-attempt.v1\0");
    for part in [&dispatch.claim_event_id, &dispatch.logical_effect_id] {
        digest.update(part.as_bytes());
        digest.update(b"\0");
    }
    digest.update(attempt_number.to_be_bytes());
    format!("provider-attempt-{:x}", digest.finalize())
}

pub(super) fn provider_request_digest(dispatch: &LogicalEffectDispatchIdentity) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.provider-request.v1\0");
    for part in [
        dispatch.internal_idempotency_key.as_str(),
        dispatch.provider_identity.as_deref().unwrap_or(""),
        dispatch.provider_idempotency_key.as_deref().unwrap_or(""),
        dispatch.reconciliation_key.as_deref().unwrap_or(""),
        dispatch.manifest_digest.as_str(),
        dispatch.payload_digest.as_str(),
    ] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    // The installed exact-overwrite recorder hashes the full opaque operand
    // envelope as `payload_digest`. Append that immutable binding once more as
    // the provider-specific v1 extension; all pre-existing providers retain
    // their byte-for-byte digest contract above.
    if dispatch.provider_identity.as_deref() == Some("carsinos.local-fs.exact-overwrite") {
        digest.update(b"carsinos.execass.provider-request.exact-overwrite-operand.v1\0");
        let part = dispatch.payload_digest.as_str();
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sha256:{:x}", digest.finalize())
}

fn latest_attempt(
    conn: &Connection,
    logical_effect_id: &str,
) -> Result<Option<ProviderAttemptRecord>> {
    conn.query_row("SELECT attempt_id FROM execass_provider_attempts WHERE logical_effect_id=?1 ORDER BY attempt_number DESC LIMIT 1", params![logical_effect_id], |row| row.get::<_, String>(0)).optional()?.map_or(Ok(None), |id| attempt_by_id(conn, &id))
}

fn attempt_by_id(conn: &Connection, attempt_id: &str) -> Result<Option<ProviderAttemptRecord>> {
    conn.query_row(
        r#"SELECT a.attempt_id,a.attempt_number,a.status,a.provider_request_digest,a.provider_response_digest,a.provider_error_class,a.remote_effect_id,a.started_at,a.finished_at,
                   a.logical_effect_id,a.delegation_id,a.continuation_id,a.action_id,a.claim_event_id,a.claim_receipt_id,
                  a.fencing_token,a.host_generation,a.host_instance_id,a.runtime_fencing_token,
                  e.internal_idempotency_key,e.provider_identity,e.provider_idempotency_key,e.reconciliation_key,e.manifest_digest,e.payload_digest
           FROM execass_provider_attempts a JOIN execass_logical_effects e ON e.delegation_id=a.delegation_id AND e.logical_effect_id=a.logical_effect_id WHERE a.attempt_id=?1"#,
        params![attempt_id],
        |row| Ok(ProviderAttemptRecord {
            attempt_id: row.get(0)?, attempt_number: row.get(1)?, status: row.get(2)?, provider_request_digest: row.get(3)?, provider_response_digest: row.get(4)?, provider_error_class: row.get(5)?, remote_effect_id: row.get(6)?, started_at: row.get(7)?, finished_at: row.get(8)?,
            dispatch: LogicalEffectDispatchIdentity { logical_effect_id: row.get(9)?, delegation_id: row.get(10)?, continuation_id: row.get(11)?, action_id: row.get(12)?, claim_event_id: row.get(13)?, claim_receipt_id: row.get(14)?, continuation_fencing_token: row.get(15)?, runtime_host_generation: row.get(16)?, runtime_host_instance_id: row.get(17)?, runtime_fencing_token: row.get(18)?, internal_idempotency_key: row.get(19)?, provider_identity: row.get(20)?, provider_idempotency_key: row.get(21)?, reconciliation_key: row.get(22)?, manifest_digest: row.get(23)?, payload_digest: row.get(24)? },
        }),
    ).optional().context("failed reading provider attempt")
}
