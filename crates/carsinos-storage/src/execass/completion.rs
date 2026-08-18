//! Receipt-backed completion assessment and append-only late correction.
//!
//! This facade accepts concurrency and receipt bindings only. Terminal truth is
//! derived inside the writer transaction from current material criteria, their
//! latest independent verifier revisions, durable owner-amendment provenance,
//! and authoritative remaining-path state.

use super::receipt::{AtomicReceiptMutation, AtomicReceiptWriteOutcome};
use super::receipt_integrity::ReceiptIntegrityStore;
use super::redaction::ReceiptRedactor;
use super::rows::{get_delegation, get_outbox, insert_outbox};
use super::store::ExecAssStore;
use super::types::*;
use super::verifier::{CriterionPredicate, CRITERION_VERIFIER_IDENTITY};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const ASSESSMENT_SCHEMA: &str = "carsinos.execass.completion-assessment.v1";
const CORRECTION_SCHEMA: &str = "carsinos.execass.terminal-correction.v1";
const CORRECTION_WARNING: &str =
    "Late contrary evidence exists; the original terminal receipt remains immutable.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CriterionSnapshot {
    criterion_id: String,
    criterion_key: String,
    disposition: String,
    verifier_result_id: Option<String>,
    result_revision: Option<i64>,
    evidence_digest: Option<String>,
    supersession: Option<SupersessionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupersessionSnapshot {
    decision_id: String,
    decision_revision: i64,
    decision_authority_provenance_id: String,
    amendment_id: String,
    amendment_authority_provenance_id: String,
    superseded_criterion_id: String,
    superseded_criteria_revision: i64,
    resulting_criteria_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AssessmentSnapshot {
    schema: String,
    delegation_id: String,
    criteria_revision: i64,
    material_criterion_count: i64,
    material_pass_count: i64,
    material_fail_count: i64,
    material_unknown_count: i64,
    material_superseded_count: i64,
    useful_outcome: bool,
    exact_unmet_criteria: Vec<UnmetCriterion>,
    no_remaining_path: bool,
    criteria: Vec<CriterionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UnmetCriterion {
    criterion_id: String,
    criterion_key: String,
    result: String,
}

#[derive(Debug)]
enum AssessmentMutation {
    Applied(CompletionAssessmentRecord, OutboxEventRecord),
    Replayed(CompletionAssessmentRecord, OutboxEventRecord),
    NotTerminal(DelegationPhase, Vec<String>),
    StaleAssessmentRevision(i64),
    CriteriaRevisionMismatch(Option<i64>),
    AuthoritativeStateInvalid(&'static str),
    Conflict(String),
}

#[derive(Debug)]
enum CorrectionMutation {
    Applied(TerminalCorrectionRecord, OutboxEventRecord),
    Replayed(TerminalCorrectionRecord, OutboxEventRecord),
    NoContraryEvidence(String),
    StaleCorrectionRevision(i64),
    AuthoritativeStateInvalid(&'static str),
    NotTerminal,
    Conflict(String),
}

#[derive(Debug)]
struct MaterialCriterionRow {
    criterion_id: String,
    criterion_key: String,
    verifier_type: VerifierType,
    predicate_json: String,
    verifier_result_id: Option<String>,
    result_revision: Option<i64>,
    result: Option<String>,
    evidence_digest: Option<String>,
    receipt_count: i64,
}

impl ExecAssStore {
    pub fn assess_completion_atomically(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &AssessCompletionCommand,
    ) -> Result<CompletionAssessmentOutcome> {
        validate_assessment_command(command)?;
        let expected_post_revision = command
            .expected_state_revision
            .checked_add(1)
            .context("completion state revision is exhausted")?;
        let atomic = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            command.expected_state_revision,
            &command.receipt,
            |tx| assess_in_transaction(tx, command, expected_post_revision),
        )?;
        Ok(match atomic {
            AtomicReceiptWriteOutcome::Appended {
                value: AssessmentMutation::Applied(assessment, outbox_event),
                receipt,
            } => CompletionAssessmentOutcome::Terminalized {
                assessment,
                outbox_event,
                receipt,
            },
            AtomicReceiptWriteOutcome::Appended { .. } => {
                bail!("completion appended a receipt for a non-mutating outcome")
            }
            AtomicReceiptWriteOutcome::NoAppend(value) => match value {
                AssessmentMutation::Replayed(assessment, outbox_event) => {
                    CompletionAssessmentOutcome::Replayed {
                        assessment,
                        outbox_event,
                    }
                }
                AssessmentMutation::NotTerminal(current_phase, blockers) => {
                    CompletionAssessmentOutcome::NotTerminal {
                        current_phase,
                        blockers,
                    }
                }
                AssessmentMutation::StaleAssessmentRevision(current_assessment_revision) => {
                    CompletionAssessmentOutcome::StaleAssessmentRevision {
                        current_assessment_revision,
                    }
                }
                AssessmentMutation::CriteriaRevisionMismatch(current_criteria_revision) => {
                    CompletionAssessmentOutcome::CriteriaRevisionMismatch {
                        current_criteria_revision,
                    }
                }
                AssessmentMutation::AuthoritativeStateInvalid(reason) => {
                    CompletionAssessmentOutcome::AuthoritativeStateInvalid { reason }
                }
                AssessmentMutation::Conflict(duplicate_identity) => {
                    CompletionAssessmentOutcome::Conflict { duplicate_identity }
                }
                AssessmentMutation::Applied(_, _) => {
                    bail!("completion assessment was recorded without its receipt")
                }
            },
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                ..
            } => CompletionAssessmentOutcome::Stale {
                current_state_revision,
            },
        })
    }

    pub fn record_late_terminal_correction_atomically(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &RecordLateTerminalCorrectionCommand,
    ) -> Result<LateTerminalCorrectionOutcome> {
        validate_correction_command(command)?;
        let preflight = self.connection()?;
        let Some(delegation) = get_delegation(&preflight, &command.delegation_id)? else {
            return Ok(LateTerminalCorrectionOutcome::MissingDelegation);
        };
        if delegation.state_revision != command.expected_state_revision {
            return Ok(LateTerminalCorrectionOutcome::Stale {
                current_state_revision: delegation.state_revision,
            });
        }
        drop(preflight);

        let atomic =
            self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
                correct_in_transaction(tx, command)
            })?;
        Ok(match atomic {
            AtomicReceiptWriteOutcome::Appended {
                value: CorrectionMutation::Applied(correction, outbox_event),
                receipt,
            } => LateTerminalCorrectionOutcome::Recorded {
                correction,
                outbox_event,
                receipt,
            },
            AtomicReceiptWriteOutcome::Appended { .. } => {
                bail!("terminal correction appended a receipt for a non-mutating outcome")
            }
            AtomicReceiptWriteOutcome::NoAppend(value) => match value {
                CorrectionMutation::Replayed(correction, outbox_event) => {
                    LateTerminalCorrectionOutcome::Replayed {
                        correction,
                        outbox_event,
                    }
                }
                CorrectionMutation::NoContraryEvidence(terminal_assessment_id) => {
                    LateTerminalCorrectionOutcome::NoContraryEvidence {
                        terminal_assessment_id,
                    }
                }
                CorrectionMutation::StaleCorrectionRevision(current_correction_revision) => {
                    LateTerminalCorrectionOutcome::StaleCorrectionRevision {
                        current_correction_revision,
                    }
                }
                CorrectionMutation::AuthoritativeStateInvalid(reason) => {
                    LateTerminalCorrectionOutcome::AuthoritativeStateInvalid { reason }
                }
                CorrectionMutation::NotTerminal => LateTerminalCorrectionOutcome::NotTerminal,
                CorrectionMutation::Conflict(duplicate_identity) => {
                    LateTerminalCorrectionOutcome::Conflict { duplicate_identity }
                }
                CorrectionMutation::Applied(_, _) => {
                    bail!("terminal correction was recorded without its receipt")
                }
            },
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                ..
            } => LateTerminalCorrectionOutcome::Stale {
                current_state_revision,
            },
        })
    }
}

