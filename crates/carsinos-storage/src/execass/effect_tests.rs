use super::recorder_tests::signed_execution_command_for_attempt;
use super::resource_tests::{acquire, claim_command, setup, setup_recorder, ResourceFixture};
use super::tests::fixture;
use super::*;
use crate::open_sqlite_connection;
use carsinos_protocol::execass_recorder::{
    ExecuteOnceV1, OpaqueOperandEnvelopeV1, RecorderBindingV1, RecorderObservationKindV1,
    RECORDER_PROTOCOL_VERSION,
};

#[test]
fn exact_overwrite_provider_request_digest_matches_protocol_specialization() {
    let dispatch = LogicalEffectDispatchIdentity {
        logical_effect_id: "effect-exact".into(),
        delegation_id: "delegation-exact".into(),
        continuation_id: "continuation-exact".into(),
        action_id: "action-exact".into(),
        claim_event_id: "claim-event-exact".into(),
        claim_receipt_id: "claim-receipt-exact".into(),
        continuation_fencing_token: 1,
        runtime_host_generation: 1,
        runtime_host_instance_id: "host-exact".into(),
        runtime_fencing_token: 1,
        internal_idempotency_key: "internal-exact".into(),
        provider_identity: Some("carsinos.local-fs.exact-overwrite".into()),
        provider_idempotency_key: None,
        reconciliation_key: Some(
            r#"{"contract_version":"carsinos.local-fs.exact-overwrite.reconciliation.v1"}"#.into(),
        ),
        manifest_digest: "a".repeat(64),
        payload_digest: format!("sha256:{}", "b".repeat(64)),
    };
    let command = ExecuteOnceV1 {
        binding: RecorderBindingV1 {
            protocol_version: RECORDER_PROTOCOL_VERSION.into(),
            canonical_root_identity: "root".into(),
            installation_id: "installation".into(),
            state_root_generation: 1,
            os_user_identity_digest: "c".repeat(64),
            runtime_host_generation: 1,
            runtime_host_instance_id: "host-exact".into(),
            runtime_fencing_token: 1,
        },
        request_id: "request".into(),
        claim_event_id: dispatch.claim_event_id.clone(),
        claim_receipt_id: dispatch.claim_receipt_id.clone(),
        continuation_fencing_token: 1,
        delegation_id: dispatch.delegation_id.clone(),
        continuation_id: dispatch.continuation_id.clone(),
        action_id: dispatch.action_id.clone(),
        logical_effect_id: dispatch.logical_effect_id.clone(),
        internal_idempotency_key: dispatch.internal_idempotency_key.clone(),
        attempt_id: "attempt".into(),
        attempt_number: 1,
        provider_identity: dispatch.provider_identity.clone().unwrap(),
        provider_version: "v1".into(),
        adapter_identity: "carsinos.effect-recorder.exact-overwrite.v1".into(),
        adapter_artifact_digest: format!("sha256:{}", "d".repeat(64)),
        provider_request_digest: String::new(),
        provider_idempotency_key: None,
        reconciliation_key: dispatch.reconciliation_key.clone(),
        manifest_digest: dispatch.manifest_digest.clone(),
        payload_digest: dispatch.payload_digest.clone(),
        operand_envelope: OpaqueOperandEnvelopeV1 {
            non_secret: serde_json::json!({}),
            secret_handles: Vec::new(),
        },
        deadline_ms: 1,
        client_nonce: "nonce".into(),
        command_mac: "mac".into(),
    };
    assert_eq!(
        super::effect::provider_request_digest(&dispatch),
        command.derived_provider_request_digest().unwrap()
    );
}
use std::sync::{Arc, Barrier};
use std::thread;

fn claimed_fixture(suffix: &str) -> (ResourceFixture, ContinuationClaimIdentity) {
    let f = setup(1, 100, 1);
    let job = acquire(&f, "worker-a", 1).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        suffix,
    );
    let ContinuationClaimOutcome::Claimed(record) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("expected canonical claim")
    };
    (f, record.identity)
}

