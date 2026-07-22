//! Canonical operational-profile compilation and authority precedence for ExecAss.
//!
//! This module deliberately models only operational behavior. Dispatch manifests,
//! ingress assurance, persistence, confirmation, and effects are separate concerns.

use crate::execass_actor::VerifiedOwnerAuthority;
use crate::execass_manifest::{canonicalize_owner_authority, CanonicalLeafAction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

/// The four guided operational-profile inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationalProfileInput {
    LockedDown,
    Balanced,
    FullSend,
    Custom(CustomOperationalProfile),
}

/// Owner-selected settings for the `custom` profile.
///
/// The value is already closed and typed, so custom settings compile to the same
/// canonical representation as the three presets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomOperationalProfile {
    pub policy: CanonicalOperationalPolicy,
}

/// The one canonical representation shared by every operational profile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CanonicalOperationalPolicy {
    pub task_delegation: TaskDelegationScope,
    pub workspace_path: WorkspacePathScope,
    pub routine: RoutineScope,
    pub tool_identity: ToolIdentityScope,
    pub target: TargetScope,
    pub audience: AudienceScope,
    pub technical_quota: TechnicalQuota,
    pub time_expiry: TimeExpiryScope,
    pub recovery: RecoveryScope,
    pub parallelism: ParallelismScope,
    pub clarification_sensitivity: ClarificationSensitivity,
    /// A later matcher may route this to the one optional confirmation path only.
    /// It cannot veto, repeat a prompt, or change exact-owner authority.
    pub model_danger_confirmation_sensitivity: ModelDangerConfirmationSensitivity,
    pub recurring_work: RecurringWorkScope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskDelegationScope {
    ExactTask,
    NarrowDelegation,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePathScope {
    ExactPath,
    ProjectBounded,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutineScope {
    Disabled,
    ExplicitRoutine,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolIdentityScope {
    FixedIdentityAndVersion,
    ConfiguredIdentityAndVersion,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetScope {
    ExactTarget,
    ResolvedTargetSet,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudienceScope {
    ExactAudience,
    ResolvedAudienceSet,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalQuota {
    Constrained,
    Standard,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeExpiryScope {
    ExplicitExpiry,
    BoundedExpiry,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryScope {
    ManualOnly,
    ObjectiveRetry,
    ObjectiveRetryWithinOwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParallelismScope {
    Sequential,
    Bounded,
    OwnerEnvelope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationSensitivity {
    AnyAmbiguity,
    MaterialAmbiguity,
    UnresolvedOnly,
}

/// How readily a later model-danger matcher may use the single confirmation path.
///
/// This is operational routing metadata only. The EA-206 matcher owns danger
/// detection and the one-confirmation flow; this axis cannot create a veto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelDangerConfirmationSensitivity {
    PlausibleModelIdentifiedDanger,
    CredibleModelIdentifiedDanger,
    CredibleDestructiveOrDangerousConsequence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecurringWorkScope {
    Disabled,
    NarrowMembership,
    OwnerEnvelope,
}

/// The closed technical-capacity vocabulary. These values are deliberately
/// unrelated to money, commercial intent, or permission to perform an action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalResourceKind {
    Tokens,
    TimeMs,
    ConnectorCalls,
    ResourceUnits,
}

impl TechnicalResourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tokens => "tokens",
            Self::TimeMs => "time_ms",
            Self::ConnectorCalls => "connector_calls",
            Self::ResourceUnits => "resource_units",
        }
    }
}

/// Server-compiled quota input for one exact technical bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TechnicalQuotaEntryInput {
    pub kind: TechnicalResourceKind,
    pub unit: String,
    pub limit: i64,
}

/// Immutable canonical entry persisted under one quota snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTechnicalQuotaEntry {
    pub kind: TechnicalResourceKind,
    pub unit: String,
    pub limit: i64,
}

/// Exact immutable accounting authority for one policy/manifest scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTechnicalQuotaSnapshot {
    pub quota_snapshot_id: String,
    pub delegation_id: String,
    pub policy_revision: i64,
    pub effective_authority_digest: String,
    pub scope_key: String,
    pub entries: Vec<CanonicalTechnicalQuotaEntry>,
    pub canonical_entries_json: String,
    pub canonical_entries_digest: String,
}

/// Canonicalize a server-owned technical quota snapshot. An empty entry list
/// is explicit and means that the action has no metered technical capacity.
pub fn compile_technical_quota_snapshot(
    delegation_id: &str,
    policy_revision: i64,
    effective_authority_digest: &str,
    scope_key: &str,
    entries: Vec<TechnicalQuotaEntryInput>,
) -> Result<CanonicalTechnicalQuotaSnapshot, String> {
    for (name, value) in [
        ("delegation_id", delegation_id),
        ("effective_authority_digest", effective_authority_digest),
        ("scope_key", scope_key),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(format!("{name} must be nonempty canonical text"));
        }
    }
    if policy_revision <= 0 {
        return Err("policy_revision must be positive".to_string());
    }

    let mut canonical_entries = Vec::with_capacity(entries.len());
    let mut identities = BTreeSet::new();
    for entry in entries {
        if entry.limit < 0 {
            return Err("technical quota limit cannot be negative".to_string());
        }
        validate_technical_unit(entry.kind, &entry.unit)?;
        if !identities.insert((entry.kind, entry.unit.clone())) {
            return Err("duplicate technical quota kind/unit".to_string());
        }
        canonical_entries.push(CanonicalTechnicalQuotaEntry {
            kind: entry.kind,
            unit: entry.unit,
            limit: entry.limit,
        });
    }
    canonical_entries.sort_by(|left, right| {
        (left.kind, left.unit.as_str()).cmp(&(right.kind, right.unit.as_str()))
    });
    let canonical_entries_json = serde_json::to_string(&canonical_entries)
        .map_err(|error| format!("failed serializing technical quota entries: {error}"))?;
    let canonical_entries_digest = domain_digest(
        b"carsinos.execass.technical-quota.entries.v1\0",
        canonical_entries_json.as_bytes(),
    );
    let identity_payload = format!(
        "{delegation_id}\0{policy_revision}\0{effective_authority_digest}\0{scope_key}\0{canonical_entries_digest}"
    );
    let quota_snapshot_id = domain_digest(
        b"carsinos.execass.technical-quota.snapshot.v1\0",
        identity_payload.as_bytes(),
    );
    Ok(CanonicalTechnicalQuotaSnapshot {
        quota_snapshot_id,
        delegation_id: delegation_id.to_owned(),
        policy_revision,
        effective_authority_digest: effective_authority_digest.to_owned(),
        scope_key: scope_key.to_owned(),
        entries: canonical_entries,
        canonical_entries_json,
        canonical_entries_digest,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TechnicalResourceRequirementInput {
    pub kind: TechnicalResourceKind,
    pub unit: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTechnicalResourceRequirement {
    pub kind: TechnicalResourceKind,
    pub unit: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTechnicalResourceRequirementSet {
    pub requirement_set_id: String,
    pub quota_snapshot_id: String,
    pub delegation_id: String,
    pub logical_effect_id: String,
    pub action_id: String,
    pub manifest_digest: String,
    pub requirements: Vec<CanonicalTechnicalResourceRequirement>,
    pub canonical_requirements_json: String,
    pub canonical_requirements_digest: String,
}

/// Compile the exact amount one logical effect requires from an immutable
/// shared quota snapshot. Limits remain in the snapshot; requirements cannot
/// create an alias bucket or exceed its corresponding limit.
pub fn compile_technical_resource_requirements(
    snapshot: &CanonicalTechnicalQuotaSnapshot,
    logical_effect_id: &str,
    action_id: &str,
    manifest_digest: &str,
    requirements: Vec<TechnicalResourceRequirementInput>,
) -> Result<CanonicalTechnicalResourceRequirementSet, String> {
    for (name, value) in [
        ("logical_effect_id", logical_effect_id),
        ("action_id", action_id),
        ("manifest_digest", manifest_digest),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(format!("{name} must be nonempty canonical text"));
        }
    }
    let mut identities = BTreeSet::new();
    let mut canonical = Vec::with_capacity(requirements.len());
    for requirement in requirements {
        if requirement.amount <= 0 {
            return Err("technical resource requirement must be positive".to_string());
        }
        validate_technical_unit(requirement.kind, &requirement.unit)?;
        if !identities.insert((requirement.kind, requirement.unit.clone())) {
            return Err("duplicate technical resource requirement kind/unit".to_string());
        }
        let Some(quota) = snapshot
            .entries
            .iter()
            .find(|entry| entry.kind == requirement.kind && entry.unit == requirement.unit)
        else {
            return Err("technical requirement has no quota snapshot entry".to_string());
        };
        if requirement.amount > quota.limit {
            return Err("technical resource requirement exceeds its quota limit".to_string());
        }
        canonical.push(CanonicalTechnicalResourceRequirement {
            kind: requirement.kind,
            unit: requirement.unit,
            amount: requirement.amount,
        });
    }
    canonical.sort_by(|left, right| {
        (left.kind, left.unit.as_str()).cmp(&(right.kind, right.unit.as_str()))
    });
    let canonical_requirements_json = serde_json::to_string(&canonical)
        .map_err(|error| format!("failed serializing technical requirements: {error}"))?;
    let canonical_requirements_digest = domain_digest(
        b"carsinos.execass.technical-resource.requirements.v1\0",
        canonical_requirements_json.as_bytes(),
    );
    let identity = format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        snapshot.quota_snapshot_id,
        snapshot.delegation_id,
        logical_effect_id,
        action_id,
        manifest_digest,
        canonical_requirements_digest
    );
    Ok(CanonicalTechnicalResourceRequirementSet {
        requirement_set_id: domain_digest(
            b"carsinos.execass.technical-resource.requirement-set.v1\0",
            identity.as_bytes(),
        ),
        quota_snapshot_id: snapshot.quota_snapshot_id.clone(),
        delegation_id: snapshot.delegation_id.clone(),
        logical_effect_id: logical_effect_id.to_owned(),
        action_id: action_id.to_owned(),
        manifest_digest: manifest_digest.to_owned(),
        requirements: canonical,
        canonical_requirements_json,
        canonical_requirements_digest,
    })
}

/// Bind quota compilation to the exact persisted effective-authority bytes.
/// The caller must supply the canonical storage representation; this digest is
/// provenance and drift protection, not the numeric quota authority itself.
pub fn technical_effective_authority_digest(canonical_json: &str) -> Result<String, String> {
    if canonical_json.trim().is_empty() || canonical_json.trim() != canonical_json {
        return Err("effective authority must be nonempty canonical JSON text".to_string());
    }
    serde_json::from_str::<serde_json::Value>(canonical_json)
        .map_err(|error| format!("effective authority is not valid JSON: {error}"))?;
    Ok(domain_digest(
        b"carsinos.execass.technical-quota.authority.v1\0",
        canonical_json.as_bytes(),
    ))
}

/// Create the only valid connector-call unit from a canonical connector/tool
/// identity and version. Raw labels cannot split one bucket into aliases.
pub fn canonical_connector_call_unit(identity: &str, version: &str) -> Result<String, String> {
    validate_registry_identity(identity)?;
    validate_connector_version(version)?;
    canonical_hashed_unit("connector", identity, version)
}

/// Create the only valid generic-resource unit from a canonical resource
/// identity and version/scope discriminator.
pub fn canonical_resource_unit(identity: &str, version: &str) -> Result<String, String> {
    validate_registry_identity(identity)?;
    validate_registry_scope(version)?;
    canonical_hashed_unit("resource", identity, version)
}

fn canonical_hashed_unit(prefix: &str, identity: &str, version: &str) -> Result<String, String> {
    let material = format!("{identity}\0{version}");
    let digest = domain_digest(
        b"carsinos.execass.technical-resource.unit.v1\0",
        material.as_bytes(),
    );
    Ok(format!(
        "{prefix}:{}",
        digest
            .strip_prefix("sha256:")
            .expect("domain digests always have the sha256 prefix")
    ))
}

fn validate_registry_identity(value: &str) -> Result<(), String> {
    validate_lower_ascii_registry_key("technical resource registry identity", value)
}

fn validate_registry_scope(value: &str) -> Result<(), String> {
    validate_lower_ascii_registry_key("technical resource registry scope", value)
}

fn validate_lower_ascii_registry_key(name: &str, value: &str) -> Result<(), String> {
    if value.nfc().collect::<String>() != value {
        return Err(format!("{name} must be NFC normalized"));
    }
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return Err(format!(
            "{name} must be a nonempty lowercase ASCII registry key"
        ));
    }
    let mut previous_separator = false;
    for (index, byte) in value.bytes().enumerate() {
        let separator = matches!(byte, b'.' | b'-');
        let valid = byte.is_ascii_lowercase()
            || (index > 0 && byte.is_ascii_digit())
            || (index > 0 && separator && !previous_separator);
        if !valid {
            return Err(format!(
                "{name} must use lowercase ASCII segments separated by one '.' or '-'"
            ));
        }
        previous_separator = separator;
    }
    if previous_separator {
        return Err(format!("{name} cannot end with a separator"));
    }
    Ok(())
}

fn validate_connector_version(value: &str) -> Result<(), String> {
    if value.nfc().collect::<String>() != value {
        return Err("connector registry version must be NFC normalized".to_string());
    }
    let Some(number) = value.strip_prefix('v') else {
        return Err("connector registry version must use canonical vN grammar".to_string());
    };
    if number.is_empty()
        || !number.bytes().all(|byte| byte.is_ascii_digit())
        || number.starts_with('0')
    {
        return Err("connector registry version must use canonical vN grammar".to_string());
    }
    Ok(())
}

fn validate_technical_unit(kind: TechnicalResourceKind, unit: &str) -> Result<(), String> {
    let valid = match kind {
        TechnicalResourceKind::Tokens => unit == "token",
        TechnicalResourceKind::TimeMs => unit == "ms",
        TechnicalResourceKind::ConnectorCalls => valid_hashed_unit(unit, "connector:"),
        TechnicalResourceKind::ResourceUnits => valid_hashed_unit(unit, "resource:"),
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid canonical unit for technical resource {}",
            kind.as_str()
        ))
    }
}

fn valid_hashed_unit(unit: &str, prefix: &str) -> bool {
    unit.strip_prefix(prefix).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn domain_digest(domain: &[u8], value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value);
    format!("sha256:{:x}", digest.finalize())
}

/// Compiles a guided input into its deterministic canonical policy.
pub fn compile_operational_profile(input: OperationalProfileInput) -> CanonicalOperationalPolicy {
    match input {
        OperationalProfileInput::LockedDown => CanonicalOperationalPolicy {
            task_delegation: TaskDelegationScope::ExactTask,
            workspace_path: WorkspacePathScope::ExactPath,
            routine: RoutineScope::Disabled,
            tool_identity: ToolIdentityScope::FixedIdentityAndVersion,
            target: TargetScope::ExactTarget,
            audience: AudienceScope::ExactAudience,
            technical_quota: TechnicalQuota::Constrained,
            time_expiry: TimeExpiryScope::ExplicitExpiry,
            recovery: RecoveryScope::ManualOnly,
            parallelism: ParallelismScope::Sequential,
            clarification_sensitivity: ClarificationSensitivity::AnyAmbiguity,
            model_danger_confirmation_sensitivity:
                ModelDangerConfirmationSensitivity::PlausibleModelIdentifiedDanger,
            recurring_work: RecurringWorkScope::Disabled,
        },
        OperationalProfileInput::Balanced => CanonicalOperationalPolicy {
            task_delegation: TaskDelegationScope::NarrowDelegation,
            workspace_path: WorkspacePathScope::ProjectBounded,
            routine: RoutineScope::ExplicitRoutine,
            tool_identity: ToolIdentityScope::ConfiguredIdentityAndVersion,
            target: TargetScope::ResolvedTargetSet,
            audience: AudienceScope::ResolvedAudienceSet,
            technical_quota: TechnicalQuota::Standard,
            time_expiry: TimeExpiryScope::BoundedExpiry,
            recovery: RecoveryScope::ObjectiveRetry,
            parallelism: ParallelismScope::Bounded,
            clarification_sensitivity: ClarificationSensitivity::MaterialAmbiguity,
            model_danger_confirmation_sensitivity:
                ModelDangerConfirmationSensitivity::CredibleModelIdentifiedDanger,
            recurring_work: RecurringWorkScope::NarrowMembership,
        },
        OperationalProfileInput::FullSend => CanonicalOperationalPolicy {
            task_delegation: TaskDelegationScope::OwnerEnvelope,
            workspace_path: WorkspacePathScope::OwnerEnvelope,
            routine: RoutineScope::OwnerEnvelope,
            tool_identity: ToolIdentityScope::OwnerEnvelope,
            target: TargetScope::OwnerEnvelope,
            audience: AudienceScope::OwnerEnvelope,
            technical_quota: TechnicalQuota::OwnerEnvelope,
            time_expiry: TimeExpiryScope::OwnerEnvelope,
            recovery: RecoveryScope::ObjectiveRetryWithinOwnerEnvelope,
            parallelism: ParallelismScope::OwnerEnvelope,
            clarification_sensitivity: ClarificationSensitivity::UnresolvedOnly,
            model_danger_confirmation_sensitivity:
                ModelDangerConfirmationSensitivity::CredibleDestructiveOrDangerousConsequence,
            recurring_work: RecurringWorkScope::OwnerEnvelope,
        },
        OperationalProfileInput::Custom(custom) => custom.policy,
    }
}

/// Persistable profile-configuration state for the first-run admission seam.
///
/// `Unconfigured` deliberately has no implicit policy. In particular, the
/// presentation layer may highlight Balanced while this value remains exactly
/// `Unconfigured` until a distinct owner selection is supplied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProfileConfigurationState {
    Unconfigured,
    Configured(ConfiguredOperationalProfile),
}

/// The identity deliberately selected by the owner during profile setup.
///
/// This remains distinct from the compiled settings: a custom profile may be
/// behavior-equivalent to a preset while still being owner-selected as custom.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationalProfileKind {
    LockedDown,
    Balanced,
    FullSend,
    Custom,
}

