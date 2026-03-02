import { useState, type Dispatch, type SetStateAction } from "react";
import type { NotifyFn } from "./useAppController";
import { ChatroomsPage } from "../features/agentMail/ChatroomsPage";
import { MailPage } from "../features/agentMail/MailPage";
import { useAgentMailController } from "../features/agentMail/useAgentMailController";
import { BoardsPage } from "../features/boards/BoardsPage";
import { useBoardsController } from "../features/boards/useBoardsController";
import { CalendarPage } from "../features/calendar/CalendarPage";
import { CockpitPage } from "../features/cockpit/CockpitPage";
import { useCockpitController } from "../features/cockpit/useCockpitController";
import { type CockpitWidgetLayoutV2 } from "../features/cockpit/cockpitLayout";
import { CockpitWidgetRenderer } from "../features/cockpit/CockpitWidgetRenderer";
import { EventsPage } from "../features/events/EventsPage";
import { FocusPage } from "../features/focus/FocusPage";
import { TeamPage } from "../features/team/TeamPage";
import { useMissionControlController } from "./useMissionControlController";
import type { EventStreamItem, MissionControlTab } from "./useAppController";
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
  setNotice: NotifyFn;
}

function renderCockpitWidget(
  widget: CockpitWidgetLayoutV2,
  props: Omit<AppContentProps, "activeTab">
) {
  const { missionControl, cockpitController, agents, visibleEvents, settings } = props;
  return (
    <CockpitWidgetRenderer
      widget={widget}
      settings={settings}
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

/** Active tab is visible; inactive tabs are hidden via CSS. */
function TabPane({
  active,
  children,
}: {
  active: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="mc-tab-pane" style={{ display: active ? "contents" : "none" }}>
      {children}
    </div>
  );
}

export function AppContent(props: AppContentProps) {
  const active = props.activeTab;
  const [editMode, setEditMode] = useState(false);

  return (
    <>
      <TabPane active={active === "boards"}>
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
      </TabPane>

      <TabPane active={active === "calendar"}>
        <CalendarPage
          calendarWeek={props.missionControl.calendarWeek}
          calendarAlwaysRunning={props.missionControl.calendarAlwaysRunning}
          calendarNextUp={props.missionControl.calendarNextUp}
          calendarJobs={props.missionControl.calendarJobs}
          onRunCalendarJobNow={props.missionControl.runCalendarJobNow}
          onToggleCalendarJob={props.missionControl.toggleCalendarJob}
        />
      </TabPane>

      <TabPane active={active === "focus"}>
        <FocusPage
          focusItems={props.missionControl.focusItems}
          approvalsCount={props.missionControl.approvalsById.size}
          channelStatuses={props.missionControl.channelStatuses}
          onResolveFocusApproval={props.missionControl.resolveFocusApproval}
          onRunCalendarJobNow={props.missionControl.runCalendarJobNow}
          onReconnectFocusChannel={props.missionControl.reconnectFocusChannel}
        />
      </TabPane>

      <TabPane active={active === "events"}>
        <EventsPage
          showRawEvents={props.showRawEvents}
          onShowRawEventsChange={props.setShowRawEvents}
          visibleEvents={props.visibleEvents}
        />
      </TabPane>

      <TabPane active={active === "mail"}>
        <MailPage
          agents={props.agents}
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
      </TabPane>

      <TabPane active={active === "chatrooms"}>
        <ChatroomsPage
          agents={props.agents}
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
      </TabPane>

      <TabPane active={active === "team"}>
        <TeamPage
          agents={props.agents}
          activeJobCount={props.missionControl.calendarJobs.filter((j) => j.enabled).length}
          settings={props.settings}
          onRefresh={() => props.missionControl.queueMissionControlRefresh(props.settings)}
        />
      </TabPane>

      <TabPane active={active === "cockpit"}>
        <CockpitPage
          cockpitPages={props.cockpitController.cockpitPages}
          activeCockpitPage={props.cockpitController.activeCockpitPage}
          editMode={editMode}
          onSetEditMode={setEditMode}
          onSetActiveCockpitPageId={props.cockpitController.setActiveCockpitPageId}
          onRenameCockpitPage={props.cockpitController.renameCockpitPage}
          onAddCockpitPage={props.cockpitController.addCockpitPage}
          onDeleteCockpitPage={props.cockpitController.deleteCockpitPage}
          onDuplicateCockpitPage={props.cockpitController.duplicateCockpitPage}
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
          onLoadTemplate={props.cockpitController.loadTemplate}
          onAddCockpitWidget={props.cockpitController.addCockpitWidget}
          onAddCustomWidget={props.cockpitController.addCustomWidget}
          onRemoveCockpitWidget={props.cockpitController.removeCockpitWidget}
          onLayoutChange={props.cockpitController.handleLayoutChange}
          renderCockpitWidget={(widget) => renderCockpitWidget(widget, props)}
          settings={props.settings}
        />
      </TabPane>
    </>
  );
}
