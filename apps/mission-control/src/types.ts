export interface BoardSummary {
  board_id: string;
  board_key: string;
  name: string;
  board_type: string;
  created_at: number;
  updated_at: number;
  column_count: number;
  card_count: number;
}

export interface BoardColumn {
  column_id: string;
  board_id: string;
  column_key: string;
  name: string;
  position: number;
  created_at: number;
  updated_at: number;
}

export interface BoardCardAsset {
  card_asset_id: string;
  card_id: string;
  filename: string;
  mime: string;
  sha256: string;
  bytes: number;
  local_path: string;
  created_at: number;
}

export interface BoardCard {
  card_id: string;
  board_id: string;
  column_id: string;
  title: string;
  description: string | null;
  owner_kind: "agent" | "human" | "unassigned" | (string & {});
  owner_agent_id: string | null;
  owner_human_id: string | null;
  due_at: number | null;
  tags: string[];
  script_markdown: string | null;
  linked_session_id: string | null;
  latest_run_id: string | null;
  position: number;
  created_at: number;
  updated_at: number;
  assets: BoardCardAsset[];
}

export interface BoardDetail {
  board: BoardSummary;
  columns: BoardColumn[];
  cards: BoardCard[];
}

export interface ListBoardsResponse {
  items: BoardSummary[];
}

export type BoardDetailResponse = BoardDetail;

export interface UpdateBoardCardResponse {
  card: BoardCard;
}

export interface MoveBoardCardResponse {
  card: BoardCard;
}

export interface UploadBoardCardAssetResponse {
  card: BoardCard;
  asset: BoardCardAsset;
}

export interface RunBoardCardResponse {
  card: BoardCard;
  run: {
    run_id: string;
    status: string;
  };
}

export interface ListAgentsResponse {
  items: Agent[];
}

export interface Agent {
  agent_id: string;
  name: string;
  model_provider: string;
  model_id: string;
  workspace_root?: string;
  tool_profile?: string;
}

export interface CreateAgentResponse {
  agent: Agent;
}

export interface UpdateAgentResponse {
  agent: Agent;
}

export interface WsEventFrame {
  schema_version: string;
  event_id: string;
  event_type: string;
  ts_unix_ms: number;
  request_id?: string | null;
  entity: string;
  payload: Record<string, unknown>;
}

export interface HealthResponse {
  status?: string;
  service?: string;
  ok?: boolean;
}

export interface RuntimeConnectionSettings {
  gateway_url: string;
}

export interface MissionControlCalendarJob {
  job_id: string;
  name: string;
  agent_id: string;
  enabled: boolean;
  schedule_kind: string;
  interval_seconds: number | null;
  cron_expr: string | null;
  next_run_at: number | null;
  last_run_at: number | null;
  last_error: string | null;
  lane: string;
  primary_action: string;
}

export interface MissionControlCalendarWeekResponse {
  week_start_ms: number;
  week_end_ms: number;
  generated_at_ms: number;
  always_running: MissionControlCalendarJob[];
  next_up: MissionControlCalendarJob[];
  jobs: MissionControlCalendarJob[];
}

export interface MissionControlFocusItem {
  item_id: string;
  category: string;
  severity: string;
  title: string;
  detail: string;
  primary_action: string;
  action_payload: Record<string, unknown>;
  created_at: number;
}

export interface MissionControlFocusResponse {
  generated_at_ms: number;
  items: MissionControlFocusItem[];
}

export interface JobResponse {
  job_id: string;
  agent_id: string;
  name: string;
  enabled: boolean;
  schedule_kind: string;
  interval_seconds: number | null;
  run_at_ms: number | null;
  cron_expr: string | null;
  next_run_at: number | null;
  payload_json: string;
  max_retries: number;
  retry_backoff_ms: number;
  timeout_ms: number;
  last_run_at: number | null;
  last_error: string | null;
  created_at: number;
  updated_at: number;
}

export interface ListJobsResponse {
  items: JobResponse[];
}

export interface RunJobNowResponse {
  job_run: {
    job_run_id: string;
    status: string;
    attempt: number;
    started_at: number | null;
    ended_at: number | null;
    error_text: string | null;
    output_json: string | null;
  };
}

