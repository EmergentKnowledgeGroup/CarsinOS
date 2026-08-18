use super::tests::{fixture, foundation, table_count, Fixture};
use super::verifier::*;
use super::*;
use crate::open_sqlite_connection;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

struct VerifierFixture {
    fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    redactor: ReceiptRedactor,
    key: ReceiptKeyRef,
}

fn setup(
    verifier_type: VerifierType,
    source_kind: &str,
    predicate_json: String,
) -> VerifierFixture {
    let fixture = fixture();
    let mut command = foundation();
    command.initial_continuation = None;
    command.outcome_criteria.truncate(1);
    command.outcome_criteria[0].criterion_id = "criterion-1".into();
    command.outcome_criteria[0].criterion_key = "criterion-1".into();
    command.outcome_criteria[0].verifier_type = verifier_type;
    command.outcome_criteria[0].authoritative_source_kind = source_kind.into();
    command.outcome_criteria[0].expected_predicate_json = predicate_json;
    fixture.store.create_foundation(&command).unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO execass_runtime_host_generations(
          generation,ownership_scope,state_root_generation,installation_identity,
          os_user_identity_digest,host_instance_id,started_at
        ) VALUES(1,'execass',1,'ea305-installation',
          'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
          'ea305-host',1800000000001);
        INSERT INTO execass_runtime_host_leases(
          lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
        ) VALUES('ea305-lease','execass',1,'ea305-host',1,1800000000001,9999999999999);
        INSERT INTO execass_authority_provenance(
          authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
          channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
          policy_revision,evidence_digest,created_at
        ) VALUES('ea305-verifier-authority','runtime','ea305-verifier','criterion-verifier',
          'local-runtime-fence','ea305-verifier-bootstrap','runtime_safety_state','{}',1,
          'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
          1800000000001);
        "#,
    )
    .unwrap();
    drop(conn);
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("ea305-receipt-key")
        .unwrap();
    VerifierFixture {
        fixture,
        integrity,
        redactor: ReceiptRedactor::new(&["ea305-secret"]).unwrap(),
        key,
    }
}

fn predicate(value: CriterionPredicate) -> String {
    serde_json::to_string(&value).unwrap()
}

