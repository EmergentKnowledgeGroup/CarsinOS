use super::tests::{fixture, Fixture};
use super::*;
use crate::open_sqlite_connection;
use carsinos_core::execass_actor::{
    issue_test_local_owner_authority, TestLocalOwnerAuthorityInput, VerifiedOwnerAuthority,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const NOW: i64 = 1_800_000_100_000;

struct Rig {
    fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    key: ReceiptKeyRef,
    redactor: ReceiptRedactor,
    owner_credential: String,
}

fn setup() -> Rig {
    let fixture = fixture();
    let identity = activate_test_confirmation_authority(&fixture.store, [71; 32]).unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute("INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(1,'execass',1,'settings-install','settings-user','settings-host',1)", []).unwrap();
    conn.execute("INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('settings-lease','execass',1,'settings-host',1,1,9999999999999)", []).unwrap();
    conn.execute("INSERT INTO execass_runtime_host_states(generation,host_instance_id,fencing_token,state_root_generation,actual_state,updated_at) VALUES(1,'settings-host',1,1,'starting',1)", []).unwrap();
    drop(conn);
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("policy-settings-key")
        .unwrap();
    Rig {
        fixture,
        integrity,
        key,
        redactor: ReceiptRedactor::new(&["policy-settings-test-secret"]).unwrap(),
        owner_credential: identity.local_credential_identity().to_string(),
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn heads(rig: &Rig) -> (i64, Option<String>, i64, Option<String>, i64) {
    let conn = open_sqlite_connection(&rig.fixture.paths.db_path).unwrap();
    conn.query_row(
        "SELECT j.receipt_count,j.receipt_head_digest,d.receipt_chain_count,d.receipt_chain_head_digest,d.state_revision FROM execass_receipt_journal_state j JOIN execass_delegations d ON d.delegation_id='execass-global-control-carrier' WHERE j.singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).unwrap()
}

fn owner(
    credential: &str,
    kind: &str,
    revision: i64,
    scope: String,
    suffix: &str,
    created_at: i64,
) -> VerifiedOwnerAuthority {
    issue_test_local_owner_authority(TestLocalOwnerAuthorityInput {
        authenticated_client_id: credential.to_string(),
        authenticated_ingress: "native-control".into(),
        channel_assurance: "interactive-local".into(),
        request_correlation_id: format!("corr-{suffix}"),
        source_message_id: None,
        normalized_intent: format!("update {kind}"),
        instruction_revision: format!("instruction-{suffix}"),
        instruction_bytes: format!("instruction bytes {suffix}").into_bytes(),
        owner_envelope_revision: format!("envelope-{suffix}"),
        owner_envelope_json: "{}".into(),
        authority_kind: kind.into(),
        normalized_scope_json: scope,
        policy_revision: revision,
        bound_decision_id: None,
        bound_decision_revision: None,
        bound_manifest_bytes: None,
        challenge_nonce_bytes: None,
        created_at,
        expires_at: None,
    })
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn base_receipt(
    rig: &Rig,
    suffix: &str,
    event_id: &str,
    kind: ReceiptKind,
    subject_kind: ReceiptSubjectKind,
    subject_id: &str,
    revision: i64,
    authority: &VerifiedOwnerAuthority,
    created_at: i64,
) -> AppendReceiptCommand {
    let (global_count, global_head, delegation_count, delegation_head, state_revision) = heads(rig);
    AppendReceiptCommand {
        receipt_id: format!("receipt-{suffix}"),
        transaction_id: format!("tx-{suffix}"),
        state_root_generation: 1,
        delegation_id: "execass-global-control-carrier".into(),
        expected_state_revision: state_revision,
        expected_global_count: global_count,
        expected_global_head_digest: global_head,
        expected_delegation_count: delegation_count,
        expected_delegation_head_digest: delegation_head,
        receipt_kind: kind,
        subject: ReceiptSubject {
            kind: subject_kind,
            subject_id: subject_id.into(),
            revision,
        },
        causation_id: format!("cause-{suffix}"),
        causation_event_id: event_id.into(),
        actor: ReceiptActorBinding {
            actor_type: ActorType::HumanLocal,
            actor_identity: SafeText::new(authority.evidence().credential(), &[]).unwrap(),
            authority_provenance_id: authority.authority_provenance_id().into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "settings-host".into(),
            fencing_token: 1,
        },
        key: rig.key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::summary("canonical settings changed", &[]).unwrap(),
        occurred_at: created_at,
        committed_at: created_at,
    }
}

trait EvidenceCredential {
    fn credential(&self) -> &str;
}

impl EvidenceCredential for carsinos_core::execass_actor::VerifiedHumanEvidenceRef<'_> {
    fn credential(&self) -> &str {
        match self {
            Self::Local {
                authenticated_client_id,
                ..
            } => authenticated_client_id,
            Self::Remote {
                adapter_id,
                provider_account_id,
                ..
            } => {
                // Remote storage identity is adapter:account. Tests use local evidence.
                assert!(adapter_id.is_empty() && provider_account_id.is_empty());
                unreachable!()
            }
        }
    }
}

fn policy_command(
    rig: &Rig,
    suffix: &str,
    expected: i64,
    idempotency_key: &str,
    snapshot: &str,
    credential: &str,
    created_at: i64,
) -> (UpdateExecAssPolicyCommand, VerifiedOwnerAuthority) {
    let safe = SafeJson::from_str(snapshot, &["policy-settings-test-secret"]).unwrap();
    let snapshot_digest = digest(&safe.canonical_bytes());
    let revision = expected + 1;
    let authority = owner(
        credential,
        "policy_snapshot",
        revision,
        json!({"policy_revision": revision, "policy_snapshot_digest": snapshot_digest}).to_string(),
        suffix,
        created_at,
    );
    let event_id = format!("policy-event-{suffix}");
    let receipt = base_receipt(
        rig,
        suffix,
        &event_id,
        ReceiptKind::Policy,
        ReceiptSubjectKind::PolicyRevision,
        "execass-policy",
        revision,
        &authority,
        created_at,
    );
    let outbox_event = NewOutboxEvent {
        event_id,
        event_name: OutboxEventName::PolicyChanged,
        aggregate_id: "execass-policy".into(),
        aggregate_revision: revision,
        correlation_id: format!("corr-{suffix}"),
        causation_id: receipt.causation_id.clone(),
        occurred_at: created_at,
        safe_payload_json: json!({"configured":true,"policy_revision":revision,"policy_snapshot_digest":snapshot_digest}).to_string(),
        duplicate_identity: idempotency_key.into(),
    };
    (
        UpdateExecAssPolicyCommand {
            expected_policy_revision: expected,
            idempotency_key: idempotency_key.into(),
            safe_policy_snapshot: safe,
            created_at,
            outbox_event,
            receipt,
        },
        authority,
    )
}

fn settings_digest(mode: RuntimeDesiredMode, start_at_login: bool, safe: &SafeJson) -> String {
    digest(json!({"desired_mode":mode.as_str(),"start_at_login":start_at_login,"settings":serde_json::from_slice::<serde_json::Value>(&safe.canonical_bytes()).unwrap()}).to_string().as_bytes())
}

#[allow(clippy::too_many_arguments)]
fn runtime_command(
    rig: &Rig,
    suffix: &str,
    expected: i64,
    idempotency_key: &str,
    mode: RuntimeDesiredMode,
    start_at_login: bool,
    settings: &str,
    credential: &str,
    created_at: i64,
) -> (UpdateExecAssRuntimeSettingsCommand, VerifiedOwnerAuthority) {
    let safe = SafeJson::from_str(settings, &["policy-settings-test-secret"]).unwrap();
    let settings_digest = settings_digest(mode, start_at_login, &safe);
    let revision = expected + 1;
    let authority = owner(
        credential,
        "runtime_settings_snapshot",
        1,
        json!({"settings_revision":revision,"settings_digest":settings_digest}).to_string(),
        suffix,
        created_at,
    );
    let event_id = format!("runtime-event-{suffix}");
    let receipt = base_receipt(
        rig,
        suffix,
        &event_id,
        ReceiptKind::RuntimeSettings,
        ReceiptSubjectKind::RuntimeSettingsRevision,
        "execass-runtime-host",
        revision,
        &authority,
        created_at,
    );
    let running = if mode == RuntimeDesiredMode::Background {
        "running_background"
    } else {
        "running_app_bound"
    };
    let outbox_event = NewOutboxEvent {
        event_id,
        event_name: OutboxEventName::RuntimeHostChanged,
        aggregate_id: "execass-runtime-host".into(),
        aggregate_revision: revision,
        correlation_id: format!("corr-{suffix}"),
        causation_id: receipt.causation_id.clone(),
        occurred_at: created_at,
        safe_payload_json: json!({"actual_state_if_running":running,"desired_mode":mode.as_str(),"settings_digest":settings_digest,"settings_revision":revision,"start_at_login":start_at_login}).to_string(),
        duplicate_identity: idempotency_key.into(),
    };
    (
        UpdateExecAssRuntimeSettingsCommand {
            expected_settings_revision: expected,
            idempotency_key: idempotency_key.into(),
            desired_mode: mode,
            start_at_login,
            safe_settings: safe,
            created_at,
            outbox_event,
            receipt,
        },
        authority,
    )
}

fn count(rig: &Rig, table: &str) -> i64 {
    let conn = open_sqlite_connection(&rig.fixture.paths.db_path).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn policy_revision_is_canonical_immutable_and_semantic_replay_is_owner_bound() {
    let rig = setup();
    let (first, owner_one) = policy_command(
        &rig,
        "policy-one",
        1,
        "policy-key",
        r#"{"profile":"balanced","token":"policy-settings-test-secret"}"#,
        &rig.owner_credential,
        NOW,
    );
    let ExecAssPolicyUpdateOutcome::Updated {
        policy, receipt, ..
    } = rig
        .fixture
        .store
        .update_execass_policy_atomically(&rig.integrity, &rig.redactor, &first, &owner_one)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(policy.policy_revision, 2);
    assert!(!policy
        .policy_snapshot_json
        .contains("policy-settings-test-secret"));
    let durable_heads = heads(&rig);

    let (later_retry, later_owner) = policy_command(
        &rig,
        "policy-later",
        1,
        "policy-key",
        r#"{"token":"policy-settings-test-secret","profile":"balanced"}"#,
        &rig.owner_credential,
        NOW + 50,
    );
    let ExecAssPolicyUpdateOutcome::Replayed {
        receipt: replayed, ..
    } = rig
        .fixture
        .store
        .update_execass_policy_atomically(&rig.integrity, &rig.redactor, &later_retry, &later_owner)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(replayed.receipt_id, receipt.receipt_id);
    assert_eq!(heads(&rig), durable_heads);

    let (other_owner_retry, other_owner) = policy_command(
        &rig,
        "policy-other-owner",
        1,
        "policy-key",
        r#"{"profile":"balanced","token":"policy-settings-test-secret"}"#,
        "different-owner",
        NOW + 60,
    );
    assert_eq!(
        rig.fixture
            .store
            .update_execass_policy_atomically(
                &rig.integrity,
                &rig.redactor,
                &other_owner_retry,
                &other_owner
            )
            .unwrap(),
        ExecAssPolicyUpdateOutcome::Conflict
    );
    let (body_collision, collision_owner) = policy_command(
        &rig,
        "policy-collision",
        2,
        "policy-key",
        r#"{"profile":"full_send"}"#,
        &rig.owner_credential,
        NOW + 70,
    );
    assert_eq!(
        rig.fixture
            .store
            .update_execass_policy_atomically(
                &rig.integrity,
                &rig.redactor,
                &body_collision,
                &collision_owner
            )
            .unwrap(),
        ExecAssPolicyUpdateOutcome::Conflict
    );
    let (stale, stale_owner) = policy_command(
        &rig,
        "policy-stale",
        1,
        "policy-stale-key",
        r#"{"profile":"custom"}"#,
        &rig.owner_credential,
        NOW + 80,
    );
    assert_eq!(
        rig.fixture
            .store
            .update_execass_policy_atomically(&rig.integrity, &rig.redactor, &stale, &stale_owner)
            .unwrap(),
        ExecAssPolicyUpdateOutcome::Stale {
            current_policy_revision: 2
        }
    );
    assert_eq!(heads(&rig), durable_heads);
    assert_eq!(rig.fixture.store.current_execass_policy().unwrap(), policy);
    assert_eq!(count(&rig, "execass_policy_revisions"), 2);
    let conn = open_sqlite_connection(&rig.fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            "UPDATE execass_policy_revisions SET policy_snapshot_json='{}' WHERE policy_revision=2",
            []
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_policy_revisions WHERE policy_revision=2",
            []
        )
        .is_err());
}

#[test]
fn policy_forgery_and_receipt_failure_roll_back_every_atomic_row_and_pointer() {
    let rig = setup();
    let baseline_heads = heads(&rig);
    let baseline_authorities = count(&rig, "execass_authority_provenance");
    let baseline_outbox = count(&rig, "execass_outbox_events");
    let (forged, forged_owner) = policy_command(
        &rig,
        "forged",
        1,
        "forged-key",
        "{\"profile\":\"balanced\"}",
        "forged-owner",
        NOW,
    );
    assert!(rig
        .fixture
        .store
        .update_execass_policy_atomically(&rig.integrity, &rig.redactor, &forged, &forged_owner)
        .is_err());
    assert_eq!(
        count(&rig, "execass_authority_provenance"),
        baseline_authorities
    );

    let (outbox_failure, outbox_owner) = policy_command(
        &rig,
        "outbox-failure",
        1,
        "outbox-failure-key",
        "{\"profile\":\"balanced\"}",
        &rig.owner_credential,
        NOW + 1,
    );
    let conn = open_sqlite_connection(&rig.fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.policy.changed','execass-policy',2,'preexisting','preexisting',1,'v1','{}','preexisting-outbox-failure')",
        [outbox_failure.outbox_event.event_id.as_str()],
    ).unwrap();
    drop(conn);
    let outbox_after_seed = count(&rig, "execass_outbox_events");
    assert!(rig
        .fixture
        .store
        .update_execass_policy_atomically(
            &rig.integrity,
            &rig.redactor,
            &outbox_failure,
            &outbox_owner
        )
        .is_err());
    assert_eq!(
        count(&rig, "execass_authority_provenance"),
        baseline_authorities
    );
    assert_eq!(count(&rig, "execass_outbox_events"), outbox_after_seed);
    assert_eq!(count(&rig, "execass_policy_revisions"), 1);
    assert_eq!(heads(&rig), baseline_heads);

    let (mut broken, owner) = policy_command(
        &rig,
        "broken-receipt",
        1,
        "broken-key",
        "{\"profile\":\"balanced\"}",
        &rig.owner_credential,
        NOW + 2,
    );
    broken.receipt.expected_global_count += 1;
    assert!(rig
        .fixture
        .store
        .update_execass_policy_atomically(&rig.integrity, &rig.redactor, &broken, &owner)
        .is_err());
    assert_eq!(heads(&rig), baseline_heads);
    assert_eq!(count(&rig, "execass_policy_revisions"), 1);
    assert_eq!(
        count(&rig, "execass_authority_provenance"),
        baseline_authorities
    );
    assert_eq!(count(&rig, "execass_outbox_events"), baseline_outbox + 1);
    assert_eq!(
        rig.fixture
            .store
            .global_stop_status()
            .unwrap()
            .current_policy_revision,
        1
    );
}

#[test]
fn runtime_settings_are_typed_immutable_cas_and_semantic_replay_safe() {
    let rig = setup();
    let (first, owner_one) = runtime_command(
        &rig,
        "runtime-one",
        0,
        "runtime-key",
        RuntimeDesiredMode::Background,
        true,
        r#"{"max_parallel":2,"secret":"policy-settings-test-secret"}"#,
        &rig.owner_credential,
        NOW,
    );
    let ExecAssRuntimeSettingsUpdateOutcome::Updated {
        status, receipt, ..
    } = rig
        .fixture
        .store
        .update_execass_runtime_settings_atomically(
            &rig.integrity,
            &rig.redactor,
            &first,
            &owner_one,
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(status.actual_state, RuntimeActualState::Starting);
    assert!(!status
        .config
        .as_ref()
        .unwrap()
        .settings_json
        .contains("policy-settings-test-secret"));
    rig.fixture
        .store
        .transition_runtime_host(
            status.live_lease.as_ref().unwrap(),
            RuntimeHostTransition::ReachDesiredMode,
            NOW + 1,
        )
        .unwrap();
    assert_eq!(
        rig.fixture
            .store
            .execass_runtime_host_status(NOW + 1)
            .unwrap()
            .actual_state,
        RuntimeActualState::RunningBackground
    );
    let durable_heads = heads(&rig);

    let (retry, retry_owner) = runtime_command(
        &rig,
        "runtime-retry",
        0,
        "runtime-key",
        RuntimeDesiredMode::Background,
        true,
        r#"{"secret":"policy-settings-test-secret","max_parallel":2}"#,
        &rig.owner_credential,
        NOW + 50,
    );
    let ExecAssRuntimeSettingsUpdateOutcome::Replayed {
        receipt: replayed, ..
    } = rig
        .fixture
        .store
        .update_execass_runtime_settings_atomically(
            &rig.integrity,
            &rig.redactor,
            &retry,
            &retry_owner,
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(replayed.receipt_id, receipt.receipt_id);
    let (owner_collision, other_owner) = runtime_command(
        &rig,
        "runtime-owner-collision",
        0,
        "runtime-key",
        RuntimeDesiredMode::Background,
        true,
        r#"{"max_parallel":2,"secret":"policy-settings-test-secret"}"#,
        "other-owner",
        NOW + 60,
    );
    assert_eq!(
        rig.fixture
            .store
            .update_execass_runtime_settings_atomically(
                &rig.integrity,
                &rig.redactor,
                &owner_collision,
                &other_owner
            )
            .unwrap(),
        ExecAssRuntimeSettingsUpdateOutcome::Conflict
    );
    let (body_collision, collision_owner) = runtime_command(
        &rig,
        "runtime-body-collision",
        1,
        "runtime-key",
        RuntimeDesiredMode::AppBound,
        false,
        "{\"max_parallel\":1}",
        &rig.owner_credential,
        NOW + 70,
    );
    assert_eq!(
        rig.fixture
            .store
            .update_execass_runtime_settings_atomically(
                &rig.integrity,
                &rig.redactor,
                &body_collision,
                &collision_owner
            )
            .unwrap(),
        ExecAssRuntimeSettingsUpdateOutcome::Conflict
    );
    let (stale, stale_owner) = runtime_command(
        &rig,
        "runtime-stale",
        0,
        "runtime-stale-key",
        RuntimeDesiredMode::AppBound,
        false,
        "{}",
        &rig.owner_credential,
        NOW + 80,
    );
    assert_eq!(
        rig.fixture
            .store
            .update_execass_runtime_settings_atomically(
                &rig.integrity,
                &rig.redactor,
                &stale,
                &stale_owner
            )
            .unwrap(),
        ExecAssRuntimeSettingsUpdateOutcome::Stale {
            current_settings_revision: 1
        }
    );
    assert_eq!(heads(&rig), durable_heads);
    assert_eq!(count(&rig, "execass_runtime_settings_revisions"), 1);
    let conn = open_sqlite_connection(&rig.fixture.paths.db_path).unwrap();
    assert!(conn.execute("UPDATE execass_runtime_settings_revisions SET settings_json='{}' WHERE settings_revision=1", []).is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_runtime_settings_revisions WHERE settings_revision=1",
            []
        )
        .is_err());
}

#[test]
fn runtime_invalid_combinations_and_receipt_failure_are_zero_mutation() {
    let rig = setup();
    let baseline_heads = heads(&rig);
    let baseline_authorities = count(&rig, "execass_authority_provenance");
    let baseline_outbox = count(&rig, "execass_outbox_events");
    let (invalid, invalid_owner) = runtime_command(
        &rig,
        "runtime-invalid",
        0,
        "invalid-key",
        RuntimeDesiredMode::AppBound,
        true,
        "{}",
        &rig.owner_credential,
        NOW,
    );
    assert!(rig
        .fixture
        .store
        .update_execass_runtime_settings_atomically(
            &rig.integrity,
            &rig.redactor,
            &invalid,
            &invalid_owner
        )
        .is_err());
    let (mut broken, owner) = runtime_command(
        &rig,
        "runtime-broken",
        0,
        "broken-runtime-key",
        RuntimeDesiredMode::Background,
        true,
        "{}",
        &rig.owner_credential,
        NOW + 1,
    );
    broken.receipt.expected_delegation_count += 1;
    assert!(rig
        .fixture
        .store
        .update_execass_runtime_settings_atomically(&rig.integrity, &rig.redactor, &broken, &owner)
        .is_err());
    assert_eq!(heads(&rig), baseline_heads);
    assert_eq!(count(&rig, "execass_runtime_settings_revisions"), 0);
    assert_eq!(
        count(&rig, "execass_authority_provenance"),
        baseline_authorities
    );
    assert_eq!(count(&rig, "execass_outbox_events"), baseline_outbox);
}
