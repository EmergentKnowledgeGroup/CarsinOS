//! Deterministic, read-only executive projections over authoritative ExecAss state.
//!
//! This module never executes work, appends receipts, publishes outbox events,
//! or writes delivery/acknowledgement state. Both public entry points call the
//! same builder over one SQLite read snapshot.

use super::receipt_integrity::{IntegrityStatus, ReceiptIntegrityStore};
use super::redaction::ReceiptRedactor;
use super::store::ExecAssStore;
use super::types::*;
use anyhow::{bail, Context, Result};
use rusqlite::{Transaction, TransactionBehavior};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const PROJECTION_VERSION: &str = "execass_projection.v1";
const SNAPSHOT_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct IntegrityFingerprint {
    failure: Option<ProjectionIntegrityFailure>,
    anchor_generation: Option<i64>,
    receipt_count: Option<i64>,
    receipt_head_digest: Option<String>,
}

impl IntegrityFingerprint {
    fn from_status(status: IntegrityStatus) -> Self {
        match status {
            IntegrityStatus::Trusted {
                anchor_generation,
                receipt_count,
                receipt_head_digest,
                ..
            } => Self {
                failure: None,
                anchor_generation: Some(anchor_generation),
                receipt_count: Some(receipt_count),
                receipt_head_digest,
            },
            IntegrityStatus::Uninitialized => {
                Self::untrusted(ProjectionIntegrityFailure::Uninitialized)
            }
            IntegrityStatus::Prepared { .. } => {
                Self::untrusted(ProjectionIntegrityFailure::Prepared)
            }
            IntegrityStatus::KeyLost { .. } => Self::untrusted(ProjectionIntegrityFailure::KeyLost),
            IntegrityStatus::Mismatch { .. } => {
                Self::untrusted(ProjectionIntegrityFailure::Mismatch)
            }
            IntegrityStatus::Quarantined { .. } => {
                Self::untrusted(ProjectionIntegrityFailure::Quarantined)
            }
        }
    }

    fn untrusted(failure: ProjectionIntegrityFailure) -> Self {
        Self {
            failure: Some(failure),
            anchor_generation: None,
            receipt_count: None,
            receipt_head_digest: None,
        }
    }

