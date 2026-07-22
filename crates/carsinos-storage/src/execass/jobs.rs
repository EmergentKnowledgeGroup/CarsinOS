use super::store::immediate_transaction;
use super::validation::require_text;
use super::ExecAssStore;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const EXECASS_CONTINUATION_JOB_MODE: &str = "execass.continuation";
const EXECASS_JOB_AGENT_ID: &str = "default";
const EXECASS_JOB_SCHEDULE_KIND: &str = "execass_continuation";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationJobBindingRecord {
    pub continuation_id: String,
    pub delegation_id: String,
    pub job_id: String,
    pub payload_json: String,
    pub next_run_at: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
}

#[derive(Serialize)]
struct ContinuationJobPayload<'a> {
    mode: &'static str,
    continuation_id: &'a str,
    delegation_id: &'a str,
    action_id: &'a str,
    target_delegation_revision: i64,
    target_plan_revision: i64,
    branch_kind: &'a str,
    causation_kind: &'a str,
    causation_id: &'a str,
}

#[derive(Debug)]
struct RunnableContinuation {
    continuation_id: String,
    delegation_id: String,
    action_id: String,
    target_delegation_revision: i64,
    target_plan_revision: i64,
    branch_kind: String,
    causation_kind: String,
    causation_id: String,
}

pub fn is_execass_continuation_job_payload(payload_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload_json)
        .ok()
        .and_then(|value| {
            value
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .map(str::to_ascii_lowercase)
        })
        .is_some_and(|mode| mode == EXECASS_CONTINUATION_JOB_MODE)
}

