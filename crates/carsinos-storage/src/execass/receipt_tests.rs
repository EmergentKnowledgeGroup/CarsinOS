//! Hostile receipt-authority tests live alongside the private storage helpers.

use super::receipt::{AtomicReceiptMutation, AtomicReceiptWriteOutcome, ReceiptFailpoints};
use super::receipt_integrity::{IntegrityFailpoints, ReceiptKeyProtector};
use super::tests::{fixture, foundation};
use super::*;
use crate::open_sqlite_connection;
use anyhow::{bail, Result};
use base64::Engine;
use rusqlite::{config::DbConfig, params, Connection};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy)]
struct Ea112Case {
    name: &'static str,
    requirement: u8,
    class: &'static str,
}

const EA112_CASES: &[Ea112Case] = &[
    Ea112Case {
        name: "tail_truncation",
        requirement: 1,
        class: "tail_truncation",
    },
    Ea112Case {
        name: "prefix_truncation",
        requirement: 1,
        class: "prefix_truncation",
    },
    Ea112Case {
        name: "full_database_rollback",
        requirement: 2,
        class: "full_database_rollback",
    },
    Ea112Case {
        name: "full_database_deletion",
        requirement: 2,
        class: "full_database_deletion",
    },
    Ea112Case {
        name: "receipt_table_deletion",
        requirement: 2,
        class: "receipt_table_deletion",
    },
    Ea112Case {
        name: "historical_row_insertion",
        requirement: 3,
        class: "historical_row_insertion",
    },
    Ea112Case {
        name: "historical_row_modification",
        requirement: 3,
        class: "historical_row_modification",
    },
    Ea112Case {
        name: "historical_row_deletion",
        requirement: 3,
        class: "historical_row_deletion",
    },
    Ea112Case {
        name: "historical_row_reordering",
        requirement: 3,
        class: "historical_row_reordering",
    },
    Ea112Case {
        name: "global_chain_fork",
        requirement: 4,
        class: "global_chain_fork",
    },
    Ea112Case {
        name: "global_chain_gap",
        requirement: 4,
        class: "global_chain_gap",
    },
    Ea112Case {
        name: "global_count_mismatch",
        requirement: 4,
        class: "global_count_mismatch",
    },
    Ea112Case {
        name: "global_head_mismatch",
        requirement: 4,
        class: "global_head_mismatch",
    },
    Ea112Case {
        name: "delegation_chain_fork",
        requirement: 4,
        class: "delegation_chain_fork",
    },
    Ea112Case {
        name: "delegation_chain_gap",
        requirement: 4,
        class: "delegation_chain_gap",
    },
    Ea112Case {
        name: "delegation_count_mismatch",
        requirement: 4,
        class: "delegation_count_mismatch",
    },
    Ea112Case {
        name: "delegation_head_mismatch",
        requirement: 4,
        class: "delegation_head_mismatch",
    },
    Ea112Case {
        name: "multiple_delegations",
        requirement: 4,
        class: "multiple_delegations",
    },
    Ea112Case {
        name: "external_anchor_rollback",
        requirement: 5,
        class: "external_anchor_rollback",
    },
    Ea112Case {
        name: "prepared_document_tamper",
        requirement: 5,
        class: "prepared_document_tamper",
    },
    Ea112Case {
        name: "current_document_tamper",
        requirement: 5,
        class: "current_document_tamper",
    },
    Ea112Case {
        name: "key_generation_mismatch",
        requirement: 5,
        class: "key_generation_mismatch",
    },
    Ea112Case {
        name: "unknown_key",
        requirement: 5,
        class: "unknown_key",
    },
    Ea112Case {
        name: "missing_key",
        requirement: 5,
        class: "missing_key",
    },
    Ea112Case {
        name: "wrong_key",
        requirement: 5,
        class: "wrong_key",
    },
    Ea112Case {
        name: "broken_rotation_cross_signature",
        requirement: 5,
        class: "broken_rotation_cross_signature",
    },
    Ea112Case {
        name: "state_root_generation_mismatch",
        requirement: 5,
        class: "state_root_generation_mismatch",
    },
    Ea112Case {
        name: "cross_root_database_anchor_copy",
        requirement: 6,
        class: "cross_root_database_anchor_copy",
    },
    Ea112Case {
        name: "cross_key_restore",
        requirement: 6,
        class: "cross_key_restore",
    },
    Ea112Case {
        name: "archive_restore_identity_binding",
        requirement: 6,
        class: "archive_restore_identity_binding",
    },
    Ea112Case {
        name: "alternate_serialization_insert",
        requirement: 7,
        class: "alternate_serialization_insert",
    },
    Ea112Case {
        name: "alternate_serialization_rewrite",
        requirement: 7,
        class: "alternate_serialization_rewrite",
    },
    Ea112Case {
        name: "evidence_row_insertion",
        requirement: 7,
        class: "evidence_row_insertion",
    },
    Ea112Case {
        name: "evidence_row_update",
        requirement: 7,
        class: "evidence_row_update",
    },
    Ea112Case {
        name: "evidence_row_deletion",
        requirement: 7,
        class: "evidence_row_deletion",
    },
    Ea112Case {
        name: "evidence_row_reordering",
        requirement: 7,
        class: "evidence_row_reordering",
    },
    Ea112Case {
        name: "positive_key_rotation_reopen",
        requirement: 7,
        class: "positive_key_rotation_reopen",
    },
    Ea112Case {
        name: "rotated_receipt_tag_tamper",
        requirement: 7,
        class: "rotated_receipt_tag_tamper",
    },
    Ea112Case {
        name: "rotated_receipt_history_tamper",
        requirement: 7,
        class: "rotated_receipt_history_tamper",
    },
    Ea112Case {
        name: "rotated_key_registry_transition_tamper",
        requirement: 7,
        class: "rotated_key_registry_transition_tamper",
    },
    Ea112Case {
        name: "rotated_key_registry_status_tamper",
        requirement: 7,
        class: "rotated_key_registry_status_tamper",
    },
    Ea112Case {
        name: "rotated_key_registry_activation_tamper",
        requirement: 7,
        class: "rotated_key_registry_activation_tamper",
    },
    Ea112Case {
        name: "pending_key_rotation_registry_valid",
        requirement: 7,
        class: "pending_key_rotation_registry_valid",
    },
    Ea112Case {
        name: "orphan_registry_key_tamper",
        requirement: 7,
        class: "orphan_registry_key_tamper",
    },
    Ea112Case {
        name: "registry_created_at_tamper",
        requirement: 7,
        class: "registry_created_at_tamper",
    },
    Ea112Case {
        name: "registry_integrity_tag_tamper",
        requirement: 7,
        class: "registry_integrity_tag_tamper",
    },
    Ea112Case {
        name: "pending_key_id_tamper",
        requirement: 7,
        class: "pending_key_id_tamper",
    },
    Ea112Case {
        name: "pending_key_material_loss",
        requirement: 7,
        class: "pending_key_material_loss",
    },
    Ea112Case {
        name: "completion_after_pending_key_tamper",
        requirement: 10,
        class: "completion_after_pending_key_tamper",
    },
    Ea112Case {
        name: "lexical_child_parent_alias",
        requirement: 8,
        class: "lexical_child_parent_alias",
    },
    Ea112Case {
        name: "case_alias",
        requirement: 8,
        class: "case_alias",
    },
    Ea112Case {
        name: "junction_or_symlink_alias",
        requirement: 8,
        class: "junction_or_symlink_alias",
    },
    Ea112Case {
        name: "hard_link_alias",
        requirement: 8,
        class: "hard_link_alias",
    },
    Ea112Case {
        name: "benign_prepare_boundaries",
        requirement: 9,
        class: "benign_prepare_boundaries",
    },
    Ea112Case {
        name: "benign_finalize_boundaries",
        requirement: 9,
        class: "benign_finalize_boundaries",
    },
    Ea112Case {
        name: "benign_receipt_restart_boundaries",
        requirement: 9,
        class: "benign_receipt_restart_boundaries",
    },
    Ea112Case {
        name: "quarantine_blocks_trusted_operations",
        requirement: 10,
        class: "quarantine_blocks_trusted_operations",
    },
    Ea112Case {
        name: "safe_non_echoing_verifier",
        requirement: 10,
        class: "safe_non_echoing_verifier",
    },
    Ea112Case {
        name: "exact_reason_and_first_difference",
        requirement: 11,
        class: "exact_reason_and_first_difference",
    },
    Ea112Case {
        name: "inventory_has_no_omissions",
        requirement: 11,
        class: "inventory_has_no_omissions",
    },
];

pub(super) fn assert_ea112_case_registered(name: &str) {
    assert!(
        EA112_CASES.iter().any(|case| case.name == name),
        "unregistered EA-112 case: {name}"
    );
}

struct TestProtector {
    keys: Mutex<BTreeMap<(String, i64), Vec<u8>>>,
    seed: u8,
}

impl Default for TestProtector {
    fn default() -> Self {
        Self {
            keys: Mutex::new(BTreeMap::new()),
            seed: 0x5a,
        }
    }
}

impl TestProtector {
    fn with_seed(seed: u8) -> Self {
        Self {
            keys: Mutex::new(BTreeMap::new()),
            seed,
        }
    }
}

impl ReceiptKeyProtector for TestProtector {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let mut keys = self.keys.lock().expect("key lock");
        let identity = (key.key_id.clone(), key.key_generation);
        if keys.contains_key(&identity) {
            bail!("duplicate test key");
        }
        let material = test_key_material(key, self.seed);
        keys.insert(identity, material.clone());
        Ok(Zeroizing::new(material))
    }

    fn load(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        Ok(Zeroizing::new(
            self.keys
                .lock()
                .expect("key lock")
                .get(&(key.key_id.clone(), key.key_generation))
                .cloned()
                .unwrap_or_else(|| test_key_material(key, self.seed)),
        ))
    }

    fn delete(&self, key: &ReceiptKeyRef) -> Result<()> {
        self.keys
            .lock()
            .expect("key lock")
            .remove(&(key.key_id.clone(), key.key_generation));
        Ok(())
    }
}

fn test_key_material(key: &ReceiptKeyRef, seed: u8) -> Vec<u8> {
    let mut material = vec![seed; 32];
    material[..8].copy_from_slice(&key.key_generation.to_be_bytes());
    material
}

struct FailOnce {
    name: &'static str,
    fired: AtomicBool,
}

impl FailOnce {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            fired: AtomicBool::new(false),
        }
    }
}

impl ReceiptFailpoints for FailOnce {
    fn hit(&self, name: &'static str) -> Result<()> {
        if name == self.name && !self.fired.swap(true, Ordering::SeqCst) {
            bail!("injected receipt failpoint: {name}");
        }
        Ok(())
    }
}

impl IntegrityFailpoints for FailOnce {
    fn hit(&self, name: &'static str) -> Result<()> {
        if name == self.name && !self.fired.swap(true, Ordering::SeqCst) {
            bail!("injected receipt-integrity failpoint: {name}");
        }
        Ok(())
    }
}

