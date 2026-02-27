import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import clsx from "clsx";
import {
  createBoardCard,
  fetchBoardCardAssetBlob,
  getAgentProviderProfileOrder,
  getChannelRuntimeStatus,
  getBoard,
  getMissionControlCalendarWeek,
  getMissionControlFocus,
  getGatewayHealth,
  getGatewayStatus,
  getJobsStatus,
  listAuthProfiles,
  listAgents,
  listBoards,
  listJobs,
  listPluginRuntimeStatus,
  listPlugins,
  listSkills,
  listApprovals,
  moveBoardCard,
  reconnectChannelRuntime,
  resolveApproval,
  runJobNow,
  runBoardCard,
  setJobEnabledState,
  setPluginEnabled,
  setSkillEnabled,
  setAgentProviderProfileOrder,
  updateBoardCard,
  uploadBoardCardAsset,
} from "./lib/api";
import {
  clearGatewayToken,
  isGatewayTokenConfigured,
  loadConnectionSettings,
  persistConnectionSettings,
  setGatewayToken,
} from "./lib/runtime";
import { connectGatewayEvents, type WsLifecycleState } from "./lib/ws";
import type {
  Agent,
  AuthProfileResponse,
  BoardCard,
  BoardColumn,
  BoardDetail,
  CircuitBreakerStateResponse,
  ChannelRuntimeAdapterStatusResponse,
  JobStatusResponse,
  MissionControlCalendarJob,
  MissionControlCalendarWeekResponse,
  MissionControlFocusItem,
  PluginManifestResponse,
  PluginRuntimeStatusResponse,
  RuntimeConnectionSettings,
  SkillResponse,
  StatusResponse,
  WsEventFrame,
} from "./types";
import "./styles.css";

interface Notice {
  tone: "info" | "error" | "critical";
  message: string;
}

type MissionControlTab = "boards" | "calendar" | "focus" | "events" | "cockpit";

interface EventStreamItem {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  payload: Record<string, unknown>;
}

interface CardEditorDraft {
  title: string;
  description: string;
  ownerKind: string;
  ownerAgentId: string;
  ownerHumanId: string;
  dueAt: string;
  tagsCsv: string;
  scriptMarkdown: string;
}

type CockpitWidgetKind =
  | "health"
  | "focus"
  | "breakers"
  | "jobs"
  | "channels"
  | "profiles"
  | "skills"
  | "plugins"
  | "events";

interface CockpitWidgetLayout {
  instance_id: string;
  widget: CockpitWidgetKind;
  title: string;
  span: number;
}

interface CockpitPageLayout {
  page_id: string;
  name: string;
  widgets: CockpitWidgetLayout[];
}

const COCKPIT_LAYOUT_STORAGE_KEY = "mission_control.cockpit.pages.v1";

function defaultCockpitPages(): CockpitPageLayout[] {
  return [
    {
      page_id: "ops-default",
      name: "Ops Default",
      widgets: [
        {
          instance_id: "health-default",
          widget: "health",
          title: "Pinned Health Strip",
          span: 4,
        },
        {
          instance_id: "focus-default",
          widget: "focus",
          title: "Incident Queue",
          span: 2,
        },
        {
          instance_id: "breakers-default",
          widget: "breakers",
          title: "Breaker Radar",
          span: 2,
        },
        {
          instance_id: "jobs-default",
          widget: "jobs",
          title: "Scheduler Matrix",
          span: 2,
        },
        {
          instance_id: "channels-default",
          widget: "channels",
          title: "Channel Control",
          span: 2,
        },
        {
          instance_id: "profiles-default",
          widget: "profiles",
          title: "Agent Provider Routing",
          span: 3,
        },
        {
          instance_id: "skills-default",
          widget: "skills",
          title: "Skills Control",
          span: 3,
        },
        {
          instance_id: "plugins-default",
          widget: "plugins",
          title: "Plugins Control",
          span: 3,
        },
        {
          instance_id: "events-default",
          widget: "events",
          title: "Event Tail",
          span: 3,
        },
      ],
    },
  ];
}

function normalizeWidgetSpan(span: number): number {
  return Math.max(1, Math.min(4, Math.round(span)));
}

function sanitizeCockpitPages(input: unknown): CockpitPageLayout[] {
  if (!Array.isArray(input)) {
    return defaultCockpitPages();
  }
  const pages = input
    .map((item) => {
      const raw = item as Partial<CockpitPageLayout>;
      if (typeof raw.page_id !== "string" || !raw.page_id.trim()) {
        return null;
      }
      const pageName =
        typeof raw.name === "string" && raw.name.trim()
          ? raw.name.trim()
          : "Custom Page";
      const widgets = Array.isArray(raw.widgets)
        ? raw.widgets
            .map((widget) => {
              const entry = widget as Partial<CockpitWidgetLayout>;
              if (
                typeof entry.instance_id !== "string" ||
                !entry.instance_id.trim() ||
                typeof entry.widget !== "string" ||
                typeof entry.title !== "string"
              ) {
                return null;
              }
              return {
                instance_id: entry.instance_id.trim(),
                widget: entry.widget as CockpitWidgetKind,
                title: entry.title.trim() || "Widget",
                span: normalizeWidgetSpan(Number(entry.span ?? 2)),
              } satisfies CockpitWidgetLayout;
            })
            .filter((widget): widget is CockpitWidgetLayout => widget !== null)
        : [];
      return {
        page_id: raw.page_id.trim(),
        name: pageName,
        widgets: widgets.length > 0 ? widgets : defaultCockpitPages()[0].widgets,
      } satisfies CockpitPageLayout;
    })
    .filter((page): page is CockpitPageLayout => page !== null);
  return pages.length > 0 ? pages : defaultCockpitPages();
}

function loadCockpitPagesFromStorage(): CockpitPageLayout[] {
  if (typeof window === "undefined") {
    return defaultCockpitPages();
  }
  const raw = window.localStorage.getItem(COCKPIT_LAYOUT_STORAGE_KEY);
  if (!raw) {
    return defaultCockpitPages();
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    return sanitizeCockpitPages(parsed);
  } catch {
    return defaultCockpitPages();
  }
}

function persistCockpitPagesToStorage(pages: CockpitPageLayout[]): void {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(COCKPIT_LAYOUT_STORAGE_KEY, JSON.stringify(pages));
}

const COCKPIT_WIDGET_PALETTE: Array<{
  widget: CockpitWidgetKind;
  title: string;
  description: string;
  defaultSpan: number;
}> = [
  {
    widget: "health",
    title: "Pinned Health Strip",
    description: "Gateway status, approvals, channels, and scheduler safety posture.",
    defaultSpan: 4,
  },
  {
    widget: "focus",
    title: "Focus Queue",
    description: "Operator attention queue with approvals, failures, and incident actions.",
    defaultSpan: 2,
  },
  {
    widget: "breakers",
    title: "Breaker Radar",
    description: "Circuit breaker and plugin breaker state with cooldown windows.",
    defaultSpan: 2,
  },
  {
    widget: "jobs",
    title: "Scheduler Matrix",
    description: "Upcoming jobs and direct run/pause controls.",
    defaultSpan: 2,
  },
  {
    widget: "channels",
    title: "Channel Ops",
    description: "Adapter health and one-click reconnect operations.",
    defaultSpan: 2,
  },
  {
    widget: "profiles",
    title: "Agent Routing",
    description: "Edit per-agent provider profile order without shell access.",
    defaultSpan: 3,
  },
  {
    widget: "skills",
    title: "Skills",
    description: "Toggle skills and inspect source paths/status.",
    defaultSpan: 3,
  },
  {
    widget: "plugins",
    title: "Plugins",
    description: "Inspect plugin runtime health and enable/disable safely.",
    defaultSpan: 3,
  },
  {
    widget: "events",
    title: "Event Tail",
    description: "Live operational event stream with noise control.",
    defaultSpan: 3,
  },
];

function emptyEditorDraft(): CardEditorDraft {
  return {
    title: "",
    description: "",
    ownerKind: "unassigned",
    ownerAgentId: "",
    ownerHumanId: "",
    dueAt: "",
    tagsCsv: "",
    scriptMarkdown: "",
  };
}

function toCardsByColumn(board: BoardDetail | null): Map<string, BoardCard[]> {
  const map = new Map<string, BoardCard[]>();
  if (!board) {
    return map;
  }
  for (const column of board.columns) {
    map.set(column.column_id, []);
  }
  for (const card of board.cards) {
    if (!map.has(card.column_id)) {
      map.set(card.column_id, []);
    }
    map.get(card.column_id)?.push(card);
  }
  for (const list of map.values()) {
    list.sort((a, b) => a.position - b.position);
  }
  return map;
}

