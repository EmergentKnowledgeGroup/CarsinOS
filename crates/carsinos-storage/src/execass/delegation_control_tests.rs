use super::tests::{fixture, foundation, table_count, Fixture};
use super::*;
use crate::open_sqlite_connection;
use carsinos_protocol::execass::{
    run_control_attestation_signing_bytes, ActorType as ProtocolActorType, RunControlAttestation,
    RunControlAttestationPayload, RunControlOperation, RunControlTarget,
};
use ed25519_dalek::{Signer, SigningKey};
use std::thread;

const NOW: i64 = 1_800_000_200_000;
const TEST_SIGNING_SEED: [u8; 32] = [73; 32];

struct ControlFixture {
    fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    key: ReceiptKeyRef,
    redactor: ReceiptRedactor,
    confirmation_identity: ConfirmationAuthorityIdentity,
}

fn setup() -> ControlFixture {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let confirmation_identity =
        activate_test_confirmation_authority(&fixture.store, TEST_SIGNING_SEED).unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(1,'execass',1,'delegation-control-install','delegation-control-user','delegation-control-host',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('delegation-control-lease','execass',1,'delegation-control-host',1,1,9999999999999)",
        [],
    )
    .unwrap();
    drop(conn);
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("delegation-control-key")
        .unwrap();
    ControlFixture {
        fixture,
        integrity,
        key,
        redactor: ReceiptRedactor::new(&["delegation-control-secret"]).unwrap(),
        confirmation_identity,
    }
}

fn heads(f: &ControlFixture) -> (i64, Option<String>, i64, Option<String>) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.query_row(
        "SELECT j.receipt_count,j.receipt_head_digest,d.receipt_chain_count,d.receipt_chain_head_digest FROM execass_receipt_journal_state j JOIN execass_delegations d ON d.delegation_id='delegation-1' WHERE j.singleton=1",
        [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    )
    .unwrap()
}

fn receipt(
    f: &ControlFixture,
    suffix: &str,
    next_revision: i64,
    event_id: &str,
    now: i64,
    drain_actor: bool,
) -> AppendReceiptCommand {
    let (global_count, global_head, delegation_count, delegation_head) = heads(f);
    AppendReceiptCommand {
        receipt_id: format!("delegation-control-receipt-{suffix}"),
        transaction_id: format!("delegation-control-tx-{suffix}"),
        state_root_generation: 1,
        delegation_id: "delegation-1".into(),
        expected_state_revision: next_revision,
        expected_global_count: global_count,
        expected_global_head_digest: global_head,
        expected_delegation_count: delegation_count,
        expected_delegation_head_digest: delegation_head,
        receipt_kind: ReceiptKind::RunControl,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::Delegation,
            subject_id: "delegation-1".into(),
            revision: next_revision,
        },
        causation_id: format!("delegation-control-cause-{suffix}"),
        causation_event_id: event_id.into(),
        actor: if drain_actor {
            ReceiptActorBinding {
                actor_type: ActorType::Runtime,
                actor_identity: SafeText::new("execass-global-control", &[]).unwrap(),
                authority_provenance_id: "execass-global-control-carrier-authority".into(),
            }
        } else {
            ReceiptActorBinding {
                actor_type: ActorType::HumanLocal,
                actor_identity: SafeText::new("local-operator", &[]).unwrap(),
                authority_provenance_id: "authority-1".into(),
            }
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "delegation-control-host".into(),
            fencing_token: 1,
        },
        key: f.key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::summary("delegation run control changed", &[]).unwrap(),
        occurred_at: now,
        committed_at: now,
    }
}

fn payload(
    operation: &str,
    status: &DelegationRunControlStatus,
    next_revision: i64,
    next_control: RunControlState,
    next_epoch: i64,
    drain_state: DelegationStopDrainState,
) -> String {
    serde_json::json!({
        "operation": operation,
        "delegation_id": status.delegation_id,
        "phase": status.phase.as_str(),
        "run_control": next_control.as_str(),
        "state_revision": next_revision,
        "current_plan_revision": status.current_plan_revision,
        "stop_epoch": next_epoch,
        "policy_revision": status.policy_revision,
        "drain_state": drain_state.as_str(),
        "unresolved_external_effects_digest": status.unresolved_external_effects_digest,
    })
    .to_string()
}

