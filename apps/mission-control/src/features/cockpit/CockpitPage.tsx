import type { ReactNode } from "react";
import {
  COCKPIT_WIDGET_PALETTE,
  normalizeWidgetSpan,
  type CockpitPageLayout,
  type CockpitWidgetKind,
  type CockpitWidgetLayout,
} from "./cockpitLayout";
import { EmptyState } from "../../ui/EmptyState";
import { InlineActions } from "../../ui/InlineActions";
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
  const handleResetCockpitLayout = () => {
    if (typeof window !== "undefined") {
      const confirmed = window.confirm(
        "Restore cockpit layout defaults for all pages?"
      );
      if (!confirmed) {
        return;
      }
    }
    props.onResetCockpitLayout();
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
          <label>
            Rename Page
            <input
              value={props.activeCockpitPage.name}
              onChange={(event) => props.onRenameActiveCockpitPage(event.target.value)}
            />
          </label>
        </div>
        <InlineActions>
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
          <button type="button" className="danger" onClick={handleResetCockpitLayout}>
            Restore Defaults
          </button>
        </InlineActions>
        <div className="mc-cockpit-palette">
          {COCKPIT_WIDGET_PALETTE.map((entry) => (
            <article key={entry.widget} className="mc-palette-item">
              <div>
                <h3>{entry.title}</h3>
                <p>{entry.description}</p>
              </div>
              <button type="button" onClick={() => props.onAddCockpitWidget(entry.widget)}>
                Add
              </button>
            </article>
          ))}
        </div>
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
                    onClick={() => props.onRemoveCockpitWidget(widget.instance_id)}
                  >
                    Remove
                  </button>
                </InlineActions>
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
    </section>
  );
}
