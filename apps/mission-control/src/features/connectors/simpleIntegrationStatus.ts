import type {
  Agent,
  ChannelConfigResponse,
  ChannelRuntimeAdapterStatusResponse,
  RuntimeChannelsConfigResponse,
} from "../../types";
import {
  getSimpleIntegrationDefinition,
  type SimpleIntegrationDefinition,
  type SimpleIntegrationId,
} from "./simpleIntegrations";

export interface SimpleIntegrationStatusViewModel {
  id: SimpleIntegrationId;
  displayName: string;
  statusLabel: string;
  tone: "up" | "warning" | "down" | "checking";
  summary: string;
  detail: string;
  assignedAgentLabel: string;
  runtimeLabel: string;
  proofText: string;
  launchLabel: string;
}

function humanizeRuntimeValue(value: string | null | undefined): string {
  if (!value?.trim()) {
    return "n/a";
  }
  return value.replaceAll("_", " ");
}

function findAssignedAgentLabel(
  agents: Agent[],
  explicitAgentId: string | null | undefined,
  modelProvider: string,
  modelId: string
): string {
  if (explicitAgentId?.trim()) {
    const configuredAgent = agents.find((agent) => agent.agent_id === explicitAgentId.trim());
    return configuredAgent?.name || configuredAgent?.agent_id || "Assigned agent missing";
  }
  if (!modelProvider.trim() || !modelId.trim()) {
    return "No agent assigned yet";
  }
  const matchingAgents = agents.filter(
    (agent) =>
      agent.model_provider === modelProvider && agent.model_id === modelId
  );
  if (matchingAgents.length === 0) {
    return "Assigned model not found";
  }
  if (matchingAgents.length === 1) {
    const assignedAgent = matchingAgents[0];
    return assignedAgent?.name || assignedAgent?.agent_id || "Assigned model not found";
  }
  return "Choose one agent again";
}

function buildRuntimeBackedStatus(
  definition: SimpleIntegrationDefinition,
  agents: Agent[],
  runtimeChannels: RuntimeChannelsConfigResponse,
  channelConfig: ChannelConfigResponse,
  channelStatuses: ChannelRuntimeAdapterStatusResponse[]
): SimpleIntegrationStatusViewModel {
  const provider = definition.channelProvider;
  if (!provider) {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: definition.statusLabel,
      tone: "warning",
      summary: definition.shortDescription,
      detail: definition.caveat,
      assignedAgentLabel: "n/a",
      runtimeLabel: "expert-only path",
      proofText: definition.workingProof,
      launchLabel: definition.launchNextStepLabel,
    };
  }

  const runtimeConfig = runtimeChannels[provider];
  const adapterStatus =
    channelStatuses.find((item) => item.provider === provider) ?? null;
  const channelDefaults =
    provider === "discord"
      ? {
          defaultModelProvider: channelConfig.discord.default_model_provider,
          defaultModelId: channelConfig.discord.default_model_id,
        }
      : {
          defaultModelProvider: channelConfig.telegram.default_model_provider,
          defaultModelId: channelConfig.telegram.default_model_id,
        };

  const assignedAgentLabel = findAssignedAgentLabel(
    agents,
    provider === "discord"
      ? channelConfig.discord.default_agent_id
      : channelConfig.telegram.default_agent_id,
    channelDefaults.defaultModelProvider,
    channelDefaults.defaultModelId
  );
  const tokenConfigured = Boolean(runtimeConfig.bot_token_secret_ref?.trim());
  const runtimeEnabled = runtimeConfig.enabled;
  const transportModeReady = runtimeConfig.operation_mode === "transport";
  const adapterHealthy = Boolean(adapterStatus?.healthy);
  const adapterDetail = adapterStatus?.last_error || adapterStatus?.detail || null;
  const sessionState = adapterStatus?.session_state ?? "offline";
  const proofState = adapterStatus?.proof_state ?? "unproven";
  const proofDetail = adapterStatus?.proof_detail ?? null;

  if (!tokenConfigured) {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: "Not connected yet",
      tone: "down",
      summary: `No ${definition.displayName} token is saved in carsinOS yet.`,
      detail: "Run Quick Setup first so carsinOS can store the token and attach one agent to this chat path.",
      assignedAgentLabel,
      runtimeLabel: "token missing",
      proofText: definition.workingProof,
      launchLabel: `Click this card and connect ${definition.displayName}.`,
    };
  }

  if (!runtimeEnabled || !transportModeReady) {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: "Saved, but runtime is off",
      tone: "warning",
      summary: `${definition.displayName} credentials exist, but the runtime is not in transport mode yet.`,
      detail: "carsinOS will not treat this as live chat traffic until the runtime is enabled and transport mode is active.",
      assignedAgentLabel,
      runtimeLabel: runtimeEnabled ? "transport off" : "runtime disabled",
      proofText: definition.workingProof,
      launchLabel: `Click this card and save again to turn ${definition.displayName} back on.`,
    };
  }

  if (!adapterHealthy) {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: "Needs attention",
      tone: "down",
      summary: `${definition.displayName} is configured, but the runtime is not healthy yet.`,
      detail: adapterDetail || "carsinOS could not confirm a healthy runtime adapter yet.",
      assignedAgentLabel,
      runtimeLabel: humanizeRuntimeValue(adapterStatus?.lifecycle_state || "runtime unhealthy"),
      proofText: definition.workingProof,
      launchLabel: `Click this card and run Save + check connection again.`,
    };
  }

  if (proofState === "roundtrip_confirmed") {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: definition.provenWorkingLabel,
      tone: "up",
      summary: `${definition.displayName} is connected, listening, and has already completed a real message roundtrip.`,
      detail:
        proofDetail ||
        `${definition.displayName} has seen a real message and carsinOS has answered back successfully.`,
      assignedAgentLabel,
      runtimeLabel: humanizeRuntimeValue(sessionState),
      proofText: definition.workingProof,
      launchLabel: `It is working. Send a real ${definition.displayName} message if you want to verify it again.`,
    };
  }

  if (sessionState === "connecting") {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: "Connecting now",
      tone: "checking",
      summary: `${definition.displayName} is configured and carsinOS is opening the live chat session now.`,
      detail: adapterDetail || `carsinOS is still bringing the ${definition.displayName} runtime online.`,
      assignedAgentLabel,
      runtimeLabel: humanizeRuntimeValue(sessionState),
      proofText: definition.workingProof,
      launchLabel: "Wait a few seconds, then refresh this page.",
    };
  }

  if (sessionState === "gateway_connected" || sessionState === "listening") {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: definition.runtimeReadyLabel,
      tone: "warning",
      summary: `${definition.displayName} is connected and listening, but carsinOS is still waiting for the first proven roundtrip.`,
      detail:
        proofDetail ||
        definition.truthWarning ||
        definition.caveat,
      assignedAgentLabel,
      runtimeLabel: humanizeRuntimeValue(sessionState),
      proofText: definition.workingProof,
      launchLabel: `Send one real ${definition.displayName} message now. carsinOS is waiting to prove the roundtrip.`,
    };
  }

  return {
    id: definition.id,
    displayName: definition.displayName,
    statusLabel: "Saved, but not listening yet",
    tone: "warning",
    summary: `${definition.displayName} is configured, but carsinOS has not opened a live listening session yet.`,
    detail:
      adapterDetail ||
      `carsinOS still needs the ${definition.displayName} runtime to enter a live listening state. Click this card and run Save + check connection again to reopen it.`,
    assignedAgentLabel,
    runtimeLabel: humanizeRuntimeValue(sessionState || adapterStatus?.lifecycle_state || "offline"),
    proofText: definition.workingProof,
    launchLabel: `Click this card and run Save + check connection again.`,
  };
}