    fn projection(&self, snapshot: &SourceBoundary) -> ProjectionIntegrity {
        match self.failure {
            None if self.receipt_count == Some(snapshot.receipt_count)
                && self.receipt_head_digest == snapshot.receipt_head_digest =>
            {
                ProjectionIntegrity::Trusted {
                    anchor_generation: self.anchor_generation.unwrap_or_default(),
                    receipt_count: snapshot.receipt_count,
                    receipt_head_digest: snapshot.receipt_head_digest.clone(),
                }
            }
            None => ProjectionIntegrity::Untrusted {
                failure: ProjectionIntegrityFailure::Mismatch,
            },
            Some(failure) => ProjectionIntegrity::Untrusted { failure },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceBoundary {
    through_global_sequence: i64,
    receipt_count: i64,
    receipt_head_digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredUnmetCriterion {
    criterion_id: String,
    criterion_key: String,
    result: String,
}

impl ExecAssStore {
    pub fn read_authoritative_projection(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        query: &ExecAssProjectionQuery,
    ) -> Result<ExecAssExecutiveProjection> {
        self.project_authoritative_state(integrity, redactor, query)
    }

    pub fn rebuild_authoritative_projection(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        query: &ExecAssProjectionQuery,
    ) -> Result<ExecAssExecutiveProjection> {
        self.project_authoritative_state(integrity, redactor, query)
    }

    fn project_authoritative_state(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        query: &ExecAssProjectionQuery,
    ) -> Result<ExecAssExecutiveProjection> {
        validate_query(query)?;
        for _ in 0..SNAPSHOT_ATTEMPTS {
            let before = IntegrityFingerprint::from_status(integrity.status()?);
            let (mut projection, source) =
                self.read_projection_snapshot(redactor, query, &before)?;
            let after = IntegrityFingerprint::from_status(integrity.status()?);
            if before == after {
                projection.integrity = before.projection(&source);
                apply_trust(&mut projection);
                projection.boundary.item_set_digest = item_set_digest(&projection)?;
                return Ok(projection);
            }
        }

        let downgraded =
            IntegrityFingerprint::untrusted(ProjectionIntegrityFailure::ConcurrentMovement);
        let (mut projection, _) = self.read_projection_snapshot(redactor, query, &downgraded)?;
        projection.integrity = ProjectionIntegrity::Untrusted {
            failure: ProjectionIntegrityFailure::ConcurrentMovement,
        };
        apply_trust(&mut projection);
        projection.boundary.item_set_digest = item_set_digest(&projection)?;
        Ok(projection)
    }

    fn read_projection_snapshot(
        &self,
        redactor: &ReceiptRedactor,
        query: &ExecAssProjectionQuery,
        integrity: &IntegrityFingerprint,
    ) -> Result<(ExecAssExecutiveProjection, SourceBoundary)> {
        let mut connection = self.connection()?;
        connection.pragma_update(None, "query_only", "ON")?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .context("failed starting ExecAss projection read transaction")?;
        let source = read_source_boundary(&transaction)?;
        let projected_integrity = integrity.projection(&source);
        let needs_you = read_needs_you(&transaction, redactor, query.trusted_now_ms)?;
        let in_motion = read_in_motion(&transaction)?;
        let done_since_you_checked =
            read_done(&transaction, redactor, projected_integrity.trust())?;
        let next = read_next(&transaction, query)?;
        let receipts = read_receipts(
            &transaction,
            redactor,
            query.receipt_limit,
            projected_integrity.trust(),
        )?;
        let reef = read_reef(&transaction, query.trusted_now_ms, &projected_integrity)?;
        transaction.commit()?;

        let mut projection = ExecAssExecutiveProjection {
            projection_version: PROJECTION_VERSION.to_owned(),
            observed_at_ms: query.trusted_now_ms,
            boundary: ProjectionBoundary {
                through_global_sequence: source.through_global_sequence,
                database_receipt_count: source.receipt_count,
                database_receipt_head_digest: source.receipt_head_digest.clone(),
                item_set_digest: String::new(),
            },
            integrity: projected_integrity,
            needs_you,
            in_motion,
            done_since_you_checked,
            next,
            receipts,
            reef,
        };
        projection.boundary.item_set_digest = item_set_digest(&projection)?;
        Ok((projection, source))
    }
}

fn validate_query(query: &ExecAssProjectionQuery) -> Result<()> {
    if query.trusted_now_ms <= 0 {
        bail!("projection trusted_now_ms must be positive")
    }
    if query.receipt_limit == 0 || query.receipt_limit > EXECASS_PROJECTION_RECEIPT_LIMIT {
        bail!("projection receipt limit must be between 1 and {EXECASS_PROJECTION_RECEIPT_LIMIT}")
    }
    Ok(())
}

fn read_source_boundary(tx: &Transaction<'_>) -> Result<SourceBoundary> {
    let through_global_sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(global_sequence),0) FROM execass_outbox_events",
        [],
        |row| row.get(0),
    )?;
    let (receipt_count, receipt_head_digest): (i64, Option<String>) = tx.query_row(
        "SELECT receipt_count,receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(SourceBoundary {
        through_global_sequence,
        receipt_count,
        receipt_head_digest,
    })
}

fn read_needs_you(
    tx: &Transaction<'_>,
    redactor: &ReceiptRedactor,
    trusted_now_ms: i64,
) -> Result<Vec<NeedsYouProjectionItem>> {
    let mut statement = tx.prepare(
        r#"SELECT a.attention_id,a.delegation_id,a.kind,a.reason,a.recommendation,
                  a.alternatives_json,a.required_assurance,a.decision_id,
                  a.delegation_revision,a.created_at,d.pending_decision_id,d.phase,
                  decision.decision_kind,decision.delegation_id,decision.decision_revision,
                  decision.delegation_revision,decision.status,
                  challenge.expires_at,challenge.status,
                  (SELECT COUNT(*) FROM execass_confirmation_challenges exact
                   WHERE exact.decision_id=decision.decision_id)
           FROM execass_attention_items a
           JOIN execass_delegations d ON d.delegation_id=a.delegation_id
           LEFT JOIN execass_decisions decision ON decision.decision_id=a.decision_id
           LEFT JOIN execass_confirmation_challenges challenge
             ON challenge.decision_id=decision.decision_id
           WHERE a.status='actionable'
           ORDER BY CASE WHEN challenge.expires_at IS NULL THEN 1 ELSE 0 END,
                    challenge.expires_at,a.created_at,a.attention_id"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<String>>(13)?,
            row.get::<_, Option<i64>>(14)?,
            row.get::<_, Option<i64>>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<i64>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, i64>(19)?,
        ))
    })?;
    let mut output = Vec::new();
    for row in rows {
        let (
            attention_id,
            delegation_id,
            stored_kind,
            reason,
            recommendation,
            alternatives_json,
            required_assurance,
            decision_id,
            delegation_revision,
            created_at_ms,
            pending_decision_id,
            delegation_phase,
            decision_kind,
            decision_delegation_id,
            decision_revision,
            decision_delegation_revision,
            decision_status,
            challenge_expiry,
            challenge_status,
            challenge_count,
        ) = row?;
        if matches!(
            delegation_phase.as_deref(),
            Some("completed" | "partially_completed" | "failed")
        ) {
            bail!("terminal delegation retains actionable attention")
        }
        let alternatives = serde_json::from_str::<serde_json::Value>(&alternatives_json)
            .context("stored attention alternatives are not valid JSON")?;
        let alternative_count = u32::try_from(
            alternatives
                .as_array()
                .context("stored attention alternatives must be an array")?
                .len(),
        )?;
        let alternative_values = alternatives
            .as_array()
            .context("stored attention alternatives must be an array")?
            .iter()
            .map(|value| {
                let value = value
                    .as_str()
                    .context("stored attention alternative must be text")?;
                Ok(redactor.summary(value)?.as_str().to_owned())
            })
            .collect::<Result<Vec<_>>>()?;

        let (kind, decision_kind_value) = match decision_kind.as_deref() {
            None => {
                if stored_kind != "reply"
                    || decision_id.is_some()
                    || decision_revision.is_some()
                    || decision_status.is_some()
                    || challenge_expiry.is_some()
                    || challenge_status.is_some()
                    || challenge_count != 0
                {
                    bail!("actionable reply attention has an invalid decision binding")
                }
                (NeedsYouKind::Reply, None)
            }
            Some(value) => {
                let derived = attention_kind_for_decision(value)?;
                if stored_kind != needs_you_kind_str(derived)
                    || decision_id.is_none()
                    || decision_id.as_deref() != pending_decision_id.as_deref()
                    || decision_delegation_id.as_deref() != Some(delegation_id.as_str())
                    || decision_status.as_deref() != Some("pending")
                    || decision_delegation_revision != Some(delegation_revision)
                    || decision_revision.is_none()
                {
                    bail!("actionable decision attention does not bind the exact pending decision")
                }
                if value == "dangerous_action_confirmation"
                    && (challenge_status.as_deref() != Some("pending")
                        || challenge_count != 1
                        || challenge_expiry.is_none()
                        || challenge_expiry.is_some_and(|expiry| expiry <= trusted_now_ms))
                {
                    bail!("dangerous confirmation attention lacks its pending challenge")
                }
                if value != "dangerous_action_confirmation"
                    && (challenge_expiry.is_some()
                        || challenge_status.is_some()
                        || challenge_count != 0)
                {
                    bail!("non-danger decision attention has a confirmation challenge")
                }
                (derived, Some(parse_decision_kind(value)?))
            }
        };
        let deep_link = match &decision_id {
            Some(id) => deep_link(ProjectionDeepLinkKind::Decision, id),
            None => deep_link(ProjectionDeepLinkKind::Delegation, &delegation_id),
        };
        output.push(NeedsYouProjectionItem {
            attention_id,
            subject: AttentionProjectionSubject::Delegation {
                delegation_id,
                delegation_revision,
            },
            kind,
            decision_id,
            decision_kind: decision_kind_value,
            decision_revision,
            reason: redactor.summary(&reason)?.as_str().to_owned(),
            recommendation: redactor.summary(&recommendation)?.as_str().to_owned(),
            alternative_count,
            alternatives: alternative_values,
            required_assurance: redactor.summary(&required_assurance)?.as_str().to_owned(),
            deadline_ms: challenge_expiry,
            created_at_ms,
            deep_link,
            runtime_recovery: None,
        });
    }
    read_runtime_needs_you(tx, redactor, &mut output)?;
    Ok(output)
}

