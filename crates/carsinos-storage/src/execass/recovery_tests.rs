use super::decision_tests::duplicate_risk_fixture_with_accepted_dangerous_grant;
use super::recorder_tests::{
    signed_execution_command_for_attempt, signed_execution_command_for_attempt_with_class,
};
use super::resource_tests::{acquire, claim_command, setup_recorder, ResourceFixture, CLAIM_NOW};
use super::*;
use crate::open_sqlite_connection;
use carsinos_core::execass_recovery::{RecoveryDelayReason, RecoveryDirective};
use carsinos_protocol::execass_recorder::{ProviderFailureClassV1, RecorderObservationKindV1};

fn claimed(suffix: &str) -> (ResourceFixture, ContinuationClaimIdentity) {
    claimed_with_quota(suffix, 100, 1)
}

fn claimed_with_quota(
    suffix: &str,
    limit: i64,
    required: i64,
) -> (ResourceFixture, ContinuationClaimIdentity) {
    let fixture = setup_recorder(1, limit, required);
    let job = acquire(&fixture, "recovery-worker", 1).remove(0);
    let command = claim_command(
        &fixture,
        "continuation-1",
        &job.job_id,
        "recovery-worker",
        job.lease_expires_at.unwrap(),
        suffix,
    );
    let ContinuationClaimOutcome::Claimed(record) = fixture
        .fixture
        .store
        .claim_continuation_atomically(&fixture.integrity, &fixture.redactor, &command)
        .unwrap()
    else {
        panic!("expected recovery claim")
    };
    (fixture, record.identity)
}

fn begin(fixture: &ResourceFixture, claim: &ContinuationClaimIdentity) -> ProviderAttemptRecord {
    let PrepareProviderAttemptOutcome::Prepared(attempt) = fixture
        .fixture
        .store
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: claim.clone(),
            trusted_now: CLAIM_NOW + 1,
            retry_authorization: None,
        })
        .unwrap()
    else {
        panic!("expected prepared recovery attempt")
    };
    let BeginProviderAttemptInvocationOutcome::Began(attempt) = fixture
        .fixture
        .store
        .begin_provider_attempt_invocation(&BeginProviderAttemptInvocationCommand {
            attempt_id: attempt.attempt_id.clone(),
            claim: claim.clone(),
            trusted_now: CLAIM_NOW + 2,
        })
        .unwrap()
    else {
        panic!("expected begun recovery attempt")
    };
    *attempt
}

fn atomic_recovery_command(
    fixture: &ResourceFixture,
    logical_effect_id: &str,
    trusted_now: i64,
    suffix: &str,
) -> ProviderRecoveryCommand {
    let context = fixture
        .fixture
        .store
        .read_continuation_receipt_context("continuation-1", trusted_now)
        .unwrap()
        .expect("recovery receipt context");
    let post_revision = context.delegation_revision + 1;
    let event_id = format!("recovery-event-{suffix}");
    let causation_id = format!("recovery-cause-{suffix}");
    ProviderRecoveryCommand {
        write: WriteContext {
            idempotency_key: format!("recovery-write-{suffix}"),
            correlation_id: format!("recovery-correlation-{suffix}"),
            causation_id: causation_id.clone(),
            occurred_at: trusted_now,
        },
        logical_effect_id: logical_effect_id.to_owned(),
        trusted_now,
        expected_pre_state_revision: context.delegation_revision,
        receipt: AppendReceiptCommand {
            receipt_id: format!("recovery-receipt-{suffix}"),
            transaction_id: format!("recovery-transaction-{suffix}"),
            state_root_generation: context.state_root_generation,
            delegation_id: context.delegation_id,
            expected_state_revision: post_revision,
            expected_global_count: context.global_receipt_count,
            expected_global_head_digest: context.global_receipt_head_digest,
            expected_delegation_count: context.delegation_receipt_count,
            expected_delegation_head_digest: context.delegation_receipt_head_digest,
            receipt_kind: ReceiptKind::Recovery,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::OutboxEvent,
                subject_id: event_id.clone(),
                revision: post_revision,
            },
            causation_id,
            causation_event_id: event_id,
            actor: context.runtime_actor,
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id,
                fencing_token: context.runtime_fencing_token,
            },
            key: fixture.key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: fixture
                .redactor
                .summary("ExecAss objective recovery updated")
                .unwrap(),
            occurred_at: trusted_now,
            committed_at: trusted_now,
        },
    }
}

