import { useState, useCallback, useEffect, useMemo } from "react";
import { Plus, Pencil, Bot, Copy } from "lucide-react";
import clsx from "clsx";
import { Modal } from "../../ui/Modal";
import { Pagination } from "../../ui/Pagination";
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
  ProviderCapabilityResponse,
  RuntimeConnectionSettings,
} from "../../types";

const PAGE_SIZE = 5;

interface TeamPageProps {
  agents: Agent[];
  activeJobCount: number;
  settings: RuntimeConnectionSettings;
  onRefresh: () => void;
}

interface AgentFormState {
  agent_id: string;
  name: string;
  model_provider: string;
  model_id: string;
  tool_profile: string;
  workspace_root: string;
}

const EMPTY_FORM: AgentFormState = {
  agent_id: "",
  name: "",
  model_provider: "",
  model_id: "",
  tool_profile: "standard",
  workspace_root: "",
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

function nextCloneAgentId(seed: string, agents: Agent[]): string {
  const normalizedSeed = seed.trim().toLowerCase() || "agent";
  const taken = new Set(agents.map((agent) => agent.agent_id.trim().toLowerCase()));
  const candidate = `${normalizedSeed}-copy`;
  if (!taken.has(candidate)) {
    return candidate;
  }
  let index = 2;
  while (taken.has(`${candidate}-${index}`)) {
    index += 1;
  }
  return `${candidate}-${index}`;
}

export function TeamPage({ agents, activeJobCount, settings, onRefresh }: TeamPageProps) {
  const [page, setPage] = useState(1);
  const [modalMode, setModalMode] = useState<"create" | "edit" | null>(null);
  const [form, setForm] = useState<AgentFormState>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);
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

  const { totalPages, getPage } = usePagination(agents, PAGE_SIZE);
  const visibleAgents = getPage(page);

  const knownWorkspaces = useMemo(() => {
    const set = new Set<string>();
    for (const agent of agents) {
      if (agent.workspace_root) set.add(agent.workspace_root);
    }
    return Array.from(set).sort();
  }, [agents]);

  const roleCardAgent = roleCardAgentId ? agents.find((a) => a.agent_id === roleCardAgentId) : null;

  const providerOptions = useMemo(() => {
    const mapped = providerCapabilities
      .filter((item) => item.provider !== "unconfigured")
      .filter((item) => showAdvanced || item.provider !== "openrouter")
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
  }, [form.model_provider, providerCapabilities, providerCapabilitiesError, showAdvanced]);

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
        if (cancelled) {
          return;
        }
        setProviderCapabilities(response.items);
      })
      .catch((err: unknown) => {
        if (cancelled) {
          return;
        }
        console.error("provider capability catalog load failed", err);
        setProviderCapabilitiesError(String(err));
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
          if (cancelled) {
            return;
          }
          setModelErrorsByProvider((prev) => ({ ...prev, [provider]: String(err) }));
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

  const openCreate = useCallback(() => {
    setForm(EMPTY_FORM);
    setError(null);
    setProviderCapabilitiesError(null);
    setModelsByProvider({});
    setModelErrorsByProvider({});
    setModelLoadingProvider(null);
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
    });
    setError(null);
    setProviderCapabilitiesError(null);
    setModelsByProvider({});
    setModelErrorsByProvider({});
    setModelLoadingProvider(null);
    setModalMode("edit");
  }, []);

  const openClone = useCallback(
    (agent: Agent) => {
      const clonedId = nextCloneAgentId(agent.agent_id, agents);
      setForm({
        agent_id: clonedId,
        name: `${agent.name} Copy`,
        model_provider: agent.model_provider,
        model_id: agent.model_id,
        tool_profile: agent.tool_profile ?? "standard",
        workspace_root: agent.workspace_root ?? "",
      });
      setError(null);
      setProviderCapabilitiesError(null);
      setModelsByProvider({});
      setModelErrorsByProvider({});
      setModelLoadingProvider(null);
      setModalMode("create");
    },
    [agents]
  );

  const closeModal = useCallback(() => {
    setModalMode(null);
    setError(null);
  }, []);

  const handleSave = useCallback(async () => {
    if (!form.agent_id.trim() || !form.name.trim()) {
      setError("Agent ID and Name are required.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      if (modalMode === "create") {
        await createAgent(settings, {
          agent_id: form.agent_id.trim(),
          name: form.name.trim(),
          model_provider: form.model_provider || undefined,
          model_id: form.model_id || undefined,
          tool_profile: form.tool_profile || undefined,
          workspace_root: form.workspace_root || undefined,
        });
      } else {
        await updateAgent(settings, form.agent_id, {
          name: form.name.trim(),
          model_provider: form.model_provider || undefined,
          model_id: form.model_id || undefined,
          tool_profile: form.tool_profile || undefined,
          workspace_root: form.workspace_root || undefined,
        });
      }
      closeModal();
      onRefresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [form, modalMode, settings, closeModal, onRefresh]);

  const updateField = useCallback(
    <K extends keyof AgentFormState>(key: K, value: AgentFormState[K]) => {
      setForm((prev) => ({ ...prev, [key]: value }));
    },
    []
  );

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
        <button type="button" className="mc-btn mc-btn-accent" onClick={openCreate}>
          <Plus size={14} />
          New Agent
        </button>
      </div>

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
                <div className="mc-team-card-meta">
                  Model: {agent.model_provider} / {agent.model_id}
                </div>
                {agent.tool_profile ? (
                  <div className="mc-team-card-meta">Tools: {agent.tool_profile}</div>
                ) : null}
                {agent.workspace_root ? (
                  <div className="mc-team-card-meta">Workspace: {agent.workspace_root}</div>
                ) : null}
                <div className="mc-team-card-tags">
                  <span className="mc-chip mc-chip-muted">{agent.model_provider}</span>
                  <span className="mc-chip mc-chip-muted">{agent.tool_profile ?? "standard"}</span>
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
                  className="mc-topbar-icon-btn"
                  onClick={() => openClone(agent)}
                  title="Clone agent as new"
                >
                  <Copy size={14} />
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
              onClick={handleSave}
              disabled={saving}
            >
              {modalMode === "create" ? "Create Agent" : "Save Changes"}
            </button>
          </div>
        }
      >
        <div className="mc-modal-form">
          {error ? <div className="mc-form-error">{error}</div> : null}

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
              {modelOptions.map((m) => (
                <option key={m} value={m}>{m}</option>
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
            onClick={() => setShowAdvanced(!showAdvanced)}
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
                  {TOOL_PROFILES.map((tp) => (
                    <option key={tp.value} value={tp.value}>{tp.label}</option>
                  ))}
                </select>
              </label>

              <label className="mc-modal-field">
                <span>Workspace</span>
                <select
                  value={knownWorkspaces.includes(form.workspace_root) ? form.workspace_root : "__custom__"}
                  onChange={(e) => {
                    if (e.target.value !== "__custom__") {
                      updateField("workspace_root", e.target.value);
                    }
                  }}
                >
                  <option value="">None</option>
                  {knownWorkspaces.map((ws) => (
                    <option key={ws} value={ws}>{ws}</option>
                  ))}
                  {!knownWorkspaces.includes(form.workspace_root) && form.workspace_root ? (
                    <option value="__custom__">Custom...</option>
                  ) : null}
                </select>
                {(!knownWorkspaces.includes(form.workspace_root) && form.workspace_root) || knownWorkspaces.length === 0 ? (
                  <input
                    value={form.workspace_root}
                    onChange={(e) => updateField("workspace_root", e.target.value)}
                    placeholder="~/projects/workspace"
                  />
                ) : null}
              </label>
            </div>
          ) : null}
        </div>
      </Modal>

      {/* ── Role Card modal ── */}
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
                <dt>Provider</dt>
                <dd>{roleCardAgent.model_provider}</dd>
                <dt>Model</dt>
                <dd>{roleCardAgent.model_id}</dd>
                <dt>Tool Profile</dt>
                <dd>{roleCardAgent.tool_profile ?? "standard"}</dd>
                <dt>Workspace</dt>
                <dd>{roleCardAgent.workspace_root ?? "not set"}</dd>
              </dl>
            </div>
          </div>
        ) : null}
      </Modal>
    </div>
  );
}
