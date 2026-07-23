import {
  Fragment,
  useCallback,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react";
import type { NotifyFn } from "./useAppController";
import { ChatroomsPage } from "../features/agentMail/ChatroomsPage";
import { MailPage } from "../features/agentMail/MailPage";
import { useAgentMailController } from "../features/agentMail/useAgentMailController";
import { AssistantChatPage } from "../features/assistant/AssistantChatPage";
import type { ExecassOfficeController } from "../features/execassOffice/useExecassOfficeController";
import { GlassWindowPage } from "../features/glassWindow/GlassWindowPage";
import { presenceTargetDestination } from "../glass/window/presence";
import type { GlassWindowController } from "../features/glassWindow/useGlassWindowController";
import type { useAssistantChatController } from "../features/assistant/useAssistantChatController";
import { BoardsPage } from "../features/boards/BoardsPage";
import { useBoardsController } from "../features/boards/useBoardsController";
import { CalendarPage } from "../features/calendar/CalendarPage";
import { CockpitPage } from "../features/cockpit/CockpitPage";
import { useCockpitController } from "../features/cockpit/useCockpitController";
import { type CockpitWidgetLayoutV2 } from "../features/cockpit/cockpitLayout";
import { CockpitWidgetRenderer } from "../features/cockpit/CockpitWidgetRenderer";
import { ConnectorsPage } from "../features/connectors/ConnectorsPage";
import type { SimpleIntegrationId } from "../features/connectors/simpleIntegrations";
import { useConnectorsController } from "../features/connectors/useConnectorsController";
import { EventsPage } from "../features/events/EventsPage";
import { FocusPage } from "../features/focus/FocusPage";
import { TeamPage } from "../features/team/TeamPage";
import { HelpDocsPage } from "../features/help/HelpDocsPage";
import { MemoryPage } from "../features/memory/MemoryPage";
import { useMemoryController } from "../features/memory/useMemoryController";
import { RunbookPage } from "../features/runbook/RunbookPage";
import { useRunbookController } from "../features/runbook/useRunbookController";
import { StrategyPage } from "../features/strategy/StrategyPage";
import { useStrategyController } from "../features/strategy/useStrategyController";
import { useMissionControlController } from "./useMissionControlController";
import { TabHelpBanner, type HelpTab } from "./TabHelpBanner";
import type { EventStreamItem, MissionControlTab } from "./useAppController";
import type {
  Agent,
  MissionControlFocusItem,
  RuntimeConnectionSettings,
} from "../types";
import type { BoardSummary } from "./useRuntimeConnectionController";
import type { ErrorEventContext } from "../lib/errorRecovery";
import { AppErrorBoundary } from "../ui/AppErrorBoundary";

interface AppContentProps {
  activeTab: MissionControlTab;
  onTabChange: (tab: MissionControlTab) => void;
  /** Navigate by stable room id (pinned room shortcuts, elevator deep links). */
  onRoomSelect: (roomId: string) => void;
  onOpenHelpDocs: (section?: string) => void;
  helpDocsTarget: { section?: string; seq: number };
  onStartGuidedTour: () => void;
  onRefreshBaseline: () => Promise<void>;
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  boards: BoardSummary[];
  agents: Agent[];
  boardsController: ReturnType<typeof useBoardsController>;
  missionControl: ReturnType<typeof useMissionControlController>;
  mailController: ReturnType<typeof useAgentMailController>;
  assistantController: ReturnType<typeof useAssistantChatController>;
  officeController: ExecassOfficeController;
  glassWindowController: GlassWindowController;
  cockpitController: ReturnType<typeof useCockpitController>;
  strategyController: ReturnType<typeof useStrategyController>;
  runbookController: ReturnType<typeof useRunbookController>;
  memoryController: ReturnType<typeof useMemoryController>;
  connectorsController: ReturnType<typeof useConnectorsController>;
  showRawEvents: boolean;
  setShowRawEvents: Dispatch<SetStateAction<boolean>>;
  visibleEvents: EventStreamItem[];
  onResetTabState: (tab: MissionControlTab) => void;
  onEnterSafeMode: (reason: string) => void;
  tabResetVersion: Partial<Record<MissionControlTab, number>>;
  setNotice: NotifyFn;
  usageChartsEnabled: boolean;
  onOpenSimpleIntegrationWizard: (integrationId?: SimpleIntegrationId) => void;
  quickGuidesCollapsed: boolean;
  quickGuideOpenTab: HelpTab | null;
  onDismissQuickGuides: () => void;
}

function E2EForceCrashSentinel({
  tab,
  active,
  forceCrashToken,
}: {
  tab: MissionControlTab;
  active: boolean;
  forceCrashToken: string | null;
}) {
  if (!active || !forceCrashToken || !forceCrashToken.startsWith(`${tab}:`)) {
    return null;
  }
  throw new Error(`[e2e] forced tab crash: ${tab}`);
}

function renderCockpitWidget(
  widget: CockpitWidgetLayoutV2,
  props: Omit<AppContentProps, "activeTab">,
  strategyActions: {
    openTask: (taskId: string) => boolean;
    selectGoal: (goalId: string) => void;
    selectProject: (projectId: string) => void;
    openRunbook: (runbookKind: string, anchorId: string) => boolean;
  }
) {
  const {
    missionControl,
    cockpitController,
    agents,
    visibleEvents,
    settings,
    strategyController,
  } = props;
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
      usageChartsEnabled={props.usageChartsEnabled}
      usageToday={missionControl.usageToday}
      usageWeek={missionControl.usageWeek}
      usageUnavailableReason={missionControl.usageUnavailableReason}
      usageCorrelationAvailable={missionControl.usageCorrelationAvailable}
      usageFreshness={missionControl.usageFreshness}
      usageTrend={missionControl.usageTrend}
      usageBudgetWarnings={missionControl.usageBudgetWarnings}
      usageUpdatedAtUtc={missionControl.usageUpdatedAtUtc}
      strategyEnabled={strategyController.enabled}
      strategyAvailability={strategyController.availability}
      strategySummary={strategyController.summary}
      strategyGoals={strategyController.goals}
      strategyProjects={strategyController.projects}
      onOpenStrategyTask={strategyActions.openTask}
      onSelectStrategyGoal={strategyActions.selectGoal}
      onSelectStrategyProject={strategyActions.selectProject}
      runbookEnabled={props.runbookController.enabled}
      runbookAvailability={props.runbookController.availability}
      runbookCountsByStatus={props.runbookController.allCountsByStatus}
      runbookItems={props.runbookController.allItems}
      onOpenRunbook={strategyActions.openRunbook}
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

/** Active tab keeps its own flex column layout; inactive tabs are hidden. */
function TabPane({
  active,
  tab,
  children,
}: {
  active: boolean;
  tab: MissionControlTab;
  children: ReactNode;
}) {
  return (
    <div
      className="mc-tab-pane"
      data-active-tab={tab}
      style={{ display: active ? "flex" : "none" }}
    >
      {children}
    </div>
  );
}

function TabBoundaryPane({
  tab,
  active,
  resetVersion,
  forceCrashToken,
  title,
  subtitle,
  events,
  onResetTabState,
  onEnterSafeMode,
  children,
}: {
  tab: MissionControlTab;
  active: boolean;
  resetVersion: number;
  forceCrashToken: string | null;
  title: string;
  subtitle: string;
  events: readonly ErrorEventContext[];
  onResetTabState: (tab: MissionControlTab) => void;
  onEnterSafeMode: (reason: string) => void;
  children: ReactNode;
}) {
  return (
    <TabPane active={active} tab={tab}>
      <AppErrorBoundary
        scope="tab"
        title={title}
        subtitle={subtitle}
        events={events}
        onResetScope={() => onResetTabState(tab)}
        onEnterSafeMode={onEnterSafeMode}
      >
        <Fragment key={`${tab}-${resetVersion}`}>
          <E2EForceCrashSentinel
            tab={tab}
            active={active}
            forceCrashToken={forceCrashToken}
          />
          {children}
        </Fragment>
      </AppErrorBoundary>
    </TabPane>
  );
}

export function AppContent(props: AppContentProps) {
  const active = props.activeTab;
  const e2eMode =
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).has("e2e");
  const [forceCrashToken, setForceCrashToken] = useState<string | null>(null);
  const [editMode, setEditMode] = useState(false);
  const tabEvents = props.visibleEvents.slice(0, 10);
  const strategyReady =
    props.strategyController.enabled &&
    props.strategyController.availability === "ready";
  const runbookReady =
    props.runbookController.enabled &&
    props.runbookController.availability === "ready";
  const showQuickGuide = useCallback(
    (tab: HelpTab) =>
      !props.quickGuidesCollapsed || props.quickGuideOpenTab === tab,
    [props.quickGuideOpenTab, props.quickGuidesCollapsed]
  );
  const renderQuickGuide = useCallback(
    (tab: HelpTab) =>
      showQuickGuide(tab) ? (
        <TabHelpBanner
          tab={tab}
          onOpenDocs={props.onOpenHelpDocs}
          onStartTour={props.onStartGuidedTour}
          onDismissAll={props.onDismissQuickGuides}
        />
      ) : null,
    [
      props.onDismissQuickGuides,
      props.onOpenHelpDocs,
      props.onStartGuidedTour,
      showQuickGuide,
    ]
  );
  const openRunbook = (runbookKind: string, anchorId: string) => {
    const opened = props.runbookController.openRunbook(runbookKind, anchorId);
    if (opened) {
      props.onTabChange("runbook");
    }
    return opened;
  };
  const openStrategyTask = (taskId: string) => {
    const opened = props.strategyController.openTaskById(taskId);
    if (opened) {
      props.onTabChange("strategy");
    }
    return opened;
  };
  const selectStrategyGoal = (goalId: string) => {
    props.strategyController.setSelectedGoalId(goalId);
    props.onTabChange("strategy");
  };
  const selectStrategyProject = (projectId: string) => {
    props.strategyController.setSelectedProjectId(projectId);
    props.onTabChange("strategy");
  };
  const openAssistantContext = (
    targetKind: string,
    targetId: string,
    runId?: string | null
  ) => {
    props.onTabChange("assistant");
    if (targetKind === "session") {
      void props.assistantController.openSession(targetId, {
        runId: runId ?? null,
      });
      return;
    }
    if (targetKind === "run") {
      const linkedSession =
        props.runbookController
          .findFirstSummaryForEntity("run", targetId)
          ?.linked_entities.find((entity) => entity.entity_kind === "session")
          ?.entity_id ??
        props.runbookController.getRunSummary(targetId)?.linked_entities.find(
          (entity) => entity.entity_kind === "session"
        )?.entity_id;
      if (linkedSession) {
        void props.assistantController.openSession(linkedSession, { runId: targetId });
      }
    }
  };
  const openAssistantAgent = (agentId: string) => {
    props.assistantController.setSelectedAgentId(agentId);
    props.onTabChange("assistant");
  };
  const getFocusRunbook = (item: MissionControlFocusItem) => {
    const approvalId = String(item.action_payload.approval_id ?? "").trim();
    const taskId = String(item.action_payload.task_id ?? "").trim();
    const jobId = String(item.action_payload.job_id ?? "").trim();
    const runId = String(item.action_payload.run_id ?? "").trim();
    if (approvalId) {
      return props.runbookController.getApprovalSummary(approvalId);
    }
    if (taskId) {
      return props.runbookController.getTaskSummary(taskId);
    }
    if (jobId) {
      return props.runbookController.getJobSummary(jobId);
    }
    if (runId) {
      return props.runbookController.getRunSummary(runId);
    }
    return null;
  };
  const openFocusRunbook = (item: MissionControlFocusItem) => {
    const summary = getFocusRunbook(item);
    if (summary) {
      return openRunbook(summary.runbook_kind, summary.anchor_id);
    }
    const taskId = String(item.action_payload.task_id ?? "").trim();
    if (taskId) {
      return openRunbook("strategy_task_execution", taskId);
    }
    const jobId = String(item.action_payload.job_id ?? "").trim();
    if (jobId) {
      return openRunbook("scheduled_job_run", jobId);
    }
    const runId = String(item.action_payload.run_id ?? "").trim();
    if (runId) {
      return openRunbook("assistant_session_run", runId);
    }
    return false;
  };
  const openRunbookDeepLink = (target: {
    tab: string;
    target_kind: string;
    target_id: string | null;
    context: string | null;
  }) => {
    if (target.tab === "strategy" && target.target_id) {
      if (target.target_kind === "task") {
        openStrategyTask(target.target_id);
        return;
      }
      if (target.target_kind === "goal") {
        selectStrategyGoal(target.target_id);
        return;
      }
      if (target.target_kind === "project") {
        selectStrategyProject(target.target_id);
        return;
      }
    }
    if (target.tab === "runbook" && target.target_id) {
      const [runbookKind, anchorId] = target.target_id.split(":", 2);
      if (runbookKind && anchorId) {
        openRunbook(runbookKind, anchorId);
        return;
      }
    }
    if (target.tab === "assistant" && target.target_id) {
      openAssistantContext(
        target.target_kind,
        target.target_id,
        target.context?.trim() || null
      );
      return;
    }
    if (
      target.tab === "boards" &&
      target.target_id &&
      (target.target_kind === "card" || target.target_kind === "board_card")
    ) {
      props.boardsController.selectCard(target.target_id);
      props.onTabChange("boards");
      return;
    }
    if (target.tab === "focus" && target.target_id) {
      props.onTabChange("focus");
      return;
    }
    props.onTabChange(target.tab as MissionControlTab);
  };

  return (
    <>
      {e2eMode ? (
        <button
          type="button"
          data-testid="e2e-crash-active-tab"
          aria-label="Force crash active tab"
          onClick={() => {
            const token = `${active}:${Date.now()}`;
            setForceCrashToken(token);
            window.setTimeout(() => {
              setForceCrashToken((current) => (current === token ? null : current));
            }, 0);
          }}
          style={{
            top: 8,
            fontSize: 10,
            minHeight: 28,
            opacity: 0.2,
            padding: "2px 6px",
            position: "fixed",
            left: "50%",
            zIndex: 9999,
          }}
        >
          Force Crash
        </button>
      ) : null}

      <TabBoundaryPane
        tab="boards"
        active={active === "boards"}
        resetVersion={props.tabResetVersion.boards ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Boards ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("boards")}
        <BoardsPage
          boards={props.boards}
          activeBoardId={props.boardsController.activeBoardId}
          loading={props.boardsController.loading}
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
          onMoveCardToColumn={props.boardsController.moveSelectedCardToColumn}
          onUploadAsset={props.boardsController.uploadAsset}
          onPreviewAsset={props.boardsController.previewAsset}
          selectedPreviewUrl={props.boardsController.selectedPreviewUrl}
          editorBusy={props.boardsController.editorBusy}
          editorBusyAction={props.boardsController.editorBusyAction}
          strategyReady={strategyReady}
          linkedTaskByCardId={props.strategyController.taskByBoardCardId}
          describeStrategyTask={props.strategyController.describeTaskContext}
          onOpenStrategyTask={openStrategyTask}
          runbookEnabled={runbookReady}
          runbookByCardId={props.runbookController.summaryIndex.byBoardCardId}
          onOpenBoardCardRunbook={(cardId) => openRunbook("board_card_run", cardId)}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="calendar"
        active={active === "calendar"}
        resetVersion={props.tabResetVersion.calendar ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Calendar ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("calendar")}
        <CalendarPage
          calendarWeek={props.missionControl.calendarWeek}
          calendarAlwaysRunning={props.missionControl.calendarAlwaysRunning}
          calendarNextUp={props.missionControl.calendarNextUp}
          calendarJobs={props.missionControl.calendarJobs}
          agents={props.agents}
          execAssAgentId={props.missionControl.selectedProviderControlAgentId}
          onRunCalendarJobNow={props.missionControl.runCalendarJobNow}
          onToggleCalendarJob={props.missionControl.toggleCalendarJob}
          onLoadCalendarJobHistory={props.missionControl.loadCalendarJobHistory}
          onCreateExecAssHeartbeatJob={props.missionControl.createExecAssHeartbeatJob}
          strategyReady={strategyReady}
          taskByJobId={props.strategyController.taskByJobId}
          describeStrategyTask={props.strategyController.describeTaskContext}
          onOpenStrategyTask={openStrategyTask}
          runbookEnabled={runbookReady}
          runbookByJobId={props.runbookController.summaryIndex.byJobId}
          onOpenJobRunbook={(jobId) => openRunbook("scheduled_job_run", jobId)}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="focus"
        active={active === "focus"}
        resetVersion={props.tabResetVersion.focus ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Focus ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("focus")}
        <FocusPage
          focusItems={props.missionControl.focusItems}
          approvalsCount={props.missionControl.approvalsById.size}
          channelStatuses={props.missionControl.channelStatuses}
          onResolveFocusApproval={props.missionControl.resolveFocusApproval}
          onRunCalendarJobNow={props.missionControl.runCalendarJobNow}
          onReconnectFocusChannel={props.missionControl.reconnectFocusChannel}
          strategyReady={strategyReady}
          approvalTaskByApprovalId={props.strategyController.approvalTaskByApprovalId}
          taskById={props.strategyController.taskById}
          taskByJobId={props.strategyController.taskByJobId}
          describeStrategyTask={props.strategyController.describeTaskContext}
          onOpenStrategyTask={openStrategyTask}
          runbookEnabled={runbookReady}
          getRunbookForFocusItem={getFocusRunbook}
          onOpenRunbookForFocusItem={openFocusRunbook}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="events"
        active={active === "events"}
        resetVersion={props.tabResetVersion.events ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Events ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("events")}
        <EventsPage
          showRawEvents={props.showRawEvents}
          onShowRawEventsChange={props.setShowRawEvents}
          visibleEvents={props.visibleEvents}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="mail"
        active={active === "mail"}
        resetVersion={props.tabResetVersion.mail ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Mail ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("mail")}
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
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="chatrooms"
        active={active === "chatrooms"}
        resetVersion={props.tabResetVersion.chatrooms ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Chatrooms ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("chatrooms")}
        <ChatroomsPage
          agents={props.agents}
          onRefresh={() => props.mailController.queueAgentMailRefresh(props.settings)}
          mailboxFilter={props.mailController.mailboxFilter}
          mailSearch={props.mailController.mailSearch}
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
          onMailboxFilterChange={props.mailController.setMailboxFilter}
          onMailPrincipalOverrideChange={props.mailController.setMailPrincipalOverride}
          onMailSearchChange={props.mailController.setMailSearch}
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
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="assistant"
        active={active === "assistant"}
        resetVersion={props.tabResetVersion.assistant ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Assistant ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("assistant")}
        <AssistantChatPage
          active={active === "assistant"}
          settings={props.settings}
          tokenConfigured={props.tokenConfigured}
          agents={props.agents}
          boards={props.boards}
          onTabChange={props.onTabChange}
          onOpenRoom={props.onRoomSelect}
          controller={props.assistantController}
          officeController={props.officeController}
          runbookEnabled={runbookReady}
          runbookSummary={
            props.assistantController.lastRunId
              ? props.runbookController.getRunSummary(props.assistantController.lastRunId)
              : props.assistantController.sessionId
                ? props.runbookController.getSessionSummary(
                    props.assistantController.sessionId
                  )
                : null
          }
          onOpenAssistantRunbook={(runId) => openRunbook("assistant_session_run", runId)}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="window"
        active={active === "window"}
        resetVersion={props.tabResetVersion.window ?? 0}
        forceCrashToken={forceCrashToken}
        title="The Window crashed."
        subtitle="Reef and Office Chatter ran into an unexpected runtime error."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("window")}
        <GlassWindowPage
          controller={props.glassWindowController}
          onOpenTarget={(target) => {
            const destination = presenceTargetDestination(target);
            if (!destination) return false;
            switch (destination.kind) {
              case "runbook":
                return openRunbook(
                  destination.runbookKind,
                  destination.anchorId,
                );
              case "session":
                openAssistantContext("session", destination.sessionId);
                return true;
              case "office":
                // The current Office has no programmatic delegation-detail
                // selection seam. Navigate honestly without claiming the
                // specific delegation was opened.
                props.onTabChange("assistant");
                return true;
            }
          }}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="team"
        active={active === "team"}
        resetVersion={props.tabResetVersion.team ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Team ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("team")}
        <TeamPage
          agents={props.agents}
          activeJobCount={props.missionControl.calendarJobs.filter((j) => j.enabled).length}
          settings={props.settings}
          strategyController={props.strategyController}
          onRefresh={props.onRefreshBaseline}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="cockpit"
        active={active === "cockpit"}
        resetVersion={props.tabResetVersion.cockpit ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Cockpit ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("cockpit")}
        <CockpitPage
          isActive={active === "cockpit"}
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
          onAutoFitCockpitLayout={props.cockpitController.autoFitActivePage}
          onNudgeCockpitWidget={props.cockpitController.nudgeCockpitWidget}
          onLayoutChange={props.cockpitController.handleLayoutChange}
          renderCockpitWidget={(widget) =>
            renderCockpitWidget(widget, props, {
              openTask: openStrategyTask,
              selectGoal: selectStrategyGoal,
              selectProject: selectStrategyProject,
              openRunbook,
            })
          }
          settings={props.settings}
          strategyEnabled={props.strategyController.enabled}
          runbookEnabled={props.runbookController.enabled}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="strategy"
        active={active === "strategy"}
        resetVersion={props.tabResetVersion.strategy ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Strategy ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("strategy")}
        <StrategyPage
          controller={props.strategyController}
          agents={props.agents}
          runbookEnabled={runbookReady}
          selectedTaskRunbook={
            props.strategyController.selectedTask
              ? props.runbookController.getTaskSummary(
                  props.strategyController.selectedTask.task_id
                )
              : null
          }
          onOpenTaskRunbook={(taskId) => openRunbook("strategy_task_execution", taskId)}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="runbook"
        active={active === "runbook"}
        resetVersion={props.tabResetVersion.runbook ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Runbook ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("runbook")}
        <RunbookPage
          key={props.runbookController.openRequestVersion}
          controller={props.runbookController}
          agents={props.agents}
          onOpenDeepLink={openRunbookDeepLink}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="memory"
        active={active === "memory"}
        resetVersion={props.tabResetVersion.memory ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Memory ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("memory")}
        <MemoryPage
          controller={props.memoryController}
          onOpenAssistant={openAssistantAgent}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="connectors"
        active={active === "connectors"}
        resetVersion={props.tabResetVersion.connectors ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Connectors ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        {renderQuickGuide("connectors")}
        <ConnectorsPage
          controller={props.connectorsController}
          onOpenSimpleIntegrationWizard={props.onOpenSimpleIntegrationWizard}
        />
      </TabBoundaryPane>

      <TabBoundaryPane
        tab="help"
        active={active === "help"}
        resetVersion={props.tabResetVersion.help ?? 0}
        forceCrashToken={forceCrashToken}
        title="This tab crashed."
        subtitle="Help/Docs ran into an unexpected runtime error. Retry, reset this tab, or reload."
        events={tabEvents}
        onResetTabState={props.onResetTabState}
        onEnterSafeMode={props.onEnterSafeMode}
      >
        <HelpDocsPage onOpenTab={props.onTabChange} onStartTour={props.onStartGuidedTour} targetSection={props.helpDocsTarget.section} targetSeq={props.helpDocsTarget.seq} />
      </TabBoundaryPane>
    </>
  );
}
