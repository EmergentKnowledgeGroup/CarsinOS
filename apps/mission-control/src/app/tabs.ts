import type { MissionControlTab } from "./useAppController";

export interface MissionControlTabItem {
  tab: MissionControlTab;
  label: string;
}

export const MISSION_CONTROL_TABS: MissionControlTabItem[] = [
  { tab: "boards", label: "Boards" },
  { tab: "calendar", label: "Calendar" },
  { tab: "focus", label: "Operator Focus" },
  { tab: "events", label: "Event Stream" },
  { tab: "mail", label: "Agent Mail" },
  { tab: "chatrooms", label: "Chatrooms" },
  { tab: "cockpit", label: "Cockpit" },
];
