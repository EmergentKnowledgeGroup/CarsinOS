use anyhow::{bail, Context};
use carsinos_protocol::execass_recorder::canonical_root_identity_from_canonical_path;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalStateRoot {
    pub path: PathBuf,
    pub identity: String,
}

/// Resolve filesystem aliases and derive the exact identity used by
/// `ExecAssStore`. Keeping this in the recorder library ensures the binary and
/// its process tests cannot silently drift to lexical path hashing.
pub fn canonical_state_root(path: &Path) -> anyhow::Result<CanonicalStateRoot> {
    let path = path
        .canonicalize()
        .context("failed resolving the configured ExecAss state root")?;
    if !path.is_dir() {
        bail!("configured ExecAss state root is not a directory");
    }
    Ok(CanonicalStateRoot {
        identity: canonical_root_identity_from_canonical_path(&path.to_string_lossy()),
        path,
    })
}

pub fn canonical_database_for_root(
    database: &Path,
    state_root: &CanonicalStateRoot,
) -> anyhow::Result<PathBuf> {
    let database = database
        .canonicalize()
        .context("failed resolving the configured ExecAss database")?;
    let expected = state_root
        .path
        .join("carsinos.db")
        .canonicalize()
        .context("failed resolving the canonical ExecAss database location")?;
    if database != expected {
        bail!("ExecAss recorder database must be canonical carsinos.db inside its state root");
    }
    Ok(database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn lexical_aliases_have_one_identity_and_other_roots_do_not() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".ea213-test-tmp")
            .join(format!(
                "root-identity-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let root = base.join("state");
        fs::create_dir_all(root.join("alias-child")).unwrap();
        let direct = canonical_state_root(&root).unwrap();
        let alias = canonical_state_root(&root.join("alias-child/..")).unwrap();
        assert_eq!(direct, alias);

        let other = base.join("other");
        fs::create_dir_all(&other).unwrap();
        assert_ne!(
            direct.identity,
            canonical_state_root(&other).unwrap().identity
        );
    }
}
