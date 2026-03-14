# Executor vs carsinOS Strategy Assessment

Generated: 2026-03-12

Repo assessed:

- `rhyssullivan/executor`
- local clone head: `904b64e`
- latest local clone commit: `2026-03-11 hide password`

## Executive Summary

`executor` is one of the more relevant external repos to study for carsinOS, but not because it looks like Mission Control.

It does not.

`executor` is not an operator console, not a scheduler, not a multi-channel runtime center, and not a broader mission-control product. It is a local-first control plane for agent tool use.

Its core idea is simple:

- connect external sources once
- turn them into a reusable typed tool catalog
- let an agent discover, inspect, and call those tools through one structured local runtime
- pause and resume cleanly when auth or human interaction is needed

That means the real value to carsinOS is not UI inspiration first.

The real value is in the agentic workflow layer.

`executor` is strongest where it improves how an agent works with external systems:

- MCP servers
- OpenAPI APIs
- GraphQL endpoints
- credentials and OAuth
- human-in-the-loop interaction during tool use

So the cleanest conclusion is:

1. `executor` is not worth absorbing wholesale.
2. `executor` is not a replacement for carsinOS runtime or Mission Control.
3. `executor` does contain high-value ideas for carsinOS's agentic workflow.
4. The highest-value area is its "source -> tool catalog -> discover/describe/call -> pause/resume interaction" model.
5. The most sensible next step, if you wanted to use it, would be a narrow sidecar prototype or selective concept absorption.

## Bottom-Line Recommendation

Recommended posture:

1. Keep `carsinOS` as the primary runtime, safety layer, and operator control plane.
2. Keep `Mission Control` as the operator-facing system of record.
3. Treat `executor` as a possible connector and tool-control layer, not as a product replacement.
4. Borrow or prototype the following ideas:
   typed external source catalog, discover/describe/call workflow, durable source auth handling, durable pause/resume interaction state.
5. Avoid handing core orchestration ownership over to `executor`.

## High-Level Comparison

| Area | Mission Control | Internal carsinOS controls | executor |
| --- | --- | --- | --- |
| Primary role | Operator console | Runtime / gateway / safety plane | Local agent tool control plane |
| Main nouns | Boards, focus, calendar, events, approvals, mail, rooms, team, cockpit, strategy | Sessions, runs, approvals, jobs, channels, tools, plugins, skills, runtime config, security audit | Sources, workspaces, tool catalog, secrets, auth sessions, executions, interactions, policies |
| Main user | Human operator | Runtime and platform internals | Agent author / local operator connecting external tools |
| Core strength | Operational visibility and control | Secure execution, approvals, audit, multi-surface runtime | Structured external tool access for agents |
| Human interaction model | GUI + channels + approvals + operator actions | First-class approval and audit flows | Paused execution + resume when credentials or interaction are needed |
| External API story | Present but not the core product shape | Sidecar/integration oriented, safety-first | Central product concept |
| Best fit for carsinOS | Already core | Already core | Potential enhancer for agentic tool workflows |

## Current Architectural Reality

### carsinOS / Mission Control

What exists now:

- `Mission Control` is a thin client over the carsinOS gateway: `apps/mission-control/API_CONTRACT.md`.
- carsinOS already owns the harder runtime concerns:
  sessions, runs, approvals, jobs, channels, extensions, runtime config, trust lock, security audit, assistant worker operations, provider auth controls, and secret lifecycle management: `crates/carsinos-gateway/src/main.rs`.
- carsinOS already has local tool execution, sandbox policy, allowlisted binaries, filesystem/network rules, and approval gating: `crates/carsinos-tools/src/lib.rs`, `PLAN.md`, `README.md`.
- carsinOS already supports human-in-the-loop approvals and interactive resolution from operator surfaces and channels: `PLAN.md`, `crates/carsinos-gateway/src/main.rs`.
- carsinOS already has operator-centric product surfaces that `executor` does not try to address:
  boards, calendar, focus, events, mail, rooms, team, cockpit, strategy: `apps/mission-control/src/app/tabs.ts`, `apps/mission-control/src/app/AppContent.tsx`.

What that means:

- carsinOS already has the runtime and operator-control center.
- The question is not whether executor replaces those pieces.
- It does not.

### executor

What exists now:

