use carsinos_protocol::execass::{
    local_decision_proof_bytes, local_owner_intake_proof_bytes, local_owner_mutation_proof_bytes,
    local_run_control_request_proof_bytes, normalized_owner_intent_digest,
    owner_instruction_digest, redact_execass_builtin_secret_patterns, IntakeRequest,
    LocalDecisionProof, LocalDecisionProofBinding, LocalOwnerIntakeProof,
    LocalOwnerMutationBinding, LocalOwnerMutationProof, LocalRunControlProof,
    RunControlRequestBinding,
};
use hmac::{Hmac, Mac};
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tauri::{Manager, RunEvent, State, WindowEvent};
use tauri_plugin_shell::process::CommandChild;
#[cfg(not(debug_assertions))]
use tauri_plugin_shell::{process::CommandEvent, ShellExt};
use zeroize::Zeroizing;

const KEYRING_SERVICE: &str = "carsinos.mission-control";
const KEYRING_USERNAME: &str = "gateway-token";
const EXECASS_OWNER_KEYRING_USERNAME: &str = "execass-local-owner-secret";
const EXECASS_DESKTOP_CLIENT_ID: &str = "carsinos-mission-control-desktop-v1";
const RUN_CONTROL_EVIDENCE_MAX_AGE_MS: i64 = 60_000;
const RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS: i64 = 5_000;
const SAFE_TEXT_MAX_BYTES: usize = 64 * 1024;
const DESKTOP_GATEWAY_URL: &str = "http://127.0.0.1:18789/";
const GRACEFUL_RUNTIME_STOP_TIMEOUT: Duration = Duration::from_secs(15);
const GRACEFUL_RUNTIME_STOP_POLL_INTERVAL: Duration = Duration::from_millis(150);

type HmacSha256 = Hmac<Sha256>;

/// Intentionally has no `Debug`, `Serialize`, or public accessor. The only
/// permitted outputs are a sidecar environment value and an HMAC operation.
struct ExecAssOwnerSecret(Zeroizing<String>);

impl ExecAssOwnerSecret {
    fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[cfg(not(debug_assertions))]
    fn into_sidecar_environment(mut self) -> String {
        std::mem::take(&mut *self.0)
    }
}

#[derive(Default)]
struct GatewaySidecarState {
    child: Option<CommandChild>,
    attached_host: bool,
    stopping: bool,
    close_confirmation_pending: bool,
    exit_authorized: bool,
    last_error: Option<String>,
}

struct GatewaySidecar(Mutex<GatewaySidecarState>);

#[derive(Serialize)]
struct DesktopBootstrap {
    gateway_url: &'static str,
    managed_gateway: bool,
    startup_error: Option<String>,
}

#[cfg(not(debug_assertions))]
#[derive(Clone, Serialize)]
struct GatewayTerminated {
    message: String,
}

/// The one concrete consequence returned by authenticated native control.
/// The opaque binding is echoed only once by a confirmation command and is
/// never synthesized by the webview.
#[derive(Clone, Serialize)]
struct RuntimeCloseConfirmation {
    consequence: String,
    binding: carsinos_runtime_control::CloseConfirmationBindingV1,
}

#[derive(Clone, Serialize)]
struct RuntimeCloseRecoveryAttention {
    message: String,
}

#[derive(Deserialize)]
struct RuntimeCloseConfirmationInput {
    binding: carsinos_runtime_control::CloseConfirmationBindingV1,
}

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USERNAME).map_err(|error| error.to_string())
}

fn execass_owner_keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, EXECASS_OWNER_KEYRING_USERNAME).map_err(|error| error.to_string())
}

fn load_or_create_execass_owner_secret() -> Result<ExecAssOwnerSecret, String> {
    let entry = execass_owner_keyring_entry()?;
    match entry.get_password() {
        Ok(value) if value.len() >= 32 => Ok(ExecAssOwnerSecret::new(value)),
        Ok(_) | Err(KeyringError::NoEntry) => {
            let value = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
            entry
                .set_password(&value)
                .map_err(|error| format!("failed to store ExecAss owner secret: {error}"))?;
            Ok(ExecAssOwnerSecret::new(value))
        }
        Err(error) => Err(format!("failed to read ExecAss owner secret: {error}")),
    }
}

#[cfg(not(debug_assertions))]
fn load_or_create_gateway_token() -> Result<String, String> {
    let entry = keyring_entry()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Ok(_) | Err(KeyringError::NoEntry) => {
            let value = uuid::Uuid::new_v4().to_string();
            entry
                .set_password(&value)
                .map_err(|error| format!("failed to store generated gateway token: {error}"))?;
            Ok(value)
        }
        Err(error) => Err(format!("failed to read gateway token: {error}")),
    }
}