fn state_revision(f: &VerifierFixture) -> i64 {
    Connection::open(&f.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn observe(
    f: &VerifierFixture,
    suffix: &str,
    target: AuthorityLinkTarget,
    ownership: Option<(AuthorityOwnerKind, &str)>,
) {
    let current = state_revision(f);
    let link_id = format!("link-{suffix}");
    let command = ObserveOrchestrationCommand {
        write: WriteContext {
            idempotency_key: format!("observe-{suffix}"),
            correlation_id: format!("observe-correlation-{suffix}"),
            causation_id: format!("observe-causation-{suffix}"),
            occurred_at: 1_800_000_000_010 + current,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: current,
        resulting_state_revision: current + 1,
        observed_at: 1_800_000_000_010 + current,
        references: vec![NewAuthorityLink {
            link_id: link_id.clone(),
            target,
        }],
        ownership_checks: ownership
            .map(|(owner_kind, expected_owner_id)| AuthorityOwnershipCheck {
                link_id,
                owner_kind,
                expected_owner_id: expected_owner_id.into(),
            })
            .into_iter()
            .collect(),
        outbox_event: NewOutboxEvent {
            event_id: format!("observe-event-{suffix}"),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: current + 1,
            correlation_id: format!("observe-correlation-{suffix}"),
            causation_id: format!("observe-causation-{suffix}"),
            occurred_at: 1_800_000_000_010 + current,
            safe_payload_json: "{}".into(),
            duplicate_identity: format!("observe-{suffix}"),
        },
    };
    assert!(matches!(
        f.fixture.store.observe_orchestration(&command).unwrap(),
        OrchestrationObservationOutcome::Linked(_)
    ));
}

fn receipt_evidence(
    link_id: &str,
    kind: AuthorityLinkKind,
    source_id: &str,
) -> Vec<ReceiptEvidenceInput> {
    vec![ReceiptEvidenceInput {
        authority_link_id: link_id.into(),
        kind,
        source_id: source_id.into(),
        authoritative_revision: 0,
    }]
}

fn command(
    f: &VerifierFixture,
    suffix: &str,
    result_revision: i64,
    evidence: Vec<ReceiptEvidenceInput>,
) -> VerifyCriterionCommand {
    let conn = Connection::open(&f.fixture.paths.db_path).unwrap();
    let (revision, delegation_count, delegation_head): (i64, i64, Option<String>) = conn
        .query_row(
            "SELECT state_revision,receipt_chain_count,receipt_chain_head_digest FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        )
        .unwrap();
    let (global_count, global_head): (i64, Option<String>) = conn
        .query_row(
            "SELECT receipt_count,receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?,row.get(1)?)),
        )
        .unwrap();
    let idempotency_key = format!("verify-{suffix}");
    let result_id =
        deterministic_verifier_result_id("criterion-1", result_revision, &idempotency_key);
    let occurred_at = 1_800_000_001_000 + result_revision;
    let event_id = format!("verify-event-{suffix}");
    VerifyCriterionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("verify-correlation-{suffix}"),
            causation_id: format!("verify-causation-{suffix}"),
            occurred_at,
        },
        delegation_id: "delegation-1".into(),
        criterion_id: "criterion-1".into(),
        expected_criteria_revision: 1,
        expected_state_revision: revision,
        expected_result_revision: result_revision,
        verifier_result_id: result_id.clone(),
        outbox_event_id: event_id.clone(),
        receipt: AppendReceiptCommand {
            receipt_id: format!("verify-receipt-{suffix}"),
            transaction_id: format!("verify-transaction-{suffix}"),
            state_root_generation: 1,
            delegation_id: "delegation-1".into(),
            expected_state_revision: revision + 1,
            expected_global_count: global_count,
            expected_global_head_digest: global_head,
            expected_delegation_count: delegation_count,
            expected_delegation_head_digest: delegation_head,
            receipt_kind: ReceiptKind::Verifier,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::VerifierResult,
                subject_id: result_id,
                revision: result_revision,
            },
            causation_id: format!("verify-causation-{suffix}"),
            causation_event_id: event_id,
            actor: ReceiptActorBinding {
                actor_type: ActorType::Runtime,
                actor_identity: SafeText::new("ea305-verifier", &[]).unwrap(),
                authority_provenance_id: "ea305-verifier-authority".into(),
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: "ea305-host".into(),
                fencing_token: 1,
            },
            key: f.key.clone(),
            rotation: None,
            evidence,
            redacted_summary: SafeText::summary("criterion independently verified", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    }
}

fn verify(f: &VerifierFixture, command: &VerifyCriterionCommand) -> CriterionVerificationOutcome {
    f.fixture
        .store
        .verify_criterion(&f.integrity, &f.redactor, command)
        .unwrap()
}

fn recorded_result(outcome: CriterionVerificationOutcome) -> VerifierResultRecord {
    match outcome {
        CriterionVerificationOutcome::Recorded { result, .. } => result,
        other => panic!("expected recorded verifier result, got {other:?}"),
    }
}

fn seed_artifact(f: &VerifierFixture, bytes: &[u8]) -> (String, String) {
    let path = f.fixture.paths.root.join("ea305-artifact.bin");
    std::fs::write(&path, bytes).unwrap();
    let sha = format!("{:x}", Sha256::digest(bytes));
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO agents(agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at)
        VALUES('agent-1','agent','Z:\carsinos','test','test','default',1,1);
        INSERT INTO sessions(session_id,session_key,agent_id,title,created_at,updated_at,closed_at)
        VALUES('session-1','session-key-1','agent-1','artifact session',1,1,NULL);
        INSERT INTO messages(message_id,session_id,source_channel,source_peer_id,source_message_id,role,content_text,content_format,created_at)
        VALUES('message-artifact','session-1','system',NULL,NULL,'tool','artifact','plain',1);
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO attachments(attachment_id,message_id,kind,mime,sha256,bytes,local_path,created_at) VALUES('artifact-1','message-artifact','file','application/octet-stream',?1,?2,?3,1)",
        params![sha,bytes.len() as i64,path.to_string_lossy()],
    )
    .unwrap();
    drop(conn);
    observe(
        f,
        "artifact",
        AuthorityLinkTarget::ArtifactAttachment {
            attachment_id: "artifact-1".into(),
        },
        Some((AuthorityOwnerKind::Message, "message-artifact")),
    );
    (path.to_string_lossy().into_owned(), sha)
}

#[test]
fn artifact_pass_then_wrong_or_missing_bytes_append_contrary_revisions() {
    let bytes = b"authoritative artifact";
    let expected_sha = format!("{:x}", Sha256::digest(bytes));
    let f = setup(
        VerifierType::Artifact,
        "artifact_store",
        predicate(CriterionPredicate::Artifact {
            version: PredicateVersion::V1,
            authority_link_id: "link-artifact".into(),
            expected_sha256: expected_sha.clone(),
            expected_bytes: bytes.len() as i64,
        }),
    );
    let (path, _) = seed_artifact(&f, bytes);
    let evidence = receipt_evidence(
        "link-artifact",
        AuthorityLinkKind::ArtifactAttachment,
        "artifact-1",
    );
    let first = recorded_result(verify(
        &f,
        &command(&f, "artifact-pass", 1, evidence.clone()),
    ));
    assert_eq!(first.result, CriterionVerificationResult::Pass);

    std::fs::write(&path, b"wrong artifact").unwrap();
    let second = recorded_result(verify(
        &f,
        &command(&f, "artifact-wrong", 2, evidence.clone()),
    ));
    assert_eq!(second.result, CriterionVerificationResult::Fail);
    std::fs::remove_file(&path).unwrap();
    let third = recorded_result(verify(&f, &command(&f, "artifact-missing", 3, evidence)));
    assert_eq!(third.result, CriterionVerificationResult::Fail);
    assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 3);
}

#[test]
fn a_succeeded_process_cannot_self_certify_a_separate_artifact() {
    let bytes = b"expected but absent";
    let f = setup(
        VerifierType::Artifact,
        "artifact_store",
        predicate(CriterionPredicate::Artifact {
            version: PredicateVersion::V1,
            authority_link_id: "link-artifact".into(),
            expected_sha256: format!("{:x}", Sha256::digest(bytes)),
            expected_bytes: bytes.len() as i64,
        }),
    );
    let (path, _) = seed_artifact(&f, bytes);
    std::fs::remove_file(path).unwrap();
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO runs(run_id,session_id,status,model_provider,model_id,started_at,ended_at,error_text,usage_json,created_at) VALUES('self-report-run','session-1','succeeded','test','test',1,2,NULL,NULL,1)",
        [],
    )
    .unwrap();
    drop(conn);
    let result = recorded_result(verify(
        &f,
        &command(
            &f,
            "self-report",
            1,
            receipt_evidence(
                "link-artifact",
                AuthorityLinkKind::ArtifactAttachment,
                "artifact-1",
            ),
        ),
    ));
    assert_eq!(result.result, CriterionVerificationResult::Fail);
}

