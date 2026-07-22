use super::receipt_integrity::AnchorCommitInput;
use super::tests::{fixture, foundation, table_count};
use super::*;
use crate::open_sqlite_connection;
use carsinos_core::execass_actor::{
    bind_follow_up_amendment_owner_authority, derive_local_owner_actor_assurance,
    AuthenticatedLocalOwnerEvidence, FollowUpAmendmentAuthoritySource,
};
use carsinos_core::execass_danger::{
    issue_test_verified_danger_metadata, match_known_danger, DangerRoute, KnownDangerMatchInput,
};
use carsinos_core::execass_manifest::{
    compile_dispatch, CanonicalLeafManifest, CanonicalValue, DispatchAction, DispatchNode,
    DispatchTree, ManifestCompilation, ResolvedLeafInput, ServerResolutionRegistry,
    TargetSnapshotInput, ToolIdentityInput,
};

fn trust_empty_receipt_history(
    fixture: &super::tests::Fixture,
) -> (ReceiptIntegrityStore, ReceiptKeyRef) {
    let integrity = ReceiptIntegrityStore::open(&fixture.paths).unwrap();
    let key = integrity
        .provision_initial_key("lifecycle-completion-key")
        .unwrap();
    integrity
        .prepare_anchor(&AnchorCommitInput {
            state_root_generation: 1,
            anchor_generation: 1,
            receipt_count: 0,
            receipt_head_digest: None,
            key: key.clone(),
            transaction_id: "lifecycle-completion-anchor".into(),
            external_receipt_digest:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            occurred_at: 1_800_000_009_000,
        })
        .unwrap();
    let mut connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    let transaction = connection.transaction().unwrap();
    integrity
        .confirm_prepared_anchor_in_transaction(
            &transaction,
            "lifecycle-completion-anchor",
            0,
            None,
            1_800_000_009_001,
        )
        .unwrap();
    transaction.commit().unwrap();
    integrity
        .finalize_anchor("lifecycle-completion-anchor")
        .unwrap();
    assert!(matches!(
        integrity.status().unwrap(),
        IntegrityStatus::Trusted {
            receipt_count: 0,
            ..
        }
    ));
    (integrity, key)
}

fn action(
    action_id: &str,
    revision: i64,
    kind: ActionBranchKind,
    status: ContinuationStatus,
) -> ActionBranchRecord {
    ActionBranchRecord {
        action_id: action_id.into(),
        delegation_id: "delegation-1".into(),
        action_revision: revision,
        target_delegation_revision: 1,
        target_plan_revision: 1,
        stop_epoch: 0,
        branch_kind: kind,
        status,
        action_summary: format!("{kind:?} action"),
        created_at: 1_800_000_001_000 + revision,
        updated_at: 1_800_000_001_000 + revision,
        terminal_at: matches!(
            status,
            ContinuationStatus::Terminal | ContinuationStatus::Superseded
        )
        .then_some(1_800_000_001_000 + revision),
    }
}

fn foundation_action(status: ContinuationStatus) -> ActionBranchRecord {
    let continuation = foundation().initial_continuation.unwrap();
    ActionBranchRecord {
        action_id: continuation.action_id,
        delegation_id: continuation.delegation_id,
        action_revision: 1,
        target_delegation_revision: continuation.target_delegation_revision,
        target_plan_revision: continuation.target_plan_revision,
        stop_epoch: continuation.stop_epoch,
        branch_kind: continuation.branch_kind,
        status,
        action_summary: "initial durable continuation".into(),
        created_at: continuation.created_at,
        updated_at: continuation.updated_at + 1,
        terminal_at: matches!(
            status,
            ContinuationStatus::Terminal | ContinuationStatus::Superseded
        )
        .then_some(continuation.updated_at + 1),
    }
}

fn attention(id: &str, revision: i64) -> AttentionItemRecord {
    AttentionItemRecord {
        attention_id: id.into(),
        delegation_id: "delegation-1".into(),
        action_id: None,
        kind: AttentionKind::Clarification,
        status: AttentionStatus::Actionable,
        reason: "need one bounded clarification".into(),
        recommendation: "answer it".into(),
        alternatives_json: "[]".into(),
        required_assurance: "human_local_or_remote".into(),
        decision_id: None,
        delegation_revision: revision,
        created_at: 1_800_000_001_000 + revision,
        resolved_at: None,
    }
}

fn external_wait(id: &str, revision: i64) -> ExternalWaitRecord {
    ExternalWaitRecord {
        external_wait_id: id.into(),
        delegation_id: "delegation-1".into(),
        action_id: None,
        kind: ExternalWaitKind::ExternalParty,
        status: ExternalWaitStatus::Waiting,
        reason: "provider confirmation remains pending".into(),
        details_json: r#"{"provider":"test"}"#.into(),
        delegation_revision: revision,
        created_at: 1_800_000_001_000 + revision,
        resolved_at: None,
    }
}

fn command(id: &str, expected: i64, control: RunControlState) -> LifecycleSnapshotCommand {
    let now = 1_800_000_010_000 + expected;
    LifecycleSnapshotCommand {
        write: WriteContext {
            idempotency_key: format!("idem-{id}"),
            correlation_id: format!("corr-{id}"),
            causation_id: format!("cause-{id}"),
            occurred_at: now,
        },
        transition_id: format!("transition-{id}"),
        delegation_id: "delegation-1".into(),
        expected_state_revision: expected,
        pre_actionable_phase: None,
        selected_run_control: control,
        resume_proof: None,
        action_branches: vec![],
        attention_items: vec![],
        external_waits: vec![],
        assessment: None,
        continuation: None,
        reason: "test lifecycle projection".into(),
        outbox_event: NewOutboxEvent {
            event_id: format!("event-{id}"),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: expected + 1,
            correlation_id: format!("corr-{id}"),
            causation_id: format!("cause-{id}"),
            occurred_at: now,
            safe_payload_json: "{}".into(),
            duplicate_identity: format!("idem-{id}"),
        },
    }
}

fn amendment_authority(
    suffix: &str,
    target_delegation_id: &str,
    delegation_revision: i64,
    plan_revision: i64,
    normalized_amendment: &str,
) -> carsinos_core::execass_actor::VerifiedOwnerAuthority {
    let actor = derive_local_owner_actor_assurance(
        AuthenticatedLocalOwnerEvidence::from_verified_native_hmac(
            "local-operator",
            "native-control",
            "interactive-local",
            format!("follow-up-correlation-{suffix}"),
        )
        .unwrap(),
    );
    let source = FollowUpAmendmentAuthoritySource::builder()
        .target(target_delegation_id, delegation_revision, plan_revision)
        .normalized_intent(normalized_amendment)
        .owner_instruction(
            format!("follow-up-instruction-{suffix}"),
            format!("follow-up instruction {suffix}").into_bytes(),
        )
        .canonical_owner_envelope(
            format!("follow-up-envelope-{suffix}"),
            r#"{"request":"follow-up"}"#,
        )
        .policy_revision(1)
        .created_at_ms(1_800_000_000_010)
        .build()
        .unwrap();
    bind_follow_up_amendment_owner_authority(&actor, source).unwrap()
}

