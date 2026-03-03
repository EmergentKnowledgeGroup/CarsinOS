import { useCallback, useEffect, useMemo, useState } from "react";
import { AppContent } from "./app/AppContent";
import { AppShell } from "./app/AppShell";
import { GuidedTourOverlay, type GuidedTourStep } from "./app/GuidedTourOverlay";
import {
  useAppController,
  type EventStreamItem,
} from "./app/useAppController";
import { useGatewayEvents } from "./app/useGatewayEvents";
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
import { ToastStack } from "./ui/Toast";
import { useToasts } from "./ui/useToasts";
import type { Agent, WsEventFrame } from "./types";
import { EVENT_STREAM_BUFFER_CAP, WS_MAX_RECONNECT_ATTEMPTS } from "./constants";
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
    () =>
      showRawEvents
        ? eventStream
        : eventStream.filter((event) => !event.event_type.startsWith("heartbeat.")),
    [eventStream, showRawEvents]
  );

  const handleGatewayEvent = useCallback(
    (frame: WsEventFrame) => {
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

      applyGatewayBoardEvent(frame, settings);
    },
    [
      applyGatewayBoardEvent,
      queueAgentMailRefresh,
      queueMissionControlRefresh,
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

  return (
    <>
    <AppShell
      activeTab={activeTab}
      onTabChange={setActiveTab}
      healthState={healthState}
      wsState={wsState}
      tokenConfigured={tokenConfigured}
      incidentMode={cockpitController.incidentMode}
      onIncidentModeChange={cockpitController.setIncidentMode}
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