fn read_runtime_needs_you(
    tx: &Transaction<'_>,
    redactor: &ReceiptRedactor,
    output: &mut Vec<NeedsYouProjectionItem>,
) -> Result<()> {
    let mut statement = tx.prepare(
        r#"SELECT attention.attention_id,attention.reason,attention.recommendation,
                  attention.alternatives_json,attention.required_assurance,
                  attention.runtime_host_generation,attention.runtime_host_instance_id,
                  attention.runtime_fencing_token,attention.runtime_actual_state,
                  attention.runtime_end_reason,attention.active_work_binding_digest,
                  attention.outbox_event_id,attention.receipt_id,attention.created_at,
                  generation.ended_at,generation.end_reason,receipt.subject_kind,
                  receipt.subject_revision,receipt.causation_event_id,
                  receipt.receipt_kind,receipt.subject_id,
                  receipt.scope_kind,receipt.scope_id,
                  event.event_name,event.aggregate_id,event.aggregate_revision
           FROM execass_attention_items attention
           JOIN execass_runtime_host_generations generation
             ON generation.generation=attention.runtime_host_generation
            AND generation.host_instance_id=attention.runtime_host_instance_id
           JOIN execass_outbox_events event ON event.event_id=attention.outbox_event_id
           JOIN execass_receipts receipt ON receipt.receipt_id=attention.receipt_id
           WHERE attention.scope_kind='runtime_host' AND attention.status='actionable'
           ORDER BY attention.created_at,attention.attention_id"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, RuntimeActualState>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, i64>(13)?,
            row.get::<_, Option<i64>>(14)?,
            row.get::<_, Option<String>>(15)?,
            row.get::<_, Option<String>>(16)?,
            row.get::<_, Option<i64>>(17)?,
            row.get::<_, String>(18)?,
            row.get::<_, String>(19)?,
            row.get::<_, Option<String>>(20)?,
            row.get::<_, Option<String>>(21)?,
            row.get::<_, Option<String>>(22)?,
            row.get::<_, String>(23)?,
            row.get::<_, String>(24)?,
            row.get::<_, i64>(25)?,
        ))
    })?;
    for row in rows {
        let (
            attention_id,
            reason,
            recommendation,
            alternatives_json,
            required_assurance,
            generation,
            host_instance_id,
            fencing_token,
            actual_state,
            end_reason,
            active_work_binding_digest,
            outbox_event_id,
            receipt_id,
            created_at_ms,
            ended_at,
            stored_end_reason,
            receipt_subject_kind,
            receipt_subject_revision,
            receipt_event_id,
            receipt_kind,
            receipt_subject_id,
            receipt_scope_kind,
            receipt_scope_id,
            event_name,
            event_aggregate_id,
            event_aggregate_revision,
        ) = row?;
        if ended_at.is_none()
            || stored_end_reason.as_deref() != Some(end_reason.as_str())
            || receipt_subject_kind.as_deref() != Some("runtime_host_generation")
            || receipt_subject_revision != Some(generation)
            || receipt_event_id != outbox_event_id
            || receipt_kind != "runtime_recovery"
            || receipt_subject_id.as_deref() != Some("execass-runtime-host")
            || receipt_scope_kind.as_deref() != Some("runtime_host")
            || receipt_scope_id.as_deref() != Some("execass-runtime-host")
            || event_name != "execass.v1.runtime_host.changed"
            || event_aggregate_id != "execass-runtime-host"
            || event_aggregate_revision != generation
        {
            bail!(
                "runtime-paused attention lost its exact generation, outbox, or receipt evidence"
            );
        }
        let alternatives = serde_json::from_str::<Vec<String>>(&alternatives_json)
            .context("runtime attention alternatives are not a string array")?;
        output.push(NeedsYouProjectionItem {
            attention_id,
            subject: AttentionProjectionSubject::RuntimeHost {
                generation,
                host_instance_id: host_instance_id.clone(),
                fencing_token,
            },
            kind: NeedsYouKind::RuntimePaused,
            decision_id: None,
            decision_kind: None,
            decision_revision: None,
            reason: redactor.summary(&reason)?.as_str().to_owned(),
            recommendation: redactor.summary(&recommendation)?.as_str().to_owned(),
            alternative_count: u32::try_from(alternatives.len())?,
            alternatives: alternatives
                .iter()
                .map(|value| Ok(redactor.summary(value)?.as_str().to_owned()))
                .collect::<Result<Vec<_>>>()?,
            required_assurance: redactor.summary(&required_assurance)?.as_str().to_owned(),
            deadline_ms: None,
            created_at_ms,
            deep_link: deep_link(ProjectionDeepLinkKind::Receipt, &receipt_id),
            runtime_recovery: Some(RuntimeRecoveryProjectionEvidence {
                predecessor_generation: generation,
                predecessor_host_instance_id: host_instance_id,
                predecessor_fencing_token: fencing_token,
                predecessor_actual_state: actual_state,
                predecessor_end_reason: end_reason,
                active_work_binding_digest,
                outbox_event_id,
                receipt_id: receipt_id.clone(),
                receipt_deep_link: deep_link(ProjectionDeepLinkKind::Receipt, &receipt_id),
            }),
        });
    }
    Ok(())
}

fn attention_kind_for_decision(value: &str) -> Result<NeedsYouKind> {
    Ok(match value {
        "clarification" => NeedsYouKind::Clarification,
        "recovery_choice" => NeedsYouKind::RecoveryChoice,
        "dangerous_action_confirmation"
        | "owner_configured_checkpoint"
        | "duplicate_risk_retry"
        | "stop"
        | "policy_change" => NeedsYouKind::Confirmation,
        _ => bail!("stored decision kind has no executive attention mapping"),
    })
}

