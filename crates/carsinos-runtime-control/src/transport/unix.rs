use crate::{
    client_exchange_stream, handle_stream, hex_encode, RuntimeControlEndpoint, RuntimeControlError,
    RuntimeControlReplyV1, RuntimeControlRequestV1, ServerState,
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};

pub(crate) async fn client_exchange(
    endpoint: &RuntimeControlEndpoint,
    request: &RuntimeControlRequestV1,
) -> Result<RuntimeControlReplyV1, RuntimeControlError> {
    let mut stream = UnixStream::connect(&endpoint.socket_path)
        .await
        .map_err(|_| RuntimeControlError::Transport)?;
    client_exchange_stream(&mut stream, request).await
}

pub(crate) async fn serve(
    endpoint: RuntimeControlEndpoint,
    state: Arc<ServerState>,
) -> Result<(), RuntimeControlError> {
    let path = endpoint.socket_path;
    let parent = path.parent().ok_or(RuntimeControlError::Transport)?;
    std::fs::create_dir_all(parent).map_err(|_| RuntimeControlError::Transport)?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| RuntimeControlError::Transport)?;
    let effective_uid = unsafe { libc::geteuid() };
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !metadata.file_type().is_socket() || metadata.uid() != effective_uid {
            return Err(RuntimeControlError::Transport);
        }
        std::fs::remove_file(&path).map_err(|_| RuntimeControlError::Transport)?;
    }
    let listener = UnixListener::bind(&path).map_err(|_| RuntimeControlError::Transport)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| RuntimeControlError::Transport)?;
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
