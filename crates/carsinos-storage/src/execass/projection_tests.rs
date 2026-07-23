use super::receipt_integrity::{AnchorCommitInput, IntegrityFailpoints, ReceiptKeyProtector};
use super::tests::{fixture, foundation, table_count, Fixture};
use super::*;
use crate::open_sqlite_connection;
use anyhow::{bail, Result};
use rusqlite::{config::DbConfig, params};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

const NOW: i64 = 1_800_000_100_000;

#[derive(Default)]
struct TestProtector {
    keys: Mutex<BTreeMap<(String, i64), Vec<u8>>>,
}

impl ReceiptKeyProtector for TestProtector {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let mut keys = self.keys.lock().expect("key map lock");
        let identity = (key.key_id.clone(), key.key_generation);
        if keys.contains_key(&identity) {
            bail!("test key already exists")
        }
        let mut material = vec![0_u8; 32];
        material[..8].copy_from_slice(&key.key_generation.to_be_bytes());
        for (offset, byte) in key.key_id.bytes().enumerate() {
            material[8 + offset % 24] ^= byte;
        }
        keys.insert(identity, material.clone());
        Ok(Zeroizing::new(material))
    }

    fn load(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        self.keys
            .lock()
            .expect("key map lock")
            .get(&(key.key_id.clone(), key.key_generation))
            .cloned()
            .map(Zeroizing::new)
            .ok_or_else(|| anyhow::anyhow!("test key unavailable"))
    }

    fn delete(&self, key: &ReceiptKeyRef) -> Result<()> {
        self.keys
            .lock()
            .expect("key map lock")
            .remove(&(key.key_id.clone(), key.key_generation));
        Ok(())
    }
}

#[derive(Default)]
struct NoFailpoints;

impl IntegrityFailpoints for NoFailpoints {
    fn hit(&self, _name: &'static str) -> Result<()> {
        Ok(())
    }
}

struct ProjectionFixture {
    fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    protector: Arc<TestProtector>,
    redactor: ReceiptRedactor,
}

fn projection_fixture(with_foundation: bool) -> ProjectionFixture {
    let fixture = fixture();
    if with_foundation {
        fixture.store.create_foundation(&foundation()).unwrap();
    }
    let protector = Arc::new(TestProtector::default());
    let anchor_dir = fixture
        .paths
        .root
        .parent()
        .expect("fixture root parent")
        .join("projection-integrity");
    let canonical_root = fixture.paths.root.canonicalize().unwrap();
    let root_identity =
        carsinos_protocol::execass_recorder::canonical_root_identity_from_canonical_path(
            &canonical_root.to_string_lossy(),
        );
    let integrity = ReceiptIntegrityStore::with_protector_and_failpoints(
        &fixture.paths,
        anchor_dir,
        root_identity,
        protector.clone(),
        Arc::new(NoFailpoints),
    )
    .unwrap();
    ProjectionFixture {
        fixture,
        integrity,
        protector,
        redactor: ReceiptRedactor::new(&["projection-secret"]).unwrap(),
    }
}

fn query() -> ExecAssProjectionQuery {
    ExecAssProjectionQuery::new(NOW)
}

fn project(f: &ProjectionFixture) -> Result<ExecAssExecutiveProjection> {
    f.fixture
        .store
        .read_authoritative_projection(&f.integrity, &f.redactor, &query())
}

fn provision_empty_anchor(f: &ProjectionFixture, finalize: bool) -> ReceiptKeyRef {
    let key = f.integrity.provision_initial_key("projection-key").unwrap();
    f.integrity
        .prepare_anchor(&AnchorCommitInput {
            state_root_generation: 1,
            anchor_generation: 1,
            receipt_count: 0,
            receipt_head_digest: None,
            key: key.clone(),
            transaction_id: "projection-empty-anchor".into(),
            external_receipt_digest:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            occurred_at: NOW - 100,
        })
        .unwrap();
    if finalize {
        let mut connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        let transaction = connection.transaction().unwrap();
        f.integrity
            .confirm_prepared_anchor_in_transaction(
                &transaction,
                "projection-empty-anchor",
                0,
                None,
                NOW - 99,
            )
            .unwrap();
        transaction.commit().unwrap();
        f.integrity
            .finalize_anchor("projection-empty-anchor")
            .unwrap();
    }
    key
}

fn seed_runtime_receipt_authority(f: &ProjectionFixture) {
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            r#"
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'projection-installation',
              'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
              'projection-host',1800000000001);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('projection-lease','execass',1,'projection-host',1,1800000000001,9999999999999);
            INSERT INTO execass_authority_provenance(
              authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
              channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
              policy_revision,evidence_digest,created_at
            ) VALUES('projection-runtime-authority','runtime','projection-assessor','completion-assessor',
              'local-runtime-fence','projection-runtime-bootstrap','runtime_safety_state','{}',1,
              'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
              1800000000001);
            "#,
        )
        .unwrap();
}

