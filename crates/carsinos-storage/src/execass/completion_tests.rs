use super::receipt_integrity::AnchorCommitInput;
use super::tests::{fixture, foundation, table_count, Fixture};
use super::*;
use crate::open_sqlite_connection;
use rusqlite::{config::DbConfig, params, Connection};
use sha2::{Digest, Sha256};

pub(super) struct CompletionFixture {
    pub(super) fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    redactor: ReceiptRedactor,
    key: ReceiptKeyRef,
}

pub(super) fn setup(material: &[bool], initial_continuation: bool) -> CompletionFixture {
    let fixture = fixture();
    let mut base = foundation();
    if !initial_continuation {
        base.initial_continuation = None;
    }
    base.outcome_criteria.truncate(material.len().max(1));
    while base.outcome_criteria.len() < material.len() {
        let mut criterion = base.outcome_criteria[0].clone();
        criterion.criterion_id = format!("criterion-{}", base.outcome_criteria.len() + 1);
        criterion.criterion_key = criterion.criterion_id.clone();
        base.outcome_criteria.push(criterion);
    }
    for (index, is_material) in material.iter().copied().enumerate() {
        base.outcome_criteria[index].criterion_id = format!("criterion-{}", index + 1);
        base.outcome_criteria[index].criterion_key = format!("criterion-{}", index + 1);
        base.outcome_criteria[index].material = is_material;
    }
    fixture.store.create_foundation(&base).unwrap();
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'ea306-installation',
              'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
              'ea306-host',1800000000001);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('ea306-lease','execass',1,'ea306-host',1,1800000000001,9999999999999);
            INSERT INTO execass_authority_provenance(
              authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
              channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
              policy_revision,evidence_digest,created_at
            ) VALUES('ea306-assessor-authority','runtime','ea306-assessor','completion-assessor',
              'local-runtime-fence','ea306-assessor-bootstrap','runtime_safety_state','{}',1,
              'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
              1800000000001);
            "#,
        )
        .unwrap();
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("ea306-receipt-key")
        .unwrap();
    integrity
        .prepare_anchor(&AnchorCommitInput {
            state_root_generation: 1,
            anchor_generation: 1,
            receipt_count: 0,
            receipt_head_digest: None,
            key: key.clone(),
            transaction_id: "ea306-empty-anchor".into(),
            external_receipt_digest:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            occurred_at: 1_800_000_000_002,
        })
        .unwrap();
    let mut anchor_connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let anchor_transaction = anchor_connection.transaction().unwrap();
    integrity
        .confirm_prepared_anchor_in_transaction(
            &anchor_transaction,
            "ea306-empty-anchor",
            0,
            None,
            1_800_000_000_003,
        )
        .unwrap();
    anchor_transaction.commit().unwrap();
    integrity.finalize_anchor("ea306-empty-anchor").unwrap();
    CompletionFixture {
        fixture,
        integrity,
        redactor: ReceiptRedactor::new(&["ea306-secret"]).unwrap(),
        key,
    }
}

