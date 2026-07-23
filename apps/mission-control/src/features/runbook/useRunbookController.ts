import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { getRunbookDetail, listRunbooks } from "../../lib/api";
import type { NotifyFn } from "../../app/useAppController";
import type {
  Agent,
  RunbookDetailResponse,
  RunbookStatusCountsResponse,
  RunbookSummaryItemResponse,
  RuntimeConnectionSettings,
} from "../../types";
import {
  RUNBOOK_FETCH_PAGE_LIMIT,
  RUNBOOK_REFRESH_DEBOUNCE_MS,
  RUNBOOK_STALE_THRESHOLD_MS,
  RUNBOOK_UNSUPPORTED_STATUS_FRAGMENTS,
} from "./runbookConfig";
import {
  buildRunbookSummaryIndex,
  getRunbookSummariesForEntity,
} from "./runbookSummaryUtils";

type RunbookAvailability = "disabled" | "loading" | "ready" | "unsupported" | "error";

export interface RunbookFilters {
  kind: string;
  status: string;
  owner_agent_id: string;
  query: string;
}

interface UseRunbookControllerOptions {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  enabled: boolean;
  setNotice: NotifyFn;
}

const EMPTY_COUNTS: RunbookStatusCountsResponse = {
  pending: 0,
  active: 0,
  waiting: 0,
  blocked: 0,
  failed: 0,
  completed: 0,
  limited: 0,
};

const DEFAULT_FILTERS: RunbookFilters = {
  kind: "all",
  status: "all",
  owner_agent_id: "",
  query: "",
};

function isDefaultFilters(filters: RunbookFilters): boolean {
  return (
    filters.kind === DEFAULT_FILTERS.kind &&
    filters.status === DEFAULT_FILTERS.status &&
    filters.owner_agent_id === DEFAULT_FILTERS.owner_agent_id &&
    filters.query === DEFAULT_FILTERS.query
  );
}

function normalizeErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isUnsupportedError(error: unknown): boolean {
  const message = normalizeErrorMessage(error).toLowerCase();
  return RUNBOOK_UNSUPPORTED_STATUS_FRAGMENTS.some((fragment) =>
    message.includes(fragment)
  );
}

async function fetchAllRunbooks(
  settings: RuntimeConnectionSettings,
  filters: RunbookFilters
): Promise<{
  generatedAtMs: number;
  items: RunbookSummaryItemResponse[];
  countsByStatus: RunbookStatusCountsResponse;
}> {
  let cursor: string | undefined;
  const items: RunbookSummaryItemResponse[] = [];
  let generatedAtMs = Date.now();
  let countsByStatus = EMPTY_COUNTS;

  do {
    const response = await listRunbooks(settings, {
      kind: filters.kind !== "all" ? filters.kind : undefined,
      status: filters.status !== "all" ? filters.status : undefined,
      owner_agent_id: filters.owner_agent_id || undefined,
      query: filters.query.trim() || undefined,
      limit: RUNBOOK_FETCH_PAGE_LIMIT,
      cursor,
    });
    generatedAtMs = response.generated_at_ms;
    countsByStatus = response.counts_by_status;
    items.push(...response.items);
    cursor = response.next_cursor ?? undefined;
  } while (cursor);

  return {
    generatedAtMs,
    items,
    countsByStatus,
  };
}

