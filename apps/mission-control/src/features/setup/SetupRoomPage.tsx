import { Chip } from "../../ui/Chip";
import { Surface } from "../../ui/Surface";
import { PinRoomToOffice } from "../execassOffice/PinRoomToOffice";
import {
  ConnectionControls,
  FeatureControls,
  type SetupSurfaceProps,
} from "./SetupControls";
import { connectionStatusPresentation } from "./setupPresentation";

/**
 * The Basement Setup room: the product's gateway/token/feature-toggle/
 * onboarding surface. It renders the same shared controls, drafts, facts,
 * and callbacks as the AppShell Settings modal — one connection authority,
 * one feature-control authority, one onboarding wizard — at room density.
 */

interface SetupRoomPageProps {
  surface: SetupSurfaceProps;
}

export function SetupRoomPage({ surface }: SetupRoomPageProps) {
  const { gatewayHealthLabel, gatewayHealthTone, liveLinkLabel, tokenLabel } =
    connectionStatusPresentation(surface);

  return (
    <section className="mc-setup-room" data-testid="setup-room-page">
      <div className="mc-room-tabs-row mc-setup-room-row">
        <div className="mc-setup-room-status" data-testid="setup-room-status">
          <Chip label={gatewayHealthLabel} tone={gatewayHealthTone} />
          <Chip label={liveLinkLabel} tone={surface.wsState} />
          <Chip
            label={tokenLabel}
            tone={surface.tokenConfigured ? "connected" : "warning"}
          />
        </div>
        <PinRoomToOffice roomId="setup" />
      </div>
      <div className="mc-setup-room-grid">
        <Surface
          className="mc-setup-room-panel"
          title="Gateway connection"
          subtitle="Where this app talks to carsinOS: the gateway address, the secure token, and the reconnect and forget-token controls."
        >
          <ConnectionControls
            idPrefix="setup-room"
            gatewayDraft={surface.gatewayDraft}
            onGatewayDraftChange={surface.onGatewayDraftChange}
            tokenDraft={surface.tokenDraft}
            onTokenDraftChange={surface.onTokenDraftChange}
            tokenConfigured={surface.tokenConfigured}
            healthState={surface.healthState}
            wsState={surface.wsState}
            onSaveConnection={surface.onSaveConnection}
            onReconnect={surface.onReconnect}
            onClearToken={surface.onClearToken}
            onOpenSetupWizard={surface.onOpenSetupWizard}
            onOpenGuidedTour={surface.onOpenGuidedTour}
          />
        </Surface>
        <Surface
          className="mc-setup-room-panel"
          title="Feature switches"
          subtitle="The same rollout switches as Settings: what changes here shows there immediately, and the other way around."
        >
          <FeatureControls
            opsUxConfig={surface.opsUxConfig}
            opsUxConfigError={surface.opsUxConfigError}
            onPatchOpsUxControls={surface.onPatchOpsUxControls}
            usageChartsEnabled={surface.usageChartsEnabled}
          />
        </Surface>
      </div>
    </section>
  );
}
