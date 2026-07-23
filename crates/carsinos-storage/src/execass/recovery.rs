//! Storage-derived EA-214 objective recovery admission.

use super::receipt::{
    load_receipt, receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::get_outbox;
#[cfg(test)]
use super::store::immediate_transaction;
use super::store::ExecAssStore;
use super::types::{
    DeclaredRecoverySafeBoundary, DelegationPhase, LogicalEffectState, ObjectiveRecoveryEvaluation,
    ProviderAttemptStatus, ProviderFailureClass, ProviderRecoveryBundle, ProviderRecoveryCommand,
    ProviderRecoveryOutcome, ProviderRetryAuthorization,
};
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use carsinos_core::execass_policy::RecoveryScope;
use carsinos_core::execass_recovery::{
    plan_objective_recovery, ObjectiveRecoveryFacts, ProviderErrorClass, RecoveryDirective,
    RecoveryPolicy, RetrySafety,
};
use rusqlite::{params, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

impl ExecAssStore {
    pub fn read_provider_recovery_receipt_context(
        &self,
        logical_effect_id: &str,
        trusted_now: i64,
    ) -> Result<Option<super::types::ContinuationReceiptContext>> {
        require_text("logical_effect_id", logical_effect_id)?;
        let conn = self.connection()?;
        let continuation_id = conn
            .query_row(
                "SELECT continuation_id FROM execass_logical_effects WHERE logical_effect_id=?1",
                [logical_effect_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        drop(conn);
        match continuation_id {
            Some(continuation_id) => {
                self.read_continuation_receipt_context(&continuation_id, trusted_now)
            }
            None => Ok(None),
        }
    }

    pub fn list_due_provider_recovery_effects(
        &self,
        trusted_now: i64,
        limit: u32,
    ) -> Result<Vec<String>> {
        if trusted_now <= 0 || limit == 0 {
            bail!("provider recovery scan requires a positive clock and limit");
        }
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            r#"SELECT e.logical_effect_id
               FROM execass_logical_effects e
               WHERE e.state IN ('failed','outcome_unknown')
                 AND (
                   NOT EXISTS(
                     SELECT 1 FROM execass_recovery_episodes episode
                     WHERE episode.logical_effect_id=e.logical_effect_id
                   )
                   OR EXISTS(
                     SELECT 1 FROM execass_recovery_evaluations evaluation
                     JOIN execass_recovery_episodes episode
                       ON episode.recovery_episode_id=evaluation.recovery_episode_id
                     WHERE episode.logical_effect_id=e.logical_effect_id
                       AND evaluation.evaluation_revision=(
                         SELECT MAX(latest.evaluation_revision)
                         FROM execass_recovery_evaluations latest
                         WHERE latest.recovery_episode_id=evaluation.recovery_episode_id
                       )
                       AND evaluation.directive IN ('wait_backoff','wait_circuit_breaker')
                       AND evaluation.not_before_ms<=?1
                   )
                 )
               ORDER BY e.updated_at,e.logical_effect_id
               LIMIT ?2"#,
        )?;
        let effects = statement
            .query_map(params![trusted_now, i64::from(limit)], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed listing due provider recovery effects")?;
        Ok(effects)
    }

    #[cfg(test)]
    pub fn evaluate_provider_recovery(
        &self,
        logical_effect_id: &str,
        trusted_now: i64,
    ) -> Result<ObjectiveRecoveryEvaluation> {
        require_text("logical_effect_id", logical_effect_id)?;
        if trusted_now <= 0 {
            bail!("provider recovery evaluation requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let evaluation = evaluate_provider_recovery_in_transaction(
            &tx,
            logical_effect_id,
            trusted_now,
            None,
            None,
        )?;
        tx.commit()
            .context("failed committing test objective recovery evaluation")?;
        Ok(evaluation)
    }

    pub fn apply_provider_recovery_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ProviderRecoveryCommand,
    ) -> Result<ProviderRecoveryOutcome> {
        validate_recovery_command(command)?;
        let post_revision = command.expected_pre_state_revision + 1;
        let event_identity = RecoveryEventIdentity {
            event_id: &command.receipt.causation_event_id,
            correlation_id: &command.write.correlation_id,
            causation_id: &command.write.causation_id,
        };
        let outcome = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            command.expected_pre_state_revision,
            &command.receipt,
            |transaction| {
                if receipt_by_causation_event(transaction, &command.receipt.causation_event_id)?
                    .is_some()
                {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        RecoveryMutationOutcome::Replayed(Box::new(load_recovery_replay(
                            transaction,
                            command,
                        )?)),
                    ));
                }
                let evaluation = evaluate_provider_recovery_in_transaction(
                    transaction,
                    &command.logical_effect_id,
                    command.trusted_now,
                    Some(post_revision),
                    Some(event_identity),
                )?;
                let selected_phase =
                    project_recovery_lifecycle(transaction, command, &evaluation, post_revision)?;
                let outbox_event = get_outbox(transaction, &command.receipt.causation_event_id)?
                    .context("atomic recovery outbox event disappeared")?;
                Ok(AtomicReceiptMutation::Append(
                    RecoveryMutationOutcome::Applied(Box::new(RecoveryDraft {
                        evaluation,
                        selected_phase,
                        state_revision: post_revision,
                        outbox_event,
                    })),
                ))
            },
        )?;
        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: RecoveryMutationOutcome::Applied(draft),
                receipt,
            } => Ok(ProviderRecoveryOutcome::Applied(Box::new(
                ProviderRecoveryBundle {
                    evaluation: draft.evaluation,
                    selected_phase: draft.selected_phase,
                    state_revision: draft.state_revision,
                    outbox_event: draft.outbox_event,
                    receipt,
                },
            ))),
            AtomicReceiptWriteOutcome::NoAppend(RecoveryMutationOutcome::Replayed(bundle)) => {
                Ok(ProviderRecoveryOutcome::Replayed(bundle))
            }
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            } => Ok(ProviderRecoveryOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            }),
            AtomicReceiptWriteOutcome::Appended { .. }
            | AtomicReceiptWriteOutcome::NoAppend(RecoveryMutationOutcome::Applied(_)) => {
                bail!("atomic recovery receipt coordinator returned an impossible outcome")
            }
        }
    }
}

