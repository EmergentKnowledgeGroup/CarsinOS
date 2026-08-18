//! Trusted production bridge between canonical ExecAss leaves and the locked
//! EA-206 deterministic danger matcher.
//!
//! This module has no HTTP input, effect, approval, persistence, or model
//! classification path.  A caller gets an ordinary route only from an exact
//! server-owned resolver tuple whose authoritative observation was available.
//! Anything unknown, stale, mismatched, ambiguous, or unavailable remains a
//! typed mechanical pause for the later orchestration layer.

// EA-206 supplies this production-constructed admission service; EA-301 owns
// the public intake caller. Keep the complete fail-closed seam compiled now.
#![allow(dead_code)]

use carsinos_core::execass_danger::{
    bind_danger_admission, issue_authoritative_danger_metadata,
    issue_authoritative_model_danger_signal, issue_authoritative_ordinary_metadata,
    match_known_danger, route_credible_model_danger, AuthoritativeDangerFact,
    AuthoritativeModelDangerResult, CredibleModelDangerSignal, DangerAdmissionProof, DangerRoute,
    KnownDangerMatchInput, ProductionDangerIssuanceError,
};
use carsinos_core::execass_manifest::{CanonicalLeafAction, CanonicalLeafManifest};
use carsinos_storage::{AppPaths, Storage};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const EXTERNAL_ACCOUNT_EVIDENCE_MAX_AGE_MILLIS: i64 = 5 * 60 * 1_000;

pub(crate) const OWNER_REQUEST_ORCHESTRATOR_TOOL_ID: &str = "carsinos.execass.intake";
pub(crate) const OWNER_REQUEST_ORCHESTRATOR_TOOL_VERSION: &str = "v1";
pub(crate) const OWNER_REQUEST_ORCHESTRATOR_ACTION_KIND: &str = "orchestrate_owner_request";

pub(crate) fn owner_request_orchestrator_tuple() -> ResolverTuple {
    ResolverTuple::new(
        OWNER_REQUEST_ORCHESTRATOR_TOOL_ID,
        OWNER_REQUEST_ORCHESTRATOR_TOOL_VERSION,
        OWNER_REQUEST_ORCHESTRATOR_ACTION_KIND,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ResolverTuple {
    tool_id: String,
    version: String,
    action_kind: String,
}

impl ResolverTuple {
    pub(crate) fn new(
        tool_id: impl Into<String>,
        version: impl Into<String>,
        action_kind: impl Into<String>,
    ) -> Self {
        Self {
            tool_id: tool_id.into(),
            version: version.into(),
            action_kind: action_kind.into(),
        }
    }

    fn from_leaf(leaf: &CanonicalLeafAction) -> Self {
        Self {
            tool_id: leaf.tool().tool_id().to_string(),
            version: leaf.tool().version().to_string(),
            action_kind: leaf.action_kind().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrustedDangerResolverEntry {
    tuple: ResolverTuple,
    resolvers: Vec<TrustedDangerResolver>,
}

/// Gateway-owned registration emitted by platform, account, and recovery
/// adapters.  The constructors select one locked semantic and bind it to a
/// server canonical identity; request DTOs never construct this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerDangerAdapterRegistration {
    tuple: ResolverTuple,
    resolver: TrustedDangerResolver,
}

/// Server-configured canonical identities.  These values are intentionally
/// not inferred from a request path, shell word, tool label, count, or model
/// prose: the canonical leaf must already contain the exact same resolved
/// operand and snapshot target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TrustedDangerResolver {
    /// A server-registered, exact benign operation.  It has no inferred
    /// meaning: only this full resolver tuple may take the ordinary route.
    OrdinaryExact,
    /// A live recovery domain found more than one reachable candidate.  The
    /// exact target is mechanically known to be non-last, so ordinary work is
    /// permitted without relabeling the path as destructive-last-recovery.
    OrdinaryExactTarget {
        canonical_identity: String,
    },
    /// A configured live adapter could not establish one exact usable fact.
    /// Keep the tuple registered so the result is a typed pause, never an
    /// accidental ordinary fallback.
    LiveEvidenceUnavailable,
    WindowsCoreTarget {
        canonical_identity: String,
    },
    UserProfile {
        canonical_identity: String,
    },
    CarsinosProtectedSystem {
        canonical_identity: String,
    },
    ExternalAccount {
        provider_id: String,
        canonical_account_identity: String,
    },
    LastRecoveryPath {
        canonical_path_identity: String,
    },
}

/// Trusted runtime observations supplied by gateway-owned platform/provider
/// adapters, never a request DTO.  The external-account and recovery cases
/// are deliberately unavailable unless their provider/reachability evidence
/// is injected for this exact resolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AuthoritativeDangerState {
    external_accounts: BTreeMap<(String, String), Vec<ExternalAccountObservation>>,
    recovery_paths: BTreeMap<String, Vec<RecoveryPathObservation>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExternalAccountObservation {
    provider_id: String,
    canonical_account_identity: String,
    provider_reachable: bool,
    current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecoveryPathObservation {
    canonical_path_identity: String,
    reachability_verified: bool,
    is_last_verified_path: bool,
    current: bool,
}

/// Production state snapshot supplied by server-owned adapters.  Fields stay
/// opaque so transport callers cannot inject category or danger flags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ServerDangerAdapterState {
    authoritative: AuthoritativeDangerState,
}

/// Startup configuration is allowed to map a tuple to an existing CarsinOS
/// connector binding.  It cannot assert the provider/account state: that is
/// re-read from storage at admission and must carry a fresh provider success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveExternalAccountAdapter {
    pub(crate) tuple: ResolverTuple,
    pub(crate) connector_id: String,
    pub(crate) auth_binding_id: String,
    pub(crate) provider_id: String,
    pub(crate) canonical_account_identity: String,
}

/// A recovery domain is configuration of candidates only.  Reachability and
/// which candidate is last are recalculated from the filesystem per admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveRecoveryDomainAdapter {
    pub(crate) tuple: ResolverTuple,
    pub(crate) recovery_domain_id: String,
    pub(crate) candidate_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LiveDangerAdapterConfig {
    pub(crate) external_accounts: Vec<LiveExternalAccountAdapter>,
    pub(crate) recovery_domains: Vec<LiveRecoveryDomainAdapter>,
}

/// One model-adapter conclusion bound to a canonical leaf and its sole target.
/// `None` means the server model completed and added no material danger; it is
/// distinct from an omitted observation, which pauses admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerModelDangerObservation {
    canonical_leaf_digest: String,
    resolved_target: String,
    signal: Option<CredibleModelDangerSignal>,
}

/// Bounded output from the gateway's server-owned model danger pass.  It can
/// add the same single confirmation as the deterministic matcher, but cannot
/// deny an action or invent an unstructured policy category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerModelDangerConclusion {
    NoAdditionalMaterialDanger,
    BoundedMaterialDanger(AuthoritativeModelDangerResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DangerResolverUnresolved {
    UnregisteredTuple,
    VersionMismatch,
    AmbiguousTuple,
    InvalidCanonicalOperands,
    TargetSnapshotMismatch,
    AuthoritativeStateUnavailable,
    AuthoritativeStateMismatch,
    DuplicateAuthoritativeObservation,
    ModelObservationMissing,
    ModelObservationUnexpected,
    ModelObservationDuplicate,
    ModelObservationMismatch,
    ModelAdapterUnavailable,
    Issuance(ProductionDangerIssuanceError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DangerBridgeOutcome {
    Routed(DangerRoute),
    MechanicalUnresolved(DangerResolverUnresolved),
}

/// Full-manifest result of the gateway-owned danger admission boundary.  A
/// mechanical pause intentionally carries no ordinary or confirmation route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DangerBridgeAdmissionOutcome {
    Admitted(DangerAdmissionProof),
    MechanicalUnresolved {
        logical_action_id: String,
        reason: DangerResolverUnresolved,
    },
}

/// Production service constructed from server-owned platform state at gateway
/// startup.  It has no HTTP constructor and never accepts caller-provided
/// facts, categories, target identities, or an ordinary/dangerous flag.
#[derive(Debug, Clone)]
pub(crate) struct DangerActionBridge {
    registry: TrustedDangerResolverRegistry,
    authoritative_state: AuthoritativeDangerState,
    live_storage: Option<Storage>,
    live_adapters: LiveDangerAdapterConfig,
}

/// Exact resolver registry.  The registry is server-owned configuration, and
/// duplicate exact tuples are rejected at routing time as ambiguous rather
/// than silently picking a resolver or classifying the action as ordinary.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrustedDangerResolverRegistry {
    entries: BTreeMap<ResolverTuple, TrustedDangerResolverEntry>,
}

impl TrustedDangerResolverRegistry {
    pub(crate) fn new(entries: Vec<TrustedDangerResolverEntry>) -> Self {
        let mut registry = Self::default();
        for entry in entries {
            registry
                .entries
                .entry(entry.tuple.clone())
                .and_modify(|existing| existing.resolvers.extend(entry.resolvers.clone()))
                .or_insert(entry);
        }
        registry
    }

    pub(crate) fn resolve(
        &self,
        leaf: &CanonicalLeafAction,
        state: &AuthoritativeDangerState,
    ) -> DangerBridgeOutcome {
        let tuple = ResolverTuple::from_leaf(leaf);
        let Some(entry) = self.entries.get(&tuple) else {
            let same_tool_and_action = self.entries.keys().any(|candidate| {
                candidate.tool_id == tuple.tool_id && candidate.action_kind == tuple.action_kind
            });
            return DangerBridgeOutcome::MechanicalUnresolved(if same_tool_and_action {
                DangerResolverUnresolved::VersionMismatch
            } else {
                DangerResolverUnresolved::UnregisteredTuple
            });
        };
        if entry.resolvers.is_empty() {
            return DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::AmbiguousTuple,
            );
        }
        let resolved_target = match canonical_resolved_target(leaf) {
            Ok(target) => target,
            Err(reason) => return DangerBridgeOutcome::MechanicalUnresolved(reason),
        };
        if entry.resolvers.len() == 1
            && matches!(entry.resolvers[0], TrustedDangerResolver::OrdinaryExact)
        {
            let metadata = issue_authoritative_ordinary_metadata(leaf);
            return match match_known_danger(KnownDangerMatchInput {
                canonical_leaf: leaf,
                verified_metadata: &metadata,
            }) {
                Ok(route) => DangerBridgeOutcome::Routed(route),
                Err(_) => DangerBridgeOutcome::MechanicalUnresolved(
                    DangerResolverUnresolved::AuthoritativeStateMismatch,
                ),
            };
        }
        if entry
            .resolvers
            .iter()
            .any(|resolver| matches!(resolver, TrustedDangerResolver::OrdinaryExact))
        {
            return DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::AmbiguousTuple,
            );
        }
        if entry
            .resolvers
            .iter()
            .any(|resolver| matches!(resolver, TrustedDangerResolver::LiveEvidenceUnavailable))
        {
            return DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::AuthoritativeStateUnavailable,
            );
        }
        let applicable = entry
            .resolvers
            .iter()
            .filter(|resolver| resolver.canonical_identity() == Some(resolved_target.as_str()))
            .collect::<Vec<_>>();
        if applicable.is_empty() {
            return DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::TargetSnapshotMismatch,
            );
        }
        if applicable.len() == 1
            && matches!(
                applicable[0],
                TrustedDangerResolver::OrdinaryExactTarget { .. }
            )
        {
            let metadata = issue_authoritative_ordinary_metadata(leaf);
            return match match_known_danger(KnownDangerMatchInput {
                canonical_leaf: leaf,
                verified_metadata: &metadata,
            }) {
                Ok(route) => DangerBridgeOutcome::Routed(route),
                Err(_) => DangerBridgeOutcome::MechanicalUnresolved(
                    DangerResolverUnresolved::AuthoritativeStateMismatch,
                ),
            };
        }
        let unique = applicable.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != applicable.len() {
            return DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::AmbiguousTuple,
            );
        }
        let mut facts = Vec::with_capacity(applicable.len());
        for resolver in applicable {
            match resolver.resolve(&resolved_target, state) {
                Ok(fact) => facts.push(fact),
                Err(reason) => return DangerBridgeOutcome::MechanicalUnresolved(reason),
            }
        }
        let metadata = match issue_authoritative_danger_metadata(leaf, facts) {
            Ok(metadata) => metadata,
            Err(reason) => {
                return DangerBridgeOutcome::MechanicalUnresolved(
                    DangerResolverUnresolved::Issuance(reason),
                )
            }
        };
        match match_known_danger(KnownDangerMatchInput {
            canonical_leaf: leaf,
            verified_metadata: &metadata,
        }) {
            Ok(route) => DangerBridgeOutcome::Routed(route),
            // Issuance and matching use the same leaf, so this is a defensive
            // mechanical boundary rather than a path to ordinary execution.
            Err(_) => DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::AuthoritativeStateMismatch,
            ),
        }
    }
}