fn revision(f: &CompletionFixture) -> i64 {
    Connection::open(&f.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

pub(super) fn insert_result(
    f: &CompletionFixture,
    criterion: usize,
    result_revision: i64,
    result: &str,
) {
    let criterion_id = format!("criterion-{criterion}");
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let current_plan: i64 = connection
        .query_row(
            "SELECT current_plan_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let threshold = match result {
        "pass" => current_plan - 1,
        "fail" => current_plan,
        _ => panic!("test verifier helper supports pass/fail only"),
    };
    let predicate = serde_json::to_string(&CriterionPredicate::DatabasePredicate {
        version: PredicateVersion::V1,
        delegation_id: "delegation-1".into(),
        canonical_plan_revision_greater_than: threshold,
    })
    .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_outcome_criteria SET verifier_type='database_predicate',expected_predicate_json=?1,authoritative_source_kind='execass_plan_store' WHERE criterion_id=?2",
            params![predicate, criterion_id],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
    drop(connection);

    let state = revision(f);
    let idempotency_key = format!("ea306-verify-{criterion}-{result_revision}");
    let verifier_result_id =
        deterministic_verifier_result_id(&criterion_id, result_revision, &idempotency_key);
    let event_id = format!("ea306-verify-event-{criterion}-{result_revision}");
    let occurred_at = 1_800_000_001_000 + result_revision;
    let mut receipt = base_receipt(
        f,
        ReceiptFixtureInput {
            receipt_kind: ReceiptKind::Verifier,
            subject_kind: ReceiptSubjectKind::VerifierResult,
            subject_id: verifier_result_id.clone(),
            subject_revision: result_revision,
            event_id: event_id.clone(),
            expected_state_revision: state + 1,
            suffix: format!("verify-{criterion}-{result_revision}"),
            occurred_at,
        },
    );
    receipt.causation_id = format!("ea306-verify-cause-{criterion}-{result_revision}");
    let command = VerifyCriterionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("ea306-verify-correlation-{criterion}-{result_revision}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at,
        },
        delegation_id: "delegation-1".into(),
        criterion_id,
        expected_criteria_revision: 1,
        expected_state_revision: state,
        expected_result_revision: result_revision,
        verifier_result_id,
        outbox_event_id: event_id,
        receipt,
    };
    let CriterionVerificationOutcome::Recorded {
        result: recorded, ..
    } = f
        .fixture
        .store
        .verify_criterion(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("production verifier result must record")
    };
    assert_eq!(recorded.result.as_str(), result);
}

fn seed_artifact_authority(f: &CompletionFixture, bytes: &[u8]) -> String {
    let path = f.fixture.paths.root.join("ea306-artifact.bin");
    std::fs::write(&path, bytes).unwrap();
    let expected_sha256 = format!("{:x}", Sha256::digest(bytes));
    let predicate = serde_json::to_string(&CriterionPredicate::Artifact {
        version: PredicateVersion::V1,
        authority_link_id: "link-artifact".into(),
        expected_sha256: expected_sha256.clone(),
        expected_bytes: bytes.len() as i64,
    })
    .unwrap();
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_outcome_criteria SET verifier_type='artifact',expected_predicate_json=?1,authoritative_source_kind='artifact_store' WHERE criterion_id='criterion-1'",
            [predicate],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO agents(agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at)
            VALUES('agent-artifact','agent','Z:\carsinos','test','test','default',1,1);
            INSERT INTO sessions(session_id,session_key,agent_id,title,created_at,updated_at,closed_at)
            VALUES('session-artifact','session-key-artifact','agent-artifact','artifact session',1,1,NULL);
            INSERT INTO messages(message_id,session_id,source_channel,source_peer_id,source_message_id,role,content_text,content_format,created_at)
            VALUES('message-artifact','session-artifact','system',NULL,NULL,'tool','artifact','plain',1);
            "#,
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO attachments(attachment_id,message_id,kind,mime,sha256,bytes,local_path,created_at) VALUES('artifact-1','message-artifact','file','application/octet-stream',?1,?2,?3,1)",
            params![expected_sha256, bytes.len() as i64, path.to_string_lossy()],
        )
        .unwrap();
    drop(connection);

    let current = revision(f);
    let command = ObserveOrchestrationCommand {
        write: WriteContext {
            idempotency_key: "ea306-observe-artifact".into(),
            correlation_id: "ea306-observe-artifact-correlation".into(),
            causation_id: "ea306-observe-artifact-cause".into(),
            occurred_at: 1_800_000_000_100,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: current,
        resulting_state_revision: current + 1,
        observed_at: 1_800_000_000_100,
        references: vec![NewAuthorityLink {
            link_id: "link-artifact".into(),
            target: AuthorityLinkTarget::ArtifactAttachment {
                attachment_id: "artifact-1".into(),
            },
        }],
        ownership_checks: vec![AuthorityOwnershipCheck {
            link_id: "link-artifact".into(),
            owner_kind: AuthorityOwnerKind::Message,
            expected_owner_id: "message-artifact".into(),
        }],
        outbox_event: NewOutboxEvent {
            event_id: "ea306-observe-artifact-event".into(),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: current + 1,
            correlation_id: "ea306-observe-artifact-correlation".into(),
            causation_id: "ea306-observe-artifact-cause".into(),
            occurred_at: 1_800_000_000_100,
            safe_payload_json: "{}".into(),
            duplicate_identity: "ea306-observe-artifact".into(),
        },
    };
    assert!(matches!(
        f.fixture.store.observe_orchestration(&command).unwrap(),
        OrchestrationObservationOutcome::Linked(_)
    ));
    path.to_string_lossy().into_owned()
}

fn insert_artifact_result(f: &CompletionFixture, result_revision: i64, expected: &str) {
    let state = revision(f);
    let suffix = format!("artifact-{result_revision}");
    let idempotency_key = format!("ea306-verify-{suffix}");
    let verifier_result_id =
        deterministic_verifier_result_id("criterion-1", result_revision, &idempotency_key);
    let event_id = format!("ea306-verify-event-{suffix}");
    let occurred_at = 1_800_000_001_500 + result_revision;
    let mut receipt = base_receipt(
        f,
        ReceiptFixtureInput {
            receipt_kind: ReceiptKind::Verifier,
            subject_kind: ReceiptSubjectKind::VerifierResult,
            subject_id: verifier_result_id.clone(),
            subject_revision: result_revision,
            event_id: event_id.clone(),
            expected_state_revision: state + 1,
            suffix: format!("verify-{suffix}"),
            occurred_at,
        },
    );
    receipt.causation_id = format!("ea306-verify-cause-{suffix}");
    receipt.evidence = vec![artifact_evidence()];
    let command = VerifyCriterionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("ea306-verify-correlation-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at,
        },
        delegation_id: "delegation-1".into(),
        criterion_id: "criterion-1".into(),
        expected_criteria_revision: 1,
        expected_state_revision: state,
        expected_result_revision: result_revision,
        verifier_result_id,
        outbox_event_id: event_id,
        receipt,
    };
    let CriterionVerificationOutcome::Recorded { result, .. } = f
        .fixture
        .store
        .verify_criterion(&f.integrity, &f.redactor, &command)
        .unwrap()
    else {
        panic!("artifact verifier result must record")
    };
    assert_eq!(result.result.as_str(), expected);
}

