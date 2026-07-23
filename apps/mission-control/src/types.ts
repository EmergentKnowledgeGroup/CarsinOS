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

export interface CreateWebSocketTicketResponse {
  ticket: string;
  expires_at: number;
}

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
  reports_to_agent_id?: string | null;
  role_label?: string | null;
  memory_binding?: AgentMemoryBindingResponse | null;
}

export interface AgentMemoryBindingResponse {
  binding_id: string;
  provider_kind: string;
  base_url: string;
  auth_mode: string;
  auth_secret_ref?: string | null;
  principal_id?: string | null;
  principal_display_name?: string | null;
  enabled: boolean;
  trusted_local_operator_actions: boolean;
}

export interface AgentMemoryBindingRequest {
  binding_id?: string;
  provider_kind?: string;
  base_url?: string;
  auth_mode?: string;
  auth_secret_ref?: string | null;
  principal_id?: string | null;
  principal_display_name?: string | null;
  enabled?: boolean;
  trusted_local_operator_actions?: boolean;
}

export interface UpdateAgentMemoryBindingRequest {
  binding_id?: string;
  provider_kind?: string;
  base_url?: string;
  auth_mode?: string;
  auth_secret_ref?: string | null;
  principal_id?: string | null;
  principal_display_name?: string | null;
  enabled?: boolean;
  trusted_local_operator_actions?: boolean;
}

export interface AgentMemoryNativeSurfaceAvailabilityResponse {
  cards: boolean;
  card_detail: boolean;
  atom_detail: boolean;
  graph_overview: boolean;
  graph_neighbors: boolean;
  episodes: boolean;
  turn_why: boolean;
  citation_lookup: boolean;
  runtime_health: boolean;
  telemetry_summary: boolean;
  telemetry_turns: boolean;
  decision_reasons: boolean;
}

export interface AgentMemoryOrchestrationStatusResponse {
  enabled: boolean;
  transport: string;
  health_status: string;
  degrade_mode: boolean;
  last_error_code: string | null;
  last_error: string | null;
}

export interface AgentMemoryStatusResponse {
  agent_id: string;
  binding_status: string;
  binding?: AgentMemoryBindingResponse | null;
  native_surface_availability: AgentMemoryNativeSurfaceAvailabilityResponse;
  orchestration: AgentMemoryOrchestrationStatusResponse;
  native_runtime_status: Record<string, unknown> | null;
  native_runtime_health_mismatch: boolean;
}

export interface GetAgentMemoryStatusResponse {
  status: AgentMemoryStatusResponse;
}

export interface AgentMemoryLaneStatusResponse {
  human_identity_id: string;
  assistant_agent_id: string;
  lane_id: string;
  configured_memory_mode: string;
  effective_memory_mode: string;
  source: string;
  status: string;
  detail?: string | null;
  local_memory_sources: string[];
  orchestration?: AgentMemoryOrchestrationStatusResponse | null;
}

export interface ListAgentMemoryLaneStatusesResponse {
  items: AgentMemoryLaneStatusResponse[];
}

export interface SyncMemorySourceItemResponse {
  source_path: string;
  note_id: string | null;
  status: string;
  detail: string | null;
  synced_at: number;
}

export interface SyncMemorySourcesResponse {
  items: SyncMemorySourceItemResponse[];
  synced: number;
  failed: number;
}

export interface AgentMemoryJsonPayloadResponse<T = unknown> {
  agent_id: string;
  binding_id: string;
  data: T;
}

export interface AgentMemoryCardSummary {
  atom_id: string;
  card_id?: string;
  kind: string;
  status?: string;
  summary?: string;
  contradiction?: string;
  distance?: number;
  via_edge_kind?: string;
  [key: string]: unknown;
}

export interface AgentMemoryGraphLink {
  source: string;
  target: string;
  kind: string;
  [key: string]: unknown;
}

export interface AgentMemoryCardsPayload {
  ok: boolean;
  total: number;
  cards: AgentMemoryCardSummary[];
}

export interface AgentMemoryCardDetailPayload {
  ok: boolean;
  card: AgentMemoryCardSummary & Record<string, unknown>;
  atom?: Record<string, unknown> | null;
  provenance_events?: Array<Record<string, unknown>>;
  graph?: {
    nodes?: AgentMemoryCardSummary[];
    links?: AgentMemoryGraphLink[];
    [key: string]: unknown;
  } | null;
}

