//! Deterministic EA-206 danger routing for exact resolved ExecAss leaves.
//!
//! The matcher consumes opaque, server-verified system metadata bound to one
//! canonical leaf. It never classifies raw request wording, item counts, tool
//! categories, money, generic risk labels, or model scores. Its output has no
//! refusal state: a dangerous action routes to one confirmation, then later
//! phases may make the confirmed action runnable.

#![cfg_attr(not(any(test, feature = "execass-test-authority")), allow(dead_code))]

use crate::execass_manifest::CanonicalLeafAction;
use crate::execass_manifest::CanonicalLeafManifest;
use carsinos_protocol::execass::{DangerAssessment, DangerSource, KnownDangerCategory};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
enum VerifiedDangerFact {
    WholeDriveVolumeBootRecoveryOrCoreOsTree { resolved_target: String },
    WholeUserProfileOrHome { resolved_target: String },
    CompleteCarsinosProtectedSystem { resolved_target: String },
    WholeConnectedExternalAccountOrTenant { resolved_target: String },
    LastAdministrativeRecoveryOrDecryptionPath { resolved_target: String },
}

impl VerifiedDangerFact {
    fn category(&self) -> KnownDangerCategory {
        match self {
            Self::WholeDriveVolumeBootRecoveryOrCoreOsTree { .. } => {
                KnownDangerCategory::WholeDriveVolumeBootRecoveryOrCoreOsTreeErasureOrUnusable
            }
            Self::WholeUserProfileOrHome { .. } => {
                KnownDangerCategory::WholeUserProfileOrHomeErasureOrUnusable
            }
            Self::CompleteCarsinosProtectedSystem { .. } => {
                KnownDangerCategory::CompleteCarsinosStateIntegrityRuntimeEnforcementStopFencingOrRecoveryConfigurationErasureOrUnusable
            }
            Self::WholeConnectedExternalAccountOrTenant { .. } => {
                KnownDangerCategory::WholeConnectedExternalAccountClosureOrErasure
            }
            Self::LastAdministrativeRecoveryOrDecryptionPath { .. } => {
                KnownDangerCategory::LastVerifiedAdministrativeRecoveryOrDecryptionPathDestruction
            }
        }
    }

    fn consequence(&self) -> String {
        match self {
            Self::WholeDriveVolumeBootRecoveryOrCoreOsTree { resolved_target } => format!(
                "This will erase or make unusable the entire drive, volume, boot/recovery environment, or core operating-system target `{resolved_target}`, including the operating system and data it contains."
            ),
            Self::WholeUserProfileOrHome { resolved_target } => format!(
                "This will erase or make unusable the entire operating-system user profile or home `{resolved_target}`, including all data stored in it."
            ),
            Self::CompleteCarsinosProtectedSystem { resolved_target } => format!(
                "This will erase, corrupt, or disable the complete CarsinOS protected state `{resolved_target}`, including the integrity, runtime-enforcement, stop/fencing, or recovery information required to recover it."
            ),
            Self::WholeConnectedExternalAccountOrTenant { resolved_target } => format!(
                "This will erase or close the entire connected external account or tenant `{resolved_target}` and make all resources owned only by it unavailable."
            ),
            Self::LastAdministrativeRecoveryOrDecryptionPath { resolved_target } => format!(
                "This will destroy the last verified administrative, recovery, or decryption path `{resolved_target}` for owner data that is otherwise unrecoverable."
            ),
        }
    }
}

/// Opaque server-verified danger metadata bound to one exact canonical leaf.
/// Production callers cannot manufacture a category or reuse metadata for a
/// changed action. A trusted platform/account/recovery resolver will own live
/// issuance at the later intake/runtime integration boundary.
///
/// ```compile_fail
/// use carsinos_core::execass_danger::VerifiedDangerSystemMetadata;
///
/// let _forged = VerifiedDangerSystemMetadata {
///     canonical_leaf_digest: "caller-selected".to_string(),
///     facts: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDangerSystemMetadata {
    canonical_leaf_digest: String,
    facts: Vec<VerifiedDangerFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredibleModelDangerSignal {
    canonical_leaf_digest: String,
    resolved_target: String,
    material_result: String,
}

/// Bounded material outcomes that a server-owned model adapter may report.
/// This is intentionally not free-form model prose, a risk score, a category,
/// or a refusal decision.  Every variant can only add the existing single
/// concrete-consequence confirmation route for one frozen target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoritativeModelDangerResult {
    DestroyExactTarget,
    RenderExactTargetUnusable,
    IrreversiblyRemoveAccessToExactTarget,
}

impl AuthoritativeModelDangerResult {
    fn material_result(self) -> &'static str {
        match self {
            Self::DestroyExactTarget => "destroy the exact target",
            Self::RenderExactTargetUnusable => "render the exact target unusable",
            Self::IrreversiblyRemoveAccessToExactTarget => {
                "irreversibly remove access to the exact target"
            }
        }
    }
}

/// Opaque saved-routine envelope verified against the stable, non-membership
/// parts of a resolved leaf. The current target snapshot remains frozen on
/// every occurrence, but expected membership changes inside this unchanged
/// selector do not manufacture a new confirmation identity.
///
/// ```compile_fail
/// use carsinos_core::execass_danger::VerifiedSavedRoutineSelector;
///
/// let _forged = VerifiedSavedRoutineSelector {
///     routine_id: "caller-selected".to_string(),
///     routine_version: 1,
///     canonical_selector_json: "{}".to_string(),
///     stable_leaf_digest: "caller-selected".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSavedRoutineSelector {
    routine_id: String,
    routine_version: i64,
    canonical_selector_json: String,
    stable_leaf_digest: String,
}

