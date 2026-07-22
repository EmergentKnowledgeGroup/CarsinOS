#![cfg_attr(not(test), allow(dead_code))]

use super::types::*;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn require_text(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

fn validate_json(field: &str, value: &str) -> Result<()> {
    serde_json::from_str::<Value>(value)
        .with_context(|| format!("{field} must contain valid JSON"))?;
    Ok(())
}

fn validate_optional_json(field: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        validate_json(field, value)?;
    }
    Ok(())
}

fn validate_write_context(context: &WriteContext) -> Result<()> {
    require_text("write.idempotency_key", &context.idempotency_key)?;
    require_text("write.correlation_id", &context.correlation_id)?;
    require_text("write.causation_id", &context.causation_id)
}

pub(super) fn validate_foundation_command(command: &CreateFoundationCommand) -> Result<()> {
    validate_foundation_command_for_causation(command, ContinuationCausationKind::Intake)
}

pub(super) fn validate_routine_foundation_command(
    command: &CreateFoundationCommand,
    occurrence_id: &str,
) -> Result<()> {
    if command.write.causation_id != occurrence_id {
        bail!("routine foundation causation must be the exact occurrence identity");
    }
    validate_foundation_command_for_causation(command, ContinuationCausationKind::RoutineOccurrence)
}

fn validate_foundation_command_for_causation(
    command: &CreateFoundationCommand,
    expected_continuation_causation: ContinuationCausationKind,
) -> Result<()> {
    validate_write_context(&command.write)?;
    validate_authority(&command.authority)?;
    validate_delegation(&command.delegation)?;
    validate_plan(&command.plan)?;
    validate_outbox(&command.outbox_event)?;
    if command.outcome_criteria.is_empty() {
        bail!("an ExecAss foundation requires at least one outcome criterion");
    }
    ensure_unique_ids(
        command
            .outcome_criteria
            .iter()
            .map(|record| record.criterion_id.as_str()),
        "criterion_id",
    )?;
    for criterion in &command.outcome_criteria {
        validate_criterion(criterion)?;
    }
    if let Some(continuation) = &command.initial_continuation {
        validate_continuation(continuation)?;
    }

    let authority = &command.authority;
    let delegation = &command.delegation;
    let plan = &command.plan;
    if authority.authority_kind != AuthorityKind::OriginalRequest
        || authority.source_correlation_id != command.write.correlation_id
        || delegation.source_correlation_id != command.write.correlation_id
        || authority.source_correlation_id != delegation.source_correlation_id
        || authority.authenticated_ingress != delegation.ingress_source
        || authority.credential_identity != delegation.ingress_credential_identity
        || authority.source_message_id != delegation.source_message_id
    {
        bail!("foundation authority and intake source provenance do not match");
    }
    if authority.policy_revision != delegation.policy_revision
        || plan.policy_revision != delegation.policy_revision
    {
        bail!("foundation authority, delegation, and plan policy revisions do not match");
    }
    if delegation.authority_provenance_id != authority.authority_provenance_id
        || plan.created_by_authority_provenance_id != authority.authority_provenance_id
        || plan.delegation_id != delegation.delegation_id
        || plan.based_on_delegation_revision != delegation.state_revision
        || delegation.current_plan_revision != Some(plan.plan_revision)
    {
        bail!("foundation plan is not exactly bound to the admitted delegation revision");
    }
    let criteria_revision = command.outcome_criteria[0].criteria_revision;
    if delegation.current_criteria_revision != Some(criteria_revision)
        || command.outcome_criteria.iter().any(|criterion| {
            criterion.delegation_id != delegation.delegation_id
                || criterion.criteria_revision != criteria_revision
        })
    {
        bail!("foundation criteria do not share the exact admitted criteria revision");
    }
    if let Some(continuation) = &command.initial_continuation {
        if continuation.delegation_id != delegation.delegation_id
            || continuation.target_delegation_revision != delegation.state_revision
            || continuation.target_plan_revision != plan.plan_revision
            || continuation.causation_kind != expected_continuation_causation
            || continuation.causation_id != command.write.causation_id
            || continuation.stop_epoch != delegation.stop_epoch
            || continuation.branch_kind != ActionBranchKind::Ordinary
        {
            bail!("initial continuation is not exactly bound to foundation causation");
        }
    }
    if command.write.idempotency_key != delegation.ingress_idempotency_key
        || command.write.idempotency_key != command.outbox_event.duplicate_identity
        || command.write.correlation_id != command.outbox_event.correlation_id
        || command.write.causation_id != command.outbox_event.causation_id
        || command.write.occurred_at != command.outbox_event.occurred_at
        || command.outbox_event.event_name != OutboxEventName::DelegationTransitioned
        || command.outbox_event.aggregate_id != delegation.delegation_id
        || command.outbox_event.aggregate_revision != delegation.state_revision
    {
        bail!("foundation outbox identity is not exactly bound to intake");
    }
    Ok(())
}