fn artifact_evidence() -> ReceiptEvidenceInput {
    ReceiptEvidenceInput {
        authority_link_id: "link-artifact".into(),
        kind: AuthorityLinkKind::ArtifactAttachment,
        source_id: "artifact-1".into(),
        authoritative_revision: 0,
    }
}

fn seed_supersession_chain(f: &CompletionFixture, decision_result: &str, superseded_id: &str) {
    let predicate = serde_json::to_string(&CriterionPredicate::HumanBoundSupersession {
        version: PredicateVersion::V1,
        decision_id: "decision-supersede".into(),
        decision_revision: 1,
        superseded_criterion_id: superseded_id.into(),
    })
    .unwrap();
    let mut connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let tx = connection.transaction().unwrap();
    tx.execute(
        r#"INSERT INTO execass_decisions(
             decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,
             policy_revision,decision_kind,status,result,exact_presented_action_json,
             confirmed_logical_action_identity,manifest_digest,payload_digest,
             payload_and_material_operands_json,target_audience_path_json,
             connector_tool_identity,connector_tool_version,side_effect_envelope_json,
             recommendation,consequence,alternatives_json,idempotency_key,requested_at,
             resolved_at,resolved_by_authority_provenance_id
           ) VALUES('decision-supersede','delegation-1',1,1,1,1,'clarification','pending',NULL,
             '{}','supersede-action','supersede-manifest','supersede-payload','{}','{}',
             NULL,NULL,'{}','revise criteria','criterion is superseded','[]',
             'decision-supersede-idem',1800000000100,NULL,NULL)"#,
        [],
    )
    .unwrap();
    tx.execute(
        r#"INSERT INTO execass_authority_provenance(
             authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
             channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
             policy_revision,bound_decision_id,bound_decision_revision,bound_manifest_digest,
             bound_challenge_nonce_digest,evidence_digest,created_at
           ) VALUES('supersede-decision-owner','human_local','owner-1','local-ui','local-owner',
             'supersede-correlation','decision_resolution','{}',1,'decision-supersede',1,
             'supersede-manifest','supersede-nonce',
             'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1800000000101)"#,
        [],
    )
    .unwrap();
    tx.execute(
        "UPDATE execass_decisions SET status='resolved',result=?1,resolved_at=1800000000102,resolved_by_authority_provenance_id='supersede-decision-owner' WHERE decision_id='decision-supersede'",
        [decision_result],
    )
    .unwrap();
    tx.execute(
        r#"INSERT INTO execass_authority_provenance(
             authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
             channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
             policy_revision,evidence_digest,created_at
           ) VALUES('supersede-amendment-owner','human_local','owner-1','local-ui','local-owner',
             'supersede-correlation','action_specific_owner_amendment',
             '{"delegation_id":"delegation-1","delegation_revision":2,"plan_revision":2}',1,
             'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',1800000000102)"#,
        [],
    )
    .unwrap();
    tx.execute(
        r#"INSERT INTO execass_plans(
             plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,
             plan_summary,resolved_leaf_manifest_json,manifest_digest,
             created_by_authority_provenance_id,created_at
           ) VALUES('plan-superseded','delegation-1',2,2,1,'owner revision','[]',
             'supersede-result-manifest','supersede-amendment-owner',1800000000102)"#,
        [],
    )
    .unwrap();
    tx.execute(
        "UPDATE execass_criteria_sets SET disposition='superseded' WHERE delegation_id='delegation-1' AND criteria_revision=1",
        [],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO execass_criteria_sets(criteria_set_id,delegation_id,criteria_revision,parent_criteria_revision,disposition,created_at) VALUES('criteria-set-supersession','delegation-1',2,1,'current',1800000000102)",
        [],
    )
    .unwrap();
    tx.execute(
        r#"INSERT INTO execass_outcome_criteria(
             criterion_id,delegation_id,criteria_revision,criterion_key,description,material,
             verifier_type,expected_predicate_json,authoritative_source_kind,created_at
           ) VALUES('criterion-supersession','delegation-1',2,'criterion-supersession',
             'owner explicitly superseded prior criterion',1,'human_bound_supersession',?1,
             'human_bound_supersession_store',1800000000102)"#,
        [predicate],
    )
    .unwrap();
    tx.execute(
        r#"INSERT INTO execass_plan_amendments(
             amendment_id,delegation_id,amendment_revision,superseded_plan_revision,
             resulting_plan_revision,normalized_amendment,intake_evidence_json,
             authority_provenance_id,created_at
           ) VALUES('amendment-supersession','delegation-1',1,1,2,
             'owner superseded criterion','{}','supersede-amendment-owner',1800000000102)"#,
        [],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO execass_amendment_criteria_links(amendment_id,delegation_id,superseded_criteria_revision,resulting_criteria_revision) VALUES('amendment-supersession','delegation-1',1,2)",
        [],
    )
    .unwrap();
    tx.execute(
        "UPDATE execass_delegations SET phase='planning',state_revision=2,current_plan_revision=2,current_criteria_revision=2,updated_at=1800000000102 WHERE delegation_id='delegation-1' AND state_revision=1",
        [],
    )
    .unwrap();
    tx.commit().unwrap();
}

