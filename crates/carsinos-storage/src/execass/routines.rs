//! EA-216 durable saved-routine timing and admission planning.
//!
//! This module intentionally does not create a delegation, continuation, or
//! external effect. Its only job integration is a reserved, typed trigger in
//! the existing `jobs` table. A gateway/lifecycle owner must consume a
//! [`RoutineAdmissionPlan`] and perform the distinct-delegation transaction.

use super::canonical::parse_strict_json;
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Offset, TimeZone};
use chrono_tz::Tz;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::str::FromStr;

pub const EXECASS_ROUTINE_TRIGGER_MODE: &str = "execass.routine_trigger";
pub const EXECASS_ROUTINE_DRIVER_MODE: &str = "execass.routine_driver";
const EXECASS_JOB_AGENT_ID: &str = "default";
const EXECASS_ROUTINE_TRIGGER_SCHEDULE_KIND: &str = "execass_routine_trigger";

#[derive(Serialize)]
struct RoutineTriggerPayload<'a> {
    mode: &'static str,
    occurrence_id: &'a str,
    routine_id: &'a str,
    routine_version: i64,
    scheduled_instant_ms: i64,
}

#[derive(Serialize)]
struct RoutineDriverPayload<'a> {
    mode: &'static str,
    routine_id: &'a str,
}

pub fn deterministic_routine_occurrence_id(
    routine_id: &str,
    routine_version: i64,
    scheduled_instant_ms: i64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.routine_occurrence.v1\0");
    digest.update(routine_id.as_bytes());
    digest.update(b"\0");
    digest.update(routine_version.to_le_bytes());
    digest.update(scheduled_instant_ms.to_le_bytes());
    format!("routine-occurrence-{:x}", digest.finalize())
}

fn deterministic_routine_trigger_job_id(occurrence_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.routine_trigger_job.v1\0");
    digest.update(occurrence_id.as_bytes());
    format!("execass-routine-trigger-{:x}", digest.finalize())
}

pub fn is_execass_routine_trigger_payload(payload_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload_json)
        .ok()
        .and_then(|value| {
            value
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|mode| mode == EXECASS_ROUTINE_TRIGGER_MODE)
}

pub fn is_execass_routine_driver_payload(payload_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload_json)
        .ok()
        .and_then(|value| {
            value
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|mode| mode == EXECASS_ROUTINE_DRIVER_MODE)
}

pub fn execass_routine_driver_id(payload_json: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    (value.get("mode")?.as_str()? == EXECASS_ROUTINE_DRIVER_MODE)
        .then(|| value.get("routine_id")?.as_str().map(str::to_owned))?
}

pub fn execass_routine_trigger_occurrence_id(payload_json: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(payload_json).ok()?;
    (value.get("mode")?.as_str()? == EXECASS_ROUTINE_TRIGGER_MODE)
        .then(|| value.get("occurrence_id")?.as_str().map(str::to_owned))?
}

/// Resolves a local IANA wall-clock time to one deterministic instant. Gaps
/// advance by a minute to the next valid local instant; overlaps select the
/// requested earlier/later instant.
pub fn resolve_routine_local_time(
    timezone: &str,
    date: NaiveDate,
    local_hour: u32,
    local_minute: u32,
    overlap_policy: RoutineOverlapPolicy,
) -> Result<RoutineOccurrenceCandidate> {
    let tz =
        Tz::from_str(timezone).with_context(|| format!("invalid IANA timezone: {timezone}"))?;
    let time = NaiveTime::from_hms_opt(local_hour, local_minute, 0)
        .context("routine local schedule time is invalid")?;
    let mut local = date.and_time(time);
    for _ in 0..=(24 * 60) {
        let local_result = tz.from_local_datetime(&local);
        let (resolved, resolution): (Option<DateTime<Tz>>, RoutineTimeResolution) =
            match local_result {
                chrono::LocalResult::None => (None, RoutineTimeResolution::GapAdvanced),
                chrono::LocalResult::Single(_) => (
                    local_result.earliest(),
                    if local != date.and_time(time) {
                        RoutineTimeResolution::GapAdvanced
                    } else {
                        RoutineTimeResolution::Single
                    },
                ),
                chrono::LocalResult::Ambiguous(_, _) => (
                    match overlap_policy {
                        RoutineOverlapPolicy::Earlier => local_result.earliest(),
                        RoutineOverlapPolicy::Later => local_result.latest(),
                    },
                    match overlap_policy {
                        RoutineOverlapPolicy::Earlier => RoutineTimeResolution::Earlier,
                        RoutineOverlapPolicy::Later => RoutineTimeResolution::Later,
                    },
                ),
            };
        if let Some(instant) = resolved {
            return Ok(RoutineOccurrenceCandidate {
                scheduled_instant_ms: instant.timestamp_millis(),
                scheduled_local: local.format("%Y-%m-%dT%H:%M").to_string(),
                utc_offset_seconds: i64::from(instant.offset().fix().local_minus_utc()),
                time_resolution: resolution,
            });
        }
        local = local
            .checked_add_signed(Duration::minutes(1))
            .context("routine DST gap advance overflowed local clock")?;
    }
    bail!("routine local time did not resolve within 24 hours")
}

