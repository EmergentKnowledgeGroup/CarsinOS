use super::receipt_integrity::{
    read_exact_document, receipt_key_count_with_conn, serialize_document, sha256_hex,
    verify_anchor_document_integrity, AnchorCommitInput, IntegrityFailpoints, ReceiptKeyProtector,
};
use super::*;
use crate::{init_execass_fresh_root, open_sqlite_connection, AppPaths};
use anyhow::{bail, Result};
use rusqlite::{OptionalExtension, TransactionBehavior};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tempfile::TempDir;
use zeroize::Zeroizing;

fn register_ea112(name: &str) {
    super::receipt_tests::assert_ea112_case_registered(name);
}

#[derive(Default)]
struct TestProtector {
    keys: Mutex<BTreeMap<(String, i64), Vec<u8>>>,
}

impl ReceiptKeyProtector for TestProtector {
    fn create(&self, key: &ReceiptKeyRef) -> Result<Zeroizing<Vec<u8>>> {
        let mut keys = self.keys.lock().expect("key map lock");
        let identity = (key.key_id.clone(), key.key_generation);
        if keys.contains_key(&identity) {
            bail!("test key already exists");
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
            .ok_or_else(|| anyhow::anyhow!("test receipt key not found"))
    }

    fn delete(&self, key: &ReceiptKeyRef) -> Result<()> {
        self.keys
            .lock()
            .expect("key map lock")
            .remove(&(key.key_id.clone(), key.key_generation));
        Ok(())
    }
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

impl IntegrityFailpoints for FailOnce {
    fn hit(&self, name: &'static str) -> Result<()> {
        if name == self.name && !self.fired.swap(true, Ordering::SeqCst) {
            bail!("injected receipt-integrity failpoint: {name}");
        }
        Ok(())
    }
}

struct Fixture {
    _temp: TempDir,
    paths: AppPaths,
    anchor_dir: PathBuf,
    protector: Arc<TestProtector>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir_in("Z:\\carsinos").expect("project-drive tempdir");
        let paths = AppPaths::from_root(temp.path().join("state"));
        init_execass_fresh_root(&paths).expect("initialize ExecAss root");
        let anchor_dir = temp.path().join("external-anchor");
        let protector = Arc::new(TestProtector::default());
        Self {
            _temp: temp,
            paths,
            anchor_dir,
            protector,
        }
    }

    fn store(&self) -> ReceiptIntegrityStore {
        ReceiptIntegrityStore::with_protector(
            &self.paths,
            self.anchor_dir.clone(),
            self.protector.clone(),
        )
        .expect("open test integrity store")
    }

    fn fail_store(&self, name: &'static str) -> ReceiptIntegrityStore {
        let base = self.store();
        ReceiptIntegrityStore::with_protector_and_failpoints(
            &self.paths,
            self.anchor_dir.clone(),
            base.root_identity().to_owned(),
            self.protector.clone(),
            Arc::new(FailOnce::new(name)),
        )
        .expect("open failpoint integrity store")
    }
}

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn input(key: ReceiptKeyRef, generation: i64, _count: i64) -> AnchorCommitInput {
    AnchorCommitInput {
        state_root_generation: 1,
        anchor_generation: generation,
        receipt_count: 0,
        receipt_head_digest: None,
        key,
        transaction_id: format!("tx-{generation}"),
        external_receipt_digest: digest(if generation == 1 { 'c' } else { 'd' }),
        occurred_at: 1_800_000_000_000 + generation,
    }
}

fn confirm(
    fixture: &Fixture,
    store: &ReceiptIntegrityStore,
    transaction_id: &str,
    _count: i64,
    _head: Option<&str>,
) {
    let mut connection = open_sqlite_connection(&fixture.paths.db_path).expect("open database");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .expect("begin receipt transaction");
    store
        .confirm_prepared_anchor_in_transaction(
            &transaction,
            transaction_id,
            0,
            None,
            1_900_000_000_000,
        )
        .expect("confirm receipt commit");
    transaction.commit().expect("commit receipt transaction");
}

fn first_finalized(fixture: &Fixture) -> (ReceiptIntegrityStore, ReceiptKeyRef) {
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-one")
        .expect("provision initial key");
    store
        .prepare_anchor(&input(key.clone(), 1, 1))
        .expect("prepare initial anchor");
    let head = digest('a');
    confirm(fixture, &store, "tx-1", 1, Some(&head));
    store
        .finalize_anchor("tx-1")
        .expect("finalize initial anchor");
    (store, key)
}

fn second_finalized(fixture: &Fixture, store: &ReceiptIntegrityStore, key: ReceiptKeyRef) {
    store
        .prepare_anchor(&input(key, 2, 2))
        .expect("prepare second anchor");
    confirm(fixture, store, "tx-2", 0, None);
    store
        .finalize_anchor("tx-2")
        .expect("finalize second anchor");
}

#[test]
fn supplied_transaction_prepare_confirms_and_finalizes_one_atomic_anchor() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-atomic")
        .expect("provision atomic receipt key");
    let mut connection =
        open_sqlite_connection(&fixture.paths.db_path).expect("open atomic database");
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .expect("begin atomic receipt transaction");
    let prepared = store
        .prepare_anchor_in_transaction(&transaction, &input(key.clone(), 1, 0))
        .expect("prepare anchor through supplied transaction");
    store
        .confirm_prepared_anchor_in_transaction(
            &transaction,
            &prepared.transaction_id,
            0,
            None,
            1_900_000_000_000,
        )
        .expect("confirm supplied-transaction anchor");
    transaction
        .commit()
        .expect("commit supplied-transaction anchor");
    store
        .finalize_anchor(&prepared.transaction_id)
        .expect("finalize supplied-transaction anchor");

