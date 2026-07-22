use super::tests::{
    admitted_dispatch_with_authority, fixture, foundation, prepared_attested_confirmation,
    prepared_combined_attested_confirmation, ready_manifest, table_count, test_authority_input,
    Fixture,
};
use super::*;
use crate::open_sqlite_connection;
use carsinos_core::execass_actor::{issue_test_local_owner_authority, VerifiedOwnerAuthority};
use rusqlite::{params, types::ValueRef, Connection};
use sha2::{Digest, Sha256};

#[cfg(feature = "execass-test-confirmation-runtime")]
#[test]
fn exact_overwrite_preparation_is_storage_derived_payload_bound_and_resource_backed() {
    let fixture = fixture();
    let expected_preimage = format!("sha256:{:x}", Sha256::digest(b"old bytes"));
    let replacement = b"new bytes";
    let replacement_digest = format!("sha256:{:x}", Sha256::digest(replacement));
    fixture
        .store
        .prepare_test_exact_overwrite_confirmation_runtime_projection(
            "exact-overwrite-decision",
            "exact-overwrite-action",
            r"Z:\carsinos\.tmp-exact-overwrite-target.txt",
            "windows:file-id:exact-overwrite-test",
            &expected_preimage,
            &replacement
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            &replacement_digest,
            1_800_000_000_010,
            1_800_000_000_110,
        )
        .unwrap();
    let prepared = fixture
        .store
        .prepare_exact_dangerous_effect(
            "exact-overwrite-decision",
            "exact-overwrite-action",
            1_800_000_000_020,
            1,
            0,
        )
        .unwrap()
        .expect("installed exact leaf must prepare one effect");
    let replay = fixture
        .store
        .prepare_exact_dangerous_effect(
            "exact-overwrite-decision",
            "exact-overwrite-action",
            1_800_000_000_020,
            1,
            0,
        )
        .unwrap()
        .expect("exact preparation must replay deterministically");
    assert_eq!(prepared, replay);
    assert_eq!(
        prepared.logical_effect.provider_identity.as_deref(),
        Some("carsinos.local-fs.exact-overwrite")
    );
    assert!(prepared.logical_effect.provider_idempotency_key.is_none());
    assert!(prepared
        .logical_effect
        .payload_digest
        .starts_with("sha256:"));
    assert_eq!(prepared.technical_quota_snapshot.entries.len(), 1);
    assert_eq!(prepared.technical_quota_snapshot.entries[0].limit, 1);
    assert_eq!(
        prepared.technical_resource_requirements.requirements.len(),
        1
    );
    assert_eq!(
        prepared.technical_resource_requirements.requirements[0].amount,
        1
    );
    let reconciliation = prepared
        .logical_effect
        .reconciliation_key
        .as_deref()
        .expect("exact effect requires reconciliation");
    assert!(reconciliation.contains("replacement_sha256"));
    assert!(!reconciliation.contains("replacement_hex"));
    assert!(!reconciliation.contains(
        &replacement
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ));

    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let confirmation_payload_digest: String = connection
        .query_row(
            "SELECT payload_digest FROM execass_confirmation_challenge_alternatives WHERE logical_action_id='exact-overwrite-action'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(
        confirmation_payload_digest, prepared.logical_effect.payload_digest,
        "manifest-leaf and recorder-envelope digest domains must remain distinct"
    );
    assert_eq!(table_count(&fixture.paths, "execass_logical_effects"), 0);
    assert!(
        fixture
            .store
            .read_exact_dangerous_effect_execution_material(
                &prepared.logical_effect.delegation_id,
                &prepared.continuation.continuation_id,
            )
            .unwrap()
            .is_none(),
        "pending confirmation must not expose recorder operands"
    );
}

pub(super) struct AtomicFixture {
    pub(super) fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    redactor: ReceiptRedactor,
    key: ReceiptKeyRef,
    authority: VerifiedOwnerAuthority,
    decision_id: String,
    action_id: String,
    manifest_digest: String,
}

fn setup(kind: DecisionKind, suffix: &str) -> AtomicFixture {
    let fixture = fixture();
    let mut base = foundation();
    base.initial_continuation = None;
    fixture.store.create_foundation(&base).unwrap();
    let action_id = format!("atomic-action-{suffix}");
    let predecessor_action_id = format!("atomic-predecessor-action-{suffix}");
    let confirmed_logical_action_identity = if kind == DecisionKind::DuplicateRiskRetry {
        predecessor_action_id.clone()
    } else {
        action_id.clone()
    };
    let decision_id = format!("atomic-decision-{suffix}");
    let manifest = ready_manifest(&admitted_dispatch_with_authority(&format!(
        "atomic-manifest-{suffix}"
    )));
    let manifest_digest = manifest.canonical().digest().as_hex().to_string();
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'atomic-installation','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','atomic-host',1);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('atomic-lease','execass',1,'atomic-host',1,1,9999999999999);
            "#,
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_action_branches(
              action_id,delegation_id,action_revision,target_delegation_revision,
              target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
            ) VALUES(?1,'delegation-1',1,1,1,0,'ordinary','waiting','atomic target',1800000000010,1800000000010,NULL)"#,
            params![action_id],
        )
        .unwrap();
    if kind == DecisionKind::DuplicateRiskRetry {
        connection
            .execute(
                r#"INSERT INTO execass_action_branches(
                  action_id,delegation_id,action_revision,target_delegation_revision,
                  target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
                ) VALUES(?1,'delegation-1',2,1,1,0,'ordinary','uncertain','uncertain predecessor',1800000000001,1800000000001,NULL)"#,
                params![predecessor_action_id],
            )
            .unwrap();
    }
    connection
        .execute(
            r#"INSERT INTO execass_decisions(
              decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,
              decision_kind,status,result,exact_presented_action_json,confirmed_logical_action_identity,
              manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,
              connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,
              consequence,alternatives_json,idempotency_key,requested_at,resolved_at,
              resolved_by_authority_provenance_id
            ) VALUES(?1,'delegation-1',1,1,1,1,?2,'pending',NULL,?3,?4,?5,?6,'{}','[]',
              'atomic.tool','1.0.0','{}','choose the exact action','the exact action will proceed',
              '["confirm_and_continue","revise","decline","stop"]',?7,1800000000010,NULL,NULL)"#,
            params![
                decision_id,
                kind.as_str(),
                format!(r#"{{"action_id":"{action_id}"}}"#),
                confirmed_logical_action_identity,
                manifest_digest,
                "a".repeat(64),
                format!("atomic-presentation-{suffix}"),
            ],
        )
        .unwrap();
    if kind == DecisionKind::DuplicateRiskRetry {
        let (claim_event_id, runtime_authority_provenance_id): (String, String) = connection
            .query_row(
                r#"SELECT o.event_id,d.authority_provenance_id
                   FROM execass_outbox_events o
                   JOIN execass_delegations d ON d.delegation_id='delegation-1'
                   WHERE o.aggregate_id='delegation-1'
                   ORDER BY o.global_sequence LIMIT 1"#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        connection
            .execute(
                r#"INSERT INTO jobs(job_id,agent_id,name,enabled,schedule_kind,interval_seconds,
                  run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,
                  lease_owner,lease_expires_at,last_run_at,last_error,created_at,updated_at,deleted_at)
                  SELECT ?1,agent_id,'duplicate-risk predecessor',0,'at',NULL,NULL,NULL,'{}',0,0,1000,
                    NULL,NULL,NULL,NULL,1800000000001,1800000000001,NULL FROM agents ORDER BY agent_id LIMIT 1"#,
                params![format!("atomic-predecessor-job-{suffix}")],
            )
            .unwrap();
        connection
            .execute(
                r#"INSERT INTO execass_continuations(
                  continuation_id,delegation_id,target_delegation_revision,target_plan_revision,
                  action_id,branch_kind,causation_kind,causation_id,status,job_id,lease_owner,
                  lease_expires_at,fencing_token,host_generation,stop_epoch,global_stop_epoch,
                  created_at,updated_at,completed_at
                ) VALUES(?1,'delegation-1',1,1,?2,'ordinary','action_result',?3,'uncertain',?4,
                  NULL,NULL,1,1,0,0,1800000000001,1800000000001,NULL)"#,
                params![
                    format!("atomic-predecessor-continuation-{suffix}"),
                    predecessor_action_id,
                    format!("atomic-predecessor-causation-{suffix}"),
                    format!("atomic-predecessor-job-{suffix}"),
                ],
            )
            .unwrap();
        connection
            .execute(
                r#"INSERT INTO execass_logical_effects(
                  logical_effect_id,delegation_id,continuation_id,action_kind,state,
                  internal_idempotency_key,provider_identity,provider_idempotency_key,
                  reconciliation_key,manifest_digest,payload_digest,outcome_json,created_at,updated_at
                ) VALUES(?1,'delegation-1',?2,'public_or_externally_consequential_communication',
                  'invoking',?3,'atomic-provider',?4,?5,?6,?7,NULL,1800000000001,1800000000002)"#,
                params![
                    format!("atomic-predecessor-effect-{suffix}"),
                    format!("atomic-predecessor-continuation-{suffix}"),
                    format!("atomic-predecessor-internal-{suffix}"),
                    format!("atomic-predecessor-provider-{suffix}"),
                    format!("atomic-predecessor-reconcile-{suffix}"),
                    manifest_digest,
                    "a".repeat(64),
                ],
            )
            .unwrap();
        connection
            .execute(
                r#"INSERT INTO execass_continuation_operation_history(
                  event_id,claim_event_id,claim_receipt_id,operation,result_status,continuation_id,
                  delegation_id,action_id,job_id,worker_id,job_lease_expires_at,
                  continuation_fencing_token,runtime_host_generation,runtime_host_instance_id,
                  runtime_fencing_token,state_root_generation,runtime_authority_provenance_id,
                  runtime_actor_identity,policy_revision,global_stop_epoch,technical_quota_policy_digest,
                  technical_quota_snapshot_id,technical_resource_reservation_set_json,
                  technical_resource_reservation_set_digest,technical_resource_evidence_digest,recorded_at
                ) VALUES(?1,?1,?2,'claim','executing',?3,'delegation-1',?4,?5,'atomic-worker',
                  1800000010000,1,1,'atomic-host',1,1,?6,'atomic-user',1,0,?7,NULL,'[]',?8,NULL,1800000000002)"#,
                params![
                    claim_event_id,
                    format!("atomic-predecessor-claim-receipt-{suffix}"),
                    format!("atomic-predecessor-continuation-{suffix}"),
                    predecessor_action_id,
                    format!("atomic-predecessor-job-{suffix}"),
                    runtime_authority_provenance_id,
                    format!("sha256:atomic-quota-{suffix}"),
                    format!("sha256:atomic-reservations-{suffix}"),
                ],
            )
            .unwrap();
        connection
            .execute(
                r#"INSERT INTO execass_provider_attempts(
                  attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,
                  claim_event_id,claim_receipt_id,attempt_number,fencing_token,host_generation,
                  host_instance_id,runtime_fencing_token,status,provider_request_digest,
                  provider_response_digest,remote_effect_id,started_at,finished_at
                ) VALUES(?1,'delegation-1',?2,?3,?4,?5,?6,1,1,1,'atomic-host',1,
                  'invoking',?7,NULL,NULL,1800000000002,NULL)"#,
                params![
                    format!("atomic-predecessor-attempt-{suffix}"),
                    format!("atomic-predecessor-effect-{suffix}"),
                    format!("atomic-predecessor-continuation-{suffix}"),
                    predecessor_action_id,
                    claim_event_id,
                    format!("atomic-predecessor-claim-receipt-{suffix}"),
                    format!(
                        "sha256:{:x}",
                        Sha256::digest(format!("atomic-predecessor-request-{suffix}"))
                    ),
                ],
            )
            .unwrap();
        seed_signed_execution_unknown_fixture(
            &connection,
            &format!("atomic-predecessor-attempt-{suffix}"),
            &format!("atomic-predecessor-effect-{suffix}"),
            1_800_000_000_003,
        )
        .unwrap();
    }
    drop(connection);
    if kind == DecisionKind::DuplicateRiskRetry {
        fixture
            .store
            .bind_duplicate_risk_predecessor(&decision_id, 1_800_000_000_015)
            .unwrap()
            .expect("duplicate-risk predecessor binding");
    }
    let mut authority_input = test_authority_input(&format!("atomic-resolution-{suffix}"));
    authority_input.request_correlation_id = format!("atomic-correlation-{suffix}");
    authority_input.source_message_id = Some(format!("atomic-message-{suffix}"));
    authority_input.authority_kind = "decision_resolution".into();
    authority_input.bound_decision_id = Some(decision_id.clone());
    authority_input.bound_decision_revision = Some(1);
    authority_input.bound_manifest_bytes = Some(manifest.canonical().bytes().to_vec());
    authority_input.challenge_nonce_bytes =
        Some(format!("atomic-presentation-{suffix}").into_bytes());
    authority_input.created_at = 1_800_000_000_020;
    let authority = issue_test_local_owner_authority(authority_input).unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key(&format!("atomic-receipt-key-{suffix}"))
        .unwrap();
    AtomicFixture {
        fixture,
        integrity,
        redactor: ReceiptRedactor::new(&["atomic-fixture-secret"]).unwrap(),
        key,
        authority,
        decision_id,
        action_id,
        manifest_digest,
    }
}

