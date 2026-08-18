import type { MissionControlTab } from "./useAppController";

export type TabTier = "core" | "advanced";

export interface MissionControlTabItem {
  tab: MissionControlTab;
  label: string;
  /** Lucide icon name — imported in AppShell */
  icon: string;
  /** Keyboard shortcut hint */
  shortcut: string;
  /** Whether this tab is core (daily use) or advanced (power user) */
  tier: TabTier;
}

export const MISSION_CONTROL_TABS: MissionControlTabItem[] = [
  { tab: "boards", label: "Boards", icon: "kanban", shortcut: "1", tier: "core" },
  { tab: "calendar", label: "Calendar", icon: "calendar", shortcut: "2", tier: "core" },
  { tab: "focus", label: "Focus", icon: "eye", shortcut: "3", tier: "core" },
  { tab: "mail", label: "Mail", icon: "mail", shortcut: "4", tier: "core" },
  { tab: "chatrooms", label: "Rooms", icon: "messages-square", shortcut: "5", tier: "core" },
  { tab: "assistant", label: "Assistant", icon: "bot", shortcut: "6", tier: "core" },
  { tab: "window", label: "The Window", icon: "waves", shortcut: "3", tier: "core" },
  { tab: "team", label: "Team", icon: "users", shortcut: "7", tier: "core" },
  { tab: "events", label: "Events", icon: "activity", shortcut: "8", tier: "advanced" },
  { tab: "cockpit", label: "Cockpit", icon: "gauge", shortcut: "9", tier: "advanced" },
  { tab: "strategy", label: "Strategy", icon: "compass", shortcut: "0", tier: "advanced" },
  { tab: "runbook", label: "Runbook", icon: "workflow", shortcut: "-", tier: "advanced" },
  { tab: "memory", label: "Memory", icon: "brain", shortcut: "=", tier: "advanced" },
  { tab: "connectors", label: "Connectors", icon: "cable", shortcut: "[", tier: "advanced" },
];
