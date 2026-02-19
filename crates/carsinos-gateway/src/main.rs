use anyhow::{Context, Result as AnyResult};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::extract::{Path, Query};
use axum::http::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use carsinos_channels_discord as discord_channel;
use carsinos_channels_telegram as telegram_channel;
use carsinos_core::{GatewayConfig, TokenSource};
use carsinos_protocol::{
    AnthropicSetupTokenIngestRequest, AnthropicSetupTokenIngestResponse, ApprovalResponse,
    AuthProfileResponse, CreateApprovalRequest, CreateApprovalResponse, CreateAuthProfileRequest,
    CreateAuthProfileResponse, CreateJobRequest, CreateJobResponse, CreateMessageRequest,
    CreateMessageResponse, CreateNoteRequest, CreateNoteResponse, CreateRunRequest,
    CreateRunResponse, CreateSessionRequest, CreateSessionResponse, DiscordChannelConfig,
    GetAgentProviderProfileOrderResponse, GetChannelConfigResponse, GetNoteResponse,
    HealthResponse, IngestChannelMessageResponse, IngestDiscordMessageRequest,
    IngestTelegramMessageRequest, JobResponse, JobRunResponse, JobStatusResponse,
    ListApprovalsQuery, ListApprovalsResponse, ListAuthProfilesQuery, ListAuthProfilesResponse,
    ListJobHistoryQuery, ListJobHistoryResponse, ListJobsQuery, ListJobsResponse,
    ListMessagesQuery, ListMessagesResponse, ListNotesQuery, ListNotesResponse,
    ListProviderCapabilitiesQuery, ListProviderCapabilitiesResponse, ListSessionsQuery,
    ListSessionsResponse, MessageResponse, MetricsResponse, NoteResponse, OpenAiOauthFinishRequest,
    OpenAiOauthFinishResponse, OpenAiOauthStartRequest, OpenAiOauthStartResponse,
    ProviderCapabilityResponse, RemoveJobResponse, ResolveApprovalRequest, ResolveApprovalResponse,
    ResolveChannelApprovalActionRequest, RunJobNowResponse, RunResponse, SearchMemoryRequest,
    SearchMemoryResponse, SearchMemoryResult, SessionDetailResponse, SessionSummary,
    SetAgentProviderProfileOrderRequest, SetAgentProviderProfileOrderResponse, StatusResponse,
    TelegramChannelConfig, UpdateAuthProfileStateRequest, UpdateAuthProfileStateResponse,
    UpdateChannelConfigRequest, UpdateChannelConfigResponse, UpdateJobRequest, UpdateJobResponse,
    UpdateNoteRequest, UpdateNoteResponse,
};
use carsinos_providers::{
    parse_provider_error_class as parse_provider_error_class_normalized,
    provider_error_retryable as provider_error_retryable_normalized, CompletionRequest,
    ProviderAuthProfile, ProviderRegistry,
};
use carsinos_storage::{
    AppPaths, ApprovalRecord, ApprovalResolveResult, AuthProfileRecord, JobRecord, JobRunRecord,
    JobUpdatePatch, MessageRecord, NewApproval, NewAuthProfile, NewJob, NewMessage, NewNote,
    NewRun, NewSecurityAuditEvent, NewSession, NoteRecord, RunRecord, SecurityAuditEventListFilter,
    SecurityAuditEventRecord, SessionRecord, Storage,
};
use carsinos_tools::{
    ExecRequest, FsReadRequest, FsWriteMode, FsWriteRequest, LocalToolRunner, ProcessRequest,
    ToolRequest, ToolRunner, WebFetchRequest, WebSearchRequest,
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::path::Path as FsPath;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tokio::time::sleep;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{debug, error, info, warn, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::writer::{BoxMakeWriter, MakeWriterExt};
use tracing_subscriber::EnvFilter;
use url::Url;

#[derive(Clone)]
struct AppState {
    auth_mode: AuthMode,
    auth_token: Arc<String>,
    jwt_auth: Option<Arc<JwtAuthConfig>>,
    jwt_replay_jti: Arc<StdRwLock<HashMap<String, i64>>>,
    rate_limiter: Arc<RequestRateLimiter>,
    trusted_proxy_headers: bool,
    trusted_proxy_allowlist: Arc<HashSet<String>>,
    operator_allowlist: Arc<Vec<String>>,
    providers: ProviderRegistry,
    tool_runner: LocalToolRunner,
    secret_store: SecretStore,
    oauth_sessions: Arc<RwLock<HashMap<String, PendingOpenAiOauthSession>>>,
    numquam_client: Option<NumquamClient>,
    storage: Storage,
    metrics: Arc<GatewayMetrics>,
    event_tx: broadcast::Sender<String>,
    event_seq: Arc<AtomicU64>,
    started_at: DateTime<Utc>,
    started_instant: Instant,
    db_path: Arc<String>,
    attachments_path: Arc<String>,
}

struct LogGuards {
    _file_guard: Option<WorkerGuard>,
}

#[derive(Debug, Default)]
struct GatewayMetrics {
    requests_total: AtomicU64,
    auth_failures_total: AtomicU64,
    runs_started_total: AtomicU64,
    runs_succeeded_total: AtomicU64,
    runs_failed_total: AtomicU64,
    notes_created_total: AtomicU64,
    notes_updated_total: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    StaticBearer,
    Jwt,
}

#[derive(Debug, Clone)]
struct JwtAuthConfig {
    issuer: String,
    audience: String,
    secret: String,
    max_token_age_seconds: i64,
    clock_skew_seconds: i64,
    replay_protection_enabled: bool,
    revoked_jti: HashSet<String>,
}

#[derive(Debug, Clone)]
struct RequestRateLimitConfig {
    enabled: bool,
    window_seconds: i64,
    per_ip_limit: usize,
    per_principal_limit: usize,
    per_run_endpoint_limit: usize,
    per_approval_endpoint_limit: usize,
}

impl RequestRateLimitConfig {
    fn from_env() -> Self {
        Self {
            enabled: bool_env("CARSINOS_RATE_LIMIT_ENABLED", true),
            window_seconds: i64_env("CARSINOS_RATE_LIMIT_WINDOW_SECONDS", 60).clamp(1, 3600),
            per_ip_limit: usize_env("CARSINOS_RATE_LIMIT_PER_IP", 2400).clamp(10, 200_000),
            per_principal_limit: usize_env("CARSINOS_RATE_LIMIT_PER_PRINCIPAL", 1200)
                .clamp(10, 200_000),
            per_run_endpoint_limit: usize_env("CARSINOS_RATE_LIMIT_RUN_ENDPOINT", 300)
                .clamp(5, 50_000),
            per_approval_endpoint_limit: usize_env("CARSINOS_RATE_LIMIT_APPROVAL_ENDPOINT", 180)
                .clamp(5, 50_000),
        }
    }
}

#[derive(Debug)]
struct RequestRateLimiter {
    config: RequestRateLimitConfig,
    counters: StdRwLock<HashMap<String, VecDeque<i64>>>,
}

impl RequestRateLimiter {
    fn from_env() -> Self {
        Self {
            config: RequestRateLimitConfig::from_env(),
            counters: StdRwLock::new(HashMap::new()),
        }
    }

    fn check(&self, key: &str, limit: usize) -> std::result::Result<(), AuthError> {
        if !self.config.enabled {
            return Ok(());
        }
        let now = current_time_ms();
        let window_ms = self.config.window_seconds.saturating_mul(1000);
        let mut guard = self.counters.write().map_err(|_| AuthError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL_ERROR",
            message: "rate limiter lock poisoned".to_string(),
            retry_after_seconds: None,
        })?;
        let bucket = guard.entry(key.to_string()).or_insert_with(VecDeque::new);
        while let Some(front) = bucket.front() {
            if now.saturating_sub(*front) > window_ms {
                bucket.pop_front();
            } else {
                break;
            }
        }
        if bucket.len() >= limit {
            let retry_after_seconds = bucket
                .front()
                .map(|oldest| {
                    let window_end = oldest.saturating_add(window_ms);
                    let remaining_ms = window_end.saturating_sub(now);
                    // Round up so callers do not retry earlier than the bucket release.
                    (remaining_ms.saturating_add(999) / 1000).max(1)
                })
                .unwrap_or(self.config.window_seconds.max(1));
            return Err(AuthError {
                status: StatusCode::TOO_MANY_REQUESTS,
                code: "RATE_LIMITED",
                message: "request rate limit exceeded".to_string(),
                retry_after_seconds: Some(retry_after_seconds),
            });
        }
        bucket.push_back(now);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct AuthContext {
    principal_id: String,
    roles: HashSet<String>,
    auth_method: &'static str,
    token_id: Option<String>,
    session_id: Option<String>,
    client_ip: String,
}

#[derive(Debug, Clone)]
struct AuthError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retry_after_seconds: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct JwtClaims {
    iss: String,
    #[serde(default)]
    aud: serde_json::Value,
    sub: String,
    exp: i64,
    iat: i64,
    jti: String,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct AuthModePolicy {
    risk_level: &'static str,
    risk_notes: &'static str,
    requires_warning: bool,
    requires_kill_switch: bool,
}

const AUTH_MODE_API_KEY: &str = "api_key";
const AUTH_MODE_OPENAI_OAUTH: &str = "openai_oauth";
const AUTH_MODE_CLAUDE_CONSUMER_OAUTH: &str = "claude_consumer_oauth";
const AUTH_MODE_AGENT_SDK: &str = "agent_sdk";

const KILL_SWITCH_SCOPE_NONE: &str = "none";
const KILL_SWITCH_SCOPE_PROFILE: &str = "profile";
const KILL_SWITCH_SCOPE_PROVIDER: &str = "provider";
const KILL_SWITCH_SCOPE_GLOBAL: &str = "global";

const APP_KV_CHANNELS_DISCORD: &str = "config.channels.discord";
const APP_KV_CHANNELS_TELEGRAM: &str = "config.channels.telegram";
const NUMQUAM_SCHEMA_VERSION: &str = "integration.v1";
const NUMQUAM_APPROVAL_KIND_WRITEBACK: &str = "memory.writeback";
const AUTH_PROVIDER_OPENAI: &str = "openai";
const AUTH_PROVIDER_ANTHROPIC: &str = "anthropic";
const OAUTH_OPENAI_DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:1455/auth/callback";
const OAUTH_OPENAI_DEFAULT_SCOPE: &str = "offline_access";
const OAUTH_OPENAI_DEFAULT_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const OAUTH_OPENAI_DEFAULT_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_DEFAULT_API_BASE: &str = "https://api.openai.com";
const ANTHROPIC_DEFAULT_API_BASE: &str = "https://api.anthropic.com";
const SECRET_FIELD_NAMES: &[&str] = &[
    "api_key",
    "token",
    "bearer_token",
    "access_token",
    "refresh_token",
    "setup_token",
    "client_secret",
];
const LOCAL_MEMORY_EMBED_MODEL: &str = "carsinos.local-embed-v1";
const LOCAL_MEMORY_DEFAULT_TOP_K: usize = 4;
const LOCAL_MEMORY_DEFAULT_MAX_CANDIDATES: usize = 128;
const LOCAL_MEMORY_DEFAULT_MAX_CHARS: usize = 1200;
const LOCAL_MEMORY_CHUNK_TARGET_CHARS: usize = 420;

const ROLE_OPERATOR_ADMIN: &str = "operator_admin";
const ROLE_OPERATOR_READONLY: &str = "operator_readonly";
const ROLE_AUTOMATION_RUNNER: &str = "automation_runner";
const ROLE_CHANNEL_ADAPTER: &str = "channel_adapter";
const ROLE_SERVICE_INTERNAL: &str = "service_internal";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumquamTransport {
    Http,
    Mcp,
    Dual,
}

impl NumquamTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Mcp => "mcp",
            Self::Dual => "dual",
        }
    }
}

#[derive(Debug, Clone)]
struct NumquamClient {
    transport: NumquamTransport,
    integration_base_url: String,
    mcp_url: String,
    token: Option<String>,
    principal_id: String,
    principal_display_name: String,
    request_timeout: Duration,
    http_client: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
struct NumquamEnvelope<T> {
    schema_version: String,
    request_id: String,
    request_id_source: String,
    operation: String,
    ok: bool,
    degrade_mode: bool,
    #[serde(default)]
    warnings: Vec<NumquamWarning>,
    data: Option<T>,
    #[serde(default)]
    error: Option<NumquamEnvelopeError>,
    #[serde(default)]
    fallback_recommendation: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NumquamWarning {
    warning_code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    started_at_utc: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NumquamEnvelopeError {
    code: String,
    message: String,
    #[serde(default)]
    retryable: Option<bool>,
    #[serde(default)]
    operator_action: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct NumquamContextData {
    #[serde(default)]
    context_text: String,
    #[serde(default)]
    evidence: Vec<NumquamContextEvidence>,
    #[serde(default)]
    route: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    timings: Option<serde_json::Value>,
    #[serde(default)]
    truncation: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct NumquamContextEvidence {
    #[serde(default)]
    evidence_id: String,
    #[serde(default)]
    section: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    citations: Vec<String>,
    #[serde(default)]
    confidence: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct NumquamWritebackProposeData {
    proposal_id: String,
    status: String,
    #[serde(default)]
    idempotent_replay: bool,
    #[serde(default)]
    audit_ref: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct NumquamWritebackResolveData {
    proposal_id: String,
    status: String,
    #[serde(default)]
    already_resolved: bool,
    #[serde(default)]
    resolved_at_utc: Option<String>,
    #[serde(default)]
    audit_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NumquamMcpRpcResponse {
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<NumquamMcpRpcError>,
}

#[derive(Debug, Clone, Deserialize)]
struct NumquamMcpRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct RunMemoryMetadata {
    enabled: bool,
    transport: Option<String>,
    context_request_id: Option<String>,
    context_request_id_source: Option<String>,
    context_degrade_mode: bool,
    context_fallback_recommendation: Option<String>,
    context_warning_codes: Vec<String>,
    context_error_code: Option<String>,
    context_error_message: Option<String>,
    context_chars: usize,
    route: Option<String>,
    confidence: Option<f64>,
    evidence: Vec<RunMemoryEvidence>,
    writeback: Option<RunMemoryWritebackMetadata>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct RunMemoryEvidence {
    evidence_id: String,
    provenance_handle: String,
    citation_refs: Vec<String>,
    confidence: f64,
    conflict_flag: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
struct RunMemoryWritebackMetadata {
    request_id: Option<String>,
    request_id_source: Option<String>,
    proposal_id: Option<String>,
    status: Option<String>,
    idempotent_replay: bool,
    audit_ref: Option<String>,
    degrade_mode: bool,
    fallback_recommendation: Option<String>,
    warning_codes: Vec<String>,
    error_code: Option<String>,
    error_message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct LocalMemoryMetadata {
    enabled: bool,
    query_chars: usize,
    top_k: usize,
    max_candidates: usize,
    max_chars: usize,
    injected_chars: usize,
    hit_count: usize,
    hits: Vec<LocalMemoryHit>,
    error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct LocalMemoryHit {
    note_id: String,
    title: Option<String>,
    score: f64,
    snippet_chars: usize,
    chunk_index: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct ListSecurityAuditQuery {
    limit: Option<u32>,
    action: Option<String>,
    principal: Option<String>,
    decision: Option<String>,
    status: Option<String>,
    error_code: Option<String>,
    created_after: Option<i64>,
    created_before: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct SecurityAuditEventResponse {
    event_id: String,
    request_id: String,
    correlation_id: String,
    principal: String,
    action: String,
    resource: String,
    decision: String,
    reason: Option<String>,
    transport: String,
    status: String,
    error_code: Option<String>,
    session_id: Option<String>,
    run_id: Option<String>,
    metadata_json: Option<String>,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
struct ListSecurityAuditResponse {
    items: Vec<SecurityAuditEventResponse>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunSecurityAuditRetentionRequest {
    #[serde(default)]
    hot_retention_days: Option<i64>,
    #[serde(default)]
    dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct RunSecurityAuditRetentionResponse {
    hot_retention_days: i64,
    cutoff_ms: i64,
    candidate_count: i64,
    archived_count: i64,
    deleted_count: i64,
    dry_run: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RotateAuthProfileSecretRequest {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RotateAuthProfileSecretResponse {
    profile: AuthProfileResponse,
    previous_secret_ref: String,
    rotated_secret_ref: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RevokeAuthProfileRequest {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    remove_secret: Option<bool>,
    #[serde(default)]
    kill_switch_scope: Option<String>,
    #[serde(default)]
    disable_profile: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct RevokeAuthProfileResponse {
    profile: AuthProfileResponse,
    revoked_secret_ref: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct RotateAuthProfileSecretOutcome {
    profile: AuthProfileRecord,
    previous_secret_ref: String,
    rotated_secret_ref: String,
}

#[derive(Debug, Clone)]
struct RevokeAuthProfileSecretOutcome {
    profile: AuthProfileRecord,
    revoked_secret_ref: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingOpenAiOauthSession {
    oauth_session_id: String,
    state: String,
    code_verifier: String,
    redirect_uri: String,
    client_id: String,
    scope: String,
    token_url: String,
    provider_api_base_url: String,
    display_name_hint: Option<String>,
    expires_at_ms: i64,
}

#[derive(Debug, Clone)]
enum SecretStoreBackend {
    Keychain,
    Memory(Arc<StdRwLock<HashMap<String, String>>>),
}

#[derive(Debug, Clone)]
struct SecretStore {
    service_name: String,
    backend: SecretStoreBackend,
}

impl SecretStore {
    fn from_env() -> Self {
        let service_name = std::env::var("CARSINOS_SECRET_SERVICE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "carsinos".to_string());
        let mode = std::env::var("CARSINOS_SECRET_STORE")
            .unwrap_or_else(|_| {
                if cfg!(test) {
                    "memory".to_string()
                } else {
                    "keychain".to_string()
                }
            })
            .trim()
            .to_ascii_lowercase();
        if mode == "memory" {
            return Self {
                service_name,
                backend: SecretStoreBackend::Memory(Arc::new(StdRwLock::new(HashMap::new()))),
            };
        }
        Self {
            service_name,
            backend: SecretStoreBackend::Keychain,
        }
    }

    fn mode_name(&self) -> &'static str {
        match self.backend {
            SecretStoreBackend::Keychain => "keychain",
            SecretStoreBackend::Memory(_) => "memory",
        }
    }

    fn set_json(&self, secret_ref: &str, payload: &serde_json::Value) -> AnyResult<()> {
        let secret =
            serde_json::to_string(payload).context("failed to serialize secret payload")?;
        self.set_raw(secret_ref, &secret)
    }

    fn get_json(&self, secret_ref: &str) -> AnyResult<Option<serde_json::Value>> {
        let Some(secret) = self.get_raw(secret_ref)? else {
            return Ok(None);
        };
        let value: serde_json::Value = serde_json::from_str(&secret)
            .with_context(|| format!("failed to parse secret payload for ref {secret_ref}"))?;
        Ok(Some(value))
    }

    fn delete(&self, secret_ref: &str) -> AnyResult<()> {
        match &self.backend {
            SecretStoreBackend::Keychain => {
                let entry = keyring::Entry::new(&self.service_name, secret_ref)
                    .context("failed to initialize keychain entry")?;
                entry.set_password("").with_context(|| {
                    format!("failed clearing keychain secret for ref {secret_ref}")
                })?;
                Ok(())
            }
            SecretStoreBackend::Memory(store) => {
                let mut guard = store
                    .write()
                    .map_err(|_| anyhow::anyhow!("memory secret store poisoned"))?;
                guard.remove(secret_ref);
                Ok(())
            }
        }
    }

    fn set_raw(&self, secret_ref: &str, secret: &str) -> AnyResult<()> {
        match &self.backend {
            SecretStoreBackend::Keychain => {
                let entry = keyring::Entry::new(&self.service_name, secret_ref)
                    .context("failed to initialize keychain entry")?;
                entry.set_password(secret).with_context(|| {
                    format!("failed storing keychain secret for ref {secret_ref}")
                })?;
                Ok(())
            }
            SecretStoreBackend::Memory(store) => {
                let mut guard = store
                    .write()
                    .map_err(|_| anyhow::anyhow!("memory secret store poisoned"))?;
                guard.insert(secret_ref.to_string(), secret.to_string());
                Ok(())
            }
        }
    }

    fn get_raw(&self, secret_ref: &str) -> AnyResult<Option<String>> {
        match &self.backend {
            SecretStoreBackend::Keychain => {
                let entry = keyring::Entry::new(&self.service_name, secret_ref)
                    .context("failed to initialize keychain entry")?;
                match entry.get_password() {
                    Ok(secret) => {
                        if secret.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(secret))
                        }
                    }
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(err) => Err(anyhow::anyhow!(
                        "failed reading keychain secret for ref {secret_ref}: {err}"
                    )),
                }
            }
            SecretStoreBackend::Memory(store) => {
                let guard = store
                    .read()
                    .map_err(|_| anyhow::anyhow!("memory secret store poisoned"))?;
                Ok(guard.get(secret_ref).cloned())
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiTokenExchangeResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    let config = GatewayConfig::load_from_env()?;
    let auth_mode = load_auth_mode_from_env()?;
    let jwt_auth = load_jwt_auth_from_env(auth_mode)?;
    let trusted_proxy_headers = bool_env("CARSINOS_TRUST_PROXY_HEADERS", false);
    let trusted_proxy_allowlist = load_trusted_proxy_allowlist_from_env();
    let rate_limiter = Arc::new(RequestRateLimiter::from_env());
    enforce_network_exposure_policy(&config, trusted_proxy_headers, &trusted_proxy_allowlist)?;
    let paths = AppPaths::from_root(config.state_dir.clone());
    let _log_guards = init_tracing(&paths.logs_dir)?;

    carsinos_storage::init(&paths)?;
    let storage = Storage::from_paths(&paths);
    let providers = ProviderRegistry::new();
    let tool_runner = LocalToolRunner::default();
    let secret_store = SecretStore::from_env();
    let oauth_sessions = Arc::new(RwLock::new(HashMap::new()));
    let numquam_client = NumquamClient::from_env()?;
    let (event_tx, _) = broadcast::channel(256);
    let operator_allowlist = load_operator_allowlist_from_env();
    let operator_allowlist_entries = operator_allowlist.len();
    if let Some(client) = numquam_client.as_ref() {
        info!(
            transport = client.transport.as_str(),
            integration_base_url = %client.integration_base_url,
            mcp_url = %client.mcp_url,
            timeout_ms = client.request_timeout.as_millis() as u64,
            token_configured = client.token.is_some(),
            "Numquam integration client enabled"
        );
    }

    let state = AppState {
        auth_mode,
        auth_token: Arc::new(config.token.clone()),
        jwt_auth: jwt_auth.map(Arc::new),
        jwt_replay_jti: Arc::new(StdRwLock::new(HashMap::new())),
        rate_limiter,
        trusted_proxy_headers,
        trusted_proxy_allowlist: Arc::new(trusted_proxy_allowlist),
        operator_allowlist: Arc::new(operator_allowlist),
        providers,
        tool_runner,
        secret_store: secret_store.clone(),
        oauth_sessions,
        numquam_client,
        storage,
        metrics: Arc::new(GatewayMetrics::default()),
        event_tx,
        event_seq: Arc::new(AtomicU64::new(1)),
        started_at: Utc::now(),
        started_instant: Instant::now(),
        db_path: Arc::new(paths.db_path.display().to_string()),
        attachments_path: Arc::new(paths.attachments_dir.display().to_string()),
    };

    let scheduler_state = state.clone();
    tokio::spawn(async move {
        scheduler_loop(scheduler_state).await;
    });

    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(config.bind).await?;

    info!(
        bind = %config.bind,
        state_dir = %config.state_dir.display(),
        auth_mode = %match auth_mode {
            AuthMode::StaticBearer => "static_bearer",
            AuthMode::Jwt => "jwt",
        },
        trusted_proxy_headers,
        secret_store = secret_store.mode_name(),
        operator_allowlist_entries,
        "carsinos gateway starting"
    );

    if auth_mode == AuthMode::StaticBearer && config.token_source == TokenSource::Generated {
        warn!(token = %config.token, "CARSINOS_GATEWAY_TOKEN not set; generated runtime token");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn root() -> &'static str {
    "carsinOS gateway is running"
}

async fn health(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let auth_ctx = require_bearer_auth(&headers, &state).map_err(|err| err.status)?;
    require_roles_raw(&auth_ctx, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])
        .map_err(|err| err.status)?;
    let db_ok = state.storage.ping().is_ok();
    if !db_ok {
        warn!("health check degraded: sqlite ping failed");
    }

    let response = HealthResponse {
        ok: db_ok,
        service: "carsinos-gateway".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_ms: state.started_instant.elapsed().as_millis() as u64,
        now_utc: Utc::now(),
    };

    if db_ok {
        Ok((StatusCode::OK, Json(response)))
    } else {
        Ok((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}

async fn status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let auth_ctx = require_bearer_auth(&headers, &state).map_err(|err| err.status)?;
    require_roles_raw(&auth_ctx, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])
        .map_err(|err| err.status)?;

    let response = StatusResponse {
        service: "carsinos-gateway".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at_utc: state.started_at,
        uptime_ms: state.started_instant.elapsed().as_millis() as u64,
        db_path: (*state.db_path).clone(),
        attachments_path: (*state.attachments_path).clone(),
    };

    Ok(Json(response))
}

async fn metrics(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let auth_ctx = require_bearer_auth(&headers, &state).map_err(|err| err.status)?;
    require_roles_raw(&auth_ctx, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])
        .map_err(|err| err.status)?;
    let response = MetricsResponse {
        service: "carsinos-gateway".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_ms: state.started_instant.elapsed().as_millis() as u64,
        requests_total: state.metrics.requests_total.load(Ordering::Relaxed),
        auth_failures_total: state.metrics.auth_failures_total.load(Ordering::Relaxed),
        runs_started_total: state.metrics.runs_started_total.load(Ordering::Relaxed),
        runs_succeeded_total: state.metrics.runs_succeeded_total.load(Ordering::Relaxed),
        runs_failed_total: state.metrics.runs_failed_total.load(Ordering::Relaxed),
        notes_created_total: state.metrics.notes_created_total.load(Ordering::Relaxed),
        notes_updated_total: state.metrics.notes_updated_total.load(Ordering::Relaxed),
    };
    Ok(Json(response))
}

async fn list_provider_capabilities(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListProviderCapabilitiesQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    let provider_filter = query
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let resource = provider_filter
        .as_ref()
        .map(|provider| format!("provider:{provider}"))
        .unwrap_or_else(|| "providers".to_string());
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY],
        "provider.capabilities.list",
        &resource,
    )?;

    let capabilities = if let Some(provider) = provider_filter.as_deref() {
        vec![state.providers.capabilities(provider).ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "unsupported provider for capabilities query",
            )
        })?]
    } else {
        state.providers.list_capabilities()
    };

    let items = capabilities
        .into_iter()
        .map(|item| ProviderCapabilityResponse {
            provider: item.provider,
            supports_streaming: item.supports_streaming,
            supports_tools: item.supports_tools,
            supports_json_mode: item.supports_json_mode,
            supports_vision: item.supports_vision,
            max_context_tokens: item.max_context_tokens,
            error_classes: item.error_classes,
            retryable_error_classes: item.retryable_error_classes,
        })
        .collect::<Vec<_>>();
    Ok(Json(ListProviderCapabilitiesResponse {
        contract_version: "v2".to_string(),
        items,
    }))
}

async fn ws_handler(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let auth_ctx = require_bearer_auth(&headers, &state).map_err(|err| err.status)?;
    require_roles_raw(
        &auth_ctx,
        &[
            ROLE_OPERATOR_ADMIN,
            ROLE_OPERATOR_READONLY,
            ROLE_AUTOMATION_RUNNER,
        ],
    )
    .map_err(|err| err.status)?;
    info!("websocket client connected");
    let rx = state.event_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, rx, state.started_at)))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut event_rx: broadcast::Receiver<String>,
    started_at: DateTime<Utc>,
) {
    let event = serde_json::json!({
        "v": 1,
        "type": "event",
        "event": "gateway.status",
        "seq": 1,
        "data": {
            "service": "carsinos-gateway",
            "started_at_utc": started_at,
            "now_utc": Utc::now(),
            "status": "ok"
        }
    });

    if socket
        .send(Message::Text(event.to_string().into()))
        .await
        .is_err()
    {
        debug!("websocket closed before initial status frame");
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) | Some(Ok(Message::Ping(_))) => {
                        // Keep the connection alive; we don't consume client commands on WS in v1.
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Err(_)) | None => break,
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(frame) => {
                        if socket.send(Message::Text(frame.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    info!("websocket client disconnected");
}

fn require_bearer_auth(
    headers: &HeaderMap,
    state: &AppState,
) -> std::result::Result<AuthContext, AuthError> {
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let client_ip = resolve_client_ip(headers, state).inspect_err(|_| {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
    })?;
    let token = parse_bearer_token(headers).ok_or_else(|| {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        warn!("auth rejected: missing bearer token");
        AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_REQUIRED",
            message: "missing bearer token".to_string(),
            retry_after_seconds: None,
        }
    })?;

    let auth_context = match state.auth_mode {
        AuthMode::StaticBearer => {
            if token != state.auth_token.as_str() {
                state
                    .metrics
                    .auth_failures_total
                    .fetch_add(1, Ordering::Relaxed);
                warn!("auth rejected: static bearer token mismatch");
                return Err(AuthError {
                    status: StatusCode::UNAUTHORIZED,
                    code: "AUTH_INVALID",
                    message: "invalid bearer token".to_string(),
                    retry_after_seconds: None,
                });
            }
            let mut roles = HashSet::new();
            roles.insert(ROLE_OPERATOR_ADMIN.to_string());
            roles.insert(ROLE_OPERATOR_READONLY.to_string());
            roles.insert(ROLE_AUTOMATION_RUNNER.to_string());
            roles.insert(ROLE_CHANNEL_ADAPTER.to_string());
            roles.insert(ROLE_SERVICE_INTERNAL.to_string());
            AuthContext {
                principal_id: "static_bearer".to_string(),
                roles,
                auth_method: "static_bearer",
                token_id: None,
                session_id: None,
                client_ip,
            }
        }
        AuthMode::Jwt => authenticate_jwt(token, state, client_ip)?,
    };

    if state.rate_limiter.config.enabled {
        state
            .rate_limiter
            .check(
                &format!("ip:{}", auth_context.client_ip),
                state.rate_limiter.config.per_ip_limit,
            )
            .inspect_err(|_| {
                state
                    .metrics
                    .auth_failures_total
                    .fetch_add(1, Ordering::Relaxed);
            })?;
        state
            .rate_limiter
            .check(
                &format!("principal:{}", auth_context.principal_id),
                state.rate_limiter.config.per_principal_limit,
            )
            .inspect_err(|_| {
                state
                    .metrics
                    .auth_failures_total
                    .fetch_add(1, Ordering::Relaxed);
            })?;
    }

    Ok(auth_context)
}

fn authenticate_jwt(
    token: &str,
    state: &AppState,
    client_ip: String,
) -> std::result::Result<AuthContext, AuthError> {
    let jwt = state.jwt_auth.as_ref().ok_or_else(|| AuthError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "INTERNAL_ERROR",
        message: "jwt mode configured without jwt settings".to_string(),
        retry_after_seconds: None,
    })?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;
    validation.validate_aud = false;
    validation.required_spec_claims = HashSet::new();
    let token_data = jsonwebtoken::decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(jwt.secret.as_bytes()),
        &validation,
    )
    .map_err(|err| {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        let code = match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => "AUTH_EXPIRED",
            _ => "AUTH_INVALID",
        };
        AuthError {
            status: StatusCode::UNAUTHORIZED,
            code,
            message: "invalid jwt token".to_string(),
            retry_after_seconds: None,
        }
    })?;

    let claims = token_data.claims;
    let now = Utc::now().timestamp();
    let skew = jwt.clock_skew_seconds.max(0);
    if claims.iss.trim() != jwt.issuer {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_INVALID",
            message: "jwt issuer mismatch".to_string(),
            retry_after_seconds: None,
        });
    }
    if !audience_matches(&claims.aud, &jwt.audience) {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_INVALID",
            message: "jwt audience mismatch".to_string(),
            retry_after_seconds: None,
        });
    }
    if now > claims.exp.saturating_add(skew) {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_EXPIRED",
            message: "jwt token expired".to_string(),
            retry_after_seconds: None,
        });
    }
    if claims.iat > now.saturating_add(skew) {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_INVALID",
            message: "jwt issued-at claim is in the future".to_string(),
            retry_after_seconds: None,
        });
    }
    if now.saturating_sub(claims.iat).saturating_sub(skew) > jwt.max_token_age_seconds {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_EXPIRED",
            message: "jwt token age exceeds maximum".to_string(),
            retry_after_seconds: None,
        });
    }
    if claims.sub.trim().is_empty() {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_INVALID",
            message: "jwt subject claim missing".to_string(),
            retry_after_seconds: None,
        });
    }
    if claims.jti.trim().is_empty() {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_INVALID",
            message: "jwt jti claim missing".to_string(),
            retry_after_seconds: None,
        });
    }
    if jwt.revoked_jti.contains(claims.jti.trim()) {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
        return Err(AuthError {
            status: StatusCode::FORBIDDEN,
            code: "AUTH_FORBIDDEN",
            message: "jwt token id is revoked".to_string(),
            retry_after_seconds: None,
        });
    }
    if jwt.replay_protection_enabled {
        enforce_jwt_replay_protection(
            &state.jwt_replay_jti,
            claims.jti.trim(),
            claims.exp.saturating_add(skew),
            now,
        )
        .inspect_err(|_| {
            state
                .metrics
                .auth_failures_total
                .fetch_add(1, Ordering::Relaxed);
        })?;
    }

    let roles = normalize_roles(&claims.roles).inspect_err(|_| {
        state
            .metrics
            .auth_failures_total
            .fetch_add(1, Ordering::Relaxed);
    })?;
    let _ = claims.scope.as_deref();

    Ok(AuthContext {
        principal_id: claims.sub.trim().to_string(),
        roles,
        auth_method: "jwt",
        token_id: Some(claims.jti.trim().to_string()),
        session_id: claims
            .session_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        client_ip,
    })
}

fn enforce_jwt_replay_protection(
    seen_jti: &StdRwLock<HashMap<String, i64>>,
    token_id: &str,
    expires_at: i64,
    now: i64,
) -> std::result::Result<(), AuthError> {
    let mut guard = seen_jti.write().map_err(|_| AuthError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "INTERNAL_ERROR",
        message: "jwt replay lock poisoned".to_string(),
        retry_after_seconds: None,
    })?;
    guard.retain(|_, expiry| *expiry > now);
    if guard.get(token_id).is_some_and(|expiry| *expiry > now) {
        return Err(AuthError {
            status: StatusCode::FORBIDDEN,
            code: "AUTH_FORBIDDEN",
            message: "jwt token id replay detected".to_string(),
            retry_after_seconds: None,
        });
    }
    guard.insert(token_id.to_string(), expires_at.max(now.saturating_add(1)));
    Ok(())
}

fn audience_matches(aud_claim: &serde_json::Value, expected: &str) -> bool {
    match aud_claim {
        serde_json::Value::String(value) => value.trim() == expected,
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| value.as_str())
            .any(|value| value.trim() == expected),
        _ => false,
    }
}

fn normalize_roles(raw_roles: &[String]) -> std::result::Result<HashSet<String>, AuthError> {
    if raw_roles.is_empty() {
        return Err(AuthError {
            status: StatusCode::FORBIDDEN,
            code: "AUTH_ROLE_MISMATCH",
            message: "jwt roles claim is required".to_string(),
            retry_after_seconds: None,
        });
    }
    let mut normalized = HashSet::new();
    for role in raw_roles {
        let value = role.trim().to_string();
        if value.is_empty() {
            continue;
        }
        if !matches!(
            value.as_str(),
            ROLE_OPERATOR_ADMIN
                | ROLE_OPERATOR_READONLY
                | ROLE_AUTOMATION_RUNNER
                | ROLE_CHANNEL_ADAPTER
                | ROLE_SERVICE_INTERNAL
        ) {
            return Err(AuthError {
                status: StatusCode::FORBIDDEN,
                code: "AUTH_ROLE_MISMATCH",
                message: format!("unsupported role claim: {value}"),
                retry_after_seconds: None,
            });
        }
        normalized.insert(value);
    }
    if normalized.is_empty() {
        return Err(AuthError {
            status: StatusCode::FORBIDDEN,
            code: "AUTH_ROLE_MISMATCH",
            message: "jwt roles claim is required".to_string(),
            retry_after_seconds: None,
        });
    }
    Ok(normalized)
}

fn resolve_client_ip(
    headers: &HeaderMap,
    state: &AppState,
) -> std::result::Result<String, AuthError> {
    let fwd = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if state.trusted_proxy_headers {
        let proxy_id = headers
            .get("x-trusted-proxy-id")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AuthError {
                status: StatusCode::FORBIDDEN,
                code: "POLICY_DENY",
                message: "trusted proxy id header is required when proxy headers are enabled"
                    .to_string(),
                retry_after_seconds: None,
            })?;
        if !state.trusted_proxy_allowlist.contains(&proxy_id) {
            return Err(AuthError {
                status: StatusCode::FORBIDDEN,
                code: "POLICY_DENY",
                message: "untrusted proxy id".to_string(),
                retry_after_seconds: None,
            });
        }
        let forwarded_for = fwd.ok_or_else(|| AuthError {
            status: StatusCode::FORBIDDEN,
            code: "POLICY_DENY",
            message: "x-forwarded-for is required when proxy headers are enabled".to_string(),
            retry_after_seconds: None,
        })?;
        let first = forwarded_for.split(',').next().unwrap_or_default().trim();
        let ip = first.parse::<IpAddr>().map_err(|_| AuthError {
            status: StatusCode::FORBIDDEN,
            code: "POLICY_DENY",
            message: "invalid x-forwarded-for client ip".to_string(),
            retry_after_seconds: None,
        })?;
        return Ok(ip.to_string());
    }

    if fwd.is_some() {
        return Err(AuthError {
            status: StatusCode::FORBIDDEN,
            code: "POLICY_DENY",
            message: "x-forwarded-for is not accepted when trusted proxy headers are disabled"
                .to_string(),
            retry_after_seconds: None,
        });
    }

    Ok("local".to_string())
}

fn require_roles_raw(
    auth: &AuthContext,
    allowed_roles: &[&str],
) -> std::result::Result<(), AuthError> {
    if allowed_roles.is_empty() {
        return Ok(());
    }
    if allowed_roles.iter().any(|role| auth.roles.contains(*role)) {
        return Ok(());
    }
    Err(AuthError {
        status: StatusCode::FORBIDDEN,
        code: "AUTH_ROLE_MISMATCH",
        message: "insufficient role for endpoint".to_string(),
        retry_after_seconds: None,
    })
}