#[test]
fn atomic_recovery_commits_evaluation_lifecycle_outbox_and_receipt_once() {
    let (fixture, claim) = claimed("atomic-objective-recovery");
    let first = begin(&fixture, &claim);
    let absence = signed_execution_command_for_attempt(
        &fixture,
        &claim,
        &first,
        RecorderObservationKindV1::Absent,
        "atomic-objective-recovery",
        1,
        CLAIM_NOW + 3,
    );
    fixture
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(&fixture.integrity, &fixture.redactor, &absence)
        .unwrap();
    let command = atomic_recovery_command(
        &fixture,
        "effect-1",
        CLAIM_NOW + 3,
        "atomic-objective-recovery",
    );
    let applied = fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &command)
        .unwrap();
    let ProviderRecoveryOutcome::Applied(bundle) = applied else {
        panic!("expected applied atomic recovery")
    };
    assert!(matches!(
        bundle.evaluation.directive,
        RecoveryDirective::WaitUntil {
            reason: RecoveryDelayReason::Backoff,
            ..
        }
    ));
    assert_eq!(bundle.selected_phase, DelegationPhase::Recovering);
    assert_eq!(
        bundle.state_revision,
        command.expected_pre_state_revision + 1
    );
    assert_eq!(bundle.receipt.receipt_id, command.receipt.receipt_id);
    assert_eq!(
        bundle.outbox_event.event.event_id,
        command.receipt.causation_event_id
    );

    let replayed = fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &command)
        .unwrap();
    assert!(matches!(replayed, ProviderRecoveryOutcome::Replayed(_)));
    let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    for (table, expected) in [
        ("execass_recovery_evaluations", 1_i64),
        ("execass_lifecycle_transitions", 2_i64),
        ("execass_receipts", 3_i64),
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            expected,
            "unexpected {table} cardinality after exact replay"
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.recovery.updated'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
}

