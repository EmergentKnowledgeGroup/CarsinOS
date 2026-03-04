const REDACTED_VALUE = "[REDACTED]";

const SENSITIVE_KEYS = new Set([
  "token",
  "setup_token",
  "access_token",
  "refresh_token",
  "api_key",
  "bearer_token",
  "authorization",
  "cookie",
  "set-cookie",
  "x-api-key",
  "secret",
  "client_secret",
]);

function looksSensitiveKey(key: string): boolean {
  const normalized = key.trim().toLowerCase();
  if (SENSITIVE_KEYS.has(normalized)) {
    return true;
  }
  return normalized.includes("token") || normalized.includes("secret") || normalized.includes("api_key");
}

function redactStringValue(value: string): string {
  let next = value;

  // Redact bearer-style header values.
  next = next.replace(/(Bearer\s+)[^\s]+/gi, `$1${REDACTED_VALUE}`);

  // Redact API key-like tokens in free text.
  next = next.replace(/\bsk-[A-Za-z0-9_-]{6,}\b/g, REDACTED_VALUE);

  // Redact generic x-api-key assignment forms.
  next = next.replace(/(x-api-key\s*[:=]\s*)[^\s,;]+/gi, `$1${REDACTED_VALUE}`);

  return next;
}

export function redactSecrets<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map((item) => redactSecrets(item)) as T;
  }

  if (value && typeof value === "object") {
    const output: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(value as Record<string, unknown>)) {
      if (looksSensitiveKey(key)) {
        output[key] = REDACTED_VALUE;
        continue;
      }
      output[key] = redactSecrets(nested);
    }
    return output as T;
  }

  if (typeof value === "string") {
    return redactStringValue(value) as T;
  }

  return value;
}
