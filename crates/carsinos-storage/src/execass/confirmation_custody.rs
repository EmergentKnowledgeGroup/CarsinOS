//! Fixed, parameter-free OS custody for the single ExecAss confirmation key.
//!
//! The locator is derived only from the already-canonical state-root identity.
//! On Windows the binding digest uses the current process token SID. On macOS
//! it uses the kernel-reported effective UID: Keychain itself supplies the
//! stronger current-user access boundary, while an in-process public API for a
//! stable macOS account UUID is not available without entitlements/services
//! that this storage crate must not invent.

#[cfg(test)]
use super::confirmation_attestation::{
    confirmation_attestation_signing_bytes, ConfirmationAttestation, ConfirmationAttestationPayload,
};
use super::store::{immediate_transaction, ExecAssStore};
use anyhow::{anyhow, bail, Context, Result};
#[cfg(test)]
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use fs2::FileExt;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

const CUSTODY_SERVICE: &str = "com.carsinos.execass.confirmation-authority.v1";
const CUSTODY_DOMAIN: &[u8] = b"carsinos.execass.confirmation-authority.v1";
const STATE_ROOT_GENERATION: u64 = 1;
const KEY_GENERATION: u64 = 1;

/// Public, serializable information needed to identify the one pinned
/// verification key and its public binding metadata. It intentionally contains
/// no custody locator, state-root path, or signing material.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ConfirmationAuthorityIdentity {
    key_id: String,
    key_generation: u64,
    verifying_key_hex: String,
    verifying_key_digest: String,
    canonical_root_identity: String,
    installation_identity: String,
    os_user_identity_digest: String,
    state_root_generation: u64,
    local_credential_identity: String,
}

impl ConfirmationAuthorityIdentity {
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    pub fn key_generation(&self) -> u64 {
        self.key_generation
    }
    pub fn verifying_key_hex(&self) -> &str {
        &self.verifying_key_hex
    }
    pub fn verifying_key_digest(&self) -> &str {
        &self.verifying_key_digest
    }
    pub fn canonical_root_identity(&self) -> &str {
        &self.canonical_root_identity
    }
    pub fn installation_identity(&self) -> &str {
        &self.installation_identity
    }
    pub fn os_user_identity_digest(&self) -> &str {
        &self.os_user_identity_digest
    }
    pub fn state_root_generation(&self) -> u64 {
        self.state_root_generation
    }
    pub fn local_credential_identity(&self) -> &str {
        &self.local_credential_identity
    }
}

impl fmt::Debug for ConfirmationAuthorityIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfirmationAuthorityIdentity")
            .field("key_id", &self.key_id)
            .field("key_generation", &self.key_generation)
            .field("verifying_key_digest", &self.verifying_key_digest)
            .field("canonical_root_identity", &self.canonical_root_identity)
            .field("installation_identity", &self.installation_identity)
            .field("os_user_identity_digest", &self.os_user_identity_digest)
            .field("state_root_generation", &self.state_root_generation)
            .field("local_credential_identity", &self.local_credential_identity)
            .finish()
    }
}

#[cfg(test)]
struct ConfirmationAuthoritySigningKey {
    identity: ConfirmationAuthorityIdentity,
    seed: Zeroizing<[u8; 32]>,
}

