import { describe, expect, it } from "vitest";
import { applyBootstrapPresetToDraft } from "./bootstrapPresetUtils";

describe("applyBootstrapPresetToDraft", () => {
  it("applies preset defaults including manager hierarchy", () => {
    const next = applyBootstrapPresetToDraft(
      {
        preset_key: "",
        role_label: "Operator",
        model_provider: "anthropic",
        model_id: "claude",
        tool_profile: "restricted",
        workspace_root: "/tmp/work",
        reports_to_agent_id: "",
      },
      {
        schema_version: "bootstrap-preset-v1",
        preset_key: "ops-lead",
        display_name: "Ops Lead",
        description: "Coordinates incident work",
        role_label: "Ops Lead",
        provider_path: "openai",
        default_model_provider: "openai",
        default_model_id: "gpt-5",
        default_tool_profile: "standard",
        default_workspace_root: "/repo",
        default_reports_to_agent_id: "agent-root",
        setup_notes: "none",
        created_at: 1,
        updated_at: 2,
      }
    );

    expect(next).toEqual({
      preset_key: "ops-lead",
      role_label: "Ops Lead",
      model_provider: "openai",
      model_id: "gpt-5",
      tool_profile: "standard",
      workspace_root: "/repo",
      reports_to_agent_id: "agent-root",
    });
  });

  it("preserves draft values when preset omits optional defaults", () => {
    const next = applyBootstrapPresetToDraft(
      {
        preset_key: "",
        role_label: "Operator",
        model_provider: "anthropic",
        model_id: "claude",
        tool_profile: "restricted",
        workspace_root: "/tmp/work",
        reports_to_agent_id: "agent-root",
      },
      {
        schema_version: "bootstrap-preset-v1",
        preset_key: "fallback",
        display_name: "Fallback",
        description: "",
        role_label: "",
        provider_path: "local",
        default_model_provider: null,
        default_model_id: null,
        default_tool_profile: null,
        default_workspace_root: null,
        default_reports_to_agent_id: null,
        setup_notes: null,
        created_at: 1,
        updated_at: 2,
      }
    );

    expect(next).toEqual({
      preset_key: "fallback",
      role_label: "Operator",
      model_provider: "anthropic",
      model_id: "claude",
      tool_profile: "restricted",
      workspace_root: "/tmp/work",
      reports_to_agent_id: "agent-root",
    });
  });
});
