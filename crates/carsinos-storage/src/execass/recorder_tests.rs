//! Signed recorder-evidence storage tests.

use super::recorder::insert_recorder_evidence;
use super::resource_tests::{
    acquire, claim_command, setup_exact_recorder, setup_recorder, ResourceFixture,
};
use super::*;
use crate::open_sqlite_connection;
use carsinos_protocol::execass_recorder::{
    recorder_observation_signing_bytes, ProviderFailureClassV1, RecorderObservationKindV1,
    RecorderObservationSourceV1, SignedRecorderObservationV1, TechnicalResourceActualV1,
};
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Barrier};
use std::thread;

const CLAIM_NOW: i64 = 1_800_000_000_200;
const EVIDENCE_NOW: i64 = 1_800_000_000_300;
const USER_DIGEST: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

struct RecorderScenario {
    resources: ResourceFixture,
    claimed: ContinuationClaimRecord,
    signer: SigningKey,
}

fn digest(label: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(label.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn setup_scenario(suffix: &str) -> RecorderScenario {
    setup_scenario_with_resources(suffix, setup_recorder(1, 10, 4))
}

fn setup_scenario_with_resources(suffix: &str, resources: ResourceFixture) -> RecorderScenario {
    let job = acquire(&resources, "worker-a", 10).remove(0);
    let claim = claim_command(
        &resources,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.expect("leased job"),
        suffix,
    );
    let ContinuationClaimOutcome::Claimed(claimed) = resources
        .fixture
        .store
        .claim_continuation_atomically(&resources.integrity, &resources.redactor, &claim)
        .expect("claim continuation")
    else {
        panic!("continuation was not claimed")
    };
    let conn = open_sqlite_connection(&resources.fixture.paths.db_path).unwrap();
    conn.execute(
        "UPDATE execass_logical_effects SET state='invoking',updated_at=?1 WHERE logical_effect_id='effect-1'",
        [CLAIM_NOW + 20],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_provider_attempts(
             attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,
             claim_event_id,claim_receipt_id,attempt_number,fencing_token,host_generation,
             host_instance_id,runtime_fencing_token,status,provider_request_digest,
             provider_response_digest,remote_effect_id,started_at,finished_at
           ) VALUES('recorder-attempt-1',?1,'effect-1',?2,?3,?4,?5,1,?6,?7,?8,?9,
             'invoking',?10,NULL,NULL,?11,NULL)"#,
        params![
            claimed.identity.delegation_id,
            claimed.identity.continuation_id,
            claimed.identity.action_id,
            claimed.identity.claim_event_id,
            claimed.identity.claim_receipt_id,
            claimed.identity.continuation_fencing_token,
            claimed.identity.runtime_host_generation,
            claimed.identity.runtime_host_instance_id,
            claimed.identity.runtime_fencing_token,
            digest("provider-request"),
            CLAIM_NOW + 20,
        ],
    )
    .unwrap();
    drop(conn);

    let signer = SigningKey::from_bytes(&[7_u8; 32]);
    let verifying_key = signer.verifying_key().to_bytes();
    resources
        .fixture
        .store
        .pin_or_validate_recorder_identity(
            &RecorderAuthorityIdentity {
                recorder_key_id: "recorder-key-1".into(),
                key_generation: 1,
                verifying_key_hex: hex(&verifying_key),
                verifying_key_digest: hex(&Sha256::digest(verifying_key)),
                canonical_root_identity: resources.fixture.store.root_identity.clone(),
                installation_identity: "resource-installation".into(),
                os_user_identity_digest: USER_DIGEST.into(),
                state_root_generation: 1,
            },
            CLAIM_NOW,
        )
        .expect("pin recorder identity");
    RecorderScenario {
        resources,
        claimed: *claimed,
        signer,
    }
}

fn signed_observation(
    scenario: &RecorderScenario,
    sequence: u64,
    kind: RecorderObservationKindV1,
    source: RecorderObservationSourceV1,
    observed_at: i64,
    previous_record_digest: String,
) -> SignedRecorderObservationV1 {
    let present = kind == RecorderObservationKindV1::Present;
    let reconciliation = source == RecorderObservationSourceV1::Reconciliation;
    let mut observation = SignedRecorderObservationV1 {
        sequence,
        record_id: format!("recorder-record-{sequence}"),
        canonical_root_identity: scenario.resources.fixture.store.root_identity.clone(),
        installation_id: "resource-installation".into(),
        state_root_generation: 1,
        os_user_identity_digest: USER_DIGEST.into(),
        attempt_id: "recorder-attempt-1".into(),
        logical_effect_id: "effect-1".into(),
        command_digest: digest("recorder-command"),
        kind,
        source,
        provider_identity: "recorder-provider".into(),
        provider_version: "recorder-provider-v1".into(),
        provider_request_digest: digest("provider-request"),
        provider_idempotency_key_digest: Some(digest("provider-idempotency-1")),
        reconciliation_key_digest: Some(digest("provider-reconciliation-1")),
        remote_effect_id: present.then(|| "remote-effect-1".into()),
        response_digest: Some(digest(&format!("response-{sequence}"))),
        evidence_payload_digest: reconciliation.then(|| digest(&format!("evidence-{sequence}"))),
        provider_error_class: (source == RecorderObservationSourceV1::Execution
            && kind == RecorderObservationKindV1::Absent)
            .then_some(ProviderFailureClassV1::Unknown),
        technical_resource_actuals: if present {
            scenario
                .claimed
                .technical_resource_reservations
                .iter()
                .map(|reservation| TechnicalResourceActualV1 {
                    reservation_id: reservation.identity.reservation_id.clone(),
                    amount_actual: 2,
                    evidence_digest: digest(&format!(
                        "actual-{}-{sequence}",
                        reservation.identity.reservation_id
                    )),
                })
                .collect()
        } else {
            Vec::new()
        },
        reconciliation_window_start_ms: reconciliation.then_some(observed_at - 20),
        reconciliation_window_end_ms: reconciliation.then_some(observed_at - 10),
        observed_at_ms: observed_at,
        previous_record_digest,
        record_digest: digest(&format!("record-{sequence}")),
        recorder_key_id: "recorder-key-1".into(),
        recorder_key_generation: 1,
        signature_hex: String::new(),
    };
    let bytes = recorder_observation_signing_bytes(&observation).unwrap();
    observation.signature_hex = hex(&scenario.signer.sign(&bytes).to_bytes());
    observation
}

fn resign(scenario: &RecorderScenario, observation: &mut SignedRecorderObservationV1) {
    observation.signature_hex.clear();
    let bytes = recorder_observation_signing_bytes(observation).unwrap();
    observation.signature_hex = hex(&scenario.signer.sign(&bytes).to_bytes());
}

#[test]
fn exact_overwrite_signed_observation_rejects_wrong_fixed_provider_version() {
    let scenario = setup_scenario_with_resources(
        "exact-overwrite-provider-version",
        setup_exact_recorder(1, 10, 4),
    );
    let mut observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    observation.provider_identity = "carsinos.local-fs.exact-overwrite".into();
    observation.provider_version = "v2".into();
    resign(&scenario, &mut observation);
    assert_eq!(
        scenario
            .resources
            .fixture
            .store
            .verify_recorder_evidence(&observation, EVIDENCE_NOW)
            .unwrap_err()
            .code(),
        "attempt_provider_binding_mismatch"
    );
}

fn receipt_heads(conn: &Connection) -> (i64, Option<String>, i64, Option<String>, i64) {
    conn.query_row(
        "SELECT j.receipt_count,j.receipt_head_digest,d.receipt_chain_count,d.receipt_chain_head_digest,d.state_revision FROM execass_receipt_journal_state j JOIN execass_delegations d ON d.delegation_id='delegation-1' WHERE j.singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    )
    .unwrap()
}

fn import_command(
    scenario: &RecorderScenario,
    observation: &SignedRecorderObservationV1,
    trusted_now: i64,
) -> ReconcileRecorderEvidenceCommand {
    let verified = scenario
        .resources
        .fixture
        .store
        .verify_recorder_evidence(observation, trusted_now)
        .expect("verify signed recorder evidence");
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        receipt_heads(&conn);
    drop(conn);
    let context = scenario
        .resources
        .fixture
        .store
        .read_continuation_receipt_context("continuation-1", trusted_now)
        .unwrap()
        .unwrap();
    let suffix = observation.sequence;
    let event_id = format!("recorder-import-event-{suffix}");
    let payload = serde_json::to_string(&serde_json::json!({
        "attempt_id": observation.attempt_id,
        "logical_effect_id": observation.logical_effect_id,
        "recorder_record_digest": observation.record_digest,
        "result": match observation.kind {
            RecorderObservationKindV1::Present => "present",
            RecorderObservationKindV1::Absent => "absent",
            RecorderObservationKindV1::Unknown => "unknown",
            _ => unreachable!(),
        },
    }))
    .unwrap();
    ReconcileRecorderEvidenceCommand {
        write: WriteContext {
            idempotency_key: format!("recorder-import-idem-{suffix}"),
            correlation_id: format!("recorder-import-corr-{suffix}"),
            causation_id: observation.record_digest.clone(),
            occurred_at: trusted_now,
        },
        claim_identity: scenario.claimed.identity.clone(),
        trusted_now,
        verified_evidence: verified,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: state_revision,
            correlation_id: format!("recorder-import-corr-{suffix}"),
            causation_id: observation.record_digest.clone(),
            occurred_at: trusted_now,
            safe_payload_json: payload,
            duplicate_identity: format!("recorder-import-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("recorder-import-receipt-{suffix}"),
            transaction_id: format!("recorder-import-tx-{suffix}"),
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
            causation_id: observation.record_digest.clone(),
            causation_event_id: event_id,
            actor: context.runtime_actor,
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id,
                fencing_token: context.runtime_fencing_token,
            },
            key: scenario.resources.key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::summary("signed recorder evidence import", &[]).unwrap(),
            occurred_at: trusted_now,
            committed_at: trusted_now,
        },
    }
}

pub(super) fn signed_execution_command_for_attempt(
    resources: &ResourceFixture,
    claim: &ContinuationClaimIdentity,
    attempt: &ProviderAttemptRecord,
    kind: RecorderObservationKindV1,
    suffix: &str,
    sequence: u64,
    observed_at: i64,
) -> ReconcileRecorderEvidenceCommand {
    let provider_error_class =
        (kind == RecorderObservationKindV1::Absent).then_some(ProviderFailureClassV1::Unknown);
    signed_execution_command_for_attempt_with_class(
        resources,
        claim,
        attempt,
        kind,
        provider_error_class,
        (suffix, sequence, observed_at),
    )
}

pub(super) fn signed_execution_command_for_attempt_with_class(
    resources: &ResourceFixture,
    claim: &ContinuationClaimIdentity,
    attempt: &ProviderAttemptRecord,
    kind: RecorderObservationKindV1,
    provider_error_class: Option<ProviderFailureClassV1>,
    identity: (&str, u64, i64),
) -> ReconcileRecorderEvidenceCommand {
    let (suffix, sequence, observed_at) = identity;
    assert!(matches!(
        kind,
        RecorderObservationKindV1::Absent | RecorderObservationKindV1::Unknown
    ));
    let signer = SigningKey::from_bytes(&[7_u8; 32]);
    let verifying_key = signer.verifying_key().to_bytes();
    resources
        .fixture
        .store
        .pin_or_validate_recorder_identity(
            &RecorderAuthorityIdentity {
                recorder_key_id: "recorder-key-1".into(),
                key_generation: 1,
                verifying_key_hex: hex(&verifying_key),
                verifying_key_digest: hex(&Sha256::digest(verifying_key)),
                canonical_root_identity: resources.fixture.store.root_identity.clone(),
                installation_identity: "resource-installation".into(),
                os_user_identity_digest: USER_DIGEST.into(),
                state_root_generation: 1,
            },
            observed_at,
        )
        .unwrap();
    let mut observation = SignedRecorderObservationV1 {
        sequence,
        record_id: format!("effect-test-record-{suffix}"),
        canonical_root_identity: resources.fixture.store.root_identity.clone(),
        installation_id: "resource-installation".into(),
        state_root_generation: 1,
        os_user_identity_digest: USER_DIGEST.into(),
        attempt_id: attempt.attempt_id.clone(),
        logical_effect_id: attempt.dispatch.logical_effect_id.clone(),
        command_digest: digest(&format!("effect-test-command-{suffix}")),
        kind,
        source: RecorderObservationSourceV1::Execution,
        provider_identity: attempt
            .dispatch
            .provider_identity
            .clone()
            .expect("effect recorder test requires a provider"),
        provider_version: "recorder-provider-v1".into(),
        provider_request_digest: attempt.provider_request_digest.clone(),
        provider_idempotency_key_digest: attempt
            .dispatch
            .provider_idempotency_key
            .as_deref()
            .map(digest),
        reconciliation_key_digest: attempt.dispatch.reconciliation_key.as_deref().map(digest),
        remote_effect_id: None,
        response_digest: Some(digest(&format!("effect-test-response-{suffix}"))),
        evidence_payload_digest: None,
        provider_error_class,
        technical_resource_actuals: vec![],
        reconciliation_window_start_ms: None,
        reconciliation_window_end_ms: None,
        observed_at_ms: observed_at,
        previous_record_digest: digest(&format!("effect-test-previous-{suffix}")),
        record_digest: digest(&format!("effect-test-record-{suffix}")),
        recorder_key_id: "recorder-key-1".into(),
        recorder_key_generation: 1,
        signature_hex: String::new(),
    };
    observation.signature_hex = hex(&signer
        .sign(&recorder_observation_signing_bytes(&observation).unwrap())
        .to_bytes());
    let verified = resources
        .fixture
        .store
        .verify_recorder_evidence(&observation, observed_at)
        .unwrap();
    let conn = open_sqlite_connection(&resources.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        receipt_heads(&conn);
    drop(conn);
    let context = resources
        .fixture
        .store
        .read_continuation_receipt_context(&claim.continuation_id, observed_at)
        .unwrap()
        .unwrap();
    let event_id = format!("effect-test-import-event-{suffix}");
    let result = match kind {
        RecorderObservationKindV1::Absent => "absent",
        RecorderObservationKindV1::Unknown => "unknown",
        _ => unreachable!(),
    };
    ReconcileRecorderEvidenceCommand {
        write: WriteContext {
            idempotency_key: format!("effect-test-import-idem-{suffix}"),
            correlation_id: format!("effect-test-import-corr-{suffix}"),
            causation_id: observation.record_digest.clone(),
            occurred_at: observed_at,
        },
        claim_identity: claim.clone(),
        trusted_now: observed_at,
        verified_evidence: verified,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: claim.delegation_id.clone(),
            aggregate_revision: state_revision,
            correlation_id: format!("effect-test-import-corr-{suffix}"),
            causation_id: observation.record_digest.clone(),
            occurred_at: observed_at,
            safe_payload_json: serde_json::to_string(&serde_json::json!({
                "attempt_id": observation.attempt_id,
                "logical_effect_id": observation.logical_effect_id,
                "recorder_record_digest": observation.record_digest,
                "result": result,
            }))
            .unwrap(),
            duplicate_identity: format!("effect-test-import-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("effect-test-import-receipt-{suffix}"),
            transaction_id: format!("effect-test-import-tx-{suffix}"),
            state_root_generation: claim.state_root_generation,
            delegation_id: claim.delegation_id.clone(),
            expected_state_revision: state_revision,
            expected_global_count: global_count,
            expected_global_head_digest: global_head,
            expected_delegation_count: delegation_count,
            expected_delegation_head_digest: delegation_head,
            receipt_kind: ReceiptKind::Continuation,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Continuation,
                subject_id: claim.continuation_id.clone(),
                revision: state_revision,
            },
            causation_id: observation.record_digest,
            causation_event_id: event_id,
            actor: context.runtime_actor,
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id,
                fencing_token: context.runtime_fencing_token,
            },
            key: resources.key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::summary("signed effect-test recorder result", &[]).unwrap(),
            occurred_at: observed_at,
            committed_at: observed_at,
        },
    }
}

#[test]
fn raw_sql_cannot_select_any_recorder_owned_execution_result_without_evidence() {
    let scenario = setup_scenario("raw-execution-result");
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    for (status, remote_effect_id) in [
        ("succeeded", Some("raw-remote")),
        ("outcome_unknown", None),
        ("reconciled_absent", None),
        ("failed", None),
    ] {
        assert!(
            conn.execute(
                "UPDATE execass_provider_attempts SET status=?1,provider_response_digest=?2,remote_effect_id=?3,finished_at=?4 WHERE attempt_id='recorder-attempt-1'",
                params![status, digest(&format!("raw-{status}")), remote_effect_id, EVIDENCE_NOW],
            )
            .is_err(),
            "raw provider-attempt transition was accepted: {status}"
        );
    }
    for state in [
        "succeeded",
        "outcome_unknown",
        "reconciled_absent",
        "failed",
    ] {
        let outcome = serde_json::to_string(&serde_json::json!({
            "recorder_record_digest": digest("raw-record"),
            "remote_effect_id": if state == "succeeded" { Some("raw-remote") } else { None },
            "response_digest": digest(&format!("raw-{state}")),
            "state": state,
        }))
        .unwrap();
        assert!(
            conn.execute(
                "UPDATE execass_logical_effects SET state=?1,outcome_json=?2,updated_at=?3 WHERE logical_effect_id='effect-1'",
                params![state, outcome, EVIDENCE_NOW],
            )
            .is_err(),
            "raw logical-effect transition was accepted: {state}"
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_provider_attempts WHERE attempt_id='recorder-attempt-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "invoking"
    );
    assert_eq!(
        conn.query_row(
            "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "invoking"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_effect_recorder_evidence",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
}

#[test]
fn raw_sql_cannot_insert_forged_or_noncanonical_recorder_evidence() {
    let scenario = setup_scenario("raw-forged-evidence");
    let observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("raw-forged-evidence-prior"),
    );
    let command = import_command(&scenario, &observation, EVIDENCE_NOW);

    type EvidenceMutation = fn(&mut VerifiedRecorderEvidence);
    let mutations: [(&str, EvidenceMutation); 3] = [
        (
            "bogus-signature",
            VerifiedRecorderEvidence::corrupt_signature_for_raw_sql_test,
        ),
        (
            "noncanonical-payload",
            VerifiedRecorderEvidence::make_signed_payload_noncanonical_for_raw_sql_test,
        ),
        (
            "duplicate-payload-key",
            VerifiedRecorderEvidence::duplicate_signed_payload_key_for_raw_sql_test,
        ),
    ];
    for (label, mutate) in mutations {
        let mut forged = command.clone();
        mutate(&mut forged.verified_evidence);
        let mut connection =
            open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
        let transaction = connection.transaction().unwrap();
        assert!(
            insert_recorder_evidence(&transaction, &forged).is_err(),
            "raw recorder evidence insert accepted {label}"
        );
        transaction.rollback().unwrap();
    }

    let mut unmanaged = Connection::open(&scenario.resources.fixture.paths.db_path).unwrap();
    unmanaged.pragma_update(None, "foreign_keys", "ON").unwrap();
    let transaction = unmanaged.transaction().unwrap();
    assert!(
        insert_recorder_evidence(&transaction, &command).is_err(),
        "an unmanaged raw SQLite connection inserted evidence without the verifier function"
    );
    transaction.rollback().unwrap();

    let connection = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM execass_effect_recorder_evidence",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert!(
        connection
            .execute(
                "UPDATE execass_provider_attempts SET status='succeeded',provider_response_digest=?1,remote_effect_id=?2,finished_at=?3 WHERE attempt_id='recorder-attempt-1'",
                params![observation.response_digest, observation.remote_effect_id, observation.observed_at_ms],
            )
            .is_err(),
        "matching raw attempt transition succeeded after forged evidence rejection"
    );
    let outcome = serde_json::to_string(&serde_json::json!({
        "recorder_record_digest": &observation.record_digest,
        "response_digest": &observation.response_digest,
        "remote_effect_id": &observation.remote_effect_id,
        "provider_error_class": &observation.provider_error_class,
        "state": "succeeded",
    }))
    .unwrap();
    assert!(
        connection
            .execute(
                "UPDATE execass_logical_effects SET state='succeeded',outcome_json=?1,updated_at=?2 WHERE logical_effect_id='effect-1'",
                params![outcome, observation.observed_at_ms],
            )
            .is_err(),
        "matching raw effect transition succeeded after forged evidence rejection"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT status FROM execass_provider_attempts WHERE attempt_id='recorder-attempt-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "invoking"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "invoking"
    );
}

#[test]
fn signed_failure_class_cannot_be_changed_in_the_normalized_sql_projection() {
    let scenario = setup_scenario("raw-failure-class-substitution");
    let observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Absent,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("raw-failure-class-substitution-prior"),
    );
    assert_eq!(
        observation.provider_error_class,
        Some(ProviderFailureClassV1::Unknown)
    );
    let mut command = import_command(&scenario, &observation, EVIDENCE_NOW);
    command
        .verified_evidence
        .corrupt_projected_failure_class_for_raw_sql_test(ProviderFailureClassV1::Permanent);
    let mut connection = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    let transaction = connection.transaction().unwrap();
    assert!(
        insert_recorder_evidence(&transaction, &command).is_err(),
        "normalized failure class diverged from signed recorder payload"
    );
    transaction.rollback().unwrap();
    let connection = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM execass_effect_recorder_evidence",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT status FROM execass_provider_attempts WHERE attempt_id='recorder-attempt-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "invoking"
    );
}

