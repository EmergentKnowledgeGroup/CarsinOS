//! Canonical, redacted, globally anchored ExecAss receipt authority.

use super::canonical::{parse_strict_json, CanonicalValue};
use super::receipt_integrity::{
    receipt_key_registry_integrity_tag, AnchorCommitInput, IntegrityStatus, ReceiptIntegrityStore,
    ReceiptKeyRef,
};
use super::redaction::{ReceiptRedactor, SafeText};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::sync::{Arc, Mutex};

const SERIALIZATION_VERSION: &str = "carsinos.execass.receipt.cjson.v1";
const HASH_ALGORITHM: &str = "sha256";
const TAG_ALGORITHM: &str = "hmac-sha256";
const DIGEST_DOMAIN: &[u8] = b"carsinos.execass.receipt.digest.v1";
const TAG_DOMAIN: &[u8] = b"carsinos.execass.receipt.tag.v1";
const ROTATION_DOMAIN: &[u8] = b"carsinos.execass.receipt.rotation.previous.v1";
const APPEND_DOMAIN: &[u8] = b"carsinos.execass.receipt.append.v1";
const EVIDENCE_OBSERVATION_DOMAIN: &[u8] = b"carsinos.execass.receipt.evidence-observation.v1";
type HmacSha256 = Hmac<Sha256>;
type RegisteredReceiptKeyState = (String, Option<String>, Option<i64>, Option<i64>);
static APPEND_LOCK: Mutex<()> = Mutex::new(());

struct AppendCoordinatorGuard {
    file: File,
}

impl AppendCoordinatorGuard {
    fn acquire(store: &ExecAssStore) -> Result<Self> {
        let path = store.db_path.with_extension("receipt-append.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .context("failed opening the configured receipt append lock")?;
        fs2::FileExt::lock_exclusive(&file)
            .context("failed acquiring the configured receipt append lock")?;
        Ok(Self { file })
    }
}

impl Drop for AppendCoordinatorGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub(crate) trait ReceiptFailpoints: Send + Sync {
    fn hit(&self, name: &'static str) -> Result<()>;
}

#[derive(Default)]
struct NoReceiptFailpoints;
impl ReceiptFailpoints for NoReceiptFailpoints {
    fn hit(&self, _name: &'static str) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ChainPosition {
    global_sequence: i64,
    delegation_sequence: i64,
    global_parent_id: Option<String>,
    delegation_parent_id: Option<String>,
    global_parent_digest: Option<String>,
    delegation_parent_digest: Option<String>,
}

#[derive(Debug)]
struct Snapshot {
    state_revision: i64,
    global_count: i64,
    global_head: Option<String>,
    global_parent_id: Option<String>,
    delegation_count: i64,
    delegation_head: Option<String>,
    delegation_parent_id: Option<String>,
}

#[derive(Debug)]
struct EventBinding {
    global_sequence: i64,
    event_name: String,
    aggregate_id: String,
    aggregate_revision: i64,
    correlation_id: String,
    causation_id: String,
    occurred_at: i64,
    schema_version: String,
}

#[derive(Debug)]
struct ActorFacts {
    credential_identity: String,
    authenticated_ingress: String,
    channel_assurance: String,
}

#[derive(Debug)]
struct RuntimeFacts {
    installation_identity: String,
    os_user_identity_digest: String,
    ownership_scope: String,
}

#[derive(Debug)]
struct ReceiptKeyRegistryRow {
    key_id: String,
    key_generation: i64,
    status: String,
    rotated_from_key_id: Option<String>,
    rotated_from_key_generation: Option<i64>,
    created_at: i64,
    registry_integrity_tag: String,
    activated_anchor_generation: Option<i64>,
}

#[derive(Debug, Clone)]
struct NormalizedEvidence {
    authority_link_id: String,
    kind: AuthorityLinkKind,
    source_id: String,
    authoritative_revision: i64,
    observation_digest: String,
    deep_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReceiptHistoryFailure {
    pub(super) category: &'static str,
    pub(super) reason: &'static str,
    pub(super) first_global_sequence: Option<i64>,
    pub(super) first_receipt_id: Option<String>,
    pub(super) first_delegation_id: Option<String>,
    pub(super) first_delegation_sequence: Option<i64>,
}

pub(super) enum AtomicReceiptMutation<T> {
    Append(T),
    NoAppend(T),
}

pub(super) enum AtomicReceiptWriteOutcome<T> {
    Appended {
        value: T,
        receipt: ReceiptRecord,
    },
    NoAppend(T),
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
}

pub(super) struct RuntimeReceiptCommand {
    pub receipt_id: String,
    pub transaction_id: String,
    pub state_root_generation: i64,
    pub expected_global_count: i64,
    pub expected_global_head_digest: Option<String>,
    pub subject_generation: i64,
    pub causation_id: String,
    pub causation_event_id: String,
    pub actor: ReceiptActorBinding,
    pub runtime: ReceiptRuntimeBinding,
    pub key: ReceiptKeyRef,
    pub redacted_summary: SafeText,
    pub occurred_at: i64,
}

pub(super) enum RuntimeAtomicReceiptMutation<T> {
    Append {
        value: T,
        command: Box<RuntimeReceiptCommand>,
    },
    NoAppend(T),
}

impl ReceiptHistoryFailure {
    pub(super) fn reason_code(&self) -> String {
        match self.first_global_sequence {
            Some(sequence) => format!("{}_at_global_sequence_{sequence}", self.reason),
            None => self.reason.to_owned(),
        }
    }
}

impl ExecAssStore {
    pub fn append_receipt(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &AppendReceiptCommand,
    ) -> Result<AppendReceiptOutcome> {
        self.append_receipt_with_failpoints(
            integrity,
            redactor,
            command,
            Arc::new(NoReceiptFailpoints),
        )
    }

    pub(crate) fn append_receipt_with_failpoints(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &AppendReceiptCommand,
        failpoints: Arc<dyn ReceiptFailpoints>,
    ) -> Result<AppendReceiptOutcome> {
        let _append_guard = APPEND_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("receipt append coordinator is poisoned"))?;
        redactor.reject_sensitive_bytes(self.db_path.to_string_lossy().as_bytes())?;
        redactor
            .reject_sensitive_bytes(integrity.anchor_directory().to_string_lossy().as_bytes())?;
        let _process_guard = AppendCoordinatorGuard::acquire(self)?;
        require_receipt_integrity_available(integrity)?;
        validate_command(command)?;
        let append_identity = append_identity(command);
        let connection = self.connection()?;
        let event = load_event(&connection, command)?;
        let actor = validate_actor(&connection, command)?;
        let runtime = validate_runtime(&connection, command)?;
        validate_subject(&connection, command)?;
        let evidence = normalize_and_validate_evidence(&connection, command)?;

        if let Some((record, position)) = receipt_by_append_identity(&connection, &append_identity)?
        {
            let candidate = build_payload(
                command,
                &append_identity,
                &position,
                &event,
                &actor,
                &runtime,
                &evidence,
            )?;
            return if replay_expectations_match(command, &position)
                && candidate == record.canonical_payload
            {
                Ok(AppendReceiptOutcome::Replayed(record))
            } else {
                Ok(AppendReceiptOutcome::Conflict { append_identity })
            };
        }

        let snapshot = load_snapshot(&connection, &command.delegation_id)?
            .context("ExecAss receipt delegation does not exist")?;
        if !snapshot_matches(command, &snapshot) {
            return Ok(stale(snapshot));
        }
        let position = next_position(&snapshot)?;
        let canonical_payload = build_payload(
            command,
            &append_identity,
            &position,
            &event,
            &actor,
            &runtime,
            &evidence,
        )?;
        redactor.reject_sensitive_bytes(&canonical_payload)?;
        let receipt_digest = digest_hex(DIGEST_DOMAIN, &canonical_payload);
        let current_key = integrity
            .load_key(&command.key)
            .context("receipt signing key is unavailable")?;
        let keyed_integrity_tag = receipt_tag(
            &current_key,
            TAG_DOMAIN,
            integrity.root_identity(),
            command.state_root_generation,
            &command.key,
            &receipt_digest,
        )?;
        let previous_key_integrity_tag =
            rotation_tag(&connection, integrity, command, &receipt_digest)?;
        let anchor_generation = validate_integrity_progression(integrity, command)?;
        let prepared = integrity.prepare_anchor(&AnchorCommitInput {
            state_root_generation: command.state_root_generation,
            anchor_generation,
            receipt_count: position.global_sequence,
            receipt_head_digest: Some(receipt_digest.clone()),
            key: command.key.clone(),
            transaction_id: command.transaction_id.clone(),
            external_receipt_digest: receipt_digest.clone(),
            occurred_at: command.committed_at,
        })?;
        failpoints.hit("receipt.after_prepare")?;

        let transaction_result = (|| -> Result<AppendReceiptOutcome> {
            let mut connection = self.connection()?;
            let transaction = immediate_transaction(&mut connection)?;
            let current = load_snapshot(&transaction, &command.delegation_id)?
                .context("ExecAss receipt delegation disappeared")?;
            if !snapshot_matches(command, &current) {
                if let Some((record, _)) =
                    receipt_by_append_identity(&transaction, &append_identity)?
                {
                    return if replay_expectations_match(command, &position)
                        && record.canonical_payload == canonical_payload
                    {
                        Ok(AppendReceiptOutcome::Replayed(record))
                    } else {
                        Ok(AppendReceiptOutcome::Conflict {
                            append_identity: append_identity.clone(),
                        })
                    };
                }
                return Ok(stale(current));
            }
            validate_event_in_transaction(&transaction, command, &event)?;
            validate_subject(&transaction, command)?;
            validate_actor(&transaction, command)?;
            validate_runtime(&transaction, command)?;
            normalize_and_validate_evidence(&transaction, command)?;

            transaction.execute(
                r#"INSERT INTO execass_receipts (
                  receipt_id,delegation_id,receipt_sequence,global_sequence,append_identity,receipt_kind,
                  causation_id,causation_event_id,parent_receipt_id,global_parent_receipt_id,
                  subject_kind,subject_id,subject_revision,actor_type,actor_identity,
                  actor_authority_provenance_id,runtime_host_generation,runtime_host_instance_id,
                  runtime_fencing_token,state_revision,canonical_payload,serialization_version,
                  hash_algorithm,key_id,key_generation,previous_receipt_digest,
                  global_previous_receipt_digest,receipt_digest,keyed_integrity_tag,
                  previous_key_integrity_tag,redacted_summary,occurred_at,committed_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                  ?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33)"#,
                params![
                    command.receipt_id, command.delegation_id, position.delegation_sequence,
                    position.global_sequence, append_identity, command.receipt_kind.as_str(),
                    command.causation_id, command.causation_event_id, position.delegation_parent_id,
                    position.global_parent_id, command.subject.kind.as_str(), command.subject.subject_id,
                    command.subject.revision, command.actor.actor_type.as_str(), command.actor.actor_identity.as_str(),
                    command.actor.authority_provenance_id, command.runtime.host_generation,
                    command.runtime.host_instance_id, command.runtime.fencing_token, command.expected_state_revision,
                    canonical_payload, SERIALIZATION_VERSION, HASH_ALGORITHM, command.key.key_id,
                    command.key.key_generation, position.delegation_parent_digest,
                    position.global_parent_digest, receipt_digest, keyed_integrity_tag,
                    previous_key_integrity_tag, command.redacted_summary.as_str(), command.occurred_at,
                    command.committed_at,
                ],
            ).context("failed inserting canonical ExecAss receipt")?;
            failpoints.hit("receipt.after_insert")?;
            insert_evidence(&transaction, &command.receipt_id, &evidence)?;
            failpoints.hit("receipt.after_evidence")?;
            transaction.execute(
                "UPDATE execass_receipt_journal_state SET receipt_count=?1,receipt_head_digest=?2,latest_receipt_id=?3 WHERE singleton=1 AND receipt_count=?4 AND receipt_head_digest IS ?5",
                params![position.global_sequence, receipt_digest, command.receipt_id, command.expected_global_count, command.expected_global_head_digest],
            ).context("failed advancing global receipt journal")?;
            failpoints.hit("receipt.after_global_head")?;
            let changed = transaction.execute(
                "UPDATE execass_delegations SET receipt_chain_count=?1,receipt_chain_head_digest=?2 WHERE delegation_id=?3 AND state_revision=?4 AND receipt_chain_count=?5 AND receipt_chain_head_digest IS ?6",
                params![position.delegation_sequence, receipt_digest, command.delegation_id,
                    command.expected_state_revision, command.expected_delegation_count, command.expected_delegation_head_digest],
            )?;
            if changed != 1 {
                bail!("delegation receipt head changed during append");
            }
            failpoints.hit("receipt.after_delegation_head")?;
            integrity.confirm_prepared_anchor_in_transaction(
                &transaction,
                &prepared.transaction_id,
                position.global_sequence,
                Some(&receipt_digest),
                command.committed_at,
            )?;
            failpoints.hit("receipt.after_confirm")?;
            let record = load_receipt(&transaction, &command.receipt_id)?
                .context("canonical receipt disappeared before commit")?;
            if record.canonical_payload != canonical_payload
                || record.receipt_digest != receipt_digest
            {
                bail!("canonical receipt reconstruction mismatch");
            }
            transaction
                .commit()
                .context("failed committing canonical ExecAss receipt")?;
            failpoints.hit("receipt.after_commit")?;
            Ok(AppendReceiptOutcome::Appended(record))
        })();

