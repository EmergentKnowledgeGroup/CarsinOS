# carsinOS Unified Connector Registry Spec

Generated: 2026-03-12
Status: Finalized after SpecSwarm review

## 1. Purpose

Build a first-class connector and agentic workflow layer in carsinOS that gives every agent, including the assistant, one consistent way to discover, inspect, authenticate, and call external tools.

This layer is inspired by the useful workflow ideas in `executor`, but it must be implemented natively in Rust for carsinOS and fit the existing carsinOS runtime, safety, and operator-control model.

The goal is not to absorb `executor`.

The goal is to make carsinOS itself the canonical shared connector/tool brain.

## 2. Product Thesis

carsinOS should support:

- one shared connector/source registry
- one normalized tool catalog derived from that registry
- one consistent discover/describe/call workflow for all agents
- one durable auth and pause/resume interaction model
- one consistent safety, approval, policy, and audit path

The target architecture is:

`connector/source registry -> convert/normalize -> reviewed published tools -> existing carsinOS runtime/tool path`

This new connector layer sits above the current plugin/tool runtime. It does not replace the gateway, Mission Control, approvals, audit, or tool execution engine.

## 3. Goals

1. Give all agents the same external-tool workflow.
2. Let outside MCP, OpenAPI, GraphQL, and similar connectors plug in cleanly.
3. Support a curated connector catalog plus operator import in v1.
4. Keep imported connectors dynamic and configurable without creating unsafe auto-live behavior.
5. Keep Connectors as the canonical source of truth while still exposing generated artifacts inside existing carsinOS surfaces.
6. Preserve existing carsinOS runtime safety, approval, and audit semantics.

## 4. Non-Goals

- No executor sidecar or second runtime/control plane in v1.
- No public remote marketplace in v1.
- No auto-live outside connector imports.
- No direct frontend-to-connector execution path.
- No replacement of the existing plugin/tool execution engine.
- No requirement that all connectors be executable binaries or classic plugins.

## 5. Locked Decisions

### 5.1 Core architecture

- The connector model is `source registry on top`.
- Connectors are first-class product objects, not just renamed plugins.
- The existing carsinOS plugin/tool runtime remains the execution substrate.

### 5.2 Shared catalog and assignment

- carsinOS maintains one shared canonical connector catalog.
- Agents receive access through per-agent connector allowlists.
- v1 assignment is connector-level, not raw tool-by-tool assignment.

### 5.3 First-class connector inputs

V1 supports:

- MCP
- OpenAPI
- GraphQL

Other connector forms may be added later through the same registry model.

### 5.4 Activation workflow

Outside connectors follow:

`import -> convert -> review -> enable`

This is mandatory for outside connectors.

### 5.5 Connector market shape

V1 ships:

- a curated built-in catalog
- operator import from file/URL/package
- scaffolding for a future remote marketplace

V1 does not fetch public marketplace inventory from a hosted service.

### 5.6 Runtime behavior

- Reads are normal if the connector is enabled, the published tool belongs to the current live version, and the connector is assigned.
- Write actions are explicitly classified and must remain gated by capability, policy, and approval rules.
- Generated connector-backed tools appear in both `Connectors` and `Extensions`.
- `Extensions` is a read-only mirror for connector-generated artifacts.
- All real editing, conversion, review, publish, assignment, and auth flows are owned by `Connectors`.

### 5.7 Auth model

V1 supports both:

- shared connector-level auth
- per-agent auth override

Precedence:

- per-agent override wins if present
- otherwise shared connector auth is used

### 5.8 Trust model

- Unsigned local imports are allowed in v1.
- Unsigned imports are not trusted by default.
- No imported connector becomes live without conversion, review, and explicit enable.

### 5.9 Update model

Connector version/schema changes must follow:

`re-convert -> diff -> review -> re-enable`

No silent auto-republish of changed connector-derived tools is allowed.

Live-state guardrails:

