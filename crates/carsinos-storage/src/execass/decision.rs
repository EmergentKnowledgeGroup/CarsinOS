//! Atomic typed decision resolution for the ExecAss aggregate.

use super::canonical::{parse_strict_json, CanonicalValue};
use super::foundation::authority_record_from_manifest;
use super::receipt::{
    receipt_by_causation_event, AtomicReceiptMutation, AtomicReceiptWriteOutcome,
};
use super::rows::{
    get_continuation, get_outbox, get_planned_logical_effect, get_technical_quota_snapshot,
    get_technical_resource_requirements_for_effect, insert_authority, insert_continuation,
    insert_outbox, insert_planned_logical_effect, insert_technical_quota_snapshot,
    insert_technical_resource_requirements,
};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{require_text, validate_continuation, validate_outbox};
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{
    owner_normalized_intent_digest, owner_resolution_challenge_nonce_digest, VerifiedOwnerAuthority,
};
use carsinos_core::execass_manifest::canonicalize_owner_authority;
use carsinos_protocol::execass_recorder::{canonical_json_bytes, OpaqueOperandEnvelopeV1};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

const EXACT_OVERWRITE_TOOL_ID: &str = "carsinos.local-fs";
const EXACT_OVERWRITE_TOOL_VERSION: &str = "exact-overwrite.v1";
const EXACT_OVERWRITE_ACTION_KIND: &str = "resolved_destroy";
const EXACT_OVERWRITE_PROVIDER_IDENTITY: &str = "carsinos.local-fs.exact-overwrite";
const EXACT_OVERWRITE_RECONCILIATION_CONTRACT: &str =
    "carsinos.local-fs.exact-overwrite.reconciliation.v1";
const EXACT_OVERWRITE_OPERAND_CONTRACT: &str = "carsinos.local-fs.exact-overwrite.operand.v1";
const MAX_EXACT_OVERWRITE_REPLACEMENT_BYTES: usize = 4096;

#[derive(Debug)]
enum MutationOutcome {
    Applied(Box<ResolutionDraft>),
    Replayed(Box<AtomicDecisionResolutionBundle>),
    NotFound,
    Conflict(Option<DecisionResult>),
}

#[derive(Debug)]
struct ResolutionDraft {
    decision: DecisionRecord,
    continuation: Option<ContinuationRecord>,
    logical_effect: Option<PlannedLogicalEffectRecord>,
    technical_quota_snapshot: Option<TechnicalQuotaSnapshotRecord>,
    technical_resource_requirements: Option<TechnicalResourceRequirementSetRecord>,
    outbox_event: OutboxEventRecord,
}

fn duplicate_risk_successor_identity(
    binding: &DuplicateRiskBindingRecord,
    decision: &DecisionRecord,
    resolution_identity: &str,
) -> String {
    let material = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        binding.decision_id,
        binding.delegation_id,
        binding.predecessor_logical_effect_id,
        binding.predecessor_attempt_id,
        binding.predecessor_uncertainty_evidence_digest,
        decision.decision_revision,
        decision.confirmed_logical_action_identity,
        decision.manifest_digest,
        resolution_identity,
    );
    format!(
        "{:x}",
        Sha256::digest(
            [
                b"carsinos.execass.duplicate-risk-successor.v1\0".as_slice(),
                material.as_bytes(),
            ]
            .concat()
        )
    )
}

fn duplicate_risk_derived_identity(purpose: &str, successor_identity: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "carsinos.execass.duplicate-risk-successor-identity.v1\0{purpose}\0{successor_identity}"
            )
            .as_bytes()
        )
    )
}

fn exact_overwrite_derived_identity(purpose: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.exact-overwrite-identity.v1\0");
    digest.update((purpose.len() as u64).to_be_bytes());
    digest.update(purpose.as_bytes());
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

#[derive(Debug)]
struct ExactOverwriteOperands {
    target_path: String,
    target_identity: String,
    expected_preimage_sha256: String,
    replacement_sha256: String,
}

fn canonical_value_object<'a>(
    value: &'a CanonicalValue,
    label: &str,
) -> Result<&'a std::collections::BTreeMap<String, CanonicalValue>> {
    match value {
        CanonicalValue::Object(value) => Ok(value),
        _ => bail!("{label} must be a canonical JSON object"),
    }
}

fn canonical_string(
    object: &std::collections::BTreeMap<String, CanonicalValue>,
    key: &str,
) -> Result<String> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) if !value.trim().is_empty() => Ok(value.clone()),
        _ => bail!("exact-overwrite canonical operands require nonempty {key}"),
    }
}

fn require_sha256_hex(label: &str, value: &str) -> Result<()> {
    let Some(value) = value.strip_prefix("sha256:") else {
        bail!("exact-overwrite {label} must use sha256:<lowercase hex>");
    };
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("exact-overwrite {label} must be lowercase SHA-256 hex");
    }
    Ok(())
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        bail!("exact-overwrite replacement_hex has odd length");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            let high =
                nibble(pair[0]).context("exact-overwrite replacement_hex is not lowercase hex")?;
            let low =
                nibble(pair[1]).context("exact-overwrite replacement_hex is not lowercase hex")?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn parse_exact_overwrite_payload(payload_json: &str) -> Result<ExactOverwriteOperands> {
    let payload = parse_strict_json(payload_json)?;
    if payload.to_bytes() != payload_json.as_bytes() {
        bail!("exact-overwrite payload is not canonical JSON");
    }
    let payload = canonical_value_object(&payload, "exact-overwrite payload")?;
    if payload.len() != 3 {
        bail!("exact-overwrite payload has an unexpected field set");
    }
    let operands = payload
        .get("operands")
        .context("exact-overwrite payload has no operands")?;
    let operands = canonical_value_object(operands, "exact-overwrite operands")?;
    let target_path = canonical_string(operands, "target_path")?;
    let target_identity = canonical_string(operands, "target_identity")?;
    let expected_preimage_sha256 = canonical_string(operands, "expected_preimage_sha256")?;
    let replacement_sha256 = canonical_string(operands, "replacement_sha256")?;
    let replacement_hex = canonical_string(operands, "replacement_hex")?;
    let contract_version = canonical_string(operands, "contract_version")?;
    if operands.len() != 6 || contract_version != EXACT_OVERWRITE_OPERAND_CONTRACT {
        bail!("exact-overwrite canonical operands have an unexpected field set");
    }
    require_sha256_hex("expected preimage digest", &expected_preimage_sha256)?;
    require_sha256_hex("replacement digest", &replacement_sha256)?;
    let replacement = decode_lower_hex(&replacement_hex)?;
    if replacement.is_empty()
        || replacement.len() > MAX_EXACT_OVERWRITE_REPLACEMENT_BYTES
        || format!("sha256:{:x}", Sha256::digest(&replacement)) != replacement_sha256
    {
        bail!("exact-overwrite replacement bytes do not match their bounded digest");
    }
    let material_digest = match payload.get("material_digest") {
        Some(CanonicalValue::String(value)) => value,
        _ => bail!("exact-overwrite payload requires its replacement material digest"),
    };
    if replacement_sha256.strip_prefix("sha256:") != Some(material_digest.as_str()) {
        bail!("exact-overwrite material digest is not the replacement digest");
    }
    let targets = match payload.get("target_snapshot") {
        Some(CanonicalValue::Object(snapshot)) if snapshot.len() == 1 => {
            match snapshot.get("targets") {
                Some(CanonicalValue::Array(targets)) => targets,
                _ => bail!("exact-overwrite target snapshot has no canonical targets"),
            }
        }
        _ => bail!("exact-overwrite payload has no canonical target snapshot"),
    };
    let expected_targets = [&target_identity, &target_path];
    if targets.len() != expected_targets.len()
        || !expected_targets.iter().all(|expected| {
            targets
                .iter()
                .any(|value| matches!(value, CanonicalValue::String(actual) if actual == *expected))
        })
    {
        bail!("exact-overwrite target snapshot is not the exact path and stable identity");
    }
    Ok(ExactOverwriteOperands {
        target_path,
        target_identity,
        expected_preimage_sha256,
        replacement_sha256,
    })
}

fn exact_overwrite_operand_envelope(payload_json: &str) -> Result<OpaqueOperandEnvelopeV1> {
    let payload = parse_strict_json(payload_json)?;
    if payload.to_bytes() != payload_json.as_bytes() {
        bail!("exact-overwrite payload is not canonical JSON");
    }
    let payload = canonical_value_object(&payload, "exact-overwrite payload")?;
    let operands = payload
        .get("operands")
        .context("exact-overwrite payload has no operands")?;
    let non_secret: serde_json::Value = serde_json::from_slice(&operands.to_bytes())?;
    Ok(OpaqueOperandEnvelopeV1 {
        non_secret,
        secret_handles: Vec::new(),
    })
}

