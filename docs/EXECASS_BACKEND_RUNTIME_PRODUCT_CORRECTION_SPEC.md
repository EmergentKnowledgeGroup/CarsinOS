# ExecAss Backend and Runtime Product Correction Specification

- Status: LOCKED
- Version: 1.1
- Lock date: 2026-07-20
- Owner: CarsinOS product owner
- Implementation branch: `codex/execass-backend-runtime-correction`
- Source briefs:
  - `docs/EXECASS_BACKEND_PRODUCT_BEHAVIOR_BRIEF.md`
  - `docs/EXECASS_FRONTEND_EXPERIENCE_BRIEF.md`

## 1. Outcome

CarsinOS remains mission control internally and becomes an executive assistant externally. The user gives ExecAss an outcome; ExecAss owns durable coordination, continuation, bounded recovery, and evidence-backed reporting across existing CarsinOS systems.

This is a backend and runtime correction, not a cosmetic frontend project. It is complete only when one local user can delegate ordinary work without knowing whether CarsinOS must use sessions, runs, jobs, schedules, boards, Agent Mail, teams, tools, connectors, memory, recovery, or another internal surface.

The product invariant is:

> The user provides intent. ExecAss owns coordination. CarsinOS preserves control, truth, and evidence underneath.

The owner-authority invariant is:

> CarsinOS does not decide what its one owner may ask their ExecAss to do. An authenticated owner instruction is authority for its exact requested or expressly delegated action envelope. CarsinOS confirms dangerous execution once, preserves technical integrity, and then carries out the confirmed action.

## 2. Deployment and Trust Boundary

### 2.1 Product topology

- Exactly one human user, one ExecAss identity, and one CarsinOS instance are supported.
- Multi-user tenancy, organization isolation, role administration, and cross-user delegation are non-goals.
- All authenticated ingress paths act for the same principal but must still carry source, authentication, correlation, and idempotency evidence.
- The deployment is local-first. Windows is the first supported production runtime. macOS is second, using the same product behavior and storage contracts.

Single-user does not mean single actor. Every request records an `actor_type`, credential identity, channel assurance, and authenticated ingress. Actor types are `human_local`, `human_remote`, `runtime`, `worker`, `connector`, and `model`. Only an authenticated human-controlled ingress may create or amend owner authority, confirm a dangerous action, or resolve another human decision. Runtime, worker, connector, retrieved, model, and child-agent content is untrusted evidence and can never impersonate or mint owner consent.

### 2.2 Threat model

The system protects against accidental misuse, duplicate delivery, ordinary process crashes, partial database commits, connector retries, stale workers, prompt injection, and ordinary local data tampering. It does not claim to defeat a fully privileged local administrator who can replace binaries, inspect process memory, or rewrite both data and locally held integrity keys.

Receipt chaining and keyed integrity tags must make ordinary modification, deletion, insertion, and reordering detectable. Product copy and documentation must state the local-admin limitation honestly.

### 2.3 Clean replacement boundary

- CarsinOS has never been deployed to users and has no production data to migrate.
- The implementation may replace incompatible local schemas and API shapes directly.
- Compatibility adapters, dual-read paths, legacy fallback behavior, and historical backfill are prohibited.
- Schema versions remain mandatory so future upgrades are explicit.
- Schema replacement is distinct from factory/audit erasure. The correction uses schema replacement only.
- No factory reset, audit erasure, or general-purpose state reset is implemented by this correction. `schema_replace` is the named offline/quiescent operation that archives the old root, initializes the new incompatible schema, preserves required retry tombstones and integrity metadata, verifies it, and atomically activates it.
- Before `schema_replace`, the runtime must be offline or provably quiescent: intake blocked, login launch disabled for the operation, active claims fenced, and active or unknown external effects reconciled or preserved as blocking records.
- Tooling archives outside the schema-replacement target and records every state-root entry, size/hash, schema version, binary compatibility version, receipt anchor/key identifier, secret-reference disposition, and deduplication tombstone required to reject pre-replacement retries.
- Protected secrets are either retained by an explicit OS-bound reference or excluded with mandatory reauthentication; they are never ambiguously described as redacted-but-restorable.
- Restore occurs in isolation and verifies file hashes, database integrity, referential integrity, receipt integrity, schema/binary compatibility, and required secret references before an atomic same-volume swap. At every injected failure, either the old state remains byte-identical and active or the fully verified new state becomes active.

## 3. Scope

### 3.1 Included

- A durable Delegation aggregate and state machine.
- Deterministic intake classification between conversational answers and durable work.
- Canonical owner-instruction authority, typed decisions, and exactly-once dangerous-action confirmation behavior.
- Atomic continuation, leasing/fencing, idempotency, bounded recovery, and uncertain-outcome handling.
- Evidence-backed completion, receipt integrity, summaries, reminders, and notification cursors.
- A single authenticated API/event contract usable by every ingress and the future frontend.
- A Windows current-user background runtime host and start-at-login behavior.
- A macOS per-user runtime host with equivalent lifecycle behavior.
- Configuration, diagnostics, schema-replacement/archive tooling, test fixtures, reference clients, and a frontend handoff harness.
- Documentation and release evidence necessary to prove the backend/runtime contract.

### 3.2 Excluded

- Production frontend redesign or broad React visual changes.
- Multi-user or server tenancy.
- Linux packaging unless separately requested later.
- Cloud control-plane requirements.
- CarsinOS-authored morality, content, commercial-purpose, or action-category vetoes over an authenticated owner's explicit instructions.
- Payment, purchasing, payee, currency, balance, monetary allowance, or financial-commitment functionality or internal financial reservation plumbing. Owner-directed use of an existing external tool is a generic external effect, not a CarsinOS financial subsystem.
- Absolute refusal or repeated confirmation of an owner-confirmed destructive or dangerous action.
- A second task, job, agent, scheduler, or receipt engine that duplicates authoritative CarsinOS subsystems.

