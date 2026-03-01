import { useState, type ReactNode } from "react";
import {
  COCKPIT_WIDGET_PALETTE,
  normalizeWidgetSpan,
  type CockpitPageLayout,
  type CockpitWidgetKind,
  type CockpitWidgetLayout,
} from "./cockpitLayout";
import { EmptyState } from "../../ui/EmptyState";
import { InlineActions } from "../../ui/InlineActions";
import { Modal } from "../../ui/Modal";
import { Surface } from "../../ui/Surface";

interface CockpitPageProps {
  cockpitPages: CockpitPageLayout[];
  activeCockpitPage: CockpitPageLayout;
  onSetActiveCockpitPageId: (pageId: string) => void;
  onRenameActiveCockpitPage: (name: string) => void;
  onAddCockpitPage: () => void;
  onExportCockpitLayout: () => void;
  onImportCockpitLayout: (file: File) => Promise<void>;
  onResetCockpitLayout: () => void;
  onAddCockpitWidget: (widget: CockpitWidgetKind) => void;
  onMoveCockpitWidget: (instanceId: string, delta: number) => void;
  onResizeCockpitWidget: (instanceId: string, delta: number) => void;
  onRemoveCockpitWidget: (instanceId: string) => void;
  renderCockpitWidget: (widget: CockpitWidgetLayout) => ReactNode;
}

export function CockpitPage(props: CockpitPageProps) {
  const [editMode, setEditMode] = useState(false);
  const [addWidgetKind, setAddWidgetKind] = useState<CockpitWidgetKind | "">(
    "",
  );
  const [confirmResetOpen, setConfirmResetOpen] = useState(false);
  const [removeWidgetId, setRemoveWidgetId] = useState<string | null>(null);

  const handleAddWidget = () => {
    if (!addWidgetKind) return;
    props.onAddCockpitWidget(addWidgetKind);
    setAddWidgetKind("");
  };

  return (
    <section className="mc-cockpit-grid">
      <Surface className="mc-cockpit-sidebar" title="Layout Studio" subtitle="Widget palette + saved pages">
        <div className="mc-field-grid">
          <label>
            Active Page
            <select
              value={props.activeCockpitPage.page_id}
              onChange={(event) => props.onSetActiveCockpitPageId(event.target.value)}
            >
              {props.cockpitPages.map((page) => (
                <option key={page.page_id} value={page.page_id}>
                  {page.name}
                </option>
              ))}
            </select>
          </label>
          {editMode ? (
            <label>
              Rename Page
              <input
                value={props.activeCockpitPage.name}
                onChange={(event) => props.onRenameActiveCockpitPage(event.target.value)}
              />
            </label>
          ) : null}
        </div>

        {/* ── Widget palette dropdown ── */}
        <div className="mc-cockpit-add-widget">
          <select
            value={addWidgetKind}
            onChange={(event) => setAddWidgetKind(event.target.value as CockpitWidgetKind | "")}
          >
            <option value="">Add widget...</option>
            {COCKPIT_WIDGET_PALETTE.map((entry) => (
              <option key={entry.widget} value={entry.widget}>
                {entry.title}
              </option>
            ))}
          </select>
          <button type="button" disabled={!addWidgetKind} onClick={handleAddWidget}>
            Add
          </button>
        </div>

        <InlineActions>
          <button
            type="button"
            className={editMode ? "mc-edit-mode-active" : ""}
            onClick={() => setEditMode(!editMode)}
          >
            {editMode ? "Done Editing" : "Edit Layout"}
          </button>
          {editMode ? (
            <>
              <button type="button" onClick={props.onAddCockpitPage}>
                Add Page
              </button>
              <button type="button" onClick={props.onExportCockpitLayout}>
                Export JSON
              </button>
              <label className="upload-pill">
                <input
                  type="file"
                  accept="application/json"
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (!file) {
                      return;
                    }
                    void props.onImportCockpitLayout(file);
                    event.currentTarget.value = "";
                  }}
                />
                Import JSON
              </label>
              <button type="button" className="danger" onClick={() => setConfirmResetOpen(true)}>
                Restore Defaults
              </button>
            </>
          ) : null}
        </InlineActions>
      </Surface>
      <Surface title={props.activeCockpitPage.name} subtitle={`${props.activeCockpitPage.widgets.length} widgets`}>
        <div className="mc-cockpit-canvas">
          {props.activeCockpitPage.widgets.map((widget) => (
            <article
              key={widget.instance_id}
              className="mc-cockpit-widget"
              style={{ gridColumn: `span ${normalizeWidgetSpan(widget.span)}` }}
            >
              <header className="mc-cockpit-widget-head">
                <h3>{widget.title}</h3>
                {editMode ? (
                  <InlineActions>
                    <button type="button" onClick={() => props.onMoveCockpitWidget(widget.instance_id, -1)}>
                      Up
                    </button>
                    <button type="button" onClick={() => props.onMoveCockpitWidget(widget.instance_id, 1)}>
                      Down
                    </button>
                    <button
                      type="button"
                      onClick={() => props.onResizeCockpitWidget(widget.instance_id, -1)}
                    >
                      -
                    </button>
                    <button
                      type="button"
                      onClick={() => props.onResizeCockpitWidget(widget.instance_id, 1)}
                    >
                      +
                    </button>
                    <button
                      type="button"
                      className="danger"
                      onClick={() => setRemoveWidgetId(widget.instance_id)}
                    >
                      Remove
                    </button>
                  </InlineActions>
                ) : null}
              </header>
              {props.renderCockpitWidget(widget)}
            </article>
          ))}
          {props.activeCockpitPage.widgets.length === 0 ? (
            <EmptyState
              className="mc-empty-drawer"
              message="Add widgets from the palette to build this page."
            />
          ) : null}
        </div>
      </Surface>

      <Modal
        open={confirmResetOpen}
        onClose={() => setConfirmResetOpen(false)}
        title="Restore Defaults"
        subtitle="This will reset all cockpit pages to their default layout."
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setConfirmResetOpen(false)}>
              Cancel
            </button>
            <button
              type="button"
              className="danger"
              onClick={() => {
                setConfirmResetOpen(false);
                props.onResetCockpitLayout();
              }}
            >
              Restore Defaults
            </button>
          </>
        }
      >
        <p>Are you sure you want to restore cockpit layout defaults for all pages? This cannot be undone.</p>
      </Modal>

      <Modal
        open={removeWidgetId !== null}
        onClose={() => setRemoveWidgetId(null)}
        title="Remove Widget"
        subtitle="This will remove the widget from the current page."
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setRemoveWidgetId(null)}>
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
            {props.activeCockpitPage.widgets.find((w) => w.instance_id === removeWidgetId)?.title ?? "this widget"}
          </strong>
          ?
        </p>
      </Modal>
    </section>
  );
}
