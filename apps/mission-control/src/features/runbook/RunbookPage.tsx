import {
  AlertTriangle,
  ArrowRight,
  Bot,
  Clock3,
  Compass,
  Gauge,
  Kanban,
  Link2,
  ListTree,
  Milestone,
  RefreshCw,
  TimerReset,
  Workflow,
} from "lucide-react";
import type { ReactNode } from "react";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Surface } from "../../ui/Surface";
import type {
  Agent,
  RunbookActionResponse,
  RunbookDeepLinkTargetResponse,
  RunbookEntityRefResponse,
  RunbookHistoryItemResponse,
  RunbookStepResponse,
} from "../../types";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import {
  RUNBOOK_HISTORY_PREVIEW_LIMIT,
  RUNBOOK_KIND_OPTIONS,
  RUNBOOK_STATUS_OPTIONS,
} from "./runbookConfig";
import type { useRunbookController } from "./useRunbookController";

interface RunbookPageProps {
  controller: ReturnType<typeof useRunbookController>;
  agents: Agent[];
  onOpenDeepLink: (target: RunbookDeepLinkTargetResponse) => void;
}

function toneForStatus(
  status: string
): "up" | "down" | "warning" | "checking" | "" {
  switch (status) {
    case "completed":
      return "up";
    case "failed":
    case "blocked":
      return "down";
    case "waiting":
    case "limited":
      return "warning";
    case "active":
      return "checking";
    default:
      return "";
  }
}

function iconForRunbookKind(kind: string) {
  switch (kind) {
    case "assistant_session_run":
      return <Bot size={16} />;
    case "board_card_run":
      return <Kanban size={16} />;
    case "scheduled_job_run":
      return <Gauge size={16} />;
    case "strategy_task_execution":
      return <Compass size={16} />;
    default:
      return <Workflow size={16} />;
  }
}

function labelForRunbookKind(kind: string): string {
  return (
    RUNBOOK_KIND_OPTIONS.find((option) => option.value === kind)?.label ?? "Runbook"
  );
}

function RunbookStatePanel({
  title,
  detail,
}: {
  title: string;
  detail: string;
}) {
  return (
    <section className="mc-runbook-page" data-testid="runbook-page">
      <Surface className="mc-runbook-state" title={title} subtitle={detail}>
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
  onClick,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  detail: string;
  onClick?: () => void;
}) {
  const Tag = onClick ? "button" : "div";
  return (
    <Tag
      type={onClick ? "button" : undefined}
      className={`mc-runbook-summary-card${onClick ? " is-action" : ""}`}
      onClick={onClick}
    >
      <div className="mc-runbook-summary-kicker">
        {icon}
        <span>{label}</span>
      </div>
      <strong>{value}</strong>
      <p>{detail}</p>
    </Tag>
  );
}

function StepStateDot({ state }: { state: string }) {
  return <span className={`mc-runbook-step-dot is-${state}`} aria-hidden="true" />;
}

function EntityLink({
  entity,
  onOpenDeepLink,
}: {
  entity: RunbookEntityRefResponse;
  onOpenDeepLink: (target: RunbookDeepLinkTargetResponse) => void;
}) {
  return (
    <button
      type="button"
      className="mc-runbook-entity-link"
      onClick={() => onOpenDeepLink(entity.deep_link)}
      title={`${entity.entity_kind}: ${entity.display_label}`}
    >
      <span>{entity.display_label}</span>
      <ArrowRight size={12} />
    </button>
  );
}

function ActionButton({
  action,
  onOpenDeepLink,
}: {
  action: RunbookActionResponse;
  onOpenDeepLink: (target: RunbookDeepLinkTargetResponse) => void;
}) {
  const target = action.target_entity_ref;
  const disabled = action.availability !== "enabled" || !target;
  return (
    <button
      type="button"
      className="mc-runbook-action-btn"
      disabled={disabled}
      title={action.disabled_reason ?? action.label}
      onClick={() => {
        if (target) {
          onOpenDeepLink(target.deep_link);
        }
      }}
    >
      <span>{action.label}</span>
      {action.availability !== "enabled" ? (
        <span className="mc-runbook-action-reason">{action.disabled_reason ?? action.availability}</span>
      ) : null}
    </button>
  );
}

