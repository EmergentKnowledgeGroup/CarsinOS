#![cfg_attr(not(test), allow(dead_code))]

use super::types::*;
use super::validation::{
    validate_authority, validate_continuation, validate_criterion, validate_delegation,
    validate_outbox, validate_plan,
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn insert_authority(
    conn: &Connection,
    record: &AuthorityProvenanceRecord,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_authority_provenance (
          authority_provenance_id, actor_type, credential_identity, authenticated_ingress,
          channel_assurance, source_correlation_id, source_message_id, authority_kind,
          normalized_scope_json, policy_revision, bound_decision_id, bound_decision_revision,
          bound_manifest_digest, bound_challenge_nonce_digest, evidence_digest, created_at, expires_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
        params![
            record.authority_provenance_id,
            record.actor_type.as_str(),
            record.credential_identity,
            record.authenticated_ingress,
            record.channel_assurance,
            record.source_correlation_id,
            record.source_message_id,
            record.authority_kind.as_str(),
            record.normalized_scope_json,
            record.policy_revision,
            record.bound_decision_id,
            record.bound_decision_revision,
            record.bound_manifest_digest,
            record.bound_challenge_nonce_digest,
            record.evidence_digest,
            record.created_at,
            record.expires_at,
        ],
    )
    .context("failed inserting ExecAss authority provenance")?;
    Ok(())
}

pub(super) fn insert_delegation(conn: &Connection, record: &DelegationRecord) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_delegations (
          delegation_id, normalized_original_intent, intake_evidence_json, ingress_source,
          ingress_credential_identity, source_message_id, source_correlation_id,
          ingress_idempotency_key, classifier_version, classifier_reasons_json, phase,
          run_control, state_revision, current_plan_revision, current_criteria_revision,
          policy_revision, effective_authority_json, authority_provenance_id,
          pending_decision_id, external_wait_json, stop_epoch, completion_assessment_json,
          receipt_chain_count, receipt_chain_head_digest, created_at, updated_at,
          acknowledged_at, terminal_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                  ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)
        "#,
        params![
            record.delegation_id,
            record.normalized_original_intent,
            record.intake_evidence_json,
            record.ingress_source,
            record.ingress_credential_identity,
            record.source_message_id,
            record.source_correlation_id,
            record.ingress_idempotency_key,
            record.classifier_version,
            record.classifier_reasons_json,
            record.phase.as_str(),
            record.run_control.as_str(),
            record.state_revision,
            record.current_plan_revision,
            record.current_criteria_revision,
            record.policy_revision,
            record.effective_authority_json,
            record.authority_provenance_id,
            record.pending_decision_id,
            record.external_wait_json,
            record.stop_epoch,
            record.completion_assessment_json,
            record.receipt_chain_count,
            record.receipt_chain_head_digest,
            record.created_at,
            record.updated_at,
            record.acknowledged_at,
            record.terminal_at,
        ],
    )
    .context("failed inserting ExecAss delegation")?;
    Ok(())
}

pub(super) fn insert_plan(conn: &Connection, record: &PlanRecord) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_plans (
          plan_id, delegation_id, plan_revision, based_on_delegation_revision, policy_revision,
          plan_summary, resolved_leaf_manifest_json, manifest_digest,
          created_by_authority_provenance_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            record.plan_id,
            record.delegation_id,
            record.plan_revision,
            record.based_on_delegation_revision,
            record.policy_revision,
            record.plan_summary,
            record.resolved_leaf_manifest_json,
            record.manifest_digest,
            record.created_by_authority_provenance_id,
            record.created_at,
        ],
    )
    .context("failed inserting immutable ExecAss plan")?;
    Ok(())
}