struct ReceiptFixture {
    fixture: super::tests::Fixture,
    integrity: ReceiptIntegrityStore,
    redactor: ReceiptRedactor,
    key: ReceiptKeyRef,
    anchor_dir: PathBuf,
    protector: Arc<TestProtector>,
}

impl ReceiptFixture {
    fn new() -> Self {
        Self::new_with_seed(0x5a)
    }

    fn new_with_seed(key_seed: u8) -> Self {
        let fixture = fixture();
        fixture
            .store
            .create_foundation(&foundation())
            .expect("create receipt foundation");
        seed_runtime_and_evidence(&fixture.paths);
        let anchor_dir = fixture
            .paths
            .root
            .parent()
            .expect("fixture root parent")
            .join("receipt-anchor");
        let protector = Arc::new(TestProtector::with_seed(key_seed));
        let integrity = ReceiptIntegrityStore::with_protector(
            &fixture.paths,
            anchor_dir.clone(),
            protector.clone(),
        )
        .expect("open receipt integrity store");
        let key = integrity
            .provision_initial_key("receipt-key-1")
            .expect("provision initial receipt key");
        let redactor = ReceiptRedactor::new(&["fixture-secret-not-for-persistence"])
            .expect("receipt redactor");
        Self {
            fixture,
            integrity,
            redactor,
            key,
            anchor_dir,
            protector,
        }
    }

    fn command(&self, suffix: &str) -> AppendReceiptCommand {
        receipt_command(self.key.clone(), suffix)
    }

    fn heads(&self) -> (i64, Option<String>, i64, Option<String>) {
        Connection::open(&self.fixture.paths.db_path)
            .unwrap()
            .query_row(
                "SELECT j.receipt_count,j.receipt_head_digest,d.receipt_chain_count,d.receipt_chain_head_digest FROM execass_receipt_journal_state j CROSS JOIN execass_delegations d WHERE j.singleton=1 AND d.delegation_id='delegation-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    }

    fn reopened_integrity(&self) -> ReceiptIntegrityStore {
        ReceiptIntegrityStore::with_protector(
            &self.fixture.paths,
            self.anchor_dir.clone(),
            self.protector.clone(),
        )
        .expect("reopen receipt integrity store")
    }

    fn integrity_with_failpoint(&self, name: &'static str) -> ReceiptIntegrityStore {
        ReceiptIntegrityStore::with_protector_and_failpoints(
            &self.fixture.paths,
            self.anchor_dir.clone(),
            self.integrity.root_identity().to_owned(),
            self.protector.clone(),
            Arc::new(FailOnce::new(name)),
        )
        .expect("open receipt integrity failpoint store")
    }

    fn append_next(&self, suffix: &str) -> ReceiptRecord {
        let (global_count, global_head, delegation_count, delegation_head) = self.heads();
        let mut command = self.command(suffix);
        command.expected_global_count = global_count;
        command.expected_global_head_digest = global_head;
        command.expected_delegation_count = delegation_count;
        command.expected_delegation_head_digest = delegation_head;
        if global_count > 0 {
            let event_id = format!("event-{suffix}");
            let cause = format!("cause-{suffix}");
            command.causation_event_id = event_id.clone();
            command.causation_id = cause.clone();
            command.subject.subject_id = event_id.clone();
            command.occurred_at += global_count * 100;
            command.committed_at += global_count * 100;
            open_sqlite_connection(&self.fixture.paths.db_path)
                .expect("open next receipt event database")
                .execute(
                    "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.runtime_host.changed','delegation-1',1,?2,?3,?4,'v1','{}',?5)",
                    params![event_id, format!("corr-{suffix}"), cause, command.occurred_at, format!("duplicate-{suffix}")],
                )
                .expect("insert next receipt event");
        }
        match self
            .fixture
            .store
            .append_receipt(&self.integrity, &self.redactor, &command)
            .expect("append next receipt")
        {
            AppendReceiptOutcome::Appended(record) => record,
            other => panic!("expected appended receipt, got {other:?}"),
        }
    }
}

fn receipt_command(key: ReceiptKeyRef, suffix: &str) -> AppendReceiptCommand {
    AppendReceiptCommand {
        receipt_id: format!("receipt-{suffix}"),
        transaction_id: format!("receipt-tx-{suffix}"),
        state_root_generation: 1,
        delegation_id: "delegation-1".into(),
        expected_state_revision: 1,
        expected_global_count: 0,
        expected_global_head_digest: None,
        expected_delegation_count: 0,
        expected_delegation_head_digest: None,
        receipt_kind: ReceiptKind::Intake,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::OutboxEvent,
            subject_id: "event-foundation-1".into(),
            revision: 1,
        },
        causation_id: "cause-foundation-1".into(),
        causation_event_id: "event-foundation-1".into(),
        actor: ReceiptActorBinding {
            actor_type: ActorType::HumanLocal,
            actor_identity: SafeText::new("local-operator", &[]).unwrap(),
            authority_provenance_id: "authority-1".into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "receipt-host-1".into(),
            fencing_token: 1,
        },
        key,
        rotation: None,
        evidence: vec![ReceiptEvidenceInput {
            authority_link_id: "receipt-session-link".into(),
            kind: AuthorityLinkKind::Session,
            source_id: "receipt-session".into(),
            authoritative_revision: 0,
        }],
        redacted_summary: SafeText::summary("delegation accepted", &[]).unwrap(),
        occurred_at: 1_800_000_000_000,
        committed_at: 1_800_000_000_010,
    }
}

#[test]
fn advancing_atomic_receipt_binds_pre_state_to_one_post_revision_and_replays_without_writes() {
    let fixture = ReceiptFixture::new();
    let mut command = fixture.command("advancing-recovery");
    command.expected_state_revision = 2;
    command.receipt_kind = ReceiptKind::Recovery;
    command.causation_id = "recovery-evaluation-1".into();
    command.causation_event_id = "recovery-event-1".into();
    command.subject = ReceiptSubject {
        kind: ReceiptSubjectKind::OutboxEvent,
        subject_id: "recovery-event-1".into(),
        revision: 2,
    };
    command.redacted_summary = SafeText::summary("objective recovery updated", &[]).unwrap();

    let outcome = fixture
        .fixture
        .store
        .mutate_with_advancing_atomic_receipt(
            &fixture.integrity,
            &fixture.redactor,
            1,
            &command,
            |transaction| {
                transaction.execute(
                    "UPDATE execass_delegations SET state_revision=2,updated_at=?1 WHERE delegation_id='delegation-1' AND state_revision=1",
                    [command.occurred_at],
                )?;
                transaction.execute(
                    r#"INSERT INTO execass_outbox_events(
                         event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
                         causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
                       ) VALUES(?1,'execass.v1.recovery.updated','delegation-1',2,?2,?3,?4,'v1','{}',?1)"#,
                    params![
                        command.causation_event_id,
                        "recovery-correlation-1",
                        command.causation_id,
                        command.occurred_at,
                    ],
                )?;
                Ok(AtomicReceiptMutation::Append("applied"))
            },
        )
        .unwrap();
    let AtomicReceiptWriteOutcome::Appended { value, receipt } = outcome else {
        panic!("expected advancing append")
    };
    assert_eq!(value, "applied");
    assert_eq!(receipt.receipt_id, command.receipt_id);

