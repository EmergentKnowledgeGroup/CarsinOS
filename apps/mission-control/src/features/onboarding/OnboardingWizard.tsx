import type { Agent } from "../../types";
import { StepAgent } from "./steps/StepAgent";
import { StepConnect } from "./steps/StepConnect";
import { StepDone } from "./steps/StepDone";
import { StepMode } from "./steps/StepMode";
import { StepPreflight } from "./steps/StepPreflight";
import { StepProvider } from "./steps/StepProvider";
import { StepReview } from "./steps/StepReview";
import { StepRouting } from "./steps/StepRouting";
import { useOnboardingController } from "./useOnboardingController";

interface OnboardingWizardProps {
  controller: ReturnType<typeof useOnboardingController>;
  agents: Agent[];
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
            connected={c.connected}
            onGatewayUrlChange={c.setGatewayUrl}
            onGatewayTokenInputChange={c.setGatewayTokenInput}
            onConnect={c.connectGateway}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "agent" ? (
          <StepAgent
            busy={c.busy}
            agents={props.agents}
            selectedAgentId={c.selectedAgentId}
            agentIdDraft={c.agentIdDraft}
            agentNameDraft={c.agentNameDraft}
            workspaceRootDraft={c.workspaceRootDraft}
            toolProfileDraft={c.toolProfileDraft}
            agentReady={c.agentReady}
            onSelectedAgentIdChange={c.setSelectedAgentId}
            onAgentIdDraftChange={c.setAgentIdDraft}
            onAgentNameDraftChange={c.setAgentNameDraft}
            onWorkspaceRootDraftChange={c.setWorkspaceRootDraft}
            onToolProfileDraftChange={c.setToolProfileDraft}
            onEnsureAgent={c.ensureAgent}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "provider" ? (
          <StepProvider
            busy={c.busy}
            providerPath={c.providerPath}
            useExistingProfile={c.useExistingProfile}
            existingProviderProfiles={c.existingProviderProfiles}
            selectedExistingProfileId={c.selectedExistingProfileId}
            providerReady={c.providerReady}
            localProvider={c.localProvider}
            localModelId={c.localModelId}
            localProviderOptions={c.localProviderOptions}
            localModelOptions={c.localModelOptions}
            localModelsLoading={c.localModelsLoading}
            localModelsError={c.localModelsError}
            anthropicDisplayName={c.anthropicDisplayName}
            anthropicSetupToken={c.anthropicSetupToken}
            anthropicApiBaseUrl={c.anthropicApiBaseUrl}
            openAiDisplayName={c.openAiDisplayName}
            openAiClientId={c.openAiClientId}
            openAiApiBaseUrl={c.openAiApiBaseUrl}
            openAiSessionId={c.openAiSessionId}
            openAiAuthorizeUrl={c.openAiAuthorizeUrl}
            openAiCallbackUrlHint={c.openAiCallbackUrlHint}
            openAiCallbackUrl={c.openAiCallbackUrl}
            openAiCode={c.openAiCode}
            openAiState={c.openAiState}
            onProviderPathChange={c.setProviderPath}
            onUseExistingProfileChange={c.setUseExistingProfile}
            onSelectedExistingProfileIdChange={c.setSelectedExistingProfileId}
            onLocalProviderChange={c.setLocalProvider}
            onLocalModelIdChange={c.setLocalModelId}
            onAnthropicDisplayNameChange={c.setAnthropicDisplayName}
            onAnthropicSetupTokenChange={c.setAnthropicSetupToken}
            onAnthropicApiBaseUrlChange={c.setAnthropicApiBaseUrl}
            onOpenAiDisplayNameChange={c.setOpenAiDisplayName}
            onOpenAiClientIdChange={c.setOpenAiClientId}
            onOpenAiApiBaseUrlChange={c.setOpenAiApiBaseUrl}
            onOpenAiCallbackUrlChange={c.setOpenAiCallbackUrl}
            onOpenAiCodeChange={c.setOpenAiCode}
            onOpenAiStateChange={c.setOpenAiState}
            onStartOpenAiOauthFlow={c.startOpenAiOauthFlow}
            onFinishOpenAiOauthFlow={c.finishOpenAiOauthFlow}
            onCompleteProvider={c.completeProvider}
            onBack={c.previousStep}
            onNext={c.nextStep}
          />
        ) : null}

        {c.step === "routing" ? (
          <StepRouting
            busy={c.busy}
            providerPath={c.providerPath}
            selectedAgentId={c.selectedAgentId}
            providerProfileId={c.providerProfileId}
            routingReady={c.routingReady}
            onApplyRouting={c.applyRouting}
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

        {c.step === "done" ? <StepDone onGoBoards={c.completeAndExit} /> : null}
      </div>
    </div>
  );
}
