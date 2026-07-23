//! Independent, typed outcome-criterion verification.
//!
//! The verifier never accepts a caller-, model-, worker-, or connector-supplied
//! result. Callers select one immutable criterion; this module parses its
//! closed predicate, re-reads the authoritative source, computes the result,
//! and commits the result, state revision, outbox event, and receipt together.

use super::receipt::{AtomicReceiptMutation, AtomicReceiptWriteOutcome};
use super::receipt_integrity::ReceiptIntegrityStore;
use super::redaction::ReceiptRedactor;
use super::rows::{get_delegation, get_outbox, insert_outbox};
use super::store::ExecAssStore;
use super::types::*;
use anyhow::{bail, Context, Result};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{ErrorKind, Read};

pub const CRITERION_VERIFIER_IDENTITY: &str = "carsinos.storage.criterion-verifier.v1";
pub const INVALID_CLOSED_PREDICATE_REASON: &str = "criterion_predicate_invalid_closed_v1";

const SOURCE_ARTIFACT: &str = "artifact_store";
const SOURCE_AUTHORITATIVE_STATE: &str = "authoritative_state_store";
const SOURCE_PROVIDER_STATE: &str = "provider_attempt_store";
const SOURCE_DELIVERY: &str = "delivery_store";
const SOURCE_PROCESS_EXIT: &str = "execution_store";
const SOURCE_DATABASE: &str = "execass_plan_store";
const SOURCE_HUMAN_SUPERSESSION: &str = "human_bound_supersession_store";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredicateVersion {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatusPredicate {
    Todo,
    InProgress,
    Blocked,
    Done,
    Archived,
}

impl TaskStatusPredicate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatusPredicate {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
    Cancelled,
}

impl RunStatusPredicate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobRunStatusPredicate {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl JobRunStatusPredicate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatusPredicate {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
    Cancelled,
}

impl ToolCallStatusPredicate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAttemptPredicate {
    Succeeded,
    Failed,
    ReconciledAbsent,
    ReconciledPresent,
}

impl ProviderAttemptPredicate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::ReconciledAbsent => "reconciled_absent",
            Self::ReconciledPresent => "reconciled_present",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteDeliveryProvider {
    Telegram,
    Discord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthoritativeStatePredicate {
    Task {
        authority_link_id: String,
        expected_status: TaskStatusPredicate,
    },
    Board {
        authority_link_id: String,
        expected_archived: bool,
    },
    BoardCard {
        authority_link_id: String,
        expected_column_id: String,
        expected_card_archived: bool,
        expected_board_archived: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProcessExitPredicate {
    Run {
        authority_link_id: String,
        expected_status: RunStatusPredicate,
    },
    JobRun {
        authority_link_id: String,
        expected_status: JobRunStatusPredicate,
    },
    ToolCall {
        authority_link_id: String,
        expected_status: ToolCallStatusPredicate,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeliveryPredicate {
    AgentMailLocal {
        authority_link_id: String,
        recipient_principal: String,
        require_ack: bool,
    },
    RemoteProvider {
        provider: RemoteDeliveryProvider,
        provider_message_id_digest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CriterionPredicate {
    Artifact {
        version: PredicateVersion,
        authority_link_id: String,
        expected_sha256: String,
        expected_bytes: i64,
    },
    AuthoritativeState {
        version: PredicateVersion,
        predicate: AuthoritativeStatePredicate,
    },
    ProviderState {
        version: PredicateVersion,
        attempt_id: String,
        expected_status: ProviderAttemptPredicate,
    },
    Delivery {
        version: PredicateVersion,
        predicate: DeliveryPredicate,
    },
    ProcessExit {
        version: PredicateVersion,
        predicate: ProcessExitPredicate,
    },
    DatabasePredicate {
        version: PredicateVersion,
        delegation_id: String,
        canonical_plan_revision_greater_than: i64,
    },
    HumanBoundSupersession {
        version: PredicateVersion,
        decision_id: String,
        decision_revision: i64,
        superseded_criterion_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionVerificationResult {
    Pass,
    Fail,
    Unknown,
}

impl CriterionVerificationResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unknown => "unknown",
        }
    }
}

impl FromSql for CriterionVerificationResult {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "unknown" => Ok(Self::Unknown),
            value => Err(FromSqlError::Other(Box::new(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("invalid criterion verification result: {value}"),
            )))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierResultRecord {
    pub verifier_result_id: String,
    pub delegation_id: String,
    pub criterion_id: String,
    pub result_revision: i64,
    pub result: CriterionVerificationResult,
    pub evidence_refs_json: String,
    pub evidence_digest: String,
    pub verifier_identity: String,
    pub verified_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyCriterionCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub criterion_id: String,
    pub expected_criteria_revision: i64,
    pub expected_state_revision: i64,
    pub expected_result_revision: i64,
    pub verifier_result_id: String,
    pub outbox_event_id: String,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionVerificationOutcome {
    Recorded {
        result: VerifierResultRecord,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Replayed {
        result: VerifierResultRecord,
        outbox_event: OutboxEventRecord,
    },
    Stale {
        current_state_revision: i64,
    },
    StaleResultRevision {
        current_result_revision: i64,
    },
    RevisionExhausted {
        revision_kind: &'static str,
        current_revision: i64,
    },
    CriteriaRevisionMismatch {
        current_criteria_revision: Option<i64>,
    },
    MissingDelegation,
    MissingCriterion,
    RejectedPredicate {
        reason: String,
    },
    Conflict {
        duplicate_identity: String,
    },
}

#[derive(Debug)]
enum MutationValue {
    Recorded(VerifierResultRecord, OutboxEventRecord),
    Replayed(VerifierResultRecord, OutboxEventRecord),
    StaleResultRevision(i64),
    RevisionExhausted(&'static str, i64),
    CriteriaRevisionMismatch(Option<i64>),
    MissingCriterion,
    Conflict(String),
}

#[derive(Debug)]
struct CriterionRow {
    verifier_type: VerifierType,
    predicate_json: String,
    authoritative_source_kind: String,
    criteria_revision: i64,
}

#[derive(Debug)]
struct Evaluation {
    result: CriterionVerificationResult,
    evidence: Value,
    receipt_evidence: Vec<ReceiptEvidenceInput>,
}

#[derive(Debug)]
struct LinkSource {
    link_id: String,
    kind: AuthorityLinkKind,
    source_id: String,
    authoritative_revision: i64,
}

impl ExecAssStore {
    pub fn verify_criterion(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &VerifyCriterionCommand,
    ) -> Result<CriterionVerificationOutcome> {
        if command.expected_state_revision.checked_add(1).is_none() {
            return Ok(CriterionVerificationOutcome::RevisionExhausted {
                revision_kind: "delegation_state",
                current_revision: command.expected_state_revision,
            });
        }
        validate_command_shape(command)?;
        let expected_result_id = deterministic_verifier_result_id(
            &command.criterion_id,
            command.expected_result_revision,
            &command.write.idempotency_key,
        );
        if command.verifier_result_id != expected_result_id {
            return Ok(CriterionVerificationOutcome::RejectedPredicate {
                reason: "verifier result identity is not deterministic for the command".into(),
            });
        }

        let preflight = self.connection()?;
        if load_result(&preflight, &command.verifier_result_id)?.is_some() {
            drop(preflight);
            let atomic =
                self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
                    let value = existing_result_mutation(tx, command)?
                        .context("immutable verifier result disappeared during replay")?;
                    Ok(AtomicReceiptMutation::NoAppend(value))
                })?;
            return match atomic {
                AtomicReceiptWriteOutcome::NoAppend(MutationValue::Replayed(
                    result,
                    outbox_event,
                )) => Ok(CriterionVerificationOutcome::Replayed {
                    result,
                    outbox_event,
                }),
                AtomicReceiptWriteOutcome::NoAppend(MutationValue::Conflict(
                    duplicate_identity,
                )) => Ok(CriterionVerificationOutcome::Conflict { duplicate_identity }),
                _ => bail!("immutable verifier replay produced an invalid atomic outcome"),
            };
        }
        let Some(delegation) = get_delegation(&preflight, &command.delegation_id)? else {
            return Ok(CriterionVerificationOutcome::MissingDelegation);
        };
        let Some(criterion) =
            load_criterion(&preflight, &command.delegation_id, &command.criterion_id)?
        else {
            return Ok(CriterionVerificationOutcome::MissingCriterion);
        };
        if delegation.current_criteria_revision != Some(command.expected_criteria_revision)
            || criterion.criteria_revision != command.expected_criteria_revision
        {
            return Ok(CriterionVerificationOutcome::CriteriaRevisionMismatch {
                current_criteria_revision: delegation.current_criteria_revision,
            });
        }
        let predicate = match parse_and_bind_predicate(&criterion, &command.delegation_id) {
            Ok(value) => value,
            Err(reason) => {
                return Ok(CriterionVerificationOutcome::RejectedPredicate { reason });
            }
        };
        drop(preflight);

        let atomic = self.mutate_with_advancing_atomic_receipt(
            integrity,
            redactor,
            command.expected_state_revision,
            &command.receipt,
            |tx| verify_in_transaction(tx, command, &criterion, &predicate),
        )?;
        Ok(match atomic {
            AtomicReceiptWriteOutcome::Appended { value, receipt } => match value {
                MutationValue::Recorded(result, outbox_event) => {
                    CriterionVerificationOutcome::Recorded {
                        result,
                        outbox_event,
                        receipt,
                    }
                }
                _ => bail!("verifier appended a receipt without recording a result"),
            },
            AtomicReceiptWriteOutcome::NoAppend(value) => match value {
                MutationValue::Replayed(result, outbox_event) => {
                    CriterionVerificationOutcome::Replayed {
                        result,
                        outbox_event,
                    }
                }
                MutationValue::StaleResultRevision(current_result_revision) => {
                    CriterionVerificationOutcome::StaleResultRevision {
                        current_result_revision,
                    }
                }
                MutationValue::RevisionExhausted(revision_kind, current_revision) => {
                    CriterionVerificationOutcome::RevisionExhausted {
                        revision_kind,
                        current_revision,
                    }
                }
                MutationValue::CriteriaRevisionMismatch(current_criteria_revision) => {
                    CriterionVerificationOutcome::CriteriaRevisionMismatch {
                        current_criteria_revision,
                    }
                }
                MutationValue::MissingCriterion => CriterionVerificationOutcome::MissingCriterion,
                MutationValue::Conflict(duplicate_identity) => {
                    CriterionVerificationOutcome::Conflict { duplicate_identity }
                }
                MutationValue::Recorded(_, _) => {
                    bail!("verifier recorded a result without appending its receipt")
                }
            },
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                ..
            } => CriterionVerificationOutcome::Stale {
                current_state_revision,
            },
        })
    }
}

pub fn deterministic_verifier_result_id(
    criterion_id: &str,
    result_revision: i64,
    idempotency_key: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.verifier-result.v1\0");
    digest.update(criterion_id.as_bytes());
    digest.update(b"\0");
    digest.update(result_revision.to_string().as_bytes());
    digest.update(b"\0");
    digest.update(idempotency_key.as_bytes());
    format!("verifier-result-{:x}", digest.finalize())
}

fn validate_command_shape(command: &VerifyCriterionCommand) -> Result<()> {
    if command.write.idempotency_key.trim().is_empty()
        || command.write.correlation_id.trim().is_empty()
        || command.write.causation_id.trim().is_empty()
        || command.delegation_id.trim().is_empty()
        || command.criterion_id.trim().is_empty()
        || command.verifier_result_id.trim().is_empty()
        || command.outbox_event_id.trim().is_empty()
        || command.expected_criteria_revision <= 0
        || command.expected_state_revision <= 0
        || command.expected_result_revision <= 0
        || command.write.occurred_at <= 0
    {
        bail!("criterion verification command contains an invalid identity, revision, or time");
    }
    let resulting_revision = command
        .expected_state_revision
        .checked_add(1)
        .context("criterion verification state revision is exhausted")?;
    let receipt = &command.receipt;
    if receipt.delegation_id != command.delegation_id
        || receipt.expected_state_revision != resulting_revision
        || receipt.receipt_kind != ReceiptKind::Verifier
        || receipt.subject.kind != ReceiptSubjectKind::VerifierResult
        || receipt.subject.subject_id != command.verifier_result_id
        || receipt.subject.revision != command.expected_result_revision
        || receipt.causation_id != command.write.causation_id
        || receipt.causation_event_id != command.outbox_event_id
        || receipt.actor.actor_type != ActorType::Runtime
        || receipt.rotation.is_some()
        || receipt.occurred_at != command.write.occurred_at
        || receipt.committed_at < receipt.occurred_at
    {
        bail!("criterion verification receipt is not bound to the exact runtime command");
    }
    Ok(())
}

fn parse_and_bind_predicate(
    criterion: &CriterionRow,
    delegation_id: &str,
) -> std::result::Result<CriterionPredicate, String> {
    let predicate: CriterionPredicate = serde_json::from_str(&criterion.predicate_json)
        .map_err(|_| INVALID_CLOSED_PREDICATE_REASON.to_string())?;
    let (verifier_type, source_kind) = match &predicate {
        CriterionPredicate::Artifact {
            expected_sha256,
            expected_bytes,
            ..
        } => {
            if !valid_sha256(expected_sha256) || *expected_bytes < 0 {
                return Err(
                    "artifact predicate requires a lowercase SHA-256 and nonnegative byte count"
                        .into(),
                );
            }
            (VerifierType::Artifact, SOURCE_ARTIFACT)
        }
        CriterionPredicate::AuthoritativeState { .. } => {
            (VerifierType::AuthoritativeState, SOURCE_AUTHORITATIVE_STATE)
        }
        CriterionPredicate::ProviderState { attempt_id, .. } => {
            if attempt_id.trim().is_empty() {
                return Err("provider predicate requires an exact attempt identity".into());
            }
            (VerifierType::ProviderState, SOURCE_PROVIDER_STATE)
        }
        CriterionPredicate::Delivery { predicate, .. } => {
            match predicate {
                DeliveryPredicate::AgentMailLocal {
                    recipient_principal,
                    ..
                } if recipient_principal.trim().is_empty() => {
                    return Err("local delivery predicate requires an exact recipient".into());
                }
                DeliveryPredicate::RemoteProvider {
                    provider_message_id_digest,
                    ..
                } if !valid_sha256(provider_message_id_digest) => {
                    return Err(
                        "remote delivery references must be represented by a SHA-256 digest".into(),
                    );
                }
                _ => {}
            }
            (VerifierType::Delivery, SOURCE_DELIVERY)
        }
        CriterionPredicate::ProcessExit { .. } => (VerifierType::ProcessExit, SOURCE_PROCESS_EXIT),
        CriterionPredicate::DatabasePredicate {
            delegation_id: predicate_delegation,
            canonical_plan_revision_greater_than,
            ..
        } => {
            if predicate_delegation != delegation_id || *canonical_plan_revision_greater_than < 0 {
                return Err("database predicate must bind this delegation and a nonnegative prior plan revision".into());
            }
            (VerifierType::DatabasePredicate, SOURCE_DATABASE)
        }
        CriterionPredicate::HumanBoundSupersession {
            decision_id,
            decision_revision,
            superseded_criterion_id,
            ..
        } => {
            if decision_id.trim().is_empty()
                || *decision_revision <= 0
                || superseded_criterion_id.trim().is_empty()
            {
                return Err(
                    "human supersession predicate requires exact decision and criterion identities"
                        .into(),
                );
            }
            (
                VerifierType::HumanBoundSupersession,
                SOURCE_HUMAN_SUPERSESSION,
            )
        }
    };
    if criterion.verifier_type != verifier_type {
        return Err("predicate kind does not match the criterion verifier type".into());
    }
    if criterion.authoritative_source_kind != source_kind {
        return Err("criterion authoritative source kind is not the closed verifier source".into());
    }
    Ok(predicate)
}

fn verify_in_transaction(
    tx: &Transaction<'_>,
    command: &VerifyCriterionCommand,
    preflight_criterion: &CriterionRow,
    predicate: &CriterionPredicate,
) -> Result<AtomicReceiptMutation<MutationValue>> {
    if let Some(value) = existing_result_mutation(tx, command)? {
        return Ok(AtomicReceiptMutation::NoAppend(value));
    }
    if duplicate_outbox_identity_exists(tx, &command.write.idempotency_key)? {
        return Ok(AtomicReceiptMutation::NoAppend(MutationValue::Conflict(
            command.write.idempotency_key.clone(),
        )));
    }

    let current_criteria_revision = tx.query_row(
        "SELECT current_criteria_revision FROM execass_delegations WHERE delegation_id=?1",
        [&command.delegation_id],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    if current_criteria_revision != Some(command.expected_criteria_revision) {
        return Ok(AtomicReceiptMutation::NoAppend(
            MutationValue::CriteriaRevisionMismatch(current_criteria_revision),
        ));
    }
    let Some(current_criterion) =
        load_criterion(tx, &command.delegation_id, &command.criterion_id)?
    else {
        return Ok(AtomicReceiptMutation::NoAppend(
            MutationValue::MissingCriterion,
        ));
    };
    if current_criterion.criteria_revision != command.expected_criteria_revision
        || current_criterion.verifier_type != preflight_criterion.verifier_type
        || current_criterion.predicate_json != preflight_criterion.predicate_json
        || current_criterion.authoritative_source_kind
            != preflight_criterion.authoritative_source_kind
    {
        return Ok(AtomicReceiptMutation::NoAppend(
            MutationValue::CriteriaRevisionMismatch(current_criteria_revision),
        ));
    }

    let current_result_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(result_revision),0) FROM execass_verifier_results WHERE criterion_id=?1",
        [&command.criterion_id],
        |row| row.get(0),
    )?;
    let Some(next_result_revision) = current_result_revision.checked_add(1) else {
        return Ok(AtomicReceiptMutation::NoAppend(
            MutationValue::RevisionExhausted("verifier_result", current_result_revision),
        ));
    };
    if command.expected_result_revision != next_result_revision {
        return Ok(AtomicReceiptMutation::NoAppend(
            MutationValue::StaleResultRevision(current_result_revision),
        ));
    }

    let evaluation = evaluate_predicate(tx, &command.delegation_id, predicate)?;
    if command.receipt.evidence != evaluation.receipt_evidence {
        return Ok(AtomicReceiptMutation::NoAppend(MutationValue::Conflict(
            command.write.idempotency_key.clone(),
        )));
    }
    let evidence_refs_json = serde_json::to_string(&evaluation.evidence)
        .context("failed serializing canonical verifier evidence")?;
    let evidence_digest = format!("sha256:{:x}", Sha256::digest(evidence_refs_json.as_bytes()));
    let result = VerifierResultRecord {
        verifier_result_id: command.verifier_result_id.clone(),
        delegation_id: command.delegation_id.clone(),
        criterion_id: command.criterion_id.clone(),
        result_revision: command.expected_result_revision,
        result: evaluation.result,
        evidence_refs_json,
        evidence_digest,
        verifier_identity: CRITERION_VERIFIER_IDENTITY.into(),
        verified_at: command.write.occurred_at,
    };
    let resulting_state_revision = command
        .expected_state_revision
        .checked_add(1)
        .context("criterion verification state revision is exhausted")?;
    let safe_payload_json = serde_json::to_string(&json!({
        "criterion_id": command.criterion_id,
        "result": result.result.as_str(),
        "result_revision": result.result_revision,
        "verifier_identity": CRITERION_VERIFIER_IDENTITY,
    }))?;
    let event = NewOutboxEvent {
        event_id: command.outbox_event_id.clone(),
        event_name: OutboxEventName::DelegationTransitioned,
        aggregate_id: command.delegation_id.clone(),
        aggregate_revision: resulting_state_revision,
        correlation_id: command.write.correlation_id.clone(),
        causation_id: command.write.causation_id.clone(),
        occurred_at: command.write.occurred_at,
        safe_payload_json,
        duplicate_identity: command.write.idempotency_key.clone(),
    };

    let changed = tx.execute(
        "UPDATE execass_delegations SET state_revision=?1,updated_at=?2 WHERE delegation_id=?3 AND state_revision=?4 AND current_criteria_revision=?5",
        params![resulting_state_revision,command.write.occurred_at,command.delegation_id,command.expected_state_revision,command.expected_criteria_revision],
    )?;
    if changed != 1 {
        bail!("delegation changed during criterion verification");
    }
    insert_outbox(tx, &event)?;
    tx.execute(
        r#"INSERT INTO execass_verifier_results(
             verifier_result_id,delegation_id,criterion_id,result_revision,result,
             evidence_refs_json,evidence_digest,verifier_identity,verified_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
        params![
            result.verifier_result_id,
            result.delegation_id,
            result.criterion_id,
            result.result_revision,
            result.result.as_str(),
            result.evidence_refs_json,
            result.evidence_digest,
            result.verifier_identity,
            result.verified_at,
        ],
    )?;
    let outbox_event = get_outbox(tx, &command.outbox_event_id)?
        .context("verifier outbox event disappeared before receipt append")?;
    Ok(AtomicReceiptMutation::Append(MutationValue::Recorded(
        result,
        outbox_event,
    )))
}

fn existing_result_mutation(
    conn: &Connection,
    command: &VerifyCriterionCommand,
) -> Result<Option<MutationValue>> {
    let Some(existing) = load_result(conn, &command.verifier_result_id)? else {
        return Ok(None);
    };
    let exact_receipt: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM execass_receipts WHERE delegation_id=?1 AND subject_kind='verifier_result' AND subject_id=?2 AND subject_revision=?3 AND causation_event_id=?4)",
        params![command.delegation_id,command.verifier_result_id,command.expected_result_revision,command.outbox_event_id],
        |row| row.get(0),
    )?;
    let event = get_outbox(conn, &command.outbox_event_id)?;
    if existing.delegation_id == command.delegation_id
        && existing.criterion_id == command.criterion_id
        && existing.result_revision == command.expected_result_revision
        && existing.verifier_identity == CRITERION_VERIFIER_IDENTITY
        && exact_receipt
        && event
            .as_ref()
            .is_some_and(|item| item.event.duplicate_identity == command.write.idempotency_key)
    {
        return Ok(Some(MutationValue::Replayed(
            existing,
            event.context("replayed verifier outbox event is missing")?,
        )));
    }
    Ok(Some(MutationValue::Conflict(
        command.write.idempotency_key.clone(),
    )))
}

fn evaluate_predicate(
    conn: &Connection,
    delegation_id: &str,
    predicate: &CriterionPredicate,
) -> Result<Evaluation> {
    match predicate {
        CriterionPredicate::Artifact {
            authority_link_id,
            expected_sha256,
            expected_bytes,
            ..
        } => evaluate_artifact(
            conn,
            delegation_id,
            authority_link_id,
            expected_sha256,
            *expected_bytes,
        ),
        CriterionPredicate::AuthoritativeState { predicate, .. } => {
            evaluate_authoritative_state(conn, delegation_id, predicate)
        }
        CriterionPredicate::ProviderState {
            attempt_id,
            expected_status,
            ..
        } => evaluate_provider_state(conn, delegation_id, attempt_id, *expected_status),
        CriterionPredicate::Delivery { predicate, .. } => {
            evaluate_delivery(conn, delegation_id, predicate)
        }
        CriterionPredicate::ProcessExit { predicate, .. } => {
            evaluate_process(conn, delegation_id, predicate)
        }
        CriterionPredicate::DatabasePredicate {
            canonical_plan_revision_greater_than,
            ..
        } => {
            evaluate_database_predicate(conn, delegation_id, *canonical_plan_revision_greater_than)
        }
        CriterionPredicate::HumanBoundSupersession {
            decision_id,
            decision_revision,
            superseded_criterion_id,
            ..
        } => Ok(Evaluation {
            result: CriterionVerificationResult::Unknown,
            evidence: json!({
                "authority_refs": [],
                "observation": {
                    "decision_id_digest": sha256_text(decision_id),
                    "decision_revision": decision_revision,
                    "reason": "no_authoritative_criterion_supersession_binding",
                    "superseded_criterion_id_digest": sha256_text(superseded_criterion_id),
                },
                "predicate_version": "v1",
            }),
            receipt_evidence: vec![],
        }),
    }
}

fn evaluate_artifact(
    conn: &Connection,
    delegation_id: &str,
    link_id: &str,
    expected_sha256: &str,
    expected_bytes: i64,
) -> Result<Evaluation> {
    let Some(link) = load_link(conn, delegation_id, link_id)? else {
        return Ok(unknown_missing_link(link_id));
    };
    let row = match link.kind {
        AuthorityLinkKind::ArtifactAttachment => conn
            .query_row(
                "SELECT sha256,bytes,local_path FROM attachments WHERE attachment_id=?1",
                [&link.source_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?,
        AuthorityLinkKind::ArtifactBoardCardAsset => conn
            .query_row(
                "SELECT sha256,bytes,local_path FROM board_card_assets WHERE card_asset_id=?1",
                [&link.source_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?,
        AuthorityLinkKind::ArtifactMailAttachment => conn
            .query_row(
                "SELECT sha256,bytes,local_path FROM agent_mail_attachments WHERE attachment_id=?1",
                [&link.source_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?,
        _ => {
            return Ok(unknown_wrong_link(
                &link,
                "artifact authority link kind mismatch",
            ))
        }
    };
    let refs = vec![receipt_evidence(&link)];
    let Some((registered_sha256, registered_bytes, local_path)) = row else {
        return Ok(evaluation_with_link(
            CriterionVerificationResult::Unknown,
            &link,
            json!({"reason":"artifact_registration_missing"}),
        ));
    };
    let mut file = match File::open(&local_path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(evaluation_with_refs(
                CriterionVerificationResult::Fail,
                refs,
                &link,
                json!({"reason":"artifact_file_missing"}),
            ));
        }
        Err(_) => {
            return Ok(evaluation_with_refs(
                CriterionVerificationResult::Unknown,
                refs,
                &link,
                json!({"reason":"artifact_file_unreadable"}),
            ));
        }
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(_) => {
            return Ok(evaluation_with_refs(
                CriterionVerificationResult::Unknown,
                refs,
                &link,
                json!({"reason":"artifact_metadata_unavailable"}),
            ));
        }
    };
    if !metadata.is_file() {
        return Ok(evaluation_with_refs(
            CriterionVerificationResult::Fail,
            refs,
            &link,
            json!({"reason":"artifact_is_not_a_regular_file"}),
        ));
    }
    let mut hasher = Sha256::new();
    let mut bytes_read = 0_i64;
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let count = match file.read(&mut buffer) {
            Ok(count) => count,
            Err(_) => {
                return Ok(evaluation_with_refs(
                    CriterionVerificationResult::Unknown,
                    refs,
                    &link,
                    json!({"reason":"artifact_read_interrupted"}),
                ));
            }
        };
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes_read = bytes_read.saturating_add(count as i64);
    }
    let actual_sha256 = format!("{:x}", hasher.finalize());
    let matches = registered_sha256 == expected_sha256
        && actual_sha256 == expected_sha256
        && registered_bytes == expected_bytes
        && bytes_read == expected_bytes
        && metadata.len() == expected_bytes as u64;
    Ok(evaluation_with_refs(
        if matches {
            CriterionVerificationResult::Pass
        } else {
            CriterionVerificationResult::Fail
        },
        refs,
        &link,
        json!({
            "actual_bytes": bytes_read,
            "actual_sha256": actual_sha256,
            "expected_bytes": expected_bytes,
            "expected_sha256": expected_sha256,
            "registered_bytes": registered_bytes,
            "registered_sha256": registered_sha256,
        }),
    ))
}

fn evaluate_authoritative_state(
    conn: &Connection,
    delegation_id: &str,
    predicate: &AuthoritativeStatePredicate,
) -> Result<Evaluation> {
    match predicate {
        AuthoritativeStatePredicate::Task {
            authority_link_id,
            expected_status,
        } => {
            let Some(link) = load_link(conn, delegation_id, authority_link_id)? else {
                return Ok(unknown_missing_link(authority_link_id));
            };
            if link.kind != AuthorityLinkKind::Task {
                return Ok(unknown_wrong_link(
                    &link,
                    "task authority link kind mismatch",
                ));
            }
            let actual = conn
                .query_row(
                    "SELECT status FROM tasks WHERE task_id=?1",
                    [&link.source_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            Ok(compare_optional_text(
                &link,
                actual,
                expected_status.as_str(),
                "task_status",
            ))
        }
        AuthoritativeStatePredicate::Board {
            authority_link_id,
            expected_archived,
        } => {
            let Some(link) = load_link(conn, delegation_id, authority_link_id)? else {
                return Ok(unknown_missing_link(authority_link_id));
            };
            if link.kind != AuthorityLinkKind::Board {
                return Ok(unknown_wrong_link(
                    &link,
                    "board authority link kind mismatch",
                ));
            }
            let actual = conn
                .query_row(
                    "SELECT archived_at IS NOT NULL FROM boards WHERE board_id=?1",
                    [&link.source_id],
                    |row| row.get::<_, bool>(0),
                )
                .optional()?;
            Ok(compare_optional_bool(
                &link,
                actual,
                *expected_archived,
                "board_archived",
            ))
        }
        AuthoritativeStatePredicate::BoardCard {
            authority_link_id,
            expected_column_id,
            expected_card_archived,
            expected_board_archived,
        } => {
            let Some(link) = load_link(conn, delegation_id, authority_link_id)? else {
                return Ok(unknown_missing_link(authority_link_id));
            };
            if link.kind != AuthorityLinkKind::BoardCard {
                return Ok(unknown_wrong_link(
                    &link,
                    "board-card authority link kind mismatch",
                ));
            }
            let actual = conn
                .query_row(
                    r#"SELECT card.column_id,card.archived_at IS NOT NULL,board.archived_at IS NOT NULL
                       FROM board_cards card JOIN boards board ON board.board_id=card.board_id
                       WHERE card.card_id=?1"#,
                    [&link.source_id],
                    |row| Ok((row.get::<_, String>(0)?,row.get::<_, bool>(1)?,row.get::<_, bool>(2)?)),
                )
                .optional()?;
            let Some((column_id, card_archived, board_archived)) = actual else {
                return Ok(evaluation_with_link(
                    CriterionVerificationResult::Unknown,
                    &link,
                    json!({"reason":"board_card_missing"}),
                ));
            };
            let matches = column_id == *expected_column_id
                && card_archived == *expected_card_archived
                && board_archived == *expected_board_archived;
            Ok(evaluation_with_link(
                if matches {
                    CriterionVerificationResult::Pass
                } else {
                    CriterionVerificationResult::Fail
                },
                &link,
                json!({
                    "actual_board_archived":board_archived,
                    "actual_card_archived":card_archived,
                    "actual_column_id":column_id,
                    "expected_board_archived":expected_board_archived,
                    "expected_card_archived":expected_card_archived,
                    "expected_column_id":expected_column_id,
                }),
            ))
        }
    }
}

fn evaluate_process(
    conn: &Connection,
    delegation_id: &str,
    predicate: &ProcessExitPredicate,
) -> Result<Evaluation> {
    let (link_id, expected, required_kind, table, id_column) = match predicate {
        ProcessExitPredicate::Run {
            authority_link_id,
            expected_status,
        } => (
            authority_link_id,
            expected_status.as_str(),
            AuthorityLinkKind::Run,
            "runs",
            "run_id",
        ),
        ProcessExitPredicate::JobRun {
            authority_link_id,
            expected_status,
        } => (
            authority_link_id,
            expected_status.as_str(),
            AuthorityLinkKind::JobRun,
            "job_runs",
            "job_run_id",
        ),
        ProcessExitPredicate::ToolCall {
            authority_link_id,
            expected_status,
        } => (
            authority_link_id,
            expected_status.as_str(),
            AuthorityLinkKind::ToolCall,
            "tool_calls",
            "tool_call_id",
        ),
    };
    let Some(link) = load_link(conn, delegation_id, link_id)? else {
        return Ok(unknown_missing_link(link_id));
    };
    if link.kind != required_kind {
        return Ok(unknown_wrong_link(
            &link,
            "execution authority link kind mismatch",
        ));
    }
    let sql = format!("SELECT status FROM {table} WHERE {id_column}=?1");
    let actual = conn
        .query_row(&sql, [&link.source_id], |row| row.get::<_, String>(0))
        .optional()?;
    Ok(compare_optional_text(
        &link,
        actual,
        expected,
        "execution_status_progress_only",
    ))
}

fn evaluate_delivery(
    conn: &Connection,
    delegation_id: &str,
    predicate: &DeliveryPredicate,
) -> Result<Evaluation> {
    match predicate {
        DeliveryPredicate::AgentMailLocal {
            authority_link_id,
            recipient_principal,
            require_ack,
        } => {
            let Some(link) = load_link(conn, delegation_id, authority_link_id)? else {
                return Ok(unknown_missing_link(authority_link_id));
            };
            if link.kind != AuthorityLinkKind::MailMessage {
                return Ok(unknown_wrong_link(
                    &link,
                    "mail-message authority link kind mismatch",
                ));
            }
            let row = conn
                .query_row(
                    "SELECT delivered_at,acked_at FROM agent_mail_message_recipients WHERE message_id=?1 AND recipient_principal=?2",
                    params![link.source_id,recipient_principal],
                    |row| Ok((row.get::<_, i64>(0)?,row.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            let Some((delivered_at, acked_at)) = row else {
                return Ok(evaluation_with_link(
                    CriterionVerificationResult::Fail,
                    &link,
                    json!({
                        "reason":"local_recipient_delivery_missing",
                        "recipient_digest":sha256_text(recipient_principal),
                    }),
                ));
            };
            let pass = delivered_at > 0 && (!require_ack || acked_at.is_some());
            Ok(evaluation_with_link(
                if pass {
                    CriterionVerificationResult::Pass
                } else {
                    CriterionVerificationResult::Fail
                },
                &link,
                json!({
                    "acked":acked_at.is_some(),
                    "delivered":delivered_at > 0,
                    "recipient_digest":sha256_text(recipient_principal),
                    "require_ack":require_ack,
                }),
            ))
        }
        DeliveryPredicate::RemoteProvider {
            provider,
            provider_message_id_digest,
        } => Ok(Evaluation {
            result: CriterionVerificationResult::Unknown,
            evidence: json!({
                "authority_refs":[],
                "observation":{
                    "provider":provider,
                    "provider_message_id_digest":provider_message_id_digest,
                    "reason":"remote_delivery_or_bounce_authority_unavailable",
                },
                "predicate_version":"v1",
            }),
            receipt_evidence: vec![],
        }),
    }
}

fn evaluate_provider_state(
    conn: &Connection,
    delegation_id: &str,
    attempt_id: &str,
    expected: ProviderAttemptPredicate,
) -> Result<Evaluation> {
    let row = conn
        .query_row(
            "SELECT status,provider_response_digest,remote_effect_id,provider_error_class FROM execass_provider_attempts WHERE delegation_id=?1 AND attempt_id=?2",
            params![delegation_id,attempt_id],
            |row| Ok((row.get::<_, String>(0)?,row.get::<_, Option<String>>(1)?,row.get::<_, Option<String>>(2)?,row.get::<_, Option<String>>(3)?)),
        )
        .optional()?;
    let Some((status, response_digest, remote_effect_id, error_class)) = row else {
        return Ok(Evaluation {
            result: CriterionVerificationResult::Unknown,
            evidence: json!({
                "authority_refs":[],
                "observation":{"attempt_id_digest":sha256_text(attempt_id),"reason":"provider_attempt_missing"},
                "predicate_version":"v1",
            }),
            receipt_evidence: vec![],
        });
    };
    let result = match status.as_str() {
        "prepared" | "invoking" | "outcome_unknown" => CriterionVerificationResult::Unknown,
        "succeeded" | "failed" | "reconciled_absent" | "reconciled_present" => {
            if status == expected.as_str() {
                CriterionVerificationResult::Pass
            } else {
                CriterionVerificationResult::Fail
            }
        }
        _ => CriterionVerificationResult::Unknown,
    };
    Ok(Evaluation {
        result,
        evidence: json!({
            "authority_refs":[],
            "observation":{
                "attempt_id_digest":sha256_text(attempt_id),
                "expected_status":expected.as_str(),
                "has_error_class":error_class.is_some(),
                "has_provider_response_digest":response_digest.is_some(),
                "has_remote_effect_id":remote_effect_id.is_some(),
                "status":status,
            },
            "predicate_version":"v1",
        }),
        receipt_evidence: vec![],
    })
}

#[cfg(test)]
pub(super) fn evaluate_provider_state_for_test(
    conn: &Connection,
    delegation_id: &str,
    attempt_id: &str,
    expected: ProviderAttemptPredicate,
) -> Result<CriterionVerificationResult> {
    Ok(evaluate_provider_state(conn, delegation_id, attempt_id, expected)?.result)
}

fn evaluate_database_predicate(
    conn: &Connection,
    delegation_id: &str,
    minimum_exclusive: i64,
) -> Result<Evaluation> {
    let current = conn
        .query_row(
            r#"SELECT d.current_plan_revision,
                      EXISTS(SELECT 1 FROM execass_plans p WHERE p.delegation_id=d.delegation_id AND p.plan_revision=d.current_plan_revision)
               FROM execass_delegations d WHERE d.delegation_id=?1"#,
            [delegation_id],
            |row| Ok((row.get::<_, Option<i64>>(0)?,row.get::<_, bool>(1)?)),
        )
        .optional()?;
    let Some((revision, plan_exists)) = current else {
        return Ok(Evaluation {
            result: CriterionVerificationResult::Unknown,
            evidence: json!({"authority_refs":[],"observation":{"reason":"delegation_missing"},"predicate_version":"v1"}),
            receipt_evidence: vec![],
        });
    };
    let pass = revision.is_some_and(|value| value > minimum_exclusive) && plan_exists;
    Ok(Evaluation {
        result: if pass {
            CriterionVerificationResult::Pass
        } else {
            CriterionVerificationResult::Fail
        },
        evidence: json!({
            "authority_refs":[],
            "observation":{
                "current_plan_revision":revision,
                "minimum_exclusive":minimum_exclusive,
                "plan_row_exists":plan_exists,
            },
            "predicate_version":"v1",
        }),
        receipt_evidence: vec![],
    })
}

fn compare_optional_text(
    link: &LinkSource,
    actual: Option<String>,
    expected: &str,
    field: &str,
) -> Evaluation {
    match actual {
        Some(actual) => evaluation_with_link(
            if actual == expected {
                CriterionVerificationResult::Pass
            } else {
                CriterionVerificationResult::Fail
            },
            link,
            json!({"actual":actual,"expected":expected,"field":field}),
        ),
        None => evaluation_with_link(
            CriterionVerificationResult::Unknown,
            link,
            json!({"field":field,"reason":"authoritative_row_missing"}),
        ),
    }
}

fn compare_optional_bool(
    link: &LinkSource,
    actual: Option<bool>,
    expected: bool,
    field: &str,
) -> Evaluation {
    match actual {
        Some(actual) => evaluation_with_link(
            if actual == expected {
                CriterionVerificationResult::Pass
            } else {
                CriterionVerificationResult::Fail
            },
            link,
            json!({"actual":actual,"expected":expected,"field":field}),
        ),
        None => evaluation_with_link(
            CriterionVerificationResult::Unknown,
            link,
            json!({"field":field,"reason":"authoritative_row_missing"}),
        ),
    }
}

fn evaluation_with_link(
    result: CriterionVerificationResult,
    link: &LinkSource,
    observation: Value,
) -> Evaluation {
    evaluation_with_refs(result, vec![receipt_evidence(link)], link, observation)
}

fn evaluation_with_refs(
    result: CriterionVerificationResult,
    receipt_evidence: Vec<ReceiptEvidenceInput>,
    link: &LinkSource,
    observation: Value,
) -> Evaluation {
    Evaluation {
        result,
        evidence: json!({
            "authority_refs":[{
                "authoritative_revision":link.authoritative_revision,
                "kind":link.kind.as_str(),
                "link_id":link.link_id,
                "source_id_digest":sha256_text(&link.source_id),
            }],
            "observation":observation,
            "predicate_version":"v1",
        }),
        receipt_evidence,
    }
}

fn unknown_missing_link(link_id: &str) -> Evaluation {
    Evaluation {
        result: CriterionVerificationResult::Unknown,
        evidence: json!({
            "authority_refs":[],
            "observation":{"link_id_digest":sha256_text(link_id),"reason":"authority_link_missing"},
            "predicate_version":"v1",
        }),
        receipt_evidence: vec![],
    }
}

fn unknown_wrong_link(link: &LinkSource, reason: &str) -> Evaluation {
    evaluation_with_link(
        CriterionVerificationResult::Unknown,
        link,
        json!({"reason":reason}),
    )
}

fn receipt_evidence(link: &LinkSource) -> ReceiptEvidenceInput {
    ReceiptEvidenceInput {
        authority_link_id: link.link_id.clone(),
        kind: link.kind,
        source_id: link.source_id.clone(),
        authoritative_revision: link.authoritative_revision,
    }
}

fn load_criterion(
    conn: &Connection,
    delegation_id: &str,
    criterion_id: &str,
) -> Result<Option<CriterionRow>> {
    conn.query_row(
        "SELECT verifier_type,expected_predicate_json,authoritative_source_kind,criteria_revision FROM execass_outcome_criteria WHERE delegation_id=?1 AND criterion_id=?2",
        params![delegation_id,criterion_id],
        |row| Ok(CriterionRow { verifier_type:row.get(0)?,predicate_json:row.get(1)?,authoritative_source_kind:row.get(2)?,criteria_revision:row.get(3)? }),
    )
    .optional()
    .map_err(Into::into)
}

fn load_result(
    conn: &Connection,
    verifier_result_id: &str,
) -> Result<Option<VerifierResultRecord>> {
    conn.query_row(
        "SELECT verifier_result_id,delegation_id,criterion_id,result_revision,result,evidence_refs_json,evidence_digest,verifier_identity,verified_at FROM execass_verifier_results WHERE verifier_result_id=?1",
        [verifier_result_id],
        |row| Ok(VerifierResultRecord { verifier_result_id:row.get(0)?,delegation_id:row.get(1)?,criterion_id:row.get(2)?,result_revision:row.get(3)?,result:row.get(4)?,evidence_refs_json:row.get(5)?,evidence_digest:row.get(6)?,verifier_identity:row.get(7)?,verified_at:row.get(8)? }),
    )
    .optional()
    .map_err(Into::into)
}

fn load_link(conn: &Connection, delegation_id: &str, link_id: &str) -> Result<Option<LinkSource>> {
    conn.query_row(
        r#"SELECT authority_kind,
          COALESCE(session_id,run_id,job_id,job_run_id,task_id,board_id,board_card_id,
                   mail_thread_id,mail_message_id,attachment_id,board_card_asset_id,
                   mail_attachment_id,security_audit_event_id,assistant_tool_call_audit_event_id,
                   tool_call_id),authoritative_revision
          FROM execass_authority_links WHERE delegation_id=?1 AND link_id=?2"#,
        params![delegation_id, link_id],
        |row| {
            Ok(LinkSource {
                link_id: link_id.to_string(),
                kind: row.get(0)?,
                source_id: row.get(1)?,
                authoritative_revision: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn duplicate_outbox_identity_exists(conn: &Connection, identity: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM execass_outbox_events WHERE duplicate_identity=?1)",
        [identity],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_text(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}
