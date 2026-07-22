use super::routines_tests::{anchored_fixture, command, materialize};
use super::tests::{
    admitted_dispatch_with_authority, ordinary_danger_admission, prepared_saved_routine_grant,
    protected_system_danger_route, ready_manifest, signed_danger_admission_for_routes, table_count,
};
use super::*;
use carsinos_core::execass_danger::saved_routine_stable_leaf_digest;
use carsinos_core::execass_manifest::{
    rebind_persisted_manifest_for_routine_occurrence, DispatchAction, RoutineOccurrenceLeafBinding,
};
use std::sync::{Arc, Barrier};

const NOW: i64 = 1_800_000_000_000;

fn claimed_trigger(
    fixture: &super::tests::Fixture,
    occurrence: &RoutineOccurrenceRecord,
    worker_id: &str,
    trusted_now: i64,
) -> RoutineAdmissionRequest {
    let trigger_job_id: String = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT job_id FROM execass_routine_job_bindings WHERE occurrence_id=?1",
            [&occurrence.occurrence_id],
            |row| row.get(0),
        )
        .unwrap();
    let job = crate::Storage::from_paths(&fixture.paths)
        .acquire_due_jobs(worker_id, trusted_now, 30_000, 64)
        .unwrap()
        .into_iter()
        .find(|job| job.job_id == trigger_job_id)
        .expect("routine trigger must be due");
    RoutineAdmissionRequest {
        occurrence_id: occurrence.occurrence_id.clone(),
        trigger_job_id,
        trigger_lease_owner: job.lease_owner.unwrap(),
        trigger_lease_expires_at: job.lease_expires_at.unwrap(),
        trusted_now,
    }
}

#[test]
fn claimed_occurrence_atomically_creates_one_delegation_continuation_and_settlement() {
    let fixture = anchored_fixture();
    let baseline_delegations = table_count(&fixture.paths, "execass_delegations");
    let baseline_continuations = table_count(&fixture.paths, "execass_continuations");
    let mut dispatch = admitted_dispatch_with_authority("authority-1");
    let source_manifest = ready_manifest(&dispatch);
    let mut routine = command(&fixture, "routine-atomic");
    routine.version.stable_leaf_digest =
        saved_routine_stable_leaf_digest(&source_manifest.leaves()[0]);
    fixture.store.create_routine(&routine).unwrap();
    let occurrence = materialize(
        &fixture,
        "routine-atomic",
        NOW + 2 * 86_400_000,
        "driver-atomic",
    )
    .pop()
    .unwrap();
    if let DispatchAction::ResolvedLeaf(action) = &mut dispatch.nodes[0].action {
        action.logical_action_id =
            deterministic_routine_occurrence_action_id(&occurrence.occurrence_id);
    } else {
        panic!("expected resolved leaf");
    }
    let request = claimed_trigger(
        &fixture,
        &occurrence,
        "trigger-atomic",
        NOW + 2 * 86_400_000 + 1,
    );
    let plan = match fixture
        .store
        .plan_routine_occurrence_admission(&request)
        .unwrap()
    {
        RoutineAdmissionOutcome::Planned(plan) => plan,
        other => panic!("expected a fresh admission plan, got {other:?}"),
    };
    let target_snapshot = match &dispatch.nodes[0].action {
        DispatchAction::ResolvedLeaf(action) => action.target_snapshot.clone(),
        _ => panic!("expected resolved leaf"),
    };
    let manifest = rebind_persisted_manifest_for_routine_occurrence(
        plan.routine_version.resolved_leaf_manifest_json.as_bytes(),
        &plan.routine_version.manifest_digest,
        &[RoutineOccurrenceLeafBinding {
            persisted_logical_action_id: source_manifest.leaves()[0]
                .logical_action_id()
                .to_string(),
            occurrence_logical_action_id: deterministic_routine_occurrence_action_id(
                &occurrence.occurrence_id,
            ),
            target_snapshot,
        }],
    )
    .unwrap();
    assert_eq!(manifest, ready_manifest(&dispatch));
    let danger = ordinary_danger_admission(&fixture.store, &dispatch);

    let admitted = fixture
        .store
        .admit_claimed_routine_occurrence(&request, &manifest, &danger)
        .unwrap();
    let RoutineOccurrenceDispatchOutcome::Admitted {
        delegation_id,
        continuation_id,
        ..
    } = admitted
    else {
        panic!("expected one admitted occurrence");
    };
    assert_eq!(
        table_count(&fixture.paths, "execass_delegations"),
        baseline_delegations + 1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_continuations"),
        baseline_continuations + 1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_routine_trigger_operations"),
        2
    );
    let bound: (String, String, i64) = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT admitted_delegation_id,status,(SELECT enabled FROM jobs WHERE job_id=?2) FROM execass_routine_occurrences WHERE occurrence_id=?1",
            rusqlite::params![occurrence.occurrence_id, request.trigger_job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(bound, (delegation_id.clone(), "settled".into(), 0));
    let continuation: (String, String, String) = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT continuation_id,causation_kind,causation_id FROM execass_continuations WHERE continuation_id=?1",
            [&continuation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        continuation,
        (
            continuation_id,
            "routine_occurrence".into(),
            occurrence.occurrence_id.clone()
        )
    );

    let replay = fixture
        .store
        .admit_claimed_routine_occurrence(&request, &manifest, &danger)
        .unwrap();
    assert_eq!(
        replay,
        RoutineOccurrenceDispatchOutcome::Replayed {
            occurrence_id: occurrence.occurrence_id,
            delegation_id: delegation_id.clone(),
        }
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_delegations"),
        baseline_delegations + 1
    );
    let bindings = fixture
        .store
        .materialize_runnable_continuation_jobs(request.trusted_now + 1, 100)
        .unwrap();
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.delegation_id == delegation_id)
            .count(),
        1
    );
}