pub(super) fn insert_criterion(conn: &Connection, record: &OutcomeCriterionRecord) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_outcome_criteria (
          criterion_id, delegation_id, criteria_revision, criterion_key, description, material,
          verifier_type, expected_predicate_json, authoritative_source_kind, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            record.criterion_id,
            record.delegation_id,
            record.criteria_revision,
            record.criterion_key,
            record.description,
            record.material as i64,
            record.verifier_type.as_str(),
            record.expected_predicate_json,
            record.authoritative_source_kind,
            record.created_at,
        ],
    )
    .context("failed inserting immutable ExecAss outcome criterion")?;
    Ok(())
}

pub(super) fn insert_continuation(conn: &Connection, record: &ContinuationRecord) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_continuations (
          continuation_id, delegation_id, target_delegation_revision, target_plan_revision,
          action_id, branch_kind, causation_kind, causation_id, status, job_id, lease_owner, lease_expires_at,
          fencing_token, host_generation, stop_epoch, global_stop_epoch, created_at, updated_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
        "#,
        params![
            record.continuation_id,
            record.delegation_id,
            record.target_delegation_revision,
            record.target_plan_revision,
            record.action_id,
            record.branch_kind.as_str(),
            record.causation_kind.as_str(),
            record.causation_id,
            record.status.as_str(),
            record.job_id,
            record.lease_owner,
            record.lease_expires_at,
            record.fencing_token,
            record.host_generation,
            record.stop_epoch,
            record.global_stop_epoch,
            record.created_at,
            record.updated_at,
            record.completed_at,
        ],
    )
    .context("failed inserting initial ExecAss continuation")?;
    Ok(())
}

pub(super) fn insert_action_branch(conn: &Connection, record: &ActionBranchRecord) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_action_branches (
          action_id, delegation_id, action_revision, target_delegation_revision,
          target_plan_revision, stop_epoch, branch_kind, status, action_summary, created_at,
          updated_at, terminal_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            record.action_id,
            record.delegation_id,
            record.action_revision,
            record.target_delegation_revision,
            record.target_plan_revision,
            record.stop_epoch,
            record.branch_kind.as_str(),
            record.status.as_str(),
            record.action_summary,
            record.created_at,
            record.updated_at,
            record.terminal_at,
        ],
    )
    .context("failed inserting ExecAss action branch")?;
    Ok(())
}

pub(super) fn insert_planned_logical_effect(
    conn: &Connection,
    record: &PlannedLogicalEffectRecord,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_logical_effects (
          logical_effect_id, delegation_id, continuation_id, action_kind,
          operation_reversible, declared_recovery_safe_boundary, state,
          internal_idempotency_key, provider_identity, provider_idempotency_key,
          reconciliation_key, manifest_digest, payload_digest, outcome_json, created_at, updated_at
        ) VALUES (?1,?2,?3,?4,?5,?6,'planned',?7,?8,?9,?10,?11,?12,NULL,?13,?13)
        "#,
        params![
            record.logical_effect_id,
            record.delegation_id,
            record.continuation_id,
            record.action_kind.as_str(),
            record.operation_reversible as i64,
            record.declared_recovery_safe_boundary.as_str(),
            record.internal_idempotency_key,
            record.provider_identity,
            record.provider_idempotency_key,
            record.reconciliation_key,
            record.manifest_digest,
            record.payload_digest,
            record.created_at,
        ],
    )
    .context("failed inserting planned ExecAss logical effect")?;
    Ok(())
}

pub(super) fn insert_technical_quota_snapshot(
    conn: &Connection,
    snapshot: &carsinos_core::execass_policy::CanonicalTechnicalQuotaSnapshot,
    created_at: i64,
) -> Result<()> {
    conn.execute(
        r#"INSERT INTO execass_technical_resource_quota_snapshots(
             quota_snapshot_id,delegation_id,policy_revision,effective_authority_digest,
             scope_key,canonical_entries_json,canonical_entries_digest,created_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
           ON CONFLICT(quota_snapshot_id) DO NOTHING"#,
        params![
            snapshot.quota_snapshot_id,
            snapshot.delegation_id,
            snapshot.policy_revision,
            snapshot.effective_authority_digest,
            snapshot.scope_key,
            snapshot.canonical_entries_json,
            snapshot.canonical_entries_digest,
            created_at,
        ],
    )
    .context("failed inserting ExecAss technical quota snapshot")?;
    for entry in &snapshot.entries {
        conn.execute(
            r#"INSERT INTO execass_technical_resource_quota_entries(
                 quota_snapshot_id,technical_resource_kind,unit,amount_limit
               ) VALUES(?1,?2,?3,?4)
               ON CONFLICT(quota_snapshot_id,technical_resource_kind,unit) DO NOTHING"#,
            params![
                snapshot.quota_snapshot_id,
                entry.kind.as_str(),
                entry.unit,
                entry.limit,
            ],
        )
        .context("failed inserting ExecAss technical quota entry")?;
    }
    Ok(())
}