/// The durable-shaped result of an explicit owner profile selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ConfiguredOperationalProfile {
    pub selected_profile: OperationalProfileKind,
    pub canonical_policy: CanonicalOperationalPolicy,
}

/// The ingress presenting work to the shared profile-admission seam.
///
/// This is routing metadata only; assurance of a current owner and durable
/// intake are owned by later adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileAdmissionIngress {
    Api,
    NativeControl,
}

/// Closed work categories that matter before an operational profile exists.
///
/// The two exact-current-owner variants retain base-objective authority on a
/// fresh root. Saved, standing, and derived work are unattended behavior and
/// therefore require guided selection first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileAdmissionWork {
    ExactCurrentOwnerInstruction,
    ExactCurrentOwnerPolicyAmendment,
    SavedOwnerInstruction,
    StandingOwnerWork,
    DerivedOrUnattendedWork,
    ExplicitOwnerProfileSelection(OperationalProfileInput),
}

/// Presentation-only guided-selection state.
///
/// It intentionally cannot be compiled into a policy or used as a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSelectionHighlight {
    Balanced,
}

/// The result shared by future API and native-control adapters.
///
/// It does not approve effects, create a confirmation, or make an owner
/// assurance claim. Callers apply base objective and later danger rules after
/// this first-run/profile admission decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileAdmission {
    ProceedUnderBaseRules {
        configured_policy: Option<CanonicalOperationalPolicy>,
    },
    GuidedSelectionRequired {
        configuration_state: ProfileConfigurationState,
        presentation_highlight: ProfileSelectionHighlight,
    },
    Configured {
        configuration_state: ProfileConfigurationState,
    },
}

