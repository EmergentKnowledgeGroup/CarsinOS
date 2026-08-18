use carsinos_protocol::execass_recorder::{
    canonical_json_bytes, stable_text_digest, ExecuteOnceV1, OpaqueOperandEnvelopeV1,
    RecorderObservationKindV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::executor::{InvocationOutcome, ReconciliationOutcome};
use crate::state_verifier::VerifiedTechnicalResourceReservation;

pub const EXACT_OVERWRITE_TOOL_ID: &str = "carsinos.local-fs";
pub const EXACT_OVERWRITE_TOOL_VERSION: &str = "exact-overwrite.v1";
pub const EXACT_OVERWRITE_ACTION_KIND: &str = "resolved_destroy";
pub const EXACT_OVERWRITE_PROVIDER_IDENTITY: &str = "carsinos.local-fs.exact-overwrite";
pub const EXACT_OVERWRITE_PROVIDER_VERSION: &str = "v1";
pub const EXACT_OVERWRITE_ADAPTER_IDENTITY: &str = "carsinos.effect-recorder.exact-overwrite.v1";
pub const EXACT_OVERWRITE_OPERAND_CONTRACT: &str = "carsinos.local-fs.exact-overwrite.operand.v1";
pub const EXACT_OVERWRITE_RECONCILIATION_CONTRACT: &str =
    "carsinos.local-fs.exact-overwrite.reconciliation.v1";
pub const EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST: &str =
    "sha256:ca57c48273a5dbcf8627d893612e56c2bff85d5e895f2c81e89156cedc90c72e";
pub const EXACT_OVERWRITE_ADAPTER_CONTRACT_PREIMAGE: &str = concat!(
    "carsinos.effect-recorder.adapter-contract.v1\0",
    "carsinos.local-fs\0",
    "exact-overwrite.v1\0",
    "resolved_destroy\0",
    "carsinos.local-fs.exact-overwrite\0",
    "v1\0",
    "carsinos.effect-recorder.exact-overwrite.v1\0",
    "operand=carsinos.local-fs.exact-overwrite.operand.v1\0",
    "reconciliation=carsinos.local-fs.exact-overwrite.reconciliation.v1"
);
pub const EXACT_OVERWRITE_MAX_REPLACEMENT_BYTES: usize = 4096;

const OBSERVATION_CONTRACT: &str = "carsinos.local-fs.exact-overwrite.observation.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExactOverwriteOperandV1 {
    pub contract_version: String,
    pub target_path: String,
    pub target_identity: String,
    pub expected_preimage_sha256: String,
    pub replacement_hex: String,
    pub replacement_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExactOverwriteReconciliationKeyV1 {
    pub contract_version: String,
    pub target_path: String,
    pub target_identity: String,
    pub expected_preimage_sha256: String,
    pub replacement_sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExactOverwriteMaterialV1 {
    pub operand_envelope: OpaqueOperandEnvelopeV1,
    pub payload_digest: String,
    pub reconciliation_key: String,
    pub reconciliation_key_digest: String,
}

/// Builds the complete immutable recorder material from a server-selected file.
/// The final path component is opened without following symlinks/reparse points.
pub fn build_exact_overwrite_material(
    target: &Path,
    replacement: &[u8],
) -> anyhow::Result<ExactOverwriteMaterialV1> {
    validate_replacement(replacement)?;
    let target_path = canonical_exact_overwrite_target(target)?;
    let mut file = open_no_follow(&target_path)?;
    let metadata = checked_file_metadata(&file)?;
    let target_identity = file_identity(&file, &metadata)?;
    let expected_preimage_sha256 = hash_open_file(&mut file)?;
    let replacement_sha256 = digest_bytes(replacement);
    if expected_preimage_sha256 == replacement_sha256 {
        anyhow::bail!("replacement must differ from the exact preimage");
    }
    let operand = ExactOverwriteOperandV1 {
        contract_version: EXACT_OVERWRITE_OPERAND_CONTRACT.to_owned(),
        target_path: path_text(&target_path)?,
        target_identity,
        expected_preimage_sha256,
        replacement_hex: crate::hex_encode(replacement),
        replacement_sha256,
    };
    material_from_operand(operand)
}

pub fn canonical_exact_overwrite_target(target: &Path) -> anyhow::Result<PathBuf> {
    if !target.is_absolute() {
        anyhow::bail!("exact-overwrite target must be absolute");
    }
    let file_name = target
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("exact-overwrite target must name one file"))?;
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("exact-overwrite target has no parent"))?;
    let canonical_parent = std::fs::canonicalize(parent)?;
    let resolved = canonical_parent.join(file_name);
    let metadata = std::fs::symlink_metadata(&resolved)?;
    reject_path_metadata(&metadata)?;
    let file = open_no_follow(&resolved)?;
    checked_file_metadata(&file)?;
    Ok(resolved)
}

