use crate::{hex_decode, hex_encode};
use anyhow::{Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const CUSTODY_SERVICE: &str = "com.carsinos.execass.effect-recorder.v1";
const CUSTODY_DOMAIN: &[u8] = b"carsinos.execass.effect-recorder.v1";
const CHANNEL_CUSTODY_SERVICE: &str = "com.carsinos.execass.effect-recorder.channel.v1";
const CHANNEL_CUSTODY_DOMAIN: &[u8] = b"carsinos.execass.effect-recorder.channel.v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecorderIdentity {
    pub key_id: String,
    pub key_generation: u64,
    pub verifying_key_hex: String,
    pub verifying_key_digest: String,
}

impl RecorderIdentity {
    /// Validates every self-authenticating field before a channel record is
    /// allowed to influence a database pin. The channel record is only an
    /// input to later proof-of-possession; it is never trusted on its own.
    pub fn validate_for_root(&self, root_identity: &str) -> Result<()> {
        if self.key_generation != 1 {
            anyhow::bail!("recorder channel key generation is unsupported");
        }
        let verifying_key = hex_decode::<32>(&self.verifying_key_hex)
            .context("recorder channel verifying key is not canonical 32-byte hex")?;
        VerifyingKey::from_bytes(&verifying_key)
            .map_err(|_| anyhow::anyhow!("recorder channel verifying key is invalid"))?;
        let expected_digest = hex_encode(&Sha256::digest(verifying_key));
        if self.verifying_key_digest != expected_digest {
            anyhow::bail!("recorder channel verifying-key digest does not match its key");
        }
        let mut key_id = Sha256::new();
        key_id.update(CUSTODY_DOMAIN);
        key_id.update([0]);
        key_id.update(root_identity.as_bytes());
        key_id.update([0]);
        key_id.update(verifying_key);
        let expected_key_id = format!("effect-recorder-v1-{}", hex_encode(&key_id.finalize()));
        if self.key_id != expected_key_id {
            anyhow::bail!("recorder channel key ID does not match the recorder custody domain");
        }
        Ok(())
    }
}

pub(crate) struct RecorderCredential {
    signing_seed: Zeroizing<[u8; 32]>,
    channel_key: Zeroizing<[u8; 32]>,
    identity: RecorderIdentity,
}

/// Gateway-safe half of the recorder custody binding. It contains the IPC MAC
/// key and the recorder's public identity, but never signing material.
pub struct RecorderChannelCredential {
    channel_key: Zeroizing<[u8; 32]>,
    identity: RecorderIdentity,
}

impl std::fmt::Debug for RecorderChannelCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecorderChannelCredential")
            .field("identity", &self.identity)
            .field("channel_key", &"[REDACTED]")
            .finish()
    }
}

impl RecorderChannelCredential {
    pub fn channel_key(&self) -> &[u8; 32] {
        &self.channel_key
    }

    pub fn identity(&self) -> &RecorderIdentity {
        &self.identity
    }
}

pub struct RecorderChannelCustody;

#[derive(Debug, thiserror::Error)]
pub enum RecorderChannelCustodyError {
    #[error("recorder channel OS custody entry is absent")]
    Missing,
    #[error("recorder channel OS custody entry is invalid: {0}")]
    Invalid(#[source] anyhow::Error),
    #[error("recorder channel OS custody is unavailable: {0}")]
    Unavailable(#[source] anyhow::Error),
}

impl RecorderChannelCustody {
    /// Opens only the separately custodied client MAC/key identity record.
    /// It never reads the recorder signing-custody record.
    pub fn load_existing(
        root_identity: &str,
    ) -> std::result::Result<RecorderChannelCredential, RecorderChannelCustodyError> {
        let entry = keyring::Entry::new(
            CHANNEL_CUSTODY_SERVICE,
            &channel_custody_account(root_identity),
        )
        .context("opening recorder channel OS custody entry")
        .map_err(RecorderChannelCustodyError::Unavailable)?;
        let value = match entry.get_password() {
            Ok(value) => value,
            Err(keyring::Error::NoEntry) => return Err(RecorderChannelCustodyError::Missing),
            Err(error) => {
                return Err(RecorderChannelCustodyError::Unavailable(anyhow::anyhow!(
                    "reading recorder channel OS custody entry: {error}"
                )))
            }
        };
        parse_channel_payload(Zeroizing::new(value), root_identity)
            .map_err(RecorderChannelCustodyError::Invalid)
    }
}

impl std::fmt::Debug for RecorderCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecorderCredential")
            .field("identity", &self.identity)
            .field("secret_material", &"[REDACTED]")
            .finish()
    }
}

