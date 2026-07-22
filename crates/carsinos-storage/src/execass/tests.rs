use super::*;
use crate::{init, init_execass_fresh_root, AppPaths};
use carsinos_core::execass_actor::{
    issue_test_local_owner_authority, TestLocalOwnerAuthorityInput, VerifiedOwnerAuthority,
};
use carsinos_core::execass_danger::{
    bind_danger_admission, danger_admission_signing_bytes, issue_test_verified_danger_metadata,
    issue_test_verified_saved_routine_selector, match_known_danger, DangerAdmissionProof,
    DangerRoute, KnownDangerMatchInput, SignedDangerAdmissionProof, TestVerifiedDangerFact,
};
use carsinos_core::execass_manifest::{
    compile_dispatch, CanonicalLeafManifest, CanonicalValue, DispatchAction, DispatchNode,
    DispatchTree, ManifestCompilation, ResolvedLeafInput, ServerResolutionRegistry,
    TargetSnapshotInput, ToolIdentityInput,
};
use carsinos_core::execass_policy::{
    authorize_exact_owner_leaf, issue_test_objective_technical_validity_proof,
    ExactOwnerActionAuthority, ExactOwnerAuthorityInput, ExactOwnerAuthorityOutcome,
    TechnicalValidity,
};
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::{params, Connection};
use tempfile::TempDir;

pub(super) struct Fixture {
    _temp: TempDir,
    pub(super) paths: AppPaths,
    pub(super) store: ExecAssStore,
}

pub(super) fn prepared_attested_confirmation() -> (
    Fixture,
    ConfirmDangerousActionCommand,
    ConfirmationAttestation,
    i64,
) {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let present = present_confirmation("attested", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&present, &manifest, &route)
        .unwrap()
    else {
        panic!("attested fixture must present a challenge");
    };
    let command = ConfirmDangerousActionCommand {
        decision_id: challenge.decision_id.clone(),
        decision_revision: challenge.decision_revision,
        grant_id: "grant-attested".into(),
        selected_logical_action_id: "action-1".into(),
        response: DangerousActionConfirmationResponse::ConfirmAndContinue,
    };
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &present.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    let attestation = fixture
        .store
        .issue_legacy_test_confirmation_attestation(&command, &resolver, 1_800_000_000_020)
        .unwrap()
        .unwrap();
    (fixture, command, attestation, challenge.expires_at)
}

pub(super) fn prepared_saved_routine_grant(
    routine_id: &str,
    selector_json: &str,
) -> (Fixture, DispatchTree, String) {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let selector = issue_test_verified_saved_routine_selector(
        &manifest.leaves()[0],
        routine_id,
        1,
        selector_json,
    )
    .unwrap();
    let present = present_confirmation(
        "prepared-saved-routine",
        1_800_000_000_010,
        1_800_000_000_110,
    );
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_saved_routine_dangerous_action_confirmation(&present, &manifest, &route, &selector)
        .unwrap()
    else {
        panic!("saved routine fixture must present one challenge");
    };
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &present.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    let grant_id = format!("grant-{routine_id}");
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: challenge.decision_id,
                decision_revision: challenge.decision_revision,
                grant_id: grant_id.clone(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &resolver,
            1_800_000_000_020,
        )
        .unwrap();
    (fixture, dispatch, grant_id)
}

pub(super) fn prepared_combined_attested_confirmation() -> (
    Fixture,
    ConfirmDangerousActionCommand,
    ConfirmationAttestation,
    i64,
) {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let mut first = admitted_dispatch_with_verified_authority(authority.clone())
        .nodes
        .remove(0);
    first.node_id = "first".into();
    let mut second = first.clone();
    second.node_id = "second".into();
    let DispatchAction::ResolvedLeaf(second_action) = &mut second.action else {
        panic!("expected resolved leaf");
    };
    second_action.logical_action_id = "action-2".into();
    second_action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "second".into(),
            value: CanonicalValue::Bool(true),
        }]);
    second_action.target_snapshot.targets = vec![CanonicalValue::String("target-2".into())];
    let dispatch = DispatchTree {
        root_id: "root".into(),
        nodes: vec![
            DispatchNode {
                node_id: "root".into(),
                action: DispatchAction::Composite {
                    children: vec!["first".into(), "second".into()],
                },
            },
            first,
            second,
        ],
    };
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    let mut base = foundation();
    base.initial_continuation = None;
    fixture
        .store
        .admit_foundation_dispatch(
            &base,
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let routes = vec![
        protected_system_danger_route_for(&manifest, "action-1"),
        protected_system_danger_route_for(&manifest, "action-2"),
    ];
    let present = present_confirmation("combined-atomic", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_combined_dangerous_action_confirmation(&present, &manifest, &routes)
        .unwrap()
    else {
        panic!("combined attested fixture must present one challenge");
    };
    let command = ConfirmDangerousActionCommand {
        decision_id: challenge.decision_id.clone(),
        decision_revision: challenge.decision_revision,
        grant_id: "grant-combined-atomic".into(),
        selected_logical_action_id: "action-2".into(),
        response: DangerousActionConfirmationResponse::ConfirmAndContinue,
    };
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &present.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    let attestation = fixture
        .store
        .issue_legacy_test_confirmation_attestation(&command, &resolver, 1_800_000_000_020)
        .unwrap()
        .unwrap();
    (fixture, command, attestation, challenge.expires_at)
}

fn resign_test_attestation(attestation: &mut ConfirmationAttestation) {
    let key = SigningKey::from_bytes(&[42; 32]);
    let bytes = confirmation_attestation_signing_bytes(&attestation.payload, &attestation.key_id)
        .expect("mutated test payload remains structurally signable");
    attestation.signature_hex = key
        .sign(&bytes)
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
}

#[test]
fn attested_confirmation_projection_local_resolution_and_post_expiry_replay_are_exact() {
    let (fixture, command, attestation, expires_at) = prepared_attested_confirmation();
    let projection = fixture
        .store
        .read_pending_danger_confirmation_alternative_binding_at_for_test(
            &command.decision_id,
            &command.selected_logical_action_id,
            1_800_000_000_020,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        projection.exact_selected_action_digest,
        attestation.payload.selected_action_digest
    );
    assert_eq!(
        projection.declared_consequence_digest,
        attestation.payload.declared_consequence_digest
    );
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_attested_at_for_test(
                &command,
                &attestation,
                1_800_000_000_020,
            )
            .unwrap(),
        DangerConfirmationResolutionOutcome::Confirmed(_)
    ));
    let reopened = ExecAssStore::open(&fixture.paths).unwrap();
    assert!(matches!(
        reopened
            .confirm_dangerous_action_attested_at_for_test(
                &command,
                &attestation,
                expires_at + 10_000,
            )
            .unwrap(),
        DangerConfirmationResolutionOutcome::Replayed(_)
    ));
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_attestations"),
        1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
}

#[test]
fn runtime_projection_is_exact_pending_or_durable_resolved_only() {
    let (fixture, command, attestation, _expires_at) = prepared_attested_confirmation();
    let pending = fixture
        .store
        .read_danger_confirmation_runtime_projection(
            &command.decision_id,
            &command.selected_logical_action_id,
        )
        .unwrap();
    let Some(DangerConfirmationRuntimeProjection::Pending(pending)) = pending else {
        panic!("exact pending alternative was not projected");
    };
    let pending = *pending;
    assert_eq!(pending.decision_id, command.decision_id);
    assert_eq!(
        pending.selected_logical_action_id,
        command.selected_logical_action_id
    );
    for (decision_id, action_id) in [
        ("missing", command.selected_logical_action_id.as_str()),
        (command.decision_id.as_str(), "other-action"),
        ("", command.selected_logical_action_id.as_str()),
        (command.decision_id.as_str(), ""),
    ] {
        assert!(fixture
            .store
            .read_danger_confirmation_runtime_projection(decision_id, action_id)
            .unwrap()
            .is_none());
    }
    assert!(fixture
        .store
        .read_danger_confirmation_runtime_projection_at_for_test(
            &command.decision_id,
            &command.selected_logical_action_id,
            pending.expires_at,
        )
        .unwrap()
        .is_none());

    let confirmed = fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(&command, &attestation, 1_800_000_000_020)
        .unwrap();
    let DangerConfirmationResolutionOutcome::Confirmed(grant) = confirmed else {
        panic!("fixture confirmation did not create a grant");
    };
    let resolved = fixture
        .store
        .read_danger_confirmation_runtime_projection(
            &command.decision_id,
            &command.selected_logical_action_id,
        )
        .unwrap();
    let Some(DangerConfirmationRuntimeProjection::Resolved(resolved)) = resolved else {
        panic!("resolved confirmation was not projected");
    };
    assert_eq!(resolved.binding, pending);
    assert_eq!(resolved.grant, grant);
    assert!(fixture
        .store
        .read_danger_confirmation_runtime_projection(&command.decision_id, "other-action")
        .unwrap()
        .is_none());

    let reopened = ExecAssStore::open(&fixture.paths).unwrap();
    let Some(DangerConfirmationRuntimeProjection::Resolved(after_restart)) = reopened
        .read_danger_confirmation_runtime_projection(
            &command.decision_id,
            &command.selected_logical_action_id,
        )
        .unwrap()
    else {
        panic!("resolved projection was not stable after reopen");
    };
    assert_eq!(after_restart.binding, pending);
    assert_eq!(after_restart.grant, grant);
}

#[test]
fn attested_confirmation_signed_binding_mutations_leave_pending_state_unchanged() {
    let (fixture, command, attestation, expires_at) = prepared_attested_confirmation();
    let mut mutations = Vec::new();
    for mutate in 0..12 {
        let mut hostile = attestation.clone();
        match mutate {
            0 => hostile.payload.installation_identity = "other-install".into(),
            1 => hostile.payload.os_user_identity_digest = "b".repeat(64),
            2 => hostile.payload.state_root_generation += 1,
            3 => hostile.payload.credential_identity = "other-owner".into(),
            4 => hostile.payload.provider_event_id = Some("forged-event".into()),
            5 => hostile.payload.selected_logical_action_id = "other-action".into(),
            6 => hostile.payload.selected_action_digest = "b".repeat(64),
            7 => hostile.payload.declared_consequence_digest = "c".repeat(64),
            8 => hostile.payload.canonical_manifest_digest = "d".repeat(64),
            9 => hostile.payload.challenge_nonce_digest = "e".repeat(64),
            10 => hostile.payload.policy_revision += 1,
            11 => hostile.payload.issued_at_ms = 1_800_000_000_009,
            _ => unreachable!(),
        }
        if mutate != 4 {
            resign_test_attestation(&mut hostile);
        }
        mutations.push(hostile);
    }
    let mut bad_signature = attestation.clone();
    let replacement = if &bad_signature.signature_hex[0..2] == "00" {
        "ff"
    } else {
        "00"
    };
    bad_signature.signature_hex.replace_range(0..2, replacement);
    mutations.push(bad_signature);

    for hostile in mutations {
        assert!(fixture
            .store
            .confirm_dangerous_action_attested_at_for_test(&command, &hostile, 1_800_000_000_020,)
            .is_err());
        assert_eq!(
            table_count(&fixture.paths, "execass_confirmation_attestations"),
            0
        );
        assert_eq!(
            table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
            0
        );
    }
    let mut wrong_key_id = attestation.clone();
    wrong_key_id.key_id = "caller-nominated-key".into();
    resign_test_attestation(&mut wrong_key_id);
    assert!(fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(&command, &wrong_key_id, 1_800_000_000_020,)
        .is_err());
    assert!(fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(&command, &attestation, expires_at,)
        .is_err());
    let mut wrong_active_key = attestation.clone();
    let wrong_signer = SigningKey::from_bytes(&[24; 32]);
    let bytes =
        confirmation_attestation_signing_bytes(&wrong_active_key.payload, &wrong_active_key.key_id)
            .unwrap();
    wrong_active_key.signature_hex = wrong_signer
        .sign(&bytes)
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert!(fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &command,
            &wrong_active_key,
            1_800_000_000_020,
        )
        .is_err());
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_confirmation_challenges WHERE decision_id=?1",
            params![command.decision_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "pending"
    );
}

