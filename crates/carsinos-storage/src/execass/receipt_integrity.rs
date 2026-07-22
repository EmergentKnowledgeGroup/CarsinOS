//! OS-bound receipt-key custody and the external receipt high-water anchor.
//!
//! The anchor directory is a deterministic sibling of the swappable state
//! root.  It is therefore not captured by a rollback of `carsinos.db` or the
//! state-root archive.  Versioned anchor documents are never overwritten; only
//! the small `current.json` pointer document is atomically replaced.

use super::receipt::{verify_receipt_history, ReceiptHistoryFailure};
use crate::{execass_schema_is_exact, open_sqlite_connection, AppPaths};
use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

const ANCHOR_DOCUMENT_VERSION: &str = "carsinos.execass.anchor.v1";
const ANCHOR_INTEGRITY_ALGORITHM: &str = "hmac-sha256";
const ANCHOR_INTEGRITY_DOMAIN: &[u8] = b"carsinos.execass.anchor.document.v1\0";
const ANCHOR_ROTATION_DOMAIN: &[u8] = b"carsinos.execass.anchor.rotation.previous.v1\0";
const RECEIPT_COMMIT_CONFIRMATION_DOMAIN: &[u8] =
    b"carsinos.execass.receipt-commit-confirmation.v1\0";
