//! Current-user reconciliation for the one ExecAss Windows logon task.
//!
//! This module deliberately knows nothing about HTTP, storage transactions, or
//! credentials.  Its caller supplies an already-committed `(desired_mode,
//! start_at_login)` decision and the canonical installed gateway executable.
//! The only command line accepted by this adapter is a fixed, non-secret mode
//! flag; Scheduler is never a source of authority for ExecAss state.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) const TASK_URI: &str = r"\CarsinOS ExecAss Runtime";
pub(crate) const INSTALLED_RUNTIME_HOST_FLAG: &str = "--mission-control-runtime-host";
const TASK_SCHEMA_MARKER: &str = "CarsinOS ExecAss Runtime scheduler contract v1";
const RESTART_INTERVAL: &str = "PT1M";
const EXECUTION_TIME_LIMIT: &str = "PT0S";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesiredMode {
    AppBound,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconcileRequest {
    pub(crate) desired_mode: DesiredMode,
    pub(crate) start_at_login: bool,
    /// Absolute path from the installation boundary, never from PATH or UI input.
    pub(crate) installed_gateway_executable: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchedulerOperation {
    #[allow(dead_code)] // Retained for the EA-407 read-only platform evidence harness.
    Inspect,
    Reconcile,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchedulerOutcome {
    Missing,
    Created,
    Repaired,
    Unchanged,
    Disabled,
    Removed,
    IdentityConflict,
    PermissionDenied,
    SchedulerUnavailable,
    InvalidRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchedulerReceipt {
    pub(crate) operation: SchedulerOperation,
    pub(crate) outcome: SchedulerOutcome,
    pub(crate) task_uri: &'static str,
    pub(crate) enabled: Option<bool>,
    pub(crate) caller_sid_digest: Option<String>,
    /// A stable category only; this must never contain COM detail, XML, or secrets.
    pub(crate) failure_category: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskDefinition {
    task_uri: String,
    schema_marker: String,
    registration_owner_sid: String,
    principal_sid: String,
    interactive_token: bool,
    least_privilege: bool,
    logon_trigger_sids: Vec<String>,
    executable: PathBuf,
    arguments: Vec<String>,
    working_directory: PathBuf,
    enabled: bool,
    ignore_new_instances: bool,
    execution_time_limit: String,
    restart_count: u8,
    restart_interval: String,
    allow_demand_start: bool,
    run_only_if_idle: bool,
    run_only_if_network_available: bool,
    disallow_start_if_on_batteries: bool,
    stop_if_going_on_batteries: bool,
    wake_to_run: bool,
    hidden: bool,
    /// Scheduler's SDDL readback.  It is used only for management capability,
    /// never returned in a receipt.
    security_descriptor: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendFailure {
    NotFound,
    PermissionDenied,
    Unavailable,
    Malformed,
}

trait SchedulerBackend {
    fn current_user_sid(&mut self) -> std::result::Result<String, BackendFailure>;
    fn inspect_exact(
        &mut self,
        task_uri: &str,
    ) -> std::result::Result<Option<TaskDefinition>, BackendFailure>;
    fn register_or_update(
        &mut self,
        definition: &TaskDefinition,
    ) -> std::result::Result<(), BackendFailure>;
    fn remove_exact(&mut self, task_uri: &str) -> std::result::Result<(), BackendFailure>;
}

#[allow(dead_code)] // Retained for the EA-407 read-only platform evidence harness.
pub(crate) fn inspect_current_user() -> SchedulerReceipt {
    #[cfg(windows)]
    {
        match WindowsTaskScheduler::open() {
            Ok(mut scheduler) => inspect_with_backend(&mut scheduler),
            Err(error) => receipt_from_failure(SchedulerOperation::Inspect, error, None),
        }
    }
    #[cfg(not(windows))]
    receipt_from_failure(
        SchedulerOperation::Inspect,
        BackendFailure::Unavailable,
        None,
    )
}

/// Resolve the current process only when it is the installed gateway binary.
/// Test binaries and `target`/`debug` developer runs must never manage the
/// production user's scheduled task.
pub(crate) fn current_installed_gateway_executable() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| canonical_installed_gateway(&path).ok())
}

pub(crate) fn reconcile_current_user(request: ReconcileRequest) -> SchedulerReceipt {
    #[cfg(windows)]
    {
        match WindowsTaskScheduler::open() {
            Ok(mut scheduler) => reconcile_with_backend(&mut scheduler, request),
            Err(error) => receipt_from_failure(SchedulerOperation::Reconcile, error, None),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        receipt_from_failure(
            SchedulerOperation::Reconcile,
            BackendFailure::Unavailable,
            None,
        )
    }
}

pub(crate) fn remove_current_user() -> SchedulerReceipt {
    #[cfg(windows)]
    {
        match WindowsTaskScheduler::open() {
            Ok(mut scheduler) => remove_with_backend(&mut scheduler),
            Err(error) => receipt_from_failure(SchedulerOperation::Remove, error, None),
        }
    }
    #[cfg(not(windows))]
    receipt_from_failure(
        SchedulerOperation::Remove,
        BackendFailure::Unavailable,
        None,
    )
}

#[allow(dead_code)] // Called only by the read-only inspection API and its platform harness.
fn inspect_with_backend(backend: &mut impl SchedulerBackend) -> SchedulerReceipt {
    let sid = match backend.current_user_sid() {
        Ok(sid) => sid,
        Err(error) => return receipt_from_failure(SchedulerOperation::Inspect, error, None),
    };
    let sid_digest = sid_digest(&sid);
    match backend.inspect_exact(TASK_URI) {
        Ok(None) => SchedulerReceipt {
            operation: SchedulerOperation::Inspect,
            outcome: SchedulerOutcome::Missing,
            task_uri: TASK_URI,
            enabled: None,
            caller_sid_digest: Some(sid_digest),
            failure_category: None,
        },
        Ok(Some(task)) => match verify_managed_shape(&task, &sid) {
            Ok(()) => SchedulerReceipt {
                operation: SchedulerOperation::Inspect,
                outcome: SchedulerOutcome::Unchanged,
                task_uri: TASK_URI,
                enabled: Some(task.enabled),
                caller_sid_digest: Some(sid_digest),
                failure_category: None,
            },
            Err(VerificationFailure::Repairable) | Err(VerificationFailure::Conflict) => {
                SchedulerReceipt {
                    operation: SchedulerOperation::Inspect,
                    outcome: SchedulerOutcome::IdentityConflict,
                    task_uri: TASK_URI,
                    enabled: None,
                    caller_sid_digest: Some(sid_digest),
                    failure_category: Some("task_definition_mismatch"),
                }
            }
        },
        Err(error) => receipt_from_failure(SchedulerOperation::Inspect, error, Some(sid_digest)),
    }
}

fn reconcile_with_backend(
    backend: &mut impl SchedulerBackend,
    request: ReconcileRequest,
) -> SchedulerReceipt {
    let desired = match desired_definition(backend, &request) {
        Ok(value) => value,
        Err(error) => {
            return SchedulerReceipt {
                operation: SchedulerOperation::Reconcile,
                outcome: SchedulerOutcome::InvalidRequest,
                task_uri: TASK_URI,
                enabled: None,
                caller_sid_digest: None,
                failure_category: Some(error),
            }
        }
    };
    let sid_digest = sid_digest(&desired.principal_sid);
    let existing = match backend.inspect_exact(TASK_URI) {
        Ok(value) => value,
        Err(error) => {
            return receipt_from_failure(SchedulerOperation::Reconcile, error, Some(sid_digest))
        }
    };
    let initial_outcome = match existing {
        None => SchedulerOutcome::Created,
        Some(ref task) => match compare_for_reconcile(task, &desired) {
            Ok(()) => {
                return SchedulerReceipt {
                    operation: SchedulerOperation::Reconcile,
                    outcome: if desired.enabled {
                        SchedulerOutcome::Unchanged
                    } else {
                        SchedulerOutcome::Disabled
                    },
                    task_uri: TASK_URI,
                    enabled: Some(desired.enabled),
                    caller_sid_digest: Some(sid_digest),
                    failure_category: None,
                }
            }
            Err(VerificationFailure::Repairable) => SchedulerOutcome::Repaired,
            Err(VerificationFailure::Conflict) => {
                return SchedulerReceipt {
                    operation: SchedulerOperation::Reconcile,
                    outcome: SchedulerOutcome::IdentityConflict,
                    task_uri: TASK_URI,
                    enabled: None,
                    caller_sid_digest: Some(sid_digest),
                    failure_category: Some("foreign_or_malformed_task"),
                }
            }
        },
    };
    if let Err(error) = backend.register_or_update(&desired) {
        return receipt_from_failure(SchedulerOperation::Reconcile, error, Some(sid_digest));
    }
    match backend.inspect_exact(TASK_URI) {
        Ok(Some(readback)) if compare_for_reconcile(&readback, &desired).is_ok() => {
            SchedulerReceipt {
                operation: SchedulerOperation::Reconcile,
                outcome: if desired.enabled {
                    initial_outcome
                } else {
                    SchedulerOutcome::Disabled
                },
                task_uri: TASK_URI,
                enabled: Some(desired.enabled),
                caller_sid_digest: Some(sid_digest),
                failure_category: None,
            }
        }
        Ok(_) => SchedulerReceipt {
            operation: SchedulerOperation::Reconcile,
            outcome: SchedulerOutcome::IdentityConflict,
            task_uri: TASK_URI,
            enabled: None,
            caller_sid_digest: Some(sid_digest),
            failure_category: Some("post_registration_readback_mismatch"),
        },
        Err(error) => receipt_from_failure(SchedulerOperation::Reconcile, error, Some(sid_digest)),
    }
}

fn remove_with_backend(backend: &mut impl SchedulerBackend) -> SchedulerReceipt {
    let sid = match backend.current_user_sid() {
        Ok(sid) => sid,
        Err(error) => return receipt_from_failure(SchedulerOperation::Remove, error, None),
    };
    let sid_digest = sid_digest(&sid);
    let task = match backend.inspect_exact(TASK_URI) {
        Ok(None) => {
            return SchedulerReceipt {
                operation: SchedulerOperation::Remove,
                outcome: SchedulerOutcome::Removed,
                task_uri: TASK_URI,
                enabled: Some(false),
                caller_sid_digest: Some(sid_digest),
                failure_category: None,
            }
        }
        Ok(Some(task)) => task,
        Err(error) => {
            return receipt_from_failure(SchedulerOperation::Remove, error, Some(sid_digest))
        }
    };
    if verify_managed_shape(&task, &sid).is_err() {
        return SchedulerReceipt {
            operation: SchedulerOperation::Remove,
            outcome: SchedulerOutcome::IdentityConflict,
            task_uri: TASK_URI,
            enabled: None,
            caller_sid_digest: Some(sid_digest),
            failure_category: Some("foreign_or_malformed_task"),
        };
    }
    if task.enabled {
        let mut disabled = task.clone();
        disabled.enabled = false;
        if let Err(error) = backend.register_or_update(&disabled) {
            return receipt_from_failure(SchedulerOperation::Remove, error, Some(sid_digest));
        }
        match backend.inspect_exact(TASK_URI) {
            Ok(Some(readback))
                if verify_managed_shape(&readback, &sid).is_ok() && !readback.enabled => {}
            Ok(_) => {
                return SchedulerReceipt {
                    operation: SchedulerOperation::Remove,
                    outcome: SchedulerOutcome::IdentityConflict,
                    task_uri: TASK_URI,
                    enabled: None,
                    caller_sid_digest: Some(sid_digest),
                    failure_category: Some("disable_before_removal_readback_mismatch"),
                }
            }
            Err(error) => {
                return receipt_from_failure(SchedulerOperation::Remove, error, Some(sid_digest))
            }
        }
    }
    if let Err(error) = backend.remove_exact(TASK_URI) {
        return receipt_from_failure(SchedulerOperation::Remove, error, Some(sid_digest));
    }
    match backend.inspect_exact(TASK_URI) {
        Ok(None) => SchedulerReceipt {
            operation: SchedulerOperation::Remove,
            outcome: SchedulerOutcome::Removed,
            task_uri: TASK_URI,
            enabled: Some(false),
            caller_sid_digest: Some(sid_digest),
            failure_category: None,
        },
        Ok(Some(_)) => SchedulerReceipt {
            operation: SchedulerOperation::Remove,
            outcome: SchedulerOutcome::IdentityConflict,
            task_uri: TASK_URI,
            enabled: None,
            caller_sid_digest: Some(sid_digest),
            failure_category: Some("post_removal_readback_present"),
        },
        Err(error) => receipt_from_failure(SchedulerOperation::Remove, error, Some(sid_digest)),
    }
}

fn desired_definition(
    backend: &mut impl SchedulerBackend,
    request: &ReconcileRequest,
) -> std::result::Result<TaskDefinition, &'static str> {
    if request.start_at_login && request.desired_mode != DesiredMode::Background {
        return Err("invalid_mode_login_combination");
    }
    let executable = canonical_installed_gateway(&request.installed_gateway_executable)
        .map_err(|_| "invalid_installed_gateway_path")?;
    let working_directory = executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or("invalid_installed_gateway_path")?;
    let sid = backend
        .current_user_sid()
        .map_err(|_| "current_user_sid_unavailable")?;
    if sid.is_empty() {
        return Err("current_user_sid_unavailable");
    }
    Ok(TaskDefinition {
        task_uri: TASK_URI.to_owned(),
        schema_marker: TASK_SCHEMA_MARKER.to_owned(),
        registration_owner_sid: sid.clone(),
        principal_sid: sid.clone(),
        interactive_token: true,
        least_privilege: true,
        logon_trigger_sids: vec![sid.clone()],
        executable,
        arguments: vec![INSTALLED_RUNTIME_HOST_FLAG.to_owned()],
        working_directory,
        enabled: request.desired_mode == DesiredMode::Background && request.start_at_login,
        ignore_new_instances: true,
        execution_time_limit: EXECUTION_TIME_LIMIT.to_owned(),
        restart_count: 3,
        restart_interval: RESTART_INTERVAL.to_owned(),
        allow_demand_start: true,
        run_only_if_idle: false,
        run_only_if_network_available: false,
        disallow_start_if_on_batteries: false,
        stop_if_going_on_batteries: false,
        wake_to_run: false,
        hidden: false,
        security_descriptor: String::new(),
    })
}

fn canonical_installed_gateway(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("installed gateway executable must be absolute");
    }
    let canonical = path
        .canonicalize()
        .with_context(|| "failed canonicalizing installed gateway executable")?;
    let file_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase);
    if file_name.as_deref() != Some("carsinos-gateway.exe") {
        bail!("scheduler action must target the installed carsinos-gateway.exe");
    }
    if canonical
        .components()
        .any(|component| matches!(component.as_os_str().to_str(), Some("target" | "debug")))
    {
        bail!("scheduler action rejects development-target gateway paths");
    }
    Ok(canonical)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerificationFailure {
    Repairable,
    Conflict,
}

fn verify_managed_shape(
    task: &TaskDefinition,
    sid: &str,
) -> std::result::Result<(), VerificationFailure> {
    if task.task_uri != TASK_URI
        || task.schema_marker != TASK_SCHEMA_MARKER
        || task.registration_owner_sid != sid
        || task.principal_sid != sid
        || !task.interactive_token
        || !task.least_privilege
        || task.logon_trigger_sids != [sid]
        || task.arguments != [INSTALLED_RUNTIME_HOST_FLAG]
        || !task.ignore_new_instances
        || task.execution_time_limit != EXECUTION_TIME_LIMIT
        || task.restart_count != 3
        || task.restart_interval != RESTART_INTERVAL
        || !task.allow_demand_start
        || task.run_only_if_idle
        || task.run_only_if_network_available
        || task.disallow_start_if_on_batteries
        || task.stop_if_going_on_batteries
        || task.wake_to_run
        || task.hidden
        || !sddl_is_owner_system_only(&task.security_descriptor, sid)
        || !is_direct_gateway_action(&task.executable, &task.working_directory)
    {
        return Err(VerificationFailure::Conflict);
    }
    Ok(())
}

#[cfg(all(test, windows))]
#[allow(dead_code)] // Used by the ignored real-scheduler evidence harness crate.
pub(crate) fn inspect_current_user_contract_mismatches_for_test() -> Vec<&'static str> {
    let Ok(mut backend) = WindowsTaskScheduler::open() else {
        return vec!["scheduler_open"];
    };
    let Ok(sid) = backend.current_user_sid() else {
        return vec!["current_sid"];
    };
    let Ok(Some(task)) = backend.inspect_exact(TASK_URI) else {
        return vec!["task_readback"];
    };
    let mut mismatches = Vec::new();
    if task.task_uri != TASK_URI {
        mismatches.push("task_uri");
    }
    if task.schema_marker != TASK_SCHEMA_MARKER {
        mismatches.push("schema_marker");
    }
    if task.registration_owner_sid != sid {
        mismatches.push("registration_owner");
    }
    if task.principal_sid != sid {
        mismatches.push("principal");
    }
    if !task.interactive_token || !task.least_privilege {
        mismatches.push("principal_mode");
    }
    if task.logon_trigger_sids != [sid.as_str()] {
        mismatches.push("logon_trigger");
    }
    if task.arguments != [INSTALLED_RUNTIME_HOST_FLAG] {
        mismatches.push("arguments");
    }
    if !task.ignore_new_instances
        || task.execution_time_limit != EXECUTION_TIME_LIMIT
        || task.restart_count != 3
        || task.restart_interval != RESTART_INTERVAL
        || !task.allow_demand_start
        || task.run_only_if_idle
        || task.run_only_if_network_available
        || task.disallow_start_if_on_batteries
        || task.stop_if_going_on_batteries
        || task.wake_to_run
        || task.hidden
    {
        mismatches.push("settings");
    }
    if !sddl_is_owner_system_only(&task.security_descriptor, &sid) {
        mismatches.push("management_acl");
    }
    if !is_direct_gateway_action(&task.executable, &task.working_directory) {
        mismatches.push("direct_action");
    }
    mismatches
}

#[cfg(all(test, windows))]
#[allow(dead_code)] // Used by the ignored real-scheduler evidence harness crate.
pub(crate) fn inspect_current_user_principal_for_test() -> Option<String> {
    let mut backend = WindowsTaskScheduler::open().ok()?;
    backend.inspect_exact(TASK_URI).ok()??.principal_sid.into()
}

#[cfg(all(test, windows))]
#[allow(dead_code)] // Used by the ignored real-scheduler evidence harness crate.
pub(crate) fn inspect_current_user_sddl_for_test() -> Option<String> {
    let mut backend = WindowsTaskScheduler::open().ok()?;
    backend
        .inspect_exact(TASK_URI)
        .ok()??
        .security_descriptor
        .into()
}

fn is_direct_gateway_action(executable: &Path, working_directory: &Path) -> bool {
    executable.is_absolute()
        && executable
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("carsinos-gateway.exe"))
        && executable.parent() == Some(working_directory)
        && !executable.components().any(|component| {
            component.as_os_str().to_str().is_some_and(|part| {
                part.eq_ignore_ascii_case("target") || part.eq_ignore_ascii_case("debug")
            })
        })
}

fn sddl_grants_current_user_manage_access(sddl: &str, sid: &str) -> bool {
    // Task Scheduler's default per-user DACL has a full-access ACE for the
    // registering SID.  A mere SID mention is not enough: it could be a deny,
    // owner, or read-only ACE.  Be deliberately conservative for this one
    // task; an unusual-but-safe ACL is an attention/repair case, not a delete.
    sddl.split('(').any(|ace| {
        ace.starts_with("A;;FA;;;")
            && ace
                .strip_suffix(')')
                .is_some_and(|body| body.ends_with(sid))
    })
}

fn sddl_is_owner_system_only(sddl: &str, sid: &str) -> bool {
    if !sddl.contains("D:") || !sddl_grants_current_user_manage_access(sddl, sid) {
        return false;
    }
    let mut owner_full = false;
    let mut system_full = false;
    let mut ace_count = 0usize;
    for ace in sddl
        .split('(')
        .skip(1)
        .filter_map(|value| value.split(')').next())
    {
        ace_count = ace_count.saturating_add(1);
        let fields: Vec<&str> = ace.split(';').collect();
        if fields.len() != 6
            || fields[0] != "A"
            || !fields[1].is_empty()
            || !matches!(fields[2], "FA" | "GA" | "FR")
            || !fields[3].is_empty()
            || !fields[4].is_empty()
        {
            return false;
        }
        match fields[5] {
            value if value.eq_ignore_ascii_case(sid) => {
                owner_full |= matches!(fields[2], "FA" | "GA")
            }
            "SY" | "S-1-5-18" => system_full |= matches!(fields[2], "FA" | "GA"),
            _ => return false,
        }
    }
    ace_count >= 2 && owner_full && system_full
}

fn compare_for_reconcile(
    actual: &TaskDefinition,
    desired: &TaskDefinition,
) -> std::result::Result<(), VerificationFailure> {
    verify_managed_shape(actual, &desired.principal_sid)?;
    if actual.executable != desired.executable
        || actual.working_directory != desired.working_directory
        || actual.enabled != desired.enabled
    {
        return Err(VerificationFailure::Repairable);
    }
    Ok(())
}

fn sid_digest(sid: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(sid.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

fn receipt_from_failure(
    operation: SchedulerOperation,
    failure: BackendFailure,
    caller_sid_digest: Option<String>,
) -> SchedulerReceipt {
    let (outcome, failure_category) = match failure {
        BackendFailure::NotFound => (SchedulerOutcome::Missing, None),
        BackendFailure::PermissionDenied => (
            SchedulerOutcome::PermissionDenied,
            Some("permission_denied"),
        ),
        BackendFailure::Unavailable => (
            SchedulerOutcome::SchedulerUnavailable,
            Some("scheduler_unavailable"),
        ),
        BackendFailure::Malformed => (
            SchedulerOutcome::IdentityConflict,
            Some("malformed_scheduler_state"),
        ),
    };
    SchedulerReceipt {
        operation,
        outcome,
        task_uri: TASK_URI,
        enabled: None,
        caller_sid_digest,
        failure_category,
    }
}

#[cfg(windows)]
struct WindowsTaskScheduler {
    service: windows::Win32::System::TaskScheduler::ITaskService,
    _com: ComApartment,
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize() }
    }
}

#[cfg(windows)]
impl WindowsTaskScheduler {
    fn open() -> std::result::Result<Self, BackendFailure> {
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        };
        use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler};
        use windows::Win32::System::Variant::VARIANT;

        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(map_windows_error)?;
        let com = ComApartment;
        let service: ITaskService = unsafe {
            CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                .map_err(map_windows_error)?
        };
        let empty = VARIANT::default();
        unsafe { service.Connect(&empty, &empty, &empty, &empty) }.map_err(map_windows_error)?;
        Ok(Self { service, _com: com })
    }

    fn root_folder(
        &self,
    ) -> std::result::Result<windows::Win32::System::TaskScheduler::ITaskFolder, BackendFailure>
    {
        unsafe { self.service.GetFolder(&windows::core::BSTR::from("\\")) }
            .map_err(map_windows_error)
    }
}