#[test]
fn raw_sql_reconciliation_requires_exact_reconciliation_source_and_projection() {
    let scenario = setup_scenario("raw-reconciliation-authority");
    let unknown = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Unknown,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("raw-reconciliation-prior"),
    );
    let unknown_command = import_command(&scenario, &unknown, EVIDENCE_NOW);
    scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &unknown_command,
        )
        .unwrap();

    let execution_present = signed_observation(
        &scenario,
        2,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW + 20,
        unknown.record_digest.clone(),
    );
    assert_eq!(
        scenario
            .resources
            .fixture
            .store
            .verify_recorder_evidence(&execution_present, EVIDENCE_NOW + 20)
            .unwrap_err()
            .code(),
        "attempt_effect_state_mismatch",
        "execution Unknown is terminal for the execution phase; only reconciliation may advance it"
    );
    let wrong_source_projection = signed_observation(
        &scenario,
        2,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Reconciliation,
        EVIDENCE_NOW + 20,
        unknown.record_digest.clone(),
    );
    let mut execution_command =
        import_command(&scenario, &wrong_source_projection, EVIDENCE_NOW + 20);
    execution_command
        .verified_evidence
        .corrupt_source_for_raw_sql_test(RecorderObservationSourceV1::Execution);
    let mut conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    let tx = conn.transaction().unwrap();
    assert!(
        insert_recorder_evidence(&tx, &execution_command).is_err(),
        "source-corrupted recorder evidence passed the cryptographic INSERT guard"
    );
    tx.rollback().unwrap();
    assert!(
        conn.execute(
            "UPDATE execass_provider_attempts SET status='reconciled_present',provider_response_digest=?1,remote_effect_id=?2,finished_at=?3 WHERE attempt_id='recorder-attempt-1'",
            params![wrong_source_projection.response_digest, wrong_source_projection.remote_effect_id, wrong_source_projection.observed_at_ms],
        )
        .is_err(),
        "signed execution evidence authorized a raw reconciliation attempt transition"
    );
    let execution_outcome = serde_json::to_string(&serde_json::json!({
        "recorder_record_digest": &wrong_source_projection.record_digest,
        "response_digest": &wrong_source_projection.response_digest,
        "remote_effect_id": &wrong_source_projection.remote_effect_id,
        "state": "reconciled_present",
    }))
    .unwrap();
    assert!(
        conn.execute(
            "UPDATE execass_logical_effects SET state='reconciled_present',outcome_json=?1,updated_at=?2 WHERE logical_effect_id='effect-1'",
            params![execution_outcome, wrong_source_projection.observed_at_ms],
        )
        .is_err(),
        "signed execution evidence authorized a raw logical-effect reconciliation"
    );

    drop(conn);
    let projection_scenario = setup_scenario("raw-reconciliation-projection");
    let projection_unknown = signed_observation(
        &projection_scenario,
        1,
        RecorderObservationKindV1::Unknown,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("raw-projection-prior"),
    );
    let projection_unknown_command =
        import_command(&projection_scenario, &projection_unknown, EVIDENCE_NOW);
    projection_scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &projection_scenario.resources.integrity,
            &projection_scenario.resources.redactor,
            &projection_unknown_command,
        )
        .unwrap();
    let reconciliation_present = signed_observation(
        &projection_scenario,
        2,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Reconciliation,
        EVIDENCE_NOW + 20,
        projection_unknown.record_digest.clone(),
    );
    let reconciliation_command = import_command(
        &projection_scenario,
        &reconciliation_present,
        EVIDENCE_NOW + 20,
    );
    let mut conn =
        open_sqlite_connection(&projection_scenario.resources.fixture.paths.db_path).unwrap();
    let tx = conn.transaction().unwrap();
    insert_recorder_evidence(&tx, &reconciliation_command).unwrap();
    tx.commit().unwrap();
    assert!(
        conn.execute(
            "UPDATE execass_provider_attempts SET status='reconciled_present',provider_response_digest=?1,remote_effect_id=?2,finished_at=?3 WHERE attempt_id='recorder-attempt-1'",
            params![digest("mismatched-response"), reconciliation_present.remote_effect_id, reconciliation_present.observed_at_ms],
        )
        .is_err(),
        "unrelated reconciliation evidence authorized mismatched attempt fields"
    );
    let mismatched_outcome = serde_json::to_string(&serde_json::json!({
        "recorder_record_digest": digest("mismatched-record"),
        "response_digest": &reconciliation_present.response_digest,
        "remote_effect_id": &reconciliation_present.remote_effect_id,
        "state": "reconciled_present",
    }))
    .unwrap();
    assert!(
        conn.execute(
            "UPDATE execass_logical_effects SET state='reconciled_present',outcome_json=?1,updated_at=?2 WHERE logical_effect_id='effect-1'",
            params![mismatched_outcome, reconciliation_present.observed_at_ms],
        )
        .is_err(),
        "unrelated reconciliation evidence authorized mismatched outcome projection"
    );
}

