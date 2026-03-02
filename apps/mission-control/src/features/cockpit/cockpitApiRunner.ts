/* ── Cockpit API Runner — dynamic dispatch for custom widgets ──────────────── */

import type { RuntimeConnectionSettings } from "../../types";
import * as api from "../../lib/api";

type RunnerFn = (
  settings: RuntimeConnectionSettings,
  params: Record<string, string>,
) => Promise<unknown>;

const REQUIRED_PARAMS: Record<string, string[]> = {
  getAgentProviderProfileOrder: ["agentId", "provider"],
  getBoard: ["boardId"],
  getAgentMailThread: ["threadId"],
  listAgentMailMessages: ["threadId"],
};

const DISPATCH: Record<string, RunnerFn> = {
  /* ── Health ── */
  getGatewayHealth: (s) => api.getGatewayHealth(s),
  getGatewayStatus: (s) => api.getGatewayStatus(s),
  getJobsStatus: (s) => api.getJobsStatus(s),
  getChannelRuntimeStatus: (s) => api.getChannelRuntimeStatus(s),

  /* ── Agents ── */
  listAgents: (s) => api.listAgents(s),
  getAgentProviderProfileOrder: (s, p) =>
    api.getAgentProviderProfileOrder(s, p.agentId ?? "", p.provider ?? ""),

  /* ── Boards ── */
  listBoards: (s) => api.listBoards(s),
  getBoard: (s, p) => api.getBoard(s, p.boardId ?? ""),

  /* ── Jobs ── */
  listJobs: (s) => api.listJobs(s),

  /* ── Focus ── */
  getMissionControlFocus: (s) => api.getMissionControlFocus(s),
  getMissionControlCalendarWeek: (s) => api.getMissionControlCalendarWeek(s),

  /* ── Approvals ── */
  listApprovals: (s, p) => api.listApprovals(s, p.status || "requested"),

  /* ── Extensions ── */
  listAuthProfiles: (s, p) =>
    api.listAuthProfiles(s, { provider: p.provider || undefined }),
  listSkills: (s) => api.listSkills(s),
  listPlugins: (s) => api.listPlugins(s),
  listPluginRuntimeStatus: (s) => api.listPluginRuntimeStatus(s),

  /* ── Memory ── */
  listMemoryNotes: (s) => api.listMemoryNotes(s),

  /* ── Mail ── */
  listAgentMailThreads: (s) => api.listAgentMailThreads(s),
  getAgentMailThread: (s, p) =>
    api.getAgentMailThread(s, p.threadId ?? ""),
  listAgentMailMessages: (s, p) =>
    api.listAgentMailMessages(s, p.threadId ?? ""),
  listAgentMailFileLeases: (s) => api.listAgentMailFileLeases(s),
};

/**
 * Runs a cockpit data source by ID, injecting params into the appropriate API
 * function. Returns the raw response object/array from the gateway.
 */
export async function runCockpitDataSource(
  id: string,
  settings: RuntimeConnectionSettings,
  params?: Record<string, string>,
): Promise<unknown> {
  const runner = DISPATCH[id];
  if (!runner) {
    throw new Error(`Unknown cockpit data source: "${id}"`);
  }
  const normalizedParams = params ?? {};
  for (const key of REQUIRED_PARAMS[id] ?? []) {
    if (!normalizedParams[key]?.trim()) {
      throw new Error(`Missing required parameter "${key}" for data source "${id}".`);
    }
  }
  return runner(settings, normalizedParams);
}

/**
 * Traverse a dot-notation path into a data structure.
 * e.g. `resolveResponsePath(data, "agents")` returns `data.agents`.
 * Returns `data` unchanged if path is empty/undefined.
 */
export function resolveResponsePath(
  data: unknown,
  path?: string,
): unknown {
  if (!path || !path.trim()) return data;
  let current: unknown = data;
  for (const segment of path.split(".")) {
    if (current == null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}