    assert_eq!(
        store.status().expect("read finalized status"),
        IntegrityStatus::Trusted {
            anchor_generation: 1,
            receipt_count: 0,
            receipt_head_digest: None,
            key,
        }
    );
}

#[test]
fn supplied_transaction_rollback_recovers_orphan_prepared_document() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-rollback")
        .expect("provision rollback receipt key");
    {
        let mut connection =
            open_sqlite_connection(&fixture.paths.db_path).expect("open rollback database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin rollback receipt transaction");
        store
            .prepare_anchor_in_transaction(&transaction, &input(key, 1, 0))
            .expect("prepare rollback anchor");
        transaction
            .rollback()
            .expect("roll back prepared anchor row");
    }

    assert_eq!(
        store.status().expect("read rolled-back status"),
        IntegrityStatus::Uninitialized
    );
    assert_eq!(
        store.recover_integrity().expect("recover orphan document"),
        IntegrityRecovery::RestoredLastProvenPair {
            anchor_generation: None,
        }
    );
    assert_eq!(
        store.status().expect("read recovered status"),
        IntegrityStatus::Uninitialized
    );
}

#[test]
fn ea112_external_anchor_and_key_tamper_matrix_has_exact_quarantine_reasons() {
    struct Case {
        name: &'static str,
        expected_reason: &'static str,
    }
    for case in [
        Case {
            name: "external_anchor_rollback",
            expected_reason: "anchor_document_mismatch",
        },
        Case {
            name: "current_document_tamper",
            expected_reason: "anchor_document_mismatch",
        },
        Case {
            name: "key_generation_mismatch",
            expected_reason: "key_generation_mismatch",
        },
        Case {
            name: "unknown_key",
            expected_reason: "receipt_key_unavailable",
        },
        Case {
            name: "missing_key",
            expected_reason: "receipt_key_unavailable",
        },
        Case {
            name: "wrong_key",
            expected_reason: "receipt_commit_confirmation_mismatch",
        },
        Case {
            name: "broken_rotation_cross_signature",
            expected_reason: "anchor_rotation_cross_signature_mismatch",
        },
        Case {
            name: "state_root_generation_mismatch",
            expected_reason: "state_root_generation_mismatch",
        },
    ] {
        register_ea112(case.name);
        let fixture = Fixture::new();
        let (store, key) = first_finalized(&fixture);
        assert!(matches!(
            store.status().unwrap(),
            IntegrityStatus::Trusted { .. }
        ));
        match case.name {
            "external_anchor_rollback" => {
                second_finalized(&fixture, &store, key.clone());
                let first = fs::read(
                    fixture
                        .anchor_dir
                        .join("anchor-00000000000000000001-tx-1.finalized.json"),
                )
                .unwrap();
                fs::write(fixture.anchor_dir.join("current.json"), first).unwrap();
            }
            "current_document_tamper" => {
                let current = fixture.anchor_dir.join("current.json");
                let mut document = read_exact_document(&current).unwrap();
                document.external_receipt_digest = digest('e');
                fs::write(current, serialize_document(&document).unwrap()).unwrap();
            }
            "key_generation_mismatch" => {
                let material = fixture.protector.load(&key).unwrap().to_vec();
                fixture
                    .protector
                    .keys
                    .lock()
                    .unwrap()
                    .insert((key.key_id.clone(), 2), material);
                let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
                connection
                    .set_db_config(
                        rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
                        false,
                    )
                    .unwrap();
                connection
                    .pragma_update(None, "foreign_keys", "OFF")
                    .unwrap();
                connection.execute("UPDATE execass_receipt_anchor_state SET key_generation=2 WHERE transaction_id='tx-1'", []).unwrap();
            }
            "unknown_key" => {
                let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
                connection
                    .set_db_config(
                        rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
                        false,
                    )
                    .unwrap();
                connection
                    .pragma_update(None, "foreign_keys", "OFF")
                    .unwrap();
                connection.execute("UPDATE execass_receipt_anchor_state SET key_id='unknown-key' WHERE transaction_id='tx-1'", []).unwrap();
            }
            "missing_key" => fixture.protector.delete(&key).unwrap(),
            "wrong_key" => {
                fixture
                    .protector
                    .keys
                    .lock()
                    .unwrap()
                    .insert((key.key_id.clone(), key.key_generation), vec![0xabu8; 32]);
            }
            "broken_rotation_cross_signature" => {
                let new_key = store.rotate_key("receipt-key-two").unwrap();
                second_finalized(&fixture, &store, new_key);
                let current = fixture.anchor_dir.join("current.json");
                let mut document = read_exact_document(&current).unwrap();
                document.previous_key_integrity_tag = Some(digest('0'));
                let bytes = serialize_document(&document).unwrap();
                fs::write(&current, &bytes).unwrap();
                fs::write(
                    fixture
                        .anchor_dir
                        .join("anchor-00000000000000000002-tx-2.finalized.json"),
                    bytes,
                )
                .unwrap();
            }
            "state_root_generation_mismatch" => {
                let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
                connection
                    .set_db_config(
                        rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
                        false,
                    )
                    .unwrap();
                connection.execute("UPDATE execass_receipt_anchor_state SET state_root_generation=2 WHERE transaction_id='tx-1'", []).unwrap();
            }
            _ => unreachable!(),
        }
        match store.status().unwrap() {
            IntegrityStatus::Mismatch { reason } => {
                assert_eq!(reason, case.expected_reason, "{}", case.name)
            }
            IntegrityStatus::KeyLost { key: lost } => {
                assert!(matches!(case.name, "unknown_key" | "missing_key"));
                if case.name == "unknown_key" {
                    assert_eq!(lost.key_id, "unknown-key");
                } else {
                    assert_eq!(lost, key);
                }
            }
            other => panic!("{} was not rejected: {other:?}", case.name),
        }
        assert_eq!(
            store.recover_integrity().unwrap(),
            IntegrityRecovery::Quarantined {
                reason: case.expected_reason.into()
            },
            "{} recovery",
            case.name
        );
        assert_eq!(
            store.status().unwrap(),
            IntegrityStatus::Quarantined {
                reason: case.expected_reason.into()
            },
            "{} persistent quarantine",
            case.name
        );
    }
}

