use super::tests::{fixture, foundation, prepared_attested_confirmation, table_count, Fixture};
use super::*;
use crate::{open_sqlite_connection, Storage};
use rusqlite::{params, Connection};
use std::sync::{Arc, Barrier};

const SCHEDULED_AT: i64 = 1_800_000_000_100;
const CLAIM_NOW: i64 = 1_800_000_000_200;
const LEASE_MS: i64 = 5_000;

struct ClaimFixture {
    fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    redactor: ReceiptRedactor,
    key: ReceiptKeyRef,
    job_id: String,
    worker_id: String,
    job_lease_expires_at: i64,
}

fn setup() -> ClaimFixture {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_host(&fixture, 1, "claim-host-1", 1, 9_999_999_999_999);
    let job_id = fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .unwrap()
        .remove(0)
        .job_id;
    let scheduler = Storage::from_paths(&fixture.paths);
    let job = scheduler
        .acquire_due_jobs("worker-a", CLAIM_NOW, LEASE_MS, 10)
        .unwrap()
        .remove(0);
    assert_eq!(job.job_id, job_id);
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity.provision_initial_key("claim-key").unwrap();
    ClaimFixture {
        fixture,
        integrity,
        redactor: ReceiptRedactor::new(&["claim-test-secret"]).unwrap(),
        key,
        job_id,
        worker_id: "worker-a".into(),
        job_lease_expires_at: job.lease_expires_at.unwrap(),
    }
}

fn seed_host(fixture: &Fixture, generation: i64, host: &str, token: i64, expires_at: i64) {
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        r#"INSERT INTO execass_runtime_host_generations(
          generation,ownership_scope,state_root_generation,installation_identity,
          os_user_identity_digest,host_instance_id,started_at
        ) VALUES(?1,'execass',1,'claim-installation','claim-user',?2,1)"#,
        params![generation, host],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_runtime_host_leases(
          lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
        ) VALUES(?1,'execass',?2,?3,?4,1,?5)"#,
        params![
            format!("claim-host-lease-{generation}"),
            generation,
            host,
            token,
            expires_at
        ],
    )
    .unwrap();
}

fn takeover_host(fixture: &Fixture) {
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE released_at IS NULL",
        params![CLAIM_NOW + 10],
    )
    .unwrap();
    conn.execute(
        "UPDATE execass_runtime_host_generations SET ended_at=?1,end_reason='takeover' WHERE ended_at IS NULL",
        params![CLAIM_NOW + 10],
    )
    .unwrap();
    drop(conn);
    seed_host(fixture, 2, "claim-host-2", 2, 9_999_999_999_999);
}

fn heads(conn: &Connection) -> (i64, Option<String>, i64, Option<String>, i64) {
    conn.query_row(
        r#"SELECT journal.receipt_count,journal.receipt_head_digest,
                  d.receipt_chain_count,d.receipt_chain_head_digest,d.state_revision
           FROM execass_receipt_journal_state journal
           JOIN execass_delegations d ON d.delegation_id='delegation-1'
           WHERE journal.singleton=1"#,
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )
    .unwrap()
}