export function useRunbookController(options: UseRunbookControllerOptions) {
  const { settings, agents, enabled, setNotice } = options;
  const [availability, setAvailability] = useState<RunbookAvailability>(
    enabled ? "loading" : "disabled"
  );
  const [availabilityMessage, setAvailabilityMessage] = useState<string | null>(null);
  const [filters, setFilters] = useState<RunbookFilters>(DEFAULT_FILTERS);
  const deferredQuery = useDeferredValue(filters.query.trim());
  const effectiveFilters = useMemo(
    () => ({
      ...filters,
      query: deferredQuery,
    }),
    [deferredQuery, filters]
  );
  const [items, setItems] = useState<RunbookSummaryItemResponse[]>([]);
  const [allItems, setAllItems] = useState<RunbookSummaryItemResponse[]>([]);
  const [countsByStatus, setCountsByStatus] =
    useState<RunbookStatusCountsResponse>(EMPTY_COUNTS);
  const [allCountsByStatus, setAllCountsByStatus] =
    useState<RunbookStatusCountsResponse>(EMPTY_COUNTS);
  const [generatedAtMs, setGeneratedAtMs] = useState<number | null>(null);
  const [selectedRunbookKind, setSelectedRunbookKind] = useState<string>("");
  const [selectedAnchorId, setSelectedAnchorId] = useState<string>("");
  const [openRequestVersion, setOpenRequestVersion] = useState(0);
  const [detail, setDetail] = useState<RunbookDetailResponse | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [lastRefreshAtMs, setLastRefreshAtMs] = useState<number | null>(null);
  const refreshTimerRef = useRef<number | null>(null);
  const listRequestIdRef = useRef(0);
  const detailRequestIdRef = useRef(0);

  const agentsById = useMemo(
    () => new Map(agents.map((agent) => [agent.agent_id, agent] as const)),
    [agents]
  );

  const selectedRunbookId =
    selectedRunbookKind && selectedAnchorId
      ? `${selectedRunbookKind}:${selectedAnchorId}`
      : "";
  const selectedSummary = selectedRunbookId
    ? items.find((item) => item.runbook_id === selectedRunbookId) ?? null
    : null;
  const summaryIndex = useMemo(() => buildRunbookSummaryIndex(items), [items]);

  const loadRunbookDetail = useCallback(
    async (
      runbookKind: string,
      anchorId: string,
      runtimeSettings: RuntimeConnectionSettings = settings
    ) => {
      if (!enabled || !runbookKind.trim() || !anchorId.trim()) {
        detailRequestIdRef.current += 1;
        setDetailLoading(false);
        setDetail(null);
        setDetailError(null);
        return;
      }

      const requestId = ++detailRequestIdRef.current;
      setDetailLoading(true);
      setDetailError(null);
      try {
        const nextDetail = await getRunbookDetail(runtimeSettings, runbookKind, anchorId);
        if (detailRequestIdRef.current !== requestId) {
          return;
        }
        setDetail(nextDetail);
      } catch (error: unknown) {
        if (detailRequestIdRef.current !== requestId) {
          return;
        }
        const message = normalizeErrorMessage(error);
        setDetail(null);
        setDetailError(message);
      } finally {
        if (detailRequestIdRef.current === requestId) {
          setDetailLoading(false);
        }
      }
    },
    [enabled, settings]
  );

  const loadRunbookData = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      nextFilters: RunbookFilters = effectiveFilters
    ) => {
      if (!enabled) {
        listRequestIdRef.current += 1;
        detailRequestIdRef.current += 1;
        setAvailability("disabled");
        setAvailabilityMessage("Runbook hub is disabled in Config.");
        setItems([]);
        setAllItems([]);
        setCountsByStatus(EMPTY_COUNTS);
        setAllCountsByStatus(EMPTY_COUNTS);
        setGeneratedAtMs(null);
        setDetailLoading(false);
        setDetail(null);
        setDetailError(null);
        return;
      }

      const requestId = ++listRequestIdRef.current;
      setAvailability((current) => (current === "ready" ? current : "loading"));
      setAvailabilityMessage(null);

      try {
        const [response, globalResponse] = await Promise.all([
          fetchAllRunbooks(runtimeSettings, nextFilters),
          isDefaultFilters(nextFilters)
            ? Promise.resolve<Awaited<ReturnType<typeof fetchAllRunbooks>> | null>(null)
            : fetchAllRunbooks(runtimeSettings, DEFAULT_FILTERS),
        ]);
        if (listRequestIdRef.current !== requestId) {
          return;
        }
        setItems(response.items);
        setCountsByStatus(response.countsByStatus);
        setAllItems(globalResponse?.items ?? response.items);
        setAllCountsByStatus(globalResponse?.countsByStatus ?? response.countsByStatus);
        setGeneratedAtMs(response.generatedAtMs);
        setLastRefreshAtMs(Date.now());
        setAvailability("ready");

        const preferredItem =
          response.items.find((item) => item.runbook_id === selectedRunbookId) ??
          response.items[0] ??
          null;
        if (preferredItem) {
          setSelectedRunbookKind(preferredItem.runbook_kind);
          setSelectedAnchorId(preferredItem.anchor_id);
        } else {
          detailRequestIdRef.current += 1;
          setDetailLoading(false);
          setSelectedRunbookKind("");
          setSelectedAnchorId("");
          setDetail(null);
          setDetailError(null);
        }
      } catch (error: unknown) {
        if (listRequestIdRef.current !== requestId) {
          return;
        }
        if (isUnsupportedError(error)) {
          setAvailability("unsupported");
          setAvailabilityMessage(
            "The connected gateway does not expose the Runbook surface yet."
          );
          setItems([]);
          setAllItems([]);
          setCountsByStatus(EMPTY_COUNTS);
          setAllCountsByStatus(EMPTY_COUNTS);
          setGeneratedAtMs(null);
          detailRequestIdRef.current += 1;
          setDetailLoading(false);
          setDetail(null);
          setDetailError(null);
          return;
        }
        setAvailability("error");
        setAvailabilityMessage(normalizeErrorMessage(error));
      }
    },
    [effectiveFilters, enabled, selectedRunbookId, settings]
  );

  const queueRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (refreshTimerRef.current) {
        window.clearTimeout(refreshTimerRef.current);
      }
      refreshTimerRef.current = window.setTimeout(() => {
        void loadRunbookData(runtimeSettings).catch((error: unknown) => {
          setAvailability("error");
          setAvailabilityMessage(normalizeErrorMessage(error));
        });
      }, RUNBOOK_REFRESH_DEBOUNCE_MS);
    },
    [loadRunbookData, settings]
  );

  useEffect(() => {
    void loadRunbookData(settings, effectiveFilters).catch((error: unknown) => {
      setAvailability("error");
      setAvailabilityMessage(normalizeErrorMessage(error));
    });
  }, [effectiveFilters, enabled, loadRunbookData, settings]);

  useEffect(() => {
    if (!selectedRunbookKind || !selectedAnchorId) {
      detailRequestIdRef.current += 1;
      setDetailLoading(false);
      setDetail(null);
      setDetailError(null);
      return;
    }
    void loadRunbookDetail(selectedRunbookKind, selectedAnchorId, settings);
  }, [loadRunbookDetail, selectedAnchorId, selectedRunbookKind, settings]);

  useEffect(() => {
    return () => {
      if (refreshTimerRef.current) {
        window.clearTimeout(refreshTimerRef.current);
      }
    };
  }, []);

  const updateFilters = useCallback((patch: Partial<RunbookFilters>) => {
    setFilters((current) => ({
      ...current,
      ...patch,
    }));
  }, []);

  const resetFilters = useCallback(() => {
    setFilters(DEFAULT_FILTERS);
  }, []);

  const selectRunbook = useCallback((runbookKind: string, anchorId: string) => {
    setSelectedRunbookKind(runbookKind);
    setSelectedAnchorId(anchorId);
  }, []);

  const openRunbook = useCallback(
    (runbookKind: string, anchorId: string): boolean => {
      if (!enabled || !runbookKind.trim() || !anchorId.trim()) {
        if (!enabled) {
          setNotice({
            tone: "info",
            message: "Runbook hub is disabled in Config.",
          });
        }
        return false;
      }
      setSelectedRunbookKind(runbookKind);
      setSelectedAnchorId(anchorId);
      setOpenRequestVersion((current) => current + 1);
      return true;
    },
    [enabled, setNotice]
  );

  const openTaskRunbook = useCallback(
    (taskId: string) => openRunbook("strategy_task_execution", taskId),
    [openRunbook]
  );
  const openBoardCardRunbook = useCallback(
    (cardId: string) => openRunbook("board_card_run", cardId),
    [openRunbook]
  );
  const openJobRunbook = useCallback(
    (jobId: string) => openRunbook("scheduled_job_run", jobId),
    [openRunbook]
  );
  const openAssistantRunbook = useCallback(
    (runId: string) => openRunbook("assistant_session_run", runId),
    [openRunbook]
  );

  const isStale =
    lastRefreshAtMs !== null && Date.now() - lastRefreshAtMs > RUNBOOK_STALE_THRESHOLD_MS;
  const selectedOwner =
    detail?.owner_agent_id != null
      ? agentsById.get(detail.owner_agent_id) ?? null
      : null;
  const getTaskSummary = useCallback(
    (taskId: string) => summaryIndex.byTaskId.get(taskId) ?? null,
    [summaryIndex]
  );
  const getBoardCardSummary = useCallback(
    (cardId: string) => summaryIndex.byBoardCardId.get(cardId) ?? null,
    [summaryIndex]
  );
  const getJobSummary = useCallback(
    (jobId: string) => summaryIndex.byJobId.get(jobId) ?? null,
    [summaryIndex]
  );
  const getRunSummary = useCallback(
    (runId: string) => summaryIndex.byRunId.get(runId) ?? null,
    [summaryIndex]
  );
  const getSessionSummary = useCallback(
    (sessionId: string) => summaryIndex.bySessionId.get(sessionId) ?? null,
    [summaryIndex]
  );
  const getApprovalSummary = useCallback(
    (approvalId: string) => summaryIndex.byApprovalId.get(approvalId) ?? null,
    [summaryIndex]
  );
  const findSummariesForEntity = useCallback(
    (entityKind: string, entityId: string) =>
      getRunbookSummariesForEntity(summaryIndex, entityKind, entityId),
    [summaryIndex]
  );
  const findFirstSummaryForEntity = useCallback(
    (entityKind: string, entityId: string) =>
      getRunbookSummariesForEntity(summaryIndex, entityKind, entityId)[0] ?? null,
    [summaryIndex]
  );

  return {
    enabled,
    availability,
    availabilityMessage,
    filters,
    setFilters: updateFilters,
    resetFilters,
    items,
    allItems,
    countsByStatus,
    allCountsByStatus,
    generatedAtMs,
    lastRefreshAtMs,
    isStale,
    selectedRunbookKind,
    selectedAnchorId,
    selectedRunbookId,
    openRequestVersion,
    selectedSummary,
    detail,
    detailError,
    detailLoading,
    selectedOwner,
    summaryIndex,
    loadRunbookData,
    loadRunbookDetail,
    queueRefresh,
    selectRunbook,
    openRunbook,
    openTaskRunbook,
    openBoardCardRunbook,
    openJobRunbook,
    openAssistantRunbook,
    getTaskSummary,
    getBoardCardSummary,
    getJobSummary,
    getRunSummary,
    getSessionSummary,
    getApprovalSummary,
    findSummariesForEntity,
    findFirstSummaryForEntity,
  };
}