#[derive(Clone, Copy)]
struct RecoveryEventIdentity<'a> {
    event_id: &'a str,
    correlation_id: &'a str,
    causation_id: &'a str,
}

struct RecoveryDraft {
    evaluation: ObjectiveRecoveryEvaluation,
    selected_phase: DelegationPhase,
    state_revision: i64,
    outbox_event: super::types::OutboxEventRecord,
}

enum RecoveryMutationOutcome {
    Applied(Box<RecoveryDraft>),
    Replayed(Box<ProviderRecoveryBundle>),
}

fn evaluate_provider_recovery_in_transaction(
    tx: &Transaction<'_>,
    logical_effect_id: &str,
    trusted_now: i64,
    recovery_state_revision: Option<i64>,
    provided_event_identity: Option<RecoveryEventIdentity<'_>>,
) -> Result<ObjectiveRecoveryEvaluation> {
    require_text("logical_effect_id", logical_effect_id)?;
    if trusted_now <= 0 {
        bail!("provider recovery evaluation requires a positive trusted clock");
    }
    let row = tx
            .query_row(
                r#"SELECT e.delegation_id,e.operation_reversible,
                          e.declared_recovery_safe_boundary,e.state,e.provider_identity,
                          e.provider_idempotency_key,d.effective_authority_json,
                           a.attempt_id,a.attempt_number,a.status,a.provider_error_class,a.started_at,
                          (SELECT MIN(started_at) FROM execass_provider_attempts
                           WHERE logical_effect_id=e.logical_effect_id),
                          (SELECT COUNT(*) FROM execass_provider_attempts
                           WHERE logical_effect_id=e.logical_effect_id),
                          d.normalized_original_intent,c.action_id,e.manifest_digest,d.state_revision,
                          (SELECT first_attempt.attempt_id FROM execass_provider_attempts first_attempt
                           WHERE first_attempt.logical_effect_id=e.logical_effect_id
                           ORDER BY first_attempt.attempt_number ASC LIMIT 1)
                   FROM execass_logical_effects e
                   JOIN execass_delegations d ON d.delegation_id=e.delegation_id
                   JOIN execass_continuations c ON c.continuation_id=e.continuation_id
                   JOIN execass_provider_attempts a ON a.attempt_id=(
                     SELECT latest.attempt_id FROM execass_provider_attempts latest
                     WHERE latest.logical_effect_id=e.logical_effect_id
                     ORDER BY latest.attempt_number DESC LIMIT 1
                   )
                   WHERE e.logical_effect_id=?1"#,
                [logical_effect_id],
                |row| {
                    Ok(RecoveryRow {
                        delegation_id: row.get(0)?,
                        operation_reversible: row.get::<_, i64>(1)? == 1,
                        declared_recovery_safe_boundary: row.get(2)?,
                        effect_state: row.get(3)?,
                        provider_identity: row.get(4)?,
                        provider_idempotency_key: row.get(5)?,
                        effective_authority_json: row.get(6)?,
                        attempt_id: row.get(7)?,
                        attempt_number: row.get(8)?,
                        attempt_status: row.get(9)?,
                        provider_error_class: row.get(10)?,
                        last_attempt_at: row.get(11)?,
                        first_attempt_at: row.get(12)?,
                        attempts_started: row.get(13)?,
                        normalized_original_intent: row.get(14)?,
                        action_id: row.get(15)?,
                        manifest_digest: row.get(16)?,
                        state_revision: row.get(17)?,
                        initial_attempt_id: row.get(18)?,
                    })
                },
            )
            .optional()
            .context("failed reading objective recovery source state")?
            .context("objective recovery requires an existing effect attempt")?;

    if !matches!(
        row.attempt_status,
        ProviderAttemptStatus::Failed
            | ProviderAttemptStatus::OutcomeUnknown
            | ProviderAttemptStatus::ReconciledAbsent
            | ProviderAttemptStatus::ReconciledPresent
    ) {
        bail!("objective recovery requires a terminal provider attempt");
    }
    if row.attempt_number != row.attempts_started {
        bail!("provider attempt history is not gap-free");
    }
    if (row.attempt_status == ProviderAttemptStatus::Failed) != row.provider_error_class.is_some() {
        bail!("provider failure classification does not match the terminal attempt state");
    }

    let independent_absence = tx.query_row(
        r#"SELECT EXISTS(
                 SELECT 1 FROM execass_effect_recorder_evidence
                 WHERE attempt_id=?1 AND journal_kind='absent'
               )"#,
        [&row.attempt_id],
        |record| record.get::<_, i64>(0),
    )? == 1;
    let technical_resources_available = tx.query_row(
        r#"SELECT
             NOT EXISTS(
               SELECT 1
               FROM execass_technical_resource_requirements requirement
               JOIN execass_technical_resource_requirement_sets requirement_set
                 ON requirement_set.requirement_set_id=requirement.requirement_set_id
               LEFT JOIN execass_technical_resource_quota_entries quota
                 ON quota.quota_snapshot_id=requirement_set.quota_snapshot_id
                AND quota.technical_resource_kind=requirement.technical_resource_kind
                AND quota.unit=requirement.unit
               WHERE requirement_set.logical_effect_id=?1
                 AND (
                   quota.amount_limit IS NULL
                   OR COALESCE((
                     SELECT SUM(CASE
                       WHEN reservation.status IN ('reserved','reconciliation_required')
                         THEN reservation.amount_reserved
                       WHEN reservation.status='settled' THEN actual.amount_actual
                       ELSE 0 END)
                     FROM execass_technical_resource_reservations reservation
                     LEFT JOIN execass_technical_resource_actuals actual
                       ON actual.reservation_id=reservation.reservation_id
                     WHERE reservation.quota_snapshot_id=requirement_set.quota_snapshot_id
                       AND reservation.technical_resource_kind=requirement.technical_resource_kind
                       AND reservation.unit=requirement.unit
                   ),0)+requirement.amount_required > quota.amount_limit
                 )
             )
             AND NOT EXISTS(
               SELECT 1 FROM execass_technical_resource_reservations
               WHERE logical_effect_id=?1 AND status='reconciliation_required'
             )"#,
        [logical_effect_id],
        |record| record.get::<_, i64>(0),
    )? == 1;
    let circuit_open_until_ms = row.provider_identity.as_deref().and_then(|provider| {
            tx.query_row(
                "SELECT cooldown_until FROM circuit_breaker_states WHERE scope='provider' AND target_id=?1 AND state='open'",
                [provider],
                |record| record.get::<_, Option<i64>>(0),
            )
            .optional()
            .ok()
            .flatten()
            .flatten()
        });
    let (scope, policy) = recovery_policy(&row.effective_authority_json);
    let retry_safety = if row.attempt_status == ProviderAttemptStatus::OutcomeUnknown
        || row.effect_state == LogicalEffectState::OutcomeUnknown
        || row.attempt_status == ProviderAttemptStatus::ReconciledPresent
    {
        RetrySafety::OutcomeUnknown
    } else if independent_absence || row.attempt_status == ProviderAttemptStatus::ReconciledAbsent {
        RetrySafety::IndependentlyProvenAbsent
    } else if row.provider_idempotency_key.is_some() {
        RetrySafety::Idempotent
    } else {
        RetrySafety::NonIdempotentFailure
    };
    let declared_safe_boundary_reached = independent_absence
        && row.declared_recovery_safe_boundary == DeclaredRecoverySafeBoundary::IndependentAbsence;
    let normalized_intent_digest = domain_digest(
        b"carsinos.execass.normalized-intent.v1\0",
        row.normalized_original_intent.as_bytes(),
    );
    let effective_authority_digest = domain_digest(
        b"carsinos.execass.effective-authority.v1\0",
        row.effective_authority_json.as_bytes(),
    );
    let recovery_episode_id = domain_digest(
        b"carsinos.execass.recovery-episode.v1\0",
        logical_effect_id.as_bytes(),
    );
    let existing_episode_binding = tx
        .query_row(
            r#"SELECT normalized_intent_digest,effective_authority_digest,action_id,manifest_digest
               FROM execass_recovery_episodes WHERE recovery_episode_id=?1"#,
            [&recovery_episode_id],
            |record| {
                Ok((
                    record.get::<_, String>(0)?,
                    record.get::<_, String>(1)?,
                    record.get::<_, String>(2)?,
                    record.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let original_intent_and_authority_unchanged =
        existing_episode_binding.as_ref().is_none_or(|binding| {
            binding.0 == normalized_intent_digest
                && binding.1 == effective_authority_digest
                && binding.2 == row.action_id
                && binding.3 == row.manifest_digest
        });
    let replan_candidate_count: i64 = tx.query_row(
        r#"SELECT COUNT(*) FROM execass_action_branches branch
           JOIN execass_delegations delegation ON delegation.delegation_id=branch.delegation_id
           WHERE branch.delegation_id=?1 AND branch.branch_kind='recovery'
             AND branch.status IN ('waiting','runnable')
             AND branch.target_plan_revision=delegation.current_plan_revision
             AND branch.stop_epoch=delegation.stop_epoch
             AND branch.action_id!=?2 AND branch.created_at<=?3"#,
        params![row.delegation_id, row.action_id, row.first_attempt_at],
        |record| record.get(0),
    )?;
    if replan_candidate_count > 1 {
        bail!("objective recovery found multiple canonical replan candidates");
    }
    let meaningful_outcome_exists = current_material_verifier_counts(tx, &row.delegation_id)?
        .is_some_and(|counts| counts.0 > 0);
    let user_judgment_useful = tx.query_row(
        r#"SELECT EXISTS(
             SELECT 1 FROM execass_attention_items
             WHERE delegation_id=?1 AND kind='recovery_choice' AND status='actionable'
           ) OR EXISTS(
             SELECT 1 FROM execass_decisions
             WHERE delegation_id=?1 AND decision_kind IN ('recovery_choice','duplicate_risk_retry')
               AND status='pending'
           )"#,
        [&row.delegation_id],
        |record| record.get::<_, i64>(0),
    )? == 1;
    let external_progress_possible = retry_safety == RetrySafety::OutcomeUnknown
        && row.provider_identity.is_some()
        && tx.query_row(
            r#"SELECT EXISTS(
                 SELECT 1 FROM execass_effect_recorder_keys WHERE status='active'
               ) AND EXISTS(
                 SELECT 1 FROM execass_logical_effects
                 WHERE logical_effect_id=?1 AND reconciliation_key IS NOT NULL
               )"#,
            [logical_effect_id],
            |record| record.get::<_, i64>(0),
        )? == 1;
    let provider_error_class = match row.provider_error_class {
        Some(ProviderFailureClass::Transient) => ProviderErrorClass::Transient,
        Some(ProviderFailureClass::RateLimited) => ProviderErrorClass::RateLimited,
        Some(ProviderFailureClass::Authentication) => ProviderErrorClass::Authentication,
        Some(ProviderFailureClass::Permanent) => ProviderErrorClass::Permanent,
        Some(ProviderFailureClass::Unknown) | None => ProviderErrorClass::Unknown,
    };
    let facts = ObjectiveRecoveryFacts {
        scope,
        attempts_started: u32::try_from(row.attempts_started)
            .context("provider attempt count exceeds recovery range")?,
        first_attempt_at_ms: row.first_attempt_at,
        last_attempt_at_ms: row.last_attempt_at,
        now_ms: trusted_now,
        technical_resources_available,
        circuit_open_until_ms,
        provider_error_class,
        retry_safety,
        operation_reversible: row.operation_reversible,
        declared_safe_boundary_reached,
        replan_available_within_original_authority: replan_candidate_count == 1
            && original_intent_and_authority_unchanged,
        original_intent_and_authority_unchanged,
        meaningful_outcome_exists,
        user_judgment_useful,
        external_progress_possible,
    };
    let directive =
        plan_objective_recovery(policy, facts).map_err(|error| anyhow::anyhow!(error))?;
    let policy_json = serde_json::to_string(&policy)
        .context("failed canonicalizing objective recovery policy")?;
    let policy_digest = domain_digest(
        b"carsinos.execass.recovery-policy.v1\0",
        policy_json.as_bytes(),
    );
    let objective_facts_json =
        serde_json::to_string(&facts).context("failed canonicalizing objective recovery facts")?;
    let objective_facts_digest = facts_digest(&row.delegation_id, policy, facts)?;
    let directive_json = serde_json::to_string(&directive)
        .context("failed canonicalizing objective recovery directive")?;
    let directive_digest = domain_digest(
        b"carsinos.execass.recovery-directive.v1\0",
        directive_json.as_bytes(),
    );
    let accepted_confirmation_grant_ids = tx
        .prepare(
            r#"SELECT grant_id FROM execass_accepted_confirmation_grants
               WHERE delegation_id=?1 AND confirmed_logical_action_identity=?2
                 AND invalidated_at IS NULL
               ORDER BY grant_id"#,
        )?
        .query_map(params![row.delegation_id, row.action_id], |record| {
            record.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let accepted_confirmation_grant_id = match accepted_confirmation_grant_ids.as_slice() {
        [] => None,
        [grant_id] => Some(grant_id.clone()),
        _ => bail!(
            "objective recovery found multiple active confirmation grants for one logical action"
        ),
    };
    tx.execute(
        r#"INSERT OR IGNORE INTO execass_recovery_episodes(
                 recovery_episode_id,delegation_id,logical_effect_id,initial_attempt_id,
                 action_id,manifest_digest,normalized_intent_digest,effective_authority_digest,
                 accepted_confirmation_grant_id,policy_json,policy_digest,opened_at
               ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
        params![
            recovery_episode_id,
            row.delegation_id,
            logical_effect_id,
            row.initial_attempt_id,
            row.action_id,
            row.manifest_digest,
            normalized_intent_digest,
            effective_authority_digest,
            accepted_confirmation_grant_id,
            policy_json,
            policy_digest,
            trusted_now,
        ],
    )?;
    let persisted_episode: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
    ) = tx.query_row(
        r#"SELECT delegation_id,logical_effect_id,initial_attempt_id,action_id,
                      manifest_digest,normalized_intent_digest,effective_authority_digest,
                      accepted_confirmation_grant_id,policy_json,policy_digest
               FROM execass_recovery_episodes WHERE recovery_episode_id=?1"#,
        [&recovery_episode_id],
        |record| {
            Ok((
                record.get(0)?,
                record.get(1)?,
                record.get(2)?,
                record.get(3)?,
                record.get(4)?,
                record.get(5)?,
                record.get(6)?,
                record.get(7)?,
                record.get(8)?,
                record.get(9)?,
            ))
        },
    )?;
    let expected_episode = (
        row.delegation_id.clone(),
        logical_effect_id.to_owned(),
        row.initial_attempt_id.clone(),
        row.action_id.clone(),
        row.manifest_digest.clone(),
        normalized_intent_digest.clone(),
        effective_authority_digest.clone(),
        accepted_confirmation_grant_id.clone(),
        policy_json.clone(),
        policy_digest.clone(),
    );
    if persisted_episode.0 != expected_episode.0
        || persisted_episode.1 != expected_episode.1
        || persisted_episode.2 != expected_episode.2
        || persisted_episode.3 != expected_episode.3
        || persisted_episode.4 != expected_episode.4
        || (existing_episode_binding.is_none() && persisted_episode != expected_episode)
    {
        bail!("persisted recovery episode does not match canonical source state");
    }
    let existing_evaluation = tx
        .query_row(
            r#"SELECT recovery_evaluation_id,recovery_state_revision FROM execass_recovery_evaluations
                   WHERE recovery_episode_id=?1 AND objective_facts_digest=?2
                     AND directive_digest=?3"#,
            params![
                recovery_episode_id,
                objective_facts_digest,
                directive_digest
            ],
            |record| Ok((record.get::<_, String>(0)?, record.get::<_, i64>(1)?)),
        )
        .optional()?;
    let (recovery_evaluation_id, committed_recovery_state_revision) = if let Some(existing) =
        existing_evaluation
    {
        existing
    } else {
        let revision: i64 = tx.query_row(
                "SELECT COALESCE(MAX(evaluation_revision),0)+1 FROM execass_recovery_evaluations WHERE recovery_episode_id=?1",
                [&recovery_episode_id],
                |record| record.get(0),
            )?;
        let id = domain_digest(
            b"carsinos.execass.recovery-evaluation.v1\0",
            format!(
                "{recovery_episode_id}\0{revision}\0{objective_facts_digest}\0{directive_digest}"
            )
            .as_bytes(),
        );
        let (directive_name, not_before_ms) = directive_projection(directive);
        let outbox_event_id = provided_event_identity
            .map(|identity| identity.event_id.to_owned())
            .unwrap_or_else(|| {
                domain_digest(b"carsinos.execass.recovery-event.v1\0", id.as_bytes())
            });
        let correlation_id = provided_event_identity
            .map(|identity| identity.correlation_id)
            .unwrap_or(recovery_episode_id.as_str());
        let causation_id = provided_event_identity
            .map(|identity| identity.causation_id)
            .unwrap_or(id.as_str());
        let recovery_state_revision = recovery_state_revision.unwrap_or(row.state_revision);
        let safe_payload_json = serde_json::json!({
            "recovery_episode_id": recovery_episode_id.clone(),
            "recovery_evaluation_id": id.clone(),
            "logical_effect_id": logical_effect_id,
            "predecessor_attempt_id": row.attempt_id.clone(),
            "objective_facts_digest": objective_facts_digest.clone(),
            "directive": directive_name,
            "directive_digest": directive_digest.clone(),
            "not_before_ms": not_before_ms,
        })
        .to_string();
        tx.execute(
            r#"INSERT INTO execass_outbox_events(
                     event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
                     causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
                   ) VALUES(?1,'execass.v1.recovery.updated',?2,?3,?4,?5,?6,'v1',?7,?1)"#,
            params![
                outbox_event_id,
                row.delegation_id,
                recovery_state_revision,
                correlation_id,
                causation_id,
                trusted_now,
                safe_payload_json,
            ],
        )?;
        tx.execute(
                r#"INSERT INTO execass_recovery_evaluations(
                     recovery_evaluation_id,recovery_episode_id,delegation_id,logical_effect_id,
                     predecessor_attempt_id,evaluation_revision,recovery_state_revision,objective_facts_json,
                     objective_facts_digest,directive,directive_json,directive_digest,
                     not_before_ms,outbox_event_id,evaluated_at
                   ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)"#,
                params![
                    id,
                    recovery_episode_id,
                    row.delegation_id,
                    logical_effect_id,
                    row.attempt_id,
                    revision,
                    recovery_state_revision,
                    objective_facts_json,
                    objective_facts_digest,
                    directive_name,
                    directive_json,
                    directive_digest,
                    not_before_ms,
                    outbox_event_id,
                    trusted_now,
                ],
            )?;
        (id, recovery_state_revision)
    };
    let retry_authorization = match directive {
        RecoveryDirective::RetrySameEffect { not_before_ms } => Some(ProviderRetryAuthorization {
            recovery_evaluation_id: recovery_evaluation_id.clone(),
            logical_effect_id: logical_effect_id.to_owned(),
            predecessor_attempt_id: row.attempt_id,
            authorized_attempt_number: row.attempt_number + 1,
            not_before_ms,
            objective_facts_digest: objective_facts_digest.clone(),
            recovery_state_revision: committed_recovery_state_revision,
        }),
        _ => None,
    };
    let evaluation = ObjectiveRecoveryEvaluation {
        recovery_evaluation_id,
        directive,
        retry_authorization,
        objective_facts_digest,
        recovery_state_revision: committed_recovery_state_revision,
    };
    Ok(evaluation)
}