#[cfg(windows)]
impl SchedulerBackend for WindowsTaskScheduler {
    fn current_user_sid(&mut self) -> std::result::Result<String, BackendFailure> {
        current_user_sid().map_err(|_| BackendFailure::PermissionDenied)
    }

    fn inspect_exact(
        &mut self,
        task_uri: &str,
    ) -> std::result::Result<Option<TaskDefinition>, BackendFailure> {
        use windows::Win32::System::TaskScheduler::IRegisteredTask;

        let folder = self.root_folder()?;
        let name = task_leaf(task_uri).ok_or(BackendFailure::Malformed)?;
        let task: IRegisteredTask =
            match unsafe { folder.GetTask(&windows::core::BSTR::from(name)) } {
                Ok(value) => value,
                Err(error) if is_not_found(&error) => return Ok(None),
                Err(error) => return Err(map_windows_error(error)),
            };
        read_registered_task(task, task_uri)
    }

    fn register_or_update(
        &mut self,
        definition: &TaskDefinition,
    ) -> std::result::Result<(), BackendFailure> {
        use windows::Win32::System::TaskScheduler::{
            TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN,
        };
        use windows::Win32::System::Variant::VARIANT;

        let folder = self.root_folder()?;
        let com_definition = build_com_definition(&self.service, definition)?;
        let empty = VARIANT::default();
        let registered = unsafe {
            folder.RegisterTaskDefinition(
                &windows::core::BSTR::from(
                    task_leaf(&definition.task_uri).ok_or(BackendFailure::Malformed)?,
                ),
                &com_definition,
                TASK_CREATE_OR_UPDATE.0,
                &empty,
                &empty,
                TASK_LOGON_INTERACTIVE_TOKEN,
                &empty,
            )
        }
        .map_err(map_windows_error)?;
        let management_sddl = format!("D:P(A;;FA;;;{})(A;;FA;;;SY)", definition.principal_sid);
        unsafe { registered.SetSecurityDescriptor(&windows::core::BSTR::from(management_sddl), 0) }
            .map_err(map_windows_error)?;
        Ok(())
    }

