use super::claim::{insert_operation_history, resource_identity_set_digest, OperationHistoryWrite};
use super::recorder_tests::signed_execution_command_for_attempt;
use super::rows::{
    insert_action_branch, insert_outbox, insert_planned_logical_effect,
    insert_technical_quota_snapshot, insert_technical_resource_requirements,
};
use super::tests::{fixture, foundation, table_count, Fixture};
use super::*;
use crate::{open_sqlite_connection, Storage};
use carsinos_core::execass_policy::{
    compile_technical_quota_snapshot, compile_technical_resource_requirements,
    technical_effective_authority_digest, TechnicalQuotaEntryInput,
    TechnicalResourceKind as CoreResourceKind, TechnicalResourceRequirementInput,
};
use carsinos_protocol::execass_recorder::RecorderObservationKindV1;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::Duration;

const SCHEDULED_AT: i64 = 1_800_000_000_100;
pub(super) const CLAIM_NOW: i64 = 1_800_000_000_200;
const LEASE_MS: i64 = 5_000;

pub(super) struct ResourceFixture {
    pub(super) fixture: Fixture,
    pub(super) integrity: ReceiptIntegrityStore,
    pub(super) redactor: ReceiptRedactor,
    pub(super) key: ReceiptKeyRef,
}

pub(super) fn setup(continuation_count: usize, limit: i64, required: i64) -> ResourceFixture {
    setup_with_provider(continuation_count, limit, required, None)
}

pub(super) fn setup_recorder(
    continuation_count: usize,
    limit: i64,
    required: i64,
) -> ResourceFixture {
    setup_with_provider(
        continuation_count,
        limit,
        required,
        Some("recorder-provider"),
    )
}

pub(super) fn setup_exact_recorder(
    continuation_count: usize,
    limit: i64,
    required: i64,
) -> ResourceFixture {
    setup_with_provider(
        continuation_count,
        limit,
        required,
        Some("carsinos.local-fs.exact-overwrite"),
    )
}

