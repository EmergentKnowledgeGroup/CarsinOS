#![cfg_attr(not(test), allow(dead_code))]

use super::lifecycle::record_genesis_lifecycle_history;
use super::rows::*;
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{require_text, validate_foundation_command};
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{owner_normalized_intent_digest, VerifiedOwnerAuthority};
use carsinos_core::execass_danger::{DangerAdmissionState, SignedDangerAdmissionProof};
use carsinos_core::execass_manifest::{
    canonicalize_owner_authority, compile_dispatch, CanonicalOwnerAuthority,
    CanonicalOwnerEvidence, DispatchTree, ManifestCompilation, ServerResolutionRegistry,
};
use carsinos_core::execass_policy::ExactOwnerActionAuthority;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

impl ExecAssStore {
    /// Sole public foundation admission path. Mechanical resolution completes before
    /// a database connection or write transaction is opened.
    pub fn admit_foundation_dispatch(
        &self,
        command: &CreateFoundationCommand,
        dispatch: &DispatchTree,
        resolutions: &ServerResolutionRegistry,
        expected_owner_authority: &VerifiedOwnerAuthority,
        authorized_actions: &[ExactOwnerActionAuthority],
        danger_admission: &SignedDangerAdmissionProof,
    ) -> Result<FoundationDispatchAdmissionOutcome> {
        let manifest = match compile_dispatch(dispatch, resolutions) {
            ManifestCompilation::Ready(manifest) => manifest,
            ManifestCompilation::MechanicalResolutionRequired(pause) => {
                return Ok(FoundationDispatchAdmissionOutcome::MechanicalResolutionRequired(pause));
            }
        };
        match self.verify_danger_admission(danger_admission, &manifest)? {
            DangerAdmissionState::Ordinary => {}
            DangerAdmissionState::RequiresOneConfirmation => {
                return Ok(FoundationDispatchAdmissionOutcome::DangerConfirmationRequired);
            }
        }
        let owner_authority = canonicalize_owner_authority(expected_owner_authority)
            .map_err(|detail| anyhow::anyhow!("invalid expected owner authority: {detail}"))?;
        if owner_normalized_intent_digest(&command.delegation.normalized_original_intent).as_deref()
            != Some(owner_authority.normalized_intent_digest().as_hex())
        {
            bail!("delegation normalized intent does not match authenticated owner authority");
        }
        if manifest
            .leaves()
            .iter()
            .any(|leaf| leaf.owner_authority() != &owner_authority)
        {
            bail!("dispatch manifest authority does not match authenticated admission authority");
        }
        if authorized_actions.len() != manifest.leaves().len()
            || manifest.leaves().iter().any(|leaf| {
                authorized_actions
                    .iter()
                    .filter(|authorization| authorization.matches(expected_owner_authority, leaf))
                    .count()
                    != 1
            })
        {
            bail!("every dispatch leaf requires one exact action-bound owner authorization");
        }
        if let Some(continuation) = &command.initial_continuation {
            if !manifest
                .leaves()
                .iter()
                .any(|leaf| leaf.logical_action_id() == continuation.action_id)
            {
                bail!("initial continuation action is absent from dispatch manifest");
            }
        }
        let mut admitted = command.clone();
        admitted.authority = authority_record_from_manifest(&owner_authority)?;
        admitted.delegation.ingress_source = admitted.authority.authenticated_ingress.clone();
        admitted.delegation.ingress_credential_identity =
            admitted.authority.credential_identity.clone();
        admitted.delegation.source_message_id = admitted.authority.source_message_id.clone();
        admitted.delegation.source_correlation_id =
            admitted.authority.source_correlation_id.clone();
        admitted.delegation.policy_revision = admitted.authority.policy_revision;
        admitted.delegation.authority_provenance_id =
            admitted.authority.authority_provenance_id.clone();
        admitted.plan.policy_revision = admitted.authority.policy_revision;
        admitted.plan.created_by_authority_provenance_id =
            admitted.authority.authority_provenance_id.clone();
        admitted.plan.resolved_leaf_manifest_json =
            String::from_utf8(manifest.canonical().bytes().to_vec())
                .expect("canonical manifest is UTF-8 JSON");
        admitted.plan.manifest_digest = manifest.canonical().digest().as_hex().to_string();
        Ok(FoundationDispatchAdmissionOutcome::Admitted(Box::new(
            self.create_foundation(&admitted)?,
        )))
    }

    pub(super) fn create_foundation(
        &self,
        command: &CreateFoundationCommand,
    ) -> Result<FoundationWriteOutcome> {
        // Every JSON-bearing field and semantic binding is checked before the
        // writable connection or IMMEDIATE transaction is opened.
        validate_foundation_command(command)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let outcome = create_foundation_in_tx(&tx, command)?;
        tx.commit()
            .context("failed committing ExecAss foundation transaction")?;
        Ok(outcome)
    }

