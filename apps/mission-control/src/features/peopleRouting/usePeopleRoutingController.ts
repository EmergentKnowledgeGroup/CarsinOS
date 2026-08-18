/**
 * The single People & Routing ownership boundary. One mounted instance owns
 * the runtime routing config read/draft/save path through getRuntimeConfig
 * and updateRuntimeConfig; every consumer (Directory / Front Desk edits,
 * Team roster chips, agent-removal cleanup) shares this state instead of
 * mounting a second fetch loop or a second editable truth.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { getRuntimeConfig, updateRuntimeConfig } from "../../lib/api";
import type {
  Agent,
  RuntimeAssistantAssignmentConfigResponse,
  RuntimeConnectionSettings,
  RuntimeHumanIdentityConfigResponse,
  RuntimeLaneMemoryPolicyConfigResponse,
  RuntimePlatformIdentityLinkConfigResponse,
  RuntimeRoutingConfigResponse,
} from "../../types";

export interface RoutingNoticeState {
  tone: "info" | "error";
  message: string;
}

export interface HumanRoutingCard {
  index: number;
  human: RuntimeHumanIdentityConfigResponse;
  assignment: RuntimeAssistantAssignmentConfigResponse | null;
  memoryPolicy: RuntimeLaneMemoryPolicyConfigResponse | null;
  links: Array<{
    index: number;
    link: RuntimePlatformIdentityLinkConfigResponse;
  }>;
}

export function cloneRoutingConfig(
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

export function createHumanIdentityId(
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

interface UsePeopleRoutingControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
}

export function usePeopleRoutingController({
  settings,
  tokenConfigured,
  agents,
}: UsePeopleRoutingControllerOptions) {
  const [routingConfig, setRoutingConfig] =
    useState<RuntimeRoutingConfigResponse | null>(null);
  const [routingDraft, setRoutingDraft] =
    useState<RuntimeRoutingConfigResponse | null>(null);
  const [routingLoading, setRoutingLoading] = useState(false);
  const [routingSaving, setRoutingSaving] = useState(false);
  const [routingError, setRoutingError] = useState<string | null>(null);
  const [routingNotice, setRoutingNotice] = useState<RoutingNoticeState | null>(
    null
  );
  // A newer read/write owns the visible truth. This prevents an older response
  // from replacing a later refresh or a just-saved authoritative config.
  const routingGenerationRef = useRef(0);
  // React state is not synchronous enough to reject two same-tick Save clicks.
  const routingSaveInFlightRef = useRef(false);
  // The editable Front Desk draft needs a synchronous identity as well as React
  // state. A lane-policy mutation may await the server while an operator edits
  // the draft; a late response must never adopt over that newer edit.
  const routingConfigRef = useRef<RuntimeRoutingConfigResponse | null>(null);
  const routingDraftRef = useRef<RuntimeRoutingConfigResponse | null>(null);
  const routingDraftRevisionRef = useRef(0);

  const replaceRoutingConfig = useCallback((next: RuntimeRoutingConfigResponse | null) => {
    routingConfigRef.current = next;
    setRoutingConfig(next);
  }, []);

  const replaceRoutingDraft = useCallback((next: RuntimeRoutingConfigResponse | null) => {
    routingDraftRef.current = next;
    routingDraftRevisionRef.current += 1;
    setRoutingDraft(next);
  }, []);

  const loadRoutingConfig = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (
        routingSaveInFlightRef.current &&
        tokenConfigured &&
        runtimeSettings.gateway_url.trim()
      ) {
        return null;
      }
      const generation = ++routingGenerationRef.current;
      if (!tokenConfigured || !runtimeSettings.gateway_url.trim()) {
        if (generation === routingGenerationRef.current) {
          replaceRoutingConfig(null);
          replaceRoutingDraft(null);
          setRoutingError(null);
          setRoutingNotice(null);
          setRoutingLoading(false);
        }
        return null;
      }
      setRoutingLoading(true);
      try {
        const response = await getRuntimeConfig(runtimeSettings);
        const nextRouting = cloneRoutingConfig(response.config.routing);
        if (generation !== routingGenerationRef.current) {
          return null;
        }
        replaceRoutingConfig(nextRouting);
        replaceRoutingDraft(cloneRoutingConfig(nextRouting));
        setRoutingError(null);
        return nextRouting;
      } catch (loadError: unknown) {
        if (generation !== routingGenerationRef.current) {
          return null;
        }
        replaceRoutingConfig(null);
        replaceRoutingDraft(null);
        setRoutingError(`People and routing could not load. (${String(loadError)})`);
        return null;
      } finally {
        if (generation === routingGenerationRef.current) {
          setRoutingLoading(false);
        }
      }
    },
    [replaceRoutingConfig, replaceRoutingDraft, settings, tokenConfigured]
  );

  useEffect(() => {
    void loadRoutingConfig(settings);
  }, [loadRoutingConfig, settings]);

  const restoreRoutingConfig = useCallback(
    async (routing: RuntimeRoutingConfigResponse) => {
      if (routingSaveInFlightRef.current) {
        throw new Error("People and routing is already being saved.");
      }
      if (!tokenConfigured || !settings.gateway_url.trim()) {
        throw new Error("Connect to the gateway before restoring people and routing.");
      }

      routingSaveInFlightRef.current = true;
      const generation = ++routingGenerationRef.current;
      setRoutingSaving(true);
      try {
        const response = await updateRuntimeConfig(settings, {
          routing: cloneRoutingConfig(routing),
        });
        const savedRouting = cloneRoutingConfig(response.config.routing);
        if (generation === routingGenerationRef.current) {
          replaceRoutingConfig(savedRouting);
          replaceRoutingDraft(cloneRoutingConfig(savedRouting));
          setRoutingError(null);
        }
        return savedRouting;
      } finally {
        routingSaveInFlightRef.current = false;
        setRoutingSaving(false);
      }
    },
    [replaceRoutingConfig, replaceRoutingDraft, settings, tokenConfigured]
  );

  /**
   * Apply one lane-policy mutation through the same serialized authority as
   * Front Desk saves. Its internal server read intentionally does not call
   * loadRoutingConfig: loading would hydrate and discard the editable draft.
   */
  const saveLaneMemoryPolicy = useCallback(
    async (
      humanIdentityId: string,
      assistantAgentId: string,
      memoryMode: string,
      options?: { localMemorySources?: string[] }
    ) => {
      if (routingSaveInFlightRef.current) {
        throw new Error("People and routing is already being saved.");
      }
      if (!tokenConfigured || !settings.gateway_url.trim()) {
        throw new Error("Connect to the gateway before saving people and routing.");
      }

      const draftAtStart = routingDraftRef.current;
      const configAtStart = routingConfigRef.current;
      if (
        !draftAtStart ||
        !configAtStart ||
        JSON.stringify(draftAtStart) !== JSON.stringify(configAtStart)
      ) {
        throw new Error(
          "Directory / Front Desk has unsaved people and routing edits. Save or reset them there before changing this lane's memory policy."
        );
      }

      const draftRevisionAtStart = routingDraftRevisionRef.current;
      routingSaveInFlightRef.current = true;
      const generation = ++routingGenerationRef.current;
      setRoutingSaving(true);
      try {
        const response = await getRuntimeConfig(settings);
        if (routingDraftRevisionRef.current !== draftRevisionAtStart) {
          throw new Error(
            "Directory / Front Desk changed while the lane memory policy was loading. The lane policy was not saved."
          );
        }

        const nextRouting = cloneRoutingConfig(response.config.routing);
        const existingIndex = nextRouting.lane_memory_policies.findIndex(
          (item) =>
            item.human_identity_id === humanIdentityId &&
            item.assistant_agent_id === assistantAgentId
        );
        if (existingIndex >= 0) {
          nextRouting.lane_memory_policies[existingIndex] = {
            ...nextRouting.lane_memory_policies[existingIndex],
            memory_mode: memoryMode,
            local_memory_sources:
              options?.localMemorySources ??
              nextRouting.lane_memory_policies[existingIndex].local_memory_sources,
          };
        } else {
          nextRouting.lane_memory_policies.push({
            human_identity_id: humanIdentityId,
            assistant_agent_id: assistantAgentId,
            memory_mode: memoryMode,
            lane_id: null,
            local_memory_sources: options?.localMemorySources ?? [],
          });
        }

        const updateResponse = await updateRuntimeConfig(settings, {
          routing: cloneRoutingConfig(nextRouting),
        });
        const savedRouting = cloneRoutingConfig(updateResponse.config.routing);
        // A write can complete after the user edits Front Desk. Keep that
        // exact draft intact; only the authoritative baseline may advance.
        replaceRoutingConfig(savedRouting);
        if (routingDraftRevisionRef.current !== draftRevisionAtStart) {
          setRoutingNotice({
            tone: "info",
            message:
              "Lane memory policy saved. Directory / Front Desk changed while saving, so its newer draft was preserved.",
          });
        } else if (generation === routingGenerationRef.current) {
          replaceRoutingDraft(cloneRoutingConfig(savedRouting));
          setRoutingError(null);
        }
        return savedRouting;
      } finally {
        routingSaveInFlightRef.current = false;
        setRoutingSaving(false);
      }
    },
    [replaceRoutingConfig, replaceRoutingDraft, settings, tokenConfigured]
  );

  const patchRoutingDraft = useCallback(
    (updater: (draft: RuntimeRoutingConfigResponse) => void) => {
      const current = routingDraftRef.current;
      if (!current) {
        return;
      }
      const next = cloneRoutingConfig(current);
      updater(next);
      replaceRoutingDraft(next);
    },
    [replaceRoutingDraft]
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
        next.assistant_assignments = next.assistant_assignments.filter(
          (item) => item.human_identity_id !== humanIdentityId
        );
        const normalizedAssistantAgentId = assistantAgentId.trim();
        if (!normalizedAssistantAgentId) {
          return;
        }
        next.enabled = true;
        if (!next.local_operator_human_identity_id?.trim()) {
          next.local_operator_human_identity_id = humanIdentityId;
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
    if (!routingConfigRef.current) {
      return;
    }
    replaceRoutingDraft(cloneRoutingConfig(routingConfigRef.current));
    setRoutingNotice(null);
    setRoutingError(null);
  }, [replaceRoutingDraft]);

  const saveRoutingDraft = useCallback(async () => {
    if (routingSaveInFlightRef.current) {
      return;
    }
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

    const normalizedLanePolicies: RuntimeLaneMemoryPolicyConfigResponse[] = [];
    for (const policy of routingDraft.lane_memory_policies) {
      const normalizedPolicy: RuntimeLaneMemoryPolicyConfigResponse = {
        human_identity_id: policy.human_identity_id.trim(),
        assistant_agent_id: policy.assistant_agent_id.trim(),
        memory_mode: policy.memory_mode.trim() || "inherit_runtime",
        lane_id: policy.lane_id?.trim() || null,
        local_memory_sources: policy.local_memory_sources
          .map((item) => item.trim())
          .filter(Boolean),
      };
      if (!humanIds.has(normalizedPolicy.human_identity_id)) {
        setRoutingNotice({
          tone: "error",
          message: `A lane memory policy points at missing human "${normalizedPolicy.human_identity_id}".`,
        });
        return;
      }
      if (!knownAgentIds.has(normalizedPolicy.assistant_agent_id)) {
        setRoutingNotice({
          tone: "error",
          message: `A lane memory policy still references unavailable assistant "${normalizedPolicy.assistant_agent_id}". Reconnect that agent before saving so its policy is not lost.`,
        });
        return;
      }
      normalizedLanePolicies.push(normalizedPolicy);
    }
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

    try {
      await restoreRoutingConfig(nextRouting);
      setRoutingNotice({
        tone: "info",
        message: "People and routing saved.",
      });
    } catch (saveError: unknown) {
      setRoutingNotice({
        tone: "error",
        message: `Saving people and routing failed: ${String(saveError)}`,
      });
    }
  }, [agents, restoreRoutingConfig, routingDraft, settings]);

  /**
   * Agent-removal cleanup: drop the agent's assignments and lane policies
   * from the freshest server routing before the agent record is removed.
   * Errors propagate to the caller, which owns the removal flow messaging.
   */
  const detachAgentRouting = useCallback(
    async (agentId: string) => {
      const latestRouting = await loadRoutingConfig(settings);
      if (!latestRouting) {
        throw new Error(
          "Could not read the current people and routing configuration before removing this agent."
        );
      }
      const nextRouting = cloneRoutingConfig(latestRouting);
      nextRouting.assistant_assignments = nextRouting.assistant_assignments.filter(
        (assignment) => assignment.assistant_agent_id !== agentId
      );
      nextRouting.lane_memory_policies = nextRouting.lane_memory_policies.filter(
        (policy) => policy.assistant_agent_id !== agentId
      );
      await restoreRoutingConfig(nextRouting);
      return latestRouting;
    },
    [loadRoutingConfig, restoreRoutingConfig, settings]
  );

  const routingDirty = useMemo(() => {
    if (!routingConfig || !routingDraft) {
      return false;
    }
    return JSON.stringify(routingConfig) !== JSON.stringify(routingDraft);
  }, [routingConfig, routingDraft]);

  const humanRoutingCards = useMemo<HumanRoutingCard[]>(() => {
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
          .map((link, linkIndex) => ({ link, index: linkIndex }))
          .filter((entry) => entry.link.human_identity_id === human.human_identity_id),
      };
    });
  }, [routingDraft]);

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
    const grouped = new Map<string, HumanRoutingCard[]>();
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

  return {
    gatewayConfigured: Boolean(tokenConfigured && settings.gateway_url.trim()),
    routingConfig,
    routingDraft,
    routingLoading,
    routingSaving,
    routingError,
    routingNotice,
    routingDirty,
    humanRoutingCards,
    routingSummary,
    routedHumansByAgentId,
    loadRoutingConfig,
    patchRoutingDraft,
    addHumanIdentity,
    updateHumanDisplayName,
    updateHumanEnabled,
    removeHumanIdentity,
    setHumanAssignment,
    addPlatformIdentityLink,
    updatePlatformIdentityLink,
    removePlatformIdentityLink,
    resetRoutingDraft,
    saveRoutingDraft,
    restoreRoutingConfig,
    saveLaneMemoryPolicy,
    detachAgentRouting,
  };
}

// Keep existing narrow controller test doubles source-compatible. Mounted
// People & Routing always supplies this operation; consumers that require it
// narrow it explicitly at their integration boundary.
export type PeopleRoutingController = Omit<
  ReturnType<typeof usePeopleRoutingController>,
  "saveLaneMemoryPolicy"
> & {
  saveLaneMemoryPolicy?: ReturnType<
    typeof usePeopleRoutingController
  >["saveLaneMemoryPolicy"];
};
