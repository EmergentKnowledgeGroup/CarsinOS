import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import {
  Search,
  Kanban,
  Calendar,
  Eye,
  Activity,
  Mail,
  MessagesSquare,
  Users,
  Gauge,
  AlertTriangle,
  Sun,
  Moon,
  RefreshCw,
  Settings,
  Maximize2,
  Minimize2,
} from "lucide-react";
import { MISSION_CONTROL_TABS } from "../app/tabs";
import type { MissionControlTab } from "../app/useAppController";

const TAB_ICONS: Record<string, React.ComponentType<{ size?: number }>> = {
  kanban: Kanban,
  calendar: Calendar,
  eye: Eye,
  activity: Activity,
  mail: Mail,
  "messages-square": MessagesSquare,
  users: Users,
  gauge: Gauge,
};

export interface CommandAction {
  id: string;
  label: string;
  hint?: string;
  icon: React.ComponentType<{ size?: number }>;
  onSelect: () => void;
  section: "navigate" | "actions" | "theme";
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onTabChange: (tab: MissionControlTab) => void;
  onToggleIncidentMode: () => void;
  onRefresh: () => void;
  onOpenSettings: () => void;
  currentThemeMode: "dark" | "light";
  onToggleThemeMode: () => void;
  density: "comfortable" | "compact";
  onToggleDensity: () => void;
}

/** Simple fuzzy match: all query chars appear in order in the target */
function fuzzyMatch(query: string, target: string): boolean {
  const q = query.toLowerCase();
  const t = target.toLowerCase();
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) qi++;
  }
  return qi === q.length;
}

export function CommandPalette(props: CommandPaletteProps) {
  const {
    currentThemeMode,
    density,
    onClose,
    onOpenSettings,
    onRefresh,
    onTabChange,
    onToggleDensity,
    onToggleIncidentMode,
    onToggleThemeMode,
    open,
  } = props;
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Build action list
  const actions = useMemo<CommandAction[]>(() => {
    const navActions: CommandAction[] = MISSION_CONTROL_TABS.map((tab) => ({
      id: `nav-${tab.tab}`,
      label: `Go to ${tab.label}`,
      hint: tab.shortcut,
      icon: TAB_ICONS[tab.icon] ?? Activity,
      onSelect: () => onTabChange(tab.tab),
      section: "navigate" as const,
    }));

    const actionItems: CommandAction[] = [
      {
        id: "incident-toggle",
        label: "Toggle Incident Mode",
        hint: "\u2318\u21e7I",
        icon: AlertTriangle,
        onSelect: onToggleIncidentMode,
        section: "actions",
      },
      {
        id: "refresh",
        label: "Refresh Data",
        icon: RefreshCw,
        onSelect: onRefresh,
        section: "actions",
      },
      {
        id: "settings",
        label: "Open Settings",
        icon: Settings,
        onSelect: onOpenSettings,
        section: "actions",
      },
    ];

    const themeItems: CommandAction[] = [
      {
        id: "theme-toggle",
        label: currentThemeMode === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode",
        icon: currentThemeMode === "dark" ? Sun : Moon,
        onSelect: onToggleThemeMode,
        section: "theme",
      },
      {
        id: "density-toggle",
        label: density === "comfortable" ? "Switch to Compact Density" : "Switch to Comfortable Density",
        icon: density === "comfortable" ? Minimize2 : Maximize2,
        onSelect: onToggleDensity,
        section: "theme",
      },
    ];

    return [...navActions, ...actionItems, ...themeItems];
  }, [
    currentThemeMode,
    density,
    onOpenSettings,
    onRefresh,
    onTabChange,
    onToggleDensity,
    onToggleIncidentMode,
    onToggleThemeMode,
  ]);

  const filtered = useMemo(() => {
    if (!query.trim()) return actions;
    return actions.filter((a) => fuzzyMatch(query, a.label));
  }, [actions, query]);

  // Reset selection on filter change
  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  // Focus input on open
  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return;
    const items = listRef.current.querySelectorAll("[data-cmd-item]");
    items[selectedIndex]?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const selectAction = useCallback(
    (action: CommandAction) => {
      action.onSelect();
      onClose();
    },
    [onClose]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter" && filtered[selectedIndex]) {
        e.preventDefault();
        selectAction(filtered[selectedIndex]);
      }
    },
    [filtered, selectedIndex, selectAction]
  );

  if (!open) return null;

  // Group by section
  const sections: { key: string; label: string; items: CommandAction[] }[] = [];
  const sectionMap = new Map<string, CommandAction[]>();
  for (const action of filtered) {
    if (!sectionMap.has(action.section)) sectionMap.set(action.section, []);
    sectionMap.get(action.section)!.push(action);
  }
  const sectionLabels: Record<string, string> = {
    navigate: "Navigate",
    actions: "Actions",
    theme: "Appearance",
  };
  for (const [key, items] of sectionMap) {
    sections.push({ key, label: sectionLabels[key] ?? key, items });
  }

  let flatIndex = -1;

  return (
    <div className="mc-cmd-overlay" onClick={onClose}>
      <div className="mc-cmd-palette" onClick={(e) => e.stopPropagation()} onKeyDown={handleKeyDown}>
        <div className="mc-cmd-input-wrap">
          <Search size={16} className="mc-cmd-search-icon" />
          <input
            ref={inputRef}
            className="mc-cmd-input"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Type a command\u2026"
            spellCheck={false}
            autoComplete="off"
          />
          <kbd className="mc-cmd-esc-hint">esc</kbd>
        </div>
        <div className="mc-cmd-list" ref={listRef}>
          {sections.map((section) => (
            <div key={section.key} className="mc-cmd-section">
              <div className="mc-cmd-section-label">{section.label}</div>
              {section.items.map((action) => {
                flatIndex++;
                const idx = flatIndex;
                const Icon = action.icon;
                return (
                  <button
                    key={action.id}
                    data-cmd-item
                    type="button"
                    className={`mc-cmd-item ${idx === selectedIndex ? "mc-cmd-item-active" : ""}`}
                    onMouseEnter={() => setSelectedIndex(idx)}
                    onClick={() => selectAction(action)}
                  >
                    <Icon size={16} />
                    <span className="mc-cmd-item-label">{action.label}</span>
                    {action.hint ? <kbd className="mc-cmd-hint">{action.hint}</kbd> : null}
                  </button>
                );
              })}
            </div>
          ))}
          {filtered.length === 0 ? (
            <div className="mc-cmd-empty">No matching commands</div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