    fn remove_exact(&mut self, task_uri: &str) -> std::result::Result<(), BackendFailure> {
        let folder = self.root_folder()?;
        let name = task_leaf(task_uri).ok_or(BackendFailure::Malformed)?;
        match unsafe { folder.DeleteTask(&windows::core::BSTR::from(name), 0) } {
            Ok(()) => Ok(()),
            Err(error) if is_not_found(&error) => Ok(()),
            Err(error) => Err(map_windows_error(error)),
        }
    }
}

#[cfg(windows)]
fn build_com_definition(
    service: &windows::Win32::System::TaskScheduler::ITaskService,
    definition: &TaskDefinition,
) -> std::result::Result<windows::Win32::System::TaskScheduler::ITaskDefinition, BackendFailure> {
    use windows::core::Interface;
    use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, TASK_ACTION_EXEC, TASK_COMPATIBILITY_V2,
        TASK_INSTANCES_IGNORE_NEW, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA,
        TASK_TRIGGER_LOGON,
    };

    let task = unsafe { service.NewTask(0) }.map_err(map_windows_error)?;
    unsafe {
        let registration = task.RegistrationInfo().map_err(map_windows_error)?;
        registration
            .SetDescription(&windows::core::BSTR::from(&definition.schema_marker))
            .map_err(map_windows_error)?;
        registration
            .SetAuthor(&windows::core::BSTR::from(
                &definition.registration_owner_sid,
            ))
            .map_err(map_windows_error)?;
        let principal = task.Principal().map_err(map_windows_error)?;
        principal
            .SetUserId(&windows::core::BSTR::from(&definition.principal_sid))
            .map_err(map_windows_error)?;
        principal
            .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
            .map_err(map_windows_error)?;
        principal
            .SetRunLevel(TASK_RUNLEVEL_LUA)
            .map_err(map_windows_error)?;

        let triggers = task.Triggers().map_err(map_windows_error)?;
        let trigger: ILogonTrigger = triggers
            .Create(TASK_TRIGGER_LOGON)
            .and_then(|value| value.cast())
            .map_err(map_windows_error)?;
        trigger
            .SetUserId(&windows::core::BSTR::from(&definition.principal_sid))
            .map_err(map_windows_error)?;

        let actions = task.Actions().map_err(map_windows_error)?;
        let action: IExecAction = actions
            .Create(TASK_ACTION_EXEC)
            .and_then(|value| value.cast())
            .map_err(map_windows_error)?;
        action
            .SetPath(&windows::core::BSTR::from(
                definition.executable.to_string_lossy().as_ref(),
            ))
            .map_err(map_windows_error)?;
        action
            .SetArguments(&windows::core::BSTR::from(INSTALLED_RUNTIME_HOST_FLAG))
            .map_err(map_windows_error)?;
        action
            .SetWorkingDirectory(&windows::core::BSTR::from(
                definition.working_directory.to_string_lossy().as_ref(),
            ))
            .map_err(map_windows_error)?;

        let settings = task.Settings().map_err(map_windows_error)?;
        settings
            .SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)
            .map_err(map_windows_error)?;
        settings
            .SetExecutionTimeLimit(&windows::core::BSTR::from(EXECUTION_TIME_LIMIT))
            .map_err(map_windows_error)?;
        settings.SetRestartCount(3).map_err(map_windows_error)?;
        settings
            .SetRestartInterval(&windows::core::BSTR::from(RESTART_INTERVAL))
            .map_err(map_windows_error)?;
        settings
            .SetAllowDemandStart(VARIANT_TRUE)
            .map_err(map_windows_error)?;
        settings
            .SetRunOnlyIfIdle(VARIANT_FALSE)
            .map_err(map_windows_error)?;
        settings
            .SetRunOnlyIfNetworkAvailable(VARIANT_FALSE)
            .map_err(map_windows_error)?;
        settings
            .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
            .map_err(map_windows_error)?;
        settings
            .SetStopIfGoingOnBatteries(VARIANT_FALSE)
            .map_err(map_windows_error)?;
        settings
            .SetWakeToRun(VARIANT_FALSE)
            .map_err(map_windows_error)?;
        settings
            .SetHidden(VARIANT_FALSE)
            .map_err(map_windows_error)?;
        settings
            .SetEnabled(if definition.enabled {
                VARIANT_TRUE
            } else {
                VARIANT_FALSE
            })
            .map_err(map_windows_error)?;
        settings
            .SetCompatibility(TASK_COMPATIBILITY_V2)
            .map_err(map_windows_error)?;
    }
    Ok(task)
}

