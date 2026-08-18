export function wouldCreateManagerCycle(
  agentId: string,
  managerId: string,
  subtreeIdsByAgentId: Map<string, string[]>
): boolean {
  const normalizedAgentId = agentId.trim();
  const normalizedManagerId = managerId.trim();
  if (!normalizedAgentId || !normalizedManagerId) {
    return false;
  }
  if (normalizedAgentId === normalizedManagerId) {
    return true;
  }
  return (
    subtreeIdsByAgentId.get(normalizedAgentId)?.includes(normalizedManagerId) ?? false
  );
}

export function isEligibleManagerForAgent(
  agentId: string,
  managerId: string,
  subtreeIdsByAgentId: Map<string, string[]>
): boolean {
  return !wouldCreateManagerCycle(agentId, managerId, subtreeIdsByAgentId);
}
