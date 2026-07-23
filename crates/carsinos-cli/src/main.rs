use anyhow::Result;
use carsinos_core::{GatewayConfig, TokenSource};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

#[derive(Debug, Parser)]
#[command(name = "carsinos")]
#[command(about = "carsinOS CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn config_summary(config: &GatewayConfig) -> String {
    let token_source = match config.token_source {
        TokenSource::Env => "environment",
        TokenSource::Generated => "generated",
    };
    format!(
        "bind={}\nstate_dir_present={}\ntoken_present={}\ntoken_source={}\n",
        config.bind,
        !config.state_dir.as_os_str().is_empty(),
        !config.token.trim().is_empty(),
        token_source,
    )
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print resolved gateway config from current environment.
    PrintConfig,
    /// Build and bundle a macOS .app package.
    PackageMacos {
        /// Output directory for carsinOS.app (default: target/dist)
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Build release binaries.
        #[arg(long, default_value_t = false, conflicts_with = "debug")]
        release: bool,
        /// Build debug binaries.
        #[arg(long, default_value_t = false, conflicts_with = "release")]
        debug: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::PrintConfig => {
            let config = GatewayConfig::load_from_env()?;
            print!("{}", config_summary(&config));
        }
        Command::PackageMacos {
            output_dir,
            release,
            debug,
        } => {
            let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()?;
            let script = workspace_root.join("scripts/package_macos_app.sh");
            if !script.exists() {
                anyhow::bail!("the configured packaging script was not found");
            }
            let mut cmd = ProcessCommand::new("bash");
            cmd.arg(&script);
            if let Some(output_dir) = output_dir {
                cmd.arg(output_dir);
            }
            if debug {
                cmd.arg("--debug");
            } else if release {
                cmd.arg("--release");
            } else {
                // Default packaging mode is release.
                cmd.arg("--release");
            }
            let status = cmd
                .current_dir(&workspace_root)
                .status()
                .map_err(|err| anyhow::anyhow!("failed to launch packaging script: {err}"))?;
            if !status.success() {
                anyhow::bail!("packaging script failed with status: {}", status);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn config_summary_never_includes_the_configured_gateway_token() {
        let configured_token = format!("test-only-gateway-token-{}", std::process::id());
        let config = GatewayConfig {
            bind: "127.0.0.1:18789".parse::<SocketAddr>().expect("parse bind"),
            token: configured_token.clone(),
            token_source: TokenSource::Env,
            state_dir: PathBuf::from("state-path-secret"),
        };
        let output = config_summary(&config);
        assert!(output.contains("token_present=true"));
        assert!(output.contains("token_source=environment"));
        assert!(output.contains("state_dir_present=true"));
        assert!(!output.contains(&configured_token));
        assert!(!output.contains("state-path-secret"));

        let debug_output = format!("{config:?}");
        assert!(debug_output.contains("token_present: true"));
        assert!(debug_output.contains("token_source: Env"));
        assert!(debug_output.contains("state_dir_present: true"));
        assert!(!debug_output.contains(&configured_token));
        assert!(!debug_output.contains("state-path-secret"));
    }
}