#[test]
fn attested_confirmation_allows_two_remote_providers_but_replay_is_provider_exact() {
    let (fixture, command, mut telegram, _) = prepared_attested_confirmation();
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    for (id, credential, ingress, assurance) in [
        (
            "remote-telegram",
            "telegram:owner",
            "telegram-adapter",
            "allowlisted-remote",
        ),
        (
            "remote-discord",
            "discord:owner",
            "discord-adapter",
            "allowlisted-remote",
        ),
    ] {
        conn.execute(
            "INSERT INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES (?1,'human_remote',?2,?3,?4,1,'active',1800000000019)",
            params![id, credential, ingress, assurance],
        )
        .unwrap();
    }
    telegram.payload.actor_type = "human_remote".into();
    telegram.payload.credential_identity = "telegram:owner".into();
    telegram.payload.authenticated_ingress = "telegram-adapter".into();
    telegram.payload.channel_assurance = "allowlisted-remote".into();
    telegram.payload.source_message_id = Some("telegram-message-1".into());
    telegram.payload.provider_event_id = Some("telegram-event-1".into());
    resign_test_attestation(&mut telegram);
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_attested_at_for_test(&command, &telegram, 1_800_000_000_020,)
            .unwrap(),
        DangerConfirmationResolutionOutcome::Confirmed(_)
    ));

    let mut discord = telegram.clone();
    discord.payload.credential_identity = "discord:owner".into();
    discord.payload.authenticated_ingress = "discord-adapter".into();
    discord.payload.source_message_id = Some("discord-message-1".into());
    discord.payload.provider_event_id = Some("discord-event-1".into());
    resign_test_attestation(&mut discord);
    assert!(fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(&command, &discord, 1_800_000_000_021,)
        .is_err());
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_attestations"),
        1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
}

pub(super) fn fixture() -> Fixture {
    let temp =
        tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).expect("create project-workspace fixture");
    let paths = AppPaths::from_root(temp.path().join("state"));
    init_execass_fresh_root(&paths).expect("initialize exact ExecAss schema");
    let store = ExecAssStore::open(&paths).expect("open exact ExecAss store");
    Fixture {
        _temp: temp,
        paths,
        store,
    }
}

pub(super) fn foundation() -> CreateFoundationCommand {
    let timestamp = 1_800_000_000_000;
    CreateFoundationCommand {
        write: WriteContext {
            idempotency_key: "idem-foundation-1".into(),
            correlation_id: "corr-foundation-1".into(),
            causation_id: "cause-foundation-1".into(),
            occurred_at: timestamp,
        },
        authority: AuthorityProvenanceRecord {
            authority_provenance_id: "authority-1".into(),
            actor_type: ActorType::HumanLocal,
            credential_identity: "local-operator".into(),
            authenticated_ingress: "native-control".into(),
            channel_assurance: "interactive-local".into(),
            source_correlation_id: "corr-foundation-1".into(),
            source_message_id: Some("message-1".into()),
            authority_kind: AuthorityKind::OriginalRequest,
            normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.into(),
            policy_revision: 1,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_digest: None,
            bound_challenge_nonce_digest: None,
            evidence_digest: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            created_at: timestamp,
            expires_at: None,
        },
        delegation: DelegationRecord {
            delegation_id: "delegation-1".into(),
            normalized_original_intent: "prepare the requested bounded result".into(),
            intake_evidence_json: r#"{"message_ref":"message-1"}"#.into(),
            ingress_source: "native-control".into(),
            ingress_credential_identity: "local-operator".into(),
            source_message_id: Some("message-1".into()),
            source_correlation_id: "corr-foundation-1".into(),
            ingress_idempotency_key: "idem-foundation-1".into(),
            classifier_version: "v1".into(),
            classifier_reasons_json: r#"["durable_work"]"#.into(),
            phase: DelegationPhase::InMotion,
            run_control: RunControlState::Running,
            state_revision: 1,
            current_plan_revision: Some(1),
            current_criteria_revision: Some(1),
            policy_revision: 1,
            effective_authority_json: r#"{"profile":"balanced"}"#.into(),
            authority_provenance_id: "authority-1".into(),
            pending_decision_id: None,
            external_wait_json: None,
            stop_epoch: 0,
            completion_assessment_json: None,
            receipt_chain_count: 0,
            receipt_chain_head_digest: None,
            created_at: timestamp,
            updated_at: timestamp,
            acknowledged_at: None,
            terminal_at: None,
        },
        plan: PlanRecord {
            plan_id: "plan-1".into(),
            delegation_id: "delegation-1".into(),
            plan_revision: 1,
            based_on_delegation_revision: 1,
            policy_revision: 1,
            plan_summary: "perform bounded work".into(),
            resolved_leaf_manifest_json: r#"[{"action":"read"}]"#.into(),
            manifest_digest: "sha256:manifest".into(),
            created_by_authority_provenance_id: "authority-1".into(),
            created_at: timestamp,
        },
        outcome_criteria: vec![
            OutcomeCriterionRecord {
                criterion_id: "criterion-z".into(),
                delegation_id: "delegation-1".into(),
                criteria_revision: 1,
                criterion_key: "z-result".into(),
                description: "result exists".into(),
                material: true,
                verifier_type: VerifierType::Artifact,
                expected_predicate_json: r#"{"exists":true}"#.into(),
                authoritative_source_kind: "artifact".into(),
                created_at: timestamp,
            },
            OutcomeCriterionRecord {
                criterion_id: "criterion-a".into(),
                delegation_id: "delegation-1".into(),
                criteria_revision: 1,
                criterion_key: "a-safety".into(),
                description: "scope preserved".into(),
                material: true,
                verifier_type: VerifierType::DatabasePredicate,
                expected_predicate_json: r#"{"unexpected_rows":0}"#.into(),
                authoritative_source_kind: "database".into(),
                created_at: timestamp,
            },
        ],
        initial_continuation: Some(ContinuationRecord {
            continuation_id: "continuation-1".into(),
            delegation_id: "delegation-1".into(),
            target_delegation_revision: 1,
            target_plan_revision: 1,
            action_id: "action-1".into(),
            branch_kind: ActionBranchKind::Ordinary,
            causation_kind: ContinuationCausationKind::Intake,
            causation_id: "cause-foundation-1".into(),
            status: ContinuationStatus::Runnable,
            job_id: None,
            lease_owner: None,
            lease_expires_at: None,
            fencing_token: 0,
            host_generation: 1,
            stop_epoch: 0,
            global_stop_epoch: 0,
            created_at: timestamp,
            updated_at: timestamp,
            completed_at: None,
        }),
        outbox_event: NewOutboxEvent {
            event_id: "event-foundation-1".into(),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 1,
            correlation_id: "corr-foundation-1".into(),
            causation_id: "cause-foundation-1".into(),
            occurred_at: timestamp,
            safe_payload_json: r#"{"summary":"delegation accepted"}"#.into(),
            duplicate_identity: "idem-foundation-1".into(),
        },
    }
}

fn foundation_occurrence(suffix: &str) -> CreateFoundationCommand {
    let mut command = foundation();
    let delegation_id = format!("delegation-{suffix}");
    command.write.idempotency_key = format!("idem-foundation-{suffix}");
    command.write.correlation_id = format!("corr-foundation-{suffix}");
    command.write.causation_id = format!("cause-foundation-{suffix}");
    command.delegation.delegation_id = delegation_id.clone();
    command.delegation.ingress_idempotency_key = command.write.idempotency_key.clone();
    command.delegation.source_correlation_id = command.write.correlation_id.clone();
    command.plan.plan_id = format!("plan-{suffix}");
    command.plan.delegation_id = delegation_id.clone();
    for (index, criterion) in command.outcome_criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("criterion-{suffix}-{index}");
        criterion.delegation_id = delegation_id.clone();
    }
    command.initial_continuation = None;
    command.outbox_event.event_id = format!("event-foundation-{suffix}");
    command.outbox_event.aggregate_id = delegation_id;
    command.outbox_event.correlation_id = command.write.correlation_id.clone();
    command.outbox_event.causation_id = command.write.causation_id.clone();
    command.outbox_event.duplicate_identity = command.write.idempotency_key.clone();
    command
}

fn cas(expected_revision: i64, new_revision: i64, suffix: &str) -> CasDelegationStateCommand {
    let timestamp = 1_800_000_000_000 + new_revision;
    CasDelegationStateCommand {
        write: WriteContext {
            idempotency_key: format!("idem-cas-{suffix}"),
            correlation_id: format!("corr-cas-{suffix}"),
            causation_id: format!("cause-cas-{suffix}"),
            occurred_at: timestamp,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: expected_revision,
        new_state_revision: new_revision,
        phase: DelegationPhase::WaitingExternal,
        run_control: RunControlState::Running,
        pending_decision_id: None,
        external_wait_json: Some(r#"{"dependency":"provider"}"#.into()),
        updated_at: timestamp,
        terminal_at: None,
        outbox_event: NewOutboxEvent {
            event_id: format!("event-cas-{suffix}"),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: new_revision,
            correlation_id: format!("corr-cas-{suffix}"),
            causation_id: format!("cause-cas-{suffix}"),
            occurred_at: timestamp,
            safe_payload_json: r#"{"summary":"waiting externally"}"#.into(),
            duplicate_identity: format!("idem-cas-{suffix}"),
        },
    }
}

pub(super) fn table_count(paths: &AppPaths, table: &str) -> i64 {
    let conn = Connection::open(&paths.db_path).expect("open fixture db");
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .expect("count table")
}

fn assert_zero_execass_runtime_rows(paths: &AppPaths) {
    for table in [
        "execass_authority_provenance",
        "execass_delegations",
        "execass_plans",
        "execass_action_branches",
        "execass_continuations",
        "execass_logical_effects",
        "execass_outbox_events",
        "execass_receipts",
    ] {
        assert_eq!(
            table_count(paths, table),
            seeded_global_control_rows(table),
            "unexpected row in {table}"
        );
    }
}

fn assert_foundation_tables_empty(paths: &AppPaths) {
    for table in [
        "execass_authority_provenance",
        "execass_delegations",
        "execass_plans",
        "execass_outcome_criteria",
        "execass_continuations",
        "execass_outbox_events",
    ] {
        assert_eq!(
            table_count(paths, table),
            seeded_global_control_rows(table),
            "unexpected {table} row"
        );
    }
}

fn seeded_global_control_rows(table: &str) -> i64 {
    match table {
        "execass_authority_provenance" | "execass_delegations" => 1,
        _ => 0,
    }
}

#[test]
fn exact_replay_after_state_advancement_returns_the_current_bundle() {
    let fixture = fixture();
    let command = foundation();
    assert!(matches!(
        fixture.store.create_foundation(&command).unwrap(),
        FoundationWriteOutcome::Created(_)
    ));
    fixture
        .store
        .compare_and_swap_delegation_state(&cas(1, 2, "advance"))
        .unwrap();

    let FoundationWriteOutcome::Replayed(current) =
        fixture.store.create_foundation(&command).unwrap()
    else {
        panic!("progressed exact intake was not replayed");
    };
    assert_eq!(current.delegation.state_revision, 2);
    assert_eq!(current.delegation.phase, DelegationPhase::WaitingExternal);
    assert_eq!(current.outbox_events.len(), 2);

    let mut conflict = command;
    conflict.plan.plan_summary = "different immutable plan".into();
    assert_eq!(
        fixture.store.create_foundation(&conflict).unwrap(),
        FoundationWriteOutcome::Conflict {
            existing_delegation_id: Some("delegation-1".into())
        }
    );
    assert_eq!(
        fixture
            .store
            .read_foundation("delegation-1")
            .unwrap()
            .unwrap()
            .delegation
            .state_revision,
        2
    );
}

#[test]
fn raw_intake_continuation_promotion_is_rejected_without_claim_receipt() {
    let fixture = fixture();
    let command = foundation();
    fixture.store.create_foundation(&command).unwrap();

    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            r#"
        UPDATE execass_continuations
        SET status = 'executing', lease_owner = 'worker-1', lease_expires_at = ?1,
            fencing_token = 2, updated_at = ?2
        WHERE continuation_id = 'continuation-1'
        "#,
            params![1_800_000_001_000i64, 1_800_000_000_500i64],
        )
        .is_err());
    drop(conn);

    let FoundationWriteOutcome::Replayed(current) =
        fixture.store.create_foundation(&command).unwrap()
    else {
        panic!("rejected raw continuation promotion prevented exact replay");
    };
    let continuation = current.initial_continuation.unwrap();
    assert_eq!(continuation.status, ContinuationStatus::Runnable);
    assert_eq!(continuation.lease_owner, None);
    assert_eq!(continuation.fencing_token, 0);
}

