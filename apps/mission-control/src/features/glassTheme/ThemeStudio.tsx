/**
 * Theme Studio: the Glass Office theme editor. Themes are data — this
 * surface lists built-ins and customs, activates one (or follows the
 * system), and edits custom token bags with a live preview. Protected
 * tokens are visibly locked, never editable, never exported differently.
 */

import { useEffect, useRef, useState } from "react";

import {
  loadGlassConfig,
  notifyGlassConfigChanged,
  saveGlassConfig,
  type GlassConfig,
} from "../../glass/config";
import {
  activeThemeName,
  deleteCustomTheme,
  duplicateTheme,
  exportThemeJson,
  importThemeJson,
  setDraftMode,
  setDraftToken,
  tokenInputKind,
  upsertCustomTheme,
} from "../../glass/themeEditor";
import {
  applyTheme,
  BUILT_IN_THEMES,
  LOCKED_TOKENS,
  REQUIRED_TOKEN_KEYS,
  validateTheme,
  type ThemeDef,
} from "../../glass/themes";

function SwatchStrip(props: { theme: ThemeDef }) {
  const keys = ["ground", "surface", "accent", "gold", "claw"] as const;
  return (
    <span className="mc-theme-swatches" aria-hidden="true">
      {keys.map((key) => (
        <i key={key} style={{ background: props.theme.tokens[key] }} />
      ))}
    </span>
  );
}

function PreviewPane(props: { draft: ThemeDef }) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (ref.current) applyTheme(ref.current, props.draft);
  }, [props.draft]);
  return (
    <div className="mc-theme-preview" data-testid="theme-preview" ref={ref}>
      <div className="mc-theme-preview-card">
        <span className="mc-theme-preview-kicker">Preview · The Office</span>
        <strong className="mc-theme-preview-title">Morning brief</strong>
        <p className="mc-theme-preview-body">
          Two things moved overnight. Nothing needs you.
        </p>
        <span className="mc-theme-preview-chips">
          <em className="is-accent">In motion</em>
          <em className="is-claw">Needs you</em>
          <em className="is-gold">One confirmation</em>
        </span>
        <span className="mc-theme-preview-status">
          <i className="is-ok" /> healthy
          <i className="is-warn" /> attention
        </span>
      </div>
    </div>
  );
}