fn seed_recovery_history(f: &CompletionFixture, terminal_effect: bool) {
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    let (continuation_id, action_id): (String, String) = connection
        .query_row(
            "SELECT continuation_id,action_id FROM execass_continuations WHERE delegation_id='delegation-1' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let claim_event: String = connection
        .query_row(
            "SELECT event_id FROM execass_outbox_events WHERE aggregate_id='delegation-1' ORDER BY global_sequence LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE execass_action_branches SET status='terminal',terminal_at=1800000000200,updated_at=1800000000200 WHERE action_id=?1",
            [&action_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE execass_continuations SET status='terminal',completed_at=1800000000200,updated_at=1800000000200 WHERE continuation_id=?1",
            [&continuation_id],
        )
        .unwrap();
    let state = if terminal_effect {
        "succeeded"
    } else {
        "outcome_unknown"
    };
    connection
        .execute(
            r#"INSERT INTO execass_logical_effects(
                 logical_effect_id,delegation_id,continuation_id,action_kind,operation_reversible,
                 declared_recovery_safe_boundary,state,internal_idempotency_key,provider_identity,
                 provider_idempotency_key,reconciliation_key,manifest_digest,payload_digest,
                 outcome_json,created_at,updated_at
               ) VALUES('effect-recovery','delegation-1',?1,
                 'public_or_externally_consequential_communication',0,'independent_absence',?2,
                 'effect-recovery-idem','provider','provider-idem',NULL,'manifest','payload','{}',
                 1800000000200,1800000000201)"#,
            params![continuation_id, state],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_provider_attempts(
                 attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,claim_event_id,
                 claim_receipt_id,attempt_number,fencing_token,host_generation,host_instance_id,
                 runtime_fencing_token,status,provider_request_digest,provider_response_digest,
                 provider_error_class,remote_effect_id,started_at,finished_at
               ) VALUES('attempt-recovery','delegation-1','effect-recovery',?1,?2,?3,
                 'historical-claim-receipt',1,1,1,'ea306-host',1,?4,'request','response',
                 NULL,NULL,1800000000200,1800000000201)"#,
            params![continuation_id, action_id, claim_event, state],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_recovery_episodes(
                 recovery_episode_id,delegation_id,logical_effect_id,initial_attempt_id,action_id,
                 manifest_digest,normalized_intent_digest,effective_authority_digest,
                 accepted_confirmation_grant_id,policy_json,policy_digest,opened_at
               ) VALUES('episode-recovery','delegation-1','effect-recovery','attempt-recovery',?1,
                 'manifest','sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 NULL,'{}','sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                 1800000000201)"#,
            [action_id],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_outbox_events(
                 event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,
                 occurred_at,schema_version,safe_payload_json,duplicate_identity,published_at
               ) VALUES('recovery-history-event','execass.v1.recovery.updated','delegation-1',1,
                 'recovery-history-correlation','recovery-history-cause',1800000000202,'v1','{}',
                 'recovery-history-idem',NULL)"#,
            [],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_recovery_evaluations(
                 recovery_evaluation_id,recovery_episode_id,delegation_id,logical_effect_id,
                 predecessor_attempt_id,evaluation_revision,recovery_state_revision,
                 objective_facts_json,objective_facts_digest,directive,directive_json,
                 directive_digest,not_before_ms,outbox_event_id,evaluated_at
               ) VALUES('evaluation-recovery','episode-recovery','delegation-1','effect-recovery',
                 'attempt-recovery',1,1,'{}',
                 'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
                 'wait_backoff','{}',
                 'sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
                 1800000000300,'recovery-history-event',1800000000202)"#,
            [],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
}

fn receipt_heads(f: &CompletionFixture) -> (i64, Option<String>, i64, Option<String>) {
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let (global_count, global_head) = connection
        .query_row(
            "SELECT receipt_count,receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let (delegation_count, delegation_head) = connection
        .query_row(
            "SELECT receipt_chain_count,receipt_chain_head_digest FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    (global_count, global_head, delegation_count, delegation_head)
}

struct ReceiptFixtureInput {
    receipt_kind: ReceiptKind,
    subject_kind: ReceiptSubjectKind,
    subject_id: String,
    subject_revision: i64,
    event_id: String,
    expected_state_revision: i64,
    suffix: String,
    occurred_at: i64,
}

fn base_receipt(f: &CompletionFixture, input: ReceiptFixtureInput) -> AppendReceiptCommand {
    let (global_count, global_head, delegation_count, delegation_head) = receipt_heads(f);
    AppendReceiptCommand {
        receipt_id: format!("ea306-receipt-{}", input.suffix),
        transaction_id: format!("ea306-transaction-{}", input.suffix),
        state_root_generation: 1,
        delegation_id: "delegation-1".into(),
        expected_state_revision: input.expected_state_revision,
        expected_global_count: global_count,
        expected_global_head_digest: global_head,
        expected_delegation_count: delegation_count,
        expected_delegation_head_digest: delegation_head,
        receipt_kind: input.receipt_kind,
        subject: ReceiptSubject {
            kind: input.subject_kind,
            subject_id: input.subject_id,
            revision: input.subject_revision,
        },
        causation_id: format!("ea306-cause-{}", input.suffix),
        causation_event_id: input.event_id,
        actor: ReceiptActorBinding {
            actor_type: ActorType::Runtime,
            actor_identity: SafeText::new("ea306-assessor", &[]).unwrap(),
            authority_provenance_id: "ea306-assessor-authority".into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "ea306-host".into(),
            fencing_token: 1,
        },
        key: f.key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::new("ExecAss terminal truth recorded", &[]).unwrap(),
        occurred_at: input.occurred_at,
        committed_at: input.occurred_at,
    }
}

pub(super) fn assessment_command(f: &CompletionFixture, suffix: &str) -> AssessCompletionCommand {
    let state = revision(f);
    let criteria_revision: i64 = Connection::open(&f.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT current_criteria_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let idempotency_key = format!("ea306-assess-{suffix}");
    let assessment_id = deterministic_completion_assessment_id("delegation-1", 1, &idempotency_key);
    let event_id = deterministic_completion_event_id(&assessment_id);
    let occurred_at = 1_800_000_010_000;
    let mut receipt = base_receipt(
        f,
        ReceiptFixtureInput {
            receipt_kind: ReceiptKind::Completion,
            subject_kind: ReceiptSubjectKind::CompletionAssessment,
            subject_id: assessment_id,
            subject_revision: 1,
            event_id,
            expected_state_revision: state + 1,
            suffix: suffix.into(),
            occurred_at,
        },
    );
    receipt.causation_id = format!("ea306-cause-{suffix}");
    AssessCompletionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("ea306-correlation-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: state,
        expected_criteria_revision: criteria_revision,
        expected_assessment_revision: 1,
        receipt,
    }
}

fn correction_command(f: &CompletionFixture, suffix: &str) -> RecordLateTerminalCorrectionCommand {
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let terminal_assessment_id: String = connection
        .query_row(
            "SELECT assessment_id FROM execass_completion_assessments ORDER BY assessment_revision DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let state = revision(f);
    let correction_revision = 1;
    let idempotency_key = format!("ea306-correct-{suffix}");
    let correction_id = deterministic_terminal_correction_id(
        &terminal_assessment_id,
        correction_revision,
        &idempotency_key,
    );
    let event_id = deterministic_terminal_correction_event_id(&correction_id);
    let occurred_at = 1_800_000_020_000;
    let mut receipt = base_receipt(
        f,
        ReceiptFixtureInput {
            receipt_kind: ReceiptKind::TerminalCorrection,
            subject_kind: ReceiptSubjectKind::TerminalCorrection,
            subject_id: correction_id,
            subject_revision: correction_revision,
            event_id,
            expected_state_revision: state,
            suffix: format!("correction-{suffix}"),
            occurred_at,
        },
    );
    receipt.causation_id = format!("ea306-correction-cause-{suffix}");
    RecordLateTerminalCorrectionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("ea306-correction-correlation-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: state,
        expected_correction_revision: correction_revision,
        receipt,
    }
}

pub(super) fn assess(
    f: &CompletionFixture,
    command: &AssessCompletionCommand,
) -> CompletionAssessmentOutcome {
    f.fixture
        .store
        .assess_completion_atomically(&f.integrity, &f.redactor, command)
        .unwrap()
}

#[test]
fn latest_authoritative_results_select_completed_partial_and_failed() {
    let completed = setup(&[true], false);
    insert_result(&completed, 1, 1, "fail");
    insert_result(&completed, 1, 2, "pass");
    let command = assessment_command(&completed, "completed");
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } = assess(&completed, &command)
    else {
        panic!("latest PASS must complete")
    };
    assert_eq!(assessment.kind, CompletionAssessmentKind::Completed);
    assert!(matches!(
        assess(&completed, &command),
        CompletionAssessmentOutcome::Replayed { .. }
    ));

    let partial = setup(&[true, true], false);
    insert_result(&partial, 1, 1, "pass");
    insert_result(&partial, 2, 1, "fail");
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } =
        assess(&partial, &assessment_command(&partial, "partial"))
    else {
        panic!("useful evidence plus exact unmet criterion must be partial")
    };
    assert_eq!(
        assessment.kind,
        CompletionAssessmentKind::PartiallyCompleted
    );
    assert_eq!(assessment.material_pass_count, 1);
    assert_eq!(assessment.material_fail_count, 1);
    assert!(assessment
        .exact_unmet_portion
        .unwrap()
        .contains("criterion-2"));

    let failed = setup(&[true], false);
    insert_result(&failed, 1, 1, "pass");
    insert_result(&failed, 1, 2, "fail");
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } =
        assess(&failed, &assessment_command(&failed, "failed"))
    else {
        panic!("latest failed criterion must defeat child/run success")
    };
    assert_eq!(assessment.kind, CompletionAssessmentKind::Failed);
    assert!(!assessment.useful_outcome);
}

#[test]
fn only_exact_owner_revise_amendment_supersession_satisfies_a_material_criterion() {
    let valid = setup(&[true], false);
    seed_supersession_chain(&valid, "revise", "criterion-1");
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } =
        assess(&valid, &assessment_command(&valid, "valid-supersession"))
    else {
        panic!("exact owner revise chain must satisfy its supersession marker")
    };
    assert_eq!(assessment.kind, CompletionAssessmentKind::Completed);
    assert_eq!(assessment.material_pass_count, 0);
    assert!(assessment
        .assessment_json
        .contains("material_superseded_count\":1"));
    assert!(assessment
        .assessment_json
        .contains("supersede-decision-owner"));
    assert!(assessment
        .assessment_json
        .contains("supersede-amendment-owner"));

    let wrong_result = setup(&[true], false);
    seed_supersession_chain(&wrong_result, "decline", "criterion-1");
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } = assess(
        &wrong_result,
        &assessment_command(&wrong_result, "wrong-result"),
    ) else {
        panic!("non-revise chain must remain an unmet terminal criterion")
    };
    assert_eq!(assessment.kind, CompletionAssessmentKind::Failed);
    assert!(assessment
        .assessment_json
        .contains("material_superseded_count\":0"));

    let wrong_criterion = setup(&[true], false);
    seed_supersession_chain(&wrong_criterion, "revise", "criterion-does-not-exist");
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } = assess(
        &wrong_criterion,
        &assessment_command(&wrong_criterion, "wrong-criterion"),
    ) else {
        panic!("wrong criterion binding must remain unmet")
    };
    assert_eq!(assessment.kind, CompletionAssessmentKind::Failed);
}

