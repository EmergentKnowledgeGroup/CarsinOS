import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  createAuthProfile,
  createAgent,
  finishOpenAiOauth,
  getAgentProviderProfileOrder,
  getGatewayHealth,
  getGatewayStatus,
  ingestAnthropicSetupToken,
  listAgents,
  listAuthProfiles,
  listProviderCapabilities,
  listProviderModels,
  removeAgent,
  revokeAuthProfile,
  setAgentProviderProfileOrder,
  startOpenAiOauth,
  updateAgent,
  validateAnthropicSetupToken,
} from "../../lib/api";
import {
  localProviderCapabilities,
  providerLabel,
} from "../../lib/providerCatalog";
import type { MissionControlTab } from "../../app/useAppController";
import type {
  Agent,
  AuthProfileResponse,
  BootstrapPresetResponse,
  RuntimeConnectionSettings,
} from "../../types";
import { applyBootstrapPresetToDraft } from "../strategy/bootstrapPresetUtils";
import {
  loadDismissedAt,
  type OnboardingAnthropicAuthMode,
  type OnboardingMode,
  type OnboardingProviderPath,
  reorderProfileFirst,
  setDismissedNow,
  shouldAutoOpenWizard,
} from "./onboardingState";
import { DEFAULT_GATEWAY_URL } from "../../constants";
import { launchAnthropicSetupTokenFlow as launchAnthropicSetupTokenFlowRuntime } from "../../lib/runtime";

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
  strategyEnabled: boolean;
  bootstrapPresets: BootstrapPresetResponse[];
  saveConnectionFromInputs: (gatewayUrl: string, tokenInput?: string) => Promise<void>;
  loadBaseline: (runtimeSettings?: RuntimeConnectionSettings) => Promise<void>;
  setActiveTab: (tab: MissionControlTab) => void;
}

const ONBOARDING_STEPS = [
  "mode",
  "preflight",
  "connect",
  "provider",
  "review",
  "done",
] as const;

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

function parseOptionalUnixTimestamp(raw: string): number | undefined {
  const trimmed = raw.trim();
  if (!trimmed) {
    return undefined;
  }
  if (!/^\d+$/.test(trimmed)) {
    throw new Error("Expiry must be a unix timestamp in seconds.");
  }
  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error("Expiry must be a unix timestamp in seconds.");
  }
  return parsed;
}