fn command(
    fixture: &AtomicFixture,
    result: DecisionResult,
    suffix: &str,
) -> AtomicDecisionResolutionCommand {
    let occurred_at = 1_800_000_000_030;
    let canonical =
        carsinos_core::execass_manifest::canonicalize_owner_authority(&fixture.authority).unwrap();
    let authority = super::foundation::authority_record_from_manifest(&canonical).unwrap();
    let continuation = (result == DecisionResult::ConfirmAndContinue).then(|| ContinuationRecord {
        continuation_id: format!("atomic-continuation-{suffix}"),
        delegation_id: "delegation-1".into(),
        target_delegation_revision: 1,
        target_plan_revision: 1,
        action_id: fixture.action_id.clone(),
        branch_kind: ActionBranchKind::Ordinary,
        causation_kind: ContinuationCausationKind::Decision,
        causation_id: fixture.decision_id.clone(),
        status: ContinuationStatus::Runnable,
        job_id: None,
        lease_owner: None,
        lease_expires_at: None,
        fencing_token: 0,
        host_generation: 1,
        stop_epoch: 0,
        global_stop_epoch: 0,
        created_at: occurred_at,
        updated_at: occurred_at,
        completed_at: None,
    });
    let is_duplicate = Connection::open(&fixture.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT decision_kind='duplicate_risk_retry' FROM execass_decisions WHERE decision_id=?1",
            params![fixture.decision_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap();
    let logical_effect =
        (is_duplicate && result == DecisionResult::ConfirmAndContinue).then(|| {
            PlannedLogicalEffectRecord {
                logical_effect_id: format!("atomic-effect-{suffix}"),
                delegation_id: "delegation-1".into(),
                continuation_id: format!("atomic-continuation-{suffix}"),
                action_kind: LogicalEffectActionKind::PublicOrExternallyConsequentialCommunication,
                operation_reversible: false,
                declared_recovery_safe_boundary: DeclaredRecoverySafeBoundary::IndependentAbsence,
                internal_idempotency_key: format!("atomic-effect-idem-{suffix}"),
                provider_identity: Some("atomic-provider".into()),
                provider_idempotency_key: Some(format!("atomic-provider-idem-{suffix}")),
                reconciliation_key: Some(format!("atomic-reconcile-{suffix}")),
                manifest_digest: fixture.manifest_digest.clone(),
                payload_digest: "a".repeat(64),
                created_at: occurred_at,
            }
        });
    let technical_quota_snapshot = logical_effect.as_ref().map(|effect| {
        let authority_json: String = Connection::open(&fixture.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT effective_authority_json FROM execass_delegations WHERE delegation_id='delegation-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let authority_digest =
            carsinos_core::execass_policy::technical_effective_authority_digest(&authority_json)
                .unwrap();
        carsinos_core::execass_policy::compile_technical_quota_snapshot(
            &effect.delegation_id,
            1,
            &authority_digest,
            "delegation",
            vec![
                carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                    kind: carsinos_core::execass_policy::TechnicalResourceKind::ResourceUnits,
                    unit: format!("resource:{}", "b".repeat(64)),
                    limit: 5,
                },
                carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                    kind: carsinos_core::execass_policy::TechnicalResourceKind::ConnectorCalls,
                    unit: format!("connector:{}", "a".repeat(64)),
                    limit: 10,
                },
                carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                    kind: carsinos_core::execass_policy::TechnicalResourceKind::TimeMs,
                    unit: "ms".into(),
                    limit: 30_000,
                },
                carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                    kind: carsinos_core::execass_policy::TechnicalResourceKind::Tokens,
                    unit: "token".into(),
                    limit: 1_000,
                },
            ],
        )
        .unwrap()
    });
    let technical_resource_requirements = logical_effect
        .as_ref()
        .zip(technical_quota_snapshot.as_ref())
        .map(|(effect, snapshot)| {
            carsinos_core::execass_policy::compile_technical_resource_requirements(
                snapshot,
                &effect.logical_effect_id,
                &fixture.action_id,
                &effect.manifest_digest,
                vec![
                    carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                        kind: carsinos_core::execass_policy::TechnicalResourceKind::ResourceUnits,
                        unit: format!("resource:{}", "b".repeat(64)),
                        amount: 2,
                    },
                    carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                        kind: carsinos_core::execass_policy::TechnicalResourceKind::ConnectorCalls,
                        unit: format!("connector:{}", "a".repeat(64)),
                        amount: 3,
                    },
                    carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                        kind: carsinos_core::execass_policy::TechnicalResourceKind::TimeMs,
                        unit: "ms".into(),
                        amount: 4_000,
                    },
                    carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                        kind: carsinos_core::execass_policy::TechnicalResourceKind::Tokens,
                        unit: "token".into(),
                        amount: 100,
                    },
                ],
            )
            .unwrap()
        });
    let event_id = format!("atomic-event-{suffix}");
    AtomicDecisionResolutionCommand {
        write: WriteContext {
            idempotency_key: format!("atomic-resolution-idem-{suffix}"),
            correlation_id: format!("atomic-correlation-{suffix}"),
            causation_id: fixture.decision_id.clone(),
            occurred_at,
        },
        decision_id: fixture.decision_id.clone(),
        decision_revision: 1,
        result,
        selected_logical_action_id: continuation.as_ref().map(|_| fixture.action_id.clone()),
        continuation,
        logical_effect,
        technical_quota_snapshot,
        technical_resource_requirements,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::DecisionRecorded,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 1,
            correlation_id: format!("atomic-correlation-{suffix}"),
            causation_id: fixture.decision_id.clone(),
            occurred_at,
            safe_payload_json: format!(
                r#"{{"decision_id":"{}","result":"{}"}}"#,
                fixture.decision_id,
                result.as_str()
            ),
            duplicate_identity: format!("atomic-resolution-idem-{suffix}"),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("atomic-receipt-{suffix}"),
            transaction_id: format!("atomic-anchor-{suffix}"),
            state_root_generation: 1,
            delegation_id: "delegation-1".into(),
            expected_state_revision: 1,
            expected_global_count: 0,
            expected_global_head_digest: None,
            expected_delegation_count: 0,
            expected_delegation_head_digest: None,
            receipt_kind: ReceiptKind::Decision,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Decision,
                subject_id: fixture.decision_id.clone(),
                revision: 1,
            },
            causation_id: fixture.decision_id.clone(),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: authority.actor_type,
                actor_identity: SafeText::new(&authority.credential_identity, &[]).unwrap(),
                authority_provenance_id: authority.authority_provenance_id,
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: "atomic-host".into(),
                fencing_token: 1,
            },
            key: fixture.key.clone(),
            rotation: None,
            evidence: Vec::new(),
            redacted_summary: SafeText::new("owner decision recorded", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    }
}