/// Evaluates first-run profile admission without persistence or side effects.
///
/// Exact current-owner work can proceed under the base objective rules even on
/// an unconfigured root. All unattended categories require guided selection on
/// that root. A profile is configured only by the explicit-selection variant.
pub fn admit_profile_work(
    _ingress: ProfileAdmissionIngress,
    configuration_state: ProfileConfigurationState,
    work: ProfileAdmissionWork,
) -> ProfileAdmission {
    match work {
        ProfileAdmissionWork::ExplicitOwnerProfileSelection(selection) => {
            ProfileAdmission::Configured {
                configuration_state: ProfileConfigurationState::Configured(
                    ConfiguredOperationalProfile {
                        selected_profile: operational_profile_kind(&selection),
                        canonical_policy: compile_operational_profile(selection),
                    },
                ),
            }
        }
        ProfileAdmissionWork::ExactCurrentOwnerInstruction
        | ProfileAdmissionWork::ExactCurrentOwnerPolicyAmendment => {
            ProfileAdmission::ProceedUnderBaseRules {
                configured_policy: None,
            }
        }
        ProfileAdmissionWork::SavedOwnerInstruction
        | ProfileAdmissionWork::StandingOwnerWork
        | ProfileAdmissionWork::DerivedOrUnattendedWork => match configuration_state {
            ProfileConfigurationState::Unconfigured => ProfileAdmission::GuidedSelectionRequired {
                configuration_state: ProfileConfigurationState::Unconfigured,
                presentation_highlight: ProfileSelectionHighlight::Balanced,
            },
            ProfileConfigurationState::Configured(profile) => {
                ProfileAdmission::ProceedUnderBaseRules {
                    configured_policy: Some(profile.canonical_policy),
                }
            }
        },
    }
}

fn operational_profile_kind(input: &OperationalProfileInput) -> OperationalProfileKind {
    match input {
        OperationalProfileInput::LockedDown => OperationalProfileKind::LockedDown,
        OperationalProfileInput::Balanced => OperationalProfileKind::Balanced,
        OperationalProfileInput::FullSend => OperationalProfileKind::FullSend,
        OperationalProfileInput::Custom(_) => OperationalProfileKind::Custom,
    }
}

/// The source that attempts to authorize the present action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoritySource {
    CurrentExactOwnerInstruction,
    CurrentExactOwnerPolicyAmendment,
    SavedOwnerInstruction,
    DerivedOrUnattended,
    NonHumanOnly,
}

/// Objective execution checks performed before authority can proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechnicalValidity {
    Valid,
    ActionIdentityUnresolved,
    CapabilityUnavailable,
    OperandUnresolved,
    RuntimePreconditionUnmet,
    TransactionOrFencingInvalid,
    ReconciliationUnavailable,
    ResourceUnavailable,
}

/// Whether the action identity was mechanically resolved into the frozen leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionIdentityResolution {
    Resolved,
    Unresolved,
}

/// Whether every operand was mechanically resolved into the frozen leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandResolution {
    Resolved,
    Unresolved,
}

/// Whether the exact capability needed by the frozen leaf is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityAvailability {
    Available,
    Unavailable,
}

/// Whether current runtime preconditions for the frozen leaf are met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePreconditionState {
    Met,
    Unmet,
}

/// Whether the required transaction and fencing boundary is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionFencingState {
    Valid,
    Invalid,
}

/// Whether the required reconciliation path is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationAvailability {
    Available,
    Unavailable,
}

/// Whether the concrete resources needed by the frozen leaf are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAvailability {
    Available,
    Unavailable,
}

/// Server-observed mechanical facts for objective technical validity.
///
/// Each field represents one closed technical condition. The caller supplies
/// neither a validity result nor leaf-binding material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectiveTechnicalValidityFacts {
    pub action_identity: ActionIdentityResolution,
    pub operands: OperandResolution,
    pub capability: CapabilityAvailability,
    pub runtime_precondition: RuntimePreconditionState,
    pub transaction_and_fencing: TransactionFencingState,
    pub reconciliation: ReconciliationAvailability,
    pub resources: ResourceAvailability,
}

/// Whether an existing owner envelope exactly covers the action under evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerEnvelopeState {
    pub frozen_version_matches: bool,
    pub action_within_envelope: bool,
}

/// Inputs needed for the pure authority-precedence evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityEvaluationInput {
    pub stopped: bool,
    pub revoked: bool,
    pub superseded_by_owner_amendment: bool,
    pub technical_validity: TechnicalValidity,
    pub authority_source: AuthoritySource,
    pub owner_envelope: Option<OwnerEnvelopeState>,
    pub operational_profile: Option<OperationalProfileInput>,
}

/// The authority result, intentionally separate from later decision/effect flows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityEvaluation {
    Paused(PauseReason),
    AuthorizedExactOwnerInstruction,
    AuthorizedExactOwnerPolicyAmendment,
    AuthorizedSavedOwnerInstruction,
    AuthorizedDerivedOrUnattended {
        operational_policy: CanonicalOperationalPolicy,
    },
    NoOwnerAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    Stopped,
    Revoked,
    SupersededByOwnerAmendment,
    TechnicalValidity(TechnicalValidity),
    OwnerEnvelopeRequired,
    OwnerEnvelopeVersionMismatch,
    ActionOutsideOwnerEnvelope,
    ProfileSelectionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactOwnerAuthorityKind {
    Instruction,
    OperationalPolicyAmendment,
}

/// Opaque proof that a trusted technical evaluator assessed one exact frozen
/// leaf. Production callers cannot construct or alter it. Later execution
/// phases must still revalidate current runtime state before an effect.
///
/// ```compile_fail
/// use carsinos_core::execass_policy::{
///     ObjectiveTechnicalValidityProof, TechnicalValidity,
/// };
/// let _forged = ObjectiveTechnicalValidityProof {
///     canonical_leaf_digest: String::new(),
///     validity: TechnicalValidity::Valid,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveTechnicalValidityProof {
    canonical_leaf_digest: String,
    validity: TechnicalValidity,
}

impl ObjectiveTechnicalValidityProof {
    fn matches(&self, canonical_leaf: &CanonicalLeafAction) -> bool {
        self.canonical_leaf_digest == canonical_leaf.canonical().digest().as_hex()
    }

    fn validity(&self) -> TechnicalValidity {
        self.validity
    }
}

/// Evaluates server-observed mechanical facts and binds the result to one exact
/// canonical leaf. When several conditions fail, the stable precedence is:
/// action identity, operands, capability, runtime precondition,
/// transaction/fencing, reconciliation, then resources.
pub fn evaluate_objective_technical_validity(
    canonical_leaf: &CanonicalLeafAction,
    facts: ObjectiveTechnicalValidityFacts,
) -> ObjectiveTechnicalValidityProof {
    let validity = if facts.action_identity == ActionIdentityResolution::Unresolved {
        TechnicalValidity::ActionIdentityUnresolved
    } else if facts.operands == OperandResolution::Unresolved {
        TechnicalValidity::OperandUnresolved
    } else if facts.capability == CapabilityAvailability::Unavailable {
        TechnicalValidity::CapabilityUnavailable
    } else if facts.runtime_precondition == RuntimePreconditionState::Unmet {
        TechnicalValidity::RuntimePreconditionUnmet
    } else if facts.transaction_and_fencing == TransactionFencingState::Invalid {
        TechnicalValidity::TransactionOrFencingInvalid
    } else if facts.reconciliation == ReconciliationAvailability::Unavailable {
        TechnicalValidity::ReconciliationUnavailable
    } else if facts.resources == ResourceAvailability::Unavailable {
        TechnicalValidity::ResourceUnavailable
    } else {
        TechnicalValidity::Valid
    };
    ObjectiveTechnicalValidityProof {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        validity,
    }
}

/// Test-only issuer for objective technical assessment fixtures. This feature
/// is disabled in production builds; real issuance belongs to the trusted
/// canonical intake/runtime evaluator rather than a wire or caller claim.
#[cfg(any(test, feature = "execass-test-authority"))]
pub fn issue_test_objective_technical_validity_proof(
    canonical_leaf: &CanonicalLeafAction,
    validity: TechnicalValidity,
) -> ObjectiveTechnicalValidityProof {
    ObjectiveTechnicalValidityProof {
        canonical_leaf_digest: canonical_leaf.canonical().digest().as_hex().to_string(),
        validity,
    }
}

pub struct ExactOwnerAuthorityInput<'a> {
    pub verified_owner_authority: &'a VerifiedOwnerAuthority,
    pub canonical_leaf: &'a CanonicalLeafAction,
    pub stopped: bool,
    pub revoked: bool,
    pub superseded_by_owner_amendment: bool,
    pub technical_validity: &'a ObjectiveTechnicalValidityProof,
}

/// Opaque proof that one exact canonical leaf passed the sole EA-205 owner-
/// authority path. It does not mean the action is runnable: EA-206 still owns
/// dangerous-action confirmation and later phases own persistence/dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactOwnerActionAuthority {
    kind: ExactOwnerAuthorityKind,
    authority_provenance_id: String,
    logical_action_id: String,
    canonical_leaf_digest: String,
}

impl ExactOwnerActionAuthority {
    pub fn kind(&self) -> ExactOwnerAuthorityKind {
        self.kind
    }

    pub fn authority_provenance_id(&self) -> &str {
        &self.authority_provenance_id
    }

    pub fn logical_action_id(&self) -> &str {
        &self.logical_action_id
    }