impl TrustedDangerResolverEntry {
    pub(crate) fn new(tuple: ResolverTuple, resolvers: Vec<TrustedDangerResolver>) -> Self {
        Self { tuple, resolvers }
    }
}

impl TrustedDangerResolver {
    pub(crate) fn ordinary_exact() -> Self {
        Self::OrdinaryExact
    }

    fn ordinary_exact_target(canonical_identity: impl Into<String>) -> Self {
        Self::OrdinaryExactTarget {
            canonical_identity: canonical_identity.into(),
        }
    }

    pub(crate) fn windows_core(canonical_identity: impl Into<String>) -> Self {
        Self::WindowsCoreTarget {
            canonical_identity: canonical_identity.into(),
        }
    }

    pub(crate) fn user_profile(canonical_identity: impl Into<String>) -> Self {
        Self::UserProfile {
            canonical_identity: canonical_identity.into(),
        }
    }

    pub(crate) fn carsinos_protected_system(canonical_identity: impl Into<String>) -> Self {
        Self::CarsinosProtectedSystem {
            canonical_identity: canonical_identity.into(),
        }
    }

    pub(crate) fn external_account(
        provider_id: impl Into<String>,
        canonical_account_identity: impl Into<String>,
    ) -> Self {
        Self::ExternalAccount {
            provider_id: provider_id.into(),
            canonical_account_identity: canonical_account_identity.into(),
        }
    }

    pub(crate) fn last_recovery_path(canonical_path_identity: impl Into<String>) -> Self {
        Self::LastRecoveryPath {
            canonical_path_identity: canonical_path_identity.into(),
        }
    }

    fn canonical_identity(&self) -> Option<&str> {
        match self {
            Self::OrdinaryExact => None,
            Self::OrdinaryExactTarget { canonical_identity } => Some(canonical_identity),
            Self::LiveEvidenceUnavailable => None,
            Self::WindowsCoreTarget { canonical_identity }
            | Self::UserProfile { canonical_identity }
            | Self::CarsinosProtectedSystem { canonical_identity } => Some(canonical_identity),
            Self::ExternalAccount {
                canonical_account_identity,
                ..
            } => Some(canonical_account_identity),
            Self::LastRecoveryPath {
                canonical_path_identity,
            } => Some(canonical_path_identity),
        }
    }
}

