import { useState, useCallback, useEffect } from "react";
import {
  Compass,
  ExternalLink,
  Kanban,
  CalendarDays,
  Eye,
  Mail,
  MessagesSquare,
  Bot,
  Users,
  Activity,
  Gauge,
  Workflow,
  Brain,
  Cable,
  Lightbulb,
  AlertTriangle,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import type { MissionControlTab } from "../../app/useAppController";

/* ── Types ────────────────────────────────────────────────────────────── */

interface HelpDocsPageProps {
  onOpenTab: (tab: MissionControlTab) => void;
  onStartTour: () => void;
  /** When set, jump to this section's page (used by per-tab "Docs" buttons). */
  targetSection?: string;
  /** Incrementing counter to force re-navigation even when targetSection is unchanged. */
  targetSeq?: number;
}

type SectionId =
  | "getting-started"
  | "boards"
  | "calendar"
  | "focus"
  | "mail"
  | "rooms"
  | "assistant"
  | "team"
  | "events"
  | "cockpit"
  | "strategy"
  | "runbook"
  | "memory"
  | "connectors"
  | "troubleshooting";

interface TocEntry {
  id: SectionId;
  label: string;
  tier: "intro" | "core" | "advanced" | "outro";
}

/* ── Table of Contents ────────────────────────────────────────────────── */

const TOC: TocEntry[] = [
  { id: "getting-started", label: "Getting Started", tier: "intro" },
  { id: "boards", label: "Boards", tier: "core" },
  { id: "calendar", label: "Calendar", tier: "core" },
  { id: "focus", label: "Focus", tier: "core" },
  { id: "mail", label: "Mail", tier: "core" },
  { id: "rooms", label: "Rooms", tier: "core" },
  { id: "assistant", label: "Assistant", tier: "core" },
  { id: "team", label: "Team", tier: "core" },
  { id: "events", label: "Events", tier: "advanced" },
  { id: "cockpit", label: "Cockpit", tier: "advanced" },
  { id: "strategy", label: "Strategy", tier: "advanced" },
  { id: "runbook", label: "Runbook", tier: "advanced" },
  { id: "memory", label: "Memory", tier: "advanced" },
  { id: "connectors", label: "Connectors", tier: "advanced" },
  { id: "troubleshooting", label: "Troubleshooting", tier: "outro" },
];

const TAB_FOR_SECTION: Partial<Record<SectionId, MissionControlTab>> = {
  boards: "boards",
  calendar: "calendar",
  focus: "focus",
  mail: "mail",
  rooms: "chatrooms",
  assistant: "assistant",
  team: "team",
  events: "events",
  cockpit: "cockpit",
  strategy: "strategy",
  runbook: "runbook",
  memory: "memory",
  connectors: "connectors",
};

/* ── Reusable doc primitives ──────────────────────────────────────────── */

function Tip({ children }: { children: React.ReactNode }) {
  return (
    <aside className="mc-docs-callout mc-docs-callout-tip">
      <Lightbulb size={14} aria-hidden />
      <div>{children}</div>
    </aside>
  );
}

function Caution({ children }: { children: React.ReactNode }) {
  return (
    <aside className="mc-docs-callout mc-docs-callout-caution">
      <AlertTriangle size={14} aria-hidden />
      <div>{children}</div>
    </aside>
  );
}

function OpenTabButton({
  tab,
  label,
  onOpenTab,
}: {
  tab: MissionControlTab;
  label: string;
  onOpenTab: (t: MissionControlTab) => void;
}) {
  return (
    <button
      type="button"
      className="mc-docs-open-tab"
      onClick={() => onOpenTab(tab)}
    >
      <ExternalLink size={13} aria-hidden /> Open {label}
    </button>
  );
}

/* ── Section content components ───────────────────────────────────────── */

function GettingStarted({ onOpenTab, onStartTour }: HelpDocsPageProps) {
  return (
    <>
      <p>
        Mission Control is your command center for managing AI agents. You tell agents what to do, watch them work, and approve their actions before anything goes live. Think of it as a control room where you're the operator.
      </p>

      <h4>Your first 5 minutes</h4>
      <ol>
        <li>
          <strong>Create an agent.</strong> Go to <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("team")}>Team</button> and click "Create Agent." Give it a name and pick an AI provider (like OpenAI or Anthropic).
        </li>
        <li>
          <strong>Talk to it.</strong> Open <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("assistant")}>Assistant</button>, select your agent from the dropdown, pick a model, and send a message.
        </li>
        <li>
          <strong>Give it a task.</strong> Open <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("boards")}>Boards</button>, create a card describing what you want done, and click "Run Card."
        </li>
        <li>
          <strong>Check on it.</strong> Open <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("focus")}>Focus</button> to see if anything needs your approval, or if something went wrong.
        </li>
      </ol>

      <Tip>
        If this is your very first time, click the <strong>Guided Tour</strong> button below. It walks you through each tab with on-screen highlights.
      </Tip>

      <div className="mc-docs-actions">
        <button type="button" onClick={onStartTour}>
          <Compass size={14} aria-hidden /> Start Guided Tour
        </button>
        <button type="button" className="ghost" onClick={() => onOpenTab("boards")}>
          <ExternalLink size={14} aria-hidden /> Jump to Boards
        </button>
      </div>

      <h4>How the tabs are organized</h4>
      <p>
        The left sidebar splits tabs into two groups:
      </p>
      <ul>
        <li><strong>Core tabs</strong> (top group) — The tabs you'll use every day: Boards, Calendar, Focus, Mail, Rooms, Assistant, and Team.</li>
        <li><strong>Advanced tabs</strong> (below the divider) — Power-user tools you can ignore until you need them: Events, Cockpit, Strategy, Runbook, Memory, and Connectors.</li>
      </ul>
      <p>
        You can jump to any tab with a keyboard shortcut. Hold <kbd>Ctrl</kbd> (or <kbd>Cmd</kbd> on Mac) and press a number key. The shortcut is shown next to each tab in the sidebar.
      </p>
    </>
  );
}

function BoardsDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Boards is a kanban-style task manager. You create cards, each card is one piece of work for an AI agent, and you run them. Cards move through columns (like "To Do," "In Progress," "Done") as they progress.
      </p>
      <OpenTabButton tab="boards" label="Boards" onOpenTab={onOpenTab} />

      <h4>Key concepts</h4>
      <ul>
        <li><strong>Board</strong> — A collection of columns and cards. You can have multiple boards for different projects.</li>
        <li><strong>Column / Lane</strong> — A vertical section of the board (e.g., "Backlog," "In Progress," "Done"). Drag cards between columns to track progress.</li>
        <li><strong>Card</strong> — One task. Has a title, description, owner (which agent does the work), tags, and a due date.</li>
      </ul>

      <h4>How to create and run a card</h4>
      <ol>
        <li>Click <strong>New Card</strong> in the toolbar.</li>
        <li>Fill in a title and description. The description is what the agent will read as its instructions.</li>
        <li>Pick an <strong>owner</strong> (the agent that will do the work).</li>
        <li>Click <strong>Save</strong>.</li>
        <li>Open the card and click <strong>Run Card</strong>. The agent starts working.</li>
      </ol>

      <h4>The card editor</h4>
      <p>Click any card to open the editor. It has three tabs:</p>
      <ul>
        <li><strong>Details</strong> — Title, description, owner, tags, due date, column assignment.</li>
        <li><strong>Script</strong> — An optional script that tells the agent exactly what steps to follow. More structured than a plain description.</li>
        <li><strong>Assets</strong> — Upload files for the agent to reference. PDFs, images, text files, etc.</li>
      </ul>

      <Tip>
        Cards don't run automatically. You always click "Run Card" to start execution. This gives you a chance to review the instructions first.
      </Tip>

      <Caution>
        Running a card costs an API call to your AI provider. Make sure the agent has a provider and model set up in Team first.
      </Caution>
    </>
  );
}

function CalendarDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Calendar shows all scheduled and recurring jobs across your agents. Think of it as a weekly planner for automated tasks. You can see what's coming up, trigger jobs immediately, or pause them.
      </p>
      <OpenTabButton tab="calendar" label="Calendar" onOpenTab={onOpenTab} />

      <h4>What you'll see</h4>
      <ul>
        <li><strong>Week grid</strong> — A 7-day view showing which jobs are scheduled for each day. Click a job block to run it now.</li>
        <li><strong>Schedule table</strong> — A detailed list of all scheduled jobs with their cron expressions, assigned agents, and enabled/disabled status.</li>
        <li><strong>Always Running</strong> — A strip showing daemon-style jobs that run continuously, not on a schedule.</li>
        <li><strong>Next Up</strong> — Jobs that will fire soonest.</li>
      </ul>

      <h4>Common actions</h4>
      <ol>
        <li><strong>Run now</strong> — Click the play button next to any job to trigger it immediately, regardless of its schedule.</li>
        <li><strong>Pause / Resume</strong> — Toggle the enable switch to stop a job from auto-running. It stays in the calendar but won't fire until you re-enable it.</li>
        <li><strong>View details</strong> — Click a job to see its full configuration including the cron expression, agent, and last run time.</li>
      </ol>

      <Tip>
        Jobs are created in the gateway configuration, not in Mission Control. The Calendar is a read-and-control view — you can run, pause, and resume, but creating new scheduled jobs is done on the backend.
      </Tip>

      <Caution>
        A disabled job won't auto-run. If you paused a job and something isn't happening, check Calendar to see if it's still paused.
      </Caution>
    </>
  );
}

function FocusDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Focus is your attention queue. Everything that needs a human decision shows up here: pending approvals, failed jobs, broken connections, and tripped circuit breakers. Check Focus first when you sit down.
      </p>
      <OpenTabButton tab="focus" label="Focus" onOpenTab={onOpenTab} />

      <h4>Two tabs</h4>
      <ul>
        <li><strong>Queue</strong> — Items needing your attention, sorted by urgency. Each item has a severity level (info, warning, critical) and a category.</li>
        <li><strong>System Status</strong> — Overall health of your channels and services. Shows connection status for Discord, Telegram, Slack, etc.</li>
      </ul>

      <h4>What shows up in the queue</h4>
      <ul>
        <li><strong>Approval requests</strong> — An agent wants to do something that requires your OK (like sending a message, running a tool, etc.). You approve or deny directly from the card.</li>
        <li><strong>Job failures</strong> — A scheduled or board-triggered job failed. Click to see what went wrong and retry.</li>
        <li><strong>Channel health</strong> — A chat channel (Discord, Telegram, etc.) disconnected. Click "Reconnect" to bring it back online.</li>
        <li><strong>Circuit breakers</strong> — Too many consecutive failures tripped a safety switch. The system paused that operation to prevent cascading errors.</li>
      </ul>

      <h4>How to handle approvals</h4>
      <ol>
        <li>Click the item to expand its details.</li>
        <li>Review what the agent is asking to do.</li>
        <li>Click <strong>Approve</strong> or <strong>Deny</strong>.</li>
      </ol>

      <Tip>
        When Focus says "All clear," everything is healthy. No action needed from you.
      </Tip>

      <Caution>
        Unresolved approval requests block the agent from continuing. If an agent seems stuck, check Focus — it might be waiting for your approval.
      </Caution>
    </>
  );
}

function MailDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Mail is for private, 1-on-1 conversations between you and a single agent. It works like email threads — you send a message, the agent can read it, and you can attach files.
      </p>
      <OpenTabButton tab="mail" label="Mail" onOpenTab={onOpenTab} />

      <h4>How it works</h4>
      <ol>
        <li>Click <strong>New Thread</strong>.</li>
        <li>Pick a <strong>mailbox</strong> (usually "operator") and a <strong>principal</strong> (the agent you're writing to).</li>
        <li>Type a subject and your message.</li>
        <li>Hit <strong>Send</strong>.</li>
      </ol>

      <h4>Key features</h4>
      <ul>
        <li><strong>Attachments</strong> — Upload files in the compose area. The agent can download and read them.</li>
        <li><strong>Acknowledge</strong> — Mark a message as "read" so you know what you've reviewed.</li>
        <li><strong>Summarize</strong> — Click "Summarize" on a thread to generate a quick summary note.</li>
        <li><strong>Leases</strong> — The Leases tab shows file locks. If an agent has a file checked out exclusively, other agents can't modify it until the lease expires or is released.</li>
      </ul>

      <Tip>
        Mail sends messages but doesn't trigger AI runs. If you want the agent to process your message with AI, use <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("assistant")}>Assistant</button> instead.
      </Tip>

      <h4>Filtering</h4>
      <p>
        Use the filter bar above the thread list to search by mailbox, principal, or keyword. Filters narrow the visible thread list.
      </p>
    </>
  );
}

function RoomsDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Rooms are group chat channels where multiple agents (and you) can talk together. Unlike Mail, which is 1-on-1, Rooms let several participants see the same conversation.
      </p>
      <OpenTabButton tab="chatrooms" label="Rooms" onOpenTab={onOpenTab} />

      <h4>How to create a room</h4>
      <ol>
        <li>Click <strong>New Room</strong>.</li>
        <li>Give it a name.</li>
        <li>Add participants (agents, other principals).</li>
        <li>Start chatting.</li>
      </ol>

      <h4>What you can do</h4>
      <ul>
        <li><strong>Send messages</strong> — Type in the compose area and press Send.</li>
        <li><strong>React with emoji</strong> — Click the reaction button on any message to add one of 12 emoji reactions.</li>
        <li><strong>Attach files</strong> — Toggle the attachment option in the compose area to upload files.</li>
        <li><strong>Acknowledge messages</strong> — Mark messages as read. "Acknowledge All" clears unread state for the whole room.</li>
        <li><strong>Room Settings</strong> — Click the settings icon to manage participants, view active leases, or perform moderation actions.</li>
      </ul>

      <h4>Rooms vs. Mail</h4>
      <ul>
        <li><strong>Mail</strong> — Private 1-on-1 threads. Only you and one agent.</li>
        <li><strong>Rooms</strong> — Group conversations. Multiple participants see everything.</li>
      </ul>

      <Tip>
        If you only need to talk to one agent privately, use Mail. Rooms are for when you need multiple agents or people in the same conversation.
      </Tip>
    </>
  );
}

function AssistantDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Assistant is a direct chat interface with an AI agent. Pick an agent, pick a model, and have a back-and-forth conversation. Each message you send triggers one AI response.
      </p>
      <OpenTabButton tab="assistant" label="Assistant" onOpenTab={onOpenTab} />

      <h4>Setting up a chat</h4>
      <ol>
        <li>Select an <strong>agent</strong> from the first dropdown.</li>
        <li>Select a <strong>provider</strong> (e.g., OpenAI, Anthropic, Ollama).</li>
        <li>Optionally pick a specific <strong>login</strong> (saved API key). Leave on "Auto" to use the agent's default.</li>
        <li>Select a <strong>model</strong> from the model dropdown. The list loads automatically from your provider.</li>
        <li>Type your message and press <strong>Send</strong> (or <kbd>Cmd+Enter</kbd>).</li>
      </ol>

      <h4>Features</h4>
      <ul>
        <li><strong>System Prompt</strong> — Click the collapsed "System Prompt" section to expand it. This sets the agent's behavior for the session. You can change and re-insert it anytime.</li>
        <li><strong>New Chat</strong> — Clears the conversation and starts fresh. Previous messages are not deleted from the backend.</li>
        <li><strong>Insert Core Prompt</strong> — Injects the agent's default system prompt into the conversation.</li>
        <li><strong>Send to Board</strong> — Takes the agent's last response and posts it to a Board as a note. Useful for saving results.</li>
        <li><strong>Reload Models</strong> — Re-fetches the model list from your provider if it seems stale or empty.</li>
      </ul>

      <h4>Understanding the status chips</h4>
      <p>
        At the bottom of the toolbar you'll see small chips:
      </p>
      <ul>
        <li><strong>session: abc123</strong> — The ID of your current conversation session.</li>
        <li><strong>run: abc123</strong> — The ID of the last execution run.</li>
        <li><strong>run: completed</strong> — The status of the last run (completed, failed, etc.).</li>
      </ul>

      <Caution>
        You need at least one agent created in Team before Assistant will work. If you see "Create an agent first," go to <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("team")}>Team</button> and make one.
      </Caution>

      <Tip>
        Each message costs one API call to your provider. If you're testing, consider using a cheaper model like a local Ollama instance.
      </Tip>
    </>
  );
}

function TeamDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Team is where you create and manage your AI agents. Every agent needs a name, an AI provider (like OpenAI or Anthropic), and a model before it can do anything.
      </p>
      <OpenTabButton tab="team" label="Team" onOpenTab={onOpenTab} />

      <h4>Creating an agent</h4>
      <ol>
        <li>Click <strong>Create Agent</strong> (or "Create Your First Agent" if the list is empty).</li>
        <li>Give the agent a <strong>name</strong> (e.g., "Research Assistant," "Code Reviewer").</li>
        <li>Set the <strong>role</strong> — a short description of what this agent does.</li>
        <li>Assign a <strong>provider</strong> and <strong>model</strong> — this determines which AI the agent uses to think.</li>
        <li>Optionally set a <strong>persona</strong> — system prompt instructions that shape the agent's personality and behavior.</li>
        <li>Click <strong>Save</strong>.</li>
      </ol>

      <h4>Agent settings</h4>
      <ul>
        <li><strong>Provider & Model</strong> — Which AI service and model the agent uses (GPT-4, Claude, Llama, etc.).</li>
        <li><strong>Auth profile</strong> — Which API key / login credentials the agent uses to talk to its provider.</li>
        <li><strong>Tool permissions</strong> — Control what tools the agent can use: file access, web browsing, code execution, etc. Restrict these to limit what the agent can do.</li>
        <li><strong>Persona</strong> — Custom system prompt instructions. Think of it as the agent's personality and operating instructions.</li>
      </ul>

      <Tip>
        Start with one agent. You can always create more later. One well-configured agent is better than five half-configured ones.
      </Tip>

      <Caution>
        An agent without a provider and model assigned can't do anything. If your agent seems broken, check Team to make sure it has both set.
      </Caution>
    </>
  );
}

function EventsDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Events is a live log of everything happening inside carsinOS. Every action, state change, error, and heartbeat shows up here. It's a debugging tool — you don't need to watch it during normal operation.
      </p>
      <OpenTabButton tab="events" label="Events" onOpenTab={onOpenTab} />

      <h4>Filtering events</h4>
      <p>
        The filter bar at the top lets you narrow events by domain:
      </p>
      <ul>
        <li><strong>All</strong> — Everything.</li>
        <li><strong>Board</strong> — Card runs, board changes.</li>
        <li><strong>Job</strong> — Scheduled job executions.</li>
        <li><strong>Approval</strong> — Approval requests and resolutions.</li>
        <li><strong>Channel</strong> — Discord/Telegram/Slack connection events.</li>
        <li><strong>Mail</strong> — Message send/receive events.</li>
      </ul>

      <h4>Reading an event</h4>
      <p>
        Each event row shows the event type, a summary, the related entity, and a timestamp. Click the expand arrow to see the full JSON payload — useful for debugging.
      </p>

      <Tip>
        Toggle "Show heartbeats" off to hide the constant health-check pings. This makes it much easier to find the events you care about.
      </Tip>
    </>
  );
}

function CockpitDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Cockpit is a customizable dashboard builder. You add widgets that show real-time data from your system: approval counts, job status, channel health, and more. It's completely optional.
      </p>
      <OpenTabButton tab="cockpit" label="Cockpit" onOpenTab={onOpenTab} />

      <h4>Getting started</h4>
      <ol>
        <li>Click <strong>Load Ops Template</strong> to start with a pre-built dashboard, or click <strong>Add Widget</strong> to build from scratch.</li>
        <li>Click <strong>Edit</strong> to enter edit mode. You can now drag, resize, and rearrange widgets.</li>
        <li>Click <strong>Done</strong> when you're happy with the layout.</li>
      </ol>

      <h4>Pages</h4>
      <p>
        Cockpit supports multiple pages. Each page is its own dashboard. Use the sidebar on the left to switch pages, add new ones, or delete existing ones. Think of pages like tabs within the Cockpit.
      </p>

      <h4>Available widgets</h4>
      <p>There are 17+ built-in widget types covering:</p>
      <ul>
        <li>Approval counts and pending items</li>
        <li>Job status and next scheduled runs</li>
        <li>Channel health (connected / disconnected)</li>
        <li>Agent status overview</li>
        <li>Circuit breaker states</li>
        <li>Event feed (mini version)</li>
        <li>Custom widgets (you define the data source and display)</li>
      </ul>

      <Tip>
        The "Load Ops Template" button gives you a reasonable starting dashboard. You can customize from there instead of building from scratch.
      </Tip>
    </>
  );
}

function StrategyDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Strategy is a planning tool for organizing work into goals, projects, and tasks. Use it to track the big picture and break large objectives into smaller, actionable pieces.
      </p>
      <OpenTabButton tab="strategy" label="Strategy" onOpenTab={onOpenTab} />

      <h4>The hierarchy</h4>
      <ul>
        <li><strong>Goal</strong> — A high-level objective (e.g., "Launch product by Q2").</li>
        <li><strong>Project</strong> — A collection of tasks that supports a goal (e.g., "Build onboarding flow").</li>
        <li><strong>Task</strong> — A single piece of work within a project (e.g., "Write welcome email copy").</li>
      </ul>

      <h4>How to use it</h4>
      <ol>
        <li>Create a <strong>Goal</strong> to define what you're trying to achieve.</li>
        <li>Add <strong>Projects</strong> under that goal to organize the work.</li>
        <li>Break each project into <strong>Tasks</strong>.</li>
        <li>Track status as work progresses. Find blocked or stale items easily.</li>
      </ol>

      <h4>Linking to execution</h4>
      <p>
        Strategy items can link to Runbook entries so you can trace from "why" (the goal) to "what happened" (the execution log). This is optional but powerful for post-mortems.
      </p>

      <Caution>
        Strategy is for planning. Actual execution happens in Boards (one-off tasks) and Calendar (recurring jobs). Strategy doesn't run anything — it just tracks the plan.
      </Caution>
    </>
  );
}

function RunbookDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Runbook records every AI execution from start to finish. When an agent runs a task (from Boards or Assistant), the full step-by-step trace — messages sent, tool calls made, tokens used, decisions taken — is saved here.
      </p>
      <OpenTabButton tab="runbook" label="Runbook" onOpenTab={onOpenTab} />

      <h4>What you'll see</h4>
      <ul>
        <li><strong>Summary strip</strong> — Counts of Pending, Active, Waiting, and Blocked runs at the top.</li>
        <li><strong>Browse sidebar</strong> — Filter and search runs by query, kind, status, or owner agent.</li>
        <li><strong>Detail view</strong> — Click a run to see its full execution flow: every message, tool call, and decision in chronological order.</li>
      </ul>

      <h4>How to investigate a run</h4>
      <ol>
        <li>Use the filter bar to find the run you're looking for (by agent, status, or keyword).</li>
        <li>Click the run in the sidebar.</li>
        <li>Read through the flow diagram. Each step shows what happened.</li>
        <li>Click a step to see linked artifacts (the board card, the tool output, etc.).</li>
      </ol>

      <Tip>
        The RunbookLink panel that appears in Assistant and Boards links directly to the relevant Runbook entry. Click it to jump straight to the execution trace.
      </Tip>

      <Caution>
        Runbook is read-only. It shows what happened but doesn't let you edit or re-run. Go back to Boards, Calendar, or Team to make changes.
      </Caution>
    </>
  );
}

function MemoryDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Memory lets you inspect what an agent has learned and stored. Each agent has its own separate memory — facts, concepts, and connections it has built over time.
      </p>
      <OpenTabButton tab="memory" label="Memory" onOpenTab={onOpenTab} />

      <h4>Main sections</h4>
      <ul>
        <li><strong>Agent selector</strong> — Switch between agents using the dropdown. Each agent's memory is completely separate.</li>
        <li><strong>Memory Cards</strong> — Individual facts and knowledge the agent has stored. Each card has a name and a value.</li>
        <li><strong>Episodes</strong> — Significant past events the agent remembers, with context about when and why they happened.</li>
        <li><strong>Knowledge Graph</strong> — A visual map of how concepts connect. Nodes are concepts, edges show relationships. Limited to 60 nodes for readability.</li>
        <li><strong>Reasoning & Sources</strong> — When the agent used its memory to make a decision, you can see which citations it pulled from.</li>
      </ul>

      <h4>How to use it</h4>
      <ol>
        <li>Pick an agent from the dropdown.</li>
        <li>Browse their memory cards to see what facts they know.</li>
        <li>Use the search bar to find specific memories.</li>
        <li>Click a node in the knowledge graph to see its connections and details.</li>
      </ol>

      <Tip>
        Memory is read-only in Mission Control. The agent builds its own memory during runs. You can inspect it here to understand why the agent made specific decisions.
      </Tip>
    </>
  );
}

function ConnectorsDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        Connectors manage external integrations — Discord bots, Telegram bots, Slack apps, and custom API tools. This is where you add channels for agents to communicate through and tools for agents to use.
      </p>
      <OpenTabButton tab="connectors" label="Connectors" onOpenTab={onOpenTab} />

      <h4>Supported integrations</h4>
      <ul>
        <li><strong>Discord</strong> — Connect a Discord bot so agents can read and post in Discord channels.</li>
        <li><strong>Telegram</strong> — Connect a Telegram bot for agent interaction via Telegram chats.</li>
        <li><strong>Slack</strong> — Connect a Slack app.</li>
        <li><strong>Signal, WhatsApp, Twitch, BlueBubbles</strong> — Additional channel adapters.</li>
        <li><strong>Custom API tools</strong> — Import OpenAPI specs, GraphQL schemas, or MCP tool definitions for agents to call.</li>
      </ul>

      <h4>Setting up a channel (e.g., Discord)</h4>
      <ol>
        <li>Browse the <strong>connector catalog</strong> for the integration you want.</li>
        <li>Click to import it. This adds it to your workspace but doesn't activate it yet.</li>
        <li>Configure <strong>auth</strong> — paste your bot token or API key.</li>
        <li><strong>Publish</strong> the connector to make it live.</li>
        <li><strong>Assign</strong> the connector to one or more agents so they can use it.</li>
      </ol>

      <h4>Connector lifecycle</h4>
      <ul>
        <li><strong>Imported</strong> — In your workspace but not active. Agents can't see it.</li>
        <li><strong>Published</strong> — Live and available to assigned agents.</li>
        <li><strong>Stopped</strong> — Manually paused. Can be republished.</li>
      </ul>

      <Caution>
        Only published connectors are visible to agents. If an agent can't reach Discord/Telegram, check that the connector is published and assigned to that agent.
      </Caution>

      <Tip>
        Auth profiles store credentials securely. You can create multiple auth profiles for the same provider if you have different bots for different purposes.
      </Tip>
    </>
  );
}

