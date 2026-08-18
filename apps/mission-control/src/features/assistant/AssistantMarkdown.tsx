import type { ReactNode } from "react";

function renderInlineMarkdown(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*\n]+\*\*|\*[^*\n]+\*)/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > cursor) {
      nodes.push(text.slice(cursor, match.index));
    }

    const token = match[0];
    const key = `${match.index}-${token.length}`;
    if (token.startsWith("`") && token.endsWith("`")) {
      nodes.push(<code key={key}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith("**") && token.endsWith("**")) {
      nodes.push(<strong key={key}>{token.slice(2, -2)}</strong>);
    } else if (token.startsWith("*") && token.endsWith("*")) {
      nodes.push(<em key={key}>{token.slice(1, -1)}</em>);
    } else {
      nodes.push(token);
    }

    cursor = match.index + token.length;
  }

  if (cursor < text.length) {
    nodes.push(text.slice(cursor));
  }

  return nodes.length > 0 ? nodes : [text];
}

function collectParagraph(lines: string[], start: number): { text: string; next: number } {
  const paragraphLines: string[] = [];
  let cursor = start;
  while (cursor < lines.length) {
    const line = lines[cursor] ?? "";
    if (
      line.trim().length === 0 ||
      line.trim().startsWith("```") ||
      /^\s*[-*]\s+/.test(line) ||
      /^\s*\d+[.)]\s+/.test(line) ||
      /^#{1,4}\s+/.test(line)
    ) {
      break;
    }
    paragraphLines.push(line.trim());
    cursor += 1;
  }
  return { text: paragraphLines.join(" "), next: cursor };
}

export function AssistantMarkdown({ content }: { content: string }) {
  const lines = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  let cursor = 0;

  while (cursor < lines.length) {
    const line = lines[cursor] ?? "";
    const trimmed = line.trim();
    if (!trimmed) {
      cursor += 1;
      continue;
    }

    if (trimmed.startsWith("```")) {
      const codeLines: string[] = [];
      cursor += 1;
      while (cursor < lines.length && !(lines[cursor] ?? "").trim().startsWith("```")) {
        codeLines.push(lines[cursor] ?? "");
        cursor += 1;
      }
      if (cursor < lines.length) {
        cursor += 1;
      }
      blocks.push(
        <pre key={`code-${cursor}`}>
          <code>{codeLines.join("\n")}</code>
        </pre>
      );
      continue;
    }

    const heading = /^(#{1,4})\s+(.+)$/.exec(trimmed);
    if (heading) {
      blocks.push(<h4 key={`heading-${cursor}`}>{renderInlineMarkdown(heading[2])}</h4>);
      cursor += 1;
      continue;
    }

    const bullet = /^\s*[-*]\s+(.+)$/.exec(line);
    if (bullet) {
      const items: string[] = [];
      while (cursor < lines.length) {
        const item = /^\s*[-*]\s+(.+)$/.exec(lines[cursor] ?? "");
        if (!item) {
          break;
        }
        items.push(item[1]);
        cursor += 1;
      }
      blocks.push(
        <ul key={`ul-${cursor}`}>
          {items.map((item, index) => (
            <li key={`${index}-${item}`}>{renderInlineMarkdown(item)}</li>
          ))}
        </ul>
      );
      continue;
    }

    const ordered = /^\s*\d+[.)]\s+(.+)$/.exec(line);
    if (ordered) {
      const items: string[] = [];
      while (cursor < lines.length) {
        const item = /^\s*\d+[.)]\s+(.+)$/.exec(lines[cursor] ?? "");
        if (!item) {
          break;
        }
        items.push(item[1]);
        cursor += 1;
      }
      blocks.push(
        <ol key={`ol-${cursor}`}>
          {items.map((item, index) => (
            <li key={`${index}-${item}`}>{renderInlineMarkdown(item)}</li>
          ))}
        </ol>
      );
      continue;
    }

    const paragraph = collectParagraph(lines, cursor);
    blocks.push(<p key={`p-${cursor}`}>{renderInlineMarkdown(paragraph.text)}</p>);
    cursor = paragraph.next;
  }

  return <div className="mc-assistant-markdown">{blocks}</div>;
}