fn claimed_recorder_fixture(suffix: &str) -> (ResourceFixture, ContinuationClaimIdentity) {
    let f = setup_recorder(1, 100, 1);
    let job = acquire(&f, "worker-a", 1).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        suffix,
    );
    let ContinuationClaimOutcome::Claimed(record) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("expected canonical recorder claim")
    };
    (f, record.identity)
}

fn prepare(
    f: &ResourceFixture,
    claim: &ContinuationClaimIdentity,
    retry: bool,
) -> ProviderAttemptRecord {
    let now = claim.job_lease_expires_at - 1;
    let retry_authorization = retry.then(|| test_retry_authorization(f, claim, now));
    match f
        .fixture
        .store
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: claim.clone(),
            trusted_now: now,
            retry_authorization,
        })
        .unwrap()
    {
        PrepareProviderAttemptOutcome::Prepared(record)
        | PrepareProviderAttemptOutcome::Replayed(record) => *record,
        other => panic!("expected prepared or replayed attempt, got {other:?}"),
    }
}

fn test_retry_authorization(
    f: &ResourceFixture,
    claim: &ContinuationClaimIdentity,
    not_before_ms: i64,
) -> ProviderRetryAuthorization {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (
        logical_effect_id,
        attempt_id,
        attempt_number,
        delegation_id,
        action_id,
        manifest_digest,
        state_revision,
    ): (String, String, i64, String, String, String, i64) = conn
        .query_row(
            r#"SELECT e.logical_effect_id,a.attempt_id,a.attempt_number,
                      e.delegation_id,c.action_id,e.manifest_digest,d.state_revision
               FROM execass_logical_effects e
               JOIN execass_provider_attempts a ON a.logical_effect_id=e.logical_effect_id
               JOIN execass_continuations c ON c.continuation_id=e.continuation_id
               JOIN execass_delegations d ON d.delegation_id=e.delegation_id
               WHERE e.continuation_id=?1
               ORDER BY a.attempt_number DESC LIMIT 1"#,
            [&claim.continuation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    let recovery_episode_id = format!("recovery-episode-test-{logical_effect_id}");
    let recovery_evaluation_id = format!("recovery-evaluation-test-{}", attempt_number + 1);
    let digest = format!("sha256:{}", "a".repeat(64));
    let outbox_event_id = format!("recovery-outbox-test-{}", attempt_number + 1);
    let recovery_state_revision = state_revision + 1;
    conn.execute(
        "UPDATE execass_delegations SET phase='recovering',state_revision=?1 WHERE delegation_id=?2 AND state_revision=?3",
        rusqlite::params![recovery_state_revision, delegation_id, state_revision],
    )
    .unwrap();
    conn.execute(
        r#"INSERT OR IGNORE INTO execass_recovery_episodes(
             recovery_episode_id,delegation_id,logical_effect_id,initial_attempt_id,
             action_id,manifest_digest,normalized_intent_digest,effective_authority_digest,
             accepted_confirmation_grant_id,policy_json,policy_digest,opened_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,NULL,'{}',?7,?8)"#,
        rusqlite::params![
            recovery_episode_id,
            delegation_id,
            logical_effect_id,
            attempt_id,
            action_id,
            manifest_digest,
            digest,
            not_before_ms,
        ],
    )
    .unwrap();
    conn.execute(
        r#"INSERT OR IGNORE INTO execass_outbox_events(
             event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
             causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
           ) VALUES(?1,'execass.v1.recovery.updated',?2,?3,?4,?5,?6,'v1','{}',?1)"#,
        rusqlite::params![
            outbox_event_id,
            delegation_id,
            recovery_state_revision,
            recovery_episode_id,
            recovery_evaluation_id,
            not_before_ms,
        ],
    )
    .unwrap();
    conn.execute(
        r#"INSERT OR IGNORE INTO execass_recovery_evaluations(
             recovery_evaluation_id,recovery_episode_id,delegation_id,logical_effect_id,
             predecessor_attempt_id,evaluation_revision,recovery_state_revision,objective_facts_json,
             objective_facts_digest,directive,directive_json,directive_digest,
             not_before_ms,outbox_event_id,evaluated_at
           ) VALUES(?1,?2,?3,?4,?5,1,?6,'{}',?7,'retry_same_effect',?8,?7,?9,?10,?9)"#,
        rusqlite::params![
            recovery_evaluation_id,
            recovery_episode_id,
            delegation_id,
            logical_effect_id,
            attempt_id,
            recovery_state_revision,
            digest,
            format!("{{\"directive\":\"retry_same_effect\",\"not_before_ms\":{not_before_ms}}}"),
            not_before_ms,
            outbox_event_id,
        ],
    )
    .unwrap();
    ProviderRetryAuthorization {
        recovery_evaluation_id,
        logical_effect_id,
        predecessor_attempt_id: attempt_id,
        authorized_attempt_number: attempt_number + 1,
        not_before_ms,
        objective_facts_digest: format!("sha256:{}", "a".repeat(64)),
        recovery_state_revision,
    }
}

