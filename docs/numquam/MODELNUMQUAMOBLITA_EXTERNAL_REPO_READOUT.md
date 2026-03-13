# ModelNumquamOblita External Repo Readout

Generated: 2026-03-10
Source inspected: `/Volumes/ultariumv3/modelNumquamOblita`

## Bottom Line

The current `ModelNumquamOblita` repo is not just a memory library.

It is already a standalone memory runtime with:

- a native HTTP runtime server
- a separate MCP server
- a local browser UI
- a full memory operations API
- a wizard/pipeline flow
- explicit carsinOS integration contract docs
- integration contract tests for HTTP and MCP parity

Pragmatically, this means carsinOS is not waiting on MNO to invent the right primitives.
The main remaining work is deciding how cleanly to bind carsinOS to the existing MNO runtime surfaces, auth model, and operator-visible APIs.

## Current Repo Shape

Primary structure:

- `engine/`
  - `runtime/` runtime session logic, HTTP server, adapters, browser UI
  - `mcp/` MCP server and transport/auth logic
  - `memory/` atom store and mutation queue
  - `retrieval/` retrieval and verifier logic
  - `continuity/` continuity and shared-language layers
  - `ingest/` import/parsing/extraction pipeline
  - `write_gate/` staged write safety logic
- `tools/`
  - setup, launch, packaging, eval, pilot, runtime, MCP connector, diagnostics
- `docs/`
  - integration requirements, API matrix, runtime UI tour, architecture, setup, eval and execution specs
- `tests/`
  - unit and integration coverage, including integration-v1 contract tests

Runtime/language baseline:

- Python `>=3.12`
- deliberately low-dependency / stdlib-heavy runtime shape
- HTTP servers implemented with `http.server`

Key entrypoints:

- `tools/run_runtime_demo.py`
- `tools/run_live_runtime.py`
- `tools/run_mcp_server.py`
- `tools/run_claude_live_mcp.py`

## What Is Already Concrete

### 1. carsinOS contract support is explicit

This repo already contains:

- `docs/CARSINOS_INTEGRATION_REQUIREMENTS.md`
- `tests/integration/test_integration_contract_api.py`

That is the strongest signal in the whole readout.
The external repo is already shaped around a carsinOS-facing contract rather than needing a fresh adapter spec invented from scratch.

### 2. Integration v1 HTTP endpoints already exist

Implemented in `engine/runtime/server.py`:

- `POST /api/integration/v1/context/build`
- `POST /api/integration/v1/context/why`
- `POST /api/integration/v1/writeback/propose`
- `POST /api/integration/v1/writeback/resolve`
- `GET /api/integration/v1/health`
- `GET /api/integration/v1/capabilities`

The route map in code matches the carsinOS-side expectations almost exactly.

### 3. MCP parity already exists

Implemented in `engine/mcp/server.py`:

- `integration.context.build`
- `integration.context.why`
- `integration.writeback.propose`
- `integration.writeback.resolve`
- `integration.capabilities.get`
- `integration.health.get`

This is not aspirational.
The repo includes parity tests across HTTP and MCP.

### 4. The repo has more than the narrow integration contract

Native MNO runtime APIs already expose operator-facing memory functionality beyond the carsinOS integration boundary:

- chat/session runtime APIs
- route preview and context-package APIs
- turn explainability / why APIs
- memory cards / atoms / graph APIs
- episode memory APIs
- mutation proposal APIs
- runtime health / telemetry / provider config APIs
- wizard/pipeline APIs

This matters because a future carsinOS `Memory` tab can either:

- stay narrowly bound to integration-v1 only, or
- surface richer native MNO runtime APIs directly

## Existing Operator/UI Surfaces In MNO

The external repo already ships a local browser UI in `engine/runtime/ui/`.

Documented/operator-visible surfaces include:

- chat shell with session management
- route preview and context-package inspection
- `Why This Answer?` panel
- telemetry ledger
- memory workbench
  - atoms/cards
  - episodes
  - memory graph
- proposal inbox
- wizard import/build/review/verify/go-live flow
- ops/health panel

This is important for carsinOS because the conceptual information architecture for a `Memory` tab is already present in MNO:

- explainability
- evidence browsing
- episode browsing
- proposal review
- health/ops status

## Auth And Runtime Security Shape