- `executor` is a local daemonized control plane for agent tool use: `README.md`, `ARCHITECTURE.md`.
- It combines:
  CLI, local server, web UI, and MCP endpoint into one local product: `README.md`, `ARCHITECTURE.md`, `packages/server/src/index.ts`.
- It turns connected sources into a workspace-visible tool catalog:
  MCP, OpenAPI, and GraphQL sources are discovered, connected, materialized, and exposed as reusable tools: `README.md`, `ARCHITECTURE.md`, `packages/control-plane/src/runtime/source-discovery.ts`, `packages/control-plane/src/runtime/workspace-execution-environment.ts`.
- It runs TypeScript in a sandboxed SES subprocess and proxies tool calls through the control plane: `ARCHITECTURE.md`, `packages/runtime-ses/src/index.ts`.
- It treats interaction state as durable runtime state:
  execution can pause, create an interaction record, and later resume: `ARCHITECTURE.md`, `packages/control-plane/src/runtime/live-execution.ts`, `packages/executor-mcp/src/index.ts`.
- Its UI is source-centric, not operations-centric:
  sources list, add source, source detail, edit source, secrets: `apps/web/src/views/home.tsx`, `apps/web/src/views/source-detail.tsx`, `README.md`.

What that means:

- executor is best thought of as a connector hub and structured tool layer for agents.
- It is much closer to a local "agent tools operating environment" than to a mission-control platform.

## What executor Adds To carsinOS Agentic Workflow

This is the most important section.

### 1. Connect Once, Reuse Many Times

executor's biggest practical idea is:

- external systems are connected once
- then reused as stable tools later

For carsinOS, that matters because it reduces repeated setup and repeated prompt baggage.

Instead of every assistant run needing to "know" the shape of an MCP server or a REST API from scratch, the runtime can work against a reusable tool surface.

Why this matters for agentic workflow:

- less prompt waste
- less repeated tool explanation
- less manual connector setup
- better reuse across runs and agents

Relevant repo evidence:

- `README.md`
- `ARCHITECTURE.md`
- `packages/control-plane/src/runtime/source-discovery.ts`
- `packages/control-plane/src/runtime/workspace-execution-environment.ts`

### 2. Discover / Describe / Call Is Better Than Guess / Fail / Retry

executor expects the agent to:

1. discover tools by intent
2. inspect schemas
3. call typed tools

That is a strong workflow for agent reliability.

For carsinOS, this could improve agentic workflow by reducing:

- malformed API calls
- raw guessing
- brittle hand-built payloads
- context-heavy tool instructions

Why this matters:

- the assistant becomes more reliable when using external systems
- tool usage becomes more inspectable and repeatable
- the runtime can become more schema-aware instead of stringly-typed

Relevant repo evidence:

- `README.md`
- `packages/codemode-core/src/system-tools.ts`
- `packages/codemode-core/src/discovery.ts`

### 3. Secrets and OAuth Stay Out of Prompt Space

executor keeps auth and credential capture in the runtime and UI rather than inside agent prompts.

For carsinOS, this is valuable because agentic workflows get messy and fragile when secrets, OAuth setup, and credential capture are mixed into normal run context.

Why this matters:

- cleaner agent context
- better operator trust
- fewer ad-hoc auth hacks
- better long-term connector reliability

Relevant repo evidence:

- `README.md`
- `packages/control-plane/src/runtime/source-auth-service.ts`
- `packages/control-plane/src/runtime/secret-material-providers.ts`
- `packages/control-plane/src/persistence/schema.ts`

### 4. Durable Pause / Resume For Tool Interactions

executor does something very useful:

- if a tool call or auth flow needs user interaction, execution pauses
- the interaction is persisted
- the run resumes cleanly later

This is highly relevant to carsinOS.

carsinOS already has approval flows, but executor shows a more connector-centric version of the same pattern for:

- source connection
- OAuth
- structured elicitation
- tool-specific interaction

Why this matters for agentic workflow:

- the assistant does not need to "fake" waiting
- auth/setup interruptions become durable instead of brittle
- interaction can be made part of the workflow instead of an exception path

Relevant repo evidence:

- `ARCHITECTURE.md`
- `packages/control-plane/src/runtime/live-execution.ts`
- `packages/executor-mcp/src/index.ts`
- `packages/control-plane/src/persistence/schema.ts`

### 5. External APIs Become A Stable Workspace Tool Surface