- importing or converting a new version must not mutate the currently enabled live version
- `ConnectorSource.current_version_id` only changes after successful review and explicit re-enable
- failed conversion or abandoned review must leave the previously enabled published tool set intact
- operators must be able to roll back by re-enabling a previously reviewed published version/tool set without reconstructing it from scratch

### 5.10 State and transition model

Connector lifecycle state must be explicit and enforceable.

Default state sets:

- `ConnectorSource.status`
  - `draft`
  - `converted`
  - `under_review`
  - `enabled`
  - `disabled`
  - `error`
- `ConnectorSource.trust_state`
  - `trusted_curated`
  - `local_untrusted`
  - `reviewed_local`
  - `blocked`
- `ConnectorConversion.status`
  - `pending`
  - `running`
  - `succeeded`
  - `failed`
- `ConnectorInteraction.status`
  - `pending`
  - `waiting_on_operator`
  - `resumed`
  - `cancelled`
  - `expired`

State transition rules:

- imported connectors start as `draft`
- successful conversion moves the source to `converted`
- starting review moves the source to `under_review`
- publish+enable moves the source to `enabled`
- disable moves the source to `disabled`
- conversion or publish failure may move the source to `error`
- unsigned imports start as `local_untrusted`
- reviewed unsigned imports move to `reviewed_local`
- blocked imports move to `blocked`

No state change may occur without an audit event.

Canonical state rules:

- `ConnectorSource.status` is the canonical connector enable/disable state
- `ConnectorSource.current_version_id` is the canonical live version pointer
- `ConnectorConversion.status` is the canonical conversion state
- any version-level conversion summary is derived read-model data only
- published tool records are immutable publication records; runtime callability is derived from connector status, current live version, assignment, and policy

### 5.11 Write classification model

Connector operations must be classified during conversion and preserved through review/publish/runtime discovery.

V1 write classes:

- `read_only`
- `operator_write_gated`
- `destructive_write_gated`
- `unsafe_blocked`

Rules:

- `read_only` operations may run normally if the connector is enabled, assigned, and the operation is published as part of the current live version
- `operator_write_gated` operations require explicit policy allowance and approval gating
- `destructive_write_gated` operations require the stricter approval/policy path used for high-risk tools
- `unsafe_blocked` operations may be visible in review output but must never be published as live tools

If a converter cannot confidently classify an operation, it must default to the stricter non-read class for review and may not auto-publish.

## 6. Canonical Objects

carsinOS must add first-class connector objects to protocol/storage/gateway contracts.

### 6.1 `ConnectorCatalogItem`

Represents a curated listing or future marketplace listing.

Minimum shape:

- `catalog_item_id`
- `slug`
- `display_name`
- `source_kind`
- `summary`
- `publisher`
- `trust_class`
- `available_versions`
- `marketplace_origin`
- `importable`
- `future_marketplace_metadata`

### 6.2 `ConnectorSource`

Represents one installed connector instance in the local registry.

Minimum shape:

- `connector_id`
- `slug`
- `display_name`
- `source_kind`
- `origin_kind` (`curated`, `imported_local`, `imported_url`, future marketplace values)
- `catalog_item_id` nullable
- `current_version_id`
- `latest_imported_version_id`
- `status`
- `trust_state`
- `assigned_agent_count`
- `published_tool_count`
- `last_conversion_at`
- `last_review_at`
- `last_enabled_at`
- `last_disabled_at`

### 6.3 `ConnectorVersion`

Represents one imported or curated source version.

Minimum shape:

- `version_id`
- `connector_id`
- `version_label`
- `source_digest`
- `raw_source_location`
- `import_metadata`
- `schema_summary`
- `latest_conversion_id`
- `external_reference_policy`

Allowed `external_reference_policy` values:

- `inline_only`
- `allowlisted_fetch`
- `reject_external`

Rules:

- `inline_only` means referenced schemas/documents must be normalized into the stored source package before conversion
- `allowlisted_fetch` means referenced schemas/documents may be fetched only during import through gateway-controlled allowlisted fetch rules, then must be normalized into the stored source package
- `reject_external` means any unresolved external reference fails import/conversion

