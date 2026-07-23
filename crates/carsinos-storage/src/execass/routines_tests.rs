use super::routines::{
    deterministic_routine_occurrence_id, resolve_routine_local_time, select_catch_up_occurrences,
};
use super::tests::{prepared_attested_confirmation, table_count, Fixture};
use super::*;
use chrono::NaiveDate;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Barrier};

const NOW: i64 = 1_800_000_000_000;

pub(super) fn anchored_fixture() -> Fixture {
    let (fixture, _, _, _) = prepared_attested_confirmation();
    fixture
}

fn source_anchor(fixture: &Fixture, delegation_id: &str) -> (String, String, String, String) {
    fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT d.authority_provenance_id,d.normalized_original_intent,p.resolved_leaf_manifest_json,p.manifest_digest FROM execass_delegations d JOIN execass_plans p ON p.delegation_id=d.delegation_id AND p.plan_revision=d.current_plan_revision WHERE d.delegation_id=?1",
            [delegation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
}

fn install_plan_for_existing_source(fixture: &Fixture, delegation_id: &str, plan_id: &str) {
    let (_, _, manifest_json, manifest_digest) = source_anchor(fixture, "delegation-1");
    let conn = fixture.store.connection().unwrap();
    let authority_id: String = conn
        .query_row(
            "SELECT authority_provenance_id FROM execass_delegations WHERE delegation_id=?1",
            [delegation_id],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO execass_plans(plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,plan_summary,resolved_leaf_manifest_json,manifest_digest,created_by_authority_provenance_id,created_at) VALUES(?1,?2,1,1,1,'routine source test plan',?3,?4,?5,?6)",
        params![plan_id, delegation_id, manifest_json, manifest_digest, authority_id, NOW],
    )
    .unwrap();
    conn.execute(
        "UPDATE execass_delegations SET state_revision=state_revision+1,current_plan_revision=1,updated_at=MAX(updated_at,?1) WHERE delegation_id=?2",
        params![NOW, delegation_id],
    )
    .unwrap();
}

fn install_second_human_source(fixture: &Fixture) {
    let conn = fixture.store.connection().unwrap();
    conn.execute(
        "INSERT INTO execass_delegations(delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,ingress_credential_identity,source_correlation_id,ingress_idempotency_key,classifier_version,classifier_reasons_json,phase,run_control,state_revision,policy_revision,effective_authority_json,authority_provenance_id,stop_epoch,receipt_chain_count,created_at,updated_at) SELECT 'delegation-2',normalized_original_intent,intake_evidence_json,ingress_source,ingress_credential_identity,'routine-source-correlation-2','routine-source-idempotency-2',classifier_version,classifier_reasons_json,'accepted','running',1,policy_revision,effective_authority_json,authority_provenance_id,0,0,created_at,updated_at FROM execass_delegations WHERE delegation_id='delegation-1'",
        [],
    )
    .unwrap();
    install_plan_for_existing_source(fixture, "delegation-2", "routine-source-plan-2");
}

pub(super) fn command(fixture: &Fixture, id: &str) -> CreateRoutineCommand {
    let (authority_id, normalized_intent, manifest_json, manifest_digest) =
        source_anchor(fixture, "delegation-1");
    CreateRoutineCommand {
        routine: RoutineRecord {
            routine_id: id.into(),
            current_version: 1,
            enabled: true,
            timezone: "America/Chicago".into(),
            overlap_policy: RoutineOverlapPolicy::Earlier,
            catch_up_policy: RoutineCatchUpPolicy::Replay,
            replay_cap: 10,
            created_at: NOW,
            updated_at: NOW,
        },
        version: RoutineVersionRecord {
            routine_id: id.into(),
            routine_version: 1,
            source_delegation_id: "delegation-1".into(),
            saved_owner_authority_provenance_id: authority_id,
            normalized_original_intent: normalized_intent,
            resolved_leaf_manifest_json: manifest_json,
            manifest_digest,
            saved_selector_json: r#"{"selector":"saved"}"#.into(),
            saved_action_envelope_json: r#"{"tool":"example.v1"}"#.into(),
            accepted_confirmation_grant_id: None,
            effective_policy_snapshot_json: r#"{"revision":1}"#.into(),
            effective_policy_revision: 1,
            stable_leaf_digest: "a".repeat(64),
            created_at: NOW,
        },
        schedule: RoutineScheduleSpec {
            local_hour: 2,
            local_minute: 30,
        },
    }
}

fn candidate(day: i64) -> RoutineOccurrenceCandidate {
    RoutineOccurrenceCandidate {
        scheduled_instant_ms: NOW + day * 86_400_000,
        scheduled_local: format!("2027-01-{day:02}T02:30"),
        utc_offset_seconds: -21_600,
        time_resolution: RoutineTimeResolution::Single,
    }
}

fn try_driver_claim(
    fixture: &Fixture,
    routine_id: &str,
    trusted_now: i64,
    worker_id: &str,
) -> Option<RoutineDriverClaim> {
    let driver_job_id: String = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT job_id FROM execass_routine_driver_jobs WHERE routine_id=?1",
            [routine_id],
            |row| row.get(0),
        )
        .unwrap();
    let job = crate::Storage::from_paths(&fixture.paths)
        .acquire_due_jobs(worker_id, trusted_now, 30_000, 64)
        .unwrap()
        .into_iter()
        .find(|job| job.job_id == driver_job_id)?;
    Some(RoutineDriverClaim {
        routine_id: routine_id.into(),
        driver_job_id,
        driver_lease_owner: job.lease_owner.unwrap(),
        driver_lease_expires_at: job.lease_expires_at.unwrap(),
        trusted_now,
    })
}