export function deriveSimpleIntegrationStatus(
  definition: SimpleIntegrationDefinition,
  agents: Agent[],
  runtimeChannels: RuntimeChannelsConfigResponse | null,
  channelConfig: ChannelConfigResponse | null,
  channelStatuses: ChannelRuntimeAdapterStatusResponse[]
): SimpleIntegrationStatusViewModel {
  if (
    definition.setupMode !== "channel_runtime" ||
    !definition.channelProvider ||
    !runtimeChannels ||
    !channelConfig
  ) {
    return {
      id: definition.id,
      displayName: definition.displayName,
      statusLabel: definition.statusLabel,
      tone: definition.setupMode === "expert_only" ? "warning" : "checking",
      summary:
        definition.setupMode === "expert_only"
          ? definition.shortDescription
          : `carsinOS is still loading ${definition.displayName} status.`,
      detail:
        definition.setupMode === "expert_only"
          ? definition.caveat
          : "Status is loading from runtime config and channel health.",
      assignedAgentLabel: definition.setupMode === "expert_only" ? "n/a" : "Loading…",
      runtimeLabel: definition.setupMode === "expert_only" ? "expert-only path" : "loading",
      proofText: definition.workingProof,
      launchLabel: definition.launchNextStepLabel,
    };
  }

  return buildRuntimeBackedStatus(
    definition,
    agents,
    runtimeChannels,
    channelConfig,
    channelStatuses
  );
}

export function deriveSimpleIntegrationStatuses(
  definitions: readonly SimpleIntegrationDefinition[],
  agents: Agent[],
  runtimeChannels: RuntimeChannelsConfigResponse | null,
  channelConfig: ChannelConfigResponse | null,
  channelStatuses: ChannelRuntimeAdapterStatusResponse[]
): SimpleIntegrationStatusViewModel[] {
  return definitions.map((definition) =>
    deriveSimpleIntegrationStatus(
      getSimpleIntegrationDefinition(definition.id),
      agents,
      runtimeChannels,
      channelConfig,
      channelStatuses
    )
  );
}