The existing production frontend may receive the smallest nonvisual client types, API adapters/controllers, decision and stop wiring, and diagnostic surfaces required to validate the exact packaged backend lifecycle. Broad visual, navigation, and information-architecture redesign remains excluded. A user-facing release must not ship with a frontend that is knowingly incompatible with the new contract.

### 3.3 Existing-system ownership and replacement matrix

| Concern | Locked disposition |
| --- | --- |
| Assistant Desk summary | Replaced by the ExecAss summary projection; no second product truth remains. |
| Decisions | Typed decision records replace legacy approvals for clarification, dangerous-action confirmation, owner-configured checkpoints, recovery choices, duplicate-risk retry, stop, and policy change. They are not a second permission layer over an already exact owner instruction. |
| Jobs and scheduler | Reused as the durable scheduler/execution substrate. Continuations and routine occurrences do not create a second scheduler. |
| Sessions and runs | Reused as authoritative execution records linked beneath Delegations. |
| Tasks, boards, Agent Mail, teams, artifacts | Reused and referenced; Delegation coordinates them but does not duplicate them. |
| Security audit, tool-call audit, job/run evidence | Remain authoritative ledgers; receipts link and attest to them instead of copying or replacing them. |
| Gateway | Evolves into the one `carsinos-runtime-host`; no wrapper or sibling host is added. |
| WebSocket transport | Reused, backed by the new durable outbox sequence rather than in-memory event identity. |
| Telegram and Discord | Routed through shared intake/decision services and tested. Other channel crates remain unclaimed until activated and tested. |
| Legacy GUI/one-click launchers | Converted to attach/control or fenced as development-only so they cannot mutate production state independently. |

Delegation owns intent, amendments, outcome criteria, coordination phase, authority snapshot, and user-facing projection. Existing executors own their actual effects. Adapters ingest authoritative state monotonically, reject stale revisions, surface conflicts, and reconcile rather than allowing child/run success to terminalize a delegation directly.

## 4. Core Domain Model

### 4.1 Delegation is the primary aggregate

`Delegation` is a durable coordinating record, not a replacement executor. It references and summarizes authoritative sessions, runs, jobs, schedules, tasks, boards, decision records, messages, artifacts, receipts, and recovery attempts.

Every delegation has at minimum:

- stable `delegation_id`;
- normalized original intent and immutable intake evidence;
- ingress source and source message correlation;
- lifecycle state and monotonic state revision;
- current plan summary and user-facing outcome criteria;
- effective authority snapshot plus policy version;
- metered technical resource reservations and consumption;
- links to authoritative internal work records;
- pending decision or external wait, if any;
- stop/recovery state;
- completion assessment;
- ordered receipt chain head;
- created, updated, acknowledged, and terminal timestamps.

### 4.2 Conversational versus durable intake

The classification is deterministic and auditable.

A request may remain conversational when it can be answered immediately without tools, external side effects, delayed continuation, scheduling, delegation to another worker, creation or mutation of durable CarsinOS state, a human decision, or a claim requiring a durable receipt. A synchronous read-only status/evidence query may also remain non-delegated, but it retains authenticated request-audit evidence and cannot cause a state change.

A request must create or attach to a durable delegation before it invokes any tool, connector, subprocess, multi-step plan, future wakeup, scheduled or recurring action, specialist/child work, human decision, recovery, external wait, artifact production, or evidence-backed completion. The dispatch gateway rejects side-effecting work that lacks a persisted delegation, plan revision, policy revision, canonical leaf-action manifest, fenced claim, technical resource reservation when applicable, and stable idempotency key. Ambiguity resolves toward a durable delegation. The classifier records its reasons and version.

Duplicate authenticated ingress with the same source identity and idempotency key must return the same result. A follow-up attaches only through an explicit delegation ID or an unambiguous reply-to correlation to exactly one active delegation. Material revision appends an immutable amendment, creates new criteria/plan revisions, reruns authority/resource checks, and supersedes pending decisions and continuations. It never mutates the original intent or leaves a decision bound to old operands executable.

### 4.3 Lifecycle, control, and attention

The exclusive delegation `phase` values are:

- `accepted`: durably admitted but not yet planned;
- `planning`: translating intent into bounded work;
- `in_motion`: at least one authorized action is runnable or executing;
- `waiting_for_user`: a specific decision, clarification, or authority boundary can improve the outcome;
- `waiting_external`: no user decision is currently useful and progress depends on time or an external party/system;
- `recovering`: bounded automated recovery is underway;
- `completed`: the requested outcome is satisfied with evidence;
- `partially_completed`: useful terminal outcome was achieved but some requested portion could not be completed and no autonomous path remains;
- `failed`: no acceptable outcome was achieved and intervention or a new delegation is required.

The aggregate phase is recomputed transactionally from authoritative branch and attention state using this precedence:

1. A valid completion assessment selects `completed`, `partially_completed`, or `failed`.
2. Before actionable planning exists, use `accepted` or `planning`.
3. If any authorized ordinary branch is runnable or executing, use `in_motion`, even when another branch contributes Needs You or an external wait.
4. Otherwise, if bounded automated recovery is runnable or executing, use `recovering`.
5. Otherwise, if at least one actionable human attention item exists, use `waiting_for_user`.
6. Otherwise, if progress depends on an external party, system, or time, use `waiting_external`.
7. If no autonomous, human, or external path remains, the completion assessor must select the honest terminal phase.

`run_control=stopped` is an orthogonal user-visible override. It freezes nonterminal execution until resume, remains visible in summary/detail projections, and does not erase the underlying phase. The phase inputs and selected result are receipt-backed at the same revision.

Orthogonal `run_control` values are `running`, `stop_requested`, and `stopped`. Orthogonal action/branch records may be runnable, executing, waiting, uncertain, or terminal. A delegation may therefore appear in both Needs You and In Motion when one branch needs a decision while another remains safely runnable. `stop_requested` is draining, not stopped. User-facing `stopped` is exposed only after new claims are blocked, active claims are fenced, and permitted safe-boundary work has ended or been recorded as unresolved external effects.

State transitions use compare-and-swap against the current revision. Terminal phases cannot transition except through an explicit new delegation or a linked correction/recovery record that prominently warns of late contrary evidence without rewriting history.

