/** Colored circle with sender initials. */
const AVATAR_COLORS = [
  "#e06c6c", "#e0a06e", "#c9c45a", "#6ec56e",
  "#5ab8b8", "#6e8ce0", "#a06ee0", "#d66ea0",
];

function hashCode(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = ((hash << 5) - hash + str.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

function extractInitials(name: string): string {
  const parts = name.split(/[\s._-]+/).filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return (name.slice(0, 2) || "??").toUpperCase();
}

interface AvatarProps {
  name: string;
  size?: number;
  decorative?: boolean;
}

export function Avatar({ name, size = 24, decorative = true }: AvatarProps) {
  const bg = AVATAR_COLORS[hashCode(name) % AVATAR_COLORS.length];
  const initials = extractInitials(name);
  return (
    <span
      className="mc-avatar"
      style={{
        width: size,
        height: size,
        borderRadius: "50%",
        background: bg,
        color: "#fff",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: size * 0.42,
        fontWeight: 600,
        lineHeight: 1,
        flexShrink: 0,
        letterSpacing: "0.02em",
      }}
      title={name}
      aria-hidden={decorative ? true : undefined}
      aria-label={decorative ? undefined : `${name} avatar`}
      role={decorative ? undefined : "img"}
    >
      {initials}
    </span>
  );
}