    let replay = fixture
        .fixture
        .store
        .mutate_with_advancing_atomic_receipt(
            &fixture.integrity,
            &fixture.redactor,
            1,
            &command,
            |_| Ok(AtomicReceiptMutation::NoAppend("replayed")),
        )
        .unwrap();
    assert!(matches!(
        replay,
        AtomicReceiptWriteOutcome::NoAppend("replayed")
    ));
    let connection = open_sqlite_connection(&fixture.fixture.paths.db_path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM execass_receipts WHERE receipt_id=?1",
                [&command.receipt_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[cfg(windows)]
#[test]
fn production_dpapi_verifier_trusts_only_a_typed_receipt_append() {
    let fixture = fixture();
    fixture
        .store
        .create_foundation(&foundation())
        .expect("create production receipt foundation");
    seed_runtime_and_evidence(&fixture.paths);
    let integrity = ReceiptIntegrityStore::open(&fixture.paths)
        .expect("open fixed production receipt verifier");
    let key = integrity
        .provision_initial_key("production-typed-append-key")
        .expect("provision production DPAPI key");
    let outcome = fixture
        .store
        .append_receipt(
            &integrity,
            &ReceiptRedactor::new(&["production-fixture-secret"]).unwrap(),
            &receipt_command(key, "production-typed"),
        )
        .expect("append through typed receipt authority");
    assert!(matches!(outcome, AppendReceiptOutcome::Appended(_)));
    assert!(matches!(
        ReceiptIntegrityStore::open(&fixture.paths)
            .expect("restart production verifier")
            .status()
            .expect("production verifier status"),
        IntegrityStatus::Trusted {
            receipt_count: 1,
            ..
        }
    ));
}

fn seed_runtime_and_evidence(paths: &crate::AppPaths) {
    let connection = open_sqlite_connection(&paths.db_path).expect("open receipt fixture database");
    connection
        .execute_batch(
            r#"
            INSERT INTO agents(agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at)
              VALUES('receipt-agent','receipt agent','.','test','test','default',1,1);
            INSERT INTO sessions(session_id,session_key,agent_id,created_at,updated_at)
              VALUES('receipt-session','receipt-session-key','receipt-agent',1,1);
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'installation-1','user-digest-1','receipt-host-1',1);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('receipt-lease-1','execass',1,'receipt-host-1',1,1,9999999999999);
            INSERT INTO execass_authority_links(
              link_id,delegation_id,link_revision,delegation_state_revision,correlation_id,
              causation_id,outbox_event_id,authority_kind,session_id,authoritative_revision,linked_at
            ) VALUES(
              'receipt-session-link','delegation-1',1,1,'corr-foundation-1',
              'cause-foundation-1','event-foundation-1','session','receipt-session',0,1800000000000
            );
            "#,
        )
        .expect("seed receipt runtime and evidence");
}

fn adversarial_connection(path: &std::path::Path) -> Connection {
    let connection = Connection::open(path).expect("open adversarial receipt database");
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .expect("disable triggers only on adversarial connection");
    connection
        .pragma_update(None, "foreign_keys", "OFF")
        .expect("disable foreign keys only on adversarial connection");
    connection
}

#[test]
fn ea112_case_inventory_has_exact_required_classes_and_no_omissions() {
    assert_ea112_case_registered("inventory_has_no_omissions");
    let mut names = EA112_CASES.iter().map(|case| case.name).collect::<Vec<_>>();
    names.sort_unstable();
    let original_len = names.len();
    names.dedup();
    assert_eq!(
        names.len(),
        original_len,
        "EA-112 case names must be unique"
    );
    let requirements = EA112_CASES
        .iter()
        .map(|case| case.requirement)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(requirements, (1_u8..=11).collect());
    let mut classes = EA112_CASES
        .iter()
        .map(|case| case.class)
        .collect::<Vec<_>>();
    classes.sort_unstable();
    classes.dedup();
    assert_eq!(
        classes.len(),
        EA112_CASES.len(),
        "each locked mutation class must have one named inventory entry"
    );
}

#[test]
fn ea112_fixed_verifier_construction_and_safe_diagnostic_source_are_locked() {
    for name in [
        "quarantine_blocks_trusted_operations",
        "safe_non_echoing_verifier",
        "exact_reason_and_first_difference",
    ] {
        assert_ea112_case_registered(name);
    }
    let integrity_source = include_str!("receipt_integrity.rs");
    for forbidden in [
        "pub trait ReceiptKeyProtector",
        "pub trait IntegrityFailpoints",
        "pub fn with_protector(",
        "pub fn with_protector_and_failpoints(",
        "pub fn prepare_anchor(",
        "pub fn confirm_prepared_anchor_in_transaction(",
        "pub fn finalize_anchor(",
    ] {
        assert!(
            !integrity_source.contains(forbidden),
            "public forgery seam returned: {forbidden}"
        );
    }
    let verifier_source = include_str!("../bin/carsinos-receipt-integrity.rs");
    assert!(!verifier_source.contains("\"state_root\": state_root"));
    assert!(verifier_source.contains("\"root_identity\": store.root_identity()"));
    assert!(!verifier_source.contains("failed: {error:#}"));
}

#[test]
fn ea112_raw_receipt_tamper_matrix_quarantines_at_the_first_difference() {
    struct Case {
        name: &'static str,
        expected_reason: &'static str,
        expected_sequence: Option<i64>,
        expected_receipt_id: Option<&'static str>,
    }
    let cases = [
        Case {
            name: "tail_truncation",
            expected_reason: "receipt_anchor_count_head_mismatch",
            expected_sequence: Some(3),
            expected_receipt_id: None,
        },
        Case {
            name: "prefix_truncation",
            expected_reason: "receipt_global_sequence_mismatch",
            expected_sequence: Some(1),
            expected_receipt_id: Some("receipt-two"),
        },
        Case {
            name: "receipt_table_deletion",
            expected_reason: "receipt_anchor_count_head_mismatch",
            expected_sequence: Some(1),
            expected_receipt_id: None,
        },
        Case {
            name: "historical_row_insertion",
            expected_reason: "receipt_digest_mismatch",
            expected_sequence: Some(4),
            expected_receipt_id: Some("receipt-inserted"),
        },
        Case {
            name: "historical_row_modification",
            expected_reason: "receipt_payload_binding_mismatch",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-two"),
        },
        Case {
            name: "historical_row_deletion",
            expected_reason: "receipt_global_sequence_mismatch",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-three"),
        },
        Case {
            name: "historical_row_reordering",
            expected_reason: "receipt_global_chain_mismatch",
            expected_sequence: Some(1),
            expected_receipt_id: Some("receipt-two"),
        },
        Case {
            name: "global_chain_fork",
            expected_reason: "receipt_global_chain_mismatch",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-two"),
        },
        Case {
            name: "global_chain_gap",
            expected_reason: "receipt_global_sequence_mismatch",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-three"),
        },
        Case {
            name: "global_count_mismatch",
            expected_reason: "receipt_global_head_mismatch",
            expected_sequence: Some(3),
            expected_receipt_id: Some("receipt-three"),
        },
        Case {
            name: "global_head_mismatch",
            expected_reason: "receipt_global_head_mismatch",
            expected_sequence: Some(3),
            expected_receipt_id: Some("receipt-three"),
        },
        Case {
            name: "delegation_chain_fork",
            expected_reason: "receipt_delegation_chain_mismatch",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-two"),
        },
        Case {
            name: "delegation_chain_gap",
            expected_reason: "receipt_delegation_chain_mismatch",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-two"),
        },
        Case {
            name: "delegation_count_mismatch",
            expected_reason: "receipt_delegation_head_mismatch",
            expected_sequence: None,
            expected_receipt_id: Some("receipt-three"),
        },
        Case {
            name: "delegation_head_mismatch",
            expected_reason: "receipt_delegation_head_mismatch",
            expected_sequence: None,
            expected_receipt_id: Some("receipt-three"),
        },
        Case {
            name: "alternate_serialization_insert",
            expected_reason: "receipt_canonical_payload_noncanonical",
            expected_sequence: Some(4),
            expected_receipt_id: Some("receipt-inserted"),
        },
        Case {
            name: "alternate_serialization_rewrite",
            expected_reason: "receipt_canonical_payload_noncanonical",
            expected_sequence: Some(2),
            expected_receipt_id: Some("receipt-two"),
        },
    ];
    for case in cases {
        assert_ea112_case_registered(case.name);
        let receipt = ReceiptFixture::new();
        receipt.append_next("one");
        receipt.append_next("two");
        receipt.append_next("three");
        assert!(
            matches!(
                receipt
                    .reopened_integrity()
                    .status()
                    .expect("untampered fixture status"),
                IntegrityStatus::Trusted {
                    receipt_count: 3,
                    ..
                }
            ),
            "untampered fixture failed for {}",
            case.name
        );

        let connection = adversarial_connection(&receipt.fixture.paths.db_path);
        match case.name {
            "tail_truncation" => {
                connection.execute("DELETE FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-three'", []).unwrap();
                connection
                    .execute(
                        "DELETE FROM execass_receipts WHERE receipt_id='receipt-three'",
                        [],
                    )
                    .unwrap();
            }
            "prefix_truncation" => {
                connection
                    .execute(
                        "DELETE FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-one'",
                        [],
                    )
                    .unwrap();
                connection
                    .execute(
                        "DELETE FROM execass_receipts WHERE receipt_id='receipt-one'",
                        [],
                    )
                    .unwrap();
            }
            "receipt_table_deletion" => {
                connection
                    .execute("DELETE FROM execass_receipt_evidence_refs", [])
                    .unwrap();
                connection
                    .execute("DELETE FROM execass_receipts", [])
                    .unwrap();
            }
            "historical_row_insertion" => {
                connection.execute("INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES('event-inserted','execass.v1.runtime_host.changed','delegation-1',1,'corr-inserted','cause-inserted',1800000000999,'v1','{}','duplicate-inserted')", []).unwrap();
                connection.execute(
                    "INSERT INTO execass_receipts SELECT 'receipt-inserted',delegation_id,4,4,?1,receipt_kind,'cause-inserted','event-inserted','receipt-three','receipt-three',subject_kind,'event-inserted',subject_revision,actor_type,actor_identity,actor_authority_provenance_id,runtime_host_generation,runtime_host_instance_id,runtime_fencing_token,state_revision,canonical_payload,?2,hash_algorithm,key_id,key_generation,receipt_digest,receipt_digest,?3,?4,previous_key_integrity_tag,redacted_summary,occurred_at,committed_at FROM execass_receipts WHERE receipt_id='receipt-three'",
                    params!["f".repeat(64), "carsinos.execass.receipt.cjson.v1", "e".repeat(64), "d".repeat(64)],
                ).unwrap();
            }
            "alternate_serialization_insert" => {
                let canonical: Vec<u8> = connection
                    .query_row(
                        "SELECT canonical_payload FROM execass_receipts WHERE receipt_id='receipt-three'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                let value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
                let alternate = serde_json::to_vec_pretty(&value).unwrap();
                assert_ne!(alternate, canonical);
                assert!(serde_json::from_slice::<serde_json::Value>(&alternate).is_ok());
                connection.execute("INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES('event-inserted','execass.v1.runtime_host.changed','delegation-1',1,'corr-inserted','cause-inserted',1800000000999,'v1','{}','duplicate-inserted')", []).unwrap();
                connection.execute(
                    "INSERT INTO execass_receipts SELECT 'receipt-inserted',delegation_id,4,4,?1,receipt_kind,'cause-inserted','event-inserted','receipt-three','receipt-three',subject_kind,'event-inserted',subject_revision,actor_type,actor_identity,actor_authority_provenance_id,runtime_host_generation,runtime_host_instance_id,runtime_fencing_token,state_revision,?2,serialization_version,hash_algorithm,key_id,key_generation,receipt_digest,receipt_digest,?3,?4,previous_key_integrity_tag,redacted_summary,occurred_at,committed_at FROM execass_receipts WHERE receipt_id='receipt-three'",
                    params!["f".repeat(64), alternate, "e".repeat(64), "d".repeat(64)],
                ).unwrap();
            }
            "historical_row_modification" => {
                connection.execute("UPDATE execass_receipts SET redacted_summary='attacker rewrite' WHERE receipt_id='receipt-two'", []).unwrap();
            }
            "historical_row_deletion" => {
                connection
                    .execute(
                        "DELETE FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-two'",
                        [],
                    )
                    .unwrap();
                connection
                    .execute(
                        "DELETE FROM execass_receipts WHERE receipt_id='receipt-two'",
                        [],
                    )
                    .unwrap();
            }
            "historical_row_reordering" => {
                connection.execute("UPDATE execass_receipts SET global_sequence=99 WHERE receipt_id='receipt-one'", []).unwrap();
                connection.execute("UPDATE execass_receipts SET global_sequence=1 WHERE receipt_id='receipt-two'", []).unwrap();
                connection.execute("UPDATE execass_receipts SET global_sequence=2 WHERE receipt_id='receipt-one'", []).unwrap();
            }
            "global_chain_fork" => {
                connection.execute("UPDATE execass_receipts SET global_previous_receipt_digest=?1 WHERE receipt_id='receipt-two'", ["0".repeat(64)]).unwrap();
            }
            "global_chain_gap" => {
                connection.execute("UPDATE execass_receipts SET global_sequence=9 WHERE receipt_id='receipt-two'", []).unwrap();
            }
            "global_count_mismatch" => {
                connection.execute("UPDATE execass_receipt_journal_state SET receipt_count=99 WHERE singleton=1", []).unwrap();
            }
            "global_head_mismatch" => {
                connection.execute("UPDATE execass_receipt_journal_state SET receipt_head_digest=?1 WHERE singleton=1", ["0".repeat(64)]).unwrap();
            }
            "delegation_chain_fork" => {
                connection.execute("UPDATE execass_receipts SET previous_receipt_digest=?1 WHERE receipt_id='receipt-two'", ["0".repeat(64)]).unwrap();
            }
            "delegation_chain_gap" => {
                connection.execute("UPDATE execass_receipts SET receipt_sequence=9 WHERE receipt_id='receipt-two'", []).unwrap();
            }
            "delegation_count_mismatch" => {
                connection.execute("UPDATE execass_delegations SET receipt_chain_count=99 WHERE delegation_id='delegation-1'", []).unwrap();
            }
            "delegation_head_mismatch" => {
                connection.execute("UPDATE execass_delegations SET receipt_chain_head_digest=?1 WHERE delegation_id='delegation-1'", ["0".repeat(64)]).unwrap();
            }
            "alternate_serialization_rewrite" => {
                let canonical: Vec<u8> = connection
                    .query_row(
                        "SELECT canonical_payload FROM execass_receipts WHERE receipt_id='receipt-two'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                let value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
                let alternate = serde_json::to_vec_pretty(&value).unwrap();
                assert_ne!(alternate, canonical);
                assert!(serde_json::from_slice::<serde_json::Value>(&alternate).is_ok());
                connection
                    .execute(
                        "UPDATE execass_receipts SET canonical_payload=?1 WHERE receipt_id='receipt-two'",
                        [alternate],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        drop(connection);

        let reopened = receipt.integrity.clone();
        let failure = reopened
            .receipt_history_failure()
            .expect("receipt history diagnosis")
            .expect("tamper must produce a receipt history failure");
        assert_eq!(
            failure.category, "receipt_history",
            "{} category",
            case.name
        );
        assert_eq!(failure.reason, case.expected_reason, "{} reason", case.name);
        assert_eq!(
            failure.first_global_sequence, case.expected_sequence,
            "{} sequence",
            case.name
        );
        assert_eq!(
            failure.first_receipt_id.as_deref(),
            case.expected_receipt_id,
            "{} identity",
            case.name
        );
        let expected_status_reason = match case.expected_sequence {
            Some(sequence) => format!("{}_at_global_sequence_{sequence}", case.expected_reason),
            None => case.expected_reason.to_owned(),
        };
        assert_eq!(
            reopened.status().expect("tampered restart status"),
            IntegrityStatus::Mismatch {
                reason: expected_status_reason.clone()
            },
            "{} restart status",
            case.name
        );
        assert_eq!(
            reopened.recover_integrity().expect("tamper quarantine"),
            IntegrityRecovery::Quarantined {
                reason: expected_status_reason.clone()
            },
            "{} quarantine",
            case.name
        );
        assert_eq!(
            reopened.status().expect("persistent quarantine status"),
            IntegrityStatus::Quarantined {
                reason: expected_status_reason
            },
            "{} persistent quarantine",
            case.name
        );
        let blocked = receipt.fixture.store.append_receipt(
            &reopened,
            &receipt.redactor,
            &receipt.command("blocked"),
        );
        assert!(
            blocked.is_err(),
            "{} allowed append after quarantine",
            case.name
        );
    }
}

#[test]
fn ea112_evidence_row_tamper_matrix_quarantines_at_the_first_difference() {
    struct Case {
        name: &'static str,
        expected_reason: &'static str,
    }
    for case in [
        Case {
            name: "evidence_row_insertion",
            expected_reason: "receipt_evidence_count_mismatch",
        },
        Case {
            name: "evidence_row_update",
            expected_reason: "receipt_evidence_binding_mismatch",
        },
        Case {
            name: "evidence_row_deletion",
            expected_reason: "receipt_evidence_count_mismatch",
        },
        Case {
            name: "evidence_row_reordering",
            expected_reason: "receipt_evidence_binding_mismatch",
        },
    ] {
        assert_ea112_case_registered(case.name);
        let receipt = ReceiptFixture::new();
        let setup = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
        setup
            .execute_batch(
                r#"
                INSERT INTO sessions(session_id,session_key,agent_id,created_at,updated_at)
                  VALUES('receipt-session-two','receipt-session-key-two','receipt-agent',2,2);
                INSERT INTO execass_authority_links(
                  link_id,delegation_id,link_revision,delegation_state_revision,correlation_id,
                  causation_id,outbox_event_id,authority_kind,session_id,authoritative_revision,linked_at
                ) VALUES(
                  'receipt-session-link-two','delegation-1',2,1,'corr-foundation-1',
                  'cause-foundation-1','event-foundation-1','session','receipt-session-two',0,1800000000000
                );
                "#,
            )
            .unwrap();
        drop(setup);

        let mut command = receipt.command("evidence-ordered");
        command.evidence.push(ReceiptEvidenceInput {
            authority_link_id: "receipt-session-link-two".into(),
            kind: AuthorityLinkKind::Session,
            source_id: "receipt-session-two".into(),
            authoritative_revision: 0,
        });
        assert!(matches!(
            receipt
                .fixture
                .store
                .append_receipt(&receipt.integrity, &receipt.redactor, &command)
                .unwrap(),
            AppendReceiptOutcome::Appended(_)
        ));
        assert!(matches!(
            receipt.integrity.status().unwrap(),
            IntegrityStatus::Trusted {
                receipt_count: 1,
                ..
            }
        ));

        let connection = adversarial_connection(&receipt.fixture.paths.db_path);
        match case.name {
            "evidence_row_insertion" => {
                connection
                    .execute(
                        "INSERT INTO execass_receipt_evidence_refs(receipt_id,ordinal,authority_kind,source_id,authoritative_revision,authority_link_id,observation_digest,deep_link) VALUES('receipt-evidence-ordered',2,'run','attacker-run',0,'attacker-link',?1,'carsinos://evidence/v1/run/attacker-run?revision=0')",
                        ["f".repeat(64)],
                    )
                    .unwrap();
                let count: i64 = connection
                    .query_row(
                        "SELECT COUNT(*) FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-evidence-ordered'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(count, 3);
            }
            "evidence_row_update" => {
                assert_eq!(
                    connection
                        .execute(
                            "UPDATE execass_receipt_evidence_refs SET source_id='attacker-session' WHERE receipt_id='receipt-evidence-ordered' AND ordinal=0",
                            [],
                        )
                        .unwrap(),
                    1
                );
            }
            "evidence_row_deletion" => {
                assert_eq!(
                    connection
                        .execute(
                            "DELETE FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-evidence-ordered' AND ordinal=0",
                            [],
                        )
                        .unwrap(),
                    1
                );
            }
            "evidence_row_reordering" => {
                connection
                    .execute(
                        "UPDATE execass_receipt_evidence_refs SET ordinal=9 WHERE receipt_id='receipt-evidence-ordered' AND ordinal=0",
                        [],
                    )
                    .unwrap();
                connection
                    .execute(
                        "UPDATE execass_receipt_evidence_refs SET ordinal=0 WHERE receipt_id='receipt-evidence-ordered' AND ordinal=1",
                        [],
                    )
                    .unwrap();
                connection
                    .execute(
                        "UPDATE execass_receipt_evidence_refs SET ordinal=1 WHERE receipt_id='receipt-evidence-ordered' AND ordinal=9",
                        [],
                    )
                    .unwrap();
                let first_source: String = connection
                    .query_row(
                        "SELECT source_id FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-evidence-ordered' AND ordinal=0",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(first_source, "receipt-session-two");
            }
            _ => unreachable!(),
        }
        drop(connection);

        let failure = receipt
            .integrity
            .receipt_history_failure()
            .unwrap()
            .expect("evidence tamper must be diagnosed");
        assert_eq!(failure.reason, case.expected_reason, "{}", case.name);
        assert_eq!(failure.first_global_sequence, Some(1), "{}", case.name);
        assert_eq!(
            failure.first_receipt_id.as_deref(),
            Some("receipt-evidence-ordered"),
            "{}",
            case.name
        );
        let reason = format!("{}_at_global_sequence_1", case.expected_reason);
        assert_eq!(
            receipt.integrity.status().unwrap(),
            IntegrityStatus::Mismatch {
                reason: reason.clone()
            }
        );
        assert_eq!(
            receipt.integrity.recover_integrity().unwrap(),
            IntegrityRecovery::Quarantined {
                reason: reason.clone()
            }
        );
        assert_eq!(
            receipt.integrity.status().unwrap(),
            IntegrityStatus::Quarantined { reason }
        );
        assert!(receipt
            .fixture
            .store
            .append_receipt(
                &receipt.integrity,
                &receipt.redactor,
                &receipt.command("blocked-after-evidence-tamper"),
            )
            .is_err());
    }
}

#[test]
fn ea112_cross_key_restore_uses_an_independently_provisioned_root_and_quarantines() {
    assert_ea112_case_registered("cross_key_restore");
    let source = ReceiptFixture::new_with_seed(0xbc);
    let source_material = source.protector.load(&source.key).unwrap().to_vec();
    source.append_next("source-root");
    assert!(matches!(
        source.integrity.status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 1,
            ..
        }
    ));

    let target = ReceiptFixture::new();
    target.append_next("target-root");
    assert!(matches!(
        target.integrity.status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 1,
            ..
        }
    ));
    assert_ne!(
        source.integrity.root_identity(),
        target.integrity.root_identity(),
        "cross-key source must be a distinct independently provisioned root"
    );
    let target_material = target.protector.load(&target.key).unwrap().to_vec();
    assert_ne!(source_material, target_material);

    target.protector.keys.lock().unwrap().insert(
        (target.key.key_id.clone(), target.key.key_generation),
        source.protector.load(&source.key).unwrap().to_vec(),
    );
    assert_eq!(
        target.integrity.status().unwrap(),
        IntegrityStatus::Mismatch {
            reason: "receipt_commit_confirmation_mismatch".into()
        }
    );
    assert_eq!(
        target.integrity.recover_integrity().unwrap(),
        IntegrityRecovery::Quarantined {
            reason: "receipt_commit_confirmation_mismatch".into()
        }
    );
    assert_eq!(
        target.integrity.status().unwrap(),
        IntegrityStatus::Quarantined {
            reason: "receipt_commit_confirmation_mismatch".into()
        }
    );
    assert!(target
        .fixture
        .store
        .append_receipt(
            &target.integrity,
            &target.redactor,
            &target.command("blocked-after-cross-key-restore"),
        )
        .is_err());
}

#[test]
fn ea112_every_benign_integrity_boundary_recovers_and_allows_two_valid_appends() {
    assert_ea112_case_registered("benign_prepare_boundaries");
    assert_ea112_case_registered("benign_finalize_boundaries");
    let prepare_failpoints = [
        "anchor.prepare.write.before",
        "anchor.prepare.write.after",
        "anchor.prepare.sync.after",
        "anchor.prepare.rename.after",
        "anchor.prepare.dir_sync.after",
        "db.prepare.before",
        "db.prepare.after",
    ];
    let finalize_failpoints = [
        "anchor.finalized_version.write.before",
        "anchor.finalized_version.write.after",
        "anchor.finalized_version.sync.after",
        "anchor.finalized_version.rename.after",
        "anchor.finalized_version.dir_sync.after",
        "anchor.current.write.before",
        "anchor.current.write.after",
        "anchor.current.sync.after",
        "anchor.current.rename.after",
        "anchor.current.dir_sync.after",
        "db.finalize.before",
        "db.finalize.after",
    ];
    for failpoint in prepare_failpoints.into_iter().chain(finalize_failpoints) {
        let receipt = ReceiptFixture::new();
        let command = receipt.command("interrupted");
        let failing = receipt.integrity_with_failpoint(failpoint);
        assert!(
            receipt
                .fixture
                .store
                .append_receipt(&failing, &receipt.redactor, &command)
                .is_err(),
            "{failpoint} did not interrupt"
        );
        let recovery = receipt
            .integrity
            .recover_integrity()
            .expect("recover benign integrity interruption");
        assert!(
            !matches!(recovery, IntegrityRecovery::Quarantined { .. }),
            "{failpoint} caused false quarantine: {recovery:?}"
        );
        let retry = receipt
            .fixture
            .store
            .append_receipt(&receipt.integrity, &receipt.redactor, &command)
            .expect("retry interrupted receipt");
        assert!(
            matches!(
                retry,
                AppendReceiptOutcome::Appended(_) | AppendReceiptOutcome::Replayed(_)
            ),
            "{failpoint} did not converge to one receipt: {retry:?}"
        );
        receipt.append_next("after-recovery");
        assert!(
            matches!(
                receipt.integrity.status().expect("post-recovery status"),
                IntegrityStatus::Trusted {
                    receipt_count: 2,
                    ..
                }
            ),
            "{failpoint} did not allow a subsequent valid receipt"
        );
    }
}

#[test]
fn ea112_every_benign_receipt_restart_boundary_recovers_and_allows_progress() {
    assert_ea112_case_registered("benign_receipt_restart_boundaries");
    for failpoint in [
        "receipt.after_prepare",
        "receipt.after_insert",
        "receipt.after_evidence",
        "receipt.after_global_head",
        "receipt.after_delegation_head",
        "receipt.after_confirm",
        "receipt.after_commit",
        "receipt.before_finalize",
        "receipt.after_finalize",
    ] {
        let receipt = ReceiptFixture::new();
        let command = receipt.command("interrupted");
        assert!(
            receipt
                .fixture
                .store
                .append_receipt_with_failpoints(
                    &receipt.integrity,
                    &receipt.redactor,
                    &command,
                    Arc::new(FailOnce::new(failpoint)),
                )
                .is_err(),
            "{failpoint} did not interrupt"
        );
        let recovery = receipt
            .integrity
            .recover_integrity()
            .expect("recover benign receipt interruption");
        assert!(
            !matches!(recovery, IntegrityRecovery::Quarantined { .. }),
            "{failpoint} caused false quarantine: {recovery:?}"
        );
        let retry = receipt
            .fixture
            .store
            .append_receipt(&receipt.integrity, &receipt.redactor, &command)
            .expect("retry interrupted receipt");
        assert!(matches!(
            retry,
            AppendReceiptOutcome::Appended(_) | AppendReceiptOutcome::Replayed(_)
        ));
        receipt.append_next("after-recovery");
        assert!(
            matches!(
                receipt.integrity.status().expect("post-recovery status"),
                IntegrityStatus::Trusted {
                    receipt_count: 2,
                    ..
                }
            ),
            "{failpoint} did not allow subsequent append"
        );
    }
}

#[test]
fn ea112_database_rollback_deletion_and_receipt_table_drop_are_detected() {
    for name in [
        "full_database_rollback",
        "full_database_deletion",
        "receipt_table_deletion",
    ] {
        assert_ea112_case_registered(name);
        let receipt = ReceiptFixture::new();
        let rollback = receipt.fixture.paths.root.join(format!("{name}.sqlite"));
        if name == "full_database_rollback" {
            open_sqlite_connection(&receipt.fixture.paths.db_path)
                .unwrap()
                .execute("VACUUM INTO ?1", [rollback.to_string_lossy().as_ref()])
                .unwrap();
        }
        receipt.append_next("one");
        receipt.append_next("two");
        assert!(matches!(
            receipt.integrity.status().unwrap(),
            IntegrityStatus::Trusted {
                receipt_count: 2,
                ..
            }
        ));
        match name {
            "full_database_rollback" => {
                fs::remove_file(&receipt.fixture.paths.db_path).unwrap();
                fs::rename(&rollback, &receipt.fixture.paths.db_path).unwrap();
                assert_eq!(
                    receipt.integrity.status().unwrap(),
                    IntegrityStatus::Mismatch {
                        reason: "external_anchor_without_database_state".into()
                    }
                );
                assert_eq!(
                    receipt.integrity.recover_integrity().unwrap(),
                    IntegrityRecovery::Quarantined {
                        reason: "external_anchor_without_database_state".into()
                    }
                );
            }
            "full_database_deletion" => {
                let restored = receipt
                    .fixture
                    .paths
                    .root
                    .join("full-database-deletion-restore.sqlite");
                let connection = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
                connection
                    .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                    .unwrap();
                connection
                    .execute("VACUUM INTO ?1", [restored.to_string_lossy().as_ref()])
                    .unwrap();
                drop(connection);
                fs::remove_file(&receipt.fixture.paths.db_path).unwrap();
                assert_eq!(
                    receipt.integrity.status().unwrap(),
                    IntegrityStatus::Mismatch {
                        reason: "external_anchor_without_database_state".into()
                    }
                );
                assert_eq!(
                    receipt.integrity.recover_integrity().unwrap(),
                    IntegrityRecovery::Quarantined {
                        reason: "external_anchor_without_database_state".into()
                    }
                );
                assert!(receipt.anchor_dir.join("quarantine.json").is_file());
                assert!(
                    !receipt.fixture.paths.db_path.exists(),
                    "external quarantine recreated the deleted receipt database"
                );
                assert_eq!(
                    receipt.integrity.status().unwrap(),
                    IntegrityStatus::Quarantined {
                        reason: "external_anchor_without_database_state".into()
                    }
                );

                fs::rename(&restored, &receipt.fixture.paths.db_path).unwrap();
                let reopened = receipt.reopened_integrity();
                assert_eq!(
                    reopened.status().unwrap(),
                    IntegrityStatus::Quarantined {
                        reason: "external_anchor_without_database_state".into()
                    }
                );
                let blocked = receipt.fixture.store.append_receipt(
                    &reopened,
                    &receipt.redactor,
                    &receipt.command("blocked-after-database-reappearance"),
                );
                assert!(blocked.is_err());
                assert_eq!(receipt.heads().0, 2);
            }
            "receipt_table_deletion" => {
                let connection = adversarial_connection(&receipt.fixture.paths.db_path);
                connection
                    .execute("DROP TABLE execass_receipts", [])
                    .unwrap();
                drop(connection);
                assert_eq!(
                    receipt.integrity.status().unwrap(),
                    IntegrityStatus::Mismatch {
                        reason: "receipt_history_query_failed".into()
                    }
                );
                assert_eq!(
                    receipt.integrity.recover_integrity().unwrap(),
                    IntegrityRecovery::Quarantined {
                        reason: "receipt_history_query_failed".into()
                    }
                );
            }
            _ => unreachable!(),
        }
    }
}

fn second_foundation() -> CreateFoundationCommand {
    let mut command = foundation();
    command.write.idempotency_key = "idem-foundation-2".into();
    command.write.correlation_id = "corr-foundation-2".into();
    command.write.causation_id = "cause-foundation-2".into();
    command.authority.authority_provenance_id = "authority-2".into();
    command.authority.source_correlation_id = "corr-foundation-2".into();
    command.authority.source_message_id = Some("message-2".into());
    command.delegation.delegation_id = "delegation-2".into();
    command.delegation.source_message_id = Some("message-2".into());
    command.delegation.source_correlation_id = "corr-foundation-2".into();
    command.delegation.ingress_idempotency_key = "idem-foundation-2".into();
    command.delegation.authority_provenance_id = "authority-2".into();
    command.plan.plan_id = "plan-2".into();
    command.plan.delegation_id = "delegation-2".into();
    command.plan.created_by_authority_provenance_id = "authority-2".into();
    for (index, criterion) in command.outcome_criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("criterion-2-{index}");
        criterion.delegation_id = "delegation-2".into();
    }
    let continuation = command.initial_continuation.as_mut().unwrap();
    continuation.continuation_id = "continuation-2".into();
    continuation.delegation_id = "delegation-2".into();
    continuation.action_id = "action-2".into();
    continuation.causation_id = "cause-foundation-2".into();
    command.outbox_event.event_id = "event-foundation-2".into();
    command.outbox_event.aggregate_id = "delegation-2".into();
    command.outbox_event.correlation_id = "corr-foundation-2".into();
    command.outbox_event.causation_id = "cause-foundation-2".into();
    command.outbox_event.duplicate_identity = "idem-foundation-2".into();
    command
}

#[test]
fn ea112_multiple_delegations_keep_independent_exact_chains() {
    assert_ea112_case_registered("multiple_delegations");
    let receipt = ReceiptFixture::new();
    receipt
        .fixture
        .store
        .create_foundation(&second_foundation())
        .expect("create second delegation foundation");
    let connection = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
    connection.execute(
        "INSERT INTO execass_authority_links(link_id,delegation_id,link_revision,delegation_state_revision,correlation_id,causation_id,outbox_event_id,authority_kind,session_id,authoritative_revision,linked_at) VALUES('receipt-session-link-2','delegation-2',1,1,'corr-foundation-2','cause-foundation-2','event-foundation-2','session','receipt-session',0,1800000000000)",
        [],
    ).unwrap();
    receipt.append_next("d1-one");

    let mut d2_first = receipt.command("d2-one");
    let (global_count, global_head): (i64, Option<String>) = connection.query_row(
        "SELECT receipt_count,receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    d2_first.delegation_id = "delegation-2".into();
    d2_first.expected_global_count = global_count;
    d2_first.expected_global_head_digest = global_head;
    d2_first.causation_id = "cause-foundation-2".into();
    d2_first.causation_event_id = "event-foundation-2".into();
    d2_first.subject.subject_id = "event-foundation-2".into();
    d2_first.actor.authority_provenance_id = "authority-2".into();
    d2_first.evidence[0].authority_link_id = "receipt-session-link-2".into();
    let AppendReceiptOutcome::Appended(d2_first_record) = receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &d2_first)
        .unwrap()
    else {
        panic!("first delegation-2 receipt was not appended")
    };
    receipt.append_next("d1-two");

    connection.execute("INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES('event-d2-two','execass.v1.runtime_host.changed','delegation-2',1,'corr-d2-two','cause-d2-two',1800000000300,'v1','{}','duplicate-d2-two')", []).unwrap();
    let (global_count, global_head): (i64, Option<String>) = connection.query_row(
        "SELECT receipt_count,receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    let mut d2_second = receipt.command("d2-two");
    d2_second.delegation_id = "delegation-2".into();
    d2_second.expected_global_count = global_count;
    d2_second.expected_global_head_digest = global_head;
    d2_second.expected_delegation_count = 1;
    d2_second.expected_delegation_head_digest = Some(d2_first_record.receipt_digest);
    d2_second.causation_id = "cause-d2-two".into();
    d2_second.causation_event_id = "event-d2-two".into();
    d2_second.subject.subject_id = "event-d2-two".into();
    d2_second.actor.authority_provenance_id = "authority-2".into();
    d2_second.evidence[0].authority_link_id = "receipt-session-link-2".into();
    d2_second.occurred_at = 1_800_000_000_300;
    d2_second.committed_at = 1_800_000_000_310;
    receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &d2_second)
        .unwrap();
    assert!(matches!(
        receipt.integrity.status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 4,
            ..
        }
    ));

    let adversary = adversarial_connection(&receipt.fixture.paths.db_path);
    adversary.execute("UPDATE execass_receipts SET previous_receipt_digest=?1 WHERE receipt_id='receipt-d2-two'", ["0".repeat(64)]).unwrap();
    drop(adversary);
    let failure = receipt
        .integrity
        .receipt_history_failure()
        .unwrap()
        .unwrap();
    assert_eq!(failure.reason, "receipt_delegation_chain_mismatch");
    assert_eq!(failure.first_global_sequence, Some(4));
    assert_eq!(failure.first_receipt_id.as_deref(), Some("receipt-d2-two"));
    assert_eq!(failure.first_delegation_id.as_deref(), Some("delegation-2"));
    assert_eq!(failure.first_delegation_sequence, Some(2));
}

#[test]
fn append_is_atomic_canonical_globally_chained_and_exactly_replayable() {
    let receipt = ReceiptFixture::new();
    let command = receipt.command("one");
    let AppendReceiptOutcome::Appended(record) = receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &command)
        .expect("append receipt")
    else {
        panic!("first receipt was not appended")
    };
    assert_eq!(record.global_sequence, 1);
    assert_eq!(record.delegation_sequence, Some(1));
    assert_eq!(
        receipt.heads(),
        (
            1,
            Some(record.receipt_digest.clone()),
            1,
            Some(record.receipt_digest.clone())
        )
    );
    assert_eq!(
        record.canonical_payload,
        super::canonical::parse_strict_json(
            std::str::from_utf8(&record.canonical_payload).unwrap()
        )
        .unwrap()
        .to_bytes()
    );
    let evidence: (String, String, i64, String, String) = Connection::open(&receipt.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT authority_kind,source_id,authoritative_revision,deep_link,observation_digest FROM execass_receipt_evidence_refs WHERE receipt_id=?1",
            params![record.receipt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        evidence,
        (
            "session".into(),
            "receipt-session".into(),
            0,
            "carsinos://evidence/v1/session/receipt-session?revision=0".into(),
            evidence.4.clone(),
        )
    );
    assert_eq!(evidence.4.len(), 64);
    assert_ne!(evidence.4, "a".repeat(64));
    assert!(String::from_utf8(record.canonical_payload.clone())
        .unwrap()
        .contains(&evidence.4));
    let receipt_columns = Connection::open(&receipt.fixture.paths.db_path)
        .unwrap()
        .prepare("PRAGMA table_info(execass_receipts)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(!receipt_columns
        .iter()
        .any(|column| column == "evidence_refs_json"));
    assert!(matches!(
        receipt
            .fixture
            .store
            .append_receipt(&receipt.integrity, &receipt.redactor, &command)
            .unwrap(),
        AppendReceiptOutcome::Replayed(replayed) if replayed == record
    ));
    let mut conflict = command;
    conflict.redacted_summary = SafeText::summary("different", &[]).unwrap();
    assert!(matches!(
        receipt
            .fixture
            .store
            .append_receipt(&receipt.integrity, &receipt.redactor, &conflict)
            .unwrap(),
        AppendReceiptOutcome::Conflict { .. }
    ));

    for field in [
        "receipt_id",
        "transaction_id",
        "receipt_kind",
        "state_root_generation",
        "committed_at",
        "evidence_set",
        "key",
        "global_expectation",
        "delegation_expectation",
    ] {
        let mut candidate = receipt.command("one");
        match field {
            "receipt_id" => candidate.receipt_id = "different-receipt-id".into(),
            "transaction_id" => candidate.transaction_id = "different-transaction-id".into(),
            "receipt_kind" => candidate.receipt_kind = ReceiptKind::Plan,
            "state_root_generation" => candidate.state_root_generation = 2,
            "committed_at" => candidate.committed_at += 1,
            "evidence_set" => candidate.evidence.clear(),
            "key" => {
                candidate.key = ReceiptKeyRef {
                    key_id: "different-key".into(),
                    key_generation: 7,
                }
            }
            "global_expectation" => {
                candidate.expected_global_count = 1;
                candidate.expected_global_head_digest = Some("b".repeat(64));
            }
            "delegation_expectation" => {
                candidate.expected_delegation_count = 1;
                candidate.expected_delegation_head_digest = Some("b".repeat(64));
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                receipt
                    .fixture
                    .store
                    .append_receipt(&receipt.integrity, &receipt.redactor, &candidate)
                    .unwrap(),
                AppendReceiptOutcome::Conflict { .. }
            ),
            "canonical replay ignored {field}"
        );
    }
}

#[test]
fn stale_heads_and_wrong_actor_runtime_subject_or_evidence_fail_closed() {
    for case in ["stale", "actor", "runtime", "subject", "evidence"] {
        let receipt = ReceiptFixture::new();
        let mut command = receipt.command(case);
        match case {
            "stale" => {
                command.expected_global_count = 1;
                command.expected_global_head_digest = Some("b".repeat(64));
            }
            "actor" => command.actor.authority_provenance_id = "missing".into(),
            "runtime" => command.runtime.fencing_token = 2,
            "subject" => command.subject.revision = 2,
            "evidence" => command.evidence[0].source_id = "other-session".into(),
            _ => unreachable!(),
        }
        let result =
            receipt
                .fixture
                .store
                .append_receipt(&receipt.integrity, &receipt.redactor, &command);
        if case == "stale" {
            assert!(matches!(
                result.unwrap(),
                AppendReceiptOutcome::Stale { .. }
            ));
        } else {
            assert!(result.is_err(), "{case} binding was accepted");
        }
        assert_eq!(receipt.heads(), (0, None, 0, None));
    }
}

#[test]
fn canonical_receipt_evidence_and_global_head_are_sql_immutable() {
    let receipt = ReceiptFixture::new();
    let command = receipt.command("immutable");
    receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &command)
        .unwrap();
    let connection = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
    for statement in [
        "UPDATE execass_receipts SET redacted_summary='changed' WHERE receipt_id='receipt-immutable'",
        "UPDATE execass_receipt_evidence_refs SET source_id='changed' WHERE receipt_id='receipt-immutable'",
        "DELETE FROM execass_receipt_evidence_refs WHERE receipt_id='receipt-immutable'",
        "UPDATE execass_receipt_journal_state SET receipt_count=3 WHERE singleton=1",
    ] {
        assert!(connection.execute(statement, []).is_err(), "accepted {statement}");
    }
    assert_eq!(receipt.heads().0, 1);
}

#[test]
fn runtime_canary_is_redacted_before_receipt_database_wal_anchor_or_error_persistence() {
    let random = uuid::Uuid::new_v4().simple().to_string();
    let canary = format!("ea111-{} /?+", &random[..12]);
    let variants = scanner_variant_bytes(&canary);
    assert_eq!(variants.len(), 23);
    let redactor = ReceiptRedactor::new(&[&canary]).unwrap();

    let bypass = ReceiptFixture::new();
    let mut bypass_command = bypass.command("canary-bypass");
    bypass_command.redacted_summary = SafeText::summary(&canary, &[]).unwrap();
    let error = bypass
        .fixture
        .store
        .append_receipt(&bypass.integrity, &redactor, &bypass_command)
        .unwrap_err()
        .to_string();
    assert!(!error.contains(&canary));
    assert_eq!(bypass.heads(), (0, None, 0, None));

    let receipt = ReceiptFixture::new();
    let wal_guard = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
    wal_guard
        .pragma_update(None, "journal_mode", "WAL")
        .expect("force the real receipt database through WAL storage");
    wal_guard
        .pragma_update(None, "wal_autocheckpoint", 0)
        .expect("retain WAL bytes until the canary scan completes");
    let journal_mode: String = wal_guard
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("read forced receipt journal mode");
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    wal_guard
        .execute_batch("BEGIN;")
        .expect("hold a pre-append WAL reader snapshot");
    let _: i64 = wal_guard
        .query_row("SELECT COUNT(*) FROM execass_receipts", [], |row| {
            row.get(0)
        })
        .expect("pin the pre-append receipt snapshot");
    let mut command = receipt.command("canary-clean");
    let encoded_text = variants
        .iter()
        .filter_map(|variant| std::str::from_utf8(variant).ok())
        .collect::<Vec<_>>()
        .join("|");
    command.redacted_summary = redactor.summary(&encoded_text).unwrap();
    receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &redactor, &command)
        .unwrap();

    let canonical_payload: Vec<u8> = open_sqlite_connection(&receipt.fixture.paths.db_path)
        .unwrap()
        .query_row(
            "SELECT canonical_payload FROM execass_receipts WHERE receipt_id=?1",
            params![command.receipt_id],
            |row| row.get(0),
        )
        .unwrap();
    std::fs::create_dir_all(&receipt.fixture.paths.logs_dir).unwrap();
    let receipt_log = receipt.fixture.paths.logs_dir.join("receipt-safe.log");
    std::fs::write(&receipt_log, &canonical_payload).unwrap();
    let export_dir = receipt.fixture.paths.root.join("exports");
    std::fs::create_dir_all(&export_dir).unwrap();
    let receipt_export = export_dir.join("receipt-safe.json");
    std::fs::write(&receipt_export, &canonical_payload).unwrap();

    let wal_path = PathBuf::from(format!("{}-wal", receipt.fixture.paths.db_path.display()));
    let shm_path = PathBuf::from(format!("{}-shm", receipt.fixture.paths.db_path.display()));
    assert!(wal_path.is_file(), "forced receipt WAL did not materialize");
    assert!(shm_path.is_file(), "forced receipt SHM did not materialize");

    let mut surfaces = vec![
        receipt.fixture.paths.db_path.clone(),
        wal_path,
        shm_path,
        receipt
            .fixture
            .paths
            .db_path
            .with_extension("receipt-append.lock"),
        receipt_log,
        receipt_export,
    ];
    surfaces.extend(files_below(&receipt.anchor_dir));
    for path in surfaces.into_iter().filter(|path| path.is_file()) {
        let bytes = std::fs::read(&path).unwrap();
        for (index, variant) in variants.iter().enumerate() {
            assert!(
                !contains_bytes(&bytes, variant),
                "canary variant {index} survived on a receipt persistence surface"
            );
        }
        assert!(!path.to_string_lossy().contains(&canary));
    }
}

#[test]
fn precommit_failpoints_roll_back_receipt_heads_and_prepared_anchor() {
    for failpoint in [
        "receipt.after_insert",
        "receipt.after_evidence",
        "receipt.after_global_head",
        "receipt.after_delegation_head",
        "receipt.after_confirm",
    ] {
        let receipt = ReceiptFixture::new();
        let command = receipt.command(failpoint.rsplit('.').next().unwrap());
        assert!(receipt
            .fixture
            .store
            .append_receipt_with_failpoints(
                &receipt.integrity,
                &receipt.redactor,
                &command,
                Arc::new(FailOnce::new(failpoint)),
            )
            .is_err());
        assert_eq!(receipt.heads(), (0, None, 0, None), "{failpoint}");
        assert!(matches!(
            receipt.integrity.status().unwrap(),
            IntegrityStatus::Uninitialized
        ));
    }
}

#[test]
fn prepared_only_and_committed_only_interruptions_recover_to_proven_state() {
    let prepared = ReceiptFixture::new();
    assert!(prepared
        .fixture
        .store
        .append_receipt_with_failpoints(
            &prepared.integrity,
            &prepared.redactor,
            &prepared.command("prepared-only"),
            Arc::new(FailOnce::new("receipt.after_prepare")),
        )
        .is_err());
    assert_eq!(prepared.heads(), (0, None, 0, None));
    assert!(matches!(
        prepared.integrity.recover_integrity().unwrap(),
        IntegrityRecovery::RestoredLastProvenPair {
            anchor_generation: None
        }
    ));

    let committed = ReceiptFixture::new();
    assert!(committed
        .fixture
        .store
        .append_receipt_with_failpoints(
            &committed.integrity,
            &committed.redactor,
            &committed.command("committed-only"),
            Arc::new(FailOnce::new("receipt.after_commit")),
        )
        .is_err());
    assert_eq!(committed.heads().0, 1);
    assert!(matches!(
        committed.integrity.status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 1,
            ..
        }
    ));
}

#[test]
fn committed_before_finalize_is_recovered_without_duplicate_receipt() {
    let receipt = ReceiptFixture::new();
    let command = receipt.command("interrupted");
    assert!(receipt
        .fixture
        .store
        .append_receipt_with_failpoints(
            &receipt.integrity,
            &receipt.redactor,
            &command,
            Arc::new(FailOnce::new("receipt.before_finalize")),
        )
        .is_err());
    assert_eq!(receipt.heads().0, 1);
    assert!(matches!(
        receipt.integrity.status().unwrap(),
        IntegrityStatus::Prepared { .. }
    ));
    assert!(matches!(
        receipt.integrity.recover_integrity().unwrap(),
        IntegrityRecovery::FinalizedInterruptedCommit { .. }
    ));
    assert!(matches!(
        receipt
            .fixture
            .store
            .append_receipt(&receipt.integrity, &receipt.redactor, &command)
            .unwrap(),
        AppendReceiptOutcome::Replayed(_)
    ));
    assert_eq!(receipt.heads().0, 1);
    assert!(receipt.anchor_dir.join("current.json").is_file());
}

#[test]
fn concurrent_identical_append_has_one_row_and_no_chain_fork() {
    let receipt = ReceiptFixture::new();
    let command = receipt.command("concurrent");
    let barrier = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let store = receipt.fixture.store.clone();
        let integrity = receipt.integrity.clone();
        let redactor = receipt.redactor.clone();
        let command = command.clone();
        let barrier = barrier.clone();
        workers.push(thread::spawn(move || {
            barrier.wait();
            store.append_receipt(&integrity, &redactor, &command)
        }));
    }
    let results = workers
        .into_iter()
        .map(|worker| worker.join().expect("receipt worker"))
        .collect::<Vec<_>>();
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(AppendReceiptOutcome::Appended(_))))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(AppendReceiptOutcome::Replayed(_))))
            .count(),
        1
    );
    assert_eq!(receipt.heads().0, 1);
    let row_count: i64 = Connection::open(&receipt.fixture.paths.db_path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM execass_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(row_count, 1);
}