#[test]
fn unknown_zero_material_and_active_paths_never_complete() {
    let unknown = setup(&[true], false);
    let CompletionAssessmentOutcome::Terminalized { assessment, .. } =
        assess(&unknown, &assessment_command(&unknown, "unknown"))
    else {
        panic!("unknown with no useful result and no path must terminate honestly")
    };
    assert_eq!(assessment.kind, CompletionAssessmentKind::Failed);
    assert_ne!(assessment.kind, CompletionAssessmentKind::Completed);

    let zero = setup(&[false], false);
    assert!(matches!(
        assess(&zero, &assessment_command(&zero, "zero")),
        CompletionAssessmentOutcome::NotTerminal { blockers, .. }
            if blockers == vec!["zero_material_criteria"]
    ));
    assert_eq!(
        table_count(&zero.fixture.paths, "execass_completion_assessments"),
        0
    );

    let active = setup(&[true], true);
    insert_result(&active, 1, 1, "pass");
    let verifier_receipts = table_count(&active.fixture.paths, "execass_receipts");
    assert!(matches!(
        assess(&active, &assessment_command(&active, "active")),
        CompletionAssessmentOutcome::NotTerminal { blockers, .. }
            if blockers.contains(&"active_action_branch".to_string())
                && blockers.contains(&"active_continuation".to_string())
    ));
    assert_eq!(
        table_count(&active.fixture.paths, "execass_completion_assessments"),
        0
    );
    assert_eq!(
        table_count(&active.fixture.paths, "execass_receipts"),
        verifier_receipts
    );
}