#[test]
fn ea112_confirmed_prepared_document_tamper_quarantines_instead_of_false_recovery() {
    register_ea112("prepared_document_tamper");
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    store.prepare_anchor(&input(key, 2, 2)).unwrap();
    confirm(&fixture, &store, "tx-2", 0, None);
    let prepared = fixture
        .anchor_dir
        .join("anchor-00000000000000000002-tx-2.prepared.json");
    let mut document = read_exact_document(&prepared).unwrap();
    document.external_receipt_digest = digest('e');
    fs::write(prepared, serialize_document(&document).unwrap()).unwrap();
    assert!(matches!(
        store.status().unwrap(),
        IntegrityStatus::Prepared {
            anchor_generation: 2,
            ..
        }
    ));
    assert_eq!(
        store.recover_integrity().unwrap(),
        IntegrityRecovery::Quarantined {
            reason: "anchor_digest_mismatch".into()
        }
    );
}

fn checkpoint_database(path: &std::path::Path) {
    open_sqlite_connection(path)
        .unwrap()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
}

fn copy_anchor_documents(source: &std::path::Path, destination: &std::path::Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn ea112_cross_root_database_anchor_copy_and_archive_restore_identity_are_rejected() {
    register_ea112("cross_root_database_anchor_copy");
    register_ea112("archive_restore_identity_binding");
    let source = Fixture::new();
    let (_source_store, _key) = first_finalized(&source);
    checkpoint_database(&source.paths.db_path);

    for name in [
        "cross_root_database_anchor_copy",
        "archive_restore_identity_binding",
    ] {
        let target = Fixture::new();
        fs::copy(&source.paths.db_path, &target.paths.db_path).unwrap();
        if name == "cross_root_database_anchor_copy" {
            copy_anchor_documents(&source.anchor_dir, &target.anchor_dir);
        }
        let copied = ReceiptIntegrityStore::with_protector(
            &target.paths,
            target.anchor_dir.clone(),
            source.protector.clone(),
        )
        .unwrap();
        assert_eq!(
            copied.status().unwrap(),
            IntegrityStatus::Mismatch {
                reason: "root_identity_mismatch".into()
            },
            "{name}"
        );
    }
}

#[cfg(windows)]
#[test]
fn ea112_windows_case_and_lexical_child_parent_aliases_keep_one_identity() {
    register_ea112("case_alias");
    register_ea112("lexical_child_parent_alias");
    let fixture = Fixture::new();
    let (store, _key) = first_finalized(&fixture);
    checkpoint_database(&fixture.paths.db_path);

    let raw = fixture.paths.root.to_string_lossy();
    let case_root = PathBuf::from(raw.replacen("Z:", "z:", 1));
    let case_store = ReceiptIntegrityStore::with_protector(
        &AppPaths::from_root(case_root),
        fixture.anchor_dir.clone(),
        fixture.protector.clone(),
    )
    .unwrap();
    assert_eq!(case_store.root_identity(), store.root_identity());
    assert!(matches!(
        case_store.status().unwrap(),
        IntegrityStatus::Trusted { .. }
    ));

    let child = fixture.paths.root.join("canonical-alias-child");
    fs::create_dir_all(&child).unwrap();
    let lexical_paths = AppPaths::from_root(child.join(".."));
    assert_ne!(lexical_paths.root, fixture.paths.root);
    let lexical_store = ReceiptIntegrityStore::with_protector(
        &lexical_paths,
        fixture.anchor_dir.clone(),
        fixture.protector.clone(),
    )
    .unwrap();
    assert_eq!(lexical_store.root_identity(), store.root_identity());
    assert!(matches!(
        lexical_store.status().unwrap(),
        IntegrityStatus::Trusted { .. }
    ));
}

#[cfg(windows)]
#[test]
fn ea112_windows_real_junction_and_hard_link_aliases_cannot_fork_identity() {
    use std::os::windows::fs::MetadataExt;

    register_ea112("junction_or_symlink_alias");
    register_ea112("hard_link_alias");
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    checkpoint_database(&fixture.paths.db_path);

    let alias_root = fixture.paths.root.parent().unwrap().join("state-junction");
    let output = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&alias_root)
        .arg(&fixture.paths.root)
        .output()
        .expect("launch native Windows junction creation");
    assert!(
        output.status.success(),
        "CONDITIONAL_PLATFORM_UNAVAILABLE: real Windows directory junction could not be created: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let attributes = fs::symlink_metadata(&alias_root)
        .expect("inspect real junction")
        .file_attributes();
    assert_ne!(
        attributes & 0x400,
        0,
        "created alias is not a Windows reparse point"
    );
    assert_eq!(
        fs::canonicalize(&alias_root).unwrap(),
        fs::canonicalize(&fixture.paths.root).unwrap()
    );
    let alias_store = ReceiptIntegrityStore::with_protector(
        &AppPaths::from_root(&alias_root),
        fixture.anchor_dir.clone(),
        fixture.protector.clone(),
    )
    .unwrap();
    assert_eq!(alias_store.root_identity(), store.root_identity());
    assert!(matches!(
        alias_store.status().unwrap(),
        IntegrityStatus::Trusted { .. }
    ));

    let hard_root = fixture.paths.root.parent().unwrap().join("hard-link-state");
    fs::create_dir_all(&hard_root).unwrap();
    let hard_paths = AppPaths::from_root(&hard_root);
    fs::hard_link(&fixture.paths.db_path, &hard_paths.db_path).unwrap();
    let hard_anchor = fixture
        .paths
        .root
        .parent()
        .unwrap()
        .join("hard-link-anchor");
    let hard_store =
        ReceiptIntegrityStore::with_protector(&hard_paths, hard_anchor, fixture.protector.clone())
            .unwrap();
    assert_ne!(hard_store.root_identity(), store.root_identity());
    assert_eq!(
        hard_store.status().unwrap(),
        IntegrityStatus::Mismatch {
            reason: "root_identity_mismatch".into()
        }
    );
    assert!(hard_store.prepare_anchor(&input(key, 2, 0)).is_err());
}

#[test]
fn create_finalize_restart_and_exact_external_pair_are_trusted() {
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    assert_eq!(
        store.status().expect("integrity status"),
        IntegrityStatus::Trusted {
            anchor_generation: 1,
            receipt_count: 0,
            receipt_head_digest: None,
            key,
        }
    );
    assert!(fixture.anchor_dir.join("current.json").is_file());
    assert!(fixture
        .anchor_dir
        .join("anchor-00000000000000000001-tx-1.finalized.json")
        .is_file());

    let reopened = fixture.store();
    assert!(matches!(
        reopened.recover_integrity().expect("restart recovery"),
        IntegrityRecovery::Healthy
    ));
}

#[test]
fn rotation_is_contiguous_and_old_key_must_still_be_available() {
    let fixture = Fixture::new();
    let (store, old_key) = first_finalized(&fixture);
    let new_key = store.rotate_key("receipt-key-two").expect("rotate key");
    assert_eq!(new_key.key_generation, 2);
    store
        .prepare_anchor(&input(new_key.clone(), 2, 2))
        .expect("prepare rotated anchor");
    let head = digest('b');
    confirm(&fixture, &store, "tx-2", 2, Some(&head));
    store
        .finalize_anchor("tx-2")
        .expect("finalize rotated anchor");
    assert!(matches!(
        store.status().expect("rotated status"),
        IntegrityStatus::Trusted { key, .. } if key == new_key
    ));
    fixture.protector.delete(&old_key).expect("delete old key");
    assert!(store.rotate_key("receipt-key-three").is_ok());
}

#[test]
fn current_key_loss_quarantines_without_fabricating_a_replacement() {
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    fixture.protector.delete(&key).expect("delete current key");
    assert_eq!(
        store.status().expect("key loss status"),
        IntegrityStatus::KeyLost { key: key.clone() }
    );
    assert_eq!(
        store.recover_integrity().expect("key-loss recovery"),
        IntegrityRecovery::Quarantined {
            reason: "receipt_key_unavailable".into()
        }
    );
    assert!(fixture.anchor_dir.join("quarantine.json").is_file());
    assert!(matches!(
        store.status().expect("quarantined status"),
        IntegrityStatus::Quarantined { .. }
    ));
    assert!(store.provision_initial_key("replacement").is_err());
}

#[test]
fn unconfirmed_prepared_pair_is_discarded_on_restart() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-one")
        .expect("provision initial key");
    store
        .prepare_anchor(&input(key, 1, 1))
        .expect("prepare initial anchor");
    assert!(matches!(
        store.status().expect("prepared status"),
        IntegrityStatus::Prepared { .. }
    ));
    assert_eq!(
        fixture
            .store()
            .recover_integrity()
            .expect("recover prepare"),
        IntegrityRecovery::RestoredLastProvenPair {
            anchor_generation: None
        }
    );
    assert!(matches!(
        fixture.store().status().expect("uninitialized status"),
        IntegrityStatus::Uninitialized
    ));
}

