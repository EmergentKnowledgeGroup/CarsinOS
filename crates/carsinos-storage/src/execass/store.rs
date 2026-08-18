use crate::{
    execass_connection_schema_is_exact, execass_schema_is_exact, open_sqlite_connection,
    open_sqlite_connection_read_only, upgrade_execass_canonical_root_if_needed, AppPaths,
    EXECASS_APPLICATION_ID,
};
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::fmt;
use std::path::{Path, PathBuf};

/// A typed ExecAss v1 persistence facade.
///
/// No raw connection or generic transaction callback is exposed. Every
/// operation revalidates the exact schema on the writable connection it uses.
#[derive(Clone)]
pub struct ExecAssStore {
    pub(super) db_path: PathBuf,
    pub(super) root_path: PathBuf,
    pub(super) root_identity: String,
}

impl fmt::Debug for ExecAssStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecAssStore")
            .field("database_configured", &true)
            .finish()
    }
}

impl ExecAssStore {
    #[cfg(feature = "execass-test-confirmation-runtime")]
    #[doc(hidden)]
    pub fn test_app_paths(&self) -> AppPaths {
        AppPaths::from_root(self.root_path.clone())
    }

    /// Return `None` for a legacy/non-replacement root. Once a database claims
    /// the ExecAss replacement application identity, any schema/root mismatch
    /// is an error rather than a silent legacy fallback.
    pub fn open_if_canonical_root(paths: &AppPaths) -> Result<Option<Self>> {
        if !paths.db_path.is_file() {
            return Ok(None);
        }
        let conn = open_sqlite_connection_read_only(&paths.db_path)?;
        let application_id = conn
            .query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
            .context("failed reading candidate ExecAss application identity")?;
        if application_id != EXECASS_APPLICATION_ID {
            return Ok(None);
        }
        drop(conn);
        upgrade_execass_canonical_root_if_needed(paths)?;
        Self::open(paths).map(Some)
    }

    pub fn open(paths: &AppPaths) -> Result<Self> {
        if !paths.db_path.is_file() || !execass_schema_is_exact(&paths.db_path)? {
            bail!("refusing to open a non-canonical ExecAss v1 database");
        }
        let root_path = paths
            .root
            .canonicalize()
            .context("failed resolving the configured ExecAss state root")?;
        if !root_path.is_dir() {
            bail!("configured ExecAss state root is not a directory");
        }
        let db_path = paths
            .db_path
            .canonicalize()
            .context("failed resolving the configured ExecAss database")?;
        let expected_db_path = root_path
            .join("carsinos.db")
            .canonicalize()
            .context("failed resolving the canonical ExecAss database location")?;
        if db_path != expected_db_path {
            bail!("ExecAss database must be the canonical carsinos.db inside its state root");
        }
        let root_identity = root_identity_for_path(&root_path);
        Ok(Self {
            db_path,
            root_path,
            root_identity,
        })
    }

    pub(super) fn connection(&self) -> Result<Connection> {
        let current_root = self
            .root_path
            .canonicalize()
            .context("configured ExecAss state root became unavailable")?;
        if current_root != self.root_path
            || self.db_path.parent() != Some(self.root_path.as_path())
            || root_identity_for_path(&current_root) != self.root_identity
        {
            bail!("ExecAss state-root identity changed after store construction");
        }
        let conn = open_sqlite_connection(&self.db_path)?;
        if !execass_connection_schema_is_exact(&conn)? {
            bail!("ExecAss schema identity changed after store construction");
        }
        Ok(conn)
    }

    /// Opens or creates the one fixed, current-user OS-custodied confirmation
    /// authority for this canonical state root and pins its public identity.
    pub fn activate_confirmation_authority(
        &self,
    ) -> Result<super::confirmation_custody::ConfirmationAuthorityIdentity> {
        super::confirmation_custody::activate_confirmation_authority(self)
    }

    pub fn open_receipt_integrity_store(
        &self,
    ) -> Result<super::receipt_integrity::ReceiptIntegrityStore> {
        super::receipt_integrity::ReceiptIntegrityStore::open(&AppPaths::from_root(
            self.root_path.clone(),
        ))
    }

    #[cfg(feature = "execass-test-confirmation-runtime")]
    #[doc(hidden)]
    pub fn open_receipt_integrity_store_for_test(
        &self,
    ) -> Result<super::receipt_integrity::ReceiptIntegrityStore> {
        super::receipt_integrity::ReceiptIntegrityStore::open_for_test(&AppPaths::from_root(
            self.root_path.clone(),
        ))
    }
}

fn root_identity_for_path(path: &Path) -> String {
    carsinos_protocol::execass_recorder::canonical_root_identity_from_canonical_path(
        &path.to_string_lossy(),
    )
}

pub(super) fn immediate_transaction(conn: &mut Connection) -> Result<Transaction<'_>> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed starting ExecAss IMMEDIATE transaction")
}
