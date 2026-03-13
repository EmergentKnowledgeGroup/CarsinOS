import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NotifyFn } from "../../app/useAppController";
import {
  getAgentMemoryAtom,
  getAgentMemoryCard,
  getAgentMemoryCitation,
  getAgentMemoryDecisionReasons,
  getAgentMemoryGraphMap,
  getAgentMemoryGraphNeighbors,
  getAgentMemoryRuntimeHealth,
  getAgentMemoryStatus,
  getAgentMemoryTelemetrySummary,
  getAgentMemoryTelemetryTurns,
  getAgentMemoryTurnWhy,
  listAgentMemoryCards,
  listAgentMemoryEpisodes,
} from "../../lib/api";
import type {
  Agent,
  AgentMemoryAtomDetailPayload,
  AgentMemoryCardDetailPayload,
  AgentMemoryCardsPayload,
  AgentMemoryCitationPayload,
  AgentMemoryDecisionReasonsPayload,
  AgentMemoryEpisodesPayload,
  AgentMemoryGraphMapPayload,
  AgentMemoryGraphNeighborsPayload,
  AgentMemoryJsonPayloadResponse,
  AgentMemoryRuntimeHealthPayload,
  AgentMemoryStatusResponse,
  AgentMemoryTelemetrySummaryPayload,
  AgentMemoryTelemetryTurnsPayload,
  AgentMemoryTurnWhyPayload,
  RuntimeConnectionSettings,
} from "../../types";
import {
  MEMORY_CARDS_PAGE_LIMIT,
  MEMORY_GRAPH_MAP_LIMIT,
  MEMORY_GRAPH_NEIGHBOR_DEPTH,
  MEMORY_GRAPH_NEIGHBOR_LINK_LIMIT,
  MEMORY_GRAPH_NEIGHBOR_NODE_LIMIT,
  MEMORY_TELEMETRY_LIMIT,
} from "./memoryConfig";
import {
  canLoadMemoryReadSurfaces,
  getMemoryBindingCacheKey,
  isMemoryUnsupportedError,
  normalizeMemoryErrorMessage,
  selectMemoryAgentId,
} from "./memoryModel";

export type MemoryAvailability =
  | "disabled"
  | "loading"
  | "ready"
  | "unsupported"
  | "error";

interface UseMemoryControllerOptions {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  enabled: boolean;
  preferredAgentId?: string | null;
  setNotice: NotifyFn;
}

interface MemoryCoreBundle {
  cardsResponse: AgentMemoryJsonPayloadResponse<AgentMemoryCardsPayload> | null;
  episodesResponse: AgentMemoryJsonPayloadResponse<AgentMemoryEpisodesPayload> | null;
  graphMapResponse: AgentMemoryJsonPayloadResponse<AgentMemoryGraphMapPayload> | null;
  runtimeHealthResponse: AgentMemoryJsonPayloadResponse<AgentMemoryRuntimeHealthPayload> | null;
  telemetrySummaryResponse:
    | AgentMemoryJsonPayloadResponse<AgentMemoryTelemetrySummaryPayload>
    | null;
  telemetryTurnsResponse:
    | AgentMemoryJsonPayloadResponse<AgentMemoryTelemetryTurnsPayload>
    | null;
  decisionReasonsResponse:
    | AgentMemoryJsonPayloadResponse<AgentMemoryDecisionReasonsPayload>
    | null;
}

interface MemoryBindingCache extends MemoryCoreBundle {
  status: AgentMemoryStatusResponse | null;
  cardDetailsById: Record<
    string,
    AgentMemoryJsonPayloadResponse<AgentMemoryCardDetailPayload>
  >;
  atomDetailsById: Record<
    string,
    AgentMemoryJsonPayloadResponse<AgentMemoryAtomDetailPayload>
  >;
  graphNeighborsByAtomId: Record<
    string,
    AgentMemoryJsonPayloadResponse<AgentMemoryGraphNeighborsPayload>
  >;
  whyByTurnId: Record<string, AgentMemoryJsonPayloadResponse<AgentMemoryTurnWhyPayload>>;
  citationsByToken: Record<
    string,
    AgentMemoryJsonPayloadResponse<AgentMemoryCitationPayload>
  >;
}

interface ScopedResponseState<T> {
  bindingKey: string;
  targetId: string;
  response: T | null;
}