pub(super) fn exact_overwrite_envelope_payload_digest(payload_json: &str) -> Result<String> {
    let envelope = exact_overwrite_operand_envelope(payload_json)?;
    let bytes = canonical_json_bytes(&envelope)
        .map_err(|_| anyhow::anyhow!("exact-overwrite operand envelope is not canonical"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn exact_overwrite_reconciliation_key(operands: &ExactOverwriteOperands) -> Result<String> {
    let value = CanonicalValue::object(vec![
        (
            "contract_version".into(),
            CanonicalValue::string(EXACT_OVERWRITE_RECONCILIATION_CONTRACT),
        ),
        (
            "expected_preimage_sha256".into(),
            CanonicalValue::string(&operands.expected_preimage_sha256),
        ),
        (
            "replacement_sha256".into(),
            CanonicalValue::string(&operands.replacement_sha256),
        ),
        (
            "target_identity".into(),
            CanonicalValue::string(&operands.target_identity),
        ),
        (
            "target_path".into(),
            CanonicalValue::string(&operands.target_path),
        ),
    ])?;
    Ok(String::from_utf8(value.to_bytes()).expect("canonical JSON is UTF-8"))
}

fn core_resource_kind(
    kind: TechnicalResourceKind,
) -> carsinos_core::execass_policy::TechnicalResourceKind {
    match kind {
        TechnicalResourceKind::Tokens => {
            carsinos_core::execass_policy::TechnicalResourceKind::Tokens
        }
        TechnicalResourceKind::TimeMs => {
            carsinos_core::execass_policy::TechnicalResourceKind::TimeMs
        }
        TechnicalResourceKind::ConnectorCalls => {
            carsinos_core::execass_policy::TechnicalResourceKind::ConnectorCalls
        }
        TechnicalResourceKind::ResourceUnits => {
            carsinos_core::execass_policy::TechnicalResourceKind::ResourceUnits
        }
    }
}

fn canonical_snapshot_from_record(
    record: &TechnicalQuotaSnapshotRecord,
) -> Result<carsinos_core::execass_policy::CanonicalTechnicalQuotaSnapshot> {
    let snapshot = carsinos_core::execass_policy::compile_technical_quota_snapshot(
        &record.delegation_id,
        record.policy_revision,
        &record.effective_authority_digest,
        &record.scope_key,
        record
            .entries
            .iter()
            .map(
                |entry| carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                    kind: core_resource_kind(entry.technical_resource_kind),
                    unit: entry.unit.clone(),
                    limit: entry.amount_limit,
                },
            )
            .collect(),
    )
    .map_err(|detail| anyhow::anyhow!("invalid persisted technical quota snapshot: {detail}"))?;
    if snapshot.quota_snapshot_id != record.quota_snapshot_id
        || snapshot.canonical_entries_json != record.canonical_entries_json
        || snapshot.canonical_entries_digest != record.canonical_entries_digest
    {
        bail!("persisted duplicate-risk quota snapshot is noncanonical");
    }
    Ok(snapshot)
}

fn canonical_requirements_from_record(
    record: &TechnicalResourceRequirementSetRecord,
    snapshot_record: &TechnicalQuotaSnapshotRecord,
) -> Result<carsinos_core::execass_policy::CanonicalTechnicalResourceRequirementSet> {
    let snapshot = canonical_snapshot_from_record(snapshot_record)?;
    let requirements = carsinos_core::execass_policy::compile_technical_resource_requirements(
        &snapshot,
        &record.logical_effect_id,
        &record.action_id,
        &record.manifest_digest,
        record
            .requirements
            .iter()
            .map(
                |requirement| carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                    kind: core_resource_kind(requirement.technical_resource_kind),
                    unit: requirement.unit.clone(),
                    amount: requirement.amount_required,
                },
            )
            .collect(),
    )
    .map_err(|detail| anyhow::anyhow!("invalid persisted technical requirements: {detail}"))?;
    if requirements.requirement_set_id != record.requirement_set_id
        || requirements.quota_snapshot_id != record.quota_snapshot_id
        || requirements.canonical_requirements_json != record.canonical_requirements_json
        || requirements.canonical_requirements_digest != record.canonical_requirements_digest
    {
        bail!("persisted duplicate-risk requirements are noncanonical");
    }
    Ok(requirements)
}

fn load_logical_effect_material(
    conn: &Connection,
    logical_effect_id: &str,
) -> Result<Option<PlannedLogicalEffectRecord>> {
    conn.query_row(
        r#"SELECT logical_effect_id,delegation_id,continuation_id,action_kind,
                  operation_reversible,declared_recovery_safe_boundary,
                  internal_idempotency_key,provider_identity,provider_idempotency_key,
                  reconciliation_key,manifest_digest,payload_digest,created_at
           FROM execass_logical_effects WHERE logical_effect_id=?1"#,
        params![logical_effect_id],
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
    .context("failed reading duplicate-risk logical effect material")
}

impl ExecAssStore {
    /// Prepare execution material only for the installed exact-overwrite leaf.
    /// A non-matching dangerous tuple intentionally returns `None`; malformed
    /// material for the installed tuple fails closed.
    pub fn prepare_exact_dangerous_effect(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        occurred_at: i64,
        runtime_host_generation: i64,
        global_stop_epoch: i64,
    ) -> Result<Option<PreparedExactDangerousEffect>> {
        require_text("decision_id", decision_id)?;
        require_text("selected_logical_action_id", selected_logical_action_id)?;
        if occurred_at <= 0 || runtime_host_generation <= 0 || global_stop_epoch < 0 {
            bail!("exact dangerous effect requires valid trusted runtime coordinates");
        }
        let conn = self.connection()?;
        prepare_exact_dangerous_effect_from_conn(
            &conn,
            decision_id,
            selected_logical_action_id,
            occurred_at,
            runtime_host_generation,
            global_stop_epoch,
        )
    }

    /// Read the recorder operands only after the resolved decision, active
    /// grant, selected alternative, continuation, and logical effect converge
    /// on the exact installed leaf. No request-supplied operand is accepted.
    pub fn read_exact_dangerous_effect_execution_material(
        &self,
        delegation_id: &str,
        continuation_id: &str,
    ) -> Result<Option<ExactDangerousEffectExecutionMaterial>> {
        require_text("delegation_id", delegation_id)?;
        require_text("continuation_id", continuation_id)?;
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            r#"SELECT e.logical_effect_id,a.payload_and_material_operands_json,e.payload_digest,e.reconciliation_key
               FROM execass_logical_effects e
               JOIN execass_continuations continuation
                 ON continuation.delegation_id=e.delegation_id
                AND continuation.continuation_id=e.continuation_id
                AND continuation.causation_kind='decision'
               JOIN execass_decisions d
                 ON d.decision_id=continuation.causation_id
                AND d.delegation_id=e.delegation_id
                AND d.status='resolved' AND d.result='confirm_and_continue'
                AND d.decision_kind='dangerous_action_confirmation'
               JOIN execass_confirmation_challenges challenge
                 ON challenge.decision_id=d.decision_id
                AND challenge.status='resolved'
                AND challenge.selected_logical_action_id=continuation.action_id
               JOIN execass_confirmation_challenge_alternatives a
                 ON a.challenge_id=challenge.challenge_id
                AND a.logical_action_id=continuation.action_id
               JOIN execass_accepted_confirmation_grants grant
                 ON grant.decision_id=d.decision_id AND grant.invalidated_at IS NULL
               WHERE e.delegation_id=?1 AND e.continuation_id=?2
                 AND e.state IN ('planned','claimed','invoking','succeeded','failed','outcome_unknown','reconciled_absent','reconciled_present')
                 AND e.action_kind='irreversible_or_destructive_action'
                 AND e.operation_reversible=0
                 AND e.declared_recovery_safe_boundary='independent_absence'
                 AND e.provider_identity='carsinos.local-fs.exact-overwrite'
                 AND e.provider_idempotency_key IS NULL
                 AND e.reconciliation_key IS NOT NULL
                 AND e.manifest_digest=d.manifest_digest
                 AND a.manifest_digest=d.manifest_digest
                 AND a.connector_tool_identity='carsinos.local-fs'
                 AND a.connector_tool_version='exact-overwrite.v1'
                 AND d.exact_presented_action_json=a.exact_presented_action_json
                 AND d.confirmed_logical_action_identity=a.confirmed_logical_action_identity
                 AND d.payload_digest=a.payload_digest
                 AND d.payload_and_material_operands_json=a.payload_and_material_operands_json
                 AND d.connector_tool_identity=a.connector_tool_identity
                 AND d.connector_tool_version=a.connector_tool_version
                 AND d.side_effect_envelope_json=a.canonical_action_envelope_or_selector_json
                 AND grant.delegation_id=d.delegation_id
                 AND grant.confirmed_logical_action_identity=a.confirmed_logical_action_identity
                 AND grant.payload_and_material_operands_json=a.payload_and_material_operands_json
                 AND grant.payload_and_material_operands_digest=a.payload_digest
                 AND grant.connector_tool_identity=a.connector_tool_identity
                 AND grant.connector_tool_version=a.connector_tool_version
                 AND grant.canonical_action_envelope_or_selector_json=a.canonical_action_envelope_or_selector_json
               LIMIT 2"#,
            )?;
        let rows = statement
            .query_map(params![delegation_id, continuation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let (logical_effect_id, payload_json, payload_digest, reconciliation_key) =
            match rows.as_slice() {
                [] => return Ok(None),
                [row] => row.clone(),
                _ => bail!("exact dangerous execution material is ambiguous"),
            };
        let operands = parse_exact_overwrite_payload(&payload_json)?;
        let expected_reconciliation_key = exact_overwrite_reconciliation_key(&operands)?;
        let operand_envelope = exact_overwrite_operand_envelope(&payload_json)?;
        let expected_payload_digest = exact_overwrite_envelope_payload_digest(&payload_json)?;
        if reconciliation_key != expected_reconciliation_key
            || payload_digest != expected_payload_digest
        {
            bail!("exact dangerous execution material drifted from persisted effect authority");
        }
        Ok(Some(ExactDangerousEffectExecutionMaterial {
            logical_effect_id,
            provider_identity: EXACT_OVERWRITE_PROVIDER_IDENTITY.into(),
            provider_version: "v1".into(),
            adapter_identity: "carsinos.effect-recorder.exact-overwrite.v1".into(),
            payload_digest,
            reconciliation_key,
            operand_envelope,
        }))
    }

    /// Load immutable, server-derived proof material for any current typed
    /// decision. Dangerous decisions use their disclosed challenge; all other
    /// kinds bind the persisted decision idempotency identity as the nonce and
    /// expose no caller-selected authority material.
    pub fn read_decision_resolution_binding(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
    ) -> Result<Option<DecisionResolutionBinding>> {
        require_text("decision_id", decision_id)?;
        require_text("selected_logical_action_id", selected_logical_action_id)?;
        let conn = self.connection()?;
        let mut binding = conn.query_row(
            r#"SELECT d.delegation_id,e.normalized_original_intent,d.policy_revision,
              d.decision_id,d.decision_revision,d.decision_kind,p.resolved_leaf_manifest_json,
              d.manifest_digest,
              CASE WHEN d.decision_kind='dangerous_action_confirmation' THEN a.logical_action_id
                   ELSE d.confirmed_logical_action_identity END,
              CASE WHEN d.decision_kind='dangerous_action_confirmation' THEN a.exact_presented_action_json
                   ELSE d.exact_presented_action_json END,
              CASE WHEN d.decision_kind='dangerous_action_confirmation' THEN a.declared_consequence
                   ELSE d.consequence END,
              c.nonce_digest,d.idempotency_key,d.requested_at,c.expires_at
              FROM execass_decisions d
              JOIN execass_delegations e ON e.delegation_id=d.delegation_id
              JOIN execass_plans p ON p.delegation_id=d.delegation_id
                AND p.plan_revision=d.plan_revision AND p.manifest_digest=d.manifest_digest
              LEFT JOIN execass_confirmation_challenges c ON c.decision_id=d.decision_id
              LEFT JOIN execass_confirmation_challenge_alternatives a
                ON a.challenge_id=c.challenge_id AND a.logical_action_id=?2
              WHERE d.decision_id=?1 AND d.status IN ('pending','resolved')
                AND (d.decision_kind!='dangerous_action_confirmation'
                     OR (a.logical_action_id IS NOT NULL
                         AND (c.status='pending' OR c.selected_logical_action_id=?2)))
                AND (d.decision_kind IN ('dangerous_action_confirmation','duplicate_risk_retry')
                     OR d.confirmed_logical_action_identity=?2)"#,
            params![decision_id, selected_logical_action_id],
            |row| {
                let decision_kind: DecisionKind = row.get(5)?;
                let action_json: String = row.get(9)?;
                let consequence: String = row.get(10)?;
                let persisted_idempotency: String = row.get(12)?;
                let challenge_nonce_digest = if decision_kind
                    == DecisionKind::DangerousActionConfirmation
                {
                    row.get(11)?
                } else {
                    owner_resolution_challenge_nonce_digest(persisted_idempotency.as_bytes())
                        .ok_or_else(|| rusqlite::Error::InvalidQuery)?
                };
                Ok(DecisionResolutionBinding {
                    delegation_id: row.get(0)?,
                    normalized_intent: row.get(1)?,
                    policy_revision: row.get(2)?,
                    decision_id: row.get(3)?,
                    decision_revision: row.get(4)?,
                    decision_kind,
                    canonical_manifest_json: row.get(6)?,
                    manifest_digest: row.get(7)?,
                    selected_logical_action_id: row.get(8)?,
                    exact_selected_action_json: action_json.clone(),
                    exact_selected_action_digest: format!(
                        "{:x}",
                        Sha256::digest(action_json.as_bytes())
                    ),
                    declared_consequence: consequence.clone(),
                    declared_consequence_digest: format!(
                        "{:x}",
                        Sha256::digest(consequence.as_bytes())
                    ),
                    challenge_nonce_digest,
                    requested_at: row.get(13)?,
                    expires_at: row.get::<_, Option<i64>>(14)?.unwrap_or(i64::MAX),
                })
            },
        )
        .optional()
        .context("failed reading exact typed decision resolution binding")?;
        let Some(binding_ref) = binding.as_mut() else {
            return Ok(None);
        };
        if binding_ref.decision_kind == DecisionKind::DuplicateRiskRetry {
            let (status, delegation_id, delegation_revision, plan_revision):
                (DecisionStatus, String, i64, i64) = conn.query_row(
                    "SELECT status,delegation_id,delegation_revision,plan_revision FROM execass_decisions WHERE decision_id=?1",
                    params![decision_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
            let sql = if status == DecisionStatus::Pending {
                r#"SELECT action_id FROM execass_action_branches
                   WHERE delegation_id=?1 AND target_delegation_revision=?2
                     AND target_plan_revision=?3 AND status='waiting'
                     AND action_id!=(SELECT confirmed_logical_action_identity
                                      FROM execass_decisions WHERE decision_id=?4)
                   ORDER BY action_id LIMIT 2"#
            } else {
                r#"SELECT action_id FROM execass_continuations
                   WHERE delegation_id=?1 AND causation_kind='decision' AND causation_id=?4
                   ORDER BY action_id LIMIT 2"#
            };
            let mut statement = conn.prepare(sql)?;
            let actions = statement
                .query_map(
                    params![
                        delegation_id,
                        delegation_revision,
                        plan_revision,
                        decision_id,
                    ],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let exact_action = match actions.as_slice() {
                [action] => action,
                [] => return Ok(None),
                _ => bail!("duplicate-risk decision has ambiguous waiting successors"),
            };
            if exact_action != selected_logical_action_id {
                return Ok(None);
            }
            binding_ref.selected_logical_action_id = exact_action.clone();
        }
        Ok(binding)
    }

    /// Derives and freezes the only unresolved provider effect that a pending
    /// duplicate-risk decision may supersede. Callers select only the decision;
    /// storage selects the exact latest unknown attempt and evidence.
    pub fn bind_duplicate_risk_predecessor(
        &self,
        decision_id: &str,
        trusted_now: i64,
    ) -> Result<Option<DuplicateRiskBindingRecord>> {
        require_text("decision_id", decision_id)?;
        if trusted_now <= 0 {
            bail!("duplicate-risk binding requires a positive trusted clock");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        if let Some(existing) = load_duplicate_risk_binding(&tx, decision_id)? {
            tx.commit()?;
            return Ok(Some(existing));
        }
        let mut rows = tx.prepare(
            r#"SELECT d.decision_id,d.delegation_id,e.logical_effect_id,a.attempt_id,
                      a.provider_response_digest,d.confirmed_logical_action_identity,
                      (SELECT g.grant_id FROM execass_accepted_confirmation_grants g
                       WHERE g.delegation_id=d.delegation_id
                         AND g.confirmed_logical_action_identity=d.confirmed_logical_action_identity
                         AND g.invalidated_at IS NULL
                       ORDER BY g.accepted_at DESC,g.grant_id DESC LIMIT 1)
               FROM execass_decisions d
               JOIN execass_continuations c
                 ON c.delegation_id=d.delegation_id
                AND c.action_id=d.confirmed_logical_action_identity
               JOIN execass_logical_effects e
                 ON e.delegation_id=c.delegation_id
                AND e.continuation_id=c.continuation_id
               JOIN execass_provider_attempts a
                 ON a.delegation_id=e.delegation_id
                AND a.logical_effect_id=e.logical_effect_id
               WHERE d.decision_id=?1
                 AND d.decision_kind='duplicate_risk_retry'
                 AND d.status='pending'
                 AND e.state='outcome_unknown'
                 AND e.manifest_digest=d.manifest_digest
                 AND e.payload_digest=d.payload_digest
                 AND a.status='outcome_unknown'
                 AND a.provider_response_digest IS NOT NULL
                 AND a.attempt_number=(SELECT MAX(latest.attempt_number)
                                       FROM execass_provider_attempts latest
                                       WHERE latest.logical_effect_id=e.logical_effect_id)
               ORDER BY e.logical_effect_id
               LIMIT 2"#,
        )?;
        let candidates = rows
            .query_map(params![decision_id], |row| {
                Ok(DuplicateRiskBindingRecord {
                    decision_id: row.get(0)?,
                    delegation_id: row.get(1)?,
                    predecessor_logical_effect_id: row.get(2)?,
                    predecessor_attempt_id: row.get(3)?,
                    predecessor_uncertainty_evidence_digest: row.get(4)?,
                    confirmed_logical_action_identity: row.get(5)?,
                    accepted_confirmation_grant_id: row.get(6)?,
                    created_at: trusted_now,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(rows);
        let binding = match candidates.as_slice() {
            [] => {
                tx.commit()?;
                return Ok(None);
            }
            [binding] => binding.clone(),
            _ => bail!("duplicate-risk decision has ambiguous unresolved predecessors"),
        };
        tx.execute(
            r#"INSERT INTO execass_duplicate_risk_bindings(
                 decision_id,delegation_id,predecessor_logical_effect_id,
                 predecessor_attempt_id,predecessor_uncertainty_evidence_digest,
                 confirmed_logical_action_identity,accepted_confirmation_grant_id,created_at
               ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"#,
            params![
                binding.decision_id,
                binding.delegation_id,
                binding.predecessor_logical_effect_id,
                binding.predecessor_attempt_id,
                binding.predecessor_uncertainty_evidence_digest,
                binding.confirmed_logical_action_identity,
                binding.accepted_confirmation_grant_id,
                binding.created_at,
            ],
        )?;
        tx.commit()?;
        Ok(Some(binding))
    }

    /// Freeze and prepare the sole fresh logical effect authorized by an
    /// affirmative duplicate-risk resolution. All action, effect, provider,
    /// quota, and resource facts come from persisted server authority. The
    /// verified resolution identity is used only to derive fresh identities.
    pub fn prepare_duplicate_risk_successor(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        resolution_identity: &str,
        occurred_at: i64,
        runtime_host_generation: i64,
        global_stop_epoch: i64,
    ) -> Result<Option<PreparedDuplicateRiskSuccessor>> {
        require_text("decision_id", decision_id)?;
        require_text("selected_logical_action_id", selected_logical_action_id)?;
        require_text("resolution_identity", resolution_identity)?;
        if occurred_at <= 0 || runtime_host_generation <= 0 || global_stop_epoch < 0 {
            bail!("duplicate-risk successor requires valid trusted runtime coordinates");
        }
        let Some(binding) = self.bind_duplicate_risk_predecessor(decision_id, occurred_at)? else {
            return Ok(None);
        };
        let conn = self.connection()?;
        let decision = load_decision(&conn, decision_id)?
            .context("duplicate-risk successor decision disappeared")?;
        if decision.decision_kind != DecisionKind::DuplicateRiskRetry
            || binding.delegation_id != decision.delegation_id
            || binding.confirmed_logical_action_identity
                != decision.confirmed_logical_action_identity
            || selected_logical_action_id == binding.confirmed_logical_action_identity
        {
            bail!("duplicate-risk successor does not match its frozen decision binding");
        }
        let identity = duplicate_risk_successor_identity(&binding, &decision, resolution_identity);
        let continuation_id = format!("duplicate-risk-continuation-{identity}");
        let logical_effect_id = format!("duplicate-risk-effect-{identity}");

        // Replays reconstruct the exact persisted command material. This keeps
        // replay stable even if the active runtime generation changes later.
        if let Some(persisted_effect_id) = conn
            .query_row(
                "SELECT successor_logical_effect_id FROM execass_duplicate_risk_successors WHERE decision_id=?1",
                params![decision_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            if persisted_effect_id != logical_effect_id {
                bail!("duplicate-risk successor resolution identity conflicts with the winner");
            }
            let effect = get_planned_logical_effect(&conn, &persisted_effect_id)?
                .context("duplicate-risk persisted successor effect disappeared")?;
            let continuation = get_continuation(&conn, &effect.continuation_id)?
                .context("duplicate-risk persisted successor continuation disappeared")?;
            if continuation.action_id != selected_logical_action_id {
                bail!("duplicate-risk successor action conflicts with the winner");
            }
            let requirements = get_technical_resource_requirements_for_effect(
                &conn,
                &effect.logical_effect_id,
            )?
            .context("duplicate-risk persisted successor requirements disappeared")?;
            let snapshot = get_technical_quota_snapshot(&conn, &requirements.quota_snapshot_id)?
                .context("duplicate-risk persisted successor quota snapshot disappeared")?;
            return Ok(Some(PreparedDuplicateRiskSuccessor {
                continuation,
                logical_effect: effect,
                technical_quota_snapshot: canonical_snapshot_from_record(&snapshot)?,
                technical_resource_requirements: canonical_requirements_from_record(
                    &requirements,
                    &snapshot,
                )?,
            }));
        }

        if decision.status != DecisionStatus::Pending {
            bail!("resolved duplicate-risk decision has no persisted successor");
        }
        let mut action_rows = conn.prepare(
            r#"SELECT action_id,target_delegation_revision,target_plan_revision,stop_epoch,branch_kind
               FROM execass_action_branches
               WHERE delegation_id=?1 AND action_id=?2 AND target_delegation_revision=?3
                 AND target_plan_revision=?4 AND status='waiting'
               ORDER BY action_id LIMIT 2"#,
        )?;
        let actions = action_rows
            .query_map(
                params![
                    decision.delegation_id,
                    selected_logical_action_id,
                    decision.delegation_revision,
                    decision.plan_revision,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let action = match actions.as_slice() {
            [action] => action.clone(),
            [] => bail!("duplicate-risk successor has no exact waiting action"),
            _ => bail!("duplicate-risk successor has ambiguous waiting actions"),
        };
        drop(action_rows);
        let predecessor =
            load_logical_effect_material(&conn, &binding.predecessor_logical_effect_id)?
                .context("duplicate-risk frozen predecessor effect disappeared")?;
        let predecessor_requirements =
            get_technical_resource_requirements_for_effect(&conn, &predecessor.logical_effect_id)?
                .context("duplicate-risk predecessor has no technical resource requirements")?;
        let predecessor_snapshot =
            get_technical_quota_snapshot(&conn, &predecessor_requirements.quota_snapshot_id)?
                .context("duplicate-risk predecessor has no technical quota snapshot")?;
        if predecessor_requirements.action_id != binding.confirmed_logical_action_identity
            || predecessor_requirements.manifest_digest != decision.manifest_digest
            || predecessor.manifest_digest != decision.manifest_digest
        {
            bail!("duplicate-risk predecessor resource authority does not match the decision");
        }
        let authority_json: String = conn.query_row(
            "SELECT effective_authority_json FROM execass_delegations WHERE delegation_id=?1 AND state_revision=?2 AND policy_revision=?3",
            params![decision.delegation_id, decision.delegation_revision, decision.policy_revision],
            |row| row.get(0),
        )?;
        let authority_digest =
            carsinos_core::execass_policy::technical_effective_authority_digest(&authority_json)
                .map_err(|detail| {
                    anyhow::anyhow!("invalid technical authority snapshot: {detail}")
                })?;
        let snapshot = carsinos_core::execass_policy::compile_technical_quota_snapshot(
            &decision.delegation_id,
            decision.policy_revision,
            &authority_digest,
            "delegation",
            predecessor_snapshot
                .entries
                .iter()
                .map(
                    |entry| carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                        kind: core_resource_kind(entry.technical_resource_kind),
                        unit: entry.unit.clone(),
                        limit: entry.amount_limit,
                    },
                )
                .collect(),
        )
        .map_err(|detail| anyhow::anyhow!("invalid duplicate-risk quota snapshot: {detail}"))?;
        let requirements = carsinos_core::execass_policy::compile_technical_resource_requirements(
            &snapshot,
            &logical_effect_id,
            &action.0,
            &decision.manifest_digest,
            predecessor_requirements
                .requirements
                .iter()
                .map(|requirement| {
                    carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                        kind: core_resource_kind(requirement.technical_resource_kind),
                        unit: requirement.unit.clone(),
                        amount: requirement.amount_required,
                    }
                })
                .collect(),
        )
        .map_err(|detail| {
            anyhow::anyhow!("invalid duplicate-risk resource requirements: {detail}")
        })?;
        let continuation = ContinuationRecord {
            continuation_id: continuation_id.clone(),
            delegation_id: decision.delegation_id.clone(),
            target_delegation_revision: action.1,
            target_plan_revision: action.2,
            action_id: action.0,
            branch_kind: action.4,
            causation_kind: ContinuationCausationKind::Decision,
            causation_id: decision.decision_id.clone(),
            status: ContinuationStatus::Runnable,
            job_id: None,
            lease_owner: None,
            lease_expires_at: None,
            fencing_token: 0,
            host_generation: runtime_host_generation,
            stop_epoch: action.3,
            global_stop_epoch,
            created_at: occurred_at,
            updated_at: occurred_at,
            completed_at: None,
        };
        let provider_idempotency_key = predecessor.provider_identity.as_ref().map(|_| {
            format!(
                "duplicate-risk-provider-{}",
                duplicate_risk_derived_identity("provider-idempotency", &identity)
            )
        });
        let reconciliation_key = predecessor.provider_identity.as_ref().map(|_| {
            format!(
                "duplicate-risk-reconcile-{}",
                duplicate_risk_derived_identity("reconciliation", &identity)
            )
        });
        Ok(Some(PreparedDuplicateRiskSuccessor {
            continuation,
            logical_effect: PlannedLogicalEffectRecord {
                logical_effect_id,
                delegation_id: decision.delegation_id,
                continuation_id,
                action_kind: predecessor.action_kind,
                operation_reversible: predecessor.operation_reversible,
                declared_recovery_safe_boundary: predecessor.declared_recovery_safe_boundary,
                internal_idempotency_key: format!(
                    "duplicate-risk-internal-{}",
                    duplicate_risk_derived_identity("internal-idempotency", &identity)
                ),
                provider_identity: predecessor.provider_identity,
                provider_idempotency_key,
                reconciliation_key,
                manifest_digest: predecessor.manifest_digest,
                payload_digest: predecessor.payload_digest,
                created_at: occurred_at,
            },
            technical_quota_snapshot: snapshot,
            technical_resource_requirements: requirements,
        }))
    }

    pub fn read_decision_receipt_context(
        &self,
        decision_id: &str,
        trusted_now: i64,
    ) -> Result<Option<DecisionReceiptContext>> {
        require_text("decision_id", decision_id)?;
        if trusted_now <= 0 {
            bail!("decision receipt context requires a positive trusted clock");
        }
        let conn = self.connection()?;
        conn.query_row(
            r#"SELECT d.delegation_id,d.delegation_revision,d.plan_revision,e.stop_epoch,
              global_control.global_stop_epoch,
              journal.receipt_count,journal.receipt_head_digest,
              e.receipt_chain_count,e.receipt_chain_head_digest,g.state_root_generation,
              lease.generation,lease.host_instance_id,lease.fencing_token
              FROM execass_decisions d
              JOIN execass_delegations e ON e.delegation_id=d.delegation_id
              CROSS JOIN execass_receipt_journal_state journal
              CROSS JOIN execass_global_runtime_control global_control
              JOIN execass_runtime_host_leases lease ON lease.ownership_scope='execass'
                AND lease.released_at IS NULL AND lease.expires_at>?2
              JOIN execass_runtime_host_generations g ON g.generation=lease.generation
                AND g.host_instance_id=lease.host_instance_id
              WHERE d.decision_id=?1 AND d.delegation_revision=e.state_revision
                AND journal.singleton=1 AND global_control.singleton=1
              ORDER BY lease.generation DESC,lease.fencing_token DESC LIMIT 2"#,
            params![decision_id, trusted_now],
            |row| {
                Ok(DecisionReceiptContext {
                    delegation_id: row.get(0)?,
                    delegation_revision: row.get(1)?,
                    plan_revision: row.get(2)?,
                    stop_epoch: row.get(3)?,
                    global_stop_epoch: row.get(4)?,
                    global_receipt_count: row.get(5)?,
                    global_receipt_head_digest: row.get(6)?,
                    delegation_receipt_count: row.get(7)?,
                    delegation_receipt_head_digest: row.get(8)?,
                    state_root_generation: row.get(9)?,
                    runtime_host_generation: row.get(10)?,
                    runtime_host_instance_id: row.get(11)?,
                    runtime_fencing_token: row.get(12)?,
                })
            },
        )
        .optional()
        .context("failed reading exact decision receipt context")
    }

    pub fn read_waiting_decision_action(
        &self,
        decision_id: &str,
        action_id: &str,
    ) -> Result<Option<ActionBranchRecord>> {
        require_text("decision_id", decision_id)?;
        require_text("action_id", action_id)?;
        let conn = self.connection()?;
        conn.query_row(
            r#"SELECT a.action_id,a.delegation_id,a.action_revision,a.target_delegation_revision,
              a.target_plan_revision,a.stop_epoch,a.branch_kind,a.status,a.action_summary,
              a.created_at,a.updated_at,a.terminal_at
              FROM execass_action_branches a JOIN execass_decisions d
                ON d.delegation_id=a.delegation_id
               AND d.delegation_revision=a.target_delegation_revision
               AND d.plan_revision=a.target_plan_revision
              WHERE d.decision_id=?1 AND a.action_id=?2 AND a.status='waiting'"#,
            params![decision_id, action_id],
            |row| {
                Ok(ActionBranchRecord {
                    action_id: row.get(0)?,
                    delegation_id: row.get(1)?,
                    action_revision: row.get(2)?,
                    target_delegation_revision: row.get(3)?,
                    target_plan_revision: row.get(4)?,
                    stop_epoch: row.get(5)?,
                    branch_kind: row.get(6)?,
                    status: row.get(7)?,
                    action_summary: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    terminal_at: row.get(11)?,
                })
            },
        )
        .optional()
        .context("failed reading exact waiting decision action")
    }

    /// Resolves a non-affirmative dangerous decision or any other typed owner
    /// decision through one receipt-anchored transaction. Affirmative dangerous
    /// confirmation remains available only through the signed-attestation
    /// adapter and is joined to this kernel separately.
    pub fn resolve_decision_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        command: &AtomicDecisionResolutionCommand,
        owner_authority: &VerifiedOwnerAuthority,
    ) -> Result<AtomicDecisionResolutionOutcome> {
        validate_static_command(command)?;
        let canonical_authority = canonicalize_owner_authority(owner_authority)
            .map_err(|detail| anyhow::anyhow!("invalid decision resolution authority: {detail}"))?;
        let authority_record = authority_record_from_manifest(&canonical_authority)?;
        if command.receipt.actor.authority_provenance_id != authority_record.authority_provenance_id
            || command.receipt.actor.actor_type != authority_record.actor_type
            || command.receipt.actor.actor_identity.as_str()
                != authority_record.credential_identity.as_str()
        {
            bail!("decision receipt actor is not the verified owner authority");
        }

        // Exact replays must not depend on reconstructing the receipt heads
        // that existed before the original append. Persisted outbox/receipt
        // identities remain the canonical material comparison.
        {
            let conn = self.connection()?;
            if let Some(decision) = load_decision(&conn, &command.decision_id)? {
                if decision.decision_revision != command.decision_revision {
                    return Ok(AtomicDecisionResolutionOutcome::Conflict {
                        winning_result: decision.result,
                    });
                }
                if decision.status == DecisionStatus::Resolved {
                    return Ok(match load_replay_bundle(&conn, command, &decision)? {
                        Some(bundle) => AtomicDecisionResolutionOutcome::Replayed(Box::new(bundle)),
                        None => AtomicDecisionResolutionOutcome::Conflict {
                            winning_result: decision.result,
                        },
                    });
                }
            }
        }

        let outcome = self.mutate_with_atomic_receipt(
            integrity,
            redactor,
            &command.receipt,
            |transaction| {
                let Some(decision) = load_decision(transaction, &command.decision_id)? else {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationOutcome::NotFound));
                };
                if decision.decision_revision != command.decision_revision {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationOutcome::Conflict(
                        decision.result,
                    )));
                }
                if decision.status == DecisionStatus::Resolved {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        match load_replay_bundle(transaction, command, &decision)? {
                            Some(bundle) => MutationOutcome::Replayed(Box::new(bundle)),
                            None => MutationOutcome::Conflict(decision.result),
                        },
                    ));
                }
                if decision.status != DecisionStatus::Pending {
                    return Ok(AtomicReceiptMutation::NoAppend(MutationOutcome::Conflict(
                        decision.result,
                    )));
                }
                validate_live_resolution(transaction, command, &decision, &canonical_authority)?;
                insert_authority(transaction, &authority_record)?;
                resolve_pending_decision(
                    transaction,
                    command,
                    &decision,
                    &authority_record.authority_provenance_id,
                )?;
                if let Some(continuation) = &command.continuation {
                    insert_continuation(transaction, continuation)?;
                    promote_decision_action_to_runnable(transaction, command, continuation)?;
                }
                if let Some(effect) = &command.logical_effect {
                    insert_planned_logical_effect(transaction, effect)?;
                    insert_duplicate_risk_successor(transaction, command, &decision, effect)?;
                    let snapshot = command
                        .technical_quota_snapshot
                        .as_ref()
                        .context("planned logical effect has no technical quota snapshot")?;
                    insert_technical_quota_snapshot(
                        transaction,
                        snapshot,
                        command.write.occurred_at,
                    )?;
                    let requirements = command
                        .technical_resource_requirements
                        .as_ref()
                        .context("planned logical effect has no technical resource requirements")?;
                    insert_technical_resource_requirements(
                        transaction,
                        requirements,
                        command.write.occurred_at,
                    )?;
                }
                insert_outbox(transaction, &command.outbox_event)?;
                let resolved = load_decision(transaction, &command.decision_id)?
                    .context("resolved decision disappeared")?;
                let outbox_event = get_outbox(transaction, &command.outbox_event.event_id)?
                    .context("decision outbox disappeared")?;
                Ok(AtomicReceiptMutation::Append(MutationOutcome::Applied(
                    Box::new(ResolutionDraft {
                        decision: resolved,
                        continuation: command.continuation.clone(),
                        logical_effect: command.logical_effect.clone(),
                        technical_quota_snapshot: command
                            .technical_quota_snapshot
                            .as_ref()
                            .map(|snapshot| {
                                get_technical_quota_snapshot(
                                    transaction,
                                    &snapshot.quota_snapshot_id,
                                )
                            })
                            .transpose()?
                            .flatten(),
                        technical_resource_requirements: command
                            .logical_effect
                            .as_ref()
                            .map(|effect| {
                                get_technical_resource_requirements_for_effect(
                                    transaction,
                                    &effect.logical_effect_id,
                                )
                            })
                            .transpose()?
                            .flatten(),
                        outbox_event,
                    }),
                )))
            },
        )?;

        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: MutationOutcome::Applied(draft),
                receipt,
            } => Ok(AtomicDecisionResolutionOutcome::Applied(Box::new(
                AtomicDecisionResolutionBundle {
                    decision: draft.decision,
                    confirmation_grant: None,
                    continuation: draft.continuation,
                    logical_effect: draft.logical_effect,
                    technical_quota_snapshot: draft.technical_quota_snapshot,
                    technical_resource_requirements: draft.technical_resource_requirements,
                    outbox_event: draft.outbox_event,
                    receipt,
                },
            ))),
            AtomicReceiptWriteOutcome::NoAppend(MutationOutcome::Replayed(bundle)) => {
                Ok(AtomicDecisionResolutionOutcome::Replayed(bundle))
            }
            AtomicReceiptWriteOutcome::NoAppend(MutationOutcome::NotFound) => {
                Ok(AtomicDecisionResolutionOutcome::NotFound)
            }
            AtomicReceiptWriteOutcome::NoAppend(MutationOutcome::Conflict(winning_result)) => {
                Ok(AtomicDecisionResolutionOutcome::Conflict { winning_result })
            }
            AtomicReceiptWriteOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            } => Ok(AtomicDecisionResolutionOutcome::Stale {
                current_state_revision,
                global_count,
                global_head_digest,
                delegation_count,
                delegation_head_digest,
            }),
            AtomicReceiptWriteOutcome::Appended { .. }
            | AtomicReceiptWriteOutcome::NoAppend(MutationOutcome::Applied(_)) => {
                bail!("atomic decision receipt coordinator returned an impossible outcome")
            }
        }
    }
}

fn prepare_exact_dangerous_effect_from_conn(
    conn: &Connection,
    decision_id: &str,
    selected_logical_action_id: &str,
    occurred_at: i64,
    runtime_host_generation: i64,
    global_stop_epoch: i64,
) -> Result<Option<PreparedExactDangerousEffect>> {
    let Some(decision) = load_decision(conn, decision_id)? else {
        return Ok(None);
    };
    if decision.decision_kind != DecisionKind::DangerousActionConfirmation {
        return Ok(None);
    }
    if decision.status != DecisionStatus::Pending {
        bail!("exact dangerous effect preparation requires its pending decision");
    }
    #[allow(clippy::type_complexity)]
    let alternative: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = conn
        .query_row(
            r#"SELECT a.exact_presented_action_json,a.confirmed_logical_action_identity,
                      a.manifest_digest,a.payload_digest,a.payload_and_material_operands_json,
                      a.target_audience_path_json,a.connector_tool_identity,
                      a.connector_tool_version,a.canonical_action_envelope_or_selector_json,
                      p.resolved_leaf_manifest_json,d.effective_authority_json
               FROM execass_confirmation_challenges c
               JOIN execass_confirmation_challenge_alternatives a
                 ON a.challenge_id=c.challenge_id AND a.logical_action_id=?2
               JOIN execass_plans p ON p.delegation_id=c.delegation_id
                 AND p.plan_revision=(SELECT current_plan_revision FROM execass_delegations WHERE delegation_id=c.delegation_id)
                 AND p.manifest_digest=a.manifest_digest
               JOIN execass_delegations d ON d.delegation_id=c.delegation_id
               WHERE c.decision_id=?1 AND c.status='pending' AND c.decision_revision=?3
                 AND c.manifest_digest=a.manifest_digest
                 AND d.state_revision=?4 AND d.policy_revision=?5"#,
            params![
                decision_id,
                selected_logical_action_id,
                decision.decision_revision,
                decision.delegation_revision,
                decision.policy_revision,
            ],
            |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                    row.get(10)?,
                ))
            },
        )
        .optional()
        .context("failed reading exact dangerous leaf authority")?;
    let Some((
        exact_action_json,
        confirmed_identity,
        manifest_digest,
        payload_digest,
        payload_json,
        target_snapshot_json,
        connector_identity,
        connector_version,
        action_envelope_json,
        manifest_json,
        effective_authority_json,
    )) = alternative
    else {
        return Ok(None);
    };
    if connector_identity != EXACT_OVERWRITE_TOOL_ID
        || connector_version != EXACT_OVERWRITE_TOOL_VERSION
    {
        return Ok(None);
    }
    if manifest_digest != decision.manifest_digest
        || format!("{:x}", Sha256::digest(manifest_json.as_bytes())) != manifest_digest
    {
        bail!("exact-overwrite manifest digest binding is invalid");
    }
    let manifest = parse_strict_json(&manifest_json)?;
    if manifest.to_bytes() != manifest_json.as_bytes() {
        bail!("exact-overwrite manifest is not canonical JSON");
    }
    let manifest_object = canonical_value_object(&manifest, "exact-overwrite manifest")?;
    if !matches!(manifest_object.get("schema"), Some(CanonicalValue::String(schema)) if schema == "carsinos.execass.leaf_action_manifest.v2")
        || manifest_object.len() != 2
    {
        bail!("exact-overwrite manifest contract is invalid");
    }
    let leaves = match manifest_object.get("leaves") {
        Some(CanonicalValue::Array(leaves)) => leaves,
        _ => bail!("exact-overwrite manifest has no leaves"),
    };
    let mut matching = leaves.iter().filter(|leaf| {
        canonical_value_object(leaf, "leaf")
            .ok()
            .and_then(|leaf| leaf.get("logical_action_id"))
            .is_some_and(|value| matches!(value, CanonicalValue::String(id) if id == selected_logical_action_id))
    });
    let leaf = matching
        .next()
        .context("exact-overwrite selected leaf is absent from its manifest")?;
    if matching.next().is_some() || leaf.to_bytes() != exact_action_json.as_bytes() {
        bail!("exact-overwrite selected leaf is ambiguous or not exact");
    }
    let leaf = canonical_value_object(leaf, "exact-overwrite leaf")?;
    if !matches!(leaf.get("action_kind"), Some(CanonicalValue::String(kind)) if kind == EXACT_OVERWRITE_ACTION_KIND)
    {
        bail!("exact-overwrite leaf action kind is invalid");
    }
    let tool = leaf
        .get("tool")
        .context("exact-overwrite leaf has no tool")?;
    let tool = canonical_value_object(tool, "exact-overwrite tool")?;
    if tool.len() != 2
        || !matches!(tool.get("tool_id"), Some(CanonicalValue::String(id)) if id == EXACT_OVERWRITE_TOOL_ID)
        || !matches!(tool.get("version"), Some(CanonicalValue::String(version)) if version == EXACT_OVERWRITE_TOOL_VERSION)
    {
        bail!("exact-overwrite leaf tool tuple is invalid");
    }
    let payload = parse_strict_json(&payload_json)?;
    let payload_object = canonical_value_object(&payload, "exact-overwrite payload")?;
    if payload.to_bytes() != payload_json.as_bytes()
        || payload_object.get("operands") != leaf.get("operands")
        || payload_object.get("target_snapshot") != leaf.get("target_snapshot")
        || payload_object.get("material_digest") != leaf.get("material_digest")
        || leaf
            .get("target_snapshot")
            .map(CanonicalValue::to_bytes)
            .as_deref()
            != Some(target_snapshot_json.as_bytes())
        || format!("{:x}", Sha256::digest(payload_json.as_bytes())) != payload_digest
    {
        bail!("exact-overwrite payload is not the selected canonical leaf material");
    }
    let recorder_payload_digest = exact_overwrite_envelope_payload_digest(&payload_json)?;
    let envelope = parse_strict_json(&action_envelope_json)?;
    let envelope_object = canonical_value_object(&envelope, "exact-overwrite action envelope")?;
    if envelope.to_bytes() != action_envelope_json.as_bytes()
        || envelope_object.len() != 3
        || !matches!(envelope_object.get("action_kind"), Some(CanonicalValue::String(kind)) if kind == EXACT_OVERWRITE_ACTION_KIND)
        || !matches!(envelope_object.get("mode"), Some(CanonicalValue::String(mode)) if mode == "exact")
        || !matches!(envelope_object.get("payload_and_material_operands_digest"), Some(CanonicalValue::String(digest)) if digest == &payload_digest)
    {
        bail!("exact-overwrite action envelope is not canonically payload-bound");
    }
    let operands = parse_exact_overwrite_payload(&payload_json)?;
    let reconciliation_key = exact_overwrite_reconciliation_key(&operands)?;
    let action: (String, i64, i64, i64, ActionBranchKind) = conn
        .query_row(
            r#"SELECT action_id,target_delegation_revision,target_plan_revision,stop_epoch,branch_kind
               FROM execass_action_branches
               WHERE delegation_id=?1 AND action_id=?2 AND target_delegation_revision=?3
                 AND target_plan_revision=?4 AND status='waiting'"#,
            params![
                decision.delegation_id,
                selected_logical_action_id,
                decision.delegation_revision,
                decision.plan_revision,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .context("exact-overwrite decision has no exact waiting action")?;
    let revision = decision.decision_revision.to_string();
    let delegation_revision = decision.delegation_revision.to_string();
    let plan_revision = decision.plan_revision.to_string();
    let continuation_identity = exact_overwrite_derived_identity(
        "continuation",
        &[
            &decision.decision_id,
            &revision,
            &decision.delegation_id,
            &delegation_revision,
            &plan_revision,
            &action.0,
            &confirmed_identity,
            &manifest_digest,
            &payload_digest,
        ],
    );
    let continuation_id = format!("decision-continuation-{continuation_identity}");
    let effect_identity = exact_overwrite_derived_identity(
        "logical-effect",
        &[
            &decision.decision_id,
            &continuation_id,
            &confirmed_identity,
            &manifest_digest,
            &payload_digest,
        ],
    );
    let logical_effect_id = format!("exact-overwrite-effect-{effect_identity}");
    let continuation = ContinuationRecord {
        continuation_id: continuation_id.clone(),
        delegation_id: decision.delegation_id.clone(),
        target_delegation_revision: action.1,
        target_plan_revision: action.2,
        action_id: action.0.clone(),
        branch_kind: action.4,
        causation_kind: ContinuationCausationKind::Decision,
        causation_id: decision.decision_id.clone(),
        status: ContinuationStatus::Runnable,
        job_id: None,
        lease_owner: None,
        lease_expires_at: None,
        fencing_token: 0,
        host_generation: runtime_host_generation,
        stop_epoch: action.3,
        global_stop_epoch,
        created_at: occurred_at,
        updated_at: occurred_at,
        completed_at: None,
    };
    let effect = PlannedLogicalEffectRecord {
        logical_effect_id: logical_effect_id.clone(),
        delegation_id: decision.delegation_id.clone(),
        continuation_id,
        action_kind: LogicalEffectActionKind::IrreversibleOrDestructiveAction,
        operation_reversible: false,
        declared_recovery_safe_boundary: DeclaredRecoverySafeBoundary::IndependentAbsence,
        internal_idempotency_key: format!(
            "exact-overwrite-internal-{}",
            exact_overwrite_derived_identity(
                "internal-idempotency",
                &[&effect_identity, &payload_digest]
            )
        ),
        provider_identity: Some(EXACT_OVERWRITE_PROVIDER_IDENTITY.into()),
        provider_idempotency_key: None,
        reconciliation_key: Some(reconciliation_key),
        manifest_digest: manifest_digest.clone(),
        payload_digest: recorder_payload_digest,
        created_at: occurred_at,
    };
    let authority_digest = carsinos_core::execass_policy::technical_effective_authority_digest(
        &effective_authority_json,
    )
    .map_err(|detail| anyhow::anyhow!("invalid exact-overwrite technical authority: {detail}"))?;
    let quota_unit = format!(
        "connector:{:x}",
        Sha256::digest(
            format!("{EXACT_OVERWRITE_TOOL_ID}\0{EXACT_OVERWRITE_TOOL_VERSION}").as_bytes()
        )
    );
    let snapshot = carsinos_core::execass_policy::compile_technical_quota_snapshot(
        &decision.delegation_id,
        decision.policy_revision,
        &authority_digest,
        "delegation",
        vec![carsinos_core::execass_policy::TechnicalQuotaEntryInput {
            kind: carsinos_core::execass_policy::TechnicalResourceKind::ConnectorCalls,
            unit: quota_unit.clone(),
            limit: 1,
        }],
    )
    .map_err(|detail| anyhow::anyhow!("invalid exact-overwrite quota snapshot: {detail}"))?;
    let requirements = carsinos_core::execass_policy::compile_technical_resource_requirements(
        &snapshot,
        &logical_effect_id,
        &action.0,
        &manifest_digest,
        vec![
            carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                kind: carsinos_core::execass_policy::TechnicalResourceKind::ConnectorCalls,
                unit: quota_unit,
                amount: 1,
            },
        ],
    )
    .map_err(|detail| anyhow::anyhow!("invalid exact-overwrite resource requirements: {detail}"))?;
    Ok(Some(PreparedExactDangerousEffect {
        continuation,
        logical_effect: effect,
        technical_quota_snapshot: snapshot,
        technical_resource_requirements: requirements,
    }))
}

