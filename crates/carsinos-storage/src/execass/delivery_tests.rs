use super::tests::{fixture, foundation, table_count};
use super::*;
use crate::open_sqlite_connection;
use chrono::{TimeZone, Utc};
use rusqlite::params;

const NOW: i64 = 1_800_000_000_000;

fn empty_projection() -> ExecAssExecutiveProjection {
    let mut projection = ExecAssExecutiveProjection {
        projection_version: "execass_projection.v1".into(),
        observed_at_ms: NOW,
        boundary: ProjectionBoundary {
            through_global_sequence: 0,
            database_receipt_count: 0,
            database_receipt_head_digest: None,
            item_set_digest: String::new(),
        },
        integrity: ProjectionIntegrity::Untrusted {
            failure: ProjectionIntegrityFailure::Uninitialized,
        },
        needs_you: vec![],
        in_motion: vec![],
        done_since_you_checked: vec![],
        next: vec![],
        receipts: ReceiptProjectionWindow {
            limit: 0,
            total: 0,
            has_older: false,
            earliest_global_sequence: None,
            latest_global_sequence: None,
            items: vec![],
        },
        reef: vec![],
    };
    projection.boundary.item_set_digest = super::projection::item_set_digest(&projection).unwrap();
    projection
}

fn delivery(
    projection: &ExecAssExecutiveProjection,
    id: &str,
    request: &str,
) -> SummaryDeliveryCommand {
    SummaryDeliveryCommand {
        delivery_id: id.into(),
        request_identity: request.into(),
        delivered_at: NOW,
        projection_version: projection.projection_version.clone(),
        through_global_sequence: projection.boundary.through_global_sequence,
        item_set_digest: projection.boundary.item_set_digest.clone(),
        items: super::projection::canonical_delivered_items(projection).unwrap(),
    }
}