The repo is not auth-free.

Relevant behavior found in `engine/mcp/server.py` and docs:

- bearer auth is required for integration routes/tools when tokens are configured
- role model is `viewer | operator | admin`
- writeback mutation operations require `operator` or `admin`
- envelope `principal` is metadata only, not authorization authority
- production mode is intended to fail closed if local default integration tokens remain enabled
- default local integration tokens still exist for local/dev workflows when explicitly enabled

Current dev/default-token identifiers still present:

- `local-integration-viewer-token`
- `local-integration-operator-token`
- `local-integration-admin-token`

Pragmatic read:

- this is safer than an ad hoc unauthenticated sidecar
- it is still carrying local/dev convenience auth that should be treated carefully during real carsinOS wiring

## carsinOS-Relevant Interface Inventory

### Narrow contract surfaces

Best immediate adapter candidates for carsinOS gateway:

- integration-v1 HTTP endpoints
- integration-v1 MCP tools

These already line up with the carsinOS Numquam expectations documented in:

- `docs/numquam/CARSINOS_NUMQUAM_INTEGRATION_STATE_MAP.md`

### Rich operator surfaces

Best candidates for future direct frontend exposure:

- `GET /api/runtime/health`
- `GET /api/state`
- `GET /api/turns/<turn_id>/why`
- `GET /api/memory/cards`
- `GET /api/memory/atoms`
- `GET /api/memory/episodes`
- `GET /api/memory/proposals`
- mutation proposal approve/reject endpoints
- wizard and diagnostics endpoints

## Integration Implications For carsinOS

### Strong alignment

The external repo appears intentionally aligned to the current carsinOS MNO contract.

That reduces integration risk substantially.

The most likely clean first integration path is:

1. keep carsinOS gateway speaking the existing integration-v1 contract
2. wire it to this external MNO runtime over HTTP first
3. treat MCP parity as secondary/optional until the clean handoff folder is final

### `Memory` tab likely should not be limited to the integration contract

If the goal is a useful operator-facing `Memory` tab in Mission Control, the better source data is probably the native MNO runtime API rather than the narrow integration-v1 adapter surface.

Most likely `Memory` tab building blocks:

- MNO health/degrade status
- turn-level explainability (`why`)
- proposal inbox state
- cards/atoms browsing
- episode browsing/edit state
- possibly wizard/run status if you want memory-pipeline visibility in carsinOS

### Existing carsinOS gaps this repo could directly fill

Based on current carsinOS state, this external repo could fill the missing frontend/operator surfaces for:

- MNO status in `Cockpit`
- `Memory` tab content and actions
- run explainability visibility
- proposal review visibility
- sync/diagnostic visibility

## Likely Clean Integration Boundary

The cleanest near-term boundary still looks like:

- MNO owns memory runtime, evidence store, retrieval, explainability, mutation proposal logic, and MNO-native operator APIs
- carsinOS owns orchestration, approvals, strategy/runbook/runtime product shell, and cross-surface operator UX

That implies:

- do not embed MNO’s existing browser UI inside carsinOS
- do integrate to its HTTP APIs cleanly
- use carsinOS Mission Control to present the selected MNO operator truths in carsinOS-native UX

## Current Risks / Unknowns

These are the main things still worth verifying when the cleaned folder is ready:

1. Whether the reviewed folder preserves the currently implemented integration-v1 endpoints unchanged.
2. Whether HTTP is the intended production transport for carsinOS, or whether MCP remains first-class for this handoff.
3. How auth tokens and role provisioning should be owned between carsinOS gateway and MNO runtime.
4. Whether carsinOS should consume only integration-v1 for backend runtime use, while consuming richer native MNO APIs for operator UI.
5. Which parts of MNO’s wizard/pipeline flow should remain MNO-local versus being surfaced or mirrored in Mission Control.

## Practical Read

The external repo is already far enough along that I would not design the carsinOS integration from scratch anymore.

I would treat this as:

- an existing standalone memory product with a carsinOS adapter already emerging
- a backend/runtime contract that is mostly real now
- a frontend/operator opportunity to expose MNO in carsinOS through a `Memory` tab and a few adjacent status surfaces

The main job after the cleaned handoff will be contract validation, boundary cleanup, and productized exposure inside Mission Control.