#[cfg(not(debug_assertions))]
fn start_gateway_sidecar(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let token = load_or_create_gateway_token()?;
    let execass_owner_secret = load_or_create_execass_owner_secret()?;
    let state_dir = app.path().app_local_data_dir()?.join("state");
    std::fs::create_dir_all(&state_dir)?;

    if attach_existing_runtime_host(&state_dir, &execass_owner_secret)? {
        let sidecar = app.state::<GatewaySidecar>();
        let mut state = sidecar.0.lock().map_err(|_| "sidecar lock poisoned")?;
        state.child = None;
        state.attached_host = true;
        state.stopping = false;
        state.last_error = None;
        return Ok(());
    }

    let command = app
        .shell()
        .sidecar("carsinos-gateway")?
        .env("CARSINOS_GATEWAY_BIND", "127.0.0.1:18789")
        .env("CARSINOS_GATEWAY_TOKEN", token)
        .env(
            "CARSINOS_EXECASS_LOCAL_OWNER_SECRET",
            execass_owner_secret.into_sidecar_environment(),
        )
        .env("CARSINOS_STATE_DIR", state_dir)
        .env("CARSINOS_LOG_STDOUT", "false")
        .env("CARSINOS_LOG_FILE", "true");
    let (mut events, child) = command.spawn()?;
    {
        let sidecar = app.state::<GatewaySidecar>();
        let mut state = sidecar.0.lock().map_err(|_| "sidecar lock poisoned")?;
        state.child = Some(child);
        state.attached_host = false;
        state.stopping = false;
        state.last_error = None;
    }

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let CommandEvent::Terminated(termination) = event {
                let message = format!("Managed gateway terminated: {termination:?}");
                let should_report = {
                    let state = handle.state::<GatewaySidecar>();
                    let result = match state.0.lock() {
                        Ok(mut state) => {
                            state.child = None;
                            if state.stopping {
                                false
                            } else {
                                state.last_error = Some(message.clone());
                                true
                            }
                        }
                        Err(_) => true,
                    };
                    result
                };
                if should_report {
                    log::error!("{message}");
                    let _ = handle.emit(
                        "gateway-terminated",
                        GatewayTerminated {
                            message: message.clone(),
                        },
                    );
                }
                break;
            }
        }
    });
    Ok(())
}

#[cfg(not(debug_assertions))]
fn attach_existing_runtime_host(
    state_dir: &std::path::Path,
    owner_secret: &ExecAssOwnerSecret,
) -> Result<bool, Box<dyn std::error::Error>> {
    let canonical_root = state_dir.canonicalize()?;
    let scope = carsinos_runtime_control::RuntimeControlScopeV1 {
        canonical_root_identity:
            carsinos_protocol::execass_recorder::canonical_root_identity_from_canonical_path(
                &canonical_root.to_string_lossy(),
            ),
        profile_identity: carsinos_runtime_control::DEFAULT_PROFILE_IDENTITY.to_string(),
        os_user_identity_digest: carsinos_runtime_control::current_os_user_identity_digest()?,
    };
    let key = carsinos_runtime_control::derive_owner_control_key(owner_secret.as_bytes())?;
    let client = carsinos_runtime_control::RuntimeControlClient::new(state_dir, scope, key)?;
    let attached = tauri::async_runtime::block_on(async {
        let status = client.status().await?;
        if status.runtime_host_generation <= 0 || status.runtime_host_instance_id.is_empty() {
            return Err(carsinos_runtime_control::RuntimeControlError::InvalidRequest);
        }
        client
            .attach(carsinos_runtime_control::AttachRequestV1 {
                client_instance_id: format!(
                    "{}-{}",
                    EXECASS_DESKTOP_CLIENT_ID,
                    uuid::Uuid::new_v4()
                ),
            })
            .await
    });
    match attached {
        Ok(reply) => Ok(reply.runtime_host_generation > 0),
        Err(carsinos_runtime_control::RuntimeControlError::Transport) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Forced termination is deliberately unavailable to normal exit handling.
/// It is a last resort after an authenticated app-bound shutdown authorization
/// has had the full bounded drain interval to disconnect the native host.
fn force_stop_owned_sidecar_after_timeout(handle: &tauri::AppHandle) -> bool {
    let child = {
        let state = handle.state::<GatewaySidecar>();
        let mut guard = match state.0.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        if guard.attached_host {
            return false;
        }
        guard.stopping = true;
        guard.child.take()
    };
    if let Some(child) = child {
        let _ = child.kill();
        true
    } else {
        false
    }
}

fn runtime_control_client(
    handle: &tauri::AppHandle,
) -> Result<carsinos_runtime_control::RuntimeControlClient, String> {
    let owner_secret = load_or_create_execass_owner_secret()?;
    let state_dir = handle
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("failed resolving desktop state directory: {error}"))?
        .join("state");
    std::fs::create_dir_all(&state_dir)
        .map_err(|error| format!("failed preparing native runtime state directory: {error}"))?;
    let canonical_root = state_dir
        .canonicalize()
        .map_err(|error| format!("failed resolving native runtime state directory: {error}"))?;
    let scope = carsinos_runtime_control::RuntimeControlScopeV1 {
        canonical_root_identity:
            carsinos_protocol::execass_recorder::canonical_root_identity_from_canonical_path(
                &canonical_root.to_string_lossy(),
            ),
        profile_identity: carsinos_runtime_control::DEFAULT_PROFILE_IDENTITY.to_string(),
        os_user_identity_digest: carsinos_runtime_control::current_os_user_identity_digest()
            .map_err(|error| format!("failed resolving desktop OS identity: {error}"))?,
    };
    let key = carsinos_runtime_control::derive_owner_control_key(owner_secret.as_bytes())
        .map_err(|error| format!("failed deriving native runtime-control key: {error}"))?;
    carsinos_runtime_control::RuntimeControlClient::new(&state_dir, scope, key)
        .map_err(|error| format!("failed opening authenticated native runtime control: {error}"))
}

fn authorize_desktop_exit(handle: &tauri::AppHandle) {
    if let Ok(mut state) = handle.state::<GatewaySidecar>().0.lock() {
        state.exit_authorized = true;
        state.close_confirmation_pending = false;
    }
    handle.exit(0);
}

fn report_runtime_close_error(handle: &tauri::AppHandle, message: impl Into<String>) {
    let message = message.into();
    log::error!("{message}");
    let _ = handle.emit(
        "runtime-close-error",
        RuntimeCloseRecoveryAttention { message },
    );
}

async fn wait_for_native_runtime_stop(
    client: &carsinos_runtime_control::RuntimeControlClient,
) -> bool {
    let deadline = Instant::now() + GRACEFUL_RUNTIME_STOP_TIMEOUT;
    loop {
        let stopped = match client.status().await {
            Err(carsinos_runtime_control::RuntimeControlError::Transport) => true,
            Ok(status) => {
                status.actual_state == carsinos_runtime_control::RuntimeActualStateV1::Stopped
            }
            Err(_) => false,
        };
        if stopped {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        // Yield the async runtime while the bounded poll interval elapses;
        // blocking a Tokio worker here could delay the gateway disconnect we
        // are waiting to observe.
        let _ = tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(GRACEFUL_RUNTIME_STOP_POLL_INTERVAL);
        })
        .await;
    }
}