/// Creates a durable dangerous-action grant through the same atomically
/// receipt-anchored storage entrypoint used by the runtime adapter.  The
/// duplicate-risk preservation proof deliberately starts from this produced
/// grant instead of inserting a test-shaped grant around the database guards.
fn accepted_dangerous_grant_fixture() -> (
    Fixture,
    ReceiptIntegrityStore,
    ReceiptRedactor,
    ReceiptKeyRef,
    String,
) {
    let (fixture, confirmation, attestation, _) = prepared_attested_confirmation();
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'danger-installation','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','danger-host',1);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('danger-lease','execass',1,'danger-host',1,1,9999999999999);
            "#,
        )
        .unwrap();
    let (delegation_id, delegation_revision): (String, i64) = connection
        .query_row(
            "SELECT delegation_id,delegation_revision FROM execass_decisions WHERE decision_id=?1",
            params![confirmation.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("grant-preservation-danger-receipt-key")
        .unwrap();
    let event_id = "grant-preservation-danger-event".to_string();
    let occurred_at = attestation.payload.issued_at_ms as i64;
    let resolution = AtomicDecisionResolutionCommand {
        write: WriteContext {
            idempotency_key: "grant-preservation-danger-resolution".into(),
            correlation_id: "grant-preservation-danger-correlation".into(),
            causation_id: confirmation.decision_id.clone(),
            occurred_at,
        },
        decision_id: confirmation.decision_id.clone(),
        decision_revision: confirmation.decision_revision,
        result: DecisionResult::ConfirmAndContinue,
        selected_logical_action_id: Some(confirmation.selected_logical_action_id.clone()),
        continuation: None,
        logical_effect: None,
        technical_quota_snapshot: None,
        technical_resource_requirements: None,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::DecisionRecorded,
            aggregate_id: delegation_id.clone(),
            aggregate_revision: delegation_revision,
            correlation_id: "grant-preservation-danger-correlation".into(),
            causation_id: confirmation.decision_id.clone(),
            occurred_at,
            safe_payload_json: r#"{"result":"confirm_and_continue"}"#.into(),
            duplicate_identity: "grant-preservation-danger-resolution".into(),
        },
        receipt: AppendReceiptCommand {
            receipt_id: "grant-preservation-danger-receipt".into(),
            transaction_id: "grant-preservation-danger-anchor".into(),
            state_root_generation: 1,
            delegation_id,
            expected_state_revision: delegation_revision,
            expected_global_count: 0,
            expected_global_head_digest: None,
            expected_delegation_count: 0,
            expected_delegation_head_digest: None,
            receipt_kind: ReceiptKind::Decision,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Decision,
                subject_id: confirmation.decision_id.clone(),
                revision: confirmation.decision_revision,
            },
            causation_id: confirmation.decision_id.clone(),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: ActorType::HumanLocal,
                actor_identity: SafeText::new("storage-derived", &[]).unwrap(),
                authority_provenance_id: "storage-derived".into(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: "danger-host".into(),
                fencing_token: 1,
            },
            key: key.clone(),
            rotation: None,
            evidence: Vec::new(),
            redacted_summary: SafeText::new("dangerous owner decision recorded", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    };
    let redactor = ReceiptRedactor::new(&["grant-preservation-danger-secret"]).unwrap();
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_attested_atomically_at_for_test(
                &integrity,
                &redactor,
                &resolution,
                &confirmation.grant_id,
                &attestation,
                occurred_at,
            ),
        Ok(AtomicDecisionResolutionOutcome::Applied(_))
    ));
    (fixture, integrity, redactor, key, confirmation.decision_id)
}

#[derive(Debug, PartialEq, Eq)]
struct DangerousConfirmationSqliteSnapshot {
    typed_rows: Vec<u8>,
    typed_rows_sha256: [u8; 32],
    grants: i64,
    challenges: i64,
    alternatives: i64,
    attestations: i64,
    dangerous_decisions: i64,
    all_grants: i64,
    all_challenges: i64,
    all_alternatives: i64,
    all_attestations: i64,
}

fn append_sqlite_typed_value(snapshot: &mut Vec<u8>, value: ValueRef<'_>) {
    let (type_tag, bytes): (&[u8], Vec<u8>) = match value {
        ValueRef::Null => (b"null", Vec::new()),
        ValueRef::Integer(value) => (b"integer", value.to_le_bytes().to_vec()),
        ValueRef::Real(value) => (b"real", value.to_bits().to_le_bytes().to_vec()),
        ValueRef::Text(value) => (b"text", value.to_vec()),
        ValueRef::Blob(value) => (b"blob", value.to_vec()),
    };
    snapshot.extend_from_slice(&(type_tag.len() as u64).to_le_bytes());
    snapshot.extend_from_slice(type_tag);
    snapshot.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    snapshot.extend_from_slice(&bytes);
}

fn append_sqlite_typed_query_snapshot(
    snapshot: &mut Vec<u8>,
    connection: &Connection,
    label: &str,
    sql: &str,
    decision_id: &str,
) {
    append_sqlite_typed_value(snapshot, ValueRef::Text(label.as_bytes()));
    let mut statement = connection.prepare(sql).unwrap();
    let column_count = statement.column_count();
    let mut rows = statement.query(params![decision_id]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        append_sqlite_typed_value(snapshot, ValueRef::Text(b"row"));
        for index in 0..column_count {
            append_sqlite_typed_value(snapshot, row.get_ref(index).unwrap());
        }
    }
    append_sqlite_typed_value(snapshot, ValueRef::Text(b"end"));
}

fn dangerous_confirmation_sqlite_snapshot(
    paths: &crate::AppPaths,
    decision_id: &str,
) -> DangerousConfirmationSqliteSnapshot {
    let connection = Connection::open(&paths.db_path).unwrap();
    let mut typed_rows = Vec::new();
    for (label, sql) in [
        (
            "grant",
            "SELECT grant_id,delegation_id,decision_id,confirmed_logical_action_identity,canonical_action_envelope_or_selector_json,payload_and_material_operands_json,payload_and_material_operands_digest,connector_tool_identity,connector_tool_version,declared_consequence,accepted_by_authority_provenance_id,confirmation_attestation_digest,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id FROM execass_accepted_confirmation_grants WHERE decision_id=?1 ORDER BY grant_id",
        ),
        (
            "challenge",
            "SELECT challenge_id,decision_id,delegation_id,decision_revision,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence,selected_logical_action_id,nonce_digest,status,created_at,expires_at,resolved_at FROM execass_confirmation_challenges WHERE decision_id=?1 ORDER BY challenge_id",
        ),
        (
            "alternative",
            "SELECT alternative.challenge_id,alternative.logical_action_id,alternative.exact_presented_action_json,alternative.confirmed_logical_action_identity,alternative.manifest_digest,alternative.payload_digest,alternative.payload_and_material_operands_json,alternative.target_audience_path_json,alternative.connector_tool_identity,alternative.connector_tool_version,alternative.canonical_action_envelope_or_selector_json,alternative.declared_consequence FROM execass_confirmation_challenge_alternatives alternative JOIN execass_confirmation_challenges challenge ON challenge.challenge_id=alternative.challenge_id WHERE challenge.decision_id=?1 ORDER BY alternative.challenge_id,alternative.logical_action_id",
        ),
        (
            "attestation",
            "SELECT attestation_digest,decision_id,authority_provenance_id,pinned_key_id,pinned_key_generation,actor_type,credential_identity,authenticated_ingress,channel_assurance,request_correlation_id,source_message_id,provider_event_id,selected_logical_action_id,signed_payload_json,signature_hex,issued_at,expires_at,verified_at FROM execass_confirmation_attestations WHERE decision_id=?1 ORDER BY attestation_digest",
        ),
        (
            "decision",
            "SELECT decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,decision_kind,status,result,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,consequence,alternatives_json,idempotency_key,requested_at,resolved_at,resolved_by_authority_provenance_id FROM execass_decisions WHERE decision_id=?1 ORDER BY decision_id",
        ),
    ] {
        append_sqlite_typed_query_snapshot(&mut typed_rows, &connection, label, sql, decision_id);
    }
    let count = |sql: &str| {
        connection
            .query_row(sql, params![decision_id], |row| row.get::<_, i64>(0))
            .unwrap()
    };
    let typed_rows_sha256 = Sha256::digest(&typed_rows).into();
    DangerousConfirmationSqliteSnapshot {
        typed_rows,
        typed_rows_sha256,
        grants: count("SELECT COUNT(*) FROM execass_accepted_confirmation_grants WHERE decision_id=?1"),
        challenges: count("SELECT COUNT(*) FROM execass_confirmation_challenges WHERE decision_id=?1"),
        alternatives: count("SELECT COUNT(*) FROM execass_confirmation_challenge_alternatives alternative JOIN execass_confirmation_challenges challenge ON challenge.challenge_id=alternative.challenge_id WHERE challenge.decision_id=?1"),
        attestations: count("SELECT COUNT(*) FROM execass_confirmation_attestations WHERE decision_id=?1"),
        dangerous_decisions: connection
            .query_row(
                "SELECT COUNT(*) FROM execass_decisions WHERE decision_kind='dangerous_action_confirmation'",
                [],
                |row| row.get(0),
            )
            .unwrap(),
        all_grants: connection
            .query_row("SELECT COUNT(*) FROM execass_accepted_confirmation_grants", [], |row| {
                row.get(0)
            })
            .unwrap(),
        all_challenges: connection
            .query_row("SELECT COUNT(*) FROM execass_confirmation_challenges", [], |row| {
                row.get(0)
            })
            .unwrap(),
        all_alternatives: connection
            .query_row(
                "SELECT COUNT(*) FROM execass_confirmation_challenge_alternatives",
                [],
                |row| row.get(0),
            )
            .unwrap(),
        all_attestations: connection
            .query_row("SELECT COUNT(*) FROM execass_confirmation_attestations", [], |row| {
                row.get(0)
            })
            .unwrap(),
    }
}