pub(super) fn materialize(
    fixture: &Fixture,
    routine_id: &str,
    trusted_now: i64,
    worker_id: &str,
) -> Vec<RoutineOccurrenceRecord> {
    let claim = try_driver_claim(fixture, routine_id, trusted_now, worker_id)
        .expect("reserved routine driver must be due and acquired");
    fixture
        .store
        .materialize_due_routine_occurrences(&claim)
        .unwrap()
}

#[test]
fn dst_gap_advances_and_overlap_selects_explicit_offset() {
    let gap = resolve_routine_local_time(
        "America/Chicago",
        NaiveDate::from_ymd_opt(2026, 3, 8).unwrap(),
        2,
        30,
        RoutineOverlapPolicy::Earlier,
    )
    .unwrap();
    assert_eq!(gap.scheduled_local, "2026-03-08T03:00");
    let earlier = resolve_routine_local_time(
        "America/Chicago",
        NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        1,
        30,
        RoutineOverlapPolicy::Earlier,
    )
    .unwrap();
    let later = resolve_routine_local_time(
        "America/Chicago",
        NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        1,
        30,
        RoutineOverlapPolicy::Later,
    )
    .unwrap();
    assert_eq!(
        later.scheduled_instant_ms - earlier.scheduled_instant_ms,
        3_600_000
    );
}

#[test]
fn catch_up_modes_are_bounded_at_ten() {
    let due = (1..=11).map(candidate).collect::<Vec<_>>();
    assert!(
        select_catch_up_occurrences(RoutineCatchUpPolicy::Skip, &due, 10)
            .unwrap()
            .is_empty()
    );
    let latest = select_catch_up_occurrences(RoutineCatchUpPolicy::LatestOnly, &due, 10).unwrap();
    assert_eq!(latest, vec![due[10].clone()]);
    let replay = select_catch_up_occurrences(RoutineCatchUpPolicy::Replay, &due, 10).unwrap();
    assert_eq!(replay.len(), 10);
    assert_eq!(replay[0], due[1]);
    assert!(select_catch_up_occurrences(RoutineCatchUpPolicy::Replay, &due, 11).is_err());
}

