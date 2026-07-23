import { useEffect, useCallback } from "react";
import { DEFAULT_FLOORS, resolveElevator, type FloorDef } from "../glass/floors";
import type { FloorOverride } from "../glass/floors";
import type { MissionControlTab } from "./useAppController";

interface UseKeyboardShortcutsOptions {
  availableTabs: MissionControlTab[];
  onTabChange: (tab: MissionControlTab) => void;
  /** Preferred over onTabChange for elevator jumps so the chosen room stays lit. */
  onRoomSelect?: (roomId: string) => void;
  onToggleIncidentMode: () => void;
  onToggleLiveFeed: () => void;
  onOpenCommandPalette: () => void;
  onCloseOverlay: () => void;
  /** True when a modal/overlay is open — suppresses tab shortcuts */
  overlayOpen: boolean;
  /** The same capability/override/tab-filtered registry rendered by the elevator. */
  elevatorFloors?: readonly FloorDef[];
  floorOverrides?: Partial<Record<string, FloorOverride>>;
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
    availableTabs,
    onTabChange,
    onRoomSelect,
    onToggleIncidentMode,
    onToggleLiveFeed,
    onOpenCommandPalette,
    onCloseOverlay,
    overlayOpen,
    elevatorFloors,
    floorOverrides,
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

      // Cmd+Shift+L — toggle live feed drawer
      if (meta && e.shiftKey && (e.key === "L" || e.key === "l")) {
        e.preventDefault();
        onToggleLiveFeed();
        return;
      }

      // Elevator shortcuts — only when not editing and no overlay.
      if (!overlayOpen && !isEditableTarget(e) && !meta && !e.altKey && !e.shiftKey) {
        const floors =
          elevatorFloors ??
          resolveElevator(DEFAULT_FLOORS, {
            capabilities: ["execass", "agent-mail"],
            overrides: floorOverrides,
          });
        const floor = floors.find(
          (candidate) =>
            candidate.shortcut.toLowerCase() === e.key.toLowerCase(),
        );
        const defaultRoom =
          floor?.rooms.find((room) => room.id === floor.defaultRoom) ??
          floor?.rooms[0];
        const jumpTo = (room: { id: string; route: MissionControlTab }) => {
          e.preventDefault();
          if (onRoomSelect) {
            onRoomSelect(room.id);
          } else {
            onTabChange(room.route);
          }
        };
        if (defaultRoom && availableTabs.includes(defaultRoom.route)) {
          jumpTo(defaultRoom);
          return;
        }
        const fallbackRoom = floor?.rooms.find((room) =>
          availableTabs.includes(room.route),
        );
        if (fallbackRoom) {
          jumpTo(fallbackRoom);
        }
      }
    },
    [
      availableTabs,
      onTabChange,
      onRoomSelect,
      onToggleIncidentMode,
      onToggleLiveFeed,
      onOpenCommandPalette,
      onCloseOverlay,
      overlayOpen,
      elevatorFloors,
      floorOverrides,
    ]
  );

  useEffect(() => {
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handler]);
}