    pub fn read_foundation(&self, delegation_id: &str) -> Result<Option<FoundationBundle>> {
        require_text("delegation_id", delegation_id)?;
        let conn = self.connection()?;
        read_foundation_with_conn(&conn, delegation_id)
    }
}

/// Transaction-aware foundation kernel used by typed orchestration paths that
/// must bind their own source record and scheduler settlement in the same
/// commit. Callers must validate the command before opening the transaction.
pub(super) fn create_foundation_in_tx(
    tx: &Transaction<'_>,
    command: &CreateFoundationCommand,
) -> Result<FoundationWriteOutcome> {
    if let Some(existing_id) = find_idempotent_delegation(tx, command)? {
        if foundation_matches_command(tx, command, &existing_id)? {
            let current = read_foundation_with_conn(tx, &existing_id)?
                .context("idempotent ExecAss foundation disappeared")?;
            return Ok(FoundationWriteOutcome::Replayed(current));
        }
        return Ok(FoundationWriteOutcome::Conflict {
            existing_delegation_id: Some(existing_id),
        });
    }

    if foundation_identity_exists(tx, command)? {
        return Ok(FoundationWriteOutcome::Conflict {
            existing_delegation_id: get_delegation(tx, &command.delegation.delegation_id)?
                .map(|record| record.delegation_id),
        });
    }

    tx.pragma_update(None, "defer_foreign_keys", "ON")
        .context("failed deferring ExecAss foundation foreign keys")?;
    if let Some(existing) = get_authority(tx, &command.authority.authority_provenance_id)? {
        if existing != command.authority {
            bail!("foundation authority provenance identity collision");
        }
    } else {
        insert_authority(tx, &command.authority)?;
    }
    insert_delegation(tx, &command.delegation)?;
    insert_plan(tx, &command.plan)?;
    for criterion in &command.outcome_criteria {
        insert_criterion(tx, criterion)?;
    }
    tx.execute(
        "INSERT INTO execass_criteria_sets (criteria_set_id,delegation_id,criteria_revision,parent_criteria_revision,disposition,created_at) VALUES (?1,?2,?3,NULL,'genesis',?4)",
        params![format!("criteria-set:{}:{}",command.delegation.delegation_id,command.outcome_criteria[0].criteria_revision),command.delegation.delegation_id,command.outcome_criteria[0].criteria_revision,command.write.occurred_at],
    )?;
    if let Some(continuation) = &command.initial_continuation {
        insert_action_branch(
            tx,
            &ActionBranchRecord {
                action_id: continuation.action_id.clone(),
                delegation_id: continuation.delegation_id.clone(),
                action_revision: 1,
                target_delegation_revision: continuation.target_delegation_revision,
                target_plan_revision: continuation.target_plan_revision,
                stop_epoch: continuation.stop_epoch,
                branch_kind: continuation.branch_kind,
                status: continuation.status,
                action_summary: "initial durable continuation".into(),
                created_at: continuation.created_at,
                updated_at: continuation.updated_at,
                terminal_at: continuation.completed_at,
            },
        )?;
        insert_continuation(tx, continuation)?;
    }
    insert_outbox(tx, &command.outbox_event)?;
    record_genesis_lifecycle_history(tx, command)?;

    if !foundation_matches_command(tx, command, &command.delegation.delegation_id)? {
        bail!(
            "ExecAss foundation verification did not match requested immutable rows: {}",
            foundation_mismatch_label(tx, command)?
        );
    }
    let bundle = read_foundation_with_conn(tx, &command.delegation.delegation_id)?
        .context("created ExecAss foundation could not be reloaded")?;
    Ok(FoundationWriteOutcome::Created(bundle))
}

fn foundation_mismatch_label(
    conn: &Connection,
    command: &CreateFoundationCommand,
) -> Result<&'static str> {
    let delegation_id = &command.delegation.delegation_id;
    if get_authority(conn, &command.authority.authority_provenance_id)?.as_ref()
        != Some(&command.authority)
    {
        return Ok("authority");
    }
    if !get_delegation(conn, delegation_id)?
        .as_ref()
        .is_some_and(|stored| immutable_intake_matches(stored, &command.delegation))
    {
        return Ok("delegation");
    }
    if get_plan(conn, &command.plan.plan_id)?.as_ref() != Some(&command.plan) {
        return Ok("plan");
    }
    if get_outbox(conn, &command.outbox_event.event_id)?
        .as_ref()
        .map(|record| &record.event)
        != Some(&command.outbox_event)
    {
        return Ok("outbox");
    }
    if list_criteria(
        conn,
        delegation_id,
        command.outcome_criteria[0].criteria_revision,
    )? != canonical_criteria(command.outcome_criteria.clone())
    {
        return Ok("criteria");
    }
    if let Some(requested) = &command.initial_continuation {
        if !get_continuation(conn, &requested.continuation_id)?
            .as_ref()
            .is_some_and(|stored| immutable_continuation_matches(stored, requested))
        {
            return Ok("continuation");
        }
    }
    Ok("initial_continuation")
}