#[test]
fn signed_absence_mints_one_delayed_exact_retry_authorization() {
    let (fixture, claim) = claimed("objective-absence");
    let first = begin(&fixture, &claim);
    let command = signed_execution_command_for_attempt(
        &fixture,
        &claim,
        &first,
        RecorderObservationKindV1::Absent,
        "objective-absence",
        1,
        CLAIM_NOW + 3,
    );
    assert!(matches!(
        fixture
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &fixture.integrity,
                &fixture.redactor,
                &command,
            )
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));

    let delayed_command = atomic_recovery_command(
        &fixture,
        "effect-1",
        CLAIM_NOW + 3,
        "objective-absence-delay",
    );
    let ProviderRecoveryOutcome::Applied(delayed_bundle) = fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &delayed_command)
        .unwrap()
    else {
        panic!("expected applied delayed recovery")
    };
    let delayed = delayed_bundle.evaluation;
    assert_eq!(
        delayed.directive,
        RecoveryDirective::WaitUntil {
            not_before_ms: CLAIM_NOW + 1_001,
            reason: RecoveryDelayReason::Backoff,
        }
    );
    assert!(delayed.retry_authorization.is_none());
    assert!(fixture
        .fixture
        .store
        .list_due_provider_recovery_effects(CLAIM_NOW + 1_000, 8)
        .unwrap()
        .is_empty());
    assert_eq!(
        fixture
            .fixture
            .store
            .list_due_provider_recovery_effects(CLAIM_NOW + 1_001, 8)
            .unwrap(),
        vec!["effect-1".to_owned()]
    );

    let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_recovery_episodes",
            [],
            |row| { row.get::<_, i64>(0) }
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_recovery_evaluations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert!(conn
        .execute("UPDATE execass_recovery_episodes SET policy_json='{}'", [],)
        .is_err());
    assert!(conn
        .execute("DELETE FROM execass_recovery_evaluations", [])
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_logical_effects SET operation_reversible=0 WHERE logical_effect_id='effect-1'",
            [],
        )
        .is_err());
    drop(conn);
    let reopened = ExecAssStore::open(&fixture.fixture.paths).unwrap();
    let ProviderRecoveryOutcome::Replayed(replayed_bundle) = reopened
        .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &delayed_command)
        .unwrap()
    else {
        panic!("expected exact recovery replay after reopen")
    };
    assert_eq!(replayed_bundle.evaluation, delayed);
    let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_recovery_evaluations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "exact restart replay must not append duplicate recovery evidence"
    );
    drop(conn);

    let authorized_command = atomic_recovery_command(
        &fixture,
        "effect-1",
        CLAIM_NOW + 1_250,
        "objective-absence-due",
    );
    let ProviderRecoveryOutcome::Applied(authorized_bundle) = reopened
        .apply_provider_recovery_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &authorized_command,
        )
        .unwrap()
    else {
        panic!("expected applied due recovery")
    };
    let authorized = authorized_bundle.evaluation;
    assert_eq!(
        authorized.directive,
        RecoveryDirective::RetrySameEffect {
            not_before_ms: CLAIM_NOW + 1_001,
        }
    );
    let stale_token = authorized.retry_authorization.unwrap();
    assert!(reopened
        .list_due_provider_recovery_effects(CLAIM_NOW + 1_251, 8)
        .unwrap()
        .is_empty());
    let latest_command = atomic_recovery_command(
        &fixture,
        "effect-1",
        CLAIM_NOW + 1_251,
        "objective-absence-latest",
    );
    let ProviderRecoveryOutcome::Applied(latest_bundle) = reopened
        .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &latest_command)
        .unwrap()
    else {
        panic!("expected latest recovery decision")
    };
    let latest_token = latest_bundle
        .evaluation
        .retry_authorization
        .expect("latest retry authorization");
    assert!(matches!(
        fixture
            .fixture
            .store
            .prepare_provider_attempt(&PrepareProviderAttemptCommand {
                claim: claim.clone(),
                trusted_now: CLAIM_NOW + 1_251,
                retry_authorization: Some(stale_token),
            })
            .unwrap(),
        PrepareProviderAttemptOutcome::Conflict
    ));
    let PrepareProviderAttemptOutcome::Prepared(second) = fixture
        .fixture
        .store
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: claim.clone(),
            trusted_now: CLAIM_NOW + 1_251,
            retry_authorization: Some(latest_token.clone()),
        })
        .unwrap()
    else {
        panic!("objective retry authorization did not prepare attempt two")
    };
    assert_eq!(second.attempt_number, 2);
    assert_ne!(second.attempt_id, first.attempt_id);

    assert!(matches!(
        fixture
            .fixture
            .store
            .prepare_provider_attempt(&PrepareProviderAttemptCommand {
                claim,
                trusted_now: CLAIM_NOW + 1_252,
                retry_authorization: Some(latest_token),
            })
            .unwrap(),
        PrepareProviderAttemptOutcome::Replayed(_) | PrepareProviderAttemptOutcome::Conflict
    ));
}

#[test]
fn recovery_storage_derivation_has_no_action_category_or_semantic_policy_input() {
    let source = include_str!("recovery.rs");
    for forbidden in [
        "action_kind",
        "purpose",
        "commerce",
        "morality",
        "wording",
        "model_risk",
    ] {
        assert!(
            !source.contains(forbidden),
            "objective recovery storage must not derive policy from {forbidden}"
        );
    }
}