const KEY_REGISTRY_INTEGRITY_DOMAIN: &[u8] = b"carsinos.execass.key-registry.v1\0";
const KEY_BYTES: usize = 32;
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptKeyRef {
    pub key_id: String,
    pub key_generation: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnchorCommitInput {
    pub state_root_generation: i64,
    pub anchor_generation: i64,
    pub receipt_count: i64,
    pub receipt_head_digest: Option<String>,
    pub key: ReceiptKeyRef,
    pub transaction_id: String,
    pub external_receipt_digest: String,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedAnchor {
    pub transaction_id: String,
    pub anchor_generation: i64,
    pub prepared_document_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityStatus {
    Uninitialized,
    Prepared {
        transaction_id: String,
        anchor_generation: i64,
    },
    Trusted {
        anchor_generation: i64,
        receipt_count: i64,
        receipt_head_digest: Option<String>,
        key: ReceiptKeyRef,
    },
    KeyLost {
        key: ReceiptKeyRef,
    },
    Mismatch {
        reason: String,
    },
    Quarantined {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityRecovery {
    Healthy,
    FinalizedInterruptedCommit { transaction_id: String },
    RestoredLastProvenPair { anchor_generation: Option<i64> },
    Quarantined { reason: String },
}

/// Testable boundary around DPAPI/Keychain custody.
///
/// Production callers use [`ReceiptIntegrityStore::open`].  The injectable
/// constructor exists so failpoint and key-loss behavior can be proven without
/// exporting a production key from its OS-protected backend.
pub(crate) trait ReceiptKeyProtector: Send + Sync {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>>;
    fn load(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>>;
    fn delete(&self, key: &ReceiptKeyRef) -> Result<()>;
}

pub(crate) trait IntegrityFailpoints: Send + Sync {
    fn hit(&self, name: &'static str) -> Result<()>;
}

#[derive(Default)]
struct NoFailpoints;

impl IntegrityFailpoints for NoFailpoints {
    fn hit(&self, _name: &'static str) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct ReceiptIntegrityStore {
    db_path: PathBuf,
    anchor_dir: PathBuf,
    root_identity: String,
    protector: Arc<dyn ReceiptKeyProtector>,
    failpoints: Arc<dyn IntegrityFailpoints>,
}

impl fmt::Debug for ReceiptIntegrityStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReceiptIntegrityStore")
            .field("database_configured", &true)
            .field("anchor_directory_configured", &true)
            .field("root_identity", &self.root_identity)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AnchorDocument {
    document_version: String,
    phase: AnchorPhase,
    root_identity: String,
    state_root_generation: i64,
    anchor_generation: i64,
    receipt_count: i64,
    receipt_head_digest: Option<String>,
    key_id: String,
    key_generation: i64,
    transaction_id: String,
    pub(super) external_receipt_digest: String,
    previous_finalized_document_digest: Option<String>,
    prepared_document_digest: Option<String>,
    occurred_at: i64,
    integrity_algorithm: String,
    integrity_tag: Option<String>,
    pub(super) previous_key_integrity_tag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AnchorPhase {
    Prepared,
    Finalized,
}

#[derive(Debug, Clone)]
struct AnchorRow {
    root_identity: String,
    state_root_generation: i64,
    anchor_generation: i64,
    status: String,
    receipt_count: i64,
    receipt_head_digest: Option<String>,
    key: ReceiptKeyRef,
    transaction_id: String,
    external_receipt_digest: String,
    prepared_document_digest: String,
    receipt_commit_confirmed: bool,
    receipt_committed_at: Option<i64>,
    receipt_commit_confirmation_tag: Option<String>,
    finalized_document_digest: Option<String>,
    prepared_at: i64,
}

#[derive(Debug)]
struct ReceiptKeyRow {
    status: String,
    rotated_from_key_id: Option<String>,
    rotated_from_key_generation: Option<i64>,
    activated_anchor_generation: Option<i64>,
}

#[derive(Serialize)]
struct ReceiptKeyRegistryIdentity<'a> {
    root_identity: &'a str,
    key_id: &'a str,
    key_generation: i64,
    rotated_from_key_id: Option<&'a str>,
    rotated_from_key_generation: Option<i64>,
    created_at: i64,
}

#[derive(Serialize)]
struct ReceiptCommitConfirmation<'a> {
    document_version: &'static str,
    root_identity: &'a str,
    state_root_generation: i64,
    anchor_generation: i64,
    transaction_id: &'a str,
    receipt_count: i64,
    receipt_head_digest: Option<&'a str>,
    key_id: &'a str,
    key_generation: i64,
    prepared_document_digest: &'a str,
    committed_at: i64,
}

impl ReceiptIntegrityStore {
    pub fn open(paths: &AppPaths) -> Result<Self> {
        #[cfg(all(test, not(any(windows, target_os = "macos"))))]
        {
            Self::open_for_test_inner(paths)
        }
        #[cfg(not(all(test, not(any(windows, target_os = "macos")))))]
        {
            let (anchor_dir, root_identity) = external_anchor_location(paths)?;
            fs::create_dir_all(&anchor_dir)
                .context("failed creating the external receipt-integrity directory")?;
            let protector = production_protector(&anchor_dir, &root_identity)?;
            Self::with_protector_and_failpoints(
                paths,
                anchor_dir,
                root_identity,
                protector,
                Arc::new(NoFailpoints),
            )
        }
    }

    /// Test-only receipt custody for dependent crates that cannot inherit this
    /// crate's `cfg(test)`. Production callers must use [`Self::open`].
    #[cfg(feature = "execass-test-confirmation-runtime")]
    #[doc(hidden)]
    pub fn open_for_test(paths: &AppPaths) -> Result<Self> {
        Self::open_for_test_inner(paths)
    }

    #[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
    fn open_for_test_inner(paths: &AppPaths) -> Result<Self> {
        let (anchor_dir, root_identity) = external_anchor_location(paths)?;
        fs::create_dir_all(&anchor_dir)
            .context("failed creating the external test receipt-integrity directory")?;
        let protector = Arc::new(FeatureTestReceiptKeyProtector::new(
            anchor_dir.join("test-only-keys"),
        )?);
        Self::with_protector_and_failpoints(
            paths,
            anchor_dir,
            root_identity,
            protector,
            Arc::new(NoFailpoints),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_protector(
        paths: &AppPaths,
        anchor_dir: PathBuf,
        protector: Arc<dyn ReceiptKeyProtector>,
    ) -> Result<Self> {
        let (_, root_identity) = external_anchor_location(paths)?;
        Self::with_protector_and_failpoints(
            paths,
            anchor_dir,
            root_identity,
            protector,
            Arc::new(NoFailpoints),
        )
    }

    pub(crate) fn with_protector_and_failpoints(
        paths: &AppPaths,
        anchor_dir: PathBuf,
        root_identity: String,
        protector: Arc<dyn ReceiptKeyProtector>,
        failpoints: Arc<dyn IntegrityFailpoints>,
    ) -> Result<Self> {
        if !paths.db_path.is_file() || !execass_schema_is_exact(&paths.db_path)? {
            bail!("refusing receipt-integrity access to a non-canonical ExecAss database");
        }
        fs::create_dir_all(&anchor_dir).context("failed creating receipt-integrity directory")?;
        if path_is_within(&anchor_dir, &paths.root)? {
            bail!("receipt-integrity anchor directory must live outside the state root");
        }
        sync_directory(&anchor_dir).context("failed syncing receipt-integrity directory")?;
        Ok(Self {
            db_path: paths.db_path.clone(),
            anchor_dir,
            root_identity,
            protector,
            failpoints,
        })
    }

    pub fn anchor_directory(&self) -> &Path {
        &self.anchor_dir
    }

    pub fn root_identity(&self) -> &str {
        &self.root_identity
    }

    pub fn provision_initial_key(&self, key_id: &str) -> Result<ReceiptKeyRef> {
        validate_identifier("key_id", key_id)?;
        let key = ReceiptKeyRef {
            key_id: key_id.to_owned(),
            key_generation: 1,
        };
        let mut connection = open_sqlite_connection(&self.db_path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("failed fencing initial receipt-key provisioning")?;
        if self.current_path().exists()
            || latest_anchor_row_with_conn(&transaction)?.is_some()
            || receipt_key_count_with_conn(&transaction)? != 0
        {
            bail!("initial receipt key cannot be provisioned after receipt-key history exists");
        }

        // A crash may have occurred after OS custody succeeded but before the
        // registry transaction committed.  With no registry history, this
        // deterministic reference is an orphan and is safe to replace.
        let _ = self.protector.delete(&key);
        self.failpoints.hit("key.create.before")?;
        let create_result = (|| -> Result<()> {
            let material = self.protector.create(&key)?;
            if material.len() != KEY_BYTES {
                bail!("OS receipt-key backend returned an invalid key length");
            }
            self.failpoints.hit("key.create.after")?;
            let created_at = unix_timestamp()?;
            let registry_integrity_tag = receipt_key_registry_integrity_tag(
                &material,
                &self.root_identity,
                &key,
                None,
                None,
                created_at,
            )?;
            transaction.execute(
                r#"
                INSERT INTO execass_receipt_keys (
                  key_id,key_generation,status,rotated_from_key_id,
                  rotated_from_key_generation,created_at,registry_integrity_tag,
                  activated_anchor_generation
                ) VALUES (?1,1,'provisioned',NULL,NULL,?2,?3,NULL)
                "#,
                params![key.key_id, created_at, registry_integrity_tag],
            )?;
            transaction.commit()?;
            Ok(())
        })();
        if let Err(error) = create_result {
            let _ = self.protector.delete(&key);
            return Err(error).context("failed provisioning initial receipt key");
        }
        Ok(key)
    }

    pub fn current_append_key(&self) -> Result<Option<ReceiptKeyRef>> {
        let connection = open_sqlite_connection(&self.db_path)?;
        if let Some(provisioned) = provisioned_key_with_conn(&connection)? {
            self.load_key(&provisioned)
                .context("provisioned receipt key is unavailable")?;
            return Ok(Some(provisioned));
        }
        let Some(anchor) = latest_anchor_row_with_conn(&connection)? else {
            return Ok(None);
        };
        if anchor.status != "finalized" {
            bail!("receipt key cannot be selected while anchor state is not finalized");
        }
        require_registered_key_status(&connection, &anchor.key, "active")?;
        self.load_key(&anchor.key)
            .context("active receipt key is unavailable")?;
        Ok(Some(anchor.key))
    }

    pub fn rotate_key(&self, new_key_id: &str) -> Result<ReceiptKeyRef> {
        validate_identifier("key_id", new_key_id)?;
        let mut connection = open_sqlite_connection(&self.db_path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("failed fencing receipt-key rotation")?;
        let current = latest_anchor_row_with_conn(&transaction)?
            .context("receipt key rotation requires a finalized current anchor")?;
        if current.status != "finalized" {
            bail!("receipt key rotation requires a finalized current anchor");
        }
        require_registered_key_status(&transaction, &current.key, "active")?;
        if provisioned_key_with_conn(&transaction)?.is_some() {
            bail!("a receipt-key rotation is already provisioned");
        }
        if has_external_prepared_document(&self.anchor_dir)? {
            bail!("unrepresented external receipt-anchor preparation blocks key rotation");
        }
        self.protector
            .load(&current.key)
            .context("current receipt key is unavailable; rotation is forbidden")?;
        let next = ReceiptKeyRef {
            key_id: new_key_id.to_owned(),
            key_generation: current
                .key
                .key_generation
                .checked_add(1)
                .context("receipt key generation overflow")?,
        };
        // As with initial provisioning, no registry row means this exact
        // candidate can only be an interrupted pre-commit orphan.
        let _ = self.protector.delete(&next);
        self.failpoints.hit("key.rotate.before")?;
        let create_result = (|| -> Result<()> {
            let material = self.protector.create(&next)?;
            if material.len() != KEY_BYTES {
                bail!("OS receipt-key backend returned an invalid key length");
            }
            self.failpoints.hit("key.rotate.after")?;
            let created_at = unix_timestamp()?;
            let registry_integrity_tag = receipt_key_registry_integrity_tag(
                &material,
                &self.root_identity,
                &next,
                Some(current.key.key_id.as_str()),
                Some(current.key.key_generation),
                created_at,
            )?;
            transaction.execute(
                r#"
                INSERT INTO execass_receipt_keys (
                  key_id,key_generation,status,rotated_from_key_id,
                  rotated_from_key_generation,created_at,registry_integrity_tag,
                  activated_anchor_generation
                ) VALUES (?1,?2,'provisioned',?3,?4,?5,?6,NULL)
                "#,
                params![
                    next.key_id,
                    next.key_generation,
                    current.key.key_id,
                    current.key.key_generation,
                    created_at,
                    registry_integrity_tag,
                ],
            )?;
            transaction.commit()?;
            Ok(())
        })();
        if let Err(error) = create_result {
            let _ = self.protector.delete(&next);
            return Err(error).context("failed rotating receipt key");
        }
        Ok(next)
    }

    pub(crate) fn load_key(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let material = self.protector.load(key)?;
        if material.len() != KEY_BYTES {
            bail!("OS receipt-key backend returned an invalid key length");
        }
        Ok(material)
    }

    pub(crate) fn prepare_anchor(&self, input: &AnchorCommitInput) -> Result<PreparedAnchor> {
        let mut connection = open_sqlite_connection(&self.db_path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("failed fencing receipt-anchor preparation")?;
        let prepared = self.prepare_anchor_in_transaction(&transaction, input)?;
        transaction
            .commit()
            .context("failed committing prepared receipt anchor")?;
        self.failpoints.hit("db.prepare.after")?;
        Ok(prepared)
    }

    /// Prepares the external anchor and records its database high-water row
    /// through an already-open writer transaction.
    ///
    /// Decision resolution needs this form because its canonical receipt is
    /// bound to the outbox global sequence allocated earlier in the *same*
    /// transaction.  The caller must either confirm the prepared anchor and
    /// commit, or invoke integrity recovery after rollback; recovery removes
    /// an external prepared document whose database row did not commit.
    pub(crate) fn prepare_anchor_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        input: &AnchorCommitInput,
    ) -> Result<PreparedAnchor> {
        validate_anchor_input(input)?;
        if self.quarantine_path().exists() {
            bail!("receipt integrity is quarantined");
        }
        let previous = latest_anchor_row_with_conn(transaction)?;
        validate_anchor_progression(previous.as_ref(), input, &self.root_identity)?;
        validate_key_for_anchor_with_conn(transaction, previous.as_ref(), &input.key)?;
        let current_key_material = self
            .load_key(&input.key)
            .context("receipt anchor key is unavailable")?;
        let previous_key_material = previous
            .as_ref()
            .filter(|row| row.key != input.key)
            .map(|row| self.load_key(&row.key))
            .transpose()
            .context("previous receipt key is unavailable for rotation cross-signing")?;
        if let Some(previous) = previous.as_ref() {
            self.verify_finalized_row(previous)
                .context("current receipt anchor is not trusted for advancement")?;
            self.load_key(&previous.key)
                .context("current receipt key is unavailable for advancement")?;
        } else if self.current_path().exists() {
            bail!("external receipt anchor exists without database high-water state");
        }
        let previous_digest = previous
            .as_ref()
            .and_then(|row| row.finalized_document_digest.clone());
        let mut prepared_document = AnchorDocument {
            document_version: ANCHOR_DOCUMENT_VERSION.to_owned(),
            phase: AnchorPhase::Prepared,
            root_identity: self.root_identity.clone(),
            state_root_generation: input.state_root_generation,
            anchor_generation: input.anchor_generation,
            receipt_count: input.receipt_count,
            receipt_head_digest: input.receipt_head_digest.clone(),
            key_id: input.key.key_id.clone(),
            key_generation: input.key.key_generation,
            transaction_id: input.transaction_id.clone(),
            external_receipt_digest: input.external_receipt_digest.clone(),
            previous_finalized_document_digest: previous_digest,
            prepared_document_digest: None,
            occurred_at: input.occurred_at,
            integrity_algorithm: ANCHOR_INTEGRITY_ALGORITHM.to_owned(),
            integrity_tag: None,
            previous_key_integrity_tag: None,
        };
        sign_anchor_document(
            &mut prepared_document,
            &current_key_material,
            previous_key_material.as_ref().map(|key| key.as_slice()),
        )?;
        let prepared_bytes = serialize_document(&prepared_document)?;
        let prepared_digest = sha256_hex(&prepared_bytes);
        let prepared_path = self.prepared_path(input.anchor_generation, &input.transaction_id);

        let prepare_result = (|| -> Result<()> {
            self.failpoints.hit("anchor.prepare.begin")?;
            durable_write_atomic(
                &prepared_path,
                &prepared_bytes,
                "anchor.prepare",
                self.failpoints.as_ref(),
            )?;
            self.failpoints.hit("db.prepare.before")?;
            transaction
                .execute(
                    r#"
                INSERT INTO execass_receipt_anchor_state (
                  anchor_id, root_identity, state_root_generation, anchor_generation,
                  status, receipt_count, receipt_head_digest, key_id, key_generation,
                  transaction_id, external_receipt_digest, prepared_document_digest,
                  prepared_at
                ) VALUES (?1,?2,?3,?4,'prepared',?5,?6,?7,?8,?9,?10,?11,?12)
                "#,
                    params![
                        anchor_id(input.anchor_generation, &input.transaction_id),
                        self.root_identity,
                        input.state_root_generation,
                        input.anchor_generation,
                        input.receipt_count,
                        input.receipt_head_digest,
                        input.key.key_id,
                        input.key.key_generation,
                        input.transaction_id,
                        input.external_receipt_digest,
                        prepared_digest,
                        input.occurred_at,
                    ],
                )
                .context("failed recording prepared receipt anchor")?;
            Ok(())
        })();
        if let Err(error) = prepare_result {
            let _ = fs::remove_file(&prepared_path);
            let _ = sync_directory(&self.anchor_dir);
            return Err(error);
        }
        Ok(PreparedAnchor {
            transaction_id: input.transaction_id.clone(),
            anchor_generation: input.anchor_generation,
            prepared_document_digest: prepared_digest,
        })
    }

    /// Marks a prepared anchor as backed by a committed receipt write.
    ///
    /// EA-111's receipt journal calls this using the *same* SQLite transaction
    /// that appends the receipt.  Keeping the transaction supplied by the
    /// caller prevents an external anchor from being finalized for receipt
    /// state that never committed.
    pub(crate) fn confirm_prepared_anchor_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        transaction_id: &str,
        receipt_count: i64,
        receipt_head_digest: Option<&str>,
        committed_at: i64,
    ) -> Result<()> {
        validate_identifier("transaction_id", transaction_id)?;
        if receipt_count < 0 || committed_at <= 0 {
            bail!("receipt commit confirmation values are invalid");
        }
        if (receipt_count == 0) != receipt_head_digest.is_none() {
            bail!("receipt commit confirmation count/head pair is inconsistent");
        }
        if let Some(digest) = receipt_head_digest {
            validate_digest("receipt_head_digest", digest)?;
        }
        let row = anchor_row_by_transaction_with_conn(transaction, transaction_id)?
            .context("prepared receipt anchor does not exist")?;
        if row.status != "prepared"
            || row.receipt_commit_confirmed
            || row.receipt_count != receipt_count
            || row.receipt_head_digest.as_deref() != receipt_head_digest
        {
            bail!("prepared receipt anchor cannot be confirmed for these committed receipts");
        }
        let key_material = self
            .load_key(&row.key)
            .context("receipt key is unavailable for commit confirmation")?;
        let confirmation_tag = receipt_commit_confirmation_tag(&row, committed_at, &key_material)?;
        let changed = transaction.execute(
            r#"
            UPDATE execass_receipt_anchor_state
            SET receipt_commit_confirmed=1, receipt_committed_at=?1,
                receipt_commit_confirmation_tag=?2
            WHERE transaction_id=?3 AND status='prepared'
              AND receipt_commit_confirmed=0 AND receipt_count=?4
              AND receipt_head_digest IS ?5
            "#,
            params![
                committed_at,
                confirmation_tag,
                transaction_id,
                receipt_count,
                receipt_head_digest
            ],
        )?;
        if changed != 1 {
            bail!("prepared receipt anchor cannot be confirmed for these committed receipts");
        }
        Ok(())
    }

    pub(crate) fn finalize_anchor(&self, transaction_id: &str) -> Result<()> {
        validate_identifier("transaction_id", transaction_id)?;
        let mut connection = open_sqlite_connection(&self.db_path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("failed fencing receipt-anchor finalization")?;
        let row = anchor_row_by_transaction_with_conn(&transaction, transaction_id)?
            .context("prepared receipt anchor does not exist")?;
        if row.status == "finalized" {
            drop(transaction);
            return self.verify_finalized_row(&row);
        }
        if row.status != "prepared" {
            bail!("receipt anchor is not eligible for finalization");
        }
        if !row.receipt_commit_confirmed || row.receipt_committed_at.is_none() {
            bail!("receipt anchor cannot finalize before its receipt commit is confirmed");
        }

        let prepared_path = self.prepared_path(row.anchor_generation, transaction_id);
        let current_path = self.current_path();
        let prior_current_bytes = current_path
            .is_file()
            .then(|| fs::read(&current_path))
            .transpose()?;
        let finalize_result = (|| -> Result<()> {
            let prepared = read_exact_document(&prepared_path)?;
            verify_prepared_document(&row, &prepared)?;
            let current_key_material = self
                .load_key(&row.key)
                .context("receipt anchor key is unavailable during finalization")?;
            verify_receipt_commit_confirmation(&row, &current_key_material)?;
            let prior = prior_finalized_anchor_row_with_conn(&transaction, row.anchor_generation)?;
            let previous_key_material = prior
                .as_ref()
                .filter(|prior| prior.key != row.key)
                .map(|prior| self.load_key(&prior.key))
                .transpose()
                .context("previous receipt key is unavailable during rotation finalization")?;
            verify_anchor_document_integrity(
                &prepared,
                &current_key_material,
                previous_key_material.as_ref().map(|key| key.as_slice()),
            )?;
            let prepared_bytes = serialize_document(&prepared)?;
            if sha256_hex(&prepared_bytes) != row.prepared_document_digest {
                bail!("prepared receipt-anchor document digest mismatch");
            }

            let mut finalized = AnchorDocument {
                phase: AnchorPhase::Finalized,
                prepared_document_digest: Some(row.prepared_document_digest.clone()),
                integrity_tag: None,
                previous_key_integrity_tag: None,
                ..prepared
            };
            sign_anchor_document(
                &mut finalized,
                &current_key_material,
                previous_key_material.as_ref().map(|key| key.as_slice()),
            )?;
            let finalized_bytes = serialize_document(&finalized)?;
            if let Some(current_bytes) = prior_current_bytes.as_deref() {
                let current = parse_exact_document(current_bytes)?;
                if current != finalized {
                    let prior =
                        prior_finalized_anchor_row_with_conn(&transaction, row.anchor_generation)?
                            .context(
                                "external receipt anchor has no prior finalized database row",
                            )?;
                    let prior_key_material = self
                        .load_key(&prior.key)
                        .context("prior receipt anchor key is unavailable")?;
                    let before_prior = prior_finalized_anchor_row_with_conn(
                        &transaction,
                        prior.anchor_generation,
                    )?;
                    let before_prior_key_material = before_prior
                        .as_ref()
                        .filter(|before| before.key != prior.key)
                        .map(|before| self.load_key(&before.key))
                        .transpose()
                        .context("prior rotation cross-signing key is unavailable")?;
                    verify_anchor_document_integrity(
                        &current,
                        &prior_key_material,
                        before_prior_key_material.as_ref().map(|key| key.as_slice()),
                    )?;
                }
            }
            validate_current_before_finalize(
                &transaction,
                &self.anchor_dir,
                &row,
                &finalized,
                prior_current_bytes.as_deref(),
            )?;
            let finalized_digest = sha256_hex(&finalized_bytes);
            let finalized_path = self.finalized_path(row.anchor_generation, transaction_id);
            durable_write_atomic(
                &finalized_path,
                &finalized_bytes,
                "anchor.finalized_version",
                self.failpoints.as_ref(),
            )?;
            durable_write_atomic(
                &current_path,
                &finalized_bytes,
                "anchor.current",
                self.failpoints.as_ref(),
            )?;
            self.failpoints.hit("db.finalize.before")?;
            activate_key_for_anchor_with_conn(&transaction, &row.key, row.anchor_generation)?;
            let changed = transaction.execute(
                r#"
                UPDATE execass_receipt_anchor_state
                SET status='finalized', finalized_document_digest=?1, finalized_at=?2
                WHERE transaction_id=?3 AND status='prepared' AND receipt_commit_confirmed=1
                "#,
                params![finalized_digest, row.receipt_committed_at, transaction_id],
            )?;
            if changed != 1 {
                bail!("prepared receipt anchor changed before finalization");
            }
            transaction
                .commit()
                .context("failed committing finalized receipt anchor")?;
            Ok(())
        })();
        if let Err(error) = finalize_result {
            restore_current_after_failed_finalize(
                &current_path,
                prior_current_bytes.as_deref(),
                &self.anchor_dir,
            )?;
            return Err(error);
        }
        self.failpoints.hit("db.finalize.after")?;
        let finalized_row = anchor_row_by_transaction(&self.db_path, transaction_id)?
            .context("finalized receipt anchor disappeared")?;
        self.verify_finalized_row(&finalized_row)?;
        if prepared_path.exists() {
            fs::remove_file(prepared_path)?;
            sync_directory(&self.anchor_dir)?;
        }
        Ok(())
    }

    pub fn status(&self) -> Result<IntegrityStatus> {
        if self.quarantine_path().is_file() {
            return Ok(IntegrityStatus::Quarantined {
                reason: read_quarantine_reason(&self.quarantine_path())?,
            });
        }
        if !self.db_path.is_file() {
            return if self.external_anchor_history_exists()? {
                Ok(IntegrityStatus::Mismatch {
                    reason: "external_anchor_without_database_state".to_owned(),
                })
            } else {
                bail!("configured receipt database is missing")
            };
        }
        let row = latest_anchor_row(&self.db_path)?;
        let Some(row) = row else {
            return if receipt_row_count(&self.db_path)? != 0 {
                Ok(IntegrityStatus::Mismatch {
                    reason: "receipt_rows_without_database_anchor".to_owned(),
                })
            } else if self.current_path().exists() {
                Ok(IntegrityStatus::Mismatch {
                    reason: "external_anchor_without_database_state".to_owned(),
                })
            } else {
                Ok(IntegrityStatus::Uninitialized)
            };
        };
        if row.root_identity != self.root_identity {
            return Ok(IntegrityStatus::Mismatch {
                reason: "root_identity_mismatch".to_owned(),
            });
        }
        match row.status.as_str() {
            "quarantined" => Ok(IntegrityStatus::Quarantined {
                reason: "database_integrity_quarantine".to_owned(),
            }),
            "prepared" => match self.load_key(&row.key) {
                Ok(key_material)
                    if !row.receipt_commit_confirmed
                        || verify_receipt_commit_confirmation(&row, &key_material).is_ok() =>
                {
                    Ok(IntegrityStatus::Prepared {
                        transaction_id: row.transaction_id,
                        anchor_generation: row.anchor_generation,
                    })
                }
                Ok(_) => Ok(IntegrityStatus::Mismatch {
                    reason: "receipt_commit_confirmation_mismatch".to_owned(),
                }),
                Err(_) => Ok(IntegrityStatus::KeyLost { key: row.key }),
            },
            "finalized" => match self.load_key(&row.key) {
                Err(_) => Ok(IntegrityStatus::KeyLost { key: row.key }),
                Ok(_) => match self.verify_anchor_history(&row) {
                    Ok(()) => match self.receipt_history_failure_for_row(&row)? {
                        None => Ok(IntegrityStatus::Trusted {
                            anchor_generation: row.anchor_generation,
                            receipt_count: row.receipt_count,
                            receipt_head_digest: row.receipt_head_digest,
                            key: row.key,
                        }),
                        Some(failure) => Ok(IntegrityStatus::Mismatch {
                            reason: failure.reason_code(),
                        }),
                    },
                    Err(error) => Ok(IntegrityStatus::Mismatch {
                        reason: classify_integrity_error(&error),
                    }),
                },
            },
            _ => Ok(IntegrityStatus::Mismatch {
                reason: "invalid_database_anchor_status".to_owned(),
            }),
        }
    }

    #[cfg(test)]
    pub(super) fn receipt_history_failure(&self) -> Result<Option<ReceiptHistoryFailure>> {
        let Some(row) = latest_anchor_row(&self.db_path)? else {
            return Ok(None);
        };
        self.receipt_history_failure_for_row(&row)
    }

    fn receipt_history_failure_for_row(
        &self,
        row: &AnchorRow,
    ) -> Result<Option<ReceiptHistoryFailure>> {
        let conn = open_sqlite_connection(&self.db_path)?;
        Ok(verify_receipt_history(
            &conn,
            self,
            row.state_root_generation,
            row.receipt_count,
            row.receipt_head_digest.as_deref(),
        )
        .err())
    }

    pub fn recover_integrity(&self) -> Result<IntegrityRecovery> {
        match self.status()? {
            IntegrityStatus::Uninitialized => {
                if self.remove_orphan_prepared_files()? {
                    Ok(IntegrityRecovery::RestoredLastProvenPair {
                        anchor_generation: None,
                    })
                } else {
                    Ok(IntegrityRecovery::Healthy)
                }
            }
            IntegrityStatus::Trusted { .. } => {
                let generation = latest_anchor_row(&self.db_path)?.map(|row| row.anchor_generation);
                if self.remove_orphan_prepared_files()? {
                    Ok(IntegrityRecovery::RestoredLastProvenPair {
                        anchor_generation: generation,
                    })
                } else {
                    Ok(IntegrityRecovery::Healthy)
                }
            }
            IntegrityStatus::Prepared { transaction_id, .. } => {
                let row = anchor_row_by_transaction(&self.db_path, &transaction_id)?
                    .context("prepared receipt anchor disappeared during recovery")?;
                if !row.receipt_commit_confirmed {
                    self.discard_unconfirmed_prepared(&row)
                } else {
                    match self.finalize_anchor(&transaction_id) {
                        Ok(()) => {
                            Ok(IntegrityRecovery::FinalizedInterruptedCommit { transaction_id })
                        }
                        Err(error) => self.quarantine(&classify_integrity_error(&error)),
                    }
                }
            }
            IntegrityStatus::KeyLost { .. } => self.quarantine("receipt_key_unavailable"),
            IntegrityStatus::Mismatch { reason } => self.quarantine(&reason),
            IntegrityStatus::Quarantined { reason } => {
                Ok(IntegrityRecovery::Quarantined { reason })
            }
        }
    }

    fn discard_unconfirmed_prepared(&self, row: &AnchorRow) -> Result<IntegrityRecovery> {
        let mut connection = open_sqlite_connection(&self.db_path)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("failed fencing unconfirmed receipt-anchor recovery")?;
        let changed = transaction.execute(
            "DELETE FROM execass_receipt_anchor_state WHERE transaction_id=?1 AND status='prepared' AND receipt_commit_confirmed=0",
            params![row.transaction_id],
        )?;
        if changed != 1 {
            bail!("unconfirmed prepared receipt anchor changed during recovery");
        }
        transaction.commit()?;

        let prepared_path = self.prepared_path(row.anchor_generation, &row.transaction_id);
        if prepared_path.exists() {
            fs::remove_file(prepared_path)?;
            sync_directory(&self.anchor_dir)?;
        }
        let prior = latest_anchor_row(&self.db_path)?;
        match prior.as_ref() {
            Some(prior) if prior.status == "finalized" => {
                let bytes =
                    fs::read(self.finalized_path(prior.anchor_generation, &prior.transaction_id))?;
                durable_write_atomic(
                    &self.current_path(),
                    &bytes,
                    "anchor.current",
                    &NoFailpoints,
                )?;
                self.verify_finalized_row(prior)?;
            }
            None => {
                if self.current_path().exists() {
                    fs::remove_file(self.current_path())?;
                    sync_directory(&self.anchor_dir)?;
                }
            }
            Some(_) => bail!("unconfirmed anchor recovery found no prior finalized state"),
        }
        Ok(IntegrityRecovery::RestoredLastProvenPair {
            anchor_generation: prior.map(|prior| prior.anchor_generation),
        })
    }

    fn verify_finalized_row(&self, row: &AnchorRow) -> Result<()> {
        if row.root_identity != self.root_identity {
            bail!("receipt anchor root identity mismatch");
        }
        let current = read_exact_document(&self.current_path())?;
        verify_finalized_document(row, &current)?;
        let conn = open_sqlite_connection(&self.db_path)?;
        let current_key_material = self
            .load_key(&row.key)
            .context("receipt anchor key is unavailable")?;
        verify_receipt_commit_confirmation(row, &current_key_material)?;
        let prior = prior_finalized_anchor_row_with_conn(&conn, row.anchor_generation)?;
        let previous_key_material = prior
            .as_ref()
            .filter(|prior| prior.key != row.key)
            .map(|prior| self.load_key(&prior.key))
            .transpose()
            .context("previous receipt key is unavailable for rotation verification")?;
        verify_anchor_document_integrity(
            &current,
            &current_key_material,
            previous_key_material.as_ref().map(|key| key.as_slice()),
        )?;
        let bytes = serialize_document(&current)?;
        if row.finalized_document_digest.as_deref() != Some(sha256_hex(&bytes).as_str()) {
            bail!("finalized receipt-anchor document digest mismatch");
        }
        let versioned =
            read_exact_document(&self.finalized_path(row.anchor_generation, &row.transaction_id))?;
        if versioned != current {
            bail!("current and versioned receipt anchors differ");
        }
        Ok(())
    }

    fn verify_anchor_history(&self, latest: &AnchorRow) -> Result<()> {
        let conn = open_sqlite_connection(&self.db_path)?;
        let rows = all_anchor_rows_with_conn(&conn)?;
        if rows.is_empty() {
            bail!("receipt anchor history is empty");
        }
        let mut previous: Option<(&AnchorRow, String)> = None;
        for (offset, row) in rows.iter().enumerate() {
            let expected_generation = i64::try_from(offset + 1)
                .context("receipt anchor generation exceeds SQLite integer range")?;
            if row.anchor_generation != expected_generation {
                bail!("receipt anchor generation is not contiguous");
            }
            if row.status != "finalized" {
                bail!("receipt anchor history contains a non-finalized predecessor");
            }
            if row.root_identity != self.root_identity {
                bail!("receipt anchor root identity mismatch");
            }
            if let Some((prior, _)) = previous.as_ref() {
                if row.state_root_generation < prior.state_root_generation {
                    bail!("receipt anchor state-root generation moved backwards");
                }
                if row.receipt_count < prior.receipt_count
                    || (row.receipt_count == prior.receipt_count
                        && row.receipt_head_digest != prior.receipt_head_digest)
                {
                    bail!("receipt anchor count/head history mismatch");
                }
                if row.key != prior.key && row.key.key_generation != prior.key.key_generation + 1 {
                    bail!("receipt anchor key generation history mismatch");
                }
            } else if row.key.key_generation != 1 {
                bail!("first receipt anchor key generation is not one");
            }

            let document = read_exact_document(
                &self.finalized_path(row.anchor_generation, &row.transaction_id),
            )?;
            verify_finalized_document(row, &document)?;
            let current_key = self
                .load_key(&row.key)
                .context("receipt anchor key is unavailable")?;
            let previous_key = previous
                .as_ref()
                .filter(|(prior, _)| prior.key != row.key)
                .map(|(prior, _)| self.load_key(&prior.key))
                .transpose()
                .context("previous receipt key is unavailable for rotation verification")?;
            verify_receipt_commit_confirmation(row, &current_key)?;
            verify_anchor_document_integrity(
                &document,
                &current_key,
                previous_key.as_ref().map(|key| key.as_slice()),
            )?;
            let document_bytes = serialize_document(&document)?;
            let document_digest = sha256_hex(&document_bytes);
            if row.finalized_document_digest.as_deref() != Some(document_digest.as_str()) {
                bail!("finalized receipt-anchor document digest mismatch");
            }
            if document.previous_finalized_document_digest
                != previous.as_ref().map(|(_, digest)| digest.clone())
            {
                bail!("receipt anchor finalized-document chain mismatch");
            }
            previous = Some((row, document_digest));
        }
        let (last, _) = previous.context("receipt anchor history is empty")?;
        if last.transaction_id != latest.transaction_id
            || last.anchor_generation != latest.anchor_generation
        {
            bail!("receipt anchor latest-row identity mismatch");
        }
        let current = read_exact_document(&self.current_path())?;
        let latest_versioned = read_exact_document(
            &self.finalized_path(latest.anchor_generation, &latest.transaction_id),
        )?;
        if current != latest_versioned {
            bail!("current and versioned receipt anchors differ");
        }
        Ok(())
    }

    fn quarantine(&self, reason: &str) -> Result<IntegrityRecovery> {
        let safe_reason = sanitize_reason(reason);
        let marker = serde_json::to_vec(&serde_json::json!({
            "document_version": "carsinos.execass.quarantine.v1",
            "root_identity": self.root_identity,
            "reason": safe_reason,
        }))?;
        durable_write_atomic(
            &self.quarantine_path(),
            &marker,
            "anchor.quarantine",
            self.failpoints.as_ref(),
        )?;
        if !self.db_path.is_file() {
            return Ok(IntegrityRecovery::Quarantined {
                reason: safe_reason,
            });
        }
        let mut conn =
            match Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_WRITE) {
                Ok(conn) => conn,
                Err(_) if !self.db_path.is_file() => {
                    return Ok(IntegrityRecovery::Quarantined {
                        reason: safe_reason,
                    });
                }
                Err(error) => {
                    return Err(error).context("failed opening existing receipt database")
                }
            };
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if safe_reason == "receipt_key_unavailable" {
            transaction.execute(
                r#"
                UPDATE execass_receipt_keys SET status='lost'
                WHERE (key_id,key_generation)=(
                  SELECT key_id,key_generation FROM execass_receipt_anchor_state
                  ORDER BY anchor_generation DESC LIMIT 1
                ) AND status IN ('provisioned','active')
                "#,
                [],
            )?;
        }
        transaction.execute(
            r#"
            UPDATE execass_receipt_anchor_state
            SET status='quarantined', quarantined_at=MAX(prepared_at, 1), quarantine_reason=?1
            WHERE anchor_id=(
              SELECT anchor_id FROM execass_receipt_anchor_state
              ORDER BY anchor_generation DESC LIMIT 1
            ) AND status!='quarantined'
            "#,
            params![safe_reason],
        )?;
        transaction.commit()?;
        Ok(IntegrityRecovery::Quarantined {
            reason: safe_reason,
        })
    }

    fn remove_orphan_prepared_files(&self) -> Result<bool> {
        let represented = latest_anchor_row(&self.db_path)?
            .filter(|row| row.status == "prepared")
            .map(|row| self.prepared_path(row.anchor_generation, &row.transaction_id));
        let known = all_anchor_prepare_paths(&self.db_path, &self.anchor_dir)?;
        let mut restored_pair = false;
        for entry in fs::read_dir(&self.anchor_dir)? {
            let path = entry?.path();
            let is_prepared = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".prepared.json"));
            if is_prepared && represented.as_ref() != Some(&path) {
                if !known.contains(&path) {
                    restored_pair = true;
                }
                fs::remove_file(path)?;
            }
        }
        sync_directory(&self.anchor_dir)?;
        Ok(restored_pair)
    }

    fn prepared_path(&self, generation: i64, transaction_id: &str) -> PathBuf {
        self.anchor_dir.join(format!(
            "anchor-{generation:020}-{transaction_id}.prepared.json"
        ))
    }

    fn finalized_path(&self, generation: i64, transaction_id: &str) -> PathBuf {
        self.anchor_dir.join(format!(
            "anchor-{generation:020}-{transaction_id}.finalized.json"
        ))
    }

    fn current_path(&self) -> PathBuf {
        self.anchor_dir.join("current.json")
    }

    fn quarantine_path(&self) -> PathBuf {
        self.anchor_dir.join("quarantine.json")
    }

    fn external_anchor_history_exists(&self) -> Result<bool> {
        if self.current_path().is_file() {
            return Ok(true);
        }
        for entry in fs::read_dir(&self.anchor_dir)
            .context("failed reading external receipt-anchor history")?
        {
            let path = entry
                .context("failed reading external receipt-anchor history entry")?
                .path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".finalized.json"))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// File-backed only so dependent-crate restart tests reopen the same key.
/// Outside this crate's unit-test build, this is unavailable unless the
/// explicit test-support feature is enabled. Production `open` never selects
/// it on any platform.
#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
struct FeatureTestReceiptKeyProtector {
    key_dir: PathBuf,
}

#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
impl FeatureTestReceiptKeyProtector {
    fn new(key_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&key_dir)?;
        sync_directory(
            key_dir
                .parent()
                .context("test receipt key directory has no parent")?,
        )?;
        Ok(Self { key_dir })
    }

    fn path(&self, key: &ReceiptKeyRef) -> PathBuf {
        self.key_dir.join(format!(
            "{}-{:020}.test-key",
            key.key_id, key.key_generation
        ))
    }
}

#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
impl ReceiptKeyProtector for FeatureTestReceiptKeyProtector {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let path = self.path(key);
        if path.exists() {
            bail!("test receipt key already exists");
        }
        let mut material = Zeroizing::new(vec![0_u8; KEY_BYTES]);
        getrandom::fill(&mut material).context("generating test receipt key")?;
        durable_write_atomic(&path, &material, "test-key", &NoFailpoints)?;
        Ok(material)
    }

    fn load(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let material =
            Zeroizing::new(fs::read(self.path(key)).context("test receipt key is unavailable")?);
        if material.len() != KEY_BYTES {
            bail!("test receipt key has invalid length");
        }
        Ok(material)
    }

    fn delete(&self, key: &ReceiptKeyRef) -> Result<()> {
        let path = self.path(key);
        if path.exists() {
            fs::remove_file(path)?;
            sync_directory(&self.key_dir)?;
        }
        Ok(())
    }
}

fn latest_anchor_row(db_path: &Path) -> Result<Option<AnchorRow>> {
    let conn = open_sqlite_connection(db_path)?;
    latest_anchor_row_with_conn(&conn)
}

fn latest_anchor_row_with_conn(conn: &Connection) -> Result<Option<AnchorRow>> {
    conn.query_row(
        r#"
        SELECT root_identity,state_root_generation,anchor_generation,status,
               receipt_count,receipt_head_digest,key_id,key_generation,transaction_id,
               external_receipt_digest,prepared_document_digest,
               receipt_commit_confirmed,receipt_committed_at,receipt_commit_confirmation_tag,
               finalized_document_digest,prepared_at
        FROM execass_receipt_anchor_state
        ORDER BY anchor_generation DESC
        LIMIT 1
        "#,
        [],
        map_anchor_row,
    )
    .optional()
    .context("failed reading current receipt anchor")
}

fn all_anchor_rows_with_conn(conn: &Connection) -> Result<Vec<AnchorRow>> {
    let mut statement = conn.prepare(
        r#"
        SELECT root_identity,state_root_generation,anchor_generation,status,
               receipt_count,receipt_head_digest,key_id,key_generation,transaction_id,
               external_receipt_digest,prepared_document_digest,
               receipt_commit_confirmed,receipt_committed_at,receipt_commit_confirmation_tag,
               finalized_document_digest,prepared_at
        FROM execass_receipt_anchor_state
        ORDER BY anchor_generation
        "#,
    )?;
    let rows = statement
        .query_map([], map_anchor_row)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed reading receipt anchor history")?;
    Ok(rows)
}

fn anchor_row_by_transaction(db_path: &Path, transaction_id: &str) -> Result<Option<AnchorRow>> {
    let conn = open_sqlite_connection(db_path)?;
    anchor_row_by_transaction_with_conn(&conn, transaction_id)
}

fn anchor_row_by_transaction_with_conn(
    conn: &Connection,
    transaction_id: &str,
) -> Result<Option<AnchorRow>> {
    conn.query_row(
        r#"
        SELECT root_identity,state_root_generation,anchor_generation,status,
               receipt_count,receipt_head_digest,key_id,key_generation,transaction_id,
               external_receipt_digest,prepared_document_digest,
               receipt_commit_confirmed,receipt_committed_at,receipt_commit_confirmation_tag,
               finalized_document_digest,prepared_at
        FROM execass_receipt_anchor_state WHERE transaction_id=?1
        "#,
        params![transaction_id],
        map_anchor_row,
    )
    .optional()
    .context("failed reading receipt anchor transaction")
}

fn prior_finalized_anchor_row_with_conn(
    conn: &Connection,
    anchor_generation: i64,
) -> Result<Option<AnchorRow>> {
    conn.query_row(
        r#"
        SELECT root_identity,state_root_generation,anchor_generation,status,
               receipt_count,receipt_head_digest,key_id,key_generation,transaction_id,
               external_receipt_digest,prepared_document_digest,
               receipt_commit_confirmed,receipt_committed_at,receipt_commit_confirmation_tag,
               finalized_document_digest,prepared_at
        FROM execass_receipt_anchor_state
        WHERE anchor_generation < ?1 AND status='finalized'
        ORDER BY anchor_generation DESC LIMIT 1
        "#,
        params![anchor_generation],
        map_anchor_row,
    )
    .optional()
    .context("failed reading prior finalized receipt anchor")
}

fn validate_current_before_finalize(
    conn: &Connection,
    anchor_dir: &Path,
    pending: &AnchorRow,
    proposed: &AnchorDocument,
    current_bytes: Option<&[u8]>,
) -> Result<()> {
    let Some(current_bytes) = current_bytes else {
        if pending.anchor_generation != 1 {
            bail!("prior external receipt anchor is missing");
        }
        return Ok(());
    };
    let current = parse_exact_document(current_bytes)?;
    if current == *proposed {
        return Ok(());
    }
    let prior = prior_finalized_anchor_row_with_conn(conn, pending.anchor_generation)?
        .context("external receipt anchor does not match a prior finalized database row")?;
    verify_finalized_document(&prior, &current)?;
    if prior.finalized_document_digest.as_deref() != Some(sha256_hex(current_bytes).as_str()) {
        bail!("prior external receipt-anchor digest mismatch");
    }
    let versioned_path = anchor_dir.join(format!(
        "anchor-{:020}-{}.finalized.json",
        prior.anchor_generation, prior.transaction_id
    ));
    if fs::read(versioned_path)? != current_bytes {
        bail!("prior current and versioned receipt anchors differ");
    }
    Ok(())
}

fn restore_current_after_failed_finalize(
    current_path: &Path,
    prior_bytes: Option<&[u8]>,
    anchor_dir: &Path,
) -> Result<()> {
    if let Some(prior_bytes) = prior_bytes {
        durable_write_atomic(current_path, prior_bytes, "anchor.current", &NoFailpoints)?;
    } else if current_path.exists() {
        fs::remove_file(current_path)?;
        sync_directory(anchor_dir)?;
    }
    Ok(())
}

fn all_anchor_prepare_paths(db_path: &Path, anchor_dir: &Path) -> Result<BTreeSet<PathBuf>> {
    let conn = open_sqlite_connection(db_path)?;
    let mut statement =
        conn.prepare("SELECT anchor_generation, transaction_id FROM execass_receipt_anchor_state")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut paths = BTreeSet::new();
    for row in rows {
        let (generation, transaction_id) = row?;
        paths.insert(anchor_dir.join(format!(
            "anchor-{generation:020}-{transaction_id}.prepared.json"
        )));
    }
    Ok(paths)
}

fn has_external_prepared_document(anchor_dir: &Path) -> Result<bool> {
    for entry in fs::read_dir(anchor_dir)? {
        if entry?
            .file_name()
            .to_str()
            .is_some_and(|name| name.ends_with(".prepared.json"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn map_anchor_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnchorRow> {
    Ok(AnchorRow {
        root_identity: row.get(0)?,
        state_root_generation: row.get(1)?,
        anchor_generation: row.get(2)?,
        status: row.get(3)?,
        receipt_count: row.get(4)?,
        receipt_head_digest: row.get(5)?,
        key: ReceiptKeyRef {
            key_id: row.get(6)?,
            key_generation: row.get(7)?,
        },
        transaction_id: row.get(8)?,
        external_receipt_digest: row.get(9)?,
        prepared_document_digest: row.get(10)?,
        receipt_commit_confirmed: row.get::<_, i64>(11)? != 0,
        receipt_committed_at: row.get(12)?,
        receipt_commit_confirmation_tag: row.get(13)?,
        finalized_document_digest: row.get(14)?,
        prepared_at: row.get(15)?,
    })
}

pub(super) fn receipt_key_count_with_conn(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM execass_receipt_keys", [], |row| {
        row.get(0)
    })
    .context("failed counting receipt-key registry")
}

fn provisioned_key_with_conn(conn: &Connection) -> Result<Option<ReceiptKeyRef>> {
    conn.query_row(
        "SELECT key_id,key_generation FROM execass_receipt_keys WHERE status='provisioned'",
        [],
        |row| {
            Ok(ReceiptKeyRef {
                key_id: row.get(0)?,
                key_generation: row.get(1)?,
            })
        },
    )
    .optional()
    .context("failed reading provisioned receipt key")
}

fn receipt_key_record_with_conn(
    conn: &Connection,
    key: &ReceiptKeyRef,
) -> Result<Option<ReceiptKeyRow>> {
    conn.query_row(
        r#"
        SELECT status,rotated_from_key_id,rotated_from_key_generation,
               activated_anchor_generation
        FROM execass_receipt_keys WHERE key_id=?1 AND key_generation=?2
        "#,
        params![key.key_id, key.key_generation],
        |row| {
            Ok(ReceiptKeyRow {
                status: row.get(0)?,
                rotated_from_key_id: row.get(1)?,
                rotated_from_key_generation: row.get(2)?,
                activated_anchor_generation: row.get(3)?,
            })
        },
    )
    .optional()
    .context("failed reading receipt-key registry record")
}

fn require_registered_key_status(
    conn: &Connection,
    key: &ReceiptKeyRef,
    expected: &str,
) -> Result<()> {
    let record = receipt_key_record_with_conn(conn, key)?
        .context("receipt key is absent from the immutable registry")?;
    if record.status != expected {
        bail!("receipt key registry status is not {expected}");
    }
    Ok(())
}

fn validate_key_for_anchor_with_conn(
    conn: &Connection,
    previous: Option<&AnchorRow>,
    key: &ReceiptKeyRef,
) -> Result<()> {
    let record = receipt_key_record_with_conn(conn, key)?
        .context("receipt anchor key is absent from the immutable registry")?;
    match previous {
        None if record.status == "provisioned" && key.key_generation == 1 => Ok(()),
        Some(previous) if previous.key == *key && record.status == "active" => Ok(()),
        Some(previous)
            if record.status == "provisioned"
                && record.rotated_from_key_id.as_deref() == Some(previous.key.key_id.as_str())
                && record.rotated_from_key_generation == Some(previous.key.key_generation) =>
        {
            Ok(())
        }
        _ => bail!("receipt anchor key is not valid for this anchor transition"),
    }
}

fn activate_key_for_anchor_with_conn(
    conn: &Connection,
    key: &ReceiptKeyRef,
    anchor_generation: i64,
) -> Result<()> {
    let record = receipt_key_record_with_conn(conn, key)?
        .context("receipt anchor key is absent from the immutable registry")?;
    match record.status.as_str() {
        "active" if record.activated_anchor_generation.is_some() => Ok(()),
        "provisioned" => {
            if let (Some(parent_id), Some(parent_generation)) = (
                record.rotated_from_key_id,
                record.rotated_from_key_generation,
            ) {
                let retired = conn.execute(
                    "UPDATE execass_receipt_keys SET status='retired' WHERE key_id=?1 AND key_generation=?2 AND status='active'",
                    params![parent_id, parent_generation],
                )?;
                if retired != 1 {
                    bail!("receipt key rotation parent is not the active key");
                }
            }
            let activated = conn.execute(
                "UPDATE execass_receipt_keys SET status='active', activated_anchor_generation=?1 WHERE key_id=?2 AND key_generation=?3 AND status='provisioned'",
                params![anchor_generation, key.key_id, key.key_generation],
            )?;
            if activated != 1 {
                bail!("provisioned receipt key changed before activation");
            }
            Ok(())
        }
        _ => bail!(
            "receipt anchor key cannot be activated from registry status {}",
            record.status
        ),
    }
}

fn receipt_row_count(db_path: &Path) -> Result<i64> {
    let conn = open_sqlite_connection(db_path)?;
    conn.query_row("SELECT COUNT(*) FROM execass_receipts", [], |row| {
        row.get(0)
    })
    .context("failed counting ExecAss receipts")
}

fn validate_anchor_input(input: &AnchorCommitInput) -> Result<()> {
    if input.state_root_generation <= 0 || input.anchor_generation <= 0 {
        bail!("receipt anchor generations must be positive integers");
    }
    if input.receipt_count < 0 || input.occurred_at < 0 {
        bail!("receipt anchor count and timestamp cannot be negative");
    }
    if (input.receipt_count == 0) != input.receipt_head_digest.is_none() {
        bail!("receipt anchor count/head pair is inconsistent");
    }
    if let Some(head) = &input.receipt_head_digest {
        validate_digest("receipt_head_digest", head)?;
    }
    validate_identifier("key_id", &input.key.key_id)?;
    validate_identifier("transaction_id", &input.transaction_id)?;
    if input.key.key_generation <= 0 {
        bail!("receipt key generation must be positive");
    }
    validate_digest("external_receipt_digest", &input.external_receipt_digest)
}

fn validate_anchor_progression(
    previous: Option<&AnchorRow>,
    input: &AnchorCommitInput,
    root_identity: &str,
) -> Result<()> {
    match previous {
        None => {
            if input.anchor_generation != 1 {
                bail!("first receipt anchor generation must be one");
            }
            if input.key.key_generation != 1 {
                bail!("first receipt key generation must be one");
            }
        }
        Some(previous) => {
            if previous.status != "finalized" {
                bail!("a receipt anchor transition is already in progress or quarantined");
            }
            if previous.root_identity != root_identity {
                bail!("receipt anchor root identity changed");
            }
            if input.anchor_generation != previous.anchor_generation + 1 {
                bail!("receipt anchor generation is not contiguous");
            }
            if input.state_root_generation < previous.state_root_generation {
                bail!("state-root generation cannot move backwards");
            }
            if input.receipt_count < previous.receipt_count {
                bail!("receipt high-water count cannot move backwards");
            }
            if input.receipt_count == previous.receipt_count
                && input.receipt_head_digest != previous.receipt_head_digest
            {
                bail!("receipt head changed without a count advance");
            }
            let valid_key = input.key == previous.key
                || input.key.key_generation == previous.key.key_generation + 1;
            if !valid_key {
                bail!("receipt key generation is neither current nor one-step rotation");
            }
        }
    }
    Ok(())
}

fn verify_prepared_document(row: &AnchorRow, document: &AnchorDocument) -> Result<()> {
    if document.document_version != ANCHOR_DOCUMENT_VERSION
        || document.phase != AnchorPhase::Prepared
    {
        bail!("prepared receipt-anchor document version or phase mismatch");
    }
    verify_anchor_document_binding(row, document)?;
    if document.prepared_document_digest.is_some() {
        bail!("prepared receipt-anchor document unexpectedly binds a prepared digest");
    }
    Ok(())
}

fn verify_finalized_document(row: &AnchorRow, document: &AnchorDocument) -> Result<()> {
    if document.document_version != ANCHOR_DOCUMENT_VERSION
        || document.phase != AnchorPhase::Finalized
    {
        bail!("finalized receipt-anchor document version or phase mismatch");
    }
    verify_anchor_document_binding(row, document)?;
    if document.prepared_document_digest.as_deref() != Some(row.prepared_document_digest.as_str()) {
        bail!("finalized receipt-anchor prepared digest mismatch");
    }
    Ok(())
}

fn verify_anchor_document_binding(row: &AnchorRow, document: &AnchorDocument) -> Result<()> {
    if document.root_identity != row.root_identity {
        bail!("receipt anchor root identity mismatch");
    }
    if document.state_root_generation != row.state_root_generation {
        bail!("receipt anchor state-root generation mismatch");
    }
    if document.anchor_generation != row.anchor_generation {
        bail!("receipt anchor generation mismatch");
    }
    if document.receipt_count != row.receipt_count
        || document.receipt_head_digest != row.receipt_head_digest
    {
        bail!("receipt anchor count/head mismatch");
    }
    if document.key_id != row.key.key_id || document.key_generation != row.key.key_generation {
        bail!("receipt anchor key generation mismatch");
    }
    if document.transaction_id != row.transaction_id {
        bail!("receipt anchor transaction identity mismatch");
    }
    if document.external_receipt_digest != row.external_receipt_digest {
        bail!("receipt anchor external receipt digest mismatch");
    }
    if document.occurred_at != row.prepared_at {
        bail!("receipt anchor occurrence timestamp mismatch");
    }
    if document.integrity_algorithm != ANCHOR_INTEGRITY_ALGORITHM
        || document.integrity_tag.is_none()
    {
        bail!("receipt anchor integrity algorithm or tag mismatch");
    }
    Ok(())
}

fn sign_anchor_document(
    document: &mut AnchorDocument,
    current_key: &[u8],
    previous_key: Option<&[u8]>,
) -> Result<()> {
    document.integrity_algorithm = ANCHOR_INTEGRITY_ALGORITHM.to_owned();
    document.integrity_tag = None;
    document.previous_key_integrity_tag = None;
    let unsigned = unsigned_anchor_document_bytes(document)?;
    document.integrity_tag = Some(hmac_hex(current_key, ANCHOR_INTEGRITY_DOMAIN, &unsigned)?);
    document.previous_key_integrity_tag = previous_key
        .map(|key| hmac_hex(key, ANCHOR_ROTATION_DOMAIN, &unsigned))
        .transpose()?;
    Ok(())
}

pub(super) fn verify_anchor_document_integrity(
    document: &AnchorDocument,
    current_key: &[u8],
    previous_key: Option<&[u8]>,
) -> Result<()> {
    if document.integrity_algorithm != ANCHOR_INTEGRITY_ALGORITHM {
        bail!("receipt-anchor integrity algorithm is unsupported");
    }
    let current_tag = document
        .integrity_tag
        .as_deref()
        .context("receipt-anchor keyed integrity tag is missing")?;
    let unsigned = unsigned_anchor_document_bytes(document)?;
    verify_hmac_hex(current_key, ANCHOR_INTEGRITY_DOMAIN, &unsigned, current_tag)
        .context("receipt-anchor keyed integrity mismatch")?;
    match (previous_key, document.previous_key_integrity_tag.as_deref()) {
        (Some(key), Some(tag)) => {
            verify_hmac_hex(key, ANCHOR_ROTATION_DOMAIN, &unsigned, tag)
                .context("receipt-anchor rotation cross-signature mismatch")?;
        }
        (Some(_), None) => bail!("receipt-anchor rotation cross-signature is missing"),
        (None, Some(_)) => bail!("receipt-anchor has an unexpected previous-key signature"),
        (None, None) => {}
    }
    Ok(())
}

fn receipt_commit_confirmation_bytes(row: &AnchorRow, committed_at: i64) -> Result<Vec<u8>> {
    serde_json::to_vec(&ReceiptCommitConfirmation {
        document_version: "carsinos.execass.receipt-commit-confirmation.v1",
        root_identity: &row.root_identity,
        state_root_generation: row.state_root_generation,
        anchor_generation: row.anchor_generation,
        transaction_id: &row.transaction_id,
        receipt_count: row.receipt_count,
        receipt_head_digest: row.receipt_head_digest.as_deref(),
        key_id: &row.key.key_id,
        key_generation: row.key.key_generation,
        prepared_document_digest: &row.prepared_document_digest,
        committed_at,
    })
    .context("failed serializing receipt-commit confirmation")
}

fn receipt_commit_confirmation_tag(
    row: &AnchorRow,
    committed_at: i64,
    key: &[u8],
) -> Result<String> {
    hmac_hex(
        key,
        RECEIPT_COMMIT_CONFIRMATION_DOMAIN,
        &receipt_commit_confirmation_bytes(row, committed_at)?,
    )
}

fn verify_receipt_commit_confirmation(row: &AnchorRow, key: &[u8]) -> Result<()> {
    if !row.receipt_commit_confirmed {
        bail!("receipt commit has not been confirmed");
    }
    let committed_at = row
        .receipt_committed_at
        .context("receipt commit timestamp is missing")?;
    let tag = row
        .receipt_commit_confirmation_tag
        .as_deref()
        .context("receipt commit confirmation tag is missing")?;
    verify_hmac_hex(
        key,
        RECEIPT_COMMIT_CONFIRMATION_DOMAIN,
        &receipt_commit_confirmation_bytes(row, committed_at)?,
        tag,
    )
    .context("receipt commit confirmation HMAC mismatch")
}

fn unsigned_anchor_document_bytes(document: &AnchorDocument) -> Result<Vec<u8>> {
    let mut unsigned = document.clone();
    unsigned.integrity_tag = None;
    unsigned.previous_key_integrity_tag = None;
    serde_json::to_vec(&unsigned).context("failed serializing unsigned receipt-anchor document")
}

fn hmac_hex(key: &[u8], domain: &[u8], payload: &[u8]) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(key).context("invalid receipt HMAC key")?;
    mac.update(domain);
    mac.update(payload);
    Ok(bytes_to_hex(&mac.finalize().into_bytes()))
}

pub(crate) fn receipt_key_registry_integrity_tag(
    key_material: &[u8],
    root_identity: &str,
    key: &ReceiptKeyRef,
    rotated_from_key_id: Option<&str>,
    rotated_from_key_generation: Option<i64>,
    created_at: i64,
) -> Result<String> {
    let identity = ReceiptKeyRegistryIdentity {
        root_identity,
        key_id: &key.key_id,
        key_generation: key.key_generation,
        rotated_from_key_id,
        rotated_from_key_generation,
        created_at,
    };
    let bytes = serde_json::to_vec(&identity)
        .context("failed serializing receipt-key registry identity")?;
    hmac_hex(key_material, KEY_REGISTRY_INTEGRITY_DOMAIN, &bytes)
}

fn verify_hmac_hex(key: &[u8], domain: &[u8], payload: &[u8], tag: &str) -> Result<()> {
    let expected = decode_lower_hex_32(tag)?;
    let mut mac = HmacSha256::new_from_slice(key).context("invalid receipt HMAC key")?;
    mac.update(domain);
    mac.update(payload);
    mac.verify_slice(&expected)
        .map_err(|_| anyhow::anyhow!("HMAC verification failed"))
}

fn decode_lower_hex_32(value: &str) -> Result<[u8; 32]> {
    validate_digest("integrity_tag", value)?;
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("invalid receipt-anchor integrity tag")?;
    }
    Ok(output)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("write to String");
    }
    output
}

pub(super) fn serialize_document(document: &AnchorDocument) -> Result<Vec<u8>> {
    serde_json::to_vec(document).context("failed serializing receipt-anchor document")
}

pub(super) fn read_exact_document(path: &Path) -> Result<AnchorDocument> {
    let mut file = File::open(path).context("failed opening the configured receipt anchor")?;
    let metadata = file.metadata()?;
    if metadata.len() == 0 || metadata.len() > 64 * 1024 {
        bail!("receipt-anchor document has an invalid size");
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)?;
    parse_exact_document(&bytes)
}

fn parse_exact_document(bytes: &[u8]) -> Result<AnchorDocument> {
    let document: AnchorDocument =
        serde_json::from_slice(bytes).context("receipt-anchor document is not strict JSON")?;
    if serialize_document(&document)? != bytes {
        bail!("receipt-anchor document is not in canonical struct encoding");
    }
    Ok(document)
}

fn durable_write_atomic(
    destination: &Path,
    bytes: &[u8],
    namespace: &'static str,
    failpoints: &dyn IntegrityFailpoints,
) -> Result<()> {
    let parent = destination
        .parent()
        .context("receipt-integrity destination has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .context("receipt-integrity filename is not Unicode")?,
        std::process::id()
    ));
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    failpoints.hit(match namespace {
        "anchor.prepare" => "anchor.prepare.write.before",
        "anchor.finalized_version" => "anchor.finalized_version.write.before",
        "anchor.current" => "anchor.current.write.before",
        "anchor.quarantine" => "anchor.quarantine.write.before",
        _ => "anchor.unknown.write.before",
    })?;
    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        failpoints.hit(match namespace {
            "anchor.prepare" => "anchor.prepare.write.after",
            "anchor.finalized_version" => "anchor.finalized_version.write.after",
            "anchor.current" => "anchor.current.write.after",
            "anchor.quarantine" => "anchor.quarantine.write.after",
            _ => "anchor.unknown.write.after",
        })?;
        file.sync_all()?;
        failpoints.hit(match namespace {
            "anchor.prepare" => "anchor.prepare.sync.after",
            "anchor.finalized_version" => "anchor.finalized_version.sync.after",
            "anchor.current" => "anchor.current.sync.after",
            "anchor.quarantine" => "anchor.quarantine.sync.after",
            _ => "anchor.unknown.sync.after",
        })?;
        atomic_replace(&temporary, destination)?;
        failpoints.hit(match namespace {
            "anchor.prepare" => "anchor.prepare.rename.after",
            "anchor.finalized_version" => "anchor.finalized_version.rename.after",
            "anchor.current" => "anchor.current.rename.after",
            "anchor.quarantine" => "anchor.quarantine.rename.after",
            _ => "anchor.unknown.rename.after",
        })?;
        sync_directory(parent)?;
        failpoints.hit(match namespace {
            "anchor.prepare" => "anchor.prepare.dir_sync.after",
            "anchor.finalized_version" => "anchor.finalized_version.dir_sync.after",
            "anchor.current" => "anchor.current.dir_sync.after",
            "anchor.quarantine" => "anchor.quarantine.dir_sync.after",
            _ => "anchor.unknown.dir_sync.after",
        })?;
        Ok(())
    })();
    if write_result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn external_anchor_location(paths: &AppPaths) -> Result<(PathBuf, String)> {
    let absolute = if paths.root.is_absolute() {
        paths.root.clone()
    } else {
        std::env::current_dir()?.join(&paths.root)
    };
    let identity_path = fs::canonicalize(&absolute).unwrap_or_else(|_| absolute.clone());
    let normalized = normalize_root_identity_path(&identity_path);
    let root_digest = sha256_hex(normalized.as_bytes());
    let parent = absolute
        .parent()
        .context("state root has no parent for external receipt anchor")?;
    Ok((
        parent.join(format!(
            ".carsinos-receipt-integrity-{}",
            &root_digest[..24]
        )),
        format!("sha256:{root_digest}"),
    ))
}

fn normalize_root_identity_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn path_is_within(candidate: &Path, root: &Path) -> Result<bool> {
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir()?.join(candidate)
    };
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let candidate = fs::canonicalize(candidate)?;
    let root = fs::canonicalize(root)?;
    Ok(candidate.starts_with(root))
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("{label} is not a safe receipt-integrity identifier");
    }
    Ok(())
}

fn validate_digest(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be a lowercase SHA-256 hex digest");
    }
    Ok(())
}

fn anchor_id(generation: i64, transaction_id: &str) -> String {
    format!("anchor-{generation:020}-{transaction_id}")
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(bytes))
}

