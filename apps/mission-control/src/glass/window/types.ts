export type FloorPresenceActivity =
  | "busy"
  | "idle"
  | "recovering"
  | "offline"
  | "unknown";
export type FloorPresenceMood =
  | "focused"
  | "calm"
  | "recovering"
  | "offline"
  | "unknown";

export interface FloorPresenceTarget {
  kind: "delegation" | "session" | "run";
  id: string;
}

export interface FloorPresenceItem {
  agent_id: string;
  display_name: string;
  activity: FloorPresenceActivity;
  activity_label: string;
  mood: FloorPresenceMood;
  observed_at_ms: number | null;
  source: "local_storage";
  target: FloorPresenceTarget | null;
}

export interface FloorPresenceResponse {
  generated_at_ms: number;
  refresh_after_ms: number;
  items: FloorPresenceItem[];
}

export interface OfficeChatterRoom {
  thread_id: string;
  workstream_id: string;
  label: string;
  unread_count: number | null;
  last_activity_at_ms: number | null;
}

export interface OfficeChatterMessage {
  message_id: string;
  thread_id: string;
  author: {
    kind: "execass" | "owner";
    display_name: string;
  };
  text: string;
  created_at_ms: number;
  source: {
    kind: "execass_event" | "owner_message";
    event_name: string | null;
    workstream_id: string;
    revision: number | null;
  };
}

export interface OfficeChatterResponse {
  rooms: OfficeChatterRoom[];
  messages: OfficeChatterMessage[];
}

export interface CreateOfficeChatterMessageResponse {
  message: OfficeChatterMessage;
}