pub fn select_catch_up_occurrences(
    catch_up_policy: RoutineCatchUpPolicy,
    due: &[RoutineOccurrenceCandidate],
    replay_cap: i64,
) -> Result<Vec<RoutineOccurrenceCandidate>> {
    if !(1..=10).contains(&replay_cap) {
        bail!("routine replay cap must be between 1 and 10");
    }
    Ok(match catch_up_policy {
        RoutineCatchUpPolicy::Skip => Vec::new(),
        RoutineCatchUpPolicy::LatestOnly => due.last().cloned().into_iter().collect(),
        RoutineCatchUpPolicy::Replay => due
            .iter()
            .rev()
            .take(replay_cap as usize)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
    })
}

impl ExecAssStore {
    pub fn create_routine(&self, command: &CreateRoutineCommand) -> Result<RoutineRecord> {
        let command = canonicalize_create(command)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        validate_source_anchor(&tx, &command.version)?;
        validate_grant_binding(&tx, &command.version)?;
        tx.execute(
            "INSERT INTO execass_routines(routine_id,current_version,enabled,timezone,overlap_policy,catch_up_policy,replay_cap,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![command.routine.routine_id, command.routine.current_version, i64::from(command.routine.enabled), command.routine.timezone, command.routine.overlap_policy.as_str(), command.routine.catch_up_policy.as_str(), command.routine.replay_cap, command.routine.created_at, command.routine.updated_at],
        ).context("creating routine")?;
        tx.execute(
            "INSERT INTO execass_routine_versions(routine_id,routine_version,source_delegation_id,saved_owner_authority_provenance_id,normalized_original_intent,resolved_leaf_manifest_json,manifest_digest,saved_selector_json,saved_action_envelope_json,accepted_confirmation_grant_id,effective_policy_snapshot_json,effective_policy_revision,stable_leaf_digest,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![command.version.routine_id, command.version.routine_version, command.version.source_delegation_id, command.version.saved_owner_authority_provenance_id, command.version.normalized_original_intent, command.version.resolved_leaf_manifest_json, command.version.manifest_digest, command.version.saved_selector_json, command.version.saved_action_envelope_json, command.version.accepted_confirmation_grant_id, command.version.effective_policy_snapshot_json, command.version.effective_policy_revision, command.version.stable_leaf_digest, command.version.created_at],
        ).context("creating immutable routine version")?;
        tx.execute(
            "INSERT INTO execass_routine_schedule_state(routine_id,local_hour,local_minute,last_evaluated_instant_ms,updated_at) VALUES(?1,?2,?3,NULL,?4)",
            params![command.routine.routine_id, command.schedule.local_hour, command.schedule.local_minute, command.routine.updated_at],
        ).context("creating routine schedule cursor")?;
        create_driver_job(&tx, &command.routine.routine_id, command.routine.created_at)?;
        tx.commit().context("committing routine creation")?;
        Ok(command.routine.clone())
    }

