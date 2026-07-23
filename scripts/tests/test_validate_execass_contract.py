"""Mutation tests for the locked ExecAss v1.1 machine contract."""

from __future__ import annotations

import copy
import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_execass_contract import DEFAULT_CONTRACT, load_contract, strict_object, validate_contract


class ExecAssContractValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = load_contract(DEFAULT_CONTRACT)

    def assert_rejected(self, contract: dict, expected_fragment: str) -> None:
        errors = validate_contract(contract)
        self.assertTrue(errors, "mutated contract was unexpectedly accepted")
        self.assertTrue(any(expected_fragment in error for error in errors), errors)

    def test_locked_contract_validates(self) -> None:
        self.assertEqual(validate_contract(self.contract), [])

    def test_duplicate_json_keys_are_rejected_during_load(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate JSON object key"):
            json.loads('{"a": 1, "a": 2}', object_pairs_hook=strict_object)

    def test_lifecycle_and_run_control_drift_are_rejected(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["lifecycle"]["precedence"][2]["rank"] = 4
        self.assert_rejected(changed, "lifecycle.precedence")

        changed = copy.deepcopy(self.contract)
        changed["run_control"]["states"][2] = "running"
        self.assert_rejected(changed, "duplicate values")

    def test_reintroduced_financial_action_or_resource_is_rejected(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["technical_resources"]["dimensions"].append("currency")
        self.assert_rejected(changed, "technical_resources.dimensions")
        self.assert_rejected(changed, "prohibited financial")

    def test_generic_approval_kind_or_result_is_rejected(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["decisions"]["kinds"][0] = "generic_approval"
        self.assert_rejected(changed, "decisions.kinds")
        self.assert_rejected(changed, "prohibited v1.0")

        changed = copy.deepcopy(self.contract)
        changed["decisions"]["results"][0] = "approve"
        self.assert_rejected(changed, "decisions.results")

    def test_category_approval_floor_and_destructive_refusal_are_rejected(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["owner_authority"]["category_approval_floor"] = "required"
        self.assert_rejected(changed, "owner_authority: unknown key")

        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["absolute_refusal"] = True
        self.assert_rejected(changed, "danger_confirmation: unknown key")
        self.assert_rejected(changed, "prohibited v1.0")

    def test_accepted_grant_cannot_expire_or_be_consumed(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["accepted_grant"]["expires"] = True
        self.assert_rejected(changed, "durable grant cannot expire")

        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["accepted_grant"]["has_use_counter"] = True
        self.assert_rejected(changed, "durable grant cannot expire")

    def test_unchanged_plan_policy_restart_and_routine_carry_forward_is_required(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["accepted_grant"]["carries_across"].remove("unchanged_policy")
        self.assert_rejected(changed, "accepted_grant.carries_across")

        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["accepted_grant"]["carries_across"].remove("restart")
        self.assert_rejected(changed, "accepted_grant.carries_across")

    def test_unrelated_decision_results_preserve_accepted_grant(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["accepted_grant"]["unrelated_decision_results_preserve_grant"] = False
        self.assert_rejected(changed, "unrelated decision results must preserve the grant")

    def test_local_and_authenticated_remote_owner_may_resolve(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["owner_authority"]["decision_resolution"]["human_remote"] = []
        self.assert_rejected(changed, "decision_resolution.human_remote")

    def test_fresh_intake_does_not_require_a_decision_nonce(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["owner_authority"]["base_owner_intake"]["decision_nonce_required"] = True
        self.assert_rejected(changed, "fresh intake cannot require a decision nonce")

    def test_policy_amendment_has_only_canonical_route(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["decisions"]["policy_amendment_uses"] = "policy_specific_confirm_handler"
        self.assert_rejected(changed, "policy amendments must use only")

        changed = copy.deepcopy(self.contract)
        changed["decisions"]["policy_amendment_has_parallel_confirmation_authority"] = True
        self.assert_rejected(changed, "policy amendments must use only")

    def test_known_danger_matcher_is_closed_and_model_cannot_veto(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["mandatory_match_inputs"].append("model_score")
        self.assert_rejected(changed, "mandatory_match_inputs")

        changed = copy.deepcopy(self.contract)
        changed["danger_confirmation"]["model_may_veto"] = True
        self.assert_rejected(changed, "model may add one confirmation")

    def test_recovery_cannot_use_purpose_or_model_score_suppression(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["recovery"]["allowed_retry_safety_facts"].append("purpose")
        self.assert_rejected(changed, "recovery.allowed_retry_safety_facts")

        changed = copy.deepcopy(self.contract)
        changed["recovery"]["allowed_retry_safety_facts"].append("model_score")
        self.assert_rejected(changed, "recovery.allowed_retry_safety_facts")

    def test_tenant_or_second_owner_path_is_rejected(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["owner_authority"]["topology"] = "tenant_with_second_owner"
        self.assert_rejected(changed, "one-owner")
        self.assert_rejected(changed, "multi-principal")

    def test_unknown_work_must_mechanically_resolve_not_deny(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["owner_authority"]["unknown_or_composite_action"] = "deny_unknown_work"
        self.assert_rejected(changed, "unknown work must mechanically resolve")

    def test_positive_ordinary_owner_authority_is_required(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["decisions"]["ordinary_exact_action_requires_decision"] = True
        self.assert_rejected(changed, "positive no-decision authority")

    def test_duplicate_risk_retry_requires_typed_confirmation_and_new_effect(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["decisions"]["duplicate_risk_retry"]["required_result"] = "revise"
        self.assert_rejected(changed, "duplicate_risk_retry")

        changed = copy.deepcopy(self.contract)
        changed["decisions"]["duplicate_risk_retry"]["creates"] = "same_logical_effect_id"
        self.assert_rejected(changed, "duplicate_risk_retry")

    def test_total_decision_attention_mapping_is_required(self) -> None:
        changed = copy.deepcopy(self.contract)
        del changed["attention"]["decision_kind_mapping"]["policy_change"]
        self.assert_rejected(changed, "totally map every locked decision kind")

    def test_ownership_event_and_safe_error_tables_remain_closed(self) -> None:
        changed = copy.deepcopy(self.contract)
        changed["ownership"].append(copy.deepcopy(changed["ownership"][0]))
        self.assert_rejected(changed, "ownership")

        changed = copy.deepcopy(self.contract)
        changed["events"]["envelope_required_fields"].remove("causation_id")
        self.assert_rejected(changed, "events.envelope_required_fields")

        changed = copy.deepcopy(self.contract)
        changed["api_errors"]["catalog"][0]["exposes_sensitive_metadata"] = True
        self.assert_rejected(changed, "unsafe error metadata")


if __name__ == "__main__":
    unittest.main()