`Needs You` is a summary projection, not a phase synonym. It contains typed actionable attention items. `partially_completed` belongs in Done with an honest qualification unless a live user choice can improve it.

### 4.4 Stop and resume

- Stop prevents new actions and cancels or safely winds down interruptible active actions.
- An already executing irreversible external action may reach its next safe completion boundary; that fact must be reported.
- Stop never deletes the delegation, authority history, or receipts.
- Resume continues the same delegation from a freshly validated plan/policy snapshot.
- A policy reduction takes effect before the next action begins. Existing actions may finish only to their declared safe boundary.
- Stop, policy change, schema replacement, decision resolution, lease expiry, and action completion races must be serialized by revisions, fencing tokens, and transactional state changes.

The preserved emergency stop-all uses an atomic global stop epoch. It blocks all new claims and routine admission, fences active claims, drains only declared safe boundaries, records unresolved external effects, and requires an explicit human resume. No worker, connector, model, or child process may clear it.

## 5. Owner Authority and Human Decisions

### 5.1 Authority precedence and operational profiles

An authenticated owner instruction is authority for the exact requested action and every choice the owner expressly delegates. This applies regardless of whether the action communicates externally, uses a secret, changes permissions, changes project state, deletes data, or invokes an existing external tool. CarsinOS does not create moral, commercial, financial, or action-category vetoes.

Authority is resolved in this order:

1. Current stop/revocation state, superseding owner amendments, exact action identity, and technical execution validity are applied.
2. A current exact authenticated owner instruction or confirmed amendment authorizes its resolved action envelope.
3. A saved owner instruction may authorize recurring or unattended work within its exact versioned envelope.
4. Profiles govern only operational behavior for derived or unattended work; they cannot nullify a current exact owner instruction.
5. Non-human content may inform planning but cannot create, expand, confirm, or revoke owner authority.

All autonomy profiles compile into one canonical operational-policy representation:

- `locked_down`: infer less, keep derived scopes narrow, and use the one-confirmation path more readily for model-identified danger;
- `balanced`: recommended presentation; infer ordinary details and ask one combined clarification/confirmation when ambiguity or danger is material;
- `full_send`: maximize work inside the owner's envelope and reserve optional confirmation for credible destructive or dangerous consequences;
- `custom`: owner-selected operational settings subject to the same one-confirmation and technical-integrity rules.

First run requires guided profile selection for unattended or standing behavior. Balanced may be highlighted but is never silently stored. An unconfigured first run still accepts and executes an authenticated exact owner request under the base technical and dangerous-action rules.

An exact authenticated owner amendment to an operational profile or policy is itself owner authority and does not require a second permission decision. It proceeds through the canonical owner-intake and revision transaction. CarsinOS asks only when clarification is unresolved or when the amendment contains a separately resolved dangerous action that qualifies for the one confirmation in Section 5.2. No policy-specific handler may create a parallel approval or authority path.

Operational dimensions include task/delegation, workspace/path, routine, connector/tool identity and version, target, audience, technical resource quota, time/expiry, recovery, parallelism, clarification sensitivity, and recurring-work scope. There is no CarsinOS money, payee, currency, balance, purchase, or financial-commitment dimension.

`actor_type` is derived server-side from authenticated ingress evidence; request bodies, headers, model text, connector payloads, and claimed actor IDs cannot select or promote it. Technical execution validity means only objective capability availability, canonical operand resolution, platform/runtime preconditions, transactional/fencing validity, idempotency/reconciliation support, and technical resource availability. It may not encode purpose, morality, commercial judgment, action category, model risk score, or another disguised owner-intent veto.

Base owner ingress establishes who supplied a new instruction or amendment; it does not require a decision that does not yet exist:

| Verified base ingress evidence | Actor type | Submit/amend owner intent |
| --- | --- | --- |
| Interactive local owner session bound to the authenticated client and request correlation | `human_local` | Yes |
| Allowlisted Telegram/Discord owner message bound by the adapter to provider account, source message, and request correlation | `human_remote` | Yes |
| Bearer, service, automation, or channel-adapter credential without independently verified interactive owner evidence | `runtime` or `connector` | Only within an existing owner grant |
| Worker, model, retrieved content, tool output, or child-agent content | Matching non-human actor type | Evidence/work only |

Resolving a human decision requires the base owner evidence plus binding to the current decision; those requirements are not prerequisites for new intake:

| Verified resolution evidence | Resolve ordinary decision | Confirm dangerous action |
| --- | --- | --- |
| Local owner action bound to the authenticated client, current decision revision, exact presented action/alternative, and unexpired one-time challenge nonce | Yes | Yes |
| Allowlisted Telegram/Discord owner action bound to provider account, source message, current decision revision, exact presented action/alternative, and unexpired one-time challenge token | Yes | Yes |
| Runtime, connector, worker, model, retrieved content, tool output, or child-agent content | No | No |

A service credential plus caller-supplied peer/operator ID never constitutes owner evidence. Replay, substitution, or mismatch performs zero decision transition and zero effect.

Every dispatch compiles to a canonical resolved leaf-action manifest and freezes its exact operands or target-set digest. Explicitly broad owner instructions are valid when they can be resolved to a deterministic snapshot. Unknown, composite, aliased, plugin, shell, or changed-version actions pause only until their leaves and operands are mechanically resolved or one clarification is answered; those labels are not permanent denial categories.

### 5.2 Exactly one dangerous-action confirmation

ExecAss never permanently refuses an authenticated owner action merely because it is destructive or dangerous. Before executing a dangerous action it asks exactly once, states the concrete result if performed, and offers `confirm_and_continue`, `revise`, or `decline` for the presented action. A verified owner confirmation makes the exact action runnable. There is no additional local-only challenge, second confirmation, or repeated pushback.

The deterministic danger matcher must require this one confirmation for resolved operations that would:

