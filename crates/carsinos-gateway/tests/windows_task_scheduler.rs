#[path = "../src/windows_task_scheduler.rs"]
#[allow(dead_code)]
mod windows_task_scheduler;

/// Destructive only to the exact CarsinOS per-user task. The caller must first
/// prove that task is absent (or preserve/restore it externally) and provide a
/// release-shaped gateway path. This is excluded from ordinary regression and
/// exists solely for EA-405/407 real Windows evidence.
#[cfg(windows)]
#[test]
#[ignore = "mutates the current user's exact CarsinOS Task Scheduler entry"]
fn real_current_user_scheduler_create_readback_disable_remove() {
    use windows_task_scheduler::{
        inspect_current_user, reconcile_current_user, remove_current_user, DesiredMode,
        ReconcileRequest, SchedulerOutcome,
    };

    let executable = std::env::var_os("CARSINOS_EA405_GATEWAY_EXE")
        .map(std::path::PathBuf::from)
        .expect("CARSINOS_EA405_GATEWAY_EXE must name the exact release-shaped gateway");
    let created = reconcile_current_user(ReconcileRequest {
        desired_mode: DesiredMode::Background,
        start_at_login: true,
        installed_gateway_executable: executable.clone(),
    });
    assert!(
        matches!(
            created.outcome,
            SchedulerOutcome::Created | SchedulerOutcome::Repaired | SchedulerOutcome::Unchanged
        ),
        "create/readback failed closed: {created:?}; mismatches={:?}; principal={:?}; sddl={:?}",
        windows_task_scheduler::inspect_current_user_contract_mismatches_for_test(),
        windows_task_scheduler::inspect_current_user_principal_for_test(),
        windows_task_scheduler::inspect_current_user_sddl_for_test()
    );
    let inspected = inspect_current_user();
    assert_eq!(inspected.outcome, SchedulerOutcome::Unchanged);
    assert_eq!(inspected.enabled, Some(true));

    let disabled = reconcile_current_user(ReconcileRequest {
        desired_mode: DesiredMode::Background,
        start_at_login: false,
        installed_gateway_executable: executable,
    });
    assert_eq!(disabled.outcome, SchedulerOutcome::Disabled);
    assert_eq!(inspect_current_user().enabled, Some(false));

    let removed = remove_current_user();
    assert!(matches!(
        removed.outcome,
        SchedulerOutcome::Removed | SchedulerOutcome::Missing
    ));
    assert_eq!(inspect_current_user().outcome, SchedulerOutcome::Missing);
}
