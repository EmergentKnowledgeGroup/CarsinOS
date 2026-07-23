use super::{exchange_stream, handle_stream, RecorderEndpoint};
use crate::service::RecorderService;
use crate::RecorderIdentity;
use anyhow::{bail, Context, Result};
use carsinos_protocol::execass_recorder::{RecorderReplyV1, RecorderRequestV1};
use sha2::{Digest, Sha256};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};

pub(super) async fn client_exchange(
    endpoint: &RecorderEndpoint,
    request: &RecorderRequestV1,
    expected_identity: &RecorderIdentity,
) -> Result<RecorderReplyV1> {
    let RecorderEndpoint::UnixSocket(path) = endpoint;
    let mut stream = UnixStream::connect(path).await?;
    exchange_stream(&mut stream, request, expected_identity).await
}

pub(super) async fn serve(
    endpoint: RecorderEndpoint,
    expected_peer_identity_digest: String,
    service: Arc<RecorderService>,
) -> Result<()> {
    let RecorderEndpoint::UnixSocket(path) = endpoint;
    let parent = path.parent().context("recorder socket has no parent")?;
    std::fs::create_dir_all(parent)?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    let effective_uid = unsafe { libc::geteuid() };
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !metadata.file_type().is_socket() || metadata.uid() != effective_uid {
            bail!("recorder socket path is occupied by an unsafe object");
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    loop {
        let (stream, _) = listener.accept().await?;
        let credential = stream.peer_cred()?;
        if credential.uid() != effective_uid
            || unix_uid_digest(credential.uid()) != expected_peer_identity_digest
        {
            tracing::warn!("recorder Unix peer identity rejected");
            continue;
        }
        let service = Arc::clone(&service);
        tokio::spawn(async move {
            if let Err(error) = handle_stream(stream, service).await {
                tracing::warn!(error = %error, "recorder Unix-socket request failed closed");
            }
        });
    }
}

fn unix_uid_digest(uid: u32) -> String {
    crate::hex_encode(&Sha256::digest(
        format!("carsinos.execass.macos-euid.v1:{uid}").as_bytes(),
    ))
}

pub(super) fn current_peer_identity_digest() -> Result<String> {
    Ok(unix_uid_digest(unsafe { libc::geteuid() }))
}