#[test]
fn outcome_unknown_never_mints_retry_and_open_breaker_defers_safe_retry() {
    let (unknown_fixture, unknown_claim) = claimed("objective-unknown");
    let unknown_attempt = begin(&unknown_fixture, &unknown_claim);
    let command = signed_execution_command_for_attempt(
        &unknown_fixture,
        &unknown_claim,
        &unknown_attempt,
        RecorderObservationKindV1::Unknown,
        "objective-unknown",
        1,
        CLAIM_NOW + 3,
    );
    unknown_fixture
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &unknown_fixture.integrity,
            &unknown_fixture.redactor,
            &command,
        )
        .unwrap();
    let recovery_command = atomic_recovery_command(
        &unknown_fixture,
        "effect-1",
        CLAIM_NOW + 4,
        "objective-unknown-atomic",
    );
    let ProviderRecoveryOutcome::Applied(bundle) = unknown_fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(
            &unknown_fixture.integrity,
            &unknown_fixture.redactor,
            &recovery_command,
        )
        .unwrap()
    else {
        panic!("expected atomic unknown-outcome recovery")
    };
    assert_eq!(
        bundle.evaluation.directive,
        RecoveryDirective::WaitingExternal
    );
    assert!(bundle.evaluation.retry_authorization.is_none());
    assert_eq!(bundle.selected_phase, DelegationPhase::WaitingExternal);
    let conn = open_sqlite_connection(&unknown_fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_external_waits WHERE status='waiting'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    drop(conn);
    assert!(unknown_fixture
        .fixture
        .store
        .list_due_provider_recovery_effects(CLAIM_NOW + 5_000, 8)
        .unwrap()
        .is_empty());

    let (safe_fixture, safe_claim) = claimed("objective-breaker");
    let safe_attempt = begin(&safe_fixture, &safe_claim);
    let absent = signed_execution_command_for_attempt(
        &safe_fixture,
        &safe_claim,
        &safe_attempt,
        RecorderObservationKindV1::Absent,
        "objective-breaker",
        1,
        CLAIM_NOW + 3,
    );
    safe_fixture
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &safe_fixture.integrity,
            &safe_fixture.redactor,
            &absent,
        )
        .unwrap();
    open_sqlite_connection(&safe_fixture.fixture.paths.db_path)
        .unwrap()
        .execute(
            r#"INSERT INTO circuit_breaker_states(
                 breaker_key,scope,target_id,state,consecutive_failures,opened_at,
                 cooldown_until,last_error_code,updated_at
               ) VALUES('ea214-breaker','provider','recorder-provider','open',3,?1,?2,'transient',?1)"#,
            rusqlite::params![CLAIM_NOW + 3, CLAIM_NOW + 2_000],
        )
        .unwrap();
    let evaluation = safe_fixture
        .fixture
        .store
        .evaluate_provider_recovery("effect-1", CLAIM_NOW + 1_001)
        .unwrap();
    assert_eq!(
        evaluation.directive,
        RecoveryDirective::WaitUntil {
            not_before_ms: CLAIM_NOW + 2_000,
            reason: RecoveryDelayReason::CircuitBreaker,
        }
    );
    assert!(evaluation.retry_authorization.is_none());
}

#[test]
fn recovery_episode_preserves_real_accepted_danger_grant_byte_identically() {
    let (atomic, _) = duplicate_risk_fixture_with_accepted_dangerous_grant("ea214-carry-forward");
    let effect_id = "grant-preservation-predecessor-effect-ea214-carry-forward";
    let grant_snapshot = |conn: &rusqlite::Connection| {
        conn.query_row(
            r#"SELECT json_array(
                 grant_id,delegation_id,decision_id,confirmed_logical_action_identity,
                 canonical_action_envelope_or_selector_json,payload_and_material_operands_json,
                 payload_and_material_operands_digest,connector_tool_identity,
                 connector_tool_version,declared_consequence,
                 accepted_by_authority_provenance_id,confirmation_attestation_digest,
                 accepted_at,invalidated_at,invalidation_reason,
                 invalidated_by_authority_provenance_id
               ) FROM execass_accepted_confirmation_grants WHERE grant_id='grant-attested'"#,
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    };
    let conn = open_sqlite_connection(&atomic.fixture.paths.db_path).unwrap();
    let before = grant_snapshot(&conn);
    drop(conn);

    let evaluation = atomic
        .fixture
        .store
        .evaluate_provider_recovery(effect_id, 1_800_000_000_030)
        .unwrap();
    assert_eq!(evaluation.directive, RecoveryDirective::WaitingExternal);
    assert!(evaluation.retry_authorization.is_none());

    let conn = open_sqlite_connection(&atomic.fixture.paths.db_path).unwrap();
    assert_eq!(grant_snapshot(&conn), before);
    assert_eq!(
        conn.query_row(
            "SELECT accepted_confirmation_grant_id FROM execass_recovery_episodes WHERE logical_effect_id=?1",
            [effect_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "grant-attested"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_confirmation_challenges WHERE delegation_id='delegation-1'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "recovery must not create a replacement confirmation challenge"
    );
}

#[test]
fn exhausted_recovery_promotes_only_the_precompiled_authorized_replan_candidate() {
    let (fixture, claim) = claimed("objective-exact-replan");
    let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    let (state_revision, plan_revision, stop_epoch, action_revision): (i64, i64, i64, i64) = conn
        .query_row(
            r#"SELECT state_revision,current_plan_revision,stop_epoch,
                      (SELECT COALESCE(MAX(action_revision),0)+1
                       FROM execass_action_branches WHERE delegation_id=d.delegation_id)
               FROM execass_delegations d WHERE delegation_id='delegation-1'"#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    conn.execute(
        r#"INSERT INTO execass_action_branches(
             action_id,delegation_id,action_revision,target_delegation_revision,
             target_plan_revision,stop_epoch,branch_kind,status,action_summary,
             created_at,updated_at,terminal_at
           ) VALUES('precompiled-recovery','delegation-1',?1,?2,?3,?4,
             'recovery','waiting','precompiled bounded alternative',?5,?5,NULL)"#,
        rusqlite::params![
            action_revision,
            state_revision,
            plan_revision,
            stop_epoch,
            CLAIM_NOW,
        ],
    )
    .unwrap();
    drop(conn);

    let first = begin(&fixture, &claim);
    let absence = signed_execution_command_for_attempt(
        &fixture,
        &claim,
        &first,
        RecorderObservationKindV1::Absent,
        "objective-exact-replan",
        1,
        CLAIM_NOW + 3,
    );
    fixture
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(&fixture.integrity, &fixture.redactor, &absence)
        .unwrap();
    let command = atomic_recovery_command(
        &fixture,
        "effect-1",
        CLAIM_NOW + 400_000,
        "objective-exact-replan",
    );
    let ProviderRecoveryOutcome::Applied(bundle) = fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &command)
        .unwrap()
    else {
        panic!("expected exact replan projection")
    };
    assert_eq!(
        bundle.evaluation.directive,
        RecoveryDirective::ReplanWithinOriginalAuthority
    );
    assert_eq!(bundle.selected_phase, DelegationPhase::Recovering);
    let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_action_branches WHERE action_id='precompiled-recovery'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "runnable"
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_action_branches WHERE action_id='action-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_action_branches WHERE branch_kind='recovery'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "replan must not invent an additional recovery action"
    );
}