export interface AgentMemoryAtomDetailPayload {
  ok: boolean;
  atom: Record<string, unknown>;
  card?: AgentMemoryCardSummary | null;
  graph?: {
    nodes?: AgentMemoryCardSummary[];
    links?: AgentMemoryGraphLink[];
    [key: string]: unknown;
  } | null;
}

export interface AgentMemoryEpisodeSummary {
  episode_id: string;
  label?: string;
  status?: string;
  run_id?: string;
  card_id?: string;
  updated_at_utc?: string;
  [key: string]: unknown;
}

export interface AgentMemoryEpisodesPayload {
  ok: boolean;
  total: number;
  episodes: AgentMemoryEpisodeSummary[];
}

export interface AgentMemoryGraphMapPayload {
  ok: boolean;
  total: number;
  nodes: AgentMemoryCardSummary[];
  links: AgentMemoryGraphLink[];
  truncated: boolean;
  snapshot_available?: boolean;
}

export interface AgentMemoryGraphNeighborsPayload {
  ok: boolean;
  node: AgentMemoryCardSummary;
  neighbors: AgentMemoryCardSummary[];
  links: AgentMemoryGraphLink[];
  depth: number;
  node_limit: number;
  link_limit: number;
  requests_used: number;
  truncated: boolean;
  truncation?: {
    node_limit_hit?: boolean;
    link_limit_hit?: boolean;
    request_budget_hit?: boolean;
    dropped_shared_language?: boolean;
    [key: string]: unknown;
  };
}

export interface AgentMemoryWhyReason {
  label?: string;
  detail?: string;
  [key: string]: unknown;
}

export interface AgentMemoryWhyEvidence {
  source_id?: string;
  excerpt?: string;
  confidence?: number;
  [key: string]: unknown;
}

export interface AgentMemoryWhyCitation {
  token?: string;
  citation_token?: string;
  label?: string;
  source_id?: string;
  [key: string]: unknown;
}

export interface AgentMemoryTurnWhyPayload {
  ok: boolean;
  why: {
    decision?: string;
    decision_reason?: string;
    evidence_time_window?: string;
    top_evidence?: AgentMemoryWhyEvidence[];
    reasons?: AgentMemoryWhyReason[];
    citations?: AgentMemoryWhyCitation[];
    citations_hidden?: boolean;
    [key: string]: unknown;
  };
}

export interface AgentMemoryCitationMatch {
  line_number?: number;
  excerpt?: string;
  [key: string]: unknown;
}

export interface AgentMemoryCitationPayload {
  ok: boolean;
  citation?: string;
  source_id?: string;
  matches?: AgentMemoryCitationMatch[];
  [key: string]: unknown;
}

