import type { BootstrapPresetResponse } from "../../types";

export interface BootstrapPresetDraftFields {
  role_label: string;
  model_provider: string;
  model_id: string;
  tool_profile: string;
  workspace_root: string;
  reports_to_agent_id: string;
  preset_key?: string;
}

export function applyBootstrapPresetToDraft<T extends BootstrapPresetDraftFields>(
  draft: T,
  preset: BootstrapPresetResponse
): T {
  return {
    ...draft,
    preset_key: preset.preset_key,
    role_label: preset.role_label || draft.role_label,
    model_provider: preset.default_model_provider ?? draft.model_provider,
    model_id: preset.default_model_id ?? draft.model_id,
    tool_profile: preset.default_tool_profile ?? draft.tool_profile,
    workspace_root: preset.default_workspace_root ?? draft.workspace_root,
    reports_to_agent_id:
      preset.default_reports_to_agent_id ?? draft.reports_to_agent_id,
  };
}
