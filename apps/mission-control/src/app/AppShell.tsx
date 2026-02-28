import clsx from "clsx";
import type { ReactNode } from "react";
import { MISSION_CONTROL_TABS } from "./tabs";
import type { MissionControlTab, Notice } from "./useAppController";
import { Chip } from "../ui/Chip";

interface AppShellProps {
  activeTab: MissionControlTab;
  onTabChange: (tab: MissionControlTab) => void;
  healthState: string;
  wsState: string;
  tokenConfigured: boolean;
  incidentMode: boolean;
  onIncidentModeChange: (value: boolean) => void;
  openBreakerCount: number;
  approvalsCount: number;
  jobsDue: number;
  schedulerRunning: boolean;
  gatewayDraft: string;
  onGatewayDraftChange: (value: string) => void;
  tokenDraft: string;
  onTokenDraftChange: (value: string) => void;
  onSaveConnection: () => Promise<void>;
  onReconnect: () => Promise<void>;
  onClearToken: () => Promise<void>;
  notice: Notice | null;
  children: ReactNode;
}

export function AppShell(props: AppShellProps) {
  const handleClearToken = () => {
    if (typeof window !== "undefined") {
      const confirmed = window.confirm(
        "Clear the stored gateway token from keychain and disconnect websocket?"
      );
      if (!confirmed) {
        return;
      }
    }
    void props.onClearToken();
  };

  return (
    <main className="mc-shell">
      <header className="mc-topbar">
        <div className="mc-brand-block">
          <p className="mc-overline">CarsinOS</p>
          <h1>Mission Control Slick</h1>
        </div>
        <div className="mc-status-strip">
          <Chip label={`health: ${props.healthState}`} tone={props.healthState} />
          <Chip label={`ws: ${props.wsState}`} tone={props.wsState} />
          <Chip label={`token: ${props.tokenConfigured ? "set" : "missing"}`} />
        </div>
      </header>

      <section className={clsx("mc-pinned-health", props.incidentMode && "incident-mode")}>
        <div className="mc-pinned-stat">
          <strong>Incident</strong>
          <span>{props.incidentMode ? "ON" : "OFF"}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Open breakers</strong>
          <span>{props.openBreakerCount}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Pending approvals</strong>
          <span>{props.approvalsCount}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Jobs due</strong>
          <span>{props.jobsDue}</span>
        </div>
        <div className="mc-pinned-stat">
          <strong>Scheduler</strong>
          <span>{props.schedulerRunning ? "running" : "paused"}</span>
        </div>
        <label className="mc-checkbox">
          <input
            type="checkbox"
            checked={props.incidentMode}
            onChange={(event) => props.onIncidentModeChange(event.target.checked)}
          />
          Incident mode filter
        </label>
      </section>

      <section className="mc-connection">
        <label>
          Gateway URL
          <input
            value={props.gatewayDraft}
            onChange={(event) => props.onGatewayDraftChange(event.target.value)}
            placeholder="http://127.0.0.1:8080"
          />
        </label>
        <label>
          Gateway Token
          <input
            value={props.tokenDraft}
            onChange={(event) => props.onTokenDraftChange(event.target.value)}
            placeholder={props.tokenConfigured ? "token stored in keychain" : "paste token"}
            type="password"
          />
        </label>
        <div className="mc-connection-actions">
          <button type="button" onClick={() => void props.onSaveConnection()}>
            Save + Connect
          </button>
          <button type="button" onClick={() => void props.onReconnect()}>
            Reconnect
          </button>
          <button type="button" className="danger" onClick={handleClearToken}>
            Clear Token
          </button>
        </div>
      </section>

      {props.notice ? (
        <div className={clsx("mc-notice", `mc-notice-${props.notice.tone}`)}>
          {props.notice.message}
        </div>
      ) : null}

      <section className="mc-tabs">
        {MISSION_CONTROL_TABS.map((item) => (
          <button
            key={item.tab}
            type="button"
            className={clsx("mc-tab", props.activeTab === item.tab && "mc-tab-active")}
            onClick={() => props.onTabChange(item.tab)}
          >
            {item.label}
          </button>
        ))}
      </section>

      {props.children}
    </main>
  );
}