async fn request_desktop_runtime_close(
    handle: tauri::AppHandle,
    confirmation: Option<carsinos_runtime_control::CloseConfirmationBindingV1>,
) {
    let client = match runtime_control_client(&handle) {
        Ok(client) => client,
        Err(error) => {
            report_runtime_close_error(&handle, error);
            return;
        }
    };
    let status = match client.status().await {
        Ok(status) => status,
        Err(carsinos_runtime_control::RuntimeControlError::Transport) => {
            // The UI always attempts authenticated status first. With no live
            // native host there is nothing it can accidentally terminate.
            authorize_desktop_exit(&handle);
            return;
        }
        Err(error) => {
            report_runtime_close_error(
                &handle,
                format!("CarsinOS could not verify native runtime status before closing: {error}"),
            );
            return;
        }
    };

    if status.desired_mode == carsinos_runtime_control::RuntimeDesiredModeV1::Background {
        // Background is a host ownership promise: exiting the UI never sends
        // shutdown and never touches an attached or background child.
        authorize_desktop_exit(&handle);
        return;
    }

    let request = carsinos_runtime_control::GracefulShutdownRequestV1 {
        client_instance_id: EXECASS_DESKTOP_CLIENT_ID.to_string(),
        runtime_host_generation: status.runtime_host_generation,
        runtime_host_instance_id: status.runtime_host_instance_id,
        confirmation,
    };
    let reply = match client.graceful_shutdown(request).await {
        Ok(reply) => reply,
        Err(error) => {
            report_runtime_close_error(
                &handle,
                format!(
                    "CarsinOS could not request a graceful app-bound runtime shutdown: {error}"
                ),
            );
            return;
        }
    };

    match reply.disposition {
        carsinos_runtime_control::GracefulShutdownDispositionV1::ConfirmationRequired {
            consequence,
            binding,
            ..
        } => {
            let should_emit = handle
                .state::<GatewaySidecar>()
                .0
                .lock()
                .map(|mut state| {
                    if state.close_confirmation_pending {
                        false
                    } else {
                        state.close_confirmation_pending = true;
                        true
                    }
                })
                .unwrap_or(false);
            if should_emit {
                let _ = handle.emit(
                    "runtime-close-confirmation-required",
                    RuntimeCloseConfirmation {
                        consequence,
                        binding,
                    },
                );
            }
        }
        carsinos_runtime_control::GracefulShutdownDispositionV1::Authorized { .. } => {
            if let Ok(mut state) = handle.state::<GatewaySidecar>().0.lock() {
                state.close_confirmation_pending = false;
                state.stopping = true;
            }
            if wait_for_native_runtime_stop(&client).await {
                authorize_desktop_exit(&handle);
                return;
            }
            let forced = force_stop_owned_sidecar_after_timeout(&handle);
            let message = if forced {
                "The app-bound runtime did not disconnect after its graceful shutdown timeout. CarsinOS forced only its owned child to stop; review recovery attention before closing the app again."
            } else {
                "The app-bound runtime did not disconnect after its graceful shutdown timeout. CarsinOS left the attached runtime untouched; review recovery attention before closing the app again."
            };
            let _ = handle.emit(
                "runtime-close-recovery-required",
                RuntimeCloseRecoveryAttention {
                    message: message.to_string(),
                },
            );
        }
        carsinos_runtime_control::GracefulShutdownDispositionV1::Rejected { reason } => {
            report_runtime_close_error(
                &handle,
                format!("CarsinOS kept the UI open because native runtime close was rejected: {reason:?}"),
            );
        }
    }
}

