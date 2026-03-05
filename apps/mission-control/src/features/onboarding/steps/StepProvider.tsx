import type { Agent, AuthProfileResponse } from "../../../types";
import { OnboardingStepShell } from "../OnboardingStepShell";
import type {
  OnboardingAnthropicAuthMode,
  OnboardingProviderPath,
} from "../onboardingState";

interface StepProviderProps {
  busy: boolean;
  agents: Agent[];
  selectedAgentId: string;
  agentIdDraft: string;
  agentNameDraft: string;
  workspaceRootDraft: string;
  toolProfileDraft: string;
  agentReady: boolean;
  providerPath: OnboardingProviderPath;
  useExistingProfile: boolean;
  existingProviderProfiles: AuthProfileResponse[];
  selectedExistingProfileId: string;
  providerReady: boolean;
  routingReady: boolean;
  localProvider: string;
  localUseConnectionProfile: boolean;
  localConnectionProfileName: string;
  localApiBaseUrl: string;
  localApiKey: string;
  localModelId: string;
  localOrchestratorEnabled: boolean;
  localOrchestratorAgentId: string;
  localOrchestratorAgentName: string;
  localOrchestratorModelId: string;
  localModelDiscoveryNote: string | null;
  localProviderOptions: Array<{ value: string; label: string }>;
  localModelOptions: string[];
  localModelsLoading: boolean;
  localModelsError: string | null;
  anthropicAuthMode: OnboardingAnthropicAuthMode;
  anthropicDisplayName: string;
  anthropicSetupToken: string;
  anthropicSetupLaunchNote: string | null;
  anthropicValidationBusy: boolean;
  anthropicValidationNote: string | null;
  anthropicApiBaseUrl: string;
  anthropicAccessToken: string;
  anthropicRefreshToken: string;
  anthropicRefreshUrl: string;
  anthropicExpiresAtUnix: string;
  anthropicHeadlessCommand: string;
  anthropicHeadlessArgs: string;
  openAiDisplayName: string;
  openAiClientId: string;
  openAiApiBaseUrl: string;
  openAiSessionId: string;
  openAiAuthorizeUrl: string;
  openAiCallbackUrlHint: string;
  openAiCallbackUrl: string;
  openAiCode: string;
  openAiState: string;
  onSelectedAgentIdChange: (value: string) => void;
  onAgentIdDraftChange: (value: string) => void;
  onAgentNameDraftChange: (value: string) => void;
  onWorkspaceRootDraftChange: (value: string) => void;
  onToolProfileDraftChange: (value: string) => void;
  onCreateNewAgentDraft: () => void;
  onSaveAgent: () => Promise<boolean>;
  onDeleteSelectedAgent: () => Promise<boolean>;
  onProviderPathChange: (value: OnboardingProviderPath) => void;
  onUseExistingProfileChange: (value: boolean) => void;
  onSelectedExistingProfileIdChange: (value: string) => void;
  onLocalProviderChange: (value: string) => void;
  onLocalUseConnectionProfileChange: (value: boolean) => void;
  onLocalConnectionProfileNameChange: (value: string) => void;
  onLocalApiBaseUrlChange: (value: string) => void;
  onLocalApiKeyChange: (value: string) => void;
  onLocalModelIdChange: (value: string) => void;
  onLocalOrchestratorEnabledChange: (value: boolean) => void;
  onLocalOrchestratorAgentIdChange: (value: string) => void;
  onLocalOrchestratorAgentNameChange: (value: string) => void;
  onLocalOrchestratorModelIdChange: (value: string) => void;
  onRefreshLocalModels: () => Promise<void>;
  onAnthropicAuthModeChange: (value: OnboardingAnthropicAuthMode) => void;
  onAnthropicDisplayNameChange: (value: string) => void;
  onAnthropicSetupTokenChange: (value: string) => void;
  onLaunchAnthropicSetupTokenFlow: () => Promise<void>;
  onValidateAnthropicSetupToken: () => Promise<void>;
  onAnthropicApiBaseUrlChange: (value: string) => void;
  onAnthropicAccessTokenChange: (value: string) => void;
  onAnthropicRefreshTokenChange: (value: string) => void;
  onAnthropicRefreshUrlChange: (value: string) => void;
  onAnthropicExpiresAtUnixChange: (value: string) => void;
  onAnthropicHeadlessCommandChange: (value: string) => void;
  onAnthropicHeadlessArgsChange: (value: string) => void;
  onOpenAiDisplayNameChange: (value: string) => void;
  onOpenAiClientIdChange: (value: string) => void;
  onOpenAiApiBaseUrlChange: (value: string) => void;
  onOpenAiCallbackUrlChange: (value: string) => void;
  onOpenAiCodeChange: (value: string) => void;
  onOpenAiStateChange: (value: string) => void;
  onStartOpenAiOauthFlow: () => Promise<void>;
  onFinishOpenAiOauthFlow: () => Promise<void>;
  onReauthSelectedProfile: () => Promise<boolean>;
  onBack: () => void;
  onNext: () => void | Promise<void>;
}