#[test]
fn criteria_replay_is_order_independent_but_member_set_exact() {
    for case in ["reordered", "omitted", "extra", "changed"] {
        let fixture = fixture();
        let original = foundation();
        fixture.store.create_foundation(&original).unwrap();
        let mut replay = original;
        match case {
            "reordered" => replay.outcome_criteria.reverse(),
            "omitted" => {
                replay.outcome_criteria.pop();
            }
            "extra" => {
                let mut extra = replay.outcome_criteria[0].clone();
                extra.criterion_id = "criterion-extra".into();
                extra.criterion_key = "m-extra".into();
                replay.outcome_criteria.push(extra);
            }
            "changed" => replay.outcome_criteria[0].description = "changed".into(),
            _ => unreachable!(),
        }
        let outcome = fixture.store.create_foundation(&replay).unwrap();
        if case == "reordered" {
            assert!(matches!(outcome, FoundationWriteOutcome::Replayed(_)));
        } else {
            assert_eq!(
                outcome,
                FoundationWriteOutcome::Conflict {
                    existing_delegation_id: Some("delegation-1".into())
                },
                "case {case}"
            );
        }
        assert_eq!(table_count(&fixture.paths, "execass_outcome_criteria"), 2);
    }
}

#[test]
fn initial_continuation_presence_and_absence_must_match_exactly() {
    let present = fixture();
    let original = foundation();
    present.store.create_foundation(&original).unwrap();
    let mut omitted = original;
    omitted.initial_continuation = None;
    assert!(matches!(
        present.store.create_foundation(&omitted).unwrap(),
        FoundationWriteOutcome::Conflict { .. }
    ));

    let absent = fixture();
    let mut original = foundation();
    let continuation = original.initial_continuation.take().unwrap();
    absent.store.create_foundation(&original).unwrap();
    let mut added = original;
    added.initial_continuation = Some(continuation);
    assert!(matches!(
        absent.store.create_foundation(&added).unwrap(),
        FoundationWriteOutcome::Conflict { .. }
    ));
}

#[test]
fn every_foundation_json_field_is_validated_before_sql_mutation() {
    for field in [
        "authority_scope",
        "intake_evidence",
        "classifier_reasons",
        "effective_authority",
        "external_wait",
        "completion_assessment",
        "plan_manifest",
        "criterion_predicate",
        "outbox_payload",
    ] {
        let fixture = fixture();
        let mut command = foundation();
        match field {
            "authority_scope" => command.authority.normalized_scope_json = "not-json".into(),
            "intake_evidence" => command.delegation.intake_evidence_json = "not-json".into(),
            "classifier_reasons" => command.delegation.classifier_reasons_json = "not-json".into(),
            "effective_authority" => {
                command.delegation.effective_authority_json = "not-json".into()
            }
            "external_wait" => command.delegation.external_wait_json = Some("not-json".into()),
            "completion_assessment" => {
                command.delegation.completion_assessment_json = Some("not-json".into())
            }
            "plan_manifest" => command.plan.resolved_leaf_manifest_json = "not-json".into(),
            "criterion_predicate" => {
                command.outcome_criteria[0].expected_predicate_json = "not-json".into()
            }
            "outbox_payload" => command.outbox_event.safe_payload_json = "not-json".into(),
            _ => unreachable!(),
        }
        assert!(
            fixture.store.create_foundation(&command).is_err(),
            "{field}"
        );
        assert_foundation_tables_empty(&fixture.paths);
    }
}

#[test]
fn every_cas_json_field_is_validated_before_sql_mutation() {
    for field in ["external_wait", "outbox_payload"] {
        let fixture = fixture();
        fixture.store.create_foundation(&foundation()).unwrap();
        let mut command = cas(1, 2, field);
        match field {
            "external_wait" => command.external_wait_json = Some("not-json".into()),
            "outbox_payload" => command.outbox_event.safe_payload_json = "not-json".into(),
            _ => unreachable!(),
        }
        assert!(fixture
            .store
            .compare_and_swap_delegation_state(&command)
            .is_err());
        let bundle = fixture
            .store
            .read_foundation("delegation-1")
            .unwrap()
            .unwrap();
        assert_eq!(bundle.delegation.state_revision, 1);
        assert_eq!(bundle.outbox_events.len(), 1);
    }
}

#[test]
fn every_foundation_semantic_binding_fails_closed_before_mutation() {
    for binding in [
        "authority_correlation",
        "delegation_correlation",
        "authority_policy",
        "plan_policy",
        "plan_based_on",
        "plan_authority",
        "criteria_revision",
        "continuation_causation",
        "outbox_aggregate",
        "outbox_revision",
        "outbox_correlation",
        "outbox_causation",
        "outbox_duplicate",
    ] {
        let fixture = fixture();
        let mut command = foundation();
        match binding {
            "authority_correlation" => command.authority.source_correlation_id = "other".into(),
            "delegation_correlation" => command.delegation.source_correlation_id = "other".into(),
            "authority_policy" => command.authority.policy_revision = 2,
            "plan_policy" => command.plan.policy_revision = 2,
            "plan_based_on" => command.plan.based_on_delegation_revision = 2,
            "plan_authority" => command.plan.created_by_authority_provenance_id = "other".into(),
            "criteria_revision" => command.outcome_criteria[0].criteria_revision = 2,
            "continuation_causation" => {
                command.initial_continuation.as_mut().unwrap().causation_id = "other".into()
            }
            "outbox_aggregate" => command.outbox_event.aggregate_id = "other".into(),
            "outbox_revision" => command.outbox_event.aggregate_revision = 2,
            "outbox_correlation" => command.outbox_event.correlation_id = "other".into(),
            "outbox_causation" => command.outbox_event.causation_id = "other".into(),
            "outbox_duplicate" => command.outbox_event.duplicate_identity = "other".into(),
            _ => unreachable!(),
        }
        assert!(
            fixture.store.create_foundation(&command).is_err(),
            "{binding}"
        );
        assert_foundation_tables_empty(&fixture.paths);
    }
}

#[test]
fn mid_bundle_sql_violation_rolls_back_every_touched_table() {
    let fixture = fixture();
    let mut command = foundation();
    command.outcome_criteria[1].criterion_key = command.outcome_criteria[0].criterion_key.clone();
    assert!(fixture.store.create_foundation(&command).is_err());
    assert_foundation_tables_empty(&fixture.paths);
}

#[test]
fn foundation_load_is_complete_and_deterministically_ordered() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    fixture
        .store
        .compare_and_swap_delegation_state(&cas(1, 2, "success"))
        .unwrap();
    let first = fixture
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap();
    let second = fixture
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .outcome_criteria
            .iter()
            .map(|criterion| criterion.criterion_key.as_str())
            .collect::<Vec<_>>(),
        ["a-safety", "z-result"]
    );
    assert_eq!(first.outbox_events.len(), 2);
    assert!(first.outbox_events[0].global_sequence < first.outbox_events[1].global_sequence);
}

#[test]
fn cas_distinguishes_stale_not_found_and_atomic_success() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    assert_eq!(
        fixture
            .store
            .compare_and_swap_delegation_state(&cas(0, 1, "stale"))
            .unwrap(),
        CasDelegationStateOutcome::Stale {
            current_state_revision: 1
        }
    );
    assert_eq!(table_count(&fixture.paths, "execass_outbox_events"), 1);

    let mut missing = cas(1, 2, "missing");
    missing.delegation_id = "delegation-missing".into();
    missing.outbox_event.aggregate_id = "delegation-missing".into();
    assert_eq!(
        fixture
            .store
            .compare_and_swap_delegation_state(&missing)
            .unwrap(),
        CasDelegationStateOutcome::NotFound
    );
    assert_eq!(table_count(&fixture.paths, "execass_outbox_events"), 1);

    let CasDelegationStateOutcome::Updated(updated) = fixture
        .store
        .compare_and_swap_delegation_state(&cas(1, 2, "updated"))
        .unwrap()
    else {
        panic!("CAS did not update");
    };
    assert_eq!(updated.delegation.state_revision, 2);
    assert_eq!(updated.outbox_event.event.aggregate_revision, 2);
    assert_eq!(table_count(&fixture.paths, "execass_outbox_events"), 2);
}

#[test]
fn cas_outbox_failure_rolls_back_the_delegation_update() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let mut command = cas(1, 2, "collision");
    command.outbox_event.event_id = "event-foundation-1".into();
    assert!(fixture
        .store
        .compare_and_swap_delegation_state(&command)
        .is_err());
    let bundle = fixture
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap();
    assert_eq!(bundle.delegation.state_revision, 1);
    assert_eq!(bundle.outbox_events.len(), 1);
}

#[test]
fn canonical_reopen_succeeds_and_legacy_or_version_only_roots_fail_closed() {
    let canonical = fixture();
    assert!(ExecAssStore::open(&canonical.paths).is_ok());

    let legacy_temp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let legacy_paths = AppPaths::from_root(legacy_temp.path().join("legacy"));
    init(&legacy_paths).unwrap();
    assert!(ExecAssStore::open(&legacy_paths).is_err());

    let version_temp = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let version_paths = AppPaths::from_root(version_temp.path().join("version-only"));
    std::fs::create_dir_all(&version_paths.root).unwrap();
    let conn = Connection::open(&version_paths.db_path).unwrap();
    conn.pragma_update(None, "application_id", crate::EXECASS_APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", crate::EXECASS_SCHEMA_VERSION)
        .unwrap();
    drop(conn);
    assert!(ExecAssStore::open(&version_paths).is_err());
}

#[test]
fn post_construction_schema_tamper_is_rejected_on_the_operation_connection() {
    for object_name in ["execass_hostile_extra", "agent_mail_messages_fts_hostile"] {
        let fixture = fixture();
        let conn = Connection::open(&fixture.paths.db_path).unwrap();
        conn.execute_batch(&format!("CREATE TABLE {object_name} (value TEXT);"))
            .unwrap();
        drop(conn);
        assert!(fixture.store.read_foundation("delegation-1").is_err());
        assert!(ExecAssStore::open(&fixture.paths).is_err());
    }
}

pub(super) fn test_authority_input(authority_seed: &str) -> TestLocalOwnerAuthorityInput {
    TestLocalOwnerAuthorityInput {
        authenticated_client_id: "local-operator".into(),
        authenticated_ingress: "native-control".into(),
        channel_assurance: "interactive-local".into(),
        request_correlation_id: "corr-foundation-1".into(),
        source_message_id: Some("message-1".into()),
        normalized_intent: "prepare the requested bounded result".into(),
        instruction_revision: "instruction-1".into(),
        instruction_bytes: format!("prepare:{authority_seed}").into_bytes(),
        owner_envelope_revision: "envelope-1".into(),
        owner_envelope_json: format!(r#"{{"request":"{authority_seed}"}}"#),
        authority_kind: "original_request".into(),
        normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.into(),
        policy_revision: 1,
        bound_decision_id: None,
        bound_decision_revision: None,
        bound_manifest_bytes: None,
        challenge_nonce_bytes: None,
        created_at: 1_800_000_000_000,
        expires_at: None,
    }
}

fn admitted_authority(authority_seed: &str) -> VerifiedOwnerAuthority {
    issue_test_local_owner_authority(test_authority_input(authority_seed)).unwrap()
}

pub(super) fn admitted_dispatch_with_authority(authority_seed: &str) -> DispatchTree {
    admitted_dispatch_with_verified_authority(admitted_authority(authority_seed))
}

fn admitted_dispatch_with_verified_authority(authority: VerifiedOwnerAuthority) -> DispatchTree {
    DispatchTree {
        root_id: "root".into(),
        nodes: vec![DispatchNode {
            node_id: "root".into(),
            action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                logical_action_id: "action-1".into(),
                action_kind: "tool_call".into(),
                tool: ToolIdentityInput {
                    tool_id: "connector.test".into(),
                    version: "1.0.0".into(),
                },
                operands: CanonicalValue::Object(vec![]),
                target_snapshot: TargetSnapshotInput {
                    targets: vec![CanonicalValue::String("target-1".into())],
                },
                material_digest: None,
                owner_authority: authority,
            })),
        }],
    }
}