export interface UpdateJobResponse {
  job: JobResponse;
}

export interface ApprovalResponse {
  approval_id: string;
  run_id: string;
  kind: string;
  status: string;
  request_summary: string;
  requested_at: number;
  decided_at: number | null;
}

export interface ListApprovalsResponse {
  items: ApprovalResponse[];
}

export interface ResolveApprovalResponse {
  approval: ApprovalResponse;
}

export interface MemoryNoteResponse {
  note_id: string;
  title: string | null;
  body: string;
  tags: string[];
  created_at: number;
  updated_at: number;
}

export interface ListMemoryNotesResponse {
  items: MemoryNoteResponse[];
}

export interface CreateMemoryNoteResponse {
  note: MemoryNoteResponse;
}

export interface ChannelRuntimeAdapterStatusResponse {
  provider: string;
  lifecycle_state: string;
  healthy: boolean;
  detail: string | null;
  last_error: string | null;
  reconnect_attempts: number;
  updated_at: number;
}

export interface GetChannelRuntimeStatusResponse {
  updated_at: number;
  items: ChannelRuntimeAdapterStatusResponse[];
}

export interface SchedulerLockStateResponse {
  enabled: boolean;
  lock_path: string;
  owner: string;
  detail: string | null;
}

export interface CircuitBreakerStateResponse {
  scope: string;
  target_id: string;
  state: string;
  consecutive_failures: number;
  cooldown_until: number | null;
  last_error_code: string | null;
  updated_at: number;
}

export interface FailureReasonCountResponse {
  code: string;
  count: number;
}

export interface RuntimeTrustContractLockSummaryResponse {
  enforced: boolean;
  lock_path: string;
  trust_hash: string;
  locked_at: number;
  drift_detected: boolean;
}

export interface PluginRuntimeStatusResponse {
  plugin_id: string;
  enabled: boolean;
  faulted: boolean;
  disabled_until_ms: number | null;
  consecutive_failures: number;
  last_error_code: string | null;
  last_error: string | null;
  last_success_ms: number | null;
  last_invoked_ms: number | null;
}

export interface StatusResponse {
  service: string;
  version: string;
  started_at_utc: string;
  uptime_ms: number;
  db_path: string;
  attachments_path: string;
  trust_contract_lock: RuntimeTrustContractLockSummaryResponse;
  scheduler_lock: SchedulerLockStateResponse;
  open_circuit_breakers: number;
  circuit_breakers: CircuitBreakerStateResponse[];
  open_plugin_breakers: number;
  plugin_breakers: PluginRuntimeStatusResponse[];
  top_stop_reasons: FailureReasonCountResponse[];
}

export interface JobStatusResponse {
  scheduler_running: boolean;
  scheduler_lock: SchedulerLockStateResponse;
  jobs_total: number;
  jobs_enabled: number;
  jobs_due: number;
  open_circuit_breakers: number;
  circuit_breakers: CircuitBreakerStateResponse[];
  top_stop_reasons: FailureReasonCountResponse[];
  now_utc: string;
}

export interface AuthProfileResponse {
  auth_profile_id: string;
  provider: string;
  display_name: string;
  auth_mode: string;
  risk_level: string;
  enabled: boolean;
  kill_switch_scope: string;
  api_base_url: string | null;
  created_at: number;
  updated_at: number;
}

export interface ListAuthProfilesResponse {
  items: AuthProfileResponse[];
}

export interface AgentProviderProfileOrderResponse {
  agent_id: string;
  provider: string;
  profile_ids: string[];
}

export interface OpenAiOauthStartResponse {
  oauth_session_id: string;
  authorize_url: string;
  callback_url: string;
  expires_in_seconds: number;
}

export interface OpenAiOauthFinishResponse {
  profile: AuthProfileResponse;
  account_id: string | null;
  expires_at_unix: number | null;
}

export interface AnthropicSetupTokenIngestResponse {
  profile: AuthProfileResponse;
}

export interface SkillResponse {
  skill_id: string;
  title: string;
  source_path: string;
  enabled: boolean;
  content_chars: number;
  preview: string;
}

export interface ListSkillsResponse {
  contract_version: string;
  items: SkillResponse[];
}