#[test]
fn claimed_remote_bounce_cannot_manufacture_a_result() {
    let raw = serde_json::json!({
        "kind":"delivery",
        "version":"v1",
        "predicate":{
            "source":"remote_provider",
            "provider":"telegram",
            "provider_message_id_digest":"a".repeat(64),
            "claimed_status":"bounced"
        }
    })
    .to_string();
    let rejected = setup(VerifierType::Delivery, "delivery_store", raw);
    let before = state_revision(&rejected);
    assert!(matches!(
        verify(&rejected, &command(&rejected, "claimed-bounce", 1, vec![])),
        CriterionVerificationOutcome::RejectedPredicate { .. }
    ));
    assert_eq!(state_revision(&rejected), before);
    assert_eq!(
        table_count(&rejected.fixture.paths, "execass_verifier_results"),
        0
    );

    let valid = setup(
        VerifierType::Delivery,
        "delivery_store",
        predicate(CriterionPredicate::Delivery {
            version: PredicateVersion::V1,
            predicate: DeliveryPredicate::RemoteProvider {
                provider: RemoteDeliveryProvider::Telegram,
                provider_message_id_digest: "a".repeat(64),
            },
        }),
    );
    let result = recorded_result(verify(
        &valid,
        &command(&valid, "remote-unknown", 1, vec![]),
    ));
    assert_eq!(result.result, CriterionVerificationResult::Unknown);
}