### 6.4 `ConnectorConversion`

Represents a normalized conversion pass from MCP/OpenAPI/GraphQL into carsinOS tool candidates.

Multiple conversion attempts for the same source version are allowed. `ConnectorVersion.latest_conversion_id` points at the most recent attempt, but only a reviewed successful conversion may be promoted to the live version.

Minimum shape:

- `conversion_id`
- `connector_id`
- `version_id`
- `status`
- `warnings`
- `proposed_tools`
- `write_capable_tools`
- `unsupported_operations`
- `normalization_notes`
- `diff_from_previous`

### 6.5 `ConnectorPublishedTool`

Represents a reviewed, published connector-derived tool record that may become live when its connector is enabled and its version is the current live version.

Minimum shape:

- `published_tool_id`
- `connector_id`
- `version_id`
- `conversion_id`
- `tool_name`
- `display_name`
- `tool_schema`
- `origin_metadata`
- `write_classification`
- `published_at`
- `unpublished_at` nullable
- `superseded_by_published_tool_id` nullable
- `deprecation_state`

### 6.6 `ConnectorAssignment`

Represents connector access granted to an agent.

Minimum shape:

- `assignment_id`
- `connector_id`
- `agent_id`
- `enabled`
- `auth_mode` (`shared_default`, `agent_override`)

### 6.7 `ConnectorAuthBinding`

Represents connector auth/session ownership.

Minimum shape:

- `auth_binding_id`
- `connector_id`
- `agent_id` nullable for shared auth
- `auth_kind`
- `secret_ref` nullable
- `oauth_session_id` nullable
- `status`
- `last_success_at`
- `last_error`
- `last_rotated_at`

### 6.8 `ConnectorInteraction`

Represents durable pause/resume state for OAuth, auth repair, or structured human interaction.

Minimum shape:

- `interaction_id`
- `connector_id`
- `agent_id` nullable
- `interaction_kind`
- `status`
- `prompt_summary`
- `resume_token`
- `expires_at`
- `consumed_at` nullable
- `created_at`
- `updated_at`

Rules:

- `resume_token` must be one-time use
- `resume_token` must expire after a bounded TTL
- resume or cancel must invalidate the token immediately
- expired interactions remain visible for audit and operator repair, but may not be resumed without creating a fresh interaction

## 7. Conversion and Publishing Lifecycle

### 7.1 Import sources

Connectors may enter carsinOS through:

- curated catalog install
- local file import
- URL import
- portable secret-free connector package import

All imported sources must be stored as source versions before conversion.

Import security rules:

- URL imports must pass gateway-owned SSRF protections
- URL imports must obey allow/deny policy and file size limits
- imports must record a digest for the fetched source
- digest mismatch between fetched source and stored version must fail closed
- external reference handling must follow `external_reference_policy`
- v1 must not follow arbitrary secondary remote references at execution time
- unsigned imports are allowed, but trusted execution is never implied by import success

### 7.2 Conversion

Each connector source version is converted into normalized tool candidates.

Conversion must:

- normalize names
- normalize schemas
- classify read vs write behavior
- mark unsupported or unsafe operations
- attach origin metadata
- prepare generated artifacts for runtime exposure
- assign an explicit write classification to every proposed tool
- assign stable generated ids/names deterministically
- record warnings for naming collisions, unsupported operations, and downgraded classifications
- fail with a blocking review error if deterministic naming would collide and no explicit operator alias override has been provided

### 7.3 Review

Operator review must support:

- seeing proposed operations/tools
- selecting which operations become live tools
- seeing warnings and unsupported items
- seeing changes vs prior published version

All-or-nothing review is not the default.

Default review behavior:

- selection defaults to `select none`
- operators must explicitly choose which proposed operations publish
- collision overrides must be explicit operator choices stored with the review result so future conversions remain deterministic

### 7.4 Enable and publish