#[cfg(windows)]
fn read_registered_task(
    task: windows::Win32::System::TaskScheduler::IRegisteredTask,
    expected_uri: &str,
) -> std::result::Result<Option<TaskDefinition>, BackendFailure> {
    use windows::core::Interface;
    use windows::Win32::Foundation::{VARIANT_BOOL, VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, TASK_ACTION_EXEC, TASK_LOGON_INTERACTIVE_TOKEN,
        TASK_RUNLEVEL_LUA, TASK_TRIGGER_LOGON,
    };

    unsafe {
        let path = bstr_string(task.Path().map_err(map_windows_error)?);
        if path != expected_uri {
            return Err(BackendFailure::Malformed);
        }
        let registered_xml = bstr_string(task.Xml().map_err(map_windows_error)?);
        let definition = task.Definition().map_err(map_windows_error)?;
        let registration = definition.RegistrationInfo().map_err(map_windows_error)?;
        let schema_marker = bstr_from_out(|value| registration.Description(value))?;
        let registration_owner_sid = bstr_from_out(|value| registration.Author(value))?;
        let principal = definition.Principal().map_err(map_windows_error)?;
        let principal_identity = bstr_from_out(|value| principal.UserId(value))?;
        let mut principal_sid = if principal_identity
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("S-1-"))
        {
            principal_identity
        } else {
            account_identity_to_sid(&principal_identity).map_err(|_| BackendFailure::Malformed)?
        };
        // On current Windows 11 Task Scheduler, IPrincipal::UserId may
        // truncate a local-user SID to its authority/domain prefix even while
        // the registered XML retains the exact full SID. Accept only the
        // current process SID when the OS-generated Principal block proves
        // that exact value; never accept a prefix as authority by itself.
        let current_sid = current_user_sid().map_err(|_| BackendFailure::PermissionDenied)?;
        if principal_sid != current_sid
            && registered_xml_principal_is_exact(&registered_xml, &current_sid)
        {
            principal_sid = current_sid;
        }
        let mut logon_type = Default::default();
        principal
            .LogonType(&mut logon_type)
            .map_err(map_windows_error)?;
        let interactive_token = logon_type == TASK_LOGON_INTERACTIVE_TOKEN;
        let mut run_level = Default::default();
        principal
            .RunLevel(&mut run_level)
            .map_err(map_windows_error)?;
        let least_privilege = run_level == TASK_RUNLEVEL_LUA;
        let triggers = definition.Triggers().map_err(map_windows_error)?;
        let mut trigger_count = 0;
        triggers
            .Count(&mut trigger_count)
            .map_err(map_windows_error)?;
        if trigger_count != 1 {
            return Err(BackendFailure::Malformed);
        }
        let trigger = triggers.get_Item(1).map_err(map_windows_error)?;
        let mut trigger_type = Default::default();
        trigger.Type(&mut trigger_type).map_err(map_windows_error)?;
        if trigger_type != TASK_TRIGGER_LOGON {
            return Err(BackendFailure::Malformed);
        }
        let logon: ILogonTrigger = trigger.cast().map_err(map_windows_error)?;
        let trigger_identity = bstr_from_out(|value| logon.UserId(value))?;
        // Task Scheduler normalizes an explicitly registered SID to a
        // SAM-compatible account name on some Windows builds. Resolve that
        // trusted OS readback back to its SID before enforcing exact identity.
        let trigger_sid = if trigger_identity
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("S-1-"))
        {
            trigger_identity
        } else {
            account_identity_to_sid(&trigger_identity).map_err(|_| BackendFailure::Malformed)?
        };
        let actions = definition.Actions().map_err(map_windows_error)?;
        let mut action_count = 0;
        actions
            .Count(&mut action_count)
            .map_err(map_windows_error)?;
        if action_count != 1 {
            return Err(BackendFailure::Malformed);
        }
        let action = actions.get_Item(1).map_err(map_windows_error)?;
        let mut action_type = Default::default();
        action.Type(&mut action_type).map_err(map_windows_error)?;
        if action_type != TASK_ACTION_EXEC {
            return Err(BackendFailure::Malformed);
        }
        let executable: IExecAction = action.cast().map_err(map_windows_error)?;
        let executable_path = PathBuf::from(bstr_from_out(|value| executable.Path(value))?);
        let arguments = bstr_from_out(|value| executable.Arguments(value))?;
        let working_directory =
            PathBuf::from(bstr_from_out(|value| executable.WorkingDirectory(value))?);
        let settings = definition.Settings().map_err(map_windows_error)?;
        let security_descriptor = bstr_string(
            task.GetSecurityDescriptor(0x0000_0007)
                .map_err(map_windows_error)?,
        );

        let read_bool = |read: &dyn Fn(*mut VARIANT_BOOL) -> windows::core::Result<()>| {
            let mut value = VARIANT_FALSE;
            read(&mut value).map_err(map_windows_error)?;
            Ok::<bool, BackendFailure>(value == VARIANT_TRUE)
        };
        let mut multiple_instances = Default::default();
        settings
            .MultipleInstances(&mut multiple_instances)
            .map_err(map_windows_error)?;
        let mut restart_count = 0;
        settings
            .RestartCount(&mut restart_count)
            .map_err(map_windows_error)?;

        Ok(Some(TaskDefinition {
            task_uri: path,
            schema_marker,
            registration_owner_sid,
            principal_sid,
            interactive_token,
            least_privilege,
            logon_trigger_sids: vec![trigger_sid],
            executable: executable_path,
            arguments: if arguments.is_empty() {
                vec![]
            } else {
                vec![arguments]
            },
            working_directory,
            enabled: read_bool(&|value| settings.Enabled(value))?,
            ignore_new_instances: multiple_instances.0 == 2,
            execution_time_limit: bstr_from_out(|value| settings.ExecutionTimeLimit(value))?,
            restart_count: restart_count
                .try_into()
                .map_err(|_| BackendFailure::Malformed)?,
            restart_interval: bstr_from_out(|value| settings.RestartInterval(value))?,
            allow_demand_start: read_bool(&|value| settings.AllowDemandStart(value))?,
            run_only_if_idle: read_bool(&|value| settings.RunOnlyIfIdle(value))?,
            run_only_if_network_available: read_bool(&|value| {
                settings.RunOnlyIfNetworkAvailable(value)
            })?,
            disallow_start_if_on_batteries: read_bool(&|value| {
                settings.DisallowStartIfOnBatteries(value)
            })?,
            stop_if_going_on_batteries: read_bool(&|value| settings.StopIfGoingOnBatteries(value))?,
            wake_to_run: read_bool(&|value| settings.WakeToRun(value))?,
            hidden: read_bool(&|value| settings.Hidden(value))?,
            security_descriptor,
        }))
    }
}

