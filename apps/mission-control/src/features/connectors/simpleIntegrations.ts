export type SimpleIntegrationId = "discord" | "telegram" | "slack";

export interface SimpleIntegrationDefinition {
  id: SimpleIntegrationId;
  displayName: string;
  shortDescription: string;
  plainLanguage: string;
  credentialLabel: string;
  credentialPlaceholder: string;
  nextStep: string;
  statusLabel: string;
  setupMode: "channel_runtime" | "expert_only";
  channelProvider?: "discord" | "telegram";
  mentionToggleLabel?: string;
  autoRunLabel?: string;
  runtimeReadyLabel: string;
  provenWorkingLabel: string;
  workingProof: string;
  caveat: string;
  truthWarning?: string;
  launchNextStepLabel: string;
}

export const SIMPLE_INTEGRATIONS: readonly SimpleIntegrationDefinition[] = [
  {
    id: "discord",
    displayName: "Discord",
    shortDescription: "Let people talk to one agent through a Discord bot.",
    plainLanguage:
      "Connect a Discord bot token, choose which agent should answer, and carsinOS will route messages to that agent.",
    credentialLabel: "Discord bot token",
    credentialPlaceholder: "Paste the Discord bot token",
    nextStep:
      "If your Discord bridge is already wired into carsinOS, mention the bot in a test channel and watch for a reply.",
    statusLabel: "Simple setup",
    setupMode: "channel_runtime",
    channelProvider: "discord",
    mentionToggleLabel: "Only answer when the bot is mentioned",
    autoRunLabel: "Let the bot answer automatically",
    runtimeReadyLabel: "Connected and waiting for first message",
    provenWorkingLabel: "Connected and proven working",
    workingProof:
      "A real Discord message should create live activity in carsinOS and the assigned agent should reply back into the same Discord conversation.",
    caveat:
      "carsinOS should only show green here after Discord has seen a real message and answered back successfully.",
    truthWarning:
      "If this is not green yet, carsinOS is still waiting for proof that Discord works end to end.",
    launchNextStepLabel: "Open quick setup",
  },
  {
    id: "telegram",
    displayName: "Telegram",
    shortDescription: "Let people talk to one agent through a Telegram bot.",
    plainLanguage:
      "Connect a Telegram bot token, choose which agent should answer, and carsinOS will route new bot messages to that agent.",
    credentialLabel: "Telegram bot token",
    credentialPlaceholder: "Paste the Telegram bot token",
    nextStep:
      "If your Telegram bridge is already wired into carsinOS, send the bot a test message and confirm your agent replies there.",
    statusLabel: "Simple setup",
    setupMode: "channel_runtime",
    channelProvider: "telegram",
    mentionToggleLabel: "Only answer in groups when the bot is mentioned",
    autoRunLabel: "Let the bot answer automatically",
    runtimeReadyLabel: "Connected and waiting for first message",
    provenWorkingLabel: "Connected and proven working",
    workingProof:
      "A real Telegram message should create live activity in carsinOS and the assigned agent should reply back into that Telegram chat.",
    caveat:
      "carsinOS should only show green here after Telegram has seen a real message and answered back successfully.",
    truthWarning:
      "If this is not green yet, carsinOS is still waiting for proof that Telegram works end to end.",
    launchNextStepLabel: "Open quick setup",
  },
  {
    id: "slack",
    displayName: "Slack",
    shortDescription: "Slack still uses the expert connector flow today.",
    plainLanguage:
      "Slack is not on the beginner-safe runtime channel path yet. carsinOS can still connect it, but today that happens through the expert Connectors catalog flow.",
    credentialLabel: "Slack connection",
    credentialPlaceholder: "",
    nextStep:
      "Use the Connectors catalog below to start the Slack connector flow, then come back here later once simple setup exists.",
    statusLabel: "Expert setup today",
    setupMode: "expert_only",
    provenWorkingLabel: "Expert setup only",
    runtimeReadyLabel: "Expert setup only",
    workingProof:
      "Slack is still on the expert connector path, so there is no beginner-safe live proof here yet.",
    caveat:
      "This card is informational only today. Use the catalog flow if you need Slack now.",
    launchNextStepLabel: "Open catalog path",
  },
] as const;

export const SIMPLE_INTEGRATIONS_BY_ID: Record<
  SimpleIntegrationId,
  SimpleIntegrationDefinition
> = Object.fromEntries(
  SIMPLE_INTEGRATIONS.map((item) => [item.id, item])
) as Record<SimpleIntegrationId, SimpleIntegrationDefinition>;

export const SIMPLE_INTEGRATION_WIZARD_STORAGE_KEY =
  "carsinos.simpleIntegrationWizardDraft.v1";

export const SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT =
  "carsinos:simple-integration-status-updated";

export function getSimpleIntegrationDefinition(
  id: SimpleIntegrationId | null | undefined
): SimpleIntegrationDefinition {
  return SIMPLE_INTEGRATIONS_BY_ID[id ?? "discord"] ?? SIMPLE_INTEGRATIONS_BY_ID.discord;
}
