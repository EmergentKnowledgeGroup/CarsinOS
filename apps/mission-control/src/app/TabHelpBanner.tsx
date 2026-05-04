import { useState } from "react";
import { BookOpen, Compass, Lightbulb, X } from "lucide-react";
import type { MissionControlTab } from "./useAppController";

export type HelpTab = Exclude<MissionControlTab, "help">;

/** Maps Mission Control tab IDs to help-docs section IDs. */
const TAB_TO_HELP_SECTION: Record<HelpTab, string> = {
  boards: "boards",
  calendar: "calendar",
  focus: "focus",
  mail: "mail",
  chatrooms: "rooms",
  assistant: "assistant",
  team: "team",
  events: "events",
  cockpit: "cockpit",
  strategy: "strategy",
  runbook: "runbook",
  memory: "memory",
  connectors: "connectors",
};

interface TabHelpBannerProps {
  tab: HelpTab;
  onOpenDocs: (section?: string) => void;
  onStartTour: () => void;
  onDismissAll: () => void;
}

interface HelpCopy {
  title: string;
  summary: string;
  examples: string[];
}

const HELP_COPY: Record<HelpTab, HelpCopy> = {
  boards: {
    title: "Create tasks and run them",
    summary:
      "Each card is one task. Fill it in, click Run Card, and your agent does the work.",
    examples: [
      "Click '+ New Card', type what you want done, then click Run Card to execute it.",
      "Drag cards between columns to track progress (To Do, In Progress, Done).",
      "Attach a file to a card so the agent has context when it runs.",
    ],
  },
  calendar: {
    title: "Schedule and manage recurring work",
    summary:
      "See what's scheduled, run a job right now, or pause something that's acting up.",
    examples: [
      "Click 'Run now' next to any job to trigger it immediately.",
      "Toggle a job off if it's running too often or causing issues.",
      "Check 'Always Running' to see which background jobs are active.",
    ],
  },
  focus: {
    title: "Handle approvals and fix problems",
    summary:
      "When something needs your attention \u2014 an approval, a broken connection, a tripped breaker \u2014 it shows up here.",
    examples: [
      "Click Approve or Deny on pending actions that need your sign-off.",
      "Hit Reconnect next to any channel that went offline.",
      "Check the severity column to handle the most urgent items first.",
    ],
  },
  events: {
    title: "Event log (advanced)",
    summary:
      "A live stream of everything happening in the system. Useful when debugging, not needed day-to-day.",
    examples: [
      "Use the filters at the top to show only the event type you care about.",
      "Click '< JSON' on any event to see the full details.",
      "If something went wrong, check here to see exactly what happened and when.",
    ],
  },
  mail: {
    title: "Send direct messages",
    summary:
      "Private threads between you and your agents. Like email, but inside Mission Control.",
    examples: [
      "Click '+ New Thread' to start a direct conversation.",
      "Attach a file and ask your agent to review or process it.",
      "Use Mail for 1-on-1 handoffs. Use Rooms instead for group conversations.",
    ],
  },
  chatrooms: {
    title: "Group chat with your team",
    summary:
      "Rooms are for conversations with multiple people or agents at once. Think group chat or a war room.",
    examples: [
      "Click '+ New Room' and add the agents or people you need.",
      "Use rooms when you need multiple agents to see the same conversation.",
      "Unlike Mail (which is private 1-on-1), Rooms are shared spaces.",
    ],
  },
  assistant: {
    title: "Chat with an AI agent",
    summary:
      "Pick an agent, pick a model, type a message, hit Send. That's it.",
    examples: [
      "Select your agent from the dropdown, choose a provider and model, then type your question.",
      "Click 'New Chat' to start fresh with a clean conversation.",
      "Want to save the response? Use 'Send to Boards' to turn it into a task card.",
    ],
  },
  team: {
    title: "Set up your agents",
    summary:
      "Create agents and assign them an AI provider and model. This is where you decide who can do what.",
    examples: [
      "Click '+ New Agent' to create your first assistant.",
      "Pick a provider (like OpenAI or Anthropic) and a model for each agent.",
      "Agents need a provider and model before they can do anything useful.",
    ],
  },
  cockpit: {
    title: "Custom dashboards (advanced)",
    summary:
      "Build your own dashboard with widgets. Optional \u2014 you can skip this until you want a custom command view.",
    examples: [
      "Click 'Load Ops Template' to start with a pre-built dashboard instead of building from scratch.",
      "Add widgets for approvals, jobs, breaker status, or any data you want to watch.",
      "This is optional. Most users don't need a custom dashboard on day one.",
    ],
  },
  strategy: {
    title: "Plan and track work (advanced)",
    summary:
      "Organize goals, projects, and tasks. Useful for planning what your agents should work on, but not where they actually run.",
    examples: [
      "Create a goal, then break it into projects and tasks.",
      "Use the 'Blocked' and 'Stale' filters to find work that's stuck.",
      "Strategy plans the work. Boards and Calendar actually execute it.",
    ],
  },
  runbook: {
    title: "Execution debugger (advanced)",
    summary:
      "Shows exactly what happened during a run \u2014 every step, approval, and decision. Come here when you need to figure out why something went wrong.",
    examples: [
      "Select a run from the list to see its full step-by-step execution history.",
      "Check for failed steps, missing approvals, or unexpected blockers.",
      "Use the linked artifacts to jump to the Board card, job, or task that triggered the run.",
    ],
  },
  memory: {
    title: "Agent memory inspector (advanced)",
    summary:
      "See what your agent remembers. Each agent has its own separate memory. This is a specialist debugging tool.",
    examples: [
      "Pick an agent from the dropdown to see its stored memory cards.",
      "Browse the knowledge graph to see how facts are connected.",
      "Select a recent interaction and click 'Why' to understand the agent's reasoning.",
    ],
  },
  connectors: {
    title: "Tool registry (advanced)",
    summary:
      "Import external APIs and tools, review them, and assign them to your agents. This is the admin panel for managing what tools are available.",
    examples: [
      "Import an OpenAPI spec, review the operations, then publish the ones you want agents to use.",
      "Assign a published connector to specific agents in the 'Assignments' section.",
      "You only need this if you're adding custom API integrations beyond the built-in tools.",
    ],
  },
};