pub fn exact_overwrite_target_identity(target: &Path) -> anyhow::Result<String> {
    let target = canonical_exact_overwrite_target(target)?;
    let file = open_no_follow(&target)?;
    let metadata = checked_file_metadata(&file)?;
    file_identity(&file, &metadata)
}

pub fn exact_overwrite_payload_digest(
    envelope: &OpaqueOperandEnvelopeV1,
) -> anyhow::Result<String> {
    Ok(digest_bytes(&canonical_json_bytes(envelope)?))
}

pub fn exact_overwrite_reconciliation_key(
    operand: &ExactOverwriteOperandV1,
) -> anyhow::Result<String> {
    let key = ExactOverwriteReconciliationKeyV1 {
        contract_version: EXACT_OVERWRITE_RECONCILIATION_CONTRACT.to_owned(),
        target_path: operand.target_path.clone(),
        target_identity: operand.target_identity.clone(),
        expected_preimage_sha256: operand.expected_preimage_sha256.clone(),
        replacement_sha256: operand.replacement_sha256.clone(),
    };
    Ok(String::from_utf8(canonical_json_bytes(&key)?)?)
}

pub(crate) fn supports_exact_overwrite(command: &ExecuteOnceV1) -> bool {
    validated_command(command)
        .and_then(|validated| inspect_current(&validated).map(|state| (validated, state)))
        .is_ok_and(|(_, state)| matches!(state, CurrentState::Original | CurrentState::Replacement))
}

pub(crate) fn invoke_exact_overwrite(
    command: &ExecuteOnceV1,
    reservations: &[VerifiedTechnicalResourceReservation],
) -> InvocationOutcome {
    if reservations.is_empty() {
        return definite_absent(command, "verified_reservation_set_missing");
    }
    let validated = match validated_command(command) {
        Ok(value) => value,
        Err(_) => return definite_absent(command, "invalid_exact_overwrite_material"),
    };
    let mut file = match open_validated_target(&validated) {
        Ok(file) => file,
        Err(_) => return definite_absent(command, "exact_target_precondition_failed"),
    };
    let current = match hash_open_file(&mut file) {
        Ok(digest) if digest == validated.operand.replacement_sha256 => {
            return present_outcome(&validated, reservations)
        }
        Ok(digest) if digest == validated.operand.expected_preimage_sha256 => digest,
        Ok(_) | Err(_) => return definite_absent(command, "exact_preimage_not_present"),
    };
    debug_assert_eq!(current, validated.operand.expected_preimage_sha256);

    // From the first seek onward, any failure is outcome-ambiguous. The service
    // must record Unknown and reconciliation must inspect the same immutable key.
    if file.seek(SeekFrom::Start(0)).is_err()
        || file.write_all(&validated.replacement).is_err()
        || file.set_len(validated.replacement.len() as u64).is_err()
        || file.sync_all().is_err()
    {
        return unknown_outcome(&validated, "ambiguous_post_write_failure");
    }
    match hash_open_file(&mut file) {
        Ok(digest) if digest == validated.operand.replacement_sha256 => {
            present_outcome(&validated, reservations)
        }
        _ => unknown_outcome(&validated, "post_write_verification_failed"),
    }
}