fn require_endpoint_rate_limit_with_error(
    state: &AppState,
    auth: &AuthContext,
    endpoint_kind: &str,
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    if !state.rate_limiter.config.enabled {
        return Ok(());
    }
    let limit = match endpoint_kind {
        "run" => state.rate_limiter.config.per_run_endpoint_limit,
        "approval" => state.rate_limiter.config.per_approval_endpoint_limit,
        _ => state.rate_limiter.config.per_principal_limit,
    };
    let principal_scope = format!("{endpoint_kind}.principal");
    state
        .rate_limiter
        .check(
            &format!("{endpoint_kind}:principal:{}", auth.principal_id),
            limit,
        )
        .map_err(|err| {
            if err.code == "RATE_LIMITED" {
                api_error_rate_limited(
                    &err.message,
                    &principal_scope,
                    err.retry_after_seconds
                        .unwrap_or(state.rate_limiter.config.window_seconds),
                )
            } else {
                api_error_with_code(err.status, err.code, &err.message)
            }
        })?;
    let ip_scope = format!("{endpoint_kind}.ip");
    state
        .rate_limiter
        .check(&format!("{endpoint_kind}:ip:{}", auth.client_ip), limit)
        .map_err(|err| {
            if err.code == "RATE_LIMITED" {
                api_error_rate_limited(
                    &err.message,
                    &ip_scope,
                    err.retry_after_seconds
                        .unwrap_or(state.rate_limiter.config.window_seconds),
                )
            } else {
                api_error_with_code(err.status, err.code, &err.message)
            }
        })?;
    Ok(())
}

fn parse_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let auth_value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = auth_value.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() {
        Some(token.trim())
    } else {
        None
    }
}

fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/api/v1/health", get(health))
        .route("/api/v1/status", get(status))
        .route("/api/v1/metrics", get(metrics))
        .route(
            "/api/v1/providers/capabilities",
            get(list_provider_capabilities),
        )
        .route("/api/v1/ws", get(ws_handler))
        .route("/api/v1/sessions", get(list_sessions).post(create_session))
        .route("/api/v1/sessions/{session_id}", get(get_session))
        .route(
            "/api/v1/sessions/{session_id}/messages",
            get(list_session_messages).post(create_message),
        )
        .route("/api/v1/sessions/{session_id}/runs", post(create_run))
        .route("/api/v1/runs/{run_id}/resume", post(resume_run))
        .route("/api/v1/memory/notes", get(list_notes).post(create_note))
        .route(
            "/api/v1/memory/notes/{note_id}",
            get(get_note).post(update_note),
        )
        .route("/api/v1/memory/search", post(search_memory))
        .route(
            "/api/v1/auth/profiles",
            get(list_auth_profiles).post(create_auth_profile),
        )
        .route(
            "/api/v1/auth/profiles/{auth_profile_id}/state",
            post(update_auth_profile_state),
        )
        .route("/api/v1/auth/openai/oauth/start", post(openai_oauth_start))
        .route(
            "/api/v1/auth/openai/oauth/finish",
            post(openai_oauth_finish),
        )
        .route(
            "/api/v1/auth/anthropic/setup-token/ingest",
            post(anthropic_setup_token_ingest),
        )
        .route(
            "/api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order",
            get(get_agent_provider_profile_order).post(set_agent_provider_profile_order),
        )
        .route(
            "/api/v1/config/channels",
            get(get_channel_config).post(update_channel_config),
        )
        .route(
            "/api/v1/channels/telegram/inbound",
            post(ingest_telegram_channel_message),
        )
        .route(
            "/api/v1/channels/discord/inbound",
            post(ingest_discord_channel_message),
        )
        .route(
            "/api/v1/channels/approvals/resolve",
            post(resolve_channel_approval_action),
        )
        .route("/api/v1/jobs", get(list_jobs))
        .route("/api/v1/jobs/status", get(job_status))
        .route("/api/v1/jobs/add", post(add_job))
        .route("/api/v1/jobs/{job_id}/update", post(update_job))
        .route("/api/v1/jobs/{job_id}/remove", post(remove_job))
        .route("/api/v1/jobs/{job_id}/run", post(run_job_now))
        .route("/api/v1/jobs/{job_id}/history", get(job_history))
        .route("/api/v1/approvals", get(list_approvals))
        .route("/api/v1/approvals/request", post(create_approval_request))
        .route(
            "/api/v1/approvals/{approval_id}/resolve",
            post(resolve_approval),
        )
        .route("/api/v1/security/audit", get(list_security_audit))
        .route(
            "/api/v1/security/audit/retention/run",
            post(run_security_audit_retention),
        )
        .route(
            "/api/v1/security/auth-profiles/{auth_profile_id}/rotate-secret",
            post(rotate_auth_profile_secret),
        )
        .route(
            "/api/v1/security/auth-profiles/{auth_profile_id}/revoke",
            post(revoke_auth_profile),
        )
        .with_state(state)
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("-");
                    tracing::info_span!(
                        "http.request",
                        method = %request.method(),
                        uri = %request.uri(),
                        request_id = %request_id
                    )
                })
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
}

async fn list_sessions(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let auth = require_bearer_auth(&headers, &state).map_err(|err| err.status)?;
    require_roles_raw(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])
        .map_err(|err| err.status)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let sessions = state
        .storage
        .list_sessions(limit)
        .map_err(|err| internal_err("listing sessions failed", err))?;
    debug!(limit, count = sessions.len(), "sessions listed");
    let payload = ListSessionsResponse {
        items: sessions.into_iter().map(to_session_summary).collect(),
    };
    Ok(Json(payload))
}

async fn create_session(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])?;

    let agent_id = request
        .agent_id
        .unwrap_or_else(|| "default".to_string())
        .trim()
        .to_string();
    if agent_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "agent_id cannot be empty",
        ));
    }

    let session = state
        .storage
        .create_session(NewSession {
            session_key: request.session_key.filter(|v| !v.trim().is_empty()),
            agent_id,
            title: request.title,
        })
        .map_err(|err| {
            if err.to_string().contains("agent does not exist") {
                api_error(StatusCode::BAD_REQUEST, &err.to_string())
            } else {
                internal_err_with_error("creating session failed", err)
            }
        })?;

    info!(
        session_id = %session.session_id,
        agent_id = %session.agent_id,
        title = ?session.title,
        "session created"
    );
    let response = CreateSessionResponse {
        session: to_session_summary(session),
    };
    Ok((StatusCode::CREATED, Json(response)))
}

async fn get_session(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    let maybe_session = state
        .storage
        .get_session(&session_id)
        .map_err(|err| internal_err_with_error("loading session failed", err))?;

    let session =
        maybe_session.ok_or_else(|| api_error(StatusCode::NOT_FOUND, "session not found"))?;
    Ok(Json(SessionDetailResponse {
        session: to_session_summary(session),
    }))
}

async fn create_message(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<CreateMessageRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    validate_role(&request.role)?;
    if request.content_text.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "content_text cannot be empty",
        ));
    }

    let message = state
        .storage
        .create_message(NewMessage {
            session_id,
            source_channel: request.source_channel.unwrap_or_else(|| "api".to_string()),
            source_peer_id: request.source_peer_id,
            source_message_id: request.source_message_id,
            role: request.role,
            content_text: request.content_text,
            content_format: request
                .content_format
                .unwrap_or_else(|| "markdown".to_string()),
        })
        .map_err(|err| internal_err_with_error("creating message failed", err))?;

    let message = message.ok_or_else(|| api_error(StatusCode::NOT_FOUND, "session not found"))?;
    info!(
        session_id = %message.session_id,
        message_id = %message.message_id,
        role = %message.role,
        content_chars = message.content_text.len(),
        "message created"
    );
    Ok((
        StatusCode::CREATED,
        Json(CreateMessageResponse {
            message: to_message_response(message),
        }),
    ))
}

async fn list_session_messages(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let items: Vec<MessageResponse> = state
        .storage
        .list_messages(&session_id, limit)
        .map_err(|err| internal_err_with_error("listing messages failed", err))?
        .into_iter()
        .map(to_message_response)
        .collect();
    debug!(session_id = %session_id, limit, count = items.len(), "session messages listed");
    Ok(Json(ListMessagesResponse { items }))
}

async fn list_notes(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListNotesQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let items = state
        .storage
        .list_notes(limit)
        .map_err(|err| internal_err_with_error("listing notes failed", err))?
        .into_iter()
        .map(to_note_response)
        .collect();
    Ok(Json(ListNotesResponse { items }))
}

async fn create_note(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateNoteRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    let body = request.body.trim().to_string();
    if body.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "note body cannot be empty",
        ));
    }
    let title = request
        .title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let tags = normalize_tags(request.tags);
    let tags_json = serde_json::to_string(&tags)
        .map_err(|err| internal_err_with_error("serializing note tags failed", err.into()))?;
    let note = state
        .storage
        .create_note(NewNote {
            title,
            body,
            tags_json,
        })
        .map_err(|err| internal_err_with_error("creating note failed", err))?;
    persist_note_embeddings(&state, &note)
        .map_err(|err| internal_err_with_error("persisting note embeddings failed", err))?;
    state
        .metrics
        .notes_created_total
        .fetch_add(1, Ordering::Relaxed);
    Ok((
        StatusCode::CREATED,
        Json(CreateNoteResponse {
            note: to_note_response(note),
        }),
    ))
}

async fn get_note(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(note_id): Path<String>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])?;
    let note = state
        .storage
        .get_note(&note_id)
        .map_err(|err| internal_err_with_error("loading note failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "note not found"))?;
    Ok(Json(GetNoteResponse {
        note: to_note_response(note),
    }))
}

async fn update_note(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(note_id): Path<String>,
    Json(request): Json<UpdateNoteRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    let title = request
        .title
        .map(|value| value.trim().to_string())
        .and_then(|value| if value.is_empty() { None } else { Some(value) });
    let body = request.body.map(|value| value.trim().to_string());
    if body.as_ref().is_some_and(|value| value.is_empty()) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "note body cannot be empty",
        ));
    }
    let tags_json = if request.tags.is_some() {
        Some(
            serde_json::to_string(&normalize_tags(request.tags)).map_err(|err| {
                internal_err_with_error("serializing note tags failed", err.into())
            })?,
        )
    } else {
        None
    };
    let note = state
        .storage
        .update_note(&note_id, title, body, tags_json)
        .map_err(|err| internal_err_with_error("updating note failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "note not found"))?;
    persist_note_embeddings(&state, &note)
        .map_err(|err| internal_err_with_error("persisting note embeddings failed", err))?;
    state
        .metrics
        .notes_updated_total
        .fetch_add(1, Ordering::Relaxed);
    Ok(Json(UpdateNoteResponse {
        note: to_note_response(note),
    }))
}

async fn search_memory(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<SearchMemoryRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])?;
    let query_text = request.query_text.trim().to_string();
    if query_text.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "query_text cannot be empty",
        ));
    }
    let top_k = request
        .top_k
        .unwrap_or(LOCAL_MEMORY_DEFAULT_TOP_K as u32)
        .clamp(1, 24) as usize;
    let max_chars = request
        .max_chars
        .unwrap_or(LOCAL_MEMORY_DEFAULT_MAX_CHARS)
        .clamp(200, 8000);
    let query_vector = embed_text_locally(&query_text);
    let matches = state
        .storage
        .search_note_embeddings(
            &query_vector,
            top_k as u32,
            (top_k * 4).max(LOCAL_MEMORY_DEFAULT_MAX_CANDIDATES) as u32,
        )
        .map_err(|err| internal_err_with_error("memory search failed", err))?;
    let mut used_chars = 0usize;
    let mut items = Vec::new();
    for item in matches {
        let snippet = item.snippet.trim().to_string();
        if snippet.is_empty() {
            continue;
        }
        if used_chars.saturating_add(snippet.len()) > max_chars {
            break;
        }
        used_chars = used_chars.saturating_add(snippet.len());
        items.push(SearchMemoryResult {
            note_id: item.note_id,
            title: item.note_title,
            snippet,
            score: item.score,
        });
    }
    Ok(Json(SearchMemoryResponse {
        query_text,
        top_k: top_k as u32,
        max_chars,
        items,
    }))
}

async fn list_auth_profiles(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListAuthProfilesQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY],
        "auth.profile.list",
        "auth_profiles",
    )?;
    let provider = query
        .provider
        .as_ref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let include_disabled = query.include_disabled.unwrap_or(false);
    let items = state
        .storage
        .list_auth_profiles(provider.as_deref(), include_disabled)
        .map_err(|err| internal_err_with_error("listing auth profiles failed", err))?
        .into_iter()
        .map(to_auth_profile_response)
        .collect();
    Ok(Json(ListAuthProfilesResponse { items }))
}

async fn create_auth_profile(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateAuthProfileRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN],
        "auth.profile.create",
        "auth_profiles",
    )?;

    let provider = request.provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "provider cannot be empty",
        ));
    }
    if !provider_supported(&provider) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "unsupported provider (expected: mock, openai, anthropic, unconfigured)",
        ));
    }

    let display_name = request.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "display_name cannot be empty",
        ));
    }

    let auth_mode = request.auth_mode.trim().to_ascii_lowercase();
    let policy = auth_mode_policy(&auth_mode)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "unsupported auth_mode"))?;
    let requested_risk_level = request.risk_level.trim().to_ascii_lowercase();
    if requested_risk_level != policy.risk_level {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "risk_level does not match registry classification for auth_mode",
        ));
    }
    if !provider_auth_mode_allowed(&provider, &auth_mode) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "auth_mode is not allowed for this provider",
        ));
    }

    let enabled = request.enabled.unwrap_or(true);
    let kill_switch_scope = request
        .kill_switch_scope
        .unwrap_or_else(|| {
            if policy.requires_kill_switch {
                KILL_SWITCH_SCOPE_PROFILE.to_string()
            } else {
                KILL_SWITCH_SCOPE_NONE.to_string()
            }
        })
        .trim()
        .to_ascii_lowercase();
    validate_kill_switch_scope(&kill_switch_scope)?;
    validate_high_risk_controls(&auth_mode, enabled, &kill_switch_scope)?;

    let credentials_json = request
        .credentials_json
        .unwrap_or_else(|| serde_json::json!({}))
        .to_string();

    if policy.requires_warning {
        warn!(
            provider = %provider,
            auth_mode = %auth_mode,
            risk_level = policy.risk_level,
            risk_notes = policy.risk_notes,
            requires_kill_switch = policy.requires_kill_switch,
            "creating high-risk auth profile"
        );
    }

    let profile = state
        .storage
        .create_auth_profile(NewAuthProfile {
            provider,
            display_name,
            auth_mode,
            risk_level: requested_risk_level,
            enabled,
            kill_switch_scope,
            api_base_url: request.api_base_url,
            credentials_json,
        })
        .map_err(|err| {
            if err.to_string().contains("UNIQUE constraint failed") {
                api_error(
                    StatusCode::CONFLICT,
                    "auth profile display_name already exists for this provider",
                )
            } else {
                internal_err_with_error("creating auth profile failed", err)
            }
        })?;

    info!(
        auth_profile_id = %profile.auth_profile_id,
        provider = %profile.provider,
        auth_mode = %profile.auth_mode,
        risk_level = %profile.risk_level,
        enabled = profile.enabled,
        kill_switch_scope = %profile.kill_switch_scope,
        "auth profile created"
    );
    record_security_audit(
        &headers,
        &state,
        &auth,
        "auth.profile.create",
        &format!("auth_profile:{}", profile.auth_profile_id),
        "allow",
        Some("auth profile created".to_string()),
        StatusCode::CREATED,
        None,
        None,
        None,
        Some(serde_json::json!({
            "provider": &profile.provider,
            "auth_mode": &profile.auth_mode,
            "risk_level": &profile.risk_level,
            "enabled": profile.enabled,
            "kill_switch_scope": &profile.kill_switch_scope
        })),
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateAuthProfileResponse {
            profile: to_auth_profile_response(profile),
        }),
    ))
}

async fn update_auth_profile_state(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(auth_profile_id): Path<String>,
    Json(request): Json<UpdateAuthProfileStateRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN],
        "auth.profile.update_state",
        &format!("auth_profile:{auth_profile_id}"),
    )?;
    let existing = state
        .storage
        .get_auth_profile(&auth_profile_id)
        .map_err(|err| internal_err_with_error("loading auth profile failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "auth profile not found"))?;

    let next_enabled = request.enabled.unwrap_or(existing.enabled);
    let next_scope = request
        .kill_switch_scope
        .unwrap_or_else(|| existing.kill_switch_scope.clone())
        .trim()
        .to_ascii_lowercase();
    validate_kill_switch_scope(&next_scope)?;
    validate_high_risk_controls(&existing.auth_mode, next_enabled, &next_scope)?;

    let updated = state
        .storage
        .update_auth_profile_state(&auth_profile_id, request.enabled, Some(next_scope.clone()))
        .map_err(|err| internal_err_with_error("updating auth profile state failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "auth profile not found"))?;

    info!(
        auth_profile_id = %updated.auth_profile_id,
        provider = %updated.provider,
        auth_mode = %updated.auth_mode,
        risk_level = %updated.risk_level,
        enabled = updated.enabled,
        kill_switch_scope = %updated.kill_switch_scope,
        "auth profile state updated"
    );
    record_security_audit(
        &headers,
        &state,
        &auth,
        "auth.profile.update_state",
        &format!("auth_profile:{}", updated.auth_profile_id),
        "allow",
        Some("auth profile state updated".to_string()),
        StatusCode::OK,
        None,
        None,
        None,
        Some(serde_json::json!({
            "provider": &updated.provider,
            "auth_mode": &updated.auth_mode,
            "risk_level": &updated.risk_level,
            "enabled": updated.enabled,
            "kill_switch_scope": &updated.kill_switch_scope
        })),
    );

    Ok(Json(UpdateAuthProfileStateResponse {
        profile: to_auth_profile_response(updated),
    }))
}

fn rotate_auth_profile_secret_outcome(
    state: &AppState,
    auth_profile_id: &str,
) -> std::result::Result<RotateAuthProfileSecretOutcome, (StatusCode, Json<ApiError>)> {
    let profile = state
        .storage
        .get_auth_profile(auth_profile_id)
        .map_err(|err| internal_err_with_error("loading auth profile failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "auth profile not found"))?;
    let mut metadata = auth_profile_credentials_payload(&profile)
        .map_err(|err| internal_err_with_error("loading auth profile credentials failed", err))?;
    let previous_secret_ref = secret_ref_from_metadata(&metadata).ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "auth profile does not reference a managed secret_ref",
        )
    })?;

    let secret_payload = state
        .secret_store
        .get_json(&previous_secret_ref)
        .map_err(|err| internal_err_with_error("loading secret payload failed", err))?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "existing secret_ref payload was not found",
            )
        })?;
    let rotated_secret_ref = format!("auth.rotate.{}", uuid::Uuid::new_v4());
    state
        .secret_store
        .set_json(&rotated_secret_ref, &secret_payload)
        .map_err(|err| internal_err_with_error("storing rotated secret failed", err))?;
    metadata["secret_ref"] = serde_json::Value::String(rotated_secret_ref.clone());
    metadata["rotated_at_unix"] = serde_json::Value::Number((current_time_ms() / 1000).into());

    let updated = state
        .storage
        .update_auth_profile_credentials(auth_profile_id, metadata.to_string())
        .map_err(|err| internal_err_with_error("updating auth profile credentials failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "auth profile not found"))?;

    if let Err(err) = state.secret_store.delete(&previous_secret_ref) {
        warn!(
            auth_profile_id = %auth_profile_id,
            secret_ref = %previous_secret_ref,
            error = %err,
            "failed to delete previous secret_ref after rotation"
        );
    }

    Ok(RotateAuthProfileSecretOutcome {
        profile: updated,
        previous_secret_ref,
        rotated_secret_ref,
    })
}

fn revoke_auth_profile_secret_outcome(
    state: &AppState,
    auth_profile_id: &str,
    remove_secret: bool,
    disable_profile: bool,
    kill_switch_scope: Option<String>,
) -> std::result::Result<RevokeAuthProfileSecretOutcome, (StatusCode, Json<ApiError>)> {
    let existing = state
        .storage
        .get_auth_profile(auth_profile_id)
        .map_err(|err| internal_err_with_error("loading auth profile failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "auth profile not found"))?;
    let metadata = auth_profile_credentials_payload(&existing)
        .map_err(|err| internal_err_with_error("loading auth profile credentials failed", err))?;
    let revoked_secret_ref = secret_ref_from_metadata(&metadata);

    let next_enabled = if disable_profile {
        false
    } else {
        existing.enabled
    };
    let next_scope = kill_switch_scope
        .unwrap_or_else(|| {
            if existing.kill_switch_scope == KILL_SWITCH_SCOPE_NONE {
                KILL_SWITCH_SCOPE_PROFILE.to_string()
            } else {
                existing.kill_switch_scope.clone()
            }
        })
        .trim()
        .to_ascii_lowercase();
    validate_kill_switch_scope(&next_scope)?;
    validate_high_risk_controls(&existing.auth_mode, next_enabled, &next_scope)?;

    let updated = state
        .storage
        .update_auth_profile_state(
            auth_profile_id,
            Some(next_enabled),
            Some(next_scope.clone()),
        )
        .map_err(|err| internal_err_with_error("updating auth profile state failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "auth profile not found"))?;

    if remove_secret {
        if let Some(secret_ref) = revoked_secret_ref.as_ref() {
            state
                .secret_store
                .delete(secret_ref)
                .map_err(|err| internal_err_with_error("deleting revoked secret failed", err))?;
        }
    }

    Ok(RevokeAuthProfileSecretOutcome {
        profile: updated,
        revoked_secret_ref,
    })
}

async fn rotate_auth_profile_secret(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(auth_profile_id): Path<String>,
    Json(request): Json<RotateAuthProfileSecretRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN],
        "security.secret.rotate",
        &format!("auth_profile:{auth_profile_id}"),
    )?;

    let outcome = rotate_auth_profile_secret_outcome(&state, &auth_profile_id)?;
    let reason = request
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    record_security_audit(
        &headers,
        &state,
        &auth,
        "security.secret.rotate",
        &format!("auth_profile:{auth_profile_id}"),
        "allow",
        reason
            .clone()
            .or_else(|| Some("auth profile secret rotated".to_string())),
        StatusCode::OK,
        None,
        None,
        None,
        Some(serde_json::json!({
            "provider": &outcome.profile.provider,
            "auth_mode": &outcome.profile.auth_mode,
            "previous_secret_ref": &outcome.previous_secret_ref,
            "rotated_secret_ref": &outcome.rotated_secret_ref
        })),
    );

    Ok(Json(RotateAuthProfileSecretResponse {
        profile: to_auth_profile_response(outcome.profile),
        previous_secret_ref: outcome.previous_secret_ref,
        rotated_secret_ref: outcome.rotated_secret_ref,
    }))
}

async fn revoke_auth_profile(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(auth_profile_id): Path<String>,
    Json(request): Json<RevokeAuthProfileRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN],
        "security.secret.revoke",
        &format!("auth_profile:{auth_profile_id}"),
    )?;

    let remove_secret = request.remove_secret.unwrap_or(true);
    let disable_profile = request.disable_profile.unwrap_or(true);
    let outcome = revoke_auth_profile_secret_outcome(
        &state,
        &auth_profile_id,
        remove_secret,
        disable_profile,
        request.kill_switch_scope.clone(),
    )?;

    let reason = request
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    record_security_audit(
        &headers,
        &state,
        &auth,
        "security.secret.revoke",
        &format!("auth_profile:{auth_profile_id}"),
        "allow",
        reason
            .clone()
            .or_else(|| Some("auth profile revoked".to_string())),
        StatusCode::OK,
        None,
        None,
        None,
        Some(serde_json::json!({
            "provider": &outcome.profile.provider,
            "auth_mode": &outcome.profile.auth_mode,
            "enabled": outcome.profile.enabled,
            "kill_switch_scope": &outcome.profile.kill_switch_scope,
            "revoked_secret_ref": &outcome.revoked_secret_ref,
            "remove_secret": remove_secret,
            "disable_profile": disable_profile
        })),
    );

    Ok(Json(RevokeAuthProfileResponse {
        profile: to_auth_profile_response(outcome.profile),
        revoked_secret_ref: outcome.revoked_secret_ref,
        reason,
    }))
}

async fn openai_oauth_start(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<OpenAiOauthStartRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER])?;

    let client_id = request
        .client_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CARSINOS_OPENAI_OAUTH_CLIENT_ID")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "openai oauth client_id is required (request.client_id or CARSINOS_OPENAI_OAUTH_CLIENT_ID)",
            )
        })?;
    let redirect_uri = request
        .redirect_uri
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CARSINOS_OPENAI_OAUTH_REDIRECT_URI")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| OAUTH_OPENAI_DEFAULT_REDIRECT_URI.to_string());
    let scope = request
        .scope
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CARSINOS_OPENAI_OAUTH_SCOPE")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| OAUTH_OPENAI_DEFAULT_SCOPE.to_string());
    let authorize_url = request
        .authorize_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CARSINOS_OPENAI_OAUTH_AUTHORIZE_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| OAUTH_OPENAI_DEFAULT_AUTHORIZE_URL.to_string());
    let token_url = request
        .token_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CARSINOS_OPENAI_OAUTH_TOKEN_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| OAUTH_OPENAI_DEFAULT_TOKEN_URL.to_string());
    let provider_api_base_url = request
        .api_base_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CARSINOS_OPENAI_API_BASE_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| OPENAI_DEFAULT_API_BASE.to_string());

    let oauth_session_id = uuid::Uuid::new_v4().to_string();
    let state_token = random_urlsafe_token(40);
    let code_verifier = random_urlsafe_token(96);
    let code_challenge = pkce_code_challenge(&code_verifier);
    let now = current_time_ms();
    let expires_at_ms = now.saturating_add(10 * 60 * 1000);

    let mut authorize = Url::parse(&authorize_url)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "authorize_url must be a valid URL"))?;
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", &scope)
        .append_pair("state", &state_token)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");

    let pending = PendingOpenAiOauthSession {
        oauth_session_id: oauth_session_id.clone(),
        state: state_token,
        code_verifier,
        redirect_uri: redirect_uri.clone(),
        client_id,
        scope,
        token_url,
        provider_api_base_url,
        display_name_hint: request
            .display_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        expires_at_ms,
    };
    cleanup_expired_oauth_sessions(&state).await;
    state
        .oauth_sessions
        .write()
        .await
        .insert(oauth_session_id.clone(), pending);

    info!(
        oauth_session_id = %oauth_session_id,
        expires_at_ms,
        "openai oauth pkce session created"
    );
    Ok(Json(OpenAiOauthStartResponse {
        oauth_session_id,
        authorize_url: authorize.to_string(),
        callback_url: redirect_uri,
        expires_in_seconds: 600,
    }))
}

async fn openai_oauth_finish(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<OpenAiOauthFinishRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN])?;
    cleanup_expired_oauth_sessions(&state).await;

    let oauth_session_id = request.oauth_session_id.trim();
    if oauth_session_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "oauth_session_id cannot be empty",
        ));
    }

    let pending = state
        .oauth_sessions
        .read()
        .await
        .get(oauth_session_id)
        .cloned()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "oauth session not found or expired"))?;
    let (code, state_token) = extract_code_state(&request)?;
    if state_token != pending.state {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "oauth callback state mismatch",
        ));
    }
    let token_response = exchange_openai_oauth_code(&pending, &code)
        .await
        .map_err(|err| {
            api_error(
                StatusCode::BAD_GATEWAY,
                &format!("token exchange failed: {err}"),
            )
        })?;
    let expires_in = token_response.expires_in.unwrap_or(3600).max(60);
    let expires_at_unix = (current_time_ms() / 1000).saturating_add(expires_in - 30);
    let account_id = extract_account_id_from_jwt(&token_response.access_token);

    let secret_ref = format!("auth.openai.oauth.{}", uuid::Uuid::new_v4());
    let secret_payload = serde_json::json!({
        "access_token": token_response.access_token,
        "refresh_token": token_response.refresh_token,
    });
    state
        .secret_store
        .set_json(&secret_ref, &secret_payload)
        .map_err(|err| internal_err_with_error("storing oauth secret failed", err))?;

    let display_name = request
        .display_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| pending.display_name_hint.clone())
        .or_else(|| {
            account_id
                .as_ref()
                .map(|value| format!("openai-codex-{value}"))
        })
        .unwrap_or_else(|| format!("openai-codex-{}", &pending.oauth_session_id[..8]));
    let provider_api_base_url = request
        .api_base_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| pending.provider_api_base_url.clone());
    let credentials_json = serde_json::json!({
        "secret_ref": secret_ref,
        "refresh_url": pending.token_url,
        "client_id": pending.client_id,
        "scope": pending.scope,
        "expires_at_unix": expires_at_unix,
        "account_id": account_id,
        "token_source": "openai_oauth_pkce"
    })
    .to_string();

    let profile = state
        .storage
        .create_auth_profile(NewAuthProfile {
            provider: AUTH_PROVIDER_OPENAI.to_string(),
            display_name,
            auth_mode: AUTH_MODE_OPENAI_OAUTH.to_string(),
            risk_level: auth_mode_policy(AUTH_MODE_OPENAI_OAUTH)
                .expect("openai oauth policy")
                .risk_level
                .to_string(),
            enabled: true,
            kill_switch_scope: KILL_SWITCH_SCOPE_NONE.to_string(),
            api_base_url: Some(provider_api_base_url),
            credentials_json,
        })
        .map_err(|err| {
            if err.to_string().contains("UNIQUE constraint failed") {
                api_error(
                    StatusCode::CONFLICT,
                    "auth profile display_name already exists for this provider",
                )
            } else {
                internal_err_with_error("creating oauth auth profile failed", err)
            }
        })?;

    state.oauth_sessions.write().await.remove(oauth_session_id);
    info!(
        oauth_session_id = %oauth_session_id,
        auth_profile_id = %profile.auth_profile_id,
        account_id = ?extract_account_id_from_profile_credentials(&profile.credentials_json),
        "openai oauth flow completed"
    );
    Ok((
        StatusCode::CREATED,
        Json(OpenAiOauthFinishResponse {
            profile: to_auth_profile_response(profile),
            account_id,
            expires_at_unix: Some(expires_at_unix),
        }),
    ))
}

async fn anthropic_setup_token_ingest(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<AnthropicSetupTokenIngestRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER])?;

    let display_name = request.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "display_name cannot be empty",
        ));
    }
    let setup_token = request.setup_token.trim().to_string();
    if setup_token.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "setup_token cannot be empty",
        ));
    }
    let api_base_url = request
        .api_base_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ANTHROPIC_DEFAULT_API_BASE.to_string());
    validate_anthropic_setup_token(&api_base_url, &setup_token)
        .await
        .map_err(|err| {
            api_error(
                StatusCode::BAD_GATEWAY,
                &format!("setup token validation failed: {err}"),
            )
        })?;

    let kill_switch_scope = request
        .kill_switch_scope
        .unwrap_or_else(|| KILL_SWITCH_SCOPE_NONE.to_string())
        .trim()
        .to_ascii_lowercase();
    validate_kill_switch_scope(&kill_switch_scope)?;
    validate_high_risk_controls(
        AUTH_MODE_API_KEY,
        request.enabled.unwrap_or(true),
        &kill_switch_scope,
    )?;

    let secret_ref = format!("auth.anthropic.setup-token.{}", uuid::Uuid::new_v4());
    let secret_payload = serde_json::json!({
        "api_key": setup_token,
        "token_kind": "setup_token"
    });
    state
        .secret_store
        .set_json(&secret_ref, &secret_payload)
        .map_err(|err| internal_err_with_error("storing anthropic setup token failed", err))?;
    let credentials_json = serde_json::json!({
        "secret_ref": secret_ref,
        "token_kind": "setup_token",
        "validated_at_unix": current_time_ms() / 1000
    })
    .to_string();

    let profile = state
        .storage
        .create_auth_profile(NewAuthProfile {
            provider: AUTH_PROVIDER_ANTHROPIC.to_string(),
            display_name,
            auth_mode: AUTH_MODE_API_KEY.to_string(),
            risk_level: auth_mode_policy(AUTH_MODE_API_KEY)
                .expect("api key policy")
                .risk_level
                .to_string(),
            enabled: request.enabled.unwrap_or(true),
            kill_switch_scope,
            api_base_url: Some(api_base_url),
            credentials_json,
        })
        .map_err(|err| {
            if err.to_string().contains("UNIQUE constraint failed") {
                api_error(
                    StatusCode::CONFLICT,
                    "auth profile display_name already exists for this provider",
                )
            } else {
                internal_err_with_error("creating anthropic setup-token profile failed", err)
            }
        })?;

    info!(
        auth_profile_id = %profile.auth_profile_id,
        provider = %profile.provider,
        auth_mode = %profile.auth_mode,
        "anthropic setup-token ingested"
    );
    Ok((
        StatusCode::CREATED,
        Json(AnthropicSetupTokenIngestResponse {
            profile: to_auth_profile_response(profile),
        }),
    ))
}

fn random_urlsafe_token(min_len: usize) -> String {
    let mut out = String::new();
    while out.len() < min_len {
        out.push_str(&URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes()));
    }
    out.truncate(min_len);
    out
}

fn pkce_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

async fn cleanup_expired_oauth_sessions(state: &AppState) {
    let now = current_time_ms();
    state
        .oauth_sessions
        .write()
        .await
        .retain(|_, session| session.expires_at_ms > now);
}

fn extract_code_state(
    request: &OpenAiOauthFinishRequest,
) -> std::result::Result<(String, String), (StatusCode, Json<ApiError>)> {
    if let Some(callback_url) = request
        .callback_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let url = Url::parse(&callback_url)
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, "callback_url must be a valid URL"))?;
        let mut code = None;
        let mut state = None;
        for (key, value) in url.query_pairs() {
            if key == "code" {
                let value = value.trim();
                if !value.is_empty() {
                    code = Some(value.to_string());
                }
            } else if key == "state" {
                let value = value.trim();
                if !value.is_empty() {
                    state = Some(value.to_string());
                }
            }
        }
        let code = code.ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "callback_url is missing code query parameter",
            )
        })?;
        let state = state.ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "callback_url is missing state query parameter",
            )
        })?;
        return Ok((code, state));
    }

    let code = request
        .code
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "code is required when callback_url is not provided",
            )
        })?;
    let state = request
        .state
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "state is required when callback_url is not provided",
            )
        })?;
    Ok((code, state))
}

async fn exchange_openai_oauth_code(
    session: &PendingOpenAiOauthSession,
    code: &str,
) -> AnyResult<OpenAiTokenExchangeResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("failed building oauth token client")?;
    let response = client
        .post(&session.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", session.client_id.as_str()),
            ("redirect_uri", session.redirect_uri.as_str()),
            ("code_verifier", session.code_verifier.as_str()),
            ("code", code),
        ])
        .send()
        .await
        .context("openai oauth token exchange HTTP request failed")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let body = body.trim();
        let body = if body.len() > 240 { &body[..240] } else { body };
        anyhow::bail!(
            "status={} body={}",
            status.as_u16(),
            if body.is_empty() { "<empty>" } else { body }
        );
    }
    let payload: OpenAiTokenExchangeResponse = response
        .json()
        .await
        .context("failed parsing oauth token exchange response")?;
    if payload.access_token.trim().is_empty() {
        anyhow::bail!("oauth token exchange response missing access_token");
    }
    Ok(payload)
}

async fn validate_anthropic_setup_token(api_base_url: &str, setup_token: &str) -> AnyResult<()> {
    let base = api_base_url.trim_end_matches('/');
    let url = format!("{base}/v1/models");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed building anthropic setup token validation client")?;
    let response = client
        .get(url)
        .header("x-api-key", setup_token)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .context("anthropic setup token validation HTTP request failed")?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let body = body.trim();
    let body = if body.len() > 220 { &body[..220] } else { body };
    anyhow::bail!(
        "status={} body={}",
        status.as_u16(),
        if body.is_empty() { "<empty>" } else { body }
    );
}

fn extract_account_id_from_jwt(access_token: &str) -> Option<String> {
    let mut parts = access_token.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    let payload = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let payload_json: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    payload_json
        .get("account_id")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            payload_json
                .get("sub")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn extract_account_id_from_profile_credentials(credentials_json: &str) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(credentials_json).ok()?;
    payload
        .get("account_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

async fn get_agent_provider_profile_order(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((agent_id, provider)): Path<(String, String)>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "auth.profile_order.get",
        &format!("agent:{agent_id}:provider:{provider}"),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "run")?;
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "provider cannot be empty",
        ));
    }
    let profile_ids = state
        .storage
        .list_agent_provider_profile_order(&agent_id, &provider)
        .map_err(|err| internal_err_with_error("listing profile order failed", err))?;
    Ok(Json(GetAgentProviderProfileOrderResponse {
        agent_id,
        provider,
        profile_ids,
    }))
}

async fn set_agent_provider_profile_order(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((agent_id, provider)): Path<(String, String)>,
    Json(request): Json<SetAgentProviderProfileOrderRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "auth.profile_order.set",
        &format!("agent:{agent_id}:provider:{provider}"),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "run")?;
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "provider cannot be empty",
        ));
    }

    let mut seen = HashSet::new();
    for profile_id in &request.profile_ids {
        if !seen.insert(profile_id.clone()) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "profile_ids cannot contain duplicates",
            ));
        }
    }

    let profile_ids = state
        .storage
        .set_agent_provider_profile_order(&agent_id, &provider, &request.profile_ids)
        .map_err(|err| {
            if err.to_string().contains("auth profile not found")
                || err.to_string().contains("belongs to provider")
                || err.to_string().contains("agent does not exist")
            {
                api_error(StatusCode::BAD_REQUEST, &err.to_string())
            } else {
                internal_err_with_error("saving profile order failed", err)
            }
        })?;

    info!(
        agent_id = %agent_id,
        provider = %provider,
        profile_count = profile_ids.len(),
        "agent provider profile order updated"
    );
    Ok(Json(SetAgentProviderProfileOrderResponse {
        agent_id,
        provider,
        profile_ids,
    }))
}

async fn get_channel_config(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_error(&auth, &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY])?;
    let config = load_channel_config(&state)
        .map_err(|err| internal_err_with_error("loading channel config failed", err))?;
    Ok(Json(GetChannelConfigResponse { config }))
}

async fn update_channel_config(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<UpdateChannelConfigRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN],
        "channel.config.update",
        "config.channels",
    )?;
    let existing = load_channel_config(&state)
        .map_err(|err| internal_err_with_error("loading channel config failed", err))?;
    let discord = request.discord.unwrap_or(existing.discord);
    let telegram = request.telegram.unwrap_or(existing.telegram);

    let discord_json = serde_json::to_string(&discord)
        .map_err(|err| internal_err_with_error("serializing discord config failed", err.into()))?;
    let telegram_json = serde_json::to_string(&telegram)
        .map_err(|err| internal_err_with_error("serializing telegram config failed", err.into()))?;
    let discord_updated_at = state
        .storage
        .set_app_kv_json(APP_KV_CHANNELS_DISCORD, discord_json)
        .map_err(|err| internal_err_with_error("saving discord config failed", err))?;
    let telegram_updated_at = state
        .storage
        .set_app_kv_json(APP_KV_CHANNELS_TELEGRAM, telegram_json)
        .map_err(|err| internal_err_with_error("saving telegram config failed", err))?;
    let updated_at = discord_updated_at.max(telegram_updated_at);

    let config = carsinos_protocol::ChannelConfigResponse {
        discord,
        telegram,
        updated_at,
    };
    Ok(Json(UpdateChannelConfigResponse { config }))
}