export interface UpdateSkillStateResponse {
  skill: SkillResponse;
}

export interface PluginCapabilityResponse {
  name: string;
  description: string | null;
}

export interface PluginArtifactResponse {
  exec_kind: string;
  local_path: string;
  command: string;
  args: string[];
  sha256: string;
}

export interface PluginCompatibilityResponse {
  min_gateway_version: string | null;
}

export interface PluginPermissionsResponse {
  allowed_roots: string[];
  network_policy: string;
  network_allowlist: string[];
}

export interface PluginLimitsResponse {
  timeout_ms: number | null;
  max_output_chars: number | null;
}

export interface PluginManifestResponse {
  schema_version: string;
  plugin_id: string;
  display_name: string;
  plugin_version: string;
  api_version: string;
  enabled: boolean;
  artifact: PluginArtifactResponse;
  compatibility: PluginCompatibilityResponse;
  permissions: PluginPermissionsResponse;
  limits: PluginLimitsResponse;
  tools: PluginCapabilityResponse[];
  hooks: PluginCapabilityResponse[];
  providers: PluginCapabilityResponse[];
  channels: PluginCapabilityResponse[];
}

export interface ListPluginsResponse {
  contract_version: string;
  plugin_api_version: string;
  items: PluginManifestResponse[];
}

export interface ListPluginRuntimeStatusResponse {
  contract_version: string;
  items: PluginRuntimeStatusResponse[];
}

export interface UpdatePluginResponse {
  plugin: PluginManifestResponse;
  rollback_available: boolean;
  hook_policy_denials: number;
}

export interface AgentMailThreadSummaryResponse {
  thread_id: string;
  kind: string;
  subject: string;
  created_by_principal: string;
  participant_count: number;
  message_count: number;
  latest_message_at: number | null;
  latest_message_preview: string | null;
  latest_sender_principal: string | null;
  unread_count: number;
  created_at: number;
  updated_at: number;
}

export interface AgentMailThreadParticipantResponse {
  principal_id: string;
  role: string;
  joined_at: number;
  last_read_at: number | null;
  muted: boolean;
}

export interface AgentMailMessageRecipientResponse {
  recipient_principal: string;
  delivered_at: number;
  acked_at: number | null;
}

export interface AgentMailAttachmentResponse {
  attachment_id: string;
  message_id: string;
  filename: string;
  mime: string;
  sha256: string;
  bytes: number;
  created_at: number;
}

export interface AgentMailMessageResponse {
  message_id: string;
  thread_id: string;
  sender_principal: string;
  sender_kind: string;
  body_text: string;
  metadata_json: string | null;
  created_at: number;
  recipients: AgentMailMessageRecipientResponse[];
  attachments: AgentMailAttachmentResponse[];
}

export interface AgentMailThreadDetailResponse {
  thread: AgentMailThreadSummaryResponse;
  participants: AgentMailThreadParticipantResponse[];
}

export interface ListAgentMailThreadsResponse {
  items: AgentMailThreadSummaryResponse[];
}

export interface ListAgentMailMessagesResponse {
  items: AgentMailMessageResponse[];
}

export interface CreateAgentMailThreadResponse {
  thread: AgentMailThreadSummaryResponse;
}

export interface SendAgentMailMessageResponse {
  message: AgentMailMessageResponse;
}

export interface AckAgentMailMessageResponse {
  message_id: string;
  recipient_principal: string;
  acked_at: number | null;
}

export interface UploadAgentMailAttachmentResponse {
  attachment: AgentMailAttachmentResponse;
}

export interface AgentMailFileLeaseResponse {
  lease_id: string;
  holder_principal: string;
  glob_pattern: string;
  exclusive: boolean;
  ttl_ms: number;
  note: string | null;
  created_at: number;
  expires_at: number;
  released_at: number | null;
}

export interface ListAgentMailFileLeasesResponse {
  items: AgentMailFileLeaseResponse[];
}

export interface CreateAgentMailFileLeaseResponse {
  lease: AgentMailFileLeaseResponse;
}

export interface ReleaseAgentMailFileLeaseResponse {
  lease: AgentMailFileLeaseResponse;
}