pub(super) fn authority_record_from_manifest(
    authority: &CanonicalOwnerAuthority,
) -> Result<AuthorityProvenanceRecord> {
    let (
        actor_type,
        credential_identity,
        authenticated_ingress,
        channel_assurance,
        source_correlation_id,
        source_message_id,
    ) = match authority.owner_evidence() {
        CanonicalOwnerEvidence::LocalInteractive {
            authenticated_client_id,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id,
            source_message_id,
        } => (
            ActorType::HumanLocal,
            authenticated_client_id.clone(),
            authenticated_ingress.clone(),
            channel_assurance.clone(),
            request_correlation_id.clone(),
            source_message_id.clone(),
        ),
        CanonicalOwnerEvidence::RemoteAuthenticated {
            adapter_id,
            provider_account_id,
            authenticated_ingress,
            channel_assurance,
            source_message_id,
            request_correlation_id,
        } => (
            ActorType::HumanRemote,
            format!("{adapter_id}:{provider_account_id}"),
            authenticated_ingress.clone(),
            channel_assurance.clone(),
            request_correlation_id.clone(),
            Some(source_message_id.clone()),
        ),
    };
    let authority_kind = match authority.authority_kind() {
        "original_request" => AuthorityKind::OriginalRequest,
        "decision_resolution" => AuthorityKind::DecisionResolution,
        "action_specific_owner_amendment" => AuthorityKind::ActionSpecificOwnerAmendment,
        "policy_snapshot" => AuthorityKind::PolicySnapshot,
        "runtime_settings_snapshot" => AuthorityKind::RuntimeSettingsSnapshot,
        "runtime_safety_state" => AuthorityKind::RuntimeSafetyState,
        other => bail!("unsupported verified owner authority kind: {other}"),
    };
    Ok(AuthorityProvenanceRecord {
        authority_provenance_id: authority.authority_provenance_id().to_string(),
        actor_type,
        credential_identity,
        authenticated_ingress,
        channel_assurance,
        source_correlation_id,
        source_message_id,
        authority_kind,
        normalized_scope_json: authority.normalized_scope_json().to_string(),
        policy_revision: authority.policy_revision(),
        bound_decision_id: authority.bound_decision_id().map(str::to_string),
        bound_decision_revision: authority.bound_decision_revision(),
        bound_manifest_digest: authority
            .bound_manifest_digest()
            .map(|digest| digest.as_hex().to_string()),
        bound_challenge_nonce_digest: authority
            .bound_challenge_nonce_digest()
            .map(|digest| digest.as_hex().to_string()),
        evidence_digest: authority.evidence_digest().as_hex().to_string(),
        created_at: authority.created_at(),
        expires_at: authority.expires_at(),
    })
}

fn find_idempotent_delegation(
    conn: &Connection,
    command: &CreateFoundationCommand,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        SELECT delegation_id FROM execass_delegations
        WHERE ingress_source = ?1
          AND ingress_credential_identity = ?2
          AND ingress_idempotency_key = ?3
        "#,
        params![
            command.delegation.ingress_source,
            command.delegation.ingress_credential_identity,
            command.delegation.ingress_idempotency_key,
        ],
        |row| row.get(0),
    )
    .optional()
    .context("failed checking ExecAss foundation idempotency")
}

