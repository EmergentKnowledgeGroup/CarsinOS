use crate::custody::{RecorderCredential, RecorderIdentity};
use crate::hex_encode;
use crate::state_verifier::VerifiedExecuteOnceAdmission;
use carsinos_protocol::execass_recorder::{
    canonical_json_bytes, recorder_observation_signing_bytes, ExecuteOnceV1,
    ProviderFailureClassV1, RecorderBindingV1, RecorderHandshakeAttestationV1,
    RecorderHandshakeChallengeV1, RecorderObservationKindV1, RecorderObservationSourceV1,
    SignedRecorderObservationV1, TechnicalResourceActualV1, RECORDER_HANDSHAKE_VERSION,
};
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const JOURNAL_VERSION: &str = "carsinos.execass.effect-recorder.journal.v1";
const MAX_RECORD_BYTES: usize = 256 * 1024;
const ZERO_DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq)]
enum DurabilityStep {
    DirectoryCreated(PathBuf),
    DirectorySynced(PathBuf),
    FileSynced(PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("journal record encoding failed: {0}")]
    Encoding(String),
    #[error("journal is corrupt: {0}")]
    Corrupt(&'static str),
    #[error("attempt already exists with a different command digest")]
    CommandConflict,
    #[error("attempt has no accepted journal record")]
    NotAccepted,
    #[error("attempt already has an invocation-started record")]
    AlreadyStarted,
    #[error("attempt already has a terminal record")]
    AlreadyTerminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalRecordV1 {
    pub journal_version: String,
    pub sequence: u64,
    pub record_id: String,
    pub canonical_root_identity: String,
    pub installation_id: String,
    pub state_root_generation: i64,
    pub os_user_identity_digest: String,
    pub attempt_id: String,
    pub logical_effect_id: String,
    pub command_digest: String,
    pub kind: RecorderObservationKindV1,
    pub source: RecorderObservationSourceV1,
    pub provider_identity: String,
    pub provider_version: String,
    pub provider_request_digest: String,
    pub provider_idempotency_key_digest: Option<String>,
    pub reconciliation_key_digest: Option<String>,
    pub remote_effect_id: Option<String>,
    pub response_digest: Option<String>,
    pub evidence_payload_digest: Option<String>,
    pub provider_error_class: Option<ProviderFailureClassV1>,
    pub technical_resource_actuals: Vec<TechnicalResourceActualV1>,
    pub reconciliation_window_start_ms: Option<i64>,
    pub reconciliation_window_end_ms: Option<i64>,
    pub observed_at_ms: i64,
    pub previous_record_digest: String,
    pub record_digest: String,
    pub recorder_key_id: String,
    pub recorder_key_generation: u64,
    pub signature_hex: String,
}

#[derive(Serialize)]
struct UnsignedRecord<'a> {
    journal_version: &'a str,
    sequence: u64,
    record_id: &'a str,
    canonical_root_identity: &'a str,
    installation_id: &'a str,
    state_root_generation: i64,
    os_user_identity_digest: &'a str,
    attempt_id: &'a str,
    logical_effect_id: &'a str,
    command_digest: &'a str,
    kind: RecorderObservationKindV1,
    source: RecorderObservationSourceV1,
    provider_identity: &'a str,
    provider_version: &'a str,
    provider_request_digest: &'a str,
    provider_idempotency_key_digest: Option<&'a str>,
    reconciliation_key_digest: Option<&'a str>,
    remote_effect_id: Option<&'a str>,
    response_digest: Option<&'a str>,
    evidence_payload_digest: Option<&'a str>,
    provider_error_class: Option<ProviderFailureClassV1>,
    technical_resource_actuals: &'a [TechnicalResourceActualV1],
    reconciliation_window_start_ms: Option<i64>,
    reconciliation_window_end_ms: Option<i64>,
    observed_at_ms: i64,
    previous_record_digest: &'a str,
    recorder_key_id: &'a str,
    recorder_key_generation: u64,
}

pub struct Journal {
    root: PathBuf,
    file: File,
    _lock: File,
    records: Vec<JournalRecordV1>,
    by_attempt: HashMap<(String, i64, String), Vec<usize>>,
    signing_key: ed25519_dalek::SigningKey,
    identity: RecorderIdentity,
}

pub(crate) enum JournalAdmissionOutcome {
    FreshStarted(Box<FreshInvocationPermit>),
    Replay(Box<JournalRecordV1>),
}

pub(crate) enum JournalExecutionState {
    RequiresLiveVerification,
    Replay(Box<JournalRecordV1>),
}

pub(crate) struct FreshInvocationPermit {
    admission: VerifiedExecuteOnceAdmission,
}

impl FreshInvocationPermit {
    pub(crate) fn into_admission(self) -> VerifiedExecuteOnceAdmission {
        self.admission
    }
}

impl std::fmt::Debug for Journal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Journal")
            .field("root", &self.root)
            .field("record_count", &self.records.len())
            .field("identity", &self.identity)
            .finish()
    }
}

