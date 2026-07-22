//! Authenticated native local control for a single CarsinOS runtime host.
//!
//! This crate deliberately has no HTTP listener, port, process discovery,
//! process termination, or handoff API. A native OS transport proves the local
//! peer identity, while a separate owner secret authenticates and binds each
//! versioned request/reply. Graceful shutdown produces authorization only.

mod transport;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use zeroize::Zeroizing;

const PROTOCOL_VERSION: &str = "carsinos.runtime-control.v1";
const MAX_FRAME_BYTES: usize = 64 * 1024;
const CLIENT_AUTH_DOMAIN: &[u8] = b"carsinos.runtime-control.client-auth.v1";
const SERVER_AUTH_DOMAIN: &[u8] = b"carsinos.runtime-control.server-auth.v1";
const ENDPOINT_DOMAIN: &[u8] = b"carsinos.runtime-control.endpoint.v1";
const DEFAULT_CLOCK_SKEW: Duration = Duration::from_secs(60);
const OWNER_SECRET_DERIVATION_DOMAIN: &[u8] =
    b"carsinos.runtime-control.owner-secret-derivation.v1";

type HmacSha256 = Hmac<Sha256>;

pub const DEFAULT_PROFILE_IDENTITY: &str = "io.carsinos.missioncontrol";

/// Derive the fixed control-channel MAC key from the existing native-owner
/// secret without exposing or reusing that variable-length secret directly.
pub fn derive_owner_control_key(owner_secret: &[u8]) -> Result<[u8; 32], RuntimeControlError> {
    if owner_secret.len() < 32 {
        return Err(RuntimeControlError::Authentication);
    }
    let mut digest = Sha256::new();
    digest.update(OWNER_SECRET_DERIVATION_DOMAIN);
    digest.update([0]);
    digest.update(owner_secret);
    Ok(digest.finalize().into())
}

/// Stable public identity of one local runtime-control scope. It contains only
/// hashes and identifiers; the owner secret is never part of this value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeControlScopeV1 {
    pub canonical_root_identity: String,
    pub profile_identity: String,
    pub os_user_identity_digest: String,
}

impl RuntimeControlScopeV1 {
    pub fn validate(&self) -> Result<(), RuntimeControlError> {
        if !is_sha256_identity(&self.canonical_root_identity)
            || !is_safe_text(&self.profile_identity, 256)
            || !is_lower_hex(&self.os_user_identity_digest, 64)
        {
            return Err(RuntimeControlError::InvalidScope);
        }
        Ok(())
    }
}

/// A native endpoint. Its actual pipe/socket name is intentionally not exposed
/// through diagnostics or `Debug`; it contains no secret regardless.
#[derive(Clone, PartialEq, Eq)]
pub struct RuntimeControlEndpoint {
    #[cfg(windows)]
    pipe_name: String,
    #[cfg(unix)]
    socket_path: PathBuf,
}

impl std::fmt::Debug for RuntimeControlEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RuntimeControlEndpoint([native endpoint redacted])")
    }
}

