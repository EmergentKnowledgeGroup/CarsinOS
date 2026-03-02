import { useCallback, useMemo, useState } from "react";
import { AppContent } from "./app/AppContent";
import { AppShell } from "./app/AppShell";
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
import { useBoardsController } from "./features/boards/useBoardsController";
import { useCockpitController } from "./features/cockpit/useCockpitController";
import { OnboardingWizard } from "./features/onboarding/OnboardingWizard";
import { useOnboardingController } from "./features/onboarding/useOnboardingController";
import { ToastStack } from "./ui/Toast";
import { useToasts } from "./ui/useToasts";
import type { Agent, WsEventFrame } from "./types";
import { EVENT_STREAM_BUFFER_CAP, WS_MAX_RECONNECT_ATTEMPTS } from "./constants";
import "./styles.css";

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
        settings={settings}
        boards={boards}
        agents={agents}
        boardsController={boardsController}
        missionControl={missionControl}
        mailController={mailController}
        cockpitController={cockpitController}
        showRawEvents={showRawEvents}
        setShowRawEvents={setShowRawEvents}
        visibleEvents={visibleEvents}
        setNotice={setNotice}
      />
    </AppShell>
    <ToastStack toasts={toasts} onDismiss={dismissToast} />
    </>
  );
}