#[test]
fn orphan_verifier_result_cannot_terminalize() {
    let f = setup(&[true], false);
    Connection::open(&f.fixture.paths.db_path)
        .unwrap()
        .execute(
            r#"INSERT INTO execass_verifier_results(
                 verifier_result_id,delegation_id,criterion_id,result_revision,result,
                 evidence_refs_json,evidence_digest,verifier_identity,verified_at
               ) VALUES('orphan-pass','delegation-1','criterion-1',1,'pass','[]',
                 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'carsinos.storage.criterion-verifier.v1',1800000001000)"#,
            [],
        )
        .unwrap();
    assert!(matches!(
        assess(&f, &assessment_command(&f, "orphan-pass")),
        CompletionAssessmentOutcome::AuthoritativeStateInvalid {
            reason: "verifier result lacks one exact receipt-chain binding"
        }
    ));
    assert_eq!(
        table_count(&f.fixture.paths, "execass_completion_assessments"),
        0
    );
}

#[test]
fn historical_terminal_recovery_does_not_block_but_active_unknown_recovery_does() {
    let historical = setup(&[true], true);
    insert_result(&historical, 1, 1, "pass");
    seed_recovery_history(&historical, true);
    assert!(matches!(
        assess(
            &historical,
            &assessment_command(&historical, "historical-recovery")
        ),
        CompletionAssessmentOutcome::Terminalized {
            assessment: CompletionAssessmentRecord {
                kind: CompletionAssessmentKind::Completed,
                ..
            },
            ..
        }
    ));

    let active = setup(&[true], true);
    insert_result(&active, 1, 1, "pass");
    seed_recovery_history(&active, false);
    assert!(matches!(
        assess(&active, &assessment_command(&active, "active-recovery")),
        CompletionAssessmentOutcome::NotTerminal { blockers, .. }
            if blockers.contains(&"active_or_uncertain_effect".to_string())
                && blockers.contains(&"active_recovery_path".to_string())
    ));
    assert_eq!(
        table_count(&active.fixture.paths, "execass_completion_assessments"),
        0
    );
}