impl VerifiedSavedRoutineSelector {
    pub fn routine_id(&self) -> &str {
        &self.routine_id
    }

    pub fn routine_version(&self) -> i64 {
        self.routine_version
    }

    pub fn canonical_selector_json(&self) -> &str {
        &self.canonical_selector_json
    }

    pub fn matches_stable_leaf(&self, leaf: &CanonicalLeafAction) -> bool {
        self.stable_leaf_digest == saved_routine_stable_leaf_digest(leaf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DangerRouteKind {
    Ordinary,
    RequiresOneConfirmation(DangerAssessment),
}

/// Opaque no-veto routing result for one exact canonical leaf. Callers may
/// inspect the result but cannot manufacture a confirmation-capable route.
///
/// ```compile_fail
/// use carsinos_core::execass_danger::DangerRoute;
///
/// let _forged = DangerRoute {
///     canonical_leaf_digest: "caller-selected".to_string(),
///     kind: panic!("a caller cannot select the route kind"),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DangerRoute {
    canonical_leaf_digest: String,
    kind: DangerRouteKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DangerRouteView<'a> {
    Ordinary,
    RequiresOneConfirmation(&'a DangerAssessment),
}

/// Opaque, leaf-exact proof that a complete canonical manifest crossed the
/// danger-routing boundary.  It is deliberately manifest-bound so persistence
/// can reject omitted, reordered, or substituted leaf routes before opening a
/// write transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DangerAdmissionProof {
    canonical_manifest_digest: String,
    routes_in_manifest_order: Vec<DangerRoute>,
}

/// Untrusted transport envelope for a gateway-sealed danger-admission proof.
/// Storage accepts it only after verifying the signature against its active,
/// independently pinned confirmation-authority key.  Public construction is
/// safe because none of these fields confer authority without that signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedDangerAdmissionProof {
    proof: DangerAdmissionProof,
    key_id: String,
    key_generation: u64,
    canonical_root_identity: String,
    installation_identity: String,
    os_user_identity_digest: String,
    state_root_generation: u64,
    signature_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DangerAdmissionState {
    Ordinary,
    RequiresOneConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DangerAdmissionProofError {
    InvalidSigningField,
    LeafCountMismatch,
    LeafRouteMismatch,
    ManifestMismatch,
}

const DANGER_ADMISSION_SIGNATURE_DOMAIN: &[u8] = b"carsinos.execass.danger_admission.v1";

impl SignedDangerAdmissionProof {
    #[allow(clippy::too_many_arguments)]
    pub fn from_untrusted_parts(
        proof: DangerAdmissionProof,
        key_id: String,
        key_generation: u64,
        canonical_root_identity: String,
        installation_identity: String,
        os_user_identity_digest: String,
        state_root_generation: u64,
        signature_hex: String,
    ) -> Self {
        Self {
            proof,
            key_id,
            key_generation,
            canonical_root_identity,
            installation_identity,
            os_user_identity_digest,
            state_root_generation,
            signature_hex,
        }
    }

    pub fn proof(&self) -> &DangerAdmissionProof {
        &self.proof
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn key_generation(&self) -> u64 {
        self.key_generation
    }

    pub fn canonical_root_identity(&self) -> &str {
        &self.canonical_root_identity
    }

    pub fn installation_identity(&self) -> &str {
        &self.installation_identity
    }

    pub fn os_user_identity_digest(&self) -> &str {
        &self.os_user_identity_digest
    }

    pub fn state_root_generation(&self) -> u64 {
        self.state_root_generation
    }

    pub fn signature_hex(&self) -> &str {
        &self.signature_hex
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>, DangerAdmissionProofError> {
        danger_admission_signing_bytes(
            &self.proof,
            &self.key_id,
            self.key_generation,
            &self.canonical_root_identity,
            &self.installation_identity,
            &self.os_user_identity_digest,
            self.state_root_generation,
        )
    }
}

/// Deterministic, domain-separated bytes covering the complete ordered route
/// result plus the exact authority/root generation that may authenticate it.
#[allow(clippy::too_many_arguments)]
pub fn danger_admission_signing_bytes(
    proof: &DangerAdmissionProof,
    key_id: &str,
    key_generation: u64,
    canonical_root_identity: &str,
    installation_identity: &str,
    os_user_identity_digest: &str,
    state_root_generation: u64,
) -> Result<Vec<u8>, DangerAdmissionProofError> {
    if key_id.trim().is_empty()
        || key_generation == 0
        || canonical_root_identity.trim().is_empty()
        || installation_identity.trim().is_empty()
        || os_user_identity_digest.trim().is_empty()
        || state_root_generation == 0
        || proof.canonical_manifest_digest.len() != 64
    {
        return Err(DangerAdmissionProofError::InvalidSigningField);
    }
    let mut framed = Vec::new();
    push_signature_frame(&mut framed, DANGER_ADMISSION_SIGNATURE_DOMAIN);
    push_signature_frame(&mut framed, key_id.as_bytes());
    push_signature_frame(&mut framed, &key_generation.to_be_bytes());
    push_signature_frame(&mut framed, canonical_root_identity.as_bytes());
    push_signature_frame(&mut framed, installation_identity.as_bytes());
    push_signature_frame(&mut framed, os_user_identity_digest.as_bytes());
    push_signature_frame(&mut framed, &state_root_generation.to_be_bytes());
    push_signature_frame(&mut framed, proof.canonical_manifest_digest.as_bytes());
    push_signature_frame(
        &mut framed,
        &(proof.routes_in_manifest_order.len() as u64).to_be_bytes(),
    );
    for route in &proof.routes_in_manifest_order {
        push_signature_frame(&mut framed, route.canonical_leaf_digest.as_bytes());
        match &route.kind {
            DangerRouteKind::Ordinary => push_signature_frame(&mut framed, b"ordinary"),
            DangerRouteKind::RequiresOneConfirmation(assessment) => {
                push_signature_frame(&mut framed, b"requires_one_confirmation");
                let source = serde_json::to_vec(&assessment.source)
                    .map_err(|_| DangerAdmissionProofError::InvalidSigningField)?;
                let category = serde_json::to_vec(&assessment.known_category)
                    .map_err(|_| DangerAdmissionProofError::InvalidSigningField)?;
                push_signature_frame(&mut framed, &source);
                push_signature_frame(&mut framed, &category);
                push_signature_frame(&mut framed, assessment.declared_consequence.as_bytes());
                push_signature_frame(
                    &mut framed,
                    &[u8::from(assessment.requires_one_confirmation)],
                );
            }
        }
    }
    Ok(framed)
}

fn push_signature_frame(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

/// Bind exactly one non-forgeable route to each leaf in canonical manifest
/// order.  This is a production boundary type; callers cannot manufacture a
/// route kind or make a route apply to a different leaf.
pub fn bind_danger_admission(
    manifest: &CanonicalLeafManifest,
    routes_in_manifest_order: Vec<DangerRoute>,
) -> Result<DangerAdmissionProof, DangerAdmissionProofError> {
    if manifest.leaves().len() != routes_in_manifest_order.len() {
        return Err(DangerAdmissionProofError::LeafCountMismatch);
    }
    if manifest
        .leaves()
        .iter()
        .zip(&routes_in_manifest_order)
        .any(|(leaf, route)| route.canonical_leaf_digest != leaf.canonical().digest().as_hex())
    {
        return Err(DangerAdmissionProofError::LeafRouteMismatch);
    }
    Ok(DangerAdmissionProof {
        canonical_manifest_digest: manifest.canonical().digest().as_hex().to_string(),
        routes_in_manifest_order,
    })
}

impl DangerAdmissionProof {
    /// Recheck the immutable manifest binding immediately at the persistence
    /// boundary.  A matching leaf set in a different order is rejected too.
    pub fn validate_for_manifest(
        &self,
        manifest: &CanonicalLeafManifest,
    ) -> Result<DangerAdmissionState, DangerAdmissionProofError> {
        if self.canonical_manifest_digest != manifest.canonical().digest().as_hex() {
            return Err(DangerAdmissionProofError::ManifestMismatch);
        }
        if self.routes_in_manifest_order.len() != manifest.leaves().len() {
            return Err(DangerAdmissionProofError::LeafCountMismatch);
        }
        if manifest
            .leaves()
            .iter()
            .zip(&self.routes_in_manifest_order)
            .any(|(leaf, route)| route.canonical_leaf_digest != leaf.canonical().digest().as_hex())
        {
            return Err(DangerAdmissionProofError::LeafRouteMismatch);
        }
        Ok(
            if self
                .routes_in_manifest_order
                .iter()
                .any(|route| matches!(route.kind, DangerRouteKind::RequiresOneConfirmation(_)))
            {
                DangerAdmissionState::RequiresOneConfirmation
            } else {
                DangerAdmissionState::Ordinary
            },
        )
    }

    pub fn routes(&self) -> &[DangerRoute] {
        &self.routes_in_manifest_order
    }
}

impl DangerRoute {
    pub fn view(&self) -> DangerRouteView<'_> {
        match &self.kind {
            DangerRouteKind::Ordinary => DangerRouteView::Ordinary,
            DangerRouteKind::RequiresOneConfirmation(assessment) => {
                DangerRouteView::RequiresOneConfirmation(assessment)
            }
        }
    }

    pub fn confirmation_for_leaf(
        &self,
        canonical_leaf: &CanonicalLeafAction,
    ) -> Option<&DangerAssessment> {
        if self.canonical_leaf_digest != canonical_leaf.canonical().digest().as_hex() {
            return None;
        }
        match &self.kind {
            DangerRouteKind::Ordinary => None,
            DangerRouteKind::RequiresOneConfirmation(assessment) => Some(assessment),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DangerRoutingError {
    MetadataLeafMismatch,
    ModelSignalLeafMismatch,
    ModelSignalTargetMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedRoutineSelectorError {
    InvalidIdentity,
    InvalidSelector,
}

pub struct KnownDangerMatchInput<'a> {
    pub canonical_leaf: &'a CanonicalLeafAction,
    pub verified_metadata: &'a VerifiedDangerSystemMetadata,
}

/// Match only the five locked deterministic categories from verified system
/// meaning. Multiple facts still produce one confirmation with one combined
/// consequence, never multiple prompts.
pub fn match_known_danger(
    input: KnownDangerMatchInput<'_>,
) -> Result<DangerRoute, DangerRoutingError> {
    if input.verified_metadata.canonical_leaf_digest
        != input.canonical_leaf.canonical().digest().as_hex()
    {
        return Err(DangerRoutingError::MetadataLeafMismatch);
    }
    let Some(primary) = input.verified_metadata.facts.first() else {
        return Ok(DangerRoute {
            canonical_leaf_digest: input
                .canonical_leaf
                .canonical()
                .digest()
                .as_hex()
                .to_string(),
            kind: DangerRouteKind::Ordinary,
        });
    };
    let mut consequences = input
        .verified_metadata
        .facts
        .iter()
        .map(VerifiedDangerFact::consequence)
        .collect::<Vec<_>>();
    consequences.sort();
    consequences.dedup();
    Ok(DangerRoute {
        canonical_leaf_digest: input
            .canonical_leaf
            .canonical()
            .digest()
            .as_hex()
            .to_string(),
        kind: DangerRouteKind::RequiresOneConfirmation(DangerAssessment {
            source: DangerSource::KnownCategory,
            known_category: Some(primary.category()),
            declared_consequence: consequences.join(" "),
            requires_one_confirmation: true,
        }),
    })
}

/// A credible model signal may add the same single confirmation route. It can
/// neither replace an existing known-category route nor create a deny/veto or
/// second confirmation state.
pub fn route_credible_model_danger(
    canonical_leaf: &CanonicalLeafAction,
    current: DangerRoute,
    signal: Option<&CredibleModelDangerSignal>,
) -> Result<DangerRoute, DangerRoutingError> {
    if current.canonical_leaf_digest != canonical_leaf.canonical().digest().as_hex() {
        return Err(DangerRoutingError::MetadataLeafMismatch);
    }
    let Some(signal) = signal else {
        return Ok(current);
    };
    if signal.canonical_leaf_digest != canonical_leaf.canonical().digest().as_hex() {
        return Err(DangerRoutingError::ModelSignalLeafMismatch);
    }
    if !is_exact_snapshot_target(canonical_leaf, &signal.resolved_target) {
        return Err(DangerRoutingError::ModelSignalTargetMismatch);
    }
    if matches!(current.kind, DangerRouteKind::RequiresOneConfirmation(_)) {
        return Ok(current);
    }
    Ok(DangerRoute {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        kind: DangerRouteKind::RequiresOneConfirmation(DangerAssessment {
            source: DangerSource::ModelCredibleDanger,
            known_category: None,
            declared_consequence: format!(
                "This will {} for the resolved target `{}`.",
                signal.material_result, signal.resolved_target
            ),
            requires_one_confirmation: true,
        }),
    })
}

/// A concrete danger fact after a production resolver has checked its own
/// authoritative state.  This is deliberately not wire metadata: it has no
/// caller-supplied `dangerous` boolean, category string, score, or prose.
/// The gateway bridge may construct one only after its exact resolver tuple
/// has validated canonical operands, the frozen target snapshot, and (where
/// applicable) provider or recovery evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoritativeDangerFact {
    WholeDriveVolumeBootRecoveryOrCoreOsTree { resolved_target: String },
    WholeUserProfileOrHome { resolved_target: String },
    CompleteCarsinosProtectedSystem { resolved_target: String },
    WholeConnectedExternalAccountOrTenant { resolved_target: String },
    LastAdministrativeRecoveryOrDecryptionPath { resolved_target: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionDangerIssuanceError {
    EmptyFacts,
    InvalidResolvedTarget,
    TargetNotInCanonicalSnapshot,
    TargetSnapshotNotExact,
}

/// Issue an opaque production model-danger signal for one exact leaf and its
/// sole frozen target.  The adapter supplies only a bounded material outcome;
/// arbitrary model prose never enters the signed admission path.
pub fn issue_authoritative_model_danger_signal(
    canonical_leaf: &CanonicalLeafAction,
    resolved_target: impl Into<String>,
    material_result: AuthoritativeModelDangerResult,
) -> Result<CredibleModelDangerSignal, ProductionDangerIssuanceError> {
    let resolved_target = resolved_target.into();
    if resolved_target.trim().is_empty() {
        return Err(ProductionDangerIssuanceError::InvalidResolvedTarget);
    }
    if !is_exact_snapshot_target(canonical_leaf, &resolved_target) {
        return Err(ProductionDangerIssuanceError::TargetSnapshotNotExact);
    }
    Ok(CredibleModelDangerSignal {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        resolved_target,
        material_result: material_result.material_result().to_string(),
    })
}

fn is_exact_snapshot_target(canonical_leaf: &CanonicalLeafAction, resolved_target: &str) -> bool {
    serde_json::from_slice::<serde_json::Value>(canonical_leaf.target_snapshot().bytes())
        .ok()
        .and_then(|snapshot| {
            snapshot
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .is_some_and(|targets| targets.len() == 1 && targets[0].as_str() == Some(resolved_target))
}

/// Production-only metadata issuance seam.  The returned metadata stays
/// opaque and exact-leaf-bound; unlike the test issuer below, it is available
/// to the trusted runtime bridge.  It also rejects facts whose claimed target
/// is not one of the leaf's already-frozen snapshot targets.
pub fn issue_authoritative_danger_metadata(
    canonical_leaf: &CanonicalLeafAction,
    facts: Vec<AuthoritativeDangerFact>,
) -> Result<VerifiedDangerSystemMetadata, ProductionDangerIssuanceError> {
    if facts.is_empty() {
        return Err(ProductionDangerIssuanceError::EmptyFacts);
    }
    let snapshot =
        serde_json::from_slice::<serde_json::Value>(canonical_leaf.target_snapshot().bytes())
            .expect("canonical target snapshots are emitted as JSON");
    let targets = snapshot
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .expect("canonical target snapshots contain targets");
    let mut verified = Vec::with_capacity(facts.len());
    for fact in facts {
        let (resolved_target, verified_fact) = match fact {
            AuthoritativeDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree {
                resolved_target,
            } => {
                let copy = resolved_target.clone();
                (
                    copy,
                    VerifiedDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree {
                        resolved_target,
                    },
                )
            }
            AuthoritativeDangerFact::WholeUserProfileOrHome { resolved_target } => {
                let copy = resolved_target.clone();
                (
                    copy,
                    VerifiedDangerFact::WholeUserProfileOrHome { resolved_target },
                )
            }
            AuthoritativeDangerFact::CompleteCarsinosProtectedSystem { resolved_target } => {
                let copy = resolved_target.clone();
                (
                    copy,
                    VerifiedDangerFact::CompleteCarsinosProtectedSystem { resolved_target },
                )
            }
            AuthoritativeDangerFact::WholeConnectedExternalAccountOrTenant { resolved_target } => {
                let copy = resolved_target.clone();
                (
                    copy,
                    VerifiedDangerFact::WholeConnectedExternalAccountOrTenant { resolved_target },
                )
            }
            AuthoritativeDangerFact::LastAdministrativeRecoveryOrDecryptionPath {
                resolved_target,
            } => {
                let copy = resolved_target.clone();
                (
                    copy,
                    VerifiedDangerFact::LastAdministrativeRecoveryOrDecryptionPath {
                        resolved_target,
                    },
                )
            }
        };
        if resolved_target.trim().is_empty() {
            return Err(ProductionDangerIssuanceError::InvalidResolvedTarget);
        }
        if !targets
            .iter()
            .any(|target| target.as_str() == Some(resolved_target.as_str()))
        {
            return Err(ProductionDangerIssuanceError::TargetNotInCanonicalSnapshot);
        }
        verified.push(verified_fact);
    }
    Ok(VerifiedDangerSystemMetadata {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        facts: verified,
    })
}

/// Server registries may explicitly classify one exact resolved operation as
/// ordinary.  The result is still leaf-bound; it is not a caller-supplied
/// category, score, or boolean and cannot apply to another leaf.
pub fn issue_authoritative_ordinary_metadata(
    canonical_leaf: &CanonicalLeafAction,
) -> VerifiedDangerSystemMetadata {
    VerifiedDangerSystemMetadata {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        facts: Vec::new(),
    }
}

#[cfg(any(test, feature = "execass-test-authority"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestVerifiedDangerFact {
    WholeDriveVolumeBootRecoveryOrCoreOsTree,
    WholeUserProfileOrHome,
    CompleteCarsinosProtectedSystem,
    WholeConnectedExternalAccountOrTenant,
    LastAdministrativeRecoveryOrDecryptionPath,
}

/// Disabled-by-default fixture issuer. Production code receives the opaque
/// metadata only from a trusted resolver, never from a wire DTO or model text.
#[cfg(any(test, feature = "execass-test-authority"))]
pub fn issue_test_verified_danger_metadata(
    canonical_leaf: &CanonicalLeafAction,
    facts: &[(TestVerifiedDangerFact, String)],
) -> VerifiedDangerSystemMetadata {
    let facts = facts
        .iter()
        .map(|(fact, resolved_target)| match fact {
            TestVerifiedDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree => {
                VerifiedDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree {
                    resolved_target: resolved_target.clone(),
                }
            }
            TestVerifiedDangerFact::WholeUserProfileOrHome => {
                VerifiedDangerFact::WholeUserProfileOrHome {
                    resolved_target: resolved_target.clone(),
                }
            }
            TestVerifiedDangerFact::CompleteCarsinosProtectedSystem => {
                VerifiedDangerFact::CompleteCarsinosProtectedSystem {
                    resolved_target: resolved_target.clone(),
                }
            }
            TestVerifiedDangerFact::WholeConnectedExternalAccountOrTenant => {
                VerifiedDangerFact::WholeConnectedExternalAccountOrTenant {
                    resolved_target: resolved_target.clone(),
                }
            }
            TestVerifiedDangerFact::LastAdministrativeRecoveryOrDecryptionPath => {
                VerifiedDangerFact::LastAdministrativeRecoveryOrDecryptionPath {
                    resolved_target: resolved_target.clone(),
                }
            }
        })
        .collect();
    VerifiedDangerSystemMetadata {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        facts,
    }
}

#[cfg(any(test, feature = "execass-test-authority"))]
pub fn issue_test_credible_model_danger_signal(
    canonical_leaf: &CanonicalLeafAction,
    resolved_target: impl Into<String>,
    material_result: impl Into<String>,
) -> CredibleModelDangerSignal {
    CredibleModelDangerSignal {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        resolved_target: resolved_target.into(),
        material_result: material_result.into(),
    }
}

/// Disabled-by-default fixture issuer. Production routine code must derive the
/// same opaque value from its durable versioned selector, never a wire flag.
#[cfg(any(test, feature = "execass-test-authority"))]
pub fn issue_test_verified_saved_routine_selector(
    leaf: &CanonicalLeafAction,
    routine_id: impl Into<String>,
    routine_version: i64,
    selector_json: &str,
) -> Result<VerifiedSavedRoutineSelector, SavedRoutineSelectorError> {
    let routine_id = routine_id.into();
    if routine_id.trim().is_empty() || routine_version <= 0 {
        return Err(SavedRoutineSelectorError::InvalidIdentity);
    }
    let selector = serde_json::from_str::<serde_json::Value>(selector_json)
        .map_err(|_| SavedRoutineSelectorError::InvalidSelector)?;
    if !selector.is_object() {
        return Err(SavedRoutineSelectorError::InvalidSelector);
    }
    let canonical_selector_json =
        serde_json::to_string(&selector).map_err(|_| SavedRoutineSelectorError::InvalidSelector)?;
    Ok(VerifiedSavedRoutineSelector {
        routine_id,
        routine_version,
        canonical_selector_json,
        stable_leaf_digest: saved_routine_stable_leaf_digest(leaf),
    })
}

/// Computes the non-membership action identity used to validate a persisted
/// saved-routine version. This digest is not authority: production callers
/// must still load the immutable routine version and owner provenance from
/// storage before using it for confirmation carry-forward.
pub fn saved_routine_stable_leaf_digest(leaf: &CanonicalLeafAction) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"carsinos.execass.saved_routine_stable_leaf.v1");
    for part in [
        leaf.action_kind().as_bytes(),
        leaf.tool().tool_id().as_bytes(),
        leaf.tool().version().as_bytes(),
        leaf.operands().bytes(),
        leaf.owner_authority().normalized_scope_json().as_bytes(),
        leaf.material_digest()
            .map(|digest| digest.as_hex().as_bytes())
            .unwrap_or_default(),
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execass_actor::{issue_test_local_owner_authority, TestLocalOwnerAuthorityInput};
    use crate::execass_manifest::{
        compile_dispatch, CanonicalField, CanonicalLeafManifest, CanonicalValue, DispatchAction,
        DispatchNode, DispatchTree, ManifestCompilation, ResolvedLeafInput,
        ServerResolutionRegistry, TargetSnapshotInput, ToolIdentityInput,
    };

    fn leaf(seed: &str, raw_action_kind: &str) -> CanonicalLeafAction {
        let authority = issue_test_local_owner_authority(TestLocalOwnerAuthorityInput {
            authenticated_client_id: "owner-desktop".to_string(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: format!("correlation-{seed}"),
            source_message_id: Some(format!("message-{seed}")),
            normalized_intent: format!("perform exact action {seed}"),
            instruction_revision: "instruction-1".to_string(),
            instruction_bytes: format!("perform exact action {seed}").into_bytes(),
            owner_envelope_revision: "envelope-1".to_string(),
            owner_envelope_json: r#"{"scope":"exact"}"#.to_string(),
            authority_kind: "original_request".to_string(),
            normalized_scope_json: r#"{"instance":"single-owner"}"#.to_string(),
            policy_revision: 1,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: 1_800_000_000_000,
            expires_at: None,
        })
        .unwrap();
        let dispatch = DispatchTree {
            root_id: "root".to_string(),
            nodes: vec![DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id: format!("action-{seed}"),
                    action_kind: raw_action_kind.to_string(),
                    tool: ToolIdentityInput {
                        tool_id: "filesystem.operation".to_string(),
                        version: "1.0.0".to_string(),
                    },
                    operands: CanonicalValue::Object(vec![CanonicalField {
                        key: "requested_target".to_string(),
                        value: CanonicalValue::String(seed.to_string()),
                    }]),
                    target_snapshot: TargetSnapshotInput {
                        targets: vec![CanonicalValue::String(seed.to_string())],
                    },
                    material_digest: None,
                    owner_authority: authority,
                })),
            }],
        };
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&dispatch, &ServerResolutionRegistry::default())
        else {
            panic!("danger fixture must compile");
        };
        manifest.leaves()[0].clone()
    }

    fn ordinary_route(leaf: &CanonicalLeafAction) -> DangerRoute {
        let metadata = issue_test_verified_danger_metadata(leaf, &[]);
        match_known_danger(KnownDangerMatchInput {
            canonical_leaf: leaf,
            verified_metadata: &metadata,
        })
        .unwrap()
    }

    fn two_leaf_manifest() -> CanonicalLeafManifest {
        let authority = issue_test_local_owner_authority(TestLocalOwnerAuthorityInput {
            authenticated_client_id: "owner-desktop".to_string(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: "batch-correlation".to_string(),
            source_message_id: None,
            normalized_intent: "perform two exact actions".to_string(),
            instruction_revision: "instruction-1".to_string(),
            instruction_bytes: b"perform two exact actions".to_vec(),
            owner_envelope_revision: "envelope-1".to_string(),
            owner_envelope_json: r#"{"scope":"exact"}"#.to_string(),
            authority_kind: "original_request".to_string(),
            normalized_scope_json: r#"{"instance":"single-owner"}"#.to_string(),
            policy_revision: 1,
            bound_decision_id: None,
            bound_decision_revision: None,
            bound_manifest_bytes: None,
            challenge_nonce_bytes: None,
            created_at: 1_800_000_000_000,
            expires_at: None,
        })
        .unwrap();
        let mut nodes = ["first", "second"]
            .into_iter()
            .map(|target| DispatchNode {
                node_id: format!("node-{target}"),
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id: format!("action-{target}"),
                    action_kind: "ordinary_tool_effect".to_string(),
                    tool: ToolIdentityInput {
                        tool_id: "filesystem.operation".to_string(),
                        version: "1.0.0".to_string(),
                    },
                    operands: CanonicalValue::Object(vec![CanonicalField {
                        key: "requested_target".to_string(),
                        value: CanonicalValue::String(target.to_string()),
                    }]),
                    target_snapshot: TargetSnapshotInput {
                        targets: vec![CanonicalValue::String(target.to_string())],
                    },
                    material_digest: None,
                    owner_authority: authority.clone(),
                })),
            })
            .collect::<Vec<_>>();
        nodes.push(DispatchNode {
            node_id: "root".to_string(),
            action: DispatchAction::Composite {
                children: vec!["node-first".to_string(), "node-second".to_string()],
            },
        });
        let dispatch = DispatchTree {
            root_id: "root".to_string(),
            nodes,
        };
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&dispatch, &ServerResolutionRegistry::default())
        else {
            panic!("two leaf danger fixture must compile");
        };
        manifest
    }

    #[test]
    fn danger_admission_proof_requires_complete_ordered_leaf_coverage() {
        let manifest = two_leaf_manifest();
        let routes = manifest
            .leaves()
            .iter()
            .map(ordinary_route)
            .collect::<Vec<_>>();
        assert_eq!(
            bind_danger_admission(&manifest, vec![routes[0].clone()]),
            Err(DangerAdmissionProofError::LeafCountMismatch)
        );
        assert_eq!(
            bind_danger_admission(&manifest, vec![routes[1].clone(), routes[0].clone()]),
            Err(DangerAdmissionProofError::LeafRouteMismatch)
        );
        let proof = bind_danger_admission(&manifest, routes).unwrap();
        assert_eq!(
            proof.validate_for_manifest(&manifest),
            Ok(DangerAdmissionState::Ordinary)
        );
    }

    #[test]
    fn danger_admission_signature_bytes_bind_route_details_and_root_generation() {
        let manifest = two_leaf_manifest();
        let ordinary_routes = manifest
            .leaves()
            .iter()
            .map(ordinary_route)
            .collect::<Vec<_>>();
        let ordinary = bind_danger_admission(&manifest, ordinary_routes.clone()).unwrap();
        let dangerous_metadata = issue_test_verified_danger_metadata(
            &manifest.leaves()[0],
            &[(
                TestVerifiedDangerFact::CompleteCarsinosProtectedSystem,
                "first".to_string(),
            )],
        );
        let dangerous_route = match_known_danger(KnownDangerMatchInput {
            canonical_leaf: &manifest.leaves()[0],
            verified_metadata: &dangerous_metadata,
        })
        .unwrap();
        let dangerous =
            bind_danger_admission(&manifest, vec![dangerous_route, ordinary_routes[1].clone()])
                .unwrap();
        let signing = |proof: &DangerAdmissionProof, root: &str, generation: u64| {
            danger_admission_signing_bytes(
                proof,
                "confirmation-key-1",
                1,
                root,
                "installation-1",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                generation,
            )
            .unwrap()
        };
        let baseline = signing(&ordinary, "root-a", 1);
        assert_ne!(baseline, signing(&dangerous, "root-a", 1));
        assert_ne!(baseline, signing(&ordinary, "root-b", 1));
        assert_ne!(baseline, signing(&ordinary, "root-a", 2));
    }

    #[test]
    fn all_five_locked_categories_route_to_one_concrete_confirmation() {
        let cases = [
            (
                TestVerifiedDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree,
                "Windows volume C:",
                KnownDangerCategory::WholeDriveVolumeBootRecoveryOrCoreOsTreeErasureOrUnusable,
            ),
            (
                TestVerifiedDangerFact::WholeUserProfileOrHome,
                "C:\\Users\\Owner",
                KnownDangerCategory::WholeUserProfileOrHomeErasureOrUnusable,
            ),
            (
                TestVerifiedDangerFact::CompleteCarsinosProtectedSystem,
                "CarsinOS state root",
                KnownDangerCategory::CompleteCarsinosStateIntegrityRuntimeEnforcementStopFencingOrRecoveryConfigurationErasureOrUnusable,
            ),
            (
                TestVerifiedDangerFact::WholeConnectedExternalAccountOrTenant,
                "connected tenant owner@example.test",
                KnownDangerCategory::WholeConnectedExternalAccountClosureOrErasure,
            ),
            (
                TestVerifiedDangerFact::LastAdministrativeRecoveryOrDecryptionPath,
                "recovery key recovery-key-1",
                KnownDangerCategory::LastVerifiedAdministrativeRecoveryOrDecryptionPathDestruction,
            ),
        ];
        for (index, (fact, target, category)) in cases.into_iter().enumerate() {
            let leaf = leaf(&format!("known-{index}"), "ordinary_tool_effect");
            let metadata =
                issue_test_verified_danger_metadata(&leaf, &[(fact, target.to_string())]);
            let route = match_known_danger(KnownDangerMatchInput {
                canonical_leaf: &leaf,
                verified_metadata: &metadata,
            })
            .unwrap();
            let DangerRouteView::RequiresOneConfirmation(assessment) = route.view() else {
                panic!("known danger must require confirmation");
            };
            assert_eq!(assessment.source, DangerSource::KnownCategory);
            assert_eq!(assessment.known_category, Some(category));
            assert!(assessment.requires_one_confirmation);
            assert!(assessment.declared_consequence.contains(target));
            assert!(assessment.declared_consequence.starts_with("This will"));
        }
    }

    #[test]
    fn raw_wording_item_count_tool_shape_and_purchase_like_use_never_classify() {
        for (index, raw_kind) in [
            "delete_everything_dangerous",
            "format_drive",
            "purchase_item",
            "send_money",
            "shell",
            "plugin",
            "500000_items",
        ]
        .into_iter()
        .enumerate()
        {
            let leaf = leaf(&format!("ordinary-{index}"), raw_kind);
            let metadata = issue_test_verified_danger_metadata(&leaf, &[]);
            let route = match_known_danger(KnownDangerMatchInput {
                canonical_leaf: &leaf,
                verified_metadata: &metadata,
            })
            .unwrap();
            assert_eq!(route.view(), DangerRouteView::Ordinary);
        }
    }

    #[test]
    fn metadata_and_model_signals_are_exact_leaf_bound() {
        let first = leaf("first", "ordinary_tool_effect");
        let second = leaf("second", "ordinary_tool_effect");
        let metadata = issue_test_verified_danger_metadata(
            &first,
            &[(
                TestVerifiedDangerFact::WholeUserProfileOrHome,
                "C:\\Users\\Owner".to_string(),
            )],
        );
        assert_eq!(
            match_known_danger(KnownDangerMatchInput {
                canonical_leaf: &second,
                verified_metadata: &metadata,
            }),
            Err(DangerRoutingError::MetadataLeafMismatch),
        );
        let signal = issue_test_credible_model_danger_signal(
            &first,
            "first",
            "irreversibly disable its safety interlock",
        );
        assert_eq!(
            route_credible_model_danger(&second, ordinary_route(&first), Some(&signal)),
            Err(DangerRoutingError::MetadataLeafMismatch),
        );
        assert_eq!(
            route_credible_model_danger(&second, ordinary_route(&second), Some(&signal)),
            Err(DangerRoutingError::ModelSignalLeafMismatch),
        );
    }

    #[test]
    fn credible_model_signal_adds_one_confirmation_but_never_a_veto_or_second_prompt() {
        let leaf = leaf("model", "ordinary_tool_effect");
        let signal = issue_test_credible_model_danger_signal(
            &leaf,
            "model",
            "irreversibly disable its safety interlock",
        );
        let routed = route_credible_model_danger(&leaf, ordinary_route(&leaf), Some(&signal))
            .expect("credible model route");
        let DangerRouteView::RequiresOneConfirmation(assessment) = routed.view() else {
            panic!("model danger must use one confirmation route");
        };
        assert_eq!(assessment.source, DangerSource::ModelCredibleDanger);
        assert_eq!(assessment.known_category, None);
        assert!(assessment.requires_one_confirmation);
        assert!(assessment.declared_consequence.contains("model"));

        let known_metadata = issue_test_verified_danger_metadata(
            &leaf,
            &[(
                TestVerifiedDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree,
                "Windows volume C:".to_string(),
            )],
        );
        let known = match_known_danger(KnownDangerMatchInput {
            canonical_leaf: &leaf,
            verified_metadata: &known_metadata,
        })
        .unwrap();
        assert_eq!(
            route_credible_model_danger(&leaf, known.clone(), Some(&signal)),
            Ok(known),
        );
    }

    #[test]
    fn multiple_verified_danger_facts_still_create_one_combined_confirmation() {
        let leaf = leaf("combined", "ordinary_tool_effect");
        let metadata = issue_test_verified_danger_metadata(
            &leaf,
            &[
                (
                    TestVerifiedDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree,
                    "recovery volume R:".to_string(),
                ),
                (
                    TestVerifiedDangerFact::LastAdministrativeRecoveryOrDecryptionPath,
                    "recovery volume R:".to_string(),
                ),
            ],
        );
        let route = match_known_danger(KnownDangerMatchInput {
            canonical_leaf: &leaf,
            verified_metadata: &metadata,
        })
        .unwrap();
        let DangerRouteView::RequiresOneConfirmation(assessment) = route.view() else {
            panic!("combined danger must route once");
        };
        assert!(assessment.declared_consequence.contains("entire drive"));
        assert!(assessment.declared_consequence.contains("last verified"));
    }

    #[test]
    fn production_issuer_refuses_targets_outside_the_frozen_leaf_snapshot() {
        let frozen_leaf = leaf("frozen-target", "ordinary_tool_effect");
        assert_eq!(
            issue_authoritative_danger_metadata(
                &frozen_leaf,
                vec![AuthoritativeDangerFact::WholeUserProfileOrHome {
                    resolved_target: "different-target".to_string(),
                }],
            ),
            Err(ProductionDangerIssuanceError::TargetNotInCanonicalSnapshot),
        );
        let metadata = issue_authoritative_danger_metadata(
            &frozen_leaf,
            vec![AuthoritativeDangerFact::WholeUserProfileOrHome {
                resolved_target: "frozen-target".to_string(),
            }],
        )
        .expect("authoritative fact must be leaf-bound");
        let second = leaf("other-target", "ordinary_tool_effect");
        assert_eq!(
            match_known_danger(KnownDangerMatchInput {
                canonical_leaf: &second,
                verified_metadata: &metadata,
            }),
            Err(DangerRoutingError::MetadataLeafMismatch),
        );
    }

    #[test]
    fn production_model_issuer_is_exact_target_bound_and_adds_only_one_confirmation() {
        let frozen_leaf = leaf("model-frozen", "ordinary_tool_effect");
        assert_eq!(
            issue_authoritative_model_danger_signal(
                &frozen_leaf,
                "different-target",
                AuthoritativeModelDangerResult::DestroyExactTarget,
            ),
            Err(ProductionDangerIssuanceError::TargetSnapshotNotExact),
        );
        let signal = issue_authoritative_model_danger_signal(
            &frozen_leaf,
            "model-frozen",
            AuthoritativeModelDangerResult::IrreversiblyRemoveAccessToExactTarget,
        )
        .expect("production signal must bind the exact frozen target");
        let route =
            route_credible_model_danger(&frozen_leaf, ordinary_route(&frozen_leaf), Some(&signal))
                .expect("bounded model danger must use the existing route");
        let DangerRouteView::RequiresOneConfirmation(assessment) = route.view() else {
            panic!("bounded model danger must add one confirmation")
        };
        assert_eq!(assessment.source, DangerSource::ModelCredibleDanger);
        assert!(assessment.requires_one_confirmation);
        assert!(assessment.declared_consequence.contains("model-frozen"));
    }
}
