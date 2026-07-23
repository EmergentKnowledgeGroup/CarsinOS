//! EA-213 pinned recorder evidence verification and atomic convergence.

use super::claim::{
    claim_resource_set_json, current_runtime_lease, derived_runtime_authority,
    ensure_runtime_receipt_actor, insert_operation_history, load_action, load_operation_history,
    load_resource_reservations, receipt_runtime_mismatch, resource_identity_set_digest,
    update_action_status, validate_claim_provenance, validate_identity,
    validate_write_outbox_receipt, OperationHistoryWrite,
};
use super::receipt::{
    load_receipt, receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{get_continuation, get_outbox, insert_outbox};
use super::store::ExecAssStore;
use super::types::*;
use anyhow::{bail, Context, Result};
use carsinos_protocol::execass_recorder::{
    recorder_observation_signing_bytes, ProviderFailureClassV1, RecorderObservationKindV1,
    RecorderObservationSourceV1, SignedRecorderObservationV1,
};
use ed25519_dalek::{Signature, VerifyingKey};
#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::functions::FunctionFlags;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const MAX_RECONCILIATION_EVIDENCE_AGE_MS: i64 = 5 * 60 * 1_000;
const RECORDER_EVIDENCE_SQL_VERIFIER: &str = "execass_verify_recorder_evidence_v1";

/// Installs the cryptographic half of the recorder-evidence INSERT guard on a
/// single SQLite connection. The SQL trigger supplies the already-pinned
/// verifying key, canonical unsigned payload, and detached signature. Raw SQL
/// can invoke this function, but it cannot make a non-canonical or invalidly
/// signed payload return true.
pub(crate) fn register_recorder_evidence_sql_verifier(connection: &Connection) -> Result<()> {
    connection.create_scalar_function(
        RECORDER_EVIDENCE_SQL_VERIFIER,
        3,
        FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
        |context| {
            let verifying_key_hex = context.get::<String>(0)?;
            let signed_payload_json = context.get::<String>(1)?;
            let signature_hex = context.get::<String>(2)?;
            Ok(i64::from(
                verify_sql_recorder_evidence(
                    &verifying_key_hex,
                    &signed_payload_json,
                    &signature_hex,
                )
                .is_ok(),
            ))
        },
    )?;
    Ok(())
}

fn verify_sql_recorder_evidence(
    verifying_key_hex: &str,
    signed_payload_json: &str,
    signature_hex: &str,
) -> Result<()> {
    let verifying_bytes = decode_hex::<32>(verifying_key_hex)
        .context("recorder SQL verifier key encoding is invalid")?;
    let verifying_key = VerifyingKey::from_bytes(&verifying_bytes)
        .context("recorder SQL verifier key is invalid")?;
    let signature_bytes = decode_hex::<64>(signature_hex)
        .context("recorder SQL verifier signature encoding is invalid")?;
    let signature = Signature::from_bytes(&signature_bytes);
    let observation: SignedRecorderObservationV1 = serde_json::from_str(signed_payload_json)
        .context("recorder SQL verifier payload is invalid")?;
    if !observation.signature_hex.is_empty() {
        bail!("recorder SQL verifier payload contains an embedded signature");
    }
    let canonical = recorder_observation_signing_bytes(&observation)
        .context("recorder SQL verifier payload cannot be canonicalized")?;
    if canonical.as_slice() != signed_payload_json.as_bytes() {
        bail!("recorder SQL verifier payload is not exact canonical signing bytes");
    }
    verifying_key
        .verify_strict(&canonical, &signature)
        .context("recorder SQL verifier signature is invalid")?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderAuthorityIdentity {
    pub recorder_key_id: String,
    pub key_generation: u64,
    pub verifying_key_hex: String,
    pub verifying_key_digest: String,
    pub canonical_root_identity: String,
    pub installation_identity: String,
    pub os_user_identity_digest: String,
    pub state_root_generation: i64,
}

#[derive(Clone)]
pub struct VerifiedRecorderEvidence {
    observation: SignedRecorderObservationV1,
    signed_payload_json: String,
    result: RecorderEvidenceResult,
    verified_at: i64,
}

impl fmt::Debug for VerifiedRecorderEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedRecorderEvidence")
            .field("result", &self.result)
            .field("record_digest", &self.observation.record_digest)
            .field("attempt_id", &self.observation.attempt_id)
            .field("signature_verified", &true)
            .finish()
    }
}

impl VerifiedRecorderEvidence {
    pub fn result(&self) -> RecorderEvidenceResult {
        self.result
    }

    pub fn recorder_record_digest(&self) -> &str {
        &self.observation.record_digest
    }

    pub fn attempt_id(&self) -> &str {
        &self.observation.attempt_id
    }

    #[cfg(test)]
    pub(super) fn corrupt_source_for_raw_sql_test(&mut self, source: RecorderObservationSourceV1) {
        self.observation.source = source;
        if source == RecorderObservationSourceV1::Execution {
            self.observation.reconciliation_window_start_ms = None;
            self.observation.reconciliation_window_end_ms = None;
        }
        self.signed_payload_json = String::from_utf8(
            recorder_observation_signing_bytes(&self.observation)
                .expect("test corruption must remain canonically encodable"),
        )
        .expect("canonical recorder payload must remain UTF-8");
    }

    #[cfg(test)]
    pub(super) fn corrupt_signature_for_raw_sql_test(&mut self) {
        self.observation.signature_hex = "00".repeat(64);
    }

    #[cfg(test)]
    pub(super) fn make_signed_payload_noncanonical_for_raw_sql_test(&mut self) {
        self.signed_payload_json.insert(0, ' ');
    }

    #[cfg(test)]
    pub(super) fn duplicate_signed_payload_key_for_raw_sql_test(&mut self) {
        self.signed_payload_json
            .insert_str(1, "\"sequence\":999999,");
    }

    #[cfg(test)]
    pub(super) fn corrupt_projected_failure_class_for_raw_sql_test(
        &mut self,
        class: ProviderFailureClassV1,
    ) {
        self.observation.provider_error_class = Some(class);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderEvidenceVerificationError {
    code: &'static str,
}

impl RecorderEvidenceVerificationError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for RecorderEvidenceVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "recorder evidence verification failed: {}",
            self.code
        )
    }
}

impl std::error::Error for RecorderEvidenceVerificationError {}

#[derive(Debug)]
struct ActiveRecorderKey {
    recorder_key_id: String,
    key_generation: i64,
    verifying_key_hex: String,
    verifying_key_digest: String,
    canonical_root_identity: String,
    installation_identity: String,
    os_user_identity_digest: String,
    state_root_generation: i64,
}

#[derive(Debug)]
struct RuntimeIdentity {
    state_root_generation: i64,
    installation_identity: String,
    os_user_identity_digest: String,
}

#[derive(Debug)]
struct AttemptFacts {
    logical_effect_id: String,
    attempt_status: String,
    provider_error_class: Option<String>,
    provider_request_digest: String,
    started_at: i64,
    effect_state: String,
    provider_identity: String,
    provider_idempotency_key: Option<String>,
    reconciliation_key: Option<String>,
}

#[derive(Debug)]
enum ImportMutationOutcome {
    Applied(ImportDraft),
    Replayed(ImportDraft),
    Conflict,
    Lost(ContinuationStaleReason),
}

#[derive(Debug)]
struct ImportDraft {
    claim_identity: ContinuationClaimIdentity,
    result: RecorderEvidenceResult,
    recorder_record_digest: String,
    outbox_event: OutboxEventRecord,
    reservations: Vec<TechnicalResourceReservationRecord>,
}

