import type { AuthProfileResponse } from "../../../types";
import { OnboardingStepShell } from "../OnboardingStepShell";
import type { OnboardingProviderPath } from "../onboardingState";

interface StepProviderProps {
  busy: boolean;
  providerPath: OnboardingProviderPath;
  useExistingProfile: boolean;
  existingProviderProfiles: AuthProfileResponse[];
  selectedExistingProfileId: string;
  providerReady: boolean;
  localProvider: string;
  localModelId: string;
  anthropicDisplayName: string;
  anthropicSetupToken: string;
  anthropicApiBaseUrl: string;
  openAiDisplayName: string;
  openAiClientId: string;
  openAiApiBaseUrl: string;
  openAiSessionId: string;
  openAiAuthorizeUrl: string;
  openAiCallbackUrlHint: string;
  openAiCallbackUrl: string;
  openAiCode: string;
  openAiState: string;
  onProviderPathChange: (value: OnboardingProviderPath) => void;
  onUseExistingProfileChange: (value: boolean) => void;
  onSelectedExistingProfileIdChange: (value: string) => void;
  onLocalProviderChange: (value: string) => void;
  onLocalModelIdChange: (value: string) => void;
  onAnthropicDisplayNameChange: (value: string) => void;
  onAnthropicSetupTokenChange: (value: string) => void;
  onAnthropicApiBaseUrlChange: (value: string) => void;
  onOpenAiDisplayNameChange: (value: string) => void;
  onOpenAiClientIdChange: (value: string) => void;
  onOpenAiApiBaseUrlChange: (value: string) => void;
  onOpenAiCallbackUrlChange: (value: string) => void;
  onOpenAiCodeChange: (value: string) => void;
  onOpenAiStateChange: (value: string) => void;
  onStartOpenAiOauthFlow: () => Promise<void>;
  onFinishOpenAiOauthFlow: () => Promise<void>;
  onCompleteProvider: () => Promise<void>;
  onBack: () => void;
  onNext: () => void;
}

