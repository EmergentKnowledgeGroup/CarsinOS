import { describe, expect, it } from "vitest";
import {
  isEligibleManagerForAgent,
  wouldCreateManagerCycle,
} from "./teamManagerValidation";

describe("teamManagerValidation", () => {
  const subtreeIdsByAgentId = new Map<string, string[]>([
    ["agent-root", ["agent-root", "agent-manager", "agent-report"]],
    ["agent-manager", ["agent-manager", "agent-report"]],
    ["agent-report", ["agent-report"]],
  ]);

  it("rejects self-manager assignments", () => {
    expect(
      wouldCreateManagerCycle("agent-manager", "agent-manager", subtreeIdsByAgentId)
    ).toBe(true);
  });

  it("rejects descendant manager assignments", () => {
    expect(
      wouldCreateManagerCycle("agent-manager", "agent-report", subtreeIdsByAgentId)
    ).toBe(true);
  });

  it("allows ancestor manager assignments", () => {
    expect(
      isEligibleManagerForAgent("agent-report", "agent-root", subtreeIdsByAgentId)
    ).toBe(true);
  });

  it("allows unrelated manager assignments", () => {
    const unrelatedSubtree = new Map(subtreeIdsByAgentId);
    unrelatedSubtree.set("agent-external", ["agent-external"]);

    expect(
      wouldCreateManagerCycle("agent-manager", "agent-external", unrelatedSubtree)
    ).toBe(false);
    expect(
      isEligibleManagerForAgent("agent-manager", "agent-external", unrelatedSubtree)
    ).toBe(true);
  });

  it("handles empty and whitespace ids without creating false cycles", () => {
    expect(wouldCreateManagerCycle("", "agent-root", subtreeIdsByAgentId)).toBe(false);
    expect(wouldCreateManagerCycle("   ", "agent-root", subtreeIdsByAgentId)).toBe(false);
    expect(isEligibleManagerForAgent("", "agent-root", subtreeIdsByAgentId)).toBe(true);
    expect(isEligibleManagerForAgent("agent-root", "   ", subtreeIdsByAgentId)).toBe(true);
  });
});
