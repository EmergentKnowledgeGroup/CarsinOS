//! EA-215 atomic global stop-all control.  This is an operational circuit
//! breaker: it does not interpret user purpose or grant any new authority.

use super::receipt::{
    receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{get_authority, get_outbox, insert_authority, insert_outbox};
use super::run_control_attestation::{
    run_control_attestation_digest, verify_run_control_attestation, PinnedRunControlAttestationKey,
    PinnedRunControlIdentity, VerifiedRunControlAttestation, RUN_CONTROL_ATTESTATION_MAX_AGE_MS,
};
use super::store::ExecAssStore;
use super::types::*;
use super::validation::validate_outbox;
use anyhow::{bail, Context, Result};
use carsinos_protocol::execass::{
    ActorType as ProtocolActorType, RunControlAttestation, RunControlOperation, RunControlTarget,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};

const GLOBAL_STOP_AGGREGATE_ID: &str = "global-stop-all";
const GLOBAL_STOP_RECEIPT_CARRIER_ID: &str = "execass-global-control-carrier";

impl ExecAssStore {
    pub fn global_stop_status(&self) -> Result<GlobalStopStatus> {
        let conn = self.connection()?;
        read_status(&conn)
    }

    /// Returns a previously committed global-resume result from its exact
    /// signed attestation without attempting another state transition. This is
    /// the restart-safe replay seam: stale human evidence can never authorize
    /// a new resume, but byte-identical evidence may recover its durable result.
    pub fn replay_global_resume_attestation(
        &self,
        attestation: &RunControlAttestation,
    ) -> Result<Option<GlobalStopMutationOutcome>> {
        let conn = self.connection()?;
        let Some(replay) = load_run_control_replay(&conn, &attestation.payload.replay_identity)?
        else {
            return Ok(None);
        };
        let pinned = load_active_run_control_key(&conn, &self.root_identity)?;
        let verified = verify_run_control_attestation(attestation, &pinned, replay.verified_at)?;
        if verified.payload().operation != RunControlOperation::GlobalResume
            || verified.payload().target != RunControlTarget::Global
        {
            bail!("persisted run-control replay is not a global resume");
        }
        let authority = authority_from_run_control(&verified)?;
        let receipt = receipt_by_causation_event(&conn, &replay.outbox_event_id)?;
        let outbox = get_outbox(&conn, &replay.outbox_event_id)?;
        if replay.attestation_digest != verified.attestation_digest()
            || replay.authority_provenance_id != authority.authority_provenance_id
            || replay.signed_payload_json != serde_json::to_string(verified.payload())?
            || replay.signature_hex != attestation.signature_hex
            || get_authority(&conn, &authority.authority_provenance_id)?.as_ref()
                != Some(&authority)
            || receipt.as_ref().map(|stored| stored.receipt_id.as_str())
                != Some(replay.receipt_id.as_str())
            || outbox.as_ref().map(|stored| stored.event.event_id.as_str())
                != Some(replay.outbox_event_id.as_str())
        {
            bail!("run-control replay identity collides with different immutable bytes");
        }
        Ok(Some(GlobalStopMutationOutcome::Replayed(read_status(
            &conn,
        )?)))
    }

    /// Read-only construction context for a gateway global-control receipt.
    /// It exposes durable heads and live fencing, but no connection or receipt
    /// key material and performs no OS-custody lookup.
    pub fn read_global_receipt_context(
        &self,
        trusted_now: i64,
    ) -> Result<Option<GlobalReceiptContext>> {
        if trusted_now <= 0 {
            bail!("global receipt context requires a positive trusted clock");
        }
        let conn = self.connection()?;
        let global_stop = read_status(&conn)?;
        conn.query_row(
            r#"SELECT carrier.state_revision,journal.receipt_count,journal.receipt_head_digest,
              carrier.receipt_chain_count,carrier.receipt_chain_head_digest,g.state_root_generation,
              lease.generation,lease.host_instance_id,lease.fencing_token,
              COALESCE((SELECT status FROM execass_receipt_anchor_state WHERE singleton=1),'uninitialized')
              FROM execass_delegations carrier
              CROSS JOIN execass_receipt_journal_state journal
              JOIN execass_runtime_host_leases lease ON lease.ownership_scope='execass'
                AND lease.released_at IS NULL AND lease.expires_at>?1
              JOIN execass_runtime_host_generations g ON g.generation=lease.generation
                AND g.host_instance_id=lease.host_instance_id
              WHERE carrier.delegation_id=?2 AND journal.singleton=1
              ORDER BY lease.generation DESC,lease.fencing_token DESC LIMIT 2"#,
            params![trusted_now, GLOBAL_STOP_RECEIPT_CARRIER_ID],
            |row| {
                Ok(GlobalReceiptContext {
                    global_stop: global_stop.clone(),
                    carrier_state_revision: row.get(0)?,
                    global_receipt_count: row.get(1)?,
                    global_receipt_head_digest: row.get(2)?,
                    carrier_receipt_count: row.get(3)?,
                    carrier_receipt_head_digest: row.get(4)?,
                    state_root_generation: row.get(5)?,
                    runtime_host_generation: row.get(6)?,
                    runtime_host_instance_id: row.get(7)?,
                    runtime_fencing_token: row.get(8)?,
                    receipt_anchor_status: row.get(9)?,
                })
            },
        )
        .optional()
        .context("failed reading exact global receipt context")
    }

    pub fn engage_global_stop_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &EngageGlobalStopCommand,
    ) -> Result<GlobalStopMutationOutcome> {
        validate_engage(command)?;
        map_atomic(self.mutate_with_atomic_receipt(integrity, redactor, &command.receipt, |tx| {
            if receipt_by_causation_event(tx, &command.outbox_event.event_id)?.is_some() {
                return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Replayed(read_status(tx)?)));
            }
            let before = read_status(tx)?;
            if before.engaged {
                return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::AlreadyEngaged(before)));
            }
            if before.global_stop_epoch != command.expected_global_stop_epoch {
                return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Stale(before)));
            }
            require_engage_actor(tx, &command.receipt)?;
            let next_epoch = before.global_stop_epoch.checked_add(1).context("global stop epoch overflow")?;
            let changed = tx.execute(
                "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=?1,updated_at=?2 WHERE singleton=1 AND engaged=0 AND global_stop_epoch=?3",
                params![next_epoch, command.trusted_now, before.global_stop_epoch],
            )?;
            if changed != 1 { return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Stale(read_status(tx)?))); }
            let after = read_status(tx)?;
            validate_event(&command.outbox_event, &command.receipt, &after, "engaged")?;
            insert_outbox(tx, &command.outbox_event)?;
            Ok(AtomicReceiptMutation::Append(GlobalStopMutationOutcome::Engaged(after)))
        })?)
    }

    pub fn resume_global_stop_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &ResumeGlobalStopCommand,
    ) -> Result<GlobalStopMutationOutcome> {
        validate_resume(command)?;
        let effective_receipt = derived_resume_receipt(command)?;
        validate_command_binding(&command.outbox_event, &effective_receipt)?;
        map_atomic(self.mutate_with_atomic_receipt(integrity, redactor, &effective_receipt, |tx| {
            let pinned = load_active_run_control_key(tx, &self.root_identity)?;
            if let Some(replay) = load_run_control_replay(tx, &command.attestation.payload.replay_identity)? {
                validate_exact_replay(tx, command, &effective_receipt, &pinned, &replay)?;
                return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Replayed(read_status(tx)?)));
            }
            let before = read_status(tx)?;
            if !before.engaged || before.global_stop_epoch != command.expected_global_stop_epoch {
                return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Stale(before)));
            }
            if before.current_policy_revision != command.expected_policy_revision
                || before.unresolved_external_effects_digest != command.disclosed_unresolved_external_effects_digest {
                return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Stale(before)));
            }
            let verified = verify_run_control_attestation(
                &command.attestation,
                &pinned,
                command.trusted_now,
            )?;
            require_exact_global_resume_binding(self, tx, command, &before, &verified)?;
            let authority = authority_from_run_control(&verified)?;
            if effective_receipt.actor.authority_provenance_id != authority.authority_provenance_id
                || effective_receipt.actor.actor_type != authority.actor_type
                || effective_receipt.actor.actor_identity.as_str() != authority.credential_identity
            {
                bail!("resume receipt actor is not derived from the verified attestation");
            }
            insert_authority(tx, &authority)?;
            insert_verified_run_control_attestation(
                tx,
                &command.attestation,
                command.trusted_now,
                &effective_receipt,
                &command.outbox_event,
                &verified,
                &authority,
            )?;
            let changed = tx.execute(
                "UPDATE execass_global_runtime_control SET engaged=0,updated_at=?1 WHERE singleton=1 AND engaged=1 AND global_stop_epoch=?2",
                params![command.trusted_now, command.expected_global_stop_epoch],
            )?;
            if changed != 1 { return Ok(AtomicReceiptMutation::NoAppend(GlobalStopMutationOutcome::Stale(read_status(tx)?))); }
            let after = read_status(tx)?;
            validate_event(&command.outbox_event, &command.receipt, &after, "resumed")?;
            insert_outbox(tx, &command.outbox_event)?;
            Ok(AtomicReceiptMutation::Append(GlobalStopMutationOutcome::Resumed(after)))
        })?)
    }
}