impl Journal {
    pub(crate) fn open(
        state_root: &Path,
        credential: &RecorderCredential,
    ) -> Result<Self, JournalError> {
        Self::open_inner(state_root, credential, |_| {})
    }

    fn open_inner(
        state_root: &Path,
        credential: &RecorderCredential,
        mut observe: impl FnMut(DurabilityStep),
    ) -> Result<Self, JournalError> {
        let root = state_root.join("runtime/effect-recorder/v1");
        durable_create_dir_all(&root, &mut observe)?;
        let lock_path = root.join("lock");
        let lock = durable_open_file(&lock_path, &root, &mut observe)?;
        lock.try_lock_exclusive().map_err(JournalError::Io)?;
        let journal_path = root.join("journal.v1");
        let file = durable_open_file(&journal_path, &root, &mut observe)?;
        let mut journal = Self {
            root,
            file,
            _lock: lock,
            records: Vec::new(),
            by_attempt: HashMap::new(),
            signing_key: credential.signing_key(),
            identity: credential.identity().clone(),
        };
        journal.scan_and_recover_torn_tail()?;
        Ok(journal)
    }

    pub fn command_digest(command: &ExecuteOnceV1) -> Result<String, JournalError> {
        let mut command = command.clone();
        // Transport freshness proves request authentication, but it is not part
        // of the durable identity of an already-committed provider attempt.
        command.request_id.clear();
        command.client_nonce.clear();
        command.deadline_ms = 0;
        command.command_mac.clear();
        let bytes = canonical_json_bytes(&command)
            .map_err(|error| JournalError::Encoding(error.to_string()))?;
        Ok(format!("sha256:{}", hex_encode(&Sha256::digest(bytes))))
    }

    pub(crate) fn sign_handshake(
        &self,
        challenge: &RecorderHandshakeChallengeV1,
        binding: RecorderBindingV1,
        server_nonce: String,
    ) -> Result<RecorderHandshakeAttestationV1, JournalError> {
        let mut attestation = RecorderHandshakeAttestationV1 {
            handshake_version: RECORDER_HANDSHAKE_VERSION.into(),
            binding,
            client_nonce: challenge.client_nonce.clone(),
            request_authentication_digest: challenge.request_authentication_digest.clone(),
            server_nonce,
            recorder_key_id: self.identity.key_id.clone(),
            recorder_key_generation: self.identity.key_generation,
            recorder_verifying_key_hex: self.identity.verifying_key_hex.clone(),
            signature_hex: String::new(),
        };
        let bytes = attestation
            .signing_bytes()
            .map_err(|error| JournalError::Encoding(error.to_string()))?;
        attestation.signature_hex = hex_encode(&self.signing_key.sign(&bytes).to_bytes());
        Ok(attestation)
    }

    pub fn accept(
        &mut self,
        command: &ExecuteOnceV1,
        observed_at_ms: i64,
    ) -> Result<(bool, JournalRecordV1), JournalError> {
        let digest = Self::command_digest(command)?;
        if let Some(existing) = self.latest_for(command, &digest)? {
            return Ok((true, existing.clone()));
        }
        let record = self.new_record(
            command,
            &digest,
            RecorderObservationKindV1::Accepted,
            RecorderObservationSourceV1::Execution,
            None,
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            observed_at_ms,
        )?;
        self.append(record.clone())?;
        Ok((false, record))
    }

    pub(crate) fn admit_verified_execution(
        &mut self,
        admission: VerifiedExecuteOnceAdmission,
        accepted_at_ms: i64,
        started_at_ms: i64,
        after_accepted: impl FnOnce(),
    ) -> Result<JournalAdmissionOutcome, JournalError> {
        let command = admission.command();
        let digest = Self::command_digest(command)?;
        if let Some(existing) = self.latest_for(command, &digest)? {
            return Ok(JournalAdmissionOutcome::Replay(Box::new(existing.clone())));
        }
        self.accept(command, accepted_at_ms)?;
        after_accepted();
        self.mark_invocation_started(command, started_at_ms)?;
        Ok(JournalAdmissionOutcome::FreshStarted(Box::new(
            FreshInvocationPermit { admission },
        )))
    }

    pub(crate) fn execution_state(
        &self,
        command: &ExecuteOnceV1,
    ) -> Result<JournalExecutionState, JournalError> {
        let digest = Self::command_digest(command)?;
        match self.latest_for(command, &digest)? {
            None => Ok(JournalExecutionState::RequiresLiveVerification),
            Some(record) => Ok(JournalExecutionState::Replay(Box::new(record.clone()))),
        }
    }