fn claim_command(f: &ClaimFixture, suffix: &str) -> ContinuationClaimCommand {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        heads(&conn);
    let context = f
        .fixture
        .store
        .read_continuation_receipt_context("continuation-1", CLAIM_NOW)
        .unwrap()
        .unwrap();
    let occurred_at = CLAIM_NOW + 10;
    let event_id = format!("claim-event-{suffix}");
    ContinuationClaimCommand {
        write: WriteContext {
            idempotency_key: format!("claim-idem-{suffix}"),
            correlation_id: format!("claim-corr-{suffix}"),
            causation_id: format!("claim-cause-{suffix}"),
            occurred_at,
        },
        continuation_id: "continuation-1".into(),
        job_id: f.job_id.clone(),
        worker_id: f.worker_id.clone(),
        job_lease_expires_at: f.job_lease_expires_at,
        trusted_now: CLAIM_NOW,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: state_revision,
            correlation_id: format!("claim-corr-{suffix}"),
            causation_id: format!("claim-cause-{suffix}"),
            occurred_at,
            safe_payload_json: format!(r#"{{"op":"claim","suffix":"{suffix}"}}"#),
            duplicate_identity: format!("claim-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("claim-receipt-{suffix}"),
            transaction_id: format!("claim-tx-{suffix}"),
            state_root_generation: context.state_root_generation,
            delegation_id: "delegation-1".into(),
            expected_state_revision: state_revision,
            expected_global_count: global_count,
            expected_global_head_digest: global_head,
            expected_delegation_count: delegation_count,
            expected_delegation_head_digest: delegation_head,
            receipt_kind: ReceiptKind::Continuation,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Continuation,
                subject_id: "continuation-1".into(),
                revision: state_revision,
            },
            causation_id: format!("claim-cause-{suffix}"),
            causation_event_id: event_id,
            actor: context.runtime_actor,
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id,
                fencing_token: context.runtime_fencing_token,
            },
            key: f.key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::summary("continuation claim recorded", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    }
}

fn settle_command(
    f: &ClaimFixture,
    identity: ContinuationClaimIdentity,
    suffix: &str,
    status: ContinuationStatus,
) -> ContinuationSettleCommand {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        heads(&conn);
    let event_id = format!("settle-event-{suffix}");
    let occurred_at = CLAIM_NOW + 20;
    ContinuationSettleCommand {
        write: WriteContext {
            idempotency_key: format!("settle-idem-{suffix}"),
            correlation_id: format!("settle-corr-{suffix}"),
            causation_id: format!("settle-cause-{suffix}"),
            occurred_at,
        },
        identity: identity.clone(),
        trusted_now: CLAIM_NOW + 1,
        result_status: status,
        technical_resource_actuals: Vec::new(),
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: state_revision,
            correlation_id: format!("settle-corr-{suffix}"),
            causation_id: format!("settle-cause-{suffix}"),
            occurred_at,
            safe_payload_json: format!(r#"{{"op":"settle","suffix":"{suffix}"}}"#),
            duplicate_identity: format!("settle-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("settle-receipt-{suffix}"),
            transaction_id: format!("settle-tx-{suffix}"),
            state_root_generation: 1,
            delegation_id: "delegation-1".into(),
            expected_state_revision: state_revision,
            expected_global_count: global_count,
            expected_global_head_digest: global_head,
            expected_delegation_count: delegation_count,
            expected_delegation_head_digest: delegation_head,
            receipt_kind: ReceiptKind::Continuation,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Continuation,
                subject_id: "continuation-1".into(),
                revision: state_revision,
            },
            causation_id: format!("settle-cause-{suffix}"),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: ActorType::Runtime,
                actor_identity: SafeText::new(&identity.runtime_actor_identity, &[]).unwrap(),
                authority_provenance_id: identity.runtime_authority_provenance_id.clone(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: identity.runtime_host_generation,
                host_instance_id: identity.runtime_host_instance_id.clone(),
                fencing_token: identity.runtime_fencing_token,
            },
            key: f.key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::summary("continuation result recorded", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    }
}

fn assert_counts(f: &ClaimFixture, outbox: i64, receipts: i64) {
    assert_eq!(
        table_count(&f.fixture.paths, "execass_outbox_events"),
        outbox
    );
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), receipts);
}

#[test]
fn claim_has_single_winner_and_exact_replay_does_not_increment_fence() {
    let f = setup();
    let command = claim_command(&f, "single");
    let first = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let ContinuationClaimOutcome::Claimed(first) = first else {
        panic!("expected first claim")
    };
    assert_eq!(first.continuation.fencing_token, 1);
    assert_eq!(first.identity.continuation_fencing_token, 1);

    let replay = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let ContinuationClaimOutcome::Replayed(replay) = replay else {
        panic!("expected exact replay")
    };
    assert_eq!(replay.identity.continuation_fencing_token, 1);
    assert_eq!(replay.result_status, ContinuationStatus::Executing);
    assert_counts(&f, 2, 1);
}

#[test]
fn concurrent_claims_leave_one_receipt_and_one_executing_owner() {
    let f = setup();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for suffix in ["race-a", "race-b"] {
        let store = f.fixture.store.clone();
        let integrity = ReceiptIntegrityStore::open(&f.fixture.paths).unwrap();
        let redactor = f.redactor.clone();
        let command = claim_command(&f, suffix);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            store
                .claim_continuation_atomically(&integrity, &redactor, &command)
                .unwrap()
        }));
    }
    barrier.wait();
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ContinuationClaimOutcome::Claimed(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ContinuationClaimOutcome::Lost { .. }))
            .count(),
        1
    );
    assert_counts(&f, 2, 1);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status||':'||lease_owner||':'||fencing_token FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "executing:worker-a:1"
    );
}