1. erase, overwrite, format, or make unusable an entire drive, volume, boot/recovery environment, or core operating-system tree such as Windows `System32`;
2. erase or make unusable an entire OS user profile/home;
3. erase, corrupt, or disable the complete CarsinOS state, receipt/integrity evidence, runtime enforcement, stop/fencing controls, or configuration required to recover them;
4. erase or close an entire connected external account or tenant; or
5. destroy the last verified administrative, recovery, or decryption path for otherwise unrecoverable owner data.

The matcher uses canonical resolved operands and verified system metadata, never item count, free-text wording, model risk scores, or a generic “consequential” category. Symlink, junction, alias, case, mount, and target-set resolution occur before matching and again at dispatch.

The model may identify another action as plausibly destructive or dangerous and route it through the same one-confirmation decision. Model judgment may add one confirmation; it may not create an absolute veto, require a second confirmation, or override an existing confirmation for the unchanged action.

The confirmation challenge is short-lived and single-resolution. It binds the decision revision, exact presented action or enumerated alternative, declared consequence, nonce/token, and challenge expiry. An unanswered expired challenge may be reissued for the same action; it is not an accepted confirmation and causes no effect.

Once the owner confirms, the transaction consumes the challenge exactly once and creates a durable accepted-confirmation grant bound to the delegation, normalized intent, confirmed logical-action identity, canonical action envelope/selector, payload and material operands, connector/tool identity and version, and declared consequence. The accepted grant has no expiry or use counter. It survives unchanged replanning, policy revalidation, continuation, restart, bounded retry, and saved-routine occurrences. Claim or policy revalidation may pause/reclaim execution for objective technical reasons but may not invalidate, consume, or reprompt the grant when the action identity is unchanged.

Operand, target, scope, payload, connector/tool version, or material-consequence drift creates a different action and invalidates the old grant. Only an explicit owner amendment, revocation, or cancellation that identifies that confirmed action/envelope ends its accepted grant. `decline`, `revise`, `stop`, or another result for an unrelated duplicate-risk, recovery, checkpoint, policy, or other decision cannot consume or invalidate it. Declining an unresolved dangerous challenge affects only that presented action/alternative; no accepted grant exists from that declined challenge. Internal plan formatting, policy revision, technical resource recalculation, process/host generation, retry attempt identity, and expected target membership changes inside the same saved routine selector/envelope do not create a new action. Each routine occurrence still resolves and freezes its exact current operands before dispatch.

If an action is both ambiguous and dangerous, ExecAss presents one combined question whose alternatives state their resolved scope and concrete consequence. Selecting and affirmatively confirming a disclosed alternative records `confirm_and_continue` for that resulting manifest. A clarification or `revise` response that does not affirm the disclosed consequence cannot make still-dangerous work runnable. If the revision removes the danger, ordinary execution may continue; if it creates a materially different dangerous action or an undisclosed material consequence, that new action may receive its one combined question. The system must not ask again for the same resolved action. A saved routine version carries its accepted grant forward to unchanged occurrences and is asked again only after a material routine amendment.

### 5.3 Typed decision binding

Decision kinds are `clarification`, `dangerous_action_confirmation`, `owner_configured_checkpoint`, `recovery_choice`, `duplicate_risk_retry`, `stop`, and `policy_change`. A decision record must state why user input is required; a generic approval is not an acceptable substitute.

Every applicable decision records exactly:

- delegation, plan, action, policy, and decision revision at presentation/resolution time;
- normalized owner intent and express delegation envelope;
- canonical resolved leaf-action manifest, payload digest, and exact operands/target-set digest;
- audience, workspace, connector/tool identity and version where applicable;
- technical resource envelope and applicable challenge expiry/nonce;
- human-readable recommendation, concrete consequence, risk, and alternatives.

`confirm_and_continue`, `revise`, `decline`, and `stop` are the only first-class decision results. Clarifying information is carried by `revise`; it is not a fifth result. Recording the winning result, receipt, outbox event, and—only when that result makes a specific target branch runnable—exactly one deterministic continuation for that target occurs atomically. `stop` creates zero runnable continuations. `decline` creates zero unless an independently owner-authorized remaining branch or bounded replan becomes runnable. `revise` creates a continuation only after the resulting amendment, criteria, policy, and manifest revision is durably valid; it cannot make unresolved dangerous work runnable without the combined affirmative confirmation defined in Section 5.2. Repeated or losing decisions return the recorded result and create no continuation or effect.

Immediately before dispatch, the runtime re-resolves the leaf manifest. Material action-identity drift in operands, target, scope, audience, payload, path identity, alias expansion, connector/tool version, or consequence invalidates the accepted confirmation grant and performs zero effects until the new action receives its one confirmation when applicable. Plan, policy, technical resource, host-generation, expiry, or stop-epoch drift supersedes the current claim and performs zero effects, but does not invalidate or consume the accepted grant for an unchanged action. All such pauses are reported as technical drift or a changed action, never as CarsinOS forbidding the owner's intent.

### 5.4 Clarification policy

Clarification behavior is configurable. Balanced infers and proceeds when plausible interpretations remain inside the owner's envelope and do not materially change operands, destructive blast radius, technical resources, or outcome. It asks one concise question when those properties are unresolved. An explicitly broad instruction is not ambiguity when it can be resolved to a frozen target snapshot. The system records the alternatives considered and why it asked or proceeded.

## 6. Execution Semantics

### 6.1 One continuation, idempotent work

Every accepted state transition that makes work runnable creates exactly one durable continuation record, receipt, and outbox event in the same transaction. The continuation ID is deterministic and protected by a database uniqueness constraint over causation plus target revision. Lease expiry reclaims the same continuation; it never enqueues a replacement. Workers claim continuations with leases and monotonically increasing fencing tokens. Claim time revalidates delegation, plan, policy, global stop epoch, technical resource quota, and host fencing generation. Invalid work becomes `superseded` with a receipt and performs no side effect. A stale worker cannot commit state, technical resource consumption, receipt, completion, or an external dispatch after ownership changes.

Each logical action/effect has a stable `logical_effect_id`; each execution has a separate `attempt_id`. The internal idempotency key and provider idempotency key remain stable across all retries of one logical effect. Internal effects are idempotent. Connector effects use provider idempotency keys when supported and reconciliation keys otherwise.

