use keyring::{Entry, Error as KeyringError};
use serde::Serialize;
use std::sync::Mutex;
#[cfg(not(debug_assertions))]
use tauri::Emitter;
use tauri::{Manager, RunEvent, State};
use tauri_plugin_shell::process::CommandChild;
#[cfg(not(debug_assertions))]
use tauri_plugin_shell::{process::CommandEvent, ShellExt};

const KEYRING_SERVICE: &str = "carsinos.mission-control";
const KEYRING_USERNAME: &str = "gateway-token";
const DESKTOP_GATEWAY_URL: &str = "http://127.0.0.1:18789/";

#[derive(Default)]
struct GatewaySidecarState {
    child: Option<CommandChild>,
    stopping: bool,
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

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USERNAME).map_err(|error| error.to_string())
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
    let state_dir = app.path().app_local_data_dir()?.join("state");
    std::fs::create_dir_all(&state_dir)?;

    let command = app
        .shell()
        .sidecar("carsinos-gateway")?
        .env("CARSINOS_GATEWAY_BIND", "127.0.0.1:18789")
        .env("CARSINOS_GATEWAY_TOKEN", token)
        .env("CARSINOS_STATE_DIR", state_dir)
        .env("CARSINOS_LOG_STDOUT", "false")
        .env("CARSINOS_LOG_FILE", "true");
    let (mut events, child) = command.spawn()?;
    {
        let sidecar = app.state::<GatewaySidecar>();
        let mut state = sidecar.0.lock().map_err(|_| "sidecar lock poisoned")?;
        state.child = Some(child);
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

fn stop_gateway_sidecar(handle: &tauri::AppHandle) {
    let child = {
        let state = handle.state::<GatewaySidecar>();
        let mut guard = match state.0.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        guard.stopping = true;
        guard.child.take()
    };
    if let Some(child) = child {
        let _ = child.kill();
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
fn get_desktop_bootstrap(state: State<'_, GatewaySidecar>) -> DesktopBootstrap {
    let (managed_gateway, startup_error) = state
        .0
        .lock()
        .map(|state| (state.child.is_some(), state.last_error.clone()))
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
            get_desktop_bootstrap,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. }) {
            stop_gateway_sidecar(handle);
        }
    });
}
