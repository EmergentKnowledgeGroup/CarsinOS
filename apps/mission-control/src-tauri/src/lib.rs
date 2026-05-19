use keyring::{Entry, Error as KeyringError};

const KEYRING_SERVICE: &str = "carsinos.mission-control";
const KEYRING_USERNAME: &str = "gateway-token";

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USERNAME).map_err(|error| error.to_string())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