fn get_or_create_channel_session(
    state: &AppState,
    session_key: &str,
    title: String,
) -> AnyResult<SessionRecord> {
    if let Some(existing) = state.storage.get_session_by_key(session_key)? {
        return Ok(existing);
    }

    match state.storage.create_session(NewSession {
        session_key: Some(session_key.to_string()),
        agent_id: "default".to_string(),
        title: Some(title),
    }) {
        Ok(created) => Ok(created),
        Err(err) => {
            if err
                .to_string()
                .contains("UNIQUE constraint failed: sessions.session_key")
            {
                if let Some(existing) = state.storage.get_session_by_key(session_key)? {
                    return Ok(existing);
                }
            }
            Err(err)
        }
    }
}

async fn execute_channel_run(
    state: &AppState,
    session: &SessionRecord,
    model_provider: String,
    model_id: String,
    requested_auth_profile_id: Option<&str>,
) -> AnyResult<RunRecord> {
    let created_run = state
        .storage
        .create_run(NewRun {
            session_id: session.session_id.clone(),
            model_provider,
            model_id,
        })?
        .with_context(|| format!("creating run for session {} failed", session.session_id))?;
    execute_run_with_status_handling(
        state,
        &created_run,
        &session.agent_id,
        requested_auth_profile_id,
    )
    .await
}

async fn ingest_telegram_channel_message(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<IngestTelegramMessageRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_CHANNEL_ADAPTER, ROLE_OPERATOR_ADMIN],
        "channel.telegram.ingest",
        &format!("telegram:chat:{}", request.chat_id),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "run")?;

    let config = load_channel_config(&state)
        .map_err(|err| internal_err_with_error("loading channel config failed", err))?;
    let route_config = telegram_channel::TelegramAdapterConfig {
        require_mention_in_groups: config.telegram.require_mention_in_groups,
        allowlisted_user_ids: config.telegram.allowlisted_user_ids.clone(),
    };
    let inbound = telegram_channel::TelegramInboundMessage {
        chat_id: request.chat_id,
        user_id: request.user_id,
        text: request.text.clone(),
        is_group_chat: request.is_group_chat,
        mentions_bot: request.mentions_bot,
        reply_to_bot: request.reply_to_bot,
    };

    match telegram_channel::route_message(&route_config, &inbound) {
        telegram_channel::RouteDecision::Reject(reason) => {
            let reason_text = reason.to_string();
            record_security_audit(
                &headers,
                &state,
                &auth,
                "channel.telegram.ingest",
                &format!("telegram:chat:{}", request.chat_id),
                "deny",
                Some(reason_text.clone()),
                StatusCode::OK,
                Some("CHANNEL_ROUTE_REJECTED"),
                None,
                None,
                None,
            );
            Ok(Json(IngestChannelMessageResponse {
                decision: "rejected".to_string(),
                reason: Some(reason_text),
                session_id: None,
                message_id: None,
                run_id: None,
            }))
        }
        telegram_channel::RouteDecision::Ignore(reason) => {
            let reason_text = reason.to_string();
            record_security_audit(
                &headers,
                &state,
                &auth,
                "channel.telegram.ingest",
                &format!("telegram:chat:{}", request.chat_id),
                "allow",
                Some(reason_text.clone()),
                StatusCode::OK,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "decision": "ignored"
                })),
            );
            Ok(Json(IngestChannelMessageResponse {
                decision: "ignored".to_string(),
                reason: Some(reason_text),
                session_id: None,
                message_id: None,
                run_id: None,
            }))
        }
        telegram_channel::RouteDecision::Accept => {
            let session_key = telegram_channel::session_key(&inbound);
            let session = get_or_create_channel_session(
                &state,
                &session_key,
                format!("telegram {}", request.chat_id),
            )
            .map_err(|err| internal_err_with_error("creating channel session failed", err))?;
            let created_message = state
                .storage
                .create_message(NewMessage {
                    session_id: session.session_id.clone(),
                    source_channel: "telegram".to_string(),
                    source_peer_id: Some(request.user_id.to_string()),
                    source_message_id: request.source_message_id.clone(),
                    role: "user".to_string(),
                    content_text: request.text.clone(),
                    content_format: "markdown".to_string(),
                })
                .map_err(|err| internal_err_with_error("creating channel message failed", err))?
                .ok_or_else(|| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "channel session missing while writing inbound message",
                    )
                })?;

            let run_immediately = request
                .run_immediately
                .unwrap_or(config.telegram.auto_run_enabled);
            let mut run_id = None;
            if run_immediately {
                let model_provider = request
                    .model_provider
                    .unwrap_or_else(|| config.telegram.default_model_provider.clone())
                    .trim()
                    .to_string();
                let model_id = request
                    .model_id
                    .unwrap_or_else(|| config.telegram.default_model_id.clone())
                    .trim()
                    .to_string();
                if model_provider.is_empty() || model_id.is_empty() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "model_provider and model_id must be non-empty when run_immediately=true",
                    ));
                }
                if !provider_supported(&model_provider) {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "unsupported model_provider",
                    ));
                }
                let requested_auth_profile_id = request
                    .auth_profile_id
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let run = execute_channel_run(
                    &state,
                    &session,
                    model_provider,
                    model_id,
                    requested_auth_profile_id.as_deref(),
                )
                .await
                .map_err(|err| {
                    internal_err_with_error("executing inbound channel run failed", err)
                })?;
                run_id = Some(run.run_id);
            }

            record_security_audit(
                &headers,
                &state,
                &auth,
                "channel.telegram.ingest",
                &format!("telegram:chat:{}", request.chat_id),
                "allow",
                Some("inbound message ingested".to_string()),
                StatusCode::OK,
                None,
                Some(&session.session_id),
                run_id.as_deref(),
                Some(serde_json::json!({
                    "decision": "accepted",
                    "run_immediately": run_immediately
                })),
            );

            Ok(Json(IngestChannelMessageResponse {
                decision: "accepted".to_string(),
                reason: None,
                session_id: Some(session.session_id),
                message_id: Some(created_message.message_id),
                run_id,
            }))
        }
    }
}

async fn ingest_discord_channel_message(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<IngestDiscordMessageRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_CHANNEL_ADAPTER, ROLE_OPERATOR_ADMIN],
        "channel.discord.ingest",
        &format!("discord:channel:{}", request.channel_id),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "run")?;

    let config = load_channel_config(&state)
        .map_err(|err| internal_err_with_error("loading channel config failed", err))?;
    let route_config = discord_channel::DiscordAdapterConfig {
        require_mention_in_guild_channels: config.discord.require_mention_in_guild_channels,
        allowlisted_user_ids: config.discord.allowlisted_user_ids.clone(),
    };
    let inbound = discord_channel::DiscordInboundMessage {
        guild_id: request.guild_id.clone(),
        channel_id: request.channel_id.clone(),
        thread_id: request.thread_id.clone(),
        author_id: request.author_id.clone(),
        text: request.text.clone(),
        mentions_bot: request.mentions_bot,
        is_dm: request.is_dm,
    };

    match discord_channel::route_message(&route_config, &inbound) {
        discord_channel::RouteDecision::Reject(reason) => {
            let reason_text = reason.to_string();
            record_security_audit(
                &headers,
                &state,
                &auth,
                "channel.discord.ingest",
                &format!("discord:channel:{}", request.channel_id),
                "deny",
                Some(reason_text.clone()),
                StatusCode::OK,
                Some("CHANNEL_ROUTE_REJECTED"),
                None,
                None,
                None,
            );
            Ok(Json(IngestChannelMessageResponse {
                decision: "rejected".to_string(),
                reason: Some(reason_text),
                session_id: None,
                message_id: None,
                run_id: None,
            }))
        }
        discord_channel::RouteDecision::Ignore(reason) => {
            let reason_text = reason.to_string();
            record_security_audit(
                &headers,
                &state,
                &auth,
                "channel.discord.ingest",
                &format!("discord:channel:{}", request.channel_id),
                "allow",
                Some(reason_text.clone()),
                StatusCode::OK,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "decision": "ignored"
                })),
            );
            Ok(Json(IngestChannelMessageResponse {
                decision: "ignored".to_string(),
                reason: Some(reason_text),
                session_id: None,
                message_id: None,
                run_id: None,
            }))
        }
        discord_channel::RouteDecision::Accept => {
            let session_key = discord_channel::session_key(&inbound);
            let session = get_or_create_channel_session(
                &state,
                &session_key,
                format!("discord {}", request.channel_id),
            )
            .map_err(|err| internal_err_with_error("creating channel session failed", err))?;
            let created_message = state
                .storage
                .create_message(NewMessage {
                    session_id: session.session_id.clone(),
                    source_channel: "discord".to_string(),
                    source_peer_id: Some(request.author_id.clone()),
                    source_message_id: request.source_message_id.clone(),
                    role: "user".to_string(),
                    content_text: request.text.clone(),
                    content_format: "markdown".to_string(),
                })
                .map_err(|err| internal_err_with_error("creating channel message failed", err))?
                .ok_or_else(|| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "channel session missing while writing inbound message",
                    )
                })?;

            let run_immediately = request
                .run_immediately
                .unwrap_or(config.discord.auto_run_enabled);
            let mut run_id = None;
            if run_immediately {
                let model_provider = request
                    .model_provider
                    .unwrap_or_else(|| config.discord.default_model_provider.clone())
                    .trim()
                    .to_string();
                let model_id = request
                    .model_id
                    .unwrap_or_else(|| config.discord.default_model_id.clone())
                    .trim()
                    .to_string();
                if model_provider.is_empty() || model_id.is_empty() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "model_provider and model_id must be non-empty when run_immediately=true",
                    ));
                }
                if !provider_supported(&model_provider) {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "unsupported model_provider",
                    ));
                }
                let requested_auth_profile_id = request
                    .auth_profile_id
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let run = execute_channel_run(
                    &state,
                    &session,
                    model_provider,
                    model_id,
                    requested_auth_profile_id.as_deref(),
                )
                .await
                .map_err(|err| {
                    internal_err_with_error("executing inbound channel run failed", err)
                })?;
                run_id = Some(run.run_id);
            }

            record_security_audit(
                &headers,
                &state,
                &auth,
                "channel.discord.ingest",
                &format!("discord:channel:{}", request.channel_id),
                "allow",
                Some("inbound message ingested".to_string()),
                StatusCode::OK,
                None,
                Some(&session.session_id),
                run_id.as_deref(),
                Some(serde_json::json!({
                    "decision": "accepted",
                    "run_immediately": run_immediately
                })),
            );

            Ok(Json(IngestChannelMessageResponse {
                decision: "accepted".to_string(),
                reason: None,
                session_id: Some(session.session_id),
                message_id: Some(created_message.message_id),
                run_id,
            }))
        }
    }
}

async fn resolve_channel_approval_action(
    headers: HeaderMap,
    state: State<AppState>,
    Json(request): Json<ResolveChannelApprovalActionRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let provider = request.provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "provider cannot be empty",
        ));
    }

    let action_payload = request.action_payload.trim();
    if action_payload.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "action_payload cannot be empty",
        ));
    }

    let actor_peer_id = request.actor_peer_id.trim();
    if actor_peer_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "actor_peer_id cannot be empty",
        ));
    }

    let parsed = match provider.as_str() {
        "telegram" => telegram_channel::parse_approval_callback_payload(action_payload),
        "discord" => discord_channel::parse_approval_custom_id(action_payload),
        _ => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "provider must be one of: telegram, discord",
            ))
        }
    };
    let (approval_id, decision) = parsed.ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid action_payload for provider approval action format",
        )
    })?;

    let mut forwarded_headers = headers.clone();
    let actor_header = HeaderValue::from_str(actor_peer_id).map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "actor_peer_id contains invalid header characters",
        )
    })?;
    forwarded_headers.insert("x-operator-id", actor_header);

    resolve_approval(
        forwarded_headers,
        state,
        Path(approval_id),
        Json(ResolveApprovalRequest {
            decision,
            decided_via: Some(provider),
            decided_by_peer_id: Some(actor_peer_id.to_string()),
        }),
    )
    .await
}

async fn list_jobs(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListJobsQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[
            ROLE_OPERATOR_ADMIN,
            ROLE_OPERATOR_READONLY,
            ROLE_AUTOMATION_RUNNER,
        ],
        "job.list",
        "jobs",
    )?;
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let include_disabled = query.include_disabled.unwrap_or(false);
    let items = state
        .storage
        .list_jobs(limit, include_disabled)
        .map_err(|err| internal_err_with_error("listing jobs failed", err))?
        .into_iter()
        .map(to_job_response)
        .collect();
    Ok(Json(ListJobsResponse { items }))
}

async fn job_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[
            ROLE_OPERATOR_ADMIN,
            ROLE_OPERATOR_READONLY,
            ROLE_AUTOMATION_RUNNER,
        ],
        "job.status",
        "jobs",
    )?;
    let now_ms = current_time_ms();
    let jobs_total = state
        .storage
        .jobs_total_count()
        .map_err(|err| internal_err_with_error("loading jobs total failed", err))?;
    let jobs_enabled = state
        .storage
        .jobs_enabled_count()
        .map_err(|err| internal_err_with_error("loading enabled jobs count failed", err))?;
    let jobs_due = state
        .storage
        .jobs_due_count(now_ms)
        .map_err(|err| internal_err_with_error("loading due jobs count failed", err))?;
    Ok(Json(JobStatusResponse {
        scheduler_running: true,
        jobs_total,
        jobs_enabled,
        jobs_due,
        now_utc: Utc::now(),
    }))
}

async fn add_job(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateJobRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "job.create",
        "jobs",
    )?;
    let now_ms = current_time_ms();
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "name cannot be empty"));
    }
    let schedule_kind = request.schedule_kind.trim().to_ascii_lowercase();
    let interval_seconds = request.interval_seconds.map(|value| value as i64);
    let run_at_ms = request.run_at_ms;
    let next_run_at =
        compute_initial_next_run_at(schedule_kind.as_str(), interval_seconds, run_at_ms, now_ms)?;
    let payload_json = request
        .payload_json
        .unwrap_or_else(|| serde_json::json!({ "mode": "noop" }))
        .to_string();
    let max_retries = request.max_retries.unwrap_or(1).clamp(0, 10) as i64;
    let retry_backoff_ms = request.retry_backoff_ms.unwrap_or(1_000).clamp(10, 300_000) as i64;
    let timeout_ms = request.timeout_ms.unwrap_or(60_000).clamp(50, 600_000) as i64;
    let enabled = request.enabled.unwrap_or(true);
    let agent_id = request
        .agent_id
        .unwrap_or_else(|| "default".to_string())
        .trim()
        .to_string();
    if agent_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "agent_id cannot be empty",
        ));
    }

    let job = state
        .storage
        .create_job(NewJob {
            agent_id,
            name,
            enabled,
            schedule_kind,
            interval_seconds,
            run_at_ms,
            next_run_at,
            payload_json,
            max_retries,
            retry_backoff_ms,
            timeout_ms,
        })
        .map_err(|err| {
            if err.to_string().contains("agent does not exist") {
                api_error(StatusCode::BAD_REQUEST, &err.to_string())
            } else {
                internal_err_with_error("creating job failed", err)
            }
        })?;

    emit_event(
        &state,
        "job.created",
        serde_json::json!({
            "job_id": &job.job_id,
            "agent_id": &job.agent_id,
            "enabled": job.enabled,
            "next_run_at": job.next_run_at
        }),
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateJobResponse {
            job: to_job_response(job),
        }),
    ))
}

async fn update_job(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Json(request): Json<UpdateJobRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "job.update",
        &format!("job:{job_id}"),
    )?;
    let current = state
        .storage
        .get_job(&job_id)
        .map_err(|err| internal_err_with_error("loading job failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "job not found"))?;

    let name = request
        .name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let interval_seconds = request.interval_seconds.map(|value| value as i64);
    let run_at_ms = request.run_at_ms;
    let next_run_at =
        compute_updated_next_run_at(&current, interval_seconds, run_at_ms, current_time_ms())?;

    let updated = state
        .storage
        .update_job(
            &job_id,
            JobUpdatePatch {
                name,
                enabled: request.enabled,
                interval_seconds,
                run_at_ms,
                next_run_at,
                payload_json: request.payload_json.map(|value| value.to_string()),
                max_retries: request.max_retries.map(|value| value as i64),
                retry_backoff_ms: request.retry_backoff_ms.map(|value| value as i64),
                timeout_ms: request.timeout_ms.map(|value| value as i64),
            },
        )
        .map_err(|err| internal_err_with_error("updating job failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "job not found"))?;

    emit_event(
        &state,
        "job.updated",
        serde_json::json!({
            "job_id": &updated.job_id,
            "enabled": updated.enabled,
            "next_run_at": updated.next_run_at
        }),
    );
    Ok(Json(UpdateJobResponse {
        job: to_job_response(updated),
    }))
}

async fn remove_job(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "job.remove",
        &format!("job:{job_id}"),
    )?;
    let removed = state
        .storage
        .remove_job(&job_id)
        .map_err(|err| internal_err_with_error("removing job failed", err))?;
    if !removed {
        return Err(api_error(StatusCode::NOT_FOUND, "job not found"));
    }
    emit_event(
        &state,
        "job.removed",
        serde_json::json!({
            "job_id": &job_id
        }),
    );
    Ok(Json(RemoveJobResponse { job_id, removed }))
}

async fn run_job_now(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "job.run_now",
        &format!("job:{job_id}"),
    )?;
    let job = state
        .storage
        .get_job(&job_id)
        .map_err(|err| internal_err_with_error("loading job for run-now failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "job not found"))?;

    let run = execute_job_once(&state, &job, "manual")
        .await
        .map_err(|err| internal_err_with_error("run-now execution failed", err))?;
    Ok(Json(RunJobNowResponse {
        job_run: to_job_run_response(run),
    }))
}

async fn job_history(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<ListJobHistoryQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[
            ROLE_OPERATOR_ADMIN,
            ROLE_OPERATOR_READONLY,
            ROLE_AUTOMATION_RUNNER,
        ],
        "job.history",
        &format!("job:{job_id}"),
    )?;
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let items = state
        .storage
        .list_job_runs(&job_id, limit)
        .map_err(|err| internal_err_with_error("listing job history failed", err))?
        .into_iter()
        .map(to_job_run_response)
        .collect();
    Ok(Json(ListJobHistoryResponse { items }))
}

async fn create_run(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<CreateRunRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "run.create",
        &format!("session:{session_id}"),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "run")?;
    let model_provider = request
        .model_provider
        .unwrap_or_else(|| "mock".to_string())
        .trim()
        .to_string();
    let model_id = request
        .model_id
        .unwrap_or_else(|| "mock-echo-v1".to_string())
        .trim()
        .to_string();

    if model_provider.is_empty() || model_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "model_provider and model_id must be non-empty",
        ));
    }
    if !provider_supported(&model_provider) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "unsupported model_provider",
        ));
    }

    let requested_auth_profile_id = request
        .auth_profile_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let session = state
        .storage
        .get_session(&session_id)
        .map_err(|err| internal_err_with_error("loading session before run failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "session not found"))?;

    let created_run = state
        .storage
        .create_run(NewRun {
            session_id: session_id.clone(),
            model_provider,
            model_id,
        })
        .map_err(|err| internal_err_with_error("creating run failed", err))?;

    let created_run =
        created_run.ok_or_else(|| api_error(StatusCode::NOT_FOUND, "session not found"))?;
    info!(
        run_id = %created_run.run_id,
        session_id = %created_run.session_id,
        model_provider = %created_run.model_provider,
        model_id = %created_run.model_id,
        "run created"
    );
    record_security_audit(
        &headers,
        &state,
        &auth,
        "run.create",
        &format!("run:{}", created_run.run_id),
        "allow",
        Some("run created".to_string()),
        StatusCode::CREATED,
        None,
        Some(&created_run.session_id),
        Some(&created_run.run_id),
        Some(serde_json::json!({
            "model_provider": &created_run.model_provider,
            "model_id": &created_run.model_id
        })),
    );
    emit_event(
        &state,
        "run.created",
        serde_json::json!({
            "run_id": &created_run.run_id,
            "session_id": &created_run.session_id,
            "status": &created_run.status,
            "model_provider": &created_run.model_provider,
            "model_id": &created_run.model_id
        }),
    );

    let run = execute_run_with_status_handling(
        &state,
        &created_run,
        &session.agent_id,
        requested_auth_profile_id.as_deref(),
    )
    .await
    .map_err(|err| internal_err_with_error("executing run failed", err))?;
    info!(
        run_id = %run.run_id,
        session_id = %run.session_id,
        status = %run.status,
        "run execution finished"
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateRunResponse {
            run: to_run_response(run),
        }),
    ))
}

async fn resume_run(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "run.resume",
        &format!("run:{run_id}"),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "run")?;
    let run = state
        .storage
        .get_run(&run_id)
        .map_err(|err| internal_err_with_error("loading run failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "run not found"))?;
    if run.status == "running" {
        return Err(api_error(
            StatusCode::CONFLICT,
            "run is currently running and cannot be resumed",
        ));
    }
    if run.status == "succeeded" {
        return Err(api_error(
            StatusCode::CONFLICT,
            "run already succeeded and cannot be resumed",
        ));
    }

    let session = state
        .storage
        .get_session(&run.session_id)
        .map_err(|err| internal_err_with_error("loading session for run failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "session not found for run"))?;
    let refreshed = execute_run_with_status_handling(&state, &run, &session.agent_id, None)
        .await
        .map_err(|err| internal_err_with_error("resuming run failed", err))?;
    record_security_audit(
        &headers,
        &state,
        &auth,
        "run.resume",
        &format!("run:{}", refreshed.run_id),
        "allow",
        Some("run resumed".to_string()),
        StatusCode::OK,
        None,
        Some(&refreshed.session_id),
        Some(&refreshed.run_id),
        Some(serde_json::json!({
            "status": &refreshed.status
        })),
    );
    Ok((
        StatusCode::OK,
        Json(CreateRunResponse {
            run: to_run_response(refreshed),
        }),
    ))
}

async fn list_approvals(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListApprovalsQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY],
        "approval.list",
        "approvals",
    )?;
    let status = query.status.as_ref().and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(value.as_str())
        }
    });
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let items: Vec<ApprovalResponse> = state
        .storage
        .list_approvals(status, limit)
        .map_err(|err| internal_err_with_error("listing approvals failed", err))?
        .into_iter()
        .map(to_approval_response)
        .collect();
    debug!(
        status_filter = status.unwrap_or("all"),
        limit,
        count = items.len(),
        "approvals listed"
    );

    Ok(Json(ListApprovalsResponse { items }))
}

async fn create_approval_request(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<CreateApprovalRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_AUTOMATION_RUNNER],
        "approval.create",
        "approvals",
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "approval")?;
    require_operator_allowlist(&headers, &state)?;
    if request.run_id.trim().is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "run_id cannot be empty"));
    }
    if request.tool_name.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "tool_name cannot be empty",
        ));
    }
    if request.request_summary.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "request_summary cannot be empty",
        ));
    }

    let request_json = request
        .request_json
        .unwrap_or_else(|| serde_json::json!({}))
        .to_string();

    let approval = state
        .storage
        .create_approval(NewApproval {
            run_id: request.run_id.clone(),
            tool_call_id: None,
            kind: request.tool_name,
            request_summary: request.request_summary,
            request_json,
        })
        .map_err(|err| internal_err_with_error("creating approval failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "run not found"))?;
    info!(
        approval_id = %approval.approval_id,
        run_id = %approval.run_id,
        kind = %approval.kind,
        "approval requested"
    );
    record_security_audit(
        &headers,
        &state,
        &auth,
        "approval.create",
        &format!("approval:{}", approval.approval_id),
        "allow",
        Some("approval created".to_string()),
        StatusCode::CREATED,
        None,
        None,
        Some(&approval.run_id),
        Some(serde_json::json!({
            "kind": &approval.kind,
            "status": &approval.status
        })),
    );

    emit_event(
        &state,
        "approval.requested",
        serde_json::json!({
            "approval_id": &approval.approval_id,
            "run_id": &approval.run_id,
            "status": &approval.status,
            "kind": &approval.kind
        }),
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateApprovalResponse {
            approval: to_approval_response(approval),
        }),
    ))
}

async fn resolve_approval(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
    Json(request): Json<ResolveApprovalRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_CHANNEL_ADAPTER],
        "approval.resolve",
        &format!("approval:{approval_id}"),
    )?;
    require_endpoint_rate_limit_with_error(&state, &auth, "approval")?;
    require_operator_allowlist(&headers, &state)?;
    if request.decision != "approve" && request.decision != "deny" {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "decision must be 'approve' or 'deny'",
        ));
    }

    let existing = state
        .storage
        .get_approval(&approval_id)
        .map_err(|err| internal_err_with_error("loading approval before resolve failed", err))?;

    if let Some(existing_record) = existing.as_ref() {
        if existing_record.kind == NUMQUAM_APPROVAL_KIND_WRITEBACK
            && existing_record.status == "requested"
        {
            let client = state.numquam_client.as_ref().ok_or_else(|| {
                api_error(
                    StatusCode::FAILED_DEPENDENCY,
                    "numquam integration is disabled for memory writeback resolve",
                )
            })?;
            let run = state
                .storage
                .get_run(&existing_record.run_id)
                .map_err(|err| {
                    internal_err_with_error("loading run for memory writeback resolve failed", err)
                })?
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "run not found for approval"))?;
            let request_json: serde_json::Value =
                serde_json::from_str(&existing_record.request_json).map_err(|_| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        "memory writeback approval payload is invalid",
                    )
                })?;
            let proposal_id = request_json
                .get("proposal_id")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        "memory writeback approval is missing proposal_id",
                    )
                })?;
            let upstream_decision = if request.decision == "approve" {
                "approve"
            } else {
                "reject"
            };
            let decided_by = request
                .decided_by_peer_id
                .clone()
                .or_else(|| {
                    headers
                        .get("x-operator-id")
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                })
                .unwrap_or_else(|| "carsinos_operator".to_string());
            let resolve_envelope = client
                .writeback_resolve(
                    &run.session_id,
                    &run.run_id,
                    &proposal_id,
                    upstream_decision,
                    &decided_by,
                    Some("carsinos_approval_resolution"),
                )
                .await
                .map_err(|err| {
                    error!(
                        approval_id = %approval_id,
                        run_id = %run.run_id,
                        error = %err,
                        "Numquam writeback resolve request failed"
                    );
                    api_error(StatusCode::BAD_GATEWAY, "numquam writeback resolve failed")
                })?;
            if !resolve_envelope.ok {
                let message = resolve_envelope
                    .error
                    .as_ref()
                    .map(|error| format!("numquam writeback resolve failed: {}", error.message))
                    .unwrap_or_else(|| {
                        "numquam writeback resolve returned non-ok response".to_string()
                    });
                return Err(api_error(
                    numquam_status_from_error_code(
                        resolve_envelope
                            .error
                            .as_ref()
                            .map(|error| error.code.as_str())
                            .unwrap_or("INTERNAL_ERROR"),
                    ),
                    &message,
                ));
            }
        }
    }

    let resolved = state
        .storage
        .resolve_approval(
            &approval_id,
            &request.decision,
            request.decided_via.clone(),
            request.decided_by_peer_id.clone(),
        )
        .map_err(|err| internal_err_with_error("resolving approval failed", err))?;

    match resolved {
        ApprovalResolveResult::Resolved(record) => {
            info!(
                approval_id = %record.approval_id,
                run_id = %record.run_id,
                status = %record.status,
                decided_via = ?record.decided_via,
                "approval resolved"
            );
            record_security_audit(
                &headers,
                &state,
                &auth,
                "approval.resolve",
                &format!("approval:{}", record.approval_id),
                "allow",
                Some("approval resolved".to_string()),
                StatusCode::OK,
                None,
                None,
                Some(&record.run_id),
                Some(serde_json::json!({
                    "status": &record.status,
                    "kind": &record.kind,
                    "decision": &request.decision
                })),
            );
            emit_event(
                &state,
                "approval.resolved",
                serde_json::json!({
                    "approval_id": &record.approval_id,
                    "run_id": &record.run_id,
                    "status": &record.status,
                    "kind": &record.kind
                }),
            );
            Ok(Json(ResolveApprovalResponse {
                approval: to_approval_response(record),
            }))
        }
        ApprovalResolveResult::AlreadyResolved(record) => {
            warn!(
                approval_id = %record.approval_id,
                status = %record.status,
                "approval resolution conflict: already resolved"
            );
            record_security_audit(
                &headers,
                &state,
                &auth,
                "approval.resolve",
                &format!("approval:{}", record.approval_id),
                "deny",
                Some(format!(
                    "approval already resolved with status {}",
                    record.status
                )),
                StatusCode::CONFLICT,
                Some("POLICY_DENY"),
                None,
                Some(&record.run_id),
                Some(serde_json::json!({
                    "status": &record.status,
                    "kind": &record.kind,
                    "decision": &request.decision
                })),
            );
            Err(api_error(
                StatusCode::CONFLICT,
                &format!("approval already resolved with status {}", record.status),
            ))
        }
        ApprovalResolveResult::NotFound => {
            Err(api_error(StatusCode::NOT_FOUND, "approval not found"))
        }
    }
}

async fn list_security_audit(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListSecurityAuditQuery>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY],
        "security.audit.list",
        "security_audit_events",
    )?;
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let action = query
        .action
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let principal = query
        .principal
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let decision = query
        .decision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let error_code = query
        .error_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let created_after = query.created_after;
    let created_before = query.created_before;
    if let (Some(after), Some(before)) = (created_after, created_before) {
        if after > before {
            return Err(api_error_with_code(
                StatusCode::BAD_REQUEST,
                "INVALID_INPUT",
                "created_after must be <= created_before",
            ));
        }
    }
    let filter = SecurityAuditEventListFilter {
        action: action.map(str::to_string),
        principal: principal.map(str::to_string),
        decision: decision.map(str::to_string),
        status: status.map(str::to_string),
        error_code: error_code.map(str::to_string),
        created_after,
        created_before,
    };
    let items = state
        .storage
        .list_security_audit_events(limit, &filter)
        .map_err(|err| internal_err_with_error("listing security audit events failed", err))?
        .into_iter()
        .map(to_security_audit_event_response)
        .collect();
    Ok(Json(ListSecurityAuditResponse { items }))
}

async fn run_security_audit_retention(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<RunSecurityAuditRetentionRequest>,
) -> std::result::Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[ROLE_OPERATOR_ADMIN, ROLE_SERVICE_INTERNAL],
        "security.audit.retention.run",
        "security_audit_events",
    )?;

    let hot_retention_days = request
        .hot_retention_days
        .unwrap_or_else(|| i64_env("CARSINOS_SECURITY_AUDIT_HOT_RETENTION_DAYS", 90));
    if !(0..=3650).contains(&hot_retention_days) {
        return Err(api_error_with_code(
            StatusCode::BAD_REQUEST,
            "INVALID_INPUT",
            "hot_retention_days must be between 0 and 3650",
        ));
    }

    let dry_run = request.dry_run.unwrap_or(false);
    let cutoff_ms = current_time_ms().saturating_sub(hot_retention_days.saturating_mul(86_400_000));
    let candidate_count = state
        .storage
        .count_security_audit_events_before(cutoff_ms)
        .map_err(|err| {
            internal_err_with_error("counting security audit retention candidates failed", err)
        })?;

    let (archived_count, deleted_count) = if dry_run {
        (0, 0)
    } else {
        let archived = state
            .storage
            .archive_security_audit_events_before(cutoff_ms)
            .map_err(|err| {
                internal_err_with_error("archiving security audit retention candidates failed", err)
            })?;
        let deleted = state
            .storage
            .delete_security_audit_events_before(cutoff_ms)
            .map_err(|err| {
                internal_err_with_error("deleting security audit retention candidates failed", err)
            })?;
        (archived, deleted)
    };

    record_security_audit(
        &headers,
        &state,
        &auth,
        "security.audit.retention.run",
        "security_audit_events",
        "allow",
        None,
        StatusCode::OK,
        None,
        None,
        None,
        Some(serde_json::json!({
            "hot_retention_days": hot_retention_days,
            "cutoff_ms": cutoff_ms,
            "candidate_count": candidate_count,
            "archived_count": archived_count,
            "deleted_count": deleted_count,
            "dry_run": dry_run
        })),
    );

    Ok(Json(RunSecurityAuditRetentionResponse {
        hot_retention_days,
        cutoff_ms,
        candidate_count,
        archived_count,
        deleted_count,
        dry_run,
    }))
}