#[test]
fn exact_replay_uses_persisted_claim_and_receipt_and_rejects_each_identity_drift() {
    type CommandMutator = Box<dyn Fn(&mut ReconcileRecorderEvidenceCommand)>;
    let scenario = setup_scenario("replay-identity");
    let observation = signed_observation(
        &scenario,
        3,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("unimported-record-2"),
    );
    let command = import_command(&scenario, &observation, EVIDENCE_NOW);
    let RecorderEvidenceImportOutcome::Applied(applied) = scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &command,
        )
        .unwrap()
    else {
        panic!("initial recorder import was not applied")
    };
    let RecorderEvidenceImportOutcome::Replayed(replayed) = scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &command,
        )
        .unwrap()
    else {
        panic!("exact recorder import did not replay")
    };
    assert_eq!(replayed.claim_identity, applied.claim_identity);
    assert_eq!(replayed.receipt, applied.receipt);

    let write_mutations: Vec<(&str, CommandMutator)> = vec![
        (
            "write-idempotency",
            Box::new(|c| c.write.idempotency_key.push_str("-drift")),
        ),
        (
            "write-correlation",
            Box::new(|c| c.write.correlation_id.push_str("-drift")),
        ),
        (
            "write-causation",
            Box::new(|c| c.write.causation_id = digest("drift-write-causation")),
        ),
        ("write-time", Box::new(|c| c.write.occurred_at += 1)),
    ];
    let claim_mutations: Vec<(&str, CommandMutator)> = vec![
        (
            "claim-event",
            Box::new(|c| c.claim_identity.claim_event_id.push_str("-drift")),
        ),
        (
            "claim-receipt",
            Box::new(|c| c.claim_identity.claim_receipt_id.push_str("-drift")),
        ),
        (
            "continuation",
            Box::new(|c| c.claim_identity.continuation_id.push_str("-drift")),
        ),
        (
            "delegation",
            Box::new(|c| c.claim_identity.delegation_id.push_str("-drift")),
        ),
        (
            "action",
            Box::new(|c| c.claim_identity.action_id.push_str("-drift")),
        ),
        (
            "job",
            Box::new(|c| c.claim_identity.job_id.push_str("-drift")),
        ),
        (
            "worker",
            Box::new(|c| c.claim_identity.worker_id.push_str("-drift")),
        ),
        (
            "job-lease",
            Box::new(|c| c.claim_identity.job_lease_expires_at += 1),
        ),
        (
            "continuation-fence",
            Box::new(|c| c.claim_identity.continuation_fencing_token += 1),
        ),
        (
            "host-generation",
            Box::new(|c| c.claim_identity.runtime_host_generation += 1),
        ),
        (
            "host-instance",
            Box::new(|c| c.claim_identity.runtime_host_instance_id.push_str("-drift")),
        ),
        (
            "runtime-fence",
            Box::new(|c| c.claim_identity.runtime_fencing_token += 1),
        ),
        (
            "root-generation",
            Box::new(|c| c.claim_identity.state_root_generation += 1),
        ),
        (
            "runtime-authority",
            Box::new(|c| {
                c.claim_identity
                    .runtime_authority_provenance_id
                    .push_str("-drift")
            }),
        ),
        (
            "runtime-actor",
            Box::new(|c| c.claim_identity.runtime_actor_identity.push_str("-drift")),
        ),
        (
            "policy-revision",
            Box::new(|c| c.claim_identity.policy_revision += 1),
        ),
        (
            "stop-epoch",
            Box::new(|c| c.claim_identity.global_stop_epoch += 1),
        ),
        (
            "quota-policy",
            Box::new(|c| {
                c.claim_identity
                    .technical_quota_policy_digest
                    .push_str("-drift")
            }),
        ),
        (
            "quota-snapshot",
            Box::new(|c| {
                c.claim_identity.technical_quota_snapshot_id = Some("drift-snapshot".into())
            }),
        ),
        (
            "reservation-set",
            Box::new(|c| {
                c.claim_identity
                    .technical_resource_reservation_set_digest
                    .push_str("-drift")
            }),
        ),
    ];
    let receipt_mutations: Vec<(&str, CommandMutator)> = vec![
        (
            "receipt-id",
            Box::new(|c| c.receipt.receipt_id.push_str("-drift")),
        ),
        (
            "transaction",
            Box::new(|c| c.receipt.transaction_id.push_str("-drift")),
        ),
        (
            "receipt-root",
            Box::new(|c| c.receipt.state_root_generation += 1),
        ),
        (
            "receipt-delegation",
            Box::new(|c| c.receipt.delegation_id.push_str("-drift")),
        ),
        (
            "state-revision",
            Box::new(|c| c.receipt.expected_state_revision += 1),
        ),
        (
            "global-count",
            Box::new(|c| c.receipt.expected_global_count += 1),
        ),
        (
            "global-head",
            Box::new(|c| c.receipt.expected_global_head_digest = Some(digest("drift-global-head"))),
        ),
        (
            "delegation-count",
            Box::new(|c| c.receipt.expected_delegation_count += 1),
        ),
        (
            "delegation-head",
            Box::new(|c| {
                c.receipt.expected_delegation_head_digest = Some(digest("drift-delegation-head"))
            }),
        ),
        (
            "receipt-kind",
            Box::new(|c| c.receipt.receipt_kind = ReceiptKind::Decision),
        ),
        (
            "subject-kind",
            Box::new(|c| c.receipt.subject.kind = ReceiptSubjectKind::Decision),
        ),
        (
            "subject-id",
            Box::new(|c| c.receipt.subject.subject_id.push_str("-drift")),
        ),
        (
            "subject-revision",
            Box::new(|c| c.receipt.subject.revision += 1),
        ),
        (
            "causation",
            Box::new(|c| c.receipt.causation_id = digest("drift-causation")),
        ),
        (
            "causation-event",
            Box::new(|c| c.receipt.causation_event_id.push_str("-drift")),
        ),
        (
            "actor-type",
            Box::new(|c| c.receipt.actor.actor_type = ActorType::Worker),
        ),
        (
            "actor-identity",
            Box::new(|c| {
                c.receipt.actor.actor_identity = SafeText::new("drift-actor", &[]).unwrap()
            }),
        ),
        (
            "actor-authority",
            Box::new(|c| c.receipt.actor.authority_provenance_id.push_str("-drift")),
        ),
        (
            "receipt-host-generation",
            Box::new(|c| c.receipt.runtime.host_generation += 1),
        ),
        (
            "receipt-host-instance",
            Box::new(|c| c.receipt.runtime.host_instance_id.push_str("-drift")),
        ),
        (
            "receipt-runtime-fence",
            Box::new(|c| c.receipt.runtime.fencing_token += 1),
        ),
        (
            "key-id",
            Box::new(|c| c.receipt.key.key_id.push_str("-drift")),
        ),
        (
            "key-generation",
            Box::new(|c| c.receipt.key.key_generation += 1),
        ),
        (
            "rotation",
            Box::new(|c| {
                c.receipt.rotation = Some(ReceiptRotation {
                    transition_id: "drift-rotation".into(),
                    reason: SafeText::summary("drift rotation", &[]).unwrap(),
                    previous_key: c.receipt.key.clone(),
                })
            }),
        ),
        (
            "evidence",
            Box::new(|c| {
                c.receipt.evidence.push(ReceiptEvidenceInput {
                    authority_link_id: "drift-link".into(),
                    kind: AuthorityLinkKind::SecurityAuditEvent,
                    source_id: "drift-source".into(),
                    authoritative_revision: 0,
                })
            }),
        ),
        (
            "summary",
            Box::new(|c| {
                c.receipt.redacted_summary = SafeText::summary("drift summary", &[]).unwrap()
            }),
        ),
        ("occurred", Box::new(|c| c.receipt.occurred_at += 1)),
        ("committed", Box::new(|c| c.receipt.committed_at += 1)),
    ];
    for (label, mutate) in write_mutations
        .into_iter()
        .chain(claim_mutations)
        .chain(receipt_mutations)
    {
        let mut drift = command.clone();
        mutate(&mut drift);
        assert_eq!(
            scenario
                .resources
                .fixture
                .store
                .reconcile_recorder_evidence_atomically(
                    &scenario.resources.integrity,
                    &scenario.resources.redactor,
                    &drift,
                )
                .unwrap(),
            RecorderEvidenceImportOutcome::Conflict,
            "replay identity drift was accepted: {label}"
        );
    }
}