pub(crate) fn reconcile_exact_overwrite(
    reconciliation_key: &str,
    reservations: &[VerifiedTechnicalResourceReservation],
) -> anyhow::Result<ReconciliationOutcome> {
    let key: ExactOverwriteReconciliationKeyV1 = serde_json::from_str(reconciliation_key)?;
    validate_key(&key)?;
    let validated = ValidatedExactOverwrite {
        operand: ExactOverwriteOperandV1 {
            contract_version: EXACT_OVERWRITE_OPERAND_CONTRACT.to_owned(),
            target_path: key.target_path.clone(),
            target_identity: key.target_identity.clone(),
            expected_preimage_sha256: key.expected_preimage_sha256.clone(),
            replacement_hex: String::new(),
            replacement_sha256: key.replacement_sha256.clone(),
        },
        replacement: Vec::new(),
        reconciliation_key: reconciliation_key.to_owned(),
    };
    let state = inspect_current(&validated).unwrap_or(CurrentState::Unknown);
    let kind = match state {
        CurrentState::Replacement if !reservations.is_empty() => RecorderObservationKindV1::Present,
        CurrentState::Replacement => RecorderObservationKindV1::Unknown,
        CurrentState::Original => RecorderObservationKindV1::Absent,
        CurrentState::Unknown => RecorderObservationKindV1::Unknown,
    };
    Ok(reconciliation_outcome(&validated, kind, reservations))
}

struct ValidatedExactOverwrite {
    operand: ExactOverwriteOperandV1,
    replacement: Vec<u8>,
    reconciliation_key: String,
}

#[derive(Clone, Copy)]
enum CurrentState {
    Original,
    Replacement,
    Unknown,
}

fn validated_command(command: &ExecuteOnceV1) -> anyhow::Result<ValidatedExactOverwrite> {
    if command.provider_identity != EXACT_OVERWRITE_PROVIDER_IDENTITY
        || command.provider_version != EXACT_OVERWRITE_PROVIDER_VERSION
        || command.adapter_identity != EXACT_OVERWRITE_ADAPTER_IDENTITY
        || command.adapter_artifact_digest != EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST
        || command.provider_idempotency_key.is_some()
        || !command.operand_envelope.secret_handles.is_empty()
    {
        anyhow::bail!("exact-overwrite fixed identity does not match");
    }
    let operand: ExactOverwriteOperandV1 =
        serde_json::from_value(command.operand_envelope.non_secret.clone())?;
    validate_operand(&operand)?;
    let replacement = decode_lower_hex(&operand.replacement_hex)?;
    validate_replacement(&replacement)?;
    if digest_bytes(&replacement) != operand.replacement_sha256
        || exact_overwrite_payload_digest(&command.operand_envelope)? != command.payload_digest
    {
        anyhow::bail!("exact-overwrite replacement or envelope digest does not match");
    }
    let reconciliation_key = exact_overwrite_reconciliation_key(&operand)?;
    if command.reconciliation_key.as_deref() != Some(reconciliation_key.as_str()) {
        anyhow::bail!("exact-overwrite reconciliation key does not match operands");
    }
    Ok(ValidatedExactOverwrite {
        operand,
        replacement,
        reconciliation_key,
    })
}

fn material_from_operand(
    operand: ExactOverwriteOperandV1,
) -> anyhow::Result<ExactOverwriteMaterialV1> {
    let reconciliation_key = exact_overwrite_reconciliation_key(&operand)?;
    let operand_envelope = OpaqueOperandEnvelopeV1 {
        non_secret: serde_json::to_value(operand)?,
        secret_handles: Vec::new(),
    };
    Ok(ExactOverwriteMaterialV1 {
        payload_digest: exact_overwrite_payload_digest(&operand_envelope)?,
        reconciliation_key_digest: stable_text_digest(&reconciliation_key),
        reconciliation_key,
        operand_envelope,
    })
}