impl ServerDangerAdapterRegistration {
    fn new(tuple: ResolverTuple, resolver: TrustedDangerResolver) -> Self {
        Self { tuple, resolver }
    }

    pub(crate) fn ordinary_exact(tuple: ResolverTuple) -> Self {
        Self::new(tuple, TrustedDangerResolver::ordinary_exact())
    }

    pub(crate) fn whole_drive_volume_boot_or_core_os(
        tuple: ResolverTuple,
        canonical_identity: impl Into<String>,
    ) -> Self {
        Self::new(
            tuple,
            TrustedDangerResolver::windows_core(canonical_identity),
        )
    }

    pub(crate) fn whole_user_profile(
        tuple: ResolverTuple,
        canonical_identity: impl Into<String>,
    ) -> Self {
        Self::new(
            tuple,
            TrustedDangerResolver::user_profile(canonical_identity),
        )
    }

    pub(crate) fn complete_carsinos_protected_system(
        tuple: ResolverTuple,
        canonical_identity: impl Into<String>,
    ) -> Self {
        Self::new(
            tuple,
            TrustedDangerResolver::carsinos_protected_system(canonical_identity),
        )
    }

    pub(crate) fn whole_external_account(
        tuple: ResolverTuple,
        provider_id: impl Into<String>,
        canonical_account_identity: impl Into<String>,
    ) -> Self {
        Self::new(
            tuple,
            TrustedDangerResolver::external_account(provider_id, canonical_account_identity),
        )
    }

    pub(crate) fn last_recovery_path(
        tuple: ResolverTuple,
        canonical_path_identity: impl Into<String>,
    ) -> Self {
        Self::new(
            tuple,
            TrustedDangerResolver::last_recovery_path(canonical_path_identity),
        )
    }
}

impl ServerDangerAdapterState {
    pub(crate) fn with_verified_external_account(
        mut self,
        provider_id: impl Into<String>,
        canonical_account_identity: impl Into<String>,
    ) -> Self {
        self.insert_external_account(provider_id, canonical_account_identity, true, true);
        self
    }

    pub(crate) fn with_stale_external_account(
        mut self,
        provider_id: impl Into<String>,
        canonical_account_identity: impl Into<String>,
    ) -> Self {
        self.insert_external_account(provider_id, canonical_account_identity, true, false);
        self
    }

    fn insert_external_account(
        &mut self,
        provider_id: impl Into<String>,
        canonical_account_identity: impl Into<String>,
        provider_reachable: bool,
        current: bool,
    ) {
        let provider_id = provider_id.into();
        let canonical_account_identity = canonical_account_identity.into();
        self.authoritative
            .external_accounts
            .entry((provider_id.clone(), canonical_account_identity.clone()))
            .or_default()
            .push(ExternalAccountObservation {
                provider_id,
                canonical_account_identity,
                provider_reachable,
                current,
            });
    }

    pub(crate) fn with_verified_last_recovery_path(
        mut self,
        canonical_path_identity: impl Into<String>,
    ) -> Self {
        self.insert_recovery_path(canonical_path_identity, true, true, true);
        self
    }

    pub(crate) fn with_unverified_recovery_path(
        mut self,
        canonical_path_identity: impl Into<String>,
    ) -> Self {
        self.insert_recovery_path(canonical_path_identity, false, false, true);
        self
    }

    fn insert_recovery_path(
        &mut self,
        canonical_path_identity: impl Into<String>,
        reachability_verified: bool,
        is_last_verified_path: bool,
        current: bool,
    ) {
        let canonical_path_identity = canonical_path_identity.into();
        self.authoritative
            .recovery_paths
            .entry(canonical_path_identity.clone())
            .or_default()
            .push(RecoveryPathObservation {
                canonical_path_identity,
                reachability_verified,
                is_last_verified_path,
                current,
            });
    }
}

impl ServerModelDangerObservation {
    pub(crate) fn no_additional_material_danger(
        canonical_leaf: &CanonicalLeafAction,
    ) -> Result<Self, DangerResolverUnresolved> {
        let resolved_target = canonical_resolved_target(canonical_leaf)?;
        Ok(Self {
            canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
            resolved_target,
            signal: None,
        })
    }

    pub(crate) fn bounded_material_danger(
        canonical_leaf: &CanonicalLeafAction,
        result: AuthoritativeModelDangerResult,
    ) -> Result<Self, DangerResolverUnresolved> {
        let resolved_target = canonical_resolved_target(canonical_leaf)?;
        let signal = issue_authoritative_model_danger_signal(
            canonical_leaf,
            resolved_target.clone(),
            result,
        )
        .map_err(DangerResolverUnresolved::Issuance)?;
        Ok(Self {
            canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
            resolved_target,
            signal: Some(signal),
        })
    }

    fn matches_leaf(&self, leaf: &CanonicalLeafAction) -> bool {
        self.canonical_leaf_digest == leaf.canonical().digest().as_hex()
            && canonical_resolved_target(leaf).as_deref() == Ok(self.resolved_target.as_str())
    }
}

impl TrustedDangerResolver {
    fn resolve(
        &self,
        resolved_target: &str,
        state: &AuthoritativeDangerState,
    ) -> Result<AuthoritativeDangerFact, DangerResolverUnresolved> {
        match self {
            Self::OrdinaryExact | Self::OrdinaryExactTarget { .. } => {
                Err(DangerResolverUnresolved::AmbiguousTuple)
            }
            Self::LiveEvidenceUnavailable => {
                Err(DangerResolverUnresolved::AuthoritativeStateUnavailable)
            }
            Self::WindowsCoreTarget { canonical_identity } => {
                exact_identity(resolved_target, canonical_identity, |resolved_target| {
                    AuthoritativeDangerFact::WholeDriveVolumeBootRecoveryOrCoreOsTree {
                        resolved_target,
                    }
                })
            }
            Self::UserProfile { canonical_identity } => {
                exact_identity(resolved_target, canonical_identity, |resolved_target| {
                    AuthoritativeDangerFact::WholeUserProfileOrHome { resolved_target }
                })
            }
            Self::CarsinosProtectedSystem { canonical_identity } => {
                exact_identity(resolved_target, canonical_identity, |resolved_target| {
                    AuthoritativeDangerFact::CompleteCarsinosProtectedSystem { resolved_target }
                })
            }
            Self::ExternalAccount {
                provider_id,
                canonical_account_identity,
            } => {
                let observations = state
                    .external_accounts
                    .get(&(provider_id.clone(), canonical_account_identity.clone()))
                    .ok_or(DangerResolverUnresolved::AuthoritativeStateUnavailable)?;
                if observations.len() != 1 {
                    return Err(DangerResolverUnresolved::DuplicateAuthoritativeObservation);
                }
                let observation = &observations[0];
                if !observation.provider_reachable || !observation.current {
                    return Err(DangerResolverUnresolved::AuthoritativeStateUnavailable);
                }
                if observation.provider_id != *provider_id
                    || observation.canonical_account_identity != *canonical_account_identity
                {
                    return Err(DangerResolverUnresolved::AuthoritativeStateMismatch);
                }
                exact_identity(
                    resolved_target,
                    canonical_account_identity,
                    |resolved_target| {
                        AuthoritativeDangerFact::WholeConnectedExternalAccountOrTenant {
                            resolved_target,
                        }
                    },
                )
            }
            Self::LastRecoveryPath {
                canonical_path_identity,
            } => {
                let observations = state
                    .recovery_paths
                    .get(canonical_path_identity)
                    .ok_or(DangerResolverUnresolved::AuthoritativeStateUnavailable)?;
                if observations.len() != 1 {
                    return Err(DangerResolverUnresolved::DuplicateAuthoritativeObservation);
                }
                let observation = &observations[0];
                if !observation.reachability_verified
                    || !observation.is_last_verified_path
                    || !observation.current
                {
                    return Err(DangerResolverUnresolved::AuthoritativeStateUnavailable);
                }
                if observation.canonical_path_identity != *canonical_path_identity {
                    return Err(DangerResolverUnresolved::AuthoritativeStateMismatch);
                }
                exact_identity(
                    resolved_target,
                    canonical_path_identity,
                    |resolved_target| {
                        AuthoritativeDangerFact::LastAdministrativeRecoveryOrDecryptionPath {
                            resolved_target,
                        }
                    },
                )
            }
        }
    }
}