#[test]
fn confirmed_prepared_pair_is_finalized_on_restart() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-one")
        .expect("provision initial key");
    store
        .prepare_anchor(&input(key, 1, 1))
        .expect("prepare initial anchor");
    let head = digest('a');
    confirm(&fixture, &store, "tx-1", 1, Some(&head));
    assert_eq!(
        fixture
            .store()
            .recover_integrity()
            .expect("recover confirmed prepare"),
        IntegrityRecovery::FinalizedInterruptedCommit {
            transaction_id: "tx-1".into()
        }
    );
    assert!(matches!(
        fixture.store().status().expect("trusted status"),
        IntegrityStatus::Trusted { .. }
    ));
}

#[test]
fn raw_sql_confirmation_flip_cannot_forge_receipt_commit_proof() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-one")
        .expect("provision initial key");
    store
        .prepare_anchor(&input(key, 1, 1))
        .expect("prepare initial anchor");
    let conn = open_sqlite_connection(&fixture.paths.db_path).expect("open database");
    assert_eq!(
        conn.execute(
            "UPDATE execass_receipt_anchor_state SET receipt_commit_confirmed=1, receipt_committed_at=1900000000001, receipt_commit_confirmation_tag=?1 WHERE transaction_id='tx-1'",
            [digest('f')],
        )
        .expect("store attacker-controlled confirmation"),
        1
    );
    assert!(store.finalize_anchor("tx-1").is_err());
    assert_eq!(
        store.status().expect("forged confirmation status"),
        IntegrityStatus::Mismatch {
            reason: "receipt_commit_confirmation_mismatch".into()
        }
    );
    assert!(matches!(
        store.recover_integrity().expect("quarantine forgery"),
        IntegrityRecovery::Quarantined { .. }
    ));
}

