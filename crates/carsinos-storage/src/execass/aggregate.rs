#![cfg_attr(not(test), allow(dead_code))]

use super::rows::{get_delegation, get_outbox, insert_outbox};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{validate_cas_command, verify_cas_result};
use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};

impl ExecAssStore {
    /// Applies a policy-neutral expected-revision mutation and emits its event.
    ///
    /// This primitive enforces atomicity and revision fencing only. EA-109 owns
    /// the allowed lifecycle/run-control transition graph.
    pub(super) fn compare_and_swap_delegation_state(
        &self,
        command: &CasDelegationStateCommand,
    ) -> Result<CasDelegationStateOutcome> {
        validate_cas_command(command)?;
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(current) = get_delegation(&tx, &command.delegation_id)? else {
            return Ok(CasDelegationStateOutcome::NotFound);
        };
        if current.state_revision != command.expected_state_revision {
            return Ok(CasDelegationStateOutcome::Stale {
                current_state_revision: current.state_revision,
            });
        }

        let changed = tx
            .execute(
                r#"
                UPDATE execass_delegations
                SET phase = ?1,
                    run_control = ?2,
                    state_revision = ?3,
                    pending_decision_id = ?4,
                    external_wait_json = ?5,
                    updated_at = ?6,
                    terminal_at = ?7
                WHERE delegation_id = ?8 AND state_revision = ?9
                "#,
                params![
                    command.phase.as_str(),
                    command.run_control.as_str(),
                    command.new_state_revision,
                    command.pending_decision_id,
                    command.external_wait_json,
                    command.updated_at,
                    command.terminal_at,
                    command.delegation_id,
                    command.expected_state_revision,
                ],
            )
            .context("failed applying ExecAss delegation CAS")?;
        if changed != 1 {
            let current_revision = tx
                .query_row(
                    "SELECT state_revision FROM execass_delegations WHERE delegation_id = ?1",
                    params![command.delegation_id],
                    |row| row.get(0),
                )
                .optional()?
                .context("ExecAss delegation disappeared during IMMEDIATE CAS")?;
            return Ok(CasDelegationStateOutcome::Stale {
                current_state_revision: current_revision,
            });
        }

        insert_outbox(&tx, &command.outbox_event)?;
        let delegation = get_delegation(&tx, &command.delegation_id)?
            .context("updated ExecAss delegation could not be reloaded")?;
        verify_cas_result(command, &delegation)?;
        let outbox_event = get_outbox(&tx, &command.outbox_event.event_id)?
            .context("CAS outbox event could not be reloaded")?;
        if outbox_event.event != command.outbox_event {
            anyhow::bail!("CAS outbox event verification did not match requested event");
        }
        tx.commit()
            .context("failed committing ExecAss delegation CAS")?;
        Ok(CasDelegationStateOutcome::Updated(Box::new(
            CasDelegationStateUpdated {
                delegation,
                outbox_event,
            },
        )))
    }
}
