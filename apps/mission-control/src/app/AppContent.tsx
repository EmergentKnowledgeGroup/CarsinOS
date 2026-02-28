import type { Dispatch, SetStateAction } from "react";
import { ChatroomsPage } from "../features/agentMail/ChatroomsPage";
import { MailPage } from "../features/agentMail/MailPage";
import { useAgentMailController } from "../features/agentMail/useAgentMailController";
import { BoardsPage } from "../features/boards/BoardsPage";
import { useBoardsController } from "../features/boards/useBoardsController";
import { CalendarPage } from "../features/calendar/CalendarPage";
import { CockpitPage } from "../features/cockpit/CockpitPage";
import { useCockpitController } from "../features/cockpit/useCockpitController";
import { type CockpitWidgetLayout } from "../features/cockpit/cockpitLayout";
import { CockpitWidgetRenderer } from "../features/cockpit/CockpitWidgetRenderer";
import { EventsPage } from "../features/events/EventsPage";
import { FocusPage } from "../features/focus/FocusPage";
import { useMissionControlController } from "./useMissionControlController";
import type { EventStreamItem, MissionControlTab, Notice } from "./useAppController";
import type { Agent, RuntimeConnectionSettings } from "../types";
import type { BoardSummary } from "./useRuntimeConnectionController";

interface AppContentProps {
  activeTab: MissionControlTab;
  settings: RuntimeConnectionSettings;
  boards: BoardSummary[];
  agents: Agent[];
  boardsController: ReturnType<typeof useBoardsController>;
  missionControl: ReturnType<typeof useMissionControlController>;
  mailController: ReturnType<typeof useAgentMailController>;
  cockpitController: ReturnType<typeof useCockpitController>;
  showRawEvents: boolean;
  setShowRawEvents: Dispatch<SetStateAction<boolean>>;
  visibleEvents: EventStreamItem[];
  setNotice: Dispatch<SetStateAction<Notice | null>>;
}

