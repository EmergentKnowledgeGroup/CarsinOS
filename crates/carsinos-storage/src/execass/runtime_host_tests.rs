use super::tests::fixture;
use super::*;
use crate::open_sqlite_connection;
use rusqlite::params;
use sha2::{Digest, Sha256};

#[test]
fn runtime_host_activation_replays_exactly_and_takeover_is_monotonic_irreversible() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [33; 32]).unwrap();
    let first = fixture
        .store
        .activate_runtime_host(&authority, "gateway-host-a", 100)
        .unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(first.fencing_token, 1);
    assert_eq!(
        fixture
            .store
            .activate_runtime_host(&authority, "gateway-host-a", 101)
            .unwrap(),
        first
    );

    let second = fixture
        .store
        .activate_runtime_host(&authority, "gateway-host-b", 200)
        .unwrap();
    assert_eq!(second.generation, 2);
    assert_eq!(second.fencing_token, 2);
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT released_at FROM execass_runtime_host_leases WHERE lease_id=?1",
            params![first.lease_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        200
    );
    assert_eq!(
        conn.query_row(
            "SELECT ended_at||':'||end_reason FROM execass_runtime_host_generations WHERE generation=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "200:gateway_lock_takeover"
    );
    assert!(conn
        .execute(
            "UPDATE execass_runtime_host_leases SET released_at=NULL WHERE generation=1",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_runtime_host_generations SET ended_at=NULL,end_reason=NULL WHERE generation=1",
            [],
        )
        .is_err());
}

#[test]
fn runtime_host_activation_rejects_wrong_root_authority() {
    let first = fixture();
    let second = fixture();
    let wrong = activate_test_confirmation_authority(&second.store, [34; 32]).unwrap();
    assert!(first
        .store
        .activate_runtime_host(&wrong, "foreign-host", 100)
        .is_err());
}

#[test]
fn runtime_host_state_machine_has_exhaustive_allowed_and_forbidden_transitions() {
    use RuntimeActualState::{
        Draining, Faulted, Handoff, RunningAppBound, RunningBackground, Starting, Stopped,
    };
    use RuntimeDesiredMode::{AppBound, Background};
    use RuntimeHostTransition::{
        BeginDrain, BeginHandoff, CompleteStop, ReachDesiredMode, RecordFault,
    };

    let transitions = [
        ReachDesiredMode,
        BeginHandoff,
        BeginDrain,
        RecordFault,
        CompleteStop,
    ];
    for desired in [AppBound, Background] {
        for current in [
            Stopped,
            Starting,
            RunningAppBound,
            Handoff,
            RunningBackground,
            Draining,
            Faulted,
        ] {
            for transition in transitions {
                let allowed = matches!(
                    (current, desired, transition),
                    (Starting | Handoff, _, ReachDesiredMode)
                        | (RunningAppBound, Background, BeginHandoff)
                        | (RunningBackground, AppBound, BeginHandoff)
                        | (
                            Starting | RunningAppBound | Handoff | RunningBackground,
                            _,
                            BeginDrain
                        )
                        | (
                            Starting | RunningAppBound | Handoff | RunningBackground | Draining,
                            _,
                            RecordFault
                        )
                        | (Draining, _, CompleteStop)
                );
                assert_eq!(
                    super::runtime_host::resolve_transition(current, desired, transition).is_ok(),
                    allowed,
                    "current={}, desired={}, transition={transition:?}",
                    current.as_str(),
                    desired.as_str(),
                );
            }
        }
    }

    assert_eq!(
        super::runtime_host::resolve_transition(Starting, AppBound, ReachDesiredMode).unwrap(),
        (
            RunningAppBound,
            RuntimeHostTransitionReason::DesiredModeReached
        )
    );
    assert_eq!(
        super::runtime_host::resolve_transition(Handoff, Background, ReachDesiredMode).unwrap(),
        (
            RunningBackground,
            RuntimeHostTransitionReason::DesiredModeReached
        )
    );
    assert_eq!(
        super::runtime_host::resolve_transition(RunningAppBound, Background, BeginHandoff).unwrap(),
        (
            Handoff,
            RuntimeHostTransitionReason::DesiredModeRequiresHandoff
        )
    );
}