Only reviewed, selected operations become published connector-backed tools.

Publishing must:

- create stable tool ids/names
- expose the results through existing tool discovery/capability surfaces
- mark them as connector-origin tools
- mirror them into `Extensions` as read-only generated entries

Disable/unpublish rules:

- disabling a connector stops new connector-backed tool executions from starting
- disabling a connector does not change `current_version_id` or erase the current published tool set
- disabling a connector does not erase audit history, prior run history, or published review records
- disabling a connector does not mutate already-completed tool calls
- disabling a connector allows already-running executions to continue to natural completion; each such execution must emit disable-aware audit metadata
- queued approvals for disabled connector-backed tools must resolve as blocked/cancelled rather than executing against a disabled connector
- active connector interactions remain visible after disable, but may not resume execution until the connector is re-enabled or explicitly repaired
- unpublish/supersede applies at the published tool record layer, but nothing becomes callable unless the connector source is enabled and points at the relevant live version

## 8. Runtime and Tooling Behavior

### 8.1 Canonical tool naming

Connector-generated tools must use stable, namespaced names that do not collide with existing core or plugin tools.

Default naming shape:

`connector.<connector_slug>.<operation_slug>`

Naming stability rules:

- generated names must be deterministic for the same connector source + operation identity
- collisions between curated and imported connectors must never rename existing live tools silently
- if a generated name collides, conversion must fail into review with a blocking warning unless the operator provides an explicit alias override during review
- renamed or removed source operations must surface as diff/review events, not silent live replacements
- deprecated or removed published tools must remain auditable even after unpublish

### 8.2 Discovery flow

All agents must be able to:

- discover available tools
- inspect schemas/descriptions
- call tools through one consistent runtime path

Discovery must reflect:

- connector assignment
- connector live enable state
- published/superseded state for the current live version
- policy restrictions
- write classification

### 8.3 Execution ownership

Tool execution remains owned by the existing carsinOS runtime path.

Connector-derived tools must reuse:

- approval gating
- audit logging
- breaker behavior
- policy enforcement
- operator allowlists where applicable

Write classification is runtime-enforced behavior, not metadata only. Execution may not bypass approval/policy checks even if the upstream connector advertises weaker semantics.

### 8.4 Pause and resume

If connector auth or connector-required human input interrupts execution:

- execution must pause durably
- a `ConnectorInteraction` must be created or updated
- the system must be able to resume without losing connector state

## 9. Mission Control UX

### 9.1 New `Connectors` tab

Mission Control should add a first-class `Connectors` tab.

Primary areas:

- `Catalog`
- `Installed`
- `Review`
- `Auth + Interactions`
- `Health`

### 9.2 Operator actions

V1 operator actions should include:

- browse curated catalog
- import connector from file/URL/package
- inspect converted operations
- select operations for publishing
- enable/disable connector
- assign/unassign connector to agents
- view shared auth and per-agent auth overrides
- reconnect or repair auth
- resume paused connector interactions
- export secret-free connector package

### 9.3 Extensions mirror

Connector-generated artifacts should appear under existing `Extensions` surfaces with:

- origin badges
- generated/read-only markers
- deep links back to `Connectors`

`Extensions` must not become a second editing surface for connector-owned artifacts.

Visibility rules:

- `Extensions` mirrors currently published connector-backed artifacts
- if a connector is disabled, its mirrored entries remain visible with disabled/unavailable badges and no callable affordance
- superseded or fully unpublished historical records remain visible in `Connectors` audit/history views, not in the primary `Extensions` mirror

## 10. Security and Safety Rules

1. Connector secrets and live auth tokens must never be exposed in frontend state, logs, exports, or checkpoints.
2. Exported connector packages must be secret-free.
3. Unsigned imports must remain untrusted until reviewed and enabled.
4. Write-capable connector tools must remain explicit, inspectable, and approval/policy gated.
5. No connector update may silently mutate live published tools.
6. Connectors must not bypass existing carsinOS tool safety, audit, or approval controls.
7. Every connector import, conversion, review, enable/disable, assignment change, auth change, and interaction resume/cancel must create a durable audit event.
8. Connector diffs, warnings, and review metadata must be treated as potentially sensitive and remain subject to the same redaction/storage rules as other operational metadata.