fn validate_operand(operand: &ExactOverwriteOperandV1) -> anyhow::Result<()> {
    if operand.contract_version != EXACT_OVERWRITE_OPERAND_CONTRACT
        || !valid_digest(&operand.expected_preimage_sha256)
        || !valid_digest(&operand.replacement_sha256)
        || operand.expected_preimage_sha256 == operand.replacement_sha256
    {
        anyhow::bail!("invalid exact-overwrite operand contract");
    }
    let target = Path::new(&operand.target_path);
    if !target.is_absolute()
        || path_text(&canonical_exact_overwrite_target(target)?)? != operand.target_path
    {
        anyhow::bail!("exact-overwrite target is not the canonical absolute path");
    }
    Ok(())
}

fn validate_key(key: &ExactOverwriteReconciliationKeyV1) -> anyhow::Result<()> {
    if key.contract_version != EXACT_OVERWRITE_RECONCILIATION_CONTRACT
        || !Path::new(&key.target_path).is_absolute()
        || !valid_digest(&key.expected_preimage_sha256)
        || !valid_digest(&key.replacement_sha256)
        || key.expected_preimage_sha256 == key.replacement_sha256
        || key.target_identity.is_empty()
    {
        anyhow::bail!("invalid exact-overwrite reconciliation key");
    }
    Ok(())
}

fn inspect_current(validated: &ValidatedExactOverwrite) -> anyhow::Result<CurrentState> {
    let mut file = match open_validated_target(validated) {
        Ok(file) => file,
        Err(_) => return Ok(CurrentState::Unknown),
    };
    let digest = hash_open_file(&mut file)?;
    Ok(if digest == validated.operand.replacement_sha256 {
        CurrentState::Replacement
    } else if digest == validated.operand.expected_preimage_sha256 {
        CurrentState::Original
    } else {
        CurrentState::Unknown
    })
}

fn open_validated_target(validated: &ValidatedExactOverwrite) -> anyhow::Result<File> {
    let target = Path::new(&validated.operand.target_path);
    if path_text(&canonical_exact_overwrite_target(target)?)? != validated.operand.target_path {
        anyhow::bail!("exact-overwrite target path drifted");
    }
    let file = open_no_follow(target)?;
    let metadata = checked_file_metadata(&file)?;
    if file_identity(&file, &metadata)? != validated.operand.target_identity {
        anyhow::bail!("exact-overwrite target identity drifted");
    }
    Ok(file)
}

fn validate_replacement(replacement: &[u8]) -> anyhow::Result<()> {
    if replacement.is_empty() || replacement.len() > EXACT_OVERWRITE_MAX_REPLACEMENT_BYTES {
        anyhow::bail!("exact-overwrite replacement is empty or exceeds the fixed bound");
    }
    Ok(())
}

fn hash_open_file(file: &mut File) -> anyhow::Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("sha256:{}", crate::hex_encode(&digest.finalize())))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::hex_encode(&Sha256::digest(bytes)))
}

fn decode_lower_hex(value: &str) -> anyhow::Result<Vec<u8>> {
    if !value.len().is_multiple_of(2)
        || value.len() > EXACT_OVERWRITE_MAX_REPLACEMENT_BYTES * 2
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    {
        anyhow::bail!("replacement hex is not bounded canonical lowercase hex");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)?;
            Ok(u8::from_str_radix(text, 16)?)
        })
        .collect()
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn path_text(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("exact-overwrite target path is not UTF-8"))
}

fn reject_path_metadata(metadata: &std::fs::Metadata) -> anyhow::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("exact-overwrite target is not a no-follow regular file");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            anyhow::bail!("exact-overwrite target is a reparse point");
        }
    }
    Ok(())
}

fn checked_file_metadata(file: &File) -> anyhow::Result<std::fs::Metadata> {
    let metadata = file.metadata()?;
    reject_path_metadata(&metadata)?;
    Ok(metadata)
}

#[cfg(windows)]
fn open_no_follow(path: &Path) -> anyhow::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH,
    };
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH)
        .open(path)?)
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> anyhow::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(file)
}