#[test]
fn expired_job_takeover_rejects_old_worker_and_allows_new_worker_before_claim() {
    let mut f = setup();
    let old = claim_command(&f, "old-worker");
    let scheduler = Storage::from_paths(&f.fixture.paths);
    let taken = scheduler
        .acquire_due_jobs("worker-b", f.job_lease_expires_at + 1, LEASE_MS, 10)
        .unwrap()
        .remove(0);
    f.worker_id = "worker-b".into();
    f.job_lease_expires_at = taken.lease_expires_at.unwrap();
    let new = claim_command(&f, "new-worker");

    assert!(matches!(
        f.fixture
            .store
            .claim_continuation_atomically(&f.integrity, &f.redactor, &old)
            .unwrap(),
        ContinuationClaimOutcome::Lost {
            reason: ContinuationStaleReason::JobLeaseLostOrExpired
        }
    ));
    assert!(matches!(
        f.fixture
            .store
            .claim_continuation_atomically(&f.integrity, &f.redactor, &new)
            .unwrap(),
        ContinuationClaimOutcome::Claimed(_)
    ));
    assert_counts(&f, 2, 1);
}

#[test]
fn host_takeover_blocks_paused_worker_dispatch_and_settle_without_writes() {
    let f = setup();
    let command = claim_command(&f, "host-takeover");
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };
    takeover_host(&f.fixture);

    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: claimed.identity.clone(),
                trusted_now: CLAIM_NOW + 11,
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::RuntimeHostLeaseLostOrExpired
        }
    );
    let settle = settle_command(
        &f,
        claimed.identity.clone(),
        "host-takeover",
        ContinuationStatus::Terminal,
    );
    assert!(matches!(
        f.fixture
            .store
            .settle_continuation_atomically(&f.integrity, &f.redactor, &settle)
            .unwrap(),
        ContinuationSettleOutcome::Lost {
            reason: ContinuationStaleReason::RuntimeHostLeaseLostOrExpired
        }
    ));
    assert_counts(&f, 2, 1);
}

#[test]
fn global_stop_epoch_drift_blocks_dispatch_and_prevents_terminal_commit() {
    let f = setup();
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim_command(&f, "global"))
        .unwrap()
    else {
        panic!("claim")
    };
    open_sqlite_connection(&f.fixture.paths.db_path)
        .unwrap()
        .execute(
            "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=1,updated_at=?1 WHERE singleton=1",
            params![CLAIM_NOW + 12],
        )
        .unwrap();
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: claimed.identity.clone(),
                trusted_now: CLAIM_NOW + 13,
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::GlobalStopEngaged
        }
    );
    assert!(matches!(
        f.fixture
            .store
            .settle_continuation_atomically(
                &f.integrity,
                &f.redactor,
                &settle_command(&f, claimed.identity, "global", ContinuationStatus::Terminal)
            )
            .unwrap(),
        ContinuationSettleOutcome::Superseded(_)
    ));
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "superseded"
    );
}

#[test]
fn policy_and_delegation_stop_epoch_drift_block_dispatch() {
    for (suffix, expected) in [
        ("policy", ContinuationStaleReason::PolicyRevisionDrift),
        ("stop", ContinuationStaleReason::DelegationStopEpochDrift),
    ] {
        let f = setup();
        let ContinuationClaimOutcome::Claimed(claimed) = f
            .fixture
            .store
            .claim_continuation_atomically(&f.integrity, &f.redactor, &claim_command(&f, suffix))
            .unwrap()
        else {
            panic!("claim")
        };
        force_same_revision_drift(&f.fixture, suffix);
        assert_eq!(
            f.fixture
                .store
                .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                    identity: claimed.identity,
                    trusted_now: CLAIM_NOW + 2,
                })
                .unwrap(),
            ContinuationDispatchValidationOutcome::Stale { reason: expected }
        );
    }
}