function withUpsertCard(board: BoardDetail, nextCard: BoardCard): BoardDetail {
  const cards = board.cards.filter((card) => card.card_id !== nextCard.card_id);
  cards.push(nextCard);
  cards.sort((a, b) => a.position - b.position);
  return {
    ...board,
    cards,
  };
}

function withOptimisticMove(
  board: BoardDetail,
  cardId: string,
  targetColumnId: string,
  beforeCardId?: string
): BoardDetail {
  const columns = board.columns.map((column) => column.column_id);
  const grouped = toCardsByColumn(board);
  const movingCard = board.cards.find((card) => card.card_id === cardId);
  if (!movingCard) {
    return board;
  }

  for (const list of grouped.values()) {
    const index = list.findIndex((card) => card.card_id === cardId);
    if (index >= 0) {
      list.splice(index, 1);
      break;
    }
  }

  const targetList = grouped.get(targetColumnId) ?? [];
  const beforeIndex =
    beforeCardId === undefined
      ? -1
      : targetList.findIndex((card) => card.card_id === beforeCardId);
  const insertIndex =
    beforeCardId === undefined || beforeIndex < 0 ? targetList.length : beforeIndex;

  const nextCard: BoardCard = {
    ...movingCard,
    column_id: targetColumnId,
  };
  if (insertIndex >= targetList.length) {
    targetList.push(nextCard);
  } else {
    targetList.splice(insertIndex, 0, nextCard);
  }
  grouped.set(targetColumnId, targetList);

  const nextCards: BoardCard[] = [];
  for (const columnId of columns) {
    const list = grouped.get(columnId) ?? [];
    list.forEach((card, idx) => {
      nextCards.push({
        ...card,
        position: idx,
      });
    });
  }

  return {
    ...board,
    cards: nextCards,
  };
}

function toInputDateTimeValue(unixMs: number | null): string {
  if (unixMs === null || unixMs === undefined) {
    return "";
  }
  const date = new Date(unixMs);
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

function fromInputDateTimeValue(value: string): number | null {
  if (!value.trim()) {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? null : parsed;
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("failed to read file"));
        return;
      }
      const marker = "base64,";
      const index = result.indexOf(marker);
      if (index < 0) {
        reject(new Error("unexpected file encoding"));
        return;
      }
      resolve(result.slice(index + marker.length));
    };
    reader.onerror = () => reject(new Error("failed to read file"));
    reader.readAsDataURL(file);
  });
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes}B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)}KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
}

function formatDateTime(unixMs: number | null | undefined): string {
  if (!unixMs) {
    return "n/a";
  }
  return new Date(unixMs).toLocaleString();
}

function BoardLane(props: {
  column: BoardColumn;
  cards: BoardCard[];
  selectedCardId: string | null;
  dragCardId: string | null;
  setDragCardId: (value: string | null) => void;
  onSelectCard: (cardId: string) => void;
  onDropCard: (cardId: string, columnId: string, beforeCardId?: string) => void;
  onCreateCard: (columnId: string, title: string) => Promise<void>;
}) {
  const [newCardTitle, setNewCardTitle] = useState("");
  const listRef = useRef<HTMLDivElement | null>(null);

  const cardVirtualizer = useVirtualizer({
    count: props.cards.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => 132,
    overscan: 5,
  });

  const submitCreate = async () => {
    const title = newCardTitle.trim();
    if (!title) {
      return;
    }
    await props.onCreateCard(props.column.column_id, title);
    setNewCardTitle("");
  };

  return (
    <section className="mc-lane">
      <header className="mc-lane-header">
        <h3>{props.column.name}</h3>
        <span>{props.cards.length}</span>
      </header>

      <div
        className="mc-lane-body"
        ref={listRef}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault();
          const cardId = event.dataTransfer.getData("text/plain") || props.dragCardId;
          if (!cardId) {
            return;
          }
          props.onDropCard(cardId, props.column.column_id);
          props.setDragCardId(null);
        }}
      >
        <div
          style={{
            height: `${cardVirtualizer.getTotalSize()}px`,
            position: "relative",
          }}
        >
          {cardVirtualizer.getVirtualItems().map((virtualRow) => {
            const card = props.cards[virtualRow.index];
            return (
              <article
                key={card.card_id}
                className={clsx("mc-card", {
                  "mc-card-selected": props.selectedCardId === card.card_id,
                })}
                style={{
                  transform: `translateY(${virtualRow.start}px)`,
                  height: `${virtualRow.size}px`,
                  position: "absolute",
                  width: "100%",
                }}
                draggable
                onClick={() => props.onSelectCard(card.card_id)}
                onDragStart={(event) => {
                  props.setDragCardId(card.card_id);
                  event.dataTransfer.setData("text/plain", card.card_id);
                  event.dataTransfer.effectAllowed = "move";
                }}
                onDragEnd={() => props.setDragCardId(null)}
                onDragOver={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  const cardId =
                    event.dataTransfer.getData("text/plain") || props.dragCardId;
                  if (!cardId || cardId === card.card_id) {
                    return;
                  }
                  props.onDropCard(cardId, props.column.column_id, card.card_id);
                  props.setDragCardId(null);
                }}
              >
                <div className="mc-card-title">{card.title}</div>
                <div className="mc-card-meta">
                  <span>{card.owner_kind}</span>
                  {card.latest_run_id ? <span>run: {card.latest_run_id}</span> : null}
                </div>
              </article>
            );
          })}
        </div>
      </div>

      <div className="mc-lane-create">
        <input
          value={newCardTitle}
          onChange={(event) => setNewCardTitle(event.target.value)}
          placeholder="Add card"
        />
        <button type="button" onClick={submitCreate}>
          Add
        </button>
      </div>
    </section>
  );
}

