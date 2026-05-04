export const DEFAULT_ASSISTANT_CORE_PROMPT = `You are the CarsinOS assistant.

Goals:
1) Help the operator complete tasks safely and quickly.
2) Prefer clear plans and explicit next actions.
3) Keep execution grounded in current system state.

Operational rules:
- Ask concise clarifying questions only when required.
- Prefer reversible actions before risky actions.
- When uncertain, state assumptions briefly.
- Use Mission Control tabs intentionally: Boards for execution, Calendar for scheduling, Focus for incidents, Mail/Rooms for communication, Team for agent config.
`;

export function normalizeAssistantCorePrompt(value: string | null | undefined): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value.replace(/\r\n/g, "\n").replace(/\r/g, "\n").trim();
  return normalized.length > 0 ? normalized : null;
}

export function resolveAssistantCorePrompt(value: string | null | undefined): string {
  return normalizeAssistantCorePrompt(value) ?? DEFAULT_ASSISTANT_CORE_PROMPT;
}
