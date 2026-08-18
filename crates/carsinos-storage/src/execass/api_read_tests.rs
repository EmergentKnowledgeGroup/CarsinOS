use super::tests::{fixture, foundation};
use super::*;
use std::time::Instant;

fn additional_foundation(suffix: &str, timestamp: i64) -> CreateFoundationCommand {
    let mut command = foundation();
    let delegation_id = format!("delegation-{suffix}");
    command.write.idempotency_key = format!("idem-foundation-{suffix}");
    command.write.correlation_id = format!("corr-foundation-{suffix}");
    command.write.causation_id = format!("cause-foundation-{suffix}");
    command.write.occurred_at = timestamp;
    command.authority.authority_provenance_id = format!("authority-{suffix}");
    command.authority.source_correlation_id = command.write.correlation_id.clone();
    command.authority.source_message_id = Some(format!("message-{suffix}"));
    command.authority.created_at = timestamp;
    command.delegation.delegation_id = delegation_id.clone();
    command.delegation.ingress_idempotency_key = command.write.idempotency_key.clone();
    command.delegation.source_correlation_id = command.write.correlation_id.clone();
    command.delegation.source_message_id = Some(format!("message-{suffix}"));
    command.delegation.authority_provenance_id = command.authority.authority_provenance_id.clone();
    command.delegation.created_at = timestamp;
    command.delegation.updated_at = timestamp;
    command.plan.plan_id = format!("plan-{suffix}");
    command.plan.delegation_id = delegation_id.clone();
    command.plan.created_by_authority_provenance_id =
        command.authority.authority_provenance_id.clone();
    command.plan.created_at = timestamp;
    for (index, criterion) in command.outcome_criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("criterion-{suffix}-{index}");
        criterion.delegation_id = delegation_id.clone();
        criterion.created_at = timestamp;
    }
    command.initial_continuation = None;
    command.outbox_event.event_id = format!("event-foundation-{suffix}");
    command.outbox_event.aggregate_id = delegation_id;
    command.outbox_event.correlation_id = command.write.correlation_id.clone();
    command.outbox_event.causation_id = command.write.causation_id.clone();
    command.outbox_event.occurred_at = timestamp;
    command.outbox_event.duplicate_identity = command.write.idempotency_key.clone();
    command
}

#[test]
fn api_list_cursor_is_tamper_detecting_and_empty_detail_is_safe() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let key = ApiCursorKey([7; 32]);
    let page = fixture
        .store
        .list_api_delegations(
            &ApiDelegationListQuery {
                phase: None,
                run_control: None,
                limit: 1,
                cursor: None,
            },
            &key,
        )
        .unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(fixture
        .store
        .read_api_delegation_detail("missing")
        .unwrap()
        .is_none());
    assert!(fixture
        .store
        .list_api_delegations(
            &ApiDelegationListQuery {
                phase: None,
                run_control: None,
                limit: 1,
                cursor: Some("forged.cursor".into()),
            },
            &key
        )
        .is_err());
}

#[test]
fn api_detail_and_receipts_are_canonical_and_nonce_free() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let detail = fixture
        .store
        .read_api_delegation_detail("delegation-1")
        .unwrap()
        .unwrap();
    assert_eq!(detail.delegation.delegation_id, "delegation-1");
    assert!(detail.current_plan.is_some());
    let receipts = fixture
        .store
        .read_api_delegation_receipts("delegation-1")
        .unwrap()
        .unwrap();
    assert_eq!(
        receipts.receipts.len() as i64,
        detail.delegation.receipt_chain_count
    );
    assert!(fixture
        .store
        .read_api_current_decision("missing")
        .unwrap()
        .is_none());
}

#[test]
fn api_list_paginates_without_duplicates_and_rejects_filter_reuse() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    fixture
        .store
        .create_foundation(&additional_foundation("2", 1_800_000_000_002))
        .unwrap();
    fixture
        .store
        .create_foundation(&additional_foundation("3", 1_800_000_000_003))
        .unwrap();
    let key = ApiCursorKey([9; 32]);
    let mut cursor = None;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let page = fixture
            .store
            .list_api_delegations(
                &ApiDelegationListQuery {
                    phase: None,
                    run_control: None,
                    limit: 1,
                    cursor: cursor.clone(),
                },
                &key,
            )
            .unwrap();
        assert_eq!(page.entries.len(), 1);
        ids.push(page.entries[0].delegation_id.clone());
        cursor = page.next_cursor;
    }
    assert_eq!(ids, ["delegation-3", "delegation-2", "delegation-1"]);
    assert!(cursor.is_none());

    let first = fixture
        .store
        .list_api_delegations(
            &ApiDelegationListQuery {
                phase: None,
                run_control: None,
                limit: 1,
                cursor: None,
            },
            &key,
        )
        .unwrap();
    assert!(fixture
        .store
        .list_api_delegations(
            &ApiDelegationListQuery {
                phase: Some(DelegationPhase::InMotion),
                run_control: None,
                limit: 1,
                cursor: first.next_cursor,
            },
            &key,
        )
        .is_err());
}