#[test]
fn runtime_host_state_writes_are_generation_fenced_and_status_is_persisted() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [35; 32]).unwrap();
    let first = fixture
        .store
        .activate_runtime_host(&authority, "gateway-host-a", 100)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .execass_runtime_host_status(100)
            .unwrap()
            .actual_state,
        RuntimeActualState::Starting
    );
    let reached = fixture
        .store
        .transition_runtime_host(&first, RuntimeHostTransition::ReachDesiredMode, 101)
        .unwrap();
    assert_eq!(reached.from_state, RuntimeActualState::Starting);
    assert_eq!(reached.actual_state, RuntimeActualState::RunningAppBound);
    assert_eq!(
        reached.reason,
        RuntimeHostTransitionReason::DesiredModeReached
    );
    assert_eq!(
        fixture
            .store
            .execass_runtime_host_status(101)
            .unwrap()
            .actual_state,
        RuntimeActualState::RunningAppBound
    );

    let second = fixture
        .store
        .activate_runtime_host(&authority, "gateway-host-b", 200)
        .unwrap();
    let stale =
        fixture
            .store
            .transition_runtime_host(&first, RuntimeHostTransition::BeginDrain, 201);
    assert!(stale.is_err());
    assert_eq!(
        fixture
            .store
            .execass_runtime_host_status(201)
            .unwrap()
            .live_lease,
        Some(second.clone())
    );

    fixture
        .store
        .transition_runtime_host(&second, RuntimeHostTransition::ReachDesiredMode, 201)
        .unwrap();
    fixture
        .store
        .transition_runtime_host(&second, RuntimeHostTransition::BeginDrain, 202)
        .unwrap();
    let stopped = fixture
        .store
        .transition_runtime_host(&second, RuntimeHostTransition::CompleteStop, 203)
        .unwrap();
    assert_eq!(stopped.actual_state, RuntimeActualState::Stopped);
    let status = fixture.store.execass_runtime_host_status(203).unwrap();
    assert_eq!(status.actual_state, RuntimeActualState::Stopped);
    assert!(status.live_lease.is_none());
}

#[test]
fn canonical_attention_schema_accepts_runtime_scope_without_a_fake_delegation() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [36; 32]).unwrap();
    let predecessor = fixture
        .store
        .activate_runtime_host(&authority, "gateway-host-runtime-attention-a", 100)
        .unwrap();
    let successor = fixture
        .store
        .activate_runtime_host(&authority, "gateway-host-runtime-attention-b", 200)
        .unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES('runtime-recovery-event','execass.v1.runtime_host.changed','execass-runtime-host',1,'runtime-recovery-correlation','runtime-recovery-cause',200,'v1','{}','runtime-recovery-duplicate')",
        [],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_receipts(
          receipt_id,delegation_id,receipt_sequence,global_sequence,
          append_identity,receipt_kind,causation_id,causation_event_id,parent_receipt_id,
          global_parent_receipt_id,subject_kind,subject_id,subject_revision,actor_type,
          actor_identity,actor_authority_provenance_id,runtime_host_generation,
          runtime_host_instance_id,runtime_fencing_token,state_revision,canonical_payload,
          serialization_version,hash_algorithm,key_id,key_generation,previous_receipt_digest,
          global_previous_receipt_digest,receipt_digest,keyed_integrity_tag,
          previous_key_integrity_tag,redacted_summary,occurred_at,committed_at
        ) VALUES(
          'runtime-recovery-receipt',NULL,NULL,1,
          'runtime-recovery-append','runtime_recovery','runtime-recovery-cause',
          'runtime-recovery-event',NULL,NULL,'runtime_host_generation','execass-runtime-host',1,
          'runtime','execass-global-control','execass-global-control-carrier-authority',?1,?2,?3,
          1,x'00','carsinos.execass.receipt.runtime-test.v1','sha256','runtime-test-key',1,
          NULL,NULL,'runtime-test-digest','runtime-test-tag',NULL,'runtime paused',200,200
        )"#,
        params![
            successor.generation,
            successor.host_instance_id,
            successor.fencing_token
        ],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_attention_items(
          attention_id,scope_kind,delegation_id,action_id,kind,status,reason,recommendation,
          alternatives_json,required_assurance,decision_id,delegation_revision,
          runtime_host_generation,runtime_host_instance_id,runtime_fencing_token,
          runtime_actual_state,runtime_end_reason,active_work_binding_digest,
          outbox_event_id,receipt_id,created_at,resolved_at
        ) VALUES(
          'runtime-paused-generation-1','runtime_host',NULL,NULL,'runtime_paused','actionable',
          'The runtime stopped unexpectedly.','Review paused work before resuming.','[]',
          'local_owner',NULL,NULL,?1,?2,?3,'starting','gateway_lock_takeover',?4,
          'runtime-recovery-event','runtime-recovery-receipt',200,NULL
        )"#,
        params![
            predecessor.generation,
            predecessor.host_instance_id,
            predecessor.fencing_token,
            format!("sha256:{}", "0".repeat(64)),
        ],
    )
    .unwrap();

    let stored: (String, Option<String>, i64) = conn
        .query_row(
            "SELECT scope_kind,delegation_id,runtime_host_generation FROM execass_attention_items WHERE attention_id='runtime-paused-generation-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        ("runtime_host".into(), None, predecessor.generation)
    );
    assert!(conn
        .execute(
            "UPDATE execass_attention_items SET delegation_id='execass-global-control-carrier' WHERE attention_id='runtime-paused-generation-1'",
            [],
        )
        .is_err());
}