function renderCockpitWidget(
  widget: CockpitWidgetLayout,
  props: Omit<AppContentProps, "activeTab">
) {
  const { missionControl, cockpitController, agents, visibleEvents, settings } = props;
  return (
    <CockpitWidgetRenderer
      widget={widget}
      incidentMode={cockpitController.incidentMode}
      setIncidentMode={cockpitController.setIncidentMode}
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
      onRefreshAll={() => missionControl.queueMissionControlRefresh(settings)}
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
}

export function AppContent(props: AppContentProps) {
  if (props.activeTab === "boards") {
    return (
      <BoardsPage
        boards={props.boards}
        activeBoardId={props.boardsController.activeBoardId}
        onBoardChange={props.boardsController.handleBoardChange}
        columns={props.boardsController.columns}
        cardsByColumn={props.boardsController.cardsByColumn}
        selectedCardId={props.boardsController.selectedCardId}
        dragCardId={props.boardsController.dragCardId}
        setDragCardId={props.boardsController.setDragCardId}
        onSelectCard={props.boardsController.selectCard}
        onDropCard={props.boardsController.handleDropCard}
        onCreateCard={props.boardsController.handleCreateCard}
        selectedCard={props.boardsController.selectedCard}
        cardEditor={props.boardsController.cardEditor}
        setCardEditor={props.boardsController.setCardEditor}
        agents={props.agents}
        onSaveCardDraft={props.boardsController.saveCardDraft}
        onRunCard={props.boardsController.runCard}
        onUploadAsset={props.boardsController.uploadAsset}
        onPreviewAsset={props.boardsController.previewAsset}
        selectedPreviewUrl={props.boardsController.selectedPreviewUrl}
      />
    );
  }

  if (props.activeTab === "calendar") {
    return (
      <CalendarPage
        calendarWeek={props.missionControl.calendarWeek}
        calendarAlwaysRunning={props.missionControl.calendarAlwaysRunning}
        calendarNextUp={props.missionControl.calendarNextUp}
        calendarJobs={props.missionControl.calendarJobs}
        onRunCalendarJobNow={props.missionControl.runCalendarJobNow}
        onToggleCalendarJob={props.missionControl.toggleCalendarJob}
      />
    );
  }

  if (props.activeTab === "focus") {
    return (
      <FocusPage
        focusItems={props.missionControl.focusItems}
        approvalsCount={props.missionControl.approvalsById.size}
        channelStatuses={props.missionControl.channelStatuses}
        onResolveFocusApproval={props.missionControl.resolveFocusApproval}
        onRunCalendarJobNow={props.missionControl.runCalendarJobNow}
        onReconnectFocusChannel={props.missionControl.reconnectFocusChannel}
      />
    );
  }

  if (props.activeTab === "events") {
    return (
      <EventsPage
        showRawEvents={props.showRawEvents}
        onShowRawEventsChange={props.setShowRawEvents}
        visibleEvents={props.visibleEvents}
      />
    );
  }

  if (props.activeTab === "mail") {
    return (
      <MailPage
        onRefresh={() => props.mailController.queueAgentMailRefresh(props.settings)}
        mailboxFilter={props.mailController.mailboxFilter}
        onMailboxFilterChange={props.mailController.setMailboxFilter}
        mailPrincipalOverride={props.mailController.mailPrincipalOverride}
        onMailPrincipalOverrideChange={props.mailController.setMailPrincipalOverride}
        mailSearch={props.mailController.mailSearch}
        onMailSearchChange={props.mailController.setMailSearch}
        newMailThreadSubject={props.mailController.newMailThreadSubject}
        onNewMailThreadSubjectChange={props.mailController.setNewMailThreadSubject}
        newMailThreadParticipants={props.mailController.newMailThreadParticipants}
        onNewMailThreadParticipantsChange={props.mailController.setNewMailThreadParticipants}
        onCreateDirectThread={async () => props.mailController.createMailThread("direct")}
        mailThreads={props.mailController.mailThreads}
        selectedMailThreadId={props.mailController.selectedMailThreadId}
        onSelectMailThread={props.mailController.setSelectedMailThreadId}
        mailThreadDetail={props.mailController.mailThreadDetail}
        mailMessages={props.mailController.mailMessages}
        onAcknowledgeMessage={props.mailController.acknowledgeMessage}
        onDownloadAttachment={props.mailController.downloadMailAttachment}
        mailComposeSender={props.mailController.mailComposeSender}
        onMailComposeSenderChange={props.mailController.setMailComposeSender}
        mailComposeRecipients={props.mailController.mailComposeRecipients}
        onMailComposeRecipientsChange={props.mailController.setMailComposeRecipients}
        mailComposeBody={props.mailController.mailComposeBody}
        onMailComposeBodyChange={props.mailController.setMailComposeBody}
        mailAttachmentFiles={props.mailController.mailAttachmentFiles}
        onMailAttachmentFilesChange={props.mailController.setMailAttachmentFiles}
        onSendMessage={async () => {
          if (!props.mailController.selectedMailThreadId) {
            return;
          }
          await props.mailController.sendThreadMessage(props.mailController.selectedMailThreadId, {
            body: props.mailController.mailComposeBody,
            recipientsCsv: props.mailController.mailComposeRecipients,
            senderPrincipal: props.mailController.mailComposeSender,
            files: props.mailController.mailAttachmentFiles,
            context: "mail",
          });
        }}
        onSummarizeToNote={props.mailController.summarizeSelectedMailThread}
        leaseHolderPrincipal={props.mailController.leaseHolderPrincipal}
        onLeaseHolderPrincipalChange={props.mailController.setLeaseHolderPrincipal}
        leaseGlobPattern={props.mailController.leaseGlobPattern}
        onLeaseGlobPatternChange={props.mailController.setLeaseGlobPattern}
        leaseTtlMs={props.mailController.leaseTtlMs}
        onLeaseTtlMsChange={props.mailController.setLeaseTtlMs}
        leaseNote={props.mailController.leaseNote}
        onLeaseNoteChange={props.mailController.setLeaseNote}
        leaseExclusive={props.mailController.leaseExclusive}
        onLeaseExclusiveChange={props.mailController.setLeaseExclusive}
        onCreateFileLease={props.mailController.createFileLease}
        leases={props.mailController.leases}
        onReleaseFileLease={props.mailController.releaseFileLease}
      />
    );
  }

  if (props.activeTab === "chatrooms") {
    return (
      <ChatroomsPage
        onRefresh={() => props.mailController.queueAgentMailRefresh(props.settings)}
        newRoomName={props.mailController.newRoomName}
        onNewRoomNameChange={props.mailController.setNewRoomName}
        newRoomParticipants={props.mailController.newRoomParticipants}
        onNewRoomParticipantsChange={props.mailController.setNewRoomParticipants}
        onCreateRoom={async () => props.mailController.createMailThread("room")}
        roomThreads={props.mailController.roomThreads}
        selectedRoomThreadId={props.mailController.selectedRoomThreadId}
        onSelectRoomThread={props.mailController.setSelectedRoomThreadId}
        roomThreadDetail={props.mailController.roomThreadDetail}
        roomMessages={props.mailController.roomMessages}
        onPostRoomReaction={props.mailController.postRoomReaction}
        mailPrincipalOverride={props.mailController.mailPrincipalOverride}
        onAcknowledgeMessage={props.mailController.acknowledgeMessage}
        onDownloadAttachment={props.mailController.downloadMailAttachment}
        chatComposeSender={props.mailController.chatComposeSender}
        onChatComposeSenderChange={props.mailController.setChatComposeSender}
        chatComposeRecipients={props.mailController.chatComposeRecipients}
        onChatComposeRecipientsChange={props.mailController.setChatComposeRecipients}
        chatComposeBody={props.mailController.chatComposeBody}
        onChatComposeBodyChange={props.mailController.setChatComposeBody}
        chatAttachmentFiles={props.mailController.chatAttachmentFiles}
        onChatAttachmentFilesChange={props.mailController.setChatAttachmentFiles}
        onSendRoomMessage={async () => {
          if (!props.mailController.selectedRoomThreadId) {
            return;
          }
          await props.mailController.sendThreadMessage(props.mailController.selectedRoomThreadId, {
            body: props.mailController.chatComposeBody,
            recipientsCsv: props.mailController.chatComposeRecipients,
            senderPrincipal: props.mailController.chatComposeSender,
            files: props.mailController.chatAttachmentFiles,
            context: "chat",
          });
        }}
        onAcknowledgeRoomUnread={props.mailController.acknowledgeRoomUnread}
        onReserveSelectedRoomWorkspace={props.mailController.reserveSelectedRoomWorkspace}
        leases={props.mailController.leases}
        onReleaseFileLease={props.mailController.releaseFileLease}
      />
    );
  }

  return (
    <CockpitPage
      cockpitPages={props.cockpitController.cockpitPages}
      activeCockpitPage={props.cockpitController.activeCockpitPage}
      onSetActiveCockpitPageId={props.cockpitController.setActiveCockpitPageId}
      onRenameActiveCockpitPage={(name) =>
        props.cockpitController.setCockpitPages((previous) =>
          previous.map((page) =>
            page.page_id === props.cockpitController.activeCockpitPage.page_id
              ? { ...page, name: name || "Custom Page" }
              : page
          )
        )
      }
      onAddCockpitPage={props.cockpitController.addCockpitPage}
      onExportCockpitLayout={props.cockpitController.exportCockpitLayout}
      onImportCockpitLayout={async (file) => {
        try {
          await props.cockpitController.importCockpitLayout(file);
        } catch (error: unknown) {
          props.setNotice({
            tone: "error",
            message: `Cockpit import failed: ${String(error)}`,
          });
        }
      }}
      onResetCockpitLayout={props.cockpitController.resetCockpitLayout}
      onAddCockpitWidget={props.cockpitController.addCockpitWidget}
      onMoveCockpitWidget={props.cockpitController.moveCockpitWidget}
      onResizeCockpitWidget={props.cockpitController.resizeCockpitWidget}
      onRemoveCockpitWidget={props.cockpitController.removeCockpitWidget}
      renderCockpitWidget={(widget) => renderCockpitWidget(widget, props)}
    />
  );
}
