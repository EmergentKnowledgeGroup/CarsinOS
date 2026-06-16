import { useState, useEffect, useCallback, useRef } from "react";
import clsx from "clsx";
import type { ReactNode } from "react";
import { MISSION_CONTROL_TABS } from "./tabs";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";
import { useTheme } from "./useTheme";
import type { MissionControlTab } from "./useAppController";
import { Badge } from "../ui/Badge";
import { Chip } from "../ui/Chip";
import { CommandPalette } from "../ui/CommandPalette";
import { Modal } from "../ui/Modal";
import { DEFAULT_GATEWAY_URL } from "../constants";
import { STORAGE_KEYS } from "../storageKeys";
import {
  Kanban,
  Calendar,
  Eye,
  Activity,
  Mail,
  MessagesSquare,
  Users,
  Bot,
  Gauge,
  Settings,
  BookOpen,
  Brain,
  Cable,
  Compass,
  Workflow,
  X,
  Command,
  Minimize2,
  Maximize2,
  PanelRightOpen,
  PanelRightClose,
  Lightbulb,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { NotificationCenter } from "../ui/NotificationCenter";
import { ThemeDropdown } from "../ui/ThemeDropdown";
import type { NotificationItem } from "../ui/useToasts";
import type {
  OpsUxFeatureControls,
  OpsUxRuntimeConfig,
} from "../lib/opsUxConfig";

const NAV_ICONS: Record<string, React.ComponentType<{ size?: number }>> = {
  kanban: Kanban,
  calendar: Calendar,
  eye: Eye,
  activity: Activity,
  mail: Mail,
  "messages-square": MessagesSquare,
  users: Users,
  bot: Bot,
  gauge: Gauge,
  compass: Compass,
  workflow: Workflow,
  brain: Brain,
  cable: Cable,
  "book-open": BookOpen,
};

interface AppShellProps {
  activeTab: MissionControlTab;
  availableTabs: MissionControlTab[];
  onTabChange: (tab: MissionControlTab) => void;
  healthState: string;
  wsState: string;
  tokenConfigured: boolean;
  incidentMode: boolean;
  onIncidentModeChange: (value: boolean) => void;
  openBreakerCount: number;
  approvalsCount: number;
  memoryReviewApprovalsCount?: number;
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
  onOpenHelpDocs: (section?: string) => void;
  onOpenGuidedTour: () => void;
  onRefresh?: () => void;
  notifications?: NotificationItem[];
  onDismissNotification?: (id: string) => void;
  onClearAllNotifications?: () => void;
  liveFeedEnabled: boolean;
  liveFeedOpen: boolean;
  liveFeedUnreadCount: number;
  onToggleLiveFeed: () => void;
  liveFeedPanel?: ReactNode;
  opsUxConfig: OpsUxRuntimeConfig;
  opsUxConfigError: string | null;
  onPatchOpsUxControls: (patch: Partial<OpsUxFeatureControls>) => void;
  usageChartsEnabled: boolean;
  assistantSystemPrompt: string;
  assistantSystemPromptDirty: boolean;
  assistantSystemPromptLoading: boolean;
  assistantSystemPromptSaving: boolean;
  assistantSystemPromptError: string | null;
  onAssistantSystemPromptChange: (value: string) => void;
  onSaveAssistantSystemPrompt: () => Promise<void>;
  onResetAssistantSystemPrompt: () => void;
  onRestoreDefaultAssistantSystemPrompt: () => void;
  quickGuideAvailable: boolean;
  quickGuideOpen: boolean;
  onToggleQuickGuide: () => void;
  /** Badge counts keyed by tab id. 0 or missing = no badge. */
  navBadges?: Partial<Record<MissionControlTab, number>>;
  children: ReactNode;
}

/* ── Gateway URL history ──────────────────────────────────────────── */

const GW_HISTORY_KEY = STORAGE_KEYS.gatewayUrlHistory;
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
  return (
    (localStorage.getItem(STORAGE_KEYS.density) as "comfortable" | "compact") ||
    "comfortable"
  );
}

function applyDensity(density: "comfortable" | "compact") {
  document.documentElement.setAttribute("data-density", density);
  localStorage.setItem(STORAGE_KEYS.density, density);
}