### 6.2 External uncertainty

If a process crashes after an external side effect may have occurred but before durable evidence is committed, the action becomes `outcome_unknown`. This is a retry prohibition, not a retry status. It prevents automatic reinvocation, material completion, and technical resource release. Recovery must reconcile using provider state, remote identifiers, an out-of-process effect record, or a safe read. Only independent proof that the prior effect is absent permits retry of the same logical effect. Otherwise the delegation waits externally or asks the user. Resolving `decision_kind=duplicate_risk_retry` with `result=confirm_and_continue` acknowledges duplicate-effect risk and atomically creates a new `logical_effect_id` with its own stable idempotency/reconciliation identity. It does not erase or consume an accepted dangerous-action confirmation for the unchanged action; material action drift follows Section 5.2.

### 6.3 Technical resource correctness

Technical resource quota checks and reservations are atomic with action claims. Parallel workers cannot each consume the same remaining tokens, elapsed-time allowance, connector calls, or generic resource units. Reservations expire or settle through fenced transitions; all release, consume, and reconciliation paths are receipt-backed. No monetary unit, balance, currency, payee, purchase, or financial commitment is represented.

### 6.4 Parallelism

Parallelism is configurable and bounded globally and per connector/action class. The scheduler must respect dependencies, policy, technical resource quotas, connector limits, exclusive resources, stop state, and recovery circuits. The same correctness rules apply at parallelism one and greater than one.

### 6.5 Recovery

Recovery is bounded only by objective retry-safety properties: attempt count, elapsed time, backoff, technical resource quota, circuit breakers, provider error class, idempotency, independent absence/reconciliation proof, reversibility, and the operation's declared safe boundary. Purpose, content, commerce, morality, action category, free-text wording, or a model risk score may not reduce or suppress recovery. A recovery plan may retry only when the effect is proven absent or the operation is idempotent. It may replan within the original intent and authority but may not silently expand them.

Exhausted recovery yields `waiting_for_user`, `waiting_external`, `partially_completed`, or `failed` according to whether user judgment is useful and whether a meaningful outcome exists. Technical failure alone does not justify interrupting the user when an autonomous safe path remains.

### 6.6 Recurring work

A routine is a durable template that creates a distinct delegation occurrence for each scheduled execution. Each occurrence records the routine version and effective policy snapshot. Policy reductions apply before the next action in active occurrences and before admission of future ones. Pausing a routine stops new occurrences without rewriting past receipts. Catch-up behavior after downtime is explicit (`skip`, `latest_only`, or bounded `replay`) and defaults to `latest_only` in Balanced.

Routine timing uses an IANA timezone and a scheduled-instant occurrence identity. DST gaps advance to the next valid local instant; DST overlaps execute once at the earlier offset unless the routine explicitly selects the later offset. Clock rollback cannot recreate an existing occurrence ID. Bounded replay is capped at 10 occurrences and never bypasses current policy or technical resource quotas.

## 7. Receipts and Completion

### 7.1 Receipt requirements

Every material claim or transition appends a canonical receipt containing the delegation, causation lineage, action/decision reference, actor/runtime identity, timestamps, state revisions, normalized evidence references, redacted summary, previous receipt digest, and keyed integrity tag. Receipt serialization and hash algorithms are versioned and deterministic. The immutable lineage must connect intake, plan/amendment, decision, continuation, action/effect, verifier result, and terminal assessment; subordinate records must remain reachable and no decision/recovery/resume may create a replacement delegation.

Sensitive values must be redacted before persistence in receipts, logs, notifications, exports, diagnostics, backups, filenames, URLs, and error messages. Evidence may refer to protected local artifacts through capability-checked handles; it must not copy secrets into ordinary history. An exact authenticated owner request may direct a secret to a stated destination through an existing capable tool, but the raw secret may exist only for the minimum delivery boundary and must not persist in CarsinOS history, evidence, state, outbox, notifications, paths, or recovery artifacts.

Receipts are retained locally indefinitely by default. The product provides integrity verification, bounded export, archive, and explicit offline maintenance procedures without pretending a local administrator cannot defeat local controls.

The integrity key is per-user, OS-protected, and identified by versioned key ID. Rotation appends a cross-signed transition; key loss quarantines trusted-history operations and follows an explicit recovery procedure that never fabricates prior verification. A sealed receipt count/head high-water anchor lives outside the receipt database rollback domain and binds the state-root generation. Startup quarantines completion/export on count, head, key, or generation mismatch. Verification covers tail/prefix truncation, full rollback/deletion, insertion, modification, reordering, anchor rollback, archive restore, and cross-root restore.

The receipt database and external anchor use a crash-recoverable prepare/finalize protocol; they are never assumed to share one atomic commit. Restart distinguishes an interrupted legitimate commit from rollback using independently verifiable prepared state. At every database commit, anchor write, sync, rename, and finalize failpoint, recovery either finalizes the last proven pair or restores the last proven pair without accepting an unanchored receipt. Ordinary crash interruption is not reported as tampering and cannot cause permanent quarantine; irreconcilable or adversarial mismatch quarantines trusted-history operations.

### 7.2 Completion assessor

A successful tool call or run is progress evidence, not automatic completion. Each material outcome criterion is versioned and declares an independent verifier type, expected predicate, authoritative evidence source, and `pass`, `fail`, or `unknown` result. Models, workers, connectors, and receipts may supply evidence but cannot certify their own success. Completion compares the original/amended outcome criteria against these verifier results.

`completed` requires every material criterion to pass or be explicitly superseded by a bound human decision; no material `unknown` may complete. `partially_completed` requires a useful evidenced result, an exact unmet portion, and no remaining autonomous path. `failed` requires the absence of a useful requested outcome. An unresolved receipt-integrity failure blocks trusted completion/export. All terminal reports state what happened, what did not, uncertainty, and evidence/deep links. Late bounce, reversal, artifact loss, or contrary evidence creates a linked correction/recovery record and prominent warning without rewriting the original terminal receipts.

## 8. Summary, Attention, and Proactivity