    pub fn canonical_leaf_digest(&self) -> &str {
        &self.canonical_leaf_digest
    }

    pub fn matches(
        &self,
        verified_owner_authority: &VerifiedOwnerAuthority,
        canonical_leaf: &CanonicalLeafAction,
    ) -> bool {
        self.authority_provenance_id == verified_owner_authority.authority_provenance_id()
            && self.logical_action_id == canonical_leaf.logical_action_id()
            && self.canonical_leaf_digest == canonical_leaf.canonical().digest().as_hex()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactOwnerAuthorityOutcome {
    Authorized(ExactOwnerActionAuthority),
    Paused(PauseReason),
    NoOwnerAuthority,
}

/// The only public EA-205 exact-owner authority entry point.
///
/// It binds verified human provenance to an exact mechanically resolved leaf.
/// No action category, purpose, commerce, permission class, model score, or
/// operational profile is an input. Dangerous-action routing remains EA-206.
pub fn authorize_exact_owner_leaf(
    input: ExactOwnerAuthorityInput<'_>,
) -> ExactOwnerAuthorityOutcome {
    let Ok(canonical_authority) = canonicalize_owner_authority(input.verified_owner_authority)
    else {
        return ExactOwnerAuthorityOutcome::NoOwnerAuthority;
    };
    if &canonical_authority != input.canonical_leaf.owner_authority() {
        return ExactOwnerAuthorityOutcome::NoOwnerAuthority;
    }
    if !input.technical_validity.matches(input.canonical_leaf) {
        return ExactOwnerAuthorityOutcome::NoOwnerAuthority;
    }
    let (authority_source, kind) = match input.verified_owner_authority.authority_kind() {
        "original_request" => (
            AuthoritySource::CurrentExactOwnerInstruction,
            ExactOwnerAuthorityKind::Instruction,
        ),
        "policy_snapshot" => (
            AuthoritySource::CurrentExactOwnerPolicyAmendment,
            ExactOwnerAuthorityKind::OperationalPolicyAmendment,
        ),
        _ => return ExactOwnerAuthorityOutcome::NoOwnerAuthority,
    };
    match evaluate_authority(AuthorityEvaluationInput {
        stopped: input.stopped,
        revoked: input.revoked,
        superseded_by_owner_amendment: input.superseded_by_owner_amendment,
        technical_validity: input.technical_validity.validity(),
        authority_source,
        owner_envelope: None,
        operational_profile: None,
    }) {
        AuthorityEvaluation::AuthorizedExactOwnerInstruction
        | AuthorityEvaluation::AuthorizedExactOwnerPolicyAmendment => {
            ExactOwnerAuthorityOutcome::Authorized(ExactOwnerActionAuthority {
                kind,
                authority_provenance_id: input
                    .verified_owner_authority
                    .authority_provenance_id()
                    .to_string(),
                logical_action_id: input.canonical_leaf.logical_action_id().to_string(),
                canonical_leaf_digest: input
                    .canonical_leaf
                    .canonical()
                    .digest()
                    .as_hex()
                    .to_string(),
            })
        }
        AuthorityEvaluation::Paused(reason) => ExactOwnerAuthorityOutcome::Paused(reason),
        _ => ExactOwnerAuthorityOutcome::NoOwnerAuthority,
    }
}

/// Applies precedence without categorically denying an otherwise resolvable action.
pub(crate) fn evaluate_authority(input: AuthorityEvaluationInput) -> AuthorityEvaluation {
    if input.stopped {
        return AuthorityEvaluation::Paused(PauseReason::Stopped);
    }
    if input.revoked {
        return AuthorityEvaluation::Paused(PauseReason::Revoked);
    }
    if input.superseded_by_owner_amendment {
        return AuthorityEvaluation::Paused(PauseReason::SupersededByOwnerAmendment);
    }
    if input.technical_validity != TechnicalValidity::Valid {
        return AuthorityEvaluation::Paused(PauseReason::TechnicalValidity(
            input.technical_validity,
        ));
    }

    match input.authority_source {
        AuthoritySource::CurrentExactOwnerInstruction => {
            AuthorityEvaluation::AuthorizedExactOwnerInstruction
        }
        AuthoritySource::CurrentExactOwnerPolicyAmendment => {
            AuthorityEvaluation::AuthorizedExactOwnerPolicyAmendment
        }
        AuthoritySource::SavedOwnerInstruction => {
            match usable_owner_envelope(input.owner_envelope) {
                Ok(_) => AuthorityEvaluation::AuthorizedSavedOwnerInstruction,
                Err(reason) => AuthorityEvaluation::Paused(reason),
            }
        }
        AuthoritySource::DerivedOrUnattended => {
            match usable_owner_envelope(input.owner_envelope) {
                Ok(_) => {}
                Err(reason) => return AuthorityEvaluation::Paused(reason),
            }
            let Some(profile) = input.operational_profile else {
                return AuthorityEvaluation::Paused(PauseReason::ProfileSelectionRequired);
            };
            AuthorityEvaluation::AuthorizedDerivedOrUnattended {
                operational_policy: compile_operational_profile(profile),
            }
        }
        AuthoritySource::NonHumanOnly => AuthorityEvaluation::NoOwnerAuthority,
    }
}

fn usable_owner_envelope(
    owner_envelope: Option<OwnerEnvelopeState>,
) -> Result<OwnerEnvelopeState, PauseReason> {
    let Some(envelope) = owner_envelope else {
        return Err(PauseReason::OwnerEnvelopeRequired);
    };
    if !envelope.frozen_version_matches {
        return Err(PauseReason::OwnerEnvelopeVersionMismatch);
    }
    if !envelope.action_within_envelope {
        return Err(PauseReason::ActionOutsideOwnerEnvelope);
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execass_actor::{
        bind_verified_owner_authority, derive_base_actor_assurance, CallerActorClaims,
        LocalInteractiveEvidence, OwnerAuthoritySourceInput, RemoteOwnerEvidence,
        ServerIngressObservation,
    };
    use crate::execass_manifest::{
        compile_dispatch, CanonicalField, CanonicalValue, DispatchAction, DispatchNode,
        DispatchTree, ManifestCompilation, ResolvedLeafInput, ServerResolutionRegistry,
        TargetSnapshotInput, ToolIdentityInput,
    };

    #[derive(Debug, Clone, Copy)]
    enum TestIngress {
        Local,
        Remote,
    }

    #[derive(Debug, Clone, Copy)]
    enum TestOwnerAction {
        Communication,
        PermissionChange,
        ProjectMutation,
        PolicyAmendment,
        NarrowDeletion,
        SecretDelivery,
        ExistingExternalToolUse,
    }

    fn verified_authority(
        ingress: TestIngress,
        action: TestOwnerAction,
        seed: &str,
    ) -> VerifiedOwnerAuthority {
        let observation = match ingress {
            TestIngress::Local => {
                ServerIngressObservation::LocalInteractive(Box::new(LocalInteractiveEvidence {
                    authenticated_client_id: format!("desktop-{seed}"),
                    authenticated_ingress: "native-control".to_string(),
                    channel_assurance: "interactive-local".to_string(),
                    request_correlation_id: format!("correlation-{seed}"),
                    source_message_id: Some(format!("message-{seed}")),
                    interactive_owner_verified: true,
                }))
            }
            TestIngress::Remote => {
                ServerIngressObservation::RemoteAuthenticated(Box::new(RemoteOwnerEvidence {
                    adapter_id: "telegram".to_string(),
                    adapter_authenticated: true,
                    allowlisted_provider_account_id: "owner-account".to_string(),
                    observed_provider_account_id: "owner-account".to_string(),
                    authenticated_ingress: "telegram-provider-listener".to_string(),
                    channel_assurance: "provider-authenticated-owner".to_string(),
                    source_message_id: format!("remote-message-{seed}"),
                    request_correlation_id: format!("remote-correlation-{seed}"),
                    callback_fresh: true,
                }))
            }
        };
        let actor = derive_base_actor_assurance(&observation, &CallerActorClaims::default());
        bind_verified_owner_authority(
            &actor,
            OwnerAuthoritySourceInput {
                normalized_intent: format!("exact owner intent {seed}"),
                instruction_revision: format!("instruction-{seed}"),
                instruction_bytes: format!("exact owner instruction {seed}").into_bytes(),
                owner_envelope_revision: format!("envelope-{seed}"),
                owner_envelope_json: format!(r#"{{"exact":"{seed}"}}"#),
                authority_kind: if matches!(action, TestOwnerAction::PolicyAmendment) {
                    "policy_snapshot".to_string()
                } else {
                    "original_request".to_string()
                },
                normalized_scope_json: r#"{"instance":"single-owner"}"#.to_string(),
                policy_revision: 1,
                bound_decision_id: None,
                bound_decision_revision: None,
                bound_manifest_bytes: None,
                challenge_nonce_bytes: None,
                created_at: 1_800_000_000_000,
                expires_at: None,
            },
        )
        .expect("verified owner authority")
    }

    fn exact_leaf(
        action: TestOwnerAction,
        authority: VerifiedOwnerAuthority,
    ) -> CanonicalLeafAction {
        let (logical_action_id, action_kind, tool_id, operands, target) = match action {
            TestOwnerAction::Communication => (
                "action.communication",
                "connector_effect",
                "connector.send",
                CanonicalValue::Object(vec![CanonicalField {
                    key: "message".to_string(),
                    value: CanonicalValue::String("send the project update".to_string()),
                }]),
                "owner-chat",
            ),
            TestOwnerAction::PermissionChange => (
                "action.permission-change",
                "filesystem_effect",
                "filesystem.permissions",
                CanonicalValue::Object(vec![CanonicalField {
                    key: "access".to_string(),
                    value: CanonicalValue::String("read_write".to_string()),
                }]),
                "Z:\\carsinos\\project-data",
            ),
            TestOwnerAction::ProjectMutation => (
                "action.project-mutation",
                "filesystem_effect",
                "filesystem.write",
                CanonicalValue::Object(vec![CanonicalField {
                    key: "content_digest".to_string(),
                    value: CanonicalValue::String(
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            .to_string(),
                    ),
                }]),
                "Z:\\carsinos\\project-data\\note.txt",
            ),
            TestOwnerAction::PolicyAmendment => (
                "action.policy-amendment",
                "policy_revision_effect",
                "execass.policy.replace",
                CanonicalValue::Object(vec![CanonicalField {
                    key: "policy_revision".to_string(),
                    value: CanonicalValue::Integer(2),
                }]),
                "execass-operational-policy",
            ),
            TestOwnerAction::NarrowDeletion => (
                "action.narrow-deletion",
                "filesystem_effect",
                "filesystem.delete",
                CanonicalValue::Object(vec![CanonicalField {
                    key: "recursive".to_string(),
                    value: CanonicalValue::Bool(false),
                }]),
                "Z:\\carsinos\\project-data\\obsolete.tmp",
            ),
            TestOwnerAction::SecretDelivery => (
                "action.secret-delivery",
                "connector_effect",
                "connector.send_secret_ref",
                CanonicalValue::Object(vec![CanonicalField {
                    key: "secret_ref".to_string(),
                    value: CanonicalValue::String("secret://owner/example".to_string()),
                }]),
                "owner-private-channel",
            ),
            TestOwnerAction::ExistingExternalToolUse => (
                "action.external-tool-use",
                "external_tool_effect",
                "vendor.catalog.submit",
                CanonicalValue::Object(vec![
                    CanonicalField {
                        key: "item_sku".to_string(),
                        value: CanonicalValue::String("SKU-42".to_string()),
                    },
                    CanonicalField {
                        key: "quantity".to_string(),
                        value: CanonicalValue::Integer(1),
                    },
                ]),
                "configured-vendor-account",
            ),
        };
        let tree = DispatchTree {
            root_id: "root".to_string(),
            nodes: vec![DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                    logical_action_id: logical_action_id.to_string(),
                    action_kind: action_kind.to_string(),
                    tool: ToolIdentityInput {
                        tool_id: tool_id.to_string(),
                        version: "1.0.0".to_string(),
                    },
                    operands,
                    target_snapshot: TargetSnapshotInput {
                        targets: vec![CanonicalValue::String(target.to_string())],
                    },
                    material_digest: None,
                    owner_authority: authority,
                })),
            }],
        };
        let ManifestCompilation::Ready(manifest) =
            compile_dispatch(&tree, &ServerResolutionRegistry::default())
        else {
            panic!("exact action fixture must compile");
        };
        manifest.leaves()[0].clone()
    }