fn assert_dangerous_confirmation_snapshot_unchanged(
    expected: &DangerousConfirmationSqliteSnapshot,
    paths: &crate::AppPaths,
    decision_id: &str,
) {
    assert_eq!(expected.grants, 1, "one accepted grant was created");
    assert_eq!(
        expected.challenges, 1,
        "one resolved challenge was retained"
    );
    assert_eq!(
        expected.alternatives, 1,
        "one disclosed alternative was retained"
    );
    assert_eq!(
        expected.attestations, 1,
        "one signed attestation was retained"
    );
    assert_eq!(
        expected.dangerous_decisions, 1,
        "one dangerous decision was retained"
    );
    let actual = dangerous_confirmation_sqlite_snapshot(paths, decision_id);
    assert_eq!(
        actual.typed_rows,
        expected.typed_rows,
        "duplicate-risk resolution must not use, expire, reissue, invalidate, or otherwise rewrite an accepted dangerous-action grant"
    );
    assert_eq!(actual.typed_rows_sha256, expected.typed_rows_sha256);
    assert_eq!(actual.grants, expected.grants, "no grant may be reissued");
    assert_eq!(
        actual.challenges, expected.challenges,
        "no challenge may be reissued"
    );
    assert_eq!(
        actual.alternatives, expected.alternatives,
        "no alternative may be reissued"
    );
    assert_eq!(
        actual.attestations, expected.attestations,
        "no attestation may be reissued"
    );
    assert_eq!(
        actual.dangerous_decisions, expected.dangerous_decisions,
        "no dangerous decision may be reissued"
    );
    assert_eq!(
        actual.all_grants, expected.all_grants,
        "no grant may be created elsewhere"
    );
    assert_eq!(
        actual.all_challenges, expected.all_challenges,
        "no challenge may be created elsewhere"
    );
    assert_eq!(
        actual.all_alternatives, expected.all_alternatives,
        "no alternative may be created elsewhere"
    );
    assert_eq!(
        actual.all_attestations, expected.all_attestations,
        "no attestation may be created elsewhere"
    );
    let connection = Connection::open(&paths.db_path).unwrap();
    let columns = connection
        .prepare("PRAGMA table_info(execass_accepted_confirmation_grants)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(
        !columns.iter().any(|column| {
            matches!(
                column.as_str(),
                "use_count" | "grant_use_count" | "expires_at" | "expired_at"
            )
        }),
        "accepted dangerous-action grants intentionally have no use counter or expiry field"
    );
}

pub(super) fn duplicate_risk_fixture_with_accepted_dangerous_grant(
    suffix: &str,
) -> (AtomicFixture, String) {
    let (fixture, integrity, redactor, key, dangerous_decision_id) =
        accepted_dangerous_grant_fixture();
    let decision_id = format!("grant-preservation-duplicate-{suffix}");
    let action_id = format!("grant-preservation-successor-action-{suffix}");
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let (delegation_id, delegation_revision, plan_revision, manifest_digest, payload_digest, confirmed_action):
        (String, i64, i64, String, String, String) = connection
        .query_row(
            "SELECT delegation_id,delegation_revision,plan_revision,manifest_digest,payload_digest,confirmed_logical_action_identity FROM execass_decisions WHERE decision_id=?1",
            params![dangerous_decision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE execass_continuations SET status='uncertain',updated_at=1800000000021 WHERE continuation_id='continuation-1' AND status='runnable'",
            [],
        )
        .unwrap();
    let (claim_event_id, runtime_authority_provenance_id): (String, String) = connection
        .query_row(
            r#"SELECT o.event_id,d.authority_provenance_id
               FROM execass_outbox_events o
               JOIN execass_delegations d ON d.delegation_id='delegation-1'
               WHERE o.aggregate_id='delegation-1'
               ORDER BY o.global_sequence LIMIT 1"#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let predecessor_job_id = format!("grant-preservation-predecessor-job-{suffix}");
    let predecessor_claim_receipt_id = format!("grant-preservation-claim-{suffix}");
    let predecessor_continuation_id =
        format!("grant-preservation-predecessor-continuation-{suffix}");
    connection
        .execute(
            r#"INSERT INTO jobs(job_id,agent_id,name,enabled,schedule_kind,interval_seconds,
                run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,
                lease_owner,lease_expires_at,last_run_at,last_error,created_at,updated_at,deleted_at)
              SELECT ?1,agent_id,'grant-preservation predecessor',0,'at',NULL,NULL,NULL,'{}',0,0,1000,
                NULL,NULL,NULL,NULL,1800000000021,1800000000021,NULL FROM agents ORDER BY agent_id LIMIT 1"#,
            params![predecessor_job_id],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_action_branches(
                action_id,delegation_id,action_revision,target_delegation_revision,
                target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
              ) VALUES(?1,?2,50,?3,?4,0,'ordinary','waiting','accepted dangerous predecessor',1800000000021,1800000000021,NULL)"#,
            params![confirmed_action, delegation_id, delegation_revision, plan_revision],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_continuations(
                continuation_id,delegation_id,target_delegation_revision,target_plan_revision,
                action_id,branch_kind,causation_kind,causation_id,status,job_id,lease_owner,
                lease_expires_at,fencing_token,host_generation,stop_epoch,global_stop_epoch,
                created_at,updated_at,completed_at
              ) VALUES(?1,?2,?3,?4,?5,'ordinary','action_result',?6,'uncertain',?7,
                NULL,NULL,1,1,0,0,1800000000021,1800000000021,NULL)"#,
            params![
                predecessor_continuation_id,
                delegation_id,
                delegation_revision,
                plan_revision,
                confirmed_action,
                format!("grant-preservation-predecessor-causation-{suffix}"),
                predecessor_job_id,
            ],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_continuation_operation_history(
                event_id,claim_event_id,claim_receipt_id,operation,result_status,continuation_id,
                delegation_id,action_id,job_id,worker_id,job_lease_expires_at,
                continuation_fencing_token,runtime_host_generation,runtime_host_instance_id,
                runtime_fencing_token,state_root_generation,runtime_authority_provenance_id,
                runtime_actor_identity,policy_revision,global_stop_epoch,technical_quota_policy_digest,
                technical_quota_snapshot_id,technical_resource_reservation_set_json,
                technical_resource_reservation_set_digest,technical_resource_evidence_digest,recorded_at
              ) VALUES(?1,?1,?2,'claim','executing',?3,'delegation-1',?4,?5,'grant-preservation-worker',
                1800000010000,1,1,'danger-host',1,1,?6,'danger-user',1,0,?7,NULL,'[]',?8,NULL,1800000000022)"#,
            params![
                claim_event_id,
                predecessor_claim_receipt_id,
                predecessor_continuation_id,
                confirmed_action,
                predecessor_job_id,
                runtime_authority_provenance_id,
                format!("sha256:grant-preservation-quota-{suffix}"),
                format!("sha256:grant-preservation-reservations-{suffix}"),
            ],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_action_branches(
                action_id,delegation_id,action_revision,target_delegation_revision,
                target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
              ) VALUES(?1,?2,51,?3,?4,0,'ordinary','waiting','duplicate-risk successor',1800000000021,1800000000021,NULL)"#,
            params![action_id, delegation_id, delegation_revision, plan_revision],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_decisions(
                decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,
                decision_kind,status,result,exact_presented_action_json,confirmed_logical_action_identity,
                manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,
                connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,
                consequence,alternatives_json,idempotency_key,requested_at,resolved_at,
                resolved_by_authority_provenance_id
              ) SELECT ?1,delegation_id,2,delegation_revision,plan_revision,policy_revision,
                'duplicate_risk_retry','pending',NULL,exact_presented_action_json,confirmed_logical_action_identity,
                manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,
                connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,
                consequence,alternatives_json,?2,1800000000021,NULL,NULL
              FROM execass_decisions WHERE decision_id=?3"#,
            params![decision_id, format!("grant-preservation-presentation-{suffix}"), dangerous_decision_id],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_logical_effects(
                logical_effect_id,delegation_id,continuation_id,action_kind,state,
                internal_idempotency_key,provider_identity,provider_idempotency_key,
                reconciliation_key,manifest_digest,payload_digest,outcome_json,created_at,updated_at
              ) VALUES(?1,?2,?3,'public_or_externally_consequential_communication',
                'invoking',?4,'atomic-provider',?5,?6,?7,?8,NULL,1800000000021,1800000000022)"#,
            params![
                format!("grant-preservation-predecessor-effect-{suffix}"),
                delegation_id,
                predecessor_continuation_id,
                format!("grant-preservation-predecessor-internal-{suffix}"),
                format!("grant-preservation-predecessor-provider-{suffix}"),
                format!("grant-preservation-predecessor-reconcile-{suffix}"),
                manifest_digest,
                payload_digest,
            ],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_provider_attempts(
                attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,
                claim_event_id,claim_receipt_id,attempt_number,fencing_token,host_generation,
                host_instance_id,runtime_fencing_token,status,provider_request_digest,
                provider_response_digest,remote_effect_id,started_at,finished_at
              ) VALUES(?1,?2,?3,?4,?5,?6,?7,1,1,1,
                'danger-host',1,'invoking',?8,NULL,NULL,1800000000022,NULL)"#,
            params![
                format!("grant-preservation-predecessor-attempt-{suffix}"),
                delegation_id,
                format!("grant-preservation-predecessor-effect-{suffix}"),
                predecessor_continuation_id,
                confirmed_action,
                claim_event_id,
                predecessor_claim_receipt_id,
                format!(
                    "sha256:{:x}",
                    Sha256::digest(format!("grant-preservation-request-{suffix}"))
                ),
            ],
        )
        .unwrap();
    seed_signed_execution_unknown_fixture(
        &connection,
        &format!("grant-preservation-predecessor-attempt-{suffix}"),
        &format!("grant-preservation-predecessor-effect-{suffix}"),
        1_800_000_000_023,
    )
    .unwrap();
    drop(connection);
    let binding = fixture
        .store
        .bind_duplicate_risk_predecessor(&decision_id, 1_800_000_000_025)
        .unwrap()
        .expect("exact duplicate-risk predecessor binding");
    assert_eq!(
        binding.accepted_confirmation_grant_id.as_deref(),
        Some("grant-attested"),
        "storage derives the already-accepted dangerous-action grant rather than accepting a caller supplied grant"
    );
    let mut authority_input =
        test_authority_input(&format!("grant-preservation-resolution-{suffix}"));
    authority_input.request_correlation_id = format!("grant-preservation-correlation-{suffix}");
    authority_input.source_message_id = Some(format!("grant-preservation-message-{suffix}"));
    authority_input.authority_kind = "decision_resolution".into();
    authority_input.bound_decision_id = Some(decision_id.clone());
    authority_input.bound_decision_revision = Some(2);
    let manifest = ready_manifest(&admitted_dispatch_with_authority("authority-1"));
    assert_eq!(
        manifest.canonical().digest().as_hex(),
        manifest_digest,
        "the duplicate-risk decision must bind the exact dangerous-action manifest"
    );
    authority_input.bound_manifest_bytes = Some(manifest.canonical().bytes().to_vec());
    authority_input.challenge_nonce_bytes =
        Some(format!("grant-preservation-presentation-{suffix}").into_bytes());
    authority_input.created_at = 1_800_000_000_026;
    let authority = issue_test_local_owner_authority(authority_input).unwrap();
    let atomic = AtomicFixture {
        fixture,
        integrity,
        redactor,
        key,
        authority,
        decision_id,
        action_id,
        manifest_digest,
    };
    (atomic, dangerous_decision_id)
}

#[test]
fn applies_once_and_exactly_replays() {
    let fixture = setup(DecisionKind::RecoveryChoice, "apply-replay");
    let command = command(&fixture, DecisionResult::ConfirmAndContinue, "apply-replay");
    let AtomicDecisionResolutionOutcome::Applied(first) = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .unwrap()
    else {
        panic!("first exact resolution must apply");
    };
    assert!(first.continuation.is_some());
    let AtomicDecisionResolutionOutcome::Replayed(replayed) = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .unwrap()
    else {
        panic!("identical resolution must replay");
    };
    assert_eq!(first, replayed);
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        1
    );
}

