use anyhow::Context;
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "ea213-fake-provider")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Invoke {
        #[arg(long)]
        fixture_root: PathBuf,
        #[arg(long)]
        attempt_id: String,
        #[arg(long)]
        idempotency_key: String,
        #[arg(long)]
        reconciliation_key: String,
        #[arg(long)]
        pause_after_ledger_fsync_root: Option<PathBuf>,
    },
    Query {
        #[arg(long)]
        fixture_root: PathBuf,
        #[arg(long)]
        reconciliation_key: String,
    },
}

fn main() -> anyhow::Result<()> {
    match Arguments::parse().command {
        Command::Invoke {
            fixture_root,
            attempt_id,
            idempotency_key,
            reconciliation_key,
            pause_after_ledger_fsync_root,
        } => invoke(
            fixture_root,
            attempt_id,
            idempotency_key,
            reconciliation_key,
            pause_after_ledger_fsync_root,
        ),
        Command::Query {
            fixture_root,
            reconciliation_key,
        } => query(fixture_root, reconciliation_key),
    }
}

fn invoke(
    fixture_root: PathBuf,
    attempt_id: String,
    idempotency_key: String,
    reconciliation_key: String,
    pause_after_ledger_fsync_root: Option<PathBuf>,
) -> anyhow::Result<()> {
    fs::create_dir_all(&fixture_root).context("creating fake-provider fixture root")?;
    let ledger_path = fixture_root.join("invocations.jsonl");
    let mut ledger = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger_path)
        .context("opening fake-provider invocation ledger")?;
    let remote_effect_id = format!(
        "fake-remote-{}",
        carsinos_effect_recorder::hex_for_binary(&Sha256::digest(
            format!("{attempt_id}\0{idempotency_key}").as_bytes()
        ))
    );
    let row = serde_json::to_vec(&serde_json::json!({
        "attempt_id": attempt_id,
        "idempotency_key": idempotency_key,
        "reconciliation_key_digest": stable_key_digest(&reconciliation_key),
        "remote_effect_id": remote_effect_id,
    }))?;
    ledger.write_all(&row)?;
    ledger.write_all(b"\n")?;
    ledger.sync_data()?;
    if let Some(root) = pause_after_ledger_fsync_root {
        fs::create_dir_all(&root)?;
        fs::write(
            root.join("afterproviderledgerfsync.reached"),
            std::process::id().to_string(),
        )?;
        while !root.join("afterproviderledgerfsync.continue").exists() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        fs::write(root.join("afterproviderledgerfsync.exited"), b"done")?;
    }
    println!(
        "{}",
        serde_json::json!({"remote_effect_id": remote_effect_id})
    );
    Ok(())
}

fn query(fixture_root: PathBuf, reconciliation_key: String) -> anyhow::Result<()> {
    let ledger_path = fixture_root.join("invocations.jsonl");
    let expected_digest = stable_key_digest(&reconciliation_key);
    let mut matches = Vec::new();
    match fs::read_to_string(&ledger_path) {
        Ok(ledger) => {
            for line in ledger.lines() {
                let row: serde_json::Value =
                    serde_json::from_str(line).context("decoding fake-provider ledger row")?;
                if row
                    .get("reconciliation_key_digest")
                    .and_then(serde_json::Value::as_str)
                    == Some(expected_digest.as_str())
                {
                    matches.push(
                        row.get("remote_effect_id")
                            .and_then(serde_json::Value::as_str)
                            .context("ledger row omitted remote effect ID")?
                            .to_owned(),
                    );
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("reading fake-provider ledger"),
    }
    if matches.len() > 1 {
        anyhow::bail!("fake-provider ledger contains duplicate reconciliation identity");
    }
    println!(
        "{}",
        serde_json::json!({
            "found": !matches.is_empty(),
            "remote_effect_id": matches.into_iter().next(),
        })
    );
    Ok(())
}

fn stable_key_digest(value: &str) -> String {
    format!(
        "sha256:{}",
        carsinos_effect_recorder::hex_for_binary(&Sha256::digest(value.as_bytes()))
    )
}