    fn valid_objective_technical_facts() -> ObjectiveTechnicalValidityFacts {
        ObjectiveTechnicalValidityFacts {
            action_identity: ActionIdentityResolution::Resolved,
            operands: OperandResolution::Resolved,
            capability: CapabilityAvailability::Available,
            runtime_precondition: RuntimePreconditionState::Met,
            transaction_and_fencing: TransactionFencingState::Valid,
            reconciliation: ReconciliationAvailability::Available,
            resources: ResourceAvailability::Available,
        }
    }

    fn valid_input(authority_source: AuthoritySource) -> AuthorityEvaluationInput {
        AuthorityEvaluationInput {
            stopped: false,
            revoked: false,
            superseded_by_owner_amendment: false,
            technical_validity: TechnicalValidity::Valid,
            authority_source,
            owner_envelope: Some(OwnerEnvelopeState {
                frozen_version_matches: true,
                action_within_envelope: true,
            }),
            operational_profile: Some(OperationalProfileInput::Balanced),
        }
    }

    fn profile_inputs_including_nontrivial_custom() -> Vec<Option<OperationalProfileInput>> {
        let mut custom_policy = compile_operational_profile(OperationalProfileInput::Balanced);
        custom_policy.workspace_path = WorkspacePathScope::ExactPath;
        custom_policy.parallelism = ParallelismScope::Sequential;
        custom_policy.model_danger_confirmation_sensitivity =
            ModelDangerConfirmationSensitivity::PlausibleModelIdentifiedDanger;
        vec![
            None,
            Some(OperationalProfileInput::LockedDown),
            Some(OperationalProfileInput::Balanced),
            Some(OperationalProfileInput::FullSend),
            Some(OperationalProfileInput::Custom(CustomOperationalProfile {
                policy: custom_policy,
            })),
        ]
    }

    fn profile_inputs_without_none() -> Vec<OperationalProfileInput> {
        profile_inputs_including_nontrivial_custom()
            .into_iter()
            .flatten()
            .collect()
    }

    #[test]
    fn fresh_root_guidance_is_nonmutating_and_never_silently_selects_balanced() {
        for ingress in [
            ProfileAdmissionIngress::Api,
            ProfileAdmissionIngress::NativeControl,
        ] {
            for work in [
                ProfileAdmissionWork::SavedOwnerInstruction,
                ProfileAdmissionWork::StandingOwnerWork,
                ProfileAdmissionWork::DerivedOrUnattendedWork,
            ] {
                assert_eq!(
                    admit_profile_work(ingress, ProfileConfigurationState::Unconfigured, work,),
                    ProfileAdmission::GuidedSelectionRequired {
                        configuration_state: ProfileConfigurationState::Unconfigured,
                        presentation_highlight: ProfileSelectionHighlight::Balanced,
                    },
                );
            }
        }
    }

    #[test]
    fn exact_current_owner_work_proceeds_on_an_unconfigured_root_for_every_ingress() {
        for ingress in [
            ProfileAdmissionIngress::Api,
            ProfileAdmissionIngress::NativeControl,
        ] {
            for work in [
                ProfileAdmissionWork::ExactCurrentOwnerInstruction,
                ProfileAdmissionWork::ExactCurrentOwnerPolicyAmendment,
            ] {
                assert_eq!(
                    admit_profile_work(ingress, ProfileConfigurationState::Unconfigured, work,),
                    ProfileAdmission::ProceedUnderBaseRules {
                        configured_policy: None,
                    },
                );
            }
        }
    }

    #[test]
    fn only_explicit_selection_configures_each_profile_deterministically() {
        for ingress in [
            ProfileAdmissionIngress::Api,
            ProfileAdmissionIngress::NativeControl,
        ] {
            for selection in profile_inputs_without_none() {
                let expected_policy = compile_operational_profile(selection.clone());
                let expected_kind = operational_profile_kind(&selection);
                let result = admit_profile_work(
                    ingress,
                    ProfileConfigurationState::Unconfigured,
                    ProfileAdmissionWork::ExplicitOwnerProfileSelection(selection),
                );
                assert_eq!(
                    result,
                    ProfileAdmission::Configured {
                        configuration_state: ProfileConfigurationState::Configured(
                            ConfiguredOperationalProfile {
                                selected_profile: expected_kind,
                                canonical_policy: expected_policy,
                            },
                        ),
                    },
                );
            }
        }
    }

    #[test]
    fn configured_work_reuses_the_canonical_policy_without_new_approval_or_confirmation() {
        let policy = compile_operational_profile(OperationalProfileInput::FullSend);
        let configuration = ProfileConfigurationState::Configured(ConfiguredOperationalProfile {
            selected_profile: OperationalProfileKind::FullSend,
            canonical_policy: policy,
        });
        for ingress in [
            ProfileAdmissionIngress::Api,
            ProfileAdmissionIngress::NativeControl,
        ] {
            for work in [
                ProfileAdmissionWork::SavedOwnerInstruction,
                ProfileAdmissionWork::StandingOwnerWork,
                ProfileAdmissionWork::DerivedOrUnattendedWork,
            ] {
                assert_eq!(
                    admit_profile_work(ingress, configuration.clone(), work),
                    ProfileAdmission::ProceedUnderBaseRules {
                        configured_policy: Some(policy),
                    },
                );
            }
        }
    }