#[test]
fn receipt_context_uses_only_the_single_current_unreleased_host_lease() {
    let fixture = setup(DecisionKind::RecoveryChoice, "current-host-lease");
    let connection = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    connection
        .execute(
            "UPDATE execass_runtime_host_leases SET released_at=10 WHERE lease_id='atomic-lease'",
            [],
        )
        .unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(2,'execass',1,'atomic-installation','atomic-user','atomic-host-2',11);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('atomic-lease-2','execass',2,'atomic-host-2',2,11,9999999999999);
            "#,
        )
        .unwrap();
    let context = fixture
        .fixture
        .store
        .read_decision_receipt_context(&fixture.decision_id, 20)
        .unwrap()
        .expect("decision has one current runtime receipt context");
    assert_eq!(context.runtime_host_generation, 2);
    assert_eq!(context.runtime_host_instance_id, "atomic-host-2");
    assert_eq!(context.runtime_fencing_token, 2);
}

#[test]
fn released_lease_cannot_commit_atomic_resolution() {
    let fixture = setup(DecisionKind::RecoveryChoice, "released-lease-commit");
    let command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "released-lease-commit",
    );
    let before_outbox = table_count(&fixture.fixture.paths, "execass_outbox_events");
    let connection = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    connection
        .execute(
            "UPDATE execass_runtime_host_leases SET released_at=?1 WHERE lease_id='atomic-lease'",
            params![command.write.occurred_at - 1],
        )
        .unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(2,'execass',1,'atomic-installation','atomic-user','atomic-host-2',1800000000029);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('atomic-lease-2','execass',2,'atomic-host-2',2,1800000000029,9999999999999);
            "#,
        )
        .unwrap();

    let error = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .expect_err("released host lease must lose commit authority");
    assert!(error
        .to_string()
        .contains("not the current live ExecAss lease"));
    let verification = Connection::open(&fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        verification
            .query_row(
                "SELECT status FROM execass_decisions WHERE decision_id=?1",
                params![fixture.decision_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "pending"
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_outbox_events"),
        before_outbox
    );
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 0);
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        0
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_logical_effects"),
        0
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        0
    );
    assert!(matches!(
        fixture.integrity.status().unwrap(),
        IntegrityStatus::Uninitialized
    ));
}

#[test]
fn expired_lease_cannot_commit_atomic_resolution() {
    let fixture = setup(DecisionKind::RecoveryChoice, "expired-lease-commit");
    let mut command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "expired-lease-commit",
    );
    command.receipt.committed_at = 9_999_999_999_999;
    let before_outbox = table_count(&fixture.fixture.paths, "execass_outbox_events");

    let error = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .expect_err("expired host lease must lose commit authority");
    assert!(error
        .to_string()
        .contains("not the current live ExecAss lease"));
    assert_eq!(
        Connection::open(&fixture.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM execass_decisions WHERE decision_id=?1",
                params![fixture.decision_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "pending"
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_outbox_events"),
        before_outbox
    );
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 0);
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        0
    );
    assert!(matches!(
        fixture.integrity.status().unwrap(),
        IntegrityStatus::Uninitialized
    ));
}

#[test]
fn nonaffirmative_results_create_no_continuation_effect_or_stop_side_effect() {
    for (result, suffix) in [
        (DecisionResult::Revise, "revise-zero"),
        (DecisionResult::Decline, "decline-zero"),
        (DecisionResult::Stop, "stop-zero"),
    ] {
        let fixture = setup(DecisionKind::RecoveryChoice, suffix);
        let command = command(&fixture, result, suffix);
        assert!(matches!(
            fixture.fixture.store.resolve_decision_atomically(
                &fixture.integrity,
                &fixture.redactor,
                &command,
                &fixture.authority,
            ),
            Ok(AtomicDecisionResolutionOutcome::Applied(_))
        ));
        assert_eq!(
            table_count(&fixture.fixture.paths, "execass_continuations"),
            0
        );
        assert_eq!(
            table_count(&fixture.fixture.paths, "execass_logical_effects"),
            0
        );
        let connection = Connection::open(&fixture.fixture.paths.db_path).unwrap();
        let (run_control, stop_epoch): (String, i64) = connection
            .query_row(
                "SELECT run_control,stop_epoch FROM execass_delegations WHERE delegation_id='delegation-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(run_control, "running");
        assert_eq!(stop_epoch, 0);
    }
}

#[test]
fn duplicate_risk_confirm_creates_exactly_one_new_effect() {
    let fixture = setup(DecisionKind::DuplicateRiskRetry, "duplicate-risk");
    let command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "duplicate-risk",
    );
    let AtomicDecisionResolutionOutcome::Applied(applied) = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .unwrap()
    else {
        panic!("duplicate-risk resolution must apply");
    };
    assert_eq!(
        applied
            .technical_quota_snapshot
            .as_ref()
            .expect("quota snapshot persisted")
            .entries
            .len(),
        4
    );
    assert_eq!(
        applied
            .technical_resource_requirements
            .as_ref()
            .expect("requirement set persisted")
            .requirements
            .len(),
        4
    );
    let AtomicDecisionResolutionOutcome::Replayed(replayed) = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .unwrap()
    else {
        panic!("duplicate-risk resolution must replay exactly");
    };
    assert_eq!(applied, replayed);
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_logical_effects"),
        2
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        1
    );
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 1);
}

