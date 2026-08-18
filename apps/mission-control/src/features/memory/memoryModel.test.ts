import { describe, expect, it } from "vitest";
import type { Agent, AgentMemoryStatusResponse } from "../../types";
import {
  canLoadMemoryReadSurfaces,
  getMemoryBindingCacheKey,
  isMemoryUnsupportedError,
  selectMemoryAgentId,
} from "./memoryModel";

function makeAgent(
  agent_id: string,
  options?: { bound?: boolean }
): Agent {
  return {
    agent_id,
    name: agent_id,
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    memory_binding: options?.bound
      ? {
          binding_id: `binding-${agent_id}`,
          provider_kind: "modelnumquamoblita",
          base_url: "http://127.0.0.1:4411",
          auth_mode: "none",
          enabled: true,
          trusted_local_operator_actions: true,
        }
      : null,
  };
}

function makeStatus(
  agentId: string,
  binding_status: string
): AgentMemoryStatusResponse {
  return {
    agent_id: agentId,
    binding_status,
    binding:
      binding_status === "unconfigured"
        ? null
        : {
            binding_id: `binding-${agentId}`,
            provider_kind: "modelnumquamoblita",
            base_url: "http://127.0.0.1:4411",
            auth_mode: "none",
            enabled: true,
            trusted_local_operator_actions: true,
          },
    native_surface_availability: {
      cards: true,
      card_detail: true,
      atom_detail: true,
      graph_overview: true,
      graph_neighbors: true,
      episodes: true,
      turn_why: true,
      citation_lookup: true,
      runtime_health: true,
      telemetry_summary: true,
      telemetry_turns: true,
      decision_reasons: true,
    },
    orchestration: {
      enabled: true,
      transport: "http",
      health_status: "ok",
      degrade_mode: false,
      last_error_code: null,
      last_error: null,
    },
    native_runtime_status: null,
    native_runtime_health_mismatch: false,
  };
}

describe("memoryModel", () => {
  it("prefers the current selected agent when still valid", () => {
    const agents = [makeAgent("root"), makeAgent("lyra", { bound: true })];
    expect(selectMemoryAgentId(agents, "lyra", "root")).toBe("root");
  });

  it("falls back to preferred then first bound agent", () => {
    const agents = [makeAgent("root"), makeAgent("lyra", { bound: true })];
    expect(selectMemoryAgentId(agents, "lyra", "")).toBe("lyra");
    expect(selectMemoryAgentId(agents, "missing", "")).toBe("lyra");
  });

  it("partitions cache by binding id when available", () => {
    expect(getMemoryBindingCacheKey("lyra", makeStatus("lyra", "available"))).toBe(
      "binding-lyra"
    );
    expect(getMemoryBindingCacheKey("root", makeStatus("root", "unconfigured"))).toBe(
      "agent:root:unconfigured"
    );
  });

  it("treats 404 memory route failures as unsupported", () => {
    expect(
      isMemoryUnsupportedError("404 Not Found: {\"error\":\"route not found\"}")
    ).toBe(true);
    expect(isMemoryUnsupportedError("500 Internal Server Error")).toBe(false);
  });

  it("only loads native read surfaces for available or degraded lanes", () => {
    expect(canLoadMemoryReadSurfaces(makeStatus("lyra", "available"))).toBe(true);
    expect(canLoadMemoryReadSurfaces(makeStatus("lyra", "degraded"))).toBe(true);
    expect(canLoadMemoryReadSurfaces(makeStatus("lyra", "unauthorized"))).toBe(false);
  });
});