executor's strongest workflow value is that multiple protocols are made to look like one coherent tool catalog.

That means:

- MCP
- OpenAPI
- GraphQL

all become part of one local agent-facing surface.

For carsinOS, this is potentially the biggest strategic value.

It could give the assistant a cleaner way to work with external systems without turning carsinOS itself into a giant connector-definition product.

Relevant repo evidence:

- `README.md`
- `ARCHITECTURE.md`
- `packages/control-plane/src/runtime/workspace-execution-environment.ts`
- `packages/codemode-openapi/*`
- `packages/codemode-mcp/*`

## What executor Does Not Add, Or What Is Mostly Redundant

### 1. It does not replace Mission Control

executor does not provide:

- board operations
- operator focus views
- calendar/job control
- team/agent operations
- cockpit-style monitoring
- strategy/task management
- channel runtime operations

Its UI is much narrower and mostly source-centric.

### 2. It does not replace carsinOS runtime governance

carsinOS already has:

- operator approvals
- channel-resolved approvals
- runtime config
- trust lock
- role-scoped sensitive endpoints
- tool sandbox policy
- security audit
- secret lifecycle drills and controls

executor has policy infrastructure, but its center of gravity is not comparable operator governance depth.

Relevant repo evidence:

- `packages/control-plane/src/runtime/invocation-policy-engine.ts`
- `packages/control-plane/src/persistence/schema.ts`
- compare with `crates/carsinos-tools/src/lib.rs`, `README.md`, `crates/carsinos-gateway/src/main.rs`

### 3. It does not solve the operator-product problem

executor improves tool use for agents.

It does not solve:

- how operators understand the system
- how operators manage missions
- how work is tracked across boards/tasks
- how incidents and approvals are surfaced across Mission Control

That remains carsinOS territory.

### 4. Its TypeScript execution runtime is not automatically the right runtime for carsinOS

executor assumes:

- TypeScript execution
- SES runtime
- local daemon model

That may be useful as a sidecar or concept source, but it is not a reason to replatform carsinOS runtime around it.

## Strategic Thesis

The best way to think about executor is:

- not as "another Mission Control"
- not as "a carsinOS replacement"
- but as "a connector and tool-catalog operating layer for agents"

So the right strategic question is:

Should carsinOS gain a better agent-facing connector hub / tool catalog model?

My answer is yes.

Should carsinOS become executor?

My answer is no.

## ROI Matrix

| Idea | ROI | Recommendation | Why |
| --- | --- | --- | --- |
| Connected source registry for MCP/OpenAPI/GraphQL | High | Absorb or prototype | Strong direct value to agentic workflow |
| Discover / describe / typed tool-call workflow | High | Absorb conceptually | Improves external tool reliability and reduces guessing |
| Durable connector auth / OAuth flows | High | Absorb or prototype | Strong quality-of-life and reliability gain |
| Durable interaction state for tool-side elicitation | High | Absorb conceptually | Fits carsinOS human-in-the-loop model well |
| executor as sidecar connector hub | Medium-High | Prototype | Could add value without replatforming carsinOS |
| Source inspection UI ideas | Medium | Borrow selectively | Useful, but secondary to runtime model |
| Policy model for tool-path/namespace/source approvals | Medium | Borrow ideas only | Helpful but carsinOS already has stronger safety posture |
| organizations/workspaces/account model | Low-Medium | Mostly ignore | Overhead for current carsinOS posture |
| executor TypeScript runtime as primary assistant runtime | Low | Avoid | Wrong center of gravity for carsinOS |
| Full executor absorption | Low | Avoid | Stack mismatch, product mismatch, overlap in the wrong places |

## Section-by-Section Recommendations

### 1. Connector Hub / Source Registry

What executor does well:

- external sources are first-class
- discovery is built in
- connection/auth is part of the runtime, not an afterthought
- source metadata and tool artifacts persist over time

What carsinOS currently has:

- core tools
- extensions
- plugins
- sidecar integration patterns
- assistant tool facade surfaces

Gap:

- carsinOS does not yet appear to have as clean a "connect once, get reusable typed external tools" model.

What this would do for us:

- make the assistant stronger on external systems
- reduce connector sprawl
- make agent workflows more repeatable

Pragmatic view:

- high-value area to learn from
- likely the single strongest lesson from this repo

Scaling view:

