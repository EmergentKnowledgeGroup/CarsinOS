#[cfg(test)]
use super::confirmation_attestation::{
    confirmation_attestation_signing_bytes, ConfirmationAttestationPayload,
};
use super::confirmation_attestation::{
    confirmation_verifying_key_digest_hex, verify_confirmation_attestation,
    ConfirmationAttestation, PinnedConfirmationAttestationKey, VerifiedConfirmationAttestation,
};
use super::foundation::authority_record_from_manifest;
use super::receipt::{AtomicReceiptMutation, AtomicReceiptWriteOutcome};
use super::rows::{
    get_outbox, get_technical_quota_snapshot, get_technical_resource_requirements_for_effect,
    insert_authority, insert_continuation, insert_outbox, insert_planned_logical_effect,
    insert_technical_quota_snapshot, insert_technical_resource_requirements,
};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use carsinos_core::execass_actor::{
    owner_normalized_intent_digest, owner_resolution_challenge_nonce_digest,
    owner_resolution_manifest_digest, VerifiedOwnerAuthority,
};
use carsinos_core::execass_danger::{
    saved_routine_stable_leaf_digest, DangerAdmissionState, DangerRoute,
    SignedDangerAdmissionProof, VerifiedSavedRoutineSelector,
};
use carsinos_core::execass_manifest::{
    canonicalize_owner_authority, CanonicalLeafAction, CanonicalLeafManifest,
};
use ed25519_dalek::{Signature, VerifyingKey};
use rusqlite::{params, types::Type, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use ed25519_dalek::{Signer, SigningKey};

#[derive(Debug, Clone)]
struct ExactConfirmationBinding {
    normalized_intent: String,
    exact_presented_action_json: String,
    confirmed_logical_action_identity: String,
    manifest_digest: String,
    payload_digest: String,
    payload_and_material_operands_json: String,
    target_audience_path_json: String,
    connector_tool_identity: String,
    connector_tool_version: String,
    canonical_action_envelope_or_selector_json: String,
    declared_consequence: String,
}

#[derive(Debug)]
enum AtomicDangerMutation {
    Applied(Box<AtomicDangerApplied>),
    Replayed(Box<AtomicDecisionResolutionBundle>),
    Conflict(Option<DecisionResult>),
    NotFound,
}

#[derive(Debug)]
struct AtomicDangerApplied {
    decision: DecisionRecord,
    grant: AcceptedConfirmationGrantRecord,
    continuation: Option<ContinuationRecord>,
    logical_effect: Option<PlannedLogicalEffectRecord>,
    technical_quota_snapshot: Option<TechnicalQuotaSnapshotRecord>,
    technical_resource_requirements: Option<TechnicalResourceRequirementSetRecord>,
    outbox_event: OutboxEventRecord,
}

#[derive(Debug)]
struct ActiveConfirmationKey {
    key_id: String,
    key_generation: u64,
    verifying_key_hex: String,
    verifying_key_digest: String,
    canonical_root_identity: String,
    installation_identity: String,
    os_user_identity_digest: String,
    state_root_generation: u64,
}

#[derive(Debug)]
struct CurrentConfirmationBinding {
    normalized_intent: String,
    delegation_policy_revision: i64,
    decision_policy_revision: i64,
    plan_policy_revision: i64,
    manifest_digest: String,
    decision_requested_at: i64,
}

#[derive(Debug)]
struct PersistedAttestationReplay {
    attestation_digest: String,
    selected_logical_action_id: String,
    signed_payload_json: String,
    signature_hex: String,
    verified_at: i64,
}

#[derive(Debug)]
struct ResolvedRuntimeProjectionRow {
    projection: ResolvedDangerConfirmationAlternativeBinding,
    attestation: ConfirmationAttestation,
    pinned_key_generation: u64,
    verified_at: i64,
}

impl ExecAssStore {
    /// Verify a gateway-sealed complete danger-routing result against the one
    /// active key pinned in canonical storage. Verification precedes every
    /// foundation write and revalidates the exact manifest after the signature
    /// passes; a caller-created unsigned or substituted proof has no authority.
    pub(super) fn verify_danger_admission(
        &self,
        signed: &SignedDangerAdmissionProof,
        manifest: &CanonicalLeafManifest,
    ) -> Result<DangerAdmissionState> {
        let conn = self.connection()?;
        verify_danger_admission_with_conn(&conn, signed, manifest)
    }

    pub(super) fn verify_danger_admission_in_tx(
        &self,
        tx: &Transaction<'_>,
        signed: &SignedDangerAdmissionProof,
        manifest: &CanonicalLeafManifest,
    ) -> Result<DangerAdmissionState> {
        verify_danger_admission_with_conn(tx, signed, manifest)
    }
}

fn verify_danger_admission_with_conn(
    conn: &Connection,
    signed: &SignedDangerAdmissionProof,
    manifest: &CanonicalLeafManifest,
) -> Result<DangerAdmissionState> {
    let pinned = load_active_confirmation_key(conn)?;
    if signed.key_id() != pinned.key_id
        || signed.key_generation() != pinned.key_generation
        || signed.canonical_root_identity() != pinned.canonical_root_identity
        || signed.installation_identity() != pinned.installation_identity
        || signed.os_user_identity_digest() != pinned.os_user_identity_digest
        || signed.state_root_generation() != pinned.state_root_generation
    {
        bail!("danger-admission authority does not match the active storage pin");
    }
    let verifying_key_bytes = decode_danger_hex::<32>(&pinned.verifying_key_hex)
        .context("active danger-admission verification key is malformed")?;
    let verifying_key = VerifyingKey::from_bytes(&verifying_key_bytes)
        .map_err(|_| anyhow::anyhow!("active danger-admission verification key is invalid"))?;
    if verifying_key.is_weak() || sha256_hex(&verifying_key_bytes) != pinned.verifying_key_digest {
        bail!("active danger-admission verification key failed integrity validation");
    }
    let signature_bytes = decode_danger_hex::<64>(signed.signature_hex())
        .context("danger-admission signature is malformed")?;
    let signature = Signature::from_bytes(&signature_bytes);
    let signing_bytes = signed
        .signing_bytes()
        .map_err(|_| anyhow::anyhow!("danger-admission signed fields are invalid"))?;
    verifying_key
        .verify_strict(&signing_bytes, &signature)
        .map_err(|_| anyhow::anyhow!("danger-admission signature is invalid"))?;
    signed
        .proof()
        .validate_for_manifest(manifest)
        .map_err(|error| anyhow::anyhow!("invalid gateway danger-admission proof: {error:?}"))
}

impl ExecAssStore {
    /// Append-only reconciliation for the configured Telegram/Discord owner.
    /// The newest row for each provider is authoritative; a `retired` tombstone
    /// makes removal effective without mutating or deleting historical proof.
    pub fn reconcile_remote_confirmation_ingress(
        &self,
        configured: &[RemoteOwnerConfirmationIngress],
        observed_at: i64,
    ) -> Result<()> {
        if observed_at <= 0 {
            bail!("remote confirmation ingress reconciliation time must be positive");
        }
        let mut expected = std::collections::BTreeMap::new();
        for item in configured {
            let provider = item.provider.trim().to_ascii_lowercase();
            let owner_account_id = item.owner_account_id.trim();
            let authenticated_ingress = item.authenticated_ingress.trim();
            if !matches!(provider.as_str(), "telegram" | "discord")
                || owner_account_id.is_empty()
                || authenticated_ingress.is_empty()
                || expected
                    .insert(
                        provider.clone(),
                        (
                            format!("{provider}:{owner_account_id}"),
                            authenticated_ingress.to_string(),
                            format!("authenticated-{provider}-provider-event"),
                        ),
                    )
                    .is_some()
            {
                bail!("remote confirmation ingress configuration is invalid or duplicated");
            }
        }

        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        for provider in ["telegram", "discord"] {
            let assurance = format!("authenticated-{provider}-provider-event");
            let current = latest_remote_ingress_binding(&tx, &assurance)?;
            match (expected.get(provider), current) {
                (Some((credential, ingress, _)), Some(row))
                    if row.status == "active"
                        && row.credential_identity == *credential
                        && row.authenticated_ingress == *ingress => {}
                (Some((credential, ingress, _)), current) => {
                    require_newer_remote_binding_time(current.as_ref(), observed_at)?;
                    insert_remote_ingress_binding(
                        &tx,
                        provider,
                        credential,
                        ingress,
                        &assurance,
                        "active",
                        observed_at,
                    )?;
                }
                (None, Some(row)) if row.status == "active" => {
                    require_newer_remote_binding_time(Some(&row), observed_at)?;
                    insert_remote_ingress_binding(
                        &tx,
                        provider,
                        &row.credential_identity,
                        &format!("retired-{provider}-ingress"),
                        &assurance,
                        "retired",
                        observed_at,
                    )?;
                }
                (None, _) => {}
            }
        }
        tx.commit()
            .context("committing remote confirmation ingress reconciliation")
    }

    /// Load the exact live binding required by the gateway actor-assurance
    /// gate. Raw nonce bytes are never persisted or returned.
    pub fn read_pending_danger_confirmation_binding(
        &self,
        decision_id: &str,
    ) -> Result<Option<PendingDangerConfirmationBinding>> {
        self.read_pending_danger_confirmation_binding_at(decision_id, trusted_unix_time_ms()?)
    }

    #[cfg(test)]
    pub(super) fn read_pending_danger_confirmation_binding_at_for_test(
        &self,
        decision_id: &str,
        trusted_observed_at: i64,
    ) -> Result<Option<PendingDangerConfirmationBinding>> {
        self.read_pending_danger_confirmation_binding_at(decision_id, trusted_observed_at)
    }

    fn read_pending_danger_confirmation_binding_at(
        &self,
        decision_id: &str,
        trusted_observed_at: i64,
    ) -> Result<Option<PendingDangerConfirmationBinding>> {
        require_text("decision_id", decision_id)?;
        if trusted_observed_at <= 0 {
            bail!("observed time must be positive");
        }
        let conn = self.connection()?;
        conn.query_row(
            "SELECT c.delegation_id,e.normalized_original_intent,d.policy_revision,c.decision_id,c.decision_revision,p.resolved_leaf_manifest_json,c.manifest_digest,c.exact_presented_action_json,c.declared_consequence,d.alternatives_json,c.nonce_digest,d.requested_at,c.expires_at FROM execass_confirmation_challenges c JOIN execass_decisions d ON d.decision_id=c.decision_id JOIN execass_delegations e ON e.delegation_id=c.delegation_id JOIN execass_plans p ON p.delegation_id=c.delegation_id AND p.manifest_digest=c.manifest_digest WHERE c.decision_id=?1 AND c.status='pending' AND d.status='pending' AND c.expires_at>?2 ORDER BY p.plan_revision DESC LIMIT 2",
            params![decision_id, trusted_observed_at],
            |row| {
                let exact_presented_action_json: String = row.get(7)?;
                let alternatives_json: String = row.get(9)?;
                let declared_consequence: String = row.get(8)?;
                let combined_question = combined_question_from_canonical_json(&alternatives_json)
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            9,
                            Type::Text,
                            Box::new(std::io::Error::other(error.to_string())),
                        )
                    })?;
                Ok(PendingDangerConfirmationBinding {
                    delegation_id: row.get(0)?,
                    normalized_intent: row.get(1)?,
                    policy_revision: row.get(2)?,
                    decision_id: row.get(3)?,
                    decision_revision: row.get(4)?,
                    canonical_manifest_json: row.get(5)?,
                    manifest_digest: row.get(6)?,
                    exact_presented_action_json: exact_presented_action_json.clone(),
                    exact_presented_action_digest: sha256_hex(exact_presented_action_json.as_bytes()),
                    declared_consequence,
                    combined_question,
                    challenge_nonce_digest: row.get(10)?,
                    requested_at: row.get(11)?,
                    expires_at: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Return one exact, disclosed, still-current alternative for the private
    /// signer. The caller names only the opaque logical action ID; all action,
    /// consequence, manifest, and challenge material comes from storage.
    pub fn read_pending_danger_confirmation_alternative_binding(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
    ) -> Result<Option<PendingDangerConfirmationAlternativeBinding>> {
        self.read_pending_danger_confirmation_alternative_binding_at(
            decision_id,
            selected_logical_action_id,
            trusted_unix_time_ms()?,
        )
    }

    /// Return the one immutable confirmation state a private runtime may
    /// consume. Pending state is the exact alternative eligible for a first
    /// signature; resolved state is a storage-validated original grant for an
    /// exact retry. This read never creates a row or exposes signing/replay
    /// material.
    pub fn read_danger_confirmation_runtime_projection(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
    ) -> Result<Option<DangerConfirmationRuntimeProjection>> {
        self.read_danger_confirmation_runtime_projection_at(
            decision_id,
            selected_logical_action_id,
            trusted_unix_time_ms()?,
        )
    }

    #[cfg(test)]
    pub(super) fn read_danger_confirmation_runtime_projection_at_for_test(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        trusted_observed_at: i64,
    ) -> Result<Option<DangerConfirmationRuntimeProjection>> {
        self.read_danger_confirmation_runtime_projection_at(
            decision_id,
            selected_logical_action_id,
            trusted_observed_at,
        )
    }

    fn read_danger_confirmation_runtime_projection_at(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        trusted_observed_at: i64,
    ) -> Result<Option<DangerConfirmationRuntimeProjection>> {
        if decision_id.trim().is_empty() || selected_logical_action_id.trim().is_empty() {
            return Ok(None);
        }
        if let Some(binding) = self.read_pending_danger_confirmation_alternative_binding_at(
            decision_id,
            selected_logical_action_id,
            trusted_observed_at,
        )? {
            return Ok(Some(DangerConfirmationRuntimeProjection::Pending(
                Box::new(binding),
            )));
        }
        self.read_resolved_danger_confirmation_runtime_projection(
            decision_id,
            selected_logical_action_id,
        )
    }

    fn read_resolved_danger_confirmation_runtime_projection(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
    ) -> Result<Option<DangerConfirmationRuntimeProjection>> {
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let pinned = load_active_confirmation_key(&tx)?;
        if pinned.canonical_root_identity != self.root_identity
            || confirmation_verifying_key_digest_hex(&pinned.verifying_key_hex)?
                != pinned.verifying_key_digest
        {
            bail!("active confirmation key is not bound to this canonical state root");
        }
        let pinned_key = PinnedConfirmationAttestationKey::from_hex(
            pinned.key_id.clone(),
            pinned.key_generation,
            &pinned.verifying_key_hex,
        )?;
        let row = tx.query_row(
            "SELECT c.delegation_id,e.normalized_original_intent,d.policy_revision,d.decision_id,d.decision_revision,p.resolved_leaf_manifest_json,c.manifest_digest,a.logical_action_id,a.exact_presented_action_json,a.declared_consequence,c.nonce_digest,d.requested_at,c.expires_at,g.grant_id,g.delegation_id,g.decision_id,g.confirmed_logical_action_identity,g.canonical_action_envelope_or_selector_json,g.payload_and_material_operands_json,g.payload_and_material_operands_digest,g.connector_tool_identity,g.connector_tool_version,g.declared_consequence,g.accepted_by_authority_provenance_id,g.confirmation_attestation_digest,g.accepted_at,g.invalidated_at,g.invalidation_reason,g.invalidated_by_authority_provenance_id,attestation.pinned_key_id,attestation.pinned_key_generation,attestation.signed_payload_json,attestation.signature_hex,attestation.verified_at FROM execass_confirmation_challenges c JOIN execass_decisions d ON d.decision_id=c.decision_id JOIN execass_delegations e ON e.delegation_id=c.delegation_id JOIN execass_plans p ON p.delegation_id=e.delegation_id AND p.manifest_digest=c.manifest_digest JOIN execass_confirmation_challenge_alternatives a ON a.challenge_id=c.challenge_id AND a.logical_action_id=c.selected_logical_action_id JOIN execass_confirmation_attestations attestation ON attestation.decision_id=c.decision_id AND attestation.selected_logical_action_id=c.selected_logical_action_id JOIN execass_accepted_confirmation_grants g ON g.decision_id=c.decision_id AND g.confirmation_attestation_digest=attestation.attestation_digest WHERE c.decision_id=?1 AND c.selected_logical_action_id=?2 AND c.status='resolved' AND d.status='resolved' AND d.result='confirm_and_continue' AND a.logical_action_id=?2 AND g.invalidated_at IS NULL AND g.delegation_id=c.delegation_id AND g.confirmed_logical_action_identity=a.confirmed_logical_action_identity AND g.declared_consequence=a.declared_consequence AND d.decision_revision=c.decision_revision AND d.manifest_digest=c.manifest_digest AND d.exact_presented_action_json=a.exact_presented_action_json AND d.confirmed_logical_action_identity=a.confirmed_logical_action_identity AND d.payload_digest=a.payload_digest AND d.payload_and_material_operands_json=a.payload_and_material_operands_json AND d.connector_tool_identity IS a.connector_tool_identity AND d.connector_tool_version IS a.connector_tool_version AND d.side_effect_envelope_json=a.canonical_action_envelope_or_selector_json AND d.consequence=a.declared_consequence ORDER BY p.plan_revision DESC LIMIT 1",
            params![decision_id, selected_logical_action_id],
            |row| {
                let action_json: String = row.get(8)?;
                let consequence: String = row.get(9)?;
                let signed_payload_json: String = row.get(31)?;
                let payload = serde_json::from_str(&signed_payload_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        31,
                        Type::Text,
                        Box::new(std::io::Error::other(error.to_string())),
                    )
                })?;
                Ok(ResolvedRuntimeProjectionRow {
                    projection: ResolvedDangerConfirmationAlternativeBinding {
                        binding: PendingDangerConfirmationAlternativeBinding {
                            delegation_id: row.get(0)?,
                            normalized_intent: row.get(1)?,
                            policy_revision: row.get(2)?,
                            decision_id: row.get(3)?,
                            decision_revision: row.get(4)?,
                            canonical_manifest_json: row.get(5)?,
                            manifest_digest: row.get(6)?,
                            selected_logical_action_id: row.get(7)?,
                            exact_selected_action_json: action_json.clone(),
                            exact_selected_action_digest: sha256_hex(action_json.as_bytes()),
                            declared_consequence: consequence.clone(),
                            declared_consequence_digest: sha256_hex(consequence.as_bytes()),
                            challenge_nonce_digest: row.get(10)?,
                            requested_at: row.get(11)?,
                            expires_at: row.get(12)?,
                        },
                        grant: AcceptedConfirmationGrantRecord {
                            grant_id: row.get(13)?,
                            delegation_id: row.get(14)?,
                            decision_id: row.get(15)?,
                            confirmed_logical_action_identity: row.get(16)?,
                            canonical_action_envelope_or_selector_json: row.get(17)?,
                            payload_and_material_operands_json: row.get(18)?,
                            payload_and_material_operands_digest: row.get(19)?,
                            connector_tool_identity: row.get(20)?,
                            connector_tool_version: row.get(21)?,
                            declared_consequence: row.get(22)?,
                            accepted_by_authority_provenance_id: row.get(23)?,
                            confirmation_attestation_digest: row.get(24)?,
                            accepted_at: row.get(25)?,
                            invalidated_at: row.get(26)?,
                            invalidation_reason: row.get(27)?,
                            invalidated_by_authority_provenance_id: row.get(28)?,
                        },
                    },
                    attestation: ConfirmationAttestation {
                        payload,
                        key_id: row.get(29)?,
                        signature_hex: row.get(32)?,
                    },
                    pinned_key_generation: u64::try_from(row.get::<_, i64>(30)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                30,
                                Type::Integer,
                                Box::new(error),
                            )
                        },
                    )?,
                    verified_at: row.get(33)?,
                })
            },
        )
        .optional()?;
        let Some(row) = row else {
            tx.commit()
                .context("closing absent resolved confirmation projection")?;
            return Ok(None);
        };
        let verification_clock = u64::try_from(row.verified_at)
            .context("persisted confirmation verification time is invalid")?;
        let verified =
            verify_confirmation_attestation(&row.attestation, &pinned_key, verification_clock)?;
        require_exact_resolved_runtime_projection(
            self,
            &row.projection,
            &pinned,
            &verified,
            row.pinned_key_generation,
        )?;
        require_active_owner_ingress_binding(&tx, verified.payload())?;
        tx.commit()
            .context("closing verified resolved confirmation projection")?;
        Ok(Some(DangerConfirmationRuntimeProjection::Resolved(
            Box::new(row.projection),
        )))
    }

    #[cfg(test)]
    pub(super) fn read_pending_danger_confirmation_alternative_binding_at_for_test(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        trusted_observed_at: i64,
    ) -> Result<Option<PendingDangerConfirmationAlternativeBinding>> {
        self.read_pending_danger_confirmation_alternative_binding_at(
            decision_id,
            selected_logical_action_id,
            trusted_observed_at,
        )
    }

    fn read_pending_danger_confirmation_alternative_binding_at(
        &self,
        decision_id: &str,
        selected_logical_action_id: &str,
        trusted_observed_at: i64,
    ) -> Result<Option<PendingDangerConfirmationAlternativeBinding>> {
        require_text("decision_id", decision_id)?;
        require_text("selected_logical_action_id", selected_logical_action_id)?;
        if trusted_observed_at <= 0 {
            bail!("observed time must be positive");
        }
        let conn = self.connection()?;
        conn.query_row(
            "SELECT c.delegation_id,e.normalized_original_intent,d.policy_revision,d.decision_id,d.decision_revision,p.resolved_leaf_manifest_json,c.manifest_digest,a.logical_action_id,a.exact_presented_action_json,a.declared_consequence,c.nonce_digest,d.requested_at,c.expires_at FROM execass_confirmation_challenges c JOIN execass_decisions d ON d.decision_id=c.decision_id JOIN execass_delegations e ON e.delegation_id=c.delegation_id JOIN execass_plans p ON p.delegation_id=e.delegation_id AND p.plan_revision=e.current_plan_revision AND p.manifest_digest=c.manifest_digest JOIN execass_confirmation_challenge_alternatives a ON a.challenge_id=c.challenge_id AND a.logical_action_id=?2 WHERE c.decision_id=?1 AND c.status='pending' AND d.status='pending' AND c.expires_at>?3",
            params![decision_id, selected_logical_action_id, trusted_observed_at],
            |row| {
                let action_json: String = row.get(8)?;
                let consequence: String = row.get(9)?;
                Ok(PendingDangerConfirmationAlternativeBinding {
                    delegation_id: row.get(0)?,
                    normalized_intent: row.get(1)?,
                    policy_revision: row.get(2)?,
                    decision_id: row.get(3)?,
                    decision_revision: row.get(4)?,
                    canonical_manifest_json: row.get(5)?,
                    manifest_digest: row.get(6)?,
                    selected_logical_action_id: row.get(7)?,
                    exact_selected_action_json: action_json.clone(),
                    exact_selected_action_digest: sha256_hex(action_json.as_bytes()),
                    declared_consequence: consequence.clone(),
                    declared_consequence_digest: sha256_hex(consequence.as_bytes()),
                    challenge_nonce_digest: row.get(10)?,
                    requested_at: row.get(11)?,
                    expires_at: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Present at most one pending concrete-consequence challenge for one exact
    /// dangerous action. An unchanged durable grant wins before prompt creation,
    /// including after restart or in a later delegation with the same normalized
    /// owner intent. This method never creates a generic approval or effect.
    pub fn ensure_dangerous_action_confirmation(
        &self,
        command: &PresentDangerousActionConfirmationCommand,
        manifest: &CanonicalLeafManifest,
        danger_route: &DangerRoute,
    ) -> Result<DangerConfirmationAdmissionOutcome> {
        self.ensure_dangerous_action_confirmation_with_scope(
            command,
            manifest,
            danger_route,
            None,
            None,
            None,
        )
    }

    /// Present or reuse confirmation for one versioned saved selector. The
    /// selector is opaque and stable-leaf-bound; only its expected resolved
    /// target membership may vary between occurrences.
    pub fn ensure_saved_routine_dangerous_action_confirmation(
        &self,
        command: &PresentDangerousActionConfirmationCommand,
        manifest: &CanonicalLeafManifest,
        danger_route: &DangerRoute,
        saved_routine: &VerifiedSavedRoutineSelector,
    ) -> Result<DangerConfirmationAdmissionOutcome> {
        self.ensure_dangerous_action_confirmation_with_scope(
            command,
            manifest,
            danger_route,
            Some(saved_routine),
            None,
            None,
        )
    }

    /// Persist one canonical combined disclosure for several dangerous
    /// alternatives while binding the resulting challenge to exactly the
    /// selected alternative.  The schema has one challenge/action binding, so
    /// this deliberately does not let resolution select another alternative.
    pub fn ensure_combined_dangerous_action_confirmation(
        &self,
        command: &PresentDangerousActionConfirmationCommand,
        manifest: &CanonicalLeafManifest,
        danger_routes: &[DangerRoute],
    ) -> Result<DangerConfirmationAdmissionOutcome> {
        let question = combined_question_from_verified_routes(manifest, danger_routes)?;
        let representative = question
            .alternatives()
            .first()
            .context("combined question has no alternatives")?;
        let leaf = exact_leaf(manifest, representative.logical_action_id())?;
        let danger_route = danger_routes
            .iter()
            .find(|route| route.confirmation_for_leaf(leaf).is_some())
            .context("selected dangerous alternative has no verified route")?;
        self.ensure_dangerous_action_confirmation_with_scope(
            command,
            manifest,
            danger_route,
            None,
            Some(question),
            Some(danger_routes),
        )
    }

    fn ensure_dangerous_action_confirmation_with_scope(
        &self,
        command: &PresentDangerousActionConfirmationCommand,
        manifest: &CanonicalLeafManifest,
        danger_route: &DangerRoute,
        saved_routine: Option<&VerifiedSavedRoutineSelector>,
        mut combined_question: Option<CombinedDangerousActionQuestion>,
        combined_routes: Option<&[DangerRoute]>,
    ) -> Result<DangerConfirmationAdmissionOutcome> {
        validate_present_command(command)?;
        let leaf = exact_leaf(manifest, &command.logical_action_id)?;
        if saved_routine.is_some_and(|selector| !selector.matches_stable_leaf(leaf)) {
            bail!("saved routine selector is not bound to the stable resolved action");
        }
        let assessment = danger_route
            .confirmation_for_leaf(leaf)
            .context("exact action does not carry a verified danger-confirmation route")?;
        if !assessment.requires_one_confirmation {
            bail!("danger route must require exactly one confirmation");
        }

        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let (normalized_intent, delegation_revision, plan_revision, policy_revision, stored_digest) =
            tx.query_row(
                "SELECT d.normalized_original_intent,d.state_revision,d.current_plan_revision,d.policy_revision,p.manifest_digest FROM execass_delegations d JOIN execass_plans p ON p.delegation_id=d.delegation_id AND p.plan_revision=d.current_plan_revision WHERE d.delegation_id=?1",
                params![command.delegation_id],
                |row| Ok((row.get::<_, String>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?,row.get::<_, i64>(3)?,row.get::<_, String>(4)?)),
            )
            .context("danger confirmation delegation/current plan is unavailable")?;
        let manifest_digest = manifest.canonical().digest().as_hex().to_string();
        if stored_digest != manifest_digest {
            bail!("danger confirmation manifest is not the current persisted plan manifest");
        }
        let binding = exact_binding(
            normalized_intent.clone(),
            manifest,
            leaf,
            &assessment.declared_consequence,
            saved_routine,
        )?;
        let alternative_bindings = match combined_routes {
            Some(routes) => {
                combined_bindings_from_verified_routes(manifest, routes, &normalized_intent)?
            }
            None => vec![(command.logical_action_id.clone(), binding.clone())],
        };
        if let Some(question) = &mut combined_question {
            *question = combined_question_with_bindings(question, &alternative_bindings)?;
        }
        if let Some(question) = &combined_question {
            validate_combined_question_binding(question, &alternative_bindings)?;
        }

        let cross_delegation_reuse = saved_routine.is_some();
        if let Some(grant) = find_active_grant(
            &tx,
            &binding,
            &command.delegation_id,
            cross_delegation_reuse,
        )? {
            tx.commit()
                .context("closing confirmed-action reuse lookup")?;
            return Ok(DangerConfirmationAdmissionOutcome::AlreadyConfirmed(grant));
        }

        expire_matching_pending_challenges(
            &tx,
            &binding,
            &command.delegation_id,
            cross_delegation_reuse,
            command.requested_at,
        )?;
        if let Some(challenge) = find_pending_challenge(
            &tx,
            &binding,
            &command.delegation_id,
            cross_delegation_reuse,
        )? {
            tx.commit().context("closing existing challenge lookup")?;
            return Ok(DangerConfirmationAdmissionOutcome::ExistingPending(
                challenge,
            ));
        }

        let decision_revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(decision_revision),0)+1 FROM execass_decisions WHERE delegation_id=?1",
            params![command.delegation_id],
            |row| row.get(0),
        )?;
        let nonce_digest = owner_resolution_challenge_nonce_digest(&command.challenge_nonce)
            .context("challenge nonce is empty")?;
        tx.execute(
            "INSERT INTO execass_decisions (decision_id,delegation_id,decision_revision,delegation_revision,plan_revision,policy_revision,decision_kind,status,result,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,side_effect_envelope_json,recommendation,consequence,alternatives_json,idempotency_key,requested_at,resolved_at,resolved_by_authority_provenance_id) VALUES (?1,?2,?3,?4,?5,?6,'dangerous_action_confirmation','pending',NULL,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,NULL,NULL)",
            params![
                command.decision_id,
                command.delegation_id,
                decision_revision,
                delegation_revision,
                plan_revision,
                policy_revision,
                binding.exact_presented_action_json,
                binding.confirmed_logical_action_identity,
                binding.manifest_digest,
                binding.payload_digest,
                binding.payload_and_material_operands_json,
                binding.target_audience_path_json,
                binding.connector_tool_identity,
                binding.connector_tool_version,
                binding.canonical_action_envelope_or_selector_json,
                "Confirm this exact action once to continue.",
                binding.declared_consequence,
                combined_question
                    .as_ref()
                    .map(canonical_combined_question_json)
                    .transpose()?
                    .unwrap_or_else(|| r#"["confirm_and_continue","revise","decline"]"#.to_string()),
                command.idempotency_key,
                command.requested_at,
            ],
        )
        .context("recording exact dangerous-action decision")?;
        tx.execute(
            "INSERT INTO execass_confirmation_challenges (challenge_id,decision_id,delegation_id,decision_revision,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence,nonce_digest,status,created_at,expires_at,resolved_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'pending',?15,?16,NULL)",
            params![
                command.challenge_id,
                command.decision_id,
                command.delegation_id,
                decision_revision,
                binding.exact_presented_action_json,
                binding.confirmed_logical_action_identity,
                binding.manifest_digest,
                binding.payload_digest,
                binding.payload_and_material_operands_json,
                binding.connector_tool_identity,
                binding.connector_tool_version,
                binding.canonical_action_envelope_or_selector_json,
                binding.declared_consequence,
                nonce_digest,
                command.requested_at,
                command.expires_at,
            ],
        )
        .context("recording one exact confirmation challenge")?;
        for (logical_action_id, alternative) in &alternative_bindings {
            tx.execute(
                "INSERT INTO execass_confirmation_challenge_alternatives (challenge_id,logical_action_id,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![command.challenge_id, logical_action_id, alternative.exact_presented_action_json, alternative.confirmed_logical_action_identity, alternative.manifest_digest, alternative.payload_digest, alternative.payload_and_material_operands_json, alternative.target_audience_path_json, alternative.connector_tool_identity, alternative.connector_tool_version, alternative.canonical_action_envelope_or_selector_json, alternative.declared_consequence],
            )?;
        }
        let challenge = read_challenge(&tx, &command.challenge_id)?
            .context("new confirmation challenge disappeared")?;
        tx.commit()
            .context("committing confirmation presentation")?;
        Ok(DangerConfirmationAdmissionOutcome::Presented(challenge))
    }

    #[cfg(test)]
    pub(super) fn confirm_dangerous_action_attested_at_for_test(
        &self,
        command: &ConfirmDangerousActionCommand,
        attestation: &ConfirmationAttestation,
        trusted_resolved_at: i64,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        self.confirm_dangerous_action_attested_at(command, attestation, trusted_resolved_at)
    }

    #[cfg(test)]
    fn confirm_dangerous_action_attested_at(
        &self,
        command: &ConfirmDangerousActionCommand,
        attestation: &ConfirmationAttestation,
        trusted_resolved_at: i64,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        validate_attested_confirmation_command(command, trusted_resolved_at)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let outcome = self.resolve_attested_confirmation_in_transaction(
            &tx,
            command,
            attestation,
            trusted_resolved_at,
        )?;
        tx.commit().context("committing attested confirmation")?;
        Ok(outcome)
    }

    fn resolve_attested_confirmation_in_transaction(
        &self,
        tx: &Transaction<'_>,
        command: &ConfirmDangerousActionCommand,
        attestation: &ConfirmationAttestation,
        trusted_resolved_at: i64,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        let trusted_now = u64::try_from(trusted_resolved_at)
            .context("trusted confirmation time must fit the attestation clock")?;
        let challenge = find_challenge_by_decision(tx, &command.decision_id)?
            .context("attested confirmation decision has no challenge")?;
        if challenge.decision_revision != command.decision_revision {
            bail!("decision revision does not match the current challenge");
        }
        let persisted_replay = if challenge.status == ConfirmationChallengeStatus::Resolved {
            Some(
                load_persisted_attestation_replay(tx, &command.decision_id)?
                    .context("resolved confirmation has no persisted attestation")?,
            )
        } else {
            None
        };

        let pinned = load_active_confirmation_key(tx)?;
        if pinned.canonical_root_identity != self.root_identity {
            bail!("active confirmation key belongs to a different canonical state root");
        }
        if confirmation_verifying_key_digest_hex(&pinned.verifying_key_hex)?
            != pinned.verifying_key_digest
        {
            bail!("active confirmation key digest does not match its stored material");
        }
        let pinned_key = PinnedConfirmationAttestationKey::from_hex(
            pinned.key_id.clone(),
            pinned.key_generation,
            &pinned.verifying_key_hex,
        )?;
        let verification_clock = match &persisted_replay {
            Some(stored) => u64::try_from(stored.verified_at)
                .context("stored attestation verification time does not fit its clock")?,
            None => trusted_now,
        };
        let verified =
            verify_confirmation_attestation(attestation, &pinned_key, verification_clock)?;
        let payload = verified.payload();

        let selected = read_challenge_alternative(
            tx,
            &challenge.challenge_id,
            &command.selected_logical_action_id,
        )?
        .context("owner selected an unknown dangerous alternative")?;
        let current = load_current_confirmation_binding(tx, &command.decision_id)?;
        require_exact_attestation_binding(
            self,
            command,
            &challenge,
            &selected,
            &current,
            &pinned,
            &verified,
            trusted_resolved_at,
        )?;
        require_active_owner_ingress_binding(tx, payload)?;

        if challenge.status == ConfirmationChallengeStatus::Resolved {
            let stored = persisted_replay
                .as_ref()
                .context("resolved confirmation replay state disappeared")?;
            let grant = find_grant_by_decision(tx, &command.decision_id)?
                .context("resolved confirmation has no durable grant")?;
            if stored.attestation_digest != verified.attestation_digest()
                || stored.signature_hex != attestation.signature_hex
                || stored.signed_payload_json != serde_json::to_string(payload)?
                || stored.selected_logical_action_id != command.selected_logical_action_id
                || grant.grant_id != command.grant_id
            {
                bail!("confirmation replay differs from the recorded attestation or grant");
            }
            return Ok(DangerConfirmationResolutionOutcome::Replayed(grant));
        }
        if challenge.status != ConfirmationChallengeStatus::Pending {
            bail!("confirmation challenge is not pending");
        }

        let authority_record = authority_record_from_verified_attestation(
            &verified,
            challenge.manifest_digest.clone(),
            challenge.nonce_digest.clone(),
        )?;
        insert_authority(tx, &authority_record)?;
        insert_verified_confirmation_attestation(
            tx,
            attestation,
            &verified,
            &authority_record,
            trusted_resolved_at,
        )?;
        tx.execute(
            "UPDATE execass_confirmation_challenges SET selected_logical_action_id=?2,status='resolved',resolved_at=?3 WHERE challenge_id=?1 AND status='pending'",
            params![challenge.challenge_id, command.selected_logical_action_id, trusted_resolved_at],
        )?;
        tx.execute(
            "UPDATE execass_decisions SET status='resolved',result='confirm_and_continue',exact_presented_action_json=?2,confirmed_logical_action_identity=?3,manifest_digest=?4,payload_digest=?5,payload_and_material_operands_json=?6,target_audience_path_json=?7,connector_tool_identity=?8,connector_tool_version=?9,side_effect_envelope_json=?10,consequence=?11,resolved_at=?12,resolved_by_authority_provenance_id=?13 WHERE decision_id=?1 AND status='pending'",
            params![command.decision_id, selected.exact_presented_action_json, selected.confirmed_logical_action_identity, selected.manifest_digest, selected.payload_digest, selected.payload_and_material_operands_json, selected.target_audience_path_json, selected.connector_tool_identity, selected.connector_tool_version, selected.canonical_action_envelope_or_selector_json, selected.declared_consequence, trusted_resolved_at, authority_record.authority_provenance_id],
        )?;
        tx.execute(
            "INSERT INTO execass_accepted_confirmation_grants (grant_id,delegation_id,decision_id,confirmed_logical_action_identity,canonical_action_envelope_or_selector_json,payload_and_material_operands_json,payload_and_material_operands_digest,connector_tool_identity,connector_tool_version,declared_consequence,accepted_by_authority_provenance_id,confirmation_attestation_digest,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,NULL,NULL,NULL)",
            params![command.grant_id, challenge.delegation_id, command.decision_id, selected.confirmed_logical_action_identity, selected.canonical_action_envelope_or_selector_json, selected.payload_and_material_operands_json, selected.payload_digest, selected.connector_tool_identity, selected.connector_tool_version, selected.declared_consequence, authority_record.authority_provenance_id, verified.attestation_digest(), trusted_resolved_at],
        )
        .context("creating durable attested confirmation grant")?;
        let grant = find_grant_by_decision(tx, &command.decision_id)?
            .context("accepted attested confirmation grant disappeared")?;
        Ok(DangerConfirmationResolutionOutcome::Confirmed(grant))
    }

    /// Resolves one signed dangerous-action confirmation and records its
    /// grant, typed result, outbox event, canonical receipt, and optional exact
    /// continuation in one writer transaction.
    pub fn confirm_dangerous_action_attested_atomically(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        resolution: &AtomicDecisionResolutionCommand,
        grant_id: &str,
        attestation: &ConfirmationAttestation,
    ) -> Result<AtomicDecisionResolutionOutcome> {
        self.confirm_dangerous_action_attested_atomically_at(
            integrity,
            redactor,
            resolution,
            grant_id,
            attestation,
            trusted_unix_time_ms()?,
        )
    }

    #[cfg(test)]
    pub(super) fn confirm_dangerous_action_attested_atomically_at_for_test(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        resolution: &AtomicDecisionResolutionCommand,
        grant_id: &str,
        attestation: &ConfirmationAttestation,
        trusted_now: i64,
    ) -> Result<AtomicDecisionResolutionOutcome> {
        self.confirm_dangerous_action_attested_atomically_at(
            integrity,
            redactor,
            resolution,
            grant_id,
            attestation,
            trusted_now,
        )
    }

    fn confirm_dangerous_action_attested_atomically_at(
        &self,
        integrity: &super::receipt_integrity::ReceiptIntegrityStore,
        redactor: &super::redaction::ReceiptRedactor,
        resolution: &AtomicDecisionResolutionCommand,
        grant_id: &str,
        attestation: &ConfirmationAttestation,
        trusted_now: i64,
    ) -> Result<AtomicDecisionResolutionOutcome> {
        require_text("grant_id", grant_id)?;
        if resolution.result != DecisionResult::ConfirmAndContinue {
            bail!("signed dangerous resolution must confirm exactly one action");
        }
        let selected_action_id = resolution
            .selected_logical_action_id
            .as_deref()
            .context("signed dangerous resolution has no selected action")?
            .to_string();
        let preflight = self.connection()?;
        let challenge = find_challenge_by_decision(&preflight, &resolution.decision_id)?
            .context("signed dangerous resolution has no challenge")?;
        let persisted_resolved_at: Option<i64> = preflight.query_row(
            "SELECT resolved_at FROM execass_decisions WHERE decision_id=?1",
            params![resolution.decision_id],
            |row| row.get(0),
        )?;
        let trusted_resolved_at = persisted_resolved_at.unwrap_or(trusted_now);
        let pinned = load_active_confirmation_key(&preflight)?;
        let pinned_key = PinnedConfirmationAttestationKey::from_hex(
            pinned.key_id,
            pinned.key_generation,
            &pinned.verifying_key_hex,
        )?;
        let verified = verify_confirmation_attestation(
            attestation,
            &pinned_key,
            u64::try_from(trusted_resolved_at)?,
        )?;
        let authority_record = authority_record_from_verified_attestation(
            &verified,
            challenge.manifest_digest,
            challenge.nonce_digest,
        )?;
        drop(preflight);
        let mut resolution = resolution.clone();
        resolution.write.occurred_at = trusted_resolved_at;
        resolution.outbox_event.occurred_at = trusted_resolved_at;
        resolution.receipt.occurred_at = trusted_resolved_at;
        resolution.receipt.committed_at = trusted_resolved_at;
        resolution.receipt.actor = ReceiptActorBinding {
            actor_type: authority_record.actor_type,
            actor_identity: super::redaction::SafeText::new(
                &authority_record.credential_identity,
                &[],
            )?,
            authority_provenance_id: authority_record.authority_provenance_id,
        };
        if let Some(continuation) = resolution.continuation.as_mut() {
            continuation.created_at = trusted_resolved_at;
            continuation.updated_at = trusted_resolved_at;
        }
        if let Some(logical_effect) = resolution.logical_effect.as_mut() {
            logical_effect.created_at = trusted_resolved_at;
        }
        super::decision::validate_static_command(&resolution)?;
        let legacy = ConfirmDangerousActionCommand {
            decision_id: resolution.decision_id.clone(),
            decision_revision: resolution.decision_revision,
            grant_id: grant_id.to_string(),
            selected_logical_action_id: selected_action_id,
            response: DangerousActionConfirmationResponse::ConfirmAndContinue,
        };

        let outcome = self.mutate_with_atomic_receipt(
            integrity,
            redactor,
            &resolution.receipt,
            |transaction| {
                let Some(decision) =
                    super::decision::load_decision(transaction, &resolution.decision_id)?
                else {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        AtomicDangerMutation::NotFound,
                    ));
                };
                if decision.decision_revision != resolution.decision_revision {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        AtomicDangerMutation::Conflict(decision.result),
                    ));
                }
                if decision.status == DecisionStatus::Resolved {
                    let confirmation = self.resolve_attested_confirmation_in_transaction(
                        transaction,
                        &legacy,
                        attestation,
                        trusted_resolved_at,
                    )?;
                    if !matches!(
                        confirmation,
                        DangerConfirmationResolutionOutcome::Replayed(_)
                    ) {
                        bail!("resolved dangerous decision did not replay its exact grant");
                    }
                    let Some(mut bundle) =
                        super::decision::load_replay_bundle(transaction, &resolution, &decision)?
                    else {
                        return Ok(AtomicReceiptMutation::NoAppend(
                            AtomicDangerMutation::Conflict(decision.result),
                        ));
                    };
                    let grant = find_grant_by_decision(transaction, &decision.decision_id)?
                        .context("resolved dangerous decision has no grant")?;
                    bundle.confirmation_grant = Some(grant);
                    return Ok(AtomicReceiptMutation::NoAppend(
                        AtomicDangerMutation::Replayed(Box::new(bundle)),
                    ));
                }
                if decision.status != DecisionStatus::Pending
                    || decision.decision_kind != DecisionKind::DangerousActionConfirmation
                    || resolution.outbox_event.aggregate_id != decision.delegation_id
                    || resolution.outbox_event.aggregate_revision != decision.delegation_revision
                    || resolution.receipt.delegation_id != decision.delegation_id
                    || resolution.receipt.expected_state_revision != decision.delegation_revision
                {
                    return Ok(AtomicReceiptMutation::NoAppend(
                        AtomicDangerMutation::Conflict(decision.result),
                    ));
                }
                super::decision::validate_continuation_and_effect(
                    transaction,
                    &resolution,
                    &decision,
                )?;
                let confirmation = self.resolve_attested_confirmation_in_transaction(
                    transaction,
                    &legacy,
                    attestation,
                    trusted_resolved_at,
                )?;
                let DangerConfirmationResolutionOutcome::Confirmed(grant) = confirmation else {
                    bail!("pending dangerous decision did not create its exact grant");
                };
                if resolution.receipt.actor.authority_provenance_id
                    != grant.accepted_by_authority_provenance_id
                {
                    bail!("dangerous decision receipt actor is not the accepted owner authority");
                }
                if let Some(continuation) = &resolution.continuation {
                    insert_continuation(transaction, continuation)?;
                    super::decision::promote_decision_action_to_runnable(
                        transaction,
                        &resolution,
                        continuation,
                    )?;
                }
                if let Some(effect) = &resolution.logical_effect {
                    insert_planned_logical_effect(transaction, effect)?;
                    let snapshot = resolution
                        .technical_quota_snapshot
                        .as_ref()
                        .context("exact dangerous effect has no technical quota snapshot")?;
                    insert_technical_quota_snapshot(
                        transaction,
                        snapshot,
                        resolution.write.occurred_at,
                    )?;
                    let requirements = resolution
                        .technical_resource_requirements
                        .as_ref()
                        .context("exact dangerous effect has no resource requirements")?;
                    insert_technical_resource_requirements(
                        transaction,
                        requirements,
                        resolution.write.occurred_at,
                    )?;
                }
                insert_outbox(transaction, &resolution.outbox_event)?;
                let resolved = super::decision::load_decision(transaction, &decision.decision_id)?
                    .context("atomically confirmed decision disappeared")?;
                let outbox_event = get_outbox(transaction, &resolution.outbox_event.event_id)?
                    .context("atomically confirmed outbox disappeared")?;
                Ok(AtomicReceiptMutation::Append(
                    AtomicDangerMutation::Applied(Box::new(AtomicDangerApplied {
                        decision: resolved,
                        grant,
                        continuation: resolution.continuation.clone(),
                        logical_effect: resolution.logical_effect.clone(),
                        technical_quota_snapshot: resolution
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
                        technical_resource_requirements: resolution
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
                    })),
                ))
            },
        )?;

        match outcome {
            AtomicReceiptWriteOutcome::Appended {
                value: AtomicDangerMutation::Applied(applied),
                receipt,
            } => Ok(AtomicDecisionResolutionOutcome::Applied(Box::new(
                AtomicDecisionResolutionBundle {
                    decision: applied.decision,
                    confirmation_grant: Some(applied.grant),
                    continuation: applied.continuation,
                    logical_effect: applied.logical_effect,
                    technical_quota_snapshot: applied.technical_quota_snapshot,
                    technical_resource_requirements: applied.technical_resource_requirements,
                    outbox_event: applied.outbox_event,
                    receipt,
                },
            ))),
            AtomicReceiptWriteOutcome::NoAppend(AtomicDangerMutation::Replayed(bundle)) => {
                Ok(AtomicDecisionResolutionOutcome::Replayed(bundle))
            }
            AtomicReceiptWriteOutcome::NoAppend(AtomicDangerMutation::Conflict(winning_result)) => {
                Ok(AtomicDecisionResolutionOutcome::Conflict { winning_result })
            }
            AtomicReceiptWriteOutcome::NoAppend(AtomicDangerMutation::NotFound) => {
                Ok(AtomicDecisionResolutionOutcome::NotFound)
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
            | AtomicReceiptWriteOutcome::NoAppend(AtomicDangerMutation::Applied { .. }) => {
                bail!("atomic dangerous receipt coordinator returned an impossible outcome")
            }
        }
    }

    /// Legacy internal helper retained only for the pre-attestation unit corpus.
    #[cfg(test)]
    pub(super) fn confirm_dangerous_action_at_for_test(
        &self,
        command: &ConfirmDangerousActionCommand,
        resolution_authority: &VerifiedOwnerAuthority,
        trusted_resolved_at: i64,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        if command.response == DangerousActionConfirmationResponse::ConfirmAndContinue {
            if let Some(attestation) = self.issue_legacy_test_confirmation_attestation(
                command,
                resolution_authority,
                trusted_resolved_at,
            )? {
                return self.confirm_dangerous_action_attested_at(
                    command,
                    &attestation,
                    trusted_resolved_at,
                );
            }
        }
        self.confirm_dangerous_action_at(command, resolution_authority, trusted_resolved_at)
    }

    #[cfg(test)]
    pub(super) fn issue_legacy_test_confirmation_attestation(
        &self,
        command: &ConfirmDangerousActionCommand,
        resolution_authority: &VerifiedOwnerAuthority,
        trusted_resolved_at: i64,
    ) -> Result<Option<ConfirmationAttestation>> {
        let Some(binding) = self.read_pending_danger_confirmation_alternative_binding_at_for_test(
            &command.decision_id,
            &command.selected_logical_action_id,
            trusted_resolved_at,
        )?
        else {
            return Ok(None);
        };
        let canonical = canonicalize_owner_authority(resolution_authority).map_err(|detail| {
            anyhow::anyhow!("invalid legacy test resolution authority: {detail}")
        })?;
        if canonical.authority_kind() != "decision_resolution"
            || owner_normalized_intent_digest(&binding.normalized_intent).as_deref()
                != Some(canonical.normalized_intent_digest().as_hex())
            || canonical.policy_revision() != binding.policy_revision
            || canonical.created_at() < binding.requested_at
            || canonical.created_at() > trusted_resolved_at
            || canonical
                .expires_at()
                .is_none_or(|expiry| expiry <= trusted_resolved_at || expiry != binding.expires_at)
            || canonical.bound_decision_id() != Some(command.decision_id.as_str())
            || canonical.bound_decision_revision() != Some(command.decision_revision)
            || canonical
                .bound_manifest_digest()
                .map(|digest| digest.as_hex())
                != Some(binding.manifest_digest.as_str())
            || canonical
                .bound_challenge_nonce_digest()
                .map(|digest| digest.as_hex())
                != Some(binding.challenge_nonce_digest.as_str())
        {
            bail!("legacy test resolution authority is not bound to this exact live challenge");
        }
        let (
            actor_type,
            mut credential_identity,
            mut authenticated_ingress,
            mut channel_assurance,
            correlation,
            source_message_id,
            provider_event_id,
        ) = match canonical.owner_evidence() {
            carsinos_core::execass_manifest::CanonicalOwnerEvidence::LocalInteractive {
                authenticated_client_id,
                authenticated_ingress,
                channel_assurance,
                request_correlation_id,
                ..
            } => (
                "human_local".to_string(),
                authenticated_client_id.clone(),
                authenticated_ingress.clone(),
                channel_assurance.clone(),
                request_correlation_id.clone(),
                None,
                None,
            ),
            carsinos_core::execass_manifest::CanonicalOwnerEvidence::RemoteAuthenticated {
                adapter_id,
                provider_account_id,
                authenticated_ingress,
                channel_assurance,
                source_message_id,
                request_correlation_id,
            } => (
                "human_remote".to_string(),
                format!("{adapter_id}:{provider_account_id}"),
                authenticated_ingress.clone(),
                channel_assurance.clone(),
                request_correlation_id.clone(),
                Some(source_message_id.clone()),
                Some(format!("legacy-test-event:{source_message_id}")),
            ),
        };
        const TEST_SECRET: [u8; 32] = [42; 32];
        let identity =
            super::confirmation_custody::activate_test_confirmation_authority(self, TEST_SECRET)?;
        if actor_type == "human_local" {
            credential_identity = identity.local_credential_identity().to_string();
            authenticated_ingress = "native-control".to_string();
            channel_assurance = "interactive-local".to_string();
        }
        let signing_key = SigningKey::from_bytes(&TEST_SECRET);
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        if actor_type == "human_remote" {
            tx.execute(
                "INSERT OR IGNORE INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES (?1,?2,?3,?4,?5,1,'active',?6)",
                params![framed_digest(&[b"legacy-test-remote-binding", credential_identity.as_bytes(), authenticated_ingress.as_bytes()]), actor_type, credential_identity, authenticated_ingress, channel_assurance, trusted_resolved_at],
            )?;
        }
        tx.commit()
            .context("provisioning legacy test confirmation trust")?;

        let payload = ConfirmationAttestationPayload {
            actor_type,
            credential_identity,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id: correlation,
            source_message_id,
            provider_event_id,
            normalized_intent_digest: canonical.normalized_intent_digest().as_hex().to_string(),
            policy_revision: u64::try_from(binding.policy_revision)?,
            decision_id: binding.decision_id,
            decision_revision: u64::try_from(binding.decision_revision)?,
            decision_result: command.response.as_str().to_string(),
            canonical_manifest_digest: binding.manifest_digest,
            selected_logical_action_id: binding.selected_logical_action_id,
            selected_action_digest: binding.exact_selected_action_digest,
            declared_consequence_digest: binding.declared_consequence_digest,
            challenge_nonce_digest: binding.challenge_nonce_digest,
            challenge_expires_at_ms: u64::try_from(binding.expires_at)?,
            issued_at_ms: u64::try_from(trusted_resolved_at)?,
            canonical_root_identity: identity.canonical_root_identity().to_string(),
            installation_identity: identity.installation_identity().to_string(),
            os_user_identity_digest: identity.os_user_identity_digest().to_string(),
            state_root_generation: identity.state_root_generation(),
            signer_key_generation: identity.key_generation(),
        };
        let signing_bytes = confirmation_attestation_signing_bytes(&payload, identity.key_id())?;
        let signature = signing_key.sign(&signing_bytes);
        Ok(Some(ConfirmationAttestation {
            payload,
            key_id: identity.key_id().to_string(),
            signature_hex: encode_hex(&signature.to_bytes()),
        }))
    }

    #[cfg(test)]
    fn confirm_dangerous_action_at(
        &self,
        command: &ConfirmDangerousActionCommand,
        resolution_authority: &VerifiedOwnerAuthority,
        trusted_resolved_at: i64,
    ) -> Result<DangerConfirmationResolutionOutcome> {
        require_text("decision_id", &command.decision_id)?;
        require_text("grant_id", &command.grant_id)?;
        if command.response == DangerousActionConfirmationResponse::ConfirmAndContinue {
            require_text(
                "selected_logical_action_id",
                &command.selected_logical_action_id,
            )?;
        }
        if command.decision_revision <= 0 || trusted_resolved_at <= 0 {
            bail!("decision revision and resolution time must be positive");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(challenge) = find_challenge_by_decision(&tx, &command.decision_id)? else {
            tx.commit().context("closing missing confirmation lookup")?;
            return Ok(DangerConfirmationResolutionOutcome::NotFound);
        };
        if challenge.decision_revision != command.decision_revision {
            bail!("decision revision does not match the current challenge");
        }
        if challenge.status == ConfirmationChallengeStatus::Resolved {
            let result: String = tx.query_row(
                "SELECT result FROM execass_decisions WHERE decision_id=?1",
                params![command.decision_id],
                |row| row.get(0),
            )?;
            match (command.response, result.as_str()) {
                (
                    DangerousActionConfirmationResponse::ConfirmAndContinue,
                    "confirm_and_continue",
                ) => {
                    let selected: Option<String> = tx.query_row(
                        "SELECT selected_logical_action_id FROM execass_confirmation_challenges WHERE challenge_id=?1",
                        params![challenge.challenge_id],
                        |row| row.get(0),
                    )?;
                    if selected.as_deref() != Some(command.selected_logical_action_id.as_str()) {
                        bail!("confirmation replay changes the originally selected alternative");
                    }
                    let grant = find_grant_by_decision(&tx, &command.decision_id)?
                        .context("resolved confirmation has no durable grant")?;
                    tx.commit().context("closing confirmation replay")?;
                    return Ok(DangerConfirmationResolutionOutcome::Replayed(grant));
                }
                (DangerousActionConfirmationResponse::Revise, "revise") => {
                    tx.commit().context("closing revise replay")?;
                    return Ok(DangerConfirmationResolutionOutcome::Revised);
                }
                (DangerousActionConfirmationResponse::Decline, "decline") => {
                    tx.commit().context("closing decline replay")?;
                    return Ok(DangerConfirmationResolutionOutcome::Declined);
                }
                _ => bail!("confirmation replay changes the terminal owner response"),
            }
        }
        if challenge.status == ConfirmationChallengeStatus::Expired
            || trusted_resolved_at >= challenge.expires_at
        {
            if challenge.status == ConfirmationChallengeStatus::Pending {
                tx.execute(
                    "UPDATE execass_confirmation_challenges SET status='expired' WHERE challenge_id=?1 AND status='pending'",
                    params![challenge.challenge_id],
                )?;
                tx.execute(
                    "UPDATE execass_decisions SET status='expired' WHERE decision_id=?1 AND status='pending'",
                    params![command.decision_id],
                )?;
            }
            tx.commit().context("committing challenge expiry")?;
            return Ok(DangerConfirmationResolutionOutcome::Expired);
        }
        if challenge.status != ConfirmationChallengeStatus::Pending {
            bail!("confirmation challenge is not pending");
        }
        if trusted_resolved_at < challenge.created_at {
            bail!("trusted resolution time precedes challenge presentation");
        }
        let canonical_authority = canonicalize_owner_authority(resolution_authority)
            .map_err(|detail| anyhow::anyhow!("invalid decision resolution authority: {detail}"))?;
        let stored_normalized_intent: String = tx.query_row(
            "SELECT normalized_original_intent FROM execass_delegations WHERE delegation_id=?1",
            params![challenge.delegation_id],
            |row| row.get(0),
        )?;
        if canonical_authority.authority_kind() != "decision_resolution"
            || owner_normalized_intent_digest(&stored_normalized_intent).as_deref()
                != Some(canonical_authority.normalized_intent_digest().as_hex())
            || canonical_authority.created_at() > trusted_resolved_at
            || canonical_authority.expires_at().is_none_or(|expiry| {
                expiry <= trusted_resolved_at || expiry != challenge.expires_at
            })
            || canonical_authority.bound_decision_id() != Some(command.decision_id.as_str())
            || canonical_authority.bound_decision_revision() != Some(command.decision_revision)
            || canonical_authority
                .bound_manifest_digest()
                .map(|digest| digest.as_hex())
                != Some(challenge.manifest_digest.as_str())
            || canonical_authority
                .bound_challenge_nonce_digest()
                .map(|digest| digest.as_hex())
                != Some(challenge.nonce_digest.as_str())
        {
            bail!("owner resolution authority is not bound to this exact live challenge");
        }
        let authority_record = authority_record_from_manifest(&canonical_authority)?;
        insert_authority(&tx, &authority_record)?;
        if command.response != DangerousActionConfirmationResponse::ConfirmAndContinue {
            tx.execute(
                "UPDATE execass_confirmation_challenges SET status='resolved',resolved_at=?2 WHERE challenge_id=?1 AND status='pending'",
                params![challenge.challenge_id, trusted_resolved_at],
            )?;
            tx.execute(
                "UPDATE execass_decisions SET status='resolved',result=?2,resolved_at=?3,resolved_by_authority_provenance_id=?4 WHERE decision_id=?1 AND status='pending'",
                params![command.decision_id, command.response.as_str(), trusted_resolved_at, authority_record.authority_provenance_id],
            )?;
            tx.commit()
                .context("committing non-affirmative confirmation response")?;
            return Ok(match command.response {
                DangerousActionConfirmationResponse::Revise => {
                    DangerConfirmationResolutionOutcome::Revised
                }
                DangerousActionConfirmationResponse::Decline => {
                    DangerConfirmationResolutionOutcome::Declined
                }
                DangerousActionConfirmationResponse::ConfirmAndContinue => unreachable!(),
            });
        }
        let selected = read_challenge_alternative(
            &tx,
            &challenge.challenge_id,
            &command.selected_logical_action_id,
        )?
        .context("owner selected an unknown dangerous alternative")?;
        tx.execute(
            "UPDATE execass_confirmation_challenges SET selected_logical_action_id=?2,status='resolved',resolved_at=?3 WHERE challenge_id=?1 AND status='pending'",
            params![challenge.challenge_id, command.selected_logical_action_id, trusted_resolved_at],
        )?;
        tx.execute(
            "UPDATE execass_decisions SET status='resolved',result='confirm_and_continue',exact_presented_action_json=?2,confirmed_logical_action_identity=?3,manifest_digest=?4,payload_digest=?5,payload_and_material_operands_json=?6,target_audience_path_json=?7,connector_tool_identity=?8,connector_tool_version=?9,side_effect_envelope_json=?10,consequence=?11,resolved_at=?12,resolved_by_authority_provenance_id=?13 WHERE decision_id=?1 AND status='pending'",
            params![command.decision_id, selected.exact_presented_action_json, selected.confirmed_logical_action_identity, selected.manifest_digest, selected.payload_digest, selected.payload_and_material_operands_json, selected.target_audience_path_json, selected.connector_tool_identity, selected.connector_tool_version, selected.canonical_action_envelope_or_selector_json, selected.declared_consequence, trusted_resolved_at, authority_record.authority_provenance_id],
        )?;
        tx.execute(
            "INSERT INTO execass_accepted_confirmation_grants (grant_id,delegation_id,decision_id,confirmed_logical_action_identity,canonical_action_envelope_or_selector_json,payload_and_material_operands_json,payload_and_material_operands_digest,connector_tool_identity,connector_tool_version,declared_consequence,accepted_by_authority_provenance_id,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,NULL,NULL,NULL)",
            params![
                command.grant_id,
                challenge.delegation_id,
                command.decision_id,
                selected.confirmed_logical_action_identity,
                selected.canonical_action_envelope_or_selector_json,
                selected.payload_and_material_operands_json,
                selected.payload_digest,
                selected.connector_tool_identity,
                selected.connector_tool_version,
                selected.declared_consequence,
                authority_record.authority_provenance_id,
                trusted_resolved_at,
            ],
        )
        .context("creating durable accepted confirmation grant")?;
        let grant = find_grant_by_decision(&tx, &command.decision_id)?
            .context("accepted confirmation grant disappeared")?;
        tx.commit().context("committing accepted confirmation")?;
        Ok(DangerConfirmationResolutionOutcome::Confirmed(grant))
    }

    /// Invalidate only the identified accepted action after a separately
    /// authenticated, action-specific owner amendment. Unrelated decisions,
    /// stops, revisions, and grants are not consulted or changed.
    pub fn invalidate_confirmation_grant_by_owner(
        &self,
        command: &InvalidateAcceptedConfirmationGrantCommand,
        owner_amendment_authority: &VerifiedOwnerAuthority,
    ) -> Result<ConfirmationGrantInvalidationOutcome> {
        require_text("grant_id", &command.grant_id)?;
        require_text("decision_id", &command.decision_id)?;
        if command.invalidated_at <= 0
            || !matches!(
                command.invalidation_reason,
                AcceptedConfirmationGrantInvalidation::ExplicitActionSpecificOwnerAmendment
                    | AcceptedConfirmationGrantInvalidation::ExplicitActionSpecificOwnerRevocation
                    | AcceptedConfirmationGrantInvalidation::ExplicitActionSpecificOwnerCancellation
            )
        {
            bail!("only an explicit action-specific owner change can use this invalidation path");
        }

        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(existing) = find_grant_by_id(&tx, &command.grant_id)? else {
            tx.commit().context("closing missing grant lookup")?;
            return Ok(ConfirmationGrantInvalidationOutcome::NotFound);
        };
        if existing.decision_id != command.decision_id {
            bail!("grant is not bound to the identified confirmed action");
        }

        let canonical_authority = canonicalize_owner_authority(owner_amendment_authority)
            .map_err(|detail| anyhow::anyhow!("invalid owner amendment authority: {detail}"))?;
        let (decision_revision, manifest_digest, challenge_nonce_digest): (i64, String, String) = tx.query_row(
            "SELECT d.decision_revision,d.manifest_digest,c.nonce_digest FROM execass_decisions d JOIN execass_confirmation_challenges c ON c.decision_id=d.decision_id WHERE d.decision_id=?1",
            params![command.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if canonical_authority.authority_kind() != "action_specific_owner_amendment"
            || canonical_authority.bound_decision_id() != Some(command.decision_id.as_str())
            || canonical_authority.bound_decision_revision() != Some(decision_revision)
            || canonical_authority
                .bound_manifest_digest()
                .map(|digest| digest.as_hex())
                != Some(manifest_digest.as_str())
            || canonical_authority
                .bound_challenge_nonce_digest()
                .map(|digest| digest.as_hex())
                != Some(challenge_nonce_digest.as_str())
        {
            bail!("owner amendment authority is not bound to this exact confirmed action");
        }
        let authority_record = authority_record_from_manifest(&canonical_authority)?;

        if existing.invalidated_at.is_some() {
            if existing.invalidation_reason == Some(command.invalidation_reason)
                && existing.invalidated_by_authority_provenance_id.as_deref()
                    == Some(authority_record.authority_provenance_id.as_str())
            {
                tx.commit().context("closing invalidation replay")?;
                return Ok(ConfirmationGrantInvalidationOutcome::Replayed(existing));
            }
            bail!("accepted confirmation grant was already invalidated differently");
        }
        if command.invalidated_at < existing.accepted_at
            || canonical_authority.created_at() > command.invalidated_at
        {
            bail!("owner invalidation time precedes the accepted action or authority");
        }

        insert_authority(&tx, &authority_record)?;
        tx.execute(
            "UPDATE execass_accepted_confirmation_grants SET invalidated_at=?2,invalidation_reason=?3,invalidated_by_authority_provenance_id=?4 WHERE grant_id=?1 AND invalidated_at IS NULL",
            params![
                command.grant_id,
                command.invalidated_at,
                command.invalidation_reason.as_str(),
                authority_record.authority_provenance_id,
            ],
        )?;
        let invalidated = find_grant_by_id(&tx, &command.grant_id)?
            .context("invalidated accepted confirmation grant disappeared")?;
        tx.commit().context("committing owner grant invalidation")?;
        Ok(ConfirmationGrantInvalidationOutcome::Invalidated(
            invalidated,
        ))
    }
}

#[derive(Debug)]
struct RemoteIngressBindingRow {
    credential_identity: String,
    authenticated_ingress: String,
    status: String,
    created_at: i64,
}

fn latest_remote_ingress_binding(
    tx: &Transaction<'_>,
    channel_assurance: &str,
) -> Result<Option<RemoteIngressBindingRow>> {
    tx.query_row(
        "SELECT credential_identity,authenticated_ingress,status,created_at FROM execass_owner_ingress_bindings WHERE actor_type='human_remote' AND channel_assurance=?1 ORDER BY created_at DESC,binding_id DESC LIMIT 1",
        params![channel_assurance],
        |row| {
            Ok(RemoteIngressBindingRow {
                credential_identity: row.get(0)?,
                authenticated_ingress: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn require_newer_remote_binding_time(
    current: Option<&RemoteIngressBindingRow>,
    observed_at: i64,
) -> Result<()> {
    if current.is_some_and(|row| observed_at <= row.created_at) {
        bail!("remote confirmation ingress change must advance its trusted time");
    }
    Ok(())
}

fn insert_remote_ingress_binding(
    tx: &Transaction<'_>,
    provider: &str,
    credential_identity: &str,
    authenticated_ingress: &str,
    channel_assurance: &str,
    status: &str,
    created_at: i64,
) -> Result<()> {
    let binding_id = framed_digest(&[
        b"carsinos.execass.remote-owner-binding.v1",
        provider.as_bytes(),
        credential_identity.as_bytes(),
        authenticated_ingress.as_bytes(),
        status.as_bytes(),
        &created_at.to_be_bytes(),
    ]);
    tx.execute(
        "INSERT INTO execass_owner_ingress_bindings (binding_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,provider_event_required,status,created_at) VALUES (?1,'human_remote',?2,?3,?4,1,?5,?6)",
        params![binding_id, credential_identity, authenticated_ingress, channel_assurance, status, created_at],
    )
    .context("recording immutable remote confirmation ingress generation")?;
    Ok(())
}

#[cfg(test)]
fn validate_attested_confirmation_command(
    command: &ConfirmDangerousActionCommand,
    trusted_resolved_at: i64,
) -> Result<()> {
    require_text("decision_id", &command.decision_id)?;
    require_text("grant_id", &command.grant_id)?;
    require_text(
        "selected_logical_action_id",
        &command.selected_logical_action_id,
    )?;
    if command.response != DangerousActionConfirmationResponse::ConfirmAndContinue {
        bail!("attested dangerous confirmation result must be confirm_and_continue");
    }
    if command.decision_revision <= 0 || trusted_resolved_at <= 0 {
        bail!("decision revision and trusted resolution time must be positive");
    }
    Ok(())
}

fn load_active_confirmation_key(conn: &Connection) -> Result<ActiveConfirmationKey> {
    let mut statement = conn.prepare(
        "SELECT key_id,key_generation,verifying_key_hex,verifying_key_digest,canonical_root_identity,installation_identity,os_user_identity_digest,state_root_generation FROM execass_confirmation_authority_keys WHERE status='active' ORDER BY key_generation,key_id LIMIT 2",
    )?;
    let rows = statement
        .query_map([], |row| {
            let key_generation = u64::try_from(row.get::<_, i64>(1)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(1, Type::Integer, Box::new(error))
            })?;
            let state_root_generation = u64::try_from(row.get::<_, i64>(7)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(7, Type::Integer, Box::new(error))
            })?;
            Ok(ActiveConfirmationKey {
                key_id: row.get(0)?,
                key_generation,
                verifying_key_hex: row.get(2)?,
                verifying_key_digest: row.get(3)?,
                canonical_root_identity: row.get(4)?,
                installation_identity: row.get(5)?,
                os_user_identity_digest: row.get(6)?,
                state_root_generation,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match rows.as_slice() {
        [key] => Ok(ActiveConfirmationKey {
            key_id: key.key_id.clone(),
            key_generation: key.key_generation,
            verifying_key_hex: key.verifying_key_hex.clone(),
            verifying_key_digest: key.verifying_key_digest.clone(),
            canonical_root_identity: key.canonical_root_identity.clone(),
            installation_identity: key.installation_identity.clone(),
            os_user_identity_digest: key.os_user_identity_digest.clone(),
            state_root_generation: key.state_root_generation,
        }),
        [] => bail!("canonical storage has no active confirmation authority key"),
        _ => bail!("canonical storage has multiple active confirmation authority keys"),
    }
}

fn decode_danger_hex<const N: usize>(value: &str) -> Result<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid fixed-length hexadecimal value");
    }
    let mut output = [0_u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow::anyhow!("invalid fixed-length hexadecimal value"))?;
    }
    Ok(output)
}

fn load_current_confirmation_binding(
    tx: &Transaction<'_>,
    decision_id: &str,
) -> Result<CurrentConfirmationBinding> {
    tx.query_row(
        "SELECT e.normalized_original_intent,e.policy_revision,d.policy_revision,p.policy_revision,p.manifest_digest,d.requested_at FROM execass_decisions d JOIN execass_delegations e ON e.delegation_id=d.delegation_id JOIN execass_plans p ON p.delegation_id=e.delegation_id AND p.plan_revision=e.current_plan_revision WHERE d.decision_id=?1",
        params![decision_id],
        |row| {
            Ok(CurrentConfirmationBinding {
                normalized_intent: row.get(0)?,
                delegation_policy_revision: row.get(1)?,
                decision_policy_revision: row.get(2)?,
                plan_policy_revision: row.get(3)?,
                manifest_digest: row.get(4)?,
                decision_requested_at: row.get(5)?,
            })
        },
    )
    .context("loading current attested confirmation binding")
}

#[allow(clippy::too_many_arguments)]
fn require_exact_attestation_binding(
    store: &ExecAssStore,
    command: &ConfirmDangerousActionCommand,
    challenge: &ConfirmationChallengeRecord,
    selected: &ExactConfirmationBinding,
    current: &CurrentConfirmationBinding,
    pinned: &ActiveConfirmationKey,
    verified: &VerifiedConfirmationAttestation,
    trusted_resolved_at: i64,
) -> Result<()> {
    let payload = verified.payload();
    let challenge_expiry = u64::try_from(challenge.expires_at)
        .context("stored challenge expiry does not fit attestation clock")?;
    let challenge_created = u64::try_from(challenge.created_at)
        .context("stored challenge creation does not fit attestation clock")?;
    let requested_at = u64::try_from(current.decision_requested_at)
        .context("stored decision request does not fit attestation clock")?;
    let trusted_now = u64::try_from(trusted_resolved_at)
        .context("trusted resolution time does not fit attestation clock")?;
    let policy_revision = u64::try_from(current.decision_policy_revision)
        .context("stored decision policy does not fit attestation revision")?;
    let decision_revision = u64::try_from(command.decision_revision)
        .context("stored decision revision does not fit attestation revision")?;
    let normalized_intent_digest = owner_normalized_intent_digest(&current.normalized_intent)
        .context("stored normalized intent is invalid")?;

    if challenge.status != ConfirmationChallengeStatus::Pending
        && challenge.status != ConfirmationChallengeStatus::Resolved
    {
        bail!("attested confirmation challenge is not resolvable");
    }
    if (challenge.status == ConfirmationChallengeStatus::Pending
        && (trusted_resolved_at >= challenge.expires_at
            || trusted_resolved_at < challenge.created_at))
        || payload.issued_at_ms < challenge_created
        || payload.issued_at_ms < requested_at
        || payload.issued_at_ms > trusted_now
        || payload.challenge_expires_at_ms != challenge_expiry
        || payload.decision_id != command.decision_id
        || payload.decision_revision != decision_revision
        || payload.decision_result != command.response.as_str()
        || payload.selected_logical_action_id != command.selected_logical_action_id
        || payload.selected_action_digest
            != sha256_hex(selected.exact_presented_action_json.as_bytes())
        || payload.declared_consequence_digest
            != sha256_hex(selected.declared_consequence.as_bytes())
        || payload.challenge_nonce_digest != challenge.nonce_digest
        || payload.canonical_manifest_digest != challenge.manifest_digest
        || payload.canonical_manifest_digest != selected.manifest_digest
        || payload.canonical_manifest_digest != current.manifest_digest
        || payload.normalized_intent_digest != normalized_intent_digest
        || payload.policy_revision != policy_revision
        || current.delegation_policy_revision != current.decision_policy_revision
        || current.plan_policy_revision != current.decision_policy_revision
        || payload.canonical_root_identity != store.root_identity
        || payload.canonical_root_identity != pinned.canonical_root_identity
        || payload.installation_identity != pinned.installation_identity
        || payload.os_user_identity_digest != pinned.os_user_identity_digest
        || payload.state_root_generation != pinned.state_root_generation
        || verified.key_id() != pinned.key_id
        || payload.signer_key_generation != pinned.key_generation
    {
        bail!("confirmation attestation is not bound to the exact live storage state");
    }
    Ok(())
}

fn require_exact_resolved_runtime_projection(
    store: &ExecAssStore,
    resolved: &ResolvedDangerConfirmationAlternativeBinding,
    pinned: &ActiveConfirmationKey,
    verified: &VerifiedConfirmationAttestation,
    stored_pinned_key_generation: u64,
) -> Result<()> {
    let binding = &resolved.binding;
    let payload = verified.payload();
    let policy_revision = u64::try_from(binding.policy_revision)
        .context("resolved confirmation policy revision is invalid")?;
    let decision_revision = u64::try_from(binding.decision_revision)
        .context("resolved confirmation decision revision is invalid")?;
    let expires_at =
        u64::try_from(binding.expires_at).context("resolved confirmation expiry is invalid")?;
    let normalized_intent_digest = owner_normalized_intent_digest(&binding.normalized_intent)
        .context("resolved confirmation normalized intent is invalid")?;
    if payload.decision_id != binding.decision_id
        || payload.decision_revision != decision_revision
        || payload.decision_result != "confirm_and_continue"
        || payload.selected_logical_action_id != binding.selected_logical_action_id
        || payload.selected_action_digest != binding.exact_selected_action_digest
        || payload.declared_consequence_digest != binding.declared_consequence_digest
        || payload.challenge_nonce_digest != binding.challenge_nonce_digest
        || payload.challenge_expires_at_ms != expires_at
        || payload.normalized_intent_digest != normalized_intent_digest
        || payload.policy_revision != policy_revision
        || payload.canonical_manifest_digest != binding.manifest_digest
        || payload.canonical_root_identity != store.root_identity
        || payload.canonical_root_identity != pinned.canonical_root_identity
        || payload.installation_identity != pinned.installation_identity
        || payload.os_user_identity_digest != pinned.os_user_identity_digest
        || payload.state_root_generation != pinned.state_root_generation
        || payload.signer_key_generation != pinned.key_generation
        || verified.key_id() != pinned.key_id
        || stored_pinned_key_generation != pinned.key_generation
        || resolved.grant.decision_id != binding.decision_id
        || resolved.grant.delegation_id != binding.delegation_id
        || resolved.grant.declared_consequence != binding.declared_consequence
        || resolved.grant.confirmation_attestation_digest != verified.attestation_digest()
        || resolved.grant.invalidated_at.is_some()
    {
        bail!("resolved confirmation projection does not match its verified immutable binding");
    }
    match payload.actor_type.as_str() {
        "human_local"
            if payload.source_message_id.is_none() && payload.provider_event_id.is_none() => {}
        "human_remote"
            if payload.source_message_id.is_some() && payload.provider_event_id.is_some() => {}
        _ => bail!("resolved confirmation payload has an invalid actor source shape"),
    }
    Ok(())
}

fn require_active_owner_ingress_binding(
    tx: &Transaction<'_>,
    payload: &super::confirmation_attestation::ConfirmationAttestationPayload,
) -> Result<()> {
    let provider_required = if payload.actor_type == "human_remote" {
        let latest = latest_remote_ingress_binding(tx, &payload.channel_assurance)?
            .context("attested remote owner ingress has no canonical binding generation")?;
        if latest.status != "active"
            || latest.credential_identity != payload.credential_identity
            || latest.authenticated_ingress != payload.authenticated_ingress
        {
            bail!("attested remote owner ingress is not the current configured generation");
        }
        1
    } else {
        tx.query_row(
            "SELECT provider_event_required FROM execass_owner_ingress_bindings WHERE actor_type=?1 AND credential_identity=?2 AND authenticated_ingress=?3 AND channel_assurance=?4 AND status='active'",
            params![payload.actor_type, payload.credential_identity, payload.authenticated_ingress, payload.channel_assurance],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("attested owner ingress does not match the canonical active binding")?
    };
    if provider_required == 1 && payload.provider_event_id.is_none() {
        bail!("remote owner ingress requires one signed provider event identity");
    }
    Ok(())
}

fn authority_record_from_verified_attestation(
    verified: &VerifiedConfirmationAttestation,
    manifest_digest: String,
    nonce_digest: String,
) -> Result<AuthorityProvenanceRecord> {
    let payload = verified.payload();
    let actor_type = match payload.actor_type.as_str() {
        "human_local" => ActorType::HumanLocal,
        "human_remote" => ActorType::HumanRemote,
        _ => bail!("verified attestation has unsupported actor type"),
    };
    let policy_revision = i64::try_from(payload.policy_revision)
        .context("attested policy revision does not fit storage")?;
    let decision_revision = i64::try_from(payload.decision_revision)
        .context("attested decision revision does not fit storage")?;
    let created_at = i64::try_from(payload.issued_at_ms)
        .context("attestation issuance does not fit storage clock")?;
    let expires_at = i64::try_from(payload.challenge_expires_at_ms)
        .context("attestation expiry does not fit storage clock")?;
    let authority_id = sha256_hex(
        [
            b"carsinos.execass.confirmation_attestation.authority.v1".as_slice(),
            verified.attestation_digest().as_bytes(),
        ]
        .concat()
        .as_slice(),
    );
    Ok(AuthorityProvenanceRecord {
        authority_provenance_id: format!("attested:{authority_id}"),
        actor_type,
        credential_identity: payload.credential_identity.clone(),
        authenticated_ingress: payload.authenticated_ingress.clone(),
        channel_assurance: payload.channel_assurance.clone(),
        source_correlation_id: payload.request_correlation_id.clone(),
        source_message_id: payload.source_message_id.clone(),
        authority_kind: AuthorityKind::DecisionResolution,
        normalized_scope_json: serde_json::json!({
            "decision_id": payload.decision_id,
            "selected_logical_action_id": payload.selected_logical_action_id,
        })
        .to_string(),
        policy_revision,
        bound_decision_id: Some(payload.decision_id.clone()),
        bound_decision_revision: Some(decision_revision),
        bound_manifest_digest: Some(manifest_digest),
        bound_challenge_nonce_digest: Some(nonce_digest),
        evidence_digest: verified.attestation_digest().to_string(),
        created_at,
        expires_at: Some(expires_at),
    })
}

fn insert_verified_confirmation_attestation(
    tx: &Transaction<'_>,
    attestation: &ConfirmationAttestation,
    verified: &VerifiedConfirmationAttestation,
    authority: &AuthorityProvenanceRecord,
    trusted_resolved_at: i64,
) -> Result<()> {
    let payload = verified.payload();
    tx.execute(
        "INSERT INTO execass_confirmation_attestations (attestation_digest,decision_id,authority_provenance_id,pinned_key_id,pinned_key_generation,actor_type,credential_identity,authenticated_ingress,channel_assurance,request_correlation_id,source_message_id,provider_event_id,selected_logical_action_id,signed_payload_json,signature_hex,issued_at,expires_at,verified_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![verified.attestation_digest(), payload.decision_id, authority.authority_provenance_id, verified.key_id(), payload.signer_key_generation, payload.actor_type, payload.credential_identity, payload.authenticated_ingress, payload.channel_assurance, payload.request_correlation_id, payload.source_message_id, payload.provider_event_id, payload.selected_logical_action_id, serde_json::to_string(payload)?, attestation.signature_hex, payload.issued_at_ms, payload.challenge_expires_at_ms, trusted_resolved_at],
    )
    .context("persisting strictly verified confirmation attestation")?;
    Ok(())
}

fn load_persisted_attestation_replay(
    tx: &Transaction<'_>,
    decision_id: &str,
) -> Result<Option<PersistedAttestationReplay>> {
    tx.query_row(
        "SELECT attestation_digest,selected_logical_action_id,signed_payload_json,signature_hex,verified_at FROM execass_confirmation_attestations WHERE decision_id=?1",
        params![decision_id],
        |row| {
            Ok(PersistedAttestationReplay {
                attestation_digest: row.get(0)?,
                selected_logical_action_id: row.get(1)?,
                signed_payload_json: row.get(2)?,
                signature_hex: row.get(3)?,
                verified_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn validate_present_command(command: &PresentDangerousActionConfirmationCommand) -> Result<()> {
    for (field, value) in [
        ("delegation_id", command.delegation_id.as_str()),
        ("logical_action_id", command.logical_action_id.as_str()),
        ("decision_id", command.decision_id.as_str()),
        ("challenge_id", command.challenge_id.as_str()),
        ("idempotency_key", command.idempotency_key.as_str()),
    ] {
        require_text(field, value)?;
    }
    if command.challenge_nonce.is_empty()
        || command.requested_at <= 0
        || command.expires_at <= command.requested_at
    {
        bail!("challenge nonce and positive increasing challenge times are required");
    }
    Ok(())
}

fn exact_leaf<'a>(
    manifest: &'a CanonicalLeafManifest,
    logical_action_id: &str,
) -> Result<&'a CanonicalLeafAction> {
    let mut matches = manifest
        .leaves()
        .iter()
        .filter(|leaf| leaf.logical_action_id() == logical_action_id);
    let leaf = matches
        .next()
        .context("logical action is absent from manifest")?;
    if matches.next().is_some() {
        bail!("logical action is not unique in manifest");
    }
    Ok(leaf)
}

#[derive(Clone, Copy)]
struct SavedRoutineBindingParts<'a> {
    routine_id: &'a str,
    canonical_selector_json: &'a str,
}

fn exact_binding(
    normalized_intent: String,
    manifest: &CanonicalLeafManifest,
    leaf: &CanonicalLeafAction,
    declared_consequence: &str,
    saved_routine: Option<&VerifiedSavedRoutineSelector>,
) -> Result<ExactConfirmationBinding> {
    let saved_parts = saved_routine.map(|selector| SavedRoutineBindingParts {
        routine_id: selector.routine_id(),
        canonical_selector_json: selector.canonical_selector_json(),
    });
    exact_binding_with_saved_parts(
        normalized_intent,
        manifest,
        leaf,
        declared_consequence,
        saved_parts,
    )
}

fn exact_binding_with_saved_parts(
    normalized_intent: String,
    manifest: &CanonicalLeafManifest,
    leaf: &CanonicalLeafAction,
    declared_consequence: &str,
    saved_routine: Option<SavedRoutineBindingParts<'_>>,
) -> Result<ExactConfirmationBinding> {
    require_text("normalized_intent", &normalized_intent)?;
    require_text("declared_consequence", declared_consequence)?;
    let operands = std::str::from_utf8(leaf.operands().bytes())?;
    let target = std::str::from_utf8(leaf.target_snapshot().bytes())?;
    let material = serde_json::to_string(&leaf.material_digest().map(|value| value.as_hex()))?;
    let payload_and_material_operands_json = if saved_routine.is_some() {
        format!(r#"{{"material_digest":{material},"operands":{operands}}}"#)
    } else {
        format!(
            r#"{{"material_digest":{material},"operands":{operands},"target_snapshot":{target}}}"#
        )
    };
    let payload_digest = sha256_hex(payload_and_material_operands_json.as_bytes());
    let canonical_action_envelope_or_selector_json = match saved_routine {
        Some(selector) => format!(
            r#"{{"action_kind":{},"mode":"saved_routine","payload_and_material_operands_digest":{},"routine_id":{},"selector":{}}}"#,
            serde_json::to_string(leaf.action_kind())?,
            serde_json::to_string(&payload_digest)?,
            serde_json::to_string(selector.routine_id)?,
            selector.canonical_selector_json,
        ),
        None => format!(
            r#"{{"action_kind":{},"mode":"exact","payload_and_material_operands_digest":{}}}"#,
            serde_json::to_string(leaf.action_kind())?,
            serde_json::to_string(&payload_digest)?,
        ),
    };
    let confirmed_logical_action_identity = framed_digest(&[
        normalized_intent.as_bytes(),
        leaf.action_kind().as_bytes(),
        leaf.tool().tool_id().as_bytes(),
        leaf.tool().version().as_bytes(),
        payload_digest.as_bytes(),
        canonical_action_envelope_or_selector_json.as_bytes(),
        declared_consequence.as_bytes(),
    ]);
    Ok(ExactConfirmationBinding {
        normalized_intent,
        exact_presented_action_json: std::str::from_utf8(leaf.canonical().bytes())?.to_string(),
        confirmed_logical_action_identity,
        manifest_digest: owner_resolution_manifest_digest(manifest.canonical().bytes())
            .context("manifest is empty")?,
        payload_digest,
        payload_and_material_operands_json,
        target_audience_path_json: target.to_string(),
        connector_tool_identity: leaf.tool().tool_id().to_string(),
        connector_tool_version: leaf.tool().version().to_string(),
        canonical_action_envelope_or_selector_json,
        declared_consequence: declared_consequence.to_string(),
    })
}

fn combined_bindings_from_verified_routes(
    manifest: &CanonicalLeafManifest,
    danger_routes: &[DangerRoute],
    normalized_intent: &str,
) -> Result<Vec<(String, ExactConfirmationBinding)>> {
    let mut bindings = Vec::with_capacity(danger_routes.len());
    for route in danger_routes {
        let mut matched = manifest.leaves().iter().filter_map(|leaf| {
            route
                .confirmation_for_leaf(leaf)
                .map(|assessment| (leaf, assessment))
        });
        let (leaf, assessment) = matched
            .next()
            .context("combined question route is not bound to a dangerous manifest leaf")?;
        if matched.next().is_some() || !assessment.requires_one_confirmation {
            bail!("combined question route must bind exactly one confirmed dangerous leaf");
        }
        bindings.push((
            leaf.logical_action_id().to_string(),
            exact_binding(
                normalized_intent.to_string(),
                manifest,
                leaf,
                &assessment.declared_consequence,
                None,
            )?,
        ));
    }
    bindings.sort_by(|left, right| left.0.cmp(&right.0));
    if bindings.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        bail!("combined question routes duplicate one dangerous alternative");
    }
    Ok(bindings)
}

fn combined_question_from_verified_routes(
    manifest: &CanonicalLeafManifest,
    danger_routes: &[DangerRoute],
) -> Result<CombinedDangerousActionQuestion> {
    if danger_routes.len() < 2 {
        bail!("combined question requires at least two verified dangerous alternatives");
    }

    let mut alternatives = Vec::with_capacity(danger_routes.len());
    for route in danger_routes {
        let mut matched = manifest.leaves().iter().filter_map(|leaf| {
            route
                .confirmation_for_leaf(leaf)
                .map(|assessment| (leaf, assessment))
        });
        let (leaf, assessment) = matched
            .next()
            .context("combined question route is not bound to a dangerous manifest leaf")?;
        if matched.next().is_some() {
            bail!("combined question route is ambiguously bound to multiple leaves");
        }
        if !assessment.requires_one_confirmation {
            bail!("combined question route must require exactly one confirmation");
        }
        alternatives.push(DisclosedDangerousAlternative {
            logical_action_id: leaf.logical_action_id().to_string(),
            exact_presented_action_json: std::str::from_utf8(leaf.canonical().bytes())?.to_string(),
            confirmed_logical_action_identity: String::new(),
            manifest_digest: String::new(),
            payload_digest: String::new(),
            payload_and_material_operands_json: String::new(),
            resolved_scope_json: std::str::from_utf8(leaf.target_snapshot().bytes())?.to_string(),
            connector_tool_identity: String::new(),
            connector_tool_version: String::new(),
            canonical_action_envelope_or_selector_json: String::new(),
            declared_consequence: assessment.declared_consequence.clone(),
        });
    }
    alternatives.sort_by(|left, right| left.logical_action_id.cmp(&right.logical_action_id));
    if alternatives
        .windows(2)
        .any(|pair| pair[0].logical_action_id == pair[1].logical_action_id)
    {
        bail!("combined question routes duplicate one dangerous alternative");
    }
    Ok(CombinedDangerousActionQuestion { alternatives })
}

fn canonical_combined_question_json(question: &CombinedDangerousActionQuestion) -> Result<String> {
    if question.alternatives.len() < 2 {
        bail!("combined question must disclose at least two dangerous alternatives");
    }
    let mut encoded_alternatives = Vec::with_capacity(question.alternatives.len());
    let mut previous_logical_action_id = None;
    for alternative in &question.alternatives {
        require_text(
            "combined alternative logical_action_id",
            &alternative.logical_action_id,
        )?;
        if previous_logical_action_id
            .is_some_and(|previous: &str| previous >= alternative.logical_action_id.as_str())
        {
            bail!("combined alternatives must have unique sorted logical action identities");
        }
        previous_logical_action_id = Some(alternative.logical_action_id.as_str());
        require_text(
            "combined alternative declared_consequence",
            &alternative.declared_consequence,
        )?;
        let exact_presented_action: serde_json::Value =
            serde_json::from_str(&alternative.exact_presented_action_json)
                .context("combined alternative action is not JSON")?;
        let resolved_scope: serde_json::Value =
            serde_json::from_str(&alternative.resolved_scope_json)
                .context("combined alternative resolved scope is not JSON")?;
        encoded_alternatives.push(format!(
            r#"{{"canonical_action_envelope_or_selector":{},"connector_tool_identity":{},"connector_tool_version":{},"confirmed_logical_action_identity":{},"declared_consequence":{},"exact_presented_action":{},"logical_action_id":{},"manifest_digest":{},"payload_and_material_operands":{},"payload_digest":{},"resolved_scope":{}}}"#,
            serde_json::to_string(&alternative.canonical_action_envelope_or_selector_json)?,
            serde_json::to_string(&alternative.connector_tool_identity)?,
            serde_json::to_string(&alternative.connector_tool_version)?,
            serde_json::to_string(&alternative.confirmed_logical_action_identity)?,
            serde_json::to_string(&alternative.declared_consequence)?,
            serde_json::to_string(&exact_presented_action)?,
            serde_json::to_string(&alternative.logical_action_id)?,
            serde_json::to_string(&alternative.manifest_digest)?,
            serde_json::to_string(&alternative.payload_and_material_operands_json)?,
            serde_json::to_string(&alternative.payload_digest)?,
            serde_json::to_string(&resolved_scope)?,
        ));
    }
    Ok(format!(
        r#"{{"alternatives":[{}],"kind":"combined_dangerous_action_confirmation","outcomes":["confirm_and_continue","revise","decline"]}}"#,
        encoded_alternatives.join(","),
    ))
}

fn combined_question_with_bindings(
    question: &CombinedDangerousActionQuestion,
    bindings: &[(String, ExactConfirmationBinding)],
) -> Result<CombinedDangerousActionQuestion> {
    let alternatives = question
        .alternatives()
        .iter()
        .map(|alternative| {
            let binding = bindings
                .iter()
                .find_map(|(logical_action_id, binding)| {
                    (logical_action_id == alternative.logical_action_id()).then_some(binding)
                })
                .context("combined disclosure alternative has no exact binding")?;
            Ok(DisclosedDangerousAlternative {
                logical_action_id: alternative.logical_action_id.clone(),
                exact_presented_action_json: alternative.exact_presented_action_json.clone(),
                confirmed_logical_action_identity: binding
                    .confirmed_logical_action_identity
                    .clone(),
                manifest_digest: binding.manifest_digest.clone(),
                payload_digest: binding.payload_digest.clone(),
                payload_and_material_operands_json: binding
                    .payload_and_material_operands_json
                    .clone(),
                resolved_scope_json: alternative.resolved_scope_json.clone(),
                connector_tool_identity: binding.connector_tool_identity.clone(),
                connector_tool_version: binding.connector_tool_version.clone(),
                canonical_action_envelope_or_selector_json: binding
                    .canonical_action_envelope_or_selector_json
                    .clone(),
                declared_consequence: alternative.declared_consequence.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CombinedDangerousActionQuestion { alternatives })
}

fn validate_combined_question_binding(
    question: &CombinedDangerousActionQuestion,
    bindings: &[(String, ExactConfirmationBinding)],
) -> Result<()> {
    if question.alternatives().len() != bindings.len() {
        bail!("combined question does not bind every disclosed alternative");
    }
    for alternative in question.alternatives() {
        let binding = bindings
            .iter()
            .find_map(|(logical_action_id, binding)| {
                (logical_action_id == alternative.logical_action_id()).then_some(binding)
            })
            .context("combined question alternative has no immutable binding")?;
        if alternative.exact_presented_action_json() != binding.exact_presented_action_json
            || alternative.resolved_scope_json() != binding.target_audience_path_json
            || alternative.declared_consequence() != binding.declared_consequence
        {
            bail!("combined question alternative does not exactly bind the challenge");
        }
    }
    Ok(())
}

fn combined_question_from_canonical_json(
    alternatives_json: &str,
) -> Result<Option<CombinedDangerousActionQuestion>> {
    if alternatives_json == r#"["confirm_and_continue","revise","decline"]"# {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(alternatives_json)
        .context("stored combined question alternatives are not JSON")?;
    let object = value
        .as_object()
        .context("stored decision alternatives are not a combined question")?;
    if object.get("kind").and_then(serde_json::Value::as_str)
        != Some("combined_dangerous_action_confirmation")
        || object.get("outcomes")
            != Some(&serde_json::json!([
                "confirm_and_continue",
                "revise",
                "decline"
            ]))
    {
        bail!("stored combined question has an unsupported shape");
    }
    let alternatives = object
        .get("alternatives")
        .and_then(serde_json::Value::as_array)
        .context("stored combined question omits alternatives")?
        .iter()
        .map(|value| {
            let alternative = value
                .as_object()
                .context("stored combined alternative is not an object")?;
            Ok(DisclosedDangerousAlternative {
                logical_action_id: alternative
                    .get("logical_action_id")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits logical action")?
                    .to_string(),
                exact_presented_action_json: serde_json::to_string(
                    alternative
                        .get("exact_presented_action")
                        .context("stored combined alternative omits exact action")?,
                )?,
                confirmed_logical_action_identity: alternative
                    .get("confirmed_logical_action_identity")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits identity")?
                    .to_string(),
                manifest_digest: alternative
                    .get("manifest_digest")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits manifest digest")?
                    .to_string(),
                payload_digest: alternative
                    .get("payload_digest")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits payload digest")?
                    .to_string(),
                payload_and_material_operands_json: alternative
                    .get("payload_and_material_operands")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits payload")?
                    .to_string(),
                resolved_scope_json: serde_json::to_string(
                    alternative
                        .get("resolved_scope")
                        .context("stored combined alternative omits resolved scope")?,
                )?,
                connector_tool_identity: alternative
                    .get("connector_tool_identity")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits tool identity")?
                    .to_string(),
                connector_tool_version: alternative
                    .get("connector_tool_version")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits tool version")?
                    .to_string(),
                canonical_action_envelope_or_selector_json: alternative
                    .get("canonical_action_envelope_or_selector")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits action envelope")?
                    .to_string(),
                declared_consequence: alternative
                    .get("declared_consequence")
                    .and_then(serde_json::Value::as_str)
                    .context("stored combined alternative omits consequence")?
                    .to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let question = CombinedDangerousActionQuestion { alternatives };
    if canonical_combined_question_json(&question)? != alternatives_json {
        bail!("stored combined question is not canonical");
    }
    Ok(Some(question))
}

/// Reconcile only active grants whose persisted logical action still occurs as
/// a dangerous leaf in the exact replacement manifest.  No new grant or
/// challenge is created here: an unchanged grant survives byte-for-byte, and
/// a materially drifted matching action is invalidated in the caller's
/// amendment transaction.
pub(super) fn reconcile_accepted_confirmation_grants_for_amendment_in_tx(
    tx: &Transaction<'_>,
    delegation_id: &str,
    manifest: &CanonicalLeafManifest,
    routes: &[DangerRoute],
    invalidated_at: i64,
    _amendment_authority_provenance_id: &str,
) -> Result<()> {
    if routes.len() != manifest.leaves().len() {
        bail!("amendment danger routes do not cover the exact replacement manifest");
    }
    let normalized_intent: String = tx.query_row(
        "SELECT normalized_original_intent FROM execass_delegations WHERE delegation_id=?1",
        [delegation_id],
        |row| row.get(0),
    )?;
    let bindings = manifest
        .leaves()
        .iter()
        .zip(routes)
        .filter_map(|(leaf, route)| {
            route.confirmation_for_leaf(leaf).map(|assessment| {
                Ok((
                    leaf.logical_action_id().to_string(),
                    exact_binding(
                        normalized_intent.clone(),
                        manifest,
                        leaf,
                        &assessment.declared_consequence,
                        None,
                    )?,
                ))
            })
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;

    let mut statement = tx.prepare(
        "SELECT g.grant_id,g.delegation_id,g.decision_id,g.confirmed_logical_action_identity,g.canonical_action_envelope_or_selector_json,g.payload_and_material_operands_json,g.payload_and_material_operands_digest,g.connector_tool_identity,g.connector_tool_version,g.declared_consequence,g.accepted_by_authority_provenance_id,g.confirmation_attestation_digest,g.accepted_at,g.invalidated_at,g.invalidation_reason,g.invalidated_by_authority_provenance_id,d.exact_presented_action_json FROM execass_accepted_confirmation_grants g JOIN execass_decisions d ON d.decision_id=g.decision_id WHERE g.delegation_id=?1 AND g.invalidated_at IS NULL",
    )?;
    let grants = statement
        .query_map([delegation_id], |row| {
            Ok((grant_from_row(row)?, row.get::<_, String>(16)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (grant, exact_presented_action_json) in grants {
        let logical_action_id =
            serde_json::from_str::<serde_json::Value>(&exact_presented_action_json)
                .context("accepted grant action identity is not canonical JSON")?
                .get("logical_action_id")
                .and_then(serde_json::Value::as_str)
                .context("accepted grant action identity is absent")?
                .to_string();
        let Some(next) = bindings.get(&logical_action_id) else {
            // The amended plan no longer presents this action as dangerous. An
            // old accepted grant cannot authorize an absent action, and this
            // path deliberately creates neither a replacement grant nor a
            // generic approval.
            continue;
        };
        if grant_matches_binding(&grant, next) {
            continue;
        }
        let reason = material_drift_reason(&grant, next)?;
        let changed = tx.execute(
            "UPDATE execass_accepted_confirmation_grants SET invalidated_at=?2,invalidation_reason=?3,invalidated_by_authority_provenance_id=NULL WHERE grant_id=?1 AND invalidated_at IS NULL",
            params![grant.grant_id, invalidated_at, reason.as_str()],
        )?;
        if changed != 1 {
            bail!("accepted confirmation grant changed during amendment reconciliation");
        }
    }
    Ok(())
}

fn grant_matches_binding(
    grant: &AcceptedConfirmationGrantRecord,
    binding: &ExactConfirmationBinding,
) -> bool {
    grant.confirmed_logical_action_identity == binding.confirmed_logical_action_identity
        && grant.canonical_action_envelope_or_selector_json
            == binding.canonical_action_envelope_or_selector_json
        && grant.payload_and_material_operands_json == binding.payload_and_material_operands_json
        && grant.payload_and_material_operands_digest == binding.payload_digest
        && grant.connector_tool_identity.as_deref() == Some(&binding.connector_tool_identity)
        && grant.connector_tool_version.as_deref() == Some(&binding.connector_tool_version)
        && grant.declared_consequence == binding.declared_consequence
}

fn material_drift_reason(
    grant: &AcceptedConfirmationGrantRecord,
    binding: &ExactConfirmationBinding,
) -> Result<AcceptedConfirmationGrantInvalidation> {
    let old_payload: serde_json::Value =
        serde_json::from_str(&grant.payload_and_material_operands_json)
            .context("accepted grant payload binding is not canonical JSON")?;
    let new_payload: serde_json::Value =
        serde_json::from_str(&binding.payload_and_material_operands_json)
            .context("replacement action payload binding is not canonical JSON")?;
    if old_payload.get("target_snapshot") != new_payload.get("target_snapshot") {
        return Ok(AcceptedConfirmationGrantInvalidation::MaterialTargetDrift);
    }
    let old_envelope: serde_json::Value =
        serde_json::from_str(&grant.canonical_action_envelope_or_selector_json)
            .context("accepted grant scope binding is not canonical JSON")?;
    let new_envelope: serde_json::Value =
        serde_json::from_str(&binding.canonical_action_envelope_or_selector_json)
            .context("replacement action scope binding is not canonical JSON")?;
    if confirmation_action_scope(&old_envelope) != confirmation_action_scope(&new_envelope) {
        return Ok(AcceptedConfirmationGrantInvalidation::MaterialScopeDrift);
    }
    let old_payload_without_target = without_json_field(&old_payload, "target_snapshot")?;
    let new_payload_without_target = without_json_field(&new_payload, "target_snapshot")?;
    if old_payload_without_target != new_payload_without_target {
        return Ok(AcceptedConfirmationGrantInvalidation::MaterialPayloadDrift);
    }
    if grant.connector_tool_identity.as_deref() != Some(&binding.connector_tool_identity)
        || grant.connector_tool_version.as_deref() != Some(&binding.connector_tool_version)
    {
        return Ok(AcceptedConfirmationGrantInvalidation::MaterialToolDrift);
    }
    if grant.declared_consequence != binding.declared_consequence {
        return Ok(AcceptedConfirmationGrantInvalidation::MaterialConsequenceDrift);
    }
    Ok(AcceptedConfirmationGrantInvalidation::MaterialPayloadDrift)
}

fn confirmation_action_scope(envelope: &serde_json::Value) -> serde_json::Value {
    let mut scope = serde_json::Map::new();
    for key in ["action_kind", "mode", "routine_id", "selector"] {
        if let Some(value) = envelope.get(key) {
            scope.insert(key.to_string(), value.clone());
        }
    }
    serde_json::Value::Object(scope)
}

fn without_json_field(value: &serde_json::Value, field: &str) -> Result<serde_json::Value> {
    let mut value = value.clone();
    value
        .as_object_mut()
        .context("confirmation payload binding is not an object")?
        .remove(field);
    Ok(value)
}

fn find_active_grant(
    tx: &Transaction<'_>,
    binding: &ExactConfirmationBinding,
    current_delegation_id: &str,
    cross_delegation_reuse: bool,
) -> Result<Option<AcceptedConfirmationGrantRecord>> {
    let mut statement = tx.prepare(
        "SELECT g.grant_id,g.delegation_id,g.decision_id,g.confirmed_logical_action_identity,g.canonical_action_envelope_or_selector_json,g.payload_and_material_operands_json,g.payload_and_material_operands_digest,g.connector_tool_identity,g.connector_tool_version,g.declared_consequence,g.accepted_by_authority_provenance_id,g.confirmation_attestation_digest,g.accepted_at,g.invalidated_at,g.invalidation_reason,g.invalidated_by_authority_provenance_id FROM execass_accepted_confirmation_grants g JOIN execass_delegations d ON d.delegation_id=g.delegation_id WHERE d.normalized_original_intent=?1 AND g.confirmed_logical_action_identity=?2 AND g.canonical_action_envelope_or_selector_json=?3 AND g.payload_and_material_operands_digest=?4 AND g.connector_tool_identity=?5 AND g.connector_tool_version=?6 AND g.declared_consequence=?7 AND (?8=1 OR g.delegation_id=?9) AND g.invalidated_at IS NULL ORDER BY g.accepted_at,g.grant_id LIMIT 2",
    )?;
    let rows = statement
        .query_map(
            params![
                binding.normalized_intent,
                binding.confirmed_logical_action_identity,
                binding.canonical_action_envelope_or_selector_json,
                binding.payload_digest,
                binding.connector_tool_identity,
                binding.connector_tool_version,
                binding.declared_consequence,
                i64::from(cross_delegation_reuse),
                current_delegation_id,
            ],
            grant_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match rows.as_slice() {
        [] => Ok(None),
        [grant] => Ok(Some(grant.clone())),
        _ => bail!("multiple active grants cover one exact confirmation identity"),
    }
}

/// Derive the saved-routine confirmation identity only from an immutable
/// persisted version and the currently resolved canonical leaf. This grants no
/// new authority: it requires the exact pinned, still-active grant row.
pub(super) fn require_pinned_saved_routine_grant_in_tx(
    tx: &Transaction<'_>,
    version: &RoutineVersionRecord,
    manifest: &CanonicalLeafManifest,
    leaf: &CanonicalLeafAction,
    declared_consequence: &str,
) -> Result<AcceptedConfirmationGrantRecord> {
    if saved_routine_stable_leaf_digest(leaf) != version.stable_leaf_digest {
        bail!("routine occurrence materially differs from its saved stable leaf");
    }
    let grant_id = version
        .accepted_confirmation_grant_id
        .as_deref()
        .context("dangerous routine occurrence has no pinned accepted confirmation grant")?;
    let binding = exact_binding_with_saved_parts(
        version.normalized_original_intent.clone(),
        manifest,
        leaf,
        declared_consequence,
        Some(SavedRoutineBindingParts {
            routine_id: &version.routine_id,
            canonical_selector_json: &version.saved_selector_json,
        }),
    )?;
    let grant = find_grant_by_id(tx, grant_id)?
        .context("pinned routine confirmation grant does not exist")?;
    if grant.delegation_id != version.source_delegation_id
        || grant.invalidated_at.is_some()
        || grant.confirmed_logical_action_identity != binding.confirmed_logical_action_identity
        || grant.canonical_action_envelope_or_selector_json
            != binding.canonical_action_envelope_or_selector_json
        || grant.payload_and_material_operands_json != binding.payload_and_material_operands_json
        || grant.payload_and_material_operands_digest != binding.payload_digest
        || grant.connector_tool_identity.as_deref() != Some(&binding.connector_tool_identity)
        || grant.connector_tool_version.as_deref() != Some(&binding.connector_tool_version)
        || grant.declared_consequence != binding.declared_consequence
    {
        bail!("pinned routine confirmation grant does not cover the exact current action");
    }
    Ok(grant)
}

fn expire_matching_pending_challenges(
    tx: &Transaction<'_>,
    binding: &ExactConfirmationBinding,
    current_delegation_id: &str,
    cross_delegation_reuse: bool,
    observed_at: i64,
) -> Result<()> {
    let mut statement = tx.prepare(
        "SELECT c.challenge_id,c.decision_id FROM execass_confirmation_challenges c JOIN execass_decisions x ON x.decision_id=c.decision_id JOIN execass_delegations d ON d.delegation_id=c.delegation_id WHERE d.normalized_original_intent=?1 AND c.confirmed_logical_action_identity=?2 AND c.canonical_action_envelope_or_selector_json=?3 AND c.payload_digest=?4 AND c.connector_tool_identity=?5 AND c.connector_tool_version=?6 AND c.declared_consequence=?7 AND (?8=1 OR c.delegation_id=?9) AND c.status='pending' AND c.expires_at<=?10",
    )?;
    let expired = statement
        .query_map(
            params![
                binding.normalized_intent,
                binding.confirmed_logical_action_identity,
                binding.canonical_action_envelope_or_selector_json,
                binding.payload_digest,
                binding.connector_tool_identity,
                binding.connector_tool_version,
                binding.declared_consequence,
                i64::from(cross_delegation_reuse),
                current_delegation_id,
                observed_at,
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (challenge_id, decision_id) in expired {
        tx.execute(
            "UPDATE execass_confirmation_challenges SET status='expired' WHERE challenge_id=?1 AND status='pending'",
            params![challenge_id],
        )?;
        tx.execute(
            "UPDATE execass_decisions SET status='expired' WHERE decision_id=?1 AND status='pending'",
            params![decision_id],
        )?;
    }
    Ok(())
}

fn find_pending_challenge(
    tx: &Transaction<'_>,
    binding: &ExactConfirmationBinding,
    current_delegation_id: &str,
    cross_delegation_reuse: bool,
) -> Result<Option<ConfirmationChallengeRecord>> {
    let mut statement = tx.prepare(
        "SELECT c.challenge_id,c.decision_id,c.delegation_id,c.decision_revision,c.exact_presented_action_json,c.confirmed_logical_action_identity,c.manifest_digest,c.payload_digest,c.payload_and_material_operands_json,c.connector_tool_identity,c.connector_tool_version,c.canonical_action_envelope_or_selector_json,c.declared_consequence,c.nonce_digest,c.status,c.created_at,c.expires_at,c.resolved_at FROM execass_confirmation_challenges c JOIN execass_delegations d ON d.delegation_id=c.delegation_id WHERE d.normalized_original_intent=?1 AND c.confirmed_logical_action_identity=?2 AND c.canonical_action_envelope_or_selector_json=?3 AND c.payload_digest=?4 AND c.connector_tool_identity=?5 AND c.connector_tool_version=?6 AND c.declared_consequence=?7 AND (?8=1 OR c.delegation_id=?9) AND c.status='pending' ORDER BY c.created_at,c.challenge_id LIMIT 2",
    )?;
    let rows = statement
        .query_map(
            params![
                binding.normalized_intent,
                binding.confirmed_logical_action_identity,
                binding.canonical_action_envelope_or_selector_json,
                binding.payload_digest,
                binding.connector_tool_identity,
                binding.connector_tool_version,
                binding.declared_consequence,
                i64::from(cross_delegation_reuse),
                current_delegation_id,
            ],
            challenge_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match rows.as_slice() {
        [] => Ok(None),
        [challenge] => Ok(Some(challenge.clone())),
        _ => bail!("multiple pending challenges cover one exact confirmation identity"),
    }
}

fn read_challenge(
    tx: &Transaction<'_>,
    challenge_id: &str,
) -> Result<Option<ConfirmationChallengeRecord>> {
    tx.query_row(
        "SELECT challenge_id,decision_id,delegation_id,decision_revision,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence,nonce_digest,status,created_at,expires_at,resolved_at FROM execass_confirmation_challenges WHERE challenge_id=?1",
        params![challenge_id],
        challenge_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn find_challenge_by_decision(
    tx: &Connection,
    decision_id: &str,
) -> Result<Option<ConfirmationChallengeRecord>> {
    tx.query_row(
        "SELECT challenge_id,decision_id,delegation_id,decision_revision,exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence,nonce_digest,status,created_at,expires_at,resolved_at FROM execass_confirmation_challenges WHERE decision_id=?1",
        params![decision_id],
        challenge_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn read_challenge_alternative(
    tx: &Transaction<'_>,
    challenge_id: &str,
    logical_action_id: &str,
) -> Result<Option<ExactConfirmationBinding>> {
    tx.query_row(
        "SELECT exact_presented_action_json,confirmed_logical_action_identity,manifest_digest,payload_digest,payload_and_material_operands_json,target_audience_path_json,connector_tool_identity,connector_tool_version,canonical_action_envelope_or_selector_json,declared_consequence FROM execass_confirmation_challenge_alternatives WHERE challenge_id=?1 AND logical_action_id=?2",
        params![challenge_id, logical_action_id],
        |row| Ok(ExactConfirmationBinding {
            normalized_intent: String::new(),
            exact_presented_action_json: row.get(0)?,
            confirmed_logical_action_identity: row.get(1)?,
            manifest_digest: row.get(2)?,
            payload_digest: row.get(3)?,
            payload_and_material_operands_json: row.get(4)?,
            target_audience_path_json: row.get(5)?,
            connector_tool_identity: row.get(6)?,
            connector_tool_version: row.get(7)?,
            canonical_action_envelope_or_selector_json: row.get(8)?,
            declared_consequence: row.get(9)?,
        }),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn find_grant_by_decision(
    tx: &Transaction<'_>,
    decision_id: &str,
) -> Result<Option<AcceptedConfirmationGrantRecord>> {
    tx.query_row(
        "SELECT grant_id,delegation_id,decision_id,confirmed_logical_action_identity,canonical_action_envelope_or_selector_json,payload_and_material_operands_json,payload_and_material_operands_digest,connector_tool_identity,connector_tool_version,declared_consequence,accepted_by_authority_provenance_id,confirmation_attestation_digest,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id FROM execass_accepted_confirmation_grants WHERE decision_id=?1",
        params![decision_id],
        grant_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn find_grant_by_id(
    tx: &Transaction<'_>,
    grant_id: &str,
) -> Result<Option<AcceptedConfirmationGrantRecord>> {
    tx.query_row(
        "SELECT grant_id,delegation_id,decision_id,confirmed_logical_action_identity,canonical_action_envelope_or_selector_json,payload_and_material_operands_json,payload_and_material_operands_digest,connector_tool_identity,connector_tool_version,declared_consequence,accepted_by_authority_provenance_id,confirmation_attestation_digest,accepted_at,invalidated_at,invalidation_reason,invalidated_by_authority_provenance_id FROM execass_accepted_confirmation_grants WHERE grant_id=?1",
        params![grant_id],
        grant_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn challenge_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConfirmationChallengeRecord> {
    Ok(ConfirmationChallengeRecord {
        challenge_id: row.get(0)?,
        decision_id: row.get(1)?,
        delegation_id: row.get(2)?,
        decision_revision: row.get(3)?,
        exact_presented_action_json: row.get(4)?,
        confirmed_logical_action_identity: row.get(5)?,
        manifest_digest: row.get(6)?,
        payload_digest: row.get(7)?,
        payload_and_material_operands_json: row.get(8)?,
        connector_tool_identity: row.get(9)?,
        connector_tool_version: row.get(10)?,
        canonical_action_envelope_or_selector_json: row.get(11)?,
        declared_consequence: row.get(12)?,
        nonce_digest: row.get(13)?,
        status: row.get(14)?,
        created_at: row.get(15)?,
        expires_at: row.get(16)?,
        resolved_at: row.get(17)?,
    })
}

fn grant_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AcceptedConfirmationGrantRecord> {
    Ok(AcceptedConfirmationGrantRecord {
        grant_id: row.get(0)?,
        delegation_id: row.get(1)?,
        decision_id: row.get(2)?,
        confirmed_logical_action_identity: row.get(3)?,
        canonical_action_envelope_or_selector_json: row.get(4)?,
        payload_and_material_operands_json: row.get(5)?,
        payload_and_material_operands_digest: row.get(6)?,
        connector_tool_identity: row.get(7)?,
        connector_tool_version: row.get(8)?,
        declared_consequence: row.get(9)?,
        accepted_by_authority_provenance_id: row.get(10)?,
        confirmation_attestation_digest: row.get(11)?,
        accepted_at: row.get(12)?,
        invalidated_at: row.get(13)?,
        invalidation_reason: row.get(14)?,
        invalidated_by_authority_provenance_id: row.get(15)?,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn framed_digest(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"carsinos.execass.confirmation_action_identity.v1");
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn trusted_unix_time_ms() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock precedes unix epoch")?;
    i64::try_from(duration.as_millis()).context("system time exceeds supported range")
}

#[cfg(test)]
mod amendment_reconciliation_tests {
    use super::*;

    fn grant() -> AcceptedConfirmationGrantRecord {
        AcceptedConfirmationGrantRecord {
            grant_id: "grant-1".into(),
            delegation_id: "delegation-1".into(),
            decision_id: "decision-1".into(),
            confirmed_logical_action_identity: "identity-1".into(),
            canonical_action_envelope_or_selector_json: r#"{"action_kind":"tool_call","mode":"exact","payload_and_material_operands_digest":"payload-1"}"#.into(),
            payload_and_material_operands_json: r#"{"material_digest":null,"operands":{},"target_snapshot":{"targets":["target-1"]}}"#.into(),
            payload_and_material_operands_digest: "payload-1".into(),
            connector_tool_identity: Some("tool-1".into()),
            connector_tool_version: Some("1".into()),
            declared_consequence: "consequence-1".into(),
            accepted_by_authority_provenance_id: "authority-1".into(),
            confirmation_attestation_digest: "attestation-1".into(),
            accepted_at: 1,
            invalidated_at: None,
            invalidation_reason: None,
            invalidated_by_authority_provenance_id: None,
        }
    }

    fn binding() -> ExactConfirmationBinding {
        ExactConfirmationBinding {
            normalized_intent: "intent".into(),
            exact_presented_action_json: "{}".into(),
            confirmed_logical_action_identity: "identity-1".into(),
            manifest_digest: "manifest-2".into(),
            payload_digest: "payload-1".into(),
            payload_and_material_operands_json: r#"{"material_digest":null,"operands":{},"target_snapshot":{"targets":["target-1"]}}"#.into(),
            target_audience_path_json: r#"{"targets":["target-1"]}"#.into(),
            connector_tool_identity: "tool-1".into(),
            connector_tool_version: "1".into(),
            canonical_action_envelope_or_selector_json: r#"{"action_kind":"tool_call","mode":"exact","payload_and_material_operands_digest":"payload-1"}"#.into(),
            declared_consequence: "consequence-1".into(),
        }
    }

    #[test]
    fn amendment_reconciliation_assigns_each_material_drift_one_exact_reason() {
        let current = grant();
        let cases = [
            (
                "target",
                ExactConfirmationBinding {
                    payload_and_material_operands_json: r#"{"material_digest":null,"operands":{},"target_snapshot":{"targets":["target-2"]}}"#.into(),
                    ..binding()
                },
                AcceptedConfirmationGrantInvalidation::MaterialTargetDrift,
            ),
            (
                "action scope",
                ExactConfirmationBinding {
                    canonical_action_envelope_or_selector_json: r#"{"action_kind":"other_tool_call","mode":"exact","payload_and_material_operands_digest":"payload-1"}"#.into(),
                    ..binding()
                },
                AcceptedConfirmationGrantInvalidation::MaterialScopeDrift,
            ),
            (
                "payload material",
                ExactConfirmationBinding {
                    payload_and_material_operands_json: r#"{"material_digest":"different","operands":{},"target_snapshot":{"targets":["target-1"]}}"#.into(),
                    ..binding()
                },
                AcceptedConfirmationGrantInvalidation::MaterialPayloadDrift,
            ),
            (
                "tool version",
                ExactConfirmationBinding {
                    connector_tool_version: "2".into(),
                    ..binding()
                },
                AcceptedConfirmationGrantInvalidation::MaterialToolDrift,
            ),
            (
                "consequence",
                ExactConfirmationBinding {
                    declared_consequence: "consequence-2".into(),
                    ..binding()
                },
                AcceptedConfirmationGrantInvalidation::MaterialConsequenceDrift,
            ),
        ];
        for (name, replacement, expected) in cases {
            assert_eq!(
                material_drift_reason(&current, &replacement).unwrap(),
                expected,
                "{name}"
            );
        }
    }
}
