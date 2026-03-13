import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Bot,
  Download,
  GitBranch,
  Pencil,
  Plus,
  Save,
  Upload,
} from "lucide-react";
import clsx from "clsx";
import { EmptyState } from "../../ui/EmptyState";
import { Modal } from "../../ui/Modal";
import { Pagination } from "../../ui/Pagination";
import { Surface } from "../../ui/Surface";
import { usePagination } from "../../ui/usePagination";
import {
  createAgent,
  listProviderCapabilities,
  listProviderModels,
  updateAgent,
} from "../../lib/api";
import { providerLabel } from "../../lib/providerCatalog";
import type {
  Agent,
  BootstrapPresetResponse,
  ProviderCapabilityResponse,
  RuntimeConnectionSettings,
} from "../../types";
import type { useStrategyController } from "../strategy/useStrategyController";
import { applyBootstrapPresetToDraft } from "../strategy/bootstrapPresetUtils";
import { managerChainLabel } from "../strategy/strategyOrg";
import { isEligibleManagerForAgent } from "./teamManagerValidation";

const PAGE_SIZE = 5;

interface TeamPageProps {
  agents: Agent[];
  activeJobCount: number;
  settings: RuntimeConnectionSettings;
  strategyController: ReturnType<typeof useStrategyController>;
  onRefresh: () => void | Promise<void>;
}

interface AgentFormState {
  agent_id: string;
  name: string;
  model_provider: string;
  model_id: string;
  tool_profile: string;
  workspace_root: string;
  reports_to_agent_id: string;
  role_label: string;
  preset_key: string;
}

interface PresetFormState {
  preset_key: string;
  display_name: string;
  description: string;
  role_label: string;
  provider_path: string;
  default_model_provider: string;
  default_model_id: string;
  default_tool_profile: string;
  default_workspace_root: string;
  default_reports_to_agent_id: string;
  setup_notes: string;
}

const EMPTY_FORM: AgentFormState = {
  agent_id: "",
  name: "",
  model_provider: "",
  model_id: "",
  tool_profile: "standard",
  workspace_root: "",
  reports_to_agent_id: "",
  role_label: "",
  preset_key: "",
};

const EMPTY_PRESET_FORM: PresetFormState = {
  preset_key: "",
  display_name: "",
  description: "",
  role_label: "",
  provider_path: "local",
  default_model_provider: "",
  default_model_id: "",
  default_tool_profile: "standard",
  default_workspace_root: ".",
  default_reports_to_agent_id: "",
  setup_notes: "",
};

const TOOL_PROFILES = [
  { value: "standard", label: "Standard" },
  { value: "restricted", label: "Restricted" },
  { value: "none", label: "None" },
];

