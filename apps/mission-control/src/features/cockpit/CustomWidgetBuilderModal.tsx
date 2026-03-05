/* ── Custom Widget Builder — 5-step wizard modal ─────────────────────────── */

import { useState, useEffect, useCallback, useRef } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Database,
  BarChart3,
  List,
  Table2,
  Rows3,
  Plus,
} from "lucide-react";
import { Modal } from "../../ui/Modal";
import {
  COCKPIT_DATA_SOURCES,
  getDataSourcesByCategory,
  type CockpitDataSource,
} from "./cockpitDataSources";
import { runCockpitDataSource, resolveResponsePath } from "./cockpitApiRunner";
import { CustomWidgetRenderer } from "./CustomWidgetRenderer";
import type { CustomWidgetConfig, CockpitWidgetLayoutV2 } from "./cockpitLayout";
import type { RuntimeConnectionSettings } from "../../types";
import { redactSecrets } from "../../lib/redaction";

interface CustomWidgetBuilderModalProps {
  open: boolean;
  onClose: () => void;
  onAddWidget: (widget: CockpitWidgetLayoutV2) => void;
  settings: RuntimeConnectionSettings;
}

type DisplayMode = CustomWidgetConfig["display_mode"];

const REFRESH_OPTIONS = [
  { label: "Manual", value: 0 },
  { label: "10s", value: 10_000 },
  { label: "30s", value: 30_000 },
  { label: "1 min", value: 60_000 },
  { label: "5 min", value: 300_000 },
];

const DISPLAY_MODES: { mode: DisplayMode; label: string; icon: typeof BarChart3 }[] = [
  { mode: "stat-card", label: "Stat Card", icon: BarChart3 },
  { mode: "table", label: "Table", icon: Table2 },
  { mode: "list", label: "List", icon: List },
  { mode: "kv-pairs", label: "Key-Value", icon: Rows3 },
];