export function useOnboardingController(options: UseOnboardingControllerOptions) {
  const {
    settings,
    tokenConfigured,
    agents,
    authProfiles,
    strategyEnabled,
    bootstrapPresets,
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

  const initialGatewayUrl = settings.gateway_url.trim() || DEFAULT_GATEWAY_URL;
  const [gatewayUrl, setGatewayUrl] = useState(initialGatewayUrl);
  const [gatewayTokenInput, setGatewayTokenInput] = useState("");
  const [connected, setConnected] = useState(
    tokenConfigured && settings.gateway_url.trim().length > 0
  );

  const [selectedAgentId, setSelectedAgentId] = useState<string>(agents[0]?.agent_id ?? "");
  const [agentIdDraft, setAgentIdDraft] = useState(agents[0]?.agent_id ?? "");
  const [agentNameDraft, setAgentNameDraft] = useState(agents[0]?.name ?? "");
  const [workspaceRootDraft, setWorkspaceRootDraft] = useState(
    agents[0]?.workspace_root?.trim() || "."
  );
  const [toolProfileDraft, setToolProfileDraft] = useState(
    agents[0]?.tool_profile?.trim() || "default"
  );
  const [reportsToAgentIdDraft, setReportsToAgentIdDraft] = useState(
    agents[0]?.reports_to_agent_id?.trim() || ""
  );
  const [roleLabelDraft, setRoleLabelDraft] = useState(
    agents[0]?.role_label?.trim() || ""
  );
  const [selectedPresetKey, setSelectedPresetKey] = useState("");
  const [agentReady, setAgentReady] = useState(agents.length > 0);

  const [providerPath, setProviderPath] = useState<OnboardingProviderPath>(
    selectedProviderFromExisting(authProfiles)
  );
  const [useExistingProfile, setUseExistingProfile] = useState(true);
  const [selectedExistingProfileId, setSelectedExistingProfileId] = useState("");
  const [providerProfileId, setProviderProfileId] = useState<string | null>(null);
  const [providerReady, setProviderReady] = useState(false);
  const [localProvider, setLocalProvider] = useState("ollama");
  const [localUseConnectionProfile, setLocalUseConnectionProfile] = useState(false);
  const [localConnectionProfileName, setLocalConnectionProfileName] = useState("local-primary");
  const [localApiBaseUrl, setLocalApiBaseUrl] = useState("");
  const [localApiKey, setLocalApiKey] = useState("");
  const [localConnectionProfileId, setLocalConnectionProfileId] = useState<string | null>(null);
  const [localModelId, setLocalModelId] = useState("");
  const [localOrchestratorEnabled, setLocalOrchestratorEnabled] = useState(false);
  const [localOrchestratorAgentId, setLocalOrchestratorAgentId] = useState("orchestrator");
  const [localOrchestratorAgentName, setLocalOrchestratorAgentName] = useState("Orchestrator");
  const [localOrchestratorModelId, setLocalOrchestratorModelId] = useState("");
  const [localModelDiscoveryNote, setLocalModelDiscoveryNote] = useState<string | null>(null);
  const [localProviderOptions, setLocalProviderOptions] = useState<
    Array<{ value: string; label: string }>
  >([
    { value: "ollama", label: providerLabel("ollama") },
    { value: "lmstudio", label: providerLabel("lmstudio") },
    { value: "vllm", label: providerLabel("vllm") },
    { value: "mock", label: providerLabel("mock") },
  ]);
  const [localModelOptions, setLocalModelOptions] = useState<string[]>([]);
  const [localModelsLoading, setLocalModelsLoading] = useState(false);
  const [localModelsError, setLocalModelsError] = useState<string | null>(null);

  const [anthropicAuthMode, setAnthropicAuthMode] =
    useState<OnboardingAnthropicAuthMode>("api_key");
  const [anthropicDisplayName, setAnthropicDisplayName] = useState("claude-primary");
  const [anthropicSetupToken, setAnthropicSetupToken] = useState("");
  const [anthropicSetupLaunchNote, setAnthropicSetupLaunchNote] = useState<string | null>(null);
  const [anthropicValidationBusy, setAnthropicValidationBusy] = useState(false);
  const [anthropicValidationNote, setAnthropicValidationNote] = useState<string | null>(null);
  const [anthropicApiBaseUrl, setAnthropicApiBaseUrl] = useState("");
  const [anthropicAccessToken, setAnthropicAccessToken] = useState("");
  const [anthropicRefreshToken, setAnthropicRefreshToken] = useState("");
  const [anthropicRefreshUrl, setAnthropicRefreshUrl] = useState("");
  const [anthropicExpiresAtUnix, setAnthropicExpiresAtUnix] = useState("");
  const [anthropicHeadlessCommand, setAnthropicHeadlessCommand] = useState("claude");
  const [anthropicHeadlessArgs, setAnthropicHeadlessArgs] = useState(
    "-p --output-format text"
  );

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
  const localProviderRef = useRef(localProvider);
  const localModelIdRef = useRef(localModelId);
  const nextStepInFlightRef = useRef(false);

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
    localProviderRef.current = localProvider;
  }, [localProvider]);

  useEffect(() => {
    setLocalConnectionProfileId(null);
  }, [localProvider]);

  useEffect(() => {
    localModelIdRef.current = localModelId;
  }, [localModelId]);

  useEffect(() => {
    if (agents.length === 0) {
      setSelectedAgentId("");
      setAgentReady(false);
      return;
    }
    setSelectedAgentId((current) => {
      if (current && agents.some((agent) => agent.agent_id === current)) {
        return current;
      }
      return agents[0].agent_id;
    });
  }, [agents]);

  useEffect(() => {
    if (!selectedAgentId) {
      return;
    }
    const selected = agents.find((agent) => agent.agent_id === selectedAgentId);
    if (!selected) {
      return;
    }
    setAgentIdDraft(selected.agent_id);
    setAgentNameDraft(selected.name);
    setWorkspaceRootDraft(selected.workspace_root?.trim() || ".");
    setToolProfileDraft(selected.tool_profile?.trim() || "default");
    setReportsToAgentIdDraft(selected.reports_to_agent_id?.trim() || "");
    setRoleLabelDraft(selected.role_label?.trim() || "");
    setAgentReady(true);
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
  }, [existingProviderProfiles]);

  useEffect(() => {
    setProviderProfileId(null);
    setProviderReady(false);
    setRoutingReady(false);
    setLocalModelDiscoveryNote(null);
    setLocalConnectionProfileId(null);
    setOpenAiSessionId("");
    setOpenAiAuthorizeUrl("");
    setOpenAiCallbackUrlHint("");
    setOpenAiCallbackUrl("");
    setOpenAiCode("");
    setOpenAiState("");
    setAnthropicValidationNote(null);
  }, [providerPath]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const fallbackLocalProvider = localProviderRef.current;
    let cancelled = false;
    void listProviderCapabilities(settings)
      .then((response) => {
        if (cancelled) {
          return;
        }
        const options = localProviderCapabilities(response.items)
          .map((item) => ({
            value: item.provider,
            label: providerLabel(item.provider),
          }))
          .sort((left, right) => left.label.localeCompare(right.label));
        if (options.length === 0) {
          return;
        }
        setLocalProviderOptions(options);
        setLocalProvider((current) => {
          if (options.some((item) => item.value === current)) {
            return current;
          }
          return options[0].value;
        });
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        setLocalProviderOptions((previous) =>
          previous.length > 0
            ? previous
            : [{ value: fallbackLocalProvider, label: providerLabel(fallbackLocalProvider) }]
        );
      });
    return () => {
      cancelled = true;
    };
  }, [isOpen, settings]);

  const refreshLocalModels = useCallback(async () => {
    const provider = localProvider.trim().toLowerCase();
    if (!provider) {
      setLocalModelOptions([]);
      setLocalModelsError("Select a local provider first.");
      setLocalModelDiscoveryNote("Choose a local provider, then scan for models.");
      return;
    }
    setLocalModelsLoading(true);
    setLocalModelsError(null);
    try {
      const response = await listProviderModels(settings, {
        provider,
        agent_id: selectedAgentId || undefined,
        auth_profile_id:
          localUseConnectionProfile && localConnectionProfileId
            ? localConnectionProfileId
            : undefined,
      });
      const models = response.items.map((item) => item.model_id);
      const currentLocalModelId = localModelIdRef.current;
      const assistantModelNext =
        (currentLocalModelId && models.includes(currentLocalModelId)
          ? currentLocalModelId
          : models[0]) ?? "";
      setLocalModelOptions(models);
      setLocalModelId(assistantModelNext);
      setLocalOrchestratorModelId((current) => {
        if (current && models.includes(current)) {
          return current;
        }
        if (models.length === 0) {
          return "";
        }
        const fallback = models.find((modelId) => modelId !== assistantModelNext);
        return fallback ?? models[0];
      });
      if (models.length === 0) {
        setLocalModelDiscoveryNote(
          "No models reported yet. Start your local model server, then click Scan loaded models."
        );
      } else {
        setLocalModelDiscoveryNote(
          `Found ${models.length} model${models.length === 1 ? "" : "s"} from ${providerLabel(provider)}.`
        );
      }
    } catch (err: unknown) {
      setLocalModelOptions([]);
      setLocalModelsError(String(err));
      setLocalModelDiscoveryNote(
        "Model discovery failed. Verify the local provider endpoint is running and reachable."
      );
    } finally {
      setLocalModelsLoading(false);
    }
  }, [
    localConnectionProfileId,
    localProvider,
    localUseConnectionProfile,
    selectedAgentId,
    settings,
  ]);

  useEffect(() => {
    if (!isOpen || providerPath !== "local") {
      return;
    }
    void refreshLocalModels();
  }, [isOpen, providerPath, refreshLocalModels]);

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

  const previousStep = useCallback(() => {
    setStepIndex((value) => Math.max(value - 1, 0));
  }, []);

  const runPreflight = useCallback(async (): Promise<boolean> => {
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
        gatewayReachable && statusResult.status === "fulfilled";
      const canReadCore =
        agentsResult.status === "fulfilled" && profilesResult.status === "fulfilled";

      // Keep preflight non-mutating: write permissions are validated when setup actions execute.
      const canManageSetup: boolean | null = null;

      const passed = gatewayReachable && authValidated && canReadCore;
      setPreflight({
        running: false,
        ranAtMs: Date.now(),
        gatewayReachable,
        authValidated,
        canReadCore,
        canManageSetup,
        detail:
          !gatewayReachable
            ? "Checks completed. Gateway is not reachable at the configured URL."
            : !authValidated
              ? "Checks completed. Gateway rejected token access for status checks."
              : canReadCore
                ? "Checks completed. Setup write permissions are validated during setup actions."
                : "Checks completed. Token cannot read core — setup actions may require operator_admin.",
      });
      return passed;
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
      return false;
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

  const setAnthropicAuthModeAndInvalidate = useCallback(
    (value: OnboardingAnthropicAuthMode) => {
      setAnthropicAuthMode(value);
      setProviderProfileId(null);
      setProviderReady(false);
      setRoutingReady(false);
      setAnthropicSetupLaunchNote(null);
      setAnthropicValidationNote(null);
    },
    []
  );

  const launchAnthropicSetupTokenFlow = useCallback(async () => {
    clearError();
    setBusy(true);
    try {
      const result = await launchAnthropicSetupTokenFlowRuntime();
      if (result.launched) {
        setAnthropicSetupLaunchNote(
          `${result.detail} Copy the token from Terminal and paste it below.`
        );
        return;
      }
      setAnthropicSetupLaunchNote(`${result.detail} Command: ${result.command}`);
    } catch (error: unknown) {
      setErrorText(`Unable to launch setup-token helper: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [clearError]);

  const runAnthropicSetupTokenValidation = useCallback(async () => {
    clearError();
    const setupToken = anthropicSetupToken.trim();
    if (!setupToken) {
      setAnthropicValidationNote("Enter a key first.");
      return;
    }
    setAnthropicValidationBusy(true);
    try {
      await validateAnthropicSetupToken(settings, {
        setup_token: setupToken,
        api_base_url: anthropicApiBaseUrl.trim() || undefined,
      });
      setAnthropicValidationNote("Key verified successfully against Anthropic.");
    } catch (error: unknown) {
      setAnthropicValidationNote(`Validation failed: ${String(error)}`);
    } finally {
      setAnthropicValidationBusy(false);
    }
  }, [anthropicApiBaseUrl, anthropicSetupToken, clearError, settings]);

  const connectGateway = useCallback(async (): Promise<boolean> => {
    clearError();
    setBusy(true);
    try {
      const hasTokenInput = gatewayTokenInput.trim().length > 0;
      await saveConnectionFromInputs(gatewayUrl, gatewayTokenInput);
      const tokenAvailable = hasTokenInput || tokenConfigured || connected;
      if (!tokenAvailable) {
        setConnected(false);
        setErrorText("Connection saved, but no gateway token is configured yet.");
        return false;
      }
      setConnected(true);
      setGatewayTokenInput("");
      return true;
    } catch (error: unknown) {
      setConnected(false);
      setErrorText(`Connection failed: ${String(error)}`);
      return false;
    } finally {
      setBusy(false);
    }
  }, [
    clearError,
    connected,
    gatewayTokenInput,
    gatewayUrl,
    saveConnectionFromInputs,
    tokenConfigured,
  ]);

  const createNewAgentDraft = useCallback(() => {
    clearError();
    setSelectedAgentId("");
    setAgentIdDraft("");
    setAgentNameDraft("");
    setWorkspaceRootDraft(".");
    setToolProfileDraft("default");
    setReportsToAgentIdDraft("");
    setRoleLabelDraft("");
    setSelectedPresetKey("");
    setAgentReady(false);
    setProviderReady(false);
    setRoutingReady(false);
  }, [clearError]);

  const applySelectedPreset = useCallback(() => {
    if (!strategyEnabled || !selectedPresetKey) {
      return;
    }
    const preset = bootstrapPresets.find((item) => item.preset_key === selectedPresetKey);
    if (!preset) {
      return;
    }
    const next = applyBootstrapPresetToDraft(
      {
        preset_key: selectedPresetKey,
        role_label: roleLabelDraft,
        model_provider: localProvider,
        model_id: localModelId,
        tool_profile: toolProfileDraft,
        workspace_root: workspaceRootDraft,
        reports_to_agent_id: reportsToAgentIdDraft,
      },
      preset
    );
    setRoleLabelDraft(next.role_label);
    setToolProfileDraft(next.tool_profile);
    setWorkspaceRootDraft(next.workspace_root);
    setReportsToAgentIdDraft(next.reports_to_agent_id);
    if (next.model_provider) {
      if (next.model_provider === "openai" || next.model_provider === "anthropic") {
        setProviderPath(next.model_provider);
      } else {
        setProviderPath("local");
        setLocalProvider(next.model_provider);
      }
    }
    if (next.model_id) {
      setLocalModelId(next.model_id);
    }
  }, [
    bootstrapPresets,
    localModelId,
    localProvider,
    reportsToAgentIdDraft,
    roleLabelDraft,
    selectedPresetKey,
    strategyEnabled,
    toolProfileDraft,
    workspaceRootDraft,
  ]);

  const saveAgent = useCallback(async (): Promise<boolean> => {
    clearError();
    const desiredAgentId = agentIdDraft.trim().toLowerCase();
    const desiredAgentName = agentNameDraft.trim();
    if (!desiredAgentId) {
      setAgentReady(false);
      setErrorText("Agent ID is required.");
      return false;
    }
    if (!desiredAgentName) {
      setAgentReady(false);
      setErrorText("Agent name is required.");
      return false;
    }
    setBusy(true);
    try {
      const patch = {
        name: desiredAgentName,
        workspace_root: workspaceRootDraft.trim() || ".",
        tool_profile: toolProfileDraft.trim() || "default",
        reports_to_agent_id: reportsToAgentIdDraft.trim() || null,
        role_label: roleLabelDraft.trim() || null,
      };
      const selectedId = selectedAgentId.trim();
      const existingByDesired =
        agents.find(
          (agent) => agent.agent_id.trim().toLowerCase() === desiredAgentId
        ) ?? null;
      let resolvedAgentId = desiredAgentId;

      if (selectedId && selectedId.toLowerCase() === desiredAgentId) {
        await updateAgent(settings, selectedId, patch);
        resolvedAgentId = selectedId;
      } else if (existingByDesired) {
        await updateAgent(settings, existingByDesired.agent_id, patch);
        resolvedAgentId = existingByDesired.agent_id;
      } else {
        await createAgent(settings, {
          agent_id: desiredAgentId,
          name: desiredAgentName,
          workspace_root: patch.workspace_root,
          tool_profile: patch.tool_profile,
          reports_to_agent_id: patch.reports_to_agent_id,
          role_label: patch.role_label,
        });
      }
      await loadBaseline(settings);
      setSelectedAgentId(resolvedAgentId);
      setAgentReady(true);
      setProviderReady(false);
      setRoutingReady(false);
      return true;
    } catch (error: unknown) {
      setAgentReady(false);
      setErrorText(`Agent setup failed: ${String(error)}`);
      return false;
    } finally {
      setBusy(false);
    }
  }, [
    agentIdDraft,
    agentNameDraft,
    agents,
    clearError,
    loadBaseline,
    selectedAgentId,
    settings,
    toolProfileDraft,
    reportsToAgentIdDraft,
    roleLabelDraft,
    workspaceRootDraft,
  ]);

  const deleteSelectedAgent = useCallback(async (): Promise<boolean> => {
    clearError();
    const targetAgentId = selectedAgentId.trim();
    if (!targetAgentId) {
      setErrorText("Select an agent to delete.");
      return false;
    }
    setBusy(true);
    try {
      const removal = await removeAgent(settings, targetAgentId);
      if (!removal.removed) {
        setErrorText("Agent was already removed.");
        return false;
      }
      const latestAgents = await listAgents(settings);
      await loadBaseline(settings);
      if (latestAgents.items.length === 0) {
        createNewAgentDraft();
        setAgentReady(false);
      } else {
        setSelectedAgentId(latestAgents.items[0].agent_id);
        setAgentReady(true);
      }
      setProviderReady(false);
      setRoutingReady(false);
      return true;
    } catch (error: unknown) {
      setErrorText(`Agent delete failed: ${String(error)}`);
      return false;
    } finally {
      setBusy(false);
    }
  }, [
    clearError,
    createNewAgentDraft,
    loadBaseline,
    selectedAgentId,
    settings,
  ]);

  const reauthSelectedProfile = useCallback(async (): Promise<boolean> => {
    clearError();
    const profileId = selectedExistingProfileId.trim();
    if (!profileId) {
      setErrorText("Select an existing profile to re-auth.");
      return false;
    }
    setBusy(true);
    try {
      await revokeAuthProfile(settings, profileId, {
        reason: "wizard reauth requested",
        remove_secret: true,
        disable_profile: true,
        kill_switch_scope: "profile",
      });
      await loadBaseline(settings);
      setUseExistingProfile(false);
      setSelectedExistingProfileId("");
      setProviderProfileId(null);
      setProviderReady(false);
      setRoutingReady(false);
      if (providerPath === "anthropic") {
        setAnthropicValidationNote("Profile revoked. Re-run auth to continue.");
      }
      if (providerPath === "openai") {
        setOpenAiSessionId("");
        setOpenAiAuthorizeUrl("");
        setOpenAiCallbackUrlHint("");
        setOpenAiCallbackUrl("");
        setOpenAiCode("");
        setOpenAiState("");
      }
      return true;
    } catch (error: unknown) {
      setErrorText(`Profile re-auth failed: ${String(error)}`);
      return false;
    } finally {
      setBusy(false);
    }
  }, [
    clearError,
    loadBaseline,
    providerPath,
    selectedExistingProfileId,
    settings,
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

  const completeProvider = useCallback(async (): Promise<{ ok: boolean; profileId: string | null }> => {
    clearError();
    setBusy(true);
    try {
      const targetAgentId = selectedAgentId.trim() || agentIdDraft.trim().toLowerCase();
      if (providerPath === "local") {
        if (!targetAgentId) {
          throw new Error("Select or create an agent first.");
        }
        const provider = localProvider.trim();
        if (!provider) {
          throw new Error("Select a local provider.");
        }
        let resolvedLocalConnectionProfileId: string | null = null;
        if (localUseConnectionProfile) {
          const credentialsJson: Record<string, unknown> = {};
          const token = localApiKey.trim();
          if (token) {
            credentialsJson.api_key = token;
            credentialsJson.token = token;
            credentialsJson.access_token = token;
            credentialsJson.bearer_token = token;
          }
          const profileResponse = await createAuthProfile(settings, {
            provider,
            display_name: localConnectionProfileName.trim() || `${provider}-local`,
            auth_mode: "api_key",
            risk_level: "low",
            enabled: true,
            kill_switch_scope: "none",
            api_base_url: localApiBaseUrl.trim() || undefined,
            credentials_json: credentialsJson,
          });
          resolvedLocalConnectionProfileId = profileResponse.profile.auth_profile_id;
          setLocalConnectionProfileId(resolvedLocalConnectionProfileId);
        }
        const assistantModel = localModelId.trim();
        if (!assistantModel) {
          throw new Error("Select an assistant model or enter one manually.");
        }
        await updateAgent(settings, targetAgentId, {
          model_provider: provider,
          model_id: assistantModel,
        });
        if (resolvedLocalConnectionProfileId) {
          const existing = await getAgentProviderProfileOrder(settings, targetAgentId, provider);
          const nextOrder = reorderProfileFirst(
            existing.profile_ids,
            resolvedLocalConnectionProfileId
          );
          await setAgentProviderProfileOrder(settings, targetAgentId, provider, nextOrder);
        }
        if (localOrchestratorEnabled) {
          const orchestratorAgentId = localOrchestratorAgentId.trim();
          if (!orchestratorAgentId) {
            throw new Error("Enter an orchestrator agent ID.");
          }
          if (orchestratorAgentId.toLowerCase() === targetAgentId.toLowerCase()) {
            throw new Error(
              "Orchestrator agent ID must be different from the assistant agent ID."
            );
          }
          const orchestratorModel = localOrchestratorModelId.trim() || assistantModel;
          if (!orchestratorModel) {
            throw new Error("Select an orchestrator model or enter one manually.");
          }
          const knownAgents = await listAgents(settings);
          const existingOrchestrator = knownAgents.items.find(
            (agent) =>
              agent.agent_id.trim().toLowerCase() === orchestratorAgentId.toLowerCase()
          );
          if (!existingOrchestrator) {
            await createAgent(settings, {
              agent_id: orchestratorAgentId,
              name: localOrchestratorAgentName.trim() || "Orchestrator",
              workspace_root: workspaceRootDraft.trim() || ".",
              tool_profile: toolProfileDraft.trim() || "default",
            });
          }
          await updateAgent(settings, orchestratorAgentId, {
            model_provider: provider,
            model_id: orchestratorModel,
          });
          if (resolvedLocalConnectionProfileId) {
            const existing = await getAgentProviderProfileOrder(
              settings,
              orchestratorAgentId,
              provider
            );
            const nextOrder = reorderProfileFirst(
              existing.profile_ids,
              resolvedLocalConnectionProfileId
            );
            await setAgentProviderProfileOrder(
              settings,
              orchestratorAgentId,
              provider,
              nextOrder
            );
          }
          setLocalModelDiscoveryNote(
            `Applied local setup: assistant=${targetAgentId}, orchestrator=${orchestratorAgentId}.`
          );
        } else {
          setLocalModelDiscoveryNote(`Applied local setup for assistant agent ${targetAgentId}.`);
        }
        await loadBaseline(settings);
        setProviderReady(true);
        setProviderProfileId(resolvedLocalConnectionProfileId);
        setLocalApiKey("");
        return {
          ok: true,
          profileId: resolvedLocalConnectionProfileId,
        };
      }

      if (useExistingProfile) {
        const existingProfileId = selectedExistingProfileId.trim();
        if (!existingProfileId) {
          throw new Error("Select an existing profile or create a new one.");
        }
        await listProviderModels(settings, {
          provider: providerPath,
          agent_id: targetAgentId || undefined,
          auth_profile_id: existingProfileId,
        });
        setProviderProfileId(existingProfileId);
        setProviderReady(true);
        return {
          ok: true,
          profileId: existingProfileId,
        };
      }

      if (providerPath === "anthropic") {
        if (anthropicAuthMode === "api_key") {
          const response = await ingestAnthropicSetupToken(settings, {
            display_name: anthropicDisplayName.trim() || "claude-primary",
            setup_token: anthropicSetupToken.trim(),
            api_base_url: anthropicApiBaseUrl.trim() || undefined,
            enabled: true,
          });
          const nextProfileId = response.profile.auth_profile_id;
          await listProviderModels(settings, {
            provider: "anthropic",
            agent_id: targetAgentId || undefined,
            auth_profile_id: nextProfileId,
          });
          setProviderProfileId(nextProfileId);
          setProviderReady(true);
          setAnthropicSetupToken("");
          await loadBaseline(settings);
          return {
            ok: true,
            profileId: nextProfileId,
          };
        }

        const accessToken = anthropicAccessToken.trim();
        if (anthropicAuthMode === "claude_consumer_oauth" && !accessToken) {
          throw new Error("Access token is required for Anthropic OAuth mode.");
        }
        const expiresAtUnix = parseOptionalUnixTimestamp(anthropicExpiresAtUnix);
        const credentialsJson: Record<string, unknown> = {};
        if (accessToken) {
          credentialsJson.access_token = accessToken;
          credentialsJson.token = accessToken;
        }
        const refreshToken = anthropicRefreshToken.trim();
        if (refreshToken) {
          credentialsJson.refresh_token = refreshToken;
        }
        const refreshUrl = anthropicRefreshUrl.trim();
        if (refreshUrl) {
          credentialsJson.refresh_url = refreshUrl;
        }
        if (expiresAtUnix !== undefined) {
          credentialsJson.expires_at_unix = expiresAtUnix;
        }
        if (anthropicAuthMode === "agent_sdk") {
          credentialsJson.headless_enabled = true;
          credentialsJson.headless_command =
            anthropicHeadlessCommand.trim() || "claude";
          const rawArgs = anthropicHeadlessArgs.trim();
          credentialsJson.headless_args = rawArgs
            ? rawArgs.split(/\s+/).filter((part) => part.length > 0)
            : [];
        }
        const response = await createAuthProfile(settings, {
          provider: "anthropic",
          display_name:
            anthropicDisplayName.trim() ||
            (anthropicAuthMode === "agent_sdk" ? "claude-headless" : "claude-oauth"),
          auth_mode: anthropicAuthMode,
          risk_level: "high",
          enabled: true,
          kill_switch_scope: "profile",
          api_base_url: anthropicApiBaseUrl.trim() || undefined,
          credentials_json: credentialsJson,
        });
        const nextProfileId = response.profile.auth_profile_id;
        await listProviderModels(settings, {
          provider: "anthropic",
          agent_id: targetAgentId || undefined,
          auth_profile_id: nextProfileId,
        });
        setProviderProfileId(nextProfileId);
        setProviderReady(true);
        setAnthropicAccessToken("");
        setAnthropicRefreshToken("");
        await loadBaseline(settings);
        return {
          ok: true,
          profileId: nextProfileId,
        };
      }

      if (providerPath === "openai") {
        const openAiProfileId = providerProfileId?.trim();
        if (!openAiProfileId) {
          throw new Error(
            "Complete OpenAI OAuth first, or switch to an existing profile."
          );
        }
        await listProviderModels(settings, {
          provider: "openai",
          agent_id: targetAgentId || undefined,
          auth_profile_id: openAiProfileId,
        });
        setProviderReady(true);
        return {
          ok: true,
          profileId: openAiProfileId,
        };
      }
      throw new Error("Unsupported provider path.");
    } catch (error: unknown) {
      setProviderReady(false);
      setErrorText(`Provider setup failed: ${String(error)}`);
      return {
        ok: false,
        profileId: null,
      };
    } finally {
      setBusy(false);
    }
  }, [
    agentIdDraft,
    anthropicAccessToken,
    anthropicApiBaseUrl,
    anthropicAuthMode,
    anthropicDisplayName,
    anthropicExpiresAtUnix,
    anthropicHeadlessArgs,
    anthropicHeadlessCommand,
    anthropicRefreshToken,
    anthropicRefreshUrl,
    anthropicSetupToken,
    clearError,
    loadBaseline,
    localApiBaseUrl,
    localApiKey,
    localConnectionProfileName,
    localModelId,
    localOrchestratorAgentId,
    localOrchestratorAgentName,
    localOrchestratorEnabled,
    localOrchestratorModelId,
    localProvider,
    localUseConnectionProfile,
    providerPath,
    providerProfileId,
    selectedAgentId,
    selectedExistingProfileId,
    settings,
    toolProfileDraft,
    useExistingProfile,
    workspaceRootDraft,
  ]);

  const applyRouting = useCallback(async (profileIdOverride?: string | null): Promise<boolean> => {
    clearError();
    setBusy(true);
    try {
      const targetAgent = selectedAgentId.trim() || agentIdDraft.trim().toLowerCase();
      if (!targetAgent) {
        throw new Error("Select an agent first.");
      }
      if (!providerRequiresProfile(providerPath)) {
        setRoutingReady(true);
        return true;
      }
      const profileId = (profileIdOverride ?? providerProfileId)?.trim();
      if (!profileId) {
        throw new Error("Provider profile is not ready.");
      }
      const existing = await getAgentProviderProfileOrder(settings, targetAgent, providerPath);
      const nextOrder = reorderProfileFirst(existing.profile_ids, profileId);
      await setAgentProviderProfileOrder(settings, targetAgent, providerPath, nextOrder);
      setRoutingReady(true);
      await loadBaseline(settings);
      return true;
    } catch (error: unknown) {
      setRoutingReady(false);
      setErrorText(`Routing update failed: ${String(error)}`);
      return false;
    } finally {
      setBusy(false);
    }
  }, [
    agentIdDraft,
    clearError,
    loadBaseline,
    providerPath,
    providerProfileId,
    selectedAgentId,
    settings,
  ]);

  const completeProviderAndRouting = useCallback(async (): Promise<boolean> => {
    const saved = await saveAgent();
    if (!saved) {
      return false;
    }
    const providerResult = await completeProvider();
    if (!providerResult.ok) {
      return false;
    }
    return applyRouting(providerResult.profileId);
  }, [applyRouting, completeProvider, saveAgent]);

  const nextStep = useCallback(async () => {
    if (nextStepInFlightRef.current) {
      return;
    }
    if (busy || preflight.running) {
      return;
    }
    nextStepInFlightRef.current = true;
    try {
      if (step === "preflight") {
        await runPreflight();
      }
      if (step === "connect") {
        const connectedOk = await connectGateway();
        if (!connectedOk) {
          return;
        }
      }
      if (step === "provider") {
        const setupOk = await completeProviderAndRouting();
        if (!setupOk) {
          return;
        }
      }
      setStepIndex((value) => Math.min(value + 1, ONBOARDING_STEPS.length - 1));
    } finally {
      nextStepInFlightRef.current = false;
    }
  }, [
    busy,
    completeProviderAndRouting,
    connectGateway,
    preflight.running,
    runPreflight,
    step,
  ]);

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
    tokenConfigured,
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
    reportsToAgentIdDraft,
    setReportsToAgentIdDraft,
    roleLabelDraft,
    setRoleLabelDraft,
    strategyEnabled,
    bootstrapPresets,
    selectedPresetKey,
    setSelectedPresetKey,
    applySelectedPreset,
    agentReady,
    createNewAgentDraft,
    saveAgent,
    deleteSelectedAgent,
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
    localUseConnectionProfile,
    setLocalUseConnectionProfile,
    localConnectionProfileName,
    setLocalConnectionProfileName,
    localApiBaseUrl,
    setLocalApiBaseUrl,
    localApiKey,
    setLocalApiKey,
    localModelId,
    setLocalModelId,
    localOrchestratorEnabled,
    setLocalOrchestratorEnabled,
    localOrchestratorAgentId,
    setLocalOrchestratorAgentId,
    localOrchestratorAgentName,
    setLocalOrchestratorAgentName,
    localOrchestratorModelId,
    setLocalOrchestratorModelId,
    localModelDiscoveryNote,
    localProviderOptions,
    localModelOptions,
    localModelsLoading,
    localModelsError,
    refreshLocalModels,
    anthropicAuthMode,
    setAnthropicAuthMode: setAnthropicAuthModeAndInvalidate,
    anthropicDisplayName,
    setAnthropicDisplayName,
    anthropicSetupToken,
    setAnthropicSetupToken,
    anthropicSetupLaunchNote,
    anthropicValidationBusy,
    anthropicValidationNote,
    anthropicApiBaseUrl,
    setAnthropicApiBaseUrl,
    anthropicAccessToken,
    setAnthropicAccessToken,
    anthropicRefreshToken,
    setAnthropicRefreshToken,
    anthropicRefreshUrl,
    setAnthropicRefreshUrl,
    anthropicExpiresAtUnix,
    setAnthropicExpiresAtUnix,
    anthropicHeadlessCommand,
    setAnthropicHeadlessCommand,
    anthropicHeadlessArgs,
    setAnthropicHeadlessArgs,
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
    launchAnthropicSetupTokenFlow,
    runAnthropicSetupTokenValidation,
    startOpenAiOauthFlow,
    finishOpenAiOauthFlow,
    completeProvider,
    completeProviderAndRouting,
    reauthSelectedProfile,
    routingReady,
    applyRouting,
    canFinishReview,
    completeAndExit,
  };
}