#[test]
fn duplicate_risk_storage_preparation_copies_authoritative_effect_and_resources() {
    let fixture = setup(
        DecisionKind::DuplicateRiskRetry,
        "duplicate-risk-storage-preparation",
    );
    let conn = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    let predecessor_effect_id = "atomic-predecessor-effect-duplicate-risk-storage-preparation";
    let predecessor_action_id = "atomic-predecessor-action-duplicate-risk-storage-preparation";
    let authority_json: String = conn
        .query_row(
            "SELECT effective_authority_json FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let authority_digest =
        carsinos_core::execass_policy::technical_effective_authority_digest(&authority_json)
            .unwrap();
    let snapshot = carsinos_core::execass_policy::compile_technical_quota_snapshot(
        "delegation-1",
        1,
        &authority_digest,
        "delegation",
        vec![carsinos_core::execass_policy::TechnicalQuotaEntryInput {
            kind: carsinos_core::execass_policy::TechnicalResourceKind::Tokens,
            unit: "token".into(),
            limit: 100,
        }],
    )
    .unwrap();
    let requirements = carsinos_core::execass_policy::compile_technical_resource_requirements(
        &snapshot,
        predecessor_effect_id,
        predecessor_action_id,
        &fixture.manifest_digest,
        vec![
            carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                kind: carsinos_core::execass_policy::TechnicalResourceKind::Tokens,
                unit: "token".into(),
                amount: 40,
            },
        ],
    )
    .unwrap();
    super::rows::insert_technical_quota_snapshot(&conn, &snapshot, 1_800_000_000_020).unwrap();
    super::rows::insert_technical_resource_requirements(&conn, &requirements, 1_800_000_000_020)
        .unwrap();
    drop(conn);

    let prepared = fixture
        .fixture
        .store
        .prepare_duplicate_risk_successor(
            &fixture.decision_id,
            &fixture.action_id,
            "verified-resolution-identity",
            1_800_000_000_030,
            1,
            0,
        )
        .unwrap()
        .expect("prepare exact successor");
    let replay_material = fixture
        .fixture
        .store
        .prepare_duplicate_risk_successor(
            &fixture.decision_id,
            &fixture.action_id,
            "verified-resolution-identity",
            1_800_000_000_030,
            1,
            0,
        )
        .unwrap()
        .expect("rebuild exact successor");
    assert_eq!(prepared, replay_material);
    assert_eq!(prepared.continuation.action_id, fixture.action_id);
    assert_eq!(
        prepared.logical_effect.provider_identity.as_deref(),
        Some("atomic-provider")
    );
    assert_ne!(
        prepared.logical_effect.internal_idempotency_key,
        "atomic-predecessor-internal-duplicate-risk-storage-preparation"
    );
    assert_ne!(
        prepared.logical_effect.provider_idempotency_key.as_deref(),
        Some("atomic-predecessor-provider-duplicate-risk-storage-preparation")
    );
    assert_ne!(
        prepared.logical_effect.reconciliation_key.as_deref(),
        Some("atomic-predecessor-reconcile-duplicate-risk-storage-preparation")
    );
    assert_eq!(prepared.technical_quota_snapshot.entries, snapshot.entries);
    assert_eq!(
        prepared.technical_resource_requirements.requirements,
        requirements.requirements
    );
}

#[test]
fn duplicate_risk_resolution_binding_rejects_zero_or_multiple_waiting_successors() {
    let (zero, _) =
        duplicate_risk_fixture_with_accepted_dangerous_grant("duplicate-risk-zero-successor");
    let conn = open_sqlite_connection(&zero.fixture.paths.db_path).unwrap();
    conn.execute(
        "UPDATE execass_action_branches SET status='superseded',terminal_at=1800000000030,updated_at=1800000000030 WHERE action_id=?1",
        [&zero.action_id],
    )
    .unwrap();
    drop(conn);
    assert!(zero
        .fixture
        .store
        .read_decision_resolution_binding(&zero.decision_id, &zero.action_id)
        .unwrap()
        .is_none());

    let (multiple, _) =
        duplicate_risk_fixture_with_accepted_dangerous_grant("duplicate-risk-multiple-successors");
    assert!(multiple
        .fixture
        .store
        .read_decision_resolution_binding(&multiple.decision_id, &multiple.action_id)
        .unwrap()
        .is_some());
    let conn = open_sqlite_connection(&multiple.fixture.paths.db_path).unwrap();
    conn.execute(
        r#"INSERT INTO execass_action_branches(
             action_id,delegation_id,action_revision,target_delegation_revision,
             target_plan_revision,stop_epoch,branch_kind,status,action_summary,
             created_at,updated_at,terminal_at
           ) SELECT 'duplicate-risk-second-successor',delegation_id,action_revision+1,
             target_delegation_revision,target_plan_revision,stop_epoch,'recovery','waiting',
             'ambiguous duplicate retry',1800000000030,1800000000030,NULL
             FROM execass_action_branches WHERE action_id=?1"#,
        [&multiple.action_id],
    )
    .unwrap();
    drop(conn);
    assert!(multiple
        .fixture
        .store
        .read_decision_resolution_binding(&multiple.decision_id, &multiple.action_id)
        .unwrap_err()
        .to_string()
        .contains("ambiguous waiting successors"));
}

#[test]
fn duplicate_risk_requires_distinct_successor_and_rejects_raw_predecessor_reconciliation() {
    let fixture = setup(DecisionKind::DuplicateRiskRetry, "duplicate-risk-distinct");
    let mut same_identity = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "duplicate-risk-distinct",
    );
    let effect = same_identity
        .logical_effect
        .as_mut()
        .expect("duplicate-risk successor");
    effect.internal_idempotency_key = "atomic-predecessor-internal-duplicate-risk-distinct".into();
    effect.provider_idempotency_key =
        Some("atomic-predecessor-provider-duplicate-risk-distinct".into());
    effect.reconciliation_key = Some("atomic-predecessor-reconcile-duplicate-risk-distinct".into());
    let error = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &same_identity,
            &fixture.authority,
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("distinct stable effect identity"));
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_logical_effects"),
        1
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        0
    );

    let drift = setup(DecisionKind::DuplicateRiskRetry, "duplicate-risk-drift");
    let connection = Connection::open(&drift.fixture.paths.db_path).unwrap();
    let raw_attempt_reconciliation = connection.execute(
            "UPDATE execass_provider_attempts SET status='reconciled_present' WHERE attempt_id='atomic-predecessor-attempt-duplicate-risk-drift' AND status='outcome_unknown'",
            [],
        );
    assert!(raw_attempt_reconciliation.is_err());
    let raw_effect_reconciliation = connection.execute(
            "UPDATE execass_logical_effects SET state='reconciled_present' WHERE logical_effect_id='atomic-predecessor-effect-duplicate-risk-drift' AND state='outcome_unknown'",
            [],
        );
    assert!(raw_effect_reconciliation.is_err());
    let immutable_update = connection.execute(
        "UPDATE execass_duplicate_risk_bindings SET predecessor_uncertainty_evidence_digest='forged' WHERE decision_id='atomic-decision-duplicate-risk-drift'",
        [],
    );
    assert!(immutable_update.is_err());
    let forbidden_delete = connection.execute(
        "DELETE FROM execass_duplicate_risk_bindings WHERE decision_id='atomic-decision-duplicate-risk-drift'",
        [],
    );
    assert!(forbidden_delete.is_err());
    drop(connection);
    assert_eq!(
        table_count(&drift.fixture.paths, "execass_logical_effects"),
        1
    );
}

#[test]
fn duplicate_risk_race_replays_one_successor_and_nonaffirmative_results_create_none() {
    let fixture = setup(DecisionKind::DuplicateRiskRetry, "duplicate-risk-race");
    let confirm_command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "duplicate-risk-race",
    );
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let results = &results;
            let store = fixture.fixture.store.clone();
            let integrity = &fixture.integrity;
            let redactor = &fixture.redactor;
            let authority = &fixture.authority;
            let command = &confirm_command;
            scope.spawn(move || {
                results.lock().unwrap().push(
                    store
                        .resolve_decision_atomically(integrity, redactor, command, authority)
                        .unwrap(),
                );
            });
        }
    });
    let results = results.into_inner().unwrap();
    assert_eq!(
        results
            .iter()
            .filter(|item| matches!(item, AtomicDecisionResolutionOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|item| matches!(item, AtomicDecisionResolutionOutcome::Replayed(_)))
            .count(),
        7
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        1
    );

    for (result, suffix) in [
        (DecisionResult::Decline, "duplicate-risk-decline"),
        (DecisionResult::Revise, "duplicate-risk-revise"),
    ] {
        let fixture = setup(DecisionKind::DuplicateRiskRetry, suffix);
        assert!(matches!(
            fixture.fixture.store.resolve_decision_atomically(
                &fixture.integrity,
                &fixture.redactor,
                &command(&fixture, result, suffix),
                &fixture.authority,
            ),
            Ok(AtomicDecisionResolutionOutcome::Applied(_))
        ));
        assert_eq!(
            table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
            0
        );
        assert_eq!(
            table_count(&fixture.fixture.paths, "execass_logical_effects"),
            1
        );
    }
}

