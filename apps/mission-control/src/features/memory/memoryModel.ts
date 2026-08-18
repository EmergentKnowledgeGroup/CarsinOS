import type { Agent, AgentMemoryStatusResponse } from "../../types";
import { MEMORY_UNSUPPORTED_STATUS_FRAGMENTS } from "./memoryConfig";

export function normalizeMemoryErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function isMemoryUnsupportedError(error: unknown): boolean {
  const message = normalizeMemoryErrorMessage(error).toLowerCase();
  return MEMORY_UNSUPPORTED_STATUS_FRAGMENTS.some((fragment) =>
    message.includes(fragment)
  );
}

export function selectMemoryAgentId(
  agents: Agent[],
  preferredAgentId: string | null | undefined,
  currentAgentId: string | null | undefined
): string {
  if (currentAgentId && agents.some((agent) => agent.agent_id === currentAgentId)) {
    return currentAgentId;
  }
  if (preferredAgentId && agents.some((agent) => agent.agent_id === preferredAgentId)) {
    return preferredAgentId;
  }
  const boundAgent = agents.find((agent) => agent.memory_binding?.enabled);
  return boundAgent?.agent_id ?? agents[0]?.agent_id ?? "";
}

export function getMemoryBindingCacheKey(
  agentId: string,
  status: AgentMemoryStatusResponse | null
): string {
  const bindingId = status?.binding?.binding_id?.trim();
  if (bindingId) {
    return bindingId;
  }
  const bindingStatus = status?.binding_status?.trim() || "unconfigured";
  return `agent:${agentId}:${bindingStatus}`;
}

export function canLoadMemoryReadSurfaces(
  status: AgentMemoryStatusResponse | null
): boolean {
  return (
    status?.binding_status === "available" ||
    status?.binding_status === "degraded"
  );
}