function shouldOpenAdvancedNavForHarness(): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).get("e2e") === "1";
}

/* ── Component ─────────────────────────────────────────────────────── */

export function AppShell(props: AppShellProps) {
  const activeTabIsAdvanced = MISSION_CONTROL_TABS.some(
    (item) => item.tab === props.activeTab && item.tier === "advanced"
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [cmdPaletteOpen, setCmdPaletteOpen] = useState(false);
  const [clearTokenConfirmOpen, setClearTokenConfirmOpen] = useState(false);
  const [settingsFocusTarget, setSettingsFocusTarget] = useState<"live-feed" | null>(null);
  const [advancedNavOpen, setAdvancedNavOpen] = useState(
    activeTabIsAdvanced || shouldOpenAdvancedNavForHarness()
  );
  const [settingsFeatureOpen, setSettingsFeatureOpen] = useState(false);
  const [settingsAssistantOpen, setSettingsAssistantOpen] = useState(false);
  const [gwUrlHistory, setGwUrlHistory] = useState<string[]>(getGatewayUrlHistory);
  const [density, setDensity] = useState<"comfortable" | "compact">(getDensity);
  const liveFeedSettingsInputRef = useRef<HTMLInputElement | null>(null);
  const theme = useTheme();
  const onPatchOpsUxControls = props.onPatchOpsUxControls;

  useEffect(() => {
    applyDensity(density);
  }, [density]);

  useEffect(() => {
    if (props.assistantSystemPromptDirty || props.assistantSystemPromptError) {
      const frame = window.requestAnimationFrame(() => setSettingsAssistantOpen(true));
      return () => window.cancelAnimationFrame(frame);
    }
  }, [props.assistantSystemPromptDirty, props.assistantSystemPromptError]);

  const toggleDensity = useCallback(() => {
    setDensity((d) => (d === "comfortable" ? "compact" : "comfortable"));
  }, []);

  const { incidentMode, onIncidentModeChange, onOpenGuidedTour } = props;
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

  useEffect(() => {
    if (!settingsOpen || settingsFocusTarget !== "live-feed") {
      return;
    }
    const raf = window.requestAnimationFrame(() => {
      liveFeedSettingsInputRef.current?.focus();
      liveFeedSettingsInputRef.current?.scrollIntoView({
        block: "center",
        behavior: "smooth",
      });
      setSettingsFocusTarget(null);
    });
    return () => window.cancelAnimationFrame(raf);
  }, [settingsFocusTarget, settingsOpen]);

  const closeOverlay = useCallback(() => {
    if (cmdPaletteOpen) {
      setCmdPaletteOpen(false);
    } else if (settingsOpen) {
      setSettingsOpen(false);
      setSettingsFocusTarget(null);
    }
  }, [cmdPaletteOpen, settingsOpen]);

  const handleSaveAndConnect = () => {
    pushGatewayUrlHistory(props.gatewayDraft);
    setGwUrlHistory(getGatewayUrlHistory());
    void props.onSaveConnection();
    setSettingsOpen(false);
    setSettingsFocusTarget(null);
  };

  const closeSettings = useCallback(() => {
    setSettingsOpen(false);
    setSettingsFocusTarget(null);
  }, []);

  const handleOpenGuidedTourFromSettings = useCallback(() => {
    setSettingsOpen(false);
    setSettingsFocusTarget(null);
    window.requestAnimationFrame(() => {
      onOpenGuidedTour();
    });
  }, [onOpenGuidedTour]);

  const openSettingsToLiveFeed = useCallback(() => {
    setAdvancedNavOpen(false);
    setSettingsFeatureOpen(true);
    setSettingsFocusTarget("live-feed");
    setSettingsOpen(true);
  }, []);

  const patchOpsControl = useCallback(
    (key: keyof OpsUxFeatureControls, value: boolean) => {
      if (
        key !== "global_kill_switch" &&
        value &&
        props.opsUxConfig.controls.global_kill_switch
      ) {
        onPatchOpsUxControls({
          global_kill_switch: false,
          [key]: value,
        });
        return;
      }
      onPatchOpsUxControls({
        [key]: value,
      });
    },
    [onPatchOpsUxControls, props.opsUxConfig.controls.global_kill_switch]
  );

  // Keyboard shortcuts
  useKeyboardShortcuts({
    availableTabs: props.availableTabs,
    onTabChange: props.onTabChange,
    onToggleIncidentMode: toggleIncidentMode,
    onToggleLiveFeed: props.liveFeedEnabled
      ? props.onToggleLiveFeed
      : () => setSettingsOpen(true),
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
  const liveFeedToggleTitle = props.liveFeedEnabled
    ? props.liveFeedOpen
      ? "Hide live feed"
      : "Show live feed"
    : "Live Feed is off. Click to open Settings and turn it on.";
  const quickGuideToggleTitle = props.quickGuideOpen
    ? "Hide quick guides"
    : "Show quick guide for this page";
  const optionalFeaturesMasterOn = !props.opsUxConfig.controls.global_kill_switch;
  const gatewayHealthLabel =
    props.healthState === "healthy"
      ? "Gateway: Healthy"
      : props.healthState === "degraded"
        ? "Gateway: Needs attention"
        : `Gateway: ${props.healthState}`;
  const liveLinkLabel =
    props.wsState === "connected"
      ? "Live link: Connected"
      : props.wsState === "connecting"
        ? "Live link: Connecting"
        : props.wsState === "idle"
          ? "Live link: Waiting"
          : `Live link: ${props.wsState}`;
  const tokenLabel = props.tokenConfigured ? "Token: Configured" : "Token: Missing";
  const rolloutState = (enabled: boolean) => {
    if (!optionalFeaturesMasterOn && enabled) {
      return {
        label: "Waiting on main switch",
        tone: "warning",
      };
    }
    return enabled
      ? {
          label: "On",
          tone: "connected",
        }
      : {
          label: "Off",
          tone: "",
        };
  };
  const liveFeedStatus = rolloutState(props.opsUxConfig.controls.live_feed_drawer);
  const incidentAutoStatus = rolloutState(props.opsUxConfig.controls.incident_auto_trigger);
  const strategyStatus = rolloutState(props.opsUxConfig.controls.strategy_hub);
  const runbookStatus = rolloutState(props.opsUxConfig.controls.runbook_hub);
  const memoryStatus = rolloutState(props.opsUxConfig.controls.memory_hub);
  const connectorsStatus = rolloutState(props.opsUxConfig.controls.connectors_hub);
  const usageChartsStatus = props.usageChartsEnabled
    ? { label: "On", tone: "connected" }
    : optionalFeaturesMasterOn && props.opsUxConfig.controls.usage_charts
      ? { label: "Waiting on data", tone: "warning" }
      : { label: "Off", tone: "" };
  const mainSwitchStatus = optionalFeaturesMasterOn
    ? { label: "On", tone: "connected" }
    : { label: "Off", tone: "" };

  return (
    <div className="mc-shell-layout">
      {/* ── NAV RAIL ── */}
      <nav className="mc-nav-rail">
        <div className="mc-nav-brand">MC</div>
        {(() => {
          const visible = MISSION_CONTROL_TABS.filter((item) =>
            props.availableTabs.includes(item.tab)
          );
          const coreTabs = visible.filter((item) => item.tier === "core");
          const advancedTabs = visible.filter((item) => item.tier === "advanced");
          const renderTab = (item: (typeof MISSION_CONTROL_TABS)[number]) => {
            const Icon = NAV_ICONS[item.icon];
            const badgeCount = props.navBadges?.[item.tab] ?? 0;
            const badgeTone = item.tab === "focus" ? "danger" : "accent";
            return (
              <button
                key={item.tab}
                type="button"
                className={clsx("mc-nav-item", props.activeTab === item.tab && "mc-nav-item-active")}
                onClick={() => {
                  props.onTabChange(item.tab);
                  setAdvancedNavOpen(false);
                }}
                title={`${item.label} (${item.shortcut})`}
                data-tour-id={`nav-${item.tab}`}
              >
                {Icon ? <Icon size={20} /> : null}
                <span className="mc-nav-label">{item.label}</span>
                <Badge count={badgeCount} tone={badgeTone} className="mc-nav-badge" />
              </button>
            );
          };
          return (
            <>
              {coreTabs.map(renderTab)}
              {advancedTabs.length > 0 ? (
                <>
                  <button
                    type="button"
                    className={clsx(
                      "mc-nav-item",
                      "mc-nav-advanced-toggle",
                      activeTabIsAdvanced && "mc-nav-item-active",
                      advancedNavOpen && "mc-nav-advanced-toggle-open"
                    )}
                    onClick={() => setAdvancedNavOpen((open) => !open)}
                    title={
                      advancedNavOpen
                        ? "Hide advanced tools"
                        : "Show advanced tools: events, cockpit, strategy, runbook, memory, connectors"
                    }
                    aria-expanded={advancedNavOpen}
                    aria-controls="mc-nav-advanced-group"
                    data-tour-id="nav-advanced"
                  >
                    {advancedNavOpen ? <ChevronDown size={20} /> : <ChevronRight size={20} />}
                    <span className="mc-nav-label">Tools</span>
                    <span className="mc-nav-count" aria-label={`${advancedTabs.length} advanced pages`}>
                      {advancedTabs.length}
                    </span>
                  </button>
                  <div
                    id="mc-nav-advanced-group"
                    className={clsx(
                      "mc-nav-advanced-group",
                      advancedNavOpen && "mc-nav-advanced-group-open"
                    )}
                    hidden={!advancedNavOpen}
                  >
                    {advancedTabs.map(renderTab)}
                  </div>
                </>
              ) : null}
            </>
          );
        })()}
        <div className="mc-nav-spacer" />
        <button
          type="button"
          className="mc-nav-item"
          onClick={() => {
            setAdvancedNavOpen(false);
            props.onOpenHelpDocs();
          }}
          title="Help and Docs"
          data-tour-id="nav-help-shortcut"
        >
          <BookOpen size={20} />
          <span className="mc-nav-label">Help/Docs</span>
        </button>
        <button
          type="button"
          className="mc-nav-item"
          onClick={() => {
            setAdvancedNavOpen(false);
            setSettingsOpen(true);
          }}
          title="Settings"
          data-tour-id="nav-config"
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
              data-tour-id="topbar-command"
            >
              <Command size={13} />
              <span>Command</span>
              <kbd className="mc-cmd-trigger-kbd">{"\u2318K"}</kbd>
            </button>
            <Chip label={`Breakers: ${props.openBreakerCount}`} tone={props.openBreakerCount > 0 ? "error" : "up"} onClick={() => props.onTabChange("focus")} />
            <Chip label={`Approvals: ${props.approvalsCount}`} tone={props.approvalsCount > 0 ? "checking" : "up"} onClick={() => props.onTabChange("focus")} />
            {(props.memoryReviewApprovalsCount ?? 0) > 0 ? (
              <Chip
                label={`Memory review: ${props.memoryReviewApprovalsCount}`}
                tone="warning"
                onClick={() => props.onTabChange("focus")}
              />
            ) : null}
            <Chip label={`Jobs: ${props.jobsDue}`} tone="" onClick={() => props.onTabChange("calendar")} />
            <Chip label={props.schedulerRunning ? "Sched: ON" : "Sched: OFF"} tone={props.schedulerRunning ? "up" : "warning"} onClick={() => props.onTabChange("calendar")} />
          </div>
          <div className="mc-topbar-right">
            <label className="mc-incident-toggle">
              <input
                type="checkbox"
                checked={props.incidentMode}
                onChange={(e) => props.onIncidentModeChange(e.target.checked)}
                aria-label="Toggle incident mode"
              />
              <span className={clsx("mc-incident-dot", props.incidentMode && "mc-incident-active")} />
            </label>
            <button
              type="button"
              className={clsx(
                "mc-topbar-icon-btn",
                "mc-live-feed-toggle",
                props.liveFeedOpen && "mc-live-feed-toggle-active",
                !props.liveFeedEnabled && "mc-live-feed-toggle-unavailable"
              )}
              data-testid="live-feed-toggle"
              aria-label={liveFeedToggleTitle}
              aria-pressed={props.liveFeedOpen}
              onClick={
                props.liveFeedEnabled
                  ? props.onToggleLiveFeed
                  : openSettingsToLiveFeed
              }
              title={liveFeedToggleTitle}
            >
              {props.liveFeedOpen ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
              {props.liveFeedUnreadCount > 0 ? (
                <span className="mc-live-feed-toggle-badge">{props.liveFeedUnreadCount}</span>
              ) : null}
            </button>
            <NotificationCenter
              notifications={props.notifications ?? []}
              onDismiss={props.onDismissNotification ?? (() => {})}
              onClearAll={props.onClearAllNotifications ?? (() => {})}
            />
            <button type="button" className="mc-topbar-icon-btn" onClick={toggleDensity} title={density === "comfortable" ? "Compact" : "Comfortable"}>
              {density === "comfortable" ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
            </button>
            {props.quickGuideAvailable ? (
              <button
                type="button"
                className={clsx(
                  "mc-topbar-icon-btn",
                  props.quickGuideOpen && "mc-topbar-icon-btn-active"
                )}
                onClick={props.onToggleQuickGuide}
                title={quickGuideToggleTitle}
                aria-label={quickGuideToggleTitle}
                aria-pressed={props.quickGuideOpen}
              >
                <Lightbulb size={16} />
              </button>
            ) : null}
            <button
              type="button"
              className="mc-topbar-icon-btn"
              onClick={() => {
                setAdvancedNavOpen(false);
                props.onOpenGuidedTour();
              }}
              title="Start guided tour"
              data-tour-id="topbar-tour"
            >
              <Compass size={16} />
            </button>
            <button
              type="button"
              className="mc-topbar-icon-btn"
              onClick={() => {
                setAdvancedNavOpen(false);
                props.onOpenHelpDocs();
              }}
              title="Help and docs"
            >
              <BookOpen size={16} />
            </button>
            <ThemeDropdown
              family={theme.family}
              mode={theme.mode}
              selectFamily={theme.selectFamily}
              setMode={theme.setMode}
              toggleMode={theme.toggleMode}
            />
            <button
              type="button"
              className="mc-topbar-icon-btn"
              onClick={() => {
                setAdvancedNavOpen(false);
                setSettingsOpen(true);
              }}
              title="Settings"
            >
              <Settings size={16} />
            </button>
            <span className={clsx("mc-connection-dot", `mc-connection-dot-${connectionTone}`)} title={`ws: ${props.wsState}`} aria-label={`Connection status: ${props.wsState}`} role="status" />
          </div>
        </header>

        {/* ── CONTENT ── */}
        <div className="mc-workspace">
          <div className="mc-content-area">
            {props.children}
          </div>
          {props.liveFeedEnabled ? props.liveFeedPanel : null}
        </div>
      </main>

      {/* ── COMMAND PALETTE ── */}
      <CommandPalette
        availableTabs={props.availableTabs}
        open={cmdPaletteOpen}
        onClose={() => setCmdPaletteOpen(false)}
        onTabChange={(tab) => { props.onTabChange(tab); setCmdPaletteOpen(false); }}
        onToggleIncidentMode={toggleIncidentMode}
        onRefresh={() => props.onRefresh?.()}
        onOpenSettings={() => {
          setAdvancedNavOpen(false);
          setSettingsOpen(true);
          setCmdPaletteOpen(false);
        }}
        currentThemeMode={theme.mode}
        onToggleThemeMode={theme.toggleMode}
        density={density}
        onToggleDensity={toggleDensity}
      />

      {/* ── SETTINGS MODAL ── */}
      {settingsOpen ? (
        <div className="mc-modal-overlay mc-settings-overlay" onClick={closeSettings}>
          <div className="mc-modal mc-settings-modal" onClick={(e) => e.stopPropagation()}>
            <div className="mc-modal-header">
              <h2>Settings</h2>
              <button type="button" className="mc-topbar-icon-btn" onClick={closeSettings}>
                <X size={18} />
              </button>
            </div>
            <div className="mc-modal-body mc-settings-body">
              {/* Connection section */}
              <details className="mc-settings-section mc-settings-disclosure" open>
                <summary className="mc-settings-summary">
                  <span>
                    <strong>1. Connect this app</strong>
                    <small>Gateway address, token, and reconnect buttons.</small>
                  </span>
                  <span className="mc-settings-summary-status">
                    <Chip label={gatewayHealthLabel} tone={props.healthState} />
                    <Chip label={tokenLabel} tone={props.tokenConfigured ? "connected" : "warning"} />
                  </span>
                </summary>
                <div className="mc-settings-disclosure-body">
                  <p className="mc-settings-help">
                    Tell Mission Control where carsinOS is running. Most people only touch this once.
                  </p>
                  <label className="mc-modal-field">
                    Gateway URL
                    <input
                      list="mc-gw-url-history"
                      value={props.gatewayDraft}
                      onChange={(e) => props.onGatewayDraftChange(e.target.value)}
                      placeholder={DEFAULT_GATEWAY_URL}
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
                      placeholder={props.tokenConfigured ? "token configured" : "paste token"}
                      type="password"
                    />
                  </label>
                  <div className="mc-modal-status-row">
                    <Chip label={liveLinkLabel} tone={props.wsState} />
                  </div>
                  <p className="mc-settings-help">
                    Desktop stores the gateway token in the OS keychain. Browser runs keep it in memory, with
                    session-only storage reserved for the explicit E2E harness.
                  </p>
                  <div className="mc-modal-actions">
                    <button type="button" onClick={handleSaveAndConnect}>
                      Save and connect
                    </button>
                    <button type="button" className="ghost" onClick={() => void props.onReconnect()}>
                      Try reconnect
                    </button>
                    <button type="button" className="ghost" onClick={props.onOpenSetupWizard}>
                      Open setup wizard
                    </button>
                    <button type="button" className="ghost" onClick={handleOpenGuidedTourFromSettings}>
                      Start guided tour
                    </button>
                    <button type="button" className="danger" onClick={handleClearToken}>
                      Forget token
                    </button>
                  </div>
                </div>
              </details>

              {/* Reliability / feature controls */}
              <details
                className="mc-settings-section mc-settings-disclosure"
                open={settingsFeatureOpen}
                onToggle={(event) => setSettingsFeatureOpen(event.currentTarget.open)}
              >
                <summary className="mc-settings-summary">
                  <span>
                    <strong>2. Choose what pages show</strong>
                    <small>Keep daily use simple. Turn on expert pages only when needed.</small>
                  </span>
                  <span className="mc-settings-summary-status">
                    <Chip label={mainSwitchStatus.label} tone={mainSwitchStatus.tone} />
                  </span>
                </summary>
                <div className="mc-settings-disclosure-body">
                  <p className="mc-settings-help">
                    These switches control optional tools. The everyday path is Boards, Calendar,
                    Focus, Mail, Rooms, Assistant, and Team.
                  </p>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={!props.opsUxConfig.controls.global_kill_switch}
                        onChange={(event) =>
                          patchOpsControl("global_kill_switch", !event.target.checked)
                        }
                      />
                      <span>Allow optional pages and tools</span>
                    </label>
                    <Chip label={mainSwitchStatus.label} tone={mainSwitchStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        ref={liveFeedSettingsInputRef}
                        type="checkbox"
                        autoFocus={settingsFocusTarget === "live-feed"}
                        checked={props.opsUxConfig.controls.live_feed_drawer}
                        onChange={(event) =>
                          patchOpsControl("live_feed_drawer", event.target.checked)
                        }
                      />
                      <span>Live Feed panel</span>
                    </label>
                    <Chip label={liveFeedStatus.label} tone={liveFeedStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={props.opsUxConfig.controls.incident_auto_trigger}
                        onChange={(event) =>
                          patchOpsControl("incident_auto_trigger", event.target.checked)
                        }
                      />
                      <span>Auto-switch to incident mode</span>
                    </label>
                    <Chip label={incidentAutoStatus.label} tone={incidentAutoStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={props.opsUxConfig.controls.usage_charts}
                        onChange={(event) =>
                          patchOpsControl("usage_charts", event.target.checked)
                        }
                      />
                      <span>Usage charts</span>
                    </label>
                    <Chip label={usageChartsStatus.label} tone={usageChartsStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={props.opsUxConfig.controls.strategy_hub}
                        onChange={(event) =>
                          patchOpsControl("strategy_hub", event.target.checked)
                        }
                      />
                      <span>Strategy page</span>
                    </label>
                    <Chip label={strategyStatus.label} tone={strategyStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={props.opsUxConfig.controls.runbook_hub}
                        onChange={(event) =>
                          patchOpsControl("runbook_hub", event.target.checked)
                        }
                      />
                      <span>Runbook page</span>
                    </label>
                    <Chip label={runbookStatus.label} tone={runbookStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={props.opsUxConfig.controls.memory_hub}
                        onChange={(event) =>
                          patchOpsControl("memory_hub", event.target.checked)
                        }
                      />
                      <span>Memory page</span>
                    </label>
                    <Chip label={memoryStatus.label} tone={memoryStatus.tone} />
                  </div>
                  <div className="mc-settings-toggle-row">
                    <label className="mc-settings-toggle">
                      <input
                        type="checkbox"
                        checked={props.opsUxConfig.controls.connectors_hub}
                        onChange={(event) =>
                          patchOpsControl("connectors_hub", event.target.checked)
                        }
                      />
                      <span>Connectors page</span>
                    </label>
                    <Chip label={connectorsStatus.label} tone={connectorsStatus.tone} />
                  </div>
                  <p className="mc-settings-help">
                    Yellow means the page is ready but the main switch is still off, or the page is
                    waiting for data to arrive.
                  </p>
                  {props.opsUxConfigError ? (
                    <p className="mc-settings-inline-error">{props.opsUxConfigError}</p>
                  ) : null}
                </div>
              </details>

              {/* Theme section */}
              <details
                className="mc-settings-section mc-settings-disclosure"
                open={settingsAssistantOpen}
                onToggle={(event) => setSettingsAssistantOpen(event.currentTarget.open)}
              >
                <summary className="mc-settings-summary">
                  <span>
                    <strong>3. Shared assistant instructions</strong>
                    <small>Default behavior for new Assistant, Telegram, and Discord runs.</small>
                  </span>
                  <span className="mc-settings-summary-status">
                    <Chip
                      label={props.assistantSystemPromptDirty ? "Unsaved" : "Saved"}
                      tone={props.assistantSystemPromptDirty ? "warning" : "connected"}
                    />
                  </span>
                </summary>
                <div className="mc-settings-disclosure-body">
                  <p className="mc-settings-help">
                    Edit this only when you want every new carsinOS conversation to follow the same
                    standing rules.
                  </p>
                  <textarea
                    className="mc-settings-prompt"
                    value={props.assistantSystemPrompt}
                    onChange={(event) => props.onAssistantSystemPromptChange(event.target.value)}
                    rows={8}
                    placeholder="Describe how carsinOS should behave, what it should prioritize, and any standing constraints."
                  />
                  <div className="mc-settings-actions">
                    <button
                      type="button"
                      className="mc-btn"
                      onClick={() => void props.onSaveAssistantSystemPrompt()}
                      disabled={
                        props.assistantSystemPromptLoading ||
                        props.assistantSystemPromptSaving ||
                        !props.assistantSystemPromptDirty
                      }
                    >
                      {props.assistantSystemPromptSaving
                        ? "Saving prompt..."
                        : "Save shared prompt"}
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={props.onResetAssistantSystemPrompt}
                      disabled={
                        props.assistantSystemPromptLoading ||
                        props.assistantSystemPromptSaving ||
                        !props.assistantSystemPromptDirty
                      }
                    >
                      Reset changes
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={props.onRestoreDefaultAssistantSystemPrompt}
                      disabled={
                        props.assistantSystemPromptLoading || props.assistantSystemPromptSaving
                      }
                    >
                      Use built-in default
                    </button>
                  </div>
                  {props.assistantSystemPromptError ? (
                    <p className="mc-settings-inline-error">{props.assistantSystemPromptError}</p>
                  ) : null}
                  <p className="mc-settings-help">
                    {props.assistantSystemPromptDirty
                      ? "You have unsaved prompt changes."
                      : "Shared prompt saved. Use Insert Core Prompt in Assistant if you want the current chat to pick up the latest version immediately."}
                  </p>
                </div>
              </details>
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
        <p>This will remove the configured gateway token from secure runtime storage and disconnect the WebSocket connection. You will need to reconfigure the token to reconnect.</p>
      </Modal>
    </div>
  );
}