#[test]
fn ea313_thousand_delegation_summary_intake_duplicate_and_outbox_slo_floor() {
    const DELEGATION_COUNT: usize = 1_000;
    const SUMMARY_SAMPLES: usize = 25;
    const INTAKE_SAMPLES: usize = 64;
    let fixture = fixture();
    for index in 0..DELEGATION_COUNT {
        fixture
            .store
            .create_foundation(&additional_foundation(
                &format!("ea313-load-{index:04}"),
                1_800_000_100_000 + index as i64,
            ))
            .unwrap();
    }

    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let redactor = ReceiptRedactor::new(&["ea313-load-secret"]).unwrap();
    let query = ExecAssProjectionQuery::new(1_800_001_000_000);
    let initial = fixture
        .store
        .read_authoritative_projection(&integrity, &redactor, &query)
        .unwrap();
    assert_eq!(initial.in_motion.len(), DELEGATION_COUNT);

    let mut summary_ms = Vec::with_capacity(SUMMARY_SAMPLES);
    for _ in 0..SUMMARY_SAMPLES {
        let started = Instant::now();
        let projection = fixture
            .store
            .rebuild_authoritative_projection(&integrity, &redactor, &query)
            .unwrap();
        assert_eq!(projection.in_motion.len(), DELEGATION_COUNT);
        summary_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    summary_ms.sort_by(f64::total_cmp);
    let summary_p95 = summary_ms[(SUMMARY_SAMPLES * 95).div_ceil(100) - 1];
    assert!(
        summary_p95 <= 250.0,
        "warm summary p95 exceeded locked 250ms floor: {summary_p95:.2}ms"
    );

    let mut intake_ms = Vec::with_capacity(INTAKE_SAMPLES);
    let mut intake_commands = Vec::with_capacity(INTAKE_SAMPLES);
    for index in 0..INTAKE_SAMPLES {
        let command = additional_foundation(
            &format!("ea313-intake-{index:04}"),
            1_800_001_100_000 + index as i64,
        );
        let started = Instant::now();
        assert!(matches!(
            fixture.store.create_foundation(&command).unwrap(),
            FoundationWriteOutcome::Created(_)
        ));
        intake_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
        intake_commands.push(command);
    }
    intake_ms.sort_by(f64::total_cmp);
    let intake_p95 = intake_ms[(INTAKE_SAMPLES * 95).div_ceil(100) - 1];
    assert!(
        intake_p95 <= 500.0,
        "durable intake p95 exceeded locked 500ms floor: {intake_p95:.2}ms"
    );

    for command in &intake_commands {
        assert!(matches!(
            fixture.store.create_foundation(command).unwrap(),
            FoundationWriteOutcome::Replayed(_)
        ));
    }
    let consumer = OutboxConsumerIdentity {
        consumer_id: "ea313-consumer".into(),
        principal_id: "local-owner".into(),
        client_id_digest: "a".repeat(64),
    };
    let replay_started = Instant::now();
    let replay = fixture.store.replay_outbox(&consumer, 0).unwrap();
    let replay_ms = replay_started.elapsed().as_secs_f64() * 1_000.0;
    let OutboxReplayOutcome::Replay(replay) = replay else {
        panic!("fresh EA-313 consumer unexpectedly required summary refetch");
    };
    assert_eq!(replay.events.len(), DELEGATION_COUNT + INTAKE_SAMPLES);
    assert_eq!(replay.head_global_sequence, replay.events.len() as i64);

    println!(
        "{}",
        serde_json::json!({
            "fixture": "execass.ea313.slo.v1",
            "delegations": DELEGATION_COUNT,
            "warm_summary_p95_ms": summary_p95,
            "durable_intake_p95_ms": intake_p95,
            "outbox_replay_ms": replay_ms,
            "outbox_events": replay.events.len(),
            "duplicate_ingress_replays": INTAKE_SAMPLES,
        })
    );
}