impl RecorderCredential {
    pub fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.signing_seed)
    }

    pub fn channel_key(&self) -> &[u8; 32] {
        &self.channel_key
    }

    pub fn identity(&self) -> &RecorderIdentity {
        &self.identity
    }

    fn from_parts(seed: [u8; 32], channel_key: [u8; 32], root_identity: &str) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key().to_bytes();
        let mut id = Sha256::new();
        id.update(CUSTODY_DOMAIN);
        id.update([0]);
        id.update(root_identity.as_bytes());
        id.update([0]);
        id.update(verifying);
        Self {
            signing_seed: Zeroizing::new(seed),
            channel_key: Zeroizing::new(channel_key),
            identity: RecorderIdentity {
                key_id: format!("effect-recorder-v1-{}", hex_encode(&id.finalize())),
                key_generation: 1,
                verifying_key_hex: hex_encode(&verifying),
                verifying_key_digest: hex_encode(&Sha256::digest(verifying)),
            },
        }
    }
}

impl Drop for RecorderCredential {
    fn drop(&mut self) {
        self.signing_seed.zeroize();
        self.channel_key.zeroize();
    }
}

pub(crate) struct NativeRecorderCustody;

impl NativeRecorderCustody {
    pub fn load_or_create(root_identity: &str) -> Result<RecorderCredential> {
        let account = custody_account(root_identity);
        let entry = keyring::Entry::new(CUSTODY_SERVICE, &account)
            .context("opening recorder OS custody entry")?;
        let credential = match entry.get_password() {
            Ok(value) => parse_payload(Zeroizing::new(value), root_identity),
            Err(keyring::Error::NoEntry) => {
                let mut seed = [0u8; 32];
                let mut channel = [0u8; 32];
                getrandom::fill(&mut seed).context("generating recorder signing seed")?;
                getrandom::fill(&mut channel).context("generating recorder channel key")?;
                let payload = Zeroizing::new(format!(
                    "carsinos-effect-recorder-custody-v1|{}|{}|{}",
                    root_identity,
                    hex_encode(&seed),
                    hex_encode(&channel)
                ));
                entry
                    .set_password(&payload)
                    .context("writing recorder OS custody entry")?;
                let credential = RecorderCredential::from_parts(seed, channel, root_identity);
                Ok(credential)
            }
            Err(error) => Err(anyhow::anyhow!(
                "reading recorder OS custody entry: {error}"
            )),
        }?;
        // Also backfill the channel-only record for a legacy combined custody
        // entry. This migration is performed by the recorder service, never
        // by the gateway client.
        persist_channel_credential(root_identity, &credential)?;
        Ok(credential)
    }
}

fn custody_account(root_identity: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(CUSTODY_DOMAIN);
    digest.update([0]);
    digest.update(root_identity.as_bytes());
    format!("root-{}", hex_encode(&digest.finalize()))
}

fn channel_custody_account(root_identity: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(CHANNEL_CUSTODY_DOMAIN);
    digest.update([0]);
    digest.update(root_identity.as_bytes());
    format!("root-{}", hex_encode(&digest.finalize()))
}

fn parse_payload(payload: Zeroizing<String>, root_identity: &str) -> Result<RecorderCredential> {
    let parts = payload.split('|').collect::<Vec<_>>();
    if parts.len() != 4
        || parts[0] != "carsinos-effect-recorder-custody-v1"
        || parts[1] != root_identity
    {
        anyhow::bail!("recorder OS custody entry has the wrong root binding");
    }
    Ok(RecorderCredential::from_parts(
        hex_decode::<32>(parts[2])?,
        hex_decode::<32>(parts[3])?,
        root_identity,
    ))
}