async fn execute_run(
    state: &AppState,
    run: &RunRecord,
    agent_id: &str,
    requested_auth_profile_id: Option<&str>,
) -> AnyResult<()> {
    let started = Instant::now();
    info!(
        run_id = %run.run_id,
        session_id = %run.session_id,
        model_provider = %run.model_provider,
        model_id = %run.model_id,
        "run execution started"
    );
    state
        .storage
        .mark_run_started(&run.run_id)
        .with_context(|| format!("failed to mark run {} started", run.run_id))?;
    emit_event(
        state,
        "run.status",
        serde_json::json!({
            "run_id": &run.run_id,
            "session_id": &run.session_id,
            "status": "running"
        }),
    );

    let input = state
        .storage
        .latest_user_message_text(&run.session_id)?
        .unwrap_or_default();

    let tool_requests = parse_tool_requests_from_input(&input)?;
    let mut tool_output_blocks = Vec::new();
    for request in tool_requests {
        let tool_name = tool_request_name(&request);
        let args_json = serde_json::to_string(&request).unwrap_or_else(|_| "{}".to_string());
        let tool_call = state
            .storage
            .create_tool_call(&run.run_id, tool_name, args_json.clone())?
            .with_context(|| format!("failed to create tool call for {}", tool_name))?;

        if tool_requires_approval(&request) && !bool_env("CARSINOS_AUTO_APPROVE_TOOLS", false) {
            let mut approved_by_existing = false;
            if let Some(existing) = state.storage.find_latest_approval_for_request(
                &run.run_id,
                tool_name,
                &args_json,
            )? {
                match existing.status.as_str() {
                    "approved" => {
                        approved_by_existing = true;
                    }
                    "denied" => {
                        let _ = state.storage.finish_tool_call(
                            &tool_call.tool_call_id,
                            "denied",
                            None,
                            Some(format!("approval denied: {}", existing.approval_id)),
                        );
                        anyhow::bail!(
                            "approval denied for tool {}; approval_id={}",
                            tool_name,
                            existing.approval_id
                        );
                    }
                    "requested" => {
                        let _ = state.storage.finish_tool_call(
                            &tool_call.tool_call_id,
                            "blocked",
                            None,
                            Some(format!("approval pending: {}", existing.approval_id)),
                        );
                        anyhow::bail!(
                            "approval pending for tool {}; approval_id={}",
                            tool_name,
                            existing.approval_id
                        );
                    }
                    _ => {}
                }
            }

            if !approved_by_existing {
                let summary = format!("Approval required for tool '{}'", tool_name);
                let approval = state
                    .storage
                    .create_approval(NewApproval {
                        run_id: run.run_id.clone(),
                        tool_call_id: Some(tool_call.tool_call_id.clone()),
                        kind: tool_name.to_string(),
                        request_summary: summary.clone(),
                        request_json: args_json.clone(),
                    })?
                    .with_context(|| format!("failed to request approval for {}", tool_name))?;
                emit_event(
                    state,
                    "approval.requested",
                    serde_json::json!({
                        "approval_id": &approval.approval_id,
                        "run_id": &approval.run_id,
                        "status": &approval.status,
                        "kind": &approval.kind
                    }),
                );
                let _ = state.storage.finish_tool_call(
                    &tool_call.tool_call_id,
                    "blocked",
                    None,
                    Some(format!("approval required: {}", approval.approval_id)),
                );
                anyhow::bail!(
                    "approval required for tool {}; approval_id={}",
                    tool_name,
                    approval.approval_id
                );
            }
        }

        let runner = state.tool_runner.clone();
        let request_for_runner = request.clone();
        let tool_result = tokio::task::spawn_blocking(move || runner.run(request_for_runner))
            .await
            .map_err(|join_err| anyhow::anyhow!("tool runner join error: {join_err}"))?
            .map_err(|tool_err| anyhow::anyhow!("tool execution failed: {tool_err}"))?;

        let tool_output_json = tool_result.output.to_string();
        let _ = state.storage.finish_tool_call(
            &tool_call.tool_call_id,
            "succeeded",
            Some(tool_output_json.clone()),
            None,
        )?;
        let tool_summary = if tool_output_json.len() > 400 {
            format!("{}...", &tool_output_json[..400])
        } else {
            tool_output_json.clone()
        };
        emit_event(
            state,
            "run.delta",
            serde_json::json!({
                "run_id": &run.run_id,
                "session_id": &run.session_id,
                "delta": format!("[tool:{}] {}", tool_name, tool_summary)
            }),
        );
        tool_output_blocks.push(format!("[{}] {}", tool_name, tool_output_json));
    }

    let mut memory_metadata = RunMemoryMetadata::default();
    let mut memory_context_text = None;
    if let Some(client) = state.numquam_client.as_ref() {
        memory_metadata.enabled = true;
        memory_metadata.transport = Some(client.transport.as_str().to_string());
        match client
            .context_build(&run.session_id, &run.run_id, &input)
            .await
        {
            Ok(envelope) => {
                memory_metadata.context_request_id = Some(envelope.request_id.clone());
                memory_metadata.context_request_id_source =
                    Some(envelope.request_id_source.clone());
                memory_metadata.context_degrade_mode = envelope.degrade_mode;
                memory_metadata.context_fallback_recommendation =
                    envelope.fallback_recommendation.clone();
                memory_metadata.context_warning_codes = numquam_warning_codes(&envelope.warnings);
                if envelope.ok {
                    if envelope.degrade_mode {
                        warn!(
                            run_id = %run.run_id,
                            session_id = %run.session_id,
                            operation = %envelope.operation,
                            "Numquam context is in degrade mode; using stateless provider input"
                        );
                    } else if let Some(data) = envelope.data {
                        let context_text = data.context_text.trim().to_string();
                        memory_metadata.route = data.route;
                        memory_metadata.confidence = data.confidence;
                        memory_metadata.context_chars = context_text.len();
                        memory_metadata.evidence = data
                            .evidence
                            .into_iter()
                            .map(|row| RunMemoryEvidence {
                                evidence_id: row.evidence_id.clone(),
                                provenance_handle: if row.evidence_id.trim().is_empty() {
                                    "evidence:unknown".to_string()
                                } else {
                                    format!("evidence:{}", row.evidence_id)
                                },
                                citation_refs: row.citations,
                                confidence: row.confidence,
                                conflict_flag: is_conflict_evidence(&row.kind, &row.section),
                            })
                            .collect();
                        if !context_text.is_empty() {
                            memory_context_text = Some(context_text);
                        }
                    }
                } else if let Some(error) = envelope.error {
                    memory_metadata.context_error_code = Some(error.code.clone());
                    memory_metadata.context_error_message = Some(error.message.clone());
                    warn!(
                        run_id = %run.run_id,
                        session_id = %run.session_id,
                        error_code = %error.code,
                        error_message = %error.message,
                        retryable = error.retryable.unwrap_or(false),
                        operator_action = error.operator_action.unwrap_or_default(),
                        "Numquam context request returned non-ok envelope; using stateless provider input"
                    );
                }
            }
            Err(err) => {
                memory_metadata.context_error_message = Some(err.to_string());
                warn!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    error = %err,
                    "Numquam context request failed; using stateless provider input"
                );
            }
        }
    }

    let provider_base_input = if tool_output_blocks.is_empty() {
        input.clone()
    } else {
        format!(
            "{}\n\nTool outputs:\n{}",
            input,
            tool_output_blocks.join("\n")
        )
    };
    let (local_memory_context, local_memory_metadata) =
        match retrieve_local_memory_context(state, &input) {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    error = %err,
                    "local memory retrieval failed; continuing without local memory context"
                );
                let fallback = LocalMemoryMetadata {
                    enabled: bool_env("CARSINOS_LOCAL_MEMORY_ENABLED", true),
                    query_chars: input.len(),
                    error: Some(err.to_string()),
                    ..Default::default()
                };
                (None, fallback)
            }
        };

    let mut context_sections = Vec::new();
    if let Some(context_text) = memory_context_text {
        context_sections.push(format!("Numquam memory context:\n{}", context_text));
    }
    if let Some(local_context) = local_memory_context {
        context_sections.push(format!("Local notes context:\n{}", local_context));
    }
    let provider_input = if context_sections.is_empty() {
        provider_base_input
    } else {
        format!(
            "{}\n\nUser request:\n{}",
            context_sections.join("\n\n"),
            provider_base_input
        )
    };

    let auth_candidates = resolve_run_auth_profiles(
        state,
        &run.model_provider,
        agent_id,
        requested_auth_profile_id,
    )
    .await?;
    let candidate_count = if auth_candidates.is_empty() {
        1
    } else {
        auth_candidates.len()
    };

    let mut completion = None;
    let mut last_error = None;

    for (attempt_index, auth_record) in auth_candidates.into_iter().enumerate() {
        let auth_profile = match auth_record.as_ref() {
            Some(record) => match to_provider_auth_profile(state, record) {
                Ok(profile) => Some(profile),
                Err(err) => {
                    warn!(
                        run_id = %run.run_id,
                        session_id = %run.session_id,
                        provider = %run.model_provider,
                        auth_profile_id = %record.auth_profile_id,
                        auth_mode = %record.auth_mode,
                        error = %err,
                        "skipping auth profile because credentials could not be hydrated"
                    );
                    last_error = Some(err);
                    if attempt_index + 1 >= candidate_count {
                        break;
                    }
                    continue;
                }
            },
            None => None,
        };
        if let Some(auth_record) = auth_record.as_ref() {
            let policy = auth_mode_policy(&auth_record.auth_mode).with_context(|| {
                format!(
                    "unsupported auth_mode in profile: {}",
                    auth_record.auth_mode
                )
            })?;
            if policy.requires_warning {
                warn!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    provider = %run.model_provider,
                    auth_profile_id = %auth_record.auth_profile_id,
                    auth_mode = %auth_record.auth_mode,
                    risk_level = %auth_record.risk_level,
                    risk_notes = policy.risk_notes,
                    "high-risk auth profile selected for run"
                );
            }
            info!(
                run_id = %run.run_id,
                session_id = %run.session_id,
                attempt = attempt_index + 1,
                attempts_total = candidate_count,
                provider = %run.model_provider,
                model_id = %run.model_id,
                auth_profile_id = %auth_record.auth_profile_id,
                auth_mode = %auth_record.auth_mode,
                risk_level = %auth_record.risk_level,
                "run auth path selected"
            );
        } else {
            info!(
                run_id = %run.run_id,
                session_id = %run.session_id,
                attempt = attempt_index + 1,
                attempts_total = candidate_count,
                provider = %run.model_provider,
                model_id = %run.model_id,
                auth_mode = "none",
                risk_level = "none",
                "run auth path selected"
            );
        }

        let provider_started = Instant::now();
        let result = state
            .providers
            .complete(CompletionRequest {
                model_provider: run.model_provider.clone(),
                model_id: run.model_id.clone(),
                input: provider_input.clone(),
                auth_profile: auth_profile.clone(),
            })
            .await;

        match result {
            Ok(response) => {
                info!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    provider = %run.model_provider,
                    model_id = %run.model_id,
                    attempt = attempt_index + 1,
                    attempts_total = candidate_count,
                    auth_mode = auth_profile
                        .as_ref()
                        .map(|profile| profile.auth_mode.as_str())
                        .unwrap_or("none"),
                    latency_ms = provider_started.elapsed().as_millis() as u64,
                    "provider completion succeeded"
                );
                completion = Some(response);
                break;
            }
            Err(err) => {
                let error_text = err.to_string();
                let error_class = parse_provider_error_class_normalized(&error_text);
                warn!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    provider = %run.model_provider,
                    model_id = %run.model_id,
                    attempt = attempt_index + 1,
                    attempts_total = candidate_count,
                    auth_mode = auth_profile
                        .as_ref()
                        .map(|profile| profile.auth_mode.as_str())
                        .unwrap_or("none"),
                    error_class = error_class.as_code(),
                    latency_ms = provider_started.elapsed().as_millis() as u64,
                    error = %error_text,
                    "provider completion failed"
                );
                last_error = Some(err);

                let can_retry = provider_error_retryable_normalized(error_class);
                if !can_retry || attempt_index + 1 >= candidate_count {
                    break;
                }
                warn!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    provider = %run.model_provider,
                    from_attempt = attempt_index + 1,
                    to_attempt = attempt_index + 2,
                    "attempting auth-profile fallback after provider failure"
                );
            }
        }
    }

    let completion = match completion {
        Some(response) => response,
        None => {
            let error = last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "provider completion failed".to_string());
            anyhow::bail!("{error}");
        }
    };

    for delta in completion.deltas {
        debug!(run_id = %run.run_id, delta_chars = delta.len(), "provider delta");
        emit_event(
            state,
            "run.delta",
            serde_json::json!({
                "run_id": &run.run_id,
                "session_id": &run.session_id,
                "delta": delta
            }),
        );
    }

    let output_text = if completion.output_text.trim().is_empty() {
        "Model produced an empty response.".to_string()
    } else {
        completion.output_text
    };
    let output_chars = output_text.len();

    let message = state.storage.create_message(NewMessage {
        session_id: run.session_id.clone(),
        source_channel: "agent".to_string(),
        source_peer_id: None,
        source_message_id: None,
        role: "assistant".to_string(),
        content_text: output_text.clone(),
        content_format: "markdown".to_string(),
    })?;

    if message.is_none() {
        anyhow::bail!("session missing while persisting assistant output");
    }

    if let Some(client) = state.numquam_client.as_ref() {
        let writeback_result = client
            .writeback_propose(
                &run.session_id,
                &run.run_id,
                &input,
                &output_text,
                &memory_metadata.evidence,
            )
            .await;
        let mut writeback_metadata = RunMemoryWritebackMetadata::default();
        match writeback_result {
            Ok(envelope) => {
                writeback_metadata.request_id = Some(envelope.request_id.clone());
                writeback_metadata.request_id_source = Some(envelope.request_id_source.clone());
                writeback_metadata.degrade_mode = envelope.degrade_mode;
                writeback_metadata.fallback_recommendation =
                    envelope.fallback_recommendation.clone();
                writeback_metadata.warning_codes = numquam_warning_codes(&envelope.warnings);

                if envelope.ok {
                    if let Some(data) = envelope.data {
                        writeback_metadata.proposal_id = Some(data.proposal_id.clone());
                        writeback_metadata.status = Some(data.status.clone());
                        writeback_metadata.idempotent_replay = data.idempotent_replay;
                        writeback_metadata.audit_ref = data.audit_ref.clone();
                        if data.status == "pending_review" {
                            let approval_payload = serde_json::json!({
                                "proposal_id": data.proposal_id,
                                "audit_ref": data.audit_ref,
                                "request_id": envelope.request_id,
                                "request_id_source": envelope.request_id_source,
                                "transport": client.transport.as_str()
                            });
                            let request_json = approval_payload.to_string();
                            let existing = state.storage.find_latest_approval_for_request(
                                &run.run_id,
                                NUMQUAM_APPROVAL_KIND_WRITEBACK,
                                &request_json,
                            )?;
                            if existing.is_none() {
                                if let Some(approval) =
                                    state.storage.create_approval(NewApproval {
                                        run_id: run.run_id.clone(),
                                        tool_call_id: None,
                                        kind: NUMQUAM_APPROVAL_KIND_WRITEBACK.to_string(),
                                        request_summary: format!(
                                            "Review Numquam memory writeback proposal {}",
                                            data.proposal_id
                                        ),
                                        request_json: request_json.clone(),
                                    })?
                                {
                                    emit_event(
                                        state,
                                        "approval.requested",
                                        serde_json::json!({
                                            "approval_id": &approval.approval_id,
                                            "run_id": &approval.run_id,
                                            "status": &approval.status,
                                            "kind": &approval.kind
                                        }),
                                    );
                                }
                            }
                        }
                    }
                } else if let Some(error) = envelope.error {
                    writeback_metadata.error_code = Some(error.code.clone());
                    writeback_metadata.error_message = Some(error.message.clone());
                    warn!(
                        run_id = %run.run_id,
                        session_id = %run.session_id,
                        error_code = %error.code,
                        error_message = %error.message,
                        retryable = error.retryable.unwrap_or(false),
                        operator_action = error.operator_action.unwrap_or_default(),
                        "Numquam writeback proposal returned non-ok envelope"
                    );
                }
            }
            Err(err) => {
                writeback_metadata.error_message = Some(err.to_string());
                warn!(
                    run_id = %run.run_id,
                    session_id = %run.session_id,
                    error = %err,
                    "Numquam writeback proposal failed"
                );
            }
        }
        memory_metadata.writeback = Some(writeback_metadata);
    }

    if memory_metadata.enabled
        || local_memory_metadata.enabled
        || local_memory_metadata.error.is_some()
    {
        let mut usage_payload = serde_json::Map::new();
        if memory_metadata.enabled {
            usage_payload.insert("memory".to_string(), serde_json::json!(&memory_metadata));
        }
        usage_payload.insert(
            "local_memory".to_string(),
            serde_json::json!(&local_memory_metadata),
        );
        let usage_json = serde_json::Value::Object(usage_payload).to_string();
        state
            .storage
            .set_run_usage_json(&run.run_id, &usage_json)
            .with_context(|| {
                format!("failed to persist run {} memory usage metadata", run.run_id)
            })?;
    }

    state
        .storage
        .mark_run_succeeded(&run.run_id)
        .with_context(|| format!("failed to mark run {} succeeded", run.run_id))?;
    emit_event(
        state,
        "run.status",
        serde_json::json!({
            "run_id": &run.run_id,
            "session_id": &run.session_id,
            "status": "succeeded"
        }),
    );
    info!(
        run_id = %run.run_id,
        session_id = %run.session_id,
        output_chars,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "run execution succeeded"
    );

    Ok(())
}

async fn execute_run_with_status_handling(
    state: &AppState,
    run: &RunRecord,
    agent_id: &str,
    requested_auth_profile_id: Option<&str>,
) -> AnyResult<RunRecord> {
    state
        .metrics
        .runs_started_total
        .fetch_add(1, Ordering::Relaxed);
    if let Err(err) = execute_run(state, run, agent_id, requested_auth_profile_id).await {
        error!(
            run_id = %run.run_id,
            session_id = %run.session_id,
            error = %err,
            "run execution failed"
        );
        state
            .metrics
            .runs_failed_total
            .fetch_add(1, Ordering::Relaxed);
        state
            .storage
            .mark_run_failed(&run.run_id, &err.to_string())
            .with_context(|| format!("failed to mark run {} failed", run.run_id))?;
        emit_event(
            state,
            "run.status",
            serde_json::json!({
                "run_id": &run.run_id,
                "session_id": &run.session_id,
                "status": "failed",
                "error": err.to_string()
            }),
        );
    }

    let refreshed = state
        .storage
        .get_run(&run.run_id)
        .with_context(|| format!("failed loading run {} after execution", run.run_id))?
        .with_context(|| format!("run {} missing after execution", run.run_id))?;
    if refreshed.status == "succeeded" {
        state
            .metrics
            .runs_succeeded_total
            .fetch_add(1, Ordering::Relaxed);
    }
    Ok(refreshed)
}

impl NumquamClient {
    fn from_env() -> AnyResult<Option<Self>> {
        let base_url_raw = std::env::var("CARSINOS_NUMQUAM_BASE_URL").ok();
        let base_url = base_url_raw
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_end_matches('/').to_string());
        let enabled = bool_env("CARSINOS_NUMQUAM_ENABLED", false) || base_url.is_some();
        if !enabled {
            return Ok(None);
        }

        let integration_base_url = base_url.unwrap_or_else(|| "http://127.0.0.1:7340".to_string());
        let transport_raw = std::env::var("CARSINOS_NUMQUAM_TRANSPORT")
            .unwrap_or_else(|_| "dual".to_string())
            .to_ascii_lowercase();
        let transport = match transport_raw.as_str() {
            "http" => NumquamTransport::Http,
            "mcp" => NumquamTransport::Mcp,
            "dual" => NumquamTransport::Dual,
            other => anyhow::bail!(
                "invalid CARSINOS_NUMQUAM_TRANSPORT value: {other} (expected http|mcp|dual)"
            ),
        };

        let timeout_ms = std::env::var("CARSINOS_NUMQUAM_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(4_000)
            .clamp(200, 30_000);
        let request_timeout = Duration::from_millis(timeout_ms);
        let token = std::env::var("CARSINOS_NUMQUAM_TOKEN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let principal_id = std::env::var("CARSINOS_NUMQUAM_PRINCIPAL_ID")
            .unwrap_or_else(|_| "carsinos_gateway".to_string())
            .trim()
            .to_string();
        let principal_display_name = std::env::var("CARSINOS_NUMQUAM_PRINCIPAL_NAME")
            .unwrap_or_else(|_| "carsinOS Gateway".to_string())
            .trim()
            .to_string();
        let mcp_url = std::env::var("CARSINOS_NUMQUAM_MCP_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{integration_base_url}/mcp"));

        let http_client = reqwest::Client::builder()
            .timeout(request_timeout)
            .build()
            .context("failed building Numquam HTTP client")?;

        Ok(Some(Self {
            transport,
            integration_base_url,
            mcp_url,
            token,
            principal_id,
            principal_display_name,
            request_timeout,
            http_client,
        }))
    }

    async fn context_build(
        &self,
        session_id: &str,
        run_id: &str,
        message: &str,
    ) -> AnyResult<NumquamEnvelope<NumquamContextData>> {
        let request_id = new_numquam_request_id();
        let mut base_arguments = serde_json::Map::new();
        base_arguments.insert(
            "request_id".to_string(),
            serde_json::Value::String(request_id),
        );
        base_arguments.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id.to_string()),
        );
        base_arguments.insert(
            "run_id".to_string(),
            serde_json::Value::String(run_id.to_string()),
        );
        base_arguments.insert(
            "message".to_string(),
            serde_json::Value::String(message.to_string()),
        );
        base_arguments.insert(
            "retrieval".to_string(),
            serde_json::json!({
                "top_k": 8
            }),
        );
        base_arguments.insert(
            "risk_signal".to_string(),
            serde_json::Value::String("low".to_string()),
        );
        base_arguments.insert(
            "principal".to_string(),
            serde_json::json!({
                "principal_id": &self.principal_id,
                "display_name": &self.principal_display_name,
                "role_hint": "operator"
            }),
        );

        match self.transport {
            NumquamTransport::Http => {
                self.context_build_http(serde_json::Value::Object(base_arguments))
                    .await
            }
            NumquamTransport::Mcp => {
                self.context_build_mcp(serde_json::Value::Object(base_arguments))
                    .await
            }
            NumquamTransport::Dual => {
                let base_value = serde_json::Value::Object(base_arguments.clone());
                let http_result = self.context_build_http(base_value.clone()).await;
                let mcp_result = self.context_build_mcp(base_value).await;
                merge_numquam_dual_result("context.build", http_result, mcp_result, |http, mcp| {
                    let left = http
                        .data
                        .as_ref()
                        .map(|value| value.context_text.trim().to_string())
                        .unwrap_or_default();
                    let right = mcp
                        .data
                        .as_ref()
                        .map(|value| value.context_text.trim().to_string())
                        .unwrap_or_default();
                    left == right
                })
            }
        }
    }

    async fn writeback_propose(
        &self,
        session_id: &str,
        run_id: &str,
        input_text: &str,
        output_text: &str,
        evidence: &[RunMemoryEvidence],
    ) -> AnyResult<NumquamEnvelope<NumquamWritebackProposeData>> {
        let request_id = new_numquam_request_id();
        let idempotency_key = format!("carsinos-writeback-{run_id}");
        let evidence_payload = if evidence.is_empty() {
            let fallback_excerpt: String = if output_text.trim().is_empty() {
                input_text.chars().take(320).collect()
            } else {
                output_text.chars().take(320).collect()
            };
            vec![serde_json::json!({
                "provenance_handle": format!("run:{run_id}"),
                "source_kind": "run_output",
                "source_id": run_id,
                "excerpt": fallback_excerpt,
                "citation": {
                    "type": "run",
                    "ref": run_id
                },
                "confidence": 0.50
            })]
        } else {
            evidence
                .iter()
                .map(|row| {
                    let citation_ref = row
                        .citation_refs
                        .first()
                        .cloned()
                        .unwrap_or_else(|| row.evidence_id.clone());
                    serde_json::json!({
                        "provenance_handle": &row.provenance_handle,
                        "source_kind": "numquam_context",
                        "source_id": &row.evidence_id,
                        "excerpt": format!("evidence:{} confidence:{:.3}", row.evidence_id, row.confidence),
                        "citation": {
                            "type": "evidence",
                            "ref": citation_ref
                        },
                        "confidence": row.confidence
                    })
                })
                .collect::<Vec<_>>()
        };
        let canonical_text: String = output_text.chars().take(2_000).collect();
        let mutation = serde_json::json!({
            "intent": "create",
            "target_kind": "assistant_run_summary",
            "body": {
                "canonical_text": canonical_text
            },
            "tags": ["carsinos", "assistant", "run"]
        });
        let payload = serde_json::json!({
            "schema_version": NUMQUAM_SCHEMA_VERSION,
            "request_id": request_id,
            "session_id": session_id,
            "run_id": run_id,
            "principal": {
                "principal_id": &self.principal_id,
                "display_name": &self.principal_display_name,
                "role_hint": "operator"
            },
            "data": {
                "mutation": mutation,
                "evidence": evidence_payload
            }
        });

        match self.transport {
            NumquamTransport::Http => self.writeback_propose_http(payload, &idempotency_key).await,
            NumquamTransport::Mcp => self.writeback_propose_mcp(payload, &idempotency_key).await,
            NumquamTransport::Dual => {
                let http_result = self
                    .writeback_propose_http(payload.clone(), &idempotency_key)
                    .await;
                let mcp_result = self.writeback_propose_mcp(payload, &idempotency_key).await;
                merge_numquam_dual_result(
                    "writeback.propose",
                    http_result,
                    mcp_result,
                    |http, mcp| {
                        let left = http
                            .data
                            .as_ref()
                            .map(|value| value.proposal_id.as_str())
                            .unwrap_or_default();
                        let right = mcp
                            .data
                            .as_ref()
                            .map(|value| value.proposal_id.as_str())
                            .unwrap_or_default();
                        left == right
                    },
                )
            }
        }
    }

    async fn writeback_resolve(
        &self,
        session_id: &str,
        run_id: &str,
        proposal_id: &str,
        decision: &str,
        decided_by: &str,
        reason: Option<&str>,
    ) -> AnyResult<NumquamEnvelope<NumquamWritebackResolveData>> {
        let request_id = new_numquam_request_id();
        let mut data = serde_json::json!({
            "proposal_id": proposal_id,
            "decision": decision,
            "decided_by": decided_by
        });
        if let Some(reason) = reason
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            data["reason"] = serde_json::Value::String(reason.to_string());
        }
        let payload = serde_json::json!({
            "schema_version": NUMQUAM_SCHEMA_VERSION,
            "request_id": request_id,
            "session_id": session_id,
            "run_id": run_id,
            "principal": {
                "principal_id": &self.principal_id,
                "display_name": &self.principal_display_name,
                "role_hint": "operator"
            },
            "data": data
        });

        match self.transport {
            NumquamTransport::Http => self.writeback_resolve_http(payload).await,
            NumquamTransport::Mcp => self.writeback_resolve_mcp(payload).await,
            NumquamTransport::Dual => {
                let http_result = self.writeback_resolve_http(payload.clone()).await;
                let mcp_result = self.writeback_resolve_mcp(payload).await;
                merge_numquam_dual_result(
                    "writeback.resolve",
                    http_result,
                    mcp_result,
                    |http, mcp| {
                        let left = http
                            .data
                            .as_ref()
                            .map(|value| value.status.as_str())
                            .unwrap_or_default();
                        let right = mcp
                            .data
                            .as_ref()
                            .map(|value| value.status.as_str())
                            .unwrap_or_default();
                        left == right
                    },
                )
            }
        }
    }

    async fn context_build_http(
        &self,
        args: serde_json::Value,
    ) -> AnyResult<NumquamEnvelope<NumquamContextData>> {
        let request_id = args
            .get("request_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let session_id = args
            .get("session_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let run_id = args
            .get("run_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let message = args
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let principal = args
            .get("principal")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let payload = serde_json::json!({
            "schema_version": NUMQUAM_SCHEMA_VERSION,
            "request_id": request_id,
            "session_id": session_id,
            "run_id": run_id,
            "principal": principal,
            "data": {
                "message": message,
                "retrieval": args.get("retrieval").cloned().unwrap_or_else(|| serde_json::json!({"top_k": 8})),
                "risk_signal": args.get("risk_signal").cloned().unwrap_or_else(|| serde_json::Value::String("low".to_string()))
            }
        });
        self.post_integration_http("/api/integration/v1/context/build", &payload, None)
            .await
    }

    async fn context_build_mcp(
        &self,
        args: serde_json::Value,
    ) -> AnyResult<NumquamEnvelope<NumquamContextData>> {
        let payload = serde_json::json!({
            "request_id": args.get("request_id").cloned().unwrap_or(serde_json::Value::Null),
            "session_id": args.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
            "run_id": args.get("run_id").cloned().unwrap_or(serde_json::Value::Null),
            "principal": args.get("principal").cloned().unwrap_or_else(|| serde_json::json!({})),
            "message": args.get("message").cloned().unwrap_or(serde_json::Value::String(String::new())),
            "retrieval": args.get("retrieval").cloned().unwrap_or_else(|| serde_json::json!({"top_k": 8})),
            "risk_signal": args.get("risk_signal").cloned().unwrap_or_else(|| serde_json::Value::String("low".to_string()))
        });
        self.post_integration_mcp("integration.context.build", payload)
            .await
    }

    async fn writeback_propose_http(
        &self,
        payload: serde_json::Value,
        idempotency_key: &str,
    ) -> AnyResult<NumquamEnvelope<NumquamWritebackProposeData>> {
        self.post_integration_http(
            "/api/integration/v1/writeback/propose",
            &payload,
            Some(idempotency_key),
        )
        .await
    }

    async fn writeback_propose_mcp(
        &self,
        payload: serde_json::Value,
        idempotency_key: &str,
    ) -> AnyResult<NumquamEnvelope<NumquamWritebackProposeData>> {
        let args = serde_json::json!({
            "request_id": payload.get("request_id").cloned().unwrap_or(serde_json::Value::Null),
            "session_id": payload.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
            "run_id": payload.get("run_id").cloned().unwrap_or(serde_json::Value::Null),
            "principal": payload.get("principal").cloned().unwrap_or_else(|| serde_json::json!({})),
            "idempotency_key": idempotency_key,
            "mutation": payload.get("data").and_then(|value| value.get("mutation")).cloned().unwrap_or_else(|| serde_json::json!({})),
            "evidence": payload.get("data").and_then(|value| value.get("evidence")).cloned().unwrap_or_else(|| serde_json::json!([]))
        });
        self.post_integration_mcp("integration.writeback.propose", args)
            .await
    }

    async fn writeback_resolve_http(
        &self,
        payload: serde_json::Value,
    ) -> AnyResult<NumquamEnvelope<NumquamWritebackResolveData>> {
        self.post_integration_http("/api/integration/v1/writeback/resolve", &payload, None)
            .await
    }

    async fn writeback_resolve_mcp(
        &self,
        payload: serde_json::Value,
    ) -> AnyResult<NumquamEnvelope<NumquamWritebackResolveData>> {
        let args = serde_json::json!({
            "request_id": payload.get("request_id").cloned().unwrap_or(serde_json::Value::Null),
            "session_id": payload.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
            "run_id": payload.get("run_id").cloned().unwrap_or(serde_json::Value::Null),
            "principal": payload.get("principal").cloned().unwrap_or_else(|| serde_json::json!({})),
            "proposal_id": payload.get("data").and_then(|value| value.get("proposal_id")).cloned().unwrap_or(serde_json::Value::Null),
            "decision": payload.get("data").and_then(|value| value.get("decision")).cloned().unwrap_or(serde_json::Value::Null),
            "decided_by": payload.get("data").and_then(|value| value.get("decided_by")).cloned().unwrap_or(serde_json::Value::Null),
            "reason": payload.get("data").and_then(|value| value.get("reason")).cloned().unwrap_or(serde_json::Value::Null)
        });
        self.post_integration_mcp("integration.writeback.resolve", args)
            .await
    }

    async fn post_integration_http<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> AnyResult<NumquamEnvelope<T>> {
        let url = format!("{}{}", self.integration_base_url, path);
        let mut request = self.http_client.post(url).json(payload);
        if let Some(token) = self.token.as_deref() {
            request = request.bearer_auth(token);
        }
        if let Some(idempotency_key) = idempotency_key {
            request = request.header("Idempotency-Key", idempotency_key);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Numquam HTTP request failed for path {path}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed reading Numquam HTTP response body")?;
        let envelope: NumquamEnvelope<T> = serde_json::from_str(&body).with_context(|| {
            let clipped = if body.len() > 300 {
                &body[..300]
            } else {
                &body
            };
            format!(
                "failed parsing Numquam integration envelope (status={} body={})",
                status.as_u16(),
                clipped
            )
        })?;
        if envelope.schema_version != NUMQUAM_SCHEMA_VERSION {
            anyhow::bail!(
                "unexpected Numquam schema_version: {}",
                envelope.schema_version
            );
        }
        Ok(envelope)
    }

    async fn post_integration_mcp<T: DeserializeOwned>(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> AnyResult<NumquamEnvelope<T>> {
        self.ensure_mcp_initialized().await?;
        let payload = serde_json::json!({
            "name": tool_name,
            "arguments": args
        });
        let response = self.mcp_request("tools/call", payload).await?;
        let result = response
            .result
            .with_context(|| format!("missing MCP result for tool {tool_name}"))?;
        let structured = result
            .get("structuredContent")
            .cloned()
            .or_else(|| result.get("structured_content").cloned())
            .with_context(|| format!("missing structuredContent for MCP tool {tool_name}"))?;
        let envelope: NumquamEnvelope<T> = serde_json::from_value(structured)
            .with_context(|| format!("invalid MCP envelope for tool {tool_name}"))?;
        if envelope.schema_version != NUMQUAM_SCHEMA_VERSION {
            anyhow::bail!(
                "unexpected Numquam MCP schema_version: {}",
                envelope.schema_version
            );
        }
        Ok(envelope)
    }

    async fn ensure_mcp_initialized(&self) -> AnyResult<()> {
        let mut params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {}
        });
        if let Some(token) = self.token.as_deref() {
            params["auth_token"] = serde_json::Value::String(token.to_string());
        }
        let response = self.mcp_request("initialize", params).await?;
        if response.result.is_none() {
            anyhow::bail!("Numquam MCP initialize returned no result");
        }
        Ok(())
    }

    async fn mcp_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> AnyResult<NumquamMcpRpcResponse> {
        let request_id = (current_time_ms() as u64).saturating_add(rand_seed_suffix() as u64);
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params
        });

        let mut request = self.http_client.post(&self.mcp_url).json(&payload);
        if let Some(token) = self.token.as_deref() {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("Numquam MCP request failed for method {method}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed reading Numquam MCP body")?;
        if !status.is_success() {
            let clipped = if body.len() > 300 {
                &body[..300]
            } else {
                &body
            };
            anyhow::bail!(
                "Numquam MCP HTTP status {} for method {}: {}",
                status.as_u16(),
                method,
                clipped
            );
        }
        let rpc: NumquamMcpRpcResponse =
            serde_json::from_str(&body).context("failed parsing Numquam MCP JSON-RPC response")?;
        if let Some(error) = rpc.error.as_ref() {
            anyhow::bail!(
                "Numquam MCP error method={} code={} message={} data={}",
                method,
                error.code,
                error.message,
                error
                    .data
                    .as_ref()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "{}".to_string())
            );
        }
        Ok(rpc)
    }
}

fn merge_numquam_dual_result<T>(
    operation: &str,
    http_result: AnyResult<NumquamEnvelope<T>>,
    mcp_result: AnyResult<NumquamEnvelope<T>>,
    parity_check: impl Fn(&NumquamEnvelope<T>, &NumquamEnvelope<T>) -> bool,
) -> AnyResult<NumquamEnvelope<T>> {
    match (http_result, mcp_result) {
        (Ok(http), Ok(mcp)) => {
            if !parity_check(&http, &mcp) {
                warn!(
                    operation,
                    "Numquam dual transport parity mismatch detected; using HTTP envelope"
                );
            }
            Ok(http)
        }
        (Ok(http), Err(err)) => {
            warn!(
                operation,
                error = %err,
                "Numquam MCP transport failed in dual mode; using HTTP envelope"
            );
            Ok(http)
        }
        (Err(err), Ok(mcp)) => {
            warn!(
                operation,
                error = %err,
                "Numquam HTTP transport failed in dual mode; using MCP envelope"
            );
            Ok(mcp)
        }
        (Err(http_err), Err(mcp_err)) => anyhow::bail!(
            "Numquam dual transport failed for {}: http_error={} mcp_error={}",
            operation,
            http_err,
            mcp_err
        ),
    }
}

fn rand_seed_suffix() -> u32 {
    use std::hash::{Hash, Hasher};
    let thread_id = std::thread::current().id();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    thread_id.hash(&mut hasher);
    (hasher.finish() as u32) & 0xFFFF
}

fn new_numquam_request_id() -> String {
    format!("req_{}", uuid::Uuid::new_v4().simple())
}