#[test]
fn concurrent_prepare_and_finalize_cannot_delete_or_split_the_winning_pair() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let key = store
        .provision_initial_key("receipt-key-one")
        .expect("provision initial key");
    let candidate = input(key, 1, 1);
    let first = store.clone();
    let second = store.clone();
    let first_input = candidate.clone();
    let second_input = candidate.clone();
    let first_result = thread::spawn(move || first.prepare_anchor(&first_input));
    let second_result = thread::spawn(move || second.prepare_anchor(&second_input));
    let outcomes = [
        first_result.join().expect("first prepare thread"),
        second_result.join().expect("second prepare thread"),
    ];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(fixture
        .anchor_dir
        .join("anchor-00000000000000000001-tx-1.prepared.json")
        .is_file());
    let head = digest('a');
    confirm(&fixture, &store, "tx-1", 1, Some(&head));

    let first = store.clone();
    let second = store.clone();
    let first_result = thread::spawn(move || first.finalize_anchor("tx-1"));
    let second_result = thread::spawn(move || second.finalize_anchor("tx-1"));
    first_result
        .join()
        .expect("first finalize thread")
        .expect("first finalize");
    second_result
        .join()
        .expect("second finalize thread")
        .expect("second finalize");
    assert!(matches!(
        store.status().expect("concurrent final status"),
        IntegrityStatus::Trusted { .. }
    ));
}