pub(super) fn validate_cas_command(command: &CasDelegationStateCommand) -> Result<()> {
    validate_write_context(&command.write)?;
    require_text("delegation_id", &command.delegation_id)?;
    validate_optional_json("external_wait_json", command.external_wait_json.as_deref())?;
    validate_outbox(&command.outbox_event)?;
    if command.new_state_revision != command.expected_state_revision + 1 {
        bail!("delegation CAS must advance the state revision by exactly one");
    }
    if command.phase.is_terminal() != command.terminal_at.is_some() {
        bail!("terminal_at must be present exactly for a terminal phase");
    }
    if command.write.idempotency_key != command.outbox_event.duplicate_identity
        || command.write.correlation_id != command.outbox_event.correlation_id
        || command.write.causation_id != command.outbox_event.causation_id
        || command.write.occurred_at != command.outbox_event.occurred_at
        || command.outbox_event.aggregate_id != command.delegation_id
        || command.outbox_event.aggregate_revision != command.new_state_revision
    {
        bail!("CAS write context and durable event identity must match");
    }
    Ok(())
}

pub(super) fn validate_lineage_command(command: &AppendAuthorityLineageCommand) -> Result<()> {
    validate_write_context(&command.write)?;
    require_text("delegation_id", &command.delegation_id)?;
    validate_outbox(&command.outbox_event)?;
    if command.resulting_state_revision != command.expected_state_revision + 1 {
        bail!("authority lineage CAS must advance the state revision by exactly one");
    }
    if command.links.is_empty() {
        bail!("authority lineage append requires at least one authority link");
    }
    if command.write.idempotency_key != command.outbox_event.duplicate_identity
        || command.write.correlation_id != command.outbox_event.correlation_id
        || command.write.causation_id != command.outbox_event.causation_id
        || command.write.occurred_at != command.outbox_event.occurred_at
        || command.outbox_event.aggregate_id != command.delegation_id
        || command.outbox_event.aggregate_revision != command.resulting_state_revision
        || command.linked_at != command.write.occurred_at
        || command.outbox_event.event_name != OutboxEventName::DelegationTransitioned
    {
        bail!("authority lineage write context and exact outbox identity must match");
    }
    ensure_unique_ids(
        command.links.iter().map(|link| link.link_id.as_str()),
        "link_id",
    )?;
    let mut members = HashSet::new();
    for link in &command.links {
        require_text("link_id", &link.link_id)?;
        let Some(kind) = link.target.kind() else {
            let AuthorityLinkTarget::Unsupported { kind } = &link.target else {
                unreachable!()
            };
            return Err(AuthorityLineageError::Unsupported(*kind).into());
        };
        let source_id = link
            .target
            .source_id()
            .expect("supported target has source id");
        require_text("authority source id", source_id)?;
        if !members.insert((kind.as_str(), source_id)) {
            bail!("duplicate authority target in one lineage append");
        }
    }
    Ok(())
}

pub(super) fn validate_authority(record: &AuthorityProvenanceRecord) -> Result<()> {
    require_text("authority_provenance_id", &record.authority_provenance_id)?;
    require_text("credential_identity", &record.credential_identity)?;
    require_text("authenticated_ingress", &record.authenticated_ingress)?;
    require_text("channel_assurance", &record.channel_assurance)?;
    require_text(
        "authority.source_correlation_id",
        &record.source_correlation_id,
    )?;
    require_text("authority.evidence_digest", &record.evidence_digest)?;
    validate_json("normalized_scope_json", &record.normalized_scope_json)
}