#[test]
fn delivery_empty_replay_and_exact_ack_idempotency_are_safe() {
    let fixture = fixture();
    let projection = empty_projection();
    let command = delivery(&projection, "delivery-empty", "request-empty");
    let SummaryDeliveryOutcome::Recorded(record) = fixture
        .store
        .record_summary_delivery(&projection, &command)
        .unwrap()
    else {
        panic!("must record");
    };
    assert!(matches!(
        fixture
            .store
            .record_summary_delivery(&projection, &command)
            .unwrap(),
        SummaryDeliveryOutcome::Replayed(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_summary_deliveries"), 1);

    let ack = SummaryAcknowledgementCommand {
        delivery_id: record.delivery_id.clone(),
        displayed_cursor: record.displayed_cursor.clone(),
        idempotency_key: "ack-one".into(),
        acknowledged_at: NOW + 1,
        items: record.items.clone(),
    };
    assert!(matches!(
        fixture.store.acknowledge_summary_delivery(&ack).unwrap(),
        SummaryAcknowledgementOutcome::Acknowledged(_)
    ));
    let second_key = SummaryAcknowledgementCommand {
        idempotency_key: "ack-two".into(),
        ..ack.clone()
    };
    assert!(matches!(
        fixture
            .store
            .acknowledge_summary_delivery(&second_key)
            .unwrap(),
        SummaryAcknowledgementOutcome::Replayed(_)
    ));
    assert_eq!(
        table_count(&fixture.paths, "execass_summary_acknowledgements"),
        1
    );
}

#[test]
fn delivery_conflict_and_cross_delivery_idempotency_collision_are_rejected() {
    let fixture = fixture();
    let projection = empty_projection();
    let first = delivery(&projection, "delivery-one", "request-one");
    let SummaryDeliveryOutcome::Recorded(first_record) = fixture
        .store
        .record_summary_delivery(&projection, &first)
        .unwrap()
    else {
        panic!("first delivery");
    };
    let changed = SummaryDeliveryCommand {
        request_identity: "different-request".into(),
        ..first.clone()
    };
    assert!(matches!(
        fixture
            .store
            .record_summary_delivery(&projection, &changed)
            .unwrap(),
        SummaryDeliveryOutcome::Conflict
    ));
    let mut second = delivery(&projection, "delivery-two", "request-two");
    second.delivered_at += 1;
    let SummaryDeliveryOutcome::Recorded(second_record) = fixture
        .store
        .record_summary_delivery(&projection, &second)
        .unwrap()
    else {
        panic!("second delivery");
    };
    let ack_one = SummaryAcknowledgementCommand {
        delivery_id: first_record.delivery_id,
        displayed_cursor: first_record.displayed_cursor,
        idempotency_key: "cross-key".into(),
        acknowledged_at: NOW + 2,
        items: vec![],
    };
    fixture
        .store
        .acknowledge_summary_delivery(&ack_one)
        .unwrap();
    let crossed = SummaryAcknowledgementCommand {
        delivery_id: second_record.delivery_id,
        displayed_cursor: second_record.displayed_cursor,
        ..ack_one
    };
    assert!(matches!(
        fixture
            .store
            .acknowledge_summary_delivery(&crossed)
            .unwrap(),
        SummaryAcknowledgementOutcome::Conflict
    ));
    let stale = SummaryAcknowledgementCommand {
        delivery_id: "unknown-delivery".into(),
        displayed_cursor: "unknown-cursor".into(),
        idempotency_key: "cross-key".into(),
        acknowledged_at: NOW + 3,
        items: vec![],
    };
    assert!(matches!(
        fixture.store.acknowledge_summary_delivery(&stale).unwrap(),
        SummaryAcknowledgementOutcome::Conflict
    ));
}

#[test]
fn reconnect_gap_keeps_prior_exact_delivery_acknowledgeable_without_acknowledging_newer() {
    let fixture = fixture();
    let projection = empty_projection();
    let first = delivery(&projection, "gap-a", "gap-request-a");
    let SummaryDeliveryOutcome::Recorded(a) = fixture
        .store
        .record_summary_delivery(&projection, &first)
        .unwrap()
    else {
        panic!("a");
    };
    let mut second = delivery(&projection, "gap-b", "gap-request-b");
    second.delivered_at += 1;
    let SummaryDeliveryOutcome::Recorded(b) = fixture
        .store
        .record_summary_delivery(&projection, &second)
        .unwrap()
    else {
        panic!("b");
    };
    let ack_a = SummaryAcknowledgementCommand {
        delivery_id: a.delivery_id.clone(),
        displayed_cursor: a.displayed_cursor.clone(),
        idempotency_key: "gap-ack-a".into(),
        acknowledged_at: NOW + 2,
        items: a.items.clone(),
    };
    assert!(matches!(
        fixture.store.acknowledge_summary_delivery(&ack_a).unwrap(),
        SummaryAcknowledgementOutcome::Acknowledged(_)
    ));
    let wrong_cursor = SummaryAcknowledgementCommand {
        displayed_cursor: b.displayed_cursor,
        idempotency_key: "gap-wrong".into(),
        ..ack_a
    };
    assert!(matches!(
        fixture
            .store
            .acknowledge_summary_delivery(&wrong_cursor)
            .unwrap(),
        SummaryAcknowledgementOutcome::NotDelivered
    ));
    assert_eq!(
        table_count(&fixture.paths, "execass_summary_acknowledgements"),
        1
    );
}

fn seed_actionable_reply() -> (
    super::tests::Fixture,
    NotificationScheduleCommand,
    ReceiptRedactor,
) {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_attention_items(attention_id,delegation_id,kind,status,reason,recommendation,alternatives_json,required_assurance,delegation_revision,created_at) VALUES('attention-1','delegation-1','reply','actionable','reply needed','respond','[]','owner',1,?1)",
        [NOW],
    ).unwrap();
    let command = NotificationScheduleCommand {
        notification_id: "notification-1".into(),
        source: NotificationSource::Attention {
            attention_id: "attention-1".into(),
        },
        delegation_id: "delegation-1".into(),
        decision_id: None,
        reason_revision: 1,
        channel: "local".into(),
        reason: SafeText::new("reply needed", &[]).unwrap(),
        safe_payload: SafeJson::from_str(r#"{"kind":"reply"}"#, &[]).unwrap(),
        scheduled_at: NOW,
        quiet_hours: None,
        idempotency_key: "notify-key".into(),
    };
    (
        fixture,
        command,
        ReceiptRedactor::new(&["delivery-test-secret"]).unwrap(),
    )
}

#[test]
fn notification_is_atomic_deduped_and_never_publishes_transport() {
    let (fixture, command, redactor) = seed_actionable_reply();
    assert!(matches!(
        fixture
            .store
            .schedule_notification(&command, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::Scheduled(_)
    ));
    assert!(matches!(
        fixture
            .store
            .schedule_notification(&command, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::Replayed(_)
    ));
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM execass_notifications", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.notification.scheduled' AND published_at IS NOT NULL", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    conn.execute("UPDATE execass_attention_items SET status='resolved',resolved_at=?1 WHERE attention_id='attention-1'", params![NOW + 1]).unwrap();
    assert!(matches!(
        fixture
            .store
            .advance_notification_reminder("notification-1", NOW + 60 * 60 * 1000)
            .unwrap(),
        NotificationScheduleOutcome::Cancelled(_)
    ));
}

#[test]
fn invalid_quiet_hours_and_unconfigured_completion_fail_closed() {
    let (fixture, mut command, redactor) = seed_actionable_reply();
    command.quiet_hours = Some(QuietHoursPolicy {
        timezone: "Not/AZone".into(),
        start_minute: 1,
        end_minute: 2,
    });
    assert!(fixture
        .store
        .schedule_notification(&command, &redactor)
        .is_err());
    command.source = NotificationSource::Completion {
        completion_assessment_id: "no-assessment".into(),
        completion_enabled: false,
    };
    command.quiet_hours = None;
    assert!(matches!(
        fixture
            .store
            .schedule_notification(&command, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::NotActionable
    ));
}

#[test]
fn ack_rejects_omitted_duplicate_and_foreign_rendered_items() {
    let fixture = fixture();
    let mut projection = empty_projection();
    projection.next.push(NextProjectionItem {
        item_id: "next-1".into(),
        item_revision: 1,
        delegation_id: None,
        kind: NextKind::RecoveryReevaluation,
        due_at_ms: NOW + 1,
        details: NextDetails::RecoveryReevaluation,
        created_at_ms: NOW,
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::AuthorityRecord,
            target_id: "next-1".into(),
        },
    });
    projection.boundary.item_set_digest = super::projection::item_set_digest(&projection).unwrap();
    let command = delivery(&projection, "delivery-rendered", "request-rendered");
    let SummaryDeliveryOutcome::Recorded(record) = fixture
        .store
        .record_summary_delivery(&projection, &command)
        .unwrap()
    else {
        panic!("recorded");
    };
    let omitted = SummaryAcknowledgementCommand {
        delivery_id: record.delivery_id.clone(),
        displayed_cursor: record.displayed_cursor.clone(),
        idempotency_key: "omit".into(),
        acknowledged_at: NOW + 1,
        items: vec![],
    };
    assert!(matches!(
        fixture
            .store
            .acknowledge_summary_delivery(&omitted)
            .unwrap(),
        SummaryAcknowledgementOutcome::Conflict
    ));
    let duplicate = SummaryAcknowledgementCommand {
        idempotency_key: "duplicate".into(),
        items: vec![record.items[0].clone(), record.items[0].clone()],
        ..omitted.clone()
    };
    assert!(fixture
        .store
        .acknowledge_summary_delivery(&duplicate)
        .is_err());
    let foreign = SummaryAcknowledgementCommand {
        delivery_id: "not-delivered".into(),
        idempotency_key: "foreign".into(),
        items: record.items,
        ..omitted
    };
    assert!(matches!(
        fixture
            .store
            .acknowledge_summary_delivery(&foreign)
            .unwrap(),
        SummaryAcknowledgementOutcome::NotDelivered
    ));
}

#[test]
fn quiet_hours_handle_overnight_and_dst_gap_without_dispatching_early() {
    let chicago = QuietHoursPolicy {
        timezone: "America/Chicago".into(),
        start_minute: 22 * 60,
        end_minute: 7 * 60,
    };
    let overnight = Utc
        .with_ymd_and_hms(2024, 1, 15, 5, 0, 0)
        .unwrap()
        .timestamp_millis(); // 23:00 CST
    assert_eq!(
        super::delivery::defer_for_quiet_hours(overnight, Some(&chicago)).unwrap(),
        Utc.with_ymd_and_hms(2024, 1, 15, 13, 0, 0)
            .unwrap()
            .timestamp_millis()
    );
    let dst = QuietHoursPolicy {
        timezone: "America/Chicago".into(),
        start_minute: 60,
        end_minute: 150,
    };
    let spring_gap = Utc
        .with_ymd_and_hms(2024, 3, 10, 7, 30, 0)
        .unwrap()
        .timestamp_millis(); // 01:30 CST
    assert_eq!(
        super::delivery::defer_for_quiet_hours(spring_gap, Some(&dst)).unwrap(),
        Utc.with_ymd_and_hms(2024, 3, 10, 8, 0, 0)
            .unwrap()
            .timestamp_millis()
    );
}

#[test]
fn notification_reminder_cap_and_schema_history_guards_hold() {
    let (fixture, command, redactor) = seed_actionable_reply();
    fixture
        .store
        .schedule_notification(&command, &redactor)
        .unwrap();
    for step in 1..=3 {
        let now = NOW + i64::from(step) * 60 * 60 * 1000;
        assert!(matches!(
            fixture
                .store
                .advance_notification_reminder("notification-1", now)
                .unwrap(),
            NotificationScheduleOutcome::Scheduled(_)
        ));
    }
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(conn.query_row("SELECT reminder_count FROM execass_notifications WHERE notification_id='notification-1'", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
    assert!(conn.query_row("SELECT next_reminder_at FROM execass_notifications WHERE notification_id='notification-1'", [], |r| r.get::<_, Option<i64>>(0)).unwrap().is_none());
    assert!(conn.execute("UPDATE execass_notifications SET reminder_count=0 WHERE notification_id='notification-1'", []).is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_notifications WHERE notification_id='notification-1'",
            []
        )
        .is_err());
}

#[test]
fn raw_secret_notification_input_fails_before_any_row_or_outbox_is_written() {
    let (fixture, mut command, redactor) = seed_actionable_reply();
    command.reason = SafeText::new("delivery-test-secret", &[]).unwrap();
    assert!(fixture
        .store
        .schedule_notification(&command, &redactor)
        .is_err());
    assert_eq!(table_count(&fixture.paths, "execass_notifications"), 0);
    assert_eq!(table_count(&fixture.paths, "execass_outbox_events"), 1); // foundation only
}

#[test]
fn done_correction_changes_source_revision_and_overflow_fails_closed() {
    let before =
        super::projection::delivered_item(SummaryProjectionKind::Done, "assessment-1", &[7, 3, 0])
            .unwrap();
    let corrected =
        super::projection::delivered_item(SummaryProjectionKind::Done, "assessment-1", &[7, 3, 1])
            .unwrap();
    assert_eq!(before.item_id, corrected.item_id);
    assert_eq!(before.revision + 1, corrected.revision);
    assert!(super::projection::delivered_item(
        SummaryProjectionKind::Done,
        "assessment-1",
        &[i64::MAX, 1]
    )
    .is_err());
}

#[test]
fn five_api_panes_have_namespaced_positive_source_revisions() {
    let mut projection = empty_projection();
    projection.needs_you.push(NeedsYouProjectionItem {
        attention_id: "attention".into(),
        subject: AttentionProjectionSubject::Delegation {
            delegation_id: "delegation".into(),
            delegation_revision: 2,
        },
        kind: NeedsYouKind::Clarification,
        decision_id: Some("decision".into()),
        decision_kind: Some(ProjectionDecisionKind::Clarification),
        decision_revision: Some(3),
        reason: "clarify".into(),
        recommendation: "answer".into(),
        alternative_count: 0,
        alternatives: vec![],
        required_assurance: "owner".into(),
        deadline_ms: None,
        created_at_ms: NOW,
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::Decision,
            target_id: "decision".into(),
        },
        runtime_recovery: None,
    });
    projection.in_motion.push(InMotionProjectionItem {
        delegation_id: "delegation".into(),
        delegation_revision: 5,
        underlying_phase: ProjectionDelegationPhase::InMotion,
        state: InMotionState::Active,
        policy_revision: 1,
        external_wait_json: None,
        stop_epoch: 0,
        created_at_ms: NOW,
        acknowledged_at_ms: None,
        runnable_branch_count: 1,
        executing_branch_count: 1,
        waiting_external_count: 0,
        updated_at_ms: NOW,
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::Delegation,
            target_id: "delegation".into(),
        },
    });
    projection.done_since_you_checked.push(DoneProjectionItem {
        delegation_id: "done-delegation".into(),
        delegation_revision: 7,
        assessment_id: "assessment".into(),
        assessment_revision: 11,
        outcome: DoneOutcome::Completed,
        policy_revision: 1,
        run_control: "running".into(),
        stop_epoch: 0,
        created_at_ms: NOW,
        acknowledged_at_ms: None,
        trust: ProjectionTrust::Trusted,
        useful_outcome: true,
        material_pass_count: 1,
        material_fail_count: 0,
        material_unknown_count: 0,
        what_did_not_happen: vec![],
        correction_id: Some("correction".into()),
        correction_revision: Some(13),
        correction_warning: Some("corrected".into()),
        correction_deep_link: Some(ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::Receipt,
            target_id: "correction".into(),
        }),
        terminal_receipt_deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::Receipt,
            target_id: "terminal-receipt".into(),
        },
        terminal_at_ms: NOW,
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::Delegation,
            target_id: "done-delegation".into(),
        },
    });
    projection.next.push(NextProjectionItem {
        item_id: "next".into(),
        item_revision: 17,
        delegation_id: None,
        kind: NextKind::RecoveryReevaluation,
        due_at_ms: NOW + 1,
        details: NextDetails::RecoveryReevaluation,
        created_at_ms: NOW,
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::AuthorityRecord,
            target_id: "next".into(),
        },
    });
    projection.receipts.items.push(ReceiptProjectionItem {
        receipt_id: "receipt".into(),
        delegation_id: Some("delegation".into()),
        delegation_sequence: Some(1),
        global_sequence: 19,
        receipt_kind: ProjectionReceiptKind::Action,
        subject_kind: ProjectionReceiptSubjectKind::ActionBranch,
        subject_id: "branch".into(),
        subject_revision: 23,
        receipt_digest: "digest".into(),
        delegation_previous_receipt_digest: None,
        global_previous_receipt_digest: None,
        key_id: "key".into(),
        key_generation: 1,
        integrity_tag: "tag".into(),
        previous_key_integrity_tag: None,
        trust: ProjectionTrust::Trusted,
        redacted_summary: "worked".into(),
        occurred_at_ms: NOW,
        committed_at_ms: NOW,
        evidence: vec![],
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::Receipt,
            target_id: "receipt".into(),
        },
    });

    let items = super::projection::canonical_delivered_items(&projection).unwrap();
    assert_eq!(items.len(), 5);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.projection_kind, item.item_id.as_str(), item.revision))
            .collect::<Vec<_>>(),
        vec![
            (SummaryProjectionKind::Done, "done:assessment", 31),
            (SummaryProjectionKind::InMotion, "in_motion:delegation", 5),
            (SummaryProjectionKind::NeedsYou, "needs_you:attention", 5),
            (SummaryProjectionKind::Next, "next:next", 17),
            (SummaryProjectionKind::Receipts, "receipts:receipt", 42),
        ]
    );
}