#[test]
fn stale_and_integrity_failure_leave_zero_terminal_writes() {
    let stale = setup(&[true], false);
    insert_result(&stale, 1, 1, "pass");
    let mut stale_command = assessment_command(&stale, "stale");
    stale_command.expected_state_revision += 1;
    stale_command.receipt.expected_state_revision += 1;
    assert!(matches!(
        assess(&stale, &stale_command),
        CompletionAssessmentOutcome::Stale { .. }
    ));
    assert_eq!(
        table_count(&stale.fixture.paths, "execass_completion_assessments"),
        0
    );

    let stale_assessment = setup(&[true], false);
    insert_result(&stale_assessment, 1, 1, "pass");
    let mut stale_assessment_command = assessment_command(&stale_assessment, "stale-assessment");
    stale_assessment_command.expected_assessment_revision = 2;
    let stale_assessment_id = deterministic_completion_assessment_id(
        "delegation-1",
        2,
        &stale_assessment_command.write.idempotency_key,
    );
    stale_assessment_command.receipt.subject.subject_id = stale_assessment_id.clone();
    stale_assessment_command.receipt.subject.revision = 2;
    stale_assessment_command.receipt.causation_event_id =
        deterministic_completion_event_id(&stale_assessment_id);
    assert!(matches!(
        assess(&stale_assessment, &stale_assessment_command),
        CompletionAssessmentOutcome::StaleAssessmentRevision {
            current_assessment_revision: 0
        }
    ));
    assert_eq!(
        table_count(
            &stale_assessment.fixture.paths,
            "execass_completion_assessments"
        ),
        0
    );

    let tampered = setup(&[true], false);
    insert_result(&tampered, 1, 1, "pass");
    let before_assessments = table_count(&tampered.fixture.paths, "execass_completion_assessments");
    let before_receipts = table_count(&tampered.fixture.paths, "execass_receipts");
    let before_outbox = table_count(&tampered.fixture.paths, "execass_outbox_events");
    let connection = Connection::open(&tampered.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_receipt_keys SET created_at=created_at+1 WHERE key_generation=1",
            [],
        )
        .unwrap();
    let command = assessment_command(&tampered, "tampered");
    assert!(tampered
        .fixture
        .store
        .assess_completion_atomically(&tampered.integrity, &tampered.redactor, &command)
        .is_err());
    assert_eq!(
        table_count(&tampered.fixture.paths, "execass_completion_assessments"),
        before_assessments
    );
    assert_eq!(
        table_count(&tampered.fixture.paths, "execass_receipts"),
        before_receipts
    );
    assert_eq!(
        table_count(&tampered.fixture.paths, "execass_outbox_events"),
        before_outbox
    );
}

