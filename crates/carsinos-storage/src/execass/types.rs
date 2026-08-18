#![cfg_attr(not(test), allow(dead_code))]

use super::redaction::{SafeJson, SafeText};
use carsinos_core::execass_manifest::MechanicalResolutionPause;
use carsinos_core::execass_policy::{
    CanonicalTechnicalQuotaSnapshot, CanonicalTechnicalResourceRequirementSet,
};
use carsinos_protocol::execass_recorder::OpaqueOperandEnvelopeV1;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[allow(dead_code)]
            pub(crate) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }

        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_str()? {
                    $($value => Ok(Self::$variant),)+
                    value => Err(FromSqlError::Other(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid {} value from ExecAss storage: {value}", stringify!($name)),
                    )))),
                }
            }
        }
    };
}

text_enum!(ActorType {
    HumanLocal => "human_local",
    HumanRemote => "human_remote",
    Runtime => "runtime",
    Worker => "worker",
    Connector => "connector",
    Model => "model",
});

text_enum!(AuthorityKind {
    OriginalRequest => "original_request",
    DecisionResolution => "decision_resolution",
    ActionSpecificOwnerAmendment => "action_specific_owner_amendment",
    PolicySnapshot => "policy_snapshot",
    RuntimeSettingsSnapshot => "runtime_settings_snapshot",
    RunControlAttestation => "run_control_attestation",
    RuntimeSafetyState => "runtime_safety_state",
});

text_enum!(DecisionKind {
    Clarification => "clarification",
    DangerousActionConfirmation => "dangerous_action_confirmation",
    OwnerConfiguredCheckpoint => "owner_configured_checkpoint",
    RecoveryChoice => "recovery_choice",
    DuplicateRiskRetry => "duplicate_risk_retry",
    Stop => "stop",
    PolicyChange => "policy_change",
});

text_enum!(DecisionResult {
    ConfirmAndContinue => "confirm_and_continue",
    Revise => "revise",
    Decline => "decline",
    Stop => "stop",
});

text_enum!(DecisionStatus {
    Pending => "pending",
    Resolved => "resolved",
    Superseded => "superseded",
    Expired => "expired",
});

text_enum!(LogicalEffectActionKind {
    ReadOnlyLocalInspectionAndBoundedReversibleLocalWork => "read_only_local_inspection_and_bounded_reversible_local_work",
    PrivateDraftCreationWithoutTransmission => "private_draft_creation_without_transmission",
    PublicOrExternallyConsequentialCommunication => "public_or_externally_consequential_communication",
    IrreversibleOrDestructiveAction => "irreversible_or_destructive_action",
    CredentialPermissionPrivilegeOrTrustPolicyChange => "credential_permission_privilege_or_trust_policy_change",
    ProjectDefiningScopeOwnershipOrLaunchDecision => "project_defining_scope_ownership_or_launch_decision",
    SecretUseThroughAuthorizedConnector => "secret_use_through_authorized_connector",
    UnknownCompositeAliasedPluginShellOrChangedVersionAction => "unknown_composite_aliased_plugin_shell_or_changed_version_action",
});

text_enum!(LogicalEffectState {
    Planned => "planned",
    Claimed => "claimed",
    Invoking => "invoking",
    Succeeded => "succeeded",
    Failed => "failed",
    OutcomeUnknown => "outcome_unknown",
    ReconciledAbsent => "reconciled_absent",
    ReconciledPresent => "reconciled_present",
});

text_enum!(ProviderAttemptStatus {
    Prepared => "prepared",
    Invoking => "invoking",
    Succeeded => "succeeded",
    Failed => "failed",
    OutcomeUnknown => "outcome_unknown",
    ReconciledAbsent => "reconciled_absent",
    ReconciledPresent => "reconciled_present",
});

text_enum!(ProviderFailureClass {
    Transient => "transient",
    RateLimited => "rate_limited",
    Authentication => "authentication",
    Permanent => "permanent",
    Unknown => "unknown",
});

text_enum!(DeclaredRecoverySafeBoundary {
    IndependentAbsence => "independent_absence",
});

text_enum!(TechnicalResourceKind {
    Tokens => "tokens",
    TimeMs => "time_ms",
    ConnectorCalls => "connector_calls",
    ResourceUnits => "resource_units",
});

text_enum!(DelegationPhase {
    Accepted => "accepted",
    Planning => "planning",
    InMotion => "in_motion",
    WaitingForUser => "waiting_for_user",
    WaitingExternal => "waiting_external",
    Recovering => "recovering",
    Completed => "completed",
    PartiallyCompleted => "partially_completed",
    Failed => "failed",
});

impl DelegationPhase {
    pub(crate) const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::PartiallyCompleted | Self::Failed
        )
    }
}

text_enum!(RunControlState {
    Running => "running",
    StopRequested => "stop_requested",
    Stopped => "stopped",
});

text_enum!(VerifierType {
    Artifact => "artifact",
    AuthoritativeState => "authoritative_state",
    ProviderState => "provider_state",
    Delivery => "delivery",
    ProcessExit => "process_exit",
    DatabasePredicate => "database_predicate",
    HumanBoundSupersession => "human_bound_supersession",
});

text_enum!(ContinuationCausationKind {
    Intake => "intake",
    Plan => "plan",
    Amendment => "amendment",
    Decision => "decision",
    ActionResult => "action_result",
    Recovery => "recovery",
    Resume => "resume",
    RoutineOccurrence => "routine_occurrence",
});

text_enum!(ContinuationStatus {
    Runnable => "runnable",
    Executing => "executing",
    Waiting => "waiting",
    Uncertain => "uncertain",
    Terminal => "terminal",
    Superseded => "superseded",
});

text_enum!(RoutineOverlapPolicy {
    Earlier => "earlier",
    Later => "later",
});

text_enum!(RoutineCatchUpPolicy {
    Skip => "skip",
    LatestOnly => "latest_only",
    Replay => "replay",
});

text_enum!(RoutineOccurrenceStatus {
    Planned => "planned",
    AdmissionPlanned => "admission_planned",
    Skipped => "skipped",
    Settled => "settled",
});

text_enum!(RoutineTimeResolution {
    Single => "single",
    Earlier => "earlier",
    Later => "later",
    GapAdvanced => "gap_advanced",
});

text_enum!(ActionBranchKind {
    Ordinary => "ordinary",
    Recovery => "recovery",
});

text_enum!(AttentionKind {
    Confirmation => "confirmation",
    Clarification => "clarification",
    Reply => "reply",
    RecoveryChoice => "recovery_choice",
    RuntimePaused => "runtime_paused",
});

text_enum!(AttentionStatus {
    Actionable => "actionable",
    Resolved => "resolved",
    Superseded => "superseded",
});

text_enum!(ExternalWaitKind {
    ExternalParty => "external_party",
    System => "system",
    Time => "time",
});

text_enum!(ExternalWaitStatus {
    Waiting => "waiting",
    Resolved => "resolved",
    Superseded => "superseded",
});

/// The only pre-actionable lifecycle selections. Once a runnable branch
/// exists, `select_lifecycle_phase` deliberately ignores this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreActionablePhase {
    Accepted,
    Planning,
}

impl PreActionablePhase {
    pub const fn phase(self) -> DelegationPhase {
        match self {
            Self::Accepted => DelegationPhase::Accepted,
            Self::Planning => DelegationPhase::Planning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionAssessmentKind {
    Completed,
    PartiallyCompleted,
    Failed,
}

impl CompletionAssessmentKind {
    pub const fn phase(self) -> DelegationPhase {
        match self {
            Self::Completed => DelegationPhase::Completed,
            Self::PartiallyCompleted => DelegationPhase::PartiallyCompleted,
            Self::Failed => DelegationPhase::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleSelectorInput {
    pub completion_assessment: Option<CompletionAssessmentKind>,
    pub pre_actionable_phase: Option<PreActionablePhase>,
    pub ordinary_runnable_or_executing: bool,
    pub recovery_runnable_or_executing: bool,
    pub actionable_attention: bool,
    pub external_wait: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleSelectionError {
    NoHonestPath,
}

impl std::fmt::Display for LifecycleSelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("no autonomous, human, or external path remains; an honest completion assessment is required")
    }
}

impl std::error::Error for LifecycleSelectionError {}

text_enum!(OutboxEventName {
    DelegationTransitioned => "execass.v1.delegation.transitioned",
    DecisionRecorded => "execass.v1.decision.recorded",
    ContinuationClaimedOrResultRecorded => "execass.v1.continuation.claimed_or_result_recorded",
    RecoveryUpdated => "execass.v1.recovery.updated",
    CompletionAssessed => "execass.v1.completion.assessed",
    SummaryChanged => "execass.v1.summary.changed",
    PolicyChanged => "execass.v1.policy.changed",
    RuntimeHostChanged => "execass.v1.runtime_host.changed",
    ReceiptIntegrityFailed => "execass.v1.receipt.integrity_failed",
    NotificationScheduled => "execass.v1.notification.scheduled",
    GlobalStopChanged => "execass.v1.global_stop.changed",
});

text_enum!(ReceiptKind {
    Intake => "intake",
    Plan => "plan",
    Amendment => "amendment",
    Decision => "decision",
    Continuation => "continuation",
    Action => "action",
    Effect => "effect",
    Verifier => "verifier",
    Recovery => "recovery",
    Resume => "resume",
    Budget => "budget",
    Completion => "completion",
    TerminalCorrection => "terminal_correction",
    AuthorityLink => "authority_link",
    KeyRotation => "key_rotation",
    GlobalStop => "global_stop",
    RunControl => "run_control",
    Policy => "policy",
    RuntimeSettings => "runtime_settings",
    RuntimeRecovery => "runtime_recovery",
});

text_enum!(ReceiptSubjectKind {
    Delegation => "delegation",
    Plan => "plan",
    PlanAmendment => "plan_amendment",
    Decision => "decision",
    Continuation => "continuation",
    ActionBranch => "action_branch",
    VerifierResult => "verifier_result",
    CompletionAssessment => "completion_assessment",
    TerminalCorrection => "terminal_correction",
    AuthorityLink => "authority_link",
    RecoveryEvaluation => "recovery_evaluation",
    OutboxEvent => "outbox_event",
    GlobalRuntimeControl => "global_runtime_control",
    PolicyRevision => "policy_revision",
    RuntimeSettingsRevision => "runtime_settings_revision",
    RuntimeHostGeneration => "runtime_host_generation",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDesiredMode {
    AppBound,
    Background,
}

impl RuntimeDesiredMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppBound => "app_bound",
            Self::Background => "background",
        }
    }
}

impl FromSql for RuntimeDesiredMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "app_bound" => Ok(Self::AppBound),
            "background" => Ok(Self::Background),
            value => Err(FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid ExecAss runtime desired mode: {value}"),
            )))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActualState {
    Stopped,
    Starting,
    RunningAppBound,
    Handoff,
    RunningBackground,
    Draining,
    Faulted,
}

/// An intentional host lifecycle operation. The storage boundary derives the
/// next actual state from this operation, the current state, and the exact
/// persisted desired mode; callers cannot select an arbitrary target state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeHostTransition {
    ReachDesiredMode,
    BeginHandoff,
    BeginDrain,
    RecordFault,
    CompleteStop,
}

/// Stable explanation for an accepted runtime-host transition. This is
/// operational reasoning only; it neither grants owner authority nor changes
/// the owner-configured desired mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHostTransitionReason {
    DesiredModeReached,
    DesiredModeRequiresHandoff,
    OrderlyShutdownRequested,
    HostFaultRecorded,
    OrderlyShutdownCompleted,
}

