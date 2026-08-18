import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AppContent } from "./app/AppContent";
import { AppShell } from "./app/AppShell";
import { GuidedTourOverlay } from "./app/GuidedTourOverlay";
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
import { usePeopleRoutingController } from "./features/peopleRouting/usePeopleRoutingController";
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
import { useExecassPolicyController } from "./features/execassPolicy/useExecassPolicyController";
import type { SetupSurfaceProps } from "./features/setup/SetupControls";
import { useGlassWindowController } from "./features/glassWindow/useGlassWindowController";
import { findRoom, roomForTab } from "./glass/floors";
import { buildGuidedTourSteps } from "./glass/guidedTour";
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
  INITIAL_INCIDENT_MODE_AUTO_STATE,
  applyOperatorIncidentToggle,
  composeIncidentPosture,
  decideAutomaticIncidentMode,
} from "./glass/incidentPosture";
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
  const [authIdentityGeneration, setAuthIdentityGeneration] = useState(0);
  const markAuthIdentityChanged = useCallback(() => {
    setAuthIdentityGeneration((generation) => generation + 1);
  }, []);
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
  const lastAutoBaselineKeyRef = useRef<string | null>(null);
  // Operator-override and announce state for the automatic incident-mode
  // policy. Pure transitions live in glass/incidentPosture.
  const incidentAutoStateRef = useRef(INITIAL_INCIDENT_MODE_AUTO_STATE);
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
  // Setup and Policy share the Connectors render route, but Setup is the
  // authority used to turn Connectors back on and Policy is the owner's
  // autonomy surface. Keep both stable rooms independently available when
  // the optional Connectors product surface is disabled.
  const alwaysAvailableElevatorRooms = useMemo(() => ["setup", "policy"], []);
  const elevatorFloors = useResolvedElevator(
    availableTabs,
    alwaysAvailableElevatorRooms,
  );
  const selectAvailableRoom = useCallback(
    (roomId: string) => selectRoom(roomId, elevatorFloors),
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
  // Tour stops derive from the exact resolved elevator registry rendered
  // to the user, so hidden or capability-disabled floors and rooms can
  // never become tour targets.
  const guidedTourSteps = useMemo(
    () => buildGuidedTourSteps(elevatorFloors),
    [elevatorFloors]
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
  // One People & Routing authority for the whole app: Directory / Front Desk
  // edits it, Team reads routed-people facts and delegates removal cleanup.
  const peopleRoutingController = usePeopleRoutingController({
    settings,
    tokenConfigured,
    agents,
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
    tokenConfigured,
    peopleRouting: peopleRoutingController,
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
    authIdentityGeneration,
    active: activeTab === "assistant",
    setNotice,
  });
  // The one App-owned policy controller. Its invalidation signal comes from
  // the Office controller's durable stream - never a second websocket.
  const policyController = useExecassPolicyController({
    settings,
    tokenConfigured,
    active: activeTab === "connectors" && resolvedActiveRoomId === "policy",
    authIdentityGeneration,
    policyInvalidationGeneration: officeController.policyInvalidationGeneration,
    setNotice,
  });
  const glassWindowController = useGlassWindowController({
    settings,
    tokenConfigured,
    active: activeTab === "window",
  });

  // One pure composition owns automatic incident posture. Inputs are facts
  // the existing controllers already hold authoritatively — never raw event
  // bursts, severity copy, clocks, or invented backend health.
  const officeSummary = officeController.summary;
  const officeStopAll = officeController.stopAll;
  const officeIntegrityFailure = officeController.integrityFailure;
  const incidentPosture = useMemo(
    () =>
      composeIncidentPosture({
        connection: {
          configured: tokenConfigured && settings.gateway_url.trim().length > 0,
          healthState,
          wsState,
        },
        operations: {
          // The mission-control read models load together; gateway or jobs
          // status arriving means the breaker facts have loaded at least once.
          loaded:
            missionControl.gatewayStatus !== null ||
            missionControl.jobsStatus !== null,
          openCoreBreakers: missionControl.openBreakers.map((breaker) => ({
            scope: breaker.scope,
            targetId: breaker.target_id,
          })),
          openPluginBreakers: missionControl.openPluginBreakers.map(
            (breaker) => ({ pluginId: breaker.plugin_id }),
          ),
          schedulerRunning: missionControl.jobsStatus?.scheduler_running ?? false,
          jobsDue: missionControl.jobsStatus?.jobs_due ?? 0,
        },
        execass: {
          summaryLoaded: officeSummary !== null,
          stopAllEngaged: officeStopAll?.engaged ?? null,
          needsYou: (officeSummary?.needs_you ?? []).map((item) => ({
            attentionId: item.attention_id,
            kind: item.kind,
            scopeKind: item.subject.scope_kind,
            delegationId:
              item.subject.scope_kind === "delegation"
                ? item.subject.delegation_id
                : null,
            runtimeActualState:
              item.subject.scope_kind === "runtime_host"
                ? item.subject.runtime_actual_state
                : null,
            reason: item.reason,
          })),
          delegations: [
            ...(officeSummary?.in_motion ?? []),
            ...(officeSummary?.done ?? []),
          ].map((delegation) => ({
            delegationId: delegation.delegation_id,
            phase: delegation.phase,
          })),
          integrityFailure: officeIntegrityFailure,
        },
      }),
    [
      healthState,
      missionControl.gatewayStatus,
      missionControl.jobsStatus,
      missionControl.openBreakers,
      missionControl.openPluginBreakers,
      officeIntegrityFailure,
      officeStopAll,
      officeSummary,
      settings.gateway_url,
      tokenConfigured,
      wsState,
    ],
  );
  const incidentWalkAvailable =
    incidentPosture.posture === "incident" &&
    Boolean(findRoom(elevatorFloors, incidentPosture.target.roomId));

  const liveFeed = useLiveFeedController({
    retentionWindowMs: opsConfig.safety.recovery_retention_window_ms,
    recoveryMaxBytes: opsConfig.safety.recovery_log_max_bytes,
    markReadUndoWindowMs: opsConfig.safety.mark_read_undo_window_ms,
  });
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
    onAuthIdentityChanged: markAuthIdentityChanged,
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
      // The operator toggle is an explicit presentation override over the
      // composed posture: "off" silences exactly the current cause, "on"
      // holds the mode through calm.
      const decision = applyOperatorIncidentToggle({
        next,
        posture: incidentPosture,
        state: incidentAutoStateRef.current,
      });
      incidentAutoStateRef.current = decision.nextState;
      if (decision.incidentMode !== null) {
        cockpitController.setIncidentMode(decision.incidentMode);
      }
    },
    [cockpitController, incidentPosture]
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

  // One shared Setup authority bundle. AppShell Settings receives these
  // exact drafts, facts, and callbacks as individual props; the Basement
  // Setup room receives this bundle. Both stay live-synchronized because
  // neither owns a copy.
  const setupSurface = useMemo<SetupSurfaceProps>(
    () => ({
      gatewayDraft,
      onGatewayDraftChange: setGatewayDraft,
      tokenDraft,
      onTokenDraftChange: setTokenDraft,
      tokenConfigured,
      healthState,
      wsState,
      onSaveConnection: saveConnection,
      onReconnect: reconnect,
      onClearToken: clearToken,
      onOpenSetupWizard: onboarding.openWizard,
      onOpenGuidedTour: openGuidedTour,
      opsUxConfig: opsConfig,
      opsUxConfigError: opsUxRuntime.error,
      onPatchOpsUxControls: patchOpsControls,
      usageChartsEnabled,
    }),
    [
      clearToken,
      gatewayDraft,
      healthState,
      onboarding.openWizard,
      openGuidedTour,
      opsConfig,
      opsUxRuntime.error,
      patchOpsControls,
      reconnect,
      saveConnection,
      setGatewayDraft,
      setTokenDraft,
      tokenConfigured,
      tokenDraft,
      usageChartsEnabled,
      wsState,
    ]
  );

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
    if (
      activeTab === "connectors" &&
      !connectorsHubEnabled &&
      activeRoomId !== "setup" &&
      activeRoomId !== "policy"
    ) {
      selectRoom("setup", elevatorFloors);
      setNotice({
        tone: "info",
        message: "Connectors was turned off, so you were moved to Setup.",
      });
    }
  }, [
    activeRoomId,
    activeTab,
    connectorsHubEnabled,
    elevatorFloors,
    selectRoom,
    setNotice,
  ]);

  useEffect(() => {
    if (!initialBootstrapSettled) {
      return;
    }
    void loadAssistantSystemPromptConfig(settings);
  }, [initialBootstrapSettled, loadAssistantSystemPromptConfig, settings]);

  // Walk the tour by stable room id through the same resolved registry the
  // elevator renders. selectRoom fails closed on unknown ids, so a stop
  // whose room vanished mid-tour changes nothing — the overlay shows its
  // honest missing-target recovery copy instead of navigating blind.
  useEffect(() => {
    if (!guidedTourOpen) {
      return;
    }
    const step = guidedTourSteps[guidedTourStep];
    if (step?.roomId && step.roomId !== resolvedActiveRoomId) {
      selectAvailableRoom(step.roomId);
    }
  }, [
    guidedTourOpen,
    guidedTourStep,
    guidedTourSteps,
    resolvedActiveRoomId,
    selectAvailableRoom,
  ]);

  // If floors resolve away while the tour is open, the steps array shrinks;
  // clamp the index so the tour recovers instead of pointing past the end.
  useEffect(() => {
    if (!guidedTourOpen || guidedTourSteps.length === 0) {
      return;
    }
    if (guidedTourStep >= guidedTourSteps.length) {
      setGuidedTourStep(guidedTourSteps.length - 1);
    }
  }, [guidedTourOpen, guidedTourStep, guidedTourSteps]);

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
    // Automatic incident mode follows the composed authoritative posture.
    // The decision is pure and idempotent: unknown holds the mode, calm
    // clears it without a click, and one cause never announces twice.
    const decision = decideAutomaticIncidentMode({
      posture: incidentPosture,
      autoEnabled: incidentAutoEnabled,
      currentMode: cockpitController.incidentMode,
      state: incidentAutoStateRef.current,
    });
    incidentAutoStateRef.current = decision.nextState;
    if (decision.announce) {
      addToast(
        decision.announce.message,
        decision.announce.kind === "raised" ? "critical" : "info",
      );
    }
    if (decision.incidentMode !== null) {
      cockpitController.setIncidentMode(decision.incidentMode);
    }
  }, [
    addToast,
    cockpitController,
    incidentAutoEnabled,
    incidentPosture,
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
      ingestLiveFeedFrame(frame);
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

      applyGatewayBoardEvent(frame, settings);
    },
    [
      applyGatewayBoardEvent,
      ingestLiveFeedFrame,
      officeController,
      queueAgentMailRefresh,
      queueMissionControlRefresh,
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
    authIdentityGeneration,
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
      incidentPosture={incidentPosture}
      incidentWalkAvailable={incidentWalkAvailable}
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
      toastPanel={<ToastStack toasts={toasts} onDismiss={dismissToast} />}
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
        memoryRoomAvailable={Boolean(findRoom(elevatorFloors, "memory"))}
        activeRoomId={resolvedActiveRoomId}
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
        peopleRoutingController={peopleRoutingController}
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
        setupSurface={setupSurface}
        policyController={policyController}
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
    </>
  );
}