#[cfg(windows)]
fn file_identity(file: &File, _metadata: &std::fs::Metadata) -> anyhow::Result<String> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    let success =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, information.as_mut_ptr()) };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let information = unsafe { information.assume_init() };
    let index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    Ok(format!(
        "windows-volume:{:08x}:file:{index:016x}",
        information.dwVolumeSerialNumber
    ))
}

#[cfg(unix)]
fn file_identity(_file: &File, metadata: &std::fs::Metadata) -> anyhow::Result<String> {
    use std::os::unix::fs::MetadataExt;
    Ok(format!(
        "unix-device:{:016x}:inode:{:016x}",
        metadata.dev(),
        metadata.ino()
    ))
}

fn remote_effect_id(validated: &ValidatedExactOverwrite) -> String {
    format!(
        "local-fs-exact-overwrite:{}",
        &stable_text_digest(&validated.reconciliation_key)[7..]
    )
}

fn present_outcome(
    validated: &ValidatedExactOverwrite,
    reservations: &[VerifiedTechnicalResourceReservation],
) -> InvocationOutcome {
    let remote_effect_id = remote_effect_id(validated);
    let evidence = serde_json::json!({
        "contract_version": OBSERVATION_CONTRACT,
        "kind": "present",
        "remote_effect_id": remote_effect_id,
        "target_identity_digest": stable_text_digest(&validated.operand.target_identity),
        "expected_preimage_sha256": validated.operand.expected_preimage_sha256,
        "replacement_sha256": validated.operand.replacement_sha256,
        "technical_resource_actuals": [],
    });
    let response_digest = digest_bytes(&canonical_json_bytes(&evidence).expect("closed evidence"));
    let technical_resource_actuals = reservation_actuals(reservations, &response_digest);
    let evidence = serde_json::json!({
        "effect": evidence,
        "technical_resource_actuals": technical_resource_actuals,
    });
    InvocationOutcome {
        kind: RecorderObservationKindV1::Present,
        response_digest,
        evidence_payload_digest: digest_bytes(
            &canonical_json_bytes(&evidence).expect("closed evidence"),
        ),
        remote_effect_id: Some(remote_effect_id),
        provider_error_class: None,
        technical_resource_actuals,
    }
}

fn definite_absent(command: &ExecuteOnceV1, basis: &str) -> InvocationOutcome {
    let evidence = serde_json::json!({
        "contract_version": OBSERVATION_CONTRACT,
        "kind": "absent",
        "basis": basis,
        "provider_request_digest": command.provider_request_digest,
        "technical_resource_actuals": [],
    });
    let digest = digest_bytes(&canonical_json_bytes(&evidence).expect("closed evidence"));
    InvocationOutcome {
        kind: RecorderObservationKindV1::Absent,
        response_digest: digest.clone(),
        evidence_payload_digest: digest,
        remote_effect_id: None,
        provider_error_class: Some(
            carsinos_protocol::execass_recorder::ProviderFailureClassV1::Permanent,
        ),
        technical_resource_actuals: Vec::new(),
    }
}

fn unknown_outcome(validated: &ValidatedExactOverwrite, basis: &str) -> InvocationOutcome {
    let evidence = evidence_value(validated, "unknown", basis);
    let digest = digest_bytes(&canonical_json_bytes(&evidence).expect("closed evidence"));
    InvocationOutcome {
        kind: RecorderObservationKindV1::Unknown,
        response_digest: digest.clone(),
        evidence_payload_digest: digest,
        remote_effect_id: Some(remote_effect_id(validated)),
        provider_error_class: None,
        technical_resource_actuals: Vec::new(),
    }
}