    #[test]
    fn configured_profiles_cannot_constrain_exact_current_owner_work() {
        for ingress in [
            ProfileAdmissionIngress::Api,
            ProfileAdmissionIngress::NativeControl,
        ] {
            for selection in profile_inputs_without_none() {
                let configuration =
                    ProfileConfigurationState::Configured(ConfiguredOperationalProfile {
                        selected_profile: operational_profile_kind(&selection),
                        canonical_policy: compile_operational_profile(selection),
                    });
                for work in [
                    ProfileAdmissionWork::ExactCurrentOwnerInstruction,
                    ProfileAdmissionWork::ExactCurrentOwnerPolicyAmendment,
                ] {
                    assert_eq!(
                        admit_profile_work(ingress, configuration.clone(), work),
                        ProfileAdmission::ProceedUnderBaseRules {
                            configured_policy: None,
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn configuration_serialization_is_stable_and_rejects_unknown_fields() {
        let policy = compile_operational_profile(OperationalProfileInput::Balanced);
        let configured = ProfileConfigurationState::Configured(ConfiguredOperationalProfile {
            selected_profile: OperationalProfileKind::Custom,
            canonical_policy: policy,
        });
        let policy_json = serde_json::to_string(&policy).expect("serialize canonical policy");
        let expected = format!(
            r#"{{"configured":{{"selected_profile":"custom","canonical_policy":{policy_json}}}}}"#
        );
        assert_eq!(
            serde_json::to_string(&configured).expect("serialize configuration state"),
            expected,
        );
        assert_eq!(
            serde_json::to_string(&ProfileConfigurationState::Unconfigured)
                .expect("serialize unconfigured state"),
            r#""unconfigured""#,
        );
        let unknown_field = expected.replacen("}}}", ",\"unexpected\":true}}}", 1);
        assert!(serde_json::from_str::<ProfileConfigurationState>(&unknown_field).is_err());
    }

    #[test]
    fn preset_equivalent_custom_preserves_custom_identity() {
        let balanced_policy = compile_operational_profile(OperationalProfileInput::Balanced);
        let result = admit_profile_work(
            ProfileAdmissionIngress::Api,
            ProfileConfigurationState::Unconfigured,
            ProfileAdmissionWork::ExplicitOwnerProfileSelection(OperationalProfileInput::Custom(
                CustomOperationalProfile {
                    policy: balanced_policy,
                },
            )),
        );
        assert_eq!(
            result,
            ProfileAdmission::Configured {
                configuration_state: ProfileConfigurationState::Configured(
                    ConfiguredOperationalProfile {
                        selected_profile: OperationalProfileKind::Custom,
                        canonical_policy: balanced_policy,
                    },
                ),
            },
        );
    }

    #[test]
    fn custom_preset_equivalence_is_field_and_byte_equal() {
        for preset in [
            OperationalProfileInput::LockedDown,
            OperationalProfileInput::Balanced,
            OperationalProfileInput::FullSend,
        ] {
            let compiled = compile_operational_profile(preset.clone());
            let custom = compile_operational_profile(OperationalProfileInput::Custom(
                CustomOperationalProfile { policy: compiled },
            ));
            assert_eq!(custom, compiled);
            assert_eq!(
                serde_json::to_vec(&custom).expect("serialize custom policy"),
                serde_json::to_vec(&compiled).expect("serialize preset policy"),
            );
        }
    }

    #[test]
    fn profile_compilation_is_deterministic_for_all_inputs() {
        let custom = CustomOperationalProfile {
            policy: compile_operational_profile(OperationalProfileInput::Balanced),
        };
        for input in [
            OperationalProfileInput::LockedDown,
            OperationalProfileInput::Balanced,
            OperationalProfileInput::FullSend,
            OperationalProfileInput::Custom(custom),
        ] {
            assert_eq!(
                compile_operational_profile(input.clone()),
                compile_operational_profile(input),
            );
        }
    }

    #[test]
    fn canonical_serialization_is_snake_case_stable_and_rejects_unknown_fields() {
        let mut policy = compile_operational_profile(OperationalProfileInput::FullSend);
        policy.tool_identity = ToolIdentityScope::ConfiguredIdentityAndVersion;
        assert_eq!(
            serde_json::to_string(&policy).expect("serialize canonical policy"),
            r#"{"task_delegation":"owner_envelope","workspace_path":"owner_envelope","routine":"owner_envelope","tool_identity":"configured_identity_and_version","target":"owner_envelope","audience":"owner_envelope","technical_quota":"owner_envelope","time_expiry":"owner_envelope","recovery":"objective_retry_within_owner_envelope","parallelism":"owner_envelope","clarification_sensitivity":"unresolved_only","model_danger_confirmation_sensitivity":"credible_destructive_or_dangerous_consequence","recurring_work":"owner_envelope"}"#,
        );
        assert!(serde_json::from_str::<CanonicalOperationalPolicy>(
            r#"{"task_delegation":"exact_task","workspace_path":"exact_path","routine":"disabled","tool_identity":"fixed_identity_and_version","target":"exact_target","audience":"exact_audience","technical_quota":"constrained","time_expiry":"explicit_expiry","recovery":"manual_only","parallelism":"sequential","clarification_sensitivity":"any_ambiguity","model_danger_confirmation_sensitivity":"plausible_model_identified_danger","recurring_work":"disabled","unexpected":"value"}"#,
        )
        .is_err());
    }

    #[test]
    fn model_danger_sensitivity_is_operational_only_and_preserves_exact_owner_authority() {
        assert_eq!(
            compile_operational_profile(OperationalProfileInput::LockedDown)
                .model_danger_confirmation_sensitivity,
            ModelDangerConfirmationSensitivity::PlausibleModelIdentifiedDanger,
        );
        assert_eq!(
            compile_operational_profile(OperationalProfileInput::Balanced)
                .model_danger_confirmation_sensitivity,
            ModelDangerConfirmationSensitivity::CredibleModelIdentifiedDanger,
        );
        let full_send = compile_operational_profile(OperationalProfileInput::FullSend);
        assert_eq!(
            full_send.model_danger_confirmation_sensitivity,
            ModelDangerConfirmationSensitivity::CredibleDestructiveOrDangerousConsequence,
        );
        assert_eq!(
            full_send.recovery,
            RecoveryScope::ObjectiveRetryWithinOwnerEnvelope,
        );

        for profile in profile_inputs_including_nontrivial_custom() {
            let mut input = valid_input(AuthoritySource::CurrentExactOwnerInstruction);
            input.operational_profile = profile;
            input.owner_envelope = None;
            assert_eq!(
                evaluate_authority(input),
                AuthorityEvaluation::AuthorizedExactOwnerInstruction,
            );
        }
    }

    #[test]
    fn exact_current_owner_instruction_is_field_byte_and_profile_independent() {
        for profile in profile_inputs_including_nontrivial_custom() {
            let mut input = valid_input(AuthoritySource::CurrentExactOwnerInstruction);
            input.operational_profile = profile;
            input.owner_envelope = None;
            assert_eq!(
                evaluate_authority(input),
                AuthorityEvaluation::AuthorizedExactOwnerInstruction,
            );
        }
    }

    #[test]
    fn exact_owner_policy_amendment_is_field_byte_and_profile_independent() {
        for profile in profile_inputs_including_nontrivial_custom() {
            let mut input = valid_input(AuthoritySource::CurrentExactOwnerPolicyAmendment);
            input.operational_profile = profile;
            input.owner_envelope = None;
            assert_eq!(
                evaluate_authority(input),
                AuthorityEvaluation::AuthorizedExactOwnerPolicyAmendment,
            );
        }
    }

    #[test]
    fn precedence_stops_before_exact_owner_authority() {
        let mut input = valid_input(AuthoritySource::CurrentExactOwnerInstruction);
        input.stopped = true;
        input.revoked = true;
        input.superseded_by_owner_amendment = true;
        input.technical_validity = TechnicalValidity::ActionIdentityUnresolved;
        assert_eq!(
            evaluate_authority(input),
            AuthorityEvaluation::Paused(PauseReason::Stopped),
        );
    }

    #[test]
    fn revocation_supersession_and_technical_validity_follow_stop_order() {
        let mut input = valid_input(AuthoritySource::CurrentExactOwnerInstruction);
        input.revoked = true;
        assert_eq!(
            evaluate_authority(input.clone()),
            AuthorityEvaluation::Paused(PauseReason::Revoked),
        );
        input.revoked = false;
        input.superseded_by_owner_amendment = true;
        assert_eq!(
            evaluate_authority(input.clone()),
            AuthorityEvaluation::Paused(PauseReason::SupersededByOwnerAmendment),
        );
        input.superseded_by_owner_amendment = false;
        input.technical_validity = TechnicalValidity::OperandUnresolved;
        assert_eq!(
            evaluate_authority(input),
            AuthorityEvaluation::Paused(PauseReason::TechnicalValidity(
                TechnicalValidity::OperandUnresolved,
            )),
        );
    }

    #[test]
    fn unresolved_or_changed_actions_pause_without_category_denial() {
        for validity in [
            TechnicalValidity::ActionIdentityUnresolved,
            TechnicalValidity::OperandUnresolved,
            TechnicalValidity::CapabilityUnavailable,
        ] {
            let mut input = valid_input(AuthoritySource::CurrentExactOwnerInstruction);
            input.technical_validity = validity;
            assert_eq!(
                evaluate_authority(input),
                AuthorityEvaluation::Paused(PauseReason::TechnicalValidity(validity)),
            );
        }
    }

    #[test]
    fn saved_instruction_requires_its_frozen_versioned_envelope() {
        let mut input = valid_input(AuthoritySource::SavedOwnerInstruction);
        input.owner_envelope = Some(OwnerEnvelopeState {
            frozen_version_matches: false,
            action_within_envelope: true,
        });
        assert_eq!(
            evaluate_authority(input.clone()),
            AuthorityEvaluation::Paused(PauseReason::OwnerEnvelopeVersionMismatch),
        );
        input.owner_envelope = Some(OwnerEnvelopeState {
            frozen_version_matches: true,
            action_within_envelope: false,
        });
        assert_eq!(
            evaluate_authority(input),
            AuthorityEvaluation::Paused(PauseReason::ActionOutsideOwnerEnvelope),
        );
    }

    #[test]
    fn saved_instruction_is_profile_independent_inside_its_frozen_envelope() {
        for profile in profile_inputs_including_nontrivial_custom() {
            let mut input = valid_input(AuthoritySource::SavedOwnerInstruction);
            input.operational_profile = profile;
            assert_eq!(
                evaluate_authority(input),
                AuthorityEvaluation::AuthorizedSavedOwnerInstruction,
            );
        }
    }

    #[test]
    fn derived_work_requires_an_owner_envelope_and_explicit_profile() {
        let mut input = valid_input(AuthoritySource::DerivedOrUnattended);
        input.owner_envelope = None;
        assert_eq!(
            evaluate_authority(input.clone()),
            AuthorityEvaluation::Paused(PauseReason::OwnerEnvelopeRequired),
        );
        input.owner_envelope = Some(OwnerEnvelopeState {
            frozen_version_matches: true,
            action_within_envelope: true,
        });
        input.operational_profile = None;
        assert_eq!(
            evaluate_authority(input),
            AuthorityEvaluation::Paused(PauseReason::ProfileSelectionRequired),
        );
    }

    #[test]
    fn nonhuman_content_never_creates_owner_authority() {
        let input = valid_input(AuthoritySource::NonHumanOnly);
        assert_eq!(
            evaluate_authority(input),
            AuthorityEvaluation::NoOwnerAuthority
        );
    }

    #[test]
    fn objective_technical_validity_exhaustively_applies_closed_truth_and_precedence() {
        let authority = verified_authority(
            TestIngress::Local,
            TestOwnerAction::Communication,
            "technical-truth-table",
        );
        let leaf = exact_leaf(TestOwnerAction::Communication, authority);

        for failures in 0_u8..=0b111_1111 {
            let facts = ObjectiveTechnicalValidityFacts {
                action_identity: if failures & (1 << 0) == 0 {
                    ActionIdentityResolution::Resolved
                } else {
                    ActionIdentityResolution::Unresolved
                },
                operands: if failures & (1 << 1) == 0 {
                    OperandResolution::Resolved
                } else {
                    OperandResolution::Unresolved
                },
                capability: if failures & (1 << 2) == 0 {
                    CapabilityAvailability::Available
                } else {
                    CapabilityAvailability::Unavailable
                },
                runtime_precondition: if failures & (1 << 3) == 0 {
                    RuntimePreconditionState::Met
                } else {
                    RuntimePreconditionState::Unmet
                },
                transaction_and_fencing: if failures & (1 << 4) == 0 {
                    TransactionFencingState::Valid
                } else {
                    TransactionFencingState::Invalid
                },
                reconciliation: if failures & (1 << 5) == 0 {
                    ReconciliationAvailability::Available
                } else {
                    ReconciliationAvailability::Unavailable
                },
                resources: if failures & (1 << 6) == 0 {
                    ResourceAvailability::Available
                } else {
                    ResourceAvailability::Unavailable
                },
            };
            let expected = if failures & (1 << 0) != 0 {
                TechnicalValidity::ActionIdentityUnresolved
            } else if failures & (1 << 1) != 0 {
                TechnicalValidity::OperandUnresolved
            } else if failures & (1 << 2) != 0 {
                TechnicalValidity::CapabilityUnavailable
            } else if failures & (1 << 3) != 0 {
                TechnicalValidity::RuntimePreconditionUnmet
            } else if failures & (1 << 4) != 0 {
                TechnicalValidity::TransactionOrFencingInvalid
            } else if failures & (1 << 5) != 0 {
                TechnicalValidity::ReconciliationUnavailable
            } else if failures & (1 << 6) != 0 {
                TechnicalValidity::ResourceUnavailable
            } else {
                TechnicalValidity::Valid
            };
            assert_eq!(
                evaluate_objective_technical_validity(&leaf, facts).validity(),
                expected,
                "wrong result for failure mask {failures:07b}"
            );
        }
    }

    #[test]
    fn production_technical_validity_proof_is_bound_to_the_exact_leaf() {
        let authority = verified_authority(
            TestIngress::Local,
            TestOwnerAction::Communication,
            "production-proof-binding",
        );
        let original_leaf = exact_leaf(TestOwnerAction::Communication, authority.clone());
        let mutated_leaf = exact_leaf(TestOwnerAction::ProjectMutation, authority.clone());
        let proof = evaluate_objective_technical_validity(
            &original_leaf,
            valid_objective_technical_facts(),
        );

        assert!(proof.matches(&original_leaf));
        assert!(!proof.matches(&mutated_leaf));
        assert_eq!(
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: &authority,
                canonical_leaf: &mutated_leaf,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &proof,
            }),
            ExactOwnerAuthorityOutcome::NoOwnerAuthority,
        );
    }

    #[test]
    fn production_technical_validity_input_surface_is_mechanical_and_closed() {
        let source = include_str!("execass_policy.rs");
        let facts_surface = source
            .split_once("pub struct ObjectiveTechnicalValidityFacts")
            .expect("facts surface")
            .1
            .split_once("/// Whether an existing owner envelope")
            .expect("facts surface end")
            .0;
        for forbidden in [
            "text",
            "purpose",
            "category",
            "morality",
            "commerce",
            "finance",
            "model",
            "score",
            "String",
            "&str",
            "bool",
            "canonical_leaf_digest",
            "pub validity",
        ] {
            assert!(
                !facts_surface.contains(forbidden),
                "objective technical facts expose forbidden caller input {forbidden}"
            );
        }

        let issuer_signature = source
            .split_once("pub fn evaluate_objective_technical_validity(")
            .expect("production evaluator")
            .1
            .split_once(") -> ObjectiveTechnicalValidityProof")
            .expect("production evaluator return")
            .0;
        assert!(issuer_signature.contains("canonical_leaf: &CanonicalLeafAction"));
        assert!(issuer_signature.contains("facts: ObjectiveTechnicalValidityFacts"));
        assert!(!issuer_signature.contains("TechnicalValidity,"));
        assert!(!issuer_signature.contains("digest"));
    }

    #[test]
    fn exact_owner_authority_real_manifest_matrix_is_profile_and_ingress_independent() {
        let actions = [
            TestOwnerAction::Communication,
            TestOwnerAction::PermissionChange,
            TestOwnerAction::ProjectMutation,
            TestOwnerAction::PolicyAmendment,
            TestOwnerAction::NarrowDeletion,
            TestOwnerAction::SecretDelivery,
            TestOwnerAction::ExistingExternalToolUse,
        ];
        let ingresses = [TestIngress::Local, TestIngress::Remote];
        let profiles = profile_inputs_including_nontrivial_custom();
        let mut authorized_cells = 0;

        for (profile_index, profile) in profiles.into_iter().enumerate() {
            let configuration =
                profile
                    .clone()
                    .map_or(ProfileConfigurationState::Unconfigured, |selected| {
                        ProfileConfigurationState::Configured(ConfiguredOperationalProfile {
                            selected_profile: operational_profile_kind(&selected),
                            canonical_policy: compile_operational_profile(selected),
                        })
                    });
            for ingress in ingresses {
                for (action_index, action) in actions.into_iter().enumerate() {
                    let profile_work = if matches!(action, TestOwnerAction::PolicyAmendment) {
                        ProfileAdmissionWork::ExactCurrentOwnerPolicyAmendment
                    } else {
                        ProfileAdmissionWork::ExactCurrentOwnerInstruction
                    };
                    assert!(matches!(
                        admit_profile_work(
                            ProfileAdmissionIngress::Api,
                            configuration.clone(),
                            profile_work,
                        ),
                        ProfileAdmission::ProceedUnderBaseRules {
                            configured_policy: None
                        }
                    ));
                    let seed = format!("{profile_index}-{action_index}-{ingress:?}");
                    let authority = verified_authority(ingress, action, &seed);
                    let leaf = exact_leaf(action, authority.clone());
                    let technical_validity = issue_test_objective_technical_validity_proof(
                        &leaf,
                        TechnicalValidity::Valid,
                    );
                    let ExactOwnerAuthorityOutcome::Authorized(token) =
                        authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                            verified_owner_authority: &authority,
                            canonical_leaf: &leaf,
                            stopped: false,
                            revoked: false,
                            superseded_by_owner_amendment: false,
                            technical_validity: &technical_validity,
                        })
                    else {
                        panic!("exact verified owner action was not authorized");
                    };
                    assert!(token.matches(&authority, &leaf));
                    assert_eq!(
                        token.kind(),
                        if matches!(action, TestOwnerAction::PolicyAmendment) {
                            ExactOwnerAuthorityKind::OperationalPolicyAmendment
                        } else {
                            ExactOwnerAuthorityKind::Instruction
                        }
                    );
                    authorized_cells += 1;
                }
            }
        }
        assert_eq!(authorized_cells, 70);
    }

    #[test]
    fn exact_owner_token_cannot_be_reused_for_another_leaf_or_authority() {
        let authority_a = verified_authority(
            TestIngress::Local,
            TestOwnerAction::Communication,
            "binding-a",
        );
        let leaf_a = exact_leaf(TestOwnerAction::Communication, authority_a.clone());
        let technical_validity_a =
            issue_test_objective_technical_validity_proof(&leaf_a, TechnicalValidity::Valid);
        let ExactOwnerAuthorityOutcome::Authorized(token_a) =
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: &authority_a,
                canonical_leaf: &leaf_a,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &technical_validity_a,
            })
        else {
            panic!("expected exact authorization");
        };

        let changed_leaf = exact_leaf(TestOwnerAction::ProjectMutation, authority_a.clone());
        assert!(!token_a.matches(&authority_a, &changed_leaf));
        assert_eq!(
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: &authority_a,
                canonical_leaf: &changed_leaf,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &technical_validity_a,
            }),
            ExactOwnerAuthorityOutcome::NoOwnerAuthority,
        );

        let authority_b = verified_authority(
            TestIngress::Local,
            TestOwnerAction::Communication,
            "binding-b",
        );
        let leaf_b = exact_leaf(TestOwnerAction::Communication, authority_b.clone());
        let technical_validity_b =
            issue_test_objective_technical_validity_proof(&leaf_b, TechnicalValidity::Valid);
        assert!(!token_a.matches(&authority_b, &leaf_b));
        assert_eq!(
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: &authority_a,
                canonical_leaf: &leaf_b,
                stopped: false,
                revoked: false,
                superseded_by_owner_amendment: false,
                technical_validity: &technical_validity_b,
            }),
            ExactOwnerAuthorityOutcome::NoOwnerAuthority,
        );
    }

    #[test]
    fn exact_owner_authority_obeys_only_stop_revoke_supersession_and_objective_validity() {
        let authority = verified_authority(
            TestIngress::Remote,
            TestOwnerAction::ExistingExternalToolUse,
            "precedence",
        );
        let leaf = exact_leaf(TestOwnerAction::ExistingExternalToolUse, authority.clone());
        let outcome = |stopped, revoked, superseded, technical_validity| {
            let technical_validity =
                issue_test_objective_technical_validity_proof(&leaf, technical_validity);
            authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                verified_owner_authority: &authority,
                canonical_leaf: &leaf,
                stopped,
                revoked,
                superseded_by_owner_amendment: superseded,
                technical_validity: &technical_validity,
            })
        };
        assert_eq!(
            outcome(true, true, true, TechnicalValidity::OperandUnresolved),
            ExactOwnerAuthorityOutcome::Paused(PauseReason::Stopped),
        );
        assert_eq!(
            outcome(false, true, true, TechnicalValidity::OperandUnresolved),
            ExactOwnerAuthorityOutcome::Paused(PauseReason::Revoked),
        );
        assert_eq!(
            outcome(false, false, true, TechnicalValidity::OperandUnresolved),
            ExactOwnerAuthorityOutcome::Paused(PauseReason::SupersededByOwnerAmendment),
        );
        for validity in [
            TechnicalValidity::ActionIdentityUnresolved,
            TechnicalValidity::CapabilityUnavailable,
            TechnicalValidity::OperandUnresolved,
            TechnicalValidity::RuntimePreconditionUnmet,
            TechnicalValidity::TransactionOrFencingInvalid,
            TechnicalValidity::ReconciliationUnavailable,
            TechnicalValidity::ResourceUnavailable,
        ] {
            assert_eq!(
                outcome(false, false, false, validity),
                ExactOwnerAuthorityOutcome::Paused(PauseReason::TechnicalValidity(validity)),
            );
        }
    }

    #[test]
    fn unsupported_human_authority_kinds_never_enter_exact_owner_path() {
        for (index, authority_kind) in [
            "decision_resolution",
            "action_specific_owner_amendment",
            "runtime_safety_state",
        ]
        .into_iter()
        .enumerate()
        {
            let observation =
                ServerIngressObservation::LocalInteractive(Box::new(LocalInteractiveEvidence {
                    authenticated_client_id: "desktop-owner".to_string(),
                    authenticated_ingress: "native-control".to_string(),
                    channel_assurance: "interactive-local".to_string(),
                    request_correlation_id: format!("unsupported-{index}"),
                    source_message_id: None,
                    interactive_owner_verified: true,
                }));
            let actor = derive_base_actor_assurance(&observation, &CallerActorClaims::default());
            let authority = bind_verified_owner_authority(
                &actor,
                OwnerAuthoritySourceInput {
                    normalized_intent: "exact owner intent".to_string(),
                    instruction_revision: "instruction-1".to_string(),
                    instruction_bytes: b"exact owner instruction".to_vec(),
                    owner_envelope_revision: "envelope-1".to_string(),
                    owner_envelope_json: r#"{"exact":true}"#.to_string(),
                    authority_kind: authority_kind.to_string(),
                    normalized_scope_json: r#"{"instance":"single-owner"}"#.to_string(),
                    policy_revision: 1,
                    bound_decision_id: None,
                    bound_decision_revision: None,
                    bound_manifest_bytes: None,
                    challenge_nonce_bytes: None,
                    created_at: 1_800_000_000_000,
                    expires_at: None,
                },
            )
            .unwrap();
            let leaf = exact_leaf(TestOwnerAction::Communication, authority.clone());
            let technical_validity =
                issue_test_objective_technical_validity_proof(&leaf, TechnicalValidity::Valid);
            assert_eq!(
                authorize_exact_owner_leaf(ExactOwnerAuthorityInput {
                    verified_owner_authority: &authority,
                    canonical_leaf: &leaf,
                    stopped: false,
                    revoked: false,
                    superseded_by_owner_amendment: false,
                    technical_validity: &technical_validity,
                }),
                ExactOwnerAuthorityOutcome::NoOwnerAuthority,
            );
        }
    }

    #[test]
    fn canonical_exact_owner_path_is_structurally_isolated_from_legacy_veto_engines() {
        let canonical_sources = [
            include_str!("execass_policy.rs"),
            include_str!("../../carsinos-storage/src/execass/foundation.rs"),
            include_str!("../../carsinos-gateway/src/execass_actor_gate.rs"),
        ]
        .map(|source| source.split("\n#[cfg(test)]").next().unwrap_or(source));
        for forbidden in [
            "ApprovalRecord",
            "NewApproval",
            "/api/v1/approvals",
            "requires_approval",
            "ToolApprovalPolicy",
            "APPROVAL_REQUIRED",
            "APPROVAL_DENIED",
            "guard_project_decision_commitment",
            "budget_month_usd",
            "daily_budget_usd",
            "payee",
            "currency",
        ] {
            assert!(
                canonical_sources
                    .iter()
                    .all(|source| !source.contains(forbidden)),
                "canonical exact-owner path references forbidden legacy veto token {forbidden}"
            );
        }
    }

    #[test]
    fn technical_quota_snapshot_is_sorted_closed_and_restart_stable() {
        let connector = canonical_connector_call_unit("mail", "v2").unwrap();
        let resource = canonical_resource_unit("gpu", "local-0").unwrap();
        let entries = vec![
            TechnicalQuotaEntryInput {
                kind: TechnicalResourceKind::ResourceUnits,
                unit: resource.clone(),
                limit: 2,
            },
            TechnicalQuotaEntryInput {
                kind: TechnicalResourceKind::Tokens,
                unit: "token".into(),
                limit: 4_000,
            },
            TechnicalQuotaEntryInput {
                kind: TechnicalResourceKind::ConnectorCalls,
                unit: connector.clone(),
                limit: 3,
            },
            TechnicalQuotaEntryInput {
                kind: TechnicalResourceKind::TimeMs,
                unit: "ms".into(),
                limit: 30_000,
            },
        ];
        let first = compile_technical_quota_snapshot(
            "delegation-1",
            7,
            "sha256:authority",
            "delegation",
            entries.clone(),
        )
        .unwrap();
        let second = compile_technical_quota_snapshot(
            "delegation-1",
            7,
            "sha256:authority",
            "delegation",
            entries.into_iter().rev().collect(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.entries.len(), 4);
        assert!(first.quota_snapshot_id.starts_with("sha256:"));
        assert!(first.canonical_entries_json.contains(&connector));
        assert!(first.canonical_entries_json.contains(&resource));
        for forbidden in ["money", "currency", "payee", "purchase", "financial"] {
            assert!(!first.canonical_entries_json.contains(forbidden));
        }
    }

    #[test]
    fn technical_quota_snapshot_rejects_aliases_duplicates_and_invalid_limits() {
        let base = |kind, unit: &str, limit| TechnicalQuotaEntryInput {
            kind,
            unit: unit.to_string(),
            limit,
        };
        for entry in [
            base(TechnicalResourceKind::Tokens, "tokens", 1),
            base(TechnicalResourceKind::TimeMs, "milliseconds", 1),
            base(TechnicalResourceKind::ConnectorCalls, "connector:mail", 1),
            base(TechnicalResourceKind::ResourceUnits, "resource:gpu", 1),
            base(TechnicalResourceKind::Tokens, "token", -1),
        ] {
            assert!(compile_technical_quota_snapshot(
                "delegation-1",
                1,
                "sha256:authority",
                "delegation",
                vec![entry],
            )
            .is_err());
        }
        assert!(compile_technical_quota_snapshot(
            "delegation-1",
            1,
            "sha256:authority",
            "delegation",
            vec![
                base(TechnicalResourceKind::Tokens, "token", 1),
                base(TechnicalResourceKind::Tokens, "token", 2),
            ],
        )
        .is_err());
        let snapshot = compile_technical_quota_snapshot(
            "delegation-1",
            1,
            "sha256:authority",
            "delegation",
            vec![TechnicalQuotaEntryInput {
                kind: TechnicalResourceKind::Tokens,
                unit: "token".into(),
                limit: 1,
            }],
        )
        .unwrap();
        assert!(compile_technical_resource_requirements(
            &snapshot,
            "effect-1",
            "action-1",
            "sha256:manifest",
            vec![TechnicalResourceRequirementInput {
                kind: TechnicalResourceKind::Tokens,
                unit: "token".into(),
                amount: 2,
            }],
        )
        .is_err());
        let requirements = compile_technical_resource_requirements(
            &snapshot,
            "effect-1",
            "action-1",
            "sha256:manifest",
            vec![TechnicalResourceRequirementInput {
                kind: TechnicalResourceKind::Tokens,
                unit: "token".into(),
                amount: 1,
            }],
        )
        .unwrap();
        assert_eq!(requirements.requirements[0].amount, 1);
    }

    #[test]
    fn technical_unit_registry_grammar_rejects_case_unicode_and_version_aliases() {
        let connector = canonical_connector_call_unit("mail", "v2").unwrap();
        let resource = canonical_resource_unit("gpu", "local-0").unwrap();
        assert!(connector.starts_with("connector:"));
        assert!(resource.starts_with("resource:"));

        for (identity, version) in [
            ("Mail", "v2"),
            ("MAIL", "v2"),
            ("mail", "V2"),
            ("mail", "2"),
            ("mail", "v02"),
            ("mail", "v2.0"),
            ("mail", "latest"),
        ] {
            assert!(
                canonical_connector_call_unit(identity, version).is_err(),
                "connector alias unexpectedly compiled: {identity}/{version}"
            );
        }

        let composed = "caf\u{e9}";
        let decomposed = "cafe\u{301}";
        assert!(canonical_connector_call_unit(composed, "v2").is_err());
        assert!(canonical_connector_call_unit(decomposed, "v2").is_err());
        assert!(canonical_resource_unit(composed, "local-0").is_err());
        assert!(canonical_resource_unit(decomposed, "local-0").is_err());

        for scope_alias in ["Local-0", "LOCAL-0", "local_0", "local--0", "local-00-"] {
            assert!(
                canonical_resource_unit("gpu", scope_alias).is_err(),
                "resource scope alias unexpectedly compiled: {scope_alias}"
            );
        }
        assert!(canonical_resource_unit("GPU", "local-0").is_err());
    }
}