fn receipt_heads(f: &ProjectionFixture) -> (i64, Option<String>, i64, Option<String>) {
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let global = connection
        .query_row(
            "SELECT receipt_count,receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let delegation = connection
        .query_row(
            "SELECT receipt_chain_count,receipt_chain_head_digest FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    (global.0, global.1, delegation.0, delegation.1)
}

fn completion_receipt(
    f: &ProjectionFixture,
    key: &ReceiptKeyRef,
    assessment_id: &str,
    event_id: &str,
    state_revision: i64,
    suffix: &str,
) -> AppendReceiptCommand {
    let (global_count, global_head, delegation_count, delegation_head) = receipt_heads(f);
    AppendReceiptCommand {
        receipt_id: format!("projection-receipt-{suffix}"),
        transaction_id: format!("projection-transaction-{suffix}"),
        state_root_generation: 1,
        delegation_id: "delegation-1".into(),
        expected_state_revision: state_revision,
        expected_global_count: global_count,
        expected_global_head_digest: global_head,
        expected_delegation_count: delegation_count,
        expected_delegation_head_digest: delegation_head,
        receipt_kind: ReceiptKind::Completion,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::CompletionAssessment,
            subject_id: assessment_id.into(),
            revision: 1,
        },
        causation_id: format!("projection-completion-cause-{suffix}"),
        causation_event_id: event_id.into(),
        actor: ReceiptActorBinding {
            actor_type: ActorType::Runtime,
            actor_identity: SafeText::new("projection-assessor", &[]).unwrap(),
            authority_provenance_id: "projection-runtime-authority".into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "projection-host".into(),
            fencing_token: 1,
        },
        key: key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::new("terminal truth recorded", &[]).unwrap(),
        occurred_at: NOW - 10,
        committed_at: NOW - 10,
    }
}

fn verify_criterion_result(
    f: &ProjectionFixture,
    key: &ReceiptKeyRef,
    criterion_id: &str,
    result: &str,
    result_revision: i64,
    index: i64,
) {
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let current_plan: i64 = connection
        .query_row(
            "SELECT current_plan_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let threshold = if result == "pass" {
        current_plan - 1
    } else {
        current_plan
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
    let state: i64 = connection
        .query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    let idempotency_key = format!("projection-verify-{criterion_id}-{index}");
    let result_id =
        deterministic_verifier_result_id(criterion_id, result_revision, &idempotency_key);
    let event_id = format!("projection-verifier-event-{index}");
    let (global_count, global_head, delegation_count, delegation_head) = receipt_heads(f);
    let occurred_at = NOW - 30 + index;
    let receipt = AppendReceiptCommand {
        receipt_id: format!("projection-verifier-receipt-{index}"),
        transaction_id: format!("projection-verifier-transaction-{index}"),
        state_root_generation: 1,
        delegation_id: "delegation-1".into(),
        expected_state_revision: state + 1,
        expected_global_count: global_count,
        expected_global_head_digest: global_head,
        expected_delegation_count: delegation_count,
        expected_delegation_head_digest: delegation_head,
        receipt_kind: ReceiptKind::Verifier,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::VerifierResult,
            subject_id: result_id.clone(),
            revision: result_revision,
        },
        causation_id: format!("projection-verifier-cause-{index}"),
        causation_event_id: event_id.clone(),
        actor: ReceiptActorBinding {
            actor_type: ActorType::Runtime,
            actor_identity: SafeText::new("projection-assessor", &[]).unwrap(),
            authority_provenance_id: "projection-runtime-authority".into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "projection-host".into(),
            fencing_token: 1,
        },
        key: key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::new("criterion independently verified", &[]).unwrap(),
        occurred_at,
        committed_at: occurred_at,
    };
    let command = VerifyCriterionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("projection-verifier-correlation-{index}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at,
        },
        delegation_id: "delegation-1".into(),
        criterion_id: criterion_id.into(),
        expected_criteria_revision: 1,
        expected_state_revision: state,
        expected_result_revision: result_revision,
        verifier_result_id: result_id,
        outbox_event_id: event_id,
        receipt,
    };
    let outcome = f
        .fixture
        .store
        .verify_criterion(&f.integrity, &f.redactor, &command)
        .unwrap();
    let CriterionVerificationOutcome::Recorded {
        result: recorded, ..
    } = outcome
    else {
        panic!("verifier did not record: {outcome:?}")
    };
    assert_eq!(recorded.result.as_str(), result);
}

