import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { NotifyFn } from "../../app/useAppController";
import {
  describeConnectorTool,
  getConnector,
  getConnectorHealth,
  importConnector,
  listConnectorCatalog,
  listConnectorInteractions,
  listConnectors,
  publishConnectorTools,
  resumeConnectorInteraction,
  rollbackConnectorVersion,
  runConnectorConversion,
  setConnectorAssignment,
  setConnectorState,
  unpublishConnectorTools,
  upsertConnectorAuthBinding,
} from "../../lib/api";
import type {
  Agent,
  ConnectorCatalogItemResponse,
  ConnectorConversionResponse,
  ConnectorHealthResponse,
  ConnectorInteractionResponse,
  ConnectorSourceResponse,
  DescribeConnectorToolResponse,
  GetConnectorResponse,
  ImportConnectorRequest,
  RuntimeConnectionSettings,
} from "../../types";
import {
  CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS,
  CONNECTOR_AUTH_KIND_OPTIONS,
  CONNECTOR_AUTH_STATUS_OPTIONS,
  CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS,
  CONNECTOR_ORIGIN_KIND_OPTIONS,
  CONNECTOR_SOURCE_KIND_OPTIONS,
} from "./connectorsConfig";
import {
  isConnectorUnsupportedError,
  normalizeConnectorErrorMessage,
  parseJsonDraft,
  resolveDefaultCandidateIds,
  resolveSelectedConnectorId,
  resolveSelectedPublishedToolId,
  resolveSelectedVersionId,
  sortConnectorCatalog,
  sortConnectorDetail,
  sortConnectorInteractions,
  sortConnectorSources,
} from "./connectorsModel";

export type ConnectorAvailability =
  | "disabled"
  | "loading"
  | "ready"
  | "unsupported"
  | "error";

export interface ConnectorImportDraft {
  source_kind: string;
  display_name: string;
  slug: string;
  catalog_item_id: string;
  version_label: string;
  origin_kind: string;
  import_url: string;
  source_text: string;
  source_json_text: string;
  endpoint_url: string;
  external_reference_policy: string;
}

export interface ConnectorAssignmentDraft {
  agent_id: string;
  enabled: boolean;
  auth_mode: string;
}

export interface ConnectorAuthBindingDraft {
  agent_id: string;
  auth_kind: string;
  secret_ref: string;
  oauth_session_id: string;
  status: string;
  auth_metadata_text: string;
}

export interface ConnectorPublishDraft {
  conversion_id: string;
  selected_candidate_ids: string[];
  alias_overrides: Record<string, string>;
  enable_after_publish: boolean;
}

export interface ConnectorFilters {
  query: string;
  source_kind: string;
  status: string;
  trust_state: string;
  include_disabled: boolean;
  catalog_query: string;
  catalog_source_kind: string;
}

interface UseConnectorsControllerOptions {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  enabled: boolean;
  setNotice: NotifyFn;
}

const EMPTY_IMPORT_DRAFT: ConnectorImportDraft = {
  source_kind: CONNECTOR_SOURCE_KIND_OPTIONS[0],
  display_name: "",
  slug: "",
  catalog_item_id: "",
  version_label: "v1",
  origin_kind: CONNECTOR_ORIGIN_KIND_OPTIONS[1],
  import_url: "",
  source_text: "",
  source_json_text: "",
  endpoint_url: "",
  external_reference_policy: CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS[0],
};

const EMPTY_ASSIGNMENT_DRAFT: ConnectorAssignmentDraft = {
  agent_id: "",
  enabled: true,
  auth_mode: CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS[0],
};

const EMPTY_AUTH_BINDING_DRAFT: ConnectorAuthBindingDraft = {
  agent_id: "",
  auth_kind: CONNECTOR_AUTH_KIND_OPTIONS[0],
  secret_ref: "",
  oauth_session_id: "",
  status: CONNECTOR_AUTH_STATUS_OPTIONS[0],
  auth_metadata_text: "{}",
};

const EMPTY_PUBLISH_DRAFT: ConnectorPublishDraft = {
  conversion_id: "",
  selected_candidate_ids: [],
  alias_overrides: {},
  enable_after_publish: false,
};

const EMPTY_FILTERS: ConnectorFilters = {
  query: "",
  source_kind: "all",
  status: "all",
  trust_state: "all",
  include_disabled: true,
  catalog_query: "",
  catalog_source_kind: "all",
};

