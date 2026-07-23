use super::{
    ExecAssStore, OutboxConsumerIdentity, OutboxDeliveryCommit, OutboxDeliveryCommitOutcome,
    OutboxGapReason, OutboxReplayOutcome,
};
use crate::{init_execass_fresh_root, open_sqlite_connection, AppPaths};
use rusqlite::params;
use tempfile::TempDir;

fn fixture() -> (TempDir, AppPaths, ExecAssStore) {
    let root = TempDir::new_in(env!("CARGO_MANIFEST_DIR")).expect("project-drive temp root");
    let paths = AppPaths::from_root(root.path().join("execass-outbox-transport"));
    init_execass_fresh_root(&paths).expect("initialize canonical ExecAss root");
    let store = ExecAssStore::open(&paths).expect("open canonical ExecAss store");
    (root, paths, store)
}

fn consumer(name: &str) -> OutboxConsumerIdentity {
    OutboxConsumerIdentity {
        consumer_id: format!("consumer-{name}"),
        principal_id: format!("principal-{name}"),
        client_id_digest: format!("sha256:client-{name}"),
    }
}

fn insert_event(paths: &AppPaths, event_id: &str, revision: i64, payload: &str) {
    open_sqlite_connection(&paths.db_path)
        .expect("open canonical database")
        .execute(
            "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.delegation.transitioned','delegation-1',?2,?3,?4,?5,'v1',?6,?7)",
            params![event_id, revision, format!("corr-{event_id}"), format!("cause-{event_id}"), 1_800_000_000_000_i64 + revision, payload, format!("duplicate-{event_id}")],
        )
        .expect("insert immutable outbox event");
}

