/**
 * Shared status presentation for the one Setup authority. Settings and the
 * Basement Setup room compute identical labels from identical facts, so the
 * two surfaces can never disagree about connection or rollout truth.
 */

export function connectionStatusPresentation(props: {
  healthState: string;
  wsState: string;
  tokenConfigured: boolean;
}) {
  const gatewayHealth =
    props.healthState === "healthy" || props.healthState === "up"
      ? { label: "Gateway: Healthy", tone: "up" }
      : props.healthState === "degraded" || props.healthState === "down"
        ? { label: "Gateway: Needs attention", tone: "down" }
        : props.healthState === "checking"
          ? { label: "Gateway: Checking", tone: "checking" }
          : props.healthState === "idle"
            ? { label: "Gateway: Waiting", tone: "idle" }
            : {
                label: `Gateway: ${props.healthState}`,
                tone: props.healthState,
              };
  const liveLinkLabel =
    props.wsState === "connected"
      ? "Live link: Connected"
      : props.wsState === "connecting"
        ? "Live link: Connecting"
        : props.wsState === "idle"
          ? "Live link: Waiting"
          : `Live link: ${props.wsState}`;
  const tokenLabel = props.tokenConfigured
    ? "Token: Configured"
    : "Token: Missing";
  return {
    gatewayHealthLabel: gatewayHealth.label,
    gatewayHealthTone: gatewayHealth.tone,
    liveLinkLabel,
    tokenLabel,
  };
}

export function rolloutStatePresentation(
  masterOn: boolean,
  enabled: boolean,
): { label: string; tone: string } {
  if (!masterOn && enabled) {
    return { label: "Waiting on main switch", tone: "warning" };
  }
  return enabled
    ? { label: "On", tone: "connected" }
    : { label: "Off", tone: "" };
}