#[test]
fn cross_process_append_worker() {
    let Ok(root) = std::env::var("CARSINOS_RECEIPT_RACE_ROOT") else {
        return;
    };
    let anchor_dir =
        PathBuf::from(std::env::var("CARSINOS_RECEIPT_RACE_ANCHOR").expect("race anchor path"));
    let coordination_dir = PathBuf::from(
        std::env::var("CARSINOS_RECEIPT_RACE_COORDINATION").expect("race coordination path"),
    );
    let worker = std::env::var("CARSINOS_RECEIPT_RACE_WORKER").expect("race worker id");
    let paths = crate::AppPaths::from_root(root);
    let store = ExecAssStore::open(&paths).expect("open cross-process receipt store");
    let integrity = ReceiptIntegrityStore::with_protector(
        &paths,
        anchor_dir,
        Arc::new(TestProtector::default()),
    )
    .expect("open cross-process integrity store");
    std::fs::write(coordination_dir.join(format!("ready-{worker}")), b"ready")
        .expect("publish race readiness");
    let start = coordination_dir.join("start");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !start.is_file() {
        assert!(
            Instant::now() < deadline,
            "cross-process race start timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let outcome = store
        .append_receipt(
            &integrity,
            &ReceiptRedactor::new(&["fixture-secret-not-for-persistence"]).unwrap(),
            &receipt_command(
                ReceiptKeyRef {
                    key_id: "receipt-key-1".into(),
                    key_generation: 1,
                },
                "cross-process",
            ),
        )
        .expect("cross-process append result");
    assert!(matches!(
        outcome,
        AppendReceiptOutcome::Appended(_) | AppendReceiptOutcome::Replayed(_)
    ));
}

#[test]
fn separately_opened_processes_serialize_to_one_exact_receipt() {
    let receipt = ReceiptFixture::new();
    let coordination_dir = receipt
        .fixture
        .paths
        .root
        .parent()
        .unwrap()
        .join("receipt-race");
    std::fs::create_dir_all(&coordination_dir).unwrap();
    let executable = std::env::current_exe().expect("current test executable");
    let mut children = Vec::new();
    for worker in ["one", "two"] {
        let mut worker_root = receipt.fixture.paths.root.clone();
        #[cfg(windows)]
        if worker == "two" {
            let raw = worker_root.to_string_lossy();
            worker_root = PathBuf::from(raw.replacen("Z:", "z:", 1));
            assert_ne!(
                worker_root.to_string_lossy(),
                receipt.fixture.paths.root.to_string_lossy()
            );
        }
        children.push(
            Command::new(&executable)
                .args([
                    "--exact",
                    "execass::receipt_tests::cross_process_append_worker",
                    "--nocapture",
                ])
                .env("CARSINOS_RECEIPT_RACE_ROOT", worker_root)
                .env("CARSINOS_RECEIPT_RACE_ANCHOR", &receipt.anchor_dir)
                .env("CARSINOS_RECEIPT_RACE_COORDINATION", &coordination_dir)
                .env("CARSINOS_RECEIPT_RACE_WORKER", worker)
                .spawn()
                .expect("spawn receipt race worker"),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while ["one", "two"]
        .iter()
        .any(|worker| !coordination_dir.join(format!("ready-{worker}")).is_file())
    {
        assert!(
            Instant::now() < deadline,
            "receipt race readiness timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(coordination_dir.join("start"), b"start").unwrap();
    for child in children {
        let output = child
            .wait_with_output()
            .expect("wait for receipt race worker");
        assert!(
            output.status.success(),
            "cross-process receipt worker failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(receipt.heads().0, 1);
    let connection = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
    let rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM execass_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 1);
    assert!(matches!(
        receipt.integrity.status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 1,
            ..
        }
    ));
}

#[test]
fn key_rotation_receipt_is_cross_signed_and_requires_exact_registry_parent() {
    let receipt = ReceiptFixture::new();
    let first = receipt.command("before-rotation");
    let AppendReceiptOutcome::Appended(first_record) = receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &first)
        .unwrap()
    else {
        panic!()
    };
    let new_key = receipt.integrity.rotate_key("receipt-key-2").unwrap();
    let mut rotation = receipt.command("rotation");
    rotation.receipt_id = "receipt-rotation".into();
    rotation.transaction_id = "receipt-tx-rotation".into();
    rotation.expected_global_count = 1;
    rotation.expected_global_head_digest = Some(first_record.receipt_digest.clone());
    rotation.expected_delegation_count = 1;
    rotation.expected_delegation_head_digest = Some(first_record.receipt_digest);
    rotation.receipt_kind = ReceiptKind::KeyRotation;
    rotation.key = new_key;
    rotation.rotation = Some(ReceiptRotation {
        transition_id: "key-transition-2".into(),
        reason: SafeText::summary("scheduled rotation", &[]).unwrap(),
        previous_key: receipt.key.clone(),
    });
    // A receipt has a one-to-one event identity. Rotation therefore needs a
    // distinct exact outbox event at the same delegation revision.
    let connection = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
    connection.execute(
        "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES('event-rotation','execass.v1.runtime_host.changed','delegation-1',1,'corr-rotation','cause-rotation',1800000000100,'v1','{}','rotation-event-identity')",
        [],
    ).unwrap();
    rotation.causation_event_id = "event-rotation".into();
    rotation.causation_id = "cause-rotation".into();
    rotation.occurred_at = 1_800_000_000_100;
    rotation.committed_at = 1_800_000_000_110;
    let mut wrong_parent = rotation.clone();
    wrong_parent.rotation.as_mut().unwrap().previous_key = ReceiptKeyRef {
        key_id: "wrong-parent".into(),
        key_generation: 1,
    };
    assert!(receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &wrong_parent)
        .is_err());
    let AppendReceiptOutcome::Appended(rotated) = receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &rotation)
        .unwrap()
    else {
        panic!()
    };
    assert!(rotated.previous_key_integrity_tag.is_some());
    assert_eq!(rotated.global_sequence, 2);
    assert_eq!(rotated.delegation_sequence, Some(2));
}

fn append_valid_key_rotation(receipt: &ReceiptFixture) -> ReceiptRecord {
    let first = receipt.append_next("before-hostile-rotation");
    let new_key = receipt
        .integrity
        .rotate_key("receipt-key-2")
        .expect("provision rotated receipt key");
    let connection = open_sqlite_connection(&receipt.fixture.paths.db_path).unwrap();
    connection.execute(
        "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES('event-hostile-rotation','execass.v1.runtime_host.changed','delegation-1',1,'corr-hostile-rotation','cause-hostile-rotation',1800000000100,'v1','{}','hostile-rotation-event-identity')",
        [],
    ).unwrap();
    drop(connection);
    let mut command = receipt.command("hostile-rotation");
    command.receipt_id = "receipt-hostile-rotation".into();
    command.transaction_id = "receipt-tx-hostile-rotation".into();
    command.expected_global_count = 1;
    command.expected_global_head_digest = Some(first.receipt_digest.clone());
    command.expected_delegation_count = 1;
    command.expected_delegation_head_digest = Some(first.receipt_digest);
    command.receipt_kind = ReceiptKind::KeyRotation;
    command.key = new_key;
    command.rotation = Some(ReceiptRotation {
        transition_id: "hostile-key-transition-2".into(),
        reason: SafeText::summary("scheduled hostile rotation proof", &[]).unwrap(),
        previous_key: receipt.key.clone(),
    });
    command.causation_event_id = "event-hostile-rotation".into();
    command.causation_id = "cause-hostile-rotation".into();
    command.subject.subject_id = "event-hostile-rotation".into();
    command.occurred_at = 1_800_000_000_100;
    command.committed_at = 1_800_000_000_110;
    match receipt
        .fixture
        .store
        .append_receipt(&receipt.integrity, &receipt.redactor, &command)
        .unwrap()
    {
        AppendReceiptOutcome::Appended(record) => record,
        other => panic!("expected rotated receipt append, got {other:?}"),
    }
}

#[test]
fn ea112_rotated_receipt_reopens_trusted_then_exact_tamper_quarantines() {
    for name in [
        "rotated_receipt_tag_tamper",
        "rotated_receipt_history_tamper",
        "rotated_key_registry_transition_tamper",
        "rotated_key_registry_status_tamper",
        "rotated_key_registry_activation_tamper",
    ] {
        assert_ea112_case_registered(name);
        assert_ea112_case_registered("positive_key_rotation_reopen");
        let receipt = ReceiptFixture::new();
        let rotated = append_valid_key_rotation(&receipt);
        assert!(rotated.previous_key_integrity_tag.is_some());
        let reopened = receipt.reopened_integrity();
        assert!(matches!(
            reopened.status().unwrap(),
            IntegrityStatus::Trusted {
                receipt_count: 2,
                key: ReceiptKeyRef {
                    key_generation: 2,
                    ..
                },
                ..
            }
        ));

        let connection = adversarial_connection(&receipt.fixture.paths.db_path);
        let expected_reason = match name {
            "rotated_receipt_tag_tamper" => {
                assert_eq!(
                    connection
                        .execute(
                            "UPDATE execass_receipts SET keyed_integrity_tag=?1 WHERE receipt_id='receipt-hostile-rotation'",
                            ["0".repeat(64)],
                        )
                        .unwrap(),
                    1
                );
                "receipt_keyed_tag_mismatch"
            }
            "rotated_receipt_history_tamper" => {
                assert_eq!(
                    connection
                        .execute(
                            "UPDATE execass_receipts SET previous_key_integrity_tag=?1 WHERE receipt_id='receipt-hostile-rotation'",
                            ["0".repeat(64)],
                        )
                        .unwrap(),
                    1
                );
                "receipt_rotation_cross_signature_mismatch"
            }
            "rotated_key_registry_transition_tamper" => {
                assert_eq!(
                    connection
                        .execute(
                            "UPDATE execass_receipt_keys SET rotated_from_key_id='attacker-parent' WHERE key_generation=2",
                            [],
                        )
                        .unwrap(),
                    1
                );
                "receipt_key_generation_mismatch"
            }
            "rotated_key_registry_status_tamper" => {
                assert_eq!(
                    connection
                        .execute(
                            "UPDATE execass_receipt_keys SET status='lost' WHERE key_generation=2",
                            [],
                        )
                        .unwrap(),
                    1
                );
                "receipt_key_registry_state_mismatch"
            }
            "rotated_key_registry_activation_tamper" => {
                assert_eq!(
                    connection
                        .execute(
                            "UPDATE execass_receipt_keys SET activated_anchor_generation=999 WHERE key_generation=2",
                            [],
                        )
                        .unwrap(),
                    1
                );
                "receipt_key_registry_state_mismatch"
            }
            _ => unreachable!(),
        };
        drop(connection);

        let failure = reopened
            .receipt_history_failure()
            .unwrap()
            .expect("rotated receipt tamper must be diagnosed");
        assert_eq!(failure.reason, expected_reason, "{name}");
        assert_eq!(failure.first_global_sequence, Some(2), "{name}");
        assert_eq!(
            failure.first_receipt_id.as_deref(),
            Some("receipt-hostile-rotation"),
            "{name}"
        );
        let reason = format!("{expected_reason}_at_global_sequence_2");
        assert_eq!(
            reopened.status().unwrap(),
            IntegrityStatus::Mismatch {
                reason: reason.clone()
            }
        );
        assert_eq!(
            reopened.recover_integrity().unwrap(),
            IntegrityRecovery::Quarantined {
                reason: reason.clone()
            }
        );
        assert_eq!(
            reopened.status().unwrap(),
            IntegrityStatus::Quarantined { reason }
        );
        assert!(receipt
            .fixture
            .store
            .append_receipt(
                &reopened,
                &receipt.redactor,
                &receipt.command("blocked-after-rotated-receipt-tamper"),
            )
            .is_err());
    }
}

#[test]
fn ea112_pending_key_rotation_is_valid_but_orphan_registry_row_quarantines() {
    assert_ea112_case_registered("pending_key_rotation_registry_valid");
    assert_ea112_case_registered("orphan_registry_key_tamper");
    let receipt = ReceiptFixture::new();
    append_valid_key_rotation(&receipt);
    let pending = receipt.integrity.rotate_key("pending-key-three").unwrap();
    assert_eq!(pending.key_generation, 3);
    assert!(matches!(
        receipt.reopened_integrity().status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 2,
            key: ReceiptKeyRef {
                key_generation: 2,
                ..
            },
            ..
        }
    ));

    let connection = adversarial_connection(&receipt.fixture.paths.db_path);
    assert_eq!(
        connection
            .execute(
                "UPDATE execass_receipt_keys SET rotated_from_key_id=?1,rotated_from_key_generation=1 WHERE key_generation=3",
                [&receipt.key.key_id],
            )
            .unwrap(),
        1
    );
    drop(connection);

    let reopened = receipt.reopened_integrity();
    let failure = reopened
        .receipt_history_failure()
        .unwrap()
        .expect("orphan registry tamper must be diagnosed");
    assert_eq!(failure.reason, "receipt_key_registry_state_mismatch");
    assert_eq!(failure.first_global_sequence, None);
    assert_eq!(
        reopened.status().unwrap(),
        IntegrityStatus::Mismatch {
            reason: "receipt_key_registry_state_mismatch".into()
        }
    );
    assert_eq!(
        reopened.recover_integrity().unwrap(),
        IntegrityRecovery::Quarantined {
            reason: "receipt_key_registry_state_mismatch".into()
        }
    );
    assert!(receipt
        .fixture
        .store
        .append_receipt(
            &reopened,
            &receipt.redactor,
            &receipt.command("blocked-after-orphan-key-registry-tamper"),
        )
        .is_err());
}

#[test]
fn ea112_registry_immutable_identity_and_pending_material_tamper_quarantine() {
    for case_name in [
        "registry_created_at_tamper",
        "registry_integrity_tag_tamper",
        "pending_key_id_tamper",
        "pending_key_material_loss",
    ] {
        assert_ea112_case_registered(case_name);
        let receipt = ReceiptFixture::new();
        append_valid_key_rotation(&receipt);
        let pending = receipt.integrity.rotate_key("pending-key-three").unwrap();
        assert!(matches!(
            receipt.reopened_integrity().status().unwrap(),
            IntegrityStatus::Trusted {
                receipt_count: 2,
                ..
            }
        ));

        if case_name == "pending_key_material_loss" {
            receipt.protector.keys.lock().expect("key lock").insert(
                (pending.key_id.clone(), pending.key_generation),
                vec![0_u8; 7],
            );
        } else {
            let connection = adversarial_connection(&receipt.fixture.paths.db_path);
            let sql = match case_name {
                "registry_created_at_tamper" => {
                    "UPDATE execass_receipt_keys SET created_at=created_at+1 WHERE key_generation=1"
                }
                "registry_integrity_tag_tamper" => {
                    "UPDATE execass_receipt_keys SET registry_integrity_tag=CASE substr(registry_integrity_tag,1,1) WHEN '0' THEN '1'||substr(registry_integrity_tag,2) ELSE '0'||substr(registry_integrity_tag,2) END WHERE key_generation=1"
                }
                "pending_key_id_tamper" => {
                    "UPDATE execass_receipt_keys SET key_id='rewritten-pending-key' WHERE key_generation=3"
                }
                _ => unreachable!(),
            };
            assert_eq!(connection.execute(sql, []).unwrap(), 1, "{case_name}");
        }

        let reopened = receipt.reopened_integrity();
        let failure = reopened
            .receipt_history_failure()
            .unwrap()
            .expect("registry identity/material tamper must be diagnosed");
        assert_eq!(
            failure.reason, "receipt_key_registry_state_mismatch",
            "{case_name}"
        );
        assert_eq!(failure.first_global_sequence, None, "{case_name}");
        assert_eq!(
            reopened.status().unwrap(),
            IntegrityStatus::Mismatch {
                reason: "receipt_key_registry_state_mismatch".into()
            },
            "{case_name}"
        );
        assert_eq!(
            reopened.recover_integrity().unwrap(),
            IntegrityRecovery::Quarantined {
                reason: "receipt_key_registry_state_mismatch".into()
            },
            "{case_name}"
        );
        assert!(receipt
            .fixture
            .store
            .append_receipt(
                &reopened,
                &receipt.redactor,
                &receipt.command("blocked-after-registry-identity-tamper"),
            )
            .is_err());
    }
}

fn files_below(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(files_below(&path));
        } else {
            files.push(path);
        }
    }
    files
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn scanner_variant_bytes(secret: &str) -> Vec<Vec<u8>> {
    let raw = secret.as_bytes().to_vec();
    let utf16le = secret
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let utf16be = secret
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect::<Vec<_>>();
    let utf16le_bom = [vec![0xff, 0xfe], utf16le.clone()].concat();
    let utf16be_bom = [vec![0xfe, 0xff], utf16be.clone()].concat();
    let percent_upper = quote_bytes(&raw, true, false).into_bytes();
    vec![
        raw.clone(),
        utf16le.clone(),
        utf16le_bom.clone(),
        utf16be.clone(),
        utf16be_bom.clone(),
        base64::engine::general_purpose::STANDARD
            .encode(&raw)
            .into_bytes(),
        base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(&raw)
            .into_bytes(),
        base64::engine::general_purpose::URL_SAFE
            .encode(&raw)
            .into_bytes(),
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(&raw)
            .into_bytes(),
        hex_bytes(&raw, false).into_bytes(),
        hex_bytes(&raw, true).into_bytes(),
        percent_upper.clone(),
        quote_bytes(&raw, false, false).into_bytes(),
        quote_bytes(&raw, true, true).into_bytes(),
        quote_bytes(&percent_upper, true, false).into_bytes(),
        base64::engine::general_purpose::STANDARD
            .encode(&utf16le)
            .into_bytes(),
        hex_bytes(&utf16le, false).into_bytes(),
        base64::engine::general_purpose::STANDARD
            .encode(&utf16le_bom)
            .into_bytes(),
        hex_bytes(&utf16le_bom, false).into_bytes(),
        base64::engine::general_purpose::STANDARD
            .encode(&utf16be)
            .into_bytes(),
        hex_bytes(&utf16be, false).into_bytes(),
        base64::engine::general_purpose::STANDARD
            .encode(&utf16be_bom)
            .into_bytes(),
        hex_bytes(&utf16be_bom, false).into_bytes(),
    ]
}

fn quote_bytes(bytes: &[u8], upper: bool, plus: bool) -> String {
    bytes
        .iter()
        .map(|byte| match *byte {
            b' ' if plus => "+".to_owned(),
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') => {
                (byte as char).to_string()
            }
            byte if upper => format!("%{byte:02X}"),
            byte => format!("%{byte:02x}"),
        })
        .collect()
}

fn hex_bytes(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|byte| {
            if upper {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            }
        })
        .collect()
}