impl ExecAssStore {
    /// Pins the recorder's public identity only when it matches the canonical
    /// root and the exact currently active installation/OS-user generation.
    /// Re-pinning is idempotent only for byte-identical identity; rotation or
    /// replacement is rejected by this EA-213 boundary.
    pub fn pin_or_validate_recorder_identity(
        &self,
        identity: &RecorderAuthorityIdentity,
        trusted_now: i64,
    ) -> Result<()> {
        validate_recorder_identity_shape(identity)?;
        if trusted_now <= 0 || identity.canonical_root_identity != self.root_identity {
            bail!("recorder identity does not match the canonical state root");
        }
        let mut connection = self.connection()?;
        let transaction = super::store::immediate_transaction(&mut connection)?;
        let runtime = active_runtime_identity(&transaction, trusted_now)?
            .context("recorder identity pin requires one active runtime identity")?;
        if runtime.state_root_generation != identity.state_root_generation
            || runtime.installation_identity != identity.installation_identity
            || runtime.os_user_identity_digest != identity.os_user_identity_digest
        {
            bail!("recorder identity does not match the active root installation identity");
        }
        let existing = load_active_recorder_key(&transaction)?;
        match existing {
            Some(existing) => {
                if existing.recorder_key_id != identity.recorder_key_id
                    || existing.key_generation != i64::try_from(identity.key_generation)?
                    || existing.verifying_key_hex != identity.verifying_key_hex
                    || existing.verifying_key_digest != identity.verifying_key_digest
                    || existing.canonical_root_identity != identity.canonical_root_identity
                    || existing.installation_identity != identity.installation_identity
                    || existing.os_user_identity_digest != identity.os_user_identity_digest
                    || existing.state_root_generation != identity.state_root_generation
                {
                    bail!("active recorder identity cannot be replaced or rotated");
                }
            }
            None => {
                transaction.execute(
                    r#"INSERT INTO execass_effect_recorder_keys(
                         recorder_key_id,key_generation,verifying_key_hex,verifying_key_digest,
                         canonical_root_identity,installation_identity,os_user_identity_digest,
                         state_root_generation,status,created_at
                       ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'active',?9)"#,
                    params![
                        identity.recorder_key_id,
                        i64::try_from(identity.key_generation)?,
                        identity.verifying_key_hex,
                        identity.verifying_key_digest,
                        identity.canonical_root_identity,
                        identity.installation_identity,
                        identity.os_user_identity_digest,
                        identity.state_root_generation,
                        trusted_now,
                    ],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Verifies one recorder-served signed terminal observation against the
    /// pinned key and current storage bindings. This is deliberately not a
    /// complete-journal membership proof: the recorder verifies its fsynced
    /// hash-chain before serving/signing, while storage may import a sparse,
    /// interleaved subset of terminal observations. The gateway has no signing
    /// key, so it cannot manufacture a skipped or contradictory observation.
    pub fn verify_recorder_evidence(
        &self,
        observation: &SignedRecorderObservationV1,
        trusted_now: i64,
    ) -> std::result::Result<VerifiedRecorderEvidence, RecorderEvidenceVerificationError> {
        let connection = self
            .connection()
            .map_err(|_| verification_error("storage_unavailable"))?;
        verify_observation(&connection, &self.root_identity, observation, trusted_now)
    }

    /// Reloads one persisted signed terminal observation and performs the full
    /// active-key and storage-binding verification again. This re-verifies the
    /// signed observation, not membership of every record in the recorder's
    /// complete journal chain.
    pub fn reload_and_verify_recorder_evidence(
        &self,
        recorder_record_digest: &str,
        trusted_now: i64,
    ) -> std::result::Result<VerifiedRecorderEvidence, RecorderEvidenceVerificationError> {
        let connection = self
            .connection()
            .map_err(|_| verification_error("storage_unavailable"))?;
        let stored = connection
            .query_row(
                "SELECT signed_payload_json,signature FROM execass_effect_recorder_evidence WHERE recorder_record_digest=?1",
                params![recorder_record_digest],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| verification_error("persisted_evidence_query_failed"))?
            .ok_or_else(|| verification_error("persisted_evidence_not_found"))?;
        let mut observation: SignedRecorderObservationV1 = serde_json::from_str(&stored.0)
            .map_err(|_| verification_error("persisted_signed_payload_invalid"))?;
        if !observation.signature_hex.is_empty() {
            return Err(verification_error("persisted_signed_payload_not_unsigned"));
        }
        observation.signature_hex = stored.1;
        verify_observation(&connection, &self.root_identity, &observation, trusted_now)
    }

    pub fn reconcile_recorder_evidence_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ReconcileRecorderEvidenceCommand,
    ) -> Result<RecorderEvidenceImportOutcome> {
        let existing = self.connection()?;
        if evidence_digest_exists(
            &existing,
            command.verified_evidence.recorder_record_digest(),
        )? {
            return match inspect_replay(&existing, command)? {
                ImportMutationOutcome::Replayed(draft) => {
                    let receipt = load_receipt(&existing, &command.receipt.receipt_id)?
                        .context("recorder evidence replay receipt disappeared")?;
                    Ok(RecorderEvidenceImportOutcome::Replayed(Box::new(
                        import_record(draft, receipt),
                    )))
                }
                ImportMutationOutcome::Conflict => Ok(RecorderEvidenceImportOutcome::Conflict),
                _ => bail!("existing recorder evidence produced an impossible replay outcome"),
            };
        }
        drop(existing);
        validate_recorder_import_command(command)?;
        let observation = &command.verified_evidence.observation;
        let result = command.verified_evidence.result;
        let outcome = self.mutate_with_atomic_receipt(
            integrity,
            redactor,
            &command.receipt,
            |transaction| {
                if receipt_by_causation_event(transaction, &command.outbox_event.event_id)?.is_some()
                {
                    return replay_import(transaction, command);
                }
                if evidence_digest_exists(transaction, &observation.record_digest)? {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        ImportMutationOutcome::Conflict,
                    ));
                }
                let Some((authoritative_claim, _)) = authoritative_claim_for_attempt(
                    transaction,
                    &observation.attempt_id,
                )? else {
                    return Ok(AtomicReceiptMutation::NoAppend(ImportMutationOutcome::Lost(
                        ContinuationStaleReason::ClaimIdentityMismatch,
                    )));
                };
                if authoritative_claim != command.claim_identity
                    || !validate_claim_provenance(transaction, &authoritative_claim)?
                {
                    return Ok(AtomicReceiptMutation::NoAppend(ImportMutationOutcome::Lost(
                        ContinuationStaleReason::ClaimIdentityMismatch,
                    )));
                }
                let continuation = get_continuation(
                    transaction,
                    &command.claim_identity.continuation_id,
                )?
                .context("recorder evidence continuation disappeared")?;
                if continuation.delegation_id != command.claim_identity.delegation_id
                    || continuation.action_id != command.claim_identity.action_id
                    || continuation.fencing_token
                        != command.claim_identity.continuation_fencing_token
                {
                    return Ok(AtomicReceiptMutation::NoAppend(ImportMutationOutcome::Lost(
                        ContinuationStaleReason::ClaimIdentityMismatch,
                    )));
                }
                let runtime = current_runtime_lease(transaction, command.trusted_now)?
                    .context("recorder evidence import requires a current runtime host")?;
                if receipt_runtime_mismatch(&command.receipt, &runtime) {
                    bail!("recorder evidence receipt is not from the current runtime host");
                }
                let delegation = super::rows::get_delegation(
                    transaction,
                    &command.claim_identity.delegation_id,
                )?
                .context("recorder evidence delegation disappeared")?;
                let (runtime_authority, runtime_actor) =
                    derived_runtime_authority(&runtime, delegation.policy_revision)?;
                ensure_runtime_receipt_actor(
                    transaction,
                    &command.receipt,
                    &runtime_authority,
                    &runtime_actor,
                )?;

                revalidate_verified_in_transaction(transaction, command)?;
                insert_recorder_evidence(transaction, command)?;
                insert_outbox(transaction, &command.outbox_event)?;
                let resource_json =
                    claim_resource_set_json(transaction, &command.claim_identity)?;
                // A signed execution `Absent` terminates only this provider attempt. The
                // continuation remains executing and may retry, so it must not consume the
                // claim's one authoritative `settle` history slot. Its signed evidence,
                // receipt/ref, and outbox event remain the durable attempt-level audit.
                if !(observation.source == RecorderObservationSourceV1::Execution
                    && result == RecorderEvidenceResult::Absent)
                {
                    let result_status = result_status(result);
                    insert_operation_history(
                        transaction,
                        OperationHistoryWrite {
                            event_id: &command.outbox_event.event_id,
                            operation: match observation.source {
                                RecorderObservationSourceV1::Execution => "settle",
                                RecorderObservationSourceV1::Reconciliation => "reconcile",
                            },
                            result_status,
                            identity: &command.claim_identity,
                            resource_set_json: &resource_json,
                            resource_evidence_digest: Some(&observation.record_digest),
                            recorded_at: command.write.occurred_at,
                        },
                    )?;
                }
                apply_atomic_convergence(transaction, command, &continuation)?;
                transaction.execute(
                    "INSERT INTO execass_receipt_recorder_evidence_refs(receipt_id,delegation_id,recorder_record_digest,linked_at) VALUES(?1,?2,?3,?4)",
                    params![
                        command.receipt.receipt_id,
                        command.claim_identity.delegation_id,
                        observation.record_digest,
                        command.write.occurred_at,
                    ],
                )?;
                let outbox_event = get_outbox(transaction, &command.outbox_event.event_id)?
                    .context("recorder evidence outbox disappeared")?;
                let reservations = load_resource_reservations(
                    transaction,
                    &command.claim_identity.claim_event_id,
                )?;
                Ok(AtomicReceiptMutation::Append(ImportMutationOutcome::Applied(
                    ImportDraft {
                        claim_identity: authoritative_claim,
                        result,
                        recorder_record_digest: observation.record_digest.clone(),
                        outbox_event,
                        reservations,
                    },
                )))
            },
        )?;

        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: ImportMutationOutcome::Applied(draft),
                receipt,
            } => Ok(RecorderEvidenceImportOutcome::Applied(Box::new(
                import_record(draft, receipt),
            ))),
            AtomicReceiptWriteOutcome::NoAppend(ImportMutationOutcome::Replayed(draft)) => {
                let receipt = receipt_by_causation_event(
                    &self.connection()?,
                    &draft.outbox_event.event.event_id,
                )?
                .context("recorder evidence replay receipt disappeared")?;
                Ok(RecorderEvidenceImportOutcome::Replayed(Box::new(
                    import_record(draft, receipt),
                )))
            }
            AtomicReceiptWriteOutcome::NoAppend(ImportMutationOutcome::Conflict) => {
                Ok(RecorderEvidenceImportOutcome::Conflict)
            }
            AtomicReceiptWriteOutcome::NoAppend(ImportMutationOutcome::Lost(reason)) => {
                Ok(RecorderEvidenceImportOutcome::Lost { reason })
            }
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            } => Ok(RecorderEvidenceImportOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            }),
            AtomicReceiptWriteOutcome::Appended { .. }
            | AtomicReceiptWriteOutcome::NoAppend(ImportMutationOutcome::Applied(_)) => {
                bail!("recorder evidence receipt coordinator returned an impossible outcome")
            }
        }
    }
}