#[test]
fn stop_policy_and_stable_leaf_drift_create_zero_occurrence_work() {
    let fixture = anchored_fixture();
    let baseline_delegations = table_count(&fixture.paths, "execass_delegations");
    let baseline_continuations = table_count(&fixture.paths, "execass_continuations");
    let dispatch = admitted_dispatch_with_authority("authority-1");
    let manifest = ready_manifest(&dispatch);
    let mut routine = command(&fixture, "routine-refuse");
    routine.version.stable_leaf_digest = saved_routine_stable_leaf_digest(&manifest.leaves()[0]);
    fixture.store.create_routine(&routine).unwrap();
    let occurrence = materialize(
        &fixture,
        "routine-refuse",
        NOW + 2 * 86_400_000,
        "driver-refuse",
    )
    .pop()
    .unwrap();
    let request = claimed_trigger(
        &fixture,
        &occurrence,
        "trigger-refuse",
        NOW + 2 * 86_400_000 + 1,
    );
    fixture.store.connection().unwrap().execute(
        "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=global_stop_epoch+1,updated_at=?1 WHERE singleton=1",
        [request.trusted_now],
    ).unwrap();
    let danger = ordinary_danger_admission(&fixture.store, &dispatch);
    assert_eq!(
        fixture
            .store
            .admit_claimed_routine_occurrence(&request, &manifest, &danger)
            .unwrap(),
        RoutineOccurrenceDispatchOutcome::Refused {
            reason: "global_stop_engaged".into()
        }
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_delegations"),
        baseline_delegations
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_continuations"),
        baseline_continuations
    );
}