impl DangerActionBridge {
    /// Derive the only startup-known target identities from local platform and
    /// CarsinOS paths.  Missing OS/profile identities are deliberately left
    /// unregistered; a future attempted dispatch therefore pauses rather than
    /// being downgraded to ordinary.  External-account and recovery facts are
    /// absent until their owning adapters supply verified observations.
    pub(crate) fn from_server_paths(paths: &AppPaths) -> Self {
        Self::from_server_paths_and_adapters(paths, Vec::new(), ServerDangerAdapterState::default())
    }

    /// Preserve the startup-derived local identities while injecting
    /// configured volume/account/recovery registrations and their current
    /// server observations.
    pub(crate) fn from_server_paths_and_adapters(
        paths: &AppPaths,
        mut registrations: Vec<ServerDangerAdapterRegistration>,
        state: ServerDangerAdapterState,
    ) -> Self {
        let mut path_registrations = vec![
            ServerDangerAdapterRegistration::ordinary_exact(ResolverTuple::new(
                "carsinos.runtime",
                "v1",
                "inspect_runtime",
            )),
            // This exact leaf performs no eventual owner-request effect. It only
            // persists an Accepted foundation for later canonical-plan production,
            // so its deterministic danger fact is ordinary. The bounded server
            // model still observes the leaf at the common admission boundary.
            ServerDangerAdapterRegistration::ordinary_exact(owner_request_orchestrator_tuple()),
        ];
        for identity in mounted_volume_root_identities() {
            path_registrations.push(
                ServerDangerAdapterRegistration::whole_drive_volume_boot_or_core_os(
                    ResolverTuple::new("carsinos.native.fs", "v1", "destroy_whole_volume"),
                    identity,
                ),
            );
        }
        if let Some(identity) = canonical_path_identity(std::env::var_os("SystemRoot")) {
            path_registrations.push(
                ServerDangerAdapterRegistration::whole_drive_volume_boot_or_core_os(
                    ResolverTuple::new("carsinos.native.fs", "v1", "destroy_windows_core"),
                    identity,
                ),
            );
        }
        if let Some(identity) = canonical_path_identity(std::env::var_os("USERPROFILE")) {
            path_registrations.push(ServerDangerAdapterRegistration::whole_user_profile(
                ResolverTuple::new("carsinos.native.fs", "v1", "destroy_owner_profile"),
                identity,
            ));
        }
        if let Some(identity) = canonical_path_identity(Some(paths.root.as_os_str().to_os_string()))
        {
            path_registrations.push(
                ServerDangerAdapterRegistration::complete_carsinos_protected_system(
                    ResolverTuple::new("carsinos.runtime", "v1", "destroy_protected_state"),
                    identity,
                ),
            );
        }
        path_registrations.append(&mut registrations);
        Self::from_server_adapters(path_registrations, state)
    }

    /// Production constructor.  Config can only name existing CarsinOS
    /// evidence sources and recovery candidates; it never supplies an
    /// authoritative observation.  Evidence is rebuilt at each admission.
    pub(crate) fn from_server_paths_and_live_adapters(
        paths: &AppPaths,
        storage: Storage,
        registrations: Vec<ServerDangerAdapterRegistration>,
        live_adapters: LiveDangerAdapterConfig,
    ) -> Self {
        let mut bridge = Self::from_server_paths_and_adapters(
            paths,
            registrations,
            ServerDangerAdapterState::default(),
        );
        bridge.live_storage = Some(storage);
        bridge.live_adapters = live_adapters;
        bridge
    }

    /// Construct the production bridge from server adapter registrations and
    /// their current authoritative state snapshot. Registrations sharing a
    /// tuple are resolved by exact canonical target, allowing multiple
    /// accounts and recovery paths without first-match ambiguity.
    pub(crate) fn from_server_adapters(
        registrations: Vec<ServerDangerAdapterRegistration>,
        state: ServerDangerAdapterState,
    ) -> Self {
        let entries = registrations
            .into_iter()
            .map(|registration| {
                TrustedDangerResolverEntry::new(registration.tuple, vec![registration.resolver])
            })
            .collect();
        Self {
            registry: TrustedDangerResolverRegistry::new(entries),
            authoritative_state: state.authoritative,
            live_storage: None,
            live_adapters: LiveDangerAdapterConfig::default(),
        }
    }

    /// Compatibility seam for callers not yet wired to the server model
    /// adapter. Omission is deliberately a mechanical pause, never ordinary.
    pub(crate) fn admit_manifest(
        &self,
        manifest: &CanonicalLeafManifest,
    ) -> DangerBridgeAdmissionOutcome {
        self.admit_manifest_with_model_observations(manifest, &[])
    }