#[test]
fn exhausted_recovery_projects_honest_partial_and_failed_assessments() {
    for (suffix, material_pass, expected_directive, expected_phase) in [
        (
            "objective-partial",
            true,
            RecoveryDirective::PartiallyCompleted,
            DelegationPhase::PartiallyCompleted,
        ),
        (
            "objective-failed",
            false,
            RecoveryDirective::Failed,
            DelegationPhase::Failed,
        ),
    ] {
        let (fixture, claim) = claimed(suffix);
        let first = begin(&fixture, &claim);
        let absence = signed_execution_command_for_attempt(
            &fixture,
            &claim,
            &first,
            RecorderObservationKindV1::Absent,
            suffix,
            1,
            CLAIM_NOW + 3,
        );
        fixture
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(&fixture.integrity, &fixture.redactor, &absence)
            .unwrap();
        if material_pass {
            open_sqlite_connection(&fixture.fixture.paths.db_path)
                .unwrap()
                .execute(
                    r#"INSERT INTO execass_verifier_results(
                         verifier_result_id,delegation_id,criterion_id,result_revision,result,
                         evidence_refs_json,evidence_digest,verifier_identity,verified_at
                       ) VALUES('recovery-pass','delegation-1','criterion-z',1,'pass','[]',
                         'sha256:recovery-pass','authoritative-test-verifier',?1)"#,
                    [CLAIM_NOW + 4],
                )
                .unwrap();
        }
        let command = atomic_recovery_command(&fixture, "effect-1", CLAIM_NOW + 400_000, suffix);
        let ProviderRecoveryOutcome::Applied(bundle) = fixture
            .fixture
            .store
            .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &command)
            .unwrap()
        else {
            panic!("expected terminal recovery projection")
        };
        assert_eq!(bundle.evaluation.directive, expected_directive);
        assert_eq!(bundle.selected_phase, expected_phase);
        let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
        let (phase, useful, no_path, pass_count): (String, i64, i64, i64) = conn
            .query_row(
                r#"SELECT terminal_phase,useful_outcome,no_remaining_path,material_pass_count
                   FROM execass_completion_assessments"#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(phase, expected_phase.as_str());
        assert_eq!(useful, material_pass as i64);
        assert_eq!(no_path, 1);
        assert_eq!(pass_count, material_pass as i64);
        assert_eq!(
            conn.query_row(
                "SELECT status FROM execass_action_branches WHERE action_id='action-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "terminal"
        );
    }
}