fn validate_recorder_identity_shape(identity: &RecorderAuthorityIdentity) -> Result<()> {
    require_text("recorder_key_id", &identity.recorder_key_id)?;
    require_root_digest(&identity.canonical_root_identity)?;
    require_text("installation_identity", &identity.installation_identity)?;
    require_hex(
        "os_user_identity_digest",
        &identity.os_user_identity_digest,
        64,
    )?;
    require_hex("verifying_key_hex", &identity.verifying_key_hex, 64)?;
    require_hex("verifying_key_digest", &identity.verifying_key_digest, 64)?;
    if identity.key_generation == 0 || identity.state_root_generation <= 0 {
        bail!("recorder key/root generation must be positive");
    }
    let verifying_bytes = decode_hex::<32>(&identity.verifying_key_hex)
        .context("recorder verifying key encoding is invalid")?;
    VerifyingKey::from_bytes(&verifying_bytes).context("recorder verifying key is invalid")?;
    let derived = hex_encode(&Sha256::digest(verifying_bytes));
    if derived != identity.verifying_key_digest {
        bail!("recorder verifying key digest mismatch");
    }
    Ok(())
}

fn verify_observation(
    connection: &Connection,
    root_identity: &str,
    observation: &SignedRecorderObservationV1,
    trusted_now: i64,
) -> std::result::Result<VerifiedRecorderEvidence, RecorderEvidenceVerificationError> {
    if trusted_now <= 0 {
        return Err(verification_error("invalid_trusted_time"));
    }
    let key = load_active_recorder_key(connection)
        .map_err(|_| verification_error("active_key_query_failed"))?
        .ok_or_else(|| verification_error("active_key_missing"))?;
    let generation = i64::try_from(observation.recorder_key_generation)
        .map_err(|_| verification_error("key_generation_invalid"))?;
    if observation.recorder_key_id != key.recorder_key_id || generation != key.key_generation {
        return Err(verification_error("active_key_mismatch"));
    }
    if observation.canonical_root_identity != root_identity
        || observation.canonical_root_identity != key.canonical_root_identity
        || observation.installation_id != key.installation_identity
        || observation.state_root_generation != key.state_root_generation
        || observation.os_user_identity_digest != key.os_user_identity_digest
    {
        return Err(verification_error("root_installation_binding_mismatch"));
    }
    let runtime = active_runtime_identity(connection, trusted_now)
        .map_err(|_| verification_error("runtime_identity_query_failed"))?
        .ok_or_else(|| verification_error("active_runtime_identity_missing"))?;
    if runtime.state_root_generation != key.state_root_generation
        || runtime.installation_identity != key.installation_identity
        || runtime.os_user_identity_digest != key.os_user_identity_digest
    {
        return Err(verification_error("active_runtime_identity_mismatch"));
    }
    validate_observation_shape(observation, trusted_now)?;
    verify_signature(&key, observation)?;
    validate_imported_observation_subset(connection, observation)?;
    let exact_import = evidence_digest_exists(connection, &observation.record_digest)
        .map_err(|_| verification_error("evidence_replay_query_failed"))?;
    let facts = load_attempt_facts(connection, &observation.attempt_id)
        .map_err(|_| verification_error("attempt_query_failed"))?
        .ok_or_else(|| verification_error("attempt_not_found"))?;
    validate_attempt_binding(observation, &facts, exact_import)?;
    validate_signed_actuals(connection, observation, exact_import)?;
    let signed_payload = recorder_observation_signing_bytes(observation)
        .map_err(|_| verification_error("signing_payload_failed"))?;
    let signed_payload_json = String::from_utf8(signed_payload)
        .map_err(|_| verification_error("signing_payload_not_utf8"))?;
    Ok(VerifiedRecorderEvidence {
        observation: observation.clone(),
        signed_payload_json,
        result: result_from_kind(observation.kind)?,
        verified_at: trusted_now,
    })
}

fn validate_observation_shape(
    observation: &SignedRecorderObservationV1,
    trusted_now: i64,
) -> std::result::Result<(), RecorderEvidenceVerificationError> {
    observation
        .validate_shape()
        .map_err(|_| verification_error("provider_error_class_shape_invalid"))?;
    if observation.sequence == 0 || observation.sequence > i64::MAX as u64 {
        return Err(verification_error("journal_sequence_invalid"));
    }
    for value in [
        &observation.record_id,
        &observation.attempt_id,
        &observation.logical_effect_id,
        &observation.provider_identity,
        &observation.provider_version,
        &observation.recorder_key_id,
    ] {
        if value.trim().is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            return Err(verification_error("text_field_invalid"));
        }
    }
    for digest in [
        &observation.record_digest,
        &observation.command_digest,
        &observation.provider_request_digest,
        &observation.previous_record_digest,
    ] {
        if !valid_digest(digest) {
            return Err(verification_error("digest_invalid"));
        }
    }
    for digest in [
        observation.provider_idempotency_key_digest.as_deref(),
        observation.reconciliation_key_digest.as_deref(),
        observation.response_digest.as_deref(),
        observation.evidence_payload_digest.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !valid_digest(digest) {
            return Err(verification_error("optional_digest_invalid"));
        }
    }
    if observation.observed_at_ms <= 0 || observation.observed_at_ms > trusted_now {
        return Err(verification_error("observation_time_invalid"));
    }
    if observation.response_digest.is_none() {
        return Err(verification_error("response_digest_missing"));
    }
    if observation
        .remote_effect_id
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
    {
        return Err(verification_error("remote_effect_id_invalid"));
    }
    match observation.source {
        RecorderObservationSourceV1::Execution => {
            if observation.reconciliation_window_start_ms.is_some()
                || observation.reconciliation_window_end_ms.is_some()
            {
                return Err(verification_error("execution_window_forbidden"));
            }
        }
        RecorderObservationSourceV1::Reconciliation => {
            let (Some(start), Some(end)) = (
                observation.reconciliation_window_start_ms,
                observation.reconciliation_window_end_ms,
            ) else {
                return Err(verification_error("reconciliation_window_missing"));
            };
            if start < 0
                || end < start
                || end > observation.observed_at_ms
                || trusted_now - observation.observed_at_ms > MAX_RECONCILIATION_EVIDENCE_AGE_MS
                || observation.evidence_payload_digest.is_none()
            {
                return Err(verification_error("reconciliation_window_invalid_or_stale"));
            }
        }
    }
    match result_from_kind(observation.kind)? {
        RecorderEvidenceResult::Absent => {
            if observation.remote_effect_id.is_some()
                || !observation.technical_resource_actuals.is_empty()
            {
                return Err(verification_error("absent_payload_invalid"));
            }
        }
        RecorderEvidenceResult::Unknown => {
            if observation.remote_effect_id.is_some()
                || !observation.technical_resource_actuals.is_empty()
            {
                return Err(verification_error("unknown_payload_invalid"));
            }
        }
        RecorderEvidenceResult::Present => {}
    }
    require_hex_verification(&observation.signature_hex, 128, "signature_invalid")
}

fn verify_signature(
    key: &ActiveRecorderKey,
    observation: &SignedRecorderObservationV1,
) -> std::result::Result<(), RecorderEvidenceVerificationError> {
    let verifying_bytes = decode_hex::<32>(&key.verifying_key_hex)
        .map_err(|_| verification_error("verifying_key_invalid"))?;
    if hex_encode(&Sha256::digest(verifying_bytes)) != key.verifying_key_digest {
        return Err(verification_error("verifying_key_digest_mismatch"));
    }
    let verifying = VerifyingKey::from_bytes(&verifying_bytes)
        .map_err(|_| verification_error("verifying_key_invalid"))?;
    let signature_bytes = decode_hex::<64>(&observation.signature_hex)
        .map_err(|_| verification_error("signature_invalid"))?;
    let signature = Signature::from_bytes(&signature_bytes);
    let bytes = recorder_observation_signing_bytes(observation)
        .map_err(|_| verification_error("signing_payload_failed"))?;
    verifying
        .verify_strict(&bytes, &signature)
        .map_err(|_| verification_error("signature_mismatch"))
}

