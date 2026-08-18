//! Typed adapter from ExecAss coordination to existing CarsinOS authorities.
//!
//! This facade only records immutable references and re-reads the referenced
//! source tables. It intentionally has no API for copying source status or for
//! changing Delegation phase/completion from a child, run, or job result.

use super::store::ExecAssStore;
use super::types::*;
use anyhow::Result;

impl ExecAssStore {
    /// Atomically observes existing authoritative work beneath one Delegation.
    ///
    /// Source records remain shareable where their authoritative subsystem
    /// permits it. Supplied parentage checks are re-read atomically (for
    /// example run-to-session), while replay, stale, and conflict outcomes
    /// perform no mutation.
    pub fn observe_orchestration(
        &self,
        command: &ObserveOrchestrationCommand,
    ) -> Result<OrchestrationObservationOutcome> {
        let lineage = AppendAuthorityLineageCommand {
            write: command.write.clone(),
            delegation_id: command.delegation_id.clone(),
            expected_state_revision: command.expected_state_revision,
            resulting_state_revision: command.resulting_state_revision,
            linked_at: command.observed_at,
            links: command.references.clone(),
            outbox_event: command.outbox_event.clone(),
        };
        match self.append_authority_lineage_with_ownership(&lineage, &command.ownership_checks) {
            Ok(AuthorityLineageOutcome::Appended(value)) => {
                Ok(OrchestrationObservationOutcome::Linked(value))
            }
            Ok(AuthorityLineageOutcome::Replayed(value)) => {
                Ok(OrchestrationObservationOutcome::Replayed(value))
            }
            Ok(AuthorityLineageOutcome::Stale {
                current_state_revision,
            }) => Ok(OrchestrationObservationOutcome::Stale {
                current_state_revision,
            }),
            Ok(AuthorityLineageOutcome::NotFound) => {
                Ok(OrchestrationObservationOutcome::MissingDelegation)
            }
            Ok(AuthorityLineageOutcome::Conflict { duplicate_identity }) => {
                Ok(OrchestrationObservationOutcome::Conflict { duplicate_identity })
            }
            Ok(AuthorityLineageOutcome::OwnershipMismatch {
                kind,
                source_id,
                expected_owner,
                actual_owner,
            }) => Ok(OrchestrationObservationOutcome::OwnershipMismatch {
                kind,
                source_id,
                expected_owner,
                actual_owner,
            }),
            Err(error) => {
                if let Some(error) = error.downcast_ref::<AuthorityLineageError>() {
                    return Ok(match error {
                        AuthorityLineageError::Unsupported(kind) => {
                            OrchestrationObservationOutcome::UnsupportedAuthority { kind: *kind }
                        }
                        AuthorityLineageError::MissingSource { kind, source_id } => {
                            OrchestrationObservationOutcome::MissingAuthority {
                                kind: *kind,
                                source_id: source_id.clone(),
                            }
                        }
                    });
                }
                Err(error)
            }
        }
    }

    /// Re-reads every linked source from its authoritative table. Mutable
    /// source fields are never returned or cached in ExecAss.
    pub fn reread_orchestration(&self, delegation_id: &str) -> Result<OrchestrationRereadOutcome> {
        if self.read_foundation(delegation_id)?.is_none() {
            return Ok(OrchestrationRereadOutcome::MissingDelegation);
        }
        if let Some(mismatch) = self.resolve_authority_parent_drift(delegation_id)? {
            return Ok(OrchestrationRereadOutcome::OwnershipMismatch {
                kind: mismatch.kind,
                source_id: mismatch.source_id,
                expected_owner: mismatch.expected_owner,
                actual_owner: mismatch.actual_owner,
            });
        }
        match self.resolve_authority_lineage(delegation_id) {
            Ok(links) => Ok(OrchestrationRereadOutcome::Current(links)),
            Err(error) => {
                if let Some(AuthorityLineageError::MissingSource { kind, source_id }) =
                    error.downcast_ref::<AuthorityLineageError>()
                {
                    return Ok(OrchestrationRereadOutcome::MissingAuthority {
                        kind: *kind,
                        source_id: source_id.clone(),
                    });
                }
                Err(error)
            }
        }
    }
}
