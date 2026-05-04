import { useEffect, useMemo, useState } from "react";
import { Link2, MessageCircle, MessagesSquare, Send } from "lucide-react";
import {
  getChannelConfig,
  getChannelRuntimeStatus,
  getRuntimeConfig,
  reconnectChannelRuntime,
  updateChannelConfig,
  updateRuntimeConfig,
  upsertRuntimeSecret,
} from "../../lib/api";
import type {
  Agent,
  ChannelConfigResponse,
  ChannelRuntimeAdapterStatusResponse,
  RuntimeChannelsConfigResponse,
  RuntimeConnectionSettings,
} from "../../types";
import type { MissionControlTab } from "../../app/useAppController";
import { Chip } from "../../ui/Chip";
import { Modal } from "../../ui/Modal";
import {
  SIMPLE_INTEGRATIONS,
  SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT,
  SIMPLE_INTEGRATION_WIZARD_STORAGE_KEY,
  getSimpleIntegrationDefinition,
  type SimpleIntegrationId,
} from "./simpleIntegrations";
import { deriveSimpleIntegrationStatus } from "./simpleIntegrationStatus";

interface SimpleIntegrationWizardProps {
  open: boolean;
  onClose: () => void;
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  initialIntegrationId?: SimpleIntegrationId | null;
  onTabChange: (tab: MissionControlTab) => void;
}

interface PersistedSimpleIntegrationDraft {
  selectedIntegrationId: SimpleIntegrationId;
  selectedAgentId: string;
  discordRequireMention: boolean;
  discordAutoRun: boolean;
  telegramRequireMention: boolean;
  telegramAutoRun: boolean;
}

const DEFAULT_PERSISTED_DRAFT: PersistedSimpleIntegrationDraft = {
  selectedIntegrationId: "discord",
  selectedAgentId: "",
  discordRequireMention: true,
  discordAutoRun: true,
  telegramRequireMention: true,
  telegramAutoRun: true,
};

function readPersistedDraft(): PersistedSimpleIntegrationDraft {
  if (typeof window === "undefined") {
    return DEFAULT_PERSISTED_DRAFT;
  }
  try {
    const raw = window.localStorage.getItem(SIMPLE_INTEGRATION_WIZARD_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_PERSISTED_DRAFT;
    }
    const parsed = JSON.parse(raw) as Partial<PersistedSimpleIntegrationDraft>;
    return {
      ...DEFAULT_PERSISTED_DRAFT,
      ...parsed,
    };
  } catch {
    return DEFAULT_PERSISTED_DRAFT;
  }
}

function writePersistedDraft(draft: PersistedSimpleIntegrationDraft) {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(
    SIMPLE_INTEGRATION_WIZARD_STORAGE_KEY,
    JSON.stringify(draft)
  );
}

function iconForIntegration(id: SimpleIntegrationId) {
  switch (id) {
    case "discord":
      return <MessageCircle size={16} />;
    case "telegram":
      return <Send size={16} />;
    case "slack":
      return <MessagesSquare size={16} />;
    default:
      return <Link2 size={16} />;
  }
}

function findAgentForModel(
  agents: Agent[],
  modelProvider: string,
  modelId: string
): string {
  return (
    agents.find(
      (agent) =>
        agent.model_provider === modelProvider && agent.model_id === modelId
    )?.agent_id ?? ""
  );
}

function resolveConfiguredAgentId(
  agents: Agent[],
  explicitAgentId: string | null | undefined,
  modelProvider: string,
  modelId: string
): string {
  if (explicitAgentId?.trim()) {
    return explicitAgentId.trim();
  }
  return findAgentForModel(agents, modelProvider, modelId);
}

const LIVE_CHANNEL_SESSION_STATES = new Set(["gateway_connected", "listening"]);
const CHANNEL_RUNTIME_READY_TIMEOUT_MS = 8_000;
const CHANNEL_RUNTIME_READY_POLL_MS = 400;
const CHANNEL_RUNTIME_REQUEST_TIMEOUT_MS = 30_000;