fn begin(
    f: &ResourceFixture,
    claim: &ContinuationClaimIdentity,
    attempt: &ProviderAttemptRecord,
) -> ProviderAttemptRecord {
    match f
        .fixture
        .store
        .begin_provider_attempt_invocation(&BeginProviderAttemptInvocationCommand {
            attempt_id: attempt.attempt_id.clone(),
            claim: claim.clone(),
            trusted_now: claim.job_lease_expires_at - 1,
        })
        .unwrap()
    {
        BeginProviderAttemptInvocationOutcome::Began(record)
        | BeginProviderAttemptInvocationOutcome::AlreadyInvoking(record) => *record,
        other => panic!("expected begun or replayed attempt, got {other:?}"),
    }
}

fn result(
    attempt: &ProviderAttemptRecord,
    claim: &ContinuationClaimIdentity,
    status: ProviderAttemptStatus,
    response: &str,
) -> RecordProviderAttemptResultCommand {
    RecordProviderAttemptResultCommand {
        attempt_id: attempt.attempt_id.clone(),
        claim: claim.clone(),
        trusted_now: claim.job_lease_expires_at - 1,
        status,
        provider_response_digest: response.into(),
        remote_effect_id: None,
        finished_at: claim.job_lease_expires_at - 2,
    }
}

fn impossible_claim() -> ContinuationClaimIdentity {
    ContinuationClaimIdentity {
        claim_event_id: "claim-event".into(),
        claim_receipt_id: "claim-receipt".into(),
        continuation_id: "continuation".into(),
        delegation_id: "delegation".into(),
        action_id: "action".into(),
        job_id: "job".into(),
        worker_id: "worker".into(),
        job_lease_expires_at: 2,
        continuation_fencing_token: 1,
        runtime_host_generation: 1,
        runtime_host_instance_id: "host".into(),
        runtime_fencing_token: 1,
        state_root_generation: 1,
        runtime_authority_provenance_id: "authority".into(),
        runtime_actor_identity: "runtime".into(),
        policy_revision: 1,
        global_stop_epoch: 0,
        technical_quota_policy_digest: "digest".into(),
        technical_quota_snapshot_id: None,
        technical_resource_reservation_set_digest: "digest".into(),
    }
}

#[test]
fn provider_attempt_schema_retains_fenced_ancestry_and_guards() {
    let fixture = fixture();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let columns = conn
        .prepare("PRAGMA table_info(execass_provider_attempts)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for required in [
        "continuation_id",
        "action_id",
        "claim_event_id",
        "claim_receipt_id",
        "host_instance_id",
        "runtime_fencing_token",
        "provider_error_class",
    ] {
        assert!(
            columns.contains(&required.to_string()),
            "missing {required}"
        );
    }
    let triggers = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'execass_provider_attempt_%'")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for required in [
        "execass_provider_attempt_no_delete",
        "execass_provider_attempt_transition_guard",
        "execass_provider_attempt_terminal_fields_immutable",
        "execass_provider_attempt_claim_binding_guard",
    ] {
        assert!(
            triggers.contains(&required.to_string()),
            "missing {required}"
        );
    }
}