fn grant_preservation_duplicate_risk_command(
    fixture: &AtomicFixture,
    result: DecisionResult,
    suffix: &str,
) -> AtomicDecisionResolutionCommand {
    let mut resolution = command(fixture, result, suffix);
    let connection = Connection::open(&fixture.fixture.paths.db_path).unwrap();
    let (decision_revision, payload_digest): (i64, String) = connection
        .query_row(
            "SELECT decision_revision,payload_digest FROM execass_decisions WHERE decision_id=?1",
            params![fixture.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    resolution.decision_revision = decision_revision;
    resolution.receipt.subject.revision = decision_revision;
    if let Some(effect) = &mut resolution.logical_effect {
        effect.payload_digest = payload_digest;
    }
    let context = fixture
        .fixture
        .store
        .read_decision_receipt_context(&fixture.decision_id, resolution.write.occurred_at)
        .unwrap()
        .expect("accepted dangerous grant leaves one live receipt context");
    resolution.receipt.expected_state_revision = context.delegation_revision;
    resolution.receipt.expected_global_count = context.global_receipt_count;
    resolution.receipt.expected_global_head_digest = context.global_receipt_head_digest;
    resolution.receipt.expected_delegation_count = context.delegation_receipt_count;
    resolution.receipt.expected_delegation_head_digest = context.delegation_receipt_head_digest;
    resolution.receipt.runtime.host_generation = context.runtime_host_generation;
    resolution.receipt.runtime.host_instance_id = context.runtime_host_instance_id;
    resolution.receipt.runtime.fencing_token = context.runtime_fencing_token;
    resolution
}

#[test]
fn duplicate_risk_preserves_real_accepted_danger_grant_sqlite_bytes_across_all_resolutions() {
    let (fixture, dangerous_decision_id) =
        duplicate_risk_fixture_with_accepted_dangerous_grant("confirm-replay");
    let snapshot =
        dangerous_confirmation_sqlite_snapshot(&fixture.fixture.paths, &dangerous_decision_id);
    assert_dangerous_confirmation_snapshot_unchanged(
        &snapshot,
        &fixture.fixture.paths,
        &dangerous_decision_id,
    );
    let confirmation = grant_preservation_duplicate_risk_command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "confirm-replay",
    );
    let outcome = fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &confirmation,
            &fixture.authority,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        AtomicDecisionResolutionOutcome::Applied(_)
    ));
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        1,
        "only the affirmative typed duplicate-risk result creates one successor"
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_logical_effects"),
        2,
        "one unresolved predecessor and exactly one successor remain"
    );
    assert_dangerous_confirmation_snapshot_unchanged(
        &snapshot,
        &fixture.fixture.paths,
        &dangerous_decision_id,
    );
    assert!(matches!(
        fixture.fixture.store.resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &confirmation,
            &fixture.authority,
        ),
        Ok(AtomicDecisionResolutionOutcome::Replayed(_))
    ));
    assert_dangerous_confirmation_snapshot_unchanged(
        &snapshot,
        &fixture.fixture.paths,
        &dangerous_decision_id,
    );

    let (fixture, dangerous_decision_id) =
        duplicate_risk_fixture_with_accepted_dangerous_grant("race");
    let snapshot =
        dangerous_confirmation_sqlite_snapshot(&fixture.fixture.paths, &dangerous_decision_id);
    let confirmation = grant_preservation_duplicate_risk_command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "race",
    );
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let results = &results;
            let store = fixture.fixture.store.clone();
            let integrity = &fixture.integrity;
            let redactor = &fixture.redactor;
            let authority = &fixture.authority;
            let confirmation = &confirmation;
            scope.spawn(move || {
                results.lock().unwrap().push(
                    store
                        .resolve_decision_atomically(integrity, redactor, confirmation, authority)
                        .unwrap(),
                );
            });
        }
    });
    let results = results.into_inner().unwrap();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, AtomicDecisionResolutionOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, AtomicDecisionResolutionOutcome::Replayed(_)))
            .count(),
        7
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        1
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_logical_effects"),
        2
    );
    assert_dangerous_confirmation_snapshot_unchanged(
        &snapshot,
        &fixture.fixture.paths,
        &dangerous_decision_id,
    );

    for (result, suffix) in [
        (DecisionResult::Decline, "decline"),
        (DecisionResult::Revise, "revise"),
    ] {
        let (fixture, dangerous_decision_id) =
            duplicate_risk_fixture_with_accepted_dangerous_grant(suffix);
        let snapshot =
            dangerous_confirmation_sqlite_snapshot(&fixture.fixture.paths, &dangerous_decision_id);
        let resolution = grant_preservation_duplicate_risk_command(&fixture, result, suffix);
        assert!(matches!(
            fixture.fixture.store.resolve_decision_atomically(
                &fixture.integrity,
                &fixture.redactor,
                &resolution,
                &fixture.authority,
            ),
            Ok(AtomicDecisionResolutionOutcome::Applied(_))
        ));
        assert_eq!(
            table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
            0,
            "non-affirmative duplicate-risk result must create no successor"
        );
        assert_eq!(
            table_count(&fixture.fixture.paths, "execass_logical_effects"),
            1,
            "non-affirmative duplicate-risk result retains only the predecessor"
        );
        assert_dangerous_confirmation_snapshot_unchanged(
            &snapshot,
            &fixture.fixture.paths,
            &dangerous_decision_id,
        );
    }
}

#[test]
fn concurrent_identical_resolution_has_one_applied_and_only_exact_replays() {
    let fixture = setup(DecisionKind::RecoveryChoice, "concurrent-identical");
    let command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "concurrent-identical",
    );
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let results = &results;
            let store = fixture.fixture.store.clone();
            let command = &command;
            let integrity = &fixture.integrity;
            let redactor = &fixture.redactor;
            let authority = &fixture.authority;
            scope.spawn(move || {
                results.lock().unwrap().push(
                    store
                        .resolve_decision_atomically(integrity, redactor, command, authority)
                        .unwrap(),
                );
            });
        }
    });
    let results = results.into_inner().unwrap();
    assert_eq!(
        results
            .iter()
            .filter(|item| matches!(item, AtomicDecisionResolutionOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|item| matches!(item, AtomicDecisionResolutionOutcome::Replayed(_)))
            .count(),
        7
    );
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        1
    );
}

#[test]
fn concurrent_mixed_results_have_one_winner_and_only_its_continuation() {
    let fixture = setup(DecisionKind::RecoveryChoice, "concurrent-mixed");
    let commands = [
        command(
            &fixture,
            DecisionResult::ConfirmAndContinue,
            "concurrent-mixed-confirm",
        ),
        command(
            &fixture,
            DecisionResult::Decline,
            "concurrent-mixed-decline",
        ),
        command(&fixture, DecisionResult::Stop, "concurrent-mixed-stop"),
        command(&fixture, DecisionResult::Revise, "concurrent-mixed-revise"),
    ];
    let barrier = std::sync::Barrier::new(commands.len());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for command in &commands {
            let barrier = &barrier;
            let results = &results;
            let store = fixture.fixture.store.clone();
            let integrity = &fixture.integrity;
            let redactor = &fixture.redactor;
            let authority = &fixture.authority;
            scope.spawn(move || {
                barrier.wait();
                results.lock().unwrap().push(
                    store
                        .resolve_decision_atomically(integrity, redactor, command, authority)
                        .unwrap(),
                );
            });
        }
    });
    let results = results.into_inner().unwrap();
    let applied = results
        .iter()
        .find_map(|result| match result {
            AtomicDecisionResolutionOutcome::Applied(bundle) => Some(bundle.as_ref()),
            _ => None,
        })
        .expect("one mixed resolution must win");
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, AtomicDecisionResolutionOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, AtomicDecisionResolutionOutcome::Conflict { .. }))
            .count(),
        3
    );
    let winning_result = applied.decision.result.expect("winner has a result");
    assert!(results.iter().all(|result| match result {
        AtomicDecisionResolutionOutcome::Applied(bundle) => {
            bundle.decision.result == Some(winning_result)
        }
        AtomicDecisionResolutionOutcome::Conflict {
            winning_result: conflict_result,
        } => *conflict_result == Some(winning_result),
        _ => false,
    }));
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        i64::from(winning_result == DecisionResult::ConfirmAndContinue)
    );
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        Connection::open(&fixture.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.decision.recorded'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn conflicting_result_loses_without_second_bundle() {
    let fixture = setup(DecisionKind::RecoveryChoice, "conflicting-result");
    let winning = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "conflicting-result",
    );
    let mut losing = command(&fixture, DecisionResult::Decline, "conflicting-loser");
    losing.receipt.expected_global_count = 0;
    assert!(matches!(
        fixture.fixture.store.resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &winning,
            &fixture.authority,
        ),
        Ok(AtomicDecisionResolutionOutcome::Applied(_))
    ));
    assert!(matches!(
        fixture.fixture.store.resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &losing,
            &fixture.authority,
        ),
        Ok(AtomicDecisionResolutionOutcome::Conflict {
            winning_result: Some(DecisionResult::ConfirmAndContinue)
        })
    ));
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        Connection::open(&fixture.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.decision.recorded'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn revise_cannot_smuggle_a_runnable_continuation() {
    let fixture = setup(DecisionKind::RecoveryChoice, "revise-smuggle");
    let mut command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "revise-smuggle",
    );
    command.result = DecisionResult::Revise;
    assert!(fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .is_err());
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 0);
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        0
    );
    assert_eq!(
        Connection::open(&fixture.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM execass_decisions WHERE decision_id=?1",
                params![fixture.decision_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "pending"
    );
}

#[test]
fn signed_dangerous_confirmation_atomically_records_grant_receipt_and_outbox() {
    let (fixture, confirmation, attestation, _) = prepared_attested_confirmation();
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'danger-installation','danger-user','danger-host',1);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('danger-lease','execass',1,'danger-host',1,1,9999999999999);
            "#,
        )
        .unwrap();
    let (delegation_id, delegation_revision): (String, i64) = connection
        .query_row(
            "SELECT delegation_id,delegation_revision FROM execass_decisions WHERE decision_id=?1",
            params![confirmation.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("atomic-danger-receipt-key")
        .unwrap();
    let event_id = "atomic-danger-event".to_string();
    let resolution = AtomicDecisionResolutionCommand {
        write: WriteContext {
            idempotency_key: "atomic-danger-resolution".into(),
            correlation_id: "atomic-danger-correlation".into(),
            causation_id: confirmation.decision_id.clone(),
            occurred_at: attestation.payload.issued_at_ms as i64,
        },
        decision_id: confirmation.decision_id.clone(),
        decision_revision: confirmation.decision_revision,
        result: DecisionResult::ConfirmAndContinue,
        selected_logical_action_id: Some(confirmation.selected_logical_action_id.clone()),
        continuation: None,
        logical_effect: None,
        technical_quota_snapshot: None,
        technical_resource_requirements: None,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::DecisionRecorded,
            aggregate_id: delegation_id.clone(),
            aggregate_revision: delegation_revision,
            correlation_id: "atomic-danger-correlation".into(),
            causation_id: confirmation.decision_id.clone(),
            occurred_at: attestation.payload.issued_at_ms as i64,
            safe_payload_json: r#"{"result":"confirm_and_continue"}"#.into(),
            duplicate_identity: "atomic-danger-resolution".into(),
        },
        receipt: AppendReceiptCommand {
            receipt_id: "atomic-danger-receipt".into(),
            transaction_id: "atomic-danger-anchor".into(),
            state_root_generation: 1,
            delegation_id,
            expected_state_revision: delegation_revision,
            expected_global_count: 0,
            expected_global_head_digest: None,
            expected_delegation_count: 0,
            expected_delegation_head_digest: None,
            receipt_kind: ReceiptKind::Decision,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Decision,
                subject_id: confirmation.decision_id.clone(),
                revision: confirmation.decision_revision,
            },
            causation_id: confirmation.decision_id.clone(),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: ActorType::HumanLocal,
                actor_identity: SafeText::new("storage-derived", &[]).unwrap(),
                authority_provenance_id: "storage-derived".into(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: "danger-host".into(),
                fencing_token: 1,
            },
            key,
            rotation: None,
            evidence: Vec::new(),
            redacted_summary: SafeText::new("dangerous owner decision recorded", &[]).unwrap(),
            occurred_at: attestation.payload.issued_at_ms as i64,
            committed_at: attestation.payload.issued_at_ms as i64,
        },
    };
    let redactor = ReceiptRedactor::new(&["atomic-danger-secret"]).unwrap();
    let AtomicDecisionResolutionOutcome::Applied(applied) = fixture
        .store
        .confirm_dangerous_action_attested_atomically_at_for_test(
            &integrity,
            &redactor,
            &resolution,
            &confirmation.grant_id,
            &attestation,
            attestation.payload.issued_at_ms as i64,
        )
        .unwrap()
    else {
        panic!("signed dangerous result must apply atomically");
    };
    assert!(applied.confirmation_grant.is_some());
    assert!(applied.continuation.is_none());
    assert_eq!(table_count(&fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        Connection::open(&fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM execass_outbox_events WHERE event_name='execass.v1.decision.recorded'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_attested_atomically_at_for_test(
                &integrity,
                &redactor,
                &resolution,
                &confirmation.grant_id,
                &attestation,
                attestation.payload.issued_at_ms as i64 + 1,
            ),
        Ok(AtomicDecisionResolutionOutcome::Replayed(_))
    ));
}