    pub fn mark_invocation_started(
        &mut self,
        command: &ExecuteOnceV1,
        observed_at_ms: i64,
    ) -> Result<JournalRecordV1, JournalError> {
        let digest = Self::command_digest(command)?;
        let records = self.records_for(command, &digest)?;
        if records.is_empty() {
            return Err(JournalError::NotAccepted);
        }
        if records.iter().any(|record| {
            matches!(
                record.kind,
                RecorderObservationKindV1::InvocationStarted
                    | RecorderObservationKindV1::Present
                    | RecorderObservationKindV1::Absent
                    | RecorderObservationKindV1::Unknown
            )
        }) {
            return Err(JournalError::AlreadyStarted);
        }
        let record = self.new_record(
            command,
            &digest,
            RecorderObservationKindV1::InvocationStarted,
            RecorderObservationSourceV1::Execution,
            None,
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            observed_at_ms,
        )?;
        self.append(record.clone())?;
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_terminal(
        &mut self,
        command: &ExecuteOnceV1,
        kind: RecorderObservationKindV1,
        response_digest: String,
        evidence_payload_digest: String,
        remote_effect_id: Option<String>,
        provider_error_class: Option<ProviderFailureClassV1>,
        technical_resource_actuals: Vec<TechnicalResourceActualV1>,
        observed_at_ms: i64,
    ) -> Result<JournalRecordV1, JournalError> {
        if !matches!(
            kind,
            RecorderObservationKindV1::Present
                | RecorderObservationKindV1::Absent
                | RecorderObservationKindV1::Unknown
        ) {
            return Err(JournalError::Encoding(
                "terminal kind is not terminal".into(),
            ));
        }
        let shape = SignedRecorderObservationV1 {
            sequence: 0,
            record_id: String::new(),
            canonical_root_identity: String::new(),
            installation_id: String::new(),
            state_root_generation: 0,
            os_user_identity_digest: String::new(),
            attempt_id: String::new(),
            logical_effect_id: String::new(),
            command_digest: String::new(),
            kind,
            source: RecorderObservationSourceV1::Execution,
            provider_identity: String::new(),
            provider_version: String::new(),
            provider_request_digest: String::new(),
            provider_idempotency_key_digest: None,
            reconciliation_key_digest: None,
            remote_effect_id: None,
            response_digest: None,
            evidence_payload_digest: None,
            provider_error_class,
            technical_resource_actuals: Vec::new(),
            reconciliation_window_start_ms: None,
            reconciliation_window_end_ms: None,
            observed_at_ms: 0,
            previous_record_digest: String::new(),
            record_digest: String::new(),
            recorder_key_id: String::new(),
            recorder_key_generation: 0,
            signature_hex: String::new(),
        };
        shape
            .validate_shape()
            .map_err(|error| JournalError::Encoding(error.to_string()))?;
        let digest = Self::command_digest(command)?;
        let records = self.records_for(command, &digest)?;
        if !records
            .iter()
            .any(|record| record.kind == RecorderObservationKindV1::InvocationStarted)
        {
            return Err(JournalError::NotAccepted);
        }
        if records.iter().any(|record| {
            matches!(
                record.kind,
                RecorderObservationKindV1::Present
                    | RecorderObservationKindV1::Absent
                    | RecorderObservationKindV1::Unknown
            )
        }) {
            return Err(JournalError::AlreadyTerminal);
        }
        let record = self.new_record(
            command,
            &digest,
            kind,
            RecorderObservationSourceV1::Execution,
            Some(response_digest),
            remote_effect_id,
            Some(evidence_payload_digest),
            provider_error_class,
            technical_resource_actuals,
            None,
            None,
            observed_at_ms,
        )?;
        self.append(record.clone())?;
        Ok(record)
    }

    pub fn query(
        &self,
        installation_id: &str,
        state_root_generation: i64,
        attempt_id: &str,
        expected_command_digest: Option<&str>,
    ) -> Result<Option<JournalRecordV1>, JournalError> {
        let key = (
            installation_id.to_owned(),
            state_root_generation,
            attempt_id.to_owned(),
        );
        let Some(indices) = self.by_attempt.get(&key) else {
            return Ok(None);
        };
        let first = &self.records[indices[0]];
        if expected_command_digest.is_some_and(|expected| expected != first.command_digest) {
            return Err(JournalError::CommandConflict);
        }
        Ok(indices.last().map(|index| self.records[*index].clone()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_reconciliation(
        &mut self,
        installation_id: &str,
        state_root_generation: i64,
        attempt_id: &str,
        expected_command_digest: &str,
        kind: RecorderObservationKindV1,
        response_digest: String,
        evidence_payload_digest: String,
        remote_effect_id: Option<String>,
        technical_resource_actuals: Vec<TechnicalResourceActualV1>,
        window_start_ms: i64,
        window_end_ms: i64,
        observed_at_ms: i64,
    ) -> Result<JournalRecordV1, JournalError> {
        if !matches!(
            kind,
            RecorderObservationKindV1::Present
                | RecorderObservationKindV1::Absent
                | RecorderObservationKindV1::Unknown
        ) {
            return Err(JournalError::Encoding(
                "reconciliation result is not an evidence state".into(),
            ));
        }
        let base = self
            .query(
                installation_id,
                state_root_generation,
                attempt_id,
                Some(expected_command_digest),
            )?
            .ok_or(JournalError::NotAccepted)?;
        if matches!(
            base.kind,
            RecorderObservationKindV1::Present | RecorderObservationKindV1::Absent
        ) {
            return Err(JournalError::AlreadyTerminal);
        }
        let sequence = self.records.len() as u64 + 1;
        let previous_record_digest = self.head_digest().to_owned();
        let record_id = format!(
            "recorder-record-{}",
            hex_encode(&Sha256::digest(
                format!(
                    "{}\0{}\0{sequence}\0reconcile",
                    base.attempt_id, base.command_digest
                )
                .as_bytes()
            ))
        );
        let mut record = JournalRecordV1 {
            journal_version: JOURNAL_VERSION.into(),
            sequence,
            record_id,
            canonical_root_identity: base.canonical_root_identity,
            installation_id: base.installation_id,
            state_root_generation: base.state_root_generation,
            os_user_identity_digest: base.os_user_identity_digest,
            attempt_id: base.attempt_id,
            logical_effect_id: base.logical_effect_id,
            command_digest: base.command_digest,
            kind,
            source: RecorderObservationSourceV1::Reconciliation,
            provider_identity: base.provider_identity,
            provider_version: base.provider_version,
            provider_request_digest: base.provider_request_digest,
            provider_idempotency_key_digest: base.provider_idempotency_key_digest,
            reconciliation_key_digest: base.reconciliation_key_digest,
            remote_effect_id,
            response_digest: Some(response_digest),
            evidence_payload_digest: Some(evidence_payload_digest),
            provider_error_class: None,
            technical_resource_actuals,
            reconciliation_window_start_ms: Some(window_start_ms),
            reconciliation_window_end_ms: Some(window_end_ms),
            observed_at_ms,
            previous_record_digest,
            record_digest: String::new(),
            recorder_key_id: self.identity.key_id.clone(),
            recorder_key_generation: self.identity.key_generation,
            signature_hex: String::new(),
        };
        let bytes = unsigned_bytes(&record)?;
        record.record_digest = format!("sha256:{}", hex_encode(&Sha256::digest(&bytes)));
        record.signature_hex =
            hex_encode(&self.signing_key.sign(&signed_bytes(&record)?).to_bytes());
        self.append(record.clone())?;
        Ok(record)
    }

    pub fn head_digest(&self) -> &str {
        self.records
            .last()
            .map(|record| record.record_digest.as_str())
            .unwrap_or(ZERO_DIGEST)
    }

    fn latest_for<'a>(
        &'a self,
        command: &ExecuteOnceV1,
        digest: &str,
    ) -> Result<Option<&'a JournalRecordV1>, JournalError> {
        Ok(self.records_for(command, digest)?.last().copied())
    }

    fn records_for<'a>(
        &'a self,
        command: &ExecuteOnceV1,
        digest: &str,
    ) -> Result<Vec<&'a JournalRecordV1>, JournalError> {
        let key = (
            command.binding.installation_id.clone(),
            command.binding.state_root_generation,
            command.attempt_id.clone(),
        );
        let Some(indices) = self.by_attempt.get(&key) else {
            return Ok(Vec::new());
        };
        let records = indices
            .iter()
            .map(|index| &self.records[*index])
            .collect::<Vec<_>>();
        if records
            .first()
            .is_some_and(|record| record.command_digest != digest)
        {
            return Err(JournalError::CommandConflict);
        }
        Ok(records)
    }

