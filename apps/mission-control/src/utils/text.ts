export function parsePrincipalCsv(raw: string): string[] {
  return [...new Set(raw.split(",").map((value) => value.trim()).filter(Boolean))];
}

export function truncateText(value: string, maxChars: number): string {
  const trimmed = value.trim();
  if (trimmed.length <= maxChars) {
    return trimmed;
  }
  if (maxChars <= 3) {
    return trimmed.slice(0, maxChars);
  }
  return `${trimmed.slice(0, maxChars - 3)}...`;
}