#[test]
fn combined_disclosed_alternative_atomically_confirms_only_the_selected_action() {
    let (fixture, confirmation, attestation, _) = prepared_combined_attested_confirmation();
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'combined-installation','combined-user','combined-host',1);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('combined-lease','execass',1,'combined-host',1,1,9999999999999);
            "#,
        )
        .unwrap();
    let (delegation_id, delegation_revision, plan_revision, stop_epoch): (String, i64, i64, i64) =
        connection
            .query_row(
                "SELECT d.delegation_id,d.delegation_revision,d.plan_revision,g.stop_epoch FROM execass_decisions d JOIN execass_delegations g ON g.delegation_id=d.delegation_id WHERE d.decision_id=?1",
                params![confirmation.decision_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_action_branches(
              action_id,delegation_id,action_revision,target_delegation_revision,
              target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at
            ) VALUES(?1,?2,2,?3,?4,?5,'ordinary','waiting','selected combined dangerous target',?6,?6,NULL)"#,
            params![
                confirmation.selected_logical_action_id,
                delegation_id,
                delegation_revision,
                plan_revision,
                stop_epoch,
                attestation.payload.issued_at_ms as i64,
            ],
        )
        .unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("atomic-combined-receipt-key")
        .unwrap();
    let occurred_at = attestation.payload.issued_at_ms as i64;
    let event_id = "atomic-combined-event".to_string();
    let continuation_id = "atomic-combined-continuation".to_string();
    let resolution = AtomicDecisionResolutionCommand {
        write: WriteContext {
            idempotency_key: "atomic-combined-resolution".into(),
            correlation_id: "atomic-combined-correlation".into(),
            causation_id: confirmation.decision_id.clone(),
            occurred_at,
        },
        decision_id: confirmation.decision_id.clone(),
        decision_revision: confirmation.decision_revision,
        result: DecisionResult::ConfirmAndContinue,
        selected_logical_action_id: Some(confirmation.selected_logical_action_id.clone()),
        continuation: Some(ContinuationRecord {
            continuation_id: continuation_id.clone(),
            delegation_id: delegation_id.clone(),
            target_delegation_revision: delegation_revision,
            target_plan_revision: plan_revision,
            action_id: confirmation.selected_logical_action_id.clone(),
            branch_kind: ActionBranchKind::Ordinary,
            causation_kind: ContinuationCausationKind::Decision,
            causation_id: confirmation.decision_id.clone(),
            status: ContinuationStatus::Runnable,
            job_id: None,
            lease_owner: None,
            lease_expires_at: None,
            fencing_token: 0,
            host_generation: 1,
            stop_epoch,
            global_stop_epoch: 0,
            created_at: occurred_at,
            updated_at: occurred_at,
            completed_at: None,
        }),
        logical_effect: None,
        technical_quota_snapshot: None,
        technical_resource_requirements: None,
        outbox_event: NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::DecisionRecorded,
            aggregate_id: delegation_id.clone(),
            aggregate_revision: delegation_revision,
            correlation_id: "atomic-combined-correlation".into(),
            causation_id: confirmation.decision_id.clone(),
            occurred_at,
            safe_payload_json: r#"{"result":"confirm_and_continue","selected":"action-2"}"#.into(),
            duplicate_identity: "atomic-combined-resolution".into(),
        },
        receipt: AppendReceiptCommand {
            receipt_id: "atomic-combined-receipt".into(),
            transaction_id: "atomic-combined-anchor".into(),
            state_root_generation: 1,
            delegation_id: delegation_id.clone(),
            expected_state_revision: delegation_revision,
            expected_global_count: 0,
            expected_global_head_digest: None,
            expected_delegation_count: 0,
            expected_delegation_head_digest: None,
            receipt_kind: ReceiptKind::Decision,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::Decision,
                subject_id: confirmation.decision_id.clone(),
                revision: confirmation.decision_revision,
            },
            causation_id: confirmation.decision_id.clone(),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: ActorType::HumanLocal,
                actor_identity: SafeText::new("storage-derived", &[]).unwrap(),
                authority_provenance_id: "storage-derived".into(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: "combined-host".into(),
                fencing_token: 1,
            },
            key,
            rotation: None,
            evidence: Vec::new(),
            redacted_summary: SafeText::new("combined owner decision recorded", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    };
    let redactor = ReceiptRedactor::new(&["atomic-combined-secret"]).unwrap();
    let AtomicDecisionResolutionOutcome::Applied(applied) = fixture
        .store
        .confirm_dangerous_action_attested_atomically_at_for_test(
            &integrity,
            &redactor,
            &resolution,
            &confirmation.grant_id,
            &attestation,
            occurred_at,
        )
        .unwrap()
    else {
        panic!("combined selected alternative must apply atomically");
    };
    assert_eq!(
        applied.decision.result,
        Some(DecisionResult::ConfirmAndContinue)
    );
    let grant = applied
        .confirmation_grant
        .as_ref()
        .expect("selected alternative must have one durable grant");
    let selected_identity: String = connection
        .query_row(
            "SELECT confirmed_logical_action_identity FROM execass_confirmation_challenge_alternatives WHERE logical_action_id=?1",
            params![confirmation.selected_logical_action_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(grant.confirmed_logical_action_identity, selected_identity);
    assert!(grant
        .payload_and_material_operands_json
        .contains("target-2"));
    assert!(!grant
        .payload_and_material_operands_json
        .contains("target-1"));
    assert_eq!(
        applied
            .continuation
            .as_ref()
            .expect("confirmed selected action must continue")
            .action_id,
        "action-2"
    );
    assert_eq!(table_count(&fixture.paths, "execass_continuations"), 1);
    assert_eq!(
        connection
            .query_row(
                "SELECT status FROM execass_action_branches WHERE action_id='action-2'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "runnable"
    );
    assert_eq!(table_count(&fixture.paths, "execass_receipts"), 1);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM execass_continuations WHERE action_id='action-1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_attested_atomically_at_for_test(
                &integrity,
                &redactor,
                &resolution,
                &confirmation.grant_id,
                &attestation,
                occurred_at + 1,
            ),
        Ok(AtomicDecisionResolutionOutcome::Replayed(_))
    ));
    assert_eq!(table_count(&fixture.paths, "execass_continuations"), 1);
}

#[test]
fn outbox_collision_rolls_back_result_continuation_effect_and_receipt() {
    let fixture = setup(DecisionKind::DuplicateRiskRetry, "atomic-rollback");
    let command = command(
        &fixture,
        DecisionResult::ConfirmAndContinue,
        "atomic-rollback",
    );
    open_sqlite_connection(&fixture.fixture.paths.db_path)
        .unwrap()
        .execute(
            "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.notification.scheduled','delegation-1',1,'preexisting','preexisting',1,'v1','{}','preexisting')",
            params![command.outbox_event.event_id],
        )
        .unwrap();
    assert!(fixture
        .fixture
        .store
        .resolve_decision_atomically(
            &fixture.integrity,
            &fixture.redactor,
            &command,
            &fixture.authority,
        )
        .is_err());
    let connection = Connection::open(&fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT status FROM execass_decisions WHERE decision_id=?1",
                params![fixture.decision_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "pending"
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_continuations"),
        1
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_logical_effects"),
        1
    );
    assert_eq!(
        table_count(&fixture.fixture.paths, "execass_duplicate_risk_successors"),
        0
    );
    assert_eq!(table_count(&fixture.fixture.paths, "execass_receipts"), 0);
    assert!(matches!(
        fixture.integrity.status().unwrap(),
        IntegrityStatus::Uninitialized
    ));
}