        let outcome = match transaction_result {
            Ok(outcome @ AppendReceiptOutcome::Appended(_)) => outcome,
            Ok(other) => {
                let _ = integrity.recover_integrity();
                return Ok(other);
            }
            Err(error) => {
                let _ = integrity.recover_integrity();
                return Err(error);
            }
        };
        failpoints.hit("receipt.before_finalize")?;
        integrity.finalize_anchor(&prepared.transaction_id)?;
        failpoints.hit("receipt.after_finalize")?;
        Ok(outcome)
    }

    /// Runs one typed aggregate mutation and its canonical receipt append in
    /// the same SQLite writer transaction. The mutation must insert the exact
    /// outbox event named by `command` before returning `Append`.
    ///
    /// This remains crate-private so no caller can acquire a generic storage
    /// transaction. Typed aggregate modules are the only consumers.
    pub(super) fn mutate_with_atomic_receipt<T, F>(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &AppendReceiptCommand,
        mutation: F,
    ) -> Result<AtomicReceiptWriteOutcome<T>>
    where
        F: FnOnce(&Transaction<'_>) -> Result<AtomicReceiptMutation<T>>,
    {
        self.mutate_with_atomic_receipt_at_revision(
            integrity,
            redactor,
            command,
            command.expected_state_revision,
            false,
            mutation,
        )
    }

    /// Runs a typed aggregate mutation that advances the delegation exactly
    /// one revision while appending the resulting event and receipt. The
    /// pre-mutation revision and receipt heads are compared before accepting
    /// the mutation; the closure must leave the delegation at the receipt's
    /// next revision. This remains storage-private and exposes no transaction.
    pub(super) fn mutate_with_advancing_atomic_receipt<T, F>(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        expected_pre_state_revision: i64,
        command: &AppendReceiptCommand,
        mutation: F,
    ) -> Result<AtomicReceiptWriteOutcome<T>>
    where
        F: FnOnce(&Transaction<'_>) -> Result<AtomicReceiptMutation<T>>,
    {
        if expected_pre_state_revision <= 0
            || command.expected_state_revision != expected_pre_state_revision + 1
        {
            bail!("advancing atomic receipt requires exactly one lifecycle revision");
        }
        self.mutate_with_atomic_receipt_at_revision(
            integrity,
            redactor,
            command,
            expected_pre_state_revision,
            true,
            mutation,
        )
    }

    fn mutate_with_atomic_receipt_at_revision<T, F>(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        command: &AppendReceiptCommand,
        expected_pre_state_revision: i64,
        precheck_revision_before_mutation: bool,
        mutation: F,
    ) -> Result<AtomicReceiptWriteOutcome<T>>
    where
        F: FnOnce(&Transaction<'_>) -> Result<AtomicReceiptMutation<T>>,
    {
        let _append_guard = APPEND_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("receipt append coordinator is poisoned"))?;
        redactor.reject_sensitive_bytes(self.db_path.to_string_lossy().as_bytes())?;
        redactor
            .reject_sensitive_bytes(integrity.anchor_directory().to_string_lossy().as_bytes())?;
        let _process_guard = AppendCoordinatorGuard::acquire(self)?;
        require_receipt_integrity_available(integrity)?;
        validate_command(command)?;

        let transaction_result = (|| -> Result<(AtomicReceiptWriteOutcome<T>, Option<String>)> {
            let mut connection = self.connection()?;
            let transaction = immediate_transaction(&mut connection)?;
            if receipt_by_causation_event(&transaction, &command.causation_event_id)?.is_some() {
                let value = match mutation(&transaction)? {
                    AtomicReceiptMutation::NoAppend(value) => value,
                    AtomicReceiptMutation::Append(_) => {
                        bail!("existing atomic receipt cannot be appended again")
                    }
                };
                transaction
                    .commit()
                    .context("failed closing existing atomic receipt replay")?;
                return Ok((AtomicReceiptWriteOutcome::NoAppend(value), None));
            }
            let snapshot = load_snapshot(&transaction, &command.delegation_id)?
                .context("ExecAss receipt delegation does not exist")?;
            let snapshot_matches =
                snapshot_matches_at_revision(command, &snapshot, expected_pre_state_revision);
            if precheck_revision_before_mutation && !snapshot_matches {
                transaction
                    .rollback()
                    .context("failed rolling back stale atomic receipt write")?;
                return Ok((
                    AtomicReceiptWriteOutcome::Stale {
                        current_state_revision: snapshot.state_revision,
                        global_count: snapshot.global_count,
                        global_head_digest: snapshot.global_head,
                        delegation_count: snapshot.delegation_count,
                        delegation_head_digest: snapshot.delegation_head,
                    },
                    None,
                ));
            }
            let value = match mutation(&transaction)? {
                AtomicReceiptMutation::NoAppend(value) => {
                    transaction
                        .commit()
                        .context("failed closing atomic receipt replay")?;
                    return Ok((AtomicReceiptWriteOutcome::NoAppend(value), None));
                }
                AtomicReceiptMutation::Append(value) => value,
            };
            if !snapshot_matches {
                transaction
                    .rollback()
                    .context("failed rolling back stale atomic receipt append")?;
                return Ok((
                    AtomicReceiptWriteOutcome::Stale {
                        current_state_revision: snapshot.state_revision,
                        global_count: snapshot.global_count,
                        global_head_digest: snapshot.global_head,
                        delegation_count: snapshot.delegation_count,
                        delegation_head_digest: snapshot.delegation_head,
                    },
                    None,
                ));
            }
            let resulting_state_revision: i64 = transaction.query_row(
                "SELECT state_revision FROM execass_delegations WHERE delegation_id=?1",
                [&command.delegation_id],
                |row| row.get(0),
            )?;
            if resulting_state_revision != command.expected_state_revision {
                bail!("atomic aggregate mutation did not produce the receipt revision");
            }

            let append_identity = append_identity(command);
            if receipt_by_append_identity(&transaction, &append_identity)?.is_some() {
                bail!("atomic receipt append identity already exists after a new mutation");
            }
            let event = load_event(&transaction, command)?;
            let actor = validate_actor(&transaction, command)?;
            let runtime = validate_runtime(&transaction, command)?;
            validate_subject(&transaction, command)?;
            let evidence = normalize_and_validate_evidence(&transaction, command)?;
            let position = next_position(&snapshot)?;
            let canonical_payload = build_payload(
                command,
                &append_identity,
                &position,
                &event,
                &actor,
                &runtime,
                &evidence,
            )?;
            redactor.reject_sensitive_bytes(&canonical_payload)?;
            let receipt_digest = digest_hex(DIGEST_DOMAIN, &canonical_payload);
            let current_key = integrity
                .load_key(&command.key)
                .context("receipt signing key is unavailable")?;
            let keyed_integrity_tag = receipt_tag(
                &current_key,
                TAG_DOMAIN,
                integrity.root_identity(),
                command.state_root_generation,
                &command.key,
                &receipt_digest,
            )?;
            let previous_key_integrity_tag =
                rotation_tag(&transaction, integrity, command, &receipt_digest)?;
            let anchor_generation = validate_integrity_progression(integrity, command)?;
            let prepared = integrity.prepare_anchor_in_transaction(
                &transaction,
                &AnchorCommitInput {
                    state_root_generation: command.state_root_generation,
                    anchor_generation,
                    receipt_count: position.global_sequence,
                    receipt_head_digest: Some(receipt_digest.clone()),
                    key: command.key.clone(),
                    transaction_id: command.transaction_id.clone(),
                    external_receipt_digest: receipt_digest.clone(),
                    occurred_at: command.committed_at,
                },
            )?;

            transaction.execute(
                r#"INSERT INTO execass_receipts (
                  receipt_id,delegation_id,receipt_sequence,global_sequence,append_identity,receipt_kind,
                  causation_id,causation_event_id,parent_receipt_id,global_parent_receipt_id,
                  subject_kind,subject_id,subject_revision,actor_type,actor_identity,
                  actor_authority_provenance_id,runtime_host_generation,runtime_host_instance_id,
                  runtime_fencing_token,state_revision,canonical_payload,serialization_version,
                  hash_algorithm,key_id,key_generation,previous_receipt_digest,
                  global_previous_receipt_digest,receipt_digest,keyed_integrity_tag,
                  previous_key_integrity_tag,redacted_summary,occurred_at,committed_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                  ?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33)"#,
                params![
                    command.receipt_id,
                    command.delegation_id,
                    position.delegation_sequence,
                    position.global_sequence,
                    append_identity,
                    command.receipt_kind.as_str(),
                    command.causation_id,
                    command.causation_event_id,
                    position.delegation_parent_id,
                    position.global_parent_id,
                    command.subject.kind.as_str(),
                    command.subject.subject_id,
                    command.subject.revision,
                    command.actor.actor_type.as_str(),
                    command.actor.actor_identity.as_str(),
                    command.actor.authority_provenance_id,
                    command.runtime.host_generation,
                    command.runtime.host_instance_id,
                    command.runtime.fencing_token,
                    command.expected_state_revision,
                    canonical_payload,
                    SERIALIZATION_VERSION,
                    HASH_ALGORITHM,
                    command.key.key_id,
                    command.key.key_generation,
                    position.delegation_parent_digest,
                    position.global_parent_digest,
                    receipt_digest,
                    keyed_integrity_tag,
                    previous_key_integrity_tag,
                    command.redacted_summary.as_str(),
                    command.occurred_at,
                    command.committed_at,
                ],
            )
            .context("failed inserting canonical atomic ExecAss receipt")?;
            insert_evidence(&transaction, &command.receipt_id, &evidence)?;
            transaction.execute(
                "UPDATE execass_receipt_journal_state SET receipt_count=?1,receipt_head_digest=?2,latest_receipt_id=?3 WHERE singleton=1 AND receipt_count=?4 AND receipt_head_digest IS ?5",
                params![position.global_sequence, receipt_digest, command.receipt_id, command.expected_global_count, command.expected_global_head_digest],
            ).context("failed advancing global receipt journal atomically")?;
            let changed = transaction.execute(
                "UPDATE execass_delegations SET receipt_chain_count=?1,receipt_chain_head_digest=?2 WHERE delegation_id=?3 AND state_revision=?4 AND receipt_chain_count=?5 AND receipt_chain_head_digest IS ?6",
                params![position.delegation_sequence, receipt_digest, command.delegation_id,
                    command.expected_state_revision, command.expected_delegation_count, command.expected_delegation_head_digest],
            )?;
            if changed != 1 {
                bail!("delegation receipt head changed during atomic append");
            }
            integrity.confirm_prepared_anchor_in_transaction(
                &transaction,
                &prepared.transaction_id,
                position.global_sequence,
                Some(&receipt_digest),
                command.committed_at,
            )?;
            let record = load_receipt(&transaction, &command.receipt_id)?
                .context("atomic receipt disappeared before commit")?;
            if record.canonical_payload != canonical_payload
                || record.receipt_digest != receipt_digest
            {
                bail!("atomic receipt reconstruction mismatch");
            }
            transaction
                .commit()
                .context("failed committing aggregate mutation and receipt")?;
            Ok((
                AtomicReceiptWriteOutcome::Appended {
                    value,
                    receipt: record,
                },
                Some(prepared.transaction_id),
            ))
        })();

        let (outcome, prepared_transaction_id) = match transaction_result {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = integrity.recover_integrity();
                return Err(error);
            }
        };
        if let Some(transaction_id) = prepared_transaction_id {
            integrity.finalize_anchor(&transaction_id)?;
        }
        Ok(outcome)
    }