    #[allow(clippy::too_many_arguments)]
    fn new_record(
        &self,
        command: &ExecuteOnceV1,
        command_digest: &str,
        kind: RecorderObservationKindV1,
        source: RecorderObservationSourceV1,
        response_digest: Option<String>,
        remote_effect_id: Option<String>,
        evidence_payload_digest: Option<String>,
        provider_error_class: Option<ProviderFailureClassV1>,
        technical_resource_actuals: Vec<TechnicalResourceActualV1>,
        reconciliation_window_start_ms: Option<i64>,
        reconciliation_window_end_ms: Option<i64>,
        observed_at_ms: i64,
    ) -> Result<JournalRecordV1, JournalError> {
        let sequence = self.records.len() as u64 + 1;
        let previous_record_digest = self.head_digest().to_owned();
        let record_id = format!(
            "recorder-record-{}",
            hex_encode(&Sha256::digest(
                format!("{}\0{}\0{sequence}", command.attempt_id, command_digest).as_bytes()
            ))
        );
        let mut record = JournalRecordV1 {
            journal_version: JOURNAL_VERSION.into(),
            sequence,
            record_id,
            canonical_root_identity: command.binding.canonical_root_identity.clone(),
            installation_id: command.binding.installation_id.clone(),
            state_root_generation: command.binding.state_root_generation,
            os_user_identity_digest: command.binding.os_user_identity_digest.clone(),
            attempt_id: command.attempt_id.clone(),
            logical_effect_id: command.logical_effect_id.clone(),
            command_digest: command_digest.to_owned(),
            kind,
            source,
            provider_identity: command.provider_identity.clone(),
            provider_version: command.provider_version.clone(),
            provider_request_digest: command.provider_request_digest.clone(),
            provider_idempotency_key_digest: command
                .provider_idempotency_key
                .as_deref()
                .map(stable_key_digest),
            reconciliation_key_digest: command.reconciliation_key.as_deref().map(stable_key_digest),
            remote_effect_id,
            response_digest,
            evidence_payload_digest,
            provider_error_class,
            technical_resource_actuals,
            reconciliation_window_start_ms,
            reconciliation_window_end_ms,
            observed_at_ms,
            previous_record_digest,
            record_digest: String::new(),
            recorder_key_id: self.identity.key_id.clone(),
            recorder_key_generation: self.identity.key_generation,
            signature_hex: String::new(),
        };
        let bytes = unsigned_bytes(&record)?;
        record.record_digest = format!("sha256:{}", hex_encode(&Sha256::digest(&bytes)));
        let signature = self.signing_key.sign(&signed_bytes(&record)?);
        record.signature_hex = hex_encode(&signature.to_bytes());
        Ok(record)
    }

