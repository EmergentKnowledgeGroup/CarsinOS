//! Focused, fail-closed persistence primitives for the ExecAss v1 aggregate.
//!
//! This module intentionally exposes typed operations only. It does not expose
//! a raw SQLite connection or a generic transaction callback.

mod active_work;
mod aggregate;
mod api_read;
mod canonical;
mod channel_reply;
mod claim;
mod completion;
mod confirmation;
mod confirmation_attestation;
mod confirmation_custody;
#[cfg(feature = "execass-test-confirmation-runtime")]
mod confirmation_test_support;
mod decision;
mod delegation_control;
mod delivery;
mod effect;
mod foundation;
mod global_stop;
mod jobs;
mod lifecycle;
mod lineage;
mod orchestration;
mod policy;
mod projection;
mod receipt;
mod receipt_integrity;
mod recorder;
mod recovery;
mod redaction;
#[cfg(feature = "execass-test-confirmation-runtime")]
mod reference_fixture_test_support;
mod routine_admission;
mod routines;
mod rows;
mod run_control_attestation;
mod runtime_host;
mod runtime_settings;
mod store;
mod transport;
mod types;
mod validation;
mod verifier;

#[cfg(test)]
mod lineage_tests;

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod jobs_tests;

#[cfg(test)]
mod claim_tests;

#[cfg(test)]
mod resource_tests;

#[cfg(test)]
mod receipt_integrity_tests;
#[cfg(test)]
mod runtime_host_tests;

#[cfg(test)]
mod receipt_tests;

#[cfg(test)]
mod active_work_tests;
#[cfg(test)]
mod api_read_tests;
#[cfg(test)]
mod decision_tests;
#[cfg(test)]
mod delivery_tests;

#[cfg(test)]
mod delegation_control_tests;

#[cfg(test)]
mod effect_tests;

#[cfg(test)]
mod global_stop_tests;

#[cfg(test)]
mod recorder_tests;

#[cfg(test)]
mod recovery_tests;

#[cfg(test)]
mod routines_tests;

#[cfg(test)]
mod routine_admission_tests;

#[cfg(test)]
mod verifier_tests;

#[cfg(test)]
mod completion_tests;

#[cfg(test)]
mod projection_tests;

#[cfg(test)]
mod policy_settings_tests;

#[cfg(test)]
mod transport_tests;

pub use channel_reply::{
    ChannelReplyBindingRecord, ChannelReplyBindingWriteOutcome, EligibleFollowUpTarget,
    ExecAssChannelProvider, NewChannelReplyBinding,
};
pub use claim::technical_resource_lifecycle_evidence_reference_digest;
pub use completion::{
    deterministic_completion_assessment_id, deterministic_completion_event_id,
    deterministic_terminal_correction_event_id, deterministic_terminal_correction_id,
};
pub use confirmation_attestation::{
    confirmation_attestation_signing_bytes, verify_confirmation_attestation,
    ConfirmationAttestation, ConfirmationAttestationPayload,
    ConfirmationAttestationVerificationError, PinnedConfirmationAttestationKey,
    VerifiedConfirmationAttestation,
};
#[cfg(any(test, feature = "execass-test-confirmation-runtime"))]
#[doc(hidden)]
pub use confirmation_custody::activate_test_confirmation_authority;
#[cfg(feature = "execass-test-confirmation-runtime")]
#[doc(hidden)]
pub use confirmation_custody::activate_test_confirmation_authority_for_os_user;
pub use confirmation_custody::ConfirmationAuthorityIdentity;
pub use jobs::{
    is_execass_continuation_job_payload, ContinuationJobBindingRecord,
    EXECASS_CONTINUATION_JOB_MODE,
};
pub use lifecycle::select_lifecycle_phase;
pub use receipt_integrity::{
    IntegrityRecovery, IntegrityStatus, ReceiptIntegrityStore, ReceiptKeyRef,
};
pub(crate) use recorder::register_recorder_evidence_sql_verifier;
#[cfg(test)]
pub(crate) use recorder::seed_signed_execution_unknown_fixture;
pub use recorder::{
    RecorderAuthorityIdentity, RecorderEvidenceVerificationError, VerifiedRecorderEvidence,
};
pub use redaction::{OpaqueSecretHandle, ReceiptRedactor, SafeJson, SafeText};
#[cfg(feature = "execass-test-confirmation-runtime")]
#[doc(hidden)]
pub use reference_fixture_test_support::{
    ReferenceFixtureRecoveryReport, ReferenceFixtureRoutineReport, ReferenceFixtureTerminalReport,
};
pub use routine_admission::{
    deterministic_routine_occurrence_action_id, RoutineOccurrenceDispatchOutcome,
};
pub use routines::{
    deterministic_routine_occurrence_id, execass_routine_driver_id,
    execass_routine_trigger_occurrence_id, is_execass_routine_driver_payload,
    is_execass_routine_trigger_payload, resolve_routine_local_time, select_catch_up_occurrences,
    EXECASS_ROUTINE_DRIVER_MODE, EXECASS_ROUTINE_TRIGGER_MODE,
};
pub use store::ExecAssStore;
pub use transport::{
    OutboxConsumerIdentity, OutboxDeliveryCommit, OutboxDeliveryCommitOutcome, OutboxGapReason,
    OutboxReplay, OutboxReplayOutcome,
};
pub use types::*;
pub use verifier::*;

#[cfg(test)]
mod tests;