#[test]
fn logical_effect_cannot_be_deleted_before_attempt_and_prepared_cannot_fake_result() {
    let f = setup(1, 100, 1);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let delete_error = conn
        .execute(
            "DELETE FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
        )
        .expect_err("logical effect history must survive before its first attempt");
    assert!(delete_error
        .to_string()
        .contains("ExecAss logical effects cannot be deleted"));

    let job = acquire(&f, "worker-a", 1).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "prepared-transition-guard",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("expected canonical claim")
    };
    let prepared = prepare(&f, &claimed.identity, false);
    for status in ["failed", "outcome_unknown"] {
        let error = conn
            .execute(
                "UPDATE execass_provider_attempts SET status=?1,provider_response_digest='forged',finished_at=2 WHERE attempt_id=?2",
                rusqlite::params![status, prepared.attempt_id],
            )
            .expect_err("prepared attempt cannot bypass begin invocation");
        assert!(error
            .to_string()
            .contains("ExecAss provider attempt transition is invalid"));
    }
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_provider_attempts WHERE attempt_id=?1",
            [&prepared.attempt_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "prepared"
    );
}

#[test]
fn attempt_prepare_rejects_invalid_clock_before_any_row_can_exist() {
    let fixture = fixture();
    let outcome = fixture
        .store
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: impossible_claim(),
            trusted_now: 0,
            retry_authorization: None,
        });
    assert!(outcome.is_err());
}

#[test]
fn attempt_result_rejects_non_terminal_status_before_any_mutation() {
    let fixture = fixture();
    let outcome =
        fixture
            .store
            .record_provider_attempt_result(&RecordProviderAttemptResultCommand {
                attempt_id: "attempt".into(),
                claim: impossible_claim(),
                trusted_now: 1,
                status: ProviderAttemptStatus::Invoking,
                provider_response_digest: "response".into(),
                remote_effect_id: None,
                finished_at: 1,
            });
    assert!(outcome.is_err());
}

#[test]
fn canonical_prepare_replays_and_failed_retry_preserves_dispatch_identity() {
    let (f, claim) = claimed_recorder_fixture("effect-retry");
    let first = prepare(&f, &claim, false);
    let replay = prepare(&f, &claim, false);
    assert_eq!(
        first, replay,
        "same operation must return byte-identical attempt"
    );
    assert_eq!(
        first.dispatch.provider_identity.as_deref(),
        Some("recorder-provider")
    );
    assert_eq!(
        first.dispatch.provider_idempotency_key.as_deref(),
        Some("provider-idempotency-1")
    );
    assert_eq!(
        first.dispatch.reconciliation_key.as_deref(),
        Some("provider-reconciliation-1")
    );
    assert!(!first.dispatch.internal_idempotency_key.is_empty());

    let first = begin(&f, &claim, &first);
    assert!(matches!(
        f.fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &f.integrity,
                &f.redactor,
                &signed_execution_command_for_attempt(
                    &f,
                    &claim,
                    &first,
                    RecorderObservationKindV1::Absent,
                    "effect-retry-failed",
                    1,
                    first.started_at,
                ),
            )
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));
    let second = prepare(&f, &claim, true);
    assert_eq!(second.attempt_number, 2);
    assert_ne!(second.attempt_id, first.attempt_id);
    assert_eq!(second.dispatch, first.dispatch);
    assert_eq!(
        second.provider_request_digest,
        first.provider_request_digest
    );
}