#[test]
fn occurrence_identity_survives_clock_rollback_and_concurrent_reservation() {
    let fixture = anchored_fixture();
    fixture
        .store
        .create_routine(&command(&fixture, "routine-clock"))
        .unwrap();
    let first = materialize(
        &fixture,
        "routine-clock",
        NOW + 3 * 86_400_000,
        "worker-first",
    );
    assert!(!first.is_empty());
    assert_eq!(
        first[0].occurrence_id,
        deterministic_routine_occurrence_id("routine-clock", 1, first[0].scheduled_instant_ms)
    );
    assert!(try_driver_claim(&fixture, "routine-clock", NOW - 1, "worker-rollback").is_none());
    let barrier = Arc::new(Barrier::new(3));
    let workers = (0..2)
        .map(|_| {
            let paths = fixture.paths.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || -> bool {
                let store = ExecAssStore::open(&paths).unwrap();
                let scheduler = crate::Storage::from_paths(&paths);
                let driver_job_id: String = store.connection().unwrap().query_row(
                    "SELECT job_id FROM execass_routine_driver_jobs WHERE routine_id='routine-clock'",
                    [],
                    |row| row.get(0),
                ).unwrap();
                barrier.wait();
                let now = NOW + 3 * 86_400_000 + 60_000;
                let Ok(acquired) = scheduler.acquire_due_jobs("worker-race", now, 30_000, 64) else {
                    return false;
                };
                let Some(job) = acquired.into_iter().find(|job| job.job_id == driver_job_id) else {
                    return false;
                };
                store.materialize_due_routine_occurrences(&RoutineDriverClaim {
                    routine_id: "routine-clock".into(),
                    driver_job_id,
                    driver_lease_owner: job.lease_owner.unwrap(),
                    driver_lease_expires_at: job.lease_expires_at.unwrap(),
                    trusted_now: now,
                }).unwrap();
                true
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_routine_occurrences"),
        first.len() as i64
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_routine_job_bindings"),
        first.len() as i64
    );
}

#[test]
fn pause_global_stop_and_policy_drift_refuse_non_executable_admission() {
    let fixture = anchored_fixture();
    let continuation_baseline = table_count(&fixture.paths, "execass_continuations");
    let mut drifted_routine = command(&fixture, "routine-admission");
    drifted_routine.version.effective_policy_revision = 2;
    drifted_routine.version.effective_policy_snapshot_json = r#"{"revision":2}"#.into();
    fixture.store.create_routine(&drifted_routine).unwrap();
    let record = materialize(
        &fixture,
        "routine-admission",
        NOW + 2 * 86_400_000,
        "driver-admission",
    )
    .pop()
    .unwrap();
    let job_id: String = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT job_id FROM execass_routine_job_bindings WHERE occurrence_id=?1",
            [&record.occurrence_id],
            |row| row.get(0),
        )
        .unwrap();
    let claim = |owner: &str| {
        fixture
            .store
            .connection()
            .unwrap()
            .execute(
                "UPDATE jobs SET lease_owner=?1,lease_expires_at=?2 WHERE job_id=?3",
                params![owner, NOW + 10_000, job_id],
            )
            .unwrap();
        RoutineAdmissionRequest {
            occurrence_id: record.occurrence_id.clone(),
            trigger_job_id: job_id.clone(),
            trigger_lease_owner: owner.into(),
            trigger_lease_expires_at: NOW + 10_000,
            trusted_now: NOW + 1,
        }
    };
    fixture
        .store
        .set_routine_enabled("routine-admission", false, NOW + 1)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .plan_routine_occurrence_admission(&claim("worker-pause"))
            .unwrap(),
        RoutineAdmissionOutcome::Refused {
            reason: "routine_paused".into()
        }
    );
    fixture
        .store
        .set_routine_enabled("routine-admission", true, NOW + 2)
        .unwrap();
    fixture.store.connection().unwrap().execute("UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=1,updated_at=?1 WHERE singleton=1", [NOW + 2]).unwrap();
    assert_eq!(
        fixture
            .store
            .plan_routine_occurrence_admission(&claim("worker-stop"))
            .unwrap(),
        RoutineAdmissionOutcome::Refused {
            reason: "global_stop_engaged".into()
        }
    );
    fixture
        .store
        .connection()
        .unwrap()
        .execute(
            "UPDATE execass_global_runtime_control SET engaged=0,updated_at=?1 WHERE singleton=1",
            [NOW + 3],
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .plan_routine_occurrence_admission(&claim("worker-policy"))
            .unwrap(),
        RoutineAdmissionOutcome::Refused {
            reason: "current_policy_changed".into()
        }
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_continuations"),
        continuation_baseline,
        "storage admission must not create executable work"
    );
}