#[cfg(test)]
impl fmt::Debug for ConfirmationAuthoritySigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfirmationAuthoritySigningKey")
            .field("identity", &self.identity)
            .field("secret_material", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
impl ConfirmationAuthoritySigningKey {
    /// Signs only storage's canonical confirmation-attestation bytes and only
    /// when the caller supplies the same public identity this signer opened.
    fn sign_confirmation_attestation(
        &self,
        identity: &ConfirmationAuthorityIdentity,
        payload: &ConfirmationAttestationPayload,
    ) -> Result<ConfirmationAttestation> {
        if identity != &self.identity {
            bail!("confirmation signing identity does not match the opened custody key");
        }
        if payload.canonical_root_identity != identity.canonical_root_identity
            || payload.installation_identity != identity.installation_identity
            || payload.os_user_identity_digest != identity.os_user_identity_digest
            || payload.state_root_generation != identity.state_root_generation
            || payload.signer_key_generation != identity.key_generation
        {
            bail!("confirmation attestation payload does not match the opened custody identity");
        }
        let bytes = confirmation_attestation_signing_bytes(payload, identity.key_id())
            .map_err(|_| anyhow!("confirmation attestation payload is invalid"))?;
        let signing_key = SigningKey::from_bytes(&self.seed);
        let signature = signing_key.sign(&bytes);
        Ok(ConfirmationAttestation {
            payload: payload.clone(),
            key_id: identity.key_id.clone(),
            signature_hex: hex_encode(&signature.to_bytes()),
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
struct CustodyCredential {
    canonical_root_identity: String,
    installation_identity: String,
    seed: [u8; 32],
}

impl Drop for CustodyCredential {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

struct OpenedCredential {
    #[cfg(test)]
    credential: CustodyCredential,
    identity: ConfirmationAuthorityIdentity,
}

trait ConfirmationCustody: Send + Sync {
    fn read(&self, locator: &CustodyLocator) -> Result<Option<Zeroizing<String>>>;
    fn write(&self, locator: &CustodyLocator, payload: &str) -> Result<()>;
    fn delete(&self, locator: &CustodyLocator) -> Result<()>;
    fn os_user_identity_digest(&self) -> Result<String>;
    fn before_database_pin(&self) -> Result<()> {
        Ok(())
    }
}

struct NativeCustody;

impl ConfirmationCustody for NativeCustody {
    fn read(&self, locator: &CustodyLocator) -> Result<Option<Zeroizing<String>>> {
        let entry = keyring::Entry::new(CUSTODY_SERVICE, &locator.account)
            .context("failed opening fixed confirmation OS custody entry")?;
        match entry.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(anyhow!(
                "failed reading fixed confirmation OS custody entry: {error}"
            )),
        }
    }

    fn write(&self, locator: &CustodyLocator, payload: &str) -> Result<()> {
        keyring::Entry::new(CUSTODY_SERVICE, &locator.account)
            .context("failed opening fixed confirmation OS custody entry")?
            .set_password(payload)
            .context("failed writing fixed confirmation OS custody entry")
    }

    fn delete(&self, locator: &CustodyLocator) -> Result<()> {
        match keyring::Entry::new(CUSTODY_SERVICE, &locator.account)
            .context("failed opening fixed confirmation OS custody entry")?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(anyhow!(
                "failed deleting newly-created confirmation OS credential: {error}"
            )),
        }
    }

    fn os_user_identity_digest(&self) -> Result<String> {
        current_os_user_identity_digest()
    }
}

struct CustodyLocator {
    account: String,
}

fn custody_locator(store: &ExecAssStore) -> CustodyLocator {
    let mut digest = Sha256::new();
    digest.update(CUSTODY_DOMAIN);
    digest.update([0]);
    digest.update(store.root_identity.as_bytes());
    CustodyLocator {
        account: format!("root-{}", hex_encode(&digest.finalize())),
    }
}

pub(super) fn activate_confirmation_authority(
    store: &ExecAssStore,
) -> Result<ConfirmationAuthorityIdentity> {
    activate_with_custody(store, Arc::new(NativeCustody), true).map(|opened| opened.identity)
}

/// Test-build-only authority activation that never touches the native
/// credential store. Production builds do not contain this symbol.
#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
#[doc(hidden)]
pub fn activate_test_confirmation_authority(
    store: &ExecAssStore,
    seed: [u8; 32],
) -> Result<ConfirmationAuthorityIdentity> {
    let credential = CustodyCredential {
        canonical_root_identity: store.root_identity.clone(),
        installation_identity: "746573742d696e7374616c6c6174696f".to_string(),
        seed,
    };
    let os_user_identity_digest =
        hex_encode(&Sha256::digest(b"carsinos.execass.test-current-user.v1"));
    let identity = identity_for(store, &credential, &os_user_identity_digest)?;
    let mut conn = store.connection()?;
    let tx = immediate_transaction(&mut conn)?;
    pin_or_validate(&tx, store, &credential, &identity, &os_user_identity_digest)?;
    ensure_local_owner_ingress_binding(&tx, store, &identity)?;
    tx.commit()
        .context("failed committing test-only confirmation authority")?;
    Ok(identity)
}

fn activate_with_custody(
    store: &ExecAssStore,
    custody: Arc<dyn ConfirmationCustody>,
    allow_create: bool,
) -> Result<OpenedCredential> {
    let _lock = lock_custody(store)?;
    let locator = custody_locator(store);
    let os_user_identity_digest = custody.os_user_identity_digest()?;
    let existing = custody.read(&locator)?;
    let (credential, created) = match existing {
        Some(payload) => (parse_credential_payload(&payload)?, false),
        None if allow_create => {
            if active_confirmation_pin_exists(store)? {
                bail!("pinned confirmation authority OS credential is missing");
            }
            (new_credential(store)?, true)
        }
        None => bail!("pinned confirmation authority OS credential is missing"),
    };
    let identity = identity_for(store, &credential, &os_user_identity_digest)?;

    if created {
        let encoded = encode_credential_payload(&credential);
        custody.write(&locator, &encoded)?;
    }

    let pin_result = (|| -> Result<()> {
        custody.before_database_pin()?;
        let mut conn = store.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        pin_or_validate(&tx, store, &credential, &identity, &os_user_identity_digest)?;
        ensure_local_owner_ingress_binding(&tx, store, &identity)?;
        tx.commit()
            .context("failed committing confirmation authority public pin")?;
        Ok(())
    })();
    if let Err(error) = pin_result {
        if created {
            let _ = custody.delete(&locator);
        }
        return Err(error);
    }
    Ok(OpenedCredential {
        #[cfg(test)]
        credential,
        identity,
    })
}

fn active_confirmation_pin_exists(store: &ExecAssStore) -> Result<bool> {
    store.connection()?.query_row(
        "SELECT EXISTS(SELECT 1 FROM execass_confirmation_authority_keys WHERE status='active')",
        [],
        |row| row.get(0),
    ).context("failed checking active confirmation authority pin")
}

fn lock_custody(store: &ExecAssStore) -> Result<File> {
    let path = store
        .root_path
        .join(".execass-confirmation-custody-v1.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .context("failed opening fixed confirmation custody lock")?;
    file.lock_exclusive()
        .context("failed locking fixed confirmation custody activation")?;
    Ok(file)
}

fn pin_or_validate(
    tx: &Transaction<'_>,
    store: &ExecAssStore,
    credential: &CustodyCredential,
    identity: &ConfirmationAuthorityIdentity,
    os_user_identity_digest: &str,
) -> Result<()> {
    let existing = tx.query_row(
        "SELECT key_id,key_generation,verifying_key_hex,verifying_key_digest,canonical_root_identity,installation_identity,os_user_identity_digest,state_root_generation FROM execass_confirmation_authority_keys WHERE status='active' ORDER BY key_generation,key_id LIMIT 2",
        [],
        |row| Ok(PinnedRow { key_id: row.get(0)?, key_generation: row.get(1)?, verifying_key_hex: row.get(2)?, verifying_key_digest: row.get(3)?, canonical_root_identity: row.get(4)?, installation_identity: row.get(5)?, os_user_identity_digest: row.get(6)?, state_root_generation: row.get(7)? }),
    ).optional().context("failed reading active confirmation authority pin")?;
    match existing {
        Some(row) => {
            if row.key_id != identity.key_id
                || row.key_generation != i64::try_from(KEY_GENERATION)?
                || row.verifying_key_hex != identity.verifying_key_hex
                || row.verifying_key_digest != identity.verifying_key_digest
                || row.canonical_root_identity != store.root_identity
                || row.installation_identity != credential.installation_identity
                || row.os_user_identity_digest != os_user_identity_digest
                || row.state_root_generation != i64::try_from(STATE_ROOT_GENERATION)?
            {
                bail!("fixed OS custody credential does not match the active confirmation authority pin");
            }
        }
        None => {
            tx.execute(
                "INSERT INTO execass_confirmation_authority_keys (key_id,key_generation,verifying_key_hex,verifying_key_digest,canonical_root_identity,installation_identity,os_user_identity_digest,state_root_generation,status,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9)",
                params![identity.key_id, i64::try_from(KEY_GENERATION)?, identity.verifying_key_hex, identity.verifying_key_digest, store.root_identity, credential.installation_identity, os_user_identity_digest, i64::try_from(STATE_ROOT_GENERATION)?, unix_ms()?],
            ).context("failed pinning fixed confirmation authority public identity")?;
        }
    }
    Ok(())
}

fn ensure_local_owner_ingress_binding(
    tx: &Transaction<'_>,
    store: &ExecAssStore,
    identity: &ConfirmationAuthorityIdentity,
) -> Result<()> {
    let mut statement = tx.prepare(
        "SELECT binding_id,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status FROM execass_owner_ingress_bindings WHERE actor_type='human_local' AND status='active' ORDER BY binding_id LIMIT 2",
    ).context("failed reading active fixed local owner ingress binding")?;
    let rows = statement
        .query_map([], |row| {
            Ok(LocalBindingRow {
                binding_id: row.get(0)?,
                credential_identity: row.get(1)?,
                authenticated_ingress: row.get(2)?,
                channel_assurance: row.get(3)?,
                provider_event_required: row.get(4)?,
                status: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed collecting active fixed local owner ingress binding")?;
    let binding_id = local_binding_id(store, identity);
    match rows.as_slice() {
        [] => {
            tx.execute(
                "INSERT INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES (?1,'human_local',?2,'native-control','interactive-local',0,'active',?3)",
                params![binding_id, identity.local_credential_identity, unix_ms()?],
            ).context("failed pinning fixed local owner ingress binding")?;
        }
        [row]
            if row.binding_id == binding_id
                && row.credential_identity == identity.local_credential_identity
                && row.authenticated_ingress == "native-control"
                && row.channel_assurance == "interactive-local"
                && row.provider_event_required == 0
                && row.status == "active" => {}
        _ => bail!("active local owner ingress binding does not match fixed OS custody identity"),
    }
    Ok(())
}

fn local_binding_id(store: &ExecAssStore, identity: &ConfirmationAuthorityIdentity) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.fixed-local-owner-binding.v1");
    digest.update([0]);
    digest.update(store.root_identity.as_bytes());
    digest.update([0]);
    digest.update(identity.local_credential_identity.as_bytes());
    format!("local-owner-v1-{}", hex_encode(&digest.finalize()))
}

struct LocalBindingRow {
    binding_id: String,
    credential_identity: String,
    authenticated_ingress: String,
    channel_assurance: String,
    provider_event_required: i64,
    status: String,
}

struct PinnedRow {
    key_id: String,
    key_generation: i64,
    verifying_key_hex: String,
    verifying_key_digest: String,
    canonical_root_identity: String,
    installation_identity: String,
    os_user_identity_digest: String,
    state_root_generation: i64,
}

fn new_credential(store: &ExecAssStore) -> Result<CustodyCredential> {
    let mut installation = [0u8; 16];
    let mut seed = [0u8; 32];
    getrandom::fill(&mut installation)
        .context("OS CSPRNG failed generating confirmation installation identity")?;
    getrandom::fill(&mut seed).context("OS CSPRNG failed generating confirmation signing seed")?;
    Ok(CustodyCredential {
        canonical_root_identity: store.root_identity.clone(),
        installation_identity: hex_encode(&installation),
        seed,
    })
}

fn identity_for(
    store: &ExecAssStore,
    credential: &CustodyCredential,
    os_user_identity_digest: &str,
) -> Result<ConfirmationAuthorityIdentity> {
    if credential.canonical_root_identity != store.root_identity {
        bail!("confirmation OS credential belongs to a different canonical state root");
    }
    if os_user_identity_digest.len() != 64
        || !os_user_identity_digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        bail!("OS user identity digest is malformed");
    }
    let signing = SigningKey::from_bytes(&credential.seed);
    let verifying_key = signing.verifying_key().to_bytes();
    let verifying_key_hex = hex_encode(&verifying_key);
    let verifying_key_digest = hex_encode(&Sha256::digest(verifying_key));
    let mut id = Sha256::new();
    id.update(CUSTODY_DOMAIN);
    id.update([0]);
    id.update(verifying_key);
    id.update([0]);
    id.update(store.root_identity.as_bytes());
    id.update([0]);
    id.update(credential.installation_identity.as_bytes());
    id.update([0]);
    id.update(os_user_identity_digest.as_bytes());
    id.update([0]);
    id.update(STATE_ROOT_GENERATION.to_le_bytes());
    Ok(ConfirmationAuthorityIdentity {
        key_id: format!("confirmation-v1-{}", hex_encode(&id.finalize())),
        key_generation: KEY_GENERATION,
        verifying_key_hex,
        verifying_key_digest,
        canonical_root_identity: store.root_identity.clone(),
        installation_identity: credential.installation_identity.clone(),
        os_user_identity_digest: os_user_identity_digest.to_owned(),
        state_root_generation: STATE_ROOT_GENERATION,
        local_credential_identity: format!("os-user:{os_user_identity_digest}"),
    })
}

fn encode_credential_payload(credential: &CustodyCredential) -> Zeroizing<String> {
    Zeroizing::new(format!(
        "carsinos-confirmation-custody-v1|{}|{}|{}",
        credential.canonical_root_identity,
        credential.installation_identity,
        hex_encode(&credential.seed)
    ))
}

fn parse_credential_payload(payload: &str) -> Result<CustodyCredential> {
    let mut parts = payload.split('|');
    let (
        Some(version),
        Some(canonical_root_identity),
        Some(installation_identity),
        Some(seed_hex),
        None,
    ) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    )
    else {
        bail!("confirmation OS credential is malformed");
    };
    if version != "carsinos-confirmation-custody-v1"
        || canonical_root_identity.len() != 71
        || !canonical_root_identity.starts_with("sha256:")
        || !canonical_root_identity[7..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || installation_identity.len() != 32
        || !installation_identity
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        bail!("confirmation OS credential is malformed");
    }
    let mut seed = [0u8; 32];
    decode_hex_into(seed_hex, &mut seed)
        .map_err(|_| anyhow!("confirmation OS credential is malformed"))?;
    Ok(CustodyCredential {
        canonical_root_identity: canonical_root_identity.to_owned(),
        installation_identity: installation_identity.to_owned(),
        seed,
    })
}

fn unix_ms() -> Result<i64> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock is before UNIX epoch")?
            .as_millis(),
    )
    .context("system clock exceeds supported range")
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}

fn decode_hex_into(text: &str, output: &mut [u8]) -> Result<()> {
    if text.len() != output.len() * 2 {
        bail!("invalid hex length");
    }
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = (hex_nibble(text.as_bytes()[index * 2])? << 4)
            | hex_nibble(text.as_bytes()[index * 2 + 1])?;
    }
    Ok(())
}
fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => bail!("invalid lowercase hex"),
    }
}

#[cfg(any(windows, test))]
fn windows_sid_digest_from_bytes(sid: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.windows-token-sid.v1");
    digest.update([0]);
    digest.update(sid);
    hex_encode(&digest.finalize())
}

#[cfg(windows)]
fn current_os_user_identity_digest() -> Result<String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::Security::{
        GetLengthSid, GetTokenInformation, IsValidSid, TokenUser, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            bail!("failed opening current process token: {}", GetLastError());
        }
        struct Token(HANDLE);
        impl Drop for Token {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let _token = Token(token);
        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
            bail!("current process token user identity is unavailable");
        }
        let mut buffer = vec![0u8; needed as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast::<c_void>(),
            needed,
            &mut needed,
        ) == 0
        {
            bail!(
                "failed reading current process token user identity: {}",
                GetLastError()
            );
        }
        let token_user = buffer.as_ptr().cast::<TOKEN_USER>();
        let sid = (*token_user).User.Sid;
        if sid.is_null() || IsValidSid(sid) == 0 {
            bail!("current process token SID is invalid");
        }
        let sid_length = GetLengthSid(sid) as usize;
        let start = buffer.as_ptr() as usize;
        let end = start
            .checked_add(buffer.len())
            .ok_or_else(|| anyhow!("current process token SID bounds overflow"))?;
        let sid_start = sid as usize;
        let sid_end = sid_start
            .checked_add(sid_length)
            .ok_or_else(|| anyhow!("current process token SID bounds overflow"))?;
        if sid_length == 0 || sid_start < start || sid_end > end {
            bail!("current process token SID is outside the returned token buffer");
        }
        Ok(windows_sid_digest_from_bytes(std::slice::from_raw_parts(
            sid.cast::<u8>(),
            sid_length,
        )))
    }
}

