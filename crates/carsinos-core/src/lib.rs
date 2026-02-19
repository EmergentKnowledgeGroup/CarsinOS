use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Env,
    Generated,
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub bind: SocketAddr,
    pub token: String,
    pub token_source: TokenSource,
    pub state_dir: PathBuf,
}

impl GatewayConfig {
    pub fn load_from_env() -> Result<Self> {
        let bind = std::env::var("CARSINOS_GATEWAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:18789".to_string());
        let bind = SocketAddr::from_str(&bind)
            .with_context(|| format!("invalid CARSINOS_GATEWAY_BIND: {bind}"))?;

        let (token, token_source) = match std::env::var("CARSINOS_GATEWAY_TOKEN") {
            Ok(token) if !token.trim().is_empty() => (token, TokenSource::Env),
            _ => (uuid::Uuid::new_v4().to_string(), TokenSource::Generated),
        };

        let state_dir = match std::env::var("CARSINOS_STATE_DIR") {
            Ok(path) if !path.trim().is_empty() => PathBuf::from(path),
            _ => default_state_dir()?,
        };

        Ok(Self {
            bind,
            token,
            token_source,
            state_dir,
        })
    }
}

fn default_state_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "carsinos", "carsinos")
        .context("failed to resolve app data directory")?;
    Ok(dirs.data_local_dir().to_path_buf())
}