fn parse_decision_kind(value: &str) -> Result<ProjectionDecisionKind> {
    Ok(match value {
        "clarification" => ProjectionDecisionKind::Clarification,
        "dangerous_action_confirmation" => ProjectionDecisionKind::DangerousActionConfirmation,
        "owner_configured_checkpoint" => ProjectionDecisionKind::OwnerConfiguredCheckpoint,
        "recovery_choice" => ProjectionDecisionKind::RecoveryChoice,
        "duplicate_risk_retry" => ProjectionDecisionKind::DuplicateRiskRetry,
        "stop" => ProjectionDecisionKind::Stop,
        "policy_change" => ProjectionDecisionKind::PolicyChange,
        _ => bail!("stored decision kind has no closed projection representation"),
    })
}

fn needs_you_kind_str(value: NeedsYouKind) -> &'static str {
    match value {
        NeedsYouKind::Confirmation => "confirmation",
        NeedsYouKind::Clarification => "clarification",
        NeedsYouKind::Reply => "reply",
        NeedsYouKind::RecoveryChoice => "recovery_choice",
        NeedsYouKind::RuntimePaused => "runtime_paused",
    }
}

fn read_in_motion(tx: &Transaction<'_>) -> Result<Vec<InMotionProjectionItem>> {
    let mut statement = tx.prepare(
        r#"SELECT d.delegation_id,d.state_revision,d.phase,d.run_control,d.updated_at,
                  d.policy_revision,d.external_wait_json,d.stop_epoch,
                  d.created_at,d.acknowledged_at,
                  (SELECT COUNT(*) FROM execass_action_branches b WHERE b.delegation_id=d.delegation_id AND b.status='runnable'),
                  (SELECT COUNT(*) FROM execass_action_branches b WHERE b.delegation_id=d.delegation_id AND b.status='executing'),
                  (SELECT COUNT(*) FROM execass_external_waits w WHERE w.delegation_id=d.delegation_id AND w.status='waiting')
           FROM execass_delegations d
           WHERE d.phase NOT IN ('completed','partially_completed','failed')
             AND (d.phase IN ('in_motion','recovering','waiting_external')
                  OR d.run_control IN ('stop_requested','stopped'))
           ORDER BY CASE d.run_control WHEN 'stopped' THEN 0 WHEN 'stop_requested' THEN 1 ELSE 2 END,
                    d.updated_at DESC,d.delegation_id"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, Option<i64>>(9)?,
            row.get::<_, i64>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, i64>(12)?,
        ))
    })?;
    rows.map(|row| {
        let (
            id,
            revision,
            phase,
            run_control,
            updated_at_ms,
            policy_revision,
            external_wait_json,
            stop_epoch,
            created_at_ms,
            acknowledged_at_ms,
            runnable,
            executing,
            waiting,
        ) = row?;
        let underlying_phase = parse_delegation_phase(&phase)?;
        let state = match run_control.as_str() {
            "stopped" => InMotionState::Stopped,
            "stop_requested" => InMotionState::Draining,
            "running" => match phase.as_str() {
                "in_motion" => InMotionState::Active,
                "recovering" => InMotionState::Recovering,
                "waiting_external" => InMotionState::WaitingExternal,
                _ => bail!("non-active delegation entered the In Motion projection"),
            },
            _ => bail!("stored run-control state is invalid"),
        };
        Ok(InMotionProjectionItem {
            delegation_id: id.clone(),
            delegation_revision: revision,
            underlying_phase,
            state,
            policy_revision,
            external_wait_json,
            stop_epoch,
            created_at_ms,
            acknowledged_at_ms,
            runnable_branch_count: u32::try_from(runnable)?,
            executing_branch_count: u32::try_from(executing)?,
            waiting_external_count: u32::try_from(waiting)?,
            updated_at_ms,
            deep_link: deep_link(ProjectionDeepLinkKind::Delegation, &id),
        })
    })
    .collect()
}

fn parse_delegation_phase(value: &str) -> Result<ProjectionDelegationPhase> {
    Ok(match value {
        "accepted" => ProjectionDelegationPhase::Accepted,
        "planning" => ProjectionDelegationPhase::Planning,
        "in_motion" => ProjectionDelegationPhase::InMotion,
        "waiting_for_user" => ProjectionDelegationPhase::WaitingForUser,
        "waiting_external" => ProjectionDelegationPhase::WaitingExternal,
        "recovering" => ProjectionDelegationPhase::Recovering,
        "completed" => ProjectionDelegationPhase::Completed,
        "partially_completed" => ProjectionDelegationPhase::PartiallyCompleted,
        "failed" => ProjectionDelegationPhase::Failed,
        _ => bail!("stored delegation phase has no closed projection representation"),
    })
}