interface ScopedErrorState {
  bindingKey: string;
  targetId: string;
  error: string | null;
}

const EMPTY_SURFACE_AVAILABILITY: AgentMemoryStatusResponse["native_surface_availability"] = {
  cards: false,
  card_detail: false,
  atom_detail: false,
  graph_overview: false,
  graph_neighbors: false,
  episodes: false,
  turn_why: false,
  citation_lookup: false,
  runtime_health: false,
  telemetry_summary: false,
  telemetry_turns: false,
  decision_reasons: false,
};

const EMPTY_SCOPED_ERROR: ScopedErrorState = {
  bindingKey: "",
  targetId: "",
  error: null,
};

function createEmptyCoreBundle(): MemoryCoreBundle {
  return {
    cardsResponse: null,
    episodesResponse: null,
    graphMapResponse: null,
    runtimeHealthResponse: null,
    telemetrySummaryResponse: null,
    telemetryTurnsResponse: null,
    decisionReasonsResponse: null,
  };
}

function createEmptyBindingCache(): MemoryBindingCache {
  return {
    status: null,
    ...createEmptyCoreBundle(),
    cardDetailsById: {},
    atomDetailsById: {},
    graphNeighborsByAtomId: {},
    whyByTurnId: {},
    citationsByToken: {},
  };
}

function createEmptyScopedResponse<T>(): ScopedResponseState<T> {
  return {
    bindingKey: "",
    targetId: "",
    response: null,
  };
}

function pickTurnId(value: Record<string, unknown>): string | null {
  const candidates = [value.turn_id, value.id, value.turnId];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate.trim()) {
      return candidate.trim();
    }
  }
  return null;
}

function pickCitationToken(value: Record<string, unknown>): string | null {
  const candidates = [value.citation_token, value.token, value.id];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate.trim()) {
      return candidate.trim();
    }
  }
  return null;
}

function pickAtomId(value: Record<string, unknown> | null | undefined): string | null {
  if (!value) {
    return null;
  }
  const candidate = value.atom_id;
  return typeof candidate === "string" && candidate.trim() ? candidate.trim() : null;
}

function resolveFirstCardId(cards: AgentMemoryCardsPayload["cards"]): string {
  return cards.find((card) => typeof card.card_id === "string" && card.card_id.trim())?.card_id ?? "";
}

function resolveFirstGraphAtomId(nodes: AgentMemoryGraphMapPayload["nodes"]): string {
  return nodes.find((node) => typeof node.atom_id === "string" && node.atom_id.trim())?.atom_id ?? "";
}

function resolveFirstTurnId(turns: AgentMemoryTelemetryTurnsPayload["turns"]): string {
  return (
    turns
      .map((item) => pickTurnId(item))
      .find((item): item is string => Boolean(item)) ?? ""
  );
}

function resolveFirstCitationToken(citations: AgentMemoryTurnWhyPayload["why"]["citations"]): string {
  return (
    (citations ?? [])
      .map((citation) => pickCitationToken(citation))
      .find((item): item is string => Boolean(item)) ?? ""
  );
}

function scopedResponseFor<T>(
  state: ScopedResponseState<T>,
  bindingKey: string,
  targetId: string,
  fallback: T | null
): T | null {
  if (!targetId) {
    return null;
  }
  if (state.bindingKey === bindingKey && state.targetId === targetId) {
    return state.response;
  }
  return fallback;
}

function scopedErrorFor(
  state: ScopedErrorState,
  bindingKey: string,
  targetId: string
): string | null {
  if (!targetId) {
    return null;
  }
  if (state.bindingKey === bindingKey && state.targetId === targetId) {
    return state.error;
  }
  return null;
}