pub fn deterministic_completion_assessment_id(
    delegation_id: &str,
    assessment_revision: i64,
    idempotency_key: &str,
) -> String {
    deterministic_id(
        b"carsinos.execass.completion-assessment.v1\0",
        &[
            delegation_id,
            &assessment_revision.to_string(),
            idempotency_key,
        ],
        "completion-assessment",
    )
}

pub fn deterministic_completion_event_id(assessment_id: &str) -> String {
    deterministic_id(
        b"carsinos.execass.completion-event.v1\0",
        &[assessment_id],
        "completion-event",
    )
}

pub fn deterministic_terminal_correction_id(
    terminal_assessment_id: &str,
    correction_revision: i64,
    idempotency_key: &str,
) -> String {
    deterministic_id(
        b"carsinos.execass.terminal-correction.v1\0",
        &[
            terminal_assessment_id,
            &correction_revision.to_string(),
            idempotency_key,
        ],
        "terminal-correction",
    )
}

pub fn deterministic_terminal_correction_event_id(correction_id: &str) -> String {
    deterministic_id(
        b"carsinos.execass.terminal-correction-event.v1\0",
        &[correction_id],
        "terminal-correction-event",
    )
}

fn assess_in_transaction(
    tx: &Transaction<'_>,
    command: &AssessCompletionCommand,
    post_revision: i64,
) -> Result<AtomicReceiptMutation<AssessmentMutation>> {
    let assessment_id = deterministic_completion_assessment_id(
        &command.delegation_id,
        command.expected_assessment_revision,
        &command.write.idempotency_key,
    );
    let event_id = deterministic_completion_event_id(&assessment_id);
    if let Some(existing) = load_assessment(tx, &assessment_id)? {
        let transition_identity: Option<String> = tx
            .query_row(
                "SELECT command_identity FROM execass_lifecycle_transitions WHERE outbox_event_id=?1",
                [&event_id],
                |row| row.get(0),
            )
            .optional()?;
        let event = get_outbox(tx, &event_id)?;
        if transition_identity.as_deref() == Some(&format!("{command:#?}"))
            && event
                .as_ref()
                .is_some_and(|item| item.event.duplicate_identity == command.write.idempotency_key)
        {
            return Ok(AtomicReceiptMutation::NoAppend(
                AssessmentMutation::Replayed(
                    existing,
                    event.context("completion replay outbox is missing")?,
                ),
            ));
        }
        return Ok(AtomicReceiptMutation::NoAppend(
            AssessmentMutation::Conflict(command.write.idempotency_key.clone()),
        ));
    }

    let Some(current) = get_delegation(tx, &command.delegation_id)? else {
        bail!("completion delegation disappeared inside its atomic write")
    };
    if current.phase.is_terminal() {
        bail!("terminal delegation requires the late-correction facade")
    }
    if current.current_criteria_revision != Some(command.expected_criteria_revision) {
        return Ok(AtomicReceiptMutation::NoAppend(
            AssessmentMutation::CriteriaRevisionMismatch(current.current_criteria_revision),
        ));
    }
    let current_assessment_revision = max_revision(
        tx,
        "execass_completion_assessments",
        "assessment_revision",
        &command.delegation_id,
    )?;
    let next_assessment_revision = current_assessment_revision
        .checked_add(1)
        .context("completion assessment revision is exhausted")?;
    if command.expected_assessment_revision != next_assessment_revision {
        return Ok(AtomicReceiptMutation::NoAppend(
            AssessmentMutation::StaleAssessmentRevision(current_assessment_revision),
        ));
    }

    let criteria = load_material_criteria(
        tx,
        &command.delegation_id,
        command.expected_criteria_revision,
    )?;
    if criteria
        .iter()
        .any(|criterion| criterion.verifier_result_id.is_some() && criterion.receipt_count != 1)
    {
        return Ok(AtomicReceiptMutation::NoAppend(
            AssessmentMutation::AuthoritativeStateInvalid(
                "verifier result lacks one exact receipt-chain binding",
            ),
        ));
    }
    let mut blockers = remaining_path_blockers(tx, &current)?;
    if criteria.is_empty() {
        blockers.push("zero_material_criteria".into());
    }
    if !blockers.is_empty() {
        return Ok(AtomicReceiptMutation::NoAppend(
            AssessmentMutation::NotTerminal(current.phase, blockers),
        ));
    }

    let snapshot = build_snapshot(
        tx,
        &command.delegation_id,
        command.expected_criteria_revision,
        criteria,
    )?;
    let Some(satisfied) = snapshot
        .material_pass_count
        .checked_add(snapshot.material_superseded_count)
    else {
        return Ok(AtomicReceiptMutation::NoAppend(
            AssessmentMutation::AuthoritativeStateInvalid(
                "material criterion coverage count is exhausted",
            ),
        ));
    };
    let kind = if satisfied == snapshot.material_criterion_count
        && snapshot.material_fail_count == 0
        && snapshot.material_unknown_count == 0
    {
        CompletionAssessmentKind::Completed
    } else if snapshot.material_pass_count > 0 {
        CompletionAssessmentKind::PartiallyCompleted
    } else {
        CompletionAssessmentKind::Failed
    };
    let assessment_json = serde_json::to_string(&snapshot)?;
    let exact_unmet = (!snapshot.exact_unmet_criteria.is_empty())
        .then(|| serde_json::to_string(&snapshot.exact_unmet_criteria))
        .transpose()?;
    let assessment = CompletionAssessmentRecord {
        assessment_id: assessment_id.clone(),
        delegation_id: command.delegation_id.clone(),
        assessment_revision: command.expected_assessment_revision,
        criteria_revision: command.expected_criteria_revision,
        kind,
        material_pass_count: snapshot.material_pass_count,
        material_fail_count: snapshot.material_fail_count,
        material_unknown_count: snapshot.material_unknown_count,
        useful_outcome: kind == CompletionAssessmentKind::Completed
            || snapshot.material_pass_count > 0,
        exact_unmet_portion: exact_unmet,
        no_remaining_path: true,
        assessment_json: assessment_json.clone(),
        assessed_at: command.write.occurred_at,
    };
    let evidence = completion_receipt_evidence(tx, &snapshot.criteria)?;
    validate_completion_receipt(command, &assessment, &event_id, post_revision, &evidence)?;

    let event = NewOutboxEvent {
        event_id: event_id.clone(),
        event_name: OutboxEventName::CompletionAssessed,
        aggregate_id: command.delegation_id.clone(),
        aggregate_revision: post_revision,
        correlation_id: command.write.correlation_id.clone(),
        causation_id: command.write.causation_id.clone(),
        occurred_at: command.write.occurred_at,
        safe_payload_json: json!({
            "assessment_id": assessment_id,
            "terminal_phase": kind.phase().as_str(),
            "criteria_revision": command.expected_criteria_revision,
            "material_criterion_count": snapshot.material_criterion_count,
            "material_pass_count": snapshot.material_pass_count,
            "material_fail_count": snapshot.material_fail_count,
            "material_unknown_count": snapshot.material_unknown_count,
            "material_superseded_count": snapshot.material_superseded_count,
        })
        .to_string(),
        duplicate_identity: command.write.idempotency_key.clone(),
    };
    insert_outbox(tx, &event)?;
    let changed = tx.execute(
        r#"UPDATE execass_delegations
           SET phase=?1,state_revision=?2,pending_decision_id=NULL,external_wait_json=NULL,
               completion_assessment_json=?3,updated_at=?4,terminal_at=?4
           WHERE delegation_id=?5 AND state_revision=?6 AND current_criteria_revision=?7"#,
        params![
            kind.phase().as_str(),
            post_revision,
            assessment_json,
            command.write.occurred_at,
            command.delegation_id,
            command.expected_state_revision,
            command.expected_criteria_revision,
        ],
    )?;
    if changed != 1 {
        bail!("completion lifecycle CAS lost its canonical revision")
    }
    insert_assessment(tx, &assessment)?;
    let transition_id = deterministic_id(
        b"carsinos.execass.completion-transition.v1\0",
        &[&assessment.assessment_id],
        "completion-transition",
    );
    let projection_snapshot =
        super::lifecycle::projection_snapshot_json(tx, &command.delegation_id, None)?;
    tx.execute(
        r#"INSERT INTO execass_lifecycle_transitions(
             transition_id,delegation_id,state_revision,previous_phase,selected_phase,
             previous_run_control,selected_run_control,selector_input_json,command_identity,
             projection_snapshot_json,reason,outbox_event_id,occurred_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?6,?7,?8,?9,'completion_assessment',?10,?11)"#,
        params![
            transition_id,
            command.delegation_id,
            post_revision,
            current.phase.as_str(),
            kind.phase().as_str(),
            current.run_control.as_str(),
            json!({
                "completion_assessment": kind.phase().as_str(),
                "pre_actionable_phase": Value::Null,
                "ordinary_runnable_or_executing": false,
                "recovery_runnable_or_executing": false,
                "actionable_attention": false,
                "external_wait": false,
            })
            .to_string(),
            format!("{command:#?}"),
            projection_snapshot,
            event_id,
            command.write.occurred_at,
        ],
    )?;
    let outbox_event = get_outbox(tx, &event.event_id)?
        .context("completion outbox disappeared before receipt append")?;
    Ok(AtomicReceiptMutation::Append(AssessmentMutation::Applied(
        assessment,
        outbox_event,
    )))
}