#[cfg(windows)]
fn bstr_from_out(
    read: impl FnOnce(*mut windows::core::BSTR) -> windows::core::Result<()>,
) -> std::result::Result<String, BackendFailure> {
    let mut value = windows::core::BSTR::new();
    read(&mut value).map_err(map_windows_error)?;
    Ok(bstr_string(value))
}

#[cfg(windows)]
fn bstr_string(value: windows::core::BSTR) -> String {
    value.to_string()
}

#[cfg(windows)]
fn task_leaf(task_uri: &str) -> Option<&str> {
    task_uri
        .strip_prefix('\\')
        .filter(|name| !name.is_empty() && !name.contains('\\'))
}

#[cfg(windows)]
fn registered_xml_principal_is_exact(xml: &str, expected_sid: &str) -> bool {
    let Some(principals_start) = xml.find("<Principals>") else {
        return false;
    };
    let Some(principals_end_offset) = xml[principals_start..].find("</Principals>") else {
        return false;
    };
    let principals = &xml[principals_start..principals_start + principals_end_offset];
    let expected = format!("<UserId>{expected_sid}</UserId>");
    principals.matches(&expected).count() == 1
}

#[cfg(windows)]
fn map_windows_error(error: windows::core::Error) -> BackendFailure {
    if is_not_found(&error) {
        BackendFailure::NotFound
    } else if matches!(error.code().0 as u32, 0x8007_0005 | 0x8004_131B) {
        BackendFailure::PermissionDenied
    } else {
        BackendFailure::Unavailable
    }
}