async fn scheduler_loop(state: AppState) {
    let worker_id = format!("gateway-worker-{}", std::process::id());
    info!(worker_id = %worker_id, "scheduler loop started");
    loop {
        let now_ms = current_time_ms();
        let due_jobs = match state
            .storage
            .acquire_due_jobs(&worker_id, now_ms, 30_000, 8)
        {
            Ok(items) => items,
            Err(err) => {
                error!(error = %err, "scheduler failed to acquire due jobs");
                sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        for job in due_jobs {
            if let Err(err) = execute_job_once(&state, &job, "scheduler").await {
                error!(job_id = %job.job_id, error = %err, "scheduler job execution failed");
                if let Err(clear_err) = state.storage.clear_job_lease(&job.job_id) {
                    error!(job_id = %job.job_id, error = %clear_err, "failed to clear job lease after execution error");
                }
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}

async fn execute_job_once(
    state: &AppState,
    job: &JobRecord,
    trigger_kind: &str,
) -> AnyResult<JobRunRecord> {
    let started = Instant::now();
    let job_run = state
        .storage
        .create_job_run(&job.job_id, trigger_kind, 1)?
        .with_context(|| format!("failed creating job run for {}", job.job_id))?;
    let mut attempt: i64 = 0;

    loop {
        attempt += 1;
        match execute_job_payload(state, job, attempt).await {
            Ok(output_json) => {
                let next_run_at = next_run_after_completion(job);
                let disable_job = job.schedule_kind == "once";
                let finished = state
                    .storage
                    .finish_job_run_success(
                        &job.job_id,
                        &job_run.job_run_id,
                        attempt,
                        output_json,
                        next_run_at,
                        disable_job,
                    )?
                    .with_context(|| format!("missing finished run {}", job_run.job_run_id))?;
                emit_event(
                    state,
                    "job.run",
                    serde_json::json!({
                        "job_id": &job.job_id,
                        "job_run_id": &finished.job_run_id,
                        "status": &finished.status,
                        "attempt": attempt,
                        "trigger_kind": trigger_kind,
                        "elapsed_ms": started.elapsed().as_millis() as u64
                    }),
                );
                return Ok(finished);
            }
            Err(err) => {
                let error_text = err.to_string();
                let retry_budget = job.max_retries.max(0);
                if attempt <= retry_budget {
                    warn!(
                        job_id = %job.job_id,
                        attempt,
                        retry_budget,
                        error = %error_text,
                        "job attempt failed; retrying"
                    );
                    let backoff = job.retry_backoff_ms.max(1) as u64;
                    sleep(Duration::from_millis(backoff)).await;
                    continue;
                }

                let next_run_at = next_run_after_completion(job);
                let finished = state
                    .storage
                    .finish_job_run_failed(
                        &job.job_id,
                        &job_run.job_run_id,
                        attempt,
                        error_text.clone(),
                        next_run_at,
                    )?
                    .with_context(|| format!("missing failed run {}", job_run.job_run_id))?;
                if job.schedule_kind == "once" {
                    let _ = state.storage.update_job(
                        &job.job_id,
                        JobUpdatePatch {
                            name: None,
                            enabled: Some(false),
                            interval_seconds: None,
                            run_at_ms: None,
                            next_run_at: None,
                            payload_json: None,
                            max_retries: None,
                            retry_backoff_ms: None,
                            timeout_ms: None,
                        },
                    );
                }
                emit_event(
                    state,
                    "job.run",
                    serde_json::json!({
                        "job_id": &job.job_id,
                        "job_run_id": &finished.job_run_id,
                        "status": &finished.status,
                        "attempt": attempt,
                        "trigger_kind": trigger_kind,
                        "error": error_text,
                        "elapsed_ms": started.elapsed().as_millis() as u64
                    }),
                );
                return Ok(finished);
            }
        }
    }
}

fn required_job_payload_string(payload: &serde_json::Value, key: &str) -> AnyResult<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow::anyhow!("job payload missing required '{key}' string"))
}

fn optional_job_payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn scheduler_auth_context(job_id: &str, mode: &str) -> AuthContext {
    let mut roles = HashSet::new();
    roles.insert(ROLE_OPERATOR_ADMIN.to_string());
    roles.insert(ROLE_SERVICE_INTERNAL.to_string());
    AuthContext {
        principal_id: format!("scheduler:{job_id}:{mode}"),
        roles,
        auth_method: "scheduler_job",
        token_id: Some(job_id.to_string()),
        session_id: None,
        client_ip: "127.0.0.1".to_string(),
    }
}

fn scheduler_headers(job_id: &str, mode: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let request_id = format!("job.{mode}.{job_id}.{}", uuid::Uuid::new_v4());
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        headers.insert("x-request-id", value);
    }
    headers
}

fn execute_secret_rotate_job(
    state: &AppState,
    job: &JobRecord,
    payload: &serde_json::Value,
) -> AnyResult<String> {
    let auth_profile_id = required_job_payload_string(payload, "auth_profile_id")?;
    let reason = optional_job_payload_string(payload, "reason");
    let auth = scheduler_auth_context(&job.job_id, "secret.rotate_profile");
    let headers = scheduler_headers(&job.job_id, "secret.rotate_profile");
    let outcome = match rotate_auth_profile_secret_outcome(state, &auth_profile_id) {
        Ok(outcome) => outcome,
        Err((status, body)) => {
            let error = body.0;
            record_security_audit(
                &headers,
                state,
                &auth,
                "security.secret.rotate",
                &format!("auth_profile:{auth_profile_id}"),
                "deny",
                Some(error.error.clone()),
                status,
                error.error_code.as_deref(),
                None,
                None,
                Some(serde_json::json!({
                    "job_id": &job.job_id,
                    "mode": "secret.rotate_profile",
                    "trigger_kind": "scheduler"
                })),
            );
            anyhow::bail!("{} {}", status.as_u16(), error.error);
        }
    };

    record_security_audit(
        &headers,
        state,
        &auth,
        "security.secret.rotate",
        &format!("auth_profile:{auth_profile_id}"),
        "allow",
        reason
            .clone()
            .or_else(|| Some("scheduled auth profile secret rotation".to_string())),
        StatusCode::OK,
        None,
        None,
        None,
        Some(serde_json::json!({
            "job_id": &job.job_id,
            "mode": "secret.rotate_profile",
            "trigger_kind": "scheduler",
            "provider": &outcome.profile.provider,
            "auth_mode": &outcome.profile.auth_mode,
            "previous_secret_ref": &outcome.previous_secret_ref,
            "rotated_secret_ref": &outcome.rotated_secret_ref
        })),
    );

    Ok(serde_json::json!({
        "mode": "secret.rotate_profile",
        "job_id": &job.job_id,
        "auth_profile_id": auth_profile_id,
        "previous_secret_ref": outcome.previous_secret_ref,
        "rotated_secret_ref": outcome.rotated_secret_ref,
        "reason": reason,
        "now_ms": current_time_ms()
    })
    .to_string())
}

fn execute_secret_revoke_job(
    state: &AppState,
    job: &JobRecord,
    payload: &serde_json::Value,
) -> AnyResult<String> {
    let auth_profile_id = required_job_payload_string(payload, "auth_profile_id")?;
    let reason = optional_job_payload_string(payload, "reason");
    let remove_secret = payload
        .get("remove_secret")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let disable_profile = payload
        .get("disable_profile")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let kill_switch_scope = optional_job_payload_string(payload, "kill_switch_scope");
    let auth = scheduler_auth_context(&job.job_id, "secret.revoke_profile");
    let headers = scheduler_headers(&job.job_id, "secret.revoke_profile");
    let outcome = match revoke_auth_profile_secret_outcome(
        state,
        &auth_profile_id,
        remove_secret,
        disable_profile,
        kill_switch_scope.clone(),
    ) {
        Ok(outcome) => outcome,
        Err((status, body)) => {
            let error = body.0;
            record_security_audit(
                &headers,
                state,
                &auth,
                "security.secret.revoke",
                &format!("auth_profile:{auth_profile_id}"),
                "deny",
                Some(error.error.clone()),
                status,
                error.error_code.as_deref(),
                None,
                None,
                Some(serde_json::json!({
                    "job_id": &job.job_id,
                    "mode": "secret.revoke_profile",
                    "trigger_kind": "scheduler",
                    "remove_secret": remove_secret,
                    "disable_profile": disable_profile
                })),
            );
            anyhow::bail!("{} {}", status.as_u16(), error.error);
        }
    };

    record_security_audit(
        &headers,
        state,
        &auth,
        "security.secret.revoke",
        &format!("auth_profile:{auth_profile_id}"),
        "allow",
        reason
            .clone()
            .or_else(|| Some("scheduled auth profile revocation".to_string())),
        StatusCode::OK,
        None,
        None,
        None,
        Some(serde_json::json!({
            "job_id": &job.job_id,
            "mode": "secret.revoke_profile",
            "trigger_kind": "scheduler",
            "provider": &outcome.profile.provider,
            "auth_mode": &outcome.profile.auth_mode,
            "enabled": outcome.profile.enabled,
            "kill_switch_scope": &outcome.profile.kill_switch_scope,
            "revoked_secret_ref": &outcome.revoked_secret_ref,
            "remove_secret": remove_secret,
            "disable_profile": disable_profile
        })),
    );

    Ok(serde_json::json!({
        "mode": "secret.revoke_profile",
        "job_id": &job.job_id,
        "auth_profile_id": auth_profile_id,
        "enabled": outcome.profile.enabled,
        "kill_switch_scope": outcome.profile.kill_switch_scope,
        "revoked_secret_ref": outcome.revoked_secret_ref,
        "remove_secret": remove_secret,
        "disable_profile": disable_profile,
        "reason": reason,
        "now_ms": current_time_ms()
    })
    .to_string())
}

async fn execute_session_run_job(
    state: &AppState,
    job: &JobRecord,
    payload: &serde_json::Value,
) -> AnyResult<String> {
    let mode = "session.run";
    let agent_id = optional_job_payload_string(payload, "agent_id").unwrap_or(job.agent_id.clone());
    let session_key = optional_job_payload_string(payload, "session_key")
        .unwrap_or_else(|| format!("scheduler:{}:{}", job.job_id, agent_id));
    let session_title = optional_job_payload_string(payload, "session_title");
    let input = required_job_payload_string(payload, "input")?;
    let model_provider = optional_job_payload_string(payload, "model_provider")
        .unwrap_or_else(|| "mock".to_string());
    let model_id = optional_job_payload_string(payload, "model_id")
        .unwrap_or_else(|| "mock-echo-v1".to_string());
    let auth_profile_id = optional_job_payload_string(payload, "auth_profile_id");

    if !provider_supported(&model_provider) {
        anyhow::bail!(
            "job payload model_provider '{}' is unsupported",
            model_provider
        );
    }

    let auth = scheduler_auth_context(&job.job_id, mode);
    let headers = scheduler_headers(&job.job_id, mode);
    let session = if let Some(existing) = state.storage.get_session_by_key(&session_key)? {
        existing
    } else {
        state.storage.create_session(NewSession {
            session_key: Some(session_key.clone()),
            agent_id: agent_id.clone(),
            title: session_title,
        })?
    };

    let message = state
        .storage
        .create_message(NewMessage {
            session_id: session.session_id.clone(),
            source_channel: "scheduler".to_string(),
            source_peer_id: Some(format!("job:{}", job.job_id)),
            source_message_id: None,
            role: "user".to_string(),
            content_text: input.clone(),
            content_format: "markdown".to_string(),
        })?
        .with_context(|| {
            format!(
                "failed creating scheduler message for session {}",
                session.session_id
            )
        })?;

    let run = state
        .storage
        .create_run(NewRun {
            session_id: session.session_id.clone(),
            model_provider: model_provider.clone(),
            model_id: model_id.clone(),
        })?
        .with_context(|| {
            format!(
                "failed creating scheduler run for session {}",
                session.session_id
            )
        })?;

    let refreshed =
        execute_run_with_status_handling(state, &run, &agent_id, auth_profile_id.as_deref())
            .await?;

    let run_succeeded = refreshed.status == "succeeded";
    record_security_audit(
        &headers,
        state,
        &auth,
        "job.session_run.execute",
        &format!("job:{}", job.job_id),
        if run_succeeded { "allow" } else { "deny" },
        Some(if run_succeeded {
            "scheduled session run completed".to_string()
        } else {
            format!(
                "scheduled session run finished with status {}",
                refreshed.status
            )
        }),
        if run_succeeded {
            StatusCode::OK
        } else {
            StatusCode::FAILED_DEPENDENCY
        },
        if run_succeeded {
            None
        } else {
            Some("DEPENDENCY_UNAVAILABLE")
        },
        Some(&session.session_id),
        Some(&refreshed.run_id),
        Some(serde_json::json!({
            "job_id": &job.job_id,
            "mode": mode,
            "agent_id": &agent_id,
            "session_key": &session_key,
            "model_provider": &model_provider,
            "model_id": &model_id,
            "auth_profile_id": auth_profile_id.as_deref().unwrap_or(""),
            "run_status": &refreshed.status
        })),
    );

    Ok(serde_json::json!({
        "mode": mode,
        "job_id": &job.job_id,
        "agent_id": &agent_id,
        "session_id": &session.session_id,
        "session_key": &session_key,
        "message_id": &message.message_id,
        "run_id": &refreshed.run_id,
        "run_status": &refreshed.status,
        "model_provider": &refreshed.model_provider,
        "model_id": &refreshed.model_id,
        "auth_profile_id": auth_profile_id,
        "now_ms": current_time_ms()
    })
    .to_string())
}

async fn execute_job_payload(state: &AppState, job: &JobRecord, attempt: i64) -> AnyResult<String> {
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_else(|_| {
        serde_json::json!({
            "mode": "noop"
        })
    });
    let mode = payload
        .get("mode")
        .and_then(|value| value.as_str())
        .unwrap_or("noop");

    if mode == "fail" {
        anyhow::bail!("job payload requested failure");
    }
    if let Some(fail_until_attempt) = payload
        .get("fail_until_attempt")
        .and_then(|value| value.as_i64())
    {
        if attempt <= fail_until_attempt {
            anyhow::bail!(
                "job payload configured to fail until attempt {}",
                fail_until_attempt
            );
        }
    }

    if mode == "secret.rotate_profile" {
        return execute_secret_rotate_job(state, job, &payload);
    }
    if mode == "secret.revoke_profile" {
        return execute_secret_revoke_job(state, job, &payload);
    }
    if mode == "session.run" {
        return execute_session_run_job(state, job, &payload).await;
    }

    let output = serde_json::json!({
        "mode": mode,
        "job_id": &job.job_id,
        "attempt": attempt,
        "now_ms": current_time_ms(),
        "message": payload
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("job executed")
    });
    Ok(output.to_string())
}

fn next_run_after_completion(job: &JobRecord) -> Option<i64> {
    match job.schedule_kind.as_str() {
        "interval" => job
            .interval_seconds
            .map(|seconds| current_time_ms().saturating_add(seconds.max(1) * 1000)),
        "once" => None,
        _ => None,
    }
}

fn default_discord_channel_config() -> DiscordChannelConfig {
    DiscordChannelConfig {
        require_mention_in_guild_channels: true,
        allowlisted_user_ids: Vec::new(),
        auto_run_enabled: true,
        default_model_provider: "mock".to_string(),
        default_model_id: "mock-echo-v1".to_string(),
    }
}

fn default_telegram_channel_config() -> TelegramChannelConfig {
    TelegramChannelConfig {
        require_mention_in_groups: true,
        allowlisted_user_ids: Vec::new(),
        auto_run_enabled: true,
        default_model_provider: "mock".to_string(),
        default_model_id: "mock-echo-v1".to_string(),
    }
}

fn load_channel_config(state: &AppState) -> AnyResult<carsinos_protocol::ChannelConfigResponse> {
    let mut discord = default_discord_channel_config();
    let mut telegram = default_telegram_channel_config();
    let mut updated_at = 0_i64;

    if let Some((json, ts)) = state.storage.get_app_kv_json(APP_KV_CHANNELS_DISCORD)? {
        discord = serde_json::from_str(&json)
            .with_context(|| "failed to deserialize discord channel config from storage")?;
        updated_at = updated_at.max(ts);
    }
    if let Some((json, ts)) = state.storage.get_app_kv_json(APP_KV_CHANNELS_TELEGRAM)? {
        telegram = serde_json::from_str(&json)
            .with_context(|| "failed to deserialize telegram channel config from storage")?;
        updated_at = updated_at.max(ts);
    }

    Ok(carsinos_protocol::ChannelConfigResponse {
        discord,
        telegram,
        updated_at,
    })
}

fn parse_tool_requests_from_input(input: &str) -> AnyResult<Vec<ToolRequest>> {
    let mut requests = Vec::new();
    for raw_line in input.lines() {
        let line = raw_line.trim();
        if let Some(command) = line.strip_prefix("tool.exec ") {
            if !command.trim().is_empty() {
                requests.push(ToolRequest::Exec(ExecRequest {
                    command: command.trim().to_string(),
                    workdir: None,
                    env: None,
                    timeout_ms: None,
                }));
            }
            continue;
        }
        if let Some(query) = line.strip_prefix("tool.web_search ") {
            if !query.trim().is_empty() {
                requests.push(ToolRequest::WebSearch(WebSearchRequest {
                    query: query.trim().to_string(),
                    count: Some(5),
                }));
            }
            continue;
        }
        if let Some(url) = line.strip_prefix("tool.web_fetch ") {
            if !url.trim().is_empty() {
                requests.push(ToolRequest::WebFetch(WebFetchRequest {
                    url: url.trim().to_string(),
                }));
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("tool.fs_read ") {
            if !path.trim().is_empty() {
                requests.push(ToolRequest::FsRead(FsReadRequest {
                    path: path.trim().to_string(),
                    max_bytes: None,
                }));
            }
            continue;
        }
        if let Some(raw) = line.strip_prefix("tool.fs_write ") {
            if let Some((path, content)) = raw.split_once('|') {
                let path = path.trim();
                if !path.is_empty() {
                    requests.push(ToolRequest::FsWrite(FsWriteRequest {
                        path: path.to_string(),
                        content: content.to_string(),
                        mode: FsWriteMode::Overwrite,
                    }));
                }
            }
            continue;
        }
        if let Some(raw) = line.strip_prefix("tool.process ") {
            let mut parts = raw.split_whitespace();
            if let Some(action) = parts.next() {
                let session_id = parts.next().map(|value| value.to_string());
                requests.push(ToolRequest::Process(ProcessRequest {
                    action: action.to_string(),
                    session_id,
                }));
            }
        }
    }
    Ok(requests)
}

fn tool_request_name(request: &ToolRequest) -> &'static str {
    match request {
        ToolRequest::Exec(_) => "exec",
        ToolRequest::Process(_) => "process",
        ToolRequest::FsRead(_) => "fs.read",
        ToolRequest::FsWrite(_) => "fs.write",
        ToolRequest::WebSearch(_) => "web.search",
        ToolRequest::WebFetch(_) => "web.fetch",
    }
}

fn tool_requires_approval(request: &ToolRequest) -> bool {
    match request {
        ToolRequest::Exec(_) | ToolRequest::FsWrite(_) => true,
        ToolRequest::Process(args) => {
            let action = args.action.trim().to_ascii_lowercase();
            matches!(action.as_str(), "terminate" | "kill")
        }
        ToolRequest::FsRead(_) | ToolRequest::WebSearch(_) | ToolRequest::WebFetch(_) => false,
    }
}

async fn resolve_run_auth_profiles(
    state: &AppState,
    provider: &str,
    agent_id: &str,
    requested_auth_profile_id: Option<&str>,
) -> AnyResult<Vec<Option<AuthProfileRecord>>> {
    if !provider_requires_auth(provider) {
        return Ok(vec![None]);
    }
    if state.storage.global_kill_switch_active()? {
        anyhow::bail!("AUTH_FORBIDDEN:global kill-switch active");
    }
    if state.storage.provider_kill_switch_active(provider)? {
        anyhow::bail!("AUTH_FORBIDDEN:provider kill-switch active");
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let enabled_profiles = state.storage.list_auth_profiles(Some(provider), false)?;
    if enabled_profiles.is_empty() {
        anyhow::bail!("AUTH_REQUIRED:no enabled auth profile for provider '{provider}'");
    }

    if let Some(profile_id) = requested_auth_profile_id {
        let profile = state
            .storage
            .get_auth_profile(profile_id)?
            .with_context(|| format!("requested auth profile not found: {profile_id}"))?;
        if profile.provider != provider {
            anyhow::bail!(
                "requested auth profile provider mismatch: expected '{}' got '{}'",
                provider,
                profile.provider
            );
        }
        if !profile.enabled {
            anyhow::bail!("requested auth profile is disabled: {profile_id}");
        }
        if profile.kill_switch_scope == KILL_SWITCH_SCOPE_PROFILE {
            anyhow::bail!("requested auth profile is kill-switched at profile scope");
        }
        if !provider_auth_mode_allowed(provider, &profile.auth_mode) {
            anyhow::bail!(
                "requested auth profile mode '{}' not allowed for provider '{}'",
                profile.auth_mode,
                provider
            );
        }
        let policy = auth_mode_policy(&profile.auth_mode)
            .with_context(|| format!("unsupported auth_mode in profile: {}", profile.auth_mode))?;
        if profile.risk_level != policy.risk_level {
            anyhow::bail!(
                "requested auth profile risk_level '{}' mismatches registry '{}'",
                profile.risk_level,
                policy.risk_level
            );
        }
        if policy.requires_kill_switch && profile.kill_switch_scope == KILL_SWITCH_SCOPE_NONE {
            anyhow::bail!("requested high-risk auth profile is missing kill-switch scope");
        }
        let profile = maybe_refresh_expired_auth_profile(state, profile).await?;
        if auth_profile_credentials_expired(&profile)? {
            anyhow::bail!("requested auth profile credentials expired");
        }
        seen.insert(profile.auth_profile_id.clone());
        candidates.push(Some(profile));
    }

    let ordered_profile_ids = state
        .storage
        .list_agent_provider_profile_order(agent_id, provider)?;
    for profile_id in ordered_profile_ids {
        if seen.contains(&profile_id) {
            continue;
        }
        let Some(profile) = state.storage.get_auth_profile(&profile_id)? else {
            continue;
        };
        if !profile.enabled
            || profile.provider != provider
            || profile.kill_switch_scope == KILL_SWITCH_SCOPE_PROFILE
        {
            continue;
        }
        let profile = match maybe_refresh_expired_auth_profile(state, profile).await {
            Ok(profile) => profile,
            Err(err) => {
                warn!(
                    provider = %provider,
                    auth_profile_id = %profile_id,
                    error = %err,
                    "skipping ordered auth profile because refresh failed"
                );
                continue;
            }
        };
        if auth_profile_credentials_expired(&profile)? {
            warn!(
                provider = %provider,
                auth_profile_id = %profile.auth_profile_id,
                auth_mode = %profile.auth_mode,
                "skipping ordered auth profile due to expired credentials"
            );
            continue;
        }
        if !provider_auth_mode_allowed(provider, &profile.auth_mode) {
            warn!(
                provider = %provider,
                auth_profile_id = %profile.auth_profile_id,
                auth_mode = %profile.auth_mode,
                "skipping ordered auth profile due to provider auth-mode policy"
            );
            continue;
        }
        let policy = match auth_mode_policy(&profile.auth_mode) {
            Some(policy) => policy,
            None => {
                warn!(
                    provider = %provider,
                    auth_profile_id = %profile.auth_profile_id,
                    auth_mode = %profile.auth_mode,
                    "skipping ordered auth profile due to unknown auth_mode"
                );
                continue;
            }
        };
        if profile.risk_level != policy.risk_level {
            warn!(
                provider = %provider,
                auth_profile_id = %profile.auth_profile_id,
                auth_mode = %profile.auth_mode,
                profile_risk = %profile.risk_level,
                registry_risk = %policy.risk_level,
                "skipping ordered auth profile due to risk-level mismatch"
            );
            continue;
        }
        if policy.requires_kill_switch && profile.kill_switch_scope == KILL_SWITCH_SCOPE_NONE {
            warn!(
                provider = %provider,
                auth_profile_id = %profile.auth_profile_id,
                auth_mode = %profile.auth_mode,
                "skipping ordered high-risk auth profile missing kill-switch scope"
            );
            continue;
        }
        seen.insert(profile.auth_profile_id.clone());
        candidates.push(Some(profile));
    }

    for profile in enabled_profiles {
        if seen.contains(&profile.auth_profile_id) {
            continue;
        }
        if profile.kill_switch_scope == KILL_SWITCH_SCOPE_PROFILE {
            continue;
        }
        let profile = match maybe_refresh_expired_auth_profile(state, profile).await {
            Ok(profile) => profile,
            Err(_) => {
                continue;
            }
        };
        if auth_profile_credentials_expired(&profile)? {
            continue;
        }
        if !provider_auth_mode_allowed(provider, &profile.auth_mode) {
            continue;
        }
        let Some(policy) = auth_mode_policy(&profile.auth_mode) else {
            continue;
        };
        if profile.risk_level != policy.risk_level {
            continue;
        }
        if policy.requires_kill_switch && profile.kill_switch_scope == KILL_SWITCH_SCOPE_NONE {
            continue;
        }
        seen.insert(profile.auth_profile_id.clone());
        candidates.push(Some(profile));
    }

    if candidates.is_empty() {
        anyhow::bail!("AUTH_REQUIRED:no eligible auth profile for provider '{provider}'");
    }
    Ok(candidates)
}

fn to_provider_auth_profile(
    state: &AppState,
    profile: &AuthProfileRecord,
) -> AnyResult<ProviderAuthProfile> {
    let hydrated = hydrate_auth_profile_credentials(state, profile)?;
    let credentials_json =
        serde_json::to_string(&hydrated).context("failed serializing hydrated credentials")?;
    Ok(ProviderAuthProfile {
        auth_profile_id: Some(profile.auth_profile_id.clone()),
        auth_mode: profile.auth_mode.clone(),
        risk_level: profile.risk_level.clone(),
        api_base_url: profile.api_base_url.clone(),
        credentials_json,
    })
}

async fn maybe_refresh_expired_auth_profile(
    state: &AppState,
    profile: AuthProfileRecord,
) -> AnyResult<AuthProfileRecord> {
    if !auth_profile_credentials_expired(&profile)? {
        return Ok(profile);
    }
    if !matches!(
        profile.auth_mode.as_str(),
        AUTH_MODE_OPENAI_OAUTH | AUTH_MODE_CLAUDE_CONSUMER_OAUTH | AUTH_MODE_AGENT_SDK
    ) {
        return Ok(profile);
    }

    let mut metadata = auth_profile_credentials_payload(&profile)?;
    let hydrated = hydrate_auth_profile_credentials(state, &profile)?;
    let refresh_token = hydrated
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .context("credentials expired and refresh_token is missing")?;
    let refresh_url = metadata
        .get("refresh_url")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            hydrated
                .get("refresh_url")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            profile
                .api_base_url
                .as_ref()
                .map(|base| format!("{}/oauth/token", base.trim_end_matches('/')))
        })
        .context("credentials expired and refresh endpoint metadata is missing")?;

    let mut form_pairs = vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        ("refresh_token".to_string(), refresh_token.clone()),
    ];
    if let Some(client_id) = hydrated.get("client_id").and_then(|value| value.as_str()) {
        let value = client_id.trim();
        if !value.is_empty() {
            form_pairs.push(("client_id".to_string(), value.to_string()));
        }
    }
    if let Some(client_secret) = hydrated
        .get("client_secret")
        .and_then(|value| value.as_str())
    {
        let value = client_secret.trim();
        if !value.is_empty() {
            form_pairs.push(("client_secret".to_string(), value.to_string()));
        }
    }
    if let Some(scope) = hydrated.get("scope").and_then(|value| value.as_str()) {
        let value = scope.trim();
        if !value.is_empty() {
            form_pairs.push(("scope".to_string(), value.to_string()));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("failed to build oauth refresh client")?;
    let response = client
        .post(&refresh_url)
        .form(&form_pairs)
        .send()
        .await
        .with_context(|| {
            format!(
                "oauth refresh HTTP request failed for {}",
                profile.auth_mode
            )
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let body = body.trim();
        let body = if body.len() > 200 { &body[..200] } else { body };
        anyhow::bail!(
            "oauth refresh rejected for auth_profile_id {}: status={} body={}",
            profile.auth_profile_id,
            status.as_u16(),
            body
        );
    }

    let refreshed_payload: serde_json::Value = response
        .json()
        .await
        .context("failed to parse oauth refresh response JSON")?;
    let access_token = refreshed_payload
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .context("oauth refresh response missing access_token")?;
    let next_refresh = refreshed_payload
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(refresh_token);
    let expires_in = refreshed_payload
        .get("expires_in")
        .and_then(|value| value.as_i64())
        .unwrap_or(3600)
        .max(60);
    let expires_at_unix = (current_time_ms() / 1000).saturating_add(expires_in - 30);
    let account_id = extract_account_id_from_jwt(&access_token);

    if let Some(secret_ref) = secret_ref_from_metadata(&metadata) {
        let mut secret_payload = state
            .secret_store
            .get_json(&secret_ref)?
            .unwrap_or_else(|| serde_json::json!({}));
        if !secret_payload.is_object() {
            secret_payload = serde_json::json!({});
        }
        secret_payload["access_token"] = serde_json::Value::String(access_token);
        secret_payload["refresh_token"] = serde_json::Value::String(next_refresh);
        state.secret_store.set_json(&secret_ref, &secret_payload)?;
    } else {
        metadata["access_token"] = serde_json::Value::String(access_token);
        metadata["refresh_token"] = serde_json::Value::String(next_refresh);
    }
    metadata["expires_at_unix"] = serde_json::Value::Number(expires_at_unix.into());
    if let Some(account_id) = account_id {
        metadata["account_id"] = serde_json::Value::String(account_id);
    }

    state
        .storage
        .update_auth_profile_credentials(&profile.auth_profile_id, metadata.to_string())?
        .context("auth profile disappeared during refresh update")?;
    let updated = state
        .storage
        .get_auth_profile(&profile.auth_profile_id)?
        .context("auth profile missing after refresh update")?;

    info!(
        auth_profile_id = %updated.auth_profile_id,
        provider = %updated.provider,
        auth_mode = %updated.auth_mode,
        "oauth credentials refreshed for auth profile"
    );
    Ok(updated)
}

fn auth_profile_credentials_payload(profile: &AuthProfileRecord) -> AnyResult<serde_json::Value> {
    serde_json::from_str(&profile.credentials_json).with_context(|| {
        format!(
            "failed parsing credentials_json for auth_profile_id {}",
            profile.auth_profile_id
        )
    })
}

fn hydrate_auth_profile_credentials(
    state: &AppState,
    profile: &AuthProfileRecord,
) -> AnyResult<serde_json::Value> {
    let mut payload = auth_profile_credentials_payload(profile)?;
    let Some(secret_ref) = secret_ref_from_metadata(&payload) else {
        return Ok(payload);
    };
    let secret_payload = state
        .secret_store
        .get_json(&secret_ref)?
        .with_context(|| format!("missing keychain secret for ref {secret_ref}"))?;
    merge_secret_payload(&mut payload, &secret_payload);
    Ok(payload)
}

fn merge_secret_payload(target: &mut serde_json::Value, secret_payload: &serde_json::Value) {
    let Some(target_obj) = target.as_object_mut() else {
        return;
    };
    let Some(secret_obj) = secret_payload.as_object() else {
        return;
    };
    for key in SECRET_FIELD_NAMES {
        if let Some(value) = secret_obj.get(*key) {
            target_obj.insert((*key).to_string(), value.clone());
        }
    }
}

fn secret_ref_from_metadata(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("secret_ref")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn auth_profile_credentials_expired(profile: &AuthProfileRecord) -> AnyResult<bool> {
    if !matches!(
        profile.auth_mode.as_str(),
        AUTH_MODE_OPENAI_OAUTH | AUTH_MODE_CLAUDE_CONSUMER_OAUTH | AUTH_MODE_AGENT_SDK
    ) {
        return Ok(false);
    }

    let payload: serde_json::Value =
        serde_json::from_str(&profile.credentials_json).with_context(|| {
            format!(
                "failed parsing credentials_json for auth_profile_id {}",
                profile.auth_profile_id
            )
        })?;
    let Some(expiry_ms) = extract_credentials_expiry_ms(&payload) else {
        return Ok(false);
    };
    Ok(current_time_ms() >= expiry_ms)
}

fn extract_credentials_expiry_ms(payload: &serde_json::Value) -> Option<i64> {
    fn parse_i64(value: &serde_json::Value) -> Option<i64> {
        if let Some(number) = value.as_i64() {
            return Some(number);
        }
        value
            .as_str()
            .and_then(|value| value.trim().parse::<i64>().ok())
    }

    if let Some(raw_ms) = payload.get("expires_at_ms").and_then(parse_i64) {
        return Some(raw_ms);
    }
    if let Some(raw_sec) = payload.get("expires_at_unix").and_then(parse_i64) {
        return Some(raw_sec.saturating_mul(1000));
    }
    if let Some(raw_sec) = payload.get("expires_at").and_then(parse_i64) {
        return Some(raw_sec.saturating_mul(1000));
    }
    None
}

fn current_time_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as i64
}

fn normalize_tags(tags: Option<Vec<String>>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in tags.unwrap_or_default() {
        let tag = raw.trim().to_ascii_lowercase();
        if tag.is_empty() {
            continue;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
        if out.len() >= 32 {
            break;
        }
    }
    out
}

fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json)
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_usize_env(name: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn split_text_chunks(text: &str, target_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in text.split('\n') {
        let part = paragraph.trim();
        if part.is_empty() {
            continue;
        }
        let next_len = if current.is_empty() {
            part.len()
        } else {
            current.len() + 1 + part.len()
        };
        if next_len > target_chars && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(part);
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(text.trim().to_string());
    }
    chunks
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .take(32)
        .collect()
}

fn embed_text_locally(text: &str) -> Vec<f32> {
    const DIMS: usize = 96;
    let mut vector = vec![0.0f32; DIMS];
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return vector;
    }
    let bytes = normalized.as_bytes();
    for window in bytes.windows(3) {
        let idx =
            (usize::from(window[0]) * 31 + usize::from(window[1]) * 17 + usize::from(window[2]))
                % DIMS;
        vector[idx] += 1.0;
    }
    for byte in bytes {
        let idx = usize::from(*byte) % DIMS;
        vector[idx] += 0.2;
    }
    let norm = vector
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    if norm > f64::EPSILON {
        for value in &mut vector {
            *value = (f64::from(*value) / norm) as f32;
        }
    }
    vector
}

fn persist_note_embeddings(state: &AppState, note: &NoteRecord) -> AnyResult<()> {
    let chunks = split_text_chunks(&note.body, LOCAL_MEMORY_CHUNK_TARGET_CHARS);
    let encoded_chunks: Vec<(String, Vec<f32>)> = chunks
        .into_iter()
        .map(|chunk| {
            let embedding = embed_text_locally(&chunk);
            (chunk, embedding)
        })
        .collect();
    state.storage.replace_note_embeddings(
        &note.note_id,
        LOCAL_MEMORY_EMBED_MODEL,
        &encoded_chunks,
    )?;
    Ok(())
}

fn retrieve_local_memory_context(
    state: &AppState,
    input: &str,
) -> AnyResult<(Option<String>, LocalMemoryMetadata)> {
    let mut metadata = LocalMemoryMetadata::default();
    metadata.enabled = bool_env("CARSINOS_LOCAL_MEMORY_ENABLED", true);
    metadata.query_chars = input.len();
    metadata.top_k = parse_usize_env(
        "CARSINOS_LOCAL_MEMORY_TOP_K",
        LOCAL_MEMORY_DEFAULT_TOP_K,
        1,
        24,
    );
    metadata.max_candidates = parse_usize_env(
        "CARSINOS_LOCAL_MEMORY_MAX_CANDIDATES",
        LOCAL_MEMORY_DEFAULT_MAX_CANDIDATES,
        metadata.top_k,
        512,
    );
    metadata.max_chars = parse_usize_env(
        "CARSINOS_LOCAL_MEMORY_MAX_CHARS",
        LOCAL_MEMORY_DEFAULT_MAX_CHARS,
        256,
        8000,
    );
    if !metadata.enabled || input.trim().is_empty() {
        return Ok((None, metadata));
    }

    let query_vector = embed_text_locally(input);
    let matches = state.storage.search_note_embeddings(
        &query_vector,
        metadata.top_k as u32,
        metadata.max_candidates as u32,
    )?;
    if matches.is_empty() {
        return Ok((None, metadata));
    }

    let mut context_lines = Vec::new();
    let mut used_chars = 0usize;
    for item in matches {
        let snippet = item.snippet.trim();
        if snippet.is_empty() {
            continue;
        }
        if used_chars.saturating_add(snippet.len()) > metadata.max_chars {
            break;
        }
        used_chars = used_chars.saturating_add(snippet.len());
        context_lines.push(format!(
            "- [{}] score={:.3} {}",
            item.note_title
                .clone()
                .unwrap_or_else(|| item.note_id.clone()),
            item.score,
            snippet
        ));
        metadata.hits.push(LocalMemoryHit {
            note_id: item.note_id,
            title: item.note_title,
            score: item.score,
            snippet_chars: snippet.len(),
            chunk_index: item.chunk_index,
        });
        if metadata.hits.len() >= metadata.top_k {
            break;
        }
    }
    metadata.injected_chars = used_chars;
    metadata.hit_count = metadata.hits.len();
    if context_lines.is_empty() {
        return Ok((None, metadata));
    }
    Ok((Some(context_lines.join("\n")), metadata))
}

fn numquam_warning_codes(warnings: &[NumquamWarning]) -> Vec<String> {
    warnings
        .iter()
        .map(|warning| {
            let _ = warning.message.as_str();
            let _ = warning.started_at_utc.as_deref();
            let _ = warning.scope.as_deref();
            warning.warning_code.trim().to_string()
        })
        .filter(|code| !code.is_empty())
        .take(16)
        .collect()
}

fn is_conflict_evidence(kind: &str, section: &str) -> bool {
    let kind_lower = kind.to_ascii_lowercase();
    let section_lower = section.to_ascii_lowercase();
    kind_lower.contains("conflict")
        || kind_lower.contains("contradiction")
        || section_lower.contains("conflict")
}

fn numquam_status_from_error_code(error_code: &str) -> StatusCode {
    match error_code {
        "INVALID_INPUT" => StatusCode::BAD_REQUEST,
        "AUTH_REQUIRED" => StatusCode::UNAUTHORIZED,
        "AUTH_FORBIDDEN" => StatusCode::FORBIDDEN,
        "RATE_LIMITED" => StatusCode::TOO_MANY_REQUESTS,
        "DEPENDENCY_UNAVAILABLE" => StatusCode::SERVICE_UNAVAILABLE,
        "TIMEOUT" => StatusCode::GATEWAY_TIMEOUT,
        "CONTRACT_VERSION_UNSUPPORTED" => StatusCode::UPGRADE_REQUIRED,
        _ => StatusCode::BAD_GATEWAY,
    }
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit_scope: Option<String>,
}

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.to_string(),
            error_code: None,
            retry_after_seconds: None,
            rate_limit_scope: None,
        }),
    )
}

fn api_error_with_code(
    status: StatusCode,
    error_code: &str,
    message: &str,
) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.to_string(),
            error_code: Some(error_code.to_string()),
            retry_after_seconds: None,
            rate_limit_scope: None,
        }),
    )
}

fn api_error_rate_limited(
    message: &str,
    scope: &str,
    retry_after_seconds: i64,
) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ApiError {
            error: message.to_string(),
            error_code: Some("RATE_LIMITED".to_string()),
            retry_after_seconds: Some(retry_after_seconds.max(1)),
            rate_limit_scope: Some(scope.to_string()),
        }),
    )
}

fn internal_err(message: &str, err: anyhow::Error) -> StatusCode {
    error!(error = %err, "{message}");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn internal_err_with_error(message: &str, err: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error!(error = %err, "{message}");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
}

fn require_bearer_auth_with_error(
    headers: &HeaderMap,
    state: &AppState,
) -> std::result::Result<AuthContext, (StatusCode, Json<ApiError>)> {
    let auth = require_bearer_auth(headers, state).map_err(|err| {
        if err.code == "RATE_LIMITED" {
            api_error_rate_limited(
                &err.message,
                "auth",
                err.retry_after_seconds
                    .unwrap_or(state.rate_limiter.config.window_seconds),
            )
        } else {
            api_error_with_code(err.status, err.code, &err.message)
        }
    })?;
    debug!(
        principal_id = %auth.principal_id,
        auth_method = auth.auth_method,
        token_id = ?auth.token_id,
        session_id = ?auth.session_id,
        client_ip = %auth.client_ip,
        "auth accepted"
    );
    Ok(auth)
}

fn require_roles_with_error(
    auth: &AuthContext,
    allowed_roles: &[&str],
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    require_roles_raw(auth, allowed_roles)
        .map_err(|err| api_error_with_code(err.status, err.code, &err.message))
}

fn require_roles_with_audit(
    headers: &HeaderMap,
    state: &AppState,
    auth: &AuthContext,
    allowed_roles: &[&str],
    action: &str,
    resource: &str,
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    match require_roles_raw(auth, allowed_roles) {
        Ok(()) => {
            record_security_audit(
                headers,
                state,
                auth,
                action,
                resource,
                "allow",
                None,
                StatusCode::OK,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "allowed_roles": allowed_roles,
                    "principal_roles": sorted_roles(&auth.roles),
                    "auth_method": auth.auth_method,
                    "token_id": auth.token_id,
                    "client_ip": auth.client_ip
                })),
            );
            Ok(())
        }
        Err(err) => {
            record_security_audit(
                headers,
                state,
                auth,
                action,
                resource,
                "deny",
                Some(err.message.clone()),
                err.status,
                Some(err.code),
                None,
                None,
                Some(serde_json::json!({
                    "allowed_roles": allowed_roles,
                    "principal_roles": sorted_roles(&auth.roles),
                    "auth_method": auth.auth_method,
                    "token_id": auth.token_id,
                    "client_ip": auth.client_ip
                })),
            );
            Err(api_error_with_code(err.status, err.code, &err.message))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_security_audit(
    headers: &HeaderMap,
    state: &AppState,
    auth: &AuthContext,
    action: &str,
    resource: &str,
    decision: &str,
    reason: Option<String>,
    status: StatusCode,
    error_code: Option<&str>,
    session_id: Option<&str>,
    run_id: Option<&str>,
    metadata: Option<serde_json::Value>,
) {
    let request_id = request_id_from_headers(headers);
    let correlation_id = request_id.clone();
    let metadata_json = metadata
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());
    if let Err(err) = state
        .storage
        .append_security_audit_event(NewSecurityAuditEvent {
            request_id: request_id.clone(),
            correlation_id,
            principal: auth.principal_id.clone(),
            action: action.to_string(),
            resource: resource.to_string(),
            decision: decision.to_string(),
            reason,
            transport: "http".to_string(),
            status: status.as_u16().to_string(),
            error_code: error_code.map(|value| value.to_string()),
            session_id: session_id
                .map(|value| value.to_string())
                .or_else(|| auth.session_id.clone()),
            run_id: run_id.map(|value| value.to_string()),
            metadata_json,
        })
    {
        warn!(
            request_id = %request_id,
            action = %action,
            resource = %resource,
            error = %err,
            "failed to persist security audit event"
        );
    }
}

fn request_id_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

fn sorted_roles(roles: &HashSet<String>) -> Vec<String> {
    let mut out = roles.iter().cloned().collect::<Vec<_>>();
    out.sort();
    out
}

fn require_operator_allowlist(
    headers: &HeaderMap,
    state: &AppState,
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    if state.operator_allowlist.is_empty() {
        return Ok(());
    }

    let operator_id = headers
        .get("x-operator-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            warn!("approval action rejected: operator id missing");
            api_error(StatusCode::FORBIDDEN, "operator id required")
        })?;

    if state
        .operator_allowlist
        .iter()
        .any(|allowed| allowed == &operator_id)
    {
        debug!(operator_id = %operator_id, "operator allowlist accepted");
        Ok(())
    } else {
        warn!(operator_id = %operator_id, "approval action rejected: operator not allowlisted");
        Err(api_error(StatusCode::FORBIDDEN, "operator not allowlisted"))
    }
}

fn validate_role(role: &str) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    let valid = matches!(role, "user" | "assistant" | "tool" | "system");
    if valid {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::BAD_REQUEST,
            "role must be one of: user, assistant, tool, system",
        ))
    }
}

fn validate_kill_switch_scope(
    kill_switch_scope: &str,
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    if matches!(
        kill_switch_scope,
        KILL_SWITCH_SCOPE_NONE
            | KILL_SWITCH_SCOPE_PROFILE
            | KILL_SWITCH_SCOPE_PROVIDER
            | KILL_SWITCH_SCOPE_GLOBAL
    ) {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::BAD_REQUEST,
            "kill_switch_scope must be one of: none, profile, provider, global",
        ))
    }
}

fn validate_high_risk_controls(
    auth_mode: &str,
    enabled: bool,
    kill_switch_scope: &str,
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    let policy = auth_mode_policy(auth_mode).ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "unsupported auth_mode in high-risk control validation",
        )
    })?;
    if policy.requires_kill_switch && enabled && kill_switch_scope == KILL_SWITCH_SCOPE_NONE {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "high-risk auth_mode requires kill_switch_scope != none while enabled",
        ));
    }
    Ok(())
}

fn provider_supported(provider: &str) -> bool {
    matches!(
        provider,
        "mock" | "openai" | "anthropic" | "openrouter" | "ollama" | "vllm" | "unconfigured"
    )
}

fn provider_requires_auth(provider: &str) -> bool {
    matches!(provider, "openai" | "anthropic" | "openrouter")
}

fn provider_auth_mode_allowed(provider: &str, auth_mode: &str) -> bool {
    match provider {
        "openai" => matches!(auth_mode, AUTH_MODE_API_KEY | AUTH_MODE_OPENAI_OAUTH),
        "anthropic" => matches!(
            auth_mode,
            AUTH_MODE_API_KEY | AUTH_MODE_CLAUDE_CONSUMER_OAUTH | AUTH_MODE_AGENT_SDK
        ),
        "openrouter" | "ollama" | "vllm" => matches!(auth_mode, AUTH_MODE_API_KEY),
        "mock" | "unconfigured" => true,
        _ => false,
    }
}

fn auth_mode_policy(auth_mode: &str) -> Option<AuthModePolicy> {
    match auth_mode {
        AUTH_MODE_API_KEY => Some(AuthModePolicy {
            risk_level: "low",
            risk_notes: "standard API key authentication",
            requires_warning: false,
            requires_kill_switch: false,
        }),
        AUTH_MODE_OPENAI_OAUTH => Some(AuthModePolicy {
            risk_level: "medium",
            risk_notes: "refreshable OAuth credentials require storage controls",
            requires_warning: false,
            requires_kill_switch: false,
        }),
        AUTH_MODE_CLAUDE_CONSUMER_OAUTH => Some(AuthModePolicy {
            risk_level: "high",
            risk_notes: "consumer OAuth path carries policy/compliance risk",
            requires_warning: true,
            requires_kill_switch: true,
        }),
        AUTH_MODE_AGENT_SDK => Some(AuthModePolicy {
            risk_level: "high",
            risk_notes: "agent-sdk mediated auth path requires explicit control",
            requires_warning: true,
            requires_kill_switch: true,
        }),
        _ => None,
    }
}