fn correct_in_transaction(
    tx: &Transaction<'_>,
    command: &RecordLateTerminalCorrectionCommand,
) -> Result<AtomicReceiptMutation<CorrectionMutation>> {
    let Some(current) = get_delegation(tx, &command.delegation_id)? else {
        bail!("terminal correction delegation disappeared inside its atomic write")
    };
    if !current.phase.is_terminal() {
        return Ok(AtomicReceiptMutation::NoAppend(
            CorrectionMutation::NotTerminal,
        ));
    }
    let terminal = load_latest_assessment(tx, &command.delegation_id)?
        .context("terminal delegation has no immutable completion assessment")?;
    let correction_id = deterministic_terminal_correction_id(
        &terminal.assessment_id,
        command.expected_correction_revision,
        &command.write.idempotency_key,
    );
    let event_id = deterministic_terminal_correction_event_id(&correction_id);
    if let Some((existing, identity)) = load_correction(tx, &correction_id)? {
        let event = get_outbox(tx, &event_id)?;
        let expected_prefix = format!("{command:#?}|");
        if identity.starts_with(&expected_prefix)
            && event
                .as_ref()
                .is_some_and(|item| item.event.duplicate_identity == command.write.idempotency_key)
        {
            return Ok(AtomicReceiptMutation::NoAppend(
                CorrectionMutation::Replayed(
                    existing,
                    event.context("terminal correction replay outbox is missing")?,
                ),
            ));
        }
        return Ok(AtomicReceiptMutation::NoAppend(
            CorrectionMutation::Conflict(command.write.idempotency_key.clone()),
        ));
    }
    let current_correction_revision = max_revision(
        tx,
        "execass_terminal_corrections",
        "correction_revision",
        &command.delegation_id,
    )?;
    let next_correction_revision = current_correction_revision
        .checked_add(1)
        .context("terminal correction revision is exhausted")?;
    if command.expected_correction_revision != next_correction_revision {
        return Ok(AtomicReceiptMutation::NoAppend(
            CorrectionMutation::StaleCorrectionRevision(current_correction_revision),
        ));
    }
    let original: AssessmentSnapshot = serde_json::from_str(&terminal.assessment_json)
        .context("terminal assessment lacks the canonical EA-306 evidence snapshot")?;
    if original.schema != ASSESSMENT_SCHEMA {
        bail!("terminal assessment schema is not supported for late correction")
    }
    let latest_rows =
        load_material_criteria(tx, &command.delegation_id, terminal.criteria_revision)?;
    if latest_rows
        .iter()
        .any(|criterion| criterion.verifier_result_id.is_some() && criterion.receipt_count != 1)
    {
        return Ok(AtomicReceiptMutation::NoAppend(
            CorrectionMutation::AuthoritativeStateInvalid(
                "verifier result lacks one exact receipt-chain binding",
            ),
        ));
    }
    let latest = build_snapshot(
        tx,
        &command.delegation_id,
        terminal.criteria_revision,
        latest_rows,
    )?;
    let contrary = contrary_evidence(&original.criteria, &latest.criteria);
    if contrary.is_empty() {
        return Ok(AtomicReceiptMutation::NoAppend(
            CorrectionMutation::NoContraryEvidence(terminal.assessment_id),
        ));
    }
    let contrary_json = json!({
        "schema": CORRECTION_SCHEMA,
        "terminal_assessment_id": terminal.assessment_id,
        "original_terminal_phase": terminal.kind.phase().as_str(),
        "changes": contrary,
        "evaluated_at": command.write.occurred_at,
    })
    .to_string();
    let correction = TerminalCorrectionRecord {
        correction_id: correction_id.clone(),
        delegation_id: command.delegation_id.clone(),
        terminal_assessment_id: terminal.assessment_id.clone(),
        correction_revision: command.expected_correction_revision,
        contrary_evidence_json: contrary_json.clone(),
        warning: CORRECTION_WARNING.into(),
        recorded_at: command.write.occurred_at,
    };
    let evidence = completion_receipt_evidence(tx, &latest.criteria)?;
    validate_correction_receipt(command, &correction, &event_id, &evidence)?;
    let event = NewOutboxEvent {
        event_id: event_id.clone(),
        event_name: OutboxEventName::CompletionAssessed,
        aggregate_id: command.delegation_id.clone(),
        aggregate_revision: command.expected_state_revision,
        correlation_id: command.write.correlation_id.clone(),
        causation_id: command.write.causation_id.clone(),
        occurred_at: command.write.occurred_at,
        safe_payload_json: json!({
            "kind": "late_terminal_correction",
            "correction_id": correction_id,
            "terminal_assessment_id": terminal.assessment_id,
            "warning": CORRECTION_WARNING,
            "contrary_evidence_digest": sha256_text(&contrary_json),
        })
        .to_string(),
        duplicate_identity: command.write.idempotency_key.clone(),
    };
    insert_outbox(tx, &event)?;
    tx.execute(
        r#"INSERT INTO execass_terminal_corrections(
             correction_id,delegation_id,terminal_assessment_id,correction_revision,
             contrary_evidence_json,warning,recorded_at,command_identity,outbox_event_id
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
        params![
            correction.correction_id,
            correction.delegation_id,
            correction.terminal_assessment_id,
            correction.correction_revision,
            correction.contrary_evidence_json,
            correction.warning,
            correction.recorded_at,
            format!("{command:#?}|{contrary_json}"),
            event.event_id,
        ],
    )?;
    let outbox_event = get_outbox(tx, &event.event_id)?
        .context("terminal correction outbox disappeared before receipt append")?;
    Ok(AtomicReceiptMutation::Append(CorrectionMutation::Applied(
        correction,
        outbox_event,
    )))
}