fn read_done(
    tx: &Transaction<'_>,
    redactor: &ReceiptRedactor,
    trust: ProjectionTrust,
) -> Result<Vec<DoneProjectionItem>> {
    let mut statement = tx.prepare(
        r#"SELECT d.delegation_id,d.state_revision,d.phase,d.completion_assessment_json,d.terminal_at,
                  a.assessment_id,a.assessment_revision,a.terminal_phase,a.material_pass_count,
                  a.material_fail_count,a.material_unknown_count,a.useful_outcome,
                  a.exact_unmet_portion,a.assessment_json,a.no_remaining_path,
                  c.correction_id,c.correction_revision,c.warning,
                  (SELECT MIN(r.receipt_id) FROM execass_receipts r
                   WHERE r.subject_kind='completion_assessment' AND r.subject_id=a.assessment_id),
                  (SELECT COUNT(*) FROM execass_receipts r
                   WHERE r.subject_kind='completion_assessment' AND r.subject_id=a.assessment_id),
                  (SELECT MIN(r.receipt_id) FROM execass_receipts r
                   WHERE r.subject_kind='terminal_correction' AND r.subject_id=c.correction_id),
                  (SELECT COUNT(*) FROM execass_receipts r
                   WHERE r.subject_kind='terminal_correction' AND r.subject_id=c.correction_id),
                  d.policy_revision,d.run_control,d.stop_epoch,
                  d.created_at,d.acknowledged_at
           FROM execass_delegations d
           JOIN execass_completion_assessments a ON a.delegation_id=d.delegation_id
             AND NOT EXISTS(SELECT 1 FROM execass_completion_assessments newer
                            WHERE newer.delegation_id=a.delegation_id
                              AND newer.assessment_revision>a.assessment_revision)
           LEFT JOIN execass_terminal_corrections c ON c.delegation_id=d.delegation_id
             AND c.terminal_assessment_id=a.assessment_id
             AND NOT EXISTS(SELECT 1 FROM execass_terminal_corrections newer
                            WHERE newer.delegation_id=c.delegation_id
                              AND newer.correction_revision>c.correction_revision)
           WHERE d.phase IN ('completed','partially_completed','failed')
           ORDER BY d.terminal_at DESC,d.delegation_id"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, i64>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, i64>(14)?,
            row.get::<_, Option<String>>(15)?,
            row.get::<_, Option<i64>>(16)?,
            row.get::<_, Option<String>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, i64>(19)?,
            row.get::<_, Option<String>>(20)?,
            row.get::<_, i64>(21)?,
            row.get::<_, i64>(22)?,
            row.get::<_, String>(23)?,
            row.get::<_, i64>(24)?,
            row.get::<_, i64>(25)?,
            row.get::<_, Option<i64>>(26)?,
        ))
    })?;
    let mut output = Vec::new();
    for row in rows {
        let (
            id,
            revision,
            phase,
            delegation_assessment,
            terminal_at,
            assessment_id,
            assessment_revision,
            assessment_phase,
            pass,
            fail,
            unknown,
            useful,
            exact_unmet,
            assessment_json,
            no_remaining_path,
            correction_id,
            correction_revision,
            correction_warning,
            terminal_receipt_id,
            terminal_receipt_count,
            correction_receipt_id,
            correction_receipt_count,
            policy_revision,
            run_control,
            stop_epoch,
            created_at_ms,
            acknowledged_at_ms,
        ) = row?;
        if phase != assessment_phase
            || delegation_assessment.as_deref() != Some(assessment_json.as_str())
            || terminal_at.is_none()
            || no_remaining_path != 1
            || terminal_receipt_count != 1
            || terminal_receipt_id.is_none()
            || (correction_id.is_some()
                && (correction_receipt_count != 1 || correction_receipt_id.is_none()))
            || (correction_id.is_none()
                && (correction_receipt_count != 0 || correction_receipt_id.is_some()))
        {
            bail!("terminal delegation disagrees with its latest completion assessment")
        }
        let what_did_not_happen = parse_unmet_criteria(exact_unmet.as_deref())?;
        let (outcome, useful_outcome) = match phase.as_str() {
            "completed" if useful != 0 && fail == 0 && unknown == 0 => {
                (DoneOutcome::Completed, true)
            }
            "partially_completed" if useful != 0 && !what_did_not_happen.is_empty() => {
                (DoneOutcome::PartiallyCompleted, true)
            }
            "failed" if useful == 0 && !what_did_not_happen.is_empty() => {
                (DoneOutcome::Failed, false)
            }
            _ => bail!("terminal completion assessment violates its outcome semantics"),
        };
        let correction_deep_link = correction_id
            .as_ref()
            .map(|id| deep_link(ProjectionDeepLinkKind::AuthorityRecord, id));
        output.push(DoneProjectionItem {
            delegation_id: id.clone(),
            delegation_revision: revision,
            assessment_id,
            assessment_revision,
            outcome,
            policy_revision,
            run_control,
            stop_epoch,
            created_at_ms,
            acknowledged_at_ms,
            trust,
            useful_outcome,
            material_pass_count: pass,
            material_fail_count: fail,
            material_unknown_count: unknown,
            what_did_not_happen,
            correction_id,
            correction_revision,
            correction_warning: correction_warning
                .map(|value| {
                    redactor
                        .summary(&value)
                        .map(|safe| safe.as_str().to_owned())
                })
                .transpose()?,
            correction_deep_link,
            terminal_receipt_deep_link: deep_link(
                ProjectionDeepLinkKind::Receipt,
                terminal_receipt_id.as_deref().unwrap_or_default(),
            ),
            terminal_at_ms: terminal_at.unwrap_or_default(),
            deep_link: deep_link(ProjectionDeepLinkKind::Delegation, &id),
        });
    }
    Ok(output)
}

fn parse_unmet_criteria(value: Option<&str>) -> Result<Vec<UnmetCriterionProjection>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let stored: Vec<StoredUnmetCriterion> =
        serde_json::from_str(value).context("terminal unmet criteria are malformed")?;
    stored
        .into_iter()
        .map(|item| {
            let result = match item.result.as_str() {
                "fail" => UnmetCriterionResult::Fail,
                "unknown" => UnmetCriterionResult::Unknown,
                _ => bail!("terminal unmet criterion has an invalid result"),
            };
            Ok(UnmetCriterionProjection {
                criterion_id: item.criterion_id,
                criterion_key: item.criterion_key,
                result,
            })
        })
        .collect()
}

fn read_next(
    tx: &Transaction<'_>,
    query: &ExecAssProjectionQuery,
) -> Result<Vec<NextProjectionItem>> {
    let mut output = Vec::new();
    read_routine_next(tx, &mut output)?;
    read_recovery_next(tx, query.trusted_now_ms, &mut output)?;
    read_confirmation_next(tx, query.trusted_now_ms, &mut output)?;
    output.sort_by(|left, right| {
        left.due_at_ms
            .cmp(&right.due_at_ms)
            .then_with(|| next_kind_rank(left.kind).cmp(&next_kind_rank(right.kind)))
            .then_with(|| left.item_id.cmp(&right.item_id))
    });
    Ok(output)
}

fn read_routine_next(tx: &Transaction<'_>, output: &mut Vec<NextProjectionItem>) -> Result<()> {
    let mut occurrences = tx.prepare(
        r#"SELECT o.occurrence_id,o.routine_version,o.admitted_delegation_id,
                  o.scheduled_instant_ms,o.scheduled_local,r.timezone,o.created_at
           FROM execass_routine_occurrences o
           JOIN execass_routines r ON r.routine_id=o.routine_id
           WHERE r.enabled=1 AND o.status IN ('planned','admission_planned')
           ORDER BY o.scheduled_instant_ms,o.occurrence_id"#,
    )?;
    for row in occurrences.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })? {
        let (id, revision, delegation_id, due, local, timezone, created) = row?;
        output.push(NextProjectionItem {
            item_id: id.clone(),
            item_revision: revision,
            delegation_id,
            kind: NextKind::RoutineOccurrence,
            due_at_ms: due,
            details: NextDetails::RoutineOccurrence {
                scheduled_local: local,
                timezone,
            },
            created_at_ms: created,
            deep_link: deep_link(ProjectionDeepLinkKind::AuthorityRecord, &id),
        });
    }
    Ok(())
}

