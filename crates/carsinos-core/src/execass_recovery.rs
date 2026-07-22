//! Objective, bounded recovery planning for ExecAss.
//!
//! This module deliberately has no free-text purpose, content, commerce,
//! morality, action-category, wording, or model-score input. A recovery
//! decision can depend only on the closed technical facts below.

use crate::execass_policy::RecoveryScope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderErrorClass {
    Transient,
    RateLimited,
    Authentication,
    Permanent,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RetrySafety {
    /// The provider boundary was never crossed.
    PreInvocationFailure,
    /// Repeating the exact logical effect is objectively idempotent.
    Idempotent,
    /// Independent evidence proves the prior effect is absent.
    IndependentlyProvenAbsent,
    /// The provider may have performed the effect. Automatic retry is forbidden.
    OutcomeUnknown,
    /// A non-idempotent invocation failed without independent absence proof.
    NonIdempotentFailure,
}

impl RetrySafety {
    const fn permits_same_effect_retry(self) -> bool {
        matches!(
            self,
            Self::PreInvocationFailure | Self::Idempotent | Self::IndependentlyProvenAbsent
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPolicy {
    pub max_attempts: u32,
    pub max_elapsed_ms: i64,
    pub base_backoff_ms: i64,
    pub max_backoff_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveRecoveryFacts {
    pub scope: RecoveryScope,
    pub attempts_started: u32,
    pub first_attempt_at_ms: i64,
    pub last_attempt_at_ms: i64,
    pub now_ms: i64,
    pub technical_resources_available: bool,
    pub circuit_open_until_ms: Option<i64>,
    pub provider_error_class: ProviderErrorClass,
    pub retry_safety: RetrySafety,
    pub operation_reversible: bool,
    pub declared_safe_boundary_reached: bool,
    pub replan_available_within_original_authority: bool,
    pub original_intent_and_authority_unchanged: bool,
    pub meaningful_outcome_exists: bool,
    pub user_judgment_useful: bool,
    pub external_progress_possible: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryDelayReason {
    Backoff,
    CircuitBreaker,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "directive", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryDirective {
    RetrySameEffect {
        not_before_ms: i64,
    },
    ReplanWithinOriginalAuthority,
    WaitUntil {
        not_before_ms: i64,
        reason: RecoveryDelayReason,
    },
    WaitingExternal,
    WaitingForUser,
    PartiallyCompleted,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPlanError {
    InvalidPolicy,
    InvalidClock,
    InvalidAttemptHistory,
}

impl std::fmt::Display for RecoveryPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPolicy => "objective recovery policy is invalid",
            Self::InvalidClock => "objective recovery clock is invalid",
            Self::InvalidAttemptHistory => "objective recovery attempt history is invalid",
        })
    }
}

impl std::error::Error for RecoveryPlanError {}

pub fn plan_objective_recovery(
    policy: RecoveryPolicy,
    facts: ObjectiveRecoveryFacts,
) -> Result<RecoveryDirective, RecoveryPlanError> {
    validate(policy, facts)?;

    if !facts.original_intent_and_authority_unchanged {
        return Ok(exhausted(facts));
    }

    if facts.retry_safety == RetrySafety::OutcomeUnknown {
        return Ok(if facts.external_progress_possible {
            RecoveryDirective::WaitingExternal
        } else {
            exhausted(facts)
        });
    }

    let autonomous_recovery_allowed = facts.scope != RecoveryScope::ManualOnly;
    if autonomous_recovery_allowed {
        if let Some(until) = facts
            .circuit_open_until_ms
            .filter(|until| *until > facts.now_ms)
        {
            return Ok(RecoveryDirective::WaitUntil {
                not_before_ms: until,
                reason: RecoveryDelayReason::CircuitBreaker,
            });
        }

        let retry_allowed = facts.retry_safety.permits_same_effect_retry()
            && !matches!(
                facts.provider_error_class,
                ProviderErrorClass::Authentication | ProviderErrorClass::Permanent
            );
        let within_attempt_limit = facts.attempts_started < policy.max_attempts;
        let elapsed = facts.now_ms - facts.first_attempt_at_ms;
        let within_elapsed_limit = elapsed <= policy.max_elapsed_ms;

        if retry_allowed
            && within_attempt_limit
            && within_elapsed_limit
            && facts.technical_resources_available
        {
            let not_before_ms = facts
                .last_attempt_at_ms
                .saturating_add(backoff_ms(policy, facts.attempts_started));
            if not_before_ms > facts.now_ms {
                return Ok(RecoveryDirective::WaitUntil {
                    not_before_ms,
                    reason: RecoveryDelayReason::Backoff,
                });
            }
            return Ok(RecoveryDirective::RetrySameEffect { not_before_ms });
        }

        if facts.replan_available_within_original_authority
            && facts.operation_reversible
            && facts.declared_safe_boundary_reached
        {
            return Ok(RecoveryDirective::ReplanWithinOriginalAuthority);
        }
    }

    Ok(exhausted(facts))
}

fn validate(
    policy: RecoveryPolicy,
    facts: ObjectiveRecoveryFacts,
) -> Result<(), RecoveryPlanError> {
    if policy.max_attempts == 0
        || policy.max_elapsed_ms < 0
        || policy.base_backoff_ms < 0
        || policy.max_backoff_ms < policy.base_backoff_ms
    {
        return Err(RecoveryPlanError::InvalidPolicy);
    }
    if facts.first_attempt_at_ms < 0
        || facts.last_attempt_at_ms < facts.first_attempt_at_ms
        || facts.now_ms < facts.last_attempt_at_ms
        || facts.circuit_open_until_ms.is_some_and(|until| until < 0)
    {
        return Err(RecoveryPlanError::InvalidClock);
    }
    if facts.attempts_started == 0 {
        return Err(RecoveryPlanError::InvalidAttemptHistory);
    }
    Ok(())
}

fn backoff_ms(policy: RecoveryPolicy, attempts_started: u32) -> i64 {
    let shift = attempts_started.saturating_sub(1).min(62);
    policy
        .base_backoff_ms
        .saturating_mul(1_i64.checked_shl(shift).unwrap_or(i64::MAX))
        .min(policy.max_backoff_ms)
}

const fn exhausted(facts: ObjectiveRecoveryFacts) -> RecoveryDirective {
    if facts.external_progress_possible {
        RecoveryDirective::WaitingExternal
    } else if facts.user_judgment_useful {
        RecoveryDirective::WaitingForUser
    } else if facts.meaningful_outcome_exists {
        RecoveryDirective::PartiallyCompleted
    } else {
        RecoveryDirective::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: RecoveryPolicy = RecoveryPolicy {
        max_attempts: 4,
        max_elapsed_ms: 60_000,
        base_backoff_ms: 1_000,
        max_backoff_ms: 8_000,
    };

    fn facts() -> ObjectiveRecoveryFacts {
        ObjectiveRecoveryFacts {
            scope: RecoveryScope::ObjectiveRetryWithinOwnerEnvelope,
            attempts_started: 1,
            first_attempt_at_ms: 1_000,
            last_attempt_at_ms: 1_000,
            now_ms: 2_000,
            technical_resources_available: true,
            circuit_open_until_ms: None,
            provider_error_class: ProviderErrorClass::Transient,
            retry_safety: RetrySafety::PreInvocationFailure,
            operation_reversible: true,
            declared_safe_boundary_reached: true,
            replan_available_within_original_authority: true,
            original_intent_and_authority_unchanged: true,
            meaningful_outcome_exists: false,
            user_judgment_useful: true,
            external_progress_possible: false,
        }
    }

    #[test]
    fn safe_retry_obeys_attempt_elapsed_backoff_resource_and_breaker_bounds() {
        assert_eq!(
            plan_objective_recovery(POLICY, facts()).unwrap(),
            RecoveryDirective::RetrySameEffect {
                not_before_ms: 2_000
            }
        );

        let mut delayed = facts();
        delayed.attempts_started = 3;
        delayed.last_attempt_at_ms = 1_900;
        assert_eq!(
            plan_objective_recovery(POLICY, delayed).unwrap(),
            RecoveryDirective::WaitUntil {
                not_before_ms: 5_900,
                reason: RecoveryDelayReason::Backoff,
            }
        );

        let mut breaker = facts();
        breaker.circuit_open_until_ms = Some(9_000);
        assert_eq!(
            plan_objective_recovery(POLICY, breaker).unwrap(),
            RecoveryDirective::WaitUntil {
                not_before_ms: 9_000,
                reason: RecoveryDelayReason::CircuitBreaker,
            }
        );

        for mut exhausted_facts in [facts(), facts(), facts()] {
            exhausted_facts.user_judgment_useful = false;
            exhausted_facts.meaningful_outcome_exists = true;
            match exhausted_facts.attempts_started {
                1 => exhausted_facts.attempts_started = POLICY.max_attempts,
                _ => unreachable!(),
            }
            assert_eq!(
                plan_objective_recovery(POLICY, exhausted_facts).unwrap(),
                RecoveryDirective::ReplanWithinOriginalAuthority
            );
        }

        let mut no_resource = facts();
        no_resource.technical_resources_available = false;
        no_resource.replan_available_within_original_authority = false;
        assert_eq!(
            plan_objective_recovery(POLICY, no_resource).unwrap(),
            RecoveryDirective::WaitingForUser
        );
    }

    #[test]
    fn unknown_never_retries_and_independent_absence_or_idempotency_can() {
        let mut unknown = facts();
        unknown.retry_safety = RetrySafety::OutcomeUnknown;
        assert_eq!(
            plan_objective_recovery(POLICY, unknown).unwrap(),
            RecoveryDirective::WaitingForUser
        );
        unknown.external_progress_possible = true;
        assert_eq!(
            plan_objective_recovery(POLICY, unknown).unwrap(),
            RecoveryDirective::WaitingExternal
        );

        for retry_safety in [
            RetrySafety::Idempotent,
            RetrySafety::IndependentlyProvenAbsent,
        ] {
            let mut retry = facts();
            retry.retry_safety = retry_safety;
            assert!(matches!(
                plan_objective_recovery(POLICY, retry).unwrap(),
                RecoveryDirective::RetrySameEffect { .. }
            ));
        }
    }

    #[test]
    fn permanent_or_unsafe_failure_replans_only_inside_unchanged_authority() {
        for error in [
            ProviderErrorClass::Authentication,
            ProviderErrorClass::Permanent,
        ] {
            let mut candidate = facts();
            candidate.provider_error_class = error;
            assert_eq!(
                plan_objective_recovery(POLICY, candidate).unwrap(),
                RecoveryDirective::ReplanWithinOriginalAuthority
            );
        }

        let mut drifted = facts();
        drifted.original_intent_and_authority_unchanged = false;
        assert_eq!(
            plan_objective_recovery(POLICY, drifted).unwrap(),
            RecoveryDirective::WaitingForUser
        );
    }

    #[test]
    fn exhaustion_projection_is_honest_and_prefers_autonomous_paths() {
        let mut exhausted_facts = facts();
        exhausted_facts.scope = RecoveryScope::ManualOnly;
        exhausted_facts.replan_available_within_original_authority = false;
        assert_eq!(
            plan_objective_recovery(POLICY, exhausted_facts).unwrap(),
            RecoveryDirective::WaitingForUser
        );
        exhausted_facts.user_judgment_useful = false;
        exhausted_facts.external_progress_possible = true;
        assert_eq!(
            plan_objective_recovery(POLICY, exhausted_facts).unwrap(),
            RecoveryDirective::WaitingExternal
        );
        exhausted_facts.external_progress_possible = false;
        exhausted_facts.meaningful_outcome_exists = true;
        assert_eq!(
            plan_objective_recovery(POLICY, exhausted_facts).unwrap(),
            RecoveryDirective::PartiallyCompleted
        );
        exhausted_facts.meaningful_outcome_exists = false;
        assert_eq!(
            plan_objective_recovery(POLICY, exhausted_facts).unwrap(),
            RecoveryDirective::Failed
        );
    }

    #[test]
    fn invalid_policy_clock_and_history_fail_closed() {
        let mut invalid_policy = POLICY;
        invalid_policy.max_attempts = 0;
        assert_eq!(
            plan_objective_recovery(invalid_policy, facts()),
            Err(RecoveryPlanError::InvalidPolicy)
        );
        let mut invalid_clock = facts();
        invalid_clock.now_ms = invalid_clock.last_attempt_at_ms - 1;
        assert_eq!(
            plan_objective_recovery(POLICY, invalid_clock),
            Err(RecoveryPlanError::InvalidClock)
        );
        let mut no_attempt = facts();
        no_attempt.attempts_started = 0;
        assert_eq!(
            plan_objective_recovery(POLICY, no_attempt),
            Err(RecoveryPlanError::InvalidAttemptHistory)
        );
    }

    #[test]
    fn serialized_facts_reject_any_unrecognized_policy_field() {
        let mut value = serde_json::to_value(facts()).unwrap();
        value.as_object_mut().unwrap().insert(
            "purpose_or_model_risk_score".to_owned(),
            serde_json::json!("forbidden"),
        );
        assert!(serde_json::from_value::<ObjectiveRecoveryFacts>(value).is_err());
    }
}