fn compute_initial_next_run_at(
    schedule_kind: &str,
    interval_seconds: Option<i64>,
    run_at_ms: Option<i64>,
    now_ms: i64,
) -> std::result::Result<Option<i64>, (StatusCode, Json<ApiError>)> {
    match schedule_kind {
        "interval" => {
            let interval = interval_seconds.ok_or_else(|| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    "interval schedule requires interval_seconds",
                )
            })?;
            if interval <= 0 {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "interval_seconds must be > 0",
                ));
            }
            Ok(Some(
                run_at_ms.unwrap_or(now_ms.saturating_add(interval * 1000)),
            ))
        }
        "once" => {
            let run_at_ms = run_at_ms.ok_or_else(|| {
                api_error(StatusCode::BAD_REQUEST, "once schedule requires run_at_ms")
            })?;
            Ok(Some(run_at_ms))
        }
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "schedule_kind must be one of: interval, once",
        )),
    }
}

fn compute_updated_next_run_at(
    current: &JobRecord,
    interval_override: Option<i64>,
    run_at_override: Option<i64>,
    now_ms: i64,
) -> std::result::Result<Option<i64>, (StatusCode, Json<ApiError>)> {
    match current.schedule_kind.as_str() {
        "interval" => {
            let interval = interval_override
                .or(current.interval_seconds)
                .ok_or_else(|| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        "interval schedule requires interval_seconds",
                    )
                })?;
            if interval <= 0 {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "interval_seconds must be > 0",
                ));
            }
            let baseline = run_at_override.or(current.next_run_at);
            Ok(Some(
                baseline.unwrap_or(now_ms.saturating_add(interval * 1000)),
            ))
        }
        "once" => Ok(run_at_override
            .or(current.run_at_ms)
            .or(current.next_run_at)),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "unsupported schedule_kind on existing job",
        )),
    }
}

fn to_session_summary(record: SessionRecord) -> SessionSummary {
    SessionSummary {
        session_id: record.session_id,
        session_key: record.session_key,
        agent_id: record.agent_id,
        title: record.title,
        created_at: record.created_at,
        updated_at: record.updated_at,
        closed_at: record.closed_at,
        message_count: record.message_count,
        run_count: record.run_count,
    }
}

fn to_message_response(record: MessageRecord) -> MessageResponse {
    MessageResponse {
        message_id: record.message_id,
        session_id: record.session_id,
        source_channel: record.source_channel,
        source_peer_id: record.source_peer_id,
        source_message_id: record.source_message_id,
        role: record.role,
        content_text: record.content_text,
        content_format: record.content_format,
        created_at: record.created_at,
    }
}

fn to_run_response(record: RunRecord) -> RunResponse {
    RunResponse {
        run_id: record.run_id,
        session_id: record.session_id,
        status: record.status,
        model_provider: record.model_provider,
        model_id: record.model_id,
        started_at: record.started_at,
        ended_at: record.ended_at,
        error_text: record.error_text,
        usage_json: record.usage_json,
        created_at: record.created_at,
    }
}

fn to_approval_response(record: ApprovalRecord) -> ApprovalResponse {
    ApprovalResponse {
        approval_id: record.approval_id,
        run_id: record.run_id,
        tool_call_id: record.tool_call_id,
        kind: record.kind,
        status: record.status,
        request_summary: record.request_summary,
        request_json: record.request_json,
        requested_at: record.requested_at,
        decided_at: record.decided_at,
        decided_via: record.decided_via,
        decided_by_peer_id: record.decided_by_peer_id,
    }
}

