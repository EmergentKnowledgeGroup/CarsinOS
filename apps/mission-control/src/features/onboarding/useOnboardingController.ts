import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createAgent,
  finishOpenAiOauth,
  getAgentProviderProfileOrder,
  getGatewayHealth,
  getGatewayStatus,
  ingestAnthropicSetupToken,
  listAgents,
  listAuthProfiles,
  setAgentProviderProfileOrder,
  startOpenAiOauth,
  updateAgent,
} from "../../lib/api";
import type { MissionControlTab } from "../../app/useAppController";
import type { Agent, AuthProfileResponse, RuntimeConnectionSettings } from "../../types";
import {
  loadDismissedAt,
  type OnboardingMode,
  type OnboardingProviderPath,
  reorderProfileFirst,
  setDismissedNow,
  shouldAutoOpenWizard,
} from "./onboardingState";

export interface OnboardingPreflightState {
  running: boolean;
  ranAtMs: number | null;
  gatewayReachable: boolean | null;
  authValidated: boolean | null;
  canReadCore: boolean | null;
  canManageSetup: boolean | null;
  detail: string;
}

interface UseOnboardingControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
  authProfiles: AuthProfileResponse[];
  saveConnectionFromInputs: (gatewayUrl: string, tokenInput?: string) => Promise<void>;
  loadBaseline: (runtimeSettings?: RuntimeConnectionSettings) => Promise<void>;
  setActiveTab: (tab: MissionControlTab) => void;
}

const ONBOARDING_STEPS = [
  "mode",
  "preflight",
  "connect",
  "agent",
  "provider",
  "routing",
  "review",
  "done",
] as const;

function parseHttpStatusCode(error: unknown): number | null {
  const match = String(error).match(/^(\d{3})\b/);
  if (!match) {
    return null;
  }
  const value = Number.parseInt(match[1], 10);
  return Number.isFinite(value) ? value : null;
}

function providerRequiresProfile(path: OnboardingProviderPath): boolean {
  return path === "anthropic" || path === "openai";
}

function selectedProviderFromExisting(
  profiles: AuthProfileResponse[]
): OnboardingProviderPath {
  if (
    profiles.some(
      (profile) => profile.enabled && profile.provider.toLowerCase() === "anthropic"
    )
  ) {
    return "anthropic";
  }
  if (
    profiles.some(
      (profile) => profile.enabled && profile.provider.toLowerCase() === "openai"
    )
  ) {
    return "openai";
  }
  return "local";
}