fn exact_owner_authorizations(
    authority: &VerifiedOwnerAuthority,
    dispatch: &DispatchTree,
) -> Vec<ExactOwnerActionAuthority> {
    let ManifestCompilation::Ready(manifest) =
        compile_dispatch(dispatch, &ServerResolutionRegistry::default())
    else {
        panic!("expected ready dispatch");
    };
    manifest
        .leaves()
        .iter()
        .map(|leaf| {
            let technical_validity =
                issue_test_objective_technical_validity_proof(leaf, TechnicalValidity::Valid);
            let ExactOwnerAuthorityOutcome::Authorized(authorization) =
                authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                    verified_owner_authority: authority,
                    canonical_leaf: leaf,
                    stopped: false,
                    revoked: false,
                    superseded_by_owner_amendment: false,
                    technical_validity: &technical_validity,
                })
            else {
                panic!("expected exact owner authorization");
            };
            authorization
        })
        .collect()
}

pub(super) fn ready_manifest(dispatch: &DispatchTree) -> CanonicalLeafManifest {
    let ManifestCompilation::Ready(manifest) =
        compile_dispatch(dispatch, &ServerResolutionRegistry::default())
    else {
        panic!("expected ready dispatch");
    };
    manifest
}

pub(super) fn protected_system_danger_route(manifest: &CanonicalLeafManifest) -> DangerRoute {
    protected_system_danger_route_for(manifest, manifest.leaves()[0].logical_action_id())
}

fn protected_system_danger_route_for(
    manifest: &CanonicalLeafManifest,
    logical_action_id: &str,
) -> DangerRoute {
    let leaf = manifest
        .leaves()
        .iter()
        .find(|leaf| leaf.logical_action_id() == logical_action_id)
        .unwrap();
    let metadata = issue_test_verified_danger_metadata(
        leaf,
        &[(
            TestVerifiedDangerFact::CompleteCarsinosProtectedSystem,
            "CarsinOS state root".to_string(),
        )],
    );
    match_known_danger(KnownDangerMatchInput {
        canonical_leaf: leaf,
        verified_metadata: &metadata,
    })
    .unwrap()
}

fn ordinary_danger_route(manifest: &CanonicalLeafManifest) -> DangerRoute {
    let leaf = &manifest.leaves()[0];
    let metadata = issue_test_verified_danger_metadata(leaf, &[]);
    match_known_danger(KnownDangerMatchInput {
        canonical_leaf: leaf,
        verified_metadata: &metadata,
    })
    .unwrap()
}

/// Test-only construction of the same opaque, complete proof gateway supplies
/// in production.  It does not restore a storage bypass: each call still has
/// to pass a leaf-exact proof to the sole admission API.
pub(super) fn ordinary_danger_admission(
    store: &ExecAssStore,
    dispatch: &DispatchTree,
) -> SignedDangerAdmissionProof {
    let manifest = ready_manifest(dispatch);
    let routes = manifest
        .leaves()
        .iter()
        .map(|leaf| {
            let metadata = issue_test_verified_danger_metadata(leaf, &[]);
            match_known_danger(KnownDangerMatchInput {
                canonical_leaf: leaf,
                verified_metadata: &metadata,
            })
            .expect("ordinary test leaf must route")
        })
        .collect();
    signed_test_danger_admission(
        store,
        bind_danger_admission(&manifest, routes).expect("complete ordinary test proof"),
    )
}