pub(super) fn promote_decision_action_to_runnable(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    continuation: &ContinuationRecord,
) -> Result<()> {
    let changed = conn.execute(
        "UPDATE execass_action_branches SET status='runnable',updated_at=?1 WHERE action_id=?2 AND delegation_id=?3 AND status='waiting' AND target_delegation_revision=?4 AND target_plan_revision=?5 AND stop_epoch=?6",
        params![
            command.write.occurred_at,
            continuation.action_id,
            continuation.delegation_id,
            continuation.target_delegation_revision,
            continuation.target_plan_revision,
            continuation.stop_epoch,
        ],
    )?;
    if changed != 1 {
        bail!("decision continuation lost its waiting-to-runnable action race");
    }
    Ok(())
}

pub(super) fn validate_static_command(command: &AtomicDecisionResolutionCommand) -> Result<()> {
    require_text("decision_id", &command.decision_id)?;
    validate_outbox(&command.outbox_event)?;
    if command.decision_revision <= 0
        || command.write.occurred_at <= 0
        || command.outbox_event.event_name != OutboxEventName::DecisionRecorded
        || command.outbox_event.causation_id != command.decision_id
        || command.outbox_event.duplicate_identity != command.write.idempotency_key
        || command.outbox_event.correlation_id != command.write.correlation_id
        || command.outbox_event.occurred_at != command.write.occurred_at
        || command.write.causation_id != command.decision_id
        || command.receipt.receipt_kind != ReceiptKind::Decision
        || command.receipt.subject.kind != ReceiptSubjectKind::Decision
        || command.receipt.subject.subject_id != command.decision_id
        || command.receipt.subject.revision != command.decision_revision
        || command.receipt.causation_id != command.decision_id
        || command.receipt.causation_event_id != command.outbox_event.event_id
        || command.receipt.occurred_at != command.write.occurred_at
        || command.receipt.committed_at < command.write.occurred_at
    {
        bail!("atomic decision write, outbox, and receipt identities do not match");
    }
    if command.result != DecisionResult::ConfirmAndContinue
        && (command.continuation.is_some() || command.logical_effect.is_some())
    {
        bail!("non-affirmative decision results cannot create continuation or effect");
    }
    if command.logical_effect.is_some() && command.continuation.is_none() {
        bail!("a planned decision effect requires its exact continuation");
    }
    if command.logical_effect.is_some() != command.technical_quota_snapshot.is_some() {
        bail!("a planned decision effect requires exactly one technical quota snapshot");
    }
    if command.logical_effect.is_some() != command.technical_resource_requirements.is_some() {
        bail!("a planned decision effect requires exactly one technical resource requirement set");
    }
    if let Some(continuation) = &command.continuation {
        validate_continuation(continuation)?;
        if continuation.causation_kind != ContinuationCausationKind::Decision
            || continuation.causation_id != command.decision_id
            || continuation.status != ContinuationStatus::Runnable
            || continuation.job_id.is_some()
            || continuation.lease_owner.is_some()
            || continuation.lease_expires_at.is_some()
            || continuation.fencing_token != 0
            || continuation.completed_at.is_some()
            || continuation.created_at != command.write.occurred_at
            || continuation.updated_at != command.write.occurred_at
            || command.selected_logical_action_id.as_deref()
                != Some(continuation.action_id.as_str())
        {
            bail!("decision continuation is not the exact new runnable target");
        }
    }
    Ok(())
}