    fn append(&mut self, record: JournalRecordV1) -> Result<(), JournalError> {
        let payload = serde_json::to_vec(&record)
            .map_err(|error| JournalError::Encoding(error.to_string()))?;
        if payload.len() > MAX_RECORD_BYTES {
            return Err(JournalError::Encoding("journal record is too large".into()));
        }
        let checksum = Sha256::digest(&payload);
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&(payload.len() as u32).to_be_bytes())?;
        self.file.write_all(&payload)?;
        self.file.write_all(&checksum)?;
        self.file.sync_data()?;
        self.index_record(record)?;
        Ok(())
    }

    fn scan_and_recover_torn_tail(&mut self) -> Result<(), JournalError> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let valid_start = offset;
            if bytes.len() - offset < 4 {
                self.file.set_len(valid_start as u64)?;
                self.file.sync_data()?;
                break;
            }
            let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            if length > MAX_RECORD_BYTES {
                return Err(JournalError::Corrupt("record length exceeds limit"));
            }
            let required = length
                .checked_add(32)
                .ok_or(JournalError::Corrupt("record length overflow"))?;
            if bytes.len() - offset < required {
                self.file.set_len(valid_start as u64)?;
                self.file.sync_data()?;
                break;
            }
            let payload = &bytes[offset..offset + length];
            offset += length;
            let checksum = &bytes[offset..offset + 32];
            offset += 32;
            if Sha256::digest(payload).as_slice() != checksum {
                return Err(JournalError::Corrupt("record checksum mismatch"));
            }
            let record: JournalRecordV1 = serde_json::from_slice(payload)
                .map_err(|_| JournalError::Corrupt("record JSON is invalid"))?;
            self.verify_record(&record)?;
            self.index_record(record)?;
        }
        self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }

    fn verify_record(&self, record: &JournalRecordV1) -> Result<(), JournalError> {
        if record.journal_version != JOURNAL_VERSION
            || record.sequence != self.records.len() as u64 + 1
            || record.previous_record_digest != self.head_digest()
            || record.recorder_key_id != self.identity.key_id
            || record.recorder_key_generation != self.identity.key_generation
        {
            return Err(JournalError::Corrupt("record identity or chain mismatch"));
        }
        let expected = format!(
            "sha256:{}",
            hex_encode(&Sha256::digest(unsigned_bytes(record)?))
        );
        if record.record_digest != expected {
            return Err(JournalError::Corrupt("record digest mismatch"));
        }
        SignedRecorderObservationV1::from(record.clone())
            .validate_shape()
            .map_err(|_| JournalError::Corrupt("record failure-class shape is invalid"))?;
        let signature_bytes = crate::hex_decode::<64>(&record.signature_hex)
            .map_err(|_| JournalError::Corrupt("signature encoding is invalid"))?;
        let signature = Signature::from_bytes(&signature_bytes);
        let verifying_bytes = crate::hex_decode::<32>(&self.identity.verifying_key_hex)
            .map_err(|_| JournalError::Corrupt("verifying key encoding is invalid"))?;
        let verifying = VerifyingKey::from_bytes(&verifying_bytes)
            .map_err(|_| JournalError::Corrupt("verifying key is invalid"))?;
        verifying
            .verify(&signed_bytes(record)?, &signature)
            .map_err(|_| JournalError::Corrupt("record signature is invalid"))
    }

    fn index_record(&mut self, record: JournalRecordV1) -> Result<(), JournalError> {
        let key = (
            record.installation_id.clone(),
            record.state_root_generation,
            record.attempt_id.clone(),
        );
        if let Some(indices) = self.by_attempt.get(&key) {
            if self.records[indices[0]].command_digest != record.command_digest {
                return Err(JournalError::CommandConflict);
            }
        } else if record.kind != RecorderObservationKindV1::Accepted {
            return Err(JournalError::Corrupt(
                "attempt chain does not begin with Accepted",
            ));
        }
        let index = self.records.len();
        self.records.push(record);
        self.by_attempt.entry(key).or_default().push(index);
        Ok(())
    }
}

