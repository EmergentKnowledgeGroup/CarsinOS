//! Exact install-relative supervision for the bounded effect-recorder sidecar.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, ChildStdin, Command};

#[cfg(unix)]
use std::ffi::CString;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

const PACKAGED_FILE_NAME: Option<&str> = option_env!("CARSINOS_PACKAGED_EFFECT_RECORDER_FILE_NAME");
const PACKAGED_SHA256: Option<&str> = option_env!("CARSINOS_PACKAGED_EFFECT_RECORDER_SHA256");

/// A live child plus its parent-liveness pipe. The recorder never becomes a
/// scheduler/runtime sibling: it can only validate an already-committed
/// invoking attempt and record one bounded provider effect.
pub(crate) struct RecorderSidecarSupervisor {
    child: Child,
    _parent_liveness: ChildStdin,
    verified_artifact: VerifiedPackagedArtifact,
}

impl std::fmt::Debug for RecorderSidecarSupervisor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecorderSidecarSupervisor")
            .field("process_id", &self.child.id())
            .field("executable", &self.verified_artifact.executable)
            .field("sha256", &self.verified_artifact.sha256)
            .finish_non_exhaustive()
    }
}

impl RecorderSidecarSupervisor {
    pub(crate) async fn launch(state_root: &Path, database: &Path) -> Result<Option<Self>> {
        let Some((file_name, expected_sha256)) = packaged_contract()? else {
            return Ok(None);
        };
        let state_root = state_root.to_path_buf();
        let database = database.to_path_buf();
        let (verified_artifact, canonical_root, canonical_database) =
            tokio::task::spawn_blocking(move || {
                let current_exe = std::env::current_exe()
                    .context("failed resolving the installed runtime-host executable")?;
                let verified_artifact =
                    prepare_packaged_artifact(&current_exe, file_name, expected_sha256)?;
                let canonical_root = state_root
                    .canonicalize()
                    .context("failed canonicalizing recorder state root")?;
                let canonical_database = database
                    .canonicalize()
                    .context("failed canonicalizing recorder database")?;
                Ok::<_, anyhow::Error>((verified_artifact, canonical_root, canonical_database))
            })
            .await
            .context("the packaged effect-recorder verification task failed")??;
        if canonical_database.parent() != Some(canonical_root.as_path()) {
            bail!("the recorder database is outside its canonical state root");
        }
        let mut child = Command::new(&verified_artifact.executable)
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
            verified_artifact,
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

struct VerifiedPackagedArtifact {
    executable: PathBuf,
    sha256: String,
    // On Windows this handle allows reads but denies write/delete sharing, so
    // the path cannot be replaced between hashing and CreateProcess. On Unix,
    // the handle pins the exact hashed inode while the permission proof below
    // prevents the invoking user from changing the file or its directory.
    _verified_handle: File,
}

fn prepare_packaged_artifact(
    current_exe: &Path,
    file_name: &str,
    expected_sha256: &str,
) -> Result<VerifiedPackagedArtifact> {
    let executable = resolve_install_relative(current_exe, file_name)?;
    let mut verified_handle = open_verified_artifact(&executable)?;
    validate_install_immutability(&executable, &verified_handle)?;
    let actual_sha256 = sha256_reader(&mut verified_handle, &executable)?;
    if actual_sha256 != expected_sha256 {
        bail!("the packaged effect-recorder artifact digest is invalid");
    }
    Ok(VerifiedPackagedArtifact {
        executable,
        sha256: actual_sha256,
        _verified_handle: verified_handle,
    })
}

#[cfg(windows)]
fn open_verified_artifact(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .with_context(|| format!("failed locking packaged artifact {}", path.display()))
}

#[cfg(not(windows))]
fn open_verified_artifact(path: &Path) -> Result<File> {
    File::open(path).with_context(|| format!("failed opening packaged artifact {}", path.display()))
}

#[cfg(windows)]
fn validate_install_immutability(_path: &Path, _file: &File) -> Result<()> {
    // open_verified_artifact keeps a non-write/non-delete-sharing handle alive
    // through process startup and for the lifetime of the child.
    Ok(())
}

#[cfg(unix)]
fn validate_install_immutability(path: &Path, file: &File) -> Result<()> {
    let file_metadata = file.metadata().with_context(|| {
        format!(
            "failed reading packaged artifact metadata {}",
            path.display()
        )
    })?;
    let install_dir = path
        .parent()
        .context("the packaged effect-recorder artifact has no install directory")?;
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    let effective_uid = unsafe { geteuid() };
    if !unix_artifact_snapshot_is_immutable(
        effective_uid,
        file_metadata.uid(),
        file_metadata.mode(),
    ) || unix_access_allows_write(path)?
    {
        bail!("the packaged effect-recorder artifact or install directory is mutable by the runtime user");
    }
    for ancestor in install_dir.ancestors() {
        let metadata = ancestor.metadata().with_context(|| {
            format!(
                "failed reading packaged artifact ancestor metadata {}",
                ancestor.display()
            )
        })?;
        if !unix_directory_snapshot_is_immutable(effective_uid, metadata.uid(), metadata.mode())
            || unix_access_allows_write(ancestor)?
        {
            bail!("the packaged effect-recorder install ancestry is mutable by the runtime user");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn unix_access_allows_write(path: &Path) -> Result<bool> {
    unsafe extern "C" {
        fn access(path: *const std::ffi::c_char, mode: std::ffi::c_int) -> std::ffi::c_int;
    }
    const WRITE_OK: std::ffi::c_int = 2;
    let path = CString::new(path.as_os_str().as_bytes())
        .context("the packaged effect-recorder install path contains an interior NUL")?;
    Ok(unsafe { access(path.as_ptr(), WRITE_OK) } == 0)
}

#[cfg(unix)]
fn unix_artifact_snapshot_is_immutable(effective_uid: u32, owner_uid: u32, mode: u32) -> bool {
    owner_uid != effective_uid && mode & 0o022 == 0 && mode & 0o6000 == 0
}

#[cfg(unix)]
fn unix_directory_snapshot_is_immutable(effective_uid: u32, owner_uid: u32, mode: u32) -> bool {
    owner_uid != effective_uid && mode & 0o022 == 0
}

#[cfg(all(unix, test))]
fn unix_ancestor_snapshots_are_immutable(effective_uid: u32, ancestors: &[(u32, u32)]) -> bool {
    ancestors.iter().all(|(owner_uid, mode)| {
        unix_directory_snapshot_is_immutable(effective_uid, *owner_uid, *mode)
    })
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

#[cfg(test)]
fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("failed opening packaged artifact {}", path.display()))?;
    sha256_reader(&mut file, path)
}

fn sha256_reader(reader: &mut File, path: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
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

    #[cfg(windows)]
    #[test]
    fn verified_artifact_handle_allows_launch_but_blocks_write_and_replacement() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let host = directory.path().join("carsinos-gateway.exe");
        let recorder = directory.path().join("carsinos-effect-recorder.exe");
        std::fs::write(&host, b"host").unwrap();
        let command_interpreter = std::env::var_os("ComSpec").unwrap();
        std::fs::copy(command_interpreter, &recorder).unwrap();
        let expected_sha256 = sha256_file(&recorder).unwrap();
        let artifact =
            prepare_packaged_artifact(&host, "carsinos-effect-recorder.exe", &expected_sha256)
                .unwrap();

        assert!(OpenOptions::new().write(true).open(&recorder).is_err());
        assert!(std::fs::remove_file(&recorder).is_err());
        assert!(std::fs::rename(&recorder, recorder.with_extension("swapped")).is_err());
        assert!(std::process::Command::new(&artifact.executable)
            .args(["/D", "/C", "exit 0"])
            .status()
            .unwrap()
            .success());

        drop(artifact);
        assert!(OpenOptions::new().write(true).open(&recorder).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn unix_immutability_rejects_mutable_artifact_and_any_mutable_ancestor() {
        assert!(unix_artifact_snapshot_is_immutable(501, 0, 0o100755));
        assert!(!unix_artifact_snapshot_is_immutable(501, 501, 0o100555));
        assert!(!unix_artifact_snapshot_is_immutable(501, 0, 0o100775));
        assert!(!unix_artifact_snapshot_is_immutable(501, 0, 0o104755));

        let immutable_chain = [(0, 0o40755), (0, 0o40755), (0, 0o40755)];
        assert!(unix_ancestor_snapshots_are_immutable(501, &immutable_chain));
        let mutable_ancestor = [(0, 0o40755), (0, 0o40777), (0, 0o40755)];
        assert!(!unix_ancestor_snapshots_are_immutable(
            501,
            &mutable_ancestor
        ));
    }
}