#[test]
fn outcome_unknown_blocks_retry_and_terminal_replay_survives_stale_fence() {
    let (f, claim) = claimed_recorder_fixture("effect-unknown");
    let first = begin(&f, &claim, &prepare(&f, &claim, false));
    let unknown = signed_execution_command_for_attempt(
        &f,
        &claim,
        &first,
        RecorderObservationKindV1::Unknown,
        "effect-unknown",
        1,
        first.started_at,
    );
    assert!(matches!(
        f.fixture
            .store
            .reconcile_recorder_evidence_atomically(&f.integrity, &f.redactor, &unknown)
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));
    let retry_outcome = f
        .fixture
        .store
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: claim.clone(),
            trusted_now: claim.job_lease_expires_at - 1,
            retry_authorization: Some(test_retry_authorization(
                &f,
                &claim,
                claim.job_lease_expires_at - 1,
            )),
        })
        .unwrap();
    assert!(
        matches!(
            retry_outcome,
            PrepareProviderAttemptOutcome::Stale {
                reason: ContinuationStaleReason::TechnicalReservationMissingOrChanged
            }
        ),
        "unknown outcome unexpectedly admitted retry: {retry_outcome:?}"
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute("UPDATE execass_runtime_host_leases SET released_at=1", [])
        .unwrap();
    drop(conn);
    assert!(matches!(
        f.fixture
            .store
            .reconcile_recorder_evidence_atomically(&f.integrity, &f.redactor, &unknown)
            .unwrap(),
        RecorderEvidenceImportOutcome::Replayed(_)
    ));
    let mut changed = unknown.clone();
    changed.claim_identity.worker_id.push_str("-changed");
    assert!(matches!(
        f.fixture
            .store
            .reconcile_recorder_evidence_atomically(&f.integrity, &f.redactor, &changed)
            .unwrap(),
        RecorderEvidenceImportOutcome::Conflict
    ));
}

#[test]
fn stale_result_cannot_mutate_and_attempt_history_cannot_be_deleted_or_rewritten() {
    let (f, claim) = claimed_fixture("effect-stale");
    let first = begin(&f, &claim, &prepare(&f, &claim, false));
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute("UPDATE execass_runtime_host_leases SET released_at=1", [])
        .unwrap();
    assert!(matches!(
        f.fixture
            .store
            .record_provider_attempt_result(&result(
                &first,
                &claim,
                ProviderAttemptStatus::Failed,
                "would-be-result"
            ))
            .unwrap(),
        RecordProviderAttemptResultOutcome::Stale { .. }
    ));
    assert!(conn
        .execute(
            "DELETE FROM execass_provider_attempts WHERE attempt_id=?1",
            [&first.attempt_id]
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_provider_attempts SET attempt_number=9 WHERE attempt_id=?1",
            [&first.attempt_id]
        )
        .is_err());
    let status: String = conn
        .query_row(
            "SELECT status FROM execass_provider_attempts WHERE attempt_id=?1",
            [&first.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "invoking");
}

#[test]
fn prepared_attempt_reopens_safely_and_begin_is_single_authorization() {
    let (f, claim) = claimed_fixture("effect-prepared-reopen");
    let prepared = prepare(&f, &claim, false);
    assert_eq!(prepared.status, ProviderAttemptStatus::Prepared);
    let reopened = ExecAssStore::open(&f.fixture.paths).unwrap();
    let replay = match reopened
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: claim.clone(),
            trusted_now: claim.job_lease_expires_at - 1,
            retry_authorization: None,
        })
        .unwrap()
    {
        PrepareProviderAttemptOutcome::Replayed(record) => *record,
        other => panic!("prepared attempt did not restart-replay: {other:?}"),
    };
    assert_eq!(prepared, replay);
    let begun = begin(&f, &claim, &prepared);
    assert_eq!(begun.status, ProviderAttemptStatus::Invoking);
    assert!(matches!(
        f.fixture
            .store
            .begin_provider_attempt_invocation(&BeginProviderAttemptInvocationCommand {
                attempt_id: begun.attempt_id.clone(),
                claim: claim.clone(),
                trusted_now: claim.job_lease_expires_at - 1,
            })
            .unwrap(),
        BeginProviderAttemptInvocationOutcome::AlreadyInvoking(_)
    ));
}

