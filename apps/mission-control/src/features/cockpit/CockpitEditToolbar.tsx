import { useState, useRef, useEffect, useCallback, useId } from "react";
import {
  Plus,
  Pencil,
  MoreHorizontal,
  Download,
  Upload,
  Trash2,
  LayoutTemplate,
  RotateCcw,
} from "lucide-react";

interface CockpitEditToolbarProps {
  onOpenWidgetPicker: () => void;
  onStartRename: () => void;
  onExportLayout: () => void;
  onImportLayout: (file: File) => Promise<void>;
  onDeletePage: () => void;
  onLoadTemplate: () => void;
  onResetLayout: () => void;
  canDeletePage: boolean;
}

export function CockpitEditToolbar(props: CockpitEditToolbarProps) {
  const [moreOpen, setMoreOpen] = useState(false);
  const moreRef = useRef<HTMLDivElement>(null);
  const menuId = useId();

  const closeMore = useCallback(() => setMoreOpen(false), []);

  useEffect(() => {
    if (!moreOpen) return;
    const handler = (e: MouseEvent) => {
      if (moreRef.current && !moreRef.current.contains(e.target as Node)) {
        closeMore();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [moreOpen, closeMore]);

  return (
    <div className="mc-cockpit-edit-toolbar">
      <button
        type="button"
        className="mc-edit-toolbar-btn mc-edit-toolbar-primary"
        onClick={props.onOpenWidgetPicker}
      >
        <Plus size={14} />
        Add Widget
      </button>

      <button
        type="button"
        className="mc-edit-toolbar-btn"
        onClick={props.onStartRename}
      >
        <Pencil size={14} />
        Rename
      </button>

      <div className="mc-edit-toolbar-more-wrap" ref={moreRef}>
        <button
          type="button"
          className="mc-edit-toolbar-btn"
          onClick={() => setMoreOpen(!moreOpen)}
          aria-haspopup="menu"
          aria-expanded={moreOpen}
          aria-controls={menuId}
        >
          <MoreHorizontal size={14} />
        </button>

        {moreOpen ? (
          <div
            id={menuId}
            className="mc-edit-toolbar-dropdown"
            role="menu"
            aria-hidden={!moreOpen}
          >
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                props.onExportLayout();
                closeMore();
              }}
            >
              <Download size={14} />
              Export JSON
            </button>

            <label className="mc-edit-toolbar-dropdown-item">
              <Upload size={14} />
              Import JSON
              <input
                type="file"
                accept="application/json"
                className="mc-sr-only"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) {
                    void props.onImportLayout(file);
                  }
                  e.currentTarget.value = "";
                  closeMore();
                }}
              />
            </label>

            <button
              type="button"
              role="menuitem"
              onClick={() => {
                props.onLoadTemplate();
                closeMore();
              }}
            >
              <LayoutTemplate size={14} />
              Load Template
            </button>

            <button
              type="button"
              role="menuitem"
              onClick={() => {
                props.onResetLayout();
                closeMore();
              }}
            >
              <RotateCcw size={14} />
              Restore Defaults
            </button>

            {props.canDeletePage ? (
              <button
                type="button"
                className="danger"
                role="menuitem"
                onClick={() => {
                  props.onDeletePage();
                  closeMore();
                }}
              >
                <Trash2 size={14} />
                Delete Page
              </button>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