export function CustomWidgetBuilderModal({
  open,
  onClose,
  onAddWidget,
  settings,
}: CustomWidgetBuilderModalProps) {
  const [step, setStep] = useState(0);
  const [selectedSource, setSelectedSource] = useState<CockpitDataSource | null>(null);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [paramOptions, setParamOptions] = useState<Record<string, { label: string; value: string }[]>>({});
  const [paramResolveErrors, setParamResolveErrors] = useState<Record<string, string | null>>({});
  const [paramLoading, setParamLoading] = useState<Record<string, boolean>>({});
  const [paramResolveNonce, setParamResolveNonce] = useState(0);
  const [displayMode, setDisplayMode] = useState<DisplayMode>("table");
  const [title, setTitle] = useState("");
  const [refreshInterval, setRefreshInterval] = useState(0);
  const [responsePath, setResponsePath] = useState("");
  const [previewData, setPreviewData] = useState<unknown>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const resolverRequestIdRef = useRef(0);

  // Reset state when modal opens
  useEffect(() => {
    if (open) {
      setStep(0);
      setSelectedSource(null);
      setParamValues({});
      setParamOptions({});
      setParamResolveErrors({});
      setParamLoading({});
      setParamResolveNonce(0);
      setDisplayMode("table");
      setTitle("");
      setRefreshInterval(0);
      setResponsePath("");
      setPreviewData(null);
      setPreviewError(null);
    }
  }, [open]);

  // Resolve param options when a source with params is selected
  useEffect(() => {
    if (!selectedSource?.params || selectedSource.params.length === 0) {
      setParamLoading({});
      setParamResolveErrors({});
      return;
    }

    const requestId = resolverRequestIdRef.current + 1;
    resolverRequestIdRef.current = requestId;
    let cancelled = false;

    const allowedKeys = new Set(selectedSource.params.map((param) => param.key));
    setParamValues((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([key]) => allowedKeys.has(key)),
      ),
    );
    setParamOptions({});
    setParamResolveErrors({});
    setParamLoading({});

    for (const param of selectedSource.params) {
      const paramKey = param.key;
      if (param.resolver.startsWith("_static:")) {
        const values = param.resolver.slice("_static:".length).split(",");
        const options = values.map((v) => ({ label: v.trim(), value: v.trim() }));
        setParamOptions((prev) => ({
          ...prev,
          [paramKey]: options,
        }));
        setParamResolveErrors((prev) => ({ ...prev, [paramKey]: null }));
        setParamLoading((prev) => ({ ...prev, [paramKey]: false }));
        setParamValues((prev) => {
          if (!prev[paramKey] && options.length > 0) {
            return { ...prev, [paramKey]: options[0]!.value };
          }
          return prev;
        });
        continue;
      }

      const resolverSource = COCKPIT_DATA_SOURCES.find((ds) => ds.id === param.resolver);
      if (!resolverSource) {
        setParamResolveErrors((prev) => ({
          ...prev,
          [paramKey]: `Resolver not found: ${param.resolver}`,
        }));
        setParamLoading((prev) => ({ ...prev, [paramKey]: false }));
        continue;
      }

      const labelField = param.resolverLabelField;
      const valueField = param.resolverValueField;
      setParamLoading((prev) => ({ ...prev, [paramKey]: true }));
      setParamResolveErrors((prev) => ({ ...prev, [paramKey]: null }));

      void (async () => {
        try {
          const raw = await runCockpitDataSource(resolverSource.id, settings);
          if (cancelled || requestId !== resolverRequestIdRef.current) {
            return;
          }
          const items = extractArray(raw);
          const options = items
            .map((item) => ({
              label: String((item as Record<string, unknown>)[labelField] ?? "—"),
              value: String((item as Record<string, unknown>)[valueField] ?? ""),
            }))
            .filter((option) => option.value.trim().length > 0);
          setParamOptions((prev) => ({ ...prev, [paramKey]: options }));
          setParamResolveErrors((prev) => ({ ...prev, [paramKey]: null }));
          setParamValues((prev) => {
            if (!prev[paramKey] && options.length > 0) {
              return { ...prev, [paramKey]: options[0]!.value };
            }
            return prev;
          });
        } catch (error: unknown) {
          if (cancelled || requestId !== resolverRequestIdRef.current) {
            return;
          }
          setParamOptions((prev) => ({ ...prev, [paramKey]: [] }));
          setParamResolveErrors((prev) => ({
            ...prev,
            [paramKey]: error instanceof Error ? error.message : String(error),
          }));
        } finally {
          if (!cancelled && requestId === resolverRequestIdRef.current) {
            setParamLoading((prev) => ({ ...prev, [paramKey]: false }));
          }
        }
      })();
    }

    return () => {
      cancelled = true;
    };
  }, [selectedSource, settings, paramResolveNonce]);

  const hasParams = selectedSource?.params && selectedSource.params.length > 0;

  // Step labels
  const steps = ["Data Source", ...(hasParams ? ["Parameters"] : []), "Display", "Configure", "Preview"];
  const totalSteps = steps.length;

  const canAdvance = (): boolean => {
    const stepLabel = steps[step];
    if (stepLabel === "Data Source") return selectedSource !== null;
    if (stepLabel === "Parameters") {
      return selectedSource?.params?.every((p) => paramValues[p.key]?.trim()) ?? true;
    }
    if (stepLabel === "Display") return true;
    if (stepLabel === "Configure") return title.trim().length > 0;
    return true;
  };

  const handleNext = useCallback(() => {
    if (step < totalSteps - 1 && canAdvance()) {
      const nextStep = step + 1;
      setStep(nextStep);

      // Auto-set title from source label if empty
      if (steps[nextStep] === "Configure" && !title.trim() && selectedSource) {
        setTitle(selectedSource.label);
      }

      // Auto-fetch preview data when entering preview step
      if (steps[nextStep] === "Preview" && selectedSource) {
        void fetchPreview();
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, totalSteps, selectedSource, title, paramValues, responsePath]);

  const handleBack = useCallback(() => {
    if (step > 0) setStep(step - 1);
  }, [step]);

  const fetchPreview = async () => {
    if (!selectedSource) return;
    setPreviewLoading(true);
    setPreviewError(null);
    try {
      const raw = await runCockpitDataSource(selectedSource.id, settings, paramValues);
      const resolved = resolveResponsePath(raw, responsePath || undefined);
      setPreviewData(resolved);
    } catch (err: unknown) {
      setPreviewError(err instanceof Error ? err.message : String(err));
    } finally {
      setPreviewLoading(false);
    }
  };

  const handleAddWidget = useCallback(() => {
    if (!selectedSource) return;

    const config: CustomWidgetConfig = {
      data_source: selectedSource.id,
      display_mode: displayMode,
      title: title.trim() || selectedSource.label,
      refresh_interval_ms: refreshInterval,
      response_path: responsePath.trim() || undefined,
      params: hasParams ? paramValues : undefined,
    };

    const widget: CockpitWidgetLayoutV2 = {
      instance_id: `custom-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      widget: "custom",
      title: config.title,
      position: { x: 0, y: Infinity, w: 4, h: 3 },
      custom_config: config,
    };

    onAddWidget(widget);
    onClose();
  }, [selectedSource, displayMode, title, refreshInterval, responsePath, paramValues, hasParams, onAddWidget, onClose]);

  const currentStepLabel = steps[step] ?? "Data Source";

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Custom Widget Builder"
      subtitle={`Step ${step + 1} of ${totalSteps}: ${currentStepLabel}`}
      footer={
        <div className="mc-builder-footer">
          <button
            type="button"
            className="ghost"
            onClick={handleBack}
            disabled={step === 0}
          >
            <ChevronLeft size={14} /> Back
          </button>
          <div className="mc-builder-step-dots">
            {steps.map((_, i) => (
              <span
                key={i}
                className={`mc-builder-dot ${i === step ? "mc-builder-dot-active" : ""} ${i < step ? "mc-builder-dot-done" : ""}`}
              />
            ))}
          </div>
          {currentStepLabel === "Preview" ? (
            <button type="button" onClick={handleAddWidget}>
              <Plus size={14} /> Add to Dashboard
            </button>
          ) : (
            <button
              type="button"
              onClick={handleNext}
              disabled={!canAdvance()}
            >
              Next <ChevronRight size={14} />
            </button>
          )}
        </div>
      }
    >
      <div className="mc-builder-body">
        {currentStepLabel === "Data Source" && (
          <DataSourceStep
            selected={selectedSource}
            onSelect={setSelectedSource}
          />
        )}

        {currentStepLabel === "Parameters" && selectedSource?.params && (
          <ParametersStep
            params={selectedSource.params}
            values={paramValues}
            options={paramOptions}
            loading={paramLoading}
            errors={paramResolveErrors}
            onRetry={() => setParamResolveNonce((current) => current + 1)}
            onChange={(key, val) =>
              setParamValues((prev) => ({ ...prev, [key]: val }))
            }
          />
        )}

        {currentStepLabel === "Display" && (
          <DisplayModeStep selected={displayMode} onSelect={setDisplayMode} />
        )}

        {currentStepLabel === "Configure" && (
          <ConfigureStep
            title={title}
            onTitleChange={setTitle}
            refreshInterval={refreshInterval}
            onRefreshIntervalChange={setRefreshInterval}
            responsePath={responsePath}
            onResponsePathChange={setResponsePath}
            sampleFields={selectedSource?.sampleFields ?? []}
          />
        )}

        {currentStepLabel === "Preview" && selectedSource && (
          <PreviewStep
            config={{
              data_source: selectedSource.id,
              display_mode: displayMode,
              title: title.trim() || selectedSource.label,
              refresh_interval_ms: 0,
              response_path: responsePath.trim() || undefined,
              params: hasParams ? paramValues : undefined,
            }}
            settings={settings}
            data={previewData}
            error={previewError}
            loading={previewLoading}
            onRetry={() => void fetchPreview()}
          />
        )}
      </div>
    </Modal>
  );
}

/* ── Step Components ─────────────────────────────────────────────────────── */

function DataSourceStep({
  selected,
  onSelect,
}: {
  selected: CockpitDataSource | null;
  onSelect: (ds: CockpitDataSource) => void;
}) {
  const categories = getDataSourcesByCategory();

  return (
    <div className="mc-builder-sources">
      {[...categories.entries()].map(([category, sources]) => (
        <div key={category} className="mc-builder-source-group">
          <h4 className="mc-builder-source-category">{category}</h4>
          <div className="mc-builder-source-list">
            {sources.map((ds) => (
              <button
                key={ds.id}
                type="button"
                className={`mc-builder-source-card ${selected?.id === ds.id ? "mc-builder-source-card-active" : ""}`}
                onClick={() => onSelect(ds)}
              >
                <Database size={14} />
                <div>
                  <strong>{ds.label}</strong>
                  <p>{ds.description}</p>
                </div>
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function ParametersStep({
  params,
  values,
  options,
  loading,
  errors,
  onRetry,
  onChange,
}: {
  params: { key: string; label: string }[];
  values: Record<string, string>;
  options: Record<string, { label: string; value: string }[]>;
  loading: Record<string, boolean>;
  errors: Record<string, string | null>;
  onRetry: () => void;
  onChange: (key: string, value: string) => void;
}) {
  return (
    <div className="mc-builder-params">
      {params.map((param) => {
        const opts = options[param.key] ?? [];
        const isLoading = loading[param.key] ?? false;
        const errorText = errors[param.key];
        return (
          <label key={param.key} className="mc-builder-param-field">
            {param.label}
            <select
              value={values[param.key] ?? ""}
              onChange={(e) => onChange(param.key, e.target.value)}
              disabled={isLoading || Boolean(errorText)}
            >
              {errorText ? (
                <option value="">Resolver failed</option>
              ) : isLoading ? (
                <option value="">Loading...</option>
              ) : opts.length === 0 ? (
                <option value="">No options available</option>
              ) : (
                <>
                  <option value="">Select {param.label}...</option>
                  {opts.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </>
              )}
            </select>
            {errorText ? (
              <small className="mc-form-error">
                Failed to load {param.label}.{" "}
                <button type="button" className="mc-inline-link-btn" onClick={onRetry}>
                  Retry
                </button>
              </small>
            ) : null}
          </label>
        );
      })}
    </div>
  );
}

function DisplayModeStep({
  selected,
  onSelect,
}: {
  selected: DisplayMode;
  onSelect: (mode: DisplayMode) => void;
}) {
  return (
    <div className="mc-builder-display-modes">
      {DISPLAY_MODES.map(({ mode, label, icon: Icon }) => (
        <button
          key={mode}
          type="button"
          className={`mc-builder-display-card ${selected === mode ? "mc-builder-display-card-active" : ""}`}
          onClick={() => onSelect(mode)}
        >
          <Icon size={24} />
          <span>{label}</span>
        </button>
      ))}
    </div>
  );
}

function ConfigureStep({
  title,
  onTitleChange,
  refreshInterval,
  onRefreshIntervalChange,
  responsePath,
  onResponsePathChange,
  sampleFields,
}: {
  title: string;
  onTitleChange: (val: string) => void;
  refreshInterval: number;
  onRefreshIntervalChange: (val: number) => void;
  responsePath: string;
  onResponsePathChange: (val: string) => void;
  sampleFields: string[];
}) {
  return (
    <div className="mc-builder-configure">
      <label className="mc-builder-field">
        Widget Title
        <input
          type="text"
          value={title}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="e.g. Active Agents"
        />
      </label>

      <label className="mc-builder-field">
        Refresh Interval
        <select
          value={refreshInterval}
          onChange={(e) => onRefreshIntervalChange(Number(e.target.value))}
        >
          {REFRESH_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </label>

      <label className="mc-builder-field">
        Response Path
        <input
          type="text"
          value={responsePath}
          onChange={(e) => onResponsePathChange(e.target.value)}
          placeholder="e.g. agents"
        />
        {sampleFields.length > 0 ? (
          <small className="mc-builder-hint">
            Available: {sampleFields.join(", ")}
          </small>
        ) : null}
      </label>
    </div>
  );
}

function PreviewStep({
  config,
  settings,
  data,
  error,
  loading,
  onRetry,
}: {
  config: CustomWidgetConfig;
  settings: RuntimeConnectionSettings;
  data: unknown;
  error: string | null;
  loading: boolean;
  onRetry: () => void;
}) {
  if (loading) {
    return <div className="mc-builder-preview-loading">Fetching data...</div>;
  }

  if (error) {
    return (
      <div className="mc-builder-preview-error">
        <p>{error}</p>
        <button type="button" onClick={onRetry}>
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="mc-builder-preview">
      <div className="mc-builder-preview-widget">
        <header className="mc-cockpit-widget-head">
          <h3>{config.title}</h3>
        </header>
        <CustomWidgetRenderer config={config} settings={settings} />
      </div>
      {data != null ? (
        <details className="mc-builder-preview-raw">
          <summary>Raw response</summary>
          <pre>{JSON.stringify(redactSecrets(data), null, 2)}</pre>
        </details>
      ) : null}
    </div>
  );
}

/* ── Helpers ──────────────────────────────────────────────────────────────── */

function extractArray(data: unknown): unknown[] {
  if (Array.isArray(data)) return data;
  if (data != null && typeof data === "object") {
    for (const val of Object.values(data as Record<string, unknown>)) {
      if (Array.isArray(val)) return val;
    }
  }
  return [];
}
