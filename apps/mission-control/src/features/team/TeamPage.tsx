import { useState, useCallback, useMemo } from "react";
import { Plus, Pencil, Bot } from "lucide-react";
import clsx from "clsx";
import { Modal } from "../../ui/Modal";
import { Pagination } from "../../ui/Pagination";
import { usePagination } from "../../ui/usePagination";
import { createAgent, updateAgent } from "../../lib/api";
import type { Agent, RuntimeConnectionSettings } from "../../types";

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

const PROVIDER_OPTIONS = [
  { value: "anthropic", label: "Anthropic" },
  { value: "openai", label: "OpenAI" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "ollama", label: "Ollama" },
  { value: "vllm", label: "vLLM" },
  { value: "mock", label: "Mock" },
];

const TOOL_PROFILES = [
  { value: "standard", label: "Standard" },
  { value: "restricted", label: "Restricted" },
  { value: "none", label: "None" },
];

const MODELS_BY_PROVIDER: Record<string, string[]> = {
  anthropic: [
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    "claude-haiku-4-5-20251001",
    "claude-sonnet-4-5-20250514",
    "claude-3-5-haiku-20241022",
  ],
  openai: [
    "gpt-4o",
    "gpt-4o-mini",
    "gpt-4-turbo",
    "o3",
    "o3-mini",
    "o4-mini",
  ],
  openrouter: [
    "anthropic/claude-sonnet-4-6",
    "openai/gpt-4o",
    "google/gemini-2.5-pro",
    "meta-llama/llama-3.3-70b-instruct",
  ],
  ollama: ["llama3.3", "mistral", "codellama", "deepseek-coder-v2"],
  vllm: [],
  mock: ["mock-model"],
};

export function TeamPage({ agents, activeJobCount, settings, onRefresh }: TeamPageProps) {
  const [page, setPage] = useState(1);
  const [modalMode, setModalMode] = useState<"create" | "edit" | null>(null);
  const [form, setForm] = useState<AgentFormState>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);

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

  const openCreate = useCallback(() => {
    setForm(EMPTY_FORM);
    setError(null);
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
    setModalMode("edit");
  }, []);

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
              {PROVIDER_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </label>

          <label className="mc-modal-field">
            <span>Model</span>
            <select
              value={form.model_id}
              onChange={(e) => updateField("model_id", e.target.value)}
            >
              <option value="">Select model...</option>
              {(MODELS_BY_PROVIDER[form.model_provider] ?? []).map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
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