#[test]
fn orphan_external_prepare_restores_the_last_proven_pair() {
    let fixture = Fixture::new();
    let (store, _key) = first_finalized(&fixture);
    fs::write(
        fixture
            .anchor_dir
            .join("anchor-00000000000000000002-tx-orphan.prepared.json"),
        b"crash-left-external-prepare",
    )
    .expect("simulate abrupt crash after external prepare");
    assert_eq!(
        store.recover_integrity().expect("restore last pair"),
        IntegrityRecovery::RestoredLastProvenPair {
            anchor_generation: Some(1)
        }
    );
    assert!(matches!(
        store.status().expect("status after restore"),
        IntegrityStatus::Trusted {
            anchor_generation: 1,
            ..
        }
    ));
}

#[test]
fn every_prepare_boundary_recovers_to_empty_or_finalized_state() {
    for failpoint in [
        "anchor.prepare.write.before",
        "anchor.prepare.write.after",
        "anchor.prepare.sync.after",
        "anchor.prepare.rename.after",
        "anchor.prepare.dir_sync.after",
        "db.prepare.before",
        "db.prepare.after",
    ] {
        let fixture = Fixture::new();
        let base = fixture.store();
        let key = base
            .provision_initial_key("receipt-key-one")
            .expect("provision key");
        let failing = fixture.fail_store(failpoint);
        assert!(
            failing.prepare_anchor(&input(key, 1, 1)).is_err(),
            "{failpoint}"
        );
        let recovery = fixture
            .store()
            .recover_integrity()
            .expect("recover prepare");
        assert!(
            matches!(
                recovery,
                IntegrityRecovery::Healthy
                    | IntegrityRecovery::RestoredLastProvenPair { .. }
                    | IntegrityRecovery::FinalizedInterruptedCommit { .. }
            ),
            "unexpected recovery for {failpoint}: {recovery:?}"
        );
        assert!(matches!(
            fixture.store().status().expect("post-recovery status"),
            IntegrityStatus::Uninitialized | IntegrityStatus::Trusted { .. }
        ));
    }
}

#[test]
fn every_finalize_boundary_recovers_without_false_quarantine() {
    for failpoint in [
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
    ] {
        let fixture = Fixture::new();
        let base = fixture.store();
        let key = base
            .provision_initial_key("receipt-key-one")
            .expect("provision key");
        base.prepare_anchor(&input(key, 1, 1)).expect("prepare");
        let head = digest('a');
        confirm(&fixture, &base, "tx-1", 1, Some(&head));
        let failing = fixture.fail_store(failpoint);
        assert!(failing.finalize_anchor("tx-1").is_err(), "{failpoint}");
        let recovery = fixture
            .store()
            .recover_integrity()
            .expect("recover finalize");
        assert!(
            matches!(
                recovery,
                IntegrityRecovery::Healthy | IntegrityRecovery::FinalizedInterruptedCommit { .. }
            ),
            "unexpected recovery for {failpoint}: {recovery:?}"
        );
        assert!(matches!(
            fixture.store().status().expect("trusted after recovery"),
            IntegrityStatus::Trusted { .. }
        ));
    }
}