fn read_recovery_next(
    tx: &Transaction<'_>,
    now: i64,
    output: &mut Vec<NextProjectionItem>,
) -> Result<()> {
    let mut statement = tx.prepare(
        r#"SELECT e.recovery_evaluation_id,e.evaluation_revision,e.delegation_id,
                  e.not_before_ms,e.evaluated_at
           FROM execass_recovery_evaluations e
           JOIN execass_logical_effects effect ON effect.logical_effect_id=e.logical_effect_id
           WHERE e.directive IN ('wait_backoff','wait_circuit_breaker')
             AND e.not_before_ms>?1
             AND effect.state NOT IN ('succeeded','failed','reconciled_absent','reconciled_present')
             AND NOT EXISTS(SELECT 1 FROM execass_recovery_evaluations newer
                            WHERE newer.recovery_episode_id=e.recovery_episode_id
                              AND newer.evaluation_revision>e.evaluation_revision)
           ORDER BY e.not_before_ms,e.recovery_evaluation_id"#,
    )?;
    for row in statement.query_map([now], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })? {
        let (id, revision, delegation_id, due, created) = row?;
        output.push(NextProjectionItem {
            item_id: id,
            item_revision: revision,
            delegation_id: Some(delegation_id.clone()),
            kind: NextKind::RecoveryReevaluation,
            due_at_ms: due,
            details: NextDetails::RecoveryReevaluation,
            created_at_ms: created,
            deep_link: deep_link(ProjectionDeepLinkKind::Delegation, &delegation_id),
        });
    }
    Ok(())
}

fn read_confirmation_next(
    tx: &Transaction<'_>,
    now: i64,
    output: &mut Vec<NextProjectionItem>,
) -> Result<()> {
    let mut statement = tx.prepare(
        r#"SELECT challenge.challenge_id,decision.decision_revision,decision.delegation_id,
                  challenge.expires_at,challenge.created_at,decision.decision_id
           FROM execass_confirmation_challenges challenge
           JOIN execass_decisions decision ON decision.decision_id=challenge.decision_id
           JOIN execass_delegations delegation ON delegation.delegation_id=decision.delegation_id
           WHERE challenge.status='pending' AND decision.status='pending'
             AND decision.decision_kind='dangerous_action_confirmation'
             AND delegation.pending_decision_id=decision.decision_id
             AND challenge.expires_at>?1
           ORDER BY challenge.expires_at,challenge.challenge_id"#,
    )?;
    for row in statement.query_map([now], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
        ))
    })? {
        let (id, revision, delegation_id, due, created, decision_id) = row?;
        output.push(NextProjectionItem {
            item_id: id,
            item_revision: revision,
            delegation_id: Some(delegation_id),
            kind: NextKind::DangerousConfirmationExpiry,
            due_at_ms: due,
            details: NextDetails::DangerousConfirmationExpiry,
            created_at_ms: created,
            deep_link: deep_link(ProjectionDeepLinkKind::Decision, &decision_id),
        });
    }
    Ok(())
}

fn next_kind_rank(kind: NextKind) -> u8 {
    match kind {
        NextKind::DangerousConfirmationExpiry => 0,
        NextKind::RecoveryReevaluation => 1,
        NextKind::RoutineOccurrence => 2,
    }
}

