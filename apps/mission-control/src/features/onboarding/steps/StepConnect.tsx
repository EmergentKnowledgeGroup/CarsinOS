import { OnboardingStepShell } from "../OnboardingStepShell";
import { DEFAULT_GATEWAY_URL } from "../../../constants";

interface StepConnectProps {
  busy: boolean;
  mode: "quickstart" | "manual";
  gatewayUrl: string;
  gatewayTokenInput: string;
  tokenConfigured: boolean;
  connected: boolean;
  onGatewayUrlChange: (value: string) => void;
  onGatewayTokenInputChange: (value: string) => void;
  onConnect: () => Promise<void>;
  onBack: () => void;
  onNext: () => void;
}

export function StepConnect(props: StepConnectProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 3 of 8"
      title="Connect Gateway"
      subtitle={
        props.tokenConfigured
          ? "Launcher/keychain token is already configured. Save connection to continue."
          : "Save connection settings and verify access using your bearer token."
      }
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button
            type="button"
            className="ghost"
            disabled={props.busy}
            aria-busy={props.busy}
            onClick={() => void props.onConnect()}
          >
            {props.busy ? "Connecting..." : "Save + Connect"}
          </button>
          <button type="button" disabled={!props.connected} onClick={props.onNext}>
            Continue
          </button>
        </>
      }
    >
      <div className="mc-onboarding-field-grid">
        <label>
          Gateway URL
          <input
            value={props.gatewayUrl}
            onChange={(event) => props.onGatewayUrlChange(event.target.value)}
            placeholder={DEFAULT_GATEWAY_URL}
          />
        </label>
        <label>
          Gateway token
          <input
            type="password"
            value={props.gatewayTokenInput}
            onChange={(event) => props.onGatewayTokenInputChange(event.target.value)}
            placeholder={
              props.tokenConfigured
                ? "Token preloaded (optional to override)"
                : "Paste bearer token"
            }
          />
        </label>
      </div>
      {props.mode === "manual" ? (
        <p className="mc-onboarding-note">
          Manual mode lets you override connection details before continuing.
        </p>
      ) : null}
      <p className="mc-onboarding-status-row">
        Connection status: <strong>{props.connected ? "Connected" : "Not connected"}</strong>
      </p>
    </OnboardingStepShell>
  );
}