### 8.1 Executive summary contract

The backend produces one authoritative projection with:

- `needs_you`: typed attention items where user action can change the result;
- `in_motion`: active, recovering, and external-wait delegations with calm status;
- `done`: evidenced completions and partial completions since the displayed cursor;
- `next`: scheduled occurrences, commitments, deadlines, and expected follow-ups;
- `receipts`: inspectable evidence references for every material claim.

Projection updates are transactionally driven from durable source state and can be rebuilt. They never invent work or treat raw event volume as progress.

`AttentionItem` variants are `confirmation`, `clarification`, `reply`, and `recovery_choice`. Decision kinds map totally and deterministically: `clarification` → `clarification`; `recovery_choice` → `recovery_choice`; and `dangerous_action_confirmation`, `owner_configured_checkpoint`, `duplicate_risk_retry`, `stop`, and `policy_change` → `confirmation`. `reply` represents an awaited external/human response that is not itself a decision kind and cannot resolve a human decision. Each decision-backed item includes its typed decision kind, reason, recommendation, alternatives/actions, assurance required, deadline/reminder state, delegation/decision revision, and authoritative deep link. Coarse Reef activity uses a privacy-safe enum, freshness timestamp, and deep links; raw telemetry or model prose cannot invent activity.

### 8.2 Cursor and acknowledgement

Every summary response includes a stable displayed cursor plus the exact delivered item-ID/revision set. Acknowledgement is an explicit idempotent request bound to both and cannot skip items not actually delivered. Multiple devices are out of scope, but reconnects and repeated requests must be safe.

### 8.3 Notifications

Notify only when user attention is useful, a deadline risk materially changes, or a user-configured completion warrants it. Deduplicate by delegation/decision/reason revision. Deadline reminders use bounded escalation, quiet hours, and recorded next-reminder time. Silence must mean nothing currently needs the user, not that background execution stopped.

## 9. API and Event Contract

All routes require the authenticated owner principal or an explicitly bounded runtime principal plus standard request correlation. Human decision resolution additionally requires verified local or authenticated remote owner evidence as defined in Section 5. Error bodies use a versioned machine code, safe human message, retryability, and correlation ID. Mutations accept an idempotency key.

Required versioned surfaces:

- `POST /api/v1/execass/intake`: answer conversationally or create/attach a delegation; returns a discriminated result.
- `GET /api/v1/execass/summary`: returns Needs You, In Motion, Done, Next, Receipts, and displayed cursor.
- `POST /api/v1/execass/summary/ack`: idempotently acknowledge one displayed cursor.
- `GET /api/v1/execass/delegations`: filterable, cursor-paginated list.
- `GET /api/v1/execass/delegations/{delegation_id}`: lifecycle, outcome, decision, references, and deep links.
- `GET /api/v1/execass/delegations/{delegation_id}/receipts`: ordered, verifiable receipt projection.
- `POST /api/v1/execass/decisions/{decision_id}/resolve`: `confirm_and_continue`, `revise` with optional clarification/amendment payload, `decline`, or `stop`.
- `POST /api/v1/execass/delegations/{delegation_id}/stop`.
- `POST /api/v1/execass/delegations/{delegation_id}/resume`.
- `GET /api/v1/execass/stop-all`: returns engaged state, current global stop epoch, drain state, and unresolved external-effect references.
- `POST /api/v1/execass/stop-all`: idempotently engages stop-all and atomically increments the global stop epoch. Authenticated humans and trusted fail-safe runtime circuits may engage it; engagement never broadens authority.
- `POST /api/v1/execass/resume-all`: requires fresh interactive human evidence bound to the exact stopped epoch, current policy revision, and disclosed unresolved effects. Runtime, worker, connector, model, and child actors can never clear stop-all.
- `GET /api/v1/execass/policy`.
- `PUT /api/v1/execass/policy`: applies an exact authenticated owner amendment through the same owner-intake/revision, actor-evidence, compare-and-swap, receipt, and outbox transaction used by the canonical decision service. It creates no second permission step; any unresolved clarification or independently dangerous action is represented by the corresponding typed decision and resolved only through `/decisions/{decision_id}/resolve`.
- `GET /api/v1/execass/runtime-host`.
- `PUT /api/v1/execass/runtime-host` for opt-in, start-at-login, and bounded runtime settings.

The authenticated native local-control channel exposes stop-all engage, status, and resume behavior. Resume requires the same fresh interactive human evidence, exact stopped epoch, current policy revision, and unresolved-effect disclosure as `POST /api/v1/execass/resume-all`; no non-human actor may clear stop-all.

Events are emitted through a transactional durable outbox and existing authenticated event transport. Event names are versioned and include aggregate ID, revision, correlation, causation, occurred time, schema version, and safe payload. Required families cover delegation transitions, decisions, continuation claims/results, recovery, completion, summary changes, policy changes, runtime-host state, receipt integrity failure, and notification scheduling.

The outbox supplies a durable global stream sequence, at-least-once replay, per-aggregate ordering, resume cursor, duplicate identity, and gap detection. A detected gap requires summary refetch. In-memory WebSocket counters are not authoritative.

`carsinos-protocol` gains schema derivation and generates a checked-in JSON Schema bundle plus an OpenAPI 3.1 contract assembled from the same DTO schemas; parity tests fail on drift. The implementation publishes example payloads, state/event/action/assurance tables, error catalog, deep-link catalog, and a runnable fixture/reference-client harness. Every authenticated channel adapter must enter through the same intake and decision services rather than duplicate lifecycle logic.

## 10. Runtime Host

### 10.1 Single-instance ownership

The existing `carsinos-gateway` executable evolves into the single `carsinos-runtime-host`; no wrapper or sibling runtime process is introduced. Ownership is scoped to `(OS user identity, canonical state root, installation/profile identity)`. App-bound mode and background mode negotiate ownership through an OS-appropriate named lock plus authenticated native local control channel. Port availability and HTTP settings alone are not ownership or process-control proof.

