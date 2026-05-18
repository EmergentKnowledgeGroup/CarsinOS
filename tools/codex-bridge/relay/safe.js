const fs = require("node:fs");
const path = require("node:path");

function normalizeForCompare(value) {
  const resolved = path.resolve(value);
  return process.platform === "win32" ? resolved.toLowerCase() : resolved;
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function safeId(value, label = "id") {
  const raw = String(value || "").trim();
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,79}$/.test(raw)) {
    throw new Error(`${label} must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,79}$`);
  }
  return raw;
}

function safeJoin(root, ...parts) {
  const resolvedRoot = path.resolve(root);
  const resolved = path.resolve(resolvedRoot, ...parts);
  const rootKey = normalizeForCompare(resolvedRoot);
  const resolvedKey = normalizeForCompare(resolved);
  if (resolvedKey !== rootKey && !resolvedKey.startsWith(`${rootKey}${path.sep}`)) {
    throw new Error("path escaped bridge root");
  }
  return resolved;
}

function splitPathList(value) {
  return String(value || "")
    .split(";")
    .map((item) => item.trim())
    .filter(Boolean);
}

function assertAllowedRoot(target, allowedRoots) {
  const resolved = path.resolve(target);
  const targetKey = normalizeForCompare(resolved);
  const ok = allowedRoots.some((root) => {
    const rootKey = normalizeForCompare(root);
    return targetKey === rootKey || targetKey.startsWith(`${rootKey}${path.sep}`);
  });
  if (!ok) {
    throw new Error(`cwd is outside allowed roots: ${resolved}`);
  }
  return resolved;
}

function tailFile(file, maxBytes = 65536) {
  if (!fs.existsSync(file)) return "";
  const stat = fs.statSync(file);
  const size = Math.min(stat.size, Math.max(0, Number(maxBytes) || 0));
  const fd = fs.openSync(file, "r");
  try {
    const buffer = Buffer.alloc(size);
    fs.readSync(fd, buffer, 0, size, stat.size - size);
    return buffer.toString("utf8");
  } finally {
    fs.closeSync(fd);
  }
}

function redactString(value) {
  return String(value)
    .replace(/(authorization:\s*bearer\s+)[^\s"']+/gi, "$1[REDACTED]")
    .replace(/(--(?:token|key|api-key|access-token|auth-token)\s+)[^\s"']+/gi, "$1[REDACTED]")
    .replace(/([?&](?:token|key|api_key|access_token|auth|__cf_chl_[a-z0-9_]+)=)[^&\s"']+/gi, "$1[REDACTED]")
    .replace(/((?:api[_-]?key|access[_-]?token|auth[_-]?token|secret)\s*[:=]\s*)[^\s"']+/gi, "$1[REDACTED]");
}

function scrubLogText(value, maxChars = 12000) {
  let text = redactString(value)
    .replace(/<html[\s\S]*?<\/html>/gi, "[REDACTED_HTML]")
    .replace(/((?:cH|md|mdrd|cRay|cUPMDTk|fa)\s*:\s*')[^']+'/gi, "$1[REDACTED]'")
    .replace(/[A-Za-z0-9_.-]{160,}/g, "[REDACTED_LONG_TOKEN]");
  if (text.length > maxChars) {
    text = `${text.slice(0, Math.floor(maxChars / 2))}\n[TRUNCATED ${text.length - maxChars} chars]\n${text.slice(-Math.ceil(maxChars / 2))}`;
  }
  return text;
}

function redact(value) {
  if (value == null) return value;
  if (typeof value === "string") return redactString(value);
  if (Array.isArray(value)) return value.map(redact);
  if (typeof value === "object") {
    const out = {};
    for (const [key, item] of Object.entries(value)) {
      if (/token|secret|password|api[_-]?key/i.test(key)) {
        out[key] = "[REDACTED]";
      } else {
        out[key] = redact(item);
      }
    }
    return out;
  }
  return value;
}

module.exports = {
  assertAllowedRoot,
  ensureDir,
  redact,
  scrubLogText,
  safeId,
  safeJoin,
  splitPathList,
  tailFile,
};