impl ExecAssStore {
    /// Reconciles runnable continuations into the one canonical `jobs` table.
    ///
    /// The deterministic identity and one-way binding make retries and
    /// concurrent reconcilers converge on one durable job. This method does
    /// not execute work and does not create a second scheduler.
    pub fn materialize_runnable_continuation_jobs(
        &self,
        scheduled_at: i64,
        limit: u32,
    ) -> Result<Vec<ContinuationJobBindingRecord>> {
        if scheduled_at <= 0 {
            bail!("scheduled_at must be positive");
        }
        if limit == 0 || limit > 1_000 {
            bail!("limit must be between 1 and 1000");
        }

        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let default_agent_exists = tx
            .query_row(
                "SELECT 1 FROM agents WHERE agent_id=?1 AND archived_at IS NULL LIMIT 1",
                params![EXECASS_JOB_AGENT_ID],
                |_| Ok(()),
            )
            .optional()
            .context("failed checking the canonical ExecAss job agent")?
            .is_some();
        if !default_agent_exists {
            bail!("canonical ExecAss job agent is unavailable");
        }

        let continuations = {
            let mut statement = tx
                .prepare(
                    r#"
                    SELECT continuation_id, delegation_id, action_id,
                           target_delegation_revision, target_plan_revision,
                           branch_kind, causation_kind, causation_id
                    FROM execass_continuations
                    WHERE status='runnable'
                      AND completed_at IS NULL
                      AND job_id IS NULL
                    ORDER BY created_at, delegation_id, continuation_id
                    LIMIT ?1
                    "#,
                )
                .context("failed preparing runnable continuation reconciliation")?;
            let rows = statement
                .query_map(params![i64::from(limit)], |row| {
                    Ok(RunnableContinuation {
                        continuation_id: row.get(0)?,
                        delegation_id: row.get(1)?,
                        action_id: row.get(2)?,
                        target_delegation_revision: row.get(3)?,
                        target_plan_revision: row.get(4)?,
                        branch_kind: row.get(5)?,
                        causation_kind: row.get(6)?,
                        causation_id: row.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
                .context("failed reading runnable continuations")?;
            rows
        };

        let mut bindings = Vec::with_capacity(continuations.len());
        for continuation in continuations {
            require_text("continuation_id", &continuation.continuation_id)?;
            require_text("delegation_id", &continuation.delegation_id)?;
            let job_id = deterministic_continuation_job_id(&continuation.continuation_id);
            let payload_json = serde_json::to_string(&ContinuationJobPayload {
                mode: EXECASS_CONTINUATION_JOB_MODE,
                continuation_id: &continuation.continuation_id,
                delegation_id: &continuation.delegation_id,
                action_id: &continuation.action_id,
                target_delegation_revision: continuation.target_delegation_revision,
                target_plan_revision: continuation.target_plan_revision,
                branch_kind: &continuation.branch_kind,
                causation_kind: &continuation.causation_kind,
                causation_id: &continuation.causation_id,
            })
            .context("failed serializing canonical continuation job payload")?;
            let name = format!("ExecAss continuation {}", continuation.continuation_id);

            tx.execute(
                r#"
                INSERT OR IGNORE INTO jobs (
                  job_id, agent_id, name, enabled, schedule_kind,
                  interval_seconds, run_at_ms, next_run_at, payload_json,
                  max_retries, retry_backoff_ms, timeout_ms,
                  lease_owner, lease_expires_at, last_run_at, last_error,
                  created_at, updated_at, deleted_at
                ) VALUES (
                  ?1, ?2, ?3, 1, ?4,
                  NULL, ?5, ?5, ?6,
                  0, 1000, 30000,
                  NULL, NULL, NULL, NULL,
                  ?5, ?5, NULL
                )
                "#,
                params![
                    job_id,
                    EXECASS_JOB_AGENT_ID,
                    name,
                    EXECASS_JOB_SCHEDULE_KIND,
                    scheduled_at,
                    payload_json,
                ],
            )
            .context("failed inserting deterministic continuation job")?;

            let identity_matches = tx
                .query_row(
                    r#"
                    SELECT 1 FROM jobs
                    WHERE job_id=?1
                      AND agent_id=?2
                      AND name=?3
                      AND enabled=1
                      AND schedule_kind=?4
                      AND interval_seconds IS NULL
                      AND run_at_ms=?5
                      AND next_run_at=?5
                      AND payload_json=?6
                      AND max_retries=0
                      AND retry_backoff_ms=1000
                      AND timeout_ms=30000
                      AND deleted_at IS NULL
                    LIMIT 1
                    "#,
                    params![
                        job_id,
                        EXECASS_JOB_AGENT_ID,
                        name,
                        EXECASS_JOB_SCHEDULE_KIND,
                        scheduled_at,
                        payload_json,
                    ],
                    |_| Ok(()),
                )
                .optional()
                .context("failed verifying deterministic continuation job")?
                .is_some();
            if !identity_matches {
                bail!("deterministic continuation job identity collision");
            }

            let changed = tx
                .execute(
                    r#"
                    UPDATE execass_continuations
                    SET job_id=?1, updated_at=MAX(updated_at, ?2)
                    WHERE continuation_id=?3
                      AND delegation_id=?4
                      AND status='runnable'
                      AND completed_at IS NULL
                      AND job_id IS NULL
                    "#,
                    params![
                        job_id,
                        scheduled_at,
                        continuation.continuation_id,
                        continuation.delegation_id,
                    ],
                )
                .context("failed binding continuation to deterministic job")?;
            if changed != 1 {
                bail!("runnable continuation changed during job reconciliation");
            }

            let existing_authority_event: Option<(String, String, String, i64)> = tx
                .query_row(
                    r#"SELECT event_id,correlation_id,causation_id,occurred_at
                   FROM execass_outbox_events
                   WHERE aggregate_id=?1 AND aggregate_revision=?2
                     AND event_name='execass.v1.delegation.transitioned'
                   ORDER BY event_id LIMIT 1"#,
                    params![
                        continuation.delegation_id,
                        continuation.target_delegation_revision,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            let (outbox_event_id, correlation_id, causation_id, linked_at) = if let Some(existing) =
                existing_authority_event
            {
                existing
            } else {
                let event_id = format!("continuation-job-authority-event-{job_id}");
                let correlation_id = format!("continuation-job-authority-correlation-{job_id}");
                let causation_id = format!("continuation-job-authority-cause-{job_id}");
                tx.execute(
                        r#"INSERT INTO execass_outbox_events(
                             event_id,event_name,aggregate_id,aggregate_revision,
                             correlation_id,causation_id,occurred_at,schema_version,
                             safe_payload_json,duplicate_identity
                           ) VALUES(?1,'execass.v1.delegation.transitioned',?2,?3,?4,?5,?6,'v1',?7,?8)"#,
                        params![
                            event_id,
                            continuation.delegation_id,
                            continuation.target_delegation_revision,
                            correlation_id,
                            causation_id,
                            scheduled_at,
                            serde_json::json!({
                                "operation": "continuation_job_materialized",
                                "continuation_id": continuation.continuation_id,
                                "job_id": job_id,
                            }).to_string(),
                            format!("continuation-job-authority-write-{job_id}"),
                        ],
                    )?;
                (event_id, correlation_id, causation_id, scheduled_at)
            };
            let link_id = format!("continuation-job-authority-{job_id}");
            let link_revision: i64 = tx.query_row(
                "SELECT COALESCE(MAX(link_revision),0)+1 FROM execass_authority_links WHERE delegation_id=?1",
                params![continuation.delegation_id],
                |row| row.get(0),
            )?;
            tx.execute(
                r#"INSERT INTO execass_authority_links(
                     link_id,delegation_id,link_revision,delegation_state_revision,
                     correlation_id,causation_id,outbox_event_id,authority_kind,
                     job_id,authoritative_revision,linked_at
                   ) VALUES(?1,?2,?3,?4,?5,?6,?7,'job',?8,0,?9)"#,
                params![
                    link_id,
                    continuation.delegation_id,
                    link_revision,
                    continuation.target_delegation_revision,
                    correlation_id,
                    causation_id,
                    outbox_event_id,
                    job_id,
                    linked_at,
                ],
            )
            .context("failed linking canonical continuation job authority")?;

            bindings.push(ContinuationJobBindingRecord {
                continuation_id: continuation.continuation_id,
                delegation_id: continuation.delegation_id,
                job_id,
                payload_json,
                next_run_at: scheduled_at,
                lease_owner: None,
                lease_expires_at: None,
            });
        }

        tx.commit()
            .context("failed committing continuation job reconciliation")?;
        Ok(bindings)
    }

    pub fn read_continuation_job_binding(
        &self,
        continuation_id: &str,
    ) -> Result<Option<ContinuationJobBindingRecord>> {
        require_text("continuation_id", continuation_id)?;
        let conn = self.connection()?;
        conn.query_row(
            r#"
            SELECT c.continuation_id, c.delegation_id, j.job_id,
                   j.payload_json, j.next_run_at, j.lease_owner,
                   j.lease_expires_at
            FROM execass_continuations c
            JOIN jobs j ON j.job_id=c.job_id
            WHERE c.continuation_id=?1 AND j.deleted_at IS NULL
            "#,
            params![continuation_id],
            |row| {
                Ok(ContinuationJobBindingRecord {
                    continuation_id: row.get(0)?,
                    delegation_id: row.get(1)?,
                    job_id: row.get(2)?,
                    payload_json: row.get(3)?,
                    next_run_at: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                    lease_owner: row.get(5)?,
                    lease_expires_at: row.get(6)?,
                })
            },
        )
        .optional()
        .context("failed reading continuation job binding")
    }
}

pub(super) fn deterministic_continuation_job_id(continuation_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.continuation_job.v1\0");
    digest.update(continuation_id.as_bytes());
    format!("execass-job-{:x}", digest.finalize())
}