/// Enforces monotonicity and terminal non-contradiction inside storage's
/// imported subset. Sequence gaps and `previous_record_digest` values that do
/// not name an imported row are valid: Accepted/InvocationStarted records and
/// observations for other attempts remain only in the recorder's complete,
/// verified journal.
fn validate_imported_observation_subset(
    connection: &Connection,
    observation: &SignedRecorderObservationV1,
) -> std::result::Result<(), RecorderEvidenceVerificationError> {
    let sequence = i64::try_from(observation.sequence)
        .map_err(|_| verification_error("journal_sequence_invalid"))?;
    let global = connection
        .query_row(
            "SELECT recorder_record_digest,attempt_id FROM execass_effect_recorder_evidence WHERE recorder_key_id=?1 AND key_generation=?2 AND journal_sequence=?3",
            params![observation.recorder_key_id, i64::try_from(observation.recorder_key_generation).unwrap_or_default(), sequence],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| verification_error("sequence_query_failed"))?;
    if let Some((digest, attempt_id)) = global {
        if digest != observation.record_digest || attempt_id != observation.attempt_id {
            return Err(verification_error("duplicate_journal_sequence"));
        }
    }
    let latest = connection
        .query_row(
            "SELECT journal_sequence,recorder_record_digest,command_digest,journal_kind FROM execass_effect_recorder_evidence WHERE attempt_id=?1 ORDER BY journal_sequence DESC LIMIT 1",
            params![observation.attempt_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
        )
        .optional()
        .map_err(|_| verification_error("attempt_sequence_query_failed"))?;
    if let Some((latest_sequence, digest, command_digest, kind)) = latest {
        if command_digest != observation.command_digest {
            return Err(verification_error("command_digest_conflict"));
        }
        if latest_sequence > sequence {
            return Err(verification_error("stale_journal_evidence"));
        }
        if latest_sequence == sequence && digest != observation.record_digest {
            return Err(verification_error("duplicate_attempt_sequence"));
        }
        if matches!(kind.as_str(), "present" | "absent") && digest != observation.record_digest {
            return Err(verification_error("terminal_evidence_conflict"));
        }
    }
    Ok(())
}

fn validate_attempt_binding(
    observation: &SignedRecorderObservationV1,
    facts: &AttemptFacts,
    exact_import: bool,
) -> std::result::Result<(), RecorderEvidenceVerificationError> {
    if observation.logical_effect_id != facts.logical_effect_id
        || observation.provider_identity != facts.provider_identity
        || (facts.provider_identity == "carsinos.local-fs.exact-overwrite"
            && observation.provider_version != "v1")
        || observation.provider_request_digest != facts.provider_request_digest
        || observation.observed_at_ms < facts.started_at
        || observation
            .reconciliation_window_start_ms
            .is_some_and(|start| start < facts.started_at)
        || observation.provider_idempotency_key_digest
            != facts
                .provider_idempotency_key
                .as_deref()
                .map(stable_key_digest)
        || observation.reconciliation_key_digest
            != facts.reconciliation_key.as_deref().map(stable_key_digest)
    {
        return Err(verification_error("attempt_provider_binding_mismatch"));
    }
    let result = result_from_kind(observation.kind)?;
    let state_matches = match (observation.source, result, exact_import) {
        (RecorderObservationSourceV1::Execution, RecorderEvidenceResult::Unknown, _)
        | (RecorderObservationSourceV1::Reconciliation, RecorderEvidenceResult::Unknown, _) => {
            matches!(
                facts.attempt_status.as_str(),
                "invoking" | "outcome_unknown"
            ) && matches!(facts.effect_state.as_str(), "invoking" | "outcome_unknown")
        }
        (RecorderObservationSourceV1::Execution, RecorderEvidenceResult::Present, false) => {
            facts.attempt_status == "invoking" && facts.effect_state == "invoking"
        }
        (RecorderObservationSourceV1::Execution, RecorderEvidenceResult::Absent, false) => {
            facts.attempt_status == "invoking" && facts.effect_state == "invoking"
        }
        (RecorderObservationSourceV1::Reconciliation, RecorderEvidenceResult::Present, false)
        | (RecorderObservationSourceV1::Reconciliation, RecorderEvidenceResult::Absent, false) => {
            facts.attempt_status == "outcome_unknown" && facts.effect_state == "outcome_unknown"
        }
        (RecorderObservationSourceV1::Execution, RecorderEvidenceResult::Present, true) => {
            matches!(
                facts.attempt_status.as_str(),
                "succeeded" | "reconciled_present"
            ) && matches!(
                facts.effect_state.as_str(),
                "succeeded" | "reconciled_present"
            )
        }
        (RecorderObservationSourceV1::Execution, RecorderEvidenceResult::Absent, true) => {
            facts.attempt_status == "failed" && facts.effect_state == "failed"
        }
        (RecorderObservationSourceV1::Reconciliation, RecorderEvidenceResult::Absent, true) => {
            facts.attempt_status == "reconciled_absent" && facts.effect_state == "reconciled_absent"
        }
        (RecorderObservationSourceV1::Reconciliation, RecorderEvidenceResult::Present, true) => {
            facts.attempt_status == "reconciled_present"
                && facts.effect_state == "reconciled_present"
        }
    };
    if !state_matches {
        return Err(verification_error("attempt_effect_state_mismatch"));
    }
    if exact_import
        && facts.provider_error_class.as_deref()
            != observation
                .provider_error_class
                .map(provider_failure_class_as_str)
    {
        return Err(verification_error(
            "persisted_provider_error_class_mismatch",
        ));
    }
    Ok(())
}

fn validate_signed_actuals(
    connection: &Connection,
    observation: &SignedRecorderObservationV1,
    exact_import: bool,
) -> std::result::Result<(), RecorderEvidenceVerificationError> {
    let mut statement = connection
        .prepare(
            "SELECT reservation_id,amount_reserved,status FROM execass_technical_resource_reservations WHERE logical_effect_id=?1 ORDER BY reservation_id",
        )
        .map_err(|_| verification_error("reservation_query_failed"))?;
    let reservations = statement
        .query_map(params![observation.logical_effect_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| verification_error("reservation_query_failed"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| verification_error("reservation_query_failed"))?;
    if reservations.is_empty() {
        return Err(verification_error("reservation_set_empty"));
    }
    let result = result_from_kind(observation.kind)?;
    match result {
        RecorderEvidenceResult::Present => {
            if observation.technical_resource_actuals.len() != reservations.len() {
                return Err(verification_error("actual_count_mismatch"));
            }
            let mut seen = BTreeSet::new();
            let actuals = observation
                .technical_resource_actuals
                .iter()
                .map(|actual| (actual.reservation_id.as_str(), actual))
                .collect::<BTreeMap<_, _>>();
            if actuals.len() != observation.technical_resource_actuals.len() {
                return Err(verification_error("actual_duplicate"));
            }
            for (reservation_id, amount_reserved, status) in reservations {
                let actual = actuals
                    .get(reservation_id.as_str())
                    .ok_or_else(|| verification_error("actual_missing"))?;
                let preimport_status_matches = match observation.source {
                    RecorderObservationSourceV1::Execution => {
                        matches!(status.as_str(), "reserved" | "reconciliation_required")
                    }
                    RecorderObservationSourceV1::Reconciliation => {
                        status == "reconciliation_required"
                    }
                };
                if !seen.insert(reservation_id.clone())
                    || actual.amount_actual < 0
                    || actual.amount_actual > amount_reserved
                    || !valid_digest(&actual.evidence_digest)
                    || (!exact_import && !preimport_status_matches)
                    || (exact_import && status != "settled")
                {
                    return Err(verification_error("actual_binding_invalid"));
                }
                if exact_import {
                    let stored = connection
                        .query_row(
                            "SELECT amount_actual,evidence_digest FROM execass_technical_resource_actuals WHERE reservation_id=?1",
                            params![reservation_id],
                            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                        )
                        .optional()
                        .map_err(|_| verification_error("actual_query_failed"))?;
                    if stored != Some((actual.amount_actual, actual.evidence_digest.clone())) {
                        return Err(verification_error("persisted_actual_mismatch"));
                    }
                }
            }
        }
        RecorderEvidenceResult::Absent => {
            let states_match = reservations.iter().all(|(_, _, status)| {
                if exact_import {
                    match observation.source {
                        RecorderObservationSourceV1::Execution => status == "reserved",
                        RecorderObservationSourceV1::Reconciliation => status == "released",
                    }
                } else {
                    match observation.source {
                        RecorderObservationSourceV1::Execution => status == "reserved",
                        RecorderObservationSourceV1::Reconciliation => {
                            status == "reconciliation_required"
                        }
                    }
                }
            });
            if !states_match {
                return Err(verification_error("absent_reservation_state_mismatch"));
            }
        }
        RecorderEvidenceResult::Unknown => {
            if reservations.iter().any(|(_, _, status)| {
                !matches!(status.as_str(), "reserved" | "reconciliation_required")
            }) {
                return Err(verification_error("unknown_reservation_state_mismatch"));
            }
        }
    }
    Ok(())
}

fn validate_recorder_import_command(command: &ReconcileRecorderEvidenceCommand) -> Result<()> {
    validate_identity(&command.claim_identity)?;
    validate_write_outbox_receipt(
        &command.write,
        &command.outbox_event,
        &command.receipt,
        &command.claim_identity.continuation_id,
        command.trusted_now,
    )?;
    let evidence = &command.verified_evidence;
    if evidence.verified_at != command.trusted_now
        || command.write.occurred_at != command.trusted_now
        || command.write.causation_id != evidence.observation.record_digest
        || command.receipt.causation_id != evidence.observation.record_digest
        || !command.receipt.evidence.is_empty()
        || command.outbox_event.aggregate_id != command.claim_identity.delegation_id
        || command.receipt.delegation_id != command.claim_identity.delegation_id
        || command.outbox_event.aggregate_revision != command.receipt.expected_state_revision
        || command.receipt.subject.revision != command.receipt.expected_state_revision
        || command.receipt.state_root_generation != command.claim_identity.state_root_generation
    {
        bail!("recorder evidence import command identity is not exact");
    }
    let expected_payload = canonical_import_payload(&evidence.observation, evidence.result)?;
    if command.outbox_event.safe_payload_json != expected_payload {
        bail!("recorder evidence outbox payload is not storage-derived canonical evidence");
    }
    Ok(())
}

fn revalidate_verified_in_transaction(
    transaction: &Transaction<'_>,
    command: &ReconcileRecorderEvidenceCommand,
) -> Result<()> {
    let observation = &command.verified_evidence.observation;
    let key = load_active_recorder_key(transaction)?.context("active recorder key disappeared")?;
    if key.recorder_key_id != observation.recorder_key_id
        || key.key_generation != i64::try_from(observation.recorder_key_generation)?
        || key.canonical_root_identity != observation.canonical_root_identity
        || key.installation_identity != observation.installation_id
        || key.state_root_generation != observation.state_root_generation
        || key.os_user_identity_digest != observation.os_user_identity_digest
    {
        bail!("verified recorder key/root binding changed before import");
    }
    let facts = load_attempt_facts(transaction, &observation.attempt_id)?
        .context("verified recorder attempt disappeared")?;
    validate_attempt_binding(observation, &facts, false).map_err(|error| anyhow::anyhow!(error))?;
    validate_signed_actuals(transaction, observation, false)
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(())
}

pub(super) fn insert_recorder_evidence(
    transaction: &Transaction<'_>,
    command: &ReconcileRecorderEvidenceCommand,
) -> Result<()> {
    let observation = &command.verified_evidence.observation;
    let actuals_json =
        String::from_utf8(carsinos_protocol::execass_recorder::canonical_json_bytes(
            &observation.technical_resource_actuals,
        )?)?;
    transaction.execute(
        r#"INSERT INTO execass_effect_recorder_evidence(
             recorder_record_digest,record_id,recorder_key_id,key_generation,
             canonical_root_identity,installation_identity,state_root_generation,
             os_user_identity_digest,journal_sequence,journal_kind,journal_source,
             attempt_id,logical_effect_id,command_digest,provider_identity,provider_version,
             provider_request_digest,provider_idempotency_key_digest,reconciliation_key_digest,
             remote_effect_id,response_digest,provider_error_class,evidence_payload_digest,
             technical_resource_actuals_json,reconciliation_window_start,
             reconciliation_window_end,observed_at,imported_at,previous_record_digest,
             signed_payload_json,signature
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
             ?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31)"#,
        params![
            observation.record_digest,
            observation.record_id,
            observation.recorder_key_id,
            i64::try_from(observation.recorder_key_generation)?,
            observation.canonical_root_identity,
            observation.installation_id,
            observation.state_root_generation,
            observation.os_user_identity_digest,
            i64::try_from(observation.sequence)?,
            result_from_kind(observation.kind)
                .map_err(|error| anyhow::anyhow!(error))?
                .as_str(),
            source_as_str(observation.source),
            observation.attempt_id,
            observation.logical_effect_id,
            observation.command_digest,
            observation.provider_identity,
            observation.provider_version,
            observation.provider_request_digest,
            observation.provider_idempotency_key_digest,
            observation.reconciliation_key_digest,
            observation.remote_effect_id,
            observation.response_digest,
            observation
                .provider_error_class
                .map(provider_failure_class_as_str),
            observation.evidence_payload_digest,
            actuals_json,
            observation.reconciliation_window_start_ms,
            observation.reconciliation_window_end_ms,
            observation.observed_at_ms,
            command.write.occurred_at,
            observation.previous_record_digest,
            command.verified_evidence.signed_payload_json,
            observation.signature_hex,
        ],
    )?;
    Ok(())
}

fn apply_atomic_convergence(
    transaction: &Transaction<'_>,
    command: &ReconcileRecorderEvidenceCommand,
    continuation: &ContinuationRecord,
) -> Result<()> {
    let observation = &command.verified_evidence.observation;
    let result = command.verified_evidence.result;
    let action = load_action(transaction, &continuation.action_id)?
        .context("recorder evidence action disappeared")?;
    let reservations =
        load_resource_reservations(transaction, &command.claim_identity.claim_event_id)?;
    if reservations.is_empty()
        || reservations.iter().any(|reservation| {
            reservation.identity.logical_effect_id != observation.logical_effect_id
        })
    {
        bail!("recorder evidence lost its exact reservation/effect set");
    }

    match result {
        RecorderEvidenceResult::Present => {
            for reservation in &reservations {
                let status_is_valid = match observation.source {
                    RecorderObservationSourceV1::Execution => matches!(
                        reservation.status.as_str(),
                        "reserved" | "reconciliation_required"
                    ),
                    RecorderObservationSourceV1::Reconciliation => {
                        reservation.status == "reconciliation_required"
                    }
                };
                if !status_is_valid {
                    bail!("present recorder evidence found an invalid reservation state");
                }
                let actual = observation
                    .technical_resource_actuals
                    .iter()
                    .find(|actual| actual.reservation_id == reservation.identity.reservation_id)
                    .context("present recorder evidence omitted a reservation")?;
                let actual_id = technical_actual_id(
                    &observation.record_digest,
                    &reservation.identity.reservation_id,
                );
                transaction.execute(
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
                        actual.evidence_digest,
                        observation.observed_at_ms,
                    ],
                )?;
                if transaction.execute(
                    "UPDATE execass_technical_resource_reservations SET status='settled',settled_at=?1 WHERE reservation_id=?2 AND status=?3",
                    params![observation.observed_at_ms, reservation.identity.reservation_id, reservation.status],
                )? != 1
                {
                    bail!("present recorder evidence lost a reservation transition");
                }
            }
            transition_present_attempt_and_effect(transaction, observation)?;
            close_recorder_continuation(
                transaction,
                command,
                continuation,
                &action,
                ContinuationStatus::Terminal,
            )?;
        }
        RecorderEvidenceResult::Absent => {
            for reservation in &reservations {
                match observation.source {
                    RecorderObservationSourceV1::Execution => {
                        if reservation.status != "reserved" {
                            bail!(
                                "definite execution failure requires a live reserved resource set"
                            );
                        }
                    }
                    RecorderObservationSourceV1::Reconciliation => {
                        if reservation.status != "reconciliation_required" {
                            bail!("absent recorder evidence found an invalid reservation state");
                        }
                        if transaction.execute(
                            "UPDATE execass_technical_resource_reservations SET status='released',settled_at=?1 WHERE reservation_id=?2 AND status='reconciliation_required'",
                            params![observation.observed_at_ms, reservation.identity.reservation_id],
                        )? != 1
                        {
                            bail!("absent recorder evidence lost a reservation transition");
                        }
                    }
                }
            }
            transition_absent_attempt_and_effect(transaction, observation)?;
            if observation.source == RecorderObservationSourceV1::Reconciliation {
                close_recorder_continuation(
                    transaction,
                    command,
                    continuation,
                    &action,
                    ContinuationStatus::Superseded,
                )?;
            } else if continuation.status != ContinuationStatus::Executing
                || action.status != ContinuationStatus::Executing
            {
                bail!("definite execution failure requires a live executing continuation");
            }
        }
        RecorderEvidenceResult::Unknown => {
            for reservation in &reservations {
                if reservation.status == "reserved"
                    && transaction.execute(
                        "UPDATE execass_technical_resource_reservations SET status='reconciliation_required',settled_at=NULL WHERE reservation_id=?1 AND status='reserved'",
                        params![reservation.identity.reservation_id],
                    )? != 1
                {
                    bail!("unknown recorder evidence lost a reservation transition");
                } else if !matches!(reservation.status.as_str(), "reserved" | "reconciliation_required") {
                    bail!("unknown recorder evidence found a terminal reservation");
                }
            }
            keep_attempt_and_effect_unknown(transaction, observation)?;
            keep_continuation_uncertain(transaction, command, continuation, &action)?;
        }
    }
    if !(observation.source == RecorderObservationSourceV1::Execution
        && result == RecorderEvidenceResult::Absent)
    {
        assert_job_disabled(transaction, &command.claim_identity.job_id)?;
    }
    Ok(())
}

fn transition_present_attempt_and_effect(
    transaction: &Transaction<'_>,
    observation: &SignedRecorderObservationV1,
) -> Result<()> {
    let current: (String, String) = transaction.query_row(
        r#"SELECT attempt.status,effect.state
           FROM execass_provider_attempts attempt
           JOIN execass_logical_effects effect
             ON effect.logical_effect_id=attempt.logical_effect_id
           WHERE attempt.attempt_id=?1 AND effect.logical_effect_id=?2"#,
        params![observation.attempt_id, observation.logical_effect_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let next_state = if observation.source == RecorderObservationSourceV1::Execution
        && current == ("invoking".to_owned(), "invoking".to_owned())
    {
        "succeeded"
    } else {
        "reconciled_present"
    };
    transition_attempt_and_effect(transaction, observation, &current.0, &current.1, next_state)
}

fn transition_absent_attempt_and_effect(
    transaction: &Transaction<'_>,
    observation: &SignedRecorderObservationV1,
) -> Result<()> {
    let current: (String, String) = transaction.query_row(
        r#"SELECT attempt.status,effect.state
           FROM execass_provider_attempts attempt
           JOIN execass_logical_effects effect
             ON effect.logical_effect_id=attempt.logical_effect_id
           WHERE attempt.attempt_id=?1 AND effect.logical_effect_id=?2"#,
        params![observation.attempt_id, observation.logical_effect_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if current == ("invoking".to_owned(), "invoking".to_owned())
        && observation.source != RecorderObservationSourceV1::Execution
    {
        bail!("only execution evidence may settle an invoking effect as absent");
    }
    let next_state = match observation.source {
        RecorderObservationSourceV1::Execution => "failed",
        RecorderObservationSourceV1::Reconciliation => "reconciled_absent",
    };
    transition_attempt_and_effect(transaction, observation, &current.0, &current.1, next_state)
}

fn transition_attempt_and_effect(
    transaction: &Transaction<'_>,
    observation: &SignedRecorderObservationV1,
    required_attempt_state: &str,
    required_effect_state: &str,
    next_state: &str,
) -> Result<()> {
    if transaction.execute(
        "UPDATE execass_provider_attempts SET status=?1,provider_response_digest=?2,provider_error_class=?3,remote_effect_id=?4,finished_at=?5 WHERE attempt_id=?6 AND logical_effect_id=?7 AND status=?8",
        params![next_state, observation.response_digest, observation.provider_error_class.map(provider_failure_class_as_str), observation.remote_effect_id, observation.observed_at_ms, observation.attempt_id, observation.logical_effect_id, required_attempt_state],
    )? != 1
    {
        bail!("recorder evidence lost its provider-attempt transition");
    }
    if transaction.execute(
        "UPDATE execass_logical_effects SET state=?1,outcome_json=?2,updated_at=?3 WHERE logical_effect_id=?4 AND state=?5",
        params![
            next_state,
            canonical_effect_outcome(observation, next_state)?,
            observation.observed_at_ms,
            observation.logical_effect_id,
            required_effect_state,
        ],
    )? != 1
    {
        bail!("recorder evidence lost its logical-effect transition");
    }
    Ok(())
}

fn keep_attempt_and_effect_unknown(
    transaction: &Transaction<'_>,
    observation: &SignedRecorderObservationV1,
) -> Result<()> {
    let attempt_status: String = transaction.query_row(
        "SELECT status FROM execass_provider_attempts WHERE attempt_id=?1",
        params![observation.attempt_id],
        |row| row.get(0),
    )?;
    if attempt_status == "invoking"
        && transaction.execute(
            "UPDATE execass_provider_attempts SET status='outcome_unknown',provider_response_digest=?1,remote_effect_id=NULL,finished_at=?2 WHERE attempt_id=?3 AND status='invoking'",
            params![observation.response_digest, observation.observed_at_ms, observation.attempt_id],
        )? != 1
    {
        bail!("unknown recorder evidence lost its provider-attempt transition");
    } else if !matches!(attempt_status.as_str(), "invoking" | "outcome_unknown") {
        bail!("unknown recorder evidence found a terminal provider attempt");
    }
    let effect_state: String = transaction.query_row(
        "SELECT state FROM execass_logical_effects WHERE logical_effect_id=?1",
        params![observation.logical_effect_id],
        |row| row.get(0),
    )?;
    if effect_state == "invoking"
        && transaction.execute(
            "UPDATE execass_logical_effects SET state='outcome_unknown',outcome_json=?1,updated_at=?2 WHERE logical_effect_id=?3 AND state='invoking'",
            params![canonical_effect_outcome(observation, "outcome_unknown")?, observation.observed_at_ms, observation.logical_effect_id],
        )? != 1
    {
        bail!("unknown recorder evidence lost its logical-effect transition");
    } else if !matches!(effect_state.as_str(), "invoking" | "outcome_unknown") {
        bail!("unknown recorder evidence found a terminal logical effect");
    }
    Ok(())
}

fn close_recorder_continuation(
    transaction: &Transaction<'_>,
    command: &ReconcileRecorderEvidenceCommand,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
    status: ContinuationStatus,
) -> Result<()> {
    let required_status = match continuation.status {
        ContinuationStatus::Executing => {
            if action.status != ContinuationStatus::Executing {
                bail!("recorder execution evidence found a mismatched action state");
            }
            ContinuationStatus::Executing
        }
        ContinuationStatus::Uncertain => {
            if action.status != ContinuationStatus::Uncertain
                || continuation.lease_owner.is_some()
                || continuation.lease_expires_at.is_some()
            {
                bail!(
                    "recorder reconciliation evidence found a leased or mismatched uncertain state"
                );
            }
            ContinuationStatus::Uncertain
        }
        _ => bail!("recorder evidence found a terminal continuation"),
    };
    if transaction.execute(
        "UPDATE execass_continuations SET status=?1,lease_owner=NULL,lease_expires_at=NULL,updated_at=?2,completed_at=?2 WHERE continuation_id=?3 AND status=?4 AND fencing_token=?5",
        params![status.as_str(), command.write.occurred_at, continuation.continuation_id, required_status.as_str(), command.claim_identity.continuation_fencing_token],
    )? != 1 {
        bail!("recorder evidence lost its exact continuation transition");
    }
    update_action_status(transaction, action, status, command.write.occurred_at)?;
    transaction.execute(
        "UPDATE jobs SET enabled=0,next_run_at=NULL,lease_owner=NULL,lease_expires_at=NULL,updated_at=?1 WHERE job_id=?2 AND json_extract(payload_json,'$.mode')='execass.continuation'",
        params![command.write.occurred_at, command.claim_identity.job_id],
    )?;
    Ok(())
}

fn keep_continuation_uncertain(
    transaction: &Transaction<'_>,
    command: &ReconcileRecorderEvidenceCommand,
    continuation: &ContinuationRecord,
    action: &ActionBranchRecord,
) -> Result<()> {
    match continuation.status {
        ContinuationStatus::Executing => {
            if action.status != ContinuationStatus::Executing
                || transaction.execute(
                    "UPDATE execass_continuations SET status='uncertain',lease_owner=NULL,lease_expires_at=NULL,updated_at=?1,completed_at=NULL WHERE continuation_id=?2 AND status='executing' AND fencing_token=?3",
                    params![command.write.occurred_at, continuation.continuation_id, command.claim_identity.continuation_fencing_token],
                )? != 1
            {
                bail!("unknown recorder evidence lost its continuation transition");
            }
            update_action_status(
                transaction,
                action,
                ContinuationStatus::Uncertain,
                command.write.occurred_at,
            )?;
            transaction.execute(
                "UPDATE jobs SET enabled=0,next_run_at=NULL,lease_owner=NULL,lease_expires_at=NULL,updated_at=?1 WHERE job_id=?2 AND json_extract(payload_json,'$.mode')='execass.continuation'",
                params![command.write.occurred_at, command.claim_identity.job_id],
            )?;
        }
        ContinuationStatus::Uncertain => {
            if action.status != ContinuationStatus::Uncertain
                || continuation.lease_owner.is_some()
                || continuation.lease_expires_at.is_some()
            {
                bail!("unknown recorder evidence found divergent uncertain state");
            }
        }
        _ => bail!("unknown recorder evidence found a terminal continuation"),
    }
    Ok(())
}

fn assert_job_disabled(connection: &Connection, job_id: &str) -> Result<()> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM jobs WHERE job_id=?1 AND enabled=0 AND next_run_at IS NULL AND lease_owner IS NULL AND lease_expires_at IS NULL AND json_extract(payload_json,'$.mode')='execass.continuation'",
        params![job_id],
        |row| row.get(0),
    )?;
    if count != 1 {
        bail!("recorder evidence convergence found a live continuation job");
    }
    Ok(())
}

fn authoritative_claim_for_attempt(
    connection: &Connection,
    attempt_id: &str,
) -> Result<
    Option<(
        ContinuationClaimIdentity,
        Vec<TechnicalResourceReservationIdentity>,
    )>,
> {
    let claim_event_id = connection
        .query_row(
            "SELECT claim_event_id FROM execass_provider_attempts WHERE attempt_id=?1",
            params![attempt_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(claim_event_id) = claim_event_id else {
        return Ok(None);
    };
    let Some((identity, status, reservations)) =
        load_operation_history(connection, &claim_event_id, "claim")?
    else {
        return Ok(None);
    };
    if status != ContinuationStatus::Executing
        || resource_identity_set_digest(&reservations)?
            != identity.technical_resource_reservation_set_digest
    {
        return Ok(None);
    }
    Ok(Some((identity, reservations)))
}

fn receipt_command_matches_persisted(
    command: &AppendReceiptCommand,
    receipt: &ReceiptRecord,
) -> bool {
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&receipt.canonical_payload)
    else {
        return false;
    };
    let string = |pointer: &str| payload.pointer(pointer).and_then(serde_json::Value::as_str);
    let integer = |pointer: &str| payload.pointer(pointer).and_then(serde_json::Value::as_i64);
    let optional_string = |pointer: &str| match payload.pointer(pointer) {
        Some(serde_json::Value::Null) => Some(None),
        Some(serde_json::Value::String(value)) => Some(Some(value.as_str())),
        _ => None,
    };
    receipt.receipt_id == command.receipt_id
        && receipt.delegation_id.as_deref() == Some(command.delegation_id.as_str())
        && receipt.delegation_sequence == Some(command.expected_delegation_count + 1)
        && receipt.global_sequence == command.expected_global_count + 1
        && string("/receipt_id") == Some(command.receipt_id.as_str())
        && string("/transaction_id") == Some(command.transaction_id.as_str())
        && string("/receipt_kind") == Some(command.receipt_kind.as_str())
        && string("/delegation_id") == Some(command.delegation_id.as_str())
        && integer("/delegation_sequence") == receipt.delegation_sequence
        && integer("/global_sequence") == Some(receipt.global_sequence)
        && integer("/state_revision") == Some(command.expected_state_revision)
        && string("/event/event_id") == Some(command.causation_event_id.as_str())
        && string("/causation/kind") == Some(command.receipt_kind.as_str())
        && string("/causation/id") == Some(command.causation_id.as_str())
        && string("/subject/kind") == Some(command.subject.kind.as_str())
        && string("/subject/id") == Some(command.subject.subject_id.as_str())
        && integer("/subject/revision") == Some(command.subject.revision)
        && string("/actor/actor_type") == Some(command.actor.actor_type.as_str())
        && string("/actor/actor_identity") == Some(command.actor.actor_identity.as_str())
        && string("/actor/authority_provenance_id")
            == Some(command.actor.authority_provenance_id.as_str())
        && integer("/runtime/host_generation") == Some(command.runtime.host_generation)
        && string("/runtime/host_instance_id") == Some(command.runtime.host_instance_id.as_str())
        && integer("/runtime/fencing_token") == Some(command.runtime.fencing_token)
        && integer("/occurred_at_ms") == Some(command.occurred_at)
        && integer("/committed_at_ms") == Some(command.committed_at)
        && string("/redacted_summary") == Some(command.redacted_summary.as_str())
        && string("/key/key_id") == Some(command.key.key_id.as_str())
        && integer("/key/key_generation") == Some(command.key.key_generation)
        && integer("/state_root_generation") == Some(command.state_root_generation)
        && optional_string("/delegation_parent_digest")
            == Some(command.expected_delegation_head_digest.as_deref())
        && optional_string("/global_parent_digest")
            == Some(command.expected_global_head_digest.as_deref())
        && payload
            .pointer("/evidence_refs")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| items.is_empty() && command.evidence.is_empty())
        && payload.pointer("/rotation") == Some(&serde_json::Value::Null)
        && command.rotation.is_none()
}

fn inspect_replay(
    connection: &Connection,
    command: &ReconcileRecorderEvidenceCommand,
) -> Result<ImportMutationOutcome> {
    let observation = &command.verified_evidence.observation;
    let exact_ref: Option<String> = connection.query_row(
        "SELECT COUNT(*) FROM execass_effect_recorder_evidence evidence JOIN execass_receipt_recorder_evidence_refs ref ON ref.recorder_record_digest=evidence.recorder_record_digest WHERE evidence.recorder_record_digest=?1 AND evidence.signed_payload_json=?2 AND evidence.signature=?3 AND ref.receipt_id=?4 AND ref.delegation_id=?5",
        params![observation.record_digest, command.verified_evidence.signed_payload_json, observation.signature_hex, command.receipt.receipt_id, command.claim_identity.delegation_id],
        |row| row.get::<_, i64>(0),
    ).map(|count| (count == 1).then(|| command.receipt.receipt_id.clone()))?;
    let history: Option<(String, String)> = connection
        .query_row(
            "SELECT technical_resource_evidence_digest,result_status FROM execass_continuation_operation_history WHERE event_id=?1",
            params![command.outbox_event.event_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let expected_history = if observation.source == RecorderObservationSourceV1::Execution
        && command.verified_evidence.result == RecorderEvidenceResult::Absent
    {
        None
    } else {
        Some((
            observation.record_digest.clone(),
            result_status(command.verified_evidence.result)
                .as_str()
                .to_owned(),
        ))
    };
    let outbox = get_outbox(connection, &command.outbox_event.event_id)?;
    let authoritative_claim = authoritative_claim_for_attempt(connection, &observation.attempt_id)?;
    let receipt = exact_ref
        .as_deref()
        .map(|receipt_id| load_receipt(connection, receipt_id))
        .transpose()?
        .flatten();
    if exact_ref.is_none()
        || history != expected_history
        || outbox.as_ref().map(|record| &record.event) != Some(&command.outbox_event)
        || command.write.idempotency_key != command.outbox_event.duplicate_identity
        || command.write.correlation_id != command.outbox_event.correlation_id
        || command.write.causation_id != command.outbox_event.causation_id
        || command.write.occurred_at != command.outbox_event.occurred_at
        || authoritative_claim.as_ref().map(|(identity, _)| identity)
            != Some(&command.claim_identity)
        || receipt
            .as_ref()
            .is_none_or(|receipt| !receipt_command_matches_persisted(&command.receipt, receipt))
    {
        return Ok(ImportMutationOutcome::Conflict);
    }
    let (claim_identity, _) = authoritative_claim.context("recorder replay claim disappeared")?;
    let reservations = load_resource_reservations(connection, &claim_identity.claim_event_id)?;
    Ok(ImportMutationOutcome::Replayed(ImportDraft {
        claim_identity,
        result: command.verified_evidence.result,
        recorder_record_digest: observation.record_digest.clone(),
        outbox_event: outbox.context("recorder evidence replay outbox disappeared")?,
        reservations,
    }))
}

fn replay_import(
    transaction: &Transaction<'_>,
    command: &ReconcileRecorderEvidenceCommand,
) -> Result<AtomicReceiptMutation<ImportMutationOutcome>> {
    Ok(AtomicReceiptMutation::NoAppend(inspect_replay(
        transaction,
        command,
    )?))
}

fn active_runtime_identity(
    connection: &Connection,
    trusted_now: i64,
) -> Result<Option<RuntimeIdentity>> {
    let mut statement = connection.prepare(
        r#"SELECT generation.state_root_generation,generation.installation_identity,
                  generation.os_user_identity_digest
           FROM execass_runtime_host_leases lease
           JOIN execass_runtime_host_generations generation
             ON generation.generation=lease.generation
            AND generation.host_instance_id=lease.host_instance_id
           WHERE lease.ownership_scope='execass' AND generation.ownership_scope='execass'
             AND lease.released_at IS NULL AND lease.expires_at>?1
             AND generation.ended_at IS NULL
           ORDER BY generation.generation DESC,lease.fencing_token DESC LIMIT 2"#,
    )?;
    let rows = statement
        .query_map(params![trusted_now], |row| {
            Ok(RuntimeIdentity {
                state_root_generation: row.get(0)?,
                installation_identity: row.get(1)?,
                os_user_identity_digest: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 1 {
        bail!("multiple active runtime identities are not authoritative");
    }
    Ok(rows.into_iter().next())
}

fn load_active_recorder_key(connection: &Connection) -> Result<Option<ActiveRecorderKey>> {
    let mut statement = connection.prepare(
        r#"SELECT recorder_key_id,key_generation,verifying_key_hex,verifying_key_digest,
                  canonical_root_identity,installation_identity,os_user_identity_digest,
                  state_root_generation
           FROM execass_effect_recorder_keys WHERE status='active'
           ORDER BY key_generation,recorder_key_id LIMIT 2"#,
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(ActiveRecorderKey {
                recorder_key_id: row.get(0)?,
                key_generation: row.get(1)?,
                verifying_key_hex: row.get(2)?,
                verifying_key_digest: row.get(3)?,
                canonical_root_identity: row.get(4)?,
                installation_identity: row.get(5)?,
                os_user_identity_digest: row.get(6)?,
                state_root_generation: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 1 {
        bail!("multiple active recorder keys are not authoritative");
    }
    Ok(rows.into_iter().next())
}

fn load_attempt_facts(connection: &Connection, attempt_id: &str) -> Result<Option<AttemptFacts>> {
    connection
        .query_row(
            r#"SELECT attempt.logical_effect_id,attempt.status,attempt.provider_error_class,attempt.provider_request_digest,
                      attempt.started_at,effect.state,effect.provider_identity,
                      effect.provider_idempotency_key,effect.reconciliation_key
               FROM execass_provider_attempts attempt
               JOIN execass_logical_effects effect
                 ON effect.logical_effect_id=attempt.logical_effect_id
                AND effect.delegation_id=attempt.delegation_id
               WHERE attempt.attempt_id=?1
                 AND attempt.attempt_number=(
                   SELECT MAX(latest.attempt_number)
                   FROM execass_provider_attempts latest
                   WHERE latest.logical_effect_id=attempt.logical_effect_id
                 )"#,
            params![attempt_id],
            |row| {
                Ok(AttemptFacts {
                    logical_effect_id: row.get(0)?,
                    attempt_status: row.get(1)?,
                    provider_error_class: row.get(2)?,
                    provider_request_digest: row.get(3)?,
                    started_at: row.get(4)?,
                    effect_state: row.get(5)?,
                    provider_identity: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    provider_idempotency_key: row.get(7)?,
                    reconciliation_key: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn evidence_digest_exists(connection: &Connection, digest: &str) -> Result<bool> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM execass_effect_recorder_evidence WHERE recorder_record_digest=?1",
            params![digest],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn result_from_kind(
    kind: RecorderObservationKindV1,
) -> std::result::Result<RecorderEvidenceResult, RecorderEvidenceVerificationError> {
    match kind {
        RecorderObservationKindV1::Present => Ok(RecorderEvidenceResult::Present),
        RecorderObservationKindV1::Absent => Ok(RecorderEvidenceResult::Absent),
        RecorderObservationKindV1::Unknown => Ok(RecorderEvidenceResult::Unknown),
        RecorderObservationKindV1::Accepted | RecorderObservationKindV1::InvocationStarted => {
            Err(verification_error("journal_kind_not_importable"))
        }
    }
}

fn source_as_str(source: RecorderObservationSourceV1) -> &'static str {
    match source {
        RecorderObservationSourceV1::Execution => "execution",
        RecorderObservationSourceV1::Reconciliation => "reconciliation",
    }
}

fn provider_failure_class_as_str(class: ProviderFailureClassV1) -> &'static str {
    match class {
        ProviderFailureClassV1::Transient => "transient",
        ProviderFailureClassV1::RateLimited => "rate_limited",
        ProviderFailureClassV1::Authentication => "authentication",
        ProviderFailureClassV1::Permanent => "permanent",
        ProviderFailureClassV1::Unknown => "unknown",
    }
}

fn result_status(result: RecorderEvidenceResult) -> ContinuationStatus {
    match result {
        RecorderEvidenceResult::Present => ContinuationStatus::Terminal,
        RecorderEvidenceResult::Absent => ContinuationStatus::Superseded,
        RecorderEvidenceResult::Unknown => ContinuationStatus::Uncertain,
    }
}

fn canonical_import_payload(
    observation: &SignedRecorderObservationV1,
    result: RecorderEvidenceResult,
) -> Result<String> {
    Ok(serde_json::to_string(&serde_json::json!({
        "attempt_id": observation.attempt_id,
        "logical_effect_id": observation.logical_effect_id,
        "recorder_record_digest": observation.record_digest,
        "result": result.as_str(),
    }))?)
}

fn canonical_effect_outcome(
    observation: &SignedRecorderObservationV1,
    state: &str,
) -> Result<String> {
    Ok(serde_json::to_string(&serde_json::json!({
        "recorder_record_digest": observation.record_digest,
        "response_digest": observation.response_digest,
        "remote_effect_id": observation.remote_effect_id,
        "provider_error_class": observation.provider_error_class,
        "state": state,
    }))?)
}

fn technical_actual_id(record_digest: &str, reservation_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.recorder-evidence-actual.v1\0");
    digest.update(record_digest.as_bytes());
    digest.update([0]);
    digest.update(reservation_id.as_bytes());
    format!("technical-actual-{:x}", digest.finalize())
}

fn import_record(draft: ImportDraft, receipt: ReceiptRecord) -> RecorderEvidenceImportRecord {
    RecorderEvidenceImportRecord {
        claim_identity: draft.claim_identity,
        result: draft.result,
        recorder_record_digest: draft.recorder_record_digest,
        outbox_event: draft.outbox_event,
        receipt,
        technical_resource_reservations: draft.reservations,
    }
}

/// Test-only bridge for legacy fixtures whose assertion target begins after an
/// already-ambiguous provider attempt. It creates real Ed25519-signed execution
/// `Unknown` evidence and lets the production SQL guards authorize the exact
/// attempt/effect projection. No test signing authority exists in production.
#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
pub(crate) fn seed_signed_execution_unknown_fixture(
    connection: &Connection,
    attempt_id: &str,
    logical_effect_id: &str,
    observed_at: i64,
) -> Result<String> {
    let (
        installation_id,
        state_root_generation,
        os_user_identity_digest,
        provider_identity,
        provider_idempotency_key,
        reconciliation_key,
        provider_request_digest,
        started_at,
    ): (
        String,
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
    ) = connection.query_row(
        r#"SELECT generation.installation_identity,generation.state_root_generation,
                      generation.os_user_identity_digest,effect.provider_identity,
                      effect.provider_idempotency_key,effect.reconciliation_key,
                      attempt.provider_request_digest,attempt.started_at
               FROM execass_provider_attempts attempt
               JOIN execass_logical_effects effect
                 ON effect.logical_effect_id=attempt.logical_effect_id
               JOIN execass_runtime_host_generations generation
                 ON generation.generation=attempt.host_generation
                AND generation.host_instance_id=attempt.host_instance_id
               WHERE attempt.attempt_id=?1 AND attempt.logical_effect_id=?2
                 AND attempt.status='invoking' AND effect.state='invoking'"#,
        params![attempt_id, logical_effect_id],
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
            ))
        },
    )?;
    if observed_at < started_at {
        bail!("signed unknown fixture predates provider invocation");
    }
    let signer = SigningKey::from_bytes(&[23_u8; 32]);
    let verifying_key = signer.verifying_key().to_bytes();
    let key_id = format!("fixture-recorder-key-{attempt_id}");
    let canonical_root_identity = stable_key_digest("fixture-recorder-root");
    connection.execute(
        r#"INSERT INTO execass_effect_recorder_keys(
             recorder_key_id,key_generation,verifying_key_hex,verifying_key_digest,
             canonical_root_identity,installation_identity,os_user_identity_digest,
             state_root_generation,status,created_at
           ) VALUES(?1,1,?2,?3,?4,?5,?6,?7,'active',?8)"#,
        params![
            key_id,
            hex_encode(&verifying_key),
            hex_encode(&Sha256::digest(verifying_key)),
            canonical_root_identity,
            installation_id,
            os_user_identity_digest,
            state_root_generation,
            observed_at,
        ],
    )?;
    let response_digest = stable_key_digest(&format!("fixture-unknown-response:{attempt_id}"));
    let mut observation = SignedRecorderObservationV1 {
        sequence: 1,
        record_id: format!("fixture-recorder-record-{attempt_id}"),
        canonical_root_identity,
        installation_id,
        state_root_generation,
        os_user_identity_digest,
        attempt_id: attempt_id.to_owned(),
        logical_effect_id: logical_effect_id.to_owned(),
        command_digest: stable_key_digest(&format!("fixture-command:{attempt_id}")),
        kind: RecorderObservationKindV1::Unknown,
        source: RecorderObservationSourceV1::Execution,
        provider_identity,
        provider_version: "fixture-provider-v1".into(),
        provider_request_digest,
        provider_idempotency_key_digest: provider_idempotency_key.as_deref().map(stable_key_digest),
        reconciliation_key_digest: reconciliation_key.as_deref().map(stable_key_digest),
        remote_effect_id: None,
        response_digest: Some(response_digest.clone()),
        evidence_payload_digest: Some(stable_key_digest(&format!(
            "fixture-unknown-evidence:{attempt_id}"
        ))),
        provider_error_class: None,
        technical_resource_actuals: Vec::new(),
        reconciliation_window_start_ms: None,
        reconciliation_window_end_ms: None,
        observed_at_ms: observed_at,
        previous_record_digest: stable_key_digest(&format!("fixture-prior:{attempt_id}")),
        record_digest: stable_key_digest(&format!("fixture-record:{attempt_id}")),
        recorder_key_id: key_id,
        recorder_key_generation: 1,
        signature_hex: String::new(),
    };
    let signed_payload = recorder_observation_signing_bytes(&observation)?;
    observation.signature_hex = hex_encode(&signer.sign(&signed_payload).to_bytes());
    let signed_payload_json = String::from_utf8(recorder_observation_signing_bytes(&observation)?)?;
    connection.execute(
        r#"INSERT INTO execass_effect_recorder_evidence(
             recorder_record_digest,record_id,recorder_key_id,key_generation,
             canonical_root_identity,installation_identity,state_root_generation,
             os_user_identity_digest,journal_sequence,journal_kind,journal_source,
             attempt_id,logical_effect_id,command_digest,provider_identity,provider_version,
             provider_request_digest,provider_idempotency_key_digest,reconciliation_key_digest,
             remote_effect_id,response_digest,evidence_payload_digest,
             technical_resource_actuals_json,reconciliation_window_start,
             reconciliation_window_end,observed_at,imported_at,previous_record_digest,
             signed_payload_json,signature
           ) VALUES(?1,?2,?3,1,?4,?5,?6,?7,1,'unknown','execution',?8,?9,?10,
             ?11,?12,?13,?14,?15,NULL,?16,?17,'[]',NULL,NULL,?18,?18,?19,?20,?21)"#,
        params![
            observation.record_digest,
            observation.record_id,
            observation.recorder_key_id,
            observation.canonical_root_identity,
            observation.installation_id,
            observation.state_root_generation,
            observation.os_user_identity_digest,
            observation.attempt_id,
            observation.logical_effect_id,
            observation.command_digest,
            observation.provider_identity,
            observation.provider_version,
            observation.provider_request_digest,
            observation.provider_idempotency_key_digest,
            observation.reconciliation_key_digest,
            observation.response_digest,
            observation.evidence_payload_digest,
            observation.observed_at_ms,
            observation.previous_record_digest,
            signed_payload_json,
            observation.signature_hex,
        ],
    )?;
    connection.execute(
        "UPDATE execass_provider_attempts SET status='outcome_unknown',provider_response_digest=?1,remote_effect_id=NULL,finished_at=?2 WHERE attempt_id=?3 AND status='invoking'",
        params![response_digest, observed_at, attempt_id],
    )?;
    let outcome_json = canonical_effect_outcome(&observation, "outcome_unknown")?;
    connection.execute(
        "UPDATE execass_logical_effects SET state='outcome_unknown',outcome_json=?1,updated_at=?2 WHERE logical_effect_id=?3 AND state='invoking'",
        params![outcome_json, observed_at, logical_effect_id],
    )?;
    Ok(observation.record_digest)
}

fn stable_key_digest(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn require_root_digest(value: &str) -> Result<()> {
    if !valid_digest(value) {
        bail!("canonical root identity is not a SHA-256 digest");
    }
    Ok(())
}

fn require_text(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        bail!("{name} is invalid");
    }
    Ok(())
}

fn require_hex(name: &str, value: &str, length: usize) -> Result<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{name} is not canonical lowercase hex");
    }
    Ok(())
}

fn require_hex_verification(
    value: &str,
    length: usize,
    code: &'static str,
) -> std::result::Result<(), RecorderEvidenceVerificationError> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(verification_error(code));
    }
    Ok(())
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N]> {
    if value.len() != N * 2 {
        bail!("hex value has wrong length");
    }
    let mut bytes = [0_u8; N];
    for (index, output) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *output = u8::from_str_radix(&value[offset..offset + 2], 16)?;
    }
    Ok(bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn verification_error(code: &'static str) -> RecorderEvidenceVerificationError {
    RecorderEvidenceVerificationError::new(code)
}