#[test]
fn malformed_predicate_details_never_escape_the_safe_rejection_code() {
    let hostile_predicates = [
        serde_json::json!({
            "kind": "database_predicate",
            "version": "v1",
            "delegation_id": "delegation-1",
            "canonical_plan_revision_greater_than": 0,
            "CANARY_UNKNOWN_FIELD_ea305_secret": true,
        })
        .to_string(),
        serde_json::json!({
            "kind": "CANARY_UNKNOWN_VARIANT_ea305_secret",
            "version": "v1",
        })
        .to_string(),
    ];

    for (index, raw) in hostile_predicates.into_iter().enumerate() {
        let f = setup(VerifierType::DatabasePredicate, "execass_plan_store", raw);
        let before = state_revision(&f);
        let outcome = verify(
            &f,
            &command(&f, &format!("hostile-predicate-{index}"), 1, vec![]),
        );
        assert_eq!(
            outcome,
            CriterionVerificationOutcome::RejectedPredicate {
                reason: INVALID_CLOSED_PREDICATE_REASON.into(),
            }
        );
        let diagnostic = format!("{outcome:?}");
        assert!(!diagnostic.contains("CANARY"));
        assert!(!diagnostic.contains("ea305_secret"));
        assert_eq!(state_revision(&f), before);
        assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 0);
        assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 0);
    }
}

fn seed_mail(f: &VerifierFixture, acked: bool) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO agent_mail_threads(thread_id,kind,subject,created_by_principal,created_at,updated_at,archived_at)
        VALUES('mail-thread','direct','EA-305','owner',1,1,NULL);
        INSERT INTO agent_mail_messages(message_id,thread_id,sender_principal,sender_kind,body_text,metadata_json,created_at)
        VALUES('mail-message','mail-thread','owner','human','safe',NULL,1);
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_mail_message_recipients(message_id,recipient_principal,delivered_at,acked_at) VALUES('mail-message','recipient',2,?1)",
        [acked.then_some(3_i64)],
    )
    .unwrap();
    drop(conn);
    observe(
        f,
        "mail",
        AuthorityLinkTarget::MailMessage {
            mail_message_id: "mail-message".into(),
        },
        Some((AuthorityOwnerKind::MailThread, "mail-thread")),
    );
}

#[test]
fn local_delivery_and_ack_are_exact_authoritative_predicates() {
    let f = setup(
        VerifierType::Delivery,
        "delivery_store",
        predicate(CriterionPredicate::Delivery {
            version: PredicateVersion::V1,
            predicate: DeliveryPredicate::AgentMailLocal {
                authority_link_id: "link-mail".into(),
                recipient_principal: "recipient".into(),
                require_ack: true,
            },
        }),
    );
    seed_mail(&f, false);
    let evidence = receipt_evidence("link-mail", AuthorityLinkKind::MailMessage, "mail-message");
    let first = recorded_result(verify(
        &f,
        &command(&f, "mail-unacked", 1, evidence.clone()),
    ));
    assert_eq!(first.result, CriterionVerificationResult::Fail);
    open_sqlite_connection(&f.fixture.paths.db_path)
        .unwrap()
        .execute(
            "UPDATE agent_mail_message_recipients SET acked_at=3 WHERE message_id='mail-message' AND recipient_principal='recipient'",
            [],
        )
        .unwrap();
    let second = recorded_result(verify(&f, &command(&f, "mail-acked", 2, evidence)));
    assert_eq!(second.result, CriterionVerificationResult::Pass);
}

#[test]
fn replay_conflict_and_stale_paths_do_not_mutate() {
    let f = setup(
        VerifierType::DatabasePredicate,
        "execass_plan_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let first_command = command(&f, "replay", 1, vec![]);
    assert_eq!(
        recorded_result(verify(&f, &first_command)).result,
        CriterionVerificationResult::Pass
    );
    let after_record = state_revision(&f);
    assert!(matches!(
        verify(&f, &first_command),
        CriterionVerificationOutcome::Replayed { .. }
    ));
    assert_eq!(state_revision(&f), after_record);
    assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 1);

    let mut conflict = command(&f, "replay", 1, vec![]);
    conflict.outbox_event_id = "changed-event".into();
    conflict.receipt.causation_event_id = "changed-event".into();
    assert!(matches!(
        verify(&f, &conflict),
        CriterionVerificationOutcome::Conflict { .. }
    ));
    assert_eq!(state_revision(&f), after_record);

    let mut stale = command(&f, "stale", 2, vec![]);
    stale.expected_state_revision -= 1;
    stale.receipt.expected_state_revision -= 1;
    assert!(matches!(
        verify(&f, &stale),
        CriterionVerificationOutcome::Stale { .. }
    ));
    assert_eq!(state_revision(&f), after_record);
    assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 1);
}