pub(super) fn validate_delegation(record: &DelegationRecord) -> Result<()> {
    require_text("delegation_id", &record.delegation_id)?;
    require_text(
        "normalized_original_intent",
        &record.normalized_original_intent,
    )?;
    require_text("ingress_source", &record.ingress_source)?;
    require_text(
        "ingress_credential_identity",
        &record.ingress_credential_identity,
    )?;
    require_text(
        "delegation.source_correlation_id",
        &record.source_correlation_id,
    )?;
    require_text("ingress_idempotency_key", &record.ingress_idempotency_key)?;
    require_text("classifier_version", &record.classifier_version)?;
    validate_json("intake_evidence_json", &record.intake_evidence_json)?;
    validate_json("classifier_reasons_json", &record.classifier_reasons_json)?;
    validate_json("effective_authority_json", &record.effective_authority_json)?;
    validate_optional_json("external_wait_json", record.external_wait_json.as_deref())?;
    validate_optional_json(
        "completion_assessment_json",
        record.completion_assessment_json.as_deref(),
    )
}

pub(super) fn validate_plan(record: &PlanRecord) -> Result<()> {
    require_text("plan_id", &record.plan_id)?;
    require_text("plan.delegation_id", &record.delegation_id)?;
    require_text("plan_summary", &record.plan_summary)?;
    require_text("manifest_digest", &record.manifest_digest)?;
    require_text(
        "plan.created_by_authority_provenance_id",
        &record.created_by_authority_provenance_id,
    )?;
    validate_json(
        "resolved_leaf_manifest_json",
        &record.resolved_leaf_manifest_json,
    )
}

pub(super) fn validate_criterion(record: &OutcomeCriterionRecord) -> Result<()> {
    require_text("criterion_id", &record.criterion_id)?;
    require_text("criterion.delegation_id", &record.delegation_id)?;
    require_text("criterion_key", &record.criterion_key)?;
    require_text("criterion.description", &record.description)?;
    require_text(
        "criterion.authoritative_source_kind",
        &record.authoritative_source_kind,
    )?;
    validate_json("expected_predicate_json", &record.expected_predicate_json)
}

pub(super) fn validate_continuation(record: &ContinuationRecord) -> Result<()> {
    require_text("continuation_id", &record.continuation_id)?;
    require_text("continuation.delegation_id", &record.delegation_id)?;
    require_text("continuation.action_id", &record.action_id)?;
    require_text("continuation.causation_id", &record.causation_id)?;
    if record.target_delegation_revision <= 0
        || record.target_plan_revision <= 0
        || record.fencing_token < 0
        || record.host_generation <= 0
        || record.stop_epoch < 0
        || record.global_stop_epoch < 0
        || record.created_at <= 0
        || record.updated_at < record.created_at
    {
        bail!("continuation has invalid revision, fence, epoch, or timestamp");
    }
    Ok(())
}

pub(super) fn validate_outbox(record: &NewOutboxEvent) -> Result<()> {
    require_text("event_id", &record.event_id)?;
    require_text("outbox.aggregate_id", &record.aggregate_id)?;
    require_text("outbox.correlation_id", &record.correlation_id)?;
    require_text("outbox.causation_id", &record.causation_id)?;
    require_text("outbox.duplicate_identity", &record.duplicate_identity)?;
    validate_json("safe_payload_json", &record.safe_payload_json)
}

fn ensure_unique_ids<'a>(values: impl Iterator<Item = &'a str>, field: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for value in values {
        require_text(field, value)?;
        if !seen.insert(value) {
            bail!("duplicate {field} in one mutation command");
        }
    }
    Ok(())
}

pub(super) fn verify_cas_result(
    command: &CasDelegationStateCommand,
    record: &DelegationRecord,
) -> Result<()> {
    validate_delegation(record)?;
    if record.delegation_id != command.delegation_id
        || record.state_revision != command.new_state_revision
        || record.phase != command.phase
        || record.run_control != command.run_control
        || record.pending_decision_id != command.pending_decision_id
        || record.external_wait_json != command.external_wait_json
        || record.updated_at != command.updated_at
        || record.terminal_at != command.terminal_at
    {
        bail!("delegation CAS verification did not match requested fields");
    }
    Ok(())
}
