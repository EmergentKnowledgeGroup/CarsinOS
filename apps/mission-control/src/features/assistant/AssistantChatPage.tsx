import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Info, MessageSquareText } from "lucide-react";
import type { MissionControlTab } from "../../app/useAppController";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import type { Agent, RunbookSummaryItemResponse } from "../../types";
import type { RuntimeConnectionSettings } from "../../types";
import { ExecassOfficePanel } from "../execassOffice/ExecassOfficePanel";
import type { ExecassOfficeController } from "../execassOffice/useExecassOfficeController";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import { AssistantMarkdown } from "./AssistantMarkdown";
import type { useAssistantChatController } from "./useAssistantChatController";

interface AssistantChatPageProps {
  active: boolean;
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
  boards: BoardSummary[];
  onTabChange: (tab: MissionControlTab) => void;
  /** Navigate to a floor room by stable id (pinned room shortcuts). */
  onOpenRoom: (roomId: string) => void;
  controller: ReturnType<typeof useAssistantChatController>;
  officeController: ExecassOfficeController;
  runbookEnabled: boolean;
  runbookSummary: RunbookSummaryItemResponse | null;
  onOpenAssistantRunbook: (runId: string) => boolean;
}

function formatTimestamp(unixMs: number): string {
  try {
    return new Date(unixMs).toLocaleString();
  } catch {
    return "";
  }
}

function FieldHelpIcon({ text }: { text: string }) {
  return (
    <span className="mc-assistant-info" title={text} aria-label={text} role="img">
      <Info size={13} />
    </span>
  );
}