#[test]
fn claimed_reserved_trigger_plans_once_and_binding_is_immutable() {
    let fixture = anchored_fixture();
    fixture
        .store
        .create_routine(&command(&fixture, "routine-plan"))
        .unwrap();
    let record = materialize(
        &fixture,
        "routine-plan",
        NOW + 2 * 86_400_000,
        "driver-plan",
    )
    .pop()
    .unwrap();
    let conn = fixture.store.connection().unwrap();
    let job_id: String = conn
        .query_row(
            "SELECT job_id FROM execass_routine_job_bindings WHERE occurrence_id=?1",
            [&record.occurrence_id],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE jobs SET lease_owner='worker-1',lease_expires_at=?1 WHERE job_id=?2",
        params![NOW + 5 * 86_400_000, job_id],
    )
    .unwrap();
    let request = RoutineAdmissionRequest {
        occurrence_id: record.occurrence_id.clone(),
        trigger_job_id: job_id.clone(),
        trigger_lease_owner: "worker-1".into(),
        trigger_lease_expires_at: NOW + 5 * 86_400_000,
        trusted_now: NOW + 2 * 86_400_000 + 1,
    };
    assert!(matches!(
        fixture
            .store
            .plan_routine_occurrence_admission(&request)
            .unwrap(),
        RoutineAdmissionOutcome::Planned(_)
    ));
    assert!(matches!(
        fixture
            .store
            .plan_routine_occurrence_admission(&request)
            .unwrap(),
        RoutineAdmissionOutcome::Replayed(_)
    ));
    assert_eq!(
        fixture
            .store
            .settle_routine_trigger(&RoutineTriggerSettlementCommand {
                occurrence_id: record.occurrence_id.clone(),
                trigger_job_id: job_id.clone(),
                trigger_lease_owner: "worker-1".into(),
                trigger_lease_expires_at: NOW + 5 * 86_400_000,
                trusted_now: NOW + 2 * 86_400_000 + 2,
            })
            .unwrap(),
        RoutineTriggerSettlementOutcome::Settled
    );
    let enabled: i64 = conn
        .query_row(
            "SELECT enabled FROM jobs WHERE job_id=?1",
            [&job_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(enabled, 0, "settled trigger cannot reacquire");
    assert!(conn
        .execute(
            "UPDATE jobs SET payload_json='{}' WHERE job_id=?1",
            [job_id]
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_routine_job_bindings SET job_id='other' WHERE occurrence_id=?1",
            [&record.occurrence_id]
        )
        .is_err());
}

#[test]
fn persisted_schedule_derives_due_dates_advances_skip_cursor_and_keeps_driver() {
    let fixture = anchored_fixture();
    let mut skipped = command(&fixture, "routine-skip");
    skipped.routine.catch_up_policy = RoutineCatchUpPolicy::Skip;
    fixture.store.create_routine(&skipped).unwrap();
    assert!(materialize(
        &fixture,
        "routine-skip",
        NOW + 3 * 86_400_000,
        "driver-skip"
    )
    .is_empty());
    let cursor: i64 = fixture.store.connection().unwrap().query_row("SELECT last_evaluated_instant_ms FROM execass_routine_schedule_state WHERE routine_id='routine-skip'", [], |row| row.get(0)).unwrap();
    assert_eq!(cursor, NOW + 3 * 86_400_000);
    assert!(
        try_driver_claim(
            &fixture,
            "routine-skip",
            NOW + 2 * 86_400_000,
            "driver-rollback"
        )
        .is_none(),
        "clock rollback must not manufacture occurrences"
    );
    let driver_count: i64 = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM execass_routine_driver_jobs",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(driver_count, 1);
    let replay = command(&fixture, "routine-future");
    fixture.store.create_routine(&replay).unwrap();
    let first = materialize(
        &fixture,
        "routine-future",
        NOW + 86_400_000,
        "driver-future-1",
    )
    .len();
    let later = materialize(
        &fixture,
        "routine-future",
        NOW + 3 * 86_400_000,
        "driver-future-2",
    )
    .len();
    assert!(
        first + later >= 2,
        "canonical driver materialization must discover future recurrence"
    );
}

#[test]
fn create_normalizes_json_rejects_forged_or_mismatched_grants_and_amendment_fences_old_trigger() {
    let fixture = anchored_fixture();
    let mut normalized = command(&fixture, "routine-normalized");
    normalized.version.saved_selector_json = r#"{ "z": 1, "a": 2 }"#.into();
    normalized.version.saved_action_envelope_json = r#"{ "tool": "example.v1", "z": null }"#.into();
    normalized.version.effective_policy_snapshot_json = r#"{ "z": 1, "revision": 1 }"#.into();
    fixture.store.create_routine(&normalized).unwrap();
    let canonical: (String,String,String) = fixture.store.connection().unwrap().query_row("SELECT saved_selector_json,saved_action_envelope_json,effective_policy_snapshot_json FROM execass_routine_versions WHERE routine_id='routine-normalized'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(
        canonical,
        (
            r#"{"a":2,"z":1}"#.into(),
            r#"{"tool":"example.v1","z":null}"#.into(),
            r#"{"revision":1,"z":1}"#.into()
        )
    );
    let mut forged = command(&fixture, "routine-forged-grant");
    forged.version.accepted_confirmation_grant_id = Some("missing-grant".into());
    assert!(fixture.store.create_routine(&forged).is_err());
    let (grant_fixture, grant_command, attestation, _) =
        super::tests::prepared_attested_confirmation();
    grant_fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &grant_command,
            &attestation,
            1_800_000_000_020,
        )
        .unwrap();
    let mut mismatch = command(&grant_fixture, "routine-mismatch-grant");
    mismatch.version.accepted_confirmation_grant_id = Some(grant_command.grant_id);
    assert!(grant_fixture.store.create_routine(&mismatch).is_err());
    let old = materialize(
        &fixture,
        "routine-normalized",
        NOW + 2 * 86_400_000,
        "driver-normalized",
    )
    .pop()
    .unwrap();
    let mut next = command(&fixture, "routine-normalized");
    next.routine.current_version = 2;
    next.version.routine_version = 2;
    next.version.saved_selector_json = normalized.version.saved_selector_json;
    next.version.saved_action_envelope_json = normalized.version.saved_action_envelope_json;
    next.schedule.local_minute = 31;
    assert!(fixture
        .store
        .amend_routine(&AmendRoutineCommand {
            expected_current_version: 1,
            routine: next.routine.clone(),
            version: next.version.clone(),
            schedule: next.schedule.clone()
        })
        .unwrap());
    let job_id: String = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT job_id FROM execass_routine_job_bindings WHERE occurrence_id=?1",
            [&old.occurrence_id],
            |row| row.get(0),
        )
        .unwrap();
    fixture
        .store
        .connection()
        .unwrap()
        .execute(
            "UPDATE jobs SET lease_owner='worker-amend',lease_expires_at=?1 WHERE job_id=?2",
            params![NOW + 99_999, job_id],
        )
        .unwrap();
    assert_eq!(
        fixture
            .store
            .plan_routine_occurrence_admission(&RoutineAdmissionRequest {
                occurrence_id: old.occurrence_id,
                trigger_job_id: job_id,
                trigger_lease_owner: "worker-amend".into(),
                trigger_lease_expires_at: NOW + 99_999,
                trusted_now: NOW + 1
            })
            .unwrap(),
        RoutineAdmissionOutcome::Refused {
            reason: "routine_version_superseded".into()
        }
    );
}