## 11. Public API / Contract Additions

carsinOS must add connector-oriented gateway/protocol contracts for:

- catalog list/detail
- installed connectors list/detail
- connector import
- connector export
- connector conversion run/detail
- connector diff review
- publish/unpublish operations
- rollback/re-enable a previously reviewed live version
- assignment CRUD
- auth/session CRUD
- interaction list/detail/resume/cancel
- connector runtime health

Existing tool capability contracts must be extended with connector-origin metadata.

The public contract must also expose:

- write classification
- trust state
- generated/read-only mirror state
- connector/version ids used to derive a published tool

## 12. Testing Requirements

### Backend

- import MCP/OpenAPI/GraphQL sources
- conversion success and failure cases
- unsafe or malformed import rejection
- SSRF, oversize import, and digest mismatch rejection
- unsigned import remains non-live until review/enable
- connector update requires re-review
- assignment filtering per agent
- shared auth vs per-agent override precedence
- write-capable tool gating
- durable interaction pause/resume
- read-only mirror behavior for generated extension entries
- disable/unpublish behavior for in-flight execution and queued approvals
- naming collision, rename, deprecation, and removal behavior across versions
- audit emission for import/review/enable/assignment/auth/interaction transitions

### Frontend

- Connectors tab core flows
- catalog/import/review/assignment/auth/health states
- generated read-only mirror visibility in Extensions
- secret-safe rendering
- explicit degraded and auth-required states

### End-to-end

- import -> convert -> review -> enable -> assign -> discover -> call
- same connector-backed tool behaves consistently across multiple agents
- per-agent auth override affects only that agent
- write-capable connector tool pauses or requests approval correctly
- connector update produces diff and requires explicit re-enable
- disabling a connector blocks new executions without corrupting existing audit history

## 13. Implementation Touchpoints Map

This section maps the spec to the likely implementation seams in the current repo so build work stays additive and ownership remains clear.

### 13.1 Protocol and storage

Expected ownership:

- `crates/carsinos-protocol/src/lib.rs`
  - connector DTOs
  - request/response contracts
  - connector-origin metadata added to existing tool capability contracts
- `crates/carsinos-storage/src/lib.rs`
  - connector source/version/conversion/published-tool persistence helpers
  - assignment/auth/interaction persistence helpers
  - enable/disable, trust-state, and audit-oriented storage transitions
- `migrations/`
  - new connector registry tables
  - version/conversion/published tool tables
  - assignment/auth/interaction tables

Likely symbols:

- `ConnectorCatalogItem`
- `ConnectorSource`
- `ConnectorVersion`
- `ConnectorConversion`
- `ConnectorPublishedTool`
- `ConnectorAssignment`
- `ConnectorAuthBinding`
- `ConnectorInteraction`
- `Storage::import_connector_source`
- `Storage::record_connector_conversion`
- `Storage::publish_connector_tools`
- `Storage::set_connector_assignment`
- `Storage::record_connector_interaction`

### 13.2 Gateway and runtime integration

Expected ownership:

- `crates/carsinos-gateway/src/main.rs`
  - connector routes
  - connector-enabled tool capability exposure
  - assignment/auth/interactions/health handlers
- optional extracted module such as `crates/carsinos-gateway/src/connectors.rs`
  - import/convert/review/publish orchestration
  - connector interaction and health helpers
- existing runtime/tool execution path
  - connector-derived tools must reuse current approval, audit, policy, and breaker behavior rather than introducing a second execution engine

Likely symbols:

- `ConnectorRegistry`
- `ConnectorImporter`
- `ConnectorConverter`
- `ConnectorPublisher`
- `ConnectorReviewService`
- `ToolRegistry::register_connector_tool`
- connector-aware `list_tool_capabilities` helpers
- gateway handlers such as `list_connectors`, `import_connector`, `publish_connector_tools`, `resume_connector_interaction`, and `get_connector_health`

