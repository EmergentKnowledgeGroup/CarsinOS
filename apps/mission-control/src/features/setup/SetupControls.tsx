import { useState, type Ref } from "react";
import { Chip } from "../../ui/Chip";
import { Modal } from "../../ui/Modal";
import { DEFAULT_GATEWAY_URL } from "../../constants";
import { STORAGE_KEYS } from "../../storageKeys";
import type {
  OpsUxFeatureControls,
  OpsUxRuntimeConfig,
} from "../../lib/opsUxConfig";
import {
  connectionStatusPresentation,
  rolloutStatePresentation,
} from "./setupPresentation";

/*
 * One shared Setup control surface.
 *
 * The AppShell Settings modal and the Basement Setup room both render these
 * components over the exact same App-owned drafts, facts, and callbacks:
 * the one runtime connection controller, the one opsUxConfig feature-control
 * authority, and the one onboarding/tour launcher. Neither consumer may add
 * a second draft, a read-modify-write copy, or its own localStorage truth.
 */

/* ── Gateway URL history (presentation nicety shared by both surfaces) ── */

const GW_HISTORY_KEY = STORAGE_KEYS.gatewayUrlHistory;
const GW_HISTORY_MAX = 8;

function getGatewayUrlHistory(): string[] {
  try {
    const raw = localStorage.getItem(GW_HISTORY_KEY);
    return raw ? (JSON.parse(raw) as string[]) : [];
  } catch {
    return [];
  }
}

function pushGatewayUrlHistory(url: string) {
  const trimmed = url.trim();
  if (!trimmed) return;
  const history = getGatewayUrlHistory().filter((u) => u !== trimmed);
  history.unshift(trimmed);
  try {
    localStorage.setItem(
      GW_HISTORY_KEY,
      JSON.stringify(history.slice(0, GW_HISTORY_MAX)),
    );
  } catch {
    // URL history is a presentation nicety. Storage denial or quota pressure
    // must never prevent the authoritative connection save.
  }
}

/* ── Shared authority prop bundles ────────────────────────────────── */

export interface ConnectionAuthorityProps {
  gatewayDraft: string;
  onGatewayDraftChange: (value: string) => void;
  tokenDraft: string;
  onTokenDraftChange: (value: string) => void;
  /** Configured-token truth is a boolean; the token value is never echoed. */
  tokenConfigured: boolean;
  healthState: string;
  wsState: string;
  onSaveConnection: () => Promise<void>;
  onReconnect: () => Promise<void>;
  onClearToken: () => Promise<void>;
  onOpenSetupWizard: () => void;
  onOpenGuidedTour: () => void;
}

export interface FeatureControlAuthorityProps {
  opsUxConfig: OpsUxRuntimeConfig;
  opsUxConfigError: string | null;
  onPatchOpsUxControls: (patch: Partial<OpsUxFeatureControls>) => void;
  usageChartsEnabled: boolean;
}

/** The whole shared Setup authority App hands to both consuming surfaces. */
export type SetupSurfaceProps = ConnectionAuthorityProps &
  FeatureControlAuthorityProps;

/* ── Connection controls ──────────────────────────────────────────── */

export interface ConnectionControlsProps extends ConnectionAuthorityProps {
  /** Unique per mounted surface so datalist ids never collide. */
  idPrefix: string;
  /** Fired synchronously after a save is initiated (Settings closes itself). */
  onSaveInitiated?: () => void;
}

