//! Installed Mission Control runtime-host bootstrap.
//!
//! Windows Task Scheduler launches the one gateway runtime host directly with
//! a fixed, non-secret mode flag. The gateway then loads the existing
//! current-user credentials and canonical Mission Control state root itself;
//! no token, owner secret, or state path is placed in the scheduled action.

use anyhow::{bail, Context, Result};
use carsinos_core::{GatewayConfig, TokenSource};
use std::ffi::{OsStr, OsString};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

pub(crate) const INSTALLED_RUNTIME_HOST_FLAG: &str = "--mission-control-runtime-host";
const KEYRING_SERVICE: &str = "carsinos.mission-control";
const GATEWAY_TOKEN_USERNAME: &str = "gateway-token";
const OWNER_SECRET_USERNAME: &str = "execass-local-owner-secret";
const PRODUCT_STATE_RELATIVE_PATH: &str = "io.carsinos.missioncontrol\\state";

pub(crate) struct InstalledRuntimeBootstrap {
    pub(crate) config: GatewayConfig,
    pub(crate) owner_secret: Vec<u8>,
}

pub(crate) fn load_from_process() -> Result<Option<InstalledRuntimeBootstrap>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if !installed_mode_requested(&args)? {
        return Ok(None);
    }
    #[cfg(not(windows))]
    bail!("the installed Mission Control runtime-host mode is Windows-only");

    #[cfg(windows)]
    {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .context("installed runtime host cannot resolve absolute current-user LOCALAPPDATA")?;
        load_from_keyring_and_local_app_data(&local_app_data).map(Some)
    }
}

fn installed_mode_requested(args: &[OsString]) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }
    if args[0] != OsStr::new(INSTALLED_RUNTIME_HOST_FLAG) {
        return Ok(false);
    }
    if args.len() != 1 {
        bail!("installed runtime-host mode accepts no additional arguments");
    }
    Ok(true)
}

#[cfg(windows)]
fn load_from_keyring_and_local_app_data(
    local_app_data: &Path,
) -> Result<InstalledRuntimeBootstrap> {
    let token = required_keyring_secret(GATEWAY_TOKEN_USERNAME, "gateway token")?;
    let owner_secret = required_keyring_secret(OWNER_SECRET_USERNAME, "ExecAss owner secret")?;
    if owner_secret.len() < 32 {
        bail!("installed runtime-host ExecAss owner secret is invalid");
    }
    let state_dir = local_app_data.join(PRODUCT_STATE_RELATIVE_PATH);
    Ok(InstalledRuntimeBootstrap {
        config: GatewayConfig {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 18789),
            token,
            token_source: TokenSource::Env,
            state_dir,
        },
        owner_secret: owner_secret.into_bytes(),
    })
}

#[cfg(windows)]
fn required_keyring_secret(username: &str, description: &str) -> Result<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, username)
        .with_context(|| format!("failed opening installed runtime-host {description}"))?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(keyring::Error::NoEntry) => {
            bail!("installed runtime-host {description} is not provisioned")
        }
        Err(error) => Err(error)
            .with_context(|| format!("failed loading installed runtime-host {description}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_mode_is_closed_and_contains_no_configuration_arguments() {
        assert!(!installed_mode_requested(&[]).unwrap());
        assert!(!installed_mode_requested(&[OsString::from("--other")]).unwrap());
        assert!(installed_mode_requested(&[OsString::from(INSTALLED_RUNTIME_HOST_FLAG)]).unwrap());
        assert!(installed_mode_requested(&[
            OsString::from(INSTALLED_RUNTIME_HOST_FLAG),
            OsString::from("state-or-secret-must-not-appear-here"),
        ])
        .is_err());
        assert!(!INSTALLED_RUNTIME_HOST_FLAG.contains('='));
    }

    #[test]
    fn canonical_product_state_path_is_fixed_and_relative() {
        let relative = Path::new(PRODUCT_STATE_RELATIVE_PATH);
        assert!(!relative.is_absolute());
        assert_eq!(relative.components().count(), 2);
        assert_eq!(
            Path::new(r"Z:\profile\AppData\Local").join(relative),
            PathBuf::from(r"Z:\profile\AppData\Local\io.carsinos.missioncontrol\state")
        );
    }
}
