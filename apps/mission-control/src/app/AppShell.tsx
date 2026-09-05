import { useState, useEffect, useCallback, useRef } from "react";
import clsx from "clsx";
import type { ReactNode } from "react";
import { useDialogFocus } from "./useDialogFocus";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";
import { useTheme } from "./useTheme";
import type { MissionControlTab } from "./useAppController";
import {
  DEFAULT_FLOORS,
  resolveElevator,
  type FloorDef,
} from "../glass/floors";
import { useGlassSurfaceTheme } from "../glass/useGlassSurfaceTheme";
import {
  GLASS_CONFIG_EVENT,
  loadGlassConfig,
  notifyGlassConfigChanged,
  saveGlassConfig,
  resolveActiveTheme,
} from "../glass/config";
import type { IncidentPosture } from "../glass/incidentPosture";
import { activeThemeName } from "../glass/themeEditor";
import { Badge } from "../ui/Badge";
import { Chip } from "../ui/Chip";
import { CommandPalette } from "../ui/CommandPalette";
import {
  ConnectionControls,
  FeatureControls,
} from "../features/setup/SetupControls";
import { connectionStatusPresentation } from "../features/setup/setupPresentation";
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
  Waves,
  X,
  Command,
  Minimize2,
  Maximize2,
  PanelRightOpen,
  PanelRightClose,
  Lightbulb,
  Palette,
} from "lucide-react";
import { NotificationCenter } from "../ui/NotificationCenter";

import { ThemeStudio } from "../features/glassTheme/ThemeStudio";
import type { NotificationItem } from "../ui/useToasts";
import type {
  OpsUxFeatureControls,
  OpsUxRuntimeConfig,
} from "../lib/opsUxConfig";
import "./glassShell.css";

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
  waves: Waves,
  brain: Brain,
  cable: Cable,
  "book-open": BookOpen,
};

interface AppShellProps {
  activeTab: MissionControlTab;
  availableTabs: MissionControlTab[];
  elevatorFloors?: readonly FloorDef[];
  onTabChange: (tab: MissionControlTab) => void;
  /** Stable room id owning the elevator lamp; null when no room owns the tab. */
  activeRoomId: string | null;
  onRoomSelect: (roomId: string) => void;
  healthState: string;
  wsState: string;
  tokenConfigured: boolean;
  incidentMode: boolean;
  onIncidentModeChange: (value: boolean) => void;
  /**
   * Composed calm/unknown/incident posture from the one pure composer.
   * Quiet (unknown) when absent so bare mounts never claim health.
   */
  incidentPosture?: IncidentPosture;
  /** True when the incident target room exists in the resolved elevator. */
  incidentWalkAvailable?: boolean;
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
  /** In-flow on narrow layouts so alerts never cover shell controls. */
  toastPanel?: ReactNode;
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

/* ── Component ─────────────────────────────────────────────────────── */

export function AppShell(props: AppShellProps) {
  const elevatorFloors =
    props.elevatorFloors ??
    resolveElevator(DEFAULT_FLOORS, {
      capabilities: ["execass", "agent-mail"],
      overrides: loadGlassConfig().floorOverrides,
    })
      .map((floor) => ({
        ...floor,
        rooms: floor.rooms.filter((room) =>
          props.availableTabs.includes(room.route),
        ),
      }))
      .filter((floor) => floor.rooms.length > 0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [themeStudioOpen, setThemeStudioOpen] = useState(false);
  const [cmdPaletteOpen, setCmdPaletteOpen] = useState(false);
  const [settingsFocusTarget, setSettingsFocusTarget] = useState<"live-feed" | null>(null);
  const [settingsFeatureOpen, setSettingsFeatureOpen] = useState(false);
  const [settingsAssistantOpen, setSettingsAssistantOpen] = useState(false);
  const [density, setDensity] = useState<"comfortable" | "compact">(getDensity);
  const [glassThemeName, setGlassThemeName] = useState(() =>
    activeThemeName(loadGlassConfig()),
  );
  const glassSurfaceRef = useRef<HTMLDivElement | null>(null);
  const liveFeedSettingsInputRef = useRef<HTMLInputElement | null>(null);
  const theme = useTheme();
  useGlassSurfaceTheme(glassSurfaceRef);
  const setGlobalMode = theme.setMode;
  useEffect(() => {
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    const syncMode = () => setGlobalMode(resolveActiveTheme(loadGlassConfig(), { prefersDark: media?.matches ?? false }).mode);
    syncMode();
    window.addEventListener(GLASS_CONFIG_EVENT, syncMode);
    media?.addEventListener?.("change", syncMode);
    return () => {
      window.removeEventListener(GLASS_CONFIG_EVENT, syncMode);
      media?.removeEventListener?.("change", syncMode);
    };
  }, [setGlobalMode]);

  useEffect(() => {
    applyDensity(density);
  }, [density]);

  useEffect(() => {
    const update = () => setGlassThemeName(activeThemeName(loadGlassConfig()));
    window.addEventListener(GLASS_CONFIG_EVENT, update);
    return () => window.removeEventListener(GLASS_CONFIG_EVENT, update);
  }, []);

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
  }, [setCmdPaletteOpen]);
  const toggleAfterHours = useCallback(() => {
    const config = loadGlassConfig();
    const current = resolveActiveTheme(config, { prefersDark: window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false });
    const themeId = current.mode === "dark" ? "porcelain-light" : "carbon-dark";
    const saved = saveGlassConfig({ ...config, themeId });
    if (saved.ok) notifyGlassConfigChanged();
  }, []);