fn signed_test_danger_admission(
    store: &ExecAssStore,
    proof: DangerAdmissionProof,
) -> SignedDangerAdmissionProof {
    let seed = [42_u8; 32];
    let identity = activate_test_confirmation_authority(store, seed)
        .expect("activate deterministic test confirmation authority");
    let bytes = danger_admission_signing_bytes(
        &proof,
        identity.key_id(),
        identity.key_generation(),
        identity.canonical_root_identity(),
        identity.installation_identity(),
        identity.os_user_identity_digest(),
        identity.state_root_generation(),
    )
    .expect("build deterministic test danger-admission bytes");
    let signature = SigningKey::from_bytes(&seed).sign(&bytes);
    SignedDangerAdmissionProof::from_untrusted_parts(
        proof,
        identity.key_id().to_string(),
        identity.key_generation(),
        identity.canonical_root_identity().to_string(),
        identity.installation_identity().to_string(),
        identity.os_user_identity_digest().to_string(),
        identity.state_root_generation(),
        signature
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

pub(super) fn signed_danger_admission_for_routes(
    store: &ExecAssStore,
    manifest: &CanonicalLeafManifest,
    routes: Vec<DangerRoute>,
) -> SignedDangerAdmissionProof {
    signed_test_danger_admission(
        store,
        bind_danger_admission(manifest, routes).expect("bind exact danger routes"),
    )
}

fn decision_resolution_authority(
    decision_id: &str,
    decision_revision: i64,
    manifest: &CanonicalLeafManifest,
    challenge_nonce: &[u8],
    created_at: i64,
    expires_at: i64,
) -> VerifiedOwnerAuthority {
    let mut input = test_authority_input("decision-resolution");
    input.request_correlation_id = format!("corr-{decision_id}");
    input.source_message_id = Some(format!("message-{decision_id}"));
    input.authority_kind = "decision_resolution".into();
    input.bound_decision_id = Some(decision_id.into());
    input.bound_decision_revision = Some(decision_revision);
    input.bound_manifest_bytes = Some(manifest.canonical().bytes().to_vec());
    input.challenge_nonce_bytes = Some(challenge_nonce.to_vec());
    input.created_at = created_at;
    input.expires_at = Some(expires_at);
    issue_test_local_owner_authority(input).unwrap()
}

fn action_specific_owner_amendment_authority(
    decision_id: &str,
    decision_revision: i64,
    manifest: &CanonicalLeafManifest,
    challenge_nonce: &[u8],
    created_at: i64,
) -> VerifiedOwnerAuthority {
    let mut input = test_authority_input("action-specific-owner-amendment");
    input.request_correlation_id = format!("corr-amend-{decision_id}");
    input.source_message_id = Some(format!("message-amend-{decision_id}"));
    input.authority_kind = "action_specific_owner_amendment".into();
    input.bound_decision_id = Some(decision_id.into());
    input.bound_decision_revision = Some(decision_revision);
    input.bound_manifest_bytes = Some(manifest.canonical().bytes().to_vec());
    input.challenge_nonce_bytes = Some(challenge_nonce.to_vec());
    input.created_at = created_at;
    issue_test_local_owner_authority(input).unwrap()
}

fn present_confirmation(
    suffix: &str,
    requested_at: i64,
    expires_at: i64,
) -> PresentDangerousActionConfirmationCommand {
    PresentDangerousActionConfirmationCommand {
        delegation_id: "delegation-1".into(),
        logical_action_id: "action-1".into(),
        decision_id: format!("decision-{suffix}"),
        challenge_id: format!("challenge-{suffix}"),
        idempotency_key: format!("idem-decision-{suffix}"),
        challenge_nonce: format!("nonce-{suffix}").into_bytes(),
        requested_at,
        expires_at,
    }
}

#[test]
fn public_dispatch_admission_derives_and_persists_the_canonical_manifest() {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    let outcome = fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let FoundationDispatchAdmissionOutcome::Admitted(outcome) = outcome else {
        panic!("expected admitted foundation");
    };
    assert!(matches!(*outcome, FoundationWriteOutcome::Created(_)));
    let stored = fixture
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap();
    assert!(stored
        .plan
        .resolved_leaf_manifest_json
        .contains("carsinos.execass.leaf_action_manifest.v2"));
    assert_eq!(stored.plan.manifest_digest.len(), 64);
    assert_eq!(stored.authority.authority_provenance_id.len(), 64);
    assert_eq!(stored.authority.evidence_digest.len(), 64);
    assert_eq!(stored.authority.actor_type, ActorType::HumanLocal);
    assert_eq!(stored.authority.credential_identity, "local-operator");
    assert_eq!(stored.authority.authenticated_ingress, "native-control");
    assert_eq!(stored.authority.channel_assurance, "interactive-local");
    assert_eq!(stored.authority.source_correlation_id, "corr-foundation-1");
    assert_eq!(
        stored.authority.source_message_id.as_deref(),
        Some("message-1")
    );
    assert_eq!(
        stored.authority.authority_kind,
        AuthorityKind::OriginalRequest
    );
    assert_eq!(stored.authority.policy_revision, 1);
    assert_eq!(
        stored.delegation.authority_provenance_id,
        stored.authority.authority_provenance_id
    );
    assert_eq!(
        stored.plan.created_by_authority_provenance_id,
        stored.authority.authority_provenance_id
    );
    assert_eq!(table_count(&fixture.paths, "execass_action_branches"), 1);
    assert_eq!(table_count(&fixture.paths, "execass_continuations"), 1);
}

#[test]
fn caller_cannot_launder_grant_reuse_through_a_chosen_normalized_intent() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    let mut command = foundation();
    command.delegation.normalized_original_intent =
        "caller selected another existing grant identity".into();
    assert!(fixture
        .store
        .admit_foundation_dispatch(
            &command,
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .is_err());
    assert_zero_execass_runtime_rows(&fixture.paths);
}

#[test]
fn dangerous_action_has_one_pending_prompt_then_one_durable_reusable_grant() {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let first = present_confirmation("one", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&first, &manifest, &route)
        .unwrap()
    else {
        panic!("first presentation must create one challenge");
    };
    assert!(challenge
        .declared_consequence
        .contains("CarsinOS state root"));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        1
    );
    let pending_binding = fixture
        .store
        .read_pending_danger_confirmation_binding_at_for_test(
            &challenge.decision_id,
            1_800_000_000_011,
        )
        .unwrap()
        .unwrap();
    assert_eq!(pending_binding.delegation_id, "delegation-1");
    assert_eq!(
        pending_binding.normalized_intent,
        "prepare the requested bounded result"
    );
    assert_eq!(pending_binding.policy_revision, 1);
    assert_eq!(
        pending_binding.decision_revision,
        challenge.decision_revision
    );
    assert_eq!(
        pending_binding.canonical_manifest_json,
        std::str::from_utf8(manifest.canonical().bytes()).unwrap()
    );
    assert_eq!(pending_binding.manifest_digest, challenge.manifest_digest);
    assert_eq!(
        pending_binding.exact_presented_action_json,
        challenge.exact_presented_action_json
    );
    assert_eq!(pending_binding.exact_presented_action_digest.len(), 64);
    assert_eq!(
        pending_binding.declared_consequence,
        challenge.declared_consequence
    );
    assert_eq!(
        pending_binding.challenge_nonce_digest,
        challenge.nonce_digest
    );

    let immutable_probe = Connection::open(&fixture.paths.db_path).unwrap();
    assert!(immutable_probe
        .execute(
            "UPDATE execass_confirmation_challenges SET exact_presented_action_json='{}' WHERE decision_id=?1",
            params![challenge.decision_id],
        )
        .is_err());
    assert!(immutable_probe
        .execute(
            "UPDATE execass_confirmation_challenges SET declared_consequence='different consequence' WHERE decision_id=?1",
            params![challenge.decision_id],
        )
        .is_err());
    drop(immutable_probe);

    let duplicate = present_confirmation("duplicate", 1_800_000_000_020, 1_800_000_000_120);
    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(&duplicate, &manifest, &route)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::ExistingPending(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);

    let mut wrong_intent_input = test_authority_input("decision-resolution-wrong-intent");
    wrong_intent_input.normalized_intent = "reuse authority from some other owner request".into();
    wrong_intent_input.authority_kind = "decision_resolution".into();
    wrong_intent_input.bound_decision_id = Some(challenge.decision_id.clone());
    wrong_intent_input.bound_decision_revision = Some(challenge.decision_revision);
    wrong_intent_input.bound_manifest_bytes = Some(manifest.canonical().bytes().to_vec());
    wrong_intent_input.challenge_nonce_bytes = Some(first.challenge_nonce.clone());
    wrong_intent_input.created_at = 1_800_000_000_025;
    wrong_intent_input.expires_at = Some(challenge.expires_at);
    let wrong_intent_authority = issue_test_local_owner_authority(wrong_intent_input).unwrap();
    let wrong_intent_resolution = ConfirmDangerousActionCommand {
        decision_id: challenge.decision_id.clone(),
        decision_revision: challenge.decision_revision,
        grant_id: "grant-wrong-intent".into(),
        selected_logical_action_id: "action-1".into(),
        response: DangerousActionConfirmationResponse::ConfirmAndContinue,
    };
    assert!(fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &wrong_intent_resolution,
            &wrong_intent_authority,
            1_800_000_000_025,
        )
        .is_err());
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );
    assert!(fixture
        .store
        .read_pending_danger_confirmation_binding_at_for_test(
            &challenge.decision_id,
            1_800_000_000_026,
        )
        .unwrap()
        .is_some());

    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &first.challenge_nonce,
        1_800_000_000_030,
        challenge.expires_at,
    );
    let resolution = ConfirmDangerousActionCommand {
        decision_id: challenge.decision_id.clone(),
        decision_revision: challenge.decision_revision,
        grant_id: "grant-one".into(),
        selected_logical_action_id: "action-1".into(),
        response: DangerousActionConfirmationResponse::ConfirmAndContinue,
    };
    let DangerConfirmationResolutionOutcome::Confirmed(grant) = fixture
        .store
        .confirm_dangerous_action_at_for_test(&resolution, &resolver, 1_800_000_000_030)
        .unwrap()
    else {
        panic!("verified owner confirmation must create one grant");
    };
    assert!(grant.invalidated_at.is_none());
    assert!(fixture
        .store
        .read_pending_danger_confirmation_binding_at_for_test(
            &challenge.decision_id,
            1_800_000_000_031,
        )
        .unwrap()
        .is_none());
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_at_for_test(&resolution, &resolver, 1_800_000_000_030)
            .unwrap(),
        DangerConfirmationResolutionOutcome::Replayed(_)
    ));

    let reopened = ExecAssStore::open(&fixture.paths).unwrap();
    let after_restart = present_confirmation("after-restart", 1_800_000_000_200, 1_800_000_000_300);
    assert!(matches!(
        reopened
            .ensure_dangerous_action_confirmation(&after_restart, &manifest, &route)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::AlreadyConfirmed(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    let alternatives: String = conn
        .query_row(
            "SELECT alternatives_json FROM execass_decisions WHERE decision_id=?1",
            params![challenge.decision_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        alternatives,
        r#"["confirm_and_continue","revise","decline"]"#
    );
    let grant_columns = conn
        .prepare("PRAGMA table_info(execass_accepted_confirmation_grants)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(!grant_columns.iter().any(|column| {
        column.contains("expiry") || column.contains("use") || column.contains("count")
    }));
}

#[test]
fn combined_question_lets_the_owner_select_one_immutable_disclosed_alternative() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let mut first = admitted_dispatch_with_verified_authority(authority.clone())
        .nodes
        .remove(0);
    first.node_id = "first".into();
    let mut second = first.clone();
    second.node_id = "second".into();
    let DispatchAction::ResolvedLeaf(second_action) = &mut second.action else {
        panic!("expected resolved leaf");
    };
    second_action.logical_action_id = "action-2".into();
    second_action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "second".into(),
            value: CanonicalValue::Bool(true),
        }]);
    second_action.target_snapshot.targets = vec![CanonicalValue::String("target-2".into())];
    let dispatch = DispatchTree {
        root_id: "root".into(),
        nodes: vec![
            DispatchNode {
                node_id: "root".into(),
                action: DispatchAction::Composite {
                    children: vec!["first".into(), "second".into()],
                },
            },
            first,
            second,
        ],
    };
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let routes = vec![
        protected_system_danger_route_for(&manifest, "action-1"),
        protected_system_danger_route_for(&manifest, "action-2"),
    ];

    let mut duplicate_routes = routes.clone();
    duplicate_routes[1] = duplicate_routes[0].clone();
    assert!(fixture
        .store
        .ensure_combined_dangerous_action_confirmation(
            &present_confirmation("combined-duplicate", 1_800_000_000_010, 1_800_000_000_110),
            &manifest,
            &duplicate_routes,
        )
        .is_err());
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 0);
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );

    let present = present_confirmation("combined", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_combined_dangerous_action_confirmation(&present, &manifest, &routes)
        .unwrap()
    else {
        panic!("combined question must create one pending challenge");
    };
    let pending = fixture
        .store
        .read_pending_danger_confirmation_binding_at_for_test(
            &challenge.decision_id,
            1_800_000_000_011,
        )
        .unwrap()
        .unwrap();
    let question = pending
        .combined_question
        .expect("combined question disclosure");
    assert_eq!(question.alternatives().len(), 2);
    assert_eq!(question.alternatives()[0].logical_action_id(), "action-1");
    assert_eq!(question.alternatives()[1].logical_action_id(), "action-2");
    assert!(question.alternatives()[1]
        .resolved_scope_json()
        .contains("target-2"));
    assert!(question.alternatives().iter().all(|alternative| alternative
        .declared_consequence()
        .contains("CarsinOS state root")));
    assert_eq!(
        table_count(
            &fixture.paths,
            "execass_confirmation_challenge_alternatives"
        ),
        2
    );

    let immutable_probe = Connection::open(&fixture.paths.db_path).unwrap();
    assert!(immutable_probe
        .execute(
            "UPDATE execass_decisions SET alternatives_json='[]' WHERE decision_id=?1",
            params![challenge.decision_id],
        )
        .is_err());
    assert!(immutable_probe
        .execute(
            "UPDATE execass_confirmation_challenge_alternatives SET declared_consequence='tampered' WHERE challenge_id=?1 AND logical_action_id='action-2'",
            params![challenge.challenge_id],
        )
        .is_err());
    assert!(immutable_probe
        .execute(
            "INSERT INTO execass_confirmation_challenge_alternatives (challenge_id,logical_action_id,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence) SELECT challenge_id,'raw-sql-fake',exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence FROM execass_confirmation_challenge_alternatives WHERE challenge_id=?1 AND logical_action_id='action-2'",
            params![challenge.challenge_id],
        )
        .is_err());
    drop(immutable_probe);

    // The presentation has no chosen action.  The affirmative response below
    // selects action-2, so only that immutable binding can receive a grant.
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &present.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    let missing_selection = ConfirmDangerousActionCommand {
        decision_id: challenge.decision_id.clone(),
        decision_revision: challenge.decision_revision,
        grant_id: "grant-missing-selection".into(),
        selected_logical_action_id: String::new(),
        response: DangerousActionConfirmationResponse::ConfirmAndContinue,
    };
    assert!(fixture
        .store
        .confirm_dangerous_action_at_for_test(&missing_selection, &resolver, 1_800_000_000_020)
        .is_err());
    let unknown_selection = ConfirmDangerousActionCommand {
        selected_logical_action_id: "not-disclosed".into(),
        ..missing_selection.clone()
    };
    assert!(fixture
        .store
        .confirm_dangerous_action_at_for_test(&unknown_selection, &resolver, 1_800_000_000_020)
        .is_err());
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );
    let resolution = ConfirmDangerousActionCommand {
        decision_id: challenge.decision_id.clone(),
        decision_revision: challenge.decision_revision,
        grant_id: "grant-combined".into(),
        selected_logical_action_id: "action-2".into(),
        response: DangerousActionConfirmationResponse::ConfirmAndContinue,
    };
    let DangerConfirmationResolutionOutcome::Confirmed(grant) = fixture
        .store
        .confirm_dangerous_action_at_for_test(&resolution, &resolver, 1_800_000_000_020)
        .unwrap()
    else {
        panic!("affirmative selected alternative must grant exactly once");
    };
    assert!(grant
        .payload_and_material_operands_json
        .contains("target-2"));
    assert!(!grant
        .payload_and_material_operands_json
        .contains("target-1"));
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
    assert!(matches!(
        fixture
            .store
            .confirm_dangerous_action_at_for_test(&resolution, &resolver, 1_800_000_000_020)
            .unwrap(),
        DangerConfirmationResolutionOutcome::Replayed(_)
    ));
    let mut changed_selection = resolution.clone();
    changed_selection.selected_logical_action_id = "action-1".into();
    assert!(fixture
        .store
        .confirm_dangerous_action_at_for_test(&changed_selection, &resolver, 1_800_000_000_020)
        .is_err());

    let after_grant =
        present_confirmation("combined-after-grant", 1_800_000_000_030, 1_800_000_000_130);
    let DangerConfirmationAdmissionOutcome::Presented(revised_challenge) = fixture
        .store
        .ensure_combined_dangerous_action_confirmation(&after_grant, &manifest, &routes)
        .unwrap()
    else {
        panic!("unconfirmed alternative must present one new challenge");
    };
    let revise_authority = decision_resolution_authority(
        &revised_challenge.decision_id,
        revised_challenge.decision_revision,
        &manifest,
        &after_grant.challenge_nonce,
        1_800_000_000_040,
        revised_challenge.expires_at,
    );
    assert_eq!(
        fixture
            .store
            .confirm_dangerous_action_at_for_test(
                &ConfirmDangerousActionCommand {
                    decision_id: revised_challenge.decision_id.clone(),
                    decision_revision: revised_challenge.decision_revision,
                    grant_id: "must-not-exist-revise".into(),
                    selected_logical_action_id: String::new(),
                    response: DangerousActionConfirmationResponse::Revise,
                },
                &revise_authority,
                1_800_000_000_040,
            )
            .unwrap(),
        DangerConfirmationResolutionOutcome::Revised
    );
    let declined_present =
        present_confirmation("combined-decline", 1_800_000_000_050, 1_800_000_000_150);
    let DangerConfirmationAdmissionOutcome::Presented(declined_challenge) = fixture
        .store
        .ensure_combined_dangerous_action_confirmation(&declined_present, &manifest, &routes)
        .unwrap()
    else {
        panic!("revised alternative must be terminal and re-presentable");
    };
    let decline_authority = decision_resolution_authority(
        &declined_challenge.decision_id,
        declined_challenge.decision_revision,
        &manifest,
        &declined_present.challenge_nonce,
        1_800_000_000_060,
        declined_challenge.expires_at,
    );
    assert_eq!(
        fixture
            .store
            .confirm_dangerous_action_at_for_test(
                &ConfirmDangerousActionCommand {
                    decision_id: declined_challenge.decision_id,
                    decision_revision: declined_challenge.decision_revision,
                    grant_id: "must-not-exist-decline".into(),
                    selected_logical_action_id: String::new(),
                    response: DangerousActionConfirmationResponse::Decline,
                },
                &decline_authority,
                1_800_000_000_060,
            )
            .unwrap(),
        DangerConfirmationResolutionOutcome::Declined
    );
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 3);
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
    let mut grant_connection = Connection::open(&fixture.paths.db_path).unwrap();
    let grant_transaction = super::store::immediate_transaction(&mut grant_connection).unwrap();
    let unchanged_grant =
        super::confirmation::find_grant_by_decision(&grant_transaction, &resolution.decision_id)
            .unwrap()
            .expect("unrelated revise and decline must preserve the accepted grant");
    grant_transaction.commit().unwrap();
    assert_eq!(unchanged_grant, grant);
}