#[test]
fn late_contrary_evidence_appends_correction_without_rewriting_terminal_history() {
    let f = setup(&[true], false);
    insert_result(&f, 1, 1, "pass");
    let completion = assessment_command(&f, "late");
    let CompletionAssessmentOutcome::Terminalized {
        assessment,
        receipt,
        ..
    } = assess(&f, &completion)
    else {
        panic!("baseline completion")
    };
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let before: (String, i64, String, Vec<u8>) = connection
        .query_row(
            r#"SELECT d.phase,d.terminal_at,d.completion_assessment_json,r.canonical_payload
               FROM execass_delegations d JOIN execass_receipts r ON r.receipt_id=?1
               WHERE d.delegation_id='delegation-1'"#,
            [&receipt.receipt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    drop(connection);

    insert_result(&f, 1, 2, "fail");
    assert!(matches!(
        assess(&f, &completion),
        CompletionAssessmentOutcome::Replayed { .. }
    ));
    let correction = correction_command(&f, "late");
    let mut wrong_evidence = correction.clone();
    wrong_evidence.receipt.evidence.push(ReceiptEvidenceInput {
        authority_link_id: "not-late-evidence".into(),
        kind: AuthorityLinkKind::ArtifactAttachment,
        source_id: "not-late-source".into(),
        authoritative_revision: 1,
    });
    assert!(f
        .fixture
        .store
        .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &wrong_evidence)
        .is_err());
    assert_eq!(
        table_count(&f.fixture.paths, "execass_terminal_corrections"),
        0
    );
    let LateTerminalCorrectionOutcome::Recorded {
        correction: record, ..
    } = f
        .fixture
        .store
        .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &correction)
        .unwrap()
    else {
        panic!("late FAIL must append a correction")
    };
    assert_eq!(record.terminal_assessment_id, assessment.assessment_id);
    assert!(record
        .contrary_evidence_json
        .contains("latest_disposition\":\"fail"));
    assert!(matches!(
        f.fixture
            .store
            .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &correction)
            .unwrap(),
        LateTerminalCorrectionOutcome::Replayed { .. }
    ));
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let after: (String, i64, String, Vec<u8>) = connection
        .query_row(
            r#"SELECT d.phase,d.terminal_at,d.completion_assessment_json,r.canonical_payload
               FROM execass_delegations d JOIN execass_receipts r ON r.receipt_id=?1
               WHERE d.delegation_id='delegation-1'"#,
            [&receipt.receipt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(
        table_count(&f.fixture.paths, "execass_terminal_corrections"),
        1
    );

    let mut conflict = correction.clone();
    conflict.receipt.receipt_id = "changed-correction-receipt".into();
    assert!(matches!(
        f.fixture
            .store
            .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &conflict)
            .unwrap(),
        LateTerminalCorrectionOutcome::Conflict { .. }
    ));

    let stale_correction = setup(&[true], false);
    insert_result(&stale_correction, 1, 1, "pass");
    let completion = assessment_command(&stale_correction, "stale-correction-base");
    assert!(matches!(
        assess(&stale_correction, &completion),
        CompletionAssessmentOutcome::Terminalized { .. }
    ));
    insert_result(&stale_correction, 1, 2, "fail");
    let mut command = correction_command(&stale_correction, "stale-correction");
    command.expected_correction_revision = 2;
    command.receipt.subject.revision = 2;
    assert!(matches!(
        stale_correction
            .fixture
            .store
            .record_late_terminal_correction_atomically(
                &stale_correction.integrity,
                &stale_correction.redactor,
                &command
            )
            .unwrap(),
        LateTerminalCorrectionOutcome::StaleCorrectionRevision {
            current_correction_revision: 0
        }
    ));
}

#[test]
fn late_artifact_loss_requires_and_persists_exact_verifier_evidence_lineage() {
    let bytes = b"authoritative completion artifact";
    let f = setup(&[true], false);
    let artifact_path = seed_artifact_authority(&f, bytes);
    insert_artifact_result(&f, 1, "pass");
    let mut completion = assessment_command(&f, "artifact-lineage");
    completion.receipt.evidence = vec![artifact_evidence()];
    assert!(matches!(
        assess(&f, &completion),
        CompletionAssessmentOutcome::Terminalized { .. }
    ));

    std::fs::write(&artifact_path, b"contrary artifact bytes").unwrap();
    insert_artifact_result(&f, 2, "fail");

    let missing = correction_command(&f, "artifact-missing-evidence");
    assert!(f
        .fixture
        .store
        .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &missing)
        .is_err());
    assert_eq!(
        table_count(&f.fixture.paths, "execass_terminal_corrections"),
        0
    );

    let mut exact = correction_command(&f, "artifact-exact-evidence");
    exact.receipt.evidence = vec![artifact_evidence()];
    let receipt_id = exact.receipt.receipt_id.clone();
    assert!(matches!(
        f.fixture
            .store
            .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &exact)
            .unwrap(),
        LateTerminalCorrectionOutcome::Recorded { .. }
    ));
    let connection = Connection::open(&f.fixture.paths.db_path).unwrap();
    let stored: (String, String, String, i64) = connection
        .query_row(
            r#"SELECT authority_link_id,authority_kind,source_id,authoritative_revision
               FROM execass_receipt_evidence_refs WHERE receipt_id=?1"#,
            [&receipt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        (
            "link-artifact".into(),
            "artifact_attachment".into(),
            "artifact-1".into(),
            0
        )
    );
}

#[test]
fn agreeing_late_revision_is_a_noop() {
    let f = setup(&[true], false);
    insert_result(&f, 1, 1, "pass");
    let completion = assessment_command(&f, "agree");
    assert!(matches!(
        assess(&f, &completion),
        CompletionAssessmentOutcome::Terminalized { .. }
    ));
    insert_result(&f, 1, 2, "pass");
    let correction = correction_command(&f, "agree");
    let before_receipts = table_count(&f.fixture.paths, "execass_receipts");
    assert!(matches!(
        f.fixture
            .store
            .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &correction)
            .unwrap(),
        LateTerminalCorrectionOutcome::NoContraryEvidence { .. }
    ));
    assert_eq!(
        table_count(&f.fixture.paths, "execass_terminal_corrections"),
        0
    );
    assert_eq!(
        table_count(&f.fixture.paths, "execass_receipts"),
        before_receipts
    );
}
