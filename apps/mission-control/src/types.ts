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