fn read_receipts(
    tx: &Transaction<'_>,
    redactor: &ReceiptRedactor,
    limit: u16,
    trust: ProjectionTrust,
) -> Result<ReceiptProjectionWindow> {
    let (total, earliest, latest): (i64, Option<i64>, Option<i64>) = tx.query_row(
        "SELECT COUNT(*),MIN(global_sequence),MAX(global_sequence) FROM execass_receipts",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut statement = tx.prepare(
        r#"SELECT receipt_id,delegation_id,receipt_sequence,global_sequence,receipt_kind,
                  subject_kind,subject_id,subject_revision,receipt_digest,previous_receipt_digest,
                  global_previous_receipt_digest,key_id,key_generation,keyed_integrity_tag,
                  previous_key_integrity_tag,redacted_summary,occurred_at,committed_at
           FROM (SELECT receipt_id,delegation_id,receipt_sequence,global_sequence,receipt_kind,
                        subject_kind,subject_id,subject_revision,receipt_digest,previous_receipt_digest,
                        global_previous_receipt_digest,key_id,key_generation,keyed_integrity_tag,
                        previous_key_integrity_tag,redacted_summary,occurred_at,committed_at
                 FROM execass_receipts ORDER BY global_sequence DESC LIMIT ?1)
           ORDER BY global_sequence"#,
    )?;
    let rows = statement.query_map([i64::from(limit)], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, i64>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, String>(15)?,
            row.get::<_, i64>(16)?,
            row.get::<_, i64>(17)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (
            receipt_id,
            delegation_id,
            delegation_sequence,
            global_sequence,
            receipt_kind,
            subject_kind,
            subject_id,
            subject_revision,
            receipt_digest,
            delegation_previous_receipt_digest,
            global_previous_receipt_digest,
            key_id,
            key_generation,
            integrity_tag,
            previous_key_integrity_tag,
            summary,
            occurred,
            committed,
        ) = row?;
        let receipt_kind = parse_receipt_kind(
            &receipt_kind.context("receipt kind is missing from canonical receipt")?,
        )?;
        let subject_kind =
            parse_receipt_subject_kind(&subject_kind.context("receipt subject kind is missing")?)?;
        let subject_id = subject_id.context("receipt subject id is missing")?;
        let subject_revision = subject_revision.context("receipt subject revision is missing")?;
        let evidence = read_receipt_evidence(tx, redactor, &receipt_id)?;
        items.push(ReceiptProjectionItem {
            receipt_id: receipt_id.clone(),
            delegation_id,
            delegation_sequence,
            global_sequence,
            receipt_kind,
            subject_kind,
            subject_id,
            subject_revision,
            receipt_digest,
            delegation_previous_receipt_digest,
            global_previous_receipt_digest,
            key_id,
            key_generation,
            integrity_tag,
            previous_key_integrity_tag,
            trust,
            redacted_summary: redactor.summary(&summary)?.as_str().to_owned(),
            occurred_at_ms: occurred,
            committed_at_ms: committed,
            evidence,
            deep_link: deep_link(ProjectionDeepLinkKind::Receipt, &receipt_id),
        });
    }
    Ok(ReceiptProjectionWindow {
        limit,
        total,
        has_older: total > i64::from(limit),
        earliest_global_sequence: earliest,
        latest_global_sequence: latest,
        items,
    })
}

fn read_receipt_evidence(
    tx: &Transaction<'_>,
    redactor: &ReceiptRedactor,
    receipt_id: &str,
) -> Result<Vec<ReceiptEvidenceProjection>> {
    let mut statement = tx.prepare(
        "SELECT ordinal,authority_kind,authority_link_id,source_id,authoritative_revision,observation_digest,deep_link FROM execass_receipt_evidence_refs WHERE receipt_id=?1 ORDER BY ordinal",
    )?;
    let rows = statement.query_map([receipt_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut output = Vec::new();
    for (expected, row) in rows.enumerate() {
        let (
            ordinal,
            authority_kind,
            authority_link_id,
            source_id,
            authoritative_revision,
            observation_digest,
            stored_deep_link,
        ) = row?;
        if ordinal != i64::try_from(expected)? {
            bail!("receipt evidence ordinals are not contiguous")
        }
        output.push(ReceiptEvidenceProjection {
            ordinal,
            authority_kind: parse_authority_kind(&authority_kind)?,
            authority_link_id: authority_link_id.clone(),
            source_id: redactor.summary(&source_id)?.as_str().to_owned(),
            authoritative_revision,
            observation_digest,
            deep_link: ProjectionDeepLink {
                kind: ProjectionDeepLinkKind::AuthorityRecord,
                target_id: stored_deep_link,
            },
        });
    }
    Ok(output)
}

fn parse_receipt_kind(value: &str) -> Result<ProjectionReceiptKind> {
    Ok(match value {
        "intake" => ProjectionReceiptKind::Intake,
        "plan" => ProjectionReceiptKind::Plan,
        "amendment" => ProjectionReceiptKind::Amendment,
        "decision" => ProjectionReceiptKind::Decision,
        "continuation" => ProjectionReceiptKind::Continuation,
        "action" => ProjectionReceiptKind::Action,
        "effect" => ProjectionReceiptKind::Effect,
        "verifier" => ProjectionReceiptKind::Verifier,
        "recovery" => ProjectionReceiptKind::Recovery,
        "resume" => ProjectionReceiptKind::Resume,
        "budget" => ProjectionReceiptKind::Budget,
        "completion" => ProjectionReceiptKind::Completion,
        "terminal_correction" => ProjectionReceiptKind::TerminalCorrection,
        "authority_link" => ProjectionReceiptKind::AuthorityLink,
        "key_rotation" => ProjectionReceiptKind::KeyRotation,
        "global_stop" => ProjectionReceiptKind::GlobalStop,
        "run_control" => ProjectionReceiptKind::RunControl,
        "policy" => ProjectionReceiptKind::Policy,
        "runtime_settings" => ProjectionReceiptKind::RuntimeSettings,
        "runtime_recovery" => ProjectionReceiptKind::RuntimeRecovery,
        _ => bail!("stored receipt kind has no closed projection representation"),
    })
}

fn parse_receipt_subject_kind(value: &str) -> Result<ProjectionReceiptSubjectKind> {
    Ok(match value {
        "delegation" => ProjectionReceiptSubjectKind::Delegation,
        "plan" => ProjectionReceiptSubjectKind::Plan,
        "plan_amendment" => ProjectionReceiptSubjectKind::PlanAmendment,
        "decision" => ProjectionReceiptSubjectKind::Decision,
        "continuation" => ProjectionReceiptSubjectKind::Continuation,
        "action_branch" => ProjectionReceiptSubjectKind::ActionBranch,
        "verifier_result" => ProjectionReceiptSubjectKind::VerifierResult,
        "completion_assessment" => ProjectionReceiptSubjectKind::CompletionAssessment,
        "terminal_correction" => ProjectionReceiptSubjectKind::TerminalCorrection,
        "authority_link" => ProjectionReceiptSubjectKind::AuthorityLink,
        "recovery_evaluation" => ProjectionReceiptSubjectKind::RecoveryEvaluation,
        "outbox_event" => ProjectionReceiptSubjectKind::OutboxEvent,
        "global_runtime_control" => ProjectionReceiptSubjectKind::GlobalRuntimeControl,
        "policy_revision" => ProjectionReceiptSubjectKind::PolicyRevision,
        "runtime_settings_revision" => ProjectionReceiptSubjectKind::RuntimeSettingsRevision,
        "runtime_host_generation" => ProjectionReceiptSubjectKind::RuntimeHostGeneration,
        _ => bail!("stored receipt subject kind has no closed projection representation"),
    })
}

fn parse_authority_kind(value: &str) -> Result<ProjectionAuthorityKind> {
    Ok(match value {
        "session" => ProjectionAuthorityKind::Session,
        "run" => ProjectionAuthorityKind::Run,
        "job" => ProjectionAuthorityKind::Job,
        "job_run" => ProjectionAuthorityKind::JobRun,
        "task" => ProjectionAuthorityKind::Task,
        "board" => ProjectionAuthorityKind::Board,
        "board_card" => ProjectionAuthorityKind::BoardCard,
        "mail_thread" => ProjectionAuthorityKind::MailThread,
        "mail_message" => ProjectionAuthorityKind::MailMessage,
        "artifact_attachment" => ProjectionAuthorityKind::ArtifactAttachment,
        "artifact_board_card_asset" => ProjectionAuthorityKind::ArtifactBoardCardAsset,
        "artifact_mail_attachment" => ProjectionAuthorityKind::ArtifactMailAttachment,
        "security_audit_event" => ProjectionAuthorityKind::SecurityAuditEvent,
        "assistant_tool_call_audit" => ProjectionAuthorityKind::AssistantToolCallAudit,
        "tool_call" => ProjectionAuthorityKind::ToolCall,
        _ => bail!("stored authority kind has no closed projection representation"),
    })
}

fn read_reef(
    tx: &Transaction<'_>,
    observed_at_ms: i64,
    integrity: &ProjectionIntegrity,
) -> Result<Vec<ReefActivityItem>> {
    let mut statement = tx.prepare(
        "SELECT delegation_id,phase,run_control,updated_at FROM execass_delegations ORDER BY updated_at DESC,delegation_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    let mut output = Vec::new();
    for row in rows {
        let (id, phase, run_control, fresh_at_ms) = row?;
        let activity = reef_activity(&phase, &run_control)?;
        output.push(ReefActivityItem {
            subject: ReefSubject::Delegation {
                delegation_id: id.clone(),
            },
            activity,
            fresh_at_ms,
            deep_link: Some(deep_link(ProjectionDeepLinkKind::Delegation, &id)),
        });
    }
    if integrity.trust() == ProjectionTrust::Untrusted {
        output.push(ReefActivityItem {
            subject: ReefSubject::SystemIntegrity,
            activity: ReefActivity::IntegrityAttention,
            fresh_at_ms: observed_at_ms,
            deep_link: None,
        });
    }
    Ok(output)
}

fn apply_trust(projection: &mut ExecAssExecutiveProjection) {
    let trust = projection.integrity.trust();
    for item in &mut projection.done_since_you_checked {
        item.trust = trust;
    }
    for item in &mut projection.receipts.items {
        item.trust = trust;
    }
    projection
        .reef
        .retain(|item| !matches!(item.subject, ReefSubject::SystemIntegrity));
    if trust == ProjectionTrust::Untrusted {
        projection.reef.push(ReefActivityItem {
            subject: ReefSubject::SystemIntegrity,
            activity: ReefActivity::IntegrityAttention,
            fresh_at_ms: projection.observed_at_ms,
            deep_link: None,
        });
    }
}

pub(super) fn item_set_digest(projection: &ExecAssExecutiveProjection) -> Result<String> {
    let identities = canonical_delivered_items(projection)?;
    let mut digest = Sha256::new();
    digest.update(PROJECTION_VERSION.as_bytes());
    for item in identities {
        digest.update(item.projection_kind.as_str().as_bytes());
        digest.update(b"\0");
        digest.update(item.item_id.as_bytes());
        digest.update(b"\0");
        digest.update(item.revision.to_be_bytes());
        digest.update(b"\0");
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

/// Convert the five API-visible executive panes into acknowledgement identities.
/// This deliberately excludes Reef: it is coarse ambient activity, not a
/// delivered summary item.  Every identity is namespaced before persistence.
pub fn canonical_delivered_items(
    projection: &ExecAssExecutiveProjection,
) -> Result<Vec<SummaryDeliveredItem>> {
    let mut items = Vec::new();
    for item in &projection.needs_you {
        let subject_revision = match &item.subject {
            AttentionProjectionSubject::Delegation {
                delegation_revision,
                ..
            } => *delegation_revision,
            AttentionProjectionSubject::RuntimeHost { generation, .. } => *generation,
        };
        items.push(delivered_item(
            SummaryProjectionKind::NeedsYou,
            &item.attention_id,
            &[subject_revision, item.decision_revision.unwrap_or(0)],
        )?);
    }
    for item in &projection.in_motion {
        items.push(delivered_item(
            SummaryProjectionKind::InMotion,
            &item.delegation_id,
            &[item.delegation_revision],
        )?);
    }
    for item in &projection.done_since_you_checked {
        items.push(delivered_item(
            SummaryProjectionKind::Done,
            &item.assessment_id,
            &[
                item.delegation_revision,
                item.assessment_revision,
                item.correction_revision.unwrap_or(0),
            ],
        )?);
    }
    for item in &projection.next {
        items.push(delivered_item(
            SummaryProjectionKind::Next,
            &item.item_id,
            &[item.item_revision],
        )?);
    }
    for item in &projection.receipts.items {
        items.push(delivered_item(
            SummaryProjectionKind::Receipts,
            &item.receipt_id,
            &[item.global_sequence, item.subject_revision],
        )?);
    }
    items.sort();
    let unique = items
        .iter()
        .map(|item| (&item.item_id, item.revision))
        .collect::<BTreeSet<_>>();
    if unique.len() != items.len() {
        bail!("executive projection contains a duplicate delivered identity")
    }
    Ok(items)
}

pub(super) fn delivered_item(
    kind: SummaryProjectionKind,
    raw_id: &str,
    revisions: &[i64],
) -> Result<SummaryDeliveredItem> {
    let revision = revisions.iter().try_fold(0_i64, |sum, revision| {
        sum.checked_add(*revision)
            .context("executive summary item revision overflowed")
    })?;
    if revision <= 0 {
        bail!("executive summary item revision must be positive");
    }
    Ok(SummaryDeliveredItem {
        item_id: format!("{}:{raw_id}", kind.as_str()),
        revision,
        projection_kind: kind,
    })
}

pub(super) fn reef_activity(phase: &str, run_control: &str) -> Result<ReefActivity> {
    Ok(match phase {
        "completed" => ReefActivity::Completed,
        "partially_completed" => ReefActivity::PartiallyCompleted,
        "failed" => ReefActivity::Failed,
        _ => match run_control {
            "stopped" => ReefActivity::Stopped,
            "stop_requested" => ReefActivity::Draining,
            "running" => match phase {
                "accepted" | "planning" => ReefActivity::Planning,
                "in_motion" => ReefActivity::Working,
                "recovering" => ReefActivity::Recovering,
                "waiting_for_user" => ReefActivity::WaitingForYou,
                "waiting_external" => ReefActivity::WaitingExternal,
                _ => bail!("stored delegation phase has no Reef mapping"),
            },
            _ => bail!("stored run-control state has no Reef mapping"),
        },
    })
}

fn deep_link(kind: ProjectionDeepLinkKind, target_id: &str) -> ProjectionDeepLink {
    ProjectionDeepLink {
        kind,
        target_id: target_id.to_owned(),
    }
}