    /// Bind exactly one bounded server-model conclusion to each exact leaf,
    /// in manifest order, before entering the common admission path.  A
    /// missing or extra conclusion pauses mechanically.  Known danger plus a
    /// model-added danger still produces one confirmation route for the leaf.
    pub(crate) fn admit_manifest_with_model_conclusions(
        &self,
        manifest: &CanonicalLeafManifest,
        conclusions: &[ServerModelDangerConclusion],
    ) -> DangerBridgeAdmissionOutcome {
        if conclusions.len() != manifest.leaves().len() {
            return self.admit_manifest_with_model_observations(manifest, &[]);
        }
        let observations = manifest
            .leaves()
            .iter()
            .zip(conclusions)
            .map(|(leaf, conclusion)| match conclusion {
                ServerModelDangerConclusion::NoAdditionalMaterialDanger => {
                    ServerModelDangerObservation::no_additional_material_danger(leaf)
                }
                ServerModelDangerConclusion::BoundedMaterialDanger(result) => {
                    ServerModelDangerObservation::bounded_material_danger(leaf, *result)
                }
            })
            .collect::<Result<Vec<_>, _>>();
        match observations {
            Ok(observations) => {
                self.admit_manifest_with_model_observations(manifest, &observations)
            }
            Err(reason) => DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                logical_action_id: "server-model-observation".to_string(),
                reason,
            },
        }
    }

    pub(crate) fn admit_manifest_with_model_observations(
        &self,
        manifest: &CanonicalLeafManifest,
        observations: &[ServerModelDangerObservation],
    ) -> DangerBridgeAdmissionOutcome {
        let mut observed_leaf_digests = BTreeSet::new();
        if observations
            .iter()
            .any(|observation| !observed_leaf_digests.insert(&observation.canonical_leaf_digest))
        {
            return DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                logical_action_id: "duplicate-model-observation".to_string(),
                reason: DangerResolverUnresolved::ModelObservationDuplicate,
            };
        }
        if observations.len() < manifest.leaves().len() {
            let missing = &manifest.leaves()[observations.len()];
            return DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                logical_action_id: missing.logical_action_id().to_string(),
                reason: DangerResolverUnresolved::ModelObservationMissing,
            };
        }
        if observations.len() > manifest.leaves().len() {
            return DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                logical_action_id: "model-observation-outside-manifest".to_string(),
                reason: DangerResolverUnresolved::ModelObservationUnexpected,
            };
        }
        // One admission sees one live authority snapshot.  Never let a
        // multi-leaf dispatch mix pre- and post-provider/recovery state.
        let (registry, authoritative_state) = self.live_registry_and_state();
        let mut routes = Vec::with_capacity(manifest.leaves().len());
        for (leaf, observation) in manifest.leaves().iter().zip(observations) {
            if !observation.matches_leaf(leaf) {
                return DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                    logical_action_id: leaf.logical_action_id().to_string(),
                    reason: DangerResolverUnresolved::ModelObservationMismatch,
                };
            }
            match registry.resolve(leaf, &authoritative_state) {
                DangerBridgeOutcome::Routed(route) => {
                    match route_credible_model_danger(leaf, route, observation.signal.as_ref()) {
                        Ok(route) => routes.push(route),
                        Err(_) => {
                            return DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                                logical_action_id: leaf.logical_action_id().to_string(),
                                reason: DangerResolverUnresolved::ModelObservationMismatch,
                            }
                        }
                    }
                }
                DangerBridgeOutcome::MechanicalUnresolved(reason) => {
                    return DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                        logical_action_id: leaf.logical_action_id().to_string(),
                        reason,
                    };
                }
            }
        }
        match bind_danger_admission(manifest, routes) {
            Ok(proof) => DangerBridgeAdmissionOutcome::Admitted(proof),
            // Routes are generated directly from the manifest in order.  If
            // this invariant ever fails, halt at the same mechanical boundary.
            Err(_) => DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                logical_action_id: "danger-admission-invariant".to_string(),
                reason: DangerResolverUnresolved::AuthoritativeStateMismatch,
            },
        }
    }

    fn live_registry_and_state(&self) -> (TrustedDangerResolverRegistry, AuthoritativeDangerState) {
        let mut registrations = self
            .registry
            .entries
            .values()
            .flat_map(|entry| {
                entry.resolvers.iter().cloned().map(|resolver| {
                    ServerDangerAdapterRegistration::new(entry.tuple.clone(), resolver)
                })
            })
            .collect::<Vec<_>>();
        let mut state = self.authoritative_state.clone();
        let Some(storage) = self.live_storage.as_ref() else {
            return (registry_from_registrations(registrations), state);
        };
        for source in &self.live_adapters.external_accounts {
            match observe_external_account(storage, source) {
                Ok((provider_id, identity)) => {
                    registrations.push(ServerDangerAdapterRegistration::whole_external_account(
                        source.tuple.clone(),
                        provider_id.clone(),
                        identity.clone(),
                    ));
                    state
                        .external_accounts
                        .entry((provider_id.clone(), identity.clone()))
                        .or_default()
                        .push(ExternalAccountObservation {
                            provider_id,
                            canonical_account_identity: identity,
                            provider_reachable: true,
                            current: true,
                        });
                }
                Err(()) => registrations.push(ServerDangerAdapterRegistration::new(
                    source.tuple.clone(),
                    TrustedDangerResolver::LiveEvidenceUnavailable,
                )),
            }
        }
        for source in &self.live_adapters.recovery_domains {
            let reachable = source
                .candidate_paths
                .iter()
                .filter_map(|path| canonical_path_identity(Some(path.clone().into_os_string())))
                .collect::<Vec<_>>();
            let unique = reachable.iter().cloned().collect::<BTreeSet<_>>();
            if unique.len() != reachable.len() || unique.is_empty() {
                registrations.push(ServerDangerAdapterRegistration::new(
                    source.tuple.clone(),
                    TrustedDangerResolver::LiveEvidenceUnavailable,
                ));
                continue;
            }
            if unique.len() == 1 {
                let identity = unique.first().expect("one reachable path").clone();
                registrations.push(ServerDangerAdapterRegistration::last_recovery_path(
                    source.tuple.clone(),
                    identity.clone(),
                ));
                state
                    .recovery_paths
                    .entry(identity.clone())
                    .or_default()
                    .push(RecoveryPathObservation {
                        canonical_path_identity: identity,
                        reachability_verified: true,
                        is_last_verified_path: true,
                        current: true,
                    });
                continue;
            }
            for identity in unique {
                registrations.push(ServerDangerAdapterRegistration::new(
                    source.tuple.clone(),
                    TrustedDangerResolver::ordinary_exact_target(identity),
                ));
            }
        }
        (registry_from_registrations(registrations), state)
    }
}

fn registry_from_registrations(
    registrations: Vec<ServerDangerAdapterRegistration>,
) -> TrustedDangerResolverRegistry {
    TrustedDangerResolverRegistry::new(
        registrations
            .into_iter()
            .map(|registration| {
                TrustedDangerResolverEntry::new(registration.tuple, vec![registration.resolver])
            })
            .collect(),
    )
}

fn observe_external_account(
    storage: &Storage,
    source: &LiveExternalAccountAdapter,
) -> Result<(String, String), ()> {
    let bindings = storage
        .list_connector_auth_bindings(&source.connector_id)
        .map_err(|_| ())?;
    let matches = bindings
        .into_iter()
        .filter(|binding| binding.auth_binding_id == source.auth_binding_id)
        .collect::<Vec<_>>();
    let [binding] = matches.as_slice() else {
        return Err(());
    };
    let now = unix_millis();
    let Some(last_success_at) = binding.last_success_at else {
        return Err(());
    };
    if binding.status != "ready"
        || binding.last_error.is_some()
        || last_success_at > now
        || now.saturating_sub(last_success_at) > EXTERNAL_ACCOUNT_EVIDENCE_MAX_AGE_MILLIS
        || binding.updated_at < last_success_at
    {
        return Err(());
    }
    // Provider/account identity is server startup configuration.  Mutable
    // binding JSON cannot choose it or manufacture a danger category.
    Ok((
        source.provider_id.clone(),
        source.canonical_account_identity.clone(),
    ))
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn canonical_path_identity(path: Option<std::ffi::OsString>) -> Option<String> {
    let path = path?;
    std::fs::canonicalize(path)
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|identity| !identity.trim().is_empty())
}

fn canonical_volume_root_identity(path: Option<std::ffi::OsString>) -> Option<String> {
    let mut path = std::path::PathBuf::from(path?);
    if path.to_string_lossy().ends_with(':') {
        path.push("\\");
    }
    canonical_path_identity(Some(path.into_os_string()))
}

/// Enumerate every Windows logical drive root through the native filesystem
/// API.  No shell, WMI, or test-only caller chooses the volume set.  Other
/// platforms intentionally return no Windows volume identities while keeping
/// the gateway cross-platform compilable.
#[cfg(windows)]
fn mounted_volume_root_identities() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDriveStringsW;

    let required = unsafe { GetLogicalDriveStringsW(0, std::ptr::null_mut()) };
    if required == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0_u16; required as usize + 1];
    let written = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };
    if written == 0 || written as usize >= buffer.len() {
        return Vec::new();
    }
    buffer[..written as usize]
        .split(|unit| *unit == 0)
        .filter(|part| !part.is_empty())
        .filter_map(|part| String::from_utf16(part).ok())
        .filter_map(|root| canonical_volume_root_identity(Some(root.into())))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(not(windows))]
