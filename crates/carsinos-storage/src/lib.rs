use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub attachments_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            db_path: root.join("carsinos.db"),
            attachments_dir: root.join("attachments"),
            logs_dir: root.join("logs"),
            root,
        }
    }
}

pub fn init(paths: &AppPaths) -> Result<()> {
    ensure_dirs(paths)?;
    migrate(&paths.db_path)?;
    seed_default_agent(&paths.db_path)?;
    harden_permissions(paths)?;
    Ok(())
}

fn ensure_dirs(paths: &AppPaths) -> Result<()> {
    std::fs::create_dir_all(&paths.root).context("failed to create state root")?;
    std::fs::create_dir_all(&paths.attachments_dir)
        .context("failed to create attachments directory")?;
    std::fs::create_dir_all(&paths.logs_dir).context("failed to create logs directory")?;
    Ok(())
}

fn migrate(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open sqlite db at {}", db_path.display()))?;
    conn.execute_batch(MIGRATION_0001)
        .context("failed applying initial migration")?;
    Ok(())
}

fn seed_default_agent(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open sqlite db at {}", db_path.display()))?;
    let now = now_ms();
    let workspace_root = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    conn.execute(
        r#"
        INSERT OR IGNORE INTO agents
          (agent_id, name, workspace_root, model_provider, model_id, tool_profile, created_at, updated_at)
        VALUES
          (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            "default",
            "Default Agent",
            workspace_root,
            "unconfigured",
            "unconfigured",
            "default",
            now,
            now
        ],
    )
    .context("failed to seed default agent")?;

    Ok(())
}

#[cfg(unix)]
fn harden_permissions(paths: &AppPaths) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(&paths.root, std::fs::Permissions::from_mode(0o700))
        .context("failed to set state root permissions")?;
    std::fs::set_permissions(
        &paths.attachments_dir,
        std::fs::Permissions::from_mode(0o700),
    )
    .context("failed to set attachments directory permissions")?;
    std::fs::set_permissions(&paths.logs_dir, std::fs::Permissions::from_mode(0o700))
        .context("failed to set logs directory permissions")?;

    if paths.db_path.exists() {
        std::fs::set_permissions(&paths.db_path, std::fs::Permissions::from_mode(0o600))
            .context("failed to set sqlite db permissions")?;
    }

    Ok(())
}