fn force_same_revision_drift(fixture: &Fixture, suffix: &str) {
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    if suffix == "policy" {
        conn.execute(
            "UPDATE execass_delegations SET state_revision=2,policy_revision=2,updated_at=?1 WHERE delegation_id='delegation-1'",
            params![CLAIM_NOW + 1],
        )
        .unwrap();
    } else {
        conn.execute(
            "UPDATE execass_delegations SET state_revision=2,stop_epoch=1,updated_at=?1 WHERE delegation_id='delegation-1'",
            params![CLAIM_NOW + 1],
        )
        .unwrap();
    }
}

#[test]
fn accepted_danger_grant_is_not_invalidated_by_technical_global_drift() {
    let (fixture, command, attestation, _) = prepared_attested_confirmation();
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_attested_at_for_test(
                &command,
                &attestation,
                1_800_000_000_020
            )
            .unwrap(),
        DangerConfirmationResolutionOutcome::Confirmed(_)
    ));
    open_sqlite_connection(&fixture.paths.db_path)
        .unwrap()
        .execute(
            "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=global_stop_epoch+1,updated_at=?1 WHERE singleton=1",
            params![CLAIM_NOW],
        )
        .unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_accepted_confirmation_grants WHERE invalidated_at IS NULL AND invalidation_reason IS NULL",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
}

#[test]
fn outbox_collision_rolls_back_claim_and_receipt() {
    let f = setup();
    let command = claim_command(&f, "collision");
    open_sqlite_connection(&f.fixture.paths.db_path)
        .unwrap()
        .execute(
            "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.runtime_host.changed','delegation-1',1,'preexisting','preexisting',?2,'v1','{}','preexisting-collision')",
            params![command.outbox_event.event_id, CLAIM_NOW],
        )
        .unwrap();
    assert!(f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .is_err());
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status||':'||fencing_token FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "runnable:0"
    );
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 0);
}

#[test]
fn human_or_caller_substituted_claim_actor_is_rejected_without_writes() {
    let f = setup();
    let mut command = claim_command(&f, "forged-actor");
    command.receipt.actor = ReceiptActorBinding {
        actor_type: ActorType::HumanLocal,
        actor_identity: SafeText::new("local-operator", &[]).unwrap(),
        authority_provenance_id: "authority-1".into(),
    };

    let error = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .expect_err("human authority cannot impersonate the runtime claimant");
    assert!(error
        .to_string()
        .contains("server-derived runtime authority"));
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status||':'||fencing_token FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "runnable:0"
    );
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 0);
    assert_eq!(table_count(&f.fixture.paths, "execass_outbox_events"), 1);
}

#[test]
fn global_runtime_control_cannot_rewind_skip_or_disappear() {
    let f = setup();
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=2,updated_at=1 WHERE singleton=1",
            [],
        )
        .is_err());
    conn.execute(
        "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=1,updated_at=1 WHERE singleton=1",
        [],
    )
    .unwrap();
    assert!(conn
        .execute(
            "UPDATE execass_global_runtime_control SET engaged=0,global_stop_epoch=2,updated_at=2 WHERE singleton=1",
            [],
        )
        .is_err());
    conn.execute(
        "UPDATE execass_global_runtime_control SET engaged=0,global_stop_epoch=1,updated_at=2 WHERE singleton=1",
        [],
    )
    .unwrap();
    assert!(conn
        .execute(
            "UPDATE execass_global_runtime_control SET global_stop_epoch=0,updated_at=3 WHERE singleton=1",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_global_runtime_control WHERE singleton=1",
            [],
        )
        .is_err());
}

#[test]
fn quota_policy_claim_digest_is_not_caller_replaceable() {
    let f = setup();
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim_command(&f, "quota"))
        .unwrap()
    else {
        panic!("claim")
    };
    let mut forged = claimed.identity;
    forged.technical_quota_policy_digest = "sha256:forged".into();
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: forged,
                trusted_now: CLAIM_NOW + 1,
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::ClaimIdentityMismatch
        }
    );
}

