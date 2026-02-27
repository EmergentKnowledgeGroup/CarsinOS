use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use std::path::Path;

pub const JOB_MODE_HEARTBEAT_RUN: &str = "heartbeat.run";
pub const HEARTBEAT_OUTPUT_OK: &str = "HEARTBEAT_OK";
pub const HEARTBEAT_OUTPUT_ALERT_PREFIX: &str = "ALERT:";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SanitizedPath(String);

impl SanitizedPath {
    pub fn from_raw(raw: impl AsRef<str>) -> Self {
        let value = raw.as_ref().trim();
        if value.is_empty() {
            return Self("<unset>".to_string());
        }
        let path = Path::new(value);
        if path.is_absolute() {
            if let Some(name) = path.file_name().and_then(|item| item.to_str()) {
                return Self(format!("<redacted>/{name}"));
            }
            return Self("<redacted>".to_string());
        }
        Self(value.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SanitizedPath {
    fn from(value: String) -> Self {
        Self::from_raw(value)
    }
}

impl From<&str> for SanitizedPath {
    fn from(value: &str) -> Self {
        Self::from_raw(value)
    }
}

impl Serialize for SanitizedPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub version: String,
    pub uptime_ms: u64,
    pub now_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub service: String,
    pub version: String,
    pub started_at_utc: DateTime<Utc>,
    pub uptime_ms: u64,
    pub db_path: String,
    pub attachments_path: String,
    pub trust_contract_lock: RuntimeTrustContractLockSummaryResponse,
    pub scheduler_lock: SchedulerLockStateResponse,
    pub autonomy_guardrails: RuntimeAutonomyGuardrailsConfig,
    pub numquam: NumquamIntegrationStatusResponse,
    pub open_circuit_breakers: u64,
    pub circuit_breakers: Vec<CircuitBreakerStateResponse>,
    pub open_plugin_breakers: u64,
    pub plugin_breakers: Vec<PluginRuntimeStatusResponse>,
    pub top_stop_reasons: Vec<FailureReasonCountResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeTrustContractLockSummaryResponse {
    pub enforced: bool,
    pub lock_path: String,
    pub trust_hash: String,
    pub locked_at: i64,
    pub drift_detected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerLockStateResponse {
    pub enabled: bool,
    pub lock_path: String,
    pub owner: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CircuitBreakerStateResponse {
    pub scope: String,
    pub target_id: String,
    pub state: String,
    pub consecutive_failures: i64,
    pub cooldown_until: Option<i64>,
    pub last_error_code: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureReasonCountResponse {
    pub code: String,
    pub count: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NumquamIntegrationStatusResponse {
    pub enabled: bool,
    pub transport: String,
    pub health_status: String,
    pub contract_version: Option<String>,
    pub supported_schema_versions: Vec<String>,
    pub degrade_mode: bool,
    pub breaker_open: bool,
    pub breaker_cooldown_until: Option<i64>,
    pub breaker_consecutive_failures: i64,
    pub required_operations_missing: Vec<String>,
    pub last_check_at: Option<i64>,
    pub last_error_code: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsResponse {
    pub service: String,
    pub version: String,
    pub uptime_ms: u64,
    pub requests_total: u64,
    pub auth_failures_total: u64,
    pub runs_started_total: u64,
    pub runs_succeeded_total: u64,
    pub runs_failed_total: u64,
    pub notes_created_total: u64,
    pub notes_updated_total: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListProviderCapabilitiesQuery {
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListToolCapabilitiesQuery {
    pub include_disabled: Option<bool>,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilitySandboxResponse {
    pub allowed_roots: Vec<String>,
    pub network_policy: String,
    pub network_allowlist_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilityResponse {
    pub tool_name: String,
    pub origin: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub timeout_ms: Option<u64>,
    pub enabled: bool,
    pub sandbox: ToolCapabilitySandboxResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListToolCapabilitiesResponse {
    pub contract_version: String,
    pub items: Vec<ToolCapabilityResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCapabilityResponse {
    pub provider: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_json_mode: bool,
    pub supports_vision: bool,
    pub max_context_tokens: Option<u32>,
    pub error_classes: Vec<String>,
    pub retryable_error_classes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListProviderCapabilitiesResponse {
    pub contract_version: String,
    pub items: Vec<ProviderCapabilityResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListPluginsQuery {
    pub include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginCapabilityResponse {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginArtifactResponse {
    pub exec_kind: String,
    pub local_path: String,
    pub command: String,
    pub args: Vec<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginCompatibilityResponse {
    pub min_gateway_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginPermissionsResponse {
    pub allowed_roots: Vec<String>,
    pub network_policy: String,
    pub network_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginLimitsResponse {
    pub timeout_ms: Option<u64>,
    pub max_output_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginManifestResponse {
    pub schema_version: String,
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub api_version: String,
    pub enabled: bool,
    pub artifact: PluginArtifactResponse,
    pub compatibility: PluginCompatibilityResponse,
    pub permissions: PluginPermissionsResponse,
    pub limits: PluginLimitsResponse,
    pub tools: Vec<PluginCapabilityResponse>,
    pub hooks: Vec<PluginCapabilityResponse>,
    pub providers: Vec<PluginCapabilityResponse>,
    pub channels: Vec<PluginCapabilityResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRuntimeStatusResponse {
    pub plugin_id: String,
    pub enabled: bool,
    pub faulted: bool,
    pub disabled_until_ms: Option<i64>,
    pub consecutive_failures: u64,
    pub last_error_code: Option<String>,
    pub last_error: Option<String>,
    pub last_success_ms: Option<i64>,
    pub last_invoked_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListPluginRuntimeStatusResponse {
    pub contract_version: String,
    pub items: Vec<PluginRuntimeStatusResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListPluginsResponse {
    pub contract_version: String,
    pub plugin_api_version: String,
    pub items: Vec<PluginManifestResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstallPluginRequest {
    pub manifest: serde_json::Value,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPluginResponse {
    pub plugin: PluginManifestResponse,
    pub rollback_available: bool,
    pub hook_policy_denials: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePluginRequest {
    pub manifest: serde_json::Value,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdatePluginResponse {
    pub plugin: PluginManifestResponse,
    pub rollback_available: bool,
    pub hook_policy_denials: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackPluginRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollbackPluginResponse {
    pub plugin: PluginManifestResponse,
    pub rollback_available: bool,
    pub hook_policy_denials: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListSkillsQuery {
    pub include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillResponse {
    pub skill_id: String,
    pub title: String,
    pub source_path: String,
    pub enabled: bool,
    pub content_chars: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListSkillsResponse {
    pub contract_version: String,
    pub items: Vec<SkillResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentResponse {
    pub agent_id: String,
    pub name: String,
    pub workspace_root: SanitizedPath,
    pub model_provider: String,
    pub model_id: String,
    pub tool_profile: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListAgentsResponse {
    pub items: Vec<AgentResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetAgentResponse {
    pub agent: AgentResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentRequest {
    pub agent_id: String,
    pub name: String,
    pub workspace_root: Option<String>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub tool_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateAgentResponse {
    pub agent: AgentResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub workspace_root: Option<String>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub tool_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateAgentResponse {
    pub agent: AgentResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardSummaryResponse {
    pub board_id: String,
    pub board_key: String,
    pub name: String,
    pub board_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub column_count: usize,
    pub card_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListBoardsResponse {
    pub items: Vec<BoardSummaryResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardColumnResponse {
    pub column_id: String,
    pub board_id: String,
    pub column_key: String,
    pub name: String,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardCardAssetResponse {
    pub card_asset_id: String,
    pub card_id: String,
    pub filename: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: i64,
    pub local_path: SanitizedPath,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardCardResponse {
    pub card_id: String,
    pub board_id: String,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub owner_kind: String,
    pub owner_agent_id: Option<String>,
    pub owner_human_id: Option<String>,
    pub due_at: Option<i64>,
    pub tags: Vec<String>,
    pub script_markdown: Option<String>,
    pub linked_session_id: Option<String>,
    pub latest_run_id: Option<String>,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub assets: Vec<BoardCardAssetResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardDetailResponse {
    pub board: BoardSummaryResponse,
    pub columns: Vec<BoardColumnResponse>,
    pub cards: Vec<BoardCardResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBoardCardRequest {
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_agent_id: Option<String>,
    pub owner_human_id: Option<String>,
    pub due_at: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub script_markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateBoardCardResponse {
    pub card: BoardCardResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBoardCardRequest {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub owner_kind: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub owner_human_id: Option<Option<String>>,
    pub due_at: Option<Option<i64>>,
    pub tags: Option<Option<Vec<String>>>,
    pub script_markdown: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateBoardCardResponse {
    pub card: BoardCardResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoveBoardCardRequest {
    pub column_id: String,
    pub before_card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoveBoardCardResponse {
    pub card: BoardCardResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadBoardCardAssetRequest {
    pub filename: String,
    pub mime: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UploadBoardCardAssetResponse {
    pub card: BoardCardResponse,
    pub asset: BoardCardAssetResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunBoardCardRequest {
    pub prompt: Option<String>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub auth_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunBoardCardResponse {
    pub card: BoardCardResponse,
    pub run: RunResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardAutomationRuleResponse {
    pub rule_id: String,
    pub job_id: String,
    pub board_id: String,
    pub column_id: String,
    pub target_column_id: String,
    pub name: String,
    pub agent_id: String,
    pub enabled: bool,
    pub schedule_kind: String,
    pub interval_seconds: Option<i64>,
    pub run_at_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub next_run_at: Option<i64>,
    pub max_cards_per_run: i64,
    pub max_runs_per_day: i64,
    pub max_attempts_per_card_per_day: i64,
    pub breaker_failure_threshold: i64,
    pub breaker_cooldown_ms: i64,
    pub generate_thumbnail_draft: bool,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub auth_profile_id: Option<String>,
    pub last_run_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListBoardAutomationRulesResponse {
    pub items: Vec<BoardAutomationRuleResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetBoardAutomationRuleResponse {
    pub rule: BoardAutomationRuleResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertBoardAutomationRuleRequest {
    pub rule_id: Option<String>,
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub agent_id: Option<String>,
    pub schedule_kind: Option<String>,
    pub interval_seconds: Option<u64>,
    pub run_at_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub target_column_id: Option<String>,
    pub max_cards_per_run: Option<u32>,
    pub max_runs_per_day: Option<u32>,
    pub max_attempts_per_card_per_day: Option<u32>,
    pub breaker_failure_threshold: Option<u32>,
    pub breaker_cooldown_ms: Option<u64>,
    pub generate_thumbnail_draft: Option<bool>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub auth_profile_id: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpsertBoardAutomationRuleResponse {
    pub rule: BoardAutomationRuleResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetBoardAutomationRuleStateRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetBoardAutomationRuleStateResponse {
    pub rule: BoardAutomationRuleResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunBoardAutomationRuleResponse {
    pub rule: BoardAutomationRuleResponse,
    pub job_run: JobRunResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSkillStateRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateSkillStateResponse {
    pub skill: SkillResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListSessionsQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListSessionsResponse {
    pub items: Vec<SessionSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub session_key: String,
    pub agent_id: String,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub message_count: i64,
    pub run_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetailResponse {
    pub session: SessionSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionRequest {
    pub session_key: Option<String>,
    pub agent_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionResponse {
    pub session: SessionSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMessageRequest {
    pub source_channel: Option<String>,
    pub source_peer_id: Option<String>,
    pub source_message_id: Option<String>,
    pub role: String,
    pub content_text: String,
    pub content_format: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateMessageResponse {
    pub message: MessageResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub message_id: String,
    pub session_id: String,
    pub source_channel: String,
    pub source_peer_id: Option<String>,
    pub source_message_id: Option<String>,
    pub role: String,
    pub content_text: String,
    pub content_format: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListMessagesQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListMessagesResponse {
    pub items: Vec<MessageResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRunRequest {
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub auth_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateRunResponse {
    pub run: RunResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    pub run_id: String,
    pub session_id: String,
    pub status: String,
    pub model_provider: String,
    pub model_id: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub error_text: Option<String>,
    pub usage_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListNotesQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListNotesResponse {
    pub items: Vec<NoteResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteResponse {
    pub note_id: String,
    pub title: Option<String>,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetNoteResponse {
    pub note: NoteResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateNoteRequest {
    pub title: Option<String>,
    pub body: String,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateNoteResponse {
    pub note: NoteResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateNoteResponse {
    pub note: NoteResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMemoryRequest {
    pub query_text: String,
    pub top_k: Option<u32>,
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMemoryResponse {
    pub query_text: String,
    pub top_k: u32,
    pub max_chars: usize,
    pub items: Vec<SearchMemoryResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMemoryResult {
    pub note_id: String,
    pub title: Option<String>,
    pub snippet: String,
    pub score: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunMemoryWhyRequest {
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub expand_citations: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunMemoryWhyResponse {
    pub run_id: String,
    pub session_id: String,
    pub transport: Option<String>,
    pub request_id: Option<String>,
    pub request_id_source: Option<String>,
    pub degrade_mode: bool,
    pub warning_codes: Vec<String>,
    pub reasons: Vec<serde_json::Value>,
    pub evidence: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncMemorySourcesRequest {
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMemorySourceItemResponse {
    pub source_path: String,
    pub note_id: Option<String>,
    pub status: String,
    pub detail: Option<String>,
    pub synced_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMemorySourcesResponse {
    pub items: Vec<SyncMemorySourceItemResponse>,
    pub synced: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListApprovalsQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListApprovalsResponse {
    pub items: Vec<ApprovalResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateApprovalRequest {
    pub run_id: String,
    pub tool_name: String,
    pub request_summary: String,
    pub request_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateApprovalResponse {
    pub approval: ApprovalResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveApprovalRequest {
    pub decision: String,
    pub decided_via: Option<String>,
    pub decided_by_peer_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveApprovalResponse {
    pub approval: ApprovalResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalResponse {
    pub approval_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub kind: String,
    pub status: String,
    pub request_summary: String,
    pub request_json: String,
    pub requested_at: i64,
    pub decided_at: Option<i64>,
    pub decided_via: Option<String>,
    pub decided_by_peer_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListAuthProfilesQuery {
    pub provider: Option<String>,
    pub include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAuthProfileRequest {
    pub provider: String,
    pub display_name: String,
    pub auth_mode: String,
    pub risk_level: String,
    pub enabled: Option<bool>,
    pub kill_switch_scope: Option<String>,
    pub api_base_url: Option<String>,
    pub credentials_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateAuthProfileResponse {
    pub profile: AuthProfileResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAuthProfileStateRequest {
    pub enabled: Option<bool>,
    pub kill_switch_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateAuthProfileStateResponse {
    pub profile: AuthProfileResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListAuthProfilesResponse {
    pub items: Vec<AuthProfileResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthProfileResponse {
    pub auth_profile_id: String,
    pub provider: String,
    pub display_name: String,
    pub auth_mode: String,
    pub risk_level: String,
    pub enabled: bool,
    pub kill_switch_scope: String,
    pub api_base_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiOauthStartRequest {
    pub display_name: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
    pub scope: Option<String>,
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub api_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiOauthStartResponse {
    pub oauth_session_id: String,
    pub authorize_url: String,
    pub callback_url: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiOauthFinishRequest {
    pub oauth_session_id: String,
    pub callback_url: Option<String>,
    pub code: Option<String>,
    pub state: Option<String>,
    pub display_name: Option<String>,
    pub api_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiOauthFinishResponse {
    pub profile: AuthProfileResponse,
    pub account_id: Option<String>,
    pub expires_at_unix: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicSetupTokenIngestRequest {
    pub display_name: String,
    pub setup_token: String,
    pub api_base_url: Option<String>,
    pub enabled: Option<bool>,
    pub kill_switch_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnthropicSetupTokenIngestResponse {
    pub profile: AuthProfileResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetAgentProviderProfileOrderRequest {
    pub profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetAgentProviderProfileOrderResponse {
    pub agent_id: String,
    pub provider: String,
    pub profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetAgentProviderProfileOrderResponse {
    pub agent_id: String,
    pub provider: String,
    pub profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannelConfig {
    pub require_mention_in_guild_channels: bool,
    pub allowlisted_user_ids: Vec<String>,
    #[serde(default = "default_channel_auto_run_enabled")]
    pub auto_run_enabled: bool,
    #[serde(default = "default_channel_model_provider")]
    pub default_model_provider: String,
    #[serde(default = "default_channel_model_id")]
    pub default_model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    pub require_mention_in_groups: bool,
    pub allowlisted_user_ids: Vec<i64>,
    #[serde(default = "default_channel_auto_run_enabled")]
    pub auto_run_enabled: bool,
    #[serde(default = "default_channel_model_provider")]
    pub default_model_provider: String,
    #[serde(default = "default_channel_model_id")]
    pub default_model_id: String,
}

fn default_channel_auto_run_enabled() -> bool {
    true
}

fn default_channel_model_provider() -> String {
    "mock".to_string()
}

fn default_channel_model_id() -> String {
    "mock-echo-v1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeGlobalConfig {
    #[serde(default)]
    pub jwt_issuer_allowlist: Vec<String>,
    #[serde(default)]
    pub jwt_audience_allowlist: Vec<String>,
    #[serde(default)]
    pub trusted_proxy_allowlist: Vec<String>,
    #[serde(default = "default_runtime_tls_termination_mode")]
    pub tls_termination_mode: String,
    #[serde(default)]
    pub public_base_url: Option<String>,
}

fn default_runtime_tls_termination_mode() -> String {
    "edge".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderPolicyConfig {
    pub provider: String,
    #[serde(default = "default_runtime_provider_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_consumer_oauth: bool,
    #[serde(default = "default_runtime_provider_kill_switch_scope")]
    pub kill_switch_scope: String,
    #[serde(default)]
    pub daily_token_budget: Option<u64>,
    #[serde(default)]
    pub daily_cost_usd_budget: Option<f64>,
    #[serde(default)]
    pub usd_per_1k_tokens: Option<f64>,
}

fn default_runtime_provider_enabled() -> bool {
    true
}

fn default_runtime_provider_kill_switch_scope() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDiscordDeploymentConfig {
    #[serde(default)]
    pub bot_token_secret_ref: Option<String>,
    #[serde(default = "default_runtime_discord_operation_mode")]
    pub operation_mode: String,
    #[serde(default)]
    pub api_base_url: Option<String>,
    #[serde(default)]
    pub transport_timeout_ms: Option<u64>,
    #[serde(default)]
    pub transport_retry_attempts: Option<usize>,
    #[serde(default)]
    pub application_id: Option<String>,
    #[serde(default)]
    pub intents: Vec<String>,
    #[serde(default)]
    pub staging_guild_ids: Vec<String>,
    #[serde(default)]
    pub staging_channel_ids: Vec<String>,
}

fn default_runtime_discord_operation_mode() -> String {
    "shim".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTelegramDeploymentConfig {
    #[serde(default)]
    pub bot_token_secret_ref: Option<String>,
    #[serde(default = "default_runtime_telegram_operation_mode")]
    pub operation_mode: String,
    #[serde(default)]
    pub api_base_url: Option<String>,
    #[serde(default)]
    pub transport_timeout_ms: Option<u64>,
    #[serde(default)]
    pub transport_retry_attempts: Option<usize>,
    #[serde(default)]
    pub long_poll_timeout_seconds: Option<u32>,
    #[serde(default = "default_runtime_telegram_webhook_mode")]
    pub webhook_mode: String,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub staging_chat_ids: Vec<i64>,
}

fn default_runtime_telegram_operation_mode() -> String {
    "shim".to_string()
}

fn default_runtime_telegram_webhook_mode() -> String {
    "long_poll".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeExtensionsConfig {
    #[serde(default)]
    pub plugin_daemon_allowlist: Vec<String>,
    #[serde(default)]
    pub plugin_bundle_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeNumquamConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub integration_base_url: Option<String>,
    #[serde(default = "default_runtime_numquam_transport")]
    pub transport: String,
    #[serde(default = "default_runtime_numquam_context_build_timeout_ms")]
    pub context_build_timeout_ms: u64,
    #[serde(default = "default_runtime_numquam_writeback_propose_timeout_ms")]
    pub writeback_propose_timeout_ms: u64,
    #[serde(default = "default_runtime_numquam_writeback_resolve_timeout_ms")]
    pub writeback_resolve_timeout_ms: u64,
    #[serde(default = "default_runtime_numquam_handshake_timeout_ms")]
    pub handshake_timeout_ms: u64,
    #[serde(default = "default_runtime_numquam_handshake_interval_ms")]
    pub handshake_interval_ms: u64,
    #[serde(default)]
    pub token_secret_ref: Option<String>,
    #[serde(default)]
    pub principal_id: Option<String>,
    #[serde(default)]
    pub principal_display_name: Option<String>,
}

fn default_runtime_numquam_transport() -> String {
    "dual".to_string()
}

fn default_runtime_numquam_context_build_timeout_ms() -> u64 {
    4_000
}

fn default_runtime_numquam_writeback_propose_timeout_ms() -> u64 {
    4_000
}

fn default_runtime_numquam_writeback_resolve_timeout_ms() -> u64 {
    4_000
}

fn default_runtime_numquam_handshake_timeout_ms() -> u64 {
    3_000
}

fn default_runtime_numquam_handshake_interval_ms() -> u64 {
    30_000
}

impl Default for RuntimeNumquamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            integration_base_url: None,
            transport: default_runtime_numquam_transport(),
            context_build_timeout_ms: default_runtime_numquam_context_build_timeout_ms(),
            writeback_propose_timeout_ms: default_runtime_numquam_writeback_propose_timeout_ms(),
            writeback_resolve_timeout_ms: default_runtime_numquam_writeback_resolve_timeout_ms(),
            handshake_timeout_ms: default_runtime_numquam_handshake_timeout_ms(),
            handshake_interval_ms: default_runtime_numquam_handshake_interval_ms(),
            token_secret_ref: None,
            principal_id: None,
            principal_display_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMemoryConfig {
    #[serde(default = "default_runtime_memory_blend_mode")]
    pub blend_mode: String,
    #[serde(default)]
    pub memory_md_sources: Vec<String>,
    #[serde(default)]
    pub numquam: RuntimeNumquamConfig,
}

fn default_runtime_memory_blend_mode() -> String {
    "mno_primary".to_string()
}

impl Default for RuntimeMemoryConfig {
    fn default() -> Self {
        Self {
            blend_mode: default_runtime_memory_blend_mode(),
            memory_md_sources: Vec::new(),
            numquam: RuntimeNumquamConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeChannelsConfig {
    pub discord: RuntimeDiscordDeploymentConfig,
    pub telegram: RuntimeTelegramDeploymentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSecurityOpsConfig {
    #[serde(default)]
    pub threat_model_approver: Option<String>,
    #[serde(default)]
    pub risk_acceptance_owner: Option<String>,
    #[serde(default)]
    pub incident_primary: Option<String>,
    #[serde(default)]
    pub incident_backup: Option<String>,
    #[serde(default)]
    pub audit_archive_target: Option<String>,
    #[serde(default)]
    pub audit_archive_encryption: Option<String>,
    #[serde(default = "default_runtime_hot_retention_days")]
    pub audit_hot_retention_days: i64,
    #[serde(default = "default_runtime_archive_retention_days")]
    pub audit_archive_retention_days: i64,
}

fn default_runtime_hot_retention_days() -> i64 {
    90
}

fn default_runtime_archive_retention_days() -> i64 {
    365
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAutonomyGuardrailsConfig {
    #[serde(default = "default_autonomy_max_run_ms")]
    pub max_run_ms: u64,
    #[serde(default = "default_autonomy_max_tool_calls_per_run")]
    pub max_tool_calls_per_run: u64,
    #[serde(default = "default_autonomy_max_provider_input_chars")]
    pub max_provider_input_chars: u64,
    #[serde(default = "default_autonomy_max_tool_output_chars_total")]
    pub max_tool_output_chars_total: u64,
    #[serde(default = "default_autonomy_max_provider_attempts")]
    pub max_provider_attempts: u64,
    #[serde(default = "default_autonomy_max_consecutive_failures_before_breaker")]
    pub max_consecutive_failures_before_breaker: u64,
    #[serde(default = "default_autonomy_heartbeat_max_run_ms")]
    pub heartbeat_max_run_ms: u64,
}

impl Default for RuntimeAutonomyGuardrailsConfig {
    fn default() -> Self {
        Self {
            max_run_ms: default_autonomy_max_run_ms(),
            max_tool_calls_per_run: default_autonomy_max_tool_calls_per_run(),
            max_provider_input_chars: default_autonomy_max_provider_input_chars(),
            max_tool_output_chars_total: default_autonomy_max_tool_output_chars_total(),
            max_provider_attempts: default_autonomy_max_provider_attempts(),
            max_consecutive_failures_before_breaker:
                default_autonomy_max_consecutive_failures_before_breaker(),
            heartbeat_max_run_ms: default_autonomy_heartbeat_max_run_ms(),
        }
    }
}

fn default_autonomy_max_run_ms() -> u64 {
    120_000
}

fn default_autonomy_max_tool_calls_per_run() -> u64 {
    16
}

fn default_autonomy_max_provider_input_chars() -> u64 {
    32_000
}

fn default_autonomy_max_tool_output_chars_total() -> u64 {
    64_000
}

fn default_autonomy_max_provider_attempts() -> u64 {
    3
}

fn default_autonomy_max_consecutive_failures_before_breaker() -> u64 {
    3
}

fn default_autonomy_heartbeat_max_run_ms() -> u64 {
    5_000
}

fn default_runtime_config_schema_version() -> String {
    "runtime.config.v1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigResponse {
    #[serde(default = "default_runtime_config_schema_version")]
    pub schema_version: String,
    pub global: RuntimeGlobalConfig,
    pub providers: Vec<RuntimeProviderPolicyConfig>,
    pub channels: RuntimeChannelsConfig,
    #[serde(default)]
    pub memory: RuntimeMemoryConfig,
    #[serde(default)]
    pub extensions: RuntimeExtensionsConfig,
    pub security: RuntimeSecurityOpsConfig,
    #[serde(default)]
    pub autonomy_guardrails: RuntimeAutonomyGuardrailsConfig,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetRuntimeConfigResponse {
    pub config: RuntimeConfigResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRuntimeConfigRequest {
    pub global: Option<RuntimeGlobalConfig>,
    pub providers: Option<Vec<RuntimeProviderPolicyConfig>>,
    pub channels: Option<RuntimeChannelsConfig>,
    pub memory: Option<RuntimeMemoryConfig>,
    pub extensions: Option<RuntimeExtensionsConfig>,
    pub security: Option<RuntimeSecurityOpsConfig>,
    pub autonomy_guardrails: Option<RuntimeAutonomyGuardrailsConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateRuntimeConfigResponse {
    pub config: RuntimeConfigResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackRuntimeConfigRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollbackRuntimeConfigResponse {
    pub config: RuntimeConfigResponse,
    pub restored_from_updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeTrustContractLockResponse {
    pub schema_version: String,
    pub lock_path: String,
    pub trust_hash: String,
    pub locked_at: i64,
    pub locked_by: String,
    pub global: RuntimeGlobalConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetRuntimeTrustContractLockResponse {
    pub lock: RuntimeTrustContractLockResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefreshRuntimeTrustContractLockRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshRuntimeTrustContractLockResponse {
    pub lock: RuntimeTrustContractLockResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertRuntimeSecretRequest {
    pub scope: String,
    pub secret_value: String,
    pub previous_secret_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpsertRuntimeSecretResponse {
    pub secret_ref: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteRuntimeSecretRequest {
    pub secret_ref: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteRuntimeSecretResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelConfigResponse {
    pub discord: DiscordChannelConfig,
    pub telegram: TelegramChannelConfig,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetChannelConfigResponse {
    pub config: ChannelConfigResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateChannelConfigRequest {
    pub discord: Option<DiscordChannelConfig>,
    pub telegram: Option<TelegramChannelConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateChannelConfigResponse {
    pub config: ChannelConfigResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelRuntimeAdapterStatusResponse {
    pub provider: String,
    pub lifecycle_state: String,
    pub healthy: bool,
    pub detail: Option<String>,
    pub last_error: Option<String>,
    pub reconnect_attempts: u64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetChannelRuntimeStatusResponse {
    pub updated_at: i64,
    pub items: Vec<ChannelRuntimeAdapterStatusResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReconnectChannelRuntimeRequest {
    pub provider: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconnectChannelRuntimeResponse {
    pub status: ChannelRuntimeAdapterStatusResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestTelegramMessageRequest {
    pub chat_id: i64,
    pub user_id: i64,
    pub text: String,
    pub is_group_chat: bool,
    pub mentions_bot: bool,
    pub reply_to_bot: bool,
    pub source_message_id: Option<String>,
    pub run_immediately: Option<bool>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub auth_profile_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestDiscordMessageRequest {
    pub guild_id: Option<String>,
    pub channel_id: String,
    pub thread_id: Option<String>,
    pub author_id: String,
    pub text: String,
    pub mentions_bot: bool,
    pub is_dm: bool,
    pub source_message_id: Option<String>,
    pub run_immediately: Option<bool>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub auth_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestChannelMessageResponse {
    pub decision: String,
    pub reason: Option<String>,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveChannelApprovalActionRequest {
    pub provider: String,
    pub action_payload: String,
    pub actor_peer_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListJobsQuery {
    pub limit: Option<u32>,
    pub include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListJobsResponse {
    pub items: Vec<JobResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateJobRequest {
    pub agent_id: Option<String>,
    pub name: String,
    pub enabled: Option<bool>,
    pub schedule_kind: String,
    pub interval_seconds: Option<u64>,
    pub run_at_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub payload_json: Option<serde_json::Value>,
    pub max_retries: Option<u32>,
    pub retry_backoff_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateJobResponse {
    pub job: JobResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub schedule_kind: Option<String>,
    pub interval_seconds: Option<u64>,
    pub run_at_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub payload_json: Option<serde_json::Value>,
    pub max_retries: Option<u32>,
    pub retry_backoff_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateJobResponse {
    pub job: JobResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveJobResponse {
    pub job_id: String,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunJobNowResponse {
    pub job_run: JobRunResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListJobHistoryQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListJobHistoryResponse {
    pub items: Vec<JobRunResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobStatusResponse {
    pub scheduler_running: bool,
    pub scheduler_lock: SchedulerLockStateResponse,
    pub jobs_total: u64,
    pub jobs_enabled: u64,
    pub jobs_due: u64,
    pub numquam: NumquamIntegrationStatusResponse,
    pub open_circuit_breakers: u64,
    pub circuit_breakers: Vec<CircuitBreakerStateResponse>,
    pub top_stop_reasons: Vec<FailureReasonCountResponse>,
    pub now_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobResponse {
    pub job_id: String,
    pub agent_id: String,
    pub name: String,
    pub enabled: bool,
    pub schedule_kind: String,
    pub interval_seconds: Option<i64>,
    pub run_at_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub next_run_at: Option<i64>,
    pub payload_json: String,
    pub max_retries: i64,
    pub retry_backoff_ms: i64,
    pub timeout_ms: i64,
    pub last_run_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRunResponse {
    pub job_run_id: String,
    pub job_id: String,
    pub trigger_kind: String,
    pub status: String,
    pub attempt: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub error_text: Option<String>,
    pub output_json: Option<String>,
    pub created_at: i64,
}