#[test]
fn expired_or_forged_confirmation_creates_zero_grants_and_can_reissue_once() {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let first = present_confirmation("expiring", 1_800_000_000_010, 1_800_000_000_020);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&first, &manifest, &route)
        .unwrap()
    else {
        panic!("expected challenge");
    };

    let forged = decision_resolution_authority(
        "different-decision",
        challenge.decision_revision,
        &manifest,
        &first.challenge_nonce,
        1_800_000_000_015,
        challenge.expires_at,
    );
    assert!(fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: challenge.decision_id.clone(),
                decision_revision: challenge.decision_revision,
                grant_id: "forged-grant".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &forged,
            1_800_000_000_015,
        )
        .is_err());
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );

    let exact = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &first.challenge_nonce,
        1_800_000_000_019,
        challenge.expires_at,
    );
    assert_eq!(
        fixture
            .store
            .confirm_dangerous_action_at_for_test(
                &ConfirmDangerousActionCommand {
                    decision_id: challenge.decision_id,
                    decision_revision: challenge.decision_revision,
                    grant_id: "expired-grant".into(),
                    selected_logical_action_id: "action-1".into(),
                    response: DangerousActionConfirmationResponse::ConfirmAndContinue,
                },
                &exact,
                1_800_000_000_020,
            )
            .unwrap(),
        DangerConfirmationResolutionOutcome::Expired,
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );

    let reissued = present_confirmation("reissued", 1_800_000_000_030, 1_800_000_000_130);
    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(&reissued, &manifest, &route)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::Presented(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 2);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        2
    );
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    let pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM execass_confirmation_challenges WHERE status='pending'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending, 1);
}

#[test]
fn ordinary_exact_action_cannot_create_confirmation_or_legacy_approval_state() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let ordinary = ordinary_danger_route(&manifest);
    assert!(fixture
        .store
        .ensure_dangerous_action_confirmation(
            &present_confirmation("ordinary", 1_800_000_000_010, 1_800_000_000_110),
            &manifest,
            &ordinary,
        )
        .is_err());
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 0);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        0
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        0
    );
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    let legacy_approval_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('approvals','tool_approvals','assistant_worker_approvals')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_approval_tables, 0);
}

#[test]
fn concurrent_presenters_create_exactly_one_pending_challenge() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let mut threads = Vec::new();
    for index in 0..8 {
        let store = fixture.store.clone();
        let manifest = manifest.clone();
        let route = route.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            store
                .ensure_dangerous_action_confirmation(
                    &present_confirmation(
                        &format!("race-{index}"),
                        1_800_000_000_010,
                        1_800_000_000_110,
                    ),
                    &manifest,
                    &route,
                )
                .unwrap()
        }));
    }
    let outcomes = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, DangerConfirmationAdmissionOutcome::Presented(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                DangerConfirmationAdmissionOutcome::ExistingPending(_)
            ))
            .count(),
        7
    );
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        1
    );
}

#[test]
fn grant_insert_failure_rolls_back_resolution_authority_decision_and_challenge() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let mut first = admitted_dispatch_with_verified_authority(authority.clone())
        .nodes
        .remove(0);
    first.node_id = "first".into();
    let mut second = first.clone();
    second.node_id = "second".into();
    let DispatchAction::ResolvedLeaf(second_action) = &mut second.action else {
        panic!("expected resolved leaf");
    };
    second_action.logical_action_id = "action-2".into();
    second_action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "second".into(),
            value: CanonicalValue::Bool(true),
        }]);
    let dispatch = DispatchTree {
        root_id: "root".into(),
        nodes: vec![
            DispatchNode {
                node_id: "root".into(),
                action: DispatchAction::Composite {
                    children: vec!["first".into(), "second".into()],
                },
            },
            first,
            second,
        ],
    };
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);

    let first_route = protected_system_danger_route_for(&manifest, "action-1");
    let first_present = present_confirmation("rollback-one", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(first_challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&first_present, &manifest, &first_route)
        .unwrap()
    else {
        panic!("expected first challenge");
    };
    let first_resolver = decision_resolution_authority(
        &first_challenge.decision_id,
        first_challenge.decision_revision,
        &manifest,
        &first_present.challenge_nonce,
        1_800_000_000_020,
        first_challenge.expires_at,
    );
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: first_challenge.decision_id,
                decision_revision: first_challenge.decision_revision,
                grant_id: "grant-collision".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &first_resolver,
            1_800_000_000_020,
        )
        .unwrap();

    let second_route = protected_system_danger_route_for(&manifest, "action-2");
    let mut second_present =
        present_confirmation("rollback-two", 1_800_000_000_030, 1_800_000_000_130);
    second_present.logical_action_id = "action-2".into();
    let DangerConfirmationAdmissionOutcome::Presented(second_challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&second_present, &manifest, &second_route)
        .unwrap()
    else {
        panic!("expected second challenge");
    };
    let second_resolver = decision_resolution_authority(
        &second_challenge.decision_id,
        second_challenge.decision_revision,
        &manifest,
        &second_present.challenge_nonce,
        1_800_000_000_040,
        second_challenge.expires_at,
    );
    assert!(fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: second_challenge.decision_id.clone(),
                decision_revision: second_challenge.decision_revision,
                grant_id: "grant-collision".into(),
                selected_logical_action_id: "action-2".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &second_resolver,
            1_800_000_000_040,
        )
        .is_err());
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    let (decision_status, challenge_status): (String, String) = conn
        .query_row(
            "SELECT d.status,c.status FROM execass_decisions d JOIN execass_confirmation_challenges c ON c.decision_id=d.decision_id WHERE d.decision_id=?1",
            params![second_challenge.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(decision_status, "pending");
    assert_eq!(challenge_status, "pending");
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_attestations"),
        1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_authority_provenance"),
        3
    );
}

#[test]
fn independent_delegation_does_not_reuse_grant_and_material_drift_prompts_once() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let first = present_confirmation("routine-one", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&first, &manifest, &route)
        .unwrap()
    else {
        panic!("expected first prompt");
    };
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &first.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: challenge.decision_id,
                decision_revision: challenge.decision_revision,
                grant_id: "grant-routine".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &resolver,
            1_800_000_000_020,
        )
        .unwrap();

    let second_command = foundation_occurrence("occurrence-2");
    let mut second_input = test_authority_input("authority-occurrence-2");
    second_input.request_correlation_id = second_command.write.correlation_id.clone();
    second_input.source_message_id = Some("message-occurrence-2".into());
    let second_authority = issue_test_local_owner_authority(second_input).unwrap();
    let second_dispatch = admitted_dispatch_with_verified_authority(second_authority.clone());
    let second_authorizations = exact_owner_authorizations(&second_authority, &second_dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &second_command,
            &second_dispatch,
            &ServerResolutionRegistry::default(),
            &second_authority,
            &second_authorizations,
            &ordinary_danger_admission(&fixture.store, &second_dispatch),
        )
        .unwrap();
    let second_manifest = ready_manifest(&second_dispatch);
    let second_route = protected_system_danger_route(&second_manifest);
    let mut second_present =
        present_confirmation("routine-two", 1_800_000_000_030, 1_800_000_000_130);
    second_present.delegation_id = second_command.delegation.delegation_id.clone();
    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(&second_present, &second_manifest, &second_route)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::Presented(_)
    ));

    let third_command = foundation_occurrence("occurrence-3");
    let mut third_input = test_authority_input("authority-occurrence-3");
    third_input.request_correlation_id = third_command.write.correlation_id.clone();
    third_input.source_message_id = Some("message-occurrence-3".into());
    let third_authority = issue_test_local_owner_authority(third_input).unwrap();
    let mut third_dispatch = admitted_dispatch_with_verified_authority(third_authority.clone());
    let DispatchAction::ResolvedLeaf(action) = &mut third_dispatch.nodes[0].action else {
        panic!("expected leaf");
    };
    action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "material_change".into(),
            value: CanonicalValue::String("different payload".into()),
        }]);
    let third_authorizations = exact_owner_authorizations(&third_authority, &third_dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &third_command,
            &third_dispatch,
            &ServerResolutionRegistry::default(),
            &third_authority,
            &third_authorizations,
            &ordinary_danger_admission(&fixture.store, &third_dispatch),
        )
        .unwrap();
    let third_manifest = ready_manifest(&third_dispatch);
    let third_route = protected_system_danger_route(&third_manifest);
    let mut third_present =
        present_confirmation("routine-three", 1_800_000_000_040, 1_800_000_000_140);
    third_present.delegation_id = third_command.delegation.delegation_id.clone();
    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(&third_present, &third_manifest, &third_route)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::Presented(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 3);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        3
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
}

#[test]
fn unchanged_action_reuses_grant_across_replan_ids_and_policy_revalidation() {
    let fixture = fixture();
    let authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let authorizations = exact_owner_authorizations(&authority, &dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .unwrap();
    let manifest = ready_manifest(&dispatch);
    let route = protected_system_danger_route(&manifest);
    let first = present_confirmation("stable-action", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&first, &manifest, &route)
        .unwrap()
    else {
        panic!("expected first prompt");
    };
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &manifest,
        &first.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: challenge.decision_id,
                decision_revision: challenge.decision_revision,
                grant_id: "grant-stable-action".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &resolver,
            1_800_000_000_020,
        )
        .unwrap();

    let mut second_dispatch = admitted_dispatch_with_verified_authority(authority.clone());
    let DispatchAction::ResolvedLeaf(action) = &mut second_dispatch.nodes[0].action else {
        panic!("expected leaf");
    };
    action.logical_action_id = "planner-local-action-77".into();
    second_dispatch.nodes[0].node_id = "planner-local-node-77".into();
    second_dispatch.root_id = "planner-local-node-77".into();
    let second_manifest = ready_manifest(&second_dispatch);
    let current = fixture
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap();
    let mut replanned = foundation().plan;
    replanned.plan_id = "plan-replanned".into();
    replanned.plan_revision = 2;
    replanned.based_on_delegation_revision = current.delegation.state_revision + 1;
    replanned.plan_summary = "same exact action with planner-local IDs changed".into();
    replanned.created_by_authority_provenance_id =
        current.authority.authority_provenance_id.clone();
    replanned.resolved_leaf_manifest_json =
        String::from_utf8(second_manifest.canonical().bytes().to_vec()).unwrap();
    replanned.manifest_digest = second_manifest.canonical().digest().as_hex().to_string();
    let mut criteria = foundation().outcome_criteria;
    for (index, criterion) in criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("replanned-criterion-{index}");
        criterion.criteria_revision = 2;
    }
    let next_revision = current.delegation.state_revision + 1;
    fixture
        .store
        .amend_lifecycle(&AmendLifecycleCommand {
            write: WriteContext {
                idempotency_key: "idem-replanned".into(),
                correlation_id: "corr-replanned".into(),
                causation_id: "cause-replanned".into(),
                occurred_at: 1_800_000_000_025,
            },
            delegation_id: "delegation-1".into(),
            expected_state_revision: current.delegation.state_revision,
            transition_id: "transition-replanned".into(),
            amendment_id: "amendment-replanned".into(),
            amendment_revision: 1,
            normalized_amendment: "replan the unchanged confirmed action".into(),
            intake_evidence_json: "{}".into(),
            authority_provenance_id: current.authority.authority_provenance_id.clone(),
            plan: replanned,
            outcome_criteria: criteria,
            outbox_event: NewOutboxEvent {
                event_id: "event-replanned".into(),
                event_name: OutboxEventName::DelegationTransitioned,
                aggregate_id: "delegation-1".into(),
                aggregate_revision: next_revision,
                correlation_id: "corr-replanned".into(),
                causation_id: "cause-replanned".into(),
                occurred_at: 1_800_000_000_025,
                safe_payload_json: "{}".into(),
                duplicate_identity: "idem-replanned".into(),
            },
        })
        .unwrap();
    let second_route = protected_system_danger_route(&second_manifest);
    let mut second_present =
        present_confirmation("replanned", 1_800_000_000_030, 1_800_000_000_130);
    second_present.delegation_id = "delegation-1".into();
    second_present.logical_action_id = "planner-local-action-77".into();
    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(&second_present, &second_manifest, &second_route,)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::AlreadyConfirmed(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
}