impl RuntimeControlEndpoint {
    pub fn for_scope(
        state_root: &Path,
        scope: &RuntimeControlScopeV1,
    ) -> Result<Self, RuntimeControlError> {
        scope.validate()?;
        let endpoint_id = endpoint_id(scope);
        #[cfg(windows)]
        {
            let _ = state_root;
            Ok(Self {
                pipe_name: format!(
                    r"\\.\pipe\carsinos-runtime-control-v1-{}",
                    &endpoint_id[..32]
                ),
            })
        }
        #[cfg(unix)]
        {
            Ok(Self {
                socket_path: state_root
                    .join("runtime")
                    .join("control")
                    .join("v1")
                    .join(format!("{endpoint_id}.sock")),
            })
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (state_root, endpoint_id);
            Err(RuntimeControlError::UnsupportedPlatform)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusRequestV1 {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AttachRequestV1 {
    pub client_instance_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GracefulShutdownRequestV1 {
    pub client_instance_id: String,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub confirmation: Option<CloseConfirmationBindingV1>,
}

/// Echoed binding for the one permitted close confirmation. The fields bind
/// confirmation to the exact original authenticated request rather than to a
/// reusable UI boolean.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CloseConfirmationBindingV1 {
    pub challenge: String,
    pub original_request_id: String,
    pub original_nonce: String,
    pub original_issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActualStateV1 {
    Starting,
    RunningAppBound,
    Handoff,
    RunningBackground,
    Draining,
    Faulted,
    Stopped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDesiredModeV1 {
    AppBound,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActiveWorkStatusV1 {
    pub active: bool,
    pub active_work_count: i64,
    pub nonterminal_delegation_count: i64,
    pub nonterminal_continuation_count: i64,
    pub nonterminal_effect_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusReplyV1 {
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub actual_state: RuntimeActualStateV1,
    pub desired_mode: RuntimeDesiredModeV1,
    pub start_at_login: bool,
    pub active_work: ActiveWorkStatusV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AttachReplyV1 {
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
}

/// Typed signal delivered to the gateway's parent supervision layer. Receipt
/// of this value authorizes a graceful drain/stop; it never kills a process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShutdownAuthorizationV1 {
    pub authorization_id: String,
    pub request_id: String,
    pub nonce: String,
    pub issued_at_ms: i64,
    pub client_instance_id: String,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    /// Exact opaque snapshot that received the close confirmation. `None`
    /// means the authenticated close observed no active work.
    pub confirmed_active_work_binding_digest: Option<String>,
    pub confirmed_active_work_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GracefulShutdownReplyV1 {
    #[serde(flatten)]
    pub disposition: GracefulShutdownDispositionV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "disposition",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GracefulShutdownDispositionV1 {
    Authorized {
        authorization: ShutdownAuthorizationV1,
    },
    ConfirmationRequired {
        active_work: ActiveWorkStatusV1,
        consequence: String,
        binding: CloseConfirmationBindingV1,
    },
    Rejected {
        reason: GracefulShutdownRejectionReasonV1,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GracefulShutdownRejectionReasonV1 {
    BackgroundMode,
    NotRunningAppBound,
    StaleHostBinding,
    InvalidConfirmation,
    CloseStateChanged,
    AlreadyAuthorized,
    SupervisorUnavailable,
}

/// Authenticated envelope metadata supplied to lifecycle handlers. The HMAC,
/// timestamp, scope, native peer, and replay checks have all succeeded before
/// this context is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeControlRequestContextV1 {
    pub request_id: String,
    pub nonce: String,
    pub issued_at_ms: i64,
}

/// Closed wire request envelope. Every operation is authenticated and replay
/// guarded over these exact bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeControlRequestV1 {
    pub protocol_version: String,
    pub scope: RuntimeControlScopeV1,
    pub request_id: String,
    pub nonce: String,
    pub issued_at_ms: i64,
    #[serde(flatten)]
    pub operation: RuntimeControlOperationV1,
    pub authentication_mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RuntimeControlOperationV1 {
    Status(StatusRequestV1),
    Attach(AttachRequestV1),
    GracefulShutdown(GracefulShutdownRequestV1),
}

/// Closed wire reply envelope. The echoed request identity prevents a signed
/// reply from one request being accepted for another.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeControlReplyV1 {
    pub protocol_version: String,
    pub request_id: String,
    pub nonce: String,
    pub issued_at_ms: i64,
    #[serde(flatten)]
    pub result: RuntimeControlResultV1,
    pub authentication_mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "result",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RuntimeControlResultV1 {
    Status(StatusReplyV1),
    Attached(AttachReplyV1),
    GracefulShutdown(GracefulShutdownReplyV1),
    Rejected { code: RuntimeControlRejectionCodeV1 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeControlRejectionCodeV1 {
    Unavailable,
    InvalidRequest,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlError {
    #[error("runtime control scope is invalid")]
    InvalidScope,
    #[error("runtime control request was rejected")]
    InvalidRequest,
    #[error("runtime control authentication failed")]
    Authentication,
    #[error("runtime control request is stale or outside the clock bound")]
    Timestamp,
    #[error("runtime control request was already used")]
    Replay,
    #[error("runtime control frame is invalid")]
    Frame,
    #[error("runtime control transport failed")]
    Transport,
    #[error("runtime control is unsupported on this platform")]
    UnsupportedPlatform,
}

/// A durable replay store can implement this trait. The provided in-memory
/// guard is safe for one live host but intentionally does not survive restart.
pub trait ReplayGuard: Send + Sync + 'static {
    fn check_and_record(
        &self,
        request_id: &str,
        nonce: &str,
        issued_at_ms: i64,
    ) -> Result<(), RuntimeControlError>;
}

#[derive(Default)]
pub struct InMemoryReplayGuard {
    seen: Mutex<HashSet<(String, String)>>,
}

impl ReplayGuard for InMemoryReplayGuard {
    fn check_and_record(
        &self,
        request_id: &str,
        nonce: &str,
        _issued_at_ms: i64,
    ) -> Result<(), RuntimeControlError> {
        let mut seen = self.seen.lock().map_err(|_| RuntimeControlError::Replay)?;
        if !seen.insert((request_id.to_owned(), nonce.to_owned())) {
            return Err(RuntimeControlError::Replay);
        }
        Ok(())
    }
}

/// Gateway-side implementation point. The control crate does not decide host
/// lifecycle state; it authenticates native requests and delegates operations
/// to the host. A graceful-shutdown reply is authorization only.
#[async_trait::async_trait]
pub trait RuntimeControlHandler: Send + Sync + 'static {
    async fn status(&self) -> Result<StatusReplyV1, RuntimeControlRejectionCodeV1>;
    async fn attach(
        &self,
        request: AttachRequestV1,
    ) -> Result<AttachReplyV1, RuntimeControlRejectionCodeV1>;
    async fn graceful_shutdown(
        &self,
        context: RuntimeControlRequestContextV1,
        request: GracefulShutdownRequestV1,
    ) -> Result<GracefulShutdownReplyV1, RuntimeControlRejectionCodeV1>;
}

struct ServerState {
    scope: RuntimeControlScopeV1,
    owner_secret: Zeroizing<[u8; 32]>,
    max_clock_skew: Duration,
    replay_guard: Arc<dyn ReplayGuard>,
    handler: Arc<dyn RuntimeControlHandler>,
}

/// Owner-side native control server. Constructing it validates that the scope
/// belongs to the current OS user before it opens a listener.
pub struct RuntimeControlServer {
    endpoint: RuntimeControlEndpoint,
    state: Arc<ServerState>,
}

impl std::fmt::Debug for RuntimeControlServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeControlServer")
            .field("endpoint", &self.endpoint)
            .field("scope", &self.state.scope)
            .field("owner_secret", &"[REDACTED]")
            .finish()
    }
}

impl RuntimeControlServer {
    pub fn new(
        state_root: &Path,
        scope: RuntimeControlScopeV1,
        owner_secret: [u8; 32],
        replay_guard: Arc<dyn ReplayGuard>,
        handler: Arc<dyn RuntimeControlHandler>,
    ) -> Result<Self, RuntimeControlError> {
        scope.validate()?;
        if current_os_user_identity_digest()? != scope.os_user_identity_digest {
            return Err(RuntimeControlError::InvalidScope);
        }
        Ok(Self {
            endpoint: RuntimeControlEndpoint::for_scope(state_root, &scope)?,
            state: Arc::new(ServerState {
                scope,
                owner_secret: Zeroizing::new(owner_secret),
                max_clock_skew: DEFAULT_CLOCK_SKEW,
                replay_guard,
                handler,
            }),
        })
    }

    pub fn with_max_clock_skew(mut self, max_clock_skew: Duration) -> Self {
        self.state = Arc::new(ServerState {
            scope: self.state.scope.clone(),
            owner_secret: Zeroizing::new(*self.state.owner_secret),
            max_clock_skew,
            replay_guard: Arc::clone(&self.state.replay_guard),
            handler: Arc::clone(&self.state.handler),
        });
        self
    }

    pub async fn serve(self) -> Result<(), RuntimeControlError> {
        transport::serve(self.endpoint, self.state).await
    }
}

/// Tauri-side authenticated native control client.
pub struct RuntimeControlClient {
    endpoint: RuntimeControlEndpoint,
    scope: RuntimeControlScopeV1,
    owner_secret: Zeroizing<[u8; 32]>,
    max_clock_skew: Duration,
}

impl std::fmt::Debug for RuntimeControlClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeControlClient")
            .field("endpoint", &self.endpoint)
            .field("scope", &self.scope)
            .field("owner_secret", &"[REDACTED]")
            .finish()
    }
}

impl RuntimeControlClient {
    pub fn new(
        state_root: &Path,
        scope: RuntimeControlScopeV1,
        owner_secret: [u8; 32],
    ) -> Result<Self, RuntimeControlError> {
        scope.validate()?;
        if current_os_user_identity_digest()? != scope.os_user_identity_digest {
            return Err(RuntimeControlError::InvalidScope);
        }
        Ok(Self {
            endpoint: RuntimeControlEndpoint::for_scope(state_root, &scope)?,
            scope,
            owner_secret: Zeroizing::new(owner_secret),
            max_clock_skew: DEFAULT_CLOCK_SKEW,
        })
    }

    pub fn with_max_clock_skew(mut self, max_clock_skew: Duration) -> Self {
        self.max_clock_skew = max_clock_skew;
        self
    }

    pub async fn status(&self) -> Result<StatusReplyV1, RuntimeControlError> {
        match self
            .exchange(RuntimeControlOperationV1::Status(StatusRequestV1 {}))
            .await?
        {
            RuntimeControlResultV1::Status(reply) => Ok(reply),
            _ => Err(RuntimeControlError::InvalidRequest),
        }
    }

    pub async fn attach(
        &self,
        request: AttachRequestV1,
    ) -> Result<AttachReplyV1, RuntimeControlError> {
        if !is_safe_text(&request.client_instance_id, 256) {
            return Err(RuntimeControlError::InvalidRequest);
        }
        match self
            .exchange(RuntimeControlOperationV1::Attach(request))
            .await?
        {
            RuntimeControlResultV1::Attached(reply) => Ok(reply),
            _ => Err(RuntimeControlError::InvalidRequest),
        }
    }

    pub async fn graceful_shutdown(
        &self,
        request: GracefulShutdownRequestV1,
    ) -> Result<GracefulShutdownReplyV1, RuntimeControlError> {
        validate_graceful_shutdown_request(&request)?;
        match self
            .exchange(RuntimeControlOperationV1::GracefulShutdown(request))
            .await?
        {
            RuntimeControlResultV1::GracefulShutdown(reply) => Ok(reply),
            _ => Err(RuntimeControlError::InvalidRequest),
        }
    }

    async fn exchange(
        &self,
        operation: RuntimeControlOperationV1,
    ) -> Result<RuntimeControlResultV1, RuntimeControlError> {
        let mut request = RuntimeControlRequestV1 {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            scope: self.scope.clone(),
            request_id: random_hex()?,
            nonce: random_hex()?,
            issued_at_ms: now_ms()?,
            operation,
            authentication_mac: String::new(),
        };
        sign_request(&mut request, &self.owner_secret)?;
        let reply = transport::client_exchange(&self.endpoint, &request).await?;
        verify_reply(&reply, &request, &self.owner_secret, self.max_clock_skew)?;
        match reply.result {
            RuntimeControlResultV1::Rejected { .. } => Err(RuntimeControlError::InvalidRequest),
            result => Ok(result),
        }
    }
}

/// Returns the platform-native identity digest used for scope construction.
pub fn current_os_user_identity_digest() -> Result<String, RuntimeControlError> {
    transport::current_os_user_identity_digest()
}

async fn handle_stream<S>(mut stream: S, state: Arc<ServerState>) -> Result<(), RuntimeControlError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let request: RuntimeControlRequestV1 = read_message(&mut stream).await?;
    verify_request(&request, &state)?;
    let context = RuntimeControlRequestContextV1 {
        request_id: request.request_id.clone(),
        nonce: request.nonce.clone(),
        issued_at_ms: request.issued_at_ms,
    };
    let result = match request.operation.clone() {
        RuntimeControlOperationV1::Status(_) => state
            .handler
            .status()
            .await
            .map(RuntimeControlResultV1::Status)
            .unwrap_or_else(|code| RuntimeControlResultV1::Rejected { code }),
        RuntimeControlOperationV1::Attach(attach) => state
            .handler
            .attach(attach)
            .await
            .map(RuntimeControlResultV1::Attached)
            .unwrap_or_else(|code| RuntimeControlResultV1::Rejected { code }),
        RuntimeControlOperationV1::GracefulShutdown(shutdown) => state
            .handler
            .graceful_shutdown(context, shutdown)
            .await
            .map(RuntimeControlResultV1::GracefulShutdown)
            .unwrap_or_else(|code| RuntimeControlResultV1::Rejected { code }),
    };
    let mut reply = RuntimeControlReplyV1 {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        request_id: request.request_id,
        nonce: request.nonce,
        issued_at_ms: now_ms()?,
        result,
        authentication_mac: String::new(),
    };
    sign_reply(&mut reply, &state.owner_secret)?;
    write_message(&mut stream, &reply).await?;
    tokio::io::AsyncWriteExt::shutdown(&mut stream)
        .await
        .map_err(|_| RuntimeControlError::Transport)
}

fn verify_request(
    request: &RuntimeControlRequestV1,
    state: &ServerState,
) -> Result<(), RuntimeControlError> {
    validate_request_shape(request)?;
    if request.scope != state.scope {
        return Err(RuntimeControlError::InvalidRequest);
    }
    verify_mac(
        &request_signing_bytes(request)?,
        &request.authentication_mac,
        &state.owner_secret,
        CLIENT_AUTH_DOMAIN,
    )?;
    validate_timestamp(request.issued_at_ms, state.max_clock_skew)?;
    state
        .replay_guard
        .check_and_record(&request.request_id, &request.nonce, request.issued_at_ms)
}

fn verify_reply(
    reply: &RuntimeControlReplyV1,
    request: &RuntimeControlRequestV1,
    owner_secret: &[u8; 32],
    max_clock_skew: Duration,
) -> Result<(), RuntimeControlError> {
    if reply.protocol_version != PROTOCOL_VERSION
        || reply.request_id != request.request_id
        || reply.nonce != request.nonce
        || !is_lower_hex(&reply.authentication_mac, 64)
    {
        return Err(RuntimeControlError::InvalidRequest);
    }
    verify_mac(
        &reply_signing_bytes(reply)?,
        &reply.authentication_mac,
        owner_secret,
        SERVER_AUTH_DOMAIN,
    )?;
    validate_timestamp(reply.issued_at_ms, max_clock_skew)
}

fn validate_request_shape(request: &RuntimeControlRequestV1) -> Result<(), RuntimeControlError> {
    request.scope.validate()?;
    if request.protocol_version != PROTOCOL_VERSION
        || !is_lower_hex(&request.request_id, 64)
        || !is_lower_hex(&request.nonce, 64)
        || !is_lower_hex(&request.authentication_mac, 64)
        || request.issued_at_ms <= 0
    {
        return Err(RuntimeControlError::InvalidRequest);
    }
    match &request.operation {
        RuntimeControlOperationV1::Status(_) => {}
        RuntimeControlOperationV1::Attach(attach) => {
            if !is_safe_text(&attach.client_instance_id, 256) {
                return Err(RuntimeControlError::InvalidRequest);
            }
        }
        RuntimeControlOperationV1::GracefulShutdown(shutdown) => {
            validate_graceful_shutdown_request(shutdown)?;
        }
    }
    Ok(())
}

fn validate_graceful_shutdown_request(
    request: &GracefulShutdownRequestV1,
) -> Result<(), RuntimeControlError> {
    if !is_safe_text(&request.client_instance_id, 256)
        || request.runtime_host_generation <= 0
        || !is_safe_text(&request.runtime_host_instance_id, 256)
    {
        return Err(RuntimeControlError::InvalidRequest);
    }
    if let Some(binding) = &request.confirmation {
        if !is_lower_hex(&binding.challenge, 64)
            || !is_lower_hex(&binding.original_request_id, 64)
            || !is_lower_hex(&binding.original_nonce, 64)
            || binding.original_issued_at_ms <= 0
        {
            return Err(RuntimeControlError::InvalidRequest);
        }
    }
    Ok(())
}

fn validate_timestamp(
    issued_at_ms: i64,
    max_clock_skew: Duration,
) -> Result<(), RuntimeControlError> {
    let now = now_ms()?;
    let bound =
        i64::try_from(max_clock_skew.as_millis()).map_err(|_| RuntimeControlError::Timestamp)?;
    if issued_at_ms <= 0 || issued_at_ms.abs_diff(now) > bound as u64 {
        return Err(RuntimeControlError::Timestamp);
    }
    Ok(())
}

fn sign_request(
    request: &mut RuntimeControlRequestV1,
    owner_secret: &[u8; 32],
) -> Result<(), RuntimeControlError> {
    request.authentication_mac = mac_hex(
        &request_signing_bytes(request)?,
        owner_secret,
        CLIENT_AUTH_DOMAIN,
    )?;
    Ok(())
}

fn sign_reply(
    reply: &mut RuntimeControlReplyV1,
    owner_secret: &[u8; 32],
) -> Result<(), RuntimeControlError> {
    reply.authentication_mac = mac_hex(
        &reply_signing_bytes(reply)?,
        owner_secret,
        SERVER_AUTH_DOMAIN,
    )?;
    Ok(())
}

fn request_signing_bytes(
    request: &RuntimeControlRequestV1,
) -> Result<Vec<u8>, RuntimeControlError> {
    let mut unsigned = request.clone();
    unsigned.authentication_mac.clear();
    canonical_json_bytes(&unsigned)
}

fn reply_signing_bytes(reply: &RuntimeControlReplyV1) -> Result<Vec<u8>, RuntimeControlError> {
    let mut unsigned = reply.clone();
    unsigned.authentication_mac.clear();
    canonical_json_bytes(&unsigned)
}

fn mac_hex(
    bytes: &[u8],
    owner_secret: &[u8; 32],
    domain: &[u8],
) -> Result<String, RuntimeControlError> {
    let mut mac = HmacSha256::new_from_slice(owner_secret)
        .map_err(|_| RuntimeControlError::Authentication)?;
    mac.update(domain);
    mac.update(&[0]);
    mac.update(bytes);
    Ok(hex_encode(&mac.finalize().into_bytes()))
}

fn verify_mac(
    bytes: &[u8],
    supplied: &str,
    owner_secret: &[u8; 32],
    domain: &[u8],
) -> Result<(), RuntimeControlError> {
    let supplied = hex_decode_32(supplied)?;
    let mut mac = HmacSha256::new_from_slice(owner_secret)
        .map_err(|_| RuntimeControlError::Authentication)?;
    mac.update(domain);
    mac.update(&[0]);
    mac.update(bytes);
    mac.verify_slice(&supplied)
        .map_err(|_| RuntimeControlError::Authentication)
}

async fn write_message<S, T>(stream: &mut S, message: &T) -> Result<(), RuntimeControlError>
where
    S: tokio::io::AsyncWrite + Unpin,
    T: Serialize,
{
    let frame = encode_frame(message)?;
    tokio::io::AsyncWriteExt::write_all(stream, &frame)
        .await
        .map_err(|_| RuntimeControlError::Transport)?;
    tokio::io::AsyncWriteExt::flush(stream)
        .await
        .map_err(|_| RuntimeControlError::Transport)
}

async fn read_message<S, T>(stream: &mut S) -> Result<T, RuntimeControlError>
where
    S: tokio::io::AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut prefix = [0u8; 4];
    tokio::io::AsyncReadExt::read_exact(stream, &mut prefix)
        .await
        .map_err(|_| RuntimeControlError::Transport)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(RuntimeControlError::Frame);
    }
    let mut payload = vec![0u8; length];
    tokio::io::AsyncReadExt::read_exact(stream, &mut payload)
        .await
        .map_err(|_| RuntimeControlError::Transport)?;
    serde_json::from_slice(&payload).map_err(|_| RuntimeControlError::Frame)
}

async fn client_exchange_stream<S>(
    stream: &mut S,
    request: &RuntimeControlRequestV1,
) -> Result<RuntimeControlReplyV1, RuntimeControlError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    write_message(stream, request).await?;
    read_message(stream).await
}

fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, RuntimeControlError> {
    let payload = canonical_json_bytes(value)?;
    if payload.len() > MAX_FRAME_BYTES || payload.len() > u32::MAX as usize {
        return Err(RuntimeControlError::Frame);
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, RuntimeControlError> {
    let value = serde_json::to_value(value).map_err(|_| RuntimeControlError::Frame)?;
    serde_json::to_vec(&canonicalize(value)).map_err(|_| RuntimeControlError::Frame)
}

fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonicalize).collect())
        }
        serde_json::Value::Object(values) => {
            let ordered = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::Value::Object(ordered.into_iter().collect())
        }
        other => other,
    }
}

fn endpoint_id(scope: &RuntimeControlScopeV1) -> String {
    let mut digest = Sha256::new();
    digest.update(ENDPOINT_DOMAIN);
    for part in [
        &scope.os_user_identity_digest,
        &scope.canonical_root_identity,
        &scope.profile_identity,
    ] {
        digest.update([0]);
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex_encode(&digest.finalize())
}

fn now_ms() -> Result<i64, RuntimeControlError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeControlError::Timestamp)?;
    i64::try_from(duration.as_millis()).map_err(|_| RuntimeControlError::Timestamp)
}