fn unix_timestamp() -> Result<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    i64::try_from(seconds).context("Unix timestamp exceeds SQLite integer range")
}

fn classify_integrity_error(error: &anyhow::Error) -> String {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("key") && (message.contains("unavailable") || message.contains("not found"))
    {
        "receipt_key_unavailable".to_owned()
    } else if message.contains("root identity") {
        "root_identity_mismatch".to_owned()
    } else if message.contains("state-root generation") {
        "state_root_generation_mismatch".to_owned()
    } else if message.contains("key generation") {
        "key_generation_mismatch".to_owned()
    } else if message.contains("rotation cross-signature") {
        "anchor_rotation_cross_signature_mismatch".to_owned()
    } else if message.contains("commit confirmation hmac") {
        "receipt_commit_confirmation_mismatch".to_owned()
    } else if message.contains("keyed integrity") || message.contains("integrity algorithm or tag")
    {
        "anchor_keyed_integrity_mismatch".to_owned()
    } else if message.contains("count/head") {
        "anchor_count_head_mismatch".to_owned()
    } else if message.contains("generation") {
        "anchor_generation_mismatch".to_owned()
    } else if message.contains("finalized-document chain") {
        "anchor_history_chain_mismatch".to_owned()
    } else if message.contains("digest") {
        "anchor_digest_mismatch".to_owned()
    } else if message.contains("current") || message.contains("versioned") {
        "anchor_document_mismatch".to_owned()
    } else {
        "anchor_recovery_mismatch".to_owned()
    }
}