#[cfg(windows)]
fn is_not_found(error: &windows::core::Error) -> bool {
    matches!(error.code().0 as u32, 0x8007_0002 | 0x8004_130F)
}

#[cfg(windows)]
fn current_user_sid() -> Result<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    struct Token(HANDLE);
    impl Drop for Token {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    let mut raw_token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) } == 0 {
        bail!("OpenProcessToken failed: {}", unsafe { GetLastError() });
    }
    let token = Token(raw_token);
    let mut required = 0;
    unsafe {
        let _ = GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required);
    }
    if required < std::mem::size_of::<TOKEN_USER>() as u32 {
        bail!("GetTokenInformation returned an invalid TOKEN_USER length");
    }
    let mut bytes = vec![0u8; required as usize];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            bytes.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        bail!("GetTokenInformation failed: {}", unsafe { GetLastError() });
    }
    let sid = unsafe { (*bytes.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut sid_string = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_string) } == 0 || sid_string.is_null() {
        bail!("ConvertSidToStringSidW failed: {}", unsafe {
            GetLastError()
        });
    }
    let mut length = 0usize;
    while unsafe { *sid_string.add(length) } != 0 {
        length += 1;
    }
    let result = OsString::from_wide(unsafe { std::slice::from_raw_parts(sid_string, length) })
        .to_string_lossy()
        .into_owned();
    unsafe {
        let _ = LocalFree(sid_string.cast());
    }
    Ok(result)
}