#[test]
fn source_anchor_rejects_missing_wrong_nonhuman_and_intent_mismatch() {
    let fixture = anchored_fixture();

    let mut missing_source = command(&fixture, "routine-missing-source");
    missing_source.version.source_delegation_id = "missing-delegation".into();
    assert!(fixture.store.create_routine(&missing_source).is_err());

    let mut missing_authority = command(&fixture, "routine-missing-authority");
    missing_authority
        .version
        .saved_owner_authority_provenance_id = "missing-authority".into();
    assert!(fixture.store.create_routine(&missing_authority).is_err());

    let mut wrong_authority = command(&fixture, "routine-wrong-authority");
    wrong_authority.version.saved_owner_authority_provenance_id =
        "execass-global-control-carrier-authority".into();
    assert!(fixture.store.create_routine(&wrong_authority).is_err());

    let mut wrong_intent = command(&fixture, "routine-wrong-intent");
    wrong_intent
        .version
        .normalized_original_intent
        .push_str(" altered");
    assert!(fixture.store.create_routine(&wrong_intent).is_err());

    install_plan_for_existing_source(
        &fixture,
        "execass-global-control-carrier",
        "routine-runtime-source-plan",
    );
    let (authority_id, normalized_intent, manifest_json, manifest_digest) =
        source_anchor(&fixture, "execass-global-control-carrier");
    let mut nonhuman = command(&fixture, "routine-nonhuman-source");
    nonhuman.version.source_delegation_id = "execass-global-control-carrier".into();
    nonhuman.version.saved_owner_authority_provenance_id = authority_id;
    nonhuman.version.normalized_original_intent = normalized_intent;
    nonhuman.version.resolved_leaf_manifest_json = manifest_json;
    nonhuman.version.manifest_digest = manifest_digest;
    assert!(fixture.store.create_routine(&nonhuman).is_err());
}