function FlowStep({
  step,
  onOpenDeepLink,
}: {
  step: RunbookStepResponse;
  onOpenDeepLink: (target: RunbookDeepLinkTargetResponse) => void;
}) {
  return (
    <article className={`mc-runbook-step is-${step.state}`}>
      <div className="mc-runbook-step-rail">
        <StepStateDot state={step.state} />
      </div>
      <div className="mc-runbook-step-body">
        <div className="mc-runbook-step-head">
          <div>
            <strong>{step.label}</strong>
            <p>{step.kind.replaceAll("_", " ")}</p>
          </div>
          <Chip label={step.state} tone={toneForStatus(step.state)} />
        </div>
        {step.state_reason ? (
          <p className="mc-runbook-step-reason">{step.state_reason}</p>
        ) : null}
        <div className="mc-runbook-step-meta">
          <span>Started {formatRelative(step.started_at_ms)}</span>
          <span>Finished {formatRelative(step.finished_at_ms)}</span>
          <span>Waiting {formatRelative(step.waiting_since_ms)}</span>
        </div>
        {step.linked_entity_refs.length > 0 ? (
          <div className="mc-runbook-entity-row">
            {step.linked_entity_refs.map((entity) => (
              <EntityLink
                key={`${step.step_id}-${entity.entity_kind}-${entity.entity_id}`}
                entity={entity}
                onOpenDeepLink={onOpenDeepLink}
              />
            ))}
          </div>
        ) : null}
      </div>
    </article>
  );
}

function HistoryItem({
  item,
  onOpenDeepLink,
}: {
  item: RunbookHistoryItemResponse;
  onOpenDeepLink: (target: RunbookDeepLinkTargetResponse) => void;
}) {
  return (
    <article className="mc-runbook-history-item">
      <div className="mc-runbook-history-meta">
        <span>{item.label}</span>
        <time title={formatDateTime(item.occurred_at_ms)}>
          {formatRelative(item.occurred_at_ms)}
        </time>
      </div>
      {item.detail ? <p>{item.detail}</p> : null}
      {item.entity_refs.length > 0 ? (
        <div className="mc-runbook-entity-row">
          {item.entity_refs.map((entity) => (
            <EntityLink
              key={`${item.history_id}-${entity.entity_kind}-${entity.entity_id}`}
              entity={entity}
              onOpenDeepLink={onOpenDeepLink}
            />
          ))}
        </div>
      ) : null}
    </article>
  );
}