export function StepProvider(props: StepProviderProps) {
  const hasExistingProfiles = props.existingProviderProfiles.length > 0;
  const canLaunchAnthropicCliAuth =
    props.anthropicAuthMode === "api_key" ||
    props.anthropicAuthMode === "claude_consumer_oauth";
  return (
    <OnboardingStepShell
      stepLabel="Step 4 of 6"
      title="Configure Agents + Providers"
      subtitle="Create or edit agents, then attach provider auth and routing in one place."
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button
            type="button"
            disabled={props.busy}
            onClick={() => {
              if (props.busy) {
                return;
              }
              void props.onNext();
            }}
          >
            {props.busy ? "Applying..." : "Continue"}
          </button>
        </>
      }
    >
      <fieldset disabled={props.busy} style={{ border: "none", margin: 0, minWidth: 0, padding: 0 }}>
        <div className="mc-onboarding-openai-block">
          <div className="mc-onboarding-inline-actions">
            <button
              type="button"
              className="ghost"
              disabled={props.busy}
              onClick={() => {
                if (props.busy) {
                  return;
                }
                props.onCreateNewAgentDraft();
              }}
            >
              New agent draft
            </button>
            <button
              type="button"
              className="ghost"
              disabled={props.busy}
              onClick={() => {
                if (props.busy) {
                  return;
                }
                void props.onSaveAgent();
              }}
            >
              {props.busy ? "Saving..." : "Save agent"}
            </button>
            <button
              type="button"
              className="ghost"
              disabled={props.busy || !props.selectedAgentId}
              onClick={() => {
                if (props.busy || !props.selectedAgentId) {
                  return;
                }
                void props.onDeleteSelectedAgent();
              }}
            >
              Delete selected
            </button>
          </div>

          {props.agents.length > 0 ? (
            <label>
              Existing agents
              <select
                value={props.selectedAgentId}
                onChange={(event) => {
                  const value = event.target.value;
                  if (!value) {
                    props.onCreateNewAgentDraft();
                    return;
                  }
                  props.onSelectedAgentIdChange(value);
                }}
              >
                <option value="">Create new agent...</option>
                {props.agents.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name} ({agent.agent_id})
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <p className="mc-onboarding-note">
              No agents are configured yet. Add your first assistant agent below.
            </p>
          )}

          <div className="mc-onboarding-field-grid">
            <label>
              Agent ID
              <input
                value={props.agentIdDraft}
                onChange={(event) => props.onAgentIdDraftChange(event.target.value)}
                placeholder="assistant-1"
              />
            </label>
            <label>
              Agent name
              <input
                value={props.agentNameDraft}
                onChange={(event) => props.onAgentNameDraftChange(event.target.value)}
                placeholder="Assistant"
              />
            </label>
            <label>
              Workspace root
              <input
                value={props.workspaceRootDraft}
                onChange={(event) => props.onWorkspaceRootDraftChange(event.target.value)}
                placeholder="."
              />
            </label>
            <label>
              Tool profile
              <input
                value={props.toolProfileDraft}
                onChange={(event) => props.onToolProfileDraftChange(event.target.value)}
                placeholder="default"
              />
            </label>
          </div>
        </div>

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
              <p>Choose API key, consumer OAuth, or Claude Code headless profile mode.</p>
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
          <div className="mc-onboarding-openai-block">
            <div className="mc-onboarding-field-grid">
              <label>
                Local provider
                <select
                  value={props.localProvider}
                  onChange={(event) => props.onLocalProviderChange(event.target.value)}
                >
                  {props.localProviderOptions.length === 0 ? (
                    <option value={props.localProvider}>{props.localProvider}</option>
                  ) : null}
                  {props.localProviderOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <label className="mc-checkbox">
              <input
                type="checkbox"
                checked={props.localUseConnectionProfile}
                onChange={(event) =>
                  props.onLocalUseConnectionProfileChange(event.target.checked)
                }
              />
              Save a connection profile for endpoint/token routing
            </label>

            {props.localUseConnectionProfile ? (
              <div className="mc-onboarding-field-grid">
                <label>
                  Profile name
                  <input
                    value={props.localConnectionProfileName}
                    onChange={(event) =>
                      props.onLocalConnectionProfileNameChange(event.target.value)
                    }
                    placeholder="lmstudio-local"
                  />
                </label>
                <label>
                  API base URL (optional)
                  <input
                    value={props.localApiBaseUrl}
                    onChange={(event) => props.onLocalApiBaseUrlChange(event.target.value)}
                    placeholder="http://127.0.0.1:1234"
                  />
                </label>
                <label>
                  API key (optional)
                  <input
                    type="text"
                    autoComplete="off"
                    autoCapitalize="none"
                    autoCorrect="off"
                    spellCheck={false}
                    value={props.localApiKey}
                    onChange={(event) => props.onLocalApiKeyChange(event.target.value)}
                    placeholder="Bearer token if required"
                  />
                </label>
              </div>
            ) : null}

            <div className="mc-onboarding-inline-actions">
              <button
                type="button"
                className="ghost"
                disabled={props.busy || props.localModelsLoading}
                onClick={() => {
                  if (props.busy || props.localModelsLoading) {
                    return;
                  }
                  void props.onRefreshLocalModels();
                }}
              >
                {props.localModelsLoading ? "Scanning..." : "Scan loaded models"}
              </button>
            </div>

            {props.localModelDiscoveryNote ? (
              <p className="mc-onboarding-note">{props.localModelDiscoveryNote}</p>
            ) : null}
            {props.localModelsError ? (
              <p className="mc-onboarding-note">
                Model discovery is unavailable. You can still paste model IDs manually.
              </p>
            ) : null}

            <div className="mc-onboarding-field-grid">
              <label>
                Assistant model
                <select
                  value={props.localModelId}
                  onChange={(event) => props.onLocalModelIdChange(event.target.value)}
                >
                  <option value="">Select model...</option>
                  {props.localModelOptions.map((modelId) => (
                    <option key={modelId} value={modelId}>
                      {modelId}
                    </option>
                  ))}
                </select>
                <input
                  value={props.localModelId}
                  onChange={(event) => props.onLocalModelIdChange(event.target.value)}
                  placeholder="Or paste assistant model ID manually"
                />
              </label>
            </div>

            <label className="mc-checkbox">
              <input
                type="checkbox"
                checked={props.localOrchestratorEnabled}
                onChange={(event) =>
                  props.onLocalOrchestratorEnabledChange(event.target.checked)
                }
              />
              Also configure a dedicated local orchestrator worker
            </label>

            {props.localOrchestratorEnabled ? (
              <div className="mc-onboarding-field-grid">
                <label>
                  Orchestrator agent ID
                  <input
                    value={props.localOrchestratorAgentId}
                    onChange={(event) =>
                      props.onLocalOrchestratorAgentIdChange(event.target.value)
                    }
                    placeholder="orchestrator"
                  />
                </label>
                <label>
                  Orchestrator name
                  <input
                    value={props.localOrchestratorAgentName}
                    onChange={(event) =>
                      props.onLocalOrchestratorAgentNameChange(event.target.value)
                    }
                    placeholder="Orchestrator"
                  />
                </label>
                <label>
                  Orchestrator model
                  <select
                    value={props.localOrchestratorModelId}
                    onChange={(event) =>
                      props.onLocalOrchestratorModelIdChange(event.target.value)
                    }
                  >
                    <option value="">Use assistant model</option>
                    {props.localModelOptions.map((modelId) => (
                      <option key={`orchestrator-${modelId}`} value={modelId}>
                        {modelId}
                      </option>
                    ))}
                  </select>
                  <input
                    value={props.localOrchestratorModelId}
                    onChange={(event) =>
                      props.onLocalOrchestratorModelIdChange(event.target.value)
                    }
                    placeholder="Or paste orchestrator model ID manually"
                  />
                </label>
              </div>
            ) : null}
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
              <>
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
                <div className="mc-onboarding-inline-actions">
                  <button
                    type="button"
                    className="ghost"
                    disabled={props.busy || !props.selectedExistingProfileId}
                    onClick={() => {
                      if (props.busy || !props.selectedExistingProfileId) {
                        return;
                      }
                      void props.onReauthSelectedProfile();
                    }}
                  >
                    Reauth selected profile
                  </button>
                </div>
              </>
            ) : null}

            {!props.useExistingProfile || !hasExistingProfiles ? (
              <>
                {props.providerPath === "anthropic" ? (
                  <div className="mc-onboarding-openai-block">
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
                        Auth method
                        <select
                          value={props.anthropicAuthMode}
                          onChange={(event) =>
                            props.onAnthropicAuthModeChange(
                              event.target.value as OnboardingAnthropicAuthMode
                            )
                          }
                        >
                          <option value="api_key">API key (setup token ingest)</option>
                          <option value="claude_consumer_oauth">
                            OAuth token (consumer account, high risk)
                          </option>
                          <option value="agent_sdk">
                            Claude Code headless profile (high risk)
                          </option>
                        </select>
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

                    {canLaunchAnthropicCliAuth ? (
                      <>
                        <div className="mc-onboarding-inline-actions">
                          <button
                            type="button"
                            className="ghost"
                            disabled={props.busy}
                            onClick={() => {
                              if (props.busy) {
                                return;
                              }
                              void props.onLaunchAnthropicSetupTokenFlow();
                            }}
                          >
                            {props.busy ? "Opening..." : "Open CLI + auth"}
                          </button>
                        </div>
                        <p className="mc-onboarding-note">
                          This opens Terminal and runs <code>claude setup-token</code>.
                          After sign-in, copy the token and paste it into the field below.
                        </p>
                        {props.anthropicSetupLaunchNote ? (
                          <p className="mc-onboarding-note">{props.anthropicSetupLaunchNote}</p>
                        ) : null}
                      </>
                    ) : null}

                    {props.anthropicAuthMode === "api_key" ? (
                      <>
                        <div className="mc-onboarding-field-grid">
                          <label>
                            Setup token
                            <input
                              type="text"
                              autoComplete="off"
                              autoCapitalize="none"
                              autoCorrect="off"
                              spellCheck={false}
                              value={props.anthropicSetupToken}
                              onChange={(event) =>
                                props.onAnthropicSetupTokenChange(event.target.value)
                              }
                              placeholder="Paste setup token"
                            />
                          </label>
                        </div>
                        <div className="mc-onboarding-inline-actions">
                          <button
                            type="button"
                            className="ghost"
                            disabled={props.busy || props.anthropicValidationBusy}
                            onClick={() => {
                              if (props.busy || props.anthropicValidationBusy) {
                                return;
                              }
                              void props.onValidateAnthropicSetupToken();
                            }}
                          >
                            {props.anthropicValidationBusy ? "Validating..." : "Validate key"}
                          </button>
                        </div>
                        {props.anthropicValidationNote ? (
                          <p className="mc-onboarding-note">{props.anthropicValidationNote}</p>
                        ) : null}
                      </>
                    ) : null}

                    {props.anthropicAuthMode !== "api_key" ? (
                      <>
                        <div className="mc-onboarding-risk-note">
                          High-risk mode: this path requires audit logs and kill-switch controls.
                          Use only if you understand provider policy risk.
                        </div>
                        <div className="mc-onboarding-field-grid">
                          <label>
                            Access token
                            <input
                              type="text"
                              autoComplete="off"
                              autoCapitalize="none"
                              autoCorrect="off"
                              spellCheck={false}
                              value={props.anthropicAccessToken}
                              onChange={(event) =>
                                props.onAnthropicAccessTokenChange(event.target.value)
                              }
                              placeholder="Paste access token"
                            />
                          </label>
                          <label>
                            Refresh token (optional)
                            <input
                              type="text"
                              autoComplete="off"
                              autoCapitalize="none"
                              autoCorrect="off"
                              spellCheck={false}
                              value={props.anthropicRefreshToken}
                              onChange={(event) =>
                                props.onAnthropicRefreshTokenChange(event.target.value)
                              }
                              placeholder="Optional refresh token"
                            />
                          </label>
                          <label>
                            Refresh URL (optional)
                            <input
                              value={props.anthropicRefreshUrl}
                              onChange={(event) =>
                                props.onAnthropicRefreshUrlChange(event.target.value)
                              }
                              placeholder="https://.../oauth/token"
                            />
                          </label>
                          <label>
                            Expires at (unix seconds, optional)
                            <input
                              value={props.anthropicExpiresAtUnix}
                              onChange={(event) =>
                                props.onAnthropicExpiresAtUnixChange(event.target.value)
                              }
                              placeholder="1735689600"
                            />
                          </label>
                        </div>
                      </>
                    ) : null}

                    {props.anthropicAuthMode === "agent_sdk" ? (
                      <div className="mc-onboarding-field-grid">
                        <label>
                          Claude CLI command
                          <input
                            value={props.anthropicHeadlessCommand}
                            onChange={(event) =>
                              props.onAnthropicHeadlessCommandChange(event.target.value)
                            }
                            placeholder="claude"
                          />
                        </label>
                        <label>
                          CLI args (optional)
                          <input
                            value={props.anthropicHeadlessArgs}
                            onChange={(event) =>
                              props.onAnthropicHeadlessArgsChange(event.target.value)
                            }
                            placeholder="-p --output-format text"
                          />
                        </label>
                      </div>
                    ) : null}
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
      </fieldset>

      <p className="mc-onboarding-status-row">
        Agent status: <strong>{props.agentReady ? "Ready" : "Not ready"}</strong> · Provider
        status: <strong>{props.providerReady ? "Ready" : "Not ready"}</strong> · Routing status:{" "}
        <strong>{props.routingReady ? "Ready" : "Not ready"}</strong>
      </p>
    </OnboardingStepShell>
  );
}