fn sanitize_reason(reason: &str) -> String {
    let safe: String = reason
        .chars()
        .filter(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '_'
        })
        .take(96)
        .collect();
    if safe.is_empty() {
        "integrity_mismatch".to_owned()
    } else {
        safe
    }
}

fn read_quarantine_reason(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    if bytes.len() > 16 * 1024 {
        bail!("receipt-integrity quarantine marker is oversized");
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    value
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .context("receipt-integrity quarantine marker lacks a reason")
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both input strings are NUL-terminated and live for the call.
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error())
            .context("failed atomically replacing receipt-integrity document");
    }
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> Result<()> {
    // Windows does not expose a portable directory `fsync`; FlushFileBuffers
    // on a directory handle returns ERROR_ACCESS_DENIED.  File bytes are synced
    // before rename and `atomic_replace` uses MOVEFILE_WRITE_THROUGH, which is
    // the supported durability boundary for the rename and its metadata.
    if !path.is_dir() {
        bail!("receipt-integrity directory disappeared before durability check");
    }
    Ok(())
}

#[cfg(windows)]
fn production_protector(
    anchor_dir: &Path,
    root_identity: &str,
) -> Result<Arc<dyn ReceiptKeyProtector>> {
    Ok(Arc::new(WindowsDpapiProtector::new(
        anchor_dir.join("keys"),
        root_identity,
    )?))
}

