import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AppContent } from "./app/AppContent";
import { AppShell } from "./app/AppShell";
import { GuidedTourOverlay, type GuidedTourStep } from "./app/GuidedTourOverlay";
import { LiveFeedDrawer } from "./app/LiveFeedDrawer";
import type { HelpTab } from "./app/TabHelpBanner";
import {
  useAppController,
  type EventStreamItem,
  type MissionControlTab,
} from "./app/useAppController";
import { useGatewayEvents } from "./app/useGatewayEvents";
import { useLiveFeedController } from "./app/useLiveFeedController";
import { useMissionControlController } from "./app/useMissionControlController";
import { useResolvedElevator } from "./app/useResolvedElevator";
import {
  useRuntimeConnectionController,
  type BoardSummary,
} from "./app/useRuntimeConnectionController";
import { useAgentMailController } from "./features/agentMail/useAgentMailController";
import { useAssistantChatController } from "./features/assistant/useAssistantChatController";
import {
  DEFAULT_ASSISTANT_CORE_PROMPT,
  normalizeAssistantCorePrompt,
  resolveAssistantCorePrompt,
} from "./features/assistant/corePrompt";
import { useBoardsController } from "./features/boards/useBoardsController";
import { useCockpitController } from "./features/cockpit/useCockpitController";
import { SimpleIntegrationWizard } from "./features/connectors/SimpleIntegrationWizard";
import type { SimpleIntegrationId } from "./features/connectors/simpleIntegrations";
import { useConnectorsController } from "./features/connectors/useConnectorsController";
import { useExecassOfficeController } from "./features/execassOffice/useExecassOfficeController";
import { useGlassWindowController } from "./features/glassWindow/useGlassWindowController";
import { roomForTab } from "./glass/floors";
import { useMemoryController } from "./features/memory/useMemoryController";
import { OnboardingWizard } from "./features/onboarding/OnboardingWizard";
import { useOnboardingController } from "./features/onboarding/useOnboardingController";
import { useRunbookController } from "./features/runbook/useRunbookController";
import { useStrategyController } from "./features/strategy/useStrategyController";
import { SafeModePanel } from "./ui/SafeModePanel";
import { ToastStack } from "./ui/Toast";
import { useToasts } from "./ui/useToasts";
import type { Agent, RuntimeGlobalConfigResponse, WsEventFrame } from "./types";
import { EVENT_STREAM_BUFFER_CAP, WS_MAX_RECONNECT_ATTEMPTS } from "./constants";
import { getRuntimeConfig, updateRuntimeConfig } from "./lib/api";
import {
  cancelDesktopRuntimeCloseConfirmation,
  confirmDesktopRuntimeClose,
  isTauriRuntime,
  type RuntimeCloseConfirmation,
} from "./lib/runtime";
import { filterVisibleEvents } from "./lib/eventStream";
import {
  countRecentHighSeverityEvents,
  hasCriticalEventWithinWindow,
} from "./lib/liveFeed";
import {
  loadOpsUxRuntimeConfig,
  saveOpsUxRuntimeConfig,
  withOpsUxControlPatch,
  type OpsUxFeatureControls,
} from "./lib/opsUxConfig";
import { STORAGE_KEYS } from "./storageKeys";
import "./styles.css";

export function RuntimeCloseDialog(props: {
  confirmation: RuntimeCloseConfirmation;
  confirming: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { confirmation, confirming, onConfirm, onCancel } = props;
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancelButtonRef.current?.focus();
  }, [confirmation.binding.challenge]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !confirming) {
        event.preventDefault();
        onCancel();
        return;
      }
      if (event.key === "Tab") {
        const focusable = Array.from(
          dialogRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [confirming, onCancel]);

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="runtime-close-title"
      aria-describedby="runtime-close-consequence runtime-close-cancel-note"
      style={{ position: "fixed", inset: 0, zIndex: 10000, display: "grid", placeItems: "center", padding: "1rem", background: "rgba(0, 0, 0, 0.6)" }}
    >
      <section style={{ maxWidth: "32rem", padding: "1.25rem", borderRadius: "0.75rem", background: "var(--mc-surface, #171717)" }}>
        <h2 id="runtime-close-title">Stop app-bound runtime?</h2>
        <p id="runtime-close-consequence">{confirmation.consequence}</p>
        <p id="runtime-close-cancel-note">The UI stays open if you cancel.</p>
        <div style={{ display: "flex", gap: "0.75rem", justifyContent: "flex-end" }}>
          <button ref={cancelButtonRef} type="button" className="ghost" disabled={confirming} onClick={onCancel}>Keep running</button>
          <button type="button" className="danger" disabled={confirming} onClick={onConfirm}>
            {confirming ? "Stopping..." : "Pause work and close"}
          </button>
        </div>
      </section>
    </div>
  );
}

interface GuidedTourStepDef extends GuidedTourStep {
  tab?: ReturnType<typeof useAppController>["activeTab"];
}