#[test]
fn exact_result_replay_survives_later_state_progress_and_criteria_supersession() {
    let f = setup(
        VerifierType::DatabasePredicate,
        "execass_plan_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let original = command(&f, "historical-replay", 1, vec![]);
    let recorded = recorded_result(verify(&f, &original));
    let current = f
        .fixture
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap();
    let next_state_revision = current.delegation.state_revision + 1;
    let mut replacement_plan = current.plan.clone();
    replacement_plan.plan_id = "plan-after-verification".into();
    replacement_plan.plan_revision += 1;
    replacement_plan.based_on_delegation_revision = next_state_revision;
    replacement_plan.manifest_digest = "replacement-manifest-digest".into();
    replacement_plan.created_at += 1;
    let mut replacement_criterion = current.outcome_criteria[0].clone();
    replacement_criterion.criterion_id = "criterion-2".into();
    replacement_criterion.criterion_key = "criterion-2".into();
    replacement_criterion.criteria_revision += 1;
    replacement_criterion.created_at += 1;
    f.fixture
        .store
        .amend_lifecycle(&AmendLifecycleCommand {
            write: WriteContext {
                idempotency_key: "amend-after-verification".into(),
                correlation_id: "amend-after-verification-correlation".into(),
                causation_id: "amend-after-verification-causation".into(),
                occurred_at: 1_800_000_002_000,
            },
            delegation_id: "delegation-1".into(),
            expected_state_revision: current.delegation.state_revision,
            transition_id: "transition-after-verification".into(),
            amendment_id: "amendment-after-verification".into(),
            amendment_revision: 1,
            normalized_amendment: "replace the verified criterion".into(),
            intake_evidence_json: "{}".into(),
            authority_provenance_id: current.authority.authority_provenance_id,
            plan: replacement_plan,
            outcome_criteria: vec![replacement_criterion],
            outbox_event: NewOutboxEvent {
                event_id: "event-after-verification".into(),
                event_name: OutboxEventName::DelegationTransitioned,
                aggregate_id: "delegation-1".into(),
                aggregate_revision: next_state_revision,
                correlation_id: "amend-after-verification-correlation".into(),
                causation_id: "amend-after-verification-causation".into(),
                occurred_at: 1_800_000_002_000,
                safe_payload_json: "{}".into(),
                duplicate_identity: "amend-after-verification".into(),
            },
        })
        .unwrap();
    let after_amendment = state_revision(&f);

    let replayed = verify(&f, &original);
    assert!(matches!(
        replayed,
        CriterionVerificationOutcome::Replayed { ref result, .. } if result == &recorded
    ));
    assert_eq!(state_revision(&f), after_amendment);
    assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 1);
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 1);
}

#[test]
fn human_supersession_without_a_bound_schema_record_stays_unknown() {
    let f = setup(
        VerifierType::HumanBoundSupersession,
        "human_bound_supersession_store",
        predicate(CriterionPredicate::HumanBoundSupersession {
            version: PredicateVersion::V1,
            decision_id: "decision-1".into(),
            decision_revision: 1,
            superseded_criterion_id: "criterion-old".into(),
        }),
    );
    let result = recorded_result(verify(&f, &command(&f, "human-unknown", 1, vec![])));
    assert_eq!(result.result, CriterionVerificationResult::Unknown);
}

#[test]
fn process_success_proves_only_the_exact_progress_predicate() {
    let f = setup(
        VerifierType::ProcessExit,
        "execution_store",
        predicate(CriterionPredicate::ProcessExit {
            version: PredicateVersion::V1,
            predicate: ProcessExitPredicate::Run {
                authority_link_id: "link-run".into(),
                expected_status: RunStatusPredicate::Succeeded,
            },
        }),
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO agents(agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at)
        VALUES('agent-run','agent','Z:\carsinos','test','test','default',1,1);
        INSERT INTO sessions(session_id,session_key,agent_id,title,created_at,updated_at,closed_at)
        VALUES('session-run','session-key-run','agent-run','run',1,1,NULL);
        INSERT INTO runs(run_id,session_id,status,model_provider,model_id,started_at,ended_at,error_text,usage_json,created_at)
        VALUES('run-1','session-run','succeeded','test','test',1,2,NULL,NULL,1);
        "#,
    )
    .unwrap();
    drop(conn);
    observe(
        &f,
        "run",
        AuthorityLinkTarget::Run {
            run_id: "run-1".into(),
        },
        Some((AuthorityOwnerKind::Session, "session-run")),
    );
    let result = recorded_result(verify(
        &f,
        &command(
            &f,
            "run-progress",
            1,
            receipt_evidence("link-run", AuthorityLinkKind::Run, "run-1"),
        ),
    ));
    assert_eq!(result.result, CriterionVerificationResult::Pass);
    assert!(result
        .evidence_refs_json
        .contains("execution_status_progress_only"));
}