#[test]
fn exact_claim_replay_survives_later_terminal_settlement() {
    let f = setup();
    let command = claim_command(&f, "replay-after-settle");
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };
    assert!(matches!(
        f.fixture
            .store
            .settle_continuation_atomically(
                &f.integrity,
                &f.redactor,
                &settle_command(
                    &f,
                    claimed.identity.clone(),
                    "replay-after-settle",
                    ContinuationStatus::Terminal,
                ),
            )
            .unwrap(),
        ContinuationSettleOutcome::Settled(_)
    ));

    let replay = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let ContinuationClaimOutcome::Replayed(replayed) = replay else {
        panic!("exact claim must replay after later progress")
    };
    assert_eq!(replayed.identity, claimed.identity);
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 2);
}

#[test]
fn exact_settle_replay_is_stable_and_terminal_settle_disables_the_bound_job() {
    let f = setup();
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(
            &f.integrity,
            &f.redactor,
            &claim_command(&f, "settle-replay"),
        )
        .unwrap()
    else {
        panic!("claim")
    };
    let command = settle_command(
        &f,
        claimed.identity.clone(),
        "settle-replay",
        ContinuationStatus::Terminal,
    );
    let first = f
        .fixture
        .store
        .settle_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let ContinuationSettleOutcome::Settled(first) = first else {
        panic!("first settle")
    };

    let replay = f
        .fixture
        .store
        .settle_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let ContinuationSettleOutcome::Replayed(replayed) = replay else {
        panic!("exact settle must replay")
    };
    assert_eq!(replayed.identity, first.identity);
    assert_eq!(replayed.receipt, first.receipt);

    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT enabled||':'||COALESCE(next_run_at,-1)||':'||COALESCE(lease_owner,'none')||':'||COALESCE(lease_expires_at,-1) FROM jobs WHERE job_id=?1",
            params![f.job_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "0:-1:none:-1"
    );
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 2);
    assert_eq!(
        table_count(&f.fixture.paths, "execass_continuation_operation_history"),
        2
    );
}

#[test]
fn continuation_operation_history_rejects_raw_update_and_delete() {
    let f = setup();
    assert!(matches!(
        f.fixture
            .store
            .claim_continuation_atomically(
                &f.integrity,
                &f.redactor,
                &claim_command(&f, "history-guards")
            )
            .unwrap(),
        ContinuationClaimOutcome::Claimed(_)
    ));
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            "UPDATE execass_continuation_operation_history SET worker_id='forged' WHERE operation='claim'",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_continuation_operation_history WHERE operation='claim'",
            [],
        )
        .is_err());
    assert_eq!(
        table_count(&f.fixture.paths, "execass_continuation_operation_history"),
        1
    );
}

#[test]
fn raw_projection_promotion_without_claim_history_is_rejected() {
    let f = setup();
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            r#"UPDATE execass_continuations
               SET status='executing',lease_owner=?1,lease_expires_at=?2,
                   fencing_token=1,host_generation=1,updated_at=?3
               WHERE continuation_id='continuation-1'"#,
            params![f.worker_id, f.job_lease_expires_at, CLAIM_NOW],
        )
        .is_err());
    assert_eq!(
        conn.query_row(
            "SELECT status||':'||fencing_token FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "runnable:0"
    );
}

#[test]
fn immutable_claim_provenance_rejects_actor_and_state_root_substitution() {
    let f = setup();
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(
            &f.integrity,
            &f.redactor,
            &claim_command(&f, "provenance-substitution"),
        )
        .unwrap()
    else {
        panic!("claim")
    };

    let mut forged_actor = claimed.identity.clone();
    forged_actor.runtime_actor_identity = "sha256:forged-runtime-actor".into();
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: forged_actor,
                trusted_now: CLAIM_NOW + 1,
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::ClaimIdentityMismatch
        }
    );

    let mut forged_settle = settle_command(
        &f,
        claimed.identity,
        "state-root-substitution",
        ContinuationStatus::Terminal,
    );
    forged_settle.receipt.state_root_generation += 1;
    assert!(f
        .fixture
        .store
        .settle_continuation_atomically(&f.integrity, &f.redactor, &forged_settle)
        .is_err());
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        open_sqlite_connection(&f.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "executing"
    );
}