pub(super) fn insert_technical_resource_requirements(
    conn: &Connection,
    requirements: &carsinos_core::execass_policy::CanonicalTechnicalResourceRequirementSet,
    created_at: i64,
) -> Result<()> {
    conn.execute(
        r#"INSERT INTO execass_technical_resource_requirement_sets(
             requirement_set_id,quota_snapshot_id,delegation_id,logical_effect_id,action_id,
             manifest_digest,canonical_requirements_json,canonical_requirements_digest,created_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
        params![
            requirements.requirement_set_id,
            requirements.quota_snapshot_id,
            requirements.delegation_id,
            requirements.logical_effect_id,
            requirements.action_id,
            requirements.manifest_digest,
            requirements.canonical_requirements_json,
            requirements.canonical_requirements_digest,
            created_at,
        ],
    )
    .context("failed inserting ExecAss technical resource requirement set")?;
    for requirement in &requirements.requirements {
        conn.execute(
            r#"INSERT INTO execass_technical_resource_requirements(
                 requirement_set_id,quota_snapshot_id,technical_resource_kind,unit,amount_required
               ) VALUES(?1,?2,?3,?4,?5)"#,
            params![
                requirements.requirement_set_id,
                requirements.quota_snapshot_id,
                requirement.kind.as_str(),
                requirement.unit,
                requirement.amount,
            ],
        )
        .context("failed inserting ExecAss technical resource requirement")?;
    }
    Ok(())
}

pub(super) fn insert_outbox(conn: &Connection, record: &NewOutboxEvent) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO execass_outbox_events (
          event_id, event_name, aggregate_id, aggregate_revision, correlation_id,
          causation_id, occurred_at, schema_version, safe_payload_json, duplicate_identity
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'v1', ?8, ?9)
        "#,
        params![
            record.event_id,
            record.event_name.as_str(),
            record.aggregate_id,
            record.aggregate_revision,
            record.correlation_id,
            record.causation_id,
            record.occurred_at,
            record.safe_payload_json,
            record.duplicate_identity,
        ],
    )
    .context("failed inserting durable ExecAss outbox event")?;
    Ok(())
}