fn mounted_volume_root_identities() -> Vec<String> {
    Vec::new()
}

fn exact_identity(
    resolved_target: &str,
    canonical_identity: &str,
    fact: impl FnOnce(String) -> AuthoritativeDangerFact,
) -> Result<AuthoritativeDangerFact, DangerResolverUnresolved> {
    if resolved_target != canonical_identity {
        return Err(DangerResolverUnresolved::TargetSnapshotMismatch);
    }
    Ok(fact(resolved_target.to_string()))
}

/// Require one exact typed resolved target in both canonical structures.  No
/// broad target-set guess or raw wording can enter the matcher.
fn canonical_resolved_target(
    leaf: &CanonicalLeafAction,
) -> Result<String, DangerResolverUnresolved> {
    let operands = serde_json::from_slice::<serde_json::Value>(leaf.operands().bytes())
        .map_err(|_| DangerResolverUnresolved::InvalidCanonicalOperands)?;
    let target = operands
        .as_object()
        .and_then(|value| value.get("resolved_target"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(DangerResolverUnresolved::InvalidCanonicalOperands)?;
    let snapshot = serde_json::from_slice::<serde_json::Value>(leaf.target_snapshot().bytes())
        .map_err(|_| DangerResolverUnresolved::TargetSnapshotMismatch)?;
    let targets = snapshot
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .ok_or(DangerResolverUnresolved::TargetSnapshotMismatch)?;
    if targets.len() != 1 || targets[0].as_str() != Some(target) {
        return Err(DangerResolverUnresolved::TargetSnapshotMismatch);
    }
    Ok(target.to_string())
}

/// The model adapter sees only this leaf-bound identity pair.  It cannot
/// receive category flags or return an unbound conclusion for another target.
pub(crate) fn model_leaf_identity(
    leaf: &CanonicalLeafAction,
) -> Result<(String, String), DangerResolverUnresolved> {
    Ok((
        leaf.canonical().digest().as_hex().to_string(),
        canonical_resolved_target(leaf)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use carsinos_core::execass_actor::{
        issue_test_local_owner_authority, TestLocalOwnerAuthorityInput,
    };
    use carsinos_core::execass_danger::DangerRouteView;
    use carsinos_core::execass_manifest::{
        compile_dispatch, CanonicalField, CanonicalValue, DispatchAction, DispatchNode,
        DispatchTree, ManifestCompilation, ResolvedLeafInput, ServerResolutionRegistry,
        TargetSnapshotInput, ToolIdentityInput,
    };

    fn leaf(
        target: &str,
        version: &str,
        action_kind: &str,
        extra_target: Option<&str>,
    ) -> CanonicalLeafAction {
        manifest(target, version, action_kind, extra_target).leaves()[0].clone()
    }

    fn manifest(
        target: &str,
        version: &str,
        action_kind: &str,
        extra_target: Option<&str>,
    ) -> CanonicalLeafManifest {
        manifest_with_tool(
            "trusted.destroy",
            target,
            version,
            action_kind,
            extra_target,
        )
    }

    fn manifest_with_tool(
        tool_id: &str,
        target: &str,
        version: &str,
        action_kind: &str,
        extra_target: Option<&str>,
    ) -> CanonicalLeafManifest {
        let authority = issue_test_local_owner_authority(TestLocalOwnerAuthorityInput {
            authenticated_client_id: "owner-desktop".to_string(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: "correlation".to_string(),
            source_message_id: None,
            normalized_intent: "exact owner action".to_string(),
            instruction_revision: "instruction-1".to_string(),
            instruction_bytes: b"exact owner action".to_vec(),
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
        let mut targets = vec![CanonicalValue::String(target.to_string())];
        if let Some(extra) = extra_target {
            targets.push(CanonicalValue::String(extra.to_string()));
        }
        let tree = DispatchTree {
            root_id: "root".to_string(),
            nodes: vec![DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id: "action-1".to_string(),
                    action_kind: action_kind.to_string(),
                    tool: ToolIdentityInput {
                        tool_id: tool_id.to_string(),
                        version: version.to_string(),
                    },
                    operands: CanonicalValue::Object(vec![CanonicalField {
                        key: "resolved_target".to_string(),
                        value: CanonicalValue::String(target.to_string()),
                    }]),
                    target_snapshot: TargetSnapshotInput { targets },
                    material_digest: None,
                    owner_authority: authority,
                })),
            }],
        };
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&tree, &ServerResolutionRegistry::default())
        else {
            panic!("fixture must compile")
        };
        manifest
    }

    fn entry(resolvers: Vec<TrustedDangerResolver>) -> TrustedDangerResolverEntry {
        TrustedDangerResolverEntry {
            tuple: ResolverTuple {
                tool_id: "trusted.destroy".to_string(),
                version: "1.0.0".to_string(),
                action_kind: "resolved_destroy".to_string(),
            },
            resolvers,
        }
    }

    fn tuple(action_kind: &str) -> ResolverTuple {
        ResolverTuple::new("trusted.destroy", "1.0.0", action_kind)
    }

    fn requires_confirmation(outcome: DangerBridgeOutcome) {
        let DangerBridgeOutcome::Routed(route) = outcome else {
            panic!("registered authoritative route must resolve")
        };
        assert!(matches!(
            route.view(),
            DangerRouteView::RequiresOneConfirmation(_)
        ));
    }

    #[test]
    fn five_authoritative_fact_shapes_require_one_confirmation() {
        let cases = vec![
            (
                "Windows:System32",
                TrustedDangerResolver::WindowsCoreTarget {
                    canonical_identity: "Windows:System32".to_string(),
                },
                AuthoritativeDangerState::default(),
            ),
            (
                "profile:owner",
                TrustedDangerResolver::UserProfile {
                    canonical_identity: "profile:owner".to_string(),
                },
                AuthoritativeDangerState::default(),
            ),
            (
                "carsinos:state",
                TrustedDangerResolver::CarsinosProtectedSystem {
                    canonical_identity: "carsinos:state".to_string(),
                },
                AuthoritativeDangerState::default(),
            ),
            (
                "tenant:owner",
                TrustedDangerResolver::ExternalAccount {
                    provider_id: "provider-a".to_string(),
                    canonical_account_identity: "tenant:owner".to_string(),
                },
                ServerDangerAdapterState::default()
                    .with_verified_external_account("provider-a", "tenant:owner")
                    .authoritative,
            ),
            (
                "recovery:key-1",
                TrustedDangerResolver::LastRecoveryPath {
                    canonical_path_identity: "recovery:key-1".to_string(),
                },
                ServerDangerAdapterState::default()
                    .with_verified_last_recovery_path("recovery:key-1")
                    .authoritative,
            ),
        ];
        for (target, resolver, state) in cases {
            requires_confirmation(
                TrustedDangerResolverRegistry::new(vec![entry(vec![resolver])])
                    .resolve(&leaf(target, "1.0.0", "resolved_destroy", None), &state),
            );
        }
    }

    #[test]
    fn owner_request_orchestrator_tuple_is_registered_as_exact_ordinary() {
        let manifest = manifest_with_tool(
            OWNER_REQUEST_ORCHESTRATOR_TOOL_ID,
            "delegation:accepted",
            OWNER_REQUEST_ORCHESTRATOR_TOOL_VERSION,
            OWNER_REQUEST_ORCHESTRATOR_ACTION_KIND,
            None,
        );
        let registry = TrustedDangerResolverRegistry::new(vec![TrustedDangerResolverEntry::new(
            owner_request_orchestrator_tuple(),
            vec![TrustedDangerResolver::ordinary_exact()],
        )]);
        let DangerBridgeOutcome::Routed(route) =
            registry.resolve(&manifest.leaves()[0], &AuthoritativeDangerState::default())
        else {
            panic!("fixed planning tuple must resolve")
        };
        assert_eq!(route.view(), DangerRouteView::Ordinary);
    }

    #[test]
    fn narrow_or_wordy_actions_do_not_classify_without_exact_registered_fact() {
        let registry = TrustedDangerResolverRegistry::new(vec![entry(vec![
            TrustedDangerResolver::UserProfile {
                canonical_identity: "profile:owner".to_string(),
            },
        ])]);
        assert_eq!(
            registry.resolve(
                &leaf("profile:other", "1.0.0", "resolved_destroy", None),
                &AuthoritativeDangerState::default()
            ),
            DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::TargetSnapshotMismatch
            )
        );
        assert_eq!(
            registry.resolve(
                &leaf(
                    "profile:owner",
                    "1.0.0",
                    "delete_everything_purchase_shell_plugin",
                    Some("extra")
                ),
                &AuthoritativeDangerState::default()
            ),
            DangerBridgeOutcome::MechanicalUnresolved(DangerResolverUnresolved::UnregisteredTuple)
        );
    }

    #[test]
    fn unresolved_registry_or_authority_never_becomes_ordinary() {
        let registry = TrustedDangerResolverRegistry::new(vec![entry(vec![
            TrustedDangerResolver::ExternalAccount {
                provider_id: "provider-a".to_string(),
                canonical_account_identity: "tenant:owner".to_string(),
            },
        ])]);
        assert_eq!(
            registry.resolve(
                &leaf("tenant:owner", "2.0.0", "resolved_destroy", None),
                &AuthoritativeDangerState::default()
            ),
            DangerBridgeOutcome::MechanicalUnresolved(DangerResolverUnresolved::VersionMismatch)
        );
        assert_eq!(
            registry.resolve(
                &leaf("tenant:owner", "1.0.0", "resolved_destroy", None),
                &AuthoritativeDangerState::default()
            ),
            DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::AuthoritativeStateUnavailable
            )
        );
        let ambiguous = TrustedDangerResolverRegistry::new(vec![
            entry(vec![TrustedDangerResolver::UserProfile {
                canonical_identity: "profile:owner".to_string(),
            }]),
            entry(vec![TrustedDangerResolver::UserProfile {
                canonical_identity: "profile:owner".to_string(),
            }]),
        ]);
        assert_eq!(
            ambiguous.resolve(
                &leaf("profile:owner", "1.0.0", "resolved_destroy", None),
                &AuthoritativeDangerState::default()
            ),
            DangerBridgeOutcome::MechanicalUnresolved(DangerResolverUnresolved::AmbiguousTuple)
        );
    }

    #[test]
    fn leaf_drift_and_multiple_facts_are_bound_to_one_route() {
        let registry = TrustedDangerResolverRegistry::new(vec![entry(vec![
            TrustedDangerResolver::WindowsCoreTarget {
                canonical_identity: "Windows:recovery".to_string(),
            },
            TrustedDangerResolver::LastRecoveryPath {
                canonical_path_identity: "Windows:recovery".to_string(),
            },
        ])]);
        let state = ServerDangerAdapterState::default()
            .with_verified_last_recovery_path("Windows:recovery")
            .authoritative;
        requires_confirmation(registry.resolve(
            &leaf("Windows:recovery", "1.0.0", "resolved_destroy", None),
            &state,
        ));
        assert_eq!(
            registry.resolve(
                &leaf("Windows:recovery-drift", "1.0.0", "resolved_destroy", None),
                &state
            ),
            DangerBridgeOutcome::MechanicalUnresolved(
                DangerResolverUnresolved::TargetSnapshotMismatch
            )
        );
    }

    #[test]
    fn production_adapters_route_all_five_categories_and_multiple_dynamic_targets() {
        let registrations = vec![
            ServerDangerAdapterRegistration::whole_drive_volume_boot_or_core_os(
                tuple("resolved_destroy"),
                "volume:system",
            ),
            ServerDangerAdapterRegistration::whole_user_profile(
                tuple("resolved_destroy"),
                "profile:owner",
            ),
            ServerDangerAdapterRegistration::complete_carsinos_protected_system(
                tuple("resolved_destroy"),
                "carsinos:protected",
            ),
            ServerDangerAdapterRegistration::whole_external_account(
                tuple("resolved_destroy"),
                "provider-a",
                "tenant:a",
            ),
            ServerDangerAdapterRegistration::whole_external_account(
                tuple("resolved_destroy"),
                "provider-b",
                "tenant:b",
            ),
            ServerDangerAdapterRegistration::last_recovery_path(
                tuple("resolved_destroy"),
                "recovery:a",
            ),
            ServerDangerAdapterRegistration::last_recovery_path(
                tuple("resolved_destroy"),
                "recovery:b",
            ),
        ];
        let state = ServerDangerAdapterState::default()
            .with_verified_external_account("provider-a", "tenant:a")
            .with_verified_external_account("provider-b", "tenant:b")
            .with_verified_last_recovery_path("recovery:a")
            .with_verified_last_recovery_path("recovery:b");
        let bridge = DangerActionBridge::from_server_adapters(registrations, state);

        for target in [
            "volume:system",
            "profile:owner",
            "carsinos:protected",
            "tenant:a",
            "tenant:b",
            "recovery:a",
            "recovery:b",
        ] {
            let manifest = manifest(target, "1.0.0", "resolved_destroy", None);
            let observation =
                ServerModelDangerObservation::no_additional_material_danger(&manifest.leaves()[0])
                    .expect("exact production model observation");
            let DangerBridgeAdmissionOutcome::Admitted(proof) =
                bridge.admit_manifest_with_model_observations(&manifest, &[observation])
            else {
                panic!("production adapter must route {target}")
            };
            assert_eq!(proof.routes().len(), 1);
            assert!(matches!(
                proof.routes()[0].view(),
                DangerRouteView::RequiresOneConfirmation(_)
            ));
        }
    }

    #[test]
    fn production_model_observation_adds_one_route_and_never_stacks_on_known_danger() {
        let ordinary_bridge = DangerActionBridge::from_server_adapters(
            vec![ServerDangerAdapterRegistration::ordinary_exact(tuple(
                "model_material_change",
            ))],
            ServerDangerAdapterState::default(),
        );
        let ordinary_manifest = manifest(
            "controller:safety-interlock",
            "1.0.0",
            "model_material_change",
            None,
        );
        let DangerBridgeAdmissionOutcome::Admitted(proof) = ordinary_bridge
            .admit_manifest_with_model_conclusions(
                &ordinary_manifest,
                &[ServerModelDangerConclusion::BoundedMaterialDanger(
                    AuthoritativeModelDangerResult::RenderExactTargetUnusable,
                )],
            )
        else {
            panic!("model-added danger must be admitted through the proof")
        };
        assert_eq!(proof.routes().len(), 1);
        assert!(matches!(
            proof.routes()[0].view(),
            DangerRouteView::RequiresOneConfirmation(_)
        ));

        let known_bridge = DangerActionBridge::from_server_adapters(
            vec![
                ServerDangerAdapterRegistration::whole_drive_volume_boot_or_core_os(
                    tuple("resolved_destroy"),
                    "volume:system",
                ),
            ],
            ServerDangerAdapterState::default(),
        );
        let known_manifest = manifest("volume:system", "1.0.0", "resolved_destroy", None);
        let DangerBridgeAdmissionOutcome::Admitted(proof) = known_bridge
            .admit_manifest_with_model_conclusions(
                &known_manifest,
                &[ServerModelDangerConclusion::BoundedMaterialDanger(
                    AuthoritativeModelDangerResult::DestroyExactTarget,
                )],
            )
        else {
            panic!("known plus model danger must retain one confirmation")
        };
        assert_eq!(proof.routes().len(), 1);
        assert!(matches!(
            proof.routes()[0].view(),
            DangerRouteView::RequiresOneConfirmation(_)
        ));
    }

    #[test]
    fn model_observations_are_complete_exact_and_unique() {
        let bridge = DangerActionBridge::from_server_adapters(
            vec![ServerDangerAdapterRegistration::ordinary_exact(tuple(
                "inspect_exact",
            ))],
            ServerDangerAdapterState::default(),
        );
        let exact = manifest("target:exact", "1.0.0", "inspect_exact", None);
        assert!(matches!(
            bridge.admit_manifest(&exact),
            DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                reason: DangerResolverUnresolved::ModelObservationMissing,
                ..
            }
        ));

        let other = manifest("target:other", "1.0.0", "inspect_exact", None);
        let mismatched =
            ServerModelDangerObservation::no_additional_material_danger(&other.leaves()[0])
                .unwrap();
        assert!(matches!(
            bridge.admit_manifest_with_model_observations(&exact, &[mismatched]),
            DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                reason: DangerResolverUnresolved::ModelObservationMismatch,
                ..
            }
        ));

        let observation =
            ServerModelDangerObservation::no_additional_material_danger(&exact.leaves()[0])
                .unwrap();
        assert!(matches!(
            bridge.admit_manifest_with_model_observations(
                &exact,
                &[observation.clone(), observation]
            ),
            DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                reason: DangerResolverUnresolved::ModelObservationDuplicate,
                ..
            }
        ));
    }

    #[test]
    fn stale_unverified_and_duplicate_server_state_pause_mechanically() {
        let external_registration = ServerDangerAdapterRegistration::whole_external_account(
            tuple("resolved_destroy"),
            "provider-a",
            "tenant:a",
        );
        let external_manifest = manifest("tenant:a", "1.0.0", "resolved_destroy", None);
        let model_observation = ServerModelDangerObservation::no_additional_material_danger(
            &external_manifest.leaves()[0],
        )
        .unwrap();
        let stale = DangerActionBridge::from_server_adapters(
            vec![external_registration.clone()],
            ServerDangerAdapterState::default()
                .with_stale_external_account("provider-a", "tenant:a"),
        );
        assert!(matches!(
            stale.admit_manifest_with_model_observations(
                &external_manifest,
                std::slice::from_ref(&model_observation)
            ),
            DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                reason: DangerResolverUnresolved::AuthoritativeStateUnavailable,
                ..
            }
        ));
        let duplicate = DangerActionBridge::from_server_adapters(
            vec![external_registration],
            ServerDangerAdapterState::default()
                .with_verified_external_account("provider-a", "tenant:a")
                .with_verified_external_account("provider-a", "tenant:a"),
        );
        assert!(matches!(
            duplicate
                .admit_manifest_with_model_observations(&external_manifest, &[model_observation]),
            DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                reason: DangerResolverUnresolved::DuplicateAuthoritativeObservation,
                ..
            }
        ));

        let recovery = DangerActionBridge::from_server_adapters(
            vec![ServerDangerAdapterRegistration::last_recovery_path(
                tuple("resolved_destroy"),
                "recovery:a",
            )],
            ServerDangerAdapterState::default().with_unverified_recovery_path("recovery:a"),
        );
        let recovery_manifest = manifest("recovery:a", "1.0.0", "resolved_destroy", None);
        let observation = ServerModelDangerObservation::no_additional_material_danger(
            &recovery_manifest.leaves()[0],
        )
        .unwrap();
        assert!(matches!(
            recovery.admit_manifest_with_model_observations(&recovery_manifest, &[observation]),
            DangerBridgeAdmissionOutcome::MechanicalUnresolved {
                reason: DangerResolverUnresolved::AuthoritativeStateUnavailable,
                ..
            }
        ));
    }

    #[test]
    fn purchase_and_money_like_words_have_no_danger_authority() {
        let bridge = DangerActionBridge::from_server_adapters(
            vec![ServerDangerAdapterRegistration::ordinary_exact(tuple(
                "purchase_and_send_money",
            ))],
            ServerDangerAdapterState::default(),
        );
        let manifest = manifest(
            "merchant:invoice-1",
            "1.0.0",
            "purchase_and_send_money",
            None,
        );
        let observation =
            ServerModelDangerObservation::no_additional_material_danger(&manifest.leaves()[0])
                .unwrap();
        let DangerBridgeAdmissionOutcome::Admitted(proof) =
            bridge.admit_manifest_with_model_observations(&manifest, &[observation])
        else {
            panic!("exact registered ordinary work must proceed")
        };
        assert_eq!(proof.routes()[0].view(), DangerRouteView::Ordinary);
    }

    #[test]
    fn live_recovery_probe_marks_one_reachable_candidate_last_and_multiple_candidates_ordinary() {
        let temp = tempfile::tempdir().expect("temporary gateway state");
        let paths = AppPaths::from_root(temp.path().join("state"));
        carsinos_storage::init(&paths).expect("initialize test storage");
        let storage = Storage::from_paths(&paths);
        let primary = temp.path().join("primary-recovery");
        let secondary = temp.path().join("secondary-recovery");
        std::fs::create_dir_all(&primary).expect("primary recovery path");
        let primary_identity = canonical_path_identity(Some(primary.clone().into_os_string()))
            .expect("canonical primary recovery path");
        let tuple = tuple("resolved_destroy");
        let bridge = DangerActionBridge::from_server_paths_and_live_adapters(
            &paths,
            storage.clone(),
            Vec::new(),
            LiveDangerAdapterConfig {
                external_accounts: Vec::new(),
                recovery_domains: vec![LiveRecoveryDomainAdapter {
                    tuple: tuple.clone(),
                    recovery_domain_id: "owner".to_string(),
                    candidate_paths: vec![primary.clone(), secondary.clone()],
                }],
            },
        );
        let exact = manifest(&primary_identity, "1.0.0", "resolved_destroy", None);
        let observation =
            ServerModelDangerObservation::no_additional_material_danger(&exact.leaves()[0])
                .expect("server model observation");
        let DangerBridgeAdmissionOutcome::Admitted(proof) =
            bridge.admit_manifest_with_model_observations(&exact, &[observation])
        else {
            panic!("one live reachable recovery candidate must require confirmation")
        };
        assert!(matches!(
            proof.routes()[0].view(),
            DangerRouteView::RequiresOneConfirmation(_)
        ));

        std::fs::create_dir_all(&secondary).expect("secondary recovery path");
        let secondary_identity = canonical_path_identity(Some(secondary.into_os_string()))
            .expect("canonical secondary recovery path");
        let multiple = manifest(&secondary_identity, "1.0.0", "resolved_destroy", None);
        let observation =
            ServerModelDangerObservation::no_additional_material_danger(&multiple.leaves()[0])
                .expect("server model observation");
        let DangerBridgeAdmissionOutcome::Admitted(proof) =
            bridge.admit_manifest_with_model_observations(&multiple, &[observation])
        else {
            panic!("a live non-last candidate must resolve mechanically to ordinary")
        };
        assert_eq!(proof.routes()[0].view(), DangerRouteView::Ordinary);
    }
}