#[test]
fn canonical_quota_exhaustion_blocks_retry_and_actionable_choice_waits_once_for_user() {
    let (quota_fixture, quota_claim) = claimed_with_quota("objective-quota-exhausted", 1, 1);
    let quota_attempt = begin(&quota_fixture, &quota_claim);
    let quota_absence = signed_execution_command_for_attempt(
        &quota_fixture,
        &quota_claim,
        &quota_attempt,
        RecorderObservationKindV1::Absent,
        "objective-quota-exhausted",
        1,
        CLAIM_NOW + 3,
    );
    quota_fixture
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &quota_fixture.integrity,
            &quota_fixture.redactor,
            &quota_absence,
        )
        .unwrap();
    let quota_command = atomic_recovery_command(
        &quota_fixture,
        "effect-1",
        CLAIM_NOW + 1_001,
        "objective-quota-exhausted",
    );
    let ProviderRecoveryOutcome::Applied(quota_bundle) = quota_fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(
            &quota_fixture.integrity,
            &quota_fixture.redactor,
            &quota_command,
        )
        .unwrap()
    else {
        panic!("expected quota-exhausted recovery")
    };
    assert_eq!(quota_bundle.evaluation.directive, RecoveryDirective::Failed);
    assert!(quota_bundle.evaluation.retry_authorization.is_none());

    let (choice_fixture, choice_claim) = claimed("objective-user-choice");
    let choice_attempt = begin(&choice_fixture, &choice_claim);
    let choice_absence = signed_execution_command_for_attempt(
        &choice_fixture,
        &choice_claim,
        &choice_attempt,
        RecorderObservationKindV1::Absent,
        "objective-user-choice",
        1,
        CLAIM_NOW + 3,
    );
    choice_fixture
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &choice_fixture.integrity,
            &choice_fixture.redactor,
            &choice_absence,
        )
        .unwrap();
    open_sqlite_connection(&choice_fixture.fixture.paths.db_path)
        .unwrap()
        .execute(
            r#"INSERT INTO execass_attention_items(
                 attention_id,delegation_id,action_id,kind,status,reason,recommendation,
                 alternatives_json,required_assurance,decision_id,delegation_revision,
                 created_at,resolved_at
               ) VALUES('precompiled-recovery-choice','delegation-1','action-1',
                 'recovery_choice','actionable','bounded recovery choices exist',
                 'choose one bounded path','[]','human_local_or_remote',NULL,1,?1,NULL)"#,
            [CLAIM_NOW],
        )
        .unwrap();
    let choice_command = atomic_recovery_command(
        &choice_fixture,
        "effect-1",
        CLAIM_NOW + 400_000,
        "objective-user-choice",
    );
    let ProviderRecoveryOutcome::Applied(choice_bundle) = choice_fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(
            &choice_fixture.integrity,
            &choice_fixture.redactor,
            &choice_command,
        )
        .unwrap()
    else {
        panic!("expected user-choice recovery")
    };
    assert_eq!(
        choice_bundle.evaluation.directive,
        RecoveryDirective::WaitingForUser
    );
    assert_eq!(
        choice_bundle.selected_phase,
        DelegationPhase::WaitingForUser
    );
    let conn = open_sqlite_connection(&choice_fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_attention_items WHERE kind='recovery_choice' AND status='actionable'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "recovery must reuse the one actionable bounded choice instead of nagging again"
    );
}