const GUIDED_TOUR_STEPS: GuidedTourStepDef[] = [
  {
    id: "boards",
    tab: "boards",
    targetId: "nav-boards",
    title: "Boards = task execution",
    body: "Create cards, attach context, then click Run Card to execute model work.",
  },
  {
    id: "calendar",
    tab: "calendar",
    targetId: "nav-calendar",
    title: "Calendar = scheduling control",
    body: "Use Calendar for run-now, pause/resume jobs, and recurring automation timing.",
  },
  {
    id: "focus",
    tab: "focus",
    targetId: "nav-focus",
    title: "Focus = incident triage",
    body: "Approvals, breakers, and urgent operational items are surfaced here first.",
  },
  {
    id: "events",
    tab: "events",
    targetId: "nav-events",
    title: "Events = runtime activity",
    body: "Watch the live event stream here when you need to verify what the system is doing right now.",
  },
  {
    id: "mail",
    tab: "mail",
    targetId: "nav-mail",
    title: "Mail = direct thread messaging",
    body: "Mail supports structured threads, attachments, and acknowledgement flow.",
  },
  {
    id: "rooms",
    tab: "chatrooms",
    targetId: "nav-chatrooms",
    title: "Rooms = group coordination",
    body: "Rooms are multi-party collaboration channels with shared context and handoffs.",
  },
  {
    id: "assistant",
    tab: "assistant",
    targetId: "nav-assistant",
    title: "Assistant = direct chat",
    body: "Use this for direct prompt/response execution with selected agent, model, and system prompt.",
  },
  {
    id: "team",
    tab: "team",
    targetId: "nav-team",
    title: "Team = agent roster",
    body: "Configure each agent's provider/model and tool profile so execution has ownership.",
  },
  {
    id: "cockpit",
    tab: "cockpit",
    targetId: "nav-cockpit",
    title: "Cockpit = custom ops dashboard",
    body: "Build operation views with widgets for approvals, jobs, channels, and runtime health.",
  },
  {
    id: "strategy",
    tab: "strategy",
    targetId: "nav-strategy",
    title: "Strategy = management layer",
    body: "Track goals, projects, blocked work, stale work, ownership, and approval-linked tasks here.",
  },
  {
    id: "runbook",
    tab: "runbook",
    targetId: "nav-runbook",
    title: "Runbook = execution truth map",
    body: "Use Runbook to inspect the canonical flow, linked artifacts, active step, and next valid step for real work already in the system.",
  },
  {
    id: "memory",
    tab: "memory",
    targetId: "nav-memory",
    title: "Memory = assistant memory truth",
    body: "Use Memory to inspect one assistant-bound MNO lane at a time: cards, episodes, graph drilldown, turn why, citations, and runtime health.",
  },
  {
    id: "connectors",
    tab: "connectors",
    targetId: "nav-connectors",
    title: "Connectors = shared tool registry",
    body: "Use Connectors to import sources, convert them into reviewable tools, publish the safe subset, and assign that same connector surface to every agent that should see it.",
  },
  {
    id: "help",
    tab: "help",
    targetId: "nav-help-shortcut",
    title: "Help/Docs = in-app knowledge base",
    body: "This section explains each tab with examples and links back into live workflows.",
  },
  {
    id: "config",
    targetId: "nav-config",
    title: "Config = connection + recovery controls",
    body: "Open Config to reconnect the gateway, re-run setup, launch this tour again, and control rollout switches.",
  },
  {
    id: "command",
    targetId: "topbar-command",
    title: "Command palette",
    body: "Use Cmd/Ctrl + K for fast navigation and actions without hunting through tabs.",
  },
];

