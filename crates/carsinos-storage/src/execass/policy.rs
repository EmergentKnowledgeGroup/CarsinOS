//! Canonical ExecAss operational-policy revision authority.
//!
//! Exact owner amendments append one immutable safe snapshot and advance the
//! sole global pointer in the existing receipt/outbox transaction. This is not
//! a decision or approval engine.

use super::foundation::authority_record_from_manifest;
use super::receipt::{
    receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{get_outbox, insert_authority, insert_outbox};
use super::store::ExecAssStore;
use super::types::*;
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::VerifiedOwnerAuthority;
use carsinos_core::execass_manifest::canonicalize_owner_authority;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};

const POLICY_AGGREGATE_ID: &str = "execass-policy";
const GLOBAL_RECEIPT_CARRIER_ID: &str = "execass-global-control-carrier";

enum PolicyMutation {
    Updated(ExecAssPolicyRevisionRecord, OutboxEventRecord),
    Replayed(
        ExecAssPolicyRevisionRecord,
        OutboxEventRecord,
        ReceiptRecord,
    ),
    Stale(i64),
    Conflict,
}

impl ExecAssStore {
    pub fn current_execass_policy(&self) -> Result<ExecAssPolicyRevisionRecord> {
        let conn = self.connection()?;
        load_current_policy(&conn)
    }

    pub fn update_execass_policy_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &UpdateExecAssPolicyCommand,
        owner_authority: &VerifiedOwnerAuthority,
    ) -> Result<ExecAssPolicyUpdateOutcome> {
        let snapshot_json = canonical_object_json(&command.safe_policy_snapshot)?;
        let snapshot_digest = digest(snapshot_json.as_bytes());
        let next_revision = command
            .expected_policy_revision
            .checked_add(1)
            .context("policy revision overflow")?;
        let canonical = canonicalize_owner_authority(owner_authority)
            .map_err(|detail| anyhow::anyhow!("invalid policy owner authority: {detail}"))?;
        let authority = authority_record_from_manifest(&canonical)?;
        validate_command(command, &authority, next_revision, &snapshot_digest)?;
        let request_digest = mutation_digest(
            command,
            &authority.authority_provenance_id,
            &snapshot_digest,
        );

        let atomic = self.mutate_with_atomic_receipt(
            integrity,
            redactor,
            &command.receipt,
            |tx| {
                if let Some(existing) = load_policy_by_idempotency(tx, &command.idempotency_key)? {
                    if existing.request_digest != request_digest
                        || existing.actor_type != authority.actor_type
                        || existing.credential_identity != authority.credential_identity
                    {
                        return Ok(AtomicReceiptMutation::NoAppend(PolicyMutation::Conflict));
                    }
                    let receipt = receipt_by_causation_event(tx, &existing.outbox_event_id)?
                        .context("policy replay receipt is missing")?;
                    let outbox = get_outbox(tx, &existing.outbox_event_id)?
                        .context("policy replay outbox event is missing")?;
                    return Ok(AtomicReceiptMutation::NoAppend(PolicyMutation::Replayed(
                        existing.record,
                        outbox,
                        receipt,
                    )));
                }
                let current = current_pointer(tx)?;
                if current != command.expected_policy_revision {
                    return Ok(AtomicReceiptMutation::NoAppend(PolicyMutation::Stale(
                        current,
                    )));
                }
                if authority_exists(tx, &authority.authority_provenance_id)? {
                    return Ok(AtomicReceiptMutation::NoAppend(PolicyMutation::Conflict));
                }
                insert_authority(tx, &authority)?;
                insert_outbox(tx, &command.outbox_event)?;
                tx.execute(
                    r#"INSERT INTO execass_policy_revisions(
                      policy_revision,idempotency_key,policy_snapshot_json,policy_snapshot_digest,
                      request_digest,authority_provenance_id,outbox_event_id,receipt_id,created_at
                    ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
                    params![
                        next_revision,
                        command.idempotency_key,
                        snapshot_json,
                        snapshot_digest,
                        request_digest,
                        authority.authority_provenance_id,
                        command.outbox_event.event_id,
                        command.receipt.receipt_id,
                        command.created_at,
                    ],
                )
                .context("failed appending canonical ExecAss policy revision")?;
                let changed = tx.execute(
                    "UPDATE execass_global_runtime_control SET current_policy_revision=?1,updated_at=?2 WHERE singleton=1 AND current_policy_revision=?3",
                    params![next_revision, command.created_at, command.expected_policy_revision],
                )?;
                if changed != 1 {
                    bail!("canonical policy pointer CAS failed after mutation began");
                }
                let record = load_policy_revision(tx, next_revision)?
                    .context("new policy revision disappeared")?;
                let outbox = get_outbox(tx, &command.outbox_event.event_id)?
                    .context("new policy outbox event disappeared")?;
                Ok(AtomicReceiptMutation::Append(PolicyMutation::Updated(
                    record, outbox,
                )))
            },
        )?;
        map_atomic(atomic)
    }
}

