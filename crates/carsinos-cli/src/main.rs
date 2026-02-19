use anyhow::Result;
use carsinos_core::GatewayConfig;
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
            println!("bind={}", config.bind);
            println!("state_dir={}", config.state_dir.display());
            println!("token={}", config.token);
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
                anyhow::bail!("packaging script not found: {}", script.display());
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