fn seed_task(f: &VerifierFixture, status: &str) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO agents(agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at)
        VALUES('agent-task','agent','Z:\carsinos','test','test','default',1,1);
        INSERT INTO goals(goal_id,slug,title,summary,status,owner_agent_id,target_date,created_at,updated_at)
        VALUES('goal-task','goal-task','goal','','active','agent-task',NULL,1,1);
        INSERT INTO projects(project_id,goal_id,slug,name,summary,status,owner_agent_id,workspace_root,budget_month_usd,created_at,updated_at)
        VALUES('project-task','goal-task','project-task','project','','active','agent-task','Z:\carsinos',NULL,1,1);
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tasks(task_id,project_id,parent_task_id,title,detail,status,priority,owner_agent_id,due_at,blocked_reason,linked_board_card_id,linked_job_id,created_at,updated_at) VALUES('task-1','project-task',NULL,'task','',?1,'normal','agent-task',NULL,NULL,NULL,NULL,1,1)",
        [status],
    )
    .unwrap();
    drop(conn);
    observe(
        f,
        "task",
        AuthorityLinkTarget::Task {
            task_id: "task-1".into(),
        },
        Some((AuthorityOwnerKind::Project, "project-task")),
    );
}

#[test]
fn task_state_is_exact_and_wrong_state_fails() {
    let f = setup(
        VerifierType::AuthoritativeState,
        "authoritative_state_store",
        predicate(CriterionPredicate::AuthoritativeState {
            version: PredicateVersion::V1,
            predicate: AuthoritativeStatePredicate::Task {
                authority_link_id: "link-task".into(),
                expected_status: TaskStatusPredicate::Done,
            },
        }),
    );
    seed_task(&f, "in_progress");
    let evidence = receipt_evidence("link-task", AuthorityLinkKind::Task, "task-1");
    let wrong = recorded_result(verify(&f, &command(&f, "task-wrong", 1, evidence.clone())));
    assert_eq!(wrong.result, CriterionVerificationResult::Fail);
    open_sqlite_connection(&f.fixture.paths.db_path)
        .unwrap()
        .execute(
            "UPDATE tasks SET status='done',updated_at=2 WHERE task_id='task-1'",
            [],
        )
        .unwrap();
    let exact = recorded_result(verify(&f, &command(&f, "task-done", 2, evidence)));
    assert_eq!(exact.result, CriterionVerificationResult::Pass);
}

fn seed_board_card(f: &VerifierFixture, column_id: &str) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO boards(board_id,board_key,name,board_type,created_at,updated_at,archived_at)
        VALUES('board-1','board-1','board','kanban',1,1,NULL);
        INSERT INTO board_columns(column_id,board_id,column_key,name,position,created_at,updated_at,archived_at)
        VALUES('column-todo','board-1','todo','Todo',0,1,1,NULL);
        INSERT INTO board_columns(column_id,board_id,column_key,name,position,created_at,updated_at,archived_at)
        VALUES('column-done','board-1','done','Done',1,1,1,NULL);
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO board_cards(card_id,board_id,column_id,title,description,owner_kind,owner_agent_id,owner_human_id,due_at,tags_json,script_markdown,linked_session_id,latest_run_id,position,created_at,updated_at,archived_at) VALUES('card-1','board-1',?1,'card',NULL,'human',NULL,'owner',NULL,NULL,NULL,NULL,NULL,0,1,1,NULL)",
        [column_id],
    )
    .unwrap();
    drop(conn);
    observe(
        f,
        "card",
        AuthorityLinkTarget::BoardCard {
            board_card_id: "card-1".into(),
        },
        Some((AuthorityOwnerKind::Board, "board-1")),
    );
}