#[cfg(target_os = "macos")]
fn production_protector(
    _anchor_dir: &Path,
    root_identity: &str,
) -> Result<Arc<dyn ReceiptKeyProtector>> {
    let keychain_access_group = option_env!("CARSINOS_KEYCHAIN_ACCESS_GROUP")
        .context("macOS receipt custody requires a build-bound Keychain access group")?;
    let team_id = keychain_access_group
        .strip_suffix(".io.carsinos.missioncontrol")
        .context("macOS receipt Keychain access group must match the app identifier")?;
    if team_id.len() != 10
        || !team_id
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        bail!("macOS receipt Keychain access group is invalid");
    }
    Ok(Arc::new(MacKeychainProtector {
        root_identity: root_identity.to_owned(),
        keychain_access_group,
    }))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn production_protector(
    _anchor_dir: &Path,
    _root_identity: &str,
) -> Result<Arc<dyn ReceiptKeyProtector>> {
    bail!("receipt key custody is supported only on Windows and macOS production hosts")
}

#[cfg(windows)]
struct WindowsDpapiProtector {
    key_dir: PathBuf,
    entropy: [u8; 32],
}

#[cfg(windows)]
impl WindowsDpapiProtector {
    fn new(key_dir: PathBuf, root_identity: &str) -> Result<Self> {
        fs::create_dir_all(&key_dir)?;
        sync_directory(
            key_dir
                .parent()
                .context("DPAPI key directory has no parent")?,
        )?;
        Ok(Self {
            key_dir,
            entropy: Sha256::digest(root_identity.as_bytes()).into(),
        })
    }