fn follow_up_manifest(
    authority: carsinos_core::execass_actor::VerifiedOwnerAuthority,
    target: &str,
) -> CanonicalLeafManifest {
    let dispatch = DispatchTree {
        root_id: "follow-up-root".into(),
        nodes: vec![DispatchNode {
            node_id: "follow-up-root".into(),
            action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                logical_action_id: "action-1".into(),
                action_kind: "tool_call".into(),
                tool: ToolIdentityInput {
                    tool_id: "connector.test".into(),
                    version: "1.0.0".into(),
                },
                operands: CanonicalValue::Object(vec![]),
                target_snapshot: TargetSnapshotInput {
                    targets: vec![CanonicalValue::String(target.into())],
                },
                material_digest: None,
                owner_authority: authority,
            })),
        }],
    };
    let ManifestCompilation::Ready(manifest) =
        compile_dispatch(&dispatch, &ServerResolutionRegistry::default())
    else {
        panic!("follow-up manifest must compile")
    };
    manifest
}

fn follow_up_runtime(fixture: &super::tests::Fixture) {
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .execute_batch(
            "INSERT INTO execass_runtime_host_generations(generation,ownership_scope,state_root_generation,installation_identity,os_user_identity_digest,host_instance_id,started_at) VALUES(1,'execass',1,'follow-up-installation','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','follow-up-host',1); INSERT INTO execass_runtime_host_leases(lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at) VALUES('follow-up-lease','execass',1,'follow-up-host',1,1,9999999999999);",
        )
        .unwrap();
}

fn follow_up_presentation(suffix: &str) -> PresentDangerousActionConfirmationCommand {
    PresentDangerousActionConfirmationCommand {
        delegation_id: "delegation-1".into(),
        logical_action_id: "action-1".into(),
        decision_id: format!("decision-follow-up-{suffix}"),
        challenge_id: format!("challenge-follow-up-{suffix}"),
        idempotency_key: format!("idem-follow-up-{suffix}"),
        challenge_nonce: format!("nonce-follow-up-{suffix}").into_bytes(),
        requested_at: 1_800_000_000_030,
        expires_at: 1_800_000_000_130,
    }
}

fn ordinary_follow_up_route(manifest: &CanonicalLeafManifest) -> DangerRoute {
    let leaf = &manifest.leaves()[0];
    let metadata = issue_test_verified_danger_metadata(leaf, &[]);
    match_known_danger(KnownDangerMatchInput {
        canonical_leaf: leaf,
        verified_metadata: &metadata,
    })
    .unwrap()
}

fn follow_up_command(
    manifest: &CanonicalLeafManifest,
    authority: &carsinos_core::execass_actor::VerifiedOwnerAuthority,
    key: ReceiptKeyRef,
    suffix: &str,
) -> ApplyVerifiedFollowUpAmendmentCommand {
    let canonical =
        carsinos_core::execass_manifest::canonicalize_owner_authority(authority).unwrap();
    let authority_record = super::foundation::authority_record_from_manifest(&canonical).unwrap();
    let base = foundation();
    let mut plan = base.plan.clone();
    plan.plan_id = format!("follow-up-plan-{suffix}");
    plan.plan_revision = 2;
    plan.based_on_delegation_revision = 2;
    plan.plan_summary = format!("follow-up plan {suffix}");
    plan.manifest_digest = manifest.canonical().digest().as_hex().to_string();
    plan.resolved_leaf_manifest_json =
        String::from_utf8(manifest.canonical().bytes().to_vec()).unwrap();
    plan.created_by_authority_provenance_id = authority_record.authority_provenance_id.clone();
    let mut outcome_criteria = base.outcome_criteria.clone();
    for (index, criterion) in outcome_criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("follow-up-criterion-{suffix}-{index}");
        criterion.criteria_revision = 2;
    }
    let occurred_at = 1_800_000_020_000;
    let outbox_event = NewOutboxEvent {
        event_id: format!("follow-up-event-{suffix}"),
        event_name: OutboxEventName::DelegationTransitioned,
        aggregate_id: "delegation-1".into(),
        aggregate_revision: 2,
        correlation_id: format!("follow-up-correlation-{suffix}"),
        causation_id: format!("follow-up-causation-{suffix}"),
        occurred_at,
        safe_payload_json: "{}".into(),
        duplicate_identity: format!("follow-up-idempotency-{suffix}"),
    };
    ApplyVerifiedFollowUpAmendmentCommand {
        amendment: AmendLifecycleCommand {
            write: WriteContext {
                idempotency_key: outbox_event.duplicate_identity.clone(),
                correlation_id: outbox_event.correlation_id.clone(),
                causation_id: outbox_event.causation_id.clone(),
                occurred_at,
            },
            delegation_id: "delegation-1".into(),
            expected_state_revision: 1,
            transition_id: format!("follow-up-transition-{suffix}"),
            amendment_id: format!("follow-up-amendment-{suffix}"),
            amendment_revision: 1,
            normalized_amendment: format!("follow-up amendment {suffix}"),
            intake_evidence_json: "{}".into(),
            authority_provenance_id: authority_record.authority_provenance_id.clone(),
            plan,
            outcome_criteria,
            outbox_event: outbox_event.clone(),
        },
        receipt: AppendReceiptCommand {
            receipt_id: format!("follow-up-receipt-{suffix}"),
            transaction_id: format!("follow-up-receipt-transaction-{suffix}"),
            state_root_generation: 1,
            delegation_id: "delegation-1".into(),
            expected_state_revision: 2,
            expected_global_count: 0,
            expected_global_head_digest: None,
            expected_delegation_count: 0,
            expected_delegation_head_digest: None,
            receipt_kind: ReceiptKind::Amendment,
            subject: ReceiptSubject {
                kind: ReceiptSubjectKind::PlanAmendment,
                subject_id: format!("follow-up-amendment-{suffix}"),
                revision: 1,
            },
            causation_id: outbox_event.causation_id,
            causation_event_id: outbox_event.event_id,
            actor: ReceiptActorBinding {
                actor_type: authority_record.actor_type,
                actor_identity: SafeText::new(&authority_record.credential_identity, &[]).unwrap(),
                authority_provenance_id: authority_record.authority_provenance_id,
            },
            runtime: ReceiptRuntimeBinding {
                host_generation: 1,
                host_instance_id: "follow-up-host".into(),
                fencing_token: 1,
            },
            key,
            rotation: None,
            evidence: vec![],
            redacted_summary: SafeText::new("verified follow-up amendment", &[]).unwrap(),
            occurred_at,
            committed_at: occurred_at,
        },
    }
}

