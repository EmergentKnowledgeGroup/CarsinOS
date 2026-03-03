use keyring::{Entry, Error as KeyringError};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const KEYRING_SERVICE: &str = "carsinos.mission-control";
const KEYRING_USERNAME: &str = "gateway-token";

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USERNAME).map_err(|error| error.to_string())
}

fn write_setup_token_script(path: &Path) -> Result<(), String> {
    let script = r#"#!/bin/bash
set +e
clear
echo "carsinOS: Anthropic setup-token helper"
echo ""
echo "1) Sign in if prompted"
echo "2) Copy the generated token"
echo "3) Paste it into Mission Control Step 5"
echo ""
claude setup-token
echo ""
echo "When done, return to Mission Control and paste the token."
echo "Press Enter to close this window."
read -r _
"#;
    fs::write(path, script).map_err(|error| format!("failed writing setup script: {error}"))?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("failed reading setup script metadata: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed setting setup script permissions: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn launch_anthropic_setup_token() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("failed reading system time: {error}"))?
            .as_nanos();
        let temp_dir = std::env::temp_dir().join("carsinos-mission-control");
        fs::create_dir_all(&temp_dir)
            .map_err(|error| format!("failed creating temp script dir: {error}"))?;
        let script_path = temp_dir.join(format!("anthropic_setup_token_{ts}.command"));
        write_setup_token_script(script_path.as_path())?;

        #[cfg(target_os = "macos")]
        {
            let status = Command::new("open")
                .arg("-a")
                .arg("Terminal")
                .arg(script_path.as_path())
                .status()
                .map_err(|error| format!("failed opening Terminal: {error}"))?;
            if !status.success() {
                return Err("Terminal launch command failed".to_string());
            }
            Ok("Opened Terminal and started `claude setup-token`.".to_string())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err("Automatic terminal launch is currently supported only on macOS desktop builds. Run `claude setup-token` manually.".to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())?
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_gateway_token,
            clear_gateway_token,
            get_gateway_token,
            gateway_token_present,
            launch_anthropic_setup_token,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