function describeChannelRuntimeFailure(
  integrationName: string,
  adapter: ChannelRuntimeAdapterStatusResponse | null
): string | null {
  if (!adapter) {
    return null;
  }
  const sessionState = adapter.session_state?.trim().toLowerCase() || "offline";
  if (adapter.healthy && !LIVE_CHANNEL_SESSION_STATES.has(sessionState)) {
    if (sessionState === "connecting") {
      return `${integrationName} saved correctly, and carsinOS is still bringing the live listener online. Wait a few seconds, then check again.`;
    }
    return `${integrationName} saved correctly, but the live listener is still offline. The saved token is still on file. Relaunch one-click once, then run Save + check connection again.`;
  }
  const detail = adapter.last_error?.trim() || adapter.detail?.trim() || "";
  if (!detail) {
    return null;
  }
  if (detail.includes("Discord rejected the bot token")) {
    return `${integrationName} rejected this bot token. Paste the bot token from the Discord developer portal and try again.`;
  }
  if (detail.includes("Discord gateway URL request failed")) {
    return `${integrationName} could not fetch the live gateway URL. Recheck the bot token and bot permissions, then try again.`;
  }
  if (detail.includes("opening Discord gateway websocket failed")) {
    return `${integrationName} accepted the saved setup, but the live chat session could not open. Recheck the bot token, gateway intents, and server invite, then try again.`;
  }
  if (detail.includes("telegram getUpdates failed")) {
    return `${integrationName} saved correctly, but Telegram is not answering long-poll requests yet. Recheck the bot token and network reachability, then try again.`;
  }
  return detail;
}

