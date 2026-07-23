use super::tests::{fixture, foundation, table_count, Fixture};
use super::*;
use crate::open_sqlite_connection;
use carsinos_protocol::execass::{
    run_control_attestation_signing_bytes, ActorType as ProtocolActorType, RunControlAttestation,
    RunControlAttestationPayload, RunControlOperation, RunControlTarget,
};
use ed25519_dalek::{Signer, SigningKey};
use std::time::Instant;

const NOW: i64 = 1_800_000_000_100;
const MAX_ATTESTATION_AGE_MS: i64 = 60_000;
const TEST_SIGNING_SEED: [u8; 32] = [41; 32];

struct StopFixture {
    fixture: Fixture,
    integrity: ReceiptIntegrityStore,
    key: ReceiptKeyRef,
    redactor: ReceiptRedactor,
    confirmation_identity: ConfirmationAuthorityIdentity,
}

fn setup() -> StopFixture {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    finish_setup(fixture)
}

fn setup_without_user_delegation() -> StopFixture {
    finish_setup(fixture())
}

fn finish_setup(fixture: Fixture) -> StopFixture {
    let confirmation_identity =
        activate_test_confirmation_authority(&fixture.store, TEST_SIGNING_SEED).unwrap();
    let conn = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(1,'execass',1,'stop-install','stop-user','stop-host',1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('stop-lease','execass',1,'stop-host',1,1,9999999999999)",
        [],
    ).unwrap();
    drop(conn);
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity.provision_initial_key("global-stop-key").unwrap();
    StopFixture {
        fixture,
        integrity,
        key,
        redactor: ReceiptRedactor::new(&["global-stop-test-secret"]).unwrap(),
        confirmation_identity,
    }
}

fn heads(f: &StopFixture) -> (i64, Option<String>, i64, Option<String>, i64) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.query_row(
        "SELECT j.receipt_count,j.receipt_head_digest,d.receipt_chain_count,d.receipt_chain_head_digest,d.state_revision FROM execass_receipt_journal_state j JOIN execass_delegations d ON d.delegation_id='execass-global-control-carrier' WHERE j.singleton=1",
        [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    ).unwrap()
}

fn payload(operation: &str, engaged: bool, epoch: i64, digest: &str) -> String {
    serde_json::json!({"operation":operation,"engaged":engaged,"global_stop_epoch":epoch,"drain_state":if engaged { "drained" } else { "running" },"current_policy_revision":1,"unresolved_external_effects_digest":digest}).to_string()
}

fn receipt(
    f: &StopFixture,
    suffix: &str,
    epoch: i64,
    event_id: &str,
    actor: &str,
) -> AppendReceiptCommand {
    let (gc, gh, dc, dh, revision) = heads(f);
    AppendReceiptCommand {
        receipt_id: format!("global-stop-receipt-{suffix}"),
        transaction_id: format!("global-stop-tx-{suffix}"),
        state_root_generation: 1,
        delegation_id: "execass-global-control-carrier".into(),
        expected_state_revision: revision,
        expected_global_count: gc,
        expected_global_head_digest: gh,
        expected_delegation_count: dc,
        expected_delegation_head_digest: dh,
        receipt_kind: ReceiptKind::GlobalStop,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::GlobalRuntimeControl,
            subject_id: "global-stop-all".into(),
            revision: epoch,
        },
        causation_id: format!("global-stop-cause-{suffix}"),
        causation_event_id: event_id.into(),
        actor: ReceiptActorBinding {
            actor_type: ActorType::HumanLocal,
            actor_identity: SafeText::new("local-operator", &[]).unwrap(),
            authority_provenance_id: actor.into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "stop-host".into(),
            fencing_token: 1,
        },
        key: f.key.clone(),
        rotation: None,
        evidence: vec![],
        redacted_summary: SafeText::summary("global stop state changed", &[]).unwrap(),
        occurred_at: NOW,
        committed_at: NOW,
    }
}

