export const CONNECTOR_UNSUPPORTED_STATUS_FRAGMENTS = [
  "404",
  "not found",
  "cannot get /api/v1/connectors",
  "/api/v1/connectors/catalog",
  "/api/v1/connectors/interactions",
] as const;

export const CONNECTOR_SOURCE_KIND_OPTIONS = ["openapi", "graphql", "mcp"] as const;
export const CONNECTOR_ORIGIN_KIND_OPTIONS = [
  "curated",
  "imported_local",
  "imported_url",
] as const;
export const CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS = [
  "inline_only",
  "allowlisted_fetch",
  "reject_external",
] as const;
export const CONNECTOR_STATUS_OPTIONS = [
  "draft",
  "converted",
  "under_review",
  "enabled",
  "disabled",
  "error",
] as const;
export const CONNECTOR_TRUST_STATE_OPTIONS = [
  "trusted_curated",
  "local_untrusted",
  "reviewed_local",
  "blocked",
] as const;
export const CONNECTOR_CONVERSION_STATUS_OPTIONS = [
  "pending",
  "running",
  "succeeded",
  "failed",
] as const;
export const CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS = [
  "shared_default",
  "agent_override",
] as const;
export const CONNECTOR_AUTH_KIND_OPTIONS = [
  "none",
  "bearer",
  "header",
  "query",
  "oauth_session",
] as const;
export const CONNECTOR_AUTH_STATUS_OPTIONS = [
  "ready",
  "pending",
  "error",
  "expired",
  "unconfigured",
] as const;
export const CONNECTOR_INTERACTION_STATUS_OPTIONS = [
  "pending",
  "waiting_on_operator",
  "resumed",
  "cancelled",
  "expired",
] as const;
export const CONNECTOR_WRITE_CLASSIFICATION_OPTIONS = [
  "read_only",
  "operator_write_gated",
  "destructive_write_gated",
  "unsafe_blocked",
] as const;
export const CONNECTOR_DEPRECATION_STATE_OPTIONS = [
  "active",
  "unpublished",
  "superseded",
] as const;

export const CONNECTOR_JSON_TEXTAREA_ROWS = 8;
export const CONNECTOR_TEXTAREA_ROWS = 6;
export const CONNECTOR_INTERACTION_PAYLOAD_PLACEHOLDER = `{
  "note": "operator follow-up completed"
}`;