fn project_recovery_lifecycle(
    tx: &Transaction<'_>,
    command: &ProviderRecoveryCommand,
    evaluation: &ObjectiveRecoveryEvaluation,
    post_revision: i64,
) -> Result<DelegationPhase> {
    let (
        delegation_id,
        previous_phase,
        run_control,
        current_plan_revision,
        current_criteria_revision,
        stop_epoch,
        action_id,
        first_attempt_at,
    ): (
        String,
        DelegationPhase,
        String,
        Option<i64>,
        Option<i64>,
        i64,
        String,
        i64,
    ) = tx
        .query_row(
            r#"SELECT d.delegation_id,d.phase,d.run_control,d.current_plan_revision,
                      d.current_criteria_revision,d.stop_epoch,c.action_id,a.started_at
               FROM execass_logical_effects e
               JOIN execass_delegations d ON d.delegation_id=e.delegation_id
               JOIN execass_continuations c ON c.continuation_id=e.continuation_id
               JOIN execass_provider_attempts a
                 ON a.logical_effect_id=e.logical_effect_id AND a.attempt_number=1
               WHERE e.logical_effect_id=?1"#,
            [&command.logical_effect_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .context("failed reading recovery lifecycle source")?;
    let current_plan_revision =
        current_plan_revision.context("recovery delegation has no current plan")?;
    if previous_phase.is_terminal()
        || run_control != "running"
        || post_revision != command.expected_pre_state_revision + 1
    {
        bail!("recovery lifecycle source is not a current running nonterminal delegation");
    }

    let selected_phase = match evaluation.directive {
        RecoveryDirective::RetrySameEffect { .. }
        | RecoveryDirective::WaitUntil { .. }
        | RecoveryDirective::ReplanWithinOriginalAuthority => DelegationPhase::Recovering,
        RecoveryDirective::WaitingExternal => DelegationPhase::WaitingExternal,
        RecoveryDirective::WaitingForUser => DelegationPhase::WaitingForUser,
        RecoveryDirective::PartiallyCompleted => DelegationPhase::PartiallyCompleted,
        RecoveryDirective::Failed => DelegationPhase::Failed,
    };
    let mut pending_decision_id: Option<String> = None;
    let mut external_wait_json: Option<String> = None;
    let mut completion_assessment_json: Option<String> = None;

    let (source_action_status, source_action_terminal_at) = match evaluation.directive {
        RecoveryDirective::ReplanWithinOriginalAuthority => {
            ("superseded", Some(command.trusted_now))
        }
        RecoveryDirective::WaitingExternal => ("uncertain", None),
        RecoveryDirective::PartiallyCompleted | RecoveryDirective::Failed => {
            ("terminal", Some(command.trusted_now))
        }
        RecoveryDirective::RetrySameEffect { .. }
        | RecoveryDirective::WaitUntil { .. }
        | RecoveryDirective::WaitingForUser => ("waiting", None),
    };
    let changed = tx.execute(
        r#"UPDATE execass_action_branches
           SET status=?1,updated_at=?2,terminal_at=?3
           WHERE action_id=?4 AND status NOT IN ('terminal','superseded')"#,
        params![
            source_action_status,
            command.trusted_now,
            source_action_terminal_at,
            action_id,
        ],
    )?;
    if changed != 1 {
        bail!("recovery source action is no longer active");
    }

    match evaluation.directive {
        RecoveryDirective::RetrySameEffect { .. } | RecoveryDirective::WaitUntil { .. } => {
            tx.execute(
                r#"UPDATE execass_action_branches
                   SET status='superseded',updated_at=?1,terminal_at=?1
                   WHERE delegation_id=?2 AND branch_kind='recovery'
                     AND created_at>?3 AND status NOT IN ('terminal','superseded')"#,
                params![command.trusted_now, delegation_id, first_attempt_at],
            )?;
            let recovery_action_id = domain_digest(
                b"carsinos.execass.recovery-action.v1\0",
                evaluation.recovery_evaluation_id.as_bytes(),
            );
            let action_revision: i64 = tx.query_row(
                "SELECT COALESCE(MAX(action_revision),0)+1 FROM execass_action_branches WHERE delegation_id=?1",
                [&delegation_id],
                |row| row.get(0),
            )?;
            tx.execute(
                r#"INSERT INTO execass_action_branches(
                     action_id,delegation_id,action_revision,target_delegation_revision,
                     target_plan_revision,stop_epoch,branch_kind,status,action_summary,
                     created_at,updated_at,terminal_at
                   ) VALUES(?1,?2,?3,?4,?5,?6,'recovery','runnable',
                     'Objective recovery for existing logical effect',?7,?7,NULL)"#,
                params![
                    recovery_action_id,
                    delegation_id,
                    action_revision,
                    post_revision,
                    current_plan_revision,
                    stop_epoch,
                    command.trusted_now,
                ],
            )?;
        }
        RecoveryDirective::ReplanWithinOriginalAuthority => {
            tx.execute(
                r#"UPDATE execass_action_branches
                   SET status='superseded',updated_at=?1,terminal_at=?1
                   WHERE delegation_id=?2 AND branch_kind='recovery'
                     AND created_at>?3 AND status NOT IN ('terminal','superseded')"#,
                params![command.trusted_now, delegation_id, first_attempt_at],
            )?;
            let candidate_id: String = tx
                .query_row(
                    r#"SELECT action_id FROM execass_action_branches
                       WHERE delegation_id=?1 AND branch_kind='recovery'
                         AND status IN ('waiting','runnable')
                         AND target_plan_revision=?2 AND stop_epoch=?3
                         AND action_id!=?4 AND created_at<=?5"#,
                    params![
                        delegation_id,
                        current_plan_revision,
                        stop_epoch,
                        action_id,
                        first_attempt_at,
                    ],
                    |row| row.get(0),
                )
                .context("canonical replan candidate disappeared before projection")?;
            let changed = tx.execute(
                r#"UPDATE execass_action_branches
                   SET status='runnable',updated_at=?1,terminal_at=NULL
                   WHERE action_id=?2 AND status IN ('waiting','runnable')"#,
                params![command.trusted_now, candidate_id],
            )?;
            if changed != 1 {
                bail!("canonical replan candidate could not be promoted");
            }
        }
        RecoveryDirective::WaitingExternal => {
            let wait_id = domain_digest(
                b"carsinos.execass.recovery-wait.v1\0",
                evaluation.recovery_evaluation_id.as_bytes(),
            );
            let details = serde_json::json!({
                "recovery_evaluation_id": evaluation.recovery_evaluation_id,
                "logical_effect_id": command.logical_effect_id,
                "objective_facts_digest": evaluation.objective_facts_digest,
            })
            .to_string();
            tx.execute(
                r#"INSERT INTO execass_external_waits(
                     external_wait_id,delegation_id,action_id,kind,status,reason,
                     details_json,delegation_revision,created_at,resolved_at
                   ) VALUES(?1,?2,?3,'system','waiting',
                     'Independent effect reconciliation is pending',?4,?5,?6,NULL)"#,
                params![
                    wait_id,
                    delegation_id,
                    action_id,
                    details,
                    post_revision,
                    command.trusted_now,
                ],
            )?;
            external_wait_json = Some(details);
        }
        RecoveryDirective::WaitingForUser => {
            let existing_attention: i64 = tx.query_row(
                r#"SELECT COUNT(*) FROM execass_attention_items
                   WHERE delegation_id=?1 AND kind='recovery_choice' AND status='actionable'"#,
                [&delegation_id],
                |row| row.get(0),
            )?;
            if existing_attention == 0 {
                let attention_id = domain_digest(
                    b"carsinos.execass.recovery-attention.v1\0",
                    evaluation.recovery_evaluation_id.as_bytes(),
                );
                tx.execute(
                    r#"INSERT INTO execass_attention_items(
                     attention_id,delegation_id,action_id,kind,status,reason,recommendation,
                     alternatives_json,required_assurance,decision_id,delegation_revision,
                     created_at,resolved_at
                   ) VALUES(?1,?2,?3,'recovery_choice','actionable',
                     'No safe autonomous recovery path remains',
                     'Review the bounded recovery choices','[]','human_local_or_remote',
                     NULL,?4,?5,NULL)"#,
                    params![
                        attention_id,
                        delegation_id,
                        action_id,
                        post_revision,
                        command.trusted_now,
                    ],
                )?;
            } else if existing_attention != 1 {
                bail!("recovery found multiple actionable recovery choices");
            }
            pending_decision_id = None;
        }
        RecoveryDirective::PartiallyCompleted | RecoveryDirective::Failed => {
            let criteria_revision = current_criteria_revision
                .context("terminal recovery requires current outcome criteria")?;
            let mut statement = tx.prepare(
                r#"SELECT c.criterion_key,COALESCE((
                     SELECT v.result FROM execass_verifier_results v
                     WHERE v.delegation_id=c.delegation_id AND v.criterion_id=c.criterion_id
                     ORDER BY v.result_revision DESC LIMIT 1
                   ),'unknown')
                   FROM execass_outcome_criteria c
                   WHERE c.delegation_id=?1 AND c.criteria_revision=?2 AND c.material=1
                   ORDER BY c.criterion_key"#,
            )?;
            let results = statement
                .query_map(params![delegation_id, criteria_revision], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let pass_count = results
                .iter()
                .filter(|(_, result)| result == "pass")
                .count() as i64;
            let fail_count = results
                .iter()
                .filter(|(_, result)| result == "fail")
                .count() as i64;
            let unknown_count = results
                .iter()
                .filter(|(_, result)| result == "unknown")
                .count() as i64;
            let unmet = results
                .iter()
                .filter(|(_, result)| result != "pass")
                .map(|(criterion, _)| criterion.clone())
                .collect::<Vec<_>>();
            let useful_outcome =
                matches!(evaluation.directive, RecoveryDirective::PartiallyCompleted);
            if useful_outcome != (pass_count > 0) {
                bail!("recovery terminal directive conflicts with authoritative verifier results");
            }
            let assessment_id = domain_digest(
                b"carsinos.execass.recovery-assessment.v1\0",
                evaluation.recovery_evaluation_id.as_bytes(),
            );
            let assessment_revision: i64 = tx.query_row(
                "SELECT COALESCE(MAX(assessment_revision),0)+1 FROM execass_completion_assessments WHERE delegation_id=?1",
                [&delegation_id],
                |row| row.get(0),
            )?;
            let assessment = serde_json::json!({
                "recovery_evaluation_id": evaluation.recovery_evaluation_id,
                "material_pass_count": pass_count,
                "material_fail_count": fail_count,
                "material_unknown_count": unknown_count,
                "exact_unmet_criteria": unmet,
                "no_remaining_path": true,
            })
            .to_string();
            let exact_unmet = useful_outcome
                .then(|| serde_json::to_string(&unmet))
                .transpose()?;
            tx.execute(
                r#"INSERT INTO execass_completion_assessments(
                     assessment_id,delegation_id,assessment_revision,criteria_revision,
                     terminal_phase,material_pass_count,material_fail_count,material_unknown_count,
                     useful_outcome,exact_unmet_portion,no_remaining_path,assessment_json,assessed_at
                   ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1,?11,?12)"#,
                params![
                    assessment_id,
                    delegation_id,
                    assessment_revision,
                    criteria_revision,
                    selected_phase.as_str(),
                    pass_count,
                    fail_count,
                    unknown_count,
                    useful_outcome as i64,
                    exact_unmet,
                    assessment,
                    command.trusted_now,
                ],
            )?;
            completion_assessment_json = Some(assessment);
        }
    }

    let terminal_at = selected_phase.is_terminal().then_some(command.trusted_now);
    let changed = tx.execute(
        r#"UPDATE execass_delegations
           SET phase=?1,state_revision=?2,pending_decision_id=?3,external_wait_json=?4,
               completion_assessment_json=?5,updated_at=?6,terminal_at=?7
           WHERE delegation_id=?8 AND state_revision=?9 AND run_control='running'"#,
        params![
            selected_phase.as_str(),
            post_revision,
            pending_decision_id,
            external_wait_json,
            completion_assessment_json,
            command.trusted_now,
            terminal_at,
            delegation_id,
            command.expected_pre_state_revision,
        ],
    )?;
    if changed != 1 {
        bail!("recovery lifecycle CAS lost its canonical delegation revision");
    }
    let selector_json = serde_json::json!({
        "completion_assessment": selected_phase.is_terminal().then(|| selected_phase.as_str()),
        "pre_actionable_phase": null,
        "ordinary_runnable_or_executing": false,
        "recovery_runnable_or_executing": selected_phase == DelegationPhase::Recovering,
        "actionable_attention": selected_phase == DelegationPhase::WaitingForUser,
        "external_wait": selected_phase == DelegationPhase::WaitingExternal,
    })
    .to_string();
    let transition_id = domain_digest(
        b"carsinos.execass.recovery-transition.v1\0",
        evaluation.recovery_evaluation_id.as_bytes(),
    );
    let snapshot = super::lifecycle::projection_snapshot_json(tx, &delegation_id, None)?;
    tx.execute(
        r#"INSERT INTO execass_lifecycle_transitions(
             transition_id,delegation_id,state_revision,previous_phase,selected_phase,
             previous_run_control,selected_run_control,selector_input_json,command_identity,
             projection_snapshot_json,reason,outbox_event_id,occurred_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?6,?7,?8,?9,
             'objective_recovery',?10,?11)"#,
        params![
            transition_id,
            delegation_id,
            post_revision,
            previous_phase.as_str(),
            selected_phase.as_str(),
            run_control,
            selector_json,
            format!("objective-recovery:{}", evaluation.recovery_evaluation_id),
            snapshot,
            command.receipt.causation_event_id,
            command.trusted_now,
        ],
    )?;
    Ok(selected_phase)
}