  const settingsDialogRef = useRef<HTMLDivElement | null>(null);
  useDialogFocus(settingsOpen, settingsDialogRef);

  useEffect(() => {
    if (!settingsOpen || settingsFocusTarget !== "live-feed") {
      return;
    }
    const raf = window.requestAnimationFrame(() => {
      liveFeedSettingsInputRef.current?.focus();
      liveFeedSettingsInputRef.current?.scrollIntoView({
        block: "center",
        behavior: window.matchMedia?.("(prefers-reduced-motion: reduce)")
          .matches
          ? "auto"
          : "smooth",
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
  }, [
    cmdPaletteOpen,
    settingsOpen,
    setCmdPaletteOpen,
    setSettingsFocusTarget,
    setSettingsOpen,
  ]);

  const closeSettings = useCallback(() => {
    setSettingsOpen(false);
    setSettingsFocusTarget(null);
  }, [setSettingsFocusTarget, setSettingsOpen]);

  const handleOpenGuidedTourFromSettings = useCallback(() => {
    setSettingsOpen(false);
    setSettingsFocusTarget(null);
    window.requestAnimationFrame(() => {
      onOpenGuidedTour();
    });
  }, [onOpenGuidedTour, setSettingsFocusTarget, setSettingsOpen]);

  const openSettingsToLiveFeed = useCallback(() => {
    setSettingsFeatureOpen(true);
    setSettingsFocusTarget("live-feed");
    setSettingsOpen(true);
  }, [setSettingsFeatureOpen, setSettingsFocusTarget, setSettingsOpen]);

  // Keyboard shortcuts
  useKeyboardShortcuts({
    availableTabs: props.availableTabs,
    onTabChange: props.onTabChange,
    onRoomSelect: props.onRoomSelect,
    onToggleIncidentMode: toggleIncidentMode,
    onToggleLiveFeed: props.liveFeedEnabled
      ? props.onToggleLiveFeed
      : () => setSettingsOpen(true),
    onOpenCommandPalette: toggleCommandPalette,
    onCloseOverlay: closeOverlay,
    overlayOpen: settingsOpen || cmdPaletteOpen,
    elevatorFloors,
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
  const { gatewayHealthLabel, gatewayHealthTone, tokenLabel } = connectionStatusPresentation({
    healthState: props.healthState,
    wsState: props.wsState,
    tokenConfigured: props.tokenConfigured,
  });
  const mainSwitchStatus = optionalFeaturesMasterOn
    ? { label: "On", tone: "connected" }
    : { label: "Off", tone: "" };
  const incidentPosture: IncidentPosture = props.incidentPosture ?? {
    posture: "unknown",
  };
  const postureWord =
    incidentPosture.posture === "incident"
      ? "Incident"
      : incidentPosture.posture === "calm"
        ? "Calm"
        : "Checking";
  const postureTitle =
    incidentPosture.posture === "incident"
      ? incidentPosture.message
      : incidentPosture.posture === "calm"
        ? "System posture: calm. No authoritative trouble."
        : "System posture is still loading.";
  const activeFloor = elevatorFloors.find((floor) =>
    floor.rooms.some((room) => room.id === props.activeRoomId),
  );
  const activeRoom = activeFloor?.rooms.find(
    (room) => room.id === props.activeRoomId,
  );
  const claimedTourRoutes = new Set<MissionControlTab>();

  return (
    <div ref={glassSurfaceRef} className="mc-shell-layout mc-glass-shell">
      {/* ── NAV RAIL ── */}
      <nav className="mc-nav-rail mc-glass-elevator-rail" aria-label="CarsinOS floors">
        <div className="mc-nav-brand mc-glass-brand">
          <svg width="36" height="36" viewBox="0 0 40 40" aria-hidden="true">
            <path
              d="M36 11 A17 17 0 1 0 36 29 L30 26 A11 11 0 1 1 30 14 Z M36 11 L26 15 l3.4 2.6 L25 20 l4.4 2.4 L26 25 l10 4 A17 17 0 0 0 36 11 Z"
              fill="currentColor"
              fillRule="evenodd"
              transform="rotate(-14 20 20)"
            />
            <path d="M14 9 l5 4 l-4 3 l5 4" fill="none" stroke="var(--ground)" strokeWidth="1.7" transform="rotate(-14 20 20)" />
            <path d="M20 16.5 l7 3.5 l-7 3.5" fill="none" stroke="var(--ink)" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <span>
            <span className="mc-glass-wordmark">Carsin<b>OS</b></span>
            <small>mission control</small>
          </span>
        </div>
        <p className="mc-glass-elevator-label">Elevator / rooms</p>
        <div className="mc-elevator" aria-label="Glass Office elevator">
          {elevatorFloors.map((floor) => {
            const FloorIcon = NAV_ICONS[floor.icon];
            const activeFloor = floor.rooms.some(
              (room) => room.id === props.activeRoomId,
            );
            return (
              <section
                key={floor.id}
                className={clsx("mc-elevator-floor", activeFloor && "is-active")}
                data-tour-id={`floor-${floor.id}`}
              >
                <div className="mc-elevator-floor-label">
                  <span className="mc-elevator-lamp">{floor.lamp}</span>
                  {FloorIcon ? <FloorIcon size={15} /> : null}
                  <strong>{floor.label}</strong>
                </div>
                <div className="mc-elevator-rooms">
                  {floor.rooms.map((room) => {
                    const badgeCount = props.navBadges?.[room.route] ?? 0;
                    const roomMark = room.label
                      .split(/\s+/)
                      .flatMap((word) => word.match(/[a-z0-9]/i)?.[0] ?? [])
                      .join("")
                      .slice(0, 2)
                      .toUpperCase();
                    const isPrimaryTourTarget = !claimedTourRoutes.has(
                      room.route,
                    );
                    claimedTourRoutes.add(room.route);
                    return (
                      <button
                        key={`${floor.id}:${room.id}`}
                        type="button"
                        className={clsx(
                          "mc-nav-item",
                          props.activeRoomId === room.id && "mc-nav-item-active",
                        )}
                        onClick={() => {
                          props.onRoomSelect(room.id);
                        }}
                        title={`${floor.lamp}F · ${room.label}`}
                        aria-label={`${floor.lamp}F · ${room.label}`}
                        aria-current={
                          props.activeRoomId === room.id ? "page" : undefined
                        }
                        data-tour-id={
                          isPrimaryTourTarget
                            ? `nav-${room.route}`
                            : `nav-${floor.id}-${room.id}`
                        }
                      >
                        <span className="mc-nav-room-mark" aria-hidden="true">
                          {roomMark}
                        </span>
                        <span className="mc-nav-label">{room.label}</span>
                        <Badge
                          count={badgeCount}
                          tone={room.route === "focus" ? "danger" : "accent"}
                          className="mc-nav-badge"
                        />
                      </button>
                    );
                  })}
                </div>
              </section>
            );
          })}
        </div>
        <div className="mc-nav-spacer" />
        <button
          type="button"
          className="mc-nav-item"
          onClick={() => {
            props.onOpenHelpDocs();
          }}
          title="Help and Docs"
          aria-label="Help and Docs"
          data-tour-id="nav-help-shortcut"
        >
          <BookOpen size={20} />
          <span className="mc-nav-label">Help/Docs</span>
        </button>
        <button
          type="button"
          className="mc-nav-item"
          onClick={() => {
            setSettingsOpen(true);
          }}
          title="Settings"
          aria-label="Settings"
          data-tour-id="nav-config"
        >
          <Settings size={20} />
          <span className="mc-nav-label">Config</span>
        </button>
      </nav>

      {/* ── MAIN COLUMN ── */}
      <main className="mc-main-column">
        {/* ── TOPBAR ── */}
        <header className={clsx("mc-topbar mc-glass-topbar", props.incidentMode && "mc-topbar-incident")}>
          <div className="mc-topbar-left">
            <div className="mc-glass-floor-context" aria-hidden="true">
              {activeFloor?.lamp ?? "·"}
            </div>
            <div>
              <p className="mc-glass-topbar-kicker">{activeFloor?.label ?? "Mission Control"}</p>
              <h1 className="mc-topbar-title">{activeRoom?.label ?? "Mission Control"}</h1>
            </div>
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
            <span
              className={clsx(
                "mc-posture-status",
                `is-${incidentPosture.posture}`,
              )}
              data-testid="incident-posture-status"
              title={postureTitle}
              role="status"
              aria-label={`System posture: ${postureWord}`}
            >
              {postureWord}
            </span>
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
            <button type="button" className="mc-topbar-icon-btn" onClick={toggleDensity} title={density === "comfortable" ? "Compact" : "Comfortable"} aria-label={density === "comfortable" ? "Switch to compact density" : "Switch to comfortable density"}>
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
                props.onOpenGuidedTour();
              }}
              title="Start guided tour"
              aria-label="Start guided tour"
              data-tour-id="topbar-tour"
            >
              <Compass size={16} />
            </button>
            <button
              type="button"
              className="mc-topbar-icon-btn"
              onClick={() => {
                props.onOpenHelpDocs();
              }}
              title="Help and docs"
              aria-label="Help and docs"
            >
              <BookOpen size={16} />
            </button>
            <button type="button" className="mc-topbar-icon-btn" aria-label="Theme" title="Theme studio" onClick={() => { setThemeStudioOpen(true); setSettingsOpen(true); }}>
              <Palette size={16} />
            </button>
            <button
              type="button"
              className="mc-glass-after-hours"
              onClick={toggleAfterHours}
              title="Toggle Glass Office after-hours theme"
              aria-label="Toggle Glass Office after-hours theme"
            >
              After hours
            </button>
            <button
              type="button"
              className="mc-topbar-icon-btn"
              onClick={() => {
                setSettingsOpen(true);
              }}
              title="Settings"
              aria-label="Settings"
            >
              <Settings size={16} />
            </button>
            <span className={clsx("mc-connection-dot", `mc-connection-dot-${connectionTone}`)} title={`ws: ${props.wsState}`} aria-label={`Connection status: ${props.wsState}`} role="status" />
          </div>
        </header>
        {props.toastPanel}

        {/* ── INCIDENT BAND ── */}
        {incidentPosture.posture === "incident" ? (
          <div
            className="mc-incident-band"
            role="status"
            aria-live="polite"
            data-testid="incident-band"
          >
            <span className="mc-incident-band-beacon" aria-hidden="true" />
            <span className="mc-incident-band-label">Incident</span>
            <div className="mc-incident-band-copy">
              <p className="mc-incident-band-message">
                {incidentPosture.message}
              </p>
              {incidentPosture.detail ? (
                <p className="mc-incident-band-detail">
                  {incidentPosture.detail}
                </p>
              ) : null}
            </div>
            {props.incidentWalkAvailable ? (
              <button
                type="button"
                className="mc-incident-band-walk"
                data-testid="incident-band-walk"
                onClick={() => {
                  props.onRoomSelect(incidentPosture.target.roomId);
                }}
              >
                Go to {incidentPosture.target.label}
              </button>
            ) : (
              <span className="mc-incident-band-unroutable">
                {incidentPosture.target.label} is not available right now.
              </span>
            )}
          </div>
        ) : null}

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
          setSettingsOpen(true);
          setCmdPaletteOpen(false);
        }}
        currentThemeMode={theme.mode}
        onToggleThemeMode={toggleAfterHours}
        density={density}
        onToggleDensity={toggleDensity}
      />

      {/* ── SETTINGS MODAL ── */}
      {settingsOpen ? (
        <div className="mc-modal-overlay mc-settings-overlay" onClick={closeSettings}>
          <div
            ref={settingsDialogRef}
            className="mc-modal mc-settings-modal"
            role="dialog"
            aria-modal="true"
            aria-label="Settings"
            tabIndex={-1}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="mc-modal-header">
              <h2>Settings</h2>
              <button
                type="button"
                className="mc-topbar-icon-btn"
                aria-label="Close settings"
                onClick={closeSettings}
              >
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
                    <Chip label={gatewayHealthLabel} tone={gatewayHealthTone} />
                    <Chip label={tokenLabel} tone={props.tokenConfigured ? "connected" : "warning"} />
                  </span>
                </summary>
                <div className="mc-settings-disclosure-body">
                  <ConnectionControls
                    idPrefix="settings"
                    gatewayDraft={props.gatewayDraft}
                    onGatewayDraftChange={props.onGatewayDraftChange}
                    tokenDraft={props.tokenDraft}
                    onTokenDraftChange={props.onTokenDraftChange}
                    tokenConfigured={props.tokenConfigured}
                    healthState={props.healthState}
                    wsState={props.wsState}
                    onSaveConnection={props.onSaveConnection}
                    onReconnect={props.onReconnect}
                    onClearToken={props.onClearToken}
                    onOpenSetupWizard={props.onOpenSetupWizard}
                    onOpenGuidedTour={handleOpenGuidedTourFromSettings}
                    onSaveInitiated={closeSettings}
                  />
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
                  <FeatureControls
                    opsUxConfig={props.opsUxConfig}
                    opsUxConfigError={props.opsUxConfigError}
                    onPatchOpsUxControls={props.onPatchOpsUxControls}
                    usageChartsEnabled={props.usageChartsEnabled}
                    liveFeedInputRef={liveFeedSettingsInputRef}
                    liveFeedAutoFocus={settingsFocusTarget === "live-feed"}
                  />
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

              {/* Glass theme studio */}
              <details className="mc-settings-section mc-settings-disclosure" open={themeStudioOpen} onToggle={(event) => setThemeStudioOpen(event.currentTarget.open)}>
                <summary className="mc-settings-summary">
                  <span>
                    <strong>4. Theme studio</strong>
                    <small>Pick, build, and share Glass Office themes.</small>
                  </span>
                  <span className="mc-settings-summary-status">
                    <Chip label={glassThemeName} tone="connected" />
                  </span>
                </summary>
                <div className="mc-settings-disclosure-body">
                  <p className="mc-settings-help">
                    Themes are token bags applied to the Office surface. Claw
                    Orange and the safety colors stay fixed in every theme.
                  </p>
                  <ThemeStudio />
                </div>
              </details>
            </div>
          </div>
        </div>
      ) : null}

    </div>
  );
}