fn terminal_projection(
    results: [&str; 2],
    suffix: &str,
) -> (ProjectionFixture, ExecAssExecutiveProjection) {
    let f = projection_fixture(true);
    seed_runtime_receipt_authority(&f);
    let key = provision_empty_anchor(&f, true);
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_action_branches SET status='terminal',updated_at=?1,terminal_at=?1 WHERE delegation_id='delegation-1'",
            [NOW - 20],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE execass_continuations SET status='terminal',updated_at=?1,completed_at=?1 WHERE delegation_id='delegation-1'",
            [NOW - 20],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
    drop(connection);

    for (index, criterion_id) in ["criterion-z", "criterion-a"].iter().enumerate() {
        verify_criterion_result(
            &f,
            &key,
            criterion_id,
            results[index],
            1,
            i64::try_from(index).unwrap(),
        );
    }

    let idempotency_key = format!("projection-assess-{suffix}");
    let assessment_id = deterministic_completion_assessment_id("delegation-1", 1, &idempotency_key);
    let event_id = deterministic_completion_event_id(&assessment_id);
    let state: i64 = open_sqlite_connection(&f.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let receipt = completion_receipt(&f, &key, &assessment_id, &event_id, state + 1, suffix);
    let command = AssessCompletionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("projection-correlation-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at: NOW - 10,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: state,
        expected_criteria_revision: 1,
        expected_assessment_revision: 1,
        receipt,
    };
    let outcome = f
        .fixture
        .store
        .assess_completion_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    assert!(
        matches!(outcome, CompletionAssessmentOutcome::Terminalized { .. }),
        "unexpected completion outcome: {outcome:?}"
    );
    let projection = project(&f).unwrap();
    (f, projection)
}

fn record_correction(f: &ProjectionFixture, key: &ReceiptKeyRef, suffix: &str) -> String {
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let (assessment_id, state): (String, i64) = connection
        .query_row(
            "SELECT a.assessment_id,d.state_revision FROM execass_completion_assessments a JOIN execass_delegations d ON d.delegation_id=a.delegation_id WHERE a.delegation_id='delegation-1' ORDER BY a.assessment_revision DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let idempotency_key = format!("projection-correction-{suffix}");
    let correction_id = deterministic_terminal_correction_id(&assessment_id, 1, &idempotency_key);
    let event_id = deterministic_terminal_correction_event_id(&correction_id);
    let (global_count, global_head, delegation_count, delegation_head) = receipt_heads(f);
    let receipt = AppendReceiptCommand {
        receipt_id: format!("projection-correction-receipt-{suffix}"),
        transaction_id: format!("projection-correction-transaction-{suffix}"),
        state_root_generation: 1,
        delegation_id: "delegation-1".into(),
        expected_state_revision: state,
        expected_global_count: global_count,
        expected_global_head_digest: global_head,
        expected_delegation_count: delegation_count,
        expected_delegation_head_digest: delegation_head,
        receipt_kind: ReceiptKind::TerminalCorrection,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::TerminalCorrection,
            subject_id: correction_id.clone(),
            revision: 1,
        },
        causation_id: format!("projection-correction-cause-{suffix}"),
        causation_event_id: event_id,
        actor: ReceiptActorBinding {
            actor_type: ActorType::Runtime,
            actor_identity: SafeText::new("projection-assessor", &[]).unwrap(),
            authority_provenance_id: "projection-runtime-authority".into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "projection-host".into(),
            fencing_token: 1,
        },
        key: key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::new("late contrary evidence recorded", &[]).unwrap(),
        occurred_at: NOW + 20,
        committed_at: NOW + 20,
    };
    let command = RecordLateTerminalCorrectionCommand {
        write: WriteContext {
            idempotency_key,
            correlation_id: format!("projection-correction-correlation-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at: NOW + 20,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: state,
        expected_correction_revision: 1,
        receipt,
    };
    assert!(matches!(
        f.fixture
            .store
            .record_late_terminal_correction_atomically(&f.integrity, &f.redactor, &command)
            .unwrap(),
        LateTerminalCorrectionOutcome::Recorded { .. }
    ));
    correction_id
}

fn set_pending_decision(
    f: &ProjectionFixture,
    decision_kind: &str,
    attention_kind: &str,
    with_challenge: bool,
) {
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_decisions(
                 decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,
                 policy_revision,decision_kind,status,exact_presented_action_json,
                 confirmed_logical_action_identity,manifest_digest,payload_digest,
                 payload_and_material_operands_json,target_audience_path_json,
                 side_effect_envelope_json,recommendation,consequence,alternatives_json,
                 idempotency_key,requested_at)
               VALUES('decision-1','delegation-1',1,1,1,1,?1,'pending','{}','action-1',
                 'manifest','payload','{}','{}','{}','continue safely','owner choice','[]',
                 'decision-idem',?2)"#,
            params![decision_kind, NOW - 50],
        )
        .unwrap();
    if with_challenge {
        connection
            .execute(
                r#"INSERT INTO execass_confirmation_challenges(
                     challenge_id,decision_id,delegation_id,decision_revision,
                     exact_presented_action_json,confirmed_logical_action_identity,
                     manifest_digest,payload_digest,payload_and_material_operands_json,
                     canonical_action_envelope_or_selector_json,declared_consequence,
                     nonce_digest,status,created_at,expires_at)
                   VALUES('challenge-1','decision-1','delegation-1',1,'{}','action-1',
                     'manifest','payload','{}','{}','owner choice',
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'pending',?1,?2)"#,
                params![NOW - 50, NOW + 500],
            )
            .unwrap();
    }
    connection
        .execute(
            r#"INSERT INTO execass_attention_items(
                 attention_id,delegation_id,kind,status,reason,recommendation,
                 alternatives_json,required_assurance,decision_id,delegation_revision,created_at)
               VALUES('attention-1','delegation-1',?1,'actionable','owner choice',
                 'continue safely','[]','authenticated_owner','decision-1',1,?2)"#,
            params![attention_kind, NOW - 40],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_delegations SET pending_decision_id='decision-1' WHERE delegation_id='delegation-1'",
            [],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
}

#[test]
fn all_decision_kinds_map_totally_and_mixed_work_remains_in_motion() {
    let cases = [
        (
            "clarification",
            "clarification",
            NeedsYouKind::Clarification,
            false,
        ),
        (
            "dangerous_action_confirmation",
            "confirmation",
            NeedsYouKind::Confirmation,
            true,
        ),
        (
            "owner_configured_checkpoint",
            "confirmation",
            NeedsYouKind::Confirmation,
            false,
        ),
        (
            "recovery_choice",
            "recovery_choice",
            NeedsYouKind::RecoveryChoice,
            false,
        ),
        (
            "duplicate_risk_retry",
            "confirmation",
            NeedsYouKind::Confirmation,
            false,
        ),
        ("stop", "confirmation", NeedsYouKind::Confirmation, false),
        (
            "policy_change",
            "confirmation",
            NeedsYouKind::Confirmation,
            false,
        ),
    ];
    for (decision_kind, attention_kind, expected, challenge) in cases {
        let f = projection_fixture(true);
        set_pending_decision(&f, decision_kind, attention_kind, challenge);
        let projection = project(&f).unwrap();
        assert_eq!(projection.needs_you.len(), 1, "{decision_kind}");
        assert_eq!(projection.needs_you[0].kind, expected, "{decision_kind}");
        assert_eq!(projection.in_motion.len(), 1, "{decision_kind}");
        assert_eq!(projection.in_motion[0].state, InMotionState::Active);
    }
}

#[test]
fn reply_has_no_decision_but_can_coexist_with_a_separate_pending_decision() {
    let f = projection_fixture(true);
    set_pending_decision(&f, "clarification", "clarification", false);
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_attention_items(
                 attention_id,delegation_id,kind,status,reason,recommendation,
                 alternatives_json,required_assurance,decision_id,delegation_revision,created_at)
               VALUES('reply-1','delegation-1','reply','actionable','awaiting reply',
                 'reply when ready','[]','authenticated_owner',NULL,1,?1)"#,
            [NOW - 30],
        )
        .unwrap();
    let projection = project(&f).unwrap();
    let reply = projection
        .needs_you
        .iter()
        .find(|item| item.attention_id == "reply-1")
        .unwrap();
    assert_eq!(reply.kind, NeedsYouKind::Reply);
    assert!(reply.decision_id.is_none());
}