- the more external systems your assistant touches, the more valuable this becomes

Recommendation:

- High ROI.
- Either absorb concepts natively or prototype executor as a sidecar connector hub.

Roadmap fit:

- Phase 1 or early Phase 2

### 2. Discover / Describe / Call Tool Workflow

What executor does well:

- it normalizes how agents use tools
- discovery and schema inspection are part of the workflow, not optional niceties

What this would do for carsinOS:

- reduce bad calls
- reduce brittle glue prompts
- make external tool use more self-explanatory

Pragmatic view:

- carsinOS should strongly consider adopting this pattern even if executor itself is never integrated

Scaling view:

- becomes more valuable as tool count grows and tool shapes get more complex

Recommendation:

- High ROI.
- Absorb conceptually regardless of whether executor is adopted directly.

Roadmap fit:

- Phase 1

### 3. Durable Auth and Credential Flows

What executor does well:

- source auth sessions are durable
- OAuth and credential capture are tied to source connection and execution interactions

What this would do for carsinOS:

- make connector setup less fragile
- reduce auth clutter in agent context
- improve recoverability of interrupted auth flows

Pragmatic view:

- valuable if carsinOS is going to connect to many external APIs or MCP servers through the assistant

Scaling view:

- increasingly important as connector count grows

Recommendation:

- High ROI.
- Absorb or prototype.

Roadmap fit:

- Phase 1 or 2

### 4. Tool-Side Interaction State

What executor does well:

- interaction records are durable runtime objects
- execution waits properly instead of just erroring or requiring a fresh run

What this would do for carsinOS:

- extend the current approval model into broader human/tool interaction
- support more than just approve/deny patterns
- enable richer connector onboarding and tool prompts

Pragmatic view:

- this fits naturally with carsinOS's existing human-in-the-loop philosophy

Scaling view:

- important if agent workflows increasingly involve external auth and structured human input

Recommendation:

- High ROI.
- Absorb conceptually.

Roadmap fit:

- Phase 1 or 2

### 5. executor As A Sidecar

What this would mean:

- carsinOS stays the orchestrator and operator platform
- executor sits beside it as a connector/tool runtime
- carsinOS calls into executor for some external-tool workflows

Why this is attractive:

- lower commitment than absorption
- faster validation
- isolates risk

Why this is risky:

- introduces another runtime
- can split responsibility if boundaries are vague
- can become a dependency before the product model is clear

Best-case use:

- narrow prototype
- selected external sources only
- strict boundary: executor handles source/tool cataloging, carsinOS handles orchestration, approvals, operator experience, and audit

Recommendation:

- Medium-High ROI as a prototype.
- Only if the boundary is kept tight.

Roadmap fit:

- Phase 2 prototype

### 6. What Not To Copy

Do not copy these blindly:

- executor's whole product shape
- executor's organizations/workspaces model as-is
- executor's TypeScript runtime as the new center of carsinOS
- source-centric UI as a replacement for Mission Control

Why:

- wrong product center
- wrong user center
- too much overlap in the wrong layer

## Integration vs Absorption

### Integration

A targeted integration is plausible and maybe worth a prototype.

The cleanest model would be:

- carsinOS = orchestrator + operator plane
- executor = tool connector/control plane for external sources

That could happen through a sidecar pattern using executor's local API or MCP bridge.

But it should stay narrow.

If carsinOS starts delegating too much of its core execution model to executor, the architecture gets muddy fast.

### Absorption

Full absorption is not worth it.

Reasons:

- stack mismatch
- product mismatch
- operator-surface mismatch
- carsinOS already owns the primary runtime/control-plane problem

The value is in concepts and perhaps a narrow sidecar prototype, not full codebase absorption.

## Roadmap

### Phase 0

- Keep carsinOS as primary orchestrator and operator platform.
- Treat executor as a tool-layer repo, not an ops-console repo.
- Decide whether you want:
  1. concept borrowing only
  2. sidecar prototype

### Phase 1

- Adopt discover / describe / typed-call ideas for external tools.
- Improve connector/auth interaction handling in carsinOS.
- Define a cleaner concept of reusable external tool sources or connector hubs.

### Phase 2

- Prototype executor as a sidecar for selected MCP/OpenAPI/GraphQL workflows.
- Validate whether it actually improves assistant outcomes and operator trust.
- Keep boundary explicit:
  source connection and tool catalog there, orchestration and oversight here.

