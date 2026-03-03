import { useState, useRef, useEffect, useCallback, useId } from "react";
import { Palette, Sun, Moon } from "lucide-react";
import clsx from "clsx";
import { THEME_FAMILIES } from "../app/useTheme";
import type { ThemeFamily, ThemeMode } from "../app/useTheme";

interface ThemeDropdownProps {
  family: ThemeFamily;
  mode: ThemeMode;
  selectFamily: (f: ThemeFamily) => void;
  setMode: (m: ThemeMode) => void;
  toggleMode: () => void;
}

export function ThemeDropdown({ family, mode, selectFamily, setMode, toggleMode }: ThemeDropdownProps) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const panelId = useId();

  const toggle = useCallback(() => setOpen((o) => !o), []);

  // Close on click-outside
  useEffect(() => {
    if (!open) return;
    function handleDown(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleDown);
    return () => document.removeEventListener("mousedown", handleDown);
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [open]);

  const handleRowClick = (f: ThemeFamily) => {
    if (f === family) {
      toggleMode();
    } else {
      selectFamily(f);
    }
  };

  const handleModeClick = (f: ThemeFamily, m: ThemeMode, e: React.MouseEvent) => {
    e.stopPropagation();
    selectFamily(f);
    setMode(m);
  };

  return (
    <div className="mc-theme-dropdown-wrap" ref={wrapRef}>
      <button
        type="button"
        className="mc-topbar-icon-btn"
        onClick={toggle}
        title="Theme"
        aria-label="Theme"
        aria-expanded={open}
        aria-controls={panelId}
      >
        <Palette size={16} />
      </button>

      {open && (
        <div id={panelId} className="mc-theme-dropdown" role="menu">
          <div className="mc-theme-dropdown-header">Theme</div>
          {THEME_FAMILIES.map((t) => {
            const isActive = t.family === family;
            return (
              <div
                key={t.family}
                className={clsx("mc-theme-dropdown-row", isActive && "mc-theme-dropdown-row-active")}
                role="menuitem"
                onClick={() => handleRowClick(t.family)}
              >
                <span className="mc-theme-dropdown-swatch" style={{ background: t.accent }} />
                <span className="mc-theme-dropdown-name">{t.label}</span>
                <span className="mc-theme-dropdown-modes">
                  <button
                    type="button"
                    className={clsx("mc-theme-mode-btn", isActive && mode === "light" && "mc-theme-mode-btn-active")}
                    onClick={(e) => handleModeClick(t.family, "light", e)}
                    title={`${t.label} — Light`}
                    aria-label={`${t.label} light mode`}
                  >
                    <Sun size={14} />
                  </button>
                  <button
                    type="button"
                    className={clsx("mc-theme-mode-btn", isActive && mode === "dark" && "mc-theme-mode-btn-active")}
                    onClick={(e) => handleModeClick(t.family, "dark", e)}
                    title={`${t.label} — Dark`}
                    aria-label={`${t.label} dark mode`}
                  >
                    <Moon size={14} />
                  </button>
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