    fn path(&self, key: &ReceiptKeyRef) -> PathBuf {
        self.key_dir
            .join(format!("{}-{:020}.dpapi", key.key_id, key.key_generation))
    }

    fn protect(&self, plaintext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let input = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len().try_into()?,
            pbData: plaintext.as_ptr().cast_mut(),
        };
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: self.entropy.len().try_into()?,
            pbData: self.entropy.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: input/entropy point to live slices; output is initialized by
        // DPAPI and released with LocalFree below. UI is explicitly forbidden
        // and the machine-wide DPAPI scope flag is intentionally absent.
        let succeeded = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if succeeded == 0 {
            bail!("Windows DPAPI failed protecting the receipt key");
        }
        // SAFETY: DPAPI returned `output.cbData` bytes at `output.pbData`.
        let protected =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        // SAFETY: DPAPI allocates output with LocalAlloc.
        unsafe { LocalFree(output.pbData.cast()) };
        Ok(Zeroizing::new(protected))
    }

    fn unprotect(&self, protected: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let input = CRYPT_INTEGER_BLOB {
            cbData: protected.len().try_into()?,
            pbData: protected.as_ptr().cast_mut(),
        };
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: self.entropy.len().try_into()?,
            pbData: self.entropy.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: pointers refer to live slices and DPAPI owns the output.
        let succeeded = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if succeeded == 0 {
            bail!("Windows DPAPI could not load the current-user receipt key");
        }
        // SAFETY: DPAPI returned `output.cbData` bytes at `output.pbData`.
        let plaintext =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        // SAFETY: DPAPI returned a writable LocalAlloc buffer of exactly this
        // length. Erase its plaintext before returning it to the allocator.
        unsafe {
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
            LocalFree(output.pbData.cast());
        }
        Ok(Zeroizing::new(plaintext))
    }
}