#[tauri::command]
async fn confirm_runtime_close(
    app: tauri::AppHandle,
    confirmation: RuntimeCloseConfirmationInput,
) -> Result<(), String> {
    request_desktop_runtime_close(app, Some(confirmation.binding)).await;
    Ok(())
}

#[tauri::command]
fn cancel_runtime_close_confirmation(app: tauri::AppHandle) {
    if let Ok(mut state) = app.state::<GatewaySidecar>().0.lock() {
        // Cancellation changes only the desktop presentation. The native
        // handler retains its exact binding so a later explicit close can
        // never receive a materially different implicit confirmation.
        state.close_confirmation_pending = false;
    }
}

#[tauri::command]
async fn set_gateway_token(token: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let value = token.trim().to_string();
        if value.is_empty() {
            return Err("token cannot be empty".to_string());
        }
        let entry = keyring_entry()?;
        entry
            .set_password(&value)
            .map_err(|error| format!("failed to store token in keychain: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn clear_gateway_token() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = keyring_entry()?;
        match entry.delete_password() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(format!("failed to clear token from keychain: {error}")),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn get_gateway_token() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = keyring_entry()?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(format!("failed to read token from keychain: {error}")),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn gateway_token_present() -> Result<bool, String> {
    get_gateway_token().await.map(|value| value.is_some())
}

#[tauri::command]
async fn sign_execass_local_run_control(
    binding: RunControlRequestBinding,
) -> Result<LocalRunControlProof, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let now_ms = current_unix_time_ms()?;
        let secret = load_or_create_execass_owner_secret()?;
        sign_execass_local_run_control_with_secret(&binding, &secret, now_ms)
    })
    .await
    .map_err(|_| "native run-control signer unavailable".to_string())?
}

/// Sign exactly one local owner-intake request. The raw request text is kept
/// only for this blocking operation, is never logged or persisted here, and
/// is reduced to both an exact-byte instruction digest and the same trimmed,
/// redacted intent digest that the gateway binds before durable intake.
#[tauri::command]
async fn sign_execass_local_owner_intake(
    request: IntakeRequest,
) -> Result<LocalOwnerIntakeProof, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let secret = load_or_create_execass_owner_secret()?;
        sign_execass_local_owner_intake_with_secret(request, &secret)
    })
    .await
    .map_err(|_| "native owner intake signer unavailable".to_string())?
}

#[tauri::command]
async fn sign_execass_local_owner_mutation(
    binding: LocalOwnerMutationBinding,
) -> Result<LocalOwnerMutationProof, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let secret = load_or_create_execass_owner_secret()?;
        sign_execass_local_owner_mutation_with_secret(&binding, &secret)
    })
    .await
    .map_err(|_| "native owner mutation signer unavailable".to_string())?
}

#[tauri::command]
async fn sign_execass_local_decision(
    binding: LocalDecisionProofBinding,
    request_correlation_id: String,
) -> Result<LocalDecisionProof, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let secret = load_or_create_execass_owner_secret()?;
        sign_execass_local_decision_with_secret(binding, request_correlation_id, &secret)
    })
    .await
    .map_err(|_| "native decision signer unavailable".to_string())?
}

fn current_unix_time_ms() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_string())?
        .as_millis();
    i64::try_from(millis).map_err(|_| "system clock is outside the supported range".to_string())
}

fn sign_execass_local_run_control_with_secret(
    binding: &RunControlRequestBinding,
    secret: &ExecAssOwnerSecret,
    server_now_ms: i64,
) -> Result<LocalRunControlProof, String> {
    binding
        .validate()
        .map_err(|_| "invalid run-control request binding".to_string())?;
    let age_ms = server_now_ms.saturating_sub(binding.observed_at_ms());
    if binding.observed_at_ms()
        > server_now_ms.saturating_add(RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS)
        || age_ms > RUN_CONTROL_EVIDENCE_MAX_AGE_MS
    {
        return Err("run-control request timestamp is stale or in the future".to_string());
    }
    if secret.as_bytes().len() < 32 {
        return Err("native owner secret is unavailable".to_string());
    }
    let bytes = local_run_control_request_proof_bytes(EXECASS_DESKTOP_CLIENT_ID, binding)
        .map_err(|_| "invalid run-control request binding".to_string())?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| "native owner signer unavailable".to_string())?;
    mac.update(&bytes);
    let proof_hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    LocalRunControlProof::from_authenticated_native_request(
        EXECASS_DESKTOP_CLIENT_ID.to_string(),
        binding.request_correlation_id().to_string(),
        proof_hex,
    )
    .map_err(|_| "native owner signer produced invalid proof".to_string())
}