export function useOnboardingController(options: UseOnboardingControllerOptions) {
  const {
    settings,
    tokenConfigured,
    agents,
    authProfiles,
    saveConnectionFromInputs,
    loadBaseline,
    setActiveTab,
  } = options;
  const [isOpen, setIsOpen] = useState(false);
  const [stepIndex, setStepIndex] = useState(0);
  const [mode, setMode] = useState<OnboardingMode>("quickstart");
  const [busy, setBusy] = useState(false);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [dismissedAtMs, setDismissedAtMs] = useState<number | null>(loadDismissedAt());

  const [gatewayUrl, setGatewayUrl] = useState(settings.gateway_url || "http://127.0.0.1:18789");
  const [gatewayTokenInput, setGatewayTokenInput] = useState("");
  const [connected, setConnected] = useState(
    tokenConfigured && settings.gateway_url.trim().length > 0
  );

  const [selectedAgentId, setSelectedAgentId] = useState<string>(agents[0]?.agent_id ?? "");
  const [agentIdDraft, setAgentIdDraft] = useState("lyra");
  const [agentNameDraft, setAgentNameDraft] = useState("Lyra");
  const [workspaceRootDraft, setWorkspaceRootDraft] = useState(".");
  const [toolProfileDraft, setToolProfileDraft] = useState("default");
  const [agentReady, setAgentReady] = useState(agents.length > 0);

  const [providerPath, setProviderPath] = useState<OnboardingProviderPath>(
    selectedProviderFromExisting(authProfiles)
  );
  const [useExistingProfile, setUseExistingProfile] = useState(true);
  const [selectedExistingProfileId, setSelectedExistingProfileId] = useState("");
  const [providerProfileId, setProviderProfileId] = useState<string | null>(null);
  const [providerReady, setProviderReady] = useState(false);
  const [localProvider, setLocalProvider] = useState("ollama");
  const [localModelId, setLocalModelId] = useState("local-default");

  const [anthropicDisplayName, setAnthropicDisplayName] = useState("claude-primary");
  const [anthropicSetupToken, setAnthropicSetupToken] = useState("");
  const [anthropicApiBaseUrl, setAnthropicApiBaseUrl] = useState("");

  const [openAiDisplayName, setOpenAiDisplayName] = useState("openai-primary");
  const [openAiClientId, setOpenAiClientId] = useState("");
  const [openAiApiBaseUrl, setOpenAiApiBaseUrl] = useState("");
  const [openAiSessionId, setOpenAiSessionId] = useState("");
  const [openAiAuthorizeUrl, setOpenAiAuthorizeUrl] = useState("");
  const [openAiCallbackUrlHint, setOpenAiCallbackUrlHint] = useState("");
  const [openAiCallbackUrl, setOpenAiCallbackUrl] = useState("");
  const [openAiCode, setOpenAiCode] = useState("");
  const [openAiState, setOpenAiState] = useState("");

  const [routingReady, setRoutingReady] = useState(false);

  const [preflight, setPreflight] = useState<OnboardingPreflightState>({
    running: false,
    ranAtMs: null,
    gatewayReachable: null,
    authValidated: null,
    canReadCore: null,
    canManageSetup: null,
    detail: "Not run yet.",
  });

  const step = ONBOARDING_STEPS[Math.max(0, Math.min(stepIndex, ONBOARDING_STEPS.length - 1))];

  useEffect(() => {
    if (!selectedAgentId && agents.length > 0) {
      setSelectedAgentId(agents[0].agent_id);
    }
  }, [agents, selectedAgentId]);

  useEffect(() => {
    setConnected(tokenConfigured && settings.gateway_url.trim().length > 0);
  }, [settings.gateway_url, tokenConfigured]);

  const existingProviderProfiles = useMemo(() => {
    return authProfiles
      .filter(
        (profile) =>
          profile.enabled && profile.provider.toLowerCase() === providerPath.toLowerCase()
      )
      .sort((a, b) => a.display_name.localeCompare(b.display_name));
  }, [authProfiles, providerPath]);

  useEffect(() => {
    const first = existingProviderProfiles[0]?.auth_profile_id ?? "";
    setSelectedExistingProfileId(first);
    setUseExistingProfile(existingProviderProfiles.length > 0);
    setProviderProfileId(null);
    setProviderReady(false);
    setRoutingReady(false);
    setOpenAiSessionId("");
    setOpenAiAuthorizeUrl("");
    setOpenAiCallbackUrlHint("");
    setOpenAiCallbackUrl("");
    setOpenAiCode("");
    setOpenAiState("");
  }, [existingProviderProfiles, providerPath]);

  useEffect(() => {
    const shouldOpen = shouldAutoOpenWizard(
      {
        settings,
        tokenConfigured,
        agents,
        authProfiles,
      },
      { dismissedAtMs }
    );
    if (shouldOpen) {
      setIsOpen(true);
    }
  }, [agents, authProfiles, dismissedAtMs, settings, tokenConfigured]);

  const clearError = useCallback(() => setErrorText(null), []);

  const openWizard = useCallback(() => {
    setIsOpen(true);
    setStepIndex(0);
    clearError();
  }, [clearError]);

  const dismissWizard = useCallback(() => {
    setDismissedNow();
    setDismissedAtMs(Date.now());
    setIsOpen(false);
  }, []);

  const nextStep = useCallback(() => {
    setStepIndex((value) => Math.min(value + 1, ONBOARDING_STEPS.length - 1));
  }, []);

  const previousStep = useCallback(() => {
    setStepIndex((value) => Math.max(value - 1, 0));
  }, []);

  const runPreflight = useCallback(async () => {
    clearError();
    setPreflight((value) => ({ ...value, running: true, detail: "Running checks..." }));
    const effectiveSettings: RuntimeConnectionSettings = {
      gateway_url: gatewayUrl.trim() || settings.gateway_url,
    };
    try {
      const [healthResult, statusResult, agentsResult, profilesResult] = await Promise.allSettled([
        getGatewayHealth(effectiveSettings),
        getGatewayStatus(effectiveSettings),
        listAgents(effectiveSettings),
        listAuthProfiles(effectiveSettings, { includeDisabled: true }),
      ]);

      const gatewayReachable = healthResult.status === "fulfilled" && healthResult.value.ok !== false;
      const authValidated =
        statusResult.status === "fulfilled" ||
        (statusResult.status === "rejected" &&
          ![401, 403].includes(parseHttpStatusCode(statusResult.reason) ?? 0));
      const canReadCore =
        agentsResult.status === "fulfilled" && profilesResult.status === "fulfilled";

      // Keep preflight non-mutating: write permissions are validated when setup actions execute.
      const canManageSetup: boolean | null = null;

      setPreflight({
        running: false,
        ranAtMs: Date.now(),
        gatewayReachable,
        authValidated,
        canReadCore,
        canManageSetup,
        detail:
          canReadCore
            ? "Checks completed. Setup write permissions are validated during setup actions."
            : "Checks completed. Token cannot read core — setup actions may require operator_admin.",
      });
    } catch (error: unknown) {
      setPreflight({
        running: false,
        ranAtMs: Date.now(),
        gatewayReachable: false,
        authValidated: false,
        canReadCore: false,
        canManageSetup: false,
        detail: `Preflight failed: ${String(error)}`,
      });
      setErrorText(`Preflight failed: ${String(error)}`);
    }
  }, [clearError, gatewayUrl, settings.gateway_url]);

  const setSelectedExistingProfileAndInvalidate = useCallback((value: string) => {
    setSelectedExistingProfileId(value);
    setProviderProfileId(null);
    setProviderReady(false);
    setRoutingReady(false);
  }, []);

  const setUseExistingProfileAndInvalidate = useCallback((value: boolean) => {
    setUseExistingProfile(value);
    setProviderProfileId(null);
    setProviderReady(false);
    setRoutingReady(false);
  }, []);

  const connectGateway = useCallback(async () => {
    clearError();
    setBusy(true);
    try {
      const hasTokenInput = gatewayTokenInput.trim().length > 0;
      await saveConnectionFromInputs(gatewayUrl, gatewayTokenInput);
      const tokenAvailable = hasTokenInput || tokenConfigured;
      if (!tokenAvailable) {
        setConnected(false);
        setErrorText("Connection saved, but no gateway token is configured yet.");
        return;
      }
      setConnected(true);
      setGatewayTokenInput("");
    } catch (error: unknown) {
      setConnected(false);
      setErrorText(`Connection failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [clearError, gatewayTokenInput, gatewayUrl, saveConnectionFromInputs, tokenConfigured]);

  const ensureAgent = useCallback(async () => {
    clearError();
    setBusy(true);
    try {
      if (selectedAgentId.trim()) {
        setAgentReady(true);
        return;
      }
      const created = await createAgent(settings, {
        agent_id: agentIdDraft.trim(),
        name: agentNameDraft.trim(),
        workspace_root: workspaceRootDraft.trim() || ".",
        tool_profile: toolProfileDraft.trim() || "default",
      });
      setSelectedAgentId(created.agent.agent_id);
      await loadBaseline(settings);
      setAgentReady(true);
    } catch (error: unknown) {
      setAgentReady(false);
      setErrorText(`Agent setup failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [
    agentIdDraft,
    agentNameDraft,
    clearError,
    loadBaseline,
    selectedAgentId,
    settings,
    toolProfileDraft,
    workspaceRootDraft,
  ]);

  const startOpenAiOauthFlow = useCallback(async () => {
    clearError();
    setBusy(true);
    try {
      const response = await startOpenAiOauth(settings, {
        display_name: openAiDisplayName.trim() || undefined,
        client_id: openAiClientId.trim() || undefined,
        api_base_url: openAiApiBaseUrl.trim() || undefined,
      });
      setOpenAiSessionId(response.oauth_session_id);
      setOpenAiAuthorizeUrl(response.authorize_url);
      setOpenAiCallbackUrlHint(response.callback_url);
      if (typeof window !== "undefined") {
        window.open(response.authorize_url, "_blank", "noopener,noreferrer");
      }
    } catch (error: unknown) {
      setErrorText(`OpenAI OAuth start failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [clearError, openAiApiBaseUrl, openAiClientId, openAiDisplayName, settings]);

  const finishOpenAiOauthFlow = useCallback(async () => {
    clearError();
    if (!openAiSessionId) {
      setErrorText("Start OAuth first.");
      return;
    }
    setBusy(true);
    try {
      const response = await finishOpenAiOauth(settings, {
        oauth_session_id: openAiSessionId,
        callback_url: openAiCallbackUrl.trim() || undefined,
        code: openAiCode.trim() || undefined,
        state: openAiState.trim() || undefined,
        display_name: openAiDisplayName.trim() || undefined,
        api_base_url: openAiApiBaseUrl.trim() || undefined,
      });
      setProviderProfileId(response.profile.auth_profile_id);
      setProviderReady(true);
      await loadBaseline(settings);
    } catch (error: unknown) {
      setProviderReady(false);
      setErrorText(`OpenAI OAuth finish failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [
    clearError,
    loadBaseline,
    openAiApiBaseUrl,
    openAiCallbackUrl,
    openAiCode,
    openAiDisplayName,
    openAiSessionId,
    openAiState,
    settings,
  ]);

  const completeProvider = useCallback(async () => {
    clearError();
    setBusy(true);
    try {
      if (providerPath === "local") {
        const targetAgent = selectedAgentId.trim();
        if (!targetAgent) {
          throw new Error("Select or create an agent first.");
        }
        await updateAgent(settings, targetAgent, {
          model_provider: localProvider.trim(),
          model_id: localModelId.trim() || "local-default",
        });
        await loadBaseline(settings);
        setProviderReady(true);
        setProviderProfileId(null);
        return;
      }

      if (useExistingProfile) {
        if (!selectedExistingProfileId.trim()) {
          throw new Error("Select an existing profile or create a new one.");
        }
        setProviderProfileId(selectedExistingProfileId.trim());
        setProviderReady(true);
        return;
      }

      if (providerPath === "anthropic") {
        const response = await ingestAnthropicSetupToken(settings, {
          display_name: anthropicDisplayName.trim() || "claude-primary",
          setup_token: anthropicSetupToken.trim(),
          api_base_url: anthropicApiBaseUrl.trim() || undefined,
          enabled: true,
        });
        setProviderProfileId(response.profile.auth_profile_id);
        setProviderReady(true);
        setAnthropicSetupToken("");
        await loadBaseline(settings);
        return;
      }

      if (providerPath === "openai") {
        if (!providerProfileId) {
          throw new Error(
            "Complete OpenAI OAuth first, or switch to an existing profile."
          );
        }
        setProviderReady(true);
      }
    } catch (error: unknown) {
      setProviderReady(false);
      setErrorText(`Provider setup failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [
    anthropicApiBaseUrl,
    anthropicDisplayName,
    anthropicSetupToken,
    clearError,
    loadBaseline,
    localModelId,
    localProvider,
    providerPath,
    providerProfileId,
    selectedAgentId,
    selectedExistingProfileId,
    settings,
    useExistingProfile,
  ]);

  const applyRouting = useCallback(async () => {
    clearError();
    setBusy(true);
    try {
      const targetAgent = selectedAgentId.trim();
      if (!targetAgent) {
        throw new Error("Select an agent first.");
      }
      if (!providerRequiresProfile(providerPath)) {
        setRoutingReady(true);
        return;
      }
      const profileId = providerProfileId?.trim();
      if (!profileId) {
        throw new Error("Provider profile is not ready.");
      }
      const existing = await getAgentProviderProfileOrder(settings, targetAgent, providerPath);
      const nextOrder = reorderProfileFirst(existing.profile_ids, profileId);
      await setAgentProviderProfileOrder(settings, targetAgent, providerPath, nextOrder);
      setRoutingReady(true);
      await loadBaseline(settings);
    } catch (error: unknown) {
      setRoutingReady(false);
      setErrorText(`Routing update failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [clearError, loadBaseline, providerPath, providerProfileId, selectedAgentId, settings]);

  const completeAndExit = useCallback(() => {
    setActiveTab("boards");
    setIsOpen(false);
  }, [setActiveTab]);

  const canFinishReview = connected && agentReady && providerReady && routingReady;

  return {
    isOpen,
    openWizard,
    dismissWizard,
    step,
    stepIndex,
    steps: ONBOARDING_STEPS,
    nextStep,
    previousStep,
    mode,
    setMode,
    busy,
    errorText,
    clearError,
    preflight,
    runPreflight,
    gatewayUrl,
    setGatewayUrl,
    gatewayTokenInput,
    setGatewayTokenInput,
    connected,
    connectGateway,
    selectedAgentId,
    setSelectedAgentId,
    agentIdDraft,
    setAgentIdDraft,
    agentNameDraft,
    setAgentNameDraft,
    workspaceRootDraft,
    setWorkspaceRootDraft,
    toolProfileDraft,
    setToolProfileDraft,
    agentReady,
    ensureAgent,
    providerPath,
    setProviderPath,
    useExistingProfile,
    setUseExistingProfile: setUseExistingProfileAndInvalidate,
    existingProviderProfiles,
    selectedExistingProfileId,
    setSelectedExistingProfileId: setSelectedExistingProfileAndInvalidate,
    providerProfileId,
    providerReady,
    localProvider,
    setLocalProvider,
    localModelId,
    setLocalModelId,
    anthropicDisplayName,
    setAnthropicDisplayName,
    anthropicSetupToken,
    setAnthropicSetupToken,
    anthropicApiBaseUrl,
    setAnthropicApiBaseUrl,
    openAiDisplayName,
    setOpenAiDisplayName,
    openAiClientId,
    setOpenAiClientId,
    openAiApiBaseUrl,
    setOpenAiApiBaseUrl,
    openAiSessionId,
    openAiAuthorizeUrl,
    openAiCallbackUrlHint,
    openAiCallbackUrl,
    setOpenAiCallbackUrl,
    openAiCode,
    setOpenAiCode,
    openAiState,
    setOpenAiState,
    startOpenAiOauthFlow,
    finishOpenAiOauthFlow,
    completeProvider,
    routingReady,
    applyRouting,
    canFinishReview,
    completeAndExit,
  };
}