fn load_recovery_replay(
    tx: &Transaction<'_>,
    command: &ProviderRecoveryCommand,
) -> Result<ProviderRecoveryBundle> {
    let (
        recovery_evaluation_id,
        logical_effect_id,
        predecessor_attempt_id,
        recovery_state_revision,
        objective_facts_digest,
        directive_json,
        not_before_ms,
        attempt_number,
        selected_phase,
    ): (
        String,
        String,
        String,
        i64,
        String,
        String,
        Option<i64>,
        i64,
        DelegationPhase,
    ) = tx
        .query_row(
            r#"SELECT e.recovery_evaluation_id,e.logical_effect_id,e.predecessor_attempt_id,
                      e.recovery_state_revision,e.objective_facts_digest,
                      e.directive_json,e.not_before_ms,a.attempt_number,d.phase
               FROM execass_recovery_evaluations e
               JOIN execass_provider_attempts a ON a.attempt_id=e.predecessor_attempt_id
               JOIN execass_delegations d ON d.delegation_id=e.delegation_id
               WHERE e.outbox_event_id=?1"#,
            [&command.receipt.causation_event_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .context("atomic recovery replay evaluation is missing")?;
    if logical_effect_id != command.logical_effect_id
        || recovery_state_revision != command.receipt.expected_state_revision
        || command.receipt.subject.revision != recovery_state_revision
    {
        bail!("atomic recovery replay identity conflicts with the original write");
    }
    let directive: RecoveryDirective = serde_json::from_str(&directive_json)
        .context("stored recovery replay directive is invalid")?;
    let retry_authorization = match directive {
        RecoveryDirective::RetrySameEffect {
            not_before_ms: directive_not_before,
        } => {
            if Some(directive_not_before) != not_before_ms {
                bail!("stored retry directive timing conflicts with its projection");
            }
            Some(ProviderRetryAuthorization {
                recovery_evaluation_id: recovery_evaluation_id.clone(),
                logical_effect_id: logical_effect_id.clone(),
                predecessor_attempt_id,
                authorized_attempt_number: attempt_number + 1,
                not_before_ms: directive_not_before,
                objective_facts_digest: objective_facts_digest.clone(),
                recovery_state_revision,
            })
        }
        _ => None,
    };
    let outbox_event = get_outbox(tx, &command.receipt.causation_event_id)?
        .context("atomic recovery replay outbox is missing")?;
    let receipt = load_receipt(tx, &command.receipt.receipt_id)?
        .context("atomic recovery replay receipt is missing")?;
    Ok(ProviderRecoveryBundle {
        evaluation: ObjectiveRecoveryEvaluation {
            recovery_evaluation_id,
            directive,
            retry_authorization,
            objective_facts_digest,
            recovery_state_revision,
        },
        selected_phase,
        state_revision: recovery_state_revision,
        outbox_event,
        receipt,
    })
}

fn validate_recovery_command(command: &ProviderRecoveryCommand) -> Result<()> {
    require_text("logical_effect_id", &command.logical_effect_id)?;
    if command.trusted_now <= 0
        || command.expected_pre_state_revision <= 0
        || command.write.occurred_at != command.trusted_now
        || command.receipt.receipt_kind != super::types::ReceiptKind::Recovery
        || command.receipt.subject.kind != super::types::ReceiptSubjectKind::OutboxEvent
        || command.receipt.subject.subject_id != command.receipt.causation_event_id
        || command.receipt.subject.revision != command.expected_pre_state_revision + 1
        || command.receipt.expected_state_revision != command.expected_pre_state_revision + 1
        || command.receipt.occurred_at != command.trusted_now
        || command.receipt.committed_at < command.trusted_now
        || command.receipt.causation_id != command.write.causation_id
    {
        bail!("atomic provider recovery command identities are invalid");
    }
    Ok(())
}

struct RecoveryRow {
    delegation_id: String,
    operation_reversible: bool,
    declared_recovery_safe_boundary: DeclaredRecoverySafeBoundary,
    effect_state: LogicalEffectState,
    provider_identity: Option<String>,
    provider_idempotency_key: Option<String>,
    effective_authority_json: String,
    attempt_id: String,
    attempt_number: i64,
    attempt_status: ProviderAttemptStatus,
    provider_error_class: Option<ProviderFailureClass>,
    last_attempt_at: i64,
    first_attempt_at: i64,
    attempts_started: i64,
    normalized_original_intent: String,
    action_id: String,
    manifest_digest: String,
    state_revision: i64,
    initial_attempt_id: String,
}

fn current_material_verifier_counts(
    tx: &Transaction<'_>,
    delegation_id: &str,
) -> Result<Option<(i64, i64, i64)>> {
    let criteria_revision = tx
        .query_row(
            "SELECT current_criteria_revision FROM execass_delegations WHERE delegation_id=?1",
            [delegation_id],
            |record| record.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    let Some(criteria_revision) = criteria_revision else {
        return Ok(None);
    };
    tx.query_row(
        r#"WITH current_results AS (
             SELECT criterion.criterion_id,
                    COALESCE((SELECT result.result FROM execass_verifier_results result
                              WHERE result.delegation_id=criterion.delegation_id
                                AND result.criterion_id=criterion.criterion_id
                              ORDER BY result.result_revision DESC LIMIT 1),'unknown') AS result
             FROM execass_outcome_criteria criterion
             WHERE criterion.delegation_id=?1 AND criterion.criteria_revision=?2
               AND criterion.material=1
           )
           SELECT COALESCE(SUM(result='pass'),0),
                  COALESCE(SUM(result='fail'),0),
                  COALESCE(SUM(result='unknown'),0)
           FROM current_results"#,
        params![delegation_id, criteria_revision],
        |record| Ok((record.get(0)?, record.get(1)?, record.get(2)?)),
    )
    .map(Some)
    .map_err(Into::into)
}

fn recovery_policy(authority_json: &str) -> (RecoveryScope, RecoveryPolicy) {
    let value = serde_json::from_str::<serde_json::Value>(authority_json).unwrap_or_default();
    let recovery = value
        .get("recovery")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("profile").and_then(serde_json::Value::as_str));
    match recovery {
        Some("objective_retry_within_owner_envelope" | "full_send") => (
            RecoveryScope::ObjectiveRetryWithinOwnerEnvelope,
            RecoveryPolicy {
                max_attempts: 8,
                max_elapsed_ms: 900_000,
                base_backoff_ms: 1_000,
                max_backoff_ms: 60_000,
            },
        ),
        Some("objective_retry" | "balanced") => (
            RecoveryScope::ObjectiveRetry,
            RecoveryPolicy {
                max_attempts: 4,
                max_elapsed_ms: 300_000,
                base_backoff_ms: 1_000,
                max_backoff_ms: 30_000,
            },
        ),
        _ => (
            RecoveryScope::ManualOnly,
            RecoveryPolicy {
                max_attempts: 1,
                max_elapsed_ms: 0,
                base_backoff_ms: 0,
                max_backoff_ms: 0,
            },
        ),
    }
}

fn facts_digest(
    delegation_id: &str,
    policy: RecoveryPolicy,
    facts: ObjectiveRecoveryFacts,
) -> Result<String> {
    let bytes = serde_json::to_vec(&(delegation_id, policy, facts))
        .context("failed canonicalizing objective recovery facts")?;
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.objective-recovery-facts.v1\0");
    digest.update(bytes);
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn domain_digest(domain: &[u8], value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value);
    format!("sha256:{:x}", digest.finalize())
}

fn directive_projection(directive: RecoveryDirective) -> (&'static str, Option<i64>) {
    match directive {
        RecoveryDirective::RetrySameEffect { not_before_ms } => {
            ("retry_same_effect", Some(not_before_ms))
        }
        RecoveryDirective::ReplanWithinOriginalAuthority => {
            ("replan_within_original_authority", None)
        }
        RecoveryDirective::WaitUntil {
            not_before_ms,
            reason: carsinos_core::execass_recovery::RecoveryDelayReason::Backoff,
        } => ("wait_backoff", Some(not_before_ms)),
        RecoveryDirective::WaitUntil {
            not_before_ms,
            reason: carsinos_core::execass_recovery::RecoveryDelayReason::CircuitBreaker,
        } => ("wait_circuit_breaker", Some(not_before_ms)),
        RecoveryDirective::WaitingExternal => ("waiting_external", None),
        RecoveryDirective::WaitingForUser => ("waiting_for_user", None),
        RecoveryDirective::PartiallyCompleted => ("partially_completed", None),
        RecoveryDirective::Failed => ("failed", None),
    }
}