export function ConnectionControls(props: ConnectionControlsProps) {
  const [gwUrlHistory, setGwUrlHistory] = useState<string[]>(
    getGatewayUrlHistory,
  );
  const [clearTokenConfirmOpen, setClearTokenConfirmOpen] = useState(false);

  const handleSaveAndConnect = () => {
    pushGatewayUrlHistory(props.gatewayDraft);
    setGwUrlHistory(getGatewayUrlHistory());
    void props.onSaveConnection();
    props.onSaveInitiated?.();
  };

  const confirmClearToken = () => {
    setClearTokenConfirmOpen(false);
    void props.onClearToken();
  };

  const historyListId = `mc-gw-url-history-${props.idPrefix}`;
  const { liveLinkLabel } = connectionStatusPresentation(props);

  return (
    <>
      <p className="mc-settings-help">
        Tell Mission Control where carsinOS is running. Most people only touch
        this once.
      </p>
      <label className="mc-modal-field">
        Gateway URL
        <input
          list={historyListId}
          value={props.gatewayDraft}
          onChange={(e) => props.onGatewayDraftChange(e.target.value)}
          placeholder={DEFAULT_GATEWAY_URL}
        />
        <datalist id={historyListId}>
          {gwUrlHistory.map((url) => (
            <option key={url} value={url} />
          ))}
        </datalist>
      </label>
      <label className="mc-modal-field">
        Gateway Token
        <input
          value={props.tokenDraft}
          onChange={(e) => props.onTokenDraftChange(e.target.value)}
          placeholder={props.tokenConfigured ? "token configured" : "paste token"}
          type="password"
        />
      </label>
      <div className="mc-modal-status-row">
        <Chip label={liveLinkLabel} tone={props.wsState} />
      </div>
      <p className="mc-settings-help">
        Desktop stores the gateway token in the OS keychain. Browser runs keep
        it in memory, with session-only storage reserved for the explicit E2E
        harness.
      </p>
      <div className="mc-modal-actions">
        <button type="button" onClick={handleSaveAndConnect}>
          Save and connect
        </button>
        <button
          type="button"
          className="ghost"
          onClick={() => void props.onReconnect()}
        >
          Try reconnect
        </button>
        <button type="button" className="ghost" onClick={props.onOpenSetupWizard}>
          Open setup wizard
        </button>
        <button type="button" className="ghost" onClick={props.onOpenGuidedTour}>
          Start guided tour
        </button>
        <button
          type="button"
          className="danger"
          onClick={() => setClearTokenConfirmOpen(true)}
        >
          Forget token
        </button>
      </div>
      <Modal
        open={clearTokenConfirmOpen}
        onClose={() => setClearTokenConfirmOpen(false)}
        title="Clear Token?"
        subtitle="This will disconnect from the gateway."
        footer={
          <>
            <button
              type="button"
              className="ghost"
              onClick={() => setClearTokenConfirmOpen(false)}
            >
              Cancel
            </button>
            <button type="button" className="danger" onClick={confirmClearToken}>
              Clear Token
            </button>
          </>
        }
      >
        <p>
          This will remove the configured gateway token from secure runtime
          storage and disconnect the WebSocket connection. You will need to
          reconfigure the token to reconnect.
        </p>
      </Modal>
    </>
  );
}

/* ── Feature controls ─────────────────────────────────────────────── */

export interface FeatureControlsProps extends FeatureControlAuthorityProps {
  /** Settings focuses its Live Feed toggle when opened from the topbar. */
  liveFeedInputRef?: Ref<HTMLInputElement>;
  liveFeedAutoFocus?: boolean;
}