    pub fn amend_routine(&self, command: &AmendRoutineCommand) -> Result<bool> {
        let create = canonicalize_create(&CreateRoutineCommand {
            routine: command.routine.clone(),
            version: command.version.clone(),
            schedule: command.schedule.clone(),
        })?;
        if create.routine.current_version != command.expected_current_version + 1 {
            bail!("routine amendment must create exactly the next immutable version");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        validate_source_anchor(&tx, &create.version)?;
        validate_grant_binding(&tx, &create.version)?;
        let previous = version_at(
            &tx,
            &create.routine.routine_id,
            command.expected_current_version,
        )?
        .context("routine amendment source version does not exist")?;
        validate_amendment_source(&previous, &create.version)?;
        let changed = tx.execute("UPDATE execass_routines SET current_version=?1,enabled=?2,timezone=?3,overlap_policy=?4,catch_up_policy=?5,replay_cap=?6,updated_at=MAX(updated_at,?7) WHERE routine_id=?8 AND current_version=?9", params![create.routine.current_version,i64::from(create.routine.enabled),create.routine.timezone,create.routine.overlap_policy.as_str(),create.routine.catch_up_policy.as_str(),create.routine.replay_cap,create.routine.updated_at,create.routine.routine_id,command.expected_current_version])?;
        if changed != 1 {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute("INSERT INTO execass_routine_versions(routine_id,routine_version,source_delegation_id,saved_owner_authority_provenance_id,normalized_original_intent,resolved_leaf_manifest_json,manifest_digest,saved_selector_json,saved_action_envelope_json,accepted_confirmation_grant_id,effective_policy_snapshot_json,effective_policy_revision,stable_leaf_digest,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)", params![create.version.routine_id,create.version.routine_version,create.version.source_delegation_id,create.version.saved_owner_authority_provenance_id,create.version.normalized_original_intent,create.version.resolved_leaf_manifest_json,create.version.manifest_digest,create.version.saved_selector_json,create.version.saved_action_envelope_json,create.version.accepted_confirmation_grant_id,create.version.effective_policy_snapshot_json,create.version.effective_policy_revision,create.version.stable_leaf_digest,create.version.created_at])?;
        tx.execute("UPDATE execass_routine_schedule_state SET local_hour=?1,local_minute=?2,updated_at=MAX(updated_at,?3) WHERE routine_id=?4", params![create.schedule.local_hour,create.schedule.local_minute,create.routine.updated_at,create.routine.routine_id])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn set_routine_enabled(
        &self,
        routine_id: &str,
        enabled: bool,
        trusted_now: i64,
    ) -> Result<bool> {
        require_text("routine_id", routine_id)?;
        if trusted_now <= 0 {
            bail!("routine pause/resume requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let changed = tx.execute(
            "UPDATE execass_routines SET enabled=?1,updated_at=MAX(updated_at,?2) WHERE routine_id=?3 AND enabled<>?1",
            params![i64::from(enabled), trusted_now, routine_id],
        )?;
        tx.commit()?;
        Ok(changed == 1)
    }

    /// Persists deterministic due occurrences and their immutable typed-job
    /// trigger bindings. No delegation, continuation, or executable admission
    /// is created here.
    pub fn materialize_due_routine_occurrences(
        &self,
        claim: &RoutineDriverClaim,
    ) -> Result<Vec<RoutineOccurrenceRecord>> {
        require_text("routine_id", &claim.routine_id)?;
        require_text("driver_job_id", &claim.driver_job_id)?;
        require_text("driver_lease_owner", &claim.driver_lease_owner)?;
        if claim.trusted_now <= 0 || claim.driver_lease_expires_at <= claim.trusted_now {
            bail!("routine reservation requires a live positive driver lease");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let expected_payload = routine_driver_payload(&claim.routine_id)?;
        let driver_matches = tx
            .query_row(
                "SELECT 1 FROM execass_routine_driver_jobs b JOIN jobs j ON j.job_id=b.job_id WHERE b.routine_id=?1 AND b.job_id=?2 AND j.payload_json=?3 AND j.schedule_kind='execass_routine_driver' AND j.interval_seconds=60 AND j.lease_owner=?4 AND j.lease_expires_at=?5 AND j.lease_expires_at>?6 AND j.enabled=1 AND j.deleted_at IS NULL",
                params![claim.routine_id, claim.driver_job_id, expected_payload, claim.driver_lease_owner, claim.driver_lease_expires_at, claim.trusted_now],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !driver_matches {
            bail!("routine driver lease or immutable binding is invalid");
        }
        let routine = routine_at(&tx, &claim.routine_id)?.context("routine does not exist")?;
        let cursor: Option<i64> = tx.query_row("SELECT last_evaluated_instant_ms FROM execass_routine_schedule_state WHERE routine_id=?1", [&claim.routine_id], |row| row.get(0))?;
        if cursor.is_some_and(|value| claim.trusted_now <= value) {
            advance_driver_job(&tx, claim)?;
            tx.commit()?;
            return Ok(Vec::new());
        }
        if !routine.enabled {
            tx.execute("UPDATE execass_routine_schedule_state SET last_evaluated_instant_ms=?1,updated_at=MAX(updated_at,?1) WHERE routine_id=?2", params![claim.trusted_now,claim.routine_id])?;
            advance_driver_job(&tx, claim)?;
            tx.commit()?;
            return Ok(Vec::new());
        }
        let version = version_at(&tx, &claim.routine_id, routine.current_version)?
            .context("routine current version is missing")?;
        let agent_exists = tx
            .query_row(
                "SELECT 1 FROM agents WHERE agent_id=?1 AND archived_at IS NULL",
                [EXECASS_JOB_AGENT_ID],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !agent_exists {
            bail!("canonical ExecAss job agent is unavailable");
        }

        let (hour, minute): (u32,u32) = tx.query_row("SELECT local_hour,local_minute FROM execass_routine_schedule_state WHERE routine_id=?1", [&claim.routine_id], |row| Ok((row.get(0)?,row.get(1)?)))?;
        let candidates = due_candidates(&routine, cursor, claim.trusted_now, hour, minute)?;
        let selected =
            select_catch_up_occurrences(routine.catch_up_policy, &candidates, routine.replay_cap)?;
        let records = reserve_candidates(&tx, &routine, &version, &selected, claim.trusted_now)?;
        tx.execute("UPDATE execass_routine_schedule_state SET last_evaluated_instant_ms=?1,updated_at=MAX(updated_at,?1) WHERE routine_id=?2", params![claim.trusted_now,claim.routine_id])?;
        advance_driver_job(&tx, claim)?;
        tx.commit()
            .context("committing routine occurrence materialization")?;
        Ok(records)
    }

    pub fn settle_routine_trigger(
        &self,
        command: &RoutineTriggerSettlementCommand,
    ) -> Result<RoutineTriggerSettlementOutcome> {
        require_text("occurrence_id", &command.occurrence_id)?;
        require_text("trigger_job_id", &command.trigger_job_id)?;
        require_text("trigger_lease_owner", &command.trigger_lease_owner)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let existing: Option<String> = tx.query_row("SELECT operation_id FROM execass_routine_trigger_operations WHERE occurrence_id=?1", [&command.occurrence_id], |row| row.get(0)).optional()?;
        if existing.is_some() {
            tx.commit()?;
            return Ok(RoutineTriggerSettlementOutcome::Replayed);
        }
        let valid = tx.query_row("SELECT 1 FROM execass_routine_occurrences o JOIN execass_routine_job_bindings b ON b.occurrence_id=o.occurrence_id JOIN jobs j ON j.job_id=b.job_id WHERE o.occurrence_id=?1 AND o.status='admission_planned' AND b.job_id=?2 AND j.lease_owner=?3 AND j.lease_expires_at=?4 AND j.lease_expires_at>?5 AND j.deleted_at IS NULL", params![command.occurrence_id,command.trigger_job_id,command.trigger_lease_owner,command.trigger_lease_expires_at,command.trusted_now], |_| Ok(())).optional()?.is_some();
        if !valid {
            tx.commit()?;
            return Ok(RoutineTriggerSettlementOutcome::Refused {
                reason: "trigger_lease_or_admission_invalid".into(),
            });
        }
        let operation_id = format!("routine-trigger-settle:{}", command.occurrence_id);
        tx.execute("INSERT INTO execass_routine_trigger_operations(operation_id,occurrence_id,job_id,operation,lease_owner,lease_expires_at,occurred_at) VALUES(?1,?2,?3,'settle_trigger',?4,?5,?6)",params![operation_id,command.occurrence_id,command.trigger_job_id,command.trigger_lease_owner,command.trigger_lease_expires_at,command.trusted_now])?;
        tx.execute("UPDATE jobs SET enabled=0,next_run_at=NULL,lease_owner=NULL,lease_expires_at=NULL,updated_at=MAX(updated_at,?1) WHERE job_id=?2",params![command.trusted_now,command.trigger_job_id])?;
        tx.commit()?;
        Ok(RoutineTriggerSettlementOutcome::Settled)
    }

    // Candidate reservation is intentionally transaction-private: callers must
    // use the persisted timezone/schedule/cursor materializer above.
}

fn reserve_candidates(
    tx: &Transaction<'_>,
    routine: &RoutineRecord,
    version: &RoutineVersionRecord,
    candidates: &[RoutineOccurrenceCandidate],
    trusted_now: i64,
) -> Result<Vec<RoutineOccurrenceRecord>> {
    let routine_id = &routine.routine_id;
    let mut records = Vec::new();
    for candidate in candidates {
        if candidate.scheduled_instant_ms <= 0 {
            bail!("routine scheduled instant must be positive");
        }
        require_text("routine scheduled local time", &candidate.scheduled_local)?;
        let occurrence_id = deterministic_routine_occurrence_id(
            routine_id,
            routine.current_version,
            candidate.scheduled_instant_ms,
        );
        tx.execute(
                "INSERT OR IGNORE INTO execass_routine_occurrences(occurrence_id,routine_id,routine_version,scheduled_instant_ms,scheduled_local,utc_offset_seconds,time_resolution,effective_policy_revision,status,admission_plan_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'planned',NULL,?9,?9)",
                params![occurrence_id, routine_id, routine.current_version, candidate.scheduled_instant_ms, candidate.scheduled_local, candidate.utc_offset_seconds, candidate.time_resolution.as_str(), version.effective_policy_revision, trusted_now],
            )?;
        let record =
            occurrence_at(tx, &occurrence_id)?.context("routine occurrence disappeared")?;
        if record.routine_id != *routine_id
            || record.routine_version != routine.current_version
            || record.scheduled_instant_ms != candidate.scheduled_instant_ms
        {
            bail!("routine occurrence identity collision");
        }
        let job_id = deterministic_routine_trigger_job_id(&occurrence_id);
        let payload_json = routine_trigger_payload(&record)?;
        tx.execute(
                "INSERT OR IGNORE INTO jobs(job_id,agent_id,name,enabled,schedule_kind,interval_seconds,run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,lease_owner,lease_expires_at,last_run_at,last_error,created_at,updated_at,deleted_at) VALUES(?1,?2,?3,1,?4,NULL,?5,?5,?6,0,1000,30000,NULL,NULL,NULL,NULL,?7,?7,NULL)",
                params![job_id, EXECASS_JOB_AGENT_ID, format!("ExecAss routine trigger {occurrence_id}"), EXECASS_ROUTINE_TRIGGER_SCHEDULE_KIND, candidate.scheduled_instant_ms, payload_json, trusted_now],
            )?;
        let exact_job = tx.query_row("SELECT 1 FROM jobs WHERE job_id=?1 AND agent_id=?2 AND enabled=1 AND schedule_kind=?3 AND interval_seconds IS NULL AND run_at_ms=?4 AND next_run_at=?4 AND payload_json=?5 AND max_retries=0 AND deleted_at IS NULL", params![job_id, EXECASS_JOB_AGENT_ID, EXECASS_ROUTINE_TRIGGER_SCHEDULE_KIND, candidate.scheduled_instant_ms, payload_json], |_| Ok(())).optional()?.is_some();
        if !exact_job {
            bail!("routine trigger job identity collision");
        }
        tx.execute("INSERT OR IGNORE INTO execass_routine_job_bindings(occurrence_id,job_id,created_at) VALUES(?1,?2,?3)", params![occurrence_id, job_id, trusted_now])?;
        let binding_matches = tx
            .query_row(
                "SELECT 1 FROM execass_routine_job_bindings WHERE occurrence_id=?1 AND job_id=?2",
                params![occurrence_id, job_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !binding_matches {
            bail!("routine trigger binding collision");
        }
        records.push(record);
    }
    Ok(records)
}

impl ExecAssStore {
    /// Atomically verifies a claimed reserved trigger, the current routine
    /// version, global stop, and current policy revision before recording a
    /// non-executable admission plan. Parent integration must turn this plan
    /// into the distinct delegation/continuation transaction.
    pub fn plan_routine_occurrence_admission(
        &self,
        request: &RoutineAdmissionRequest,
    ) -> Result<RoutineAdmissionOutcome> {
        require_text("occurrence_id", &request.occurrence_id)?;
        require_text("trigger_job_id", &request.trigger_job_id)?;
        require_text("trigger_lease_owner", &request.trigger_lease_owner)?;
        if request.trusted_now <= 0 || request.trigger_lease_expires_at <= request.trusted_now {
            bail!("routine admission requires a live positive job lease");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(occurrence) = occurrence_at(&tx, &request.occurrence_id)? else {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "occurrence_not_found".into(),
            });
        };
        let Some(routine) = routine_at(&tx, &occurrence.routine_id)? else {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "routine_not_found".into(),
            });
        };
        if !routine.enabled {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "routine_paused".into(),
            });
        }
        if routine.current_version != occurrence.routine_version {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "routine_version_superseded".into(),
            });
        }
        let version = version_at(&tx, &routine.routine_id, routine.current_version)?
            .context("routine current version is missing")?;
        let (engaged, current_policy_revision): (i64, i64) = tx.query_row("SELECT engaged,current_policy_revision FROM execass_global_runtime_control WHERE singleton=1", [], |row| Ok((row.get(0)?,row.get(1)?)))?;
        if engaged != 0 {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "global_stop_engaged".into(),
            });
        }
        if current_policy_revision != version.effective_policy_revision
            || occurrence.effective_policy_revision != current_policy_revision
        {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "current_policy_changed".into(),
            });
        }
        let expected_payload = routine_trigger_payload(&occurrence)?;
        let job_matches = tx.query_row("SELECT 1 FROM execass_routine_job_bindings b JOIN jobs j ON j.job_id=b.job_id WHERE b.occurrence_id=?1 AND b.job_id=?2 AND j.payload_json=?3 AND j.lease_owner=?4 AND j.lease_expires_at=?5 AND j.lease_expires_at>?6 AND j.deleted_at IS NULL", params![occurrence.occurrence_id, request.trigger_job_id, expected_payload, request.trigger_lease_owner, request.trigger_lease_expires_at, request.trusted_now], |_| Ok(())).optional()?.is_some();
        if !job_matches {
            tx.commit()?;
            return Ok(RoutineAdmissionOutcome::Refused {
                reason: "trigger_lease_or_binding_invalid".into(),
            });
        }
        let plan_json = json!({"kind":"execass.routine_admission_plan.v1","occurrence_id":occurrence.occurrence_id,"routine_id":occurrence.routine_id,"routine_version":occurrence.routine_version,"scheduled_instant_ms":occurrence.scheduled_instant_ms,"effective_policy_revision":current_policy_revision,"effective_policy_snapshot":serde_json::from_str::<serde_json::Value>(&version.effective_policy_snapshot_json)?}).to_string();
        let updated = tx.execute("UPDATE execass_routine_occurrences SET status='admission_planned',admission_plan_json=?1,updated_at=MAX(updated_at,?2) WHERE occurrence_id=?3 AND status='planned'", params![plan_json, request.trusted_now, occurrence.occurrence_id])?;
        let planned = occurrence_at(&tx, &occurrence.occurrence_id)?
            .context("routine occurrence disappeared after admission")?;
        tx.commit().context("committing routine admission plan")?;
        let replayed = updated == 0 && planned.status == RoutineOccurrenceStatus::AdmissionPlanned;
        let plan = RoutineAdmissionPlan {
            occurrence: planned,
            routine_version: version,
            trigger_job_id: request.trigger_job_id.clone(),
        };
        Ok(if updated == 1 {
            RoutineAdmissionOutcome::Planned(Box::new(plan))
        } else if replayed {
            RoutineAdmissionOutcome::Replayed(Box::new(plan))
        } else {
            RoutineAdmissionOutcome::Refused {
                reason: "occurrence_not_admissible".into(),
            }
        })
    }
}