### Phase 3

- If prototype proves strong, decide whether to:
  - keep executor as an external companion runtime
  - or absorb only the concepts into a native carsinOS connector layer

## Pragmatic View

From a pragmatic product perspective, executor is interesting because it solves a real pain point:

- agents are bad at raw external tool use when every run starts from scratch

executor tries to fix that by making the tool environment:

- persistent
- typed
- discoverable
- resumable

That is very relevant to carsinOS.

The wrong pragmatic move would be pretending that this also solves carsinOS's operator experience, mission planning, or runtime governance.

It does not.

## Scaling View

As carsinOS grows, the value of a connector/control-plane layer goes up if:

- the assistant touches more external systems
- more tools require auth, OAuth, or structured setup
- tool counts grow
- the agent needs better schema awareness

So executor-like ideas scale well in the agentic workflow layer.

What does not scale well is letting another runtime quietly become the hidden center of the product.

carsinOS should scale as:

- the main orchestrator
- the safety and operator system
- optionally backed by a cleaner connector layer

not as:

- a thin shell around executor

## Final Stance

`executor` is worth studying.

It is more relevant to carsinOS than a generic workflow SaaS repo because it directly improves the agent-tool layer.

The strongest value it adds is:

- reusable source connections
- typed tool catalogs
- discover / describe / call workflow
- durable connector auth
- durable pause/resume interaction handling

My recommendation is:

1. do not absorb executor wholesale
2. do not confuse it for a Mission Control replacement
3. strongly consider borrowing its connector and tool-catalog ideas
4. if you want to test it directly, do a narrow sidecar prototype

## Reference Notes

Key `executor` references used:

- `vendor/executor/README.md`
- `vendor/executor/ARCHITECTURE.md`
- `vendor/executor/package.json`
- `vendor/executor/apps/executor/src/cli/main.ts`
- `vendor/executor/apps/web/src/views/home.tsx`
- `vendor/executor/apps/web/src/views/source-detail.tsx`
- `vendor/executor/packages/server/src/index.ts`
- `vendor/executor/packages/runtime-ses/src/index.ts`
- `vendor/executor/packages/control-plane/src/runtime/workspace-execution-environment.ts`
- `vendor/executor/packages/control-plane/src/runtime/live-execution.ts`
- `vendor/executor/packages/control-plane/src/runtime/source-discovery.ts`
- `vendor/executor/packages/control-plane/src/runtime/source-auth-service.ts`
- `vendor/executor/packages/control-plane/src/runtime/invocation-policy-engine.ts`
- `vendor/executor/packages/control-plane/src/runtime/executor-tools.ts`
- `vendor/executor/packages/control-plane/src/runtime/secret-material-providers.ts`
- `vendor/executor/packages/control-plane/src/persistence/schema.ts`
- `vendor/executor/packages/executor-mcp/src/index.ts`
- `vendor/executor/packages/codemode-core/src/discovery.ts`
- `vendor/executor/packages/codemode-core/src/system-tools.ts`

Key `carsinOS` references used:

