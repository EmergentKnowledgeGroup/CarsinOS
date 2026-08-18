//! Owner-configured runtime-host settings over the existing single host engine.

use super::foundation::authority_record_from_manifest;
use super::receipt::{
    receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{get_outbox, insert_authority, insert_outbox};
use super::store::ExecAssStore;
use super::types::*;
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::VerifiedOwnerAuthority;
use carsinos_core::execass_manifest::canonicalize_owner_authority;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};

const RUNTIME_AGGREGATE_ID: &str = "execass-runtime-host";
const GLOBAL_RECEIPT_CARRIER_ID: &str = "execass-global-control-carrier";

enum SettingsMutation {
    Updated(ExecAssRuntimeHostStatus, OutboxEventRecord),
    Replayed(ExecAssRuntimeHostStatus, OutboxEventRecord, ReceiptRecord),
    Stale(i64),
    Conflict,
}

impl ExecAssStore {
    pub fn execass_runtime_host_status(
        &self,
        trusted_now: i64,
    ) -> Result<ExecAssRuntimeHostStatus> {
        if trusted_now <= 0 {
            bail!("runtime-host status requires a positive trusted clock");
        }
        let conn = self.connection()?;
        load_status(&conn, trusted_now)
    }

    pub fn update_execass_runtime_settings_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &UpdateExecAssRuntimeSettingsCommand,
        owner_authority: &VerifiedOwnerAuthority,
    ) -> Result<ExecAssRuntimeSettingsUpdateOutcome> {
        let settings_json = canonical_object_json(&command.safe_settings)?;
        validate_combination(command)?;
        let settings_digest = digest(
            json!({
                "desired_mode": command.desired_mode.as_str(),
                "start_at_login": command.start_at_login,
                "settings": serde_json::from_str::<serde_json::Value>(&settings_json)?,
            })
            .to_string()
            .as_bytes(),
        );
        let next_revision = command
            .expected_settings_revision
            .checked_add(1)
            .context("runtime settings revision overflow")?;
        let canonical = canonicalize_owner_authority(owner_authority).map_err(|detail| {
            anyhow::anyhow!("invalid runtime-settings owner authority: {detail}")
        })?;
        let authority = authority_record_from_manifest(&canonical)?;
        validate_command(command, &authority, next_revision, &settings_digest)?;
        let request_digest = mutation_digest(
            command,
            &authority.authority_provenance_id,
            &settings_digest,
        );

        let atomic =
            self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
                if let Some(existing) = load_by_idempotency(tx, &command.idempotency_key)? {
                    if existing.request_digest != request_digest
                        || existing.actor_type != authority.actor_type
                        || existing.credential_identity != authority.credential_identity
                    {
                        return Ok(AtomicReceiptMutation::NoAppend(SettingsMutation::Conflict));
                    }
                    let receipt = receipt_by_causation_event(tx, &existing.outbox_event_id)?
                        .context("runtime-settings replay receipt is missing")?;
                    let outbox = get_outbox(tx, &existing.outbox_event_id)?
                        .context("runtime-settings replay outbox event is missing")?;
                    return Ok(AtomicReceiptMutation::NoAppend(SettingsMutation::Replayed(
                        load_status(tx, command.created_at)?,
                        outbox,
                        receipt,
                    )));
                }
                let current = current_settings_revision(tx)?;
                if current != command.expected_settings_revision {
                    return Ok(AtomicReceiptMutation::NoAppend(SettingsMutation::Stale(
                        current,
                    )));
                }
                if authority_exists(tx, &authority.authority_provenance_id)? {
                    return Ok(AtomicReceiptMutation::NoAppend(SettingsMutation::Conflict));
                }
                let policy_revision = current_policy_revision(tx)?;
                let live_lease = load_live_lease(tx, command.created_at)?;
                let actual_state =
                    super::runtime_host::actual_state_for_live_lease(tx, live_lease.as_ref())?;
                insert_authority(tx, &authority)?;
                insert_outbox(tx, &command.outbox_event)?;
                tx.execute(
                    r#"INSERT INTO execass_runtime_settings_revisions(
                      settings_revision,desired_mode,actual_state,start_at_login,settings_json,
                      settings_digest,idempotency_key,request_digest,policy_revision,
                      authority_provenance_id,outbox_event_id,receipt_id,created_at
                    ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
                    params![
                        next_revision,
                        command.desired_mode.as_str(),
                        actual_state.as_str(),
                        i64::from(command.start_at_login),
                        settings_json,
                        settings_digest,
                        command.idempotency_key,
                        request_digest,
                        policy_revision,
                        authority.authority_provenance_id,
                        command.outbox_event.event_id,
                        command.receipt.receipt_id,
                        command.created_at,
                    ],
                )
                .context("failed appending immutable runtime settings revision")?;
                let status = load_status(tx, command.created_at)?;
                let outbox = get_outbox(tx, &command.outbox_event.event_id)?
                    .context("new runtime-settings outbox event disappeared")?;
                Ok(AtomicReceiptMutation::Append(SettingsMutation::Updated(
                    status, outbox,
                )))
            })?;
        map_atomic(atomic)
    }
}

