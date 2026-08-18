import { describe, expect, it } from "vitest";
import type {
  Agent,
  ChannelConfigResponse,
  ChannelRuntimeAdapterStatusResponse,
  RuntimeChannelsConfigResponse,
} from "../../types";
import { SIMPLE_INTEGRATIONS } from "./simpleIntegrations";
import { deriveSimpleIntegrationStatuses } from "./simpleIntegrationStatus";

const AGENTS: Agent[] = [
  {
    agent_id: "claude",
    name: "Claude",
    model_provider: "anthropic",
    model_id: "claude-sonnet-4-5",
    workspace_root: "",
    tool_profile: "default",
    reports_to_agent_id: null,
    role_label: "Assistant",
  },
];

const CHANNEL_CONFIG: ChannelConfigResponse = {
  discord: {
    require_mention_in_guild_channels: true,
    allowlisted_user_ids: [],
    auto_run_enabled: true,
    default_agent_id: "claude",
    default_model_provider: "anthropic",
    default_model_id: "claude-sonnet-4-5",
  },
  telegram: {
    require_mention_in_groups: true,
    allowlisted_user_ids: [],
    dm_policy: "pairing",
    group_policy: "allowlist",
    group_allowlisted_user_ids: [],
    allowlisted_chat_ids: [],
    auto_leave_unauthorized_groups: true,
    pairing_code_ttl_seconds: 3600,
    pairing_max_pending: 3,
    unauthorized_spam_threshold: 4,
    unauthorized_spam_block_seconds: 3600,
    auto_run_enabled: true,
    default_agent_id: "claude",
    default_model_provider: "anthropic",
    default_model_id: "claude-sonnet-4-5",
  },
  updated_at: 0,
};

const RUNTIME_CHANNELS: RuntimeChannelsConfigResponse = {
  discord: {
    enabled: true,
    bot_token_secret_ref: "secret://runtime.channels.discord.bot_token",
    operation_mode: "transport",
    api_base_url: null,
    transport_timeout_ms: null,
    transport_retry_attempts: null,
    application_id: null,
    intents: [],
    staging_guild_ids: [],
    staging_channel_ids: [],
  },
  telegram: {
    enabled: false,
    bot_token_secret_ref: null,
    operation_mode: "shim",
    api_base_url: null,
    transport_timeout_ms: null,
    transport_retry_attempts: null,
    long_poll_timeout_seconds: null,
    webhook_mode: "long_poll",
    webhook_url: null,
    staging_chat_ids: [],
  },
};

const CHANNEL_STATUSES: ChannelRuntimeAdapterStatusResponse[] = [
  {
    provider: "discord",
    lifecycle_state: "running",
    healthy: true,
    session_state: "gateway_connected",
    proof_state: "unproven",
    detail: "discord adapter healthy (mode=transport)",
    proof_detail: "Discord is connected and waiting for the first real message",
    last_error: null,
    last_inbound_at: null,
    last_outbound_at: null,
    last_proven_at: null,
    reconnect_attempts: 1,
    updated_at: 0,
  },
];

describe("deriveSimpleIntegrationStatuses", () => {
  it("treats connected but unproven discord setup as waiting for first message", () => {
    const discord = deriveSimpleIntegrationStatuses(
      SIMPLE_INTEGRATIONS,
      AGENTS,
      RUNTIME_CHANNELS,
      CHANNEL_CONFIG,
      CHANNEL_STATUSES
    ).find((item) => item.id === "discord");

    expect(discord).toMatchObject({
      statusLabel: "Connected and waiting for first message",
      tone: "warning",
      assignedAgentLabel: "Claude",
      runtimeLabel: "gateway connected",
    });
    expect(discord?.detail).toContain("waiting for the first real message");
  });

  it("turns green only after a real roundtrip is proven", () => {
    const provenStatuses: ChannelRuntimeAdapterStatusResponse[] = [
      {
        ...CHANNEL_STATUSES[0],
        proof_state: "roundtrip_confirmed",
        proof_detail: "Discord has already seen a real message and answered back successfully.",
        last_inbound_at: 1000,
        last_outbound_at: 1100,
        last_proven_at: 1100,
      },
    ];

    const discord = deriveSimpleIntegrationStatuses(
      SIMPLE_INTEGRATIONS,
      AGENTS,
      RUNTIME_CHANNELS,
      CHANNEL_CONFIG,
      provenStatuses
    ).find((item) => item.id === "discord");

    expect(discord).toMatchObject({
      statusLabel: "Connected and proven working",
      tone: "up",
      runtimeLabel: "gateway connected",
    });
  });

  it("treats missing telegram token as not connected", () => {
    const telegram = deriveSimpleIntegrationStatuses(
      SIMPLE_INTEGRATIONS,
      AGENTS,
      RUNTIME_CHANNELS,
      CHANNEL_CONFIG,
      CHANNEL_STATUSES
    ).find((item) => item.id === "telegram");

    expect(telegram).toMatchObject({
      statusLabel: "Not connected yet",
      tone: "down",
      runtimeLabel: "token missing",
    });
  });

  it("keeps slack on the expert-only path", () => {
    const slack = deriveSimpleIntegrationStatuses(
      SIMPLE_INTEGRATIONS,
      AGENTS,
      RUNTIME_CHANNELS,
      CHANNEL_CONFIG,
      CHANNEL_STATUSES
    ).find((item) => item.id === "slack");

    expect(slack).toMatchObject({
      statusLabel: "Expert setup today",
      tone: "warning",
      runtimeLabel: "expert-only path",
    });
  });

  it("uses the explicit assigned agent instead of guessing from a shared model", () => {
    const duplicateAgents: Agent[] = [
      ...AGENTS,
      {
        agent_id: "assistant",
        name: "Assistant",
        model_provider: "anthropic",
        model_id: "claude-sonnet-4-5",
        workspace_root: "",
        tool_profile: "default",
        reports_to_agent_id: null,
        role_label: "Built-in",
      },
    ];

    const discord = deriveSimpleIntegrationStatuses(
      SIMPLE_INTEGRATIONS,
      duplicateAgents,
      RUNTIME_CHANNELS,
      CHANNEL_CONFIG,
      CHANNEL_STATUSES
    ).find((item) => item.id === "discord");

    expect(discord?.assignedAgentLabel).toBe("Claude");
  });
});