#[test]
fn sealed_provider_failure_classes_control_only_same_effect_retry() {
    for (suffix, class, retry_expected) in [
        ("class-transient", ProviderFailureClassV1::Transient, true),
        (
            "class-rate-limited",
            ProviderFailureClassV1::RateLimited,
            true,
        ),
        ("class-unknown", ProviderFailureClassV1::Unknown, true),
        (
            "class-authentication",
            ProviderFailureClassV1::Authentication,
            false,
        ),
        ("class-permanent", ProviderFailureClassV1::Permanent, false),
    ] {
        let (fixture, claim) = claimed(suffix);
        let attempt = begin(&fixture, &claim);
        let evidence = signed_execution_command_for_attempt_with_class(
            &fixture,
            &claim,
            &attempt,
            RecorderObservationKindV1::Absent,
            Some(class),
            (suffix, 1, CLAIM_NOW + 3),
        );
        fixture
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &fixture.integrity,
                &fixture.redactor,
                &evidence,
            )
            .unwrap();
        let command = atomic_recovery_command(&fixture, "effect-1", CLAIM_NOW + 1_001, suffix);
        let ProviderRecoveryOutcome::Applied(bundle) = fixture
            .fixture
            .store
            .apply_provider_recovery_atomically(&fixture.integrity, &fixture.redactor, &command)
            .unwrap()
        else {
            panic!("expected class-bound recovery")
        };
        if retry_expected {
            assert_eq!(
                bundle.evaluation.directive,
                RecoveryDirective::RetrySameEffect {
                    not_before_ms: CLAIM_NOW + 1_001,
                }
            );
            assert!(bundle.evaluation.retry_authorization.is_some());
            assert_eq!(bundle.selected_phase, DelegationPhase::Recovering);
        } else {
            assert_eq!(bundle.evaluation.directive, RecoveryDirective::Failed);
            assert!(bundle.evaluation.retry_authorization.is_none());
            assert_eq!(bundle.selected_phase, DelegationPhase::Failed);
        }
        let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
        let expected_class = match class {
            ProviderFailureClassV1::Transient => "transient",
            ProviderFailureClassV1::RateLimited => "rate_limited",
            ProviderFailureClassV1::Authentication => "authentication",
            ProviderFailureClassV1::Permanent => "permanent",
            ProviderFailureClassV1::Unknown => "unknown",
        };
        assert_eq!(
            conn.query_row(
                "SELECT provider_error_class FROM execass_provider_attempts WHERE attempt_id=?1",
                [&attempt.attempt_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            expected_class
        );
    }
}

#[test]
fn stale_heads_and_outbox_collision_leave_recovery_state_unwritten() {
    let prepare_absent = |suffix: &str| {
        let (fixture, claim) = claimed(suffix);
        let attempt = begin(&fixture, &claim);
        let evidence = signed_execution_command_for_attempt(
            &fixture,
            &claim,
            &attempt,
            RecorderObservationKindV1::Absent,
            suffix,
            1,
            CLAIM_NOW + 3,
        );
        fixture
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &fixture.integrity,
                &fixture.redactor,
                &evidence,
            )
            .unwrap();
        fixture
    };

    let stale_fixture = prepare_absent("recovery-stale-heads");
    let mut stale_command = atomic_recovery_command(
        &stale_fixture,
        "effect-1",
        CLAIM_NOW + 3,
        "recovery-stale-heads",
    );
    stale_command.receipt.expected_global_count += 1;
    assert!(matches!(
        stale_fixture
            .fixture
            .store
            .apply_provider_recovery_atomically(
                &stale_fixture.integrity,
                &stale_fixture.redactor,
                &stale_command,
            )
            .unwrap(),
        ProviderRecoveryOutcome::Stale { .. }
    ));
    let stale_conn = open_sqlite_connection(&stale_fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        stale_conn
            .query_row(
                "SELECT COUNT(*) FROM execass_recovery_evaluations",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    drop(stale_conn);

    let collision_fixture = prepare_absent("recovery-outbox-collision");
    let collision_command = atomic_recovery_command(
        &collision_fixture,
        "effect-1",
        CLAIM_NOW + 3,
        "recovery-outbox-collision",
    );
    let conn = open_sqlite_connection(&collision_fixture.fixture.paths.db_path).unwrap();
    conn.execute(
        r#"INSERT INTO execass_outbox_events(
             event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
             causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
           ) VALUES(?1,'execass.v1.recovery.updated','delegation-1',?2,
             'collision-correlation','collision-causation',?3,'v1','{}','forced-recovery-collision')"#,
        rusqlite::params![
            collision_command.receipt.causation_event_id,
            collision_command.expected_pre_state_revision + 1,
            collision_command.trusted_now,
        ],
    )
    .unwrap();
    let pre_revision: i64 = conn
        .query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    assert!(collision_fixture
        .fixture
        .store
        .apply_provider_recovery_atomically(
            &collision_fixture.integrity,
            &collision_fixture.redactor,
            &collision_command,
        )
        .is_err());
    let conn = open_sqlite_connection(&collision_fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        pre_revision
    );
    for table in [
        "execass_recovery_episodes",
        "execass_recovery_evaluations",
        "execass_completion_assessments",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "collision leaked {table}"
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_action_branches WHERE branch_kind='recovery'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
}
