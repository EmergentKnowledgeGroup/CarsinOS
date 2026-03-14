import { GatewayApiError } from "../../lib/api";
import type {
  ConnectorAssignmentResponse,
  ConnectorAuthBindingResponse,
  ConnectorCatalogItemResponse,
  ConnectorConversionResponse,
  ConnectorInteractionResponse,
  ConnectorPublishedToolResponse,
  ConnectorSourceResponse,
  ConnectorVersionResponse,
  GetConnectorResponse,
} from "../../types";
import { CONNECTOR_UNSUPPORTED_STATUS_FRAGMENTS } from "./connectorsConfig";

export function normalizeConnectorErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function isConnectorUnsupportedError(error: unknown): boolean {
  if (error instanceof GatewayApiError && error.kind === "http") {
    const path = error.path?.toLowerCase() ?? "";
    const isRegistrySurfacePath =
      path === "/api/v1/connectors" ||
      path === "/api/v1/connectors/" ||
      path.startsWith("/api/v1/connectors?") ||
      path.startsWith("/api/v1/connectors/catalog") ||
      path.startsWith("/api/v1/connectors/interactions");
    if (error.status === 404 && isRegistrySurfacePath) {
      return true;
    }
    if (error.status != null) {
      return false;
    }
  }
  const message = normalizeConnectorErrorMessage(error).toLowerCase();
  return CONNECTOR_UNSUPPORTED_STATUS_FRAGMENTS.some((fragment) =>
    message.includes(fragment)
  );
}

export function sortConnectorCatalog(
  items: ConnectorCatalogItemResponse[]
): ConnectorCatalogItemResponse[] {
  return [...items].sort((left, right) => {
    if (left.importable !== right.importable) {
      return Number(right.importable) - Number(left.importable);
    }
    return left.display_name.localeCompare(right.display_name);
  });
}

export function sortConnectorSources(
  items: ConnectorSourceResponse[]
): ConnectorSourceResponse[] {
  return [...items].sort((left, right) => {
    if (left.updated_at !== right.updated_at) {
      return right.updated_at - left.updated_at;
    }
    return left.display_name.localeCompare(right.display_name);
  });
}

export function sortConnectorVersions(
  items: ConnectorVersionResponse[]
): ConnectorVersionResponse[] {
  return [...items].sort((left, right) => {
    if (left.created_at !== right.created_at) {
      return right.created_at - left.created_at;
    }
    return right.updated_at - left.updated_at;
  });
}

export function sortConnectorPublishedTools(
  items: ConnectorPublishedToolResponse[]
): ConnectorPublishedToolResponse[] {
  return [...items].sort((left, right) => {
    const leftActive = left.unpublished_at == null;
    const rightActive = right.unpublished_at == null;
    if (leftActive !== rightActive) {
      return Number(rightActive) - Number(leftActive);
    }
    return right.published_at - left.published_at;
  });
}

export function sortConnectorAssignments(
  items: ConnectorAssignmentResponse[]
): ConnectorAssignmentResponse[] {
  return [...items].sort((left, right) => left.agent_id.localeCompare(right.agent_id));
}

export function sortConnectorAuthBindings(
  items: ConnectorAuthBindingResponse[]
): ConnectorAuthBindingResponse[] {
  return [...items].sort((left, right) => {
    const leftScope = left.agent_id ? 1 : 0;
    const rightScope = right.agent_id ? 1 : 0;
    if (leftScope !== rightScope) {
      return leftScope - rightScope;
    }
    if ((left.agent_id ?? "") !== (right.agent_id ?? "")) {
      return (left.agent_id ?? "").localeCompare(right.agent_id ?? "");
    }
    return right.updated_at - left.updated_at;
  });
}

export function sortConnectorInteractions(
  items: ConnectorInteractionResponse[]
): ConnectorInteractionResponse[] {
  return [...items].sort((left, right) => {
    if (left.updated_at !== right.updated_at) {
      return right.updated_at - left.updated_at;
    }
    return right.created_at - left.created_at;
  });
}

export function sortConnectorDetail(detail: GetConnectorResponse): GetConnectorResponse {
  return {
    ...detail,
    versions: sortConnectorVersions(detail.versions),
    published_tools: sortConnectorPublishedTools(detail.published_tools),
    assignments: sortConnectorAssignments(detail.assignments),
    auth_bindings: sortConnectorAuthBindings(detail.auth_bindings),
    interactions: sortConnectorInteractions(detail.interactions),
  };
}

export function resolveSelectedConnectorId(
  connectors: ConnectorSourceResponse[],
  preferredConnectorId: string
): string {
  if (preferredConnectorId && connectors.some((item) => item.connector_id === preferredConnectorId)) {
    return preferredConnectorId;
  }
  return connectors[0]?.connector_id ?? "";
}

export function resolveSelectedVersionId(
  detail: GetConnectorResponse | null,
  preferredVersionId: string
): string {
  if (!detail) {
    return "";
  }
  if (preferredVersionId && detail.versions.some((item) => item.version_id === preferredVersionId)) {
    return preferredVersionId;
  }
  if (
    detail.connector.current_version_id &&
    detail.versions.some((item) => item.version_id === detail.connector.current_version_id)
  ) {
    return detail.connector.current_version_id;
  }
  if (
    detail.connector.latest_imported_version_id &&
    detail.versions.some((item) => item.version_id === detail.connector.latest_imported_version_id)
  ) {
    return detail.connector.latest_imported_version_id;
  }
  return detail.versions[0]?.version_id ?? "";
}

export function resolveSelectedPublishedToolId(
  detail: GetConnectorResponse | null,
  preferredPublishedToolId: string
): string {
  if (!detail) {
    return "";
  }
  if (
    preferredPublishedToolId &&
    detail.published_tools.some((item) => item.published_tool_id === preferredPublishedToolId)
  ) {
    return preferredPublishedToolId;
  }
  return (
    detail.published_tools.find((item) => item.unpublished_at == null)?.published_tool_id ??
    detail.published_tools[0]?.published_tool_id ??
    ""
  );
}

export function resolveDefaultCandidateIds(
  conversion: ConnectorConversionResponse | null
): string[] {
  if (!conversion) {
    return [];
  }
  return conversion.proposed_tools
    .filter((item) => !item.review_blocked)
    .map((item) => item.candidate_id);
}

export function parseJsonDraft(text: string, label: string): unknown | undefined {
  const trimmed = text.trim();
  if (!trimmed) {
    return undefined;
  }
  try {
    return JSON.parse(trimmed);
  } catch {
    throw new Error(`${label} must be valid JSON.`);
  }
}

export function stringifyJson(value: unknown): string {
  if (value == null) {
    return "{}";
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