fn sign_execass_local_owner_intake_with_secret(
    request: IntakeRequest,
    secret: &ExecAssOwnerSecret,
) -> Result<LocalOwnerIntakeProof, String> {
    let IntakeRequest {
        request_id,
        idempotency_key,
        text,
        source_correlation_id,
        attach_to_delegation_id,
    } = request;
    let text = Zeroizing::new(text);
    if secret.as_bytes().len() < 32 {
        return Err("native owner secret is unavailable".to_string());
    }
    let instruction_digest = owner_instruction_digest(text.as_bytes())
        .ok_or_else(|| "local owner intake text is invalid".to_string())?;
    let normalized_intent_digest = safe_normalized_intent_digest(text.as_str())?;
    let unsigned = LocalOwnerIntakeProof {
        authenticated_client_id: EXECASS_DESKTOP_CLIENT_ID.to_string(),
        request_correlation_id: source_correlation_id,
        request_id,
        idempotency_key,
        attach_to_delegation_id,
        normalized_intent_digest,
        instruction_digest,
        // The canonical byte encoder intentionally excludes the MAC itself.
        proof_hex: "00".repeat(32),
    };
    let bytes = local_owner_intake_proof_bytes(&unsigned)
        .map_err(|_| "invalid local owner intake binding".to_string())?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| "native owner intake signer unavailable".to_string())?;
    mac.update(&bytes);
    LocalOwnerIntakeProof::from_authenticated_native_request(
        unsigned.authenticated_client_id,
        unsigned.request_correlation_id,
        unsigned.request_id,
        unsigned.idempotency_key,
        unsigned.attach_to_delegation_id,
        unsigned.normalized_intent_digest,
        unsigned.instruction_digest,
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
    .map_err(|_| "native owner signer produced invalid proof".to_string())
}

fn sign_execass_local_owner_mutation_with_secret(
    binding: &LocalOwnerMutationBinding,
    secret: &ExecAssOwnerSecret,
) -> Result<LocalOwnerMutationProof, String> {
    binding
        .validate()
        .map_err(|_| "invalid owner mutation binding".to_string())?;
    if secret.as_bytes().len() < 32 {
        return Err("native owner secret is unavailable".to_string());
    }
    let mut proof = LocalOwnerMutationProof {
        authenticated_client_id: EXECASS_DESKTOP_CLIENT_ID.to_string(),
        request_correlation_id: binding.request_correlation_id.clone(),
        proof_hex: "00".repeat(32),
    };
    let bytes = local_owner_mutation_proof_bytes(&proof, binding)
        .map_err(|_| "invalid owner mutation binding".to_string())?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| "native owner mutation signer unavailable".to_string())?;
    mac.update(&bytes);
    proof.proof_hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(proof)
}

fn sign_execass_local_decision_with_secret(
    binding: LocalDecisionProofBinding,
    request_correlation_id: String,
    secret: &ExecAssOwnerSecret,
) -> Result<LocalDecisionProof, String> {
    let now_ms = current_unix_time_ms()?;
    if secret.as_bytes().len() < 32
        || request_correlation_id.trim().is_empty()
        || binding.idempotency_key.trim().is_empty()
        || binding.observed_at_ms <= 0
        || binding.observed_at_ms > now_ms.saturating_add(RUN_CONTROL_EVIDENCE_MAX_FUTURE_SKEW_MS)
        || now_ms.saturating_sub(binding.observed_at_ms) > RUN_CONTROL_EVIDENCE_MAX_AGE_MS
        || binding.observed_at_ms >= binding.expires_at_ms
    {
        return Err("invalid local decision binding".to_string());
    }
    let mut proof = LocalDecisionProof {
        authenticated_client_id: EXECASS_DESKTOP_CLIENT_ID.to_string(),
        request_correlation_id,
        proof_hex: "00".repeat(32),
    };
    let bytes = local_decision_proof_bytes(&proof, &binding);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| "native decision signer unavailable".to_string())?;
    mac.update(&bytes);
    proof.proof_hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(proof)
}

fn safe_normalized_intent_digest(raw_text: &str) -> Result<String, String> {
    let normalized = raw_text.trim();
    if normalized.is_empty() || normalized.len() > SAFE_TEXT_MAX_BYTES {
        return Err("local owner intake text is invalid".to_string());
    }
    let normalized = redact_execass_builtin_secret_patterns(normalized);
    normalized_owner_intent_digest(&normalized)
        .ok_or_else(|| "local owner intake text is invalid".to_string())
}