Every host generation is monotonically persisted in authoritative storage and required by every mutating transaction and action claim. A replaced or stale host cannot write or dispatch. Startup handles stale locks, port collisions, concurrent app launch, user logoff/login, sleep/wake, crash restart, upgrade, uninstall, and schema-replacement activation without spawning competing runtimes.

Desired host mode is `app_bound` or `background`; actual state is `stopped`, `starting`, `running_app_bound`, `handoff`, `running_background`, `draining`, or `faulted`. `start_at_login=true` is invalid unless desired mode is `background`. With active work and background disabled, a voluntary UI close requires explicit pause/stop confirmation and a successful drain. Forced termination produces a durable runtime-paused/fault attention item on restart; it never creates reassuring silence.

### 10.2 Windows

- Background operation is opt-in.
- Start-at-login is a separate explicit setting.
- The installed program registers a current-user Task Scheduler entry; it does not require a machine service or administrator privileges for ordinary setup.
- Closing the UI leaves the opted-in host running. With background mode off, app close stops app-owned runtime cleanly.
- Initial release support is Windows 11 23H2 or later on x86-64. Installation, enable/disable, repair, upgrade, logout/login, reboot, sleep/wake, crash, uninstall, and MSI lifecycle behavior must be exercised on a clean non-developer profile using the exact hashed release candidate.

### 10.3 macOS

- The same host binary contract runs per user.
- Background/start-at-login uses Apple-supported per-user service management and a LaunchAgent-equivalent lifecycle, without root.
- Initial macOS support is macOS 15 or later on Apple Silicon. Enable/disable, app close, login, reboot, sleep/wake, crash, upgrade, and uninstall behavior must be exercised on a clean non-developer profile on the available M4 Mac mini using the exact hashed release candidate.
- Behavioral APIs, persistence, fencing, and evidence formats must remain platform-identical where OS mechanics do not require a documented difference.

### 10.4 Packaging

The Tauri application may bundle the runtime host as an external binary/sidecar. Builds must be reproducible enough to identify exact source revision and artifact hashes. Host diagnostics expose safe version, ownership mode, PID/start time, fencing generation, state-root version, restart reason, and health without exposing secrets.

Windows and macOS are separately gated milestones; Windows may ship first only if product copy and artifacts make macOS unavailable rather than implying parity. The overall three-document goal does not close until both platform gates pass. Signing/notarization and update-channel requirements must match the repository's public distribution policy; an unsigned or blocked artifact cannot be called production-ready where the OS distribution path requires those controls.

## 11. Security Invariants

- Exact owner authority, deterministic known-danger matching, decision binding, and technical execution validity are enforced in code. Model judgment may add the one optional danger confirmation described in Section 5.2, but it may not mint owner authority, bypass a mandatory confirmation, veto an owner-confirmed action, or cause repeated confirmation of the unchanged action.
- Ingress text, retrieved content, connector output, tool output, and child-worker messages are untrusted evidence.
- Child workers receive no authority broader than the action envelope they are assigned.
- Composite actions, aliases, plugins, shell indirection, and changed tool versions are reclassified before execution.
- Path and workspace scope is resolved canonically and protected against traversal, symlink/junction races, and case/normalization differences.
- Any applicable dangerous-action confirmation or other typed decision is validated before side effects and committed with a fenced action claim atomically; no side effect may precede the compare-and-swap.
- Secret scanning/redaction is fail closed for ordinary receipt/export/notification paths.
- Integrity failure is surfaced as a security state and cannot be silently repaired by rewriting history.
- Schema-replacement/archive/restore requires the runtime stopped or a proven quiescent fenced state.

## 12. Verification Gates

All gates are required. Tests must use fresh isolated state roots under the project drive and preserve user state.

`PASS` means the exact required behavior was observed in authoritative persisted state and, where required, from the exact packaged release-candidate artifact on the named real platform. Code inspection, model/worker self-report, successful tool return, mock-only execution, reduced fixtures, screenshots without underlying state, or owner waiver cannot substitute for required runtime evidence.

`CONDITIONAL` means implementation exists but required external, platform, security, restore, or compatibility proof is unavailable. Conditional is not shippable, release-ready, completed, or green. `FAIL` includes a skipped gate, missing evidence, unresolved release blocker, weakened assertion, duplicate or unauthorized effect, unverifiable completion, integrity failure, data loss, or unavailable mandatory proof.

### 12.1 Contract and state tests

- Exhaustive allowed/forbidden lifecycle transition tests.
- Property tests for state revision monotonicity, terminal immutability, classifier determinism, exact owner-authority precedence, operational-profile intersection, and receipt-chain verification.
- API schema, pagination, cursor, idempotency, deep-link, error, and event compatibility tests.
- Reference fixture demonstrates conversational answer, ordinary delegation without permission theater, dangerous-action confirmation continuation, stop/resume, external wait, recovery, completion, partial completion, and recurring occurrence.
- Reference and packaged-consumer fixtures prove immutable lineage and exactly one delegation across kill/restart at every transition, with no orphan runnable work.

### 12.2 Concurrency and fault injection

Inject failure before and after every durability boundary for intake, continuation enqueue, claim, technical resource reservation, decision resolution, external invocation, receipt append, completion, summary projection, and outbox publication.

Prove:

- one continuation per transition;
- no stale-fence commit;
- no double technical-resource consumption;
- no duplicate external effect where provider idempotency/reconciliation can prevent it;
- ambiguous effects become `outcome_unknown`, never blind retries;
- concurrent confirm/decline/stop/policy-reduce outcomes are serialized and receipt-backed;
- restart/replay converges to the same authoritative state.

An out-of-process effect recorder is mandatory for the crash-after-effect/before-commit matrix. Invocation count must remain one until absence is independently proven or a fresh duplicate-risk decision creates a new logical effect.

### 12.3 Adversarial security