export interface AgentMemoryRuntimeHealthPayload {
  ok: boolean;
  status: string;
  checked_at?: string;
  checks?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

export interface AgentMemoryTelemetrySummaryPayload {
  ok: boolean;
  limit: number;
  summary: Array<Record<string, unknown>>;
}

export interface AgentMemoryTelemetryTurnsPayload {
  ok: boolean;
  limit: number;
  turns: Array<Record<string, unknown>>;
  warn_turns?: Array<Record<string, unknown>>;
}

export interface AgentMemoryDecisionReasonsPayload {
  ok: boolean;
  routes?: Array<Record<string, unknown>>;
  memory_preferences?: Array<Record<string, unknown>>;
  reasons?: Array<Record<string, unknown>>;
}

export interface CreateAgentResponse {
  agent: Agent;
}

export interface UpdateAgentResponse {
  agent: Agent;
}

export interface GoalResponse {
  goal_id: string;
  slug: string;
  title: string;
  summary: string;
  status: string;
  owner_agent_id: string | null;
  target_date: number | null;
  progress_pct: number;
  created_at: number;
  updated_at: number;
}

export interface ListGoalsResponse {
  items: GoalResponse[];
  next_cursor: string | null;
}

export interface CreateGoalResponse {
  goal: GoalResponse;
}

export interface UpdateGoalResponse {
  goal: GoalResponse;
}

export interface ProjectResponse {
  project_id: string;
  goal_id: string;
  slug: string;
  name: string;
  summary: string;
  status: string;
  owner_agent_id: string | null;
  workspace_root: string | null;
  budget_month_usd: number | null;
  created_at: number;
  updated_at: number;
}

export interface ListProjectsResponse {
  items: ProjectResponse[];
  next_cursor: string | null;
}

export interface CreateProjectResponse {
  project: ProjectResponse;
}

export interface UpdateProjectResponse {
  project: ProjectResponse;
}

export interface TaskResponse {
  task_id: string;
  project_id: string;
  parent_task_id: string | null;
  title: string;
  detail: string;
  status: string;
  priority: string;
  owner_agent_id: string | null;
  due_at: number | null;
  blocked_reason: string | null;
  linked_board_card_id: string | null;
  linked_job_id: string | null;
  latest_run_id: string | null;
  latest_session_id: string | null;
  created_at: number;
  updated_at: number;
}

export interface ListTasksResponse {
  items: TaskResponse[];
  next_cursor: string | null;
}

export interface CreateTaskResponse {
  task: TaskResponse;
}

export interface UpdateTaskResponse {
  task: TaskResponse;
}

export interface TaskLinkMutationResponse {
  task: TaskResponse;
}

export interface StrategyTaskListItemResponse {
  task_id: string;
  title: string;
  status: string;
  priority: string;
  owner_agent_id: string | null;
  owner_name: string | null;
  project_id: string;
  project_name: string;
  goal_id: string;
  goal_title: string;
  updated_at: number;
  due_at: number | null;
  blocked_reason: string | null;
}

export interface StrategySpendByAgentItemResponse {
  agent_id: string;
  agent_name: string;
  estimated_cost_total: number;
  linked_task_count: number;
}

export interface StrategySpendByProjectItemResponse {
  project_id: string;
  project_name: string;
  goal_id: string;
  goal_title: string;
  estimated_cost_total: number;
  attributed_run_count: number;
}

export interface StrategyGoalProgressItemResponse {
  goal_id: string;
  title: string;
  progress_pct: number;
  open_task_count: number;
  blocked_task_count: number;
}

export interface StrategyApprovalBacklogItemResponse {
  approval_id: string;
  kind: string;
  summary: string;
  linked_task_id: string | null;
  requested_at: number;
}

export interface StrategySummaryResponse {
  generated_at_ms: number;
  currency: string;
  blocked_task_count: number;
  blocked_tasks: StrategyTaskListItemResponse[];
  stale_task_count: number;
  stale_tasks: StrategyTaskListItemResponse[];
  spend_by_agent: StrategySpendByAgentItemResponse[];
  spend_by_project: StrategySpendByProjectItemResponse[];
  unattributed_spend_total: number;
  goal_progress: StrategyGoalProgressItemResponse[];
  critical_approval_backlog_count: number;
  critical_approval_backlog: StrategyApprovalBacklogItemResponse[];
}

export interface RunbookDeepLinkTargetResponse {
  tab: string;
  target_kind: string;
  target_id: string | null;
  context: string | null;
}

export interface RunbookEntityRefResponse {
  entity_kind: string;
  entity_id: string;
  display_label: string;
  deep_link: RunbookDeepLinkTargetResponse;
}

export interface RunbookExecutionRefResponse {
  entity_kind: string;
  entity_id: string;
  created_at_ms: number;
  started_at_ms: number | null;
  waiting_since_ms: number | null;
  finished_at_ms: number | null;
}

export interface RunbookDataAvailabilityResponse {
  is_limited: boolean;
  is_stale: boolean;
  last_refresh_at_ms: number;
  missing_source_kinds: string[];
  stale_reason: string | null;
}

export interface RunbookWarningResponse {
  warning_id: string;
  warning_kind: string;
  message: string;
}

export interface RunbookSourceFactResponse {
  fact_id: string;
  fact_kind: string;
  entity_ref: RunbookEntityRefResponse | null;
  occurred_at_ms: number | null;
  partial: boolean;
}

export interface RunbookActionResponse {
  action_id: string;
  action_kind: string;
  label: string;
  availability: string;
  disabled_reason: string | null;
  target_entity_ref: RunbookEntityRefResponse | null;
}

export interface RunbookStepResponse {
  step_id: string;
  label: string;
  kind: string;
  state: string;
  state_reason: string | null;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  waiting_since_ms: number | null;
  linked_entity_refs: RunbookEntityRefResponse[];
  action_refs: string[];
  template_index: number;
}

export interface RunbookHistoryItemResponse {
  history_id: string;
  event_kind: string;
  label: string;
  detail: string | null;
  occurred_at_ms: number;
  step_id: string | null;
  entity_refs: RunbookEntityRefResponse[];
}

export interface RunbookStatusCountsResponse {
  pending: number;
  active: number;
  waiting: number;
  blocked: number;
  failed: number;
  completed: number;
  limited: number;
}

export interface RunbookSummaryItemResponse {
  runbook_id: string;
  runbook_kind: string;
  anchor_kind: string;
  anchor_id: string;
  title: string;
  status: string;
  status_reason: string | null;
  owner_agent_id: string | null;
  owner_agent_label: string | null;
  primary_entity_label: string;
  updated_at_ms: number;
  current_step_label: string | null;
  warning_count: number;
  linked_entities: RunbookEntityRefResponse[];
  availability: RunbookDataAvailabilityResponse;
}

export interface ListRunbooksResponse {
  generated_at_ms: number;
  items: RunbookSummaryItemResponse[];
  counts_by_status: RunbookStatusCountsResponse;
  next_cursor: string | null;
}

export interface RunbookDetailResponse {
  runbook_id: string;
  runbook_kind: string;
  template_id: string;
  template_version: string;
  anchor_kind: string;
  anchor_id: string;
  title: string;
  status: string;
  status_reason: string | null;
  generated_at_ms: number;
  selected_execution_ref: RunbookExecutionRefResponse | null;
  active_step_id: string | null;
  next_step_ids: string[];
  linked_entities: RunbookEntityRefResponse[];
  steps: RunbookStepResponse[];
  history: RunbookHistoryItemResponse[];
  actions: RunbookActionResponse[];
  source_facts: RunbookSourceFactResponse[];
  availability: RunbookDataAvailabilityResponse;
  warnings: RunbookWarningResponse[];
  owner_agent_id: string | null;
  owner_agent_label: string | null;
}

export interface BootstrapPresetResponse {
  schema_version: string;
  preset_key: string;
  display_name: string;
  description: string;
  role_label: string;
  provider_path: string;
  default_model_provider: string | null;
  default_model_id: string | null;
  default_tool_profile: string | null;
  default_workspace_root: string | null;
  default_reports_to_agent_id: string | null;
  setup_notes: string | null;
  created_at: number;
  updated_at: number;
}

export interface ListBootstrapPresetsResponse {
  items: BootstrapPresetResponse[];
  next_cursor: string | null;
}

export interface CreateBootstrapPresetResponse {
  preset: BootstrapPresetResponse;
}

export interface UpdateBootstrapPresetResponse {
  preset: BootstrapPresetResponse;
}

export interface ExportBootstrapPresetResponse {
  preset: BootstrapPresetResponse;
}

export interface ImportBootstrapPresetResponse {
  preset: BootstrapPresetResponse;
}

export interface SessionSummaryResponse {
  session_id: string;
  session_key: string;
  agent_id: string;
  title: string | null;
  created_at: number;
  updated_at: number;
  closed_at: number | null;
  message_count: number;
  run_count: number;
}

export interface CreateSessionResponse {
  session: SessionSummaryResponse;
}

export interface MessageResponse {
  message_id: string;
  session_id: string;
  source_channel: string;
  source_peer_id: string | null;
  source_message_id: string | null;
  role: string;
  content_text: string;
  content_format: string;
  created_at: number;
}

export interface CreateMessageResponse {
  message: MessageResponse;
}

export interface ListMessagesResponse {
  items: MessageResponse[];
}

export interface RunResponse {
  run_id: string;
  session_id: string;
  status: string;
  model_provider: string;
  model_id: string;
  started_at: number | null;
  ended_at: number | null;
  error_text: string | null;
  usage_json: string | null;
  created_at: number;
}

export interface CreateRunResponse {
  run: RunResponse;
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


export type MissionControlUsageWindow = "today" | "week" | "custom";

export interface MissionControlUsageByAgent {
  agent_id: string;
  agent_name: string;
  estimated_cost_total: number;
  token_input_total: number;
  token_output_total: number;
}

export interface MissionControlUsageByModel {
  model_provider: string;
  model_id: string;
  estimated_cost_total: number;
  token_input_total: number;
  token_output_total: number;
}

export interface MissionControlUsageByProvider {
  provider: string;
  estimated_cost_total: number;
  token_input_total: number;
  token_output_total: number;
}

export interface MissionControlUsageByTime {
  bucket_start_utc: string;
  bucket_end_utc: string;
  estimated_cost_total: number;
  token_input_total: number;
  token_output_total: number;
}

export interface MissionControlUsageByJob {
  job_id: string;
  name: string | null;
  estimated_cost_total: number;
  token_input_total: number;
  token_output_total: number;
}

export interface MissionControlUsageByCard {
  card_id: string;
  title: string | null;
  estimated_cost_total: number;
  token_input_total: number;
  token_output_total: number;
}

export interface MissionControlUsageBudgetThreshold {
  provider: string;
  daily_token_budget: number | null;
  daily_cost_usd_budget: number | null;
  token_usage_total: number;
  cost_usage_total: number;
  token_ratio: number | null;
  cost_ratio: number | null;
}

export interface MissionControlUsageResponse {
  contract_version: string;
  available: boolean;
  window: MissionControlUsageWindow | string;
  timezone: string;
  currency: string;
  window_start_utc: string | null;
  window_end_utc: string | null;
  estimated_cost_total: number | null;
  token_input_total: number | null;
  token_output_total: number | null;
  by_agent: MissionControlUsageByAgent[] | null;
  by_model: MissionControlUsageByModel[] | null;
  by_provider: MissionControlUsageByProvider[] | null;
  by_time: MissionControlUsageByTime[] | null;
  by_job: MissionControlUsageByJob[] | null;
  by_card: MissionControlUsageByCard[] | null;
  budget_thresholds: MissionControlUsageBudgetThreshold[] | null;
  updated_at_utc: string | null;
  reason_code?: string | null;
  detail?: string | null;
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

export interface CreateJobRequest {
  agent_id?: string;
  name: string;
  enabled?: boolean;
  schedule_kind: string;
  interval_seconds?: number | null;
  run_at_ms?: number | null;
  cron_expr?: string | null;
  payload_json?: Record<string, unknown>;
  max_retries?: number;
  retry_backoff_ms?: number;
  timeout_ms?: number;
}

export interface CreateJobResponse {
  job: JobResponse;
}

export interface ListJobsResponse {
  items: JobResponse[];
}

export interface JobRunResponse {
  job_run_id: string;
  job_id: string;
  trigger_kind: string;
  status: string;
  attempt: number;
  started_at: number | null;
  ended_at: number | null;
  error_text: string | null;
  output_json: string | null;
  created_at: number;
}

export interface RunJobNowResponse {
  job_run: JobRunResponse;
}

export interface ListJobHistoryResponse {
  items: JobRunResponse[];
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
  session_state: string;
  proof_state: string;
  detail: string | null;
  proof_detail: string | null;
  last_error: string | null;
  last_inbound_at: number | null;
  last_outbound_at: number | null;
  last_proven_at: number | null;
  reconnect_attempts: number;
  updated_at: number;
}

export interface GetChannelRuntimeStatusResponse {
  updated_at: number;
  items: ChannelRuntimeAdapterStatusResponse[];
}

export interface DiscordChannelConfigResponse {
  require_mention_in_guild_channels: boolean;
  allowlisted_user_ids: string[];
  auto_run_enabled: boolean;
  default_agent_id: string | null;
  default_model_provider: string;
  default_model_id: string;
}

export interface TelegramChannelConfigResponse {
  require_mention_in_groups: boolean;
  allowlisted_user_ids: number[];
  dm_policy: string;
  group_policy: string;
  group_allowlisted_user_ids: number[];
  allowlisted_chat_ids: number[];
  auto_leave_unauthorized_groups: boolean;
  pairing_code_ttl_seconds: number;
  pairing_max_pending: number;
  unauthorized_spam_threshold: number;
  unauthorized_spam_block_seconds: number;
  auto_run_enabled: boolean;
  default_agent_id: string | null;
  default_model_provider: string;
  default_model_id: string;
}

export interface ChannelConfigResponse {
  discord: DiscordChannelConfigResponse;
  telegram: TelegramChannelConfigResponse;
  updated_at: number;
}

export interface GetChannelConfigResponse {
  config: ChannelConfigResponse;
}

export interface UpdateChannelConfigResponse {
  config: ChannelConfigResponse;
}

export interface TelegramPairingPendingRequestResponse {
  code: string;
  user_id: number;
  chat_id: number;
  preview_text: string;
  first_seen_at: number;
  last_seen_at: number;
  expires_at: number;
  attempt_count: number;
}

export interface TelegramBlockedSenderResponse {
  user_id: number;
  blocked_until: number;
  reason: string;
  attempt_count: number;
  last_attempt_at: number;
}

export interface GetTelegramPairingStatusResponse {
  dm_policy: string;
  group_policy: string;
  auto_leave_unauthorized_groups: boolean;
  pending_requests: TelegramPairingPendingRequestResponse[];
  blocked_senders: TelegramBlockedSenderResponse[];
  updated_at: number;
}

export interface ResolveTelegramPairingResponse {
  status: GetTelegramPairingStatusResponse;
  approved_user_id: number | null;
  linked_human_identity_id: string | null;
}

export interface DiscordPairingPendingRequestResponse {
  code: string;
  user_id: string;
  channel_id: string;
  preview_text: string;
  first_seen_at: number;
  last_seen_at: number;
  expires_at: number;
  attempt_count: number;
}

export interface DiscordBlockedSenderResponse {
  user_id: string;
  blocked_until: number;
  reason: string;
  attempt_count: number;
  last_attempt_at: number;
}

export interface GetDiscordPairingStatusResponse {
  dm_policy: string;
  pending_requests: DiscordPairingPendingRequestResponse[];
  blocked_senders: DiscordBlockedSenderResponse[];
  updated_at: number;
}

export interface ResolveDiscordPairingResponse {
  status: GetDiscordPairingStatusResponse;
  approved_user_id: string | null;
  linked_human_identity_id: string | null;
}

export interface RuntimeDiscordDeploymentConfigResponse {
  enabled: boolean;
  bot_token_secret_ref: string | null;
  operation_mode: string;
  api_base_url: string | null;
  transport_timeout_ms: number | null;
  transport_retry_attempts: number | null;
  application_id: string | null;
  intents: string[];
  staging_guild_ids: string[];
  staging_channel_ids: string[];
}

export interface RuntimeTelegramDeploymentConfigResponse {
  enabled: boolean;
  bot_token_secret_ref: string | null;
  operation_mode: string;
  api_base_url: string | null;
  transport_timeout_ms: number | null;
  transport_retry_attempts: number | null;
  long_poll_timeout_seconds: number | null;
  webhook_mode: string;
  webhook_url: string | null;
  staging_chat_ids: number[];
}

export interface RuntimeChannelsConfigResponse {
  discord: RuntimeDiscordDeploymentConfigResponse;
  telegram: RuntimeTelegramDeploymentConfigResponse;
}

export interface RuntimeGlobalConfigResponse {
  jwt_issuer_allowlist: string[];
  jwt_audience_allowlist: string[];
  trusted_proxy_allowlist: string[];
  tls_termination_mode: string;
  public_base_url: string | null;
  assistant_system_prompt: string | null;
}

export interface RuntimeHumanIdentityConfigResponse {
  human_identity_id: string;
  display_name: string;
  enabled: boolean;
}

export interface RuntimePlatformIdentityLinkConfigResponse {
  provider: string;
  platform_user_id: string;
  human_identity_id: string;
  display_name: string | null;
  enabled: boolean;
}

export interface RuntimeAssistantAssignmentConfigResponse {
  human_identity_id: string;
  assistant_agent_id: string;
  enabled: boolean;
}

export interface RuntimeLaneMemoryPolicyConfigResponse {
  human_identity_id: string;
  assistant_agent_id: string;
  memory_mode: string;
  lane_id: string | null;
  local_memory_sources: string[];
}

export interface RuntimeRoutingConfigResponse {
  enabled: boolean;
  use_channel_defaults_as_fallback: boolean;
  local_operator_human_identity_id: string | null;
  dm_unmapped_policy: string;
  shared_unmapped_policy: string;
  human_identities: RuntimeHumanIdentityConfigResponse[];
  platform_identity_links: RuntimePlatformIdentityLinkConfigResponse[];
  assistant_assignments: RuntimeAssistantAssignmentConfigResponse[];
  lane_memory_policies: RuntimeLaneMemoryPolicyConfigResponse[];
}

export interface RuntimeMemoryConfigResponse {
  blend_mode: string;
  memory_md_sources: string[];
  numquam: Record<string, unknown>;
}

export interface RuntimeConfigResponse {
  schema_version: string;
  global: RuntimeGlobalConfigResponse;
  providers: Array<Record<string, unknown>>;
  channels: RuntimeChannelsConfigResponse;
  routing: RuntimeRoutingConfigResponse;
  memory: RuntimeMemoryConfigResponse;
  extensions: Record<string, unknown>;
  security: Record<string, unknown>;
  autonomy_guardrails: Record<string, unknown>;
  updated_at: number;
}

export interface GetRuntimeConfigResponse {
  config: RuntimeConfigResponse;
}

export interface UpdateRuntimeConfigResponse {
  config: RuntimeConfigResponse;
}

export interface UpsertRuntimeSecretResponse {
  secret_ref: string;
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

export interface CreateAuthProfileResponse {
  profile: AuthProfileResponse;
}

export interface RevokeAuthProfileResponse {
  profile: AuthProfileResponse;
  revoked_secret_ref: string | null;
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

export interface RemoveAgentResponse {
  removed: boolean;
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

export interface ProviderCapabilityResponse {
  provider: string;
  supports_streaming: boolean;
  supports_tools: boolean;
  supports_json_mode: boolean;
  supports_vision: boolean;
  max_context_tokens: number | null;
  error_classes: string[];
  retryable_error_classes: string[];
}

export interface ListProviderCapabilitiesResponse {
  contract_version: string;
  items: ProviderCapabilityResponse[];
}

export interface ProviderModelResponse {
  model_id: string;
  label: string;
}

export interface ListProviderModelsResponse {
  contract_version: string;
  provider: string;
  auth_profile_id: string | null;
  items: ProviderModelResponse[];
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

export interface ListConnectorCatalogRequest {
  source_kind?: string;
  query?: string;
}

export interface ConnectorCatalogItemResponse {
  catalog_item_id: string;
  slug: string;
  display_name: string;
  source_kind: string;
  summary: string;
  publisher: string;
  trust_class: string;
  available_versions: string[];
  marketplace_origin: string | null;
  importable: boolean;
  future_marketplace_metadata: unknown;
}

export interface ListConnectorCatalogResponse {
  contract_version: string;
  items: ConnectorCatalogItemResponse[];
}

export interface ListConnectorsRequest {
  source_kind?: string;
  status?: string;
  trust_state?: string;
  query?: string;
  include_disabled?: boolean;
}

export interface ConnectorSourceResponse {
  connector_id: string;
  slug: string;
  display_name: string;
  source_kind: string;
  origin_kind: string;
  catalog_item_id: string | null;
  current_version_id: string | null;
  latest_imported_version_id: string | null;
  status: string;
  trust_state: string;
  assigned_agent_count: number;
  published_tool_count: number;
  last_conversion_at: number | null;
  last_review_at: number | null;
  last_enabled_at: number | null;
  last_disabled_at: number | null;
  created_at: number;
  updated_at: number;
}

export interface ConnectorVersionResponse {
  version_id: string;
  connector_id: string;
  version_label: string;
  source_digest: string;
  raw_source_location: string | null;
  import_metadata: unknown;
  schema_summary: unknown;
  latest_conversion_id: string | null;
  external_reference_policy: string;
  created_at: number;
  updated_at: number;
}

export interface ConnectorWarningResponse {
  code: string;
  message: string;
  blocking: boolean;
  field?: string | null;
}

export interface ConnectorProposedToolResponse {
  candidate_id: string;
  operation_key: string;
  proposed_tool_name: string;
  display_name: string;
  description?: string | null;
  input_schema: unknown;
  write_classification: string;
  auth_required: boolean;
  review_blocked: boolean;
  review_block_reason?: string | null;
}

export interface ConnectorUnsupportedOperationResponse {
  operation_key: string;
  display_name: string;
  reason: string;
}

export interface ConnectorConversionResponse {
  conversion_id: string;
  connector_id: string;
  version_id: string;
  status: string;
  warnings: ConnectorWarningResponse[];
  proposed_tools: ConnectorProposedToolResponse[];
  write_capable_tools: number;
  unsupported_operations: ConnectorUnsupportedOperationResponse[];
  normalization_notes: string[];
  diff_from_previous: unknown;
  created_at: number;
  updated_at: number;
}

export interface ConnectorPublishedToolResponse {
  published_tool_id: string;
  connector_id: string;
  version_id: string;
  conversion_id: string;
  tool_name: string;
  display_name: string;
  tool_schema: unknown;
  origin_metadata: unknown;
  write_classification: string;
  published_at: number;
  unpublished_at: number | null;
  superseded_by_published_tool_id: string | null;
  deprecation_state: string;
}

export interface ConnectorAssignmentResponse {
  assignment_id: string;
  connector_id: string;
  agent_id: string;
  enabled: boolean;
  auth_mode: string;
  created_at: number;
  updated_at: number;
}

export interface ConnectorAuthBindingResponse {
  auth_binding_id: string;
  connector_id: string;
  agent_id: string | null;
  auth_kind: string;
  secret_ref?: string | null;
  oauth_session_id?: string | null;
  status: string;
  auth_metadata: unknown;
  last_success_at: number | null;
  last_error: string | null;
  last_rotated_at: number | null;
  created_at: number;
  updated_at: number;
}

export interface ConnectorInteractionResponse {
  interaction_id: string;
  connector_id: string;
  agent_id: string | null;
  interaction_kind: string;
  status: string;
  prompt_summary: string;
  resume_token?: string | null;
  expires_at: number | null;
  consumed_at: number | null;
  detail: unknown;
  created_at: number;
  updated_at: number;
}

export interface ConnectorHealthResponse {
  connector_id: string;
  status: string;
  degraded_reason: string | null;
  auth_required: boolean;
  auth_required_tool_count: number;
  auth_missing_tool_count: number;
  last_checked_at: number | null;
  published_tool_count: number;
  assigned_agent_count: number;
}

export interface ListConnectorsResponse {
  contract_version: string;
  items: ConnectorSourceResponse[];
}

export interface GetConnectorResponse {
  connector: ConnectorSourceResponse;
  versions: ConnectorVersionResponse[];
  conversions: ConnectorConversionResponse[];
  published_tools: ConnectorPublishedToolResponse[];
  assignments: ConnectorAssignmentResponse[];
  auth_bindings: ConnectorAuthBindingResponse[];
  interactions: ConnectorInteractionResponse[];
}

export interface ImportConnectorRequest {
  source_kind: string;
  display_name: string;
  slug?: string;
  catalog_item_id?: string;
  version_label?: string;
  origin_kind?: string;
  import_url?: string;
  source_text?: string;
  source_json?: unknown;
  endpoint_url?: string;
  auth_required?: boolean;
  external_reference_policy?: string;
}

export interface ImportConnectorResponse {
  connector: ConnectorSourceResponse;
  version: ConnectorVersionResponse;
}

export interface RunConnectorConversionRequest {
  version_id?: string;
}

export interface RunConnectorConversionResponse {
  connector: ConnectorSourceResponse;
  version: ConnectorVersionResponse;
  conversion: ConnectorConversionResponse;
}

export interface ConnectorAliasOverrideRequest {
  candidate_id: string;
  alias: string;
}

export interface PublishConnectorToolsRequest {
  conversion_id: string;
  selected_candidate_ids: string[];
  alias_overrides?: ConnectorAliasOverrideRequest[];
  enable_after_publish?: boolean;
}

export interface PublishConnectorToolsResponse {
  connector: ConnectorSourceResponse;
  version: ConnectorVersionResponse;
  published_tools: ConnectorPublishedToolResponse[];
}

export interface UnpublishConnectorToolsRequest {
  published_tool_ids: string[];
}

export interface UnpublishConnectorToolsResponse {
  connector: ConnectorSourceResponse;
  published_tools: ConnectorPublishedToolResponse[];
}

export interface RollbackConnectorVersionRequest {
  version_id: string;
}

export interface RollbackConnectorVersionResponse {
  connector: ConnectorSourceResponse;
  version: ConnectorVersionResponse;
  published_tools: ConnectorPublishedToolResponse[];
}

export interface SetConnectorStateRequest {
  enabled: boolean;
}

export interface SetConnectorStateResponse {
  connector: ConnectorSourceResponse;
}

export interface SetConnectorAssignmentRequest {
  agent_id: string;
  enabled?: boolean;
  auth_mode?: string;
}

export interface SetConnectorAssignmentResponse {
  assignment: ConnectorAssignmentResponse;
}

export interface UpsertConnectorAuthBindingRequest {
  agent_id?: string;
  auth_kind: string;
  secret_ref?: string;
  oauth_session_id?: string;
  auth_metadata?: unknown;
  status?: string;
}

export interface UpsertConnectorAuthBindingResponse {
  binding: ConnectorAuthBindingResponse;
}

export interface ResumeConnectorInteractionRequest {
  payload?: unknown;
}

export interface ResumeConnectorInteractionResponse {
  interaction: ConnectorInteractionResponse;
}

export interface ListConnectorInteractionsResponse {
  contract_version: string;
  items: ConnectorInteractionResponse[];
}

export interface GetConnectorHealthResponse {
  health: ConnectorHealthResponse;
}

export interface DescribeConnectorToolResponse {
  connector: ConnectorSourceResponse;
  published_tool: ConnectorPublishedToolResponse;
}