fn setup_with_provider(
    continuation_count: usize,
    limit: i64,
    required: i64,
    provider_identity: Option<&str>,
) -> ResourceFixture {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_host(&fixture);
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(continuation_count, 1);
    let authority_digest =
        technical_effective_authority_digest(r#"{"profile":"balanced"}"#).unwrap();
    let snapshot = compile_technical_quota_snapshot(
        "delegation-1",
        1,
        &authority_digest,
        "delegation",
        quota_inputs(limit),
    )
    .unwrap();
    insert_technical_quota_snapshot(&conn, &snapshot, SCHEDULED_AT - 50).unwrap();
    for index in 1..=continuation_count {
        let continuation_id = format!("continuation-{index}");
        let action_id = format!("action-{index}");
        let effect_id = format!("effect-{index}");
        insert_planned_logical_effect(
            &conn,
            &PlannedLogicalEffectRecord {
                logical_effect_id: effect_id.clone(),
                delegation_id: "delegation-1".into(),
                continuation_id,
                action_kind:
                    LogicalEffectActionKind::ReadOnlyLocalInspectionAndBoundedReversibleLocalWork,
                operation_reversible: true,
                declared_recovery_safe_boundary: DeclaredRecoverySafeBoundary::IndependentAbsence,
                internal_idempotency_key: format!("effect-idem-{index}"),
                provider_identity: provider_identity.map(str::to_owned),
                provider_idempotency_key: provider_identity
                    .filter(|provider| *provider != "carsinos.local-fs.exact-overwrite")
                    .map(|_| format!("provider-idempotency-{index}")),
                reconciliation_key: provider_identity
                    .map(|_| format!("provider-reconciliation-{index}")),
                manifest_digest: "sha256:manifest".into(),
                payload_digest: format!("sha256:payload-{index}"),
                created_at: SCHEDULED_AT - 50,
            },
        )
        .unwrap();
        let requirements = compile_technical_resource_requirements(
            &snapshot,
            &effect_id,
            &action_id,
            "sha256:manifest",
            resource_inputs(required, |kind| TechnicalResourceRequirementInput {
                kind,
                unit: unit(kind).into(),
                amount: required,
            }),
        )
        .unwrap();
        insert_technical_resource_requirements(&conn, &requirements, SCHEDULED_AT - 50).unwrap();
    }
    drop(conn);
    fixture
        .store
        .materialize_runnable_continuation_jobs(SCHEDULED_AT, 10)
        .unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity.provision_initial_key("resource-key").unwrap();
    ResourceFixture {
        fixture,
        integrity,
        redactor: ReceiptRedactor::new(&["resource-test-secret"]).unwrap(),
        key,
    }
}

fn resource_inputs<T>(amount: i64, make: impl Fn(CoreResourceKind) -> T) -> Vec<T> {
    let _ = amount;
    [
        CoreResourceKind::Tokens,
        CoreResourceKind::TimeMs,
        CoreResourceKind::ConnectorCalls,
        CoreResourceKind::ResourceUnits,
    ]
    .into_iter()
    .map(make)
    .collect()
}

fn unit(kind: CoreResourceKind) -> &'static str {
    match kind {
        CoreResourceKind::Tokens => "token",
        CoreResourceKind::TimeMs => "ms",
        CoreResourceKind::ConnectorCalls => {
            "connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }
        CoreResourceKind::ResourceUnits => {
            "resource:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }
    }
}

fn quota_inputs(limit: i64) -> Vec<TechnicalQuotaEntryInput> {
    resource_inputs(limit, |kind| TechnicalQuotaEntryInput {
        kind,
        unit: unit(kind).into(),
        limit,
    })
}

fn seed_host(fixture: &Fixture) {
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(1,'execass',1,'resource-installation','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','resource-host',1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('resource-host-lease','execass',1,'resource-host',1,1,9999999999999)",
        [],
    ).unwrap();
}

fn heads(conn: &Connection) -> (i64, Option<String>, i64, Option<String>, i64) {
    conn.query_row(
        "SELECT j.receipt_count,j.receipt_head_digest,d.receipt_chain_count,d.receipt_chain_head_digest,d.state_revision FROM execass_receipt_journal_state j JOIN execass_delegations d ON d.delegation_id='delegation-1' WHERE j.singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).unwrap()
}

fn lifecycle_evidence(conn: &Connection, suffix: &str) -> ReceiptEvidenceInput {
    let source_id = format!("resource-lifecycle-evidence-{suffix}");
    let link_id = format!("resource-lifecycle-evidence-link-{suffix}");
    let exists = conn
        .query_row(
            "SELECT 1 FROM execass_authority_links WHERE link_id=?1",
            params![link_id],
            |_| Ok(()),
        )
        .optional()
        .unwrap()
        .is_some();
    if !exists {
        conn.execute(
            r#"INSERT INTO security_audit_events(
                 event_id,request_id,correlation_id,principal,action,resource,
                 decision,transport,status,created_at
               ) VALUES(?1,?2,?3,'execass-runtime','technical-resource-lifecycle-evidence',
                 'technical-resource-reservation','allow','local-runtime','recorded',?4)"#,
            params![
                source_id,
                format!("request-{suffix}"),
                format!("corr-{suffix}"),
                SCHEDULED_AT
            ],
        )
        .unwrap();
        let (event_id, correlation_id, causation_id, occurred_at): (String, String, String, i64) = conn
            .query_row(
                "SELECT event_id,correlation_id,causation_id,occurred_at FROM execass_outbox_events WHERE aggregate_id='delegation-1' AND event_name='execass.v1.delegation.transitioned' ORDER BY aggregate_revision DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        let (state_revision, link_revision): (i64, i64) = conn
            .query_row(
                "SELECT state_revision,(SELECT COALESCE(MAX(link_revision),0)+1 FROM execass_authority_links WHERE delegation_id='delegation-1') FROM execass_delegations WHERE delegation_id='delegation-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        conn.execute(
            r#"INSERT INTO execass_authority_links(
                 link_id,delegation_id,link_revision,delegation_state_revision,
                 correlation_id,causation_id,outbox_event_id,authority_kind,
                 security_audit_event_id,authoritative_revision,linked_at
               ) VALUES(?1,'delegation-1',?2,?3,?4,?5,?6,'security_audit_event',?7,0,?8)"#,
            params![
                link_id,
                link_revision,
                state_revision,
                correlation_id,
                causation_id,
                event_id,
                source_id,
                occurred_at,
            ],
        )
        .unwrap();
    }
    ReceiptEvidenceInput {
        authority_link_id: link_id,
        kind: AuthorityLinkKind::SecurityAuditEvent,
        source_id,
        authoritative_revision: 0,
    }
}

fn assert_receipt_evidence(conn: &Connection, receipt_id: &str, expected: &ReceiptEvidenceInput) {
    let row: (String, String, i64, String, String, String) = conn
        .query_row(
            r#"SELECT authority_kind,source_id,authoritative_revision,authority_link_id,
                      observation_digest,deep_link
               FROM execass_receipt_evidence_refs WHERE receipt_id=?1"#,
            [receipt_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("read canonical lifecycle receipt evidence");
    assert_eq!(row.0, expected.kind.as_str());
    assert_eq!(row.1, expected.source_id);
    assert_eq!(row.2, expected.authoritative_revision);
    assert_eq!(row.3, expected.authority_link_id);
    assert_eq!(row.4.len(), 64);
    assert_eq!(
        row.5,
        format!(
            "carsinos://evidence/v1/{}/{}?revision={}",
            expected.kind.as_str(),
            expected.source_id,
            expected.authoritative_revision
        )
    );
}

pub(super) fn claim_command(
    f: &ResourceFixture,
    continuation_id: &str,
    job_id: &str,
    worker_id: &str,
    lease_expires_at: i64,
    suffix: &str,
) -> ContinuationClaimCommand {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        heads(&conn);
    let context = f
        .fixture
        .store
        .read_continuation_receipt_context(continuation_id, CLAIM_NOW)
        .unwrap()
        .unwrap();
    let event_id = format!("resource-claim-event-{suffix}");
    ContinuationClaimCommand {
        write: WriteContext {
            idempotency_key: format!("resource-claim-idem-{suffix}"),
            correlation_id: format!("resource-claim-corr-{suffix}"),
            causation_id: format!("resource-claim-cause-{suffix}"),
            occurred_at: CLAIM_NOW + 10,
        },
        continuation_id: continuation_id.into(),
        job_id: job_id.into(),
        worker_id: worker_id.into(),
        job_lease_expires_at: lease_expires_at,
        trusted_now: CLAIM_NOW,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: state_revision,
            correlation_id: format!("resource-claim-corr-{suffix}"),
            causation_id: format!("resource-claim-cause-{suffix}"),
            occurred_at: CLAIM_NOW + 10,
            safe_payload_json: "{}".into(),
            duplicate_identity: format!("resource-claim-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("resource-claim-receipt-{suffix}"),
            transaction_id: format!("resource-claim-tx-{suffix}"),
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
                subject_id: continuation_id.into(),
                revision: state_revision,
            },
            causation_id: format!("resource-claim-cause-{suffix}"),
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
            redacted_summary: SafeText::summary("resource claim", &[]).unwrap(),
            occurred_at: CLAIM_NOW + 10,
            committed_at: CLAIM_NOW + 10,
        },
    }
}

fn settle_command(
    f: &ResourceFixture,
    claimed: &ContinuationClaimRecord,
    actuals: Vec<TechnicalResourceActualInput>,
) -> ContinuationSettleCommand {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        heads(&conn);
    let event_id = "resource-settle-event".to_string();
    ContinuationSettleCommand {
        write: WriteContext {
            idempotency_key: "resource-settle-idem".into(),
            correlation_id: "resource-settle-corr".into(),
            causation_id: "resource-settle-cause".into(),
            occurred_at: CLAIM_NOW + 20,
        },
        identity: claimed.identity.clone(),
        trusted_now: CLAIM_NOW + 1,
        result_status: ContinuationStatus::Terminal,
        technical_resource_actuals: actuals,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: state_revision,
            correlation_id: "resource-settle-corr".into(),
            causation_id: "resource-settle-cause".into(),
            occurred_at: CLAIM_NOW + 20,
            safe_payload_json: "{}".into(),
            duplicate_identity: "resource-settle-idem".into(),
        },
        receipt: AppendReceiptCommand {
            receipt_id: "resource-settle-receipt".into(),
            transaction_id: "resource-settle-tx".into(),
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
                subject_id: claimed.identity.continuation_id.clone(),
                revision: state_revision,
            },
            causation_id: "resource-settle-cause".into(),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: ActorType::Runtime,
                actor_identity: SafeText::new(&claimed.identity.runtime_actor_identity, &[])
                    .unwrap(),
                authority_provenance_id: claimed.identity.runtime_authority_provenance_id.clone(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: claimed.identity.runtime_host_generation,
                host_instance_id: claimed.identity.runtime_host_instance_id.clone(),
                fencing_token: claimed.identity.runtime_fencing_token,
            },
            key: f.key.clone(),
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::summary("resource settle", &[]).unwrap(),
            occurred_at: CLAIM_NOW + 20,
            committed_at: CLAIM_NOW + 20,
        },
    }
}

fn lifecycle_command(
    f: &ResourceFixture,
    identity: &ContinuationClaimIdentity,
    suffix: &str,
    trusted_now: i64,
    resolution: TechnicalResourceLifecycleResolution,
    actuals: Vec<TechnicalResourceActualInput>,
) -> TechnicalResourceLifecycleCommand {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (global_count, global_head, delegation_count, delegation_head, state_revision) =
        heads(&conn);
    let context = f
        .fixture
        .store
        .read_continuation_receipt_context(&identity.continuation_id, trusted_now)
        .unwrap()
        .unwrap();
    let event_id = format!("resource-lifecycle-event-{suffix}");
    let occurred_at = trusted_now + 1;
    let evidence = lifecycle_evidence(&conn, suffix);
    let evidence_digest =
        technical_resource_lifecycle_evidence_reference_digest(std::slice::from_ref(&evidence))
            .unwrap();
    TechnicalResourceLifecycleCommand {
        write: WriteContext {
            idempotency_key: format!("resource-lifecycle-idem-{suffix}"),
            correlation_id: format!("resource-lifecycle-corr-{suffix}"),
            causation_id: format!("resource-lifecycle-cause-{suffix}"),
            occurred_at,
        },
        identity: identity.clone(),
        trusted_now,
        resolution,
        evidence_digest,
        technical_resource_actuals: actuals,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
            aggregate_id: identity.delegation_id.clone(),
            aggregate_revision: state_revision,
            correlation_id: format!("resource-lifecycle-corr-{suffix}"),
            causation_id: format!("resource-lifecycle-cause-{suffix}"),
            occurred_at,
            safe_payload_json: "{}".into(),
            duplicate_identity: format!("resource-lifecycle-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("resource-lifecycle-receipt-{suffix}"),
            transaction_id: format!("resource-lifecycle-tx-{suffix}"),
            state_root_generation: context.state_root_generation,
            delegation_id: identity.delegation_id.clone(),
            expected_state_revision: state_revision,
            expected_global_count: global_count,
            expected_global_head_digest: global_head,
            expected_delegation_count: delegation_count,
            expected_delegation_head_digest: delegation_head,
            receipt_kind: ReceiptKind::Continuation,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Continuation,
                subject_id: identity.continuation_id.clone(),
                revision: state_revision,
            },
            causation_id: format!("resource-lifecycle-cause-{suffix}"),
            causation_event_id: event_id,
            actor: context.runtime_actor,
            runtime: ReceiptRuntimeBinding {
                host_generation: context.runtime_host_generation,
                host_instance_id: context.runtime_host_instance_id,
                fencing_token: context.runtime_fencing_token,
            },
            key: f.key.clone(),
            rotation: None,
            evidence: vec![evidence],
            redacted_summary: SafeText::summary("resource lifecycle", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    }
}

pub(super) fn acquire(f: &ResourceFixture, worker: &str, limit: u32) -> Vec<crate::JobRecord> {
    Storage::from_paths(&f.fixture.paths)
        .acquire_due_jobs(worker, CLAIM_NOW, LEASE_MS, limit)
        .unwrap()
}

#[test]
fn claim_reserves_all_exact_kinds_and_exact_replay_is_row_stable() {
    let f = setup(1, 100, 7);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "all-four",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };
    assert_eq!(claimed.technical_resource_reservations.len(), 4);
    assert_eq!(
        claimed
            .technical_resource_reservations
            .iter()
            .map(|r| (
                r.identity.technical_resource_kind.as_str(),
                r.identity.unit.as_str(),
                r.identity.amount_reserved
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "connector_calls",
                "connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                7
            ),
            (
                "resource_units",
                "resource:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                7
            ),
            ("time_ms", "ms", 7),
            ("tokens", "token", 7)
        ]
    );
    let ContinuationClaimOutcome::Replayed(replayed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("replay")
    };
    assert_eq!(
        replayed.technical_resource_reservations,
        claimed
            .technical_resource_reservations
            .iter()
            .map(|r| r.identity.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        table_count(&f.fixture.paths, "execass_technical_resource_reservations"),
        4
    );
}

#[test]
fn schema_rejects_second_independent_continuation_at_same_live_revision() {
    let f = setup(1, 1, 1);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    insert_action_branch(
        &conn,
        &ActionBranchRecord {
            action_id: "action-2".into(),
            delegation_id: "delegation-1".into(),
            action_revision: 2,
            target_delegation_revision: 1,
            target_plan_revision: 1,
            stop_epoch: 0,
            branch_kind: ActionBranchKind::Ordinary,
            status: ContinuationStatus::Runnable,
            action_summary: "second resource contender".into(),
            created_at: SCHEDULED_AT,
            updated_at: SCHEDULED_AT,
            terminal_at: None,
        },
    )
    .unwrap();
    let error = super::rows::insert_continuation(
        &conn,
        &ContinuationRecord {
            continuation_id: "continuation-2".into(),
            delegation_id: "delegation-1".into(),
            target_delegation_revision: 1,
            target_plan_revision: 1,
            action_id: "action-2".into(),
            branch_kind: ActionBranchKind::Ordinary,
            causation_kind: ContinuationCausationKind::Intake,
            causation_id: "resource-race".into(),
            status: ContinuationStatus::Runnable,
            job_id: None,
            lease_owner: None,
            lease_expires_at: None,
            fencing_token: 0,
            host_generation: 1,
            stop_epoch: 0,
            global_stop_epoch: 0,
            created_at: SCHEDULED_AT,
            updated_at: SCHEDULED_AT,
            completed_at: None,
        },
    )
    .expect_err("one live delegation revision cannot own two continuations");
    assert!(error
        .to_string()
        .contains("failed inserting initial ExecAss continuation"));
    assert_eq!(table_count(&f.fixture.paths, "execass_continuations"), 1);
}

#[test]
fn schema_capacity_guard_serializes_raw_last_unit_inserts_across_effects() {
    let f = setup(1, 1, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let claim = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "race-seed",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim)
        .unwrap()
    else {
        panic!("seed claim")
    };
    let expiry = lifecycle_command(
        &f,
        &claimed.identity,
        "race-seed-expiry",
        claimed.identity.job_lease_expires_at,
        TechnicalResourceLifecycleResolution::ExpireUndispatched,
        vec![],
    );
    assert!(matches!(
        f.fixture
            .store
            .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &expiry)
            .unwrap(),
        TechnicalResourceLifecycleOutcome::Applied(_)
    ));

    let snapshot_id = claimed
        .identity
        .technical_quota_snapshot_id
        .clone()
        .unwrap();
    let mut contenders = Vec::new();
    for revision in [2_i64, 3_i64] {
        let action_id = format!("race-action-{revision}");
        let continuation_id = format!("race-continuation-{revision}");
        let effect_id = format!("race-effect-{revision}");
        let requirement_set_id = format!("race-requirements-{revision}");
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        insert_action_branch(
            &conn,
            &ActionBranchRecord {
                action_id: action_id.clone(),
                delegation_id: "delegation-1".into(),
                action_revision: revision,
                target_delegation_revision: revision,
                target_plan_revision: 1,
                stop_epoch: 0,
                branch_kind: ActionBranchKind::Ordinary,
                status: ContinuationStatus::Runnable,
                action_summary: format!("parallel quota contender {revision}"),
                created_at: SCHEDULED_AT + revision,
                updated_at: SCHEDULED_AT + revision,
                terminal_at: None,
            },
        )
        .unwrap();
        super::rows::insert_continuation(
            &conn,
            &ContinuationRecord {
                continuation_id: continuation_id.clone(),
                delegation_id: "delegation-1".into(),
                target_delegation_revision: revision,
                target_plan_revision: 1,
                action_id: action_id.clone(),
                branch_kind: ActionBranchKind::Ordinary,
                causation_kind: ContinuationCausationKind::Intake,
                causation_id: format!("parallel-quota-race-{revision}"),
                status: ContinuationStatus::Runnable,
                job_id: None,
                lease_owner: None,
                lease_expires_at: None,
                fencing_token: 0,
                host_generation: 1,
                stop_epoch: 0,
                global_stop_epoch: 0,
                created_at: SCHEDULED_AT + revision,
                updated_at: SCHEDULED_AT + revision,
                completed_at: None,
            },
        )
        .unwrap();
        insert_planned_logical_effect(
            &conn,
            &PlannedLogicalEffectRecord {
                logical_effect_id: effect_id.clone(),
                delegation_id: "delegation-1".into(),
                continuation_id: continuation_id.clone(),
                action_kind:
                    LogicalEffectActionKind::ReadOnlyLocalInspectionAndBoundedReversibleLocalWork,
                operation_reversible: true,
                declared_recovery_safe_boundary: DeclaredRecoverySafeBoundary::IndependentAbsence,
                internal_idempotency_key: format!("race-effect-idem-{revision}"),
                provider_identity: None,
                provider_idempotency_key: None,
                reconciliation_key: None,
                manifest_digest: "sha256:manifest".into(),
                payload_digest: format!("sha256:race-payload-{revision}"),
                created_at: SCHEDULED_AT + revision,
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO execass_technical_resource_requirement_sets(requirement_set_id,quota_snapshot_id,delegation_id,logical_effect_id,action_id,manifest_digest,canonical_requirements_json,canonical_requirements_digest,created_at) VALUES(?1,?2,'delegation-1',?3,?4,'sha256:manifest','[]',?5,?6)",
            rusqlite::params![
                requirement_set_id,
                snapshot_id,
                effect_id,
                action_id,
                format!("sha256:race-requirements-{revision}"),
                SCHEDULED_AT + revision,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO execass_technical_resource_requirements(requirement_set_id,quota_snapshot_id,technical_resource_kind,unit,amount_required) VALUES(?1,?2,'tokens','token',1)",
            rusqlite::params![requirement_set_id, snapshot_id],
        )
        .unwrap();
        let job_id = format!("race-job-{revision}");
        let payload_json = serde_json::json!({
            "mode": "execass.continuation",
            "continuation_id": continuation_id,
            "delegation_id": "delegation-1",
            "action_id": action_id,
            "target_delegation_revision": revision,
            "target_plan_revision": 1,
            "branch_kind": "ordinary",
            "causation_kind": "intake",
            "causation_id": format!("parallel-quota-race-{revision}"),
        })
        .to_string();
        conn.execute(
            "INSERT INTO jobs(job_id,agent_id,name,enabled,schedule_kind,run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,created_at,updated_at) VALUES(?1,'default',?2,1,'execass_continuation',?3,?3,?4,0,1000,30000,?3,?3)",
            rusqlite::params![
                job_id,
                format!("raw schema capacity contender {revision}"),
                SCHEDULED_AT + revision,
                payload_json,
            ],
        )
        .unwrap();
        conn.execute(
            "UPDATE execass_continuations SET job_id=?1 WHERE continuation_id=?2 AND job_id IS NULL",
            rusqlite::params![job_id, continuation_id],
        )
        .unwrap();
        drop(conn);
        contenders.push((revision, action_id, continuation_id, effect_id));
    }

    let mut inserts = Vec::new();
    for (revision, action_id, continuation_id, effect_id) in contenders {
        let job_id = format!("race-job-{revision}");
        let event_id = format!("parallel-quota-claim-{revision}");
        let reservation = TechnicalResourceReservationIdentity {
            reservation_id: format!("parallel-quota-reservation-{revision}"),
            quota_snapshot_id: snapshot_id.clone(),
            logical_effect_id: effect_id.clone(),
            technical_resource_kind: "tokens".into(),
            unit: "token".into(),
            amount_reserved: 1,
        };
        let identity = ContinuationClaimIdentity {
            claim_event_id: event_id.clone(),
            claim_receipt_id: format!("parallel-quota-receipt-{revision}"),
            continuation_id: continuation_id.clone(),
            delegation_id: "delegation-1".into(),
            action_id,
            job_id,
            worker_id: format!("race-worker-{revision}"),
            job_lease_expires_at: CLAIM_NOW + LEASE_MS,
            continuation_fencing_token: 1,
            runtime_host_generation: claimed.identity.runtime_host_generation,
            runtime_host_instance_id: claimed.identity.runtime_host_instance_id.clone(),
            runtime_fencing_token: claimed.identity.runtime_fencing_token,
            state_root_generation: claimed.identity.state_root_generation,
            runtime_authority_provenance_id: claimed
                .identity
                .runtime_authority_provenance_id
                .clone(),
            runtime_actor_identity: claimed.identity.runtime_actor_identity.clone(),
            policy_revision: claimed.identity.policy_revision,
            global_stop_epoch: claimed.identity.global_stop_epoch,
            technical_quota_policy_digest: claimed.identity.technical_quota_policy_digest.clone(),
            technical_quota_snapshot_id: Some(snapshot_id.clone()),
            technical_resource_reservation_set_digest: resource_identity_set_digest(
                std::slice::from_ref(&reservation),
            )
            .unwrap(),
        };
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        insert_outbox(
            &conn,
            &NewOutboxEvent {
                event_id: event_id.clone(),
                event_name: OutboxEventName::ContinuationClaimedOrResultRecorded,
                aggregate_id: "delegation-1".into(),
                aggregate_revision: 1,
                correlation_id: format!("parallel-quota-corr-{revision}"),
                causation_id: format!("parallel-quota-cause-{revision}"),
                occurred_at: CLAIM_NOW + revision,
                safe_payload_json: "{}".into(),
                duplicate_identity: format!("parallel-quota-idem-{revision}"),
            },
        )
        .unwrap();
        let resource_json = serde_json::to_string(std::slice::from_ref(&reservation)).unwrap();
        insert_operation_history(
            &conn,
            OperationHistoryWrite {
                event_id: &event_id,
                operation: "claim",
                result_status: ContinuationStatus::Executing,
                identity: &identity,
                resource_set_json: &resource_json,
                resource_evidence_digest: None,
                recorded_at: CLAIM_NOW + revision,
            },
        )
        .unwrap();
        inserts.push((identity, reservation));
    }

    let barrier = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for (identity, reservation) in inserts {
        let db_path = f.fixture.paths.db_path.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let conn = open_sqlite_connection(&db_path).unwrap();
            conn.busy_timeout(Duration::from_secs(5)).unwrap();
            barrier.wait();
            conn.execute_batch("BEGIN DEFERRED").unwrap();
            let result = conn.execute(
                "INSERT INTO execass_technical_resource_reservations(reservation_id,delegation_id,logical_effect_id,quota_snapshot_id,continuation_id,claim_event_id,claim_receipt_id,technical_resource_kind,unit,amount_reserved,status,idempotency_key,continuation_fencing_token,runtime_host_generation,runtime_fencing_token,created_at,expires_at,settled_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'reserved',?11,?12,?13,?14,?15,?16,NULL)",
                rusqlite::params![
                    reservation.reservation_id,
                    identity.delegation_id,
                    reservation.logical_effect_id,
                    reservation.quota_snapshot_id,
                    identity.continuation_id,
                    identity.claim_event_id,
                    identity.claim_receipt_id,
                    reservation.technical_resource_kind,
                    reservation.unit,
                    reservation.amount_reserved,
                    format!("{}:{}", identity.claim_event_id, reservation.reservation_id),
                    identity.continuation_fencing_token,
                    identity.runtime_host_generation,
                    identity.runtime_fencing_token,
                    CLAIM_NOW + 100,
                    CLAIM_NOW + LEASE_MS,
                ],
            );
            match result {
                Ok(_) => {
                    conn.execute_batch("COMMIT").unwrap();
                    Ok(())
                }
                Err(error) => {
                    conn.execute_batch("ROLLBACK").unwrap();
                    Err(error.to_string())
                }
            }
        }));
    }
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE quota_snapshot_id=?1 AND technical_resource_kind='tokens' AND unit='token' AND status='reserved'",
            [&snapshot_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT SUM(amount_reserved) FROM execass_technical_resource_reservations WHERE quota_snapshot_id=?1 AND technical_resource_kind='tokens' AND unit='token' AND status='reserved'",
            [&snapshot_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
}

#[test]
fn parallel_canonical_claims_for_the_last_unit_leave_one_complete_winner_and_no_loser_artifacts() {
    // A canonical jobs lease has exactly one durable worker identity. Two
    // distinct worker identities cannot both be eligible by design, so this
    // races two independently opened worker processes holding the same exact
    // lease identity. Both requests are valid before the writer CAS; only the
    // canonical claim transaction may choose a winner.
    let f = setup(1, 1, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let commands = ["canonical-race-a", "canonical-race-b"].map(|suffix| {
        claim_command(
            &f,
            "continuation-1",
            &job.job_id,
            "worker-a",
            job.lease_expires_at.unwrap(),
            suffix,
        )
    });
    let loser_event_ids = commands
        .iter()
        .map(|command| command.outbox_event.event_id.clone())
        .collect::<Vec<_>>();
    let loser_receipt_ids = commands
        .iter()
        .map(|command| command.receipt.receipt_id.clone())
        .collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for command in commands {
        let store = f.fixture.store.clone();
        let integrity = ReceiptIntegrityStore::open(&f.fixture.paths).unwrap();
        let redactor = f.redactor.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            barrier.wait();
            store.claim_continuation_atomically(&integrity, &redactor, &command)
        }));
    }
    barrier.wait();
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap().unwrap())
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
            .filter(|outcome| matches!(
                outcome,
                ContinuationClaimOutcome::Lost { .. } | ContinuationClaimOutcome::Stale { .. }
            ))
            .count(),
        1
    );
    let winner = outcomes
        .iter()
        .find_map(|outcome| match outcome {
            ContinuationClaimOutcome::Claimed(record) => Some(record),
            _ => None,
        })
        .unwrap();
    assert_eq!(winner.technical_resource_reservations.len(), 4);
    assert!(winner
        .technical_resource_reservations
        .iter()
        .all(|reservation| reservation.identity.amount_reserved == 1));

    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_continuation_operation_history WHERE operation='claim'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE status='reserved'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        4
    );
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 1);
    let persisted_events = loser_event_ids
        .iter()
        .filter(|event_id| {
            conn.query_row(
                "SELECT COUNT(*) FROM execass_outbox_events WHERE event_id=?1",
                [event_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
                == 1
        })
        .count();
    let persisted_receipts = loser_receipt_ids
        .iter()
        .filter(|receipt_id| {
            conn.query_row(
                "SELECT COUNT(*) FROM execass_receipts WHERE receipt_id=?1",
                [receipt_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
                == 1
        })
        .count();
    assert_eq!(persisted_events, 1);
    assert_eq!(persisted_receipts, 1);
    assert_eq!(
        conn.query_row(
            "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "claimed"
    );
}

#[test]
fn pre_dispatch_rejects_expired_and_tampered_reservation_identity() {
    let f = setup(1, 10, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "dispatch",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: claimed.identity.clone(),
                trusted_now: claimed.identity.job_lease_expires_at
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::TechnicalReservationExpired
        }
    );
    let mut tampered = claimed.identity;
    tampered.technical_resource_reservation_set_digest = "sha256:tampered".into();
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: tampered,
                trusted_now: CLAIM_NOW + 1
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::ClaimIdentityMismatch
        }
    );
}

#[test]
fn successful_pre_dispatch_durably_marks_invoking_and_forbids_undispatched_expiry() {
    let f = setup(1, 10, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "dispatch-proof",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };
    let validation = ContinuationDispatchValidationCommand {
        identity: claimed.identity.clone(),
        trusted_now: CLAIM_NOW + 1,
    };
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&validation)
            .unwrap(),
        ContinuationDispatchValidationOutcome::Valid
    );
    assert_eq!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&validation)
            .unwrap(),
        ContinuationDispatchValidationOutcome::Valid
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "invoking"
    );
    drop(conn);
    let expiry = lifecycle_command(
        &f,
        &claimed.identity,
        "invoking-expiry-rejected",
        claimed.identity.job_lease_expires_at,
        TechnicalResourceLifecycleResolution::ExpireUndispatched,
        vec![],
    );
    assert!(f
        .fixture
        .store
        .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &expiry)
        .is_err());
}