#[cfg(windows)]
fn account_identity_to_sid(account_identity: &str) -> Result<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::ptr;
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{LookupAccountNameW, SID_NAME_USE};

    let account: Vec<u16> = std::ffi::OsStr::new(account_identity)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut sid_length = 0u32;
    let mut domain_length = 0u32;
    let mut sid_use: SID_NAME_USE = 0;
    unsafe {
        let _ = LookupAccountNameW(
            ptr::null(),
            account.as_ptr(),
            ptr::null_mut(),
            &mut sid_length,
            ptr::null_mut(),
            &mut domain_length,
            &mut sid_use,
        );
    }
    if sid_length == 0 {
        bail!("LookupAccountNameW returned no SID size");
    }
    let mut sid = vec![0u8; sid_length as usize];
    let mut domain = vec![0u16; domain_length.max(1) as usize];
    if unsafe {
        LookupAccountNameW(
            ptr::null(),
            account.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_length,
            domain.as_mut_ptr(),
            &mut domain_length,
            &mut sid_use,
        )
    } == 0
    {
        bail!("LookupAccountNameW failed: {}", unsafe { GetLastError() });
    }
    let mut sid_string = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid.as_mut_ptr().cast(), &mut sid_string) } == 0
        || sid_string.is_null()
    {
        bail!("ConvertSidToStringSidW failed: {}", unsafe {
            GetLastError()
        });
    }
    let mut length = 0usize;
    while unsafe { *sid_string.add(length) } != 0 {
        length += 1;
    }
    let result = OsString::from_wide(unsafe { std::slice::from_raw_parts(sid_string, length) })
        .to_string_lossy()
        .into_owned();
    unsafe {
        let _ = LocalFree(sid_string.cast());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeScheduler {
        sid: String,
        task: Option<TaskDefinition>,
        reads: VecDeque<std::result::Result<Option<TaskDefinition>, BackendFailure>>,
        register_calls: usize,
        remove_calls: usize,
    }

    impl FakeScheduler {
        fn with_task(task: TaskDefinition) -> Self {
            Self {
                sid: task.principal_sid.clone(),
                task: Some(task),
                ..Self::default()
            }
        }
    }

    impl SchedulerBackend for FakeScheduler {
        fn current_user_sid(&mut self) -> std::result::Result<String, BackendFailure> {
            Ok(self.sid.clone())
        }

        fn inspect_exact(
            &mut self,
            _: &str,
        ) -> std::result::Result<Option<TaskDefinition>, BackendFailure> {
            self.reads
                .pop_front()
                .unwrap_or_else(|| Ok(self.task.clone()))
        }

        fn register_or_update(
            &mut self,
            definition: &TaskDefinition,
        ) -> std::result::Result<(), BackendFailure> {
            self.register_calls += 1;
            let mut readback = definition.clone();
            readback.security_descriptor =
                format!("D:P(A;;FA;;;{})(A;;FA;;;SY)", definition.principal_sid);
            self.task = Some(readback);
            Ok(())
        }

        fn remove_exact(&mut self, _: &str) -> std::result::Result<(), BackendFailure> {
            self.remove_calls += 1;
            self.task = None;
            Ok(())
        }
    }

    fn gateway_fixture() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let gateway = temp.path().join("carsinos-gateway.exe");
        std::fs::write(gateway, b"test gateway").unwrap();
        temp
    }

    fn request(
        temp: &tempfile::TempDir,
        desired_mode: DesiredMode,
        start_at_login: bool,
    ) -> ReconcileRequest {
        ReconcileRequest {
            desired_mode,
            start_at_login,
            installed_gateway_executable: temp.path().join("carsinos-gateway.exe"),
        }
    }

    fn desired_for_test(temp: &tempfile::TempDir, enabled: bool) -> TaskDefinition {
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            ..Default::default()
        };
        desired_definition(
            &mut scheduler,
            &request(temp, DesiredMode::Background, enabled),
        )
        .unwrap()
    }

    #[test]
    fn contract_is_fixed_non_secret_and_has_no_wrapper_arguments() {
        assert_eq!(TASK_URI, r"\CarsinOS ExecAss Runtime");
        assert_eq!(
            INSTALLED_RUNTIME_HOST_FLAG,
            "--mission-control-runtime-host"
        );
        assert!(!INSTALLED_RUNTIME_HOST_FLAG.contains('='));
        assert!(!INSTALLED_RUNTIME_HOST_FLAG.contains("token"));
        assert!(!INSTALLED_RUNTIME_HOST_FLAG.contains("secret"));
        assert!(!INSTALLED_RUNTIME_HOST_FLAG.contains("state"));
        assert!(current_installed_gateway_executable().is_none());
    }

    #[test]
    fn background_login_true_creates_exact_enabled_definition() {
        let temp = gateway_fixture();
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            ..Default::default()
        };
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&temp, DesiredMode::Background, true),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::Created);
        assert_eq!(receipt.enabled, Some(true));
        assert_eq!(scheduler.register_calls, 1);
        let task = scheduler.task.unwrap();
        assert_eq!(task.arguments, [INSTALLED_RUNTIME_HOST_FLAG]);
        assert!(task.interactive_token && task.least_privilege && task.ignore_new_instances);
        assert_eq!(task.execution_time_limit, "PT0S");
        assert_eq!(task.restart_count, 3);
        assert!(!task.run_only_if_idle && !task.run_only_if_network_available);
        assert!(!task.disallow_start_if_on_batteries && !task.stop_if_going_on_batteries);
    }

    #[test]
    fn app_bound_login_true_is_rejected_before_scheduler_mutation() {
        let temp = gateway_fixture();
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            ..Default::default()
        };
        let receipt =
            reconcile_with_backend(&mut scheduler, request(&temp, DesiredMode::AppBound, true));
        assert_eq!(receipt.outcome, SchedulerOutcome::InvalidRequest);
        assert_eq!(
            receipt.failure_category,
            Some("invalid_mode_login_combination")
        );
        assert_eq!(scheduler.register_calls, 0);
    }

    #[test]
    fn background_login_false_keeps_identity_but_disables_task() {
        let temp = gateway_fixture();
        let mut previous = desired_for_test(&temp, true);
        previous.security_descriptor = "D:P(A;;FA;;;S-1-5-21-100)(A;;FA;;;SY)".to_owned();
        let mut scheduler = FakeScheduler::with_task(previous);
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&temp, DesiredMode::Background, false),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::Disabled);
        assert_eq!(receipt.enabled, Some(false));
        assert_eq!(scheduler.register_calls, 1);
    }

    #[test]
    fn same_owner_install_path_change_is_repaired() {
        let old_temp = gateway_fixture();
        let new_temp = gateway_fixture();
        let mut previous = desired_for_test(&old_temp, true);
        previous.security_descriptor = "D:P(A;;FA;;;S-1-5-21-100)(A;;FA;;;SY)".to_owned();
        let mut scheduler = FakeScheduler::with_task(previous);
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&new_temp, DesiredMode::Background, true),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::Repaired);
        assert_eq!(scheduler.register_calls, 1);
    }

    #[test]
    fn foreign_owner_or_action_is_never_repaired_or_removed() {
        let temp = gateway_fixture();
        let mut foreign = desired_for_test(&temp, true);
        foreign.security_descriptor = "D:P(A;;FA;;;S-1-5-21-999)(A;;FA;;;SY)".to_owned();
        foreign.principal_sid = "S-1-5-21-999".to_owned();
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            task: Some(foreign),
            ..Default::default()
        };
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&temp, DesiredMode::Background, true),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::IdentityConflict);
        assert_eq!(scheduler.register_calls, 0);
        let removed = remove_with_backend(&mut scheduler);
        assert_eq!(removed.outcome, SchedulerOutcome::IdentityConflict);
        assert_eq!(scheduler.remove_calls, 0);
    }

    #[test]
    fn weak_or_unreadable_management_acl_is_a_conflict_not_a_repair_target() {
        let temp = gateway_fixture();
        let mut task = desired_for_test(&temp, true);
        task.security_descriptor = "D:P(A;;FR;;;S-1-5-21-100)(A;;FA;;;SY)".to_owned();
        let mut scheduler = FakeScheduler::with_task(task);
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&temp, DesiredMode::Background, true),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::IdentityConflict);
        assert_eq!(scheduler.register_calls, 0);
        assert_eq!(
            remove_with_backend(&mut scheduler).outcome,
            SchedulerOutcome::IdentityConflict
        );
        assert_eq!(scheduler.remove_calls, 0);
    }

    #[test]
    fn scheduler_permission_failure_is_returned_as_a_safe_typed_receipt() {
        let temp = gateway_fixture();
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            ..Default::default()
        };
        scheduler
            .reads
            .push_back(Err(BackendFailure::PermissionDenied));
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&temp, DesiredMode::Background, true),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::PermissionDenied);
        assert_eq!(receipt.failure_category, Some("permission_denied"));
        assert!(!format!("{receipt:?}").contains("S-1-5-21-100"));
    }

    #[test]
    fn malformed_readback_after_register_fails_closed() {
        let temp = gateway_fixture();
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            ..Default::default()
        };
        scheduler.reads.push_back(Ok(None));
        scheduler.reads.push_back(Ok(Some(TaskDefinition {
            task_uri: TASK_URI.to_owned(),
            schema_marker: TASK_SCHEMA_MARKER.to_owned(),
            registration_owner_sid: "S-1-5-21-100".to_owned(),
            principal_sid: "S-1-5-21-100".to_owned(),
            interactive_token: true,
            least_privilege: true,
            logon_trigger_sids: vec!["S-1-5-21-100".to_owned()],
            executable: temp.path().join("carsinos-gateway.exe"),
            arguments: vec!["--unsafe-extra".to_owned()],
            working_directory: temp.path().to_owned(),
            enabled: true,
            ignore_new_instances: true,
            execution_time_limit: "PT0S".to_owned(),
            restart_count: 3,
            restart_interval: "PT1M".to_owned(),
            allow_demand_start: true,
            run_only_if_idle: false,
            run_only_if_network_available: false,
            disallow_start_if_on_batteries: false,
            stop_if_going_on_batteries: false,
            wake_to_run: false,
            hidden: false,
            security_descriptor: "D:P(A;;FA;;;S-1-5-21-100)(A;;FA;;;SY)".to_owned(),
        })));
        let receipt = reconcile_with_backend(
            &mut scheduler,
            request(&temp, DesiredMode::Background, true),
        );
        assert_eq!(receipt.outcome, SchedulerOutcome::IdentityConflict);
        assert_eq!(
            receipt.failure_category,
            Some("post_registration_readback_mismatch")
        );
    }

    #[test]
    fn exact_remove_is_idempotent_but_refuses_bad_acl() {
        let temp = gateway_fixture();
        let mut scheduler = FakeScheduler {
            sid: "S-1-5-21-100".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            remove_with_backend(&mut scheduler).outcome,
            SchedulerOutcome::Removed
        );
        let mut managed = desired_for_test(&temp, true);
        managed.security_descriptor = "D:P(A;;FA;;;S-1-5-21-100)(A;;FA;;;SY)".to_owned();
        scheduler.task = Some(managed);
        assert_eq!(
            remove_with_backend(&mut scheduler).outcome,
            SchedulerOutcome::Removed
        );
        assert_eq!(scheduler.remove_calls, 1);
    }

    #[test]
    fn receipts_redact_identity_and_do_not_include_command_line_or_sddl() {
        let receipt = receipt_from_failure(
            SchedulerOperation::Reconcile,
            BackendFailure::PermissionDenied,
            Some(sid_digest("S-1-5-21-100")),
        );
        let debug = format!("{receipt:?}");
        assert!(!debug.contains("S-1-5-21-100"));
        assert!(!debug.contains(INSTALLED_RUNTIME_HOST_FLAG));
        assert!(!debug.contains("D:("));
    }
}