fn random_hex() -> Result<String, RuntimeControlError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| RuntimeControlError::Transport)?;
    Ok(hex_encode(&bytes))
}

fn is_safe_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_sha256_identity(value: &str) -> bool {
    value.len() == 71 && value.starts_with("sha256:") && is_lower_hex(&value[7..], 64)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_decode_32(value: &str) -> Result<[u8; 32], RuntimeControlError> {
    if !is_lower_hex(value, 64) {
        return Err(RuntimeControlError::Authentication);
    }
    let mut output = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (decode_nibble(pair[0])? << 4) | decode_nibble(pair[1])?;
    }
    Ok(output)
}

fn decode_nibble(value: u8) -> Result<u8, RuntimeControlError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(RuntimeControlError::Authentication),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scope() -> RuntimeControlScopeV1 {
        RuntimeControlScopeV1 {
            canonical_root_identity: format!("sha256:{}", "a".repeat(64)),
            profile_identity: "default".into(),
            os_user_identity_digest: current_os_user_identity_digest().unwrap(),
        }
    }

    fn request() -> RuntimeControlRequestV1 {
        RuntimeControlRequestV1 {
            protocol_version: PROTOCOL_VERSION.into(),
            scope: scope(),
            request_id: "1".repeat(64),
            nonce: "2".repeat(64),
            issued_at_ms: now_ms().unwrap(),
            operation: RuntimeControlOperationV1::Status(StatusRequestV1 {}),
            authentication_mac: String::new(),
        }
    }

    #[test]
    fn endpoint_is_stable_tuple_bound_and_redacted() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let first = RuntimeControlEndpoint::for_scope(root, &scope()).unwrap();
        let second = RuntimeControlEndpoint::for_scope(root, &scope()).unwrap();
        assert_eq!(first, second);
        assert!(!format!("{first:?}").contains("pipe"));
        let mut other = scope();
        other.profile_identity = "other".into();
        assert_ne!(
            first,
            RuntimeControlEndpoint::for_scope(root, &other).unwrap()
        );
    }

    #[test]
    fn protocol_rejects_unknown_fields_oversized_and_cross_domain_mac() {
        let secret = [7u8; 32];
        let mut request = request();
        sign_request(&mut request, &secret).unwrap();
        let bytes = request_signing_bytes(&request).unwrap();
        assert!(verify_mac(
            &bytes,
            &request.authentication_mac,
            &secret,
            SERVER_AUTH_DOMAIN
        )
        .is_err());
        let unknown = serde_json::json!({
            "protocol_version": PROTOCOL_VERSION,
            "scope": scope(),
            "request_id": "1".repeat(64),
            "nonce": "2".repeat(64),
            "issued_at_ms": now_ms().unwrap(),
            "operation": "status",
            "payload": {},
            "authentication_mac": "0".repeat(64),
            "unexpected": true
        });
        assert!(serde_json::from_value::<RuntimeControlRequestV1>(unknown).is_err());
        assert!(encode_frame(&"x".repeat(MAX_FRAME_BYTES + 1)).is_err());

        let shutdown_unknown = serde_json::json!({
            "client_instance_id": "tauri-test",
            "runtime_host_generation": 3,
            "runtime_host_instance_id": "host-3",
            "confirmation": null,
            "kill_process": true
        });
        assert!(serde_json::from_value::<GracefulShutdownRequestV1>(shutdown_unknown).is_err());
    }

    #[test]
    fn owner_control_key_derivation_is_domain_separated_and_closed() {
        let secret = b"native-owner-secret-at-least-thirty-two-bytes";
        let first = derive_owner_control_key(secret).unwrap();
        assert_eq!(first, derive_owner_control_key(secret).unwrap());
        assert_ne!(first, Sha256::digest(secret).as_slice());
        assert!(derive_owner_control_key(b"short").is_err());
    }

    #[test]
    fn replay_and_timestamp_rejections_fail_closed() {
        let secret = [7u8; 32];
        let guard = Arc::new(InMemoryReplayGuard::default());
        let handler = Arc::new(TestHandler::default());
        let state = ServerState {
            scope: scope(),
            owner_secret: Zeroizing::new(secret),
            max_clock_skew: Duration::from_secs(1),
            replay_guard: guard,
            handler,
        };
        let mut request = request();
        sign_request(&mut request, &secret).unwrap();
        assert!(verify_request(&request, &state).is_ok());
        assert_eq!(
            verify_request(&request, &state),
            Err(RuntimeControlError::Replay)
        );
        request.request_id = "3".repeat(64);
        request.nonce = "4".repeat(64);
        request.issued_at_ms = 1;
        sign_request(&mut request, &secret).unwrap();
        assert_eq!(
            verify_request(&request, &state),
            Err(RuntimeControlError::Timestamp)
        );
    }

    #[derive(Default)]
    struct TestHandler {
        attached: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl RuntimeControlHandler for TestHandler {
        async fn status(&self) -> Result<StatusReplyV1, RuntimeControlRejectionCodeV1> {
            Ok(StatusReplyV1 {
                runtime_host_generation: 3,
                runtime_host_instance_id: "host-3".into(),
                actual_state: RuntimeActualStateV1::RunningAppBound,
                desired_mode: RuntimeDesiredModeV1::AppBound,
                start_at_login: false,
                active_work: ActiveWorkStatusV1 {
                    active: true,
                    active_work_count: 1,
                    nonterminal_delegation_count: 1,
                    nonterminal_continuation_count: 0,
                    nonterminal_effect_count: 0,
                },
            })
        }

        async fn attach(
            &self,
            _request: AttachRequestV1,
        ) -> Result<AttachReplyV1, RuntimeControlRejectionCodeV1> {
            self.attached.fetch_add(1, Ordering::SeqCst);
            Ok(AttachReplyV1 {
                runtime_host_generation: 3,
                runtime_host_instance_id: "host-3".into(),
            })
        }

        async fn graceful_shutdown(
            &self,
            context: RuntimeControlRequestContextV1,
            request: GracefulShutdownRequestV1,
        ) -> Result<GracefulShutdownReplyV1, RuntimeControlRejectionCodeV1> {
            Ok(GracefulShutdownReplyV1 {
                disposition: GracefulShutdownDispositionV1::Authorized {
                    authorization: ShutdownAuthorizationV1 {
                        authorization_id: "a".repeat(64),
                        request_id: context.request_id,
                        nonce: context.nonce,
                        issued_at_ms: context.issued_at_ms,
                        client_instance_id: request.client_instance_id,
                        runtime_host_generation: request.runtime_host_generation,
                        runtime_host_instance_id: request.runtime_host_instance_id,
                        confirmed_active_work_binding_digest: Some(
                            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                                .into(),
                        ),
                        confirmed_active_work_count: 1,
                    },
                },
            })
        }
    }

    #[tokio::test]
    async fn graceful_shutdown_is_bound_to_authenticated_envelope_and_exact_host() {
        let secret = [7u8; 32];
        let handler = Arc::new(TestHandler::default());
        let state = Arc::new(ServerState {
            scope: scope(),
            owner_secret: Zeroizing::new(secret),
            max_clock_skew: Duration::from_secs(1),
            replay_guard: Arc::new(InMemoryReplayGuard::default()),
            handler,
        });
        let mut request = request();
        request.operation =
            RuntimeControlOperationV1::GracefulShutdown(GracefulShutdownRequestV1 {
                client_instance_id: "tauri-test".into(),
                runtime_host_generation: 3,
                runtime_host_instance_id: "host-3".into(),
                confirmation: None,
            });
        sign_request(&mut request, &secret).unwrap();
        let (mut client_stream, server_stream) = tokio::io::duplex(MAX_FRAME_BYTES * 2);
        let server = tokio::spawn(handle_stream(server_stream, state));
        let reply = client_exchange_stream(&mut client_stream, &request)
            .await
            .unwrap();
        verify_reply(&reply, &request, &secret, Duration::from_secs(1)).unwrap();
        let RuntimeControlResultV1::GracefulShutdown(reply) = reply.result else {
            panic!("expected graceful shutdown reply");
        };
        let GracefulShutdownDispositionV1::Authorized { authorization } = reply.disposition else {
            panic!("expected shutdown authorization");
        };
        assert_eq!(authorization.request_id, request.request_id);
        assert_eq!(authorization.nonce, request.nonce);
        assert_eq!(authorization.issued_at_ms, request.issued_at_ms);
        assert_eq!(authorization.client_instance_id, "tauri-test");
        assert_eq!(authorization.runtime_host_generation, 3);
        assert_eq!(authorization.runtime_host_instance_id, "host-3");
        server.await.unwrap().unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn real_windows_pipe_roundtrip_authenticates_status_and_attach() {
        let temp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let scope = scope();
        let secret = [9u8; 32];
        let handler = Arc::new(TestHandler::default());
        let server = RuntimeControlServer::new(
            temp.path(),
            scope.clone(),
            secret,
            Arc::new(InMemoryReplayGuard::default()),
            Arc::clone(&handler) as Arc<dyn RuntimeControlHandler>,
        )
        .unwrap();
        let task = tokio::spawn(server.serve());
        let client = RuntimeControlClient::new(temp.path(), scope, secret).unwrap();
        let status = client.status().await.unwrap();
        assert_eq!(status.runtime_host_generation, 3);
        let attached = client
            .attach(AttachRequestV1 {
                client_instance_id: "tauri-test".into(),
            })
            .await
            .unwrap();
        assert_eq!(attached.runtime_host_instance_id, "host-3");
        assert_eq!(handler.attached.load(Ordering::SeqCst), 1);
        task.abort();
    }
}
