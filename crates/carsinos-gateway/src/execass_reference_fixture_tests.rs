// EA-312's deterministic fresh-root composed reference fixture. It lives
// beneath the gateway's existing test module so it uses the real app router,
// owner-proof verifier, confirmation runtime, scheduler, storage services,
// projection, and receipt chain rather than introducing a second authority.

use super::*;

#[tokio::test]
async fn execass_reference_fixture_coherent_lifecycle_executes_exact_dangerous_leaf_once() {
    let ctx = execass_global_control_test_context();
    let store = ctx
        .state
        .execass_store
        .as_ref()
        .expect("canonical ExecAss store")
        .clone();
    let runtime = ctx
        .state
        .execass_confirmation_runtime
        .as_ref()
        .expect("canonical ExecAss runtime")
        .clone();
    let direct_secret = "sk-proj-ea312-reference-fixture-direct-secret";
    let root = ctx._temp_dir.path().join("execass-global-control");
    let paths = AppPaths::from_root(root.clone());

    // An exact conversational request answers inline and creates no work.
    let baseline_delegations = reference_table_count(&paths, "execass_delegations");
    let conversation = carsinos_protocol::execass::IntakeRequest {
        request_id: "ea312-conversation-request".into(),
        idempotency_key: "ea312-conversation-idempotency".into(),
        text: "hello".into(),
        source_correlation_id: "ea312-conversation-correlation".into(),
        attach_to_delegation_id: None,
    };
    let response = ctx
        .app
        .clone()
        .oneshot(execass_intake_request(
            &conversation,
            &signed_execass_local_intake(&conversation),
        ))
        .await
        .expect("canonical conversational response");
    assert_eq!(response.status(), StatusCode::OK);
    let conversation_body = parse_json(response).await;
    assert_eq!(conversation_body["kind"], "conversational");
    assert_eq!(
        conversation_body["response_text"],
        "Hello! What can I help you with?"
    );
    assert_eq!(
        reference_table_count(&paths, "execass_delegations"),
        baseline_delegations,
        "conversation must not create a delegation"
    );

    // Exact ordinary owner actions across every formerly suspect category
    // proceed without a prompt, approval object, or veto.
    let cases = vec![
        (
            "communication",
            "send the bounded project update to the already selected team channel".to_string(),
        ),
        (
            "permission_change",
            "grant the approved project workspace contributor permission".to_string(),
        ),
        (
            "project_mutation",
            "update the existing project task with the verified implementation note".to_string(),
        ),
        (
            "narrow_deletion",
            "delete the single explicitly selected temporary build artifact".to_string(),
        ),
        (
            "direct_secret_delivery",
            format!("deliver the direct secret {direct_secret} to the exact selected recipient"),
        ),
        (
            "purchase_like_existing_tool",
            "use the existing purchase_item tool for the exact owner-selected item".to_string(),
        ),
    ];
    let mut ordinary_delegation_ids = Vec::new();
    for (scenario, text) in &cases {
        let request = carsinos_protocol::execass::IntakeRequest {
            request_id: format!("ea312-{scenario}-request"),
            idempotency_key: format!("ea312-{scenario}-idempotency"),
            text: text.clone(),
            source_correlation_id: format!("ea312-{scenario}-correlation"),
            attach_to_delegation_id: None,
        };
        let proof = signed_execass_local_intake(&request);
        let response = ctx
            .app
            .clone()
            .oneshot(execass_intake_request(&request, &proof))
            .await
            .expect("canonical ordinary intake response");
        assert_eq!(response.status(), StatusCode::OK, "{scenario} must admit");
        let body = parse_json(response).await;
        assert_eq!(body["kind"], "delegation", "{scenario} must be durable");
        let delegation_id = body["delegation"]["delegation_id"]
            .as_str()
            .expect("durable intake delegation id")
            .to_string();
        assert!(
            body["delegation"]["pending_decision"].is_null(),
            "{scenario} must not create permission theater"
        );
        let detail = store
            .read_api_delegation_detail(&delegation_id)
            .expect("canonical delegation detail")
            .expect("admitted ordinary delegation exists");
        assert!(detail.delegation.pending_decision_id.is_none());
        assert!(!detail
            .delegation
            .normalized_original_intent
            .contains(direct_secret));
        ordinary_delegation_ids.push(delegation_id);
    }
    assert_eq!(
        reference_decision_count_for(&paths, &ordinary_delegation_ids),
        0,
        "ordinary prompt count must be zero"
    );

    // Build one real fixed dangerous leaf against a disposable project-drive
    // file, then admit it through the production manifest compiler and known-
    // danger matcher. The replacement bytes are not a simulated provider result.
    let exact_decision_id = "ea312-exact-overwrite-decision";
    let exact_action_id = "ea312-exact-overwrite-action";
    let exact_delegation_id = format!("exact-overwrite-delegation-{exact_decision_id}");
    let exact_target = root.join("ea312-disposable-recovery-path.bin");
    let exact_original = b"ea312-original-recovery-material";
    let exact_replacement = b"ea312-confirmed-destroyed-material";
    std::fs::write(&exact_target, exact_original).expect("write disposable exact target");
    let exact_material = carsinos_effect_recorder::build_exact_overwrite_material(
        &exact_target,
        exact_replacement,
    )
    .expect("build fixed exact-overwrite material");
    let exact_operand: carsinos_effect_recorder::ExactOverwriteOperandV1 =
        serde_json::from_value(exact_material.operand_envelope.non_secret.clone())
            .expect("decode fixed exact-overwrite operand");
    let danger_requested_at = current_time_ms();
    store
        .prepare_test_exact_overwrite_confirmation_runtime_projection(
            exact_decision_id,
            exact_action_id,
            &exact_operand.target_path,
            &exact_operand.target_identity,
            &exact_operand.expected_preimage_sha256,
            &exact_operand.replacement_hex,
            &exact_operand.replacement_sha256,
            danger_requested_at,
            danger_requested_at + 120_000,
        )
        .expect("admit exact overwrite through canonical danger path");

    // The known dangerous action has exactly one pending consequence prompt.
    let selected = match store
        .read_danger_confirmation_runtime_projection(
            exact_decision_id,
            exact_action_id,
        )
        .expect("read dangerous confirmation")
        .expect("dangerous confirmation exists")
    {
        carsinos_storage::execass::DangerConfirmationRuntimeProjection::Pending(value) => *value,
        other => panic!("expected one pending dangerous confirmation, got {other:?}"),
    };
    assert_eq!(
        reference_pending_danger_count(&paths, &exact_delegation_id),
        1,
        "dangerous prompt count must be exactly one"
    );
    assert!(!selected.declared_consequence.trim().is_empty());

    let decision_correlation = "ea312-danger-decision-correlation";
    let decision_idempotency = "ea312-danger-decision-idempotency";
    let decision_binding = carsinos_protocol::execass::LocalDecisionProofBinding {
        decision_id: selected.decision_id.clone(),
        decision_revision: u64::try_from(selected.decision_revision).unwrap(),
        normalized_intent_digest: carsinos_core::execass_actor::owner_normalized_intent_digest(
            &selected.normalized_intent,
        )
        .unwrap(),
        policy_revision: selected.policy_revision,
        canonical_manifest_digest: selected.manifest_digest.clone(),
        selected_logical_action_id: selected.selected_logical_action_id.clone(),
        presented_action_digest: selected.exact_selected_action_digest.clone(),
        declared_consequence_digest: selected.declared_consequence_digest.clone(),
        challenge_digest: selected.challenge_nonce_digest.clone(),
        expires_at_ms: selected.expires_at,
        response_selected_logical_action_id: selected.selected_logical_action_id.clone(),
        decision_result: carsinos_protocol::execass::DecisionResult::ConfirmAndContinue,
        idempotency_key: decision_idempotency.into(),
        revision_text_digest: None,
        challenge_response_digest: None,
        observed_at_ms: current_time_ms(),
    };
    let decision_request = carsinos_protocol::execass::ResolveDecisionRequest {
        idempotency_key: decision_idempotency.into(),
        decision_revision: selected.decision_revision,
        result: carsinos_protocol::execass::DecisionResult::ConfirmAndContinue,
        revision_text: None,
        challenge_response: None,
        local_proof: signed_execass_local_decision(&decision_binding, decision_correlation),
        local_proof_binding: decision_binding,
    };
    let mut confirmed_continuation_id = None;
    for _ in 0..2 {
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/execass/decisions/{exact_decision_id}/resolve"))
                    .header("authorization", "Bearer test-token")
                    .header("content-type", "application/json")
                    .header("idempotency-key", decision_idempotency)
                    .body(Body::from(serde_json::to_vec(&decision_request).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("dangerous decision response");
        let response_status = response.status();
        let body = parse_json(response).await;
        assert_eq!(response_status, StatusCode::OK, "{body}");
        assert_eq!(body["decision"]["result"], "confirm_and_continue");
        let continuation_id = body["continuation_id"]
            .as_str()
            .expect("confirmation continuation")
            .to_string();
        assert_eq!(
            confirmed_continuation_id.get_or_insert(continuation_id.clone()),
            &continuation_id,
            "decision replay must return the same continuation"
        );
    }
    let confirmed_continuation_id = confirmed_continuation_id.unwrap();
    let exact_execution_material = store
        .read_exact_dangerous_effect_execution_material(
            &exact_delegation_id,
            &confirmed_continuation_id,
        )
        .expect("read exact execution material")
        .expect("resolved exact confirmation must expose one storage-owned recorder operand");
    assert_eq!(
        exact_execution_material.operand_envelope,
        exact_material.operand_envelope
    );
    let detail = store
        .read_api_delegation_detail(&exact_delegation_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        detail
            .continuations
            .iter()
            .filter(|item| item.causation_id == exact_decision_id)
            .count(),
        1,
        "dangerous confirmation must create one continuation"
    );

    // The shared scheduler claims the one continuation and routes it through
    // the production fixed recorder. Exact replay creates no second claim,
    // continuation, effect, provider attempt, or file mutation.
    store
        .materialize_runnable_continuation_jobs(current_time_ms(), 100)
        .expect("materialize dangerous continuation job");
    let scheduler = carsinos_storage::Storage::from_paths(&paths);
    let claim_now = current_time_ms() + 1;
    let job = scheduler
        .acquire_due_jobs("ea312-danger-worker", claim_now, 30_000, 100)
        .expect("acquire dangerous continuation job")
        .into_iter()
        .find(|job| {
            carsinos_storage::execass::is_execass_continuation_job_payload(&job.payload_json)
                && job.payload_json.contains(&confirmed_continuation_id)
        })
        .expect("dangerous continuation scheduler job");
    execute_execass_continuation_once(&ctx.state, &job, "ea312-danger-worker")
        .await
        .expect("execute dangerous continuation through canonical scheduler");
    execute_execass_continuation_once(&ctx.state, &job, "ea312-danger-worker")
        .await
        .expect("replay dangerous continuation through canonical scheduler");
    let recorder_requests = ctx
        .execass_recorder_requests
        .as_ref()
        .expect("reference fixture must retain direct recorder request evidence")
        .lock()
        .await
        .clone();
    assert_eq!(
        recorder_requests.len(),
        1,
        "canonical execution plus replay must physically reach the recorder exactly once"
    );
    assert!(
        matches!(
            recorder_requests[0],
            carsinos_protocol::execass_recorder::RecorderRequestV1::ExecuteOnce(_)
        ),
        "the sole physical recorder request must be the first-authorizing ExecuteOnce"
    );
    let integrity = store.open_receipt_integrity_store().unwrap();
    let redactor = carsinos_storage::execass::ReceiptRedactor::new(&[direct_secret]).unwrap();
    let executed_detail = store
        .read_api_delegation_detail(&exact_delegation_id)
        .unwrap()
        .unwrap();
    let executed = executed_detail
        .continuations
        .iter()
        .find(|item| item.continuation_id == confirmed_continuation_id)
        .expect("executed dangerous continuation remains in lineage");
    assert_eq!(
        std::fs::read(&exact_target).expect("read executed exact target"),
        exact_replacement,
        "confirmed exact leaf must change the disposable target"
    );
    assert_eq!(
        executed.status,
        carsinos_storage::execass::ContinuationStatus::Terminal
    );
    assert_eq!(reference_table_count(&paths, "execass_logical_effects"), 1);
    assert_eq!(reference_table_count(&paths, "execass_provider_attempts"), 1);
    assert_eq!(
        reference_continuation_claim_count(&paths, &confirmed_continuation_id),
        1,
        "dangerous continuation must be claimed exactly once"
    );

    // Stop and resume the same delegation with exact owner proof and snapshot.
    let stop_now = claim_now + 10;
    let stop_binding = RunControlRequestBinding::delegation_stop(
        exact_delegation_id.clone(),
        "ea312-delegation-stop-idempotency".into(),
        "ea312-delegation-stop-correlation".into(),
        stop_now,
    )
    .unwrap();
    let stop_request = DelegationRunControlRequest {
        local_proof: signed_execass_local_control(&stop_binding),
        binding: stop_binding.clone(),
    };
    for _ in 0..2 {
        let response = ctx
            .app
            .clone()
            .oneshot(execass_control_request(
                &format!("/api/v1/execass/delegations/{exact_delegation_id}/stop"),
                Body::from(serde_json::to_vec(&stop_request).unwrap()),
                stop_binding.idempotency_key(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(parse_json(response).await["run_control"], "stopped");
    }
    let stopped = runtime
        .read_delegation_control_status(&exact_delegation_id, stop_now + 1)
        .unwrap()
        .unwrap();
    let resume_binding = RunControlRequestBinding::delegation_resume(
        exact_delegation_id.clone(),
        "ea312-delegation-resume-idempotency".into(),
        "ea312-delegation-resume-correlation".into(),
        stop_now + 1,
        RunControlResumeSnapshot::new(
            stopped.stop_epoch,
            stopped.policy_revision,
            stopped.unresolved_external_effects_digest,
            Some(stopped.state_revision),
            stopped.current_plan_revision,
        )
        .unwrap(),
    )
    .unwrap();
    let response = ctx
        .app
        .clone()
        .oneshot(execass_control_request(
            &format!("/api/v1/execass/delegations/{exact_delegation_id}/resume"),
            Body::from(
                serde_json::to_vec(&DelegationRunControlRequest {
                    local_proof: signed_execass_local_control(&resume_binding),
                    binding: resume_binding.clone(),
                })
                .unwrap(),
            ),
            resume_binding.idempotency_key(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(parse_json(response).await["run_control"], "running");

    // External wait and recovery remain on that same original delegation.
    let recovery = store
        .apply_test_reference_wait_and_recovery(
            &exact_delegation_id,
            exact_action_id,
            stop_now + 2,
        )
        .expect("apply reference external wait and recovery");
    assert_eq!(
        recovery.waiting_phase,
        carsinos_storage::execass::DelegationPhase::WaitingExternal
    );
    assert_eq!(
        recovery.recovery_phase,
        carsinos_storage::execass::DelegationPhase::Recovering
    );

    // Terminal outcomes are distinct aggregates because terminal state is
    // immutable. Both are backed by canonical verifier receipts.
    let completed = store
        .create_test_reference_terminal_case(
            "completed",
            carsinos_storage::execass::CompletionAssessmentKind::Completed,
            stop_now + 100,
        )
        .expect("create completed reference case");
    let partial = store
        .create_test_reference_terminal_case(
            "partial",
            carsinos_storage::execass::CompletionAssessmentKind::PartiallyCompleted,
            stop_now + 1_000,
        )
        .expect("create partial reference case");
    assert_eq!(completed.verifier_result_ids.len(), 1);
    assert_eq!(partial.verifier_result_ids.len(), 2);
    assert!(completed.receipt_chain_count >= 2);
    assert!(partial.receipt_chain_count >= 3);

    // Recurrence is necessarily a linked new occurrence delegation. The
    // canonical routine admission service proves and replays that boundary.
    let routine = store
        .create_test_reference_recurring_occurrence(stop_now + 10_000)
        .expect("create recurring occurrence reference case");
    assert_ne!(
        routine.source_delegation_id,
        routine.occurrence_delegation_id
    );
    let occurrence_detail = store
        .read_api_delegation_detail(&routine.occurrence_delegation_id)
        .unwrap()
        .unwrap();
    assert!(occurrence_detail.continuations.iter().any(|item| {
        item.continuation_id == routine.occurrence_continuation_id
            && item.causation_id == routine.occurrence_id
            && item.job_id.is_some()
    }));

    // Live and rebuilt executive truth must be identical and receipt history
    // must cryptographically verify before the fixture may emit its honest
    // green status. The confirmed dangerous leaf has already crossed the real
    // recorder service and converged signed Present evidence.
    let query = carsinos_storage::execass::ExecAssProjectionQuery::new(stop_now + 20_000);
    let live = store
        .read_authoritative_projection(&integrity, &redactor, &query)
        .expect("read authoritative reference projection");
    let rebuilt = store
        .rebuild_authoritative_projection(&integrity, &redactor, &query)
        .expect("rebuild authoritative reference projection");
    assert_eq!(live, rebuilt);
    assert!(live
        .done_since_you_checked
        .iter()
        .any(|item| item.delegation_id == completed.delegation_id));
    assert!(live
        .done_since_you_checked
        .iter()
        .any(|item| item.delegation_id == partial.delegation_id));
    assert!(matches!(
        integrity.status().unwrap(),
        carsinos_storage::execass::IntegrityStatus::Trusted { .. }
    ));
    assert_eq!(
        reference_orphan_runnable_count(&paths),
        0,
        "every runnable continuation must have a scheduler job"
    );
    assert_eq!(
        reference_pending_danger_count(&paths, &exact_delegation_id),
        0,
        "resolved danger must not prompt again"
    );
    let variants = direct_secret_canary_variants(direct_secret);
    assert_direct_secret_absent_from_tree(ctx._temp_dir.path(), &variants);

    println!(
        "{}",
        serde_json::json!({
            "fixture": "execass.ea312.reference.v1",
            "status": "green",
            "fresh_root": root,
            "scenarios": {
                "conversational_response": true,
                "ordinary_no_prompt_categories": cases.iter().map(|(scenario, _)| *scenario).collect::<Vec<_>>(),
                "ordinary_prompt_count": 0,
                "dangerous_prompt_count": 1,
                "dangerous_confirmation_replay": true,
                "dangerous_continuation_claim_count": 1,
                "dangerous_action_executed": true,
                "dangerous_runtime_result": "signed_present_converged",
                "delegation_stop_resume": exact_delegation_id,
                "external_wait_id": recovery.external_wait_id,
                "recovery_continuation_id": recovery.recovery_continuation_id,
                "completed_delegation_id": completed.delegation_id,
                "partial_delegation_id": partial.delegation_id,
                "routine_source_delegation_id": routine.source_delegation_id,
                "routine_occurrence_id": routine.occurrence_id,
                "routine_occurrence_delegation_id": routine.occurrence_delegation_id,
                "authoritative_projection_rebuild_equal": true,
                "receipt_chain_verified": true,
                "secret_persistence_scan": "clean",
                "orphan_runnable_continuations": 0
            },
            "remaining": []
        })
    );
}

fn reference_table_count(paths: &AppPaths, table: &str) -> i64 {
    assert!(matches!(
        table,
        "execass_delegations"
            | "execass_continuations"
            | "execass_logical_effects"
            | "execass_provider_attempts"
    ));
    rusqlite::Connection::open(&paths.db_path)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn reference_decision_count_for(paths: &AppPaths, delegation_ids: &[String]) -> i64 {
    let connection = rusqlite::Connection::open(&paths.db_path).unwrap();
    delegation_ids
        .iter()
        .map(|delegation_id| {
            connection
                .query_row(
                    "SELECT COUNT(*) FROM execass_decisions WHERE delegation_id=?1",
                    [delegation_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
        })
        .sum()
}

fn reference_pending_danger_count(paths: &AppPaths, delegation_id: &str) -> i64 {
    rusqlite::Connection::open(&paths.db_path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM execass_decisions WHERE delegation_id=?1 AND decision_kind='dangerous_action_confirmation' AND status='pending'",
            [delegation_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn reference_orphan_runnable_count(paths: &AppPaths) -> i64 {
    rusqlite::Connection::open(&paths.db_path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM execass_continuations WHERE status='runnable' AND job_id IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn reference_continuation_claim_count(paths: &AppPaths, continuation_id: &str) -> i64 {
    rusqlite::Connection::open(&paths.db_path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM execass_continuation_operation_history WHERE continuation_id=?1 AND operation='claim'",
            [continuation_id],
            |row| row.get(0),
        )
        .unwrap()
}