fn stop(f: &ControlFixture, suffix: &str, now: i64) -> RequestDelegationStopCommand {
    let status = f
        .fixture
        .store
        .read_delegation_run_control_status("delegation-1", now)
        .unwrap()
        .unwrap();
    let next_revision = status.state_revision + 1;
    let event_id = format!("delegation-stop-event-{suffix}");
    let receipt = receipt(f, suffix, next_revision, &event_id, now, false);
    let executing = {
        let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
        conn.query_row(
            "SELECT (SELECT COUNT(*) FROM execass_continuations WHERE delegation_id='delegation-1' AND status='executing') + (SELECT COUNT(*) FROM execass_action_branches WHERE delegation_id='delegation-1' AND status='executing')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    let outbox_event = NewOutboxEvent {
        event_id,
        event_name: OutboxEventName::DelegationTransitioned,
        aggregate_id: "delegation-1".into(),
        aggregate_revision: next_revision,
        correlation_id: format!("delegation-stop-correlation-{suffix}"),
        causation_id: receipt.causation_id.clone(),
        occurred_at: now,
        safe_payload_json: payload(
            "stop_requested",
            &status,
            next_revision,
            RunControlState::StopRequested,
            status.stop_epoch + 1,
            if executing == 0 {
                DelegationStopDrainState::ReadyToStop
            } else {
                DelegationStopDrainState::Draining
            },
        ),
        duplicate_identity: format!("delegation-stop-idem-{suffix}"),
    };
    let attestation = sign_attestation(
        f,
        RunControlAttestationPayload {
            actor_type: ProtocolActorType::HumanLocal,
            credential_identity: f
                .confirmation_identity
                .local_credential_identity()
                .to_string(),
            authenticated_ingress: "native-control".into(),
            channel_assurance: "interactive-local".into(),
            request_correlation_id: outbox_event.correlation_id.clone(),
            source_message_id: None,
            provider_event_id: None,
            operation: RunControlOperation::DelegationStop,
            target: RunControlTarget::Delegation {
                delegation_id: "delegation-1".into(),
            },
            idempotency_key: outbox_event.duplicate_identity.clone(),
            replay_identity: format!("delegation-stop-replay-{suffix}"),
            observed_at_ms: now,
            issued_at_ms: now,
            stopped_epoch: status.stop_epoch,
            policy_revision: status.policy_revision,
            unresolved_effect_disclosure_digest: status.unresolved_external_effects_digest.clone(),
            delegation_state_revision: Some(status.state_revision),
            current_plan_revision: status.current_plan_revision,
            canonical_root_identity: f
                .confirmation_identity
                .canonical_root_identity()
                .to_string(),
            installation_identity: f.confirmation_identity.installation_identity().to_string(),
            os_user_identity_digest: f
                .confirmation_identity
                .os_user_identity_digest()
                .to_string(),
            state_root_generation: i64::try_from(f.confirmation_identity.state_root_generation())
                .unwrap(),
            signer_key_generation: i64::try_from(f.confirmation_identity.key_generation()).unwrap(),
        },
    );
    RequestDelegationStopCommand {
        delegation_id: "delegation-1".into(),
        expected_state_revision: status.state_revision,
        expected_stop_epoch: status.stop_epoch,
        expected_plan_revision: status.current_plan_revision,
        expected_policy_revision: status.policy_revision,
        disclosed_unresolved_external_effects_digest: status.unresolved_external_effects_digest,
        attestation,
        trusted_now: now,
        outbox_event,
        receipt,
    }
}

fn drain(f: &ControlFixture, suffix: &str, now: i64) -> CompleteDelegationStopDrainCommand {
    let status = f
        .fixture
        .store
        .read_delegation_run_control_status("delegation-1", now)
        .unwrap()
        .unwrap();
    let next_revision = status.state_revision + 1;
    let event_id = format!("delegation-drain-event-{suffix}");
    let receipt = receipt(f, suffix, next_revision, &event_id, now, true);
    CompleteDelegationStopDrainCommand {
        delegation_id: "delegation-1".into(),
        expected_state_revision: status.state_revision,
        expected_stop_epoch: status.stop_epoch,
        trusted_now: now,
        outbox_event: NewOutboxEvent {
            event_id,
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: next_revision,
            correlation_id: format!("delegation-drain-correlation-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at: now,
            safe_payload_json: payload(
                "stopped",
                &status,
                next_revision,
                RunControlState::Stopped,
                status.stop_epoch,
                DelegationStopDrainState::Stopped,
            ),
            duplicate_identity: format!("delegation-drain-idem-{suffix}"),
        },
        receipt,
    }
}

fn resume(f: &ControlFixture, suffix: &str, now: i64) -> ResumeDelegationCommand {
    let status = f
        .fixture
        .store
        .read_delegation_run_control_status("delegation-1", now)
        .unwrap()
        .unwrap();
    let next_revision = status.state_revision + 1;
    let event_id = format!("delegation-resume-event-{suffix}");
    let receipt = receipt(f, suffix, next_revision, &event_id, now, false);
    let outbox_event = NewOutboxEvent {
        event_id,
        event_name: OutboxEventName::DelegationTransitioned,
        aggregate_id: "delegation-1".into(),
        aggregate_revision: next_revision,
        correlation_id: format!("delegation-resume-correlation-{suffix}"),
        causation_id: receipt.causation_id.clone(),
        occurred_at: now,
        safe_payload_json: payload(
            "resumed",
            &status,
            next_revision,
            RunControlState::Running,
            status.stop_epoch + 1,
            DelegationStopDrainState::Running,
        ),
        duplicate_identity: format!("delegation-resume-idem-{suffix}"),
    };
    let payload = RunControlAttestationPayload {
        actor_type: ProtocolActorType::HumanLocal,
        credential_identity: f
            .confirmation_identity
            .local_credential_identity()
            .to_string(),
        authenticated_ingress: "native-control".into(),
        channel_assurance: "interactive-local".into(),
        request_correlation_id: outbox_event.correlation_id.clone(),
        source_message_id: None,
        provider_event_id: None,
        operation: RunControlOperation::DelegationResume,
        target: RunControlTarget::Delegation {
            delegation_id: "delegation-1".into(),
        },
        idempotency_key: outbox_event.duplicate_identity.clone(),
        replay_identity: format!("delegation-resume-replay-{suffix}"),
        observed_at_ms: now,
        issued_at_ms: now,
        stopped_epoch: status.stop_epoch,
        policy_revision: status.policy_revision,
        unresolved_effect_disclosure_digest: status.unresolved_external_effects_digest.clone(),
        delegation_state_revision: Some(status.state_revision),
        current_plan_revision: status.current_plan_revision,
        canonical_root_identity: f
            .confirmation_identity
            .canonical_root_identity()
            .to_string(),
        installation_identity: f.confirmation_identity.installation_identity().to_string(),
        os_user_identity_digest: f
            .confirmation_identity
            .os_user_identity_digest()
            .to_string(),
        state_root_generation: i64::try_from(f.confirmation_identity.state_root_generation())
            .unwrap(),
        signer_key_generation: i64::try_from(f.confirmation_identity.key_generation()).unwrap(),
    };
    ResumeDelegationCommand {
        delegation_id: "delegation-1".into(),
        expected_state_revision: status.state_revision,
        expected_plan_revision: status.current_plan_revision,
        expected_stop_epoch: status.stop_epoch,
        expected_policy_revision: status.policy_revision,
        disclosed_unresolved_external_effects_digest: status.unresolved_external_effects_digest,
        attestation: sign_attestation(f, payload),
        trusted_now: now,
        outbox_event,
        receipt,
    }
}

fn sign_attestation(
    f: &ControlFixture,
    payload: RunControlAttestationPayload,
) -> RunControlAttestation {
    let key_id = f.confirmation_identity.key_id().to_string();
    let bytes = run_control_attestation_signing_bytes(&payload, &key_id).unwrap();
    let signature = SigningKey::from_bytes(&TEST_SIGNING_SEED).sign(&bytes);
    RunControlAttestation {
        payload,
        key_id,
        signature_hex: signature
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn resign(f: &ControlFixture, command: &mut ResumeDelegationCommand) {
    command.attestation = sign_attestation(f, command.attestation.payload.clone());
}

fn resign_stop(f: &ControlFixture, command: &mut RequestDelegationStopCommand) {
    command.attestation = sign_attestation(f, command.attestation.payload.clone());
}

fn set_executing(f: &ControlFixture, executing: bool) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let status = if executing { "executing" } else { "waiting" };
    conn.execute(
        "UPDATE execass_action_branches SET status=?1,updated_at=?2 WHERE action_id='action-1'",
        rusqlite::params![status, NOW],
    )
    .unwrap();
}

fn snapshot(f: &ControlFixture, now: i64) -> (i64, i64, i64, i64, RunControlState) {
    let status = f
        .fixture
        .store
        .read_delegation_run_control_status("delegation-1", now)
        .unwrap()
        .unwrap();
    (
        table_count(&f.fixture.paths, "execass_run_control_attestations"),
        table_count(&f.fixture.paths, "execass_outbox_events"),
        table_count(&f.fixture.paths, "execass_receipts"),
        status.stop_epoch,
        status.run_control,
    )
}

#[test]
fn stop_drain_signed_resume_and_restart_replay_preserve_phase_and_do_not_create_work() {
    let f = setup();
    let initial_continuations = table_count(&f.fixture.paths, "execass_continuations");
    set_executing(&f, true);
    let stop = stop(&f, "roundtrip-stop", NOW + 10);
    let stopped = f
        .fixture
        .store
        .request_delegation_stop_atomically(&f.integrity, &f.redactor, &stop)
        .unwrap();
    assert!(
        matches!(stopped, DelegationRunControlMutationOutcome::StopRequested(ref status) if status.drain_state == DelegationStopDrainState::Draining && status.stop_epoch == 1)
    );
    let connection = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let stop_actor: (String, String) = connection
        .query_row(
            "SELECT actor_type,actor_authority_provenance_id FROM execass_receipts WHERE causation_event_id=?1",
            [stop.outbox_event.event_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stop_actor.0, "human_local");
    assert!(stop_actor.1.starts_with("run-control:"));
    assert_eq!(stop_actor.1.len(), "run-control:".len() + 64);
    assert_eq!(
        table_count(&f.fixture.paths, "execass_run_control_attestations"),
        1
    );
    drop(connection);
    assert!(f
        .fixture
        .store
        .complete_delegation_stop_drain_atomically(
            &f.integrity,
            &f.redactor,
            &drain(&f, "unsafe", NOW + 20),
        )
        .is_err());
    set_executing(&f, false);
    let drained = drain(&f, "roundtrip-drain", NOW + 30);
    assert!(matches!(
        f.fixture
            .store
            .complete_delegation_stop_drain_atomically(&f.integrity, &f.redactor, &drained)
            .unwrap(),
        DelegationRunControlMutationOutcome::Drained(ref status)
          if status.run_control == RunControlState::Stopped
          && status.phase == DelegationPhase::InMotion
    ));
    let resume = resume(&f, "roundtrip-resume", NOW + 40);
    assert!(matches!(
        f.fixture
            .store
            .resume_delegation_atomically(&f.integrity, &f.redactor, &resume)
            .unwrap(),
        DelegationRunControlMutationOutcome::Resumed(ref status)
          if status.run_control == RunControlState::Running
          && status.stop_epoch == 2
          && status.phase == DelegationPhase::InMotion
    ));
    assert_eq!(
        table_count(&f.fixture.paths, "execass_continuations"),
        initial_continuations
    );
    let committed = snapshot(&f, NOW + 41);
    assert!(matches!(
        f.fixture
            .store
            .resume_delegation_atomically(&f.integrity, &f.redactor, &resume)
            .unwrap(),
        DelegationRunControlMutationOutcome::Replayed(_)
    ));
    let reopened = ExecAssStore::open(&f.fixture.paths).unwrap();
    assert!(matches!(
        reopened
            .replay_delegation_resume_attestation("delegation-1", &resume.attestation, NOW + 50,)
            .unwrap(),
        Some(DelegationRunControlMutationOutcome::Replayed(_))
    ));
    assert_eq!(snapshot(&f, NOW + 51), committed);
}

#[test]
fn stale_heads_outbox_collision_policy_effect_and_signature_tamper_are_atomic() {
    let f = setup();

    let mut wrong_target = stop(&f, "wrong-target", NOW + 5);
    wrong_target.attestation.payload.target = RunControlTarget::Delegation {
        delegation_id: "different-delegation".into(),
    };
    resign_stop(&f, &mut wrong_target);
    let wrong_target_before = snapshot(&f, NOW + 5);
    assert!(f
        .fixture
        .store
        .request_delegation_stop_atomically(&f.integrity, &f.redactor, &wrong_target)
        .is_err());
    assert_eq!(snapshot(&f, NOW + 6), wrong_target_before);

    let mut stale_head = stop(&f, "stale-head", NOW + 10);
    stale_head.receipt.expected_global_count += 1;
    let before = snapshot(&f, NOW + 10);
    assert!(f
        .fixture
        .store
        .request_delegation_stop_atomically(&f.integrity, &f.redactor, &stale_head)
        .is_err());
    assert_eq!(snapshot(&f, NOW + 11), before);

    let collision = stop(&f, "collision", NOW + 20);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.summary.changed','delegation-1',99,'other','other',1,'v1','{}','delegation-control-other')",
        [collision.outbox_event.event_id.as_str()],
    )
    .unwrap();
    drop(conn);
    let collision_before = snapshot(&f, NOW + 20);
    assert!(f
        .fixture
        .store
        .request_delegation_stop_atomically(&f.integrity, &f.redactor, &collision)
        .is_err());
    assert_eq!(snapshot(&f, NOW + 21), collision_before);

    let live = stop(&f, "live", NOW + 30);
    f.fixture
        .store
        .request_delegation_stop_atomically(&f.integrity, &f.redactor, &live)
        .unwrap();
    let drain = drain(&f, "live-drain", NOW + 40);
    f.fixture
        .store
        .complete_delegation_stop_drain_atomically(&f.integrity, &f.redactor, &drain)
        .unwrap();
    let original = resume(&f, "hostile", NOW + 50);

    let mut forged = original.clone();
    forged.attestation.signature_hex.replace_range(
        0..2,
        if forged.attestation.signature_hex.starts_with("00") {
            "01"
        } else {
            "00"
        },
    );
    let stopped_snapshot = snapshot(&f, NOW + 50);
    assert!(f
        .fixture
        .store
        .resume_delegation_atomically(&f.integrity, &f.redactor, &forged)
        .is_err());
    assert_eq!(snapshot(&f, NOW + 51), stopped_snapshot);

    let mut policy = original.clone();
    policy.attestation.payload.policy_revision += 1;
    resign(&f, &mut policy);
    assert!(f
        .fixture
        .store
        .resume_delegation_atomically(&f.integrity, &f.redactor, &policy)
        .is_err());

    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_logical_effects(logical_effect_id,delegation_id,continuation_id,action_kind,state,internal_idempotency_key,manifest_digest,payload_digest,created_at,updated_at) VALUES('delegation-unresolved-effect','delegation-1','continuation-1','public_or_externally_consequential_communication','outcome_unknown','delegation-effect-key','manifest','payload',?1,?1)",
        [NOW + 52],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        f.fixture
            .store
            .resume_delegation_atomically(&f.integrity, &f.redactor, &original)
            .unwrap(),
        DelegationRunControlMutationOutcome::Stale(_)
    ));
    assert_eq!(snapshot(&f, NOW + 53).4, RunControlState::Stopped);
}

#[test]
fn concurrent_stop_requests_have_one_winner_and_stop_epoch_blocks_old_claim_identity() {
    let f = setup();
    let first = stop(&f, "race-a", NOW + 10);
    let mut second = first.clone();
    second.outbox_event.event_id = "delegation-stop-event-race-b".into();
    second.outbox_event.correlation_id = "delegation-stop-correlation-race-b".into();
    second.outbox_event.duplicate_identity = "delegation-stop-idem-race-b".into();
    second.receipt.receipt_id = "delegation-control-receipt-race-b".into();
    second.receipt.transaction_id = "delegation-control-tx-race-b".into();
    second.receipt.causation_event_id = second.outbox_event.event_id.clone();
    second.receipt.causation_id = "delegation-control-cause-race-b".into();
    second.outbox_event.causation_id = second.receipt.causation_id.clone();
    second.attestation.payload.request_correlation_id = second.outbox_event.correlation_id.clone();
    second.attestation.payload.idempotency_key = second.outbox_event.duplicate_identity.clone();
    second.attestation.payload.replay_identity = "delegation-stop-replay-race-b".into();
    resign_stop(&f, &mut second);

    let store_a = f.fixture.store.clone();
    let store_b = f.fixture.store.clone();
    let integrity_a = f.integrity.clone();
    let integrity_b = f.integrity.clone();
    let redactor_a = f.redactor.clone();
    let redactor_b = f.redactor.clone();
    let a = thread::spawn(move || {
        store_a.request_delegation_stop_atomically(&integrity_a, &redactor_a, &first)
    });
    let b = thread::spawn(move || {
        store_b.request_delegation_stop_atomically(&integrity_b, &redactor_b, &second)
    });
    let outcomes = [a.join().unwrap().unwrap(), b.join().unwrap().unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                DelegationRunControlMutationOutcome::StopRequested(_)
            ))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, DelegationRunControlMutationOutcome::Stale(_)))
            .count(),
        1
    );
    let status = f
        .fixture
        .store
        .read_delegation_run_control_status("delegation-1", NOW + 20)
        .unwrap()
        .unwrap();
    assert_eq!(status.run_control, RunControlState::StopRequested);
    assert_eq!(status.stop_epoch, 1);
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let old_epoch_claimable: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM execass_continuations c JOIN execass_delegations d ON d.delegation_id=c.delegation_id WHERE c.delegation_id='delegation-1' AND c.status='runnable' AND d.run_control='running' AND c.stop_epoch=d.stop_epoch",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        old_epoch_claimable, 0,
        "no post-stop claim may satisfy the live fence"
    );
}

#[test]
fn immutable_attestation_rejects_update_and_delete_after_resume() {
    let f = setup();
    f.fixture
        .store
        .request_delegation_stop_atomically(
            &f.integrity,
            &f.redactor,
            &stop(&f, "immutable-stop", NOW + 10),
        )
        .unwrap();
    f.fixture
        .store
        .complete_delegation_stop_drain_atomically(
            &f.integrity,
            &f.redactor,
            &drain(&f, "immutable-drain", NOW + 20),
        )
        .unwrap();
    let resume = resume(&f, "immutable-resume", NOW + 30);
    f.fixture
        .store
        .resume_delegation_atomically(&f.integrity, &f.redactor, &resume)
        .unwrap();
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            "UPDATE execass_run_control_attestations SET policy_revision=2 WHERE replay_identity=?1",
            [resume.attestation.payload.replay_identity.as_str()],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_run_control_attestations WHERE replay_identity=?1",
            [resume.attestation.payload.replay_identity.as_str()],
        )
        .is_err());
}