export function useMemoryController(options: UseMemoryControllerOptions) {
  const { settings, agents, enabled, preferredAgentId, setNotice } = options;
  const [availability, setAvailability] = useState<MemoryAvailability>(
    enabled ? "loading" : "disabled"
  );
  const [availabilityMessage, setAvailabilityMessage] = useState<string | null>(null);
  const [status, setStatus] = useState<AgentMemoryStatusResponse | null>(null);
  const [cardsResponse, setCardsResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryCardsPayload> | null>(null);
  const [episodesResponse, setEpisodesResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryEpisodesPayload> | null>(null);
  const [graphMapResponse, setGraphMapResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryGraphMapPayload> | null>(null);
  const [runtimeHealthResponse, setRuntimeHealthResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryRuntimeHealthPayload> | null>(null);
  const [telemetrySummaryResponse, setTelemetrySummaryResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryTelemetrySummaryPayload> | null>(null);
  const [telemetryTurnsResponse, setTelemetryTurnsResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryTelemetryTurnsPayload> | null>(null);
  const [decisionReasonsResponse, setDecisionReasonsResponse] =
    useState<AgentMemoryJsonPayloadResponse<AgentMemoryDecisionReasonsPayload> | null>(null);
  const [cardDetailState, setCardDetailState] = useState<
    ScopedResponseState<AgentMemoryJsonPayloadResponse<AgentMemoryCardDetailPayload>>
  >(createEmptyScopedResponse);
  const [atomDetailState, setAtomDetailState] = useState<
    ScopedResponseState<AgentMemoryJsonPayloadResponse<AgentMemoryAtomDetailPayload>>
  >(createEmptyScopedResponse);
  const [graphNeighborsState, setGraphNeighborsState] = useState<
    ScopedResponseState<AgentMemoryJsonPayloadResponse<AgentMemoryGraphNeighborsPayload>>
  >(createEmptyScopedResponse);
  const [turnWhyState, setTurnWhyState] = useState<
    ScopedResponseState<AgentMemoryJsonPayloadResponse<AgentMemoryTurnWhyPayload>>
  >(createEmptyScopedResponse);
  const [citationState, setCitationState] = useState<
    ScopedResponseState<AgentMemoryJsonPayloadResponse<AgentMemoryCitationPayload>>
  >(createEmptyScopedResponse);
  const [selectedAgentIdState, setSelectedAgentIdState] = useState("");
  const [selectedCardIdState, setSelectedCardIdState] = useState("");
  const [selectedAtomIdState, setSelectedAtomIdState] = useState("");
  const [selectedGraphAtomIdState, setSelectedGraphAtomIdState] = useState("");
  const [selectedTurnIdState, setSelectedTurnIdState] = useState("");
  const [selectedCitationTokenState, setSelectedCitationTokenState] = useState("");
  const [cardQuery, setCardQuery] = useState("");
  const [cardStatusFilter, setCardStatusFilter] = useState("all");
  const [episodeQuery, setEpisodeQuery] = useState("");
  const [cardDetailErrorState, setCardDetailErrorState] =
    useState<ScopedErrorState>(EMPTY_SCOPED_ERROR);
  const [atomDetailErrorState, setAtomDetailErrorState] =
    useState<ScopedErrorState>(EMPTY_SCOPED_ERROR);
  const [graphErrorState, setGraphErrorState] = useState<ScopedErrorState>(EMPTY_SCOPED_ERROR);
  const [whyErrorState, setWhyErrorState] = useState<ScopedErrorState>(EMPTY_SCOPED_ERROR);
  const [citationErrorState, setCitationErrorState] =
    useState<ScopedErrorState>(EMPTY_SCOPED_ERROR);
  const cacheRef = useRef<Map<string, MemoryBindingCache>>(new Map());
  const [cacheSnapshot, setCacheSnapshot] = useState<Map<string, MemoryBindingCache>>(
    () => new Map()
  );
  const listRequestIdRef = useRef(0);
  const cardRequestIdRef = useRef(0);
  const atomRequestIdRef = useRef(0);
  const graphRequestIdRef = useRef(0);
  const whyRequestIdRef = useRef(0);
  const citationRequestIdRef = useRef(0);

  const selectedAgentId = useMemo(
    () => selectMemoryAgentId(agents, preferredAgentId, selectedAgentIdState),
    [agents, preferredAgentId, selectedAgentIdState]
  );
  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.agent_id === selectedAgentId) ?? null,
    [agents, selectedAgentId]
  );
  const bindingKey = useMemo(
    () => getMemoryBindingCacheKey(selectedAgentId, status),
    [selectedAgentId, status]
  );
  const nativeSurfaceAvailability =
    status?.native_surface_availability ?? EMPTY_SURFACE_AVAILABILITY;
  const canRead = canLoadMemoryReadSurfaces(status);

  const cards = useMemo(() => cardsResponse?.data.cards ?? [], [cardsResponse]);
  const episodes = useMemo(() => episodesResponse?.data.episodes ?? [], [episodesResponse]);
  const graphNodes = useMemo(() => graphMapResponse?.data.nodes ?? [], [graphMapResponse]);
  const graphLinks = useMemo(() => graphMapResponse?.data.links ?? [], [graphMapResponse]);
  const telemetryTurns = useMemo(
    () => telemetryTurnsResponse?.data.turns ?? [],
    [telemetryTurnsResponse]
  );
  const telemetrySummary = useMemo(
    () => telemetrySummaryResponse?.data.summary ?? [],
    [telemetrySummaryResponse]
  );
  const decisionReasons = useMemo(
    () => decisionReasonsResponse?.data.reasons ?? [],
    [decisionReasonsResponse]
  );

  const selectedCardId = useMemo(() => {
    if (
      selectedCardIdState &&
      cards.some((card) => card.card_id === selectedCardIdState)
    ) {
      return selectedCardIdState;
    }
    return resolveFirstCardId(cards);
  }, [cards, selectedCardIdState]);

  const selectedCard = useMemo(
    () => cards.find((card) => card.card_id === selectedCardId) ?? null,
    [cards, selectedCardId]
  );

  const selectedGraphAtomId = useMemo(() => {
    if (
      selectedGraphAtomIdState &&
      graphNodes.some((node) => node.atom_id === selectedGraphAtomIdState)
    ) {
      return selectedGraphAtomIdState;
    }
    if (
      selectedCard?.atom_id &&
      graphNodes.some((node) => node.atom_id === selectedCard.atom_id)
    ) {
      return selectedCard.atom_id;
    }
    return resolveFirstGraphAtomId(graphNodes);
  }, [graphNodes, selectedCard, selectedGraphAtomIdState]);

  const selectedTurnId = useMemo(() => {
    if (
      selectedTurnIdState &&
      telemetryTurns.some((item) => pickTurnId(item) === selectedTurnIdState)
    ) {
      return selectedTurnIdState;
    }
    return resolveFirstTurnId(telemetryTurns);
  }, [selectedTurnIdState, telemetryTurns]);

  const applyCoreBundle = useCallback((bundle: MemoryCoreBundle) => {
    setCardsResponse(bundle.cardsResponse);
    setEpisodesResponse(bundle.episodesResponse);
    setGraphMapResponse(bundle.graphMapResponse);
    setRuntimeHealthResponse(bundle.runtimeHealthResponse);
    setTelemetrySummaryResponse(bundle.telemetrySummaryResponse);
    setTelemetryTurnsResponse(bundle.telemetryTurnsResponse);
    setDecisionReasonsResponse(bundle.decisionReasonsResponse);
  }, []);

  const clearCoreBundle = useCallback(() => {
    applyCoreBundle(createEmptyCoreBundle());
  }, [applyCoreBundle]);

  const updateCache = useCallback(
    (cacheKey: string, updater: (current: MemoryBindingCache) => MemoryBindingCache) => {
      const current = cacheRef.current.get(cacheKey) ?? createEmptyBindingCache();
      const nextCache = new Map(cacheRef.current);
      nextCache.set(cacheKey, updater(current));
      cacheRef.current = nextCache;
      setCacheSnapshot(nextCache);
    },
    []
  );

  const resetLaneSelections = useCallback(() => {
    setSelectedCardIdState("");
    setSelectedAtomIdState("");
    setSelectedGraphAtomIdState("");
    setSelectedTurnIdState("");
    setSelectedCitationTokenState("");
  }, []);

  const setSelectedAgentId = useCallback(
    (nextAgentId: string) => {
      setSelectedAgentIdState(nextAgentId);
      resetLaneSelections();
    },
    [resetLaneSelections]
  );

  const setSelectedCardId = useCallback((nextCardId: string) => {
    setSelectedCardIdState(nextCardId);
  }, []);

  const setSelectedAtomId = useCallback((nextAtomId: string) => {
    setSelectedAtomIdState(nextAtomId);
  }, []);

  const setSelectedGraphAtomId = useCallback((nextAtomId: string) => {
    setSelectedGraphAtomIdState(nextAtomId);
  }, []);

  const setSelectedTurnId = useCallback((nextTurnId: string) => {
    setSelectedTurnIdState(nextTurnId);
    setSelectedCitationTokenState("");
  }, []);

  const setSelectedCitationToken = useCallback((nextToken: string) => {
    setSelectedCitationTokenState(nextToken);
  }, []);

  const loadMemoryData = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      targetAgentId = selectedAgentId
    ) => {
      if (!enabled) {
        setAvailability("disabled");
        setAvailabilityMessage("Memory hub is disabled in Config.");
        setStatus(null);
        clearCoreBundle();
        return;
      }

      if (!targetAgentId.trim()) {
        setAvailability("loading");
        setAvailabilityMessage("Waiting for an assistant selection.");
        setStatus(null);
        clearCoreBundle();
        return;
      }

      const requestId = ++listRequestIdRef.current;
      setAvailability((current) => (current === "ready" ? current : "loading"));
      setAvailabilityMessage(null);

      try {
        const nextStatus = await getAgentMemoryStatus(runtimeSettings, targetAgentId);
        if (listRequestIdRef.current !== requestId) {
          return;
        }

        const nextBindingKey = getMemoryBindingCacheKey(targetAgentId, nextStatus);
        const cached = cacheRef.current.get(nextBindingKey);

        setStatus(nextStatus);
        setAvailability("ready");
        setAvailabilityMessage(null);
        if (cached) {
          applyCoreBundle(cached);
        } else {
          clearCoreBundle();
        }
        updateCache(nextBindingKey, (current) => ({
          ...current,
          status: nextStatus,
        }));

        if (!canLoadMemoryReadSurfaces(nextStatus)) {
          clearCoreBundle();
          return;
        }

        const [
          nextCardsResponse,
          nextEpisodesResponse,
          nextGraphMapResponse,
          nextRuntimeHealthResponse,
          nextTelemetrySummaryResponse,
          nextTelemetryTurnsResponse,
          nextDecisionReasonsResponse,
        ] = await Promise.all([
          nextStatus.native_surface_availability.cards
            ? listAgentMemoryCards(runtimeSettings, targetAgentId, {
                status: cardStatusFilter,
                q: cardQuery.trim() || undefined,
                limit: MEMORY_CARDS_PAGE_LIMIT,
              })
            : Promise.resolve(null),
          nextStatus.native_surface_availability.episodes
            ? listAgentMemoryEpisodes(runtimeSettings, targetAgentId, {
                status: "all",
                q: episodeQuery.trim() || undefined,
              })
            : Promise.resolve(null),
          nextStatus.native_surface_availability.graph_overview
            ? getAgentMemoryGraphMap(runtimeSettings, targetAgentId, {
                status: "all",
                q: cardQuery.trim() || undefined,
                limit: MEMORY_GRAPH_MAP_LIMIT,
              })
            : Promise.resolve(null),
          nextStatus.native_surface_availability.runtime_health
            ? getAgentMemoryRuntimeHealth(runtimeSettings, targetAgentId)
            : Promise.resolve(null),
          nextStatus.native_surface_availability.telemetry_summary
            ? getAgentMemoryTelemetrySummary(runtimeSettings, targetAgentId, {
                limit: MEMORY_TELEMETRY_LIMIT,
              })
            : Promise.resolve(null),
          nextStatus.native_surface_availability.telemetry_turns
            ? getAgentMemoryTelemetryTurns(runtimeSettings, targetAgentId, {
                limit: MEMORY_TELEMETRY_LIMIT,
              })
            : Promise.resolve(null),
          nextStatus.native_surface_availability.decision_reasons
            ? getAgentMemoryDecisionReasons(runtimeSettings, targetAgentId)
            : Promise.resolve(null),
        ]);

        if (listRequestIdRef.current !== requestId) {
          return;
        }

        const nextBundle: MemoryCoreBundle = {
          cardsResponse: nextCardsResponse,
          episodesResponse: nextEpisodesResponse,
          graphMapResponse: nextGraphMapResponse,
          runtimeHealthResponse: nextRuntimeHealthResponse,
          telemetrySummaryResponse: nextTelemetrySummaryResponse,
          telemetryTurnsResponse: nextTelemetryTurnsResponse,
          decisionReasonsResponse: nextDecisionReasonsResponse,
        };
        applyCoreBundle(nextBundle);
        updateCache(nextBindingKey, (current) => ({
          ...current,
          ...nextBundle,
        }));
      } catch (error: unknown) {
        if (listRequestIdRef.current !== requestId) {
          return;
        }
        if (isMemoryUnsupportedError(error)) {
          setAvailability("unsupported");
          setAvailabilityMessage(
            "The connected gateway does not expose the Memory surface yet."
          );
        } else {
          setAvailability("error");
          setAvailabilityMessage(normalizeMemoryErrorMessage(error));
        }
        setStatus(null);
        clearCoreBundle();
      }
    },
    [
      applyCoreBundle,
      cardQuery,
      cardStatusFilter,
      clearCoreBundle,
      enabled,
      episodeQuery,
      selectedAgentId,
      settings,
      updateCache,
    ]
  );

  const refresh = useCallback(async () => {
    try {
      await loadMemoryData(settings, selectedAgentId);
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Memory refresh failed: ${normalizeMemoryErrorMessage(error)}`,
      });
    }
  }, [loadMemoryData, selectedAgentId, setNotice, settings]);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled) {
        void loadMemoryData(settings, selectedAgentId);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [enabled, loadMemoryData, selectedAgentId, settings]);

  const cachedCardDetailResponse = selectedCardId
    ? cacheSnapshot.get(bindingKey)?.cardDetailsById[selectedCardId] ?? null
    : null;

  const cardDetailResponse = scopedResponseFor(
    cardDetailState,
    bindingKey,
    selectedCardId,
    cachedCardDetailResponse
  );

  useEffect(() => {
    if (!canRead || !selectedCardId || !nativeSurfaceAvailability.card_detail) {
      return;
    }

    const requestId = ++cardRequestIdRef.current;
    void getAgentMemoryCard(settings, selectedAgentId, selectedCardId)
      .then((response) => {
        if (cardRequestIdRef.current !== requestId) {
          return;
        }
        setCardDetailState({
          bindingKey,
          targetId: selectedCardId,
          response,
        });
        setCardDetailErrorState({
          bindingKey,
          targetId: selectedCardId,
          error: null,
        });
        updateCache(bindingKey, (current) => ({
          ...current,
          cardDetailsById: {
            ...current.cardDetailsById,
            [selectedCardId]: response,
          },
        }));
      })
      .catch((error: unknown) => {
        if (cardRequestIdRef.current !== requestId) {
          return;
        }
        setCardDetailErrorState({
          bindingKey,
          targetId: selectedCardId,
          error: normalizeMemoryErrorMessage(error),
        });
      });
  }, [
    bindingKey,
    canRead,
    nativeSurfaceAvailability.card_detail,
    selectedAgentId,
    selectedCardId,
    settings,
    updateCache,
  ]);

  const selectedAtomId = useMemo(() => {
    if (selectedAtomIdState.trim()) {
      return selectedAtomIdState.trim();
    }
    if (selectedCard?.atom_id) {
      return selectedCard.atom_id;
    }
    return (
      pickAtomId(cardDetailResponse?.data.atom as Record<string, unknown> | null) ??
      selectedGraphAtomId
    );
  }, [cardDetailResponse, selectedAtomIdState, selectedCard?.atom_id, selectedGraphAtomId]);

  const cachedAtomDetailResponse = selectedAtomId
    ? cacheSnapshot.get(bindingKey)?.atomDetailsById[selectedAtomId] ?? null
    : null;

  const atomDetailResponse = scopedResponseFor(
    atomDetailState,
    bindingKey,
    selectedAtomId,
    cachedAtomDetailResponse
  );

  useEffect(() => {
    if (!canRead || !selectedAtomId || !nativeSurfaceAvailability.atom_detail) {
      return;
    }

    const requestId = ++atomRequestIdRef.current;
    void getAgentMemoryAtom(settings, selectedAgentId, selectedAtomId)
      .then((response) => {
        if (atomRequestIdRef.current !== requestId) {
          return;
        }
        setAtomDetailState({
          bindingKey,
          targetId: selectedAtomId,
          response,
        });
        setAtomDetailErrorState({
          bindingKey,
          targetId: selectedAtomId,
          error: null,
        });
        updateCache(bindingKey, (current) => ({
          ...current,
          atomDetailsById: {
            ...current.atomDetailsById,
            [selectedAtomId]: response,
          },
        }));
      })
      .catch((error: unknown) => {
        if (atomRequestIdRef.current !== requestId) {
          return;
        }
        setAtomDetailErrorState({
          bindingKey,
          targetId: selectedAtomId,
          error: normalizeMemoryErrorMessage(error),
        });
      });
  }, [
    bindingKey,
    canRead,
    nativeSurfaceAvailability.atom_detail,
    selectedAgentId,
    selectedAtomId,
    settings,
    updateCache,
  ]);

  const cachedGraphNeighborsResponse = selectedGraphAtomId
    ? cacheSnapshot.get(bindingKey)?.graphNeighborsByAtomId[selectedGraphAtomId] ?? null
    : null;

  const graphNeighborsResponse = scopedResponseFor(
    graphNeighborsState,
    bindingKey,
    selectedGraphAtomId,
    cachedGraphNeighborsResponse
  );

  useEffect(() => {
    if (!canRead || !selectedGraphAtomId || !nativeSurfaceAvailability.graph_neighbors) {
      return;
    }

    const requestId = ++graphRequestIdRef.current;
    void getAgentMemoryGraphNeighbors(settings, selectedAgentId, {
      atom_id: selectedGraphAtomId,
      depth: MEMORY_GRAPH_NEIGHBOR_DEPTH,
      node_limit: MEMORY_GRAPH_NEIGHBOR_NODE_LIMIT,
      link_limit: MEMORY_GRAPH_NEIGHBOR_LINK_LIMIT,
      include_root_detail: true,
      include_shared_language: true,
    })
      .then((response) => {
        if (graphRequestIdRef.current !== requestId) {
          return;
        }
        setGraphNeighborsState({
          bindingKey,
          targetId: selectedGraphAtomId,
          response,
        });
        setGraphErrorState({
          bindingKey,
          targetId: selectedGraphAtomId,
          error: null,
        });
        updateCache(bindingKey, (current) => ({
          ...current,
          graphNeighborsByAtomId: {
            ...current.graphNeighborsByAtomId,
            [selectedGraphAtomId]: response,
          },
        }));
      })
      .catch((error: unknown) => {
        if (graphRequestIdRef.current !== requestId) {
          return;
        }
        setGraphErrorState({
          bindingKey,
          targetId: selectedGraphAtomId,
          error: normalizeMemoryErrorMessage(error),
        });
      });
  }, [
    bindingKey,
    canRead,
    nativeSurfaceAvailability.graph_neighbors,
    selectedAgentId,
    selectedGraphAtomId,
    settings,
    updateCache,
  ]);

  const selectedTurn = useMemo(
    () => telemetryTurns.find((item) => pickTurnId(item) === selectedTurnId) ?? null,
    [selectedTurnId, telemetryTurns]
  );

  const cachedTurnWhyResponse = selectedTurnId
    ? cacheSnapshot.get(bindingKey)?.whyByTurnId[selectedTurnId] ?? null
    : null;

  const turnWhyResponse = scopedResponseFor(
    turnWhyState,
    bindingKey,
    selectedTurnId,
    cachedTurnWhyResponse
  );

  useEffect(() => {
    if (!canRead || !selectedTurnId || !nativeSurfaceAvailability.turn_why) {
      return;
    }

    const requestId = ++whyRequestIdRef.current;
    void getAgentMemoryTurnWhy(settings, selectedAgentId, selectedTurnId, {
      citations: true,
    })
      .then((response) => {
        if (whyRequestIdRef.current !== requestId) {
          return;
        }
        setTurnWhyState({
          bindingKey,
          targetId: selectedTurnId,
          response,
        });
        setWhyErrorState({
          bindingKey,
          targetId: selectedTurnId,
          error: null,
        });
        updateCache(bindingKey, (current) => ({
          ...current,
          whyByTurnId: {
            ...current.whyByTurnId,
            [selectedTurnId]: response,
          },
        }));
      })
      .catch((error: unknown) => {
        if (whyRequestIdRef.current !== requestId) {
          return;
        }
        setWhyErrorState({
          bindingKey,
          targetId: selectedTurnId,
          error: normalizeMemoryErrorMessage(error),
        });
      });
  }, [
    bindingKey,
    canRead,
    nativeSurfaceAvailability.turn_why,
    selectedAgentId,
    selectedTurnId,
    settings,
    updateCache,
  ]);

  const activeWhyCitations = useMemo(
    () => turnWhyResponse?.data.why.citations ?? [],
    [turnWhyResponse]
  );

  const selectedCitationToken = useMemo(() => {
    if (
      selectedCitationTokenState &&
      activeWhyCitations.some(
        (citation) => pickCitationToken(citation) === selectedCitationTokenState
      )
    ) {
      return selectedCitationTokenState;
    }
    return resolveFirstCitationToken(activeWhyCitations);
  }, [activeWhyCitations, selectedCitationTokenState]);

  const cachedCitationResponse = selectedCitationToken
    ? cacheSnapshot.get(bindingKey)?.citationsByToken[selectedCitationToken] ?? null
    : null;

  const citationResponse = scopedResponseFor(
    citationState,
    bindingKey,
    selectedCitationToken,
    cachedCitationResponse
  );

  useEffect(() => {
    if (!canRead || !selectedCitationToken || !nativeSurfaceAvailability.citation_lookup) {
      return;
    }

    const requestId = ++citationRequestIdRef.current;
    void getAgentMemoryCitation(settings, selectedAgentId, selectedCitationToken)
      .then((response) => {
        if (citationRequestIdRef.current !== requestId) {
          return;
        }
        setCitationState({
          bindingKey,
          targetId: selectedCitationToken,
          response,
        });
        setCitationErrorState({
          bindingKey,
          targetId: selectedCitationToken,
          error: null,
        });
        updateCache(bindingKey, (current) => ({
          ...current,
          citationsByToken: {
            ...current.citationsByToken,
            [selectedCitationToken]: response,
          },
        }));
      })
      .catch((error: unknown) => {
        if (citationRequestIdRef.current !== requestId) {
          return;
        }
        setCitationErrorState({
          bindingKey,
          targetId: selectedCitationToken,
          error: normalizeMemoryErrorMessage(error),
        });
      });
  }, [
    bindingKey,
    canRead,
    nativeSurfaceAvailability.citation_lookup,
    selectedAgentId,
    selectedCitationToken,
    settings,
    updateCache,
  ]);

  const detailError =
    scopedErrorFor(cardDetailErrorState, bindingKey, selectedCardId) ??
    scopedErrorFor(atomDetailErrorState, bindingKey, selectedAtomId);
  const graphError = scopedErrorFor(graphErrorState, bindingKey, selectedGraphAtomId);
  const whyError = scopedErrorFor(whyErrorState, bindingKey, selectedTurnId);
  const citationError = scopedErrorFor(
    citationErrorState,
    bindingKey,
    selectedCitationToken
  );
  const selectedEpisode = useMemo(
    () => episodes.find((episode) => episode.episode_id === selectedCitationToken) ?? null,
    [episodes, selectedCitationToken]
  );

  return {
    enabled,
    availability,
    availabilityMessage,
    agents,
    selectedAgentId,
    setSelectedAgentId,
    selectedAgent,
    status,
    nativeSurfaceAvailability,
    bindingKey,
    cardsResponse,
    cards,
    cardQuery,
    setCardQuery,
    cardStatusFilter,
    setCardStatusFilter,
    episodesResponse,
    episodes,
    episodeQuery,
    setEpisodeQuery,
    graphMapResponse,
    graphNodes,
    graphLinks,
    runtimeHealthResponse,
    telemetrySummaryResponse,
    telemetrySummary,
    telemetryTurnsResponse,
    telemetryTurns,
    decisionReasonsResponse,
    decisionReasons,
    selectedCardId,
    setSelectedCardId,
    selectedCard,
    cardDetailResponse,
    selectedAtomId,
    setSelectedAtomId,
    atomDetailResponse,
    selectedGraphAtomId,
    setSelectedGraphAtomId,
    graphNeighborsResponse,
    selectedTurnId,
    setSelectedTurnId,
    selectedTurn,
    turnWhyResponse,
    selectedCitationToken,
    setSelectedCitationToken,
    citationResponse,
    selectedEpisode,
    detailError,
    graphError,
    whyError,
    citationError,
    canRead,
    refresh,
  };
}