#[test]
fn board_and_card_predicates_use_exact_ids_and_archive_state() {
    let card = setup(
        VerifierType::AuthoritativeState,
        "authoritative_state_store",
        predicate(CriterionPredicate::AuthoritativeState {
            version: PredicateVersion::V1,
            predicate: AuthoritativeStatePredicate::BoardCard {
                authority_link_id: "link-card".into(),
                expected_column_id: "column-done".into(),
                expected_card_archived: false,
                expected_board_archived: false,
            },
        }),
    );
    seed_board_card(&card, "column-todo");
    let wrong = recorded_result(verify(
        &card,
        &command(
            &card,
            "card-wrong",
            1,
            receipt_evidence("link-card", AuthorityLinkKind::BoardCard, "card-1"),
        ),
    ));
    assert_eq!(wrong.result, CriterionVerificationResult::Fail);

    let board = setup(
        VerifierType::AuthoritativeState,
        "authoritative_state_store",
        predicate(CriterionPredicate::AuthoritativeState {
            version: PredicateVersion::V1,
            predicate: AuthoritativeStatePredicate::Board {
                authority_link_id: "link-board".into(),
                expected_archived: false,
            },
        }),
    );
    let conn = open_sqlite_connection(&board.fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO boards(board_id,board_key,name,board_type,created_at,updated_at,archived_at) VALUES('board-exact','board-exact','board','kanban',1,1,NULL)",
        [],
    )
    .unwrap();
    drop(conn);
    observe(
        &board,
        "board",
        AuthorityLinkTarget::Board {
            board_id: "board-exact".into(),
        },
        None,
    );
    let exact = recorded_result(verify(
        &board,
        &command(
            &board,
            "board-exact",
            1,
            receipt_evidence("link-board", AuthorityLinkKind::Board, "board-exact"),
        ),
    ));
    assert_eq!(exact.result, CriterionVerificationResult::Pass);
}

#[test]
fn provider_attempt_predicates_fail_closed_across_uncertain_and_reconciled_states() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE execass_provider_attempts(
          attempt_id TEXT PRIMARY KEY,delegation_id TEXT NOT NULL,status TEXT NOT NULL,
          provider_response_digest TEXT,remote_effect_id TEXT,provider_error_class TEXT
        );
        INSERT INTO execass_provider_attempts VALUES(
          'attempt-1','delegation-1','outcome_unknown','sha256:unknown',NULL,NULL
        );
        "#,
    )
    .unwrap();
    assert_eq!(
        evaluate_provider_state_for_test(
            &conn,
            "delegation-1",
            "attempt-1",
            ProviderAttemptPredicate::Succeeded,
        )
        .unwrap(),
        CriterionVerificationResult::Unknown
    );
    conn.execute(
        "UPDATE execass_provider_attempts SET status='reconciled_absent' WHERE attempt_id='attempt-1'",
        [],
    )
    .unwrap();
    assert_eq!(
        evaluate_provider_state_for_test(
            &conn,
            "delegation-1",
            "attempt-1",
            ProviderAttemptPredicate::ReconciledAbsent,
        )
        .unwrap(),
        CriterionVerificationResult::Pass
    );
    conn.execute(
        "UPDATE execass_provider_attempts SET status='reconciled_present',remote_effect_id='remote-1' WHERE attempt_id='attempt-1'",
        [],
    )
    .unwrap();
    assert_eq!(
        evaluate_provider_state_for_test(
            &conn,
            "delegation-1",
            "attempt-1",
            ProviderAttemptPredicate::ReconciledAbsent,
        )
        .unwrap(),
        CriterionVerificationResult::Fail
    );
    assert_eq!(
        evaluate_provider_state_for_test(
            &conn,
            "delegation-1",
            "attempt-1",
            ProviderAttemptPredicate::ReconciledPresent,
        )
        .unwrap(),
        CriterionVerificationResult::Pass
    );
}

#[test]
fn database_predicate_is_closed_and_off_by_one_is_a_real_failure() {
    let f = setup(
        VerifierType::DatabasePredicate,
        "execass_plan_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 1,
        }),
    );
    let result = recorded_result(verify(&f, &command(&f, "database-threshold", 1, vec![])));
    assert_eq!(result.result, CriterionVerificationResult::Fail);
}