fn map_atomic(
    outcome: AtomicReceiptWriteOutcome<GlobalStopMutationOutcome>,
) -> Result<GlobalStopMutationOutcome> {
    match outcome {
        AtomicReceiptWriteOutcome::Appended { value, .. }
        | AtomicReceiptWriteOutcome::NoAppend(value) => Ok(value),
        AtomicReceiptWriteOutcome::Stale { .. } => {
            bail!("global stop receipt carrier delegation changed")
        }
    }
}

fn read_status(conn: &Connection) -> Result<GlobalStopStatus> {
    let (engaged, global_stop_epoch, current_policy_revision): (i64, i64, i64) = conn.query_row(
        "SELECT engaged,global_stop_epoch,current_policy_revision FROM execass_global_runtime_control WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut statement = conn.prepare(
        r#"SELECT effect.logical_effect_id,effect.delegation_id,effect.continuation_id,effect.state,
                  (SELECT attempt_id FROM execass_provider_attempts attempt WHERE attempt.logical_effect_id=effect.logical_effect_id ORDER BY attempt.attempt_number DESC LIMIT 1)
           FROM execass_logical_effects effect
           WHERE effect.state IN ('claimed','invoking','outcome_unknown')
             AND effect.action_kind IN (
               'public_or_externally_consequential_communication',
               'irreversible_or_destructive_action',
               'credential_permission_privilege_or_trust_policy_change',
               'project_defining_scope_ownership_or_launch_decision',
               'secret_use_through_authorized_connector',
               'unknown_composite_aliased_plugin_shell_or_changed_version_action'
             )
           ORDER BY effect.delegation_id,effect.logical_effect_id"#,
    )?;
    let unresolved_external_effects = statement
        .query_map([], |row| {
            Ok(UnresolvedExternalEffectReference {
                logical_effect_id: row.get(0)?,
                delegation_id: row.get(1)?,
                continuation_id: row.get(2)?,
                state: row.get(3)?,
                latest_attempt_id: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let executing: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM execass_continuations WHERE status='executing') + (SELECT COUNT(*) FROM execass_action_branches WHERE status='executing')", [], |row| row.get(0),
    )?;
    let drain_state = if engaged == 0 {
        GlobalStopDrainState::Running
    } else if executing > 0 {
        GlobalStopDrainState::Draining
    } else {
        GlobalStopDrainState::Drained
    };
    Ok(GlobalStopStatus {
        engaged: engaged != 0,
        global_stop_epoch,
        drain_state,
        current_policy_revision,
        unresolved_external_effects_digest: unresolved_digest(&unresolved_external_effects),
        unresolved_external_effects,
    })
}

fn unresolved_digest(effects: &[UnresolvedExternalEffectReference]) -> String {
    let canonical = effects.iter().map(|effect| json!({
        "logical_effect_id": effect.logical_effect_id, "delegation_id": effect.delegation_id,
        "continuation_id": effect.continuation_id, "state": effect.state.as_str(), "latest_attempt_id": effect.latest_attempt_id,
    })).collect::<Vec<_>>();
    let bytes =
        serde_json::to_vec(&canonical).expect("unresolved effect references are serializable");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn validate_engage(command: &EngageGlobalStopCommand) -> Result<()> {
    if command.expected_global_stop_epoch < 0 || command.trusted_now <= 0 {
        bail!("global stop engage requires a nonnegative epoch and positive trusted clock");
    }
    validate_command_binding(&command.outbox_event, &command.receipt)
}
fn validate_resume(command: &ResumeGlobalStopCommand) -> Result<()> {
    if command.expected_global_stop_epoch <= 0
        || command.expected_policy_revision <= 0
        || command.trusted_now <= 0
        || command.disclosed_unresolved_external_effects_digest.len() != 71
        || !command
            .disclosed_unresolved_external_effects_digest
            .starts_with("sha256:")
    {
        bail!("global stop resume proof is incomplete");
    }
    run_control_attestation_digest(&command.attestation)?;
    Ok(())
}
fn validate_command_binding(event: &NewOutboxEvent, receipt: &AppendReceiptCommand) -> Result<()> {
    validate_outbox(event)?;
    if event.event_name != OutboxEventName::GlobalStopChanged
        || event.aggregate_id != GLOBAL_STOP_AGGREGATE_ID
        || event.event_id != receipt.causation_event_id
    {
        bail!("global stop event is not bound to its receipt");
    }
    if receipt.receipt_kind != ReceiptKind::GlobalStop
        || receipt.subject.kind != ReceiptSubjectKind::GlobalRuntimeControl
        || receipt.subject.subject_id != GLOBAL_STOP_AGGREGATE_ID
        || receipt.delegation_id != GLOBAL_STOP_RECEIPT_CARRIER_ID
        || event.causation_id != receipt.causation_id
        || event.occurred_at != receipt.occurred_at
        || receipt.committed_at != receipt.occurred_at
    {
        bail!("global stop receipt is not an exact global-control receipt");
    }
    Ok(())
}
fn validate_event(
    event: &NewOutboxEvent,
    receipt: &AppendReceiptCommand,
    status: &GlobalStopStatus,
    operation: &str,
) -> Result<()> {
    if event.aggregate_revision != status.global_stop_epoch
        || receipt.subject.revision != status.global_stop_epoch
    {
        bail!("global stop event revision does not match the resulting epoch");
    }
    let expected = json!({"operation": operation, "engaged": status.engaged, "global_stop_epoch": status.global_stop_epoch,
        "drain_state": status.drain_state.as_str(), "current_policy_revision": status.current_policy_revision,
        "unresolved_external_effects_digest": status.unresolved_external_effects_digest}).to_string();
    if event.safe_payload_json != expected {
        bail!("global stop outbox payload is not the deterministic status disclosure");
    }
    Ok(())
}
fn require_exact_global_resume_binding(
    store: &ExecAssStore,
    tx: &Transaction<'_>,
    command: &ResumeGlobalStopCommand,
    status: &GlobalStopStatus,
    verified: &VerifiedRunControlAttestation,
) -> Result<()> {
    let payload = verified.payload();
    let actor_type = storage_actor_type(payload.actor_type)?;
    let active: Option<i64> = tx.query_row(
        "SELECT provider_event_required FROM execass_owner_ingress_bindings WHERE actor_type=?1 AND credential_identity=?2 AND authenticated_ingress=?3 AND channel_assurance=?4 AND status='active'",
        params![actor_type.as_str(), payload.credential_identity, payload.authenticated_ingress, payload.channel_assurance], |row| row.get(0)).optional()?;
    if active != Some(i64::from(actor_type == ActorType::HumanRemote)) {
        bail!("resume authority does not match an active owner ingress");
    }
    let stopped_at: i64 = tx.query_row(
        "SELECT updated_at FROM execass_global_runtime_control WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if payload.operation != RunControlOperation::GlobalResume
        || payload.target != RunControlTarget::Global
        || payload.stopped_epoch != status.global_stop_epoch
        || payload.stopped_epoch != command.expected_global_stop_epoch
        || payload.policy_revision != status.current_policy_revision
        || payload.policy_revision != command.expected_policy_revision
        || payload.unresolved_effect_disclosure_digest != status.unresolved_external_effects_digest
        || payload.unresolved_effect_disclosure_digest
            != command.disclosed_unresolved_external_effects_digest
        || payload.delegation_state_revision.is_some()
        || payload.current_plan_revision.is_some()
        || payload.idempotency_key != command.outbox_event.duplicate_identity
        || payload.request_correlation_id != command.outbox_event.correlation_id
        || payload.issued_at_ms != command.receipt.occurred_at
        || payload.observed_at_ms < stopped_at
        || payload.canonical_root_identity != store.root_identity
    {
        bail!("run-control attestation is not bound to the exact live global stop state");
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct PersistedRunControlReplay {
    pub(super) attestation_digest: String,
    pub(super) authority_provenance_id: String,
    pub(super) signed_payload_json: String,
    pub(super) signature_hex: String,
    pub(super) verified_at: i64,
    pub(super) receipt_id: String,
    pub(super) outbox_event_id: String,
    pub(super) receipt_command_digest: String,
    pub(super) outbox_event_digest: String,
}

pub(super) fn load_active_run_control_key(
    tx: &Connection,
    canonical_root_identity: &str,
) -> Result<PinnedRunControlAttestationKey> {
    let mut statement = tx.prepare(
        "SELECT key_id,key_generation,verifying_key_hex,verifying_key_digest,canonical_root_identity,installation_identity,os_user_identity_digest,state_root_generation FROM execass_confirmation_authority_keys WHERE status='active' ORDER BY key_generation,key_id LIMIT 2",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                PinnedRunControlIdentity {
                    key_id: row.get(0)?,
                    generation: row.get(1)?,
                    canonical_root_identity: row.get(4)?,
                    installation_identity: row.get(5)?,
                    os_user_identity_digest: row.get(6)?,
                    state_root_generation: row.get(7)?,
                },
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let [(identity, verifying_key_hex, verifying_key_digest)] = rows.as_slice() else {
        bail!("canonical storage must have exactly one active confirmation authority key");
    };
    let decoded_key = decode_lower_hex::<32>(verifying_key_hex)
        .context("active run-control verification key is malformed")?;
    if identity.canonical_root_identity != canonical_root_identity
        || format!("{:x}", Sha256::digest(decoded_key)) != *verifying_key_digest
    {
        bail!("active run-control key does not match its canonical storage identity");
    }
    PinnedRunControlAttestationKey::from_hex(identity.clone(), verifying_key_hex)
        .map_err(Into::into)
}

fn derived_resume_receipt(command: &ResumeGlobalStopCommand) -> Result<AppendReceiptCommand> {
    derived_human_run_control_receipt(&command.attestation, &command.receipt)
}

pub(super) fn derived_human_run_control_receipt(
    attestation: &RunControlAttestation,
    template: &AppendReceiptCommand,
) -> Result<AppendReceiptCommand> {
    let digest = run_control_attestation_digest(attestation)?;
    let payload = &attestation.payload;
    let mut receipt = template.clone();
    receipt.actor = ReceiptActorBinding {
        actor_type: storage_actor_type(payload.actor_type)?,
        actor_identity: super::redaction::SafeText::new(&payload.credential_identity, &[])?,
        authority_provenance_id: authority_id(&digest),
    };
    Ok(receipt)
}

fn storage_actor_type(actor_type: ProtocolActorType) -> Result<ActorType> {
    match actor_type {
        ProtocolActorType::HumanLocal => Ok(ActorType::HumanLocal),
        ProtocolActorType::HumanRemote => Ok(ActorType::HumanRemote),
        _ => bail!("run control requires a verified human actor"),
    }
}

fn authority_id(attestation_digest: &str) -> String {
    format!("run-control:{attestation_digest}")
}

fn canonical_run_control_scope(
    payload: &carsinos_protocol::execass::RunControlAttestationPayload,
) -> Result<String> {
    let (target_kind, target_delegation_id) = match &payload.target {
        RunControlTarget::Global => ("global", None),
        RunControlTarget::Delegation { delegation_id } => {
            ("delegation", Some(delegation_id.as_str()))
        }
    };
    let operation = match payload.operation {
        RunControlOperation::GlobalStop => "global_stop",
        RunControlOperation::GlobalResume => "global_resume",
        RunControlOperation::DelegationStop => "delegation_stop",
        RunControlOperation::DelegationResume => "delegation_resume",
    };
    serde_json::to_string(&json!({
        "current_plan_revision": payload.current_plan_revision,
        "delegation_state_revision": payload.delegation_state_revision,
        "operation": operation,
        "policy_revision": payload.policy_revision,
        "stopped_epoch": payload.stopped_epoch,
        "target_delegation_id": target_delegation_id,
        "target_kind": target_kind,
        "unresolved_effect_disclosure_digest": payload.unresolved_effect_disclosure_digest,
    }))
    .map_err(Into::into)
}

pub(super) fn authority_from_run_control(
    verified: &VerifiedRunControlAttestation,
) -> Result<AuthorityProvenanceRecord> {
    let payload = verified.payload();
    let expires_at = payload
        .observed_at_ms
        .checked_add(RUN_CONTROL_ATTESTATION_MAX_AGE_MS)
        .context("run-control attestation expiry overflows the trusted clock")?;
    if expires_at <= payload.issued_at_ms {
        bail!("run-control attestation cannot create a non-expiring authority row");
    }
    Ok(AuthorityProvenanceRecord {
        authority_provenance_id: authority_id(verified.attestation_digest()),
        actor_type: storage_actor_type(payload.actor_type)?,
        credential_identity: payload.credential_identity.clone(),
        authenticated_ingress: payload.authenticated_ingress.clone(),
        channel_assurance: payload.channel_assurance.clone(),
        source_correlation_id: payload.request_correlation_id.clone(),
        source_message_id: payload.source_message_id.clone(),
        authority_kind: AuthorityKind::RunControlAttestation,
        normalized_scope_json: canonical_run_control_scope(payload)?,
        policy_revision: payload.policy_revision,
        bound_decision_id: None,
        bound_decision_revision: None,
        bound_manifest_digest: None,
        bound_challenge_nonce_digest: None,
        evidence_digest: verified.attestation_digest().to_string(),
        created_at: payload.issued_at_ms,
        expires_at: Some(expires_at),
    })
}

pub(super) fn insert_verified_run_control_attestation(
    tx: &Transaction<'_>,
    attestation: &RunControlAttestation,
    trusted_now: i64,
    receipt: &AppendReceiptCommand,
    event: &NewOutboxEvent,
    verified: &VerifiedRunControlAttestation,
    authority: &AuthorityProvenanceRecord,
) -> Result<()> {
    let payload = verified.payload();
    let (operation, target_kind, target_delegation_id) = match (&payload.operation, &payload.target)
    {
        (RunControlOperation::GlobalResume, RunControlTarget::Global) => {
            ("global_resume", "global", None::<&str>)
        }
        (RunControlOperation::DelegationStop, RunControlTarget::Delegation { delegation_id }) => (
            "delegation_stop",
            "delegation",
            Some(delegation_id.as_str()),
        ),
        (RunControlOperation::DelegationResume, RunControlTarget::Delegation { delegation_id }) => {
            (
                "delegation_resume",
                "delegation",
                Some(delegation_id.as_str()),
            )
        }
        _ => bail!("signed run-control consumption requires an exact supported operation"),
    };
    tx.execute(
        r#"INSERT INTO execass_run_control_attestations(
          attestation_digest,replay_identity,authority_provenance_id,pinned_key_id,pinned_key_generation,
          actor_type,credential_identity,authenticated_ingress,channel_assurance,request_correlation_id,
          source_message_id,provider_event_id,operation,target_kind,target_delegation_id,idempotency_key,
          stopped_epoch,policy_revision,unresolved_effect_disclosure_digest,delegation_state_revision,
          current_plan_revision,canonical_root_identity,installation_identity,os_user_identity_digest,
          state_root_generation,normalized_scope_json,signed_payload_json,signature_hex,observed_at,issued_at,
          verified_at,receipt_id,outbox_event_id,receipt_command_digest,outbox_event_digest
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,
          ?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35)"#,
        params![
            verified.attestation_digest(), payload.replay_identity, authority.authority_provenance_id,
            verified.key_id(), payload.signer_key_generation, authority.actor_type.as_str(),
            payload.credential_identity, payload.authenticated_ingress, payload.channel_assurance,
            payload.request_correlation_id, payload.source_message_id, payload.provider_event_id,
            operation, target_kind, target_delegation_id, payload.idempotency_key, payload.stopped_epoch,
            payload.policy_revision, payload.unresolved_effect_disclosure_digest,
            payload.delegation_state_revision, payload.current_plan_revision,
            payload.canonical_root_identity, payload.installation_identity, payload.os_user_identity_digest,
            payload.state_root_generation, authority.normalized_scope_json, serde_json::to_string(payload)?,
            attestation.signature_hex, payload.observed_at_ms, payload.issued_at_ms,
            trusted_now, receipt.receipt_id, event.event_id,
            receipt_command_digest(receipt), outbox_event_digest(event),
        ],
    )
    .context("persisting verified run-control attestation")?;
    Ok(())
}

pub(super) fn load_run_control_replay(
    tx: &Connection,
    replay_identity: &str,
) -> Result<Option<PersistedRunControlReplay>> {
    tx.query_row(
        "SELECT attestation_digest,authority_provenance_id,signed_payload_json,signature_hex,verified_at,receipt_id,outbox_event_id,receipt_command_digest,outbox_event_digest FROM execass_run_control_attestations WHERE replay_identity=?1",
        [replay_identity],
        |row| Ok(PersistedRunControlReplay {
            attestation_digest: row.get(0)?, authority_provenance_id: row.get(1)?,
            signed_payload_json: row.get(2)?, signature_hex: row.get(3)?, verified_at: row.get(4)?,
            receipt_id: row.get(5)?, outbox_event_id: row.get(6)?, receipt_command_digest: row.get(7)?,
            outbox_event_digest: row.get(8)?,
        }),
    ).optional().map_err(Into::into)
}

fn validate_exact_replay(
    tx: &Transaction<'_>,
    command: &ResumeGlobalStopCommand,
    receipt: &AppendReceiptCommand,
    pinned: &PinnedRunControlAttestationKey,
    replay: &PersistedRunControlReplay,
) -> Result<()> {
    validate_exact_run_control_replay(
        tx,
        &command.attestation,
        receipt,
        &command.outbox_event,
        pinned,
        replay,
    )
}

pub(super) fn validate_exact_run_control_replay(
    tx: &Connection,
    attestation: &RunControlAttestation,
    receipt: &AppendReceiptCommand,
    event: &NewOutboxEvent,
    pinned: &PinnedRunControlAttestationKey,
    replay: &PersistedRunControlReplay,
) -> Result<()> {
    let verified = verify_run_control_attestation(attestation, pinned, replay.verified_at)?;
    let authority = authority_from_run_control(&verified)?;
    if replay.attestation_digest != verified.attestation_digest()
        || replay.authority_provenance_id != authority.authority_provenance_id
        || replay.signed_payload_json != serde_json::to_string(verified.payload())?
        || replay.signature_hex != attestation.signature_hex
        || replay.receipt_id != receipt.receipt_id
        || replay.outbox_event_id != event.event_id
        || replay.receipt_command_digest != receipt_command_digest(receipt)
        || replay.outbox_event_digest != outbox_event_digest(event)
        || get_authority(tx, &authority.authority_provenance_id)?.as_ref() != Some(&authority)
        || receipt_by_causation_event(tx, &event.event_id)?
            .is_none_or(|stored| stored.receipt_id != receipt.receipt_id)
        || get_outbox(tx, &event.event_id)?
            .as_ref()
            .is_none_or(|stored| stored.event != *event)
    {
        bail!("run-control replay identity collides with different immutable bytes");
    }
    Ok(())
}

pub(super) fn receipt_command_digest(receipt: &AppendReceiptCommand) -> String {
    let evidence = receipt
        .evidence
        .iter()
        .map(|item| {
            json!({
                "authority_link_id": item.authority_link_id,
                "kind": item.kind.as_str(),
                "source_id": item.source_id,
                "authoritative_revision": item.authoritative_revision,
            })
        })
        .collect::<Vec<_>>();
    let rotation = receipt.rotation.as_ref().map(|item| {
        json!({
            "transition_id": item.transition_id,
            "reason": item.reason.as_str(),
            "previous_key_id": item.previous_key.key_id,
            "previous_key_generation": item.previous_key.key_generation,
        })
    });
    digest_json(json!({
        "receipt_id": receipt.receipt_id, "transaction_id": receipt.transaction_id,
        "state_root_generation": receipt.state_root_generation, "delegation_id": receipt.delegation_id,
        "expected_state_revision": receipt.expected_state_revision,
        "expected_global_count": receipt.expected_global_count,
        "expected_global_head_digest": receipt.expected_global_head_digest,
        "expected_delegation_count": receipt.expected_delegation_count,
        "expected_delegation_head_digest": receipt.expected_delegation_head_digest,
        "receipt_kind": receipt.receipt_kind.as_str(), "subject_kind": receipt.subject.kind.as_str(),
        "subject_id": receipt.subject.subject_id, "subject_revision": receipt.subject.revision,
        "causation_id": receipt.causation_id, "causation_event_id": receipt.causation_event_id,
        "actor_type": receipt.actor.actor_type.as_str(), "actor_identity": receipt.actor.actor_identity.as_str(),
        "authority_provenance_id": receipt.actor.authority_provenance_id,
        "host_generation": receipt.runtime.host_generation, "host_instance_id": receipt.runtime.host_instance_id,
        "fencing_token": receipt.runtime.fencing_token, "key_id": receipt.key.key_id,
        "key_generation": receipt.key.key_generation, "rotation": rotation, "evidence": evidence,
        "redacted_summary": receipt.redacted_summary.as_str(), "occurred_at": receipt.occurred_at,
        "committed_at": receipt.committed_at,
    }))
}

pub(super) fn outbox_event_digest(event: &NewOutboxEvent) -> String {
    digest_json(json!({
        "event_id": event.event_id, "event_name": event.event_name.as_str(),
        "aggregate_id": event.aggregate_id, "aggregate_revision": event.aggregate_revision,
        "correlation_id": event.correlation_id, "causation_id": event.causation_id,
        "occurred_at": event.occurred_at, "safe_payload_json": event.safe_payload_json,
        "duplicate_identity": event.duplicate_identity,
    }))
}

fn digest_json(value: serde_json::Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

fn decode_lower_hex<const N: usize>(value: &str) -> Result<[u8; N]> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        bail!("malformed lowercase hexadecimal value");
    }
    let mut output = [0_u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(output)
}

fn require_engage_actor(tx: &Transaction<'_>, receipt: &AppendReceiptCommand) -> Result<()> {
    let authority = get_authority(tx, &receipt.actor.authority_provenance_id)?
        .context("stop-all actor authority is absent")?;
    let allowed = matches!(
        authority.actor_type,
        ActorType::HumanLocal | ActorType::HumanRemote
    ) || (authority.actor_type == ActorType::Runtime
        && authority.authority_kind == AuthorityKind::RuntimeSafetyState);
    if !allowed
        || authority.actor_type != receipt.actor.actor_type
        || authority.credential_identity != receipt.actor.actor_identity.as_str()
    {
        bail!(
            "stop-all engagement requires a verified human or trusted fail-safe runtime authority"
        );
    }
    Ok(())
}