#[test]
fn explicit_owner_revocation_invalidates_only_the_identified_confirmed_action() {
    let fixture = fixture();
    let first_authority = admitted_authority("authority-1");
    let first_dispatch = admitted_dispatch_with_verified_authority(first_authority.clone());
    let first_authorizations = exact_owner_authorizations(&first_authority, &first_dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &first_dispatch,
            &ServerResolutionRegistry::default(),
            &first_authority,
            &first_authorizations,
            &ordinary_danger_admission(&fixture.store, &first_dispatch),
        )
        .unwrap();
    let first_manifest = ready_manifest(&first_dispatch);
    let first_route = protected_system_danger_route(&first_manifest);
    let first_present =
        present_confirmation("revocation-first", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(first_challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&first_present, &first_manifest, &first_route)
        .unwrap()
    else {
        panic!("expected first challenge");
    };
    let first_resolver = decision_resolution_authority(
        &first_challenge.decision_id,
        first_challenge.decision_revision,
        &first_manifest,
        &first_present.challenge_nonce,
        1_800_000_000_020,
        first_challenge.expires_at,
    );
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: first_challenge.decision_id.clone(),
                decision_revision: first_challenge.decision_revision,
                grant_id: "grant-revocation-first".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &first_resolver,
            1_800_000_000_020,
        )
        .unwrap();

    let second_command = foundation_occurrence("revocation-second");
    let mut second_input = test_authority_input("authority-revocation-second");
    second_input.request_correlation_id = second_command.write.correlation_id.clone();
    second_input.source_message_id = Some("message-revocation-second".into());
    let second_authority = issue_test_local_owner_authority(second_input).unwrap();
    let mut second_dispatch = admitted_dispatch_with_verified_authority(second_authority.clone());
    let DispatchAction::ResolvedLeaf(second_action) = &mut second_dispatch.nodes[0].action else {
        panic!("expected second leaf");
    };
    second_action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "material_change".into(),
            value: CanonicalValue::String("second exact action".into()),
        }]);
    let second_authorizations = exact_owner_authorizations(&second_authority, &second_dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &second_command,
            &second_dispatch,
            &ServerResolutionRegistry::default(),
            &second_authority,
            &second_authorizations,
            &ordinary_danger_admission(&fixture.store, &second_dispatch),
        )
        .unwrap();
    let second_manifest = ready_manifest(&second_dispatch);
    let second_route = protected_system_danger_route(&second_manifest);
    let mut second_present =
        present_confirmation("revocation-second", 1_800_000_000_030, 1_800_000_000_130);
    second_present.delegation_id = second_command.delegation.delegation_id.clone();
    let DangerConfirmationAdmissionOutcome::Presented(second_challenge) = fixture
        .store
        .ensure_dangerous_action_confirmation(&second_present, &second_manifest, &second_route)
        .unwrap()
    else {
        panic!("expected second challenge");
    };
    let second_resolver = decision_resolution_authority(
        &second_challenge.decision_id,
        second_challenge.decision_revision,
        &second_manifest,
        &second_present.challenge_nonce,
        1_800_000_000_040,
        second_challenge.expires_at,
    );
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: second_challenge.decision_id.clone(),
                decision_revision: second_challenge.decision_revision,
                grant_id: "grant-revocation-second".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &second_resolver,
            1_800_000_000_040,
        )
        .unwrap();

    let command = InvalidateAcceptedConfirmationGrantCommand {
        grant_id: "grant-revocation-first".into(),
        decision_id: first_challenge.decision_id.clone(),
        invalidation_reason:
            AcceptedConfirmationGrantInvalidation::ExplicitActionSpecificOwnerRevocation,
        invalidated_at: 1_800_000_000_060,
    };
    let wrong_authority = action_specific_owner_amendment_authority(
        &second_challenge.decision_id,
        second_challenge.decision_revision,
        &second_manifest,
        &second_present.challenge_nonce,
        1_800_000_000_050,
    );
    assert!(fixture
        .store
        .invalidate_confirmation_grant_by_owner(&command, &wrong_authority)
        .is_err());

    let exact_authority = action_specific_owner_amendment_authority(
        &first_challenge.decision_id,
        first_challenge.decision_revision,
        &first_manifest,
        &first_present.challenge_nonce,
        1_800_000_000_050,
    );
    let ConfirmationGrantInvalidationOutcome::Invalidated(first_grant) = fixture
        .store
        .invalidate_confirmation_grant_by_owner(&command, &exact_authority)
        .unwrap()
    else {
        panic!("expected exact owner revocation");
    };
    assert_eq!(first_grant.invalidated_at, Some(command.invalidated_at));
    assert_eq!(
        first_grant.invalidation_reason,
        Some(AcceptedConfirmationGrantInvalidation::ExplicitActionSpecificOwnerRevocation)
    );
    assert!(matches!(
        fixture
            .store
            .invalidate_confirmation_grant_by_owner(&command, &exact_authority)
            .unwrap(),
        ConfirmationGrantInvalidationOutcome::Replayed(_)
    ));

    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    let second_invalidated_at: Option<i64> = conn
        .query_row(
            "SELECT invalidated_at FROM execass_accepted_confirmation_grants WHERE grant_id='grant-revocation-second'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(second_invalidated_at, None);
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        2
    );
    assert!(fixture
        .store
        .read_danger_confirmation_runtime_projection(&first_challenge.decision_id, "action-1",)
        .unwrap()
        .is_none());
    assert!(matches!(
        fixture
            .store
            .read_danger_confirmation_runtime_projection(&second_challenge.decision_id, "action-1",)
            .unwrap(),
        Some(DangerConfirmationRuntimeProjection::Resolved(_))
    ));

    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(
                &present_confirmation(
                    "revocation-first-reprompt",
                    1_800_000_000_070,
                    1_800_000_000_170,
                ),
                &first_manifest,
                &first_route,
            )
            .unwrap(),
        DangerConfirmationAdmissionOutcome::Presented(_)
    ));
    assert!(matches!(
        fixture
            .store
            .ensure_dangerous_action_confirmation(&second_present, &second_manifest, &second_route,)
            .unwrap(),
        DangerConfirmationAdmissionOutcome::AlreadyConfirmed(_)
    ));
}

#[test]
fn saved_routine_expected_membership_change_reuses_grant_but_selector_change_prompts() {
    let fixture = fixture();
    let first_authority = admitted_authority("authority-1");
    let first_dispatch = admitted_dispatch_with_verified_authority(first_authority.clone());
    let first_authorizations = exact_owner_authorizations(&first_authority, &first_dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &first_dispatch,
            &ServerResolutionRegistry::default(),
            &first_authority,
            &first_authorizations,
            &ordinary_danger_admission(&fixture.store, &first_dispatch),
        )
        .unwrap();
    let first_manifest = ready_manifest(&first_dispatch);
    let first_route = protected_system_danger_route(&first_manifest);
    let selector = issue_test_verified_saved_routine_selector(
        &first_manifest.leaves()[0],
        "routine-cleanup",
        1,
        r#"{"selector":"all protected roots currently selected by owner"}"#,
    )
    .unwrap();
    let first_present =
        present_confirmation("routine-selector", 1_800_000_000_010, 1_800_000_000_110);
    let DangerConfirmationAdmissionOutcome::Presented(challenge) = fixture
        .store
        .ensure_saved_routine_dangerous_action_confirmation(
            &first_present,
            &first_manifest,
            &first_route,
            &selector,
        )
        .unwrap()
    else {
        panic!("expected first saved-routine challenge");
    };
    let resolver = decision_resolution_authority(
        &challenge.decision_id,
        challenge.decision_revision,
        &first_manifest,
        &first_present.challenge_nonce,
        1_800_000_000_020,
        challenge.expires_at,
    );
    fixture
        .store
        .confirm_dangerous_action_at_for_test(
            &ConfirmDangerousActionCommand {
                decision_id: challenge.decision_id,
                decision_revision: challenge.decision_revision,
                grant_id: "grant-routine-selector".into(),
                selected_logical_action_id: "action-1".into(),
                response: DangerousActionConfirmationResponse::ConfirmAndContinue,
            },
            &resolver,
            1_800_000_000_020,
        )
        .unwrap();

    let second_command = foundation_occurrence("routine-membership-two");
    let mut second_input = test_authority_input("authority-routine-membership-two");
    second_input.request_correlation_id = second_command.write.correlation_id.clone();
    second_input.source_message_id = Some("message-routine-membership-two".into());
    let second_authority = issue_test_local_owner_authority(second_input).unwrap();
    let mut second_dispatch = admitted_dispatch_with_verified_authority(second_authority.clone());
    let DispatchAction::ResolvedLeaf(second_action) = &mut second_dispatch.nodes[0].action else {
        panic!("expected second leaf");
    };
    second_action.logical_action_id = "occurrence-local-action-2".into();
    second_action.target_snapshot.targets = vec![
        CanonicalValue::String("target-1".into()),
        CanonicalValue::String("new-expected-member".into()),
    ];
    let second_authorizations = exact_owner_authorizations(&second_authority, &second_dispatch);
    fixture
        .store
        .admit_foundation_dispatch(
            &second_command,
            &second_dispatch,
            &ServerResolutionRegistry::default(),
            &second_authority,
            &second_authorizations,
            &ordinary_danger_admission(&fixture.store, &second_dispatch),
        )
        .unwrap();
    let second_manifest = ready_manifest(&second_dispatch);
    assert!(selector.matches_stable_leaf(&second_manifest.leaves()[0]));
    let second_route = protected_system_danger_route(&second_manifest);
    let mut second_present = present_confirmation(
        "routine-membership-two",
        1_800_000_000_030,
        1_800_000_000_130,
    );
    second_present.delegation_id = second_command.delegation.delegation_id;
    second_present.logical_action_id = "occurrence-local-action-2".into();
    assert!(matches!(
        fixture
            .store
            .ensure_saved_routine_dangerous_action_confirmation(
                &second_present,
                &second_manifest,
                &second_route,
                &selector,
            )
            .unwrap(),
        DangerConfirmationAdmissionOutcome::AlreadyConfirmed(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);

    let schedule_only_version = issue_test_verified_saved_routine_selector(
        &second_manifest.leaves()[0],
        "routine-cleanup",
        2,
        r#"{"selector":"all protected roots currently selected by owner"}"#,
    )
    .unwrap();
    assert!(matches!(
        fixture
            .store
            .ensure_saved_routine_dangerous_action_confirmation(
                &second_present,
                &second_manifest,
                &second_route,
                &schedule_only_version,
            )
            .unwrap(),
        DangerConfirmationAdmissionOutcome::AlreadyConfirmed(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);

    let amended_selector = issue_test_verified_saved_routine_selector(
        &second_manifest.leaves()[0],
        "routine-cleanup",
        2,
        r#"{"selector":"broader materially amended owner selector"}"#,
    )
    .unwrap();
    assert!(matches!(
        fixture
            .store
            .ensure_saved_routine_dangerous_action_confirmation(
                &PresentDangerousActionConfirmationCommand {
                    decision_id: "decision-routine-amended".into(),
                    challenge_id: "challenge-routine-amended".into(),
                    idempotency_key: "idem-routine-amended".into(),
                    challenge_nonce: b"nonce-routine-amended".to_vec(),
                    requested_at: 1_800_000_000_040,
                    expires_at: 1_800_000_000_140,
                    ..second_present
                },
                &second_manifest,
                &second_route,
                &amended_selector,
            )
            .unwrap(),
        DangerConfirmationAdmissionOutcome::Presented(_)
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 2);
    assert_eq!(
        table_count(&fixture.paths, "execass_accepted_confirmation_grants"),
        1
    );
}

#[test]
fn public_dispatch_admission_requires_the_exact_action_bound_owner_token() {
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);

    let missing = fixture();
    assert!(missing
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &[],
            &ordinary_danger_admission(&missing.store, &dispatch),
        )
        .is_err());
    assert_zero_execass_runtime_rows(&missing.paths);

    let mut changed_dispatch = dispatch.clone();
    let DispatchAction::ResolvedLeaf(action) = &mut changed_dispatch.nodes[0].action else {
        panic!("expected resolved leaf");
    };
    action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "changed".into(),
            value: CanonicalValue::Bool(true),
        }]);
    let mismatched = fixture();
    assert!(mismatched
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &changed_dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &ordinary_danger_admission(&mismatched.store, &changed_dispatch),
        )
        .is_err());
    assert_zero_execass_runtime_rows(&mismatched.paths);
}

