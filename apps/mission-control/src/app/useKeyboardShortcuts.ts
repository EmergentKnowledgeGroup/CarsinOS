import { useEffect, useCallback } from "react";
import { MISSION_CONTROL_TABS } from "./tabs";
import type { MissionControlTab } from "./useAppController";

interface UseKeyboardShortcutsOptions {
  onTabChange: (tab: MissionControlTab) => void;
  onToggleIncidentMode: () => void;
  onOpenCommandPalette: () => void;
  onCloseOverlay: () => void;
  /** True when a modal/overlay is open — suppresses tab shortcuts */
  overlayOpen: boolean;
}

/** Returns true if focus is inside an editable field */
function isEditableTarget(e: KeyboardEvent): boolean {
  const tag = (e.target as HTMLElement)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if ((e.target as HTMLElement)?.isContentEditable) return true;
  return false;
}

export function useKeyboardShortcuts(opts: UseKeyboardShortcutsOptions) {
  const {
    onTabChange,
    onToggleIncidentMode,
    onOpenCommandPalette,
    onCloseOverlay,
    overlayOpen,
  } = opts;

  const handler = useCallback(
    (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;

      // Cmd+K — command palette (always active)
      if (meta && e.key === "k") {
        e.preventDefault();
        onOpenCommandPalette();
        return;
      }

      // Escape — close overlay
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCloseOverlay();
        return;
      }

      // Cmd+Shift+I — toggle incident mode
      if (meta && e.shiftKey && e.key === "I") {
        e.preventDefault();
        onToggleIncidentMode();
        return;
      }

      // Tab shortcuts 1-8 — only when not editing and no overlay
      if (!overlayOpen && !isEditableTarget(e) && !meta && !e.altKey && !e.shiftKey) {
        const idx = parseInt(e.key, 10);
        if (idx >= 1 && idx <= MISSION_CONTROL_TABS.length) {
          e.preventDefault();
          onTabChange(MISSION_CONTROL_TABS[idx - 1].tab);
        }
      }
    },
    [onTabChange, onToggleIncidentMode, onOpenCommandPalette, onCloseOverlay, overlayOpen]
  );

  useEffect(() => {
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handler]);
}
