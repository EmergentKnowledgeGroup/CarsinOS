use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod execass;

#[derive(Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub attachments_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl fmt::Debug for AppPaths {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppPaths")
            .field("root_configured", &(!self.root.as_os_str().is_empty()))
            .field(
                "database_configured",
                &(!self.db_path.as_os_str().is_empty()),
            )
            .field(
                "attachments_configured",
                &(!self.attachments_dir.as_os_str().is_empty()),
            )
            .field("logs_configured", &(!self.logs_dir.as_os_str().is_empty()))
            .finish()
    }
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
    seed_default_entities(&paths.db_path)?;
    harden_permissions(paths)?;
    Ok(())
}

pub const EXECASS_APPLICATION_ID: i64 = 1_163_411_761;
pub const EXECASS_SCHEMA_VERSION: i64 = 8;
const EXECASS_REQUIRED_TABLES: &[&str] = &[
    "execass_accepted_confirmation_grants",
    "execass_action_branches",
    "execass_amendment_criteria_links",
    "execass_attention_items",
    "execass_authority_links",
    "execass_authority_parent_bindings",
    "execass_authority_provenance",
    "execass_channel_reply_bindings",
    "execass_completion_assessments",
    "execass_confirmation_attestations",
    "execass_confirmation_authority_keys",
    "execass_confirmation_challenge_alternatives",
    "execass_confirmation_challenges",
    "execass_continuation_operation_history",
    "execass_continuations",
    "execass_criteria_sets",
    "execass_decisions",
    "execass_delegations",
    "execass_duplicate_risk_bindings",
    "execass_duplicate_risk_successors",
    "execass_effect_recorder_evidence",
    "execass_effect_recorder_keys",
    "execass_effect_tombstones",
    "execass_external_waits",
    "execass_global_runtime_control",
    "execass_lifecycle_transitions",
    "execass_logical_effects",
    "execass_notifications",
    "execass_outbox_cursors",
    "execass_outbox_events",
    "execass_outcome_criteria",
    "execass_owner_ingress_bindings",
    "execass_plan_amendments",
    "execass_plans",
    "execass_policy_revisions",
    "execass_provider_attempts",
    "execass_receipt_anchor_state",
    "execass_receipt_evidence_refs",
    "execass_receipt_journal_state",
    "execass_receipt_keys",
    "execass_receipt_recorder_evidence_refs",
    "execass_receipts",
    "execass_recovery_episodes",
    "execass_recovery_evaluations",
    "execass_routine_driver_jobs",
    "execass_routine_job_bindings",
    "execass_routine_occurrences",
    "execass_routine_schedule_state",
    "execass_routine_trigger_operations",
    "execass_routine_versions",
    "execass_routines",
    "execass_run_control_attestations",
    "execass_runtime_host_generations",
    "execass_runtime_host_leases",
    "execass_runtime_host_states",
    "execass_runtime_settings_revisions",
    "execass_schema_metadata",
    "execass_summary_acknowledgements",
    "execass_summary_deliveries",
    "execass_summary_delivery_items",
    "execass_technical_resource_actuals",
    "execass_technical_resource_quota_entries",
    "execass_technical_resource_quota_snapshots",
    "execass_technical_resource_requirement_sets",
    "execass_technical_resource_requirements",
    "execass_technical_resource_reservations",
    "execass_terminal_corrections",
    "execass_verifier_results",
];

/// Installs the incompatible ExecAss v1 schema into a brand-new state root.
///
/// This is deliberately separate from [`init`]: ordinary legacy startup stays
/// on schema version 6 until the offline schema-replacement cutover exists.
/// An already-installed exact ExecAss v1 root is accepted idempotently; every
/// other pre-existing database or non-empty state root is rejected before any
/// mutation.
pub fn init_execass_fresh_root(paths: &AppPaths) -> Result<()> {
    if paths.db_path.exists() {
        upgrade_execass_canonical_root_if_needed(paths)?;
        if execass_schema_is_exact(&paths.db_path)? {
            seed_default_entities(&paths.db_path)?;
            harden_permissions(paths)?;
            return Ok(());
        }
        bail!("ExecAss clean-root initialization refused a pre-existing database");
    }

    ensure_execass_root_is_fresh(paths)?;
    ensure_dirs(paths)?;

    let install_result = (|| -> Result<()> {
        let mut conn = open_sqlite_connection(&paths.db_path)?;
        install_execass_schema(&mut conn, MIGRATION_0007)?;
        verify_execass_schema(&conn)?;
        drop(conn);
        seed_default_entities(&paths.db_path)?;
        harden_permissions(paths)
    })();

    if let Err(error) = install_result {
        // The file was created by this call after a strict fresh-root check.
        // Removing the rolled-back empty database keeps a corrected retry safe.
        let _ = std::fs::remove_file(&paths.db_path);
        return Err(error);
    }

    Ok(())
}

type MigrationAppliedCheck = fn(&Connection) -> Result<bool>;

fn ensure_dirs(paths: &AppPaths) -> Result<()> {
    std::fs::create_dir_all(&paths.root).context("failed to create state root")?;
    std::fs::create_dir_all(&paths.attachments_dir)
        .context("failed to create attachments directory")?;
    std::fs::create_dir_all(&paths.logs_dir).context("failed to create logs directory")?;
    Ok(())
}

fn migrate(db_path: &Path) -> Result<()> {
    let mut conn = open_sqlite_connection(db_path)?;
    ensure_schema_migrations_table(&conn)?;
    apply_sql_migration(&mut conn, 1, MIGRATION_0001, "initial schema", None)?;
    apply_sql_migration(
        &mut conn,
        2,
        MIGRATION_0002,
        "strategy phase 1 schema",
        None,
    )?;
    apply_sql_migration(
        &mut conn,
        3,
        MIGRATION_0003,
        "strategy hierarchy cleanup",
        Some(migration_0003_already_applied),
    )?;
    apply_sql_migration(
        &mut conn,
        4,
        MIGRATION_0004,
        "assistant memory binding schema",
        Some(migration_0004_already_applied),
    )?;
    apply_sql_migration(
        &mut conn,
        5,
        MIGRATION_0005,
        "connector registry schema",
        None,
    )?;
    apply_sql_migration(
        &mut conn,
        6,
        MIGRATION_0006,
        "agent archival schema",
        Some(migration_0006_already_applied),
    )?;
    Ok(())
}

fn seed_default_entities(db_path: &Path) -> Result<()> {
    let mut conn = open_sqlite_connection(db_path)?;
    let tx = conn
        .transaction()
        .context("failed to start default-entity seed transaction")?;
    let now = now_ms();
    let workspace_root = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let (agent_id, name) = ("default", "Default Agent");
    tx.execute(
        r#"
        INSERT OR IGNORE INTO agents
          (
            agent_id,
            name,
            workspace_root,
            model_provider,
            model_id,
            tool_profile,
            reports_to_agent_id,
            role_label,
            created_at,
            updated_at
          )
        VALUES
          (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?8)
        "#,
        params![
            agent_id,
            name,
            workspace_root,
            "unconfigured",
            "unconfigured",
            "default",
            now,
            now
        ],
    )
    .with_context(|| format!("failed to seed {agent_id} agent"))?;
    tx.execute(
        r#"
        UPDATE agents
           SET workspace_root = ?1,
               updated_at = ?2
         WHERE agent_id = ?3
           AND archived_at IS NULL
           AND workspace_root != ?1
        "#,
        params![workspace_root, now, agent_id],
    )
    .with_context(|| format!("failed to refresh {agent_id} agent workspace"))?;

    seed_default_boards(&tx, now)?;
    tx.commit()
        .context("failed to commit default-entity seed transaction")?;
    Ok(())
}

fn open_sqlite_connection(db_path: &Path) -> Result<Connection> {
    let conn =
        Connection::open(db_path).context("failed to open the configured sqlite database")?;
    execass::register_recorder_evidence_sql_verifier(&conn)
        .context("failed registering the ExecAss recorder evidence verifier")?;
    conn.busy_timeout(Duration::from_secs(5))
        .context("failed setting the configured sqlite busy timeout")?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("failed enabling configured sqlite foreign keys")?;
    Ok(conn)
}

fn open_sqlite_connection_read_only(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("failed to inspect the configured sqlite database")?;
    execass::register_recorder_evidence_sql_verifier(&conn)
        .context("failed registering the read-only ExecAss recorder evidence verifier")?;
    conn.busy_timeout(Duration::from_secs(5))
        .context("failed setting the configured sqlite busy timeout")?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("failed enabling configured sqlite foreign keys")?;
    Ok(conn)
}

fn ensure_execass_root_is_fresh(paths: &AppPaths) -> Result<()> {
    if !paths.root.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&paths.root).context("failed to inspect ExecAss state root")? {
        let entry = entry.context("failed to inspect ExecAss state-root entry")?;
        let path = entry.path();
        let is_expected_empty_directory = (path == paths.attachments_dir || path == paths.logs_dir)
            && path.is_dir()
            && std::fs::read_dir(&path)
                .context("failed to inspect an expected ExecAss state directory")?
                .next()
                .is_none();
        if !is_expected_empty_directory {
            bail!("ExecAss clean-root initialization refused a non-empty state root entry");
        }
    }
    Ok(())
}

fn execass_schema_is_exact(db_path: &Path) -> Result<bool> {
    let conn = open_sqlite_connection_read_only(db_path)?;
    execass_connection_schema_is_exact(&conn)
}

fn execass_connection_schema_is_exact(conn: &Connection) -> Result<bool> {
    if verify_execass_schema(conn).is_err() {
        return Ok(false);
    }

    let actual = sqlite_schema_inventory(conn)?;
    let mut canonical =
        Connection::open_in_memory().context("failed opening canonical ExecAss schema database")?;
    execass::register_recorder_evidence_sql_verifier(&canonical)
        .context("failed registering the canonical ExecAss recorder evidence verifier")?;
    canonical
        .pragma_update(None, "foreign_keys", "ON")
        .context("failed enabling canonical ExecAss foreign keys")?;
    install_execass_schema(&mut canonical, MIGRATION_0007)
        .context("failed building canonical ExecAss schema")?;
    let expected = sqlite_schema_inventory(&canonical)?;
    Ok(actual == expected)
}

pub(crate) fn upgrade_execass_canonical_root_if_needed(paths: &AppPaths) -> Result<()> {
    if !paths.db_path.is_file() {
        return Ok(());
    }
    let mut conn = open_sqlite_connection(&paths.db_path)?;
    let application_id = conn.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))?;
    if application_id != EXECASS_APPLICATION_ID {
        return Ok(());
    }
    let user_version = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
    if user_version != 7 {
        return Ok(());
    }
    let tx = conn.transaction()?;
    let versions = tx
        .prepare("SELECT version FROM schema_migrations ORDER BY version")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let valid_v7 = versions == [1, 2, 3, 4, 5, 6, 7]
        && tx.query_row(
            "SELECT 1 FROM execass_schema_metadata WHERE singleton=1 AND application_id=?1 AND schema_version=7 AND contract_id='carsinos.execass.contract' AND contract_version='v1'",
            params![EXECASS_APPLICATION_ID], |_| Ok(())).optional()?.is_some();
    if !valid_v7 {
        bail!("refusing to upgrade a claimed but non-canonical ExecAss v7 database");
    }
    tx.execute_batch(MIGRATION_0008)
        .context("failed applying ExecAss v8 Glass Office upgrade")?;
    tx.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (8, ?1)",
        params![now_ms()],
    )?;
    tx.commit()?;
    Ok(())
}

fn sqlite_schema_inventory(conn: &Connection) -> Result<Vec<(String, String, String)>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT type, name, sql
            FROM sqlite_schema
            WHERE type IN ('table', 'index', 'trigger', 'view')
              AND sql IS NOT NULL
              AND name NOT LIKE 'sqlite_%'
            ORDER BY type, name
            "#,
        )
        .context("failed preparing SQLite schema inventory")?;
    let inventory = stmt
        .query_map([], |row| {
            let sql = row.get::<_, String>(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                sql.split_whitespace().collect::<Vec<_>>().join(" "),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed collecting SQLite schema inventory")?;
    Ok(inventory)
}

fn install_execass_schema(conn: &mut Connection, execass_sql: &str) -> Result<()> {
    let tx = conn
        .transaction()
        .context("failed to start ExecAss clean-root schema transaction")?;
    tx.execute_batch(
        r#"
        CREATE TABLE schema_migrations (
          version INTEGER PRIMARY KEY,
          applied_at INTEGER NOT NULL
        );
        "#,
    )
    .context("failed creating clean-root schema migration ledger")?;

    for (version, sql, label) in [
        (1, MIGRATION_0001, "initial authority schema"),
        (2, MIGRATION_0002, "strategy authority schema"),
        (3, MIGRATION_0003, "strategy hierarchy cleanup"),
        (4, MIGRATION_0004, "assistant memory binding schema"),
        (5, MIGRATION_0005, "connector registry schema"),
        (6, MIGRATION_0006, "agent archival schema"),
        (7, execass_sql, "ExecAss incompatible replacement schema"),
        (8, MIGRATION_0008, "Glass Office projections"),
    ] {
        tx.execute_batch(sql).with_context(|| {
            format!("failed installing clean-root migration {version:04} ({label})")
        })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, now_ms()],
        )
        .with_context(|| format!("failed recording clean-root migration {version:04}"))?;
    }

    tx.commit()
        .context("failed to commit ExecAss clean-root schema transaction")?;
    Ok(())
}

fn verify_execass_schema(conn: &Connection) -> Result<()> {
    let application_id = conn
        .query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
        .context("failed reading ExecAss application_id")?;
    if application_id != EXECASS_APPLICATION_ID {
        bail!(
            "unexpected ExecAss application_id: expected {}, found {}",
            EXECASS_APPLICATION_ID,
            application_id
        );
    }

    let user_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .context("failed reading ExecAss user_version")?;
    if user_version != EXECASS_SCHEMA_VERSION {
        bail!(
            "unexpected ExecAss schema version: expected {}, found {}",
            EXECASS_SCHEMA_VERSION,
            user_version
        );
    }

    let metadata_matches = conn
        .query_row(
            r#"
            SELECT 1
            FROM execass_schema_metadata
            WHERE singleton = 1
              AND application_id = ?1
              AND schema_version = ?2
              AND contract_id = 'carsinos.execass.contract'
              AND contract_version = 'v1'
            LIMIT 1
            "#,
            params![EXECASS_APPLICATION_ID, EXECASS_SCHEMA_VERSION],
            |_| Ok(()),
        )
        .optional()
        .context("failed reading ExecAss schema metadata")?
        .is_some();
    if !metadata_matches {
        bail!("ExecAss schema metadata is missing or incompatible");
    }

    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .context("failed reading ExecAss migration ledger")?;
    let versions = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if versions != [1, 2, 3, 4, 5, 6, 7, EXECASS_SCHEMA_VERSION] {
        bail!("ExecAss migration ledger is not the exact installed schema");
    }

    let mut table_stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'execass_%' ORDER BY name",
        )
        .context("failed reading ExecAss table inventory")?;
    let tables = table_stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !tables
        .iter()
        .map(String::as_str)
        .eq(EXECASS_REQUIRED_TABLES.iter().copied())
    {
        bail!("ExecAss table inventory is not the exact installed schema");
    }

    let retired_authority_exists = conn
        .query_row(
            r#"
            SELECT 1
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN ('approvals', 'assistant_workers', 'assistant_task_links')
            LIMIT 1
            "#,
            [],
            |_| Ok(()),
        )
        .optional()
        .context("failed checking retired orchestration authorities")?
        .is_some();
    if retired_authority_exists {
        bail!("retired orchestration approval authority exists in ExecAss schema");
    }

    let foreign_key_violation = conn
        .query_row("SELECT 1 FROM pragma_foreign_key_check LIMIT 1", [], |_| {
            Ok(())
        })
        .optional()
        .context("failed checking ExecAss foreign keys")?
        .is_some();
    if foreign_key_violation {
        bail!("ExecAss schema contains foreign-key violations");
    }
    Ok(())
}

fn ensure_schema_migrations_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
          version INTEGER PRIMARY KEY,
          applied_at INTEGER NOT NULL
        );
        "#,
    )
    .context("failed ensuring schema_migrations table")?;
    Ok(())
}

fn apply_sql_migration(
    conn: &mut Connection,
    version: i64,
    sql: &str,
    label: &str,
    already_applied: Option<MigrationAppliedCheck>,
) -> Result<()> {
    if migration_recorded(conn, version)? {
        return Ok(());
    }
    if let Some(check) = already_applied {
        if check(conn)? {
            record_migration(conn, version)?;
            return Ok(());
        }
    }
    let tx = conn
        .transaction()
        .with_context(|| format!("failed to start migration {version:04} transaction"))?;
    tx.execute_batch(sql)
        .with_context(|| format!("failed applying migration {version:04} ({label})"))?;
    record_migration(&tx, version)?;
    tx.commit()
        .with_context(|| format!("failed to commit migration {version:04} ({label})"))?;
    Ok(())
}

fn migration_recorded(conn: &Connection, version: i64) -> Result<bool> {
    let recorded = conn
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = ?1 LIMIT 1",
            params![version],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(recorded)
}

fn record_migration(conn: &Connection, version: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
        params![version, now_ms()],
    )
    .with_context(|| format!("failed recording migration {version:04}"))?;
    Ok(())
}

fn migration_0003_already_applied(conn: &Connection) -> Result<bool> {
    Ok(column_exists(conn, "agents", "reports_to_agent_id")?
        && column_exists(conn, "agents", "role_label")?
        && !bootstrap_preset_manager_has_agent_fk(conn)?)
}

fn migration_0004_already_applied(conn: &Connection) -> Result<bool> {
    Ok(column_exists(conn, "agents", "memory_binding_id")?
        && column_exists(conn, "agents", "memory_provider_kind")?
        && column_exists(conn, "agents", "memory_base_url")?
        && column_exists(conn, "agents", "memory_auth_mode")?
        && column_exists(conn, "agents", "memory_auth_secret_ref")?
        && column_exists(conn, "agents", "memory_principal_id")?
        && column_exists(conn, "agents", "memory_principal_display_name")?
        && column_exists(conn, "agents", "memory_enabled")?
        && column_exists(conn, "agents", "memory_trusted_local_operator_actions")?)
}

fn migration_0006_already_applied(conn: &Connection) -> Result<bool> {
    column_exists(conn, "agents", "archived_at")
}

fn bootstrap_preset_manager_has_agent_fk(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_list(bootstrap_presets)")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(2)?, row.get::<_, String>(3)?))
    })?;
    for row in rows {
        let (table_name, from_column) = row?;
        if table_name.eq_ignore_ascii_case("agents")
            && from_column.eq_ignore_ascii_case("default_reports_to_agent_id")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({table_name})");
    let mut stmt = conn.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row?.eq_ignore_ascii_case(column_name) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn seed_default_boards(conn: &Transaction<'_>, now: i64) -> Result<()> {
    let tasks_board_id = upsert_board(conn, "tasks", "Tasks", "tasks", now)?;
    let content_board_id = upsert_board(conn, "content", "Content Pipeline", "content", now)?;

    for (position, (column_key, name)) in [
        ("backlog", "Backlog"),
        ("in_progress", "In Progress"),
        ("review", "Review"),
        ("done", "Done"),
    ]
    .into_iter()
    .enumerate()
    {
        upsert_board_column(
            conn,
            &tasks_board_id,
            column_key,
            name,
            position as i64,
            now,
        )?;
    }

    for (position, (column_key, name)) in [
        ("ideas", "Ideas"),
        ("scripting", "Scripting"),
        ("thumbnail", "Thumbnail"),
        ("filming", "Filming"),
        ("editing", "Editing"),
        ("published", "Published"),
    ]
    .into_iter()
    .enumerate()
    {
        upsert_board_column(
            conn,
            &content_board_id,
            column_key,
            name,
            position as i64,
            now,
        )?;
    }

    Ok(())
}

fn upsert_board(
    conn: &Transaction<'_>,
    board_key: &str,
    name: &str,
    board_type: &str,
    now: i64,
) -> Result<String> {
    let board_id: Option<String> = conn
        .query_row(
            "SELECT board_id FROM boards WHERE board_key = ?1",
            params![board_key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(board_id) = board_id {
        conn.execute(
            r#"
            UPDATE boards
            SET name = ?1, board_type = ?2, updated_at = ?3, archived_at = NULL
            WHERE board_id = ?4
            "#,
            params![name, board_type, now, board_id],
        )?;
        Ok(board_id)
    } else {
        let board_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO boards (
              board_id, board_key, name, board_type, created_at, updated_at, archived_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
            "#,
            params![board_id, board_key, name, board_type, now, now],
        )?;
        Ok(board_id)
    }
}

fn upsert_board_column(
    conn: &Transaction<'_>,
    board_id: &str,
    column_key: &str,
    name: &str,
    position: i64,
    now: i64,
) -> Result<String> {
    let column_id: Option<String> = conn
        .query_row(
            "SELECT column_id FROM board_columns WHERE board_id = ?1 AND column_key = ?2",
            params![board_id, column_key],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(column_id) = column_id {
        conn.execute(
            r#"
            UPDATE board_columns
            SET name = ?1, position = ?2, updated_at = ?3, archived_at = NULL
            WHERE column_id = ?4
            "#,
            params![name, position, now, column_id],
        )?;
        Ok(column_id)
    } else {
        let column_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO board_columns (
              column_id, board_id, column_key, name, position, created_at, updated_at, archived_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
            "#,
            params![column_id, board_id, column_key, name, position, now, now],
        )?;
        Ok(column_id)
    }
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
pub struct AssistantWorkerRecord {
    pub boss_key: String,
    pub root_session_id: String,
    pub worker_key: String,
    pub worker_kind: String,
    pub status: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub template_key: String,
    pub display_name: String,
    pub instructions: Option<String>,
    pub run_defaults_json: String,
    pub session_mode: String,
    pub last_run_id: Option<String>,
    pub last_run_status: Option<String>,
    pub last_stop_reason: Option<String>,
    pub pending_approval_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewAssistantWorker {
    pub boss_key: String,
    pub root_session_id: String,
    pub worker_key: String,
    pub worker_kind: String,
    pub status: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub template_key: String,
    pub display_name: String,
    pub instructions: Option<String>,
    pub run_defaults_json: String,
    pub session_mode: String,
    pub pending_approval_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AssistantWorkerPatch {
    pub status: Option<String>,
    pub agent_id: Option<Option<String>>,
    pub session_id: Option<Option<String>>,
    pub template_key: Option<String>,
    pub display_name: Option<String>,
    pub instructions: Option<Option<String>>,
    pub run_defaults_json: Option<String>,
    pub session_mode: Option<String>,
    pub last_run_id: Option<Option<String>>,
    pub last_run_status: Option<Option<String>>,
    pub last_stop_reason: Option<Option<String>>,
    pub pending_approval_id: Option<Option<String>>,
    pub archived_at: Option<Option<i64>>,
}

#[derive(Debug, Clone)]
pub struct NewAssistantToolCallAudit {
    pub request_id: String,
    pub boss_key: String,
    pub root_session_id: String,
    pub root_run_id: Option<String>,
    pub caller_agent_id: String,
    pub tool_name: String,
    pub decision: String,
    pub reason_code: Option<String>,
    pub audit_ref: Option<String>,
    pub metadata_json: Option<String>,
}

/// Exact persisted identity returned by assistant tool-call audit insertion.
/// Callers that need lineage must retain this ID; querying a "latest" audit
/// event is intentionally not an API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantToolCallAuditRecord {
    pub event_id: String,
    pub request_id: String,
    pub created_at: i64,
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
    pub schedule_kind: Option<String>,
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
pub struct AgentMemoryBindingRecord {
    pub binding_id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub auth_mode: String,
    pub auth_secret_ref: Option<String>,
    pub principal_id: Option<String>,
    pub principal_display_name: Option<String>,
    pub enabled: bool,
    pub trusted_local_operator_actions: bool,
}

#[derive(Debug, Clone)]
pub struct NewAgentMemoryBinding {
    pub binding_id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub auth_mode: String,
    pub auth_secret_ref: Option<String>,
    pub principal_id: Option<String>,
    pub principal_display_name: Option<String>,
    pub enabled: bool,
    pub trusted_local_operator_actions: bool,
}

#[derive(Debug, Clone)]
pub struct AgentMemoryBindingUpdatePatch {
    pub binding_id: Option<String>,
    pub provider_kind: Option<String>,
    pub base_url: Option<String>,
    pub auth_mode: Option<String>,
    pub auth_secret_ref: Option<Option<String>>,
    pub principal_id: Option<Option<String>>,
    pub principal_display_name: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub trusted_local_operator_actions: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub agent_id: String,
    pub name: String,
    pub workspace_root: String,
    pub model_provider: String,
    pub model_id: String,
    pub tool_profile: String,
    pub reports_to_agent_id: Option<String>,
    pub role_label: Option<String>,
    pub memory_binding: Option<AgentMemoryBindingRecord>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewAgent {
    pub agent_id: String,
    pub name: String,
    pub workspace_root: String,
    pub model_provider: String,
    pub model_id: String,
    pub tool_profile: String,
    pub reports_to_agent_id: Option<String>,
    pub role_label: Option<String>,
    pub memory_binding: Option<NewAgentMemoryBinding>,
}

#[derive(Debug, Clone)]
pub struct AgentUpdatePatch {
    pub name: Option<String>,
    pub workspace_root: Option<String>,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub tool_profile: Option<String>,
    pub reports_to_agent_id: Option<Option<String>>,
    pub role_label: Option<Option<String>>,
    pub memory_binding: Option<Option<AgentMemoryBindingUpdatePatch>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveAgentOutcome {
    Removed,
    NotFound,
    InvalidAgentId,
    HasSessions,
    HasReferences,
}

#[derive(Debug, Clone)]
pub struct GoalRecord {
    pub goal_id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub owner_agent_id: Option<String>,
    pub target_date: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewGoal {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub owner_agent_id: Option<String>,
    pub target_date: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GoalUpdatePatch {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub target_date: Option<Option<i64>>,
}

#[derive(Debug, Clone)]
pub struct ProjectRecord {
    pub project_id: String,
    pub goal_id: String,
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub status: String,
    pub owner_agent_id: Option<String>,
    pub workspace_root: Option<String>,
    pub budget_month_usd: Option<f64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub goal_id: String,
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub status: String,
    pub owner_agent_id: Option<String>,
    pub workspace_root: Option<String>,
    pub budget_month_usd: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ProjectUpdatePatch {
    pub goal_id: Option<String>,
    pub slug: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub workspace_root: Option<Option<String>>,
    pub budget_month_usd: Option<Option<f64>>,
}

#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub task_id: String,
    pub project_id: String,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub priority: String,
    pub owner_agent_id: Option<String>,
    pub due_at: Option<i64>,
    pub blocked_reason: Option<String>,
    pub linked_board_card_id: Option<String>,
    pub linked_job_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct TaskRuntimeLinkRecord {
    pub latest_run_id: Option<String>,
    pub latest_session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTask {
    pub project_id: String,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub priority: String,
    pub owner_agent_id: Option<String>,
    pub due_at: Option<i64>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskUpdatePatch {
    pub project_id: Option<String>,
    pub parent_task_id: Option<Option<String>>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub due_at: Option<Option<i64>>,
    pub blocked_reason: Option<Option<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalListFilter {
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectListFilter {
    pub goal_id: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskListFilter {
    pub goal_id: Option<String>,
    pub project_id: Option<String>,
    pub status: Option<String>,
    pub owner_agent_id: Option<String>,
    pub query: Option<String>,
    pub stale: Option<bool>,
    pub blocked: Option<bool>,
    pub unassigned: Option<bool>,
    pub hierarchy_root_agent_id: Option<String>,
    pub hierarchy_scope: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
    pub sort: Option<String>,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct BootstrapPresetRecord {
    pub preset_key: String,
    pub display_name: String,
    pub description: String,
    pub role_label: String,
    pub provider_path: String,
    pub default_model_provider: Option<String>,
    pub default_model_id: Option<String>,
    pub default_tool_profile: Option<String>,
    pub default_workspace_root: Option<String>,
    pub default_reports_to_agent_id: Option<String>,
    pub setup_notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewBootstrapPreset {
    pub preset_key: String,
    pub display_name: String,
    pub description: String,
    pub role_label: String,
    pub provider_path: String,
    pub default_model_provider: Option<String>,
    pub default_model_id: Option<String>,
    pub default_tool_profile: Option<String>,
    pub default_workspace_root: Option<String>,
    pub default_reports_to_agent_id: Option<String>,
    pub setup_notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BootstrapPresetUpdatePatch {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub role_label: Option<String>,
    pub provider_path: Option<String>,
    pub default_model_provider: Option<Option<String>>,
    pub default_model_id: Option<Option<String>>,
    pub default_tool_profile: Option<Option<String>>,
    pub default_workspace_root: Option<Option<String>>,
    pub default_reports_to_agent_id: Option<Option<String>>,
    pub setup_notes: Option<Option<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct BootstrapPresetListFilter {
    pub query: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConnectorSourceRecord {
    pub connector_id: String,
    pub slug: String,
    pub display_name: String,
    pub source_kind: String,
    pub origin_kind: String,
    pub catalog_item_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_imported_version_id: Option<String>,
    pub status: String,
    pub trust_state: String,
    pub assigned_agent_count: usize,
    pub published_tool_count: usize,
    pub last_conversion_at: Option<i64>,
    pub last_review_at: Option<i64>,
    pub last_enabled_at: Option<i64>,
    pub last_disabled_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ConnectorVersionRecord {
    pub version_id: String,
    pub connector_id: String,
    pub version_label: String,
    pub source_digest: String,
    pub raw_source_location: Option<String>,
    pub import_metadata_json: String,
    pub schema_summary_json: String,
    pub latest_conversion_id: Option<String>,
    pub external_reference_policy: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ConnectorConversionRecord {
    pub conversion_id: String,
    pub connector_id: String,
    pub version_id: String,
    pub status: String,
    pub warnings_json: String,
    pub proposed_tools_json: String,
    pub write_capable_tools: usize,
    pub unsupported_operations_json: String,
    pub normalization_notes_json: String,
    pub diff_from_previous_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ConnectorPublishedToolRecord {
    pub published_tool_id: String,
    pub connector_id: String,
    pub version_id: String,
    pub conversion_id: String,
    pub tool_name: String,
    pub display_name: String,
    pub tool_schema_json: String,
    pub origin_metadata_json: String,
    pub write_classification: String,
    pub published_at: i64,
    pub unpublished_at: Option<i64>,
    pub superseded_by_published_tool_id: Option<String>,
    pub deprecation_state: String,
}

#[derive(Debug, Clone)]
pub struct ConnectorAssignmentRecord {
    pub assignment_id: String,
    pub connector_id: String,
    pub agent_id: String,
    pub enabled: bool,
    pub auth_mode: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ConnectorAuthBindingRecord {
    pub auth_binding_id: String,
    pub connector_id: String,
    pub agent_id: Option<String>,
    pub auth_kind: String,
    pub secret_ref: Option<String>,
    pub oauth_session_id: Option<String>,
    pub status: String,
    pub auth_metadata_json: String,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_rotated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ConnectorInteractionRecord {
    pub interaction_id: String,
    pub connector_id: String,
    pub agent_id: Option<String>,
    pub interaction_kind: String,
    pub status: String,
    pub prompt_summary: String,
    pub resume_token: Option<String>,
    pub expires_at: Option<i64>,
    pub consumed_at: Option<i64>,
    pub detail_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectorListFilter {
    pub source_kind: Option<String>,
    pub status: Option<String>,
    pub trust_state: Option<String>,
    pub query: Option<String>,
    pub include_disabled: bool,
}

#[derive(Debug, Clone)]
pub struct NewConnectorImport {
    pub display_name: String,
    pub slug: String,
    pub source_kind: String,
    pub origin_kind: String,
    pub catalog_item_id: Option<String>,
    pub version_label: String,
    pub source_digest: String,
    pub raw_source_location: Option<String>,
    pub import_metadata_json: String,
    pub schema_summary_json: String,
    pub external_reference_policy: String,
    pub trust_state: String,
}

#[derive(Debug, Clone)]
pub struct NewConnectorConversion {
    pub status: String,
    pub warnings_json: String,
    pub proposed_tools_json: String,
    pub write_capable_tools: usize,
    pub unsupported_operations_json: String,
    pub normalization_notes_json: String,
    pub diff_from_previous_json: String,
}

#[derive(Debug, Clone)]
pub struct NewConnectorPublishedTool {
    pub tool_name: String,
    pub display_name: String,
    pub tool_schema_json: String,
    pub origin_metadata_json: String,
    pub write_classification: String,
}

#[derive(Debug, Clone)]
pub struct NewConnectorAssignment {
    pub agent_id: String,
    pub enabled: bool,
    pub auth_mode: String,
}

#[derive(Debug, Clone)]
pub struct NewConnectorAuthBinding {
    pub agent_id: Option<String>,
    pub auth_kind: String,
    pub secret_ref: Option<String>,
    pub oauth_session_id: Option<String>,
    pub status: String,
    pub auth_metadata_json: String,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_rotated_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewConnectorInteraction {
    pub agent_id: Option<String>,
    pub interaction_kind: String,
    pub status: String,
    pub prompt_summary: String,
    pub resume_token: Option<String>,
    pub expires_at: Option<i64>,
    pub detail_json: String,
}

#[derive(Debug, Clone)]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BoardRecord {
    pub board_id: String,
    pub board_key: String,
    pub name: String,
    pub board_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BoardColumnRecord {
    pub column_id: String,
    pub board_id: String,
    pub column_key: String,
    pub name: String,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BoardCardRecord {
    pub card_id: String,
    pub board_id: String,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub owner_kind: String,
    pub owner_agent_id: Option<String>,
    pub owner_human_id: Option<String>,
    pub due_at: Option<i64>,
    pub tags_json: Option<String>,
    pub script_markdown: Option<String>,
    pub linked_session_id: Option<String>,
    pub latest_run_id: Option<String>,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BoardCardAssetRecord {
    pub card_asset_id: String,
    pub card_id: String,
    pub filename: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: i64,
    pub local_path: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewBoardCard {
    pub board_id: String,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub owner_kind: String,
    pub owner_agent_id: Option<String>,
    pub owner_human_id: Option<String>,
    pub due_at: Option<i64>,
    pub tags_json: Option<String>,
    pub script_markdown: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BoardCardUpdatePatch {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub owner_kind: Option<String>,
    pub owner_agent_id: Option<Option<String>>,
    pub owner_human_id: Option<Option<String>>,
    pub due_at: Option<Option<i64>>,
    pub tags_json: Option<Option<String>>,
    pub script_markdown: Option<Option<String>>,
}

#[derive(Debug, Clone)]
pub struct NewBoardCardAsset {
    pub card_id: String,
    pub filename: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: i64,
    pub local_path: String,
}

#[derive(Debug, Clone)]
pub struct AgentMailThreadRecord {
    pub thread_id: String,
    pub kind: String,
    pub subject: String,
    pub created_by_principal: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AgentMailThreadParticipantRecord {
    pub thread_id: String,
    pub principal_id: String,
    pub role: String,
    pub joined_at: i64,
    pub last_read_at: Option<i64>,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct AgentMailMessageRecord {
    pub message_id: String,
    pub thread_id: String,
    pub sender_principal: String,
    pub sender_kind: String,
    pub body_text: String,
    pub metadata_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct OfficeFloorPresenceRecord {
    pub agent_id: String,
    pub display_name: String,
    pub state: String,
    pub observed_at: Option<i64>,
    pub target_run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OfficeChatterMessageRecord {
    pub message_id: String,
    pub thread_id: String,
    pub source_kind: String,
    pub event_name: Option<String>,
    pub delegation_id: String,
    pub revision: Option<i64>,
    pub body_text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct OfficeChatterRoomRecord {
    pub thread_id: String,
    pub delegation_id: String,
    pub safe_label: String,
    pub last_activity_at: i64,
}

#[derive(Debug, Clone)]
pub struct AgentMailMessageRecipientRecord {
    pub message_id: String,
    pub recipient_principal: String,
    pub delivered_at: i64,
    pub acked_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AgentMailAttachmentRecord {
    pub attachment_id: String,
    pub message_id: String,
    pub filename: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: i64,
    pub local_path: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AgentMailThreadSummaryRecord {
    pub thread: AgentMailThreadRecord,
    pub participant_count: i64,
    pub message_count: i64,
    pub latest_message_at: Option<i64>,
    pub latest_message_preview: Option<String>,
    pub latest_sender_principal: Option<String>,
    pub unread_count: i64,
}

#[derive(Debug, Clone)]
pub struct NewAgentMailThread {
    pub kind: String,
    pub subject: String,
    pub created_by_principal: String,
    pub participants: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct NewAgentMailMessage {
    pub thread_id: String,
    pub sender_principal: String,
    pub sender_kind: String,
    pub body_text: String,
    pub metadata_json: Option<String>,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NewAgentMailAttachment {
    pub message_id: String,
    pub filename: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: i64,
    pub local_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct AgentMailThreadListFilter {
    pub kind: Option<String>,
    pub principal_id: Option<String>,
    pub mailbox: Option<String>,
    pub search_text: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct AgentMailFileLeaseRecord {
    pub lease_id: String,
    pub holder_principal: String,
    pub glob_pattern: String,
    pub exclusive: bool,
    pub ttl_ms: i64,
    pub note: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub released_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewAgentMailFileLease {
    pub holder_principal: String,
    pub glob_pattern: String,
    pub exclusive: bool,
    pub ttl_ms: i64,
    pub note: Option<String>,
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

#[derive(Debug, Clone)]
pub struct DailyAuthProfileUsageRecord {
    pub usage_day_utc: String,
    pub auth_profile_id: String,
    pub provider: String,
    pub input_chars: i64,
    pub output_chars: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DailyAuthProfileUsageIncrement {
    pub usage_day_utc: String,
    pub auth_profile_id: String,
    pub provider: String,
    pub input_chars: i64,
    pub output_chars: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone)]
pub struct RunUsageSampleRecord {
    pub run_id: String,
    pub session_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub model_provider: String,
    pub model_id: String,
    pub usage_json: String,
    pub sample_ts_ms: i64,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerStateRecord {
    pub breaker_key: String,
    pub scope: String,
    pub target_id: String,
    pub state: String,
    pub consecutive_failures: i64,
    pub opened_at: Option<i64>,
    pub cooldown_until: Option<i64>,
    pub last_error_code: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerStateUpsert {
    pub scope: String,
    pub target_id: String,
    pub state: String,
    pub consecutive_failures: i64,
    pub opened_at: Option<i64>,
    pub cooldown_until: Option<i64>,
    pub last_error_code: Option<String>,
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

    pub fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              agent_id, name, workspace_root, model_provider, model_id, tool_profile,
              reports_to_agent_id, role_label,
              memory_binding_id, memory_provider_kind, memory_base_url, memory_auth_mode,
              memory_auth_secret_ref, memory_principal_id, memory_principal_display_name,
              memory_enabled, memory_trusted_local_operator_actions,
              created_at, updated_at
            FROM agents
            WHERE archived_at IS NULL
            ORDER BY updated_at DESC, agent_id ASC
            "#,
        )?;
        let rows = stmt.query_map([], map_agent_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_agent(&self, agent_id: &str) -> Result<Option<AgentRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              agent_id, name, workspace_root, model_provider, model_id, tool_profile,
              reports_to_agent_id, role_label,
              memory_binding_id, memory_provider_kind, memory_base_url, memory_auth_mode,
              memory_auth_secret_ref, memory_principal_id, memory_principal_display_name,
              memory_enabled, memory_trusted_local_operator_actions,
              created_at, updated_at
            FROM agents
            WHERE agent_id = ?1
              AND archived_at IS NULL
            "#,
        )?;
        Ok(stmt
            .query_row(params![agent_id], map_agent_row)
            .optional()?)
    }

    pub fn create_agent(&self, new_agent: NewAgent) -> Result<AgentRecord> {
        let conn = self.connect()?;
        validate_agent_manager_assignment(
            &conn,
            &new_agent.agent_id,
            new_agent.reports_to_agent_id.as_deref(),
        )?;
        let memory_binding = normalize_new_agent_memory_binding(
            new_agent.memory_binding,
            &new_agent.agent_id,
            &new_agent.name,
        )?;
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO agents (
              agent_id, name, workspace_root, model_provider, model_id, tool_profile,
              reports_to_agent_id, role_label,
              memory_binding_id, memory_provider_kind, memory_base_url, memory_auth_mode,
              memory_auth_secret_ref, memory_principal_id, memory_principal_display_name,
              memory_enabled, memory_trusted_local_operator_actions,
              created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            "#,
            params![
                new_agent.agent_id,
                new_agent.name,
                new_agent.workspace_root,
                new_agent.model_provider,
                new_agent.model_id,
                new_agent.tool_profile,
                new_agent.reports_to_agent_id,
                new_agent.role_label,
                memory_binding.as_ref().map(|binding| binding.binding_id.as_str()),
                memory_binding
                    .as_ref()
                    .map(|binding| binding.provider_kind.as_str()),
                memory_binding.as_ref().map(|binding| binding.base_url.as_str()),
                memory_binding.as_ref().map(|binding| binding.auth_mode.as_str()),
                memory_binding
                    .as_ref()
                    .and_then(|binding| binding.auth_secret_ref.as_deref()),
                memory_binding
                    .as_ref()
                    .and_then(|binding| binding.principal_id.as_deref()),
                memory_binding
                    .as_ref()
                    .and_then(|binding| binding.principal_display_name.as_deref()),
                memory_binding
                    .as_ref()
                    .map(|binding| i64::from(binding.enabled))
                    .unwrap_or(0),
                memory_binding
                    .as_ref()
                    .map(|binding| i64::from(binding.trusted_local_operator_actions))
                    .unwrap_or(0),
                now,
                now
            ],
        )
        .context("failed to create agent")?;
        self.get_agent(&new_agent.agent_id)?
            .context("created agent could not be reloaded")
    }

    pub fn update_agent(
        &self,
        agent_id: &str,
        patch: AgentUpdatePatch,
    ) -> Result<Option<AgentRecord>> {
        if patch.name.is_none()
            && patch.workspace_root.is_none()
            && patch.model_provider.is_none()
            && patch.model_id.is_none()
            && patch.tool_profile.is_none()
            && patch.reports_to_agent_id.is_none()
            && patch.role_label.is_none()
            && patch.memory_binding.is_none()
        {
            return self.get_agent(agent_id);
        }

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match get_agent_with_conn(&tx, agent_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let next_reports_to_agent_id = patch
            .reports_to_agent_id
            .clone()
            .unwrap_or(current.reports_to_agent_id.clone());
        validate_agent_manager_assignment(&tx, agent_id, next_reports_to_agent_id.as_deref())?;
        let next_memory_binding = normalize_updated_agent_memory_binding(
            current.memory_binding.clone(),
            patch.memory_binding.clone(),
            agent_id,
            patch.name.as_deref().unwrap_or(current.name.as_str()),
        )?;
        let now = now_ms();
        tx.execute(
            r#"
            UPDATE agents
            SET name = COALESCE(?1, name),
                workspace_root = COALESCE(?2, workspace_root),
                model_provider = COALESCE(?3, model_provider),
                model_id = COALESCE(?4, model_id),
                tool_profile = COALESCE(?5, tool_profile),
                reports_to_agent_id = ?6,
                role_label = ?7,
                memory_binding_id = ?8,
                memory_provider_kind = ?9,
                memory_base_url = ?10,
                memory_auth_mode = ?11,
                memory_auth_secret_ref = ?12,
                memory_principal_id = ?13,
                memory_principal_display_name = ?14,
                memory_enabled = ?15,
                memory_trusted_local_operator_actions = ?16,
                updated_at = ?17
            WHERE agent_id = ?18
            "#,
            params![
                patch.name,
                patch.workspace_root,
                patch.model_provider,
                patch.model_id,
                patch.tool_profile,
                next_reports_to_agent_id,
                patch
                    .role_label
                    .clone()
                    .unwrap_or(current.role_label.clone()),
                next_memory_binding
                    .as_ref()
                    .map(|binding| binding.binding_id.as_str()),
                next_memory_binding
                    .as_ref()
                    .map(|binding| binding.provider_kind.as_str()),
                next_memory_binding
                    .as_ref()
                    .map(|binding| binding.base_url.as_str()),
                next_memory_binding
                    .as_ref()
                    .map(|binding| binding.auth_mode.as_str()),
                next_memory_binding
                    .as_ref()
                    .and_then(|binding| binding.auth_secret_ref.as_deref()),
                next_memory_binding
                    .as_ref()
                    .and_then(|binding| binding.principal_id.as_deref()),
                next_memory_binding
                    .as_ref()
                    .and_then(|binding| binding.principal_display_name.as_deref()),
                next_memory_binding
                    .as_ref()
                    .map(|binding| i64::from(binding.enabled))
                    .unwrap_or(0),
                next_memory_binding
                    .as_ref()
                    .map(|binding| i64::from(binding.trusted_local_operator_actions))
                    .unwrap_or(0),
                now,
                agent_id
            ],
        )
        .context("failed to update agent")?;
        tx.commit().context("failed to commit agent update")?;
        self.get_agent(agent_id)
    }

    pub fn remove_agent(&self, agent_id: &str) -> Result<RemoveAgentOutcome> {
        let agent_id = agent_id.trim().to_string();
        if agent_id.is_empty() {
            return Ok(RemoveAgentOutcome::InvalidAgentId);
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let exists = tx
            .query_row(
                "SELECT 1 FROM agents WHERE agent_id = ?1 AND archived_at IS NULL LIMIT 1",
                params![agent_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(RemoveAgentOutcome::NotFound);
        }

        let session_refs: i64 = tx.query_row(
            "SELECT COUNT(1) FROM sessions WHERE agent_id = ?1",
            params![agent_id.as_str()],
            |row| row.get(0),
        )?;

        let job_refs: i64 = tx.query_row(
            "SELECT COUNT(1) FROM jobs WHERE agent_id = ?1 AND deleted_at IS NULL",
            params![agent_id.as_str()],
            |row| row.get(0),
        )?;
        if job_refs > 0 {
            return Ok(RemoveAgentOutcome::HasReferences);
        }

        let now = now_ms();
        tx.execute(
            "UPDATE agents SET reports_to_agent_id = NULL, updated_at = ?1 WHERE reports_to_agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE goals SET owner_agent_id = NULL, updated_at = ?1 WHERE owner_agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE projects SET owner_agent_id = NULL, updated_at = ?1 WHERE owner_agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE tasks SET owner_agent_id = NULL, updated_at = ?1 WHERE owner_agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE bootstrap_presets SET default_reports_to_agent_id = NULL, updated_at = ?1 WHERE default_reports_to_agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE board_cards SET owner_kind = 'unassigned', owner_agent_id = NULL, updated_at = ?1 WHERE owner_agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE assistant_workers SET agent_id = NULL WHERE agent_id = ?1",
            params![agent_id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM agent_provider_profile_order WHERE agent_id = ?1",
            params![agent_id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM routing_rules WHERE agent_id = ?1",
            params![agent_id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM connector_assignments WHERE agent_id = ?1",
            params![agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE connector_auth_bindings SET agent_id = NULL, updated_at = ?1 WHERE agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        tx.execute(
            "UPDATE connector_interactions SET agent_id = NULL, updated_at = ?1 WHERE agent_id = ?2",
            params![now, agent_id.as_str()],
        )?;
        let removed = if session_refs > 0 {
            tx.execute(
                "UPDATE agents SET reports_to_agent_id = NULL, archived_at = ?1, updated_at = ?1 WHERE agent_id = ?2",
                params![now, agent_id.as_str()],
            )?
        } else {
            tx.execute(
                "DELETE FROM agents WHERE agent_id = ?1",
                params![agent_id.as_str()],
            )?
        };
        tx.commit()?;
        if removed > 0 {
            Ok(RemoveAgentOutcome::Removed)
        } else {
            Ok(RemoveAgentOutcome::NotFound)
        }
    }

    pub fn list_boards(&self) -> Result<Vec<BoardRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              board_id, board_key, name, board_type, created_at, updated_at, archived_at
            FROM boards
            WHERE archived_at IS NULL
            ORDER BY board_type ASC, updated_at DESC, name ASC
            "#,
        )?;
        let rows = stmt.query_map([], map_board_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_board(&self, board_id: &str) -> Result<Option<BoardRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              board_id, board_key, name, board_type, created_at, updated_at, archived_at
            FROM boards
            WHERE board_id = ?1
              AND archived_at IS NULL
            "#,
        )?;
        Ok(stmt
            .query_row(params![board_id], map_board_row)
            .optional()?)
    }

    pub fn list_board_columns(&self, board_id: &str) -> Result<Vec<BoardColumnRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              column_id, board_id, column_key, name, position, created_at, updated_at, archived_at
            FROM board_columns
            WHERE board_id = ?1
              AND archived_at IS NULL
            ORDER BY position ASC, updated_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![board_id], map_board_column_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_board_cards(&self, board_id: &str) -> Result<Vec<BoardCardRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              c.card_id, c.board_id, c.column_id, c.title, c.description, c.owner_kind, c.owner_agent_id, c.owner_human_id,
              c.due_at, c.tags_json, c.script_markdown, c.linked_session_id, c.latest_run_id, c.position, c.created_at, c.updated_at, c.archived_at
            FROM board_cards c
            JOIN board_columns bc
              ON bc.column_id = c.column_id
             AND bc.board_id = c.board_id
             AND bc.archived_at IS NULL
            WHERE c.board_id = ?1
              AND c.archived_at IS NULL
            ORDER BY bc.position ASC, c.position ASC, c.updated_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![board_id], map_board_card_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_board_card(&self, card_id: &str) -> Result<Option<BoardCardRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              card_id, board_id, column_id, title, description, owner_kind, owner_agent_id, owner_human_id,
              due_at, tags_json, script_markdown, linked_session_id, latest_run_id, position, created_at, updated_at, archived_at
            FROM board_cards
            WHERE card_id = ?1
              AND archived_at IS NULL
            "#,
        )?;
        Ok(stmt
            .query_row(params![card_id], map_board_card_row)
            .optional()?)
    }

    pub fn get_board_card_in_board(
        &self,
        board_id: &str,
        card_id: &str,
    ) -> Result<Option<BoardCardRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              card_id, board_id, column_id, title, description, owner_kind, owner_agent_id, owner_human_id,
              due_at, tags_json, script_markdown, linked_session_id, latest_run_id, position, created_at, updated_at, archived_at
            FROM board_cards
            WHERE card_id = ?1
              AND board_id = ?2
              AND archived_at IS NULL
            "#,
        )?;
        Ok(stmt
            .query_row(params![card_id, board_id], map_board_card_row)
            .optional()?)
    }

    fn get_board_card_tx(tx: &Transaction<'_>, card_id: &str) -> Result<Option<BoardCardRecord>> {
        let mut stmt = tx.prepare(
            r#"
            SELECT
              card_id, board_id, column_id, title, description, owner_kind, owner_agent_id, owner_human_id,
              due_at, tags_json, script_markdown, linked_session_id, latest_run_id, position, created_at, updated_at, archived_at
            FROM board_cards
            WHERE card_id = ?1
              AND archived_at IS NULL
            "#,
        )?;
        Ok(stmt
            .query_row(params![card_id], map_board_card_row)
            .optional()?)
    }

    pub fn create_board_card(&self, new_card: NewBoardCard) -> Result<BoardCardRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        self.ensure_board_exists(&tx, &new_card.board_id)?;
        self.ensure_column_in_board(&tx, &new_card.board_id, &new_card.column_id)?;
        if new_card.owner_kind == "agent" {
            if let Some(agent_id) = new_card.owner_agent_id.as_ref() {
                self.ensure_agent_exists(&tx, agent_id)?;
            } else {
                anyhow::bail!("owner_agent_id is required when owner_kind=agent");
            }
        }
        let max_position: i64 = tx.query_row(
            r#"
                SELECT COALESCE(MAX(position), -1)
                FROM board_cards
                WHERE board_id = ?1
                  AND column_id = ?2
                  AND archived_at IS NULL
                "#,
            params![new_card.board_id, new_card.column_id],
            |row| row.get(0),
        )?;
        let position = max_position.saturating_add(1);
        let now = now_ms();
        let card_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            r#"
            INSERT INTO board_cards (
              card_id, board_id, column_id, title, description, owner_kind, owner_agent_id, owner_human_id,
              due_at, tags_json, script_markdown, linked_session_id, latest_run_id, position, created_at, updated_at, archived_at
            ) VALUES (
              ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
              ?9, ?10, ?11, NULL, NULL, ?12, ?13, ?14, NULL
            )
            "#,
            params![
                card_id,
                new_card.board_id,
                new_card.column_id,
                new_card.title,
                new_card.description,
                new_card.owner_kind,
                new_card.owner_agent_id,
                new_card.owner_human_id,
                new_card.due_at,
                new_card.tags_json,
                new_card.script_markdown,
                position,
                now,
                now
            ],
        )
        .context("failed to create board card")?;
        tx.execute(
            "UPDATE boards SET updated_at = ?1 WHERE board_id = ?2",
            params![now, new_card.board_id],
        )?;
        tx.commit()?;
        self.get_board_card(&card_id)?
            .context("created board card could not be reloaded")
    }

    pub fn update_board_card(
        &self,
        board_id: &str,
        card_id: &str,
        patch: BoardCardUpdatePatch,
    ) -> Result<Option<BoardCardRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match Self::get_board_card_tx(&tx, card_id)? {
            Some(card) => card,
            None => return Ok(None),
        };
        if current.board_id != board_id {
            anyhow::bail!("card does not belong to board");
        }
        let next_title = patch.title.unwrap_or(current.title);
        let next_description = patch.description.unwrap_or(current.description);
        let next_owner_kind = patch.owner_kind.unwrap_or(current.owner_kind);
        let next_owner_agent_id = patch.owner_agent_id.unwrap_or(current.owner_agent_id);
        let next_owner_human_id = patch.owner_human_id.unwrap_or(current.owner_human_id);
        let next_due_at = patch.due_at.unwrap_or(current.due_at);
        let next_tags_json = patch.tags_json.unwrap_or(current.tags_json);
        let next_script_markdown = patch.script_markdown.unwrap_or(current.script_markdown);
        if next_owner_kind == "agent" {
            if let Some(agent_id) = next_owner_agent_id.as_ref() {
                self.ensure_agent_exists(&tx, agent_id)?;
            } else {
                anyhow::bail!("owner_agent_id is required when owner_kind=agent");
            }
        }
        let now = now_ms();
        let updated_rows = tx.execute(
            r#"
            UPDATE board_cards
            SET title = ?1, description = ?2, owner_kind = ?3, owner_agent_id = ?4, owner_human_id = ?5,
                due_at = ?6, tags_json = ?7, script_markdown = ?8, updated_at = ?9
            WHERE card_id = ?10
              AND board_id = ?11
              AND archived_at IS NULL
            "#,
            params![
                next_title,
                next_description,
                next_owner_kind,
                next_owner_agent_id,
                next_owner_human_id,
                next_due_at,
                next_tags_json,
                next_script_markdown,
                now,
                card_id,
                board_id
            ],
        )?;
        if updated_rows == 0 {
            return Ok(None);
        }
        tx.execute(
            "UPDATE boards SET updated_at = ?1 WHERE board_id = ?2",
            params![now, board_id],
        )?;
        tx.commit()?;
        self.get_board_card_in_board(board_id, card_id)
    }

    pub fn move_board_card(
        &self,
        board_id: &str,
        card_id: &str,
        target_column_id: &str,
        before_card_id: Option<&str>,
    ) -> Result<Option<BoardCardRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match Self::get_board_card_tx(&tx, card_id)? {
            Some(card) => card,
            None => return Ok(None),
        };
        if current.board_id != board_id {
            anyhow::bail!("card does not belong to board");
        }
        if before_card_id == Some(card_id) {
            tx.commit()?;
            return Ok(Some(current));
        }
        self.ensure_column_in_board(&tx, board_id, target_column_id)?;
        let now = now_ms();
        let target_position = if let Some(before_card_id) = before_card_id {
            let maybe_position = tx
                .query_row(
                    r#"
                    SELECT position
                    FROM board_cards
                    WHERE card_id = ?1
                      AND board_id = ?2
                      AND column_id = ?3
                      AND archived_at IS NULL
                    "#,
                    params![before_card_id, board_id, target_column_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let position = maybe_position.with_context(|| {
                format!("before_card_id not found in target column: {before_card_id}")
            })?;
            tx.execute(
                r#"
                UPDATE board_cards
                SET position = position + 1, updated_at = ?1
                WHERE board_id = ?2
                  AND column_id = ?3
                  AND archived_at IS NULL
                  AND card_id != ?4
                  AND position >= ?5
                "#,
                params![now, board_id, target_column_id, card_id, position],
            )?;
            position
        } else {
            let max_position: i64 = tx.query_row(
                r#"
                    SELECT COALESCE(MAX(position), -1)
                    FROM board_cards
                    WHERE board_id = ?1
                      AND column_id = ?2
                      AND archived_at IS NULL
                      AND card_id != ?3
                    "#,
                params![board_id, target_column_id, card_id],
                |row| row.get(0),
            )?;
            max_position.saturating_add(1)
        };

        let updated_rows = tx.execute(
            r#"
            UPDATE board_cards
            SET column_id = ?1, position = ?2, updated_at = ?3
            WHERE card_id = ?4
              AND board_id = ?5
              AND archived_at IS NULL
            "#,
            params![target_column_id, target_position, now, card_id, board_id],
        )?;
        if updated_rows == 0 {
            return Ok(None);
        }
        tx.execute(
            "UPDATE boards SET updated_at = ?1 WHERE board_id = ?2",
            params![now, board_id],
        )?;
        tx.commit()?;
        self.get_board_card_in_board(board_id, card_id)
    }

    pub fn update_board_card_run_link(
        &self,
        board_id: &str,
        card_id: &str,
        linked_session_id: Option<&str>,
        latest_run_id: Option<&str>,
    ) -> Result<Option<BoardCardRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        let updated_rows = conn.execute(
            r#"
            UPDATE board_cards
            SET linked_session_id = ?1, latest_run_id = ?2, updated_at = ?3
            WHERE card_id = ?4
              AND board_id = ?5
              AND archived_at IS NULL
            "#,
            params![linked_session_id, latest_run_id, now, card_id, board_id],
        )?;
        if updated_rows == 0 {
            return Ok(None);
        }
        conn.execute(
            "UPDATE boards SET updated_at = ?1 WHERE board_id = ?2",
            params![now, board_id],
        )?;
        self.get_board_card_in_board(board_id, card_id)
    }

    pub fn create_board_card_asset(
        &self,
        new_asset: NewBoardCardAsset,
    ) -> Result<Option<BoardCardAssetRecord>> {
        let conn = self.connect()?;
        let card_asset_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let inserted_rows = conn.execute(
            r#"
            INSERT INTO board_card_assets (
              card_asset_id, card_id, filename, mime, sha256, bytes, local_path, created_at
            )
            SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
            FROM board_cards
            WHERE card_id = ?9
              AND archived_at IS NULL
            LIMIT 1
            "#,
            params![
                card_asset_id,
                new_asset.card_id,
                new_asset.filename,
                new_asset.mime,
                new_asset.sha256,
                new_asset.bytes,
                new_asset.local_path,
                now,
                new_asset.card_id
            ],
        )?;
        if inserted_rows == 0 {
            return Ok(None);
        }
        self.get_board_card_asset(&card_asset_id)
    }

    pub fn get_board_card_asset(
        &self,
        card_asset_id: &str,
    ) -> Result<Option<BoardCardAssetRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              card_asset_id, card_id, filename, mime, sha256, bytes, local_path, created_at
            FROM board_card_assets
            WHERE card_asset_id = ?1
            "#,
        )?;
        Ok(stmt
            .query_row(params![card_asset_id], map_board_card_asset_row)
            .optional()?)
    }

    pub fn list_board_card_assets(&self, card_id: &str) -> Result<Vec<BoardCardAssetRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              card_asset_id, card_id, filename, mime, sha256, bytes, local_path, created_at
            FROM board_card_assets
            WHERE card_id = ?1
            ORDER BY created_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![card_id], map_board_card_asset_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_goals(&self, filter: GoalListFilter) -> Result<PageResult<GoalRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              goal_id, slug, title, summary, status, owner_agent_id, target_date, created_at, updated_at
            FROM goals
            "#,
        )?;
        let rows = stmt.query_map([], map_goal_row)?;
        let mut items = Vec::new();
        for row in rows {
            let record = row?;
            if goal_matches_filter(&record, &filter) {
                items.push(record);
            }
        }
        sort_records_by_updated(&mut items, filter.sort.as_deref(), |item| {
            (item.updated_at, item.goal_id.as_str())
        })?;
        page_records(items, filter.limit, filter.cursor.as_deref(), |item| {
            (item.updated_at, item.goal_id.as_str())
        })
    }

    pub fn get_goal(&self, goal_id: &str) -> Result<Option<GoalRecord>> {
        let conn = self.connect()?;
        get_goal_with_conn(&conn, goal_id)
    }

    pub fn create_goal(&self, new_goal: NewGoal) -> Result<GoalRecord> {
        let conn = self.connect()?;
        let slug = normalize_management_slug(&new_goal.slug)?;
        validate_goal_status(&new_goal.status)?;
        validate_optional_owner_agent(self, &conn, new_goal.owner_agent_id.as_deref())?;
        let goal_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO goals (
              goal_id, slug, title, summary, status, owner_agent_id, target_date, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                goal_id,
                slug,
                new_goal.title.trim(),
                new_goal.summary.trim(),
                new_goal.status.trim(),
                new_goal.owner_agent_id,
                new_goal.target_date,
                now,
                now
            ],
        )
        .context("failed to create goal")?;
        self.get_goal(&goal_id)?
            .context("created goal could not be reloaded")
    }

    pub fn update_goal(&self, goal_id: &str, patch: GoalUpdatePatch) -> Result<Option<GoalRecord>> {
        if patch.slug.is_none()
            && patch.title.is_none()
            && patch.summary.is_none()
            && patch.status.is_none()
            && patch.owner_agent_id.is_none()
            && patch.target_date.is_none()
        {
            return self.get_goal(goal_id);
        }
        let conn = self.connect()?;
        let current = match get_goal_with_conn(&conn, goal_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let next_slug = match patch.slug {
            Some(value) => normalize_management_slug(&value)?,
            None => current.slug,
        };
        let next_title = patch.title.unwrap_or(current.title);
        let next_summary = patch.summary.unwrap_or(current.summary);
        let next_status = patch.status.unwrap_or(current.status);
        let next_owner_agent_id = patch.owner_agent_id.unwrap_or(current.owner_agent_id);
        let next_target_date = patch.target_date.unwrap_or(current.target_date);
        validate_goal_status(&next_status)?;
        validate_optional_owner_agent(self, &conn, next_owner_agent_id.as_deref())?;
        let now = now_ms();
        let updated = conn.execute(
            r#"
            UPDATE goals
            SET slug = ?1,
                title = ?2,
                summary = ?3,
                status = ?4,
                owner_agent_id = ?5,
                target_date = ?6,
                updated_at = ?7
            WHERE goal_id = ?8
            "#,
            params![
                next_slug,
                next_title.trim(),
                next_summary.trim(),
                next_status.trim(),
                next_owner_agent_id,
                next_target_date,
                now,
                goal_id
            ],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        self.get_goal(goal_id)
    }

    pub fn list_projects(&self, filter: ProjectListFilter) -> Result<PageResult<ProjectRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              project_id, goal_id, slug, name, summary, status, owner_agent_id, workspace_root,
              budget_month_usd, created_at, updated_at
            FROM projects
            "#,
        )?;
        let rows = stmt.query_map([], map_project_row)?;
        let mut items = Vec::new();
        for row in rows {
            let record = row?;
            if project_matches_filter(&record, &filter) {
                items.push(record);
            }
        }
        sort_records_by_updated(&mut items, filter.sort.as_deref(), |item| {
            (item.updated_at, item.project_id.as_str())
        })?;
        page_records(items, filter.limit, filter.cursor.as_deref(), |item| {
            (item.updated_at, item.project_id.as_str())
        })
    }

    pub fn get_project(&self, project_id: &str) -> Result<Option<ProjectRecord>> {
        let conn = self.connect()?;
        get_project_with_conn(&conn, project_id)
    }

    pub fn create_project(&self, new_project: NewProject) -> Result<ProjectRecord> {
        let conn = self.connect()?;
        ensure_goal_exists(&conn, &new_project.goal_id)?;
        validate_project_status(&new_project.status)?;
        validate_optional_owner_agent(self, &conn, new_project.owner_agent_id.as_deref())?;
        let workspace_root =
            normalize_project_workspace_root(new_project.workspace_root.as_deref())?;
        validate_budget_month_usd(new_project.budget_month_usd)?;
        let project_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO projects (
              project_id, goal_id, slug, name, summary, status, owner_agent_id, workspace_root,
              budget_month_usd, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                project_id,
                new_project.goal_id,
                normalize_management_slug(&new_project.slug)?,
                new_project.name.trim(),
                new_project.summary.trim(),
                new_project.status.trim(),
                new_project.owner_agent_id,
                workspace_root,
                new_project.budget_month_usd,
                now,
                now
            ],
        )
        .context("failed to create project")?;
        self.get_project(&project_id)?
            .context("created project could not be reloaded")
    }

    pub fn update_project(
        &self,
        project_id: &str,
        patch: ProjectUpdatePatch,
    ) -> Result<Option<ProjectRecord>> {
        if patch.goal_id.is_none()
            && patch.slug.is_none()
            && patch.name.is_none()
            && patch.summary.is_none()
            && patch.status.is_none()
            && patch.owner_agent_id.is_none()
            && patch.workspace_root.is_none()
            && patch.budget_month_usd.is_none()
        {
            return self.get_project(project_id);
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match get_project_with_conn(&tx, project_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let next_goal_id = patch.goal_id.unwrap_or(current.goal_id);
        ensure_goal_exists(&tx, &next_goal_id)?;
        let next_slug = match patch.slug {
            Some(value) => normalize_management_slug(&value)?,
            None => current.slug,
        };
        let next_name = patch.name.unwrap_or(current.name);
        let next_summary = patch.summary.unwrap_or(current.summary);
        let next_status = patch.status.unwrap_or(current.status);
        let next_owner_agent_id = patch.owner_agent_id.unwrap_or(current.owner_agent_id);
        let next_workspace_root = match patch.workspace_root {
            Some(value) => normalize_project_workspace_root(value.as_deref())?,
            None => current.workspace_root,
        };
        let next_budget_month_usd = patch.budget_month_usd.unwrap_or(current.budget_month_usd);
        validate_project_status(&next_status)?;
        validate_optional_owner_agent(self, &tx, next_owner_agent_id.as_deref())?;
        validate_budget_month_usd(next_budget_month_usd)?;
        if next_status == PROJECT_STATUS_COMPLETED && has_open_tasks_in_project(&tx, project_id)? {
            anyhow::bail!("project cannot be completed while it has open tasks");
        }
        let now = now_ms();
        let updated = tx.execute(
            r#"
            UPDATE projects
            SET goal_id = ?1,
                slug = ?2,
                name = ?3,
                summary = ?4,
                status = ?5,
                owner_agent_id = ?6,
                workspace_root = ?7,
                budget_month_usd = ?8,
                updated_at = ?9
            WHERE project_id = ?10
            "#,
            params![
                next_goal_id,
                next_slug,
                next_name.trim(),
                next_summary.trim(),
                next_status.trim(),
                next_owner_agent_id,
                next_workspace_root,
                next_budget_month_usd,
                now,
                project_id
            ],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        let updated_project = get_project_with_conn(&tx, project_id)?
            .context("updated project could not be reloaded")?;
        tx.commit().context("failed to commit project update")?;
        Ok(Some(updated_project))
    }

    pub fn list_tasks(&self, filter: TaskListFilter) -> Result<PageResult<TaskRecord>> {
        let conn = self.connect()?;
        let projects = load_all_projects(&conn)?;
        let project_goal_by_id = projects
            .iter()
            .map(|item| (item.project_id.clone(), item.goal_id.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let hierarchy_agent_ids =
            if let Some(root_agent_id) = filter.hierarchy_root_agent_id.as_ref() {
                Some(agent_subtree_ids(&conn, root_agent_id)?)
            } else {
                None
            };
        let mut stmt = conn.prepare(
            r#"
            SELECT
              task_id, project_id, parent_task_id, title, detail, status, priority, owner_agent_id,
              due_at, blocked_reason, linked_board_card_id, linked_job_id, created_at, updated_at
            FROM tasks
            "#,
        )?;
        let rows = stmt.query_map([], map_task_row)?;
        let mut items = Vec::new();
        for row in rows {
            let record = row?;
            if task_matches_filter(
                &record,
                &filter,
                &project_goal_by_id,
                hierarchy_agent_ids.as_ref(),
            ) {
                items.push(record);
            }
        }
        sort_records_by_updated(&mut items, filter.sort.as_deref(), |item| {
            (item.updated_at, item.task_id.as_str())
        })?;
        page_records(items, filter.limit, filter.cursor.as_deref(), |item| {
            (item.updated_at, item.task_id.as_str())
        })
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<TaskRecord>> {
        let conn = self.connect()?;
        get_task_with_conn(&conn, task_id)
    }

    pub fn create_task(&self, new_task: NewTask) -> Result<TaskRecord> {
        let NewTask {
            project_id,
            parent_task_id,
            title,
            detail,
            status,
            priority,
            owner_agent_id,
            due_at,
            blocked_reason,
        } = new_task;
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        ensure_project_accepts_task_changes(&tx, &project_id)?;
        validate_optional_owner_agent(self, &tx, owner_agent_id.as_deref())?;
        validate_task_status(&status)?;
        validate_task_priority(&priority)?;
        validate_task_parent(&tx, &project_id, parent_task_id.as_deref(), None)?;
        let blocked_reason = normalize_blocked_reason(&status, blocked_reason)?;
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        tx.execute(
            r#"
            INSERT INTO tasks (
              task_id, project_id, parent_task_id, title, detail, status, priority, owner_agent_id,
              due_at, blocked_reason, linked_board_card_id, linked_job_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, ?11, ?12)
            "#,
            params![
                task_id.as_str(),
                project_id,
                parent_task_id,
                title.trim(),
                detail.trim(),
                status.trim(),
                priority.trim(),
                owner_agent_id,
                due_at,
                blocked_reason,
                now,
                now
            ],
        )
        .context("failed to create task")?;
        let created =
            get_task_with_conn(&tx, &task_id)?.context("created task could not be reloaded")?;
        tx.commit().context("failed to commit task creation")?;
        Ok(created)
    }

    pub fn update_task(&self, task_id: &str, patch: TaskUpdatePatch) -> Result<Option<TaskRecord>> {
        if patch.project_id.is_none()
            && patch.parent_task_id.is_none()
            && patch.title.is_none()
            && patch.detail.is_none()
            && patch.status.is_none()
            && patch.priority.is_none()
            && patch.owner_agent_id.is_none()
            && patch.due_at.is_none()
            && patch.blocked_reason.is_none()
        {
            return self.get_task(task_id);
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match get_task_with_conn(&tx, task_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let next_project_id = patch
            .project_id
            .unwrap_or_else(|| current.project_id.clone());
        ensure_project_accepts_task_changes(&tx, &next_project_id)?;
        ensure_task_project_move_is_safe(&tx, task_id, &current.project_id, &next_project_id)?;
        let next_parent_task_id = patch.parent_task_id.unwrap_or(current.parent_task_id);
        let next_title = patch.title.unwrap_or(current.title);
        let next_detail = patch.detail.unwrap_or(current.detail);
        let next_status = patch.status.unwrap_or(current.status);
        let next_priority = patch.priority.unwrap_or(current.priority);
        let next_owner_agent_id = patch.owner_agent_id.unwrap_or(current.owner_agent_id);
        let next_due_at = patch.due_at.unwrap_or(current.due_at);
        let next_blocked_reason = normalize_blocked_reason(
            &next_status,
            patch.blocked_reason.unwrap_or(current.blocked_reason),
        )?;
        validate_optional_owner_agent(self, &tx, next_owner_agent_id.as_deref())?;
        validate_task_status(&next_status)?;
        validate_task_priority(&next_priority)?;
        validate_task_parent(
            &tx,
            &next_project_id,
            next_parent_task_id.as_deref(),
            Some(task_id),
        )?;
        let now = now_ms();
        let updated = tx.execute(
            r#"
            UPDATE tasks
            SET project_id = ?1,
                parent_task_id = ?2,
                title = ?3,
                detail = ?4,
                status = ?5,
                priority = ?6,
                owner_agent_id = ?7,
                due_at = ?8,
                blocked_reason = ?9,
                updated_at = ?10
            WHERE task_id = ?11
            "#,
            params![
                next_project_id,
                next_parent_task_id,
                next_title.trim(),
                next_detail.trim(),
                next_status.trim(),
                next_priority.trim(),
                next_owner_agent_id,
                next_due_at,
                next_blocked_reason,
                now,
                task_id
            ],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        let updated_task =
            get_task_with_conn(&tx, task_id)?.context("updated task could not be reloaded")?;
        tx.commit().context("failed to commit task update")?;
        Ok(Some(updated_task))
    }

    pub fn link_task_board_card(
        &self,
        task_id: &str,
        board_card_id: &str,
        force_reassign: bool,
    ) -> Result<Option<TaskRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match get_task_with_conn(&tx, task_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        if Self::get_board_card_tx(&tx, board_card_id)?.is_none() {
            anyhow::bail!("board card not found");
        }
        let linked_task_id =
            find_task_id_by_link_target(&tx, "linked_board_card_id", board_card_id)?;
        let now = now_ms();
        if let Some(existing_task_id) = linked_task_id {
            if existing_task_id != task_id {
                if !force_reassign {
                    anyhow::bail!("board card already linked to another task");
                }
                tx.execute(
                    "UPDATE tasks SET linked_board_card_id = NULL, updated_at = ?1 WHERE task_id = ?2",
                    params![now, existing_task_id],
                )?;
            }
        }
        if current.linked_board_card_id.as_deref() != Some(board_card_id) {
            tx.execute(
                "UPDATE tasks SET linked_board_card_id = ?1, updated_at = ?2 WHERE task_id = ?3",
                params![board_card_id, now, task_id],
            )?;
        }
        tx.commit()?;
        self.get_task(task_id)
    }

    pub fn link_task_job(
        &self,
        task_id: &str,
        job_id: &str,
        force_reassign: bool,
    ) -> Result<Option<TaskRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let current = match get_task_with_conn(&tx, task_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        if get_job_with_conn(&tx, job_id)?.is_none() {
            anyhow::bail!("job not found");
        }
        let linked_task_id = find_task_id_by_link_target(&tx, "linked_job_id", job_id)?;
        let now = now_ms();
        if let Some(existing_task_id) = linked_task_id {
            if existing_task_id != task_id {
                if !force_reassign {
                    anyhow::bail!("job already linked to another task");
                }
                tx.execute(
                    "UPDATE tasks SET linked_job_id = NULL, updated_at = ?1 WHERE task_id = ?2",
                    params![now, existing_task_id],
                )?;
            }
        }
        if current.linked_job_id.as_deref() != Some(job_id) {
            tx.execute(
                "UPDATE tasks SET linked_job_id = ?1, updated_at = ?2 WHERE task_id = ?3",
                params![job_id, now, task_id],
            )?;
        }
        tx.commit()?;
        self.get_task(task_id)
    }

    pub fn clear_task_links(
        &self,
        task_id: &str,
        clear_board_card: bool,
        clear_job: bool,
    ) -> Result<Option<TaskRecord>> {
        let conn = self.connect()?;
        let current = match get_task_with_conn(&conn, task_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let next_clear_board_card = clear_board_card;
        let next_clear_job = clear_job;
        let next_board_card_id = if next_clear_board_card {
            None
        } else {
            current.linked_board_card_id.clone()
        };
        let next_job_id = if next_clear_job {
            None
        } else {
            current.linked_job_id.clone()
        };
        if next_board_card_id == current.linked_board_card_id
            && next_job_id == current.linked_job_id
        {
            return Ok(Some(current));
        }
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE tasks
            SET linked_board_card_id = ?1,
                linked_job_id = ?2,
                updated_at = ?3
            WHERE task_id = ?4
            "#,
            params![next_board_card_id, next_job_id, now, task_id],
        )?;
        self.get_task(task_id)
    }

    pub fn resolve_task_runtime_link(&self, task: &TaskRecord) -> Result<TaskRuntimeLinkRecord> {
        let conn = self.connect()?;
        let board_candidate = match task.linked_board_card_id.as_deref() {
            Some(card_id) => resolve_board_runtime_candidate(&conn, card_id)?,
            None => None,
        };
        let job_candidate = match task.linked_job_id.as_deref() {
            Some(job_id) => resolve_job_runtime_candidate(&conn, job_id)?,
            None => None,
        };
        Ok(select_runtime_link(board_candidate, job_candidate))
    }

    pub fn list_bootstrap_presets(
        &self,
        filter: BootstrapPresetListFilter,
    ) -> Result<PageResult<BootstrapPresetRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              preset_key, display_name, description, role_label, provider_path, default_model_provider,
              default_model_id, default_tool_profile, default_workspace_root, default_reports_to_agent_id,
              setup_notes, created_at, updated_at
            FROM bootstrap_presets
            "#,
        )?;
        let rows = stmt.query_map([], map_bootstrap_preset_row)?;
        let mut items = Vec::new();
        for row in rows {
            let record = row?;
            if bootstrap_preset_matches_filter(&record, &filter) {
                items.push(record);
            }
        }
        sort_records_by_updated(&mut items, filter.sort.as_deref(), |item| {
            (item.updated_at, item.preset_key.as_str())
        })?;
        page_records(items, filter.limit, filter.cursor.as_deref(), |item| {
            (item.updated_at, item.preset_key.as_str())
        })
    }

    pub fn get_bootstrap_preset(&self, preset_key: &str) -> Result<Option<BootstrapPresetRecord>> {
        let conn = self.connect()?;
        get_bootstrap_preset_with_conn(&conn, preset_key)
    }

    pub fn create_bootstrap_preset(
        &self,
        new_preset: NewBootstrapPreset,
    ) -> Result<BootstrapPresetRecord> {
        let conn = self.connect()?;
        let default_workspace_root =
            normalize_project_workspace_root(new_preset.default_workspace_root.as_deref())?;
        let default_reports_to_agent_id =
            normalize_optional_agent_reference(new_preset.default_reports_to_agent_id.as_deref());
        validate_bootstrap_preset_provider(
            &new_preset.provider_path,
            new_preset.default_model_provider.as_deref(),
        )?;
        let preset_key = normalize_preset_key(&new_preset.preset_key)?;
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO bootstrap_presets (
              preset_key, display_name, description, role_label, provider_path, default_model_provider,
              default_model_id, default_tool_profile, default_workspace_root, default_reports_to_agent_id,
              setup_notes, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                preset_key,
                new_preset.display_name.trim(),
                new_preset.description.trim(),
                new_preset.role_label.trim(),
                new_preset.provider_path.trim(),
                new_preset.default_model_provider,
                new_preset.default_model_id,
                new_preset.default_tool_profile,
                default_workspace_root,
                default_reports_to_agent_id,
                new_preset.setup_notes,
                now,
                now
            ],
        )
        .context("failed to create bootstrap preset")?;
        self.get_bootstrap_preset(&preset_key)?
            .context("created bootstrap preset could not be reloaded")
    }

    pub fn update_bootstrap_preset(
        &self,
        preset_key: &str,
        patch: BootstrapPresetUpdatePatch,
    ) -> Result<Option<BootstrapPresetRecord>> {
        if patch.display_name.is_none()
            && patch.description.is_none()
            && patch.role_label.is_none()
            && patch.provider_path.is_none()
            && patch.default_model_provider.is_none()
            && patch.default_model_id.is_none()
            && patch.default_tool_profile.is_none()
            && patch.default_workspace_root.is_none()
            && patch.default_reports_to_agent_id.is_none()
            && patch.setup_notes.is_none()
        {
            return self.get_bootstrap_preset(preset_key);
        }
        let conn = self.connect()?;
        let current = match get_bootstrap_preset_with_conn(&conn, preset_key)? {
            Some(record) => record,
            None => return Ok(None),
        };
        let next_display_name = patch.display_name.unwrap_or(current.display_name);
        let next_description = patch.description.unwrap_or(current.description);
        let next_role_label = patch.role_label.unwrap_or(current.role_label);
        let next_provider_path = patch.provider_path.unwrap_or(current.provider_path);
        let next_default_model_provider = patch
            .default_model_provider
            .unwrap_or(current.default_model_provider);
        let next_default_model_id = patch.default_model_id.unwrap_or(current.default_model_id);
        let next_default_tool_profile = patch
            .default_tool_profile
            .unwrap_or(current.default_tool_profile);
        let next_default_workspace_root = match patch.default_workspace_root {
            Some(value) => normalize_project_workspace_root(value.as_deref())?,
            None => current.default_workspace_root,
        };
        let next_default_reports_to_agent_id = match patch.default_reports_to_agent_id {
            Some(value) => normalize_optional_agent_reference(value.as_deref()),
            None => current.default_reports_to_agent_id,
        };
        let next_setup_notes = patch.setup_notes.unwrap_or(current.setup_notes);
        validate_bootstrap_preset_provider(
            &next_provider_path,
            next_default_model_provider.as_deref(),
        )?;
        let now = now_ms();
        let updated = conn.execute(
            r#"
            UPDATE bootstrap_presets
            SET display_name = ?1,
                description = ?2,
                role_label = ?3,
                provider_path = ?4,
                default_model_provider = ?5,
                default_model_id = ?6,
                default_tool_profile = ?7,
                default_workspace_root = ?8,
                default_reports_to_agent_id = ?9,
                setup_notes = ?10,
                updated_at = ?11
            WHERE preset_key = ?12
            "#,
            params![
                next_display_name.trim(),
                next_description.trim(),
                next_role_label.trim(),
                next_provider_path.trim(),
                next_default_model_provider,
                next_default_model_id,
                next_default_tool_profile,
                next_default_workspace_root,
                next_default_reports_to_agent_id,
                next_setup_notes,
                now,
                preset_key
            ],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        self.get_bootstrap_preset(preset_key)
    }

    pub fn list_connectors(
        &self,
        filter: ConnectorListFilter,
    ) -> Result<Vec<ConnectorSourceRecord>> {
        if let Some(status) = filter.status.as_deref() {
            validate_connector_status(status)?;
        }
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              cs.connector_id,
              cs.slug,
              cs.display_name,
              cs.source_kind,
              cs.origin_kind,
              cs.catalog_item_id,
              cs.current_version_id,
              cs.latest_imported_version_id,
              cs.status,
              cs.trust_state,
              (
                SELECT COUNT(1)
                FROM connector_assignments a
                WHERE a.connector_id = cs.connector_id
                  AND a.enabled = 1
              ) AS assigned_agent_count,
              (
                SELECT COUNT(1)
                FROM connector_published_tools pt
                WHERE pt.connector_id = cs.connector_id
                  AND pt.unpublished_at IS NULL
                  AND (
                    cs.current_version_id IS NULL
                    OR pt.version_id = cs.current_version_id
                  )
              ) AS published_tool_count,
              cs.last_conversion_at,
              cs.last_review_at,
              cs.last_enabled_at,
              cs.last_disabled_at,
              cs.created_at,
              cs.updated_at
            FROM connector_sources cs
            "#,
        )?;
        let rows = stmt.query_map([], map_connector_source_row)?;
        let mut items = Vec::new();
        for row in rows {
            let record = row?;
            if connector_matches_filter(&record, &filter) {
                items.push(record);
            }
        }
        sort_records_by_updated(&mut items, Some("updated_at_desc"), |item| {
            (item.updated_at, item.connector_id.as_str())
        })?;
        Ok(items)
    }

    pub fn get_connector(&self, connector_id: &str) -> Result<Option<ConnectorSourceRecord>> {
        let conn = self.connect()?;
        get_connector_with_conn(&conn, connector_id)
    }

    pub fn get_connector_by_slug(&self, slug: &str) -> Result<Option<ConnectorSourceRecord>> {
        let conn = self.connect()?;
        get_connector_by_slug_with_conn(&conn, slug)
    }

    pub fn list_connector_versions(
        &self,
        connector_id: &str,
    ) -> Result<Vec<ConnectorVersionRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              version_id, connector_id, version_label, source_digest, raw_source_location,
              import_metadata_json, schema_summary_json, latest_conversion_id,
              external_reference_policy, created_at, updated_at
            FROM connector_versions
            WHERE connector_id = ?1
            ORDER BY created_at DESC, version_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![connector_id], map_connector_version_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn get_connector_version(
        &self,
        version_id: &str,
    ) -> Result<Option<ConnectorVersionRecord>> {
        let conn = self.connect()?;
        get_connector_version_with_conn(&conn, version_id)
    }

    pub fn list_connector_conversions(
        &self,
        connector_id: &str,
        version_id: Option<&str>,
    ) -> Result<Vec<ConnectorConversionRecord>> {
        let conn = self.connect()?;
        let query = if version_id.is_some() {
            r#"
            SELECT
              conversion_id, connector_id, version_id, status, warnings_json, proposed_tools_json,
              write_capable_tools, unsupported_operations_json, normalization_notes_json,
              diff_from_previous_json, created_at, updated_at
            FROM connector_conversions
            WHERE connector_id = ?1 AND version_id = ?2
            ORDER BY created_at DESC, conversion_id ASC
            "#
        } else {
            r#"
            SELECT
              conversion_id, connector_id, version_id, status, warnings_json, proposed_tools_json,
              write_capable_tools, unsupported_operations_json, normalization_notes_json,
              diff_from_previous_json, created_at, updated_at
            FROM connector_conversions
            WHERE connector_id = ?1
            ORDER BY created_at DESC, conversion_id ASC
            "#
        };
        let mut stmt = conn.prepare(query)?;
        let rows = if let Some(version_id) = version_id {
            stmt.query_map(
                params![connector_id, version_id],
                map_connector_conversion_row,
            )?
        } else {
            stmt.query_map(params![connector_id], map_connector_conversion_row)?
        };
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn get_connector_conversion(
        &self,
        conversion_id: &str,
    ) -> Result<Option<ConnectorConversionRecord>> {
        let conn = self.connect()?;
        get_connector_conversion_with_conn(&conn, conversion_id)
    }

    pub fn list_connector_published_tools(
        &self,
        connector_id: &str,
        include_unpublished: bool,
    ) -> Result<Vec<ConnectorPublishedToolRecord>> {
        let conn = self.connect()?;
        list_connector_published_tools_with_conn(&conn, connector_id, include_unpublished)
    }

    pub fn get_connector_published_tool(
        &self,
        published_tool_id: &str,
    ) -> Result<Option<ConnectorPublishedToolRecord>> {
        let conn = self.connect()?;
        get_connector_published_tool_with_conn(&conn, published_tool_id)
    }

    pub fn get_connector_published_tool_by_name(
        &self,
        tool_name: &str,
        include_unpublished: bool,
    ) -> Result<Option<ConnectorPublishedToolRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              published_tool_id, connector_id, version_id, conversion_id, tool_name, display_name,
              tool_schema_json, origin_metadata_json, write_classification, published_at,
              unpublished_at, superseded_by_published_tool_id, deprecation_state
            FROM connector_published_tools
            WHERE tool_name = ?1
            ORDER BY published_at DESC, published_tool_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![tool_name], map_connector_published_tool_row)?;
        for row in rows {
            let record = row?;
            if include_unpublished || record.unpublished_at.is_none() {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }

    pub fn list_connector_assignments(
        &self,
        connector_id: &str,
    ) -> Result<Vec<ConnectorAssignmentRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              assignment_id, connector_id, agent_id, enabled, auth_mode, created_at, updated_at
            FROM connector_assignments
            WHERE connector_id = ?1
            ORDER BY updated_at DESC, assignment_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![connector_id], map_connector_assignment_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_connector_auth_bindings(
        &self,
        connector_id: &str,
    ) -> Result<Vec<ConnectorAuthBindingRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              auth_binding_id, connector_id, agent_id, auth_kind, secret_ref, oauth_session_id,
              status, auth_metadata_json, last_success_at, last_error, last_rotated_at, created_at, updated_at
            FROM connector_auth_bindings
            WHERE connector_id = ?1
            ORDER BY agent_id IS NOT NULL DESC, updated_at DESC, auth_binding_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![connector_id], map_connector_auth_binding_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    /// Record the health result of an actual connector invocation only while
    /// the exact binding generation used for that invocation is still
    /// current. A late response from a rotated/replaced credential therefore
    /// cannot certify or poison its replacement.
    pub fn record_connector_auth_binding_execution(
        &self,
        expected: &ConnectorAuthBindingRecord,
        success: bool,
    ) -> Result<bool> {
        let conn = self.connect()?;
        let observed_at = now_ms();
        let next_updated_at = observed_at.max(expected.updated_at.saturating_add(1));
        let changed = conn.execute(
            r#"
            UPDATE connector_auth_bindings
            SET last_success_at = CASE WHEN ?1 = 1 THEN ?2 ELSE last_success_at END,
                last_error = CASE WHEN ?1 = 1 THEN NULL ELSE 'connector_invocation_failed' END,
                updated_at = ?3
            WHERE auth_binding_id = ?4
              AND connector_id = ?5
              AND agent_id IS ?6
              AND auth_kind = ?7
              AND secret_ref IS ?8
              AND oauth_session_id IS ?9
              AND status = 'ready'
              AND last_rotated_at IS ?10
              AND updated_at = ?11
            "#,
            params![
                if success { 1 } else { 0 },
                observed_at,
                next_updated_at,
                expected.auth_binding_id,
                expected.connector_id,
                expected.agent_id,
                expected.auth_kind,
                expected.secret_ref,
                expected.oauth_session_id,
                expected.last_rotated_at,
                expected.updated_at,
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn list_connector_interactions(
        &self,
        connector_id: Option<&str>,
    ) -> Result<Vec<ConnectorInteractionRecord>> {
        let conn = self.connect()?;
        let query = if connector_id.is_some() {
            r#"
            SELECT
              interaction_id, connector_id, agent_id, interaction_kind, status, prompt_summary,
              resume_token, expires_at, consumed_at, detail_json, created_at, updated_at
            FROM connector_interactions
            WHERE connector_id = ?1
            ORDER BY updated_at DESC, interaction_id ASC
            "#
        } else {
            r#"
            SELECT
              interaction_id, connector_id, agent_id, interaction_kind, status, prompt_summary,
              resume_token, expires_at, consumed_at, detail_json, created_at, updated_at
            FROM connector_interactions
            ORDER BY updated_at DESC, interaction_id ASC
            "#
        };
        let mut stmt = conn.prepare(query)?;
        let rows = if let Some(connector_id) = connector_id {
            stmt.query_map(params![connector_id], map_connector_interaction_row)?
        } else {
            stmt.query_map([], map_connector_interaction_row)?
        };
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn import_connector(
        &self,
        new_import: NewConnectorImport,
    ) -> Result<(ConnectorSourceRecord, ConnectorVersionRecord)> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let slug = normalize_management_slug(&new_import.slug)?;
        let source_kind = normalize_connector_source_kind(&new_import.source_kind)?;
        let origin_kind = normalize_connector_origin_kind(&new_import.origin_kind)?;
        let trust_state = new_import.trust_state.trim().to_ascii_lowercase();
        validate_connector_trust_state(&trust_state)?;
        let external_reference_policy =
            normalize_connector_external_reference_policy(&new_import.external_reference_policy)?;
        let display_name = new_import.display_name.trim();
        if display_name.is_empty() {
            anyhow::bail!("display_name cannot be empty");
        }
        let import_metadata_json = normalize_connector_json_payload(
            &new_import.import_metadata_json,
            "import_metadata_json",
        )?;
        let schema_summary_json = normalize_connector_json_payload(
            &new_import.schema_summary_json,
            "schema_summary_json",
        )?;
        let version_label = new_import.version_label.trim();
        if version_label.is_empty() {
            anyhow::bail!("version_label cannot be empty");
        }
        let now = now_ms();
        let existing = get_connector_by_slug_with_conn(&tx, &slug)?;
        let connector_id = existing
            .as_ref()
            .map(|record| record.connector_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let version_id = uuid::Uuid::new_v4().to_string();

        if let Some(current) = existing.as_ref() {
            if current.source_kind != source_kind {
                anyhow::bail!("existing connector source_kind does not match import");
            }
            tx.execute(
                r#"
                UPDATE connector_sources
                SET display_name = ?1,
                    origin_kind = ?2,
                    catalog_item_id = ?3,
                    latest_imported_version_id = ?4,
                    trust_state = ?5,
                    updated_at = ?6
                WHERE connector_id = ?7
                "#,
                params![
                    display_name,
                    origin_kind,
                    new_import.catalog_item_id,
                    version_id,
                    trust_state,
                    now,
                    connector_id
                ],
            )?;
        } else {
            tx.execute(
                r#"
                INSERT INTO connector_sources (
                  connector_id, slug, display_name, source_kind, origin_kind, catalog_item_id,
                  current_version_id, latest_imported_version_id, status, trust_state,
                  last_conversion_at, last_review_at, last_enabled_at, last_disabled_at,
                  created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, NULL, NULL, NULL, NULL, ?10, ?11)
                "#,
                params![
                    connector_id,
                    slug,
                    display_name,
                    source_kind,
                    origin_kind,
                    new_import.catalog_item_id,
                    version_id,
                    CONNECTOR_STATUS_DRAFT,
                    trust_state,
                    now,
                    now
                ],
            )?;
        }

        tx.execute(
            r#"
            INSERT INTO connector_versions (
              version_id, connector_id, version_label, source_digest, raw_source_location,
              import_metadata_json, schema_summary_json, latest_conversion_id,
              external_reference_policy, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10)
            "#,
            params![
                version_id,
                connector_id,
                version_label,
                new_import.source_digest.trim(),
                new_import.raw_source_location,
                import_metadata_json,
                schema_summary_json,
                external_reference_policy,
                now,
                now
            ],
        )?;

        let connector = get_connector_with_conn(&tx, &connector_id)?
            .context("imported connector could not be reloaded")?;
        let version = get_connector_version_with_conn(&tx, &version_id)?
            .context("imported connector version could not be reloaded")?;
        tx.commit()?;
        Ok((connector, version))
    }

    pub fn record_connector_conversion(
        &self,
        connector_id: &str,
        version_id: &str,
        new_conversion: NewConnectorConversion,
    ) -> Result<ConnectorConversionRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let connector = get_connector_with_conn(&tx, connector_id)?
            .with_context(|| format!("connector not found: {connector_id}"))?;
        let version = get_connector_version_with_conn(&tx, version_id)?
            .with_context(|| format!("connector version not found: {version_id}"))?;
        if version.connector_id != connector.connector_id {
            anyhow::bail!("version does not belong to connector");
        }
        validate_connector_conversion_status(&new_conversion.status)?;
        let warnings_json =
            normalize_connector_json_payload(&new_conversion.warnings_json, "warnings_json")?;
        let proposed_tools_json = normalize_connector_json_payload(
            &new_conversion.proposed_tools_json,
            "proposed_tools_json",
        )?;
        let unsupported_operations_json = normalize_connector_json_payload(
            &new_conversion.unsupported_operations_json,
            "unsupported_operations_json",
        )?;
        let normalization_notes_json = normalize_connector_json_payload(
            &new_conversion.normalization_notes_json,
            "normalization_notes_json",
        )?;
        let diff_from_previous_json = normalize_connector_json_payload(
            &new_conversion.diff_from_previous_json,
            "diff_from_previous_json",
        )?;
        let conversion_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        tx.execute(
            r#"
            INSERT INTO connector_conversions (
              conversion_id, connector_id, version_id, status, warnings_json, proposed_tools_json,
              write_capable_tools, unsupported_operations_json, normalization_notes_json,
              diff_from_previous_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                conversion_id,
                connector_id,
                version_id,
                new_conversion.status.trim(),
                warnings_json,
                proposed_tools_json,
                new_conversion.write_capable_tools as i64,
                unsupported_operations_json,
                normalization_notes_json,
                diff_from_previous_json,
                now,
                now
            ],
        )?;
        tx.execute(
            r#"
            UPDATE connector_versions
            SET latest_conversion_id = ?1,
                updated_at = ?2
            WHERE version_id = ?3
            "#,
            params![conversion_id, now, version_id],
        )?;
        tx.execute(
            r#"
            UPDATE connector_sources
            SET status = ?1,
                last_conversion_at = ?2,
                updated_at = ?3
            WHERE connector_id = ?4
            "#,
            params![
                if new_conversion.status.trim() == CONNECTOR_CONVERSION_SUCCEEDED {
                    CONNECTOR_STATUS_CONVERTED
                } else {
                    CONNECTOR_STATUS_ERROR
                },
                now,
                now,
                connector_id
            ],
        )?;
        let conversion = get_connector_conversion_with_conn(&tx, &conversion_id)?
            .context("connector conversion could not be reloaded")?;
        tx.commit()?;
        Ok(conversion)
    }

    pub fn publish_connector_tools(
        &self,
        connector_id: &str,
        conversion_id: &str,
        selected_candidate_ids: &[String],
        published_tools: &[NewConnectorPublishedTool],
        enable_after_publish: bool,
    ) -> Result<(
        ConnectorSourceRecord,
        ConnectorVersionRecord,
        Vec<ConnectorPublishedToolRecord>,
    )> {
        if selected_candidate_ids.is_empty() || published_tools.is_empty() {
            anyhow::bail!("at least one connector tool must be selected for publish");
        }
        if selected_candidate_ids.len() != published_tools.len() {
            anyhow::bail!("selected candidate count must match published tool count");
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let connector = get_connector_with_conn(&tx, connector_id)?
            .with_context(|| format!("connector not found: {connector_id}"))?;
        let conversion = get_connector_conversion_with_conn(&tx, conversion_id)?
            .with_context(|| format!("connector conversion not found: {conversion_id}"))?;
        if conversion.connector_id != connector.connector_id {
            anyhow::bail!("conversion does not belong to connector");
        }
        if conversion.status != CONNECTOR_CONVERSION_SUCCEEDED {
            anyhow::bail!("conversion must succeed before publish");
        }
        let version = get_connector_version_with_conn(&tx, &conversion.version_id)?
            .context("connector version missing for publish")?;
        let mut selected = Vec::new();
        for tool in published_tools {
            validate_connector_write_classification(&tool.write_classification)?;
            selected.push((
                tool.tool_name.trim().to_string(),
                tool.display_name.trim().to_string(),
                normalize_connector_json_payload(&tool.tool_schema_json, "tool_schema_json")?,
                normalize_connector_json_payload(
                    &tool.origin_metadata_json,
                    "origin_metadata_json",
                )?,
                normalize_connector_deprecation_state(CONNECTOR_DEPRECATION_ACTIVE)?,
                tool.write_classification.trim().to_string(),
            ));
        }
        let now = now_ms();
        let mut published = Vec::new();
        for (
            tool_name,
            display_name,
            tool_schema_json,
            origin_metadata_json,
            deprecation_state,
            write_classification,
        ) in selected
        {
            let tool_name_for_supersede = tool_name.clone();
            let published_tool_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                r#"
                INSERT INTO connector_published_tools (
                  published_tool_id, connector_id, version_id, conversion_id, tool_name, display_name,
                  tool_schema_json, origin_metadata_json, write_classification, published_at,
                  unpublished_at, superseded_by_published_tool_id, deprecation_state
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, ?11)
                "#,
                params![
                    published_tool_id,
                    connector_id,
                    version.version_id,
                    conversion_id,
                    tool_name,
                    display_name,
                    tool_schema_json,
                    origin_metadata_json,
                    write_classification,
                    now,
                    deprecation_state
                ],
            )?;
            tx.execute(
                r#"
                UPDATE connector_published_tools
                SET superseded_by_published_tool_id = ?1,
                    deprecation_state = ?2
                WHERE connector_id = ?3
                  AND tool_name = ?4
                  AND published_tool_id != ?1
                  AND superseded_by_published_tool_id IS NULL
                  AND unpublished_at IS NULL
                "#,
                params![
                    published_tool_id,
                    CONNECTOR_DEPRECATION_SUPERSEDED,
                    connector_id,
                    tool_name_for_supersede
                ],
            )?;
            let record = get_connector_published_tool_with_conn(&tx, &published_tool_id)?
                .context("published connector tool could not be reloaded")?;
            published.push(record);
        }
        let keep_enabled = enable_after_publish
            || (connector.current_version_id.is_some()
                && connector.status == CONNECTOR_STATUS_ENABLED);
        let next_status = if keep_enabled {
            CONNECTOR_STATUS_ENABLED
        } else {
            CONNECTOR_STATUS_DISABLED
        };
        tx.execute(
            r#"
            UPDATE connector_sources
            SET current_version_id = CASE WHEN ?1 = 1 THEN ?2 ELSE current_version_id END,
                status = ?3,
                last_review_at = ?4,
                last_enabled_at = CASE WHEN ?1 = 1 THEN ?4 ELSE last_enabled_at END,
                updated_at = ?5
            WHERE connector_id = ?6
            "#,
            params![
                if enable_after_publish { 1 } else { 0 },
                version.version_id,
                next_status,
                now,
                now,
                connector_id
            ],
        )?;
        let source = get_connector_with_conn(&tx, connector_id)?
            .context("published connector source could not be reloaded")?;
        tx.commit()?;
        Ok((source, version, published))
    }

    pub fn unpublish_connector_tools(
        &self,
        connector_id: &str,
        published_tool_ids: &[String],
    ) -> Result<(ConnectorSourceRecord, Vec<ConnectorPublishedToolRecord>)> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let now = now_ms();
        for published_tool_id in published_tool_ids {
            let Some(record) = get_connector_published_tool_with_conn(&tx, published_tool_id)?
            else {
                continue;
            };
            if record.connector_id != connector_id {
                anyhow::bail!("published tool does not belong to connector");
            }
            tx.execute(
                r#"
                UPDATE connector_published_tools
                SET unpublished_at = COALESCE(unpublished_at, ?1),
                    deprecation_state = ?2
                WHERE published_tool_id = ?3
                "#,
                params![now, CONNECTOR_DEPRECATION_UNPUBLISHED, published_tool_id],
            )?;
        }
        tx.execute(
            "UPDATE connector_sources SET updated_at = ?1, last_review_at = ?1 WHERE connector_id = ?2",
            params![now, connector_id],
        )?;
        let source = get_connector_with_conn(&tx, connector_id)?
            .context("connector source missing after unpublish")?;
        let tools = list_connector_published_tools_with_conn(&tx, connector_id, true)?
            .into_iter()
            .filter(|item| {
                published_tool_ids
                    .iter()
                    .any(|id| id == &item.published_tool_id)
            })
            .collect::<Vec<_>>();
        tx.commit()?;
        Ok((source, tools))
    }

    pub fn rollback_connector_version(
        &self,
        connector_id: &str,
        version_id: &str,
    ) -> Result<
        Option<(
            ConnectorSourceRecord,
            ConnectorVersionRecord,
            Vec<ConnectorPublishedToolRecord>,
        )>,
    > {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let Some(version) = get_connector_version_with_conn(&tx, version_id)? else {
            return Ok(None);
        };
        if version.connector_id != connector_id {
            anyhow::bail!("version does not belong to connector");
        }
        let tools = list_connector_published_tools_with_conn(&tx, connector_id, false)?
            .into_iter()
            .filter(|item| item.version_id == version_id)
            .collect::<Vec<_>>();
        if tools.is_empty() {
            anyhow::bail!("rollback target version has no active published tools");
        }
        let now = now_ms();
        tx.execute(
            r#"
            UPDATE connector_sources
            SET current_version_id = ?1,
                status = ?2,
                last_enabled_at = ?3,
                updated_at = ?4
            WHERE connector_id = ?5
            "#,
            params![version_id, CONNECTOR_STATUS_ENABLED, now, now, connector_id],
        )?;
        let source = get_connector_with_conn(&tx, connector_id)?
            .context("connector source missing after rollback")?;
        tx.commit()?;
        Ok(Some((source, version, tools)))
    }

    pub fn set_connector_enabled(
        &self,
        connector_id: &str,
        enabled: bool,
    ) -> Result<Option<ConnectorSourceRecord>> {
        let conn = self.connect()?;
        let current = match get_connector_with_conn(&conn, connector_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        if enabled && current.current_version_id.is_none() {
            anyhow::bail!("connector cannot be enabled without a current_version_id");
        }
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE connector_sources
            SET status = ?1,
                last_enabled_at = CASE WHEN ?2 = 1 THEN ?3 ELSE last_enabled_at END,
                last_disabled_at = CASE WHEN ?2 = 1 THEN last_disabled_at ELSE ?3 END,
                updated_at = ?4
            WHERE connector_id = ?5
            "#,
            params![
                if enabled {
                    CONNECTOR_STATUS_ENABLED
                } else {
                    CONNECTOR_STATUS_DISABLED
                },
                if enabled { 1 } else { 0 },
                now,
                now,
                connector_id
            ],
        )?;
        self.get_connector(connector_id)
    }

    pub fn upsert_connector_assignment(
        &self,
        connector_id: &str,
        assignment: NewConnectorAssignment,
    ) -> Result<ConnectorAssignmentRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let _connector = get_connector_with_conn(&tx, connector_id)?
            .with_context(|| format!("connector not found: {connector_id}"))?;
        validate_optional_owner_agent(self, &tx, Some(assignment.agent_id.as_str()))?;
        validate_connector_assignment_auth_mode(&assignment.auth_mode)?;
        let now = now_ms();
        let existing = get_connector_assignment_with_conn(&tx, connector_id, &assignment.agent_id)?;
        let assignment_id = existing
            .as_ref()
            .map(|item| item.assignment_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if existing.is_some() {
            tx.execute(
                r#"
                UPDATE connector_assignments
                SET enabled = ?1,
                    auth_mode = ?2,
                    updated_at = ?3
                WHERE assignment_id = ?4
                "#,
                params![
                    if assignment.enabled { 1 } else { 0 },
                    assignment.auth_mode.trim(),
                    now,
                    assignment_id
                ],
            )?;
        } else {
            tx.execute(
                r#"
                INSERT INTO connector_assignments (
                  assignment_id, connector_id, agent_id, enabled, auth_mode, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    assignment_id,
                    connector_id,
                    assignment.agent_id.trim(),
                    if assignment.enabled { 1 } else { 0 },
                    assignment.auth_mode.trim(),
                    now,
                    now
                ],
            )?;
        }
        tx.execute(
            "UPDATE connector_sources SET updated_at = ?1 WHERE connector_id = ?2",
            params![now, connector_id],
        )?;
        let record = get_connector_assignment_with_conn(&tx, connector_id, &assignment.agent_id)?
            .context("connector assignment could not be reloaded")?;
        tx.commit()?;
        Ok(record)
    }

    pub fn upsert_connector_auth_binding(
        &self,
        connector_id: &str,
        binding: NewConnectorAuthBinding,
    ) -> Result<ConnectorAuthBindingRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let _connector = get_connector_with_conn(&tx, connector_id)?
            .with_context(|| format!("connector not found: {connector_id}"))?;
        if let Some(agent_id) = binding.agent_id.as_deref() {
            validate_optional_owner_agent(self, &tx, Some(agent_id))?;
        }
        let auth_kind = normalize_connector_auth_kind(&binding.auth_kind)?;
        let status = normalize_connector_auth_status(&binding.status)?;
        let auth_metadata_json =
            normalize_connector_json_payload(&binding.auth_metadata_json, "auth_metadata_json")?;
        let now = now_ms();
        let existing =
            get_connector_auth_binding_with_conn(&tx, connector_id, binding.agent_id.as_deref())?;
        let auth_binding_id = existing
            .as_ref()
            .map(|item| item.auth_binding_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if existing.is_some() {
            tx.execute(
                r#"
                UPDATE connector_auth_bindings
                SET auth_kind = ?1,
                    secret_ref = ?2,
                    oauth_session_id = ?3,
                    status = ?4,
                    auth_metadata_json = ?5,
                    last_success_at = ?6,
                    last_error = ?7,
                    last_rotated_at = ?8,
                    updated_at = ?9
                WHERE auth_binding_id = ?10
                "#,
                params![
                    auth_kind,
                    binding.secret_ref,
                    binding.oauth_session_id,
                    status,
                    auth_metadata_json,
                    binding.last_success_at,
                    binding.last_error,
                    binding.last_rotated_at,
                    now,
                    auth_binding_id
                ],
            )?;
        } else {
            tx.execute(
                r#"
                INSERT INTO connector_auth_bindings (
                  auth_binding_id, connector_id, agent_id, auth_kind, secret_ref, oauth_session_id,
                  status, auth_metadata_json, last_success_at, last_error, last_rotated_at,
                  created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                params![
                    auth_binding_id,
                    connector_id,
                    binding.agent_id,
                    auth_kind,
                    binding.secret_ref,
                    binding.oauth_session_id,
                    status,
                    auth_metadata_json,
                    binding.last_success_at,
                    binding.last_error,
                    binding.last_rotated_at,
                    now,
                    now
                ],
            )?;
        }
        tx.execute(
            "UPDATE connector_sources SET updated_at = ?1 WHERE connector_id = ?2",
            params![now, connector_id],
        )?;
        let record =
            get_connector_auth_binding_with_conn(&tx, connector_id, binding.agent_id.as_deref())?
                .context("connector auth binding could not be reloaded")?;
        tx.commit()?;
        Ok(record)
    }

    pub fn create_connector_interaction(
        &self,
        connector_id: &str,
        interaction: NewConnectorInteraction,
    ) -> Result<ConnectorInteractionRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let _connector = get_connector_with_conn(&tx, connector_id)?
            .with_context(|| format!("connector not found: {connector_id}"))?;
        if let Some(agent_id) = interaction.agent_id.as_deref() {
            validate_optional_owner_agent(self, &tx, Some(agent_id))?;
        }
        let interaction_kind = normalize_connector_interaction_kind(&interaction.interaction_kind)?;
        validate_connector_interaction_status(&interaction.status)?;
        let detail_json =
            normalize_connector_json_payload(&interaction.detail_json, "detail_json")?;
        let prompt_summary = normalize_connector_prompt_summary(&interaction.prompt_summary)?;
        let interaction_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        tx.execute(
            r#"
            INSERT INTO connector_interactions (
              interaction_id, connector_id, agent_id, interaction_kind, status, prompt_summary,
              resume_token, expires_at, consumed_at, detail_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11)
            "#,
            params![
                interaction_id,
                connector_id,
                interaction.agent_id,
                interaction_kind,
                interaction.status.trim(),
                prompt_summary,
                interaction.resume_token,
                interaction.expires_at,
                detail_json,
                now,
                now
            ],
        )?;
        tx.execute(
            "UPDATE connector_sources SET updated_at = ?1 WHERE connector_id = ?2",
            params![now, connector_id],
        )?;
        let record = get_connector_interaction_with_conn(&tx, &interaction_id)?
            .context("connector interaction could not be reloaded")?;
        tx.commit()?;
        Ok(record)
    }

    pub fn resume_connector_interaction(
        &self,
        interaction_id: &str,
        status: &str,
        detail_json: Option<&str>,
    ) -> Result<Option<ConnectorInteractionRecord>> {
        let conn = self.connect()?;
        let current = match get_connector_interaction_with_conn(&conn, interaction_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        validate_connector_interaction_status(status)?;
        let next_detail_json = match detail_json {
            Some(value) => normalize_connector_json_payload(value, "detail_json")?,
            None => current.detail_json.clone(),
        };
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE connector_interactions
            SET status = ?1,
                resume_token = NULL,
                consumed_at = CASE WHEN ?1 IN (?2, ?3) THEN ?4 ELSE consumed_at END,
                detail_json = ?5,
                updated_at = ?6
            WHERE interaction_id = ?7
            "#,
            params![
                status.trim(),
                CONNECTOR_INTERACTION_RESUMED,
                CONNECTOR_INTERACTION_CANCELLED,
                now,
                next_detail_json,
                now,
                interaction_id
            ],
        )?;
        self.get_connector_interaction(interaction_id)
    }

    pub fn get_connector_interaction(
        &self,
        interaction_id: &str,
    ) -> Result<Option<ConnectorInteractionRecord>> {
        let conn = self.connect()?;
        get_connector_interaction_with_conn(&conn, interaction_id)
    }

    pub fn create_agent_mail_thread(
        &self,
        new_thread: NewAgentMailThread,
    ) -> Result<AgentMailThreadRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let now = now_ms();
        let thread_id = uuid::Uuid::new_v4().to_string();
        let creator = new_thread.created_by_principal.trim().to_string();
        if creator.is_empty() {
            anyhow::bail!("created_by_principal is required");
        }
        tx.execute(
            r#"
            INSERT INTO agent_mail_threads (
              thread_id, kind, subject, created_by_principal, created_at, updated_at, archived_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
            "#,
            params![
                thread_id,
                new_thread.kind.trim(),
                new_thread.subject.trim(),
                creator.as_str(),
                now,
                now
            ],
        )
        .context("failed to create agent mail thread")?;

        let mut participants = std::collections::BTreeMap::<String, String>::new();
        participants.insert(creator.clone(), "owner".to_string());
        for (principal, role) in new_thread.participants {
            let principal = principal.trim().to_string();
            if principal.is_empty() {
                continue;
            }
            if principal == creator {
                participants.insert(principal, "owner".to_string());
                continue;
            }
            let normalized_role = role.trim();
            participants.insert(
                principal,
                if normalized_role.is_empty() {
                    "member".to_string()
                } else {
                    normalized_role.to_string()
                },
            );
        }

        for (principal_id, role) in participants {
            tx.execute(
                r#"
                INSERT OR IGNORE INTO agent_mail_thread_participants (
                  thread_id, principal_id, role, joined_at, last_read_at, muted
                ) VALUES (?1, ?2, ?3, ?4, NULL, 0)
                "#,
                params![thread_id, principal_id, role, now],
            )
            .context("failed to insert thread participant")?;
        }
        tx.commit()?;
        self.get_agent_mail_thread(&thread_id)?
            .context("created agent mail thread missing after commit")
    }

    pub fn get_agent_mail_thread(&self, thread_id: &str) -> Result<Option<AgentMailThreadRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              thread_id, kind, subject, created_by_principal, created_at, updated_at, archived_at
            FROM agent_mail_threads
            WHERE thread_id = ?1
              AND archived_at IS NULL
            "#,
        )?;
        Ok(stmt
            .query_row(params![thread_id], map_agent_mail_thread_row)
            .optional()?)
    }

    pub fn list_agent_mail_thread_participants(
        &self,
        thread_id: &str,
    ) -> Result<Vec<AgentMailThreadParticipantRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              thread_id, principal_id, role, joined_at, last_read_at, muted
            FROM agent_mail_thread_participants
            WHERE thread_id = ?1
            ORDER BY joined_at ASC, principal_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![thread_id], map_agent_mail_participant_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_agent_mail_threads(
        &self,
        filter: &AgentMailThreadListFilter,
    ) -> Result<Vec<AgentMailThreadSummaryRecord>> {
        let conn = self.connect()?;
        let principal = filter
            .principal_id
            .as_ref()
            .map(|value| value.trim().to_string());
        let kind = filter.kind.as_ref().map(|value| value.trim().to_string());
        let limit = filter.limit.max(1) as usize;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              t.thread_id,
              t.kind,
              t.subject,
              t.created_by_principal,
              t.created_at,
              t.updated_at,
              t.archived_at,
              (SELECT COUNT(*) FROM agent_mail_thread_participants p WHERE p.thread_id = t.thread_id) AS participant_count,
              (SELECT COUNT(*) FROM agent_mail_messages m WHERE m.thread_id = t.thread_id) AS message_count,
              (SELECT MAX(m.created_at) FROM agent_mail_messages m WHERE m.thread_id = t.thread_id) AS latest_message_at,
              (
                SELECT substr(m.body_text, 1, 220)
                FROM agent_mail_messages m
                WHERE m.thread_id = t.thread_id
                ORDER BY m.created_at DESC
                LIMIT 1
              ) AS latest_message_preview,
              (
                SELECT m.sender_principal
                FROM agent_mail_messages m
                WHERE m.thread_id = t.thread_id
                ORDER BY m.created_at DESC
                LIMIT 1
              ) AS latest_sender_principal,
              (
                SELECT COUNT(*)
                FROM agent_mail_messages m
                JOIN agent_mail_message_recipients r
                  ON r.message_id = m.message_id
                WHERE m.thread_id = t.thread_id
                  AND (?1 IS NOT NULL AND r.recipient_principal = ?1)
                  AND r.acked_at IS NULL
              ) AS unread_count,
              (
                SELECT COUNT(*)
                FROM agent_mail_messages m
                WHERE m.thread_id = t.thread_id
                  AND (?1 IS NOT NULL AND m.sender_principal = ?1)
              ) AS outbox_count,
              (
                SELECT COUNT(*)
                FROM agent_mail_messages m
                JOIN agent_mail_message_recipients r
                  ON r.message_id = m.message_id
                WHERE m.thread_id = t.thread_id
                  AND (?1 IS NOT NULL AND r.recipient_principal = ?1)
              ) AS inbox_count
            FROM agent_mail_threads t
            WHERE t.archived_at IS NULL
              AND (?2 IS NULL OR t.kind = ?2)
              AND (
                ?1 IS NULL OR EXISTS (
                  SELECT 1
                  FROM agent_mail_thread_participants p
                  WHERE p.thread_id = t.thread_id
                    AND p.principal_id = ?1
                )
              )
            ORDER BY t.updated_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![principal.as_deref(), kind.as_deref()], |row| {
            let summary = AgentMailThreadSummaryRecord {
                thread: map_agent_mail_thread_row(row)?,
                participant_count: row.get(7)?,
                message_count: row.get(8)?,
                latest_message_at: row.get(9)?,
                latest_message_preview: row.get(10)?,
                latest_sender_principal: row.get(11)?,
                unread_count: row.get(12)?,
            };
            let outbox_count: i64 = row.get(13)?;
            let inbox_count: i64 = row.get(14)?;
            Ok((summary, outbox_count, inbox_count))
        })?;
        let mut out = Vec::new();
        let mailbox = filter
            .mailbox
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase());
        let search_text = filter
            .search_text
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let search_thread_ids = if let Some(query) = search_text.as_deref() {
            self.search_agent_mail_thread_ids(query, 2_000)?
        } else {
            std::collections::HashSet::new()
        };
        for row in rows {
            let (summary, outbox_count, inbox_count) = row?;
            let mailbox_match = match mailbox.as_deref() {
                Some("inbox") => inbox_count > 0,
                Some("outbox") => outbox_count > 0,
                _ => true,
            };
            if !mailbox_match {
                continue;
            }
            if let Some(query) = search_text.as_deref() {
                let lowered = query.to_ascii_lowercase();
                let subject_match = summary
                    .thread
                    .subject
                    .to_ascii_lowercase()
                    .contains(lowered.as_str());
                let preview_match = summary
                    .latest_message_preview
                    .as_ref()
                    .map(|value| value.to_ascii_lowercase().contains(lowered.as_str()))
                    .unwrap_or(false);
                if !subject_match
                    && !preview_match
                    && !search_thread_ids.contains(summary.thread.thread_id.as_str())
                {
                    continue;
                }
            }
            out.push(summary);
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    pub fn create_agent_mail_message(
        &self,
        new_message: NewAgentMailMessage,
    ) -> Result<Option<AgentMailMessageRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let thread_id = new_message.thread_id.trim().to_string();
        let sender_principal = new_message.sender_principal.trim().to_string();
        if thread_id.is_empty() || sender_principal.is_empty() {
            anyhow::bail!("thread_id and sender_principal are required");
        }
        let sender_kind = new_message.sender_kind.trim().to_string();
        let thread = tx
            .query_row(
                r#"
                SELECT thread_id
                FROM agent_mail_threads
                WHERE thread_id = ?1
                  AND archived_at IS NULL
                "#,
                params![thread_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if thread.is_none() {
            return Ok(None);
        }

        let now = now_ms();
        tx.execute(
            r#"
            INSERT OR IGNORE INTO agent_mail_thread_participants (
              thread_id, principal_id, role, joined_at, last_read_at, muted
            ) VALUES (?1, ?2, 'member', ?3, ?3, 0)
            "#,
            params![thread_id.as_str(), sender_principal.as_str(), now],
        )?;

        let message_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            r#"
            INSERT INTO agent_mail_messages (
              message_id, thread_id, sender_principal, sender_kind, body_text, metadata_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                message_id,
                thread_id.as_str(),
                sender_principal.as_str(),
                sender_kind.as_str(),
                new_message.body_text,
                new_message.metadata_json,
                now
            ],
        )
        .context("failed to create agent mail message")?;

        let mut recipients = new_message
            .recipients
            .into_iter()
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        if recipients.is_empty() {
            let mut stmt = tx.prepare(
                r#"
                SELECT principal_id
                FROM agent_mail_thread_participants
                WHERE thread_id = ?1
                  AND principal_id != ?2
                "#,
            )?;
            let rows = stmt.query_map(
                params![thread_id.as_str(), sender_principal.as_str()],
                |row| row.get::<_, String>(0),
            )?;
            for row in rows {
                recipients.insert(row?);
            }
        }

        for recipient in recipients {
            tx.execute(
                r#"
                INSERT OR IGNORE INTO agent_mail_thread_participants (
                  thread_id, principal_id, role, joined_at, last_read_at, muted
                ) VALUES (?1, ?2, 'member', ?3, NULL, 0)
                "#,
                params![thread_id.as_str(), recipient.as_str(), now],
            )?;
            tx.execute(
                r#"
                INSERT OR REPLACE INTO agent_mail_message_recipients (
                  message_id, recipient_principal, delivered_at, acked_at
                ) VALUES (?1, ?2, ?3, NULL)
                "#,
                params![message_id.as_str(), recipient.as_str(), now],
            )?;
        }

        tx.execute(
            "UPDATE agent_mail_threads SET updated_at = ?1 WHERE thread_id = ?2",
            params![now, thread_id.as_str()],
        )?;
        tx.commit()?;
        self.get_agent_mail_message(&message_id)
    }

    pub fn list_office_floor_presence(&self) -> Result<Vec<OfficeFloorPresenceRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT a.agent_id, a.name,
              CASE
                WHEN EXISTS (
                  SELECT 1 FROM sessions s JOIN runs r ON r.session_id=s.session_id
                  WHERE s.agent_id=a.agent_id AND r.ended_at IS NULL
                    AND r.status IN ('queued','running','in_progress')
                ) THEN 'busy'
                WHEN EXISTS (SELECT 1 FROM sessions s WHERE s.agent_id=a.agent_id) THEN 'idle'
                ELSE 'unknown'
              END,
              (SELECT MAX(COALESCE(r.started_at, r.created_at))
                 FROM sessions s JOIN runs r ON r.session_id=s.session_id
                WHERE s.agent_id=a.agent_id),
              (SELECT r.run_id FROM sessions s JOIN runs r ON r.session_id=s.session_id
                WHERE s.agent_id=a.agent_id
                ORDER BY COALESCE(r.started_at, r.created_at) DESC, r.run_id DESC LIMIT 1)
            FROM agents a WHERE a.archived_at IS NULL ORDER BY a.name ASC, a.agent_id ASC
            "#,
        )?;
        let records = stmt
            .query_map([], |row| {
                Ok(OfficeFloorPresenceRecord {
                    agent_id: row.get(0)?,
                    display_name: row.get(1)?,
                    state: row.get(2)?,
                    observed_at: row.get(3)?,
                    target_run_id: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(records)
    }

    pub fn produce_office_chatter(&self, limit: u32) -> Result<Vec<OfficeChatterMessageRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let cursor: i64 = tx.query_row(
            "SELECT last_global_sequence FROM office_chatter_producer_cursor WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let mut event_stmt = tx.prepare(
            "SELECT global_sequence,event_id,event_name,aggregate_id,aggregate_revision,occurred_at FROM execass_outbox_events WHERE global_sequence>?1 ORDER BY global_sequence ASC LIMIT ?2",
        )?;
        let events = event_stmt
            .query_map(params![cursor, i64::from(limit.max(1))], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(event_stmt);
        let mut out = Vec::new();
        let mut last = cursor;
        for (sequence, event_id, event_name, delegation_id, revision, occurred_at) in events {
            last = sequence;
            let template = match event_name.as_str() {
                "execass.v1.delegation.transitioned" => Some("Workstream status changed."),
                "execass.v1.continuation.claimed_or_result_recorded" => {
                    Some("Work item progressed.")
                }
                "execass.v1.recovery.updated" => Some("Workstream recovery updated."),
                "execass.v1.completion.assessed" => Some("Workstream completion assessed."),
                _ => None,
            };
            let Some(body_text) = template else { continue };
            let exists = tx
                .query_row(
                    "SELECT 1 FROM execass_delegations WHERE delegation_id=?1",
                    params![delegation_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                continue;
            }
            let existing = tx
                .query_row(
                    "SELECT 1 FROM office_chatter_messages WHERE source_event_id=?1",
                    params![event_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if existing {
                continue;
            }
            let thread_id = tx
                .query_row(
                    "SELECT thread_id FROM office_chatter_workstreams WHERE delegation_id=?1",
                    params![delegation_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let thread_id = match thread_id {
                Some(thread_id) => thread_id,
                None => {
                    let thread_id = uuid::Uuid::new_v4().to_string();
                    tx.execute("INSERT INTO agent_mail_threads(thread_id,kind,subject,created_by_principal,created_at,updated_at,archived_at) VALUES(?1,'office_chatter','ExecAss activity','execass',?2,?2,NULL)", params![thread_id, occurred_at])?;
                    tx.execute("INSERT INTO agent_mail_thread_participants(thread_id,principal_id,role,joined_at,last_read_at,muted) VALUES(?1,'execass','owner',?2,NULL,0)", params![thread_id, occurred_at])?;
                    tx.execute("INSERT INTO office_chatter_workstreams(delegation_id,thread_id,safe_label,created_at) VALUES(?1,?2,'ExecAss workstream',?3)", params![delegation_id, thread_id, occurred_at])?;
                    thread_id
                }
            };
            let message_id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO agent_mail_messages(message_id,thread_id,sender_principal,sender_kind,body_text,metadata_json,created_at) VALUES(?1,?2,'execass','system',?3,NULL,?4)", params![message_id, thread_id, body_text, occurred_at])?;
            tx.execute(
                "UPDATE agent_mail_threads SET updated_at=?1 WHERE thread_id=?2",
                params![occurred_at, thread_id],
            )?;
            tx.execute("INSERT INTO office_chatter_messages(message_id,thread_id,source_event_id,source_kind,delegation_id,event_name,revision,created_at) VALUES(?1,?2,?3,'execass_event',?4,?5,?6,?7)", params![message_id, thread_id, event_id, delegation_id, event_name, revision, occurred_at])?;
            out.push(OfficeChatterMessageRecord {
                message_id,
                thread_id,
                source_kind: "execass_event".to_string(),
                event_name: Some(event_name),
                delegation_id,
                revision: Some(revision),
                body_text: body_text.to_string(),
                created_at: occurred_at,
            });
        }
        if last != cursor {
            tx.execute("UPDATE office_chatter_producer_cursor SET last_global_sequence=?1,updated_at=?2 WHERE singleton=1", params![last, now_ms()])?;
        }
        tx.commit()?;
        Ok(out)
    }

    pub fn create_office_chatter_owner_message(
        &self,
        thread_id: &str,
        principal_id: &str,
        body_text: &str,
    ) -> Result<Option<OfficeChatterMessageRecord>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let workstream = tx
            .query_row(
                "SELECT delegation_id FROM office_chatter_workstreams WHERE thread_id=?1",
                params![thread_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(delegation_id) = workstream else {
            return Ok(None);
        };
        let message_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        tx.execute("INSERT OR IGNORE INTO agent_mail_thread_participants(thread_id,principal_id,role,joined_at,last_read_at,muted) VALUES(?1,?2,'member',?3,?3,0)", params![thread_id, principal_id, now])?;
        tx.execute("INSERT INTO agent_mail_messages(message_id,thread_id,sender_principal,sender_kind,body_text,metadata_json,created_at) VALUES(?1,?2,?3,'owner',?4,NULL,?5)", params![message_id, thread_id, principal_id, body_text, now])?;
        tx.execute(
            "UPDATE agent_mail_threads SET updated_at=?1 WHERE thread_id=?2",
            params![now, thread_id],
        )?;
        tx.execute("INSERT INTO office_chatter_messages(message_id,thread_id,source_event_id,source_kind,delegation_id,event_name,revision,created_at) VALUES(?1,?2,NULL,'owner_message',?3,NULL,NULL,?4)", params![message_id, thread_id, delegation_id, now])?;
        tx.commit()?;
        Ok(Some(OfficeChatterMessageRecord {
            message_id,
            thread_id: thread_id.to_string(),
            source_kind: "owner_message".to_string(),
            event_name: None,
            delegation_id,
            revision: None,
            body_text: body_text.to_string(),
            created_at: now,
        }))
    }

    pub fn list_office_chatter(
        &self,
        room_limit: u32,
        message_limit: u32,
    ) -> Result<(
        Vec<OfficeChatterRoomRecord>,
        Vec<OfficeChatterMessageRecord>,
    )> {
        let conn = self.connect()?;
        let mut room_stmt = conn.prepare("SELECT w.thread_id,w.delegation_id,w.safe_label,MAX(m.created_at) FROM office_chatter_workstreams w JOIN office_chatter_messages m ON m.thread_id=w.thread_id GROUP BY w.thread_id,w.delegation_id,w.safe_label ORDER BY MAX(m.created_at) DESC LIMIT ?1")?;
        let rooms = room_stmt
            .query_map(params![i64::from(room_limit.max(1))], |row| {
                Ok(OfficeChatterRoomRecord {
                    thread_id: row.get(0)?,
                    delegation_id: row.get(1)?,
                    safe_label: row.get(2)?,
                    last_activity_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut message_stmt = conn.prepare("SELECT c.message_id,c.thread_id,c.source_kind,c.event_name,c.delegation_id,c.revision,m.body_text,c.created_at FROM office_chatter_messages c JOIN agent_mail_messages m ON m.message_id=c.message_id ORDER BY c.created_at DESC,c.message_id DESC LIMIT ?1")?;
        let mut messages = message_stmt
            .query_map(params![i64::from(message_limit.max(1))], |row| {
                Ok(OfficeChatterMessageRecord {
                    message_id: row.get(0)?,
                    thread_id: row.get(1)?,
                    source_kind: row.get(2)?,
                    event_name: row.get(3)?,
                    delegation_id: row.get(4)?,
                    revision: row.get(5)?,
                    body_text: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok((rooms, messages))
    }

    pub fn get_agent_mail_message(
        &self,
        message_id: &str,
    ) -> Result<Option<AgentMailMessageRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              message_id, thread_id, sender_principal, sender_kind, body_text, metadata_json, created_at
            FROM agent_mail_messages
            WHERE message_id = ?1
            "#,
        )?;
        Ok(stmt
            .query_row(params![message_id], map_agent_mail_message_row)
            .optional()?)
    }

    pub fn list_agent_mail_messages(
        &self,
        thread_id: &str,
        limit: u32,
    ) -> Result<Vec<AgentMailMessageRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              message_id, thread_id, sender_principal, sender_kind, body_text, metadata_json, created_at
            FROM agent_mail_messages
            WHERE thread_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(
            params![thread_id, i64::from(limit.max(1))],
            map_agent_mail_message_row,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out.reverse();
        Ok(out)
    }

    pub fn list_agent_mail_message_recipients(
        &self,
        message_id: &str,
    ) -> Result<Vec<AgentMailMessageRecipientRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              message_id, recipient_principal, delivered_at, acked_at
            FROM agent_mail_message_recipients
            WHERE message_id = ?1
            ORDER BY recipient_principal ASC
            "#,
        )?;
        let rows = stmt.query_map(params![message_id], map_agent_mail_message_recipient_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn acknowledge_agent_mail_message(
        &self,
        message_id: &str,
        recipient_principal: &str,
    ) -> Result<Option<AgentMailMessageRecipientRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE agent_mail_message_recipients
            SET acked_at = COALESCE(acked_at, ?1)
            WHERE message_id = ?2
              AND recipient_principal = ?3
            "#,
            params![now, message_id, recipient_principal],
        )?;
        conn.execute(
            r#"
            UPDATE agent_mail_thread_participants
            SET last_read_at = ?1
            WHERE thread_id = (
                SELECT thread_id
                FROM agent_mail_messages
                WHERE message_id = ?2
            )
              AND principal_id = ?3
            "#,
            params![now, message_id, recipient_principal],
        )?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              message_id, recipient_principal, delivered_at, acked_at
            FROM agent_mail_message_recipients
            WHERE message_id = ?1
              AND recipient_principal = ?2
            "#,
        )?;
        Ok(stmt
            .query_row(
                params![message_id, recipient_principal],
                map_agent_mail_message_recipient_row,
            )
            .optional()?)
    }

    pub fn create_agent_mail_attachment(
        &self,
        new_attachment: NewAgentMailAttachment,
    ) -> Result<Option<AgentMailAttachmentRecord>> {
        let conn = self.connect()?;
        let attachment_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let inserted = conn.execute(
            r#"
            INSERT INTO agent_mail_attachments (
              attachment_id, message_id, filename, mime, sha256, bytes, local_path, created_at
            )
            SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
            FROM agent_mail_messages
            WHERE message_id = ?9
            LIMIT 1
            "#,
            params![
                attachment_id,
                new_attachment.message_id,
                new_attachment.filename,
                new_attachment.mime,
                new_attachment.sha256,
                new_attachment.bytes,
                new_attachment.local_path,
                now,
                new_attachment.message_id
            ],
        )?;
        if inserted == 0 {
            return Ok(None);
        }
        self.get_agent_mail_attachment(&attachment_id)
    }

    pub fn get_agent_mail_attachment(
        &self,
        attachment_id: &str,
    ) -> Result<Option<AgentMailAttachmentRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              attachment_id, message_id, filename, mime, sha256, bytes, local_path, created_at
            FROM agent_mail_attachments
            WHERE attachment_id = ?1
            "#,
        )?;
        Ok(stmt
            .query_row(params![attachment_id], map_agent_mail_attachment_row)
            .optional()?)
    }

    pub fn list_agent_mail_attachments(
        &self,
        message_id: &str,
    ) -> Result<Vec<AgentMailAttachmentRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              attachment_id, message_id, filename, mime, sha256, bytes, local_path, created_at
            FROM agent_mail_attachments
            WHERE message_id = ?1
            ORDER BY created_at ASC
            "#,
        )?;
        let rows = stmt.query_map(params![message_id], map_agent_mail_attachment_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn create_agent_mail_file_lease(
        &self,
        new_lease: NewAgentMailFileLease,
    ) -> Result<AgentMailFileLeaseRecord> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let now = now_ms();
        tx.execute(
            r#"
            UPDATE agent_mail_file_leases
            SET released_at = COALESCE(released_at, ?1)
            WHERE released_at IS NULL
              AND expires_at <= ?1
            "#,
            params![now],
        )?;
        let conflict_count: i64 = tx.query_row(
            r#"
            SELECT COUNT(*)
            FROM agent_mail_file_leases
            WHERE released_at IS NULL
              AND expires_at > ?1
              AND glob_pattern = ?2
              AND (exclusive = 1 OR ?3 = 1)
            "#,
            params![now, new_lease.glob_pattern.trim(), new_lease.exclusive],
            |row| row.get(0),
        )?;
        if conflict_count > 0 {
            anyhow::bail!("active lease conflict");
        }
        let lease_id = uuid::Uuid::new_v4().to_string();
        let ttl_ms = new_lease.ttl_ms.clamp(60_000, 86_400_000);
        let expires_at = now.saturating_add(ttl_ms);
        tx.execute(
            r#"
            INSERT INTO agent_mail_file_leases (
              lease_id, holder_principal, glob_pattern, exclusive, ttl_ms, note, created_at, expires_at, released_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
            "#,
            params![
                lease_id,
                new_lease.holder_principal.trim(),
                new_lease.glob_pattern.trim(),
                new_lease.exclusive,
                ttl_ms,
                new_lease.note,
                now,
                expires_at
            ],
        )?;
        tx.commit()?;
        self.get_agent_mail_file_lease(&lease_id)?
            .context("created lease missing after insert")
    }

    pub fn get_agent_mail_file_lease(
        &self,
        lease_id: &str,
    ) -> Result<Option<AgentMailFileLeaseRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              lease_id, holder_principal, glob_pattern, exclusive, ttl_ms, note, created_at, expires_at, released_at
            FROM agent_mail_file_leases
            WHERE lease_id = ?1
            "#,
        )?;
        Ok(stmt
            .query_row(params![lease_id], map_agent_mail_file_lease_row)
            .optional()?)
    }

    pub fn list_agent_mail_file_leases(
        &self,
        holder_principal: Option<&str>,
        include_released: bool,
    ) -> Result<Vec<AgentMailFileLeaseRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE agent_mail_file_leases
            SET released_at = COALESCE(released_at, ?1)
            WHERE released_at IS NULL
              AND expires_at <= ?1
            "#,
            params![now],
        )?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              lease_id, holder_principal, glob_pattern, exclusive, ttl_ms, note, created_at, expires_at, released_at
            FROM agent_mail_file_leases
            WHERE (?1 IS NULL OR holder_principal = ?1)
              AND (?2 = 1 OR released_at IS NULL)
            ORDER BY created_at DESC
            "#,
        )?;
        let rows = stmt.query_map(
            params![holder_principal, if include_released { 1 } else { 0 }],
            map_agent_mail_file_lease_row,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn release_agent_mail_file_lease(
        &self,
        lease_id: &str,
        holder_principal: Option<&str>,
    ) -> Result<Option<AgentMailFileLeaseRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        let rows_affected = conn.execute(
            r#"
            UPDATE agent_mail_file_leases
            SET released_at = COALESCE(released_at, ?1)
            WHERE lease_id = ?2
              AND released_at IS NULL
              AND (?3 IS NULL OR holder_principal = ?3)
            "#,
            params![now, lease_id, holder_principal],
        )?;
        if rows_affected == 0 {
            return Ok(None);
        }
        self.get_agent_mail_file_lease(lease_id)
    }

    fn search_agent_mail_thread_ids(
        &self,
        query_text: &str,
        limit: u32,
    ) -> Result<std::collections::HashSet<String>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT thread_id
            FROM agent_mail_messages_fts
            WHERE agent_mail_messages_fts MATCH ?1
            LIMIT ?2
            "#,
        )?;
        let sanitized = query_text.trim().replace('"', "").replace('\'', " ");
        let rows = stmt.query_map(params![sanitized, i64::from(limit.max(1))], |row| {
            row.get::<_, String>(0)
        })?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            out.insert(row?);
        }
        Ok(out)
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

    pub fn get_session_by_key(&self, session_key: &str) -> Result<Option<SessionRecord>> {
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
            WHERE s.session_key = ?1
            "#,
        )?;

        let record = stmt
            .query_row(params![session_key], |row| {
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

    pub fn update_session_agent(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Option<SessionRecord>> {
        let conn = self.connect()?;
        self.ensure_agent_exists(&conn, agent_id)?;
        if !self.session_exists(&conn, session_id)? {
            return Ok(None);
        }
        let now = now_ms();
        conn.execute(
            "UPDATE sessions SET agent_id = ?1, updated_at = ?2 WHERE session_id = ?3",
            params![agent_id, now, session_id],
        )
        .context("failed to update session agent")?;
        self.get_session(session_id)
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

    pub fn latest_run_for_session(&self, session_id: &str) -> Result<Option<RunRecord>> {
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
            WHERE session_id = ?1
            ORDER BY created_at DESC, rowid DESC
            LIMIT 1
            "#,
        )?;
        let record = stmt
            .query_row(params![session_id], map_run_row)
            .optional()?;
        Ok(record)
    }

    pub fn previous_run_for_session(
        &self,
        session_id: &str,
        before_created_at: i64,
    ) -> Result<Option<RunRecord>> {
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
            WHERE session_id = ?1
              AND (
                created_at < ?2
                OR (
                  created_at = ?2
                  AND rowid < COALESCE(
                    (
                      SELECT MAX(rowid)
                      FROM runs
                      WHERE session_id = ?1
                        AND created_at = ?2
                    ),
                    rowid
                  )
                )
              )
            ORDER BY created_at DESC, rowid DESC
            LIMIT 1
            "#,
        )?;
        let record = stmt
            .query_row(params![session_id, before_created_at], map_run_row)
            .optional()?;
        Ok(record)
    }

    pub fn create_assistant_worker(
        &self,
        new_worker: NewAssistantWorker,
    ) -> Result<AssistantWorkerRecord> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO assistant_workers (
              boss_key,
              root_session_id,
              worker_key,
              worker_kind,
              status,
              agent_id,
              session_id,
              template_key,
              display_name,
              instructions,
              run_defaults_json,
              session_mode,
              last_run_id,
              last_run_status,
              last_stop_reason,
              pending_approval_id,
              created_at,
              updated_at,
              archived_at
            ) VALUES (
              ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, NULL, NULL, ?13, ?14, ?15, NULL
            )
            "#,
            params![
                new_worker.boss_key,
                new_worker.root_session_id,
                new_worker.worker_key,
                new_worker.worker_kind,
                new_worker.status,
                new_worker.agent_id,
                new_worker.session_id,
                new_worker.template_key,
                new_worker.display_name,
                new_worker.instructions,
                new_worker.run_defaults_json,
                new_worker.session_mode,
                new_worker.pending_approval_id,
                now,
                now
            ],
        )
        .context("failed to create assistant worker")?;

        self.get_assistant_worker(&new_worker.boss_key, &new_worker.worker_key)?
            .context("created assistant worker could not be reloaded")
    }

    pub fn get_assistant_worker(
        &self,
        boss_key: &str,
        worker_key: &str,
    ) -> Result<Option<AssistantWorkerRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              boss_key,
              root_session_id,
              worker_key,
              worker_kind,
              status,
              agent_id,
              session_id,
              template_key,
              display_name,
              instructions,
              run_defaults_json,
              session_mode,
              last_run_id,
              last_run_status,
              last_stop_reason,
              pending_approval_id,
              created_at,
              updated_at,
              archived_at
            FROM assistant_workers
            WHERE boss_key = ?1 AND worker_key = ?2
            "#,
        )?;
        let record = stmt
            .query_row(params![boss_key, worker_key], map_assistant_worker_row)
            .optional()?;
        Ok(record)
    }

    pub fn get_assistant_worker_by_pending_approval(
        &self,
        approval_id: &str,
    ) -> Result<Option<AssistantWorkerRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              boss_key,
              root_session_id,
              worker_key,
              worker_kind,
              status,
              agent_id,
              session_id,
              template_key,
              display_name,
              instructions,
              run_defaults_json,
              session_mode,
              last_run_id,
              last_run_status,
              last_stop_reason,
              pending_approval_id,
              created_at,
              updated_at,
              archived_at
            FROM assistant_workers
            WHERE pending_approval_id = ?1
              AND archived_at IS NULL
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )?;
        let record = stmt
            .query_row(params![approval_id], map_assistant_worker_row)
            .optional()?;
        Ok(record)
    }

    pub fn list_assistant_workers(
        &self,
        boss_key: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<AssistantWorkerRecord>> {
        let conn = self.connect()?;
        let mut out = Vec::new();
        let mut stmt = if include_archived {
            conn.prepare(
                r#"
                SELECT
                  boss_key,
                  root_session_id,
                  worker_key,
                  worker_kind,
                  status,
                  agent_id,
                  session_id,
                  template_key,
                  display_name,
                  instructions,
                  run_defaults_json,
                  session_mode,
                  last_run_id,
                  last_run_status,
                  last_stop_reason,
                  pending_approval_id,
                  created_at,
                  updated_at,
                  archived_at
                FROM assistant_workers
                WHERE boss_key = ?1
                ORDER BY updated_at DESC
                LIMIT ?2
                "#,
            )?
        } else {
            conn.prepare(
                r#"
                SELECT
                  boss_key,
                  root_session_id,
                  worker_key,
                  worker_kind,
                  status,
                  agent_id,
                  session_id,
                  template_key,
                  display_name,
                  instructions,
                  run_defaults_json,
                  session_mode,
                  last_run_id,
                  last_run_status,
                  last_stop_reason,
                  pending_approval_id,
                  created_at,
                  updated_at,
                  archived_at
                FROM assistant_workers
                WHERE boss_key = ?1
                  AND archived_at IS NULL
                ORDER BY updated_at DESC
                LIMIT ?2
                "#,
            )?
        };
        let rows = stmt.query_map(
            params![boss_key, i64::from(limit.max(1))],
            map_assistant_worker_row,
        )?;
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_assistant_worker(
        &self,
        boss_key: &str,
        worker_key: &str,
        patch: AssistantWorkerPatch,
    ) -> Result<Option<AssistantWorkerRecord>> {
        let now = now_ms();
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .context("failed to start assistant worker update transaction")?;

        let status = patch.status;
        let template_key = patch.template_key;
        let display_name = patch.display_name;
        let run_defaults_json = patch.run_defaults_json;
        let session_mode = patch.session_mode;

        let apply_agent_id = patch.agent_id.is_some();
        let agent_id = patch.agent_id.flatten();
        let apply_session_id = patch.session_id.is_some();
        let session_id = patch.session_id.flatten();
        let apply_instructions = patch.instructions.is_some();
        let instructions = patch.instructions.flatten();
        let apply_last_run_id = patch.last_run_id.is_some();
        let last_run_id = patch.last_run_id.flatten();
        let apply_last_run_status = patch.last_run_status.is_some();
        let last_run_status = patch.last_run_status.flatten();
        let apply_last_stop_reason = patch.last_stop_reason.is_some();
        let last_stop_reason = patch.last_stop_reason.flatten();
        let apply_pending_approval_id = patch.pending_approval_id.is_some();
        let pending_approval_id = patch.pending_approval_id.flatten();
        let apply_archived_at = patch.archived_at.is_some();
        let archived_at = patch.archived_at.flatten();

        let rows_updated = tx
            .execute(
                r#"
                UPDATE assistant_workers
                SET
                  status = COALESCE(?1, status),
                  agent_id = CASE WHEN ?2 = 1 THEN ?3 ELSE agent_id END,
                  session_id = CASE WHEN ?4 = 1 THEN ?5 ELSE session_id END,
                  template_key = COALESCE(?6, template_key),
                  display_name = COALESCE(?7, display_name),
                  instructions = CASE WHEN ?8 = 1 THEN ?9 ELSE instructions END,
                  run_defaults_json = COALESCE(?10, run_defaults_json),
                  session_mode = COALESCE(?11, session_mode),
                  last_run_id = CASE WHEN ?12 = 1 THEN ?13 ELSE last_run_id END,
                  last_run_status = CASE WHEN ?14 = 1 THEN ?15 ELSE last_run_status END,
                  last_stop_reason = CASE WHEN ?16 = 1 THEN ?17 ELSE last_stop_reason END,
                  pending_approval_id = CASE WHEN ?18 = 1 THEN ?19 ELSE pending_approval_id END,
                  archived_at = CASE WHEN ?20 = 1 THEN ?21 ELSE archived_at END,
                  updated_at = ?22
                WHERE boss_key = ?23 AND worker_key = ?24
                "#,
                params![
                    status,
                    if apply_agent_id { 1_i64 } else { 0_i64 },
                    agent_id,
                    if apply_session_id { 1_i64 } else { 0_i64 },
                    session_id,
                    template_key,
                    display_name,
                    if apply_instructions { 1_i64 } else { 0_i64 },
                    instructions,
                    run_defaults_json,
                    session_mode,
                    if apply_last_run_id { 1_i64 } else { 0_i64 },
                    last_run_id,
                    if apply_last_run_status { 1_i64 } else { 0_i64 },
                    last_run_status,
                    if apply_last_stop_reason { 1_i64 } else { 0_i64 },
                    last_stop_reason,
                    if apply_pending_approval_id {
                        1_i64
                    } else {
                        0_i64
                    },
                    pending_approval_id,
                    if apply_archived_at { 1_i64 } else { 0_i64 },
                    archived_at,
                    now,
                    boss_key,
                    worker_key
                ],
            )
            .context("failed to update assistant worker")?;
        if rows_updated == 0 {
            tx.commit()
                .context("failed to commit assistant worker update transaction")?;
            return Ok(None);
        }

        let record = {
            let mut stmt = tx
                .prepare(
                    r#"
                    SELECT
                      boss_key,
                      root_session_id,
                      worker_key,
                      worker_kind,
                      status,
                      agent_id,
                      session_id,
                      template_key,
                      display_name,
                      instructions,
                      run_defaults_json,
                      session_mode,
                      last_run_id,
                      last_run_status,
                      last_stop_reason,
                      pending_approval_id,
                      created_at,
                      updated_at,
                      archived_at
                    FROM assistant_workers
                    WHERE boss_key = ?1 AND worker_key = ?2
                    "#,
                )
                .context("failed to prepare assistant worker reload query")?;
            stmt.query_row(params![boss_key, worker_key], map_assistant_worker_row)
                .optional()
                .context("failed to reload assistant worker after update")?
        };
        tx.commit()
            .context("failed to commit assistant worker update transaction")?;
        Ok(record)
    }

    pub fn create_assistant_task_link(
        &self,
        boss_key: &str,
        worker_key: &str,
        run_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .context("failed to start assistant task-link transaction")?;
        let run_session_id: Option<String> = tx
            .query_row(
                "SELECT session_id FROM runs WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .optional()
            .context("failed to validate run session for assistant task link")?;
        match run_session_id.as_deref() {
            Some(value) if value == session_id => {}
            Some(_) => anyhow::bail!("run_id does not belong to session_id"),
            None => anyhow::bail!("run does not exist"),
        }

        let conflict_exists = tx
            .query_row(
                r#"
                SELECT 1
                FROM assistant_task_links
                WHERE (run_id = ?1 OR session_id = ?2)
                  AND NOT (run_id = ?1 AND session_id = ?2)
                LIMIT 1
                "#,
                params![run_id, session_id],
                |_| Ok(()),
            )
            .optional()
            .context("failed to validate assistant task-link consistency")?
            .is_some();
        if conflict_exists {
            anyhow::bail!("assistant task-link conflict for run/session pair");
        }

        let worker_active = tx
            .query_row(
                r#"
                SELECT 1
                FROM assistant_workers
                WHERE boss_key = ?1
                  AND worker_key = ?2
                  AND archived_at IS NULL
                  AND status != 'archived'
                LIMIT 1
                "#,
                params![boss_key, worker_key],
                |_| Ok(()),
            )
            .optional()
            .context("failed to validate assistant worker before task-link create")?
            .is_some();
        if !worker_active {
            anyhow::bail!("assistant worker missing or inactive");
        }

        let duplicate_exists = tx
            .query_row(
                r#"
                SELECT 1
                FROM assistant_task_links
                WHERE boss_key = ?1
                  AND worker_key = ?2
                  AND run_id = ?3
                  AND session_id = ?4
                LIMIT 1
                "#,
                params![boss_key, worker_key, run_id, session_id],
                |_| Ok(()),
            )
            .optional()
            .context("failed to validate assistant task-link duplicate state")?
            .is_some();
        if duplicate_exists {
            anyhow::bail!("assistant task-link already exists");
        }

        let now = now_ms();
        tx.execute(
            r#"
            INSERT INTO assistant_task_links (
              boss_key, worker_key, run_id, session_id, linked_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![boss_key, worker_key, run_id, session_id, now],
        )
        .context("failed to create assistant task link")?;
        tx.commit()
            .context("failed to commit assistant task-link transaction")?;
        Ok(())
    }

    pub fn assistant_task_link_exists(
        &self,
        boss_key: &str,
        worker_key: &str,
        run_id: &str,
    ) -> Result<bool> {
        let conn = self.connect()?;
        let exists = conn
            .query_row(
                r#"
                SELECT 1
                FROM assistant_task_links
                WHERE boss_key = ?1
                  AND worker_key = ?2
                  AND run_id = ?3
                LIMIT 1
                "#,
                params![boss_key, worker_key, run_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(exists)
    }

    pub fn create_assistant_tool_call_audit(
        &self,
        event: NewAssistantToolCallAudit,
    ) -> Result<AssistantToolCallAuditRecord> {
        let conn = self.connect()?;
        let now = now_ms();
        let event_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            r#"
            INSERT INTO assistant_tool_calls_audit (
              event_id,
              request_id,
              boss_key,
              root_session_id,
              root_run_id,
              caller_agent_id,
              tool_name,
              decision,
              reason_code,
              audit_ref,
              metadata_json,
              created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                event_id,
                event.request_id,
                event.boss_key,
                event.root_session_id,
                event.root_run_id,
                event.caller_agent_id,
                event.tool_name,
                event.decision,
                event.reason_code,
                event.audit_ref,
                event.metadata_json,
                now
            ],
        )
        .context("failed to create assistant tool call audit event")?;
        Ok(AssistantToolCallAuditRecord {
            event_id,
            request_id: event.request_id,
            created_at: now,
        })
    }

    pub fn get_assistant_tool_call_audit(
        &self,
        event_id: &str,
    ) -> Result<Option<AssistantToolCallAuditRecord>> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT event_id, request_id, created_at FROM assistant_tool_calls_audit WHERE event_id = ?1",
            params![event_id],
            |row| Ok(AssistantToolCallAuditRecord {
                event_id: row.get(0)?, request_id: row.get(1)?, created_at: row.get(2)?,
            }),
        ).optional().context("failed reading assistant tool call audit by exact identity")
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

    pub fn replace_auth_profile(
        &self,
        auth_profile_id: &str,
        replacement: NewAuthProfile,
    ) -> Result<Option<AuthProfileRecord>> {
        let conn = self.connect()?;
        let now = now_ms();
        conn.execute(
            r#"
            UPDATE auth_profiles
            SET provider = ?1,
                display_name = ?2,
                auth_mode = ?3,
                risk_level = ?4,
                enabled = ?5,
                kill_switch_scope = ?6,
                api_base_url = ?7,
                credentials_json = ?8,
                updated_at = ?9
            WHERE auth_profile_id = ?10
            "#,
            params![
                replacement.provider,
                replacement.display_name,
                replacement.auth_mode,
                replacement.risk_level,
                if replacement.enabled { 1 } else { 0 },
                replacement.kill_switch_scope,
                replacement.api_base_url,
                replacement.credentials_json,
                now,
                auth_profile_id
            ],
        )
        .context("failed to replace auth profile")?;
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

    pub fn count_security_audit_events_before(&self, created_before: i64) -> Result<i64> {
        let conn = self.connect()?;
        let count = conn.query_row(
            r#"
            SELECT COUNT(1)
            FROM security_audit_events
            WHERE created_at < ?1
            "#,
            params![created_before],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count)
    }

    pub fn archive_security_audit_events_before(&self, created_before: i64) -> Result<i64> {
        let conn = self.connect()?;
        let archived_at = now_ms();
        let inserted = conn
            .execute(
                r#"
                INSERT OR IGNORE INTO security_audit_events_archive (
                  event_id, request_id, correlation_id, principal, action, resource,
                  decision, reason, transport, status, error_code, session_id, run_id,
                  metadata_json, created_at, archived_at
                )
                SELECT
                  event_id, request_id, correlation_id, principal, action, resource,
                  decision, reason, transport, status, error_code, session_id, run_id,
                  metadata_json, created_at, ?2
                FROM security_audit_events
                WHERE created_at < ?1
                "#,
                params![created_before, archived_at],
            )
            .context("failed to archive security audit events")?;
        Ok(inserted as i64)
    }

    pub fn delete_security_audit_events_before(&self, created_before: i64) -> Result<i64> {
        let conn = self.connect()?;
        let deleted = conn
            .execute(
                "DELETE FROM security_audit_events WHERE created_at < ?1",
                params![created_before],
            )
            .context("failed to delete old security audit events")?;
        Ok(deleted as i64)
    }

    pub fn get_archived_security_audit_event(
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
            FROM security_audit_events_archive
            WHERE event_id = ?1
            "#,
        )?;
        let record = stmt
            .query_row(params![event_id], map_security_audit_event_row)
            .optional()?;
        Ok(record)
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

    pub fn get_daily_auth_profile_usage(
        &self,
        usage_day_utc: &str,
        auth_profile_id: &str,
    ) -> Result<Option<DailyAuthProfileUsageRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              usage_day_utc, auth_profile_id, provider,
              input_chars, output_chars, input_tokens, output_tokens, total_tokens,
              estimated_cost_usd, updated_at
            FROM daily_auth_profile_usage
            WHERE usage_day_utc = ?1 AND auth_profile_id = ?2
            "#,
        )?;
        let record = stmt
            .query_row(params![usage_day_utc, auth_profile_id], |row| {
                Ok(DailyAuthProfileUsageRecord {
                    usage_day_utc: row.get(0)?,
                    auth_profile_id: row.get(1)?,
                    provider: row.get(2)?,
                    input_chars: row.get(3)?,
                    output_chars: row.get(4)?,
                    input_tokens: row.get(5)?,
                    output_tokens: row.get(6)?,
                    total_tokens: row.get(7)?,
                    estimated_cost_usd: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .optional()?;
        Ok(record)
    }

    pub fn increment_daily_auth_profile_usage(
        &self,
        increment: DailyAuthProfileUsageIncrement,
    ) -> Result<DailyAuthProfileUsageRecord> {
        let conn = self.connect()?;
        let updated_at = now_ms();
        conn.execute(
            r#"
            INSERT INTO daily_auth_profile_usage (
              usage_day_utc, auth_profile_id, provider,
              input_chars, output_chars, input_tokens, output_tokens, total_tokens,
              estimated_cost_usd, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(usage_day_utc, auth_profile_id) DO UPDATE SET
              provider = excluded.provider,
              input_chars = daily_auth_profile_usage.input_chars + excluded.input_chars,
              output_chars = daily_auth_profile_usage.output_chars + excluded.output_chars,
              input_tokens = daily_auth_profile_usage.input_tokens + excluded.input_tokens,
              output_tokens = daily_auth_profile_usage.output_tokens + excluded.output_tokens,
              total_tokens = daily_auth_profile_usage.total_tokens + excluded.total_tokens,
              estimated_cost_usd = daily_auth_profile_usage.estimated_cost_usd + excluded.estimated_cost_usd,
              updated_at = excluded.updated_at
            "#,
            params![
                increment.usage_day_utc,
                increment.auth_profile_id,
                increment.provider,
                increment.input_chars,
                increment.output_chars,
                increment.input_tokens,
                increment.output_tokens,
                increment.total_tokens,
                increment.estimated_cost_usd,
                updated_at
            ],
        )
        .context("failed to upsert daily auth profile usage")?;
        self.get_daily_auth_profile_usage(&increment.usage_day_utc, &increment.auth_profile_id)?
            .context("daily auth profile usage missing after upsert")
    }

    pub fn list_run_usage_samples_between(
        &self,
        start_ms: i64,
        end_ms: i64,
        limit: u32,
    ) -> Result<Vec<RunUsageSampleRecord>> {
        let bounded_limit = limit.clamp(1, 50_000);
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
              r.run_id,
              r.session_id,
              s.agent_id,
              COALESCE(a.name, s.agent_id) AS agent_name,
              r.model_provider,
              r.model_id,
              r.usage_json,
              COALESCE(r.ended_at, r.started_at, r.created_at) AS sample_ts_ms
            FROM runs r
            INNER JOIN sessions s ON s.session_id = r.session_id
            LEFT JOIN agents a ON a.agent_id = s.agent_id
            WHERE r.usage_json IS NOT NULL
              AND COALESCE(r.ended_at, r.started_at, r.created_at) >= ?1
              AND COALESCE(r.ended_at, r.started_at, r.created_at) < ?2
            ORDER BY sample_ts_ms ASC, r.rowid ASC
            LIMIT ?3
            "#,
        )?;
        let mut rows = stmt.query(params![start_ms, end_ms, bounded_limit])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(RunUsageSampleRecord {
                run_id: row.get(0)?,
                session_id: row.get(1)?,
                agent_id: row.get(2)?,
                agent_name: row.get(3)?,
                model_provider: row.get(4)?,
                model_id: row.get(5)?,
                usage_json: row.get(6)?,
                sample_ts_ms: row.get(7)?,
            });
        }
        Ok(items)
    }

    pub fn get_circuit_breaker_state(
        &self,
        scope: &str,
        target_id: &str,
    ) -> Result<Option<CircuitBreakerStateRecord>> {
        let conn = self.connect()?;
        let breaker_key = format!("{scope}:{target_id}");
        let mut stmt = conn.prepare(
            r#"
            SELECT
              breaker_key, scope, target_id, state, consecutive_failures,
              opened_at, cooldown_until, last_error_code, updated_at
            FROM circuit_breaker_states
            WHERE breaker_key = ?1
            "#,
        )?;
        let record = stmt
            .query_row(params![breaker_key], |row| {
                Ok(CircuitBreakerStateRecord {
                    breaker_key: row.get(0)?,
                    scope: row.get(1)?,
                    target_id: row.get(2)?,
                    state: row.get(3)?,
                    consecutive_failures: row.get(4)?,
                    opened_at: row.get(5)?,
                    cooldown_until: row.get(6)?,
                    last_error_code: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .optional()?;
        Ok(record)
    }

    pub fn list_circuit_breaker_states(
        &self,
        limit: u32,
        scope: Option<&str>,
    ) -> Result<Vec<CircuitBreakerStateRecord>> {
        let conn = self.connect()?;
        let mut out = Vec::new();
        if let Some(scope) = scope {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                  breaker_key, scope, target_id, state, consecutive_failures,
                  opened_at, cooldown_until, last_error_code, updated_at
                FROM circuit_breaker_states
                WHERE scope = ?1
                ORDER BY updated_at DESC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![scope, i64::from(limit)], |row| {
                Ok(CircuitBreakerStateRecord {
                    breaker_key: row.get(0)?,
                    scope: row.get(1)?,
                    target_id: row.get(2)?,
                    state: row.get(3)?,
                    consecutive_failures: row.get(4)?,
                    opened_at: row.get(5)?,
                    cooldown_until: row.get(6)?,
                    last_error_code: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                  breaker_key, scope, target_id, state, consecutive_failures,
                  opened_at, cooldown_until, last_error_code, updated_at
                FROM circuit_breaker_states
                ORDER BY updated_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt.query_map(params![i64::from(limit)], |row| {
                Ok(CircuitBreakerStateRecord {
                    breaker_key: row.get(0)?,
                    scope: row.get(1)?,
                    target_id: row.get(2)?,
                    state: row.get(3)?,
                    consecutive_failures: row.get(4)?,
                    opened_at: row.get(5)?,
                    cooldown_until: row.get(6)?,
                    last_error_code: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn upsert_circuit_breaker_state(
        &self,
        upsert: CircuitBreakerStateUpsert,
    ) -> Result<CircuitBreakerStateRecord> {
        let conn = self.connect()?;
        let updated_at = now_ms();
        let breaker_key = format!("{}:{}", upsert.scope, upsert.target_id);
        conn.execute(
            r#"
            INSERT INTO circuit_breaker_states (
              breaker_key, scope, target_id, state, consecutive_failures,
              opened_at, cooldown_until, last_error_code, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(breaker_key) DO UPDATE SET
              scope = excluded.scope,
              target_id = excluded.target_id,
              state = excluded.state,
              consecutive_failures = excluded.consecutive_failures,
              opened_at = excluded.opened_at,
              cooldown_until = excluded.cooldown_until,
              last_error_code = excluded.last_error_code,
              updated_at = excluded.updated_at
            "#,
            params![
                breaker_key,
                upsert.scope,
                upsert.target_id,
                upsert.state,
                upsert.consecutive_failures,
                upsert.opened_at,
                upsert.cooldown_until,
                upsert.last_error_code,
                updated_at
            ],
        )
        .context("failed to upsert circuit breaker state")?;
        self.get_circuit_breaker_state(&upsert.scope, &upsert.target_id)?
            .context("circuit breaker state missing after upsert")
    }

    pub fn clear_circuit_breaker_state(&self, scope: &str, target_id: &str) -> Result<()> {
        let conn = self.connect()?;
        let breaker_key = format!("{scope}:{target_id}");
        conn.execute(
            "DELETE FROM circuit_breaker_states WHERE breaker_key = ?1",
            params![breaker_key],
        )
        .context("failed to clear circuit breaker state")?;
        Ok(())
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
        if execass::is_execass_continuation_job_payload(&new_job.payload_json)
            || execass::is_execass_routine_driver_payload(&new_job.payload_json)
            || execass::is_execass_routine_trigger_payload(&new_job.payload_json)
        {
            bail!("reserved ExecAss jobs must be created by their typed reconciler");
        }
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
        if execass::is_execass_continuation_job_payload(&current.payload_json)
            || execass::is_execass_routine_driver_payload(&current.payload_json)
            || execass::is_execass_routine_trigger_payload(&current.payload_json)
        {
            bail!("reserved ExecAss jobs cannot be changed through the generic job API");
        }
        let now = now_ms();
        let next_name = patch.name.unwrap_or(current.name);
        let next_enabled = patch.enabled.unwrap_or(current.enabled);
        let next_schedule_kind = patch.schedule_kind.unwrap_or(current.schedule_kind);
        let mut next_interval = patch.interval_seconds.or(current.interval_seconds);
        let mut next_run_at = patch.run_at_ms.or(current.run_at_ms);
        let next_next_run_at = patch.next_run_at.or(current.next_run_at);
        let next_payload = patch.payload_json.unwrap_or(current.payload_json);
        if execass::is_execass_continuation_job_payload(&next_payload)
            || execass::is_execass_routine_driver_payload(&next_payload)
            || execass::is_execass_routine_trigger_payload(&next_payload)
        {
            bail!("generic job updates cannot forge a reserved ExecAss payload");
        }
        let next_max_retries = patch.max_retries.unwrap_or(current.max_retries);
        let next_retry_backoff = patch.retry_backoff_ms.unwrap_or(current.retry_backoff_ms);
        let next_timeout = patch.timeout_ms.unwrap_or(current.timeout_ms);
        match next_schedule_kind.as_str() {
            "cron" => {
                next_interval = None;
                next_run_at = None;
            }
            "once" | "at" => {
                next_interval = None;
            }
            "interval" | "every" => {}
            _ => {}
        }

        conn.execute(
            r#"
            UPDATE jobs
            SET name = ?1,
                enabled = ?2,
                schedule_kind = ?3,
                interval_seconds = ?4,
                run_at_ms = ?5,
                next_run_at = ?6,
                payload_json = ?7,
                max_retries = ?8,
                retry_backoff_ms = ?9,
                timeout_ms = ?10,
                updated_at = ?11
            WHERE job_id = ?12 AND deleted_at IS NULL
            "#,
            params![
                next_name,
                if next_enabled { 1 } else { 0 },
                next_schedule_kind,
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
        if let Some(current) = self.get_job(job_id)? {
            if execass::is_execass_continuation_job_payload(&current.payload_json)
                || execass::is_execass_routine_driver_payload(&current.payload_json)
                || execass::is_execass_routine_trigger_payload(&current.payload_json)
            {
                bail!("reserved ExecAss jobs cannot be removed through the generic job API");
            }
        }
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
        let resolved_status = match decision {
            "approve" => "approved",
            "deny" => "denied",
            other => anyhow::bail!("invalid approval decision: {other}"),
        };
        let decided_at = now_ms();
        let conn = self.connect()?;
        let updated_rows = conn
            .execute(
                r#"
            UPDATE approvals
            SET status = ?1, decided_at = ?2, decided_via = ?3, decided_by_peer_id = ?4
            WHERE approval_id = ?5
              AND status = 'requested'
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
        if updated_rows == 0 {
            return match get_approval_with_conn(&conn, approval_id)? {
                Some(record) => Ok(ApprovalResolveResult::AlreadyResolved(record)),
                None => Ok(ApprovalResolveResult::NotFound),
            };
        }

        let updated = get_approval_with_conn(&conn, approval_id)?
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
        open_sqlite_connection(&self.db_path)
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
                "SELECT 1 FROM agents WHERE agent_id = ?1 AND archived_at IS NULL LIMIT 1",
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

    fn ensure_board_exists(&self, conn: &Connection, board_id: &str) -> Result<()> {
        let exists = conn
            .query_row(
                "SELECT 1 FROM boards WHERE board_id = ?1 AND archived_at IS NULL LIMIT 1",
                params![board_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            Ok(())
        } else {
            anyhow::bail!("board does not exist: {board_id}");
        }
    }

    fn ensure_column_in_board(
        &self,
        conn: &Connection,
        board_id: &str,
        column_id: &str,
    ) -> Result<()> {
        let exists = conn
            .query_row(
                r#"
                SELECT 1
                FROM board_columns
                WHERE board_id = ?1
                  AND column_id = ?2
                  AND archived_at IS NULL
                LIMIT 1
                "#,
                params![board_id, column_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            Ok(())
        } else {
            anyhow::bail!("column does not belong to board");
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

const GOAL_STATUS_ACTIVE: &str = "active";
const GOAL_STATUS_AT_RISK: &str = "at_risk";
const GOAL_STATUS_COMPLETED: &str = "completed";
const GOAL_STATUS_ARCHIVED: &str = "archived";
const PROJECT_STATUS_ACTIVE: &str = "active";
const PROJECT_STATUS_BLOCKED: &str = "blocked";
const PROJECT_STATUS_COMPLETED: &str = "completed";
const PROJECT_STATUS_ARCHIVED: &str = "archived";
const TASK_STATUS_TODO: &str = "todo";
const TASK_STATUS_IN_PROGRESS: &str = "in_progress";
const TASK_STATUS_BLOCKED: &str = "blocked";
const TASK_STATUS_DONE: &str = "done";
const TASK_STATUS_ARCHIVED: &str = "archived";
const TASK_PRIORITY_LOW: &str = "low";
const TASK_PRIORITY_NORMAL: &str = "normal";
const TASK_PRIORITY_HIGH: &str = "high";
const TASK_PRIORITY_CRITICAL: &str = "critical";
const STRATEGY_STALE_THRESHOLD_MS: i64 = 72 * 60 * 60_000;
const CONNECTOR_STATUS_DRAFT: &str = "draft";
const CONNECTOR_STATUS_CONVERTED: &str = "converted";
const CONNECTOR_STATUS_UNDER_REVIEW: &str = "under_review";
const CONNECTOR_STATUS_ENABLED: &str = "enabled";
const CONNECTOR_STATUS_DISABLED: &str = "disabled";
const CONNECTOR_STATUS_ERROR: &str = "error";
const CONNECTOR_TRUST_TRUSTED_CURATED: &str = "trusted_curated";
const CONNECTOR_TRUST_LOCAL_UNTRUSTED: &str = "local_untrusted";
const CONNECTOR_TRUST_REVIEWED_LOCAL: &str = "reviewed_local";
const CONNECTOR_TRUST_BLOCKED: &str = "blocked";
const CONNECTOR_CONVERSION_PENDING: &str = "pending";
const CONNECTOR_CONVERSION_RUNNING: &str = "running";
const CONNECTOR_CONVERSION_SUCCEEDED: &str = "succeeded";
const CONNECTOR_CONVERSION_FAILED: &str = "failed";
const CONNECTOR_INTERACTION_PENDING: &str = "pending";
const CONNECTOR_INTERACTION_WAITING: &str = "waiting_on_operator";
const CONNECTOR_INTERACTION_RESUMED: &str = "resumed";
const CONNECTOR_INTERACTION_CANCELLED: &str = "cancelled";
const CONNECTOR_INTERACTION_EXPIRED: &str = "expired";
const CONNECTOR_WRITE_READ_ONLY: &str = "read_only";
const CONNECTOR_WRITE_OPERATOR_GATED: &str = "operator_write_gated";
const CONNECTOR_WRITE_DESTRUCTIVE_GATED: &str = "destructive_write_gated";
const CONNECTOR_WRITE_UNSAFE_BLOCKED: &str = "unsafe_blocked";
const CONNECTOR_DEPRECATION_ACTIVE: &str = "active";
const CONNECTOR_DEPRECATION_UNPUBLISHED: &str = "unpublished";
const CONNECTOR_DEPRECATION_SUPERSEDED: &str = "superseded";

#[derive(Debug, Clone)]
struct RuntimeCandidate {
    latest_run_id: Option<String>,
    latest_session_id: Option<String>,
    sort_ms: i64,
}

fn get_agent_with_conn(conn: &Connection, agent_id: &str) -> Result<Option<AgentRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          agent_id, name, workspace_root, model_provider, model_id, tool_profile,
          reports_to_agent_id, role_label,
          memory_binding_id, memory_provider_kind, memory_base_url, memory_auth_mode,
          memory_auth_secret_ref, memory_principal_id, memory_principal_display_name,
          memory_enabled, memory_trusted_local_operator_actions,
          created_at, updated_at
        FROM agents
        WHERE agent_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![agent_id], map_agent_row)
        .optional()?)
}

fn get_goal_with_conn(conn: &Connection, goal_id: &str) -> Result<Option<GoalRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          goal_id, slug, title, summary, status, owner_agent_id, target_date, created_at, updated_at
        FROM goals
        WHERE goal_id = ?1
        "#,
    )?;
    Ok(stmt.query_row(params![goal_id], map_goal_row).optional()?)
}

fn get_project_with_conn(conn: &Connection, project_id: &str) -> Result<Option<ProjectRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          project_id, goal_id, slug, name, summary, status, owner_agent_id, workspace_root,
          budget_month_usd, created_at, updated_at
        FROM projects
        WHERE project_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![project_id], map_project_row)
        .optional()?)
}

fn get_task_with_conn(conn: &Connection, task_id: &str) -> Result<Option<TaskRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          task_id, project_id, parent_task_id, title, detail, status, priority, owner_agent_id,
          due_at, blocked_reason, linked_board_card_id, linked_job_id, created_at, updated_at
        FROM tasks
        WHERE task_id = ?1
        "#,
    )?;
    Ok(stmt.query_row(params![task_id], map_task_row).optional()?)
}

fn get_bootstrap_preset_with_conn(
    conn: &Connection,
    preset_key: &str,
) -> Result<Option<BootstrapPresetRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          preset_key, display_name, description, role_label, provider_path, default_model_provider,
          default_model_id, default_tool_profile, default_workspace_root, default_reports_to_agent_id,
          setup_notes, created_at, updated_at
        FROM bootstrap_presets
        WHERE preset_key = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![preset_key], map_bootstrap_preset_row)
        .optional()?)
}

fn get_connector_with_conn(
    conn: &Connection,
    connector_id: &str,
) -> Result<Option<ConnectorSourceRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          cs.connector_id,
          cs.slug,
          cs.display_name,
          cs.source_kind,
          cs.origin_kind,
          cs.catalog_item_id,
          cs.current_version_id,
          cs.latest_imported_version_id,
          cs.status,
          cs.trust_state,
          (
            SELECT COUNT(1)
            FROM connector_assignments a
            WHERE a.connector_id = cs.connector_id
              AND a.enabled = 1
          ) AS assigned_agent_count,
          (
            SELECT COUNT(1)
            FROM connector_published_tools pt
            WHERE pt.connector_id = cs.connector_id
              AND pt.unpublished_at IS NULL
              AND (
                cs.current_version_id IS NULL
                OR pt.version_id = cs.current_version_id
              )
          ) AS published_tool_count,
          cs.last_conversion_at,
          cs.last_review_at,
          cs.last_enabled_at,
          cs.last_disabled_at,
          cs.created_at,
          cs.updated_at
        FROM connector_sources cs
        WHERE cs.connector_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![connector_id], map_connector_source_row)
        .optional()?)
}

fn get_connector_by_slug_with_conn(
    conn: &Connection,
    slug: &str,
) -> Result<Option<ConnectorSourceRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          cs.connector_id,
          cs.slug,
          cs.display_name,
          cs.source_kind,
          cs.origin_kind,
          cs.catalog_item_id,
          cs.current_version_id,
          cs.latest_imported_version_id,
          cs.status,
          cs.trust_state,
          (
            SELECT COUNT(1)
            FROM connector_assignments a
            WHERE a.connector_id = cs.connector_id
              AND a.enabled = 1
          ) AS assigned_agent_count,
          (
            SELECT COUNT(1)
            FROM connector_published_tools pt
            WHERE pt.connector_id = cs.connector_id
              AND pt.unpublished_at IS NULL
              AND (
                cs.current_version_id IS NULL
                OR pt.version_id = cs.current_version_id
              )
          ) AS published_tool_count,
          cs.last_conversion_at,
          cs.last_review_at,
          cs.last_enabled_at,
          cs.last_disabled_at,
          cs.created_at,
          cs.updated_at
        FROM connector_sources cs
        WHERE LOWER(cs.slug) = LOWER(?1)
        "#,
    )?;
    Ok(stmt
        .query_row(params![slug], map_connector_source_row)
        .optional()?)
}

fn get_connector_version_with_conn(
    conn: &Connection,
    version_id: &str,
) -> Result<Option<ConnectorVersionRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          version_id, connector_id, version_label, source_digest, raw_source_location,
          import_metadata_json, schema_summary_json, latest_conversion_id,
          external_reference_policy, created_at, updated_at
        FROM connector_versions
        WHERE version_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![version_id], map_connector_version_row)
        .optional()?)
}

fn get_connector_conversion_with_conn(
    conn: &Connection,
    conversion_id: &str,
) -> Result<Option<ConnectorConversionRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          conversion_id, connector_id, version_id, status, warnings_json, proposed_tools_json,
          write_capable_tools, unsupported_operations_json, normalization_notes_json,
          diff_from_previous_json, created_at, updated_at
        FROM connector_conversions
        WHERE conversion_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![conversion_id], map_connector_conversion_row)
        .optional()?)
}

fn get_connector_published_tool_with_conn(
    conn: &Connection,
    published_tool_id: &str,
) -> Result<Option<ConnectorPublishedToolRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          published_tool_id, connector_id, version_id, conversion_id, tool_name, display_name,
          tool_schema_json, origin_metadata_json, write_classification, published_at,
          unpublished_at, superseded_by_published_tool_id, deprecation_state
        FROM connector_published_tools
        WHERE published_tool_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![published_tool_id], map_connector_published_tool_row)
        .optional()?)
}

fn get_connector_assignment_with_conn(
    conn: &Connection,
    connector_id: &str,
    agent_id: &str,
) -> Result<Option<ConnectorAssignmentRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          assignment_id, connector_id, agent_id, enabled, auth_mode, created_at, updated_at
        FROM connector_assignments
        WHERE connector_id = ?1 AND agent_id = ?2
        "#,
    )?;
    Ok(stmt
        .query_row(
            params![connector_id, agent_id],
            map_connector_assignment_row,
        )
        .optional()?)
}

fn get_connector_auth_binding_with_conn(
    conn: &Connection,
    connector_id: &str,
    agent_id: Option<&str>,
) -> Result<Option<ConnectorAuthBindingRecord>> {
    let query = if agent_id.is_some() {
        r#"
        SELECT
          auth_binding_id, connector_id, agent_id, auth_kind, secret_ref, oauth_session_id,
          status, auth_metadata_json, last_success_at, last_error, last_rotated_at, created_at, updated_at
        FROM connector_auth_bindings
        WHERE connector_id = ?1 AND agent_id = ?2
        ORDER BY updated_at DESC, auth_binding_id ASC
        LIMIT 1
        "#
    } else {
        r#"
        SELECT
          auth_binding_id, connector_id, agent_id, auth_kind, secret_ref, oauth_session_id,
          status, auth_metadata_json, last_success_at, last_error, last_rotated_at, created_at, updated_at
        FROM connector_auth_bindings
        WHERE connector_id = ?1 AND agent_id IS NULL
        ORDER BY updated_at DESC, auth_binding_id ASC
        LIMIT 1
        "#
    };
    let mut stmt = conn.prepare(query)?;
    let binding = if let Some(agent_id) = agent_id {
        stmt.query_row(
            params![connector_id, agent_id],
            map_connector_auth_binding_row,
        )
    } else {
        stmt.query_row(params![connector_id], map_connector_auth_binding_row)
    };
    Ok(binding.optional()?)
}

fn get_connector_interaction_with_conn(
    conn: &Connection,
    interaction_id: &str,
) -> Result<Option<ConnectorInteractionRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          interaction_id, connector_id, agent_id, interaction_kind, status, prompt_summary,
          resume_token, expires_at, consumed_at, detail_json, created_at, updated_at
        FROM connector_interactions
        WHERE interaction_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![interaction_id], map_connector_interaction_row)
        .optional()?)
}

fn list_connector_published_tools_with_conn(
    conn: &Connection,
    connector_id: &str,
    include_unpublished: bool,
) -> Result<Vec<ConnectorPublishedToolRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          published_tool_id, connector_id, version_id, conversion_id, tool_name, display_name,
          tool_schema_json, origin_metadata_json, write_classification, published_at,
          unpublished_at, superseded_by_published_tool_id, deprecation_state
        FROM connector_published_tools
        WHERE connector_id = ?1
        ORDER BY published_at DESC, published_tool_id ASC
        "#,
    )?;
    let rows = stmt.query_map(params![connector_id], map_connector_published_tool_row)?;
    let mut items = Vec::new();
    for row in rows {
        let record = row?;
        if include_unpublished || record.unpublished_at.is_none() {
            items.push(record);
        }
    }
    Ok(items)
}

fn get_approval_with_conn(conn: &Connection, approval_id: &str) -> Result<Option<ApprovalRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          approval_id, run_id, tool_call_id, kind, status, request_summary, request_json,
          requested_at, decided_at, decided_via, decided_by_peer_id
        FROM approvals
        WHERE approval_id = ?1
        "#,
    )?;
    Ok(stmt
        .query_row(params![approval_id], map_approval_row)
        .optional()?)
}

fn get_job_with_conn(conn: &Connection, job_id: &str) -> Result<Option<JobRecord>> {
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
    Ok(stmt.query_row(params![job_id], map_job_row).optional()?)
}

fn latest_run_for_session_with_conn(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<RunRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          run_id, session_id, status, model_provider, model_id, started_at, ended_at, error_text,
          usage_json, created_at
        FROM runs
        WHERE session_id = ?1
        ORDER BY COALESCE(started_at, created_at) DESC, created_at DESC, run_id DESC
        LIMIT 1
        "#,
    )?;
    Ok(stmt
        .query_row(params![session_id], map_run_row)
        .optional()?)
}

fn load_all_projects(conn: &Connection) -> Result<Vec<ProjectRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          project_id, goal_id, slug, name, summary, status, owner_agent_id, workspace_root,
          budget_month_usd, created_at, updated_at
        FROM projects
        "#,
    )?;
    let rows = stmt.query_map([], map_project_row)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

fn ensure_goal_exists(conn: &Connection, goal_id: &str) -> Result<()> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM goals WHERE goal_id = ?1 LIMIT 1",
            params![goal_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        Ok(())
    } else {
        anyhow::bail!("goal does not exist: {goal_id}");
    }
}

fn ensure_project_accepts_task_changes(conn: &Connection, project_id: &str) -> Result<()> {
    let project = get_project_with_conn(conn, project_id)?
        .with_context(|| format!("project does not exist: {project_id}"))?;
    if matches!(
        project.status.as_str(),
        PROJECT_STATUS_COMPLETED | PROJECT_STATUS_ARCHIVED
    ) {
        anyhow::bail!("project does not allow task changes: {project_id}");
    }
    Ok(())
}

fn validate_optional_owner_agent(
    _storage: &Storage,
    conn: &Connection,
    owner_agent_id: Option<&str>,
) -> Result<()> {
    if let Some(agent_id) = owner_agent_id {
        let exists = conn
            .query_row(
                "SELECT 1 FROM agents WHERE agent_id = ?1 AND archived_at IS NULL LIMIT 1",
                params![agent_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            anyhow::bail!("agent does not exist: {agent_id}");
        }
    }
    Ok(())
}

fn validate_agent_manager_assignment(
    conn: &Connection,
    agent_id: &str,
    reports_to_agent_id: Option<&str>,
) -> Result<()> {
    let Some(manager_id) = reports_to_agent_id else {
        return Ok(());
    };
    if manager_id == agent_id {
        anyhow::bail!("reports_to_agent_id cannot reference the same agent");
    }
    let mut current = Some(manager_id.to_string());
    while let Some(candidate) = current {
        if candidate == agent_id {
            anyhow::bail!("agent hierarchy cycle detected");
        }
        current =
            get_agent_with_conn(conn, &candidate)?.and_then(|record| record.reports_to_agent_id);
        if current.is_none() && get_agent_with_conn(conn, &candidate)?.is_none() {
            anyhow::bail!("reports_to_agent_id does not exist: {manager_id}");
        }
    }
    Ok(())
}

fn validate_goal_status(status: &str) -> Result<()> {
    match status.trim() {
        GOAL_STATUS_ACTIVE | GOAL_STATUS_AT_RISK | GOAL_STATUS_COMPLETED | GOAL_STATUS_ARCHIVED => {
            Ok(())
        }
        _ => anyhow::bail!("invalid goal status"),
    }
}

fn validate_project_status(status: &str) -> Result<()> {
    match status.trim() {
        PROJECT_STATUS_ACTIVE
        | PROJECT_STATUS_BLOCKED
        | PROJECT_STATUS_COMPLETED
        | PROJECT_STATUS_ARCHIVED => Ok(()),
        _ => anyhow::bail!("invalid project status"),
    }
}

fn validate_task_status(status: &str) -> Result<()> {
    match status.trim() {
        TASK_STATUS_TODO
        | TASK_STATUS_IN_PROGRESS
        | TASK_STATUS_BLOCKED
        | TASK_STATUS_DONE
        | TASK_STATUS_ARCHIVED => Ok(()),
        _ => anyhow::bail!("invalid task status"),
    }
}

fn validate_task_priority(priority: &str) -> Result<()> {
    match priority.trim() {
        TASK_PRIORITY_LOW | TASK_PRIORITY_NORMAL | TASK_PRIORITY_HIGH | TASK_PRIORITY_CRITICAL => {
            Ok(())
        }
        _ => anyhow::bail!("invalid task priority"),
    }
}

fn normalize_project_workspace_root(workspace_root: Option<&str>) -> Result<Option<String>> {
    let Some(value) = workspace_root else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(None);
    }
    if Path::new(trimmed).is_absolute() {
        return Ok(Some(trimmed.to_string()));
    }
    anyhow::bail!("workspace_root must be null, '.', or an absolute path");
}

fn normalize_optional_agent_reference(value: Option<&str>) -> Option<String> {
    value
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_agent_memory_binding_id(value: Option<&str>, agent_id: &str) -> Result<String> {
    let normalized = value
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| format!("mno-{agent_id}"));
    if normalized.len() > 128
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        anyhow::bail!(
            "memory binding_id must be 1..128 chars and contain only a-z, 0-9, '_' or '-'"
        );
    }
    Ok(normalized)
}

fn normalize_agent_memory_provider_kind(value: Option<&str>) -> Result<String> {
    let normalized = value
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .context("memory provider_kind is required")?;
    if normalized.len() > 64
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        anyhow::bail!(
            "memory provider_kind must be 1..64 chars and contain only a-z, 0-9, '_' or '-'"
        );
    }
    Ok(normalized)
}

fn normalize_agent_memory_base_url(value: Option<&str>) -> Result<String> {
    let normalized = value
        .map(|item| item.trim().trim_end_matches('/').to_string())
        .filter(|item| !item.is_empty())
        .context("memory base_url is required")?;
    if !(normalized.starts_with("http://") || normalized.starts_with("https://")) {
        anyhow::bail!("memory base_url must start with http:// or https://");
    }
    Ok(normalized)
}

fn normalize_agent_memory_auth_mode(value: Option<&str>) -> Result<String> {
    let normalized = value
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .context("memory auth_mode is required")?;
    match normalized.as_str() {
        "none" | "secret_ref" => Ok(normalized),
        _ => anyhow::bail!("memory auth_mode must be one of: none, secret_ref"),
    }
}

struct AgentMemoryBindingNormalizationInput<'a> {
    binding_id: Option<&'a str>,
    provider_kind: Option<&'a str>,
    base_url: Option<&'a str>,
    auth_mode: Option<&'a str>,
    auth_secret_ref: Option<&'a str>,
    principal_id: Option<&'a str>,
    principal_display_name: Option<&'a str>,
    enabled: bool,
    trusted_local_operator_actions: bool,
    agent_id: &'a str,
    agent_name: &'a str,
}

fn normalize_agent_memory_binding(
    input: AgentMemoryBindingNormalizationInput<'_>,
) -> Result<AgentMemoryBindingRecord> {
    let auth_mode = normalize_agent_memory_auth_mode(input.auth_mode)?;
    let auth_secret_ref = normalize_optional_text(input.auth_secret_ref);
    if auth_mode == "secret_ref" && auth_secret_ref.is_none() {
        anyhow::bail!("memory auth_secret_ref is required when auth_mode=secret_ref");
    }
    Ok(AgentMemoryBindingRecord {
        binding_id: normalize_agent_memory_binding_id(input.binding_id, input.agent_id)?,
        provider_kind: normalize_agent_memory_provider_kind(input.provider_kind)?,
        base_url: normalize_agent_memory_base_url(input.base_url)?,
        auth_mode,
        auth_secret_ref,
        principal_id: normalize_optional_text(input.principal_id)
            .or_else(|| Some(input.agent_id.to_string())),
        principal_display_name: normalize_optional_text(input.principal_display_name)
            .or_else(|| Some(input.agent_name.trim().to_string()).filter(|item| !item.is_empty())),
        enabled: input.enabled,
        trusted_local_operator_actions: input.trusted_local_operator_actions,
    })
}

fn normalize_new_agent_memory_binding(
    binding: Option<NewAgentMemoryBinding>,
    agent_id: &str,
    agent_name: &str,
) -> Result<Option<AgentMemoryBindingRecord>> {
    let Some(binding) = binding else {
        return Ok(None);
    };
    Ok(Some(normalize_agent_memory_binding(
        AgentMemoryBindingNormalizationInput {
            binding_id: Some(binding.binding_id.as_str()),
            provider_kind: Some(binding.provider_kind.as_str()),
            base_url: Some(binding.base_url.as_str()),
            auth_mode: Some(binding.auth_mode.as_str()),
            auth_secret_ref: binding.auth_secret_ref.as_deref(),
            principal_id: binding.principal_id.as_deref(),
            principal_display_name: binding.principal_display_name.as_deref(),
            enabled: binding.enabled,
            trusted_local_operator_actions: binding.trusted_local_operator_actions,
            agent_id,
            agent_name,
        },
    )?))
}

fn normalize_updated_agent_memory_binding(
    current: Option<AgentMemoryBindingRecord>,
    patch: Option<Option<AgentMemoryBindingUpdatePatch>>,
    agent_id: &str,
    agent_name: &str,
) -> Result<Option<AgentMemoryBindingRecord>> {
    match patch {
        None => Ok(current),
        Some(None) => Ok(None),
        Some(Some(patch)) => {
            let current = current.unwrap_or(AgentMemoryBindingRecord {
                binding_id: String::new(),
                provider_kind: String::new(),
                base_url: String::new(),
                auth_mode: String::new(),
                auth_secret_ref: None,
                principal_id: None,
                principal_display_name: None,
                enabled: false,
                trusted_local_operator_actions: false,
            });
            Ok(Some(normalize_agent_memory_binding(
                AgentMemoryBindingNormalizationInput {
                    binding_id: patch
                        .binding_id
                        .as_deref()
                        .or(Some(current.binding_id.as_str())),
                    provider_kind: patch
                        .provider_kind
                        .as_deref()
                        .or(Some(current.provider_kind.as_str())),
                    base_url: patch
                        .base_url
                        .as_deref()
                        .or(Some(current.base_url.as_str())),
                    auth_mode: patch
                        .auth_mode
                        .as_deref()
                        .or(Some(current.auth_mode.as_str())),
                    auth_secret_ref: match patch.auth_secret_ref {
                        Some(Some(ref value)) => Some(value.as_str()),
                        Some(None) => None,
                        None => current.auth_secret_ref.as_deref(),
                    },
                    principal_id: match patch.principal_id {
                        Some(Some(ref value)) => Some(value.as_str()),
                        Some(None) => None,
                        None => current.principal_id.as_deref(),
                    },
                    principal_display_name: match patch.principal_display_name {
                        Some(Some(ref value)) => Some(value.as_str()),
                        Some(None) => None,
                        None => current.principal_display_name.as_deref(),
                    },
                    enabled: patch.enabled.unwrap_or(current.enabled),
                    trusted_local_operator_actions: patch
                        .trusted_local_operator_actions
                        .unwrap_or(current.trusted_local_operator_actions),
                    agent_id,
                    agent_name,
                },
            )?))
        }
    }
}

fn validate_budget_month_usd(value: Option<f64>) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if value < 0.0 {
        anyhow::bail!("budget_month_usd must be non-negative");
    }
    let cents = value * 100.0;
    if (cents.round() - cents).abs() > 1e-9 {
        anyhow::bail!("budget_month_usd supports at most two decimal places");
    }
    Ok(())
}

fn normalize_connector_source_kind(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "mcp" | "openapi" | "graphql" => Ok(value),
        _ => anyhow::bail!("source_kind must be one of: mcp, openapi, graphql"),
    }
}

fn normalize_connector_origin_kind(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "curated" | "imported_local" | "imported_url" => Ok(value),
        _ => anyhow::bail!("origin_kind must be one of: curated, imported_local, imported_url"),
    }
}

fn validate_connector_status(status: &str) -> Result<()> {
    match status.trim() {
        CONNECTOR_STATUS_DRAFT
        | CONNECTOR_STATUS_CONVERTED
        | CONNECTOR_STATUS_UNDER_REVIEW
        | CONNECTOR_STATUS_ENABLED
        | CONNECTOR_STATUS_DISABLED
        | CONNECTOR_STATUS_ERROR => Ok(()),
        _ => anyhow::bail!("invalid connector status"),
    }
}

fn validate_connector_trust_state(trust_state: &str) -> Result<()> {
    match trust_state.trim() {
        CONNECTOR_TRUST_TRUSTED_CURATED
        | CONNECTOR_TRUST_LOCAL_UNTRUSTED
        | CONNECTOR_TRUST_REVIEWED_LOCAL
        | CONNECTOR_TRUST_BLOCKED => Ok(()),
        _ => anyhow::bail!("invalid connector trust_state"),
    }
}

fn validate_connector_conversion_status(status: &str) -> Result<()> {
    match status.trim() {
        CONNECTOR_CONVERSION_PENDING
        | CONNECTOR_CONVERSION_RUNNING
        | CONNECTOR_CONVERSION_SUCCEEDED
        | CONNECTOR_CONVERSION_FAILED => Ok(()),
        _ => anyhow::bail!("invalid connector conversion status"),
    }
}

fn validate_connector_interaction_status(status: &str) -> Result<()> {
    match status.trim() {
        CONNECTOR_INTERACTION_PENDING
        | CONNECTOR_INTERACTION_WAITING
        | CONNECTOR_INTERACTION_RESUMED
        | CONNECTOR_INTERACTION_CANCELLED
        | CONNECTOR_INTERACTION_EXPIRED => Ok(()),
        _ => anyhow::bail!("invalid connector interaction status"),
    }
}

fn validate_connector_write_classification(value: &str) -> Result<()> {
    match value.trim() {
        CONNECTOR_WRITE_READ_ONLY
        | CONNECTOR_WRITE_OPERATOR_GATED
        | CONNECTOR_WRITE_DESTRUCTIVE_GATED
        | CONNECTOR_WRITE_UNSAFE_BLOCKED => Ok(()),
        _ => anyhow::bail!("invalid connector write_classification"),
    }
}

fn validate_connector_assignment_auth_mode(value: &str) -> Result<()> {
    match value.trim() {
        "shared_default" | "agent_override" => Ok(()),
        _ => anyhow::bail!("invalid connector assignment auth_mode"),
    }
}

fn normalize_connector_external_reference_policy(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "inline_only" | "allowlisted_fetch" | "reject_external" => Ok(value),
        _ => anyhow::bail!(
            "external_reference_policy must be one of: inline_only, allowlisted_fetch, reject_external"
        ),
    }
}

fn normalize_connector_deprecation_state(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        CONNECTOR_DEPRECATION_ACTIVE
        | CONNECTOR_DEPRECATION_UNPUBLISHED
        | CONNECTOR_DEPRECATION_SUPERSEDED => Ok(value),
        _ => anyhow::bail!("invalid connector deprecation_state"),
    }
}

fn normalize_connector_auth_kind(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "none" | "bearer" | "header" | "query" | "oauth_session" => Ok(value),
        _ => anyhow::bail!("auth_kind must be one of: none, bearer, header, query, oauth_session"),
    }
}

fn normalize_connector_auth_status(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "ready" | "pending" | "error" | "expired" | "unconfigured" => Ok(value),
        _ => anyhow::bail!("invalid connector auth status"),
    }
}

fn normalize_connector_interaction_kind(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "oauth" | "auth_repair" | "operator_input" => Ok(value),
        _ => anyhow::bail!("interaction_kind must be one of: oauth, auth_repair, operator_input"),
    }
}

fn normalize_connector_json_payload(raw: &str, field_name: &str) -> Result<String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).with_context(|| format!("{field_name} must be valid JSON"))?;
    serde_json::to_string(&value)
        .with_context(|| format!("failed to normalize JSON field {field_name}"))
}

fn normalize_connector_prompt_summary(raw: &str) -> Result<String> {
    let value = raw.trim();
    if value.is_empty() {
        anyhow::bail!("prompt_summary cannot be empty");
    }
    Ok(value.to_string())
}

fn connector_matches_filter(record: &ConnectorSourceRecord, filter: &ConnectorListFilter) -> bool {
    if let Some(source_kind) = filter.source_kind.as_deref() {
        if record.source_kind != source_kind {
            return false;
        }
    }
    if let Some(status) = filter.status.as_deref() {
        if record.status != status {
            return false;
        }
    } else if !filter.include_disabled && record.status == CONNECTOR_STATUS_DISABLED {
        return false;
    }
    if let Some(trust_state) = filter.trust_state.as_deref() {
        if record.trust_state != trust_state {
            return false;
        }
    }
    record_matches_query(
        filter.query.as_deref(),
        &[
            &record.slug,
            &record.display_name,
            &record.source_kind,
            &record.origin_kind,
        ],
    )
}

fn validate_task_parent(
    conn: &Connection,
    project_id: &str,
    parent_task_id: Option<&str>,
    current_task_id: Option<&str>,
) -> Result<()> {
    let Some(parent_task_id) = parent_task_id else {
        return Ok(());
    };
    if Some(parent_task_id) == current_task_id {
        anyhow::bail!("task cannot parent itself");
    }
    let parent = get_task_with_conn(conn, parent_task_id)?.context("parent task does not exist")?;
    if parent.project_id != project_id {
        anyhow::bail!("parent task must belong to the same project");
    }
    let mut cursor = parent.parent_task_id.clone();
    while let Some(ancestor_id) = cursor {
        if Some(ancestor_id.as_str()) == current_task_id {
            anyhow::bail!("task hierarchy cycle detected");
        }
        cursor = get_task_with_conn(conn, &ancestor_id)?.and_then(|item| item.parent_task_id);
    }
    Ok(())
}

fn ensure_task_project_move_is_safe(
    conn: &Connection,
    task_id: &str,
    current_project_id: &str,
    next_project_id: &str,
) -> Result<()> {
    if current_project_id == next_project_id {
        return Ok(());
    }
    let child_count: i64 = conn.query_row(
        "SELECT COUNT(1) FROM tasks WHERE parent_task_id = ?1",
        params![task_id],
        |row| row.get(0),
    )?;
    if child_count > 0 {
        anyhow::bail!("task with subtasks cannot move to another project");
    }
    Ok(())
}

fn normalize_blocked_reason(
    status: &str,
    blocked_reason: Option<String>,
) -> Result<Option<String>> {
    if status.trim() == TASK_STATUS_BLOCKED {
        let normalized = blocked_reason
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .context("blocked_reason is required when task status is blocked")?;
        return Ok(Some(normalized));
    }
    Ok(None)
}

fn has_open_tasks_in_project(conn: &Connection, project_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(1)
        FROM tasks
        WHERE project_id = ?1
          AND status IN (?2, ?3, ?4)
        "#,
        params![
            project_id,
            TASK_STATUS_TODO,
            TASK_STATUS_IN_PROGRESS,
            TASK_STATUS_BLOCKED
        ],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn normalize_management_slug(raw: &str) -> Result<String> {
    let slug = raw.trim().to_ascii_lowercase();
    if slug.len() < 3 || slug.len() > 64 {
        anyhow::bail!("slug must be 3..64 characters");
    }
    if !slug
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        anyhow::bail!("slug must contain only a-z, 0-9, or '-'");
    }
    Ok(slug)
}

fn normalize_preset_key(raw: &str) -> Result<String> {
    let key = raw.trim().to_ascii_lowercase();
    if key.len() < 3 || key.len() > 64 {
        anyhow::bail!("preset_key must be 3..64 characters");
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
    {
        anyhow::bail!("preset_key must contain only a-z, 0-9, '-' or '_'");
    }
    Ok(key)
}

fn validate_bootstrap_preset_provider(
    provider_path: &str,
    default_model_provider: Option<&str>,
) -> Result<()> {
    let provider_path = provider_path.trim();
    match provider_path {
        "openai" => {
            if let Some(provider) = default_model_provider {
                if provider.trim() != "openai" {
                    anyhow::bail!("openai presets require default_model_provider=openai");
                }
            }
        }
        "anthropic" => {
            if let Some(provider) = default_model_provider {
                if provider.trim() != "anthropic" {
                    anyhow::bail!("anthropic presets require default_model_provider=anthropic");
                }
            }
        }
        "local" => {}
        _ => anyhow::bail!("provider_path must be one of: openai, anthropic, local"),
    }
    Ok(())
}

fn goal_matches_filter(record: &GoalRecord, filter: &GoalListFilter) -> bool {
    if let Some(status) = filter.status.as_deref() {
        if record.status != status {
            return false;
        }
    } else if record.status == GOAL_STATUS_ARCHIVED {
        return false;
    }
    if let Some(owner_agent_id) = filter.owner_agent_id.as_deref() {
        if record.owner_agent_id.as_deref() != Some(owner_agent_id) {
            return false;
        }
    }
    record_matches_query(
        filter.query.as_deref(),
        &[&record.slug, &record.title, &record.summary],
    )
}

fn project_matches_filter(record: &ProjectRecord, filter: &ProjectListFilter) -> bool {
    if let Some(goal_id) = filter.goal_id.as_deref() {
        if record.goal_id != goal_id {
            return false;
        }
    }
    if let Some(status) = filter.status.as_deref() {
        if record.status != status {
            return false;
        }
    } else if record.status == PROJECT_STATUS_ARCHIVED {
        return false;
    }
    if let Some(owner_agent_id) = filter.owner_agent_id.as_deref() {
        if record.owner_agent_id.as_deref() != Some(owner_agent_id) {
            return false;
        }
    }
    record_matches_query(
        filter.query.as_deref(),
        &[&record.slug, &record.name, &record.summary],
    )
}

fn task_matches_filter(
    record: &TaskRecord,
    filter: &TaskListFilter,
    project_goal_by_id: &std::collections::HashMap<String, String>,
    hierarchy_agent_ids: Option<&std::collections::HashSet<String>>,
) -> bool {
    if let Some(project_id) = filter.project_id.as_deref() {
        if record.project_id != project_id {
            return false;
        }
    }
    if let Some(goal_id) = filter.goal_id.as_deref() {
        if project_goal_by_id
            .get(&record.project_id)
            .map(String::as_str)
            != Some(goal_id)
        {
            return false;
        }
    }
    if let Some(status) = filter.status.as_deref() {
        if record.status != status {
            return false;
        }
    } else if record.status == TASK_STATUS_ARCHIVED {
        return false;
    }
    if let Some(owner_agent_id) = filter.owner_agent_id.as_deref() {
        if record.owner_agent_id.as_deref() != Some(owner_agent_id) {
            return false;
        }
    }
    if let Some(blocked) = filter.blocked {
        if blocked != (record.status == TASK_STATUS_BLOCKED) {
            return false;
        }
    }
    if let Some(unassigned) = filter.unassigned {
        if unassigned != record.owner_agent_id.is_none() {
            return false;
        }
    }
    if let Some(stale) = filter.stale {
        if stale != is_task_stale(record, filter.now_ms) {
            return false;
        }
    }
    if let Some(agent_ids) = hierarchy_agent_ids {
        if filter.hierarchy_scope.as_deref() == Some("subtree") {
            let Some(owner_agent_id) = record.owner_agent_id.as_ref() else {
                return false;
            };
            if !agent_ids.contains(owner_agent_id) {
                return false;
            }
        }
    }
    record_matches_query(filter.query.as_deref(), &[&record.title, &record.detail])
}

fn bootstrap_preset_matches_filter(
    record: &BootstrapPresetRecord,
    filter: &BootstrapPresetListFilter,
) -> bool {
    record_matches_query(
        filter.query.as_deref(),
        &[
            &record.preset_key,
            &record.display_name,
            &record.description,
            &record.role_label,
        ],
    )
}

fn record_matches_query(query: Option<&str>, fields: &[&str]) -> bool {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let query = query.to_ascii_lowercase();
    fields
        .iter()
        .any(|field| field.to_ascii_lowercase().contains(&query))
}

fn sort_records_by_updated<T, F>(items: &mut [T], sort: Option<&str>, key_fn: F) -> Result<()>
where
    F: Fn(&T) -> (i64, &str),
{
    match sort.unwrap_or("updated_at_desc") {
        "updated_at_desc" => items.sort_by(|left, right| {
            let left_key = key_fn(left);
            let right_key = key_fn(right);
            right_key
                .0
                .cmp(&left_key.0)
                .then_with(|| left_key.1.cmp(right_key.1))
        }),
        "updated_at_asc" => items.sort_by(|left, right| {
            let left_key = key_fn(left);
            let right_key = key_fn(right);
            left_key
                .0
                .cmp(&right_key.0)
                .then_with(|| left_key.1.cmp(right_key.1))
        }),
        other => anyhow::bail!("unsupported sort: {other}"),
    }
    Ok(())
}

fn page_records<T, F>(
    items: Vec<T>,
    limit: u32,
    cursor: Option<&str>,
    key_fn: F,
) -> Result<PageResult<T>>
where
    F: Fn(&T) -> (i64, &str),
{
    let limit = limit.clamp(1, 200) as usize;
    let cursor = decode_page_cursor(cursor)?;
    let start = if let Some((updated_at, record_id)) = cursor {
        items
            .iter()
            .position(|item| {
                let key = key_fn(item);
                key.0 == updated_at && key.1 == record_id
            })
            .map(|index| index + 1)
            .context("cursor does not match an existing record")?
    } else {
        0
    };
    let mut window = items
        .into_iter()
        .skip(start)
        .take(limit + 1)
        .collect::<Vec<_>>();
    let next_cursor = if window.len() > limit {
        window.pop();
        window.last().map(|item| {
            let key = key_fn(item);
            encode_page_cursor(key.0, key.1)
        })
    } else {
        None
    };
    Ok(PageResult {
        items: window.into_iter().take(limit).collect(),
        next_cursor,
    })
}

fn decode_page_cursor(cursor: Option<&str>) -> Result<Option<(i64, String)>> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let (updated_at, record_id) = cursor
        .split_once(':')
        .context("cursor must be formatted as '<updated_at>:<record_id>'")?;
    let updated_at = updated_at
        .parse::<i64>()
        .context("cursor updated_at must be an integer")?;
    Ok(Some((updated_at, record_id.to_string())))
}

fn encode_page_cursor(updated_at: i64, record_id: &str) -> String {
    format!("{updated_at}:{record_id}")
}

fn is_task_stale(record: &TaskRecord, now_ms: i64) -> bool {
    !matches!(
        record.status.as_str(),
        TASK_STATUS_DONE | TASK_STATUS_ARCHIVED
    ) && now_ms.saturating_sub(record.updated_at) > STRATEGY_STALE_THRESHOLD_MS
}

fn agent_subtree_ids(
    conn: &Connection,
    root_agent_id: &str,
) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          agent_id, name, workspace_root, model_provider, model_id, tool_profile,
          reports_to_agent_id, role_label,
          memory_binding_id, memory_provider_kind, memory_base_url, memory_auth_mode,
          memory_auth_secret_ref, memory_principal_id, memory_principal_display_name,
          memory_enabled, memory_trusted_local_operator_actions,
          created_at, updated_at
        FROM agents
        "#,
    )?;
    let rows = stmt.query_map([], map_agent_row)?;
    let mut children_by_parent: std::collections::HashMap<Option<String>, Vec<String>> =
        std::collections::HashMap::new();
    let mut exists = false;
    for row in rows {
        let record = row?;
        if record.agent_id == root_agent_id {
            exists = true;
        }
        children_by_parent
            .entry(record.reports_to_agent_id.clone())
            .or_default()
            .push(record.agent_id);
    }
    if !exists {
        anyhow::bail!("hierarchy_root_agent_id does not exist");
    }
    let mut ids = std::collections::HashSet::new();
    let mut stack = vec![root_agent_id.to_string()];
    while let Some(agent_id) = stack.pop() {
        if !ids.insert(agent_id.clone()) {
            continue;
        }
        if let Some(children) = children_by_parent.get(&Some(agent_id)) {
            stack.extend(children.iter().cloned());
        }
    }
    Ok(ids)
}

fn find_task_id_by_link_target(
    conn: &Connection,
    column_name: &str,
    target_id: &str,
) -> Result<Option<String>> {
    let query = match column_name {
        "linked_board_card_id" => {
            "SELECT task_id FROM tasks WHERE linked_board_card_id = ?1 LIMIT 1"
        }
        "linked_job_id" => "SELECT task_id FROM tasks WHERE linked_job_id = ?1 LIMIT 1",
        _ => anyhow::bail!("unsupported task link lookup column"),
    };
    Ok(conn
        .query_row(query, params![target_id], |row| row.get::<_, String>(0))
        .optional()?)
}

fn resolve_board_runtime_candidate(
    conn: &Connection,
    card_id: &str,
) -> Result<Option<RuntimeCandidate>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          card_id, board_id, column_id, title, description, owner_kind, owner_agent_id, owner_human_id,
          due_at, tags_json, script_markdown, linked_session_id, latest_run_id, position, created_at, updated_at, archived_at
        FROM board_cards
        WHERE card_id = ?1
          AND archived_at IS NULL
        "#,
    )?;
    let card = stmt
        .query_row(params![card_id], map_board_card_row)
        .optional()?;
    let Some(card) = card else {
        return Ok(None);
    };
    let run = match card.latest_run_id.as_deref() {
        Some(run_id) => get_run_with_conn(conn, run_id)?,
        None => None,
    };
    Ok(Some(RuntimeCandidate {
        latest_run_id: card.latest_run_id,
        latest_session_id: card.linked_session_id,
        sort_ms: run
            .as_ref()
            .map(|item| item.started_at.unwrap_or(item.created_at))
            .unwrap_or(0),
    }))
}

fn resolve_job_runtime_candidate(
    conn: &Connection,
    job_id: &str,
) -> Result<Option<RuntimeCandidate>> {
    let Some(job) = get_job_with_conn(conn, job_id)? else {
        return Ok(None);
    };
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
    let Some(session_id) = payload
        .get("session_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let latest_run = latest_run_for_session_with_conn(conn, &session_id)?;
    Ok(Some(RuntimeCandidate {
        latest_run_id: latest_run.as_ref().map(|item| item.run_id.clone()),
        latest_session_id: Some(session_id),
        sort_ms: latest_run
            .as_ref()
            .map(|item| item.started_at.unwrap_or(item.created_at))
            .unwrap_or(0),
    }))
}

fn select_runtime_link(
    board_candidate: Option<RuntimeCandidate>,
    job_candidate: Option<RuntimeCandidate>,
) -> TaskRuntimeLinkRecord {
    let selected = match (board_candidate, job_candidate) {
        (Some(left), Some(right)) => {
            if right.sort_ms > left.sort_ms {
                right
            } else {
                left
            }
        }
        (Some(candidate), None) | (None, Some(candidate)) => candidate,
        (None, None) => RuntimeCandidate {
            latest_run_id: None,
            latest_session_id: None,
            sort_ms: 0,
        },
    };
    TaskRuntimeLinkRecord {
        latest_run_id: selected.latest_run_id,
        latest_session_id: selected.latest_session_id,
    }
}

fn get_run_with_conn(conn: &Connection, run_id: &str) -> Result<Option<RunRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
          run_id, session_id, status, model_provider, model_id, started_at, ended_at, error_text,
          usage_json, created_at
        FROM runs
        WHERE run_id = ?1
        "#,
    )?;
    Ok(stmt.query_row(params![run_id], map_run_row).optional()?)
}

fn map_agent_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    let binding_id: Option<String> = row.get(8)?;
    let provider_kind: Option<String> = row.get(9)?;
    let base_url: Option<String> = row.get(10)?;
    let auth_mode: Option<String> = row.get(11)?;
    let auth_secret_ref: Option<String> = row.get(12)?;
    let principal_id: Option<String> = row.get(13)?;
    let principal_display_name: Option<String> = row.get(14)?;
    let enabled: i64 = row.get(15)?;
    let trusted_local_operator_actions: i64 = row.get(16)?;
    let memory_binding = match (binding_id, provider_kind, base_url, auth_mode) {
        (Some(binding_id), Some(provider_kind), Some(base_url), Some(auth_mode)) => {
            Some(AgentMemoryBindingRecord {
                binding_id,
                provider_kind,
                base_url,
                auth_mode,
                auth_secret_ref,
                principal_id,
                principal_display_name,
                enabled: enabled != 0,
                trusted_local_operator_actions: trusted_local_operator_actions != 0,
            })
        }
        _ => None,
    };
    Ok(AgentRecord {
        agent_id: row.get(0)?,
        name: row.get(1)?,
        workspace_root: row.get(2)?,
        model_provider: row.get(3)?,
        model_id: row.get(4)?,
        tool_profile: row.get(5)?,
        reports_to_agent_id: row.get(6)?,
        role_label: row.get(7)?,
        memory_binding,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn map_goal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GoalRecord> {
    Ok(GoalRecord {
        goal_id: row.get(0)?,
        slug: row.get(1)?,
        title: row.get(2)?,
        summary: row.get(3)?,
        status: row.get(4)?,
        owner_agent_id: row.get(5)?,
        target_date: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRecord> {
    Ok(ProjectRecord {
        project_id: row.get(0)?,
        goal_id: row.get(1)?,
        slug: row.get(2)?,
        name: row.get(3)?,
        summary: row.get(4)?,
        status: row.get(5)?,
        owner_agent_id: row.get(6)?,
        workspace_root: row.get(7)?,
        budget_month_usd: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        task_id: row.get(0)?,
        project_id: row.get(1)?,
        parent_task_id: row.get(2)?,
        title: row.get(3)?,
        detail: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        owner_agent_id: row.get(7)?,
        due_at: row.get(8)?,
        blocked_reason: row.get(9)?,
        linked_board_card_id: row.get(10)?,
        linked_job_id: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_bootstrap_preset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BootstrapPresetRecord> {
    Ok(BootstrapPresetRecord {
        preset_key: row.get(0)?,
        display_name: row.get(1)?,
        description: row.get(2)?,
        role_label: row.get(3)?,
        provider_path: row.get(4)?,
        default_model_provider: row.get(5)?,
        default_model_id: row.get(6)?,
        default_tool_profile: row.get(7)?,
        default_workspace_root: row.get(8)?,
        default_reports_to_agent_id: row.get(9)?,
        setup_notes: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn map_connector_source_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectorSourceRecord> {
    let assigned_agent_count: i64 = row.get(10)?;
    let published_tool_count: i64 = row.get(11)?;
    Ok(ConnectorSourceRecord {
        connector_id: row.get(0)?,
        slug: row.get(1)?,
        display_name: row.get(2)?,
        source_kind: row.get(3)?,
        origin_kind: row.get(4)?,
        catalog_item_id: row.get(5)?,
        current_version_id: row.get(6)?,
        latest_imported_version_id: row.get(7)?,
        status: row.get(8)?,
        trust_state: row.get(9)?,
        assigned_agent_count: assigned_agent_count.max(0) as usize,
        published_tool_count: published_tool_count.max(0) as usize,
        last_conversion_at: row.get(12)?,
        last_review_at: row.get(13)?,
        last_enabled_at: row.get(14)?,
        last_disabled_at: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn map_connector_version_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectorVersionRecord> {
    Ok(ConnectorVersionRecord {
        version_id: row.get(0)?,
        connector_id: row.get(1)?,
        version_label: row.get(2)?,
        source_digest: row.get(3)?,
        raw_source_location: row.get(4)?,
        import_metadata_json: row.get(5)?,
        schema_summary_json: row.get(6)?,
        latest_conversion_id: row.get(7)?,
        external_reference_policy: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_connector_conversion_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConnectorConversionRecord> {
    let write_capable_tools: i64 = row.get(6)?;
    Ok(ConnectorConversionRecord {
        conversion_id: row.get(0)?,
        connector_id: row.get(1)?,
        version_id: row.get(2)?,
        status: row.get(3)?,
        warnings_json: row.get(4)?,
        proposed_tools_json: row.get(5)?,
        write_capable_tools: write_capable_tools.max(0) as usize,
        unsupported_operations_json: row.get(7)?,
        normalization_notes_json: row.get(8)?,
        diff_from_previous_json: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn map_connector_published_tool_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConnectorPublishedToolRecord> {
    Ok(ConnectorPublishedToolRecord {
        published_tool_id: row.get(0)?,
        connector_id: row.get(1)?,
        version_id: row.get(2)?,
        conversion_id: row.get(3)?,
        tool_name: row.get(4)?,
        display_name: row.get(5)?,
        tool_schema_json: row.get(6)?,
        origin_metadata_json: row.get(7)?,
        write_classification: row.get(8)?,
        published_at: row.get(9)?,
        unpublished_at: row.get(10)?,
        superseded_by_published_tool_id: row.get(11)?,
        deprecation_state: row.get(12)?,
    })
}

fn map_connector_assignment_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConnectorAssignmentRecord> {
    let enabled: i64 = row.get(3)?;
    Ok(ConnectorAssignmentRecord {
        assignment_id: row.get(0)?,
        connector_id: row.get(1)?,
        agent_id: row.get(2)?,
        enabled: enabled != 0,
        auth_mode: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_connector_auth_binding_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConnectorAuthBindingRecord> {
    Ok(ConnectorAuthBindingRecord {
        auth_binding_id: row.get(0)?,
        connector_id: row.get(1)?,
        agent_id: row.get(2)?,
        auth_kind: row.get(3)?,
        secret_ref: row.get(4)?,
        oauth_session_id: row.get(5)?,
        status: row.get(6)?,
        auth_metadata_json: row.get(7)?,
        last_success_at: row.get(8)?,
        last_error: row.get(9)?,
        last_rotated_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn map_connector_interaction_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConnectorInteractionRecord> {
    Ok(ConnectorInteractionRecord {
        interaction_id: row.get(0)?,
        connector_id: row.get(1)?,
        agent_id: row.get(2)?,
        interaction_kind: row.get(3)?,
        status: row.get(4)?,
        prompt_summary: row.get(5)?,
        resume_token: row.get(6)?,
        expires_at: row.get(7)?,
        consumed_at: row.get(8)?,
        detail_json: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn map_board_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BoardRecord> {
    Ok(BoardRecord {
        board_id: row.get(0)?,
        board_key: row.get(1)?,
        name: row.get(2)?,
        board_type: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        archived_at: row.get(6)?,
    })
}

fn map_board_column_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BoardColumnRecord> {
    Ok(BoardColumnRecord {
        column_id: row.get(0)?,
        board_id: row.get(1)?,
        column_key: row.get(2)?,
        name: row.get(3)?,
        position: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        archived_at: row.get(7)?,
    })
}

fn map_board_card_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BoardCardRecord> {
    Ok(BoardCardRecord {
        card_id: row.get(0)?,
        board_id: row.get(1)?,
        column_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        owner_kind: row.get(5)?,
        owner_agent_id: row.get(6)?,
        owner_human_id: row.get(7)?,
        due_at: row.get(8)?,
        tags_json: row.get(9)?,
        script_markdown: row.get(10)?,
        linked_session_id: row.get(11)?,
        latest_run_id: row.get(12)?,
        position: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        archived_at: row.get(16)?,
    })
}

fn map_board_card_asset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BoardCardAssetRecord> {
    Ok(BoardCardAssetRecord {
        card_asset_id: row.get(0)?,
        card_id: row.get(1)?,
        filename: row.get(2)?,
        mime: row.get(3)?,
        sha256: row.get(4)?,
        bytes: row.get(5)?,
        local_path: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_agent_mail_thread_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentMailThreadRecord> {
    Ok(AgentMailThreadRecord {
        thread_id: row.get(0)?,
        kind: row.get(1)?,
        subject: row.get(2)?,
        created_by_principal: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        archived_at: row.get(6)?,
    })
}

fn map_agent_mail_participant_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AgentMailThreadParticipantRecord> {
    let muted: i64 = row.get(5)?;
    Ok(AgentMailThreadParticipantRecord {
        thread_id: row.get(0)?,
        principal_id: row.get(1)?,
        role: row.get(2)?,
        joined_at: row.get(3)?,
        last_read_at: row.get(4)?,
        muted: muted != 0,
    })
}

fn map_agent_mail_message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentMailMessageRecord> {
    Ok(AgentMailMessageRecord {
        message_id: row.get(0)?,
        thread_id: row.get(1)?,
        sender_principal: row.get(2)?,
        sender_kind: row.get(3)?,
        body_text: row.get(4)?,
        metadata_json: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn map_agent_mail_message_recipient_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AgentMailMessageRecipientRecord> {
    Ok(AgentMailMessageRecipientRecord {
        message_id: row.get(0)?,
        recipient_principal: row.get(1)?,
        delivered_at: row.get(2)?,
        acked_at: row.get(3)?,
    })
}

fn map_agent_mail_attachment_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AgentMailAttachmentRecord> {
    Ok(AgentMailAttachmentRecord {
        attachment_id: row.get(0)?,
        message_id: row.get(1)?,
        filename: row.get(2)?,
        mime: row.get(3)?,
        sha256: row.get(4)?,
        bytes: row.get(5)?,
        local_path: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_agent_mail_file_lease_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AgentMailFileLeaseRecord> {
    let exclusive: i64 = row.get(3)?;
    Ok(AgentMailFileLeaseRecord {
        lease_id: row.get(0)?,
        holder_principal: row.get(1)?,
        glob_pattern: row.get(2)?,
        exclusive: exclusive != 0,
        ttl_ms: row.get(4)?,
        note: row.get(5)?,
        created_at: row.get(6)?,
        expires_at: row.get(7)?,
        released_at: row.get(8)?,
    })
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

fn map_assistant_worker_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssistantWorkerRecord> {
    Ok(AssistantWorkerRecord {
        boss_key: row.get(0)?,
        root_session_id: row.get(1)?,
        worker_key: row.get(2)?,
        worker_kind: row.get(3)?,
        status: row.get(4)?,
        agent_id: row.get(5)?,
        session_id: row.get(6)?,
        template_key: row.get(7)?,
        display_name: row.get(8)?,
        instructions: row.get(9)?,
        run_defaults_json: row.get(10)?,
        session_mode: row.get(11)?,
        last_run_id: row.get(12)?,
        last_run_status: row.get(13)?,
        last_stop_reason: row.get(14)?,
        pending_approval_id: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        archived_at: row.get(18)?,
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
const MIGRATION_0002: &str = include_str!("../../../migrations/0002_strategy_phase1.sql");
const MIGRATION_0003: &str = include_str!("../../../migrations/0003_strategy_schema_cleanup.sql");
const MIGRATION_0004: &str = include_str!("../../../migrations/0004_agent_memory_bindings.sql");
const MIGRATION_0005: &str = include_str!("../../../migrations/0005_connector_registry.sql");
const MIGRATION_0006: &str = include_str!("../../../migrations/0006_agent_archival.sql");
const MIGRATION_0007: &str = include_str!("../../../migrations/0007_execass_replacement.sql");
const MIGRATION_0008: &str = include_str!("../../../migrations/0008_glass_office_projection.sql");

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn app_paths_debug_never_exposes_configured_paths() {
        let canary = format!("state-path-secret-debug-{}", std::process::id());
        let paths = AppPaths::from_root(PathBuf::from(format!("Z:\\{canary}\\state")));
        let output = format!("{paths:?}");

        assert!(output.contains("root_configured: true"));
        assert!(output.contains("database_configured: true"));
        assert!(output.contains("attachments_configured: true"));
        assert!(output.contains("logs_configured: true"));
        assert!(!output.contains(&canary));
        assert!(!output.contains("carsinos.db"));
    }

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
    fn previous_run_for_session_uses_rowid_tiebreaker_for_timestamp_collisions() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("timestamp collision".to_string()),
            })
            .expect("create session");
        let first = storage
            .create_run(NewRun {
                session_id: session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock".to_string(),
            })
            .expect("create first run")
            .expect("first run exists");
        let second = storage
            .create_run(NewRun {
                session_id: session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock".to_string(),
            })
            .expect("create second run")
            .expect("second run exists");
        let created_at = 42_000;
        let conn = storage.connect().expect("connect");
        conn.execute(
            "UPDATE runs SET created_at = ?1 WHERE run_id IN (?2, ?3)",
            params![created_at, first.run_id, second.run_id],
        )
        .expect("force timestamp collision");

        let previous = storage
            .previous_run_for_session(&session.session_id, created_at)
            .expect("previous lookup")
            .expect("previous run");
        assert_eq!(previous.run_id, first.run_id);
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
    fn get_session_by_key_returns_created_session() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: Some("telegram:dm:42".to_string()),
                agent_id: "default".to_string(),
                title: Some("channel-session".to_string()),
            })
            .expect("create session");

        let by_key = storage
            .get_session_by_key("telegram:dm:42")
            .expect("lookup by key")
            .expect("session exists");
        assert_eq!(by_key.session_id, session.session_id);
        assert_eq!(by_key.session_key, "telegram:dm:42");
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

        let replaced = storage
            .replace_auth_profile(
                &profile_b.auth_profile_id,
                NewAuthProfile {
                    provider: "anthropic".to_string(),
                    display_name: "claude-primary".to_string(),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    enabled: true,
                    kill_switch_scope: "none".to_string(),
                    api_base_url: Some("https://api.anthropic.com".to_string()),
                    credentials_json: r#"{"secret_ref":"auth.anthropic.setup-token.test","token_kind":"setup_token"}"#.to_string(),
                },
            )
            .expect("replace auth profile")
            .expect("profile exists");
        assert_eq!(replaced.provider, "anthropic");
        assert_eq!(replaced.display_name, "claude-primary");
        assert_eq!(replaced.auth_mode, "api_key");
        assert_eq!(replaced.risk_level, "low");
        assert_eq!(replaced.kill_switch_scope, "none");
        assert!(replaced.enabled);
    }

    #[test]
    fn auth_profile_kill_switch_queries_work() {
        let (_temp_dir, storage) = test_storage();

        let _ = storage
            .create_auth_profile(NewAuthProfile {
                provider: "anthropic".to_string(),
                display_name: "anthropic-test".to_string(),
                auth_mode: "agent_sdk".to_string(),
                risk_level: "high".to_string(),
                enabled: true,
                kill_switch_scope: "provider".to_string(),
                api_base_url: Some("https://api.anthropic.com".to_string()),
                credentials_json:
                    r#"{"headless_enabled":true,"headless_command":"claude","headless_args":["-p"]}"#
                        .to_string(),
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
    fn daily_auth_profile_usage_upsert_increments_totals() {
        let (_temp_dir, storage) = test_storage();
        let profile = storage
            .create_auth_profile(NewAuthProfile {
                provider: "openai".to_string(),
                display_name: "usage-profile".to_string(),
                auth_mode: "api_key".to_string(),
                risk_level: "low".to_string(),
                enabled: true,
                kill_switch_scope: "none".to_string(),
                api_base_url: Some("https://api.openai.com".to_string()),
                credentials_json: r#"{"api_key":"redacted"}"#.to_string(),
            })
            .expect("create usage profile");

        let first = storage
            .increment_daily_auth_profile_usage(DailyAuthProfileUsageIncrement {
                usage_day_utc: "2026-02-20".to_string(),
                auth_profile_id: profile.auth_profile_id.clone(),
                provider: "openai".to_string(),
                input_chars: 120,
                output_chars: 80,
                input_tokens: 30,
                output_tokens: 20,
                total_tokens: 50,
                estimated_cost_usd: 0.015,
            })
            .expect("increment first usage");
        assert_eq!(first.total_tokens, 50);
        assert_eq!(first.estimated_cost_usd, 0.015);

        let second = storage
            .increment_daily_auth_profile_usage(DailyAuthProfileUsageIncrement {
                usage_day_utc: "2026-02-20".to_string(),
                auth_profile_id: profile.auth_profile_id.clone(),
                provider: "openai".to_string(),
                input_chars: 60,
                output_chars: 40,
                input_tokens: 15,
                output_tokens: 10,
                total_tokens: 25,
                estimated_cost_usd: 0.0075,
            })
            .expect("increment second usage");
        assert_eq!(second.input_chars, 180);
        assert_eq!(second.output_chars, 120);
        assert_eq!(second.input_tokens, 45);
        assert_eq!(second.output_tokens, 30);
        assert_eq!(second.total_tokens, 75);
        assert!((second.estimated_cost_usd - 0.0225).abs() < 1e-9);

        let loaded = storage
            .get_daily_auth_profile_usage("2026-02-20", &profile.auth_profile_id)
            .expect("load daily usage")
            .expect("daily usage exists");
        assert_eq!(loaded.total_tokens, 75);
        assert!((loaded.estimated_cost_usd - 0.0225).abs() < 1e-9);
    }

    #[test]
    fn list_run_usage_samples_between_returns_joined_agent_metadata() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("usage sample".to_string()),
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
        storage
            .set_run_usage_json(
                &run.run_id,
                r#"{"provider":{"input_tokens":12,"output_tokens":7,"estimated_cost_usd":0.031}}"#,
            )
            .expect("set run usage");

        let start_ms = run.created_at.saturating_sub(60_000);
        let end_ms = run.created_at.saturating_add(60_000);
        let rows = storage
            .list_run_usage_samples_between(start_ms, end_ms, 100)
            .expect("list usage samples");
        assert_eq!(rows.len(), 1);
        let sample = &rows[0];
        assert_eq!(sample.run_id, run.run_id);
        assert_eq!(sample.session_id, session.session_id);
        assert_eq!(sample.agent_id, "default");
        assert_eq!(sample.agent_name, "Default Agent");
        assert_eq!(sample.model_provider, "mock");
        assert_eq!(sample.model_id, "mock-echo-v1");
        assert!(sample.usage_json.contains("\"input_tokens\":12"));
        assert!(sample.sample_ts_ms >= start_ms);
        assert!(sample.sample_ts_ms < end_ms);
    }

    #[test]
    fn list_run_usage_samples_between_clamps_zero_limit_to_one() {
        let (_temp_dir, storage) = test_storage();
        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "default".to_string(),
                title: Some("usage sample clamp".to_string()),
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
        storage
            .set_run_usage_json(
                &run.run_id,
                r#"{"provider":{"input_tokens":4,"output_tokens":2,"estimated_cost_usd":0.01}}"#,
            )
            .expect("set run usage");

        let start_ms = run.created_at.saturating_sub(60_000);
        let end_ms = run.created_at.saturating_add(60_000);
        let rows = storage
            .list_run_usage_samples_between(start_ms, end_ms, 0)
            .expect("list usage samples with zero limit");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].run_id, run.run_id);
    }

    #[test]
    fn circuit_breaker_state_upsert_round_trip_and_clear() {
        let (_temp_dir, storage) = test_storage();

        let first = storage
            .upsert_circuit_breaker_state(CircuitBreakerStateUpsert {
                scope: "provider".to_string(),
                target_id: "openai".to_string(),
                state: "closed".to_string(),
                consecutive_failures: 1,
                opened_at: None,
                cooldown_until: None,
                last_error_code: Some("TIMEOUT".to_string()),
            })
            .expect("upsert first breaker state");
        assert_eq!(first.scope, "provider");
        assert_eq!(first.target_id, "openai");
        assert_eq!(first.state, "closed");
        assert_eq!(first.consecutive_failures, 1);
        assert_eq!(first.last_error_code.as_deref(), Some("TIMEOUT"));

        let second = storage
            .upsert_circuit_breaker_state(CircuitBreakerStateUpsert {
                scope: "provider".to_string(),
                target_id: "openai".to_string(),
                state: "open".to_string(),
                consecutive_failures: 3,
                opened_at: Some(1_000),
                cooldown_until: Some(2_000),
                last_error_code: Some("RATE_LIMITED".to_string()),
            })
            .expect("upsert second breaker state");
        assert_eq!(second.state, "open");
        assert_eq!(second.consecutive_failures, 3);
        assert_eq!(second.opened_at, Some(1_000));
        assert_eq!(second.cooldown_until, Some(2_000));
        assert_eq!(second.last_error_code.as_deref(), Some("RATE_LIMITED"));

        let loaded = storage
            .get_circuit_breaker_state("provider", "openai")
            .expect("get breaker state")
            .expect("breaker state exists");
        assert_eq!(loaded.state, "open");
        assert_eq!(loaded.consecutive_failures, 3);

        let listed = storage
            .list_circuit_breaker_states(10, None)
            .expect("list breaker states");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].scope, "provider");
        assert_eq!(listed[0].target_id, "openai");
        let listed_provider = storage
            .list_circuit_breaker_states(10, Some("provider"))
            .expect("list provider breaker states");
        assert_eq!(listed_provider.len(), 1);
        let listed_job = storage
            .list_circuit_breaker_states(10, Some("job"))
            .expect("list job breaker states");
        assert!(listed_job.is_empty());

        storage
            .clear_circuit_breaker_state("provider", "openai")
            .expect("clear breaker");
        assert!(
            storage
                .get_circuit_breaker_state("provider", "openai")
                .expect("get breaker after clear")
                .is_none(),
            "breaker should be cleared"
        );
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
    fn security_audit_retention_archive_and_delete_work() {
        let (_temp_dir, storage) = test_storage();
        let old = storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "req-old".to_string(),
                correlation_id: "corr-old".to_string(),
                principal: "operator_admin:test".to_string(),
                action: "security.test.old".to_string(),
                resource: "test:old".to_string(),
                decision: "allow".to_string(),
                reason: None,
                transport: "http".to_string(),
                status: "200".to_string(),
                error_code: None,
                session_id: None,
                run_id: None,
                metadata_json: None,
            })
            .expect("append old audit event");
        let fresh = storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "req-fresh".to_string(),
                correlation_id: "corr-fresh".to_string(),
                principal: "operator_admin:test".to_string(),
                action: "security.test.fresh".to_string(),
                resource: "test:fresh".to_string(),
                decision: "allow".to_string(),
                reason: None,
                transport: "http".to_string(),
                status: "200".to_string(),
                error_code: None,
                session_id: None,
                run_id: None,
                metadata_json: None,
            })
            .expect("append fresh audit event");

        let cutoff_ms = now_ms().saturating_sub(60_000);
        let conn = storage.connect().expect("connect for test update");
        conn.execute(
            "UPDATE security_audit_events SET created_at = ?1 WHERE event_id = ?2",
            params![cutoff_ms.saturating_sub(1), old.event_id],
        )
        .expect("mark old audit event created_at");

        let candidate_count = storage
            .count_security_audit_events_before(cutoff_ms)
            .expect("count retention candidates");
        assert_eq!(candidate_count, 1);

        let archived_count = storage
            .archive_security_audit_events_before(cutoff_ms)
            .expect("archive retention candidates");
        assert_eq!(archived_count, 1);

        let deleted_count = storage
            .delete_security_audit_events_before(cutoff_ms)
            .expect("delete retention candidates");
        assert_eq!(deleted_count, 1);

        assert!(storage
            .get_security_audit_event(&old.event_id)
            .expect("load old after delete")
            .is_none());
        assert!(storage
            .get_archived_security_audit_event(&old.event_id)
            .expect("load old in archive")
            .is_some());
        assert!(storage
            .get_security_audit_event(&fresh.event_id)
            .expect("load fresh after retention")
            .is_some());
    }

    #[test]
    fn security_audit_retention_respects_ninety_day_hot_window() {
        let (_temp_dir, storage) = test_storage();
        let old = storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "req-old-90d".to_string(),
                correlation_id: "corr-old-90d".to_string(),
                principal: "operator_admin:test".to_string(),
                action: "security.test.old.90d".to_string(),
                resource: "test:old:90d".to_string(),
                decision: "allow".to_string(),
                reason: None,
                transport: "http".to_string(),
                status: "200".to_string(),
                error_code: None,
                session_id: None,
                run_id: None,
                metadata_json: None,
            })
            .expect("append old 90d audit event");
        let recent = storage
            .append_security_audit_event(NewSecurityAuditEvent {
                request_id: "req-recent-90d".to_string(),
                correlation_id: "corr-recent-90d".to_string(),
                principal: "operator_admin:test".to_string(),
                action: "security.test.recent.90d".to_string(),
                resource: "test:recent:90d".to_string(),
                decision: "allow".to_string(),
                reason: None,
                transport: "http".to_string(),
                status: "200".to_string(),
                error_code: None,
                session_id: None,
                run_id: None,
                metadata_json: None,
            })
            .expect("append recent 90d audit event");

        let now = now_ms();
        let day_ms = 86_400_000_i64;
        let cutoff_ms = now.saturating_sub(90 * day_ms);
        let old_created_at = now.saturating_sub(91 * day_ms);
        let recent_created_at = now.saturating_sub(89 * day_ms);

        let conn = storage.connect().expect("connect for 90d retention update");
        conn.execute(
            "UPDATE security_audit_events SET created_at = ?1 WHERE event_id = ?2",
            params![old_created_at, old.event_id],
        )
        .expect("set old 90d created_at");
        conn.execute(
            "UPDATE security_audit_events SET created_at = ?1 WHERE event_id = ?2",
            params![recent_created_at, recent.event_id],
        )
        .expect("set recent 90d created_at");

        let candidate_count = storage
            .count_security_audit_events_before(cutoff_ms)
            .expect("count 90d retention candidates");
        assert_eq!(candidate_count, 1);

        let archived_count = storage
            .archive_security_audit_events_before(cutoff_ms)
            .expect("archive 90d retention candidates");
        assert_eq!(archived_count, 1);

        let deleted_count = storage
            .delete_security_audit_events_before(cutoff_ms)
            .expect("delete 90d retention candidates");
        assert_eq!(deleted_count, 1);

        assert!(storage
            .get_security_audit_event(&old.event_id)
            .expect("load old 90d after delete")
            .is_none());
        assert!(storage
            .get_archived_security_audit_event(&old.event_id)
            .expect("load old 90d in archive")
            .is_some());
        assert!(storage
            .get_security_audit_event(&recent.event_id)
            .expect("load recent 90d after retention")
            .is_some());
        assert!(storage
            .get_archived_security_audit_event(&recent.event_id)
            .expect("load recent 90d in archive")
            .is_none());
    }

    #[test]
    fn init_upgrades_legacy_db_to_current_schema() {
        let temp_dir = TempDir::new().expect("tempdir");
        let paths = AppPaths::from_root(temp_dir.path().to_path_buf());
        std::fs::create_dir_all(&paths.root).expect("create root");
        let conn = open_sqlite_connection(&paths.db_path).expect("open legacy db");
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
        let conn = open_sqlite_connection(&paths.db_path).expect("open upgraded db");
        for table in [
            "auth_profiles",
            "notes",
            "embeddings",
            "jobs",
            "job_runs",
            "security_audit_events",
            "security_audit_events_archive",
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
        assert!(
            column_exists(&conn, "agents", "reports_to_agent_id").expect("query reports_to column"),
            "expected migrated column: agents.reports_to_agent_id"
        );
        assert!(
            column_exists(&conn, "agents", "role_label").expect("query role_label column"),
            "expected migrated column: agents.role_label"
        );
        assert!(
            !bootstrap_preset_manager_has_agent_fk(&conn)
                .expect("query bootstrap preset manager foreign key"),
            "bootstrap preset manager column should not enforce a local agent foreign key"
        );
        let foreign_keys_enabled = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .expect("query foreign_keys pragma");
        assert_eq!(
            foreign_keys_enabled, 1,
            "sqlite foreign keys should be enabled"
        );
    }

    #[test]
    fn init_refreshes_default_agent_workspace_to_launch_directory() {
        let temp_dir = TempDir::new().expect("tempdir");
        let paths = AppPaths::from_root(temp_dir.path().to_path_buf());
        init(&paths).expect("initial init");
        let storage = Storage::from_paths(&paths);
        let launch_workspace = std::env::current_dir()
            .expect("current dir")
            .display()
            .to_string();

        storage
            .update_agent(
                "default",
                AgentUpdatePatch {
                    name: Some("G4".to_string()),
                    workspace_root: Some("/Users/example/old/carsinos".to_string()),
                    model_provider: Some("lmstudio".to_string()),
                    model_id: Some("local-model".to_string()),
                    tool_profile: None,
                    reports_to_agent_id: None,
                    role_label: None,
                    memory_binding: None,
                },
            )
            .expect("seed stale default agent");

        init(&paths).expect("re-init should refresh default workspace");
        let default_agent = storage
            .get_agent("default")
            .expect("load default agent")
            .expect("default agent exists");

        assert_eq!(default_agent.workspace_root, launch_workspace);
        assert_eq!(default_agent.name, "G4");
        assert_eq!(default_agent.model_provider, "lmstudio");
        assert_eq!(default_agent.model_id, "local-model");
    }

    #[test]
    fn storage_connections_enable_foreign_keys() {
        let (_temp_dir, storage) = test_storage();
        let conn = storage
            .connect()
            .expect("open configured storage connection");
        let foreign_keys_enabled = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .expect("query foreign_keys pragma");
        assert_eq!(
            foreign_keys_enabled, 1,
            "sqlite foreign keys should be enabled"
        );
        let busy_timeout_ms = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
            .expect("query busy_timeout pragma");
        assert_eq!(
            busy_timeout_ms, 5_000,
            "sqlite busy timeout should wait for transient writer locks"
        );
    }

    fn project_local_tempdir() -> TempDir {
        TempDir::new_in(std::env::current_dir().expect("project working directory"))
            .expect("project-local tempdir")
    }

    fn execass_test_root() -> (TempDir, AppPaths) {
        let temp_dir = project_local_tempdir();
        let paths = AppPaths::from_root(temp_dir.path().join("execass-state"));
        init_execass_fresh_root(&paths).expect("initialize ExecAss clean root");
        (temp_dir, paths)
    }

    fn sqlite_object_names(conn: &Connection, object_type: &str, prefix: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = ?1 AND name LIKE ?2 ORDER BY name",
            )
            .expect("prepare sqlite object inventory");
        stmt.query_map(params![object_type, format!("{prefix}%")], |row| {
            row.get::<_, String>(0)
        })
        .expect("query sqlite object inventory")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect sqlite object inventory")
    }

    fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
        let sql = format!("PRAGMA table_info({table})");
        let mut stmt = conn.prepare(&sql).expect("prepare table_info");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("query table_info")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect table_info")
    }

    fn test_table_exists(conn: &Connection, table: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
            params![table],
            |_| Ok(()),
        )
        .optional()
        .expect("query table existence")
        .is_some()
    }

    #[test]
    fn execass_authority_schema_distinguishes_follow_up_from_decision_bound_amendments() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");
        let insert = |id: &str,
                      correlation: &str,
                      kind: &str,
                      scope: &str,
                      decision_id: Option<&str>,
                      decision_revision: Option<i64>,
                      manifest: Option<&str>,
                      nonce: Option<&str>| {
            conn.execute(
                "INSERT INTO execass_authority_provenance (authority_provenance_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,policy_revision,bound_decision_id,bound_decision_revision,bound_manifest_digest,bound_challenge_nonce_digest,evidence_digest,created_at) VALUES (?1,'human_local','owner','native-owner-intake','interactive-native-owner',?2,?3,?4,1,?5,?6,?7,?8,?9,1)",
                params![
                    id,
                    correlation,
                    kind,
                    scope,
                    decision_id,
                    decision_revision,
                    manifest,
                    nonce,
                    format!("evidence-{id}"),
                ],
            )
        };

        assert_eq!(
            insert(
                "follow-up",
                "corr-follow-up",
                "action_specific_owner_amendment",
                r#"{"delegation_id":"delegation-1","delegation_revision":1,"plan_revision":1}"#,
                None,
                None,
                None,
                None,
            )
            .expect("typed follow-up amendment authority"),
            1
        );
        assert!(insert(
            "missing-plan",
            "corr-missing-plan",
            "action_specific_owner_amendment",
            r#"{"delegation_id":"delegation-1","delegation_revision":1}"#,
            None,
            None,
            None,
            None,
        )
        .is_err());
        assert!(insert(
            "partial-decision",
            "corr-partial-decision",
            "action_specific_owner_amendment",
            r#"{"delegation_id":"delegation-1","delegation_revision":1,"plan_revision":1}"#,
            Some("decision-1"),
            Some(1),
            None,
            None,
        )
        .is_err());
        assert_eq!(
            insert(
                "decision-amendment",
                "corr-decision-amendment",
                "action_specific_owner_amendment",
                r#"{"logical_action_id":"action-1"}"#,
                Some("decision-2"),
                Some(1),
                Some("manifest-2"),
                Some("nonce-2"),
            )
            .expect("decision-bound action-specific amendment authority"),
            1
        );
        assert!(insert(
            "original-with-decision",
            "corr-original-bound",
            "original_request",
            r#"{"request":"new"}"#,
            Some("decision-3"),
            Some(1),
            Some("manifest-3"),
            Some("nonce-3"),
        )
        .is_err());
    }

    #[test]
    fn execass_clean_root_installs_exact_schema_contract() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");

        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .expect("foreign_keys"),
            1
        );
        assert_eq!(
            conn.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
                .expect("application_id"),
            EXECASS_APPLICATION_ID
        );
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("user_version"),
            EXECASS_SCHEMA_VERSION
        );

        assert_eq!(
            sqlite_object_names(&conn, "table", "execass_"),
            [
                "execass_accepted_confirmation_grants",
                "execass_action_branches",
                "execass_amendment_criteria_links",
                "execass_attention_items",
                "execass_authority_links",
                "execass_authority_parent_bindings",
                "execass_authority_provenance",
                "execass_channel_reply_bindings",
                "execass_completion_assessments",
                "execass_confirmation_attestations",
                "execass_confirmation_authority_keys",
                "execass_confirmation_challenge_alternatives",
                "execass_confirmation_challenges",
                "execass_continuation_operation_history",
                "execass_continuations",
                "execass_criteria_sets",
                "execass_decisions",
                "execass_delegations",
                "execass_duplicate_risk_bindings",
                "execass_duplicate_risk_successors",
                "execass_effect_recorder_evidence",
                "execass_effect_recorder_keys",
                "execass_effect_tombstones",
                "execass_external_waits",
                "execass_global_runtime_control",
                "execass_lifecycle_transitions",
                "execass_logical_effects",
                "execass_notifications",
                "execass_outbox_cursors",
                "execass_outbox_events",
                "execass_outcome_criteria",
                "execass_owner_ingress_bindings",
                "execass_plan_amendments",
                "execass_plans",
                "execass_policy_revisions",
                "execass_provider_attempts",
                "execass_receipt_anchor_state",
                "execass_receipt_evidence_refs",
                "execass_receipt_journal_state",
                "execass_receipt_keys",
                "execass_receipt_recorder_evidence_refs",
                "execass_receipts",
                "execass_recovery_episodes",
                "execass_recovery_evaluations",
                "execass_routine_driver_jobs",
                "execass_routine_job_bindings",
                "execass_routine_occurrences",
                "execass_routine_schedule_state",
                "execass_routine_trigger_operations",
                "execass_routine_versions",
                "execass_routines",
                "execass_run_control_attestations",
                "execass_runtime_host_generations",
                "execass_runtime_host_leases",
                "execass_runtime_host_states",
                "execass_runtime_settings_revisions",
                "execass_schema_metadata",
                "execass_summary_acknowledgements",
                "execass_summary_deliveries",
                "execass_summary_delivery_items",
                "execass_technical_resource_actuals",
                "execass_technical_resource_quota_entries",
                "execass_technical_resource_quota_snapshots",
                "execass_technical_resource_requirement_sets",
                "execass_technical_resource_requirements",
                "execass_technical_resource_reservations",
                "execass_terminal_corrections",
                "execass_verifier_results",
            ]
        );
        assert_eq!(
            sqlite_object_names(&conn, "index", "idx_execass_"),
            [
                "idx_execass_action_branches_selection",
                "idx_execass_attention_items_actionable",
                "idx_execass_authority_link_assistant_audit_observation",
                "idx_execass_authority_link_attachment_observation",
                "idx_execass_authority_link_board_asset_observation",
                "idx_execass_authority_link_board_card_observation",
                "idx_execass_authority_link_board_observation",
                "idx_execass_authority_link_job_observation",
                "idx_execass_authority_link_job_run_observation",
                "idx_execass_authority_link_mail_attachment_observation",
                "idx_execass_authority_link_mail_message_observation",
                "idx_execass_authority_link_mail_thread_observation",
                "idx_execass_authority_link_run_observation",
                "idx_execass_authority_link_security_audit_observation",
                "idx_execass_authority_link_session_observation",
                "idx_execass_authority_link_task_observation",
                "idx_execass_authority_link_tool_call_observation",
                "idx_execass_authority_links_delegation_kind",
                "idx_execass_authority_links_outbox",
                "idx_execass_confirmation_attestations_provider_event",
                "idx_execass_confirmation_authority_one_active",
                "idx_execass_confirmation_challenges_pending",
                "idx_execass_continuation_one_nonreconcile_operation",
                "idx_execass_continuations_claim",
                "idx_execass_continuations_one_current_per_revision",
                "idx_execass_continuations_one_job",
                "idx_execass_criteria_sets_one_current",
                "idx_execass_decisions_pending",
                "idx_execass_delegations_source_message",
                "idx_execass_delegations_summary",
                "idx_execass_effect_recorder_active_generation",
                "idx_execass_effect_recorder_evidence_attempt",
                "idx_execass_effect_recorder_one_active",
                "idx_execass_external_waits_waiting",
                "idx_execass_logical_effects_reconcile",
                "idx_execass_notifications_dedupe",
                "idx_execass_notifications_due",
                "idx_execass_outbox_one_delegation_transition_per_revision",
                "idx_execass_outbox_unpublished",
                "idx_execass_owner_ingress_one_active_local",
                "idx_execass_receipt_evidence_source",
                "idx_execass_receipt_keys_one_active",
                "idx_execass_receipt_keys_one_provisioned",
                "idx_execass_receipt_recorder_evidence_source",
                "idx_execass_receipts_causation",
                "idx_execass_recovery_evaluations_due",
                "idx_execass_run_control_attestations_provider_event",
                "idx_execass_runtime_host_one_live_lease",
                "idx_execass_technical_resource_quota_entries_lookup",
                "idx_execass_technical_resource_requirements_lookup",
                "idx_execass_technical_resource_reservations_active",
            ]
        );
        assert_eq!(
            sqlite_object_names(&conn, "trigger", "execass_"),
            [
                "execass_accepted_confirmation_grants_no_delete",
                "execass_action_branch_identity_immutable",
                "execass_amendment_criteria_links_immutable",
                "execass_amendment_criteria_links_no_delete",
                "execass_attention_identity_immutable",
                "execass_attention_runtime_generation_unique",
                "execass_authority_links_immutable",
                "execass_authority_links_insert_guard",
                "execass_authority_links_no_delete",
                "execass_authority_parent_binding_kind",
                "execass_authority_parent_bindings_immutable",
                "execass_authority_parent_bindings_no_delete",
                "execass_authority_provenance_immutable",
                "execass_authority_provenance_no_delete",
                "execass_channel_reply_bindings_immutable",
                "execass_channel_reply_bindings_no_delete",
                "execass_completion_assessments_immutable",
                "execass_completion_assessments_no_delete",
                "execass_confirmation_attestation_insert_binding",
                "execass_confirmation_attestations_immutable",
                "execass_confirmation_attestations_no_delete",
                "execass_confirmation_authority_keys_immutable",
                "execass_confirmation_authority_keys_no_delete",
                "execass_confirmation_challenge_alternative_insert_binding",
                "execass_confirmation_challenge_alternatives_immutable",
                "execass_confirmation_challenge_alternatives_no_delete",
                "execass_confirmation_challenge_binding_immutable",
                "execass_confirmation_challenge_insert_binding",
                "execass_confirmation_challenge_resolution_once",
                "execass_confirmation_challenge_selection_once",
                "execass_confirmation_challenges_no_delete",
                "execass_confirmation_grant_identity_immutable",
                "execass_confirmation_grant_invalidation_is_action_specific",
                "execass_confirmation_grant_owner_invalidation_provenance",
                "execass_confirmation_grant_requires_resolved_challenge",
                "execass_continuation_claim_transition_requires_history",
                "execass_continuation_fence_monotonic",
                "execass_continuation_job_binding_one_way",
                "execass_continuation_operation_history_immutable",
                "execass_continuation_operation_history_no_delete",
                "execass_continuation_settle_transition_requires_history",
                "execass_criteria_sets_lineage_immutable",
                "execass_criteria_sets_no_delete",
                "execass_decision_binding_immutable",
                "execass_decision_resolution_requires_pending_record",
                "execass_decision_resolution_requires_server_derived_owner",
                "execass_decisions_no_delete",
                "execass_delegation_criteria_set_guard",
                "execass_delegation_revision_monotonic",
                "execass_duplicate_risk_binding_insert_guard",
                "execass_duplicate_risk_bindings_immutable",
                "execass_duplicate_risk_bindings_no_delete",
                "execass_duplicate_risk_successor_insert_guard",
                "execass_duplicate_risk_successors_immutable",
                "execass_duplicate_risk_successors_no_delete",
                "execass_effect_recorder_evidence_immutable",
                "execass_effect_recorder_evidence_insert_guard",
                "execass_effect_recorder_evidence_no_delete",
                "execass_effect_recorder_evidence_payload_guard",
                "execass_effect_recorder_keys_immutable",
                "execass_effect_recorder_keys_no_delete",
                "execass_effect_tombstones_immutable",
                "execass_effect_tombstones_no_delete",
                "execass_external_wait_identity_immutable",
                "execass_global_runtime_control_no_delete",
                "execass_global_runtime_control_transition_guard",
                "execass_internal_job_delete_forbidden",
                "execass_internal_job_identity_immutable",
                "execass_lifecycle_transitions_immutable",
                "execass_lifecycle_transitions_no_delete",
                "execass_logical_effect_identity_immutable",
                "execass_logical_effect_no_delete",
                "execass_logical_effect_recorder_execution_guard",
                "execass_logical_effect_recorder_reconcile_guard",
                "execass_notifications_identity_immutable",
                "execass_notifications_monotonic",
                "execass_notifications_no_delete",
                "execass_outbox_cursor_monotonic",
                "execass_outbox_cursors_no_delete",
                "execass_outbox_events_immutable_identity",
                "execass_outbox_events_no_delete",
                "execass_outbox_global_sequence_gap_free",
                "execass_outcome_criteria_immutable",
                "execass_outcome_criteria_no_delete",
                "execass_outcome_unknown_attempt_prohibition",
                "execass_owner_ingress_bindings_immutable",
                "execass_owner_ingress_bindings_no_delete",
                "execass_plan_amendments_immutable",
                "execass_plan_amendments_no_delete",
                "execass_plans_immutable",
                "execass_plans_no_delete",
                "execass_policy_owner_provenance_guard",
                "execass_policy_revision_progression_guard",
                "execass_policy_revisions_immutable",
                "execass_policy_revisions_no_delete",
                "execass_provider_attempt_claim_binding_guard",
                "execass_provider_attempt_identity_immutable",
                "execass_provider_attempt_no_delete",
                "execass_provider_attempt_recorder_execution_guard",
                "execass_provider_attempt_recorder_reconcile_guard",
                "execass_provider_attempt_terminal_fields_immutable",
                "execass_provider_attempt_transition_guard",
                "execass_receipt_anchor_delete_guard",
                "execass_receipt_anchor_identity_immutable",
                "execass_receipt_anchor_terminal_immutable",
                "execass_receipt_anchor_transition_guard",
                "execass_receipt_canonical_insert_guard",
                "execass_receipt_evidence_immutable",
                "execass_receipt_evidence_insert_guard",
                "execass_receipt_evidence_no_delete",
                "execass_receipt_journal_advance_guard",
                "execass_receipt_keys_identity_immutable",
                "execass_receipt_keys_no_delete",
                "execass_receipt_keys_transition_guard",
                "execass_receipt_recorder_evidence_immutable",
                "execass_receipt_recorder_evidence_insert_guard",
                "execass_receipt_recorder_evidence_no_delete",
                "execass_receipt_recorder_evidence_receipt_guard",
                "execass_receipts_immutable",
                "execass_receipts_no_delete",
                "execass_recovery_episodes_immutable",
                "execass_recovery_episodes_no_delete",
                "execass_recovery_evaluations_immutable",
                "execass_recovery_evaluations_no_delete",
                "execass_reserved_routine_job_immutable",
                "execass_reserved_routine_job_no_delete",
                "execass_routine_driver_jobs_immutable",
                "execass_routine_driver_jobs_no_delete",
                "execass_routine_job_bindings_immutable",
                "execass_routine_job_bindings_no_delete",
                "execass_routine_occurrence_identity_immutable",
                "execass_routine_occurrences_no_delete",
                "execass_routine_trigger_operations_immutable",
                "execass_routine_trigger_operations_no_delete",
                "execass_routine_versions_immutable",
                "execass_routine_versions_no_delete",
                "execass_run_control_attestation_insert_binding",
                "execass_run_control_attestations_immutable",
                "execass_run_control_attestations_no_delete",
                "execass_runtime_generation_identity_immutable",
                "execass_runtime_generation_terminal_irreversible",
                "execass_runtime_host_state_identity_immutable",
                "execass_runtime_host_state_no_delete",
                "execass_runtime_lease_identity_immutable",
                "execass_runtime_lease_release_irreversible",
                "execass_runtime_settings_immutable",
                "execass_runtime_settings_no_delete",
                "execass_runtime_settings_owner_provenance_guard",
                "execass_security_audit_archive_no_delete_when_linked",
                "execass_security_audit_live_no_delete_without_archive",
                "execass_summary_acknowledgements_immutable",
                "execass_summary_acknowledgements_no_delete",
                "execass_summary_deliveries_immutable",
                "execass_summary_deliveries_no_delete",
                "execass_summary_delivery_items_count_guard",
                "execass_summary_delivery_items_immutable",
                "execass_summary_delivery_items_no_delete",
                "execass_technical_resource_actual_insert_guard",
                "execass_technical_resource_actuals_immutable",
                "execass_technical_resource_actuals_no_delete",
                "execass_technical_resource_quota_entries_immutable",
                "execass_technical_resource_quota_entries_no_delete",
                "execass_technical_resource_quota_snapshots_immutable",
                "execass_technical_resource_quota_snapshots_no_delete",
                "execass_technical_resource_requirement_set_insert_guard",
                "execass_technical_resource_requirement_sets_immutable",
                "execass_technical_resource_requirement_sets_no_delete",
                "execass_technical_resource_requirements_immutable",
                "execass_technical_resource_requirements_no_delete",
                "execass_technical_resource_reservation_capacity_guard",
                "execass_technical_resource_reservation_identity_immutable",
                "execass_technical_resource_reservation_insert_guard",
                "execass_technical_resource_reservation_status_guard",
                "execass_technical_resource_reservations_no_delete",
                "execass_terminal_corrections_immutable",
                "execass_terminal_corrections_no_delete",
                "execass_verifier_results_immutable",
                "execass_verifier_results_no_delete",
            ]
        );

        assert_eq!(
            table_columns(&conn, "execass_delegations"),
            [
                "delegation_id",
                "normalized_original_intent",
                "intake_evidence_json",
                "ingress_source",
                "ingress_credential_identity",
                "source_message_id",
                "source_correlation_id",
                "ingress_idempotency_key",
                "classifier_version",
                "classifier_reasons_json",
                "phase",
                "run_control",
                "state_revision",
                "current_plan_revision",
                "current_criteria_revision",
                "policy_revision",
                "effective_authority_json",
                "authority_provenance_id",
                "pending_decision_id",
                "external_wait_json",
                "stop_epoch",
                "completion_assessment_json",
                "receipt_chain_count",
                "receipt_chain_head_digest",
                "created_at",
                "updated_at",
                "acknowledged_at",
                "terminal_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_continuations"),
            [
                "continuation_id",
                "delegation_id",
                "target_delegation_revision",
                "target_plan_revision",
                "action_id",
                "branch_kind",
                "causation_kind",
                "causation_id",
                "status",
                "job_id",
                "lease_owner",
                "lease_expires_at",
                "fencing_token",
                "host_generation",
                "stop_epoch",
                "global_stop_epoch",
                "created_at",
                "updated_at",
                "completed_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_routine_versions"),
            [
                "routine_id",
                "routine_version",
                "source_delegation_id",
                "saved_owner_authority_provenance_id",
                "normalized_original_intent",
                "resolved_leaf_manifest_json",
                "manifest_digest",
                "saved_selector_json",
                "saved_action_envelope_json",
                "accepted_confirmation_grant_id",
                "effective_policy_snapshot_json",
                "effective_policy_revision",
                "stable_leaf_digest",
                "created_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_global_runtime_control"),
            [
                "singleton",
                "engaged",
                "global_stop_epoch",
                "current_policy_revision",
                "updated_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_run_control_attestations"),
            [
                "attestation_digest",
                "replay_identity",
                "authority_provenance_id",
                "pinned_key_id",
                "pinned_key_generation",
                "actor_type",
                "credential_identity",
                "authenticated_ingress",
                "channel_assurance",
                "request_correlation_id",
                "source_message_id",
                "provider_event_id",
                "operation",
                "target_kind",
                "target_delegation_id",
                "idempotency_key",
                "stopped_epoch",
                "policy_revision",
                "unresolved_effect_disclosure_digest",
                "delegation_state_revision",
                "current_plan_revision",
                "canonical_root_identity",
                "installation_identity",
                "os_user_identity_digest",
                "state_root_generation",
                "normalized_scope_json",
                "signed_payload_json",
                "signature_hex",
                "observed_at",
                "issued_at",
                "verified_at",
                "receipt_id",
                "outbox_event_id",
                "receipt_command_digest",
                "outbox_event_digest",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_decisions"),
            [
                "decision_id",
                "delegation_id",
                "decision_revision",
                "delegation_revision",
                "plan_revision",
                "policy_revision",
                "decision_kind",
                "status",
                "result",
                "exact_presented_action_json",
                "confirmed_logical_action_identity",
                "manifest_digest",
                "payload_digest",
                "payload_and_material_operands_json",
                "target_audience_path_json",
                "connector_tool_identity",
                "connector_tool_version",
                "side_effect_envelope_json",
                "recommendation",
                "consequence",
                "alternatives_json",
                "idempotency_key",
                "requested_at",
                "resolved_at",
                "resolved_by_authority_provenance_id",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_logical_effects"),
            [
                "logical_effect_id",
                "delegation_id",
                "continuation_id",
                "action_kind",
                "operation_reversible",
                "declared_recovery_safe_boundary",
                "state",
                "internal_idempotency_key",
                "provider_identity",
                "provider_idempotency_key",
                "reconciliation_key",
                "manifest_digest",
                "payload_digest",
                "outcome_json",
                "created_at",
                "updated_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_confirmation_challenges"),
            [
                "challenge_id",
                "decision_id",
                "delegation_id",
                "decision_revision",
                "exact_presented_action_json",
                "confirmed_logical_action_identity",
                "manifest_digest",
                "payload_digest",
                "payload_and_material_operands_json",
                "connector_tool_identity",
                "connector_tool_version",
                "canonical_action_envelope_or_selector_json",
                "declared_consequence",
                "selected_logical_action_id",
                "nonce_digest",
                "status",
                "created_at",
                "expires_at",
                "resolved_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_confirmation_authority_keys"),
            [
                "key_id",
                "key_generation",
                "verifying_key_hex",
                "verifying_key_digest",
                "canonical_root_identity",
                "installation_identity",
                "os_user_identity_digest",
                "state_root_generation",
                "status",
                "created_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_owner_ingress_bindings"),
            [
                "binding_id",
                "actor_type",
                "credential_identity",
                "authenticated_ingress",
                "channel_assurance",
                "provider_event_required",
                "status",
                "created_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_channel_reply_bindings"),
            [
                "binding_id",
                "delegation_id",
                "provider",
                "authenticated_ingress",
                "owner_credential_identity",
                "conversation_id",
                "outbound_message_id",
                "created_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_confirmation_attestations"),
            [
                "attestation_digest",
                "decision_id",
                "authority_provenance_id",
                "pinned_key_id",
                "pinned_key_generation",
                "actor_type",
                "credential_identity",
                "authenticated_ingress",
                "channel_assurance",
                "request_correlation_id",
                "source_message_id",
                "provider_event_id",
                "selected_logical_action_id",
                "signed_payload_json",
                "signature_hex",
                "issued_at",
                "expires_at",
                "verified_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_accepted_confirmation_grants"),
            [
                "grant_id",
                "delegation_id",
                "decision_id",
                "confirmed_logical_action_identity",
                "canonical_action_envelope_or_selector_json",
                "payload_and_material_operands_json",
                "payload_and_material_operands_digest",
                "connector_tool_identity",
                "connector_tool_version",
                "declared_consequence",
                "accepted_by_authority_provenance_id",
                "confirmation_attestation_digest",
                "accepted_at",
                "invalidated_at",
                "invalidation_reason",
                "invalidated_by_authority_provenance_id",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_receipt_anchor_state"),
            [
                "anchor_id",
                "root_identity",
                "state_root_generation",
                "anchor_generation",
                "status",
                "receipt_count",
                "receipt_head_digest",
                "key_id",
                "key_generation",
                "transaction_id",
                "external_receipt_digest",
                "prepared_document_digest",
                "receipt_commit_confirmed",
                "receipt_committed_at",
                "receipt_commit_confirmation_tag",
                "finalized_document_digest",
                "prepared_at",
                "finalized_at",
                "quarantined_at",
                "quarantine_reason",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_receipt_keys"),
            [
                "key_id",
                "key_generation",
                "status",
                "rotated_from_key_id",
                "rotated_from_key_generation",
                "created_at",
                "registry_integrity_tag",
                "activated_anchor_generation",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_outbox_events"),
            [
                "global_sequence",
                "event_id",
                "event_name",
                "aggregate_id",
                "aggregate_revision",
                "correlation_id",
                "causation_id",
                "occurred_at",
                "schema_version",
                "safe_payload_json",
                "duplicate_identity",
                "published_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_summary_deliveries"),
            [
                "delivery_id",
                "displayed_cursor",
                "projection_version",
                "through_global_sequence",
                "item_set_digest",
                "item_count",
                "request_correlation_id",
                "delivered_at",
            ]
        );
        assert_eq!(
            table_columns(&conn, "execass_notifications"),
            [
                "notification_id",
                "attention_id",
                "completion_assessment_id",
                "outbox_event_id",
                "delegation_id",
                "decision_id",
                "reason_revision",
                "attention_variant",
                "reason",
                "channel",
                "status",
                "safe_payload_json",
                "requested_at",
                "scheduled_at",
                "next_reminder_at",
                "quiet_hours_json",
                "reminder_count",
                "last_reminded_at",
                "dispatched_at",
                "updated_at",
                "idempotency_key",
            ]
        );

        let authority_targets = {
            let mut stmt = conn
                .prepare("PRAGMA foreign_key_list(execass_authority_links)")
                .expect("prepare authority-link foreign keys");
            let mut rows = stmt
                .query_map([], |row| row.get::<_, String>(2))
                .expect("query authority-link foreign keys")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect authority-link foreign keys");
            rows.sort();
            rows
        };
        assert_eq!(
            authority_targets,
            [
                "agent_mail_attachments",
                "agent_mail_messages",
                "agent_mail_threads",
                "assistant_tool_calls_audit",
                "attachments",
                "board_card_assets",
                "board_cards",
                "boards",
                "execass_delegations",
                "execass_outbox_events",
                "job_runs",
                "jobs",
                "runs",
                "sessions",
                "tasks",
                "tool_calls",
            ]
        );

        let execass_tables = sqlite_object_names(&conn, "table", "execass_");
        for table in execass_tables {
            let pragma = format!("PRAGMA foreign_key_list({table})");
            let mut stmt = conn
                .prepare(&pragma)
                .expect("prepare foreign-key inventory");
            let delete_actions = stmt
                .query_map([], |row| row.get::<_, String>(6))
                .expect("query foreign-key inventory")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect foreign-key inventory");
            assert!(
                delete_actions.iter().all(|action| action != "CASCADE"),
                "{table} must not erase evidence through cascading deletes"
            );
        }

        let delegation_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='execass_delegations'",
                [],
                |row| row.get(0),
            )
            .expect("delegation DDL");
        for closed_value in [
            "accepted",
            "partially_completed",
            "failed",
            "running",
            "stop_requested",
            "stopped",
        ] {
            assert!(
                delegation_sql.contains(&format!("'{closed_value}'")),
                "delegation CHECK must contain {closed_value}"
            );
        }

        let versions = {
            let mut stmt = conn
                .prepare("SELECT version FROM schema_migrations ORDER BY version")
                .expect("prepare migration versions");
            stmt.query_map([], |row| row.get::<_, i64>(0))
                .expect("query migration versions")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect migration versions")
        };
        assert_eq!(versions, [1, 2, 3, 4, 5, 6, 7, 8]);
        for retired in ["approvals", "assistant_workers", "assistant_task_links"] {
            assert!(
                !test_table_exists(&conn, retired),
                "clean ExecAss root must not retain retired authority table {retired}"
            );
        }
    }

    fn seed_execass_constraint_fixture(conn: &Connection) {
        conn.execute(
            r#"
            INSERT INTO execass_authority_provenance (
              authority_provenance_id, actor_type, credential_identity,
              authenticated_ingress, channel_assurance, source_correlation_id,
              authority_kind, normalized_scope_json, policy_revision,
              evidence_digest, created_at
            ) VALUES ('authority-1', 'human_local', 'local-user', 'local-ui',
                      'interactive', 'correlation-1', 'original_request', '{}', 1,
                      'evidence-1', 1)
            "#,
            [],
        )
        .expect("seed authority provenance");
        conn.execute(
            r#"
            INSERT INTO execass_delegations (
              delegation_id, normalized_original_intent, intake_evidence_json,
              ingress_source, ingress_credential_identity, source_correlation_id,
              ingress_idempotency_key, classifier_version, classifier_reasons_json,
              phase, run_control, state_revision, policy_revision,
              effective_authority_json, authority_provenance_id, created_at, updated_at
            ) VALUES ('delegation-1', 'test', '{}', 'local-ui', 'local-user',
                      'correlation-1', 'intake-key-1', 'v1', '[]', 'accepted',
                      'running', 1, 1, '{}', 'authority-1', 1, 1)
            "#,
            [],
        )
        .expect("seed delegation");
        conn.execute(
            r#"
            INSERT INTO execass_plans (
              plan_id, delegation_id, plan_revision, based_on_delegation_revision,
              policy_revision, plan_summary, resolved_leaf_manifest_json,
              manifest_digest, created_by_authority_provenance_id, created_at
            ) VALUES ('plan-1', 'delegation-1', 1, 1, 1, 'test plan', '[]',
                      'manifest-1', 'authority-1', 1)
            "#,
            [],
        )
        .expect("seed plan");
        conn.execute(
            r#"
            INSERT INTO execass_action_branches (
              action_id, delegation_id, action_revision, target_delegation_revision,
              target_plan_revision, stop_epoch, branch_kind, status, action_summary,
              created_at, updated_at
            ) VALUES ('seed-action', 'delegation-1', 1, 1, 1, 0, 'ordinary',
                      'runnable', 'seed action', 1, 1)
            "#,
            [],
        )
        .expect("seed action branch");
        conn.execute(
            r#"
            INSERT INTO execass_continuations (
              continuation_id, delegation_id, target_delegation_revision,
              target_plan_revision, action_id, branch_kind, causation_kind, causation_id, status,
              fencing_token, host_generation, stop_epoch, global_stop_epoch, created_at, updated_at
            ) VALUES ('continuation-1', 'delegation-1', 1, 1, 'seed-action', 'ordinary', 'plan', 'plan-1',
                      'runnable', 0, 1, 0, 0, 1, 1)
            "#,
            [],
        )
        .expect("seed continuation");

        assert!(conn
            .execute(
                "UPDATE execass_delegations SET phase='not_a_phase', state_revision=2 WHERE delegation_id='delegation-1'",
                [],
            )
            .is_err());
    }

    #[test]
    fn execass_v11_challenge_grant_and_technical_resource_schema_reject_stale_authority() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");
        seed_execass_constraint_fixture(&conn);

        let schema: String = conn
            .query_row(
                "SELECT group_concat(sql, '\n') FROM sqlite_schema WHERE name LIKE 'execass_%' AND sql IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("load exact ExecAss schema");
        for forbidden in [
            "ordinary_decision",
            "hard_lock",
            "maximum_budget",
            "spending_or_financial_commitment",
            "money",
            "currency",
            "payee",
            "purchase",
            "'approval'",
        ] {
            assert!(
                !schema.contains(forbidden),
                "retired ExecAss schema token must be absent: {forbidden}"
            );
        }
        assert!(schema.contains("execass_confirmation_challenges"));
        assert!(schema.contains("execass_accepted_confirmation_grants"));
        assert!(schema.contains(
            "technical_resource_kind IN ('tokens', 'time_ms', 'connector_calls', 'resource_units')"
        ));

        conn.execute(
            r#"
            INSERT INTO execass_decisions (
              decision_id, delegation_id, decision_revision, delegation_revision,
              plan_revision, policy_revision, decision_kind, status,
              exact_presented_action_json, confirmed_logical_action_identity, manifest_digest, payload_digest,
              payload_and_material_operands_json,
              target_audience_path_json, connector_tool_identity, connector_tool_version,
              side_effect_envelope_json, recommendation,
              consequence, alternatives_json, idempotency_key, requested_at
            ) VALUES ('danger-decision', 'delegation-1', 1, 1, 1, 1,
                      'dangerous_action_confirmation', 'pending', '{}', 'logical-action-1', 'manifest-1',
                      'payload-1', '{}', '[]', 'tool-1', 'v1', '{}', 'continue', 'concrete consequence',
                      '["confirm_and_continue","revise","decline"]', 'danger-idempotency', 1)
            "#,
            [],
        )
        .expect("seed typed dangerous decision");
        let insert_challenge = r#"
            INSERT INTO execass_confirmation_challenges (
              challenge_id, decision_id, delegation_id, decision_revision,
              exact_presented_action_json, confirmed_logical_action_identity, manifest_digest,
              payload_digest, payload_and_material_operands_json, connector_tool_identity,
              connector_tool_version, canonical_action_envelope_or_selector_json,
              declared_consequence, nonce_digest, status, created_at, expires_at, resolved_at
            ) VALUES (?1, 'danger-decision', 'delegation-1', 1, ?2, ?3, 'manifest-1',
                      ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, 100, ?12)
        "#;
        for (
            suffix,
            action,
            logical_action,
            payload_digest,
            operands,
            tool,
            version,
            envelope,
            consequence,
            status,
            resolved_at,
        ) in [
            (
                "pre-resolved",
                "{}",
                "logical-action-1",
                "payload-1",
                "{}",
                "tool-1",
                "v1",
                "{}",
                "concrete consequence",
                "resolved",
                Some(2_i64),
            ),
            (
                "pre-expired",
                "{}",
                "logical-action-1",
                "payload-1",
                "{}",
                "tool-1",
                "v1",
                "{}",
                "concrete consequence",
                "expired",
                None,
            ),
            (
                "action",
                "{\"other\":true}",
                "logical-action-1",
                "payload-1",
                "{}",
                "tool-1",
                "v1",
                "{}",
                "concrete consequence",
                "pending",
                None,
            ),
            (
                "consequence",
                "{}",
                "logical-action-1",
                "payload-1",
                "{}",
                "tool-1",
                "v1",
                "{}",
                "other consequence",
                "pending",
                None,
            ),
            (
                "envelope",
                "{}",
                "logical-action-1",
                "payload-1",
                "{}",
                "tool-1",
                "v1",
                "{\"other\":true}",
                "concrete consequence",
                "pending",
                None,
            ),
            (
                "tool",
                "{}",
                "logical-action-1",
                "payload-1",
                "{}",
                "other-tool",
                "v1",
                "{}",
                "concrete consequence",
                "pending",
                None,
            ),
            (
                "payload",
                "{}",
                "logical-action-1",
                "payload-1",
                "{\"other\":true}",
                "tool-1",
                "v1",
                "{}",
                "concrete consequence",
                "pending",
                None,
            ),
        ] {
            assert!(
                conn.execute(
                    insert_challenge,
                    params![
                        format!("challenge-{suffix}"),
                        action,
                        logical_action,
                        payload_digest,
                        operands,
                        tool,
                        version,
                        envelope,
                        consequence,
                        format!("nonce-{suffix}"),
                        status,
                        resolved_at,
                    ],
                )
                .is_err(),
                "hostile challenge case {suffix} must fail"
            );
        }
        conn.execute(
            r#"
            INSERT INTO execass_confirmation_challenges (
              challenge_id, decision_id, delegation_id, decision_revision,
              exact_presented_action_json, confirmed_logical_action_identity, manifest_digest, payload_digest,
              payload_and_material_operands_json, connector_tool_identity, connector_tool_version,
              canonical_action_envelope_or_selector_json, declared_consequence,
              nonce_digest, status, created_at, expires_at
            ) VALUES ('challenge-1', 'danger-decision', 'delegation-1', 1,
                      '{}', 'logical-action-1', 'manifest-1', 'payload-1', '{}', 'tool-1', 'v1', '{}',
                      'concrete consequence', 'nonce-1', 'pending', 1, 100)
            "#,
            [],
        )
        .expect("seed expiring single-resolution challenge");
        conn.execute(
            r#"
            INSERT INTO execass_confirmation_challenge_alternatives (
              challenge_id, logical_action_id, exact_presented_action_json,
              confirmed_logical_action_identity, manifest_digest, payload_digest,
              payload_and_material_operands_json, target_audience_path_json,
              connector_tool_identity, connector_tool_version,
              canonical_action_envelope_or_selector_json, declared_consequence
            ) VALUES ('challenge-1', 'logical-action-1', '{}', 'logical-action-1',
                      'manifest-1', 'payload-1', '{}', '[]', 'tool-1', 'v1', '{}',
                      'concrete consequence')
            "#,
            [],
        )
        .expect("seed exact disclosed confirmation alternative");
        conn.execute(
            r#"
            INSERT INTO execass_authority_provenance (
              authority_provenance_id, actor_type, credential_identity,
              authenticated_ingress, channel_assurance, source_correlation_id,
              source_message_id,
              authority_kind, normalized_scope_json, policy_revision,
              bound_decision_id, bound_decision_revision, bound_manifest_digest,
              bound_challenge_nonce_digest, evidence_digest, created_at, expires_at
            ) VALUES ('remote-owner-resolution', 'human_remote', 'owner', 'allowlisted-channel',
                      'authenticated', 'resolution-correlation', 'provider-message-1',
                      'decision_resolution', '{}', 1, 'danger-decision', 1, 'manifest-1',
                      'nonce-1', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 2, 100)
            "#,
            [],
        )
        .expect("seed authenticated remote owner resolution");
        conn.execute(
            "INSERT INTO execass_confirmation_authority_keys (key_id,key_generation,verifying_key_hex,verifying_key_digest,canonical_root_identity,installation_identity,os_user_identity_digest,state_root_generation,status,created_at) VALUES ('confirmation-key-1',1,'0000000000000000000000000000000000000000000000000000000000000000','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc','install-1','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',1,'active',1)",
            [],
        )
        .expect("seed pinned confirmation key");
        conn.execute(
            "INSERT INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES ('remote-binding-1','human_remote','owner','allowlisted-channel','authenticated',1,'active',1)",
            [],
        )
        .expect("seed exact remote owner binding");
        conn.execute(
            r#"
            INSERT INTO execass_confirmation_attestations (
              attestation_digest,decision_id,authority_provenance_id,pinned_key_id,
              pinned_key_generation,actor_type,credential_identity,authenticated_ingress,
              channel_assurance,request_correlation_id,source_message_id,provider_event_id,
              selected_logical_action_id,signed_payload_json,signature_hex,issued_at,expires_at,verified_at
            ) VALUES (
              'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
              'danger-decision','remote-owner-resolution','confirmation-key-1',1,
              'human_remote','owner','allowlisted-channel','authenticated','resolution-correlation',
              'provider-message-1','provider-event-1','logical-action-1',
              '{"actor_type":"human_remote","credential_identity":"owner","authenticated_ingress":"allowlisted-channel","channel_assurance":"authenticated","request_correlation_id":"resolution-correlation","source_message_id":"provider-message-1","provider_event_id":"provider-event-1","policy_revision":1,"decision_id":"danger-decision","decision_revision":1,"decision_result":"confirm_and_continue","canonical_manifest_digest":"manifest-1","selected_logical_action_id":"logical-action-1","challenge_nonce_digest":"nonce-1","challenge_expires_at_ms":100,"issued_at_ms":2,"canonical_root_identity":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","installation_identity":"install-1","os_user_identity_digest":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","state_root_generation":1,"signer_key_generation":1}',
              '00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000',
              2,100,2
            )
            "#,
            [],
        )
        .expect("seed exact stored confirmation attestation");
        assert!(conn
            .execute(
                "UPDATE execass_confirmation_challenges SET status='resolved', selected_logical_action_id='logical-action-1', resolved_at=101 WHERE challenge_id='challenge-1'",
                [],
            )
            .is_err());
        conn.execute(
            "UPDATE execass_confirmation_challenges SET status='resolved', selected_logical_action_id='logical-action-1', resolved_at=2 WHERE challenge_id='challenge-1'",
            [],
        )
        .expect("resolve challenge once");
        assert!(conn
            .execute(
                "UPDATE execass_confirmation_challenges SET status='expired' WHERE challenge_id='challenge-1'",
                [],
            )
            .is_err());
        conn.execute(
            "UPDATE execass_decisions SET status='resolved', result='confirm_and_continue', resolved_at=2, resolved_by_authority_provenance_id='remote-owner-resolution' WHERE decision_id='danger-decision'",
            [],
        )
        .expect("authenticated remote owner resolves typed decision");
        let insert_grant = r#"
            INSERT INTO execass_accepted_confirmation_grants (
              grant_id, delegation_id, decision_id, confirmed_logical_action_identity,
              canonical_action_envelope_or_selector_json, payload_and_material_operands_json,
              payload_and_material_operands_digest, connector_tool_identity, connector_tool_version,
              declared_consequence, accepted_by_authority_provenance_id,
              confirmation_attestation_digest, accepted_at
            ) VALUES (?1, 'delegation-1', 'danger-decision', ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                      'remote-owner-resolution',
                      'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', ?9)
        "#;
        for (suffix, action, envelope, operands, digest, tool, version, consequence, accepted_at) in [
            (
                "action",
                "other-logical-action",
                "{}",
                "{}",
                "payload-1",
                "tool-1",
                "v1",
                "concrete consequence",
                2_i64,
            ),
            (
                "envelope",
                "logical-action-1",
                "{\"other\":true}",
                "{}",
                "payload-1",
                "tool-1",
                "v1",
                "concrete consequence",
                2_i64,
            ),
            (
                "payload",
                "logical-action-1",
                "{}",
                "{\"other\":true}",
                "payload-1",
                "tool-1",
                "v1",
                "concrete consequence",
                2_i64,
            ),
            (
                "digest",
                "logical-action-1",
                "{}",
                "{}",
                "other-payload",
                "tool-1",
                "v1",
                "concrete consequence",
                2_i64,
            ),
            (
                "tool",
                "logical-action-1",
                "{}",
                "{}",
                "payload-1",
                "other-tool",
                "v1",
                "concrete consequence",
                2_i64,
            ),
            (
                "consequence",
                "logical-action-1",
                "{}",
                "{}",
                "payload-1",
                "tool-1",
                "v1",
                "other consequence",
                2_i64,
            ),
            (
                "accepted-before-resolution",
                "logical-action-1",
                "{}",
                "{}",
                "payload-1",
                "tool-1",
                "v1",
                "concrete consequence",
                1_i64,
            ),
        ] {
            assert!(
                conn.execute(
                    insert_grant,
                    params![
                        format!("grant-{suffix}"),
                        action,
                        envelope,
                        operands,
                        digest,
                        tool,
                        version,
                        consequence,
                        accepted_at,
                    ],
                )
                .is_err(),
                "hostile grant case {suffix} must fail"
            );
        }
        conn.execute(
            r#"
            INSERT INTO execass_accepted_confirmation_grants (
              grant_id, delegation_id, decision_id, confirmed_logical_action_identity,
              canonical_action_envelope_or_selector_json, payload_and_material_operands_json,
              payload_and_material_operands_digest,
              connector_tool_identity, connector_tool_version,
              declared_consequence, accepted_by_authority_provenance_id,
              confirmation_attestation_digest, accepted_at
            ) VALUES ('grant-1', 'delegation-1', 'danger-decision', 'logical-action-1',
                      '{}', '{}', 'payload-1', 'tool-1', 'v1', 'concrete consequence',
                      'remote-owner-resolution',
                      'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 2)
            "#,
            [],
        )
        .expect("persist durable accepted confirmation grant");
        for statement in [
            "DELETE FROM execass_accepted_confirmation_grants WHERE grant_id='grant-1'",
            "DELETE FROM execass_confirmation_attestations WHERE decision_id='danger-decision'",
            "DELETE FROM execass_confirmation_authority_keys WHERE key_id='confirmation-key-1'",
            "DELETE FROM execass_owner_ingress_bindings WHERE binding_id='remote-binding-1'",
            "DELETE FROM execass_confirmation_challenges WHERE challenge_id='challenge-1'",
            "DELETE FROM execass_decisions WHERE decision_id='danger-decision'",
        ] {
            assert!(conn.execute(statement, []).is_err(), "{statement}");
        }
        for statement in [
            "UPDATE execass_decisions SET confirmed_logical_action_identity='mutated' WHERE decision_id='danger-decision'",
            "UPDATE execass_confirmation_challenges SET confirmed_logical_action_identity='mutated' WHERE challenge_id='challenge-1'",
            "UPDATE execass_confirmation_challenges SET payload_digest='mutated' WHERE challenge_id='challenge-1'",
            "UPDATE execass_confirmation_challenges SET payload_and_material_operands_json='{\"mutated\":true}' WHERE challenge_id='challenge-1'",
            "UPDATE execass_confirmation_challenges SET connector_tool_identity='mutated' WHERE challenge_id='challenge-1'",
            "UPDATE execass_confirmation_challenges SET connector_tool_version='mutated' WHERE challenge_id='challenge-1'",
            "UPDATE execass_confirmation_challenges SET canonical_action_envelope_or_selector_json='{\"mutated\":true}' WHERE challenge_id='challenge-1'",
            "UPDATE execass_confirmation_attestations SET signature_hex='ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff' WHERE decision_id='danger-decision'",
            "UPDATE execass_confirmation_authority_keys SET status='retired' WHERE key_id='confirmation-key-1'",
            "UPDATE execass_owner_ingress_bindings SET status='retired' WHERE binding_id='remote-binding-1'",
            "UPDATE execass_accepted_confirmation_grants SET payload_and_material_operands_digest='mutated' WHERE grant_id='grant-1'",
        ] {
            assert!(conn.execute(statement, []).is_err(), "{statement}");
        }
        assert!(conn
            .execute(
                "UPDATE execass_accepted_confirmation_grants SET invalidated_at=3, invalidation_reason='explicit_action_specific_owner_revocation' WHERE grant_id='grant-1'",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE execass_accepted_confirmation_grants SET invalidated_at=3, invalidation_reason='material_target_drift', invalidated_by_authority_provenance_id='remote-owner-resolution' WHERE grant_id='grant-1'",
                [],
            )
            .is_err());
        conn.execute(
            r#"
            INSERT INTO execass_authority_provenance (
              authority_provenance_id, actor_type, credential_identity,
              authenticated_ingress, channel_assurance, source_correlation_id,
              authority_kind, normalized_scope_json, policy_revision,
              bound_decision_id, bound_decision_revision, bound_manifest_digest,
              bound_challenge_nonce_digest, evidence_digest, created_at
            ) VALUES ('missing-challenge-resolution', 'human_local', 'owner', 'native-control',
                      'authenticated', 'missing-challenge-correlation', 'decision_resolution', '{}', 1,
                      'missing-challenge-decision', 2, 'manifest-1', 'nonce-missing',
                      'missing-challenge-evidence', 3)
            "#,
            [],
        )
        .expect("seed owner resolution without a challenge");
        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_decisions (
                  decision_id, delegation_id, decision_revision, delegation_revision,
                  plan_revision, policy_revision, decision_kind, status, result,
                  exact_presented_action_json, confirmed_logical_action_identity, manifest_digest, payload_digest,
                  payload_and_material_operands_json,
                  target_audience_path_json, side_effect_envelope_json, recommendation,
                  consequence, alternatives_json, idempotency_key, requested_at, resolved_at,
                  resolved_by_authority_provenance_id
                ) VALUES ('missing-challenge-decision', 'delegation-1', 2, 1, 1, 1,
                          'dangerous_action_confirmation', 'resolved', 'confirm_and_continue',
                          '{}', 'missing-logical-action', 'manifest-1', 'payload-1', '{}', '[]', '{}', 'continue',
                          'concrete consequence', '[]', 'missing-challenge-idempotency', 1, 3,
                          'missing-challenge-resolution')
                "#,
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO execass_technical_resource_reservations (reservation_id, delegation_id, technical_resource_kind, unit, amount_reserved, status, idempotency_key, fencing_token, created_at, expires_at) VALUES ('bad-resource', 'delegation-1', 'money', 'unit', 1, 'reserved', 'bad-resource-key', 1, 1, 2)",
                [],
            )
            .is_err());

        for forbidden_column in ["expires_at", "maximum_uses", "uses_consumed"] {
            assert!(
                !table_columns(&conn, "execass_accepted_confirmation_grants")
                    .iter()
                    .any(|column| column == forbidden_column),
                "durable accepted grant must not expose {forbidden_column}"
            );
        }
    }

    #[test]
    fn execass_technical_resource_requirement_schema_is_exact_and_immutable() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");
        seed_execass_constraint_fixture(&conn);
        conn.execute(
            r#"
            INSERT INTO execass_logical_effects (
              logical_effect_id, delegation_id, continuation_id, action_kind,
              state, internal_idempotency_key, manifest_digest, payload_digest,
              created_at, updated_at
            ) VALUES ('resource-effect', 'delegation-1', 'continuation-1',
                      'read_only_local_inspection_and_bounded_reversible_local_work',
                      'planned', 'resource-effect-key', 'manifest-1', 'payload-1', 1, 1)
            "#,
            [],
        )
        .expect("seed resource-bound logical effect");
        for (snapshot_id, entries_digest) in [("quota-1", "entries-1"), ("quota-2", "entries-2")] {
            conn.execute(
                r#"
                INSERT INTO execass_technical_resource_quota_snapshots (
                  quota_snapshot_id, delegation_id, policy_revision,
                  effective_authority_digest, scope_key, canonical_entries_json,
                  canonical_entries_digest, created_at
                ) VALUES (?1, 'delegation-1', 1, 'authority-digest', 'delegation',
                          '[{"kind":"tokens","unit":"token","limit":10}]', ?2, 1)
                "#,
                params![snapshot_id, entries_digest],
            )
            .expect("seed immutable shared quota snapshot");
            conn.execute(
                "INSERT INTO execass_technical_resource_quota_entries (quota_snapshot_id, technical_resource_kind, unit, amount_limit) VALUES (?1, 'tokens', 'token', 10)",
                params![snapshot_id],
            )
            .expect("seed token quota entry");
        }

        let lowercase_connector = format!("connector:{}", "a".repeat(64));
        conn.execute(
            "INSERT INTO execass_technical_resource_quota_entries (quota_snapshot_id, technical_resource_kind, unit, amount_limit) VALUES ('quota-1', 'connector_calls', ?1, 2)",
            params![lowercase_connector],
        )
        .expect("accept exact lowercase canonical connector unit");
        for (kind, unit) in [
            ("money", "unit".to_string()),
            ("connector_calls", format!("connector:{}B", "a".repeat(63))),
            ("connector_calls", format!("connector:{}g", "a".repeat(63))),
            ("resource_units", format!("resource:{}A", "a".repeat(63))),
        ] {
            assert!(
                conn.execute(
                    "INSERT INTO execass_technical_resource_quota_entries (quota_snapshot_id, technical_resource_kind, unit, amount_limit) VALUES ('quota-1', ?1, ?2, 1)",
                    params![kind, unit],
                )
                .is_err(),
                "invalid technical resource kind/unit must be rejected: {kind}"
            );
        }

        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_technical_resource_requirement_sets (
                  requirement_set_id, quota_snapshot_id, delegation_id,
                  logical_effect_id, action_id, manifest_digest,
                  canonical_requirements_json, canonical_requirements_digest, created_at
                ) VALUES ('bad-manifest-set', 'quota-1', 'delegation-1',
                          'resource-effect', 'seed-action', 'other-manifest',
                          '[]', 'bad-manifest-digest', 1)
                "#,
                [],
            )
            .is_err());
        conn.execute(
            r#"
            INSERT INTO execass_technical_resource_requirement_sets (
              requirement_set_id, quota_snapshot_id, delegation_id,
              logical_effect_id, action_id, manifest_digest,
              canonical_requirements_json, canonical_requirements_digest, created_at
            ) VALUES ('requirements-1', 'quota-1', 'delegation-1',
                      'resource-effect', 'seed-action', 'manifest-1',
                      '[{"kind":"tokens","unit":"token","amount":6}]',
                      'requirements-digest-1', 1)
            "#,
            [],
        )
        .expect("seed exact immutable requirement-set header");
        for (kind, unit) in [
            ("money", "token".to_string()),
            ("connector_calls", format!("connector:{}B", "a".repeat(63))),
        ] {
            assert!(conn
                .execute(
                    "INSERT INTO execass_technical_resource_requirements (requirement_set_id, quota_snapshot_id, technical_resource_kind, unit, amount_required) VALUES ('requirements-1', 'quota-1', ?1, ?2, 1)",
                    params![kind, unit],
                )
                .is_err());
        }
        assert!(conn
            .execute(
                "INSERT INTO execass_technical_resource_requirements (requirement_set_id, quota_snapshot_id, technical_resource_kind, unit, amount_required) VALUES ('requirements-1', 'quota-2', 'tokens', 'token', 6)",
                [],
            )
            .is_err());
        conn.execute(
            "INSERT INTO execass_technical_resource_requirements (requirement_set_id, quota_snapshot_id, technical_resource_kind, unit, amount_required) VALUES ('requirements-1', 'quota-1', 'tokens', 'token', 6)",
            [],
        )
        .expect("seed exact immutable requirement row");

        let agent_id = conn
            .query_row(
                "SELECT agent_id FROM agents ORDER BY agent_id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("load seeded agent for canonical jobs");
        conn.execute(
            "INSERT INTO execass_runtime_host_generations (generation, ownership_scope, state_root_generation, installation_identity, os_user_identity_digest, host_instance_id, started_at) VALUES (1, 'execass', 1, 'installation-1', 'user-1', 'host-1', 1)",
            [],
        )
        .expect("seed runtime host generation");
        conn.execute(
            "INSERT INTO jobs (job_id, agent_id, name, enabled, schedule_kind, payload_json, max_retries, retry_backoff_ms, timeout_ms, created_at, updated_at) VALUES ('resource-job-1', ?1, 'resource job 1', 1, 'manual', '{}', 0, 0, 1000, 1, 1)",
            params![agent_id],
        )
        .expect("seed first canonical job");
        conn.execute(
            "INSERT INTO execass_outbox_events (event_id, event_name, aggregate_id, aggregate_revision, correlation_id, causation_id, occurred_at, schema_version, safe_payload_json, duplicate_identity) VALUES ('resource-claim-1', 'execass.v1.continuation.claimed_or_result_recorded', 'delegation-1', 1, 'resource-correlation-1', 'continuation-1', 1, 'v1', '{}', 'resource-claim-1')",
            [],
        )
        .expect("seed first claim event");
        conn.execute(
            r#"
            INSERT INTO execass_continuation_operation_history (
              event_id, claim_event_id, claim_receipt_id, operation, result_status,
              continuation_id, delegation_id, action_id, job_id, worker_id,
              job_lease_expires_at, continuation_fencing_token,
              runtime_host_generation, runtime_host_instance_id, runtime_fencing_token,
              state_root_generation, runtime_authority_provenance_id,
              runtime_actor_identity, policy_revision, global_stop_epoch,
              technical_quota_policy_digest, technical_quota_snapshot_id,
              technical_resource_reservation_set_json,
              technical_resource_reservation_set_digest, recorded_at
            ) VALUES ('resource-claim-1', 'resource-claim-1', 'resource-receipt-1',
                      'claim', 'executing', 'continuation-1', 'delegation-1',
                      'seed-action', 'resource-job-1', 'worker-1', 100, 1,
                      1, 'host-1', 1, 1, 'authority-1', 'runtime-1', 1, 0,
                      'quota-policy-1', 'quota-1', '[]', 'reservation-set-1', 1)
            "#,
            [],
        )
        .expect("seed exact first claim provenance");
        for (snapshot_id, amount) in [("quota-2", 6), ("quota-1", 5)] {
            assert!(
                conn.execute(
                    r#"
                    INSERT INTO execass_technical_resource_reservations (
                      reservation_id, delegation_id, logical_effect_id,
                      quota_snapshot_id, continuation_id, claim_event_id,
                      claim_receipt_id, technical_resource_kind, unit,
                      amount_reserved, status, idempotency_key,
                      continuation_fencing_token, runtime_host_generation,
                      runtime_fencing_token, created_at, expires_at
                    ) VALUES (?1, 'delegation-1', 'resource-effect', ?2,
                              'continuation-1', 'resource-claim-1', 'resource-receipt-1',
                              'tokens', 'token', ?3, 'reserved', ?1, 1, 1, 1, 1, 100)
                    "#,
                    params![
                        format!("mismatch-{snapshot_id}-{amount}"),
                        snapshot_id,
                        amount
                    ],
                )
                .is_err(),
                "reservation must match the exact claim snapshot and required amount"
            );
        }
        conn.execute(
            r#"
            INSERT INTO execass_technical_resource_reservations (
              reservation_id, delegation_id, logical_effect_id, quota_snapshot_id,
              continuation_id, claim_event_id, claim_receipt_id,
              technical_resource_kind, unit, amount_reserved, status, idempotency_key,
              continuation_fencing_token, runtime_host_generation,
              runtime_fencing_token, created_at, expires_at
            ) VALUES ('reservation-1', 'delegation-1', 'resource-effect', 'quota-1',
                      'continuation-1', 'resource-claim-1', 'resource-receipt-1',
                      'tokens', 'token', 6, 'reserved', 'reservation-1', 1, 1, 1, 1, 100)
            "#,
            [],
        )
        .expect("reserve exact first-effect requirement");

        conn.execute_batch(
            r#"
            INSERT INTO execass_plans (
              plan_id, delegation_id, plan_revision, based_on_delegation_revision,
              policy_revision, plan_summary, resolved_leaf_manifest_json,
              manifest_digest, created_by_authority_provenance_id, created_at
            ) VALUES ('resource-plan-2', 'delegation-1', 2, 2, 1,
                      'resource plan 2', '[]', 'manifest-2', 'authority-1', 2);
            INSERT INTO execass_action_branches (
              action_id, delegation_id, action_revision, target_delegation_revision,
              target_plan_revision, stop_epoch, branch_kind, status, action_summary,
              created_at, updated_at
            ) VALUES ('resource-action-2', 'delegation-1', 2, 2, 2, 0,
                      'ordinary', 'waiting', 'resource action 2', 2, 2);
            INSERT INTO execass_continuations (
              continuation_id, delegation_id, target_delegation_revision,
              target_plan_revision, action_id, branch_kind, causation_kind,
              causation_id, status, fencing_token, host_generation,
              stop_epoch, global_stop_epoch, created_at, updated_at
            ) VALUES ('resource-continuation-2', 'delegation-1', 2, 2,
                      'resource-action-2', 'ordinary', 'plan', 'resource-plan-2',
                      'waiting', 0, 1, 0, 0, 2, 2);
            INSERT INTO execass_logical_effects (
              logical_effect_id, delegation_id, continuation_id, action_kind,
              state, internal_idempotency_key, manifest_digest, payload_digest,
              created_at, updated_at
            ) VALUES ('resource-effect-2', 'delegation-1', 'resource-continuation-2',
                      'read_only_local_inspection_and_bounded_reversible_local_work',
                      'planned', 'resource-effect-key-2', 'manifest-2', 'payload-2', 2, 2);
            INSERT INTO execass_technical_resource_requirement_sets (
              requirement_set_id, quota_snapshot_id, delegation_id,
              logical_effect_id, action_id, manifest_digest,
              canonical_requirements_json, canonical_requirements_digest, created_at
            ) VALUES ('requirements-2', 'quota-1', 'delegation-1',
                      'resource-effect-2', 'resource-action-2', 'manifest-2',
                      '[{"kind":"tokens","unit":"token","amount":5}]',
                      'requirements-digest-2', 2);
            INSERT INTO execass_technical_resource_requirements (
              requirement_set_id, quota_snapshot_id, technical_resource_kind,
              unit, amount_required
            ) VALUES ('requirements-2', 'quota-1', 'tokens', 'token', 5);
            "#,
        )
        .expect("seed second effect sharing the immutable quota snapshot");
        conn.execute(
            "INSERT INTO jobs (job_id, agent_id, name, enabled, schedule_kind, payload_json, max_retries, retry_backoff_ms, timeout_ms, created_at, updated_at) VALUES ('resource-job-2', ?1, 'resource job 2', 1, 'manual', '{}', 0, 0, 1000, 2, 2)",
            params![agent_id],
        )
        .expect("seed second canonical job");
        conn.execute(
            "INSERT INTO execass_outbox_events (event_id, event_name, aggregate_id, aggregate_revision, correlation_id, causation_id, occurred_at, schema_version, safe_payload_json, duplicate_identity) VALUES ('resource-claim-2', 'execass.v1.continuation.claimed_or_result_recorded', 'delegation-1', 2, 'resource-correlation-2', 'resource-continuation-2', 2, 'v1', '{}', 'resource-claim-2')",
            [],
        )
        .expect("seed second claim event");
        conn.execute(
            r#"
            INSERT INTO execass_continuation_operation_history (
              event_id, claim_event_id, claim_receipt_id, operation, result_status,
              continuation_id, delegation_id, action_id, job_id, worker_id,
              job_lease_expires_at, continuation_fencing_token,
              runtime_host_generation, runtime_host_instance_id, runtime_fencing_token,
              state_root_generation, runtime_authority_provenance_id,
              runtime_actor_identity, policy_revision, global_stop_epoch,
              technical_quota_policy_digest, technical_quota_snapshot_id,
              technical_resource_reservation_set_json,
              technical_resource_reservation_set_digest, recorded_at
            ) VALUES ('resource-claim-2', 'resource-claim-2', 'resource-receipt-2',
                      'claim', 'executing', 'resource-continuation-2', 'delegation-1',
                      'resource-action-2', 'resource-job-2', 'worker-2', 100, 1,
                      1, 'host-1', 1, 1, 'authority-1', 'runtime-1', 1, 0,
                      'quota-policy-1', 'quota-1', '[]', 'reservation-set-2', 2)
            "#,
            [],
        )
        .expect("seed exact second claim provenance");
        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_technical_resource_reservations (
                  reservation_id, delegation_id, logical_effect_id, quota_snapshot_id,
                  continuation_id, claim_event_id, claim_receipt_id,
                  technical_resource_kind, unit, amount_reserved, status, idempotency_key,
                  continuation_fencing_token, runtime_host_generation,
                  runtime_fencing_token, created_at, expires_at
                ) VALUES ('reservation-2', 'delegation-1', 'resource-effect-2', 'quota-1',
                          'resource-continuation-2', 'resource-claim-2', 'resource-receipt-2',
                          'tokens', 'token', 5, 'reserved', 'reservation-2', 1, 1, 1, 2, 100)
                "#,
                [],
            )
            .is_err());
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE quota_snapshot_id='quota-1' AND technical_resource_kind='tokens' AND unit='token'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("count shared-snapshot reservations"),
            1,
            "capacity must be shared by snapshot/kind/unit across logical effects"
        );

        for statement in [
            "UPDATE execass_technical_resource_requirement_sets SET manifest_digest='mutated' WHERE requirement_set_id='requirements-1'",
            "DELETE FROM execass_technical_resource_requirement_sets WHERE requirement_set_id='requirements-1'",
            "UPDATE execass_technical_resource_requirements SET amount_required=7 WHERE requirement_set_id='requirements-1'",
            "DELETE FROM execass_technical_resource_requirements WHERE requirement_set_id='requirements-1'",
        ] {
            assert!(conn.execute(statement, []).is_err(), "{statement}");
        }
    }

    #[test]
    fn execass_v11_local_confirmation_and_challenge_expiry_are_exact() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");
        seed_execass_constraint_fixture(&conn);
        let insert_decision = |decision_id: &str, revision: i64| {
            conn.execute(
                r#"
                INSERT INTO execass_decisions (
                  decision_id, delegation_id, decision_revision, delegation_revision,
                  plan_revision, policy_revision, decision_kind, status,
                  exact_presented_action_json, confirmed_logical_action_identity, manifest_digest,
                  payload_digest, payload_and_material_operands_json, target_audience_path_json,
                  side_effect_envelope_json, recommendation, consequence, alternatives_json,
                  idempotency_key, requested_at
                ) VALUES (?1, 'delegation-1', ?2, 1, 1, 1,
                          'dangerous_action_confirmation', 'pending', '{}', ?1, 'manifest-1',
                          'payload-1', '{}', '[]', '{}', 'continue', 'concrete consequence',
                          '["confirm_and_continue","revise","decline"]', ?1, 1)
                "#,
                params![decision_id, revision],
            )
        };
        let insert_challenge =
            |challenge_id: &str, decision_id: &str, revision: i64, expires_at: i64| {
                conn.execute(
                    r#"
                INSERT INTO execass_confirmation_challenges (
                  challenge_id, decision_id, delegation_id, decision_revision,
                  exact_presented_action_json, confirmed_logical_action_identity, manifest_digest,
                  payload_digest, payload_and_material_operands_json,
                  canonical_action_envelope_or_selector_json, declared_consequence,
                  nonce_digest, status, created_at, expires_at
                ) VALUES (?1, ?2, 'delegation-1', ?3, '{}', ?2, 'manifest-1',
                          'payload-1', '{}', '{}', 'concrete consequence', ?1, 'pending', 1, ?4)
                "#,
                    params![challenge_id, decision_id, revision, expires_at],
                )
            };
        let insert_local_resolution = |provenance_id: &str,
                                       decision_id: &str,
                                       revision: i64,
                                       nonce: &str,
                                       created_at: i64,
                                       expires_at: i64| {
            conn.execute(
                r#"
                INSERT INTO execass_authority_provenance (
                  authority_provenance_id, actor_type, credential_identity,
                  authenticated_ingress, channel_assurance, source_correlation_id,
                  authority_kind, normalized_scope_json, policy_revision,
                  bound_decision_id, bound_decision_revision, bound_manifest_digest,
                  bound_challenge_nonce_digest, evidence_digest, created_at, expires_at
                ) VALUES (?1, 'human_local', 'owner', 'native-control', 'authenticated', ?1,
                          'decision_resolution', '{}', 1, ?2, ?3, 'manifest-1', ?4, ?1, ?5, ?6)
                "#,
                params![
                    provenance_id,
                    decision_id,
                    revision,
                    nonce,
                    created_at,
                    expires_at
                ],
            )
        };

        insert_decision("local-decision", 1).expect("seed local decision");
        insert_challenge("local-challenge", "local-decision", 1, 10).expect("seed local challenge");
        conn.execute(
            r#"
            INSERT INTO execass_confirmation_challenge_alternatives (
              challenge_id, logical_action_id, exact_presented_action_json,
              confirmed_logical_action_identity, manifest_digest, payload_digest,
              payload_and_material_operands_json, target_audience_path_json,
              canonical_action_envelope_or_selector_json, declared_consequence
            ) VALUES ('local-challenge', 'local-decision', '{}', 'local-decision',
                      'manifest-1', 'payload-1', '{}', '[]', '{}', 'concrete consequence')
            "#,
            [],
        )
        .expect("seed exact disclosed local alternative");
        conn.execute(
            r#"
            INSERT INTO execass_confirmation_authority_keys (
              key_id, key_generation, verifying_key_hex, verifying_key_digest,
              canonical_root_identity, installation_identity, os_user_identity_digest,
              state_root_generation, status, created_at
            ) VALUES ('local-key', 1, ?1, ?2, ?3, 'local-install', ?4, 1, 'active', 1)
            "#,
            params![
                "1".repeat(64),
                "2".repeat(64),
                format!("sha256:{}", "3".repeat(64)),
                "4".repeat(64)
            ],
        )
        .expect("seed pinned local confirmation key");
        conn.execute(
            "INSERT INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES ('local-owner-binding','human_local','owner','native-control','authenticated',0,'active',1)",
            [],
        )
        .expect("seed local owner ingress binding");
        let local_resolution = "6".repeat(64);
        insert_local_resolution(
            &local_resolution,
            "local-decision",
            1,
            "local-challenge",
            4,
            10,
        )
        .expect("seed local owner resolution");
        conn.execute(
            r#"
            INSERT INTO execass_confirmation_attestations (
              attestation_digest, decision_id, authority_provenance_id,
              pinned_key_id, pinned_key_generation, actor_type, credential_identity,
              authenticated_ingress, channel_assurance, request_correlation_id,
              selected_logical_action_id, signed_payload_json, signature_hex,
              issued_at, expires_at, verified_at
            ) VALUES (
              ?3, 'local-decision', ?3,
              'local-key', 1, 'human_local', 'owner', 'native-control', 'authenticated',
              ?3, 'local-decision',
              json_object(
                'actor_type','human_local',
                'credential_identity','owner',
                'authenticated_ingress','native-control',
                'channel_assurance','authenticated',
                'request_correlation_id',?3,
                'source_message_id',NULL,
                'provider_event_id',NULL,
                'decision_id','local-decision',
                'decision_revision',1,
                'decision_result','confirm_and_continue',
                'policy_revision',1,
                'canonical_manifest_digest','manifest-1',
                'selected_logical_action_id','local-decision',
                'challenge_nonce_digest','local-challenge',
                'challenge_expires_at_ms',10,
                'issued_at_ms',4,
                'canonical_root_identity',?1,
                'installation_identity','local-install',
                'os_user_identity_digest',?2,
                'state_root_generation',1,
                'signer_key_generation',1
              ),
              ?4, 4, 10, 5
            )
            "#,
            params![
                format!("sha256:{}", "3".repeat(64)),
                "4".repeat(64),
                local_resolution,
                "5".repeat(128)
            ],
        )
        .expect("seed structurally attested local resolution");
        conn.execute(
            "UPDATE execass_confirmation_challenges SET selected_logical_action_id='local-decision', status='resolved', resolved_at=5 WHERE challenge_id='local-challenge'",
            [],
        )
        .expect("local challenge resolves before expiry");
        conn.execute(
            "UPDATE execass_decisions SET status='resolved', result='confirm_and_continue', resolved_at=5, resolved_by_authority_provenance_id=?1 WHERE decision_id='local-decision'",
            params![local_resolution],
        )
        .expect("local owner resolves dangerous decision");

        insert_decision("expired-decision", 2).expect("seed expiry decision");
        insert_challenge("expired-challenge", "expired-decision", 2, 5)
            .expect("seed expiry challenge");
        conn.execute(
            "UPDATE execass_confirmation_challenges SET status='expired' WHERE challenge_id='expired-challenge'",
            [],
        )
        .expect("pending challenge may expire once");
        assert!(conn
            .execute(
                "UPDATE execass_confirmation_challenges SET status='resolved', resolved_at=4 WHERE challenge_id='expired-challenge'",
                [],
            )
            .is_err());

        insert_decision("late-decision", 3).expect("seed late decision");
        insert_challenge("late-challenge", "late-decision", 3, 5).expect("seed late challenge");
        insert_local_resolution(
            "late-resolution",
            "late-decision",
            3,
            "late-challenge",
            6,
            7,
        )
        .expect("seed too-late owner resolution");
        assert!(conn
            .execute(
                "UPDATE execass_confirmation_challenges SET status='resolved', resolved_at=5 WHERE challenge_id='late-challenge'",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE execass_decisions SET status='resolved', result='confirm_and_continue', resolved_at=5, resolved_by_authority_provenance_id='late-resolution' WHERE decision_id='late-decision'",
                [],
            )
            .is_err());
    }

    #[test]
    fn execass_schema_rejects_duplicate_continuation_effect_and_outbox_identities() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");
        seed_execass_constraint_fixture(&conn);
        conn.execute(
            r#"
            INSERT INTO execass_plans (
              plan_id, delegation_id, plan_revision, based_on_delegation_revision,
              policy_revision, plan_summary, resolved_leaf_manifest_json,
              manifest_digest, created_by_authority_provenance_id, created_at
            ) VALUES ('plan-2', 'delegation-1', 2, 2, 1, 'revised plan', '[]',
                      'manifest-2', 'authority-1', 2)
            "#,
            [],
        )
        .expect("seed second plan revision");

        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_continuations (
                  continuation_id, delegation_id, target_delegation_revision,
                  target_plan_revision, action_id, branch_kind, causation_kind, causation_id, status,
                  fencing_token, host_generation, stop_epoch, global_stop_epoch, created_at, updated_at
                ) VALUES ('continuation-duplicate', 'delegation-1', 2, 2, 'seed-action-duplicate', 'ordinary', 'plan',
                          'plan-1', 'runnable', 0, 1, 0, 0, 1, 1)
                "#,
                [],
            )
            .is_err());

        conn.execute(
            r#"
            INSERT INTO execass_logical_effects (
              logical_effect_id, delegation_id, continuation_id, action_kind,
              state, internal_idempotency_key, manifest_digest, payload_digest,
              created_at, updated_at
            ) VALUES ('effect-1', 'delegation-1', 'continuation-1',
                      'read_only_local_inspection_and_bounded_reversible_local_work',
                      'planned', 'effect-key-1', 'manifest-1', 'payload-1', 1, 1)
            "#,
            [],
        )
        .expect("seed logical effect");
        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_logical_effects (
                  logical_effect_id, delegation_id, continuation_id, action_kind,
                  state, internal_idempotency_key, manifest_digest, payload_digest,
                  created_at, updated_at
                ) VALUES ('effect-2', 'delegation-1', 'continuation-1',
                          'read_only_local_inspection_and_bounded_reversible_local_work',
                          'planned', 'effect-key-1', 'manifest-2', 'payload-2', 1, 1)
                "#,
                [],
            )
            .is_err());

        conn.execute(
            r#"
            INSERT INTO execass_outbox_events (
              global_sequence, event_id, event_name, aggregate_id,
              aggregate_revision, correlation_id, causation_id, occurred_at,
              schema_version, safe_payload_json, duplicate_identity
            ) VALUES (1, 'event-1', 'execass.v1.delegation.transitioned',
                      'delegation-1', 1, 'correlation-1', 'plan-1', 1,
                      'v1', '{}', 'duplicate-1')
            "#,
            [],
        )
        .expect("seed outbox event");
        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_outbox_events (
                  global_sequence, event_id, event_name, aggregate_id,
                  aggregate_revision, correlation_id, causation_id, occurred_at,
                  schema_version, safe_payload_json, duplicate_identity
                ) VALUES (1, 'event-2', 'execass.v1.summary.changed',
                          'delegation-1', 2, 'correlation-2', 'plan-2', 2,
                          'v1', '{}', 'duplicate-2')
                "#,
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                r#"
                INSERT INTO execass_outbox_events (
                  event_id, event_name, aggregate_id, aggregate_revision,
                  correlation_id, causation_id, occurred_at, schema_version,
                  safe_payload_json, duplicate_identity
                ) VALUES ('event-3', 'execass.v1.summary.changed', 'delegation-1', 3,
                          'correlation-3', 'plan-3', 3, 'v1', '{}', 'duplicate-1')
                "#,
                [],
            )
            .is_err());
    }

    #[test]
    fn execass_outcome_unknown_blocks_retry_and_raw_reconciliation_without_signed_evidence() {
        let (_temp_dir, paths) = execass_test_root();
        let conn = open_sqlite_connection(&paths.db_path).expect("open ExecAss database");
        seed_execass_constraint_fixture(&conn);
        conn.execute(
            r#"
            INSERT INTO execass_logical_effects (
              logical_effect_id, delegation_id, continuation_id, action_kind,
              state, internal_idempotency_key, provider_identity,
              provider_idempotency_key, reconciliation_key, manifest_digest,
              payload_digest, created_at, updated_at
            ) VALUES ('unknown-effect', 'delegation-1', 'continuation-1',
                      'public_or_externally_consequential_communication',
                      'invoking', 'unknown-effect-key', 'unknown-provider',
                      'unknown-provider-idempotency', 'unknown-reconciliation',
                      'manifest-1', 'payload-1', 1, 1)
            "#,
            [],
        )
        .expect("seed invoking logical effect");
        let agent_id = conn
            .query_row(
                "SELECT agent_id FROM agents ORDER BY agent_id LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("load canonical agent");
        conn.execute(
            "INSERT INTO execass_runtime_host_generations (generation, ownership_scope, state_root_generation, installation_identity, os_user_identity_digest, host_instance_id, started_at) VALUES (1, 'execass', 1, 'installation-1', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 'host-1', 1)",
            [],
        )
        .expect("seed exact runtime generation");
        conn.execute(
            "INSERT INTO jobs (job_id, agent_id, name, enabled, schedule_kind, payload_json, max_retries, retry_backoff_ms, timeout_ms, created_at, updated_at) VALUES ('unknown-job', ?1, 'unknown job', 1, 'manual', '{}', 0, 0, 1000, 1, 1)",
            params![agent_id],
        )
        .expect("seed exact attempt job");
        conn.execute(
            "INSERT INTO execass_outbox_events (event_id, event_name, aggregate_id, aggregate_revision, correlation_id, causation_id, occurred_at, schema_version, safe_payload_json, duplicate_identity) VALUES ('unknown-claim', 'execass.v1.continuation.claimed_or_result_recorded', 'delegation-1', 1, 'unknown-correlation', 'continuation-1', 1, 'v1', '{}', 'unknown-claim')",
            [],
        )
        .expect("seed exact claim event");
        conn.execute(
            r#"
            INSERT INTO execass_continuation_operation_history (
              event_id, claim_event_id, claim_receipt_id, operation, result_status,
              continuation_id, delegation_id, action_id, job_id, worker_id,
              job_lease_expires_at, continuation_fencing_token,
              runtime_host_generation, runtime_host_instance_id, runtime_fencing_token,
              state_root_generation, runtime_authority_provenance_id,
              runtime_actor_identity, policy_revision, global_stop_epoch,
              technical_quota_policy_digest, technical_resource_reservation_set_json,
              technical_resource_reservation_set_digest, recorded_at
            ) VALUES ('unknown-claim', 'unknown-claim', 'unknown-receipt',
                      'claim', 'executing', 'continuation-1', 'delegation-1',
                      'seed-action', 'unknown-job', 'unknown-worker', 100, 1,
                      1, 'host-1', 1, 1, 'authority-1', 'runtime-1', 1, 0,
                      'quota-policy-1', '[]', 'reservation-set-1', 1)
            "#,
            [],
        )
        .expect("seed exact claim ancestry");
        conn.execute(
            r#"
            INSERT INTO execass_provider_attempts (
              attempt_id, delegation_id, logical_effect_id, continuation_id, action_id,
              claim_event_id, claim_receipt_id, attempt_number, fencing_token,
              host_generation, host_instance_id, runtime_fencing_token, status,
              provider_request_digest, provider_response_digest, started_at, finished_at
            ) VALUES ('attempt-1', 'delegation-1', 'unknown-effect', 'continuation-1',
                      'seed-action', 'unknown-claim', 'unknown-receipt', 1, 1, 1,
                      'host-1', 1, 'invoking',
                      'sha256:2e91d232ca0387d09e48f86b54e80d9d64e9142ed7f0b606a013934b507d2f46',
                      NULL, 1, NULL)
            "#,
            [],
        )
        .expect("record first ambiguous attempt");
        execass::seed_signed_execution_unknown_fixture(&conn, "attempt-1", "unknown-effect", 2)
            .expect("mark logical effect outcome unknown through signed recorder evidence");

        let retry_sql = r#"
            INSERT INTO execass_provider_attempts (
              attempt_id, delegation_id, logical_effect_id, continuation_id, action_id,
              claim_event_id, claim_receipt_id, attempt_number, fencing_token,
              host_generation, host_instance_id, runtime_fencing_token, status,
              provider_request_digest, started_at
            ) VALUES ('attempt-2', 'delegation-1', 'unknown-effect', 'continuation-1',
                      'seed-action', 'unknown-claim', 'unknown-receipt', 2, 1, 1,
                      'host-1', 1, 'prepared', 'request-2', 2)
        "#;
        assert!(conn.execute(retry_sql, []).is_err());

        assert!(conn
            .execute(
                "UPDATE execass_logical_effects SET state='reconciled_absent', updated_at=3 WHERE logical_effect_id='unknown-effect'",
                [],
            )
            .is_err());
        assert_eq!(
            conn.query_row(
                "SELECT state FROM execass_logical_effects WHERE logical_effect_id='unknown-effect'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "outcome_unknown"
        );
        assert!(conn.execute(retry_sql, []).is_err());
    }

    #[test]
    fn execass_initializer_rejects_legacy_database_without_mutation() {
        let temp_dir = project_local_tempdir();
        let paths = AppPaths::from_root(temp_dir.path().join("legacy-state"));
        init(&paths).expect("initialize legacy database");
        let before = std::fs::read(&paths.db_path).expect("read legacy database before");

        let error = init_execass_fresh_root(&paths).expect_err("legacy database must be rejected");
        assert!(error.to_string().contains("pre-existing database"));
        let after = std::fs::read(&paths.db_path).expect("read legacy database after");
        assert_eq!(
            after, before,
            "rejected legacy database must remain byte-identical"
        );
    }

    #[test]
    fn execass_store_discovery_distinguishes_legacy_from_claimed_but_tampered_root() {
        let legacy_temp = project_local_tempdir();
        let legacy_paths = AppPaths::from_root(legacy_temp.path().join("legacy-discovery"));
        init(&legacy_paths).expect("initialize legacy discovery root");
        assert!(execass::ExecAssStore::open_if_canonical_root(&legacy_paths)
            .expect("legacy discovery is non-fatal")
            .is_none());

        let (_exact_temp, exact_paths) = execass_test_root();
        assert!(execass::ExecAssStore::open_if_canonical_root(&exact_paths)
            .expect("discover exact replacement root")
            .is_some());
        let conn = open_sqlite_connection(&exact_paths.db_path).expect("open exact root to tamper");
        conn.execute_batch("DROP TRIGGER execass_outcome_unknown_attempt_prohibition")
            .expect("tamper claimed replacement root");
        drop(conn);
        assert!(execass::ExecAssStore::open_if_canonical_root(&exact_paths).is_err());
    }

    #[test]
    fn execass_initializer_rejects_nonempty_state_root_without_creating_database() {
        let temp_dir = project_local_tempdir();
        let paths = AppPaths::from_root(temp_dir.path().join("occupied-state"));
        std::fs::create_dir_all(&paths.root).expect("create occupied state root");
        let marker = paths.root.join("existing-data.bin");
        std::fs::write(&marker, b"do not mutate").expect("seed existing state-root data");

        let error = init_execass_fresh_root(&paths).expect_err("occupied root must be rejected");
        assert!(error.to_string().contains("non-empty state root"));
        assert_eq!(
            std::fs::read(&marker).expect("read preserved state-root data"),
            b"do not mutate"
        );
        assert!(!paths.db_path.exists());
    }

    #[test]
    fn execass_initializer_is_idempotent_only_for_exact_installed_schema() {
        let (_temp_dir, paths) = execass_test_root();
        init_execass_fresh_root(&paths).expect("second exact initialization");

        let conn = open_sqlite_connection(&paths.db_path).expect("open exact ExecAss database");
        conn.execute_batch("DROP TRIGGER execass_outcome_unknown_attempt_prohibition")
            .expect("tamper exact trigger inventory");
        drop(conn);

        let error =
            init_execass_fresh_root(&paths).expect_err("incomplete schema must be rejected");
        assert!(error.to_string().contains("pre-existing database"));
    }

    #[test]
    fn execass_v7_root_upgrades_atomically_to_the_exact_v8_office_schema() {
        let temp_dir = project_local_tempdir();
        let paths = AppPaths::from_root(temp_dir.path().join("v7-upgrade"));
        ensure_dirs(&paths).expect("create upgrade root");
        let mut conn = open_sqlite_connection(&paths.db_path).expect("open v7 database");
        install_execass_schema(&mut conn, MIGRATION_0007).expect("install canonical v7 schema");
        drop(conn);

        upgrade_execass_canonical_root_if_needed(&paths).expect("upgrade canonical v7 root");
        let conn = open_sqlite_connection(&paths.db_path).expect("open upgraded database");
        verify_execass_schema(&conn).expect("v8 schema remains canonical");
        assert!(conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_chatter_messages'",
                [],
                |_| Ok(()),
            )
            .optional()
            .expect("query projection table")
            .is_some());
        assert_eq!(
            conn.query_row(
                "SELECT last_global_sequence FROM office_chatter_producer_cursor",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("read seeded producer cursor"),
            0
        );
    }

    #[test]
    fn office_chatter_producer_is_allowlisted_deduplicated_and_payload_blind() {
        let (_temp_dir, paths) = execass_test_root();
        let storage = Storage::from_paths(&paths);
        let conn = open_sqlite_connection(&paths.db_path).expect("open office projection root");
        let events = [
            ("transition", "execass.v1.delegation.transitioned"),
            (
                "continuation",
                "execass.v1.continuation.claimed_or_result_recorded",
            ),
            ("recovery", "execass.v1.recovery.updated"),
            ("completion", "execass.v1.completion.assessed"),
            ("skipped", "execass.v1.summary.changed"),
        ];
        for (index, (suffix, event_name)) in events.iter().enumerate() {
            conn.execute(
                "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,?2,'execass-global-control-carrier',?3,?4,?5,?6,'v1',?7,?8)",
                params![
                    format!("office-event-{suffix}"),
                    event_name,
                    i64::try_from(index + 1).expect("small revision"),
                    format!("office-correlation-{suffix}"),
                    format!("office-causation-{suffix}"),
                    10_000_i64 + i64::try_from(index).expect("small timestamp"),
                    r#"{"summary":"SECRET raw tool output must never be copied"}"#,
                    format!("office-duplicate-{suffix}"),
                ],
            )
            .expect("seed office source event");
        }
        drop(conn);

        let produced = storage
            .produce_office_chatter(32)
            .expect("produce safe chatter");
        assert_eq!(produced.len(), 4);
        assert!(produced
            .iter()
            .all(|message| !message.body_text.contains("SECRET")));
        assert!(storage
            .produce_office_chatter(32)
            .expect("replay producer")
            .is_empty());

        let (rooms, messages) = storage
            .list_office_chatter(10, 10)
            .expect("list safe chatter");
        assert_eq!(rooms.len(), 1);
        assert_eq!(messages.len(), 4);
        assert!(messages
            .iter()
            .all(|message| message.source_kind == "execass_event"));

        let owner = storage
            .create_office_chatter_owner_message(
                &rooms[0].thread_id,
                "authenticated-owner",
                "  Keep this safe and short.  ",
            )
            .expect("create owner message")
            .expect("office room exists");
        assert_eq!(owner.source_kind, "owner_message");
        let canonical = storage
            .get_agent_mail_message(&owner.message_id)
            .expect("load canonical Agent Mail message")
            .expect("owner message exists");
        assert_eq!(canonical.sender_principal, "authenticated-owner");
        assert_eq!(canonical.metadata_json, None);
    }

    #[test]
    fn office_floor_presence_never_infers_offline_from_silence() {
        let (_temp_dir, paths) = execass_test_root();
        let storage = Storage::from_paths(&paths);
        storage
            .create_agent(NewAgent {
                agent_id: "reef-agent".to_string(),
                name: "Reef Agent".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: None,
                memory_binding: None,
            })
            .expect("create presence agent");
        let initial = storage
            .list_office_floor_presence()
            .expect("list unknown presence");
        let initial_agent = initial
            .iter()
            .find(|item| item.agent_id == "reef-agent")
            .expect("presence agent exists");
        assert_eq!(initial_agent.state, "unknown");

        let session = storage
            .create_session(NewSession {
                session_key: None,
                agent_id: "reef-agent".to_string(),
                title: Some("must not leak into presence".to_string()),
            })
            .expect("create presence session");
        let idle = storage
            .list_office_floor_presence()
            .expect("list idle presence");
        assert_eq!(
            idle.iter()
                .find(|item| item.agent_id == "reef-agent")
                .expect("idle agent exists")
                .state,
            "idle"
        );

        let run = storage
            .create_run(NewRun {
                session_id: session.session_id,
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create presence run")
            .expect("presence run exists");
        storage.mark_run_started(&run.run_id).expect("start presence run");
        let busy = storage
            .list_office_floor_presence()
            .expect("list busy presence");
        let busy_agent = busy
            .iter()
            .find(|item| item.agent_id == "reef-agent")
            .expect("busy agent exists");
        assert_eq!(busy_agent.state, "busy");
        assert_eq!(busy_agent.target_run_id.as_deref(), Some(run.run_id.as_str()));
    }

    #[test]
    fn execass_schema_install_rolls_back_every_migration_on_failure() {
        let temp_dir = project_local_tempdir();
        let db_path = temp_dir.path().join("intentional-schema-failure.db");
        let mut conn = open_sqlite_connection(&db_path).expect("open failure database");
        let invalid_execass_sql = format!("{MIGRATION_0007}\nTHIS IS INTENTIONALLY INVALID SQL;");

        install_execass_schema(&mut conn, &invalid_execass_sql)
            .expect_err("intentional schema failure must roll back");

        let application_tables = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'index', 'trigger')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("count rolled-back objects");
        assert_eq!(application_tables, 0);
        assert_eq!(
            conn.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
                .expect("rolled-back application_id"),
            0
        );
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("rolled-back user_version"),
            0
        );
    }

    #[test]
    fn remove_agent_deletes_unreferenced_agent() {
        let (_temp_dir, storage) = test_storage();
        let created = storage
            .create_agent(NewAgent {
                agent_id: "Delete-Me".to_string(),
                name: "Delete Me".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: None,
                memory_binding: None,
            })
            .expect("create removable agent");
        assert_eq!(created.agent_id, "Delete-Me");

        let removed = storage.remove_agent("Delete-Me").expect("remove agent");
        assert_eq!(removed, RemoveAgentOutcome::Removed);
        assert!(storage
            .get_agent("Delete-Me")
            .expect("reload removed agent")
            .is_none());
    }

    #[test]
    fn remove_agent_archives_agent_with_sessions() {
        let (_temp_dir, storage) = test_storage();
        let created = storage
            .create_agent(NewAgent {
                agent_id: "has-session".to_string(),
                name: "Has Session".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: None,
                memory_binding: None,
            })
            .expect("create session-bound agent");
        storage
            .create_session(NewSession {
                session_key: Some("session-bound-agent".to_string()),
                agent_id: created.agent_id.clone(),
                title: Some("Session Bound".to_string()),
            })
            .expect("create blocking session");

        let outcome = storage
            .remove_agent(&created.agent_id)
            .expect("remove should return outcome");
        assert_eq!(outcome, RemoveAgentOutcome::Removed);
        assert!(storage
            .get_agent(&created.agent_id)
            .expect("reload removed agent")
            .is_none());
        let retained_session = storage
            .get_session_by_key("session-bound-agent")
            .expect("reload archived session")
            .expect("session remains");
        assert_eq!(retained_session.agent_id, created.agent_id);
        assert!(storage
            .create_session(NewSession {
                session_key: Some("new-session-for-archived-agent".to_string()),
                agent_id: created.agent_id.clone(),
                title: Some("Should fail".to_string()),
            })
            .is_err());
    }

    #[test]
    fn remove_agent_detaches_bootstrap_preset_manager_reference() {
        let (_temp_dir, storage) = test_storage();
        let created = storage
            .create_agent(NewAgent {
                agent_id: "preset-manager".to_string(),
                name: "Preset Manager".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: Some("Manager".to_string()),
                memory_binding: None,
            })
            .expect("create preset manager");
        storage
            .create_bootstrap_preset(NewBootstrapPreset {
                preset_key: "ops-manager".to_string(),
                display_name: "Ops Manager".to_string(),
                description: "Preset with manager link".to_string(),
                role_label: "Manager".to_string(),
                provider_path: "openai".to_string(),
                default_model_provider: Some("openai".to_string()),
                default_model_id: Some("gpt-5-mini".to_string()),
                default_tool_profile: Some("default".to_string()),
                default_workspace_root: Some(".".to_string()),
                default_reports_to_agent_id: Some(created.agent_id.clone()),
                setup_notes: None,
            })
            .expect("create preset");

        let outcome = storage
            .remove_agent(&created.agent_id)
            .expect("remove should return outcome");
        assert_eq!(outcome, RemoveAgentOutcome::Removed);
        assert!(storage
            .get_agent(&created.agent_id)
            .expect("reload removed agent")
            .is_none());
        let preset = storage
            .get_bootstrap_preset("ops-manager")
            .expect("reload preset")
            .expect("preset remains");
        assert_eq!(preset.default_reports_to_agent_id, None);
    }

    #[test]
    fn remove_agent_rejects_active_jobs_and_detaches_board_cards() {
        let (_temp_dir, storage) = test_storage();
        let job_agent = storage
            .create_agent(NewAgent {
                agent_id: "job-owner".to_string(),
                name: "Job Owner".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: None,
                memory_binding: None,
            })
            .expect("create job owner agent");
        let board_agent = storage
            .create_agent(NewAgent {
                agent_id: "board-owner".to_string(),
                name: "Board Owner".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: None,
                memory_binding: None,
            })
            .expect("create board owner agent");

        let now = now_ms();
        let _job = storage
            .create_job(NewJob {
                agent_id: job_agent.agent_id.clone(),
                name: "job-owner-check".to_string(),
                enabled: true,
                schedule_kind: "interval".to_string(),
                interval_seconds: Some(60),
                run_at_ms: None,
                next_run_at: Some(now),
                payload_json: r#"{"mode":"noop"}"#.to_string(),
                max_retries: 1,
                retry_backoff_ms: 500,
                timeout_ms: 2_000,
            })
            .expect("create job reference");

        let board = storage
            .list_boards()
            .expect("list boards")
            .into_iter()
            .next()
            .expect("seeded board");
        let column = storage
            .list_board_columns(&board.board_id)
            .expect("list board columns")
            .into_iter()
            .next()
            .expect("seeded board column");
        let _card = storage
            .create_board_card(NewBoardCard {
                board_id: board.board_id.clone(),
                column_id: column.column_id.clone(),
                title: "Board-owned card".to_string(),
                description: None,
                owner_kind: "agent".to_string(),
                owner_agent_id: Some(board_agent.agent_id.clone()),
                owner_human_id: None,
                due_at: None,
                tags_json: Some("[]".to_string()),
                script_markdown: None,
            })
            .expect("create board card reference");

        let job_outcome = storage
            .remove_agent(&job_agent.agent_id)
            .expect("remove job owner should return outcome");
        assert_eq!(job_outcome, RemoveAgentOutcome::HasReferences);

        let board_outcome = storage
            .remove_agent(&board_agent.agent_id)
            .expect("remove board owner should return outcome");
        assert_eq!(board_outcome, RemoveAgentOutcome::Removed);
        assert!(storage
            .get_agent(&board_agent.agent_id)
            .expect("reload board owner")
            .is_none());
        let card = storage
            .get_board_card(&_card.card_id)
            .expect("reload board card")
            .expect("board card remains");
        assert_eq!(card.owner_kind, "unassigned");
        assert_eq!(card.owner_agent_id, None);
    }

    #[test]
    fn bootstrap_presets_allow_unresolved_manager_defaults() {
        let (temp_dir, storage) = test_storage();
        let workspace_root = temp_dir.path().join("shared");
        let workspace_root_string = workspace_root.to_string_lossy().to_string();
        let created = storage
            .create_bootstrap_preset(NewBootstrapPreset {
                preset_key: "cross-workspace".to_string(),
                display_name: "Cross Workspace".to_string(),
                description: "Importable without local manager".to_string(),
                role_label: "Strategist".to_string(),
                provider_path: "openai".to_string(),
                default_model_provider: Some("openai".to_string()),
                default_model_id: Some("gpt-5-mini".to_string()),
                default_tool_profile: Some("default".to_string()),
                default_workspace_root: Some(format!(" {workspace_root_string} ")),
                default_reports_to_agent_id: Some("Missing-Manager".to_string()),
                setup_notes: Some("Imported preset".to_string()),
            })
            .expect("create preset with soft manager reference");
        assert_eq!(
            created.default_workspace_root.as_deref(),
            Some(workspace_root_string.as_str())
        );
        assert_eq!(
            created.default_reports_to_agent_id.as_deref(),
            Some("missing-manager")
        );

        let updated = storage
            .update_bootstrap_preset(
                &created.preset_key,
                BootstrapPresetUpdatePatch {
                    display_name: None,
                    description: None,
                    role_label: None,
                    provider_path: None,
                    default_model_provider: None,
                    default_model_id: None,
                    default_tool_profile: None,
                    default_workspace_root: Some(Some(" . ".to_string())),
                    default_reports_to_agent_id: Some(Some("Still-Missing".to_string())),
                    setup_notes: None,
                },
            )
            .expect("update preset")
            .expect("preset should exist");
        assert!(updated.default_workspace_root.is_none());
        assert_eq!(
            updated.default_reports_to_agent_id.as_deref(),
            Some("still-missing")
        );
    }

    #[test]
    fn create_task_rejects_completed_projects_and_cross_project_parent_moves() {
        let (_temp_dir, storage) = test_storage();
        let goal = storage
            .create_goal(NewGoal {
                slug: "goal-a".to_string(),
                title: "Goal A".to_string(),
                summary: String::new(),
                status: GOAL_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                target_date: None,
            })
            .expect("create goal");
        let active_project = storage
            .create_project(NewProject {
                goal_id: goal.goal_id.clone(),
                slug: "active-project".to_string(),
                name: "Active Project".to_string(),
                summary: String::new(),
                status: PROJECT_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                workspace_root: Some(".".to_string()),
                budget_month_usd: None,
            })
            .expect("create active project");
        let completed_project = storage
            .create_project(NewProject {
                goal_id: goal.goal_id.clone(),
                slug: "completed-project".to_string(),
                name: "Completed Project".to_string(),
                summary: String::new(),
                status: PROJECT_STATUS_COMPLETED.to_string(),
                owner_agent_id: None,
                workspace_root: Some(".".to_string()),
                budget_month_usd: None,
            })
            .expect("create completed project");
        let second_active_project = storage
            .create_project(NewProject {
                goal_id: goal.goal_id.clone(),
                slug: "second-active-project".to_string(),
                name: "Second Active Project".to_string(),
                summary: String::new(),
                status: PROJECT_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                workspace_root: Some(".".to_string()),
                budget_month_usd: None,
            })
            .expect("create second active project");

        let create_err = storage
            .create_task(NewTask {
                project_id: completed_project.project_id.clone(),
                parent_task_id: None,
                title: "Blocked Create".to_string(),
                detail: String::new(),
                status: TASK_STATUS_TODO.to_string(),
                priority: TASK_PRIORITY_NORMAL.to_string(),
                owner_agent_id: None,
                due_at: None,
                blocked_reason: None,
            })
            .expect_err("completed project should reject new task");
        assert!(create_err
            .to_string()
            .contains("project does not allow task changes"));

        let parent = storage
            .create_task(NewTask {
                project_id: active_project.project_id.clone(),
                parent_task_id: None,
                title: "Parent Task".to_string(),
                detail: String::new(),
                status: TASK_STATUS_TODO.to_string(),
                priority: TASK_PRIORITY_NORMAL.to_string(),
                owner_agent_id: None,
                due_at: None,
                blocked_reason: None,
            })
            .expect("create parent task");
        storage
            .create_task(NewTask {
                project_id: active_project.project_id.clone(),
                parent_task_id: Some(parent.task_id.clone()),
                title: "Child Task".to_string(),
                detail: String::new(),
                status: TASK_STATUS_TODO.to_string(),
                priority: TASK_PRIORITY_NORMAL.to_string(),
                owner_agent_id: None,
                due_at: None,
                blocked_reason: None,
            })
            .expect("create child task");

        let update_err = storage
            .update_task(
                &parent.task_id,
                TaskUpdatePatch {
                    project_id: Some(second_active_project.project_id.clone()),
                    parent_task_id: Some(None),
                    title: None,
                    detail: None,
                    status: None,
                    priority: None,
                    owner_agent_id: None,
                    due_at: None,
                    blocked_reason: None,
                },
            )
            .expect_err("task with subtasks should not move projects");
        assert!(update_err
            .to_string()
            .contains("task with subtasks cannot move to another project"));
    }

    #[test]
    fn project_workspace_roots_are_normalized() {
        let (temp_dir, storage) = test_storage();
        let workspace_root = temp_dir.path().join("strategy-project");
        let workspace_root_string = workspace_root.to_string_lossy().to_string();
        let goal = storage
            .create_goal(NewGoal {
                slug: "normalized-roots".to_string(),
                title: "Normalize Roots".to_string(),
                summary: String::new(),
                status: GOAL_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                target_date: None,
            })
            .expect("create goal");
        let created = storage
            .create_project(NewProject {
                goal_id: goal.goal_id.clone(),
                slug: "normalized-project".to_string(),
                name: "Normalized Project".to_string(),
                summary: String::new(),
                status: PROJECT_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                workspace_root: Some(format!(" {workspace_root_string} ")),
                budget_month_usd: None,
            })
            .expect("create project");
        assert_eq!(
            created.workspace_root.as_deref(),
            Some(workspace_root_string.as_str())
        );

        let updated = storage
            .update_project(
                &created.project_id,
                ProjectUpdatePatch {
                    goal_id: None,
                    slug: None,
                    name: None,
                    summary: None,
                    status: None,
                    owner_agent_id: None,
                    workspace_root: Some(Some(" . ".to_string())),
                    budget_month_usd: None,
                },
            )
            .expect("update project")
            .expect("project should exist");
        assert!(updated.workspace_root.is_none());
    }

    #[test]
    fn clear_task_links_only_clears_requested_targets() {
        let (_temp_dir, storage) = test_storage();
        let goal = storage
            .create_goal(NewGoal {
                slug: "clear-links-goal".to_string(),
                title: "Clear Links".to_string(),
                summary: String::new(),
                status: GOAL_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                target_date: None,
            })
            .expect("create goal");
        let project = storage
            .create_project(NewProject {
                goal_id: goal.goal_id.clone(),
                slug: "clear-links-project".to_string(),
                name: "Clear Links Project".to_string(),
                summary: String::new(),
                status: PROJECT_STATUS_ACTIVE.to_string(),
                owner_agent_id: None,
                workspace_root: Some(".".to_string()),
                budget_month_usd: None,
            })
            .expect("create project");
        let task = storage
            .create_task(NewTask {
                project_id: project.project_id.clone(),
                parent_task_id: None,
                title: "Linked Task".to_string(),
                detail: String::new(),
                status: TASK_STATUS_TODO.to_string(),
                priority: TASK_PRIORITY_NORMAL.to_string(),
                owner_agent_id: Some("default".to_string()),
                due_at: None,
                blocked_reason: None,
            })
            .expect("create task");

        let board = storage
            .list_boards()
            .expect("list boards")
            .into_iter()
            .next()
            .expect("seeded board");
        let column = storage
            .list_board_columns(&board.board_id)
            .expect("list board columns")
            .into_iter()
            .next()
            .expect("seeded board column");
        let card = storage
            .create_board_card(NewBoardCard {
                board_id: board.board_id.clone(),
                column_id: column.column_id.clone(),
                title: "Linked Card".to_string(),
                description: None,
                owner_kind: "agent".to_string(),
                owner_agent_id: Some("default".to_string()),
                owner_human_id: None,
                due_at: None,
                tags_json: Some("[]".to_string()),
                script_markdown: None,
            })
            .expect("create board card");
        let job = storage
            .create_job(NewJob {
                agent_id: "default".to_string(),
                name: "linked-job".to_string(),
                enabled: true,
                schedule_kind: "interval".to_string(),
                interval_seconds: Some(60),
                run_at_ms: None,
                next_run_at: Some(now_ms()),
                payload_json: r#"{"mode":"noop"}"#.to_string(),
                max_retries: 1,
                retry_backoff_ms: 250,
                timeout_ms: 2_000,
            })
            .expect("create job");

        let linked = storage
            .link_task_board_card(&task.task_id, &card.card_id, false)
            .expect("link board card")
            .expect("linked task");
        assert_eq!(
            linked.linked_board_card_id.as_deref(),
            Some(card.card_id.as_str())
        );
        let linked = storage
            .link_task_job(&task.task_id, &job.job_id, false)
            .expect("link job")
            .expect("linked task");
        assert_eq!(linked.linked_job_id.as_deref(), Some(job.job_id.as_str()));

        let unchanged = storage
            .clear_task_links(&task.task_id, false, false)
            .expect("clear no-op")
            .expect("task exists");
        assert_eq!(
            unchanged.linked_board_card_id.as_deref(),
            Some(card.card_id.as_str())
        );
        assert_eq!(
            unchanged.linked_job_id.as_deref(),
            Some(job.job_id.as_str())
        );

        let cleared_job = storage
            .clear_task_links(&task.task_id, false, true)
            .expect("clear only job")
            .expect("task exists");
        assert_eq!(
            cleared_job.linked_board_card_id.as_deref(),
            Some(card.card_id.as_str())
        );
        assert!(cleared_job.linked_job_id.is_none());
    }

    #[test]
    fn connector_registry_import_publish_and_rollback_flow_works() {
        let (_temp_dir, storage) = test_storage();
        let (connector, version_v1) = storage
            .import_connector(NewConnectorImport {
                display_name: "GitHub".to_string(),
                slug: "github".to_string(),
                source_kind: "openapi".to_string(),
                origin_kind: "imported_local".to_string(),
                catalog_item_id: None,
                version_label: "v1".to_string(),
                source_digest: "digest-v1".to_string(),
                raw_source_location: Some("inline".to_string()),
                import_metadata_json: r#"{"source_kind":"openapi","endpoint_url":"https://api.example.test","source_json":{"paths":{}}}"#.to_string(),
                schema_summary_json: r#"{"operation_count":1}"#.to_string(),
                external_reference_policy: "inline_only".to_string(),
                trust_state: "local_untrusted".to_string(),
            })
            .expect("import connector");
        assert_eq!(connector.slug, "github");
        assert_eq!(connector.status, CONNECTOR_STATUS_DRAFT);

        let conversion_v1 = storage
            .record_connector_conversion(
                &connector.connector_id,
                &version_v1.version_id,
                NewConnectorConversion {
                    status: CONNECTOR_CONVERSION_SUCCEEDED.to_string(),
                    warnings_json: "[]".to_string(),
                    proposed_tools_json: r#"[{"candidate_id":"cand-1","operation_key":"listIssues","proposed_tool_name":"connector.github.list-issues","display_name":"List Issues","description":"Lists issues","input_schema":{"type":"object","properties":{}},"write_classification":"read_only","review_blocked":false,"review_block_reason":null,"origin_metadata":{"source_kind":"openapi","endpoint_url":"https://api.example.test","path":"/issues","method":"GET"}}]"#.to_string(),
                    write_capable_tools: 0,
                    unsupported_operations_json: "[]".to_string(),
                    normalization_notes_json: r#"["source_kind=openapi"]"#.to_string(),
                    diff_from_previous_json: r#"{"added":["connector.github.list-issues"],"removed":[],"unchanged":[]}"#.to_string(),
                },
            )
            .expect("record conversion");
        assert_eq!(conversion_v1.status, CONNECTOR_CONVERSION_SUCCEEDED);

        let (published_connector, _published_version, published_tools) = storage
            .publish_connector_tools(
                &connector.connector_id,
                &conversion_v1.conversion_id,
                &[String::from("cand-1")],
                &[NewConnectorPublishedTool {
                    tool_name: "connector.github.list-issues".to_string(),
                    display_name: "List Issues".to_string(),
                    tool_schema_json: r#"{"type":"object","properties":{}}"#.to_string(),
                    origin_metadata_json: r#"{"source_kind":"openapi","endpoint_url":"https://api.example.test","path":"/issues","method":"GET"}"#.to_string(),
                    write_classification: CONNECTOR_WRITE_READ_ONLY.to_string(),
                }],
                true,
            )
            .expect("publish connector tools");
        assert_eq!(published_connector.status, CONNECTOR_STATUS_ENABLED);
        assert_eq!(published_tools.len(), 1);

        let assignment = storage
            .upsert_connector_assignment(
                &connector.connector_id,
                NewConnectorAssignment {
                    agent_id: "default".to_string(),
                    enabled: true,
                    auth_mode: "shared_default".to_string(),
                },
            )
            .expect("upsert assignment");
        assert_eq!(assignment.agent_id, "default");

        let auth_binding = storage
            .upsert_connector_auth_binding(
                &connector.connector_id,
                NewConnectorAuthBinding {
                    agent_id: None,
                    auth_kind: "bearer".to_string(),
                    secret_ref: Some("connector/github".to_string()),
                    oauth_session_id: None,
                    status: "ready".to_string(),
                    auth_metadata_json: r#"{"header_name":"authorization"}"#.to_string(),
                    last_success_at: Some(now_ms()),
                    last_error: None,
                    last_rotated_at: Some(now_ms()),
                },
            )
            .expect("upsert auth binding");
        assert_eq!(auth_binding.auth_kind, "bearer");

        let interaction = storage
            .create_connector_interaction(
                &connector.connector_id,
                NewConnectorInteraction {
                    agent_id: Some("default".to_string()),
                    interaction_kind: "auth_repair".to_string(),
                    status: CONNECTOR_INTERACTION_WAITING.to_string(),
                    prompt_summary: "Repair auth".to_string(),
                    resume_token: Some("resume-token".to_string()),
                    expires_at: Some(now_ms() + 60_000),
                    detail_json: r#"{"reason":"missing token"}"#.to_string(),
                },
            )
            .expect("create interaction");
        let resumed = storage
            .resume_connector_interaction(
                &interaction.interaction_id,
                CONNECTOR_INTERACTION_RESUMED,
                Some(r#"{"reason":"fixed"}"#),
            )
            .expect("resume interaction")
            .expect("interaction exists");
        assert_eq!(resumed.status, CONNECTOR_INTERACTION_RESUMED);
        assert!(resumed.resume_token.is_none());
        assert!(resumed.consumed_at.is_some());

        let (_connector_v2, version_v2) = storage
            .import_connector(NewConnectorImport {
                display_name: "GitHub".to_string(),
                slug: "github".to_string(),
                source_kind: "openapi".to_string(),
                origin_kind: "imported_local".to_string(),
                catalog_item_id: None,
                version_label: "v2".to_string(),
                source_digest: "digest-v2".to_string(),
                raw_source_location: Some("inline".to_string()),
                import_metadata_json: r#"{"source_kind":"openapi","endpoint_url":"https://api.example.test","source_json":{"paths":{}}}"#.to_string(),
                schema_summary_json: r#"{"operation_count":1}"#.to_string(),
                external_reference_policy: "inline_only".to_string(),
                trust_state: "reviewed_local".to_string(),
            })
            .expect("import second version");
        let conversion_v2 = storage
            .record_connector_conversion(
                &connector.connector_id,
                &version_v2.version_id,
                NewConnectorConversion {
                    status: CONNECTOR_CONVERSION_SUCCEEDED.to_string(),
                    warnings_json: "[]".to_string(),
                    proposed_tools_json: r#"[{"candidate_id":"cand-2","operation_key":"createIssue","proposed_tool_name":"connector.github.create-issue","display_name":"Create Issue","description":"Creates issue","input_schema":{"type":"object","properties":{"title":{"type":"string"}}},"write_classification":"operator_write_gated","review_blocked":false,"review_block_reason":null,"origin_metadata":{"source_kind":"openapi","endpoint_url":"https://api.example.test","path":"/issues","method":"POST"}}]"#.to_string(),
                    write_capable_tools: 1,
                    unsupported_operations_json: "[]".to_string(),
                    normalization_notes_json: r#"["source_kind=openapi"]"#.to_string(),
                    diff_from_previous_json: r#"{"added":["connector.github.create-issue"],"removed":["connector.github.list-issues"],"unchanged":[]}"#.to_string(),
                },
            )
            .expect("record v2 conversion");
        storage
            .publish_connector_tools(
                &connector.connector_id,
                &conversion_v2.conversion_id,
                &[String::from("cand-2")],
                &[NewConnectorPublishedTool {
                    tool_name: "connector.github.create-issue".to_string(),
                    display_name: "Create Issue".to_string(),
                    tool_schema_json: r#"{"type":"object","properties":{"title":{"type":"string"}}}"#.to_string(),
                    origin_metadata_json: r#"{"source_kind":"openapi","endpoint_url":"https://api.example.test","path":"/issues","method":"POST"}"#.to_string(),
                    write_classification: CONNECTOR_WRITE_OPERATOR_GATED.to_string(),
                }],
                true,
            )
            .expect("publish v2");

        let (rolled_back_connector, rolled_back_version, rolled_back_tools) = storage
            .rollback_connector_version(&connector.connector_id, &version_v1.version_id)
            .expect("rollback version")
            .expect("version exists");
        assert_eq!(
            rolled_back_connector.current_version_id.as_deref(),
            Some(version_v1.version_id.as_str())
        );
        assert_eq!(rolled_back_version.version_id, version_v1.version_id);
        assert_eq!(rolled_back_tools.len(), 1);
        assert_eq!(
            rolled_back_tools[0].tool_name,
            "connector.github.list-issues"
        );
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
                    schedule_kind: None,
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
    fn generic_job_crud_cannot_forge_or_mutate_reserved_execass_modes() {
        let (_temp_dir, storage) = test_storage();
        let now = now_ms();
        let new_job = |payload_json: &str| NewJob {
            agent_id: "default".to_string(),
            name: "reserved-forgery".to_string(),
            enabled: true,
            schedule_kind: "interval".to_string(),
            interval_seconds: Some(60),
            run_at_ms: None,
            next_run_at: Some(now),
            payload_json: payload_json.to_string(),
            max_retries: 0,
            retry_backoff_ms: 250,
            timeout_ms: 2_000,
        };
        for payload in [
            r#"{"mode":"execass.continuation","continuation_id":"c-forged"}"#,
            r#"{"mode":"execass.routine_driver","routine_id":"r-forged"}"#,
            r#"{"mode":"execass.routine_trigger","occurrence_id":"o-forged"}"#,
        ] {
            assert!(storage.create_job(new_job(payload)).is_err());
        }

        let ordinary = storage
            .create_job(new_job(r#"{"mode":"noop"}"#))
            .expect("create ordinary control job");
        let forge_patch = JobUpdatePatch {
            name: None,
            enabled: None,
            schedule_kind: None,
            interval_seconds: None,
            run_at_ms: None,
            next_run_at: None,
            payload_json: Some(
                r#"{"mode":"execass.routine_driver","routine_id":"r-forged"}"#.into(),
            ),
            max_retries: None,
            retry_backoff_ms: None,
            timeout_ms: None,
        };
        assert!(storage.update_job(&ordinary.job_id, forge_patch).is_err());

        storage
            .connect()
            .unwrap()
            .execute(
                "UPDATE jobs SET payload_json=?1 WHERE job_id=?2",
                params![
                    r#"{"mode":"execass.routine_trigger","occurrence_id":"o-forged"}"#,
                    ordinary.job_id
                ],
            )
            .unwrap();
        assert!(storage
            .update_job(
                &ordinary.job_id,
                JobUpdatePatch {
                    name: Some("still-forged".into()),
                    enabled: None,
                    schedule_kind: None,
                    interval_seconds: None,
                    run_at_ms: None,
                    next_run_at: None,
                    payload_json: None,
                    max_retries: None,
                    retry_backoff_ms: None,
                    timeout_ms: None,
                },
            )
            .is_err());
        assert!(storage.remove_job(&ordinary.job_id).is_err());
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

    #[test]
    fn agent_mail_thread_message_search_and_ack_work() {
        let (_temp_dir, storage) = test_storage();
        let thread = storage
            .create_agent_mail_thread(NewAgentMailThread {
                kind: "direct".to_string(),
                subject: "Launch sync".to_string(),
                created_by_principal: "lyra".to_string(),
                participants: vec![("claude".to_string(), "member".to_string())],
            })
            .expect("create thread");
        assert_eq!(thread.kind, "direct");

        let participants = storage
            .list_agent_mail_thread_participants(&thread.thread_id)
            .expect("list participants");
        assert_eq!(participants.len(), 2);

        let first = storage
            .create_agent_mail_message(NewAgentMailMessage {
                thread_id: thread.thread_id.clone(),
                sender_principal: "lyra".to_string(),
                sender_kind: "agent".to_string(),
                body_text: "We should ship the pipeline fix tonight.".to_string(),
                metadata_json: None,
                recipients: vec!["claude".to_string()],
            })
            .expect("create first message")
            .expect("message inserted");
        assert_eq!(first.sender_principal, "lyra");

        let summaries = storage
            .list_agent_mail_threads(&AgentMailThreadListFilter {
                kind: Some("direct".to_string()),
                principal_id: Some("claude".to_string()),
                mailbox: Some("inbox".to_string()),
                search_text: Some("pipeline".to_string()),
                limit: 25,
            })
            .expect("list inbox summaries");
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].unread_count >= 1);

        let ack = storage
            .acknowledge_agent_mail_message(&first.message_id, "claude")
            .expect("ack query")
            .expect("ack row");
        assert!(ack.acked_at.is_some());

        let attachment = storage
            .create_agent_mail_attachment(NewAgentMailAttachment {
                message_id: first.message_id.clone(),
                filename: "notes.txt".to_string(),
                mime: "text/plain".to_string(),
                sha256: "abc123".to_string(),
                bytes: 5,
                local_path: "/tmp/notes.txt".to_string(),
            })
            .expect("create attachment")
            .expect("attachment exists");
        assert_eq!(attachment.filename, "notes.txt");

        let attachments = storage
            .list_agent_mail_attachments(&first.message_id)
            .expect("list attachments");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].attachment_id, attachment.attachment_id);
    }

    #[test]
    fn agent_mail_file_leases_enforce_conflicts_and_release() {
        let (_temp_dir, storage) = test_storage();
        let lease = storage
            .create_agent_mail_file_lease(NewAgentMailFileLease {
                holder_principal: "lyra".to_string(),
                glob_pattern: "src/**".to_string(),
                exclusive: true,
                ttl_ms: 120_000,
                note: Some("migration pass".to_string()),
            })
            .expect("create lease");
        assert!(lease.exclusive);

        let conflict = storage.create_agent_mail_file_lease(NewAgentMailFileLease {
            holder_principal: "claude".to_string(),
            glob_pattern: "src/**".to_string(),
            exclusive: true,
            ttl_ms: 120_000,
            note: None,
        });
        assert!(conflict.is_err());
        let conflict_message = conflict
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(
            conflict_message.contains("active lease conflict"),
            "conflict error should preserve stable message; got: {conflict_message}"
        );

        let non_conflict = storage
            .create_agent_mail_file_lease(NewAgentMailFileLease {
                holder_principal: "claude".to_string(),
                glob_pattern: "docs/**".to_string(),
                exclusive: false,
                ttl_ms: 120_000,
                note: None,
            })
            .expect("create non-conflicting lease");
        assert!(!non_conflict.exclusive);

        let active_before_release = storage
            .list_agent_mail_file_leases(None, false)
            .expect("list active leases");
        assert_eq!(active_before_release.len(), 2);

        let blocked_release = storage
            .release_agent_mail_file_lease(&lease.lease_id, Some("wrong_holder"))
            .expect("blocked release check");
        assert!(blocked_release.is_none());

        let released = storage
            .release_agent_mail_file_lease(&lease.lease_id, Some("lyra"))
            .expect("release lease")
            .expect("released lease exists");
        assert!(released.released_at.is_some());

        let recreated = storage
            .create_agent_mail_file_lease(NewAgentMailFileLease {
                holder_principal: "claude".to_string(),
                glob_pattern: "src/**".to_string(),
                exclusive: true,
                ttl_ms: 120_000,
                note: Some("post-release".to_string()),
            })
            .expect("create lease after release");
        assert_eq!(recreated.glob_pattern, "src/**");
    }

    #[test]
    fn assistant_worker_lifecycle_supports_pending_and_active_updates() {
        let (_temp_dir, storage) = test_storage();
        let root_session = storage
            .create_session(NewSession {
                session_key: Some("assistant-root-session".to_string()),
                agent_id: "default".to_string(),
                title: Some("Assistant Root".to_string()),
            })
            .expect("create root session");
        let root_run = storage
            .create_run(NewRun {
                session_id: root_session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create root run")
            .expect("root run inserted");
        let approval = storage
            .create_approval(NewApproval {
                run_id: root_run.run_id.clone(),
                tool_call_id: None,
                kind: "assistant.worker.spawn".to_string(),
                request_summary: "Hire worker".to_string(),
                request_json: r#"{"worker_key":"research_1"}"#.to_string(),
            })
            .expect("create approval")
            .expect("approval inserted");
        let worker = storage
            .create_assistant_worker(NewAssistantWorker {
                boss_key: "default".to_string(),
                root_session_id: root_session.session_id.clone(),
                worker_key: "research_1".to_string(),
                worker_kind: "employee".to_string(),
                status: "pending_approval".to_string(),
                agent_id: None,
                session_id: None,
                template_key: "researcher".to_string(),
                display_name: "Researcher".to_string(),
                instructions: Some("Find relevant context".to_string()),
                run_defaults_json: r#"{"model_provider":"openai","model_id":"gpt-4.1"}"#
                    .to_string(),
                session_mode: "persistent".to_string(),
                pending_approval_id: Some(approval.approval_id.clone()),
            })
            .expect("create assistant worker");
        assert_eq!(worker.status, "pending_approval");
        assert!(worker.agent_id.is_none());

        let loaded = storage
            .get_assistant_worker_by_pending_approval(&approval.approval_id)
            .expect("lookup by pending approval")
            .expect("worker by pending approval");
        assert_eq!(loaded.worker_key, "research_1");

        let worker_agent = storage
            .create_agent(NewAgent {
                agent_id: "worker-research-1".to_string(),
                name: "Research Worker".to_string(),
                workspace_root: ".".to_string(),
                model_provider: "openai".to_string(),
                model_id: "gpt-4.1".to_string(),
                tool_profile: "default".to_string(),
                reports_to_agent_id: None,
                role_label: None,
                memory_binding: None,
            })
            .expect("create worker agent");
        let worker_session = storage
            .create_session(NewSession {
                session_key: Some("assistant-worker-session-1".to_string()),
                agent_id: worker_agent.agent_id.clone(),
                title: Some("Worker Session".to_string()),
            })
            .expect("create worker session");

        let updated = storage
            .update_assistant_worker(
                "default",
                "research_1",
                AssistantWorkerPatch {
                    status: Some("active".to_string()),
                    agent_id: Some(Some(worker_agent.agent_id.clone())),
                    session_id: Some(Some(worker_session.session_id.clone())),
                    pending_approval_id: Some(None),
                    ..AssistantWorkerPatch::default()
                },
            )
            .expect("update assistant worker")
            .expect("updated worker");
        assert_eq!(updated.status, "active");
        assert_eq!(
            updated.agent_id.as_deref(),
            Some(worker_agent.agent_id.as_str())
        );
        assert_eq!(
            updated.session_id.as_deref(),
            Some(worker_session.session_id.as_str())
        );
        assert!(updated.pending_approval_id.is_none());

        let listed = storage
            .list_assistant_workers("default", false, 20)
            .expect("list assistant workers");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].worker_key, "research_1");

        storage
            .create_assistant_task_link(
                "default",
                "research_1",
                &root_run.run_id,
                &root_session.session_id,
            )
            .expect("create assistant task link");
        let link_exists = storage
            .assistant_task_link_exists("default", "research_1", &root_run.run_id)
            .expect("query assistant task link");
        assert!(link_exists);

        let duplicate_link = storage.create_assistant_task_link(
            "default",
            "research_1",
            &root_run.run_id,
            &root_session.session_id,
        );
        assert!(duplicate_link.is_err());

        let mismatch_link = storage.create_assistant_task_link(
            "default",
            "research_1",
            &root_run.run_id,
            &worker_session.session_id,
        );
        assert!(mismatch_link.is_err());

        let second_run = storage
            .create_run(NewRun {
                session_id: root_session.session_id.clone(),
                model_provider: "mock".to_string(),
                model_id: "mock-echo-v1".to_string(),
            })
            .expect("create second root run")
            .expect("second run inserted");
        let conflicting_pair = storage.create_assistant_task_link(
            "default",
            "research_1",
            &second_run.run_id,
            &root_session.session_id,
        );
        assert!(conflicting_pair.is_err());

        let missing_worker_link = storage.create_assistant_task_link(
            "default",
            "missing-worker",
            &root_run.run_id,
            &root_session.session_id,
        );
        assert!(missing_worker_link.is_err());

        storage
            .update_assistant_worker(
                "default",
                "research_1",
                AssistantWorkerPatch {
                    status: Some("archived".to_string()),
                    archived_at: Some(Some(now_ms())),
                    ..AssistantWorkerPatch::default()
                },
            )
            .expect("archive assistant worker")
            .expect("archived worker row");
        let archived_worker_link = storage.create_assistant_task_link(
            "default",
            "research_1",
            &second_run.run_id,
            &root_session.session_id,
        );
        assert!(archived_worker_link.is_err());

        let audit = storage
            .create_assistant_tool_call_audit(NewAssistantToolCallAudit {
                request_id: "req-assistant-audit-1".to_string(),
                boss_key: "default".to_string(),
                root_session_id: root_session.session_id.clone(),
                root_run_id: Some(root_run.run_id.clone()),
                caller_agent_id: "default".to_string(),
                tool_name: "assistant.worker.spawn".to_string(),
                decision: "allow".to_string(),
                reason_code: Some("APPROVED".to_string()),
                audit_ref: Some("approval:assistant-worker".to_string()),
                metadata_json: Some(r#"{"worker_key":"research_1"}"#.to_string()),
            })
            .expect("create assistant audit event");
        assert_eq!(audit.request_id, "req-assistant-audit-1");
        assert_eq!(
            storage
                .get_assistant_tool_call_audit(&audit.event_id)
                .expect("read assistant audit by exact identity")
                .expect("assistant audit exists"),
            audit
        );
        let conn = storage.connect().expect("open storage connection");
        let audit_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM assistant_tool_calls_audit WHERE request_id = ?1",
                params!["req-assistant-audit-1"],
                |row| row.get(0),
            )
            .expect("query assistant audit events");
        assert_eq!(audit_count, 1);
    }
}