fn validate_live_resolution(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    decision: &DecisionRecord,
    authority: &carsinos_core::execass_manifest::CanonicalOwnerAuthority,
) -> Result<()> {
    if command.outbox_event.aggregate_id != decision.delegation_id
        || command.outbox_event.aggregate_revision != decision.delegation_revision
        || command.receipt.delegation_id != decision.delegation_id
        || command.receipt.expected_state_revision != decision.delegation_revision
    {
        bail!("decision resolution does not target its exact delegation revision");
    }
    if authority.authority_kind() != "decision_resolution"
        || authority.bound_decision_id() != Some(decision.decision_id.as_str())
        || authority.bound_decision_revision() != Some(decision.decision_revision)
        || authority
            .bound_manifest_digest()
            .map(|digest| digest.as_hex())
            != Some(decision.manifest_digest.as_str())
        || authority.policy_revision() != decision.policy_revision
        || authority.created_at() < decision.requested_at
        || authority.created_at() > command.write.occurred_at
        || authority
            .expires_at()
            .is_some_and(|expiry| expiry <= command.write.occurred_at)
    {
        bail!("owner authority is not bound to this exact pending decision");
    }
    let normalized_intent: String = conn.query_row(
        "SELECT normalized_original_intent FROM execass_delegations WHERE delegation_id=?1 AND state_revision=?2",
        params![decision.delegation_id, decision.delegation_revision],
        |row| row.get(0),
    )?;
    if owner_normalized_intent_digest(&normalized_intent).as_deref()
        != Some(authority.normalized_intent_digest().as_hex())
    {
        bail!("owner authority intent is not the decision delegation intent");
    }
    if decision.decision_kind == DecisionKind::DangerousActionConfirmation {
        if command.result == DecisionResult::ConfirmAndContinue {
            bail!("dangerous confirmation requires the signed attestation path");
        }
        if !matches!(
            command.result,
            DecisionResult::Revise | DecisionResult::Decline
        ) {
            bail!("dangerous confirmation supports only confirm, revise, or decline");
        }
        let selected = command
            .selected_logical_action_id
            .as_deref()
            .context("dangerous non-affirmative result must identify the presented action")?;
        let challenge: (String, String, i64) = conn.query_row(
            "SELECT challenge_id,nonce_digest,expires_at FROM execass_confirmation_challenges WHERE decision_id=?1 AND status='pending'",
            params![decision.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if authority
            .bound_challenge_nonce_digest()
            .map(|digest| digest.as_hex())
            != Some(challenge.1.as_str())
            || authority.expires_at() != Some(challenge.2)
            || command.write.occurred_at >= challenge.2
            || conn
                .query_row(
                    "SELECT 1 FROM execass_confirmation_challenge_alternatives WHERE challenge_id=?1 AND logical_action_id=?2",
                    params![challenge.0, selected],
                    |_| Ok(()),
                )
                .optional()?
                .is_none()
        {
            bail!("dangerous non-affirmative result is not bound to one live disclosed action");
        }
    } else if authority
        .bound_challenge_nonce_digest()
        .map(|digest| digest.as_hex())
        != owner_resolution_challenge_nonce_digest(decision.idempotency_key.as_bytes()).as_deref()
    {
        bail!("non-dangerous decision authority nonce is not the persisted decision identity");
    }
    validate_continuation_and_effect(conn, command, decision)
}

pub(super) fn validate_continuation_and_effect(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    decision: &DecisionRecord,
) -> Result<()> {
    if let Some(continuation) = &command.continuation {
        if continuation.delegation_id != decision.delegation_id
            || continuation.target_delegation_revision != decision.delegation_revision
            || continuation.target_plan_revision != decision.plan_revision
        {
            bail!("decision continuation revision binding is invalid");
        }
        let branch_matches = conn
            .query_row(
                r#"SELECT 1 FROM execass_action_branches WHERE delegation_id=?1 AND action_id=?2
                  AND target_delegation_revision=?3 AND target_plan_revision=?4 AND stop_epoch=?5
                  AND branch_kind=?6 AND status='waiting'"#,
                params![
                    decision.delegation_id,
                    continuation.action_id,
                    decision.delegation_revision,
                    decision.plan_revision,
                    continuation.stop_epoch,
                    continuation.branch_kind.as_str(),
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !branch_matches {
            bail!("decision continuation target is not one exact waiting action branch");
        }
    }
    match (
        &command.logical_effect,
        decision.decision_kind,
        command.result,
    ) {
        (Some(effect), DecisionKind::DuplicateRiskRetry, DecisionResult::ConfirmAndContinue) => {
            let continuation = command.continuation.as_ref().expect("validated above");
            let binding = load_duplicate_risk_binding(conn, &decision.decision_id)?
                .context("duplicate-risk confirm has no exact unresolved predecessor binding")?;
            if effect.delegation_id != decision.delegation_id
                || effect.continuation_id != continuation.continuation_id
                || effect.manifest_digest != decision.manifest_digest
                || effect.created_at != command.write.occurred_at
                || binding.delegation_id != decision.delegation_id
                || binding.confirmed_logical_action_identity
                    != decision.confirmed_logical_action_identity
            {
                bail!("duplicate-risk effect is not bound to the winning continuation");
            }
            let predecessor: (String, Option<String>, Option<String>, Option<String>) = conn
                .query_row(
                    r#"SELECT internal_idempotency_key,provider_identity,
                              provider_idempotency_key,reconciliation_key
                       FROM execass_logical_effects
                       WHERE delegation_id=?1 AND logical_effect_id=?2 AND state='outcome_unknown'"#,
                    params![
                        binding.delegation_id,
                        binding.predecessor_logical_effect_id
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .context("duplicate-risk predecessor is no longer unresolved")?;
            if effect.internal_idempotency_key == predecessor.0
                || effect.provider_identity != predecessor.1
                || (predecessor.1.is_some()
                    && (effect.provider_idempotency_key.is_none()
                        || effect.reconciliation_key.is_none()
                        || effect.provider_idempotency_key == predecessor.2
                        || effect.reconciliation_key == predecessor.3))
                || (predecessor.1.is_none()
                    && (effect.provider_idempotency_key.is_some()
                        || effect.reconciliation_key.is_some()))
            {
                bail!("duplicate-risk successor does not have distinct stable effect identity");
            }
        }
        (
            Some(effect),
            DecisionKind::DangerousActionConfirmation,
            DecisionResult::ConfirmAndContinue,
        ) => {
            let continuation = command.continuation.as_ref().expect("validated above");
            let expected = prepare_exact_dangerous_effect_from_conn(
                conn,
                &decision.decision_id,
                &continuation.action_id,
                command.write.occurred_at,
                continuation.host_generation,
                continuation.global_stop_epoch,
            )?
            .context("only the installed exact-overwrite dangerous leaf may create an effect")?;
            if continuation != &expected.continuation
                || effect != &expected.logical_effect
                || command.technical_quota_snapshot.as_ref()
                    != Some(&expected.technical_quota_snapshot)
                || command.technical_resource_requirements.as_ref()
                    != Some(&expected.technical_resource_requirements)
            {
                bail!("dangerous exact-overwrite effect is not the storage-derived shape");
            }
        }
        (Some(_), _, _) => {
            bail!("only duplicate-risk or the installed exact-overwrite confirm may create a new logical effect")
        }
        (None, DecisionKind::DuplicateRiskRetry, DecisionResult::ConfirmAndContinue) => {
            bail!("duplicate-risk confirm must create exactly one new logical effect")
        }
        (None, DecisionKind::DangerousActionConfirmation, DecisionResult::ConfirmAndContinue) => {
            if let Some(continuation) = command.continuation.as_ref() {
                if prepare_exact_dangerous_effect_from_conn(
                    conn,
                    &decision.decision_id,
                    &continuation.action_id,
                    command.write.occurred_at,
                    continuation.host_generation,
                    continuation.global_stop_epoch,
                )?
                .is_some()
                {
                    bail!("installed exact-overwrite confirmation must create its logical effect");
                }
            }
        }
        (None, _, _) => {}
    }
    validate_technical_resource_binding(conn, command, decision)?;
    Ok(())
}

fn load_duplicate_risk_binding(
    conn: &Connection,
    decision_id: &str,
) -> Result<Option<DuplicateRiskBindingRecord>> {
    conn.query_row(
        r#"SELECT decision_id,delegation_id,predecessor_logical_effect_id,
                  predecessor_attempt_id,predecessor_uncertainty_evidence_digest,
                  confirmed_logical_action_identity,accepted_confirmation_grant_id,created_at
           FROM execass_duplicate_risk_bindings WHERE decision_id=?1"#,
        params![decision_id],
        |row| {
            Ok(DuplicateRiskBindingRecord {
                decision_id: row.get(0)?,
                delegation_id: row.get(1)?,
                predecessor_logical_effect_id: row.get(2)?,
                predecessor_attempt_id: row.get(3)?,
                predecessor_uncertainty_evidence_digest: row.get(4)?,
                confirmed_logical_action_identity: row.get(5)?,
                accepted_confirmation_grant_id: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    )
    .optional()
    .context("failed reading duplicate-risk predecessor binding")
}

fn insert_duplicate_risk_successor(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    decision: &DecisionRecord,
    effect: &PlannedLogicalEffectRecord,
) -> Result<()> {
    if decision.decision_kind != DecisionKind::DuplicateRiskRetry
        || command.result != DecisionResult::ConfirmAndContinue
    {
        return Ok(());
    }
    let binding = load_duplicate_risk_binding(conn, &decision.decision_id)?
        .context("duplicate-risk successor has no predecessor binding")?;
    conn.execute(
        r#"INSERT INTO execass_duplicate_risk_successors(
             decision_id,predecessor_logical_effect_id,predecessor_attempt_id,
             successor_logical_effect_id,predecessor_uncertainty_evidence_digest,created_at
           ) VALUES(?1,?2,?3,?4,?5,?6)"#,
        params![
            binding.decision_id,
            binding.predecessor_logical_effect_id,
            binding.predecessor_attempt_id,
            effect.logical_effect_id,
            binding.predecessor_uncertainty_evidence_digest,
            command.write.occurred_at,
        ],
    )?;
    Ok(())
}

fn validate_technical_resource_binding(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    decision: &DecisionRecord,
) -> Result<()> {
    let Some(snapshot) = &command.technical_quota_snapshot else {
        return Ok(());
    };
    let effect = command
        .logical_effect
        .as_ref()
        .context("technical quota snapshot has no logical effect")?;
    let continuation = command
        .continuation
        .as_ref()
        .context("technical quota snapshot has no continuation")?;
    let requirements = command
        .technical_resource_requirements
        .as_ref()
        .context("technical quota snapshot has no technical resource requirements")?;
    let effective_authority_json: String = conn.query_row(
        "SELECT effective_authority_json FROM execass_delegations WHERE delegation_id=?1 AND state_revision=?2 AND policy_revision=?3",
        params![decision.delegation_id, decision.delegation_revision, decision.policy_revision],
        |row| row.get(0),
    )?;
    let authority_digest = carsinos_core::execass_policy::technical_effective_authority_digest(
        &effective_authority_json,
    )
    .map_err(|detail| anyhow::anyhow!("invalid technical authority snapshot: {detail}"))?;
    let rebuilt = carsinos_core::execass_policy::compile_technical_quota_snapshot(
        &snapshot.delegation_id,
        snapshot.policy_revision,
        &snapshot.effective_authority_digest,
        &snapshot.scope_key,
        snapshot
            .entries
            .iter()
            .map(
                |entry| carsinos_core::execass_policy::TechnicalQuotaEntryInput {
                    kind: entry.kind,
                    unit: entry.unit.clone(),
                    limit: entry.limit,
                },
            )
            .collect(),
    )
    .map_err(|detail| anyhow::anyhow!("invalid canonical technical quota snapshot: {detail}"))?;
    if rebuilt != *snapshot
        || snapshot.delegation_id != decision.delegation_id
        || snapshot.policy_revision != decision.policy_revision
        || snapshot.effective_authority_digest != authority_digest
        || snapshot.scope_key != "delegation"
        || effect.continuation_id != continuation.continuation_id
    {
        bail!("technical quota snapshot is not bound to the exact delegation policy authority");
    }
    let rebuilt_requirements =
        carsinos_core::execass_policy::compile_technical_resource_requirements(
            snapshot,
            &requirements.logical_effect_id,
            &requirements.action_id,
            &requirements.manifest_digest,
            requirements
                .requirements
                .iter()
                .map(|requirement| {
                    carsinos_core::execass_policy::TechnicalResourceRequirementInput {
                        kind: requirement.kind,
                        unit: requirement.unit.clone(),
                        amount: requirement.amount,
                    }
                })
                .collect(),
        )
        .map_err(|detail| {
            anyhow::anyhow!("invalid canonical technical resource requirements: {detail}")
        })?;
    if rebuilt_requirements != *requirements
        || requirements.quota_snapshot_id != snapshot.quota_snapshot_id
        || requirements.delegation_id != decision.delegation_id
        || requirements.logical_effect_id != effect.logical_effect_id
        || Some(requirements.action_id.as_str()) != command.selected_logical_action_id.as_deref()
        || requirements.action_id != continuation.action_id
        || requirements.manifest_digest != decision.manifest_digest
        || requirements.manifest_digest != effect.manifest_digest
    {
        bail!("technical resource requirements are not bound to the exact decision effect");
    }
    Ok(())
}

fn resolve_pending_decision(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    decision: &DecisionRecord,
    authority_id: &str,
) -> Result<()> {
    if decision.decision_kind == DecisionKind::DangerousActionConfirmation {
        let selected = command
            .selected_logical_action_id
            .as_deref()
            .context("dangerous result has no selected action")?;
        let changed = conn.execute(
            "UPDATE execass_confirmation_challenges SET selected_logical_action_id=?2,status='resolved',resolved_at=?3 WHERE decision_id=?1 AND status='pending'",
            params![decision.decision_id, selected, command.write.occurred_at],
        )?;
        if changed != 1 {
            bail!("dangerous decision challenge lost its resolution race");
        }
    }
    let changed = conn.execute(
        "UPDATE execass_decisions SET status='resolved',result=?2,resolved_at=?3,resolved_by_authority_provenance_id=?4 WHERE decision_id=?1 AND status='pending'",
        params![decision.decision_id, command.result.as_str(), command.write.occurred_at, authority_id],
    )?;
    if changed != 1 {
        bail!("decision lost its resolution race");
    }
    Ok(())
}

pub(super) fn load_decision(
    conn: &Connection,
    decision_id: &str,
) -> Result<Option<DecisionRecord>> {
    conn.query_row(
        r#"SELECT decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,
          policy_revision,decision_kind,status,result,confirmed_logical_action_identity,
          manifest_digest,idempotency_key,requested_at,resolved_at,resolved_by_authority_provenance_id
          FROM execass_decisions WHERE decision_id=?1"#,
        params![decision_id],
        |row| {
            Ok(DecisionRecord {
                decision_id: row.get(0)?,
                delegation_id: row.get(1)?,
                decision_revision: row.get(2)?,
                delegation_revision: row.get(3)?,
                plan_revision: row.get(4)?,
                policy_revision: row.get(5)?,
                decision_kind: row.get(6)?,
                status: row.get(7)?,
                result: row.get(8)?,
                confirmed_logical_action_identity: row.get(9)?,
                manifest_digest: row.get(10)?,
                idempotency_key: row.get(11)?,
                requested_at: row.get(12)?,
                resolved_at: row.get(13)?,
                resolved_by_authority_provenance_id: row.get(14)?,
            })
        },
    )
    .optional()
    .context("failed reading ExecAss decision")
}

pub(super) fn load_replay_bundle(
    conn: &Connection,
    command: &AtomicDecisionResolutionCommand,
    decision: &DecisionRecord,
) -> Result<Option<AtomicDecisionResolutionBundle>> {
    if decision.result != Some(command.result) {
        return Ok(None);
    }
    if decision.decision_kind == DecisionKind::DangerousActionConfirmation {
        let selected: Option<String> = conn
            .query_row(
                "SELECT selected_logical_action_id FROM execass_confirmation_challenges WHERE decision_id=?1 AND status='resolved'",
                params![decision.decision_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if selected.as_deref() != command.selected_logical_action_id.as_deref() {
            return Ok(None);
        }
    } else if command.selected_logical_action_id.is_some()
        && command
            .continuation
            .as_ref()
            .map(|item| item.action_id.as_str())
            != command.selected_logical_action_id.as_deref()
    {
        return Ok(None);
    }
    let Some(outbox_event) = get_outbox(conn, &command.outbox_event.event_id)? else {
        return Ok(None);
    };
    if outbox_event.event != command.outbox_event {
        return Ok(None);
    }
    let Some(receipt) = receipt_by_causation_event(conn, &command.outbox_event.event_id)? else {
        return Ok(None);
    };
    if receipt.receipt_id != command.receipt.receipt_id {
        return Ok(None);
    }
    let continuation = match &command.continuation {
        Some(expected) => match get_continuation(conn, &expected.continuation_id)? {
            Some(found) if found == *expected => Some(found),
            _ => return Ok(None),
        },
        None => {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT continuation_id FROM execass_continuations WHERE causation_kind='decision' AND causation_id=?1",
                    params![decision.decision_id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing.is_some() {
                return Ok(None);
            }
            None
        }
    };
    let logical_effect = match &command.logical_effect {
        Some(expected) => match get_planned_logical_effect(conn, &expected.logical_effect_id)? {
            Some(found) if found == *expected => {
                if decision.decision_kind == DecisionKind::DuplicateRiskRetry {
                    let exact_link = conn
                        .query_row(
                            r#"SELECT 1 FROM execass_duplicate_risk_successors s
                               JOIN execass_duplicate_risk_bindings b ON b.decision_id=s.decision_id
                               WHERE s.decision_id=?1
                                 AND s.successor_logical_effect_id=?2
                                 AND s.predecessor_logical_effect_id=b.predecessor_logical_effect_id
                                 AND s.predecessor_attempt_id=b.predecessor_attempt_id
                                 AND s.predecessor_uncertainty_evidence_digest=b.predecessor_uncertainty_evidence_digest"#,
                            params![decision.decision_id, found.logical_effect_id],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                    if !exact_link {
                        return Ok(None);
                    }
                }
                Some(found)
            }
            _ => return Ok(None),
        },
        None => None,
    };
    let technical_resource_requirements =
        match (&command.technical_resource_requirements, &logical_effect) {
            (Some(expected), Some(effect)) => {
                let Some(found) = get_technical_resource_requirements_for_effect(
                    conn,
                    &effect.logical_effect_id,
                )?
                else {
                    return Ok(None);
                };
                if !technical_requirement_record_matches(expected, &found, command) {
                    return Ok(None);
                }
                Some(found)
            }
            (None, None) => None,
            _ => return Ok(None),
        };
    let technical_quota_snapshot = match (
        &command.technical_quota_snapshot,
        &technical_resource_requirements,
    ) {
        (Some(expected), Some(requirements)) => {
            let Some(found) = get_technical_quota_snapshot(conn, &requirements.quota_snapshot_id)?
            else {
                return Ok(None);
            };
            if !technical_snapshot_record_matches(expected, &found) {
                return Ok(None);
            }
            Some(found)
        }
        (None, None) => None,
        _ => return Ok(None),
    };
    Ok(Some(AtomicDecisionResolutionBundle {
        decision: decision.clone(),
        confirmation_grant: None,
        continuation,
        logical_effect,
        technical_quota_snapshot,
        technical_resource_requirements,
        outbox_event,
        receipt,
    }))
}

fn technical_snapshot_record_matches(
    expected: &carsinos_core::execass_policy::CanonicalTechnicalQuotaSnapshot,
    found: &TechnicalQuotaSnapshotRecord,
) -> bool {
    found.quota_snapshot_id == expected.quota_snapshot_id
        && found.delegation_id == expected.delegation_id
        && found.policy_revision == expected.policy_revision
        && found.effective_authority_digest == expected.effective_authority_digest
        && found.scope_key == expected.scope_key
        && found.canonical_entries_json == expected.canonical_entries_json
        && found.canonical_entries_digest == expected.canonical_entries_digest
        && found.entries.len() == expected.entries.len()
        && found
            .entries
            .iter()
            .zip(&expected.entries)
            .all(|(left, right)| {
                left.quota_snapshot_id == expected.quota_snapshot_id
                    && left.technical_resource_kind.as_str() == right.kind.as_str()
                    && left.unit == right.unit
                    && left.amount_limit == right.limit
            })
}

fn technical_requirement_record_matches(
    expected: &carsinos_core::execass_policy::CanonicalTechnicalResourceRequirementSet,
    found: &TechnicalResourceRequirementSetRecord,
    command: &AtomicDecisionResolutionCommand,
) -> bool {
    found.requirement_set_id == expected.requirement_set_id
        && found.quota_snapshot_id == expected.quota_snapshot_id
        && found.delegation_id == expected.delegation_id
        && found.logical_effect_id == expected.logical_effect_id
        && found.action_id == expected.action_id
        && found.manifest_digest == expected.manifest_digest
        && found.canonical_requirements_json == expected.canonical_requirements_json
        && found.canonical_requirements_digest == expected.canonical_requirements_digest
        && found.created_at == command.write.occurred_at
        && found.requirements.len() == expected.requirements.len()
        && found
            .requirements
            .iter()
            .zip(&expected.requirements)
            .all(|(left, right)| {
                left.requirement_set_id == expected.requirement_set_id
                    && left.quota_snapshot_id == expected.quota_snapshot_id
                    && left.technical_resource_kind.as_str() == right.kind.as_str()
                    && left.unit == right.unit
                    && left.amount_required == right.amount
            })
}
