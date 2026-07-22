use super::jobs::deterministic_continuation_job_id;
use super::tests::{fixture, foundation, table_count};
use super::EXECASS_CONTINUATION_JOB_MODE;
use crate::{JobUpdatePatch, NewJob, Storage};
use rusqlite::params;
use std::sync::{Arc, Barrier};

const SCHEDULED_AT: i64 = 1_800_000_000_100;

fn seeded_fixture() -> super::tests::Fixture {
    let fixture = fixture();
    fixture
        .store
        .create_foundation(&foundation())
        .expect("create runnable ExecAss foundation");
    fixture
}

#[test]
fn runnable_continuation_reconciles_once_into_canonical_jobs() {
    let fixture = seeded_fixture();

    let first = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .expect("materialize first continuation job");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].continuation_id, "continuation-1");
    assert_eq!(
        first[0].job_id,
        deterministic_continuation_job_id("continuation-1")
    );
    assert!(first[0]
        .payload_json
        .contains(&format!(r#""mode":"{EXECASS_CONTINUATION_JOB_MODE}""#)));

    let replay = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .expect("reconcile same continuation again");
    assert!(
        replay.is_empty(),
        "reconciliation must not create a second job"
    );
    assert_eq!(table_count(&fixture.paths, "jobs"), 1);
    assert_eq!(
        fixture
            .store
            .read_continuation_job_binding("continuation-1")
            .expect("read deterministic job binding")
            .expect("binding exists"),
        first[0]
    );
}

#[test]
fn concurrent_reconcilers_converge_on_one_job() {
    let fixture = seeded_fixture();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let store = fixture.store.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            store
                .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
                .expect("concurrent reconciliation")
                .len()
        }));
    }
    barrier.wait();
    let created = workers
        .into_iter()
        .map(|worker| worker.join().expect("reconciler thread"))
        .sum::<usize>();

    assert_eq!(created, 1);
    assert_eq!(table_count(&fixture.paths, "jobs"), 1);
}

#[test]
fn lease_expiry_reclaims_the_same_job_without_duplication() {
    let fixture = seeded_fixture();
    let binding = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .expect("materialize continuation job")
        .remove(0);
    let scheduler = Storage::from_paths(&fixture.paths);

    let first = scheduler
        .acquire_due_jobs("scheduler-a", SCHEDULED_AT, 100, 10)
        .expect("first scheduler claim");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].job_id, binding.job_id);
    assert!(scheduler
        .acquire_due_jobs("scheduler-b", SCHEDULED_AT + 100, 100, 10)
        .expect("claim at non-expired boundary")
        .is_empty());

    let reclaimed = scheduler
        .acquire_due_jobs("scheduler-b", SCHEDULED_AT + 101, 100, 10)
        .expect("reclaim expired durable job");
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].job_id, binding.job_id);
    assert_eq!(
        scheduler.jobs_total_count().expect("count canonical jobs"),
        1
    );
    assert_eq!(
        fixture
            .store
            .read_continuation_job_binding("continuation-1")
            .expect("read preserved binding")
            .expect("binding remains")
            .job_id,
        binding.job_id
    );
}

#[test]
fn deterministic_identity_collision_rolls_back_binding() {
    let fixture = seeded_fixture();
    let colliding_job_id = deterministic_continuation_job_id("continuation-1");
    let conn = fixture.store.connection().expect("open exact database");
    conn.execute(
        r#"
        INSERT INTO jobs (
          job_id, agent_id, name, enabled, schedule_kind,
          interval_seconds, run_at_ms, next_run_at, payload_json,
          max_retries, retry_backoff_ms, timeout_ms,
          lease_owner, lease_expires_at, last_run_at, last_error,
          created_at, updated_at, deleted_at
        ) VALUES (?1, 'default', 'collision', 1, 'once', NULL, ?2, ?2,
                  '{"mode":"ordinary"}', 0, 1000, 30000,
                  NULL, NULL, NULL, NULL, ?2, ?2, NULL)
        "#,
        params![colliding_job_id, SCHEDULED_AT],
    )
    .expect("seed deterministic identity collision");
    drop(conn);

    let error = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .expect_err("identity collision must reject reconciliation");
    assert!(error.to_string().contains("identity collision"));
    assert!(fixture
        .store
        .read_continuation_job_binding("continuation-1")
        .expect("read rolled-back binding")
        .is_none());
    assert_eq!(table_count(&fixture.paths, "jobs"), 1);
}