export default function App() {
  const {
    activeTab,
    setActiveTab,
    activeRoomId,
    selectRoom,
    settings,
    setSettings,
    gatewayDraft,
    setGatewayDraft,
    tokenDraft,
    setTokenDraft,
    tokenConfigured,
    setTokenConfigured,
    healthState,
    setHealthState,
    wsState,
    setWsState,
    eventStream,
    setEventStream,
    showRawEvents,
    setShowRawEvents,
  } = useAppController();

  /* Toast system — adapts legacy setNotice({tone,message}) calls to toast stack */
  const { toasts, addToast, dismissToast, notifications, dismissNotification, clearAllNotifications } = useToasts();
  const setNotice = useCallback(
    (n: { tone: "info" | "error" | "critical"; message: string } | null) => {
      if (n) addToast(n.message, n.tone);
    },
    [addToast],
  );

  const [boards, setBoards] = useState<BoardSummary[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [tokenConfiguredChecked, setTokenConfiguredChecked] = useState(false);
  const [guidedTourOpen, setGuidedTourOpen] = useState(false);
  const [guidedTourStep, setGuidedTourStep] = useState(0);
  const [safeModeReason, setSafeModeReason] = useState<string | null>(null);
  const [runtimeCloseConfirmation, setRuntimeCloseConfirmation] =
    useState<RuntimeCloseConfirmation | null>(null);
  const [runtimeCloseConfirming, setRuntimeCloseConfirming] = useState(false);
  const [runtimeGlobalConfig, setRuntimeGlobalConfig] =
    useState<RuntimeGlobalConfigResponse | null>(null);
  const [assistantSystemPromptSaved, setAssistantSystemPromptSaved] = useState(
    DEFAULT_ASSISTANT_CORE_PROMPT
  );
  const [assistantSystemPromptDraft, setAssistantSystemPromptDraft] = useState(
    DEFAULT_ASSISTANT_CORE_PROMPT
  );
  const [assistantSystemPromptLoading, setAssistantSystemPromptLoading] = useState(false);
  const [assistantSystemPromptSaving, setAssistantSystemPromptSaving] = useState(false);
  const [assistantSystemPromptError, setAssistantSystemPromptError] =
    useState<string | null>(null);
  const [simpleIntegrationWizardState, setSimpleIntegrationWizardState] = useState<{
    open: boolean;
    initialIntegrationId: SimpleIntegrationId | null;
  }>({
    open: false,
    initialIntegrationId: null,
  });
  const [initialBootstrapSettledKey, setInitialBootstrapSettledKey] = useState<string | null>(null);
  const [tabResetVersion, setTabResetVersion] = useState<Partial<Record<MissionControlTab, number>>>({});
  const [quickGuideState, setQuickGuideState] = useState<{
    collapsed: boolean;
    openTab: HelpTab | null;
  }>({
    collapsed: false,
    openTab: null,
  });
  const [opsUxRuntime, setOpsUxRuntime] = useState(() => loadOpsUxRuntimeConfig());
  const [incidentAutoSuppressedUntilMs, setIncidentAutoSuppressedUntilMs] = useState(0);
  const [incidentAutoTickMs, setIncidentAutoTickMs] = useState(() => Date.now());
  const lastAutoBaselineKeyRef = useRef<string | null>(null);
  const manualIncidentOverrideRef = useRef<"on" | "off" | null>(null);
  const wsDegradedSinceRef = useRef<number | null>(null);
  const healthySinceRef = useRef<number>(0);
  const previousIncidentModeRef = useRef(false);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }
    let disposed = false;
    let unlistenConfirmation: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unlistenRecovery: (() => void) | undefined;
    void Promise.all([
      listen<RuntimeCloseConfirmation>("runtime-close-confirmation-required", (event) => {
        if (!disposed) {
          setRuntimeCloseConfirming(false);
          setRuntimeCloseConfirmation(event.payload);
        }
      }),
      listen<{ message?: string }>("runtime-close-error", (event) => {
        if (!disposed) {
          setRuntimeCloseConfirming(false);
          setNotice({ tone: "critical", message: event.payload?.message || "CarsinOS kept the app open because runtime close could not be verified." });
        }
      }),
      listen<{ message?: string }>("runtime-close-recovery-required", (event) => {
        if (!disposed) {
          setRuntimeCloseConfirming(false);
          setRuntimeCloseConfirmation(null);
          setNotice({ tone: "critical", message: event.payload?.message || "Runtime shutdown needs recovery attention before closing." });
        }
      }),
    ]).then(([confirmation, error, recovery]) => {
      if (disposed) {
        confirmation();
        error();
        recovery();
      } else {
        unlistenConfirmation = confirmation;
        unlistenError = error;
        unlistenRecovery = recovery;
      }
    });
    return () => {
      disposed = true;
      unlistenConfirmation?.();
      unlistenError?.();
      unlistenRecovery?.();
    };
  }, [setNotice]);

  const confirmRuntimeClose = useCallback(() => {
    if (!runtimeCloseConfirmation || runtimeCloseConfirming) {
      return;
    }
    setRuntimeCloseConfirming(true);
    void confirmDesktopRuntimeClose(runtimeCloseConfirmation.binding).catch((error: unknown) => {
      setRuntimeCloseConfirming(false);
      setNotice({ tone: "critical", message: `CarsinOS could not confirm runtime close: ${String(error)}` });
    });
  }, [runtimeCloseConfirmation, runtimeCloseConfirming, setNotice]);

  const cancelRuntimeClose = useCallback(() => {
    setRuntimeCloseConfirmation(null);
    setRuntimeCloseConfirming(false);
    void cancelDesktopRuntimeCloseConfirmation().catch((error: unknown) => {
      setNotice({ tone: "error", message: `CarsinOS could not cancel runtime close: ${String(error)}` });
    });
  }, [setNotice]);

  const opsConfig = opsUxRuntime.config;
  const startupBaselineKey = useMemo(() => {
    const gatewayUrl = settings.gateway_url.trim();
    if (!tokenConfigured || !gatewayUrl) {
      return null;
    }
    return `${gatewayUrl}::token-ready`;
  }, [settings.gateway_url, tokenConfigured]);
  const initialBootstrapSettled =
    tokenConfiguredChecked &&
    (startupBaselineKey === null || initialBootstrapSettledKey === startupBaselineKey);
  const optionalModulesEnabled = !opsConfig.controls.global_kill_switch;
  const liveFeedEnabled = optionalModulesEnabled && opsConfig.controls.live_feed_drawer;
  const incidentAutoEnabled =
    optionalModulesEnabled && opsConfig.controls.incident_auto_trigger;
  const usageChartsEnabled = optionalModulesEnabled && opsConfig.controls.usage_charts;
  const strategyHubEnabled = optionalModulesEnabled && opsConfig.controls.strategy_hub;
  const runbookHubEnabled = optionalModulesEnabled && opsConfig.controls.runbook_hub;
  const memoryHubEnabled = optionalModulesEnabled && opsConfig.controls.memory_hub;
  const connectorsHubEnabled =
    optionalModulesEnabled && opsConfig.controls.connectors_hub;
  const assistantSystemPromptDirty = useMemo(
    () =>
      normalizeAssistantCorePrompt(assistantSystemPromptDraft) !==
      normalizeAssistantCorePrompt(assistantSystemPromptSaved),
    [assistantSystemPromptDraft, assistantSystemPromptSaved]
  );
  const availableTabs = useMemo<MissionControlTab[]>(
    () =>
      [
        "boards",
        "calendar",
        "focus",
        "events",
        "mail",
        "chatrooms",
        "assistant",
        "window",
        "team",
        "cockpit",
        "strategy",
        ...(runbookHubEnabled ? (["runbook"] as MissionControlTab[]) : []),
        ...(memoryHubEnabled ? (["memory"] as MissionControlTab[]) : []),
        ...(connectorsHubEnabled ? (["connectors"] as MissionControlTab[]) : []),
      ],
    [connectorsHubEnabled, memoryHubEnabled, runbookHubEnabled]
  );
  const elevatorFloors = useResolvedElevator(availableTabs);
  const selectAvailableRoom = useCallback(
    (roomId: string) => {
      selectRoom(roomId, elevatorFloors);
    },
    [elevatorFloors, selectRoom],
  );
  const resolvedActiveRoomId =
    roomForTab(elevatorFloors, activeTab, activeRoomId ?? undefined)?.room.id ??
    null;
  const dismissQuickGuides = useCallback(() => {
    setQuickGuideState({
      collapsed: true,
      openTab: null,
    });
  }, []);
  const toggleQuickGuideForActiveTab = useCallback(() => {
    if (activeTab === "help") {
      return;
    }
    setQuickGuideState((current) => {
      const activeGuideTab = activeTab as HelpTab;
      const currentTabOpen = !current.collapsed || current.openTab === activeGuideTab;
      if (currentTabOpen) {
        return {
          collapsed: true,
          openTab: null,
        };
      }
      return {
        collapsed: true,
        openTab: activeGuideTab,
      };
    });
  }, [activeTab]);
  const quickGuideVisibleOnActiveTab =
    activeTab !== "help" &&
    (!quickGuideState.collapsed || quickGuideState.openTab === activeTab);
  const guidedTourSteps = useMemo(
    () =>
      GUIDED_TOUR_STEPS.filter((step) => {
        if (step.id === "runbook" && !runbookHubEnabled) {
          return false;
        }
        if (step.id === "memory" && !memoryHubEnabled) {
          return false;
        }
        if (step.id === "connectors" && !connectorsHubEnabled) {
          return false;
        }
        return true;
      }),
    [connectorsHubEnabled, memoryHubEnabled, runbookHubEnabled]
  );

  const patchOpsControls = useCallback(
    (patch: Partial<OpsUxFeatureControls>) => {
      // Side effects stay out of the state updater: updaters run during
      // render, and the store emit would synchronously update other
      // subscribed components mid-render. The persisted store is the
      // source of truth, so patch against a fresh load.
      const nextConfig = withOpsUxControlPatch(
        loadOpsUxRuntimeConfig().config,
        patch,
      );
      const persisted = saveOpsUxRuntimeConfig(nextConfig);
      if (!persisted.ok) {
        setNotice({
          tone: "error",
          message: persisted.error ?? "Runtime config persistence failed.",
        });
      }
      setOpsUxRuntime({
        config: nextConfig,
        degraded: !persisted.ok,
        error: persisted.error,
      });
    },
    [setNotice]
  );

  const applyRuntimeGlobalConfig = useCallback((global: RuntimeGlobalConfigResponse | null) => {
    setRuntimeGlobalConfig(global);
    if (!global) {
      return;
    }
    const resolved = resolveAssistantCorePrompt(global?.assistant_system_prompt);
    setAssistantSystemPromptSaved(resolved);
    setAssistantSystemPromptDraft(resolved);
  }, []);

  const loadAssistantSystemPromptConfig = useCallback(
    async (runtimeSettings = settings) => {
      if (!tokenConfigured || !runtimeSettings.gateway_url.trim()) {
        applyRuntimeGlobalConfig(null);
        setAssistantSystemPromptError(null);
        return;
      }

      setAssistantSystemPromptLoading(true);
      try {
        const response = await getRuntimeConfig(runtimeSettings);
        applyRuntimeGlobalConfig(response.config.global);
        setAssistantSystemPromptError(null);
      } catch (error: unknown) {
        applyRuntimeGlobalConfig(null);
        setAssistantSystemPromptError(
          `Shared prompt settings could not load. carsinOS is using the built-in default for now. (${String(error)})`
        );
      } finally {
        setAssistantSystemPromptLoading(false);
      }
    },
    [applyRuntimeGlobalConfig, settings, tokenConfigured]
  );

  const saveAssistantSystemPrompt = useCallback(async () => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      setNotice({
        tone: "error",
        message: "Connect to the gateway before saving the shared assistant prompt.",
      });
      return;
    }

    setAssistantSystemPromptSaving(true);
    try {
      const baseGlobal =
        runtimeGlobalConfig ?? (await getRuntimeConfig(settings)).config.global;
      const response = await updateRuntimeConfig(settings, {
        global: {
          ...baseGlobal,
          assistant_system_prompt: normalizeAssistantCorePrompt(assistantSystemPromptDraft),
        },
      });
      applyRuntimeGlobalConfig(response.config.global);
      setAssistantSystemPromptError(null);
      setNotice({ tone: "info", message: "Shared assistant prompt saved." });
    } catch (error: unknown) {
      const message = `Saving the shared assistant prompt failed: ${String(error)}`;
      setAssistantSystemPromptError(message);
      setNotice({ tone: "error", message });
    } finally {
      setAssistantSystemPromptSaving(false);
    }
  }, [
    applyRuntimeGlobalConfig,
    assistantSystemPromptDraft,
    runtimeGlobalConfig,
    setNotice,
    settings,
    tokenConfigured,
  ]);

  const resetAssistantSystemPromptDraft = useCallback(() => {
    setAssistantSystemPromptDraft(assistantSystemPromptSaved);
  }, [assistantSystemPromptSaved]);

  const restoreDefaultAssistantSystemPromptDraft = useCallback(() => {
    setAssistantSystemPromptDraft(DEFAULT_ASSISTANT_CORE_PROMPT);
  }, []);

  const boardsController = useBoardsController({
    settings,
    setNotice,
  });

  const cockpitController = useCockpitController();

  const mailController = useAgentMailController({
    settings,
    tokenConfigured,
    setNotice,
  });
  const missionControl = useMissionControlController({
    settings,
    agents,
    incidentMode: cockpitController.incidentMode,
    setNotice,
  });
  const assistantController = useAssistantChatController({
    settings,
    tokenConfigured,
    agents,
    authProfiles: missionControl.authProfiles,
    boards,
    setNotice,
    corePrompt: assistantSystemPromptDraft,
    corePromptSaved: assistantSystemPromptSaved,
    corePromptLoading: assistantSystemPromptLoading,
    corePromptSaving: assistantSystemPromptSaving,
    corePromptError: assistantSystemPromptError,
    corePromptDirty: assistantSystemPromptDirty,
    setCorePrompt: setAssistantSystemPromptDraft,
    saveCorePrompt: saveAssistantSystemPrompt,
    resetCorePrompt: resetAssistantSystemPromptDraft,
    restoreDefaultCorePrompt: restoreDefaultAssistantSystemPromptDraft,
  });
  const strategyController = useStrategyController({
    settings,
    agents,
    enabled: strategyHubEnabled,
    setNotice,
  });
  const runbookController = useRunbookController({
    settings,
    agents,
    enabled: runbookHubEnabled,
    setNotice,
  });
  const memoryController = useMemoryController({
    settings,
    agents,
    enabled: memoryHubEnabled,
    preferredAgentId: assistantController.selectedAgentId,
    setNotice,
  });
  const connectorsController = useConnectorsController({
    settings,
    agents,
    enabled: connectorsHubEnabled,
    setNotice,
  });

  const officeController = useExecassOfficeController({
    settings,
    tokenConfigured,
    active: activeTab === "assistant",
    setNotice,
  });
  const glassWindowController = useGlassWindowController({
    settings,
    tokenConfigured,
    active: activeTab === "window",
  });

  const liveFeed = useLiveFeedController({
    retentionWindowMs: opsConfig.safety.recovery_retention_window_ms,
    recoveryMaxBytes: opsConfig.safety.recovery_log_max_bytes,
    markReadUndoWindowMs: opsConfig.safety.mark_read_undo_window_ms,
  });
  const liveFeedEvents = liveFeed.events;
  const liveFeedSeverityFilter = liveFeed.severityFilter;
  const setLiveFeedSeverityFilter = liveFeed.setSeverityFilter;
  const ingestLiveFeedFrame = liveFeed.ingestWsFrame;
  const queueMissionControlRefresh = missionControl.queueMissionControlRefresh;
  const applyGatewayBoardEvent = boardsController.applyGatewayBoardEvent;
  const queueAgentMailRefresh = mailController.queueAgentMailRefresh;

  const { loadBaseline, saveConnection, saveConnectionFromInputs, clearToken, reconnect } =
    useRuntimeConnectionController({
    settings,
    gatewayDraft,
    tokenDraft,
    setSettings,
    setGatewayDraft,
    setTokenDraft,
    setTokenConfigured,
    setTokenConfiguredChecked,
    setHealthState,
    setWsState,
    setNotice,
    setBoards,
    setAgents,
    activeBoardId: boardsController.activeBoardId,
    setActiveBoardId: boardsController.setActiveBoardId,
    refreshBoard: boardsController.refreshBoard,
    setBoard: boardsController.setBoard,
    loadMissionControlReadModels: missionControl.loadMissionControlReadModels,
    loadRunbookReadModels: runbookController.loadRunbookData,
    loadAgentMailReadModels: mailController.loadAgentMailReadModels,
  });

  const onboarding = useOnboardingController({
    settings,
    tokenConfigured,
    initialBootstrapSettled,
    agents,
    authProfiles: missionControl.authProfiles,
    strategyEnabled: strategyHubEnabled,
    bootstrapPresets: strategyController.presets,
    saveConnectionFromInputs,
    loadBaseline,
    setActiveTab,
  });

  const setIncidentModeFromOperator = useCallback(
    (next: boolean) => {
      const now = Date.now();
      if (next) {
        manualIncidentOverrideRef.current = "on";
        setIncidentAutoSuppressedUntilMs(0);
        healthySinceRef.current = now;
      } else {
        manualIncidentOverrideRef.current = "off";
        setIncidentAutoSuppressedUntilMs(
          now + opsConfig.safety.incident_auto_cooldown_ms
        );
        healthySinceRef.current = now;
      }
      cockpitController.setIncidentMode(next);
    },
    [cockpitController, opsConfig.safety.incident_auto_cooldown_ms]
  );

  const setIncidentModeAutomatically = useCallback(
    (next: boolean, reason: string) => {
      if (cockpitController.incidentMode === next) {
        return;
      }
      if (next) {
        manualIncidentOverrideRef.current = null;
        healthySinceRef.current = Date.now();
        addToast(`Incident mode auto-enabled: ${reason}.`, "critical");
      } else {
        addToast(`Incident mode auto-disabled: ${reason}.`, "info");
        healthySinceRef.current = Date.now();
      }
      cockpitController.setIncidentMode(next);
    },
    [addToast, cockpitController]
  );

  const [helpDocsTarget, setHelpDocsTarget] = useState<{ section?: string; seq: number }>({ seq: 0 });

  const openHelpDocs = useCallback(
    (section?: string) => {
      setHelpDocsTarget((prev) => ({ section, seq: prev.seq + 1 }));
      setActiveTab("help");
    },
    [setActiveTab]
  );

  const openSimpleIntegrationWizard = useCallback(
    (integrationId?: SimpleIntegrationId) => {
      setSimpleIntegrationWizardState({
        open: true,
        initialIntegrationId: integrationId ?? null,
      });
    },
    []
  );

  const closeSimpleIntegrationWizard = useCallback(() => {
    setSimpleIntegrationWizardState((current) => ({
      ...current,
      open: false,
    }));
  }, []);

  const openGuidedTour = useCallback(() => {
    setGuidedTourStep(0);
    setGuidedTourOpen(true);
  }, []);

  const closeGuidedTour = useCallback(() => {
    setGuidedTourOpen(false);
    try {
      localStorage.setItem(STORAGE_KEYS.guidedTourCompletedV1, "true");
    } catch {
      // no-op in constrained environments
    }
  }, []);

  useEffect(() => {
    if (onboarding.isOpen) {
      return;
    }
    let completed = false;
    try {
      completed = localStorage.getItem(STORAGE_KEYS.guidedTourCompletedV1) === "true";
    } catch {
      completed = false;
    }
    if (!completed) {
      const timer = window.setTimeout(() => {
        setGuidedTourOpen(true);
      }, 0);
      return () => window.clearTimeout(timer);
    }
  }, [onboarding.isOpen]);

  useEffect(() => {
    if (activeTab === "runbook" && !runbookHubEnabled) {
      setActiveTab("boards");
      setNotice({
        tone: "info",
        message: "Runbook was turned off, so you were moved back to Boards.",
      });
    }
  }, [activeTab, runbookHubEnabled, setActiveTab, setNotice]);

  useEffect(() => {
    if (activeTab === "memory" && !memoryHubEnabled) {
      setActiveTab("boards");
      setNotice({
        tone: "info",
        message: "Memory was turned off, so you were moved back to Boards.",
      });
    }
  }, [activeTab, memoryHubEnabled, setActiveTab, setNotice]);

  useEffect(() => {
    if (activeTab === "connectors" && !connectorsHubEnabled) {
      setActiveTab("boards");
      setNotice({
        tone: "info",
        message: "Connectors was turned off, so you were moved back to Boards.",
      });
    }
  }, [activeTab, connectorsHubEnabled, setActiveTab, setNotice]);

  useEffect(() => {
    if (!initialBootstrapSettled) {
      return;
    }
    void loadAssistantSystemPromptConfig(settings);
  }, [initialBootstrapSettled, loadAssistantSystemPromptConfig, settings]);

  useEffect(() => {
    if (!guidedTourOpen) {
      return;
    }
    const step = guidedTourSteps[guidedTourStep];
    if (step?.tab) {
      setActiveTab(step.tab);
    }
  }, [guidedTourOpen, guidedTourStep, guidedTourSteps, setActiveTab]);

  const visibleEvents = useMemo(
    () => filterVisibleEvents(eventStream, showRawEvents),
    [eventStream, showRawEvents]
  );

  useEffect(() => {
    const wasIncident = previousIncidentModeRef.current;
    if (!wasIncident && cockpitController.incidentMode) {
      setLiveFeedSeverityFilter("critical_high");
    } else if (
      wasIncident &&
      !cockpitController.incidentMode &&
      liveFeedSeverityFilter === "critical_high"
    ) {
      setLiveFeedSeverityFilter("all");
    }
    previousIncidentModeRef.current = cockpitController.incidentMode;
  }, [
    cockpitController.incidentMode,
    liveFeedSeverityFilter,
    setLiveFeedSeverityFilter,
  ]);

  useEffect(() => {
    if (!cockpitController.incidentMode || !incidentAutoEnabled || !liveFeedEnabled) {
      return;
    }
    const timer = window.setInterval(() => {
      setIncidentAutoTickMs(Date.now());
    }, 1_000);
    return () => {
      window.clearInterval(timer);
    };
  }, [cockpitController.incidentMode, incidentAutoEnabled, liveFeedEnabled]);

  useEffect(() => {
    const now = incidentAutoTickMs;
    if (wsState === "connected") {
      wsDegradedSinceRef.current = null;
    } else if (
      wsState === "connecting" ||
      wsState === "reconnecting" ||
      wsState === "error"
    ) {
      if (wsDegradedSinceRef.current === null) {
        wsDegradedSinceRef.current = now;
      }
    }

    if (!incidentAutoEnabled || !liveFeedEnabled) {
      return;
    }

    const hasCriticalNow = hasCriticalEventWithinWindow(
      liveFeedEvents,
      now,
      opsConfig.safety.incident_high_burst_window_ms
    );
    const recentHighCount = countRecentHighSeverityEvents(
      liveFeedEvents,
      now,
      opsConfig.safety.incident_high_burst_window_ms
    );
    const highBurstTriggered =
      recentHighCount >= opsConfig.safety.incident_high_burst_threshold;
    const healthDegradedTriggered =
      wsDegradedSinceRef.current !== null &&
      now - wsDegradedSinceRef.current >=
        opsConfig.safety.incident_health_degraded_trigger_ms;

    if (!cockpitController.incidentMode) {
      if (hasCriticalNow) {
        setIncidentModeAutomatically(true, "critical event");
        return;
      }
      if (manualIncidentOverrideRef.current === "off") {
        if (now < incidentAutoSuppressedUntilMs) {
          return;
        }
        manualIncidentOverrideRef.current = null;
      }
      if (manualIncidentOverrideRef.current === "on") {
        return;
      }
      if (highBurstTriggered) {
        setIncidentModeAutomatically(
          true,
          `${recentHighCount} high/critical events in 60 seconds`
        );
        return;
      }
      if (healthDegradedTriggered) {
        setIncidentModeAutomatically(true, "gateway degraded >30s");
      }
      return;
    }

    if (manualIncidentOverrideRef.current === "on") {
      return;
    }

    const hasRecentHighOrCritical = countRecentHighSeverityEvents(
      liveFeedEvents,
      now,
      opsConfig.safety.incident_healthy_exit_ms
    ) > 0;
    const wsHealthy = wsState === "connected";
    if (wsHealthy && !hasRecentHighOrCritical) {
      if (healthySinceRef.current <= 0) {
        healthySinceRef.current = now;
      }
      if (now - healthySinceRef.current >= opsConfig.safety.incident_healthy_exit_ms) {
        setIncidentModeAutomatically(false, "system healthy for 5 minutes");
      }
      return;
    }
    healthySinceRef.current = now;
  }, [
    cockpitController.incidentMode,
    incidentAutoTickMs,
    incidentAutoEnabled,
    incidentAutoSuppressedUntilMs,
    liveFeedEvents,
    liveFeedEnabled,
    opsConfig.safety.incident_health_degraded_trigger_ms,
    opsConfig.safety.incident_healthy_exit_ms,
    opsConfig.safety.incident_high_burst_threshold,
    opsConfig.safety.incident_high_burst_window_ms,
    setIncidentModeAutomatically,
    wsState,
  ]);

  const resetTabState = useCallback((tab: MissionControlTab) => {
    setTabResetVersion((previous) => ({
      ...previous,
      [tab]: (previous[tab] ?? 0) + 1,
    }));
  }, []);

  const enterSafeMode = useCallback((reason: string) => {
    setSafeModeReason(reason);
  }, []);

  const resumeFromSafeMode = useCallback(() => {
    setSafeModeReason(null);
    setTabResetVersion((previous) => {
      const next: Partial<Record<MissionControlTab, number>> = {};
      for (const [tab, version] of Object.entries(previous)) {
        next[tab as MissionControlTab] = (version ?? 0) + 1;
      }
      return next;
    });
  }, []);

  const handleGatewayEvent = useCallback(
    (frame: WsEventFrame) => {
      if (frame.event_type === "gateway.status") {
        // The ExecAss durable resume frame follows gateway.status by contract.
        officeController.notifyGatewayStatus();
      }
      const normalized = ingestLiveFeedFrame(frame);
      setEventStream((previous) => {
        const next: EventStreamItem = {
          event_id: frame.event_id,
          event_type: frame.event_type,
          entity: frame.entity,
          ts_unix_ms: frame.ts_unix_ms,
          payload: frame.payload,
        };
        return [next, ...previous].slice(0, EVENT_STREAM_BUFFER_CAP);
      });

      const isAgentMailEvent = frame.event_type.startsWith("agent_mail.");
      if (
        frame.event_type.startsWith("job.") ||
        frame.event_type.startsWith("approval.") ||
        frame.event_type.startsWith("board.") ||
        frame.event_type.startsWith("channel.") ||
        frame.event_type.startsWith("extension.")
      ) {
        queueMissionControlRefresh(settings);
        strategyController.queueRefresh(settings);
        runbookController.queueRefresh(settings);
        connectorsController.queueRefresh();
      }
      if (isAgentMailEvent) {
        queueAgentMailRefresh(settings);
      }

      if (
        incidentAutoEnabled &&
        liveFeedEnabled &&
        normalized.severity === "critical"
      ) {
        setIncidentModeAutomatically(true, "critical event");
      }

      applyGatewayBoardEvent(frame, settings);
    },
    [
      applyGatewayBoardEvent,
      ingestLiveFeedFrame,
      incidentAutoEnabled,
      liveFeedEnabled,
      officeController,
      queueAgentMailRefresh,
      queueMissionControlRefresh,
      setIncidentModeAutomatically,
      setEventStream,
      settings,
      connectorsController,
      runbookController,
      strategyController,
    ]
  );

  useGatewayEvents({
    settings,
    tokenConfigured,
    maxReconnectAttempts: WS_MAX_RECONNECT_ATTEMPTS,
    onState: setWsState,
    onEvent: handleGatewayEvent,
    onExecassFrame: officeController.handleExecassFrame,
    onOpen: officeController.handleWsOpen,
  });

  const refreshAllReadModels = useCallback(() => {
    missionControl.queueMissionControlRefresh(settings);
    strategyController.queueRefresh(settings);
    runbookController.queueRefresh(settings);
    connectorsController.queueRefresh();
  }, [
    connectorsController,
    missionControl,
    runbookController,
    settings,
    strategyController,
  ]);

  useEffect(() => {
    if (!startupBaselineKey) {
      lastAutoBaselineKeyRef.current = null;
      return;
    }
    if (lastAutoBaselineKeyRef.current === startupBaselineKey) {
      return;
    }
    lastAutoBaselineKeyRef.current = startupBaselineKey;
    void loadBaseline(settings).catch((error: unknown) => {
      lastAutoBaselineKeyRef.current = null;
      setNotice({
        tone: "error",
        message: `Initial connection sync failed: ${String(error)}`,
      });
    }).finally(() => {
      setInitialBootstrapSettledKey(startupBaselineKey);
    });
  }, [loadBaseline, setNotice, settings, startupBaselineKey]);

  useEffect(() => {
    if (!runbookHubEnabled || !assistantController.lastRunId) {
      return;
    }
    runbookController.queueRefresh(settings);
  }, [
    assistantController.lastRunId,
    runbookController,
    runbookHubEnabled,
    settings,
  ]);

  if (safeModeReason) {
    return <SafeModePanel reason={safeModeReason} onResume={resumeFromSafeMode} />;
  }

  return (
    <>
    <AppShell
      activeTab={activeTab}
      availableTabs={availableTabs}
      elevatorFloors={elevatorFloors}
      onTabChange={setActiveTab}
      activeRoomId={resolvedActiveRoomId}
      onRoomSelect={selectAvailableRoom}
      healthState={healthState}
      wsState={wsState}
      tokenConfigured={tokenConfigured}
      incidentMode={cockpitController.incidentMode}
      onIncidentModeChange={setIncidentModeFromOperator}
      openBreakerCount={
        missionControl.openBreakers.length + missionControl.openPluginBreakers.length
      }
      approvalsCount={missionControl.approvalsById.size}
      memoryReviewApprovalsCount={missionControl.memoryReviewApprovalsCount}
      jobsDue={missionControl.jobsStatus?.jobs_due ?? 0}
      schedulerRunning={missionControl.jobsStatus?.scheduler_running ?? false}
      gatewayDraft={gatewayDraft}
      onGatewayDraftChange={setGatewayDraft}
      tokenDraft={tokenDraft}
      onTokenDraftChange={setTokenDraft}
      onSaveConnection={saveConnection}
      onReconnect={reconnect}
      onClearToken={clearToken}
      onOpenSetupWizard={onboarding.openWizard}
      onOpenHelpDocs={openHelpDocs}
      onOpenGuidedTour={openGuidedTour}
      onRefresh={refreshAllReadModels}
      notifications={notifications}
      onDismissNotification={dismissNotification}
      onClearAllNotifications={clearAllNotifications}
      liveFeedEnabled={liveFeedEnabled}
      liveFeedOpen={liveFeed.drawerOpen}
      liveFeedUnreadCount={liveFeed.unreadCount}
      onToggleLiveFeed={liveFeed.toggleDrawer}
      opsUxConfig={opsConfig}
      opsUxConfigError={opsUxRuntime.error}
      onPatchOpsUxControls={patchOpsControls}
      usageChartsEnabled={usageChartsEnabled}
      assistantSystemPrompt={assistantSystemPromptDraft}
      assistantSystemPromptDirty={assistantSystemPromptDirty}
      assistantSystemPromptLoading={assistantSystemPromptLoading}
      assistantSystemPromptSaving={assistantSystemPromptSaving}
      assistantSystemPromptError={assistantSystemPromptError}
      onAssistantSystemPromptChange={setAssistantSystemPromptDraft}
      onSaveAssistantSystemPrompt={saveAssistantSystemPrompt}
      onResetAssistantSystemPrompt={resetAssistantSystemPromptDraft}
      onRestoreDefaultAssistantSystemPrompt={restoreDefaultAssistantSystemPromptDraft}
      quickGuideAvailable={activeTab !== "help"}
      quickGuideOpen={quickGuideVisibleOnActiveTab}
      onToggleQuickGuide={toggleQuickGuideForActiveTab}
      liveFeedPanel={
        <LiveFeedDrawer
          enabled={liveFeedEnabled}
          open={liveFeed.drawerOpen}
          paused={liveFeed.paused}
          unreadCount={liveFeed.unreadCount}
          domainFilter={liveFeed.domainFilter}
          severityFilter={liveFeed.severityFilter}
          events={liveFeed.renderEvents}
          storageMode={liveFeed.storageMode}
          storageError={liveFeed.storageError}
          recoveryAvailableCount={liveFeed.recoveryAvailableCount}
          markAllUndoAvailable={liveFeed.markAllUndoAvailable}
          clearUndoAvailable={liveFeed.clearUndoAvailable}
          approvalsCount={missionControl.approvalsById.size}
          openBreakersCount={
            missionControl.openBreakers.length + missionControl.openPluginBreakers.length
          }
          mailUnreadCount={mailController.mailThreads.reduce((sum, t) => sum + (t.unread_count ?? 0), 0)}
          onToggleOpen={liveFeed.toggleDrawer}
          onTogglePause={liveFeed.togglePause}
          onDomainFilterChange={liveFeed.setDomainFilter}
          onSeverityFilterChange={liveFeed.setSeverityFilter}
          onMarkAllRead={liveFeed.markAllRead}
          onUndoMarkAllRead={liveFeed.undoMarkAllRead}
          onClearSoft={liveFeed.clearFeedSoft}
          onRestoreClear={liveFeed.restoreFromClearUndo}
          onRestoreRecovery={liveFeed.restoreFromRecoveryLog}
        />
      }
      navBadges={{
        focus: missionControl.approvalsById.size,
        mail: mailController.mailThreads.reduce((sum, t) => sum + (t.unread_count ?? 0), 0),
        connectors: connectorsController.summary.pendingInteractions,
      }}
    >
      <OnboardingWizard
        controller={onboarding}
        agents={agents}
        onOpenSimpleIntegrationWizard={openSimpleIntegrationWizard}
      />
      <AppContent
        activeTab={activeTab}
        onTabChange={setActiveTab}
        onRoomSelect={selectAvailableRoom}
        onOpenHelpDocs={openHelpDocs}
        helpDocsTarget={helpDocsTarget}
        onStartGuidedTour={openGuidedTour}
        onRefreshBaseline={() => loadBaseline(settings)}
        settings={settings}
        tokenConfigured={tokenConfigured}
        boards={boards}
        agents={agents}
        boardsController={boardsController}
        missionControl={missionControl}
        mailController={mailController}
        assistantController={assistantController}
        officeController={officeController}
        glassWindowController={glassWindowController}
        cockpitController={cockpitController}
        strategyController={strategyController}
        runbookController={runbookController}
        memoryController={memoryController}
        connectorsController={connectorsController}
        showRawEvents={showRawEvents}
        setShowRawEvents={setShowRawEvents}
        visibleEvents={visibleEvents}
        onResetTabState={resetTabState}
        onEnterSafeMode={enterSafeMode}
        tabResetVersion={tabResetVersion}
        setNotice={setNotice}
        usageChartsEnabled={usageChartsEnabled}
        onOpenSimpleIntegrationWizard={openSimpleIntegrationWizard}
        quickGuidesCollapsed={quickGuideState.collapsed}
        quickGuideOpenTab={quickGuideState.openTab}
        onDismissQuickGuides={dismissQuickGuides}
      />
    </AppShell>
    <SimpleIntegrationWizard
      open={simpleIntegrationWizardState.open}
      onClose={closeSimpleIntegrationWizard}
      settings={settings}
      agents={agents}
      initialIntegrationId={simpleIntegrationWizardState.initialIntegrationId}
      onTabChange={setActiveTab}
    />
    <GuidedTourOverlay
      open={guidedTourOpen}
      steps={guidedTourSteps}
      stepIndex={guidedTourStep}
      onPrev={() => setGuidedTourStep((value) => Math.max(0, value - 1))}
      onNext={() => {
        setGuidedTourStep((value) => {
          if (value + 1 >= guidedTourSteps.length) {
            closeGuidedTour();
            return value;
          }
          return value + 1;
        });
      }}
      onClose={closeGuidedTour}
    />
    {runtimeCloseConfirmation ? (
      <RuntimeCloseDialog
        confirmation={runtimeCloseConfirmation}
        confirming={runtimeCloseConfirming}
        onConfirm={confirmRuntimeClose}
        onCancel={cancelRuntimeClose}
      />
    ) : null}
    <ToastStack toasts={toasts} onDismiss={dismissToast} />
    </>
  );
}