- Prompt and connector injection cannot broaden authority.
- Decision payload mutation, replay, expiry, plan drift, tool-version drift, and target drift perform zero effects until the action is validly resolved.
- Dangerous-action challenges accept verified local or authenticated remote owner evidence and are exact, expiring before resolution, and single-resolution. A successful resolution creates a durable accepted grant with no expiry or use counter and authorizes unchanged replan/policy revalidation/continuation/restart/bounded retry/routine occurrence without another prompt.
- Known dangerous operations deterministically require exactly one confirmation; model-identified credible danger may require exactly one confirmation; neither path may become a categorical veto or a repeated prompt.
- Positive no-censorship cases prove that exact owner-directed external communication, permission change, project mutation, narrow deletion, secret delivery, and purchase-like use of an existing external tool proceed without a CarsinOS category veto or invented financial layer.
- Direct secret delivery tests prove the requested effect succeeds while the raw secret is absent from every persisted CarsinOS surface.
- Composite/alias/plugin/shell actions cannot evade classification.
- Path traversal, symlink/junction swap, secret-in-error, receipt/log/export/backup leaks, and notification leaks are tested.
- Receipt deletion, tail truncation, full rollback, insertion, modification, reordering, key loss, anchor rollback, and cross-root restore mismatch are detected.

### 12.4 Load and reliability

- Stress duplicate ingress, parallel actions, decision races, host restarts, outbox replay, recurring catch-up, and summary rebuild.
- Meet the pre-implementation SLO floor: warm summary p95 at most 250 ms for 1,000 delegations; durable intake admission p95 at most 500 ms excluding model/tool time; committed attention visible to local clients within 2 seconds; notification dispatch scheduled within 60 seconds; cold host ready within 15 seconds; host takeover/fault recovery within 30 seconds; global stop blocks new claims within 1 second; graceful drain within 15 seconds or transitions honestly to unresolved/paused evidence. Measurement may tighten but not weaken these values without change control.
- Run the complete existing Rust workspace, gateway process/security/benchmark suites, frontend typecheck/lint/unit/build tests, and targeted end-to-end smoke to prove no regression.

### 12.5 Platform proof

- Real Windows installed lifecycle matrix passes with background mode off/on and start-at-login off/on using the exact release candidate on a clean non-developer profile.
- Real macOS M4 lifecycle matrix passes for the equivalent supported modes using the exact release candidate on a clean non-developer profile.
- Upgrade/schema-replacement/archive/restore and app/background ownership handoff are tested on both.
- Artifact hashes, source revision, logs, screenshots or terminal receipts, and exact commands are retained in the release evidence folder.
- Evidence includes actual Task Scheduler or Apple service-manager state, login/reboot, UI close, crash, sleep/wake, upgrade, disable, uninstall, preserved-state hashes, timestamps, and artifact hashes. Mocks/emulators cannot satisfy a platform PASS.

### 12.6 Anti-laundering and consumer compatibility

- Deleting a required case, reducing a fixture, weakening an assertion, skipping a test, or omitting evidence fails the gate.
- A harness proves the backend contract only. A user-facing release additionally requires the production consumer to execute intake through decision/continuation to terminal evidence against the packaged backend.
- Old schema/API use fails with an explicit version error. Source/runtime scans prove there is no dual read, legacy fallback, silent translation, duplicate summary/decision/scheduler/host authority, financial contract field, money-kind resource dimension, category-laudered approval floor, absolute destructive-action refusal, or repeated confirmation path.

## 13. Delivery Sequence

1. Lock this specification, its execution checklist, and blockerboard through SpecSwarm.
2. Implement protocol/domain types, schema-replacement tooling, delegation storage, receipts, and property tests.
3. Implement canonical owner authority, typed decisions, one-confirmation danger handling, exact continuation, leases/fencing, technical resource quotas, recovery, and fault injection.
4. Implement intake, orchestration adapters, completion assessor, summary/notification projections, APIs, outbox events, and reference harness.
5. Implement and prove the Windows runtime host and packaging lifecycle.
6. Implement and prove macOS parity on the available M4 Mac mini.
7. Run full regressions, security/release gates, documentation reconciliation, frontend handoff validation, and protected-main PR review/CI.

No phase may mark a requirement complete from code inspection alone when its acceptance criterion demands runtime, fault, security, platform, or external evidence.

## 14. Definition of Done

This correction is done only when:

- every locked checklist item is checked with an evidence link;
- the blockerboard has no open release blocker and every risk has a tested mitigation or explicit accepted disposition;
- a request remains one coherent delegation through any applicable decision, recovery, stop/resume, and terminal reporting;
- ordinary exact owner requests proceed without permission theater, while destructive or dangerous actions receive exactly one concrete consequence confirmation and proceed after confirmation;
- Needs You, In Motion, Done, Next, and Receipts are derived from authoritative state;
- completion language is outcome-based and evidence-backed;
- Windows and macOS background lifecycle claims are proven on real supported hardware;
- the frontend owner receives runnable schemas, examples, fixtures, state/event/error/deep-link documentation, and does not need to infer backend behavior;
- full local regression, security, packaging, protected-main CI, and review are green with zero unresolved actionable or nit findings;
- checkpoint ledgers identify the exact final commit, proof artifacts, and any explicitly deferred non-blocker.

The final product test is: **Did CarsinOS take responsibility for the work, or did it merely give the user a better console from which to manage the work?**

## 15. Change Control

This locked version 1.1 control set amends and supersedes the locked version 1.0 control set. Version 1.0's financial plumbing, blanket category approval floors, deny-wins authority model, and destructive-action hard-lock semantics are rejected and are not implementation authority. The owner decision for version 1.1 is: one owner may direct their ExecAss; ordinary exact requests proceed; dangerous or destructive actions receive one concrete consequence confirmation; the confirmed unchanged action then executes; CarsinOS supplies no financial or morality subsystem.

Version 1.1 was locked through one completed lock-transition batch that: folded the required SpecSwarm gap, implementation-map, guardrail, and fresh final-QA passes into this specification, its checklist, and blockerboard; set all three to the same locked version; hash-identified all three together; reopened or superseded affected implementation evidence; recorded the transition in both checkpoint ledgers; and verified the final hashes, checkpoint JSON, and document status. After lock, any material change must update all affected control documents together, record the reason and owner decision, rerun affected review/test gates, and append a checkpoint. Silent scope drift is prohibited.