export function StepProvider(props: StepProviderProps) {
  const hasExistingProfiles = props.existingProviderProfiles.length > 0;
  return (
    <OnboardingStepShell
      stepLabel="Step 5 of 8"
      title="Choose Provider Path"
      subtitle="Attach Claude, OpenAI, or local connector mode."
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button
            type="button"
            className="ghost"
            disabled={props.busy}
            aria-busy={props.busy}
            onClick={() => void props.onCompleteProvider()}
          >
            {props.busy ? "Applying..." : "Apply Provider Setup"}
          </button>
          <button type="button" disabled={!props.providerReady} onClick={props.onNext}>
            Continue
          </button>
        </>
      }
    >
      <div className="mc-onboarding-choice-grid">
        <label className="mc-onboarding-choice">
          <input
            type="radio"
            name="provider-path"
            checked={props.providerPath === "anthropic"}
            onChange={() => props.onProviderPathChange("anthropic")}
          />
          <div>
            <strong>Anthropic (Claude)</strong>
            <p>Setup-token ingest flow.</p>
          </div>
        </label>
        <label className="mc-onboarding-choice">
          <input
            type="radio"
            name="provider-path"
            checked={props.providerPath === "openai"}
            onChange={() => props.onProviderPathChange("openai")}
          />
          <div>
            <strong>OpenAI</strong>
            <p>OAuth PKCE flow.</p>
          </div>
        </label>
        <label className="mc-onboarding-choice">
          <input
            type="radio"
            name="provider-path"
            checked={props.providerPath === "local"}
            onChange={() => props.onProviderPathChange("local")}
          />
          <div>
            <strong>Local connector</strong>
            <p>No OAuth; sets local provider on selected agent.</p>
          </div>
        </label>
      </div>

      {props.providerPath === "local" ? (
        <div className="mc-onboarding-field-grid">
          <label>
            Local provider
            <select
              value={props.localProvider}
              onChange={(event) => props.onLocalProviderChange(event.target.value)}
            >
              <option value="ollama">ollama</option>
              <option value="vllm">vllm</option>
              <option value="mock">mock</option>
            </select>
          </label>
          <label>
            Model ID
            <input
              value={props.localModelId}
              onChange={(event) => props.onLocalModelIdChange(event.target.value)}
              placeholder="local-default"
            />
          </label>
        </div>
      ) : (
        <>
          {hasExistingProfiles ? (
            <label className="mc-checkbox">
              <input
                type="checkbox"
                checked={props.useExistingProfile}
                onChange={(event) => props.onUseExistingProfileChange(event.target.checked)}
              />
              Use existing enabled profile
            </label>
          ) : null}

          {props.useExistingProfile && hasExistingProfiles ? (
            <label>
              Existing profile
              <select
                value={props.selectedExistingProfileId}
                onChange={(event) => props.onSelectedExistingProfileIdChange(event.target.value)}
              >
                {props.existingProviderProfiles.map((profile) => (
                  <option key={profile.auth_profile_id} value={profile.auth_profile_id}>
                    {profile.display_name}
                  </option>
                ))}
              </select>
            </label>
          ) : null}

          {!props.useExistingProfile || !hasExistingProfiles ? (
            <>
              {props.providerPath === "anthropic" ? (
                <div className="mc-onboarding-field-grid">
                  <label>
                    Profile name
                    <input
                      value={props.anthropicDisplayName}
                      onChange={(event) => props.onAnthropicDisplayNameChange(event.target.value)}
                      placeholder="claude-primary"
                    />
                  </label>
                  <label>
                    Setup token
                    <input
                      type="password"
                      value={props.anthropicSetupToken}
                      onChange={(event) => props.onAnthropicSetupTokenChange(event.target.value)}
                      placeholder="Paste setup token"
                    />
                  </label>
                  <label>
                    API base URL (optional)
                    <input
                      value={props.anthropicApiBaseUrl}
                      onChange={(event) => props.onAnthropicApiBaseUrlChange(event.target.value)}
                      placeholder="https://api.anthropic.com"
                    />
                  </label>
                </div>
              ) : null}

              {props.providerPath === "openai" ? (
                <div className="mc-onboarding-openai-block">
                  <div className="mc-onboarding-field-grid">
                    <label>
                      Profile name
                      <input
                        value={props.openAiDisplayName}
                        onChange={(event) => props.onOpenAiDisplayNameChange(event.target.value)}
                        placeholder="openai-primary"
                      />
                    </label>
                    <label>
                      Client ID (optional)
                      <input
                        value={props.openAiClientId}
                        onChange={(event) => props.onOpenAiClientIdChange(event.target.value)}
                        placeholder="Optional override"
                      />
                    </label>
                    <label>
                      API base URL (optional)
                      <input
                        value={props.openAiApiBaseUrl}
                        onChange={(event) => props.onOpenAiApiBaseUrlChange(event.target.value)}
                        placeholder="https://api.openai.com"
                      />
                    </label>
                  </div>
                  <div className="mc-onboarding-inline-actions">
                    <button
                      type="button"
                      className="ghost"
                      disabled={props.busy}
                      onClick={() => {
                        if (props.busy) {
                          return;
                        }
                        void props.onStartOpenAiOauthFlow();
                      }}
                    >
                      Start OAuth
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      disabled={props.busy || !props.openAiSessionId}
                      onClick={() => {
                        if (props.busy) {
                          return;
                        }
                        void props.onFinishOpenAiOauthFlow();
                      }}
                    >
                      Finish OAuth
                    </button>
                  </div>
                  {props.openAiAuthorizeUrl ? (
                    <p className="mc-onboarding-note">
                      Authorize URL: <a href={props.openAiAuthorizeUrl}>{props.openAiAuthorizeUrl}</a>
                    </p>
                  ) : null}
                  {props.openAiCallbackUrlHint ? (
                    <p className="mc-onboarding-note">Callback hint: {props.openAiCallbackUrlHint}</p>
                  ) : null}
                  <div className="mc-onboarding-field-grid">
                    <label>
                      Callback URL (preferred)
                      <input
                        value={props.openAiCallbackUrl}
                        onChange={(event) => props.onOpenAiCallbackUrlChange(event.target.value)}
                        placeholder="https://.../auth/callback?code=...&state=..."
                      />
                    </label>
                    <label>
                      Code (fallback)
                      <input
                        value={props.openAiCode}
                        onChange={(event) => props.onOpenAiCodeChange(event.target.value)}
                        placeholder="OAuth code"
                      />
                    </label>
                    <label>
                      State (fallback)
                      <input
                        value={props.openAiState}
                        onChange={(event) => props.onOpenAiStateChange(event.target.value)}
                        placeholder="OAuth state"
                      />
                    </label>
                  </div>
                </div>
              ) : null}
            </>
          ) : null}
        </>
      )}

      <p className="mc-onboarding-status-row">
        Provider status: <strong>{props.providerReady ? "Ready" : "Not ready"}</strong>
      </p>
    </OnboardingStepShell>
  );
}