fn map_atomic(
    atomic: AtomicReceiptWriteOutcome<SettingsMutation>,
) -> Result<ExecAssRuntimeSettingsUpdateOutcome> {
    match atomic {
        AtomicReceiptWriteOutcome::Appended {
            value: SettingsMutation::Updated(status, outbox_event),
            receipt,
        } => Ok(ExecAssRuntimeSettingsUpdateOutcome::Updated {
            status,
            outbox_event,
            receipt,
        }),
        AtomicReceiptWriteOutcome::NoAppend(SettingsMutation::Replayed(
            status,
            outbox_event,
            receipt,
        )) => Ok(ExecAssRuntimeSettingsUpdateOutcome::Replayed {
            status,
            outbox_event,
            receipt,
        }),
        AtomicReceiptWriteOutcome::NoAppend(SettingsMutation::Stale(current_settings_revision)) => {
            Ok(ExecAssRuntimeSettingsUpdateOutcome::Stale {
                current_settings_revision,
            })
        }
        AtomicReceiptWriteOutcome::NoAppend(SettingsMutation::Conflict) => {
            Ok(ExecAssRuntimeSettingsUpdateOutcome::Conflict)
        }
        AtomicReceiptWriteOutcome::Stale { .. } => {
            bail!("canonical runtime-settings receipt carrier revision changed")
        }
        _ => bail!("runtime-settings mutation returned an impossible receipt outcome"),
    }
}

fn validate_combination(command: &UpdateExecAssRuntimeSettingsCommand) -> Result<()> {
    if command.expected_settings_revision < 0
        || command.created_at <= 0
        || command.idempotency_key.trim().is_empty()
        || (command.start_at_login && command.desired_mode != RuntimeDesiredMode::Background)
    {
        bail!("runtime settings contain an invalid mode/start-at-login combination");
    }
    Ok(())
}

fn validate_command(
    command: &UpdateExecAssRuntimeSettingsCommand,
    authority: &AuthorityProvenanceRecord,
    next_revision: i64,
    settings_digest: &str,
) -> Result<()> {
    if authority.authority_kind != AuthorityKind::RuntimeSettingsSnapshot
        || !matches!(
            authority.actor_type,
            ActorType::HumanLocal | ActorType::HumanRemote
        )
        || authority.policy_revision <= 0
        || authority.created_at != command.created_at
        || authority.expires_at.is_some()
        || authority.source_correlation_id != command.outbox_event.correlation_id
        || command.receipt.receipt_kind != ReceiptKind::RuntimeSettings
        || command.receipt.subject.kind != ReceiptSubjectKind::RuntimeSettingsRevision
        || command.receipt.subject.subject_id != RUNTIME_AGGREGATE_ID
        || command.receipt.subject.revision != next_revision
        || command.receipt.delegation_id != GLOBAL_RECEIPT_CARRIER_ID
        || command.receipt.causation_event_id != command.outbox_event.event_id
        || command.receipt.causation_id != command.outbox_event.causation_id
        || command.receipt.occurred_at != command.created_at
        || command.receipt.committed_at != command.created_at
        || command.receipt.actor.authority_provenance_id != authority.authority_provenance_id
        || command.receipt.actor.actor_type != authority.actor_type
        || command.receipt.actor.actor_identity.as_str() != authority.credential_identity
        || command.outbox_event.event_name != OutboxEventName::RuntimeHostChanged
        || command.outbox_event.aggregate_id != RUNTIME_AGGREGATE_ID
        || command.outbox_event.aggregate_revision != next_revision
        || command.outbox_event.duplicate_identity != command.idempotency_key
        || command.outbox_event.occurred_at != command.created_at
    {
        bail!("runtime settings are not bound to their exact owner, revision, receipt, and outbox event");
    }
    let expected_scope = json!({
        "settings_revision": next_revision,
        "settings_digest": settings_digest,
    });
    if serde_json::from_str::<serde_json::Value>(&authority.normalized_scope_json)?
        != expected_scope
    {
        bail!("runtime-settings owner authority is not bound to the exact safe configuration");
    }
    let expected_state = if command.desired_mode == RuntimeDesiredMode::Background {
        "running_background"
    } else {
        "running_app_bound"
    };
    let expected_payload = json!({
        "actual_state_if_running": expected_state,
        "desired_mode": command.desired_mode.as_str(),
        "settings_digest": settings_digest,
        "settings_revision": next_revision,
        "start_at_login": command.start_at_login,
    })
    .to_string();
    if command.outbox_event.safe_payload_json != expected_payload {
        bail!("runtime-settings outbox payload is not the deterministic safe configuration projection");
    }
    Ok(())
}