fn canonicalize_create(command: &CreateRoutineCommand) -> Result<CreateRoutineCommand> {
    let mut canonical = command.clone();
    canonical.version.resolved_leaf_manifest_json =
        canonical_json(&command.version.resolved_leaf_manifest_json)?;
    canonical.version.saved_selector_json = canonical_json(&command.version.saved_selector_json)?;
    canonical.version.saved_action_envelope_json =
        canonical_json(&command.version.saved_action_envelope_json)?;
    canonical.version.effective_policy_snapshot_json =
        canonical_json(&command.version.effective_policy_snapshot_json)?;
    validate_create(&canonical)?;
    Ok(canonical)
}

fn canonical_json(value: &str) -> Result<String> {
    String::from_utf8(parse_strict_json(value)?.to_bytes()).context("canonical JSON is not UTF-8")
}

fn validate_create(command: &CreateRoutineCommand) -> Result<()> {
    require_text("routine_id", &command.routine.routine_id)?;
    require_text(
        "source_delegation_id",
        &command.version.source_delegation_id,
    )?;
    require_text(
        "saved_owner_authority_provenance_id",
        &command.version.saved_owner_authority_provenance_id,
    )?;
    require_text(
        "normalized_original_intent",
        &command.version.normalized_original_intent,
    )?;
    if command.routine.current_version != command.version.routine_version
        || command.routine.routine_id != command.version.routine_id
    {
        bail!("routine current version must match its immutable version");
    }
    if !(1..=10).contains(&command.routine.replay_cap) {
        bail!("routine replay cap must be between 1 and 10");
    }
    if command.routine.created_at <= 0
        || command.routine.updated_at < command.routine.created_at
        || command.version.created_at <= 0
    {
        bail!("routine timestamps are invalid");
    }
    if command.schedule.local_hour > 23 || command.schedule.local_minute > 59 {
        bail!("routine local schedule is invalid");
    }
    Tz::from_str(&command.routine.timezone).context("routine timezone must be an IANA timezone")?;
    if command.version.stable_leaf_digest.len() != 64
        || !command
            .version
            .stable_leaf_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        bail!("routine stable leaf digest must be lowercase SHA-256 hex");
    }
    if command.version.manifest_digest.len() != 64
        || !command
            .version
            .manifest_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        bail!("routine manifest digest must be lowercase SHA-256 hex");
    }
    let computed_manifest_digest = format!(
        "{:x}",
        Sha256::digest(command.version.resolved_leaf_manifest_json.as_bytes())
    );
    if computed_manifest_digest != command.version.manifest_digest {
        bail!("routine manifest digest does not match canonical manifest bytes");
    }
    for (field, value) in [
        (
            "resolved_leaf_manifest_json",
            &command.version.resolved_leaf_manifest_json,
        ),
        ("saved_selector_json", &command.version.saved_selector_json),
        (
            "saved_action_envelope_json",
            &command.version.saved_action_envelope_json,
        ),
        (
            "effective_policy_snapshot_json",
            &command.version.effective_policy_snapshot_json,
        ),
    ] {
        parse_strict_json(value).with_context(|| format!("{field} must be canonical JSON"))?;
    }
    Ok(())
}