export function RunbookPage({ controller, agents, onOpenDeepLink }: RunbookPageProps) {
  if (!controller.enabled || controller.availability === "disabled") {
    return (
      <RunbookStatePanel
        title="Runbook hub is disabled"
        detail="Enable Runbook hub in Config > Reliability + Rollout to expose live visual runbooks."
      />
    );
  }

  if (controller.availability === "unsupported") {
    return (
      <RunbookStatePanel
        title="Runbook surface unavailable"
        detail={
          controller.availabilityMessage ??
          "The connected gateway does not expose the Runbook contracts yet."
        }
      />
    );
  }

  if (controller.availability === "error") {
    return (
      <RunbookStatePanel
        title="Runbook failed to load"
        detail={controller.availabilityMessage ?? "Runbook could not load."}
      />
    );
  }

  if (controller.availability === "loading" && controller.items.length === 0) {
    return (
      <RunbookStatePanel
        title="Loading Runbook"
        detail="Building the latest execution map from runs, approvals, jobs, board cards, and strategy tasks."
      />
    );
  }

  const detail = controller.detail;
  const ownerLabel =
    detail?.owner_agent_label ??
    (detail?.owner_agent_id
      ? agents.find((agent) => agent.agent_id === detail.owner_agent_id)?.name ??
        detail.owner_agent_id
      : null);

  return (
    <section className="mc-runbook-page" data-testid="runbook-page">
      <div className="mc-runbook-summary-strip">
        <SummaryCard
          icon={<ListTree size={16} />}
          label="Pending"
          value={String(controller.countsByStatus.pending)}
          detail="Defined but not executing yet."
          onClick={() => controller.setFilters({ status: "pending" })}
        />
        <SummaryCard
          icon={<RefreshCw size={16} />}
          label="Active"
          value={String(controller.countsByStatus.active)}
          detail="Currently executing."
          onClick={() => controller.setFilters({ status: "active" })}
        />
        <SummaryCard
          icon={<TimerReset size={16} />}
          label="Waiting"
          value={String(controller.countsByStatus.waiting)}
          detail="Paused on approvals or upstream action."
          onClick={() => controller.setFilters({ status: "waiting" })}
        />
        <SummaryCard
          icon={<AlertTriangle size={16} />}
          label="Blocked"
          value={String(controller.countsByStatus.blocked)}
          detail="Cannot advance without intervention."
          onClick={() => controller.setFilters({ status: "blocked" })}
        />
      </div>

      <div className="mc-runbook-grid">
        <Surface
          className="mc-runbook-sidebar"
          title="Browse Runbooks"
          subtitle={
            controller.generatedAtMs
              ? `Last refresh ${formatRelative(controller.generatedAtMs)}`
              : "Live execution map"
          }
          headerRight={
            controller.isStale ? (
              <Chip label="stale" tone="warning" />
            ) : (
              <Chip label={`${controller.items.length} visible`} tone="" />
            )
          }
        >
          <div className="mc-runbook-filter-bar">
            <label>
              Query
              <input
                value={controller.filters.query}
                onChange={(event) =>
                  controller.setFilters({ query: event.target.value })
                }
                placeholder="Search title, owner, or step"
              />
            </label>
            <label>
              Kind
              <select
                value={controller.filters.kind}
                onChange={(event) =>
                  controller.setFilters({ kind: event.target.value })
                }
              >
                {RUNBOOK_KIND_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Status
              <select
                value={controller.filters.status}
                onChange={(event) =>
                  controller.setFilters({ status: event.target.value })
                }
              >
                {RUNBOOK_STATUS_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Owner
              <select
                value={controller.filters.owner_agent_id}
                onChange={(event) =>
                  controller.setFilters({ owner_agent_id: event.target.value })
                }
              >
                <option value="">All owners</option>
                {agents.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <div className="mc-runbook-sidebar-actions">
            <button type="button" className="ghost" onClick={controller.resetFilters}>
              Reset filters
            </button>
            <button
              type="button"
              className="ghost mc-runbook-refresh-btn"
              onClick={(e) => {
                controller.queueRefresh();
                const btn = e.currentTarget;
                btn.classList.add("is-spinning");
                setTimeout(() => btn.classList.remove("is-spinning"), 800);
              }}
            >
              <RefreshCw size={12} /> Refresh
            </button>
          </div>

          <div className="mc-runbook-list">
            {controller.items.length === 0 ? (
              <EmptyState message="No runbooks match the current filters." />
            ) : null}
            {controller.items.map((item) => (
              <button
                key={item.runbook_id}
                type="button"
                className={`mc-runbook-list-item${
                  item.runbook_id === controller.selectedRunbookId ? " is-active" : ""
                }`}
                onClick={() =>
                  controller.selectRunbook(item.runbook_kind, item.anchor_id)
                }
              >
                <div className="mc-runbook-list-item-head">
                  <div className="mc-runbook-list-title">
                    {iconForRunbookKind(item.runbook_kind)}
                    <strong>{item.title}</strong>
                  </div>
                  <Chip label={item.status} tone={toneForStatus(item.status)} />
                </div>
                <div className="mc-runbook-list-meta">
                  <span>{labelForRunbookKind(item.runbook_kind)}</span>
                  <span>{item.current_step_label ?? "No active step"}</span>
                </div>
                <div className="mc-runbook-list-meta">
                  <span>{item.owner_agent_label ?? item.owner_agent_id ?? "Unassigned"}</span>
                  <span>{formatRelative(item.updated_at_ms)}</span>
                </div>
                {item.status_reason ? <p>{item.status_reason}</p> : null}
                {item.availability.is_limited || item.availability.is_stale ? (
                  <div className="mc-runbook-list-foot">
                    {item.availability.is_limited ? (
                      <Chip label="limited" tone="warning" />
                    ) : null}
                    {item.availability.is_stale ? (
                      <Chip label="stale" tone="warning" />
                    ) : null}
                  </div>
                ) : null}
              </button>
            ))}
          </div>
        </Surface>

        <Surface
          className="mc-runbook-detail"
          title={detail?.title ?? "Runbook Detail"}
          subtitle={
            detail
              ? `${labelForRunbookKind(detail.runbook_kind)} · generated ${formatRelative(
                  detail.generated_at_ms
                )}`
              : "Select a runbook to inspect steps, history, and linked artifacts."
          }
          headerRight={
            detail ? <Chip label={detail.status} tone={toneForStatus(detail.status)} /> : null
          }
        >
          {controller.detailLoading ? (
            <EmptyState message="Loading selected runbook…" />
          ) : null}
          {!controller.detailLoading && controller.detailError ? (
            <EmptyState message={controller.detailError} />
          ) : null}
          {!controller.detailLoading && !controller.detailError && !detail ? (
            <EmptyState message="Select a runbook from the list to inspect the execution flow." />
          ) : null}

          {detail ? (
            <div className="mc-runbook-detail-body">
              <div className="mc-runbook-hero">
                <div className="mc-runbook-hero-main">
                  <div className="mc-runbook-hero-kicker">
                    {iconForRunbookKind(detail.runbook_kind)}
                    <span>{labelForRunbookKind(detail.runbook_kind)}</span>
                  </div>
                  <h3>{detail.title}</h3>
                  <p>{detail.status_reason ?? "No exception noted on the active execution."}</p>
                </div>
                <div className="mc-runbook-hero-side">
                  <Chip label={detail.status} tone={toneForStatus(detail.status)} />
                  {detail.availability.is_limited ? (
                    <Chip label="limited" tone="warning" />
                  ) : null}
                  {detail.availability.is_stale ? (
                    <Chip label="stale" tone="warning" />
                  ) : null}
                </div>
              </div>

              <div className="mc-runbook-fact-row">
                <div className="mc-runbook-fact-card">
                  <span>Owner</span>
                  <strong>{ownerLabel ?? "Unassigned"}</strong>
                </div>
                <div className="mc-runbook-fact-card">
                  <span>Active step</span>
                  <strong>{detail.active_step_id ?? "n/a"}</strong>
                </div>
                <div className="mc-runbook-fact-card">
                  <span>Next valid step</span>
                  <strong>{detail.next_step_ids[0] ?? "terminal"}</strong>
                </div>
                <div className="mc-runbook-fact-card">
                  <span>Execution</span>
                  <strong>
                    {detail.selected_execution_ref?.entity_kind ?? "anchor-only"} ·{" "}
                    {formatRelative(detail.selected_execution_ref?.created_at_ms)}
                  </strong>
                </div>
              </div>

              {detail.warnings.length > 0 ? (
                <div className="mc-runbook-warning-strip">
                  {detail.warnings.map((warning) => (
                    <div key={warning.warning_id} className="mc-runbook-warning-card">
                      <AlertTriangle size={14} />
                      <div>
                        <strong>{warning.warning_kind}</strong>
                        <p>{warning.message}</p>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}

              <div className="mc-runbook-detail-grid">
                <div className="mc-runbook-flow-column">
                  <div className="mc-runbook-section-head">
                    <h4>
                      <Workflow size={16} /> Flow
                    </h4>
                    <span>{detail.steps.length} steps</span>
                  </div>
                  <div className="mc-runbook-flow">
                    {detail.steps.map((step) => (
                      <FlowStep
                        key={step.step_id}
                        step={step}
                        onOpenDeepLink={onOpenDeepLink}
                      />
                    ))}
                  </div>
                </div>

                <div className="mc-runbook-side-column">
                  <div className="mc-runbook-panel">
                    <div className="mc-runbook-section-head">
                      <h4>
                        <Link2 size={16} /> Linked artifacts
                      </h4>
                    </div>
                    <div className="mc-runbook-entity-row">
                      {detail.linked_entities.length > 0 ? (
                        detail.linked_entities.map((entity) => (
                          <EntityLink
                            key={`${entity.entity_kind}-${entity.entity_id}`}
                            entity={entity}
                            onOpenDeepLink={onOpenDeepLink}
                          />
                        ))
                      ) : (
                        <p className="mc-runbook-empty-hint">No linked artifacts yet.</p>
                      )}
                    </div>
                  </div>

                  <div className="mc-runbook-panel">
                    <div className="mc-runbook-section-head">
                      <h4>
                        <ArrowRight size={16} /> Actions
                      </h4>
                    </div>
                    <div className="mc-runbook-action-grid">
                      {detail.actions.map((action) => (
                        <ActionButton
                          key={action.action_id}
                          action={action}
                          onOpenDeepLink={onOpenDeepLink}
                        />
                      ))}
                    </div>
                  </div>

                  <div className="mc-runbook-panel">
                    <div className="mc-runbook-section-head">
                      <h4>
                        <Clock3 size={16} /> History
                      </h4>
                      <span>
                        latest {Math.min(detail.history.length, RUNBOOK_HISTORY_PREVIEW_LIMIT)}
                      </span>
                    </div>
                    <div className="mc-runbook-history">
                      {detail.history
                        .slice(-RUNBOOK_HISTORY_PREVIEW_LIMIT)
                        .reverse()
                        .map((item) => (
                          <HistoryItem
                            key={item.history_id}
                            item={item}
                            onOpenDeepLink={onOpenDeepLink}
                          />
                        ))}
                    </div>
                  </div>

                  <div className="mc-runbook-panel">
                    <div className="mc-runbook-section-head">
                      <h4>
                        <Milestone size={16} /> Source facts
                      </h4>
                    </div>
                    <div className="mc-runbook-source-list">
                      {detail.source_facts.map((fact) => (
                        <article key={fact.fact_id} className="mc-runbook-source-item">
                          <div className="mc-runbook-history-meta">
                            <span>{fact.fact_kind}</span>
                            <time title={formatDateTime(fact.occurred_at_ms)}>
                              {formatRelative(fact.occurred_at_ms)}
                            </time>
                          </div>
                          {fact.entity_ref ? (
                            <EntityLink
                              entity={fact.entity_ref}
                              onOpenDeepLink={onOpenDeepLink}
                            />
                          ) : null}
                          {fact.partial ? <Chip label="partial data" tone="warning" title="Some source fields were unavailable when this fact was recorded" /> : null}
                        </article>
                      ))}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          ) : null}
        </Surface>
      </div>
    </section>
  );
}
