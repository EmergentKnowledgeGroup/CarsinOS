import { useState } from "react";
import { BookOpen, Compass, Lightbulb } from "lucide-react";
import type { MissionControlTab } from "./useAppController";

type HelpTab = Exclude<MissionControlTab, "help">;

interface TabHelpBannerProps {
  tab: HelpTab;
  onOpenDocs: () => void;
  onStartTour: () => void;
}

interface HelpCopy {
  title: string;
  summary: string;
  examples: string[];
}

const HELP_COPY: Record<HelpTab, HelpCopy> = {
  boards: {
    title: "Boards are your execution lane",
    summary:
      "Capture tasks as cards, attach context, and run cards to execute agent work on demand.",
    examples: [
      "Create card: 'Draft launch checklist', assign an owner, click Run Card.",
      "Attach a spec file to a card so the run has grounded context.",
      "Use due dates and tags to batch similar tasks for one sprint.",
    ],
  },
  calendar: {
    title: "Calendar is your scheduler",
    summary:
      "View jobs by time, trigger run-now, and toggle automations without leaving Mission Control.",
    examples: [
      "Run a weekly report job instantly from Run now.",
      "Disable a noisy job while debugging provider configuration.",
      "Check Always Running jobs for core operational loops.",
    ],
  },
  focus: {
    title: "Focus is your risk queue",
    summary:
      "Approvals, breakers, and channel issues surface here first so you can resolve blockers fast.",
    examples: [
      "Approve or deny pending high-risk tool actions.",
      "Reconnect a down channel runtime from one place.",
      "Review top severity items before starting daily execution.",
    ],
  },
  events: {
    title: "Events are your timeline",
    summary:
      "Inspect live event flow for observability and troubleshooting across runs, jobs, and tools.",
    examples: [
      "Filter for run.* to isolate model execution behavior.",
      "Switch raw mode on when debugging payload-level details.",
      "Watch approval and breaker transitions in real time.",
    ],
  },
  mail: {
    title: "Mail is structured direct messaging",
    summary:
      "Use direct threads for traceable communication, acknowledgements, and attachment handoffs.",
    examples: [
      "Create a direct thread with one or more recipients.",
      "Attach a document, then ask a specific reviewer for feedback.",
      "Acknowledge unread items to clear response backlog.",
    ],
  },
  chatrooms: {
    title: "Rooms are group coordination threads",
    summary:
      "Use rooms for multi-party coordination, shared files, and persistent collaboration history.",
    examples: [
      "Create room 'release-war-room' with agents + human operators.",
      "Post a status update and reserve workspace lease for edits.",
      "Use reactions to mark quick consensus before approval actions.",
    ],
  },
  assistant: {
    title: "Assistant is direct chat execution",
    summary:
      "Use this tab for prompt/response chat with a selected agent, model, and optional core system prompt.",
    examples: [
      "Ask the assistant to draft a plan, then move execution tasks to Boards.",
      "Use a different model for orchestrator-style reasoning in one session.",
      "Start a fresh chat when you want a clean context window.",
    ],
  },
  team: {
    title: "Team defines who can run what",
    summary:
      "Configure agent provider/model pairs and tool profiles so execution has explicit ownership.",
    examples: [
      "Set an agent to ollama + a local model for cost-controlled runs.",
      "Create a dedicated orchestrator agent separate from assistant.",
      "Use restricted tool profile for high-risk environments.",
    ],
  },
  cockpit: {
    title: "Cockpit is your custom operations view",
    summary:
      "Build dashboards using widgets, runtime data sources, and incident-focused layouts.",
    examples: [
      "Pin approvals, breaker status, and jobs on one page.",
      "Create a custom widget for a specific API data source path.",
      "Switch to incident mode for tighter operational focus.",
    ],
  },
};

export function TabHelpBanner(props: TabHelpBannerProps) {
  const [examplesOpen, setExamplesOpen] = useState(false);
  const copy = HELP_COPY[props.tab];

  return (
    <section className="mc-tab-help-banner mc-surface" data-tour-id={`help-${props.tab}`}>
      <div className="mc-tab-help-main">
        <p className="mc-tab-help-kicker">
          <Lightbulb size={14} /> Quick Guide
        </p>
        <h3>{copy.title}</h3>
        <p>{copy.summary}</p>
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
        <button type="button" className="mc-tab-help-btn mc-tab-help-btn-docs" onClick={props.onOpenDocs}>
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