fn map_atomic(
    atomic: AtomicReceiptWriteOutcome<PolicyMutation>,
) -> Result<ExecAssPolicyUpdateOutcome> {
    match atomic {
        AtomicReceiptWriteOutcome::Appended {
            value: PolicyMutation::Updated(policy, outbox_event),
            receipt,
        } => Ok(ExecAssPolicyUpdateOutcome::Updated {
            policy,
            outbox_event,
            receipt,
        }),
        AtomicReceiptWriteOutcome::NoAppend(PolicyMutation::Replayed(
            policy,
            outbox_event,
            receipt,
        )) => Ok(ExecAssPolicyUpdateOutcome::Replayed {
            policy,
            outbox_event,
            receipt,
        }),
        AtomicReceiptWriteOutcome::NoAppend(PolicyMutation::Stale(current_policy_revision)) => {
            Ok(ExecAssPolicyUpdateOutcome::Stale {
                current_policy_revision,
            })
        }
        AtomicReceiptWriteOutcome::NoAppend(PolicyMutation::Conflict) => {
            Ok(ExecAssPolicyUpdateOutcome::Conflict)
        }
        AtomicReceiptWriteOutcome::Stale { .. } => {
            bail!("canonical policy receipt carrier revision changed")
        }
        _ => bail!("canonical policy mutation returned an impossible receipt outcome"),
    }
}

fn validate_command(
    command: &UpdateExecAssPolicyCommand,
    authority: &AuthorityProvenanceRecord,
    next_revision: i64,
    snapshot_digest: &str,
) -> Result<()> {
    if command.expected_policy_revision <= 0
        || command.created_at <= 0
        || command.idempotency_key.trim().is_empty()
        || authority.authority_kind != AuthorityKind::PolicySnapshot
        || !matches!(
            authority.actor_type,
            ActorType::HumanLocal | ActorType::HumanRemote
        )
        || authority.policy_revision != next_revision
        || authority.created_at != command.created_at
        || authority.expires_at.is_some()
        || authority.source_correlation_id != command.outbox_event.correlation_id
        || command.receipt.receipt_kind != ReceiptKind::Policy
        || command.receipt.subject.kind != ReceiptSubjectKind::PolicyRevision
        || command.receipt.subject.subject_id != POLICY_AGGREGATE_ID
        || command.receipt.subject.revision != next_revision
        || command.receipt.delegation_id != GLOBAL_RECEIPT_CARRIER_ID
        || command.receipt.causation_event_id != command.outbox_event.event_id
        || command.receipt.causation_id != command.outbox_event.causation_id
        || command.receipt.occurred_at != command.created_at
        || command.receipt.committed_at != command.created_at
        || command.receipt.actor.authority_provenance_id != authority.authority_provenance_id
        || command.receipt.actor.actor_type != authority.actor_type
        || command.receipt.actor.actor_identity.as_str() != authority.credential_identity
        || command.outbox_event.event_name != OutboxEventName::PolicyChanged
        || command.outbox_event.aggregate_id != POLICY_AGGREGATE_ID
        || command.outbox_event.aggregate_revision != next_revision
        || command.outbox_event.duplicate_identity != command.idempotency_key
        || command.outbox_event.occurred_at != command.created_at
    {
        bail!("policy update is not bound to its exact owner, revision, receipt, and outbox event");
    }
    let expected_scope = json!({
        "policy_revision": next_revision,
        "policy_snapshot_digest": snapshot_digest,
    });
    if serde_json::from_str::<serde_json::Value>(&authority.normalized_scope_json)?
        != expected_scope
    {
        bail!("policy owner authority is not bound to the exact safe snapshot");
    }
    let expected_payload = json!({
        "configured": true,
        "policy_revision": next_revision,
        "policy_snapshot_digest": snapshot_digest,
    })
    .to_string();
    if command.outbox_event.safe_payload_json != expected_payload {
        bail!("policy outbox payload is not the deterministic safe revision projection");
    }
    Ok(())
}