fn reconciliation_outcome(
    validated: &ValidatedExactOverwrite,
    kind: RecorderObservationKindV1,
    reservations: &[VerifiedTechnicalResourceReservation],
) -> ReconciliationOutcome {
    let label = match kind {
        RecorderObservationKindV1::Present => "present",
        RecorderObservationKindV1::Absent => "absent",
        _ => "unknown",
    };
    let evidence = evidence_value(validated, label, "independent_exact_target_inspection");
    let response_digest = digest_bytes(&canonical_json_bytes(&evidence).expect("closed evidence"));
    let technical_resource_actuals = if kind == RecorderObservationKindV1::Present {
        reservation_actuals(reservations, &response_digest)
    } else {
        Vec::new()
    };
    let evidence = serde_json::json!({
        "effect": evidence,
        "technical_resource_actuals": technical_resource_actuals,
    });
    ReconciliationOutcome {
        kind,
        response_digest,
        evidence_payload_digest: digest_bytes(
            &canonical_json_bytes(&evidence).expect("closed evidence"),
        ),
        remote_effect_id: (kind == RecorderObservationKindV1::Present)
            .then(|| remote_effect_id(validated)),
        technical_resource_actuals,
    }
}

fn reservation_actuals(
    reservations: &[VerifiedTechnicalResourceReservation],
    exact_overwrite_evidence_digest: &str,
) -> Vec<carsinos_protocol::execass_recorder::TechnicalResourceActualV1> {
    reservations
        .iter()
        .map(|reservation| {
            let evidence = serde_json::json!({
                "contract_version": "carsinos.local-fs.exact-overwrite.resource-actual.v1",
                "exact_overwrite_evidence_digest": exact_overwrite_evidence_digest,
                "reservation_id": reservation.reservation_id,
                "amount_actual": reservation.amount_reserved,
            });
            carsinos_protocol::execass_recorder::TechnicalResourceActualV1 {
                reservation_id: reservation.reservation_id.clone(),
                amount_actual: reservation.amount_reserved,
                evidence_digest: digest_bytes(
                    &canonical_json_bytes(&evidence).expect("closed reservation evidence"),
                ),
            }
        })
        .collect()
}

