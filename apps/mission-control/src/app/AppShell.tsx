import { useState, useEffect, useCallback } from "react";
import clsx from "clsx";
import type { ReactNode } from "react";
import { MISSION_CONTROL_TABS } from "./tabs";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";
import { useTheme, THEME_FAMILIES } from "./useTheme";
import type { MissionControlTab } from "./useAppController";
import { Badge } from "../ui/Badge";
import { Chip } from "../ui/Chip";
import { CommandPalette } from "../ui/CommandPalette";
import { Modal } from "../ui/Modal";
import {
  Kanban,
  Calendar,
  Eye,
  Activity,
  Mail,
  MessagesSquare,
  Users,
  Gauge,
  Settings,
  Sun,
  Moon,
  X,
  Command,
  Minimize2,
  Maximize2,
} from "lucide-react";

const NAV_ICONS: Record<string, React.ComponentType<{ size?: number }>> = {
  kanban: Kanban,
  calendar: Calendar,
  eye: Eye,
  activity: Activity,
  mail: Mail,
  "messages-square": MessagesSquare,
  users: Users,
  gauge: Gauge,
};

interface AppShellProps {
  activeTab: MissionControlTab;
  onTabChange: (tab: MissionControlTab) => void;
  healthState: string;
  wsState: string;
  tokenConfigured: boolean;
  incidentMode: boolean;
  onIncidentModeChange: (value: boolean) => void;
  openBreakerCount: number;
  approvalsCount: number;
  jobsDue: number;
  schedulerRunning: boolean;
  gatewayDraft: string;
  onGatewayDraftChange: (value: string) => void;
  tokenDraft: string;
  onTokenDraftChange: (value: string) => void;
  onSaveConnection: () => Promise<void>;
  onReconnect: () => Promise<void>;
  onClearToken: () => Promise<void>;
  onOpenSetupWizard: () => void;
  onRefresh?: () => void;
  /** Badge counts keyed by tab id. 0 or missing = no badge. */
  navBadges?: Partial<Record<MissionControlTab, number>>;
  children: ReactNode;
}

/* ── Gateway URL history ──────────────────────────────────────────── */

const GW_HISTORY_KEY = "mc-gateway-url-history";
const GW_HISTORY_MAX = 8;

function getGatewayUrlHistory(): string[] {
  try {
    const raw = localStorage.getItem(GW_HISTORY_KEY);
    return raw ? (JSON.parse(raw) as string[]) : [];
  } catch {
    return [];
  }
}

function pushGatewayUrlHistory(url: string) {
  const trimmed = url.trim();
  if (!trimmed) return;
  const history = getGatewayUrlHistory().filter((u) => u !== trimmed);
  history.unshift(trimmed);
  localStorage.setItem(GW_HISTORY_KEY, JSON.stringify(history.slice(0, GW_HISTORY_MAX)));
}

/* ── Density persistence ───────────────────────────────────────────── */

function getDensity(): "comfortable" | "compact" {
  if (typeof window === "undefined") return "comfortable";
  return (localStorage.getItem("mc-density") as "comfortable" | "compact") || "comfortable";
}

function applyDensity(density: "comfortable" | "compact") {
  document.documentElement.setAttribute("data-density", density);
  localStorage.setItem("mc-density", density);
}

/* ── Component ─────────────────────────────────────────────────────── */