#[test]
fn restart_replay_orders_same_aggregate_and_preserves_send_before_commit_duplicates() {
    let (_root, paths, store) = fixture();
    insert_event(&paths, "event-1", 1, r#"{"summary":"first"}"#);
    insert_event(&paths, "event-2", 2, r#"{"summary":"second"}"#);
    let consumer = consumer("one");

    let first = match store.replay_outbox(&consumer, 0).unwrap() {
        OutboxReplayOutcome::Replay(replay) => replay,
        outcome => panic!("expected replay, got {outcome:?}"),
    };
    assert_eq!(
        first
            .events
            .iter()
            .map(|event| event.global_sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    // Crash before the cursor transaction: a reopened store must replay event 1.
    let restarted = ExecAssStore::open(&paths).expect("reopen after pre-commit crash");
    let before_commit = match restarted.replay_outbox(&consumer, 0).unwrap() {
        OutboxReplayOutcome::Replay(replay) => replay,
        outcome => panic!("expected replay, got {outcome:?}"),
    };
    assert_eq!(before_commit.events[0].global_sequence, 1);

    assert_eq!(
        restarted
            .commit_outbox_delivery(OutboxDeliveryCommit {
                consumer: &consumer,
                expected_cursor: 0,
                global_sequence: 1,
                published_at: 1_800_000_000_100,
            })
            .unwrap(),
        OutboxDeliveryCommitOutcome::Committed
    );
    // Overlap/crash after cursor commit is an exact idempotent success, never rewind.
    assert_eq!(
        restarted
            .commit_outbox_delivery(OutboxDeliveryCommit {
                consumer: &consumer,
                expected_cursor: 0,
                global_sequence: 1,
                published_at: 1_800_000_000_101,
            })
            .unwrap(),
        OutboxDeliveryCommitOutcome::AlreadyCommitted
    );
    let after_commit = match ExecAssStore::open(&paths)
        .unwrap()
        .replay_outbox(&consumer, 1)
        .unwrap()
    {
        OutboxReplayOutcome::Replay(replay) => replay,
        outcome => panic!("expected replay, got {outcome:?}"),
    };
    assert_eq!(
        after_commit
            .events
            .iter()
            .map(|event| event.global_sequence)
            .collect::<Vec<_>>(),
        vec![2]
    );
}

#[test]
fn cursor_gaps_and_two_consumers_are_isolated_without_published_filtering() {
    let (_root, paths, store) = fixture();
    insert_event(&paths, "event-1", 1, r#"{"summary":"first"}"#);
    insert_event(&paths, "event-2", 2, r#"{"summary":"second"}"#);
    let first = consumer("first");
    let second = consumer("second");
    store
        .commit_outbox_delivery(OutboxDeliveryCommit {
            consumer: &first,
            expected_cursor: 0,
            global_sequence: 1,
            published_at: 1_800_000_000_100,
        })
        .unwrap();
    assert!(matches!(
        store.replay_outbox(&first, 0).unwrap(),
        OutboxReplayOutcome::SummaryRefetchRequired {
            reason: OutboxGapReason::StaleCursor,
            ..
        }
    ));
    assert!(matches!(
        store.replay_outbox(&first, 99).unwrap(),
        OutboxReplayOutcome::SummaryRefetchRequired {
            reason: OutboxGapReason::FutureCursor,
            ..
        }
    ));
    let second_replay = match store.replay_outbox(&second, 0).unwrap() {
        OutboxReplayOutcome::Replay(replay) => replay,
        outcome => panic!("expected independent consumer replay, got {outcome:?}"),
    };
    assert_eq!(
        second_replay.events.len(),
        2,
        "published rows replay for a new consumer"
    );
    assert_eq!(
        store
            .commit_outbox_delivery(OutboxDeliveryCommit {
                consumer: &first,
                expected_cursor: 1,
                global_sequence: 2,
                published_at: 1_800_000_000_200,
            })
            .unwrap(),
        OutboxDeliveryCommitOutcome::Committed
    );
    assert_eq!(
        store
            .commit_outbox_delivery(OutboxDeliveryCommit {
                consumer: &second,
                expected_cursor: 0,
                global_sequence: 1,
                published_at: 1_800_000_000_201,
            })
            .unwrap(),
        OutboxDeliveryCommitOutcome::Committed
    );
}

#[test]
fn advanced_overlap_and_identity_conflict_do_not_mutate_cursor() {
    let (_root, paths, store) = fixture();
    insert_event(&paths, "event-1", 1, r#"{"summary":"first"}"#);
    insert_event(&paths, "event-2", 2, r#"{"summary":"second"}"#);
    let owner = consumer("owner");
    store
        .commit_outbox_delivery(OutboxDeliveryCommit {
            consumer: &owner,
            expected_cursor: 0,
            global_sequence: 1,
            published_at: 1_800_000_000_100,
        })
        .unwrap();
    store
        .commit_outbox_delivery(OutboxDeliveryCommit {
            consumer: &owner,
            expected_cursor: 1,
            global_sequence: 2,
            published_at: 1_800_000_000_101,
        })
        .unwrap();
    assert_eq!(
        store
            .commit_outbox_delivery(OutboxDeliveryCommit {
                consumer: &owner,
                expected_cursor: 0,
                global_sequence: 1,
                published_at: 1_800_000_000_102,
            })
            .unwrap(),
        OutboxDeliveryCommitOutcome::ConsumerAdvanced { consumer_cursor: 2 }
    );
    let attacker = OutboxConsumerIdentity {
        consumer_id: owner.consumer_id.clone(),
        principal_id: "different-principal".to_string(),
        client_id_digest: owner.client_id_digest.clone(),
    };
    assert!(matches!(
        store.replay_outbox(&attacker, 2).unwrap(),
        OutboxReplayOutcome::SummaryRefetchRequired {
            reason: OutboxGapReason::ConsumerIdentityConflict,
            ..
        }
    ));
    let cursor: i64 = open_sqlite_connection(&paths.db_path)
        .unwrap()
        .query_row(
            "SELECT last_global_sequence FROM execass_outbox_cursors WHERE consumer_id=?1",
            [&owner.consumer_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        cursor, 2,
        "overlap and identity conflict never rewind cursor"
    );
}

#[test]
fn sequence_gaps_missing_commits_and_immutable_rows_fail_closed() {
    let (_root, paths, store) = fixture();
    let gap_insert = open_sqlite_connection(&paths.db_path)
        .unwrap()
        .execute(
            "INSERT INTO execass_outbox_events(global_sequence,event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(2,'event-2','execass.v1.delegation.transitioned','delegation-1',1,'corr-2','cause-2',1800000000002,'v1','{\"summary\":\"second\"}','duplicate-2')",
            [],
        );
    assert!(
        gap_insert.is_err(),
        "schema rejects explicit global sequence gaps"
    );
    let conn = open_sqlite_connection(&paths.db_path).unwrap();
    conn.execute("DROP TRIGGER execass_outbox_global_sequence_gap_free", [])
        .unwrap();
    conn.execute(
        "INSERT INTO execass_outbox_events(global_sequence,event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(2,'event-2','execass.v1.delegation.transitioned','delegation-1',1,'corr-2','cause-2',1800000000002,'v1','{\"summary\":\"second\"}','duplicate-2')",
        [],
    )
    .unwrap();
    let gap_consumer = consumer("gap");
    assert!(matches!(
        store
            .replay_outbox_after_schema_tamper_for_test(&gap_consumer, 0)
            .unwrap(),
        OutboxReplayOutcome::SummaryRefetchRequired {
            reason: OutboxGapReason::SequenceGap,
            ..
        }
    ));
    let cursor_count: i64 = open_sqlite_connection(&paths.db_path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM execass_outbox_cursors", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(cursor_count, 0, "gap detection does not create a cursor");

    let (_root, paths, store) = fixture();
    insert_event(&paths, "event-1", 1, r#"{"summary":"first"}"#);
    let consumer = consumer("immutable");
    store
        .commit_outbox_delivery(OutboxDeliveryCommit {
            consumer: &consumer,
            expected_cursor: 0,
            global_sequence: 1,
            published_at: 1_800_000_000_100,
        })
        .unwrap();
    assert!(store
        .commit_outbox_delivery(OutboxDeliveryCommit {
            consumer: &consumer,
            expected_cursor: 1,
            global_sequence: 2,
            published_at: 1_800_000_000_101,
        })
        .is_err());
    let conn = open_sqlite_connection(&paths.db_path).unwrap();
    let cursor: i64 = conn
        .query_row(
            "SELECT last_global_sequence FROM execass_outbox_cursors WHERE consumer_id=?1",
            [&consumer.consumer_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        cursor, 1,
        "missing event commit rolls back cursor advancement"
    );
    assert!(conn
        .execute(
            "UPDATE execass_outbox_cursors SET principal_id='forged' WHERE consumer_id=?1",
            [&consumer.consumer_id],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_outbox_cursors WHERE consumer_id=?1",
            [&consumer.consumer_id],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_outbox_events WHERE global_sequence=1",
            []
        )
        .is_err());
}
