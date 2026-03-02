/* ── Custom Widget Renderer ────────────────────────────────────────────────── */

import { useState, useEffect, useRef, useCallback, type ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { useWidgetPagination } from "./useWidgetPagination";
import { runCockpitDataSource, resolveResponsePath } from "./cockpitApiRunner";
import type { CustomWidgetConfig } from "./cockpitLayout";
import type { RuntimeConnectionSettings } from "../../types";

interface CustomWidgetRendererProps {
  config: CustomWidgetConfig;
  settings: RuntimeConnectionSettings;
}

const TABLE_ROW_HEIGHT = 36;
const LIST_ITEM_HEIGHT = 40;

export function CustomWidgetRenderer({
  config,
  settings,
}: CustomWidgetRendererProps) {
  const [data, setData] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const mountedRef = useRef(true);

  const fetchData = useCallback(async () => {
    try {
      setLoading(true);
      const raw = await runCockpitDataSource(
        config.data_source,
        settings,
        config.params,
      );
      if (!mountedRef.current) return;
      const resolved = resolveResponsePath(raw, config.response_path);
      setData(resolved);
      setError(null);
    } catch (err: unknown) {
      if (!mountedRef.current) return;
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }, [config.data_source, config.params, config.response_path, settings]);

  useEffect(() => {
    mountedRef.current = true;
    void fetchData();

    let timer: ReturnType<typeof setInterval> | null = null;
    if (config.refresh_interval_ms > 0) {
      timer = setInterval(() => void fetchData(), config.refresh_interval_ms);
    }
    return () => {
      mountedRef.current = false;
      if (timer) clearInterval(timer);
    };
  }, [fetchData, config.refresh_interval_ms]);

  if (error) {
    return (
      <article className="mc-cockpit-widget-body mc-custom-widget-error">
        <AlertTriangle size={16} />
        <span>{error}</span>
        <button type="button" onClick={() => void fetchData()}>
          <RefreshCw size={12} /> Retry
        </button>
      </article>
    );
  }

  if (loading && data == null) {
    return (
      <article className="mc-cockpit-widget-body mc-custom-widget-loading">
        <span>Loading...</span>
      </article>
    );
  }

  switch (config.display_mode) {
    case "stat-card":
      return <StatCardView data={data} />;
    case "table":
      return <TableView data={data} />;
    case "list":
      return <ListView data={data} />;
    case "kv-pairs":
      return <KvPairsView data={data} />;
    default:
      return (
        <article className="mc-cockpit-widget-body">
          <pre className="mc-custom-widget-raw">
            {JSON.stringify(data, null, 2)}
          </pre>
        </article>
      );
  }
}

/* ── Display mode components ─────────────────────────────────────────────── */

function StatCardView({ data }: { data: unknown }) {
  let value: ReactNode;
  let label: string | undefined;

  if (typeof data === "number" || typeof data === "string" || typeof data === "boolean") {
    value = String(data);
  } else if (Array.isArray(data)) {
    value = String(data.length);
    label = "items";
  } else if (data != null && typeof data === "object") {
    const entries = Object.entries(data as Record<string, unknown>);
    if (entries.length === 1) {
      const [key, val] = entries[0]!;
      label = key;
      value = Array.isArray(val) ? String(val.length) : String(val);
    } else {
      value = String(entries.length);
      label = "fields";
    }
  } else {
    value = "—";
  }

  return (
    <article className="mc-cockpit-widget-body mc-custom-stat-card">
      <div className="mc-stat-value">{value}</div>
      {label ? <div className="mc-stat-label">{label}</div> : null}
    </article>
  );
}

function TableView({ data }: { data: unknown }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rows = toRowArray(data);
  const columns = inferColumns(rows);
  const pagination = useWidgetPagination(rows.length, containerRef, TABLE_ROW_HEIGHT);
  const visible = rows.slice(pagination.startIndex, pagination.endIndex);

  if (rows.length === 0) {
    return (
      <article className="mc-cockpit-widget-body">
        <span className="mc-custom-widget-empty">No data.</span>
      </article>
    );
  }

  return (
    <article className="mc-cockpit-widget-body">
      <div className="mc-custom-table-wrap" ref={containerRef}>
        <table className="mc-custom-table">
          <thead>
            <tr>
              {columns.map((col) => (
                <th key={col}>{col}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visible.map((row, i) => (
              <tr key={pagination.startIndex + i}>
                {columns.map((col) => (
                  <td key={col}>{formatCell(row[col])}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <PaginationFooter
        page={pagination.page}
        totalPages={pagination.totalPages}
        onSetPage={pagination.setPage}
      />
    </article>
  );
}

function ListView({ data }: { data: unknown }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rows = toRowArray(data);
  const pagination = useWidgetPagination(rows.length, containerRef, LIST_ITEM_HEIGHT);
  const visible = rows.slice(pagination.startIndex, pagination.endIndex);

  if (rows.length === 0) {
    return (
      <article className="mc-cockpit-widget-body">
        <span className="mc-custom-widget-empty">No data.</span>
      </article>
    );
  }

  return (
    <article className="mc-cockpit-widget-body">
      <div className="mc-widget-list-container" ref={containerRef}>
        <ul className="mc-cockpit-list">
          {visible.map((row, i) => {
            const label = pickLabel(row);
            const detail = pickDetail(row, label);
            return (
              <li key={pagination.startIndex + i}>
                <div>
                  <strong>{label}</strong>
                  {detail ? <p>{detail}</p> : null}
                </div>
              </li>
            );
          })}
        </ul>
      </div>
      <PaginationFooter
        page={pagination.page}
        totalPages={pagination.totalPages}
        onSetPage={pagination.setPage}
      />
    </article>
  );
}

function KvPairsView({ data }: { data: unknown }) {
  const containerRef = useRef<HTMLDivElement>(null);
  let entries: [string, unknown][];

  if (data != null && typeof data === "object" && !Array.isArray(data)) {
    entries = Object.entries(data as Record<string, unknown>);
  } else if (Array.isArray(data) && data.length > 0) {
    const first = data[0] as Record<string, unknown>;
    entries = first != null && typeof first === "object"
      ? Object.entries(first)
      : data.map((v, i) => [`[${i}]`, v] as [string, unknown]);
  } else {
    entries = [["value", data]];
  }

  const pagination = useWidgetPagination(entries.length, containerRef, TABLE_ROW_HEIGHT);
  const visible = entries.slice(pagination.startIndex, pagination.endIndex);

  return (
    <article className="mc-cockpit-widget-body">
      <div className="mc-custom-kv-wrap" ref={containerRef}>
        <div className="mc-custom-kv-grid">
          {visible.map(([key, val]) => (
            <div key={key} className="mc-custom-kv-row">
              <span className="mc-custom-kv-key">{key}</span>
              <span className="mc-custom-kv-val">{formatCell(val)}</span>
            </div>
          ))}
        </div>
      </div>
      <PaginationFooter
        page={pagination.page}
        totalPages={pagination.totalPages}
        onSetPage={pagination.setPage}
      />
    </article>
  );
}

/* ── Shared helpers ──────────────────────────────────────────────────────── */

function PaginationFooter({
  page,
  totalPages,
  onSetPage,
}: {
  page: number;
  totalPages: number;
  onSetPage: (p: number) => void;
}) {
  if (totalPages <= 1) return null;
  return (
    <div className="mc-widget-pagination">
      <button type="button" disabled={page <= 0} onClick={() => onSetPage(page - 1)}>
        &lsaquo;
      </button>
      <span className="mc-widget-pagination-label">
        {page + 1}/{totalPages}
      </span>
      <button type="button" disabled={page >= totalPages - 1} onClick={() => onSetPage(page + 1)}>
        &rsaquo;
      </button>
    </div>
  );
}

function toRowArray(data: unknown): Record<string, unknown>[] {
  if (Array.isArray(data)) {
    return data.filter(
      (item): item is Record<string, unknown> =>
        item != null && typeof item === "object",
    );
  }
  if (data != null && typeof data === "object") {
    const obj = data as Record<string, unknown>;
    // Look for the first array-typed value in the response
    for (const val of Object.values(obj)) {
      if (Array.isArray(val)) {
        return val.filter(
          (item): item is Record<string, unknown> =>
            item != null && typeof item === "object",
        );
      }
    }
    // Single object: wrap it
    return [obj];
  }
  return [];
}

function inferColumns(rows: Record<string, unknown>[]): string[] {
  if (rows.length === 0) return [];
  const seen = new Set<string>();
  for (const row of rows.slice(0, 5)) {
    for (const key of Object.keys(row)) {
      seen.add(key);
    }
  }
  return [...seen].slice(0, 8);
}

function formatCell(val: unknown): string {
  if (val == null) return "—";
  if (typeof val === "boolean") return val ? "true" : "false";
  if (typeof val === "number") return String(val);
  if (typeof val === "string") return val.length > 80 ? val.slice(0, 77) + "..." : val;
  if (Array.isArray(val)) return `[${val.length}]`;
  if (typeof val === "object") return "{...}";
  return String(val);
}

const LABEL_KEYS = ["name", "title", "display_name", "subject", "skill_id", "plugin_id", "agent_id", "provider", "scope"];
const DETAIL_KEYS = ["description", "detail", "status", "state", "lifecycle_state", "auth_mode", "kind"];

function pickLabel(row: Record<string, unknown>): string {
  for (const key of LABEL_KEYS) {
    if (typeof row[key] === "string" && (row[key] as string).trim()) {
      return row[key] as string;
    }
  }
  const keys = Object.keys(row);
  if (keys.length > 0) {
    const first = row[keys[0]!];
    return typeof first === "string" ? first : JSON.stringify(first);
  }
  return "—";
}

function pickDetail(row: Record<string, unknown>, usedLabel: string): string | null {
  for (const key of DETAIL_KEYS) {
    const val = row[key];
    if (typeof val === "string" && val.trim() && val !== usedLabel) {
      return val;
    }
  }
  return null;
}