fn durable_create_dir_all(
    target: &Path,
    observe: &mut impl FnMut(DurabilityStep),
) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut cursor = target;
    while !cursor.exists() {
        missing.push(cursor.to_path_buf());
        cursor = cursor.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "recorder directory has no existing parent",
            )
        })?;
    }
    if !cursor.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "recorder directory ancestor is not a directory",
        ));
    }
    for directory in missing.into_iter().rev() {
        fs::create_dir(&directory)?;
        observe(DurabilityStep::DirectoryCreated(directory.clone()));
        let parent = directory.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "created recorder directory has no parent",
            )
        })?;
        sync_directory(parent)?;
        observe(DurabilityStep::DirectorySynced(parent.to_path_buf()));
    }
    Ok(())
}

// These calls fail closed unless the operating system accepts the requested
// file/directory flush. They establish an OS durability boundary; they cannot
// certify that physical storage firmware survives a real power loss.
fn durable_open_file(
    path: &Path,
    containing_directory: &Path,
    observe: &mut impl FnMut(DurabilityStep),
) -> std::io::Result<File> {
    let existed = path.exists();
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    if !existed {
        file.sync_all()?;
        observe(DurabilityStep::FileSynced(path.to_path_buf()));
        sync_directory(containing_directory)?;
        observe(DurabilityStep::DirectorySynced(
            containing_directory.to_path_buf(),
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

    OpenOptions::new()
        .access_mode(GENERIC_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

impl From<JournalRecordV1> for SignedRecorderObservationV1 {
    fn from(record: JournalRecordV1) -> Self {
        Self {
            sequence: record.sequence,
            record_id: record.record_id,
            canonical_root_identity: record.canonical_root_identity,
            installation_id: record.installation_id,
            state_root_generation: record.state_root_generation,
            os_user_identity_digest: record.os_user_identity_digest,
            attempt_id: record.attempt_id,
            logical_effect_id: record.logical_effect_id,
            command_digest: record.command_digest,
            kind: record.kind,
            source: record.source,
            provider_identity: record.provider_identity,
            provider_version: record.provider_version,
            provider_request_digest: record.provider_request_digest,
            provider_idempotency_key_digest: record.provider_idempotency_key_digest,
            reconciliation_key_digest: record.reconciliation_key_digest,
            remote_effect_id: record.remote_effect_id,
            response_digest: record.response_digest,
            evidence_payload_digest: record.evidence_payload_digest,
            provider_error_class: record.provider_error_class,
            technical_resource_actuals: record.technical_resource_actuals,
            reconciliation_window_start_ms: record.reconciliation_window_start_ms,
            reconciliation_window_end_ms: record.reconciliation_window_end_ms,
            observed_at_ms: record.observed_at_ms,
            previous_record_digest: record.previous_record_digest,
            record_digest: record.record_digest,
            recorder_key_id: record.recorder_key_id,
            recorder_key_generation: record.recorder_key_generation,
            signature_hex: record.signature_hex,
        }
    }
}

fn unsigned_bytes(record: &JournalRecordV1) -> Result<Vec<u8>, JournalError> {
    canonical_json_bytes(&UnsignedRecord {
        journal_version: &record.journal_version,
        sequence: record.sequence,
        record_id: &record.record_id,
        canonical_root_identity: &record.canonical_root_identity,
        installation_id: &record.installation_id,
        state_root_generation: record.state_root_generation,
        os_user_identity_digest: &record.os_user_identity_digest,
        attempt_id: &record.attempt_id,
        logical_effect_id: &record.logical_effect_id,
        command_digest: &record.command_digest,
        kind: record.kind,
        source: record.source,
        provider_identity: &record.provider_identity,
        provider_version: &record.provider_version,
        provider_request_digest: &record.provider_request_digest,
        provider_idempotency_key_digest: record.provider_idempotency_key_digest.as_deref(),
        reconciliation_key_digest: record.reconciliation_key_digest.as_deref(),
        remote_effect_id: record.remote_effect_id.as_deref(),
        response_digest: record.response_digest.as_deref(),
        evidence_payload_digest: record.evidence_payload_digest.as_deref(),
        provider_error_class: record.provider_error_class,
        technical_resource_actuals: &record.technical_resource_actuals,
        reconciliation_window_start_ms: record.reconciliation_window_start_ms,
        reconciliation_window_end_ms: record.reconciliation_window_end_ms,
        observed_at_ms: record.observed_at_ms,
        previous_record_digest: &record.previous_record_digest,
        recorder_key_id: &record.recorder_key_id,
        recorder_key_generation: record.recorder_key_generation,
    })
    .map_err(|error| JournalError::Encoding(error.to_string()))
}

pub fn stable_key_digest(value: &str) -> String {
    format!("sha256:{}", hex_encode(&Sha256::digest(value.as_bytes())))
}

fn signed_bytes(record: &JournalRecordV1) -> Result<Vec<u8>, JournalError> {
    recorder_observation_signing_bytes(&record.clone().into())
        .map_err(|error| JournalError::Encoding(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custody::test_credential;
    use carsinos_protocol::execass_recorder::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn test_root(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".ea213-test-tmp")
            .join(format!(
                "{}-{}-{}",
                name,
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn command(payload: &str) -> ExecuteOnceV1 {
        let mut command = ExecuteOnceV1 {
            binding: RecorderBindingV1 {
                protocol_version: RECORDER_PROTOCOL_VERSION.into(),
                canonical_root_identity: "root-1".into(),
                installation_id: "installation-1".into(),
                state_root_generation: 1,
                os_user_identity_digest: "user-1".into(),
                runtime_host_generation: 1,
                runtime_host_instance_id: "host-1".into(),
                runtime_fencing_token: 1,
            },
            request_id: "request-1".into(),
            claim_event_id: "claim-1".into(),
            claim_receipt_id: "receipt-1".into(),
            continuation_fencing_token: 1,
            delegation_id: "delegation-1".into(),
            continuation_id: "continuation-1".into(),
            action_id: "action-1".into(),
            logical_effect_id: "effect-1".into(),
            internal_idempotency_key: "internal-key-1".into(),
            attempt_id: "attempt-1".into(),
            attempt_number: 1,
            provider_identity: "fake".into(),
            provider_version: "v1".into(),
            adapter_identity: "ea213.fake-provider.v1".into(),
            adapter_artifact_digest: "artifact".into(),
            provider_request_digest: String::new(),
            provider_idempotency_key: Some("provider-key".into()),
            reconciliation_key: Some("reconciliation-key".into()),
            manifest_digest: "manifest".into(),
            payload_digest: payload.into(),
            operand_envelope: OpaqueOperandEnvelopeV1 {
                non_secret: serde_json::json!({}),
                secret_handles: vec![],
            },
            deadline_ms: i64::MAX,
            client_nonce: "nonce".into(),
            command_mac: String::new(),
        };
        command.provider_request_digest = command.derived_provider_request_digest().unwrap();
        command
    }

    #[test]
    fn replay_and_different_command_are_exact() {
        let root = test_root("replay");
        let credential = test_credential("root-1");
        let mut journal = Journal::open(&root, &credential).unwrap();
        let (replayed, accepted) = journal.accept(&command("payload-a"), 10).unwrap();
        assert!(!replayed);
        let (replayed, same) = journal.accept(&command("payload-a"), 11).unwrap();
        assert!(replayed);
        assert_eq!(accepted, same);
        assert!(matches!(
            journal.accept(&command("payload-b"), 12),
            Err(JournalError::CommandConflict)
        ));
    }

    #[test]
    fn restart_replays_full_signed_chain() {
        let root = test_root("restart");
        let credential = test_credential("root-1");
        let terminal = {
            let mut journal = Journal::open(&root, &credential).unwrap();
            journal.accept(&command("payload"), 10).unwrap();
            journal
                .mark_invocation_started(&command("payload"), 11)
                .unwrap();
            journal
                .record_terminal(
                    &command("payload"),
                    RecorderObservationKindV1::Present,
                    "response".into(),
                    "evidence".into(),
                    Some("remote-1".into()),
                    None,
                    Vec::new(),
                    12,
                )
                .unwrap()
        };
        let journal = Journal::open(&root, &credential).unwrap();
        assert_eq!(
            journal
                .query("installation-1", 1, "attempt-1", None)
                .unwrap(),
            Some(terminal)
        );
    }

    #[test]
    fn terminal_failure_class_is_closed_signed_and_exactly_replayed() {
        let root = test_root("failure-class");
        let credential = test_credential("root-1");
        let command = command("payload");
        let mut journal = Journal::open(&root, &credential).unwrap();
        journal.accept(&command, 10).unwrap();
        journal.mark_invocation_started(&command, 11).unwrap();

        for (kind, class) in [
            (RecorderObservationKindV1::Absent, None),
            (
                RecorderObservationKindV1::Present,
                Some(ProviderFailureClassV1::Authentication),
            ),
            (
                RecorderObservationKindV1::Unknown,
                Some(ProviderFailureClassV1::Transient),
            ),
        ] {
            assert!(matches!(
                journal.record_terminal(
                    &command,
                    kind,
                    "response".into(),
                    "evidence".into(),
                    None,
                    class,
                    Vec::new(),
                    12,
                ),
                Err(JournalError::Encoding(_))
            ));
        }

        let terminal = journal
            .record_terminal(
                &command,
                RecorderObservationKindV1::Absent,
                "response".into(),
                "evidence".into(),
                None,
                Some(ProviderFailureClassV1::Authentication),
                Vec::new(),
                12,
            )
            .unwrap();
        assert_eq!(
            terminal.provider_error_class,
            Some(ProviderFailureClassV1::Authentication)
        );

        let mut tampered = terminal.clone();
        tampered.provider_error_class = Some(ProviderFailureClassV1::Permanent);
        assert_ne!(
            unsigned_bytes(&tampered).unwrap(),
            unsigned_bytes(&terminal).unwrap()
        );
        assert!(journal.verify_record(&tampered).is_err());
        assert!(matches!(
            journal.execution_state(&command),
            Ok(JournalExecutionState::Replay(replayed)) if *replayed == terminal
        ));
    }

    #[test]
    fn torn_tail_is_truncated_but_interior_corruption_fails() {
        let root = test_root("torn");
        let credential = test_credential("root-1");
        {
            let mut journal = Journal::open(&root, &credential).unwrap();
            journal.accept(&command("payload"), 10).unwrap();
        }
        let path = root.join("runtime/effect-recorder/v1/journal.v1");
        let valid_len = fs::metadata(&path).unwrap().len();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[0, 0, 0, 20, b'{'])
            .unwrap();
        drop(Journal::open(&root, &credential).unwrap());
        assert_eq!(fs::metadata(&path).unwrap().len(), valid_len);

        let mut bytes = fs::read(&path).unwrap();
        bytes[8] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            Journal::open(&root, &credential),
            Err(JournalError::Corrupt(_))
        ));
    }

    #[test]
    fn first_creation_syncs_each_new_directory_and_file_entry() {
        let root = test_root("durable-first-creation");
        let credential = test_credential("root-1");
        let mut steps = Vec::new();
        let journal = Journal::open_inner(&root, &credential, |step| steps.push(step)).unwrap();
        let recorder_root = root.join("runtime/effect-recorder/v1");
        for directory in [
            root.join("runtime"),
            root.join("runtime/effect-recorder"),
            recorder_root.clone(),
        ] {
            let created = steps
                .iter()
                .position(|step| step == &DurabilityStep::DirectoryCreated(directory.clone()))
                .unwrap();
            assert_eq!(
                steps.get(created + 1),
                Some(&DurabilityStep::DirectorySynced(
                    directory.parent().unwrap().to_path_buf()
                ))
            );
        }
        for file in [recorder_root.join("lock"), recorder_root.join("journal.v1")] {
            let synced = steps
                .iter()
                .position(|step| step == &DurabilityStep::FileSynced(file.clone()))
                .unwrap();
            assert_eq!(
                steps.get(synced + 1),
                Some(&DurabilityStep::DirectorySynced(recorder_root.clone()))
            );
        }
        drop(journal);

        let mut restart_steps = Vec::new();
        drop(Journal::open_inner(&root, &credential, |step| restart_steps.push(step)).unwrap());
        assert!(restart_steps.is_empty());
    }
}
