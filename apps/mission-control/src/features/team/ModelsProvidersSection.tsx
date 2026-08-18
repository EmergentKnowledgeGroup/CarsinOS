import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Surface } from "../../ui/Surface";
import { listProviderCapabilities, listProviderModels } from "../../lib/api";
import { providerLabel } from "../../lib/providerCatalog";
import type {
  Agent,
  AuthProfileResponse,
  ProviderCapabilityResponse,
  ProviderModelResponse,
  RuntimeConnectionSettings,
} from "../../types";

/**
 * The Basement Models & Providers room: a bounded read over authoritative
 * provider capability, auth-profile, model-discovery, and agent-assignment
 * facts. It owns no second store and no mutation path; every async state is
 * scoped per provider so one failure never masquerades as a global empty
 * catalog, and missing facts render as unavailable — never as healthy.
 */

interface ModelsProvidersSectionProps {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  authProfiles: AuthProfileResponse[];
}

type CapabilitiesStatus = "loading" | "ready" | "error";

interface ProviderModelsState {
  status: "loading" | "ready" | "error";
  error: string | null;
  items: ProviderModelResponse[];
  resolvedAuthProfileId: string | null;
  requestContext: string;
}

interface ProviderCatalogTarget {
  key: string;
  provider: string;
  authProfileId: string | null;
  profileLabel: string | null;
}

function normalizeProvider(provider: string): string {
  return provider.trim().toLowerCase();
}

function catalogTargetKey(
  provider: string,
  authProfileId: string | null,
): string {
  return authProfileId ? `${provider}\u0000${authProfileId}` : provider;
}

function riskTone(riskLevel: string): "up" | "down" | "warning" | "checking" {
  switch (riskLevel) {
    case "low":
      return "up";
    case "medium":
      return "warning";
    case "high":
    case "critical":
      return "down";
    default:
      return "checking";
  }
}

function capabilityChip(label: string, supported: boolean) {
  return (
    <Chip
      key={label}
      label={supported ? label : `no ${label}`}
      tone={supported ? "up" : ""}
    />
  );
}

