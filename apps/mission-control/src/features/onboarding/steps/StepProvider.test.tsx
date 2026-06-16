// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StepProvider } from "./StepProvider";

const noop = () => {};
const asyncTrue = async () => true;
const asyncVoid = async () => {};


describe("StepProvider", () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    // @ts-expect-error test-only React flag
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  it("shows the ExecAss default-name prompt while creating a new agent", async () => {
    const root = createRoot(container);

    await act(async () => {
      root.render(
        <StepProvider
          busy={false}
          mode="quickstart"
          agents={[]}
          selectedAgentId=""
          agentIdDraft="execass"
          agentNameDraft="ExecAss"
          workspaceRootDraft="."
          toolProfileDraft="default"
          reportsToAgentIdDraft=""
          roleLabelDraft="Executive Assistant"
          strategyEnabled={false}
          bootstrapPresets={[]}
          selectedPresetKey=""
          agentReady
          providerPath="local"
          useExistingProfile={false}
          existingProviderProfiles={[]}
          selectedExistingProfileId=""
          providerReady={false}
          routingReady={false}
          localProvider="lmstudio"
          localUseConnectionProfile={false}
          localConnectionProfileName="lmstudio-local"
          localApiBaseUrl=""
          localApiKey=""
          localModelId=""
          localOrchestratorEnabled={false}
          localOrchestratorAgentId="orchestrator"
          localOrchestratorAgentName="Orchestrator"
          localOrchestratorModelId=""
          localModelDiscoveryNote={null}
          localProviderOptions={[{ value: "lmstudio", label: "LM Studio" }]}
          localModelOptions={[]}
          localModelsLoading={false}
          localModelsError={null}
          cloudModelId=""
          cloudModelOptions={[]}
          cloudModelsLoading={false}
          cloudModelsError={null}
          cloudModelDiscoveryNote={null}
          anthropicDisplayName=""
          anthropicSetupToken=""
          anthropicValidationBusy={false}
          anthropicValidationNote={null}
          anthropicApiBaseUrl=""
          openAiDisplayName=""
          openAiClientId=""
          openAiApiBaseUrl=""
          openAiSessionId=""
          openAiAuthorizeUrl=""
          openAiCallbackUrlHint=""
          openAiCallbackUrl=""
          openAiCode=""
          openAiState=""
          onSelectedAgentIdChange={noop}
          onAgentIdDraftChange={noop}
          onAgentNameDraftChange={noop}
          onWorkspaceRootDraftChange={noop}
          onToolProfileDraftChange={noop}
          onReportsToAgentIdDraftChange={noop}
          onRoleLabelDraftChange={noop}
          onSelectedPresetKeyChange={noop}
          onApplySelectedPreset={noop}
          onCreateNewAgentDraft={noop}
          onSaveAgent={asyncTrue}
          onDeleteSelectedAgent={asyncTrue}
          onProviderPathChange={noop}
          onUseExistingProfileChange={noop}
          onSelectedExistingProfileIdChange={noop}
          onLocalProviderChange={noop}
          onLocalUseConnectionProfileChange={noop}
          onLocalConnectionProfileNameChange={noop}
          onLocalApiBaseUrlChange={noop}
          onLocalApiKeyChange={noop}
          onLocalModelIdChange={noop}
          onLocalOrchestratorEnabledChange={noop}
          onLocalOrchestratorAgentIdChange={noop}
          onLocalOrchestratorAgentNameChange={noop}
          onLocalOrchestratorModelIdChange={noop}
          onRefreshLocalModels={asyncVoid}
          onCloudModelIdChange={noop}
          onAnthropicDisplayNameChange={noop}
          onAnthropicSetupTokenChange={noop}
          onValidateAnthropicSetupToken={asyncVoid}
          onAnthropicApiBaseUrlChange={noop}
          onOpenAiDisplayNameChange={noop}
          onOpenAiClientIdChange={noop}
          onOpenAiApiBaseUrlChange={noop}
          onOpenAiCallbackUrlChange={noop}
          onOpenAiCodeChange={noop}
          onOpenAiStateChange={noop}
          onStartOpenAiOauthFlow={asyncVoid}
          onFinishOpenAiOauthFlow={asyncVoid}
          onReauthSelectedProfile={asyncTrue}
          onBack={noop}
          onNext={noop}
        />
      );
    });

    expect(container.textContent).toContain(
      "Before we begin, my default name is ExecAss, short for Executive Assistant."
    );
    expect(container.textContent).toContain(
      "Is that okay, or would you like to give me a different name?"
    );
  });
});
