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
  onConnect: () => Promise<boolean>;
  onBack: () => void;
  onNext: () => void | Promise<void>;
}

export function StepConnect(props: StepConnectProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 3 of 6"
      title="Connect Gateway"
      subtitle={
        props.tokenConfigured
          ? "Launcher or keychain already has the token. Confirm the URL and continue."
          : "Paste the gateway token once. Continue saves it and verifies access for you."
      }
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button type="button" disabled={props.busy} onClick={() => void props.onNext()}>
            {props.busy ? "Connecting..." : "Save connection + Continue"}
          </button>
        </>
      }
    >
      <div className="mc-onboarding-inline-actions">
        <button
          type="button"
          className="ghost"
          disabled={props.busy}
          aria-busy={props.busy}
          onClick={() => {
            if (props.busy) {
              return;
            }
            void props.onConnect();
          }}
        >
          {props.busy ? "Connecting..." : "Save + Connect now"}
        </button>
      </div>
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
            type="text"
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
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
      ) : (
        <p className="mc-onboarding-note">
          If the launcher already opened carsinOS for you, you usually only need the default URL
          and the token once.
        </p>
      )}
      <p className="mc-onboarding-status-row">
        Connection status: <strong>{props.connected ? "Connected" : "Not connected"}</strong>
      </p>
    </OnboardingStepShell>
  );
}