fn engage(f: &StopFixture, suffix: &str) -> EngageGlobalStopCommand {
    let status = f.fixture.store.global_stop_status().unwrap();
    let event_id = format!("global-stop-event-{suffix}");
    let receipt = receipt(
        f,
        suffix,
        status.global_stop_epoch + 1,
        &event_id,
        "authority-1",
    );
    EngageGlobalStopCommand {
        expected_global_stop_epoch: status.global_stop_epoch,
        trusted_now: NOW,
        outbox_event: NewOutboxEvent {
            event_id,
            event_name: OutboxEventName::GlobalStopChanged,
            aggregate_id: "global-stop-all".into(),
            aggregate_revision: status.global_stop_epoch + 1,
            correlation_id: format!("global-stop-corr-{suffix}"),
            causation_id: receipt.causation_id.clone(),
            occurred_at: NOW,
            safe_payload_json: payload(
                "engaged",
                true,
                status.global_stop_epoch + 1,
                &status.unresolved_external_effects_digest,
            ),
            duplicate_identity: format!("global-stop-idem-{suffix}"),
        },
        receipt,
    }
}

fn resume(f: &StopFixture, suffix: &str, epoch: i64, digest: &str) -> ResumeGlobalStopCommand {
    let event_id = format!("global-resume-event-{suffix}");
    let receipt = receipt(f, suffix, epoch, &event_id, "caller-value-is-ignored");
    let outbox_event = NewOutboxEvent {
        event_id,
        event_name: OutboxEventName::GlobalStopChanged,
        aggregate_id: "global-stop-all".into(),
        aggregate_revision: epoch,
        correlation_id: format!("global-resume-corr-{suffix}"),
        causation_id: receipt.causation_id.clone(),
        occurred_at: NOW,
        safe_payload_json: payload("resumed", false, epoch, digest),
        duplicate_identity: format!("global-resume-idem-{suffix}"),
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
        operation: RunControlOperation::GlobalResume,
        target: RunControlTarget::Global,
        idempotency_key: outbox_event.duplicate_identity.clone(),
        replay_identity: format!("global-resume-replay-{suffix}"),
        observed_at_ms: NOW,
        issued_at_ms: NOW,
        stopped_epoch: epoch,
        policy_revision: 1,
        unresolved_effect_disclosure_digest: digest.into(),
        delegation_state_revision: None,
        current_plan_revision: None,
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
    let attestation = sign_attestation(f, payload);
    ResumeGlobalStopCommand {
        expected_global_stop_epoch: epoch,
        expected_policy_revision: 1,
        disclosed_unresolved_external_effects_digest: digest.into(),
        attestation,
        trusted_now: NOW + 2,
        outbox_event,
        receipt,
    }
}

fn sign_attestation(
    f: &StopFixture,
    payload: RunControlAttestationPayload,
) -> RunControlAttestation {
    let key_id = f.confirmation_identity.key_id().to_string();
    let bytes = run_control_attestation_signing_bytes(&payload, &key_id).unwrap();
    let signature = SigningKey::from_bytes(&TEST_SIGNING_SEED).sign(&bytes);
    RunControlAttestation {
        payload,
        key_id,
        signature_hex: hex(&signature.to_bytes()),
    }
}

fn resign(f: &StopFixture, command: &mut ResumeGlobalStopCommand) {
    command.attestation = sign_attestation(f, command.attestation.payload.clone());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mutation_snapshot(f: &StopFixture) -> (i64, i64, i64, i64, bool) {
    (
        table_count(&f.fixture.paths, "execass_authority_provenance"),
        table_count(&f.fixture.paths, "execass_run_control_attestations"),
        table_count(&f.fixture.paths, "execass_outbox_events"),
        table_count(&f.fixture.paths, "execass_receipts"),
        f.fixture.store.global_stop_status().unwrap().engaged,
    )
}

fn assert_rejected_without_partial(f: &StopFixture, command: &ResumeGlobalStopCommand) {
    let before = mutation_snapshot(f);
    let outcome = f
        .fixture
        .store
        .resume_global_stop_atomically(&f.integrity, &f.redactor, command);
    assert!(
        outcome.is_err()
            || matches!(
                &outcome,
                Ok(GlobalStopMutationOutcome::Stale(_) | GlobalStopMutationOutcome::Conflict)
            ),
        "hostile command unexpectedly succeeded: {outcome:?}"
    );
    assert_eq!(mutation_snapshot(f), before);
}

fn insert_unresolved_effect(f: &StopFixture) {
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute(
        "INSERT INTO execass_logical_effects(logical_effect_id,delegation_id,continuation_id,action_kind,state,internal_idempotency_key,manifest_digest,payload_digest,created_at,updated_at) VALUES('global-stop-unresolved-effect','delegation-1','continuation-1','public_or_externally_consequential_communication','outcome_unknown','global-stop-effect-key','manifest','payload',?1,?1)",
        [NOW + 2],
    ).unwrap();
}

#[test]
fn trusted_fail_safe_engages_before_any_user_delegation_exists() {
    let f = setup_without_user_delegation();
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    let user_delegations: i64 = conn.query_row("SELECT COUNT(*) FROM execass_delegations WHERE delegation_id!='execass-global-control-carrier'", [], |row| row.get(0)).unwrap();
    assert_eq!(user_delegations, 0);
    drop(conn);
    let mut command = engage(&f, "empty");
    command.receipt.actor = ReceiptActorBinding {
        actor_type: ActorType::Runtime,
        actor_identity: SafeText::new("execass-global-control", &[]).unwrap(),
        authority_provenance_id: "execass-global-control-carrier-authority".into(),
    };
    assert!(
        matches!(f.fixture.store.engage_global_stop_atomically(&f.integrity, &f.redactor, &command).unwrap(), GlobalStopMutationOutcome::Engaged(status) if status.global_stop_epoch == 1)
    );
}

#[test]
fn ea313_global_stop_blocks_new_claims_within_locked_one_second_floor() {
    let f = setup();
    let command = engage(&f, "ea313-latency");
    let started = Instant::now();
    let outcome = f
        .fixture
        .store
        .engage_global_stop_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
    assert!(matches!(outcome, GlobalStopMutationOutcome::Engaged(_)));
    assert!(f.fixture.store.global_stop_status().unwrap().engaged);
    assert!(
        elapsed_ms <= 1_000.0,
        "global stop exceeded locked one-second claim-blocking floor: {elapsed_ms:.2}ms"
    );
    println!(
        "{}",
        serde_json::json!({
            "fixture": "execass.ea313.global-stop-latency.v1",
            "global_stop_ms": elapsed_ms,
            "engaged": true,
        })
    );
}

#[test]
fn engage_replay_resume_and_stale_disclosure_fail_closed() {
    let f = setup();
    let command = engage(&f, "first");
    assert!(matches!(
        f.fixture
            .store
            .engage_global_stop_atomically(&f.integrity, &f.redactor, &command)
            .unwrap(),
        GlobalStopMutationOutcome::Engaged(_)
    ));
    assert!(matches!(
        f.fixture
            .store
            .engage_global_stop_atomically(&f.integrity, &f.redactor, &command)
            .unwrap(),
        GlobalStopMutationOutcome::Replayed(_)
    ));
    let stopped = f.fixture.store.global_stop_status().unwrap();
    assert!(stopped.engaged);
    assert_eq!(stopped.global_stop_epoch, 1);
    assert_eq!(table_count(&f.fixture.paths, "execass_outbox_events"), 2);
    let stale = resume(
        &f,
        "stale",
        1,
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    assert!(matches!(
        f.fixture
            .store
            .resume_global_stop_atomically(&f.integrity, &f.redactor, &stale)
            .unwrap(),
        GlobalStopMutationOutcome::Stale(_)
    ));
    let old_disclosure = stopped.unresolved_external_effects_digest.clone();
    insert_unresolved_effect(&f);
    let changed_effects = f.fixture.store.global_stop_status().unwrap();
    assert_ne!(
        old_disclosure,
        changed_effects.unresolved_external_effects_digest
    );
    let stale_after_effect = resume(&f, "effect-mutation", 1, &old_disclosure);
    assert!(matches!(
        f.fixture
            .store
            .resume_global_stop_atomically(&f.integrity, &f.redactor, &stale_after_effect)
            .unwrap(),
        GlobalStopMutationOutcome::Stale(_)
    ));
    let mut stale = resume(
        &f,
        "current",
        1,
        &changed_effects.unresolved_external_effects_digest,
    );
    stale.outbox_event.safe_payload_json = payload(
        "resumed",
        false,
        1,
        &changed_effects.unresolved_external_effects_digest,
    );
    assert!(matches!(
        f.fixture
            .store
            .resume_global_stop_atomically(&f.integrity, &f.redactor, &stale)
            .unwrap(),
        GlobalStopMutationOutcome::Resumed(_)
    ));
    assert!(!f.fixture.store.global_stop_status().unwrap().engaged);
}

#[test]
fn outbox_collision_rolls_back_the_epoch_and_nonhuman_resume_is_rejected() {
    let f = setup();
    let command = engage(&f, "collision");
    let conn = open_sqlite_connection(&f.fixture.paths.db_path).unwrap();
    conn.execute("INSERT INTO execass_outbox_events(event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity) VALUES(?1,'execass.v1.global_stop.changed','global-stop-all',1,'other','other',1,'v1','{}','other')", [command.outbox_event.event_id.as_str()]).unwrap();
    assert!(f
        .fixture
        .store
        .engage_global_stop_atomically(&f.integrity, &f.redactor, &command)
        .is_err());
    assert_eq!(
        f.fixture
            .store
            .global_stop_status()
            .unwrap()
            .global_stop_epoch,
        0
    );
    drop(conn);
    let command = engage(&f, "human");
    f.fixture
        .store
        .engage_global_stop_atomically(&f.integrity, &f.redactor, &command)
        .unwrap();
    let stopped = f.fixture.store.global_stop_status().unwrap();
    let mut resume = resume(
        &f,
        "nonhuman",
        1,
        &stopped.unresolved_external_effects_digest,
    );
    for actor_type in [
        ActorType::Runtime,
        ActorType::Worker,
        ActorType::Connector,
        ActorType::Model,
    ] {
        resume.attestation.payload.actor_type = match actor_type {
            ActorType::Runtime => ProtocolActorType::Runtime,
            ActorType::Worker => ProtocolActorType::Worker,
            ActorType::Connector => ProtocolActorType::Connector,
            ActorType::Model => ProtocolActorType::Model,
            _ => unreachable!(),
        };
        assert!(f
            .fixture
            .store
            .resume_global_stop_atomically(&f.integrity, &f.redactor, &resume)
            .is_err());
    }
    assert!(f.fixture.store.global_stop_status().unwrap().engaged);
}

#[test]
fn signed_resume_exact_replay_collision_and_full_tamper_matrix_are_atomic() {
    let f = setup();
    f.fixture
        .store
        .engage_global_stop_atomically(&f.integrity, &f.redactor, &engage(&f, "matrix"))
        .unwrap();
    let stopped = f.fixture.store.global_stop_status().unwrap();
    let original = resume(
        &f,
        "matrix-resume",
        stopped.global_stop_epoch,
        &stopped.unresolved_external_effects_digest,
    );

    let mut hostile = original.clone();
    let replacement = if hostile.attestation.signature_hex.starts_with("00") {
        "01"
    } else {
        "00"
    };
    hostile
        .attestation
        .signature_hex
        .replace_range(0..2, replacement);
    assert_rejected_without_partial(&f, &hostile);

    let mut hostile = original.clone();
    hostile.attestation.payload.credential_identity = "attacker".into();
    resign(&f, &mut hostile);
    assert_rejected_without_partial(&f, &hostile);

    let mut hostile = original.clone();
    hostile.attestation.payload.operation = RunControlOperation::GlobalStop;
    resign(&f, &mut hostile);
    assert_rejected_without_partial(&f, &hostile);

    let mut hostile = original.clone();
    hostile.attestation.payload.observed_at_ms = NOW - 60_001;
    hostile.attestation.payload.issued_at_ms = NOW - 60_001;
    resign(&f, &mut hostile);
    assert_rejected_without_partial(&f, &hostile);

    let mut hostile = original.clone();
    hostile.attestation.payload.observed_at_ms = NOW + 5_001;
    hostile.attestation.payload.issued_at_ms = NOW + 5_001;
    resign(&f, &mut hostile);
    assert_rejected_without_partial(&f, &hostile);

    for mutation in 0..7 {
        let mut hostile = original.clone();
        match mutation {
            0 => hostile.attestation.payload.stopped_epoch += 1,
            1 => hostile.attestation.payload.policy_revision += 1,
            2 => {
                hostile
                    .attestation
                    .payload
                    .unresolved_effect_disclosure_digest = format!("sha256:{}", "a".repeat(64))
            }
            3 => {
                hostile.attestation.payload.canonical_root_identity =
                    format!("sha256:{}", "b".repeat(64))
            }
            4 => hostile.attestation.payload.installation_identity.push('x'),
            5 => hostile.attestation.payload.os_user_identity_digest = "c".repeat(64),
            6 => hostile.attestation.payload.signer_key_generation += 1,
            _ => unreachable!(),
        }
        resign(&f, &mut hostile);
        assert_rejected_without_partial(&f, &hostile);
    }

    let mut hostile = original.clone();
    hostile.outbox_event.safe_payload_json = "{}".into();
    assert_rejected_without_partial(&f, &hostile);

    let mut hostile = original.clone();
    hostile.receipt.expected_global_count += 1;
    assert_rejected_without_partial(&f, &hostile);

    let mut overflow = original.clone();
    let near_max = i64::MAX - MAX_ATTESTATION_AGE_MS + 1;
    overflow.attestation.payload.observed_at_ms = near_max;
    overflow.attestation.payload.issued_at_ms = near_max;
    overflow.trusted_now = near_max;
    overflow.receipt.occurred_at = near_max;
    overflow.receipt.committed_at = near_max;
    overflow.outbox_event.occurred_at = near_max;
    resign(&f, &mut overflow);
    assert_rejected_without_partial(&f, &overflow);

    assert!(matches!(
        f.fixture
            .store
            .resume_global_stop_atomically(&f.integrity, &f.redactor, &original,)
            .unwrap(),
        GlobalStopMutationOutcome::Resumed(_)
    ));
    let committed = mutation_snapshot(&f);
    let mut replay = original.clone();
    replay.trusted_now = NOW + MAX_ATTESTATION_AGE_MS + 100;
    assert!(matches!(
        f.fixture
            .store
            .resume_global_stop_atomically(&f.integrity, &f.redactor, &replay,)
            .unwrap(),
        GlobalStopMutationOutcome::Replayed(_)
    ));
    assert_eq!(mutation_snapshot(&f), committed);

    let reopened = ExecAssStore::open(&f.fixture.paths).unwrap();
    assert!(matches!(
        reopened
            .replay_global_resume_attestation(&replay.attestation)
            .unwrap(),
        Some(GlobalStopMutationOutcome::Replayed(_))
    ));
    let mut forged_replay = replay.attestation.clone();
    let replacement = if forged_replay.signature_hex.starts_with("00") {
        "01"
    } else {
        "00"
    };
    forged_replay.signature_hex.replace_range(0..2, replacement);
    assert!(reopened
        .replay_global_resume_attestation(&forged_replay)
        .is_err());
    let mut missing_replay = replay.attestation.clone();
    missing_replay.payload.replay_identity.push_str("-missing");
    assert!(reopened
        .replay_global_resume_attestation(&missing_replay)
        .unwrap()
        .is_none());

    let mut collision = original;
    collision
        .attestation
        .payload
        .request_correlation_id
        .push_str("-collision");
    resign(&f, &mut collision);
    assert_rejected_without_partial(&f, &collision);
}

#[test]
fn global_receipt_context_exposes_heads_and_live_fence_without_key_material() {
    let f = setup_without_user_delegation();
    let context = f
        .fixture
        .store
        .read_global_receipt_context(NOW)
        .unwrap()
        .unwrap();
    assert_eq!(context.carrier_state_revision, 1);
    assert_eq!(context.global_receipt_count, 0);
    assert_eq!(context.carrier_receipt_count, 0);
    assert_eq!(context.state_root_generation, 1);
    assert_eq!(context.runtime_host_generation, 1);
    assert_eq!(context.runtime_host_instance_id, "stop-host");
    assert_eq!(context.runtime_fencing_token, 1);
    assert_eq!(context.receipt_anchor_status, "uninitialized");
    assert!(context.global_receipt_head_digest.is_none());
    assert!(context.carrier_receipt_head_digest.is_none());
}