#[test]
fn forced_takeover_atomically_records_runtime_attention_outbox_receipt_and_anchor() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [37; 32]).unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    integrity
        .provision_initial_key("runtime-recovery-key-1")
        .unwrap();
    let redactor = ReceiptRedactor::new(&["runtime-recovery-test-secret"]).unwrap();
    let predecessor = fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-atomic-a",
            100,
        )
        .unwrap();
    let successor = fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-atomic-b",
            200,
        )
        .unwrap();
    assert_eq!(successor.generation, predecessor.generation + 1);

    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let package: (i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM execass_attention_items WHERE scope_kind='runtime_host' AND runtime_host_generation=?1),(SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.runtime_host.changed' AND aggregate_revision=?1),(SELECT COUNT(*) FROM execass_receipts WHERE delegation_id IS NULL AND subject_kind='runtime_host_generation' AND subject_revision=?1)",
            [predecessor.generation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(package, (1, 1, 1));
    let predecessor_end: (String, String) = conn
        .query_row(
            "SELECT state.actual_state,generation.end_reason FROM execass_runtime_host_generations generation JOIN execass_runtime_host_states state ON state.generation=generation.generation AND state.host_instance_id=generation.host_instance_id WHERE generation.generation=?1",
            [predecessor.generation],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        predecessor_end,
        ("faulted".into(), "gateway_forced_exit_takeover".into())
    );
    let integrity_status = integrity.status().unwrap();
    assert!(
        matches!(
            integrity_status,
            IntegrityStatus::Trusted {
                receipt_count: 1,
                ..
            }
        ),
        "unexpected integrity status: {integrity_status:?}"
    );
    let projection = fixture
        .store
        .read_authoritative_projection(&integrity, &redactor, &ExecAssProjectionQuery::new(300))
        .unwrap();
    assert_eq!(projection.needs_you.len(), 1);
    let item = &projection.needs_you[0];
    assert_eq!(item.kind, NeedsYouKind::RuntimePaused);
    assert_eq!(
        item.subject,
        AttentionProjectionSubject::RuntimeHost {
            generation: predecessor.generation,
            host_instance_id: predecessor.host_instance_id.clone(),
            fencing_token: predecessor.fencing_token,
        }
    );
    assert_eq!(
        item.runtime_recovery
            .as_ref()
            .map(|evidence| evidence.predecessor_generation),
        Some(predecessor.generation)
    );
    let runtime_receipt = projection
        .receipts
        .items
        .iter()
        .find(|receipt| receipt.receipt_kind == ProjectionReceiptKind::RuntimeRecovery)
        .expect("runtime recovery receipt is projected");
    assert_eq!(runtime_receipt.delegation_id, None);
    assert_eq!(runtime_receipt.delegation_sequence, None);
}

#[test]
fn forced_takeover_rolls_back_generation_attention_and_receipt_on_outbox_collision() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [39; 32]).unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    integrity
        .provision_initial_key("runtime-rollback-key-1")
        .unwrap();
    let redactor = ReceiptRedactor::new(&["runtime-rollback-secret"]).unwrap();
    let predecessor = fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-rollback-a",
            100,
        )
        .unwrap();
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.runtime_recovery_incident.v1\0");
    digest.update(predecessor.generation.to_le_bytes());
    digest.update(predecessor.host_instance_id.as_bytes());
    digest.update([0]);
    digest.update(predecessor.fencing_token.to_le_bytes());
    let collision_identity = format!("runtime-recovery-{:x}", digest.finalize());
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    super::rows::insert_outbox(
        &conn,
        &NewOutboxEvent {
            event_id: "unrelated-collision-event".into(),
            event_name: OutboxEventName::SummaryChanged,
            aggregate_id: "unrelated-aggregate".into(),
            aggregate_revision: 1,
            correlation_id: "unrelated-correlation".into(),
            causation_id: "unrelated-cause".into(),
            occurred_at: 150,
            safe_payload_json: "{}".into(),
            duplicate_identity: collision_identity,
        },
    )
    .unwrap();
    drop(conn);

    assert!(fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-rollback-b",
            200,
        )
        .is_err());
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let preserved: (i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM execass_runtime_host_generations WHERE generation=2),(SELECT COUNT(*) FROM execass_runtime_host_generations WHERE generation=1 AND ended_at IS NULL),(SELECT COUNT(*) FROM execass_runtime_host_leases WHERE generation=1 AND released_at IS NULL),(SELECT COUNT(*) FROM execass_attention_items WHERE scope_kind='runtime_host'),(SELECT COUNT(*) FROM execass_receipts WHERE scope_kind='runtime_host')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(preserved, (0, 1, 1, 0, 0));
    assert!(matches!(
        integrity.status().unwrap(),
        IntegrityStatus::Uninitialized
    ));
}

