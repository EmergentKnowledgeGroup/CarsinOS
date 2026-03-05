import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  LIVE_FEED_IN_MEMORY_CAP,
  LIVE_FEED_MAX_RENDERED_ROWS,
  applyLiveFeedOverflowPolicy,
  filterLiveFeedEvents,
  normalizeLiveFeedEvent,
  pruneRecoveryLog,
  type LiveFeedDomain,
  type LiveFeedEvent,
  type LiveFeedOverflowResult,
  type LiveFeedSeverityFilter,
} from "../lib/liveFeed";
import { STORAGE_KEYS } from "../storageKeys";
import type { WsEventFrame } from "../types";

export type LiveFeedStorageMode = "durable" | "memory-only";

interface UseLiveFeedControllerOptions {
  retentionWindowMs: number;
  recoveryMaxBytes: number;
  markReadUndoWindowMs: number;
  inMemoryCap?: number;
}

interface MarkAllReadUndoState {
  unread_ids: string[];
  expires_at_ms: number;
}

interface ClearUndoState {
  events: LiveFeedEvent[];
  unread_ids: string[];
  expires_at_ms: number;
}

function readStorage(): Storage | null {
  try {
    if (typeof window === "undefined") {
      return null;
    }
    return window.localStorage;
  } catch {
    return null;
  }
}

function sortNewestFirst(events: readonly LiveFeedEvent[]): LiveFeedEvent[] {
  return [...events].sort((left, right) => {
    if (left.ts_unix_ms !== right.ts_unix_ms) {
      return right.ts_unix_ms - left.ts_unix_ms;
    }
    return right.arrival_index - left.arrival_index;
  });
}

function parseRecoveryLog(raw: string | null): LiveFeedEvent[] {
  if (!raw) {
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter((item): item is LiveFeedEvent => {
      return (
        typeof item === "object" &&
        item !== null &&
        typeof (item as LiveFeedEvent).event_id === "string"
      );
    });
  } catch {
    return [];
  }
}

function loadInitialState(
  options: UseLiveFeedControllerOptions,
  inMemoryCap: number
): {
  storageMode: LiveFeedStorageMode;
  storageError: string | null;
  recoveryLog: LiveFeedEvent[];
  events: LiveFeedEvent[];
  maxArrivalIndex: number;
} {
  const storage = readStorage();
  if (!storage) {
    return {
      storageMode: "memory-only",
      storageError: "Local storage unavailable; recovery log is memory-only.",
      recoveryLog: [],
      events: [],
      maxArrivalIndex: 0,
    };
  }

  const parsed = parseRecoveryLog(storage.getItem(STORAGE_KEYS.liveFeedRecoveryV1));
  const pruned = pruneRecoveryLog(parsed, {
    now_ms: Date.now(),
    retention_window_ms: options.retentionWindowMs,
    max_bytes: options.recoveryMaxBytes,
  });
  const newest = sortNewestFirst(pruned);
  return {
    storageMode: "durable",
    storageError: null,
    recoveryLog: pruned,
    events: applyLiveFeedOverflowPolicy(newest, inMemoryCap).events,
    maxArrivalIndex: newest.length > 0 ? Math.max(...newest.map((event) => event.arrival_index)) : 0,
  };
}