#[test]
fn takeover_waiting_at_atomic_predispatch_boundary_returns_stale_without_invoking() {
    let f = setup(1, 10, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "atomic-boundary",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };

    let mut takeover = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let tx = takeover
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute(
        "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE released_at IS NULL",
        [CLAIM_NOW + 20],
    )
    .unwrap();
    tx.execute(
        "UPDATE execass_runtime_host_generations SET ended_at=?1,end_reason='boundary_takeover' WHERE ended_at IS NULL",
        [CLAIM_NOW + 20],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(2,'execass',1,'resource-installation','resource-user','resource-host-boundary',?1)",
        [CLAIM_NOW + 20],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('resource-host-boundary-lease','execass',2,'resource-host-boundary',2,?1,9999999999999)",
        [CLAIM_NOW + 20],
    )
    .unwrap();

    let (started_tx, started_rx) = mpsc::channel();
    let store = f.fixture.store.clone();
    let identity = claimed.identity.clone();
    let validator = thread::spawn(move || {
        started_tx.send(()).unwrap();
        store.validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
            identity,
            trusted_now: CLAIM_NOW + 21,
        })
    });
    started_rx.recv().unwrap();
    thread::sleep(Duration::from_millis(50));
    tx.commit().unwrap();

    assert_eq!(
        validator.join().unwrap().unwrap(),
        ContinuationDispatchValidationOutcome::Stale {
            reason: ContinuationStaleReason::RuntimeHostLeaseLostOrExpired
        }
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "claimed"
    );
}

