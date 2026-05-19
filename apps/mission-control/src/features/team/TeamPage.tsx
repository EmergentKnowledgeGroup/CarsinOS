import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Bot,
  Download,
  GitBranch,
  Link2,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  Upload,
  Users,
} from "lucide-react";
import clsx from "clsx";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Modal } from "../../ui/Modal";
import { Pagination } from "../../ui/Pagination";
import { Surface } from "../../ui/Surface";
import { usePagination } from "../../ui/usePagination";
import {
  createAgent,
  GatewayApiError,
  getRuntimeConfig,
  listProviderCapabilities,
  listProviderModels,
  removeAgent,
  updateRuntimeConfig,
  updateAgent,
} from "../../lib/api";
import { providerLabel } from "../../lib/providerCatalog";
import type {
  Agent,
  BootstrapPresetResponse,
  ProviderCapabilityResponse,
  RuntimeAssistantAssignmentConfigResponse,
  RuntimeConnectionSettings,
  RuntimeHumanIdentityConfigResponse,
  RuntimeLaneMemoryPolicyConfigResponse,
  RuntimePlatformIdentityLinkConfigResponse,
  RuntimeRoutingConfigResponse,
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

const ROUTING_PROVIDER_OPTIONS = [
  { value: "discord", label: "Discord" },
  { value: "telegram", label: "Telegram" },
];

const DM_UNMAPPED_POLICY_OPTIONS = [
  { value: "approval_required", label: "Ask for approval" },
  { value: "block", label: "Block" },
];

const SHARED_UNMAPPED_POLICY_OPTIONS = [
  { value: "block", label: "Block" },
];

interface RoutingNoticeState {
  tone: "info" | "error";
  message: string;
}

interface TeamHumanRoutingCard {
  index: number;
  human: RuntimeHumanIdentityConfigResponse;
  assignment: RuntimeAssistantAssignmentConfigResponse | null;
  memoryPolicy: RuntimeLaneMemoryPolicyConfigResponse | null;
  links: Array<{
    index: number;
    link: RuntimePlatformIdentityLinkConfigResponse;
  }>;
}

function cloneRoutingConfig(
  routing: RuntimeRoutingConfigResponse
): RuntimeRoutingConfigResponse {
  return {
    ...routing,
    human_identities: routing.human_identities.map((item) => ({ ...item })),
    platform_identity_links: routing.platform_identity_links.map((item) => ({
      ...item,
    })),
    assistant_assignments: routing.assistant_assignments.map((item) => ({
      ...item,
    })),
    lane_memory_policies: routing.lane_memory_policies.map((item) => ({
      ...item,
      local_memory_sources: [...item.local_memory_sources],
    })),
  };
}

function slugifyHumanIdentity(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function createHumanIdentityId(
  displayName: string,
  existingIds: Iterable<string>
): string {
  const existing = new Set(
    Array.from(existingIds, (value) => value.trim()).filter(Boolean)
  );
  const base = slugifyHumanIdentity(displayName) || "person";
  if (!existing.has(base)) {
    return base;
  }
  let index = 2;
  while (existing.has(`${base}-${index}`)) {
    index += 1;
  }
  return `${base}-${index}`;
}

function providerIdentityLabel(provider: string): string {
  switch (provider.trim().toLowerCase()) {
    case "discord":
      return "Discord";
    case "telegram":
      return "Telegram";
    default:
      return provider || "Link";
  }
}

function laneMemoryModeLabel(mode: string): string {
  switch (mode) {
    case "disabled":
      return "Memory off";
    case "local_only":
      return "Local only";
    case "mno_only":
      return "MNO only";
    case "mno_with_local_sources":
      return "MNO + local";
    case "inherit_runtime":
    default:
      return "Runtime default";
  }
}

function friendlyRemoveAgentError(error: unknown): string {
  const raw =
    error instanceof GatewayApiError
      ? `${error.responseBody ?? ""} ${error.message}`.trim()
      : String(error);
  const normalized = raw.toLowerCase();
  if (normalized.includes("scheduled job") || normalized.includes("active job")) {
    return "This agent still owns scheduled jobs. Move or delete those jobs first so CarsinOS does not leave automation pointing at nowhere.";
  }
  if (normalized.includes("session") || normalized.includes("chat history")) {
    return "CarsinOS tried to archive this agent and keep its old chats, but the gateway still reported a history conflict. Refresh Team and try once more.";
  }
  if (normalized.includes("not found")) {
    return "That agent is already gone from Team. Refreshing should clear the stale card.";
  }
  return `Removing the agent failed. ${raw}`;
}

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
  const [routingPage, setRoutingPage] = useState(1);
  const [activeSection, setActiveSection] = useState<"agents" | "routing" | "presets" | "org">("agents");
  const [routingView, setRoutingView] = useState<"people" | "overview">("people");
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
  const [presetModalMode, setPresetModalMode] = useState<"create" | "edit" | null>(null);
  const [presetForm, setPresetForm] = useState<PresetFormState>(EMPTY_PRESET_FORM);
  const [presetError, setPresetError] = useState<string | null>(null);
  const [presetSaving, setPresetSaving] = useState(false);
  const [routingConfig, setRoutingConfig] = useState<RuntimeRoutingConfigResponse | null>(null);
  const [routingDraft, setRoutingDraft] = useState<RuntimeRoutingConfigResponse | null>(null);
  const [routingLoading, setRoutingLoading] = useState(false);
  const [routingSaving, setRoutingSaving] = useState(false);
  const [routingError, setRoutingError] = useState<string | null>(null);
  const [routingNotice, setRoutingNotice] = useState<RoutingNoticeState | null>(null);
  const [deletingAgentId, setDeletingAgentId] = useState<string | null>(null);
  const importFileRef = useRef<HTMLInputElement | null>(null);

  const strategyEnabled = strategyController.enabled;
  const { totalPages, getPage } = usePagination(agents, PAGE_SIZE);
  const visibleAgents = getPage(page);
  const roleCardAgent = roleCardAgentId ? agents.find((a) => a.agent_id === roleCardAgentId) : null;
  const agentsById = useMemo(
    () => new Map(agents.map((agent) => [agent.agent_id, agent] as const)),
    [agents]
  );

  const loadRoutingConfig = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (!runtimeSettings.gateway_url.trim()) {
        setRoutingConfig(null);
        setRoutingDraft(null);
        setRoutingError(null);
        setRoutingNotice(null);
        setRoutingLoading(false);
        return;
      }

      setRoutingLoading(true);
      try {
        const response = await getRuntimeConfig(runtimeSettings);
        const nextRouting = cloneRoutingConfig(response.config.routing);
        setRoutingConfig(nextRouting);
        setRoutingDraft(cloneRoutingConfig(nextRouting));
        setRoutingError(null);
      } catch (loadError: unknown) {
        setRoutingConfig(null);
        setRoutingDraft(null);
        setRoutingError(`People and routing could not load. (${String(loadError)})`);
      } finally {
        setRoutingLoading(false);
      }
    },
    [settings]
  );

  useEffect(() => {
    void loadRoutingConfig(settings);
  }, [loadRoutingConfig, settings]);

  const patchRoutingDraft = useCallback(
    (updater: (draft: RuntimeRoutingConfigResponse) => void) => {
      setRoutingDraft((current) => {
        if (!current) {
          return current;
        }
        const next = cloneRoutingConfig(current);
        updater(next);
        return next;
      });
    },
    []
  );

  const addHumanIdentity = useCallback(() => {
    patchRoutingDraft((next) => {
      const nextId = createHumanIdentityId(
        `person ${next.human_identities.length + 1}`,
        next.human_identities.map((item) => item.human_identity_id)
      );
      next.human_identities.push({
        human_identity_id: nextId,
        display_name: `Person ${next.human_identities.length + 1}`,
        enabled: true,
      });
    });
    setRoutingNotice(null);
    setRoutingError(null);
  }, [patchRoutingDraft]);

  const updateHumanDisplayName = useCallback(
    (humanIdentityId: string, displayName: string) => {
      patchRoutingDraft((next) => {
        const human = next.human_identities.find(
          (item) => item.human_identity_id === humanIdentityId
        );
        if (human) {
          human.display_name = displayName;
        }
      });
    },
    [patchRoutingDraft]
  );

  const updateHumanEnabled = useCallback(
    (humanIdentityId: string, enabled: boolean) => {
      patchRoutingDraft((next) => {
        const human = next.human_identities.find(
          (item) => item.human_identity_id === humanIdentityId
        );
        if (human) {
          human.enabled = enabled;
        }
      });
    },
    [patchRoutingDraft]
  );

  const removeHumanIdentity = useCallback(
    (humanIdentityId: string) => {
      if (routingDraft?.local_operator_human_identity_id === humanIdentityId) {
        setRoutingNotice({
          tone: "error",
          message:
            "You cannot remove the local app operator while it still owns this desktop lane. Pick a different local operator first, then remove this person if you still want to.",
        });
        return;
      }
      patchRoutingDraft((next) => {
        next.human_identities = next.human_identities.filter(
          (item) => item.human_identity_id !== humanIdentityId
        );
        next.platform_identity_links = next.platform_identity_links.filter(
          (item) => item.human_identity_id !== humanIdentityId
        );
        next.assistant_assignments = next.assistant_assignments.filter(
          (item) => item.human_identity_id !== humanIdentityId
        );
        next.lane_memory_policies = next.lane_memory_policies.filter(
          (item) => item.human_identity_id !== humanIdentityId
        );
      });
      setRoutingNotice(null);
    },
    [patchRoutingDraft, routingDraft]
  );

  const setHumanAssignment = useCallback(
    (humanIdentityId: string, assistantAgentId: string) => {
      patchRoutingDraft((next) => {
        next.enabled = true;
        if (!next.local_operator_human_identity_id?.trim()) {
          next.local_operator_human_identity_id = humanIdentityId;
        }
        next.assistant_assignments = next.assistant_assignments.filter(
          (item) => item.human_identity_id !== humanIdentityId
        );
        const normalizedAssistantAgentId = assistantAgentId.trim();
        if (!normalizedAssistantAgentId) {
          return;
        }
        next.assistant_assignments.push({
          human_identity_id: humanIdentityId,
          assistant_agent_id: normalizedAssistantAgentId,
          enabled: true,
        });
      });
    },
    [patchRoutingDraft]
  );

  const addPlatformIdentityLink = useCallback(
    (humanIdentityId: string) => {
      patchRoutingDraft((next) => {
        next.platform_identity_links.push({
          provider: "discord",
          platform_user_id: "",
          human_identity_id: humanIdentityId,
          display_name: null,
          enabled: true,
        });
      });
    },
    [patchRoutingDraft]
  );

  const updatePlatformIdentityLink = useCallback(
    (
      index: number,
      patch: Partial<RuntimePlatformIdentityLinkConfigResponse>
    ) => {
      patchRoutingDraft((next) => {
        const existing = next.platform_identity_links[index];
        if (!existing) {
          return;
        }
        next.platform_identity_links[index] = {
          ...existing,
          ...patch,
        };
      });
    },
    [patchRoutingDraft]
  );

  const removePlatformIdentityLink = useCallback(
    (index: number) => {
      patchRoutingDraft((next) => {
        next.platform_identity_links.splice(index, 1);
      });
    },
    [patchRoutingDraft]
  );

  const resetRoutingDraft = useCallback(() => {
    if (!routingConfig) {
      return;
    }
    setRoutingDraft(cloneRoutingConfig(routingConfig));
    setRoutingNotice(null);
    setRoutingError(null);
  }, [routingConfig]);

  const saveRoutingDraft = useCallback(async () => {
    if (!routingDraft) {
      return;
    }
    if (!settings.gateway_url.trim()) {
      setRoutingNotice({
        tone: "error",
        message: "Connect to the gateway before saving people and routing.",
      });
      return;
    }

    const knownAgentIds = new Set(agents.map((agent) => agent.agent_id));
    const normalizedHumans: RuntimeHumanIdentityConfigResponse[] = [];
    const humanIds = new Set<string>();

    for (const human of routingDraft.human_identities) {
      const humanIdentityId = human.human_identity_id.trim();
      const displayName = human.display_name.trim() || humanIdentityId;
      if (!humanIdentityId) {
        setRoutingNotice({
          tone: "error",
          message: "Every person needs a human ID before routing can be saved.",
        });
        return;
      }
      if (humanIds.has(humanIdentityId)) {
        setRoutingNotice({
          tone: "error",
          message: `The human ID "${humanIdentityId}" is duplicated. Give each person one unique ID.`,
        });
        return;
      }
      humanIds.add(humanIdentityId);
      normalizedHumans.push({
        human_identity_id: humanIdentityId,
        display_name: displayName,
        enabled: human.enabled,
      });
    }

    const normalizedLinks: RuntimePlatformIdentityLinkConfigResponse[] = [];
    const seenLinks = new Set<string>();
    for (const link of routingDraft.platform_identity_links) {
      const provider = link.provider.trim().toLowerCase();
      const platformUserId = link.platform_user_id.trim();
      const humanIdentityId = link.human_identity_id.trim();
      const displayName = link.display_name?.trim() || null;
      if (!provider && !platformUserId && !displayName) {
        continue;
      }
      if (!provider || !platformUserId) {
        setRoutingNotice({
          tone: "error",
          message:
            "Every linked account needs both a provider and that person’s platform user ID.",
        });
        return;
      }
      if (!humanIds.has(humanIdentityId)) {
        setRoutingNotice({
          tone: "error",
          message: `A linked account points at missing human "${humanIdentityId}".`,
        });
        return;
      }
      const duplicateKey = `${provider}:${platformUserId}`;
      if (seenLinks.has(duplicateKey)) {
        setRoutingNotice({
          tone: "error",
          message: `The linked account "${duplicateKey}" is duplicated.`,
        });
        return;
      }
      seenLinks.add(duplicateKey);
      normalizedLinks.push({
        provider,
        platform_user_id: platformUserId,
        human_identity_id: humanIdentityId,
        display_name: displayName,
        enabled: link.enabled,
      });
    }

    const normalizedAssignments: RuntimeAssistantAssignmentConfigResponse[] = [];
    const seenAssignmentPairs = new Set<string>();
    const seenEnabledHumans = new Set<string>();
    for (const assignment of routingDraft.assistant_assignments) {
      const humanIdentityId = assignment.human_identity_id.trim();
      const assistantAgentId = assignment.assistant_agent_id.trim();
      if (!humanIdentityId || !assistantAgentId) {
        continue;
      }
      if (!humanIds.has(humanIdentityId)) {
        setRoutingNotice({
          tone: "error",
          message: `An assistant assignment points at missing human "${humanIdentityId}".`,
        });
        return;
      }
      if (!knownAgentIds.has(assistantAgentId)) {
        setRoutingNotice({
          tone: "error",
          message: `Assistant "${assistantAgentId}" does not exist anymore.`,
        });
        return;
      }
      const pairKey = `${humanIdentityId}:${assistantAgentId}`;
      if (seenAssignmentPairs.has(pairKey)) {
        setRoutingNotice({
          tone: "error",
          message: `The assistant route "${pairKey}" is duplicated.`,
        });
        return;
      }
      seenAssignmentPairs.add(pairKey);
      if (assignment.enabled && seenEnabledHumans.has(humanIdentityId)) {
        setRoutingNotice({
          tone: "error",
          message: `Only one enabled assistant can be assigned to "${humanIdentityId}" at a time.`,
        });
        return;
      }
      if (assignment.enabled) {
        seenEnabledHumans.add(humanIdentityId);
      }
      normalizedAssignments.push({
        human_identity_id: humanIdentityId,
        assistant_agent_id: assistantAgentId,
        enabled: assignment.enabled,
      });
    }

    const normalizedLanePolicies = routingDraft.lane_memory_policies
      .map((policy) => ({
        human_identity_id: policy.human_identity_id.trim(),
        assistant_agent_id: policy.assistant_agent_id.trim(),
        memory_mode: policy.memory_mode.trim() || "inherit_runtime",
        lane_id: policy.lane_id?.trim() || null,
        local_memory_sources: policy.local_memory_sources
          .map((item) => item.trim())
          .filter(Boolean),
      }))
      .filter(
        (policy) =>
          humanIds.has(policy.human_identity_id) &&
          knownAgentIds.has(policy.assistant_agent_id)
      );
    const localOperatorHumanIdentityId =
      routingDraft.local_operator_human_identity_id?.trim() || null;
    if (localOperatorHumanIdentityId && !humanIds.has(localOperatorHumanIdentityId)) {
      setRoutingNotice({
        tone: "error",
        message: `The local app operator points at missing human "${localOperatorHumanIdentityId}".`,
      });
      return;
    }
    if (
      localOperatorHumanIdentityId &&
      !normalizedHumans.some(
        (human) =>
          human.enabled && human.human_identity_id === localOperatorHumanIdentityId
      )
    ) {
      setRoutingNotice({
        tone: "error",
        message: `The local app operator must point at an enabled person. Re-enable "${localOperatorHumanIdentityId}" or choose another local operator first.`,
      });
      return;
    }

    const nextRouting: RuntimeRoutingConfigResponse = {
      enabled:
        routingDraft.enabled ||
        normalizedAssignments.some((assignment) => assignment.enabled),
      use_channel_defaults_as_fallback: false,
      local_operator_human_identity_id: localOperatorHumanIdentityId,
      dm_unmapped_policy:
        routingDraft.dm_unmapped_policy.trim() === "block"
          ? "block"
          : "approval_required",
      shared_unmapped_policy: "block",
      human_identities: normalizedHumans,
      platform_identity_links: normalizedLinks,
      assistant_assignments: normalizedAssignments,
      lane_memory_policies: normalizedLanePolicies,
    };

    setRoutingSaving(true);
    try {
      const response = await updateRuntimeConfig(settings, {
        routing: nextRouting,
      });
      const savedRouting = cloneRoutingConfig(response.config.routing);
      setRoutingConfig(savedRouting);
      setRoutingDraft(cloneRoutingConfig(savedRouting));
      setRoutingNotice({
        tone: "info",
        message: "People and routing saved.",
      });
      setRoutingError(null);
    } catch (saveError: unknown) {
      setRoutingNotice({
        tone: "error",
        message: `Saving people and routing failed: ${String(saveError)}`,
      });
    } finally {
      setRoutingSaving(false);
    }
  }, [agents, routingDraft, settings]);

  const routingDirty = useMemo(() => {
    if (!routingConfig || !routingDraft) {
      return false;
    }
    return JSON.stringify(routingConfig) !== JSON.stringify(routingDraft);
  }, [routingConfig, routingDraft]);

  const humanRoutingCards = useMemo<TeamHumanRoutingCard[]>(() => {
    if (!routingDraft) {
      return [];
    }
    return routingDraft.human_identities.map((human, index) => {
      const assignment =
        routingDraft.assistant_assignments.find(
          (item) => item.human_identity_id === human.human_identity_id && item.enabled
        ) ??
        routingDraft.assistant_assignments.find(
          (item) => item.human_identity_id === human.human_identity_id
        ) ??
        null;

      return {
        index,
        human,
        assignment,
        memoryPolicy:
          routingDraft.lane_memory_policies.find(
            (item) =>
              item.human_identity_id === human.human_identity_id &&
              item.assistant_agent_id === assignment?.assistant_agent_id
          ) ?? null,
        links: routingDraft.platform_identity_links
          .map((link, index) => ({ link, index }))
          .filter((entry) => entry.link.human_identity_id === human.human_identity_id),
      };
    });
  }, [routingDraft]);
  const routingPagination = usePagination(humanRoutingCards, 1);
  const visibleHumanRoutingCards = routingPagination.getPage(routingPage);

  const routingSummary = useMemo(
    () => ({
      humans: routingDraft?.human_identities.filter((item) => item.enabled).length ?? 0,
      linkedAccounts:
        routingDraft?.platform_identity_links.filter((item) => item.enabled).length ?? 0,
      assignedHumans:
        humanRoutingCards.filter(
          (item) => item.assignment?.enabled && item.assignment.assistant_agent_id.trim()
        ).length ?? 0,
      waitingForAssignment:
        humanRoutingCards.filter(
          (item) =>
            item.human.enabled &&
            !(item.assignment?.enabled && item.assignment.assistant_agent_id.trim())
        ).length ?? 0,
      localOperator:
        routingDraft?.local_operator_human_identity_id?.trim() || "Not selected",
    }),
    [humanRoutingCards, routingDraft]
  );

  const routedHumansByAgentId = useMemo(() => {
    const grouped = new Map<string, TeamHumanRoutingCard[]>();
    for (const card of humanRoutingCards) {
      if (!card.human.enabled || !card.assignment?.enabled) {
        continue;
      }
      const assistantAgentId = card.assignment.assistant_agent_id.trim();
      if (!assistantAgentId) {
        continue;
      }
      const next = grouped.get(assistantAgentId) ?? [];
      next.push(card);
      grouped.set(assistantAgentId, next);
    }
    return grouped;
  }, [humanRoutingCards]);

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

  const handleRemoveAgent = useCallback(
    async (agent: Agent) => {
      if (!settings.gateway_url.trim()) {
        setRoutingNotice({
          tone: "error",
          message: "Connect to the gateway before removing an agent.",
        });
        return;
      }
      const confirmed = window.confirm(
        `Remove ${agent.name} from Team?\n\nOld chat history is kept automatically in the archive. Active scheduled jobs still need to be moved or deleted first.`
      );
      if (!confirmed) {
        return;
      }

      setDeletingAgentId(agent.agent_id);
      setRoutingNotice(null);
      try {
        if (routingConfig) {
          const nextRouting = cloneRoutingConfig(routingConfig);
          nextRouting.assistant_assignments = nextRouting.assistant_assignments.filter(
            (assignment) => assignment.assistant_agent_id !== agent.agent_id
          );
          nextRouting.lane_memory_policies = nextRouting.lane_memory_policies.filter(
            (policy) => policy.assistant_agent_id !== agent.agent_id
          );
          const response = await updateRuntimeConfig(settings, {
            routing: nextRouting,
          });
          const savedRouting = cloneRoutingConfig(response.config.routing);
          setRoutingConfig(savedRouting);
          setRoutingDraft(cloneRoutingConfig(savedRouting));
        }

        await removeAgent(settings, agent.agent_id);
        setRoutingNotice({
          tone: "info",
          message: `${agent.name} was removed from Team. Old chats stay archived automatically.`,
        });
        await refreshAll();
        await loadRoutingConfig(settings);
      } catch (removeError: unknown) {
        setRoutingNotice({
          tone: "error",
          message: friendlyRemoveAgentError(removeError),
        });
      } finally {
        setDeletingAgentId(null);
      }
    },
    [loadRoutingConfig, refreshAll, routingConfig, settings]
  );

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
  const roleCardRoutedHumans = roleCardAgent
    ? routedHumansByAgentId.get(roleCardAgent.agent_id) ?? []
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
                onClick={() => setActiveSection("org")}
              >
                <GitBranch size={14} />
                Org View
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

      <div className="mc-page-section-tabs" aria-label="Team sections">
        <button
          type="button"
          className={`mc-page-section-btn${activeSection === "agents" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("agents")}
        >
          Agents
        </button>
        <button
          type="button"
          className={`mc-page-section-btn${activeSection === "routing" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("routing")}
        >
          People & Routing
        </button>
        {strategyEnabled ? (
          <>
            <button
              type="button"
              className={`mc-page-section-btn${activeSection === "presets" ? " mc-page-section-btn-active" : ""}`}
              onClick={() => setActiveSection("presets")}
            >
              Presets
            </button>
            <button
              type="button"
              className={`mc-page-section-btn${activeSection === "org" ? " mc-page-section-btn-active" : ""}`}
              onClick={() => setActiveSection("org")}
            >
              Org
            </button>
          </>
        ) : null}
      </div>

      {strategyEnabled && activeSection === "presets" ? (
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

      {strategyEnabled && activeSection === "org" ? (
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
              <EmptyState message="No org structure yet. Assign managers to agents to see reporting chains." />
            ) : null}
          </div>
        </Surface>
      ) : null}

      {activeSection === "routing" ? (
      <Surface
        className="mc-team-routing-surface"
        title="People And Routing"
        subtitle="Decide who is talking, where they come from, and which assistant owns their main lane across Discord and Telegram."
        headerRight={
          <div className="mc-strategy-inline-actions">
            <button
              type="button"
              className="ghost"
              onClick={() => void loadRoutingConfig(settings)}
              disabled={routingLoading}
            >
              <RefreshCw size={14} />
              Refresh
            </button>
            <button type="button" className="ghost" onClick={addHumanIdentity}>
              <Plus size={14} />
              Add Person
            </button>
            <button
              type="button"
              className="ghost"
              onClick={resetRoutingDraft}
              disabled={!routingDirty || routingSaving}
            >
              Reset
            </button>
            <button
              type="button"
              className={clsx("mc-btn mc-btn-accent", routingSaving && "mc-btn-loading")}
              onClick={() => void saveRoutingDraft()}
              disabled={!routingDraft || !routingDirty || routingSaving}
            >
              <Save size={14} />
              Save Routing
            </button>
          </div>
        }
      >
        {!settings.gateway_url.trim() ? (
          <EmptyState message="Connect Mission Control to the gateway before you manage people and routing." />
        ) : routingLoading && !routingDraft ? (
          <EmptyState message="Loading people and routing..." />
        ) : (
          <>
            {routingError ? <div className="mc-notice mc-notice-error">{routingError}</div> : null}
            {routingNotice ? (
              <div
                className={clsx(
                  "mc-notice",
                  routingNotice.tone === "error" ? "mc-notice-error" : "mc-notice-info"
                )}
              >
                {routingNotice.message}
              </div>
            ) : null}

            {routingDraft ? (
              <>
                <div className="mc-page-section-tabs" aria-label="Routing views">
                  <button
                    type="button"
                    className={`mc-page-section-btn${routingView === "people" ? " mc-page-section-btn-active" : ""}`}
                    onClick={() => setRoutingView("people")}
                  >
                    People
                  </button>
                  <button
                    type="button"
                    className={`mc-page-section-btn${routingView === "overview" ? " mc-page-section-btn-active" : ""}`}
                    onClick={() => setRoutingView("overview")}
                  >
                    Routing Setup
                  </button>
                </div>

                {routingView === "overview" ? (
                  <div className="mc-page-section-stack">
                    <div className="mc-team-routing-summary">
                      <div className="mc-team-routing-summary-card">
                        <span className="mc-team-routing-kicker">
                          <Users size={14} />
                          People
                        </span>
                        <strong>{routingSummary.humans}</strong>
                        <p>Humans currently active in routing.</p>
                      </div>
                      <div className="mc-team-routing-summary-card">
                        <span className="mc-team-routing-kicker">
                          <Link2 size={14} />
                          Linked Accounts
                        </span>
                        <strong>{routingSummary.linkedAccounts}</strong>
                        <p>Discord or Telegram identities tied to those people.</p>
                      </div>
                      <div className="mc-team-routing-summary-card">
                        <span className="mc-team-routing-kicker">
                          <Bot size={14} />
                          Assigned Assistants
                        </span>
                        <strong>{routingSummary.assignedHumans}</strong>
                        <p>People already routed to one real assistant.</p>
                      </div>
                      <div className="mc-team-routing-summary-card">
                        <span className="mc-team-routing-kicker">
                          <GitBranch size={14} />
                          Local Operator
                        </span>
                        <strong>{routingSummary.localOperator}</strong>
                        <p>
                          {routingSummary.waitingForAssignment} active people still need an
                          assistant route.
                        </p>
                      </div>
                    </div>

                    <div className="mc-team-routing-controls">
                      <label className="mc-modal-field">
                        <span>Local app operator</span>
                        <select
                          value={routingDraft.local_operator_human_identity_id ?? ""}
                          onChange={(event) =>
                            patchRoutingDraft((next) => {
                              next.local_operator_human_identity_id =
                                event.target.value || null;
                            })
                          }
                        >
                          <option value="">Choose person...</option>
                          {routingDraft.human_identities
                            .filter((human) => human.enabled)
                            .map((human) => (
                              <option
                                key={human.human_identity_id}
                                value={human.human_identity_id}
                              >
                                {human.display_name || human.human_identity_id}
                              </option>
                            ))}
                        </select>
                        <small>
                          Assistant chat on this machine uses this person record so your desktop,
                          Discord, and Telegram conversations can stay in one shared lane once
                          those accounts are linked.
                        </small>
                      </label>
                      <label className="mc-modal-field">
                        <span>Unknown DMs</span>
                        <select
                          value={routingDraft.dm_unmapped_policy}
                          onChange={(event) =>
                            patchRoutingDraft((next) => {
                              next.dm_unmapped_policy = event.target.value;
                            })
                          }
                        >
                          {DM_UNMAPPED_POLICY_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                        <small>
                          Best default here is ask first. Unknown DMs should not silently land
                          in an assistant lane.
                        </small>
                      </label>
                      <label className="mc-modal-field">
                        <span>Unknown shared-space messages</span>
                        <select
                          value={routingDraft.shared_unmapped_policy}
                          onChange={(event) =>
                            patchRoutingDraft((next) => {
                              next.shared_unmapped_policy = event.target.value;
                            })
                          }
                        >
                          {SHARED_UNMAPPED_POLICY_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                        <small>
                          Best default here is block. Shared spaces should not guess which
                          assistant a stranger belongs to.
                        </small>
                      </label>
                    </div>
                  </div>
                ) : null}

                {routingView === "people" ? (
                  <div className="mc-page-section-stack">
                    <div className="mc-team-routing-grid">
                      {humanRoutingCards.length === 0 ? (
                        <EmptyState message="No routed people yet. Add one person, link their account, then choose their assistant." />
                      ) : (
                        visibleHumanRoutingCards.map((card) => {
                      const assignedAgentName = card.assignment?.assistant_agent_id
                        ? agentsById.get(card.assignment.assistant_agent_id)?.name ??
                          card.assignment.assistant_agent_id
                        : null;
                      return (
                        <article
                          key={`human-route-${card.index}`}
                          className={clsx(
                            "mc-team-routing-card",
                            !card.human.enabled && "is-paused"
                          )}
                        >
                          <div className="mc-team-routing-card-head">
                            <div>
                              <strong>
                                {card.human.display_name || card.human.human_identity_id}
                              </strong>
                              <span>{card.human.human_identity_id}</span>
                            </div>
                            <div className="mc-team-card-tags">
                              <Chip
                                label={card.human.enabled ? "active" : "paused"}
                                tone={card.human.enabled ? "up" : "checking"}
                              />
                              {routingDraft.local_operator_human_identity_id ===
                              card.human.human_identity_id ? (
                                <Chip label="local operator" tone="up" />
                              ) : null}
                              <Chip
                                label={
                                  assignedAgentName
                                    ? `assistant: ${assignedAgentName}`
                                    : "needs assistant"
                                }
                                tone={assignedAgentName ? "up" : "warning"}
                              />
                              <Chip
                                label={
                                  card.memoryPolicy
                                    ? laneMemoryModeLabel(card.memoryPolicy.memory_mode)
                                    : "runtime default memory"
                                }
                                tone={card.memoryPolicy ? "warning" : "checking"}
                              />
                            </div>
                          </div>

                          <div className="mc-field-grid">
                            <label className="mc-modal-field">
                              <span>Display name</span>
                              <input
                                value={card.human.display_name}
                                onChange={(event) =>
                                  updateHumanDisplayName(
                                    card.human.human_identity_id,
                                    event.target.value
                                  )
                                }
                                placeholder="Alex"
                              />
                            </label>
                            <label className="mc-modal-field">
                              <span>Human ID</span>
                              <input
                                value={card.human.human_identity_id}
                                readOnly
                              />
                              <small className="mc-field-help">
                                Lane IDs are locked after creation so history, memory, and channel
                                routing do not silently fork.
                              </small>
                            </label>
                          </div>

                          <div className="mc-field-grid">
                            <label className="mc-modal-field">
                              <span>Assistant</span>
                              <select
                                value={card.assignment?.assistant_agent_id ?? ""}
                                onChange={(event) =>
                                  setHumanAssignment(
                                    card.human.human_identity_id,
                                    event.target.value
                                  )
                                }
                              >
                                <option value="">Choose assistant...</option>
                                {agents.map((agent) => (
                                  <option key={agent.agent_id} value={agent.agent_id}>
                                    {agent.name}
                                  </option>
                                ))}
                              </select>
                              <small className="mc-field-help">
                                This is the assistant this person talks to no matter which
                                linked channel they use.
                              </small>
                            </label>
                            <label className="mc-team-routing-toggle mc-team-routing-toggle-inline">
                              <input
                                type="checkbox"
                                checked={card.human.enabled}
                                onChange={(event) =>
                                  updateHumanEnabled(
                                    card.human.human_identity_id,
                                    event.target.checked
                                  )
                                }
                              />
                              <span>Person is active</span>
                              <small>Turn this off to pause routing without deleting the record.</small>
                            </label>
                          </div>

                          <div className="mc-team-routing-links">
                            <div className="mc-team-routing-links-head">
                              <div>
                                <strong>Linked accounts</strong>
                                <p>
                                  Link this person’s Discord or Telegram identity so carsinOS
                                  can resume the same assistant lane everywhere.
                                </p>
                              </div>
                              <button
                                type="button"
                                className="ghost"
                                onClick={() =>
                                  addPlatformIdentityLink(card.human.human_identity_id)
                                }
                              >
                                <Plus size={14} />
                                Add Link
                              </button>
                            </div>
                            {card.links.length === 0 ? (
                              <EmptyState message="No linked accounts yet." />
                            ) : (
                              <div className="mc-team-routing-link-list">
                                {card.links.map(({ index, link }) => (
                                  <div key={`${card.human.human_identity_id}:${index}`} className="mc-team-routing-link-row">
                                    <label className="mc-modal-field">
                                      <span>Provider</span>
                                      <select
                                        value={link.provider}
                                        onChange={(event) =>
                                          updatePlatformIdentityLink(index, {
                                            provider: event.target.value,
                                          })
                                        }
                                      >
                                        {ROUTING_PROVIDER_OPTIONS.map((option) => (
                                          <option key={option.value} value={option.value}>
                                            {option.label}
                                          </option>
                                        ))}
                                      </select>
                                    </label>
                                    <label className="mc-modal-field">
                                      <span>{providerIdentityLabel(link.provider)} user ID</span>
                                      <input
                                        value={link.platform_user_id}
                                        onChange={(event) =>
                                          updatePlatformIdentityLink(index, {
                                            platform_user_id: event.target.value,
                                          })
                                        }
                                        placeholder="platform user id"
                                      />
                                    </label>
                                    <label className="mc-modal-field">
                                      <span>Label</span>
                                      <input
                                        value={link.display_name ?? ""}
                                        onChange={(event) =>
                                          updatePlatformIdentityLink(index, {
                                            display_name: event.target.value || null,
                                          })
                                        }
                                        placeholder="optional nickname"
                                      />
                                    </label>
                                    <label className="mc-team-routing-toggle mc-team-routing-toggle-inline">
                                      <input
                                        type="checkbox"
                                        checked={link.enabled}
                                        onChange={(event) =>
                                          updatePlatformIdentityLink(index, {
                                            enabled: event.target.checked,
                                          })
                                        }
                                      />
                                      <span>Link is active</span>
                                      <small>
                                        Turn this off to keep the mapping saved without using it.
                                      </small>
                                    </label>
                                    <button
                                      type="button"
                                      className="ghost danger"
                                      onClick={() => removePlatformIdentityLink(index)}
                                      title="Remove linked account"
                                    >
                                      <Trash2 size={14} />
                                      Remove
                                    </button>
                                  </div>
                                ))}
                              </div>
                            )}
                          </div>

                          <div className="mc-team-routing-card-foot">
                            <p>
                              {card.assignment?.assistant_agent_id ? (
                                <>
                                  Main lane:{" "}
                                  <code>
                                    {card.memoryPolicy?.lane_id ??
                                      `human:${card.human.human_identity_id}:assistant:${card.assignment.assistant_agent_id}`}
                                  </code>
                                </>
                              ) : (
                                "Pick an assistant so this person has somewhere real to land."
                              )}
                            </p>
                            <button
                              type="button"
                              className="ghost danger"
                              onClick={() => removeHumanIdentity(card.human.human_identity_id)}
                            >
                              <Trash2 size={14} />
                              Remove Person
                            </button>
                          </div>
                        </article>
                      );
                        })
                      )}
                    </div>
                    <Pagination
                      currentPage={routingPage}
                      totalPages={routingPagination.totalPages}
                      onPageChange={setRoutingPage}
                    />
                  </div>
                ) : null}
              </>
            ) : null}
          </>
        )}
      </Surface>
      ) : null}

      {activeSection === "agents" ? (
      <div className="mc-team-roster">
        {visibleAgents.length === 0 ? (
          <div className="mc-team-empty">
            <Bot size={40} />
            <p>No agents yet</p>
            <p className="mc-team-empty-sub">
              Agents do the work in CarsinOS. Create your first one to start chatting, running tasks, and scheduling jobs.
            </p>
            <button
              type="button"
              className="mc-team-empty-cta"
              onClick={openCreate}
            >
              <Plus size={16} /> Create Your First Agent
            </button>
          </div>
        ) : (
          visibleAgents.map((agent) => {
            const routedHumans = routedHumansByAgentId.get(agent.agent_id) ?? [];
            return (
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
                  <div className="mc-team-card-meta">
                    Routed people: {routedHumans.length || "none yet"}
                  </div>
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
                    {routedHumans.length > 0 ? (
                      <span className="mc-chip mc-chip-muted">
                        {routedHumans.length} routed
                      </span>
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
                  <button
                    type="button"
                    className={clsx(
                      "mc-btn",
                      "mc-btn-sm",
                      "mc-btn-danger",
                      deletingAgentId === agent.agent_id && "mc-btn-loading"
                    )}
                    onClick={() => void handleRemoveAgent(agent)}
                    disabled={deletingAgentId === agent.agent_id}
                  >
                    <Trash2 size={14} />
                    Remove
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
      ) : null}

      {activeSection === "agents" ? (
        <Pagination currentPage={page} totalPages={totalPages} onPageChange={setPage} />
      ) : null}

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
            {roleCardRoutedHumans.length > 0 ? (
              <div className="mc-role-card-section">
                <h4>Routed People</h4>
                <div className="mc-strategy-chip-row">
                  {roleCardRoutedHumans.map((card) => (
                    <span key={`${card.human.human_identity_id}:${card.index}`} className="mc-strategy-filter-chip">
                      {card.human.display_name || card.human.human_identity_id}
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
