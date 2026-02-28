import type {
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
} from "../../types";
import { formatDateTime } from "../../utils/datetime";
import { truncateText } from "../../utils/text";

export function buildThreadSummaryNote(
  detail: AgentMailThreadDetailResponse,
  messages: AgentMailMessageResponse[]
): string {
  const head = [
    `Thread: ${detail.thread.subject}`,
    `Kind: ${detail.thread.kind}`,
    `Participants: ${detail.participants.map((item) => item.principal_id).join(", ")}`,
    `Message count: ${messages.length}`,
    `Generated at: ${new Date().toISOString()}`,
  ];
  const timeline = messages
    .slice(-12)
    .map((message, index) => {
      const recipients = message.recipients
        .map((recipient) => recipient.recipient_principal)
        .join(", ");
      return `${index + 1}. [${formatDateTime(message.created_at)}] ${message.sender_principal} -> ${recipients || "thread"}\n${truncateText(message.body_text, 280)}`;
    });
  return `${head.join("\n")}\n\nRecent Timeline\n${timeline.join("\n\n")}`;
}