#[test]
fn runtime_projection_fails_closed_when_exact_outbox_binding_is_tampered() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [40; 32]).unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    integrity
        .provision_initial_key("runtime-tamper-key-1")
        .unwrap();
    let redactor = ReceiptRedactor::new(&["runtime-tamper-secret"]).unwrap();
    fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-tamper-a",
            100,
        )
        .unwrap();
    fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-tamper-b",
            200,
        )
        .unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute_batch("DROP TRIGGER execass_outbox_events_immutable_identity;")
        .unwrap();
    conn.execute(
        "UPDATE execass_outbox_events SET aggregate_revision=999 WHERE event_name='execass.v1.runtime_host.changed'",
        [],
    )
    .unwrap();
    assert!(fixture
        .store
        .read_authoritative_projection(&integrity, &redactor, &ExecAssProjectionQuery::new(300),)
        .is_err());
}

#[test]
fn runtime_projection_fails_closed_for_each_authoritative_receipt_binding_tamper() {
    let mutations = [
        "delegation_id='execass-global-control-carrier'",
        "receipt_kind='completion'",
        "subject_kind='delegation'",
        "subject_id='tampered-runtime-subject'",
        "subject_revision=999",
    ];
    for (index, mutation) in mutations.into_iter().enumerate() {
        let fixture = fixture();
        let authority = activate_test_confirmation_authority(
            &fixture.store,
            [u8::try_from(41 + index).unwrap(); 32],
        )
        .unwrap();
        let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
        integrity
            .provision_initial_key(&format!("runtime-binding-tamper-key-{index}"))
            .unwrap();
        let redactor = ReceiptRedactor::new(&["runtime-binding-tamper-secret"]).unwrap();
        fixture
            .store
            .activate_runtime_host_with_recovery(
                &integrity,
                &redactor,
                &authority,
                "gateway-host-binding-tamper-a",
                100,
            )
            .unwrap();
        fixture
            .store
            .activate_runtime_host_with_recovery(
                &integrity,
                &redactor,
                &authority,
                "gateway-host-binding-tamper-b",
                200,
            )
            .unwrap();
        let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
        conn.execute_batch(
            "DROP TRIGGER execass_receipts_immutable; PRAGMA ignore_check_constraints=ON;",
        )
        .unwrap();
        conn.execute(
            &format!("UPDATE execass_receipts SET {mutation} WHERE scope_kind='runtime_host'"),
            [],
        )
        .unwrap();
        assert!(
            fixture
                .store
                .read_authoritative_projection(
                    &integrity,
                    &redactor,
                    &ExecAssProjectionQuery::new(300),
                )
                .is_err(),
            "projection accepted tampered runtime receipt binding: {mutation}"
        );
    }
}