fn load_material_criteria(
    tx: &Transaction<'_>,
    delegation_id: &str,
    criteria_revision: i64,
) -> Result<Vec<MaterialCriterionRow>> {
    let mut statement = tx.prepare(
        r#"SELECT c.criterion_id,c.criterion_key,c.verifier_type,c.expected_predicate_json,
                  v.verifier_result_id,v.result_revision,v.result,v.evidence_digest,
                  CASE WHEN v.verifier_result_id IS NULL THEN 0 ELSE (
                    SELECT COUNT(*) FROM execass_receipts receipt
                    JOIN execass_outbox_events event
                      ON event.event_id=receipt.causation_event_id
                    WHERE receipt.delegation_id=c.delegation_id
                      AND receipt.receipt_kind='verifier'
                      AND receipt.actor_type='runtime'
                      AND receipt.subject_kind='verifier_result'
                      AND receipt.subject_id=v.verifier_result_id
                      AND receipt.subject_revision=v.result_revision
                      AND event.event_name='execass.v1.delegation.transitioned'
                      AND event.aggregate_id=c.delegation_id
                      AND event.aggregate_revision=receipt.state_revision
                      AND event.causation_id=receipt.causation_id
                      AND json_extract(event.safe_payload_json,'$.criterion_id')=v.criterion_id
                      AND json_extract(event.safe_payload_json,'$.result')=v.result
                      AND json_extract(event.safe_payload_json,'$.result_revision')=v.result_revision
                      AND json_extract(event.safe_payload_json,'$.verifier_identity')=?3
                      AND v.verifier_identity=?3
                  ) END
           FROM execass_outcome_criteria c
           LEFT JOIN execass_verifier_results v ON v.verifier_result_id=(
             SELECT latest.verifier_result_id FROM execass_verifier_results latest
             WHERE latest.delegation_id=c.delegation_id AND latest.criterion_id=c.criterion_id
             ORDER BY latest.result_revision DESC LIMIT 1
           )
           WHERE c.delegation_id=?1 AND c.criteria_revision=?2 AND c.material=1
           ORDER BY c.criterion_key,c.criterion_id"#,
    )?;
    let rows = statement
        .query_map(
            params![
                delegation_id,
                criteria_revision,
                CRITERION_VERIFIER_IDENTITY
            ],
            |row| {
                Ok(MaterialCriterionRow {
                    criterion_id: row.get(0)?,
                    criterion_key: row.get(1)?,
                    verifier_type: row.get(2)?,
                    predicate_json: row.get(3)?,
                    verifier_result_id: row.get(4)?,
                    result_revision: row.get(5)?,
                    result: row.get(6)?,
                    evidence_digest: row.get(7)?,
                    receipt_count: row.get(8)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    Ok(rows)
}

fn build_snapshot(
    tx: &Transaction<'_>,
    delegation_id: &str,
    criteria_revision: i64,
    criteria: Vec<MaterialCriterionRow>,
) -> Result<AssessmentSnapshot> {
    let mut snapshots = Vec::with_capacity(criteria.len());
    let mut pass = 0_i64;
    let mut fail = 0_i64;
    let mut unknown = 0_i64;
    let mut superseded = 0_i64;
    let mut unmet = Vec::new();
    for row in criteria {
        let binding = if row.verifier_type == VerifierType::HumanBoundSupersession {
            resolve_human_supersession(
                tx,
                delegation_id,
                criteria_revision,
                &row.criterion_id,
                &row.predicate_json,
            )?
        } else {
            None
        };
        let disposition = if binding.is_some() {
            superseded = superseded
                .checked_add(1)
                .context("superseded criterion count is exhausted")?;
            "superseded".to_string()
        } else {
            let result = row.result.as_deref().unwrap_or("unknown");
            match result {
                "pass" => pass = pass.checked_add(1).context("pass count is exhausted")?,
                "fail" => fail = fail.checked_add(1).context("fail count is exhausted")?,
                "unknown" => {
                    unknown = unknown
                        .checked_add(1)
                        .context("unknown count is exhausted")?
                }
                _ => bail!("stored verifier result is invalid"),
            }
            if result != "pass" {
                unmet.push(UnmetCriterion {
                    criterion_id: row.criterion_id.clone(),
                    criterion_key: row.criterion_key.clone(),
                    result: result.into(),
                });
            }
            result.to_string()
        };
        snapshots.push(CriterionSnapshot {
            criterion_id: row.criterion_id,
            criterion_key: row.criterion_key,
            disposition,
            verifier_result_id: row.verifier_result_id,
            result_revision: row.result_revision,
            evidence_digest: row.evidence_digest,
            supersession: binding,
        });
    }
    let total = i64::try_from(snapshots.len()).context("material criterion count is exhausted")?;
    Ok(AssessmentSnapshot {
        schema: ASSESSMENT_SCHEMA.into(),
        delegation_id: delegation_id.into(),
        criteria_revision,
        material_criterion_count: total,
        material_pass_count: pass,
        material_fail_count: fail,
        material_unknown_count: unknown,
        material_superseded_count: superseded,
        useful_outcome: pass > 0,
        exact_unmet_criteria: unmet,
        no_remaining_path: true,
        criteria: snapshots,
    })
}

fn resolve_human_supersession(
    tx: &Transaction<'_>,
    delegation_id: &str,
    criteria_revision: i64,
    criterion_id: &str,
    predicate_json: &str,
) -> Result<Option<SupersessionSnapshot>> {
    let predicate: CriterionPredicate = match serde_json::from_str(predicate_json) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let CriterionPredicate::HumanBoundSupersession {
        decision_id,
        decision_revision,
        superseded_criterion_id,
        ..
    } = predicate
    else {
        return Ok(None);
    };
    let mut statement = tx.prepare(
        r#"SELECT decision.resolved_by_authority_provenance_id,
                  amendment.amendment_id,amendment.authority_provenance_id,
                  link.superseded_criteria_revision,link.resulting_criteria_revision
           FROM execass_decisions decision
           JOIN execass_authority_provenance decision_authority
             ON decision_authority.authority_provenance_id=decision.resolved_by_authority_provenance_id
           JOIN execass_plan_amendments amendment ON amendment.delegation_id=decision.delegation_id
           JOIN execass_authority_provenance amendment_authority
             ON amendment_authority.authority_provenance_id=amendment.authority_provenance_id
           JOIN execass_amendment_criteria_links link ON link.amendment_id=amendment.amendment_id
           JOIN execass_outcome_criteria old_criterion
             ON old_criterion.delegation_id=decision.delegation_id
            AND old_criterion.criterion_id=?4
            AND old_criterion.criteria_revision=link.superseded_criteria_revision
           WHERE decision.delegation_id=?1 AND decision.decision_id=?2
             AND decision.decision_revision=?3 AND decision.status='resolved'
             AND decision.result='revise'
             AND decision_authority.actor_type IN ('human_local','human_remote')
             AND decision_authority.authority_kind='decision_resolution'
             AND decision_authority.bound_decision_id=decision.decision_id
             AND decision_authority.bound_decision_revision=decision.decision_revision
             AND amendment_authority.actor_type IN ('human_local','human_remote')
             AND amendment_authority.authority_kind='action_specific_owner_amendment'
             AND amendment_authority.credential_identity=decision_authority.credential_identity
             AND amendment_authority.authenticated_ingress=decision_authority.authenticated_ingress
             AND amendment_authority.source_correlation_id=decision_authority.source_correlation_id
             AND link.resulting_criteria_revision=?5
             AND EXISTS(SELECT 1 FROM execass_outcome_criteria current_criterion
                        WHERE current_criterion.delegation_id=?1
                          AND current_criterion.criteria_revision=?5
                          AND current_criterion.criterion_id=?6
                          AND current_criterion.material=1
                          AND current_criterion.verifier_type='human_bound_supersession')"#,
    )?;
    let rows = statement
        .query_map(
            params![
                delegation_id,
                decision_id,
                decision_revision,
                superseded_criterion_id,
                criteria_revision,
                criterion_id,
            ],
            |row| {
                Ok(SupersessionSnapshot {
                    decision_id: decision_id.clone(),
                    decision_revision,
                    decision_authority_provenance_id: row.get(0)?,
                    amendment_id: row.get(1)?,
                    amendment_authority_provenance_id: row.get(2)?,
                    superseded_criterion_id: superseded_criterion_id.clone(),
                    superseded_criteria_revision: row.get(3)?,
                    resulting_criteria_revision: row.get(4)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((rows.len() == 1).then(|| rows[0].clone()))
}

fn remaining_path_blockers(
    tx: &Transaction<'_>,
    current: &DelegationRecord,
) -> Result<Vec<String>> {
    let mut blockers = Vec::new();
    let count = |sql: &str| -> Result<i64> {
        tx.query_row(sql, [&current.delegation_id], |row| row.get(0))
            .map_err(Into::into)
    };
    if count("SELECT COUNT(*) FROM execass_action_branches WHERE delegation_id=?1 AND status NOT IN ('terminal','superseded')")? > 0 {
        blockers.push("active_action_branch".into());
    }
    if count("SELECT COUNT(*) FROM execass_continuations WHERE delegation_id=?1 AND status NOT IN ('terminal','superseded')")? > 0 {
        blockers.push("active_continuation".into());
    }
    if count("SELECT COUNT(*) FROM execass_attention_items WHERE delegation_id=?1 AND status='actionable'")? > 0 {
        blockers.push("actionable_human_attention".into());
    }
    if count(
        "SELECT COUNT(*) FROM execass_external_waits WHERE delegation_id=?1 AND status='waiting'",
    )? > 0
    {
        blockers.push("external_wait".into());
    }
    if count("SELECT COUNT(*) FROM execass_logical_effects WHERE delegation_id=?1 AND state IN ('planned','claimed','invoking','outcome_unknown')")? > 0
        || count("SELECT COUNT(*) FROM execass_provider_attempts WHERE delegation_id=?1 AND status IN ('prepared','invoking','outcome_unknown')")? > 0
    {
        blockers.push("active_or_uncertain_effect".into());
    }
    if count(
        r#"SELECT COUNT(*) FROM execass_recovery_evaluations latest
           JOIN execass_recovery_episodes episode
             ON episode.recovery_episode_id=latest.recovery_episode_id
           JOIN execass_logical_effects effect
             ON effect.logical_effect_id=episode.logical_effect_id
           WHERE latest.delegation_id=?1
             AND latest.evaluation_revision=(SELECT MAX(candidate.evaluation_revision)
               FROM execass_recovery_evaluations candidate
               WHERE candidate.recovery_episode_id=latest.recovery_episode_id)
             AND effect.state NOT IN ('succeeded','reconciled_absent','reconciled_present')
             AND latest.directive NOT IN ('partially_completed','failed')"#,
    )? > 0
    {
        blockers.push("active_recovery_path".into());
    }
    Ok(blockers)
}

fn completion_receipt_evidence(
    tx: &Transaction<'_>,
    criteria: &[CriterionSnapshot],
) -> Result<Vec<ReceiptEvidenceInput>> {
    let mut evidence = Vec::new();
    let mut seen = BTreeSet::new();
    for criterion in criteria {
        let Some(result_id) = criterion.verifier_result_id.as_deref() else {
            continue;
        };
        let mut statement = tx.prepare(
            r#"SELECT ref.authority_link_id,ref.authority_kind,ref.source_id,ref.authoritative_revision
               FROM execass_receipts receipt
               JOIN execass_receipt_evidence_refs ref ON ref.receipt_id=receipt.receipt_id
               WHERE receipt.subject_kind='verifier_result' AND receipt.subject_id=?1
               ORDER BY ref.ordinal"#,
        )?;
        let rows = statement
            .query_map([result_id], |row| {
                Ok(ReceiptEvidenceInput {
                    authority_link_id: row.get(0)?,
                    kind: row.get(1)?,
                    source_id: row.get(2)?,
                    authoritative_revision: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for item in rows {
            let key = format!(
                "{}\0{}\0{}\0{}",
                item.authority_link_id,
                item.kind.as_str(),
                item.source_id,
                item.authoritative_revision
            );
            if seen.insert(key) {
                evidence.push(item);
            }
        }
    }
    if evidence.len() > 64 {
        bail!("completion evidence exceeds the canonical receipt limit")
    }
    Ok(evidence)
}

fn contrary_evidence(original: &[CriterionSnapshot], latest: &[CriterionSnapshot]) -> Vec<Value> {
    let mut changes = Vec::new();
    for old in original {
        match latest
            .iter()
            .find(|item| item.criterion_id == old.criterion_id)
        {
            Some(new) if new.disposition != old.disposition => changes.push(json!({
                "criterion_id": old.criterion_id,
                "criterion_key": old.criterion_key,
                "original_disposition": old.disposition,
                "latest_disposition": new.disposition,
                "latest_verifier_result_id": new.verifier_result_id,
                "latest_result_revision": new.result_revision,
                "latest_evidence_digest": new.evidence_digest,
            })),
            None => changes.push(json!({
                "criterion_id": old.criterion_id,
                "criterion_key": old.criterion_key,
                "original_disposition": old.disposition,
                "latest_disposition": "missing_current_criterion",
            })),
            _ => {}
        }
    }
    changes
}

fn validate_assessment_command(command: &AssessCompletionCommand) -> Result<()> {
    require_command_text(&command.delegation_id, "completion delegation")?;
    require_command_text(&command.write.idempotency_key, "completion idempotency key")?;
    if command.expected_state_revision <= 0
        || command.expected_criteria_revision <= 0
        || command.expected_assessment_revision <= 0
    {
        bail!("completion revisions must be positive")
    }
    let assessment_id = deterministic_completion_assessment_id(
        &command.delegation_id,
        command.expected_assessment_revision,
        &command.write.idempotency_key,
    );
    let event_id = deterministic_completion_event_id(&assessment_id);
    if command.receipt.delegation_id != command.delegation_id
        || command.receipt.receipt_kind != ReceiptKind::Completion
        || command.receipt.subject.kind != ReceiptSubjectKind::CompletionAssessment
        || command.receipt.subject.subject_id != assessment_id
        || command.receipt.subject.revision != command.expected_assessment_revision
        || command.receipt.causation_event_id != event_id
        || command.receipt.causation_id != command.write.causation_id
        || command.receipt.occurred_at != command.write.occurred_at
    {
        bail!("completion receipt does not bind the deterministic assessment command")
    }
    Ok(())
}

fn validate_completion_receipt(
    command: &AssessCompletionCommand,
    assessment: &CompletionAssessmentRecord,
    event_id: &str,
    post_revision: i64,
    evidence: &[ReceiptEvidenceInput],
) -> Result<()> {
    if command.receipt.expected_state_revision != post_revision
        || command.receipt.subject.subject_id != assessment.assessment_id
        || command.receipt.subject.revision != assessment.assessment_revision
        || command.receipt.causation_event_id != event_id
        || command.receipt.evidence != evidence
    {
        bail!("completion receipt does not match authoritative assessment evidence")
    }
    Ok(())
}

fn validate_correction_command(command: &RecordLateTerminalCorrectionCommand) -> Result<()> {
    require_command_text(&command.delegation_id, "terminal correction delegation")?;
    require_command_text(
        &command.write.idempotency_key,
        "terminal correction idempotency key",
    )?;
    if command.expected_state_revision <= 0 || command.expected_correction_revision <= 0 {
        bail!("terminal correction revisions must be positive")
    }
    if command.receipt.delegation_id != command.delegation_id
        || command.receipt.expected_state_revision != command.expected_state_revision
        || command.receipt.receipt_kind != ReceiptKind::TerminalCorrection
        || command.receipt.subject.kind != ReceiptSubjectKind::TerminalCorrection
        || command.receipt.subject.revision != command.expected_correction_revision
        || command.receipt.causation_id != command.write.causation_id
        || command.receipt.occurred_at != command.write.occurred_at
    {
        bail!("terminal correction receipt does not bind the correction command")
    }
    Ok(())
}

fn validate_correction_receipt(
    command: &RecordLateTerminalCorrectionCommand,
    correction: &TerminalCorrectionRecord,
    event_id: &str,
    evidence: &[ReceiptEvidenceInput],
) -> Result<()> {
    if command.receipt.subject.subject_id != correction.correction_id
        || command.receipt.causation_event_id != event_id
        || command.receipt.evidence != evidence
    {
        bail!("terminal correction receipt does not match the derived contrary evidence")
    }
    Ok(())
}

fn load_assessment(
    tx: &Transaction<'_>,
    assessment_id: &str,
) -> Result<Option<CompletionAssessmentRecord>> {
    tx.query_row(
        r#"SELECT assessment_id,delegation_id,assessment_revision,criteria_revision,
                  terminal_phase,material_pass_count,material_fail_count,material_unknown_count,
                  useful_outcome,exact_unmet_portion,no_remaining_path,assessment_json,assessed_at
           FROM execass_completion_assessments WHERE assessment_id=?1"#,
        [assessment_id],
        assessment_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn load_latest_assessment(
    tx: &Transaction<'_>,
    delegation_id: &str,
) -> Result<Option<CompletionAssessmentRecord>> {
    tx.query_row(
        r#"SELECT assessment_id,delegation_id,assessment_revision,criteria_revision,
                  terminal_phase,material_pass_count,material_fail_count,material_unknown_count,
                  useful_outcome,exact_unmet_portion,no_remaining_path,assessment_json,assessed_at
           FROM execass_completion_assessments WHERE delegation_id=?1
           ORDER BY assessment_revision DESC LIMIT 1"#,
        [delegation_id],
        assessment_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn assessment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CompletionAssessmentRecord> {
    let phase: String = row.get(4)?;
    let kind = match phase.as_str() {
        "completed" => CompletionAssessmentKind::Completed,
        "partially_completed" => CompletionAssessmentKind::PartiallyCompleted,
        "failed" => CompletionAssessmentKind::Failed,
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                "invalid completion assessment phase".into(),
            ))
        }
    };
    Ok(CompletionAssessmentRecord {
        assessment_id: row.get(0)?,
        delegation_id: row.get(1)?,
        assessment_revision: row.get(2)?,
        criteria_revision: row.get(3)?,
        kind,
        material_pass_count: row.get(5)?,
        material_fail_count: row.get(6)?,
        material_unknown_count: row.get(7)?,
        useful_outcome: row.get::<_, i64>(8)? != 0,
        exact_unmet_portion: row.get(9)?,
        no_remaining_path: row.get::<_, i64>(10)? != 0,
        assessment_json: row.get(11)?,
        assessed_at: row.get(12)?,
    })
}

fn insert_assessment(tx: &Transaction<'_>, item: &CompletionAssessmentRecord) -> Result<()> {
    tx.execute(
        r#"INSERT INTO execass_completion_assessments(
             assessment_id,delegation_id,assessment_revision,criteria_revision,terminal_phase,
             material_pass_count,material_fail_count,material_unknown_count,useful_outcome,
             exact_unmet_portion,no_remaining_path,assessment_json,assessed_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
        params![
            item.assessment_id,
            item.delegation_id,
            item.assessment_revision,
            item.criteria_revision,
            item.kind.phase().as_str(),
            item.material_pass_count,
            item.material_fail_count,
            item.material_unknown_count,
            item.useful_outcome as i64,
            item.exact_unmet_portion,
            item.no_remaining_path as i64,
            item.assessment_json,
            item.assessed_at,
        ],
    )?;
    Ok(())
}

fn load_correction(
    tx: &Transaction<'_>,
    correction_id: &str,
) -> Result<Option<(TerminalCorrectionRecord, String)>> {
    tx.query_row(
        r#"SELECT correction_id,delegation_id,terminal_assessment_id,correction_revision,
                  contrary_evidence_json,warning,recorded_at,command_identity
           FROM execass_terminal_corrections WHERE correction_id=?1"#,
        [correction_id],
        |row| {
            Ok((
                TerminalCorrectionRecord {
                    correction_id: row.get(0)?,
                    delegation_id: row.get(1)?,
                    terminal_assessment_id: row.get(2)?,
                    correction_revision: row.get(3)?,
                    contrary_evidence_json: row.get(4)?,
                    warning: row.get(5)?,
                    recorded_at: row.get(6)?,
                },
                row.get(7)?,
            ))
        },
    )
    .optional()
    .map_err(Into::into)
}

fn max_revision(
    tx: &Transaction<'_>,
    table: &str,
    column: &str,
    delegation_id: &str,
) -> Result<i64> {
    let sql = format!("SELECT COALESCE(MAX({column}),0) FROM {table} WHERE delegation_id=?1");
    tx.query_row(&sql, [delegation_id], |row| row.get(0))
        .map_err(Into::into)
}

fn deterministic_id(domain: &[u8], values: &[&str], prefix: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for value in values {
        digest.update(value.as_bytes());
        digest.update(b"\0");
    }
    format!("{prefix}-{:x}", digest.finalize())
}

fn sha256_text(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

fn require_command_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("{label} is invalid")
    }
    Ok(())
}