#[test]
fn synchronous_finalize_failure_restores_the_previous_external_current_document() {
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    let current_path = fixture.anchor_dir.join("current.json");
    let previous_current = fs::read(&current_path).expect("read previous current anchor");
    store
        .prepare_anchor(&input(key, 2, 2))
        .expect("prepare second anchor");
    let head = digest('b');
    confirm(&fixture, &store, "tx-2", 2, Some(&head));
    let failing = fixture.fail_store("db.finalize.before");
    assert!(failing.finalize_anchor("tx-2").is_err());
    assert_eq!(
        fs::read(&current_path).expect("read restored current anchor"),
        previous_current
    );
    assert!(matches!(
        store.status().expect("prepared after restored failure"),
        IntegrityStatus::Prepared {
            anchor_generation: 2,
            ..
        }
    ));
    assert!(matches!(
        store.recover_integrity().expect("recover second anchor"),
        IntegrityRecovery::FinalizedInterruptedCommit { .. }
    ));
    assert!(matches!(
        store.status().expect("trusted recovered second anchor"),
        IntegrityStatus::Trusted {
            anchor_generation: 2,
            ..
        }
    ));
}

#[test]
fn tampered_current_anchor_is_quarantined() {
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    let current = fixture.anchor_dir.join("current.json");
    let mut bytes = fs::read(&current).expect("read current anchor");
    let offset = bytes.len() / 2;
    bytes[offset] ^= 1;
    fs::write(&current, &bytes).expect("tamper current anchor");
    assert!(store.prepare_anchor(&input(key, 2, 2)).is_err());
    assert_eq!(fs::read(&current).expect("read rejected anchor"), bytes);
    assert!(matches!(
        store.status().expect("tamper status"),
        IntegrityStatus::Mismatch { .. }
    ));
    assert!(matches!(
        store.recover_integrity().expect("tamper recovery"),
        IntegrityRecovery::Quarantined { .. }
    ));
}

#[test]
fn canonical_rewrite_and_plain_sha_recompute_cannot_forge_anchor_hmac() {
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    let current = fixture.anchor_dir.join("current.json");
    let mut document = read_exact_document(&current).expect("read canonical anchor");
    let key_material = store.load_key(&key).expect("load test key");
    verify_anchor_document_integrity(&document, &key_material, None)
        .expect("original HMAC verifies");

    document.external_receipt_digest = digest('e');
    let canonical = serialize_document(&document).expect("canonical attacker rewrite");
    let recomputed_plain_sha = sha256_hex(&canonical);
    assert_eq!(recomputed_plain_sha.len(), 64);
    assert!(verify_anchor_document_integrity(&document, &key_material, None).is_err());
}

#[test]
fn rotated_anchor_is_cross_signed_by_old_and_new_keys() {
    let fixture = Fixture::new();
    let (store, old_key) = first_finalized(&fixture);
    let new_key = store.rotate_key("receipt-key-two").expect("rotate key");
    store
        .prepare_anchor(&input(new_key.clone(), 2, 2))
        .expect("prepare rotated anchor");
    let prepared = read_exact_document(
        &fixture
            .anchor_dir
            .join("anchor-00000000000000000002-tx-2.prepared.json"),
    )
    .expect("read prepared rotation");
    let new_material = store.load_key(&new_key).expect("load new key");
    let old_material = store.load_key(&old_key).expect("load old key");
    verify_anchor_document_integrity(&prepared, &new_material, Some(&old_material))
        .expect("prepared rotation cross-signatures verify");
    assert!(prepared.previous_key_integrity_tag.is_some());

    let head = digest('b');
    confirm(&fixture, &store, "tx-2", 2, Some(&head));
    store.finalize_anchor("tx-2").expect("finalize rotation");
    let finalized = read_exact_document(&fixture.anchor_dir.join("current.json"))
        .expect("read finalized rotation");
    verify_anchor_document_integrity(&finalized, &new_material, Some(&old_material))
        .expect("finalized rotation cross-signatures verify");
}

