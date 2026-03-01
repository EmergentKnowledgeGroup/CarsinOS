import type { MissionControlTab } from "./useAppController";

export interface MissionControlTabItem {
  tab: MissionControlTab;
  label: string;
  /** Lucide icon name — imported in AppShell */
  icon: string;
  /** Keyboard shortcut hint */
  shortcut: string;
}

export const MISSION_CONTROL_TABS: MissionControlTabItem[] = [
  { tab: "boards", label: "Boards", icon: "kanban", shortcut: "1" },
  { tab: "calendar", label: "Calendar", icon: "calendar", shortcut: "2" },
  { tab: "focus", label: "Focus", icon: "eye", shortcut: "3" },
  { tab: "events", label: "Events", icon: "activity", shortcut: "4" },
  { tab: "mail", label: "Mail", icon: "mail", shortcut: "5" },
  { tab: "chatrooms", label: "Rooms", icon: "messages-square", shortcut: "6" },
  { tab: "team", label: "Team", icon: "users", shortcut: "7" },
  { tab: "cockpit", label: "Cockpit", icon: "gauge", shortcut: "8" },
];
