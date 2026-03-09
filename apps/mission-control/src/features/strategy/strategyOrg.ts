import type { Agent } from "../../types";

export interface AgentOrgModel {
  agentsById: Map<string, Agent>;
  directReportsByManagerId: Map<string, Agent[]>;
  managerChainByAgentId: Map<string, Agent[]>;
  subtreeIdsByAgentId: Map<string, string[]>;
  rootAgents: Agent[];
}

function sortAgents(agents: Agent[]): Agent[] {
  return [...agents].sort((left, right) => left.name.localeCompare(right.name));
}

function buildSubtreeIds(
  agentId: string,
  reportsByManagerId: Map<string, Agent[]>,
  visited: Set<string>
): string[] {
  if (visited.has(agentId)) {
    return [];
  }
  visited.add(agentId);
  const descendants = reportsByManagerId.get(agentId) ?? [];
  const nested = descendants.flatMap((agent) =>
    buildSubtreeIds(agent.agent_id, reportsByManagerId, visited)
  );
  return [agentId, ...nested];
}

export function buildAgentOrgModel(agents: Agent[]): AgentOrgModel {
  const agentsById = new Map(
    agents.map((agent) => [agent.agent_id, agent] as const)
  );
  const directReportsByManagerId = new Map<string, Agent[]>();

  for (const agent of agents) {
    const managerId = agent.reports_to_agent_id?.trim();
    if (!managerId) {
      continue;
    }
    const reports = directReportsByManagerId.get(managerId) ?? [];
    reports.push(agent);
    directReportsByManagerId.set(managerId, sortAgents(reports));
  }

  const managerChainByAgentId = new Map<string, Agent[]>();
  for (const agent of agents) {
    const chain: Agent[] = [];
    const seen = new Set<string>();
    let current = agent;
    while (current.reports_to_agent_id) {
      const next = agentsById.get(current.reports_to_agent_id);
      if (!next || seen.has(next.agent_id)) {
        break;
      }
      chain.push(next);
      seen.add(next.agent_id);
      current = next;
    }
    managerChainByAgentId.set(agent.agent_id, chain);
  }

  const subtreeIdsByAgentId = new Map<string, string[]>();
  for (const agent of agents) {
    subtreeIdsByAgentId.set(
      agent.agent_id,
      buildSubtreeIds(agent.agent_id, directReportsByManagerId, new Set<string>())
    );
  }

  const rootAgents = sortAgents(
    agents.filter((agent) => {
      const managerId = agent.reports_to_agent_id?.trim();
      return !managerId || !agentsById.has(managerId);
    })
  );

  return {
    agentsById,
    directReportsByManagerId,
    managerChainByAgentId,
    subtreeIdsByAgentId,
    rootAgents,
  };
}

export function managerChainLabel(
  agentId: string | null | undefined,
  org: AgentOrgModel
): string | null {
  if (!agentId) {
    return null;
  }
  const chain = org.managerChainByAgentId.get(agentId) ?? [];
  if (chain.length === 0) {
    return null;
  }
  return chain.map((agent) => agent.name).join(" -> ");
}