function TroubleshootingDocs({ onOpenTab }: { onOpenTab: (t: MissionControlTab) => void }) {
  return (
    <>
      <p>
        If something isn't working, start here. These are the most common issues and how to fix them.
      </p>

      <h4>"My agent isn't doing anything"</h4>
      <ol>
        <li>Check <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("team")}>Team</button> — does the agent have a provider and model assigned?</li>
        <li>Check <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("focus")}>Focus</button> — is there a pending approval blocking it?</li>
        <li>Check <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("events")}>Events</button> — are there error events from the agent?</li>
      </ol>

      <h4>"My Discord/Telegram bot isn't responding"</h4>
      <ol>
        <li>Check <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("focus")}>Focus</button> &rarr; System Status — is the channel connected?</li>
        <li>If disconnected, click <strong>Reconnect</strong>.</li>
        <li>Check <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("connectors")}>Connectors</button> — is the connector published and assigned to an agent?</li>
        <li>Check that the bot token is still valid.</li>
      </ol>

      <h4>"The model dropdown is empty"</h4>
      <ol>
        <li>Make sure you've selected a provider first.</li>
        <li>Click <strong>Reload Models</strong> to re-fetch the list.</li>
        <li>If it still fails, the provider might be unreachable. Check your auth credentials in <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("connectors")}>Connectors</button>.</li>
      </ol>

      <h4>"I see a circuit breaker warning"</h4>
      <p>
        Circuit breakers trip when the same operation fails too many times in a row. The system pauses that operation to prevent further damage. Wait for the cooldown period, then retry. If it keeps tripping, there's an underlying issue — check Events for error details.
      </p>

      <h4>"Where did my run go?"</h4>
      <p>
        Every execution is logged in <button type="button" className="mc-docs-inline-link" onClick={() => onOpenTab("runbook")}>Runbook</button>. Use the filters to search by agent, status, or time range.
      </p>

      <Tip>
        When in doubt, check Focus first, then Events. Focus shows actionable items. Events shows everything.
      </Tip>
    </>
  );
}

/* ── Section icon map ─────────────────────────────────────────────────── */

const SECTION_ICON: Record<SectionId, React.ReactNode> = {
  "getting-started": <Compass size={16} />,
  boards: <Kanban size={16} />,
  calendar: <CalendarDays size={16} />,
  focus: <Eye size={16} />,
  mail: <Mail size={16} />,
  rooms: <MessagesSquare size={16} />,
  assistant: <Bot size={16} />,
  team: <Users size={16} />,
  events: <Activity size={16} />,
  cockpit: <Gauge size={16} />,
  strategy: <Compass size={16} />,
  runbook: <Workflow size={16} />,
  memory: <Brain size={16} />,
  connectors: <Cable size={16} />,
  troubleshooting: <AlertTriangle size={16} />,
};

/* ── Section renderer map ─────────────────────────────────────────────── */

const SECTION_CONTENT: Record<
  SectionId,
  (props: HelpDocsPageProps & { onOpenTab: (t: MissionControlTab) => void }) => React.ReactNode
> = {
  "getting-started": (p) => <GettingStarted {...p} />,
  boards: (p) => <BoardsDocs onOpenTab={p.onOpenTab} />,
  calendar: (p) => <CalendarDocs onOpenTab={p.onOpenTab} />,
  focus: (p) => <FocusDocs onOpenTab={p.onOpenTab} />,
  mail: (p) => <MailDocs onOpenTab={p.onOpenTab} />,
  rooms: (p) => <RoomsDocs onOpenTab={p.onOpenTab} />,
  assistant: (p) => <AssistantDocs onOpenTab={p.onOpenTab} />,
  team: (p) => <TeamDocs onOpenTab={p.onOpenTab} />,
  events: (p) => <EventsDocs onOpenTab={p.onOpenTab} />,
  cockpit: (p) => <CockpitDocs onOpenTab={p.onOpenTab} />,
  strategy: (p) => <StrategyDocs onOpenTab={p.onOpenTab} />,
  runbook: (p) => <RunbookDocs onOpenTab={p.onOpenTab} />,
  memory: (p) => <MemoryDocs onOpenTab={p.onOpenTab} />,
  connectors: (p) => <ConnectorsDocs onOpenTab={p.onOpenTab} />,
  troubleshooting: (p) => <TroubleshootingDocs onOpenTab={p.onOpenTab} />,
};

/* ── Main component ───────────────────────────────────────────────────── */