#[test]
fn source_manifest_is_canonical_and_tamper_evident() {
    let fixture = anchored_fixture();
    let exact = command(&fixture, "routine-canonical-source");
    let source_value: serde_json::Value =
        serde_json::from_str(&exact.version.resolved_leaf_manifest_json).unwrap();
    let noncanonical = serde_json::to_string_pretty(&source_value).unwrap();

    let mut canonicalized = exact.clone();
    canonicalized.version.routine_id = "routine-canonicalized-source".into();
    canonicalized.routine.routine_id = "routine-canonicalized-source".into();
    canonicalized.version.resolved_leaf_manifest_json = noncanonical.clone();
    fixture.store.create_routine(&canonicalized).unwrap();
    let stored: String = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT resolved_leaf_manifest_json FROM execass_routine_versions WHERE routine_id='routine-canonicalized-source'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, exact.version.resolved_leaf_manifest_json);

    let mut raw_digest = exact.clone();
    raw_digest.version.routine_id = "routine-raw-manifest-digest".into();
    raw_digest.routine.routine_id = "routine-raw-manifest-digest".into();
    raw_digest.version.resolved_leaf_manifest_json = noncanonical.clone();
    raw_digest.version.manifest_digest = format!("{:x}", Sha256::digest(noncanonical.as_bytes()));
    assert!(fixture.store.create_routine(&raw_digest).is_err());

    let mut tampered_manifest = command(&fixture, "routine-tampered-manifest");
    tampered_manifest.version.resolved_leaf_manifest_json = "[]".into();
    assert!(fixture.store.create_routine(&tampered_manifest).is_err());

    let mut tampered_digest = command(&fixture, "routine-tampered-digest");
    tampered_digest.version.manifest_digest = "0".repeat(64);
    assert!(fixture.store.create_routine(&tampered_digest).is_err());

    let mut uppercase_digest = command(&fixture, "routine-uppercase-digest");
    uppercase_digest.version.manifest_digest = uppercase_digest
        .version
        .manifest_digest
        .to_ascii_uppercase();
    assert!(fixture.store.create_routine(&uppercase_digest).is_err());
}