function trimToUndefined(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function normalizeAgentId(value: string): string | undefined {
  const trimmed = value.trim().toLowerCase();
  return trimmed ? trimmed : undefined;
}

function createImportDraft(
  partial: Partial<ConnectorImportDraft> = {}
): ConnectorImportDraft {
  return {
    ...EMPTY_IMPORT_DRAFT,
    ...partial,
  };
}

export function useConnectorsController(options: UseConnectorsControllerOptions) {
  const { settings, agents, enabled, setNotice } = options;
  const [availability, setAvailability] = useState<ConnectorAvailability>(
    enabled ? "loading" : "disabled"
  );
  const [availabilityMessage, setAvailabilityMessage] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<ConnectorCatalogItemResponse[]>([]);
  const [installedConnectors, setInstalledConnectors] = useState<ConnectorSourceResponse[]>([]);
  const [interactions, setInteractions] = useState<ConnectorInteractionResponse[]>([]);
  const [selectedConnectorId, setSelectedConnectorId] = useState("");
  const [selectedConnectorDetail, setSelectedConnectorDetail] =
    useState<GetConnectorResponse | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [selectedPublishedToolId, setSelectedPublishedToolId] = useState("");
  const [selectedPublishedToolIds, setSelectedPublishedToolIds] = useState<string[]>([]);
  const [selectedToolDetail, setSelectedToolDetail] =
    useState<DescribeConnectorToolResponse | null>(null);
  const [toolDetailLoading, setToolDetailLoading] = useState(false);
  const [toolDetailError, setToolDetailError] = useState<string | null>(null);
  const [health, setHealth] = useState<ConnectorHealthResponse | null>(null);
  const [healthLoading, setHealthLoading] = useState(false);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [importDraft, setImportDraft] = useState<ConnectorImportDraft>(EMPTY_IMPORT_DRAFT);
  const [assignmentDraft, setAssignmentDraft] =
    useState<ConnectorAssignmentDraft>(EMPTY_ASSIGNMENT_DRAFT);
  const [authBindingDraft, setAuthBindingDraft] =
    useState<ConnectorAuthBindingDraft>(EMPTY_AUTH_BINDING_DRAFT);
  const [publishDraft, setPublishDraft] = useState<ConnectorPublishDraft>(EMPTY_PUBLISH_DRAFT);
  const [filters, setFilters] = useState<ConnectorFilters>(EMPTY_FILTERS);
  const [interactionPayloadText, setInteractionPayloadText] = useState("");
  const [mutatingAction, setMutatingAction] = useState<string | null>(null);
  const [conversionByConnectorId, setConversionByConnectorId] = useState<
    Record<string, ConnectorConversionResponse>
  >({});
  const listRequestIdRef = useRef(0);
  const detailRequestIdRef = useRef(0);
  const healthRequestIdRef = useRef(0);
  const toolDetailRequestIdRef = useRef(0);
  const deferredInstalledQuery = useDeferredValue(filters.query.trim().toLowerCase());
  const deferredCatalogQuery = useDeferredValue(
    filters.catalog_query.trim().toLowerCase()
  );

  const selectedConnector =
    installedConnectors.find((item) => item.connector_id === selectedConnectorId) ??
    selectedConnectorDetail?.connector ??
    null;
  const selectedConversion = selectedConnectorId
    ? conversionByConnectorId[selectedConnectorId] ?? null
    : null;
  const selectedVersion =
    selectedConnectorDetail?.versions.find((item) => item.version_id === selectedVersionId) ?? null;
  const selectedPublishedTool =
    selectedConnectorDetail?.published_tools.find(
      (item) => item.published_tool_id === selectedPublishedToolId
    ) ?? null;
  const selectedConnectorInteractions = useMemo(() => {
    if (!selectedConnectorId) {
      return [];
    }
    const scoped = interactions.filter((item) => item.connector_id === selectedConnectorId);
    if (scoped.length > 0) {
      return scoped;
    }
    return selectedConnectorDetail?.interactions ?? [];
  }, [interactions, selectedConnectorDetail, selectedConnectorId]);
  const pausedInteractions = useMemo(
    () =>
      interactions.filter(
        (item) => item.status === "pending" || item.status === "waiting_on_operator"
      ),
    [interactions]
  );
  const filteredCatalogItems = useMemo(
    () =>
      catalog.filter((item) => {
        if (
          filters.catalog_source_kind !== "all" &&
          item.source_kind !== filters.catalog_source_kind
        ) {
          return false;
        }
        if (!deferredCatalogQuery) {
          return true;
        }
        const haystack = [
          item.display_name,
          item.summary,
          item.publisher,
          item.source_kind,
          item.slug,
        ]
          .join(" ")
          .toLowerCase();
        return haystack.includes(deferredCatalogQuery);
      }),
    [catalog, deferredCatalogQuery, filters.catalog_source_kind]
  );
  const filteredInstalledConnectors = useMemo(
    () =>
      installedConnectors.filter((item) => {
        if (filters.source_kind !== "all" && item.source_kind !== filters.source_kind) {
          return false;
        }
        if (filters.status !== "all" && item.status !== filters.status) {
          return false;
        }
        if (filters.trust_state !== "all" && item.trust_state !== filters.trust_state) {
          return false;
        }
        if (!filters.include_disabled && item.status === "disabled") {
          return false;
        }
        if (!deferredInstalledQuery) {
          return true;
        }
        const haystack = [
          item.display_name,
          item.slug,
          item.source_kind,
          item.status,
          item.trust_state,
        ]
          .join(" ")
          .toLowerCase();
        return haystack.includes(deferredInstalledQuery);
      }),
    [
      deferredInstalledQuery,
      filters.include_disabled,
      filters.source_kind,
      filters.status,
      filters.trust_state,
      installedConnectors,
    ]
  );
  const summary = useMemo(
    () => ({
      installed: installedConnectors.length,
      liveTools: installedConnectors.reduce(
        (total, item) => total + item.published_tool_count,
        0
      ),
      agentAssignments: installedConnectors.reduce(
        (total, item) => total + item.assigned_agent_count,
        0
      ),
      pendingInteractions: pausedInteractions.length,
    }),
    [installedConnectors, pausedInteractions.length]
  );

  const invalidateConnectorScopedRequests = useCallback(() => {
    detailRequestIdRef.current += 1;
    healthRequestIdRef.current += 1;
    toolDetailRequestIdRef.current += 1;
  }, []);

  const resetConnectorScopedState = useCallback(() => {
    setSelectedVersionId("");
    setSelectedPublishedToolId("");
    setSelectedPublishedToolIds([]);
    setSelectedConnectorDetail(null);
    setDetailError(null);
    setDetailLoading(false);
    setHealth(null);
    setHealthError(null);
    setHealthLoading(false);
    setSelectedToolDetail(null);
    setToolDetailError(null);
    setToolDetailLoading(false);
  }, []);

  const loadIndexData = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      preferredConnectorId?: string
    ) => {
      if (!enabled) {
        listRequestIdRef.current += 1;
        setAvailability("disabled");
        setAvailabilityMessage("Connectors are disabled in Config > Reliability + Rollout.");
        setCatalog([]);
        setInstalledConnectors([]);
        setInteractions([]);
        setSelectedConnectorId("");
        invalidateConnectorScopedRequests();
        resetConnectorScopedState();
        return;
      }

      const requestId = ++listRequestIdRef.current;
      setAvailability((current) => (current === "ready" ? current : "loading"));
      setAvailabilityMessage(null);

      try {
        const [catalogResponse, connectorsResponse, interactionsResponse] = await Promise.all([
          listConnectorCatalog(runtimeSettings),
          listConnectors(runtimeSettings, { include_disabled: true }),
          listConnectorInteractions(runtimeSettings),
        ]);
        if (listRequestIdRef.current !== requestId) {
          return;
        }
        const nextCatalog = sortConnectorCatalog(catalogResponse.items);
        const nextConnectors = sortConnectorSources(connectorsResponse.items);
        const nextInteractions = sortConnectorInteractions(interactionsResponse.items);
        setCatalog(nextCatalog);
        setInstalledConnectors(nextConnectors);
        setInteractions(nextInteractions);
        setAvailability("ready");
        setSelectedConnectorId((current) =>
          resolveSelectedConnectorId(nextConnectors, preferredConnectorId ?? current)
        );

        if (nextConnectors.length === 0) {
          setSelectedConnectorDetail(null);
          setDetailError(null);
          setHealth(null);
          setHealthError(null);
          setSelectedVersionId("");
          setSelectedPublishedToolId("");
          setSelectedPublishedToolIds([]);
          setSelectedToolDetail(null);
          setToolDetailError(null);
        }
      } catch (error: unknown) {
        if (listRequestIdRef.current !== requestId) {
          return;
        }
        if (isConnectorUnsupportedError(error)) {
          setAvailability("unsupported");
          setAvailabilityMessage(
            "The connected gateway does not expose the Connectors surface yet."
          );
          setCatalog([]);
          setInstalledConnectors([]);
          setInteractions([]);
          return;
        }
        setAvailability("error");
        setAvailabilityMessage(normalizeConnectorErrorMessage(error));
      }
    },
    [enabled, invalidateConnectorScopedRequests, resetConnectorScopedState, settings]
  );

  const loadConnectorDetail = useCallback(
    async (
      connectorId: string,
      runtimeSettings: RuntimeConnectionSettings = settings
    ) => {
      if (!enabled || !connectorId.trim()) {
        detailRequestIdRef.current += 1;
        setDetailLoading(false);
        setSelectedConnectorDetail(null);
        setDetailError(null);
        return;
      }

      const requestId = ++detailRequestIdRef.current;
      setDetailLoading(true);
      setDetailError(null);
      try {
        const detail = sortConnectorDetail(await getConnector(runtimeSettings, connectorId));
        if (detailRequestIdRef.current !== requestId) {
          return;
        }
        setSelectedConnectorDetail(detail);
      } catch (error: unknown) {
        if (detailRequestIdRef.current !== requestId) {
          return;
        }
        setSelectedConnectorDetail(null);
        setDetailError(normalizeConnectorErrorMessage(error));
      } finally {
        if (detailRequestIdRef.current === requestId) {
          setDetailLoading(false);
        }
      }
    },
    [enabled, settings]
  );

  const loadConnectorHealth = useCallback(
    async (
      connectorId: string,
      runtimeSettings: RuntimeConnectionSettings = settings
    ) => {
      if (!enabled || !connectorId.trim()) {
        healthRequestIdRef.current += 1;
        setHealthLoading(false);
        setHealth(null);
        setHealthError(null);
        return;
      }

      const requestId = ++healthRequestIdRef.current;
      setHealthLoading(true);
      setHealthError(null);
      try {
        const nextHealth = await getConnectorHealth(runtimeSettings, connectorId);
        if (healthRequestIdRef.current !== requestId) {
          return;
        }
        setHealth(nextHealth);
      } catch (error: unknown) {
        if (healthRequestIdRef.current !== requestId) {
          return;
        }
        setHealth(null);
        setHealthError(normalizeConnectorErrorMessage(error));
      } finally {
        if (healthRequestIdRef.current === requestId) {
          setHealthLoading(false);
        }
      }
    },
    [enabled, settings]
  );

  const loadConnectorToolDetail = useCallback(
    async (
      connectorId: string,
      publishedToolId: string,
      runtimeSettings: RuntimeConnectionSettings = settings
    ) => {
      if (!enabled || !connectorId.trim() || !publishedToolId.trim()) {
        toolDetailRequestIdRef.current += 1;
        setToolDetailLoading(false);
        setSelectedToolDetail(null);
        setToolDetailError(null);
        return;
      }

      const requestId = ++toolDetailRequestIdRef.current;
      setToolDetailLoading(true);
      setToolDetailError(null);
      try {
        const detail = await describeConnectorTool(runtimeSettings, connectorId, publishedToolId);
        if (toolDetailRequestIdRef.current !== requestId) {
          return;
        }
        setSelectedToolDetail(detail);
      } catch (error: unknown) {
        if (toolDetailRequestIdRef.current !== requestId) {
          return;
        }
        setSelectedToolDetail(null);
        setToolDetailError(normalizeConnectorErrorMessage(error));
      } finally {
        if (toolDetailRequestIdRef.current === requestId) {
          setToolDetailLoading(false);
        }
      }
    },
    [enabled, settings]
  );

  const refresh = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      preferredConnectorId = selectedConnectorId
    ) => {
      await loadIndexData(runtimeSettings, preferredConnectorId);
      if (preferredConnectorId) {
        await Promise.all([
          loadConnectorDetail(preferredConnectorId, runtimeSettings),
          loadConnectorHealth(preferredConnectorId, runtimeSettings),
        ]);
      }
    },
    [loadConnectorDetail, loadConnectorHealth, loadIndexData, selectedConnectorId, settings]
  );

  useEffect(() => {
    void loadIndexData(settings);
  }, [loadIndexData, settings]);

  useEffect(() => {
    if (!selectedConnectorId) {
      invalidateConnectorScopedRequests();
      resetConnectorScopedState();
      return;
    }

    invalidateConnectorScopedRequests();
    resetConnectorScopedState();
    void loadConnectorDetail(selectedConnectorId, settings);
    void loadConnectorHealth(selectedConnectorId, settings);
  }, [
    invalidateConnectorScopedRequests,
    loadConnectorDetail,
    loadConnectorHealth,
    resetConnectorScopedState,
    selectedConnectorId,
    settings,
  ]);

  useEffect(() => {
    const nextVersionId = resolveSelectedVersionId(selectedConnectorDetail, selectedVersionId);
    if (nextVersionId !== selectedVersionId) {
      setSelectedVersionId(nextVersionId);
    }

    const nextPublishedToolId = resolveSelectedPublishedToolId(
      selectedConnectorDetail,
      selectedPublishedToolId
    );
    if (nextPublishedToolId !== selectedPublishedToolId) {
      setSelectedPublishedToolId(nextPublishedToolId);
    }
  }, [selectedConnectorDetail, selectedPublishedToolId, selectedVersionId]);

  useEffect(() => {
    if (!selectedConnectorId || !selectedPublishedToolId) {
      setSelectedToolDetail(null);
      setToolDetailError(null);
      return;
    }
    void loadConnectorToolDetail(selectedConnectorId, selectedPublishedToolId, settings);
  }, [loadConnectorToolDetail, selectedConnectorId, selectedPublishedToolId, settings]);

  useEffect(() => {
    setSelectedPublishedToolIds((current) => {
      const allowed = new Set(
        (selectedConnectorDetail?.published_tools ?? []).map((item) => item.published_tool_id)
      );
      return current.filter((item) => allowed.has(item));
    });
  }, [selectedConnectorDetail]);

  useEffect(() => {
    setAssignmentDraft((current) => {
      if (current.agent_id && agents.some((item) => item.agent_id === current.agent_id)) {
        return current;
      }
      return {
        ...current,
        agent_id: agents[0]?.agent_id ?? "",
      };
    });
    setAuthBindingDraft((current) => {
      if (!current.agent_id || agents.some((item) => item.agent_id === current.agent_id)) {
        return current;
      }
      return {
        ...current,
        agent_id: "",
      };
    });
  }, [agents]);

  useEffect(() => {
    if (!selectedConversion) {
      setPublishDraft((current) =>
        current.conversion_id ? EMPTY_PUBLISH_DRAFT : current
      );
      return;
    }

    setPublishDraft((current) => {
      if (current.conversion_id !== selectedConversion.conversion_id) {
        return {
          conversion_id: selectedConversion.conversion_id,
          selected_candidate_ids: resolveDefaultCandidateIds(selectedConversion),
          alias_overrides: {},
          enable_after_publish: false,
        };
      }
      const validIds = new Set(
        selectedConversion.proposed_tools.map((item) => item.candidate_id)
      );
      return {
        ...current,
        selected_candidate_ids: current.selected_candidate_ids.filter((item) =>
          validIds.has(item)
        ),
        alias_overrides: Object.fromEntries(
          Object.entries(current.alias_overrides).filter(([candidateId]) =>
            validIds.has(candidateId)
          )
        ),
      };
    });
  }, [selectedConversion]);

  const applyCatalogTemplate = useCallback(
    (catalogItemId: string) => {
      const item = catalog.find((entry) => entry.catalog_item_id === catalogItemId);
      if (!item) {
        return;
      }
      setImportDraft((current) =>
        createImportDraft({
          ...current,
          source_kind: item.source_kind,
          display_name: item.display_name,
          slug: item.slug,
          catalog_item_id: item.catalog_item_id,
          version_label: item.available_versions[0] ?? current.version_label,
          origin_kind: CONNECTOR_ORIGIN_KIND_OPTIONS[0],
        })
      );
    },
    [catalog]
  );

  const updateImportDraft = useCallback((patch: Partial<ConnectorImportDraft>) => {
    setImportDraft((current) => ({
      ...current,
      ...patch,
    }));
  }, []);

  const updateImportDraftCompat = useCallback(
    (
      patch: Partial<
        ConnectorImportDraft & {
          source_payload: string;
        }
      >
    ) => {
      setImportDraft((current) => ({
        ...current,
        ...patch,
        source_json_text:
          patch.source_payload !== undefined
            ? patch.source_payload
            : patch.source_json_text !== undefined
              ? patch.source_json_text
              : current.source_json_text,
      }));
    },
    []
  );

  const resetImportDraft = useCallback(() => {
    setImportDraft(EMPTY_IMPORT_DRAFT);
  }, []);

  const updateAssignmentDraft = useCallback((patch: Partial<ConnectorAssignmentDraft>) => {
    setAssignmentDraft((current) => ({
      ...current,
      ...patch,
    }));
  }, []);

  const updateAuthBindingDraft = useCallback((patch: Partial<ConnectorAuthBindingDraft>) => {
    setAuthBindingDraft((current) => ({
      ...current,
      ...patch,
    }));
  }, []);

  const updateAuthBindingDraftCompat = useCallback(
    (
      patch: Partial<
        ConnectorAuthBindingDraft & {
          auth_metadata: string;
        }
      >
    ) => {
      setAuthBindingDraft((current) => ({
        ...current,
        ...patch,
        auth_metadata_text:
          patch.auth_metadata !== undefined
            ? patch.auth_metadata
            : patch.auth_metadata_text !== undefined
              ? patch.auth_metadata_text
              : current.auth_metadata_text,
      }));
    },
    []
  );

  const updateFilters = useCallback((patch: Partial<ConnectorFilters>) => {
    setFilters((current) => ({
      ...current,
      ...patch,
    }));
  }, []);

  const togglePublishCandidate = useCallback((candidateId: string) => {
    setPublishDraft((current) => {
      const selected = new Set(current.selected_candidate_ids);
      if (selected.has(candidateId)) {
        selected.delete(candidateId);
      } else {
        selected.add(candidateId);
      }
      return {
        ...current,
        selected_candidate_ids: Array.from(selected),
      };
    });
  }, []);

  const setPublishAlias = useCallback((candidateId: string, alias: string) => {
    setPublishDraft((current) => ({
      ...current,
      alias_overrides: {
        ...current.alias_overrides,
        [candidateId]: alias,
      },
    }));
  }, []);

  const setEnableAfterPublish = useCallback((enableAfterPublish: boolean) => {
    setPublishDraft((current) => ({
      ...current,
      enable_after_publish: enableAfterPublish,
    }));
  }, []);

  const togglePublishedToolSelection = useCallback((publishedToolId: string) => {
    setSelectedPublishedToolIds((current) => {
      const selected = new Set(current);
      if (selected.has(publishedToolId)) {
        selected.delete(publishedToolId);
      } else {
        selected.add(publishedToolId);
      }
      return Array.from(selected);
    });
  }, []);

  const selectConnector = useCallback((connectorId: string) => {
    setSelectedConnectorId(connectorId);
  }, []);

  const selectPublishedTool = useCallback((publishedToolId: string) => {
    setSelectedPublishedToolId(publishedToolId);
  }, []);

  const importFromDraft = useCallback(async (): Promise<boolean> => {
    if (!enabled) {
      return false;
    }

    let payload: ImportConnectorRequest;
    try {
      payload = {
        source_kind: importDraft.source_kind,
        display_name: importDraft.display_name.trim(),
        slug: trimToUndefined(importDraft.slug),
        catalog_item_id: trimToUndefined(importDraft.catalog_item_id),
        version_label: trimToUndefined(importDraft.version_label),
        origin_kind: trimToUndefined(importDraft.origin_kind),
        import_url: trimToUndefined(importDraft.import_url),
        source_text: trimToUndefined(importDraft.source_text),
        source_json: parseJsonDraft(importDraft.source_json_text, "Source JSON"),
        endpoint_url: trimToUndefined(importDraft.endpoint_url),
        external_reference_policy: trimToUndefined(importDraft.external_reference_policy),
      };
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector import failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    }

    try {
      setMutatingAction("import");
      const response = await importConnector(settings, payload);
      setImportDraft(
        createImportDraft({
          source_kind: response.connector.source_kind,
          catalog_item_id: response.connector.catalog_item_id ?? "",
          origin_kind: response.connector.origin_kind,
        })
      );
      setSelectedConnectorId(response.connector.connector_id);
      await refresh(settings, response.connector.connector_id);
      setNotice({
        tone: "info",
        message: `Imported connector: ${response.connector.display_name}`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector import failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [enabled, importDraft, refresh, setNotice, settings]);

  const convertSelectedConnector = useCallback(async (): Promise<boolean> => {
    if (!selectedConnectorId) {
      setNotice({ tone: "error", message: "Select a connector before converting it." });
      return false;
    }

    try {
      setMutatingAction("convert");
      const response = await runConnectorConversion(settings, selectedConnectorId, {
        version_id: trimToUndefined(selectedVersionId),
      });
      setConversionByConnectorId((current) => ({
        ...current,
        [selectedConnectorId]: response.conversion,
      }));
      setSelectedVersionId(response.version.version_id);
      await refresh(settings, selectedConnectorId);
      setNotice({
        tone: "info",
        message: `Conversion ready for ${response.connector.display_name}.`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector conversion failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [refresh, selectedConnectorId, selectedVersionId, setNotice, settings]);

  const publishSelectedTools = useCallback(async (): Promise<boolean> => {
    if (!selectedConnectorId || !selectedConversion) {
      setNotice({
        tone: "error",
        message: "Run conversion before publishing connector tools.",
      });
      return false;
    }
    if (publishDraft.selected_candidate_ids.length === 0) {
      setNotice({
        tone: "error",
        message: "Select at least one proposed tool before publishing.",
      });
      return false;
    }

    try {
      setMutatingAction("publish");
      const response = await publishConnectorTools(settings, selectedConnectorId, {
        conversion_id: selectedConversion.conversion_id,
        selected_candidate_ids: publishDraft.selected_candidate_ids,
        alias_overrides: Object.entries(publishDraft.alias_overrides)
          .filter(([candidateId]) =>
            publishDraft.selected_candidate_ids.includes(candidateId)
          )
          .map(([candidate_id, alias]) => ({
            candidate_id,
            alias: alias.trim(),
          }))
          .filter((item) => item.alias),
        enable_after_publish: publishDraft.enable_after_publish,
      });
      await refresh(settings, selectedConnectorId);
      setSelectedPublishedToolIds([]);
      setNotice({
        tone: "info",
        message: `Published ${response.published_tools.length} connector tool(s).`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector publish failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [publishDraft, refresh, selectedConnectorId, selectedConversion, setNotice, settings]);

  const unpublishSelectedTools = useCallback(async (): Promise<boolean> => {
    if (!selectedConnectorId) {
      setNotice({ tone: "error", message: "Select a connector before unpublishing tools." });
      return false;
    }
    if (selectedPublishedToolIds.length === 0) {
      setNotice({ tone: "error", message: "Select at least one published tool to unpublish." });
      return false;
    }

    try {
      setMutatingAction("unpublish");
      const response = await unpublishConnectorTools(settings, selectedConnectorId, {
        published_tool_ids: selectedPublishedToolIds,
      });
      await refresh(settings, selectedConnectorId);
      setSelectedPublishedToolIds([]);
      setNotice({
        tone: "info",
        message: `Unpublished ${response.published_tools.length} connector tool(s).`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector unpublish failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [refresh, selectedConnectorId, selectedPublishedToolIds, setNotice, settings]);

  const rollbackSelectedVersion = useCallback(async (): Promise<boolean> => {
    if (!selectedConnectorId || !selectedVersionId) {
      setNotice({ tone: "error", message: "Select a version before rolling back." });
      return false;
    }

    try {
      setMutatingAction("rollback");
      const response = await rollbackConnectorVersion(settings, selectedConnectorId, {
        version_id: selectedVersionId,
      });
      setConversionByConnectorId((current) => {
        const next = { ...current };
        delete next[selectedConnectorId];
        return next;
      });
      await refresh(settings, selectedConnectorId);
      setSelectedVersionId(response.version.version_id);
      setNotice({
        tone: "info",
        message: `Rolled back to ${response.version.version_label}.`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector rollback failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [refresh, selectedConnectorId, selectedVersionId, setNotice, settings]);

  const updateConnectorEnabled = useCallback(
    async (nextEnabled: boolean): Promise<boolean> => {
      if (!selectedConnectorId) {
        setNotice({ tone: "error", message: "Select a connector before changing state." });
        return false;
      }

      try {
        setMutatingAction("state");
        const response = await setConnectorState(settings, selectedConnectorId, {
          enabled: nextEnabled,
        });
        await refresh(settings, selectedConnectorId);
        setNotice({
          tone: "info",
          message: `${response.connector.display_name} is now ${
            nextEnabled ? "enabled" : "disabled"
          }.`,
        });
        return true;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Connector state change failed: ${normalizeConnectorErrorMessage(error)}`,
        });
        return false;
      } finally {
        setMutatingAction(null);
      }
    },
    [refresh, selectedConnectorId, setNotice, settings]
  );

  const saveAssignment = useCallback(async (): Promise<boolean> => {
    if (!selectedConnectorId) {
      setNotice({ tone: "error", message: "Select a connector before assigning it." });
      return false;
    }
    if (!assignmentDraft.agent_id.trim()) {
      setNotice({ tone: "error", message: "Select an agent before saving assignment." });
      return false;
    }

    try {
      setMutatingAction("assignment");
      const response = await setConnectorAssignment(settings, selectedConnectorId, {
        agent_id: assignmentDraft.agent_id,
        enabled: assignmentDraft.enabled,
        auth_mode: assignmentDraft.auth_mode,
      });
      await refresh(settings, selectedConnectorId);
      setNotice({
        tone: "info",
        message: `Assignment saved for ${response.assignment.agent_id}.`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector assignment failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [assignmentDraft, refresh, selectedConnectorId, setNotice, settings]);

  const saveAssignmentForAgent = useCallback(
    async (agentId: string, enabledValue: boolean, authMode: string): Promise<boolean> => {
      if (!selectedConnectorId) {
        setNotice({
          tone: "error",
          message: "Select a connector before assigning it.",
        });
        return false;
      }
      try {
        setMutatingAction(`assignment:${agentId}`);
        const response = await setConnectorAssignment(settings, selectedConnectorId, {
          agent_id: agentId,
          enabled: enabledValue,
          auth_mode: authMode,
        });
        await refresh(settings, selectedConnectorId);
        setNotice({
          tone: "info",
          message: `Assignment saved for ${response.assignment.agent_id}.`,
        });
        return true;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Connector assignment failed: ${normalizeConnectorErrorMessage(error)}`,
        });
        return false;
      } finally {
        setMutatingAction(null);
      }
    },
    [refresh, selectedConnectorId, setNotice, settings]
  );

  const saveAuthBinding = useCallback(async (): Promise<boolean> => {
    if (!selectedConnectorId) {
      setNotice({ tone: "error", message: "Select a connector before saving auth." });
      return false;
    }

    let authMetadata: unknown;
    try {
      authMetadata = parseJsonDraft(authBindingDraft.auth_metadata_text, "Auth metadata") ?? {};
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector auth failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    }

    try {
      setMutatingAction("auth");
      const response = await upsertConnectorAuthBinding(settings, selectedConnectorId, {
        agent_id: normalizeAgentId(authBindingDraft.agent_id),
        auth_kind: authBindingDraft.auth_kind,
        secret_ref: trimToUndefined(authBindingDraft.secret_ref),
        oauth_session_id: trimToUndefined(authBindingDraft.oauth_session_id),
        auth_metadata: authMetadata,
        status: authBindingDraft.status,
      });
      await refresh(settings, selectedConnectorId);
      setNotice({
        tone: "info",
        message: `Auth binding saved: ${response.binding.auth_kind}`,
      });
      return true;
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Connector auth failed: ${normalizeConnectorErrorMessage(error)}`,
      });
      return false;
    } finally {
      setMutatingAction(null);
    }
  }, [authBindingDraft, refresh, selectedConnectorId, setNotice, settings]);

  const resumeInteraction = useCallback(
    async (interactionId: string): Promise<boolean> => {
      let payload: unknown;
      try {
        payload = parseJsonDraft(interactionPayloadText, "Interaction payload");
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Interaction resume failed: ${normalizeConnectorErrorMessage(error)}`,
        });
        return false;
      }

      try {
        setMutatingAction(`resume:${interactionId}`);
        const response = await resumeConnectorInteraction(settings, interactionId, {
          payload,
        });
        await refresh(settings, response.interaction.connector_id);
        setInteractionPayloadText("");
        setNotice({
          tone: "info",
          message: `Interaction resumed: ${response.interaction.interaction_kind}`,
        });
        return true;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Interaction resume failed: ${normalizeConnectorErrorMessage(error)}`,
        });
        return false;
      } finally {
        setMutatingAction(null);
      }
    },
    [interactionPayloadText, refresh, setNotice, settings]
  );

  const refreshHealth = useCallback(async (): Promise<void> => {
    if (!selectedConnectorId) {
      return;
    }
    await loadConnectorHealth(selectedConnectorId, settings);
  }, [loadConnectorHealth, selectedConnectorId, settings]);

  const queueRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      void refresh(runtimeSettings, selectedConnectorId);
    },
    [refresh, selectedConnectorId, settings]
  );

  const unpublishTool = useCallback(
    async (publishedToolId: string): Promise<boolean> => {
      if (!selectedConnectorId) {
        setNotice({
          tone: "error",
          message: "Select a connector before unpublishing tools.",
        });
        return false;
      }
      try {
        setMutatingAction(`unpublish:${publishedToolId}`);
        const response = await unpublishConnectorTools(settings, selectedConnectorId, {
          published_tool_ids: [publishedToolId],
        });
        await refresh(settings, selectedConnectorId);
        setNotice({
          tone: "info",
          message: `Unpublished ${response.published_tools.length} connector tool(s).`,
        });
        return true;
      } catch (error: unknown) {
        setNotice({
          tone: "error",
          message: `Connector unpublish failed: ${normalizeConnectorErrorMessage(error)}`,
        });
        return false;
      } finally {
        setMutatingAction(null);
      }
    },
    [refresh, selectedConnectorId, setNotice, settings]
  );

  return {
    settings,
    agents,
    availability,
    availabilityMessage,
    summary,
    filters,
    catalog,
    catalogItems: filteredCatalogItems,
    installedConnectors,
    connectors: filteredInstalledConnectors,
    allConnectors: installedConnectors,
    interactions,
    pausedInteractions,
    selectedConnector,
    selectedConnectorId,
    setSelectedConnectorId: selectConnector,
    selectedConnectorDetail,
    connectorDetail: selectedConnectorDetail,
    selectedConnectorInteractions,
    selectedVersion,
    selectedVersionId,
    selectedPublishedTool,
    selectedPublishedToolId,
    selectedPublishedToolIds,
    selectedToolDetail,
    selectedConversion,
    health,
    connectorHealth: health,
    importDraft,
    importDraftCompat: {
      ...importDraft,
      source_payload: importDraft.source_json_text || importDraft.source_text,
    },
    assignmentDraft,
    authBindingDraft,
    authBindingDraftCompat: {
      ...authBindingDraft,
      auth_metadata: authBindingDraft.auth_metadata_text,
    },
    publishDraft,
    selectedCandidateIds: publishDraft.selected_candidate_ids,
    aliasOverrides: publishDraft.alias_overrides,
    interactionPayloadText,
    mutatingAction,
    mutating: mutatingAction,
    detailLoading,
    detailError,
    detailMessage: detailError ?? healthError,
    toolDetailLoading,
    toolDetailError,
    healthLoading,
    healthError,
    enabled,
    selectConnector,
    setSelectedVersionId,
    selectPublishedTool,
    applyCatalogTemplate,
    hydrateImportDraftFromCatalog: applyCatalogTemplate,
    updateImportDraft,
    updateImportDraftCompat,
    updateFilters,
    resetImportDraft,
    updateAssignmentDraft,
    updateAuthBindingDraft,
    updateAuthBindingDraftCompat,
    togglePublishCandidate,
    toggleCandidateSelection: togglePublishCandidate,
    setPublishAlias,
    setCandidateAlias: setPublishAlias,
    setEnableAfterPublish,
    togglePublishedToolSelection,
    setInteractionPayloadText,
    importFromDraft,
    submitImport: importFromDraft,
    convertSelectedConnector,
    runSelectedConnectorConversion: convertSelectedConnector,
    publishSelectedTools,
    unpublishSelectedTools,
    unpublishTool,
    rollbackSelectedVersion,
    updateConnectorEnabled,
    setSelectedConnectorEnabled: updateConnectorEnabled,
    saveAssignmentDraft: saveAssignment,
    saveAssignment,
    saveAssignmentForAgent,
    saveAuthBinding,
    resumeInteraction,
    refresh,
    queueRefresh,
    refreshHealth,
  };
}