#[test]
fn dangerous_attention_quiet_deferral_past_expiry_is_not_scheduled() {
    let (fixture, confirmation, _, expiry) = super::tests::prepared_attested_confirmation();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let (delegation_id, delegation_revision, reason): (String, i64, String) = conn
        .query_row(
            "SELECT delegation_id,delegation_revision,consequence FROM execass_decisions WHERE decision_id=?1",
            [&confirmation.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO execass_attention_items(attention_id,delegation_id,decision_id,kind,status,reason,recommendation,alternatives_json,required_assurance,delegation_revision,created_at) VALUES('danger-expiry-attention',?1,?2,'confirmation','actionable',?3,'confirm or revise','[]','verified_owner',?4,?5)",
        params![delegation_id, confirmation.decision_id, reason, delegation_revision, NOW],
    )
    .unwrap();
    let before_outbox = table_count(&fixture.paths, "execass_outbox_events");
    let command = NotificationScheduleCommand {
        notification_id: "danger-expiry-notification".into(),
        source: NotificationSource::Attention {
            attention_id: "danger-expiry-attention".into(),
        },
        delegation_id,
        decision_id: Some(confirmation.decision_id),
        reason_revision: confirmation.decision_revision,
        channel: "local".into(),
        reason: SafeText::new("dangerous action needs confirmation", &[]).unwrap(),
        safe_payload: SafeJson::from_str("{}", &[]).unwrap(),
        scheduled_at: expiry - 1,
        quiet_hours: Some(QuietHoursPolicy {
            timezone: "UTC".into(),
            start_minute: 0,
            end_minute: 1439,
        }),
        idempotency_key: "danger-expiry-key".into(),
    };
    let redactor = ReceiptRedactor::new(&["ea306-secret"]).unwrap();
    assert!(matches!(
        fixture
            .store
            .schedule_notification(&command, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::NotActionable
    ));
    assert_eq!(table_count(&fixture.paths, "execass_notifications"), 0);
    assert_eq!(
        table_count(&fixture.paths, "execass_outbox_events"),
        before_outbox
    );
}

#[test]
fn delivery_and_ack_history_cannot_be_deleted_directly() {
    let fixture = fixture();
    let mut projection = empty_projection();
    projection.next.push(NextProjectionItem {
        item_id: "sealed-next".into(),
        item_revision: 1,
        delegation_id: None,
        kind: NextKind::RecoveryReevaluation,
        due_at_ms: NOW + 1,
        details: NextDetails::RecoveryReevaluation,
        created_at_ms: NOW,
        deep_link: ProjectionDeepLink {
            kind: ProjectionDeepLinkKind::AuthorityRecord,
            target_id: "sealed-next".into(),
        },
    });
    projection.boundary.item_set_digest = super::projection::item_set_digest(&projection).unwrap();
    let command = delivery(&projection, "delivery-history", "request-history");
    let SummaryDeliveryOutcome::Recorded(record) = fixture
        .store
        .record_summary_delivery(&projection, &command)
        .unwrap()
    else {
        panic!("record");
    };
    let ack = SummaryAcknowledgementCommand {
        delivery_id: record.delivery_id.clone(),
        displayed_cursor: record.displayed_cursor,
        idempotency_key: "ack-history".into(),
        acknowledged_at: NOW + 1,
        items: record.items,
    };
    fixture.store.acknowledge_summary_delivery(&ack).unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            "DELETE FROM execass_summary_delivery_items WHERE delivery_id='delivery-history'",
            []
        )
        .is_err());
    assert!(conn.execute("INSERT INTO execass_summary_delivery_items(delivery_id,item_id,item_revision,projection_kind) VALUES('delivery-history','next:forged',1,'next')", []).is_err());
    assert!(conn
        .execute("DELETE FROM execass_summary_acknowledgements", [])
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_summary_deliveries WHERE delivery_id='delivery-history'",
            []
        )
        .is_err());
}