- `README.md`
- `PLAN.md`
- `apps/mission-control/API_CONTRACT.md`
- `apps/mission-control/src/app/tabs.ts`
- `apps/mission-control/src/app/AppContent.tsx`
- `crates/carsinos-tools/src/lib.rs`
- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-protocol/src/lib.rs`

## Follow-Up: executor As A Shared Internal Connector Hub

This is the strongest concrete use case for `executor` inside carsinOS.

The basic idea is:

- connect external systems once
- materialize them into one shared internal tool layer
- let many agents use the same internal tool surface
- avoid duplicating connector setup, auth setup, and tool definitions per agent

In other words, `executor` would not be the boss.

It would be the internal switchboard.

### Why This Use Case Is Attractive

Without a shared connector hub, multi-agent systems tend to drift into one of these bad patterns:

- every agent has its own connector setup
- every agent carries its own understanding of the same external API
- every workflow repeats auth and tool setup logic
- different agents use the same system in inconsistent ways

That creates waste, inconsistency, and fragile behavior.

`executor` offers a cleaner alternative:

- one source connection
- one reusable catalog
- one consistent way to discover and call tools
- one place to manage auth and interaction

For carsinOS, that could mean all agents speak against the same internal tool language.

### What This Would Mean Inside carsinOS

The assistant and internal agents would not each need their own separate connector universe.

Instead:

- external systems would be connected once
- their usable operations would be exposed as shared internal tools
- agents would discover and call those tools through the same stable layer
- auth and setup would live with the connector runtime instead of living inside each run

That would make agent collaboration cleaner because all agents would be working from the same tool map.

### What Value This Adds To Agentic Workflow

This shared-hub model adds several important kinds of value.

#### 1. Consistency

If one external system is connected once and exposed the same way to everyone, then all agents use it the same way.

That means less drift and less "Agent A knows GitHub one way, Agent B knows it another way."

#### 2. Lower Repetition

You stop repeating:

- connector setup
- auth setup
- schema explanation
- tool naming logic

That reduces operational clutter for both humans and agents.

#### 3. Better Reliability

When tool access is shared and structured, agents spend less time guessing and less time failing on malformed calls.

#### 4. Easier Maintenance

If a tool source changes, you update the shared connection layer once instead of fixing multiple agent-specific implementations.

#### 5. Better Collaboration Between Agents

When multiple agents use the same internal tool layer, handoffs become easier because they are working against the same system vocabulary.

### Best-Case Architectural Role

The cleanest role for `executor` would be:

- `executor` manages source connection, tool catalog materialization, and source-auth interaction
- `carsinOS` manages orchestration, approvals, operator visibility, auditing, safety, and workflow control

That split matters.

`executor` should know:

- what external systems exist
- how to connect to them
- how to expose them as tools

`carsinOS` should know:

- which agent can use which tool
- what requires approval
- what gets logged
- what is allowed in a given workflow
- how work is coordinated across the system

### The Most Important Boundary

The shared connector hub should centralize tool access.

It should not centralize product authority.

That means:

- yes to shared tools
- yes to shared connector state
- yes to shared auth handling
- no to handing orchestration ownership away from carsinOS
- no to making Mission Control depend on executor for basic product identity

This boundary is the difference between:

- "executor improves the agent layer"

and

- "carsinOS quietly becomes a wrapper around executor"

The first is useful.

The second is a mistake.

### The Right Access Model

Even if the tool catalog is shared, access should not be universal.

The right shape is:

- one common internal connector layer
- filtered or scoped views of that layer per agent, role, or workflow
- carsinOS still enforcing approval, safety, and audit policy on top

So the system becomes:

- shared catalog underneath
- controlled access above it

not:

- one giant unrestricted bag of tools for everyone

### What This Does Not Solve

Even in the best case, this shared-hub model does not solve:

- operator experience
- mission tracking
- boards and workflow visibility
- calendar and scheduling surfaces
- incident management
- safety governance by itself

Those remain carsinOS concerns.

So this is an agent-layer improvement, not a full product-layer solution.

### Risks

This model is strong, but there are real risks.

#### 1. Boundary blur

If executor starts owning too much of execution logic, architecture gets muddy quickly.

#### 2. Hidden dependency

If many critical workflows depend on executor before the boundary is proven, carsinOS can become more fragile instead of less.

#### 3. Overexposure

If all agents can see all tools by default, shared infrastructure becomes shared risk.

#### 4. Product drift

If connector management becomes the center of the product, carsinOS loses focus.

### Recommended Stance For This Specific Use Case

This is the strongest direct recommendation I would make from the repo:

If you want to use `executor` at all, use it as a shared internal connector hub for agents.

Do not use it as:

- a Mission Control replacement
- a runtime replacement
- a broader product shell

Use it only if the goal is:

- one connected tool layer
- many agents sharing that layer
- carsinOS staying in charge of orchestration and safety

### Practical Recommendation

The smartest path would be:

1. prototype a narrow sidecar integration
2. connect a small number of high-value external systems
3. expose them as shared internal tools
4. let carsinOS decide which agents can see and use them
5. keep approvals, policy, and audit in carsinOS

That would let you test the real value of the shared connector-hub model without committing the architecture too early.

### Final Take

Yes, `executor` could plausibly be the connecting point to external systems so all agents share the same internal tool layer instead of each carrying separate tool setups.

That is probably the single best reason to care about it.

But the correct shape is:

- shared connector hub underneath
- carsinOS orchestration and safety above it

If that boundary is respected, this could be a genuinely useful addition to the carsinOS agentic workflow.