#[cfg(not(unix))]
fn harden_permissions(_paths: &AppPaths) -> Result<()> {
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Storage {
    db_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub session_key: String,
    pub agent_id: String,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub message_count: i64,
    pub run_count: i64,
}

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub message_id: String,
    pub session_id: String,
    pub source_channel: String,
    pub source_peer_id: Option<String>,
    pub source_message_id: Option<String>,
    pub role: String,
    pub content_text: String,
    pub content_format: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct RunRecord {
    pub run_id: String,
    pub session_id: String,
    pub status: String,
    pub model_provider: String,
    pub model_id: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub error_text: Option<String>,
    pub usage_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ApprovalRecord {
    pub approval_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub kind: String,
    pub status: String,
    pub request_summary: String,
    pub request_json: String,
    pub requested_at: i64,
    pub decided_at: Option<i64>,
    pub decided_via: Option<String>,
    pub decided_by_peer_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_call_id: String,
    pub run_id: String,
    pub tool_name: String,
    pub args_json: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub status: String,
    pub result_json: Option<String>,
    pub error_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewApproval {
    pub run_id: String,
    pub tool_call_id: Option<String>,
    pub kind: String,
    pub request_summary: String,
    pub request_json: String,
}

#[derive(Debug, Clone)]
pub enum ApprovalResolveResult {
    Resolved(ApprovalRecord),
    AlreadyResolved(ApprovalRecord),
    NotFound,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub session_key: Option<String>,
    pub agent_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub session_id: String,
    pub source_channel: String,
    pub source_peer_id: Option<String>,
    pub source_message_id: Option<String>,
    pub role: String,
    pub content_text: String,
    pub content_format: String,
}

#[derive(Debug, Clone)]
pub struct NewRun {
    pub session_id: String,
    pub model_provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub job_id: String,
    pub agent_id: String,
    pub name: String,
    pub enabled: bool,
    pub schedule_kind: String,
    pub interval_seconds: Option<i64>,
    pub run_at_ms: Option<i64>,
    pub next_run_at: Option<i64>,
    pub payload_json: String,
    pub max_retries: i64,
    pub retry_backoff_ms: i64,
    pub timeout_ms: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub agent_id: String,
    pub name: String,
    pub enabled: bool,
    pub schedule_kind: String,
    pub interval_seconds: Option<i64>,
    pub run_at_ms: Option<i64>,
    pub next_run_at: Option<i64>,
    pub payload_json: String,
    pub max_retries: i64,
    pub retry_backoff_ms: i64,
    pub timeout_ms: i64,
}

#[derive(Debug, Clone)]
pub struct JobUpdatePatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub interval_seconds: Option<i64>,
    pub run_at_ms: Option<i64>,
    pub next_run_at: Option<i64>,
    pub payload_json: Option<String>,
    pub max_retries: Option<i64>,
    pub retry_backoff_ms: Option<i64>,
    pub timeout_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct JobRunRecord {
    pub job_run_id: String,
    pub job_id: String,
    pub trigger_kind: String,
    pub status: String,
    pub attempt: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub error_text: Option<String>,
    pub output_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AuthProfileRecord {
    pub auth_profile_id: String,
    pub provider: String,
    pub display_name: String,
    pub auth_mode: String,
    pub risk_level: String,
    pub enabled: bool,
    pub kill_switch_scope: String,
    pub api_base_url: Option<String>,
    pub credentials_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewAuthProfile {
    pub provider: String,
    pub display_name: String,
    pub auth_mode: String,
    pub risk_level: String,
    pub enabled: bool,
    pub kill_switch_scope: String,
    pub api_base_url: Option<String>,
    pub credentials_json: String,
}

#[derive(Debug, Clone)]
pub struct SecurityAuditEventRecord {
    pub event_id: String,
    pub request_id: String,
    pub correlation_id: String,
    pub principal: String,
    pub action: String,
    pub resource: String,
    pub decision: String,
    pub reason: Option<String>,
    pub transport: String,
    pub status: String,
    pub error_code: Option<String>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewSecurityAuditEvent {
    pub request_id: String,
    pub correlation_id: String,
    pub principal: String,
    pub action: String,
    pub resource: String,
    pub decision: String,
    pub reason: Option<String>,
    pub transport: String,
    pub status: String,
    pub error_code: Option<String>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SecurityAuditEventListFilter {
    pub action: Option<String>,
    pub principal: Option<String>,
    pub decision: Option<String>,
    pub status: Option<String>,
    pub error_code: Option<String>,
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NoteRecord {
    pub note_id: String,
    pub title: Option<String>,
    pub body: String,
    pub tags_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewNote {
    pub title: Option<String>,
    pub body: String,
    pub tags_json: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddingSearchMatch {
    pub note_id: String,
    pub note_title: Option<String>,
    pub snippet: String,
    pub chunk_index: i64,
    pub score: f64,
}

impl Storage {
    pub fn from_paths(paths: &AppPaths) -> Self {
        Self {
            db_path: paths.db_path.clone(),
        }
    }

    pub fn ping(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.query_row("SELECT 1", [], |_| Ok(()))
            .context("failed health-check ping query")?;
        Ok(())
    }

    pub fn list_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              s.session_id,
              s.session_key,
              s.agent_id,
              s.title,
              s.created_at,
              s.updated_at,
              s.closed_at,
              (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.session_id) AS message_count,
              (SELECT COUNT(*) FROM runs r WHERE r.session_id = s.session_id) AS run_count
            FROM sessions s
            ORDER BY s.updated_at DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![i64::from(limit)], |row| {
            Ok(SessionRecord {
                session_id: row.get(0)?,
                session_key: row.get(1)?,
                agent_id: row.get(2)?,
                title: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                closed_at: row.get(6)?,
                message_count: row.get(7)?,
                run_count: row.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              s.session_id,
              s.session_key,
              s.agent_id,
              s.title,
              s.created_at,
              s.updated_at,
              s.closed_at,
              (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.session_id) AS message_count,
              (SELECT COUNT(*) FROM runs r WHERE r.session_id = s.session_id) AS run_count
            FROM sessions s
            WHERE s.session_id = ?1
            "#,
        )?;

        let record = stmt
            .query_row(params![session_id], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    session_key: row.get(1)?,
                    agent_id: row.get(2)?,
                    title: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    closed_at: row.get(6)?,
                    message_count: row.get(7)?,
                    run_count: row.get(8)?,
                })
            })
            .optional()?;

        Ok(record)
    }

    pub fn create_session(&self, new_session: NewSession) -> Result<SessionRecord> {
        let conn = self.connect()?;
        self.ensure_agent_exists(&conn, &new_session.agent_id)?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let session_key = new_session
            .session_key
            .unwrap_or_else(|| format!("session:{session_id}"));

        conn.execute(
            r#"
            INSERT INTO sessions
              (session_id, session_key, agent_id, title, created_at, updated_at, closed_at)
            VALUES
              (?1, ?2, ?3, ?4, ?5, ?6, NULL)
            "#,
            params![
                session_id,
                session_key,
                new_session.agent_id,
                new_session.title,
                now,
                now
            ],
        )
        .context("failed to create session")?;

        self.get_session(&session_id)?
            .context("created session could not be reloaded")
    }

    pub fn create_message(&self, new_message: NewMessage) -> Result<Option<MessageRecord>> {
        let conn = self.connect()?;
        if !self.session_exists(&conn, &new_message.session_id)? {
            return Ok(None);
        }

        let message_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();

        conn.execute(
            r#"
            INSERT INTO messages
              (message_id, session_id, source_channel, source_peer_id, source_message_id, role, content_text, content_format, created_at)
            VALUES
              (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                message_id,
                new_message.session_id,
                new_message.source_channel,
                new_message.source_peer_id,
                new_message.source_message_id,
                new_message.role,
                new_message.content_text,
                new_message.content_format,
                now
            ],
        )
        .context("failed to create message")?;

        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE session_id = ?2",
            params![now, new_message.session_id],
        )
        .context("failed to bump session updated_at after message insert")?;

        Ok(Some(MessageRecord {
            message_id,
            session_id: new_message.session_id,
            source_channel: new_message.source_channel,
            source_peer_id: new_message.source_peer_id,
            source_message_id: new_message.source_message_id,
            role: new_message.role,
            content_text: new_message.content_text,
            content_format: new_message.content_format,
            created_at: now,
        }))
    }

    pub fn list_messages(&self, session_id: &str, limit: u32) -> Result<Vec<MessageRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              message_id,
              session_id,
              source_channel,
              source_peer_id,
              source_message_id,
              role,
              content_text,
              content_format,
              created_at
            FROM messages
            WHERE session_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![session_id, i64::from(limit)], |row| {
            Ok(MessageRecord {
                message_id: row.get(0)?,
                session_id: row.get(1)?,
                source_channel: row.get(2)?,
                source_peer_id: row.get(3)?,
                source_message_id: row.get(4)?,
                role: row.get(5)?,
                content_text: row.get(6)?,
                content_format: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out.reverse();
        Ok(out)
    }

    pub fn create_run(&self, new_run: NewRun) -> Result<Option<RunRecord>> {
        let conn = self.connect()?;
        if !self.session_exists(&conn, &new_run.session_id)? {
            return Ok(None);
        }

        let run_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let status = "queued".to_string();

        conn.execute(
            r#"
            INSERT INTO runs
              (run_id, session_id, status, model_provider, model_id, started_at, ended_at, error_text, usage_json, created_at)
            VALUES
              (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, NULL, ?6)
            "#,
            params![
                run_id,
                new_run.session_id,
                status,
                new_run.model_provider,
                new_run.model_id,
                now
            ],
        )
        .context("failed to create run")?;

        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE session_id = ?2",
            params![now, new_run.session_id],
        )
        .context("failed to bump session updated_at after run insert")?;

        Ok(Some(RunRecord {
            run_id,
            session_id: new_run.session_id,
            status,
            model_provider: new_run.model_provider,
            model_id: new_run.model_id,
            started_at: None,
            ended_at: None,
            error_text: None,
            usage_json: None,
            created_at: now,
        }))
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              run_id,
              session_id,
              status,
              model_provider,
              model_id,
              started_at,
              ended_at,
              error_text,
              usage_json,
              created_at
            FROM runs
            WHERE run_id = ?1
            "#,
        )?;

        let record = stmt.query_row(params![run_id], map_run_row).optional()?;

        Ok(record)
    }

    pub fn latest_user_message_text(&self, session_id: &str) -> Result<Option<String>> {
        let conn = self.connect()?;
        let text = conn
            .query_row(
                r#"
                SELECT content_text
                FROM messages
                WHERE session_id = ?1 AND role = 'user'
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                params![session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        Ok(text)
    }

    pub fn mark_run_started(&self, run_id: &str) -> Result<()> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE runs
            SET status = 'running', started_at = COALESCE(started_at, ?1)
            WHERE run_id = ?2
            "#,
            params![now, run_id],
        )
        .context("failed to mark run started")?;
        Ok(())
    }

    pub fn mark_run_succeeded(&self, run_id: &str) -> Result<()> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE runs
            SET status = 'succeeded', ended_at = ?1, error_text = NULL
            WHERE run_id = ?2
            "#,
            params![now, run_id],
        )
        .context("failed to mark run succeeded")?;
        Ok(())
    }

    pub fn mark_run_failed(&self, run_id: &str, error_text: &str) -> Result<()> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE runs
            SET status = 'failed', ended_at = ?1, error_text = ?2
            WHERE run_id = ?3
            "#,
            params![now, error_text, run_id],
        )
        .context("failed to mark run failed")?;
        Ok(())
    }

    pub fn set_run_usage_json(&self, run_id: &str, usage_json: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE runs
            SET usage_json = ?1
            WHERE run_id = ?2
            "#,
            params![usage_json, run_id],
        )
        .context("failed to update run usage_json")?;
        Ok(())
    }

    pub fn create_auth_profile(&self, new_profile: NewAuthProfile) -> Result<AuthProfileRecord> {
        let conn = self.connect()?;
        let auth_profile_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();

        conn.execute(
            r#"
            INSERT INTO auth_profiles
              (auth_profile_id, provider, display_name, auth_mode, risk_level, enabled, kill_switch_scope, api_base_url, credentials_json, created_at, updated_at)
            VALUES
              (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                auth_profile_id,
                new_profile.provider,
                new_profile.display_name,
                new_profile.auth_mode,
                new_profile.risk_level,
                if new_profile.enabled { 1 } else { 0 },
                new_profile.kill_switch_scope,
                new_profile.api_base_url,
                new_profile.credentials_json,
                now,
                now
            ],
        )
        .context("failed to create auth profile")?;

        self.get_auth_profile(&auth_profile_id)?
            .context("created auth profile could not be reloaded")
    }

    pub fn list_auth_profiles(
        &self,
        provider: Option<&str>,
        include_disabled: bool,
    ) -> Result<Vec<AuthProfileRecord>> {
        let conn = self.connect()?;
        let mut out = Vec::new();
        let mut query = String::from(
            r#"
            SELECT
              auth_profile_id, provider, display_name, auth_mode, risk_level,
              enabled, kill_switch_scope, api_base_url, credentials_json, created_at, updated_at
            FROM auth_profiles
            WHERE 1 = 1
            "#,
        );

        if provider.is_some() {
            query.push_str(" AND provider = ?1");
        }
        if !include_disabled {
            query.push_str(" AND enabled = 1");
        }
        query.push_str(" ORDER BY provider ASC, display_name ASC");

        if let Some(provider) = provider {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(params![provider], map_auth_profile_row)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map([], map_auth_profile_row)?;
            for row in rows {
                out.push(row?);
            }
        }

        Ok(out)
    }

    pub fn get_auth_profile(&self, auth_profile_id: &str) -> Result<Option<AuthProfileRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              auth_profile_id, provider, display_name, auth_mode, risk_level,
              enabled, kill_switch_scope, api_base_url, credentials_json, created_at, updated_at
            FROM auth_profiles
            WHERE auth_profile_id = ?1
            "#,
        )?;

        let record = stmt
            .query_row(params![auth_profile_id], map_auth_profile_row)
            .optional()?;
        Ok(record)
    }

    pub fn update_auth_profile_state(
        &self,
        auth_profile_id: &str,
        enabled: Option<bool>,
        kill_switch_scope: Option<String>,
    ) -> Result<Option<AuthProfileRecord>> {
        if enabled.is_none() && kill_switch_scope.is_none() {
            return self.get_auth_profile(auth_profile_id);
        }

        let conn = self.connect()?;
        let current = match self.get_auth_profile(auth_profile_id)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let now = now_ms();
        let next_enabled = enabled.unwrap_or(current.enabled);
        let next_scope = kill_switch_scope.unwrap_or(current.kill_switch_scope);
        conn.execute(
            r#"
            UPDATE auth_profiles
            SET enabled = ?1, kill_switch_scope = ?2, updated_at = ?3
            WHERE auth_profile_id = ?4
            "#,
            params![
                if next_enabled { 1 } else { 0 },
                next_scope,
                now,
                auth_profile_id
            ],
        )
        .context("failed to update auth profile state")?;

        self.get_auth_profile(auth_profile_id)
    }

    pub fn update_auth_profile_credentials(
        &self,
        auth_profile_id: &str,
        credentials_json: String,
    ) -> Result<Option<AuthProfileRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE auth_profiles
            SET credentials_json = ?1, updated_at = ?2
            WHERE auth_profile_id = ?3
            "#,
            params![credentials_json, now, auth_profile_id],
        )
        .context("failed to update auth profile credentials")?;
        self.get_auth_profile(auth_profile_id)
    }

    pub fn set_agent_provider_profile_order(
        &self,
        agent_id: &str,
        provider: &str,
        profile_ids: &[String],
    ) -> Result<Vec<String>> {
        let mut conn = self.connect()?;
        self.ensure_agent_exists(&conn, agent_id)?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM agent_provider_profile_order WHERE agent_id = ?1 AND provider = ?2",
            params![agent_id, provider],
        )?;

        let now = now_ms();
        let mut inserted = Vec::new();
        for (priority, profile_id) in profile_ids.iter().enumerate() {
            let profile = tx
                .query_row(
                    r#"
                    SELECT provider
                    FROM auth_profiles
                    WHERE auth_profile_id = ?1
                    "#,
                    params![profile_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .with_context(|| format!("auth profile not found: {profile_id}"))?;
            if profile != provider {
                anyhow::bail!(
                    "auth profile {} belongs to provider '{}' not '{}'",
                    profile_id,
                    profile,
                    provider
                );
            }

            tx.execute(
                r#"
                INSERT INTO agent_provider_profile_order
                  (agent_id, provider, auth_profile_id, priority, created_at, updated_at)
                VALUES
                  (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![agent_id, provider, profile_id, priority as i64, now, now],
            )?;
            inserted.push(profile_id.clone());
        }

        tx.commit()
            .context("failed to save provider profile order")?;
        Ok(inserted)
    }

    pub fn list_agent_provider_profile_order(
        &self,
        agent_id: &str,
        provider: &str,
    ) -> Result<Vec<String>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT auth_profile_id
            FROM agent_provider_profile_order
            WHERE agent_id = ?1 AND provider = ?2
            ORDER BY priority ASC
            "#,
        )?;
        let rows = stmt.query_map(params![agent_id, provider], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn provider_kill_switch_active(&self, provider: &str) -> Result<bool> {
        let conn = self.connect()?;
        let active = conn
            .query_row(
                r#"
                SELECT 1
                FROM auth_profiles
                WHERE enabled = 1
                  AND provider = ?1
                  AND kill_switch_scope IN ('provider', 'global')
                LIMIT 1
                "#,
                params![provider],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(active)
    }

    pub fn global_kill_switch_active(&self) -> Result<bool> {
        let conn = self.connect()?;
        let active = conn
            .query_row(
                r#"
                SELECT 1
                FROM auth_profiles
                WHERE enabled = 1
                  AND kill_switch_scope = 'global'
                LIMIT 1
                "#,
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(active)
    }

    pub fn append_security_audit_event(
        &self,
        event: NewSecurityAuditEvent,
    ) -> Result<SecurityAuditEventRecord> {
        let conn = self.connect()?;
        let event_id = uuid::Uuid::new_v4().to_string();
        let created_at = now_ms();
        conn.execute(
            r#"
            INSERT INTO security_audit_events (
              event_id, request_id, correlation_id, principal, action, resource,
              decision, reason, transport, status, error_code, session_id, run_id,
              metadata_json, created_at
            )
            VALUES (
              ?1, ?2, ?3, ?4, ?5, ?6,
              ?7, ?8, ?9, ?10, ?11, ?12, ?13,
              ?14, ?15
            )
            "#,
            params![
                event_id,
                event.request_id,
                event.correlation_id,
                event.principal,
                event.action,
                event.resource,
                event.decision,
                event.reason,
                event.transport,
                event.status,
                event.error_code,
                event.session_id,
                event.run_id,
                event.metadata_json,
                created_at
            ],
        )
        .context("failed to append security audit event")?;
        self.get_security_audit_event(&event_id)?
            .context("inserted security audit event missing")
    }

    pub fn get_security_audit_event(
        &self,
        event_id: &str,
    ) -> Result<Option<SecurityAuditEventRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              event_id, request_id, correlation_id, principal, action, resource,
              decision, reason, transport, status, error_code, session_id, run_id,
              metadata_json, created_at
            FROM security_audit_events
            WHERE event_id = ?1
            "#,
        )?;
        let record = stmt
            .query_row(params![event_id], map_security_audit_event_row)
            .optional()?;
        Ok(record)
    }

    pub fn list_security_audit_events(
        &self,
        limit: u32,
        filter: &SecurityAuditEventListFilter,
    ) -> Result<Vec<SecurityAuditEventRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              event_id, request_id, correlation_id, principal, action, resource,
              decision, reason, transport, status, error_code, session_id, run_id,
              metadata_json, created_at
            FROM security_audit_events
            WHERE (?1 IS NULL OR action = ?1)
              AND (?2 IS NULL OR principal = ?2)
              AND (?3 IS NULL OR decision = ?3)
              AND (?4 IS NULL OR status = ?4)
              AND (?5 IS NULL OR error_code = ?5)
              AND (?6 IS NULL OR created_at >= ?6)
              AND (?7 IS NULL OR created_at <= ?7)
            ORDER BY created_at DESC, event_id DESC
            LIMIT ?8
            "#,
        )?;
        let action = filter.action.as_deref();
        let principal = filter.principal.as_deref();
        let decision = filter.decision.as_deref();
        let status = filter.status.as_deref();
        let error_code = filter.error_code.as_deref();
        let created_after = filter.created_after;
        let created_before = filter.created_before;
        let rows = stmt.query_map(
            params![
                action,
                principal,
                decision,
                status,
                error_code,
                created_after,
                created_before,
                i64::from(limit.clamp(1, 1000))
            ],
            map_security_audit_event_row,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_app_kv_json(&self, key: &str) -> Result<Option<(String, i64)>> {
        let conn = self.connect()?;
        let record = conn
            .query_row(
                r#"
                SELECT value_json, updated_at
                FROM app_kv
                WHERE key = ?1
                "#,
                params![key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        Ok(record)
    }

    pub fn set_app_kv_json(&self, key: &str, value_json: String) -> Result<i64> {
        let conn = self.connect()?;
        let updated_at = now_ms();
        conn.execute(
            r#"
            INSERT INTO app_kv (key, value_json, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value_json = excluded.value_json,
              updated_at = excluded.updated_at
            "#,
            params![key, value_json, updated_at],
        )
        .context("failed to upsert app kv value")?;
        Ok(updated_at)
    }

    pub fn create_note(&self, new_note: NewNote) -> Result<NoteRecord> {
        let conn = self.connect()?;
        let note_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO notes
              (note_id, title, body, tags_json, created_at, updated_at)
            VALUES
              (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                note_id,
                new_note.title,
                new_note.body,
                new_note.tags_json,
                now,
                now
            ],
        )
        .context("failed to create note")?;
        self.get_note(&note_id)?
            .context("created note could not be reloaded")
    }

    pub fn list_notes(&self, limit: u32) -> Result<Vec<NoteRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT note_id, title, body, tags_json, created_at, updated_at
            FROM notes
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![i64::from(limit)], map_note_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_note(&self, note_id: &str) -> Result<Option<NoteRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT note_id, title, body, tags_json, created_at, updated_at
            FROM notes
            WHERE note_id = ?1
            "#,
        )?;
        let record = stmt.query_row(params![note_id], map_note_row).optional()?;
        Ok(record)
    }

    pub fn update_note(
        &self,
        note_id: &str,
        title: Option<String>,
        body: Option<String>,
        tags_json: Option<String>,
    ) -> Result<Option<NoteRecord>> {
        let current = match self.get_note(note_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let conn = self.connect()?;
        let updated_at = now_ms();
        conn.execute(
            r#"
            UPDATE notes
            SET title = ?1, body = ?2, tags_json = ?3, updated_at = ?4
            WHERE note_id = ?5
            "#,
            params![
                title.or(current.title),
                body.unwrap_or(current.body),
                tags_json.unwrap_or(current.tags_json),
                updated_at,
                note_id
            ],
        )
        .context("failed to update note")?;
        self.get_note(note_id)
    }

    pub fn replace_note_embeddings(
        &self,
        note_id: &str,
        model: &str,
        chunks: &[(String, Vec<f32>)],
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM embeddings WHERE source_kind = 'note' AND source_id = ?1",
            params![note_id],
        )?;
        let now = now_ms();
        for (chunk_index, (text, vector)) in chunks.iter().enumerate() {
            if vector.is_empty() {
                continue;
            }
            let embedding_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                r#"
                INSERT INTO embeddings
                  (embedding_id, source_kind, source_id, chunk_index, model, dims, vec, text, created_at)
                VALUES
                  (?1, 'note', ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    embedding_id,
                    note_id,
                    chunk_index as i64,
                    model,
                    vector.len() as i64,
                    vec_to_blob(vector),
                    text,
                    now
                ],
            )
            .context("failed to insert note embedding")?;
        }
        tx.commit().context("failed to commit note embeddings")?;
        Ok(())
    }

    pub fn search_note_embeddings(
        &self,
        query_vector: &[f32],
        top_k: u32,
        max_candidates: u32,
    ) -> Result<Vec<EmbeddingSearchMatch>> {
        if query_vector.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              e.source_id,
              n.title,
              e.text,
              e.chunk_index,
              e.dims,
              e.vec
            FROM embeddings e
            JOIN notes n ON n.note_id = e.source_id
            WHERE e.source_kind = 'note'
            ORDER BY e.created_at DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![i64::from(max_candidates)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;

        let mut scored = Vec::new();
        for row in rows {
            let (note_id, note_title, snippet, chunk_index, dims, blob) = row?;
            if dims <= 0 || dims as usize != query_vector.len() {
                continue;
            }
            let vector = blob_to_vec(&blob)?;
            if vector.len() != query_vector.len() {
                continue;
            }
            let score = cosine_similarity(query_vector, &vector);
            if !score.is_finite() {
                continue;
            }
            scored.push(EmbeddingSearchMatch {
                note_id,
                note_title,
                snippet,
                chunk_index,
                score,
            });
        }

        scored.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k as usize);
        Ok(scored)
    }

    pub fn create_job(&self, new_job: NewJob) -> Result<JobRecord> {
        let conn = self.connect()?;
        self.ensure_agent_exists(&conn, &new_job.agent_id)?;
        let job_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO jobs
              (job_id, agent_id, name, enabled, schedule_kind, interval_seconds, run_at_ms, next_run_at,
               payload_json, max_retries, retry_backoff_ms, timeout_ms, lease_owner, lease_expires_at,
               last_run_at, last_error, created_at, updated_at, deleted_at)
            VALUES
              (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, NULL, NULL, NULL, ?13, ?14, NULL)
            "#,
            params![
                job_id,
                new_job.agent_id,
                new_job.name,
                if new_job.enabled { 1 } else { 0 },
                new_job.schedule_kind,
                new_job.interval_seconds,
                new_job.run_at_ms,
                new_job.next_run_at,
                new_job.payload_json,
                new_job.max_retries,
                new_job.retry_backoff_ms,
                new_job.timeout_ms,
                now,
                now
            ],
        )
        .context("failed to create job")?;
        self.get_job(&job_id)?
            .context("created job could not be reloaded")
    }

    pub fn list_jobs(&self, limit: u32, include_disabled: bool) -> Result<Vec<JobRecord>> {
        let conn = self.connect()?;
        let mut out = Vec::new();
        let query = if include_disabled {
            r#"
            SELECT
              job_id, agent_id, name, enabled, schedule_kind, interval_seconds, run_at_ms, next_run_at,
              payload_json, max_retries, retry_backoff_ms, timeout_ms, lease_owner, lease_expires_at,
              last_run_at, last_error, created_at, updated_at, deleted_at
            FROM jobs
            WHERE deleted_at IS NULL
            ORDER BY updated_at DESC
            LIMIT ?1
            "#
        } else {
            r#"
            SELECT
              job_id, agent_id, name, enabled, schedule_kind, interval_seconds, run_at_ms, next_run_at,
              payload_json, max_retries, retry_backoff_ms, timeout_ms, lease_owner, lease_expires_at,
              last_run_at, last_error, created_at, updated_at, deleted_at
            FROM jobs
            WHERE deleted_at IS NULL AND enabled = 1
            ORDER BY updated_at DESC
            LIMIT ?1
            "#
        };
        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map(params![i64::from(limit)], map_job_row)?;
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              job_id, agent_id, name, enabled, schedule_kind, interval_seconds, run_at_ms, next_run_at,
              payload_json, max_retries, retry_backoff_ms, timeout_ms, lease_owner, lease_expires_at,
              last_run_at, last_error, created_at, updated_at, deleted_at
            FROM jobs
            WHERE job_id = ?1 AND deleted_at IS NULL
            "#,
        )?;
        let record = stmt.query_row(params![job_id], map_job_row).optional()?;
        Ok(record)
    }

    pub fn update_job(&self, job_id: &str, patch: JobUpdatePatch) -> Result<Option<JobRecord>> {
        let conn = self.connect()?;
        let current = match self.get_job(job_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let now = now_ms();
        let next_name = patch.name.unwrap_or(current.name);
        let next_enabled = patch.enabled.unwrap_or(current.enabled);
        let next_interval = patch.interval_seconds.or(current.interval_seconds);
        let next_run_at = patch.run_at_ms.or(current.run_at_ms);
        let next_next_run_at = patch.next_run_at.or(current.next_run_at);
        let next_payload = patch.payload_json.unwrap_or(current.payload_json);
        let next_max_retries = patch.max_retries.unwrap_or(current.max_retries);
        let next_retry_backoff = patch.retry_backoff_ms.unwrap_or(current.retry_backoff_ms);
        let next_timeout = patch.timeout_ms.unwrap_or(current.timeout_ms);

        conn.execute(
            r#"
            UPDATE jobs
            SET name = ?1,
                enabled = ?2,
                interval_seconds = ?3,
                run_at_ms = ?4,
                next_run_at = ?5,
                payload_json = ?6,
                max_retries = ?7,
                retry_backoff_ms = ?8,
                timeout_ms = ?9,
                updated_at = ?10
            WHERE job_id = ?11 AND deleted_at IS NULL
            "#,
            params![
                next_name,
                if next_enabled { 1 } else { 0 },
                next_interval,
                next_run_at,
                next_next_run_at,
                next_payload,
                next_max_retries,
                next_retry_backoff,
                next_timeout,
                now,
                job_id
            ],
        )
        .context("failed to update job")?;

        self.get_job(job_id)
    }

    pub fn remove_job(&self, job_id: &str) -> Result<bool> {
        let conn = self.connect()?;
        let now = now_ms();
        let changed = conn
            .execute(
                r#"
                UPDATE jobs
                SET enabled = 0, deleted_at = ?1, updated_at = ?2
                WHERE job_id = ?3 AND deleted_at IS NULL
                "#,
                params![now, now, job_id],
            )
            .context("failed to remove job")?;
        Ok(changed > 0)
    }

    pub fn jobs_total_count(&self) -> Result<u64> {
        let conn = self.connect()?;
        let count = conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE deleted_at IS NULL",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn jobs_enabled_count(&self) -> Result<u64> {
        let conn = self.connect()?;
        let count = conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE deleted_at IS NULL AND enabled = 1",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn jobs_due_count(&self, now_ms: i64) -> Result<u64> {
        let conn = self.connect()?;
        let count = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE deleted_at IS NULL
              AND enabled = 1
              AND next_run_at IS NOT NULL
              AND next_run_at <= ?1
              AND (lease_expires_at IS NULL OR lease_expires_at < ?1)
            "#,
            params![now_ms],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn acquire_due_jobs(
        &self,
        worker_id: &str,
        now_ms: i64,
        lease_ms: i64,
        limit: u32,
    ) -> Result<Vec<JobRecord>> {
        let conn = self.connect()?;
        let mut out = Vec::new();
        let mut stmt = conn.prepare(
            r#"
            SELECT job_id
            FROM jobs
            WHERE deleted_at IS NULL
              AND enabled = 1
              AND next_run_at IS NOT NULL
              AND next_run_at <= ?1
              AND (lease_expires_at IS NULL OR lease_expires_at < ?1)
            ORDER BY next_run_at ASC
            LIMIT ?2
            "#,
        )?;
        let ids = stmt.query_map(params![now_ms, i64::from(limit)], |row| {
            row.get::<_, String>(0)
        })?;

        for maybe_id in ids {
            let job_id = maybe_id?;
            let lease_expires_at = now_ms.saturating_add(lease_ms.max(1));
            let changed = conn.execute(
                r#"
                UPDATE jobs
                SET lease_owner = ?1, lease_expires_at = ?2, updated_at = ?3
                WHERE job_id = ?4
                  AND deleted_at IS NULL
                  AND enabled = 1
                  AND next_run_at IS NOT NULL
                  AND next_run_at <= ?5
                  AND (lease_expires_at IS NULL OR lease_expires_at < ?5)
                "#,
                params![worker_id, lease_expires_at, now_ms, job_id, now_ms],
            )?;
            if changed == 0 {
                continue;
            }
            if let Some(job) = self.get_job(&job_id)? {
                out.push(job);
            }
        }

        Ok(out)
    }

    pub fn create_job_run(
        &self,
        job_id: &str,
        trigger_kind: &str,
        attempt: i64,
    ) -> Result<Option<JobRunRecord>> {
        let conn = self.connect()?;
        let job_exists = conn
            .query_row(
                "SELECT 1 FROM jobs WHERE job_id = ?1 AND deleted_at IS NULL LIMIT 1",
                params![job_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !job_exists {
            return Ok(None);
        }

        let job_run_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO job_runs
              (job_run_id, job_id, trigger_kind, status, attempt, started_at, ended_at, error_text, output_json, created_at)
            VALUES
              (?1, ?2, ?3, 'running', ?4, ?5, NULL, NULL, NULL, ?6)
            "#,
            params![job_run_id, job_id, trigger_kind, attempt, now, now],
        )
        .context("failed to create job run")?;

        self.get_job_run(&job_run_id)
    }

    pub fn get_job_run(&self, job_run_id: &str) -> Result<Option<JobRunRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              job_run_id, job_id, trigger_kind, status, attempt, started_at,
              ended_at, error_text, output_json, created_at
            FROM job_runs
            WHERE job_run_id = ?1
            "#,
        )?;
        let record = stmt
            .query_row(params![job_run_id], map_job_run_row)
            .optional()?;
        Ok(record)
    }

    pub fn list_job_runs(&self, job_id: &str, limit: u32) -> Result<Vec<JobRunRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              job_run_id, job_id, trigger_kind, status, attempt, started_at,
              ended_at, error_text, output_json, created_at
            FROM job_runs
            WHERE job_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![job_id, i64::from(limit)], map_job_run_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn finish_job_run_success(
        &self,
        job_id: &str,
        job_run_id: &str,
        attempt: i64,
        output_json: String,
        next_run_at: Option<i64>,
        disable_job: bool,
    ) -> Result<Option<JobRunRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE job_runs
            SET status = 'succeeded', attempt = ?1, ended_at = ?2, error_text = NULL, output_json = ?3
            WHERE job_run_id = ?4
            "#,
            params![attempt, now, output_json, job_run_id],
        )
        .context("failed to mark job run success")?;
        conn.execute(
            r#"
            UPDATE jobs
            SET enabled = CASE WHEN ?1 = 1 THEN 0 ELSE enabled END,
                next_run_at = ?2,
                last_run_at = ?3,
                last_error = NULL,
                lease_owner = NULL,
                lease_expires_at = NULL,
                updated_at = ?4
            WHERE job_id = ?5 AND deleted_at IS NULL
            "#,
            params![
                if disable_job { 1 } else { 0 },
                next_run_at,
                now,
                now,
                job_id
            ],
        )
        .context("failed to update job after success")?;
        self.get_job_run(job_run_id)
    }

    pub fn finish_job_run_failed(
        &self,
        job_id: &str,
        job_run_id: &str,
        attempt: i64,
        error_text: String,
        next_run_at: Option<i64>,
    ) -> Result<Option<JobRunRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE job_runs
            SET status = 'failed', attempt = ?1, ended_at = ?2, error_text = ?3
            WHERE job_run_id = ?4
            "#,
            params![attempt, now, error_text, job_run_id],
        )
        .context("failed to mark job run failure")?;
        conn.execute(
            r#"
            UPDATE jobs
            SET next_run_at = ?1,
                last_run_at = ?2,
                last_error = ?3,
                lease_owner = NULL,
                lease_expires_at = NULL,
                updated_at = ?4
            WHERE job_id = ?5 AND deleted_at IS NULL
            "#,
            params![next_run_at, now, error_text, now, job_id],
        )
        .context("failed to update job after failure")?;
        self.get_job_run(job_run_id)
    }

    pub fn clear_job_lease(&self, job_id: &str) -> Result<()> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            "UPDATE jobs SET lease_owner = NULL, lease_expires_at = NULL, updated_at = ?1 WHERE job_id = ?2 AND deleted_at IS NULL",
            params![now, job_id],
        )
        .context("failed to clear job lease")?;
        Ok(())
    }

    pub fn create_tool_call(
        &self,
        run_id: &str,
        tool_name: &str,
        args_json: String,
    ) -> Result<Option<ToolCallRecord>> {
        let conn = self.connect()?;
        if !self.run_exists(&conn, run_id)? {
            return Ok(None);
        }
        let tool_call_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO tool_calls
              (tool_call_id, run_id, tool_name, args_json, started_at, ended_at, status, result_json, error_text)
            VALUES
              (?1, ?2, ?3, ?4, ?5, NULL, 'running', NULL, NULL)
            "#,
            params![tool_call_id, run_id, tool_name, args_json, now],
        )
        .context("failed to create tool call")?;
        self.get_tool_call(&tool_call_id)
    }

    pub fn finish_tool_call(
        &self,
        tool_call_id: &str,
        status: &str,
        result_json: Option<String>,
        error_text: Option<String>,
    ) -> Result<Option<ToolCallRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE tool_calls
            SET ended_at = ?1, status = ?2, result_json = ?3, error_text = ?4
            WHERE tool_call_id = ?5
            "#,
            params![now, status, result_json, error_text, tool_call_id],
        )
        .context("failed to finish tool call")?;
        self.get_tool_call(tool_call_id)
    }

    pub fn get_tool_call(&self, tool_call_id: &str) -> Result<Option<ToolCallRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              tool_call_id, run_id, tool_name, args_json, started_at,
              ended_at, status, result_json, error_text
            FROM tool_calls
            WHERE tool_call_id = ?1
            "#,
        )?;
        let record = stmt
            .query_row(params![tool_call_id], map_tool_call_row)
            .optional()?;
        Ok(record)
    }

    pub fn list_tool_calls(&self, run_id: &str, limit: u32) -> Result<Vec<ToolCallRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              tool_call_id, run_id, tool_name, args_json, started_at,
              ended_at, status, result_json, error_text
            FROM tool_calls
            WHERE run_id = ?1
            ORDER BY COALESCE(started_at, 0) DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![run_id, i64::from(limit)], map_tool_call_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn create_approval(&self, new_approval: NewApproval) -> Result<Option<ApprovalRecord>> {
        let conn = self.connect()?;
        if !self.run_exists(&conn, &new_approval.run_id)? {
            return Ok(None);
        }

        let tool_call_id = if let Some(tool_call_id) = new_approval.tool_call_id {
            let existing_run_id = conn
                .query_row(
                    "SELECT run_id FROM tool_calls WHERE tool_call_id = ?1",
                    params![&tool_call_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .with_context(|| format!("tool call not found: {tool_call_id}"))?;
            if existing_run_id != new_approval.run_id {
                anyhow::bail!(
                    "tool call {} belongs to run {} not {}",
                    tool_call_id,
                    existing_run_id,
                    new_approval.run_id
                );
            }
            tool_call_id
        } else {
            let tool_call_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                r#"
                INSERT INTO tool_calls
                  (tool_call_id, run_id, tool_name, args_json, started_at, ended_at, status, result_json, error_text)
                VALUES
                  (?1, ?2, ?3, ?4, NULL, NULL, 'pending', NULL, NULL)
                "#,
                params![
                    tool_call_id,
                    new_approval.run_id,
                    new_approval.kind,
                    new_approval.request_json
                ],
            )
            .context("failed to create tool_call for approval")?;
            tool_call_id
        };

        let approval_id = uuid::Uuid::new_v4().to_string();
        let requested_at = now_ms();
        conn.execute(
            r#"
            INSERT INTO approvals
              (approval_id, run_id, tool_call_id, kind, status, request_summary, request_json, requested_at, decided_at, decided_via, decided_by_peer_id)
            VALUES
              (?1, ?2, ?3, ?4, 'requested', ?5, ?6, ?7, NULL, NULL, NULL)
            "#,
            params![
                approval_id,
                new_approval.run_id,
                tool_call_id,
                new_approval.kind,
                new_approval.request_summary,
                new_approval.request_json,
                requested_at
            ],
        )
        .context("failed to create approval")?;

        self.get_approval(&approval_id)
    }

    pub fn find_latest_approval_for_request(
        &self,
        run_id: &str,
        kind: &str,
        request_json: &str,
    ) -> Result<Option<ApprovalRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              approval_id, run_id, tool_call_id, kind, status, request_summary, request_json,
              requested_at, decided_at, decided_via, decided_by_peer_id
            FROM approvals
            WHERE run_id = ?1 AND kind = ?2 AND request_json = ?3
            ORDER BY requested_at DESC
            LIMIT 1
            "#,
        )?;
        let record = stmt
            .query_row(params![run_id, kind, request_json], map_approval_row)
            .optional()?;
        Ok(record)
    }

    pub fn list_approvals(&self, status: Option<&str>, limit: u32) -> Result<Vec<ApprovalRecord>> {
        let conn = self.connect()?;
        let mut out = Vec::new();

        if let Some(status) = status {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                  approval_id, run_id, tool_call_id, kind, status, request_summary, request_json,
                  requested_at, decided_at, decided_via, decided_by_peer_id
                FROM approvals
                WHERE status = ?1
                ORDER BY requested_at DESC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![status, i64::from(limit)], map_approval_row)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                  approval_id, run_id, tool_call_id, kind, status, request_summary, request_json,
                  requested_at, decided_at, decided_via, decided_by_peer_id
                FROM approvals
                ORDER BY requested_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt.query_map(params![i64::from(limit)], map_approval_row)?;
            for row in rows {
                out.push(row?);
            }
        }

        Ok(out)
    }

    pub fn resolve_approval(
        &self,
        approval_id: &str,
        decision: &str,
        decided_via: Option<String>,
        decided_by_peer_id: Option<String>,
    ) -> Result<ApprovalResolveResult> {
        let existing = match self.get_approval(approval_id)? {
            Some(record) => record,
            None => return Ok(ApprovalResolveResult::NotFound),
        };

        if existing.status != "requested" {
            return Ok(ApprovalResolveResult::AlreadyResolved(existing));
        }

        let resolved_status = match decision {
            "approve" => "approved",
            "deny" => "denied",
            other => anyhow::bail!("invalid approval decision: {other}"),
        };
        let decided_at = now_ms();
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE approvals
            SET status = ?1, decided_at = ?2, decided_via = ?3, decided_by_peer_id = ?4
            WHERE approval_id = ?5
            "#,
            params![
                resolved_status,
                decided_at,
                decided_via,
                decided_by_peer_id,
                approval_id
            ],
        )
        .context("failed to resolve approval")?;

        let updated = self
            .get_approval(approval_id)?
            .context("resolved approval missing after update")?;

        Ok(ApprovalResolveResult::Resolved(updated))
    }

    pub fn get_approval(&self, approval_id: &str) -> Result<Option<ApprovalRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              approval_id, run_id, tool_call_id, kind, status, request_summary, request_json,
              requested_at, decided_at, decided_via, decided_by_peer_id
            FROM approvals
            WHERE approval_id = ?1
            "#,
        )?;

        let record = stmt
            .query_row(params![approval_id], map_approval_row)
            .optional()?;
        Ok(record)
    }

    fn connect(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
            .with_context(|| format!("failed to open sqlite db at {}", self.db_path.display()))
    }

    fn session_exists(&self, conn: &Connection, session_id: &str) -> Result<bool> {
        let exists = conn
            .query_row(
                "SELECT 1 FROM sessions WHERE session_id = ?1 LIMIT 1",
                params![session_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(exists)
    }

    fn ensure_agent_exists(&self, conn: &Connection, agent_id: &str) -> Result<()> {
        let exists = conn
            .query_row(
                "SELECT 1 FROM agents WHERE agent_id = ?1 LIMIT 1",
                params![agent_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if exists {
            Ok(())
        } else {
            anyhow::bail!("agent does not exist: {agent_id}");
        }
    }

    fn run_exists(&self, conn: &Connection, run_id: &str) -> Result<bool> {
        let exists = conn
            .query_row(
                "SELECT 1 FROM runs WHERE run_id = ?1 LIMIT 1",
                params![run_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(exists)
    }
}

fn map_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        run_id: row.get(0)?,
        session_id: row.get(1)?,
        status: row.get(2)?,
        model_provider: row.get(3)?,
        model_id: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        error_text: row.get(7)?,
        usage_json: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn map_approval_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApprovalRecord> {
    Ok(ApprovalRecord {
        approval_id: row.get(0)?,
        run_id: row.get(1)?,
        tool_call_id: row.get(2)?,
        kind: row.get(3)?,
        status: row.get(4)?,
        request_summary: row.get(5)?,
        request_json: row.get(6)?,
        requested_at: row.get(7)?,
        decided_at: row.get(8)?,
        decided_via: row.get(9)?,
        decided_by_peer_id: row.get(10)?,
    })
}

fn map_tool_call_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolCallRecord> {
    Ok(ToolCallRecord {
        tool_call_id: row.get(0)?,
        run_id: row.get(1)?,
        tool_name: row.get(2)?,
        args_json: row.get(3)?,
        started_at: row.get(4)?,
        ended_at: row.get(5)?,
        status: row.get(6)?,
        result_json: row.get(7)?,
        error_text: row.get(8)?,
    })
}

fn map_auth_profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthProfileRecord> {
    let enabled_i64: i64 = row.get(5)?;
    Ok(AuthProfileRecord {
        auth_profile_id: row.get(0)?,
        provider: row.get(1)?,
        display_name: row.get(2)?,
        auth_mode: row.get(3)?,
        risk_level: row.get(4)?,
        enabled: enabled_i64 != 0,
        kill_switch_scope: row.get(6)?,
        api_base_url: row.get(7)?,
        credentials_json: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let enabled_i64: i64 = row.get(3)?;
    Ok(JobRecord {
        job_id: row.get(0)?,
        agent_id: row.get(1)?,
        name: row.get(2)?,
        enabled: enabled_i64 != 0,
        schedule_kind: row.get(4)?,
        interval_seconds: row.get(5)?,
        run_at_ms: row.get(6)?,
        next_run_at: row.get(7)?,
        payload_json: row.get(8)?,
        max_retries: row.get(9)?,
        retry_backoff_ms: row.get(10)?,
        timeout_ms: row.get(11)?,
        lease_owner: row.get(12)?,
        lease_expires_at: row.get(13)?,
        last_run_at: row.get(14)?,
        last_error: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        deleted_at: row.get(18)?,
    })
}

fn map_job_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRunRecord> {
    Ok(JobRunRecord {
        job_run_id: row.get(0)?,
        job_id: row.get(1)?,
        trigger_kind: row.get(2)?,
        status: row.get(3)?,
        attempt: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        error_text: row.get(7)?,
        output_json: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn map_security_audit_event_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SecurityAuditEventRecord> {
    Ok(SecurityAuditEventRecord {
        event_id: row.get(0)?,
        request_id: row.get(1)?,
        correlation_id: row.get(2)?,
        principal: row.get(3)?,
        action: row.get(4)?,
        resource: row.get(5)?,
        decision: row.get(6)?,
        reason: row.get(7)?,
        transport: row.get(8)?,
        status: row.get(9)?,
        error_code: row.get(10)?,
        session_id: row.get(11)?,
        run_id: row.get(12)?,
        metadata_json: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn map_note_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRecord> {
    Ok(NoteRecord {
        note_id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        tags_json: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn vec_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn blob_to_vec(blob: &[u8]) -> Result<Vec<f32>> {
    if !blob.len().is_multiple_of(4) {
        anyhow::bail!("embedding blob length {} is not aligned to f32", blob.len());
    }
    let mut out = Vec::with_capacity(blob.len() / 4);
    let mut idx = 0;
    while idx < blob.len() {
        let bytes = [blob[idx], blob[idx + 1], blob[idx + 2], blob[idx + 3]];
        out.push(f32::from_le_bytes(bytes));
        idx += 4;
    }
    Ok(out)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_left = 0.0f64;
    let mut norm_right = 0.0f64;
    for (l, r) in left.iter().zip(right.iter()) {
        let l = f64::from(*l);
        let r = f64::from(*r);
        dot += l * r;
        norm_left += l * l;
        norm_right += r * r;
    }
    if norm_left <= f64::EPSILON || norm_right <= f64::EPSILON {
        return 0.0;
    }
    dot / (norm_left.sqrt() * norm_right.sqrt())
}

fn now_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as i64
}

const MIGRATION_0001: &str = include_str!("../../../migrations/0001_init.sql");

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_storage() -> (TempDir, Storage) {
        let temp_dir = TempDir::new().expect("tempdir");
        let paths = AppPaths::from_root(temp_dir.path().to_path_buf());
        init(&paths).expect("storage init");
        (temp_dir, Storage::from_paths(&paths))
    }

    #[test]
    fn session_message_run_lifecycle_updates_counts() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("lifecycle".to_string()),
            })
            .expect("create session");

        let created_user = storage
            .create_message(NewMessage {
                session_id: session.session_id.clone(),
                source_channel: "api".to_string(),
                source_peer_id: None,
                source_message_id: None,
                role: "user".to_string(),
                content_text: "hello".to_string(),
                content_format: "markdown".to_string(),
            })
            .expect("create user message")
            .expect("message exists");
        assert_eq!(created_user.role, "user");

        let created_assistant = storage
            .create_message(NewMessage {
                session_id: session.session_id.clone(),
                source_channel: "agent".to_string(),
                source_peer_id: None,
                source_message_id: None,
                role: "assistant".to_string(),
                content_text: "world".to_string(),
                content_format: "markdown".to_string(),
            })
            .expect("create assistant message")
            .expect("message exists");
        assert_eq!(created_assistant.role, "assistant");

        let run = storage
            .create_run(NewRun {
                session_id: session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create run")
            .expect("run exists");
        assert_eq!(run.status, "queued");
        storage.mark_run_started(&run.run_id).expect("mark started");
        storage
            .mark_run_succeeded(&run.run_id)
            .expect("mark succeeded");
        storage
            .set_run_usage_json(&run.run_id, r#"{"memory":{"enabled":true}}"#)
            .expect("set run usage_json");
        let updated_run = storage
            .get_run(&run.run_id)
            .expect("get updated run")
            .expect("run exists");
        assert_eq!(
            updated_run.usage_json.as_deref(),
            Some(r#"{"memory":{"enabled":true}}"#)
        );

        let session_after = storage
            .get_session(&session.session_id)
            .expect("get session")
            .expect("session exists");
        assert_eq!(session_after.message_count, 2);
        assert_eq!(session_after.run_count, 1);

        let messages = storage
            .list_messages(&session.session_id, 10)
            .expect("list messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");

        let latest_user = storage
            .latest_user_message_text(&session.session_id)
            .expect("latest user message lookup");
        assert_eq!(latest_user.as_deref(), Some("hello"));
    }

    #[test]
    fn missing_session_returns_none_for_message_and_run_create() {
        let (_temp_dir, storage) = test_storage();
        let message = storage
            .create_message(NewMessage {
                session_id: "missing-session".to_string(),
                source_channel: "api".to_string(),
                source_peer_id: None,
                source_message_id: None,
                role: "user".to_string(),
                content_text: "x".to_string(),
                content_format: "markdown".to_string(),
            })
            .expect("create message result");
        assert!(message.is_none());

        let run = storage
            .create_run(NewRun {
                session_id: "missing-session".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock".to_string(),
            })
            .expect("create run result");
        assert!(run.is_none());
    }

    #[test]
    fn approval_state_machine_resolves_once_and_filters() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("approval".to_string()),
            })
            .expect("create session");
        let _ = storage
            .create_message(NewMessage {
                session_id: session.session_id.clone(),
                source_channel: "api".to_string(),
                source_peer_id: None,
                source_message_id: None,
                role: "user".to_string(),
                content_text: "approval please".to_string(),
                content_format: "markdown".to_string(),
            })
            .expect("create message");

        let run = storage
            .create_run(NewRun {
                session_id: session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock".to_string(),
            })
            .expect("create run")
            .expect("run exists");

        let approval = storage
            .create_approval(NewApproval {
                run_id: run.run_id.clone(),
                tool_call_id: None,
                kind: "exec".to_string(),
                request_summary: "do thing".to_string(),
                request_json: r#"{"command":"echo hi"}"#.to_string(),
            })
            .expect("create approval")
            .expect("approval exists");
        assert_eq!(approval.status, "requested");

        let requested = storage
            .list_approvals(Some("requested"), 10)
            .expect("list requested");
        assert!(requested
            .iter()
            .any(|item| item.approval_id == approval.approval_id));

        let resolved = storage
            .resolve_approval(
                &approval.approval_id,
                "approve",
                Some("test".to_string()),
                Some("peer-1".to_string()),
            )
            .expect("resolve approval");
        let resolved = match resolved {
            ApprovalResolveResult::Resolved(record) => record,
            _ => panic!("expected resolved approval"),
        };
        assert_eq!(resolved.status, "approved");

        let second = storage
            .resolve_approval(
                &approval.approval_id,
                "deny",
                Some("test".to_string()),
                Some("peer-2".to_string()),
            )
            .expect("resolve approval second time");
        match second {
            ApprovalResolveResult::AlreadyResolved(record) => {
                assert_eq!(record.status, "approved");
            }
            _ => panic!("expected already resolved"),
        }

        let approved = storage
            .list_approvals(Some("approved"), 10)
            .expect("list approved");
        assert!(approved
            .iter()
            .any(|item| item.approval_id == approval.approval_id));

        let found = storage
            .find_latest_approval_for_request(&run.run_id, "exec", r#"{"command":"echo hi"}"#)
            .expect("lookup approval by request")
            .expect("approval exists");
        assert_eq!(found.approval_id, approval.approval_id);
    }

    #[test]
    fn tool_call_lifecycle_round_trip() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("tool-call".to_string()),
            })
            .expect("create session");
        let run = storage
            .create_run(NewRun {
                session_id: session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create run")
            .expect("run exists");

        let call = storage
            .create_tool_call(&run.run_id, "exec", r#"{"command":"echo hi"}"#.to_string())
            .expect("create tool call")
            .expect("tool call exists");
        assert_eq!(call.status, "running");

        let finished = storage
            .finish_tool_call(
                &call.tool_call_id,
                "succeeded",
                Some(r#"{"stdout":"hi"}"#.to_string()),
                None,
            )
            .expect("finish tool call")
            .expect("finished call exists");
        assert_eq!(finished.status, "succeeded");
        assert_eq!(finished.result_json.as_deref(), Some(r#"{"stdout":"hi"}"#));

        let list = storage
            .list_tool_calls(&run.run_id, 10)
            .expect("list tool calls");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].tool_call_id, call.tool_call_id);
    }

    #[test]
    fn approval_can_reference_existing_tool_call_and_validates_run_match() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("approval-tool-call-link".to_string()),
            })
            .expect("create session");
        let run = storage
            .create_run(NewRun {
                session_id: session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create run")
            .expect("run exists");
        let call = storage
            .create_tool_call(&run.run_id, "exec", r#"{"command":"echo hi"}"#.to_string())
            .expect("create tool call")
            .expect("tool call exists");

        let approval = storage
            .create_approval(NewApproval {
                run_id: run.run_id.clone(),
                tool_call_id: Some(call.tool_call_id.clone()),
                kind: "exec".to_string(),
                request_summary: "reuse call".to_string(),
                request_json: r#"{"command":"echo hi"}"#.to_string(),
            })
            .expect("create approval")
            .expect("approval exists");
        assert_eq!(approval.tool_call_id, call.tool_call_id);

        let other_session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("other".to_string()),
            })
            .expect("create other session");
        let other_run = storage
            .create_run(NewRun {
                session_id: other_session.session_id,
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create other run")
            .expect("other run exists");
        let mismatch = storage.create_approval(NewApproval {
            run_id: other_run.run_id,
            tool_call_id: Some(call.tool_call_id),
            kind: "exec".to_string(),
            request_summary: "bad link".to_string(),
            request_json: r#"{"command":"echo nope"}"#.to_string(),
        });
        assert!(mismatch.is_err());
    }

    #[test]
    fn auth_profile_crud_and_order_work() {
        let (_temp_dir, storage) = test_storage();
        let profile_a = storage
            .create_auth_profile(NewAuthProfile {
                provider: "openai".to_string(),
                display_name: "openai-primary".to_string(),
                auth_mode: "api_key".to_string(),
                risk_level: "low".to_string(),
                enabled: true,
                kill_switch_scope: "none".to_string(),
                api_base_url: Some("https://api.openai.com".to_string()),
                credentials_json: r#"{"api_key":"redacted"}"#.to_string(),
            })
            .expect("create profile a");
        let profile_b = storage
            .create_auth_profile(NewAuthProfile {
                provider: "openai".to_string(),
                display_name: "openai-oauth".to_string(),
                auth_mode: "openai_oauth".to_string(),
                risk_level: "medium".to_string(),
                enabled: true,
                kill_switch_scope: "none".to_string(),
                api_base_url: Some("https://api.openai.com".to_string()),
                credentials_json: r#"{"refresh_token":"redacted"}"#.to_string(),
            })
            .expect("create profile b");

        let listed = storage
            .list_auth_profiles(Some("openai"), false)
            .expect("list openai profiles");
        assert_eq!(listed.len(), 2);
        assert!(listed
            .iter()
            .any(|item| item.auth_profile_id == profile_a.auth_profile_id));
        assert!(listed
            .iter()
            .any(|item| item.auth_profile_id == profile_b.auth_profile_id));

        let order = storage
            .set_agent_provider_profile_order(
                "default",
                "openai",
                &[
                    profile_b.auth_profile_id.clone(),
                    profile_a.auth_profile_id.clone(),
                ],
            )
            .expect("set order");
        assert_eq!(
            order,
            vec![
                profile_b.auth_profile_id.clone(),
                profile_a.auth_profile_id.clone()
            ]
        );

        let loaded_order = storage
            .list_agent_provider_profile_order("default", "openai")
            .expect("list order");
        assert_eq!(loaded_order, order);

        let updated = storage
            .update_auth_profile_state(
                &profile_b.auth_profile_id,
                Some(false),
                Some("profile".to_string()),
            )
            .expect("update profile b")
            .expect("profile exists");
        assert!(!updated.enabled);
        assert_eq!(updated.kill_switch_scope, "profile");

        let refreshed = storage
            .update_auth_profile_credentials(
                &profile_b.auth_profile_id,
                r#"{"access_token":"new-token","expires_at_unix":123}"#.to_string(),
            )
            .expect("update profile credentials")
            .expect("profile exists");
        assert!(refreshed
            .credentials_json
            .contains(r#""access_token":"new-token""#));

        let enabled_only = storage
            .list_auth_profiles(Some("openai"), false)
            .expect("list enabled profiles");
        assert_eq!(enabled_only.len(), 1);
        assert_eq!(enabled_only[0].auth_profile_id, profile_a.auth_profile_id);
    }

    #[test]
    fn auth_profile_kill_switch_queries_work() {
        let (_temp_dir, storage) = test_storage();

        let _ = storage
            .create_auth_profile(NewAuthProfile {
                provider: "anthropic".to_string(),
                display_name: "anthropic-test".to_string(),
                auth_mode: "claude_consumer_oauth".to_string(),
                risk_level: "high".to_string(),
                enabled: true,
                kill_switch_scope: "provider".to_string(),
                api_base_url: Some("https://api.anthropic.com".to_string()),
                credentials_json: r#"{"token":"redacted"}"#.to_string(),
            })
            .expect("create anthropic profile");
        assert!(storage
            .provider_kill_switch_active("anthropic")
            .expect("provider kill switch query"));
        assert!(!storage
            .global_kill_switch_active()
            .expect("global kill switch query"));

        let _ = storage
            .create_auth_profile(NewAuthProfile {
                provider: "openai".to_string(),
                display_name: "global-stop".to_string(),
                auth_mode: "api_key".to_string(),
                risk_level: "high".to_string(),
                enabled: true,
                kill_switch_scope: "global".to_string(),
                api_base_url: None,
                credentials_json: r#"{"api_key":"redacted"}"#.to_string(),
            })
            .expect("create global kill switch profile");

        assert!(storage
            .global_kill_switch_active()
            .expect("global kill switch query"));
        assert!(storage
            .provider_kill_switch_active("openai")
            .expect("openai provider kill switch query"));
    }

    #[test]
    fn app_kv_round_trip_updates_value_and_timestamp() {
        let (_temp_dir, storage) = test_storage();
        assert!(storage
            .get_app_kv_json("config.channels.discord")
            .expect("read missing kv")
            .is_none());

        let first_updated = storage
            .set_app_kv_json(
                "config.channels.discord",
                r#"{"require_mention_in_guild_channels":true,"allowlisted_user_ids":[]}"#
                    .to_string(),
            )
            .expect("set kv first");
        let first = storage
            .get_app_kv_json("config.channels.discord")
            .expect("load kv first")
            .expect("kv exists");
        assert!(first
            .0
            .contains("\"require_mention_in_guild_channels\":true"));
        assert_eq!(first.1, first_updated);

        let second_updated = storage
            .set_app_kv_json(
                "config.channels.discord",
                r#"{"require_mention_in_guild_channels":false,"allowlisted_user_ids":["u1"]}"#
                    .to_string(),
            )
            .expect("set kv second");
        let second = storage
            .get_app_kv_json("config.channels.discord")
            .expect("load kv second")
            .expect("kv exists");
        assert!(second
            .0
            .contains("\"require_mention_in_guild_channels\":false"));
        assert!(second.0.contains("\"u1\""));
        assert!(second_updated >= first_updated);
    }

    #[test]
    fn security_audit_event_round_trip_and_filters_work() {
        let (_temp_dir, storage) = test_storage();
        let created = storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "req-1".to_string(),
                correlation_id: "corr-1".to_string(),
                principal: "operator_admin:test".to_string(),
                action: "auth.profile.update".to_string(),
                resource: "auth_profile:abc".to_string(),
                decision: "allow".to_string(),
                reason: Some("state update".to_string()),
                transport: "http".to_string(),
                status: "200".to_string(),
                error_code: None,
                session_id: Some("session-1".to_string()),
                run_id: None,
                metadata_json: Some(r#"{"kill_switch_scope":"profile"}"#.to_string()),
            })
            .expect("append audit event");
        assert_eq!(created.request_id, "req-1");
        let denied = storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "req-2".to_string(),
                correlation_id: "corr-2".to_string(),
                principal: "operator_readonly:test".to_string(),
                action: "approval.resolve".to_string(),
                resource: "approval:123".to_string(),
                decision: "deny".to_string(),
                reason: Some("role mismatch".to_string()),
                transport: "http".to_string(),
                status: "403".to_string(),
                error_code: Some("AUTH_ROLE_MISMATCH".to_string()),
                session_id: None,
                run_id: Some("run-1".to_string()),
                metadata_json: Some(r#"{"allowed_roles":["operator_admin"]}"#.to_string()),
            })
            .expect("append denied audit event");

        let by_id = storage
            .get_security_audit_event(&created.event_id)
            .expect("get audit event by id")
            .expect("event exists");
        assert_eq!(by_id.action, "auth.profile.update");

        let listed = storage
            .list_security_audit_events(
                20,
                &SecurityAuditEventListFilter {
                    action: Some("auth.profile.update".to_string()),
                    ..SecurityAuditEventListFilter::default()
                },
            )
            .expect("list audit events");
        assert!(!listed.is_empty());
        assert_eq!(listed[0].action, "auth.profile.update");

        let principal_filtered = storage
            .list_security_audit_events(
                20,
                &SecurityAuditEventListFilter {
                    principal: Some("operator_admin:test".to_string()),
                    ..SecurityAuditEventListFilter::default()
                },
            )
            .expect("list principal filtered");
        assert!(!principal_filtered.is_empty());
        assert_eq!(principal_filtered[0].principal, "operator_admin:test");

        let deny_filtered = storage
            .list_security_audit_events(
                20,
                &SecurityAuditEventListFilter {
                    decision: Some("deny".to_string()),
                    status: Some("403".to_string()),
                    error_code: Some("AUTH_ROLE_MISMATCH".to_string()),
                    created_after: Some(created.created_at),
                    created_before: Some(denied.created_at),
                    ..SecurityAuditEventListFilter::default()
                },
            )
            .expect("list deny filtered");
        assert_eq!(deny_filtered.len(), 1);
        assert_eq!(deny_filtered[0].event_id, denied.event_id);
    }

    #[test]
    fn init_upgrades_legacy_db_to_current_schema() {
        let temp_dir = TempDir::new().expect("tempdir");
        let paths = AppPaths::from_root(temp_dir.path().to_path_buf());
        std::fs::create_dir_all(&paths.root).expect("create root");
        let conn = Connection::open(&paths.db_path).expect("open legacy db");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS agents (
              agent_id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              workspace_root TEXT NOT NULL,
              model_provider TEXT NOT NULL,
              model_id TEXT NOT NULL,
              tool_profile TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );
            "#,
        )
        .expect("seed legacy schema");
        drop(conn);

        init(&paths).expect("upgrade init");
        let conn = Connection::open(&paths.db_path).expect("open upgraded db");
        for table in [
            "auth_profiles",
            "notes",
            "embeddings",
            "jobs",
            "job_runs",
            "security_audit_events",
        ] {
            let exists = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1 LIMIT 1",
                    params![table],
                    |_| Ok(()),
                )
                .optional()
                .expect("query sqlite_master")
                .is_some();
            assert!(exists, "expected migrated table: {}", table);
        }
    }

    #[test]
    fn note_crud_and_embedding_search_work() {
        let (_temp_dir, storage) = test_storage();
        let first = storage
            .create_note(NewNote {
                title: Some("Fruits".to_string()),
                body: "Alice likes apples and bananas".to_string(),
                tags_json: r#"["food","fruit"]"#.to_string(),
            })
            .expect("create first note");
        let second = storage
            .create_note(NewNote {
                title: Some("Vehicles".to_string()),
                body: "Bob prefers electric cars".to_string(),
                tags_json: r#"["transport"]"#.to_string(),
            })
            .expect("create second note");

        storage
            .replace_note_embeddings(
                &first.note_id,
                "test-embed-v1",
                &[("fruit chunk".to_string(), vec![1.0, 0.0, 0.0])],
            )
            .expect("replace first embeddings");
        storage
            .replace_note_embeddings(
                &second.note_id,
                "test-embed-v1",
                &[("vehicle chunk".to_string(), vec![0.0, 1.0, 0.0])],
            )
            .expect("replace second embeddings");

        let search = storage
            .search_note_embeddings(&[0.95, 0.02, 0.0], 4, 20)
            .expect("search embeddings");
        assert_eq!(search.len(), 2);
        assert_eq!(search[0].note_id, first.note_id);
        assert!(search[0].score > search[1].score);

        let updated = storage
            .update_note(
                &first.note_id,
                Some("Fruit Preferences".to_string()),
                Some("Alice really likes apples".to_string()),
                Some(r#"["food","memory"]"#.to_string()),
            )
            .expect("update note")
            .expect("note exists");
        assert_eq!(updated.title.as_deref(), Some("Fruit Preferences"));
        assert!(updated.body.contains("likes apples"));

        let listed = storage.list_notes(10).expect("list notes");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].note_id, updated.note_id);
    }

    #[test]
    fn replace_note_embeddings_overwrites_existing_chunks() {
        let (_temp_dir, storage) = test_storage();
        let note = storage
            .create_note(NewNote {
                title: Some("Overwrite".to_string()),
                body: "Initial body".to_string(),
                tags_json: "[]".to_string(),
            })
            .expect("create note");

        storage
            .replace_note_embeddings(
                &note.note_id,
                "test-embed-v1",
                &[
                    ("old chunk a".to_string(), vec![1.0, 0.0]),
                    ("old chunk b".to_string(), vec![0.8, 0.2]),
                ],
            )
            .expect("insert old embeddings");
        let old_results = storage
            .search_note_embeddings(&[1.0, 0.0], 10, 20)
            .expect("old search");
        assert!(old_results.iter().any(|item| item.snippet == "old chunk a"));

        storage
            .replace_note_embeddings(
                &note.note_id,
                "test-embed-v1",
                &[("new chunk".to_string(), vec![0.0, 1.0])],
            )
            .expect("replace with new embedding");
        let new_results = storage
            .search_note_embeddings(&[0.0, 1.0], 10, 20)
            .expect("new search");
        assert_eq!(new_results.len(), 1);
        assert_eq!(new_results[0].snippet, "new chunk");
    }

    #[test]
    fn job_crud_and_run_history_work() {
        let (_temp_dir, storage) = test_storage();
        let now = now_ms();
        let job = storage
            .create_job(NewJob {
                agent_id: "default".to_string(),
                name: "test-job".to_string(),
                enabled: true,
                schedule_kind: "interval".to_string(),
                interval_seconds: Some(60),
                run_at_ms: None,
                next_run_at: Some(now),
                payload_json: r#"{"mode":"noop"}"#.to_string(),
                max_retries: 2,
                retry_backoff_ms: 250,
                timeout_ms: 2000,
            })
            .expect("create job");
        assert_eq!(job.name, "test-job");
        assert!(job.enabled);

        let listed = storage.list_jobs(20, true).expect("list jobs");
        assert!(listed.iter().any(|item| item.job_id == job.job_id));
        assert_eq!(storage.jobs_total_count().expect("total count"), 1);
        assert_eq!(storage.jobs_enabled_count().expect("enabled count"), 1);
        assert!(storage.jobs_due_count(now).expect("due count") >= 1);

        let updated = storage
            .update_job(
                &job.job_id,
                JobUpdatePatch {
                    name: Some("updated-job".to_string()),
                    enabled: Some(true),
                    interval_seconds: Some(120),
                    run_at_ms: None,
                    next_run_at: Some(now + 120_000),
                    payload_json: Some(r#"{"mode":"updated"}"#.to_string()),
                    max_retries: Some(3),
                    retry_backoff_ms: Some(400),
                    timeout_ms: Some(3000),
                },
            )
            .expect("update job")
            .expect("job exists");
        assert_eq!(updated.name, "updated-job");
        assert_eq!(updated.interval_seconds, Some(120));
        assert_eq!(updated.max_retries, 3);

        let acquired = storage
            .acquire_due_jobs("worker-a", now + 120_001, 30_000, 10)
            .expect("acquire due jobs");
        assert_eq!(acquired.len(), 1);
        assert_eq!(acquired[0].job_id, job.job_id);

        let run = storage
            .create_job_run(&job.job_id, "scheduler", 1)
            .expect("create job run")
            .expect("run exists");
        assert_eq!(run.status, "running");
        let finished = storage
            .finish_job_run_success(
                &job.job_id,
                &run.job_run_id,
                1,
                r#"{"ok":true}"#.to_string(),
                Some(now + 240_000),
                false,
            )
            .expect("finish job run")
            .expect("finished run exists");
        assert_eq!(finished.status, "succeeded");

        let history = storage
            .list_job_runs(&job.job_id, 10)
            .expect("list job history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "succeeded");

        let removed = storage.remove_job(&job.job_id).expect("remove job");
        assert!(removed);
        assert!(storage.get_job(&job.job_id).expect("get job").is_none());
    }

    #[test]
    fn failed_job_run_updates_last_error_and_releases_lease() {
        let (_temp_dir, storage) = test_storage();
        let now = now_ms();
        let job = storage
            .create_job(NewJob {
                agent_id: "default".to_string(),
                name: "fail-job".to_string(),
                enabled: true,
                schedule_kind: "interval".to_string(),
                interval_seconds: Some(15),
                run_at_ms: None,
                next_run_at: Some(now),
                payload_json: r#"{"mode":"fail"}"#.to_string(),
                max_retries: 1,
                retry_backoff_ms: 1000,
                timeout_ms: 1000,
            })
            .expect("create job");

        let acquired = storage
            .acquire_due_jobs("worker-fail", now + 1, 30_000, 5)
            .expect("acquire");
        assert_eq!(acquired.len(), 1);

        let run = storage
            .create_job_run(&job.job_id, "manual", 2)
            .expect("create run")
            .expect("run exists");
        let _ = storage
            .finish_job_run_failed(
                &job.job_id,
                &run.job_run_id,
                2,
                "intentional failure".to_string(),
                Some(now + 5_000),
            )
            .expect("finish failed run");

        let refreshed = storage
            .get_job(&job.job_id)
            .expect("get job")
            .expect("job exists");
        assert_eq!(refreshed.last_error.as_deref(), Some("intentional failure"));
        assert!(refreshed.lease_owner.is_none());
        assert!(refreshed.lease_expires_at.is_none());
    }
}