#[test]
fn orderly_stopped_predecessor_creates_no_runtime_recovery_incident() {
    let fixture = fixture();
    let authority = activate_test_confirmation_authority(&fixture.store, [38; 32]).unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    integrity
        .provision_initial_key("runtime-clean-stop-key-1")
        .unwrap();
    let redactor = ReceiptRedactor::new(&["runtime-clean-stop-test-secret"]).unwrap();
    let predecessor = fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-clean-a",
            100,
        )
        .unwrap();
    fixture
        .store
        .transition_runtime_host(&predecessor, RuntimeHostTransition::BeginDrain, 101)
        .unwrap();
    fixture
        .store
        .transition_runtime_host(&predecessor, RuntimeHostTransition::CompleteStop, 102)
        .unwrap();
    fixture
        .store
        .activate_runtime_host_with_recovery(
            &integrity,
            &redactor,
            &authority,
            "gateway-host-clean-b",
            200,
        )
        .unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let counts: (i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM execass_attention_items WHERE scope_kind='runtime_host'),(SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.runtime_host.changed'),(SELECT COUNT(*) FROM execass_receipts WHERE delegation_id IS NULL AND subject_kind='runtime_host_generation')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[test]
fn recovery_activation_classifies_fault_and_interrupted_drain_and_same_host_replays_once() {
    let cases = [
        (RuntimeHostTransition::RecordFault, "gateway_fault_takeover"),
        (
            RuntimeHostTransition::BeginDrain,
            "gateway_drain_interrupted_takeover",
        ),
    ];
    for (index, (transition, expected_reason)) in cases.into_iter().enumerate() {
        let fixture = fixture();
        let authority = activate_test_confirmation_authority(
            &fixture.store,
            [u8::try_from(50 + index).unwrap(); 32],
        )
        .unwrap();
        let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
        integrity
            .provision_initial_key(&format!("runtime-classification-key-{index}"))
            .unwrap();
        let redactor = ReceiptRedactor::new(&["runtime-classification-secret"]).unwrap();
        let predecessor = fixture
            .store
            .activate_runtime_host_with_recovery(
                &integrity,
                &redactor,
                &authority,
                "gateway-host-classification-a",
                100,
            )
            .unwrap();
        let replay = fixture
            .store
            .activate_runtime_host_with_recovery(
                &integrity,
                &redactor,
                &authority,
                "gateway-host-classification-a",
                101,
            )
            .unwrap();
        assert_eq!(replay, predecessor);
        fixture
            .store
            .transition_runtime_host(&predecessor, transition, 102)
            .unwrap();
        fixture
            .store
            .activate_runtime_host_with_recovery(
                &integrity,
                &redactor,
                &authority,
                "gateway-host-classification-b",
                200,
            )
            .unwrap();
        let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
        let result: (String, i64, i64, i64) = conn
            .query_row(
                "SELECT generation.end_reason,(SELECT COUNT(*) FROM execass_attention_items WHERE scope_kind='runtime_host'),(SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.runtime_host.changed'),(SELECT COUNT(*) FROM execass_receipts WHERE scope_kind='runtime_host') FROM execass_runtime_host_generations generation WHERE generation.generation=?1",
                [predecessor.generation],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(result, (expected_reason.into(), 1, 1, 1));
    }
}