fn foundation_identity_exists(
    conn: &Connection,
    command: &CreateFoundationCommand,
) -> Result<bool> {
    let mut identities = vec![
        (
            "execass_delegations",
            "delegation_id",
            command.delegation.delegation_id.as_str(),
        ),
        ("execass_plans", "plan_id", command.plan.plan_id.as_str()),
        (
            "execass_outbox_events",
            "event_id",
            command.outbox_event.event_id.as_str(),
        ),
    ];
    if let Some(continuation) = &command.initial_continuation {
        identities.push((
            "execass_continuations",
            "continuation_id",
            continuation.continuation_id.as_str(),
        ));
    }
    for (table, column, value) in identities {
        let sql = format!("SELECT 1 FROM {table} WHERE {column} = ?1 LIMIT 1");
        if conn
            .query_row(&sql, params![value], |_| Ok(()))
            .optional()?
            .is_some()
        {
            return Ok(true);
        }
    }
    for criterion in &command.outcome_criteria {
        if conn
            .query_row(
                "SELECT 1 FROM execass_outcome_criteria WHERE criterion_id = ?1 LIMIT 1",
                params![criterion.criterion_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn immutable_intake_matches(stored: &DelegationRecord, requested: &DelegationRecord) -> bool {
    stored.delegation_id == requested.delegation_id
        && stored.normalized_original_intent == requested.normalized_original_intent
        && stored.intake_evidence_json == requested.intake_evidence_json
        && stored.ingress_source == requested.ingress_source
        && stored.ingress_credential_identity == requested.ingress_credential_identity
        && stored.source_message_id == requested.source_message_id
        && stored.source_correlation_id == requested.source_correlation_id
        && stored.ingress_idempotency_key == requested.ingress_idempotency_key
        && stored.classifier_version == requested.classifier_version
        && stored.classifier_reasons_json == requested.classifier_reasons_json
        && stored.authority_provenance_id == requested.authority_provenance_id
        && stored.created_at == requested.created_at
}

fn immutable_continuation_matches(
    stored: &ContinuationRecord,
    requested: &ContinuationRecord,
) -> bool {
    stored.continuation_id == requested.continuation_id
        && stored.delegation_id == requested.delegation_id
        && stored.target_delegation_revision == requested.target_delegation_revision
        && stored.target_plan_revision == requested.target_plan_revision
        && stored.action_id == requested.action_id
        && stored.branch_kind == requested.branch_kind
        && stored.causation_kind == requested.causation_kind
        && stored.causation_id == requested.causation_id
        && stored.stop_epoch == requested.stop_epoch
        && stored.created_at == requested.created_at
}

fn canonical_criteria(mut criteria: Vec<OutcomeCriterionRecord>) -> Vec<OutcomeCriterionRecord> {
    criteria.sort_by(|left, right| {
        left.criterion_key
            .cmp(&right.criterion_key)
            .then_with(|| left.criterion_id.cmp(&right.criterion_id))
    });
    criteria
}

fn foundation_matches_command(
    conn: &Connection,
    command: &CreateFoundationCommand,
    delegation_id: &str,
) -> Result<bool> {
    if delegation_id != command.delegation.delegation_id
        || get_authority(conn, &command.authority.authority_provenance_id)?.as_ref()
            != Some(&command.authority)
        || !get_delegation(conn, delegation_id)?
            .as_ref()
            .is_some_and(|stored| immutable_intake_matches(stored, &command.delegation))
        || get_plan(conn, &command.plan.plan_id)?.as_ref() != Some(&command.plan)
        || get_outbox(conn, &command.outbox_event.event_id)?
            .as_ref()
            .map(|record| &record.event)
            != Some(&command.outbox_event)
    {
        return Ok(false);
    }

    let stored_criteria = list_criteria(
        conn,
        delegation_id,
        command.outcome_criteria[0].criteria_revision,
    )?;
    if stored_criteria != canonical_criteria(command.outcome_criteria.clone()) {
        return Ok(false);
    }

    match &command.initial_continuation {
        Some(requested) => {
            if !get_continuation(conn, &requested.continuation_id)?
                .as_ref()
                .is_some_and(|stored| immutable_continuation_matches(stored, requested))
                || !initial_continuation(conn, delegation_id)?
                    .as_ref()
                    .is_some_and(|stored| immutable_continuation_matches(stored, requested))
            {
                return Ok(false);
            }
        }
        None if initial_continuation(conn, delegation_id)?.is_some() => return Ok(false),
        None => {}
    }
    Ok(true)
}

fn read_foundation_with_conn(
    conn: &Connection,
    delegation_id: &str,
) -> Result<Option<FoundationBundle>> {
    let Some(delegation) = get_delegation(conn, delegation_id)? else {
        return Ok(None);
    };
    let authority = get_authority(conn, &delegation.authority_provenance_id)?
        .context("ExecAss delegation authority provenance is missing")?;
    let plan_revision = delegation
        .current_plan_revision
        .context("ExecAss foundation delegation has no current plan")?;
    let plan = get_plan_by_revision(conn, delegation_id, plan_revision)?
        .context("ExecAss foundation current plan is missing")?;
    let criteria_revision = delegation
        .current_criteria_revision
        .context("ExecAss foundation delegation has no criteria revision")?;
    let outcome_criteria = list_criteria(conn, delegation_id, criteria_revision)?;
    if outcome_criteria.is_empty() {
        bail!("ExecAss foundation has no outcome criteria");
    }
    Ok(Some(FoundationBundle {
        authority,
        delegation,
        plan,
        outcome_criteria,
        initial_continuation: initial_continuation(conn, delegation_id)?,
        outbox_events: list_outbox(conn, delegation_id)?,
    }))
}