#[test]
fn missing_authority_is_unknown_and_revision_staleness_is_zero_mutation() {
    let missing = setup(
        VerifierType::AuthoritativeState,
        "authoritative_state_store",
        predicate(CriterionPredicate::AuthoritativeState {
            version: PredicateVersion::V1,
            predicate: AuthoritativeStatePredicate::Task {
                authority_link_id: "missing-link".into(),
                expected_status: TaskStatusPredicate::Done,
            },
        }),
    );
    let unknown = recorded_result(verify(
        &missing,
        &command(&missing, "missing-link", 1, vec![]),
    ));
    assert_eq!(unknown.result, CriterionVerificationResult::Unknown);

    let stale = setup(
        VerifierType::DatabasePredicate,
        "execass_plan_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let before = state_revision(&stale);
    assert!(matches!(
        verify(&stale, &command(&stale, "result-revision-stale", 2, vec![])),
        CriterionVerificationOutcome::StaleResultRevision {
            current_result_revision: 0
        }
    ));
    assert_eq!(state_revision(&stale), before);
    assert_eq!(
        table_count(&stale.fixture.paths, "execass_verifier_results"),
        0
    );
}

#[test]
fn maximum_state_revision_is_rejected_without_mutation() {
    let f = setup(
        VerifierType::DatabasePredicate,
        "execass_plan_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let before = state_revision(&f);
    let mut exhausted = command(&f, "max-state", 1, vec![]);
    exhausted.expected_state_revision = i64::MAX;

    assert_eq!(
        verify(&f, &exhausted),
        CriterionVerificationOutcome::RevisionExhausted {
            revision_kind: "delegation_state",
            current_revision: i64::MAX,
        }
    );
    assert_eq!(state_revision(&f), before);
    assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 0);
}

#[test]
fn exhausted_persisted_result_revision_is_typed_and_zero_mutation() {
    let f = setup(
        VerifierType::DatabasePredicate,
        "execass_plan_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        r#"INSERT INTO execass_verifier_results(
             verifier_result_id,delegation_id,criterion_id,result_revision,result,
             evidence_refs_json,evidence_digest,verifier_identity,verified_at
           ) VALUES('max-result','delegation-1','criterion-1',?1,'unknown','{}',
             'sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
             ?2,1)"#,
        params![i64::MAX, CRITERION_VERIFIER_IDENTITY],
    )
    .unwrap();
    drop(conn);
    let before = state_revision(&f);
    let mut exhausted = command(&f, "max-result", 1, vec![]);
    exhausted.expected_result_revision = i64::MAX;
    exhausted.verifier_result_id =
        deterministic_verifier_result_id("criterion-1", i64::MAX, "verify-max-result");
    exhausted.receipt.subject.subject_id = exhausted.verifier_result_id.clone();
    exhausted.receipt.subject.revision = i64::MAX;

    assert_eq!(
        verify(&f, &exhausted),
        CriterionVerificationOutcome::RevisionExhausted {
            revision_kind: "verifier_result",
            current_revision: i64::MAX,
        }
    );
    assert_eq!(state_revision(&f), before);
    assert_eq!(table_count(&f.fixture.paths, "execass_verifier_results"), 1);
    assert_eq!(table_count(&f.fixture.paths, "execass_receipts"), 0);
}

#[test]
fn predicate_kind_and_source_kind_mismatches_are_rejected_without_mutation() {
    let kind_mismatch = setup(
        VerifierType::Artifact,
        "artifact_store",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let before = state_revision(&kind_mismatch);
    assert!(matches!(
        verify(
            &kind_mismatch,
            &command(&kind_mismatch, "kind-mismatch", 1, vec![])
        ),
        CriterionVerificationOutcome::RejectedPredicate { .. }
    ));
    assert_eq!(state_revision(&kind_mismatch), before);

    let source_mismatch = setup(
        VerifierType::DatabasePredicate,
        "caller_selected_source",
        predicate(CriterionPredicate::DatabasePredicate {
            version: PredicateVersion::V1,
            delegation_id: "delegation-1".into(),
            canonical_plan_revision_greater_than: 0,
        }),
    );
    let before = state_revision(&source_mismatch);
    assert!(matches!(
        verify(
            &source_mismatch,
            &command(&source_mismatch, "source-mismatch", 1, vec![])
        ),
        CriterionVerificationOutcome::RejectedPredicate { .. }
    ));
    assert_eq!(state_revision(&source_mismatch), before);
}
