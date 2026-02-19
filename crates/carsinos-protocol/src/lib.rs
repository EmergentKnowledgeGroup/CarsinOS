use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
pub struct PluginManifestResponse {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub api_version: String,
    pub enabled: bool,
    pub tools: Vec<PluginCapabilityResponse>,
    pub hooks: Vec<PluginCapabilityResponse>,
    pub providers: Vec<PluginCapabilityResponse>,
    pub channels: Vec<PluginCapabilityResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListPluginsResponse {
    pub contract_version: String,
    pub plugin_api_version: String,
    pub items: Vec<PluginManifestResponse>,
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
    pub interval_seconds: Option<u64>,
    pub run_at_ms: Option<i64>,
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
    pub jobs_total: u64,
    pub jobs_enabled: u64,
    pub jobs_due: u64,
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