fn evidence_value(
    validated: &ValidatedExactOverwrite,
    kind: &str,
    basis: &str,
) -> serde_json::Value {
    serde_json::json!({
        "contract_version": OBSERVATION_CONTRACT,
        "kind": kind,
        "basis": basis,
        "remote_effect_id": remote_effect_id(validated),
        "target_identity_digest": stable_text_digest(&validated.operand.target_identity),
        "expected_preimage_sha256": validated.operand.expected_preimage_sha256,
        "replacement_sha256": validated.operand.replacement_sha256,
        "technical_resource_actuals": [],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use carsinos_protocol::execass_recorder::{RecorderBindingV1, RECORDER_PROTOCOL_VERSION};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(name: &str) -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                ".exact-overwrite-test-{}-{id}-{name}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn command(material: ExactOverwriteMaterialV1) -> ExecuteOnceV1 {
        let mut command = ExecuteOnceV1 {
            binding: RecorderBindingV1 {
                protocol_version: RECORDER_PROTOCOL_VERSION.into(),
                canonical_root_identity: "root".into(),
                installation_id: "installation".into(),
                state_root_generation: 1,
                os_user_identity_digest: "user".into(),
                runtime_host_generation: 1,
                runtime_host_instance_id: "host".into(),
                runtime_fencing_token: 1,
            },
            request_id: "request".into(),
            claim_event_id: "claim".into(),
            claim_receipt_id: "receipt".into(),
            continuation_fencing_token: 1,
            delegation_id: "delegation".into(),
            continuation_id: "continuation".into(),
            action_id: "action".into(),
            logical_effect_id: "effect".into(),
            internal_idempotency_key: "internal".into(),
            attempt_id: "attempt".into(),
            attempt_number: 1,
            provider_identity: EXACT_OVERWRITE_PROVIDER_IDENTITY.into(),
            provider_version: EXACT_OVERWRITE_PROVIDER_VERSION.into(),
            adapter_identity: EXACT_OVERWRITE_ADAPTER_IDENTITY.into(),
            adapter_artifact_digest: EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST.into(),
            provider_request_digest: String::new(),
            provider_idempotency_key: None,
            reconciliation_key: Some(material.reconciliation_key),
            manifest_digest: "manifest".into(),
            payload_digest: material.payload_digest,
            operand_envelope: material.operand_envelope,
            deadline_ms: i64::MAX,
            client_nonce: "nonce".into(),
            command_mac: String::new(),
        };
        command.provider_request_digest = command.derived_provider_request_digest().unwrap();
        command
    }

    #[test]
    fn exact_supported_command_overwrites_one_file_and_reconciles() {
        let fixture = Fixture::new("execute");
        let target = fixture.0.join("owner-recovery.conf");
        let original = b"sole-recovery-path=enabled\n";
        let replacement = b"sole-recovery-path=destroyed\n";
        fs::write(&target, original).unwrap();
        let material = build_exact_overwrite_material(&target, replacement).unwrap();
        let reconciliation_key = material.reconciliation_key.clone();
        let command = command(material);

        assert!(supports_exact_overwrite(&command));
        assert_eq!(
            reconcile_exact_overwrite(&reconciliation_key, &[])
                .unwrap()
                .kind,
            RecorderObservationKindV1::Absent
        );
        let missing_reservations = invoke_exact_overwrite(&command, &[]);
        assert_eq!(missing_reservations.kind, RecorderObservationKindV1::Absent);
        assert_eq!(fs::read(&target).unwrap(), original);
        let reservations = [VerifiedTechnicalResourceReservation {
            reservation_id: "reservation-1".into(),
            amount_reserved: 1,
        }];
        let first = invoke_exact_overwrite(&command, &reservations);
        assert_eq!(first.kind, RecorderObservationKindV1::Present);
        assert_eq!(first.technical_resource_actuals.len(), 1);
        assert_eq!(
            first.technical_resource_actuals[0].reservation_id,
            "reservation-1"
        );
        assert_eq!(first.technical_resource_actuals[0].amount_actual, 1);
        assert_eq!(fs::read(&target).unwrap(), replacement);
        assert!(target.is_file());
        assert_eq!(
            reconcile_exact_overwrite(&reconciliation_key, &reservations)
                .unwrap()
                .kind,
            RecorderObservationKindV1::Present
        );
        let second = invoke_exact_overwrite(&command, &reservations);
        assert_eq!(second.kind, RecorderObservationKindV1::Present);
        assert_eq!(first.remote_effect_id, second.remote_effect_id);
        assert_eq!(
            first.evidence_payload_digest,
            second.evidence_payload_digest
        );
        assert_eq!(fs::read(&target).unwrap(), replacement);
    }

    #[test]
    fn malformed_identity_digest_secret_extra_and_preimage_have_zero_effect() {
        let fixture = Fixture::new("reject");
        let target = fixture.0.join("sentinel.conf");
        let original = b"original-sentinel";
        fs::write(&target, original).unwrap();
        let base = command(build_exact_overwrite_material(&target, b"replacement").unwrap());

        let mut cases = Vec::new();
        let mut wrong_provider = base.clone();
        wrong_provider.provider_identity.push_str(".wrong");
        cases.push(wrong_provider);
        let mut wrong_version = base.clone();
        wrong_version.provider_version = "v2".into();
        cases.push(wrong_version);
        let mut wrong_adapter = base.clone();
        wrong_adapter.adapter_identity.push_str(".wrong");
        cases.push(wrong_adapter);
        let mut wrong_artifact = base.clone();
        wrong_artifact.adapter_artifact_digest = format!("sha256:{}", "0".repeat(64));
        cases.push(wrong_artifact);
        let mut secret = base.clone();
        secret.operand_envelope.secret_handles.push(
            carsinos_protocol::execass_recorder::OpaqueSecretHandleV1 {
                version: 1,
                backend: "keyring".into(),
                opaque_id: "opaque".into(),
                purpose: "replacement".into(),
                capability_class: "provider".into(),
            },
        );
        cases.push(secret);
        let mut extra = base.clone();
        extra.operand_envelope.non_secret["extra"] = serde_json::json!(true);
        cases.push(extra);
        let mut wrong_preimage = base.clone();
        wrong_preimage.operand_envelope.non_secret["expected_preimage_sha256"] =
            serde_json::json!(format!("sha256:{}", "1".repeat(64)));
        rebind_material(&mut wrong_preimage);
        cases.push(wrong_preimage);
        let mut wrong_identity = base.clone();
        wrong_identity.operand_envelope.non_secret["target_identity"] =
            serde_json::json!("windows-volume:00000000:file:0000000000000000");
        rebind_material(&mut wrong_identity);
        cases.push(wrong_identity);
        let mut wrong_replacement_digest = base.clone();
        wrong_replacement_digest.operand_envelope.non_secret["replacement_sha256"] =
            serde_json::json!(format!("sha256:{}", "2".repeat(64)));
        rebind_material(&mut wrong_replacement_digest);
        cases.push(wrong_replacement_digest);
        let mut relative = base.clone();
        relative.operand_envelope.non_secret["target_path"] =
            serde_json::json!("relative-sentinel.conf");
        rebind_material(&mut relative);
        cases.push(relative);

        for candidate in cases {
            assert!(!supports_exact_overwrite(&candidate));
            assert_eq!(fs::read(&target).unwrap(), original);
        }
    }

    fn rebind_material(command: &mut ExecuteOnceV1) {
        let operand: ExactOverwriteOperandV1 =
            serde_json::from_value(command.operand_envelope.non_secret.clone()).unwrap();
        command.payload_digest = exact_overwrite_payload_digest(&command.operand_envelope).unwrap();
        command.reconciliation_key = Some(exact_overwrite_reconciliation_key(&operand).unwrap());
        command.provider_request_digest = command.derived_provider_request_digest().unwrap();
    }

    #[test]
    fn identity_and_partial_content_drift_reconcile_unknown() {
        let fixture = Fixture::new("drift");
        let target = fixture.0.join("sentinel.conf");
        fs::write(&target, b"original").unwrap();
        let material = build_exact_overwrite_material(&target, b"replacement").unwrap();
        let key = material.reconciliation_key.clone();
        fs::write(&target, b"partial").unwrap();
        assert_eq!(
            reconcile_exact_overwrite(&key, &[]).unwrap().kind,
            RecorderObservationKindV1::Unknown
        );
        fs::remove_file(&target).unwrap();
        fs::write(&target, b"replacement").unwrap();
        assert_eq!(
            reconcile_exact_overwrite(&key, &[]).unwrap().kind,
            RecorderObservationKindV1::Unknown
        );
    }

    #[test]
    fn final_component_escape_is_never_followed() {
        let fixture = Fixture::new("escape");
        let outside = fixture.0.join("outside");
        let link = fixture.0.join("escape-link");
        fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("sentinel.conf");
        fs::write(&sentinel, b"must-stay").unwrap();

        #[cfg(windows)]
        {
            let status = std::process::Command::new("cmd")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .status()
                .unwrap();
            assert!(status.success());
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        assert!(build_exact_overwrite_material(&link, b"replacement").is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"must-stay");
    }

    #[test]
    fn evidence_and_remote_identity_are_deterministic_and_content_free() {
        let fixture = Fixture::new("evidence");
        let target = fixture.0.join("sentinel.conf");
        let raw_secret_like_content = b"raw-owner-recovery-secret-never-in-evidence";
        fs::write(&target, raw_secret_like_content).unwrap();
        let command = command(
            build_exact_overwrite_material(&target, b"replacement-without-secret").unwrap(),
        );
        let outcome = invoke_exact_overwrite(
            &command,
            &[VerifiedTechnicalResourceReservation {
                reservation_id: "reservation-1".into(),
                amount_reserved: 1,
            }],
        );
        let debug = format!("{outcome:?}");
        assert!(!debug.contains(std::str::from_utf8(raw_secret_like_content).unwrap()));
        assert!(!outcome
            .response_digest
            .contains("replacement-without-secret"));
        assert!(outcome
            .remote_effect_id
            .as_deref()
            .unwrap()
            .starts_with("local-fs-exact-overwrite:"));
    }

    #[test]
    fn published_adapter_contract_digest_matches_its_locked_preimage() {
        assert_eq!(
            digest_bytes(EXACT_OVERWRITE_ADAPTER_CONTRACT_PREIMAGE.as_bytes()),
            EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST
        );
    }
}