#[test]
fn foundation_admission_rejects_a_proof_for_a_substituted_canonical_leaf() {
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    let mut substituted = dispatch.clone();
    let DispatchAction::ResolvedLeaf(action) = &mut substituted.nodes[0].action else {
        panic!("expected resolved leaf");
    };
    action.operands =
        CanonicalValue::Object(vec![carsinos_core::execass_manifest::CanonicalField {
            key: "resolved_target".into(),
            value: CanonicalValue::String("substituted-target".into()),
        }]);
    let fixture = fixture();
    assert!(fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &ordinary_danger_admission(&fixture.store, &substituted),
        )
        .is_err());
    assert_zero_execass_runtime_rows(&fixture.paths);
}

#[test]
fn foundation_admission_routes_a_proven_danger_to_confirmation_without_writes() {
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    let manifest = ready_manifest(&dispatch);
    let proof =
        bind_danger_admission(&manifest, vec![protected_system_danger_route(&manifest)]).unwrap();
    let fixture = fixture();
    let proof = signed_test_danger_admission(&fixture.store, proof);
    let outcome = fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &proof,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        FoundationDispatchAdmissionOutcome::DangerConfirmationRequired
    ));
    assert_zero_execass_runtime_rows(&fixture.paths);
}

#[test]
fn foundation_admission_rejects_publicly_minted_proof_without_custody_signature() {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
    let manifest = ready_manifest(&dispatch);
    let routes = manifest
        .leaves()
        .iter()
        .map(|leaf| {
            let metadata = issue_test_verified_danger_metadata(leaf, &[]);
            match_known_danger(KnownDangerMatchInput {
                canonical_leaf: leaf,
                verified_metadata: &metadata,
            })
            .unwrap()
        })
        .collect();
    let proof = bind_danger_admission(&manifest, routes).unwrap();
    let identity = activate_test_confirmation_authority(&fixture.store, [42_u8; 32]).unwrap();
    let forged = SignedDangerAdmissionProof::from_untrusted_parts(
        proof,
        identity.key_id().to_string(),
        identity.key_generation(),
        identity.canonical_root_identity().to_string(),
        identity.installation_identity().to_string(),
        identity.os_user_identity_digest().to_string(),
        identity.state_root_generation(),
        "00".repeat(64),
    );
    assert!(fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &authorizations,
            &forged,
        )
        .is_err());
    assert_zero_execass_runtime_rows(&fixture.paths);
}

#[test]
fn every_objective_invalidity_produces_no_token_and_zero_runtime_rows() {
    for validity in [
        TechnicalValidity::ActionIdentityUnresolved,
        TechnicalValidity::CapabilityUnavailable,
        TechnicalValidity::OperandUnresolved,
        TechnicalValidity::RuntimePreconditionUnmet,
        TechnicalValidity::TransactionOrFencingInvalid,
        TechnicalValidity::ReconciliationUnavailable,
        TechnicalValidity::ResourceUnavailable,
    ] {
        let fixture = fixture();
        let authority = admitted_authority("authority-1");
        let dispatch = admitted_dispatch_with_verified_authority(authority.clone());
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&dispatch, &ServerResolutionRegistry::default())
        else {
            panic!("expected ready dispatch");
        };
        let leaf = &manifest.leaves()[0];
        let technical_validity = issue_test_objective_technical_validity_proof(leaf, validity);
        assert!(matches!(
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: &authority,
                canonical_leaf: leaf,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &technical_validity,
            }),
            ExactOwnerAuthorityOutcome::Paused(_)
        ));
        assert!(fixture
            .store
            .admit_foundation_dispatch(
                &foundation(),
                &dispatch,
                &ServerResolutionRegistry::default(),
                &authority,
                &[],
                &ordinary_danger_admission(&fixture.store, &dispatch),
            )
            .is_err());
        assert_zero_execass_runtime_rows(&fixture.paths);
    }
}

#[test]
fn public_admission_ignores_every_caller_supplied_authority_field() {
    let baseline = fixture();
    let expected_authority = admitted_authority("authority-1");
    let baseline_dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
    let baseline_authorizations =
        exact_owner_authorizations(&expected_authority, &baseline_dispatch);
    baseline
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &baseline_dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &baseline_authorizations,
            &ordinary_danger_admission(&baseline.store, &baseline_dispatch),
        )
        .unwrap();
    let expected = baseline
        .store
        .read_foundation("delegation-1")
        .unwrap()
        .unwrap()
        .authority;
    for mutation in 0..17 {
        let fixture = fixture();
        let mut command = foundation();
        let dispatch = admitted_dispatch_with_verified_authority(expected_authority.clone());
        let authorizations = exact_owner_authorizations(&expected_authority, &dispatch);
        match mutation {
            0 => command.authority.authority_provenance_id = "caller-id".into(),
            1 => command.authority.actor_type = ActorType::Model,
            2 => command.authority.credential_identity = "caller-credential".into(),
            3 => command.authority.authenticated_ingress = "caller-ingress".into(),
            4 => command.authority.channel_assurance = "caller-assurance".into(),
            5 => command.authority.source_correlation_id = "caller-correlation".into(),
            6 => command.authority.source_message_id = Some("caller-message".into()),
            7 => command.authority.authority_kind = AuthorityKind::RuntimeSafetyState,
            8 => command.authority.normalized_scope_json = "not-json".into(),
            9 => command.authority.policy_revision = 999,
            10 => command.authority.bound_decision_id = Some("caller-decision".into()),
            11 => command.authority.bound_decision_revision = Some(999),
            12 => command.authority.bound_manifest_digest = Some("caller-manifest".into()),
            13 => command.authority.bound_challenge_nonce_digest = Some("caller-challenge".into()),
            14 => command.authority.evidence_digest = "caller-evidence".into(),
            15 => command.authority.created_at = -1,
            _ => command.authority.expires_at = Some(-1),
        }
        let outcome = fixture
            .store
            .admit_foundation_dispatch(
                &command,
                &dispatch,
                &ServerResolutionRegistry::default(),
                &expected_authority,
                &authorizations,
                &ordinary_danger_admission(&fixture.store, &dispatch),
            )
            .unwrap();
        assert!(matches!(
            outcome,
            FoundationDispatchAdmissionOutcome::Admitted(_)
        ));
        let stored = fixture
            .store
            .read_foundation("delegation-1")
            .unwrap()
            .unwrap();
        assert_eq!(stored.authority, expected, "caller mutation {mutation}");
        assert_eq!(
            stored.delegation.authority_provenance_id, expected.authority_provenance_id,
            "caller mutation {mutation}"
        );
        assert_eq!(
            stored.plan.created_by_authority_provenance_id, expected.authority_provenance_id,
            "caller mutation {mutation}"
        );
    }
}

#[test]
fn every_authenticated_source_mutation_fails_expected_authority_admission_with_zero_rows() {
    let expected_authority = admitted_authority("authority-1");
    for mutation in 0..23 {
        let fixture = fixture();
        let mut input = test_authority_input("authority-1");
        match mutation {
            0 => input.authenticated_client_id = "other-client".into(),
            1 => input.authenticated_ingress = "other-ingress".into(),
            2 => input.channel_assurance = "other-assurance".into(),
            3 => input.request_correlation_id = "other-correlation".into(),
            4 => input.source_message_id = Some("other-message".into()),
            5 => input.normalized_intent = "other intent".into(),
            6 => input.instruction_revision = "instruction-2".into(),
            7 => input.instruction_bytes = b"other instruction".to_vec(),
            8 => input.owner_envelope_revision = "envelope-2".into(),
            9 => input.owner_envelope_json = r#"{"request":"other"}"#.into(),
            10 => input.authority_kind = "policy_snapshot".into(),
            11 => input.normalized_scope_json = r#"{"workspace":"Z:\\other"}"#.into(),
            12 => input.policy_revision = 2,
            13 => input.bound_decision_id = Some("decision-1".into()),
            14 => input.bound_decision_revision = Some(1),
            15 => {
                input.bound_decision_id = Some("decision-1".into());
                input.bound_decision_revision = Some(1);
            }
            16 => input.bound_manifest_bytes = Some(b"other manifest".to_vec()),
            17 => input.challenge_nonce_bytes = Some(b"other nonce".to_vec()),
            18 => input.created_at += 1,
            19 => input.expires_at = Some(1_800_000_000_100),
            20 => input.normalized_scope_json = "not-json".into(),
            21 => input.owner_envelope_json = "not-json".into(),
            _ => input.instruction_bytes.clear(),
        }
        if let Ok(mutated_authority) = issue_test_local_owner_authority(input) {
            let dispatch = admitted_dispatch_with_verified_authority(mutated_authority);
            assert!(
                fixture
                    .store
                    .admit_foundation_dispatch(
                        &foundation(),
                        &dispatch,
                        &ServerResolutionRegistry::default(),
                        &expected_authority,
                        &[],
                        &ordinary_danger_admission(&fixture.store, &dispatch),
                    )
                    .is_err(),
                "source mutation {mutation} was admitted"
            );
        }
        assert_zero_execass_runtime_rows(&fixture.paths);
    }
}

#[test]
fn mixed_verified_authorities_across_manifest_leaves_create_zero_runtime_rows() {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let mut first = admitted_dispatch_with_authority("authority-1")
        .nodes
        .remove(0);
    first.node_id = "first".into();
    let mut second = admitted_dispatch_with_authority("authority-2")
        .nodes
        .remove(0);
    second.node_id = "second".into();
    if let DispatchAction::ResolvedLeaf(action) = &mut second.action {
        action.logical_action_id = "action-2".into();
    }
    let dispatch = DispatchTree {
        root_id: "root".into(),
        nodes: vec![
            DispatchNode {
                node_id: "root".into(),
                action: DispatchAction::Composite {
                    children: vec!["first".into(), "second".into()],
                },
            },
            first,
            second,
        ],
    };
    assert!(fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &dispatch,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &[],
            &ordinary_danger_admission(&fixture.store, &dispatch),
        )
        .is_err());
    for table in [
        "execass_authority_provenance",
        "execass_delegations",
        "execass_plans",
        "execass_action_branches",
        "execass_continuations",
        "execass_logical_effects",
        "execass_outbox_events",
        "execass_receipts",
    ] {
        assert_eq!(
            table_count(&fixture.paths, table),
            seeded_global_control_rows(table),
            "unexpected row in {table}"
        );
    }
}

#[test]
fn mechanical_pause_from_public_admission_creates_zero_runtime_rows() {
    let fixture = fixture();
    let expected_authority = admitted_authority("authority-1");
    let unresolved = DispatchTree {
        root_id: "root".into(),
        nodes: vec![DispatchNode {
            node_id: "root".into(),
            action: DispatchAction::Alias {
                alias: "missing-resolution".into(),
            },
        }],
    };
    let unused_ready_dispatch =
        admitted_dispatch_with_verified_authority(expected_authority.clone());
    let unused_danger_admission = ordinary_danger_admission(&fixture.store, &unused_ready_dispatch);
    let outcome = fixture
        .store
        .admit_foundation_dispatch(
            &foundation(),
            &unresolved,
            &ServerResolutionRegistry::default(),
            &expected_authority,
            &[],
            &unused_danger_admission,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        FoundationDispatchAdmissionOutcome::MechanicalResolutionRequired(_)
    ));
    for table in [
        "execass_authority_provenance",
        "execass_delegations",
        "execass_plans",
        "execass_action_branches",
        "execass_continuations",
        "execass_logical_effects",
        "execass_outbox_events",
        "execass_receipts",
    ] {
        assert_eq!(
            table_count(&fixture.paths, table),
            seeded_global_control_rows(table),
            "unexpected row in {table}"
        );
    }
}
