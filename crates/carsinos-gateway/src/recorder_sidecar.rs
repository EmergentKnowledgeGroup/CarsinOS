//! Exact install-relative supervision for the bounded effect-recorder sidecar.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, ChildStdin, Command};

const PACKAGED_FILE_NAME: Option<&str> = option_env!("CARSINOS_PACKAGED_EFFECT_RECORDER_FILE_NAME");
const PACKAGED_SHA256: Option<&str> = option_env!("CARSINOS_PACKAGED_EFFECT_RECORDER_SHA256");

/// A live child plus its parent-liveness pipe. The recorder never becomes a
/// scheduler/runtime sibling: it can only validate an already-committed
/// invoking attempt and record one bounded provider effect.
pub(crate) struct RecorderSidecarSupervisor {
    child: Child,
    _parent_liveness: ChildStdin,
    executable: PathBuf,
    sha256: String,
}

impl std::fmt::Debug for RecorderSidecarSupervisor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecorderSidecarSupervisor")
            .field("process_id", &self.child.id())
            .field("executable", &self.executable)
            .field("sha256", &self.sha256)
            .finish_non_exhaustive()
    }
}

impl RecorderSidecarSupervisor {
    pub(crate) async fn launch(state_root: &Path, database: &Path) -> Result<Option<Self>> {
        let Some((file_name, expected_sha256)) = packaged_contract()? else {
            return Ok(None);
        };
        let current_exe = std::env::current_exe()
            .context("failed resolving the installed runtime-host executable")?;
        let executable = resolve_install_relative(&current_exe, file_name)?;
        let actual_sha256 = sha256_file(&executable)?;
        if actual_sha256 != expected_sha256 {
            bail!("the packaged effect-recorder artifact digest is invalid");
        }
        let canonical_root = state_root
            .canonicalize()
            .context("failed canonicalizing recorder state root")?;
        let canonical_database = database
            .canonicalize()
            .context("failed canonicalizing recorder database")?;
        if canonical_database.parent() != Some(canonical_root.as_path()) {
            bail!("the recorder database is outside its canonical state root");
        }
        let mut child = Command::new(&executable)
            .arg("--state-root")
            .arg(&canonical_root)
            .arg("--database")
            .arg(&canonical_database)
            .arg("--parent-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("failed starting the verified packaged effect recorder")?;
        let parent_liveness = child
            .stdin
            .take()
            .context("the packaged recorder did not expose its parent-liveness pipe")?;
        Ok(Some(Self {
            child,
            _parent_liveness: parent_liveness,
            executable,
            sha256: actual_sha256,
        }))
    }

    pub(crate) fn ensure_running(&mut self) -> Result<()> {
        if let Some(status) = self
            .child
            .try_wait()
            .context("failed reading the packaged recorder process state")?
        {
            bail!("the packaged effect recorder exited during startup: {status}");
        }
        Ok(())
    }
}

impl Drop for RecorderSidecarSupervisor {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn packaged_contract() -> Result<Option<(&'static str, &'static str)>> {
    match (PACKAGED_FILE_NAME, PACKAGED_SHA256) {
        (Some(file_name), Some(digest)) if valid_file_name(file_name) && valid_sha256(digest) => {
            Ok(Some((file_name, digest)))
        }
        (None, None) => Ok(None),
        _ => bail!("the compiled effect-recorder package contract is incomplete or malformed"),
    }
}

fn valid_file_name(value: &str) -> bool {
    !value.is_empty()
        && value
            == Path::new(value)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
        && !value.contains(['/', '\\'])
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn resolve_install_relative(current_exe: &Path, file_name: &str) -> Result<PathBuf> {
    if !valid_file_name(file_name) {
        bail!("the packaged effect-recorder filename is invalid");
    }
    let install_dir = current_exe
        .parent()
        .context("the runtime-host executable has no install directory")?
        .canonicalize()
        .context("failed canonicalizing the runtime-host install directory")?;
    let candidate = install_dir.join(file_name);
    let canonical = candidate
        .canonicalize()
        .context("the packaged effect-recorder artifact is missing")?;
    if canonical.parent() != Some(install_dir.as_path()) || !canonical.is_file() {
        bail!("the packaged effect-recorder artifact escaped its install directory");
    }
    Ok(canonical)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed opening packaged artifact {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed hashing packaged artifact {}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn package_contract_components_are_closed_and_canonical() {
        assert!(valid_file_name("carsinos-effect-recorder.exe"));
        assert!(!valid_file_name("..\\carsinos-effect-recorder.exe"));
        assert!(!valid_file_name("sub/carsinos-effect-recorder"));
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(!valid_sha256(&"A".repeat(64)));
        assert!(!valid_sha256("short"));
    }

    #[test]
    fn install_relative_resolution_and_streaming_digest_are_exact() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let host = directory.path().join("carsinos-gateway.exe");
        let recorder = directory.path().join("carsinos-effect-recorder.exe");
        std::fs::File::create(&host)
            .unwrap()
            .write_all(b"host")
            .unwrap();
        std::fs::File::create(&recorder)
            .unwrap()
            .write_all(b"recorder")
            .unwrap();
        assert_eq!(
            resolve_install_relative(&host, "carsinos-effect-recorder.exe").unwrap(),
            recorder.canonicalize().unwrap()
        );
        assert_eq!(
            sha256_file(&recorder).unwrap(),
            "93384247058b5e037a16c08536d5a3b3c20453cda6571c7e016942f9f93b274f"
        );
    }
}