#[test]
fn sparse_signed_observation_subset_accepts_gaps_and_rejects_forks_and_terminal_conflicts() {
    let scenario = setup_scenario("sparse-observation-subset");
    let unknown = signed_observation(
        &scenario,
        7,
        RecorderObservationKindV1::Unknown,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("unimported-record-6"),
    );
    let unknown_command = import_command(&scenario, &unknown, EVIDENCE_NOW);
    scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &unknown_command,
        )
        .unwrap();

    let mut fork = unknown.clone();
    fork.record_id = "forked-record-7".into();
    fork.record_digest = digest("forked-record-7");
    fork.response_digest = Some(digest("forked-response-7"));
    resign(&scenario, &mut fork);
    assert_eq!(
        scenario
            .resources
            .fixture
            .store
            .verify_recorder_evidence(&fork, EVIDENCE_NOW)
            .unwrap_err()
            .code(),
        "duplicate_journal_sequence"
    );

    let present = signed_observation(
        &scenario,
        11,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Reconciliation,
        EVIDENCE_NOW + 20,
        digest("unimported-record-10"),
    );
    let present_command = import_command(&scenario, &present, EVIDENCE_NOW + 20);
    assert!(matches!(
        scenario
            .resources
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &scenario.resources.integrity,
                &scenario.resources.redactor,
                &present_command,
            )
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));

    let conflict = signed_observation(
        &scenario,
        12,
        RecorderObservationKindV1::Absent,
        RecorderObservationSourceV1::Reconciliation,
        EVIDENCE_NOW + 30,
        present.record_digest.clone(),
    );
    assert_eq!(
        scenario
            .resources
            .fixture
            .store
            .verify_recorder_evidence(&conflict, EVIDENCE_NOW + 30)
            .unwrap_err()
            .code(),
        "terminal_evidence_conflict"
    );
}

