#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use windows::{client_exchange, current_os_user_identity_digest, serve};

#[cfg(unix)]
pub(crate) use unix::{client_exchange, current_os_user_identity_digest, serve};

#[cfg(not(any(windows, unix)))]
pub(crate) async fn client_exchange(
    _endpoint: &crate::RuntimeControlEndpoint,
    _request: &crate::RuntimeControlRequestV1,
) -> Result<crate::RuntimeControlReplyV1, crate::RuntimeControlError> {
    Err(crate::RuntimeControlError::UnsupportedPlatform)
}

#[cfg(not(any(windows, unix)))]
pub(crate) async fn serve(
    _endpoint: crate::RuntimeControlEndpoint,
    _state: std::sync::Arc<crate::ServerState>,
) -> Result<(), crate::RuntimeControlError> {
    Err(crate::RuntimeControlError::UnsupportedPlatform)
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn current_os_user_identity_digest() -> Result<String, crate::RuntimeControlError> {
    Err(crate::RuntimeControlError::UnsupportedPlatform)
}
