import { useDeferredValue, useMemo, useState, type ReactNode } from "react";
import {
  Compass,
  GitBranch,
  HeartPulse,
  PackageOpen,
  RefreshCw,
  ShieldAlert,
  Upload,
} from "lucide-react";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Modal } from "../../ui/Modal";
import { Surface } from "../../ui/Surface";
import { formatRelative } from "../../utils/datetime";
import type { ConnectorPublishedToolResponse } from "../../types";
import {
  CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS,
  CONNECTOR_AUTH_KIND_OPTIONS,
  CONNECTOR_AUTH_STATUS_OPTIONS,
  CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS,
  CONNECTOR_JSON_TEXTAREA_ROWS,
  CONNECTOR_INTERACTION_PAYLOAD_PLACEHOLDER,
  CONNECTOR_ORIGIN_KIND_OPTIONS,
  CONNECTOR_SOURCE_KIND_OPTIONS,
  CONNECTOR_TEXTAREA_ROWS,
} from "./connectorsConfig";
import "./connectors.css";
import { stringifyJson } from "./connectorsModel";
import type { useConnectorsController } from "./useConnectorsController";

interface ConnectorsPageProps {
  controller: ReturnType<typeof useConnectorsController>;
}

function humanize(value: string | null | undefined): string {
  if (!value) {
    return "n/a";
  }
  return value.replaceAll("_", " ").replaceAll("-", " ");
}

function formatTimestamp(value: number | null | undefined): string {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "n/a";
  }
  return formatRelative(value);
}

function toneForStatus(status: string | null | undefined): "up" | "down" | "warning" | "checking" | "" {
  switch (status) {
    case "enabled":
    case "ready":
    case "succeeded":
    case "trusted_curated":
    case "reviewed_local":
    case "active":
      return "up";
    case "disabled":
    case "error":
    case "failed":
    case "blocked":
    case "expired":
    case "unsafe_blocked":
    case "cancelled":
    case "unpublished":
      return "down";
    case "under_review":
    case "converted":
    case "pending":
    case "waiting_on_operator":
    case "operator_write_gated":
    case "destructive_write_gated":
    case "superseded":
      return "warning";
    default:
      return status ? "checking" : "";
  }
}

function toneForHealth(
  status: string | null | undefined,
  degradedReason: string | null | undefined,
  authRequired: boolean
): "up" | "down" | "warning" | "checking" | "" {
  if (status === "enabled" && !degradedReason && !authRequired) {
    return "up";
  }
  if (status === "disabled" || status === "error") {
    return "down";
  }
  if (degradedReason || authRequired) {
    return "warning";
  }
  return status ? "checking" : "";
}

function ConnectorsStatePanel({
  title,
  detail,
}: {
  title: string;
  detail: string;
}) {
  return (
    <section className="mc-connectors-page" data-testid="connectors-page">
      <Surface className="mc-connectors-state" title={title} subtitle={detail}>
        <EmptyState message={detail} />
      </Surface>
    </section>
  );
}

function SummaryCard({
  icon,
  label,
  value,
  detail,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="mc-connectors-summary-card">
      <div className="mc-connectors-summary-kicker">
        {icon}
        <span>{label}</span>
      </div>
      <strong>{value}</strong>
      <p>{detail}</p>
    </div>
  );
}

function ToolSelectionRow({
  tool,
  selected,
  checked,
  onSelect,
  onToggle,
}: {
  tool: ConnectorPublishedToolResponse;
  selected: boolean;
  checked: boolean;
  onSelect: () => void;
  onToggle: () => void;
}) {
  return (
    <div className={`mc-connectors-tool-row${selected ? " is-selected" : ""}`}>
      <label className="mc-connectors-tool-check">
        <input type="checkbox" checked={checked} onChange={onToggle} />
      </label>
      <button type="button" className="mc-connectors-tool-button" onClick={onSelect}>
        <div className="mc-connectors-tool-head">
          <strong>{tool.display_name}</strong>
          <div className="mc-connectors-inline-chips">
            <Chip label={humanize(tool.deprecation_state)} tone={toneForStatus(tool.deprecation_state)} />
            <Chip
              label={humanize(tool.write_classification)}
              tone={toneForStatus(tool.write_classification)}
            />
          </div>
        </div>
        <div className="mc-connectors-tool-meta">
          <span>{tool.tool_name}</span>
          <span>{formatTimestamp(tool.published_at)}</span>
        </div>
      </button>
    </div>
  );
}

