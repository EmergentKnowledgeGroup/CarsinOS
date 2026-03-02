import { useState, useCallback, type ReactNode } from "react";
import clsx from "clsx";
import {
  type CockpitPageLayoutV2,
  type CockpitWidgetKind,
  type CockpitWidgetLayoutV2,
} from "./cockpitLayout";
import { CockpitCanvas } from "./CockpitCanvas";
import { CockpitEditToolbar } from "./CockpitEditToolbar";
import { WidgetPickerModal } from "./WidgetPickerModal";
import { CustomWidgetBuilderModal } from "./CustomWidgetBuilderModal";
import { EmptyState } from "../../ui/EmptyState";
import { Modal } from "../../ui/Modal";
import type { RuntimeConnectionSettings } from "../../types";
import {
  Plus,
  Pencil,
  Check,
  Trash2,
  Copy,
  GripVertical,
} from "lucide-react";

interface CockpitPageProps {
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
  onLayoutChange: (layout: Array<{ i: string; x: number; y: number; w: number; h: number }>) => void;
  renderCockpitWidget: (widget: CockpitWidgetLayoutV2) => ReactNode;
  settings: RuntimeConnectionSettings;
}

export function CockpitPage(props: CockpitPageProps) {
  const [removeWidgetId, setRemoveWidgetId] = useState<string | null>(null);
  const [widgetPickerOpen, setWidgetPickerOpen] = useState(false);
  const [customBuilderOpen, setCustomBuilderOpen] = useState(false);
  const [contextMenuPageId, setContextMenuPageId] = useState<string | null>(null);
  const [contextMenuPos, setContextMenuPos] = useState({ x: 0, y: 0 });
  const [renamingPageId, setRenamingPageId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const handlePageContext = useCallback(
    (e: React.MouseEvent, pageId: string) => {
      e.preventDefault();
      setContextMenuPageId(pageId);
      setContextMenuPos({ x: e.clientX, y: e.clientY });
    },
    [],
  );

  const closeContextMenu = useCallback(() => {
    setContextMenuPageId(null);
  }, []);

  const startRename = useCallback(
    (pageId: string, currentName: string) => {
      setRenamingPageId(pageId);
      setRenameValue(currentName);
      closeContextMenu();
    },
    [closeContextMenu],
  );

  const confirmRename = useCallback(() => {
    if (renamingPageId && renameValue.trim()) {
      props.onRenameCockpitPage(renamingPageId, renameValue.trim());
    }
    setRenamingPageId(null);
  }, [renamingPageId, renameValue, props]);

  const hasWidgets = props.activeCockpitPage.widgets.length > 0;

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
                title={page.name}
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
            widgets={props.activeCockpitPage.widgets}
            editMode={props.editMode}
            onLayoutChange={props.onLayoutChange}
          >
            {props.activeCockpitPage.widgets.map((widget) => (
              <div
                key={widget.instance_id}
                className={clsx(
                  "mc-cockpit-widget",
                  props.editMode && "mc-cockpit-widget-editing",
                )}
              >
                <header className="mc-cockpit-widget-head">
                  {props.editMode ? (
                    <span className="mc-widget-drag-handle">
                      <GripVertical size={14} />
                    </span>
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
                {props.renderCockpitWidget(widget)}
              </div>
            ))}
          </CockpitCanvas>
        ) : (
          <div className="mc-cockpit-empty-canvas">
            <EmptyState
              className="mc-cockpit-empty-message"
              message="Your dashboard is empty."
            />
            <div className="mc-cockpit-empty-actions">
              <button
                type="button"
                onClick={() => setWidgetPickerOpen(true)}
              >
                Add Widget
              </button>
              <button
                type="button"
                className="ghost"
                onClick={props.onLoadTemplate}
              >
                Load Ops Template
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── CONTEXT MENU ── */}
      {contextMenuPageId ? (
        <>
          <div className="mc-context-backdrop" onClick={closeContextMenu} />
          <div
            className="mc-context-menu"
            style={{ left: contextMenuPos.x, top: contextMenuPos.y }}
          >
            <button
              type="button"
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