struct StoredPolicyReplay {
    record: ExecAssPolicyRevisionRecord,
    request_digest: String,
    outbox_event_id: String,
    actor_type: ActorType,
    credential_identity: String,
}

fn load_policy_by_idempotency(
    conn: &Connection,
    idempotency_key: &str,
) -> Result<Option<StoredPolicyReplay>> {
    conn.query_row(
        "SELECT policy.policy_revision,policy.policy_snapshot_json,policy.policy_snapshot_digest,policy.authority_provenance_id,policy.created_at,policy.request_digest,policy.outbox_event_id,authority.actor_type,authority.credential_identity FROM execass_policy_revisions policy JOIN execass_authority_provenance authority ON authority.authority_provenance_id=policy.authority_provenance_id WHERE policy.idempotency_key=?1",
        [idempotency_key],
        |row| Ok(StoredPolicyReplay {
            record: ExecAssPolicyRevisionRecord { policy_revision: row.get(0)?, policy_snapshot_json: row.get(1)?, policy_snapshot_digest: row.get(2)?, authority_provenance_id: row.get(3)?, created_at: row.get(4)? },
            request_digest: row.get(5)?, outbox_event_id: row.get(6)?, actor_type: row.get(7)?, credential_identity: row.get(8)?,
        }),
    ).optional().map_err(Into::into)
}

fn load_policy_revision(
    conn: &Connection,
    revision: i64,
) -> Result<Option<ExecAssPolicyRevisionRecord>> {
    conn.query_row(
        "SELECT policy_revision,policy_snapshot_json,policy_snapshot_digest,authority_provenance_id,created_at FROM execass_policy_revisions WHERE policy_revision=?1",
        [revision],
        |row| Ok(ExecAssPolicyRevisionRecord { policy_revision: row.get(0)?, policy_snapshot_json: row.get(1)?, policy_snapshot_digest: row.get(2)?, authority_provenance_id: row.get(3)?, created_at: row.get(4)? }),
    ).optional().map_err(Into::into)
}

fn load_current_policy(conn: &Connection) -> Result<ExecAssPolicyRevisionRecord> {
    let current = current_pointer(conn)?;
    let maximum: i64 = conn.query_row(
        "SELECT MAX(policy_revision) FROM execass_policy_revisions",
        [],
        |row| row.get(0),
    )?;
    if maximum != current {
        bail!("canonical policy pointer does not equal immutable history head");
    }
    load_policy_revision(conn, current)?.context("canonical policy pointer has no revision")
}

fn current_pointer(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT current_policy_revision FROM execass_global_runtime_control WHERE singleton=1",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn authority_exists(conn: &Connection, authority_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM execass_authority_provenance WHERE authority_provenance_id=?1",
            [authority_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn canonical_object_json(value: &super::redaction::SafeJson) -> Result<String> {
    let bytes = value.canonical_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
    if !parsed.is_object() {
        bail!("canonical policy snapshot must be a JSON object");
    }
    String::from_utf8(bytes).context("canonical policy JSON is not UTF-8")
}

fn mutation_digest(
    command: &UpdateExecAssPolicyCommand,
    _authority_id: &str,
    snapshot_digest: &str,
) -> String {
    digest(
        json!({
            "expected_policy_revision": command.expected_policy_revision,
            "idempotency_key": command.idempotency_key,
            "policy_snapshot_digest": snapshot_digest,
        })
        .to_string()
        .as_bytes(),
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