#[test]
fn selector_is_exhaustive_and_preserves_locked_precedence() {
    for assessment in [None, Some(CompletionAssessmentKind::Completed)] {
        for pre in [
            None,
            Some(PreActionablePhase::Accepted),
            Some(PreActionablePhase::Planning),
        ] {
            for ordinary in [false, true] {
                for recovery in [false, true] {
                    for attention in [false, true] {
                        for external in [false, true] {
                            let input = LifecycleSelectorInput {
                                completion_assessment: assessment,
                                pre_actionable_phase: pre,
                                ordinary_runnable_or_executing: ordinary,
                                recovery_runnable_or_executing: recovery,
                                actionable_attention: attention,
                                external_wait: external,
                            };
                            let selected = select_lifecycle_phase(input);
                            let expected = assessment
                                .map(CompletionAssessmentKind::phase)
                                .or_else(|| pre.map(PreActionablePhase::phase))
                                .or(ordinary.then_some(DelegationPhase::InMotion))
                                .or(recovery.then_some(DelegationPhase::Recovering))
                                .or(attention.then_some(DelegationPhase::WaitingForUser))
                                .or(external.then_some(DelegationPhase::WaitingExternal));
                            assert_eq!(selected.ok(), expected, "input={input:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn no_path_is_rejected_without_an_assessment() {
    assert!(matches!(
        select_lifecycle_phase(LifecycleSelectorInput {
            completion_assessment: None,
            pre_actionable_phase: None,
            ordinary_runnable_or_executing: false,
            recovery_runnable_or_executing: false,
            actionable_attention: false,
            external_wait: false,
        }),
        Err(error) if error.to_string().contains("honest completion assessment")
    ));
}

#[test]
fn foundation_continuation_is_an_ordinary_action_branch() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    let branch: (String, String) = conn
        .query_row(
            "SELECT action_id,branch_kind FROM execass_action_branches WHERE action_id='action-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(branch, ("action-1".into(), "ordinary".into()));
}

#[test]
fn persisted_mixed_branch_precedence_and_exact_replay_are_deterministic() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let mut first = command("mixed", 1, RunControlState::Running);
    first.attention_items.push(attention("attention-mixed", 2));
    first.external_waits.push(external_wait("wait-mixed", 2));
    let LifecycleWriteOutcome::Applied(applied) =
        fixture.store.apply_lifecycle_snapshot(&first).unwrap()
    else {
        panic!("expected applied")
    };
    assert_eq!(applied.delegation.phase, DelegationPhase::InMotion);
    let later = command("mixed-later", 2, RunControlState::Running);
    let LifecycleWriteOutcome::Applied(later_snapshot) =
        fixture.store.apply_lifecycle_snapshot(&later).unwrap()
    else {
        panic!("expected later progress")
    };
    assert_eq!(later_snapshot.delegation.state_revision, 3);
    let LifecycleWriteOutcome::Replayed(replayed) =
        fixture.store.apply_lifecycle_snapshot(&first).unwrap()
    else {
        panic!("expected replay")
    };
    assert_eq!(replayed, applied);
    let mut changed_reason = first.clone();
    changed_reason.reason = "changed replay payload".into();
    assert!(fixture
        .store
        .apply_lifecycle_snapshot(&changed_reason)
        .is_err());
    let mut changed_attention = first.clone();
    changed_attention.attention_items[0].reason = "changed attention identity".into();
    assert!(fixture
        .store
        .apply_lifecycle_snapshot(&changed_attention)
        .is_err());
    let mut changed_wait = first.clone();
    changed_wait.external_waits[0].details_json = r#"{"provider":"changed"}"#.into();
    assert!(fixture
        .store
        .apply_lifecycle_snapshot(&changed_wait)
        .is_err());
    let mut changed_pre_phase = first.clone();
    changed_pre_phase.pre_actionable_phase = Some(PreActionablePhase::Planning);
    assert!(fixture
        .store
        .apply_lifecycle_snapshot(&changed_pre_phase)
        .is_err());
    let stale = command("stale", 1, RunControlState::Running);
    assert_eq!(
        fixture.store.apply_lifecycle_snapshot(&stale).unwrap(),
        LifecycleWriteOutcome::Stale {
            current_state_revision: 3
        }
    );
}

#[test]
fn recovery_attention_and_attention_external_have_the_locked_selection() {
    let recovery_fixture = fixture();
    recovery_fixture
        .store
        .create_foundation(&foundation())
        .unwrap();
    let mut recovery = command("recovery", 1, RunControlState::Running);
    let mut recovery_action = action(
        "action-recovery",
        2,
        ActionBranchKind::Recovery,
        ContinuationStatus::Runnable,
    );
    recovery_action.target_delegation_revision = 2;
    recovery.action_branches = vec![
        foundation_action(ContinuationStatus::Terminal),
        recovery_action,
    ];
    recovery
        .attention_items
        .push(attention("attention-recovery", 2));
    let LifecycleWriteOutcome::Applied(snapshot) = recovery_fixture
        .store
        .apply_lifecycle_snapshot(&recovery)
        .unwrap()
    else {
        panic!("expected recovery")
    };
    assert_eq!(snapshot.delegation.phase, DelegationPhase::Recovering);

    let fixture = fixture();
    let mut base = foundation();
    base.initial_continuation = None;
    fixture.store.create_foundation(&base).unwrap();
    let mut waits = command("waits", 1, RunControlState::Running);
    waits.attention_items.push(attention("attention-waits", 2));
    waits.external_waits.push(external_wait("wait-waits", 2));
    let LifecycleWriteOutcome::Applied(snapshot) =
        fixture.store.apply_lifecycle_snapshot(&waits).unwrap()
    else {
        panic!("expected waiting")
    };
    assert_eq!(snapshot.delegation.phase, DelegationPhase::WaitingForUser);
}

#[test]
fn no_path_and_outbox_collision_roll_back_branch_and_history() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let mut no_path = command("no-path", 1, RunControlState::Running);
    no_path
        .action_branches
        .push(foundation_action(ContinuationStatus::Terminal));
    assert!(fixture.store.apply_lifecycle_snapshot(&no_path).is_err());
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_action_branches WHERE action_id='action-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "runnable"
    );
    drop(conn);
    let mut collision = command("collision", 1, RunControlState::Running);
    collision.outbox_event.event_id = "event-foundation-1".into();
    collision
        .action_branches
        .push(foundation_action(ContinuationStatus::Executing));
    assert!(fixture.store.apply_lifecycle_snapshot(&collision).is_err());
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_action_branches WHERE action_id='action-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "runnable"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_lifecycle_transitions",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn stop_drain_resume_fences_claims_without_relabeling_phase() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let stop = command("stop-request", 1, RunControlState::StopRequested);
    let LifecycleWriteOutcome::Applied(snapshot) =
        fixture.store.apply_lifecycle_snapshot(&stop).unwrap()
    else {
        panic!("stop request")
    };
    assert_eq!(
        (snapshot.delegation.phase, snapshot.delegation.run_control),
        (DelegationPhase::InMotion, RunControlState::StopRequested)
    );
    assert_eq!(snapshot.delegation.stop_epoch, 1);
    let mut blocked = command("blocked", 2, RunControlState::StopRequested);
    blocked.action_branches.push(action(
        "action-new",
        2,
        ActionBranchKind::Ordinary,
        ContinuationStatus::Runnable,
    ));
    assert!(fixture.store.apply_lifecycle_snapshot(&blocked).is_err());
    let mut drain = command("drain", 2, RunControlState::Stopped);
    drain
        .action_branches
        .push(foundation_action(ContinuationStatus::Terminal));
    drain.external_waits.push(external_wait("wait-drain", 3));
    let LifecycleWriteOutcome::Applied(snapshot) =
        fixture.store.apply_lifecycle_snapshot(&drain).unwrap()
    else {
        panic!("drain")
    };
    assert_eq!(
        (snapshot.delegation.phase, snapshot.delegation.run_control),
        (DelegationPhase::WaitingExternal, RunControlState::Stopped)
    );
    assert_eq!(snapshot.delegation.stop_epoch, 1);
    let blind_resume = command("blind-resume", 3, RunControlState::Running);
    assert!(fixture
        .store
        .apply_lifecycle_snapshot(&blind_resume)
        .is_err());
    let mut resume = command("resume", 3, RunControlState::Running);
    let mut resume_action = action(
        "action-resume",
        2,
        ActionBranchKind::Ordinary,
        ContinuationStatus::Runnable,
    );
    resume_action.target_delegation_revision = 4;
    resume_action.stop_epoch = 2;
    resume.action_branches.push(resume_action);
    resume.resume_proof = Some(ResumeProof {
        plan_revision: 1,
        policy_revision: 1,
        authority_provenance_id: "authority-1".into(),
        budget_snapshot_digest: "sha256:budget".into(),
        global_stop_epoch: 0,
    });
    resume.continuation = Some(ContinuationRecord {
        continuation_id: "continuation-resume".into(),
        delegation_id: "delegation-1".into(),
        target_delegation_revision: 4,
        target_plan_revision: 1,
        action_id: "action-resume".into(),
        branch_kind: ActionBranchKind::Ordinary,
        causation_kind: ContinuationCausationKind::Resume,
        causation_id: "cause-resume".into(),
        status: ContinuationStatus::Runnable,
        job_id: None,
        lease_owner: None,
        lease_expires_at: None,
        fencing_token: 0,
        host_generation: 1,
        stop_epoch: 2,
        global_stop_epoch: 0,
        created_at: 1_800_000_010_003,
        updated_at: 1_800_000_010_003,
        completed_at: None,
    });
    let LifecycleWriteOutcome::Applied(snapshot) =
        fixture.store.apply_lifecycle_snapshot(&resume).unwrap()
    else {
        panic!("resume")
    };
    assert_eq!(
        (snapshot.delegation.phase, snapshot.delegation.run_control),
        (DelegationPhase::InMotion, RunControlState::Running)
    );
    assert_eq!(snapshot.delegation.stop_epoch, 2);
}

#[test]
fn stop_drain_rejects_an_omitted_persisted_executing_branch() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let mut executing = command("executing", 1, RunControlState::Running);
    executing
        .action_branches
        .push(foundation_action(ContinuationStatus::Executing));
    fixture.store.apply_lifecycle_snapshot(&executing).unwrap();
    fixture
        .store
        .apply_lifecycle_snapshot(&command(
            "executing-stop",
            2,
            RunControlState::StopRequested,
        ))
        .unwrap();
    let mut omitted = command("omitted-drain", 3, RunControlState::Stopped);
    omitted
        .external_waits
        .push(external_wait("omitted-wait", 4));
    assert!(fixture.store.apply_lifecycle_snapshot(&omitted).is_err());
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT run_control FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "stop_requested"
    );
}

#[test]
fn terminal_assessment_is_honest_and_terminal_correction_does_not_reopen() {
    let fixture = fixture();
    let mut base = foundation();
    base.initial_continuation = None;
    fixture.store.create_foundation(&base).unwrap();
    let _ = trust_empty_receipt_history(&fixture);
    let mut complete = command("complete", 1, RunControlState::Running);
    complete.assessment = Some(CompletionAssessmentRecord {
        assessment_id: "assessment-1".into(),
        delegation_id: "delegation-1".into(),
        assessment_revision: 1,
        criteria_revision: 1,
        kind: CompletionAssessmentKind::Completed,
        material_pass_count: 2,
        material_fail_count: 0,
        material_unknown_count: 0,
        useful_outcome: true,
        exact_unmet_portion: None,
        no_remaining_path: true,
        assessment_json: "{}".into(),
        assessed_at: 1_800_000_010_001,
    });
    let LifecycleWriteOutcome::Applied(terminal) =
        fixture.store.apply_lifecycle_snapshot(&complete).unwrap()
    else {
        panic!("completed")
    };
    assert_eq!(terminal.delegation.phase, DelegationPhase::Completed);
    let mut invalid = command("invalid-terminal", 2, RunControlState::Running);
    invalid.assessment = Some(CompletionAssessmentRecord {
        material_unknown_count: 1,
        assessment_id: "bad".into(),
        ..complete.assessment.clone().unwrap()
    });
    assert!(fixture.store.apply_lifecycle_snapshot(&invalid).is_err());
    let correction = TerminalCorrectionCommand {
        write: WriteContext {
            idempotency_key: "correction-idem".into(),
            correlation_id: "correction-corr".into(),
            causation_id: "correction-cause".into(),
            occurred_at: 1_800_000_011_000,
        },
        correction: TerminalCorrectionRecord {
            correction_id: "correction-1".into(),
            delegation_id: "delegation-1".into(),
            terminal_assessment_id: "assessment-1".into(),
            correction_revision: 1,
            contrary_evidence_json: "{}".into(),
            warning: "late contrary provider evidence".into(),
            recorded_at: 1_800_000_011_000,
        },
        outbox_event: NewOutboxEvent {
            event_id: "correction-event".into(),
            event_name: OutboxEventName::CompletionAssessed,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 2,
            correlation_id: "correction-corr".into(),
            causation_id: "correction-cause".into(),
            occurred_at: 1_800_000_011_000,
            safe_payload_json: "{}".into(),
            duplicate_identity: "correction-idem".into(),
        },
    };
    let LifecycleWriteOutcome::Applied(snapshot) = fixture
        .store
        .record_terminal_correction(&correction)
        .unwrap()
    else {
        panic!("correction")
    };
    assert_eq!(snapshot.delegation.phase, DelegationPhase::Completed);
    let LifecycleWriteOutcome::Replayed(replayed) = fixture
        .store
        .record_terminal_correction(&correction)
        .unwrap()
    else {
        panic!("exact correction replay")
    };
    assert_eq!(replayed, snapshot);
    let mut changed = correction.clone();
    changed.correction.warning = "different contrary evidence warning".into();
    assert!(fixture.store.record_terminal_correction(&changed).is_err());
    let mut changed_event = correction.clone();
    changed_event.outbox_event.event_id = "different-correction-event".into();
    assert!(fixture
        .store
        .record_terminal_correction(&changed_event)
        .is_err());
    let rebuilt = fixture
        .store
        .rebuild_lifecycle_projection("delegation-1")
        .unwrap()
        .unwrap();
    assert_eq!(rebuilt.phase, DelegationPhase::Completed);
    assert_eq!(rebuilt.state_revision, 2);
    assert_eq!(rebuilt.completion_assessment_json.as_deref(), Some("{}"));
}

#[derive(Debug, PartialEq, Eq)]
struct CompletionFenceSnapshot {
    state_revision: i64,
    phase: String,
    completion_assessment_json: Option<String>,
    assessment_rows: i64,
    outbox_rows: i64,
    receipt_rows: i64,
    logical_effect_rows: i64,
}

fn completion_fence_snapshot(fixture: &super::tests::Fixture) -> CompletionFenceSnapshot {
    let connection = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    let (state_revision, phase, completion_assessment_json) = connection
        .query_row(
            "SELECT state_revision,phase,completion_assessment_json FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let count = |table: &str| {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    CompletionFenceSnapshot {
        state_revision,
        phase,
        completion_assessment_json,
        assessment_rows: count("execass_completion_assessments"),
        outbox_rows: count("execass_outbox_events"),
        receipt_rows: count("execass_receipts"),
        logical_effect_rows: count("execass_logical_effects"),
    }
}

#[test]
fn ea112_key_registry_tamper_blocks_completion_without_any_mutation() {
    super::receipt_tests::assert_ea112_case_registered("completion_after_pending_key_tamper");
    let mut attacks = vec!["used_created_at", "pending_key_id"];
    #[cfg(windows)]
    attacks.push("pending_key_material");

    for attack in attacks {
        let fixture = fixture();
        let mut base = foundation();
        base.initial_continuation = None;
        fixture.store.create_foundation(&base).unwrap();
        let (integrity, _active_key) = trust_empty_receipt_history(&fixture);
        let pending_key = integrity
            .rotate_key(&format!("lifecycle-pending-{attack}"))
            .unwrap();

        if attack == "pending_key_material" {
            #[cfg(windows)]
            {
                let key_path = integrity.anchor_directory().join("keys").join(format!(
                    "{}-{:020}.dpapi",
                    pending_key.key_id, pending_key.key_generation
                ));
                std::fs::write(&key_path, b"invalid-dpapi-material").unwrap();
            }
        } else {
            let connection = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
            connection
                .set_db_config(
                    rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
                    false,
                )
                .unwrap();
            connection
                .pragma_update(None, "foreign_keys", "OFF")
                .unwrap();
            let sql = match attack {
                "used_created_at" => {
                    "UPDATE execass_receipt_keys SET created_at=created_at+1 WHERE key_generation=1"
                }
                "pending_key_id" => {
                    "UPDATE execass_receipt_keys SET key_id='rewritten-lifecycle-pending' WHERE key_generation=2"
                }
                _ => unreachable!(),
            };
            assert_eq!(connection.execute(sql, []).unwrap(), 1, "{attack}");
        }
        assert!(matches!(
            integrity.status().unwrap(),
            IntegrityStatus::Mismatch { .. }
        ));

        let before = completion_fence_snapshot(&fixture);
        let mut complete = command(
            &format!("registry-tamper-completion-{attack}"),
            1,
            RunControlState::Running,
        );
        complete.assessment = Some(CompletionAssessmentRecord {
            assessment_id: format!("assessment-registry-tamper-{attack}"),
            delegation_id: "delegation-1".into(),
            assessment_revision: 1,
            criteria_revision: 1,
            kind: CompletionAssessmentKind::Completed,
            material_pass_count: 2,
            material_fail_count: 0,
            material_unknown_count: 0,
            useful_outcome: true,
            exact_unmet_portion: None,
            no_remaining_path: true,
            assessment_json: "{}".into(),
            assessed_at: 1_800_000_010_001,
        });
        assert!(
            fixture.store.apply_lifecycle_snapshot(&complete).is_err(),
            "{attack}"
        );
        assert_eq!(completion_fence_snapshot(&fixture), before, "{attack}");
        assert!(matches!(
            integrity.recover_integrity().unwrap(),
            IntegrityRecovery::Quarantined { .. }
        ));
        assert_eq!(completion_fence_snapshot(&fixture), before, "{attack}");
    }
}

#[test]
fn amendment_supersedes_old_continuation_and_links_the_criteria_revisions() {
    let fixture = fixture();
    let base = foundation();
    fixture.store.create_foundation(&base).unwrap();
    let mut plan = base.plan.clone();
    plan.plan_id = "plan-2".into();
    plan.plan_revision = 2;
    plan.based_on_delegation_revision = 2;
    plan.plan_summary = "amended bounded work".into();
    plan.manifest_digest = "sha256:amended-manifest".into();
    let mut criteria = base.outcome_criteria.clone();
    for (index, criterion) in criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("amended-criterion-{index}");
        criterion.criteria_revision = 2;
    }
    let amendment = AmendLifecycleCommand {
        write: WriteContext {
            idempotency_key: "amendment-idem".into(),
            correlation_id: "amendment-corr".into(),
            causation_id: "amendment-cause".into(),
            occurred_at: 1_800_000_020_000,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: 1,
        transition_id: "transition-amendment".into(),
        amendment_id: "amendment-1".into(),
        amendment_revision: 1,
        normalized_amendment: "tighten the requested outcome".into(),
        intake_evidence_json: "{}".into(),
        authority_provenance_id: "authority-1".into(),
        plan,
        outcome_criteria: criteria,
        outbox_event: NewOutboxEvent {
            event_id: "event-amendment".into(),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 2,
            correlation_id: "amendment-corr".into(),
            causation_id: "amendment-cause".into(),
            occurred_at: 1_800_000_020_000,
            safe_payload_json: "{}".into(),
            duplicate_identity: "amendment-idem".into(),
        },
    };
    let LifecycleWriteOutcome::Applied(snapshot) =
        fixture.store.amend_lifecycle(&amendment).unwrap()
    else {
        panic!("amendment must apply")
    };
    assert_eq!(
        (
            snapshot.delegation.phase,
            snapshot.delegation.current_plan_revision,
            snapshot.delegation.current_criteria_revision
        ),
        (DelegationPhase::Planning, Some(2), Some(2))
    );
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "superseded"
    );
    assert_eq!(conn.query_row("SELECT superseded_criteria_revision || ':' || resulting_criteria_revision FROM execass_amendment_criteria_links WHERE amendment_id='amendment-1'", [], |row| row.get::<_, String>(0)).unwrap(), "1:2");
    drop(conn);
    let LifecycleWriteOutcome::Replayed(replayed) =
        fixture.store.amend_lifecycle(&amendment).unwrap()
    else {
        panic!("exact amendment replay")
    };
    assert_eq!(replayed, snapshot);
    let mut changed_body = amendment.clone();
    changed_body.normalized_amendment = "conflicting amendment body".into();
    assert!(fixture.store.amend_lifecycle(&changed_body).is_err());
    let mut changed_plan = amendment.clone();
    changed_plan.plan.plan_summary = "conflicting plan".into();
    assert!(fixture.store.amend_lifecycle(&changed_plan).is_err());
}

#[test]
fn verified_follow_up_amendment_is_atomic_idempotent_and_reconciles_only_material_drift() {
    let (fixture, confirmation, attestation, _) = super::tests::prepared_attested_confirmation();
    let DangerConfirmationResolutionOutcome::Confirmed(grant) = fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &confirmation,
            &attestation,
            1_800_000_000_020,
        )
        .unwrap()
    else {
        panic!("fixture must produce an accepted grant")
    };
    follow_up_runtime(&fixture);
    let (integrity, key) = trust_empty_receipt_history(&fixture);
    let follow_up_authority =
        amendment_authority("same", "delegation-1", 1, 1, "follow-up amendment same");
    let manifest = follow_up_manifest(follow_up_authority.clone(), "target-1");
    let canonical_follow_up =
        carsinos_core::execass_manifest::canonicalize_owner_authority(&follow_up_authority)
            .unwrap();
    assert_eq!(
        canonical_follow_up.normalized_scope_json(),
        r#"{"delegation_id":"delegation-1","delegation_revision":1,"plan_revision":1}"#
    );
    assert_eq!(
        carsinos_core::execass_actor::owner_normalized_intent_digest("follow-up amendment same")
            .as_deref(),
        Some(canonical_follow_up.normalized_intent_digest().as_hex())
    );
    let route = super::tests::protected_system_danger_route(&manifest);
    let proof =
        super::tests::signed_danger_admission_for_routes(&fixture.store, &manifest, vec![route]);
    let command = follow_up_command(&manifest, &follow_up_authority, key.clone(), "same");
    let redactor = ReceiptRedactor::new(&["follow-up-test-secret"]).unwrap();
    let before = grant_row(&fixture, &grant.grant_id);

    assert!(matches!(
        fixture
            .store
            .apply_verified_follow_up_amendment(
                &integrity,
                &redactor,
                &command,
                &follow_up_authority,
                &manifest,
                &proof,
            )
            .unwrap(),
        VerifiedFollowUpAmendmentOutcome::Applied(_)
    ));
    assert_eq!(grant_row(&fixture, &grant.grant_id), before);
    let reused = fixture
        .store
        .ensure_dangerous_action_confirmation(
            &follow_up_presentation("same"),
            &manifest,
            &super::tests::protected_system_danger_route(&manifest),
        )
        .unwrap();
    assert!(matches!(
        reused,
        DangerConfirmationAdmissionOutcome::AlreadyConfirmed(ref reused_grant)
            if reused_grant.grant_id == grant.grant_id
    ));
    assert_eq!(table_count(&fixture.paths, "execass_decisions"), 1);
    assert_eq!(
        table_count(&fixture.paths, "execass_confirmation_challenges"),
        1,
        "an unchanged amended action must not issue a replacement prompt"
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_receipts"),
        1,
        "receipt is part of the same amendment mutation"
    );
    assert!(matches!(
        fixture
            .store
            .apply_verified_follow_up_amendment(
                &integrity,
                &redactor,
                &command,
                &follow_up_authority,
                &manifest,
                &proof,
            )
            .unwrap(),
        VerifiedFollowUpAmendmentOutcome::Replayed(_)
    ));
    let stale_authority =
        amendment_authority("stale", "delegation-1", 1, 1, "follow-up amendment stale");
    let stale_manifest = follow_up_manifest(stale_authority.clone(), "target-1");
    let stale_proof = super::tests::signed_danger_admission_for_routes(
        &fixture.store,
        &stale_manifest,
        vec![super::tests::protected_system_danger_route(&stale_manifest)],
    );
    let stale_command = follow_up_command(&stale_manifest, &stale_authority, key.clone(), "stale");
    let stale_counts = (
        table_count(&fixture.paths, "execass_plan_amendments"),
        table_count(&fixture.paths, "execass_receipts"),
        table_count(&fixture.paths, "execass_authority_provenance"),
    );
    assert!(matches!(
        fixture
            .store
            .apply_verified_follow_up_amendment(
                &integrity,
                &redactor,
                &stale_command,
                &stale_authority,
                &stale_manifest,
                &stale_proof,
            )
            .unwrap(),
        VerifiedFollowUpAmendmentOutcome::Stale {
            current_state_revision: 2
        }
    ));
    assert_eq!(
        (
            table_count(&fixture.paths, "execass_plan_amendments"),
            table_count(&fixture.paths, "execass_receipts"),
            table_count(&fixture.paths, "execass_authority_provenance"),
        ),
        stale_counts,
        "a stale target must leave plan, receipt, and authority state untouched"
    );
    let mut conflict = command.clone();
    conflict.amendment.normalized_amendment = "changed material command".into();
    assert!(fixture
        .store
        .apply_verified_follow_up_amendment(
            &integrity,
            &redactor,
            &conflict,
            &follow_up_authority,
            &manifest,
            &proof,
        )
        .is_err());
    assert_eq!(grant_row(&fixture, &grant.grant_id), before);

    let (drift_fixture, drift_confirmation, drift_attestation, _) =
        super::tests::prepared_attested_confirmation();
    let DangerConfirmationResolutionOutcome::Confirmed(drift_grant) = drift_fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &drift_confirmation,
            &drift_attestation,
            1_800_000_000_020,
        )
        .unwrap()
    else {
        panic!("fixture must produce a driftable accepted grant")
    };
    follow_up_runtime(&drift_fixture);
    let (drift_integrity, drift_key) = trust_empty_receipt_history(&drift_fixture);
    let drift_authority = amendment_authority(
        "target-drift",
        "delegation-1",
        1,
        1,
        "follow-up amendment target-drift",
    );
    let drift_manifest = follow_up_manifest(drift_authority.clone(), "target-2");
    let drift_route: DangerRoute = super::tests::protected_system_danger_route(&drift_manifest);
    let drift_proof = super::tests::signed_danger_admission_for_routes(
        &drift_fixture.store,
        &drift_manifest,
        vec![drift_route],
    );
    let drift_command =
        follow_up_command(&drift_manifest, &drift_authority, drift_key, "target-drift");
    assert!(matches!(
        drift_fixture
            .store
            .apply_verified_follow_up_amendment(
                &drift_integrity,
                &redactor,
                &drift_command,
                &drift_authority,
                &drift_manifest,
                &drift_proof,
            )
            .unwrap(),
        VerifiedFollowUpAmendmentOutcome::Applied(_)
    ));
    let connection = open_sqlite_connection(&drift_fixture.paths.db_path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT invalidation_reason FROM execass_accepted_confirmation_grants WHERE grant_id=?1",
                [&drift_grant.grant_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        AcceptedConfirmationGrantInvalidation::MaterialTargetDrift.as_str()
    );

    let (removal_fixture, removal_confirmation, removal_attestation, _) =
        super::tests::prepared_attested_confirmation();
    let DangerConfirmationResolutionOutcome::Confirmed(removal_grant) = removal_fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &removal_confirmation,
            &removal_attestation,
            1_800_000_000_020,
        )
        .unwrap()
    else {
        panic!("fixture must produce a removable accepted grant")
    };
    follow_up_runtime(&removal_fixture);
    let (removal_integrity, removal_key) = trust_empty_receipt_history(&removal_fixture);
    let removal_authority = amendment_authority(
        "danger-removed",
        "delegation-1",
        1,
        1,
        "follow-up amendment danger-removed",
    );
    let removal_manifest = follow_up_manifest(removal_authority.clone(), "target-1");
    let ordinary_route = ordinary_follow_up_route(&removal_manifest);
    assert!(ordinary_route
        .confirmation_for_leaf(&removal_manifest.leaves()[0])
        .is_none());
    let removal_proof = super::tests::signed_danger_admission_for_routes(
        &removal_fixture.store,
        &removal_manifest,
        vec![ordinary_route],
    );
    let removal_command = follow_up_command(
        &removal_manifest,
        &removal_authority,
        removal_key,
        "danger-removed",
    );
    let removal_counts = (
        table_count(&removal_fixture.paths, "execass_decisions"),
        table_count(&removal_fixture.paths, "execass_confirmation_challenges"),
        table_count(&removal_fixture.paths, "execass_continuations"),
    );
    assert!(matches!(
        removal_fixture
            .store
            .apply_verified_follow_up_amendment(
                &removal_integrity,
                &redactor,
                &removal_command,
                &removal_authority,
                &removal_manifest,
                &removal_proof,
            )
            .unwrap(),
        VerifiedFollowUpAmendmentOutcome::Applied(_)
    ));
    assert_eq!(
        (
            table_count(&removal_fixture.paths, "execass_decisions"),
            table_count(&removal_fixture.paths, "execass_confirmation_challenges"),
            table_count(&removal_fixture.paths, "execass_continuations"),
        ),
        removal_counts,
        "danger removal creates neither a new decision/challenge nor a continuation"
    );
    let removal_connection = open_sqlite_connection(&removal_fixture.paths.db_path).unwrap();
    assert_eq!(
        removal_connection
            .query_row(
                "SELECT plan_revision FROM execass_decisions WHERE decision_id=?1",
                [&removal_confirmation.decision_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1,
        "the historical decision remains immutable and bound to the superseded plan"
    );
    assert_eq!(
        removal_connection
            .query_row(
                "SELECT current_plan_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .unwrap(),
        Some(2),
        "the replacement ordinary plan is current, so the old dangerous decision is not current work"
    );
    assert!(
        removal_connection
            .query_row(
                "SELECT invalidated_at FROM execass_accepted_confirmation_grants WHERE grant_id=?1",
                [&removal_grant.grant_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .unwrap()
            .is_none(),
        "removing danger creates no replacement or generic grant"
    );

    let rollback_fixture = super::tests::fixture();
    rollback_fixture
        .store
        .create_foundation(&foundation())
        .unwrap();
    follow_up_runtime(&rollback_fixture);
    let (rollback_integrity, rollback_key) = trust_empty_receipt_history(&rollback_fixture);
    let rollback_authority = amendment_authority(
        "rollback",
        "delegation-1",
        1,
        1,
        "follow-up amendment collision",
    );
    for (suffix, authority, expected_error) in [
        (
            "wrong-target",
            amendment_authority(
                "wrong-target",
                "another-delegation",
                1,
                1,
                "follow-up amendment wrong-target",
            ),
            "wrong target",
        ),
        (
            "wrong-revision",
            amendment_authority(
                "wrong-revision",
                "delegation-1",
                99,
                1,
                "follow-up amendment wrong-revision",
            ),
            "wrong revision",
        ),
        (
            "wrong-intent",
            amendment_authority(
                "wrong-intent",
                "delegation-1",
                1,
                1,
                "different normalized amendment",
            ),
            "wrong normalized amendment",
        ),
    ] {
        let manifest = follow_up_manifest(authority.clone(), "target-1");
        let proof = super::tests::signed_danger_admission_for_routes(
            &rollback_fixture.store,
            &manifest,
            vec![super::tests::protected_system_danger_route(&manifest)],
        );
        let rejected = follow_up_command(&manifest, &authority, rollback_key.clone(), suffix);
        assert!(
            rollback_fixture
                .store
                .apply_verified_follow_up_amendment(
                    &rollback_integrity,
                    &redactor,
                    &rejected,
                    &authority,
                    &manifest,
                    &proof,
                )
                .is_err(),
            "{expected_error} must be rejected"
        );
        assert_eq!(
            table_count(&rollback_fixture.paths, "execass_plan_amendments"),
            0,
            "{expected_error} must not mutate the amendment history"
        );
        assert_eq!(
            table_count(&rollback_fixture.paths, "execass_receipts"),
            0,
            "{expected_error} must not append a receipt"
        );
    }
    let rollback_manifest = follow_up_manifest(rollback_authority.clone(), "target-1");
    let rollback_proof = super::tests::signed_danger_admission_for_routes(
        &rollback_fixture.store,
        &rollback_manifest,
        vec![super::tests::protected_system_danger_route(
            &rollback_manifest,
        )],
    );
    let mut collision = follow_up_command(
        &rollback_manifest,
        &rollback_authority,
        rollback_key,
        "collision",
    );
    collision.amendment.outbox_event.event_id = "event-foundation-1".into();
    collision.receipt.causation_event_id = "event-foundation-1".into();
    assert!(rollback_fixture
        .store
        .apply_verified_follow_up_amendment(
            &rollback_integrity,
            &redactor,
            &collision,
            &rollback_authority,
            &rollback_manifest,
            &rollback_proof,
        )
        .is_err());
    assert_eq!(
        table_count(&rollback_fixture.paths, "execass_plan_amendments"),
        0,
        "outbox conflict cannot leave a partial amendment"
    );
}

#[test]
fn verified_follow_up_amendment_supersedes_a_pending_danger_decision_and_all_old_work() {
    let (fixture, pending_confirmation, attestation, _) =
        super::tests::prepared_attested_confirmation();
    let mut attention_transition = command("pending-danger-attention", 1, RunControlState::Running);
    attention_transition
        .attention_items
        .push(attention("pending-danger-attention", 2));
    assert!(matches!(
        fixture
            .store
            .apply_lifecycle_snapshot(&attention_transition)
            .unwrap(),
        LifecycleWriteOutcome::Applied(_)
    ));

    let before = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(
        before
            .query_row(
                "SELECT status FROM execass_decisions WHERE decision_id=?1",
                [&pending_confirmation.decision_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "pending"
    );
    assert_eq!(
        before
            .query_row(
                "SELECT status FROM execass_attention_items WHERE attention_id='pending-danger-attention'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "actionable"
    );
    drop(before);

    follow_up_runtime(&fixture);
    let (integrity, key) = trust_empty_receipt_history(&fixture);
    let authority = amendment_authority(
        "pending-danger",
        "delegation-1",
        2,
        1,
        "follow-up amendment pending-danger",
    );
    let manifest = follow_up_manifest(authority.clone(), "target-1");
    let proof = super::tests::signed_danger_admission_for_routes(
        &fixture.store,
        &manifest,
        vec![super::tests::protected_system_danger_route(&manifest)],
    );
    let mut amendment = follow_up_command(&manifest, &authority, key, "pending-danger");
    amendment.amendment.expected_state_revision = 2;
    amendment.amendment.plan.based_on_delegation_revision = 3;
    amendment.amendment.outbox_event.aggregate_revision = 3;
    amendment.receipt.expected_state_revision = 3;
    let redactor = ReceiptRedactor::new(&["pending-danger-secret"]).unwrap();
    assert!(matches!(
        fixture
            .store
            .apply_verified_follow_up_amendment(
                &integrity, &redactor, &amendment, &authority, &manifest, &proof,
            )
            .unwrap(),
        VerifiedFollowUpAmendmentOutcome::Applied(_)
    ));

    let after = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    assert_eq!(
        after
            .query_row(
                "SELECT status FROM execass_decisions WHERE decision_id=?1",
                [&pending_confirmation.decision_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "superseded"
    );
    for (table, id_column, id) in [
        (
            "execass_attention_items",
            "attention_id",
            "pending-danger-attention",
        ),
        ("execass_action_branches", "action_id", "action-1"),
        ("execass_continuations", "continuation_id", "continuation-1"),
    ] {
        assert_eq!(
            after
                .query_row(
                    &format!("SELECT status FROM {table} WHERE {id_column}=?1"),
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "superseded",
            "{table} must be superseded in the same amendment transaction"
        );
    }
    assert!(after
        .query_row(
            "SELECT pending_decision_id FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap()
        .is_none());
    drop(after);
    assert!(fixture
        .store
        .confirm_dangerous_action_attested_at_for_test(
            &pending_confirmation,
            &attestation,
            1_800_000_000_020,
        )
        .is_err());
}

fn grant_row(fixture: &super::tests::Fixture, grant_id: &str) -> String {
    let connection = open_sqlite_connection(&fixture.paths.db_path).unwrap();
    connection
        .query_row(
            "SELECT quote(grant_id)||'|'||quote(delegation_id)||'|'||quote(decision_id)||'|'||quote(confirmed_logical_action_identity)||'|'||quote(canonical_action_envelope_or_selector_json)||'|'||quote(payload_and_material_operands_json)||'|'||quote(payload_and_material_operands_digest)||'|'||quote(connector_tool_identity)||'|'||quote(connector_tool_version)||'|'||quote(declared_consequence)||'|'||quote(accepted_by_authority_provenance_id)||'|'||quote(confirmation_attestation_digest)||'|'||quote(accepted_at)||'|'||quote(invalidated_at)||'|'||quote(invalidation_reason)||'|'||quote(invalidated_by_authority_provenance_id) FROM execass_accepted_confirmation_grants WHERE grant_id=?1",
            [grant_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn restart_reloads_the_same_live_projection_and_transition_history_is_immutable() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let applied = fixture
        .store
        .apply_lifecycle_snapshot(&command("restart", 1, RunControlState::Running))
        .unwrap();
    let LifecycleWriteOutcome::Applied(snapshot) = applied else {
        panic!("snapshot")
    };
    let reopened = ExecAssStore::open(&fixture.paths).unwrap();
    assert_eq!(
        reopened
            .read_foundation("delegation-1")
            .unwrap()
            .unwrap()
            .delegation,
        snapshot.delegation
    );
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    assert!(conn.execute("UPDATE execass_lifecycle_transitions SET reason='forged' WHERE transition_id='transition-restart'", []).is_err());
}

#[test]
fn destructive_rebuild_restores_all_mutable_lifecycle_subprojections() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let mut projection = command("rebuild", 1, RunControlState::Running);
    projection
        .attention_items
        .push(attention("attention-rebuild", 2));
    projection
        .external_waits
        .push(external_wait("wait-rebuild", 2));
    let LifecycleWriteOutcome::Applied(expected) =
        fixture.store.apply_lifecycle_snapshot(&projection).unwrap()
    else {
        panic!("rebuild source snapshot")
    };
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    conn.execute("UPDATE execass_action_branches SET status='waiting',updated_at=updated_at+10 WHERE action_id='action-1'", []).unwrap();
    conn.execute("UPDATE execass_continuations SET status='waiting',updated_at=updated_at+10 WHERE continuation_id='continuation-1'", []).unwrap();
    conn.execute("UPDATE execass_attention_items SET status='resolved',resolved_at=created_at+10 WHERE attention_id='attention-rebuild'", []).unwrap();
    conn.execute("UPDATE execass_external_waits SET status='resolved',resolved_at=created_at+10 WHERE external_wait_id='wait-rebuild'", []).unwrap();
    conn.execute("INSERT INTO execass_attention_items (attention_id,delegation_id,kind,status,reason,recommendation,alternatives_json,required_assurance,delegation_revision,created_at) VALUES ('extra-attention','delegation-1','clarification','actionable','extra','ignore','[]','human_local_or_remote',2,1800000009000)", []).unwrap();
    conn.execute("INSERT INTO execass_external_waits (external_wait_id,delegation_id,kind,status,reason,details_json,delegation_revision,created_at) VALUES ('extra-wait','delegation-1','system','waiting','extra','{}',2,1800000009000)", []).unwrap();
    drop(conn);

    let rebuilt = fixture
        .store
        .rebuild_lifecycle_projection("delegation-1")
        .unwrap()
        .unwrap();
    assert_eq!(rebuilt, expected.delegation);
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_action_branches WHERE action_id='action-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "runnable"
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_continuations WHERE continuation_id='continuation-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "runnable"
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_attention_items WHERE attention_id='attention-rebuild'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "actionable"
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM execass_external_waits WHERE external_wait_id='wait-rebuild'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "waiting"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_attention_items WHERE attention_id='extra-attention'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM execass_external_waits WHERE external_wait_id='extra-wait'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn criteria_sets_reject_orphan_and_mixed_revision_parentage() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let mut conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn.execute("UPDATE execass_delegations SET current_criteria_revision=99 WHERE delegation_id='delegation-1'", []).is_err());
    {
        let tx = conn.transaction().unwrap();
        tx.execute("INSERT INTO execass_criteria_sets (criteria_set_id,delegation_id,criteria_revision,parent_criteria_revision,disposition,created_at) VALUES ('orphan-set','delegation-1',3,2,'current',1800000030000)", []).unwrap();
        assert!(tx.commit().is_err());
    }
    {
        let tx = conn.transaction().unwrap();
        tx.execute("INSERT INTO execass_outcome_criteria (criterion_id,delegation_id,criteria_revision,criterion_key,description,material,verifier_type,expected_predicate_json,authoritative_source_kind,created_at) VALUES ('orphan-criterion','delegation-1',2,'orphan','orphan',1,'artifact','{}','artifact',1800000030000)", []).unwrap();
        assert!(tx.commit().is_err());
    }
}

#[test]
fn current_continuation_is_unique_and_requires_a_current_runnable_branch() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let conn = rusqlite::Connection::open(&fixture.paths.db_path).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn.execute("INSERT INTO execass_action_branches (action_id,delegation_id,action_revision,target_delegation_revision,target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at) VALUES ('action-2','delegation-1',2,1,1,0,'ordinary','runnable','second',1800000040000,1800000040000)", []).unwrap();
    assert!(conn.execute("INSERT INTO execass_continuations (continuation_id,delegation_id,target_delegation_revision,target_plan_revision,action_id,branch_kind,causation_kind,causation_id,status,fencing_token,host_generation,stop_epoch,global_stop_epoch,created_at,updated_at) VALUES ('continuation-2','delegation-1',1,1,'action-2','ordinary','plan','second-cause','runnable',0,1,0,0,1800000040000,1800000040000)", []).is_err());
    drop(conn);

    let second_fixture = super::tests::fixture();
    let mut base = foundation();
    base.initial_continuation = None;
    second_fixture.store.create_foundation(&base).unwrap();
    let mut invalid = command("terminal-branch", 1, RunControlState::Running);
    let mut branch = action(
        "terminal-action",
        1,
        ActionBranchKind::Ordinary,
        ContinuationStatus::Terminal,
    );
    branch.target_delegation_revision = 2;
    invalid.action_branches.push(branch);
    invalid
        .attention_items
        .push(attention("terminal-attention", 2));
    invalid.continuation = Some(ContinuationRecord {
        continuation_id: "terminal-continuation".into(),
        delegation_id: "delegation-1".into(),
        target_delegation_revision: 2,
        target_plan_revision: 1,
        action_id: "terminal-action".into(),
        branch_kind: ActionBranchKind::Ordinary,
        causation_kind: ContinuationCausationKind::Plan,
        causation_id: "terminal-cause".into(),
        status: ContinuationStatus::Runnable,
        job_id: None,
        lease_owner: None,
        lease_expires_at: None,
        fencing_token: 0,
        host_generation: 1,
        stop_epoch: 0,
        global_stop_epoch: 0,
        created_at: 1_800_000_040_000,
        updated_at: 1_800_000_040_000,
        completed_at: None,
    });
    assert!(second_fixture
        .store
        .apply_lifecycle_snapshot(&invalid)
        .is_err());
}