export function ConnectorsPage({ controller }: ConnectorsPageProps) {
  const [catalogQuery, setCatalogQuery] = useState("");
  const [registryQuery, setRegistryQuery] = useState("");
  const [confirmAction, setConfirmAction] = useState<"publish" | "unpublish" | null>(null);
  const deferredCatalogQuery = useDeferredValue(catalogQuery.trim().toLowerCase());
  const deferredRegistryQuery = useDeferredValue(registryQuery.trim().toLowerCase());
  const isBusy = Boolean(controller.mutatingAction);

  const agentsById = useMemo(
    () => new Map(controller.agents.map((agent) => [agent.agent_id, agent] as const)),
    [controller.agents]
  );

  const filteredCatalog = useMemo(() => {
    return controller.catalog.filter((item) => {
      if (!deferredCatalogQuery) {
        return true;
      }
      const haystack = [
        item.display_name,
        item.summary,
        item.publisher,
        item.slug,
        item.source_kind,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(deferredCatalogQuery);
    });
  }, [controller.catalog, deferredCatalogQuery]);

  const filteredConnectors = useMemo(() => {
    return controller.installedConnectors.filter((item) => {
      if (!deferredRegistryQuery) {
        return true;
      }
      const haystack = [
        item.display_name,
        item.slug,
        item.source_kind,
        item.status,
        item.trust_state,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(deferredRegistryQuery);
    });
  }, [controller.installedConnectors, deferredRegistryQuery]);

  const totalPublishedTools = controller.installedConnectors.reduce(
    (sum, item) => sum + item.published_tool_count,
    0
  );
  const sharedBinding =
    controller.selectedConnectorDetail?.auth_bindings.find((item) => !item.agent_id) ?? null;
  const toolDetail =
    controller.selectedToolDetail?.published_tool ?? controller.selectedPublishedTool ?? null;

  if (!controller.enabled || controller.availability === "disabled") {
    return (
      <ConnectorsStatePanel
        title="Connectors are disabled"
        detail="Enable Connectors in Config > Reliability + Rollout to expose connector intake, review, and auth controls."
      />
    );
  }

  if (controller.availability === "unsupported") {
    return (
      <ConnectorsStatePanel
        title="Connectors surface unavailable"
        detail={
          controller.availabilityMessage ??
          "The connected gateway does not expose the Connectors surface yet."
        }
      />
    );
  }

  if (controller.availability === "error") {
    return (
      <ConnectorsStatePanel
        title="Connectors failed to load"
        detail={controller.availabilityMessage ?? "Connectors could not load."}
      />
    );
  }

  if (controller.availability === "loading" && controller.installedConnectors.length === 0) {
    return (
      <ConnectorsStatePanel
        title="Loading Connectors"
        detail="Resolving catalog intake, installed registry state, and paused connector interactions."
      />
    );
  }

  return (
    <section className="mc-connectors-page" data-testid="connectors-page">
      <div className="mc-connectors-summary-strip">
        <SummaryCard
          icon={<Compass size={16} />}
          label="Catalog"
          value={String(controller.catalog.length)}
          detail="Curated intake templates"
        />
        <SummaryCard
          icon={<PackageOpen size={16} />}
          label="Installed"
          value={String(controller.installedConnectors.length)}
          detail="Connector records in registry"
        />
        <SummaryCard
          icon={<GitBranch size={16} />}
          label="Live tools"
          value={String(totalPublishedTools)}
          detail="Published capabilities on deck"
        />
        <SummaryCard
          icon={<HeartPulse size={16} />}
          label="Paused"
          value={String(controller.pausedInteractions.length)}
          detail="Interactions waiting on operator"
        />
      </div>

      <div className="mc-connectors-layout">
        <div className="mc-connectors-column">
          <Surface
            className="mc-connectors-panel"
            title="Catalog intake"
            subtitle="Start from curated scaffolds, then land a connector draft with a clear source of truth."
          >
            <div className="mc-connectors-toolbar">
              <input
                value={catalogQuery}
                onChange={(event) => setCatalogQuery(event.target.value)}
                placeholder="Search catalog"
              />
              <span className="mc-connectors-sort-label">Sorted by name</span>
            </div>
            <div className="mc-connectors-card-list">
              {filteredCatalog.length === 0 ? (
                <EmptyState message="No catalog templates match the current search." />
              ) : (
                filteredCatalog.map((item) => (
                  <button
                    type="button"
                    key={item.catalog_item_id}
                    className="mc-connectors-card"
                    onClick={() => controller.applyCatalogTemplate(item.catalog_item_id)}
                  >
                    <div className="mc-connectors-card-head">
                      <strong>{item.display_name}</strong>
                      <div className="mc-connectors-inline-chips">
                        <Chip label={item.source_kind} tone="checking" />
                        <Chip label={humanize(item.trust_class)} tone={toneForStatus(item.trust_class)} />
                      </div>
                    </div>
                    <p>{item.summary}</p>
                    <div className="mc-connectors-card-meta">
                      <span>{item.publisher}</span>
                      <span>{item.available_versions.join(", ") || "v1"}</span>
                    </div>
                  </button>
                ))
              )}
            </div>
          </Surface>

          <Surface
            className="mc-connectors-panel"
            title="Import connector"
            subtitle="Pragmatic intake flow for raw JSON, inline source text, or URL-backed imports."
            headerRight={
              <div className="mc-connectors-surface-actions">
                <button
                  type="button"
                  className="ghost"
                  onClick={() => controller.resetImportDraft()}
                  disabled={isBusy}
                >
                  Clear
                </button>
                <button
                  type="button"
                  data-testid="connectors-import-submit"
                  onClick={() => {
                    void controller.importFromDraft();
                  }}
                  disabled={isBusy}
                >
                  <Upload size={14} />
                  <span>Import</span>
                </button>
              </div>
            }
          >
            <div className="mc-connectors-form-grid">
              <label className="mc-connectors-field">
                <span>Display name</span>
                <input
                  value={controller.importDraft.display_name}
                  onChange={(event) =>
                    controller.updateImportDraft({ display_name: event.target.value })
                  }
                  placeholder="GitHub automation"
                />
              </label>
              <label className="mc-connectors-field">
                <span>Source kind</span>
                <select
                  value={controller.importDraft.source_kind}
                  onChange={(event) =>
                    controller.updateImportDraft({ source_kind: event.target.value })
                  }
                >
                  {CONNECTOR_SOURCE_KIND_OPTIONS.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>
              <label className="mc-connectors-field">
                <span>Slug</span>
                <input
                  value={controller.importDraft.slug}
                  onChange={(event) => controller.updateImportDraft({ slug: event.target.value })}
                  placeholder="github-automation"
                />
              </label>
              <label className="mc-connectors-field">
                <span>Version label</span>
                <input
                  value={controller.importDraft.version_label}
                  onChange={(event) =>
                    controller.updateImportDraft({ version_label: event.target.value })
                  }
                  placeholder="v1"
                />
              </label>
              <label className="mc-connectors-field">
                <span>Origin kind</span>
                <select
                  value={controller.importDraft.origin_kind}
                  onChange={(event) =>
                    controller.updateImportDraft({ origin_kind: event.target.value })
                  }
                >
                  {CONNECTOR_ORIGIN_KIND_OPTIONS.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>
              <label className="mc-connectors-field">
                <span>External ref policy</span>
                <select
                  value={controller.importDraft.external_reference_policy}
                  onChange={(event) =>
                    controller.updateImportDraft({
                      external_reference_policy: event.target.value,
                    })
                  }
                >
                  {CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>
              <label className="mc-connectors-field">
                <span>Catalog item id</span>
                <input
                  value={controller.importDraft.catalog_item_id}
                  onChange={(event) =>
                    controller.updateImportDraft({ catalog_item_id: event.target.value })
                  }
                  placeholder="github-openapi"
                />
              </label>
              <label className="mc-connectors-field">
                <span>Import URL</span>
                <input
                  value={controller.importDraft.import_url}
                  onChange={(event) =>
                    controller.updateImportDraft({ import_url: event.target.value })
                  }
                  placeholder="https://example.com/openapi.json"
                />
              </label>
              <label className="mc-connectors-field mc-connectors-field-span">
                <span>Endpoint URL</span>
                <input
                  value={controller.importDraft.endpoint_url}
                  onChange={(event) =>
                    controller.updateImportDraft({ endpoint_url: event.target.value })
                  }
                  placeholder="https://api.example.com"
                />
              </label>
              <label className="mc-connectors-field mc-connectors-field-span">
                <span>Source text</span>
                <textarea
                  rows={CONNECTOR_TEXTAREA_ROWS}
                  value={controller.importDraft.source_text}
                  onChange={(event) =>
                    controller.updateImportDraft({ source_text: event.target.value })
                  }
                  placeholder="Paste raw source text when the upstream is easier to stage inline."
                />
              </label>
              <label className="mc-connectors-field mc-connectors-field-span">
                <span>Source JSON</span>
                <textarea
                  rows={CONNECTOR_JSON_TEXTAREA_ROWS}
                  value={controller.importDraft.source_json_text}
                  onChange={(event) =>
                    controller.updateImportDraft({ source_json_text: event.target.value })
                  }
                  placeholder='{"openapi":"3.1.0","paths":{}}'
                />
              </label>
            </div>
          </Surface>

          <Surface
            className="mc-connectors-panel"
            title="Global interactions"
            subtitle="Resume paused connector work without losing the operator handoff trail."
          >
            <label className="mc-connectors-field mc-connectors-field-span">
              <span>Resume payload JSON</span>
              <textarea
                rows={CONNECTOR_TEXTAREA_ROWS}
                value={controller.interactionPayloadText}
                onChange={(event) => controller.setInteractionPayloadText(event.target.value)}
                placeholder={CONNECTOR_INTERACTION_PAYLOAD_PLACEHOLDER}
              />
            </label>
            <div className="mc-connectors-list">
              {controller.pausedInteractions.length === 0 ? (
                <EmptyState message="No paused connector interactions are waiting on operator follow-up." />
              ) : (
                controller.pausedInteractions.map((item) => (
                  <div key={item.interaction_id} className="mc-connectors-list-row">
                    <div>
                      <div className="mc-connectors-list-title">
                        <strong>{humanize(item.interaction_kind)}</strong>
                        <Chip label={humanize(item.status)} tone={toneForStatus(item.status)} />
                      </div>
                      <p>{item.prompt_summary}</p>
                      <div className="mc-connectors-list-meta">
                        <span>{item.agent_id ? agentsById.get(item.agent_id)?.name ?? item.agent_id : "shared"}</span>
                        <span>{formatTimestamp(item.updated_at)}</span>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="secondary"
                      disabled={isBusy}
                      onClick={() => {
                        void controller.resumeInteraction(item.interaction_id);
                      }}
                    >
                      Resume
                    </button>
                  </div>
                ))
              )}
            </div>
          </Surface>
        </div>

        <div className="mc-connectors-column mc-connectors-column-wide">
          <Surface
            className="mc-connectors-panel"
            title="Installed registry"
            subtitle="Select a connector to inspect its current version, auth posture, and published tools."
          >
            <div className="mc-connectors-toolbar">
              <input
                value={registryQuery}
                onChange={(event) => setRegistryQuery(event.target.value)}
                placeholder="Search installed connectors"
              />
              <span className="mc-connectors-sort-label">Sorted by name</span>
              <button
                type="button"
                className="ghost"
                disabled={isBusy}
                onClick={() => {
                  void controller.refresh();
                }}
              >
                <RefreshCw size={14} />
                <span>Refresh</span>
              </button>
            </div>
            <div className="mc-connectors-card-list">
              {filteredConnectors.length === 0 ? (
                <EmptyState message="No installed connectors match the current search." />
              ) : (
                filteredConnectors.map((item) => (
                  <button
                    type="button"
                    key={item.connector_id}
                    className={`mc-connectors-card${
                      item.connector_id === controller.selectedConnectorId ? " is-selected" : ""
                    }`}
                    onClick={() => controller.selectConnector(item.connector_id)}
                  >
                    <div className="mc-connectors-card-head">
                      <strong>{item.display_name}</strong>
                      <div className="mc-connectors-inline-chips">
                        <Chip label={humanize(item.status)} tone={toneForStatus(item.status)} />
                        <Chip
                          label={humanize(item.trust_state)}
                          tone={toneForStatus(item.trust_state)}
                        />
                      </div>
                    </div>
                    <p>{item.slug}</p>
                    <div className="mc-connectors-card-meta">
                      <span>{item.source_kind}</span>
                      <span>{item.published_tool_count} live tool(s)</span>
                      <span>{item.assigned_agent_count} agent(s)</span>
                    </div>
                  </button>
                ))
              )}
            </div>
          </Surface>

          <Surface
            className="mc-connectors-panel mc-connectors-hero"
            title={controller.selectedConnector?.display_name ?? "Connector detail"}
            subtitle={
              controller.selectedConnector
                ? `${controller.selectedConnector.slug} · ${controller.selectedConnector.source_kind}`
                : "Select an installed connector to open review, auth, and health surfaces."
            }
            headerRight={
              controller.selectedConnector ? (
                <div className="mc-connectors-surface-actions">
                  <button
                    type="button"
                    className="ghost"
                    disabled={isBusy}
                    onClick={() => {
                      void controller.refresh();
                    }}
                  >
                    <RefreshCw size={14} />
                    <span>Sync</span>
                  </button>
                  <button
                    type="button"
                    className={
                      controller.selectedConnector.status === "enabled" ? "danger" : "secondary"
                    }
                    disabled={isBusy}
                    onClick={() => {
                      void controller.updateConnectorEnabled(
                        controller.selectedConnector?.status !== "enabled"
                      );
                    }}
                  >
                    {controller.selectedConnector.status === "enabled" ? "Disable" : "Enable"}
                  </button>
                </div>
              ) : null
            }
          >
            {!controller.selectedConnector ? (
              <EmptyState message="Select a connector from the registry to open its lifecycle controls." />
            ) : controller.detailLoading && !controller.selectedConnectorDetail ? (
              <EmptyState message="Loading selected connector detail…" />
            ) : controller.detailError ? (
              <EmptyState message={controller.detailError} />
            ) : (
              <div className="mc-connectors-overview">
                <div className="mc-connectors-inline-chips">
                  <Chip
                    label={humanize(controller.selectedConnector.status)}
                    tone={toneForStatus(controller.selectedConnector.status)}
                  />
                  <Chip
                    label={humanize(controller.selectedConnector.trust_state)}
                    tone={toneForStatus(controller.selectedConnector.trust_state)}
                  />
                  {sharedBinding ? (
                    <Chip
                      label={`shared auth · ${humanize(sharedBinding.status)}`}
                      tone={toneForStatus(sharedBinding.status)}
                    />
                  ) : (
                    <Chip label="shared auth missing" tone="warning" />
                  )}
                </div>
                <dl className="mc-connectors-fact-grid">
                  <div>
                    <dt>Origin</dt>
                    <dd>{humanize(controller.selectedConnector.origin_kind)}</dd>
                  </div>
                  <div>
                    <dt>Current version</dt>
                    <dd>{controller.selectedVersion?.version_label ?? "not live"}</dd>
                  </div>
                  <div>
                    <dt>Last conversion</dt>
                    <dd>{formatTimestamp(controller.selectedConnector.last_conversion_at)}</dd>
                  </div>
                  <div>
                    <dt>Last review</dt>
                    <dd>{formatTimestamp(controller.selectedConnector.last_review_at)}</dd>
                  </div>
                  <div>
                    <dt>Published tools</dt>
                    <dd>{controller.selectedConnector.published_tool_count}</dd>
                  </div>
                  <div>
                    <dt>Assigned agents</dt>
                    <dd>{controller.selectedConnector.assigned_agent_count}</dd>
                  </div>
                </dl>
              </div>
            )}
          </Surface>

          <div className="mc-connectors-detail-grid">
            <Surface
              className="mc-connectors-panel"
              title="Review + publish"
              subtitle="Convert the selected version, inspect generated operations, then publish only the tools you want live."
              headerRight={
                <div className="mc-connectors-surface-actions">
                  <button
                    type="button"
                    className="secondary"
                    data-testid="connectors-convert-submit"
                    disabled={isBusy || !controller.selectedConnector}
                    onClick={() => {
                      void controller.convertSelectedConnector();
                    }}
                  >
                    Convert
                  </button>
                  <button
                    type="button"
                    className="ghost"
                    disabled={isBusy || !controller.selectedVersionId}
                    onClick={() => {
                      void controller.rollbackSelectedVersion();
                    }}
                  >
                    Roll back
                  </button>
                </div>
              }
            >
              {!controller.selectedConnector ? (
                <EmptyState message="Select a connector to review proposed tools." />
              ) : (
                <>
                  <div className="mc-connectors-toolbar mc-connectors-toolbar-stacked">
                    <label className="mc-connectors-field">
                      <span>Version target</span>
                      <select
                        value={controller.selectedVersionId}
                        onChange={(event) => controller.setSelectedVersionId(event.target.value)}
                      >
                        {(controller.selectedConnectorDetail?.versions ?? []).map((item) => (
                          <option key={item.version_id} value={item.version_id}>
                            {item.version_label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <div className="mc-connectors-inline-chips">
                      <Chip
                        label={
                          controller.selectedConversion
                            ? `conversion · ${humanize(controller.selectedConversion.status)}`
                            : "no review cache"
                        }
                        tone={toneForStatus(controller.selectedConversion?.status)}
                      />
                      {controller.selectedConversion ? (
                        <Chip
                          label={`${controller.selectedConversion.proposed_tools.length} proposed`}
                          tone="checking"
                        />
                      ) : null}
                    </div>
                  </div>

                  {!controller.selectedConversion ? (
                    <EmptyState message="Run conversion on the selected version to inspect reviewable operations." />
                  ) : (
                    <>
                      {controller.selectedConversion.warnings.length > 0 ? (
                        <div className="mc-connectors-warning-list">
                          {controller.selectedConversion.warnings.map((warning) => (
                            <div key={`${warning.code}-${warning.message}`} className="mc-connectors-warning">
                              <ShieldAlert size={14} />
                              <span>
                                <strong>{warning.code}</strong> {warning.message}
                              </span>
                            </div>
                          ))}
                        </div>
                      ) : null}

                      <div className="mc-connectors-toggle-row">
                        <label className="mc-connectors-checkbox">
                          <input
                            type="checkbox"
                            checked={controller.publishDraft.enable_after_publish}
                            disabled={isBusy}
                            onChange={(event) =>
                              controller.setEnableAfterPublish(event.target.checked)
                            }
                          />
                          <span>Enable immediately after publish</span>
                        </label>
                        <button
                          type="button"
                          data-testid="connectors-publish-submit"
                          disabled={
                            isBusy || controller.publishDraft.selected_candidate_ids.length === 0
                          }
                          onClick={() => {
                            setConfirmAction("publish");
                          }}
                        >
                          Publish selected
                        </button>
                      </div>

                      <div className="mc-connectors-candidate-list">
                        {controller.selectedConversion.proposed_tools.map((candidate) => {
                          const selected = controller.publishDraft.selected_candidate_ids.includes(
                            candidate.candidate_id
                          );
                          return (
                            <div
                              key={candidate.candidate_id}
                              className={`mc-connectors-candidate${
                                selected ? " is-selected" : ""
                              }${candidate.review_blocked ? " is-blocked" : ""}`}
                            >
                              <label className="mc-connectors-checkbox">
                                <input
                                  type="checkbox"
                                  checked={selected}
                                  disabled={candidate.review_blocked}
                                  onChange={() =>
                                    controller.togglePublishCandidate(candidate.candidate_id)
                                  }
                                />
                              </label>
                              <div className="mc-connectors-candidate-main">
                                <div className="mc-connectors-card-head">
                                  <strong>{candidate.display_name}</strong>
                                  <div className="mc-connectors-inline-chips">
                                    <Chip
                                      label={humanize(candidate.write_classification)}
                                      tone={toneForStatus(candidate.write_classification)}
                                    />
                                    {candidate.review_blocked ? (
                                      <Chip label="blocked" tone="down" />
                                    ) : null}
                                  </div>
                                </div>
                                <p>{candidate.description ?? candidate.operation_key}</p>
                                {candidate.review_block_reason ? (
                                  <p className="mc-connectors-inline-error">
                                    {candidate.review_block_reason}
                                  </p>
                                ) : null}
                                <div className="mc-connectors-field">
                                  <span>Alias override</span>
                                  <input
                                    value={
                                      controller.publishDraft.alias_overrides[
                                        candidate.candidate_id
                                      ] ?? ""
                                    }
                                    disabled={candidate.review_blocked}
                                    onChange={(event) =>
                                      controller.setPublishAlias(
                                        candidate.candidate_id,
                                        event.target.value
                                      )
                                    }
                                    placeholder={candidate.proposed_tool_name}
                                  />
                                </div>
                              </div>
                            </div>
                          );
                        })}
                      </div>

                      {controller.selectedConversion.normalization_notes.length > 0 ? (
                        <div className="mc-connectors-note-stack">
                          {controller.selectedConversion.normalization_notes.map((note) => (
                            <p key={note}>{note}</p>
                          ))}
                        </div>
                      ) : null}

                      <details className="mc-connectors-json-panel">
                        <summary>Diff from previous</summary>
                        <pre>{stringifyJson(controller.selectedConversion.diff_from_previous)}</pre>
                      </details>
                    </>
                  )}
                </>
              )}
            </Surface>

            <Surface
              className="mc-connectors-panel"
              title="Assignments + auth"
              subtitle="Own the connector once, then expose it to the right agents with the right auth shape."
            >
              {!controller.selectedConnector ? (
                <EmptyState message="Select a connector to manage assignments and auth bindings." />
              ) : (
                <div className="mc-connectors-stack">
                  <section className="mc-connectors-section">
                    <div className="mc-connectors-section-head">
                      <h3>Assignments</h3>
                      <button
                        type="button"
                        className="secondary"
                        data-testid="connectors-assignment-submit"
                        disabled={isBusy}
                        onClick={() => {
                          void controller.saveAssignmentDraft();
                        }}
                      >
                        Save assignment
                      </button>
                    </div>
                    <div className="mc-connectors-list">
                      {(controller.selectedConnectorDetail?.assignments ?? []).length === 0 ? (
                        <EmptyState message="No agent assignments have been saved yet." />
                      ) : (
                        controller.selectedConnectorDetail?.assignments.map((item) => (
                          <div key={item.assignment_id} className="mc-connectors-list-row">
                            <div>
                              <div className="mc-connectors-list-title">
                                <strong>
                                  {agentsById.get(item.agent_id)?.name ?? item.agent_id}
                                </strong>
                                <Chip
                                  label={item.enabled ? "enabled" : "disabled"}
                                  tone={item.enabled ? "up" : "down"}
                                />
                              </div>
                              <div className="mc-connectors-list-meta">
                                <span>{humanize(item.auth_mode)}</span>
                                <span>{formatTimestamp(item.updated_at)}</span>
                              </div>
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                    <div className="mc-connectors-form-grid">
                      <label className="mc-connectors-field">
                        <span>Agent</span>
                        <select
                          data-testid="connectors-assignment-agent"
                          value={controller.assignmentDraft.agent_id}
                          onChange={(event) =>
                            controller.updateAssignmentDraft({ agent_id: event.target.value })
                          }
                        >
                          {controller.agents.map((agent) => (
                            <option key={agent.agent_id} value={agent.agent_id}>
                              {agent.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="mc-connectors-field">
                        <span>Auth mode</span>
                        <select
                          value={controller.assignmentDraft.auth_mode}
                          onChange={(event) =>
                            controller.updateAssignmentDraft({ auth_mode: event.target.value })
                          }
                        >
                          {CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS.map((option) => (
                            <option key={option} value={option}>
                              {option}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="mc-connectors-checkbox mc-connectors-checkbox-card">
                        <input
                          type="checkbox"
                          checked={controller.assignmentDraft.enabled}
                          onChange={(event) =>
                            controller.updateAssignmentDraft({ enabled: event.target.checked })
                          }
                        />
                        <span>Assignment enabled</span>
                      </label>
                    </div>
                  </section>

                  <section className="mc-connectors-section">
                    <div className="mc-connectors-section-head">
                      <h3>Auth bindings</h3>
                      <button
                        type="button"
                        className="secondary"
                        data-testid="connectors-auth-binding-submit"
                        disabled={isBusy}
                        onClick={() => {
                          void controller.saveAuthBinding();
                        }}
                      >
                        Save binding
                      </button>
                    </div>
                    <div className="mc-connectors-list">
                      {(controller.selectedConnectorDetail?.auth_bindings ?? []).length === 0 ? (
                        <EmptyState message="No shared or agent-scoped auth bindings are on record." />
                      ) : (
                        controller.selectedConnectorDetail?.auth_bindings.map((item) => (
                          <div key={item.auth_binding_id} className={`mc-connectors-list-row${item.agent_id ? "" : " mc-connectors-shared-binding"}`}>
                            <div>
                              <div className="mc-connectors-list-title">
                                <strong>
                                  {item.agent_id
                                    ? agentsById.get(item.agent_id)?.name ?? item.agent_id
                                    : "\u2731 Shared default"}
                                </strong>
                                <Chip
                                  label={humanize(item.status)}
                                  tone={toneForStatus(item.status)}
                                />
                              </div>
                              <div className="mc-connectors-list-meta">
                                <span>{item.auth_kind}</span>
                                <span>{item.secret_ref ?? "no secret ref"}</span>
                                <span>{formatTimestamp(item.updated_at)}</span>
                              </div>
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                    <div className="mc-connectors-form-grid">
                      <label className="mc-connectors-field">
                        <span>Scope</span>
                        <select
                          value={controller.authBindingDraft.agent_id}
                          onChange={(event) =>
                            controller.updateAuthBindingDraft({ agent_id: event.target.value })
                          }
                        >
                          <option value="">Shared default</option>
                          {controller.agents.map((agent) => (
                            <option key={agent.agent_id} value={agent.agent_id}>
                              {agent.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="mc-connectors-field">
                        <span>Auth kind</span>
                        <select
                          value={controller.authBindingDraft.auth_kind}
                          onChange={(event) =>
                            controller.updateAuthBindingDraft({ auth_kind: event.target.value })
                          }
                        >
                          {CONNECTOR_AUTH_KIND_OPTIONS.map((option) => (
                            <option key={option} value={option}>
                              {option}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="mc-connectors-field">
                        <span>Status</span>
                        <select
                          value={controller.authBindingDraft.status}
                          onChange={(event) =>
                            controller.updateAuthBindingDraft({ status: event.target.value })
                          }
                        >
                          {CONNECTOR_AUTH_STATUS_OPTIONS.map((option) => (
                            <option key={option} value={option}>
                              {option}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="mc-connectors-field">
                        <span>Secret ref</span>
                        <input
                          data-testid="connectors-auth-secret-ref"
                          value={controller.authBindingDraft.secret_ref}
                          onChange={(event) =>
                            controller.updateAuthBindingDraft({ secret_ref: event.target.value })
                          }
                          placeholder="secrets/connectors/github"
                        />
                      </label>
                      <label className="mc-connectors-field mc-connectors-field-span">
                        <span>OAuth session id</span>
                        <input
                          value={controller.authBindingDraft.oauth_session_id}
                          onChange={(event) =>
                            controller.updateAuthBindingDraft({
                              oauth_session_id: event.target.value,
                            })
                          }
                          placeholder="optional oauth session id"
                        />
                      </label>
                      <label className="mc-connectors-field mc-connectors-field-span">
                        <span>Auth metadata JSON</span>
                        <textarea
                          rows={CONNECTOR_JSON_TEXTAREA_ROWS}
                          value={controller.authBindingDraft.auth_metadata_text}
                          onChange={(event) =>
                            controller.updateAuthBindingDraft({
                              auth_metadata_text: event.target.value,
                            })
                          }
                          placeholder='{"header_name":"Authorization"}'
                        />
                      </label>
                    </div>
                  </section>
                </div>
              )}
            </Surface>

            <Surface
              className="mc-connectors-panel"
              title="Health + live tools"
              subtitle="Read current runtime posture, then inspect the tool payload that is actually live."
              headerRight={
                <button
                  type="button"
                  className="ghost"
                  data-testid="connectors-health-refresh"
                  disabled={isBusy || !controller.selectedConnector}
                  onClick={() => {
                    void controller.refreshHealth();
                  }}
                >
                  <RefreshCw size={14} />
                  <span>Refresh health</span>
                </button>
              }
            >
              {!controller.selectedConnector ? (
                <EmptyState message="Select a connector to inspect health and published tools." />
              ) : (
                <div className="mc-connectors-stack">
                  <div className="mc-connectors-health-strip">
                    <div className="mc-connectors-health-stat">
                      <strong>Status</strong>
                      <span>{humanize(controller.health?.status ?? controller.selectedConnector.status)}</span>
                    </div>
                    <div className="mc-connectors-health-stat">
                      <strong>Health</strong>
                      <span>
                        <Chip
                          label={
                            controller.health?.degraded_reason
                              ? "degraded"
                              : controller.health?.auth_required
                                ? "auth required"
                                : "ready"
                          }
                          tone={toneForHealth(
                            controller.health?.status,
                            controller.health?.degraded_reason,
                            controller.health?.auth_required ?? false
                          )}
                        />
                      </span>
                    </div>
                    <div className="mc-connectors-health-stat">
                      <strong>Checked</strong>
                      <span>{formatTimestamp(controller.health?.last_checked_at)}</span>
                    </div>
                  </div>

                  {controller.healthError ? (
                    <p className="mc-connectors-inline-error">{controller.healthError}</p>
                  ) : controller.health?.degraded_reason ? (
                    <p className="mc-connectors-inline-error">{controller.health.degraded_reason}</p>
                  ) : null}

                  <div className="mc-connectors-list">
                    {(controller.selectedConnectorDetail?.published_tools ?? []).length === 0 ? (
                      <EmptyState message="No published tools are live for this connector yet." />
                    ) : (
                      controller.selectedConnectorDetail?.published_tools.map((tool) => (
                        <ToolSelectionRow
                          key={tool.published_tool_id}
                          tool={tool}
                          selected={tool.published_tool_id === controller.selectedPublishedToolId}
                          checked={controller.selectedPublishedToolIds.includes(
                            tool.published_tool_id
                          )}
                          onSelect={() => controller.selectPublishedTool(tool.published_tool_id)}
                          onToggle={() =>
                            controller.togglePublishedToolSelection(tool.published_tool_id)
                          }
                        />
                      ))
                    )}
                  </div>

                  <div className="mc-connectors-surface-actions">
                    <button
                      type="button"
                      className="danger"
                      disabled={isBusy || controller.selectedPublishedToolIds.length === 0}
                      onClick={() => {
                        setConfirmAction("unpublish");
                      }}
                    >
                      Unpublish selected
                    </button>
                  </div>

                  {controller.toolDetailError ? (
                    <p className="mc-connectors-inline-error">{controller.toolDetailError}</p>
                  ) : toolDetail ? (
                    <div className="mc-connectors-json-stack">
                      <div className="mc-connectors-json-panel">
                        <div className="mc-connectors-json-head">
                          <h3>{toolDetail.display_name}</h3>
                          <div className="mc-connectors-inline-chips">
                            <Chip label={toolDetail.tool_name} tone="checking" />
                            <Chip
                              label={humanize(toolDetail.write_classification)}
                              tone={toneForStatus(toolDetail.write_classification)}
                            />
                          </div>
                        </div>
                        <pre>{stringifyJson(toolDetail.tool_schema)}</pre>
                      </div>
                      <div className="mc-connectors-json-panel">
                        <div className="mc-connectors-json-head">
                          <h3>Origin metadata</h3>
                        </div>
                        <pre>{stringifyJson(toolDetail.origin_metadata)}</pre>
                      </div>
                    </div>
                  ) : (
                    <EmptyState message="Select a published tool to inspect its live schema and origin metadata." />
                  )}
                </div>
              )}
            </Surface>

            <Surface
              className="mc-connectors-panel"
              title="Connector interactions"
              subtitle="Track connector-specific pauses, resumptions, and operator-sensitive handoffs."
            >
              {!controller.selectedConnector ? (
                <EmptyState message="Select a connector to inspect its interaction history." />
              ) : controller.selectedConnectorInteractions.length === 0 ? (
                <EmptyState message="No connector-specific interactions have been recorded yet." />
              ) : (
                <div className="mc-connectors-list">
                  {controller.selectedConnectorInteractions.map((item) => (
                    <div key={item.interaction_id} className="mc-connectors-list-row">
                      <div>
                        <div className="mc-connectors-list-title">
                          <strong>{humanize(item.interaction_kind)}</strong>
                          <Chip label={humanize(item.status)} tone={toneForStatus(item.status)} />
                        </div>
                        <p>{item.prompt_summary}</p>
                        <div className="mc-connectors-list-meta">
                          <span>
                            {item.agent_id
                              ? agentsById.get(item.agent_id)?.name ?? item.agent_id
                              : "shared"}
                          </span>
                          <span>{formatTimestamp(item.updated_at)}</span>
                        </div>
                      </div>
                      <button
                        type="button"
                        className="secondary"
                        disabled={isBusy}
                        onClick={() => {
                          void controller.resumeInteraction(item.interaction_id);
                        }}
                      >
                        Resume
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </Surface>
          </div>
        </div>
      </div>
      <Modal
        open={confirmAction !== null}
        onClose={() => setConfirmAction(null)}
        title={
          confirmAction === "publish"
            ? "Publish connector tools"
            : "Unpublish connector tools"
        }
        subtitle={
          confirmAction === "publish"
            ? "This will update the live tool catalog available to assigned agents."
            : "This will remove selected live tools from assigned agents."
        }
        width="560px"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setConfirmAction(null)}>
              Cancel
            </button>
            <button
              type="button"
              className={confirmAction === "unpublish" ? "danger" : undefined}
              disabled={isBusy}
              onClick={() => {
                const action = confirmAction;
                setConfirmAction(null);
                if (action === "publish") {
                  void controller.publishSelectedTools();
                } else if (action === "unpublish") {
                  void controller.unpublishSelectedTools();
                }
              }}
            >
              {confirmAction === "publish" ? "Publish" : "Unpublish"}
            </button>
          </>
        }
      >
        <div className="mc-connectors-note-stack">
          <p>
            Connector:{" "}
            <strong>{controller.selectedConnector?.display_name ?? "Unknown connector"}</strong>
          </p>
          <p>
            {confirmAction === "publish"
              ? `${controller.publishDraft.selected_candidate_ids.length} proposed tool(s) are selected for publish.`
              : `${controller.selectedPublishedToolIds.length} live tool(s) are selected for unpublish.`}
          </p>
          {confirmAction === "publish" ? (
            <p>
              {controller.publishDraft.enable_after_publish
                ? "The connector will be enabled immediately after publish."
                : "The connector will keep its current enabled state after publish."}
            </p>
          ) : null}
        </div>
      </Modal>
    </section>
  );
}