impl RuntimeHostTransitionReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DesiredModeReached => "desired_mode_reached",
            Self::DesiredModeRequiresHandoff => "desired_mode_requires_handoff",
            Self::OrderlyShutdownRequested => "orderly_shutdown_requested",
            Self::HostFaultRecorded => "host_fault_recorded",
            Self::OrderlyShutdownCompleted => "orderly_shutdown_completed",
        }
    }
}

impl RuntimeActualState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::RunningAppBound => "running_app_bound",
            Self::Handoff => "handoff",
            Self::RunningBackground => "running_background",
            Self::Draining => "draining",
            Self::Faulted => "faulted",
        }
    }
}

impl FromSql for RuntimeActualState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "stopped" => Ok(Self::Stopped),
            "starting" => Ok(Self::Starting),
            "running_app_bound" => Ok(Self::RunningAppBound),
            "handoff" => Ok(Self::Handoff),
            "running_background" => Ok(Self::RunningBackground),
            "draining" => Ok(Self::Draining),
            "faulted" => Ok(Self::Faulted),
            value => Err(FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid ExecAss runtime actual state: {value}"),
            )))),
        }
    }
}

text_enum!(GlobalStopDrainState {
    Running => "running",
    Draining => "draining",
    Drained => "drained",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedExternalEffectReference {
    pub logical_effect_id: String,
    pub delegation_id: String,
    pub continuation_id: String,
    pub state: LogicalEffectState,
    pub latest_attempt_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalStopStatus {
    pub engaged: bool,
    pub global_stop_epoch: i64,
    pub drain_state: GlobalStopDrainState,
    pub current_policy_revision: i64,
    pub unresolved_external_effects: Vec<UnresolvedExternalEffectReference>,
    pub unresolved_external_effects_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngageGlobalStopCommand {
    pub expected_global_stop_epoch: i64,
    pub trusted_now: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeGlobalStopCommand {
    pub expected_global_stop_epoch: i64,
    pub expected_policy_revision: i64,
    pub disclosed_unresolved_external_effects_digest: String,
    pub attestation: carsinos_protocol::execass::RunControlAttestation,
    pub trusted_now: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalReceiptContext {
    pub global_stop: GlobalStopStatus,
    pub carrier_state_revision: i64,
    pub global_receipt_count: i64,
    pub global_receipt_head_digest: Option<String>,
    pub carrier_receipt_count: i64,
    pub carrier_receipt_head_digest: Option<String>,
    pub state_root_generation: i64,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
    /// Durable anchor state only; this does not expose or probe receipt keys.
    pub receipt_anchor_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalStopMutationOutcome {
    Engaged(GlobalStopStatus),
    Resumed(GlobalStopStatus),
    Replayed(GlobalStopStatus),
    AlreadyEngaged(GlobalStopStatus),
    Stale(GlobalStopStatus),
    Conflict,
}

text_enum!(DelegationStopDrainState {
    Running => "running",
    Draining => "draining",
    ReadyToStop => "ready_to_stop",
    Stopped => "stopped",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationRunControlRuntimeContext {
    pub state_root_generation: i64,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationRunControlStatus {
    pub delegation_id: String,
    pub phase: DelegationPhase,
    pub run_control: RunControlState,
    pub state_revision: i64,
    pub current_plan_revision: Option<i64>,
    pub stop_epoch: i64,
    pub policy_revision: i64,
    pub drain_state: DelegationStopDrainState,
    pub executing_branch_count: i64,
    pub unresolved_external_effects: Vec<UnresolvedExternalEffectReference>,
    pub unresolved_external_effects_digest: String,
    pub global_receipt_count: i64,
    pub global_receipt_head_digest: Option<String>,
    pub delegation_receipt_count: i64,
    pub delegation_receipt_head_digest: Option<String>,
    pub receipt_anchor_status: String,
    pub runtime: Option<DelegationRunControlRuntimeContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDelegationStopCommand {
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub expected_stop_epoch: i64,
    pub expected_plan_revision: Option<i64>,
    pub expected_policy_revision: i64,
    pub disclosed_unresolved_external_effects_digest: String,
    pub attestation: carsinos_protocol::execass::RunControlAttestation,
    pub trusted_now: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteDelegationStopDrainCommand {
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub expected_stop_epoch: i64,
    pub trusted_now: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeDelegationCommand {
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub expected_plan_revision: Option<i64>,
    pub expected_stop_epoch: i64,
    pub expected_policy_revision: i64,
    pub disclosed_unresolved_external_effects_digest: String,
    pub attestation: carsinos_protocol::execass::RunControlAttestation,
    pub trusted_now: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationRunControlMutationOutcome {
    StopRequested(DelegationRunControlStatus),
    Drained(DelegationRunControlStatus),
    Resumed(DelegationRunControlStatus),
    Replayed(DelegationRunControlStatus),
    AlreadyStopped(DelegationRunControlStatus),
    Stale(DelegationRunControlStatus),
    NotFound,
}

// The only existing CarsinOS records that can be cited as ExecAss authority.
// This deliberately excludes mutable container or recipient records whose
// identity would be a misleading stand-in for a specific authority source.
text_enum!(AuthorityLinkKind {
    Session => "session",
    Run => "run",
    Job => "job",
    JobRun => "job_run",
    Task => "task",
    Board => "board",
    BoardCard => "board_card",
    MailThread => "mail_thread",
    MailMessage => "mail_message",
    ArtifactAttachment => "artifact_attachment",
    ArtifactBoardCardAsset => "artifact_board_card_asset",
    ArtifactMailAttachment => "artifact_mail_attachment",
    SecurityAuditEvent => "security_audit_event",
    AssistantToolCallAudit => "assistant_tool_call_audit",
    ToolCall => "tool_call",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedAuthorityKind {
    /// The current repository has a Team presentation but no authoritative
    /// team record/table. Keep that absence typed instead of inventing one.
    Team,
    Project,
    Goal,
    BoardColumn,
    GenericMessage,
    MailRecipient,
    FileLease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityLinkTarget {
    Session { session_id: String },
    Run { run_id: String },
    Job { job_id: String },
    JobRun { job_run_id: String },
    Task { task_id: String },
    Board { board_id: String },
    BoardCard { board_card_id: String },
    MailThread { mail_thread_id: String },
    MailMessage { mail_message_id: String },
    ArtifactAttachment { attachment_id: String },
    ArtifactBoardCardAsset { board_card_asset_id: String },
    ArtifactMailAttachment { mail_attachment_id: String },
    SecurityAuditEvent { event_id: String },
    AssistantToolCallAudit { event_id: String },
    ToolCall { tool_call_id: String },
    Unsupported { kind: UnsupportedAuthorityKind },
}

impl AuthorityLinkTarget {
    pub fn kind(&self) -> Option<AuthorityLinkKind> {
        Some(match self {
            Self::Session { .. } => AuthorityLinkKind::Session,
            Self::Run { .. } => AuthorityLinkKind::Run,
            Self::Job { .. } => AuthorityLinkKind::Job,
            Self::JobRun { .. } => AuthorityLinkKind::JobRun,
            Self::Task { .. } => AuthorityLinkKind::Task,
            Self::Board { .. } => AuthorityLinkKind::Board,
            Self::BoardCard { .. } => AuthorityLinkKind::BoardCard,
            Self::MailThread { .. } => AuthorityLinkKind::MailThread,
            Self::MailMessage { .. } => AuthorityLinkKind::MailMessage,
            Self::ArtifactAttachment { .. } => AuthorityLinkKind::ArtifactAttachment,
            Self::ArtifactBoardCardAsset { .. } => AuthorityLinkKind::ArtifactBoardCardAsset,
            Self::ArtifactMailAttachment { .. } => AuthorityLinkKind::ArtifactMailAttachment,
            Self::SecurityAuditEvent { .. } => AuthorityLinkKind::SecurityAuditEvent,
            Self::AssistantToolCallAudit { .. } => AuthorityLinkKind::AssistantToolCallAudit,
            Self::ToolCall { .. } => AuthorityLinkKind::ToolCall,
            Self::Unsupported { .. } => return None,
        })
    }

    pub fn source_id(&self) -> Option<&str> {
        match self {
            Self::Session { session_id } => Some(session_id),
            Self::Run { run_id } => Some(run_id),
            Self::Job { job_id } => Some(job_id),
            Self::JobRun { job_run_id } => Some(job_run_id),
            Self::Task { task_id } => Some(task_id),
            Self::Board { board_id } => Some(board_id),
            Self::BoardCard { board_card_id } => Some(board_card_id),
            Self::MailThread { mail_thread_id } => Some(mail_thread_id),
            Self::MailMessage { mail_message_id } => Some(mail_message_id),
            Self::ArtifactAttachment { attachment_id } => Some(attachment_id),
            Self::ArtifactBoardCardAsset {
                board_card_asset_id,
            } => Some(board_card_asset_id),
            Self::ArtifactMailAttachment { mail_attachment_id } => Some(mail_attachment_id),
            Self::SecurityAuditEvent { event_id } | Self::AssistantToolCallAudit { event_id } => {
                Some(event_id)
            }
            Self::ToolCall { tool_call_id } => Some(tool_call_id),
            Self::Unsupported { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuthorityLink {
    pub link_id: String,
    pub target: AuthorityLinkTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendAuthorityLineageCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub resulting_state_revision: i64,
    pub linked_at: i64,
    pub links: Vec<NewAuthorityLink>,
    pub outbox_event: NewOutboxEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoritySourceLocation {
    Live,
    Archived,
}

/// Deliberately safe: no status, content, paths, payloads, tool data, or audit
/// metadata escape through lineage resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityLinkProjection {
    pub link_id: String,
    pub delegation_id: String,
    pub link_revision: i64,
    pub delegation_state_revision: i64,
    pub kind: AuthorityLinkKind,
    pub source_id: String,
    pub authoritative_revision: i64,
    pub linked_at: i64,
    pub outbox_event_id: String,
    pub location: AuthoritySourceLocation,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachabilityRecordRef {
    pub record_id: String,
    /// The record's immutable domain revision or sequence. Identity-only
    /// records use zero rather than inventing a mutable version.
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationReachabilityReport {
    pub delegation_id: String,
    pub delegation_state_revision: i64,
    pub authority_provenance: Vec<ReachabilityRecordRef>,
    pub plans: Vec<ReachabilityRecordRef>,
    pub plan_amendments: Vec<ReachabilityRecordRef>,
    pub outcome_criteria: Vec<ReachabilityRecordRef>,
    pub verifier_results: Vec<ReachabilityRecordRef>,
    pub decisions: Vec<ReachabilityRecordRef>,
    pub confirmation_challenges: Vec<ReachabilityRecordRef>,
    pub accepted_confirmation_grants: Vec<ReachabilityRecordRef>,
    pub continuations: Vec<ReachabilityRecordRef>,
    pub continuation_operation_history: Vec<ReachabilityRecordRef>,
    pub logical_effects: Vec<ReachabilityRecordRef>,
    pub provider_attempts: Vec<ReachabilityRecordRef>,
    pub effect_tombstones: Vec<ReachabilityRecordRef>,
    pub technical_resource_quota_snapshots: Vec<ReachabilityRecordRef>,
    pub technical_resource_quota_entries: Vec<ReachabilityRecordRef>,
    pub technical_resource_requirement_sets: Vec<ReachabilityRecordRef>,
    pub technical_resource_requirements: Vec<ReachabilityRecordRef>,
    pub technical_resource_reservations: Vec<ReachabilityRecordRef>,
    pub technical_resource_actuals: Vec<ReachabilityRecordRef>,
    pub receipts: Vec<ReachabilityRecordRef>,
    pub outbox_events: Vec<ReachabilityRecordRef>,
    pub authority_links: Vec<ReachabilityRecordRef>,
    pub violations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationReachabilityOutcome {
    Valid(DelegationReachabilityReport),
    Invalid(DelegationReachabilityReport),
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityLineageAppend {
    pub delegation_id: String,
    pub resulting_state_revision: i64,
    pub outbox_event: OutboxEventRecord,
    pub links: Vec<AuthorityLinkProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityLineageOutcome {
    Appended(AuthorityLineageAppend),
    Replayed(AuthorityLineageAppend),
    Stale {
        current_state_revision: i64,
    },
    NotFound,
    Conflict {
        duplicate_identity: String,
    },
    OwnershipMismatch {
        kind: AuthorityLinkKind,
        source_id: String,
        expected_owner: String,
        actual_owner: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityOwnerKind {
    Agent,
    Session,
    Run,
    Job,
    Project,
    Board,
    BoardCard,
    Message,
    MailThread,
    MailMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityOwnershipCheck {
    pub link_id: String,
    pub owner_kind: AuthorityOwnerKind,
    pub expected_owner_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityOwnershipMismatch {
    pub kind: AuthorityLinkKind,
    pub source_id: String,
    pub expected_owner: String,
    pub actual_owner: Option<String>,
}

/// The production orchestration-adapter input. Deliberately absent are child
/// status, run status, completion, and delegation phase: authoritative work may
/// be observed here, but it cannot directly terminalize the coordinating
/// Delegation aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveOrchestrationCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub resulting_state_revision: i64,
    pub observed_at: i64,
    pub references: Vec<NewAuthorityLink>,
    pub ownership_checks: Vec<AuthorityOwnershipCheck>,
    pub outbox_event: NewOutboxEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestrationObservationOutcome {
    Linked(AuthorityLineageAppend),
    Replayed(AuthorityLineageAppend),
    Stale {
        current_state_revision: i64,
    },
    MissingDelegation,
    MissingAuthority {
        kind: AuthorityLinkKind,
        source_id: String,
    },
    UnsupportedAuthority {
        kind: UnsupportedAuthorityKind,
    },
    Conflict {
        duplicate_identity: String,
    },
    OwnershipMismatch {
        kind: AuthorityLinkKind,
        source_id: String,
        expected_owner: String,
        actual_owner: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestrationRereadOutcome {
    Current(Vec<AuthorityLinkProjection>),
    MissingDelegation,
    MissingAuthority {
        kind: AuthorityLinkKind,
        source_id: String,
    },
    OwnershipMismatch {
        kind: AuthorityLinkKind,
        source_id: String,
        expected_owner: String,
        actual_owner: Option<String>,
    },
}

#[derive(Debug)]
pub enum AuthorityLineageError {
    Unsupported(UnsupportedAuthorityKind),
    MissingSource {
        kind: AuthorityLinkKind,
        source_id: String,
    },
}

impl std::fmt::Display for AuthorityLineageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(kind) => {
                write!(formatter, "unsupported ExecAss authority kind: {kind:?}")
            }
            Self::MissingSource { kind, source_id } => write!(
                formatter,
                "authority source is missing: {kind:?}/{source_id}"
            ),
        }
    }
}

impl std::error::Error for AuthorityLineageError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteContext {
    pub idempotency_key: String,
    pub correlation_id: String,
    pub causation_id: String,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptSubject {
    pub kind: ReceiptSubjectKind,
    pub subject_id: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptActorBinding {
    pub actor_type: ActorType,
    pub actor_identity: super::redaction::SafeText,
    pub authority_provenance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptRuntimeBinding {
    pub host_generation: i64,
    pub host_instance_id: String,
    pub fencing_token: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptEvidenceInput {
    pub authority_link_id: String,
    pub kind: AuthorityLinkKind,
    pub source_id: String,
    pub authoritative_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptRotation {
    pub transition_id: String,
    pub reason: super::redaction::SafeText,
    pub previous_key: super::receipt_integrity::ReceiptKeyRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendReceiptCommand {
    pub receipt_id: String,
    pub transaction_id: String,
    pub state_root_generation: i64,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub expected_global_count: i64,
    pub expected_global_head_digest: Option<String>,
    pub expected_delegation_count: i64,
    pub expected_delegation_head_digest: Option<String>,
    pub receipt_kind: ReceiptKind,
    pub subject: ReceiptSubject,
    pub causation_id: String,
    pub causation_event_id: String,
    pub actor: ReceiptActorBinding,
    pub runtime: ReceiptRuntimeBinding,
    pub key: super::receipt_integrity::ReceiptKeyRef,
    pub rotation: Option<ReceiptRotation>,
    pub evidence: Vec<ReceiptEvidenceInput>,
    pub redacted_summary: super::redaction::SafeText,
    pub occurred_at: i64,
    pub committed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptRecord {
    pub receipt_id: String,
    pub delegation_id: Option<String>,
    pub delegation_sequence: Option<i64>,
    pub global_sequence: i64,
    pub append_identity: String,
    pub receipt_digest: String,
    pub keyed_integrity_tag: String,
    pub previous_key_integrity_tag: Option<String>,
    pub canonical_payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendReceiptOutcome {
    Appended(ReceiptRecord),
    Replayed(ReceiptRecord),
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
    Conflict {
        append_identity: String,
    },
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityProvenanceRecord {
    pub authority_provenance_id: String,
    pub actor_type: ActorType,
    pub credential_identity: String,
    pub authenticated_ingress: String,
    pub channel_assurance: String,
    pub source_correlation_id: String,
    pub source_message_id: Option<String>,
    pub authority_kind: AuthorityKind,
    pub normalized_scope_json: String,
    pub policy_revision: i64,
    pub bound_decision_id: Option<String>,
    pub bound_decision_revision: Option<i64>,
    pub bound_manifest_digest: Option<String>,
    pub bound_challenge_nonce_digest: Option<String>,
    pub evidence_digest: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// One expiring presentation of a dangerous action.  It can resolve exactly
/// once and is deliberately distinct from the durable accepted grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmationChallengeRecord {
    pub challenge_id: String,
    pub decision_id: String,
    pub delegation_id: String,
    pub decision_revision: i64,
    pub exact_presented_action_json: String,
    pub confirmed_logical_action_identity: String,
    pub manifest_digest: String,
    pub payload_digest: String,
    pub payload_and_material_operands_json: String,
    pub connector_tool_identity: Option<String>,
    pub connector_tool_version: Option<String>,
    pub canonical_action_envelope_or_selector_json: String,
    pub declared_consequence: String,
    pub nonce_digest: String,
    pub status: ConfirmationChallengeStatus,
    pub created_at: i64,
    pub expires_at: i64,
    pub resolved_at: Option<i64>,
}

text_enum!(ConfirmationChallengeStatus {
    Pending => "pending",
    Resolved => "resolved",
    Expired => "expired",
});

/// Durable accepted confirmation for one confirmed logical action.  It has no
/// expiry or use counter; execution-time revalidation must compare the bound
/// action rather than consume this record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedConfirmationGrantRecord {
    pub grant_id: String,
    pub delegation_id: String,
    pub decision_id: String,
    pub confirmed_logical_action_identity: String,
    pub canonical_action_envelope_or_selector_json: String,
    pub payload_and_material_operands_json: String,
    pub payload_and_material_operands_digest: String,
    pub connector_tool_identity: Option<String>,
    pub connector_tool_version: Option<String>,
    pub declared_consequence: String,
    pub accepted_by_authority_provenance_id: String,
    pub confirmation_attestation_digest: String,
    pub accepted_at: i64,
    pub invalidated_at: Option<i64>,
    pub invalidation_reason: Option<AcceptedConfirmationGrantInvalidation>,
    pub invalidated_by_authority_provenance_id: Option<String>,
}

/// Owner-controlled production configuration for one authenticated remote
/// confirmation ingress. Storage derives every persisted identity/assurance
/// field from these bounded provider facts; callers cannot supply a generic
/// actor type or assurance label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOwnerConfirmationIngress {
    pub provider: String,
    pub owner_account_id: String,
    pub authenticated_ingress: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentDangerousActionConfirmationCommand {
    pub delegation_id: String,
    pub logical_action_id: String,
    pub decision_id: String,
    pub challenge_id: String,
    pub idempotency_key: String,
    /// One-time raw nonce/token bytes. The store persists only the canonical
    /// digest and never returns these bytes.
    pub challenge_nonce: Vec<u8>,
    pub requested_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DangerConfirmationAdmissionOutcome {
    AlreadyConfirmed(AcceptedConfirmationGrantRecord),
    ExistingPending(ConfirmationChallengeRecord),
    Presented(ConfirmationChallengeRecord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDangerConfirmationBinding {
    pub delegation_id: String,
    pub normalized_intent: String,
    pub policy_revision: i64,
    pub decision_id: String,
    pub decision_revision: i64,
    pub canonical_manifest_json: String,
    pub manifest_digest: String,
    pub exact_presented_action_json: String,
    pub exact_presented_action_digest: String,
    pub declared_consequence: String,
    /// Present only for the bounded EA-206 combined-question form.  Its
    /// alternatives are canonical, server-derived disclosures; callers cannot
    /// manufacture them through a public constructor.
    pub combined_question: Option<CombinedDangerousActionQuestion>,
    pub challenge_nonce_digest: String,
    pub requested_at: i64,
    pub expires_at: i64,
}

/// Exact storage-selected input for signing one disclosed pending alternative.
/// It deliberately excludes raw nonce bytes and all verification-key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDangerConfirmationAlternativeBinding {
    pub delegation_id: String,
    pub normalized_intent: String,
    pub policy_revision: i64,
    pub decision_id: String,
    pub decision_revision: i64,
    pub canonical_manifest_json: String,
    pub manifest_digest: String,
    pub selected_logical_action_id: String,
    pub exact_selected_action_json: String,
    pub exact_selected_action_digest: String,
    pub declared_consequence: String,
    pub declared_consequence_digest: String,
    pub challenge_nonce_digest: String,
    pub requested_at: i64,
    pub expires_at: i64,
}

/// Exact server-derived material for resolving any current typed decision.
/// Dangerous alternatives retain their live disclosed challenge; other kinds
/// use the persisted decision idempotency identity as their canonical nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionResolutionBinding {
    pub delegation_id: String,
    pub normalized_intent: String,
    pub policy_revision: i64,
    pub decision_id: String,
    pub decision_revision: i64,
    pub decision_kind: DecisionKind,
    pub canonical_manifest_json: String,
    pub manifest_digest: String,
    pub selected_logical_action_id: String,
    pub exact_selected_action_json: String,
    pub exact_selected_action_digest: String,
    pub declared_consequence: String,
    pub declared_consequence_digest: String,
    pub challenge_nonce_digest: String,
    pub requested_at: i64,
    pub expires_at: i64,
}

/// Storage-owned view used by the private gateway confirmation runtime.  The
/// resolved variant intentionally returns only the immutable selected binding
/// and its durable grant; the persisted signature and payload remain internal
/// replay-integrity state and are never re-issued to a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DangerConfirmationRuntimeProjection {
    Pending(Box<PendingDangerConfirmationAlternativeBinding>),
    Resolved(Box<ResolvedDangerConfirmationAlternativeBinding>),
}

/// One already-resolved confirmation whose attestation-to-grant linkage was
/// verified by storage before this projection was returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDangerConfirmationAlternativeBinding {
    pub binding: PendingDangerConfirmationAlternativeBinding,
    pub grant: AcceptedConfirmationGrantRecord,
}

/// One server-derived alternative disclosed by a combined dangerous-action
/// question.  The fields are deliberately private: storage derives this only
/// from canonical leaves and opaque verified danger routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisclosedDangerousAlternative {
    pub(crate) logical_action_id: String,
    pub(crate) exact_presented_action_json: String,
    pub(crate) confirmed_logical_action_identity: String,
    pub(crate) manifest_digest: String,
    pub(crate) payload_digest: String,
    pub(crate) payload_and_material_operands_json: String,
    pub(crate) resolved_scope_json: String,
    pub(crate) connector_tool_identity: String,
    pub(crate) connector_tool_version: String,
    pub(crate) canonical_action_envelope_or_selector_json: String,
    pub(crate) declared_consequence: String,
}

impl DisclosedDangerousAlternative {
    pub fn logical_action_id(&self) -> &str {
        &self.logical_action_id
    }

    pub fn exact_presented_action_json(&self) -> &str {
        &self.exact_presented_action_json
    }

    pub fn resolved_scope_json(&self) -> &str {
        &self.resolved_scope_json
    }

    pub fn declared_consequence(&self) -> &str {
        &self.declared_consequence
    }
}

/// Canonical disclosure for a combined dangerous-action question. Selection is
/// deliberately absent: it belongs to the verified owner response, never the
/// presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedDangerousActionQuestion {
    pub(crate) alternatives: Vec<DisclosedDangerousAlternative>,
}

impl CombinedDangerousActionQuestion {
    pub fn alternatives(&self) -> &[DisclosedDangerousAlternative] {
        &self.alternatives
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmDangerousActionCommand {
    pub decision_id: String,
    pub decision_revision: i64,
    pub grant_id: String,
    /// Exact opaque logical-action identifier disclosed by the pending
    /// challenge. It is checked transactionally against immutable bindings.
    pub selected_logical_action_id: String,
    pub response: DangerousActionConfirmationResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRecord {
    pub decision_id: String,
    pub delegation_id: String,
    pub decision_revision: i64,
    pub delegation_revision: i64,
    pub plan_revision: i64,
    pub policy_revision: i64,
    pub decision_kind: DecisionKind,
    pub status: DecisionStatus,
    pub result: Option<DecisionResult>,
    pub confirmed_logical_action_identity: String,
    pub manifest_digest: String,
    pub idempotency_key: String,
    pub requested_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by_authority_provenance_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedLogicalEffectRecord {
    pub logical_effect_id: String,
    pub delegation_id: String,
    pub continuation_id: String,
    pub action_kind: LogicalEffectActionKind,
    pub operation_reversible: bool,
    pub declared_recovery_safe_boundary: DeclaredRecoverySafeBoundary,
    pub internal_idempotency_key: String,
    pub provider_identity: Option<String>,
    pub provider_idempotency_key: Option<String>,
    pub reconciliation_key: Option<String>,
    pub manifest_digest: String,
    pub payload_digest: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateRiskBindingRecord {
    pub decision_id: String,
    pub delegation_id: String,
    pub predecessor_logical_effect_id: String,
    pub predecessor_attempt_id: String,
    pub predecessor_uncertainty_evidence_digest: String,
    pub confirmed_logical_action_identity: String,
    pub accepted_confirmation_grant_id: Option<String>,
    pub created_at: i64,
}

/// Canonical successor material derived entirely from a frozen duplicate-risk
/// predecessor, the persisted decision/action binding, and one verified owner
/// resolution identity.  Gateways cannot select or rewrite effect facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDuplicateRiskSuccessor {
    pub continuation: ContinuationRecord,
    pub logical_effect: PlannedLogicalEffectRecord,
    pub technical_quota_snapshot: CanonicalTechnicalQuotaSnapshot,
    pub technical_resource_requirements: CanonicalTechnicalResourceRequirementSet,
}

/// Canonical execution material for the single installed destructive leaf.
/// Every field is reconstructed from persisted decision, manifest, action, and
/// delegation authority; gateways can request it but cannot choose effect or
/// resource facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedExactDangerousEffect {
    pub continuation: ContinuationRecord,
    pub logical_effect: PlannedLogicalEffectRecord,
    pub technical_quota_snapshot: CanonicalTechnicalQuotaSnapshot,
    pub technical_resource_requirements: CanonicalTechnicalResourceRequirementSet,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExactDangerousEffectExecutionMaterial {
    pub logical_effect_id: String,
    pub provider_identity: String,
    pub provider_version: String,
    pub adapter_identity: String,
    pub payload_digest: String,
    pub reconciliation_key: String,
    pub operand_envelope: OpaqueOperandEnvelopeV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicDecisionResolutionCommand {
    pub write: WriteContext,
    pub decision_id: String,
    pub decision_revision: i64,
    pub result: DecisionResult,
    pub selected_logical_action_id: Option<String>,
    pub continuation: Option<ContinuationRecord>,
    pub logical_effect: Option<PlannedLogicalEffectRecord>,
    pub technical_quota_snapshot: Option<CanonicalTechnicalQuotaSnapshot>,
    pub technical_resource_requirements: Option<CanonicalTechnicalResourceRequirementSet>,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicDecisionResolutionBundle {
    pub decision: DecisionRecord,
    pub confirmation_grant: Option<AcceptedConfirmationGrantRecord>,
    pub continuation: Option<ContinuationRecord>,
    pub logical_effect: Option<PlannedLogicalEffectRecord>,
    pub technical_quota_snapshot: Option<TechnicalQuotaSnapshotRecord>,
    pub technical_resource_requirements: Option<TechnicalResourceRequirementSetRecord>,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalQuotaEntryRecord {
    pub quota_snapshot_id: String,
    pub technical_resource_kind: TechnicalResourceKind,
    pub unit: String,
    pub amount_limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalQuotaSnapshotRecord {
    pub quota_snapshot_id: String,
    pub delegation_id: String,
    pub policy_revision: i64,
    pub effective_authority_digest: String,
    pub scope_key: String,
    pub canonical_entries_json: String,
    pub canonical_entries_digest: String,
    pub created_at: i64,
    pub entries: Vec<TechnicalQuotaEntryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceRequirementRecord {
    pub requirement_set_id: String,
    pub quota_snapshot_id: String,
    pub technical_resource_kind: TechnicalResourceKind,
    pub unit: String,
    pub amount_required: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceRequirementSetRecord {
    pub requirement_set_id: String,
    pub quota_snapshot_id: String,
    pub delegation_id: String,
    pub logical_effect_id: String,
    pub action_id: String,
    pub manifest_digest: String,
    pub canonical_requirements_json: String,
    pub canonical_requirements_digest: String,
    pub created_at: i64,
    pub requirements: Vec<TechnicalResourceRequirementRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicDecisionResolutionOutcome {
    Applied(Box<AtomicDecisionResolutionBundle>),
    Replayed(Box<AtomicDecisionResolutionBundle>),
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
    NotFound,
    Conflict {
        winning_result: Option<DecisionResult>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionReceiptContext {
    pub delegation_id: String,
    pub delegation_revision: i64,
    pub plan_revision: i64,
    pub stop_epoch: i64,
    pub global_stop_epoch: i64,
    pub global_receipt_count: i64,
    pub global_receipt_head_digest: Option<String>,
    pub delegation_receipt_count: i64,
    pub delegation_receipt_head_digest: Option<String>,
    pub state_root_generation: i64,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
}

text_enum!(DangerousActionConfirmationResponse {
    ConfirmAndContinue => "confirm_and_continue",
    Revise => "revise",
    Decline => "decline",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DangerConfirmationResolutionOutcome {
    Confirmed(AcceptedConfirmationGrantRecord),
    Replayed(AcceptedConfirmationGrantRecord),
    Revised,
    Declined,
    Expired,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidateAcceptedConfirmationGrantCommand {
    pub grant_id: String,
    pub decision_id: String,
    pub invalidation_reason: AcceptedConfirmationGrantInvalidation,
    pub invalidated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationGrantInvalidationOutcome {
    Invalidated(AcceptedConfirmationGrantRecord),
    Replayed(AcceptedConfirmationGrantRecord),
    NotFound,
}

text_enum!(AcceptedConfirmationGrantInvalidation {
    MaterialTargetDrift => "material_target_drift",
    MaterialScopeDrift => "material_scope_drift",
    MaterialPayloadDrift => "material_payload_drift",
    MaterialToolDrift => "material_tool_drift",
    MaterialConsequenceDrift => "material_consequence_drift",
    ExplicitActionSpecificOwnerAmendment => "explicit_action_specific_owner_amendment",
    ExplicitActionSpecificOwnerRevocation => "explicit_action_specific_owner_revocation",
    ExplicitActionSpecificOwnerCancellation => "explicit_action_specific_owner_cancellation",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationRecord {
    pub delegation_id: String,
    pub normalized_original_intent: String,
    pub intake_evidence_json: String,
    pub ingress_source: String,
    pub ingress_credential_identity: String,
    pub source_message_id: Option<String>,
    pub source_correlation_id: String,
    pub ingress_idempotency_key: String,
    pub classifier_version: String,
    pub classifier_reasons_json: String,
    pub phase: DelegationPhase,
    pub run_control: RunControlState,
    pub state_revision: i64,
    pub current_plan_revision: Option<i64>,
    pub current_criteria_revision: Option<i64>,
    pub policy_revision: i64,
    pub effective_authority_json: String,
    pub authority_provenance_id: String,
    pub pending_decision_id: Option<String>,
    pub external_wait_json: Option<String>,
    pub stop_epoch: i64,
    pub completion_assessment_json: Option<String>,
    pub receipt_chain_count: i64,
    pub receipt_chain_head_digest: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub acknowledged_at: Option<i64>,
    pub terminal_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRecord {
    pub plan_id: String,
    pub delegation_id: String,
    pub plan_revision: i64,
    pub based_on_delegation_revision: i64,
    pub policy_revision: i64,
    pub plan_summary: String,
    pub resolved_leaf_manifest_json: String,
    pub manifest_digest: String,
    pub created_by_authority_provenance_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeCriterionRecord {
    pub criterion_id: String,
    pub delegation_id: String,
    pub criteria_revision: i64,
    pub criterion_key: String,
    pub description: String,
    pub material: bool,
    pub verifier_type: VerifierType,
    pub expected_predicate_json: String,
    pub authoritative_source_kind: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationRecord {
    pub continuation_id: String,
    pub delegation_id: String,
    pub target_delegation_revision: i64,
    pub target_plan_revision: i64,
    pub action_id: String,
    pub branch_kind: ActionBranchKind,
    pub causation_kind: ContinuationCausationKind,
    pub causation_id: String,
    pub status: ContinuationStatus,
    pub job_id: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub fencing_token: i64,
    pub host_generation: i64,
    pub stop_epoch: i64,
    pub global_stop_epoch: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

/// Immutable saved routine definition.  The selector/envelope are deliberately
/// opaque canonical JSON: unrelated business classification controls are not
/// part of this storage surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineVersionRecord {
    pub routine_id: String,
    pub routine_version: i64,
    pub source_delegation_id: String,
    pub saved_owner_authority_provenance_id: String,
    pub normalized_original_intent: String,
    pub resolved_leaf_manifest_json: String,
    pub manifest_digest: String,
    pub saved_selector_json: String,
    pub saved_action_envelope_json: String,
    pub accepted_confirmation_grant_id: Option<String>,
    pub effective_policy_snapshot_json: String,
    pub effective_policy_revision: i64,
    pub stable_leaf_digest: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineRecord {
    pub routine_id: String,
    pub current_version: i64,
    pub enabled: bool,
    pub timezone: String,
    pub overlap_policy: RoutineOverlapPolicy,
    pub catch_up_policy: RoutineCatchUpPolicy,
    pub replay_cap: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineOccurrenceRecord {
    pub occurrence_id: String,
    pub routine_id: String,
    pub routine_version: i64,
    pub scheduled_instant_ms: i64,
    pub scheduled_local: String,
    pub utc_offset_seconds: i64,
    pub time_resolution: RoutineTimeResolution,
    pub effective_policy_revision: i64,
    pub status: RoutineOccurrenceStatus,
    pub admission_plan_json: Option<String>,
    pub admitted_delegation_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineScheduleSpec {
    pub local_hour: u32,
    pub local_minute: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRoutineCommand {
    pub routine: RoutineRecord,
    pub version: RoutineVersionRecord,
    pub schedule: RoutineScheduleSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineOccurrenceCandidate {
    pub scheduled_instant_ms: i64,
    pub scheduled_local: String,
    pub utc_offset_seconds: i64,
    pub time_resolution: RoutineTimeResolution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendRoutineCommand {
    pub expected_current_version: i64,
    pub routine: RoutineRecord,
    pub version: RoutineVersionRecord,
    pub schedule: RoutineScheduleSpec,
}

/// Exact lease identity for the one reserved scheduler job that advances a
/// routine.  Callers cannot materialize occurrences from a routine ID and a
/// clock alone; the existing jobs scheduler must first win this lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineDriverClaim {
    pub routine_id: String,
    pub driver_job_id: String,
    pub driver_lease_owner: String,
    pub driver_lease_expires_at: i64,
    pub trusted_now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineTriggerSettlementCommand {
    pub occurrence_id: String,
    pub trigger_job_id: String,
    pub trigger_lease_owner: String,
    pub trigger_lease_expires_at: i64,
    pub trusted_now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutineTriggerSettlementOutcome {
    Settled,
    Replayed,
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineAdmissionRequest {
    pub occurrence_id: String,
    pub trigger_job_id: String,
    pub trigger_lease_owner: String,
    pub trigger_lease_expires_at: i64,
    pub trusted_now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineAdmissionPlan {
    pub occurrence: RoutineOccurrenceRecord,
    pub routine_version: RoutineVersionRecord,
    pub trigger_job_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutineAdmissionOutcome {
    Planned(Box<RoutineAdmissionPlan>),
    Replayed(Box<RoutineAdmissionPlan>),
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationClaimIdentity {
    pub claim_event_id: String,
    pub claim_receipt_id: String,
    pub continuation_id: String,
    pub delegation_id: String,
    pub action_id: String,
    pub job_id: String,
    pub worker_id: String,
    pub job_lease_expires_at: i64,
    pub continuation_fencing_token: i64,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
    pub state_root_generation: i64,
    pub runtime_authority_provenance_id: String,
    pub runtime_actor_identity: String,
    pub policy_revision: i64,
    pub global_stop_epoch: i64,
    pub technical_quota_policy_digest: String,
    pub technical_quota_snapshot_id: Option<String>,
    pub technical_resource_reservation_set_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TechnicalResourceReservationIdentity {
    pub reservation_id: String,
    pub quota_snapshot_id: String,
    pub logical_effect_id: String,
    pub technical_resource_kind: String,
    pub unit: String,
    pub amount_reserved: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceReservationRecord {
    pub identity: TechnicalResourceReservationIdentity,
    pub delegation_id: String,
    pub continuation_id: String,
    pub claim_event_id: String,
    pub claim_receipt_id: String,
    pub status: String,
    pub idempotency_key: String,
    pub continuation_fencing_token: i64,
    pub runtime_host_generation: i64,
    pub runtime_fencing_token: i64,
    pub created_at: i64,
    pub expires_at: i64,
    pub settled_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceActualInput {
    pub reservation_id: String,
    pub amount_actual: i64,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechnicalResourceLifecycleResolution {
    ExpireUndispatched,
    RecoverPossiblyInvoked,
    /// Retained only for source compatibility. The public lifecycle entry
    /// point rejects caller-selected reconciliation; signed recorder evidence
    /// is the sole reconciliation authority.
    ReconcileAbsent,
    /// Retained only for source compatibility. See `ReconcileAbsent`.
    ReconcilePresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechnicalResourceRecoveryKind {
    ExpireUndispatched,
    RecoverPossiblyInvoked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceRecoveryCandidate {
    pub identity: ContinuationClaimIdentity,
    pub kind: TechnicalResourceRecoveryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceLifecycleCommand {
    pub write: WriteContext,
    pub identity: ContinuationClaimIdentity,
    pub trusted_now: i64,
    pub resolution: TechnicalResourceLifecycleResolution,
    pub evidence_digest: String,
    pub technical_resource_actuals: Vec<TechnicalResourceActualInput>,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalResourceLifecycleRecord {
    pub identity: ContinuationClaimIdentity,
    pub resolution: TechnicalResourceLifecycleResolution,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
    pub technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TechnicalResourceLifecycleOutcome {
    Applied(Box<TechnicalResourceLifecycleRecord>),
    Replayed(Box<TechnicalResourceLifecycleRecord>),
    Lost {
        reason: ContinuationStaleReason,
    },
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ReconcileRecorderEvidenceCommand {
    pub write: WriteContext,
    pub claim_identity: ContinuationClaimIdentity,
    pub trusted_now: i64,
    pub verified_evidence: super::recorder::VerifiedRecorderEvidence,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderEvidenceResult {
    Present,
    Absent,
    Unknown,
}

impl RecorderEvidenceResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Absent => "absent",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderEvidenceImportRecord {
    pub claim_identity: ContinuationClaimIdentity,
    pub result: RecorderEvidenceResult,
    pub recorder_record_digest: String,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
    pub technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderEvidenceImportOutcome {
    Applied(Box<RecorderEvidenceImportRecord>),
    Replayed(Box<RecorderEvidenceImportRecord>),
    Conflict,
    Lost {
        reason: ContinuationStaleReason,
    },
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationOperationReplayRecord {
    pub identity: ContinuationClaimIdentity,
    pub result_status: ContinuationStatus,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
    pub technical_resource_reservations: Vec<TechnicalResourceReservationIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationReceiptContext {
    pub delegation_id: String,
    pub delegation_revision: i64,
    pub policy_revision: i64,
    pub global_stop_epoch: i64,
    pub technical_quota_policy_digest: String,
    pub global_receipt_count: i64,
    pub global_receipt_head_digest: Option<String>,
    pub delegation_receipt_count: i64,
    pub delegation_receipt_head_digest: Option<String>,
    pub state_root_generation: i64,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
    pub runtime_actor: ReceiptActorBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeHostLeaseRecord {
    pub lease_id: String,
    pub state_root_generation: i64,
    pub generation: i64,
    pub host_instance_id: String,
    pub fencing_token: i64,
    pub acquired_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationStaleReason {
    NotFound,
    JobBindingMismatch,
    JobLeaseLostOrExpired,
    JobPayloadMismatch,
    ContinuationNotRunnable,
    ContinuationNotExecuting,
    ClaimIdentityMismatch,
    ActionStateDrift,
    DelegationRevisionDrift,
    PlanRevisionDrift,
    PolicyRevisionDrift,
    MissingCurrentCriteria,
    DelegationRunControlDrift,
    DelegationStopEpochDrift,
    GlobalStopEngaged,
    GlobalStopEpochDrift,
    TechnicalQuotaPolicyDrift,
    TechnicalQuotaSnapshotDrift,
    TechnicalResourceUnavailable,
    TechnicalReservationMissingOrChanged,
    TechnicalReservationExpired,
    RuntimeHostLeaseLostOrExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationClaimCommand {
    pub write: WriteContext,
    pub continuation_id: String,
    pub job_id: String,
    pub worker_id: String,
    pub job_lease_expires_at: i64,
    pub trusted_now: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationClaimRecord {
    pub continuation: ContinuationRecord,
    pub action: ActionBranchRecord,
    pub identity: ContinuationClaimIdentity,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
    pub technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationSupersededRecord {
    pub reason: ContinuationStaleReason,
    pub continuation: ContinuationRecord,
    pub action: ActionBranchRecord,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuationClaimOutcome {
    Claimed(Box<ContinuationClaimRecord>),
    Replayed(Box<ContinuationOperationReplayRecord>),
    Superseded(Box<ContinuationSupersededRecord>),
    Lost {
        reason: ContinuationStaleReason,
    },
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuationDispatchValidationOutcome {
    Valid,
    Stale { reason: ContinuationStaleReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationDispatchValidationCommand {
    pub identity: ContinuationClaimIdentity,
    pub trusted_now: i64,
}

/// Persisted dispatch material. Every key is read from the immutable logical
/// effect row; callers supply neither effect keys nor attempt identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalEffectDispatchIdentity {
    pub logical_effect_id: String,
    pub delegation_id: String,
    pub continuation_id: String,
    pub action_id: String,
    pub claim_event_id: String,
    pub claim_receipt_id: String,
    pub continuation_fencing_token: i64,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
    pub internal_idempotency_key: String,
    pub provider_identity: Option<String>,
    pub provider_idempotency_key: Option<String>,
    pub reconciliation_key: Option<String>,
    pub manifest_digest: String,
    pub payload_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAttemptRecord {
    pub attempt_id: String,
    pub attempt_number: i64,
    pub status: ProviderAttemptStatus,
    pub dispatch: LogicalEffectDispatchIdentity,
    pub provider_request_digest: String,
    pub provider_response_digest: Option<String>,
    pub provider_error_class: Option<ProviderFailureClass>,
    pub remote_effect_id: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareProviderAttemptCommand {
    pub claim: ContinuationClaimIdentity,
    pub trusted_now: i64,
    /// Opaque storage-minted proof for one exact next attempt. Callers cannot
    /// construct or edit it; replay is revalidated against current storage.
    pub retry_authorization: Option<ProviderRetryAuthorization>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRetryAuthorization {
    pub(super) recovery_evaluation_id: String,
    pub(super) logical_effect_id: String,
    pub(super) predecessor_attempt_id: String,
    pub(super) authorized_attempt_number: i64,
    pub(super) not_before_ms: i64,
    pub(super) objective_facts_digest: String,
    pub(super) recovery_state_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveRecoveryEvaluation {
    pub recovery_evaluation_id: String,
    pub directive: carsinos_core::execass_recovery::RecoveryDirective,
    pub retry_authorization: Option<ProviderRetryAuthorization>,
    pub objective_facts_digest: String,
    pub recovery_state_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRecoveryCommand {
    pub write: WriteContext,
    pub logical_effect_id: String,
    pub trusted_now: i64,
    pub expected_pre_state_revision: i64,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRecoveryBundle {
    pub evaluation: ObjectiveRecoveryEvaluation,
    pub selected_phase: DelegationPhase,
    pub state_revision: i64,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRecoveryOutcome {
    Applied(Box<ProviderRecoveryBundle>),
    Replayed(Box<ProviderRecoveryBundle>),
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareProviderAttemptOutcome {
    Prepared(Box<ProviderAttemptRecord>),
    Replayed(Box<ProviderAttemptRecord>),
    Stale { reason: ContinuationStaleReason },
    Conflict,
}

/// The sole storage transition that authorizes an adapter to make the external
/// call. A prepared attempt is durable but deliberately not dispatchable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginProviderAttemptInvocationCommand {
    pub attempt_id: String,
    pub claim: ContinuationClaimIdentity,
    pub trusted_now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginProviderAttemptInvocationOutcome {
    Began(Box<ProviderAttemptRecord>),
    /// The invocation boundary already committed. This outcome is explicitly
    /// non-authorizing: the provider must not be called again.
    AlreadyInvoking(Box<ProviderAttemptRecord>),
    Stale {
        reason: ContinuationStaleReason,
    },
    Conflict,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordProviderAttemptResultCommand {
    pub attempt_id: String,
    pub claim: ContinuationClaimIdentity,
    pub trusted_now: i64,
    pub status: ProviderAttemptStatus,
    pub provider_response_digest: String,
    pub remote_effect_id: Option<String>,
    pub finished_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordProviderAttemptResultOutcome {
    Recorded(Box<ProviderAttemptRecord>),
    Replayed(Box<ProviderAttemptRecord>),
    Conflict,
    NotFound,
    Stale { reason: ContinuationStaleReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationSettleCommand {
    pub write: WriteContext,
    pub identity: ContinuationClaimIdentity,
    pub trusted_now: i64,
    pub result_status: ContinuationStatus,
    pub technical_resource_actuals: Vec<TechnicalResourceActualInput>,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationSettleRecord {
    pub continuation: ContinuationRecord,
    pub action: ActionBranchRecord,
    pub identity: ContinuationClaimIdentity,
    pub outbox_event: OutboxEventRecord,
    pub receipt: ReceiptRecord,
    pub technical_resource_reservations: Vec<TechnicalResourceReservationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuationSettleOutcome {
    Settled(Box<ContinuationSettleRecord>),
    Replayed(Box<ContinuationOperationReplayRecord>),
    Superseded(Box<ContinuationSupersededRecord>),
    Lost {
        reason: ContinuationStaleReason,
    },
    Stale {
        current_state_revision: i64,
        global_count: i64,
        global_head_digest: Option<String>,
        delegation_count: i64,
        delegation_head_digest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionBranchRecord {
    pub action_id: String,
    pub delegation_id: String,
    pub action_revision: i64,
    pub target_delegation_revision: i64,
    pub target_plan_revision: i64,
    pub stop_epoch: i64,
    pub branch_kind: ActionBranchKind,
    pub status: ContinuationStatus,
    pub action_summary: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub terminal_at: Option<i64>,
}

/// Minimal EA-109 resume fence. Later policy/global-stop/budget engines own
/// the actual evaluators; this kernel only binds their already-computed exact
/// snapshots before it permits a resumed continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeProof {
    pub plan_revision: i64,
    pub policy_revision: i64,
    pub authority_provenance_id: String,
    pub budget_snapshot_digest: String,
    pub global_stop_epoch: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItemRecord {
    pub attention_id: String,
    pub delegation_id: String,
    pub action_id: Option<String>,
    pub kind: AttentionKind,
    pub status: AttentionStatus,
    pub reason: String,
    pub recommendation: String,
    pub alternatives_json: String,
    pub required_assurance: String,
    pub decision_id: Option<String>,
    pub delegation_revision: i64,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

/// A host-scoped item in the canonical attention table. It deliberately has
/// no delegation, action, or decision identity: the runtime generation is the
/// authoritative subject and the linked receipt/outbox rows are its evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePausedAttentionRecord {
    pub attention_id: String,
    pub status: AttentionStatus,
    pub reason: String,
    pub recommendation: String,
    pub alternatives_json: String,
    pub required_assurance: String,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
    pub runtime_actual_state: RuntimeActualState,
    pub runtime_end_reason: String,
    pub active_work_binding_digest: String,
    pub outbox_event_id: String,
    pub receipt_id: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalWaitRecord {
    pub external_wait_id: String,
    pub delegation_id: String,
    pub action_id: Option<String>,
    pub kind: ExternalWaitKind,
    pub status: ExternalWaitStatus,
    pub reason: String,
    pub details_json: String,
    pub delegation_revision: i64,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionAssessmentRecord {
    pub assessment_id: String,
    pub delegation_id: String,
    pub assessment_revision: i64,
    pub criteria_revision: i64,
    pub kind: CompletionAssessmentKind,
    pub material_pass_count: i64,
    pub material_fail_count: i64,
    pub material_unknown_count: i64,
    pub useful_outcome: bool,
    pub exact_unmet_portion: Option<String>,
    pub no_remaining_path: bool,
    pub assessment_json: String,
    pub assessed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessCompletionCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub expected_criteria_revision: i64,
    pub expected_assessment_revision: i64,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionAssessmentOutcome {
    Terminalized {
        assessment: CompletionAssessmentRecord,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Replayed {
        assessment: CompletionAssessmentRecord,
        outbox_event: OutboxEventRecord,
    },
    NotTerminal {
        current_phase: DelegationPhase,
        blockers: Vec<String>,
    },
    Stale {
        current_state_revision: i64,
    },
    StaleAssessmentRevision {
        current_assessment_revision: i64,
    },
    CriteriaRevisionMismatch {
        current_criteria_revision: Option<i64>,
    },
    AuthoritativeStateInvalid {
        reason: &'static str,
    },
    MissingDelegation,
    Conflict {
        duplicate_identity: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCorrectionRecord {
    pub correction_id: String,
    pub delegation_id: String,
    pub terminal_assessment_id: String,
    pub correction_revision: i64,
    pub contrary_evidence_json: String,
    pub warning: String,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleTransitionRecord {
    pub transition_id: String,
    pub delegation_id: String,
    pub state_revision: i64,
    pub previous_phase: DelegationPhase,
    pub selected_phase: DelegationPhase,
    pub previous_run_control: RunControlState,
    pub selected_run_control: RunControlState,
    pub selector_input_json: String,
    pub reason: String,
    pub outbox_event_id: String,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSnapshotCommand {
    pub write: WriteContext,
    pub transition_id: String,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub pre_actionable_phase: Option<PreActionablePhase>,
    pub selected_run_control: RunControlState,
    pub resume_proof: Option<ResumeProof>,
    pub action_branches: Vec<ActionBranchRecord>,
    pub attention_items: Vec<AttentionItemRecord>,
    pub external_waits: Vec<ExternalWaitRecord>,
    pub assessment: Option<CompletionAssessmentRecord>,
    pub continuation: Option<ContinuationRecord>,
    pub reason: String,
    pub outbox_event: NewOutboxEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSnapshot {
    pub delegation: DelegationRecord,
    pub transition: LifecycleTransitionRecord,
    pub continuation: Option<ContinuationRecord>,
    pub outbox_event: OutboxEventRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleWriteOutcome {
    Applied(LifecycleSnapshot),
    Replayed(LifecycleSnapshot),
    Stale { current_state_revision: i64 },
    NotFound,
    Conflict { duplicate_identity: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendLifecycleCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub transition_id: String,
    pub amendment_id: String,
    pub amendment_revision: i64,
    pub normalized_amendment: String,
    pub intake_evidence_json: String,
    pub authority_provenance_id: String,
    pub plan: PlanRecord,
    pub outcome_criteria: Vec<OutcomeCriterionRecord>,
    pub outbox_event: NewOutboxEvent,
}

/// Storage input for one already-authenticated, explicitly attached follow-up.
/// The public facade validates the accompanying owner authority, manifest, and
/// sealed danger-routing proof before it appends this amendment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyVerifiedFollowUpAmendmentCommand {
    pub amendment: AmendLifecycleCommand,
    pub receipt: AppendReceiptCommand,
}

/// A follow-up amendment never creates a continuation or effect.  The receipt
/// is intentionally not returned here: it remains reachable from the durable
/// transition/outbox lineage and cannot be substituted by a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedFollowUpAmendmentOutcome {
    Applied(LifecycleSnapshot),
    Replayed(LifecycleSnapshot),
    Stale { current_state_revision: i64 },
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCorrectionCommand {
    pub write: WriteContext,
    pub correction: TerminalCorrectionRecord,
    pub outbox_event: NewOutboxEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLateTerminalCorrectionCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub expected_correction_revision: i64,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LateTerminalCorrectionOutcome {
    Recorded {
        correction: TerminalCorrectionRecord,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Replayed {
        correction: TerminalCorrectionRecord,
        outbox_event: OutboxEventRecord,
    },
    NoContraryEvidence {
        terminal_assessment_id: String,
    },
    Stale {
        current_state_revision: i64,
    },
    StaleCorrectionRevision {
        current_correction_revision: i64,
    },
    AuthoritativeStateInvalid {
        reason: &'static str,
    },
    MissingDelegation,
    NotTerminal,
    Conflict {
        duplicate_identity: String,
    },
}

pub const EXECASS_PROJECTION_RECEIPT_LIMIT: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecAssProjectionQuery {
    pub trusted_now_ms: i64,
    pub receipt_limit: u16,
}

impl ExecAssProjectionQuery {
    pub const fn new(trusted_now_ms: i64) -> Self {
        Self {
            trusted_now_ms,
            receipt_limit: EXECASS_PROJECTION_RECEIPT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionTrust {
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionIntegrityFailure {
    Uninitialized,
    Prepared,
    KeyLost,
    Mismatch,
    Quarantined,
    ConcurrentMovement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "trust", rename_all = "snake_case")]
pub enum ProjectionIntegrity {
    Trusted {
        anchor_generation: i64,
        receipt_count: i64,
        receipt_head_digest: Option<String>,
    },
    Untrusted {
        failure: ProjectionIntegrityFailure,
    },
}

impl ProjectionIntegrity {
    pub const fn trust(&self) -> ProjectionTrust {
        match self {
            Self::Trusted { .. } => ProjectionTrust::Trusted,
            Self::Untrusted { .. } => ProjectionTrust::Untrusted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectionBoundary {
    pub through_global_sequence: i64,
    pub database_receipt_count: i64,
    pub database_receipt_head_digest: Option<String>,
    pub item_set_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDeepLinkKind {
    Delegation,
    Decision,
    Receipt,
    AuthorityRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectionDeepLink {
    pub kind: ProjectionDeepLinkKind,
    pub target_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedsYouKind {
    Confirmation,
    Clarification,
    Reply,
    RecoveryChoice,
    RuntimePaused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "scope_kind", rename_all = "snake_case")]
pub enum AttentionProjectionSubject {
    Delegation {
        delegation_id: String,
        delegation_revision: i64,
    },
    RuntimeHost {
        generation: i64,
        host_instance_id: String,
        fencing_token: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeRecoveryProjectionEvidence {
    pub predecessor_generation: i64,
    pub predecessor_host_instance_id: String,
    pub predecessor_fencing_token: i64,
    pub predecessor_actual_state: RuntimeActualState,
    pub predecessor_end_reason: String,
    pub active_work_binding_digest: String,
    pub outbox_event_id: String,
    pub receipt_id: String,
    pub receipt_deep_link: ProjectionDeepLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDecisionKind {
    Clarification,
    DangerousActionConfirmation,
    OwnerConfiguredCheckpoint,
    RecoveryChoice,
    DuplicateRiskRetry,
    Stop,
    PolicyChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NeedsYouProjectionItem {
    pub attention_id: String,
    pub subject: AttentionProjectionSubject,
    pub kind: NeedsYouKind,
    pub decision_id: Option<String>,
    pub decision_kind: Option<ProjectionDecisionKind>,
    pub decision_revision: Option<i64>,
    pub reason: String,
    pub recommendation: String,
    pub alternative_count: u32,
    pub alternatives: Vec<String>,
    pub required_assurance: String,
    pub deadline_ms: Option<i64>,
    pub created_at_ms: i64,
    pub deep_link: ProjectionDeepLink,
    /// Present only for a host-scoped canonical runtime-paused attention.
    pub runtime_recovery: Option<RuntimeRecoveryProjectionEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InMotionState {
    Active,
    Recovering,
    WaitingExternal,
    Draining,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDelegationPhase {
    Accepted,
    Planning,
    InMotion,
    WaitingForUser,
    WaitingExternal,
    Recovering,
    Completed,
    PartiallyCompleted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InMotionProjectionItem {
    pub delegation_id: String,
    pub delegation_revision: i64,
    pub underlying_phase: ProjectionDelegationPhase,
    pub state: InMotionState,
    pub policy_revision: i64,
    pub external_wait_json: Option<String>,
    pub stop_epoch: i64,
    pub created_at_ms: i64,
    pub acknowledged_at_ms: Option<i64>,
    pub runnable_branch_count: u32,
    pub executing_branch_count: u32,
    pub waiting_external_count: u32,
    pub updated_at_ms: i64,
    pub deep_link: ProjectionDeepLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoneOutcome {
    Completed,
    PartiallyCompleted,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmetCriterionResult {
    Fail,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnmetCriterionProjection {
    pub criterion_id: String,
    pub criterion_key: String,
    pub result: UnmetCriterionResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoneProjectionItem {
    pub delegation_id: String,
    pub delegation_revision: i64,
    pub assessment_id: String,
    pub assessment_revision: i64,
    pub outcome: DoneOutcome,
    pub policy_revision: i64,
    pub run_control: String,
    pub stop_epoch: i64,
    pub created_at_ms: i64,
    pub acknowledged_at_ms: Option<i64>,
    pub trust: ProjectionTrust,
    pub useful_outcome: bool,
    pub material_pass_count: i64,
    pub material_fail_count: i64,
    pub material_unknown_count: i64,
    pub what_did_not_happen: Vec<UnmetCriterionProjection>,
    pub correction_id: Option<String>,
    pub correction_revision: Option<i64>,
    pub correction_warning: Option<String>,
    pub correction_deep_link: Option<ProjectionDeepLink>,
    pub terminal_receipt_deep_link: ProjectionDeepLink,
    pub terminal_at_ms: i64,
    pub deep_link: ProjectionDeepLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NextKind {
    RoutineOccurrence,
    RecoveryReevaluation,
    DangerousConfirmationExpiry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NextDetails {
    RoutineOccurrence {
        scheduled_local: String,
        timezone: String,
    },
    RecoveryReevaluation,
    DangerousConfirmationExpiry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextProjectionItem {
    pub item_id: String,
    pub item_revision: i64,
    pub delegation_id: Option<String>,
    pub kind: NextKind,
    pub due_at_ms: i64,
    pub details: NextDetails,
    pub created_at_ms: i64,
    pub deep_link: ProjectionDeepLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptEvidenceProjection {
    pub ordinal: i64,
    pub authority_kind: ProjectionAuthorityKind,
    pub authority_link_id: String,
    pub source_id: String,
    pub authoritative_revision: i64,
    pub observation_digest: String,
    pub deep_link: ProjectionDeepLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionAuthorityKind {
    Session,
    Run,
    Job,
    JobRun,
    Task,
    Board,
    BoardCard,
    MailThread,
    MailMessage,
    ArtifactAttachment,
    ArtifactBoardCardAsset,
    ArtifactMailAttachment,
    SecurityAuditEvent,
    AssistantToolCallAudit,
    ToolCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionReceiptKind {
    Intake,
    Plan,
    Amendment,
    Decision,
    Continuation,
    Action,
    Effect,
    Verifier,
    Recovery,
    Resume,
    Budget,
    Completion,
    TerminalCorrection,
    AuthorityLink,
    KeyRotation,
    GlobalStop,
    RunControl,
    Policy,
    RuntimeSettings,
    RuntimeRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionReceiptSubjectKind {
    Delegation,
    Plan,
    PlanAmendment,
    Decision,
    Continuation,
    ActionBranch,
    VerifierResult,
    CompletionAssessment,
    TerminalCorrection,
    AuthorityLink,
    RecoveryEvaluation,
    OutboxEvent,
    GlobalRuntimeControl,
    PolicyRevision,
    RuntimeSettingsRevision,
    RuntimeHostGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptProjectionItem {
    pub receipt_id: String,
    pub delegation_id: Option<String>,
    pub delegation_sequence: Option<i64>,
    pub global_sequence: i64,
    pub receipt_kind: ProjectionReceiptKind,
    pub subject_kind: ProjectionReceiptSubjectKind,
    pub subject_id: String,
    pub subject_revision: i64,
    pub receipt_digest: String,
    pub delegation_previous_receipt_digest: Option<String>,
    pub global_previous_receipt_digest: Option<String>,
    pub key_id: String,
    pub key_generation: i64,
    pub integrity_tag: String,
    pub previous_key_integrity_tag: Option<String>,
    pub trust: ProjectionTrust,
    pub redacted_summary: String,
    pub occurred_at_ms: i64,
    pub committed_at_ms: i64,
    pub evidence: Vec<ReceiptEvidenceProjection>,
    pub deep_link: ProjectionDeepLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptProjectionWindow {
    pub limit: u16,
    pub total: i64,
    pub has_older: bool,
    pub earliest_global_sequence: Option<i64>,
    pub latest_global_sequence: Option<i64>,
    pub items: Vec<ReceiptProjectionItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReefActivity {
    Planning,
    Working,
    Recovering,
    WaitingForYou,
    WaitingExternal,
    Draining,
    Stopped,
    Completed,
    PartiallyCompleted,
    Failed,
    IntegrityAttention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReefSubject {
    Delegation { delegation_id: String },
    SystemIntegrity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReefActivityItem {
    pub subject: ReefSubject,
    pub activity: ReefActivity,
    pub fresh_at_ms: i64,
    pub deep_link: Option<ProjectionDeepLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecAssExecutiveProjection {
    pub projection_version: String,
    pub observed_at_ms: i64,
    pub boundary: ProjectionBoundary,
    pub integrity: ProjectionIntegrity,
    pub needs_you: Vec<NeedsYouProjectionItem>,
    pub in_motion: Vec<InMotionProjectionItem>,
    pub done_since_you_checked: Vec<DoneProjectionItem>,
    pub next: Vec<NextProjectionItem>,
    pub receipts: ReceiptProjectionWindow,
    pub reef: Vec<ReefActivityItem>,
}

/// A namespaced item exactly rendered in one executive-summary response.
/// The namespace is part of the identity so an aggregate identifier cannot
/// accidentally acknowledge an item rendered in a different projection pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SummaryProjectionKind {
    NeedsYou,
    InMotion,
    Done,
    Next,
    Receipts,
}

impl SummaryProjectionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NeedsYou => "needs_you",
            Self::InMotion => "in_motion",
            Self::Done => "done",
            Self::Next => "next",
            Self::Receipts => "receipts",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "needs_you" => Some(Self::NeedsYou),
            "in_motion" => Some(Self::InMotion),
            "done" => Some(Self::Done),
            "next" => Some(Self::Next),
            "receipts" => Some(Self::Receipts),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SummaryDeliveredItem {
    pub item_id: String,
    pub revision: i64,
    pub projection_kind: SummaryProjectionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryDeliveryCommand {
    pub delivery_id: String,
    pub request_identity: String,
    pub delivered_at: i64,
    pub projection_version: String,
    pub through_global_sequence: i64,
    pub item_set_digest: String,
    pub items: Vec<SummaryDeliveredItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryDeliveryRecord {
    pub delivery_id: String,
    pub displayed_cursor: String,
    pub projection_version: String,
    pub through_global_sequence: i64,
    pub item_set_digest: String,
    pub request_identity: String,
    pub delivered_at: i64,
    pub items: Vec<SummaryDeliveredItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryDeliveryOutcome {
    Recorded(SummaryDeliveryRecord),
    Replayed(SummaryDeliveryRecord),
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryAcknowledgementCommand {
    pub delivery_id: String,
    pub displayed_cursor: String,
    pub idempotency_key: String,
    pub acknowledged_at: i64,
    pub items: Vec<SummaryDeliveredItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryAcknowledgementRecord {
    pub acknowledgement_id: String,
    pub delivery_id: String,
    pub displayed_cursor: String,
    pub acknowledged_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryAcknowledgementOutcome {
    Acknowledged(SummaryAcknowledgementRecord),
    Replayed(SummaryAcknowledgementRecord),
    Conflict,
    NotDelivered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHoursPolicy {
    pub timezone: String,
    pub start_minute: u16,
    pub end_minute: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationScheduleCommand {
    pub notification_id: String,
    pub source: NotificationSource,
    pub delegation_id: String,
    pub decision_id: Option<String>,
    pub reason_revision: i64,
    pub channel: String,
    pub reason: SafeText,
    pub safe_payload: SafeJson,
    pub scheduled_at: i64,
    pub quiet_hours: Option<QuietHoursPolicy>,
    pub idempotency_key: String,
}

/// The only two notification sources allowed by the executive-summary spec.
/// Completion delivery requires an explicit configuration bit; it is never
/// implied merely because a delegation became terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationSource {
    Attention {
        attention_id: String,
    },
    Completion {
        completion_assessment_id: String,
        completion_enabled: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationScheduleRecord {
    pub notification_id: String,
    pub scheduled_at: i64,
    pub next_reminder_at: Option<i64>,
    pub reminder_count: u8,
    pub last_reminded_at: Option<i64>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationScheduleOutcome {
    Scheduled(NotificationScheduleRecord),
    Replayed(NotificationScheduleRecord),
    Cancelled(NotificationScheduleRecord),
    Conflict,
    NotActionable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCursorKey(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDelegationListQuery {
    pub phase: Option<DelegationPhase>,
    pub run_control: Option<RunControlState>,
    pub limit: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDelegationListEntry {
    pub delegation_id: String,
    pub state_revision: i64,
    pub phase: DelegationPhase,
    pub run_control: RunControlState,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDelegationListPage {
    pub entries: Vec<ApiDelegationListEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiActionRead {
    pub action_id: String,
    pub action_revision: i64,
    pub status: String,
    pub safe_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiEffectRead {
    pub logical_effect_id: String,
    pub continuation_id: String,
    pub state: LogicalEffectState,
    pub provider_identity: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiVerifierRead {
    pub verifier_result_id: String,
    pub criterion_id: String,
    pub result_revision: i64,
    pub result: String,
    pub evidence_digest: String,
    pub verified_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRecoveryRead {
    pub recovery_evaluation_id: String,
    pub logical_effect_id: String,
    pub evaluation_revision: i64,
    pub directive: String,
    pub not_before_ms: Option<i64>,
    pub evaluated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDelegationDetail {
    pub delegation: DelegationRecord,
    pub current_plan: Option<PlanRecord>,
    pub criteria: Vec<OutcomeCriterionRecord>,
    pub actions: Vec<ApiActionRead>,
    pub continuations: Vec<ContinuationRecord>,
    pub effects: Vec<ApiEffectRead>,
    pub recovery: Vec<ApiRecoveryRead>,
    pub verifiers: Vec<ApiVerifierRead>,
    pub receipt_chain_head: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDelegationReceiptPage {
    pub delegation_id: String,
    pub chain_head: Option<String>,
    pub receipts: Vec<ApiReceiptRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiReceiptRead {
    pub receipt_id: String,
    pub delegation_sequence: i64,
    pub global_sequence: i64,
    pub receipt_digest: String,
    pub previous_receipt_digest: Option<String>,
    pub global_previous_receipt_digest: Option<String>,
    pub key_id: String,
    pub key_generation: i64,
    pub integrity_tag: String,
    pub previous_key_integrity_tag: Option<String>,
    pub safe_summary: String,
    pub receipt_kind: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub subject_revision: i64,
    pub occurred_at: i64,
    pub committed_at: i64,
    pub evidence: Vec<ApiReceiptEvidenceRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiReceiptEvidenceRead {
    pub authority_kind: String,
    pub source_id: String,
    pub authoritative_revision: i64,
    pub authority_link_id: String,
    pub observation_digest: String,
    pub deep_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryDeliveryMetadata {
    pub delivery_id: String,
    pub request_identity: String,
    pub delivered_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDecisionRead {
    pub decision: DecisionRecord,
    pub exact_presented_action_json: String,
    pub recommendation: String,
    pub consequence: String,
    pub alternatives_json: String,
    pub challenge_id: Option<String>,
    pub challenge_nonce_digest: Option<String>,
    pub challenge_expires_at: Option<i64>,
    pub accepted_grant: Option<ApiAcceptedGrantRead>,
    pub resolved_owner: Option<ApiResolvedOwnerRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiAcceptedGrantRead {
    pub grant_id: String,
    pub canonical_action_envelope_or_selector_json: String,
    pub payload_and_material_operands_digest: String,
    pub connector_tool_identity_and_version: Option<String>,
    pub declared_consequence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResolvedOwnerRead {
    pub authenticated_ingress: String,
    pub verified_evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecAssPolicyRevisionRecord {
    pub policy_revision: i64,
    pub policy_snapshot_json: String,
    pub policy_snapshot_digest: String,
    pub authority_provenance_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateExecAssPolicyCommand {
    pub expected_policy_revision: i64,
    pub idempotency_key: String,
    pub safe_policy_snapshot: SafeJson,
    pub created_at: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecAssPolicyUpdateOutcome {
    Updated {
        policy: ExecAssPolicyRevisionRecord,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Replayed {
        policy: ExecAssPolicyRevisionRecord,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Stale {
        current_policy_revision: i64,
    },
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecAssRuntimeSettingsRevisionRecord {
    pub settings_revision: i64,
    pub desired_mode: RuntimeDesiredMode,
    pub start_at_login: bool,
    pub settings_json: String,
    pub settings_digest: String,
    pub policy_revision: i64,
    pub authority_provenance_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecAssRuntimeHostStatus {
    pub config: Option<ExecAssRuntimeSettingsRevisionRecord>,
    pub actual_state: RuntimeActualState,
    /// The sole live canonical lease at the trusted read time. Runtime-host
    /// diagnostics must not reuse a startup snapshot after ownership changes.
    pub live_lease: Option<RuntimeHostLeaseRecord>,
}

/// Exact row counts for unfinished local work. These counts are derived only
/// from closed lifecycle state columns; free-form intent, summary, and action
/// text never participate in the close decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecAssActiveWorkStatus {
    pub active: bool,
    pub active_work_count: i64,
    pub nonterminal_delegation_count: i64,
    pub nonterminal_continuation_count: i64,
    pub nonterminal_effect_count: i64,
}

/// One read-transaction snapshot used by native desktop close control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecAssRuntimeCloseSnapshot {
    pub host: ExecAssRuntimeHostStatus,
    pub active_work: ExecAssActiveWorkStatus,
    /// Opaque digest of the exact nonterminal row identities and closed state
    /// values. It binds a confirmation without exposing work content.
    pub active_work_binding_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeHostTransitionOutcome {
    pub from_state: RuntimeActualState,
    pub actual_state: RuntimeActualState,
    pub reason: RuntimeHostTransitionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateExecAssRuntimeSettingsCommand {
    pub expected_settings_revision: i64,
    pub idempotency_key: String,
    pub desired_mode: RuntimeDesiredMode,
    pub start_at_login: bool,
    pub safe_settings: SafeJson,
    pub created_at: i64,
    pub outbox_event: NewOutboxEvent,
    pub receipt: AppendReceiptCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecAssRuntimeSettingsUpdateOutcome {
    Updated {
        status: ExecAssRuntimeHostStatus,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Replayed {
        status: ExecAssRuntimeHostStatus,
        outbox_event: OutboxEventRecord,
        receipt: ReceiptRecord,
    },
    Stale {
        current_settings_revision: i64,
    },
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOutboxEvent {
    pub event_id: String,
    pub event_name: OutboxEventName,
    pub aggregate_id: String,
    pub aggregate_revision: i64,
    pub correlation_id: String,
    pub causation_id: String,
    pub occurred_at: i64,
    pub safe_payload_json: String,
    pub duplicate_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEventRecord {
    pub global_sequence: i64,
    pub event: NewOutboxEvent,
    pub published_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundationBundle {
    pub authority: AuthorityProvenanceRecord,
    pub delegation: DelegationRecord,
    pub plan: PlanRecord,
    pub outcome_criteria: Vec<OutcomeCriterionRecord>,
    pub initial_continuation: Option<ContinuationRecord>,
    pub outbox_events: Vec<OutboxEventRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFoundationCommand {
    pub write: WriteContext,
    pub authority: AuthorityProvenanceRecord,
    pub delegation: DelegationRecord,
    pub plan: PlanRecord,
    pub outcome_criteria: Vec<OutcomeCriterionRecord>,
    pub initial_continuation: Option<ContinuationRecord>,
    pub outbox_event: NewOutboxEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundationWriteOutcome {
    Created(FoundationBundle),
    Replayed(FoundationBundle),
    Conflict {
        existing_delegation_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundationDispatchAdmissionOutcome {
    Admitted(Box<FoundationWriteOutcome>),
    MechanicalResolutionRequired(MechanicalResolutionPause),
    /// All leaves were proven covered by gateway danger routing, and at least
    /// one exact leaf requires the one EA-206 confirmation.  No foundation,
    /// continuation, effect, or challenge is created at this boundary.
    DangerConfirmationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasDelegationStateCommand {
    pub write: WriteContext,
    pub delegation_id: String,
    pub expected_state_revision: i64,
    pub new_state_revision: i64,
    pub phase: DelegationPhase,
    pub run_control: RunControlState,
    pub pending_decision_id: Option<String>,
    pub external_wait_json: Option<String>,
    pub updated_at: i64,
    pub terminal_at: Option<i64>,
    pub outbox_event: NewOutboxEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasDelegationStateUpdated {
    pub delegation: DelegationRecord,
    pub outbox_event: OutboxEventRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasDelegationStateOutcome {
    Updated(Box<CasDelegationStateUpdated>),
    Stale { current_state_revision: i64 },
    NotFound,
}