export function ThemeStudio() {
  const [config, setConfig] = useState<GlassConfig>(() => loadGlassConfig());
  const [draft, setDraft] = useState<ThemeDef | null>(null);
  const [draftErrors, setDraftErrors] = useState<string[]>([]);
  const [transferText, setTransferText] = useState("");
  const [transferError, setTransferError] = useState<string | null>(null);

  const persist = (next: GlassConfig) => {
    setConfig(next);
    saveGlassConfig(next);
    notifyGlassConfigChanged();
  };

  const allThemes = [...BUILT_IN_THEMES, ...config.customThemes];
  const activeName = activeThemeName(config);

  const startEdit = (theme: ThemeDef) => {
    setDraft({ ...theme, tokens: { ...theme.tokens } });
    setDraftErrors([]);
  };

  const duplicate = (theme: ThemeDef) => {
    const copy = duplicateTheme(theme, allThemes);
    persist(upsertCustomTheme(config, copy));
    setDraft(copy);
    setDraftErrors([]);
  };

  const saveDraft = () => {
    if (!draft) return;
    const result = validateTheme(draft);
    if (!result.ok) {
      setDraftErrors(result.errors);
      return;
    }
    const next = upsertCustomTheme(config, result.theme);
    if (next === config) {
      setDraftErrors([`"${draft.id}" cannot replace a built-in theme`]);
      return;
    }
    persist(next);
    setDraft(null);
    setDraftErrors([]);
  };

  const runImport = () => {
    const result = importThemeJson(transferText);
    if (!result.ok) {
      setTransferError(result.errors.join("; "));
      return;
    }
    persist(upsertCustomTheme(config, result.theme));
    setTransferError(null);
    setTransferText("");
  };

  return (
    <div className="mc-theme-studio">
      <p className="mc-theme-active" data-testid="theme-active">
        Active theme: <strong>{activeName}</strong>
      </p>

      <ul className="mc-theme-list">
        <li className="mc-theme-row">
          <span className="mc-theme-row-name">
            <strong>Follow system</strong>
            <small>Porcelain by day, Carbon after hours</small>
          </span>
          <span className="mc-theme-row-actions">
            <button
              type="button"
              aria-label="Use Follow system"
              disabled={config.themeId === "auto"}
              onClick={() => persist({ ...config, themeId: "auto" })}
            >
              Use
            </button>
          </span>
        </li>
        {allThemes.map((theme) => {
          const isCustom = config.customThemes.some((t) => t.id === theme.id);
          return (
            <li className="mc-theme-row" key={theme.id}>
              <SwatchStrip theme={theme} />
              <span className="mc-theme-row-name">
                <strong>{theme.name}</strong>
                <small>
                  {theme.mode === "dark" ? "Dark" : "Light"}
                  {isCustom ? " · custom" : " · built-in"}
                </small>
              </span>
              <span className="mc-theme-row-actions">
                <button
                  type="button"
                  aria-label={`Use ${theme.name}`}
                  disabled={config.themeId === theme.id}
                  onClick={() => persist({ ...config, themeId: theme.id })}
                >
                  Use
                </button>
                <button
                  type="button"
                  aria-label={`Duplicate ${theme.name}`}
                  onClick={() => duplicate(theme)}
                >
                  Duplicate
                </button>
                <button
                  type="button"
                  aria-label={`Export ${theme.name}`}
                  onClick={() => {
                    setTransferText(exportThemeJson(theme));
                    setTransferError(null);
                  }}
                >
                  Export
                </button>
                {isCustom ? (
                  <>
                    <button
                      type="button"
                      aria-label={`Edit ${theme.name}`}
                      onClick={() => startEdit(theme)}
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      aria-label={`Delete ${theme.name}`}
                      onClick={() => {
                        persist(deleteCustomTheme(config, theme.id));
                        if (draft?.id === theme.id) setDraft(null);
                      }}
                    >
                      Delete
                    </button>
                  </>
                ) : null}
              </span>
            </li>
          );
        })}
      </ul>

      {draft ? (
        <div className="mc-theme-editor" data-testid="theme-editor">
          <div className="mc-theme-editor-head">
            <label>
              Name
              <input
                type="text"
                value={draft.name}
                onChange={(event) =>
                  setDraft({ ...draft, name: event.target.value })
                }
              />
            </label>
            <span
              className="mc-theme-mode-toggle"
              role="group"
              aria-label="Theme mode"
            >
              <button
                type="button"
                className={draft.mode === "light" ? "is-on" : ""}
                onClick={() => setDraft(setDraftMode(draft, "light"))}
              >
                Light
              </button>
              <button
                type="button"
                className={draft.mode === "dark" ? "is-on" : ""}
                onClick={() => setDraft(setDraftMode(draft, "dark"))}
              >
                Dark
              </button>
            </span>
          </div>
          <div className="mc-theme-editor-grid">
            <div className="mc-theme-tokens">
              {REQUIRED_TOKEN_KEYS.map((key) => {
                const locked = LOCKED_TOKENS.includes(key);
                const kind = tokenInputKind(key);
                return (
                  <label
                    className={`mc-theme-token${locked ? " is-locked" : ""}`}
                    data-token={key}
                    key={key}
                  >
                    <span className="mc-theme-token-name">
                      {key}
                      {locked ? (
                        <em title="Protected — part of the CarsinOS identity and safety palette">
                          🔒 protected
                        </em>
                      ) : null}
                    </span>
                    <input
                      type={kind === "color" ? "color" : "text"}
                      value={draft.tokens[key] ?? ""}
                      disabled={locked}
                      aria-label={`Token ${key}`}
                      onChange={(event) =>
                        setDraft(setDraftToken(draft, key, event.target.value))
                      }
                    />
                    {kind === "color" ? (
                      <code>{draft.tokens[key]}</code>
                    ) : null}
                  </label>
                );
              })}
            </div>
            <PreviewPane draft={draft} />
          </div>
          {draftErrors.length > 0 ? (
            <ul className="mc-theme-errors" role="alert">
              {draftErrors.map((error) => (
                <li key={error}>{error}</li>
              ))}
            </ul>
          ) : null}
          <div className="mc-theme-editor-actions">
            <button type="button" onClick={saveDraft}>
              Save theme
            </button>
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setDraft(null);
                setDraftErrors([]);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}

      <div className="mc-theme-transfer" data-testid="theme-transfer">
        <label>
          Share themes as JSON — export fills this box, or paste one to import.
          <textarea
            rows={4}
            value={transferText}
            spellCheck={false}
            onChange={(event) => setTransferText(event.target.value)}
            placeholder='{"id": "my-theme", "name": "My Theme", "mode": "light", "tokens": { ... }}'
          />
        </label>
        <div className="mc-theme-editor-actions">
          <button
            type="button"
            disabled={!transferText.trim()}
            onClick={runImport}
          >
            Import theme
          </button>
        </div>
        {transferError ? (
          <p className="mc-theme-errors" role="alert">
            {transferError}
          </p>
        ) : null}
      </div>
    </div>
  );
}
