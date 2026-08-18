#!/usr/bin/env python3
"""Independently validate the locked ExecAss v1.1 machine contract."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTRACT = ROOT / "contracts" / "execass" / "v1" / "execass_contract.json"

PHASES = ["accepted", "planning", "in_motion", "waiting_for_user", "waiting_external", "recovering", "completed", "partially_completed", "failed"]
TERMINAL_PHASES = ["completed", "partially_completed", "failed"]
BRANCH_STATES = ["runnable", "executing", "waiting", "uncertain", "terminal"]
PRECEDENCE = [
    (1, "valid_completion_assessment", ["completed", "partially_completed", "failed"]),
    (2, "before_actionable_planning", ["accepted", "planning"]),
    (3, "authorized_ordinary_branch_runnable_or_executing", ["in_motion"]),
    (4, "bounded_recovery_runnable_or_executing", ["recovering"]),
    (5, "actionable_human_attention", ["waiting_for_user"]),
    (6, "external_party_system_or_time_dependency", ["waiting_external"]),
    (7, "no_autonomous_human_or_external_path", ["completion_assessor_required"]),
]
OWNERSHIP = {
    "assistant_desk_summary": "replaced_by_execass_summary_projection_no_second_product_truth",
    "approvals": "replaced_by_decision_records_no_legacy_dual_read",
    "jobs_and_scheduler": "reused_as_durable_scheduler_execution_substrate_no_second_scheduler",
    "sessions_and_runs": "reused_as_authoritative_execution_records_linked_beneath_delegations",
    "tasks_boards_agent_mail_teams_artifacts": "reused_and_referenced_delegation_does_not_duplicate",
    "security_audit_tool_call_audit_job_run_evidence": "remain_authoritative_ledgers_receipts_link_and_attest_without_copying",
    "gateway": "evolves_to_single_carsinos_runtime_host_no_wrapper_or_sibling_host",
    "websocket_transport": "reused_with_durable_outbox_sequence_not_in_memory_event_identity",
    "telegram_and_discord": "shared_intake_and_decision_services_when_activated_and_tested",
    "legacy_gui_and_one_click_launchers": "attach_control_or_development_fenced_no_independent_production_mutation",
}
EVENTS = {
    "delegation_transitions": "execass.v1.delegation.transitioned",
    "decisions": "execass.v1.decision.recorded",
    "continuation_claims_results": "execass.v1.continuation.claimed_or_result_recorded",
    "recovery": "execass.v1.recovery.updated",
    "completion": "execass.v1.completion.assessed",
    "summary_changes": "execass.v1.summary.changed",
    "policy_changes": "execass.v1.policy.changed",
    "runtime_host_state": "execass.v1.runtime_host.changed",
    "receipt_integrity_failure": "execass.v1.receipt.integrity_failed",
    "notification_scheduling": "execass.v1.notification.scheduled",
}
EVENT_ENVELOPE = ["event_name", "aggregate_id", "revision", "correlation_id", "causation_id", "occurred_at", "schema_version", "safe_payload", "global_sequence", "duplicate_identity"]
ERROR_SHAPE = ["code", "safe_human_message", "retryable", "correlation_id", "safe_for_display", "exposes_sensitive_metadata"]
ERROR_CODES = {
    "execass.v1.invalid_request", "execass.v1.authentication_required", "execass.v1.idempotency_conflict",
    "execass.v1.authority_denied", "execass.v1.decision_assurance_required", "execass.v1.decision_challenge_expired",
    "execass.v1.decision_superseded", "execass.v1.not_found", "execass.v1.revision_conflict",
    "execass.v1.invalid_transition", "execass.v1.stop_all_engaged", "execass.v1.outcome_unknown_retry_prohibited",
    "execass.v1.technical_resource_exhausted", "execass.v1.receipt_integrity_quarantined",
    "execass.v1.runtime_host_conflict", "execass.v1.schema_replace_requires_quiescence",
    "execass.v1.rate_limited", "execass.v1.external_dependency", "execass.v1.schema_version_unsupported",
    "execass.v1.internal_safe_failure",
}
DECISION_KINDS = ["clarification", "dangerous_action_confirmation", "owner_configured_checkpoint", "recovery_choice", "duplicate_risk_retry", "stop", "policy_change"]
DECISION_RESULTS = ["confirm_and_continue", "revise", "decline", "stop"]
DANGER_CATEGORIES = [
    "whole_drive_volume_boot_recovery_or_core_os_tree_erasure_or_unusable",
    "whole_user_profile_or_home_erasure_or_unusable",
    "complete_carsinos_state_integrity_runtime_enforcement_stop_fencing_or_recovery_configuration_erasure_or_unusable",
    "whole_connected_external_account_closure_or_erasure",
    "last_verified_administrative_recovery_or_decryption_path_destruction",
]


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON object key: {key}")
        result[key] = value
    return result


def load_contract(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as source:
        value = json.load(source, object_pairs_hook=strict_object)
    if not isinstance(value, dict):
        raise ValueError("contract root must be an object")
    return value


def validate_contract(contract: dict[str, Any]) -> list[str]:
    """Return all independent invariant violations without normalizing input."""
    errors: list[str] = []

    def require_keys(value: Any, required: set[str], path: str) -> dict[str, Any] | None:
        if not isinstance(value, dict):
            errors.append(f"{path}: must be an object")
            return None
        actual = set(value)
        for key in sorted(required - actual):
            errors.append(f"{path}: missing required key {key}")
        for key in sorted(actual - required):
            errors.append(f"{path}: unknown key {key}")
        return value

    def exact_list(value: Any, expected: list[Any], path: str) -> None:
        if not isinstance(value, list):
            errors.append(f"{path}: must be an array")
            return
        encoded = [json.dumps(item, sort_keys=True, separators=(",", ":")) for item in value]
        if len(encoded) != len(set(encoded)):
            errors.append(f"{path}: duplicate values are forbidden")
        elif value != expected:
            errors.append(f"{path}: does not match the locked ordered table")

    root = require_keys(contract, {
        "schema_metadata", "lifecycle", "run_control", "attention", "owner_authority",
        "danger_confirmation", "decisions", "technical_resources", "recovery", "ownership", "events", "api_errors",
    }, "root")
    if root is None:
        return errors

    metadata = require_keys(root.get("schema_metadata"), {"contract_id", "contract_version", "schema_version", "locked_spec_version", "api_prefix", "additional_properties"}, "schema_metadata")
    if metadata != {
        "contract_id": "carsinos.execass.contract", "contract_version": "v1", "schema_version": "1.1.0",
        "locked_spec_version": "1.1", "api_prefix": "/api/v1/execass", "additional_properties": False,
    }:
        errors.append("schema_metadata: must match the locked v1.1 metadata")

    lifecycle = require_keys(root.get("lifecycle"), {"phases", "terminal_phases", "branch_states", "precedence"}, "lifecycle")
    if lifecycle:
        exact_list(lifecycle.get("phases"), PHASES, "lifecycle.phases")
        exact_list(lifecycle.get("terminal_phases"), TERMINAL_PHASES, "lifecycle.terminal_phases")
        exact_list(lifecycle.get("branch_states"), BRANCH_STATES, "lifecycle.branch_states")
        exact_list(lifecycle.get("precedence"), [{"rank": rank, "condition": condition, "selects": selects} for rank, condition, selects in PRECEDENCE], "lifecycle.precedence")

    run_control = require_keys(root.get("run_control"), {"states", "stopped_is_orthogonal", "stop_requested_is_draining", "stopped_preconditions", "resume_requires"}, "run_control")
    if run_control:
        exact_list(run_control.get("states"), ["running", "stop_requested", "stopped"], "run_control.states")
        exact_list(run_control.get("stopped_preconditions"), ["new_claims_blocked", "active_claims_fenced", "safe_boundary_work_ended_or_unresolved_external_effect_recorded"], "run_control.stopped_preconditions")
        exact_list(run_control.get("resume_requires"), ["fresh_plan_snapshot", "fresh_policy_snapshot"], "run_control.resume_requires")
        if run_control.get("stopped_is_orthogonal") is not True or run_control.get("stop_requested_is_draining") is not True:
            errors.append("run_control: stopped must remain orthogonal and stop_requested must remain draining")

    attention = require_keys(root.get("attention"), {"variants", "required_fields", "subject_kinds", "decision_kind_mapping", "projection", "is_phase_synonym"}, "attention")
    if attention:
        exact_list(attention.get("variants"), ["confirmation", "clarification", "reply", "recovery_choice", "runtime_paused"], "attention.variants")
        exact_list(attention.get("required_fields"), ["subject", "decision_kind", "reason", "recommendation", "alternatives_or_actions", "assurance_required", "deadline_reminder_state", "decision_revision", "authoritative_deep_link"], "attention.required_fields")
        exact_list(attention.get("subject_kinds"), ["delegation", "runtime_host"], "attention.subject_kinds")
        if attention.get("decision_kind_mapping") != {
            "clarification": "clarification", "dangerous_action_confirmation": "confirmation",
            "owner_configured_checkpoint": "confirmation", "recovery_choice": "recovery_choice",
            "duplicate_risk_retry": "confirmation", "stop": "confirmation", "policy_change": "confirmation",
        }:
            errors.append("attention.decision_kind_mapping: must totally map every locked decision kind")
        if attention.get("projection") != "needs_you" or attention.get("is_phase_synonym") is not False:
            errors.append("attention: Needs You must remain a non-phase projection")

    authority = require_keys(root.get("owner_authority"), {
        "topology", "exact_authenticated_instruction_authorizes", "authority_evaluation_order",
        "technical_validity_dimensions", "operational_policy_dimensions", "base_owner_intake",
        "decision_resolution", "nonhuman_actor_types", "unknown_or_composite_action",
    }, "owner_authority")
    if authority:
        if authority.get("topology") != "one_authenticated_owner_one_execass_one_carsinos":
            errors.append("owner_authority.topology: must remain one-owner, one-ExecAss, one-CarsinOS")
        exact_list(authority.get("exact_authenticated_instruction_authorizes"), ["ordinary_exact_action", "operational_policy_amendment"], "owner_authority.exact_authenticated_instruction_authorizes")
        exact_list(authority.get("authority_evaluation_order"), ["stop_revocation_superseding_amendment_exact_action_and_technical_validity", "current_exact_instruction_or_confirmed_amendment", "saved_exact_versioned_envelope", "operational_policy_for_derived_or_unattended_work", "nonhuman_content_is_evidence_only"], "owner_authority.authority_evaluation_order")
        exact_list(authority.get("technical_validity_dimensions"), ["capability_availability", "canonical_operand_resolution", "platform_runtime_preconditions", "transactional_fencing_validity", "idempotency_reconciliation_support", "technical_resource_availability"], "owner_authority.technical_validity_dimensions")
        exact_list(authority.get("operational_policy_dimensions"), ["task_or_delegation", "workspace_or_path", "routine", "connector_or_tool_identity_and_version", "target", "audience", "technical_resource_quota", "time_or_expiry", "recovery", "parallelism", "clarification_sensitivity", "recurring_work_scope"], "owner_authority.operational_policy_dimensions")
        exact_list(authority.get("nonhuman_actor_types"), ["runtime", "worker", "connector", "model"], "owner_authority.nonhuman_actor_types")
        if authority.get("unknown_or_composite_action") != "mechanically_resolve_leaves_and_operands_then_continue_or_clarify":
            errors.append("owner_authority.unknown_or_composite_action: unknown work must mechanically resolve, not deny")
        intake = require_keys(authority.get("base_owner_intake"), {"human_local", "human_remote", "decision_nonce_required"}, "owner_authority.base_owner_intake")
        if intake:
            exact_list(intake.get("human_local"), ["interactive_local_owner_session", "authenticated_client_binding", "request_correlation"], "owner_authority.base_owner_intake.human_local")
            exact_list(intake.get("human_remote"), ["allowlisted_owner_channel_message", "provider_account_binding", "source_message", "request_correlation"], "owner_authority.base_owner_intake.human_remote")
            if intake.get("decision_nonce_required") is not False:
                errors.append("owner_authority.base_owner_intake: fresh intake cannot require a decision nonce")
        resolution = require_keys(authority.get("decision_resolution"), {"human_local", "human_remote", "nonhuman_may_resolve"}, "owner_authority.decision_resolution")
        if resolution:
            exact_list(resolution.get("human_local"), ["interactive_local_owner_session", "authenticated_client_binding", "current_decision_revision", "exact_presented_action_or_alternative", "unexpired_challenge_nonce"], "owner_authority.decision_resolution.human_local")
            exact_list(resolution.get("human_remote"), ["allowlisted_owner_channel_action", "provider_account_binding", "source_message", "current_decision_revision", "exact_presented_action_or_alternative", "unexpired_challenge_token"], "owner_authority.decision_resolution.human_remote")
            if resolution.get("nonhuman_may_resolve") is not False:
                errors.append("owner_authority.decision_resolution: non-human actors cannot resolve owner decisions")

    danger = require_keys(root.get("danger_confirmation"), {"known_danger_categories", "mandatory_match_inputs", "model_may_add_one_credible_danger_confirmation", "model_may_veto", "model_may_repeat_unchanged_action_confirmation", "challenge", "accepted_grant"}, "danger_confirmation")
    if danger:
        exact_list(danger.get("known_danger_categories"), DANGER_CATEGORIES, "danger_confirmation.known_danger_categories")
        exact_list(danger.get("mandatory_match_inputs"), ["canonical_resolved_operands", "verified_system_metadata"], "danger_confirmation.mandatory_match_inputs")
        if danger.get("model_may_add_one_credible_danger_confirmation") is not True or danger.get("model_may_veto") is not False or danger.get("model_may_repeat_unchanged_action_confirmation") is not False:
            errors.append("danger_confirmation: model may add one confirmation but cannot veto or repeat")
        challenge = require_keys(danger.get("challenge"), {"fields", "expires", "single_resolution"}, "danger_confirmation.challenge")
        if challenge:
            exact_list(challenge.get("fields"), ["decision_revision", "exact_presented_action_or_alternative", "declared_consequence", "nonce_or_token", "expires_at"], "danger_confirmation.challenge.fields")
            if challenge.get("expires") is not True or challenge.get("single_resolution") is not True:
                errors.append("danger_confirmation.challenge: must be expiring and single-resolution")
        grant = require_keys(danger.get("accepted_grant"), {"fields", "expires", "has_use_counter", "carries_across", "unrelated_decision_results_preserve_grant", "invalidated_only_by"}, "danger_confirmation.accepted_grant")
        if grant:
            exact_list(grant.get("fields"), ["delegation", "normalized_intent", "confirmed_logical_action_identity", "canonical_action_envelope_or_selector", "payload_and_material_operands", "connector_or_tool_identity_and_version", "declared_consequence"], "danger_confirmation.accepted_grant.fields")
            if grant.get("expires") is not False or grant.get("has_use_counter") is not False:
                errors.append("danger_confirmation.accepted_grant: durable grant cannot expire or have a use counter")
            exact_list(grant.get("carries_across"), ["unchanged_plan", "unchanged_policy", "technical_resource_revalidation", "host_generation_change", "restart", "bounded_retry", "routine_occurrence_with_expected_membership_changes_within_same_selector_envelope"], "danger_confirmation.accepted_grant.carries_across")
            if grant.get("unrelated_decision_results_preserve_grant") is not True:
                errors.append("danger_confirmation.accepted_grant: unrelated decision results must preserve the grant")
            exact_list(grant.get("invalidated_only_by"), ["material_target_drift", "material_scope_drift", "material_payload_drift", "material_tool_drift", "material_consequence_drift", "explicit_action_specific_owner_amendment", "explicit_action_specific_owner_revocation", "explicit_action_specific_owner_cancellation"], "danger_confirmation.accepted_grant.invalidated_only_by")

    decisions = require_keys(root.get("decisions"), {"kinds", "results", "ordinary_exact_action_requires_decision", "operational_policy_amendment_route", "policy_amendment_uses", "policy_amendment_has_parallel_confirmation_authority", "duplicate_risk_retry"}, "decisions")
    if decisions:
        exact_list(decisions.get("kinds"), DECISION_KINDS, "decisions.kinds")
        exact_list(decisions.get("results"), DECISION_RESULTS, "decisions.results")
        if decisions.get("ordinary_exact_action_requires_decision") is not False:
            errors.append("decisions: ordinary exact owner action must have positive no-decision authority")
        if decisions.get("operational_policy_amendment_route") != "PUT /api/v1/execass/policy" or decisions.get("policy_amendment_uses") != "canonical_owner_intake_revision_transaction" or decisions.get("policy_amendment_has_parallel_confirmation_authority") is not False:
            errors.append("decisions: policy amendments must use only the canonical owner-intake route")
        retry = require_keys(decisions.get("duplicate_risk_retry"), {"required_kind", "required_result", "creates"}, "decisions.duplicate_risk_retry")
        if retry != {"required_kind": "duplicate_risk_retry", "required_result": "confirm_and_continue", "creates": "new_logical_effect_id"}:
            errors.append("decisions.duplicate_risk_retry: must create a new logical effect after typed confirmation")

    resources = require_keys(root.get("technical_resources"), {"dimensions"}, "technical_resources")
    if resources:
        exact_list(resources.get("dimensions"), ["tokens", "time_ms", "connector_calls", "resource_units"], "technical_resources.dimensions")

    recovery = require_keys(root.get("recovery"), {"allowed_retry_safety_facts", "outcome_unknown_resolution", "suppression_by_owner_intent_dimension"}, "recovery")
    if recovery:
        exact_list(recovery.get("allowed_retry_safety_facts"), ["attempt_count", "elapsed_time", "backoff", "technical_resource_quota", "circuit_breakers", "provider_error_class", "idempotency", "independent_absence_or_reconciliation_proof", "reversibility", "declared_safe_boundary"], "recovery.allowed_retry_safety_facts")
        if recovery.get("outcome_unknown_resolution") != "reconcile_or_wait_or_typed_duplicate_risk_retry" or recovery.get("suppression_by_owner_intent_dimension") is not False:
            errors.append("recovery: retry safety must remain objective and use the typed duplicate-risk path")

    ownership = root.get("ownership")
    if not isinstance(ownership, list):
        errors.append("ownership: must be an array")
    else:
        values = {item.get("concern"): item.get("disposition") for item in ownership if isinstance(item, dict)}
        if len(ownership) != len(values) or values != OWNERSHIP:
            errors.append("ownership: missing, duplicated, or changed ownership disposition")
        for item in ownership:
            if isinstance(item, dict):
                require_keys(item, {"concern", "disposition"}, f"ownership[{item.get('concern')!r}]")

    events = require_keys(root.get("events"), {"envelope_required_fields", "families"}, "events")
    if events:
        exact_list(events.get("envelope_required_fields"), EVENT_ENVELOPE, "events.envelope_required_fields")
        families = events.get("families")
        if not isinstance(families, list):
            errors.append("events.families: must be an array")
        else:
            values = {item.get("id"): item.get("event_name") for item in families if isinstance(item, dict)}
            if len(families) != len(values) or values != EVENTS:
                errors.append("events.families: missing, unknown, duplicate, or changed family")
            for item in families:
                if isinstance(item, dict):
                    require_keys(item, {"id", "event_name"}, f"events.families[{item.get('id')!r}]")

    api_errors = require_keys(root.get("api_errors"), {"shape_required_fields", "forbidden_metadata_fields", "catalog"}, "api_errors")
    if api_errors:
        exact_list(api_errors.get("shape_required_fields"), ERROR_SHAPE, "api_errors.shape_required_fields")
        exact_list(api_errors.get("forbidden_metadata_fields"), ["secret", "credential", "token", "authorization", "raw_payload", "stack_trace", "internal_path"], "api_errors.forbidden_metadata_fields")
        catalog = api_errors.get("catalog")
        if not isinstance(catalog, list):
            errors.append("api_errors.catalog: must be an array")
        else:
            codes = [entry.get("code") for entry in catalog if isinstance(entry, dict)]
            if len(catalog) != len(codes) or len(codes) != len(set(codes)) or set(codes) != ERROR_CODES:
                errors.append("api_errors.catalog: missing, unknown, or duplicate codes")
            for entry in catalog:
                if not isinstance(entry, dict):
                    errors.append("api_errors.catalog: every entry must be an object")
                    continue
                require_keys(entry, set(ERROR_SHAPE), f"api_errors.catalog[{entry.get('code')!r}]")
                if not isinstance(entry.get("safe_human_message"), str) or not entry.get("safe_human_message"):
                    errors.append(f"api_errors.catalog[{entry.get('code')!r}]: safe_human_message must be non-empty")
                if not isinstance(entry.get("retryable"), bool) or entry.get("correlation_id") != "request_correlation_id":
                    errors.append(f"api_errors.catalog[{entry.get('code')!r}]: retryability or correlation metadata is invalid")
                if entry.get("safe_for_display") is not True or entry.get("exposes_sensitive_metadata") is not False:
                    errors.append(f"api_errors.catalog[{entry.get('code')!r}]: unsafe error metadata is forbidden")

    # A structural scan catches renamed reintroductions in a table that might otherwise be ignored.
    serialized = json.dumps(contract, sort_keys=True).lower()
    if re.search(r"\bpay(?:ment|ee)\w*|\bcurrenc\w*|\bbalance\w*|\bpurchas\w*|\bfinancial\w*|\bmonetary\w*|\bmoney\w*", serialized):
        errors.append("contract: prohibited financial action, field, or resource dimension")
    if re.search(r"tenant|organization|\"role\"|second_owner|cross_user", serialized):
        errors.append("contract: prohibited multi-principal authority path")
    if re.search(r"generic_approval|fresh_approval|hard_lock|absolute_refusal", serialized):
        errors.append("contract: prohibited v1.0 approval or absolute-refusal semantics")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("contract", nargs="?", type=Path, default=DEFAULT_CONTRACT)
    args = parser.parse_args(argv)
    try:
        errors = validate_contract(load_contract(args.contract))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"ExecAss contract validation failed: {error}", file=sys.stderr)
        return 1
    if errors:
        print("ExecAss contract validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"ExecAss contract valid: {args.contract}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
