import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getAssistantDesk, getAssistantDeskTranscript } from "../../lib/api";
import type {
  AssistantDeskResponse,
  AssistantDeskTranscriptResponse,
  AssistantDeskWorkItem,
  RuntimeConnectionSettings,
} from "../../types";

const STATUS_STRIP_POLL_MS = 12_000;
const OPEN_DESK_POLL_MS = 3_000;
const STATUS_STRIP_VISIBLE_LIMIT = 4;

interface UseAssistantDeskControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  assistantDeskEnabled?: boolean;
  assistantDeskStatusStripEnabled?: boolean;
  deskOpen?: boolean;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function flattenDeskItems(desk: AssistantDeskResponse | null): AssistantDeskWorkItem[] {
  if (!desk) {
    return [];
  }
  return [
    ...desk.buckets.needs_you,
    ...desk.buckets.working,
    ...desk.buckets.done_recently,
  ];
}

export function useAssistantDeskController(options: UseAssistantDeskControllerOptions) {
  const {
    settings,
    tokenConfigured,
    assistantDeskEnabled = true,
    assistantDeskStatusStripEnabled = true,
    deskOpen = false,
  } = options;

  const [desk, setDesk] = useState<AssistantDeskResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [selectedWorkItemId, setSelectedWorkItemId] = useState<string | null>(null);
  const [transcript, setTranscript] =
    useState<AssistantDeskTranscriptResponse | null>(null);
  const [transcriptLoading, setTranscriptLoading] = useState(false);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  const inFlightRef = useRef(false);

  const pollingEnabled =
    tokenConfigured &&
    settings.gateway_url.trim().length > 0 &&
    (assistantDeskEnabled || assistantDeskStatusStripEnabled);
  const pollIntervalMs =
    deskOpen && assistantDeskEnabled ? OPEN_DESK_POLL_MS : STATUS_STRIP_POLL_MS;

  const refresh = useCallback(async () => {
    if (!pollingEnabled || inFlightRef.current) {
      return;
    }
    inFlightRef.current = true;
    setLoading(true);
    try {
      const response = await getAssistantDesk(settings);
      setDesk(response);
      setStale(response.stale);
      setError(null);
    } catch (err) {
      setStale(true);
      setError(errorMessage(err));
    } finally {
      inFlightRef.current = false;
      setLoading(false);
    }
  }, [pollingEnabled, settings]);

  useEffect(() => {
    if (!pollingEnabled) {
      return;
    }
    void refresh();
    const interval = window.setInterval(() => {
      void refresh();
    }, pollIntervalMs);
    return () => window.clearInterval(interval);
  }, [pollIntervalMs, pollingEnabled, refresh]);

  const allItems = useMemo(() => flattenDeskItems(desk), [desk]);
  const visibleStatusItems = useMemo(
    () => allItems.slice(0, STATUS_STRIP_VISIBLE_LIMIT),
    [allItems]
  );
  const overflowStatusCount = Math.max(
    0,
    allItems.length - visibleStatusItems.length
  );
  const selectedWorkItem = useMemo(
    () => allItems.find((item) => item.id === selectedWorkItemId) ?? null,
    [allItems, selectedWorkItemId]
  );

  const selectWorkItem = useCallback((workItemId: string | null) => {
    setSelectedWorkItemId(workItemId);
  }, []);

  const openTranscript = useCallback(
    async (workItemId: string, cursor?: string | null) => {
      setSelectedWorkItemId(workItemId);
      setTranscriptLoading(true);
      setTranscriptError(null);
      try {
        const response = await getAssistantDeskTranscript(settings, workItemId, cursor);
        setTranscript(response);
      } catch (err) {
        setTranscriptError(errorMessage(err));
      } finally {
        setTranscriptLoading(false);
      }
    },
    [settings]
  );

  const closeTranscript = useCallback(() => {
    setTranscript(null);
    setTranscriptError(null);
    setTranscriptLoading(false);
  }, []);

  return {
    desk,
    loading,
    error,
    stale,
    allItems,
    visibleStatusItems,
    overflowStatusCount,
    selectedWorkItemId,
    selectedWorkItem,
    transcript,
    transcriptLoading,
    transcriptError,
    pollIntervalMs,
    refresh,
    selectWorkItem,
    openTranscript,
    closeTranscript,
  };
}