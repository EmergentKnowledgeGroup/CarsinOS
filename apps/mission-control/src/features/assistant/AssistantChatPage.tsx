import type { MissionControlTab } from "../../app/useAppController";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import type { Agent } from "../../types";
import type { useAssistantChatController } from "./useAssistantChatController";

interface AssistantChatPageProps {
  agents: Agent[];
  boards: BoardSummary[];
  onTabChange: (tab: MissionControlTab) => void;
  controller: ReturnType<typeof useAssistantChatController>;
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
            <select
              value={c.modelProvider}
              onChange={(event) => c.setModelProvider(event.target.value)}
              disabled={c.providerOptions.length === 0}
            >
              {c.providerOptions.length === 0 ? (
                <option value="">No providers discovered</option>
              ) : null}
              {c.providerOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Model ID
            <select
              value={c.modelId}
              onChange={(event) => c.setModelId(event.target.value)}
              disabled={c.modelsLoading || c.modelOptions.length === 0}
            >
              {c.modelsLoading ? <option value="">Loading models...</option> : null}
              {!c.modelsLoading && c.modelOptions.length === 0 ? (
                <option value="">No models discovered</option>
              ) : null}
              {c.modelOptions.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          </label>
          <label>
            Auth profile (optional)
            <select
              value={c.authProfileId}
              onChange={(event) => c.setAuthProfileId(event.target.value)}
            >
              <option value="">Automatic / none</option>
              {c.authProfileOptions.map((profile) => (
                <option key={profile.auth_profile_id} value={profile.auth_profile_id}>
                  {profile.display_name}
                </option>
              ))}
            </select>
            {c.authProfileOptions.length > 0 ? (
              <small>Choose and save to keep this provider profile pinned for the selected agent.</small>
            ) : null}
          </label>
        </div>
        {c.modelsError ? <p className="mc-form-error">{c.modelsError}</p> : null}
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
          <button
            type="button"
            className="ghost"
            onClick={() => void c.saveAuthProfileSelection()}
            disabled={
              c.busy ||
              c.savingAuthProfileRoute ||
              !c.selectedAgentId.trim() ||
              !c.modelProvider.trim() ||
              !c.authProfileId.trim()
            }
          >
            {c.savingAuthProfileRoute ? "Saving Profile..." : "Save Profile Routing"}
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => void c.clearAllAssistantProfiles()}
            disabled={c.busy || c.savingAuthProfileRoute || c.clearingAllProfiles}
            title="Revoke and disable all auth profiles so you can re-auth from scratch"
          >
            {c.clearingAllProfiles ? "Clearing Profiles..." : "Clear All Profiles"}
          </button>
          {c.sessionId ? <span className="chip">session: {c.sessionId}</span> : null}
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
          </div>
        </article>
      </div>
    </section>
  );
}
