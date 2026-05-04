import type { Agent } from "../../types";
import type { SimpleIntegrationId } from "../connectors/simpleIntegrations";
import { StepConnect } from "./steps/StepConnect";
import { StepDone } from "./steps/StepDone";
import { StepMode } from "./steps/StepMode";
import { StepPreflight } from "./steps/StepPreflight";
import { StepProvider } from "./steps/StepProvider";
import { StepReview } from "./steps/StepReview";
import { useOnboardingController } from "./useOnboardingController";

interface OnboardingWizardProps {
  controller: ReturnType<typeof useOnboardingController>;
  agents: Agent[];
  onOpenSimpleIntegrationWizard: (integrationId?: SimpleIntegrationId) => void;
}

export function OnboardingWizard(props: OnboardingWizardProps) {
  const c = props.controller;
  if (!c.isOpen) {
    return null;
  }

  return (
    <div
      className="mc-onboarding-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="mc-onboarding-title"
    >
      <div className="mc-onboarding-modal">
        <header className="mc-onboarding-header">
          <div>
            <p className="mc-overline">Mission Control</p>
            <h2 id="mc-onboarding-title">Setup Wizard</h2>
          </div>
          <button type="button" className="ghost" onClick={c.dismissWizard}>
            Dismiss (24h)
          </button>
        </header>

        <div className="mc-onboarding-progress">
          {c.steps.map((stepId, index) => (
            <span
              key={stepId}
              className={index <= c.stepIndex ? "active" : ""}
              aria-label={`wizard-step-${stepId}`}
            />
          ))}
        </div>

        {c.errorText ? <div className="mc-onboarding-error">{c.errorText}</div> : null}

        {c.step === "mode" ? (
          <StepMode mode={c.mode} onModeChange={c.setMode} onNext={c.nextStep} />
        ) : null}

        {c.step === "preflight" ? (
          <StepPreflight
            preflight={c.preflight}
            onRun={c.runPreflight}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "connect" ? (
          <StepConnect
            busy={c.busy}
            mode={c.mode}
            gatewayUrl={c.gatewayUrl}
            gatewayTokenInput={c.gatewayTokenInput}
            tokenConfigured={c.tokenConfigured}
            connected={c.connected}
            onGatewayUrlChange={c.setGatewayUrl}
            onGatewayTokenInputChange={c.setGatewayTokenInput}
            onConnect={c.connectGateway}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "provider" ? (
          <StepProvider
            busy={c.busy}
            mode={c.mode}
            agents={props.agents}
            selectedAgentId={c.selectedAgentId}
            agentIdDraft={c.agentIdDraft}
            agentNameDraft={c.agentNameDraft}
            workspaceRootDraft={c.workspaceRootDraft}
            toolProfileDraft={c.toolProfileDraft}
            reportsToAgentIdDraft={c.reportsToAgentIdDraft}
            roleLabelDraft={c.roleLabelDraft}
            strategyEnabled={c.strategyEnabled}
            bootstrapPresets={c.bootstrapPresets}
            selectedPresetKey={c.selectedPresetKey}
            agentReady={c.agentReady}
            providerPath={c.providerPath}
            useExistingProfile={c.useExistingProfile}
            existingProviderProfiles={c.existingProviderProfiles}
            selectedExistingProfileId={c.selectedExistingProfileId}
            providerReady={c.providerReady}
            routingReady={c.routingReady}
            localProvider={c.localProvider}
            localUseConnectionProfile={c.localUseConnectionProfile}
            localConnectionProfileName={c.localConnectionProfileName}
            localApiBaseUrl={c.localApiBaseUrl}
            localApiKey={c.localApiKey}
            localModelId={c.localModelId}
            localOrchestratorEnabled={c.localOrchestratorEnabled}
            localOrchestratorAgentId={c.localOrchestratorAgentId}
            localOrchestratorAgentName={c.localOrchestratorAgentName}
            localOrchestratorModelId={c.localOrchestratorModelId}
            localModelDiscoveryNote={c.localModelDiscoveryNote}
            localProviderOptions={c.localProviderOptions}
            localModelOptions={c.localModelOptions}
            localModelsLoading={c.localModelsLoading}
            localModelsError={c.localModelsError}
            cloudModelId={c.cloudModelId}
            cloudModelOptions={c.cloudModelOptions}
            cloudModelsLoading={c.cloudModelsLoading}
            cloudModelsError={c.cloudModelsError}
            cloudModelDiscoveryNote={c.cloudModelDiscoveryNote}
            anthropicAuthMode={c.anthropicAuthMode}
            anthropicDisplayName={c.anthropicDisplayName}
            anthropicSetupToken={c.anthropicSetupToken}
            anthropicSetupLaunchNote={c.anthropicSetupLaunchNote}
            anthropicValidationBusy={c.anthropicValidationBusy}
            anthropicValidationNote={c.anthropicValidationNote}
            anthropicApiBaseUrl={c.anthropicApiBaseUrl}
            anthropicHeadlessCommand={c.anthropicHeadlessCommand}
            anthropicHeadlessArgs={c.anthropicHeadlessArgs}
            openAiDisplayName={c.openAiDisplayName}
            openAiClientId={c.openAiClientId}
            openAiApiBaseUrl={c.openAiApiBaseUrl}
            openAiSessionId={c.openAiSessionId}
            openAiAuthorizeUrl={c.openAiAuthorizeUrl}
            openAiCallbackUrlHint={c.openAiCallbackUrlHint}
            openAiCallbackUrl={c.openAiCallbackUrl}
            openAiCode={c.openAiCode}
            openAiState={c.openAiState}
            onSelectedAgentIdChange={c.setSelectedAgentId}
            onAgentIdDraftChange={c.setAgentIdDraft}
            onAgentNameDraftChange={c.setAgentNameDraft}
            onWorkspaceRootDraftChange={c.setWorkspaceRootDraft}
            onToolProfileDraftChange={c.setToolProfileDraft}
            onReportsToAgentIdDraftChange={c.setReportsToAgentIdDraft}
            onRoleLabelDraftChange={c.setRoleLabelDraft}
            onSelectedPresetKeyChange={c.setSelectedPresetKey}
            onApplySelectedPreset={c.applySelectedPreset}
            onCreateNewAgentDraft={c.createNewAgentDraft}
            onSaveAgent={c.saveAgent}
            onDeleteSelectedAgent={c.deleteSelectedAgent}
            onProviderPathChange={c.setProviderPath}
            onUseExistingProfileChange={c.setUseExistingProfile}
            onSelectedExistingProfileIdChange={c.setSelectedExistingProfileId}
            onLocalProviderChange={c.setLocalProvider}
            onLocalUseConnectionProfileChange={c.setLocalUseConnectionProfile}
            onLocalConnectionProfileNameChange={c.setLocalConnectionProfileName}
            onLocalApiBaseUrlChange={c.setLocalApiBaseUrl}
            onLocalApiKeyChange={c.setLocalApiKey}
            onLocalModelIdChange={c.setLocalModelId}
            onLocalOrchestratorEnabledChange={c.setLocalOrchestratorEnabled}
            onLocalOrchestratorAgentIdChange={c.setLocalOrchestratorAgentId}
            onLocalOrchestratorAgentNameChange={c.setLocalOrchestratorAgentName}
            onLocalOrchestratorModelIdChange={c.setLocalOrchestratorModelId}
            onRefreshLocalModels={c.refreshLocalModels}
            onCloudModelIdChange={c.setCloudModelId}
            onAnthropicAuthModeChange={c.setAnthropicAuthMode}
            onAnthropicDisplayNameChange={c.setAnthropicDisplayName}
            onAnthropicSetupTokenChange={c.setAnthropicSetupToken}
            onLaunchAnthropicSetupTokenFlow={c.launchAnthropicSetupTokenFlow}
            onValidateAnthropicSetupToken={c.runAnthropicSetupTokenValidation}
            onAnthropicApiBaseUrlChange={c.setAnthropicApiBaseUrl}
            onAnthropicHeadlessCommandChange={c.setAnthropicHeadlessCommand}
            onAnthropicHeadlessArgsChange={c.setAnthropicHeadlessArgs}
            onOpenAiDisplayNameChange={c.setOpenAiDisplayName}
            onOpenAiClientIdChange={c.setOpenAiClientId}
            onOpenAiApiBaseUrlChange={c.setOpenAiApiBaseUrl}
            onOpenAiCallbackUrlChange={c.setOpenAiCallbackUrl}
            onOpenAiCodeChange={c.setOpenAiCode}
            onOpenAiStateChange={c.setOpenAiState}
            onStartOpenAiOauthFlow={c.startOpenAiOauthFlow}
            onFinishOpenAiOauthFlow={c.finishOpenAiOauthFlow}
            onReauthSelectedProfile={c.reauthSelectedProfile}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "review" ? (
          <StepReview
            connected={c.connected}
            agentReady={c.agentReady}
            providerReady={c.providerReady}
            routingReady={c.routingReady}
            selectedAgentId={c.selectedAgentId}
            providerPath={c.providerPath}
            providerProfileId={c.providerProfileId}
            canFinishReview={c.canFinishReview}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "done" ? (
          <StepDone
            actions={[
              {
                id: "assistant",
                label: "Go to Assistant",
                description: "Send your first message right away.",
                onClick: () => c.completeAndExitTo("assistant"),
              },
              {
                id: "boards",
                label: "Go to Boards",
                description: "Create your first task or card.",
                onClick: () => c.completeAndExitTo("boards"),
              },
              {
                id: "integrations",
                label: "Set up Discord or Telegram",
                description: "Open the simple integration wizard next.",
                onClick: () => {
                  c.dismissWizard();
                  props.onOpenSimpleIntegrationWizard();
                },
              },
            ]}
          />
        ) : null}
      </div>
    </div>
  );
}