### 13.3 Provider and conversion adapters

Expected ownership:

- `crates/carsinos-providers/src/lib.rs` or adjacent provider modules
  - source-kind adapters for MCP/OpenAPI/GraphQL normalization
  - schema normalization and operation classification helpers
  - stable generated naming/id logic

Likely symbols:

- `McpConnectorAdapter`
- `OpenApiConnectorAdapter`
- `GraphQlConnectorAdapter`
- `ConnectorConversionWarning`
- `ConnectorOperationClassifier`

### 13.4 Mission Control product surface

Expected ownership:

- `apps/mission-control/src/app/tabs.ts`
  - add `Connectors` tab metadata
- `apps/mission-control/src/app/useAppController.ts`
  - tab availability, navigation, and deep-link ownership
- `apps/mission-control/src/lib/api.ts`
  - connector list/detail/import/review/publish/assignment/auth/interaction/health API wrappers
- `apps/mission-control/src/types.ts`
  - connector DTOs and connector-origin tool metadata
- `apps/mission-control/src/features/connectors/`
  - controller and page components for Catalog, Installed, Review, Auth + Interactions, and Health
- `apps/mission-control/API_CONTRACT.md`
  - operator-facing contract documentation for the new surface

Likely symbols:

- `useConnectorsController`
- `ConnectorsPage`
- `ConnectorReviewPanel`
- `ConnectorAuthPanel`
- `ConnectorHealthCard`
- `ConnectorAssignmentEditor`

### 13.5 Adjacent product integrations

Connector ownership stays in `Connectors`, but additive read-path integrations are expected in:

- existing tool capability and assistant surfaces
  - connector-origin badges and write classification visibility
- `Extensions`
  - read-only mirror entries with deep links back to `Connectors`
- `Team`
  - connector assignment visibility and per-agent auth override state
- `Cockpit`
  - connector health/degraded/auth-required summaries if surfaced as widgets later

### 13.6 Test surfaces

Expected test ownership:

- `crates/carsinos-storage/src/lib.rs`
  - storage unit tests for conversion, assignment, trust, and auth precedence behavior
- `crates/carsinos-gateway/tests/e2e_process.rs`
  - connector import/convert/review/enable/assign/call workflows
- additional gateway test modules if extraction is cleaner than expanding `e2e_process.rs`
- `apps/mission-control/src/lib/api.test.ts`
  - connector client typing and secret-safe contract behavior
- `apps/mission-control/e2e/`
  - `connectors.spec.ts` and related UI regression coverage

## 14. Rollout Shape

### Phase 0

- connector/source registry model
- connector contracts/storage
- generated tool origin metadata
- Connectors tab shell
- rollout guard so Mission Control hides or degrades the tab cleanly if connector endpoints are unavailable

### Phase 1

- MCP/OpenAPI/GraphQL conversion pipeline
- review and publish flow
- connector assignment
- read-path discovery and stable execution path

### Phase 2

- durable auth + pause/resume interactions
- export/import package polish
- curated catalog expansion
- future marketplace scaffolding hardening

Backout/default-safe rules:

- disabling the feature flag must hide the `Connectors` tab and exclude connector-backed tools from discovery without deleting stored connector records
- failed migrations or disabled connector routes must degrade read paths cleanly rather than hard-failing Mission Control startup
- rollback to a previous reviewed live version must be an explicit supported operator action

## 15. Explicit Defaults

- `Connectors` is the canonical owner
- `Extensions` is a read-only mirror for generated connector-backed artifacts
- v1 ships curated catalog plus operator import
- v1 supports both shared auth and per-agent overrides
- shared auth is default
- per-agent override wins if configured
- v1 supports connector-level assignment
- tool-level assignment is deferred
- outside connector updates always require re-review
- unsigned imports remain non-live until explicit review+enable
