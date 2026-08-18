//! Narrow offline verifier used by the state activation tool.

use anyhow::{bail, Context, Result};
use carsinos_storage::execass::{ExecAssStore, IntegrityStatus, ReceiptIntegrityStore};
use carsinos_storage::AppPaths;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn failure_context() -> (String, Option<String>) {
    let values = std::env::args().skip(1).collect::<Vec<_>>();
    let action = values
        .first()
        .filter(|value| matches!(value.as_str(), "inspect-db" | "verify-active"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_owned());
    let root_locator = (values.len() == 3
        && values.get(1).is_some_and(|value| value == "--state-root"))
    .then(|| {
        let supplied = PathBuf::from(&values[2]);
        let resolved = std::fs::canonicalize(&supplied).unwrap_or_else(|_| {
            if supplied.is_absolute() {
                supplied
            } else {
                std::env::current_dir().unwrap_or_default().join(supplied)
            }
        });
        let normalized = resolved.to_string_lossy().to_lowercase();
        let mut digest = Sha256::new();
        digest.update(b"carsinos.receipt-verifier.path.v1\0");
        digest.update(normalized.as_bytes());
        format!("sha256:{:x}", digest.finalize())
    });
    (action, root_locator)
}

fn arguments() -> Result<(String, PathBuf)> {
    let mut values = std::env::args().skip(1);
    let action = values.next().context("missing verifier action")?;
    if values.next().as_deref() != Some("--state-root") {
        bail!("expected --state-root");
    }
    let state_root = values.next().context("missing state-root path")?;
    if values.next().is_some() {
        bail!("unexpected verifier arguments");
    }
    if !matches!(action.as_str(), "inspect-db" | "verify-active") {
        bail!("unsupported verifier action");
    }
    Ok((action, PathBuf::from(state_root)))
}

fn run() -> Result<serde_json::Value> {
    let (action, state_root) = arguments()?;
    let state_root =
        std::fs::canonicalize(&state_root).context("configured state root is unavailable")?;
    let paths = AppPaths::from_root(&state_root);
    ExecAssStore::open(&paths).context("state root is not the exact ExecAss schema")?;
    if action == "inspect-db" {
        return Ok(json!({
            "ok": true,
            "action": action,
            "schema": "execass.v1",
        }));
    }

    let store = ReceiptIntegrityStore::open(&paths)?;
    let initial = store.status()?;
    let recovery = matches!(initial, IntegrityStatus::Prepared { .. });
    store.recover_integrity()?;
    let final_status = store.status()?;
    let IntegrityStatus::Trusted {
        anchor_generation, ..
    } = final_status
    else {
        bail!("active receipt integrity did not recover to a trusted finalized anchor");
    };
    Ok(json!({
        "ok": true,
        "action": action,
        "status": "trusted",
        "root_identity": store.root_identity(),
        "anchor_generation": anchor_generation,
        "recovered": recovery,
    }))
}

fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(_error) => {
            let (action, root_locator) = failure_context();
            eprintln!("CarsinOS receipt-integrity verification failed");
            println!(
                "{}",
                json!({
                    "ok": false,
                    "action": action,
                    "reason": "receipt_integrity_rejected",
                    "root_locator": root_locator,
                })
            );
            std::process::exit(2);
        }
    }
}
