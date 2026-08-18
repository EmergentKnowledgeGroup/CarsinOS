//! Production-off fixtures used only by the gateway confirmation integration
//! tests. This module is absent unless the explicit test feature is enabled.

use super::store::{immediate_transaction, ExecAssStore};
use super::types::{
    DangerConfirmationAdmissionOutcome, DangerConfirmationRuntimeProjection,
    PendingDangerConfirmationAlternativeBinding, PresentDangerousActionConfirmationCommand,
};
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{
    issue_test_local_owner_authority, TestLocalOwnerAuthorityInput,
};
use carsinos_core::execass_danger::{
    issue_test_verified_danger_metadata, match_known_danger, KnownDangerMatchInput,
    TestVerifiedDangerFact,
};
use carsinos_core::execass_manifest::{
    canonicalize_owner_authority, compile_dispatch, CanonicalField, CanonicalValue, DispatchAction,
    DispatchNode, DispatchTree, ManifestCompilation, ResolvedLeafInput, ServerResolutionRegistry,
    TargetSnapshotInput, ToolIdentityInput,
};
use rusqlite::params;

impl ExecAssStore {
    /// Build the installed local-fs exact-overwrite confirmation through the
    /// production manifest compiler, danger matcher, and confirmation writer.
    /// Raw setup is feature-gated and limited to the pre-decision foundation;
    /// the dangerous decision/challenge itself has no fixture bypass.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_test_exact_overwrite_confirmation_runtime_projection(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        target_path: &str,
        target_identity: &str,
        expected_preimage_sha256: &str,
        replacement_hex: &str,
        replacement_sha256: &str,
        requested_at: i64,
        expires_at: i64,
    ) -> Result<PendingDangerConfirmationAlternativeBinding> {
        for (label, value) in [
            ("decision_id", decision_id),
            ("selected_logical_action_id", selected_logical_action_id),
            ("target_path", target_path),
            ("target_identity", target_identity),
            ("expected_preimage_sha256", expected_preimage_sha256),
            ("replacement_hex", replacement_hex),
            ("replacement_sha256", replacement_sha256),
        ] {
            if value.trim().is_empty() {
                bail!("exact-overwrite test fixture requires {label}");
            }
        }
        if requested_at <= 0 || expires_at <= requested_at {
            bail!("exact-overwrite test fixture has invalid times");
        }
        let normalized_intent = format!("replace exactly {target_path}");
        let authority = issue_test_local_owner_authority(TestLocalOwnerAuthorityInput {
            authenticated_client_id: "exact-overwrite-test-owner".into(),
            authenticated_ingress: "native-control".into(),
            channel_assurance: "interactive-local".into(),
            request_correlation_id: format!("exact-overwrite-correlation-{decision_id}"),
            source_message_id: None,
            normalized_intent: normalized_intent.clone(),
            instruction_revision: "exact-overwrite-instruction-v1".into(),
            instruction_bytes: normalized_intent.as_bytes().to_vec(),
            owner_envelope_revision: "exact-overwrite-envelope-v1".into(),
            owner_envelope_json: r#"{"scope":"exact-overwrite-test"}"#.into(),
            authority_kind: "original_request".into(),
            normalized_scope_json: serde_json::json!({"target_path": target_path}).to_string(),
            policy_revision: 1,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: requested_at,
            expires_at: None,
        })
        .map_err(|error| {
            anyhow::anyhow!("invalid exact-overwrite test owner authority: {error:?}")
        })?;
        let dispatch = DispatchTree {
            root_id: "root".into(),
            nodes: vec![DispatchNode {
                node_id: "root".into(),
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id: selected_logical_action_id.into(),
                    action_kind: "resolved_destroy".into(),
                    tool: ToolIdentityInput {
                        tool_id: "carsinos.local-fs".into(),
                        version: "exact-overwrite.v1".into(),
                    },
                    operands: CanonicalValue::Object(vec![
                        CanonicalField {
                            key: "contract_version".into(),
                            value: CanonicalValue::String(
                                "carsinos.local-fs.exact-overwrite.operand.v1".into(),
                            ),
                        },
                        CanonicalField {
                            key: "target_path".into(),
                            value: CanonicalValue::String(target_path.into()),
                        },
                        CanonicalField {
                            key: "target_identity".into(),
                            value: CanonicalValue::String(target_identity.into()),
                        },
                        CanonicalField {
                            key: "expected_preimage_sha256".into(),
                            value: CanonicalValue::String(expected_preimage_sha256.into()),
                        },
                        CanonicalField {
                            key: "replacement_hex".into(),
                            value: CanonicalValue::String(replacement_hex.into()),
                        },
                        CanonicalField {
                            key: "replacement_sha256".into(),
                            value: CanonicalValue::String(replacement_sha256.into()),
                        },
                    ]),
                    target_snapshot: TargetSnapshotInput {
                        targets: vec![
                            CanonicalValue::String(target_path.into()),
                            CanonicalValue::String(target_identity.into()),
                        ],
                    },
                    material_digest: replacement_sha256
                        .strip_prefix("sha256:")
                        .map(str::to_owned),
                    owner_authority: authority.clone(),
                })),
            }],
        };
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&dispatch, &ServerResolutionRegistry::default())
        else {
            bail!("exact-overwrite test manifest did not compile");
        };
        let leaf = &manifest.leaves()[0];
        let metadata = issue_test_verified_danger_metadata(
            leaf,
            &[(
                TestVerifiedDangerFact::LastAdministrativeRecoveryOrDecryptionPath,
                target_path.to_string(),
            )],
        );
        let danger_route = match_known_danger(KnownDangerMatchInput {
            canonical_leaf: leaf,
            verified_metadata: &metadata,
        })
        .map_err(|_| anyhow::anyhow!("exact-overwrite test danger route failed"))?;
        let canonical_authority = canonicalize_owner_authority(&authority).map_err(|detail| {
            anyhow::anyhow!("invalid exact-overwrite test authority: {detail}")
        })?;
        let authority_record =
            super::foundation::authority_record_from_manifest(&canonical_authority)?;
        let manifest_json = std::str::from_utf8(manifest.canonical().bytes())?;
        let manifest_digest = manifest.canonical().digest().as_hex();
        let delegation_id = format!("exact-overwrite-delegation-{decision_id}");
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        super::rows::insert_authority(&tx, &authority_record)?;
        tx.execute(
            r#"INSERT INTO execass_delegations(
                 delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,
                 ingress_credential_identity,source_correlation_id,ingress_idempotency_key,
                 classifier_version,classifier_reasons_json,phase,run_control,state_revision,
                 policy_revision,effective_authority_json,authority_provenance_id,current_plan_revision,
                 created_at,updated_at
               ) VALUES(?1,?2,'{}','native-control','exact-overwrite-test-owner',?3,?4,
                 'exact-overwrite-fixture-v1','["durable_work"]','accepted','running',1,1,
                 '{}',?5,NULL,?6,?6)"#,
            params![
                delegation_id,
                normalized_intent,
                format!("exact-overwrite-correlation-{decision_id}"),
                format!("exact-overwrite-intake-{decision_id}"),
                authority_record.authority_provenance_id,
                requested_at,
            ],
        )?;
        tx.execute(
            r#"INSERT INTO execass_plans(
                 plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,
                 plan_summary,resolved_leaf_manifest_json,manifest_digest,
                 created_by_authority_provenance_id,created_at
               ) VALUES(?1,?2,1,1,1,'exact local file overwrite',?3,?4,?5,?6)"#,
            params![
                format!("exact-overwrite-plan-{decision_id}"),
                delegation_id,
                manifest_json,
                manifest_digest,
                authority_record.authority_provenance_id,
                requested_at,
            ],
        )?;
        tx.execute(
            r#"INSERT INTO execass_criteria_sets(
                 criteria_set_id,delegation_id,criteria_revision,parent_criteria_revision,
                 disposition,created_at
               ) VALUES(?1,?2,1,NULL,'genesis',?3)"#,
            params![
                format!("exact-overwrite-criteria-{decision_id}"),
                delegation_id,
                requested_at,
            ],
        )?;
        tx.execute(
            r#"INSERT INTO execass_outcome_criteria(
                 criterion_id,delegation_id,criteria_revision,criterion_key,description,
                 material,verifier_type,expected_predicate_json,authoritative_source_kind,created_at
               ) VALUES(?1,?2,1,'exact-result','the exact replacement is independently verified',
                 1,'authoritative_state','{}','exact-overwrite-test-fixture',?3)"#,
            params![
                format!("exact-overwrite-criterion-{decision_id}"),
                delegation_id,
                requested_at,
            ],
        )?;
        tx.execute(
            "UPDATE execass_delegations SET current_plan_revision=1,current_criteria_revision=1,state_revision=2,updated_at=updated_at+1 WHERE delegation_id=?1 AND state_revision=1",
            params![delegation_id],
        )?;
        tx.execute(
            r#"INSERT INTO execass_action_branches(
                 action_id,delegation_id,action_revision,target_delegation_revision,
                 target_plan_revision,stop_epoch,branch_kind,status,action_summary,
                 created_at,updated_at,terminal_at
               ) VALUES(?1,?2,1,2,1,0,'ordinary','waiting','exact local file overwrite',?3,?3,NULL)"#,
            params![selected_logical_action_id, delegation_id, requested_at],
        )?;
        tx.commit()?;
        let command = PresentDangerousActionConfirmationCommand {
            delegation_id: delegation_id.clone(),
            logical_action_id: selected_logical_action_id.into(),
            decision_id: decision_id.into(),
            challenge_id: format!("exact-overwrite-challenge-{decision_id}"),
            idempotency_key: format!("exact-overwrite-decision-{decision_id}"),
            challenge_nonce: format!("exact-overwrite-nonce-{decision_id}").into_bytes(),
            requested_at,
            expires_at,
        };
        match self.ensure_dangerous_action_confirmation(&command, &manifest, &danger_route)? {
            DangerConfirmationAdmissionOutcome::Presented(_)
            | DangerConfirmationAdmissionOutcome::ExistingPending(_) => {}
            DangerConfirmationAdmissionOutcome::AlreadyConfirmed(_) => {
                bail!("fresh exact-overwrite fixture unexpectedly reused a grant")
            }
        }
        match self
            .read_danger_confirmation_runtime_projection(decision_id, selected_logical_action_id)?
        {
            Some(DangerConfirmationRuntimeProjection::Pending(binding)) => Ok(*binding),
            _ => bail!("exact-overwrite fixture did not project a real pending confirmation"),
        }
    }

    /// Create the storage-owned recovery branch and duplicate-risk question
    /// that follow one uncertain predecessor. The gateway supplies identities
    /// only; immutable effect material is copied inside storage.
    #[doc(hidden)]
    pub fn prepare_test_duplicate_risk_decision(
        &self,
        decision_id: &str,
        successor_action_id: &str,
        predecessor_action_id: &str,
        predecessor_effect_id: &str,
        idempotency_key: &str,
        requested_at: i64,
    ) -> Result<()> {
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let (delegation_id, delegation_revision, plan_revision, policy_revision):
            (String, i64, i64, i64) = tx.query_row(
                "SELECT delegation_id,state_revision,current_plan_revision,policy_revision FROM execass_delegations WHERE delegation_id='test-delegation'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let (manifest_digest, payload_digest): (String, String) = tx.query_row(
            "SELECT manifest_digest,payload_digest FROM execass_logical_effects WHERE logical_effect_id=?1 AND state='outcome_unknown'",
            [predecessor_effect_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let decision_delegation_revision = delegation_revision + 1;
        tx.execute(
            r#"INSERT INTO execass_action_branches(
                 action_id,delegation_id,action_revision,target_delegation_revision,
                 target_plan_revision,stop_epoch,branch_kind,status,action_summary,
                 created_at,updated_at,terminal_at
               ) VALUES(?1,?2,2,?3,?4,0,'recovery','waiting',
                 'retry uncertain effect with a fresh identity',?5,?5,NULL)"#,
            params![
                successor_action_id,
                delegation_id,
                decision_delegation_revision,
                plan_revision,
                requested_at,
            ],
        )?;
        tx.execute(
            r#"INSERT INTO execass_decisions(
                 decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,
                 policy_revision,decision_kind,status,exact_presented_action_json,
                 confirmed_logical_action_identity,manifest_digest,payload_digest,
                 payload_and_material_operands_json,target_audience_path_json,
                 connector_tool_identity,connector_tool_version,side_effect_envelope_json,
                 recommendation,consequence,alternatives_json,idempotency_key,requested_at
               ) VALUES(?1,?2,2,?3,?4,?5,'duplicate_risk_retry','pending',?6,?7,?8,?9,
                 '{}','[]',NULL,NULL,'{}','retry only after owner confirmation',
                 'the uncertain external effect may be duplicated',
                 '["confirm_and_continue","revise","decline","stop"]',?10,?11)"#,
            params![
                decision_id,
                delegation_id,
                decision_delegation_revision,
                plan_revision,
                policy_revision,
                serde_json::json!({"successor_action_id": successor_action_id}).to_string(),
                predecessor_action_id,
                manifest_digest,
                payload_digest,
                idempotency_key,
                requested_at,
            ],
        )?;
        let changed = tx.execute(
            "UPDATE execass_delegations SET state_revision=?2,phase='waiting_for_user',pending_decision_id=?3,updated_at=?4 WHERE delegation_id=?1 AND state_revision=?5",
            params![
                delegation_id,
                decision_delegation_revision,
                decision_id,
                requested_at,
                delegation_revision,
            ],
        )?;
        if changed != 1 {
            bail!("test duplicate-risk decision lost its delegation revision transition");
        }
        tx.commit()?;
        Ok(())
    }

    /// Drive an already-invoking test attempt through the same signed recorder
    /// evidence path used by storage's recovery tests.
    #[doc(hidden)]
    pub fn mark_test_provider_attempt_outcome_unknown(
        &self,
        attempt_id: &str,
        logical_effect_id: &str,
        observed_at: i64,
    ) -> Result<()> {
        let conn = self.connection()?;
        super::recorder::seed_signed_execution_unknown_fixture(
            &conn,
            attempt_id,
            logical_effect_id,
            observed_at,
        )
        .map(|_| ())
    }

    /// Seed one schema-valid pending confirmation without minting any owner
    /// attestation or grant. The runtime under test must perform that work.
    #[doc(hidden)]
    pub fn prepare_test_confirmation_runtime_projection(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        requested_at: i64,
        expires_at: i64,
    ) -> Result<PendingDangerConfirmationAlternativeBinding> {
        if decision_id.trim().is_empty()
            || selected_logical_action_id.trim().is_empty()
            || requested_at <= 0
            || expires_at <= requested_at
        {
            bail!("invalid test confirmation fixture");
        }
        let manifest_digest = "a".repeat(64);
        let payload_digest = "b".repeat(64);
        let nonce_digest = "c".repeat(64);
        let action_json = r#"{"operation":"erase","resolved_target":"CarsinOS state root"}"#;
        let consequence = "This permanently erases the complete CarsinOS protected state root.";
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        tx.execute(
            "INSERT INTO execass_authority_provenance (authority_provenance_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,policy_revision,evidence_digest,created_at) VALUES ('test-original-authority','human_local','test-local-owner','native-control','interactive-local','test-original-correlation','original_request','{}',1,?1,?2)",
            params!["d".repeat(64), requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_delegations (delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,ingress_credential_identity,source_correlation_id,ingress_idempotency_key,classifier_version,classifier_reasons_json,phase,run_control,state_revision,policy_revision,effective_authority_json,authority_provenance_id,created_at,updated_at) VALUES ('test-delegation','erase the complete CarsinOS protected state root','{}','native-control','test-local-owner','test-original-correlation','test-intake-idempotency','test-v1','[\"durable_work\"]','accepted','running',1,1,'{}','test-original-authority',?1,?1)",
            params![requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_plans (plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,plan_summary,resolved_leaf_manifest_json,manifest_digest,created_by_authority_provenance_id,created_at) VALUES ('test-plan','test-delegation',1,1,1,'erase exact protected state','[]',?1,'test-original-authority',?2)",
            params![manifest_digest, requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_criteria_sets (criteria_set_id,delegation_id,criteria_revision,parent_criteria_revision,disposition,created_at) VALUES ('test-criteria-set','test-delegation',1,NULL,'genesis',?1)",
            params![requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_outcome_criteria (criterion_id,delegation_id,criteria_revision,criterion_key,description,material,verifier_type,expected_predicate_json,authoritative_source_kind,created_at) VALUES ('test-criterion','test-delegation',1,'exact-result','exact result is independently verified',1,'authoritative_state','{}','test-fixture',?1)",
            params![requested_at],
        )?;
        tx.execute(
            "UPDATE execass_delegations SET current_plan_revision=1,current_criteria_revision=1,state_revision=2,updated_at=?1 WHERE delegation_id='test-delegation' AND state_revision=1",
            params![requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_action_branches (action_id,delegation_id,action_revision,target_delegation_revision,target_plan_revision,stop_epoch,branch_kind,status,action_summary,created_at,updated_at,terminal_at) VALUES (?1,'test-delegation',1,2,1,0,'ordinary','waiting','exact dangerous target',?2,?2,NULL)",
            params![selected_logical_action_id, requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_decisions (decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,decision_kind,status,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,consequence,alternatives_json,idempotency_key,requested_at) VALUES (?1,'test-delegation',1,2,1,1,'dangerous_action_confirmation','pending',?2,?3,?4,?5,'{}','[]','test.destroy','1.0.0','{}','confirm only if this exact result is intended',?6,'[\"confirm_and_continue\",\"revise\",\"decline\"]','test-decision-idempotency',?7)",
            params![decision_id, action_json, selected_logical_action_id, manifest_digest, payload_digest, consequence, requested_at],
        )?;
        tx.execute(
            "INSERT INTO execass_confirmation_challenges (challenge_id,decision_id,delegation_id,decision_revision,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence,nonce_digest,status,created_at,expires_at) VALUES ('test-challenge',?1,'test-delegation',1,?2,?3,?4,?5,'{}','test.destroy','1.0.0','{}',?6,?7,'pending',?8,?9)",
            params![decision_id, action_json, selected_logical_action_id, manifest_digest, payload_digest, consequence, nonce_digest, requested_at, expires_at],
        )?;
        tx.execute(
            "INSERT INTO execass_confirmation_challenge_alternatives (challenge_id,logical_action_id,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence) VALUES ('test-challenge',?1,?2,?1,?3,?4,'{}','[]','test.destroy','1.0.0','{}',?5)",
            params![selected_logical_action_id, action_json, manifest_digest, payload_digest, consequence],
        )?;
        tx.commit()
            .context("failed committing test confirmation fixture")?;
        match self
            .read_danger_confirmation_runtime_projection(decision_id, selected_logical_action_id)?
        {
            Some(DangerConfirmationRuntimeProjection::Pending(binding)) => Ok(*binding),
            _ => bail!("test confirmation fixture did not project pending"),
        }
    }
}
