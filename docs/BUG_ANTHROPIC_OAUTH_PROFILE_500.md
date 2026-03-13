# Bug Report: Anthropic OAuth Profile Creation Returns 500 Instead of 409

**Reported:** 2026-03-13
**Severity:** High (blocks wizard UX flow)
**Symptom:** Wizard shows `Provider setup failed: GatewayApiError: 500 Internal Server Error: {"error":"internal server error"}`

---

## Root Cause

Two issues combine to produce this bug.

### 1. Anyhow Error Chain Masks SQLite UNIQUE Violation

`main.rs:12447-12456` — the `create_auth_profile` handler checks for UNIQUE constraint violations:

```rust
.map_err(|err| {
    if err.to_string().contains("UNIQUE constraint failed") {  // <-- BUG
        api_error(StatusCode::CONFLICT, "auth profile display_name already exists for this provider")
    } else {
        internal_err_with_error("creating auth profile failed", err)
    }
})?;
```

But `storage/lib.rs:5709` wraps the rusqlite error:

```rust
conn.execute(/* INSERT ... */)
    .context("failed to create auth profile")?;  // <-- wraps the real error
```

`err.to_string()` on an anyhow error returns the **outermost context** — `"failed to create auth profile"` — not the underlying `rusqlite::Error` containing `"UNIQUE constraint failed"`. The check never matches, so every UNIQUE violation falls through to the 500 branch.

### 2. Stale Profile in DB

The user's DB already contains a `claude-primary` / `anthropic` profile from a prior attempt. Every subsequent wizard attempt hits the UNIQUE index on `(provider, display_name)` and gets the masked 500.

---

## Reproduction

```sql
-- Confirm stale profile exists:
sqlite3 runtime/oneclick-state/carsinos.db \
  "SELECT auth_profile_id, provider, display_name, auth_mode FROM auth_profiles;"
-- Returns: df45bd31-... | anthropic | claude-primary | claude_consumer_oauth
```

Gateway log shows the sequence clearly:
```
10:13:20.901Z WARN  main.rs:12425 creating high-risk auth profile provider=anthropic auth_mode=claude_consumer_oauth
10:13:20.902Z ERROR main.rs:30986 creating auth profile failed error=failed to create auth profile
10:13:20.902Z INFO  finished processing request status=500 method=POST uri=/api/v1/auth/profiles
```

Note: the ERROR line shows only the anyhow wrapper, not the rusqlite root cause. This is because `internal_err_with_error` at `main.rs:30986` uses `error!(error = %err, ...)` — the `%` (Display) format only shows the outermost context.

---

## Suggested Fix

**`main.rs:12448`** — search the full error chain instead of just `to_string()`:

```rust
// Option A: format with Debug to get full chain
if format!("{err:?}").contains("UNIQUE constraint failed") {

// Option B: walk the anyhow chain explicitly
if err.chain().any(|e| e.to_string().contains("UNIQUE constraint failed")) {
```

**`main.rs:30986`** — consider logging with `{err:?}` instead of `%err` so root causes are visible:

```rust
error!(error = ?err, "{message}");  // Debug format shows full chain
```

---

## Dex Review Checklist

Things to double-check against the broader codebase:

- [ ] **Is this pattern used elsewhere?** Search for `.to_string().contains("UNIQUE constraint failed")` across the gateway — every instance has the same bug if the storage layer wraps errors with `.context()`.
- [ ] **Other `internal_err_with_error` consumers** — are there other storage errors being masked by the `%err` Display format? The `error = %err` pattern hides root causes for ALL 500s.
- [ ] **Wizard retry logic** — does the frontend onboarding controller handle 409 Conflict? If the gateway correctly returns 409, does `useOnboardingController.ts` show a useful message or does it need a specific catch for duplicate profile names?
- [ ] **Profile cleanup** — should the wizard delete or update an existing profile with the same name instead of creating a new one? Or at least check for existence first?
- [ ] **The `claude_consumer_oauth` auth mode** — the log says `requires_kill_switch=true`. Is the kill switch being set up correctly in the wizard flow, or could that also fail silently?
- [ ] **Background SQLite failures** — the same log shows repeated `failed to open sqlite db` errors on discord/telegram listeners and scheduler. These are separate from this bug but indicate broader DB access issues (file locking? path with spaces?).

---

## Immediate Workaround

```sql
sqlite3 runtime/oneclick-state/carsinos.db \
  "DELETE FROM auth_profiles WHERE display_name = 'claude-primary' AND provider = 'anthropic';"
```

Then retry the wizard.