    /// Runs a runtime-host aggregate mutation and appends its receipt to the
    /// same global journal and external anchor as delegation receipts. Runtime
    /// receipts deliberately have no delegation chain or carrier delegation.
    pub(super) fn mutate_with_runtime_atomic_receipt<T, F>(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        mutation: F,
    ) -> Result<T>
    where
        F: FnOnce(&Transaction<'_>, i64, Option<&str>) -> Result<RuntimeAtomicReceiptMutation<T>>,
    {
        let _append_guard = APPEND_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("receipt append coordinator is poisoned"))?;
        redactor.reject_sensitive_bytes(self.db_path.to_string_lossy().as_bytes())?;
        redactor
            .reject_sensitive_bytes(integrity.anchor_directory().to_string_lossy().as_bytes())?;
        let _process_guard = AppendCoordinatorGuard::acquire(self)?;
        require_receipt_integrity_available(integrity)?;

        let transaction_result = (|| -> Result<(T, Option<String>)> {
            let mut connection = self.connection()?;
            let transaction = immediate_transaction(&mut connection)?;
            let (global_count, global_head, global_parent_id): (
                i64,
                Option<String>,
                Option<String>,
            ) = transaction.query_row(
                "SELECT receipt_count,receipt_head_digest,latest_receipt_id FROM execass_receipt_journal_state WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let mutation = mutation(&transaction, global_count, global_head.as_deref())?;
            let RuntimeAtomicReceiptMutation::Append { value, command } = mutation else {
                let RuntimeAtomicReceiptMutation::NoAppend(value) = mutation else {
                    unreachable!()
                };
                transaction.commit()?;
                return Ok((value, None));
            };
            validate_runtime_receipt_command(&command, global_count, global_head.as_deref())?;
            let event = load_runtime_event(&transaction, &command)?;
            let actor = validate_runtime_receipt_actor(&transaction, &command)?;
            let runtime = validate_runtime_receipt_host(&transaction, &command)?;
            validate_runtime_receipt_subject(&transaction, &command)?;
            let global_sequence = global_count
                .checked_add(1)
                .context("global receipt sequence overflow")?;
            let append_identity = runtime_append_identity(&command);
            if transaction
                .query_row(
                    "SELECT 1 FROM execass_receipts WHERE append_identity=?1 OR causation_event_id=?2",
                    params![append_identity, command.causation_event_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some()
            {
                bail!("runtime recovery receipt identity already exists");
            }
            let canonical_payload = build_runtime_payload(
                &command,
                &append_identity,
                global_sequence,
                global_parent_id.as_deref(),
                global_head.as_deref(),
                &event,
                &actor,
                &runtime,
            )?;
            redactor.reject_sensitive_bytes(&canonical_payload)?;
            let receipt_digest = digest_hex(DIGEST_DOMAIN, &canonical_payload);
            let current_key = integrity
                .load_key(&command.key)
                .context("runtime recovery receipt signing key is unavailable")?;
            let keyed_integrity_tag = receipt_tag(
                &current_key,
                TAG_DOMAIN,
                integrity.root_identity(),
                command.state_root_generation,
                &command.key,
                &receipt_digest,
            )?;
            let anchor_generation = validate_runtime_integrity_progression(integrity, &command)?;
            let prepared = integrity.prepare_anchor_in_transaction(
                &transaction,
                &AnchorCommitInput {
                    state_root_generation: command.state_root_generation,
                    anchor_generation,
                    receipt_count: global_sequence,
                    receipt_head_digest: Some(receipt_digest.clone()),
                    key: command.key.clone(),
                    transaction_id: command.transaction_id.clone(),
                    external_receipt_digest: receipt_digest.clone(),
                    occurred_at: command.occurred_at,
                },
            )?;
            transaction.execute(
                r#"INSERT INTO execass_receipts(
                  receipt_id,delegation_id,receipt_sequence,global_sequence,
                  append_identity,receipt_kind,causation_id,causation_event_id,parent_receipt_id,
                  global_parent_receipt_id,subject_kind,subject_id,subject_revision,actor_type,
                  actor_identity,actor_authority_provenance_id,runtime_host_generation,
                  runtime_host_instance_id,runtime_fencing_token,state_revision,canonical_payload,
                  serialization_version,hash_algorithm,key_id,key_generation,previous_receipt_digest,
                  global_previous_receipt_digest,receipt_digest,keyed_integrity_tag,
                  previous_key_integrity_tag,redacted_summary,occurred_at,committed_at
                ) VALUES(
                  ?1,NULL,NULL,?2,?3,'runtime_recovery',
                  ?4,?5,NULL,?6,'runtime_host_generation','execass-runtime-host',?7,'runtime',
                  ?8,?9,?10,?11,?12,?7,?13,?14,?15,?16,?17,NULL,?18,?19,?20,NULL,?21,?22,?22
                )"#,
                params![
                    command.receipt_id,
                    global_sequence,
                    append_identity,
                    command.causation_id,
                    command.causation_event_id,
                    global_parent_id,
                    command.subject_generation,
                    command.actor.actor_identity.as_str(),
                    command.actor.authority_provenance_id,
                    command.runtime.host_generation,
                    command.runtime.host_instance_id,
                    command.runtime.fencing_token,
                    canonical_payload,
                    SERIALIZATION_VERSION,
                    HASH_ALGORITHM,
                    command.key.key_id,
                    command.key.key_generation,
                    global_head,
                    receipt_digest,
                    keyed_integrity_tag,
                    command.redacted_summary.as_str(),
                    command.occurred_at,
                ],
            )?;
            let advanced = transaction.execute(
                "UPDATE execass_receipt_journal_state SET receipt_count=?1,receipt_head_digest=?2,latest_receipt_id=?3 WHERE singleton=1 AND receipt_count=?4 AND receipt_head_digest IS ?5",
                params![global_sequence, receipt_digest, command.receipt_id, command.expected_global_count, command.expected_global_head_digest],
            )?;
            if advanced != 1 {
                bail!("global receipt journal changed during runtime recovery append");
            }
            integrity.confirm_prepared_anchor_in_transaction(
                &transaction,
                &prepared.transaction_id,
                global_sequence,
                Some(&receipt_digest),
                command.occurred_at,
            )?;
            transaction.commit()?;
            Ok((value, Some(prepared.transaction_id)))
        })();

        let (value, prepared_transaction_id) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                let _ = integrity.recover_integrity();
                return Err(error);
            }
        };
        if let Some(transaction_id) = prepared_transaction_id {
            integrity.finalize_anchor(&transaction_id)?;
        }
        Ok(value)
    }
}

fn require_receipt_integrity_available(integrity: &ReceiptIntegrityStore) -> Result<()> {
    match integrity.status()? {
        IntegrityStatus::Uninitialized | IntegrityStatus::Trusted { .. } => Ok(()),
        IntegrityStatus::Prepared { .. } => {
            bail!("receipt integrity has an interrupted prepared commit; recovery is required")
        }
        IntegrityStatus::KeyLost { .. } => bail!("receipt integrity key is unavailable"),
        IntegrityStatus::Mismatch { .. } | IntegrityStatus::Quarantined { .. } => {
            bail!("receipt integrity is quarantined")
        }
    }
}

fn validate_runtime_receipt_command(
    command: &RuntimeReceiptCommand,
    global_count: i64,
    global_head: Option<&str>,
) -> Result<()> {
    for (name, value) in [
        ("receipt_id", command.receipt_id.as_str()),
        ("transaction_id", command.transaction_id.as_str()),
        ("causation_id", command.causation_id.as_str()),
        ("causation_event_id", command.causation_event_id.as_str()),
        (
            "authority_provenance_id",
            command.actor.authority_provenance_id.as_str(),
        ),
        (
            "host_instance_id",
            command.runtime.host_instance_id.as_str(),
        ),
    ] {
        clean_identity(name, value)?;
    }
    if command.state_root_generation <= 0
        || command.subject_generation <= 0
        || command.runtime.host_generation <= 0
        || command.runtime.fencing_token <= 0
        || command.occurred_at <= 0
        || command.expected_global_count != global_count
        || command.expected_global_head_digest.as_deref() != global_head
        || command.actor.actor_type != ActorType::Runtime
    {
        bail!("runtime recovery receipt command is invalid");
    }
    validate_digest_opt(command.expected_global_head_digest.as_deref())?;
    Ok(())
}

fn load_runtime_event(conn: &Connection, command: &RuntimeReceiptCommand) -> Result<EventBinding> {
    let event = conn
        .query_row(
            "SELECT global_sequence,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version FROM execass_outbox_events WHERE event_id=?1",
            params![command.causation_event_id],
            |row| Ok(EventBinding { global_sequence: row.get(0)?, event_name: row.get(1)?, aggregate_id: row.get(2)?, aggregate_revision: row.get(3)?, correlation_id: row.get(4)?, causation_id: row.get(5)?, occurred_at: row.get(6)?, schema_version: row.get(7)? }),
        )
        .context("runtime recovery outbox event does not exist")?;
    if event.event_name != OutboxEventName::RuntimeHostChanged.as_str()
        || event.aggregate_id != "execass-runtime-host"
        || event.aggregate_revision != command.subject_generation
        || event.causation_id != command.causation_id
        || event.occurred_at != command.occurred_at
    {
        bail!("runtime recovery receipt does not bind its exact outbox event");
    }
    Ok(event)
}

fn validate_runtime_receipt_actor(
    conn: &Connection,
    command: &RuntimeReceiptCommand,
) -> Result<ActorFacts> {
    let (facts, actor_type, authority_kind): (ActorFacts, String, String) = conn
        .query_row(
            "SELECT credential_identity,authenticated_ingress,channel_assurance,actor_type,authority_kind FROM execass_authority_provenance WHERE authority_provenance_id=?1",
            params![command.actor.authority_provenance_id],
            |row| Ok((ActorFacts { credential_identity: row.get(0)?, authenticated_ingress: row.get(1)?, channel_assurance: row.get(2)? }, row.get(3)?, row.get(4)?)),
        )
        .context("runtime recovery authority does not exist")?;
    if actor_type != ActorType::Runtime.as_str()
        || authority_kind != AuthorityKind::RuntimeSafetyState.as_str()
        || facts.credential_identity != command.actor.actor_identity.as_str()
    {
        bail!("runtime recovery receipt actor is not the sealed runtime safety authority");
    }
    Ok(facts)
}

fn validate_runtime_receipt_host(
    conn: &Connection,
    command: &RuntimeReceiptCommand,
) -> Result<RuntimeFacts> {
    conn.query_row(
        r#"SELECT g.installation_identity,g.os_user_identity_digest,g.ownership_scope
           FROM execass_runtime_host_generations g JOIN execass_runtime_host_leases l
             ON l.generation=g.generation AND l.host_instance_id=g.host_instance_id
           WHERE g.generation=?1 AND g.host_instance_id=?2 AND l.fencing_token=?3
             AND l.released_at IS NULL AND g.ended_at IS NULL AND l.expires_at>?4"#,
        params![
            command.runtime.host_generation,
            command.runtime.host_instance_id,
            command.runtime.fencing_token,
            command.occurred_at
        ],
        |row| {
            Ok(RuntimeFacts {
                installation_identity: row.get(0)?,
                os_user_identity_digest: row.get(1)?,
                ownership_scope: row.get(2)?,
            })
        },
    )
    .context("runtime recovery receipt is not fenced to the current successor")
}

fn validate_runtime_receipt_subject(
    conn: &Connection,
    command: &RuntimeReceiptCommand,
) -> Result<()> {
    let exact: Option<i64> = conn
        .query_row(
            "SELECT generation FROM execass_runtime_host_generations WHERE generation=?1 AND ended_at=?2 AND end_reason IN ('gateway_forced_exit_takeover','gateway_fault_takeover','gateway_drain_interrupted_takeover')",
            params![command.subject_generation, command.occurred_at],
            |row| row.get(0),
        )
        .optional()?;
    if exact.is_none() || command.subject_generation >= command.runtime.host_generation {
        bail!("runtime recovery receipt subject is not the exact ended predecessor generation");
    }
    Ok(())
}

fn runtime_append_identity(command: &RuntimeReceiptCommand) -> String {
    let mut bytes = Vec::new();
    for value in [
        APPEND_DOMAIN,
        b"runtime_host",
        b"execass-runtime-host",
        command.causation_event_id.as_bytes(),
    ] {
        length_prefix(&mut bytes, value);
    }
    bytes.extend_from_slice(&command.subject_generation.to_be_bytes());
    hex(&Sha256::digest(bytes))
}

#[allow(clippy::too_many_arguments)]
fn build_runtime_payload(
    command: &RuntimeReceiptCommand,
    append_identity: &str,
    global_sequence: i64,
    global_parent_id: Option<&str>,
    global_parent_digest: Option<&str>,
    event: &EventBinding,
    actor: &ActorFacts,
    runtime: &RuntimeFacts,
) -> Result<Vec<u8>> {
    let payload = serde_json::json!({
        "schema": "carsinos.execass.receipt",
        "version": 1,
        "receipt_id": command.receipt_id,
        "transaction_id": command.transaction_id,
        "receipt_kind": "runtime_recovery",
        "append_identity": append_identity,
        "scope_kind": "runtime_host",
        "scope_id": "execass-runtime-host",
        "delegation_id": serde_json::Value::Null,
        "delegation_sequence": serde_json::Value::Null,
        "global_sequence": global_sequence,
        "state_revision": command.subject_generation,
        "event": {
            "event_id": command.causation_event_id,
            "event_name": event.event_name,
            "aggregate_id": event.aggregate_id,
            "aggregate_revision": event.aggregate_revision,
            "global_sequence": event.global_sequence,
            "correlation_id": event.correlation_id,
            "causation_id": event.causation_id,
            "occurred_at_ms": event.occurred_at,
            "schema_version": event.schema_version,
        },
        "causation": { "kind": "runtime_recovery", "id": command.causation_id },
        "subject": { "kind": "runtime_host_generation", "id": "execass-runtime-host", "revision": command.subject_generation },
        "actor": {
            "actor_type": "runtime",
            "actor_identity": command.actor.actor_identity.as_str(),
            "credential_identity": actor.credential_identity,
            "authenticated_ingress": actor.authenticated_ingress,
            "channel_assurance": actor.channel_assurance,
            "authority_provenance_id": command.actor.authority_provenance_id,
        },
        "runtime": {
            "installation_identity": runtime.installation_identity,
            "os_user_identity_digest": runtime.os_user_identity_digest,
            "host_generation": command.runtime.host_generation,
            "host_instance_id": command.runtime.host_instance_id,
            "fencing_token": command.runtime.fencing_token,
            "ownership_scope": runtime.ownership_scope,
        },
        "occurred_at_ms": command.occurred_at,
        "committed_at_ms": command.occurred_at,
        "evidence_refs": [],
        "redacted_summary": command.redacted_summary.as_str(),
        "delegation_parent_digest": serde_json::Value::Null,
        "global_parent_receipt_id": global_parent_id,
        "global_parent_digest": global_parent_digest,
        "rotation": serde_json::Value::Null,
        "key": { "key_id": command.key.key_id, "key_generation": command.key.key_generation, "tag_algorithm": TAG_ALGORITHM },
        "state_root_generation": command.state_root_generation,
    });
    let serialized = serde_json::to_string(&payload)?;
    Ok(parse_strict_json(&serialized)?.to_bytes())
}

fn validate_runtime_integrity_progression(
    integrity: &ReceiptIntegrityStore,
    command: &RuntimeReceiptCommand,
) -> Result<i64> {
    match integrity.status()? {
        IntegrityStatus::Uninitialized
            if command.expected_global_count == 0
                && command.expected_global_head_digest.is_none() =>
        {
            Ok(1)
        }
        IntegrityStatus::Trusted {
            anchor_generation,
            receipt_count,
            receipt_head_digest,
            key,
        } if receipt_count == command.expected_global_count
            && receipt_head_digest == command.expected_global_head_digest
            && key == command.key =>
        {
            anchor_generation
                .checked_add(1)
                .context("receipt anchor generation overflow")
        }
        _ => bail!("receipt anchor and runtime recovery journal do not match"),
    }
}

fn validate_command(command: &AppendReceiptCommand) -> Result<()> {
    for (name, value) in [
        ("receipt_id", command.receipt_id.as_str()),
        ("transaction_id", command.transaction_id.as_str()),
        ("delegation_id", command.delegation_id.as_str()),
        ("causation_id", command.causation_id.as_str()),
        ("causation_event_id", command.causation_event_id.as_str()),
        ("subject_id", command.subject.subject_id.as_str()),
        (
            "authority_provenance_id",
            command.actor.authority_provenance_id.as_str(),
        ),
        (
            "host_instance_id",
            command.runtime.host_instance_id.as_str(),
        ),
    ] {
        clean_identity(name, value)?;
    }
    if command.state_root_generation <= 0
        || command.expected_state_revision <= 0
        || command.subject.revision < 0
        || command.runtime.host_generation <= 0
        || command.runtime.fencing_token <= 0
        || command.occurred_at < 0
        || command.committed_at < command.occurred_at
        || command.evidence.len() > 64
    {
        bail!("canonical receipt command is invalid");
    }
    if command.expected_global_count < 0
        || command.expected_delegation_count < 0
        || (command.expected_global_count == 0) != command.expected_global_head_digest.is_none()
        || (command.expected_delegation_count == 0)
            != command.expected_delegation_head_digest.is_none()
    {
        bail!("canonical receipt expected head is invalid");
    }
    validate_digest_opt(command.expected_global_head_digest.as_deref())?;
    validate_digest_opt(command.expected_delegation_head_digest.as_deref())?;
    if command.rotation.is_some() != (command.receipt_kind == ReceiptKind::KeyRotation) {
        bail!("receipt rotation binding is inconsistent");
    }
    Ok(())
}

fn clean_identity(name: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("{name} is invalid");
    }
    let safe = SafeText::new(value, &[])?;
    if safe.as_str() != value {
        bail!("{name} contains sensitive content");
    }
    Ok(())
}

fn snapshot_matches_at_revision(
    command: &AppendReceiptCommand,
    snapshot: &Snapshot,
    expected_state_revision: i64,
) -> bool {
    snapshot.state_revision == expected_state_revision
        && snapshot.global_count == command.expected_global_count
        && snapshot.global_head == command.expected_global_head_digest
        && snapshot.delegation_count == command.expected_delegation_count
        && snapshot.delegation_head == command.expected_delegation_head_digest
}

fn snapshot_matches(command: &AppendReceiptCommand, snapshot: &Snapshot) -> bool {
    snapshot_matches_at_revision(command, snapshot, command.expected_state_revision)
}

fn replay_expectations_match(command: &AppendReceiptCommand, position: &ChainPosition) -> bool {
    command.expected_global_count == position.global_sequence - 1
        && command.expected_global_head_digest == position.global_parent_digest
        && command.expected_delegation_count == position.delegation_sequence - 1
        && command.expected_delegation_head_digest == position.delegation_parent_digest
}

fn stale(snapshot: Snapshot) -> AppendReceiptOutcome {
    AppendReceiptOutcome::Stale {
        current_state_revision: snapshot.state_revision,
        global_count: snapshot.global_count,
        global_head_digest: snapshot.global_head,
        delegation_count: snapshot.delegation_count,
        delegation_head_digest: snapshot.delegation_head,
    }
}

fn load_snapshot(conn: &Connection, delegation_id: &str) -> Result<Option<Snapshot>> {
    conn.query_row(
        r#"SELECT d.state_revision,j.receipt_count,j.receipt_head_digest,j.latest_receipt_id,
          d.receipt_chain_count,d.receipt_chain_head_digest,
          (SELECT receipt_id FROM execass_receipts r WHERE r.delegation_id=d.delegation_id ORDER BY receipt_sequence DESC LIMIT 1)
          FROM execass_delegations d CROSS JOIN execass_receipt_journal_state j
          WHERE d.delegation_id=?1 AND j.singleton=1"#,
        params![delegation_id],
        |row| Ok(Snapshot {
            state_revision: row.get(0)?, global_count: row.get(1)?, global_head: row.get(2)?,
            global_parent_id: row.get(3)?, delegation_count: row.get(4)?, delegation_head: row.get(5)?,
            delegation_parent_id: row.get(6)?,
        }),
    ).optional().context("failed reading receipt heads")
}

fn next_position(snapshot: &Snapshot) -> Result<ChainPosition> {
    Ok(ChainPosition {
        global_sequence: snapshot
            .global_count
            .checked_add(1)
            .context("global receipt sequence overflow")?,
        delegation_sequence: snapshot
            .delegation_count
            .checked_add(1)
            .context("delegation receipt sequence overflow")?,
        global_parent_id: snapshot.global_parent_id.clone(),
        delegation_parent_id: snapshot.delegation_parent_id.clone(),
        global_parent_digest: snapshot.global_head.clone(),
        delegation_parent_digest: snapshot.delegation_head.clone(),
    })
}

fn load_event(conn: &Connection, command: &AppendReceiptCommand) -> Result<EventBinding> {
    conn.query_row(
        "SELECT global_sequence,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version FROM execass_outbox_events WHERE event_id=?1",
        params![command.causation_event_id],
        |row| Ok(EventBinding { global_sequence: row.get(0)?, event_name: row.get(1)?, aggregate_id: row.get(2)?, aggregate_revision: row.get(3)?, correlation_id: row.get(4)?, causation_id: row.get(5)?, occurred_at: row.get(6)?, schema_version: row.get(7)? }),
    ).context("receipt outbox event does not exist")
}

fn validate_event_in_transaction(
    conn: &Connection,
    command: &AppendReceiptCommand,
    expected: &EventBinding,
) -> Result<()> {
    let current = load_event(conn, command)?;
    if current.global_sequence != expected.global_sequence
        || current.event_name != expected.event_name
        || current.aggregate_id != expected.aggregate_id
        || current.aggregate_revision != expected.aggregate_revision
        || current.correlation_id != expected.correlation_id
        || current.causation_id != expected.causation_id
        || current.occurred_at != expected.occurred_at
        || current.schema_version != expected.schema_version
    {
        bail!("receipt outbox event changed before append");
    }
    Ok(())
}

fn validate_actor(conn: &Connection, command: &AppendReceiptCommand) -> Result<ActorFacts> {
    let facts = conn.query_row(
        "SELECT credential_identity,authenticated_ingress,channel_assurance,actor_type FROM execass_authority_provenance WHERE authority_provenance_id=?1",
        params![command.actor.authority_provenance_id],
        |row| Ok((ActorFacts { credential_identity: row.get(0)?, authenticated_ingress: row.get(1)?, channel_assurance: row.get(2)? }, row.get::<_, String>(3)?)),
    ).context("receipt actor authority provenance does not exist")?;
    if facts.1 != command.actor.actor_type.as_str()
        || facts.0.credential_identity != command.actor.actor_identity.as_str()
    {
        bail!("receipt actor authority provenance mismatch");
    }
    Ok(facts.0)
}

fn validate_runtime(conn: &Connection, command: &AppendReceiptCommand) -> Result<RuntimeFacts> {
    conn.query_row(
        r#"SELECT g.installation_identity,g.os_user_identity_digest,g.ownership_scope
          FROM execass_runtime_host_generations g JOIN execass_runtime_host_leases l
          ON l.generation=g.generation AND l.host_instance_id=g.host_instance_id
          WHERE g.generation=?1 AND g.host_instance_id=?2 AND l.fencing_token=?3
            AND l.ownership_scope=g.ownership_scope AND l.ownership_scope='execass'
            AND l.released_at IS NULL AND l.expires_at>?4"#,
        params![
            command.runtime.host_generation,
            command.runtime.host_instance_id,
            command.runtime.fencing_token,
            command.committed_at,
        ],
        |row| {
            Ok(RuntimeFacts {
                installation_identity: row.get(0)?,
                os_user_identity_digest: row.get(1)?,
                ownership_scope: row.get(2)?,
            })
        },
    )
    .context("receipt runtime host generation/fence is not the current live ExecAss lease")
}

fn validate_subject(conn: &Connection, command: &AppendReceiptCommand) -> Result<()> {
    let (table, id_column, revision_column) = match command.subject.kind {
        ReceiptSubjectKind::Delegation => {
            ("execass_delegations", "delegation_id", "state_revision")
        }
        ReceiptSubjectKind::Plan => ("execass_plans", "plan_id", "plan_revision"),
        ReceiptSubjectKind::PlanAmendment => (
            "execass_plan_amendments",
            "amendment_id",
            "amendment_revision",
        ),
        ReceiptSubjectKind::Decision => ("execass_decisions", "decision_id", "decision_revision"),
        ReceiptSubjectKind::Continuation => (
            "execass_continuations",
            "continuation_id",
            "target_delegation_revision",
        ),
        ReceiptSubjectKind::ActionBranch => {
            ("execass_action_branches", "action_id", "action_revision")
        }
        ReceiptSubjectKind::VerifierResult => (
            "execass_verifier_results",
            "verifier_result_id",
            "result_revision",
        ),
        ReceiptSubjectKind::CompletionAssessment => (
            "execass_completion_assessments",
            "assessment_id",
            "assessment_revision",
        ),
        ReceiptSubjectKind::TerminalCorrection => (
            "execass_terminal_corrections",
            "correction_id",
            "correction_revision",
        ),
        ReceiptSubjectKind::AuthorityLink => {
            ("execass_authority_links", "link_id", "link_revision")
        }
        ReceiptSubjectKind::RecoveryEvaluation => (
            "execass_recovery_evaluations",
            "recovery_evaluation_id",
            "evaluation_revision",
        ),
        ReceiptSubjectKind::OutboxEvent => {
            ("execass_outbox_events", "event_id", "aggregate_revision")
        }
        ReceiptSubjectKind::GlobalRuntimeControl => {
            if command.subject.subject_id != "global-stop-all" {
                bail!("global runtime receipt subject must use the fixed aggregate identity");
            }
            let current: Option<i64> = conn
                .query_row(
                    "SELECT global_stop_epoch FROM execass_global_runtime_control WHERE singleton=1 AND global_stop_epoch=?1",
                    params![command.subject.revision],
                    |row| row.get(0),
                )
                .optional()?;
            if current.is_none() {
                bail!("global runtime receipt subject does not match the current stop epoch");
            }
            return Ok(());
        }
        ReceiptSubjectKind::PolicyRevision => {
            if command.delegation_id != "execass-global-control-carrier"
                || command.subject.subject_id != "execass-policy"
            {
                bail!("policy receipt must use the canonical global carrier and aggregate");
            }
            let current: Option<i64> = conn
                .query_row(
                    "SELECT policy_revision FROM execass_policy_revisions WHERE policy_revision=?1 AND receipt_id=?2",
                    params![command.subject.revision, command.receipt_id],
                    |row| row.get(0),
                )
                .optional()?;
            if current.is_none() {
                bail!("policy receipt subject does not match its immutable revision");
            }
            return Ok(());
        }
        ReceiptSubjectKind::RuntimeSettingsRevision => {
            if command.delegation_id != "execass-global-control-carrier"
                || command.subject.subject_id != "execass-runtime-host"
            {
                bail!(
                    "runtime settings receipt must use the canonical global carrier and aggregate"
                );
            }
            let current: Option<i64> = conn
                .query_row(
                    "SELECT settings_revision FROM execass_runtime_settings_revisions WHERE settings_revision=?1 AND receipt_id=?2",
                    params![command.subject.revision, command.receipt_id],
                    |row| row.get(0),
                )
                .optional()?;
            if current.is_none() {
                bail!("runtime settings receipt subject does not match its immutable revision");
            }
            return Ok(());
        }
        ReceiptSubjectKind::RuntimeHostGeneration => {
            if command.subject.subject_id != "execass-runtime-host" {
                bail!("runtime recovery receipt must use the canonical runtime-host aggregate");
            }
            let current: Option<i64> = conn
                .query_row(
                    "SELECT generation FROM execass_runtime_host_generations WHERE generation=?1",
                    params![command.subject.revision],
                    |row| row.get(0),
                )
                .optional()?;
            if current.is_none() {
                bail!("runtime recovery receipt subject does not match a host generation");
            }
            return Ok(());
        }
    };
    let sql = format!(
        "SELECT 1 FROM {table} WHERE {id_column}=?1 AND {revision_column}=?2 AND {}=?3",
        if command.subject.kind == ReceiptSubjectKind::OutboxEvent {
            "aggregate_id"
        } else {
            "delegation_id"
        }
    );
    if conn
        .query_row(
            &sql,
            params![
                command.subject.subject_id,
                command.subject.revision,
                command.delegation_id
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_none()
    {
        bail!("receipt subject does not match its exact delegation revision");
    }
    Ok(())
}

fn normalize_and_validate_evidence(
    conn: &Connection,
    command: &AppendReceiptCommand,
) -> Result<Vec<NormalizedEvidence>> {
    let mut evidence = Vec::new();
    for item in &command.evidence {
        clean_identity("authority_link_id", &item.authority_link_id)?;
        clean_identity("evidence source_id", &item.source_id)?;
        if item.authoritative_revision != 0 {
            bail!("legacy authority source revision must be zero");
        }
        let link = conn
            .query_row(
                r#"SELECT link_revision,delegation_state_revision,linked_at,outbox_event_id
              FROM execass_authority_links WHERE link_id=?1 AND delegation_id=?2
              AND authority_kind=?3 AND authoritative_revision=?4
              AND COALESCE(session_id,run_id,job_id,job_run_id,task_id,board_id,board_card_id,
                mail_thread_id,mail_message_id,attachment_id,board_card_asset_id,mail_attachment_id,
                security_audit_event_id,assistant_tool_call_audit_event_id,tool_call_id)=?5"#,
                params![
                    item.authority_link_id,
                    command.delegation_id,
                    item.kind.as_str(),
                    item.authoritative_revision,
                    item.source_id
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
            .context("receipt evidence does not match an exact authority link")?;
        require_authoritative_source(conn, item.kind, &item.source_id)?;
        let observation = CanonicalValue::object(vec![
            (
                "schema".into(),
                CanonicalValue::string("carsinos.execass.evidence-observation"),
            ),
            ("version".into(), CanonicalValue::Integer(1)),
            (
                "authority_link_id".into(),
                CanonicalValue::string(&item.authority_link_id),
            ),
            (
                "delegation_id".into(),
                CanonicalValue::string(&command.delegation_id),
            ),
            ("link_revision".into(), CanonicalValue::Integer(link.0)),
            (
                "delegation_state_revision".into(),
                CanonicalValue::Integer(link.1),
            ),
            (
                "authority_kind".into(),
                CanonicalValue::string(item.kind.as_str()),
            ),
            ("source_id".into(), CanonicalValue::string(&item.source_id)),
            (
                "authoritative_revision".into(),
                CanonicalValue::Integer(item.authoritative_revision),
            ),
            ("linked_at_ms".into(), CanonicalValue::Integer(link.2)),
            ("outbox_event_id".into(), CanonicalValue::string(&link.3)),
        ])?
        .to_bytes();
        let observation_digest = digest_hex(EVIDENCE_OBSERVATION_DOMAIN, &observation);
        let deep_link = deep_link(item.kind, &item.source_id, item.authoritative_revision)?;
        evidence.push(NormalizedEvidence {
            authority_link_id: item.authority_link_id.clone(),
            kind: item.kind,
            source_id: item.source_id.clone(),
            authoritative_revision: item.authoritative_revision,
            observation_digest,
            deep_link,
        });
    }
    evidence.sort_by(|left, right| {
        (
            left.kind.as_str(),
            left.source_id.as_str(),
            left.authoritative_revision,
            left.authority_link_id.as_str(),
        )
            .cmp(&(
                right.kind.as_str(),
                right.source_id.as_str(),
                right.authoritative_revision,
                right.authority_link_id.as_str(),
            ))
    });
    let mut normalized = Vec::<NormalizedEvidence>::new();
    for item in evidence {
        if let Some(previous) = normalized.last() {
            if previous.kind == item.kind
                && previous.source_id == item.source_id
                && previous.authoritative_revision == item.authoritative_revision
            {
                if previous.authority_link_id == item.authority_link_id
                    && previous.observation_digest == item.observation_digest
                {
                    continue;
                }
                bail!("duplicate receipt evidence identity conflicts");
            }
        }
        normalized.push(item);
    }
    Ok(normalized)
}

fn require_authoritative_source(
    conn: &Connection,
    kind: AuthorityLinkKind,
    source_id: &str,
) -> Result<()> {
    let (table, column) = match kind {
        AuthorityLinkKind::Session => ("sessions", "session_id"),
        AuthorityLinkKind::Run => ("runs", "run_id"),
        AuthorityLinkKind::Job => ("jobs", "job_id"),
        AuthorityLinkKind::JobRun => ("job_runs", "job_run_id"),
        AuthorityLinkKind::Task => ("tasks", "task_id"),
        AuthorityLinkKind::Board => ("boards", "board_id"),
        AuthorityLinkKind::BoardCard => ("board_cards", "card_id"),
        AuthorityLinkKind::MailThread => ("agent_mail_threads", "thread_id"),
        AuthorityLinkKind::MailMessage => ("agent_mail_messages", "message_id"),
        AuthorityLinkKind::ArtifactAttachment => ("attachments", "attachment_id"),
        AuthorityLinkKind::ArtifactBoardCardAsset => ("board_card_assets", "card_asset_id"),
        AuthorityLinkKind::ArtifactMailAttachment => ("agent_mail_attachments", "attachment_id"),
        AuthorityLinkKind::AssistantToolCallAudit => ("assistant_tool_calls_audit", "event_id"),
        AuthorityLinkKind::ToolCall => ("tool_calls", "tool_call_id"),
        AuthorityLinkKind::SecurityAuditEvent => {
            if source_exists(conn, "security_audit_events", "event_id", source_id)?
                || source_exists(conn, "security_audit_events_archive", "event_id", source_id)?
            {
                return Ok(());
            }
            bail!("receipt evidence authoritative source does not exist");
        }
    };
    if source_exists(conn, table, column, source_id)? {
        Ok(())
    } else {
        bail!("receipt evidence authoritative source does not exist")
    }
}

fn source_exists(conn: &Connection, table: &str, column: &str, source_id: &str) -> Result<bool> {
    let sql = format!("SELECT 1 FROM {table} WHERE {column}=?1");
    Ok(conn
        .query_row(&sql, params![source_id], |_| Ok(()))
        .optional()?
        .is_some())
}

fn deep_link(kind: AuthorityLinkKind, source_id: &str, revision: i64) -> Result<String> {
    let mut encoded = String::new();
    for byte in source_id.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    let link = format!(
        "carsinos://evidence/v1/{}/{encoded}?revision={revision}",
        kind.as_str()
    );
    if SafeText::new(&link, &[])?.as_str() != link {
        bail!("evidence deep link contains sensitive content");
    }
    Ok(link)
}

fn build_payload(
    command: &AppendReceiptCommand,
    append_identity: &str,
    position: &ChainPosition,
    event: &EventBinding,
    actor: &ActorFacts,
    runtime: &RuntimeFacts,
    evidence: &[NormalizedEvidence],
) -> Result<Vec<u8>> {
    let event_aggregate_matches = match command.subject.kind {
        ReceiptSubjectKind::GlobalRuntimeControl => {
            event.aggregate_id == "global-stop-all"
                && event.aggregate_revision == command.subject.revision
        }
        ReceiptSubjectKind::PolicyRevision => {
            event.aggregate_id == "execass-policy"
                && event.aggregate_revision == command.subject.revision
        }
        ReceiptSubjectKind::RuntimeSettingsRevision => {
            event.aggregate_id == "execass-runtime-host"
                && event.aggregate_revision == command.subject.revision
        }
        ReceiptSubjectKind::RuntimeHostGeneration => {
            event.aggregate_id == "execass-runtime-host"
                && event.aggregate_revision == command.subject.revision
        }
        _ => {
            event.aggregate_id == command.delegation_id
                && event.aggregate_revision == command.expected_state_revision
        }
    };
    if !event_aggregate_matches
        || event.causation_id != command.causation_id
        || event.occurred_at != command.occurred_at
    {
        bail!("receipt event/delegation/causation binding mismatch");
    }
    let evidence_value = CanonicalValue::Array(
        evidence
            .iter()
            .map(|item| {
                CanonicalValue::object(vec![
                    (
                        "authority_kind".into(),
                        CanonicalValue::string(item.kind.as_str()),
                    ),
                    ("source_id".into(), CanonicalValue::string(&item.source_id)),
                    (
                        "authoritative_revision".into(),
                        CanonicalValue::Integer(item.authoritative_revision),
                    ),
                    (
                        "authority_link_id".into(),
                        CanonicalValue::string(&item.authority_link_id),
                    ),
                    (
                        "observation_digest".into(),
                        CanonicalValue::string(&item.observation_digest),
                    ),
                    ("deep_link".into(), CanonicalValue::string(&item.deep_link)),
                ])
            })
            .collect::<Result<Vec<_>>>()?,
    );
    let rotation = match &command.rotation {
        Some(rotation) => CanonicalValue::object(vec![
            (
                "transition_id".into(),
                CanonicalValue::string(&rotation.transition_id),
            ),
            (
                "reason".into(),
                CanonicalValue::string(rotation.reason.as_str()),
            ),
            (
                "previous_key_id".into(),
                CanonicalValue::string(&rotation.previous_key.key_id),
            ),
            (
                "previous_key_generation".into(),
                CanonicalValue::Integer(rotation.previous_key.key_generation),
            ),
            (
                "new_key_id".into(),
                CanonicalValue::string(&command.key.key_id),
            ),
            (
                "new_key_generation".into(),
                CanonicalValue::Integer(command.key.key_generation),
            ),
        ])?,
        None => CanonicalValue::Null,
    };
    CanonicalValue::object(vec![
        (
            "schema".into(),
            CanonicalValue::string("carsinos.execass.receipt"),
        ),
        ("version".into(), CanonicalValue::Integer(1)),
        (
            "receipt_id".into(),
            CanonicalValue::string(&command.receipt_id),
        ),
        (
            "transaction_id".into(),
            CanonicalValue::string(&command.transaction_id),
        ),
        (
            "receipt_kind".into(),
            CanonicalValue::string(command.receipt_kind.as_str()),
        ),
        (
            "append_identity".into(),
            CanonicalValue::string(append_identity),
        ),
        (
            "delegation_id".into(),
            CanonicalValue::string(&command.delegation_id),
        ),
        (
            "delegation_sequence".into(),
            CanonicalValue::Integer(position.delegation_sequence),
        ),
        (
            "global_sequence".into(),
            CanonicalValue::Integer(position.global_sequence),
        ),
        (
            "state_revision".into(),
            CanonicalValue::Integer(command.expected_state_revision),
        ),
        (
            "event".into(),
            CanonicalValue::object(vec![
                (
                    "event_id".into(),
                    CanonicalValue::string(&command.causation_event_id),
                ),
                (
                    "event_name".into(),
                    CanonicalValue::string(&event.event_name),
                ),
                (
                    "aggregate_id".into(),
                    CanonicalValue::string(&event.aggregate_id),
                ),
                (
                    "aggregate_revision".into(),
                    CanonicalValue::Integer(event.aggregate_revision),
                ),
                (
                    "global_sequence".into(),
                    CanonicalValue::Integer(event.global_sequence),
                ),
                (
                    "correlation_id".into(),
                    CanonicalValue::string(&event.correlation_id),
                ),
                (
                    "causation_id".into(),
                    CanonicalValue::string(&event.causation_id),
                ),
                (
                    "occurred_at_ms".into(),
                    CanonicalValue::Integer(event.occurred_at),
                ),
                (
                    "schema_version".into(),
                    CanonicalValue::string(&event.schema_version),
                ),
            ])?,
        ),
        (
            "causation".into(),
            CanonicalValue::object(vec![
                (
                    "kind".into(),
                    CanonicalValue::string(command.receipt_kind.as_str()),
                ),
                ("id".into(), CanonicalValue::string(&command.causation_id)),
            ])?,
        ),
        (
            "subject".into(),
            CanonicalValue::object(vec![
                (
                    "kind".into(),
                    CanonicalValue::string(command.subject.kind.as_str()),
                ),
                (
                    "id".into(),
                    CanonicalValue::string(&command.subject.subject_id),
                ),
                (
                    "revision".into(),
                    CanonicalValue::Integer(command.subject.revision),
                ),
            ])?,
        ),
        (
            "actor".into(),
            CanonicalValue::object(vec![
                (
                    "actor_type".into(),
                    CanonicalValue::string(command.actor.actor_type.as_str()),
                ),
                (
                    "actor_identity".into(),
                    CanonicalValue::string(command.actor.actor_identity.as_str()),
                ),
                (
                    "credential_identity".into(),
                    CanonicalValue::string(&actor.credential_identity),
                ),
                (
                    "authenticated_ingress".into(),
                    CanonicalValue::string(&actor.authenticated_ingress),
                ),
                (
                    "channel_assurance".into(),
                    CanonicalValue::string(&actor.channel_assurance),
                ),
                (
                    "authority_provenance_id".into(),
                    CanonicalValue::string(&command.actor.authority_provenance_id),
                ),
            ])?,
        ),
        (
            "runtime".into(),
            CanonicalValue::object(vec![
                (
                    "installation_identity".into(),
                    CanonicalValue::string(&runtime.installation_identity),
                ),
                (
                    "os_user_identity_digest".into(),
                    CanonicalValue::string(&runtime.os_user_identity_digest),
                ),
                (
                    "host_generation".into(),
                    CanonicalValue::Integer(command.runtime.host_generation),
                ),
                (
                    "host_instance_id".into(),
                    CanonicalValue::string(&command.runtime.host_instance_id),
                ),
                (
                    "fencing_token".into(),
                    CanonicalValue::Integer(command.runtime.fencing_token),
                ),
                (
                    "ownership_scope".into(),
                    CanonicalValue::string(&runtime.ownership_scope),
                ),
            ])?,
        ),
        (
            "occurred_at_ms".into(),
            CanonicalValue::Integer(command.occurred_at),
        ),
        (
            "committed_at_ms".into(),
            CanonicalValue::Integer(command.committed_at),
        ),
        ("evidence_refs".into(), evidence_value),
        (
            "redacted_summary".into(),
            CanonicalValue::string(command.redacted_summary.as_str()),
        ),
        (
            "delegation_parent_digest".into(),
            option_string(position.delegation_parent_digest.as_deref()),
        ),
        (
            "global_parent_digest".into(),
            option_string(position.global_parent_digest.as_deref()),
        ),
        (
            "key".into(),
            CanonicalValue::object(vec![
                ("key_id".into(), CanonicalValue::string(&command.key.key_id)),
                (
                    "key_generation".into(),
                    CanonicalValue::Integer(command.key.key_generation),
                ),
                (
                    "tag_algorithm".into(),
                    CanonicalValue::string(TAG_ALGORITHM),
                ),
            ])?,
        ),
        ("rotation".into(), rotation),
        (
            "state_root_generation".into(),
            CanonicalValue::Integer(command.state_root_generation),
        ),
    ])?
    .to_bytes()
    .pipe(Ok)
}

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}
impl<T> Pipe for T {}

fn option_string(value: Option<&str>) -> CanonicalValue {
    value
        .map(CanonicalValue::string)
        .unwrap_or(CanonicalValue::Null)
}

fn append_identity(command: &AppendReceiptCommand) -> String {
    let mut bytes = Vec::new();
    for value in [
        APPEND_DOMAIN,
        command.delegation_id.as_bytes(),
        command.causation_event_id.as_bytes(),
        command.subject.kind.as_str().as_bytes(),
        command.subject.subject_id.as_bytes(),
    ] {
        length_prefix(&mut bytes, value);
    }
    bytes.extend_from_slice(&command.subject.revision.to_be_bytes());
    hex(&Sha256::digest(bytes))
}

fn digest_hex(domain: &[u8], payload: &[u8]) -> String {
    let mut bytes = Vec::new();
    length_prefix(&mut bytes, domain);
    length_prefix(&mut bytes, payload);
    hex(&Sha256::digest(bytes))
}

fn receipt_tag(
    key: &[u8],
    domain: &[u8],
    root_identity: &str,
    state_root_generation: i64,
    key_ref: &ReceiptKeyRef,
    digest: &str,
) -> Result<String> {
    let mut payload = Vec::new();
    length_prefix(&mut payload, domain);
    length_prefix(&mut payload, root_identity.as_bytes());
    payload.extend_from_slice(&state_root_generation.to_be_bytes());
    length_prefix(&mut payload, key_ref.key_id.as_bytes());
    payload.extend_from_slice(&key_ref.key_generation.to_be_bytes());
    length_prefix(&mut payload, &decode_hex(digest)?);
    let mut mac = HmacSha256::new_from_slice(key)?;
    mac.update(&payload);
    Ok(hex(&mac.finalize().into_bytes()))
}

fn rotation_tag(
    conn: &Connection,
    integrity: &ReceiptIntegrityStore,
    command: &AppendReceiptCommand,
    digest: &str,
) -> Result<Option<String>> {
    let Some(rotation) = &command.rotation else {
        return Ok(None);
    };
    clean_identity("rotation transition_id", &rotation.transition_id)?;
    let parent: Option<(String,i64)> = conn.query_row("SELECT rotated_from_key_id,rotated_from_key_generation FROM execass_receipt_keys WHERE key_id=?1 AND key_generation=?2 AND status='provisioned'", params![command.key.key_id,command.key.key_generation], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
    if parent.as_ref()
        != Some(&(
            rotation.previous_key.key_id.clone(),
            rotation.previous_key.key_generation,
        ))
    {
        bail!("receipt rotation does not match the immutable key registry");
    }
    let old = integrity
        .load_key(&rotation.previous_key)
        .context("previous receipt key is unavailable for rotation")?;
    receipt_tag(
        &old,
        ROTATION_DOMAIN,
        integrity.root_identity(),
        command.state_root_generation,
        &rotation.previous_key,
        digest,
    )
    .map(Some)
}

fn validate_integrity_progression(
    integrity: &ReceiptIntegrityStore,
    command: &AppendReceiptCommand,
) -> Result<i64> {
    match integrity.status()? {
        IntegrityStatus::Uninitialized
            if command.expected_global_count == 0
                && command.expected_global_head_digest.is_none() =>
        {
            Ok(1)
        }
        IntegrityStatus::Trusted {
            anchor_generation,
            receipt_count,
            receipt_head_digest,
            key,
        } if receipt_count == command.expected_global_count
            && receipt_head_digest == command.expected_global_head_digest
            && (key == command.key
                || command
                    .rotation
                    .as_ref()
                    .is_some_and(|rotation| rotation.previous_key == key)) =>
        {
            anchor_generation
                .checked_add(1)
                .context("receipt anchor generation overflow")
        }
        IntegrityStatus::Prepared { .. } => {
            bail!("receipt integrity has an interrupted prepared commit; recovery is required")
        }
        IntegrityStatus::KeyLost { .. } => bail!("receipt integrity key is unavailable"),
        IntegrityStatus::Mismatch { .. } | IntegrityStatus::Quarantined { .. } => {
            bail!("receipt integrity is quarantined")
        }
        _ => bail!("receipt anchor and receipt journal do not match"),
    }
}

fn insert_evidence(
    transaction: &Transaction<'_>,
    receipt_id: &str,
    evidence: &[NormalizedEvidence],
) -> Result<()> {
    for (ordinal, item) in evidence.iter().enumerate() {
        transaction.execute("INSERT INTO execass_receipt_evidence_refs(receipt_id,ordinal,authority_kind,source_id,authoritative_revision,authority_link_id,observation_digest,deep_link) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![receipt_id,ordinal as i64,item.kind.as_str(),item.source_id,item.authoritative_revision,item.authority_link_id,item.observation_digest,item.deep_link])?;
    }
    Ok(())
}

#[derive(Debug)]
struct PersistedReceipt {
    receipt_id: String,
    delegation_id: Option<String>,
    receipt_sequence: Option<i64>,
    global_sequence: i64,
    append_identity: Option<String>,
    receipt_kind: Option<String>,
    causation_id: String,
    causation_event_id: String,
    parent_receipt_id: Option<String>,
    global_parent_receipt_id: Option<String>,
    subject_kind: Option<String>,
    subject_id: Option<String>,
    subject_revision: Option<i64>,
    actor_type: String,
    actor_identity: String,
    actor_authority_provenance_id: Option<String>,
    runtime_host_generation: i64,
    runtime_host_instance_id: Option<String>,
    runtime_fencing_token: Option<i64>,
    state_revision: i64,
    canonical_payload: Vec<u8>,
    serialization_version: String,
    hash_algorithm: String,
    key: ReceiptKeyRef,
    previous_receipt_digest: Option<String>,
    global_previous_receipt_digest: Option<String>,
    receipt_digest: String,
    keyed_integrity_tag: String,
    previous_key_integrity_tag: Option<String>,
    redacted_summary: String,
    occurred_at: i64,
    committed_at: i64,
}

#[derive(Debug, Default)]
struct VerifiedDelegationHead {
    count: i64,
    head_digest: Option<String>,
    head_receipt_id: Option<String>,
}

pub(super) fn verify_receipt_history(
    conn: &Connection,
    integrity: &ReceiptIntegrityStore,
    expected_state_root_generation: i64,
    expected_count: i64,
    expected_head_digest: Option<&str>,
) -> std::result::Result<(), ReceiptHistoryFailure> {
    let query_failure = || history_failure("receipt_history_query_failed", None, None, None, None);
    let mut statement = conn
        .prepare(
            r#"
            SELECT receipt_id,delegation_id,receipt_sequence,global_sequence,append_identity,
                   receipt_kind,causation_id,causation_event_id,parent_receipt_id,
                   global_parent_receipt_id,subject_kind,subject_id,subject_revision,actor_type,
                   actor_identity,actor_authority_provenance_id,runtime_host_generation,
                   runtime_host_instance_id,runtime_fencing_token,state_revision,canonical_payload,
                   serialization_version,hash_algorithm,key_id,key_generation,
                   previous_receipt_digest,global_previous_receipt_digest,receipt_digest,
                   keyed_integrity_tag,previous_key_integrity_tag,redacted_summary,occurred_at,
                   committed_at
            FROM execass_receipts ORDER BY global_sequence,receipt_id
            "#,
        )
        .map_err(|_| query_failure())?;
    let mapped = statement
        .query_map([], |row| {
            Ok(PersistedReceipt {
                receipt_id: row.get(0)?,
                delegation_id: row.get(1)?,
                receipt_sequence: row.get(2)?,
                global_sequence: row.get(3)?,
                append_identity: row.get(4)?,
                receipt_kind: row.get(5)?,
                causation_id: row.get(6)?,
                causation_event_id: row.get(7)?,
                parent_receipt_id: row.get(8)?,
                global_parent_receipt_id: row.get(9)?,
                subject_kind: row.get(10)?,
                subject_id: row.get(11)?,
                subject_revision: row.get(12)?,
                actor_type: row.get(13)?,
                actor_identity: row.get(14)?,
                actor_authority_provenance_id: row.get(15)?,
                runtime_host_generation: row.get(16)?,
                runtime_host_instance_id: row.get(17)?,
                runtime_fencing_token: row.get(18)?,
                state_revision: row.get(19)?,
                canonical_payload: row.get(20)?,
                serialization_version: row.get(21)?,
                hash_algorithm: row.get(22)?,
                key: ReceiptKeyRef {
                    key_id: row.get(23)?,
                    key_generation: row.get(24)?,
                },
                previous_receipt_digest: row.get(25)?,
                global_previous_receipt_digest: row.get(26)?,
                receipt_digest: row.get(27)?,
                keyed_integrity_tag: row.get(28)?,
                previous_key_integrity_tag: row.get(29)?,
                redacted_summary: row.get(30)?,
                occurred_at: row.get(31)?,
                committed_at: row.get(32)?,
            })
        })
        .map_err(|_| query_failure())?;
    let rows = mapped
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| query_failure())?;

    let mut global_head: Option<String> = None;
    let mut global_head_id: Option<String> = None;
    let mut delegation_heads: BTreeMap<String, VerifiedDelegationHead> = BTreeMap::new();
    let mut previous_key: Option<ReceiptKeyRef> = None;
    let latest_anchor_key: Option<(String, i64)> = conn
        .query_row(
            "SELECT key_id,key_generation FROM execass_receipt_anchor_state WHERE status='finalized' ORDER BY anchor_generation DESC LIMIT 1",
            [],
            |anchor_row| Ok((anchor_row.get(0)?, anchor_row.get(1)?)),
        )
        .optional()
        .map_err(|_| query_failure())?;

    for (offset, row) in rows.iter().enumerate() {
        let expected_global_sequence = i64::try_from(offset + 1)
            .map_err(|_| history_failure("receipt_count_overflow", None, None, None, None))?;
        let fail = |reason| {
            history_failure(
                reason,
                Some(expected_global_sequence),
                Some(&row.receipt_id),
                row.delegation_id.as_deref(),
                row.receipt_sequence,
            )
        };
        if row.global_sequence != expected_global_sequence {
            return Err(fail("receipt_global_sequence_mismatch"));
        }
        if row.serialization_version != SERIALIZATION_VERSION {
            return Err(fail("receipt_serialization_version_mismatch"));
        }
        if row.hash_algorithm != HASH_ALGORITHM {
            return Err(fail("receipt_hash_algorithm_mismatch"));
        }
        if row.global_previous_receipt_digest != global_head
            || row.global_parent_receipt_id != global_head_id
        {
            return Err(fail("receipt_global_chain_mismatch"));
        }
        if let Some(delegation_id) = row.delegation_id.as_ref() {
            let expected_delegation = delegation_heads.get(delegation_id);
            let expected_delegation_count = expected_delegation.map_or(0, |head| head.count);
            let expected_delegation_digest =
                expected_delegation.and_then(|head| head.head_digest.as_ref());
            let expected_delegation_id =
                expected_delegation.and_then(|head| head.head_receipt_id.as_ref());
            if row.receipt_sequence != Some(expected_delegation_count + 1)
                || row.previous_receipt_digest.as_ref() != expected_delegation_digest
                || row.parent_receipt_id.as_ref() != expected_delegation_id
            {
                return Err(fail("receipt_delegation_chain_mismatch"));
            }
        } else if row.receipt_sequence.is_some()
            || row.previous_receipt_digest.is_some()
            || row.parent_receipt_id.is_some()
            || row.subject_kind.as_deref() != Some("runtime_host_generation")
        {
            return Err(fail("receipt_runtime_scope_mismatch"));
        }
        let payload_text = std::str::from_utf8(&row.canonical_payload)
            .map_err(|_| fail("receipt_canonical_payload_invalid"))?;
        let canonical = parse_strict_json(payload_text)
            .map_err(|_| fail("receipt_canonical_payload_invalid"))?;
        if canonical.to_bytes() != row.canonical_payload {
            return Err(fail("receipt_canonical_payload_noncanonical"));
        }
        let payload: serde_json::Value = serde_json::from_slice(&row.canonical_payload)
            .map_err(|_| fail("receipt_canonical_payload_invalid"))?;
        if digest_hex(DIGEST_DOMAIN, &row.canonical_payload) != row.receipt_digest {
            return Err(fail("receipt_digest_mismatch"));
        }

        verify_payload_binding(row, &payload).map_err(&fail)?;
        let state_root_generation = json_i64(&payload, &["state_root_generation"])
            .ok_or_else(|| fail("receipt_payload_binding_mismatch"))?;
        if state_root_generation != expected_state_root_generation {
            return Err(fail("receipt_state_root_generation_mismatch"));
        }

        let registered: Option<RegisteredReceiptKeyState> = conn
            .query_row(
                "SELECT status,rotated_from_key_id,rotated_from_key_generation,activated_anchor_generation FROM execass_receipt_keys WHERE key_id=?1 AND key_generation=?2",
                params![row.key.key_id, row.key.key_generation],
                |key_row| Ok((key_row.get(0)?, key_row.get(1)?, key_row.get(2)?, key_row.get(3)?)),
            )
            .optional()
            .map_err(|_| fail("receipt_key_registry_query_failed"))?;
        let Some((
            status,
            rotated_from_key_id,
            rotated_from_key_generation,
            activated_anchor_generation,
        )) = registered
        else {
            return Err(fail("receipt_key_registry_mismatch"));
        };
        let first_anchor_generation: Option<i64> = conn
            .query_row(
                "SELECT MIN(anchor_generation) FROM execass_receipt_anchor_state WHERE status='finalized' AND key_id=?1 AND key_generation=?2",
                params![row.key.key_id, row.key.key_generation],
                |anchor_row| anchor_row.get(0),
            )
            .map_err(|_| fail("receipt_key_registry_query_failed"))?;
        let expected_status = if matches!(
            latest_anchor_key.as_ref(),
            Some((key_id, key_generation))
                if key_id == &row.key.key_id && *key_generation == row.key.key_generation
        ) {
            "active"
        } else {
            "retired"
        };
        if status != expected_status
            || first_anchor_generation.is_none()
            || activated_anchor_generation != first_anchor_generation
        {
            return Err(fail("receipt_key_registry_state_mismatch"));
        }
        let current_key = integrity
            .load_key(&row.key)
            .map_err(|_| fail("receipt_key_unavailable"))?;
        let expected_tag = receipt_tag(
            &current_key,
            TAG_DOMAIN,
            integrity.root_identity(),
            state_root_generation,
            &row.key,
            &row.receipt_digest,
        )
        .map_err(|_| fail("receipt_keyed_tag_mismatch"))?;
        if expected_tag != row.keyed_integrity_tag {
            return Err(fail("receipt_keyed_tag_mismatch"));
        }

        let rotation = payload.get("rotation");
        match (previous_key.as_ref(), rotation) {
            (None, Some(serde_json::Value::Null))
                if row.key.key_generation == 1
                    && row.previous_key_integrity_tag.is_none()
                    && rotated_from_key_id.is_none()
                    && rotated_from_key_generation.is_none() => {}
            (Some(previous), Some(serde_json::Value::Null))
                if previous == &row.key && row.previous_key_integrity_tag.is_none() => {}
            (Some(previous), Some(serde_json::Value::Object(rotation)))
                if previous != &row.key
                    && row.key.key_generation == previous.key_generation + 1
                    && rotation
                        .get("previous_key_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(previous.key_id.as_str())
                    && rotation
                        .get("previous_key_generation")
                        .and_then(serde_json::Value::as_i64)
                        == Some(previous.key_generation)
                    && rotation
                        .get("new_key_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(row.key.key_id.as_str())
                    && rotation
                        .get("new_key_generation")
                        .and_then(serde_json::Value::as_i64)
                        == Some(row.key.key_generation)
                    && rotated_from_key_id.as_deref() == Some(previous.key_id.as_str())
                    && rotated_from_key_generation == Some(previous.key_generation) =>
            {
                let previous_material = integrity
                    .load_key(previous)
                    .map_err(|_| fail("receipt_previous_key_unavailable"))?;
                let expected_rotation_tag = receipt_tag(
                    &previous_material,
                    ROTATION_DOMAIN,
                    integrity.root_identity(),
                    state_root_generation,
                    previous,
                    &row.receipt_digest,
                )
                .map_err(|_| fail("receipt_rotation_cross_signature_mismatch"))?;
                if row.previous_key_integrity_tag.as_deref() != Some(expected_rotation_tag.as_str())
                {
                    return Err(fail("receipt_rotation_cross_signature_mismatch"));
                }
            }
            _ => return Err(fail("receipt_key_generation_mismatch")),
        }
        previous_key = Some(row.key.clone());

        verify_evidence_binding(conn, row, &payload).map_err(fail)?;
        if let (Some(delegation_id), Some(receipt_sequence)) =
            (row.delegation_id.as_ref(), row.receipt_sequence)
        {
            let delegation = delegation_heads.entry(delegation_id.clone()).or_default();
            delegation.count = receipt_sequence;
            delegation.head_digest = Some(row.receipt_digest.clone());
            delegation.head_receipt_id = Some(row.receipt_id.clone());
        }
        global_head = Some(row.receipt_digest.clone());
        global_head_id = Some(row.receipt_id.clone());
    }

    match receipt_key_registry_history_is_exact(conn, integrity, latest_anchor_key.as_ref()) {
        Ok(true) => {}
        Ok(false) => {
            return Err(history_failure(
                "receipt_key_registry_state_mismatch",
                None,
                None,
                None,
                None,
            ));
        }
        Err(_) => {
            return Err(history_failure(
                "receipt_key_registry_query_failed",
                None,
                None,
                None,
                None,
            ));
        }
    }

    let actual_count = i64::try_from(rows.len())
        .map_err(|_| history_failure("receipt_count_overflow", None, None, None, None))?;
    if actual_count != expected_count || global_head.as_deref() != expected_head_digest {
        return Err(history_failure(
            "receipt_anchor_count_head_mismatch",
            Some(actual_count + 1),
            None,
            None,
            None,
        ));
    }
    let journal: (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT receipt_count,receipt_head_digest,latest_receipt_id FROM execass_receipt_journal_state WHERE singleton=1",
            [],
            |journal_row| Ok((journal_row.get(0)?, journal_row.get(1)?, journal_row.get(2)?)),
        )
        .map_err(|_| query_failure())?;
    if journal != (actual_count, global_head.clone(), global_head_id.clone()) {
        return Err(history_failure(
            "receipt_global_head_mismatch",
            Some(actual_count.max(1)),
            global_head_id.as_deref(),
            None,
            None,
        ));
    }
    for (delegation_id, verified) in delegation_heads {
        let stored: (i64, Option<String>) = conn
            .query_row(
                "SELECT receipt_chain_count,receipt_chain_head_digest FROM execass_delegations WHERE delegation_id=?1",
                [&delegation_id],
                |delegation_row| Ok((delegation_row.get(0)?, delegation_row.get(1)?)),
            )
            .map_err(|_| {
                history_failure(
                    "receipt_delegation_head_query_failed",
                    None,
                    verified.head_receipt_id.as_deref(),
                    Some(&delegation_id),
                    Some(verified.count),
                )
            })?;
        if stored != (verified.count, verified.head_digest.clone()) {
            return Err(history_failure(
                "receipt_delegation_head_mismatch",
                None,
                verified.head_receipt_id.as_deref(),
                Some(&delegation_id),
                Some(verified.count),
            ));
        }
    }
    Ok(())
}

fn receipt_key_registry_history_is_exact(
    conn: &Connection,
    integrity: &ReceiptIntegrityStore,
    latest_anchor_key: Option<&(String, i64)>,
) -> rusqlite::Result<bool> {
    let Some((latest_key_id, latest_key_generation)) = latest_anchor_key else {
        return Ok(false);
    };
    let mut statement = conn.prepare(
        "SELECT key_id,key_generation,status,rotated_from_key_id,rotated_from_key_generation,created_at,registry_integrity_tag,activated_anchor_generation FROM execass_receipt_keys ORDER BY key_generation",
    )?;
    let registry = statement
        .query_map([], |key_row| {
            Ok(ReceiptKeyRegistryRow {
                key_id: key_row.get(0)?,
                key_generation: key_row.get(1)?,
                status: key_row.get(2)?,
                rotated_from_key_id: key_row.get(3)?,
                rotated_from_key_generation: key_row.get(4)?,
                created_at: key_row.get(5)?,
                registry_integrity_tag: key_row.get(6)?,
                activated_anchor_generation: key_row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if registry.is_empty() {
        return Ok(false);
    }

    for (offset, row) in registry.iter().enumerate() {
        let expected_generation = i64::try_from(offset + 1).unwrap_or(i64::MAX);
        if row.key_generation != expected_generation {
            return Ok(false);
        }
        if offset == 0 {
            if row.rotated_from_key_id.is_some() || row.rotated_from_key_generation.is_some() {
                return Ok(false);
            }
        } else {
            let previous = &registry[offset - 1];
            if row.rotated_from_key_id.as_deref() != Some(previous.key_id.as_str())
                || row.rotated_from_key_generation != Some(previous.key_generation)
            {
                return Ok(false);
            }
        }

        let key = ReceiptKeyRef {
            key_id: row.key_id.clone(),
            key_generation: row.key_generation,
        };
        let Ok(key_material) = integrity.load_key(&key) else {
            return Ok(false);
        };
        let Ok(expected_registry_tag) = receipt_key_registry_integrity_tag(
            &key_material,
            integrity.root_identity(),
            &key,
            row.rotated_from_key_id.as_deref(),
            row.rotated_from_key_generation,
            row.created_at,
        ) else {
            return Ok(false);
        };
        if row.registry_integrity_tag != expected_registry_tag {
            return Ok(false);
        }

        let first_anchor_generation: Option<i64> = conn.query_row(
            "SELECT MIN(anchor_generation) FROM execass_receipt_anchor_state WHERE status='finalized' AND key_id=?1 AND key_generation=?2",
            params![row.key_id, row.key_generation],
            |anchor_row| anchor_row.get(0),
        )?;
        let is_latest =
            row.key_id == *latest_key_id && row.key_generation == *latest_key_generation;
        match (first_anchor_generation, is_latest, row.status.as_str()) {
            (Some(first), true, "active") if row.activated_anchor_generation == Some(first) => {}
            (Some(first), false, "retired") if row.activated_anchor_generation == Some(first) => {}
            (None, false, "provisioned")
                if offset + 1 == registry.len()
                    && latest_key_generation.checked_add(1) == Some(row.key_generation)
                    && row.rotated_from_key_id.as_deref() == Some(latest_key_id.as_str())
                    && row.rotated_from_key_generation == Some(*latest_key_generation)
                    && row.activated_anchor_generation.is_none() => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn verify_payload_binding(
    row: &PersistedReceipt,
    payload: &serde_json::Value,
) -> std::result::Result<(), &'static str> {
    let expected_strings = [
        (&["schema"][..], Some("carsinos.execass.receipt")),
        (&["receipt_id"][..], Some(row.receipt_id.as_str())),
        (&["append_identity"][..], row.append_identity.as_deref()),
        (&["receipt_kind"][..], row.receipt_kind.as_deref()),
        (&["causation", "kind"][..], row.receipt_kind.as_deref()),
        (&["causation", "id"][..], Some(row.causation_id.as_str())),
        (
            &["event", "event_id"][..],
            Some(row.causation_event_id.as_str()),
        ),
        (&["subject", "kind"][..], row.subject_kind.as_deref()),
        (&["subject", "id"][..], row.subject_id.as_deref()),
        (&["actor", "actor_type"][..], Some(row.actor_type.as_str())),
        (
            &["actor", "actor_identity"][..],
            Some(row.actor_identity.as_str()),
        ),
        (
            &["actor", "authority_provenance_id"][..],
            row.actor_authority_provenance_id.as_deref(),
        ),
        (
            &["runtime", "host_instance_id"][..],
            row.runtime_host_instance_id.as_deref(),
        ),
        (
            &["redacted_summary"][..],
            Some(row.redacted_summary.as_str()),
        ),
        (&["key", "key_id"][..], Some(row.key.key_id.as_str())),
        (&["key", "tag_algorithm"][..], Some(TAG_ALGORITHM)),
    ];
    if expected_strings
        .into_iter()
        .any(|(path, expected)| json_str(payload, path) != expected)
    {
        return Err("receipt_payload_binding_mismatch");
    }
    let expected_integers = [
        (&["version"][..], Some(1)),
        (&["global_sequence"][..], Some(row.global_sequence)),
        (&["state_revision"][..], Some(row.state_revision)),
        (&["subject", "revision"][..], row.subject_revision),
        (
            &["runtime", "host_generation"][..],
            Some(row.runtime_host_generation),
        ),
        (&["runtime", "fencing_token"][..], row.runtime_fencing_token),
        (&["occurred_at_ms"][..], Some(row.occurred_at)),
        (&["committed_at_ms"][..], Some(row.committed_at)),
        (&["key", "key_generation"][..], Some(row.key.key_generation)),
    ];
    if expected_integers
        .into_iter()
        .any(|(path, expected)| json_i64(payload, path) != expected)
    {
        return Err("receipt_payload_binding_mismatch");
    }
    match (row.delegation_id.as_deref(), row.receipt_sequence) {
        (Some(delegation_id), Some(receipt_sequence))
            if json_str(payload, &["delegation_id"]) == Some(delegation_id)
                && json_i64(payload, &["delegation_sequence"]) == Some(receipt_sequence)
                && payload.get("scope_kind").is_none()
                && payload.get("scope_id").is_none() => {}
        (None, None)
            if payload
                .get("delegation_id")
                .is_some_and(serde_json::Value::is_null)
                && payload
                    .get("delegation_sequence")
                    .is_some_and(serde_json::Value::is_null)
                && json_str(payload, &["scope_kind"]) == Some("runtime_host")
                && json_str(payload, &["scope_id"]) == Some("execass-runtime-host") => {}
        _ => return Err("receipt_payload_binding_mismatch"),
    }
    if json_optional_string(payload, &["delegation_parent_digest"])
        != row.previous_receipt_digest.as_deref()
        || json_optional_string(payload, &["global_parent_digest"])
            != row.global_previous_receipt_digest.as_deref()
    {
        return Err("receipt_payload_binding_mismatch");
    }
    Ok(())
}

fn verify_evidence_binding(
    conn: &Connection,
    row: &PersistedReceipt,
    payload: &serde_json::Value,
) -> std::result::Result<(), &'static str> {
    let Some(expected) = payload
        .get("evidence_refs")
        .and_then(serde_json::Value::as_array)
    else {
        return Err("receipt_evidence_binding_mismatch");
    };
    let mut statement = conn
        .prepare("SELECT authority_kind,source_id,authoritative_revision,authority_link_id,observation_digest,deep_link FROM execass_receipt_evidence_refs WHERE receipt_id=?1 ORDER BY ordinal")
        .map_err(|_| "receipt_evidence_query_failed")?;
    let actual = statement
        .query_map([&row.receipt_id], |evidence_row| {
            Ok((
                evidence_row.get::<_, String>(0)?,
                evidence_row.get::<_, String>(1)?,
                evidence_row.get::<_, i64>(2)?,
                evidence_row.get::<_, String>(3)?,
                evidence_row.get::<_, String>(4)?,
                evidence_row.get::<_, String>(5)?,
            ))
        })
        .map_err(|_| "receipt_evidence_query_failed")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "receipt_evidence_query_failed")?;
    if expected.len() != actual.len() {
        return Err("receipt_evidence_count_mismatch");
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let matches = expected
            .get("authority_kind")
            .and_then(serde_json::Value::as_str)
            == Some(actual.0.as_str())
            && expected
                .get("source_id")
                .and_then(serde_json::Value::as_str)
                == Some(actual.1.as_str())
            && expected
                .get("authoritative_revision")
                .and_then(serde_json::Value::as_i64)
                == Some(actual.2)
            && expected
                .get("authority_link_id")
                .and_then(serde_json::Value::as_str)
                == Some(actual.3.as_str())
            && expected
                .get("observation_digest")
                .and_then(serde_json::Value::as_str)
                == Some(actual.4.as_str())
            && expected
                .get("deep_link")
                .and_then(serde_json::Value::as_str)
                == Some(actual.5.as_str());
        if !matches {
            return Err("receipt_evidence_binding_mismatch");
        }
    }
    Ok(())
}

fn json_at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    path.iter().try_fold(value, |current, key| current.get(key))
}

fn json_str<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    json_at(value, path).and_then(serde_json::Value::as_str)
}

fn json_i64(value: &serde_json::Value, path: &[&str]) -> Option<i64> {
    json_at(value, path).and_then(serde_json::Value::as_i64)
}

fn json_optional_string<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    match json_at(value, path) {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value),
        _ => None,
    }
}

fn history_failure(
    reason: &'static str,
    first_global_sequence: Option<i64>,
    first_receipt_id: Option<&str>,
    first_delegation_id: Option<&str>,
    first_delegation_sequence: Option<i64>,
) -> ReceiptHistoryFailure {
    ReceiptHistoryFailure {
        category: "receipt_history",
        reason,
        first_global_sequence,
        first_receipt_id: first_receipt_id.map(str::to_owned),
        first_delegation_id: first_delegation_id.map(str::to_owned),
        first_delegation_sequence,
    }
}

fn receipt_by_append_identity(
    conn: &Connection,
    identity: &str,
) -> Result<Option<(ReceiptRecord, ChainPosition)>> {
    conn.query_row("SELECT receipt_id,delegation_id,receipt_sequence,global_sequence,append_identity,receipt_digest,keyed_integrity_tag,previous_key_integrity_tag,canonical_payload,parent_receipt_id,global_parent_receipt_id,previous_receipt_digest,global_previous_receipt_digest FROM execass_receipts WHERE append_identity=?1", params![identity], |row| Ok((ReceiptRecord { receipt_id:row.get(0)?,delegation_id:row.get(1)?,delegation_sequence:row.get(2)?,global_sequence:row.get(3)?,append_identity:row.get(4)?,receipt_digest:row.get(5)?,keyed_integrity_tag:row.get(6)?,previous_key_integrity_tag:row.get(7)?,canonical_payload:row.get(8)? }, ChainPosition { delegation_sequence:row.get(2)?,global_sequence:row.get(3)?,delegation_parent_id:row.get(9)?,global_parent_id:row.get(10)?,delegation_parent_digest:row.get(11)?,global_parent_digest:row.get(12)? }))).optional().context("failed reading receipt replay identity")
}

pub(super) fn load_receipt(conn: &Connection, receipt_id: &str) -> Result<Option<ReceiptRecord>> {
    conn.query_row("SELECT receipt_id,delegation_id,receipt_sequence,global_sequence,append_identity,receipt_digest,keyed_integrity_tag,previous_key_integrity_tag,canonical_payload FROM execass_receipts WHERE receipt_id=?1",params![receipt_id],|row| Ok(ReceiptRecord{receipt_id:row.get(0)?,delegation_id:row.get(1)?,delegation_sequence:row.get(2)?,global_sequence:row.get(3)?,append_identity:row.get(4)?,receipt_digest:row.get(5)?,keyed_integrity_tag:row.get(6)?,previous_key_integrity_tag:row.get(7)?,canonical_payload:row.get(8)?})).optional().context("failed loading canonical receipt")
}

pub(super) fn receipt_by_causation_event(
    conn: &Connection,
    event_id: &str,
) -> Result<Option<ReceiptRecord>> {
    let receipt_id = conn
        .query_row(
            "SELECT receipt_id FROM execass_receipts WHERE causation_event_id=?1",
            params![event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed reading receipt by causation event")?;
    receipt_id
        .as_deref()
        .map(|id| load_receipt(conn, id))
        .transpose()
        .map(Option::flatten)
}

fn validate_digest_opt(value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        validate_digest(value)?;
    }
    Ok(())
}
fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("digest must be lowercase SHA-256 hex");
    }
    Ok(())
}
fn decode_hex(value: &str) -> Result<Vec<u8>> {
    validate_digest(value)?;
    (0..32)
        .map(|index| {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).context("invalid digest")
        })
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn length_prefix(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}