fn persist_channel_credential(root_identity: &str, credential: &RecorderCredential) -> Result<()> {
    let entry = keyring::Entry::new(
        CHANNEL_CUSTODY_SERVICE,
        &channel_custody_account(root_identity),
    )
    .context("opening recorder channel OS custody entry")?;
    let identity = credential.identity();
    let payload = Zeroizing::new(format!(
        "carsinos-effect-recorder-channel-v1|{}|{}|{}|{}|{}|{}",
        root_identity,
        hex_encode(credential.channel_key()),
        identity.key_id,
        identity.key_generation,
        identity.verifying_key_hex,
        identity.verifying_key_digest,
    ));
    match entry.get_password() {
        Ok(existing) => {
            let existing = parse_channel_payload(Zeroizing::new(existing), root_identity)?;
            if existing.channel_key() != credential.channel_key() || existing.identity() != identity
            {
                anyhow::bail!("recorder channel custody does not match signing custody identity");
            }
        }
        Err(keyring::Error::NoEntry) => entry
            .set_password(&payload)
            .context("writing recorder channel OS custody entry")?,
        Err(error) => {
            return Err(anyhow::anyhow!(
                "reading recorder channel OS custody entry: {error}"
            ))
        }
    }
    Ok(())
}

fn parse_channel_payload(
    payload: Zeroizing<String>,
    root_identity: &str,
) -> Result<RecorderChannelCredential> {
    let parts = payload.split('|').collect::<Vec<_>>();
    if parts.len() != 7
        || parts[0] != "carsinos-effect-recorder-channel-v1"
        || parts[1] != root_identity
    {
        anyhow::bail!("recorder channel OS custody entry has the wrong root binding");
    }
    let credential = RecorderChannelCredential {
        channel_key: Zeroizing::new(hex_decode::<32>(parts[2])?),
        identity: RecorderIdentity {
            key_id: parts[3].to_owned(),
            key_generation: parts[4]
                .parse()
                .context("invalid recorder channel key generation")?,
            verifying_key_hex: parts[5].to_owned(),
            verifying_key_digest: parts[6].to_owned(),
        },
    };
    credential.identity.validate_for_root(root_identity)?;
    Ok(credential)
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn test_credential(root_identity: &str) -> RecorderCredential {
    RecorderCredential::from_parts([0x31; 32], [0x72; 32], root_identity)
}

/// Test-only opaque fixture. It preserves process-test ergonomics without
/// exporting recorder signing custody, even when Cargo feature unification
/// enables `test-support` for another workspace package.
#[cfg(feature = "test-support")]
pub struct TestRecorderFixture {
    credential: RecorderCredential,
}

#[cfg(feature = "test-support")]
impl TestRecorderFixture {
    pub fn for_root(root_identity: &str) -> Self {
        Self {
            credential: test_credential(root_identity),
        }
    }

    pub fn client(&self, endpoint: crate::RecorderEndpoint) -> crate::RecorderClient {
        crate::RecorderClient::new(
            endpoint,
            *self.credential.channel_key(),
            self.credential.identity().clone(),
        )
    }

    pub fn identity(&self) -> &RecorderIdentity {
        self.credential.identity()
    }

    pub fn client_with_channel_key(
        &self,
        endpoint: crate::RecorderEndpoint,
        channel_key: [u8; 32],
    ) -> crate::RecorderClient {
        crate::RecorderClient::new(endpoint, channel_key, self.credential.identity().clone())
    }

    pub fn sign_request(
        &self,
        request: &mut carsinos_protocol::execass_recorder::RecorderRequestV1,
    ) -> anyhow::Result<()> {
        crate::sign_request(request, self.credential.channel_key())
    }

    pub fn open_journal(
        &self,
        state_root: &std::path::Path,
    ) -> Result<crate::Journal, crate::JournalError> {
        crate::Journal::open(state_root, &self.credential)
    }

    #[doc(hidden)]
    pub fn production_service(
        &self,
        verifier: crate::ReadOnlyBeganVerifier,
        journal: crate::Journal,
    ) -> std::sync::Arc<crate::RecorderService> {
        crate::RecorderService::production(*self.credential.channel_key(), verifier, journal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_identity_recomputes_key_digest_key_id_and_generation() {
        let root = "sha256:test-root";
        let credential = test_credential(root);
        let identity = credential.identity().clone();
        identity.validate_for_root(root).unwrap();

        let mut bad_digest = identity.clone();
        bad_digest.verifying_key_digest = "00".repeat(32);
        assert!(bad_digest.validate_for_root(root).is_err());

        let mut bad_key_id = identity.clone();
        bad_key_id.key_id.push('0');
        assert!(bad_key_id.validate_for_root(root).is_err());

        let mut bad_generation = identity.clone();
        bad_generation.key_generation = 2;
        assert!(bad_generation.validate_for_root(root).is_err());

        let mut bad_hex = identity;
        bad_hex.verifying_key_hex = "ff".repeat(31);
        assert!(bad_hex.validate_for_root(root).is_err());
    }
}