fn validate_source_anchor(tx: &Transaction<'_>, version: &RoutineVersionRecord) -> Result<()> {
    let source: Option<(String, String, ActorType, String, String)> = tx
        .query_row(
            "SELECT d.authority_provenance_id,d.normalized_original_intent,a.actor_type,p.resolved_leaf_manifest_json,p.manifest_digest FROM execass_delegations d JOIN execass_authority_provenance a ON a.authority_provenance_id=d.authority_provenance_id JOIN execass_plans p ON p.delegation_id=d.delegation_id AND p.plan_revision=d.current_plan_revision WHERE d.delegation_id=?1",
            [&version.source_delegation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?;
    let Some((authority_id, normalized_intent, actor_type, manifest_json, manifest_digest)) =
        source
    else {
        bail!("routine source delegation with a current admitted plan does not exist");
    };
    if authority_id != version.saved_owner_authority_provenance_id {
        bail!("routine saved owner authority does not match source delegation authority");
    }
    if !matches!(actor_type, ActorType::HumanLocal | ActorType::HumanRemote) {
        bail!("routine source authority must be a human owner");
    }
    if normalized_intent != version.normalized_original_intent {
        bail!("routine normalized intent does not match source delegation intent");
    }
    if canonical_json(&manifest_json)? != version.resolved_leaf_manifest_json
        || manifest_digest != version.manifest_digest
    {
        bail!("routine manifest does not match the source delegation current plan");
    }
    Ok(())
}

fn validate_amendment_source(
    previous: &RoutineVersionRecord,
    next: &RoutineVersionRecord,
) -> Result<()> {
    let material_action_changed = previous.saved_selector_json != next.saved_selector_json
        || previous.saved_action_envelope_json != next.saved_action_envelope_json
        || previous.stable_leaf_digest != next.stable_leaf_digest;
    if material_action_changed
        && previous.source_delegation_id == next.source_delegation_id
        && previous.manifest_digest == next.manifest_digest
    {
        bail!("material routine action amendment requires a newly admitted source plan");
    }
    Ok(())
}

fn validate_grant_binding(tx: &Transaction<'_>, version: &RoutineVersionRecord) -> Result<()> {
    let Some(grant_id) = &version.accepted_confirmation_grant_id else {
        return Ok(());
    };
    let grant: Option<(String, String, Option<i64>)> = tx.query_row("SELECT delegation_id,canonical_action_envelope_or_selector_json,invalidated_at FROM execass_accepted_confirmation_grants WHERE grant_id=?1", [grant_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    let Some((delegation_id, canonical_envelope, invalidated_at)) = grant else {
        bail!("routine accepted confirmation grant does not exist");
    };
    if invalidated_at.is_some() {
        bail!("routine accepted confirmation grant is not active");
    }
    if delegation_id != version.source_delegation_id {
        bail!("routine accepted confirmation grant belongs to another delegation");
    }
    if canonical_json(&canonical_envelope)? != version.saved_action_envelope_json {
        bail!("routine accepted confirmation grant envelope does not match saved routine envelope");
    }
    Ok(())
}

fn due_candidates(
    routine: &RoutineRecord,
    cursor: Option<i64>,
    trusted_now: i64,
    hour: u32,
    minute: u32,
) -> Result<Vec<RoutineOccurrenceCandidate>> {
    let tz = Tz::from_str(&routine.timezone)?;
    let start = cursor.unwrap_or(routine.created_at);
    if trusted_now <= start {
        return Ok(Vec::new());
    }
    let mut day = DateTime::from_timestamp_millis(start)
        .context("routine cursor timestamp is invalid")?
        .with_timezone(&tz)
        .date_naive();
    let end = DateTime::from_timestamp_millis(trusted_now)
        .context("routine trusted timestamp is invalid")?
        .with_timezone(&tz)
        .date_naive();
    let mut due = Vec::new();
    while day <= end {
        let candidate = resolve_routine_local_time(
            &routine.timezone,
            day,
            hour,
            minute,
            routine.overlap_policy,
        )?;
        if candidate.scheduled_instant_ms > start && candidate.scheduled_instant_ms <= trusted_now {
            due.push(candidate);
        }
        day = day
            .checked_add_signed(Duration::days(1))
            .context("routine date cursor overflowed")?;
    }
    Ok(due)
}

fn create_driver_job(tx: &Transaction<'_>, routine_id: &str, created_at: i64) -> Result<()> {
    let job_id = format!(
        "execass-routine-driver-{:x}",
        Sha256::digest(routine_id.as_bytes())
    );
    let payload = routine_driver_payload(routine_id)?;
    tx.execute("INSERT INTO jobs(job_id,agent_id,name,enabled,schedule_kind,interval_seconds,run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,lease_owner,lease_expires_at,last_run_at,last_error,created_at,updated_at,deleted_at) VALUES(?1,?2,?3,1,'execass_routine_driver',60,NULL,?4,?5,0,1000,30000,NULL,NULL,NULL,NULL,?4,?4,NULL)",params![job_id,EXECASS_JOB_AGENT_ID,format!("ExecAss routine driver {routine_id}"),created_at,payload])?;
    tx.execute(
        "INSERT INTO execass_routine_driver_jobs(routine_id,job_id,created_at) VALUES(?1,?2,?3)",
        params![routine_id, job_id, created_at],
    )?;
    Ok(())
}

fn advance_driver_job(tx: &Transaction<'_>, claim: &RoutineDriverClaim) -> Result<()> {
    let operation_id = format!(
        "routine-driver-advance:{}:{}",
        claim.routine_id, claim.trusted_now
    );
    tx.execute("INSERT INTO execass_routine_trigger_operations(operation_id,occurrence_id,job_id,operation,lease_owner,lease_expires_at,occurred_at) VALUES(?1,NULL,?2,'advance_driver',?3,?4,?5)",params![operation_id,claim.driver_job_id,claim.driver_lease_owner,claim.driver_lease_expires_at,claim.trusted_now])?;
    let changed = tx.execute(
        "UPDATE jobs SET next_run_at=?1,lease_owner=NULL,lease_expires_at=NULL,updated_at=?2 WHERE job_id=?3 AND lease_owner=?4 AND lease_expires_at=?5",
        params![claim.trusted_now + 60_000, claim.trusted_now, claim.driver_job_id, claim.driver_lease_owner, claim.driver_lease_expires_at],
    )?;
    if changed != 1 {
        bail!("routine driver lease changed before settlement");
    }
    Ok(())
}

fn routine_driver_payload(routine_id: &str) -> Result<String> {
    serde_json::to_string(&RoutineDriverPayload {
        mode: EXECASS_ROUTINE_DRIVER_MODE,
        routine_id,
    })
    .map_err(Into::into)
}

pub(super) fn routine_trigger_payload(occurrence: &RoutineOccurrenceRecord) -> Result<String> {
    serde_json::to_string(&RoutineTriggerPayload {
        mode: EXECASS_ROUTINE_TRIGGER_MODE,
        occurrence_id: &occurrence.occurrence_id,
        routine_id: &occurrence.routine_id,
        routine_version: occurrence.routine_version,
        scheduled_instant_ms: occurrence.scheduled_instant_ms,
    })
    .map_err(Into::into)
}

fn routine_at(conn: &Transaction<'_>, routine_id: &str) -> Result<Option<RoutineRecord>> {
    conn.query_row("SELECT routine_id,current_version,enabled,timezone,overlap_policy,catch_up_policy,replay_cap,created_at,updated_at FROM execass_routines WHERE routine_id=?1", [routine_id], |row| Ok(RoutineRecord { routine_id: row.get(0)?, current_version: row.get(1)?, enabled: row.get::<_,i64>(2)? != 0, timezone: row.get(3)?, overlap_policy: row.get(4)?, catch_up_policy: row.get(5)?, replay_cap: row.get(6)?, created_at: row.get(7)?, updated_at: row.get(8)? })).optional().map_err(Into::into)
}

fn version_at(
    conn: &Transaction<'_>,
    routine_id: &str,
    routine_version: i64,
) -> Result<Option<RoutineVersionRecord>> {
    conn.query_row("SELECT routine_id,routine_version,source_delegation_id,saved_owner_authority_provenance_id,normalized_original_intent,resolved_leaf_manifest_json,manifest_digest,saved_selector_json,saved_action_envelope_json,accepted_confirmation_grant_id,effective_policy_snapshot_json,effective_policy_revision,stable_leaf_digest,created_at FROM execass_routine_versions WHERE routine_id=?1 AND routine_version=?2", params![routine_id,routine_version], |row| Ok(RoutineVersionRecord { routine_id: row.get(0)?, routine_version: row.get(1)?, source_delegation_id: row.get(2)?, saved_owner_authority_provenance_id: row.get(3)?, normalized_original_intent: row.get(4)?, resolved_leaf_manifest_json: row.get(5)?, manifest_digest: row.get(6)?, saved_selector_json: row.get(7)?, saved_action_envelope_json: row.get(8)?, accepted_confirmation_grant_id: row.get(9)?, effective_policy_snapshot_json: row.get(10)?, effective_policy_revision: row.get(11)?, stable_leaf_digest: row.get(12)?, created_at: row.get(13)? })).optional().map_err(Into::into)
}

fn occurrence_at(
    conn: &Transaction<'_>,
    occurrence_id: &str,
) -> Result<Option<RoutineOccurrenceRecord>> {
    conn.query_row("SELECT occurrence_id,routine_id,routine_version,scheduled_instant_ms,scheduled_local,utc_offset_seconds,time_resolution,effective_policy_revision,status,admission_plan_json,admitted_delegation_id,created_at,updated_at FROM execass_routine_occurrences WHERE occurrence_id=?1", [occurrence_id], |row| Ok(RoutineOccurrenceRecord { occurrence_id: row.get(0)?, routine_id: row.get(1)?, routine_version: row.get(2)?, scheduled_instant_ms: row.get(3)?, scheduled_local: row.get(4)?, utc_offset_seconds: row.get(5)?, time_resolution: row.get(6)?, effective_policy_revision: row.get(7)?, status: row.get(8)?, admission_plan_json: row.get(9)?, admitted_delegation_id: row.get(10)?, created_at: row.get(11)?, updated_at: row.get(12)? })).optional().map_err(Into::into)
}