#[test]
fn signed_execution_present_converges_atomically_and_reloads_verifiably() {
    let scenario = setup_scenario("execution-present");
    let observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let command = import_command(&scenario, &observation, EVIDENCE_NOW);
    let RecorderEvidenceImportOutcome::Applied(applied) = scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &command,
        )
        .unwrap()
    else {
        panic!("signed execution evidence was not applied")
    };
    assert_eq!(applied.result, RecorderEvidenceResult::Present);
    assert!(applied
        .technical_resource_reservations
        .iter()
        .all(|reservation| reservation.status == "settled"));
    let reopened = ExecAssStore::open(&scenario.resources.fixture.paths).unwrap();
    let reloaded = reopened
        .reload_and_verify_recorder_evidence(&observation.record_digest, EVIDENCE_NOW)
        .unwrap();
    assert_eq!(reloaded.result(), RecorderEvidenceResult::Present);
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_provider_attempts WHERE attempt_id='recorder-attempt-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "succeeded"
    );
    assert!(conn
        .execute(
            "UPDATE execass_provider_attempts SET provider_error_class='permanent' WHERE attempt_id='recorder-attempt-1'",
            [],
        )
        .is_err(), "nonfailed attempt accepted a provider failure class");
    assert_eq!(
        conn.query_row(
            "SELECT operation FROM execass_continuation_operation_history WHERE event_id=?1",
            [&command.outbox_event.event_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "settle"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_receipt_recorder_evidence_refs WHERE recorder_record_digest=?1",
            [&observation.record_digest],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
}

#[test]
fn signed_execution_absent_records_definite_failure_and_preserves_retry_eligibility() {
    let scenario = setup_scenario("execution-absent");
    let observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Absent,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let command = import_command(&scenario, &observation, EVIDENCE_NOW);
    assert!(matches!(
        scenario
            .resources
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &scenario.resources.integrity,
                &scenario.resources.redactor,
                &command,
            )
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));
    assert!(matches!(
        scenario
            .resources
            .fixture
            .store
            .reconcile_recorder_evidence_atomically(
                &scenario.resources.integrity,
                &scenario.resources.redactor,
                &command,
            )
            .unwrap(),
        RecorderEvidenceImportOutcome::Replayed(_)
    ));
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    let (attempt, effect): (String, String) = conn
        .query_row(
            "SELECT a.status,e.state FROM execass_provider_attempts a JOIN execass_logical_effects e ON e.logical_effect_id=a.logical_effect_id WHERE a.attempt_id='recorder-attempt-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((attempt.as_str(), effect.as_str()), ("failed", "failed"));
    assert_eq!(
        conn.query_row(
            "SELECT provider_error_class FROM execass_provider_attempts WHERE attempt_id='recorder-attempt-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "unknown"
    );
    assert_eq!(
        conn.query_row(
            "SELECT provider_error_class FROM execass_effect_recorder_evidence WHERE attempt_id='recorder-attempt-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "unknown"
    );
    for forged in ["permanent", "transient"] {
        assert!(
            conn.execute(
                "UPDATE execass_provider_attempts SET provider_error_class=?1 WHERE attempt_id='recorder-attempt-1'",
                [forged],
            )
            .is_err(),
            "terminal attempt class changed to {forged}"
        );
    }
    assert!(conn
        .execute(
            "UPDATE execass_provider_attempts SET provider_error_class=NULL WHERE attempt_id='recorder-attempt-1'",
            [],
        )
        .is_err(), "failed attempt dropped its provider failure class");
    assert!(conn
        .execute(
            "UPDATE execass_effect_recorder_evidence SET provider_error_class='permanent' WHERE attempt_id='recorder-attempt-1'",
            [],
        )
        .is_err(), "signed evidence class was mutable");
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_continuation_operation_history WHERE event_id=?1",
            [&command.outbox_event.event_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "a nonterminal provider failure must not consume the claim's settle history slot"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_actuals",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE status='reserved'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        4
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "executing"
    );
    assert_eq!(
        conn.query_row(
            "SELECT enabled FROM jobs WHERE job_id=?1",
            [&scenario.claimed.identity.job_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
}

#[test]
fn signed_unknown_then_reconciliation_absent_retains_then_converges() {
    let scenario = setup_scenario("unknown-absent");
    let unknown = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Unknown,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let unknown_command = import_command(&scenario, &unknown, EVIDENCE_NOW);
    scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &unknown_command,
        )
        .unwrap();
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE status='reconciliation_required'", [], |row| row.get::<_, i64>(0)).unwrap(),
        4
    );
    assert_eq!(
        conn.query_row(
            "SELECT enabled FROM jobs WHERE job_id=?1",
            [&scenario.claimed.identity.job_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(conn);
    let absent = signed_observation(
        &scenario,
        2,
        RecorderObservationKindV1::Absent,
        RecorderObservationSourceV1::Reconciliation,
        EVIDENCE_NOW + 20,
        unknown.record_digest.clone(),
    );
    let absent_command = import_command(&scenario, &absent, EVIDENCE_NOW + 20);
    scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &absent_command,
        )
        .unwrap();
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        conn.query_row(
            "SELECT operation FROM execass_continuation_operation_history WHERE event_id=?1",
            [&absent_command.outbox_event.event_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "reconcile"
    );
}

#[test]
fn signed_unknown_then_reconciliation_present_settles_exact_bounded_actuals() {
    let scenario = setup_scenario("unknown-present");
    let unknown = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Unknown,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let unknown_command = import_command(&scenario, &unknown, EVIDENCE_NOW);
    scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &unknown_command,
        )
        .unwrap();
    let present = signed_observation(
        &scenario,
        2,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Reconciliation,
        EVIDENCE_NOW + 20,
        unknown.record_digest.clone(),
    );
    assert_eq!(present.reconciliation_window_start_ms, Some(EVIDENCE_NOW));
    assert_eq!(
        present.reconciliation_window_end_ms,
        Some(EVIDENCE_NOW + 10)
    );
    let present_command = import_command(&scenario, &present, EVIDENCE_NOW + 20);
    let RecorderEvidenceImportOutcome::Applied(applied) = scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &present_command,
        )
        .unwrap()
    else {
        panic!("reconciliation present evidence was not applied")
    };
    assert!(applied
        .technical_resource_reservations
        .iter()
        .all(|reservation| reservation.status == "settled"));
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    let (attempt, effect, operation): (String, String, String) = conn
        .query_row(
            "SELECT a.status,e.state,h.operation FROM execass_provider_attempts a JOIN execass_logical_effects e ON e.logical_effect_id=a.logical_effect_id JOIN execass_continuation_operation_history h ON h.event_id=?1 WHERE a.attempt_id='recorder-attempt-1'",
            [&present_command.outbox_event.event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (attempt.as_str(), effect.as_str(), operation.as_str()),
        ("reconciled_present", "reconciled_present", "reconcile")
    );
    let stored_actuals = conn
        .query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_actuals actual JOIN execass_technical_resource_reservations reservation ON reservation.reservation_id=actual.reservation_id WHERE actual.amount_actual=2 AND reservation.status='settled'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(stored_actuals, 4);
}

#[test]
fn every_signed_authority_and_result_field_mutation_is_rejected_without_storage() {
    type Mutator = Box<dyn Fn(&mut SignedRecorderObservationV1)>;
    let scenario = setup_scenario("mutation-corpus");
    let base = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let mutations: Vec<(&str, Mutator)> = vec![
        (
            "root",
            Box::new(|o| o.canonical_root_identity = digest("other-root")),
        ),
        (
            "installation",
            Box::new(|o| o.installation_id = "other-installation".into()),
        ),
        (
            "os-user",
            Box::new(|o| o.os_user_identity_digest = "e".repeat(64)),
        ),
        (
            "root-generation",
            Box::new(|o| o.state_root_generation += 1),
        ),
        (
            "key-id",
            Box::new(|o| o.recorder_key_id = "other-key".into()),
        ),
        (
            "key-generation",
            Box::new(|o| o.recorder_key_generation += 1),
        ),
        ("sequence", Box::new(|o| o.sequence += 1)),
        ("record-id", Box::new(|o| o.record_id.push_str("-changed"))),
        (
            "record-digest",
            Box::new(|o| o.record_digest = digest("other-record")),
        ),
        (
            "attempt",
            Box::new(|o| o.attempt_id = "other-attempt".into()),
        ),
        (
            "effect",
            Box::new(|o| o.logical_effect_id = "other-effect".into()),
        ),
        (
            "command",
            Box::new(|o| o.command_digest = digest("other-command")),
        ),
        (
            "provider",
            Box::new(|o| o.provider_identity = "other-provider".into()),
        ),
        (
            "provider-version",
            Box::new(|o| o.provider_version.push_str("-changed")),
        ),
        (
            "provider-request",
            Box::new(|o| o.provider_request_digest = digest("other-request")),
        ),
        (
            "provider-idempotency",
            Box::new(|o| o.provider_idempotency_key_digest = Some(digest("other-idempotency"))),
        ),
        (
            "reconciliation-key",
            Box::new(|o| o.reconciliation_key_digest = Some(digest("other-reconciliation"))),
        ),
        (
            "source",
            Box::new(|o| o.source = RecorderObservationSourceV1::Reconciliation),
        ),
        (
            "kind",
            Box::new(|o| o.kind = RecorderObservationKindV1::Absent),
        ),
        (
            "window-start",
            Box::new(|o| o.reconciliation_window_start_ms = Some(1)),
        ),
        (
            "window-end",
            Box::new(|o| o.reconciliation_window_end_ms = Some(2)),
        ),
        (
            "response",
            Box::new(|o| o.response_digest = Some(digest("other-response"))),
        ),
        (
            "evidence",
            Box::new(|o| o.evidence_payload_digest = Some(digest("other-evidence"))),
        ),
        (
            "remote",
            Box::new(|o| o.remote_effect_id = Some("other-remote".into())),
        ),
        (
            "actual-id",
            Box::new(|o| {
                o.technical_resource_actuals[0]
                    .reservation_id
                    .push_str("-changed")
            }),
        ),
        (
            "actual-amount",
            Box::new(|o| o.technical_resource_actuals[0].amount_actual += 1),
        ),
        (
            "actual-digest",
            Box::new(|o| o.technical_resource_actuals[0].evidence_digest = digest("other-actual")),
        ),
        ("observed-at", Box::new(|o| o.observed_at_ms -= 1)),
        (
            "previous-record",
            Box::new(|o| o.previous_record_digest = digest("other-previous")),
        ),
        (
            "signature",
            Box::new(|o| {
                let replacement = if o.signature_hex.starts_with("00") {
                    "ff"
                } else {
                    "00"
                };
                o.signature_hex.replace_range(..2, replacement);
            }),
        ),
    ];
    for (label, mutate) in mutations {
        let mut observation = base.clone();
        mutate(&mut observation);
        assert!(
            scenario
                .resources
                .fixture
                .store
                .verify_recorder_evidence(&observation, EVIDENCE_NOW)
                .is_err(),
            "mutation was accepted: {label}"
        );
    }
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    for table in [
        "execass_effect_recorder_evidence",
        "execass_receipt_recorder_evidence_refs",
        "execass_technical_resource_actuals",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "mutation corpus must not write {table}"
        );
    }
}

#[test]
fn signature_mutation_is_rejected_before_storage() {
    let scenario = setup_scenario("signature-mutation");
    let mut observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    observation.provider_version.push_str("-tampered");
    assert_eq!(
        scenario
            .resources
            .fixture
            .store
            .verify_recorder_evidence(&observation, EVIDENCE_NOW)
            .unwrap_err()
            .code(),
        "signature_mismatch"
    );
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_effect_recorder_evidence",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn receipt_failure_rolls_back_evidence_resources_and_audit_rows() {
    let scenario = setup_scenario("rollback");
    let observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let mut command = import_command(&scenario, &observation, EVIDENCE_NOW);
    command.receipt.key.key_id = "missing-receipt-key".into();
    assert!(scenario
        .resources
        .fixture
        .store
        .reconcile_recorder_evidence_atomically(
            &scenario.resources.integrity,
            &scenario.resources.redactor,
            &command,
        )
        .is_err());
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    for table in [
        "execass_effect_recorder_evidence",
        "execass_receipt_recorder_evidence_refs",
        "execass_technical_resource_actuals",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "{table} must roll back"
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_provider_attempts WHERE attempt_id='recorder-attempt-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "invoking"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE status='reserved'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        4
    );
}

#[test]
fn concurrent_import_is_one_apply_one_exact_replay_and_never_touches_dangerous_grants() {
    let scenario = setup_scenario("race");
    let observation = signed_observation(
        &scenario,
        1,
        RecorderObservationKindV1::Present,
        RecorderObservationSourceV1::Execution,
        EVIDENCE_NOW,
        digest("prior-journal-record"),
    );
    let command = import_command(&scenario, &observation, EVIDENCE_NOW);
    let barrier = Arc::new(Barrier::new(8));
    let mut joins = Vec::new();
    for _ in 0..8 {
        let store = scenario.resources.fixture.store.clone();
        let paths = scenario.resources.fixture.paths.clone();
        let command = command.clone();
        let barrier = Arc::clone(&barrier);
        joins.push(thread::spawn(move || {
            let integrity = ReceiptIntegrityStore::open(&paths).unwrap();
            let redactor = ReceiptRedactor::new(&["resource-test-secret"]).unwrap();
            barrier.wait();
            store
                .reconcile_recorder_evidence_atomically(&integrity, &redactor, &command)
                .unwrap()
        }));
    }
    let outcomes = joins
        .into_iter()
        .map(|join| join.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RecorderEvidenceImportOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RecorderEvidenceImportOutcome::Replayed(_)))
            .count(),
        7
    );
    let conn = open_sqlite_connection(&scenario.resources.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_effect_recorder_evidence",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_receipts WHERE receipt_id=?1",
            [&command.receipt.receipt_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM execass_receipt_recorder_evidence_refs WHERE recorder_record_digest=?1", [&observation.record_digest], |row| row.get::<_, i64>(0)).unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_actuals",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        4
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_accepted_confirmation_grants",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "recorder import must not create or mutate dangerous-action grants"
    );
}