#[cfg(windows)]
impl ReceiptKeyProtector for WindowsDpapiProtector {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        use windows_sys::Win32::Security::Cryptography::{
            BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        };

        let path = self.path(key);
        if path.exists() {
            bail!("receipt key already exists in Windows DPAPI custody");
        }
        let mut plaintext = Zeroizing::new(vec![0_u8; KEY_BYTES]);
        // SAFETY: BCrypt writes exactly the supplied live mutable buffer.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                plaintext.as_mut_ptr(),
                plaintext.len().try_into()?,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status < 0 {
            bail!("Windows CNG failed generating the receipt key");
        }
        let protected = self.protect(&plaintext)?;
        fs::create_dir_all(&self.key_dir)?;
        durable_write_atomic(&path, &protected, "key.dpapi", &NoFailpoints)?;
        Ok(plaintext)
    }

    fn load(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let protected = Zeroizing::new(
            fs::read(self.path(key)).context("Windows DPAPI receipt key blob is unavailable")?,
        );
        self.unprotect(&protected)
    }

    fn delete(&self, key: &ReceiptKeyRef) -> Result<()> {
        let path = self.path(key);
        if path.exists() {
            fs::remove_file(path)?;
            sync_directory(&self.key_dir)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
struct MacKeychainProtector {
    root_identity: String,
    keychain_access_group: &'static str,
}

#[cfg(target_os = "macos")]
impl MacKeychainProtector {
    fn options(&self, key: &ReceiptKeyRef) -> security_framework::passwords::PasswordOptions {
        let account = format!(
            "{}:{}:{}",
            self.root_identity, key.key_id, key.key_generation
        );
        let mut options = security_framework::passwords::PasswordOptions::new_generic_password(
            "com.carsinos.execass.receipt-integrity",
            &account,
        );
        options.use_protected_keychain();
        options.set_access_synchronized(Some(false));
        options.set_access_group(self.keychain_access_group);
        options
    }
}

#[cfg(target_os = "macos")]
impl ReceiptKeyProtector for MacKeychainProtector {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        use security_framework::passwords::{generic_password, set_generic_password_options};
        use security_framework::random::SecRandom;

        if generic_password(self.options(key)).is_ok() {
            bail!("receipt key already exists in the Data Protection Keychain");
        }
        let mut material = Zeroizing::new(vec![0_u8; KEY_BYTES]);
        SecRandom::default()
            .copy_bytes(&mut material)
            .context("Security.framework failed generating the receipt key")?;
        set_generic_password_options(&material, self.options(key))
            .context("Data Protection Keychain failed storing the receipt key")?;
        Ok(material)
    }

    fn load(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        use security_framework::passwords::generic_password;
        let material = generic_password(self.options(key))
            .context("Data Protection Keychain receipt key is unavailable")?;
        Ok(Zeroizing::new(material))
    }

    fn delete(&self, key: &ReceiptKeyRef) -> Result<()> {
        use security_framework::passwords::delete_generic_password_options;
        delete_generic_password_options(self.options(key))
            .context("Data Protection Keychain failed deleting the receipt key")
    }
}
