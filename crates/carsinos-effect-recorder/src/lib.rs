//! EA-213 out-of-process execute-once recorder.
//!
//! This crate deliberately owns no scheduler, job claim, retry, decision, or
//! CarsinOS state mutation. It accepts only a storage-committed `invoking`
//! attempt proven independently from read-only SQLite.

mod auth;
mod custody;
mod exact_overwrite;
mod executor;
pub mod ipc;
mod journal;
mod root_identity;
mod service;
mod state_verifier;

pub use auth::{authenticate_request, sign_request};
pub use custody::{
    RecorderChannelCredential, RecorderChannelCustody, RecorderChannelCustodyError,
    RecorderIdentity,
};
pub use exact_overwrite::{
    build_exact_overwrite_material, canonical_exact_overwrite_target,
    exact_overwrite_payload_digest, exact_overwrite_reconciliation_key,
    exact_overwrite_target_identity, ExactOverwriteMaterialV1, ExactOverwriteOperandV1,
    ExactOverwriteReconciliationKeyV1, EXACT_OVERWRITE_ACTION_KIND,
    EXACT_OVERWRITE_ADAPTER_ARTIFACT_DIGEST, EXACT_OVERWRITE_ADAPTER_CONTRACT_PREIMAGE,
    EXACT_OVERWRITE_ADAPTER_IDENTITY, EXACT_OVERWRITE_MAX_REPLACEMENT_BYTES,
    EXACT_OVERWRITE_OPERAND_CONTRACT, EXACT_OVERWRITE_PROVIDER_IDENTITY,
    EXACT_OVERWRITE_PROVIDER_VERSION, EXACT_OVERWRITE_RECONCILIATION_CONTRACT,
    EXACT_OVERWRITE_TOOL_ID, EXACT_OVERWRITE_TOOL_VERSION,
};
#[cfg(feature = "test-support")]
pub use ipc::RecorderServer;
#[cfg(feature = "test-support")]
pub use ipc::TestRecorderTransport;
pub use ipc::{current_peer_identity_digest, RecorderClient, RecorderEndpoint};
#[cfg(not(feature = "test-support"))]
pub(crate) use journal::Journal;
pub use journal::JournalError;
#[cfg(feature = "test-support")]
pub use journal::{Journal, JournalRecordV1};
pub use root_identity::{canonical_database_for_root, canonical_state_root, CanonicalStateRoot};
#[cfg(not(feature = "test-support"))]
pub(crate) use service::RecorderService;
#[cfg(feature = "test-support")]
pub use service::TestFailpoint;
#[cfg(feature = "test-support")]
pub use service::{RecorderService, ServiceError};
pub use state_verifier::{AuthoritativeRecorderBinding, ReadOnlyBeganVerifier, VerificationError};

#[cfg(feature = "test-support")]
pub use custody::TestRecorderFixture;

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[doc(hidden)]
pub fn hex_for_binary(bytes: &[u8]) -> String {
    hex_encode(bytes)
}

pub(crate) fn hex_decode<const N: usize>(value: &str) -> anyhow::Result<[u8; N]> {
    if value.len() != N * 2 {
        anyhow::bail!("hex value has the wrong length");
    }
    let mut output = [0u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_nibble(pair[0])?;
        let low = decode_nibble(pair[1])?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

/// Narrow recorder-process configuration. Calling this starts the recorder and
/// does not return any custody, signing, journal, or service capability.
pub struct RecorderServiceLaunch {
    pub state_root: std::path::PathBuf,
    pub database: std::path::PathBuf,
    #[cfg(feature = "test-support")]
    pub test_fake_provider: bool,
    #[cfg(feature = "test-support")]
    pub test_failpoint: Option<TestFailpoint>,
    #[cfg(feature = "test-support")]
    pub test_coordination_root: Option<std::path::PathBuf>,
}

pub async fn run_recorder_service(launch: RecorderServiceLaunch) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::time::{SystemTime, UNIX_EPOCH};

    let now_ms = || -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(i64::MAX)
    };
    let state_root = canonical_state_root(&launch.state_root)
        .context("binding recorder state root to its canonical identity")?;
    let database = canonical_database_for_root(&launch.database, &state_root)
        .context("binding recorder database to its canonical state root")?;
    let verifier = ReadOnlyBeganVerifier::new(&database);
    let binding = verifier
        .load_authoritative_binding(now_ms())
        .context("loading authoritative recorder binding")?;
    if state_root.identity != binding.canonical_root_identity {
        anyhow::bail!("configured recorder state root does not match authoritative root identity");
    }
    #[cfg(feature = "test-support")]
    let credential = if launch.test_fake_provider {
        custody::test_credential(&binding.canonical_root_identity)
    } else {
        custody::NativeRecorderCustody::load_or_create(&binding.canonical_root_identity)?
    };
    #[cfg(not(feature = "test-support"))]
    let credential =
        custody::NativeRecorderCustody::load_or_create(&binding.canonical_root_identity)?;
    let channel_key = *credential.channel_key();
    let journal = Journal::open(&state_root.path, &credential)?;
    #[cfg(feature = "test-support")]
    let service = if launch.test_fake_provider {
        match (launch.test_failpoint, launch.test_coordination_root) {
            (Some(failpoint), Some(coordination_root)) => {
                RecorderService::with_fake_provider_coordination(
                    channel_key,
                    verifier,
                    journal,
                    state_root.path.join("ea213-fake-provider"),
                    failpoint,
                    coordination_root,
                )
            }
            _ => RecorderService::with_fake_provider(
                channel_key,
                verifier,
                journal,
                state_root.path.join("ea213-fake-provider"),
            ),
        }
    } else {
        RecorderService::production(channel_key, verifier, journal)
    };
    #[cfg(not(feature = "test-support"))]
    let service = RecorderService::production(channel_key, verifier, journal);
    let endpoint = RecorderEndpoint::for_binding(
        &state_root.path,
        &binding.installation_id,
        binding.state_root_generation,
    );
    ipc::RecorderServer::new(endpoint, binding.os_user_identity_digest, service)
        .serve()
        .await
}

fn decode_nibble(value: u8) -> anyhow::Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => anyhow::bail!("hex value is not canonical lowercase"),
    }
}
