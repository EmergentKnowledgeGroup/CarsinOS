use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use std::path::Path;

pub mod execass;
pub mod execass_recorder;

pub const JOB_MODE_HEARTBEAT_RUN: &str = "heartbeat.run";
pub const HEARTBEAT_OUTPUT_OK: &str = "HEARTBEAT_OK";
pub const HEARTBEAT_OUTPUT_ALERT_PREFIX: &str = "ALERT:";
pub const ASSISTANT_TOOL_CONTRACT_VERSION: &str = "assistant.tools.v1";

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
pub struct ListProviderModelsQuery {
    pub provider: String,
    pub agent_id: Option<String>,
    pub auth_profile_id: Option<String>,
    pub refresh: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListToolCapabilitiesQuery {
    pub include_disabled: Option<bool>,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateWebSocketTicketResponse {
    pub ticket: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilitySandboxResponse {
    pub allowed_roots: Vec<String>,
    pub network_policy: String,
    pub network_allowlist_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapabilityConnectorOriginResponse {
    pub connector_id: String,
    pub connector_slug: String,
    pub connector_display_name: String,
    pub version_id: String,
    pub published_tool_id: String,
    pub source_kind: String,
    pub write_classification: String,
    pub trust_state: String,
    pub read_only_mirror: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<ToolCapabilityConnectorOriginResponse>,
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

#[derive(Debug, Clone, Serialize)]
pub struct ProviderModelResponse {
    pub model_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListProviderModelsResponse {
    pub contract_version: String,
    pub provider: String,
    pub auth_profile_id: Option<String>,
    pub items: Vec<ProviderModelResponse>,
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
pub struct ListConnectorCatalogQuery {
    pub source_kind: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorCatalogItemResponse {
    pub catalog_item_id: String,
    pub slug: String,
    pub display_name: String,
    pub source_kind: String,
    pub summary: String,
    pub publisher: String,
    pub trust_class: String,
    pub available_versions: Vec<String>,
    pub marketplace_origin: Option<String>,
    pub importable: bool,
    pub future_marketplace_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListConnectorCatalogResponse {
    pub contract_version: String,
    pub items: Vec<ConnectorCatalogItemResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListConnectorsQuery {
    pub source_kind: Option<String>,
    pub status: Option<String>,
    pub trust_state: Option<String>,
    pub query: Option<String>,
    pub include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorSourceResponse {
    pub connector_id: String,
    pub slug: String,
    pub display_name: String,
    pub source_kind: String,
    pub origin_kind: String,
    pub catalog_item_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_imported_version_id: Option<String>,
    pub status: String,
    pub trust_state: String,
    pub assigned_agent_count: usize,
    pub published_tool_count: usize,
    pub last_conversion_at: Option<i64>,
    pub last_review_at: Option<i64>,
    pub last_enabled_at: Option<i64>,
    pub last_disabled_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorVersionResponse {
    pub version_id: String,
    pub connector_id: String,
    pub version_label: String,
    pub source_digest: String,
    pub raw_source_location: Option<String>,
    pub import_metadata: serde_json::Value,
    pub schema_summary: serde_json::Value,
    pub latest_conversion_id: Option<String>,
    pub external_reference_policy: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorWarningResponse {
    pub code: String,
    pub message: String,
    pub blocking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorProposedToolResponse {
    pub candidate_id: String,
    pub operation_key: String,
    pub proposed_tool_name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub write_classification: String,
    pub auth_required: bool,
    pub review_blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_block_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorUnsupportedOperationResponse {
    pub operation_key: String,
    pub display_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConversionResponse {
    pub conversion_id: String,
    pub connector_id: String,
    pub version_id: String,
    pub status: String,
    pub warnings: Vec<ConnectorWarningResponse>,
    pub proposed_tools: Vec<ConnectorProposedToolResponse>,
    pub write_capable_tools: usize,
    pub unsupported_operations: Vec<ConnectorUnsupportedOperationResponse>,
    pub normalization_notes: Vec<String>,
    pub diff_from_previous: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorPublishedToolResponse {
    pub published_tool_id: String,
    pub connector_id: String,
    pub version_id: String,
    pub conversion_id: String,
    pub tool_name: String,
    pub display_name: String,
    pub tool_schema: serde_json::Value,
    pub origin_metadata: serde_json::Value,
    pub write_classification: String,
    pub published_at: i64,
    pub unpublished_at: Option<i64>,
    pub superseded_by_published_tool_id: Option<String>,
    pub deprecation_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorAssignmentResponse {
    pub assignment_id: String,
    pub connector_id: String,
    pub agent_id: String,
    pub enabled: bool,
    pub auth_mode: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorAuthBindingResponse {
    pub auth_binding_id: String,
    pub connector_id: String,
    pub agent_id: Option<String>,
    pub auth_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_session_id: Option<String>,
    pub status: String,
    pub auth_metadata: serde_json::Value,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_rotated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorInteractionResponse {
    pub interaction_id: String,
    pub connector_id: String,
    pub agent_id: Option<String>,
    pub interaction_kind: String,
    pub status: String,
    pub prompt_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
    pub expires_at: Option<i64>,
    pub consumed_at: Option<i64>,
    pub detail: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorHealthResponse {
    pub connector_id: String,
    pub status: String,
    pub degraded_reason: Option<String>,
    pub auth_required: bool,
    pub auth_required_tool_count: usize,
    pub auth_missing_tool_count: usize,
    pub last_checked_at: Option<i64>,
    pub published_tool_count: usize,
    pub assigned_agent_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListConnectorsResponse {
    pub contract_version: String,
    pub items: Vec<ConnectorSourceResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetConnectorResponse {
    pub connector: ConnectorSourceResponse,
    pub versions: Vec<ConnectorVersionResponse>,
    pub conversions: Vec<ConnectorConversionResponse>,
    pub published_tools: Vec<ConnectorPublishedToolResponse>,
    pub assignments: Vec<ConnectorAssignmentResponse>,
    pub auth_bindings: Vec<ConnectorAuthBindingResponse>,
    pub interactions: Vec<ConnectorInteractionResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportConnectorRequest {
    pub source_kind: String,
    pub display_name: String,
    pub slug: Option<String>,
    pub catalog_item_id: Option<String>,
    pub version_label: Option<String>,
    pub origin_kind: Option<String>,
    pub import_url: Option<String>,
    pub source_text: Option<String>,
    pub source_json: Option<serde_json::Value>,
    pub endpoint_url: Option<String>,
    pub auth_required: Option<bool>,
    pub external_reference_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportConnectorResponse {
    pub connector: ConnectorSourceResponse,
    pub version: ConnectorVersionResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunConnectorConversionRequest {
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunConnectorConversionResponse {
    pub connector: ConnectorSourceResponse,
    pub version: ConnectorVersionResponse,
    pub conversion: ConnectorConversionResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorAliasOverrideRequest {
    pub candidate_id: String,
    pub alias: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishConnectorToolsRequest {
    pub conversion_id: String,
    pub selected_candidate_ids: Vec<String>,
    #[serde(default)]
    pub alias_overrides: Vec<ConnectorAliasOverrideRequest>,
    pub enable_after_publish: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishConnectorToolsResponse {
    pub connector: ConnectorSourceResponse,
    pub version: ConnectorVersionResponse,
    pub published_tools: Vec<ConnectorPublishedToolResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnpublishConnectorToolsRequest {
    pub published_tool_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnpublishConnectorToolsResponse {
    pub connector: ConnectorSourceResponse,
    pub published_tools: Vec<ConnectorPublishedToolResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackConnectorVersionRequest {
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollbackConnectorVersionResponse {
    pub connector: ConnectorSourceResponse,
    pub version: ConnectorVersionResponse,
    pub published_tools: Vec<ConnectorPublishedToolResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetConnectorStateRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetConnectorStateResponse {
    pub connector: ConnectorSourceResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetConnectorAssignmentRequest {
    pub agent_id: String,
    pub enabled: Option<bool>,
    pub auth_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetConnectorAssignmentResponse {
    pub assignment: ConnectorAssignmentResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertConnectorAuthBindingRequest {
    pub agent_id: Option<String>,
    pub auth_kind: String,
    pub secret_ref: Option<String>,
    pub oauth_session_id: Option<String>,
    pub auth_metadata: Option<serde_json::Value>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpsertConnectorAuthBindingResponse {
    pub binding: ConnectorAuthBindingResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResumeConnectorInteractionRequest {
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResumeConnectorInteractionResponse {
    pub interaction: ConnectorInteractionResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListConnectorInteractionsResponse {
    pub contract_version: String,
    pub items: Vec<ConnectorInteractionResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetConnectorHealthResponse {
    pub health: ConnectorHealthResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct DescribeConnectorToolResponse {
    pub connector: ConnectorSourceResponse,
    pub published_tool: ConnectorPublishedToolResponse,
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
    pub reports_to_agent_id: Option<String>,
    pub role_label: Option<String>,
    pub memory_binding: Option<AgentMemoryBindingResponse>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryBindingResponse {
    pub binding_id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub auth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_secret_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_display_name: Option<String>,
    pub enabled: bool,
    pub trusted_local_operator_actions: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentMemoryBindingRequest {
    pub binding_id: Option<String>,
    pub provider_kind: Option<String>,
    pub base_url: Option<String>,
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub auth_secret_ref: Option<String>,
    #[serde(default)]
    pub principal_id: Option<String>,
    #[serde(default)]
    pub principal_display_name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trusted_local_operator_actions: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentMemoryBindingRequest {
    pub binding_id: Option<String>,
    pub provider_kind: Option<String>,
    pub base_url: Option<String>,
    pub auth_mode: Option<String>,
    pub auth_secret_ref: Option<Option<String>>,
    pub principal_id: Option<Option<String>>,
    pub principal_display_name: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub trusted_local_operator_actions: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentMemoryNativeSurfaceAvailabilityResponse {
    pub cards: bool,
    pub card_detail: bool,
    pub atom_detail: bool,
    pub graph_overview: bool,
    pub graph_neighbors: bool,
    pub episodes: bool,
    pub turn_why: bool,
    pub citation_lookup: bool,
    pub runtime_health: bool,
    pub telemetry_summary: bool,
    pub telemetry_turns: bool,
    pub decision_reasons: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMemoryStatusResponse {
    pub agent_id: String,
    pub binding_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<AgentMemoryBindingResponse>,
    pub native_surface_availability: AgentMemoryNativeSurfaceAvailabilityResponse,
    pub orchestration: NumquamIntegrationStatusResponse,
    pub native_runtime_status: Option<serde_json::Value>,
    pub native_runtime_health_mismatch: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetAgentMemoryStatusResponse {
    pub status: AgentMemoryStatusResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMemoryLaneStatusResponse {
    pub human_identity_id: String,
    pub assistant_agent_id: String,
    pub lane_id: String,
    pub configured_memory_mode: String,
    pub effective_memory_mode: String,
    pub source: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub local_memory_sources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<NumquamIntegrationStatusResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListAgentMemoryLaneStatusesResponse {
    pub items: Vec<AgentMemoryLaneStatusResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMemoryJsonPayloadResponse {
    pub agent_id: String,
    pub binding_id: String,
    pub data: serde_json::Value,
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
    pub reports_to_agent_id: Option<String>,
    pub role_label: Option<String>,
    pub memory_binding: Option<AgentMemoryBindingRequest>,
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
    pub reports_to_agent_id: Option<Option<String>>,
    pub role_label: Option<Option<String>>,
    pub memory_binding: Option<Option<UpdateAgentMemoryBindingRequest>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateAgentResponse {
    pub agent: AgentResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveAgentResponse {
    pub removed: bool,
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

pub const WS_EVENT_SCHEMA_VERSION_V1: &str = "carsinos.ws.event.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsEventFrame {
    #[serde(default = "default_ws_event_legacy_version")]
    pub v: u32,
    #[serde(default = "default_ws_event_legacy_type")]
    pub r#type: String,
    pub event: String,
    pub seq: u64,
    pub data: serde_json::Value,
    pub schema_version: String,
    pub event_id: String,
    pub event_type: String,
    pub ts_unix_ms: i64,
    pub request_id: Option<String>,
    pub entity: String,
    pub payload: serde_json::Value,
}

impl WsEventFrame {
    pub fn new(seq: u64, event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        let event_type = event_type.into();
        let request_id = payload
            .get("request_id")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        Self {
            v: default_ws_event_legacy_version(),
            r#type: default_ws_event_legacy_type(),
            event: event_type.clone(),
            seq,
            data: payload.clone(),
            schema_version: WS_EVENT_SCHEMA_VERSION_V1.to_string(),
            event_id: format!("evt-{seq}"),
            event_type: event_type.clone(),
            ts_unix_ms: Utc::now().timestamp_millis(),
            request_id,
            entity: ws_event_entity_from_type(&event_type),
            payload,
        }
    }
}

fn default_ws_event_legacy_version() -> u32 {
    1
}

fn default_ws_event_legacy_type() -> String {
    "event".to_string()
}

pub fn ws_event_entity_from_type(event_type: &str) -> String {
    let parts = event_type
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        ["board", "card", ..] => "board.card".to_string(),
        ["board", "asset", ..] => "board.asset".to_string(),
        ["board", "automation", ..] => "board.automation".to_string(),
        ["run", ..] => "run".to_string(),
        ["approval", ..] => "approval".to_string(),
        ["job", ..] => "job".to_string(),
        ["channel", ..] => "channel".to_string(),
        ["agent_mail", "thread", ..] => "agent_mail.thread".to_string(),
        ["agent_mail", "message", ..] => "agent_mail.message".to_string(),
        [head, ..] => (*head).to_string(),
        [] => "system".to_string(),
    }
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
pub struct ListGoalsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub sort: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalResponse {
    pub goal_id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub owner_agent_id: Option<String>,
    pub target_date: Option<i64>,
    pub progress_pct: u8,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListGoalsResponse {
    pub items: Vec<GoalResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetGoalResponse {
    pub goal: GoalResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGoalRequest {
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub target_date: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateGoalResponse {
    pub goal: GoalResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateGoalRequest {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub target_date: Option<Option<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateGoalResponse {
    pub goal: GoalResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListProjectsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub sort: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
    pub goal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectResponse {
    pub project_id: String,
    pub goal_id: String,
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub status: String,
    pub owner_agent_id: Option<String>,
    pub workspace_root: Option<String>,
    pub budget_month_usd: Option<f64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListProjectsResponse {
    pub items: Vec<ProjectResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetProjectResponse {
    pub project: ProjectResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProjectRequest {
    pub goal_id: String,
    pub slug: String,
    pub name: String,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub workspace_root: Option<String>,
    pub budget_month_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateProjectResponse {
    pub project: ProjectResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProjectRequest {
    pub goal_id: Option<String>,
    pub slug: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub workspace_root: Option<Option<String>>,
    pub budget_month_usd: Option<Option<f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateProjectResponse {
    pub project: ProjectResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListTasksQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub sort: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
    pub goal_id: Option<String>,
    pub project_id: Option<String>,
    pub stale: Option<bool>,
    pub blocked: Option<bool>,
    pub unassigned: Option<bool>,
    pub hierarchy_root_agent_id: Option<String>,
    pub hierarchy_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub project_id: String,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub priority: String,
    pub owner_agent_id: Option<String>,
    pub due_at: Option<i64>,
    pub blocked_reason: Option<String>,
    pub linked_board_card_id: Option<String>,
    pub linked_job_id: Option<String>,
    pub latest_run_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListTasksResponse {
    pub items: Vec<TaskResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetTaskResponse {
    pub task: TaskResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskRequest {
    pub project_id: String,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub owner_agent_id: Option<String>,
    pub due_at: Option<i64>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateTaskResponse {
    pub task: TaskResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTaskRequest {
    pub project_id: Option<String>,
    pub parent_task_id: Option<Option<String>>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub due_at: Option<Option<i64>>,
    pub blocked_reason: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateTaskResponse {
    pub task: TaskResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkTaskBoardCardRequest {
    pub board_card_id: String,
    pub force_reassign: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkTaskJobRequest {
    pub job_id: String,
    pub force_reassign: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClearTaskLinksRequest {
    pub clear_board_card: Option<bool>,
    pub clear_job: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLinkMutationResponse {
    pub task: TaskResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyTaskListItemResponse {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub owner_agent_id: Option<String>,
    pub owner_name: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub goal_id: String,
    pub goal_title: String,
    pub updated_at: i64,
    pub due_at: Option<i64>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategySpendByAgentItemResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub estimated_cost_total: f64,
    pub linked_task_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategySpendByProjectItemResponse {
    pub project_id: String,
    pub project_name: String,
    pub goal_id: String,
    pub goal_title: String,
    pub estimated_cost_total: f64,
    pub attributed_run_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyGoalProgressItemResponse {
    pub goal_id: String,
    pub title: String,
    pub progress_pct: u8,
    pub open_task_count: u64,
    pub blocked_task_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyApprovalBacklogItemResponse {
    pub approval_id: String,
    pub kind: String,
    pub summary: String,
    pub linked_task_id: Option<String>,
    pub requested_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategySummaryResponse {
    pub generated_at_ms: i64,
    pub currency: String,
    pub blocked_task_count: u64,
    pub blocked_tasks: Vec<StrategyTaskListItemResponse>,
    pub stale_task_count: u64,
    pub stale_tasks: Vec<StrategyTaskListItemResponse>,
    pub spend_by_agent: Vec<StrategySpendByAgentItemResponse>,
    pub spend_by_project: Vec<StrategySpendByProjectItemResponse>,
    pub unattributed_spend_total: f64,
    pub goal_progress: Vec<StrategyGoalProgressItemResponse>,
    pub critical_approval_backlog_count: u64,
    pub critical_approval_backlog: Vec<StrategyApprovalBacklogItemResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategySummaryQuery {
    pub timezone: Option<String>,
    pub tz_offset_minutes: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListRunbooksQuery {
    pub kind: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
    pub linked_task_id: Option<String>,
    pub linked_project_id: Option<String>,
    pub linked_goal_id: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookDeepLinkTargetResponse {
    pub tab: String,
    pub target_kind: String,
    pub target_id: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookEntityRefResponse {
    pub entity_kind: String,
    pub entity_id: String,
    pub display_label: String,
    pub deep_link: RunbookDeepLinkTargetResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookExecutionRefResponse {
    pub entity_kind: String,
    pub entity_id: String,
    pub created_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub waiting_since_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookDataAvailabilityResponse {
    pub is_limited: bool,
    pub is_stale: bool,
    pub last_refresh_at_ms: i64,
    pub missing_source_kinds: Vec<String>,
    pub stale_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookWarningResponse {
    pub warning_id: String,
    pub warning_kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookSourceFactResponse {
    pub fact_id: String,
    pub fact_kind: String,
    pub entity_ref: Option<RunbookEntityRefResponse>,
    pub occurred_at_ms: Option<i64>,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookActionResponse {
    pub action_id: String,
    pub action_kind: String,
    pub label: String,
    pub availability: String,
    pub disabled_reason: Option<String>,
    pub target_entity_ref: Option<RunbookEntityRefResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookStepResponse {
    pub step_id: String,
    pub label: String,
    pub kind: String,
    pub state: String,
    pub state_reason: Option<String>,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub waiting_since_ms: Option<i64>,
    pub linked_entity_refs: Vec<RunbookEntityRefResponse>,
    pub action_refs: Vec<String>,
    pub template_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookHistoryItemResponse {
    pub history_id: String,
    pub event_kind: String,
    pub label: String,
    pub detail: Option<String>,
    pub occurred_at_ms: i64,
    pub step_id: Option<String>,
    pub entity_refs: Vec<RunbookEntityRefResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookStatusCountsResponse {
    pub pending: u64,
    pub active: u64,
    pub waiting: u64,
    pub blocked: u64,
    pub failed: u64,
    pub completed: u64,
    pub limited: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookSummaryItemResponse {
    pub runbook_id: String,
    pub runbook_kind: String,
    pub anchor_kind: String,
    pub anchor_id: String,
    pub title: String,
    pub status: String,
    pub status_reason: Option<String>,
    pub owner_agent_id: Option<String>,
    pub owner_agent_label: Option<String>,
    pub primary_entity_label: String,
    pub updated_at_ms: i64,
    pub current_step_label: Option<String>,
    pub warning_count: u32,
    pub linked_entities: Vec<RunbookEntityRefResponse>,
    pub availability: RunbookDataAvailabilityResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRunbooksResponse {
    pub generated_at_ms: i64,
    pub items: Vec<RunbookSummaryItemResponse>,
    pub counts_by_status: RunbookStatusCountsResponse,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookDetailResponse {
    pub runbook_id: String,
    pub runbook_kind: String,
    pub template_id: String,
    pub template_version: String,
    pub anchor_kind: String,
    pub anchor_id: String,
    pub title: String,
    pub status: String,
    pub status_reason: Option<String>,
    pub generated_at_ms: i64,
    pub selected_execution_ref: Option<RunbookExecutionRefResponse>,
    pub active_step_id: Option<String>,
    pub next_step_ids: Vec<String>,
    pub linked_entities: Vec<RunbookEntityRefResponse>,
    pub steps: Vec<RunbookStepResponse>,
    pub history: Vec<RunbookHistoryItemResponse>,
    pub actions: Vec<RunbookActionResponse>,
    pub source_facts: Vec<RunbookSourceFactResponse>,
    pub availability: RunbookDataAvailabilityResponse,
    pub warnings: Vec<RunbookWarningResponse>,
    pub owner_agent_id: Option<String>,
    pub owner_agent_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListBootstrapPresetsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub sort: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapPresetResponse {
    pub schema_version: String,
    pub preset_key: String,
    pub display_name: String,
    pub description: String,
    pub role_label: String,
    pub provider_path: String,
    pub default_model_provider: Option<String>,
    pub default_model_id: Option<String>,
    pub default_tool_profile: Option<String>,
    pub default_workspace_root: Option<String>,
    pub default_reports_to_agent_id: Option<String>,
    pub setup_notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListBootstrapPresetsResponse {
    pub items: Vec<BootstrapPresetResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetBootstrapPresetResponse {
    pub preset: BootstrapPresetResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBootstrapPresetRequest {
    pub preset_key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub role_label: String,
    pub provider_path: String,
    pub default_model_provider: Option<String>,
    pub default_model_id: Option<String>,
    pub default_tool_profile: Option<String>,
    pub default_workspace_root: Option<String>,
    pub default_reports_to_agent_id: Option<String>,
    pub setup_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateBootstrapPresetResponse {
    pub preset: BootstrapPresetResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBootstrapPresetRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub role_label: Option<String>,
    pub provider_path: Option<String>,
    pub default_model_provider: Option<Option<String>>,
    pub default_model_id: Option<Option<String>>,
    pub default_tool_profile: Option<Option<String>>,
    pub default_workspace_root: Option<Option<String>>,
    pub default_reports_to_agent_id: Option<Option<String>>,
    pub setup_notes: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateBootstrapPresetResponse {
    pub preset: BootstrapPresetResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportBootstrapPresetResponse {
    pub preset: BootstrapPresetResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportBootstrapPresetRequest {
    pub payload: serde_json::Value,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportBootstrapPresetResponse {
    pub preset: BootstrapPresetResponse,
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
    #[serde(default)]
    pub human_identity_id: Option<String>,
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
    #[serde(default)]
    pub human_identity_id: Option<String>,
    #[serde(default)]
    pub assistant_agent_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMemoryScope {
    #[serde(default)]
    pub tenant_key: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub db_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantToolRpcRequest {
    #[serde(default)]
    pub contract_version: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub root_session_id: Option<String>,
    #[serde(default)]
    pub root_run_id: Option<String>,
    #[serde(default)]
    pub caller_agent_id: Option<String>,
    #[serde(default)]
    pub memory_scope: Option<AssistantMemoryScope>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantToolError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantToolRpcResponse {
    pub contract_version: String,
    pub request_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AssistantToolError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_ref: Option<String>,
    pub timing_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantToolEnvelope {
    pub contract_version: String,
    pub request_id: String,
    pub root_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_run_id: Option<String>,
    pub caller_agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_scope: Option<AssistantMemoryScope>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantWorkerSummary {
    pub worker_key: String,
    pub worker_kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub template_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_stop_reason: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantTaskHandle {
    pub worker_key: String,
    pub session_id: String,
    pub run_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantToolCapabilityItem {
    pub tool_name: String,
    pub risk_level: String,
    pub requires_approval: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<ToolCapabilityConnectorOriginResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantToolLimitsResponse {
    pub max_spawn_depth: u32,
    pub max_children_per_root_run: u32,
    pub max_active_workers_total: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssistantWorkerTemplateRunDefaults {
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantWorkerTemplateResponse {
    pub template_key: String,
    pub display_name: String,
    pub instructions: String,
    pub run_defaults: AssistantWorkerTemplateRunDefaults,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantToolCapabilitiesResponse {
    pub contract_version: String,
    pub tools: Vec<AssistantToolCapabilityItem>,
    pub limits: AssistantToolLimitsResponse,
    pub templates: Vec<AssistantWorkerTemplateResponse>,
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
pub struct AnthropicSetupTokenValidateRequest {
    pub setup_token: String,
    pub api_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnthropicSetupTokenValidateResponse {
    pub valid: bool,
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
    #[serde(default)]
    pub default_agent_id: Option<String>,
    #[serde(default = "default_channel_model_provider")]
    pub default_model_provider: String,
    #[serde(default = "default_channel_model_id")]
    pub default_model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    pub require_mention_in_groups: bool,
    pub allowlisted_user_ids: Vec<i64>,
    #[serde(default = "default_telegram_dm_policy")]
    pub dm_policy: String,
    #[serde(default = "default_telegram_group_policy")]
    pub group_policy: String,
    #[serde(default)]
    pub group_allowlisted_user_ids: Vec<i64>,
    #[serde(default)]
    pub allowlisted_chat_ids: Vec<i64>,
    #[serde(default = "default_telegram_auto_leave_unauthorized_groups")]
    pub auto_leave_unauthorized_groups: bool,
    #[serde(default = "default_telegram_pairing_code_ttl_seconds")]
    pub pairing_code_ttl_seconds: u32,
    #[serde(default = "default_telegram_pairing_max_pending")]
    pub pairing_max_pending: u32,
    #[serde(default = "default_telegram_unauthorized_spam_threshold")]
    pub unauthorized_spam_threshold: u32,
    #[serde(default = "default_telegram_unauthorized_spam_block_seconds")]
    pub unauthorized_spam_block_seconds: u32,
    #[serde(default = "default_channel_auto_run_enabled")]
    pub auto_run_enabled: bool,
    #[serde(default)]
    pub default_agent_id: Option<String>,
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

fn default_telegram_dm_policy() -> String {
    "pairing".to_string()
}

fn default_telegram_group_policy() -> String {
    "allowlist".to_string()
}

fn default_telegram_auto_leave_unauthorized_groups() -> bool {
    true
}

fn default_telegram_pairing_code_ttl_seconds() -> u32 {
    3600
}

fn default_telegram_pairing_max_pending() -> u32 {
    3
}

fn default_telegram_unauthorized_spam_threshold() -> u32 {
    4
}

fn default_telegram_unauthorized_spam_block_seconds() -> u32 {
    3600
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
    #[serde(default)]
    pub assistant_system_prompt: Option<String>,
}

fn default_runtime_tls_termination_mode() -> String {
    "edge".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderPolicyConfig {
    pub provider: String,
    #[serde(default = "default_runtime_provider_enabled")]
    pub enabled: bool,
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
    #[serde(default = "default_runtime_channel_enabled")]
    pub enabled: bool,
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
    #[serde(default = "default_runtime_channel_enabled")]
    pub enabled: bool,
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

fn default_runtime_channel_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeExtensionsConfig {
    #[serde(default)]
    pub plugin_daemon_allowlist: Vec<String>,
    #[serde(default)]
    pub plugin_bundle_root: Option<String>,
    #[serde(default)]
    pub assistant_tools: RuntimeAssistantToolsConfig,
    #[serde(default)]
    pub browser: RuntimeBrowserConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeBrowserConfig {
    #[serde(default)]
    pub pinchtab: RuntimePinchTabConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePinchTabConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_runtime_pinchtab_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub token_secret_ref: Option<String>,
    #[serde(default = "default_runtime_pinchtab_allowed_domains")]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_runtime_pinchtab_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_runtime_pinchtab_use_agent_sessions")]
    pub use_agent_sessions: bool,
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub risk_gates: RuntimePinchTabRiskGatesConfig,
}

fn default_runtime_pinchtab_base_url() -> String {
    "http://127.0.0.1:9867".to_string()
}

fn default_runtime_pinchtab_timeout_ms() -> u64 {
    30_000
}

fn default_runtime_pinchtab_use_agent_sessions() -> bool {
    true
}

fn default_runtime_pinchtab_allowed_domains() -> Vec<String> {
    vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ]
}

impl Default for RuntimePinchTabConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_runtime_pinchtab_base_url(),
            token_secret_ref: None,
            allowed_domains: default_runtime_pinchtab_allowed_domains(),
            timeout_ms: default_runtime_pinchtab_timeout_ms(),
            use_agent_sessions: default_runtime_pinchtab_use_agent_sessions(),
            default_profile: None,
            risk_gates: RuntimePinchTabRiskGatesConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimePinchTabRiskGatesConfig {
    #[serde(default)]
    pub allow_eval: bool,
    #[serde(default)]
    pub allow_downloads: bool,
    #[serde(default)]
    pub allow_uploads: bool,
    #[serde(default)]
    pub allow_clipboard: bool,
    #[serde(default)]
    pub allow_cookies: bool,
    #[serde(default)]
    pub allow_network_intercept: bool,
    #[serde(default)]
    pub allow_attach: bool,
    #[serde(default)]
    pub allow_screencast: bool,
    #[serde(default)]
    pub allow_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAssistantToolLimitsConfig {
    #[serde(default = "default_runtime_assistant_max_spawn_depth")]
    pub max_spawn_depth: u32,
    #[serde(default = "default_runtime_assistant_max_children_per_root_run")]
    pub max_children_per_root_run: u32,
    #[serde(default = "default_runtime_assistant_max_active_workers_total")]
    pub max_active_workers_total: u32,
}

fn default_runtime_assistant_max_spawn_depth() -> u32 {
    2
}

fn default_runtime_assistant_max_children_per_root_run() -> u32 {
    8
}

fn default_runtime_assistant_max_active_workers_total() -> u32 {
    16
}

impl Default for RuntimeAssistantToolLimitsConfig {
    fn default() -> Self {
        Self {
            max_spawn_depth: default_runtime_assistant_max_spawn_depth(),
            max_children_per_root_run: default_runtime_assistant_max_children_per_root_run(),
            max_active_workers_total: default_runtime_assistant_max_active_workers_total(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAssistantWorkerTemplateConfig {
    pub template_key: String,
    pub display_name: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub run_defaults: AssistantWorkerTemplateRunDefaults,
}

fn default_runtime_assistant_worker_templates() -> Vec<RuntimeAssistantWorkerTemplateConfig> {
    vec![
        RuntimeAssistantWorkerTemplateConfig {
            template_key: "general".to_string(),
            display_name: "General Worker".to_string(),
            instructions: "General purpose helper worker.".to_string(),
            run_defaults: AssistantWorkerTemplateRunDefaults {
                model_provider: Some("mock".to_string()),
                model_id: Some("mock-echo-v1".to_string()),
            },
        },
        RuntimeAssistantWorkerTemplateConfig {
            template_key: "researcher".to_string(),
            display_name: "Research Worker".to_string(),
            instructions: "Research and summarize relevant information with citations.".to_string(),
            run_defaults: AssistantWorkerTemplateRunDefaults {
                model_provider: Some("openai".to_string()),
                model_id: Some("gpt-4.1-mini".to_string()),
            },
        },
        RuntimeAssistantWorkerTemplateConfig {
            template_key: "archivist".to_string(),
            display_name: "Archivist Worker".to_string(),
            instructions: "Maintain project artifacts and produce concise structured notes."
                .to_string(),
            run_defaults: AssistantWorkerTemplateRunDefaults {
                model_provider: Some("anthropic".to_string()),
                model_id: Some("claude-3-5-sonnet-latest".to_string()),
            },
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAssistantToolsConfig {
    #[serde(default)]
    pub limits: RuntimeAssistantToolLimitsConfig,
    #[serde(default = "default_runtime_assistant_worker_templates")]
    pub templates: Vec<RuntimeAssistantWorkerTemplateConfig>,
}

impl Default for RuntimeAssistantToolsConfig {
    fn default() -> Self {
        Self {
            limits: RuntimeAssistantToolLimitsConfig::default(),
            templates: default_runtime_assistant_worker_templates(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeNumquamConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub integration_base_url: Option<String>,
    #[serde(default)]
    pub managed_runtime_enabled: bool,
    #[serde(default)]
    pub managed_repo_root: Option<String>,
    #[serde(default)]
    pub managed_lanes_root: Option<String>,
    #[serde(default)]
    pub managed_python_bin: Option<String>,
    #[serde(default = "default_runtime_numquam_managed_runtime_port_base")]
    pub managed_runtime_port_base: u16,
    #[serde(default = "default_runtime_numquam_managed_mcp_port_base")]
    pub managed_mcp_port_base: u16,
    #[serde(default = "default_runtime_numquam_managed_launch_timeout_ms")]
    pub managed_launch_timeout_ms: u64,
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

fn default_runtime_numquam_managed_runtime_port_base() -> u16 {
    17_340
}

fn default_runtime_numquam_managed_mcp_port_base() -> u16 {
    18_340
}

fn default_runtime_numquam_managed_launch_timeout_ms() -> u64 {
    15_000
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
            managed_runtime_enabled: false,
            managed_repo_root: None,
            managed_lanes_root: None,
            managed_python_bin: None,
            managed_runtime_port_base: default_runtime_numquam_managed_runtime_port_base(),
            managed_mcp_port_base: default_runtime_numquam_managed_mcp_port_base(),
            managed_launch_timeout_ms: default_runtime_numquam_managed_launch_timeout_ms(),
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
    "local_augment".to_string()
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
pub struct RuntimeHumanIdentityConfig {
    pub human_identity_id: String,
    pub display_name: String,
    #[serde(default = "default_runtime_lane_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePlatformIdentityLinkConfig {
    pub provider: String,
    pub platform_user_id: String,
    pub human_identity_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "default_runtime_lane_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAssistantAssignmentConfig {
    pub human_identity_id: String,
    pub assistant_agent_id: String,
    #[serde(default = "default_runtime_lane_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLaneMemoryPolicyConfig {
    pub human_identity_id: String,
    pub assistant_agent_id: String,
    #[serde(default = "default_runtime_lane_memory_mode")]
    pub memory_mode: String,
    #[serde(default)]
    pub lane_id: Option<String>,
    #[serde(default)]
    pub local_memory_sources: Vec<String>,
}

fn default_runtime_lane_enabled() -> bool {
    true
}

fn default_runtime_lane_memory_mode() -> String {
    "inherit_runtime".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRoutingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_runtime_routing_use_channel_defaults_as_fallback")]
    pub use_channel_defaults_as_fallback: bool,
    #[serde(default)]
    pub local_operator_human_identity_id: Option<String>,
    #[serde(default)]
    pub dm_unmapped_policy: String,
    #[serde(default)]
    pub shared_unmapped_policy: String,
    #[serde(default)]
    pub human_identities: Vec<RuntimeHumanIdentityConfig>,
    #[serde(default)]
    pub platform_identity_links: Vec<RuntimePlatformIdentityLinkConfig>,
    #[serde(default)]
    pub assistant_assignments: Vec<RuntimeAssistantAssignmentConfig>,
    #[serde(default)]
    pub lane_memory_policies: Vec<RuntimeLaneMemoryPolicyConfig>,
}

fn default_runtime_routing_use_channel_defaults_as_fallback() -> bool {
    false
}

fn default_runtime_dm_unmapped_policy() -> String {
    "approval_required".to_string()
}

fn default_runtime_shared_unmapped_policy() -> String {
    "block".to_string()
}

impl Default for RuntimeRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            use_channel_defaults_as_fallback:
                default_runtime_routing_use_channel_defaults_as_fallback(),
            local_operator_human_identity_id: Some("local-operator".to_string()),
            dm_unmapped_policy: default_runtime_dm_unmapped_policy(),
            shared_unmapped_policy: default_runtime_shared_unmapped_policy(),
            human_identities: Vec::new(),
            platform_identity_links: Vec::new(),
            assistant_assignments: Vec::new(),
            lane_memory_policies: Vec::new(),
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
    pub routing: RuntimeRoutingConfig,
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
    pub routing: Option<RuntimeRoutingConfig>,
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
    pub session_state: String,
    pub proof_state: String,
    pub detail: Option<String>,
    pub proof_detail: Option<String>,
    pub last_error: Option<String>,
    pub last_inbound_at: Option<i64>,
    pub last_outbound_at: Option<i64>,
    pub last_proven_at: Option<i64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramPairingPendingRequestResponse {
    pub code: String,
    pub user_id: i64,
    pub chat_id: i64,
    pub preview_text: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub attempt_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramBlockedSenderResponse {
    pub user_id: i64,
    pub blocked_until: i64,
    pub reason: String,
    pub attempt_count: u32,
    pub last_attempt_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetTelegramPairingStatusResponse {
    pub dm_policy: String,
    pub group_policy: String,
    pub auto_leave_unauthorized_groups: bool,
    pub pending_requests: Vec<TelegramPairingPendingRequestResponse>,
    pub blocked_senders: Vec<TelegramBlockedSenderResponse>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveTelegramPairingRequest {
    pub code: String,
    #[serde(default)]
    pub human_identity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveTelegramPairingResponse {
    pub status: GetTelegramPairingStatusResponse,
    pub approved_user_id: Option<i64>,
    pub linked_human_identity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordPairingPendingRequestResponse {
    pub code: String,
    pub user_id: String,
    pub channel_id: String,
    pub preview_text: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub attempt_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordBlockedSenderResponse {
    pub user_id: String,
    pub blocked_until: i64,
    pub reason: String,
    pub attempt_count: u32,
    pub last_attempt_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetDiscordPairingStatusResponse {
    pub dm_policy: String,
    pub pending_requests: Vec<DiscordPairingPendingRequestResponse>,
    pub blocked_senders: Vec<DiscordBlockedSenderResponse>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveDiscordPairingRequest {
    pub code: String,
    #[serde(default)]
    pub human_identity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveDiscordPairingResponse {
    pub status: GetDiscordPairingStatusResponse,
    pub approved_user_id: Option<String>,
    pub linked_human_identity_id: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct MissionControlCalendarWeekQuery {
    pub week_start_ms: Option<i64>,
    pub tz_offset_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlCalendarWeekJobResponse {
    pub job_id: String,
    pub name: String,
    pub agent_id: String,
    pub enabled: bool,
    pub schedule_kind: String,
    pub interval_seconds: Option<i64>,
    pub cron_expr: Option<String>,
    pub next_run_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub last_error: Option<String>,
    pub lane: String,
    pub primary_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlCalendarWeekResponse {
    pub week_start_ms: i64,
    pub week_end_ms: i64,
    pub generated_at_ms: i64,
    pub always_running: Vec<MissionControlCalendarWeekJobResponse>,
    pub next_up: Vec<MissionControlCalendarWeekJobResponse>,
    pub jobs: Vec<MissionControlCalendarWeekJobResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MissionControlFocusQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlFocusItemResponse {
    pub item_id: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub primary_action: String,
    pub action_payload: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlFocusResponse {
    pub generated_at_ms: i64,
    pub items: Vec<MissionControlFocusItemResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MissionControlUsageQuery {
    #[serde(default)]
    pub window: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub window_start_ms: Option<i64>,
    #[serde(default)]
    pub window_end_ms: Option<i64>,
    #[serde(default)]
    pub tz_offset_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageByAgentResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub estimated_cost_total: f64,
    pub token_input_total: u64,
    pub token_output_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageByModelResponse {
    pub model_provider: String,
    pub model_id: String,
    pub estimated_cost_total: f64,
    pub token_input_total: u64,
    pub token_output_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageByProviderResponse {
    pub provider: String,
    pub estimated_cost_total: f64,
    pub token_input_total: u64,
    pub token_output_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageByTimeResponse {
    pub bucket_start_utc: DateTime<Utc>,
    pub bucket_end_utc: DateTime<Utc>,
    pub estimated_cost_total: f64,
    pub token_input_total: u64,
    pub token_output_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageByJobResponse {
    pub job_id: String,
    pub name: Option<String>,
    pub estimated_cost_total: f64,
    pub token_input_total: u64,
    pub token_output_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageByCardResponse {
    pub card_id: String,
    pub title: Option<String>,
    pub estimated_cost_total: f64,
    pub token_input_total: u64,
    pub token_output_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageBudgetThresholdResponse {
    pub provider: String,
    pub daily_token_budget: Option<u64>,
    pub daily_cost_usd_budget: Option<f64>,
    pub token_usage_total: u64,
    pub cost_usage_total: f64,
    pub token_ratio: Option<f64>,
    pub cost_ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionControlUsageResponse {
    pub contract_version: String,
    pub available: bool,
    pub window: String,
    pub timezone: String,
    pub currency: String,
    pub window_start_utc: Option<DateTime<Utc>>,
    pub window_end_utc: Option<DateTime<Utc>>,
    pub estimated_cost_total: Option<f64>,
    pub token_input_total: Option<u64>,
    pub token_output_total: Option<u64>,
    pub by_agent: Option<Vec<MissionControlUsageByAgentResponse>>,
    pub by_model: Option<Vec<MissionControlUsageByModelResponse>>,
    pub by_provider: Option<Vec<MissionControlUsageByProviderResponse>>,
    pub by_time: Option<Vec<MissionControlUsageByTimeResponse>>,
    pub by_job: Option<Vec<MissionControlUsageByJobResponse>>,
    pub by_card: Option<Vec<MissionControlUsageByCardResponse>>,
    pub budget_thresholds: Option<Vec<MissionControlUsageBudgetThresholdResponse>>,
    pub updated_at_utc: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListAgentMailThreadsQuery {
    pub kind: Option<String>,
    pub mailbox: Option<String>,
    pub principal_id: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListAgentMailMessagesQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListAgentMailFileLeasesQuery {
    pub holder_principal: Option<String>,
    pub include_released: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListOfficeChatterQuery {
    pub limit_rooms: Option<u32>,
    pub limit_messages: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FloorPresenceTargetResponse {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FloorPresenceItemResponse {
    pub agent_id: String,
    pub display_name: String,
    pub activity: String,
    pub activity_label: String,
    pub mood: String,
    pub observed_at_ms: Option<i64>,
    pub source: String,
    pub target: Option<FloorPresenceTargetResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FloorPresenceResponse {
    pub generated_at_ms: i64,
    pub refresh_after_ms: i64,
    pub items: Vec<FloorPresenceItemResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfficeChatterRoomResponse {
    pub thread_id: String,
    pub workstream_id: String,
    pub label: String,
    pub unread_count: Option<i64>,
    pub last_activity_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfficeChatterAuthorResponse {
    pub kind: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfficeChatterSourceResponse {
    pub kind: String,
    pub event_name: Option<String>,
    pub workstream_id: String,
    pub revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfficeChatterMessageResponse {
    pub message_id: String,
    pub thread_id: String,
    pub author: OfficeChatterAuthorResponse,
    pub text: String,
    pub created_at_ms: i64,
    pub source: OfficeChatterSourceResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfficeChatterResponse {
    pub rooms: Vec<OfficeChatterRoomResponse>,
    pub messages: Vec<OfficeChatterMessageResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOfficeChatterMessageRequest {
    pub body_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateOfficeChatterMessageResponse {
    pub message: OfficeChatterMessageResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailThreadSummaryResponse {
    pub thread_id: String,
    pub kind: String,
    pub subject: String,
    pub created_by_principal: String,
    pub participant_count: i64,
    pub message_count: i64,
    pub latest_message_at: Option<i64>,
    pub latest_message_preview: Option<String>,
    pub latest_sender_principal: Option<String>,
    pub unread_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailThreadParticipantResponse {
    pub principal_id: String,
    pub role: String,
    pub joined_at: i64,
    pub last_read_at: Option<i64>,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailMessageRecipientResponse {
    pub recipient_principal: String,
    pub delivered_at: i64,
    pub acked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailAttachmentResponse {
    pub attachment_id: String,
    pub message_id: String,
    pub filename: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailMessageResponse {
    pub message_id: String,
    pub thread_id: String,
    pub sender_principal: String,
    pub sender_kind: String,
    pub body_text: String,
    pub metadata_json: Option<String>,
    pub created_at: i64,
    pub recipients: Vec<AgentMailMessageRecipientResponse>,
    pub attachments: Vec<AgentMailAttachmentResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailThreadDetailResponse {
    pub thread: AgentMailThreadSummaryResponse,
    pub participants: Vec<AgentMailThreadParticipantResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListAgentMailThreadsResponse {
    pub items: Vec<AgentMailThreadSummaryResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListAgentMailMessagesResponse {
    pub items: Vec<AgentMailMessageResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentMailThreadRequest {
    pub kind: Option<String>,
    pub subject: String,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateAgentMailThreadResponse {
    pub thread: AgentMailThreadSummaryResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendAgentMailMessageRequest {
    pub sender_principal: Option<String>,
    pub sender_kind: Option<String>,
    pub body_text: String,
    pub metadata_json: Option<serde_json::Value>,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendAgentMailMessageResponse {
    pub message: AgentMailMessageResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AckAgentMailMessageRequest {
    pub recipient_principal: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AckAgentMailMessageResponse {
    pub message_id: String,
    pub recipient_principal: String,
    pub acked_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadAgentMailAttachmentRequest {
    pub filename: String,
    pub mime: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UploadAgentMailAttachmentResponse {
    pub attachment: AgentMailAttachmentResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentMailFileLeaseResponse {
    pub lease_id: String,
    pub holder_principal: String,
    pub glob_pattern: String,
    pub exclusive: bool,
    pub ttl_ms: i64,
    pub note: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub released_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentMailFileLeaseRequest {
    pub holder_principal: Option<String>,
    pub glob_pattern: String,
    pub exclusive: bool,
    pub ttl_ms: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAgentMailFileLeaseRequest {
    pub holder_principal: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListAgentMailFileLeasesResponse {
    pub items: Vec<AgentMailFileLeaseResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateAgentMailFileLeaseResponse {
    pub lease: AgentMailFileLeaseResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseAgentMailFileLeaseResponse {
    pub lease: AgentMailFileLeaseResponse,
}

#[cfg(test)]
mod tests {
    use super::{ws_event_entity_from_type, WsEventFrame, WS_EVENT_SCHEMA_VERSION_V1};

    #[test]
    fn ws_event_frame_populates_legacy_and_v1_fields() {
        let frame = WsEventFrame::new(
            42,
            "board.card.moved",
            serde_json::json!({
                "request_id": "req_123",
                "board_id": "board_1",
                "card_id": "card_1"
            }),
        );
        assert_eq!(frame.v, 1);
        assert_eq!(frame.r#type, "event");
        assert_eq!(frame.event, "board.card.moved");
        assert_eq!(frame.data["card_id"], "card_1");
        assert_eq!(frame.schema_version, WS_EVENT_SCHEMA_VERSION_V1);
        assert_eq!(frame.event_id, "evt-42");
        assert_eq!(frame.event_type, "board.card.moved");
        assert_eq!(frame.request_id.as_deref(), Some("req_123"));
        assert_eq!(frame.entity, "board.card");
        assert_eq!(frame.payload["board_id"], "board_1");
    }

    #[test]
    fn ws_event_entity_mapper_covers_primary_domains() {
        assert_eq!(
            ws_event_entity_from_type("board.asset.uploaded"),
            "board.asset"
        );
        assert_eq!(ws_event_entity_from_type("approval.requested"), "approval");
        assert_eq!(
            ws_event_entity_from_type("agent_mail.message.created"),
            "agent_mail.message"
        );
        assert_eq!(ws_event_entity_from_type("gateway.status"), "gateway");
    }
}