#[test]
fn internal_continuation_jobs_reject_generic_mutation_and_raw_rebinding() {
    let fixture = seeded_fixture();
    let generic = Storage::from_paths(&fixture.paths);
    let forged = generic.create_job(NewJob {
        agent_id: "default".into(),
        name: "forged internal job".into(),
        enabled: true,
        schedule_kind: "once".into(),
        interval_seconds: None,
        run_at_ms: Some(SCHEDULED_AT),
        next_run_at: Some(SCHEDULED_AT),
        payload_json: format!(r#"{{"mode":"{EXECASS_CONTINUATION_JOB_MODE}"}}"#),
        max_retries: 0,
        retry_backoff_ms: 1_000,
        timeout_ms: 30_000,
    });
    assert!(forged.is_err());

    let binding = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .expect("materialize protected internal job")
        .remove(0);
    assert!(generic
        .update_job(
            &binding.job_id,
            JobUpdatePatch {
                name: Some("rewritten".into()),
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
    assert!(generic.remove_job(&binding.job_id).is_err());

    let conn = fixture.store.connection().expect("open exact database");
    assert!(conn
        .execute(
            "UPDATE jobs SET name='raw rewrite' WHERE job_id=?1",
            params![binding.job_id],
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_continuations SET job_id=NULL WHERE continuation_id='continuation-1'",
            [],
        )
        .is_err());
    assert!(conn
        .execute("DELETE FROM jobs WHERE job_id=?1", params![binding.job_id])
        .is_err());
}

#[test]
fn routine_occurrence_uses_the_same_durable_job_reconciler() {
    let fixture = fixture();
    let mut command = foundation();
    command.initial_continuation = None;
    fixture
        .store
        .create_foundation(&command)
        .expect("create delegation before routine occurrence");
    let conn = fixture.store.connection().expect("open exact database");
    conn.execute_batch(
        r#"
        INSERT INTO execass_action_branches (
          action_id, delegation_id, action_revision,
          target_delegation_revision, target_plan_revision, stop_epoch,
          branch_kind, status, action_summary, created_at, updated_at, terminal_at
        ) VALUES (
          'routine-action-1', 'delegation-1', 1, 1, 1, 0,
          'ordinary', 'runnable', 'execute saved routine occurrence',
          1800000000050, 1800000000050, NULL
        );
        INSERT INTO execass_continuations (
          continuation_id, delegation_id, target_delegation_revision,
          target_plan_revision, action_id, branch_kind, causation_kind,
          causation_id, status, job_id, lease_owner, lease_expires_at,
          fencing_token, host_generation, stop_epoch, global_stop_epoch, created_at, updated_at,
          completed_at
        ) VALUES (
          'routine-continuation-1', 'delegation-1', 1, 1,
          'routine-action-1', 'ordinary', 'routine_occurrence',
          'routine-occurrence-1', 'runnable', NULL, NULL, NULL,
          0, 1, 0, 0, 1800000000050, 1800000000050, NULL
        );
        "#,
    )
    .expect("seed independently-created routine occurrence continuation");
    drop(conn);

    let binding = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .expect("materialize routine occurrence")
        .remove(0);
    assert!(binding
        .payload_json
        .contains(r#""causation_kind":"routine_occurrence""#));
    assert_eq!(binding.continuation_id, "routine-continuation-1");
    assert_eq!(table_count(&fixture.paths, "jobs"), 1);
}