struct StoredSettingsReplay {
    request_digest: String,
    outbox_event_id: String,
    actor_type: ActorType,
    credential_identity: String,
}

fn load_by_idempotency(conn: &Connection, key: &str) -> Result<Option<StoredSettingsReplay>> {
    conn.query_row(
        "SELECT settings.request_digest,settings.outbox_event_id,authority.actor_type,authority.credential_identity FROM execass_runtime_settings_revisions settings JOIN execass_authority_provenance authority ON authority.authority_provenance_id=settings.authority_provenance_id WHERE settings.idempotency_key=?1",
        [key],
        |row| Ok(StoredSettingsReplay { request_digest: row.get(0)?, outbox_event_id: row.get(1)?, actor_type: row.get(2)?, credential_identity: row.get(3)? }),
    ).optional().map_err(Into::into)
}

fn load_latest_settings(conn: &Connection) -> Result<Option<ExecAssRuntimeSettingsRevisionRecord>> {
    conn.query_row(
        "SELECT settings_revision,desired_mode,start_at_login,settings_json,settings_digest,policy_revision,authority_provenance_id,created_at FROM execass_runtime_settings_revisions ORDER BY settings_revision DESC LIMIT 1",
        [],
        |row| Ok(ExecAssRuntimeSettingsRevisionRecord {
            settings_revision: row.get(0)?, desired_mode: row.get(1)?, start_at_login: row.get::<_, i64>(2)? != 0,
            settings_json: row.get(3)?, settings_digest: row.get(4)?, policy_revision: row.get(5)?,
            authority_provenance_id: row.get(6)?, created_at: row.get(7)?,
        }),
    ).optional().map_err(Into::into)
}

pub(super) fn load_status(conn: &Connection, trusted_now: i64) -> Result<ExecAssRuntimeHostStatus> {
    let config = load_latest_settings(conn)?;
    let live_lease = load_live_lease(conn, trusted_now)?;
    Ok(ExecAssRuntimeHostStatus {
        actual_state: super::runtime_host::actual_state_for_live_lease(conn, live_lease.as_ref())?,
        config,
        live_lease,
    })
}

fn load_live_lease(conn: &Connection, trusted_now: i64) -> Result<Option<RuntimeHostLeaseRecord>> {
    conn.query_row(
        r#"SELECT lease.lease_id,generation.state_root_generation,lease.generation,
              lease.host_instance_id,lease.fencing_token,lease.acquired_at,lease.expires_at
              FROM execass_runtime_host_leases lease
              JOIN execass_runtime_host_generations generation
                ON generation.generation=lease.generation
                AND generation.host_instance_id=lease.host_instance_id
              WHERE lease.ownership_scope='execass' AND lease.released_at IS NULL
                AND lease.expires_at>?1
              ORDER BY lease.generation DESC,lease.fencing_token DESC LIMIT 1"#,
        [trusted_now],
        |row| {
            Ok(RuntimeHostLeaseRecord {
                lease_id: row.get(0)?,
                state_root_generation: row.get(1)?,
                generation: row.get(2)?,
                host_instance_id: row.get(3)?,
                fencing_token: row.get(4)?,
                acquired_at: row.get(5)?,
                expires_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn current_settings_revision(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(settings_revision),0) FROM execass_runtime_settings_revisions",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn current_policy_revision(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT current_policy_revision FROM execass_global_runtime_control WHERE singleton=1",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn authority_exists(conn: &Connection, authority_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM execass_authority_provenance WHERE authority_provenance_id=?1",
            [authority_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn canonical_object_json(value: &super::redaction::SafeJson) -> Result<String> {
    let bytes = value.canonical_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
    if !parsed.is_object() {
        bail!("runtime settings must be a JSON object");
    }
    String::from_utf8(bytes).context("canonical runtime-settings JSON is not UTF-8")
}

fn mutation_digest(
    command: &UpdateExecAssRuntimeSettingsCommand,
    _authority_id: &str,
    settings_digest: &str,
) -> String {
    digest(
        json!({
            "expected_settings_revision": command.expected_settings_revision,
            "idempotency_key": command.idempotency_key,
            "settings_digest": settings_digest,
        })
        .to_string()
        .as_bytes(),
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
