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
});