#[test]
fn dangerous_occurrence_requires_the_exact_pinned_active_saved_grant() {
    const ROUTINE_ID: &str = "routine-danger-atomic";
    const SELECTOR: &str = r#"{"selector":"all protected roots currently selected by owner"}"#;
    let (fixture, mut dispatch, grant_id) = prepared_saved_routine_grant(ROUTINE_ID, SELECTOR);
    let source_manifest = ready_manifest(&dispatch);
    let mut routine = command(&fixture, ROUTINE_ID);
    routine.version.saved_selector_json = SELECTOR.into();
    routine.version.stable_leaf_digest =
        saved_routine_stable_leaf_digest(&source_manifest.leaves()[0]);
    routine.version.accepted_confirmation_grant_id = Some(grant_id.clone());
    routine.version.saved_action_envelope_json = fixture
        .store
        .connection()
        .unwrap()
        .query_row(
            "SELECT canonical_action_envelope_or_selector_json FROM execass_accepted_confirmation_grants WHERE grant_id=?1",
            [&grant_id],
            |row| row.get(0),
        )
        .unwrap();
    fixture.store.create_routine(&routine).unwrap();

    let occurrence = materialize(
        &fixture,
        ROUTINE_ID,
        NOW + 2 * 86_400_000,
        "driver-danger-1",
    )
    .pop()
    .unwrap();
    if let DispatchAction::ResolvedLeaf(action) = &mut dispatch.nodes[0].action {
        action.logical_action_id =
            deterministic_routine_occurrence_action_id(&occurrence.occurrence_id);
    } else {
        panic!("expected resolved leaf");
    }
    let manifest = ready_manifest(&dispatch);
    let danger = signed_danger_admission_for_routes(
        &fixture.store,
        &manifest,
        vec![protected_system_danger_route(&manifest)],
    );
    let request = claimed_trigger(
        &fixture,
        &occurrence,
        "trigger-danger-1",
        NOW + 2 * 86_400_000 + 1,
    );
    assert!(matches!(
        fixture
            .store
            .admit_claimed_routine_occurrence(&request, &manifest, &danger)
            .unwrap(),
        RoutineOccurrenceDispatchOutcome::Admitted { .. }
    ));

    let second = materialize(
        &fixture,
        ROUTINE_ID,
        NOW + 3 * 86_400_000,
        "driver-danger-2",
    )
    .pop()
    .unwrap();
    if let DispatchAction::ResolvedLeaf(action) = &mut dispatch.nodes[0].action {
        action.logical_action_id =
            deterministic_routine_occurrence_action_id(&second.occurrence_id);
    } else {
        panic!("expected resolved leaf");
    }
    let second_manifest = ready_manifest(&dispatch);
    let second_danger = signed_danger_admission_for_routes(
        &fixture.store,
        &second_manifest,
        vec![protected_system_danger_route(&second_manifest)],
    );
    let second_request = claimed_trigger(
        &fixture,
        &second,
        "trigger-danger-2",
        NOW + 3 * 86_400_000 + 1,
    );
    let before = table_count(&fixture.paths, "execass_delegations");
    fixture
        .store
        .connection()
        .unwrap()
        .execute(
            "UPDATE execass_accepted_confirmation_grants SET invalidated_at=?1,invalidation_reason='material_target_drift' WHERE grant_id=?2",
            rusqlite::params![second_request.trusted_now, grant_id],
        )
        .unwrap();
    assert!(fixture
        .store
        .admit_claimed_routine_occurrence(&second_request, &second_manifest, &second_danger)
        .is_err());
    assert_eq!(table_count(&fixture.paths, "execass_delegations"), before);
}

#[test]
fn concurrent_claimed_trigger_replays_one_atomic_occurrence() {
    let fixture = anchored_fixture();
    let mut dispatch = admitted_dispatch_with_authority("authority-1");
    let source_manifest = ready_manifest(&dispatch);
    let mut routine = command(&fixture, "routine-trigger-race");
    routine.version.stable_leaf_digest =
        saved_routine_stable_leaf_digest(&source_manifest.leaves()[0]);
    fixture.store.create_routine(&routine).unwrap();
    let occurrence = materialize(
        &fixture,
        "routine-trigger-race",
        NOW + 2 * 86_400_000,
        "driver-trigger-race",
    )
    .pop()
    .unwrap();
    if let DispatchAction::ResolvedLeaf(action) = &mut dispatch.nodes[0].action {
        action.logical_action_id =
            deterministic_routine_occurrence_action_id(&occurrence.occurrence_id);
    } else {
        panic!("expected resolved leaf");
    }
    let manifest = ready_manifest(&dispatch);
    let danger = ordinary_danger_admission(&fixture.store, &dispatch);
    let request = claimed_trigger(
        &fixture,
        &occurrence,
        "trigger-race",
        NOW + 2 * 86_400_000 + 1,
    );
    let baseline = table_count(&fixture.paths, "execass_delegations");
    let barrier = Arc::new(Barrier::new(3));
    let workers = (0..2)
        .map(|_| {
            let paths = fixture.paths.clone();
            let barrier = Arc::clone(&barrier);
            let request = request.clone();
            let manifest = manifest.clone();
            let danger = danger.clone();
            std::thread::spawn(move || {
                let store = ExecAssStore::open(&paths).unwrap();
                barrier.wait();
                store.admit_claimed_routine_occurrence(&request, &manifest, &danger)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RoutineOccurrenceDispatchOutcome::Admitted { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RoutineOccurrenceDispatchOutcome::Replayed { .. }))
            .count(),
        1
    );
    assert_eq!(
        table_count(&fixture.paths, "execass_delegations"),
        baseline + 1
    );
    assert_eq!(
        fixture
            .store
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM execass_routine_trigger_operations WHERE occurrence_id=?1",
                [&occurrence.occurrence_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}