#[test]
fn key_creation_failpoint_removes_os_orphan_and_registry_row() {
    let fixture = Fixture::new();
    let failing = fixture.fail_store("key.create.after");
    assert!(failing.provision_initial_key("receipt-key-one").is_err());
    assert!(fixture
        .protector
        .keys
        .lock()
        .expect("key map lock")
        .is_empty());
    let conn = open_sqlite_connection(&fixture.paths.db_path).expect("open database");
    assert_eq!(receipt_key_count_with_conn(&conn).expect("count keys"), 0);
    fixture
        .store()
        .provision_initial_key("receipt-key-one")
        .expect("retry cleanly provisions key");
}

#[test]
fn concurrent_initial_key_provisioning_has_one_registry_winner() {
    let fixture = Fixture::new();
    let first = fixture.store();
    let second = fixture.store();
    let first_result = thread::spawn(move || first.provision_initial_key("candidate-one"));
    let second_result = thread::spawn(move || second.provision_initial_key("candidate-two"));
    let outcomes = [
        first_result.join().expect("first key thread"),
        second_result.join().expect("second key thread"),
    ];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    let conn = open_sqlite_connection(&fixture.paths.db_path).expect("open database");
    assert_eq!(receipt_key_count_with_conn(&conn).expect("count keys"), 1);
    assert_eq!(
        fixture.protector.keys.lock().expect("key map lock").len(),
        1
    );
}

#[test]
fn schema_anchor_identity_and_quarantine_are_terminally_guarded() {
    let fixture = Fixture::new();
    let (store, key) = first_finalized(&fixture);
    let conn = open_sqlite_connection(&fixture.paths.db_path).expect("open database");
    assert!(conn
        .execute(
            "UPDATE execass_receipt_anchor_state SET receipt_count=99 WHERE transaction_id='tx-1'",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_receipt_anchor_state SET status='quarantined', receipt_commit_confirmed=0, receipt_committed_at=NULL, quarantined_at=1, quarantine_reason='forged' WHERE transaction_id='tx-1'",
            [],
        )
        .is_err());
    fixture.protector.delete(&key).expect("lose key");
    store.recover_integrity().expect("quarantine key loss");
    assert!(conn
        .execute(
            "UPDATE execass_receipt_anchor_state SET status='finalized' WHERE transaction_id='tx-1'",
            [],
        )
        .is_err());
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM execass_receipt_anchor_state WHERE transaction_id='tx-1'",
            [],
            |row| row.get(0),
        )
        .optional()
        .expect("query status");
    assert_eq!(status.as_deref(), Some("quarantined"));
    let key_status: String = conn
        .query_row(
            "SELECT status FROM execass_receipt_keys WHERE key_id='receipt-key-one'",
            [],
            |row| row.get(0),
        )
        .expect("query key status");
    assert_eq!(key_status, "lost");
}

#[test]
fn production_custody_source_keeps_locked_platform_security_flags() {
    let source = include_str!("receipt_integrity.rs");
    assert!(source.contains("CRYPTPROTECT_UI_FORBIDDEN"));
    assert!(source.contains("BCRYPT_USE_SYSTEM_PREFERRED_RNG"));
    assert!(!source.contains("CRYPTPROTECT_LOCAL_MACHINE"));
    assert!(source.contains("use_protected_keychain"));
    assert!(source.contains("set_access_synchronized(Some(false))"));
    assert!(source.contains("set_access_group(self.keychain_access_group)"));
    assert!(source.contains("CARSINOS_KEYCHAIN_ACCESS_GROUP"));
    assert!(source.contains("std::ptr::write_bytes(output.pbData, 0"));
}

#[cfg(windows)]
#[test]
fn production_windows_dpapi_roundtrip_keeps_plaintext_out_of_blob() {
    let temp = tempfile::tempdir_in("Z:\\carsinos").expect("project-drive tempdir");
    let paths = AppPaths::from_root(temp.path().join("state"));
    init_execass_fresh_root(&paths).expect("initialize ExecAss root");
    let store = ReceiptIntegrityStore::open(&paths).expect("open production integrity store");
    let key = store
        .provision_initial_key("windows-dpapi-proof")
        .expect("provision DPAPI key");
    let plaintext = store.load_key(&key).expect("load DPAPI key");
    assert_eq!(plaintext.len(), 32);
    let blob_path = fs::read_dir(store.anchor_directory().join("keys"))
        .expect("read key directory")
        .next()
        .expect("DPAPI blob entry")
        .expect("DPAPI blob")
        .path();
    let blob = fs::read(blob_path).expect("read DPAPI blob");
    assert_ne!(blob.as_slice(), plaintext.as_slice());
    assert!(!blob
        .windows(plaintext.len())
        .any(|window| window == plaintext.as_slice()));
}