export function HelpDocsPage(props: HelpDocsPageProps) {
  const [pageIndex, setPageIndex] = useState(0);
  const [appliedSeq, setAppliedSeq] = useState(0);

  /* Jump to a specific section when targetSection changes (state-during-render pattern) */
  const incomingSeq = props.targetSeq ?? 0;
  if (props.targetSection && incomingSeq !== appliedSeq) {
    setAppliedSeq(incomingSeq);
    const idx = TOC.findIndex((e) => e.id === props.targetSection);
    if (idx >= 0) {
      setPageIndex(idx);
    }
  }

  const goTo = useCallback((idx: number) => {
    setPageIndex(Math.max(0, Math.min(TOC.length - 1, idx)));
  }, []);

  /* Arrow-key page navigation */
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "ArrowRight") {
        e.preventDefault();
        setPageIndex((i) => Math.min(TOC.length - 1, i + 1));
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        setPageIndex((i) => Math.max(0, i - 1));
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const entry = TOC[pageIndex];
  const tab = TAB_FOR_SECTION[entry.id];
  const progress = ((pageIndex + 1) / TOC.length) * 100;

  /* Group TOC entries for sidebar rendering */
  const tocGroups = [
    { label: null, entries: TOC.filter((e) => e.tier === "intro") },
    { label: "Core", entries: TOC.filter((e) => e.tier === "core") },
    { label: "Advanced", entries: TOC.filter((e) => e.tier === "advanced") },
    { label: null, entries: TOC.filter((e) => e.tier === "outro") },
  ];

  return (
    <section className="mc-docs-page" data-tour-id="help-page">
      {/* ── Sidebar TOC ── */}
      <nav className="mc-docs-sidebar" aria-label="Documentation navigation">
        <div className="mc-docs-sidebar-header">
          <span className="mc-docs-sidebar-kicker">Mission Control</span>
          <strong>Documentation</strong>
        </div>

        {tocGroups.map((group, gi) => (
          <div key={gi} className="mc-docs-toc-group">
            {group.label ? (
              <span className="mc-docs-toc-label">{group.label}</span>
            ) : null}
            {group.entries.map((tocEntry) => {
              const idx = TOC.indexOf(tocEntry);
              return (
                <button
                  key={tocEntry.id}
                  type="button"
                  className={`mc-docs-toc-item ${pageIndex === idx ? "active" : ""}`}
                  onClick={() => goTo(idx)}
                >
                  <span className="mc-docs-toc-icon">{SECTION_ICON[tocEntry.id]}</span>
                  {tocEntry.label}
                  <ChevronRight size={12} className="mc-docs-toc-chevron" />
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      {/* ── Content — one page at a time, no scroll ── */}
      <div className="mc-docs-content">
        <article className="mc-docs-section" key={entry.id}>
          <header className="mc-docs-section-header">
            <div className="mc-docs-section-icon">{SECTION_ICON[entry.id]}</div>
            <div>
              <h2>{entry.label}</h2>
              {entry.tier === "advanced" ? (
                <span className="mc-docs-tier-badge">Advanced</span>
              ) : null}
            </div>
            {tab ? (
              <button
                type="button"
                className="mc-docs-header-link ghost"
                onClick={() => props.onOpenTab(tab)}
              >
                <ExternalLink size={13} /> Open
              </button>
            ) : null}
          </header>
          <div className="mc-docs-section-body">
            {SECTION_CONTENT[entry.id](props)}
          </div>
        </article>
      </div>

      {/* ── Bottom pager ── */}
      <footer className="mc-docs-pager">
        <button
          type="button"
          className="mc-docs-pager-btn"
          disabled={pageIndex === 0}
          onClick={() => goTo(pageIndex - 1)}
        >
          <ChevronLeft size={16} />
          <span>{pageIndex > 0 ? TOC[pageIndex - 1].label : ""}</span>
        </button>

        <div className="mc-docs-pager-center">
          <span className="mc-docs-pager-count">
            {pageIndex + 1} / {TOC.length}
          </span>
          <div className="mc-docs-pager-track">
            <div className="mc-docs-pager-fill" style={{ width: `${progress}%` }} />
          </div>
          <span className="mc-docs-pager-hint">&larr; &rarr;</span>
        </div>

        <button
          type="button"
          className="mc-docs-pager-btn"
          disabled={pageIndex === TOC.length - 1}
          onClick={() => goTo(pageIndex + 1)}
        >
          <span>{pageIndex < TOC.length - 1 ? TOC[pageIndex + 1].label : ""}</span>
          <ChevronRight size={16} />
        </button>
      </footer>
    </section>
  );
}