export function TabHelpBanner(props: TabHelpBannerProps) {
  const [examplesOpen, setExamplesOpen] = useState(false);
  const copy = HELP_COPY[props.tab];

  return (
    <section className="mc-tab-help-banner mc-surface" data-tour-id={`help-${props.tab}`}>
      <div className="mc-tab-help-head">
        <div className="mc-tab-help-main">
          <p className="mc-tab-help-kicker">
            <Lightbulb size={14} /> Quick Guide
          </p>
          <h3>{copy.title}</h3>
          <p>{copy.summary}</p>
        </div>
        <button
          type="button"
          className="mc-tab-help-dismiss"
          aria-label="Hide quick guides"
          title="Hide quick guides"
          onClick={props.onDismissAll}
        >
          <X size={16} />
        </button>
      </div>
      <div className="mc-tab-help-actions">
        <button
          type="button"
          className={`mc-tab-help-btn mc-tab-help-btn-examples${examplesOpen ? " is-active" : ""}`}
          aria-pressed={examplesOpen}
          onClick={() => setExamplesOpen((v) => !v)}
        >
          {examplesOpen ? "Hide examples" : "Show examples"}
        </button>
        <button type="button" className="mc-tab-help-btn mc-tab-help-btn-tour" onClick={props.onStartTour}>
          <Compass size={14} /> Tour
        </button>
        <button type="button" className="mc-tab-help-btn mc-tab-help-btn-docs" onClick={() => props.onOpenDocs(TAB_TO_HELP_SECTION[props.tab])}>
          <BookOpen size={14} /> Docs
        </button>
      </div>
      {examplesOpen ? (
        <ul className="mc-tab-help-examples">
          {copy.examples.map((example, index) => (
            <li key={`${props.tab}-${index}`}>{example}</li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
