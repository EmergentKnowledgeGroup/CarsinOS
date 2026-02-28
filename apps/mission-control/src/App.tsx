import { useCallback, useEffect, useMemo, useState } from "react";
import {
  getGatewayHealth,
  listAgents,
  listBoards,
} from "./lib/api";
import {
  clearGatewayToken,
  isGatewayTokenConfigured,
  persistConnectionSettings,
  setGatewayToken,
} from "./lib/runtime";
import { AppShell } from "./app/AppShell";
import {
  useAppController,
  type EventStreamItem,
} from "./app/useAppController";
import { useGatewayEvents } from "./app/useGatewayEvents";
import { useMissionControlController } from "./app/useMissionControlController";
import { useAgentMailController } from "./features/agentMail/useAgentMailController";
import { ChatroomsPage } from "./features/agentMail/ChatroomsPage";
import { MailPage } from "./features/agentMail/MailPage";
import { BoardsPage } from "./features/boards/BoardsPage";
import { useBoardsController } from "./features/boards/useBoardsController";
import { CalendarPage } from "./features/calendar/CalendarPage";
import { CockpitPage } from "./features/cockpit/CockpitPage";
import { useCockpitController } from "./features/cockpit/useCockpitController";
import { EventsPage } from "./features/events/EventsPage";
import { FocusPage } from "./features/focus/FocusPage";
import type { CockpitWidgetLayout } from "./features/cockpit/cockpitLayout";
import { CockpitWidgetRenderer } from "./features/cockpit/CockpitWidgetRenderer";
import type {
  Agent,
  RuntimeConnectionSettings,
  WsEventFrame,
} from "./types";
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
    notice,
    setNotice,
    eventStream,
    setEventStream,
    showRawEvents,
    setShowRawEvents,
  } = useAppController();

  const [boards, setBoards] = useState<{ board_id: string; name: string }[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);

  const boardsController = useBoardsController({
    settings,
    setNotice,
  });

  const {
    incidentMode,
    setIncidentMode,
    cockpitPages,
    setCockpitPages,
    setActiveCockpitPageId,
    activeCockpitPage,
    addCockpitWidget,
    removeCockpitWidget,
    moveCockpitWidget,
    resizeCockpitWidget,
    resetCockpitLayout,
    addCockpitPage,
    exportCockpitLayout,
    importCockpitLayout,
  } = useCockpitController();

  const mailController = useAgentMailController({
    settings,
    tokenConfigured,
    setNotice,
  });

  const missionControl = useMissionControlController({
    settings,
    agents,
    incidentMode,
    setNotice,
  });
  const queueMissionControlRefresh = missionControl.queueMissionControlRefresh;
  const loadMissionControlReadModels = missionControl.loadMissionControlReadModels;
  const queueAgentMailRefresh = mailController.queueAgentMailRefresh;
  const loadAgentMailReadModels = mailController.loadAgentMailReadModels;
  const applyGatewayBoardEvent = boardsController.applyGatewayBoardEvent;

  const visibleEvents = useMemo(() => {
    if (showRawEvents) {
      return eventStream;
    }
    return eventStream.filter((event) => !event.event_type.startsWith("heartbeat."));
  }, [eventStream, showRawEvents]);

  const loadBaseline = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      preferredBoardId?: string | null
    ) => {
      if (!runtimeSettings.gateway_url.trim()) {
        return;
      }

      setHealthState("checking");
      const [health, boardList, agentList] = await Promise.all([
        getGatewayHealth(runtimeSettings),
        listBoards(runtimeSettings),
        listAgents(runtimeSettings),
      ]);

      setHealthState(health.ok === false ? "down" : "up");
      setBoards(boardList.items.map((item) => ({ board_id: item.board_id, name: item.name })));
      setAgents(agentList.items);

      const targetBoardId =
        preferredBoardId ??
        boardsController.activeBoardId ??
        boardList.items[0]?.board_id ??
        null;
      boardsController.setActiveBoardId(targetBoardId);
      if (targetBoardId) {
        await boardsController.refreshBoard(targetBoardId, runtimeSettings);
      } else {
        boardsController.setBoard(null);
      }
      await Promise.all([
        loadMissionControlReadModels(runtimeSettings),
        loadAgentMailReadModels(runtimeSettings),
      ]);
    },
    [
      boardsController,
      loadAgentMailReadModels,
      loadMissionControlReadModels,
      setHealthState,
      settings,
    ]
  );

  useEffect(() => {
    void isGatewayTokenConfigured().then(setTokenConfigured);
  }, [setTokenConfigured]);

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
        return [next, ...previous].slice(0, 400);
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
    maxReconnectAttempts: 40,
    onState: setWsState,
    onEvent: handleGatewayEvent,
  });

  const saveConnection = async () => {
    try {
      const nextSettings: RuntimeConnectionSettings = {
        gateway_url: gatewayDraft.trim(),
      };
      persistConnectionSettings(nextSettings);
      setSettings(nextSettings);

      if (tokenDraft.trim()) {
        await setGatewayToken(tokenDraft.trim());
        setTokenDraft("");
      }

      const hasToken = await isGatewayTokenConfigured();
      setTokenConfigured(hasToken);

      if (hasToken && nextSettings.gateway_url.trim()) {
        await loadBaseline(nextSettings);
        setNotice({ tone: "info", message: "Connection settings saved." });
      }
    } catch (error) {
      setNotice({
        tone: "critical",
        message: `Connection save failed: ${String(error)}`,
      });
    }
  };

  const clearToken = async () => {
    await clearGatewayToken();
    setTokenConfigured(false);
    setWsState("idle");
    setNotice({ tone: "info", message: "Gateway token cleared." });
  };

  const reconnect = async () => {
    try {
      await loadBaseline(settings);
      setNotice({ tone: "info", message: "Connection refreshed." });
    } catch (error) {
      setNotice({
        tone: "critical",
        message: `Reconnect failed: ${String(error)}`,
      });
    }
  };

  const renderCockpitWidget = (widget: CockpitWidgetLayout) => (
    <CockpitWidgetRenderer
      widget={widget}
      incidentMode={incidentMode}
      setIncidentMode={setIncidentMode}
      gatewayStatus={missionControl.gatewayStatus}
      jobsStatus={missionControl.jobsStatus}
      approvalsCount={missionControl.approvalsById.size}
      openBreakers={missionControl.openBreakers}
      openPluginBreakers={missionControl.openPluginBreakers}
      channelStatuses={missionControl.channelStatuses}
      incidentFocusItems={missionControl.incidentFocusItems}
      calendarJobs={missionControl.calendarJobs}
      selectedProviderControlAgentId={missionControl.selectedProviderControlAgentId}
      setSelectedProviderControlAgentId={missionControl.setSelectedProviderControlAgentId}
      selectedProviderControlProvider={missionControl.selectedProviderControlProvider}
      setSelectedProviderControlProvider={missionControl.setSelectedProviderControlProvider}
      providerOptions={missionControl.providerOptions}
      orderedProviderProfiles={missionControl.orderedProviderProfiles}
      providerProfileOrderDirty={missionControl.providerProfileOrderDirty}
      agents={agents}
      skills={missionControl.skills}
      plugins={missionControl.plugins}
      pluginRuntimeById={missionControl.pluginRuntimeById}
      visibleEvents={visibleEvents}
      onRefreshAll={() => queueMissionControlRefresh(settings)}
      onRunCalendarJobNow={missionControl.runCalendarJobNow}
      onToggleCalendarJob={missionControl.toggleCalendarJob}
      onReconnectFocusChannel={missionControl.reconnectFocusChannel}
      onMoveProviderProfile={missionControl.moveProviderProfile}
      onSaveProviderOrder={missionControl.saveProviderOrder}
      onReloadProviderProfileOrder={() => missionControl.reloadProviderProfileOrder(settings)}
      onToggleSkillState={missionControl.toggleSkillState}
      onTogglePluginState={missionControl.togglePluginState}
    />
  );

  return (
    <AppShell
      activeTab={activeTab}
      onTabChange={setActiveTab}
      healthState={healthState}
      wsState={wsState}
      tokenConfigured={tokenConfigured}
      incidentMode={incidentMode}
      onIncidentModeChange={setIncidentMode}
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
      notice={notice}
    >
      {activeTab === "boards" ? (
        <BoardsPage
          boards={boards}
          activeBoardId={boardsController.activeBoardId}
          onBoardChange={boardsController.handleBoardChange}
          columns={boardsController.columns}
          cardsByColumn={boardsController.cardsByColumn}
          selectedCardId={boardsController.selectedCardId}
          dragCardId={boardsController.dragCardId}
          setDragCardId={boardsController.setDragCardId}
          onSelectCard={boardsController.selectCard}
          onDropCard={boardsController.handleDropCard}
          onCreateCard={boardsController.handleCreateCard}
          selectedCard={boardsController.selectedCard}
          cardEditor={boardsController.cardEditor}
          setCardEditor={boardsController.setCardEditor}
          agents={agents}
          onSaveCardDraft={boardsController.saveCardDraft}
          onRunCard={boardsController.runCard}
          onUploadAsset={boardsController.uploadAsset}
          onPreviewAsset={boardsController.previewAsset}
          selectedPreviewUrl={boardsController.selectedPreviewUrl}
        />
      ) : null}

      {activeTab === "calendar" ? (
        <CalendarPage
          calendarWeek={missionControl.calendarWeek}
          calendarAlwaysRunning={missionControl.calendarAlwaysRunning}
          calendarNextUp={missionControl.calendarNextUp}
          calendarJobs={missionControl.calendarJobs}
          onRunCalendarJobNow={missionControl.runCalendarJobNow}
          onToggleCalendarJob={missionControl.toggleCalendarJob}
        />
      ) : null}

      {activeTab === "focus" ? (
        <FocusPage
          focusItems={missionControl.focusItems}
          approvalsCount={missionControl.approvalsById.size}
          channelStatuses={missionControl.channelStatuses}
          onResolveFocusApproval={missionControl.resolveFocusApproval}
          onRunCalendarJobNow={missionControl.runCalendarJobNow}
          onReconnectFocusChannel={missionControl.reconnectFocusChannel}
        />
      ) : null}

      {activeTab === "events" ? (
        <EventsPage
          showRawEvents={showRawEvents}
          onShowRawEventsChange={setShowRawEvents}
          visibleEvents={visibleEvents}
        />
      ) : null}

      {activeTab === "mail" ? (
        <MailPage
          onRefresh={() => mailController.queueAgentMailRefresh(settings)}
          mailboxFilter={mailController.mailboxFilter}
          onMailboxFilterChange={mailController.setMailboxFilter}
          mailPrincipalOverride={mailController.mailPrincipalOverride}
          onMailPrincipalOverrideChange={mailController.setMailPrincipalOverride}
          mailSearch={mailController.mailSearch}
          onMailSearchChange={mailController.setMailSearch}
          newMailThreadSubject={mailController.newMailThreadSubject}
          onNewMailThreadSubjectChange={mailController.setNewMailThreadSubject}
          newMailThreadParticipants={mailController.newMailThreadParticipants}
          onNewMailThreadParticipantsChange={mailController.setNewMailThreadParticipants}
          onCreateDirectThread={async () => mailController.createMailThread("direct")}
          mailThreads={mailController.mailThreads}
          selectedMailThreadId={mailController.selectedMailThreadId}
          onSelectMailThread={mailController.setSelectedMailThreadId}
          mailThreadDetail={mailController.mailThreadDetail}
          mailMessages={mailController.mailMessages}
          onAcknowledgeMessage={mailController.acknowledgeMessage}
          onDownloadAttachment={mailController.downloadMailAttachment}
          mailComposeSender={mailController.mailComposeSender}
          onMailComposeSenderChange={mailController.setMailComposeSender}
          mailComposeRecipients={mailController.mailComposeRecipients}
          onMailComposeRecipientsChange={mailController.setMailComposeRecipients}
          mailComposeBody={mailController.mailComposeBody}
          onMailComposeBodyChange={mailController.setMailComposeBody}
          mailAttachmentFiles={mailController.mailAttachmentFiles}
          onMailAttachmentFilesChange={mailController.setMailAttachmentFiles}
          onSendMessage={async () => {
            if (!mailController.selectedMailThreadId) {
              return;
            }
            await mailController.sendThreadMessage(mailController.selectedMailThreadId, {
              body: mailController.mailComposeBody,
              recipientsCsv: mailController.mailComposeRecipients,
              senderPrincipal: mailController.mailComposeSender,
              files: mailController.mailAttachmentFiles,
              context: "mail",
            });
          }}
          onSummarizeToNote={mailController.summarizeSelectedMailThread}
          leaseHolderPrincipal={mailController.leaseHolderPrincipal}
          onLeaseHolderPrincipalChange={mailController.setLeaseHolderPrincipal}
          leaseGlobPattern={mailController.leaseGlobPattern}
          onLeaseGlobPatternChange={mailController.setLeaseGlobPattern}
          leaseTtlMs={mailController.leaseTtlMs}
          onLeaseTtlMsChange={mailController.setLeaseTtlMs}
          leaseNote={mailController.leaseNote}
          onLeaseNoteChange={mailController.setLeaseNote}
          leaseExclusive={mailController.leaseExclusive}
          onLeaseExclusiveChange={mailController.setLeaseExclusive}
          onCreateFileLease={mailController.createFileLease}
          leases={mailController.leases}
          onReleaseFileLease={mailController.releaseFileLease}
        />
      ) : null}

      {activeTab === "chatrooms" ? (
        <ChatroomsPage
          onRefresh={() => mailController.queueAgentMailRefresh(settings)}
          newRoomName={mailController.newRoomName}
          onNewRoomNameChange={mailController.setNewRoomName}
          newRoomParticipants={mailController.newRoomParticipants}
          onNewRoomParticipantsChange={mailController.setNewRoomParticipants}
          onCreateRoom={async () => mailController.createMailThread("room")}
          roomThreads={mailController.roomThreads}
          selectedRoomThreadId={mailController.selectedRoomThreadId}
          onSelectRoomThread={mailController.setSelectedRoomThreadId}
          roomThreadDetail={mailController.roomThreadDetail}
          roomMessages={mailController.roomMessages}
          onPostRoomReaction={mailController.postRoomReaction}
          mailPrincipalOverride={mailController.mailPrincipalOverride}
          onAcknowledgeMessage={mailController.acknowledgeMessage}
          onDownloadAttachment={mailController.downloadMailAttachment}
          chatComposeSender={mailController.chatComposeSender}
          onChatComposeSenderChange={mailController.setChatComposeSender}
          chatComposeRecipients={mailController.chatComposeRecipients}
          onChatComposeRecipientsChange={mailController.setChatComposeRecipients}
          chatComposeBody={mailController.chatComposeBody}
          onChatComposeBodyChange={mailController.setChatComposeBody}
          chatAttachmentFiles={mailController.chatAttachmentFiles}
          onChatAttachmentFilesChange={mailController.setChatAttachmentFiles}
          onSendRoomMessage={async () => {
            if (!mailController.selectedRoomThreadId) {
              return;
            }
            await mailController.sendThreadMessage(mailController.selectedRoomThreadId, {
              body: mailController.chatComposeBody,
              recipientsCsv: mailController.chatComposeRecipients,
              senderPrincipal: mailController.chatComposeSender,
              files: mailController.chatAttachmentFiles,
              context: "chat",
            });
          }}
          onAcknowledgeRoomUnread={mailController.acknowledgeRoomUnread}
          onReserveSelectedRoomWorkspace={mailController.reserveSelectedRoomWorkspace}
          leases={mailController.leases}
          onReleaseFileLease={mailController.releaseFileLease}
        />
      ) : null}

      {activeTab === "cockpit" ? (
        <CockpitPage
          cockpitPages={cockpitPages}
          activeCockpitPage={activeCockpitPage}
          onSetActiveCockpitPageId={setActiveCockpitPageId}
          onRenameActiveCockpitPage={(name) =>
            setCockpitPages((previous) =>
              previous.map((page) =>
                page.page_id === activeCockpitPage.page_id
                  ? { ...page, name: name || "Custom Page" }
                  : page
              )
            )
          }
          onAddCockpitPage={addCockpitPage}
          onExportCockpitLayout={exportCockpitLayout}
          onImportCockpitLayout={async (file) => {
            try {
              await importCockpitLayout(file);
            } catch (error: unknown) {
              setNotice({
                tone: "error",
                message: `Cockpit import failed: ${String(error)}`,
              });
            }
          }}
          onResetCockpitLayout={resetCockpitLayout}
          onAddCockpitWidget={addCockpitWidget}
          onMoveCockpitWidget={moveCockpitWidget}
          onResizeCockpitWidget={resizeCockpitWidget}
          onRemoveCockpitWidget={removeCockpitWidget}
          renderCockpitWidget={renderCockpitWidget}
        />
      ) : null}
    </AppShell>
  );
}
