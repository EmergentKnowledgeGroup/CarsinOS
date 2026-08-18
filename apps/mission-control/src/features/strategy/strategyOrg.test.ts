import { describe, expect, it } from "vitest";
import { buildAgentOrgModel, managerChainLabel } from "./strategyOrg";

describe("buildAgentOrgModel", () => {
  it("derives roots, manager chains, and subtree membership", () => {
    const org = buildAgentOrgModel([
      {
        agent_id: "agent-root",
        name: "Root",
        model_provider: "openai",
        model_id: "gpt-5",
        reports_to_agent_id: null,
        role_label: "Director",
      },
      {
        agent_id: "agent-manager",
        name: "Manager",
        model_provider: "openai",
        model_id: "gpt-5-mini",
        reports_to_agent_id: "agent-root",
        role_label: "Manager",
      },
      {
        agent_id: "agent-ic",
        name: "IC",
        model_provider: "anthropic",
        model_id: "claude",
        reports_to_agent_id: "agent-manager",
        role_label: "Worker",
      },
      {
        agent_id: "agent-orphan",
        name: "Orphan",
        model_provider: "local",
        model_id: "llama",
        reports_to_agent_id: "missing-manager",
        role_label: "Specialist",
      },
    ]);

    expect(org.rootAgents.map((agent) => agent.agent_id)).toEqual([
      "agent-orphan",
      "agent-root",
    ]);
    expect(
      org.directReportsByManagerId.get("agent-root")?.map((agent) => agent.agent_id)
    ).toEqual(["agent-manager"]);
    expect(
      org.managerChainByAgentId.get("agent-ic")?.map((agent) => agent.agent_id)
    ).toEqual(["agent-manager", "agent-root"]);
    expect(org.subtreeIdsByAgentId.get("agent-root")).toEqual([
      "agent-root",
      "agent-manager",
      "agent-ic",
    ]);
  });

  it("formats manager-chain labels from the derived model", () => {
    const org = buildAgentOrgModel([
      {
        agent_id: "root",
        name: "Root",
        model_provider: "openai",
        model_id: "gpt-5",
        reports_to_agent_id: null,
        role_label: null,
      },
      {
        agent_id: "worker",
        name: "Worker",
        model_provider: "openai",
        model_id: "gpt-5-mini",
        reports_to_agent_id: "root",
        role_label: null,
      },
    ]);

    expect(managerChainLabel("worker", org)).toBe("Root");
    expect(managerChainLabel("root", org)).toBeNull();
    expect(managerChainLabel(null, org)).toBeNull();
  });

  it("trims manager ids and avoids self/cycle pollution in the chain", () => {
    const org = buildAgentOrgModel([
      {
        agent_id: "root",
        name: "Root",
        model_provider: "openai",
        model_id: "gpt-5",
        reports_to_agent_id: null,
        role_label: null,
      },
      {
        agent_id: "worker",
        name: "Worker",
        model_provider: "openai",
        model_id: "gpt-5-mini",
        reports_to_agent_id: " root ",
        role_label: null,
      },
      {
        agent_id: "self-loop",
        name: "Self Loop",
        model_provider: "local",
        model_id: "llama",
        reports_to_agent_id: "self-loop",
        role_label: null,
      },
    ]);

    expect(org.managerChainByAgentId.get("worker")?.map((agent) => agent.agent_id)).toEqual([
      "root",
    ]);
    expect(org.managerChainByAgentId.get("self-loop")).toEqual([]);
  });
});