export function AppShell(props: AppShellProps) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [cmdPaletteOpen, setCmdPaletteOpen] = useState(false);
  const [clearTokenConfirmOpen, setClearTokenConfirmOpen] = useState(false);
  const [gwUrlHistory, setGwUrlHistory] = useState<string[]>(getGatewayUrlHistory);
  const [density, setDensity] = useState<"comfortable" | "compact">(getDensity);
  const theme = useTheme();

  useEffect(() => {
    applyDensity(density);
  }, [density]);

  const toggleDensity = useCallback(() => {
    setDensity((d) => (d === "comfortable" ? "compact" : "comfortable"));
  }, []);

  const { incidentMode, onIncidentModeChange } = props;
  const toggleIncidentMode = useCallback(() => {
    onIncidentModeChange(!incidentMode);
  }, [incidentMode, onIncidentModeChange]);
  const toggleCommandPalette = useCallback(() => {
    setCmdPaletteOpen((open) => !open);
  }, []);

  const handleClearToken = () => {
    setClearTokenConfirmOpen(true);
  };

  const confirmClearToken = () => {
    setClearTokenConfirmOpen(false);
    void props.onClearToken();
  };

  const closeOverlay = useCallback(() => {
    if (cmdPaletteOpen) {
      setCmdPaletteOpen(false);
    } else if (settingsOpen) {
      setSettingsOpen(false);
    }
  }, [cmdPaletteOpen, settingsOpen]);

  const handleSaveAndConnect = () => {
    pushGatewayUrlHistory(props.gatewayDraft);
    setGwUrlHistory(getGatewayUrlHistory());
    void props.onSaveConnection();
    setSettingsOpen(false);
  };

  // Keyboard shortcuts
  useKeyboardShortcuts({
    onTabChange: props.onTabChange,
    onToggleIncidentMode: toggleIncidentMode,
    onOpenCommandPalette: toggleCommandPalette,
    onCloseOverlay: closeOverlay,
    overlayOpen: settingsOpen || cmdPaletteOpen,
  });

  // Connection status dot color
  const connectionTone =
    props.wsState === "connected"
      ? "up"
      : props.wsState === "error"
        ? "down"
        : props.wsState === "idle"
          ? ""
          : "checking";

  return (
    <div className="mc-shell-layout">
      {/* ── NAV RAIL ── */}
      <nav className="mc-nav-rail">
        <div className="mc-nav-brand">MC</div>
        {MISSION_CONTROL_TABS.map((item) => {
          const Icon = NAV_ICONS[item.icon];
          const badgeCount = props.navBadges?.[item.tab] ?? 0;
          const badgeTone = item.tab === "focus" ? "danger" : "accent";
          return (
            <button
              key={item.tab}
              type="button"
              className={clsx("mc-nav-item", props.activeTab === item.tab && "mc-nav-item-active")}
              onClick={() => props.onTabChange(item.tab)}
              title={`${item.label} (${item.shortcut})`}
            >
              {Icon ? <Icon size={20} /> : null}
              <span className="mc-nav-label">{item.label}</span>
              <Badge count={badgeCount} tone={badgeTone} className="mc-nav-badge" />
            </button>
          );
        })}
        <div className="mc-nav-spacer" />
        <button
          type="button"
          className="mc-nav-item"
          onClick={() => setSettingsOpen(true)}
          title="Settings"
        >
          <Settings size={20} />
          <span className="mc-nav-label">Config</span>
        </button>
      </nav>

      {/* ── MAIN COLUMN ── */}
      <main className="mc-main-column">
        {/* ── TOPBAR ── */}
        <header className={clsx("mc-topbar", props.incidentMode && "mc-topbar-incident")}>
          <div className="mc-topbar-left">
            <h1 className="mc-topbar-title">Mission Control</h1>
          </div>
          <div className="mc-topbar-center">
            <button
              type="button"
              className="mc-cmd-trigger"
              onClick={() => setCmdPaletteOpen(true)}
            >
              <Command size={13} />
              <span>Command</span>
              <kbd className="mc-cmd-trigger-kbd">{"\u2318K"}</kbd>
            </button>
            <Chip label={`Breakers: ${props.openBreakerCount}`} tone={props.openBreakerCount > 0 ? "error" : "up"} onClick={() => props.onTabChange("focus")} />
            <Chip label={`Approvals: ${props.approvalsCount}`} tone={props.approvalsCount > 0 ? "checking" : "up"} onClick={() => props.onTabChange("focus")} />
            <Chip label={`Jobs: ${props.jobsDue}`} tone="" onClick={() => props.onTabChange("calendar")} />
            <Chip label={props.schedulerRunning ? "Sched: ON" : "Sched: OFF"} tone={props.schedulerRunning ? "up" : "down"} onClick={() => props.onTabChange("calendar")} />
          </div>
          <div className="mc-topbar-right">
            <label className="mc-incident-toggle">
              <input
                type="checkbox"
                checked={props.incidentMode}
                onChange={(e) => props.onIncidentModeChange(e.target.checked)}
              />
              <span className={clsx("mc-incident-dot", props.incidentMode && "mc-incident-active")} />
            </label>
            <button type="button" className="mc-topbar-icon-btn" onClick={toggleDensity} title={density === "comfortable" ? "Compact" : "Comfortable"}>
              {density === "comfortable" ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
            </button>
            <button type="button" className="mc-topbar-icon-btn" onClick={theme.toggleMode} title="Toggle theme">
              {theme.mode === "dark" ? <Sun size={16} /> : <Moon size={16} />}
            </button>
            <button type="button" className="mc-topbar-icon-btn" onClick={() => setSettingsOpen(true)} title="Settings">
              <Settings size={16} />
            </button>
            <span className={clsx("mc-connection-dot", `mc-connection-dot-${connectionTone}`)} title={`ws: ${props.wsState}`} />
          </div>
        </header>

        {/* ── CONTENT ── */}
        <div className="mc-content-area">
          {props.children}
        </div>
      </main>

      {/* ── COMMAND PALETTE ── */}
      <CommandPalette
        open={cmdPaletteOpen}
        onClose={() => setCmdPaletteOpen(false)}
        onTabChange={(tab) => { props.onTabChange(tab); setCmdPaletteOpen(false); }}
        onToggleIncidentMode={toggleIncidentMode}
        onRefresh={() => props.onRefresh?.()}
        onOpenSettings={() => { setSettingsOpen(true); setCmdPaletteOpen(false); }}
        currentThemeMode={theme.mode}
        onToggleThemeMode={theme.toggleMode}
        density={density}
        onToggleDensity={toggleDensity}
      />

      {/* ── SETTINGS MODAL ── */}
      {settingsOpen ? (
        <div className="mc-modal-overlay" onClick={() => setSettingsOpen(false)}>
          <div className="mc-modal mc-settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="mc-modal-header">
              <h2>Settings</h2>
              <button type="button" className="mc-topbar-icon-btn" onClick={() => setSettingsOpen(false)}>
                <X size={18} />
              </button>
            </div>
            <div className="mc-modal-body">
              {/* Connection section */}
              <div className="mc-settings-section">
                <h3 className="mc-settings-section-title">Connection</h3>
                <label className="mc-modal-field">
                  Gateway URL
                  <input
                    list="mc-gw-url-history"
                    value={props.gatewayDraft}
                    onChange={(e) => props.onGatewayDraftChange(e.target.value)}
                    placeholder="http://127.0.0.1:18789"
                  />
                  <datalist id="mc-gw-url-history">
                    {gwUrlHistory.map((url) => (
                      <option key={url} value={url} />
                    ))}
                  </datalist>
                </label>
                <label className="mc-modal-field">
                  Gateway Token
                  <input
                    value={props.tokenDraft}
                    onChange={(e) => props.onTokenDraftChange(e.target.value)}
                    placeholder={props.tokenConfigured ? "token stored in keychain" : "paste token"}
                    type="password"
                  />
                </label>
                <div className="mc-modal-status-row">
                  <Chip label={`health: ${props.healthState}`} tone={props.healthState} />
                  <Chip label={`ws: ${props.wsState}`} tone={props.wsState} />
                  <Chip label={`token: ${props.tokenConfigured ? "set" : "missing"}`} />
                </div>
                <div className="mc-modal-actions">
                  <button type="button" onClick={handleSaveAndConnect}>
                    Save + Connect
                  </button>
                  <button type="button" className="ghost" onClick={() => void props.onReconnect()}>
                    Reconnect
                  </button>
                  <button type="button" className="ghost" onClick={props.onOpenSetupWizard}>
                    Setup Wizard
                  </button>
                  <button type="button" className="danger" onClick={handleClearToken}>
                    Clear Token
                  </button>
                </div>
              </div>

              {/* Theme section */}
              <div className="mc-settings-section">
                <h3 className="mc-settings-section-title">Theme</h3>
                <div className="mc-theme-picker">
                  {THEME_FAMILIES.map((t) => (
                    <button
                      key={t.family}
                      type="button"
                      className={clsx("mc-theme-option", theme.family === t.family && "mc-theme-option-active")}
                      onClick={() => theme.selectFamily(t.family)}
                    >
                      <span className="mc-theme-option-swatch" style={{ background: t.accent }} />
                      <span className="mc-theme-option-info">
                        <span className="mc-theme-option-name">{t.label}</span>
                        <span className="mc-theme-option-desc">{t.description}</span>
                      </span>
                    </button>
                  ))}
                </div>
                <div className="mc-settings-row">
                  <span className="mc-settings-row-label">Mode</span>
                  <button type="button" className="mc-btn" onClick={theme.toggleMode}>
                    {theme.mode === "dark" ? <Sun size={14} /> : <Moon size={14} />}
                    {theme.mode === "dark" ? "Light" : "Dark"}
                  </button>
                </div>
                <div className="mc-settings-row">
                  <span className="mc-settings-row-label">Density</span>
                  <button type="button" className="mc-btn" onClick={toggleDensity}>
                    {density === "comfortable" ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
                    {density === "comfortable" ? "Compact" : "Comfortable"}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {/* ── Clear token confirmation ── */}
      <Modal
        open={clearTokenConfirmOpen}
        onClose={() => setClearTokenConfirmOpen(false)}
        title="Clear Token?"
        subtitle="This will disconnect from the gateway."
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setClearTokenConfirmOpen(false)}>
              Cancel
            </button>
            <button type="button" className="danger" onClick={confirmClearToken}>
              Clear Token
            </button>
          </>
        }
      >
        <p>This will remove the stored gateway token from keychain and disconnect the WebSocket connection. You will need to reconfigure the token to reconnect.</p>
      </Modal>
    </div>
  );
}