const FALLBACK_PROVIDER_OPTIONS = [
  { value: "openai", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic" },
  { value: "other", label: "Other" },
];

function toPresetFormState(
  preset: BootstrapPresetResponse | null,
  form: AgentFormState
): PresetFormState {
  if (preset) {
    return {
      preset_key: preset.preset_key,
      display_name: preset.display_name,
      description: preset.description,
      role_label: preset.role_label,
      provider_path: preset.provider_path,
      default_model_provider: preset.default_model_provider ?? "",
      default_model_id: preset.default_model_id ?? "",
      default_tool_profile: preset.default_tool_profile ?? "standard",
      default_workspace_root: preset.default_workspace_root ?? ".",
      default_reports_to_agent_id: preset.default_reports_to_agent_id ?? "",
      setup_notes: preset.setup_notes ?? "",
    };
  }
  return {
    ...EMPTY_PRESET_FORM,
    role_label: form.role_label,
    default_model_provider: form.model_provider,
    default_model_id: form.model_id,
    default_tool_profile: form.tool_profile,
    default_workspace_root: form.workspace_root || ".",
    default_reports_to_agent_id: form.reports_to_agent_id,
  };
}

function downloadJson(filename: string, payload: unknown) {
  const blob = new Blob([JSON.stringify(payload, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function TeamOrgNode({
  agent,
  agentsById,
  directReportsByManagerId,
}: {
  agent: Agent;
  agentsById: Map<string, Agent>;
  directReportsByManagerId: Map<string, Agent[]>;
}) {
  const reports = directReportsByManagerId.get(agent.agent_id) ?? [];
  return (
    <div className="mc-team-org-node">
      <div className="mc-team-org-card">
        <strong>{agent.name}</strong>
        <span>{agent.role_label || agent.agent_id}</span>
        {agent.reports_to_agent_id ? (
          <small>
            reports to {agentsById.get(agent.reports_to_agent_id)?.name ?? agent.reports_to_agent_id}
          </small>
        ) : (
          <small>root owner</small>
        )}
      </div>
      {reports.length > 0 ? (
        <div className="mc-team-org-children">
          {reports.map((report) => (
            <TeamOrgNode
              key={report.agent_id}
              agent={report}
              agentsById={agentsById}
              directReportsByManagerId={directReportsByManagerId}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function TeamPage({
  agents,
  activeJobCount,
  settings,
  strategyController,
  onRefresh,
}: TeamPageProps) {
  const [page, setPage] = useState(1);
  const [modalMode, setModalMode] = useState<"create" | "edit" | null>(null);
  const [form, setForm] = useState<AgentFormState>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [showOrgView, setShowOrgView] = useState(false);
  const [providerCapabilities, setProviderCapabilities] = useState<
    ProviderCapabilityResponse[]
  >([]);
  const [providerCapabilitiesLoading, setProviderCapabilitiesLoading] = useState(false);
  const [providerCapabilitiesError, setProviderCapabilitiesError] = useState<string | null>(null);
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, string[]>>({});
  const [modelErrorsByProvider, setModelErrorsByProvider] = useState<
    Record<string, string | null>
  >({});
  const [modelLoadingProvider, setModelLoadingProvider] = useState<string | null>(null);
  const [roleCardAgentId, setRoleCardAgentId] = useState<string | null>(null);
  const [presetModalMode, setPresetModalMode] = useState<"create" | "edit" | null>(null);
  const [presetForm, setPresetForm] = useState<PresetFormState>(EMPTY_PRESET_FORM);
  const [presetError, setPresetError] = useState<string | null>(null);
  const [presetSaving, setPresetSaving] = useState(false);
  const importFileRef = useRef<HTMLInputElement | null>(null);

  const strategyEnabled = strategyController.enabled;
  const { totalPages, getPage } = usePagination(agents, PAGE_SIZE);
  const visibleAgents = getPage(page);
  const roleCardAgent = roleCardAgentId ? agents.find((a) => a.agent_id === roleCardAgentId) : null;

  const knownWorkspaces = useMemo(() => {
    const values = new Set<string>();
    for (const agent of agents) {
      if (agent.workspace_root) {
        values.add(agent.workspace_root);
      }
    }
    return Array.from(values).sort();
  }, [agents]);

  const managerOptions = useMemo(
    () =>
      agents.filter((agent) =>
        isEligibleManagerForAgent(
          form.agent_id,
          agent.agent_id,
          strategyController.org.subtreeIdsByAgentId
        )
      ),
    [agents, form.agent_id, strategyController.org.subtreeIdsByAgentId]
  );
  const managerSelectionInvalid = useMemo(
    () =>
      !isEligibleManagerForAgent(
        form.agent_id,
        form.reports_to_agent_id,
        strategyController.org.subtreeIdsByAgentId
      ),
    [form.agent_id, form.reports_to_agent_id, strategyController.org.subtreeIdsByAgentId]
  );

  const providerOptions = useMemo(() => {
    const mapped = providerCapabilities
      .filter((item) => item.provider !== "unconfigured")
      .map((item) => ({
        value: item.provider,
        label: providerLabel(item.provider),
      }))
      .sort((left, right) => left.label.localeCompare(right.label));
    const useFallback = Boolean(providerCapabilitiesError) || mapped.length === 0;
    const base = useFallback ? [...FALLBACK_PROVIDER_OPTIONS] : mapped;
    if (form.model_provider && !base.some((item) => item.value === form.model_provider)) {
      base.push({
        value: form.model_provider,
        label: providerLabel(form.model_provider),
      });
    }
    return base;
  }, [form.model_provider, providerCapabilities, providerCapabilitiesError]);

  const modelOptions = useMemo(() => {
    const provider = form.model_provider.trim().toLowerCase();
    const options = modelsByProvider[provider] ?? [];
    if (form.model_id && !options.includes(form.model_id)) {
      return [form.model_id, ...options];
    }
    return options;
  }, [form.model_id, form.model_provider, modelsByProvider]);

  const modelFetchError = useMemo(() => {
    const provider = form.model_provider.trim().toLowerCase();
    return modelErrorsByProvider[provider] ?? null;
  }, [form.model_provider, modelErrorsByProvider]);

  useEffect(() => {
    if (modalMode === null) {
      return;
    }
    let cancelled = false;
    setProviderCapabilitiesLoading(true);
    setProviderCapabilitiesError(null);
    void listProviderCapabilities(settings)
      .then((response) => {
        if (!cancelled) {
          setProviderCapabilities(response.items);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          console.error("provider capability catalog load failed", err);
          setProviderCapabilitiesError(String(err));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setProviderCapabilitiesLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [modalMode, settings]);

  useEffect(() => {
    if (modalMode === null) {
      return;
    }
    const provider = form.model_provider.trim().toLowerCase();
    const agentId = form.agent_id.trim() || undefined;
    if (!provider) {
      setModelLoadingProvider(null);
      return;
    }
    let cancelled = false;
    setModelLoadingProvider(provider);
    setModelErrorsByProvider((prev) => ({ ...prev, [provider]: null }));
    const timeoutId = window.setTimeout(() => {
      void listProviderModels(settings, {
        provider,
        agent_id: agentId,
      })
        .then((response) => {
          if (cancelled) {
            return;
          }
          const modelIds = response.items.map((item) => item.model_id);
          setModelsByProvider((prev) => ({ ...prev, [provider]: modelIds }));
          if (modelIds.length > 0) {
            setForm((prev) => {
              if (
                prev.model_provider.trim().toLowerCase() !== provider ||
                prev.model_id.trim()
              ) {
                return prev;
              }
              return {
                ...prev,
                model_id: modelIds[0],
              };
            });
          }
        })
        .catch((err: unknown) => {
          if (!cancelled) {
            setModelErrorsByProvider((prev) => ({ ...prev, [provider]: String(err) }));
          }
        })
        .finally(() => {
          if (!cancelled) {
            setModelLoadingProvider((current) => (current === provider ? null : current));
          }
        });
    }, 400);
    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [form.agent_id, form.model_provider, modalMode, settings]);

  const refreshAll = useCallback(async () => {
    await Promise.resolve(onRefresh());
    strategyController.queueRefresh(settings);
  }, [onRefresh, settings, strategyController]);

  const openCreate = useCallback(() => {
    setForm(EMPTY_FORM);
    setError(null);
    setModelsByProvider({});
    setModelErrorsByProvider({});
    setModalMode("create");
  }, []);

  const openEdit = useCallback((agent: Agent) => {
    setForm({
      agent_id: agent.agent_id,
      name: agent.name,
      model_provider: agent.model_provider,
      model_id: agent.model_id,
      tool_profile: agent.tool_profile ?? "standard",
      workspace_root: agent.workspace_root ?? "",
      reports_to_agent_id: agent.reports_to_agent_id ?? "",
      role_label: agent.role_label ?? "",
      preset_key: "",
    });
    setError(null);
    setModelsByProvider({});
    setModelErrorsByProvider({});
    setModalMode("edit");
  }, []);

  const closeModal = useCallback(() => {
    setModalMode(null);
    setError(null);
  }, []);

  const updateField = useCallback(
    <K extends keyof AgentFormState>(key: K, value: AgentFormState[K]) => {
      setForm((prev) => ({ ...prev, [key]: value }));
    },
    []
  );

  const handleApplyPreset = useCallback(
    (presetKey: string) => {
      const preset = strategyController.presets.find((item) => item.preset_key === presetKey);
      if (!preset) {
        return;
      }
      setForm((prev) => applyBootstrapPresetToDraft(prev, preset));
    },
    [strategyController.presets]
  );

  const handleSave = useCallback(async () => {
    if (!form.agent_id.trim() || !form.name.trim()) {
      setError("Agent ID and Name are required.");
      return;
    }
    if (managerSelectionInvalid) {
      setError("Reports To cannot point to the same agent or one of its own descendants.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const createPayload = {
        agent_id: form.agent_id.trim(),
        name: form.name.trim(),
        model_provider: form.model_provider || undefined,
        model_id: form.model_id || undefined,
        tool_profile: form.tool_profile || undefined,
        workspace_root: form.workspace_root || undefined,
        reports_to_agent_id: form.reports_to_agent_id || null,
        role_label: form.role_label || null,
      };
      const updatePayload = {
        name: createPayload.name,
        model_provider: createPayload.model_provider,
        model_id: createPayload.model_id,
        tool_profile: createPayload.tool_profile,
        workspace_root: createPayload.workspace_root,
        reports_to_agent_id: createPayload.reports_to_agent_id,
        role_label: createPayload.role_label,
      };
      if (modalMode === "create") {
        await createAgent(settings, createPayload);
      } else {
        await updateAgent(settings, form.agent_id, updatePayload);
      }
      closeModal();
      await refreshAll();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [closeModal, form, managerSelectionInvalid, modalMode, refreshAll, settings]);

  const exportPreset = useCallback(
    async (presetKey: string) => {
      const response = await strategyController.exportBootstrapPreset(presetKey);
      downloadJson(`${response.preset.preset_key}.json`, response.preset);
    },
    [strategyController]
  );

  const savePreset = useCallback(async () => {
    if (!presetForm.preset_key.trim() || !presetForm.display_name.trim()) {
      setPresetError("Preset key and display name are required.");
      return;
    }
    setPresetSaving(true);
    setPresetError(null);
    try {
      const payload = {
        preset_key: presetForm.preset_key.trim(),
        display_name: presetForm.display_name.trim(),
        description: presetForm.description.trim() || null,
        role_label: presetForm.role_label.trim(),
        provider_path: presetForm.provider_path,
        default_model_provider: presetForm.default_model_provider.trim() || null,
        default_model_id: presetForm.default_model_id.trim() || null,
        default_tool_profile: presetForm.default_tool_profile.trim() || null,
        default_workspace_root: presetForm.default_workspace_root.trim() || null,
        default_reports_to_agent_id:
          presetForm.default_reports_to_agent_id.trim() || null,
        setup_notes: presetForm.setup_notes.trim() || null,
      };
      if (presetModalMode === "create") {
        await strategyController.createBootstrapPreset(payload);
      } else {
        await strategyController.updateBootstrapPreset(presetForm.preset_key, payload);
      }
      setPresetModalMode(null);
    } catch (err) {
      setPresetError(String(err));
    } finally {
      setPresetSaving(false);
    }
  }, [presetForm, presetModalMode, strategyController]);

  const handleImportPresetFile = useCallback(
    async (file: File) => {
      const raw = await file.text();
      const payload = JSON.parse(raw) as Record<string, unknown>;
      try {
        await strategyController.importBootstrapPreset({ payload });
      } catch (error) {
        const message = String(error);
        if (message.includes("409")) {
          const overwrite = window.confirm(
            "A preset with that key already exists. Overwrite it?"
          );
          if (overwrite) {
            await strategyController.importBootstrapPreset({
              payload,
              overwrite: true,
            });
            return;
          }
        }
        throw error;
      }
    },
    [strategyController]
  );

  const roleCardReports = roleCardAgent
    ? strategyController.org.directReportsByManagerId.get(roleCardAgent.agent_id) ?? []
    : [];

  return (
    <div className="mc-team-page">
      <div className="mc-team-header">
        <div>
          <h2>Meet Your Agents</h2>
          <p className="mc-team-stats">
            <span>{agents.length} agent{agents.length !== 1 ? "s" : ""}</span>
            <span className="mc-team-stats-sep">&middot;</span>
            <span>
              {new Set(agents.map((a) => a.model_provider)).size} provider
              {new Set(agents.map((a) => a.model_provider)).size !== 1 ? "s" : ""}
            </span>
            <span className="mc-team-stats-sep">&middot;</span>
            <span>{activeJobCount} active job{activeJobCount !== 1 ? "s" : ""}</span>
          </p>
        </div>
        <div className="mc-strategy-inline-actions">
          {strategyEnabled ? (
            <>
              <button
                type="button"
                className="ghost"
                onClick={() => setShowOrgView((value) => !value)}
              >
                <GitBranch size={14} />
                {showOrgView ? "Hide Org View" : "Org View"}
              </button>
              <button
                type="button"
                className="ghost"
                onClick={() => importFileRef.current?.click()}
              >
                <Upload size={14} />
                Import Preset
              </button>
            </>
          ) : null}
          <button type="button" className="mc-btn mc-btn-accent" onClick={openCreate}>
            <Plus size={14} />
            New Agent
          </button>
        </div>
      </div>

      {strategyEnabled ? (
        <Surface
          className="mc-team-preset-surface"
          title="Bootstrap Presets"
          subtitle="Reusable setup defaults for role, provider/model, tools, workspace, and manager."
          headerRight={
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setPresetError(null);
                setPresetForm(toPresetFormState(null, form));
                setPresetModalMode("create");
              }}
            >
              <Save size={14} />
              Save Draft As Preset
            </button>
          }
        >
          <div className="mc-team-preset-grid">
            {strategyController.presets.map((preset) => (
              <article key={preset.preset_key} className="mc-team-preset-card">
                <div className="mc-team-preset-head">
                  <strong>{preset.display_name}</strong>
                  <span>{preset.preset_key}</span>
                </div>
                <p>{preset.description || "No preset description yet."}</p>
                <div className="mc-team-card-tags">
                  <span className="mc-chip mc-chip-muted">
                    {preset.role_label || "role"}
                  </span>
                  <span className="mc-chip mc-chip-muted">
                    {preset.default_model_provider ?? preset.provider_path}
                  </span>
                  {preset.default_reports_to_agent_id ? (
                    <span className="mc-chip mc-chip-muted">
                      mgr:{" "}
                      {strategyController.org.agentsById.get(
                        preset.default_reports_to_agent_id
                      )?.name ?? preset.default_reports_to_agent_id}
                    </span>
                  ) : null}
                </div>
                <div className="mc-strategy-inline-actions">
                  <button
                    type="button"
                    className="ghost"
                      onClick={() => {
                        openCreate();
                        setForm((prev) => applyBootstrapPresetToDraft(prev, preset));
                      }}
                  >
                    Apply
                  </button>
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => {
                      setPresetError(null);
                      setPresetForm(toPresetFormState(preset, form));
                      setPresetModalMode("edit");
                    }}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => void exportPreset(preset.preset_key)}
                  >
                    <Download size={14} />
                    Export
                  </button>
                </div>
              </article>
            ))}
            {strategyController.presets.length === 0 ? (
              <EmptyState message="No bootstrap presets yet. Save one from an agent draft to start standardizing setup." />
            ) : null}
          </div>
        </Surface>
      ) : null}

      {strategyEnabled && showOrgView ? (
        <Surface
          className="mc-team-org-surface"
          title="Org View"
          subtitle="Hierarchy is suggestive in Phase 1: ownership context, reporting lines, and subtree filters."
        >
          <div className="mc-team-org-tree">
            {strategyController.org.rootAgents.map((agent) => (
              <TeamOrgNode
                key={agent.agent_id}
                agent={agent}
                agentsById={strategyController.org.agentsById}
                directReportsByManagerId={strategyController.org.directReportsByManagerId}
              />
            ))}
            {strategyController.org.rootAgents.length === 0 ? (
              <EmptyState message="No hierarchy roots yet. Assign managers to expose org structure." />
            ) : null}
          </div>
        </Surface>
      ) : null}

      <div className="mc-team-roster">
        {visibleAgents.length === 0 ? (
          <div className="mc-team-empty">
            <Bot size={40} />
            <p>No agents registered yet</p>
            <p className="mc-team-empty-sub">Create your first agent to get started.</p>
          </div>
        ) : (
          visibleAgents.map((agent) => (
            <div key={agent.agent_id} className="mc-team-card">
              <div className="mc-team-card-avatar">
                {agent.name.charAt(0).toUpperCase()}
              </div>
              <div className="mc-team-card-info">
                <div className="mc-team-card-name">{agent.name}</div>
                <div className="mc-team-card-id">{agent.agent_id}</div>
                {agent.role_label ? (
                  <div className="mc-team-card-meta">Role: {agent.role_label}</div>
                ) : null}
                <div className="mc-team-card-meta">
                  Model: {agent.model_provider} / {agent.model_id}
                </div>
                {agent.tool_profile ? (
                  <div className="mc-team-card-meta">Tools: {agent.tool_profile}</div>
                ) : null}
                {agent.workspace_root ? (
                  <div className="mc-team-card-meta">Workspace: {agent.workspace_root}</div>
                ) : null}
                {agent.memory_binding?.enabled ? (
                  <div className="mc-team-card-meta">
                    Memory lane: {agent.memory_binding.binding_id}
                  </div>
                ) : null}
                {agent.reports_to_agent_id ? (
                  <div className="mc-team-card-meta">
                    Reports to{" "}
                    {strategyController.org.agentsById.get(agent.reports_to_agent_id)?.name ??
                      agent.reports_to_agent_id}
                  </div>
                ) : null}
                <div className="mc-team-card-tags">
                  <span className="mc-chip mc-chip-muted">{agent.model_provider}</span>
                  <span className="mc-chip mc-chip-muted">
                    {agent.tool_profile ?? "standard"}
                  </span>
                  {agent.memory_binding?.enabled ? (
                    <span className="mc-chip mc-chip-muted">memory bound</span>
                  ) : null}
                  {agent.role_label ? (
                    <span className="mc-chip mc-chip-muted">{agent.role_label}</span>
                  ) : null}
                </div>
              </div>
              <div className="mc-team-card-actions">
                <button
                  type="button"
                  className="mc-topbar-icon-btn"
                  onClick={() => openEdit(agent)}
                  title="Edit agent"
                >
                  <Pencil size={14} />
                </button>
                <button
                  type="button"
                  className="mc-btn mc-btn-sm"
                  onClick={() => setRoleCardAgentId(agent.agent_id)}
                >
                  Role Card
                </button>
              </div>
            </div>
          ))
        )}
      </div>

      <Pagination currentPage={page} totalPages={totalPages} onPageChange={setPage} />

      <Modal
        open={modalMode !== null}
        onClose={closeModal}
        title={modalMode === "create" ? "Create Agent" : "Edit Agent"}
        footer={
          <div className="mc-modal-actions">
            <button type="button" className="mc-btn" onClick={closeModal}>
              Cancel
            </button>
            <button
              type="button"
              className={clsx("mc-btn mc-btn-accent", saving && "mc-btn-loading")}
              onClick={() => void handleSave()}
              disabled={saving}
            >
              {modalMode === "create" ? "Create Agent" : "Save Changes"}
            </button>
          </div>
        }
      >
        <div className="mc-modal-form">
          {error ? <div className="mc-form-error">{error}</div> : null}

          {strategyEnabled ? (
            <div className="mc-team-preset-inline">
              <label className="mc-modal-field">
                <span>Preset</span>
                <select
                  value={form.preset_key}
                  onChange={(event) => updateField("preset_key", event.target.value)}
                >
                  <option value="">No preset</option>
                  {strategyController.presets.map((preset) => (
                    <option key={preset.preset_key} value={preset.preset_key}>
                      {preset.display_name}
                    </option>
                  ))}
                </select>
              </label>
              <div className="mc-strategy-inline-actions">
                <button
                  type="button"
                  className="ghost"
                  disabled={!form.preset_key}
                  onClick={() => handleApplyPreset(form.preset_key)}
                >
                  Apply Preset
                </button>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    setPresetError(null);
                    setPresetForm(toPresetFormState(null, form));
                    setPresetModalMode("create");
                  }}
                >
                  Save As Preset
                </button>
              </div>
            </div>
          ) : null}

          <label className="mc-modal-field">
            <span>Agent ID</span>
            <input
              value={form.agent_id}
              onChange={(e) => updateField("agent_id", e.target.value)}
              placeholder="e.g. agent-alpha"
              disabled={modalMode === "edit"}
            />
          </label>

          <label className="mc-modal-field">
            <span>Name</span>
            <input
              value={form.name}
              onChange={(e) => updateField("name", e.target.value)}
              placeholder="Display name"
            />
          </label>

          <div className="mc-field-grid">
            <label className="mc-modal-field">
              <span>Role Label</span>
              <input
                value={form.role_label}
                onChange={(e) => updateField("role_label", e.target.value)}
                placeholder="Operations Lead"
              />
            </label>

            <label className="mc-modal-field">
              <span>Reports To</span>
              <select
                value={form.reports_to_agent_id}
                onChange={(e) => updateField("reports_to_agent_id", e.target.value)}
              >
                <option value="">No manager</option>
                {managerOptions.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name}
                  </option>
                ))}
              </select>
              {managerSelectionInvalid ? (
                <small className="mc-form-error">
                  This manager choice would create an agent hierarchy cycle.
                </small>
              ) : null}
            </label>
          </div>

          <label className="mc-modal-field">
            <span>Provider</span>
            <select
              value={form.model_provider}
              onChange={(e) => {
                updateField("model_provider", e.target.value);
                updateField("model_id", "");
              }}
            >
              <option value="">Select provider...</option>
              {providerOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
            {providerCapabilitiesLoading ? (
              <small className="mc-field-help">Loading provider catalog...</small>
            ) : null}
            {providerCapabilitiesError ? (
              <small className="mc-form-error">
                Provider catalog is currently unavailable. Please try again.
              </small>
            ) : null}
          </label>

          <label className="mc-modal-field">
            <span>Model</span>
            <select
              value={form.model_id}
              onChange={(e) => updateField("model_id", e.target.value)}
            >
              <option value="">Select model...</option>
              {modelOptions.map((modelId) => (
                <option key={modelId} value={modelId}>
                  {modelId}
                </option>
              ))}
            </select>
            {modelLoadingProvider === form.model_provider.trim().toLowerCase() ? (
              <small className="mc-field-help">Loading model catalog...</small>
            ) : null}
            {modelFetchError ? (
              <small className="mc-form-error">
                Model catalog unavailable for this provider.
              </small>
            ) : null}
            {(showAdvanced || Boolean(modelFetchError)) ? (
              <input
                value={form.model_id}
                onChange={(e) => updateField("model_id", e.target.value)}
                placeholder="Manual model ID fallback"
              />
            ) : null}
          </label>

          <button
            type="button"
            className={clsx("mc-btn mc-btn-sm", showAdvanced && "mc-edit-mode-active")}
            onClick={() => setShowAdvanced((value) => !value)}
          >
            {showAdvanced ? "Hide Advanced" : "Advanced..."}
          </button>

          {showAdvanced ? (
            <div className="mc-field-grid">
              <label className="mc-modal-field">
                <span>Tool Profile</span>
                <select
                  value={form.tool_profile}
                  onChange={(e) => updateField("tool_profile", e.target.value)}
                >
                  {TOOL_PROFILES.map((toolProfile) => (
                    <option key={toolProfile.value} value={toolProfile.value}>
                      {toolProfile.label}
                    </option>
                  ))}
                </select>
              </label>

              <label className="mc-modal-field">
                <span>Workspace</span>
                <select
                  value={
                    knownWorkspaces.includes(form.workspace_root)
                      ? form.workspace_root
                      : "__custom__"
                  }
                  onChange={(e) => {
                    if (e.target.value !== "__custom__") {
                      updateField("workspace_root", e.target.value);
                    }
                  }}
                >
                  <option value="">None</option>
                  {knownWorkspaces.map((workspace) => (
                    <option key={workspace} value={workspace}>
                      {workspace}
                    </option>
                  ))}
                  {!knownWorkspaces.includes(form.workspace_root) && form.workspace_root ? (
                    <option value="__custom__">Custom...</option>
                  ) : null}
                </select>
                {(!knownWorkspaces.includes(form.workspace_root) && form.workspace_root) ||
                knownWorkspaces.length === 0 ? (
                  <input
                    value={form.workspace_root}
                    onChange={(e) => updateField("workspace_root", e.target.value)}
                    placeholder="~/projects/workspace"
                  />
                ) : null}
              </label>
            </div>
          ) : null}

          {strategyEnabled && form.reports_to_agent_id ? (
            <div className="mc-strategy-suggestion-row">
              <span>Hierarchy owner suggestions</span>
              <div className="mc-strategy-chip-row">
                {(strategyController.org.subtreeIdsByAgentId.get(form.reports_to_agent_id) ?? [])
                  .slice(0, 6)
                  .map((agentId) => (
                    <span key={agentId} className="mc-strategy-filter-chip">
                      {strategyController.org.agentsById.get(agentId)?.name ?? agentId}
                    </span>
                  ))}
              </div>
            </div>
          ) : null}
        </div>
      </Modal>

      <Modal
        open={presetModalMode !== null}
        onClose={() => setPresetModalMode(null)}
        title={presetModalMode === "create" ? "Save Bootstrap Preset" : "Edit Bootstrap Preset"}
        width="680px"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setPresetModalMode(null)}>
              Cancel
            </button>
            <button
              type="button"
              disabled={presetSaving}
              onClick={() => void savePreset()}
            >
              {presetSaving ? "Saving..." : "Save Preset"}
            </button>
          </>
        }
      >
        <div className="mc-modal-form">
          {presetError ? <div className="mc-form-error">{presetError}</div> : null}
          <div className="mc-field-grid">
            <label className="mc-modal-field">
              <span>Preset Key</span>
              <input
                value={presetForm.preset_key}
                disabled={presetModalMode === "edit"}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    preset_key: event.target.value,
                  }))
                }
              />
            </label>
            <label className="mc-modal-field">
              <span>Display Name</span>
              <input
                value={presetForm.display_name}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    display_name: event.target.value,
                  }))
                }
              />
            </label>
          </div>
          <label className="mc-modal-field">
            <span>Description</span>
            <textarea
              rows={3}
              value={presetForm.description}
              onChange={(event) =>
                setPresetForm((current) => ({
                  ...current,
                  description: event.target.value,
                }))
              }
            />
          </label>
          <div className="mc-field-grid">
            <label className="mc-modal-field">
              <span>Role Label</span>
              <input
                value={presetForm.role_label}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    role_label: event.target.value,
                  }))
                }
              />
            </label>
            <label className="mc-modal-field">
              <span>Provider Path</span>
              <select
                value={presetForm.provider_path}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    provider_path: event.target.value,
                  }))
                }
              >
                <option value="local">local</option>
                <option value="openai">openai</option>
                <option value="anthropic">anthropic</option>
              </select>
            </label>
          </div>
          <div className="mc-field-grid">
            <label className="mc-modal-field">
              <span>Default Model Provider</span>
              <input
                value={presetForm.default_model_provider}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    default_model_provider: event.target.value,
                  }))
                }
              />
            </label>
            <label className="mc-modal-field">
              <span>Default Model ID</span>
              <input
                value={presetForm.default_model_id}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    default_model_id: event.target.value,
                  }))
                }
              />
            </label>
          </div>
          <div className="mc-field-grid">
            <label className="mc-modal-field">
              <span>Default Tool Profile</span>
              <input
                value={presetForm.default_tool_profile}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    default_tool_profile: event.target.value,
                  }))
                }
              />
            </label>
            <label className="mc-modal-field">
              <span>Default Workspace Root</span>
              <input
                value={presetForm.default_workspace_root}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    default_workspace_root: event.target.value,
                  }))
                }
              />
            </label>
          </div>
          <div className="mc-field-grid">
            <label className="mc-modal-field">
              <span>Default Manager</span>
              <select
                value={presetForm.default_reports_to_agent_id}
                onChange={(event) =>
                  setPresetForm((current) => ({
                    ...current,
                    default_reports_to_agent_id: event.target.value,
                  }))
                }
              >
                <option value="">No manager</option>
                {agents.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <label className="mc-modal-field">
            <span>Setup Notes</span>
            <textarea
              rows={4}
              value={presetForm.setup_notes}
              onChange={(event) =>
                setPresetForm((current) => ({
                  ...current,
                  setup_notes: event.target.value,
                }))
              }
            />
          </label>
        </div>
      </Modal>

      <Modal
        open={roleCardAgent !== null}
        onClose={() => setRoleCardAgentId(null)}
        title={roleCardAgent ? `${roleCardAgent.name} — Role Card` : "Role Card"}
        subtitle={roleCardAgent?.agent_id}
        width="560px"
      >
        {roleCardAgent ? (
          <div className="mc-role-card">
            <div className="mc-role-card-avatar">
              {roleCardAgent.name.charAt(0).toUpperCase()}
            </div>
            <div className="mc-role-card-section">
              <h4>Configuration</h4>
              <dl className="mc-role-card-dl">
                <dt>Role</dt>
                <dd>{roleCardAgent.role_label ?? "not set"}</dd>
                <dt>Provider</dt>
                <dd>{roleCardAgent.model_provider}</dd>
                <dt>Model</dt>
                <dd>{roleCardAgent.model_id}</dd>
                <dt>Tool Profile</dt>
                <dd>{roleCardAgent.tool_profile ?? "standard"}</dd>
                <dt>Workspace</dt>
                <dd>{roleCardAgent.workspace_root ?? "not set"}</dd>
                <dt>Manager Chain</dt>
                <dd>
                  {managerChainLabel(roleCardAgent.agent_id, strategyController.org) ??
                    "root agent"}
                </dd>
              </dl>
            </div>
            {roleCardReports.length > 0 ? (
              <div className="mc-role-card-section">
                <h4>Direct Reports</h4>
                <div className="mc-strategy-chip-row">
                  {roleCardReports.map((agent) => (
                    <span key={agent.agent_id} className="mc-strategy-filter-chip">
                      {agent.name}
                    </span>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        ) : null}
      </Modal>

      <input
        ref={importFileRef}
        type="file"
        accept="application/json"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (!file) {
            return;
          }
          void handleImportPresetFile(file)
            .catch((error) => {
              setError(`Preset import failed: ${String(error)}`);
            })
            .finally(() => {
              event.target.value = "";
            });
        }}
      />
    </div>
  );
}