export function ModelsProvidersSection({
  settings,
  agents,
  authProfiles,
}: ModelsProvidersSectionProps) {
  const [capabilitiesStatus, setCapabilitiesStatus] =
    useState<CapabilitiesStatus>("loading");
  const [capabilitiesError, setCapabilitiesError] = useState<string | null>(
    null,
  );
  const [capabilities, setCapabilities] = useState<
    ProviderCapabilityResponse[]
  >([]);
  const [capabilitiesGateway, setCapabilitiesGateway] = useState<string | null>(
    null,
  );
  const [modelsByProvider, setModelsByProvider] = useState<
    Record<string, ProviderModelsState>
  >({});
  const [refreshNonce, setRefreshNonce] = useState(0);
  const requestSequenceByTarget = useRef(new Map<string, number>());

  const gatewayIdentity = settings.gateway_url.trim();
  const gatewayConfigured = Boolean(gatewayIdentity);
  const effectiveCapabilitiesStatus =
    gatewayConfigured && capabilitiesGateway !== gatewayIdentity
      ? "loading"
      : capabilitiesStatus;
  const requestContext = `${gatewayIdentity}\u0000${refreshNonce}`;

  // The initial state is already "loading"; a manual refresh resets it from
  // its event handler, so the effect only performs the fetch itself.
  useEffect(() => {
    if (!gatewayConfigured) {
      return;
    }
    let cancelled = false;
    listProviderCapabilities(settings)
      .then((response) => {
        if (cancelled) return;
        setCapabilities(response.items);
        setCapabilitiesStatus("ready");
        setCapabilitiesError(null);
        setCapabilitiesGateway(gatewayIdentity);
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setCapabilities([]);
        setCapabilitiesStatus("error");
        setCapabilitiesError(String(error));
        setCapabilitiesGateway(gatewayIdentity);
      });
    return () => {
      cancelled = true;
    };
  }, [gatewayConfigured, gatewayIdentity, refreshNonce, settings]);

  const capabilityByProvider = useMemo(
    () =>
      new Map(
        capabilities
          .map((item) => [normalizeProvider(item.provider), item] as const)
          .filter(([provider]) => provider),
      ),
    [capabilities],
  );

  const agentsByProvider = useMemo(() => {
    const grouped = new Map<string, Agent[]>();
    for (const agent of agents) {
      const provider = normalizeProvider(agent.model_provider);
      if (!provider) continue;
      grouped.set(provider, [...(grouped.get(provider) ?? []), agent]);
    }
    return grouped;
  }, [agents]);

  const profilesByProvider = useMemo(() => {
    const grouped = new Map<string, AuthProfileResponse[]>();
    for (const profile of authProfiles) {
      const provider = normalizeProvider(profile.provider);
      if (!provider) continue;
      grouped.set(provider, [...(grouped.get(provider) ?? []), profile]);
    }
    return grouped;
  }, [authProfiles]);

  /**
   * The room presents the union of every provider the system actually
   * references, so an agent assignment or configured profile stays visible
   * even when the capability catalog omits or fails to describe it.
   */
  const providers = useMemo(() => {
    const union = new Set<string>();
    for (const item of capabilities) {
      const provider = normalizeProvider(item.provider);
      if (provider && provider !== "unconfigured") union.add(provider);
    }
    for (const profile of authProfiles) {
      const provider = normalizeProvider(profile.provider);
      if (provider) union.add(provider);
    }
    for (const agent of agents) {
      const provider = normalizeProvider(agent.model_provider);
      if (provider) union.add(provider);
    }
    return Array.from(union).sort((a, b) => a.localeCompare(b));
  }, [agents, authProfiles, capabilities]);

  const catalogTargetsByProvider = useMemo(() => {
    const targets = new Map<string, ProviderCatalogTarget[]>();
    for (const provider of providers) {
      if (provider === "unconfigured") {
        targets.set(provider, []);
        continue;
      }
      const enabledProfiles = (profilesByProvider.get(provider) ?? []).filter(
        (profile) => profile.enabled,
      );
      if (enabledProfiles.length === 0) {
        targets.set(provider, [
          {
            key: catalogTargetKey(provider, null),
            provider,
            authProfileId: null,
            profileLabel: null,
          },
        ]);
        continue;
      }
      targets.set(
        provider,
        enabledProfiles.map((profile) => ({
          key: catalogTargetKey(provider, profile.auth_profile_id),
          provider,
          authProfileId: profile.auth_profile_id,
          profileLabel: profile.display_name,
        })),
      );
    }
    return targets;
  }, [profilesByProvider, providers]);

  const catalogTargetSignature = useMemo(
    () =>
      providers
        .flatMap((provider) => catalogTargetsByProvider.get(provider) ?? [])
        .map((target) => target.key)
        .join("\u0001"),
    [catalogTargetsByProvider, providers],
  );

  // A provider with no entry renders as loading, so the discovery effect
  // never needs a synchronous marker write; only user-driven retries mark
  // loading from their event handler.
  const fetchModelsForTarget = useCallback(
    (target: ProviderCatalogTarget, refresh: boolean) => {
      const requestSequence =
        (requestSequenceByTarget.current.get(target.key) ?? 0) + 1;
      requestSequenceByTarget.current.set(target.key, requestSequence);
      listProviderModels(settings, {
        provider: target.provider,
        ...(target.authProfileId
          ? { auth_profile_id: target.authProfileId }
          : {}),
        ...(refresh ? { refresh: true } : {}),
      })
        .then((response) => {
          if (
            requestSequenceByTarget.current.get(target.key) !== requestSequence
          ) {
            return;
          }
          if (
            target.authProfileId &&
            response.auth_profile_id !== target.authProfileId
          ) {
            setModelsByProvider((current) => ({
              ...current,
              [target.key]: {
                status: "error",
                error:
                  "The gateway returned a catalog for a different auth profile.",
                items: [],
                resolvedAuthProfileId: response.auth_profile_id,
                requestContext,
              },
            }));
            return;
          }
          setModelsByProvider((current) => ({
            ...current,
            [target.key]: {
              status: "ready",
              error: null,
              items: response.items,
              resolvedAuthProfileId: response.auth_profile_id,
              requestContext,
            },
          }));
        })
        .catch((error: unknown) => {
          if (
            requestSequenceByTarget.current.get(target.key) !== requestSequence
          ) {
            return;
          }
          setModelsByProvider((current) => ({
            ...current,
            [target.key]: {
              status: "error",
              error: String(error),
              items: [],
              resolvedAuthProfileId: null,
              requestContext,
            },
          }));
        });
    },
    [requestContext, settings],
  );

  const retryModelsForTarget = useCallback(
    (target: ProviderCatalogTarget) => {
      setModelsByProvider((current) => ({
        ...current,
        [target.key]: {
          status: "loading",
          error: null,
          items: [],
          resolvedAuthProfileId: null,
          requestContext,
        },
      }));
      fetchModelsForTarget(target, true);
    },
    [fetchModelsForTarget, requestContext],
  );

  const refreshEverything = useCallback(() => {
    setCapabilitiesStatus("loading");
    setCapabilitiesError(null);
    setModelsByProvider({});
    setRefreshNonce((nonce) => nonce + 1);
  }, []);

  useEffect(() => {
    if (!gatewayConfigured || effectiveCapabilitiesStatus === "loading") {
      return;
    }
    for (const provider of providers) {
      for (const target of catalogTargetsByProvider.get(provider) ?? []) {
        fetchModelsForTarget(target, refreshNonce > 0);
      }
    }
    // Discovery re-runs when the provider union or refresh nonce changes;
    // per-provider results land independently so one slow or failing
    // provider never blocks the rest.
  }, [
    effectiveCapabilitiesStatus,
    catalogTargetSignature,
    catalogTargetsByProvider,
    fetchModelsForTarget,
    gatewayConfigured,
    providers,
    refreshNonce,
  ]);

  if (!gatewayConfigured) {
    return (
      <Surface
        className="mc-models-surface"
        title="Models & Providers"
        subtitle="Authoritative provider capability, profile, and assignment facts."
      >
        <EmptyState message="Connect Mission Control to the gateway before you inspect providers and models." />
      </Surface>
    );
  }

  return (
    <Surface
      className="mc-models-surface"
      title="Models & Providers"
      subtitle="What each provider can do, which auth profiles are configured, which models were discovered, and who is assigned where. Facts only — setup and credentials stay on their existing surfaces."
      headerRight={
        <button
          type="button"
          className="ghost"
          onClick={refreshEverything}
          disabled={effectiveCapabilitiesStatus === "loading"}
        >
          <RefreshCw size={14} />
          Refresh
        </button>
      }
    >
      {effectiveCapabilitiesStatus === "error" ? (
        <div className="mc-notice mc-notice-error">
          Provider capability facts could not be loaded. ({capabilitiesError})
        </div>
      ) : null}
      {effectiveCapabilitiesStatus === "loading" ? (
        <EmptyState message="Loading provider capability facts…" />
      ) : null}
      {effectiveCapabilitiesStatus !== "loading" && providers.length === 0 ? (
        <EmptyState message="No providers are described, configured, or assigned yet." />
      ) : null}

      <div className="mc-models-provider-grid">
        {effectiveCapabilitiesStatus === "loading"
          ? null
          : providers.map((provider) => {
              const capability = capabilityByProvider.get(provider) ?? null;
              const providerProfiles = profilesByProvider.get(provider) ?? [];
              const providerAgents = agentsByProvider.get(provider) ?? [];
              const catalogTargets =
                catalogTargetsByProvider.get(provider) ?? [];
              return (
                <article
                  key={provider}
                  className="mc-models-provider-card"
                  data-testid={`models-provider-${provider}`}
                >
                  <header className="mc-models-provider-head">
                    <strong>{providerLabel(provider)}</strong>
                    <span className="mc-models-provider-id">{provider}</span>
                  </header>

                  {capability ? (
                    <div className="mc-team-card-tags">
                      {capabilityChip("streaming", capability.supports_streaming)}
                      {capabilityChip("tools", capability.supports_tools)}
                      {capabilityChip("JSON mode", capability.supports_json_mode)}
                      {capabilityChip("vision", capability.supports_vision)}
                    </div>
                  ) : (
                    <div className="mc-team-card-tags">
                      <Chip label="capability facts unavailable" tone="warning" />
                    </div>
                  )}
                  <p className="mc-models-fact-line">
                    Context window:{" "}
                    {capability?.max_context_tokens != null
                      ? `${capability.max_context_tokens.toLocaleString()} tokens`
                      : "unknown"}
                  </p>
                  {capability && capability.error_classes.length > 0 ? (
                    <p className="mc-models-fact-line">
                      {capability.error_classes.length} known error class
                      {capability.error_classes.length === 1 ? "" : "es"} (
                      {capability.retryable_error_classes.length} retryable)
                    </p>
                  ) : null}

                  <h4 className="mc-models-subhead">Auth profiles</h4>
                  {providerProfiles.length === 0 ? (
                    <p className="mc-models-empty-hint">
                      No configured profiles for this provider.
                    </p>
                  ) : (
                    <ul className="mc-models-profile-list">
                      {providerProfiles.map((profile) => (
                        <li key={profile.auth_profile_id}>
                          <span className="mc-models-profile-name">
                            {profile.display_name}
                          </span>
                          <span className="mc-team-card-tags">
                            <Chip
                              label={profile.enabled ? "enabled" : "disabled"}
                              tone={profile.enabled ? "up" : "down"}
                            />
                            <Chip label={profile.auth_mode} tone="" />
                            <Chip
                              label={`risk: ${profile.risk_level}`}
                              tone={riskTone(profile.risk_level)}
                            />
                            <Chip
                              label={`kill switch: ${profile.kill_switch_scope}`}
                              tone=""
                            />
                          </span>
                          {profile.api_base_url ? (
                            <span className="mc-models-profile-url">
                              {profile.api_base_url}
                            </span>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  )}

                  <h4 className="mc-models-subhead">Discovered models</h4>
                  {provider === "unconfigured" ? (
                    <p className="mc-models-empty-hint">
                      Model discovery does not apply to unconfigured
                      assignments.
                    </p>
                  ) : null}
                  {catalogTargets.map((target) => {
                    const models = modelsByProvider[target.key] ?? {
                      status: "loading" as const,
                      error: null,
                      items: [],
                      resolvedAuthProfileId: null,
                      requestContext,
                    };
                    const currentModels =
                      models.requestContext === requestContext
                        ? models
                        : {
                            status: "loading" as const,
                            error: null,
                            items: [],
                            resolvedAuthProfileId: null,
                            requestContext,
                          };
                    const resolvedProfile = currentModels.resolvedAuthProfileId
                      ? providerProfiles.find(
                          (profile) =>
                            profile.auth_profile_id ===
                            currentModels.resolvedAuthProfileId,
                        )
                      : null;
                    return (
                      <div
                        key={target.key}
                        className="mc-models-catalog"
                        data-testid={`models-catalog-${provider}-${target.authProfileId ?? "default"}`}
                      >
                        <p className="mc-models-catalog-scope">
                          {target.profileLabel
                            ? `Profile: ${target.profileLabel}`
                            : resolvedProfile
                              ? `Gateway-selected profile: ${resolvedProfile.display_name}`
                              : "Provider default catalog"}
                        </p>
                        {currentModels.status === "loading" ? (
                          <p className="mc-models-empty-hint">
                            Discovering models…
                          </p>
                        ) : null}
                        {currentModels.status === "error" ? (
                          <div className="mc-notice mc-notice-error mc-models-scoped-error">
                            <span>
                              Model discovery failed for{" "}
                              {target.profileLabel ??
                                providerLabel(provider)}
                              . ({currentModels.error})
                            </span>
                            <button
                              type="button"
                              className="ghost"
                              onClick={() => retryModelsForTarget(target)}
                            >
                              Retry
                            </button>
                          </div>
                        ) : null}
                        {currentModels.status === "ready" &&
                        currentModels.items.length === 0 ? (
                          <p className="mc-models-empty-hint">
                            No models discovered.
                          </p>
                        ) : null}
                        {currentModels.status === "ready" &&
                        currentModels.items.length > 0 ? (
                          <ul className="mc-models-model-list">
                            {currentModels.items.map((model) => {
                              const assignedAgents = providerAgents.filter(
                                (agent) => agent.model_id === model.model_id,
                              );
                              return (
                                <li key={model.model_id}>
                                  <span className="mc-models-model-id">
                                    {model.model_id}
                                  </span>
                                  {model.label &&
                                  model.label !== model.model_id ? (
                                    <span className="mc-models-model-label">
                                      {model.label}
                                    </span>
                                  ) : null}
                                  {assignedAgents.length > 0 ? (
                                    <Chip
                                      label={`in use: ${assignedAgents
                                        .map((agent) => agent.name)
                                        .join(", ")}`}
                                      tone="up"
                                    />
                                  ) : null}
                                </li>
                              );
                            })}
                          </ul>
                        ) : null}
                        {currentModels.status === "ready" ? (
                          <button
                            type="button"
                            className="ghost mc-models-refresh-btn"
                            onClick={() => retryModelsForTarget(target)}
                          >
                            <RefreshCw size={12} />
                            Refresh models
                          </button>
                        ) : null}
                      </div>
                    );
                  })}

                  <h4 className="mc-models-subhead">Assigned agents</h4>
                  {providerAgents.length === 0 ? (
                    <p className="mc-models-empty-hint">
                      No agents currently assigned.
                    </p>
                  ) : (
                    <ul className="mc-models-assignment-list">
                      {providerAgents.map((agent) => (
                        <li key={agent.agent_id}>
                          <span>{agent.name}</span>
                          <span className="mc-models-model-id">
                            {agent.model_id}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </article>
              );
            })}
      </div>
    </Surface>
  );
}
