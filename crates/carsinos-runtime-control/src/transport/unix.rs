use crate::{
    client_exchange_stream, handle_stream, hex_encode, RuntimeControlEndpoint, RuntimeControlError,
    RuntimeControlReplyV1, RuntimeControlRequestV1, ServerState,
};
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};

pub(crate) async fn client_exchange(
    endpoint: &RuntimeControlEndpoint,
    request: &RuntimeControlRequestV1,
) -> Result<RuntimeControlReplyV1, RuntimeControlError> {
    validate_endpoint_base(endpoint)?;
    validate_owner_only_socket(&endpoint.socket_path)?;
    let mut stream = UnixStream::connect(&endpoint.socket_path)
        .await
        .map_err(|_| RuntimeControlError::Transport)?;
    let credentials = stream
        .peer_cred()
        .map_err(|_| RuntimeControlError::Transport)?;
    let effective_uid = unsafe { libc::geteuid() };
    if credentials.uid() != effective_uid {
        return Err(RuntimeControlError::Transport);
    }
    client_exchange_stream(&mut stream, request).await
}

pub(crate) async fn serve(
    endpoint: RuntimeControlEndpoint,
    state: Arc<ServerState>,
) -> Result<(), RuntimeControlError> {
    let path = endpoint.socket_path;
    let parent = path.parent().ok_or(RuntimeControlError::Transport)?;
    let effective_uid = unsafe { libc::geteuid() };
    if parent != runtime_base_dir()? {
        return Err(RuntimeControlError::Transport);
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !metadata.file_type().is_socket() || metadata.uid() != effective_uid {
            return Err(RuntimeControlError::Transport);
        }
        std::fs::remove_file(&path).map_err(|_| RuntimeControlError::Transport)?;
    }
    let listener = UnixListener::bind(&path).map_err(|_| RuntimeControlError::Transport)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| RuntimeControlError::Transport)?;
    // Re-read the bound pathname after chmod. The owner-only runtime base
    // excludes foreign UIDs from replacing this entry; this final check also
    // rejects unexpected filesystem or platform behavior before accepting.
    validate_owner_only_socket(&path)?;
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|_| RuntimeControlError::Transport)?;
        let credentials = stream
            .peer_cred()
            .map_err(|_| RuntimeControlError::Transport)?;
        if credentials.uid() != effective_uid
            || unix_uid_digest(credentials.uid()) != state.scope.os_user_identity_digest
        {
            continue;
        }
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let _ = handle_stream(stream, state).await;
        });
    }
}

fn validate_endpoint_base(endpoint: &RuntimeControlEndpoint) -> Result<(), RuntimeControlError> {
    if endpoint.socket_path.parent() != Some(runtime_base_dir()?.as_path()) {
        return Err(RuntimeControlError::Transport);
    }
    Ok(())
}

fn validate_owner_only_socket(path: &Path) -> Result<(), RuntimeControlError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| RuntimeControlError::Transport)?;
    let effective_uid = unsafe { libc::geteuid() };
    if !metadata.file_type().is_socket()
        || metadata.uid() != effective_uid
        || metadata.mode() & 0o077 != 0
    {
        return Err(RuntimeControlError::Transport);
    }
    Ok(())
}

pub(crate) fn runtime_base_dir() -> Result<PathBuf, RuntimeControlError> {
    let base = platform_runtime_base_dir()?;
    validate_runtime_base_path(&base)?;
    Ok(base)
}

fn validate_runtime_base_path(base: &Path) -> Result<(), RuntimeControlError> {
    if !base.is_absolute() {
        return Err(RuntimeControlError::Transport);
    }
    let metadata = std::fs::symlink_metadata(&base).map_err(|_| RuntimeControlError::Transport)?;
    let effective_uid = unsafe { libc::geteuid() };
    if !metadata.file_type().is_dir()
        || metadata.uid() != effective_uid
        || metadata.mode() & 0o077 != 0
    {
        return Err(RuntimeControlError::Transport);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_runtime_base_dir() -> Result<PathBuf, RuntimeControlError> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or(RuntimeControlError::Transport)
}

#[cfg(target_os = "macos")]
fn platform_runtime_base_dir() -> Result<PathBuf, RuntimeControlError> {
    let required =
        unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0) };
    if required <= 1 {
        return Err(RuntimeControlError::Transport);
    }
    let mut buffer = vec![0u8; required];
    let written = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if written == 0 || written > buffer.len() {
        return Err(RuntimeControlError::Transport);
    }
    buffer.truncate(written.saturating_sub(1));
    Ok(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_runtime_base_dir() -> Result<PathBuf, RuntimeControlError> {
    Err(RuntimeControlError::UnsupportedPlatform)
}

pub(crate) fn current_os_user_identity_digest() -> Result<String, RuntimeControlError> {
    Ok(unix_uid_digest(unsafe { libc::geteuid() }))
}

fn unix_uid_digest(uid: u32) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.macos-euid.v1");
    digest.update([0]);
    digest.update(uid.to_be_bytes());
    hex_encode(&digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_validation_rejects_symlink_and_shared_permissions() {
        let root = tempfile::tempdir().unwrap();
        let effective_uid = unsafe { libc::geteuid() };
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(validate_runtime_base_path(root.path()), Ok(()));
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o770)).unwrap();
        assert_eq!(
            validate_runtime_base_path(root.path()),
            Err(RuntimeControlError::Transport)
        );
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let substituted = root.path().join("substituted");
        std::os::unix::fs::symlink(&real, &substituted).unwrap();
        assert_eq!(
            validate_runtime_base_path(&substituted),
            Err(RuntimeControlError::Transport)
        );
        assert_eq!(
            std::fs::symlink_metadata(root.path()).unwrap().uid(),
            effective_uid
        );
    }

    #[test]
    fn client_rejects_a_regular_file_at_the_socket_path() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("not-a-socket");
        std::fs::write(&path, b"not a socket").unwrap();
        assert_eq!(
            validate_owner_only_socket(&path),
            Err(RuntimeControlError::Transport)
        );
    }
}
