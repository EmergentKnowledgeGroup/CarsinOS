import {
  useState,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import clsx from "clsx";
import {
  COCKPIT_WIDGET_PALETTE,
  type CockpitPageLayoutV2,
  type CockpitWidgetKind,
  type CockpitWidgetLayoutV2,
  RUNBOOK_COCKPIT_WIDGET_KINDS,
  STRATEGY_COCKPIT_WIDGET_KINDS,
} from "./cockpitLayout";
import { CockpitCanvas } from "./CockpitCanvas";
import { CockpitEditToolbar } from "./CockpitEditToolbar";
import { WidgetPickerModal } from "./WidgetPickerModal";
import { CustomWidgetBuilderModal } from "./CustomWidgetBuilderModal";
import { EmptyState } from "../../ui/EmptyState";
import { Modal } from "../../ui/Modal";
import type { RuntimeConnectionSettings } from "../../types";
import { useOpsUxRuntimeConfigValue } from "../../lib/opsUxConfig";
import {
  Plus,
  Pencil,
  Check,
  Trash2,
  Copy,
  MoveDown,
  MoveLeft,
  MoveRight,
  MoveUp,
} from "lucide-react";

interface CockpitPageProps {
  isActive?: boolean;
  cockpitPages: CockpitPageLayoutV2[];
  activeCockpitPage: CockpitPageLayoutV2;
  editMode: boolean;
  onSetEditMode: (editing: boolean) => void;
  onSetActiveCockpitPageId: (pageId: string) => void;
  onRenameCockpitPage: (pageId: string, name: string) => void;
  onAddCockpitPage: () => void;
  onDeleteCockpitPage: (pageId: string) => void;
  onDuplicateCockpitPage: (pageId: string) => void;
  onExportCockpitLayout: () => void;
  onImportCockpitLayout: (file: File) => Promise<void>;
  onResetCockpitLayout: () => void;
  onLoadTemplate: () => void;
  onAddCockpitWidget: (widget: CockpitWidgetKind) => void;
  onAddCustomWidget: (widget: CockpitWidgetLayoutV2) => void;
  onRemoveCockpitWidget: (instanceId: string) => void;
  onAutoFitCockpitLayout: () => void;
  onNudgeCockpitWidget: (
    instanceId: string,
    delta: { x?: number; y?: number }
  ) => void;
  onLayoutChange: (layout: Array<{ i: string; x: number; y: number; w: number; h: number }>) => void;
  renderCockpitWidget: (widget: CockpitWidgetLayoutV2) => ReactNode;
  settings: RuntimeConnectionSettings;
  strategyEnabled?: boolean;
  runbookEnabled?: boolean;
}

const STRATEGY_WIDGET_KIND_SET = new Set<CockpitWidgetKind>(
  STRATEGY_COCKPIT_WIDGET_KINDS
);
const RUNBOOK_WIDGET_KIND_SET = new Set<CockpitWidgetKind>(
  RUNBOOK_COCKPIT_WIDGET_KINDS
);

function renderCockpitEditHint(widget: CockpitWidgetLayoutV2) {
  const kindLabel =
    widget.widget === "custom"
      ? "Custom widget"
      : `${String(widget.widget).replaceAll("_", " ")} widget`;
  return (
    <div className="mc-cockpit-widget-edit-hint" aria-hidden="true">
      <strong>{kindLabel}</strong>
      <span>Drag anywhere. Resize from right or bottom edges.</span>
    </div>
  );
}

export function CockpitPage(props: CockpitPageProps) {
  const { editMode, onAutoFitCockpitLayout } = props;
  const [removeWidgetId, setRemoveWidgetId] = useState<string | null>(null);
  const [widgetPickerOpen, setWidgetPickerOpen] = useState(false);
  const [customBuilderOpen, setCustomBuilderOpen] = useState(false);
  const [contextMenuPageId, setContextMenuPageId] = useState<string | null>(null);
  const [contextMenuPos, setContextMenuPos] = useState({ x: 0, y: 0 });
  const [contextMenuTriggerPageId, setContextMenuTriggerPageId] = useState<string | null>(null);
  const [renamingPageId, setRenamingPageId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const opsUxRuntime = useOpsUxRuntimeConfigValue();
  const contextMenuRef = useRef<HTMLDivElement | null>(null);

  const handlePageContext = useCallback(
    (e: React.MouseEvent | React.KeyboardEvent, pageId: string) => {
      e.preventDefault();
      setContextMenuPageId(pageId);
      setContextMenuTriggerPageId(pageId);
      if ("clientX" in e) {
        setContextMenuPos({ x: e.clientX, y: e.clientY });
        return;
      }
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      setContextMenuPos({ x: rect.right + 8, y: rect.top + rect.height / 2 });
    },
    [],
  );

  const closeContextMenu = useCallback((options: { restoreFocus?: boolean } = {}) => {
    const shouldRestoreFocus = options.restoreFocus ?? true;
    setContextMenuPageId(null);
    const triggerPageId = contextMenuTriggerPageId;
    setContextMenuTriggerPageId(null);
    if (!shouldRestoreFocus || !triggerPageId) {
      return;
    }
    window.requestAnimationFrame(() => {
      const trigger = document.querySelector<HTMLElement>(
        `[data-cockpit-page-tab="${triggerPageId}"]`
      );
      trigger?.focus();
    });
  }, [contextMenuTriggerPageId]);

  useEffect(() => {
    if (!contextMenuPageId) {
      return;
    }
    const firstButton = contextMenuRef.current?.querySelector<HTMLElement>("button");
    firstButton?.focus();
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeContextMenu();
      }
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [closeContextMenu, contextMenuPageId]);

  const startRename = useCallback(
    (pageId: string, currentName: string) => {
      setRenamingPageId(pageId);
      setRenameValue(currentName);
      closeContextMenu({ restoreFocus: false });
    },
    [closeContextMenu],
  );

  const confirmRename = useCallback(() => {
    if (renamingPageId && renameValue.trim()) {
      props.onRenameCockpitPage(renamingPageId, renameValue.trim());
    }
    setRenamingPageId(null);
  }, [renamingPageId, renameValue, props]);

  const strategyEnabled =
    props.strategyEnabled ?? opsUxRuntime.config.controls.strategy_hub;
  const runbookEnabled =
    props.runbookEnabled ?? opsUxRuntime.config.controls.runbook_hub;
  const visibleWidgets = useMemo(
    () =>
      props.activeCockpitPage.widgets.filter((widget) => {
        if (widget.widget === "custom") {
          return true;
        }
        if (!strategyEnabled && STRATEGY_WIDGET_KIND_SET.has(widget.widget)) {
          return false;
        }
        if (!runbookEnabled && RUNBOOK_WIDGET_KIND_SET.has(widget.widget)) {
          return false;
        }
        return true;
      }),
    [props.activeCockpitPage.widgets, runbookEnabled, strategyEnabled]
  );
  const hiddenWidgetCount =
    props.activeCockpitPage.widgets.length - visibleWidgets.length;
  const hasWidgets = visibleWidgets.length > 0;
  const availableWidgetKinds =
    strategyEnabled && runbookEnabled
      ? undefined
      : COCKPIT_WIDGET_PALETTE.filter((entry) => {
          if (!strategyEnabled && STRATEGY_WIDGET_KIND_SET.has(entry.widget)) {
            return false;
          }
          if (!runbookEnabled && RUNBOOK_WIDGET_KIND_SET.has(entry.widget)) {
            return false;
          }
          return true;
        }).map((entry) => entry.widget);
  const visibleWidgetIds = useMemo(
    () => visibleWidgets.map((widget) => widget.instance_id),
    [visibleWidgets]
  );
  const previousEditModeRef = useRef(editMode);

  useEffect(() => {
    const enteredEditMode = !previousEditModeRef.current && editMode;
    previousEditModeRef.current = editMode;
    if (!enteredEditMode || visibleWidgets.length === 0) {
      return;
    }
    const bottomEdge = visibleWidgets.reduce(
      (maxRows, widget) => Math.max(maxRows, widget.position.y + widget.position.h),
      0,
    );
    const expectedRows = Math.max(4, Math.ceil(visibleWidgets.length / 5) * 4);
    if (bottomEdge > expectedRows + 2) {
      onAutoFitCockpitLayout();
    }
  }, [editMode, onAutoFitCockpitLayout, visibleWidgets]);

  return (
    <section className="mc-cockpit-grid">
      {/* ── SLIM SIDEBAR ── */}
      <nav className="mc-cockpit-sidebar-slim">
        <div className="mc-cockpit-sidebar-pages">
          {props.cockpitPages.map((page) => {
            const isActive = page.page_id === props.activeCockpitPage.page_id;
            const letter = page.name.charAt(0).toUpperCase();

            if (renamingPageId === page.page_id) {
              return (
                <div key={page.page_id} className="mc-cockpit-page-rename">
                  <input
                    autoFocus
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onBlur={confirmRename}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") confirmRename();
                      if (e.key === "Escape") setRenamingPageId(null);
                    }}
                  />
                </div>
              );
            }

            return (
              <button
                key={page.page_id}
                type="button"
                className={clsx(
                  "mc-cockpit-page-tab",
                  isActive && "mc-cockpit-page-tab-active",
                )}
                onClick={() => props.onSetActiveCockpitPageId(page.page_id)}
                onContextMenu={(e) => handlePageContext(e, page.page_id)}
                onKeyDown={(event) => {
                  if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                    handlePageContext(event, page.page_id);
                  }
                }}
                title={page.name}
                aria-label={`Cockpit page: ${page.name}`}
                aria-current={isActive ? "page" : undefined}
                data-cockpit-page-tab={page.page_id}
              >
                <span className="mc-cockpit-page-tab-letter">{letter}</span>
              </button>
            );
          })}
        </div>

        <div className="mc-cockpit-sidebar-actions">
          <button
            type="button"
            className="mc-cockpit-sidebar-btn"
            onClick={props.onAddCockpitPage}
            title="Add page"
            aria-label="Add cockpit page"
          >
            <Plus size={16} />
          </button>
          <button
            type="button"
            className={clsx(
              "mc-cockpit-sidebar-btn",
              props.editMode && "mc-cockpit-sidebar-btn-active",
            )}
            onClick={() => props.onSetEditMode(!props.editMode)}
            title={props.editMode ? "Done editing" : "Edit layout"}
            aria-label={props.editMode ? "Exit cockpit edit mode" : "Enter cockpit edit mode"}
            aria-pressed={props.editMode}
          >
            {props.editMode ? <Check size={16} /> : <Pencil size={16} />}
          </button>
        </div>
      </nav>

      {/* ── CANVAS AREA ── */}
      <div className="mc-cockpit-canvas-area">
        {/* Floating edit toolbar — only in edit mode */}
        {props.editMode ? (
          <CockpitEditToolbar
            onOpenWidgetPicker={() => setWidgetPickerOpen(true)}
            onAutoFitLayout={props.onAutoFitCockpitLayout}
            onStartRename={() =>
              startRename(
                props.activeCockpitPage.page_id,
                props.activeCockpitPage.name,
              )
            }
            onExportLayout={props.onExportCockpitLayout}
            onImportLayout={props.onImportCockpitLayout}
            onDeletePage={() =>
              props.onDeleteCockpitPage(props.activeCockpitPage.page_id)
            }
            onLoadTemplate={props.onLoadTemplate}
            onResetLayout={props.onResetCockpitLayout}
            canDeletePage={props.cockpitPages.length > 1}
          />
        ) : null}

        {hasWidgets ? (
          <CockpitCanvas
            key={`${props.activeCockpitPage.page_id}-${
              props.isActive ? "active" : "hidden"
            }-${props.editMode ? "edit" : "view"}`}
            widgets={visibleWidgets}
            editMode={props.editMode}
            onLayoutChange={props.onLayoutChange}
          >
            {visibleWidgets.map((widget) => (
              <div
                key={widget.instance_id}
                className={clsx(
                  "mc-cockpit-widget",
                  props.editMode && "mc-cockpit-widget-editing",
                )}
              >
                <header className="mc-cockpit-widget-head">
                  {props.editMode ? (
                    <div className="mc-widget-edit-controls">
                      <div className="mc-widget-nudge-controls" aria-label="Move widget">
                        <button
                          type="button"
                          className="mc-widget-nudge-btn"
                          aria-label="Move widget left"
                          title="Move widget left"
                          onClick={() => props.onNudgeCockpitWidget(widget.instance_id, { x: -1 })}
                        >
                          <MoveLeft size={14} />
                        </button>
                        <button
                          type="button"
                          className="mc-widget-nudge-btn"
                          aria-label="Move widget up"
                          title="Move widget up"
                          onClick={() => props.onNudgeCockpitWidget(widget.instance_id, { y: -1 })}
                        >
                          <MoveUp size={14} />
                        </button>
                        <button
                          type="button"
                          className="mc-widget-nudge-btn"
                          aria-label="Move widget right"
                          title="Move widget right"
                          onClick={() => props.onNudgeCockpitWidget(widget.instance_id, { x: 1 })}
                        >
                          <MoveRight size={14} />
                        </button>
                        <button
                          type="button"
                          className="mc-widget-nudge-btn"
                          aria-label="Move widget down"
                          title="Move widget down"
                          onClick={() => props.onNudgeCockpitWidget(widget.instance_id, { y: 1 })}
                        >
                          <MoveDown size={14} />
                        </button>
                      </div>
                    </div>
                  ) : null}
                  <h3>{widget.title}</h3>
                  {props.editMode ? (
                    <button
                      type="button"
                      className="mc-widget-remove-btn"
                      onClick={() => setRemoveWidgetId(widget.instance_id)}
                      title="Remove widget"
                    >
                      <Trash2 size={14} />
                    </button>
                  ) : null}
                </header>
                {props.editMode ? (
                  <div className="mc-cockpit-widget-order-hint">
                    Position {visibleWidgetIds.indexOf(widget.instance_id) + 1} of{" "}
                    {visibleWidgetIds.length}
                  </div>
                ) : null}
                <div className="mc-cockpit-widget-live-preview">
                  {props.renderCockpitWidget(widget)}
                  {props.editMode ? renderCockpitEditHint(widget) : null}
                </div>
              </div>
            ))}
          </CockpitCanvas>
        ) : (
          <div className="mc-cockpit-empty-canvas">
            <EmptyState
              className="mc-cockpit-empty-message"
              message={
                hiddenWidgetCount > 0
                  ? "Some widgets are hidden because their features are turned off. Enable them in Config > Reliability + Rollout."
                  : "No dashboard set up yet. This is optional \u2014 most users skip it at first."
              }
            />
            <p className="mc-cockpit-empty-hint">
              Want a quick start? Load a pre-built template, or add individual widgets.
            </p>
            <div className="mc-cockpit-empty-actions">
              <button
                type="button"
                onClick={props.onLoadTemplate}
              >
                Load Ops Template
              </button>
              <button
                type="button"
                className="ghost"
                onClick={() => setWidgetPickerOpen(true)}
              >
                Add Individual Widget
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── CONTEXT MENU ── */}
      {contextMenuPageId ? (
        <>
          <div className="mc-context-backdrop" onClick={() => closeContextMenu()} />
          <div
            ref={contextMenuRef}
            className="mc-context-menu"
            style={{ left: contextMenuPos.x, top: contextMenuPos.y }}
            role="menu"
          >
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                const page = props.cockpitPages.find(
                  (p) => p.page_id === contextMenuPageId,
                );
                if (page) startRename(contextMenuPageId, page.name);
              }}
            >
              <Pencil size={14} />
              Rename
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                props.onDuplicateCockpitPage(contextMenuPageId);
                closeContextMenu();
              }}
            >
              <Copy size={14} />
              Duplicate
            </button>
            {props.cockpitPages.length > 1 ? (
              <button
                type="button"
                className="danger"
                role="menuitem"
                onClick={() => {
                  props.onDeleteCockpitPage(contextMenuPageId);
                  closeContextMenu();
                }}
              >
                <Trash2 size={14} />
                Delete
              </button>
            ) : null}
          </div>
        </>
      ) : null}

      {/* ── WIDGET PICKER MODAL ── */}
      <WidgetPickerModal
        open={widgetPickerOpen}
        onClose={() => setWidgetPickerOpen(false)}
        onAddWidget={props.onAddCockpitWidget}
        availableWidgets={availableWidgetKinds}
        onOpenCustomBuilder={() => {
          setWidgetPickerOpen(false);
          setCustomBuilderOpen(true);
        }}
      />

      {/* ── CUSTOM WIDGET BUILDER MODAL ── */}
      <CustomWidgetBuilderModal
        open={customBuilderOpen}
        onClose={() => setCustomBuilderOpen(false)}
        onAddWidget={props.onAddCustomWidget}
        settings={props.settings}
      />

      {/* ── CONFIRM REMOVE WIDGET ── */}
      <Modal
        open={removeWidgetId !== null}
        onClose={() => setRemoveWidgetId(null)}
        title="Remove Widget"
        subtitle="This will remove the widget from the current page."
        footer={
          <>
            <button
              type="button"
              className="ghost"
              onClick={() => setRemoveWidgetId(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="danger"
              onClick={() => {
                if (removeWidgetId) {
                  props.onRemoveCockpitWidget(removeWidgetId);
                }
                setRemoveWidgetId(null);
              }}
            >
              Remove
            </button>
          </>
        }
      >
        <p>
          Are you sure you want to remove{" "}
          <strong>
            {props.activeCockpitPage.widgets.find(
              (w) => w.instance_id === removeWidgetId,
            )?.title ?? "this widget"}
          </strong>
          ?
        </p>
      </Modal>
    </section>
  );
}