#[test]
fn exact_source_grant_is_accepted_and_other_delegation_grant_is_rejected() {
    let (fixture, grant_command, attestation, _) = prepared_attested_confirmation();
    fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &grant_command,
            &attestation,
            1_800_000_000_020,
        )
        .unwrap();
    let canonical_envelope: String = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT canonical_action_envelope_or_selector_json FROM execass_accepted_confirmation_grants WHERE grant_id=?1",
            [&grant_command.grant_id],
            |row| row.get(0),
        )
        .unwrap();

    let mut exact = command(&fixture, "routine-exact-grant");
    exact.version.accepted_confirmation_grant_id = Some(grant_command.grant_id.clone());
    exact.version.saved_action_envelope_json = canonical_envelope.clone();
    fixture.store.create_routine(&exact).unwrap();

    install_second_human_source(&fixture);
    let (authority_id, normalized_intent, manifest_json, manifest_digest) =
        source_anchor(&fixture, "delegation-2");
    let mut other_source = command(&fixture, "routine-other-source-grant");
    other_source.version.source_delegation_id = "delegation-2".into();
    other_source.version.saved_owner_authority_provenance_id = authority_id;
    other_source.version.normalized_original_intent = normalized_intent;
    other_source.version.resolved_leaf_manifest_json = manifest_json;
    other_source.version.manifest_digest = manifest_digest;
    other_source.version.accepted_confirmation_grant_id = Some(grant_command.grant_id);
    other_source.version.saved_action_envelope_json = canonical_envelope;
    assert!(fixture.store.create_routine(&other_source).is_err());
}

#[test]
fn material_action_amendment_requires_a_new_admitted_source_plan() {
    let fixture = anchored_fixture();
    fixture
        .store
        .create_routine(&command(&fixture, "routine-material-amend"))
        .unwrap();
    let mut next = command(&fixture, "routine-material-amend");
    next.routine.current_version = 2;
    next.version.routine_version = 2;
    next.version.saved_action_envelope_json = r#"{"tool":"changed.v2"}"#.into();
    let error = fixture
        .store
        .amend_routine(&AmendRoutineCommand {
            expected_current_version: 1,
            routine: next.routine,
            version: next.version,
            schedule: next.schedule,
        })
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("material routine action amendment requires a newly admitted source plan"));
    let current: i64 = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT current_version FROM execass_routines WHERE routine_id='routine-material-amend'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current, 1);
}

#[test]
fn routine_schema_has_no_finance_category_or_purpose_controls() {
    let sql =
        include_str!("../../../../migrations/0007_execass_replacement.sql").to_ascii_lowercase();
    let routine_slice = &sql[sql.find("create table execass_routines").unwrap()..];
    for forbidden in [
        "currency",
        "money",
        "balance",
        "payee",
        "purchase",
        "financial",
        "category",
        "purpose",
        "tenant",
        "user_id",
    ] {
        assert!(
            !routine_slice.contains(forbidden),
            "forbidden routine control: {forbidden}"
        );
    }
}