#[test]
fn stale_begin_leaves_prepared_attempt_non_dispatchable() {
    let (f, claim) = claimed_fixture("effect-stale-begin");
    let prepared = prepare(&f, &claim, false);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute("UPDATE execass_runtime_host_leases SET released_at=1", [])
        .unwrap();
    assert!(matches!(
        f.fixture
            .store
            .begin_provider_attempt_invocation(&BeginProviderAttemptInvocationCommand {
                attempt_id: prepared.attempt_id.clone(),
                claim: claim.clone(),
                trusted_now: claim.job_lease_expires_at - 1,
            })
            .unwrap(),
        BeginProviderAttemptInvocationOutcome::Stale { .. }
    ));
    let status: String = conn
        .query_row(
            "SELECT status FROM execass_provider_attempts WHERE attempt_id=?1",
            [&prepared.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "prepared");
}

#[test]
fn reopen_and_concurrent_retry_converge_on_only_attempt_two() {
    let (f, claim) = claimed_recorder_fixture("effect-reopen-retry");
    let first = begin(&f, &claim, &prepare(&f, &claim, false));
    assert!(matches!(
        f.fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &f.integrity,
                &f.redactor,
                &signed_execution_command_for_attempt(
                    &f,
                    &claim,
                    &first,
                    RecorderObservationKindV1::Absent,
                    "effect-reopen-retry-failed",
                    1,
                    first.started_at,
                ),
            )
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));
    let reopened = ExecAssStore::open(&f.fixture.paths).unwrap();
    let retry = PrepareProviderAttemptCommand {
        claim: claim.clone(),
        trusted_now: claim.job_lease_expires_at - 1,
        retry_authorization: Some(test_retry_authorization(
            &f,
            &claim,
            claim.job_lease_expires_at - 1,
        )),
    };
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let store = reopened.clone();
        let command = retry.clone();
        let barrier = barrier.clone();
        joins.push(thread::spawn(move || {
            barrier.wait();
            store.prepare_provider_attempt(&command).unwrap()
        }));
    }
    barrier.wait();
    let outcomes = joins
        .into_iter()
        .map(|join| join.join().unwrap())
        .collect::<Vec<_>>();
    let attempts = outcomes
        .into_iter()
        .map(|outcome| match outcome {
            PrepareProviderAttemptOutcome::Prepared(record)
            | PrepareProviderAttemptOutcome::Replayed(record) => *record,
            other => panic!("concurrent retry did not converge: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(attempts.iter().all(|attempt| attempt.attempt_number == 2));
    assert_eq!(attempts[0], attempts[1]);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM execass_provider_attempts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2, "no partial third retry row");
}

#[test]
fn changed_claim_or_persisted_dispatch_identity_cannot_create_another_attempt() {
    let (f, claim) = claimed_fixture("effect-mutation");
    let first = prepare(&f, &claim, false);
    let count_attempts = || -> i64 {
        open_sqlite_connection(&f.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM execass_provider_attempts",
                [],
                |row| row.get(0),
            )
            .unwrap()
    };
    for mutated in [
        {
            let mut value = claim.clone();
            value.claim_event_id = "other-event".into();
            value
        },
        {
            let mut value = claim.clone();
            value.claim_receipt_id = "other-receipt".into();
            value
        },
        {
            let mut value = claim.clone();
            value.continuation_fencing_token += 1;
            value
        },
        {
            let mut value = claim.clone();
            value.runtime_host_generation += 1;
            value
        },
        {
            let mut value = claim.clone();
            value.runtime_host_instance_id = "other-host".into();
            value
        },
        {
            let mut value = claim.clone();
            value.runtime_fencing_token += 1;
            value
        },
    ] {
        assert!(matches!(
            f.fixture
                .store
                .prepare_provider_attempt(&PrepareProviderAttemptCommand {
                    claim: mutated,
                    trusted_now: claim.job_lease_expires_at - 1,
                    retry_authorization: None,
                })
                .unwrap(),
            PrepareProviderAttemptOutcome::Stale { .. } | PrepareProviderAttemptOutcome::Conflict
        ));
        assert_eq!(count_attempts(), 1);
    }
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    for column in [
        "manifest_digest",
        "payload_digest",
        "provider_identity",
        "provider_idempotency_key",
        "reconciliation_key",
    ] {
        assert!(conn.execute(&format!("UPDATE execass_logical_effects SET {column}='mutated' WHERE logical_effect_id=?1"), [&first.dispatch.logical_effect_id]).is_err());
        assert_eq!(count_attempts(), 1);
    }
}
