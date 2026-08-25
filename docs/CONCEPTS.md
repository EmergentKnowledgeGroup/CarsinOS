# How CarsinOS works

CarsinOS is built around a simple relationship:

> One owner gives one ExecAss coordinator an outcome. The system makes the work
> visible, controllable, and inspectable.

It is not trying to turn every request into an approval maze. It is also not
trying to hide consequential actions behind a cheerful “sure thing.” The point
is to keep the owner in charge while making real work durable enough to inspect,
recover, and correct.

## The cast

| Term | Meaning |
| --- | --- |
| **Owner** | The person whose authenticated local or allowlisted remote evidence carries authority in this CarsinOS instance. |
| **ExecAss** | The executive-assistant coordinator that receives outcomes, coordinates work, and brings meaningful decisions back. |
| **Workers** | Configured execution actors that may work within the owner’s existing authority. They do not become additional owners. |
| **Providers, channels, and tools** | Owner-configured capabilities that give work a route to models, communications, or bounded local/network actions. |
| **CarsinOS** | The local control plane: gateway, storage, policy, tools, schedules, receipts, and Mission Control interface. |
| **Mission Control** | The operator surface where the owner sees work and machinery instead of reconstructing it from chat. |

This is a one-owner model by design. It should not be read as a multi-user
permissions product, a shared SaaS workspace, or a marketplace of unrelated
agents.

## Conversation versus durable delegation

Not every sentence needs to become a project. An intake can remain
conversational, or it can become a durable delegation when the system needs to
coordinate and preserve the work.

A durable delegation carries more than an assistant reply:

- the owner’s intent and execution criteria;
- lifecycle state and history;
- plans, continuations, waits, and decisions when they apply;
- visible completion, partial-completion, or failure truth;
- receipt-backed evidence where the typed lifecycle records it; and
- recovery context if the work cannot safely continue automatically.

The admission path creates the work foundation atomically. That prevents a
half-created “task” where the UI looks committed but the authority, plan, or
durable lifecycle record never existed.

## From a request to a result

The normal path is intentionally conversational at the front and durable
underneath:

~~~text
You state an outcome
        ↓
ExecAss answers or opens a durable delegation
        ↓
Work moves, waits, completes, or asks for a decision
        ↓
Mission Control shows an authoritative projection
        ↓
Receipts and history support important lifecycle claims
~~~

That lets you say, “Handle the supplier follow-up,” without losing the ability
to later see what moved, what is waiting on someone else, what needs a decision,
and what evidence supports a stated result.

## The Office is the daily desk

The Office is the first place to look. It is meant to answer five questions
without making you hunt through a transcript:

| Question | Where it appears |
| --- | --- |
| What changed since I last looked? | The briefing and **Done since you checked** |
| What needs my judgment? | **Needs You** |
| What is actively progressing? | **In motion** |
| What is waiting on the outside world? | The briefing and work status |
| What should happen next? | **Next** and the planning/schedule surfaces |

You can also walk to the Assistant’s Desk to talk something through. That
temporary conversation is useful, but it does not pretend to be a replacement
for durable work. When a result needs to survive the sitting, it should be a
delegation with its own visible state and, where recorded, receipts.

## A confirmation is not a refusal

CarsinOS uses one confirmation for a dangerous ExecAss action. The intended
conversation is closer to a good secretary than a content filter:

~~~text
Owner: “Shred those files in that cabinet.”
ExecAss: “All of them? This permanently deletes the cabinet’s contents.”
Owner: “Yes.”
ExecAss: carries out that unchanged action.
~~~

The system is not supposed to endlessly veto the owner or turn ordinary work
into repeated warning dialogs. The confirmation should state the real
consequence once, wait for a yes or clarification, and preserve that reasoning
for the unchanged action.

“Unchanged” matters. The confirmation is bound to the exact action, target,
material/payload, tool/version, and declared consequence. If one of those
changes materially, the prior grant is invalidated rather than being silently
reused.

There can still be other kinds of questions—ordinary task decisions, recovery
choices, and mechanical-resolution pauses are different from the
dangerous-action confirmation rule.

## Policy is for unattended work

The policy surface governs how work that is inferred, scheduled, delegated, or
continued without a fresh owner instruction should proceed. It does not turn
the policy screen into a veto over a direct owner instruction.

That distinction is important:

- **Direct owner intent** should not get moralized or repeatedly second-guessed.
- **Dangerous execution** gets one exact-action confirmation.
- **Derived or unattended work** has a deliberate policy posture and may pause
  when authority, evidence, tool policy, or safe recovery is unavailable.

## Work can be honest without being perfect

Real operations have more states than “done” and “failed.” CarsinOS makes room
for those distinctions:

- **Working**: ExecAss is actively progressing the work.
- **Waiting on the world**: a dependency outside the system is holding things
  up; the owner does not need to manufacture an action just to clear the card.
- **Partially complete**: useful work finished, but a stated material remainder
  is still unresolved.
- **Needs a decision**: the system has reached a point where owner judgment is
  actually needed.
- **Recovery needed**: work cannot continue safely until a concrete recovery
  choice is made.
- **Late contrary evidence**: a later fact can correct a terminal outcome
  without silently rewriting the original receipt record.

This is why the product emphasizes receipts and state instead of a single
all-knowing “AI status” badge.

## The building: floors, rooms, and pins

Mission Control uses a Glass Office metaphor because different kinds of work
deserve different places:

| Floor | What belongs there |
| --- | --- |
| **The Office** | Owner briefing, delegation, decisions, and proof |
| **The Window** | Observed agent presence and Agent Mail-backed office chatter |
| **The Trenches** | Boards, schedules, strategy, staff, and history |
| **The Basement** | Models, connectors, runtime posture, locks, events, setup, and policy |

Mission Control is the whole operator application. Glass Office is the current
presentation and navigation model inside it. Rooms are not just decorative
tabs: stable room IDs let the elevator remain honest when rooms share an
underlying route, a capability is unavailable, or a configuration changes. You
can pin selected room shortcuts to the Office without duplicating the room’s
authoritative data.

## Evidence, posture, and recovery

CarsinOS should be calm only when the necessary authoritative facts are known.
If they have not loaded, it says **Checking** rather than inventing a healthy
status. If a known condition needs attention, it raises one focused incident and
points to the room that owns the next action.

Examples include a gateway the system cannot reach, an open circuit breaker, a
runtime recovery decision, a receipt-integrity problem, or failed/partial work
that explicitly needs the owner. A schedule that is idle, a normal external
wait, an owner freeze, or an ordinary confirmation prompt is not automatically
an incident.

## Local-first means local responsibility

CarsinOS is designed around a local gateway, local state, explicit credentials,
and inspectable tool boundaries. It does not turn a source build into a hosted
service, make every provider/channel adapter production-ready by its name alone,
or erase the responsibility that comes with connecting tools and external
accounts.

For the system map, read [Architecture](ARCHITECTURE.md). For operating
boundaries, read [Security model](SECURITY_MODEL.md).