export function useLiveFeedController(options: UseLiveFeedControllerOptions) {
  const inMemoryCap = options.inMemoryCap ?? LIVE_FEED_IN_MEMORY_CAP;
  const [initialState] = useState(() => loadInitialState(options, inMemoryCap));

  const arrivalIndexRef = useRef(initialState.maxArrivalIndex);
  const recoveryLogRef = useRef<LiveFeedEvent[]>(initialState.recoveryLog);

  const [storageMode, setStorageMode] = useState<LiveFeedStorageMode>(initialState.storageMode);
  const [storageError, setStorageError] = useState<string | null>(initialState.storageError);
  const [events, setEvents] = useState<LiveFeedEvent[]>(initialState.events);
  const [recoveryLog, setRecoveryLog] = useState<LiveFeedEvent[]>(initialState.recoveryLog);
  const [overflowDropped, setOverflowDropped] = useState<Record<string, number>>({
    critical: 0,
    high: 0,
    normal: 0,
    low: 0,
  });
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [paused, setPaused] = useState(false);
  const [domainFilter, setDomainFilter] = useState<LiveFeedDomain>("all");
  const [severityFilter, setSeverityFilter] = useState<LiveFeedSeverityFilter>("all");
  const [unreadIds, setUnreadIds] = useState<Set<string>>(new Set());
  const [markAllUndo, setMarkAllUndo] = useState<MarkAllReadUndoState | null>(null);
  const [clearUndo, setClearUndo] = useState<ClearUndoState | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    const timer = globalThis.setInterval(() => {
      setNowMs(Date.now());
    }, 1_000);
    return () => {
      globalThis.clearInterval(timer);
    };
  }, []);

  const persistRecoveryLog = useCallback((next: LiveFeedEvent[]) => {
    recoveryLogRef.current = next;
    setRecoveryLog(next);

    const storage = readStorage();
    if (!storage) {
      setStorageMode("memory-only");
      setStorageError("Local storage unavailable; recovery log is memory-only.");
      return;
    }

    try {
      storage.setItem(STORAGE_KEYS.liveFeedRecoveryV1, JSON.stringify(next));
      setStorageMode("durable");
      setStorageError(null);
    } catch {
      setStorageMode("memory-only");
      setStorageError("Recovery log persistence failed; fallback to memory-only mode.");
    }
  }, []);

  const ingestWsFrame = useCallback(
    (frame: WsEventFrame): LiveFeedEvent => {
      arrivalIndexRef.current += 1;
      const normalized = normalizeLiveFeedEvent(frame, arrivalIndexRef.current);

      setEvents((previous) => {
        const deduped = previous.filter((item) => item.event_id !== normalized.event_id);
        const overflow: LiveFeedOverflowResult = applyLiveFeedOverflowPolicy(
          [normalized, ...deduped],
          inMemoryCap
        );
        setOverflowDropped((current) => ({
          critical: (current.critical ?? 0) + overflow.dropped.critical,
          high: (current.high ?? 0) + overflow.dropped.high,
          normal: (current.normal ?? 0) + overflow.dropped.normal,
          low: (current.low ?? 0) + overflow.dropped.low,
        }));
        return overflow.events;
      });

      if (!(drawerOpen && !paused)) {
        setUnreadIds((previous) => {
          const next = new Set(previous);
          next.add(normalized.event_id);
          return next;
        });
      }

      const nextRecovery = pruneRecoveryLog([...recoveryLogRef.current, normalized], {
        now_ms: Date.now(),
        retention_window_ms: options.retentionWindowMs,
        max_bytes: options.recoveryMaxBytes,
      });
      persistRecoveryLog(nextRecovery);

      return normalized;
    },
    [
      drawerOpen,
      inMemoryCap,
      options.recoveryMaxBytes,
      options.retentionWindowMs,
      paused,
      persistRecoveryLog,
    ]
  );

  const toggleDrawer = useCallback(() => {
    setDrawerOpen((open) => {
      const next = !open;
      if (next && !paused) {
        setUnreadIds(new Set());
      }
      return next;
    });
  }, [paused]);

  const setDrawerOpenWithReadSync = useCallback(
    (next: boolean) => {
      setDrawerOpen(next);
      if (next && !paused) {
        setUnreadIds(new Set());
      }
    },
    [paused]
  );

  const togglePause = useCallback(() => {
    setPaused((value) => {
      const next = !value;
      if (!next && drawerOpen) {
        setUnreadIds(new Set());
      }
      return next;
    });
  }, [drawerOpen]);

  const markAllRead = useCallback(() => {
    const currentUnread = [...unreadIds];
    if (currentUnread.length === 0) {
      return;
    }
    setMarkAllUndo({
      unread_ids: currentUnread,
      expires_at_ms: Date.now() + options.markReadUndoWindowMs,
    });
    setUnreadIds(new Set());
  }, [options.markReadUndoWindowMs, unreadIds]);

  const undoMarkAllRead = useCallback(() => {
    if (!markAllUndo || markAllUndo.expires_at_ms < Date.now()) {
      setMarkAllUndo(null);
      return;
    }
    setUnreadIds((previous) => {
      const next = new Set(previous);
      for (const id of markAllUndo.unread_ids) {
        next.add(id);
      }
      return next;
    });
    setMarkAllUndo(null);
  }, [markAllUndo]);

  const clearFeedSoft = useCallback(() => {
    const now = Date.now();
    setClearUndo({
      events,
      unread_ids: [...unreadIds],
      expires_at_ms: now + options.retentionWindowMs,
    });
    setEvents([]);
    setUnreadIds(new Set());
  }, [events, options.retentionWindowMs, unreadIds]);

  const restoreFromClearUndo = useCallback(() => {
    if (!clearUndo || clearUndo.expires_at_ms < Date.now()) {
      setClearUndo(null);
      return;
    }
    setEvents((previous) =>
      applyLiveFeedOverflowPolicy([...clearUndo.events, ...previous], inMemoryCap).events
    );
    setUnreadIds(new Set(clearUndo.unread_ids));
    setClearUndo(null);
  }, [clearUndo, inMemoryCap]);

  const restoreFromRecoveryLog = useCallback(() => {
    const fromStorage = pruneRecoveryLog(recoveryLog, {
      now_ms: Date.now(),
      retention_window_ms: options.retentionWindowMs,
      max_bytes: options.recoveryMaxBytes,
    });
    persistRecoveryLog(fromStorage);
    const newest = sortNewestFirst(fromStorage);
    setEvents(applyLiveFeedOverflowPolicy(newest, inMemoryCap).events);
  }, [
    inMemoryCap,
    options.recoveryMaxBytes,
    options.retentionWindowMs,
    persistRecoveryLog,
    recoveryLog,
  ]);

  const filteredEvents = useMemo(
    () => filterLiveFeedEvents(events, domainFilter, severityFilter),
    [domainFilter, events, severityFilter]
  );

  const renderEvents = useMemo(
    () => filteredEvents.slice(0, LIVE_FEED_MAX_RENDERED_ROWS),
    [filteredEvents]
  );

  const unreadCount = unreadIds.size;
  const markAllUndoAvailable = Boolean(markAllUndo && markAllUndo.expires_at_ms >= nowMs);
  const clearUndoAvailable = Boolean(clearUndo && clearUndo.expires_at_ms >= nowMs);
  const recoveryAvailableCount = useMemo(() => {
    return pruneRecoveryLog(recoveryLog, {
      now_ms: nowMs,
      retention_window_ms: options.retentionWindowMs,
      max_bytes: options.recoveryMaxBytes,
    }).length;
  }, [nowMs, options.recoveryMaxBytes, options.retentionWindowMs, recoveryLog]);

  return {
    events,
    filteredEvents,
    renderEvents,
    overflowDropped,
    domainFilter,
    setDomainFilter,
    severityFilter,
    setSeverityFilter,
    drawerOpen,
    setDrawerOpen: setDrawerOpenWithReadSync,
    toggleDrawer,
    paused,
    togglePause,
    unreadCount,
    markAllRead,
    undoMarkAllRead,
    markAllUndoAvailable,
    clearFeedSoft,
    restoreFromClearUndo,
    clearUndoAvailable,
    restoreFromRecoveryLog,
    recoveryAvailableCount,
    ingestWsFrame,
    storageMode,
    storageError,
  };
}