#[test]
fn malformed_attention_decision_bindings_fail_closed() {
    let mismatch = projection_fixture(true);
    set_pending_decision(&mismatch, "clarification", "confirmation", false);
    assert!(project(&mismatch)
        .unwrap_err()
        .to_string()
        .contains("exact pending decision"));

    let wrong_owner = projection_fixture(true);
    set_pending_decision(&wrong_owner, "clarification", "clarification", false);
    let connection = open_sqlite_connection(&wrong_owner.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    connection
        .execute(
            "UPDATE execass_decisions SET delegation_id='other-delegation' WHERE decision_id='decision-1'",
            [],
        )
        .unwrap();
    assert!(project(&wrong_owner).is_err());
}

#[test]
fn stopped_and_draining_are_distinct_without_exposing_action_text() {
    for (stored, expected) in [
        ("stop_requested", InMotionState::Draining),
        ("stopped", InMotionState::Stopped),
    ] {
        let f = projection_fixture(true);
        let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        connection
            .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
            .unwrap();
        connection
            .execute(
                "UPDATE execass_delegations SET run_control=?1 WHERE delegation_id='delegation-1'",
                [stored],
            )
            .unwrap();
        let projection = project(&f).unwrap();
        assert_eq!(projection.in_motion[0].state, expected);
        let serialized = serde_json::to_string(&projection).unwrap();
        assert!(!serialized.contains("perform bounded work"));
        assert!(!serialized.contains("prepare the requested"));
    }
}

#[test]
fn read_and_rebuild_are_identical_and_do_not_write_delivery_or_outbox_state() {
    let f = projection_fixture(true);
    let before = (
        table_count(&f.fixture.paths, "execass_outbox_events"),
        table_count(&f.fixture.paths, "execass_summary_deliveries"),
        table_count(&f.fixture.paths, "execass_summary_delivery_items"),
        table_count(&f.fixture.paths, "execass_summary_acknowledgements"),
    );
    let read = project(&f).unwrap();
    let rebuilt = f
        .fixture
        .store
        .rebuild_authoritative_projection(&f.integrity, &f.redactor, &query())
        .unwrap();
    assert_eq!(read, rebuilt);
    assert_eq!(
        before,
        (
            table_count(&f.fixture.paths, "execass_outbox_events"),
            table_count(&f.fixture.paths, "execass_summary_deliveries"),
            table_count(&f.fixture.paths, "execass_summary_delivery_items"),
            table_count(&f.fixture.paths, "execass_summary_acknowledgements"),
        )
    );
}

#[test]
fn terminal_outcomes_are_closed_honest_and_receipt_backed() {
    let (_, completed) = terminal_projection(["pass", "pass"], "completed");
    let done = &completed.done_since_you_checked[0];
    assert_eq!(done.outcome, DoneOutcome::Completed);
    assert!(done.useful_outcome);
    assert!(done.what_did_not_happen.is_empty());
    assert_eq!(done.trust, ProjectionTrust::Trusted);
    assert_eq!(
        done.terminal_receipt_deep_link.kind,
        ProjectionDeepLinkKind::Receipt
    );

    let (_, partial) = terminal_projection(["pass", "fail"], "partial");
    let done = &partial.done_since_you_checked[0];
    assert_eq!(done.outcome, DoneOutcome::PartiallyCompleted);
    assert!(done.useful_outcome);
    assert_eq!(done.what_did_not_happen.len(), 1);
    assert_eq!(
        done.what_did_not_happen[0].result,
        UnmetCriterionResult::Fail
    );

    let (_, failed) = terminal_projection(["fail", "fail"], "failed");
    let done = &failed.done_since_you_checked[0];
    assert_eq!(done.outcome, DoneOutcome::Failed);
    assert!(!done.useful_outcome);
    assert_eq!(done.what_did_not_happen.len(), 2);
    assert!(failed
        .reef
        .iter()
        .any(|item| item.activity == ReefActivity::Failed));
}

#[test]
fn late_correction_warns_and_links_without_rewriting_terminal_truth() {
    let (f, before) = terminal_projection(["pass", "pass"], "corrected");
    let original = before.done_since_you_checked[0].clone();
    let key = f.integrity.current_append_key().unwrap().unwrap();
    verify_criterion_result(&f, &key, "criterion-z", "fail", 2, 20);
    let correction_id = record_correction(&f, &key, "contrary");
    let after = project(&f).unwrap();
    let corrected = &after.done_since_you_checked[0];
    assert_eq!(corrected.outcome, DoneOutcome::Completed);
    assert_eq!(corrected.assessment_id, original.assessment_id);
    assert_eq!(corrected.assessment_revision, original.assessment_revision);
    assert_eq!(
        corrected.correction_id.as_deref(),
        Some(correction_id.as_str())
    );
    assert!(corrected.correction_warning.is_some());
    assert!(corrected.correction_deep_link.is_some());
    assert_eq!(
        corrected.terminal_receipt_deep_link,
        original.terminal_receipt_deep_link
    );
}

#[test]
fn expired_or_terminal_attention_fails_closed_and_terminal_reef_wins() {
    let expired = projection_fixture(true);
    set_pending_decision(
        &expired,
        "dangerous_action_confirmation",
        "confirmation",
        true,
    );
    let connection = open_sqlite_connection(&expired.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_confirmation_challenges SET expires_at=?1 WHERE challenge_id='challenge-1'",
            [NOW - 1],
        )
        .unwrap();
    assert!(project(&expired).is_err());

    let terminal = projection_fixture(true);
    set_pending_decision(&terminal, "clarification", "clarification", false);
    let connection = open_sqlite_connection(&terminal.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_delegations SET phase='failed',terminal_at=?1 WHERE delegation_id='delegation-1'",
            [NOW],
        )
        .unwrap();
    assert!(project(&terminal)
        .unwrap_err()
        .to_string()
        .contains("terminal delegation retains actionable attention"));
    assert_eq!(
        super::projection::reef_activity("failed", "stopped").unwrap(),
        ReefActivity::Failed
    );
}

#[test]
fn item_set_digest_is_insertion_order_independent_and_excludes_ambient_reef() {
    let f = projection_fixture(true);
    let projection = project(&f).unwrap();
    let mut reversed = projection.clone();
    reversed.needs_you.reverse();
    reversed.in_motion.reverse();
    reversed.done_since_you_checked.reverse();
    reversed.next.reverse();
    reversed.receipts.items.reverse();
    reversed.reef.reverse();
    assert_eq!(
        super::projection::item_set_digest(&projection).unwrap(),
        super::projection::item_set_digest(&reversed).unwrap()
    );
    assert!(projection.boundary.item_set_digest.starts_with("sha256:"));
    let mut different_reef = projection.clone();
    different_reef.reef.clear();
    assert_eq!(
        super::projection::item_set_digest(&projection).unwrap(),
        super::projection::item_set_digest(&different_reef).unwrap()
    );
}

#[test]
fn integrity_modes_are_closed_and_never_leak_reasons() {
    let uninitialized = projection_fixture(false);
    assert_eq!(
        project(&uninitialized).unwrap().integrity,
        ProjectionIntegrity::Untrusted {
            failure: ProjectionIntegrityFailure::Uninitialized
        }
    );

    let prepared = projection_fixture(false);
    provision_empty_anchor(&prepared, false);
    assert_eq!(
        project(&prepared).unwrap().integrity,
        ProjectionIntegrity::Untrusted {
            failure: ProjectionIntegrityFailure::Prepared
        }
    );

    let trusted = projection_fixture(false);
    let key = provision_empty_anchor(&trusted, true);
    assert!(matches!(
        project(&trusted).unwrap().integrity,
        ProjectionIntegrity::Trusted {
            receipt_count: 0,
            ..
        }
    ));
    trusted.protector.delete(&key).unwrap();
    assert_eq!(
        project(&trusted).unwrap().integrity,
        ProjectionIntegrity::Untrusted {
            failure: ProjectionIntegrityFailure::KeyLost
        }
    );

    let quarantined = projection_fixture(false);
    provision_empty_anchor(&quarantined, true);
    let connection = open_sqlite_connection(&quarantined.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            "UPDATE execass_receipt_anchor_state SET status='quarantined',quarantined_at=?1,quarantine_reason='projection-secret-reason'",
            [NOW],
        )
        .unwrap();
    let projection = project(&quarantined).unwrap();
    assert_eq!(
        projection.integrity,
        ProjectionIntegrity::Untrusted {
            failure: ProjectionIntegrityFailure::Quarantined
        }
    );
    let serialized = serde_json::to_string(&projection).unwrap();
    assert!(!serialized.contains("projection-secret-reason"));
    assert!(projection.reef.iter().any(|item| {
        item.subject == ReefSubject::SystemIntegrity
            && item.activity == ReefActivity::IntegrityAttention
            && item.deep_link.is_none()
    }));
}

#[test]
fn empty_projection_is_deterministic_and_validates_query_bounds() {
    let f = projection_fixture(false);
    let first = project(&f).unwrap();
    let second = project(&f).unwrap();
    assert_eq!(first, second);
    assert!(first.needs_you.is_empty());
    assert!(first.in_motion.is_empty());
    assert!(first.done_since_you_checked.is_empty());
    assert!(first.next.is_empty());
    assert!(first.receipts.items.is_empty());
    assert!(first
        .reef
        .iter()
        .any(|item| item.subject == ReefSubject::SystemIntegrity));
    assert!(f
        .fixture
        .store
        .read_authoritative_projection(
            &f.integrity,
            &f.redactor,
            &ExecAssProjectionQuery {
                trusted_now_ms: NOW,
                receipt_limit: 101,
            },
        )
        .is_err());
}

#[test]
fn receipt_window_is_bounded_ordered_redacted_and_requires_contiguous_evidence() {
    let f = projection_fixture(true);
    let mut connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    let transaction = connection.transaction().unwrap();
    for sequence in 1_i64..=105 {
        let receipt_id = format!("projection-receipt-{sequence:03}");
        transaction
            .execute(
                r#"INSERT INTO execass_outbox_events(
                 event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
                 causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
               ) VALUES(?1,'execass.v1.summary.changed','delegation-1',?2,?3,?4,?5,'v1','{}',?6)"#,
                params![
                    format!("event-{sequence}"),
                    sequence,
                    format!("correlation-{sequence}"),
                    format!("cause-{sequence}"),
                    NOW + sequence,
                    format!("projection-outbox-{sequence}"),
                ],
            )
            .unwrap();
        transaction
            .execute(
                r#"INSERT INTO execass_receipts(
                 receipt_id,delegation_id,receipt_sequence,global_sequence,append_identity,
                 receipt_kind,causation_id,causation_event_id,subject_kind,subject_id,
                 subject_revision,actor_type,actor_identity,runtime_host_generation,state_revision,
                 canonical_payload,serialization_version,hash_algorithm,key_id,key_generation,
                 receipt_digest,keyed_integrity_tag,redacted_summary,occurred_at,committed_at
               ) VALUES(?1,'delegation-1',?2,?2,?3,'action',?4,?5,'outbox_event',?5,
                 1,'runtime','projection-runtime',1,1,X'00','test','sha256','test-key',1,
                 ?6,?7,?8,?9,?9)"#,
                params![
                    receipt_id,
                    sequence,
                    format!("append-{sequence}"),
                    format!("cause-{sequence}"),
                    format!("event-{sequence}"),
                    format!("digest-{sequence}"),
                    format!("tag-{sequence}"),
                    format!("safe summary {sequence}"),
                    NOW + sequence,
                ],
            )
            .unwrap();
    }
    for ordinal in 0_i64..=1 {
        transaction
            .execute(
                r#"INSERT INTO execass_authority_links(
                 link_id,delegation_id,link_revision,delegation_state_revision,correlation_id,
                 causation_id,outbox_event_id,authority_kind,security_audit_event_id,
                 authoritative_revision,linked_at
               ) VALUES(?1,'delegation-1',?2,1,?3,?4,?5,'security_audit_event',?6,0,?7)"#,
                params![
                    format!("authority-link-{ordinal}"),
                    ordinal + 1,
                    format!("authority-correlation-{ordinal}"),
                    format!("authority-cause-{ordinal}"),
                    format!("event-{}", 104 + ordinal),
                    format!("security-event-{ordinal}"),
                    NOW + ordinal,
                ],
            )
            .unwrap();
        transaction
            .execute(
                r#"INSERT INTO execass_receipt_evidence_refs(
                 receipt_id,ordinal,authority_kind,source_id,authoritative_revision,
                 authority_link_id,observation_digest,deep_link
               ) VALUES('projection-receipt-105',?1,?2,?3,0,?4,?5,?6)"#,
                params![
                    ordinal,
                    "security_audit_event",
                    format!("projection-secret-source-{ordinal}"),
                    format!("authority-link-{ordinal}"),
                    format!("observation-{ordinal}"),
                    format!("authority://{ordinal}"),
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();

    let projection = project(&f).unwrap();
    assert_eq!(projection.receipts.total, 105);
    assert_eq!(projection.receipts.items.len(), 100);
    assert!(projection.receipts.has_older);
    assert_eq!(projection.receipts.earliest_global_sequence, Some(1));
    assert_eq!(projection.receipts.latest_global_sequence, Some(105));
    assert_eq!(
        projection.receipts.items.first().unwrap().global_sequence,
        6
    );
    assert_eq!(
        projection.receipts.items.last().unwrap().global_sequence,
        105
    );
    let evidence = &projection.receipts.items.last().unwrap().evidence;
    assert_eq!(
        evidence.iter().map(|item| item.ordinal).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(evidence
        .iter()
        .all(|item| !item.source_id.contains("projection-secret")));

    connection.execute(
        "UPDATE execass_receipt_evidence_refs SET ordinal=2 WHERE receipt_id='projection-receipt-105' AND ordinal=1",
        [],
    ).unwrap();
    assert!(project(&f)
        .unwrap_err()
        .to_string()
        .contains("evidence ordinals are not contiguous"));
}

#[test]
fn next_uses_typed_due_sources_for_routines_and_confirmation_expiry() {
    let routine = projection_fixture(false);
    let digest = "a".repeat(64);
    let mut routine_foundation = foundation();
    routine_foundation.plan.manifest_digest = digest.clone();
    routine
        .fixture
        .store
        .create_foundation(&routine_foundation)
        .unwrap();
    let connection = open_sqlite_connection(&routine.fixture.paths.db_path).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_routines(
             routine_id,current_version,enabled,timezone,overlap_policy,catch_up_policy,
             replay_cap,created_at,updated_at
           ) VALUES('projection-routine',1,1,'America/Chicago','earlier','replay',10,?1,?1)"#,
            [NOW - 100],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_routine_versions(
             routine_id,routine_version,source_delegation_id,saved_owner_authority_provenance_id,
             normalized_original_intent,resolved_leaf_manifest_json,manifest_digest,
             saved_selector_json,saved_action_envelope_json,accepted_confirmation_grant_id,
             effective_policy_snapshot_json,effective_policy_revision,stable_leaf_digest,created_at
           ) SELECT 'projection-routine',1,d.delegation_id,d.authority_provenance_id,
             d.normalized_original_intent,p.resolved_leaf_manifest_json,p.manifest_digest,
             '{}','{}',NULL,'{}',1,?1,?2
           FROM execass_delegations d JOIN execass_plans p ON p.delegation_id=d.delegation_id
           WHERE d.delegation_id='delegation-1'"#,
            params![digest, NOW - 100],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_routine_occurrences(
             occurrence_id,routine_id,routine_version,scheduled_instant_ms,scheduled_local,
             utc_offset_seconds,time_resolution,effective_policy_revision,status,
             admission_plan_json,admitted_delegation_id,created_at,updated_at
           ) VALUES('projection-occurrence','projection-routine',1,?1,'2027-01-15T02:30:00',
             -21600,'single',1,'planned',NULL,NULL,?2,?2)"#,
            params![NOW + 50_000, NOW - 50],
        )
        .unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
    let projected = project(&routine).unwrap();
    let item = projected
        .next
        .iter()
        .find(|item| item.item_id == "projection-occurrence")
        .expect("routine occurrence is projected");
    assert_eq!(item.kind, NextKind::RoutineOccurrence);
    assert_eq!(item.due_at_ms, NOW + 50_000);
    assert_eq!(
        item.details,
        NextDetails::RoutineOccurrence {
            scheduled_local: "2027-01-15T02:30:00".into(),
            timezone: "America/Chicago".into(),
        }
    );

    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    let (continuation_id, action_id): (String, String) = connection.query_row(
        "SELECT continuation_id,action_id FROM execass_continuations WHERE delegation_id='delegation-1' LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    let claim_event: String = connection.query_row(
        "SELECT event_id FROM execass_outbox_events WHERE aggregate_id='delegation-1' ORDER BY global_sequence LIMIT 1",
        [],
        |row| row.get(0),
    ).unwrap();
    connection.execute(
        r#"INSERT INTO execass_logical_effects(
             logical_effect_id,delegation_id,continuation_id,action_kind,operation_reversible,
             declared_recovery_safe_boundary,state,internal_idempotency_key,provider_identity,
             provider_idempotency_key,manifest_digest,payload_digest,outcome_json,created_at,updated_at
           ) VALUES('projection-effect','delegation-1',?1,
             'public_or_externally_consequential_communication',0,'independent_absence',
             'outcome_unknown','projection-effect-idem','provider','provider-idem',
             'manifest','payload','{}',?2,?2)"#,
        params![continuation_id, NOW - 200],
    ).unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_provider_attempts(
             attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,claim_event_id,
             claim_receipt_id,attempt_number,fencing_token,host_generation,host_instance_id,
             runtime_fencing_token,status,provider_request_digest,provider_response_digest,
             started_at,finished_at
           ) VALUES('projection-attempt','delegation-1','projection-effect',?1,?2,?3,
             'projection-claim-receipt',1,1,1,'projection-host',1,'outcome_unknown',
             'request','response',?4,?4)"#,
            params![continuation_id, action_id, claim_event, NOW - 190],
        )
        .unwrap();
    connection
        .execute(
            r#"INSERT INTO execass_recovery_episodes(
             recovery_episode_id,delegation_id,logical_effect_id,initial_attempt_id,action_id,
             manifest_digest,normalized_intent_digest,effective_authority_digest,
             policy_json,policy_digest,opened_at
           ) VALUES('projection-episode','delegation-1','projection-effect','projection-attempt',?1,
             'manifest',?2,?3,'{}',?4,?5)"#,
            params![
                action_id,
                format!("sha256:{}", "b".repeat(64)),
                format!("sha256:{}", "c".repeat(64)),
                format!("sha256:{}", "d".repeat(64)),
                NOW - 180,
            ],
        )
        .unwrap();
    for revision in 1_i64..=2 {
        connection
            .execute(
                r#"INSERT INTO execass_outbox_events(
                 event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,
                 occurred_at,schema_version,safe_payload_json,duplicate_identity
               ) VALUES(?1,'execass.v1.recovery.updated','delegation-1',?2,?3,?4,?5,'v1','{}',?6)"#,
                params![
                    format!("projection-recovery-event-{revision}"),
                    revision + 20,
                    format!("projection-recovery-correlation-{revision}"),
                    format!("projection-recovery-cause-{revision}"),
                    NOW - 170 + revision,
                    format!("projection-recovery-outbox-{revision}"),
                ],
            )
            .unwrap();
        connection
            .execute(
                r#"INSERT INTO execass_recovery_evaluations(
                 recovery_evaluation_id,recovery_episode_id,delegation_id,logical_effect_id,
                 predecessor_attempt_id,evaluation_revision,recovery_state_revision,
                 objective_facts_json,objective_facts_digest,directive,directive_json,
                 directive_digest,not_before_ms,outbox_event_id,evaluated_at
               ) VALUES(?1,'projection-episode','delegation-1','projection-effect',
                 'projection-attempt',?2,?2,'{}',?3,'wait_backoff','{}',?4,?5,?6,?7)"#,
                params![
                    format!("projection-recovery-evaluation-{revision}"),
                    revision,
                    format!(
                        "sha256:{}",
                        if revision == 1 { "e" } else { "f" }.repeat(64)
                    ),
                    format!(
                        "sha256:{}",
                        if revision == 1 { "1" } else { "2" }.repeat(64)
                    ),
                    NOW + revision * 10_000,
                    format!("projection-recovery-event-{revision}"),
                    NOW - 170 + revision,
                ],
            )
            .unwrap();
    }
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
    let projected = project(&routine).unwrap();
    let recovery_items = projected
        .next
        .iter()
        .filter(|item| item.kind == NextKind::RecoveryReevaluation)
        .collect::<Vec<_>>();
    assert_eq!(recovery_items.len(), 1);
    assert_eq!(
        recovery_items[0].item_id,
        "projection-recovery-evaluation-2"
    );
    assert_eq!(recovery_items[0].due_at_ms, NOW + 20_000);
    assert_eq!(recovery_items[0].details, NextDetails::RecoveryReevaluation);

    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .unwrap();
    connection.execute(
        "UPDATE execass_logical_effects SET state='reconciled_present' WHERE logical_effect_id='projection-effect'",
        [],
    ).unwrap();
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true)
        .unwrap();
    assert!(project(&routine)
        .unwrap()
        .next
        .iter()
        .all(|item| item.kind != NextKind::RecoveryReevaluation));

    let confirmation = projection_fixture(true);
    set_pending_decision(
        &confirmation,
        "dangerous_action_confirmation",
        "confirmation",
        true,
    );
    let projected = project(&confirmation).unwrap();
    let item = projected
        .next
        .iter()
        .find(|item| item.item_id == "challenge-1")
        .expect("dangerous confirmation expiry is projected");
    assert_eq!(item.kind, NextKind::DangerousConfirmationExpiry);
    assert_eq!(item.details, NextDetails::DangerousConfirmationExpiry);
    assert!(item.due_at_ms > NOW);
}
