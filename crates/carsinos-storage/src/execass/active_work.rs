//! Read-only, state-derived active-work snapshot for native runtime close control.

use super::store::ExecAssStore;
use super::types::{ExecAssActiveWorkStatus, ExecAssRuntimeCloseSnapshot};
use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

const ACTIVE_WORK_BINDING_DOMAIN: &[u8] = b"carsinos.execass.active-work-binding.v1";

const ACTIVE_WORK_COUNTS_SQL: &str = r#"
SELECT
  (SELECT COUNT(*) FROM execass_delegations
   WHERE phase IN (
     'accepted','planning','in_motion','waiting_for_user','waiting_external','recovering'
   )),
  (SELECT COUNT(*) FROM execass_continuations
   WHERE status IN ('runnable','executing','waiting','uncertain')),
  (SELECT COUNT(*) FROM execass_logical_effects
   WHERE state IN ('planned','claimed','invoking','outcome_unknown'))
"#;

const ACTIVE_WORK_BINDING_ROWS_SQL: &str = r#"
SELECT work_kind, work_id, work_state FROM (
  SELECT 'delegation' AS work_kind, delegation_id AS work_id, phase AS work_state
  FROM execass_delegations
  WHERE phase IN (
    'accepted','planning','in_motion','waiting_for_user','waiting_external','recovering'
  )
  UNION ALL
  SELECT 'continuation', continuation_id, status
  FROM execass_continuations
  WHERE status IN ('runnable','executing','waiting','uncertain')
  UNION ALL
  SELECT 'effect', logical_effect_id, state
  FROM execass_logical_effects
  WHERE state IN ('planned','claimed','invoking','outcome_unknown')
)
ORDER BY work_kind, work_id, work_state
"#;

impl ExecAssStore {
    /// Read the exact host/settings and active-work counts from one SQLite read
    /// transaction. No raw action, intent, consequence, or summary text is
    /// inspected by this query.
    pub fn execass_runtime_close_snapshot(
        &self,
        trusted_now: i64,
    ) -> Result<ExecAssRuntimeCloseSnapshot> {
        if trusted_now <= 0 {
            bail!("runtime close snapshot requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = conn
            .transaction()
            .context("failed starting runtime close read transaction")?;
        let host = super::runtime_settings::load_status(&tx, trusted_now)?;
        let (active_work, active_work_binding_digest) = load_active_work_snapshot(&tx)?;
        tx.commit()
            .context("failed completing runtime close read transaction")?;
        Ok(ExecAssRuntimeCloseSnapshot {
            host,
            active_work,
            active_work_binding_digest,
        })
    }

    /// Read only the exact state-derived active-work counts.
    pub fn execass_active_work_status(&self) -> Result<ExecAssActiveWorkStatus> {
        let conn = self.connection()?;
        load_active_work_status(&conn)
    }
}

pub(super) fn load_active_work_status(conn: &Connection) -> Result<ExecAssActiveWorkStatus> {
    load_active_work_snapshot(conn).map(|(status, _)| status)
}

pub(super) fn load_active_work_snapshot(
    conn: &Connection,
) -> Result<(ExecAssActiveWorkStatus, String)> {
    let (delegations, continuations, effects): (i64, i64, i64) = conn
        .query_row(ACTIVE_WORK_COUNTS_SQL, [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .context("failed reading explicit active-work states")?;
    let active_work_count = delegations
        .checked_add(continuations)
        .and_then(|count| count.checked_add(effects))
        .context("active-work count overflow")?;
    let status = ExecAssActiveWorkStatus {
        active: active_work_count > 0,
        active_work_count,
        nonterminal_delegation_count: delegations,
        nonterminal_continuation_count: continuations,
        nonterminal_effect_count: effects,
    };
    let mut digest = Sha256::new();
    digest.update(ACTIVE_WORK_BINDING_DOMAIN);
    let mut statement = conn
        .prepare(ACTIVE_WORK_BINDING_ROWS_SQL)
        .context("failed preparing active-work binding query")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("failed reading active-work binding rows")?;
    for row in rows {
        let (kind, id, state) = row.context("failed decoding active-work binding row")?;
        update_digest(&mut digest, &kind);
        update_digest(&mut digest, &id);
        update_digest(&mut digest, &state);
    }
    Ok((status, format!("sha256:{}", encode_hex(&digest.finalize()))))
}

fn update_digest(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