export function AssistantChatPage(props: AssistantChatPageProps) {
  const c = props.controller;
  const assistantRunId = c.lastRunId;
  const [promptOpen, setPromptOpen] = useState(false);
  const [activePanel, setActivePanel] = useState<"chat" | "prompt">("chat");
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const refreshRoutingState = c.refreshRoutingState;
  const messageCount = c.messages.length;
  const sendStatus = c.sendStatus;

  useEffect(() => {
    if (!props.active) {
      return;
    }
    void refreshRoutingState();
  }, [props.active, refreshRoutingState]);

  useEffect(() => {
    if (!props.active || activePanel !== "chat") {
      return;
    }
    const transcript = transcriptRef.current;
    if (!transcript) {
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      transcript.scrollTop = transcript.scrollHeight;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activePanel, messageCount, props.active, sendStatus]);

  if (props.agents.length === 0) {
    return (
      <section className="mc-assistant-page" data-tour-id="assistant-page">
        <article className="mc-surface mc-assistant-empty-state">
          <header>
            <h2>Create an agent first</h2>
            <p>
              Assistant chat needs one configured agent before it can route a message anywhere.
            </p>
          </header>
          <div className="mc-empty-drawer">
            No agents are ready yet. Go to Team, create one agent, attach a provider, then come back here to chat.
          </div>
          <div className="mc-assistant-empty-actions">
            <button type="button" onClick={() => props.onTabChange("team")}>
              Go to Team
            </button>
            <button type="button" className="ghost" onClick={() => props.onTabChange("help")}>
              Open Help
            </button>
          </div>
        </article>
      </section>
    );
  }

  if (c.runtimeRoutingLoaded && c.availableAgents.length === 0) {
    return (
      <section className="mc-assistant-page" data-tour-id="assistant-page">
        <article className="mc-surface mc-assistant-empty-state">
          <header>
            <h2>Route one assistant to yourself first</h2>
            <p>Assistant chat only shows the assistants assigned to the local operator.</p>
          </header>
          <div className="mc-empty-drawer">{c.assistantAvailabilityMessage}</div>
          <div className="mc-assistant-empty-actions">
            <button type="button" onClick={() => props.onTabChange("team")}>
              Go to Team
            </button>
            <button type="button" className="ghost" onClick={() => props.onTabChange("help")}>
              Open Help
            </button>
          </div>
        </article>
      </section>
    );
  }

  return (
    <section className="mc-assistant-page" data-tour-id="assistant-page">
      <article className="mc-surface mc-assistant-toolbar">
        <div className="mc-assistant-toolbar-grid">
          <label>
            <span className="mc-assistant-field-label">
              Assistant agent
              <FieldHelpIcon text={c.assistantAvailabilityMessage} />
            </span>
            <select
              value={c.selectedAgentId}
              onChange={(event) => c.setSelectedAgentId(event.target.value)}
            >
              {c.availableAgents.length === 0 ? (
                <option value="">No assistants routed to you</option>
              ) : null}
              {c.availableAgents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name || agent.agent_id}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span className="mc-assistant-field-label">
              Provider
              <FieldHelpIcon text="Lane routing is authoritative here. Change provider in Team, not per chat run." />
            </span>
            <select
              aria-label="Assistant provider"
              value={c.modelProvider}
              onChange={(event) => {
                c.setModelProvider(event.target.value);
                c.setAuthProfileId("");
                c.setModelId("");
              }}
              disabled
            >
              <option value="">Choose provider...</option>
              {c.providerOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span className="mc-assistant-field-label">
              Login
              <FieldHelpIcon text="Assistant chat follows the assistant's saved routing and login path automatically." />
            </span>
            <select
              aria-label="Assistant login"
              value={c.authProfileId}
              onChange={(event) => {
                c.setAuthProfileId(event.target.value);
                c.setModelId("");
              }}
              disabled
            >
              <option value="">Auto (use agent routing)</option>
              {c.availableAuthProfiles.map((profile) => (
                <option key={profile.auth_profile_id} value={profile.auth_profile_id}>
                  {profile.display_name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span className="mc-assistant-field-label">
              Model
              <FieldHelpIcon text="carsinOS pulls the live model list in Team, and this chat stays locked to the selected assistant's saved route." />
            </span>
            <select
              aria-label="Assistant model"
              value={c.modelId}
              onChange={(event) => c.setModelId(event.target.value)}
              disabled
            >
              <option value="">
                {c.catalogLoading ? "Loading models..." : "Choose model..."}
              </option>
              {c.catalogModelOptions.map((modelId) => (
                <option key={modelId} value={modelId}>
                  {modelId}
                </option>
              ))}
            </select>
          </label>
        </div>
        {c.runtimeRoutingError ||
        c.providerCapabilitiesError ||
        c.catalogError ||
        c.providerCapabilitiesLoading ||
        c.catalogLoading ? (
          <div className="mc-assistant-toolbar-alert" role="status">
            {c.runtimeRoutingError ? (
              <span>Team route refresh failed. Showing the last known local route.</span>
            ) : null}
            {c.providerCapabilitiesError ? (
              <span>Provider details did not refresh cleanly.</span>
            ) : null}
            {c.catalogError ? <span>Model details did not refresh cleanly.</span> : null}
            {c.providerCapabilitiesLoading ? <span>Loading provider choices...</span> : null}
            {c.catalogLoading ? <span>Loading model list...</span> : null}
          </div>
        ) : null}
        <div className="mc-assistant-toolbar-actions">
          <span
            className="chip mc-assistant-route-chip"
            title="Provider, login, and model come from Team setup."
          >
            Team route locked
          </span>
          {c.sessionMode === "pinned_session" ? (
            <button type="button" className="ghost" onClick={c.resetToCanonicalLane} disabled={c.busy}>
              Return to my lane
            </button>
          ) : null}
          <label>
            Send reply to board
            <select
              value={c.targetBoardId}
              onChange={(event) => c.setTargetBoardId(event.target.value)}
              disabled={props.boards.length === 0 || c.busy}
            >
              {props.boards.length === 0 ? <option value="">No boards available</option> : null}
              {props.boards.map((board) => (
                <option key={board.board_id} value={board.board_id}>
                  {board.name}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="ghost"
            onClick={async () => {
              const ok = await c.sendLastAssistantToBoard();
              if (ok) {
                props.onTabChange("boards");
              }
            }}
            disabled={!c.lastAssistantMessage || c.busy || !c.targetBoardId}
          >
            Send to Boards
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() =>
              void (c.corePromptDirty ? c.saveCorePrompt() : c.injectCorePrompt())
            }
            disabled={c.busy || c.corePromptLoading || c.corePromptSaving}
          >
            {c.corePromptDirty ? "Save Shared Prompt" : "Inject Core Prompt"}
          </button>
          <span className="chip">
            {c.sessionMode === "pinned_session" ? "view: pinned transcript" : "view: live lane"}
          </span>
          {c.sessionId ? <span className="chip" title={c.sessionId}>session: {c.sessionId.slice(0, 8)}</span> : null}
          {c.lastRunId ? <span className="chip" title={c.lastRunId}>run: {c.lastRunId.slice(0, 8)}</span> : null}
          {c.lastRunStatus ? <span className="chip">run: {c.lastRunStatus}</span> : null}
        </div>
      </article>

      <ExecassOfficePanel
        controller={props.officeController}
        onOpenRoom={props.onOpenRoom}
      />

      <div className="mc-page-section-tabs" aria-label="Assistant sections">
        <button
          type="button"
          className={`mc-page-section-btn${activePanel === "chat" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActivePanel("chat")}
        >
          Chat
        </button>
        <button
          type="button"
          className={`mc-page-section-btn${activePanel === "prompt" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActivePanel("prompt")}
        >
          Shared Prompt
        </button>
      </div>

      {activePanel === "prompt" ? (
        <article className="mc-surface mc-assistant-prompt mc-assistant-main-panel">
          <header>
            <button
              type="button"
              className="mc-assistant-prompt-toggle"
              onClick={() => setPromptOpen(!promptOpen)}
              aria-expanded={promptOpen}
            >
              {promptOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
              <h3>System Prompt</h3>
              <span className="mc-assistant-prompt-hint">
                {promptOpen ? "" : "(optional \u2014 click to expand)"}
              </span>
            </button>
          </header>
          {promptOpen ? (
            <>
              <p className="mc-assistant-prompt-desc">
                This is the shared carsinOS system prompt. Assistant chat, Telegram, and Discord
                all fall back to it unless a session already has its own system prompt.
              </p>
              <textarea
                value={c.corePrompt}
                onChange={(event) => c.setCorePrompt(event.target.value)}
                rows={16}
                placeholder="Describe how the agent should behave, what it should prioritize, and any constraints."
              />
              <div className="mc-assistant-prompt-actions">
                <button
                  type="button"
                  onClick={() => void c.saveCorePrompt()}
                  disabled={c.corePromptLoading || c.corePromptSaving || !c.corePromptDirty}
                >
                  {c.corePromptSaving ? "Saving prompt..." : "Save shared prompt"}
                </button>
                <button
                  type="button"
                  className="ghost"
                  onClick={c.resetCorePrompt}
                  disabled={c.corePromptLoading || c.corePromptSaving || !c.corePromptDirty}
                >
                  Reset changes
                </button>
                <button
                  type="button"
                  className="ghost"
                  onClick={c.restoreDefaultCorePrompt}
                  disabled={c.corePromptLoading || c.corePromptSaving}
                >
                  Use built-in default
                </button>
              </div>
              {c.corePromptError ? (
                <p className="mc-form-error">
                  Shared prompt settings could not load cleanly: {c.corePromptError}
                </p>
              ) : null}
              <p className="mc-assistant-prompt-status">
                {c.corePromptDirty
                  ? "You have unsaved prompt changes. Save them to make new chats and channel runs use this version."
                  : "Shared prompt saved. Assistant, Discord, and Telegram all use it automatically on new runs."}
              </p>
            </>
          ) : null}
        </article>
      ) : (
        <article className="mc-surface mc-assistant-chat mc-assistant-main-panel">
          <header>
            <h3>Chat</h3>
            <p>
              Type a message and hit Send. Each message triggers one AI response.
            </p>
          </header>

          <div className="mc-assistant-transcript" ref={transcriptRef}>
            {c.messages.length === 0 ? (
              <div className="mc-assistant-chat-empty">
                <MessageSquareText size={24} />
                <strong>Ready when you are.</strong>
                <span>Type a request below. Replies will appear here as the live lane transcript.</span>
              </div>
            ) : (
              c.messages.map((message) => (
                <article key={message.message_id} className={`mc-assistant-msg mc-assistant-msg-${message.role}`}>
                  <div className="mc-assistant-msg-meta">
                    <strong>{message.role}</strong>
                    <span>{formatTimestamp(message.created_at)}</span>
                  </div>
                  <AssistantMarkdown content={message.content_text} />
                </article>
              ))
            )}
            {c.sendStatus ? (
              <div className="mc-assistant-send-status" role="status" aria-live="polite">
                <span />
                {c.sendStatus}
              </div>
            ) : null}
          </div>

          {c.lastError ? <p className="mc-form-error">{c.lastError}</p> : null}
          {props.runbookEnabled ? (
            <RunbookLinkPanel
              className="mc-assistant-runbook-panel"
              summary={props.runbookSummary}
              emptyMessage={null}
              onOpen={
                assistantRunId
                  ? () => props.onOpenAssistantRunbook(assistantRunId)
                  : undefined
              }
            />
          ) : null}

          <div className="mc-assistant-compose">
            <textarea
              value={c.draft}
              onChange={(event) => c.setDraft(event.target.value)}
              rows={4}
              placeholder="Type your request..."
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                  event.preventDefault();
                  if (!c.busy) {
                    void c.send();
                  }
                }
              }}
            />
            <button type="button" className="primary" onClick={() => void c.send()} disabled={c.busy || !c.draft.trim()}>
              {c.busy ? "Running..." : "Send"}
            </button>
            <kbd className="mc-shortcut-hint">{"\u2318\u21A9"}</kbd>
          </div>
        </article>
      )}
    </section>
  );
}
