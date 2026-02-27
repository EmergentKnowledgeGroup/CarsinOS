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
  owner_kind: "agent" | "human" | "unassigned" | string;
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

export interface BoardDetailResponse {
  board: BoardSummary;
  columns: BoardColumn[];
  cards: BoardCard[];
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