export function FeatureControls({
  opsUxConfig,
  opsUxConfigError,
  onPatchOpsUxControls,
  usageChartsEnabled,
  liveFeedInputRef,
  liveFeedAutoFocus,
}: FeatureControlsProps) {
  const masterOn = !opsUxConfig.controls.global_kill_switch;

  const patchOpsControl = (
    key: keyof OpsUxFeatureControls,
    value: boolean,
  ) => {
    // Turning a feature on while the main switch is off flips both in one
    // patch through the single opsUxConfig authority.
    if (key !== "global_kill_switch" && value && !masterOn) {
      onPatchOpsUxControls({
        global_kill_switch: false,
        [key]: value,
      });
      return;
    }
    onPatchOpsUxControls({ [key]: value });
  };

  const mainSwitchStatus = masterOn
    ? { label: "On", tone: "connected" }
    : { label: "Off", tone: "" };
  const liveFeedStatus = rolloutStatePresentation(
    masterOn,
    opsUxConfig.controls.live_feed_drawer,
  );
  const incidentAutoStatus = rolloutStatePresentation(
    masterOn,
    opsUxConfig.controls.incident_auto_trigger,
  );
  const strategyStatus = rolloutStatePresentation(
    masterOn,
    opsUxConfig.controls.strategy_hub,
  );
  const runbookStatus = rolloutStatePresentation(
    masterOn,
    opsUxConfig.controls.runbook_hub,
  );
  const memoryStatus = rolloutStatePresentation(
    masterOn,
    opsUxConfig.controls.memory_hub,
  );
  const connectorsStatus = rolloutStatePresentation(
    masterOn,
    opsUxConfig.controls.connectors_hub,
  );
  const usageChartsStatus = usageChartsEnabled
    ? { label: "On", tone: "connected" }
    : masterOn && opsUxConfig.controls.usage_charts
      ? { label: "Waiting on data", tone: "warning" }
      : { label: "Off", tone: "" };

  return (
    <>
      <p className="mc-settings-help">
        These switches control optional tools. The everyday path is Boards,
        Calendar, Focus, Mail, Rooms, Assistant, and Team.
      </p>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={masterOn}
            onChange={(event) =>
              patchOpsControl("global_kill_switch", !event.target.checked)
            }
          />
          <span>Allow optional pages and tools</span>
        </label>
        <Chip label={mainSwitchStatus.label} tone={mainSwitchStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            ref={liveFeedInputRef}
            type="checkbox"
            autoFocus={liveFeedAutoFocus}
            checked={opsUxConfig.controls.live_feed_drawer}
            onChange={(event) =>
              patchOpsControl("live_feed_drawer", event.target.checked)
            }
          />
          <span>Live Feed panel</span>
        </label>
        <Chip label={liveFeedStatus.label} tone={liveFeedStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={opsUxConfig.controls.incident_auto_trigger}
            onChange={(event) =>
              patchOpsControl("incident_auto_trigger", event.target.checked)
            }
          />
          <span>Auto-switch to incident mode</span>
        </label>
        <Chip label={incidentAutoStatus.label} tone={incidentAutoStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={opsUxConfig.controls.usage_charts}
            onChange={(event) =>
              patchOpsControl("usage_charts", event.target.checked)
            }
          />
          <span>Usage charts</span>
        </label>
        <Chip label={usageChartsStatus.label} tone={usageChartsStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={opsUxConfig.controls.strategy_hub}
            onChange={(event) =>
              patchOpsControl("strategy_hub", event.target.checked)
            }
          />
          <span>Strategy page</span>
        </label>
        <Chip label={strategyStatus.label} tone={strategyStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={opsUxConfig.controls.runbook_hub}
            onChange={(event) =>
              patchOpsControl("runbook_hub", event.target.checked)
            }
          />
          <span>Runbook page</span>
        </label>
        <Chip label={runbookStatus.label} tone={runbookStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={opsUxConfig.controls.memory_hub}
            onChange={(event) =>
              patchOpsControl("memory_hub", event.target.checked)
            }
          />
          <span>Memory page</span>
        </label>
        <Chip label={memoryStatus.label} tone={memoryStatus.tone} />
      </div>
      <div className="mc-settings-toggle-row">
        <label className="mc-settings-toggle">
          <input
            type="checkbox"
            checked={opsUxConfig.controls.connectors_hub}
            onChange={(event) =>
              patchOpsControl("connectors_hub", event.target.checked)
            }
          />
          <span>Connectors page</span>
        </label>
        <Chip label={connectorsStatus.label} tone={connectorsStatus.tone} />
      </div>
      <p className="mc-settings-help">
        Yellow means the page is ready but the main switch is still off, or the
        page is waiting for data to arrive.
      </p>
      {opsUxConfigError ? (
        <p className="mc-settings-inline-error">{opsUxConfigError}</p>
      ) : null}
    </>
  );
}