fn map_authority(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthorityProvenanceRecord> {
    Ok(AuthorityProvenanceRecord {
        authority_provenance_id: row.get(0)?,
        actor_type: row.get(1)?,
        credential_identity: row.get(2)?,
        authenticated_ingress: row.get(3)?,
        channel_assurance: row.get(4)?,
        source_correlation_id: row.get(5)?,
        source_message_id: row.get(6)?,
        authority_kind: row.get(7)?,
        normalized_scope_json: row.get(8)?,
        policy_revision: row.get(9)?,
        bound_decision_id: row.get(10)?,
        bound_decision_revision: row.get(11)?,
        bound_manifest_digest: row.get(12)?,
        bound_challenge_nonce_digest: row.get(13)?,
        evidence_digest: row.get(14)?,
        created_at: row.get(15)?,
        expires_at: row.get(16)?,
    })
}

pub(super) fn get_authority(
    conn: &Connection,
    id: &str,
) -> Result<Option<AuthorityProvenanceRecord>> {
    let record = conn
        .query_row(
            r#"SELECT authority_provenance_id, actor_type, credential_identity, authenticated_ingress,
                      channel_assurance, source_correlation_id, source_message_id, authority_kind,
                      normalized_scope_json, policy_revision, bound_decision_id, bound_decision_revision,
                      bound_manifest_digest, bound_challenge_nonce_digest, evidence_digest, created_at, expires_at
               FROM execass_authority_provenance WHERE authority_provenance_id = ?1"#,
            params![id],
            map_authority,
        )
        .optional()?;
    if let Some(record) = &record {
        validate_authority(record)?;
    }
    Ok(record)
}

fn map_delegation(row: &rusqlite::Row<'_>) -> rusqlite::Result<DelegationRecord> {
    Ok(DelegationRecord {
        delegation_id: row.get(0)?,
        normalized_original_intent: row.get(1)?,
        intake_evidence_json: row.get(2)?,
        ingress_source: row.get(3)?,
        ingress_credential_identity: row.get(4)?,
        source_message_id: row.get(5)?,
        source_correlation_id: row.get(6)?,
        ingress_idempotency_key: row.get(7)?,
        classifier_version: row.get(8)?,
        classifier_reasons_json: row.get(9)?,
        phase: row.get(10)?,
        run_control: row.get(11)?,
        state_revision: row.get(12)?,
        current_plan_revision: row.get(13)?,
        current_criteria_revision: row.get(14)?,
        policy_revision: row.get(15)?,
        effective_authority_json: row.get(16)?,
        authority_provenance_id: row.get(17)?,
        pending_decision_id: row.get(18)?,
        external_wait_json: row.get(19)?,
        stop_epoch: row.get(20)?,
        completion_assessment_json: row.get(21)?,
        receipt_chain_count: row.get(22)?,
        receipt_chain_head_digest: row.get(23)?,
        created_at: row.get(24)?,
        updated_at: row.get(25)?,
        acknowledged_at: row.get(26)?,
        terminal_at: row.get(27)?,
    })
}

pub(super) fn get_delegation(conn: &Connection, id: &str) -> Result<Option<DelegationRecord>> {
    let record = conn
        .query_row(
            r#"SELECT delegation_id, normalized_original_intent, intake_evidence_json, ingress_source,
                      ingress_credential_identity, source_message_id, source_correlation_id,
                      ingress_idempotency_key, classifier_version, classifier_reasons_json, phase,
                      run_control, state_revision, current_plan_revision, current_criteria_revision,
                      policy_revision, effective_authority_json, authority_provenance_id,
                      pending_decision_id, external_wait_json, stop_epoch, completion_assessment_json,
                      receipt_chain_count, receipt_chain_head_digest, created_at, updated_at,
                      acknowledged_at, terminal_at
               FROM execass_delegations WHERE delegation_id = ?1"#,
            params![id],
            map_delegation,
        )
        .optional()?;
    if let Some(record) = &record {
        validate_delegation(record)?;
    }
    Ok(record)
}

fn map_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlanRecord> {
    Ok(PlanRecord {
        plan_id: row.get(0)?,
        delegation_id: row.get(1)?,
        plan_revision: row.get(2)?,
        based_on_delegation_revision: row.get(3)?,
        policy_revision: row.get(4)?,
        plan_summary: row.get(5)?,
        resolved_leaf_manifest_json: row.get(6)?,
        manifest_digest: row.get(7)?,
        created_by_authority_provenance_id: row.get(8)?,
        created_at: row.get(9)?,
    })
}

const PLAN_SELECT: &str = r#"SELECT plan_id, delegation_id, plan_revision,
    based_on_delegation_revision, policy_revision, plan_summary,
    resolved_leaf_manifest_json, manifest_digest, created_by_authority_provenance_id, created_at
    FROM execass_plans"#;

pub(super) fn get_plan(conn: &Connection, id: &str) -> Result<Option<PlanRecord>> {
    let record = conn
        .query_row(
            &format!("{PLAN_SELECT} WHERE plan_id = ?1"),
            params![id],
            map_plan,
        )
        .optional()?;
    if let Some(record) = &record {
        validate_plan(record)?;
    }
    Ok(record)
}

pub(super) fn get_plan_by_revision(
    conn: &Connection,
    delegation_id: &str,
    revision: i64,
) -> Result<Option<PlanRecord>> {
    let record = conn
        .query_row(
            &format!("{PLAN_SELECT} WHERE delegation_id = ?1 AND plan_revision = ?2"),
            params![delegation_id, revision],
            map_plan,
        )
        .optional()?;
    if let Some(record) = &record {
        validate_plan(record)?;
    }
    Ok(record)
}

fn map_criterion(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutcomeCriterionRecord> {
    Ok(OutcomeCriterionRecord {
        criterion_id: row.get(0)?,
        delegation_id: row.get(1)?,
        criteria_revision: row.get(2)?,
        criterion_key: row.get(3)?,
        description: row.get(4)?,
        material: row.get::<_, i64>(5)? == 1,
        verifier_type: row.get(6)?,
        expected_predicate_json: row.get(7)?,
        authoritative_source_kind: row.get(8)?,
        created_at: row.get(9)?,
    })
}

pub(super) fn list_criteria(
    conn: &Connection,
    delegation_id: &str,
    revision: i64,
) -> Result<Vec<OutcomeCriterionRecord>> {
    let mut stmt = conn.prepare(
        r#"SELECT criterion_id, delegation_id, criteria_revision, criterion_key, description,
                  material, verifier_type, expected_predicate_json, authoritative_source_kind, created_at
           FROM execass_outcome_criteria WHERE delegation_id = ?1 AND criteria_revision = ?2
           ORDER BY criterion_key, criterion_id"#,
    )?;
    let records = stmt
        .query_map(params![delegation_id, revision], map_criterion)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for record in &records {
        validate_criterion(record)?;
    }
    Ok(records)
}

fn map_continuation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContinuationRecord> {
    Ok(ContinuationRecord {
        continuation_id: row.get(0)?,
        delegation_id: row.get(1)?,
        target_delegation_revision: row.get(2)?,
        target_plan_revision: row.get(3)?,
        action_id: row.get(4)?,
        branch_kind: row.get(5)?,
        causation_kind: row.get(6)?,
        causation_id: row.get(7)?,
        status: row.get(8)?,
        job_id: row.get(9)?,
        lease_owner: row.get(10)?,
        lease_expires_at: row.get(11)?,
        fencing_token: row.get(12)?,
        host_generation: row.get(13)?,
        stop_epoch: row.get(14)?,
        global_stop_epoch: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        completed_at: row.get(18)?,
    })
}

const CONTINUATION_SELECT: &str = r#"SELECT continuation_id, delegation_id,
    target_delegation_revision, target_plan_revision, action_id, branch_kind, causation_kind, causation_id, status,
    job_id, lease_owner, lease_expires_at, fencing_token, host_generation, stop_epoch,
    global_stop_epoch, created_at, updated_at, completed_at FROM execass_continuations"#;

pub(super) fn get_continuation(conn: &Connection, id: &str) -> Result<Option<ContinuationRecord>> {
    let record = conn
        .query_row(
            &format!("{CONTINUATION_SELECT} WHERE continuation_id = ?1"),
            params![id],
            map_continuation,
        )
        .optional()?;
    if let Some(record) = &record {
        validate_continuation(record)?;
    }
    Ok(record)
}

pub(super) fn get_planned_logical_effect(
    conn: &Connection,
    id: &str,
) -> Result<Option<PlannedLogicalEffectRecord>> {
    conn.query_row(
        r#"SELECT logical_effect_id,delegation_id,continuation_id,action_kind,
          operation_reversible,declared_recovery_safe_boundary,
          internal_idempotency_key,provider_identity,provider_idempotency_key,
          reconciliation_key,manifest_digest,payload_digest,created_at
          FROM execass_logical_effects WHERE logical_effect_id=?1 AND state='planned'"#,
        params![id],
        |row| {
            Ok(PlannedLogicalEffectRecord {
                logical_effect_id: row.get(0)?,
                delegation_id: row.get(1)?,
                continuation_id: row.get(2)?,
                action_kind: row.get(3)?,
                operation_reversible: row.get::<_, i64>(4)? == 1,
                declared_recovery_safe_boundary: row.get(5)?,
                internal_idempotency_key: row.get(6)?,
                provider_identity: row.get(7)?,
                provider_idempotency_key: row.get(8)?,
                reconciliation_key: row.get(9)?,
                manifest_digest: row.get(10)?,
                payload_digest: row.get(11)?,
                created_at: row.get(12)?,
            })
        },
    )
    .optional()
    .context("failed reading planned ExecAss logical effect")
}

pub(super) fn get_technical_quota_snapshot(
    conn: &Connection,
    quota_snapshot_id: &str,
) -> Result<Option<TechnicalQuotaSnapshotRecord>> {
    let header = conn
        .query_row(
            r#"SELECT quota_snapshot_id,delegation_id,policy_revision,effective_authority_digest,scope_key,
                      canonical_entries_json,canonical_entries_digest,created_at
               FROM execass_technical_resource_quota_snapshots
               WHERE quota_snapshot_id=?1"#,
            params![quota_snapshot_id],
            |row| {
                Ok(TechnicalQuotaSnapshotRecord {
                    quota_snapshot_id: row.get(0)?,
                    delegation_id: row.get(1)?,
                    policy_revision: row.get(2)?,
                    effective_authority_digest: row.get(3)?,
                    scope_key: row.get(4)?,
                    canonical_entries_json: row.get(5)?,
                    canonical_entries_digest: row.get(6)?,
                    created_at: row.get(7)?,
                    entries: Vec::new(),
                })
            },
        )
        .optional()
        .context("failed reading ExecAss technical quota snapshot")?;
    let Some(mut snapshot) = header else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        r#"SELECT quota_snapshot_id,technical_resource_kind,unit,amount_limit
           FROM execass_technical_resource_quota_entries
           WHERE quota_snapshot_id=?1
           ORDER BY CASE technical_resource_kind
             WHEN 'tokens' THEN 0
             WHEN 'time_ms' THEN 1
             WHEN 'connector_calls' THEN 2
             WHEN 'resource_units' THEN 3
           END,unit"#,
    )?;
    snapshot.entries = statement
        .query_map(params![snapshot.quota_snapshot_id], |row| {
            Ok(TechnicalQuotaEntryRecord {
                quota_snapshot_id: row.get(0)?,
                technical_resource_kind: row.get(1)?,
                unit: row.get(2)?,
                amount_limit: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(snapshot))
}

pub(super) fn get_technical_resource_requirements_for_effect(
    conn: &Connection,
    logical_effect_id: &str,
) -> Result<Option<TechnicalResourceRequirementSetRecord>> {
    let header = conn
        .query_row(
            r#"SELECT requirement_set_id,quota_snapshot_id,delegation_id,logical_effect_id,
                      action_id,manifest_digest,canonical_requirements_json,
                      canonical_requirements_digest,created_at
               FROM execass_technical_resource_requirement_sets WHERE logical_effect_id=?1"#,
            params![logical_effect_id],
            |row| {
                Ok(TechnicalResourceRequirementSetRecord {
                    requirement_set_id: row.get(0)?,
                    quota_snapshot_id: row.get(1)?,
                    delegation_id: row.get(2)?,
                    logical_effect_id: row.get(3)?,
                    action_id: row.get(4)?,
                    manifest_digest: row.get(5)?,
                    canonical_requirements_json: row.get(6)?,
                    canonical_requirements_digest: row.get(7)?,
                    created_at: row.get(8)?,
                    requirements: Vec::new(),
                })
            },
        )
        .optional()
        .context("failed reading ExecAss technical resource requirement set")?;
    let Some(mut set) = header else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        r#"SELECT requirement_set_id,quota_snapshot_id,technical_resource_kind,unit,amount_required
           FROM execass_technical_resource_requirements WHERE requirement_set_id=?1
           ORDER BY CASE technical_resource_kind
             WHEN 'tokens' THEN 0
             WHEN 'time_ms' THEN 1
             WHEN 'connector_calls' THEN 2
             WHEN 'resource_units' THEN 3
           END,unit"#,
    )?;
    set.requirements = statement
        .query_map(params![set.requirement_set_id], |row| {
            Ok(TechnicalResourceRequirementRecord {
                requirement_set_id: row.get(0)?,
                quota_snapshot_id: row.get(1)?,
                technical_resource_kind: row.get(2)?,
                unit: row.get(3)?,
                amount_required: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(set))
}

pub(super) fn initial_continuation(
    conn: &Connection,
    delegation_id: &str,
) -> Result<Option<ContinuationRecord>> {
    let mut stmt = conn.prepare(
        "SELECT continuation_id FROM execass_continuations WHERE delegation_id = ?1 AND causation_kind IN ('intake','routine_occurrence') ORDER BY created_at, continuation_id LIMIT 2",
    )?;
    let ids = stmt
        .query_map(params![delegation_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > 1 {
        bail!("ExecAss foundation has multiple initial continuations");
    }
    ids.first()
        .map(|id| get_continuation(conn, id))
        .transpose()
        .map(Option::flatten)
}

fn map_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<(OutboxEventRecord, String)> {
    let schema_version: String = row.get(8)?;
    Ok((
        OutboxEventRecord {
            global_sequence: row.get(0)?,
            event: NewOutboxEvent {
                event_id: row.get(1)?,
                event_name: row.get(2)?,
                aggregate_id: row.get(3)?,
                aggregate_revision: row.get(4)?,
                correlation_id: row.get(5)?,
                causation_id: row.get(6)?,
                occurred_at: row.get(7)?,
                safe_payload_json: row.get(9)?,
                duplicate_identity: row.get(10)?,
            },
            published_at: row.get(11)?,
        },
        schema_version,
    ))
}

const OUTBOX_SELECT: &str = r#"SELECT global_sequence, event_id, event_name, aggregate_id,
    aggregate_revision, correlation_id, causation_id, occurred_at, schema_version,
    safe_payload_json, duplicate_identity, published_at FROM execass_outbox_events"#;

fn validate_outbox_row(row: (OutboxEventRecord, String)) -> Result<OutboxEventRecord> {
    if row.1 != "v1" {
        bail!("invalid ExecAss outbox schema version from storage");
    }
    validate_outbox(&row.0.event)?;
    Ok(row.0)
}

pub(super) fn get_outbox(conn: &Connection, event_id: &str) -> Result<Option<OutboxEventRecord>> {
    conn.query_row(
        &format!("{OUTBOX_SELECT} WHERE event_id = ?1"),
        params![event_id],
        map_outbox,
    )
    .optional()?
    .map(validate_outbox_row)
    .transpose()
}

pub(super) fn list_outbox(conn: &Connection, aggregate_id: &str) -> Result<Vec<OutboxEventRecord>> {
    let mut stmt = conn.prepare(&format!(
        "{OUTBOX_SELECT} WHERE aggregate_id = ?1 ORDER BY global_sequence, event_id"
    ))?;
    let raw = stmt
        .query_map(params![aggregate_id], map_outbox)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raw.into_iter().map(validate_outbox_row).collect()
}

pub(super) fn list_outbox_after(
    conn: &Connection,
    global_sequence: i64,
) -> Result<Vec<OutboxEventRecord>> {
    let mut stmt = conn.prepare(&format!(
        "{OUTBOX_SELECT} WHERE global_sequence > ?1 ORDER BY global_sequence, event_id"
    ))?;
    let raw = stmt
        .query_map(params![global_sequence], map_outbox)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raw.into_iter().map(validate_outbox_row).collect()
}
