import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NotifyFn } from "../../app/useAppController";
import {
  getAgentMemoryAtom,
  getAgentMemoryCard,
  getAgentMemoryCitation,
  getAgentMemoryDecisionReasons,
  getAgentMemoryLaneStatuses,
  getRuntimeConfig,
  syncMemorySources,
  updateRuntimeConfig,
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
  AgentMemoryLaneStatusResponse,
  AgentMemoryRuntimeHealthPayload,
  AgentMemoryStatusResponse,
  AgentMemoryTelemetrySummaryPayload,
  AgentMemoryTelemetryTurnsPayload,
  AgentMemoryTurnWhyPayload,
  RuntimeConnectionSettings,
  RuntimeMemoryConfigResponse,
  RuntimeRoutingConfigResponse,
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

function createSuccessResult() {
  return { ok: true as const };
}

function createFailureResult(error: unknown) {
  return { ok: false as const, error };
}

type MemoryLoadResult = ReturnType<typeof createSuccessResult> | ReturnType<typeof createFailureResult>;

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
  const [routingConfig, setRoutingConfig] = useState<RuntimeRoutingConfigResponse | null>(null);
  const [runtimeMemoryConfig, setRuntimeMemoryConfig] =
    useState<RuntimeMemoryConfigResponse | null>(null);
  const [runtimeMemorySourceCount, setRuntimeMemorySourceCount] = useState(0);
  const [routingLoading, setRoutingLoading] = useState(enabled);
  const [routingError, setRoutingError] = useState<string | null>(null);
  const [laneStatuses, setLaneStatuses] = useState<AgentMemoryLaneStatusResponse[]>([]);
  const [laneStatusLoading, setLaneStatusLoading] = useState(enabled);
  const [laneStatusError, setLaneStatusError] = useState<string | null>(null);
  const [lanePolicySaveKey, setLanePolicySaveKey] = useState<string | null>(null);
  const [runtimeMemorySavePending, setRuntimeMemorySavePending] = useState(false);
  const [memorySyncPendingKey, setMemorySyncPendingKey] = useState<string | null>(null);
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
  const laneStatusRequestIdRef = useRef(0);
  const routingRequestIdRef = useRef(0);

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

  const clearScopedResponses = useCallback(() => {
    setCardDetailState(createEmptyScopedResponse());
    setAtomDetailState(createEmptyScopedResponse());
    setGraphNeighborsState(createEmptyScopedResponse());
    setTurnWhyState(createEmptyScopedResponse());
    setCitationState(createEmptyScopedResponse());
    setCardDetailErrorState(EMPTY_SCOPED_ERROR);
    setAtomDetailErrorState(EMPTY_SCOPED_ERROR);
    setGraphErrorState(EMPTY_SCOPED_ERROR);
    setWhyErrorState(EMPTY_SCOPED_ERROR);
    setCitationErrorState(EMPTY_SCOPED_ERROR);
  }, []);

  const invalidateInFlightRequests = useCallback(() => {
    listRequestIdRef.current += 1;
    cardRequestIdRef.current += 1;
    atomRequestIdRef.current += 1;
    graphRequestIdRef.current += 1;
    whyRequestIdRef.current += 1;
    citationRequestIdRef.current += 1;
    laneStatusRequestIdRef.current += 1;
    routingRequestIdRef.current += 1;
  }, []);

  const resetVisibleMemoryState = useCallback(
    (nextAvailability: MemoryAvailability, nextMessage: string | null) => {
      setAvailability(nextAvailability);
      setAvailabilityMessage(nextMessage);
      setStatus(null);
      clearCoreBundle();
      clearScopedResponses();
    },
    [clearCoreBundle, clearScopedResponses]
  );

  const setSelectedAgentId = useCallback(
    (nextAgentId: string) => {
      invalidateInFlightRequests();
      resetVisibleMemoryState(
        enabled ? "loading" : "disabled",
        enabled
          ? nextAgentId.trim()
            ? null
            : "Waiting for an assistant selection."
          : "Memory hub is disabled in Config."
      );
      resetLaneSelections();
      setSelectedAgentIdState(nextAgentId);
    },
    [enabled, invalidateInFlightRequests, resetLaneSelections, resetVisibleMemoryState]
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
    ): Promise<MemoryLoadResult> => {
      if (!enabled) {
        invalidateInFlightRequests();
        resetVisibleMemoryState("disabled", "Memory hub is disabled in Config.");
        return createSuccessResult();
      }

      if (!targetAgentId.trim()) {
        invalidateInFlightRequests();
        resetVisibleMemoryState("loading", "Waiting for an assistant selection.");
        return createSuccessResult();
      }

      const requestId = ++listRequestIdRef.current;
      setAvailability((current) => (current === "ready" ? current : "loading"));
      setAvailabilityMessage(null);

      try {
        const nextStatus = await getAgentMemoryStatus(runtimeSettings, targetAgentId);
        if (listRequestIdRef.current !== requestId) {
          return createSuccessResult();
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
          return createSuccessResult();
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
          return createSuccessResult();
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
        return createSuccessResult();
      } catch (error: unknown) {
        if (listRequestIdRef.current !== requestId) {
          return createSuccessResult();
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
        clearScopedResponses();
        return createFailureResult(error);
      }
    },
    [
      applyCoreBundle,
      cardQuery,
      cardStatusFilter,
      clearCoreBundle,
      clearScopedResponses,
      enabled,
      episodeQuery,
      invalidateInFlightRequests,
      resetVisibleMemoryState,
      selectedAgentId,
      settings,
      updateCache,
    ]
  );

  const loadRoutingSnapshot = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      const requestId = ++routingRequestIdRef.current;
      if (!enabled || !runtimeSettings.gateway_url.trim()) {
        setRoutingConfig(null);
        setRuntimeMemoryConfig(null);
        setRuntimeMemorySourceCount(0);
        setRoutingError(null);
        setRoutingLoading(false);
        return createSuccessResult();
      }

      setRoutingLoading(true);
      try {
        const response = await getRuntimeConfig(runtimeSettings);
        if (routingRequestIdRef.current !== requestId) {
          return createSuccessResult();
        }
        setRoutingConfig(response.config.routing);
        setRuntimeMemoryConfig(response.config.memory);
        setRuntimeMemorySourceCount(response.config.memory.memory_md_sources.length);
        setRoutingError(null);
        return createSuccessResult();
      } catch (error: unknown) {
        if (routingRequestIdRef.current !== requestId) {
          return createFailureResult(error);
        }
        setRoutingConfig(null);
        setRuntimeMemoryConfig(null);
        setRuntimeMemorySourceCount(0);
        setRoutingError(`Lane routing could not load. (${String(error)})`);
        return createFailureResult(error);
      } finally {
        if (routingRequestIdRef.current === requestId) {
          setRoutingLoading(false);
        }
      }
    },
    [enabled, settings]
  );

  const loadLaneStatuses = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      targetAgentId = selectedAgentId
    ) => {
      const requestId = ++laneStatusRequestIdRef.current;
      if (!enabled || !runtimeSettings.gateway_url.trim() || !targetAgentId.trim()) {
        setLaneStatuses([]);
        setLaneStatusError(null);
        setLaneStatusLoading(false);
        return createSuccessResult();
      }

      setLaneStatusLoading(true);
      try {
        const items = await getAgentMemoryLaneStatuses(runtimeSettings, targetAgentId);
        if (laneStatusRequestIdRef.current !== requestId) {
          return createSuccessResult();
        }
        setLaneStatuses(items);
        setLaneStatusError(null);
        return createSuccessResult();
      } catch (error: unknown) {
        if (laneStatusRequestIdRef.current !== requestId) {
          return createSuccessResult();
        }
        setLaneStatuses([]);
        setLaneStatusError(`Lane memory status could not load. (${String(error)})`);
        return createFailureResult(error);
      } finally {
        if (laneStatusRequestIdRef.current === requestId) {
          setLaneStatusLoading(false);
        }
      }
    },
    [enabled, selectedAgentId, settings]
  );

  const saveLaneMemoryPolicy = useCallback(
    async (
      humanIdentityId: string,
      assistantAgentId: string,
      memoryMode: string,
      options?: { localMemorySources?: string[] }
    ) => {
      const pairKey = `${humanIdentityId}:${assistantAgentId}`;
      if (!settings.gateway_url.trim()) {
        setNotice({
          tone: "error",
          message: "Connect to the gateway before saving lane memory policy.",
        });
        return false;
      }

      setLanePolicySaveKey(pairKey);
      try {
        const response = await getRuntimeConfig(settings);
        const nextRouting: RuntimeRoutingConfigResponse = {
          ...response.config.routing,
          human_identities: [...response.config.routing.human_identities],
          platform_identity_links: [...response.config.routing.platform_identity_links],
          assistant_assignments: [...response.config.routing.assistant_assignments],
          lane_memory_policies: [...response.config.routing.lane_memory_policies],
        };
        const existingIndex = nextRouting.lane_memory_policies.findIndex(
          (item) =>
            item.human_identity_id === humanIdentityId &&
            item.assistant_agent_id === assistantAgentId
        );
        if (existingIndex >= 0) {
          nextRouting.lane_memory_policies[existingIndex] = {
            ...nextRouting.lane_memory_policies[existingIndex],
            memory_mode: memoryMode,
            local_memory_sources:
              options?.localMemorySources ??
              nextRouting.lane_memory_policies[existingIndex].local_memory_sources,
          };
        } else {
          nextRouting.lane_memory_policies.push({
            human_identity_id: humanIdentityId,
            assistant_agent_id: assistantAgentId,
            memory_mode: memoryMode,
            lane_id: null,
            local_memory_sources: options?.localMemorySources ?? [],
          });
        }

        const updateResponse = await updateRuntimeConfig(settings, {
          routing: nextRouting,
        });
        setRoutingConfig(updateResponse.config.routing);
        setRuntimeMemorySourceCount(updateResponse.config.memory.memory_md_sources.length);
        setRoutingError(null);
        await loadLaneStatuses(settings, assistantAgentId);
        setNotice({
          tone: "info",
          message:
            options?.localMemorySources !== undefined
              ? "Lane memory settings saved."
              : "Lane memory mode saved.",
        });
        return true;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Saving lane memory mode failed: ${String(error)}`,
        });
        return false;
      } finally {
        setLanePolicySaveKey(null);
      }
    },
    [loadLaneStatuses, setNotice, settings]
  );

  const saveRuntimeMemoryDefaults = useCallback(
    async (blendMode: string, memoryMdSources: string[]) => {
      if (!settings.gateway_url.trim()) {
        setNotice({
          tone: "error",
          message: "Connect to the gateway before saving runtime memory defaults.",
        });
        return false;
      }

      setRuntimeMemorySavePending(true);
      try {
        const response = await getRuntimeConfig(settings);
        const updateResponse = await updateRuntimeConfig(settings, {
          memory: {
            ...response.config.memory,
            blend_mode: blendMode,
            memory_md_sources: memoryMdSources,
          },
        });
        setRoutingConfig(updateResponse.config.routing);
        setRuntimeMemoryConfig(updateResponse.config.memory);
        setRuntimeMemorySourceCount(updateResponse.config.memory.memory_md_sources.length);
        await loadLaneStatuses(settings, selectedAgentId);
        setRoutingError(null);
        setNotice({
          tone: "info",
          message: "Runtime memory defaults saved.",
        });
        return true;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Saving runtime memory defaults failed: ${String(error)}`,
        });
        return false;
      } finally {
        setRuntimeMemorySavePending(false);
      }
    },
    [loadLaneStatuses, selectedAgentId, setNotice, settings]
  );

  const syncRuntimeMemoryDefaults = useCallback(async () => {
    if (!settings.gateway_url.trim()) {
      setNotice({
        tone: "error",
        message: "Connect to the gateway before syncing runtime memory files.",
      });
      return false;
    }

    setMemorySyncPendingKey("runtime");
    try {
      const response = await syncMemorySources(settings, {});
      await Promise.all([
        loadMemoryData(settings, selectedAgentId),
        loadRoutingSnapshot(settings),
        loadLaneStatuses(settings, selectedAgentId),
      ]);
      setNotice({
        tone: response.failed > 0 ? "error" : "info",
        message:
          response.failed > 0
            ? `Runtime memory sync finished with issues: ${response.synced} synced, ${response.failed} failed.`
            : `Runtime memory sync complete: ${response.synced} file${
                response.synced === 1 ? "" : "s"
              } synced.`,
      });
      return response.failed === 0;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Runtime memory sync failed: ${String(error)}`,
      });
      return false;
    } finally {
      setMemorySyncPendingKey(null);
    }
  }, [
    loadLaneStatuses,
    loadMemoryData,
    loadRoutingSnapshot,
    selectedAgentId,
    setNotice,
    settings,
  ]);

  const syncLaneMemorySources = useCallback(
    async (humanIdentityId: string, assistantAgentId: string) => {
      if (!settings.gateway_url.trim()) {
        setNotice({
          tone: "error",
          message: "Connect to the gateway before syncing lane memory files.",
        });
        return false;
      }

      const syncKey = `lane:${humanIdentityId}:${assistantAgentId}`;
      setMemorySyncPendingKey(syncKey);
      try {
        const response = await syncMemorySources(settings, {
          human_identity_id: humanIdentityId,
          assistant_agent_id: assistantAgentId,
        });
        await Promise.all([
          loadMemoryData(settings, assistantAgentId),
          loadRoutingSnapshot(settings),
          loadLaneStatuses(settings, assistantAgentId),
        ]);
        setNotice({
          tone: response.failed > 0 ? "error" : "info",
          message:
            response.failed > 0
              ? `Lane memory sync finished with issues: ${response.synced} synced, ${response.failed} failed.`
              : `Lane memory sync complete: ${response.synced} file${
                  response.synced === 1 ? "" : "s"
                } synced.`,
        });
        return response.failed === 0;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Lane memory sync failed: ${String(error)}`,
        });
        return false;
      } finally {
        setMemorySyncPendingKey(null);
      }
    },
    [loadLaneStatuses, loadMemoryData, loadRoutingSnapshot, setNotice, settings]
  );

  const refresh = useCallback(async () => {
    const [memoryResult, routingResult, laneStatusResult] = await Promise.all([
      loadMemoryData(settings, selectedAgentId),
      loadRoutingSnapshot(settings),
      loadLaneStatuses(settings, selectedAgentId),
    ]);
    if (!memoryResult.ok) {
      setNotice({
        tone: "error",
        message: `Memory refresh failed: ${normalizeMemoryErrorMessage(memoryResult.error)}`,
      });
      return;
    }
    if (!routingResult.ok) {
      setNotice({
        tone: "info",
        message: "Memory loaded, but the lane routing snapshot could not be refreshed.",
      });
    }
    if (!laneStatusResult.ok) {
      setNotice({
        tone: "info",
        message: "Memory loaded, but the per-lane runtime status could not be refreshed.",
      });
    }
  }, [loadLaneStatuses, loadMemoryData, loadRoutingSnapshot, selectedAgentId, setNotice, settings]);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled) {
        void loadMemoryData(settings, selectedAgentId);
        void loadRoutingSnapshot(settings);
        void loadLaneStatuses(settings, selectedAgentId);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [enabled, loadLaneStatuses, loadMemoryData, loadRoutingSnapshot, selectedAgentId, settings]);

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
    routingConfig,
    runtimeMemoryConfig,
    runtimeMemorySourceCount,
    routingLoading,
    routingError,
    laneStatuses,
    laneStatusLoading,
    laneStatusError,
    lanePolicySaveKey,
    runtimeMemorySavePending,
    memorySyncPendingKey,
    saveLaneMemoryPolicy,
    saveRuntimeMemoryDefaults,
    syncRuntimeMemoryDefaults,
    syncLaneMemorySources,
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
