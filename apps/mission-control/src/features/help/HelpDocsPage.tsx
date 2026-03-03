import { BookOpen, Compass, ExternalLink } from "lucide-react";
import type { MissionControlTab } from "../../app/useAppController";

interface HelpDocsPageProps {
  onOpenTab: (tab: MissionControlTab) => void;
  onStartTour: () => void;
}

interface HelpSection {
  tab: Exclude<MissionControlTab, "help">;
  title: string;
  whatItDoes: string;
  goodFor: string[];
  caution: string;
}

const HELP_SECTIONS: HelpSection[] = [
  {
    tab: "boards",
    title: "Boards",
    whatItDoes: "Task execution surface. Cards hold scope, ownership, and run context.",
    goodFor: [
      "Running one task now",
      "Attaching files and scripts to a task",
      "Tracking status by column",
    ],
    caution: "A card only executes when you click Run Card.",
  },
  {
    tab: "calendar",
    title: "Calendar",
    whatItDoes: "Scheduler operations and time-based job oversight.",
    goodFor: [
      "Run a job now",
      "Pause/resume recurring jobs",
      "View always-running loops",
    ],
    caution: "Disabled jobs will not auto-run until re-enabled.",
  },
  {
    tab: "focus",
    title: "Focus",
    whatItDoes: "Triage queue for approvals, blockers, and runtime incidents.",
    goodFor: [
      "Clearing approvals",
      "Responding to breaker events",
      "Channel reconnect operations",
    ],
    caution: "Unresolved high-severity items can block downstream automation.",
  },
  {
    tab: "events",
    title: "Events",
    whatItDoes: "Live event stream for diagnostics and behavior tracing.",
    goodFor: [
      "Debugging run lifecycle",
      "Inspecting system decisions",
      "Auditing operation sequence",
    ],
    caution: "Raw mode can be noisy during high activity.",
  },
  {
    tab: "mail",
    title: "Mail",
    whatItDoes: "Structured direct thread messaging with attachments and acknowledgements.",
    goodFor: [
      "1:1 or small group handoffs",
      "Persistent discussion records",
      "Attachment-based review flow",
    ],
    caution: "Mail sends messages; it does not auto-run model completions.",
  },
  {
    tab: "chatrooms",
    title: "Rooms",
    whatItDoes: "Group coordination threads for broad collaboration.",
    goodFor: [
      "War-room style coordination",
      "Multi-party context sharing",
      "Thread-level workspace lease workflows",
    ],
    caution: "Rooms are communication channels, not direct LLM chat windows.",
  },
  {
    tab: "assistant",
    title: "Assistant",
    whatItDoes: "Direct conversational chat UI for session/message/run model loops.",
    goodFor: [
      "Ask for planning or synthesis in plain language",
      "Use one model as assistant and another for orchestration experiments",
      "Apply an explicit core system prompt for product-aware behavior",
    ],
    caution: "Each send triggers a run against your selected provider/model; watch budget policy.",
  },
  {
    tab: "team",
    title: "Team",
    whatItDoes: "Agent roster and provider/model assignment control.",
    goodFor: [
      "Setting provider/model per agent",
      "Creating orchestrator workers",
      "Changing tool profiles",
    ],
    caution: "Agents without provider/model will appear but cannot run useful work.",
  },
  {
    tab: "cockpit",
    title: "Cockpit",
    whatItDoes: "Custom dashboard builder for operational command views.",
    goodFor: [
      "Executive overviews",
      "Incident runbooks",
      "Pinned KPI and queue widgets",
    ],
    caution: "Cockpit visualizes control data; it is not the primary editing surface.",
  },
];

export function HelpDocsPage(props: HelpDocsPageProps) {
  return (
    <section className="mc-help-page" data-tour-id="help-page">
      <article className="mc-surface mc-help-hero">
        <p className="mc-help-kicker">Mission Control Docs</p>
        <h2>Operator Guide</h2>
        <p>
          Use this page as the in-app knowledge base. Each section links directly to the tab it
          describes.
        </p>
        <div className="mc-help-hero-actions">
          <button type="button" onClick={props.onStartTour}>
            <Compass size={14} /> Start Guided Tour
          </button>
          <button type="button" className="ghost" onClick={() => props.onOpenTab("boards")}>
            <ExternalLink size={14} /> Open Boards
          </button>
        </div>
      </article>

      <div className="mc-help-grid">
        {HELP_SECTIONS.map((section) => (
          <article key={section.tab} className="mc-surface mc-help-card">
            <header>
              <h3>{section.title}</h3>
              <button type="button" className="ghost" onClick={() => props.onOpenTab(section.tab)}>
                <BookOpen size={14} /> Open
              </button>
            </header>
            <p>{section.whatItDoes}</p>
            <ul>
              {section.goodFor.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
            <p className="mc-help-caution">{section.caution}</p>
          </article>
        ))}
      </div>
    </section>
  );
}