fn to_auth_profile_response(record: AuthProfileRecord) -> AuthProfileResponse {
    AuthProfileResponse {
        auth_profile_id: record.auth_profile_id,
        provider: record.provider,
        display_name: record.display_name,
        auth_mode: record.auth_mode,
        risk_level: record.risk_level,
        enabled: record.enabled,
        kill_switch_scope: record.kill_switch_scope,
        api_base_url: record.api_base_url,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn to_note_response(record: NoteRecord) -> NoteResponse {
    NoteResponse {
        note_id: record.note_id,
        title: record.title,
        body: record.body,
        tags: parse_tags_json(&record.tags_json),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn to_job_response(record: JobRecord) -> JobResponse {
    JobResponse {
        job_id: record.job_id,
        agent_id: record.agent_id,
        name: record.name,
        enabled: record.enabled,
        schedule_kind: record.schedule_kind,
        interval_seconds: record.interval_seconds,
        run_at_ms: record.run_at_ms,
        next_run_at: record.next_run_at,
        payload_json: record.payload_json,
        max_retries: record.max_retries,
        retry_backoff_ms: record.retry_backoff_ms,
        timeout_ms: record.timeout_ms,
        last_run_at: record.last_run_at,
        last_error: record.last_error,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn to_job_run_response(record: JobRunRecord) -> JobRunResponse {
    JobRunResponse {
        job_run_id: record.job_run_id,
        job_id: record.job_id,
        trigger_kind: record.trigger_kind,
        status: record.status,
        attempt: record.attempt,
        started_at: record.started_at,
        ended_at: record.ended_at,
        error_text: record.error_text,
        output_json: record.output_json,
        created_at: record.created_at,
    }
}

fn to_security_audit_event_response(
    record: SecurityAuditEventRecord,
) -> SecurityAuditEventResponse {
    SecurityAuditEventResponse {
        event_id: record.event_id,
        request_id: record.request_id,
        correlation_id: record.correlation_id,
        principal: record.principal,
        action: record.action,
        resource: record.resource,
        decision: record.decision,
        reason: record.reason,
        transport: record.transport,
        status: record.status,
        error_code: record.error_code,
        session_id: record.session_id,
        run_id: record.run_id,
        metadata_json: record.metadata_json,
        created_at: record.created_at,
    }
}

fn emit_event(state: &AppState, event_name: &str, data: serde_json::Value) {
    let seq = state.event_seq.fetch_add(1, Ordering::Relaxed);
    let frame = serde_json::json!({
        "v": 1,
        "type": "event",
        "event": event_name,
        "seq": seq,
        "data": data
    });
    let _ = state.event_tx.send(frame.to_string());
}

fn load_operator_allowlist_from_env() -> Vec<String> {
    match std::env::var("CARSINOS_OPERATOR_ALLOWLIST") {
        Ok(value) => value
            .split(',')
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn load_auth_mode_from_env() -> AnyResult<AuthMode> {
    let raw = std::env::var("CARSINOS_AUTH_MODE").unwrap_or_else(|_| "static_bearer".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "static_bearer" | "bearer" | "token" => Ok(AuthMode::StaticBearer),
        "jwt" => Ok(AuthMode::Jwt),
        other => {
            anyhow::bail!("invalid CARSINOS_AUTH_MODE value: {other} (expected static_bearer|jwt)")
        }
    }
}

fn load_jwt_auth_from_env(auth_mode: AuthMode) -> AnyResult<Option<JwtAuthConfig>> {
    if auth_mode != AuthMode::Jwt {
        return Ok(None);
    }
    let issuer = std::env::var("CARSINOS_AUTH_JWT_ISSUER")
        .context("CARSINOS_AUTH_JWT_ISSUER is required in jwt auth mode")?;
    let audience = std::env::var("CARSINOS_AUTH_JWT_AUDIENCE")
        .context("CARSINOS_AUTH_JWT_AUDIENCE is required in jwt auth mode")?;
    let secret = std::env::var("CARSINOS_AUTH_JWT_HS256_SECRET")
        .context("CARSINOS_AUTH_JWT_HS256_SECRET is required in jwt auth mode")?;
    if secret.trim().len() < 16 {
        anyhow::bail!("CARSINOS_AUTH_JWT_HS256_SECRET must be at least 16 characters");
    }
    let max_token_age_seconds =
        i64_env("CARSINOS_AUTH_JWT_MAX_TOKEN_AGE_SECONDS", 86_400).clamp(60, 7 * 24 * 3600);
    let clock_skew_seconds = i64_env("CARSINOS_AUTH_JWT_CLOCK_SKEW_SECONDS", 60).clamp(0, 300);
    let replay_protection_enabled = bool_env("CARSINOS_AUTH_JWT_REPLAY_PROTECTION_ENABLED", true);
    let revoked_jti =
        parse_csv_set(&std::env::var("CARSINOS_AUTH_JWT_REVOKED_JTIS").unwrap_or_default());

    Ok(Some(JwtAuthConfig {
        issuer,
        audience,
        secret,
        max_token_age_seconds,
        clock_skew_seconds,
        replay_protection_enabled,
        revoked_jti,
    }))
}

fn load_trusted_proxy_allowlist_from_env() -> HashSet<String> {
    parse_csv_set(&std::env::var("CARSINOS_TRUSTED_PROXY_ALLOWLIST").unwrap_or_default())
}

fn parse_csv_set(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn enforce_network_exposure_policy(
    config: &GatewayConfig,
    trusted_proxy_headers: bool,
    trusted_proxy_allowlist: &HashSet<String>,
) -> AnyResult<()> {
    let public_bind_allowed = bool_env("CARSINOS_PUBLIC_BIND_ALLOWED", false);
    let edge_tls_terminated = bool_env("CARSINOS_EDGE_TLS_TERMINATED", false);
    let is_loopback = config.bind.ip().is_loopback();

    if !is_loopback && !public_bind_allowed {
        anyhow::bail!(
            "non-loopback bind {} requires CARSINOS_PUBLIC_BIND_ALLOWED=true",
            config.bind
        );
    }

    if public_bind_allowed && !edge_tls_terminated {
        anyhow::bail!(
            "public bind mode requires CARSINOS_EDGE_TLS_TERMINATED=true (TLS termination contract)"
        );
    }

    if trusted_proxy_headers && trusted_proxy_allowlist.is_empty() {
        anyhow::bail!(
            "CARSINOS_TRUST_PROXY_HEADERS=true requires CARSINOS_TRUSTED_PROXY_ALLOWLIST to be non-empty"
        );
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            warn!("failed to listen for ctrl-c signal");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => {
                warn!("failed to listen for terminate signal");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}

fn init_tracing(logs_dir: &FsPath) -> AnyResult<LogGuards> {
    let filter =
        std::env::var("CARSINOS_LOG_FILTER").unwrap_or_else(|_| "info,tower_http=info".to_string());
    let env_filter = EnvFilter::try_new(filter.clone())
        .with_context(|| format!("invalid CARSINOS_LOG_FILTER value: {filter}"))?;
    let format = std::env::var("CARSINOS_LOG_FORMAT")
        .unwrap_or_else(|_| "compact".to_string())
        .to_ascii_lowercase();
    let log_to_stdout = bool_env("CARSINOS_LOG_STDOUT", true);
    let log_to_file = bool_env("CARSINOS_LOG_FILE", true);
    let file_prefix =
        std::env::var("CARSINOS_LOG_FILE_PREFIX").unwrap_or_else(|_| "gateway.log".to_string());

    let mut file_guard = None;
    let mut file_writer = None;
    if log_to_file {
        std::fs::create_dir_all(logs_dir)
            .with_context(|| format!("failed to create logs directory {}", logs_dir.display()))?;
        let appender = tracing_appender::rolling::daily(logs_dir, file_prefix.clone());
        let (non_blocking, guard) = tracing_appender::non_blocking(appender);
        file_writer = Some(non_blocking);
        file_guard = Some(guard);
    }

    let writer: BoxMakeWriter = match (log_to_stdout, file_writer) {
        (true, Some(file_writer)) => BoxMakeWriter::new(std::io::stdout.and(file_writer)),
        (true, None) => BoxMakeWriter::new(std::io::stdout),
        (false, Some(file_writer)) => BoxMakeWriter::new(file_writer),
        (false, None) => BoxMakeWriter::new(std::io::sink),
    };

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(writer)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_thread_names(true);

    match format.as_str() {
        "json" => subscriber
            .json()
            .try_init()
            .map_err(|err| anyhow::anyhow!("failed to initialize json logger: {err}"))?,
        "pretty" => subscriber
            .pretty()
            .try_init()
            .map_err(|err| anyhow::anyhow!("failed to initialize pretty logger: {err}"))?,
        "compact" | "text" => subscriber
            .compact()
            .try_init()
            .map_err(|err| anyhow::anyhow!("failed to initialize compact logger: {err}"))?,
        other => {
            anyhow::bail!(
                "invalid CARSINOS_LOG_FORMAT value: {other} (expected compact|text|pretty|json)"
            )
        }
    }

    info!(
        log_filter = filter,
        log_format = format,
        log_to_stdout,
        log_to_file,
        logs_dir = %logs_dir.display(),
        "tracing initialized"
    );

    Ok(LogGuards {
        _file_guard: file_guard,
    })
}

fn bool_env(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => parse_bool_value(&raw).unwrap_or(default),
        Err(_) => default,
    }
}

fn i64_env(name: &str, default: i64) -> i64 {
    match std::env::var(name) {
        Ok(raw) => raw.trim().parse::<i64>().unwrap_or(default),
        Err(_) => default,
    }
}

fn usize_env(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(raw) => raw.trim().parse::<usize>().unwrap_or(default),
        Err(_) => default,
    }
}

fn parse_bool_value(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::extract::State as AxumState;
    use axum::http::Request;
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tower::util::ServiceExt;

    struct TestContext {
        _temp_dir: TempDir,
        app: Router,
        storage: Storage,
        secret_store: SecretStore,
    }

    fn test_context() -> TestContext {
        build_test_context(
            vec![],
            None,
            AuthMode::StaticBearer,
            None,
            "test-token".to_string(),
            None,
            false,
            HashSet::new(),
        )
    }

    fn test_context_with_allowlist(allowlist: Vec<String>) -> TestContext {
        build_test_context(
            allowlist,
            None,
            AuthMode::StaticBearer,
            None,
            "test-token".to_string(),
            None,
            false,
            HashSet::new(),
        )
    }

    fn test_context_with_numquam(numquam_client: NumquamClient) -> TestContext {
        build_test_context(
            vec![],
            Some(numquam_client),
            AuthMode::StaticBearer,
            None,
            "test-token".to_string(),
            None,
            false,
            HashSet::new(),
        )
    }

    fn test_context_with_jwt(secret: &str, revoked_jti: HashSet<String>) -> TestContext {
        test_context_with_jwt_replay(secret, revoked_jti, false)
    }

    fn test_context_with_jwt_replay(
        secret: &str,
        revoked_jti: HashSet<String>,
        replay_protection_enabled: bool,
    ) -> TestContext {
        build_test_context(
            vec![],
            None,
            AuthMode::Jwt,
            Some(JwtAuthConfig {
                issuer: "carsinos-test-issuer".to_string(),
                audience: "carsinos-test-audience".to_string(),
                secret: secret.to_string(),
                max_token_age_seconds: 86_400,
                clock_skew_seconds: 60,
                replay_protection_enabled,
                revoked_jti,
            }),
            "unused-static-token".to_string(),
            None,
            false,
            HashSet::new(),
        )
    }

    fn test_context_with_rate_limits(config: RequestRateLimitConfig) -> TestContext {
        build_test_context(
            vec![],
            None,
            AuthMode::StaticBearer,
            None,
            "test-token".to_string(),
            Some(config),
            false,
            HashSet::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_test_context(
        allowlist: Vec<String>,
        numquam_client: Option<NumquamClient>,
        auth_mode: AuthMode,
        jwt_auth: Option<JwtAuthConfig>,
        auth_token: String,
        rate_limit_config: Option<RequestRateLimitConfig>,
        trusted_proxy_headers: bool,
        trusted_proxy_allowlist: HashSet<String>,
    ) -> TestContext {
        let temp_dir = TempDir::new().expect("tempdir");
        let paths = AppPaths::from_root(temp_dir.path().to_path_buf());
        carsinos_storage::init(&paths).expect("storage init");
        let (event_tx, _) = broadcast::channel(64);
        let storage = Storage::from_paths(&paths);
        let secret_store = SecretStore::from_env();
        let rate_limiter = RequestRateLimiter {
            config: rate_limit_config.unwrap_or(RequestRateLimitConfig {
                enabled: false,
                window_seconds: 60,
                per_ip_limit: 10_000,
                per_principal_limit: 10_000,
                per_run_endpoint_limit: 10_000,
                per_approval_endpoint_limit: 10_000,
            }),
            counters: StdRwLock::new(HashMap::new()),
        };

        let state = AppState {
            auth_mode,
            auth_token: Arc::new(auth_token),
            jwt_auth: jwt_auth.map(Arc::new),
            jwt_replay_jti: Arc::new(StdRwLock::new(HashMap::new())),
            rate_limiter: Arc::new(rate_limiter),
            trusted_proxy_headers,
            trusted_proxy_allowlist: Arc::new(trusted_proxy_allowlist),
            operator_allowlist: Arc::new(allowlist),
            providers: ProviderRegistry::new(),
            tool_runner: LocalToolRunner::default(),
            secret_store: secret_store.clone(),
            oauth_sessions: Arc::new(RwLock::new(HashMap::new())),
            numquam_client,
            storage: storage.clone(),
            metrics: Arc::new(GatewayMetrics::default()),
            event_tx,
            event_seq: Arc::new(AtomicU64::new(1)),
            started_at: Utc::now(),
            started_instant: Instant::now(),
            db_path: Arc::new(paths.db_path.display().to_string()),
            attachments_path: Arc::new(paths.attachments_dir.display().to_string()),
        };

        TestContext {
            _temp_dir: temp_dir,
            app: build_app(state),
            storage,
            secret_store,
        }
    }

    #[derive(Clone)]
    struct NumquamStubConfig {
        context_text: String,
        context_degrade: bool,
    }

    impl NumquamStubConfig {
        fn healthy() -> Self {
            Self {
                context_text: "stored memory says user likes tea".to_string(),
                context_degrade: false,
            }
        }
    }

    #[derive(Clone)]
    struct NumquamStubState {
        config: NumquamStubConfig,
        resolve_calls: Arc<AtomicU64>,
    }

    struct NumquamStubServer {
        base_url: String,
        resolve_calls: Arc<AtomicU64>,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for NumquamStubServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn spawn_numquam_stub(config: NumquamStubConfig) -> NumquamStubServer {
        let resolve_calls = Arc::new(AtomicU64::new(0));
        let app_state = NumquamStubState {
            config,
            resolve_calls: resolve_calls.clone(),
        };
        let app = Router::new()
            .route(
                "/api/integration/v1/context/build",
                post(numquam_stub_context_build),
            )
            .route(
                "/api/integration/v1/writeback/propose",
                post(numquam_stub_writeback_propose),
            )
            .route(
                "/api/integration/v1/writeback/resolve",
                post(numquam_stub_writeback_resolve),
            )
            .route("/mcp", post(numquam_stub_mcp))
            .with_state(app_state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind numquam stub");
        let addr = listener.local_addr().expect("numquam stub local addr");
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("numquam stub server");
        });
        NumquamStubServer {
            base_url: format!("http://{addr}"),
            resolve_calls,
            task,
        }
    }

    async fn numquam_stub_context_build(
        AxumState(state): AxumState<NumquamStubState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let request_id = payload
            .get("request_id")
            .and_then(|value| value.as_str())
            .unwrap_or("req_stub_context")
            .to_string();
        let warnings = if state.config.context_degrade {
            vec![serde_json::json!({
                "warning_code": "TIMEOUT_RATE_HIGH",
                "message": "degraded in stub",
                "started_at_utc": Utc::now(),
                "scope": "context"
            })]
        } else {
            vec![]
        };
        let mut response = serde_json::json!({
            "schema_version": NUMQUAM_SCHEMA_VERSION,
            "request_id": request_id,
            "request_id_source": "client",
            "operation": "context.build",
            "ok": true,
            "degrade_mode": state.config.context_degrade,
            "warnings": warnings,
            "data": {
                "context_text": state.config.context_text,
                "evidence": [{
                    "evidence_id": "ev_stub_1",
                    "section": "fact",
                    "kind": "fact_card",
                    "summary": "User preference memory",
                    "citations": ["conv#1"],
                    "confidence": 0.88
                }],
                "route": "ltm_light",
                "confidence": 0.88
            }
        });
        if state.config.context_degrade {
            response["fallback_recommendation"] =
                serde_json::Value::String("stateless_chat".to_string());
        }
        Json(response)
    }

    async fn numquam_stub_writeback_propose(
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let request_id = payload
            .get("request_id")
            .and_then(|value| value.as_str())
            .unwrap_or("req_stub_propose")
            .to_string();
        let run_id = payload
            .get("run_id")
            .and_then(|value| value.as_str())
            .unwrap_or("run_stub");
        Json(serde_json::json!({
            "schema_version": NUMQUAM_SCHEMA_VERSION,
            "request_id": request_id,
            "request_id_source": "client",
            "operation": "writeback.propose",
            "ok": true,
            "degrade_mode": false,
            "warnings": [],
            "data": {
                "proposal_id": format!("proposal_{run_id}"),
                "status": "pending_review",
                "idempotent_replay": false,
                "audit_ref": format!("audit_{run_id}")
            }
        }))
    }

    async fn numquam_stub_writeback_resolve(
        AxumState(state): AxumState<NumquamStubState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        state.resolve_calls.fetch_add(1, Ordering::Relaxed);
        let request_id = payload
            .get("request_id")
            .and_then(|value| value.as_str())
            .unwrap_or("req_stub_resolve")
            .to_string();
        let data = payload
            .get("data")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let proposal_id = data
            .get("proposal_id")
            .and_then(|value| value.as_str())
            .unwrap_or("proposal_stub")
            .to_string();
        let decision = data
            .get("decision")
            .and_then(|value| value.as_str())
            .unwrap_or("approve");
        let status = if decision == "approve" {
            "approved"
        } else {
            "rejected"
        };
        Json(serde_json::json!({
            "schema_version": NUMQUAM_SCHEMA_VERSION,
            "request_id": request_id,
            "request_id_source": "client",
            "operation": "writeback.resolve",
            "ok": true,
            "degrade_mode": false,
            "warnings": [],
            "data": {
                "proposal_id": proposal_id,
                "status": status,
                "already_resolved": false,
                "resolved_at_utc": Utc::now(),
                "audit_ref": "audit_stub_resolve"
            }
        }))
    }

    async fn numquam_stub_mcp(
        AxumState(state): AxumState<NumquamStubState>,
        Json(payload): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let id = payload
            .get("id")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Number(1u64.into()));
        let method = payload
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if method == "initialize" {
            return Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {}
                }
            }));
        }
        if method != "tools/call" {
            return Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": "method not found"
                }
            }));
        }

        let params = payload
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let name = params
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let structured_content = match name {
            "integration.context.build" => {
                let request_id = arguments
                    .get("request_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("req_stub_mcp_context")
                    .to_string();
                serde_json::json!({
                    "schema_version": NUMQUAM_SCHEMA_VERSION,
                    "request_id": request_id,
                    "request_id_source": "client",
                    "operation": "context.build",
                    "ok": true,
                    "degrade_mode": state.config.context_degrade,
                    "warnings": [],
                    "data": {
                        "context_text": state.config.context_text,
                        "evidence": [{
                            "evidence_id": "ev_stub_1",
                            "section": "fact",
                            "kind": "fact_card",
                            "summary": "User preference memory",
                            "citations": ["conv#1"],
                            "confidence": 0.88
                        }],
                        "route": "ltm_light",
                        "confidence": 0.88
                    }
                })
            }
            "integration.writeback.propose" => {
                let request_id = arguments
                    .get("request_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("req_stub_mcp_propose")
                    .to_string();
                let run_id = arguments
                    .get("run_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("run_stub");
                serde_json::json!({
                    "schema_version": NUMQUAM_SCHEMA_VERSION,
                    "request_id": request_id,
                    "request_id_source": "client",
                    "operation": "writeback.propose",
                    "ok": true,
                    "degrade_mode": false,
                    "warnings": [],
                    "data": {
                        "proposal_id": format!("proposal_{run_id}"),
                        "status": "pending_review",
                        "idempotent_replay": false,
                        "audit_ref": format!("audit_{run_id}")
                    }
                })
            }
            "integration.writeback.resolve" => {
                state.resolve_calls.fetch_add(1, Ordering::Relaxed);
                let request_id = arguments
                    .get("request_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("req_stub_mcp_resolve")
                    .to_string();
                let proposal_id = arguments
                    .get("proposal_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("proposal_stub")
                    .to_string();
                let decision = arguments
                    .get("decision")
                    .and_then(|value| value.as_str())
                    .unwrap_or("approve");
                let status = if decision == "approve" {
                    "approved"
                } else {
                    "rejected"
                };
                serde_json::json!({
                    "schema_version": NUMQUAM_SCHEMA_VERSION,
                    "request_id": request_id,
                    "request_id_source": "client",
                    "operation": "writeback.resolve",
                    "ok": true,
                    "degrade_mode": false,
                    "warnings": [],
                    "data": {
                        "proposal_id": proposal_id,
                        "status": status,
                        "already_resolved": false,
                        "resolved_at_utc": Utc::now(),
                        "audit_ref": "audit_stub_resolve"
                    }
                })
            }
            _ => serde_json::json!({
                "schema_version": NUMQUAM_SCHEMA_VERSION,
                "request_id": "req_stub_unknown",
                "request_id_source": "server_generated",
                "operation": name,
                "ok": false,
                "degrade_mode": false,
                "warnings": [],
                "error": {
                    "code": "INVALID_INPUT",
                    "message": "unknown tool",
                    "retryable": false,
                    "operator_action": "use_supported_tool"
                }
            }),
        };
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "structuredContent": structured_content,
                "content": [{
                    "type": "text",
                    "text": structured_content.to_string()
                }],
                "isError": false
            }
        }))
    }

    fn build_test_numquam_client(base_url: &str) -> NumquamClient {
        build_test_numquam_client_with_transport(base_url, NumquamTransport::Http)
    }

    fn build_test_numquam_client_with_transport(
        base_url: &str,
        transport: NumquamTransport,
    ) -> NumquamClient {
        NumquamClient {
            transport,
            integration_base_url: base_url.trim_end_matches('/').to_string(),
            mcp_url: format!("{}/mcp", base_url.trim_end_matches('/')),
            token: Some("stub-token".to_string()),
            principal_id: "test_operator".to_string(),
            principal_display_name: "Test Operator".to_string(),
            request_timeout: Duration::from_secs(3),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("numquam test client"),
        }
    }

    #[derive(Clone)]
    struct AuthFlowStubConfig {
        token_status: StatusCode,
        access_token: String,
        refresh_token: Option<String>,
        expires_in: i64,
        expected_openai_bearer: Option<String>,
        expected_anthropic_token: Option<String>,
    }

    impl AuthFlowStubConfig {
        fn openai(access_token: &str) -> Self {
            Self {
                token_status: StatusCode::OK,
                access_token: access_token.to_string(),
                refresh_token: Some("refresh-from-stub".to_string()),
                expires_in: 3600,
                expected_openai_bearer: Some(access_token.to_string()),
                expected_anthropic_token: None,
            }
        }

        fn anthropic(setup_token: &str) -> Self {
            Self {
                token_status: StatusCode::OK,
                access_token: "unused".to_string(),
                refresh_token: None,
                expires_in: 3600,
                expected_openai_bearer: None,
                expected_anthropic_token: Some(setup_token.to_string()),
            }
        }
    }

    struct AuthFlowStubServer {
        base_url: String,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for AuthFlowStubServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn spawn_auth_flow_stub(config: AuthFlowStubConfig) -> AuthFlowStubServer {
        let app = Router::new()
            .route("/oauth/token", post(auth_stub_oauth_token))
            .route("/v1/chat/completions", post(auth_stub_openai_completion))
            .route("/v1/models", get(auth_stub_anthropic_models))
            .route("/v1/messages", post(auth_stub_anthropic_messages))
            .with_state(config);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind auth stub");
        let addr = listener.local_addr().expect("auth stub local addr");
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("auth stub server");
        });
        AuthFlowStubServer {
            base_url: format!("http://{addr}"),
            task,
        }
    }

    async fn auth_stub_oauth_token(
        AxumState(config): AxumState<AuthFlowStubConfig>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        if config.token_status != StatusCode::OK {
            return (
                config.token_status,
                Json(serde_json::json!({
                    "error": "invalid_grant"
                })),
            );
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "access_token": config.access_token,
                "refresh_token": config.refresh_token,
                "expires_in": config.expires_in
            })),
        )
    }

    async fn auth_stub_openai_completion(
        AxumState(config): AxumState<AuthFlowStubConfig>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<serde_json::Value>) {
        if let Some(expected) = config.expected_openai_bearer {
            let token = parse_bearer_token(&headers).unwrap_or_default().to_string();
            if token != expected {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": { "message": "invalid bearer" }
                    })),
                );
            }
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "choices": [
                    {
                        "message": {
                            "content": "oauth openai ok"
                        }
                    }
                ]
            })),
        )
    }

    async fn auth_stub_anthropic_models(
        AxumState(config): AxumState<AuthFlowStubConfig>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<serde_json::Value>) {
        if let Some(expected) = config.expected_anthropic_token {
            let token = headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .trim()
                .to_string();
            if token != expected {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": { "message": "invalid setup token" }
                    })),
                );
            }
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "data": [
                    { "id": "claude-test" }
                ]
            })),
        )
    }

    async fn auth_stub_anthropic_messages(
        AxumState(config): AxumState<AuthFlowStubConfig>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<serde_json::Value>) {
        if let Some(expected) = config.expected_anthropic_token {
            let token = headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .trim()
                .to_string();
            if token != expected {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": { "message": "invalid api key" }
                    })),
                );
            }
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "anthropic setup token ok"
                    }
                ]
            })),
        )
    }

    fn auth_request(method: &str, uri: &str, body: Body) -> Request<Body> {
        auth_request_with_token(method, uri, body, "test-token")
    }

    fn auth_request_with_token(method: &str, uri: &str, body: Body, token: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(body)
            .expect("request")
    }

    fn auth_request_with_operator(
        method: &str,
        uri: &str,
        body: Body,
        operator_id: &str,
    ) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", "Bearer test-token")
            .header("content-type", "application/json")
            .header("x-operator-id", operator_id)
            .body(body)
            .expect("request")
    }

    async fn parse_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        serde_json::from_slice(&bytes).expect("json body")
    }

    #[derive(Debug, Serialize)]
    struct TestJwtClaims {
        iss: String,
        aud: String,
        sub: String,
        exp: i64,
        iat: i64,
        jti: String,
        roles: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    }

    #[allow(clippy::too_many_arguments)]
    fn mint_test_jwt(
        secret: &str,
        issuer: &str,
        audience: &str,
        subject: &str,
        jti: &str,
        roles: &[&str],
        exp_offset_seconds: i64,
        iat_offset_seconds: i64,
    ) -> String {
        let now = Utc::now().timestamp();
        let claims = TestJwtClaims {
            iss: issuer.to_string(),
            aud: audience.to_string(),
            sub: subject.to_string(),
            exp: now.saturating_add(exp_offset_seconds),
            iat: now.saturating_add(iat_offset_seconds),
            jti: jti.to_string(),
            roles: roles.iter().map(|role| role.to_string()).collect(),
            scope: None,
            session_id: Some("test-session-id".to_string()),
        };
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("mint jwt")
    }

    #[test]
    fn parse_bool_value_handles_expected_values() {
        assert_eq!(parse_bool_value("true"), Some(true));
        assert_eq!(parse_bool_value("1"), Some(true));
        assert_eq!(parse_bool_value("On"), Some(true));
        assert_eq!(parse_bool_value("false"), Some(false));
        assert_eq!(parse_bool_value("0"), Some(false));
        assert_eq!(parse_bool_value("OFF"), Some(false));
        assert_eq!(parse_bool_value("maybe"), None);
    }

    #[test]
    fn bool_env_falls_back_to_default_for_invalid_values() {
        std::env::set_var("CARSINOS_TEST_BOOL_ENV", "invalid");
        assert!(bool_env("CARSINOS_TEST_BOOL_ENV", true));
        assert!(!bool_env("CARSINOS_TEST_BOOL_ENV", false));
        std::env::remove_var("CARSINOS_TEST_BOOL_ENV");
    }

    #[test]
    fn credential_expiry_parser_supports_seconds_and_ms() {
        let payload_ms = serde_json::json!({ "expires_at_ms": 12345 });
        let payload_sec = serde_json::json!({ "expires_at_unix": 12 });
        let payload_alt = serde_json::json!({ "expires_at": "15" });
        let payload_none = serde_json::json!({ "x": 1 });

        assert_eq!(extract_credentials_expiry_ms(&payload_ms), Some(12345));
        assert_eq!(extract_credentials_expiry_ms(&payload_sec), Some(12000));
        assert_eq!(extract_credentials_expiry_ms(&payload_alt), Some(15000));
        assert_eq!(extract_credentials_expiry_ms(&payload_none), None);
    }

    #[tokio::test]
    async fn unauthorized_requests_are_rejected() {
        let ctx = test_context();
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/sessions")
            .body(Body::empty())
            .expect("request");

        let response = ctx.app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().get("x-request-id").is_some());
    }

    #[tokio::test]
    async fn jwt_auth_rejects_invalid_signature_issuer_audience_expired_and_revoked_jti() {
        let secret = "0123456789abcdef0123456789abcdef";
        let mut revoked = HashSet::new();
        revoked.insert("revoked-jti".to_string());
        let ctx = test_context_with_jwt(secret, revoked);

        let valid = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "principal-valid",
            "jti-valid",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );
        let valid_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &valid,
            ))
            .await
            .expect("valid jwt request");
        assert_eq!(valid_response.status(), StatusCode::OK);

        let invalid_signature = mint_test_jwt(
            "wrong-secret-wrong-secret",
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "principal-valid",
            "jti-bad-signature",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );
        let invalid_signature_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &invalid_signature,
            ))
            .await
            .expect("invalid signature response");
        assert_eq!(
            invalid_signature_response.status(),
            StatusCode::UNAUTHORIZED
        );
        let invalid_signature_json = parse_json(invalid_signature_response).await;
        assert_eq!(invalid_signature_json["error_code"], "AUTH_INVALID");

        let invalid_issuer = mint_test_jwt(
            secret,
            "different-issuer",
            "carsinos-test-audience",
            "principal-valid",
            "jti-bad-issuer",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );
        let invalid_issuer_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &invalid_issuer,
            ))
            .await
            .expect("invalid issuer response");
        assert_eq!(invalid_issuer_response.status(), StatusCode::UNAUTHORIZED);
        let invalid_issuer_json = parse_json(invalid_issuer_response).await;
        assert_eq!(invalid_issuer_json["error_code"], "AUTH_INVALID");

        let invalid_audience = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "different-audience",
            "principal-valid",
            "jti-bad-audience",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );
        let invalid_audience_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &invalid_audience,
            ))
            .await
            .expect("invalid audience response");
        assert_eq!(invalid_audience_response.status(), StatusCode::UNAUTHORIZED);
        let invalid_audience_json = parse_json(invalid_audience_response).await;
        assert_eq!(invalid_audience_json["error_code"], "AUTH_INVALID");

        let expired = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "principal-valid",
            "jti-expired",
            &[ROLE_OPERATOR_READONLY],
            -600,
            -700,
        );
        let expired_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &expired,
            ))
            .await
            .expect("expired response");
        assert_eq!(expired_response.status(), StatusCode::UNAUTHORIZED);
        let expired_json = parse_json(expired_response).await;
        assert_eq!(expired_json["error_code"], "AUTH_EXPIRED");

        let revoked_token = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "principal-valid",
            "revoked-jti",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );
        let revoked_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &revoked_token,
            ))
            .await
            .expect("revoked response");
        assert_eq!(revoked_response.status(), StatusCode::FORBIDDEN);
        let revoked_json = parse_json(revoked_response).await;
        assert_eq!(revoked_json["error_code"], "AUTH_FORBIDDEN");
    }

    #[tokio::test]
    async fn role_mismatch_blocks_auth_profile_mutation_and_approval_resolution() {
        let secret = "abcdef0123456789abcdef0123456789";
        let ctx = test_context_with_jwt(secret, HashSet::new());
        let admin_token = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "admin-user",
            "jti-admin",
            &[ROLE_OPERATOR_ADMIN],
            300,
            0,
        );
        let readonly_token = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "readonly-user",
            "jti-readonly",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );

        let forbidden_create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{"provider":"mock","display_name":"nope","auth_mode":"api_key","risk_level":"low","enabled":true,"kill_switch_scope":"none","credentials_json":{}}"#,
                ),
                &readonly_token,
            ))
            .await
            .expect("readonly create profile response");
        assert_eq!(forbidden_create_profile.status(), StatusCode::FORBIDDEN);
        let forbidden_create_profile_json = parse_json(forbidden_create_profile).await;
        assert_eq!(
            forbidden_create_profile_json["error_code"],
            "AUTH_ROLE_MISMATCH"
        );

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"rbac-check"}"#),
                &admin_token,
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"approval pls"}"#),
                &admin_token,
            ))
            .await
            .expect("create message");
        let create_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
                &admin_token,
            ))
            .await
            .expect("create run");
        let create_run_json = parse_json(create_run_response).await;
        let run_id = create_run_json["run"]["run_id"]
            .as_str()
            .expect("run id")
            .to_string();

        let create_approval_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"process.exec","request_summary":"rbac check","request_json":{{"cmd":"echo hi"}}}}"#
                )),
                &admin_token,
            ))
            .await
            .expect("create approval");
        assert_eq!(create_approval_response.status(), StatusCode::CREATED);
        let create_approval_json = parse_json(create_approval_response).await;
        let approval_id = create_approval_json["approval"]["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        let forbidden_resolve = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                &format!("/api/v1/approvals/{approval_id}/resolve"),
                Body::from(r#"{"decision":"approve"}"#),
                &readonly_token,
            ))
            .await
            .expect("readonly resolve response");
        assert_eq!(forbidden_resolve.status(), StatusCode::FORBIDDEN);
        let forbidden_resolve_json = parse_json(forbidden_resolve).await;
        assert_eq!(forbidden_resolve_json["error_code"], "AUTH_ROLE_MISMATCH");

        let forbidden_job_create = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                "/api/v1/jobs/add",
                Body::from(
                    r#"{"name":"blocked-job","schedule_kind":"interval","interval_seconds":60,"payload_json":{"mode":"noop"}}"#,
                ),
                &readonly_token,
            ))
            .await
            .expect("readonly job create response");
        assert_eq!(forbidden_job_create.status(), StatusCode::FORBIDDEN);
        let forbidden_job_create_json = parse_json(forbidden_job_create).await;
        assert_eq!(
            forbidden_job_create_json["error_code"],
            "AUTH_ROLE_MISMATCH"
        );

        let forbidden_channel_update = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "POST",
                "/api/v1/config/channels",
                Body::from(
                    r#"{"telegram":{"require_mention_in_groups":false,"allowlisted_user_ids":[]}}"#,
                ),
                &readonly_token,
            ))
            .await
            .expect("readonly channel update response");
        assert_eq!(forbidden_channel_update.status(), StatusCode::FORBIDDEN);
        let forbidden_channel_update_json = parse_json(forbidden_channel_update).await;
        assert_eq!(
            forbidden_channel_update_json["error_code"],
            "AUTH_ROLE_MISMATCH"
        );

        let audit_list_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/security/audit?limit=200",
                Body::empty(),
                &admin_token,
            ))
            .await
            .expect("audit list response");
        assert_eq!(audit_list_response.status(), StatusCode::OK);
        let audit_list_json = parse_json(audit_list_response).await;
        let audit_items = audit_list_json["items"].as_array().expect("audit items");
        assert!(audit_items
            .iter()
            .any(|item| { item["action"] == "auth.profile.create" && item["decision"] == "deny" }));
        assert!(audit_items
            .iter()
            .any(|item| { item["action"] == "approval.resolve" && item["decision"] == "deny" }));
        assert!(audit_items
            .iter()
            .any(|item| item["action"] == "job.create" && item["decision"] == "deny"));
        assert!(audit_items.iter().any(|item| {
            item["action"] == "channel.config.update" && item["decision"] == "deny"
        }));

        let deny_filtered_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/security/audit?limit=200&decision=deny&status=403&error_code=AUTH_ROLE_MISMATCH",
                Body::empty(),
                &admin_token,
            ))
            .await
            .expect("deny filtered audit response");
        assert_eq!(deny_filtered_response.status(), StatusCode::OK);
        let deny_filtered_json = parse_json(deny_filtered_response).await;
        let deny_items = deny_filtered_json["items"]
            .as_array()
            .expect("deny filtered items");
        assert!(!deny_items.is_empty());
        assert!(deny_items.iter().all(|item| item["decision"] == "deny"));
        assert!(deny_items.iter().all(|item| item["status"] == "403"));
        assert!(deny_items
            .iter()
            .all(|item| item["error_code"] == "AUTH_ROLE_MISMATCH"));
    }

    #[tokio::test]
    async fn jwt_replay_protection_rejects_reused_jti() {
        let secret = "00112233445566778899aabbccddeeff";
        let ctx = test_context_with_jwt_replay(secret, HashSet::new(), true);
        let replay_token = mint_test_jwt(
            secret,
            "carsinos-test-issuer",
            "carsinos-test-audience",
            "replay-principal",
            "replay-jti-1",
            &[ROLE_OPERATOR_READONLY],
            300,
            0,
        );

        let first_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &replay_token,
            ))
            .await
            .expect("first replay-protection request");
        assert_eq!(first_response.status(), StatusCode::OK);

        let second_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_token(
                "GET",
                "/api/v1/auth/profiles",
                Body::empty(),
                &replay_token,
            ))
            .await
            .expect("second replay-protection request");
        assert_eq!(second_response.status(), StatusCode::FORBIDDEN);
        let second_json = parse_json(second_response).await;
        assert_eq!(second_json["error_code"], "AUTH_FORBIDDEN");
        assert_eq!(second_json["error"], "jwt token id replay detected");
    }

    #[tokio::test]
    async fn forwarded_for_header_is_rejected_when_proxy_headers_are_disabled() {
        let ctx = test_context();
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/auth/profiles")
            .header("authorization", "Bearer test-token")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .expect("request");
        let response = ctx
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("forwarded-for response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let json = parse_json(response).await;
        assert_eq!(json["error_code"], "POLICY_DENY");
    }

    #[tokio::test]
    async fn security_audit_rejects_inverted_created_time_range() {
        let ctx = test_context();

        let response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/security/audit?created_after=200&created_before=100",
                Body::empty(),
            ))
            .await
            .expect("audit list response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json = parse_json(response).await;
        assert_eq!(json["error_code"], "INVALID_INPUT");
        assert_eq!(json["error"], "created_after must be <= created_before");
    }

    #[tokio::test]
    async fn security_audit_retention_run_archives_and_prunes_events() {
        let ctx = test_context();
        let created = ctx
            .storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "retention-seed-1".to_string(),
                correlation_id: "retention-seed-1".to_string(),
                principal: "operator_admin:test".to_string(),
                action: "security.audit.retention.seed".to_string(),
                resource: "seed:event".to_string(),
                decision: "allow".to_string(),
                reason: None,
                transport: "http".to_string(),
                status: "200".to_string(),
                error_code: None,
                session_id: None,
                run_id: None,
                metadata_json: None,
            })
            .expect("seed security audit event");
        sleep(Duration::from_millis(5)).await;

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/security/audit/retention/run",
                Body::from(r#"{"hot_retention_days":0,"dry_run":false}"#),
            ))
            .await
            .expect("retention run response");
        assert_eq!(run_response.status(), StatusCode::OK);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["hot_retention_days"], 0);
        assert!(run_json["candidate_count"].as_i64().unwrap_or_default() >= 1);
        assert!(run_json["archived_count"].as_i64().unwrap_or_default() >= 1);
        assert!(run_json["deleted_count"].as_i64().unwrap_or_default() >= 1);
        assert_eq!(run_json["dry_run"], false);

        assert!(ctx
            .storage
            .get_security_audit_event(&created.event_id)
            .expect("load seeded live event")
            .is_none());
        assert!(ctx
            .storage
            .get_archived_security_audit_event(&created.event_id)
            .expect("load seeded archived event")
            .is_some());
    }

    #[tokio::test]
    async fn security_audit_retention_run_rejects_invalid_day_range() {
        let ctx = test_context();
        let response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/security/audit/retention/run",
                Body::from(r#"{"hot_retention_days":4000,"dry_run":true}"#),
            ))
            .await
            .expect("invalid retention request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json = parse_json(response).await;
        assert_eq!(json["error_code"], "INVALID_INPUT");
        assert_eq!(
            json["error"],
            "hot_retention_days must be between 0 and 3650"
        );
    }

    #[tokio::test]
    async fn run_endpoint_rate_limit_returns_429_with_stable_code() {
        let ctx = test_context_with_rate_limits(RequestRateLimitConfig {
            enabled: true,
            window_seconds: 60,
            per_ip_limit: 100,
            per_principal_limit: 100,
            per_run_endpoint_limit: 1,
            per_approval_endpoint_limit: 100,
        });

        let create_session = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"run-rate-limit"}"#),
            ))
            .await
            .expect("create session");
        let session_json = parse_json(create_session).await;
        let session_id = session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"first run"}"#),
            ))
            .await
            .expect("create message");

        let first_run = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("first run");
        assert_eq!(first_run.status(), StatusCode::CREATED);

        let second_run = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("second run");
        assert_eq!(second_run.status(), StatusCode::TOO_MANY_REQUESTS);
        let second_run_json = parse_json(second_run).await;
        assert_eq!(second_run_json["error_code"], "RATE_LIMITED");
        assert_eq!(second_run_json["rate_limit_scope"], "run.principal");
        assert_eq!(second_run_json["retry_after_seconds"], 60);
    }

    #[tokio::test]
    async fn run_endpoint_rate_limit_retry_after_tracks_remaining_window() {
        let ctx = test_context_with_rate_limits(RequestRateLimitConfig {
            enabled: true,
            window_seconds: 3,
            per_ip_limit: 100,
            per_principal_limit: 100,
            per_run_endpoint_limit: 1,
            per_approval_endpoint_limit: 100,
        });

        let create_session = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"rate-limit-retry-after"}"#),
            ))
            .await
            .expect("create session");
        let session_json = parse_json(create_session).await;
        let session_id = session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"retry-after check"}"#),
            ))
            .await
            .expect("create message");

        let first_run = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("first run");
        assert_eq!(first_run.status(), StatusCode::CREATED);

        sleep(Duration::from_millis(1200)).await;

        let second_run = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("second run");
        assert_eq!(second_run.status(), StatusCode::TOO_MANY_REQUESTS);
        let second_run_json = parse_json(second_run).await;
        assert_eq!(second_run_json["error_code"], "RATE_LIMITED");
        assert_eq!(second_run_json["rate_limit_scope"], "run.principal");
        let retry_after_seconds = second_run_json["retry_after_seconds"]
            .as_i64()
            .expect("retry_after_seconds");
        assert!(
            (1..=2).contains(&retry_after_seconds),
            "expected retry_after_seconds between 1 and 2, got {retry_after_seconds}"
        );
    }

    #[tokio::test]
    async fn approval_endpoint_rate_limit_returns_429_with_stable_code() {
        let ctx = test_context_with_rate_limits(RequestRateLimitConfig {
            enabled: true,
            window_seconds: 60,
            per_ip_limit: 100,
            per_principal_limit: 100,
            per_run_endpoint_limit: 100,
            per_approval_endpoint_limit: 1,
        });

        let create_session = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"approval-rate-limit"}"#),
            ))
            .await
            .expect("create session");
        let session_json = parse_json(create_session).await;
        let session_id = session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"approval test"}"#),
            ))
            .await
            .expect("create message");
        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        let run_json = parse_json(run_response).await;
        let run_id = run_json["run"]["run_id"].as_str().expect("run id");

        let first_approval = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"process.exec","request_summary":"first","request_json":{{"cmd":"echo hi"}}}}"#
                )),
            ))
            .await
            .expect("first approval");
        assert_eq!(first_approval.status(), StatusCode::CREATED);

        let second_approval = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"process.exec","request_summary":"second","request_json":{{"cmd":"echo hi"}}}}"#
                )),
            ))
            .await
            .expect("second approval");
        assert_eq!(second_approval.status(), StatusCode::TOO_MANY_REQUESTS);
        let second_approval_json = parse_json(second_approval).await;
        assert_eq!(second_approval_json["error_code"], "RATE_LIMITED");
        assert_eq!(
            second_approval_json["rate_limit_scope"],
            "approval.principal"
        );
        assert_eq!(second_approval_json["retry_after_seconds"], 60);
    }

    #[tokio::test]
    async fn auth_rate_limit_returns_429_with_scope_and_retry_hint() {
        let ctx = test_context_with_rate_limits(RequestRateLimitConfig {
            enabled: true,
            window_seconds: 60,
            per_ip_limit: 1,
            per_principal_limit: 100,
            per_run_endpoint_limit: 100,
            per_approval_endpoint_limit: 100,
        });

        let first = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"rate-limit-auth-1"}"#),
            ))
            .await
            .expect("first create session");
        assert_eq!(first.status(), StatusCode::CREATED);

        let second = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"rate-limit-auth-2"}"#),
            ))
            .await
            .expect("second create session");
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        let second_json = parse_json(second).await;
        assert_eq!(second_json["error_code"], "RATE_LIMITED");
        assert_eq!(second_json["rate_limit_scope"], "auth");
        assert_eq!(second_json["retry_after_seconds"], 60);
    }

    #[tokio::test]
    async fn metrics_endpoint_reports_runtime_counters() {
        let ctx = test_context();

        let unauthorized = Request::builder()
            .method("GET")
            .uri("/api/v1/health")
            .body(Body::empty())
            .expect("unauthorized request");
        let unauthorized_response = ctx
            .app
            .clone()
            .oneshot(unauthorized)
            .await
            .expect("unauthorized response");
        assert_eq!(unauthorized_response.status(), StatusCode::UNAUTHORIZED);

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/memory/notes",
                Body::from(r#"{"title":"metric-note","body":"track me","tags":["ops"]}"#),
            ))
            .await
            .expect("create note");

        let note_list = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/memory/notes?limit=1",
                Body::empty(),
            ))
            .await
            .expect("list notes");
        let note_list_json = parse_json(note_list).await;
        let note_id = note_list_json["items"][0]["note_id"]
            .as_str()
            .expect("note_id")
            .to_string();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/memory/notes/{note_id}"),
                Body::from(r#"{"body":"track me updated"}"#),
            ))
            .await
            .expect("update note");

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"metric-run"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"metric run"}"#),
            ))
            .await
            .expect("create message");
        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);

        let metrics_response = ctx
            .app
            .clone()
            .oneshot(auth_request("GET", "/api/v1/metrics", Body::empty()))
            .await
            .expect("metrics response");
        assert_eq!(metrics_response.status(), StatusCode::OK);
        let metrics_json = parse_json(metrics_response).await;
        assert!(metrics_json["requests_total"].as_u64().unwrap_or(0) >= 8);
        assert!(metrics_json["auth_failures_total"].as_u64().unwrap_or(0) >= 1);
        assert!(metrics_json["runs_started_total"].as_u64().unwrap_or(0) >= 1);
        assert!(metrics_json["runs_succeeded_total"].as_u64().unwrap_or(0) >= 1);
        assert!(metrics_json["notes_created_total"].as_u64().unwrap_or(0) >= 1);
        assert!(metrics_json["notes_updated_total"].as_u64().unwrap_or(0) >= 1);
    }

    #[tokio::test]
    async fn session_lifecycle_endpoints_work() {
        let ctx = test_context();

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"alpha"}"#),
            ))
            .await
            .expect("create response");
        assert_eq!(create_response.status(), StatusCode::CREATED);
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let list_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/sessions?limit=10",
                Body::empty(),
            ))
            .await
            .expect("list response");
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_json = parse_json(list_response).await;
        let items = list_json["items"].as_array().expect("items array");
        assert!(items.iter().any(|item| item["session_id"] == session_id));

        let get_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}"),
                Body::empty(),
            ))
            .await
            .expect("get response");
        assert_eq!(get_response.status(), StatusCode::OK);

        let message_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello"}"#),
            ))
            .await
            .expect("message response");
        assert_eq!(message_response.status(), StatusCode::CREATED);

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("run response");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");

        let messages_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}/messages?limit=50"),
                Body::empty(),
            ))
            .await
            .expect("messages response");
        assert_eq!(messages_response.status(), StatusCode::OK);
        let messages_json = parse_json(messages_response).await;
        let message_items = messages_json["items"].as_array().expect("message items");
        assert_eq!(message_items.len(), 2);
        assert_eq!(message_items[0]["role"], "user");
        assert_eq!(message_items[1]["role"], "assistant");

        let updated_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}"),
                Body::empty(),
            ))
            .await
            .expect("updated session response");
        let updated_json = parse_json(updated_session_response).await;
        assert_eq!(updated_json["session"]["message_count"], 2);
        assert_eq!(updated_json["session"]["run_count"], 1);
    }

    #[tokio::test]
    async fn note_crud_and_memory_search_endpoints_work() {
        let ctx = test_context();

        let create_first = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/memory/notes",
                Body::from(
                    r#"{"title":"Preference","body":"User prefers apples and tea","tags":["food","memory"]}"#,
                ),
            ))
            .await
            .expect("create first note");
        assert_eq!(create_first.status(), StatusCode::CREATED);
        let first_json = parse_json(create_first).await;
        let first_note_id = first_json["note"]["note_id"]
            .as_str()
            .expect("first note id")
            .to_string();

        let create_second = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/memory/notes",
                Body::from(
                    r#"{"title":"Infra","body":"Production host runs Linux","tags":["ops"]}"#,
                ),
            ))
            .await
            .expect("create second note");
        assert_eq!(create_second.status(), StatusCode::CREATED);

        let list_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/memory/notes?limit=20",
                Body::empty(),
            ))
            .await
            .expect("list notes");
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_json = parse_json(list_response).await;
        let items = list_json["items"].as_array().expect("note items");
        assert_eq!(items.len(), 2);

        let update_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/memory/notes/{first_note_id}"),
                Body::from(
                    r#"{"body":"User strongly prefers apples and green tea","tags":["food","updated"]}"#,
                ),
            ))
            .await
            .expect("update note");
        assert_eq!(update_response.status(), StatusCode::OK);
        let updated_json = parse_json(update_response).await;
        assert!(updated_json["note"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("green tea"));

        let search_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/memory/search",
                Body::from(
                    r#"{"query_text":"what drink does user prefer","top_k":4,"max_chars":500}"#,
                ),
            ))
            .await
            .expect("search memory");
        assert_eq!(search_response.status(), StatusCode::OK);
        let search_json = parse_json(search_response).await;
        let search_items = search_json["items"].as_array().expect("search items");
        assert!(!search_items.is_empty());
        assert!(search_items
            .iter()
            .any(|item| item["note_id"] == first_note_id));
    }

    #[tokio::test]
    async fn local_memory_context_is_injected_into_provider_input_with_usage_metadata() {
        let ctx = test_context();

        let create_note_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/memory/notes",
                Body::from(
                    r#"{"title":"Shell Preference","body":"Remember this user prefers zsh shell in terminal sessions.","tags":["preference"]}"#,
                ),
            ))
            .await
            .expect("create note");
        assert_eq!(create_note_response.status(), StatusCode::CREATED);

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"local-memory-injection"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(
                    r#"{"role":"user","content_text":"Which shell should be used for terminal commands?"}"#,
                ),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
        let usage_json = run_json["run"]["usage_json"].as_str().expect("usage_json");
        let usage_value: serde_json::Value =
            serde_json::from_str(usage_json).expect("usage json parse");
        assert_eq!(usage_value["local_memory"]["enabled"], true);
        assert!(
            usage_value["local_memory"]["hit_count"]
                .as_u64()
                .unwrap_or(0)
                >= 1
        );

        let messages_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}/messages?limit=10"),
                Body::empty(),
            ))
            .await
            .expect("messages response");
        let messages_json = parse_json(messages_response).await;
        let items = messages_json["items"].as_array().expect("items");
        let assistant_text = items[1]["content_text"].as_str().expect("assistant text");
        assert!(assistant_text.contains("Local notes context"));
        assert!(assistant_text.contains("prefers zsh shell"));
    }

    #[tokio::test]
    async fn numquam_context_and_writeback_are_wired_into_run_flow() {
        let stub = spawn_numquam_stub(NumquamStubConfig::healthy()).await;
        let ctx = test_context_with_numquam(build_test_numquam_client(&stub.base_url));

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"numquam-run"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello memory"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
        let usage_json = run_json["run"]["usage_json"]
            .as_str()
            .expect("usage_json in run response");
        let usage_value: serde_json::Value =
            serde_json::from_str(usage_json).expect("valid usage_json payload");
        assert_eq!(usage_value["memory"]["enabled"], true);
        assert_eq!(
            usage_value["memory"]["evidence"][0]["evidence_id"],
            "ev_stub_1"
        );

        let messages_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}/messages?limit=10"),
                Body::empty(),
            ))
            .await
            .expect("list messages");
        let messages_json = parse_json(messages_response).await;
        let items = messages_json["items"].as_array().expect("message items");
        let assistant_text = items[1]["content_text"]
            .as_str()
            .expect("assistant content");
        assert!(assistant_text.contains("stored memory says user likes tea"));

        let approvals_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals");
        let approvals_json = parse_json(approvals_response).await;
        let approvals = approvals_json["items"].as_array().expect("approval items");
        assert!(approvals
            .iter()
            .any(|item| item["kind"] == NUMQUAM_APPROVAL_KIND_WRITEBACK));
    }

    #[tokio::test]
    async fn numquam_mcp_transport_executes_context_and_writeback_paths() {
        let stub = spawn_numquam_stub(NumquamStubConfig::healthy()).await;
        let ctx = test_context_with_numquam(build_test_numquam_client_with_transport(
            &stub.base_url,
            NumquamTransport::Mcp,
        ));

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"numquam-mcp"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello mcp memory"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
        let usage_json = run_json["run"]["usage_json"]
            .as_str()
            .expect("usage_json in run response");
        let usage_value: serde_json::Value =
            serde_json::from_str(usage_json).expect("valid usage_json");
        assert_eq!(usage_value["memory"]["transport"], "mcp");
    }

    #[tokio::test]
    async fn numquam_degrade_mode_falls_back_to_stateless_provider_input() {
        let mut config = NumquamStubConfig::healthy();
        config.context_degrade = true;
        config.context_text = "degraded context should not be injected".to_string();
        let stub = spawn_numquam_stub(config).await;
        let ctx = test_context_with_numquam(build_test_numquam_client(&stub.base_url));

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"numquam-degrade"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello degrade"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
        let usage_json = run_json["run"]["usage_json"]
            .as_str()
            .expect("usage_json in run response");
        let usage_value: serde_json::Value =
            serde_json::from_str(usage_json).expect("valid usage json");
        assert_eq!(usage_value["memory"]["context_degrade_mode"], true);

        let messages_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}/messages?limit=10"),
                Body::empty(),
            ))
            .await
            .expect("list messages");
        let messages_json = parse_json(messages_response).await;
        let items = messages_json["items"].as_array().expect("message items");
        let assistant_text = items[1]["content_text"]
            .as_str()
            .expect("assistant content");
        assert!(!assistant_text.contains("degraded context should not be injected"));
    }

    #[tokio::test]
    async fn resolving_memory_writeback_approval_calls_numquam_resolve() {
        let stub = spawn_numquam_stub(NumquamStubConfig::healthy()).await;
        let resolve_counter = stub.resolve_calls.clone();
        let ctx = test_context_with_numquam(build_test_numquam_client(&stub.base_url));

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"numquam-resolve"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello resolve"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        let run_json = parse_json(run_response).await;
        let run_id = run_json["run"]["run_id"].as_str().expect("run_id");

        let approvals_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals");
        let approvals_json = parse_json(approvals_response).await;
        let approval_id = approvals_json["items"]
            .as_array()
            .expect("approval items")
            .iter()
            .find(|item| {
                item["run_id"] == run_id && item["kind"] == NUMQUAM_APPROVAL_KIND_WRITEBACK
            })
            .and_then(|item| item["approval_id"].as_str())
            .expect("memory writeback approval id")
            .to_string();

        let resolve_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/approvals/{approval_id}/resolve"),
                Body::from(
                    r#"{"decision":"approve","decided_via":"test","decided_by_peer_id":"op-1"}"#,
                ),
            ))
            .await
            .expect("resolve memory approval");
        assert_eq!(resolve_response.status(), StatusCode::OK);
        let resolve_json = parse_json(resolve_response).await;
        assert_eq!(resolve_json["approval"]["status"], "approved");
        assert_eq!(resolve_counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn high_risk_tool_requests_are_gated_by_approval() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"tool-approval-gate"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"tool.exec echo gated"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "failed");
        assert!(run_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("approval required"));

        let approvals_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals");
        assert_eq!(approvals_response.status(), StatusCode::OK);
        let approvals_json = parse_json(approvals_response).await;
        let items = approvals_json["items"].as_array().expect("approval items");
        assert!(!items.is_empty());
    }

    #[tokio::test]
    async fn low_risk_tool_requests_execute_inside_run_loop() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"tool-loop-success"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let message_text = format!("tool.process status {}", std::process::id());
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(format!(
                    r#"{{"role":"user","content_text":"{}"}}"#,
                    message_text
                )),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
    }

    #[tokio::test]
    async fn high_risk_tool_run_can_resume_after_approval() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"tool-resume-approved"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"tool.exec echo resumed"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "failed");
        let run_id = run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();

        let approvals_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals");
        let approvals_json = parse_json(approvals_response).await;
        let approval_id = approvals_json["items"]
            .as_array()
            .expect("approvals")
            .iter()
            .find(|item| item["run_id"] == run_id)
            .and_then(|item| item["approval_id"].as_str())
            .expect("approval_id")
            .to_string();

        let resolve_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/approvals/{approval_id}/resolve"),
                Body::from(r#"{"decision":"approve","decided_via":"test"}"#),
            ))
            .await
            .expect("resolve approval");
        assert_eq!(resolve_response.status(), StatusCode::OK);

        let resume_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/resume"),
                Body::empty(),
            ))
            .await
            .expect("resume run");
        assert_eq!(resume_response.status(), StatusCode::OK);
        let resume_json = parse_json(resume_response).await;
        assert_eq!(resume_json["run"]["status"], "succeeded");

        let messages_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}/messages?limit=50"),
                Body::empty(),
            ))
            .await
            .expect("list messages");
        let messages_json = parse_json(messages_response).await;
        let items = messages_json["items"].as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["role"], "assistant");
    }

    #[tokio::test]
    async fn high_risk_tool_run_resume_without_decision_stays_blocked() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"tool-resume-pending"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"tool.exec echo blocked"}"#),
            ))
            .await
            .expect("create message");

        let first_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(first_run_response.status(), StatusCode::CREATED);
        let first_run_json = parse_json(first_run_response).await;
        let run_id = first_run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();
        assert_eq!(first_run_json["run"]["status"], "failed");

        let approvals_before = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals before resume");
        let approvals_before_json = parse_json(approvals_before).await;
        let before_count = approvals_before_json["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|item| item["run_id"] == run_id)
            .count();
        assert_eq!(before_count, 1);

        let resume_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/resume"),
                Body::empty(),
            ))
            .await
            .expect("resume run");
        assert_eq!(resume_response.status(), StatusCode::OK);
        let resume_json = parse_json(resume_response).await;
        assert_eq!(resume_json["run"]["status"], "failed");
        assert!(resume_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("approval pending"));

        let approvals_after = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals after resume");
        let approvals_after_json = parse_json(approvals_after).await;
        let after_count = approvals_after_json["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|item| item["run_id"] == run_id)
            .count();
        assert_eq!(after_count, 1);
    }

    #[tokio::test]
    async fn high_risk_tool_run_resume_after_denial_remains_failed() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"tool-resume-denied"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"tool.exec echo denied"}"#),
            ))
            .await
            .expect("create message");

        let first_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        let first_run_json = parse_json(first_run_response).await;
        let run_id = first_run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();

        let approvals_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals");
        let approvals_json = parse_json(approvals_response).await;
        let approval_id = approvals_json["items"]
            .as_array()
            .expect("approvals")
            .iter()
            .find(|item| item["run_id"] == run_id)
            .and_then(|item| item["approval_id"].as_str())
            .expect("approval_id")
            .to_string();

        let resolve_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/approvals/{approval_id}/resolve"),
                Body::from(r#"{"decision":"deny","decided_via":"test"}"#),
            ))
            .await
            .expect("resolve approval");
        assert_eq!(resolve_response.status(), StatusCode::OK);

        let resume_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/runs/{run_id}/resume"),
                Body::empty(),
            ))
            .await
            .expect("resume run");
        let resume_json = parse_json(resume_response).await;
        assert_eq!(resume_json["run"]["status"], "failed");
        assert!(resume_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("approval denied"));
    }

    #[tokio::test]
    async fn invalid_tool_process_action_fails_run() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"tool-invalid-process"}"#),
            ))
            .await
            .expect("create session");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"tool.process unknown 123"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "failed");
        assert!(run_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("unsupported process action"));
    }

    #[tokio::test]
    async fn invalid_message_role_is_rejected() {
        let ctx = test_context();
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"beta"}"#),
            ))
            .await
            .expect("create response");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id");

        let bad_message_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"intruder","content_text":"x"}"#),
            ))
            .await
            .expect("bad message response");
        assert_eq!(bad_message_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_session_returns_not_found() {
        let ctx = test_context();
        let missing = "missing-session-id";

        let message_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{missing}/messages"),
                Body::from(r#"{"role":"user","content_text":"x"}"#),
            ))
            .await
            .expect("message response");
        assert_eq!(message_response.status(), StatusCode::NOT_FOUND);

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{missing}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("run response");
        assert_eq!(run_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unsupported_provider_is_rejected_early() {
        let ctx = test_context();

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"provider-failure"}"#),
            ))
            .await
            .expect("create response");
        let create_json = parse_json(create_response).await;
        let session_id = create_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello"}"#),
            ))
            .await
            .expect("message response");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from(r#"{"model_provider":"unsupported","model_id":"x"}"#),
            ))
            .await
            .expect("run response");
        assert_eq!(run_response.status(), StatusCode::BAD_REQUEST);

        let session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/sessions/{session_id}"),
                Body::empty(),
            ))
            .await
            .expect("session response");
        let session_json = parse_json(session_response).await;
        // user message exists; run is rejected before insert
        assert_eq!(session_json["session"]["message_count"], 1);
        assert_eq!(session_json["session"]["run_count"], 0);
    }

    #[tokio::test]
    async fn auth_profile_crud_and_order_endpoints_work() {
        let ctx = test_context();

        let create_primary = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"openai",
                        "display_name":"primary",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "api_base_url":"https://api.openai.com",
                        "credentials_json":{"api_key":"k1"}
                    }"#,
                ),
            ))
            .await
            .expect("create primary profile");
        assert_eq!(create_primary.status(), StatusCode::CREATED);
        let create_primary_json = parse_json(create_primary).await;
        let primary_id = create_primary_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("primary profile id")
            .to_string();

        let create_secondary = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"openai",
                        "display_name":"secondary",
                        "auth_mode":"openai_oauth",
                        "risk_level":"medium",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "api_base_url":"https://api.openai.com",
                        "credentials_json":{"refresh_token":"rt1"}
                    }"#,
                ),
            ))
            .await
            .expect("create secondary profile");
        assert_eq!(create_secondary.status(), StatusCode::CREATED);
        let create_secondary_json = parse_json(create_secondary).await;
        let secondary_id = create_secondary_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("secondary profile id")
            .to_string();

        let list_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/auth/profiles?provider=openai&include_disabled=true",
                Body::empty(),
            ))
            .await
            .expect("list auth profiles");
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_json = parse_json(list_response).await;
        let profiles = list_json["items"].as_array().expect("profile array");
        assert_eq!(profiles.len(), 2);

        let set_order_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/agents/default/providers/openai/profile-order",
                Body::from(format!(
                    r#"{{"profile_ids":["{secondary_id}","{primary_id}"]}}"#
                )),
            ))
            .await
            .expect("set order response");
        assert_eq!(set_order_response.status(), StatusCode::OK);
        let set_order_json = parse_json(set_order_response).await;
        let order = set_order_json["profile_ids"]
            .as_array()
            .expect("order ids array");
        assert_eq!(order.len(), 2);
        assert_eq!(order[0], secondary_id);
        assert_eq!(order[1], primary_id);

        let get_order_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/auth/agents/default/providers/openai/profile-order",
                Body::empty(),
            ))
            .await
            .expect("get order response");
        assert_eq!(get_order_response.status(), StatusCode::OK);
        let get_order_json = parse_json(get_order_response).await;
        let loaded = get_order_json["profile_ids"]
            .as_array()
            .expect("loaded order ids");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], secondary_id);
        assert_eq!(loaded[1], primary_id);

        let disable_secondary = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/auth/profiles/{secondary_id}/state"),
                Body::from(r#"{"enabled":false,"kill_switch_scope":"profile"}"#),
            ))
            .await
            .expect("disable secondary profile");
        assert_eq!(disable_secondary.status(), StatusCode::OK);

        let list_enabled = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/auth/profiles?provider=openai&include_disabled=false",
                Body::empty(),
            ))
            .await
            .expect("list enabled profiles");
        assert_eq!(list_enabled.status(), StatusCode::OK);
        let list_enabled_json = parse_json(list_enabled).await;
        let enabled_profiles = list_enabled_json["items"]
            .as_array()
            .expect("enabled profile array");
        assert_eq!(enabled_profiles.len(), 1);
        assert_eq!(enabled_profiles[0]["auth_profile_id"], primary_id);
    }

    #[tokio::test]
    async fn rotate_auth_profile_secret_updates_secret_ref_and_deletes_previous_secret() {
        let ctx = test_context();
        let initial_secret_ref = "auth.test.rotate.initial";
        let create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(format!(
                    r#"{{
                        "provider":"openai",
                        "display_name":"rotate-target",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "api_base_url":"https://api.openai.com",
                        "credentials_json":{{"secret_ref":"{initial_secret_ref}","token_kind":"api_key"}}
                    }}"#
                )),
            ))
            .await
            .expect("create rotate profile");
        assert_eq!(create_profile.status(), StatusCode::CREATED);
        let create_profile_json = parse_json(create_profile).await;
        let profile_id = create_profile_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("profile_id")
            .to_string();

        ctx.secret_store
            .set_json(
                initial_secret_ref,
                &serde_json::json!({ "api_key": "rotate-secret" }),
            )
            .expect("seed initial secret");

        let rotate_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/security/auth-profiles/{profile_id}/rotate-secret"),
                Body::from(r#"{"reason":"routine rotation"}"#),
            ))
            .await
            .expect("rotate response");
        assert_eq!(rotate_response.status(), StatusCode::OK);
        let rotate_json = parse_json(rotate_response).await;
        let previous_secret_ref = rotate_json["previous_secret_ref"]
            .as_str()
            .expect("previous_secret_ref");
        let rotated_secret_ref = rotate_json["rotated_secret_ref"]
            .as_str()
            .expect("rotated_secret_ref");
        assert_eq!(previous_secret_ref, initial_secret_ref);
        assert_ne!(rotated_secret_ref, initial_secret_ref);

        assert!(ctx
            .secret_store
            .get_json(initial_secret_ref)
            .expect("load old secret")
            .is_none());
        let rotated_secret = ctx
            .secret_store
            .get_json(rotated_secret_ref)
            .expect("load rotated secret")
            .expect("rotated secret exists");
        assert_eq!(rotated_secret["api_key"], "rotate-secret");

        let stored = ctx
            .storage
            .get_auth_profile(&profile_id)
            .expect("load updated profile")
            .expect("profile exists");
        let stored_payload: serde_json::Value =
            serde_json::from_str(&stored.credentials_json).expect("credentials json");
        assert_eq!(stored_payload["secret_ref"], rotated_secret_ref);

        let audit_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/security/audit?limit=100",
                Body::empty(),
            ))
            .await
            .expect("audit list");
        assert_eq!(audit_response.status(), StatusCode::OK);
        let audit_json = parse_json(audit_response).await;
        let items = audit_json["items"].as_array().expect("audit items");
        assert!(items.iter().any(|item| {
            item["action"] == "security.secret.rotate" && item["decision"] == "allow"
        }));
    }

    #[tokio::test]
    async fn revoke_auth_profile_disables_profile_and_deletes_secret() {
        let ctx = test_context();
        let initial_secret_ref = "auth.test.revoke.initial";
        let create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(format!(
                    r#"{{
                        "provider":"openai",
                        "display_name":"revoke-target",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "api_base_url":"https://api.openai.com",
                        "credentials_json":{{"secret_ref":"{initial_secret_ref}","token_kind":"api_key"}}
                    }}"#
                )),
            ))
            .await
            .expect("create revoke profile");
        assert_eq!(create_profile.status(), StatusCode::CREATED);
        let create_profile_json = parse_json(create_profile).await;
        let profile_id = create_profile_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("profile_id")
            .to_string();

        ctx.secret_store
            .set_json(
                initial_secret_ref,
                &serde_json::json!({ "api_key": "revoke-secret" }),
            )
            .expect("seed revoke secret");

        let revoke_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/security/auth-profiles/{profile_id}/revoke"),
                Body::from(r#"{"reason":"incident","remove_secret":true}"#),
            ))
            .await
            .expect("revoke response");
        assert_eq!(revoke_response.status(), StatusCode::OK);
        let revoke_json = parse_json(revoke_response).await;
        assert_eq!(revoke_json["profile"]["enabled"], false);
        assert_eq!(revoke_json["profile"]["kill_switch_scope"], "profile");
        assert_eq!(revoke_json["revoked_secret_ref"], initial_secret_ref);

        assert!(ctx
            .secret_store
            .get_json(initial_secret_ref)
            .expect("load revoked secret")
            .is_none());

        let create_session = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"revoke-run"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello"}"#),
            ))
            .await
            .expect("create message");
        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from(format!(
                    r#"{{"model_provider":"openai","model_id":"gpt-4o-mini","auth_profile_id":"{profile_id}"}}"#
                )),
            ))
            .await
            .expect("run response");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "failed");
        let error_text = run_json["run"]["error_text"].as_str().unwrap_or_default();
        assert!(
            error_text.contains("requested auth profile is disabled")
                || error_text.contains("no enabled auth profile")
        );

        let audit_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/security/audit?limit=100",
                Body::empty(),
            ))
            .await
            .expect("audit list");
        let audit_json = parse_json(audit_response).await;
        let items = audit_json["items"].as_array().expect("audit items");
        assert!(items.iter().any(|item| {
            item["action"] == "security.secret.revoke" && item["decision"] == "allow"
        }));
    }

    #[tokio::test]
    async fn openai_oauth_pkce_start_finish_and_run_flow_uses_secret_ref() {
        let ctx = test_context();
        let stub = spawn_auth_flow_stub(AuthFlowStubConfig::openai("openai-access-token")).await;

        let start_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/start",
                Body::from(format!(
                    r#"{{
                        "display_name":"openai-oauth",
                        "client_id":"test-client",
                        "scope":"offline_access",
                        "authorize_url":"{}/oauth/authorize",
                        "token_url":"{}/oauth/token",
                        "api_base_url":"{}"
                    }}"#,
                    stub.base_url, stub.base_url, stub.base_url
                )),
            ))
            .await
            .expect("oauth start response");
        assert_eq!(start_response.status(), StatusCode::OK);
        let start_json = parse_json(start_response).await;
        let oauth_session_id = start_json["oauth_session_id"]
            .as_str()
            .expect("oauth session id")
            .to_string();
        let authorize_url = start_json["authorize_url"]
            .as_str()
            .expect("authorize_url")
            .to_string();
        let authorize = Url::parse(&authorize_url).expect("authorize url parse");
        let state = authorize
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string())
            .expect("state query param");

        let callback_url = format!(
            "{}/auth/callback?code=code-123&state={state}",
            stub.base_url
        );
        let finish_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/finish",
                Body::from(format!(
                    r#"{{
                        "oauth_session_id":"{oauth_session_id}",
                        "callback_url":"{callback_url}",
                        "api_base_url":"{}"
                    }}"#,
                    stub.base_url
                )),
            ))
            .await
            .expect("oauth finish response");
        assert_eq!(finish_response.status(), StatusCode::CREATED);
        let finish_json = parse_json(finish_response).await;
        let profile_id = finish_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("oauth profile id")
            .to_string();

        let record = ctx
            .storage
            .get_auth_profile(&profile_id)
            .expect("load profile")
            .expect("profile exists");
        let credentials: serde_json::Value =
            serde_json::from_str(&record.credentials_json).expect("credentials json");
        assert!(credentials
            .get("secret_ref")
            .and_then(|value| value.as_str())
            .is_some());
        assert!(credentials.get("access_token").is_none());
        assert!(credentials.get("refresh_token").is_none());

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"oauth-run"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello oauth"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from(format!(
                    r#"{{"model_provider":"openai","model_id":"gpt-4o-mini","auth_profile_id":"{profile_id}"}}"#
                )),
            ))
            .await
            .expect("run response");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
    }

    #[tokio::test]
    async fn openai_oauth_finish_supports_manual_code_state_fallback() {
        let ctx = test_context();
        let stub = spawn_auth_flow_stub(AuthFlowStubConfig::openai("manual-fallback-token")).await;

        let start_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/start",
                Body::from(format!(
                    r#"{{
                        "display_name":"manual-fallback",
                        "client_id":"test-client",
                        "authorize_url":"{}/oauth/authorize",
                        "token_url":"{}/oauth/token",
                        "api_base_url":"{}"
                    }}"#,
                    stub.base_url, stub.base_url, stub.base_url
                )),
            ))
            .await
            .expect("oauth start");
        let start_json = parse_json(start_response).await;
        let oauth_session_id = start_json["oauth_session_id"]
            .as_str()
            .expect("oauth session id")
            .to_string();
        let authorize = Url::parse(start_json["authorize_url"].as_str().expect("authorize url"))
            .expect("authorize url parse");
        let state = authorize
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string())
            .expect("state query");

        let finish_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/finish",
                Body::from(format!(
                    r#"{{
                        "oauth_session_id":"{oauth_session_id}",
                        "code":"manual-code",
                        "state":"{state}"
                    }}"#
                )),
            ))
            .await
            .expect("oauth finish");
        assert_eq!(finish_response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn openai_oauth_finish_rejects_state_mismatch_and_missing_code() {
        let ctx = test_context();
        let stub = spawn_auth_flow_stub(AuthFlowStubConfig::openai("state-check-token")).await;

        let start_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/start",
                Body::from(format!(
                    r#"{{
                        "client_id":"test-client",
                        "authorize_url":"{}/oauth/authorize",
                        "token_url":"{}/oauth/token",
                        "api_base_url":"{}"
                    }}"#,
                    stub.base_url, stub.base_url, stub.base_url
                )),
            ))
            .await
            .expect("oauth start");
        let start_json = parse_json(start_response).await;
        let oauth_session_id = start_json["oauth_session_id"]
            .as_str()
            .expect("oauth session id");
        let authorize = Url::parse(start_json["authorize_url"].as_str().expect("authorize url"))
            .expect("authorize url parse");
        let good_state = authorize
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string())
            .expect("state query");

        let bad_state_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/finish",
                Body::from(format!(
                    r#"{{
                        "oauth_session_id":"{oauth_session_id}",
                        "code":"manual-code",
                        "state":"wrong-{good_state}"
                    }}"#
                )),
            ))
            .await
            .expect("finish wrong state");
        assert_eq!(bad_state_response.status(), StatusCode::BAD_REQUEST);

        let missing_code_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/finish",
                Body::from(format!(
                    r#"{{
                        "oauth_session_id":"{oauth_session_id}",
                        "callback_url":"{}/auth/callback?state={good_state}"
                    }}"#,
                    stub.base_url
                )),
            ))
            .await
            .expect("finish missing code");
        assert_eq!(missing_code_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn openai_oauth_finish_reports_token_exchange_failures() {
        let ctx = test_context();
        let mut config = AuthFlowStubConfig::openai("unused-token");
        config.token_status = StatusCode::UNAUTHORIZED;
        let stub = spawn_auth_flow_stub(config).await;

        let start_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/start",
                Body::from(format!(
                    r#"{{
                        "client_id":"test-client",
                        "authorize_url":"{}/oauth/authorize",
                        "token_url":"{}/oauth/token",
                        "api_base_url":"{}"
                    }}"#,
                    stub.base_url, stub.base_url, stub.base_url
                )),
            ))
            .await
            .expect("oauth start");
        let start_json = parse_json(start_response).await;
        let oauth_session_id = start_json["oauth_session_id"]
            .as_str()
            .expect("oauth session id");
        let authorize = Url::parse(start_json["authorize_url"].as_str().expect("authorize url"))
            .expect("authorize url parse");
        let state = authorize
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string())
            .expect("state query");

        let finish_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/openai/oauth/finish",
                Body::from(format!(
                    r#"{{
                        "oauth_session_id":"{oauth_session_id}",
                        "callback_url":"{}/auth/callback?code=abc&state={state}"
                    }}"#,
                    stub.base_url
                )),
            ))
            .await
            .expect("oauth finish");
        assert_eq!(finish_response.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn anthropic_setup_token_ingest_creates_profile_and_supports_run_flow() {
        let ctx = test_context();
        let stub = spawn_auth_flow_stub(AuthFlowStubConfig::anthropic("setup-token-123")).await;

        let ingest_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/anthropic/setup-token/ingest",
                Body::from(format!(
                    r#"{{
                        "display_name":"anthropic-setup",
                        "setup_token":"setup-token-123",
                        "api_base_url":"{}"
                    }}"#,
                    stub.base_url
                )),
            ))
            .await
            .expect("ingest setup token");
        assert_eq!(ingest_response.status(), StatusCode::CREATED);
        let ingest_json = parse_json(ingest_response).await;
        let profile_id = ingest_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("profile id")
            .to_string();

        let record = ctx
            .storage
            .get_auth_profile(&profile_id)
            .expect("load profile")
            .expect("profile exists");
        let credentials: serde_json::Value =
            serde_json::from_str(&record.credentials_json).expect("credentials json");
        assert!(credentials
            .get("secret_ref")
            .and_then(|value| value.as_str())
            .is_some());
        assert!(credentials.get("api_key").is_none());

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"anthropic-setup-run"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello anthropic setup token"}"#),
            ))
            .await
            .expect("create message");

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from(format!(
                    r#"{{"model_provider":"anthropic","model_id":"claude-test","auth_profile_id":"{profile_id}"}}"#
                )),
            ))
            .await
            .expect("run response");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "succeeded");
    }

    #[tokio::test]
    async fn channel_config_endpoints_round_trip() {
        let ctx = test_context();
        let get_default = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/config/channels",
                Body::empty(),
            ))
            .await
            .expect("get channel config");
        assert_eq!(get_default.status(), StatusCode::OK);
        let default_json = parse_json(get_default).await;
        assert_eq!(
            default_json["config"]["discord"]["require_mention_in_guild_channels"],
            true
        );
        assert_eq!(
            default_json["config"]["telegram"]["require_mention_in_groups"],
            true
        );
        assert_eq!(default_json["config"]["discord"]["auto_run_enabled"], true);
        assert_eq!(default_json["config"]["telegram"]["auto_run_enabled"], true);

        let update = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/config/channels",
                Body::from(
                    r#"{
                        "discord":{
                            "require_mention_in_guild_channels":false,
                            "allowlisted_user_ids":["du1","du2"],
                            "auto_run_enabled":false,
                            "default_model_provider":"mock",
                            "default_model_id":"mock-echo-v1"
                        },
                        "telegram":{
                            "require_mention_in_groups":false,
                            "allowlisted_user_ids":[1001,1002],
                            "auto_run_enabled":true,
                            "default_model_provider":"mock",
                            "default_model_id":"mock-echo-v1"
                        }
                    }"#,
                ),
            ))
            .await
            .expect("update channel config");
        assert_eq!(update.status(), StatusCode::OK);
        let update_json = parse_json(update).await;
        assert_eq!(
            update_json["config"]["discord"]["require_mention_in_guild_channels"],
            false
        );
        assert_eq!(
            update_json["config"]["telegram"]["require_mention_in_groups"],
            false
        );
        assert_eq!(update_json["config"]["discord"]["auto_run_enabled"], false);
        assert_eq!(update_json["config"]["telegram"]["auto_run_enabled"], true);
        let updated_at = update_json["config"]["updated_at"]
            .as_i64()
            .expect("updated_at");
        assert!(updated_at > 0);

        let get_updated = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/config/channels",
                Body::empty(),
            ))
            .await
            .expect("get updated config");
        assert_eq!(get_updated.status(), StatusCode::OK);
        let updated_json = parse_json(get_updated).await;
        assert_eq!(
            updated_json["config"]["discord"]["allowlisted_user_ids"][0],
            "du1"
        );
        assert_eq!(
            updated_json["config"]["telegram"]["allowlisted_user_ids"][1],
            1002
        );
    }

    #[tokio::test]
    async fn telegram_channel_inbound_rejects_non_allowlisted_sender() {
        let ctx = test_context();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/config/channels",
                Body::from(
                    r#"{
                        "telegram":{"require_mention_in_groups":false,"allowlisted_user_ids":[1001]}
                    }"#,
                ),
            ))
            .await
            .expect("update channel config");

        let inbound = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/telegram/inbound",
                Body::from(
                    r#"{
                        "chat_id":123,
                        "user_id":999,
                        "text":"hello",
                        "is_group_chat":false,
                        "mentions_bot":false,
                        "reply_to_bot":false,
                        "run_immediately":false
                    }"#,
                ),
            ))
            .await
            .expect("telegram inbound");
        assert_eq!(inbound.status(), StatusCode::OK);
        let inbound_json = parse_json(inbound).await;
        assert_eq!(inbound_json["decision"], "rejected");
        assert_eq!(inbound_json["reason"], "sender_not_allowlisted");
        assert!(inbound_json["session_id"].is_null());
        assert!(inbound_json["run_id"].is_null());
    }

    #[tokio::test]
    async fn telegram_channel_inbound_accept_reuses_session_key() {
        let ctx = test_context();
        let first = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/telegram/inbound",
                Body::from(
                    r#"{
                        "chat_id":777,
                        "user_id":42,
                        "text":"first",
                        "is_group_chat":true,
                        "mentions_bot":true,
                        "reply_to_bot":false,
                        "run_immediately":false
                    }"#,
                ),
            ))
            .await
            .expect("telegram inbound first");
        assert_eq!(first.status(), StatusCode::OK);
        let first_json = parse_json(first).await;
        assert_eq!(first_json["decision"], "accepted");
        let session_id = first_json["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let second = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/telegram/inbound",
                Body::from(
                    r#"{
                        "chat_id":777,
                        "user_id":42,
                        "text":"second",
                        "is_group_chat":true,
                        "mentions_bot":true,
                        "reply_to_bot":false,
                        "run_immediately":false
                    }"#,
                ),
            ))
            .await
            .expect("telegram inbound second");
        assert_eq!(second.status(), StatusCode::OK);
        let second_json = parse_json(second).await;
        assert_eq!(second_json["decision"], "accepted");
        assert_eq!(second_json["session_id"], session_id);
        assert!(second_json["run_id"].is_null());

        let messages = ctx
            .storage
            .list_messages(&session_id, 10)
            .expect("list messages for channel session");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content_text, "first");
        assert_eq!(messages[1].content_text, "second");
    }

    #[tokio::test]
    async fn discord_channel_inbound_ignores_guild_message_without_mention() {
        let ctx = test_context();
        let inbound = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/discord/inbound",
                Body::from(
                    r#"{
                        "guild_id":"g1",
                        "channel_id":"c1",
                        "author_id":"u1",
                        "text":"hello",
                        "mentions_bot":false,
                        "is_dm":false,
                        "run_immediately":false
                    }"#,
                ),
            ))
            .await
            .expect("discord inbound");
        assert_eq!(inbound.status(), StatusCode::OK);
        let inbound_json = parse_json(inbound).await;
        assert_eq!(inbound_json["decision"], "ignored");
        assert_eq!(inbound_json["reason"], "mention_required_in_guild_channel");
        assert!(inbound_json["session_id"].is_null());
    }

    #[tokio::test]
    async fn discord_channel_inbound_can_trigger_run_execution() {
        let ctx = test_context();
        let inbound = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/discord/inbound",
                Body::from(
                    r#"{
                        "channel_id":"dm-c1",
                        "author_id":"u-run",
                        "text":"run now",
                        "mentions_bot":false,
                        "is_dm":true,
                        "run_immediately":true,
                        "model_provider":"mock",
                        "model_id":"mock-echo-v1"
                    }"#,
                ),
            ))
            .await
            .expect("discord inbound run");
        assert_eq!(inbound.status(), StatusCode::OK);
        let inbound_json = parse_json(inbound).await;
        assert_eq!(inbound_json["decision"], "accepted");
        let run_id = inbound_json["run_id"].as_str().expect("run id");
        let run = ctx
            .storage
            .get_run(run_id)
            .expect("get run")
            .expect("run exists");
        assert_eq!(run.status, "succeeded");
    }

    #[tokio::test]
    async fn discord_channel_inbound_respects_auto_run_disabled_default() {
        let ctx = test_context();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/config/channels",
                Body::from(
                    r#"{
                        "discord":{
                            "require_mention_in_guild_channels":false,
                            "allowlisted_user_ids":[],
                            "auto_run_enabled":false,
                            "default_model_provider":"mock",
                            "default_model_id":"mock-echo-v1"
                        }
                    }"#,
                ),
            ))
            .await
            .expect("update channel config");

        let inbound = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/discord/inbound",
                Body::from(
                    r#"{
                        "channel_id":"dm-no-auto-run",
                        "author_id":"u-no-auto-run",
                        "text":"no auto run",
                        "mentions_bot":false,
                        "is_dm":true
                    }"#,
                ),
            ))
            .await
            .expect("discord inbound");
        assert_eq!(inbound.status(), StatusCode::OK);
        let inbound_json = parse_json(inbound).await;
        assert_eq!(inbound_json["decision"], "accepted");
        assert!(inbound_json["run_id"].is_null());
    }

    #[tokio::test]
    async fn discord_channel_inbound_can_override_disabled_auto_run() {
        let ctx = test_context();
        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/config/channels",
                Body::from(
                    r#"{
                        "discord":{
                            "require_mention_in_guild_channels":false,
                            "allowlisted_user_ids":[],
                            "auto_run_enabled":false,
                            "default_model_provider":"mock",
                            "default_model_id":"mock-echo-v1"
                        }
                    }"#,
                ),
            ))
            .await
            .expect("update channel config");

        let inbound = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/discord/inbound",
                Body::from(
                    r#"{
                        "channel_id":"dm-override-auto-run",
                        "author_id":"u-override-auto-run",
                        "text":"force run",
                        "mentions_bot":false,
                        "is_dm":true,
                        "run_immediately":true
                    }"#,
                ),
            ))
            .await
            .expect("discord inbound");
        assert_eq!(inbound.status(), StatusCode::OK);
        let inbound_json = parse_json(inbound).await;
        assert_eq!(inbound_json["decision"], "accepted");
        let run_id = inbound_json["run_id"].as_str().expect("run id");
        let run = ctx
            .storage
            .get_run(run_id)
            .expect("get run")
            .expect("run exists");
        assert_eq!(run.status, "succeeded");
    }

    #[tokio::test]
    async fn high_risk_auth_profile_requires_kill_switch() {
        let ctx = test_context();

        let bad_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"anthropic",
                        "display_name":"bad-high-risk",
                        "auth_mode":"claude_consumer_oauth",
                        "risk_level":"high",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{"token":"x"}
                    }"#,
                ),
            ))
            .await
            .expect("create high-risk profile with missing kill-switch");
        assert_eq!(bad_response.status(), StatusCode::BAD_REQUEST);

        let good_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"anthropic",
                        "display_name":"good-high-risk",
                        "auth_mode":"claude_consumer_oauth",
                        "risk_level":"high",
                        "enabled":true,
                        "kill_switch_scope":"profile",
                        "credentials_json":{"token":"x"}
                    }"#,
                ),
            ))
            .await
            .expect("create high-risk profile with kill-switch");
        assert_eq!(good_response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn provider_expansion_profiles_enforce_auth_mode_allowlist() {
        let ctx = test_context();

        let openrouter_ok = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"openrouter",
                        "display_name":"openrouter-key",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{"api_key":"or-key"}
                    }"#,
                ),
            ))
            .await
            .expect("create openrouter profile");
        assert_eq!(openrouter_ok.status(), StatusCode::CREATED);

        let openrouter_reject = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"openrouter",
                        "display_name":"openrouter-oauth",
                        "auth_mode":"openai_oauth",
                        "risk_level":"medium",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{"access_token":"x"}
                    }"#,
                ),
            ))
            .await
            .expect("create invalid openrouter profile");
        assert_eq!(openrouter_reject.status(), StatusCode::BAD_REQUEST);

        let ollama_ok = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"ollama",
                        "display_name":"ollama-key",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{"api_key":"ollama-token"}
                    }"#,
                ),
            ))
            .await
            .expect("create ollama profile");
        assert_eq!(ollama_ok.status(), StatusCode::CREATED);

        let vllm_ok = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"vllm",
                        "display_name":"vllm-key",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{"api_key":"vllm-token"}
                    }"#,
                ),
            ))
            .await
            .expect("create vllm profile");
        assert_eq!(vllm_ok.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn expired_requested_oauth_profile_fails_before_provider_call() {
        let ctx = test_context();

        let create_session = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"expired-oauth"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello"}"#),
            ))
            .await
            .expect("create message");

        let create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"openai",
                        "display_name":"expired-openai-oauth",
                        "auth_mode":"openai_oauth",
                        "risk_level":"medium",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{"access_token":"old","refresh_token":"rt","expires_at_unix":1}
                    }"#,
                ),
            ))
            .await
            .expect("create profile");
        assert_eq!(create_profile.status(), StatusCode::CREATED);
        let create_profile_json = parse_json(create_profile).await;
        let profile_id = create_profile_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("profile id")
            .to_string();

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from(format!(
                    r#"{{"model_provider":"openai","model_id":"gpt-4o-mini","auth_profile_id":"{profile_id}"}}"#
                )),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "failed");
        assert!(run_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("credentials expired"));
    }

    #[tokio::test]
    async fn provider_kill_switch_blocks_run_execution() {
        let ctx = test_context();

        let create_session = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"kill-switch-run"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hello"}"#),
            ))
            .await
            .expect("create message");

        let create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(
                    r#"{
                        "provider":"openai",
                        "display_name":"provider-kill-switch",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"provider",
                        "credentials_json":{"api_key":"test"}
                    }"#,
                ),
            ))
            .await
            .expect("create profile");
        assert_eq!(create_profile.status(), StatusCode::CREATED);

        let run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from(r#"{"model_provider":"openai","model_id":"gpt-4o-mini"}"#),
            ))
            .await
            .expect("create run");
        assert_eq!(run_response.status(), StatusCode::CREATED);
        let run_json = parse_json(run_response).await;
        assert_eq!(run_json["run"]["status"], "failed");
        assert!(run_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("provider kill-switch active"));
    }

    #[tokio::test]
    async fn jobs_endpoints_lifecycle_and_history_work() {
        let ctx = test_context();

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/jobs/add",
                Body::from(
                    r#"{
                        "agent_id":"default",
                        "name":"job-lifecycle",
                        "enabled":true,
                        "schedule_kind":"interval",
                        "interval_seconds":60,
                        "payload_json":{"mode":"noop","message":"hello"},
                        "max_retries":1,
                        "retry_backoff_ms":25,
                        "timeout_ms":500
                    }"#,
                ),
            ))
            .await
            .expect("create job response");
        assert_eq!(create_response.status(), StatusCode::CREATED);
        let create_json = parse_json(create_response).await;
        let job_id = create_json["job"]["job_id"]
            .as_str()
            .expect("job_id")
            .to_string();

        let list_response = ctx
            .app
            .clone()
            .oneshot(auth_request("GET", "/api/v1/jobs?limit=20", Body::empty()))
            .await
            .expect("list jobs response");
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_json = parse_json(list_response).await;
        let jobs = list_json["items"].as_array().expect("jobs array");
        assert!(jobs.iter().any(|item| item["job_id"] == job_id));

        let status_response = ctx
            .app
            .clone()
            .oneshot(auth_request("GET", "/api/v1/jobs/status", Body::empty()))
            .await
            .expect("job status response");
        assert_eq!(status_response.status(), StatusCode::OK);
        let status_json = parse_json(status_response).await;
        assert!(status_json["jobs_total"].as_u64().unwrap_or(0) >= 1);

        let run_now_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/run"),
                Body::empty(),
            ))
            .await
            .expect("run now response");
        assert_eq!(run_now_response.status(), StatusCode::OK);
        let run_now_json = parse_json(run_now_response).await;
        assert_eq!(run_now_json["job_run"]["status"], "succeeded");

        let history_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/jobs/{job_id}/history?limit=10"),
                Body::empty(),
            ))
            .await
            .expect("job history response");
        assert_eq!(history_response.status(), StatusCode::OK);
        let history_json = parse_json(history_response).await;
        let history_items = history_json["items"].as_array().expect("history items");
        assert_eq!(history_items.len(), 1);
        assert_eq!(history_items[0]["status"], "succeeded");

        let update_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/update"),
                Body::from(r#"{"enabled":false}"#),
            ))
            .await
            .expect("update job response");
        assert_eq!(update_response.status(), StatusCode::OK);
        let update_json = parse_json(update_response).await;
        assert_eq!(update_json["job"]["enabled"], false);

        let remove_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/remove"),
                Body::empty(),
            ))
            .await
            .expect("remove job response");
        assert_eq!(remove_response.status(), StatusCode::OK);
        let remove_json = parse_json(remove_response).await;
        assert_eq!(remove_json["removed"], true);
    }

    #[tokio::test]
    async fn run_now_retries_until_payload_succeeds() {
        let ctx = test_context();

        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/jobs/add",
                Body::from(
                    r#"{
                        "agent_id":"default",
                        "name":"job-retry",
                        "enabled":true,
                        "schedule_kind":"interval",
                        "interval_seconds":60,
                        "payload_json":{"mode":"noop","fail_until_attempt":2},
                        "max_retries":3,
                        "retry_backoff_ms":5,
                        "timeout_ms":500
                    }"#,
                ),
            ))
            .await
            .expect("create retry job");
        assert_eq!(create_response.status(), StatusCode::CREATED);
        let create_json = parse_json(create_response).await;
        let job_id = create_json["job"]["job_id"]
            .as_str()
            .expect("job_id")
            .to_string();

        let run_now_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/run"),
                Body::empty(),
            ))
            .await
            .expect("run now response");
        assert_eq!(run_now_response.status(), StatusCode::OK);
        let run_json = parse_json(run_now_response).await;
        assert_eq!(run_json["job_run"]["status"], "succeeded");
        assert_eq!(run_json["job_run"]["attempt"], 3);
    }

    #[tokio::test]
    async fn run_now_session_run_payload_executes_real_run_path() {
        let ctx = test_context();
        let session_key = "scheduler:test:session-run";
        let create_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/jobs/add",
                Body::from(format!(
                    r#"{{
                        "agent_id":"default",
                        "name":"job-session-run",
                        "enabled":true,
                        "schedule_kind":"interval",
                        "interval_seconds":60,
                        "payload_json":{{
                            "mode":"session.run",
                            "session_key":"{session_key}",
                            "session_title":"scheduler session run",
                            "input":"hello from scheduler session run",
                            "model_provider":"mock",
                            "model_id":"mock-echo-v1"
                        }},
                        "max_retries":1,
                        "retry_backoff_ms":5,
                        "timeout_ms":1000
                    }}"#
                )),
            ))
            .await
            .expect("create session-run job");
        assert_eq!(create_response.status(), StatusCode::CREATED);
        let create_json = parse_json(create_response).await;
        let job_id = create_json["job"]["job_id"]
            .as_str()
            .expect("job_id")
            .to_string();

        let run_now_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/run"),
                Body::empty(),
            ))
            .await
            .expect("run session-run job");
        assert_eq!(run_now_response.status(), StatusCode::OK);
        let run_json = parse_json(run_now_response).await;
        assert_eq!(run_json["job_run"]["status"], "succeeded");

        let output_json = run_json["job_run"]["output_json"]
            .as_str()
            .expect("job run output_json");
        let output: serde_json::Value =
            serde_json::from_str(output_json).expect("parse output_json as json");
        assert_eq!(output["mode"], "session.run");
        assert_eq!(output["run_status"], "succeeded");
        let output_session_id = output["session_id"]
            .as_str()
            .expect("output session_id")
            .to_string();

        let persisted_session = ctx
            .storage
            .get_session_by_key(session_key)
            .expect("lookup session by key")
            .expect("session should exist");
        assert_eq!(persisted_session.session_id, output_session_id);
        let messages = ctx
            .storage
            .list_messages(&persisted_session.session_id, 50)
            .expect("list messages");
        assert!(
            messages.iter().any(|item| item.role == "user"),
            "expected user message from scheduler payload"
        );
        assert!(
            messages.iter().any(|item| item.role == "assistant"),
            "expected assistant message from executed run"
        );
    }

    #[tokio::test]
    async fn scheduled_secret_rotation_job_rotates_ref_without_secret_leakage() {
        let ctx = test_context();
        let initial_secret_ref = "auth.test.schedule.rotate.initial";

        let create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(format!(
                    r#"{{
                        "provider":"openai",
                        "display_name":"scheduled-rotate-target",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{{"secret_ref":"{initial_secret_ref}","token_kind":"api_key"}}
                    }}"#
                )),
            ))
            .await
            .expect("create scheduled rotate profile");
        assert_eq!(create_profile.status(), StatusCode::CREATED);
        let create_profile_json = parse_json(create_profile).await;
        let profile_id = create_profile_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("profile_id")
            .to_string();

        ctx.secret_store
            .set_json(
                initial_secret_ref,
                &serde_json::json!({ "api_key": "scheduled-rotate-secret" }),
            )
            .expect("seed rotate secret");

        let create_job = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/jobs/add",
                Body::from(format!(
                    r#"{{
                        "agent_id":"default",
                        "name":"scheduled-secret-rotate",
                        "enabled":true,
                        "schedule_kind":"interval",
                        "interval_seconds":60,
                        "payload_json":{{
                            "mode":"secret.rotate_profile",
                            "auth_profile_id":"{profile_id}",
                            "reason":"scheduled cadence rotation"
                        }},
                        "max_retries":1,
                        "retry_backoff_ms":5,
                        "timeout_ms":500
                    }}"#
                )),
            ))
            .await
            .expect("create rotate job");
        assert_eq!(create_job.status(), StatusCode::CREATED);
        let create_job_json = parse_json(create_job).await;
        let job_id = create_job_json["job"]["job_id"]
            .as_str()
            .expect("job_id")
            .to_string();

        let run_now = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/run"),
                Body::empty(),
            ))
            .await
            .expect("run rotate job");
        assert_eq!(run_now.status(), StatusCode::OK);
        let run_now_json = parse_json(run_now).await;
        assert_eq!(run_now_json["job_run"]["status"], "succeeded");

        let output_json = run_now_json["job_run"]["output_json"]
            .as_str()
            .expect("job output json");
        assert!(!output_json.contains("scheduled-rotate-secret"));
        let output = serde_json::from_str::<serde_json::Value>(output_json).expect("output value");
        let rotated_secret_ref = output["rotated_secret_ref"]
            .as_str()
            .expect("rotated_secret_ref");
        assert_ne!(rotated_secret_ref, initial_secret_ref);

        let updated_profile = ctx
            .storage
            .get_auth_profile(&profile_id)
            .expect("load rotated profile")
            .expect("rotated profile exists");
        let updated_credentials = auth_profile_credentials_payload(&updated_profile)
            .expect("rotated profile credentials");
        assert_eq!(updated_credentials["secret_ref"], rotated_secret_ref);
        assert!(ctx
            .secret_store
            .get_json(initial_secret_ref)
            .expect("load old secret")
            .is_none());
        assert_eq!(
            ctx.secret_store
                .get_json(rotated_secret_ref)
                .expect("load rotated secret")
                .expect("rotated secret exists")["api_key"],
            "scheduled-rotate-secret"
        );

        let audit_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/security/audit?action=security.secret.rotate&limit=20",
                Body::empty(),
            ))
            .await
            .expect("audit response");
        assert_eq!(audit_response.status(), StatusCode::OK);
        let audit_json = parse_json(audit_response).await;
        let items = audit_json["items"].as_array().expect("audit items");
        assert!(items.iter().any(|item| {
            item["action"] == "security.secret.rotate"
                && item["decision"] == "allow"
                && item["principal"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("scheduler:")
        }));
    }

    #[tokio::test]
    async fn scheduled_secret_revoke_job_disables_profile_and_deletes_secret() {
        let ctx = test_context();
        let initial_secret_ref = "auth.test.schedule.revoke.initial";

        let create_profile = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/auth/profiles",
                Body::from(format!(
                    r#"{{
                        "provider":"openai",
                        "display_name":"scheduled-revoke-target",
                        "auth_mode":"api_key",
                        "risk_level":"low",
                        "enabled":true,
                        "kill_switch_scope":"none",
                        "credentials_json":{{"secret_ref":"{initial_secret_ref}","token_kind":"api_key"}}
                    }}"#
                )),
            ))
            .await
            .expect("create scheduled revoke profile");
        assert_eq!(create_profile.status(), StatusCode::CREATED);
        let create_profile_json = parse_json(create_profile).await;
        let profile_id = create_profile_json["profile"]["auth_profile_id"]
            .as_str()
            .expect("profile_id")
            .to_string();

        ctx.secret_store
            .set_json(
                initial_secret_ref,
                &serde_json::json!({ "api_key": "scheduled-revoke-secret" }),
            )
            .expect("seed revoke secret");

        let create_job = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/jobs/add",
                Body::from(format!(
                    r#"{{
                        "agent_id":"default",
                        "name":"scheduled-secret-revoke",
                        "enabled":true,
                        "schedule_kind":"interval",
                        "interval_seconds":60,
                        "payload_json":{{
                            "mode":"secret.revoke_profile",
                            "auth_profile_id":"{profile_id}",
                            "reason":"scheduled revocation",
                            "remove_secret":true,
                            "disable_profile":true
                        }},
                        "max_retries":1,
                        "retry_backoff_ms":5,
                        "timeout_ms":500
                    }}"#
                )),
            ))
            .await
            .expect("create revoke job");
        assert_eq!(create_job.status(), StatusCode::CREATED);
        let create_job_json = parse_json(create_job).await;
        let job_id = create_job_json["job"]["job_id"]
            .as_str()
            .expect("job_id")
            .to_string();

        let run_now = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/jobs/{job_id}/run"),
                Body::empty(),
            ))
            .await
            .expect("run revoke job");
        assert_eq!(run_now.status(), StatusCode::OK);
        let run_now_json = parse_json(run_now).await;
        assert_eq!(run_now_json["job_run"]["status"], "succeeded");
        let output_json = run_now_json["job_run"]["output_json"]
            .as_str()
            .expect("job output json");
        assert!(!output_json.contains("scheduled-revoke-secret"));

        let updated_profile = ctx
            .storage
            .get_auth_profile(&profile_id)
            .expect("load revoked profile")
            .expect("revoked profile exists");
        assert!(!updated_profile.enabled);
        assert_eq!(updated_profile.kill_switch_scope, "profile");
        assert!(ctx
            .secret_store
            .get_json(initial_secret_ref)
            .expect("load revoked secret")
            .is_none());

        let audit_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/security/audit?action=security.secret.revoke&limit=20",
                Body::empty(),
            ))
            .await
            .expect("audit response");
        assert_eq!(audit_response.status(), StatusCode::OK);
        let audit_json = parse_json(audit_response).await;
        let items = audit_json["items"].as_array().expect("audit items");
        assert!(items.iter().any(|item| {
            item["action"] == "security.secret.revoke"
                && item["decision"] == "allow"
                && item["principal"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("scheduler:")
        }));
    }

    #[tokio::test]
    async fn approval_request_list_and_resolve_flow() {
        let ctx = test_context();

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"approval-flow"}"#),
            ))
            .await
            .expect("create session response");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"need approval"}"#),
            ))
            .await
            .expect("create user message");

        let create_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("create run");
        let create_run_json = parse_json(create_run_response).await;
        let run_id = create_run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();

        let create_approval_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"execute shell","request_json":{{"command":"echo hi"}}}}"#
                )),
            ))
            .await
            .expect("create approval");
        assert_eq!(create_approval_response.status(), StatusCode::CREATED);
        let create_approval_json = parse_json(create_approval_response).await;
        let approval_id = create_approval_json["approval"]["approval_id"]
            .as_str()
            .expect("approval_id")
            .to_string();
        assert_eq!(create_approval_json["approval"]["status"], "requested");

        let pending_approvals_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/approvals?status=requested",
                Body::empty(),
            ))
            .await
            .expect("list approvals");
        let pending_approvals_json = parse_json(pending_approvals_response).await;
        let pending_items = pending_approvals_json["items"]
            .as_array()
            .expect("items array");
        assert!(pending_items
            .iter()
            .any(|item| item["approval_id"] == approval_id));

        let resolve_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/approvals/{approval_id}/resolve"),
                Body::from(r#"{"decision":"approve","decided_via":"gui"}"#),
            ))
            .await
            .expect("resolve approval");
        assert_eq!(resolve_response.status(), StatusCode::OK);
        let resolve_json = parse_json(resolve_response).await;
        assert_eq!(resolve_json["approval"]["status"], "approved");

        let resolve_again_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/approvals/{approval_id}/resolve"),
                Body::from(r#"{"decision":"deny"}"#),
            ))
            .await
            .expect("resolve approval again");
        assert_eq!(resolve_again_response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn approval_resolve_is_race_safe() {
        let ctx = test_context();

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"approval-race"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hi"}"#),
            ))
            .await
            .expect("message");

        let create_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("run");
        let create_run_json = parse_json(create_run_response).await;
        let run_id = create_run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();

        let create_approval_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"race","request_json":{{"command":"echo hi"}}}}"#
                )),
            ))
            .await
            .expect("approval");
        let create_approval_json = parse_json(create_approval_response).await;
        let approval_id = create_approval_json["approval"]["approval_id"]
            .as_str()
            .expect("approval_id")
            .to_string();

        let left = ctx.app.clone().oneshot(auth_request(
            "POST",
            &format!("/api/v1/approvals/{approval_id}/resolve"),
            Body::from(r#"{"decision":"approve","decided_via":"gui"}"#),
        ));
        let right = ctx.app.clone().oneshot(auth_request(
            "POST",
            &format!("/api/v1/approvals/{approval_id}/resolve"),
            Body::from(r#"{"decision":"approve","decided_via":"discord"}"#),
        ));
        let (left_response, right_response) = tokio::join!(left, right);
        let left_status = left_response.expect("left").status();
        let right_status = right_response.expect("right").status();

        assert!(
            (left_status == StatusCode::OK && right_status == StatusCode::CONFLICT)
                || (left_status == StatusCode::CONFLICT && right_status == StatusCode::OK)
        );
    }

    #[tokio::test]
    async fn approval_actions_require_allowlisted_operator_when_configured() {
        let ctx = test_context_with_allowlist(vec!["op-1".to_string()]);

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"approval-auth"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"hi"}"#),
            ))
            .await
            .expect("message");

        let create_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("run");
        let create_run_json = parse_json(create_run_response).await;
        let run_id = create_run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();

        let forbidden_missing_header = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"auth","request_json":{{"command":"echo hi"}}}}"#
                )),
            ))
            .await
            .expect("approval request without operator");
        assert_eq!(forbidden_missing_header.status(), StatusCode::FORBIDDEN);

        let forbidden_wrong_operator = ctx
            .app
            .clone()
            .oneshot(auth_request_with_operator(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"auth","request_json":{{"command":"echo hi"}}}}"#
                )),
                "op-2",
            ))
            .await
            .expect("approval request wrong operator");
        assert_eq!(forbidden_wrong_operator.status(), StatusCode::FORBIDDEN);

        let allowed_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_operator(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"auth","request_json":{{"command":"echo hi"}}}}"#
                )),
                "op-1",
            ))
            .await
            .expect("approval request allowlisted operator");
        assert_eq!(allowed_response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn channel_approval_action_resolves_discord_payload() {
        let ctx = test_context_with_allowlist(vec!["op-1".to_string()]);

        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"channel-approval"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"approve me"}"#),
            ))
            .await
            .expect("message");

        let create_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("run");
        let create_run_json = parse_json(create_run_response).await;
        let run_id = create_run_json["run"]["run_id"]
            .as_str()
            .expect("run_id")
            .to_string();

        let create_approval_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_operator(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"channel","request_json":{{"command":"echo hi"}}}}"#
                )),
                "op-1",
            ))
            .await
            .expect("create approval");
        assert_eq!(create_approval_response.status(), StatusCode::CREATED);
        let create_approval_json = parse_json(create_approval_response).await;
        let approval_id = create_approval_json["approval"]["approval_id"]
            .as_str()
            .expect("approval id");
        let action_payload =
            discord_channel::approval_custom_id(approval_id, "approve").expect("custom id");

        let resolve_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/approvals/resolve",
                Body::from(format!(
                    r#"{{"provider":"discord","action_payload":"{action_payload}","actor_peer_id":"op-1"}}"#
                )),
            ))
            .await
            .expect("channel approval resolve");
        assert_eq!(resolve_response.status(), StatusCode::OK);
        let resolve_json = parse_json(resolve_response).await;
        assert_eq!(resolve_json["approval"]["status"], "approved");
        assert_eq!(resolve_json["approval"]["decided_via"], "discord");
        assert_eq!(resolve_json["approval"]["decided_by_peer_id"], "op-1");
    }

    #[tokio::test]
    async fn channel_approval_action_rejects_invalid_payload() {
        let ctx = test_context();
        let response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/approvals/resolve",
                Body::from(
                    r#"{"provider":"discord","action_payload":"not-a-custom-id","actor_peer_id":"op-1"}"#,
                ),
            ))
            .await
            .expect("channel resolve response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn channel_approval_action_respects_operator_allowlist() {
        let ctx = test_context_with_allowlist(vec!["op-1".to_string()]);
        let create_session_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/sessions",
                Body::from(r#"{"title":"channel-approval-allowlist"}"#),
            ))
            .await
            .expect("create session");
        let create_session_json = parse_json(create_session_response).await;
        let session_id = create_session_json["session"]["session_id"]
            .as_str()
            .expect("session id")
            .to_string();

        let _ = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/messages"),
                Body::from(r#"{"role":"user","content_text":"approve"}"#),
            ))
            .await
            .expect("message");
        let create_run_response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/sessions/{session_id}/runs"),
                Body::from("{}"),
            ))
            .await
            .expect("run");
        let create_run_json = parse_json(create_run_response).await;
        let run_id = create_run_json["run"]["run_id"]
            .as_str()
            .expect("run id")
            .to_string();
        let create_approval_response = ctx
            .app
            .clone()
            .oneshot(auth_request_with_operator(
                "POST",
                "/api/v1/approvals/request",
                Body::from(format!(
                    r#"{{"run_id":"{run_id}","tool_name":"exec","request_summary":"channel","request_json":{{"command":"echo hi"}}}}"#
                )),
                "op-1",
            ))
            .await
            .expect("create approval");
        let create_approval_json = parse_json(create_approval_response).await;
        let approval_id = create_approval_json["approval"]["approval_id"]
            .as_str()
            .expect("approval id");
        let action_payload =
            telegram_channel::approval_callback_payload(approval_id, "deny").expect("payload");

        let forbidden = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/channels/approvals/resolve",
                Body::from(format!(
                    r#"{{"provider":"telegram","action_payload":"{action_payload}","actor_peer_id":"op-2"}}"#
                )),
            ))
            .await
            .expect("channel resolve forbidden");
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn provider_capabilities_list_returns_contract_v2() {
        let ctx = test_context();
        let response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/providers/capabilities",
                Body::empty(),
            ))
            .await
            .expect("provider capabilities list response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = parse_json(response).await;
        assert_eq!(body["contract_version"], "v2");
        let items = body["items"].as_array().expect("capabilities items array");
        assert!(items.iter().any(|item| item["provider"] == "openai"));
        assert!(items.iter().any(|item| item["provider"] == "anthropic"));
        assert!(items.iter().any(|item| item["provider"] == "openrouter"));
        assert!(items.iter().any(|item| item["provider"] == "ollama"));
        assert!(items.iter().any(|item| item["provider"] == "vllm"));
    }

    #[tokio::test]
    async fn provider_capabilities_filter_rejects_unknown_provider() {
        let ctx = test_context();
        let response = ctx
            .app
            .clone()
            .oneshot(auth_request(
                "GET",
                "/api/v1/providers/capabilities?provider=unknown",
                Body::empty(),
            ))
            .await
            .expect("provider capabilities filtered response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