#[cfg(target_os = "macos")]
fn current_os_user_identity_digest() -> Result<String> {
    let uid = unsafe { libc::geteuid() };
    Ok(hex_encode(&Sha256::digest(
        format!("carsinos.execass.macos-euid.v1:{uid}").as_bytes(),
    )))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn current_os_user_identity_digest() -> Result<String> {
    bail!("fixed confirmation OS custody is supported only on Windows and macOS")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_execass_fresh_root, AppPaths};
    use std::collections::HashMap;
    use std::sync::{Barrier, Mutex};
    use std::thread;
    use tempfile::TempDir;

    #[derive(Default)]
    struct MemoryCustody {
        values: Mutex<HashMap<String, String>>,
        user_digest: Mutex<String>,
        fail_pin: Mutex<bool>,
        writes: Mutex<u32>,
        deletes: Mutex<u32>,
    }
    impl MemoryCustody {
        fn new(user_digest: &str) -> Self {
            Self {
                user_digest: Mutex::new(user_digest.to_owned()),
                ..Self::default()
            }
        }
        fn value(&self, store: &ExecAssStore) -> Option<String> {
            self.values
                .lock()
                .unwrap()
                .get(&custody_locator(store).account)
                .cloned()
        }
        fn set_value(&self, store: &ExecAssStore, value: Option<String>) {
            let account = custody_locator(store).account;
            let mut values = self.values.lock().unwrap();
            if let Some(value) = value {
                values.insert(account, value);
            } else {
                values.remove(&account);
            }
        }
    }
    impl ConfirmationCustody for MemoryCustody {
        fn read(&self, locator: &CustodyLocator) -> Result<Option<Zeroizing<String>>> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&locator.account)
                .cloned()
                .map(Zeroizing::new))
        }
        fn write(&self, locator: &CustodyLocator, payload: &str) -> Result<()> {
            *self.writes.lock().unwrap() += 1;
            self.values
                .lock()
                .unwrap()
                .insert(locator.account.clone(), payload.to_owned());
            Ok(())
        }
        fn delete(&self, locator: &CustodyLocator) -> Result<()> {
            *self.deletes.lock().unwrap() += 1;
            self.values.lock().unwrap().remove(&locator.account);
            Ok(())
        }
        fn os_user_identity_digest(&self) -> Result<String> {
            Ok(self.user_digest.lock().unwrap().clone())
        }
        fn before_database_pin(&self) -> Result<()> {
            if *self.fail_pin.lock().unwrap() {
                bail!("injected database pin failure");
            }
            Ok(())
        }
    }
    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }
    fn store() -> (TempDir, ExecAssStore) {
        let dir = TempDir::new_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = AppPaths::from_root(dir.path());
        init_execass_fresh_root(&paths).unwrap();
        (dir, ExecAssStore::open(&paths).unwrap())
    }
    fn count_pin(store: &ExecAssStore) -> i64 {
        store
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM execass_confirmation_authority_keys WHERE status='active'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }
    fn count_local_binding(store: &ExecAssStore) -> i64 {
        store.connection().unwrap().query_row("SELECT COUNT(*) FROM execass_owner_ingress_bindings WHERE actor_type='human_local' AND status='active'", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn first_activation_reopens_and_signer_is_redacted() {
        let (_dir, store) = store();
        let custody = Arc::new(MemoryCustody::new(&digest('a')));
        let first = activate_with_custody(&store, custody.clone(), true).unwrap();
        let second = activate_with_custody(&store, custody.clone(), true).unwrap();
        assert_eq!(first.identity, second.identity);
        assert_eq!(count_pin(&store), 1);
        assert_eq!(count_local_binding(&store), 1);
        assert_eq!(
            first.identity.local_credential_identity(),
            format!("os-user:{}", digest('a'))
        );
        assert_eq!(*custody.writes.lock().unwrap(), 1);
        let signer = ConfirmationAuthoritySigningKey {
            identity: first.identity.clone(),
            seed: Zeroizing::new(first.credential.seed),
        };
        let output = format!("{signer:?}");
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains(&hex_encode(&signer.seed[..])));
        let json = serde_json::to_string(&first.identity).unwrap();
        assert!(!json.contains("seed"));
        assert!(!json.contains(&hex_encode(&signer.seed[..])));
    }

    #[test]
    fn concurrent_activation_converges_on_one_pin() {
        let (_dir, store) = store();
        let custody = Arc::new(MemoryCustody::new(&digest('b')));
        let barrier = Arc::new(Barrier::new(8));
        let mut joins = Vec::new();
        for _ in 0..8 {
            let store = store.clone();
            let custody = custody.clone();
            let barrier = barrier.clone();
            joins.push(thread::spawn(move || {
                barrier.wait();
                activate_with_custody(&store, custody, true).map(|opened| opened.identity)
            }));
        }
        let identities: Vec<_> = joins
            .into_iter()
            .map(|join| join.join().unwrap().unwrap())
            .collect();
        assert!(identities.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(count_pin(&store), 1);
        assert_eq!(*custody.writes.lock().unwrap(), 1);
    }

    #[test]
    fn missing_malformed_and_mismatched_credential_fail_closed() {
        let (_dir, store) = store();
        let custody = Arc::new(MemoryCustody::new(&digest('c')));
        activate_with_custody(&store, custody.clone(), true).unwrap();
        custody.set_value(&store, None);
        assert!(activate_with_custody(&store, custody.clone(), true).is_err());
        assert_eq!(count_pin(&store), 1);
        assert_eq!(*custody.writes.lock().unwrap(), 1);
        custody.set_value(&store, Some("bad".to_owned()));
        assert!(activate_with_custody(&store, custody.clone(), true).is_err());
        assert_eq!(*custody.writes.lock().unwrap(), 1);
        custody.set_value(&store, Some("carsinos-confirmation-custody-v1|sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|00000000000000000000000000000000|0000000000000000000000000000000000000000000000000000000000000000".to_owned()));
        assert!(activate_with_custody(&store, custody, true).is_err());
    }

    #[test]
    fn different_root_and_user_do_not_validate() {
        let (_dir, current_store) = store();
        let custody = Arc::new(MemoryCustody::new(&digest('d')));
        activate_with_custody(&current_store, custody.clone(), true).unwrap();
        let copied_credential = custody.value(&current_store).unwrap();
        *custody.user_digest.lock().unwrap() = digest('e');
        assert!(activate_with_custody(&current_store, custody.clone(), true).is_err());
        let (_other_dir, other) = store();
        custody.set_value(&other, Some(copied_credential));
        assert!(activate_with_custody(&other, custody, true).is_err());
    }

    #[test]
    fn pin_failure_deletes_new_credential_and_leaves_zero_pin() {
        let (_dir, store) = store();
        let custody = Arc::new(MemoryCustody::new(&digest('f')));
        *custody.fail_pin.lock().unwrap() = true;
        assert!(activate_with_custody(&store, custody.clone(), true).is_err());
        assert!(custody.value(&store).is_none());
        assert_eq!(count_pin(&store), 0);
        assert_eq!(count_local_binding(&store), 0);
        assert_eq!(*custody.deletes.lock().unwrap(), 1);
    }

    #[test]
    fn conflicting_local_binding_fails_closed_and_rolls_back_new_credential() {
        let (_dir, store) = store();
        store.connection().unwrap().execute(
            "INSERT INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES ('conflict','human_local','other','native-control','interactive-local',0,'active',1)",
            [],
        ).unwrap();
        let custody = Arc::new(MemoryCustody::new(&digest('a')));
        assert!(activate_with_custody(&store, custody.clone(), true).is_err());
        assert_eq!(count_pin(&store), 0);
        assert!(custody.value(&store).is_none());
        assert_eq!(*custody.deletes.lock().unwrap(), 1);
    }

    #[test]
    fn signing_requires_matching_identity_and_no_static_secret_literal() {
        let (_dir, store) = store();
        let custody = Arc::new(MemoryCustody::new(&digest('a')));
        let opened = activate_with_custody(&store, custody, true).unwrap();
        let signer = ConfirmationAuthoritySigningKey {
            identity: opened.identity.clone(),
            seed: Zeroizing::new(opened.credential.seed),
        };
        let mut wrong = opened.identity.clone();
        wrong.key_id.push('x');
        let payload = ConfirmationAttestationPayload {
            actor_type: "human_local".into(),
            credential_identity: "native".into(),
            authenticated_ingress: "local".into(),
            channel_assurance: "native".into(),
            request_correlation_id: "request".into(),
            source_message_id: None,
            provider_event_id: None,
            normalized_intent_digest: digest('1'),
            policy_revision: 1,
            decision_id: "decision".into(),
            decision_revision: 1,
            decision_result: "confirm_and_continue".into(),
            canonical_manifest_digest: digest('2'),
            selected_logical_action_id: "action".into(),
            selected_action_digest: digest('3'),
            declared_consequence_digest: digest('4'),
            challenge_nonce_digest: digest('5'),
            challenge_expires_at_ms: 2,
            issued_at_ms: 1,
            canonical_root_identity: opened.identity.canonical_root_identity.clone(),
            installation_identity: opened.identity.installation_identity.clone(),
            os_user_identity_digest: opened.identity.os_user_identity_digest.clone(),
            state_root_generation: opened.identity.state_root_generation,
            signer_key_generation: opened.identity.key_generation,
        };
        assert!(signer
            .sign_confirmation_attestation(&wrong, &payload)
            .is_err());
        let mut mismatched_payload = payload.clone();
        mismatched_payload.installation_identity.push('x');
        assert!(signer
            .sign_confirmation_attestation(&opened.identity, &mismatched_payload)
            .is_err());
        let source = include_str!("confirmation_custody.rs");
        assert!(!source.contains(&"0123456789abcdef".repeat(4)));
    }

    #[test]
    fn custody_payload_roundtrips_and_sid_digest_ignores_address() {
        let credential = CustodyCredential {
            canonical_root_identity: format!("sha256:{}", digest('a')),
            installation_identity: "b".repeat(32),
            seed: [7; 32],
        };
        let encoded = encode_credential_payload(&credential);
        let parsed = parse_credential_payload(&encoded).unwrap();
        assert_eq!(
            parsed.canonical_root_identity,
            credential.canonical_root_identity
        );
        assert_eq!(
            parsed.installation_identity,
            credential.installation_identity
        );
        assert_eq!(parsed.seed, credential.seed);
        let first = vec![1u8, 2, 3, 4];
        let second = first.clone();
        assert_ne!(first.as_ptr(), second.as_ptr());
        assert_eq!(
            windows_sid_digest_from_bytes(&first),
            windows_sid_digest_from_bytes(&second)
        );
    }
}
