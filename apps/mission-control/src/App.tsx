import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AppContent } from "./app/AppContent";
import { AppShell } from "./app/AppShell";
import { GuidedTourOverlay, type GuidedTourStep } from "./app/GuidedTourOverlay";
import { LiveFeedDrawer } from "./app/LiveFeedDrawer";
import {
  useAppController,
  type EventStreamItem,
  type MissionControlTab,
} from "./app/useAppController";
import { useGatewayEvents } from "./app/useGatewayEvents";
import { useLiveFeedController } from "./app/useLiveFeedController";
import { useMissionControlController } from "./app/useMissionControlController";
import {
  useRuntimeConnectionController,
  type BoardSummary,
} from "./app/useRuntimeConnectionController";
import { useAgentMailController } from "./features/agentMail/useAgentMailController";
import { useAssistantChatController } from "./features/assistant/useAssistantChatController";
import { useBoardsController } from "./features/boards/useBoardsController";
import { useCockpitController } from "./features/cockpit/useCockpitController";
import { OnboardingWizard } from "./features/onboarding/OnboardingWizard";
import { useOnboardingController } from "./features/onboarding/useOnboardingController";
import { SafeModePanel } from "./ui/SafeModePanel";
import { ToastStack } from "./ui/Toast";
import { useToasts } from "./ui/useToasts";
import type { Agent, WsEventFrame } from "./types";
import { EVENT_STREAM_BUFFER_CAP, WS_MAX_RECONNECT_ATTEMPTS } from "./constants";
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
    id: "help",
    tab: "help",
    targetId: "nav-help-shortcut",
    title: "Help/Docs = in-app knowledge base",
    body: "This section explains each tab with examples and links back into live workflows.",
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
  const [guidedTourOpen, setGuidedTourOpen] = useState(false);
  const [guidedTourStep, setGuidedTourStep] = useState(0);
  const [safeModeReason, setSafeModeReason] = useState<string | null>(null);
  const [tabResetVersion, setTabResetVersion] = useState<Partial<Record<MissionControlTab, number>>>({});
  const [opsUxRuntime, setOpsUxRuntime] = useState(() => loadOpsUxRuntimeConfig());
  const [incidentAutoSuppressedUntilMs, setIncidentAutoSuppressedUntilMs] = useState(0);
  const manualIncidentOverrideRef = useRef<"on" | "off" | null>(null);
  const wsDegradedSinceRef = useRef<number | null>(null);
  const healthySinceRef = useRef<number>(0);
  const previousIncidentModeRef = useRef(false);

  const opsConfig = opsUxRuntime.config;
  const optionalModulesEnabled = !opsConfig.controls.global_kill_switch;
  const liveFeedEnabled = optionalModulesEnabled && opsConfig.controls.live_feed_drawer;
  const incidentAutoEnabled =
    optionalModulesEnabled && opsConfig.controls.incident_auto_trigger;
  const usageChartsEnabled = optionalModulesEnabled && opsConfig.controls.usage_charts;

  const patchOpsControls = useCallback(
    (patch: Partial<OpsUxFeatureControls>) => {
      const nextConfig = withOpsUxControlPatch(opsConfig, patch);
      const persisted = saveOpsUxRuntimeConfig(nextConfig);
      setOpsUxRuntime({
        config: nextConfig,
        degraded: !persisted.ok,
        error: persisted.error,
      });
      if (!persisted.ok) {
        setNotice({
          tone: "error",
          message: persisted.error ?? "Runtime config persistence failed.",
        });
      }
    },
    [opsConfig, setNotice]
  );

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
    boards,
    setNotice,
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
    setTokenDraft,
    setTokenConfigured,
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
    loadAgentMailReadModels: mailController.loadAgentMailReadModels,
    });

  const onboarding = useOnboardingController({
    settings,
    tokenConfigured,
    agents,
    authProfiles: missionControl.authProfiles,
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

  const openHelpDocs = useCallback(() => {
    setActiveTab("help");
  }, [setActiveTab]);

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
    if (!guidedTourOpen) {
      return;
    }
    const step = GUIDED_TOUR_STEPS[guidedTourStep];
    if (step?.tab) {
      setActiveTab(step.tab);
    }
  }, [guidedTourOpen, guidedTourStep, setActiveTab]);

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
    const now = Date.now();
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
        frame.event_type.startsWith("channel.") ||
        frame.event_type.startsWith("extension.")
      ) {
        queueMissionControlRefresh(settings);
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
      queueAgentMailRefresh,
      queueMissionControlRefresh,
      setIncidentModeAutomatically,
      setEventStream,
      settings,
    ]
  );

  useGatewayEvents({
    settings,
    tokenConfigured,
    maxReconnectAttempts: WS_MAX_RECONNECT_ATTEMPTS,
    onState: setWsState,
    onEvent: handleGatewayEvent,
  });

  if (safeModeReason) {
    return <SafeModePanel reason={safeModeReason} onResume={resumeFromSafeMode} />;
  }

  return (
    <>
    <AppShell
      activeTab={activeTab}
      onTabChange={setActiveTab}
      healthState={healthState}
      wsState={wsState}
      tokenConfigured={tokenConfigured}
      incidentMode={cockpitController.incidentMode}
      onIncidentModeChange={setIncidentModeFromOperator}
      openBreakerCount={
        missionControl.openBreakers.length + missionControl.openPluginBreakers.length
      }
      approvalsCount={missionControl.approvalsById.size}
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
      onRefresh={() => missionControl.queueMissionControlRefresh(settings)}
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
      }}
    >
      <OnboardingWizard controller={onboarding} agents={agents} />
      <AppContent
        activeTab={activeTab}
        onTabChange={setActiveTab}
        onOpenHelpDocs={openHelpDocs}
        onStartGuidedTour={openGuidedTour}
        settings={settings}
        boards={boards}
        agents={agents}
        boardsController={boardsController}
        missionControl={missionControl}
        mailController={mailController}
        assistantController={assistantController}
        cockpitController={cockpitController}
        showRawEvents={showRawEvents}
        setShowRawEvents={setShowRawEvents}
        visibleEvents={visibleEvents}
        onResetTabState={resetTabState}
        onEnterSafeMode={enterSafeMode}
        tabResetVersion={tabResetVersion}
        setNotice={setNotice}
      />
    </AppShell>
    <GuidedTourOverlay
      open={guidedTourOpen}
      steps={GUIDED_TOUR_STEPS}
      stepIndex={guidedTourStep}
      onPrev={() => setGuidedTourStep((value) => Math.max(0, value - 1))}
      onNext={() => {
        setGuidedTourStep((value) => {
          if (value + 1 >= GUIDED_TOUR_STEPS.length) {
            closeGuidedTour();
            return value;
          }
          return value + 1;
        });
      }}
      onClose={closeGuidedTour}
    />
    <ToastStack toasts={toasts} onDismiss={dismissToast} />
    </>
  );
}