#[test]
fn restart_recovery_marks_possible_invocation_unknown_and_prevents_second_dispatch() {
    let f = setup_recorder(1, 10, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let command = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "restart-recovery",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("claim")
    };
    let attempt = match f
        .fixture
        .store
        .prepare_provider_attempt(&PrepareProviderAttemptCommand {
            claim: claimed.identity.clone(),
            trusted_now: CLAIM_NOW + 1,
            retry_authorization: None,
        })
        .unwrap()
    {
        PrepareProviderAttemptOutcome::Prepared(attempt) => *attempt,
        other => panic!("restart recovery did not prepare an attempt: {other:?}"),
    };
    let attempt = match f
        .fixture
        .store
        .begin_provider_attempt_invocation(&BeginProviderAttemptInvocationCommand {
            attempt_id: attempt.attempt_id.clone(),
            claim: claimed.identity.clone(),
            trusted_now: CLAIM_NOW + 1,
        })
        .unwrap()
    {
        BeginProviderAttemptInvocationOutcome::Began(attempt) => *attempt,
        other => panic!("restart recovery did not begin the attempt: {other:?}"),
    };
    assert!(f
        .fixture
        .store
        .list_technical_resource_recovery_candidates(CLAIM_NOW + 2, 8)
        .unwrap()
        .is_empty());

    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE released_at IS NULL",
        [CLAIM_NOW + 3],
    )
    .unwrap();
    conn.execute(
        "UPDATE execass_runtime_host_generations SET ended_at=?1,end_reason='restart' WHERE ended_at IS NULL",
        [CLAIM_NOW + 3],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(2,'execass',1,'resource-installation','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','resource-host-restart',?1)",
        [CLAIM_NOW + 3],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('resource-host-restart-lease','execass',2,'resource-host-restart',2,?1,9999999999999)",
        [CLAIM_NOW + 3],
    )
    .unwrap();
    drop(conn);

    let candidates = f
        .fixture
        .store
        .list_technical_resource_recovery_candidates(CLAIM_NOW + 4, 8)
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].kind,
        TechnicalResourceRecoveryKind::RecoverPossiblyInvoked
    );
    assert_eq!(candidates[0].identity, claimed.identity);
    let recovery = lifecycle_command(
        &f,
        &candidates[0].identity,
        "restart-recovery",
        CLAIM_NOW + 4,
        TechnicalResourceLifecycleResolution::RecoverPossiblyInvoked,
        vec![],
    );
    assert!(
        f.fixture
            .store
            .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &recovery)
            .is_err(),
        "unsigned restart recovery selected an outcome_unknown result"
    );
    let recorder_command = signed_execution_command_for_attempt(
        &f,
        &claimed.identity,
        &attempt,
        RecorderObservationKindV1::Unknown,
        "restart-recovery-unknown",
        1,
        CLAIM_NOW + 4,
    );
    assert!(matches!(
        f.fixture
            .store
            .reconcile_recorder_evidence_atomically(&f.integrity, &f.redactor, &recorder_command,)
            .unwrap(),
        RecorderEvidenceImportOutcome::Applied(_)
    ));

    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "outcome_unknown"
    );
    assert_eq!(
        conn.query_row(
            "SELECT status||':'||provider_response_digest||':'||finished_at FROM execass_provider_attempts WHERE attempt_id=?1",
            [&attempt.attempt_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        format!(
            "outcome_unknown:{}:{}",
            "sha256:c31e28c49383c79f4a9e89a83d652a3ab0383b88f92309f7dd15f2fabdb41323",
            recorder_command.write.occurred_at
        )
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE status='reconciliation_required'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        4
    );
    assert_eq!(
        conn.query_row(
            "SELECT status||':'||COALESCE(lease_owner,'none') FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "uncertain:none"
    );
    assert_eq!(
        conn.query_row(
            "SELECT enabled||':'||COALESCE(lease_owner,'none') FROM jobs WHERE job_id=?1",
            [&claimed.identity.job_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "0:none"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_continuation_operation_history WHERE operation='settle' AND event_id=?1",
            [&recorder_command.outbox_event.event_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    drop(conn);
    assert!(f
        .fixture
        .store
        .list_technical_resource_recovery_candidates(CLAIM_NOW + 5, 8)
        .unwrap()
        .is_empty());
    assert!(matches!(
        f.fixture
            .store
            .validate_continuation_pre_dispatch(&ContinuationDispatchValidationCommand {
                identity: claimed.identity,
                trusted_now: CLAIM_NOW + 5,
            })
            .unwrap(),
        ContinuationDispatchValidationOutcome::Stale { .. }
    ));
    f.fixture
        .store
        .materialize_runnable_continuation_jobs(CLAIM_NOW + 5, 8)
        .unwrap();
    assert!(Storage::from_paths(&f.fixture.paths)
        .acquire_due_jobs("worker-after-restart", CLAIM_NOW + 5, LEASE_MS, 8)
        .unwrap()
        .is_empty());
}

#[test]
fn terminal_settlement_records_exact_actuals_once_releases_unused_and_replays() {
    let f = setup(1, 20, 10);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let claim = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "settle",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim)
        .unwrap()
    else {
        panic!("claim")
    };
    let actuals = claimed
        .technical_resource_reservations
        .iter()
        .enumerate()
        .map(|(i, r)| TechnicalResourceActualInput {
            reservation_id: r.identity.reservation_id.clone(),
            amount_actual: i as i64,
            evidence_digest: format!("sha256:evidence-{i}"),
        })
        .collect::<Vec<_>>();
    let command = settle_command(&f, &claimed, actuals.clone());
    let ContinuationSettleOutcome::Settled(settled) = f
        .fixture
        .store
        .settle_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("settle")
    };
    assert!(settled
        .technical_resource_reservations
        .iter()
        .all(|r| r.status == "settled"));
    let ContinuationSettleOutcome::Replayed(replayed) = f
        .fixture
        .store
        .settle_continuation_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("replay")
    };
    assert_eq!(
        replayed.technical_resource_reservations,
        settled
            .technical_resource_reservations
            .iter()
            .map(|r| r.identity.clone())
            .collect::<Vec<_>>()
    );
    let mut conflicting_replay = command.clone();
    conflicting_replay.technical_resource_actuals[0].amount_actual += 1;
    assert_eq!(
        f.fixture
            .store
            .settle_continuation_atomically(&f.integrity, &f.redactor, &conflicting_replay)
            .unwrap(),
        ContinuationSettleOutcome::Lost {
            reason: ContinuationStaleReason::ClaimIdentityMismatch
        }
    );
    assert_eq!(
        table_count(&f.fixture.paths, "execass_technical_resource_actuals"),
        4
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let sum: i64 = conn
        .query_row(
            "SELECT SUM(amount_actual) FROM execass_technical_resource_actuals",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sum, actuals.iter().map(|a| a.amount_actual).sum::<i64>());
    let reserved: i64 = conn
        .query_row(
            "SELECT SUM(amount_reserved) FROM execass_technical_resource_reservations",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reserved - sum, 34, "unused reserved capacity is released");
    assert!(conn
        .execute(
            "UPDATE execass_technical_resource_actuals SET amount_actual=99",
            []
        )
        .is_err());
    assert!(conn
        .execute("DELETE FROM execass_technical_resource_actuals", [])
        .is_err());
}

#[test]
fn expired_undispatched_reservations_transition_once_with_receipt_and_replay() {
    let f = setup(1, 10, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let claim = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "expire",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim)
        .unwrap()
    else {
        panic!("claim")
    };
    let command = lifecycle_command(
        &f,
        &claimed.identity,
        "expire",
        claimed.identity.job_lease_expires_at,
        TechnicalResourceLifecycleResolution::ExpireUndispatched,
        vec![],
    );
    let TechnicalResourceLifecycleOutcome::Applied(applied) = f
        .fixture
        .store
        .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("expiry")
    };
    assert!(applied
        .technical_resource_reservations
        .iter()
        .all(|reservation| reservation.status == "expired"));
    let TechnicalResourceLifecycleOutcome::Replayed(replayed) = f
        .fixture
        .store
        .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("expiry replay")
    };
    assert_eq!(applied, replayed);
    assert_eq!(
        table_count(&f.fixture.paths, "execass_technical_resource_actuals"),
        0
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_receipt_evidence(
        &conn,
        &applied.receipt.receipt_id,
        &command.receipt.evidence[0],
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_receipt_evidence_refs WHERE receipt_id=?1",
            [&applied.receipt.receipt_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "expiry replay must not append evidence rows"
    );
    let DelegationReachabilityOutcome::Valid(report) = f
        .fixture
        .store
        .validate_delegation_reachability("delegation-1")
        .unwrap()
    else {
        panic!("expiry authoritative evidence must remain lineage-reachable")
    };
    assert!(report.violations.is_empty());
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "superseded"
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
            "SELECT enabled FROM jobs WHERE job_id=?1",
            [&claimed.identity.job_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "failed"
    );
}

#[test]
fn caller_selected_reconciliation_is_rejected_in_favor_of_signed_recorder_evidence() {
    for present in [false, true] {
        let f = setup(1, 10, 4);
        let job = acquire(&f, "worker-a", 10).remove(0);
        let claim = claim_command(
            &f,
            "continuation-1",
            &job.job_id,
            "worker-a",
            job.lease_expires_at.unwrap(),
            if present { "present" } else { "absent" },
        );
        let ContinuationClaimOutcome::Claimed(claimed) = f
            .fixture
            .store
            .claim_continuation_atomically(&f.integrity, &f.redactor, &claim)
            .unwrap()
        else {
            panic!("claim")
        };
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        conn.execute(
            r#"INSERT INTO execass_provider_attempts(
                 attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,
                 claim_event_id,claim_receipt_id,attempt_number,fencing_token,host_generation,
                 host_instance_id,runtime_fencing_token,status,provider_request_digest,
                 provider_response_digest,remote_effect_id,started_at,finished_at
               ) VALUES(?1,?2,'effect-1',?3,?4,?5,?6,1,?7,?8,?9,?10,
                 'outcome_unknown',?11,?12,NULL,?13,?14)"#,
            params![
                format!(
                    "attempt-reconcile-{}",
                    if present { "present" } else { "absent" }
                ),
                claimed.identity.delegation_id,
                claimed.identity.continuation_id,
                claimed.identity.action_id,
                claimed.identity.claim_event_id,
                claimed.identity.claim_receipt_id,
                claimed.identity.continuation_fencing_token,
                claimed.identity.runtime_host_generation,
                claimed.identity.runtime_host_instance_id,
                claimed.identity.runtime_fencing_token,
                format!(
                    "sha256:provider-request-{}",
                    if present { "present" } else { "absent" }
                ),
                format!(
                    "sha256:provider-unknown-{}",
                    if present { "present" } else { "absent" }
                ),
                CLAIM_NOW + 1,
                CLAIM_NOW + 2,
            ],
        )
        .unwrap();
        drop(conn);
        let mut uncertain = settle_command(&f, &claimed, vec![]);
        uncertain.result_status = ContinuationStatus::Uncertain;
        let ContinuationSettleOutcome::Settled(uncertain_record) = f
            .fixture
            .store
            .settle_continuation_atomically(&f.integrity, &f.redactor, &uncertain)
            .unwrap()
        else {
            panic!("uncertain settlement")
        };
        assert!(uncertain_record
            .technical_resource_reservations
            .iter()
            .all(|reservation| reservation.status == "reconciliation_required"));
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "outcome_unknown"
        );
        drop(conn);
        let suffix = if present { "present" } else { "absent" };
        let actuals = if present {
            claimed
                .technical_resource_reservations
                .iter()
                .map(|reservation| TechnicalResourceActualInput {
                    reservation_id: reservation.identity.reservation_id.clone(),
                    amount_actual: 2,
                    evidence_digest: format!("sha256:resource-lifecycle-evidence-{suffix}"),
                })
                .collect()
        } else {
            vec![]
        };
        let mut command = lifecycle_command(
            &f,
            &claimed.identity,
            suffix,
            CLAIM_NOW + 30,
            if present {
                TechnicalResourceLifecycleResolution::ReconcilePresent
            } else {
                TechnicalResourceLifecycleResolution::ReconcileAbsent
            },
            actuals,
        );
        for actual in &mut command.technical_resource_actuals {
            actual.evidence_digest = command.evidence_digest.clone();
        }
        let reconciliation = f.fixture.store.resolve_technical_resources_atomically(
            &f.integrity,
            &f.redactor,
            &command,
        );
        let applied = match reconciliation {
            Err(error) => {
                assert!(error
                    .to_string()
                    .contains("verified recorder evidence is required"));
                let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
                assert_eq!(
                    conn.query_row(
                        "SELECT COUNT(*) FROM execass_effect_recorder_evidence",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                    0
                );
                continue;
            }
            Ok(TechnicalResourceLifecycleOutcome::Applied(applied)) => applied,
            Ok(other) => panic!("unexpected caller-selected reconciliation outcome: {other:?}"),
        };
        let expected_status = if present { "settled" } else { "released" };
        assert!(applied
            .technical_resource_reservations
            .iter()
            .all(|reservation| reservation.status == expected_status));
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        assert_receipt_evidence(
            &conn,
            &applied.receipt.receipt_id,
            &command.receipt.evidence[0],
        );
        let reachability = f
            .fixture
            .store
            .validate_delegation_reachability("delegation-1")
            .unwrap();
        let report = match reachability {
            DelegationReachabilityOutcome::Valid(report)
            | DelegationReachabilityOutcome::Invalid(report) => report,
            DelegationReachabilityOutcome::NotFound => panic!("delegation disappeared"),
        };
        assert!(report
            .violations
            .iter()
            .all(|violation| !violation.contains("authoritative_evidence_reference")));
        assert_eq!(
            conn.query_row(
                "SELECT state FROM execass_logical_effects WHERE logical_effect_id='effect-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            if present {
                "reconciled_present"
            } else {
                "reconciled_absent"
            }
        );
        assert_eq!(
            conn.query_row(
                "SELECT status FROM execass_provider_attempts WHERE logical_effect_id='effect-1' ORDER BY attempt_number DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            if present {
                "reconciled_present"
            } else {
                "reconciled_absent"
            }
        );
        assert_eq!(
            conn.query_row(
                "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            if present { "terminal" } else { "superseded" }
        );
        drop(conn);
        let TechnicalResourceLifecycleOutcome::Replayed(replayed) = f
            .fixture
            .store
            .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &command)
            .unwrap()
        else {
            panic!("reconciliation replay")
        };
        assert_eq!(applied, replayed);
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        let conflicting_evidence = lifecycle_evidence(&conn, &format!("{suffix}-conflict"));
        drop(conn);
        let mut conflicting_replay = command.clone();
        conflicting_replay.receipt.evidence = vec![conflicting_evidence];
        conflicting_replay.evidence_digest =
            technical_resource_lifecycle_evidence_reference_digest(
                &conflicting_replay.receipt.evidence,
            )
            .unwrap();
        assert_eq!(
            f.fixture
                .store
                .resolve_technical_resources_atomically(
                    &f.integrity,
                    &f.redactor,
                    &conflicting_replay
                )
                .unwrap(),
            TechnicalResourceLifecycleOutcome::Lost {
                reason: ContinuationStaleReason::ClaimIdentityMismatch
            }
        );
        assert_eq!(
            table_count(&f.fixture.paths, "execass_technical_resource_actuals"),
            if present { 4 } else { 0 }
        );
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM execass_receipt_evidence_refs WHERE receipt_id=?1",
                [&applied.receipt.receipt_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1,
            "replay/conflict must not append evidence rows"
        );
    }
}

#[test]
fn stale_runtime_cannot_expire_resources_after_host_takeover_but_current_host_can() {
    let f = setup(1, 10, 1);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let claim = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "takeover",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim)
        .unwrap()
    else {
        panic!("claim")
    };
    let stale = lifecycle_command(
        &f,
        &claimed.identity,
        "stale-expire",
        claimed.identity.job_lease_expires_at,
        TechnicalResourceLifecycleResolution::ExpireUndispatched,
        vec![],
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE released_at IS NULL",
        [CLAIM_NOW + 100],
    )
    .unwrap();
    conn.execute(
        "UPDATE execass_runtime_host_generations SET ended_at=?1,end_reason='takeover' WHERE ended_at IS NULL",
        [CLAIM_NOW + 100],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(2,'execass',1,'resource-installation','resource-user','resource-host-2',?1)",
        [CLAIM_NOW + 100],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('resource-host-lease-2','execass',2,'resource-host-2',2,?1,9999999999999)",
        [CLAIM_NOW + 100],
    )
    .unwrap();
    drop(conn);
    assert!(f
        .fixture
        .store
        .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &stale)
        .is_err());
    let current = lifecycle_command(
        &f,
        &claimed.identity,
        "current-expire",
        claimed.identity.job_lease_expires_at,
        TechnicalResourceLifecycleResolution::ExpireUndispatched,
        vec![],
    );
    let TechnicalResourceLifecycleOutcome::Applied(applied) = f
        .fixture
        .store
        .resolve_technical_resources_atomically(&f.integrity, &f.redactor, &current)
        .unwrap()
    else {
        panic!("current host expiry")
    };
    assert!(applied
        .technical_resource_reservations
        .iter()
        .all(|reservation| reservation.status == "expired"));
}

#[test]
fn invalid_terminal_actual_rolls_back_every_resource_and_receipt_mutation() {
    let f = setup(1, 10, 4);
    let job = acquire(&f, "worker-a", 10).remove(0);
    let claim = claim_command(
        &f,
        "continuation-1",
        &job.job_id,
        "worker-a",
        job.lease_expires_at.unwrap(),
        "rollback",
    );
    let ContinuationClaimOutcome::Claimed(claimed) = f
        .fixture
        .store
        .claim_continuation_atomically(&f.integrity, &f.redactor, &claim)
        .unwrap()
    else {
        panic!("claim")
    };
    let actuals = claimed
        .technical_resource_reservations
        .iter()
        .map(|reservation| TechnicalResourceActualInput {
            reservation_id: reservation.identity.reservation_id.clone(),
            amount_actual: reservation.identity.amount_reserved + 1,
            evidence_digest: "sha256:invalid-overage".into(),
        })
        .collect();
    let settle = settle_command(&f, &claimed, actuals);
    assert!(f
        .fixture
        .store
        .settle_continuation_atomically(&f.integrity, &f.redactor, &settle)
        .is_err());
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_technical_resource_reservations WHERE status='reserved'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        4
    );
    assert_eq!(
        table_count(&f.fixture.paths, "execass_technical_resource_actuals"),
        0
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
            "SELECT COUNT(*) FROM execass_outbox_events WHERE event_id='resource-settle-event'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
}
