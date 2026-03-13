import type { MissionControlTab } from "../../app/useAppController";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import type { Agent, RunbookSummaryItemResponse } from "../../types";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import type { useAssistantChatController } from "./useAssistantChatController";

interface AssistantChatPageProps {
  agents: Agent[];
  boards: BoardSummary[];
  onTabChange: (tab: MissionControlTab) => void;
  controller: ReturnType<typeof useAssistantChatController>;
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

export function AssistantChatPage(props: AssistantChatPageProps) {
  const c = props.controller;
  const assistantRunId = c.lastRunId;

  return (
    <section className="mc-assistant-page" data-tour-id="assistant-page">
      <article className="mc-surface mc-assistant-toolbar">
        <div className="mc-assistant-toolbar-grid">
          <label>
            Assistant agent
            <select
              value={c.selectedAgentId}
              onChange={(event) => c.setSelectedAgentId(event.target.value)}
            >
              {props.agents.length === 0 ? <option value="">No agents configured</option> : null}
              {props.agents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name || agent.agent_id}
                </option>
              ))}
            </select>
          </label>
          <label>
            Model provider
            <input
              value={c.modelProvider}
              onChange={(event) => c.setModelProvider(event.target.value)}
              placeholder="ollama"
            />
          </label>
          <label>
            Model ID
            <input
              value={c.modelId}
              onChange={(event) => c.setModelId(event.target.value)}
              placeholder="qwen3.5-9b"
            />
          </label>
          <label>
            Auth profile (optional)
            <input
              value={c.authProfileId}
              onChange={(event) => c.setAuthProfileId(event.target.value)}
              placeholder="auth-profile-id"
            />
          </label>
        </div>
        <div className="mc-assistant-toolbar-actions">
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
          <button type="button" className="ghost" onClick={c.startNewChat} disabled={c.busy}>
            New Chat
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => void c.injectCorePrompt()}
            disabled={c.busy}
          >
            Insert Core Prompt
          </button>
          {c.sessionId ? <span className="chip" title={c.sessionId}>session: {c.sessionId.slice(0, 8)}</span> : null}
          {c.lastRunId ? <span className="chip" title={c.lastRunId}>run: {c.lastRunId.slice(0, 8)}</span> : null}
          {c.lastRunStatus ? <span className="chip">run: {c.lastRunStatus}</span> : null}
        </div>
      </article>

      <div className="mc-assistant-grid">
        <article className="mc-surface mc-assistant-prompt">
          <header>
            <h3>Core System Prompt</h3>
            <p>Inserted at session start, can be re-inserted any time.</p>
          </header>
          <textarea
            value={c.corePrompt}
            onChange={(event) => c.setCorePrompt(event.target.value)}
            rows={16}
            placeholder="Describe system behavior, priorities, and tool usage expectations."
          />
        </article>

        <article className="mc-surface mc-assistant-chat">
          <header>
            <h3>Assistant Chat</h3>
            <p>
              Send a message to execute one run. This is direct chat, separate from Mail/Rooms.
            </p>
          </header>

          <div className="mc-assistant-transcript">
            {c.messages.length === 0 ? (
              <div className="mc-empty-drawer">No chat messages yet. Send your first prompt.</div>
            ) : (
              c.messages.map((message) => (
                <article key={message.message_id} className={`mc-assistant-msg mc-assistant-msg-${message.role}`}>
                  <div className="mc-assistant-msg-meta">
                    <strong>{message.role}</strong>
                    <span>{formatTimestamp(message.created_at)}</span>
                  </div>
                  <p>{message.content_text}</p>
                </article>
              ))
            )}
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
            <button type="button" onClick={() => void c.send()} disabled={c.busy || !c.draft.trim()}>
              {c.busy ? "Running..." : "Send"}
            </button>
            <kbd className="mc-shortcut-hint">{"\u2318\u21A9"}</kbd>
          </div>
        </article>
      </div>
    </section>
  );
}