#[tauri::command]
fn get_desktop_bootstrap(state: State<'_, GatewaySidecar>) -> DesktopBootstrap {
    let (managed_gateway, startup_error) = state
        .0
        .lock()
        .map(|state| {
            (
                state.child.is_some() || state.attached_host,
                state.last_error.clone(),
            )
        })
        .unwrap_or_else(|_| {
            (
                false,
                Some("Managed gateway state is unavailable.".to_string()),
            )
        });
    DesktopBootstrap {
        gateway_url: DESKTOP_GATEWAY_URL,
        managed_gateway,
        startup_error,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(GatewaySidecar(Mutex::new(GatewaySidecarState::default())))
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|_app| {
            #[cfg(not(debug_assertions))]
            if let Err(error) = start_gateway_sidecar(_app) {
                let message = format!("Managed gateway failed to start: {error}");
                log::error!("{message}");
                if let Ok(mut state) = _app.state::<GatewaySidecar>().0.lock() {
                    state.last_error = Some(message);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_gateway_token,
            clear_gateway_token,
            get_gateway_token,
            gateway_token_present,
            sign_execass_local_run_control,
            sign_execass_local_owner_intake,
            sign_execass_local_owner_mutation,
            sign_execass_local_decision,
            get_desktop_bootstrap,
            confirm_runtime_close,
            cancel_runtime_close_confirmation,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| match event {
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            api.prevent_close();
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                request_desktop_runtime_close(handle, None).await;
            });
        }
        RunEvent::ExitRequested { api, .. } => {
            let exit_authorized = handle
                .state::<GatewaySidecar>()
                .0
                .lock()
                .map(|state| state.exit_authorized)
                .unwrap_or(false);
            if !exit_authorized {
                api.prevent_exit();
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    request_desktop_runtime_close(handle, None).await;
                });
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use carsinos_protocol::execass::{RunControlResumeSnapshot, RunControlTarget};

    const NOW_MS: i64 = 1_800_000_000_000;
    const TEST_SECRET: &str = "test-native-owner-secret-with-32-bytes";

    fn binding() -> RunControlRequestBinding {
        RunControlRequestBinding::delegation_resume(
            "delegation-1".to_string(),
            "resume-idempotency-1".to_string(),
            "resume-correlation-1".to_string(),
            NOW_MS,
            RunControlResumeSnapshot::new(
                9,
                17,
                format!("sha256:{}", "a".repeat(64)),
                Some(31),
                Some(42),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn intake_request() -> IntakeRequest {
        IntakeRequest {
            request_id: "intake-request-1".into(),
            idempotency_key: "intake-idempotency-1".into(),
            text: "prepare the exact owner request".into(),
            source_correlation_id: "intake-correlation-1".into(),
            attach_to_delegation_id: None,
        }
    }

    fn decision_binding(observed_at_ms: i64) -> LocalDecisionProofBinding {
        LocalDecisionProofBinding {
            decision_id: "decision-1".into(),
            decision_revision: 3,
            normalized_intent_digest: "a".repeat(64),
            policy_revision: 2,
            canonical_manifest_digest: "b".repeat(64),
            selected_logical_action_id: "action-1".into(),
            presented_action_digest: "c".repeat(64),
            declared_consequence_digest: "d".repeat(64),
            challenge_digest: "e".repeat(64),
            expires_at_ms: observed_at_ms + 60_000,
            response_selected_logical_action_id: "action-1".into(),
            decision_result: carsinos_protocol::execass::DecisionResult::ConfirmAndContinue,
            idempotency_key: "decision-idempotency-1".into(),
            revision_text_digest: None,
            challenge_response_digest: None,
            observed_at_ms,
        }
    }

    #[test]
    fn native_decision_signer_binds_exact_resolution_without_exposing_secret() {
        let observed_at_ms = current_unix_time_ms().unwrap();
        let binding = decision_binding(observed_at_ms);
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let proof = sign_execass_local_decision_with_secret(
            binding.clone(),
            "decision-correlation-1".into(),
            &secret,
        )
        .unwrap();
        let proof_bytes = (0..proof.proof_hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&proof.proof_hex[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let mut verifier = HmacSha256::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
        verifier.update(&local_decision_proof_bytes(&proof, &binding));
        assert!(verifier.verify_slice(&proof_bytes).is_ok());
        assert!(!serde_json::to_string(&proof).unwrap().contains(TEST_SECRET));

        let mut changed = binding;
        changed.idempotency_key = "other-idempotency".into();
        let mut changed_verifier = HmacSha256::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
        changed_verifier.update(&local_decision_proof_bytes(&proof, &changed));
        assert!(changed_verifier.verify_slice(&proof_bytes).is_err());
    }

    #[test]
    fn native_signer_uses_fixed_identity_and_exposes_no_secret() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let proof =
            sign_execass_local_run_control_with_secret(&binding(), &secret, NOW_MS).unwrap();
        assert_eq!(proof.authenticated_client_id, EXECASS_DESKTOP_CLIENT_ID);
        assert_eq!(proof.request_correlation_id, "resume-correlation-1");
        let json = serde_json::to_string(&proof).unwrap();
        let debug = format!("{proof:?}");
        assert!(!json.contains(TEST_SECRET));
        assert!(!debug.contains(TEST_SECRET));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn native_signer_proof_rejects_bad_secret_and_every_bound_field_mutation() {
        let original = binding();
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let proof = sign_execass_local_run_control_with_secret(&original, &secret, NOW_MS).unwrap();
        let expected =
            local_run_control_request_proof_bytes(EXECASS_DESKTOP_CLIENT_ID, &original).unwrap();
        let proof_bytes = (0..proof.proof_hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&proof.proof_hex[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let mut correct = HmacSha256::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
        correct.update(&expected);
        assert!(correct.verify_slice(&proof_bytes).is_ok());
        let mut wrong =
            HmacSha256::new_from_slice(b"different-native-owner-secret-with-32").unwrap();
        wrong.update(&expected);
        assert!(wrong.verify_slice(&proof_bytes).is_err());

        for mutation in 0..9 {
            let mut changed = original.clone();
            match mutation {
                0 => changed.idempotency_key = "different-idempotency".to_string(),
                1 => changed.request_correlation_id = "different-correlation".to_string(),
                2 => changed.observed_at_ms += 1,
                3 => {
                    changed.target = RunControlTarget::Delegation {
                        delegation_id: "delegation-2".to_string(),
                    }
                }
                4 => changed.resume.as_mut().unwrap().stopped_epoch += 1,
                5 => changed.resume.as_mut().unwrap().current_policy_revision += 1,
                6 => changed.resume.as_mut().unwrap().delegation_state_revision = Some(32),
                7 => changed.resume.as_mut().unwrap().current_plan_revision = Some(43),
                _ => {
                    changed
                        .resume
                        .as_mut()
                        .unwrap()
                        .unresolved_effect_disclosure_digest = format!("sha256:{}", "b".repeat(64))
                }
            }
            let changed_bytes =
                local_run_control_request_proof_bytes(EXECASS_DESKTOP_CLIENT_ID, &changed).unwrap();
            let mut verifier = HmacSha256::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
            verifier.update(&changed_bytes);
            assert!(
                verifier.verify_slice(&proof_bytes).is_err(),
                "bound mutation {mutation} accepted"
            );
        }
    }

    #[test]
    fn native_signer_rejects_operation_target_mismatch_stale_time_and_short_secret() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let mut mismatch_json = serde_json::to_value(binding()).unwrap();
        mismatch_json["operation"] = serde_json::json!("global_resume");
        let mismatch: RunControlRequestBinding = serde_json::from_value(mismatch_json).unwrap();
        assert!(sign_execass_local_run_control_with_secret(&mismatch, &secret, NOW_MS).is_err());

        let mut stale_json = serde_json::to_value(binding()).unwrap();
        stale_json["observed_at_ms"] =
            serde_json::json!(NOW_MS - RUN_CONTROL_EVIDENCE_MAX_AGE_MS - 1);
        let stale: RunControlRequestBinding = serde_json::from_value(stale_json).unwrap();
        assert!(sign_execass_local_run_control_with_secret(&stale, &secret, NOW_MS).is_err());
        let short = ExecAssOwnerSecret::new("too-short".to_string());
        assert!(sign_execass_local_run_control_with_secret(&binding(), &short, NOW_MS).is_err());
    }

    #[test]
    fn native_intake_signer_uses_fixed_identity_and_never_returns_raw_text_or_secret() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let proof = sign_execass_local_owner_intake_with_secret(intake_request(), &secret).unwrap();
        assert_eq!(proof.authenticated_client_id, EXECASS_DESKTOP_CLIENT_ID);
        assert_eq!(proof.request_correlation_id, "intake-correlation-1");
        assert_eq!(proof.request_id, "intake-request-1");
        let json = serde_json::to_string(&proof).unwrap();
        assert!(!json.contains("prepare the exact owner request"));
        assert!(!json.contains(TEST_SECRET));
    }

    #[test]
    fn native_intake_signer_rejects_wrong_secret_and_every_bound_request_mutation() {
        let original = intake_request();
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let proof = sign_execass_local_owner_intake_with_secret(original.clone(), &secret).unwrap();
        let expected = local_owner_intake_proof_bytes(&proof).unwrap();
        let proof_bytes = (0..proof.proof_hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&proof.proof_hex[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let mut correct = HmacSha256::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
        correct.update(&expected);
        assert!(correct.verify_slice(&proof_bytes).is_ok());
        let mut wrong =
            HmacSha256::new_from_slice(b"different-native-owner-secret-with-32").unwrap();
        wrong.update(&expected);
        assert!(wrong.verify_slice(&proof_bytes).is_err());

        for mutation in 0..5 {
            let mut changed = original.clone();
            match mutation {
                0 => changed.request_id = "another-request".into(),
                1 => changed.idempotency_key = "another-idempotency".into(),
                2 => changed.source_correlation_id = "another-correlation".into(),
                3 => changed.attach_to_delegation_id = Some("delegation-2".into()),
                _ => changed.text = "a different owner request".into(),
            }
            let changed_proof =
                sign_execass_local_owner_intake_with_secret(changed, &secret).unwrap();
            let changed_bytes = local_owner_intake_proof_bytes(&changed_proof).unwrap();
            let mut verifier = HmacSha256::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
            verifier.update(&changed_bytes);
            assert!(
                verifier.verify_slice(&proof_bytes).is_err(),
                "bound mutation {mutation} accepted"
            );
        }
    }

    #[test]
    fn native_intake_signer_distinguishes_raw_secrets_with_the_same_redacted_shape() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let mut first_request = intake_request();
        first_request.text = format!("deliver token sk-proj-{}", "a".repeat(24));
        let mut second_request = first_request.clone();
        second_request.text = format!("deliver token sk-proj-{}", "b".repeat(24));

        let first =
            sign_execass_local_owner_intake_with_secret(first_request.clone(), &secret).unwrap();
        let exact_retry =
            sign_execass_local_owner_intake_with_secret(first_request, &secret).unwrap();
        let second = sign_execass_local_owner_intake_with_secret(second_request, &secret).unwrap();

        assert_eq!(first, exact_retry, "an exact raw retry must be stable");
        assert_eq!(
            first.normalized_intent_digest,
            second.normalized_intent_digest
        );
        assert_ne!(first.instruction_digest, second.instruction_digest);
        assert_ne!(
            local_owner_intake_proof_bytes(&first).unwrap(),
            local_owner_intake_proof_bytes(&second).unwrap()
        );
    }

    #[test]
    fn native_intake_signer_rejects_empty_oversized_and_short_secret_input() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let mut empty = intake_request();
        empty.text = "   ".into();
        assert!(sign_execass_local_owner_intake_with_secret(empty, &secret).is_err());
        let mut oversized = intake_request();
        oversized.text = "a".repeat(SAFE_TEXT_MAX_BYTES + 1);
        assert!(sign_execass_local_owner_intake_with_secret(oversized, &secret).is_err());
        let short = ExecAssOwnerSecret::new("too-short".to_string());
        assert!(sign_execass_local_owner_intake_with_secret(intake_request(), &short).is_err());
    }

    #[test]
    fn native_intake_signer_uses_protocol_redaction_for_secret_pattern_parity() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        for raw in [
            "Bearer abcdefghijkl",
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==",
            "JWT eyJabc.def.ghi",
            "token sk-proj-abcdefghijklmnopqrstuvwxyz123456",
            "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----",
        ] {
            let mut request = intake_request();
            request.text = raw.into();
            let proof = sign_execass_local_owner_intake_with_secret(request, &secret).unwrap();
            assert_eq!(
                proof.normalized_intent_digest,
                safe_normalized_intent_digest(redact_execass_builtin_secret_patterns(raw).as_str())
                    .unwrap(),
                "protocol-redacted intent digest mismatch for {raw}"
            );
        }
    }

    #[test]
    fn native_owner_mutation_signer_binds_full_created_request_and_rejects_open_routes() {
        let secret = ExecAssOwnerSecret::new(TEST_SECRET.to_string());
        let binding = LocalOwnerMutationBinding {
            operation: carsinos_protocol::execass::OwnerMutationOperation::PolicyUpdate,
            method: "PUT".into(),
            path: "/api/v1/execass/policy".into(),
            request_correlation_id: "native-mutation-correlation".into(),
            idempotency_key: "native-mutation-idempotency".into(),
            expected_revision: 1,
            canonical_body_digest: "11".repeat(32),
            safe_snapshot_digest: "22".repeat(32),
            created_at_ms: 1_700_000_000_000,
        };
        let proof = sign_execass_local_owner_mutation_with_secret(&binding, &secret).unwrap();
        assert_eq!(proof.authenticated_client_id, EXECASS_DESKTOP_CLIENT_ID);
        assert!(local_owner_mutation_proof_bytes(&proof, &binding).is_ok());

        let mut changed = binding.clone();
        changed.created_at_ms += 1;
        assert_ne!(
            local_owner_mutation_proof_bytes(&proof, &binding).unwrap(),
            local_owner_mutation_proof_bytes(&proof, &changed).unwrap()
        );
        let mut open_route = binding;
        open_route.path = "/api/v1/config/runtime".into();
        assert!(sign_execass_local_owner_mutation_with_secret(&open_route, &secret).is_err());
    }
}