export function SimpleIntegrationWizard(props: SimpleIntegrationWizardProps) {
  const [selectedIntegrationId, setSelectedIntegrationId] =
    useState<SimpleIntegrationId>("discord");
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [discordRequireMention, setDiscordRequireMention] = useState(true);
  const [discordAutoRun, setDiscordAutoRun] = useState(true);
  const [telegramRequireMention, setTelegramRequireMention] = useState(true);
  const [telegramAutoRun, setTelegramAutoRun] = useState(true);
  const [tokenDraft, setTokenDraft] = useState("");
  const [runtimeChannels, setRuntimeChannels] =
    useState<RuntimeChannelsConfigResponse | null>(null);
  const [channelConfig, setChannelConfig] = useState<ChannelConfigResponse | null>(null);
  const [channelStatuses, setChannelStatuses] = useState<
    ChannelRuntimeAdapterStatusResponse[]
  >([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [successText, setSuccessText] = useState<string | null>(null);
  const [step, setStep] = useState<"pick" | "setup" | "done">("pick");

  const selectedIntegration = useMemo(
    () => getSimpleIntegrationDefinition(selectedIntegrationId),
    [selectedIntegrationId]
  );

  const selectedStatusCard = useMemo(
    () =>
      deriveSimpleIntegrationStatus(
        selectedIntegration,
        props.agents,
        runtimeChannels,
        channelConfig,
        channelStatuses
      ),
    [channelConfig, channelStatuses, props.agents, runtimeChannels, selectedIntegration]
  );

  const selectedRuntimeChannel = selectedIntegration.channelProvider
    ? runtimeChannels?.[selectedIntegration.channelProvider] ?? null
    : null;
  const savedTokenExists = Boolean(selectedRuntimeChannel?.bot_token_secret_ref?.trim());

  useEffect(() => {
    if (!props.open) {
      return;
    }
    const persisted = readPersistedDraft();
    const nextIntegrationId = props.initialIntegrationId ?? persisted.selectedIntegrationId;
    setSelectedIntegrationId(nextIntegrationId);
    setSelectedAgentId(persisted.selectedAgentId);
    setDiscordRequireMention(persisted.discordRequireMention);
    setDiscordAutoRun(persisted.discordAutoRun);
    setTelegramRequireMention(persisted.telegramRequireMention);
    setTelegramAutoRun(persisted.telegramAutoRun);
    setTokenDraft("");
    setStep(props.initialIntegrationId ? "setup" : "pick");
    setErrorText(null);
    setSuccessText(null);
  }, [props.initialIntegrationId, props.open]);

  useEffect(() => {
    writePersistedDraft({
      selectedIntegrationId,
      selectedAgentId,
      discordRequireMention,
      discordAutoRun,
      telegramRequireMention,
      telegramAutoRun,
    });
  }, [
    discordAutoRun,
    discordRequireMention,
    selectedAgentId,
    selectedIntegrationId,
    telegramAutoRun,
    telegramRequireMention,
  ]);

  useEffect(() => {
    if (!props.open) {
      return;
    }
    let cancelled = false;
    setLoading(true);
    void Promise.all([
      getRuntimeConfig(props.settings),
      getChannelConfig(props.settings),
      getChannelRuntimeStatus(props.settings),
    ])
      .then(([runtimeResponse, channelResponse, runtimeStatusResponse]) => {
        if (cancelled) {
          return;
        }
        setRuntimeChannels(runtimeResponse.config.channels);
        setChannelConfig(channelResponse.config);
        setChannelStatuses(runtimeStatusResponse.items);
        setDiscordRequireMention(
          channelResponse.config.discord.require_mention_in_guild_channels
        );
        setDiscordAutoRun(channelResponse.config.discord.auto_run_enabled);
        setTelegramRequireMention(
          channelResponse.config.telegram.require_mention_in_groups
        );
        setTelegramAutoRun(channelResponse.config.telegram.auto_run_enabled);
        setSelectedAgentId((current) => {
          if (current) {
            return current;
          }
          const integrationId = props.initialIntegrationId ?? selectedIntegrationId;
          if (integrationId === "discord") {
            return resolveConfiguredAgentId(
              props.agents,
              channelResponse.config.discord.default_agent_id,
              channelResponse.config.discord.default_model_provider,
              channelResponse.config.discord.default_model_id
            );
          }
          if (integrationId === "telegram") {
            return resolveConfiguredAgentId(
              props.agents,
              channelResponse.config.telegram.default_agent_id,
              channelResponse.config.telegram.default_model_provider,
              channelResponse.config.telegram.default_model_id
            );
          }
          return "";
        });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setErrorText(`Quick setup could not load: ${String(error)}`);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [props.agents, props.initialIntegrationId, props.open, props.settings, selectedIntegrationId]);

  const selectedAgent = props.agents.find((agent) => agent.agent_id === selectedAgentId) ?? null;

  const beginSetup = (integrationId: SimpleIntegrationId) => {
    setSelectedIntegrationId(integrationId);
    setStep("setup");
    setErrorText(null);
    setSuccessText(null);
    setTokenDraft("");
  };

  const saveSupportedIntegration = async () => {
    if (!runtimeChannels || !channelConfig || !selectedIntegration.channelProvider) {
      setErrorText("Quick setup is still loading the current channel settings.");
      return;
    }
    if (!selectedAgent) {
      setErrorText("Choose which agent should answer through this integration first.");
      return;
    }
    if (!selectedAgent.model_provider || !selectedAgent.model_id) {
      setErrorText(
        "That agent does not have a provider and model yet. Finish the Team or Wizard setup first."
      );
      return;
    }
    const normalizedToken = tokenDraft.replace(/\s+/g, "").trim();
    const currentChannelRuntime =
      runtimeChannels[selectedIntegration.channelProvider];
    const existingSecretRef = currentChannelRuntime.bot_token_secret_ref;
    const usingSavedToken = !normalizedToken && Boolean(existingSecretRef?.trim());
    if (!normalizedToken && !usingSavedToken) {
      setErrorText(`Paste the ${selectedIntegration.credentialLabel.toLowerCase()} first.`);
      return;
    }

    setSaving(true);
    setErrorText(null);
    setSuccessText(null);
    try {
      const waitForRuntimeReady = async () => {
        const deadline = Date.now() + CHANNEL_RUNTIME_READY_TIMEOUT_MS;
        let latestStatuses: ChannelRuntimeAdapterStatusResponse[] = [];
        while (Date.now() <= deadline) {
          const statusResponse = await getChannelRuntimeStatus(props.settings, {
            timeoutMs: CHANNEL_RUNTIME_REQUEST_TIMEOUT_MS,
          });
          latestStatuses = statusResponse.items;
          const adapter =
            latestStatuses.find(
              (item) => item.provider === selectedIntegration.channelProvider
            ) ?? null;
          if (
            adapter?.healthy &&
            LIVE_CHANNEL_SESSION_STATES.has(adapter.session_state ?? "")
          ) {
            return {
              statuses: latestStatuses,
              adapter,
            };
          }
          await new Promise((resolve) =>
            window.setTimeout(resolve, CHANNEL_RUNTIME_READY_POLL_MS)
          );
        }
        const statusResponse = await getChannelRuntimeStatus(props.settings, {
          timeoutMs: CHANNEL_RUNTIME_REQUEST_TIMEOUT_MS,
        });
        const adapter =
          statusResponse.items.find(
            (item) => item.provider === selectedIntegration.channelProvider
          ) ?? null;
        return {
          statuses: statusResponse.items,
          adapter,
        };
      };

      const scope = `channels/${selectedIntegration.channelProvider}/bot_token`;
      let nextSecretRef = existingSecretRef;
      if (normalizedToken) {
        const secretResponse = await upsertRuntimeSecret(props.settings, {
          scope,
          secret_value: normalizedToken,
          previous_secret_ref: existingSecretRef,
        });
        nextSecretRef = secretResponse.secret_ref;
      }

      const nextRuntimeChannels: RuntimeChannelsConfigResponse = {
        ...runtimeChannels,
        [selectedIntegration.channelProvider]: {
          ...currentChannelRuntime,
          enabled: true,
          operation_mode: "transport",
          bot_token_secret_ref: nextSecretRef,
        },
      };
      await updateRuntimeConfig(props.settings, {
        channels: nextRuntimeChannels,
      });

      const nextChannelConfig =
        selectedIntegration.channelProvider === "discord"
          ? {
              discord: {
                ...channelConfig.discord,
                require_mention_in_guild_channels: discordRequireMention,
                auto_run_enabled: discordAutoRun,
                default_agent_id: selectedAgent.agent_id,
                default_model_provider: selectedAgent.model_provider,
                default_model_id: selectedAgent.model_id,
              },
            }
          : {
              telegram: {
                ...channelConfig.telegram,
                require_mention_in_groups: telegramRequireMention,
                auto_run_enabled: telegramAutoRun,
                default_agent_id: selectedAgent.agent_id,
                default_model_provider: selectedAgent.model_provider,
                default_model_id: selectedAgent.model_id,
              },
            };
      const updatedChannelResponse = await updateChannelConfig(
        props.settings,
        nextChannelConfig
      );
      setChannelConfig(updatedChannelResponse.config);
      setRuntimeChannels(nextRuntimeChannels);

      await reconnectChannelRuntime(props.settings, selectedIntegration.channelProvider, {
        timeoutMs: CHANNEL_RUNTIME_REQUEST_TIMEOUT_MS,
      });
      const { statuses, adapter } = await waitForRuntimeReady();
      setChannelStatuses(statuses);
      if (!adapter?.healthy) {
        throw new Error(
          adapter?.last_error ||
            adapter?.detail ||
            `${selectedIntegration.displayName} did not report healthy yet.`
        );
      }
      if (!LIVE_CHANNEL_SESSION_STATES.has(adapter.session_state ?? "")) {
        throw new Error(
          adapter?.detail ||
            `${selectedIntegration.displayName} saved correctly, but the live chat session did not come online yet.`
        );
      }
      setTokenDraft("");
      setSuccessText(
        `${selectedIntegration.displayName} is ${normalizedToken ? "saved" : "rechecked"} and assigned to ${selectedAgent.name || selectedAgent.agent_id}. Check the final step to see what carsinOS can actually prove right now.`
      );
      window.dispatchEvent(new CustomEvent(SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT));
      setStep("done");
    } catch (error: unknown) {
      let fallbackMessage = String(error);
      try {
        const latestStatusResponse = await getChannelRuntimeStatus(props.settings, {
          timeoutMs: CHANNEL_RUNTIME_REQUEST_TIMEOUT_MS,
        });
        setChannelStatuses(latestStatusResponse.items);
        const adapter =
          latestStatusResponse.items.find(
            (item) => item.provider === selectedIntegration.channelProvider
          ) ?? null;
        const describedFailure = describeChannelRuntimeFailure(
          selectedIntegration.displayName,
          adapter
        );
        if (describedFailure) {
          fallbackMessage = describedFailure;
        }
      } catch {
        // Keep the original error when the status refresh also fails.
      }
      setErrorText(`Quick setup failed: ${fallbackMessage}`);
    } finally {
      setSaving(false);
    }
  };

  const footer =
    step === "pick" ? (
      <button type="button" className="ghost" onClick={props.onClose}>
        Close
      </button>
    ) : step === "setup" ? (
      <>
        <button
          type="button"
          className="ghost"
          onClick={() => {
            setStep("pick");
            setErrorText(null);
            setSuccessText(null);
          }}
          disabled={saving}
        >
          Back
        </button>
        {selectedIntegration.setupMode === "channel_runtime" ? (
          <button type="button" onClick={() => void saveSupportedIntegration()} disabled={saving}>
            {saving ? "Saving + checking..." : "Save + check connection"}
          </button>
        ) : (
          <button
            type="button"
            onClick={() => {
              props.onTabChange("connectors");
              props.onClose();
            }}
          >
            Stay here and use catalog intake
          </button>
        )}
      </>
    ) : (
      <>
        <button
          type="button"
          className="ghost"
          onClick={() => {
            props.onTabChange("assistant");
            props.onClose();
          }}
        >
          Go to Assistant
        </button>
        <button
          type="button"
          className="ghost"
          onClick={() => {
            setStep("pick");
            setSuccessText(null);
          }}
        >
          Set up another
        </button>
        <button type="button" onClick={props.onClose}>
          Done
        </button>
      </>
    );

  return (
    <Modal
      open={props.open}
      onClose={props.onClose}
      title="Simple Integration Setup"
      subtitle="Use this beginner-safe flow for common chat connections. carsinOS will show you whether the token is saved, whether the runtime is healthy, and whether live traffic has actually been proven."
      footer={footer}
      width="760px"
    >
      <div className="mc-simple-integration-wizard">
        <div className="mc-simple-integration-steps">
          <span className={`chip${step === "pick" ? " chip-info" : ""}`}>1. Pick</span>
          <span className={`chip${step === "setup" ? " chip-info" : ""}`}>2. Connect</span>
          <span className={`chip${step === "done" ? " chip-info" : ""}`}>3. Test</span>
        </div>

        {loading ? (
          <p className="mc-field-help">Loading current integration settings...</p>
        ) : null}
        {errorText ? <p className="mc-form-error">{errorText}</p> : null}
        {successText ? <p className="mc-form-success">{successText}</p> : null}

        {step === "pick" ? (
          <div className="mc-simple-integration-card-grid">
            {SIMPLE_INTEGRATIONS.map((integration) => (
              <button
                type="button"
                key={integration.id}
                className="mc-simple-integration-card"
                onClick={() => beginSetup(integration.id)}
              >
                <div className="mc-simple-integration-card-head">
                  <strong>
                    {iconForIntegration(integration.id)}
                    <span>{integration.displayName}</span>
                  </strong>
                  <Chip
                    label={integration.statusLabel}
                    tone={
                      integration.setupMode === "channel_runtime" ? "up" : "warning"
                    }
                  />
                </div>
                <p>{integration.shortDescription}</p>
              </button>
            ))}
          </div>
        ) : null}

        {step === "setup" ? (
          <div className="mc-simple-integration-setup">
            <div className="mc-simple-integration-intro">
                <div className="mc-simple-integration-card-head">
                  <strong>
                    {iconForIntegration(selectedIntegration.id)}
                    <span>{selectedIntegration.displayName}</span>
                  </strong>
                  <Chip
                    label={selectedStatusCard.statusLabel}
                    tone={selectedStatusCard.tone}
                  />
                </div>
                <p>{selectedIntegration.plainLanguage}</p>
            </div>

            {selectedIntegration.setupMode === "channel_runtime" ? (
              <>
                <div className="mc-simple-integration-form-grid">
                  <label>
                    {selectedIntegration.credentialLabel}
                    <textarea
                      value={tokenDraft}
                      onChange={(event) => setTokenDraft(event.target.value)}
                      rows={3}
                      placeholder={selectedIntegration.credentialPlaceholder}
                    />
                    <small className="mc-field-help">
                      {savedTokenExists
                        ? `A ${selectedIntegration.displayName} token is already saved in the gateway. Leave this blank to keep using it, or paste a new token to replace it.`
                        : "carsinOS sends this straight to the gateway secret store. It is never persisted in browser storage."}
                    </small>
                  </label>

                  <label>
                    Agent that should answer
                    <select
                      value={selectedAgentId}
                      onChange={(event) => setSelectedAgentId(event.target.value)}
                    >
                      <option value="">Choose agent...</option>
                      {props.agents.map((agent) => (
                        <option key={agent.agent_id} value={agent.agent_id}>
                          {agent.name || agent.agent_id}
                        </option>
                      ))}
                    </select>
                    <small className="mc-field-help">
                      carsinOS will route this integration to one agent by using that agent's current provider and model.
                    </small>
                  </label>
                </div>

                {selectedIntegration.id === "discord" ? (
                  <div className="mc-simple-integration-toggle-row">
                    <label className="mc-checkbox">
                      <input
                        type="checkbox"
                        checked={discordRequireMention}
                        onChange={(event) =>
                          setDiscordRequireMention(event.target.checked)
                        }
                      />
                      {selectedIntegration.mentionToggleLabel}
                    </label>
                    <label className="mc-checkbox">
                      <input
                        type="checkbox"
                        checked={discordAutoRun}
                        onChange={(event) => setDiscordAutoRun(event.target.checked)}
                      />
                      {selectedIntegration.autoRunLabel}
                    </label>
                  </div>
                ) : null}

                {selectedIntegration.id === "telegram" ? (
                  <>
                    <div className="mc-simple-integration-toggle-row">
                      <label className="mc-checkbox">
                        <input
                          type="checkbox"
                          checked={telegramRequireMention}
                          onChange={(event) =>
                            setTelegramRequireMention(event.target.checked)
                          }
                        />
                        {selectedIntegration.mentionToggleLabel}
                      </label>
                      <label className="mc-checkbox">
                        <input
                          type="checkbox"
                          checked={telegramAutoRun}
                          onChange={(event) => setTelegramAutoRun(event.target.checked)}
                        />
                        {selectedIntegration.autoRunLabel}
                      </label>
                    </div>
                    <p className="mc-field-help">
                      Telegram direct messages start locked on purpose. The first new person gets
                      an approval code, and you approve them later in Connectors under Telegram
                      access requests.
                    </p>
                  </>
                ) : null}
              </>
            ) : (
              <div className="mc-simple-integration-expert-note">
                <p>{selectedIntegration.nextStep}</p>
                <p className="mc-field-help">
                  We are being explicit here on purpose: Slack is still expert-only today, so this flow will not pretend otherwise.
                </p>
              </div>
            )}
          </div>
        ) : null}

        {step === "done" ? (
          <div className="mc-simple-integration-done">
            <h3>{selectedIntegration.displayName}: {selectedStatusCard.statusLabel}</h3>
            <p>{selectedStatusCard.summary}</p>
            <ul className="mc-onboarding-checklist">
              <li className="done">Token stored in gateway secret storage</li>
              <li className="done">Integration assigned to one agent</li>
              <li className="done">Runtime state checked by carsinOS</li>
            </ul>
            <div className="mc-simple-integration-expert-note">
              <p><strong>What this means:</strong> {selectedStatusCard.detail}</p>
              <p><strong>How to tell it is really working:</strong> {selectedStatusCard.proofText}</p>
            </div>
          </div>
        ) : null}
      </div>
    </Modal>
  );
}