#[test]
fn distinct_attention_sources_at_same_locked_dedupe_key_are_not_double_scheduled() {
    let (fixture, command, redactor) = seed_actionable_reply();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute("INSERT INTO execass_attention_items(attention_id,delegation_id,kind,status,reason,recommendation,alternatives_json,required_assurance,delegation_revision,created_at) VALUES('attention-2','delegation-1','reply','actionable','reply needed','respond','[]','owner',1,?1)", [NOW + 1]).unwrap();
    fixture
        .store
        .schedule_notification(&command, &redactor)
        .unwrap();
    let second = NotificationScheduleCommand {
        notification_id: "notification-2".into(),
        source: NotificationSource::Attention {
            attention_id: "attention-2".into(),
        },
        idempotency_key: "notify-key-2".into(),
        ..command
    };
    assert!(matches!(
        fixture
            .store
            .schedule_notification(&second, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::Conflict
    ));
    assert_eq!(table_count(&fixture.paths, "execass_notifications"), 1);
}

#[test]
fn configured_receipt_backed_completion_schedules_once_and_disabled_cannot_replay() {
    let completed = super::completion_tests::setup(&[true], false);
    super::completion_tests::insert_result(&completed, 1, 1, "pass");
    let outcome = super::completion_tests::assess(
        &completed,
        &super::completion_tests::assessment_command(&completed, "notification-completion"),
    );
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } = outcome else {
        panic!("terminal completion");
    };
    let command = NotificationScheduleCommand {
        notification_id: "completion-notification".into(),
        source: NotificationSource::Completion {
            completion_assessment_id: assessment.assessment_id.clone(),
            completion_enabled: true,
        },
        delegation_id: "delegation-1".into(),
        decision_id: None,
        reason_revision: assessment.assessment_revision,
        channel: "local".into(),
        reason: SafeText::new("completed", &[]).unwrap(),
        safe_payload: SafeJson::from_str("{}", &[]).unwrap(),
        scheduled_at: NOW,
        quiet_hours: None,
        idempotency_key: "completion-notify-key".into(),
    };
    let redactor = ReceiptRedactor::new(&["ea306-secret"]).unwrap();
    assert!(matches!(
        completed
            .fixture
            .store
            .schedule_notification(&command, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::Scheduled(_)
    ));
    let disabled = NotificationScheduleCommand {
        source: NotificationSource::Completion {
            completion_assessment_id: assessment.assessment_id,
            completion_enabled: false,
        },
        ..command
    };
    assert!(matches!(
        completed
            .fixture
            .store
            .schedule_notification(&disabled, &redactor)
            .unwrap(),
        NotificationScheduleOutcome::Conflict
    ));
    assert_eq!(
        table_count(&completed.fixture.paths, "execass_notifications"),
        1
    );
}