export default function App() {
  const [activeTab, setActiveTab] = useState<MissionControlTab>("boards");
  const [settings, setSettings] = useState<RuntimeConnectionSettings>(
    loadConnectionSettings()
  );
  const [gatewayDraft, setGatewayDraft] = useState(settings.gateway_url);
  const [tokenDraft, setTokenDraft] = useState("");
  const [tokenConfigured, setTokenConfigured] = useState(false);

  const [healthState, setHealthState] = useState("idle");
  const [wsState, setWsState] = useState<WsLifecycleState>("idle");
  const [notice, setNotice] = useState<Notice | null>(null);
  const [eventStream, setEventStream] = useState<EventStreamItem[]>([]);
  const [showRawEvents, setShowRawEvents] = useState(false);

  const [boards, setBoards] = useState<{ board_id: string; name: string }[]>([]);
  const [activeBoardId, setActiveBoardId] = useState<string | null>(null);
  const [board, setBoard] = useState<BoardDetail | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [calendarWeek, setCalendarWeek] = useState<MissionControlCalendarWeekResponse | null>(
    null
  );
  const [focusItems, setFocusItems] = useState<MissionControlFocusItem[]>([]);
  const [channelStatuses, setChannelStatuses] = useState<ChannelRuntimeAdapterStatusResponse[]>(
    []
  );
  const [jobsById, setJobsById] = useState<Map<string, MissionControlCalendarJob>>(new Map());
  const [approvalsById, setApprovalsById] = useState<Set<string>>(new Set());
  const [gatewayStatus, setGatewayStatus] = useState<StatusResponse | null>(null);
  const [jobsStatus, setJobsStatus] = useState<JobStatusResponse | null>(null);
  const [authProfiles, setAuthProfiles] = useState<AuthProfileResponse[]>([]);
  const [skills, setSkills] = useState<SkillResponse[]>([]);
  const [plugins, setPlugins] = useState<PluginManifestResponse[]>([]);
  const [pluginRuntimeById, setPluginRuntimeById] = useState<
    Map<string, PluginRuntimeStatusResponse>
  >(new Map());
  const [incidentMode, setIncidentMode] = useState(false);
  const [cockpitPages, setCockpitPages] = useState<CockpitPageLayout[]>(
    loadCockpitPagesFromStorage()
  );
  const [activeCockpitPageId, setActiveCockpitPageId] = useState(
    loadCockpitPagesFromStorage()[0]?.page_id ?? "ops-default"
  );
  const [selectedProviderControlAgentId, setSelectedProviderControlAgentId] = useState("");
  const [selectedProviderControlProvider, setSelectedProviderControlProvider] = useState("");
  const [providerProfileOrder, setProviderProfileOrder] = useState<string[]>([]);
  const [providerProfileOrderDirty, setProviderProfileOrderDirty] = useState(false);

  const [selectedCardId, setSelectedCardId] = useState<string | null>(null);
  const [cardEditor, setCardEditor] = useState<CardEditorDraft>(emptyEditorDraft());
  const [selectedPreviewUrl, setSelectedPreviewUrl] = useState<string | null>(null);
  const [dragCardId, setDragCardId] = useState<string | null>(null);

  const boardRefreshTimer = useRef<number | null>(null);
  const missionControlRefreshTimer = useRef<number | null>(null);

  const cardsByColumn = useMemo(() => toCardsByColumn(board), [board]);

  const selectedCard = useMemo(() => {
    if (!board || !selectedCardId) {
      return null;
    }
    return board.cards.find((card) => card.card_id === selectedCardId) ?? null;
  }, [board, selectedCardId]);

  const activeCockpitPage = useMemo(() => {
    return (
      cockpitPages.find((page) => page.page_id === activeCockpitPageId) ??
      cockpitPages[0] ??
      defaultCockpitPages()[0]
    );
  }, [activeCockpitPageId, cockpitPages]);

  const providerOptions = useMemo(() => {
    return [...new Set(authProfiles.map((profile) => profile.provider))].sort((a, b) =>
      a.localeCompare(b)
    );
  }, [authProfiles]);

  const providerProfiles = useMemo(() => {
    if (!selectedProviderControlProvider) {
      return [] as AuthProfileResponse[];
    }
    return authProfiles
      .filter((profile) => profile.provider === selectedProviderControlProvider)
      .sort((left, right) => left.display_name.localeCompare(right.display_name));
  }, [authProfiles, selectedProviderControlProvider]);

  const orderedProviderProfiles = useMemo(() => {
    if (providerProfiles.length === 0) {
      return [] as AuthProfileResponse[];
    }
    const byId = new Map(providerProfiles.map((profile) => [profile.auth_profile_id, profile]));
    const ordered: AuthProfileResponse[] = [];
    for (const profileId of providerProfileOrder) {
      const match = byId.get(profileId);
      if (match) {
        ordered.push(match);
        byId.delete(profileId);
      }
    }
    const remaining = [...byId.values()].sort((left, right) =>
      left.display_name.localeCompare(right.display_name)
    );
    return [...ordered, ...remaining];
  }, [providerProfiles, providerProfileOrder]);

  useEffect(() => {
    if (!selectedCard) {
      setCardEditor(emptyEditorDraft());
      return;
    }
    setCardEditor({
      title: selectedCard.title,
      description: selectedCard.description ?? "",
      ownerKind: selectedCard.owner_kind,
      ownerAgentId: selectedCard.owner_agent_id ?? "",
      ownerHumanId: selectedCard.owner_human_id ?? "",
      dueAt: toInputDateTimeValue(selectedCard.due_at),
      tagsCsv: selectedCard.tags.join(", "),
      scriptMarkdown: selectedCard.script_markdown ?? "",
    });
  }, [selectedCard]);

  useEffect(() => {
    persistCockpitPagesToStorage(cockpitPages);
    if (!cockpitPages.some((page) => page.page_id === activeCockpitPageId)) {
      setActiveCockpitPageId(cockpitPages[0]?.page_id ?? "ops-default");
    }
  }, [activeCockpitPageId, cockpitPages]);

  const refreshBoard = useCallback(
    async (boardId: string, runtimeSettings: RuntimeConnectionSettings = settings) => {
      const detail = await getBoard(runtimeSettings, boardId);
      setBoard(detail);
    },
    [settings]
  );

  const queueBoardRefresh = useCallback(
    (boardId: string, runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (boardRefreshTimer.current) {
        globalThis.clearTimeout(boardRefreshTimer.current);
      }
      boardRefreshTimer.current = globalThis.setTimeout(() => {
        void refreshBoard(boardId, runtimeSettings).catch((error: unknown) => {
          setNotice({
            tone: "error",
            message: `Board refresh failed: ${String(error)}`,
          });
        });
      }, 250);
    },
    [refreshBoard, settings]
  );

  const loadMissionControlReadModels = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      const [
        calendar,
        focus,
        jobs,
        approvals,
        channelRuntime,
        status,
        jobsStatusResponse,
        profiles,
        skillResponse,
        pluginResponse,
        pluginRuntimeResponse,
      ] = await Promise.all([
        getMissionControlCalendarWeek(runtimeSettings),
        getMissionControlFocus(runtimeSettings, 100),
        listJobs(runtimeSettings, 200),
        listApprovals(runtimeSettings, "requested", 200),
        getChannelRuntimeStatus(runtimeSettings),
        getGatewayStatus(runtimeSettings),
        getJobsStatus(runtimeSettings),
        listAuthProfiles(runtimeSettings, { includeDisabled: true }),
        listSkills(runtimeSettings, true),
        listPlugins(runtimeSettings, true),
        listPluginRuntimeStatus(runtimeSettings, true),
      ]);
      setCalendarWeek(calendar);
      setFocusItems(focus.items);
      setJobsById(
        new Map(
          jobs.items.map((item) => [
            item.job_id,
            {
              job_id: item.job_id,
              name: item.name,
              agent_id: item.agent_id,
              enabled: item.enabled,
              schedule_kind: item.schedule_kind,
              interval_seconds: item.interval_seconds,
              cron_expr: item.cron_expr,
              next_run_at: item.next_run_at,
              last_run_at: item.last_run_at,
              last_error: item.last_error,
              lane:
                item.enabled &&
                item.schedule_kind === "interval" &&
                (item.interval_seconds ?? 0) <= 300 &&
                (item.interval_seconds ?? 0) > 0
                  ? "always_running"
                  : "scheduled",
              primary_action: item.enabled ? "pause" : "resume",
            } satisfies MissionControlCalendarJob,
          ])
        )
      );
      setApprovalsById(new Set(approvals.items.map((item) => item.approval_id)));
      setChannelStatuses(channelRuntime.items);
      setGatewayStatus(status);
      setJobsStatus(jobsStatusResponse);
      setAuthProfiles(profiles);
      setSkills(skillResponse.items);
      setPlugins(pluginResponse.items);
      setPluginRuntimeById(
        new Map(pluginRuntimeResponse.items.map((item) => [item.plugin_id, item]))
      );
    },
    [settings]
  );

  const queueMissionControlRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (missionControlRefreshTimer.current) {
        globalThis.clearTimeout(missionControlRefreshTimer.current);
      }
      missionControlRefreshTimer.current = globalThis.setTimeout(() => {
        void loadMissionControlReadModels(runtimeSettings).catch((error: unknown) => {
          setNotice({
            tone: "error",
            message: `Mission Control refresh failed: ${String(error)}`,
          });
        });
      }, 300);
    },
    [loadMissionControlReadModels, settings]
  );

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
        preferredBoardId ?? activeBoardId ?? boardList.items[0]?.board_id ?? null;
      setActiveBoardId(targetBoardId);
      if (targetBoardId) {
        await refreshBoard(targetBoardId, runtimeSettings);
      } else {
        setBoard(null);
      }
      await loadMissionControlReadModels(runtimeSettings);
    },
    [activeBoardId, loadMissionControlReadModels, refreshBoard, settings]
  );

  useEffect(() => {
    if (agents.length === 0) {
      setSelectedProviderControlAgentId("");
      return;
    }
    if (
      !selectedProviderControlAgentId ||
      !agents.some((agent) => agent.agent_id === selectedProviderControlAgentId)
    ) {
      setSelectedProviderControlAgentId(agents[0].agent_id);
    }
  }, [agents, selectedProviderControlAgentId]);

  useEffect(() => {
    if (providerOptions.length === 0) {
      setSelectedProviderControlProvider("");
      return;
    }
    if (
      !selectedProviderControlProvider ||
      !providerOptions.includes(selectedProviderControlProvider)
    ) {
      setSelectedProviderControlProvider(providerOptions[0]);
    }
  }, [providerOptions, selectedProviderControlProvider]);

  const reloadProviderProfileOrder = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      agentId: string = selectedProviderControlAgentId,
      provider: string = selectedProviderControlProvider
    ) => {
      if (!agentId || !provider) {
        setProviderProfileOrder([]);
        setProviderProfileOrderDirty(false);
        return;
      }
      const response = await getAgentProviderProfileOrder(runtimeSettings, agentId, provider);
      setProviderProfileOrder(response.profile_ids);
      setProviderProfileOrderDirty(false);
    },
    [selectedProviderControlAgentId, selectedProviderControlProvider, settings]
  );

  useEffect(() => {
    if (!settings.gateway_url.trim() || !selectedProviderControlAgentId || !selectedProviderControlProvider) {
      return;
    }
    void reloadProviderProfileOrder(settings).catch((error: unknown) => {
      setNotice({
        tone: "error",
        message: `Profile order load failed: ${String(error)}`,
      });
    });
  }, [
    reloadProviderProfileOrder,
    selectedProviderControlAgentId,
    selectedProviderControlProvider,
    settings,
  ]);

  useEffect(() => {
    void isGatewayTokenConfigured().then(setTokenConfigured);
  }, []);

  useEffect(() => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      setWsState("idle");
      return;
    }

    const subscription = connectGatewayEvents({
      settings,
      maxReconnectAttempts: 40,
      onState: setWsState,
      onEvent: (frame: WsEventFrame) => {
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

        if (
          frame.event_type.startsWith("job.") ||
          frame.event_type.startsWith("approval.") ||
          frame.event_type.startsWith("channel.") ||
          frame.event_type.startsWith("extension.") ||
          frame.event_type.startsWith("agent_mail.")
        ) {
          queueMissionControlRefresh(settings);
        }

        if (!activeBoardId) {
          return;
        }
        const payloadBoardId =
          typeof frame.payload.board_id === "string" ? frame.payload.board_id : null;
        if (payloadBoardId !== activeBoardId) {
          return;
        }

        setBoard((previous) => {
          if (!previous) {
            return previous;
          }
          if (frame.event_type === "board.card.moved") {
            const cardId =
              typeof frame.payload.card_id === "string" ? frame.payload.card_id : null;
            const columnId =
              typeof frame.payload.column_id === "string" ? frame.payload.column_id : null;
            const position =
              typeof frame.payload.position === "number" ? frame.payload.position : null;
            if (!cardId || !columnId) {
              return previous;
            }
            const target = previous.cards.find((item) => item.card_id === cardId);
            if (!target) {
              queueBoardRefresh(activeBoardId, settings);
              return previous;
            }
            return withUpsertCard(previous, {
              ...target,
              column_id: columnId,
              position: position ?? target.position,
            });
          }

          if (frame.event_type === "board.card.run") {
            const cardId =
              typeof frame.payload.card_id === "string" ? frame.payload.card_id : null;
            const runId =
              typeof frame.payload.run_id === "string" ? frame.payload.run_id : null;
            if (!cardId) {
              return previous;
            }
            const target = previous.cards.find((item) => item.card_id === cardId);
            if (!target) {
              return previous;
            }
            return withUpsertCard(previous, {
              ...target,
              latest_run_id: runId ?? target.latest_run_id,
            });
          }

          if (frame.event_type === "board.card.created") {
            queueBoardRefresh(activeBoardId, settings);
            return previous;
          }

          if (frame.event_type === "board.card.updated") {
            const cardId =
              typeof frame.payload.card_id === "string" ? frame.payload.card_id : null;
            const updatedAt =
              typeof frame.payload.updated_at === "number"
                ? frame.payload.updated_at
                : null;
            if (!cardId) {
              return previous;
            }
            const target = previous.cards.find((item) => item.card_id === cardId);
            if (!target) {
              queueBoardRefresh(activeBoardId, settings);
              return previous;
            }
            return withUpsertCard(previous, {
              ...target,
              updated_at: updatedAt ?? target.updated_at,
            });
          }

          if (frame.event_type === "board.asset.uploaded") {
            queueBoardRefresh(activeBoardId, settings);
          }
          return previous;
        });
      },
    });

    return () => {
      subscription.close();
    };
  }, [
    activeBoardId,
    queueBoardRefresh,
    queueMissionControlRefresh,
    settings,
    tokenConfigured,
  ]);

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

  const handleBoardChange = async (boardId: string) => {
    try {
      setActiveBoardId(boardId);
      await refreshBoard(boardId, settings);
    } catch (error) {
      setNotice({
        tone: "critical",
        message: `Board load failed: ${String(error)}`,
      });
    }
  };

  const handleDropCard = async (
    cardId: string,
    columnId: string,
    beforeCardId?: string
  ) => {
    if (!board || !activeBoardId) {
      return;
    }
    const snapshot = board;
    setBoard((previous) =>
      previous ? withOptimisticMove(previous, cardId, columnId, beforeCardId) : previous
    );
    try {
      const moved = await moveBoardCard(settings, activeBoardId, cardId, {
        column_id: columnId,
        before_card_id: beforeCardId,
      });
      setBoard((previous) =>
        previous ? withUpsertCard(previous, moved.card) : previous
      );
    } catch (error) {
      setBoard(snapshot);
      setNotice({ tone: "error", message: `Move failed: ${String(error)}` });
    }
  };

  const handleCreateCard = async (columnId: string, title: string) => {
    if (!activeBoardId) {
      return;
    }
    try {
      const created = await createBoardCard(settings, activeBoardId, {
        column_id: columnId,
        title,
      });
      setBoard((previous) =>
        previous ? withUpsertCard(previous, created.card) : previous
      );
      setNotice({ tone: "info", message: `Card created: ${created.card.title}` });
    } catch (error) {
      setNotice({ tone: "error", message: `Card create failed: ${String(error)}` });
    }
  };

  const saveCardDraft = async () => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const response = await updateBoardCard(settings, activeBoardId, selectedCardId, {
        title: cardEditor.title.trim(),
        description: cardEditor.description.trim() || null,
        owner_kind: cardEditor.ownerKind,
        owner_agent_id: cardEditor.ownerAgentId.trim() || null,
        owner_human_id: cardEditor.ownerHumanId.trim() || null,
        due_at: fromInputDateTimeValue(cardEditor.dueAt),
        tags: cardEditor.tagsCsv.trim()
          ? cardEditor.tagsCsv
              .split(",")
              .map((tag) => tag.trim())
              .filter(Boolean)
          : null,
        script_markdown: cardEditor.scriptMarkdown.trim() || null,
      });
      setBoard((previous) =>
        previous ? withUpsertCard(previous, response.card) : previous
      );
      setNotice({ tone: "info", message: "Card updated." });
    } catch (error) {
      setNotice({ tone: "error", message: `Card update failed: ${String(error)}` });
    }
  };

  const runCard = async () => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const response = await runBoardCard(settings, activeBoardId, selectedCardId);
      setBoard((previous) =>
        previous ? withUpsertCard(previous, response.card) : previous
      );
      setNotice({
        tone: "info",
        message: `Run queued: ${response.run.run_id} (${response.run.status})`,
      });
    } catch (error) {
      setNotice({ tone: "error", message: `Run failed: ${String(error)}` });
    }
  };

  const uploadAsset = async (file: File) => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const contentBase64 = await fileToBase64(file);
      const response = await uploadBoardCardAsset(
        settings,
        activeBoardId,
        selectedCardId,
        {
          filename: file.name,
          mime: file.type || "application/octet-stream",
          content_base64: contentBase64,
        }
      );
      setBoard((previous) =>
        previous ? withUpsertCard(previous, response.card) : previous
      );
      setNotice({ tone: "info", message: `Asset uploaded: ${response.asset.filename}` });
    } catch (error) {
      setNotice({ tone: "error", message: `Asset upload failed: ${String(error)}` });
    }
  };

  const previewAsset = async (cardId: string, cardAssetId: string) => {
    if (!activeBoardId) {
      return;
    }
    try {
      const blob = await fetchBoardCardAssetBlob(
        settings,
        activeBoardId,
        cardId,
        cardAssetId
      );
      if (selectedPreviewUrl) {
        URL.revokeObjectURL(selectedPreviewUrl);
      }
      const url = URL.createObjectURL(blob);
      setSelectedPreviewUrl(url);
    } catch (error) {
      setNotice({ tone: "error", message: `Asset preview failed: ${String(error)}` });
    }
  };

  const runCalendarJobNow = async (jobId: string) => {
    try {
      const response = await runJobNow(settings, jobId);
      setNotice({
        tone: "info",
        message: `Job run started (${response.job_run.status})`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Run-now failed: ${String(error)}`,
      });
    }
  };

  const toggleCalendarJob = async (jobId: string, enabled: boolean) => {
    try {
      const response = await setJobEnabledState(settings, jobId, enabled);
      setJobsById((previous) => {
        const next = new Map(previous);
        const existing = next.get(jobId);
        if (existing) {
          next.set(jobId, {
            ...existing,
            enabled: response.job.enabled,
            primary_action: response.job.enabled ? "pause" : "resume",
            next_run_at: response.job.next_run_at,
            last_error: response.job.last_error,
          });
        }
        return next;
      });
      setNotice({
        tone: "info",
        message: enabled ? "Job resumed." : "Job paused.",
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Job state update failed: ${String(error)}`,
      });
    }
  };

  const resolveFocusApproval = async (
    approvalId: string,
    decision: "approve" | "deny"
  ) => {
    try {
      const response = await resolveApproval(settings, approvalId, decision);
      setApprovalsById((previous) => {
        const next = new Set(previous);
        if (response.approval.status !== "requested") {
          next.delete(approvalId);
        }
        return next;
      });
      setNotice({
        tone: "info",
        message: `Approval ${decision}d.`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Approval ${decision} failed: ${String(error)}`,
      });
    }
  };

  const reconnectFocusChannel = async (provider: string) => {
    try {
      await reconnectChannelRuntime(settings, provider);
      setNotice({
        tone: "info",
        message: `Channel reconnect requested for ${provider}.`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Channel reconnect failed: ${String(error)}`,
      });
    }
  };

  const addCockpitWidget = (widgetKind: CockpitWidgetKind) => {
    const palette = COCKPIT_WIDGET_PALETTE.find((item) => item.widget === widgetKind);
    if (!palette) {
      return;
    }
    setCockpitPages((previous) =>
      previous.map((page) => {
        if (page.page_id !== activeCockpitPage.page_id) {
          return page;
        }
        const instanceId = `${widgetKind}-${Date.now()}-${Math.random()
          .toString(36)
          .slice(2, 8)}`;
        return {
          ...page,
          widgets: [
            ...page.widgets,
            {
              instance_id: instanceId,
              widget: widgetKind,
              title: palette.title,
              span: palette.defaultSpan,
            },
          ],
        };
      })
    );
  };

  const removeCockpitWidget = (instanceId: string) => {
    setCockpitPages((previous) =>
      previous.map((page) =>
        page.page_id === activeCockpitPage.page_id
          ? {
              ...page,
              widgets: page.widgets.filter((widget) => widget.instance_id !== instanceId),
            }
          : page
      )
    );
  };

  const moveCockpitWidget = (instanceId: string, delta: number) => {
    setCockpitPages((previous) =>
      previous.map((page) => {
        if (page.page_id !== activeCockpitPage.page_id) {
          return page;
        }
        const index = page.widgets.findIndex((widget) => widget.instance_id === instanceId);
        if (index < 0) {
          return page;
        }
        const target = Math.max(0, Math.min(page.widgets.length - 1, index + delta));
        if (target === index) {
          return page;
        }
        const nextWidgets = [...page.widgets];
        const [entry] = nextWidgets.splice(index, 1);
        nextWidgets.splice(target, 0, entry);
        return { ...page, widgets: nextWidgets };
      })
    );
  };

  const resizeCockpitWidget = (instanceId: string, delta: number) => {
    setCockpitPages((previous) =>
      previous.map((page) => {
        if (page.page_id !== activeCockpitPage.page_id) {
          return page;
        }
        return {
          ...page,
          widgets: page.widgets.map((widget) =>
            widget.instance_id === instanceId
              ? { ...widget, span: normalizeWidgetSpan(widget.span + delta) }
              : widget
          ),
        };
      })
    );
  };

  const resetCockpitLayout = () => {
    const defaults = defaultCockpitPages();
    setCockpitPages(defaults);
    setActiveCockpitPageId(defaults[0].page_id);
  };

  const addCockpitPage = () => {
    const nextPageId = `custom-${Date.now()}`;
    setCockpitPages((previous) => [
      ...previous,
      {
        page_id: nextPageId,
        name: `Custom ${previous.length + 1}`,
        widgets: [],
      },
    ]);
    setActiveCockpitPageId(nextPageId);
  };

  const exportCockpitLayout = () => {
    if (typeof window === "undefined") {
      return;
    }
    const payload = JSON.stringify(cockpitPages, null, 2);
    const blob = new Blob([payload], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `mission-control-cockpit-${Date.now()}.json`;
    document.body.appendChild(anchor);
    anchor.click();
    document.body.removeChild(anchor);
    URL.revokeObjectURL(url);
  };

  const importCockpitLayout = async (file: File) => {
    const raw = await file.text();
    const parsed = JSON.parse(raw) as unknown;
    const sanitized = sanitizeCockpitPages(parsed);
    setCockpitPages(sanitized);
    setActiveCockpitPageId(sanitized[0].page_id);
  };

  const moveProviderProfile = (profileId: string, delta: number) => {
    setProviderProfileOrder((previous) => {
      const baseOrder = previous.length > 0 ? [...previous] : orderedProviderProfiles.map((item) => item.auth_profile_id);
      const index = baseOrder.findIndex((item) => item === profileId);
      if (index < 0) {
        return previous;
      }
      const target = Math.max(0, Math.min(baseOrder.length - 1, index + delta));
      if (target === index) {
        return previous;
      }
      const [entry] = baseOrder.splice(index, 1);
      baseOrder.splice(target, 0, entry);
      setProviderProfileOrderDirty(true);
      return baseOrder;
    });
  };

  const saveProviderOrder = async () => {
    if (!selectedProviderControlAgentId || !selectedProviderControlProvider) {
      return;
    }
    try {
      const response = await setAgentProviderProfileOrder(
        settings,
        selectedProviderControlAgentId,
        selectedProviderControlProvider,
        providerProfileOrder
      );
      setProviderProfileOrder(response.profile_ids);
      setProviderProfileOrderDirty(false);
      setNotice({
        tone: "info",
        message: `Saved provider order for ${response.agent_id}/${response.provider}.`,
      });
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Saving provider order failed: ${String(error)}`,
      });
    }
  };

  const toggleSkillState = async (skillId: string, enabled: boolean) => {
    try {
      const response = await setSkillEnabled(settings, skillId, enabled);
      setSkills((previous) =>
        previous.map((item) => (item.skill_id === skillId ? response.skill : item))
      );
      setNotice({
        tone: "info",
        message: enabled ? `Skill enabled: ${skillId}` : `Skill disabled: ${skillId}`,
      });
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Skill update failed: ${String(error)}`,
      });
    }
  };

  const togglePluginState = async (pluginId: string, enabled: boolean) => {
    const target = plugins.find((item) => item.plugin_id === pluginId);
    if (!target) {
      return;
    }
    try {
      const response = await setPluginEnabled(
        settings,
        target,
        enabled,
        enabled ? "mission-control-enable" : "mission-control-disable"
      );
      setPlugins((previous) =>
        previous.map((item) => (item.plugin_id === pluginId ? response.plugin : item))
      );
      setNotice({
        tone: "info",
        message: enabled ? `Plugin enabled: ${pluginId}` : `Plugin disabled: ${pluginId}`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Plugin update failed: ${String(error)}`,
      });
    }
  };

  const visibleEvents = useMemo(() => {
    if (showRawEvents) {
      return eventStream;
    }
    return eventStream.filter((event) => !event.event_type.startsWith("heartbeat."));
  }, [eventStream, showRawEvents]);

  const incidentFocusItems = useMemo(() => {
    return focusItems.filter((item) =>
      incidentMode
        ? ["critical", "high", "error"].includes(item.severity.toLowerCase())
        : true
    );
  }, [focusItems, incidentMode]);

  const openBreakers = useMemo(() => {
    const fromStatus = gatewayStatus?.circuit_breakers ?? [];
    const fromJobs = jobsStatus?.circuit_breakers ?? [];
    const merged = new Map<string, CircuitBreakerStateResponse>();
    for (const item of [...fromStatus, ...fromJobs]) {
      const key = `${item.scope}:${item.target_id}`;
      merged.set(key, item);
    }
    return [...merged.values()].filter((item) => item.state.toLowerCase() === "open");
  }, [gatewayStatus, jobsStatus]);

  const openPluginBreakers = useMemo(() => {
    return [...pluginRuntimeById.values()].filter((item) => item.faulted);
  }, [pluginRuntimeById]);

  const columns = board?.columns ?? [];
  const boardScrollerRef = useRef<HTMLDivElement | null>(null);
  const columnVirtualizer = useVirtualizer({
    count: columns.length,
    horizontal: true,
    getScrollElement: () => boardScrollerRef.current,
    estimateSize: () => 320,
    overscan: 2,
  });

  const calendarAlwaysRunning = calendarWeek?.always_running ?? [];
  const calendarNextUp = calendarWeek?.next_up ?? [];
  const calendarJobs = calendarWeek?.jobs ?? Array.from(jobsById.values());

  useEffect(() => {
    return () => {
      if (boardRefreshTimer.current) {
        globalThis.clearTimeout(boardRefreshTimer.current);
      }
      if (missionControlRefreshTimer.current) {
        globalThis.clearTimeout(missionControlRefreshTimer.current);
      }
      if (selectedPreviewUrl) {
        URL.revokeObjectURL(selectedPreviewUrl);
      }
    };
  }, [selectedPreviewUrl]);

  const renderCockpitWidget = (widget: CockpitWidgetLayout) => {
    if (widget.widget === "health") {
      return (
        <article className="mc-cockpit-widget-body">
          <div className="mc-health-grid">
            <div>
              <strong>Gateway</strong>
              <p>{gatewayStatus?.service ?? "offline"}</p>
            </div>
            <div>
              <strong>Scheduler</strong>
              <p>{jobsStatus?.scheduler_running ? "running" : "paused"}</p>
            </div>
            <div>
              <strong>Approvals</strong>
              <p>{approvalsById.size}</p>
            </div>
            <div>
              <strong>Open Breakers</strong>
              <p>{openBreakers.length + openPluginBreakers.length}</p>
            </div>
            <div>
              <strong>Degraded Channels</strong>
              <p>
                {
                  channelStatuses.filter(
                    (item) => !item.healthy || item.lifecycle_state !== "running"
                  ).length
                }
              </p>
            </div>
          </div>
          <div className="mc-inline-actions">
            <label className="mc-checkbox">
              <input
                type="checkbox"
                checked={incidentMode}
                onChange={(event) => setIncidentMode(event.target.checked)}
              />
              Incident mode
            </label>
            <button type="button" onClick={() => queueMissionControlRefresh(settings)}>
              Refresh all
            </button>
          </div>
        </article>
      );
    }

    if (widget.widget === "focus") {
      return (
        <article className="mc-cockpit-widget-body">
          <ul className="mc-cockpit-list">
            {incidentFocusItems.slice(0, 8).map((item) => (
              <li key={item.item_id}>
                <div>
                  <strong>{item.title}</strong>
                  <p>{item.detail}</p>
                </div>
                <span className={clsx("chip", `chip-${item.severity}`)}>{item.severity}</span>
              </li>
            ))}
            {incidentFocusItems.length === 0 ? <li>No active items.</li> : null}
          </ul>
        </article>
      );
    }

    if (widget.widget === "breakers") {
      return (
        <article className="mc-cockpit-widget-body">
          <h4>Core Breakers</h4>
          <ul className="mc-cockpit-list compact">
            {openBreakers.slice(0, 6).map((breaker) => (
              <li key={`${breaker.scope}:${breaker.target_id}`}>
                <div>
                  <strong>{breaker.scope}</strong>
                  <p>{breaker.target_id}</p>
                </div>
                <span>{breaker.last_error_code ?? breaker.state}</span>
              </li>
            ))}
            {openBreakers.length === 0 ? <li>No open core breakers.</li> : null}
          </ul>
          <h4>Plugin Breakers</h4>
          <ul className="mc-cockpit-list compact">
            {openPluginBreakers.slice(0, 6).map((breaker) => (
              <li key={breaker.plugin_id}>
                <div>
                  <strong>{breaker.plugin_id}</strong>
                  <p>{breaker.last_error ?? "faulted"}</p>
                </div>
                <span>{breaker.last_error_code ?? "faulted"}</span>
              </li>
            ))}
            {openPluginBreakers.length === 0 ? <li>No plugin breakers.</li> : null}
          </ul>
        </article>
      );
    }

    if (widget.widget === "jobs") {
      return (
        <article className="mc-cockpit-widget-body">
          <ul className="mc-cockpit-list">
            {calendarJobs.slice(0, 10).map((job) => (
              <li key={job.job_id}>
                <div>
                  <strong>{job.name}</strong>
                  <p>{formatDateTime(job.next_run_at)}</p>
                </div>
                <div className="mc-inline-actions">
                  <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                    Run
                  </button>
                  <button
                    type="button"
                    className={job.enabled ? "danger" : ""}
                    onClick={() => void toggleCalendarJob(job.job_id, !job.enabled)}
                  >
                    {job.enabled ? "Pause" : "Resume"}
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </article>
      );
    }

    if (widget.widget === "channels") {
      return (
        <article className="mc-cockpit-widget-body">
          <ul className="mc-cockpit-list">
            {channelStatuses.map((item) => (
              <li key={item.provider}>
                <div>
                  <strong>{item.provider}</strong>
                  <p>{item.last_error ?? item.detail ?? item.lifecycle_state}</p>
                </div>
                <button
                  type="button"
                  onClick={() => void reconnectFocusChannel(item.provider)}
                >
                  Reconnect
                </button>
              </li>
            ))}
          </ul>
        </article>
      );
    }

    if (widget.widget === "profiles") {
      return (
        <article className="mc-cockpit-widget-body">
          <div className="mc-field-grid">
            <label>
              Agent
              <select
                value={selectedProviderControlAgentId}
                onChange={(event) => setSelectedProviderControlAgentId(event.target.value)}
              >
                {agents.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name} ({agent.agent_id})
                  </option>
                ))}
              </select>
            </label>
            <label>
              Provider
              <select
                value={selectedProviderControlProvider}
                onChange={(event) => setSelectedProviderControlProvider(event.target.value)}
              >
                {providerOptions.map((provider) => (
                  <option key={provider} value={provider}>
                    {provider}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <ul className="mc-cockpit-list">
            {orderedProviderProfiles.map((profile) => (
              <li key={profile.auth_profile_id}>
                <div>
                  <strong>{profile.display_name}</strong>
                  <p>
                    {profile.auth_mode} / {profile.risk_level} /{" "}
                    {profile.enabled ? "enabled" : "disabled"}
                  </p>
                </div>
                <div className="mc-inline-actions">
                  <button
                    type="button"
                    onClick={() => moveProviderProfile(profile.auth_profile_id, -1)}
                  >
                    Up
                  </button>
                  <button
                    type="button"
                    onClick={() => moveProviderProfile(profile.auth_profile_id, 1)}
                  >
                    Down
                  </button>
                </div>
              </li>
            ))}
            {orderedProviderProfiles.length === 0 ? <li>No profiles for provider.</li> : null}
          </ul>
          <div className="mc-inline-actions">
            <button type="button" onClick={() => void saveProviderOrder()}>
              Save Order
            </button>
            <button type="button" onClick={() => void reloadProviderProfileOrder(settings)}>
              Reload
            </button>
            {providerProfileOrderDirty ? <span className="chip chip-error">unsaved</span> : null}
          </div>
        </article>
      );
    }

    if (widget.widget === "skills") {
      return (
        <article className="mc-cockpit-widget-body">
          <ul className="mc-cockpit-list">
            {skills.map((skill) => (
              <li key={skill.skill_id}>
                <div>
                  <strong>{skill.title}</strong>
                  <p>{skill.skill_id}</p>
                </div>
                <button
                  type="button"
                  className={skill.enabled ? "danger" : ""}
                  onClick={() => void toggleSkillState(skill.skill_id, !skill.enabled)}
                >
                  {skill.enabled ? "Disable" : "Enable"}
                </button>
              </li>
            ))}
            {skills.length === 0 ? <li>No skills loaded.</li> : null}
          </ul>
        </article>
      );
    }

    if (widget.widget === "plugins") {
      return (
        <article className="mc-cockpit-widget-body">
          <ul className="mc-cockpit-list">
            {plugins.map((plugin) => {
              const runtime = pluginRuntimeById.get(plugin.plugin_id);
              return (
                <li key={plugin.plugin_id}>
                  <div>
                    <strong>{plugin.display_name}</strong>
                    <p>
                      {plugin.plugin_id} / {runtime?.faulted ? "faulted" : "ok"}
                    </p>
                  </div>
                  <button
                    type="button"
                    className={plugin.enabled ? "danger" : ""}
                    onClick={() => void togglePluginState(plugin.plugin_id, !plugin.enabled)}
                  >
                    {plugin.enabled ? "Disable" : "Enable"}
                  </button>
                </li>
              );
            })}
            {plugins.length === 0 ? <li>No plugins installed.</li> : null}
          </ul>
        </article>
      );
    }

    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-events compact">
          {visibleEvents.slice(0, 24).map((event) => (
            <article key={event.event_id} className="mc-event-item">
              <div className="mc-event-head">
                <span>{event.event_type}</span>
                <span>{formatDateTime(event.ts_unix_ms)}</span>
              </div>
            </article>
          ))}
          {visibleEvents.length === 0 ? <p className="mc-empty-events">No events captured.</p> : null}
        </div>
      </article>
    );
  };

  return (
    <main className="mc-shell">
      <header className="mc-topbar">
        <div className="mc-brand-block">
          <p className="mc-overline">CarsinOS</p>
          <h1>Mission Control Slick</h1>
        </div>
        <div className="mc-status-strip">
          <span className={clsx("chip", `chip-${healthState}`)}>health: {healthState}</span>
          <span className={clsx("chip", `chip-${wsState}`)}>ws: {wsState}</span>
          <span className="chip">token: {tokenConfigured ? "set" : "missing"}</span>
        </div>
      </header>

      <section className={clsx("mc-pinned-health", incidentMode && "incident-mode")}>
        <div className="mc-pinned-stat">
          <strong>Incident</strong>
          <span>{incidentMode ? "ON" : "OFF"}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Open breakers</strong>
          <span>{openBreakers.length + openPluginBreakers.length}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Pending approvals</strong>
          <span>{approvalsById.size}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Jobs due</strong>
          <span>{jobsStatus?.jobs_due ?? 0}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Scheduler</strong>
          <span>{jobsStatus?.scheduler_running ? "running" : "paused"}</span>
        </div>
        <label className="mc-checkbox">
          <input
            type="checkbox"
            checked={incidentMode}
            onChange={(event) => setIncidentMode(event.target.checked)}
          />
          Incident mode filter
        </label>
      </section>

      <section className="mc-connection">
        <label>
          Gateway URL
          <input
            value={gatewayDraft}
            onChange={(event) => setGatewayDraft(event.target.value)}
            placeholder="http://127.0.0.1:8080"
          />
        </label>
        <label>
          Gateway Token
          <input
            value={tokenDraft}
            onChange={(event) => setTokenDraft(event.target.value)}
            placeholder={tokenConfigured ? "token stored in keychain" : "paste token"}
            type="password"
          />
        </label>
        <div className="mc-connection-actions">
          <button type="button" onClick={() => void saveConnection()}>
            Save + Connect
          </button>
          <button type="button" onClick={() => void reconnect()}>
            Reconnect
          </button>
          <button type="button" className="danger" onClick={() => void clearToken()}>
            Clear Token
          </button>
        </div>
      </section>

      {notice ? (
        <div className={clsx("mc-notice", `mc-notice-${notice.tone}`)}>{notice.message}</div>
      ) : null}

      <section className="mc-tabs">
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "boards" && "mc-tab-active")}
          onClick={() => setActiveTab("boards")}
        >
          Boards
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "calendar" && "mc-tab-active")}
          onClick={() => setActiveTab("calendar")}
        >
          Calendar
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "focus" && "mc-tab-active")}
          onClick={() => setActiveTab("focus")}
        >
          Operator Focus
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "events" && "mc-tab-active")}
          onClick={() => setActiveTab("events")}
        >
          Event Stream
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "cockpit" && "mc-tab-active")}
          onClick={() => setActiveTab("cockpit")}
        >
          Cockpit
        </button>
      </section>

      {activeTab === "boards" ? (
        <section className="mc-main-grid">
          <section className="mc-board-panel">
            <div className="mc-board-toolbar">
              <label>
                Board
                <select
                  value={activeBoardId ?? ""}
                  onChange={(event) => void handleBoardChange(event.target.value)}
                >
                  {boards.map((item) => (
                    <option key={item.board_id} value={item.board_id}>
                      {item.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div className="mc-board-scroll" ref={boardScrollerRef}>
              <div
                className="mc-board-canvas"
                style={{ width: `${columnVirtualizer.getTotalSize()}px` }}
              >
                {columnVirtualizer.getVirtualItems().map((virtualColumn) => {
                  const column = columns[virtualColumn.index];
                  const cards = cardsByColumn.get(column.column_id) ?? [];
                  return (
                    <div
                      key={column.column_id}
                      className="mc-board-column-wrap"
                      style={{ transform: `translateX(${virtualColumn.start}px)` }}
                    >
                      <BoardLane
                        column={column}
                        cards={cards}
                        selectedCardId={selectedCardId}
                        dragCardId={dragCardId}
                        setDragCardId={setDragCardId}
                        onSelectCard={setSelectedCardId}
                        onDropCard={handleDropCard}
                        onCreateCard={handleCreateCard}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
          </section>

          <aside className="mc-drawer">
            {!selectedCard ? (
              <div className="mc-empty-drawer">Select a card to edit and run.</div>
            ) : (
              <>
                <header className="mc-drawer-header">
                  <h2>Card Drawer</h2>
                  {selectedCard.latest_run_id ? (
                    <span className="run-pill">latest run: {selectedCard.latest_run_id}</span>
                  ) : null}
                </header>

                <label>
                  Title
                  <input
                    value={cardEditor.title}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        title: event.target.value,
                      }))
                    }
                  />
                </label>

                <label>
                  Description
                  <textarea
                    value={cardEditor.description}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        description: event.target.value,
                      }))
                    }
                  />
                </label>

                <div className="mc-field-grid">
                  <label>
                    Owner Kind
                    <select
                      value={cardEditor.ownerKind}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          ownerKind: event.target.value,
                        }))
                      }
                    >
                      <option value="unassigned">unassigned</option>
                      <option value="agent">agent</option>
                      <option value="human">human</option>
                    </select>
                  </label>

                  <label>
                    Owner Agent
                    <select
                      value={cardEditor.ownerAgentId}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          ownerAgentId: event.target.value,
                        }))
                      }
                    >
                      <option value="">none</option>
                      {agents.map((agent) => (
                        <option key={agent.agent_id} value={agent.agent_id}>
                          {agent.name} ({agent.agent_id})
                        </option>
                      ))}
                    </select>
                  </label>
                </div>

                <div className="mc-field-grid">
                  <label>
                    Owner Human
                    <input
                      value={cardEditor.ownerHumanId}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          ownerHumanId: event.target.value,
                        }))
                      }
                    />
                  </label>

                  <label>
                    Due
                    <input
                      type="datetime-local"
                      value={cardEditor.dueAt}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          dueAt: event.target.value,
                        }))
                      }
                    />
                  </label>
                </div>

                <label>
                  Tags (comma separated)
                  <input
                    value={cardEditor.tagsCsv}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        tagsCsv: event.target.value,
                      }))
                    }
                  />
                </label>

                <label>
                  Script Markdown
                  <textarea
                    className="script-area"
                    value={cardEditor.scriptMarkdown}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        scriptMarkdown: event.target.value,
                      }))
                    }
                  />
                </label>

                <div className="mc-drawer-actions">
                  <button type="button" onClick={() => void saveCardDraft()}>
                    Save Card
                  </button>
                  <button type="button" onClick={() => void runCard()}>
                    Run Card
                  </button>
                </div>

                <section className="mc-assets">
                  <h3>Assets</h3>
                  <label className="upload-pill">
                    <input
                      type="file"
                      onChange={(event) => {
                        const file = event.target.files?.[0];
                        if (!file) {
                          return;
                        }
                        void uploadAsset(file);
                        event.currentTarget.value = "";
                      }}
                    />
                    Upload
                  </label>
                  <ul>
                    {selectedCard.assets.map((asset) => (
                      <li key={asset.card_asset_id}>
                        <button
                          type="button"
                          onClick={() =>
                            void previewAsset(selectedCard.card_id, asset.card_asset_id)
                          }
                        >
                          {asset.filename}
                        </button>
                        <span>{formatBytes(asset.bytes)}</span>
                      </li>
                    ))}
                  </ul>
                  {selectedPreviewUrl ? (
                    <div className="mc-preview-wrap">
                      <img src={selectedPreviewUrl} alt="asset preview" />
                    </div>
                  ) : null}
                </section>
              </>
            )}
          </aside>
        </section>
      ) : null}

      {activeTab === "calendar" ? (
        <section className="mc-alt-grid">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Week Planning</h2>
              <p>
                {calendarWeek
                  ? `${formatDateTime(calendarWeek.week_start_ms)} - ${formatDateTime(
                      calendarWeek.week_end_ms
                    )}`
                  : "No week data loaded"}
              </p>
            </header>
            <div className="mc-lane-grid">
              <section className="mc-lane-panel">
                <h3>Always Running</h3>
                <ul>
                  {calendarAlwaysRunning.map((job) => (
                    <li key={job.job_id}>
                      <div>
                        <strong>{job.name}</strong>
                        <p>{job.agent_id}</p>
                      </div>
                      <div className="mc-inline-actions">
                        <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                          Run now
                        </button>
                        <button
                          type="button"
                          className={job.enabled ? "danger" : ""}
                          onClick={() => void toggleCalendarJob(job.job_id, !job.enabled)}
                        >
                          {job.enabled ? "Pause" : "Resume"}
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
              <section className="mc-lane-panel">
                <h3>Next Up</h3>
                <ul>
                  {calendarNextUp.map((job) => (
                    <li key={job.job_id}>
                      <div>
                        <strong>{job.name}</strong>
                        <p>{formatDateTime(job.next_run_at)}</p>
                      </div>
                      <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                        Run now
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            </div>
          </article>
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Scheduler Matrix</h2>
              <p>{calendarJobs.length} jobs</p>
            </header>
            <div className="mc-table-wrap">
              <table className="mc-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Schedule</th>
                    <th>Next Run</th>
                    <th>Status</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {calendarJobs.map((job) => (
                    <tr key={job.job_id}>
                      <td>
                        <strong>{job.name}</strong>
                        <p>{job.agent_id}</p>
                      </td>
                      <td>
                        {job.schedule_kind}
                        {job.interval_seconds ? ` / ${job.interval_seconds}s` : ""}
                        {job.cron_expr ? ` / ${job.cron_expr}` : ""}
                      </td>
                      <td>{formatDateTime(job.next_run_at)}</td>
                      <td>
                        <span
                          className={clsx("chip", job.enabled ? "chip-up" : "chip-down")}
                        >
                          {job.enabled ? "enabled" : "paused"}
                        </span>
                      </td>
                      <td>
                        <div className="mc-inline-actions">
                          <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                            Run
                          </button>
                          <button
                            type="button"
                            className={job.enabled ? "danger" : ""}
                            onClick={() => void toggleCalendarJob(job.job_id, !job.enabled)}
                          >
                            {job.enabled ? "Pause" : "Resume"}
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </article>
        </section>
      ) : null}

      {activeTab === "focus" ? (
        <section className="mc-alt-grid">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Operator Focus Queue</h2>
              <p>{focusItems.length} open attention items</p>
            </header>
            <div className="mc-focus-list">
              {focusItems.map((item) => (
                <article key={item.item_id} className={clsx("mc-focus-item", item.severity)}>
                  <div className="mc-focus-head">
                    <span className={clsx("chip", `chip-${item.severity}`)}>
                      {item.severity}
                    </span>
                    <span>{item.category}</span>
                    <span>{formatDateTime(item.created_at)}</span>
                  </div>
                  <h3>{item.title}</h3>
                  <p>{item.detail}</p>
                  <div className="mc-inline-actions">
                    {item.category === "approval" ? (
                      <>
                        <button
                          type="button"
                          onClick={() =>
                            void resolveFocusApproval(
                              String(item.action_payload.approval_id ?? ""),
                              "approve"
                            )
                          }
                        >
                          Approve
                        </button>
                        <button
                          type="button"
                          className="danger"
                          onClick={() =>
                            void resolveFocusApproval(
                              String(item.action_payload.approval_id ?? ""),
                              "deny"
                            )
                          }
                        >
                          Deny
                        </button>
                      </>
                    ) : null}
                    {item.category === "run_failure" ? (
                      <button
                        type="button"
                        onClick={() =>
                          void runCalendarJobNow(String(item.action_payload.job_id ?? ""))
                        }
                      >
                        Retry Job
                      </button>
                    ) : null}
                    {item.category === "channel_health" ? (
                      <button
                        type="button"
                        onClick={() =>
                          void reconnectFocusChannel(
                            String(item.action_payload.provider ?? "")
                          )
                        }
                      >
                        Reconnect Channel
                      </button>
                    ) : null}
                  </div>
                </article>
              ))}
            </div>
          </article>
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Ops Snapshot</h2>
              <p>Live queue and channel posture</p>
            </header>
            <ul className="mc-stat-list">
              <li>
                <strong>Pending approvals</strong>
                <span>{approvalsById.size}</span>
              </li>
              <li>
                <strong>Channel adapters</strong>
                <span>{channelStatuses.length}</span>
              </li>
              <li>
                <strong>Degraded channels</strong>
                <span>
                  {
                    channelStatuses.filter(
                      (item) => !item.healthy || item.lifecycle_state !== "running"
                    ).length
                  }
                </span>
              </li>
            </ul>
            <div className="mc-channel-grid">
              {channelStatuses.map((item) => (
                <article key={item.provider} className="mc-channel-card">
                  <h3>{item.provider}</h3>
                  <p>{item.lifecycle_state}</p>
                  <p>{item.last_error ?? item.detail ?? "healthy"}</p>
                  <button
                    type="button"
                    onClick={() => void reconnectFocusChannel(item.provider)}
                  >
                    Reconnect
                  </button>
                </article>
              ))}
            </div>
          </article>
        </section>
      ) : null}

      {activeTab === "events" ? (
        <section className="mc-alt-grid">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Realtime Event Stream</h2>
              <label className="mc-checkbox">
                <input
                  type="checkbox"
                  checked={showRawEvents}
                  onChange={(event) => setShowRawEvents(event.target.checked)}
                />
                Show raw heartbeat events
              </label>
            </header>
            <div className="mc-events">
              {visibleEvents.map((event) => (
                <article key={event.event_id} className="mc-event-item">
                  <div className="mc-event-head">
                    <span>{event.event_type}</span>
                    <span>{event.entity}</span>
                    <span>{formatDateTime(event.ts_unix_ms)}</span>
                  </div>
                  <pre>{JSON.stringify(event.payload, null, 2)}</pre>
                </article>
              ))}
              {visibleEvents.length === 0 ? (
                <p className="mc-empty-events">No events captured yet.</p>
              ) : null}
            </div>
          </article>
        </section>
      ) : null}

      {activeTab === "cockpit" ? (
        <section className="mc-cockpit-grid">
          <aside className="mc-surface mc-cockpit-sidebar">
            <header className="mc-surface-header">
              <h2>Layout Studio</h2>
              <p>Widget palette + saved pages</p>
            </header>
            <div className="mc-field-grid">
              <label>
                Active Page
                <select
                  value={activeCockpitPage.page_id}
                  onChange={(event) => setActiveCockpitPageId(event.target.value)}
                >
                  {cockpitPages.map((page) => (
                    <option key={page.page_id} value={page.page_id}>
                      {page.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Rename Page
                <input
                  value={activeCockpitPage.name}
                  onChange={(event) =>
                    setCockpitPages((previous) =>
                      previous.map((page) =>
                        page.page_id === activeCockpitPage.page_id
                          ? { ...page, name: event.target.value || "Custom Page" }
                          : page
                      )
                    )
                  }
                />
              </label>
            </div>
            <div className="mc-inline-actions">
              <button type="button" onClick={addCockpitPage}>
                Add Page
              </button>
              <button type="button" onClick={exportCockpitLayout}>
                Export JSON
              </button>
              <label className="upload-pill">
                <input
                  type="file"
                  accept="application/json"
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (!file) {
                      return;
                    }
                    void importCockpitLayout(file).catch((error: unknown) =>
                      setNotice({
                        tone: "error",
                        message: `Cockpit import failed: ${String(error)}`,
                      })
                    );
                    event.currentTarget.value = "";
                  }}
                />
                Import JSON
              </label>
              <button type="button" className="danger" onClick={resetCockpitLayout}>
                Restore Defaults
              </button>
            </div>
            <div className="mc-cockpit-palette">
              {COCKPIT_WIDGET_PALETTE.map((entry) => (
                <article key={entry.widget} className="mc-palette-item">
                  <div>
                    <h3>{entry.title}</h3>
                    <p>{entry.description}</p>
                  </div>
                  <button type="button" onClick={() => addCockpitWidget(entry.widget)}>
                    Add
                  </button>
                </article>
              ))}
            </div>
          </aside>
          <section className="mc-surface">
            <header className="mc-surface-header">
              <h2>{activeCockpitPage.name}</h2>
              <p>{activeCockpitPage.widgets.length} widgets</p>
            </header>
            <div className="mc-cockpit-canvas">
              {activeCockpitPage.widgets.map((widget) => (
                <article
                  key={widget.instance_id}
                  className="mc-cockpit-widget"
                  style={{ gridColumn: `span ${normalizeWidgetSpan(widget.span)}` }}
                >
                  <header className="mc-cockpit-widget-head">
                    <h3>{widget.title}</h3>
                    <div className="mc-inline-actions">
                      <button type="button" onClick={() => moveCockpitWidget(widget.instance_id, -1)}>
                        Up
                      </button>
                      <button type="button" onClick={() => moveCockpitWidget(widget.instance_id, 1)}>
                        Down
                      </button>
                      <button type="button" onClick={() => resizeCockpitWidget(widget.instance_id, -1)}>
                        -
                      </button>
                      <button type="button" onClick={() => resizeCockpitWidget(widget.instance_id, 1)}>
                        +
                      </button>
                      <button
                        type="button"
                        className="danger"
                        onClick={() => removeCockpitWidget(widget.instance_id)}
                      >
                        Remove
                      </button>
                    </div>
                  </header>
                  {renderCockpitWidget(widget)}
                </article>
              ))}
              {activeCockpitPage.widgets.length === 0 ? (
                <div className="mc-empty-drawer">
                  Add widgets from the palette to build this page.
                </div>
              ) : null}
            </div>
          </section>
        </section>
      ) : null}
    </main>
  );
}
